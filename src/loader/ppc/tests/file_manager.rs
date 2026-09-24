use super::*;

    #[test]
    fn hrename_moves_both_forks_and_open_paths_without_changing_directory() {
        let pef = synthetic_pef_with_import(b"HRename");
        let mut loaded = load_pef_application(&pef).unwrap();
        let old_ptr = PPC_DATA_BASE + 0x1000;
        let new_ptr = old_ptr + 0x40;
        loaded.memory.add_region(old_ptr, vec![0; 0x100]);
        write_ppc_pstring(&mut loaded.memory, old_ptr, b"Old Log");
        write_ppc_pstring(&mut loaded.memory, new_ptr, b"New Log");
        let old_path = "System Folder/Preferences/Old Log";
        let new_path = "System Folder/Preferences/New Log";
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: old_path.to_string(),
            data: b"data fork".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: old_path.to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            resource_len: 13,
            raw_data: Some(b"resource fork".to_vec().into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.push_test_open_file(PpcFileRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: old_path.to_string(),
            position: 3,
        });
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = old_ptr;
        loaded.cpu.gpr[6] = new_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_files[0].path, new_path);
        assert_eq!(loaded.vfs_resource_files[0].path, new_path);
        assert_eq!(loaded.files[0].path, new_path);
        assert_eq!(loaded.files[0].position, 3);
        assert_eq!(loaded.take_deleted_vfs_file_paths(), vec![old_path]);
        assert_eq!(loaded.take_dirty_vfs_files()[0].data, b"data fork");
        assert_eq!(
            loaded.take_dirty_vfs_resource_forks()[0].data,
            b"resource fork"
        );
    }

    #[test]
    fn hrename_rejects_missing_and_duplicate_names() {
        let pef = synthetic_pef_with_import(b"HRename");
        let mut loaded = load_pef_application(&pef).unwrap();
        let old_ptr = PPC_DATA_BASE + 0x1000;
        let new_ptr = old_ptr + 0x40;
        loaded.memory.add_region(old_ptr, vec![0; 0x100]);
        write_ppc_pstring(&mut loaded.memory, old_ptr, b"Absent");
        write_ppc_pstring(&mut loaded.memory, new_ptr, b"Other");
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_ROOT_DIR_ID;
        loaded.cpu.gpr[5] = old_ptr;
        loaded.cpu.gpr[6] = new_ptr;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));

        for name in ["Source", "Other"] {
            loaded.push_test_vfs_file(PpcVfsFileRecord {
                path: name.to_string(),
                data: Vec::new().into(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                dirty: false,
            });
        }
        write_ppc_pstring(&mut loaded.memory, old_ptr, b"Source");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_DUP_FN_ERR));
        assert!(loaded.vfs_files.iter().all(|file| !file.dirty));
    }

    #[test]
    fn hrename_directory_keeps_child_ids_and_rewrites_descendant_paths() {
        let pef = synthetic_pef_with_import(b"HRename");
        let mut loaded = load_pef_application(&pef).unwrap();
        let old_ptr = PPC_DATA_BASE + 0x1000;
        let new_ptr = old_ptr + 0x40;
        loaded.memory.add_region(old_ptr, vec![0; 0x100]);
        write_ppc_pstring(&mut loaded.memory, old_ptr, b"Hangar");
        write_ppc_pstring(&mut loaded.memory, new_ptr, b"Airfield");
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: 1000,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Hangar".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: 1001,
            parent_dir_id: 1000,
            path: "Hangar/Logs".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, PPC_ROOT_DIR_ID, 1002);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Hangar/Logs/Pilot".to_string(),
            data: b"flight".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_ROOT_DIR_ID;
        loaded.cpu.gpr[5] = old_ptr;
        loaded.cpu.gpr[6] = new_ptr;

        loaded.run_with_hle_imports(64);

        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_directories[3].dir_id, 1000);
        assert_eq!(loaded.vfs_directories[3].path, "Airfield");
        assert_eq!(loaded.vfs_directories[4].dir_id, 1001);
        assert_eq!(loaded.vfs_directories[4].path, "Airfield/Logs");
        assert_eq!(loaded.vfs_files[0].path, "Airfield/Logs/Pilot");
        assert_eq!(
            loaded.take_deleted_vfs_file_paths(),
            vec!["Hangar/Logs/Pilot"]
        );
    }

    #[test]
    fn hle_import_runner_handles_find_folder_preferences_outputs() {
        let pef = synthetic_pef_with_import(b"FindFolder");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = 0xffff_8000; // kOnSystemDisk
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[5] = 1; // createFolder = true
        loaded.cpu.gpr[6] = PPC_DATA_BASE;
        loaded.cpu.gpr[7] = PPC_DATA_BASE + 4;
        loaded.memory.write_u16_be(PPC_DATA_BASE, 0xbeef).unwrap();
        loaded
            .memory
            .write_u32_be(PPC_DATA_BASE + 4, 0xdead_beef)
            .unwrap();

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(PPC_DATA_BASE),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_DATA_BASE + 4),
            Some(PPC_PREFERENCES_DIR_ID)
        );
    }

    #[test]
    fn hle_import_runner_returns_param_err_for_unmapped_find_folder_outputs() {
        let pef = synthetic_pef_with_import(b"FindFolder");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }

    #[test]
    fn hle_import_runner_find_folder_outputs_are_all_or_nothing() {
        let pef = synthetic_pef_with_import(b"FindFolder");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0x44; 4]);
        loaded.cpu.gpr[3] = 0xffff_8000;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = scratch;
        loaded.cpu.gpr[7] = scratch + 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        for offset in 0..4 {
            assert_eq!(loaded.memory.read_u8(scratch + offset), Some(0x44));
        }
    }

    fn classic_alias_record_with_full_path(path: &[u8]) -> Vec<u8> {
        assert!(path.len() <= usize::from(u16::MAX));
        let padded_len = (path.len() + 1) & !1;
        let record_size = PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE + 4 + padded_len + 4;
        assert!(record_size <= usize::from(u16::MAX));

        let mut bytes = vec![0; record_size];
        bytes[4..6].copy_from_slice(&(record_size as u16).to_be_bytes());
        bytes[6..8].copy_from_slice(&PPC_CLASSIC_ALIAS_RECORD_VERSION.to_be_bytes());
        bytes[PPC_CLASSIC_ALIAS_DIR_ID_OFFSET..PPC_CLASSIC_ALIAS_DIR_ID_OFFSET + 4]
            .copy_from_slice(&u32::MAX.to_be_bytes());

        let tag_offset = PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE;
        bytes[tag_offset..tag_offset + 2]
            .copy_from_slice(&PPC_CLASSIC_ALIAS_FULL_PATH_TAG.to_be_bytes());
        bytes[tag_offset + 2..tag_offset + 4].copy_from_slice(&(path.len() as u16).to_be_bytes());
        bytes[tag_offset + 4..tag_offset + 4 + path.len()].copy_from_slice(path);

        let end_offset = tag_offset + 4 + padded_len;
        bytes[end_offset..end_offset + 2].copy_from_slice(&PPC_CLASSIC_ALIAS_END_TAG.to_be_bytes());
        bytes
    }

    #[test]
    fn hle_import_runner_new_alias_allocates_classic_alias_record_handle() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let target_ptr = scratch;
        let alias_out_ptr = scratch + 80;
        let heap_cursor = loaded.heap_cursor();
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded
            .memory
            .write_u32_be(alias_out_ptr, 0xdead_beef)
            .unwrap();
        loaded.cpu.gpr[3] = 0; // fromFile = nil
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        let alias_handle = loaded.memory.read_u32_be(alias_out_ptr).unwrap();
        assert_ne!(alias_handle, 0);
        let expected_heap_advance = ppc_allocation_size(PPC_CLASSIC_ALIAS_RECORD_SIZE as u32)
            .unwrap()
            + ppc_allocation_size(4).unwrap();
        assert_eq!(loaded.heap_cursor(), heap_cursor + expected_heap_advance);
        assert_eq!(test_handle_records!(loaded).len(), 1);
        assert_eq!(test_handle_records!(loaded)[0].handle, alias_handle);
        assert_eq!(
            test_handle_records!(loaded)[0].size,
            PPC_CLASSIC_ALIAS_RECORD_SIZE as u32
        );
        assert_eq!(loaded.aliases.len(), 1);
        assert_eq!(loaded.aliases[0].handle, alias_handle);
        assert_eq!(loaded.aliases[0].target_vref, PPC_BOOT_VOLUME_REF_NUM);
        assert_eq!(loaded.aliases[0].target_dir_id, PPC_PREFERENCES_DIR_ID);
        assert_eq!(loaded.aliases[0].target_name.as_slice(), b"Test App Prefs");
        let alias_data_ptr = loaded.memory.read_u32_be(alias_handle).unwrap();
        assert_eq!(
            loaded.memory.read_u16_be(alias_data_ptr + 4),
            Some(PPC_CLASSIC_ALIAS_RECORD_SIZE as u16)
        );
        assert_eq!(
            loaded.memory.read_u16_be(alias_data_ptr + 6),
            Some(PPC_CLASSIC_ALIAS_RECORD_VERSION)
        );
        assert_eq!(loaded.memory.read_u32_be(alias_data_ptr), Some(0));
        assert_eq!(
            loaded
                .memory
                .read_u32_be(alias_data_ptr + PPC_CLASSIC_ALIAS_DIR_ID_OFFSET as u32),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(
            ppc_read_fixed_pstring_bytes(
                &mut loaded.memory,
                alias_data_ptr + PPC_CLASSIC_ALIAS_FILE_NAME_OFFSET as u32,
                PPC_FSSPEC_MAX_NAME_LEN
            )
            .as_deref(),
            Some(b"Test App Prefs".as_slice())
        );
        assert_eq!(
            loaded
                .memory
                .read_u16_be(alias_data_ptr + PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE as u32),
            Some(PPC_CLASSIC_ALIAS_TAIL_TAG)
        );
    }

    #[test]
    fn hle_import_runner_resolve_alias_returns_tracked_target_fsspec() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let target_ptr = scratch;
        let alias_out_ptr = scratch + 80;
        let resolved_ptr = scratch + 120;
        let was_changed_ptr = scratch + 200;
        loaded.memory.add_region(scratch, vec![0xaa; 256]);
        write_ppc_fsspec(
            &mut loaded.memory,
            target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let alias_handle = loaded.memory.read_u32_be(alias_out_ptr).unwrap();
        assert_ne!(alias_handle, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ResolveAlias;
        loaded.cpu.gpr[3] = 0; // fromFile = nil
        loaded.cpu.gpr[4] = alias_handle;
        loaded.cpu.gpr[5] = resolved_ptr;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(resolved_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(resolved_ptr + 2),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, resolved_ptr + 6).as_deref(),
            Some(b"Test App Prefs".as_slice())
        );
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(0));
    }

    #[test]
    fn hle_import_runner_resolve_alias_decodes_classic_alias_record() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let target_ptr = scratch;
        let alias_out_ptr = scratch + 80;
        let resolved_ptr = scratch + 120;
        let was_changed_ptr = scratch + 200;
        loaded.memory.add_region(scratch, vec![0xaa; 256]);
        write_ppc_fsspec(
            &mut loaded.memory,
            target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let alias_handle = loaded.memory.read_u32_be(alias_out_ptr).unwrap();
        assert_ne!(alias_handle, 0);
        loaded.aliases.clear();

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ResolveAlias;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = alias_handle;
        loaded.cpu.gpr[5] = resolved_ptr;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(resolved_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(resolved_ptr + 2),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, resolved_ptr + 6).as_deref(),
            Some(b"Test App Prefs".as_slice())
        );
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(0));
    }

    #[test]
    fn hle_import_runner_resolve_alias_decodes_full_path_tag_with_vfs_directory() {
        let pef = synthetic_pef_with_import(b"ResolveAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let app_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let data_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: app_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Game Folder".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: data_dir_id,
            parent_dir_id: app_dir_id,
            path: "Game Folder/Data".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, app_dir_id, data_dir_id + 1);
        let scratch = PPC_HEAP_BASE + 0x1000;
        let resolved_ptr = scratch;
        let was_changed_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0xaa; 128]);
        let alias_data =
            classic_alias_record_with_full_path(b"Systemless:Game Folder:Data:Level 1");
        let alias_handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &alias_data,
        );
        assert_ne!(alias_handle, 0);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = alias_handle;
        loaded.cpu.gpr[5] = resolved_ptr;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(resolved_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(resolved_ptr + 2),
            Some(data_dir_id)
        );
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, resolved_ptr + 6).as_deref(),
            Some(b"Level 1".as_slice())
        );
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(0));
    }

    #[test]
    fn hle_import_runner_resolve_alias_decodes_legacy_self_describing_alias_record() {
        let pef = synthetic_pef_with_import(b"ResolveAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let resolved_ptr = scratch;
        let was_changed_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0xaa; 128]);
        let alias_data = ppc_legacy_alias_record_bytes(
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        )
        .unwrap();
        let alias_handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &alias_data,
        );
        assert_ne!(alias_handle, 0);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = alias_handle;
        loaded.cpu.gpr[5] = resolved_ptr;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(resolved_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(resolved_ptr + 2),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, resolved_ptr + 6).as_deref(),
            Some(b"Test App Prefs".as_slice())
        );
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(0));
    }

    #[test]
    fn hle_import_runner_resolve_alias_unknown_handle_returns_param_err() {
        let pef = synthetic_pef_with_import(b"ResolveAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let resolved_ptr = scratch;
        let was_changed_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0xaa; 128]);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0x0bad_cafe;
        loaded.cpu.gpr[5] = resolved_ptr;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(0xaa));
    }

    #[test]
    fn hle_import_runner_update_alias_rewrites_alias_record_and_side_metadata() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let target_ptr = scratch;
        let updated_target_ptr = scratch + 80;
        let alias_out_ptr = scratch + 160;
        let was_changed_ptr = scratch + 200;
        let resolved_ptr = scratch + 220;
        loaded.memory.add_region(scratch, vec![0xaa; 512]);
        write_ppc_fsspec(
            &mut loaded.memory,
            target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        write_ppc_fsspec(
            &mut loaded.memory,
            updated_target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App HighScores",
        );
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let alias_handle = loaded.memory.read_u32_be(alias_out_ptr).unwrap();
        assert_ne!(alias_handle, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateAlias;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = updated_target_ptr;
        loaded.cpu.gpr[5] = alias_handle;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(1));
        assert_eq!(loaded.aliases.len(), 1);
        assert_eq!(loaded.aliases[0].target_vref, PPC_BOOT_VOLUME_REF_NUM);
        assert_eq!(loaded.aliases[0].target_dir_id, PPC_PREFERENCES_DIR_ID);
        assert_eq!(
            loaded.aliases[0].target_name.as_slice(),
            b"Test App HighScores"
        );
        let decoded = ppc_alias_record_from_handle(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            alias_handle,
            None,
        )
        .expect("updated handle should contain a classic AliasRecord");
        assert_eq!(decoded.target_vref, PPC_BOOT_VOLUME_REF_NUM);
        assert_eq!(decoded.target_dir_id, PPC_PREFERENCES_DIR_ID);
        assert_eq!(decoded.target_name.as_slice(), b"Test App HighScores");

        loaded.aliases.clear();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ResolveAlias;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = alias_handle;
        loaded.cpu.gpr[5] = resolved_ptr;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, resolved_ptr + 6).as_deref(),
            Some(b"Test App HighScores".as_slice())
        );
    }

    #[test]
    fn hle_import_runner_update_alias_same_target_reports_unchanged() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let target_ptr = scratch;
        let alias_out_ptr = scratch + 80;
        let was_changed_ptr = scratch + 120;
        loaded.memory.add_region(scratch, vec![0xaa; 256]);
        write_ppc_fsspec(
            &mut loaded.memory,
            target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let alias_handle = loaded.memory.read_u32_be(alias_out_ptr).unwrap();
        let heap_cursor = loaded.heap_cursor();
        loaded.memory.write_u8(was_changed_ptr, 0xff).unwrap();

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateAlias;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_handle;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(0));
        assert_eq!(loaded.heap_cursor(), heap_cursor);
        assert_eq!(
            test_handle_records!(loaded)[0].size,
            PPC_CLASSIC_ALIAS_RECORD_SIZE as u32
        );
        assert_eq!(loaded.aliases[0].target_name.as_slice(), b"Test App Prefs");
    }

    #[test]
    fn hle_import_runner_update_alias_prevalidates_was_changed_output() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let target_ptr = scratch;
        let updated_target_ptr = scratch + 80;
        let alias_out_ptr = scratch + 160;
        loaded.memory.add_region(scratch, vec![0xaa; 256]);
        write_ppc_fsspec(
            &mut loaded.memory,
            target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        write_ppc_fsspec(
            &mut loaded.memory,
            updated_target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App HighScores",
        );
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let alias_handle = loaded.memory.read_u32_be(alias_out_ptr).unwrap();

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateAlias;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = updated_target_ptr;
        loaded.cpu.gpr[5] = alias_handle;
        loaded.cpu.gpr[6] = 0x06ff_0000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.aliases[0].target_name.as_slice(), b"Test App Prefs");
        let decoded = ppc_alias_record_from_handle(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            alias_handle,
            None,
        )
        .expect("original handle should remain decodable");
        assert_eq!(decoded.target_name.as_slice(), b"Test App Prefs");
    }

    #[test]
    fn hle_import_runner_update_alias_small_legacy_record_returns_mem_full() {
        let pef = synthetic_pef_with_import(b"UpdateAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let target_ptr = scratch;
        let was_changed_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0xaa; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            target_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App HighScores",
        );
        let alias_data = ppc_legacy_alias_record_bytes(
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        )
        .unwrap();
        let alias_handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &alias_data,
        );
        loaded.memory.write_u8(was_changed_ptr, 0xff).unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = target_ptr;
        loaded.cpu.gpr[5] = alias_handle;
        loaded.cpu.gpr[6] = was_changed_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
        assert_eq!(loaded.memory.read_u8(was_changed_ptr), Some(0xff));
        assert_eq!(
            test_handle_records!(loaded)[0].size,
            PPC_ALIAS_RECORD_SIZE as u32
        );
        let decoded = ppc_alias_record_from_handle(
            &mut loaded.memory,
            &test_handle_records!(loaded),
            alias_handle,
            None,
        )
        .expect("legacy handle should remain unchanged");
        assert_eq!(decoded.target_name.as_slice(), b"Test App Prefs");
    }

    #[test]
    fn hle_import_runner_new_alias_null_output_returns_param_err() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let heap_cursor = loaded.heap_cursor();
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.heap_cursor(), heap_cursor);
        assert!(test_handle_records!(loaded).is_empty());
        assert!(loaded.aliases.is_empty());
    }

    #[test]
    fn hle_import_runner_new_alias_unmapped_output_returns_param_err() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let heap_cursor = loaded.heap_cursor();
        loaded.memory.add_region(scratch, vec![0; PPC_FSSPEC_SIZE]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = 0x06ff_0000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.heap_cursor(), heap_cursor);
        assert!(test_handle_records!(loaded).is_empty());
        assert!(loaded.aliases.is_empty());
    }

    #[test]
    fn hle_import_runner_new_alias_heap_full_returns_mem_full() {
        let pef = synthetic_pef_with_import(b"NewAlias");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE + 0x1000;
        let alias_out_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded
            .memory
            .write_u32_be(alias_out_ptr, 0xdead_beef)
            .unwrap();
        loaded.set_heap_limit(loaded.heap_cursor() + 16);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = alias_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
        assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
        assert_eq!(loaded.memory.read_u32_be(alias_out_ptr), Some(0xdead_beef));
        assert!(test_handle_records!(loaded).is_empty());
        assert!(loaded.aliases.is_empty());
    }

    #[test]
    fn hle_import_runner_resolve_alias_file_non_alias_file_returns_false_flags() {
        let pef = synthetic_pef_with_import(b"ResolveAliasFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        let target_is_folder_ptr = scratch + 80;
        let was_aliased_ptr = scratch + 81;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (Vec::new()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.memory.write_u8(target_is_folder_ptr, 0xff).unwrap();
        loaded.memory.write_u8(was_aliased_ptr, 0xff).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1; // resolveAliasChains
        loaded.cpu.gpr[5] = target_is_folder_ptr;
        loaded.cpu.gpr[6] = was_aliased_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(target_is_folder_ptr), Some(0));
        assert_eq!(loaded.memory.read_u8(was_aliased_ptr), Some(0));
        assert_eq!(
            loaded.memory.read_u32_be(scratch + 2),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(loaded.memory.read_u8(scratch + 6), Some(14));
    }

    #[test]
    fn hle_import_runner_resolve_alias_file_resolves_alis_resource_target() {
        let pef = synthetic_pef_with_import(b"ResolveAliasFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        let target_is_folder_ptr = scratch + 80;
        let was_aliased_ptr = scratch + 81;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_ROOT_DIR_ID,
            b"Prefs Alias",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Prefs Alias".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"alis"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Prefs Alias".to_string(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"alis"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Prefs Alias".to_string(),
            res_type: u32::from_be_bytes(*b"alis"),
            res_id: 0,
            name: Vec::new(),
            data: ppc_alias_record_bytes(
                PPC_BOOT_VOLUME_REF_NUM,
                PPC_PREFERENCES_DIR_ID,
                b"Test App Prefs",
            )
            .unwrap(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.memory.write_u8(target_is_folder_ptr, 0xff).unwrap();
        loaded.memory.write_u8(was_aliased_ptr, 0xff).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = target_is_folder_ptr;
        loaded.cpu.gpr[6] = was_aliased_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(target_is_folder_ptr), Some(0));
        assert_eq!(loaded.memory.read_u8(was_aliased_ptr), Some(1));
        assert_eq!(
            loaded.memory.read_u16_be(scratch),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(scratch + 2),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, scratch + 6).as_deref(),
            Some(b"Test App Prefs".as_slice())
        );
    }

    #[test]
    fn hle_import_runner_resolve_alias_file_decodes_full_path_tag_with_vfs_directory() {
        let pef = synthetic_pef_with_import(b"ResolveAliasFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let app_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let data_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: app_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Game Folder".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: data_dir_id,
            parent_dir_id: app_dir_id,
            path: "Game Folder/Data".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, app_dir_id, data_dir_id + 1);
        let scratch = PPC_HEAP_BASE;
        let target_is_folder_ptr = scratch + 80;
        let was_aliased_ptr = scratch + 81;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_ROOT_DIR_ID,
            b"Level Alias",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Level Alias".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"alis"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Level Alias".to_string(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"alis"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Level Alias".to_string(),
            res_type: u32::from_be_bytes(*b"alis"),
            res_id: 0,
            name: Vec::new(),
            data: classic_alias_record_with_full_path(b"Systemless:Game Folder:Data:Level 1"),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Folder/Data/Level 1".to_string(),
            data: (b"level".to_vec()).into(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"DATA"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.memory.write_u8(target_is_folder_ptr, 0xff).unwrap();
        loaded.memory.write_u8(was_aliased_ptr, 0xff).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = target_is_folder_ptr;
        loaded.cpu.gpr[6] = was_aliased_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(target_is_folder_ptr), Some(0));
        assert_eq!(loaded.memory.read_u8(was_aliased_ptr), Some(1));
        assert_eq!(
            loaded.memory.read_u16_be(scratch),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(loaded.memory.read_u32_be(scratch + 2), Some(data_dir_id));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, scratch + 6).as_deref(),
            Some(b"Level 1".as_slice())
        );
    }

    #[test]
    fn hle_import_runner_resolve_alias_file_prevalidates_outputs_before_rewriting_spec() {
        let pef = synthetic_pef_with_import(b"ResolveAliasFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        let target_is_folder_ptr = scratch + 80;
        let was_aliased_ptr = 0x06ff_0000;
        loaded.memory.add_region(scratch, vec![0xaa; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_ROOT_DIR_ID,
            b"Prefs Alias",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Prefs Alias".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"alis"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Prefs Alias".to_string(),
            creator: u32::from_be_bytes(*b"MACS"),
            file_type: u32::from_be_bytes(*b"alis"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Prefs Alias".to_string(),
            res_type: u32::from_be_bytes(*b"alis"),
            res_id: 0,
            name: Vec::new(),
            data: ppc_alias_record_bytes(
                PPC_BOOT_VOLUME_REF_NUM,
                PPC_PREFERENCES_DIR_ID,
                b"Test App Prefs",
            )
            .unwrap(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.memory.write_u8(target_is_folder_ptr, 0xff).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = target_is_folder_ptr;
        loaded.cpu.gpr[6] = was_aliased_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(target_is_folder_ptr), Some(0xff));
        assert_eq!(
            loaded.memory.read_u16_be(scratch),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(scratch + 2),
            Some(PPC_ROOT_DIR_ID)
        );
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, scratch + 6).as_deref(),
            Some(b"Prefs Alias".as_slice())
        );
    }

    #[test]
    fn hle_import_runner_resolve_alias_file_reports_directory_targets() {
        let pef = synthetic_pef_with_import(b"ResolveAliasFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        let target_is_folder_ptr = scratch + 80;
        let was_aliased_ptr = scratch + 81;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_SYSTEM_FOLDER_DIR_ID,
            b"Preferences",
        );
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = target_is_folder_ptr;
        loaded.cpu.gpr[6] = was_aliased_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(target_is_folder_ptr), Some(1));
        assert_eq!(loaded.memory.read_u8(was_aliased_ptr), Some(0));
    }

    #[test]
    fn hle_import_runner_resolve_alias_file_missing_target_returns_fnf() {
        let pef = synthetic_pef_with_import(b"ResolveAliasFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        let target_is_folder_ptr = scratch + 80;
        let was_aliased_ptr = scratch + 81;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Missing Prefs",
        );
        loaded.memory.write_u8(target_is_folder_ptr, 0xff).unwrap();
        loaded.memory.write_u8(was_aliased_ptr, 0xff).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = target_is_folder_ptr;
        loaded.cpu.gpr[6] = was_aliased_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.memory.read_u8(target_is_folder_ptr), Some(0xff));
        assert_eq!(loaded.memory.read_u8(was_aliased_ptr), Some(0xff));
    }

    #[test]
    fn hle_import_runner_handles_dir_create_under_preferences() {
        let pef = synthetic_pef_with_import(b"DirCreate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"Test App");
        let created_dir_id_ptr = scratch + 32;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = created_dir_id_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(created_dir_id_ptr),
            Some(PPC_FIRST_DYNAMIC_DIR_ID)
        );
        assert_eq!(loaded.next_vfs_dir_id, PPC_FIRST_DYNAMIC_DIR_ID + 1);
        assert_eq!(
            ppc_directory_path_for_id(&loaded.vfs_directories, PPC_FIRST_DYNAMIC_DIR_ID),
            Some("System Folder/Preferences/Test App")
        );
    }

    #[test]
    fn hle_import_runner_handles_fsp_dir_create() {
        let pef = synthetic_pef_with_import(b"FSpDirCreate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let spec_ptr = PPC_HEAP_BASE;
        let created_dir_id_ptr = spec_ptr + 80;
        loaded.memory.add_region(spec_ptr, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Gridz",
        );
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = created_dir_id_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(created_dir_id_ptr),
            Some(PPC_FIRST_DYNAMIC_DIR_ID)
        );
        assert_eq!(
            ppc_directory_path_for_id(&loaded.vfs_directories, PPC_FIRST_DYNAMIC_DIR_ID),
            Some("System Folder/Preferences/Gridz")
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = created_dir_id_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_DUP_FN_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(created_dir_id_ptr),
            Some(PPC_FIRST_DYNAMIC_DIR_ID)
        );
    }

    #[test]
    fn hle_import_runner_reports_duplicate_dir_create() {
        let pef = synthetic_pef_with_import(b"DirCreate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"Test App");
        let created_dir_id_ptr = scratch + 32;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = created_dir_id_ptr;
        let first = loaded.run_with_hle_imports(64);
        assert_eq!(first.unsupported_import_index, None);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = created_dir_id_ptr;
        let second = loaded.run_with_hle_imports(64);

        assert_eq!(second.handled_import_count, 1);
        assert_eq!(second.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_DUP_FN_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(created_dir_id_ptr),
            Some(PPC_FIRST_DYNAMIC_DIR_ID)
        );
    }

    #[test]
    fn hle_import_runner_reports_missing_parent_for_dir_create() {
        let pef = synthetic_pef_with_import(b"DirCreate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"Test App");
        loaded.cpu.gpr[4] = 999_999;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = scratch + 32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.next_vfs_dir_id, PPC_FIRST_DYNAMIC_DIR_ID);
    }

    #[test]
    fn hle_import_runner_makes_existing_directory_fsspec() {
        let pef = synthetic_pef_with_import(b"FSMakeFSSpec");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"Preferences");
        let spec_ptr = scratch + 32;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = PPC_SYSTEM_FOLDER_DIR_ID;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = spec_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(spec_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(spec_ptr + 2),
            Some(PPC_SYSTEM_FOLDER_DIR_ID)
        );
        assert_eq!(loaded.memory.read_u8(spec_ptr + 6), Some(11));
        for (offset, byte) in b"Preferences".iter().enumerate() {
            assert_eq!(
                loaded.memory.read_u8(spec_ptr + 7 + offset as u32),
                Some(*byte)
            );
        }
    }

    #[test]
    fn ppc_vfs_basename_bytes_returns_mac_roman_bytes() {
        let name_bytes = ppc_vfs_basename_bytes("Gridz Demo ƒ/Gridz™ Data");

        assert_eq!(
            name_bytes,
            [b'G', b'r', b'i', b'd', b'z', 0xAA, b' ', b'D', b'a', b't', b'a']
        );
        assert_eq!(decode_mac_roman(&name_bytes), "Gridz™ Data");
    }

    #[test]
    fn hle_import_runner_resolves_fsspec_relative_to_default_directory() {
        let pef = synthetic_pef_with_import(b"FSMakeFSSpec");
        let mut loaded = load_pef_application(&pef).unwrap();
        let app_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let data_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: app_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Game Folder".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: data_dir_id,
            parent_dir_id: app_dir_id,
            path: "Game Folder/Data".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, app_dir_id, data_dir_id + 1);
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"Data");
        let spec_ptr = scratch + 32;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = spec_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(spec_ptr + 2), Some(app_dir_id));
        assert_eq!(loaded.next_vfs_dir_id, data_dir_id + 1);
    }

    #[test]
    fn hle_import_runner_does_not_fallback_relative_fsspec_to_source_tree() {
        let pef = synthetic_pef_with_import(b"FSMakeFSSpec");
        let mut loaded = load_pef_application(&pef).unwrap();
        let app_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let source_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1;
        let source_data_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 2;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: app_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Gridz Demo".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: source_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Gridz_ CD".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: source_data_dir_id,
            parent_dir_id: source_dir_id,
            path: "Gridz_ CD/Gridz_ Data".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, app_dir_id, source_data_dir_id + 1);
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, scratch, b":Gridz_ Data:");
        let spec_ptr = scratch + 32;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = spec_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.memory.read_u32_be(spec_ptr + 2), Some(app_dir_id));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, spec_ptr + 6).as_deref(),
            Some(b":Gridz_ Data:".as_slice())
        );
    }

    #[test]
    fn hle_import_runner_make_fsspec_canonicalizes_unique_archive_suffix_path() {
        let pef = synthetic_pef_with_import(b"FSMakeFSSpec");
        let mut loaded = load_pef_application(&pef).unwrap();
        let data_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let audio_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: data_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Data".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: audio_dir_id,
            parent_dir_id: data_dir_id,
            path: "Data/Audio".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, PPC_ROOT_DIR_ID, audio_dir_id + 1);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Data/Audio/Song_Pangea".to_string(),
            data: (b"aiff".to_vec()).into(),
            creator: 0,
            file_type: u32::from_be_bytes(*b"AIFF"),
            finder_flags: 0,
            dirty: false,
        });
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, scratch, b":audio:song_pangea");
        let spec_ptr = scratch + 32;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_ROOT_DIR_ID;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = spec_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(spec_ptr + 2), Some(audio_dir_id));
        assert_eq!(loaded.memory.read_u8(spec_ptr + 6), Some(11));
        for (offset, byte) in b"Song_Pangea".iter().enumerate() {
            assert_eq!(
                loaded.memory.read_u8(spec_ptr + 7 + offset as u32),
                Some(*byte)
            );
        }
    }

    #[test]
    fn hle_import_runner_makes_missing_file_fsspec_and_returns_fnf() {
        let pef = synthetic_pef_with_import(b"FSMakeFSSpec");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"Test App Prefs");
        let spec_ptr = scratch + 32;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = spec_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(spec_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(spec_ptr + 2),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(loaded.memory.read_u8(spec_ptr + 6), Some(14));
        for (offset, byte) in b"Test App Prefs".iter().enumerate() {
            assert_eq!(
                loaded.memory.read_u8(spec_ptr + 7 + offset as u32),
                Some(*byte)
            );
        }
    }

    #[test]
    fn hle_import_runner_makes_fsspec_and_reports_missing_parent() {
        let pef = synthetic_pef_with_import(b"FSMakeFSSpec");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_HEAP_BASE;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"Test App Prefs");
        let spec_ptr = scratch + 32;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 999_999;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = spec_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_DIR_NF_ERR));
        assert_eq!(loaded.memory.read_u32_be(spec_ptr + 2), Some(999_999));
        assert_eq!(loaded.memory.read_u8(spec_ptr + 6), Some(14));
    }

    #[test]
    fn hle_import_runner_pb_get_finfo_uses_default_directory() {
        let pef = synthetic_pef_with_import(b"PBGetFInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded
            .default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = PPC_PREFERENCES_DIR_ID);
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Test App Prefs");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded.memory.write_u16_be(pb + 22, 0).unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded.memory.write_u32_be(pb + 100, 0xdead_beef).unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            dirty: false,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(
            loaded.memory.read_u16_be(pb + 22),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(loaded.memory.read_u8(pb + 30), Some(0));
        assert_eq!(
            loaded.memory.read_u32_be(pb + 32),
            Some(u32::from_be_bytes(*b"pref"))
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 36),
            Some(u32::from_be_bytes(*b"NanO"))
        );
        assert_eq!(loaded.memory.read_u16_be(pb + 40), Some(0x0200));
        assert_eq!(loaded.memory.read_u32_be(pb + 54), Some(5));
        assert_eq!(loaded.memory.read_u32_be(pb + 58), Some(5));
        assert_eq!(loaded.memory.read_u32_be(pb + 100), Some(0xdead_beef));
    }

    #[test]
    fn hle_import_runner_h_get_and_set_finfo_use_explicit_directory() {
        let pef = synthetic_pef_with_import(b"HGetFInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let name_ptr = scratch;
        let finfo_ptr = scratch + 0x100;
        loaded.memory.add_region(scratch, vec![0; 0x200]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Escape Velocity Prefs");
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Escape Velocity Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"EvlT"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0100,
            dirty: false,
        });
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = name_ptr;
        loaded.cpu.gpr[6] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            ppc_read_finfo(&mut loaded.memory, finfo_ptr),
            Some((
                u32::from_be_bytes(*b"pref"),
                u32::from_be_bytes(*b"EvlT"),
                0x0100,
            ))
        );

        ppc_write_finfo(
            &mut loaded.memory,
            finfo_ptr,
            u32::from_be_bytes(*b"SAVE"),
            u32::from_be_bytes(*b"EVLT"),
            0x4000,
        )
        .unwrap();
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HSetFInfo;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = name_ptr;
        loaded.cpu.gpr[6] = finfo_ptr;
        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_files[0].file_type, u32::from_be_bytes(*b"SAVE"));
        assert_eq!(loaded.vfs_files[0].creator, u32::from_be_bytes(*b"EVLT"));
        assert_eq!(loaded.vfs_files[0].finder_flags, 0x4000);
        assert!(loaded.vfs_files[0].dirty);
    }

    #[test]
    fn hle_import_runner_standard_get_file_returns_cancelled_reply() {
        let pef = synthetic_pef_with_import(b"StandardGetFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let reply = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(reply, vec![0xff; 88]);
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = PPC_DATA_BASE;
        loaded.cpu.gpr[6] = reply;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert!(loaded.toolbox_startup.standard_file_get_tracking.is_some());
        loaded.set_event_queue([PpcQueuedEvent {
            what: 3,
            message: u32::from(PPC_KEY_ESCAPE) << 8,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.memory.read_u8(reply), Some(0));
        assert_eq!(
            loaded.memory.read_u8(reply + 1),
            Some(0xff),
            "cancellation must preserve caller-owned reply tail"
        );
    }

    #[test]
    fn hle_import_runner_standard_get_file_keeps_empty_filtered_dialog_open() {
        let pef = synthetic_pef_with_import(b"StandardGetFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let reply = PPC_DATA_BASE + 0x1a00;
        let type_list = PPC_DATA_BASE + 0x1b00;
        loaded.memory.add_region(reply, vec![0xaa; 88]);
        loaded.memory.add_region(type_list, vec![0; 4]);
        loaded
            .memory
            .write_u32_be(type_list, u32::from_be_bytes(*b"NONE"))
            .unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = type_list;
        loaded.cpu.gpr[6] = reply;

        let probe = loaded.run_with_hle_imports(64);

        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert!(loaded.toolbox_startup.standard_file_get_tracking.is_some());
        assert_eq!(loaded.memory.read_u8(reply), Some(0xaa));
        loaded.set_event_queue([PpcQueuedEvent {
            what: 3,
            message: u32::from(PPC_KEY_ESCAPE) << 8,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.memory.read_u8(reply), Some(0));
    }

    #[test]
    fn hle_import_runner_standard_get_file_gui_navigates_and_filters_vfs_entries() {
        let pef = synthetic_pef_with_import(b"StandardGetFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let reply = PPC_DATA_BASE + 0x1000;
        let type_list = PPC_DATA_BASE + 0x1100;
        let fixtures_dir = 42;
        loaded.memory.add_region(reply, vec![0xaa; 88]);
        loaded.memory.add_region(type_list, vec![0; 4]);
        loaded
            .memory
            .write_u32_be(type_list, u32::from_be_bytes(*b"TEXT"))
            .unwrap();
        loaded.vfs_directories.push(PpcVfsDirectory {
            dir_id: fixtures_dir,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Standard File Fixtures".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Standard File Fixtures/Text Document".to_string(),
            data: (b"text".to_vec()).into(),
            creator: u32::from_be_bytes(*b"SHWC"),
            file_type: u32::from_be_bytes(*b"TEXT"),
            finder_flags: 0x4000,
            dirty: false,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Standard File Fixtures/Binary Data".to_string(),
            data: (b"data".to_vec()).into(),
            creator: u32::from_be_bytes(*b"SHWC"),
            file_type: u32::from_be_bytes(*b"DATA"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = type_list;
        loaded.cpu.gpr[6] = reply;

        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        let bounds = loaded
            .toolbox_startup
            .standard_file_get_tracking
            .as_ref()
            .expect("StandardGetFile must retain its dialog across the import yield")
            .bounds;
        loaded.set_event_queue([PpcQueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: bounds.0
                + (PPC_STANDARD_FILE_GET_OPEN_RECT.0 + PPC_STANDARD_FILE_GET_OPEN_RECT.2) / 2,
            where_h: bounds.1
                + (PPC_STANDARD_FILE_GET_OPEN_RECT.1 + PPC_STANDARD_FILE_GET_OPEN_RECT.3) / 2,
            modifiers: 0,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(
            loaded
                .toolbox_startup
                .standard_file_get_tracking
                .as_ref()
                .map(|tracking| tracking.entries.len()),
            Some(1),
            "TEXT filter must hide the DATA fixture after entering its folder"
        );
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
        assert!(loaded.toolbox_startup.standard_file_get_tracking.is_none());
        assert_eq!(loaded.memory.read_u8(reply), Some(1));
        assert_eq!(
            loaded.memory.read_u32_be(reply + 2),
            Some(u32::from_be_bytes(*b"TEXT"))
        );
        assert_eq!(
            loaded.memory.read_u16_be(reply + 6),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(loaded.memory.read_u32_be(reply + 8), Some(fixtures_dir));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, reply + 12),
            Some(b"Text Document".to_vec())
        );
    }

    #[test]
    fn hle_import_runner_sf_get_file_gui_cancel_uses_legacy_reply_abi() {
        let pef = synthetic_pef_with_import(b"SFGetFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let reply = PPC_DATA_BASE + 0x1200;
        let type_list = PPC_DATA_BASE + 0x1300;
        loaded.memory.add_region(reply, vec![0xaa; 75]);
        loaded.memory.add_region(type_list, vec![0; 4]);
        loaded
            .memory
            .write_u32_be(type_list, u32::from_be_bytes(*b"TEXT"))
            .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Text Document".to_string(),
            data: (b"text".to_vec()).into(),
            creator: u32::from_be_bytes(*b"SHWC"),
            file_type: u32::from_be_bytes(*b"TEXT"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = type_list;
        loaded.cpu.gpr[9] = reply;
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert!(loaded.toolbox_startup.standard_file_get_tracking.is_some());
        loaded.set_event_queue([PpcQueuedEvent {
            what: 3,
            message: u32::from(PPC_KEY_ESCAPE) << 8,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.memory.read_u8(reply), Some(0));
        assert_eq!(loaded.memory.read_u8(reply + 1), Some(0xaa));
    }

    #[test]
    fn hle_import_runner_sf_get_file_returns_resolvable_parent_working_directory() {
        let pef = synthetic_pef_with_import(b"SFGetFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let reply = PPC_DATA_BASE + 0x1800;
        let type_list = PPC_DATA_BASE + 0x1900;
        let fixtures_dir = 42;
        loaded.memory.add_region(reply, vec![0xaa; 75]);
        loaded.memory.add_region(type_list, vec![0; 4]);
        loaded
            .memory
            .write_u32_be(type_list, u32::from_be_bytes(*b"TEXT"))
            .unwrap();
        loaded.vfs_directories.push(PpcVfsDirectory {
            dir_id: fixtures_dir,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Standard File Fixtures".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Standard File Fixtures/Text Document".to_string(),
            data: (b"text".to_vec()).into(),
            creator: u32::from_be_bytes(*b"SHWC"),
            file_type: u32::from_be_bytes(*b"TEXT"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = type_list;
        loaded.cpu.gpr[9] = reply;

        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        let bounds = loaded
            .toolbox_startup
            .standard_file_get_tracking
            .as_ref()
            .expect("SFGetFile must retain its dialog across the import yield")
            .bounds;
        loaded.set_event_queue([PpcQueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: bounds.0
                + (PPC_STANDARD_FILE_GET_OPEN_RECT.0 + PPC_STANDARD_FILE_GET_OPEN_RECT.2) / 2,
            where_h: bounds.1
                + (PPC_STANDARD_FILE_GET_OPEN_RECT.1 + PPC_STANDARD_FILE_GET_OPEN_RECT.3) / 2,
            modifiers: 0,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
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
            loaded.memory.read_u16_be(reply + 6),
            Some(32),
            "legacy SFReply must return a working-directory reference"
        );
        let wd_ref = loaded.memory.read_u16_be(reply + 6).unwrap() as i16;
        assert_eq!(
            loaded.working_directories.get(&wd_ref),
            Some(&ProcessWorkingDirectory {
                ref_num: wd_ref,
                volume_ref_num: PPC_BOOT_VOLUME_REF_NUM,
                dir_id: fixtures_dir,
                proc_id: 0,
            })
        );
        assert_eq!(loaded.memory.read_u16_be(reply + 8), Some(0));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, reply + 10),
            Some(b"Text Document".to_vec())
        );
    }

    #[test]
    fn hle_import_runner_standard_put_file_gui_edits_name_and_accepts() {
        let pef = synthetic_pef_with_import(b"StandardPutFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let reply = PPC_DATA_BASE + 0x1400;
        let default_name = PPC_DATA_BASE + 0x1500;
        loaded.memory.add_region(reply, vec![0xaa; 88]);
        loaded.memory.add_region(default_name, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, default_name, b"Untitled");
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = default_name;
        loaded.cpu.gpr[5] = reply;
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert!(loaded.toolbox_startup.standard_file_put_tracking.is_some());
        loaded.set_event_queue([PpcQueuedEvent {
            what: 3,
            message: 0x0000_0061,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0x0100,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        loaded.set_event_queue([PpcQueuedEvent {
            what: 3,
            message: u32::from(b'S'),
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
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
        assert_eq!(loaded.memory.read_u8(reply + 1), Some(0));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, reply + 12),
            Some(b"S".to_vec())
        );
        assert!(loaded.toolbox_startup.standard_file_put_tracking.is_none());
    }

    #[test]
    fn hle_import_runner_sf_put_file_gui_cancel_uses_legacy_reply_abi() {
        let pef = synthetic_pef_with_import(b"SFPutFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let reply = PPC_DATA_BASE + 0x1600;
        let default_name = PPC_DATA_BASE + 0x1700;
        loaded.memory.add_region(reply, vec![0xaa; 75]);
        loaded.memory.add_region(default_name, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, default_name, b"Untitled");
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = default_name;
        loaded.cpu.gpr[7] = reply;
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert!(loaded.toolbox_startup.standard_file_put_tracking.is_some());
        loaded.set_event_queue([PpcQueuedEvent {
            what: 3,
            message: u32::from(PPC_KEY_ESCAPE) << 8,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        }]);
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.memory.read_u8(reply), Some(0));
        assert_eq!(loaded.memory.read_u8(reply + 1), Some(0xaa));
    }

    #[test]
    fn standard_file_name_edit_at_maximum_length_is_safe() {
        let mut tracking = PpcStandardFilePutTrackingState {
            call: PpcStandardFileCall {
                mode: PpcStandardFileMode::PutModern,
                reply: 0,
                return_address: 0,
            },
            vref: PPC_BOOT_VOLUME_REF_NUM,
            dir_id: PPC_ROOT_DIR_ID,
            prompt: Vec::new(),
            name: vec![b'x'; 63],
            sel_start: 63,
            sel_end: 63,
            bounds: (0, 0, 0, 0),
            front_buffer: PpcFrontBuffer {
                base_addr: 0,
                row_bytes: 0,
                width: 0,
                height: 0,
                depth: 16,
            },
            saved_pixels: Vec::new().into(),
        };

        ppc_standard_file_insert_name_character(&mut tracking, b'y');
        assert_eq!(tracking.name.len(), 63);
        assert_eq!(tracking.sel_start, 63);
        assert_eq!(tracking.sel_end, 63);

        ppc_standard_file_backspace_name(&mut tracking);
        assert_eq!(tracking.name.len(), 62);
        assert_eq!(tracking.sel_start, 62);
        assert_eq!(tracking.sel_end, 62);
    }

    #[test]
    fn import_bindings_classify_all_standard_file_entry_points() {
        for (symbol, operation) in [
            ("CustomGetFile", PpcStandardFileOperation::CustomGetFile),
            ("CustomPutFile", PpcStandardFileOperation::CustomPutFile),
            ("SFGetFile", PpcStandardFileOperation::SfGetFile),
            ("SFPGetFile", PpcStandardFileOperation::SfpGetFile),
            ("SFPPutFile", PpcStandardFileOperation::SfpPutFile),
            ("SFPutFile", PpcStandardFileOperation::SfPutFile),
            ("StandardPutFile", PpcStandardFileOperation::StandardPutFile),
        ] {
            assert_eq!(
                dispatcher_target_for_import("InterfaceLib", symbol),
                PpcImportDispatcherTarget::StandardFileCompatibility(operation),
                "{symbol} must use the Standard File compatibility ABI"
            );
        }
    }

    #[test]
    fn hle_import_runner_pbh_get_finfo_returns_resource_backed_metadata() {
        let pef = synthetic_pef_with_import(b"PBHGetFInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Test App HighScores");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0400,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 200,
            name: b"Scores".to_vec(),
            data: b"score".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(
            loaded.memory.read_u32_be(pb + 32),
            Some(u32::from_be_bytes(*b"pref"))
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 36),
            Some(u32::from_be_bytes(*b"NanO"))
        );
        assert_eq!(loaded.memory.read_u16_be(pb + 40), Some(0x0400));
        assert_eq!(loaded.memory.read_u32_be(pb + 54), Some(0));
        assert_eq!(
            loaded.memory.read_u32_be(pb + 64).unwrap(),
            serialize_resource_fork(&[ResourceForkEntry {
                res_type: *b"pref",
                id: 200,
                name: b"Scores".to_vec(),
                data: b"score".to_vec(),
                attrs: 0,
            }])
            .unwrap()
            .len() as u32
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 100),
            Some(PPC_PREFERENCES_DIR_ID)
        );
    }

    #[test]
    fn hle_import_runner_pbh_set_finfo_updates_file_and_resource_metadata() {
        let pef = synthetic_pef_with_import(b"PBHSetFInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Test App HighScores");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 32, u32::from_be_bytes(*b"pref"))
            .unwrap();
        loaded
            .memory
            .write_u32_be(pb + 36, u32::from_be_bytes(*b"NanO"))
            .unwrap();
        loaded.memory.write_u16_be(pb + 40, 0x0400).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            data: (Vec::new()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(loaded.vfs_files[0].file_type, u32::from_be_bytes(*b"pref"));
        assert_eq!(loaded.vfs_files[0].creator, u32::from_be_bytes(*b"NanO"));
        assert_eq!(loaded.vfs_files[0].finder_flags, 0x0400);
        assert!(loaded.vfs_files[0].dirty);
        assert_eq!(
            loaded.vfs_resource_files[0].file_type,
            u32::from_be_bytes(*b"pref")
        );
        assert_eq!(
            loaded.vfs_resource_files[0].creator,
            u32::from_be_bytes(*b"NanO")
        );
        assert_eq!(loaded.vfs_resource_files[0].finder_flags, 0x0400);
        assert!(loaded.vfs_resource_files[0].dirty);
    }

    #[test]
    fn hle_import_runner_pb_set_finfo_missing_target_returns_fnf() {
        let pef = synthetic_pef_with_import(b"PBSetFInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded
            .default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = PPC_PREFERENCES_DIR_ID);
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Missing Prefs");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded.memory.write_u16_be(pb + 22, 0).unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 32, u32::from_be_bytes(*b"pref"))
            .unwrap();
        loaded
            .memory
            .write_u32_be(pb + 36, u32::from_be_bytes(*b"NanO"))
            .unwrap();
        loaded.memory.write_u16_be(pb + 40, 0x0400).unwrap();
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_FNF_ERR as u16));
    }

    #[test]
    fn hle_import_runner_pb_get_cat_info_returns_file_metadata() {
        let pef = synthetic_pef_with_import(b"PBGetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Test App Prefs");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            resource_len: 321,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(
            loaded.memory.read_u16_be(pb + 22),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(loaded.memory.read_u8(pb + 30), Some(0));
        assert_eq!(
            loaded.memory.read_u32_be(pb + 32),
            Some(u32::from_be_bytes(*b"pref"))
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 36),
            Some(u32::from_be_bytes(*b"NanO"))
        );
        assert_eq!(loaded.memory.read_u16_be(pb + 40), Some(0x0200));
        assert_eq!(loaded.memory.read_u32_be(pb + 54), Some(5));
        assert_eq!(loaded.memory.read_u32_be(pb + 58), Some(5));
        assert_eq!(loaded.memory.read_u32_be(pb + 64), Some(321));
        assert_eq!(loaded.memory.read_u32_be(pb + 68), Some(321));
        assert_eq!(
            loaded.memory.read_u32_be(pb + 100),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(loaded.memory.read_u8(name_ptr), Some(14));
        for (offset, byte) in b"Test App Prefs".iter().enumerate() {
            assert_eq!(
                loaded.memory.read_u8(name_ptr + 1 + offset as u32),
                Some(*byte)
            );
        }
    }

    #[test]
    fn pb_get_cat_info_resolves_a_full_path_on_a_mounted_volume() {
        let pef = synthetic_pef_with_import(b"PBGetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.seed_vfs_volumes(vec![PpcVfsVolumeRecord {
            ref_num: -2,
            name: "Gridz™ CD".to_string(),
            root_dir_id: 42,
            attributes: 0x8080,
            file_count: 1,
            allocation_block_count: 100,
            allocation_block_size: 2048,
            clump_size: 2048,
            free_blocks: 0,
            bitmap_start: 3,
            allocation_pointer: 4,
            allocation_start: 5,
            next_catalog_id: 44,
            created_date: 0,
            modified_date: 0,
        }]);
        loaded.vfs_directories.push(PpcVfsDirectory {
            dir_id: 42,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Gridz™ CD".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Gridz™ CD/ToolBot Power Supply".to_string(),
            data: (b"license".to_vec()).into(),
            creator: 0,
            file_type: u32::from_be_bytes(*b"TEXT"),
            finder_flags: 0,
            dirty: false,
        });
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b"Gridz\xaa CD:ToolBot Power Supply",
        );
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded.memory.write_u16_be(pb + 22, 0).unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded.memory.write_u32_be(pb + 48, 164).unwrap();
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 22), Some((-2i16) as u16));
        assert_eq!(loaded.memory.read_u32_be(pb + 100), Some(42));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, name_ptr),
            Some(b"ToolBot Power Supply".to_vec())
        );
    }

    #[test]
    fn hle_import_runner_handles_get_vol() {
        let pef = synthetic_pef_with_import(b"GetVol");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        let vref_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(name_ptr, vec![0xcc; 32]);
        loaded.memory.add_region(vref_ptr, vec![0xcc; 2]);
        loaded.cpu.gpr[3] = name_ptr;
        loaded.cpu.gpr[4] = vref_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, name_ptr).as_deref(),
            Some(crate::trap::TrapDispatcher::boot_volume_name().as_bytes())
        );
        assert_eq!(
            loaded.memory.read_u16_be(vref_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
    }

    #[test]
    fn hle_import_runner_handles_get_wd_info() {
        let pef = synthetic_pef_with_import(b"GetWDInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let vref_ptr = PPC_DATA_BASE + 0x1000;
        let dir_id_ptr = PPC_DATA_BASE + 0x1100;
        let proc_id_ptr = PPC_DATA_BASE + 0x1200;
        loaded.memory.add_region(vref_ptr, vec![0xcc; 2]);
        loaded.memory.add_region(dir_id_ptr, vec![0xcc; 4]);
        loaded.memory.add_region(proc_id_ptr, vec![0xcc; 4]);
        loaded
            .default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = PPC_PREFERENCES_DIR_ID);
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = vref_ptr;
        loaded.cpu.gpr[5] = dir_id_ptr;
        loaded.cpu.gpr[6] = proc_id_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(vref_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(dir_id_ptr),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(loaded.memory.read_u32_be(proc_id_ptr), Some(0));
    }

    #[test]
    fn hle_import_runner_handles_hget_vol() {
        let pef = synthetic_pef_with_import(b"HGetVol");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        let vref_ptr = PPC_DATA_BASE + 0x1100;
        let dir_id_ptr = PPC_DATA_BASE + 0x1200;
        loaded.memory.add_region(name_ptr, vec![0xcc; 32]);
        loaded.memory.add_region(vref_ptr, vec![0xcc; 2]);
        loaded.memory.add_region(dir_id_ptr, vec![0xcc; 4]);
        loaded
            .default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = PPC_PREFERENCES_DIR_ID);
        loaded.cpu.gpr[3] = name_ptr;
        loaded.cpu.gpr[4] = vref_ptr;
        loaded.cpu.gpr[5] = dir_id_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, name_ptr).as_deref(),
            Some(crate::trap::TrapDispatcher::boot_volume_name().as_bytes())
        );
        assert_eq!(
            loaded.memory.read_u16_be(vref_ptr),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(
            loaded.memory.read_u32_be(dir_id_ptr),
            Some(PPC_PREFERENCES_DIR_ID)
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = name_ptr;
        loaded.cpu.gpr[4] = vref_ptr;
        loaded.cpu.gpr[5] = PPC_DATA_BASE + 0x5000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(dir_id_ptr),
            Some(PPC_PREFERENCES_DIR_ID)
        );
    }

    #[test]
    fn hle_import_runner_hset_vol_updates_default_directory() {
        let pef = synthetic_pef_with_import(b"HSetVol");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[5] = PPC_PREFERENCES_DIR_ID;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.default_dir_id, PPC_PREFERENCES_DIR_ID);
        assert_eq!(
            loaded
                .memory
                .read_u32_be(crate::memory::globals::addr::CUR_DIR_STORE),
            Some(PPC_PREFERENCES_DIR_ID)
        );
    }

    #[test]
    fn hle_import_runner_handles_flush_vol() {
        let pef = synthetic_pef_with_import(b"FlushVol");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 32]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            crate::trap::TrapDispatcher::boot_volume_name().as_bytes(),
        );
        loaded.cpu.gpr[3] = name_ptr;
        loaded.cpu.gpr[4] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_DATA_BASE + 0x5000;
        loaded.cpu.gpr[4] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    }

    #[test]
    fn hle_import_runner_pb_get_cat_info_returns_directory_metadata() {
        let pef = synthetic_pef_with_import(b"PBGetCatInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Preferences");
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_SYSTEM_FOLDER_DIR_ID)
            .unwrap();
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(loaded.memory.read_u8(pb + 30), Some(0x10));
        assert_eq!(
            loaded.memory.read_u32_be(pb + 32),
            Some(u32::from_be_bytes(*b"fold"))
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 36),
            Some(u32::from_be_bytes(*b"MACS"))
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 48),
            Some(PPC_PREFERENCES_DIR_ID)
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 100),
            Some(PPC_SYSTEM_FOLDER_DIR_ID)
        );
    }

    #[test]
    fn pb_get_cat_info_absolute_boot_path_ignores_current_directory() {
        let directories = initial_ppc_vfs_directories();
        let entry = ppc_catalog_entry_for_lookup(
            &directories,
            &[],
            &[],
            PPC_PREFERENCES_DIR_ID,
            b"MacintoshHD:System Folder:Preferences",
            0,
        )
        .expect("absolute boot-volume pathname");
        assert_eq!(entry.path, "System Folder/Preferences");
        assert!(entry.is_directory);
    }

    #[test]
    fn hle_import_runner_pb_get_cat_info_enumerates_children_by_index() {
        let pef = synthetic_pef_with_import(b"PBGetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"");
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 1).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            dirty: false,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(loaded.memory.read_u8(name_ptr), Some(14));
        for (offset, byte) in b"Test App Prefs".iter().enumerate() {
            assert_eq!(
                loaded.memory.read_u8(name_ptr + 1 + offset as u32),
                Some(*byte)
            );
        }
        assert_eq!(
            loaded.memory.read_u32_be(pb + 100),
            Some(PPC_PREFERENCES_DIR_ID)
        );
    }

    #[test]
    fn native_partition_growth_skips_stack_display_and_system_reservations() {
        let pef = synthetic_pef_with_import(b"NewPtrClear");
        let mut loaded = load_pef_application(&pef).unwrap();
        let old_limit = loaded.heap_limit();
        let sp = loaded.cpu.gpr[1];
        loaded.memory.write_u32_be(sp, 0xdecafbad).unwrap();
        loaded
            .memory
            .write_u8(PPC_DSP_BACK_SCREEN_BASE, 0x7b)
            .unwrap();
        loaded
            .memory
            .add_readonly_allocation_exclusion(PPC_STACK_TOP + 8 * 1024 * 1024, 4096)
            .unwrap();
        let partition = 96 * 1024 * 1024;
        loaded.grow_application_partition(partition);
        assert_eq!(
            ppc_heap_free_capacity(&loaded.memory, loaded.heap_base(), loaded.heap_limit()).0,
            partition - loaded.stack_size
        );
        loaded.set_heap_cursor(old_limit - 16);
        loaded.cpu.gpr[3] = 128;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert!(loaded.cpu.gpr[3] >= PPC_DSP_BACK_SCREEN_BASE + ppc_main_screen_buffer_size());
        assert_eq!(loaded.memory.read_u32_be(sp), Some(0xdecafbad));
        assert_eq!(loaded.memory.read_u8(PPC_DSP_BACK_SCREEN_BASE), Some(0x7b));
        assert_eq!(loaded.memory.read_u32_be(loaded.cpu.gpr[3]), Some(0));
    }

    #[test]
    fn hle_import_runner_pb_get_cat_info_negative_index_returns_directory_name_without_reading_buffer() {
        let pef = synthetic_pef_with_import(b"PBGetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        // The output buffer need not contain a readable input Pascal string.
        loaded.memory.write_u8(name_ptr, 255).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, (-1i16) as u16).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Settings".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"TEST"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(loaded.memory.read_u16_be(pb + 52), Some(1));
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, name_ptr).as_deref(),
            Some(b"Preferences".as_slice())
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 100),
            Some(PPC_SYSTEM_FOLDER_DIR_ID)
        );
    }

    #[test]
    fn hle_import_runner_pb_get_cat_info_missing_target_returns_fnf() {
        let pef = synthetic_pef_with_import(b"PBGetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Missing Prefs");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_FNF_ERR as u16));
    }

    #[test]
    fn hle_import_runner_pb_set_cat_info_updates_file_and_resource_metadata() {
        let pef = synthetic_pef_with_import(b"PBSetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Test App Prefs");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 32, u32::from_be_bytes(*b"TEXT"))
            .unwrap();
        loaded
            .memory
            .write_u32_be(pb + 36, u32::from_be_bytes(*b"ttxt"))
            .unwrap();
        loaded.memory.write_u16_be(pb + 40, 0x4000).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            resource_len: 321,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(
            loaded.memory.read_u16_be(pb + 22),
            Some(PPC_BOOT_VOLUME_REF_NUM as u16)
        );
        assert_eq!(loaded.vfs_files[0].file_type, u32::from_be_bytes(*b"TEXT"));
        assert_eq!(loaded.vfs_files[0].creator, u32::from_be_bytes(*b"ttxt"));
        assert_eq!(loaded.vfs_files[0].finder_flags, 0x4000);
        assert!(loaded.vfs_files[0].dirty);
        assert_eq!(
            loaded.vfs_resource_files[0].file_type,
            u32::from_be_bytes(*b"TEXT")
        );
        assert_eq!(
            loaded.vfs_resource_files[0].creator,
            u32::from_be_bytes(*b"ttxt")
        );
        assert_eq!(loaded.vfs_resource_files[0].finder_flags, 0x4000);
        assert!(loaded.vfs_resource_files[0].dirty);
    }

    #[test]
    fn hle_import_runner_pb_set_cat_info_updates_directory_metadata() {
        let pef = synthetic_pef_with_import(b"PBSetCatInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Preferences");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 32, u32::from_be_bytes(*b"fold"))
            .unwrap();
        loaded
            .memory
            .write_u32_be(pb + 36, u32::from_be_bytes(*b"MACS"))
            .unwrap();
        loaded.memory.write_u16_be(pb + 40, 0x0010).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_SYSTEM_FOLDER_DIR_ID)
            .unwrap();
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert!(loaded.vfs_files.is_empty());
        assert!(loaded.vfs_resource_files.is_empty());
        let directory = loaded
            .vfs_directories
            .iter()
            .find(|directory| directory.path == "System Folder/Preferences")
            .unwrap();
        assert_eq!(directory.file_type, PPC_DIRECTORY_FILE_TYPE);
        assert_eq!(directory.creator, PPC_DIRECTORY_CREATOR);
        assert_eq!(directory.finder_flags, 0x0010);
        assert!(directory.dirty);

        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 32, 0).unwrap();
        loaded.memory.write_u32_be(pb + 36, 0).unwrap();
        loaded.memory.write_u16_be(pb + 40, 0).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PBGetCatInfo;
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(loaded.memory.read_u8(pb + 30), Some(0x10));
        assert_eq!(
            loaded.memory.read_u32_be(pb + 32),
            Some(PPC_DIRECTORY_FILE_TYPE)
        );
        assert_eq!(
            loaded.memory.read_u32_be(pb + 36),
            Some(PPC_DIRECTORY_CREATOR)
        );
        assert_eq!(loaded.memory.read_u16_be(pb + 40), Some(0x0010));
        let exports = loaded.take_dirty_vfs_directories();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].path, "System Folder/Preferences");
        assert_eq!(exports[0].file_type, PPC_DIRECTORY_FILE_TYPE);
        assert_eq!(exports[0].creator, PPC_DIRECTORY_CREATOR);
        assert_eq!(exports[0].finder_flags, 0x0010);
        assert!(loaded.take_dirty_vfs_directories().is_empty());
    }

    #[test]
    fn hle_import_runner_pb_set_cat_info_missing_target_returns_fnf() {
        let pef = synthetic_pef_with_import(b"PBSetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 128;
        loaded.memory.add_region(pb, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Missing Prefs");
        loaded.memory.write_u16_be(pb + 16, 0x3fff).unwrap();
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded
            .memory
            .write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .unwrap();
        loaded.memory.write_u16_be(pb + 28, 0).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 32, u32::from_be_bytes(*b"TEXT"))
            .unwrap();
        loaded
            .memory
            .write_u32_be(pb + 36, u32::from_be_bytes(*b"ttxt"))
            .unwrap();
        loaded.memory.write_u16_be(pb + 40, 0x4000).unwrap();
        loaded
            .memory
            .write_u32_be(pb + 48, PPC_PREFERENCES_DIR_ID)
            .unwrap();
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_FNF_ERR as u16));
    }

    #[test]
    fn hle_import_runner_pb_set_cat_info_prevalidates_result_writeback() {
        let pef = synthetic_pef_with_import(b"PBSetCatInfoSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let pb = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(pb, vec![0; 17]);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            dirty: false,
        });
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.vfs_files[0].file_type, u32::from_be_bytes(*b"pref"));
        assert_eq!(loaded.vfs_files[0].creator, u32::from_be_bytes(*b"NanO"));
        assert_eq!(loaded.vfs_files[0].finder_flags, 0x0200);
        assert!(!loaded.vfs_files[0].dirty);
    }

    #[test]
    fn hle_import_runner_h_open_resolves_working_directory_refnum() {
        let pef = synthetic_pef_with_import(b"HOpen");
        let mut loaded = load_pef_application(&pef).unwrap();
        let app_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let wd_ref_num = 32;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: app_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Game Folder".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, app_dir_id, app_dir_id + 1);
        loaded.working_directories.with_mut(|directories| {
            directories.insert(
                wd_ref_num,
                ProcessWorkingDirectory {
                    ref_num: wd_ref_num,
                    volume_ref_num: PPC_BOOT_VOLUME_REF_NUM,
                    dir_id: app_dir_id,
                    proc_id: 0,
                },
            );
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Folder/High.Scores".to_string(),
            data: (b"scores".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        let scratch = PPC_DATA_BASE + 0x1000;
        let ref_num_out_ptr = scratch + 64;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"High.Scores");
        loaded.cpu.gpr[3] = wd_ref_num as u16 as u32;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = scratch;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(ref_num_out_ptr),
            Some(PPC_FIRST_FILE_REF_NUM as u16)
        );
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].path, "Game Folder/High.Scores");
    }

    #[test]
    fn hle_import_runner_pb_open_sync_uses_current_working_directory() {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", "PBOpenSync"),
            PpcImportDispatcherTarget::PBOpen
        );
        let pef = synthetic_pef_with_import(b"PBOpenSync");
        let mut loaded = load_pef_application(&pef).unwrap();
        let app_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let wd_ref_num = 32;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: app_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Game Folder".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, app_dir_id, app_dir_id + 1);
        loaded.working_directories.with_mut(|directories| {
            directories.insert(
                wd_ref_num,
                ProcessWorkingDirectory {
                    ref_num: wd_ref_num,
                    volume_ref_num: PPC_BOOT_VOLUME_REF_NUM,
                    dir_id: app_dir_id,
                    proc_id: 0,
                },
            );
        });
        loaded
            .application_working_directory_ref_num
            .with_mut(|ref_num| *ref_num = wd_ref_num);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Folder/Scores".to_string(),
            data: (b"scores".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        let pb = PPC_DATA_BASE + 0x1000;
        let name_ptr = pb + 64;
        loaded.memory.add_region(pb, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Scores");
        loaded.memory.write_u32_be(pb + 18, name_ptr).unwrap();
        loaded.memory.write_u16_be(pb + 22, 0).unwrap();
        loaded.memory.write_u16_be(pb + 24, 0xcafe).unwrap();
        loaded.memory.write_u8(pb + 27, 1).unwrap();
        loaded.cpu.gpr[3] = pb;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u16_be(pb + 16), Some(PPC_NO_ERR as u16));
        assert_eq!(
            loaded.memory.read_u16_be(pb + 24),
            Some(PPC_FIRST_FILE_REF_NUM as u16)
        );
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].path, "Game Folder/Scores");
    }

    #[test]
    fn hle_import_runner_handles_fsp_open_df() {
        let pef = synthetic_pef_with_import(b"FSpOpenDF");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let ref_num_out_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            dirty: false,
        });
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1; // fsRdPerm
        loaded.cpu.gpr[5] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u16_be(ref_num_out_ptr),
            Some(PPC_FIRST_FILE_REF_NUM as u16)
        );
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].ref_num, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(
            loaded.files[0].path,
            "System Folder/Preferences/Test App Prefs"
        );
        assert_eq!(loaded.vfs_files.len(), 1);
        assert_eq!(
            loaded.vfs_files[0].path,
            "System Folder/Preferences/Test App Prefs"
        );
        assert_eq!(loaded.vfs_files[0].data, b"prefs");
        assert!(!loaded.vfs_files[0].dirty);
        assert_eq!(loaded.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 1);
        assert!(!loaded
            .process_file_system
            .writable_refnums
            .contains(&(PPC_FIRST_FILE_REF_NUM as u16)));

        let count_ptr = scratch + 84;
        let buffer_ptr = scratch + 88;
        loaded.memory.write_u32_be(count_ptr, 1).unwrap();
        loaded.memory.write_u8(buffer_ptr, b'x').unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSWrite;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = count_ptr;
        loaded.cpu.gpr[5] = buffer_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_WR_PERM_ERR));
        assert_eq!(loaded.memory.read_u32_be(count_ptr), Some(1));
        assert_eq!(loaded.vfs_files[0].data, b"prefs");
    }

    #[test]
    fn hle_import_runner_fsp_open_df_resolves_unique_archive_suffix_path() {
        let pef = synthetic_pef_with_import(b"FSpOpenDF");
        let mut loaded = load_pef_application(&pef).unwrap();
        let data_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let audio_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: data_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Data".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: audio_dir_id,
            parent_dir_id: data_dir_id,
            path: "Data/Audio".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, PPC_ROOT_DIR_ID, audio_dir_id + 1);
        let scratch = PPC_DATA_BASE + 0x1000;
        let ref_num_out_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_ROOT_DIR_ID,
            b":audio:song_pangea",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Data/Audio/Song_Pangea".to_string(),
            data: (b"aiff".to_vec()).into(),
            creator: 0,
            file_type: u32::from_be_bytes(*b"AIFF"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(loaded.files[0].path, "Data/Audio/Song_Pangea");
    }

    #[test]
    fn hle_import_runner_fsp_open_df_prefers_unique_non_empty_file_over_empty_placeholder() {
        let pef = synthetic_pef_with_import(b"FSpOpenDF");
        let mut loaded = load_pef_application(&pef).unwrap();
        let app_dir_id = PPC_FIRST_DYNAMIC_DIR_ID;
        let data_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1;
        let mut directories = initial_ppc_vfs_directories();
        directories.push(PpcVfsDirectory {
            dir_id: app_dir_id,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "Gridz Demo".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        directories.push(PpcVfsDirectory {
            dir_id: data_dir_id,
            parent_dir_id: app_dir_id,
            path: "Gridz Demo/Gridz Data".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        });
        loaded.seed_vfs_directories(directories, app_dir_id, data_dir_id + 1);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Gridz Demo/Gridz Demo Bits".to_string(),
            data: (Vec::new()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Gridz Demo/Gridz Data/Gridz Demo Bits".to_string(),
            data: (b"packed-bits".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        let scratch = PPC_DATA_BASE + 0x1000;
        let ref_num_out_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            app_dir_id,
            b"Gridz Demo Bits",
        );
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 3;
        loaded.cpu.gpr[5] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.files.len(), 1);
        assert_eq!(
            loaded.files[0].path,
            "Gridz Demo/Gridz Data/Gridz Demo Bits"
        );
    }

    #[test]
    fn hle_import_runner_fsp_open_df_missing_file_returns_fnf() {
        let pef = synthetic_pef_with_import(b"FSpOpenDF");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let ref_num_out_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Missing Prefs",
        );
        loaded.memory.write_u16_be(ref_num_out_ptr, 0xcafe).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.memory.read_u16_be(ref_num_out_ptr), Some(0xcafe));
        assert!(loaded.files.is_empty());
        assert!(loaded.vfs_files.is_empty());
        assert_eq!(loaded.next_file_ref_num, PPC_FIRST_FILE_REF_NUM);
    }

    #[test]
    fn hle_import_runner_file_open_and_position_outputs_are_all_or_nothing() {
        let pef = synthetic_pef_with_import(b"FSpOpenDF");
        let mut loaded = load_pef_application(&pef).unwrap();
        let spec_ptr = PPC_DATA_BASE + 0x1000;
        let ref_num_out_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(spec_ptr, vec![0; 70]);
        loaded.memory.add_region(ref_num_out_ptr, vec![0xee; 1]);
        write_ppc_fsspec(
            &mut loaded.memory,
            spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(ref_num_out_ptr), Some(0xee));
        assert!(loaded.files.is_empty());
        assert!(loaded.vfs_files.is_empty());
        assert_eq!(loaded.next_file_ref_num, PPC_FIRST_FILE_REF_NUM);

        let pef = synthetic_pef_with_import(b"GetEOF");
        let mut loaded = load_pef_application(&pef).unwrap();
        let out_ptr = PPC_DATA_BASE + 0x1200;
        loaded.memory.add_region(out_ptr, vec![0xab; 2]);
        loaded.push_test_open_file(PpcFileRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            position: 3,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u16_be(out_ptr), Some(0xabab));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetFPos;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u16_be(out_ptr), Some(0xabab));
        assert_eq!(loaded.files[0].position, 3);
    }

    #[test]
    fn hle_import_runner_gets_and_sets_ppc_finder_info_for_data_fork() {
        let pef = synthetic_pef_with_import(b"FSpGetFInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let finfo_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"old".to_vec()).into(),
            creator: u32::from_be_bytes(*b"NanO"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            dirty: false,
        });
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(finfo_ptr),
            Some(u32::from_be_bytes(*b"pref"))
        );
        assert_eq!(
            loaded.memory.read_u32_be(finfo_ptr + 4),
            Some(u32::from_be_bytes(*b"NanO"))
        );
        assert_eq!(loaded.memory.read_u16_be(finfo_ptr + 8), Some(0x0200));

        loaded
            .memory
            .write_u32_be(finfo_ptr, u32::from_be_bytes(*b"TEXT"))
            .unwrap();
        loaded
            .memory
            .write_u32_be(finfo_ptr + 4, u32::from_be_bytes(*b"ttxt"))
            .unwrap();
        loaded.memory.write_u16_be(finfo_ptr + 8, 0x4000).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpSetFInfo;
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_files[0].file_type, u32::from_be_bytes(*b"TEXT"));
        assert_eq!(loaded.vfs_files[0].creator, u32::from_be_bytes(*b"ttxt"));
        assert_eq!(loaded.vfs_files[0].finder_flags, 0x4000);
        assert!(loaded.vfs_files[0].dirty);
        let exports = loaded.take_dirty_vfs_files();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].file_type, u32::from_be_bytes(*b"TEXT"));
        assert_eq!(exports[0].creator, u32::from_be_bytes(*b"ttxt"));
        assert_eq!(exports[0].finder_flags, 0x4000);
    }

    #[test]
    fn hle_import_runner_gets_and_sets_ppc_finder_info_for_directory() {
        let pef = synthetic_pef_with_import(b"FSpGetFInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let finfo_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_SYSTEM_FOLDER_DIR_ID,
            b"Preferences",
        );
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(finfo_ptr),
            Some(PPC_DIRECTORY_FILE_TYPE)
        );
        assert_eq!(
            loaded.memory.read_u32_be(finfo_ptr + 4),
            Some(PPC_DIRECTORY_CREATOR)
        );
        assert_eq!(loaded.memory.read_u16_be(finfo_ptr + 8), Some(0));

        loaded
            .memory
            .write_u32_be(finfo_ptr, u32::from_be_bytes(*b"dir "))
            .unwrap();
        loaded
            .memory
            .write_u32_be(finfo_ptr + 4, u32::from_be_bytes(*b"Nano"))
            .unwrap();
        loaded.memory.write_u16_be(finfo_ptr + 8, 0x0080).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpSetFInfo;
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let directory = loaded
            .vfs_directories
            .iter()
            .find(|directory| directory.path == "System Folder/Preferences")
            .unwrap();
        assert_eq!(directory.file_type, u32::from_be_bytes(*b"dir "));
        assert_eq!(directory.creator, u32::from_be_bytes(*b"Nano"));
        assert_eq!(directory.finder_flags, 0x0080);
        assert!(directory.dirty);

        loaded.memory.write_u32_be(finfo_ptr, 0).unwrap();
        loaded.memory.write_u32_be(finfo_ptr + 4, 0).unwrap();
        loaded.memory.write_u16_be(finfo_ptr + 8, 0).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpGetFInfo;
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.memory.read_u32_be(finfo_ptr),
            Some(u32::from_be_bytes(*b"dir "))
        );
        assert_eq!(
            loaded.memory.read_u32_be(finfo_ptr + 4),
            Some(u32::from_be_bytes(*b"Nano"))
        );
        assert_eq!(loaded.memory.read_u16_be(finfo_ptr + 8), Some(0x0080));
        let exports = loaded.take_dirty_vfs_directories();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].path, "System Folder/Preferences");
        assert_eq!(exports[0].file_type, u32::from_be_bytes(*b"dir "));
        assert_eq!(exports[0].creator, u32::from_be_bytes(*b"Nano"));
        assert_eq!(exports[0].finder_flags, 0x0080);
    }

    #[test]
    fn hle_import_runner_sets_ppc_finder_info_for_path_backed_resource_file() {
        let pef = synthetic_pef_with_import(b"FSpSetFInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let finfo_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App HighScores",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            data: (Vec::new()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 200,
            name: b"Scores".to_vec(),
            data: b"score".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded
            .memory
            .write_u32_be(finfo_ptr, u32::from_be_bytes(*b"pref"))
            .unwrap();
        loaded
            .memory
            .write_u32_be(finfo_ptr + 4, u32::from_be_bytes(*b"NanO"))
            .unwrap();
        loaded.memory.write_u16_be(finfo_ptr + 8, 0x0400).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_files[0].file_type, u32::from_be_bytes(*b"pref"));
        assert_eq!(loaded.vfs_files[0].creator, u32::from_be_bytes(*b"NanO"));
        assert_eq!(loaded.vfs_files[0].finder_flags, 0x0400);
        assert!(loaded.vfs_files[0].dirty);
        assert_eq!(
            loaded.vfs_resource_files[0].file_type,
            u32::from_be_bytes(*b"pref")
        );
        assert_eq!(
            loaded.vfs_resource_files[0].creator,
            u32::from_be_bytes(*b"NanO")
        );
        assert_eq!(loaded.vfs_resource_files[0].finder_flags, 0x0400);
        assert!(loaded.vfs_resource_files[0].dirty);
        assert_eq!(loaded.take_dirty_vfs_files()[0].finder_flags, 0x0400);
        assert_eq!(
            loaded.take_dirty_vfs_resource_forks()[0].finder_flags,
            0x0400
        );
    }

    #[test]
    fn hle_import_runner_fsp_get_finfo_missing_file_returns_fnf() {
        let pef = synthetic_pef_with_import(b"FSpGetFInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let finfo_ptr = scratch + 80;
        loaded.memory.add_region(scratch, vec![0xff; 128]);
        write_ppc_fsspec(
            &mut loaded.memory,
            scratch,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Missing Prefs",
        );
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = finfo_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FNF_ERR));
        assert_eq!(loaded.memory.read_u32_be(finfo_ptr), Some(0));
        assert_eq!(loaded.memory.read_u32_be(finfo_ptr + 4), Some(0));
        assert_eq!(loaded.memory.read_u16_be(finfo_ptr + 8), Some(0));
    }

    #[test]
    fn hle_import_runner_handles_fsp_open_res_file() {
        let pef = synthetic_pef_with_import(b"FSpOpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let spec_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(spec_ptr, vec![0; 70]);
        write_ppc_fsspec(
            &mut loaded.memory,
            spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.set_test_resource_error(PPC_RES_NOT_FOUND_ERR);
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 1; // fsRdPerm

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FIRST_FILE_REF_NUM));
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.resource_files.len(), 1);
        assert_eq!(
            loaded.resource_files[0].path,
            "System Folder/Preferences/Test App Prefs"
        );
        assert_eq!(loaded.vfs_resource_files.len(), 1);
        assert_eq!(loaded.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 1);
    }

    #[test]
    fn hle_import_runner_handles_open_res_file_by_basename() {
        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Test App Prefs");
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.resource_files.len(), 1);
        assert_eq!(
            loaded.resource_files[0].path,
            "System Folder/Preferences/Test App Prefs"
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        assert_eq!(loaded.resource_files.len(), 1);
    }

    #[test]
    fn hle_import_runner_open_res_file_scopes_duplicate_basename_to_launched_app() {
        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 64]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Board.Map");
        loaded.set_launched_app_path("Demo Install/Game Folder/Gridz Demo");
        for path in [
            "Full Install/Game Data/Graphics/Board.Map",
            "Demo Install/Game Folder/Game Data/Graphics/Board.Map",
        ] {
            loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
                path: path.to_string(),
                creator: u32::from_be_bytes(*b"BABL"),
                file_type: u32::from_be_bytes(*b"PICT"),
                finder_flags: 0,
                resource_len: 0,
                raw_data: None,
                map_attrs: 0,
                dirty: false,
            });
        }
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        assert_eq!(loaded.resource_files.len(), 1);
        assert_eq!(
            loaded.resource_files[0].path,
            "Demo Install/Game Folder/Game Data/Graphics/Board.Map"
        );
    }

    #[test]
    fn hle_import_runner_open_res_file_materializes_unique_named_archive_resource() {
        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b":Game Data:Graphics:Common:ColorSwatches.PICR",
        );
        let raw_fork = serialize_resource_fork(&[
            ResourceForkEntry {
                res_type: *b"#Img",
                id: 31999,
                name: b"ColorSwatches.PICR".to_vec(),
                data: b"image-metadata".to_vec(),
                attrs: 0,
            },
            ResourceForkEntry {
                res_type: *b"PICT",
                id: 32000,
                name: b"ColorSwatches.PICR".to_vec(),
                data: b"packed-picture".to_vec(),
                attrs: 0x20,
            },
        ])
        .unwrap();
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.resource_files.len(), 1);
        assert_eq!(
            loaded.resource_files[0].path,
            "Game Data/Graphics/Common/ColorSwatches.PICR"
        );
        assert_eq!(loaded.process_file_system.vfs_resources.len(), 2);
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].path,
            "Game Data/Graphics/Common/ColorSwatches.PICR"
        );
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].res_type,
            u32::from_be_bytes(*b"#Img")
        );
        assert_eq!(loaded.process_file_system.vfs_resources[0].res_id, 31999);
        assert_eq!(loaded.process_file_system.vfs_resources[0].data, b"image-metadata");
        assert_eq!(
            loaded.process_file_system.vfs_resources[1].path,
            "Game Data/Graphics/Common/ColorSwatches.PICR"
        );
        assert_eq!(
            loaded.process_file_system.vfs_resources[1].res_type,
            u32::from_be_bytes(*b"PICT")
        );
        assert_eq!(loaded.process_file_system.vfs_resources[1].res_id, 32000);
        assert_eq!(loaded.process_file_system.vfs_resources[1].data, b"packed-picture");
    }

    #[test]
    fn hle_import_runner_open_res_file_materializes_quilt_qdir_entries() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b":Game Data:Graphics:Common:ColorSwatches.PICR",
        );
        let mut data = vec![0u8; 128];
        data[16..21].copy_from_slice(b"#data");
        data[64..72].copy_from_slice(b"pictdata");
        let name = b"ColorSwatches.PICR";
        let mut qdir = Vec::new();
        qdir.extend_from_slice(&qdir_record(b"#Img", 1000, 5, 16, name));
        qdir.extend_from_slice(&qdir_record(b"PICT", 1000, 8, 64, name));
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir,
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.resource_files.len(), 1);
        assert_eq!(
            loaded.resource_files[0].path,
            "Game Data/Graphics/Common/ColorSwatches.PICR"
        );
        assert_eq!(loaded.process_file_system.vfs_resources.len(), 2);
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].res_type,
            u32::from_be_bytes(*b"#Img")
        );
        assert_eq!(loaded.process_file_system.vfs_resources[0].data, b"#data");
        assert_eq!(
            loaded.process_file_system.vfs_resources[1].res_type,
            u32::from_be_bytes(*b"PICT")
        );
        assert_eq!(loaded.process_file_system.vfs_resources[1].data, b"pictdata");
    }

    #[test]
    fn prepare_vfs_resource_forks_for_native_export_combines_base_and_quilt_resources() {
        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let mut bits = vec![0u8; 64];
        bits[16..21].copy_from_slice(b"state");

        let mut qdir = [0u8; 60];
        qdir[0..4].copy_from_slice(b"Stbl");
        qdir[4..8].copy_from_slice(&1003u32.to_be_bytes());
        qdir[8..12].copy_from_slice(&5u32.to_be_bytes());
        qdir[12..16].copy_from_slice(&16u32.to_be_bytes());
        qdir[24..26].copy_from_slice(&1u16.to_be_bytes());
        qdir[26..28].copy_from_slice(&12u16.to_be_bytes());
        qdir[28..40].copy_from_slice(b"Game Control");
        let bits_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir.to_vec(),
            attrs: 0,
        }])
        .unwrap();
        let target_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"PICT",
            id: 1000,
            name: b"Game Control 2".to_vec(),
            data: b"picture".to_vec(),
            attrs: 0,
        }])
        .unwrap();

        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Gridz Data/Gridz Demo Bits".to_string(),
            data: (bits).into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Gridz Data/Gridz Demo Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: bits_fork.len() as u32,
            raw_data: Some(bits_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Gridz Data/Control Files/Game Control 2".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"ctrl"),
            finder_flags: 0,
            resource_len: target_fork.len() as u32,
            raw_data: Some(target_fork.into()),
            map_attrs: 0,
            dirty: false,
        });

        assert_eq!(loaded.prepare_vfs_resource_forks_for_native_export(), 2);
        let exports = loaded.take_dirty_vfs_resource_forks();
        let target_export = exports
            .iter()
            .find(|export| export.path == "Gridz Data/Control Files/Game Control 2")
            .unwrap();
        let fork = ResourceFork::parse(&target_export.data).unwrap();
        assert_eq!(fork.get(*b"PICT", 1000).unwrap().data, b"picture");
        assert_eq!(fork.get(*b"Stbl", 1003).unwrap().data, b"state");
    }

    #[test]
    fn hle_import_runner_open_res_file_keeps_path_qualified_quilt_lookup() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b":Game Data:Graphics:Common:Options Button Control",
        );
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Control Files/Options Button Control".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"ctrl"),
            finder_flags: 0,
            resource_len: 4,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "Game Data/Control Files/Options Button Control".to_string(),
            res_type: u32::from_be_bytes(*b"Alst"),
            res_id: 1000,
            name: b"Options Button Control".to_vec(),
            data: b"control".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        let mut data = vec![0u8; 128];
        data[16..21].copy_from_slice(b"#data");
        data[64..72].copy_from_slice(b"pictdata");
        let name = b"Options Button Control";
        let mut qdir = Vec::new();
        qdir.extend_from_slice(&qdir_record(b"#Img", 1000, 5, 16, name));
        qdir.extend_from_slice(&qdir_record(b"PICT", 1000, 8, 64, name));
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir,
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.resource_files.len(), 1);
        assert_eq!(
            loaded.resource_files[0].path,
            "Game Data/Graphics/Common/Options Button Control"
        );
        assert!(loaded.process_file_system.vfs_resources.iter().any(|resource| {
            resource.path == "Game Data/Graphics/Common/Options Button Control"
                && resource.res_type == u32::from_be_bytes(*b"PICT")
                && resource.data == b"pictdata"
        }));
    }

    #[test]
    fn ppc_vfs_file_or_resource_path_matches_path_qualified_parent_basename() {
        let vfs_files = vec![PpcVfsFileRecord {
            path: "Gridz Demo/Gridz Data/Control Files/Game Control 2".to_string(),
            data: (Vec::new()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        }];
        let vfs_resource_files = Vec::new();

        assert_eq!(
            ppc_vfs_file_or_resource_path_by_parent_basename(
                &vfs_files,
                &vfs_resource_files,
                "Gridz Demo Mojibake/Gridz Data Mojibake/Control Files/Game Control 2",
            ),
            Some("Gridz Demo/Gridz Data/Control Files/Game Control 2".to_string())
        );
        assert_eq!(
            ppc_vfs_file_or_resource_path_by_parent_basename(
                &vfs_files,
                &vfs_resource_files,
                "Gridz Demo Mojibake/Gridz Data Mojibake/Graphics/Common/Game Control 2",
            ),
            None
        );
    }

    #[test]
    fn hle_import_runner_open_res_file_adds_quilt_anam_picture_resources() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b":Game Data:Graphics:Common:Options Button Control",
        );
        let control_name = b"Options Button Control";
        let picture_name = b"MMOptions Button.PICR";
        let mut data = vec![0u8; 128];
        data[8..15].copy_from_slice(b"control");
        data[16..39].copy_from_slice(b"MMOptions Button.PICR\0x");
        data[48..53].copy_from_slice(b"#data");
        data[64..72].copy_from_slice(b"pictdata");
        let mut qdir = Vec::new();
        qdir.extend_from_slice(&qdir_record(b"Alst", 1000, 7, 8, control_name));
        qdir.extend_from_slice(&qdir_record(b"ANAM", 1000, 23, 16, control_name));
        qdir.extend_from_slice(&qdir_record(b"#Img", 1000, 5, 48, picture_name));
        qdir.extend_from_slice(&qdir_record(b"PICT", 1000, 8, 64, picture_name));
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir,
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.resource_files.len(), 1);
        assert_eq!(
            loaded.resource_files[0].path,
            "Game Data/Graphics/Common/Options Button Control"
        );
        assert!(loaded.process_file_system.vfs_resources.iter().any(|resource| {
            resource.path == "Game Data/Graphics/Common/Options Button Control"
                && resource.res_type == u32::from_be_bytes(*b"Alst")
                && resource.data == b"control"
        }));
        assert!(loaded.process_file_system.vfs_resources.iter().any(|resource| {
            resource.path == "Game Data/Graphics/Common/Options Button Control"
                && resource.res_type == u32::from_be_bytes(*b"PICT")
                && resource.data == b"pictdata"
        }));
    }

    #[test]
    fn hle_import_runner_open_res_file_uses_quilt_when_named_matches_are_ambiguous() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b":Game Data:Control Files:Options Button Control",
        );
        let ambiguous = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"STR#",
            id: 1000,
            name: b"Options Button Control".to_vec(),
            data: b"wrong".to_vec(),
            attrs: 0,
        }])
        .unwrap();
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Other One".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            resource_len: ambiguous.len() as u32,
            raw_data: Some(ambiguous.clone().into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Other Two".to_string(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            resource_len: ambiguous.len() as u32,
            raw_data: Some(ambiguous.into()),
            map_attrs: 0,
            dirty: false,
        });
        let control_name = b"Options Button Control";
        let mut data = vec![0u8; 64];
        data[8..15].copy_from_slice(b"control");
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir_record(b"Alst", 1000, 7, 8, control_name).to_vec(),
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            loaded.resource_files[0].path,
            "Game Data/Control Files/Options Button Control"
        );
        assert!(loaded.process_file_system.vfs_resources.iter().any(|resource| {
            resource.path == "Game Data/Control Files/Options Button Control"
                && resource.res_type == u32::from_be_bytes(*b"Alst")
                && resource.data == b"control"
        }));
    }

    #[test]
    fn hle_import_runner_open_res_file_splits_compact_quilt_frames() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b":Game Data:Sprite.PICR");
        let mut data = vec![0u8; 128];
        data[8..26].copy_from_slice(&[0, 1, 0, 1, 0, 1, 0, 2, 0, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
        data[32..48].copy_from_slice(&[0, 0, 0, 0, 0, 4, 0, 4, 0, 0, 0, 0, 0, 4, 0, 4]);
        data[64..72].copy_from_slice(b"aaaabbbb");
        let name = b"Sprite.PICR";
        let mut qdir = Vec::new();
        qdir.extend_from_slice(&qdir_record(b"#Img", 1000, 18, 8, name));
        qdir.extend_from_slice(&qdir_record(b"frms", 1000, 16, 32, name));
        qdir.extend_from_slice(&qdir_record(b"PICT", 1000, 8, 64, name));
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir,
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        let pict_type = u32::from_be_bytes(*b"PICT");
        let picts = loaded
            .vfs_resources
            .iter()
            .filter(|resource| resource.res_type == pict_type)
            .collect::<Vec<_>>();
        assert_eq!(picts.len(), 2);
        assert_eq!(picts[0].res_id, 1000);
        assert_eq!(u16::from_be_bytes([picts[0].data[0], picts[0].data[1]]), 28);
        assert_eq!(&picts[0].data[2..10], &[0, 0, 0, 0, 0, 4, 0, 4]);
        assert_eq!(&picts[0].data[10..24], &[0; 14]);
        assert_eq!(&picts[0].data[24..], b"aaaa");
        assert_eq!(picts[1].res_id, 1001);
        assert_eq!(u16::from_be_bytes([picts[1].data[0], picts[1].data[1]]), 28);
        assert_eq!(&picts[1].data[2..10], &[0, 0, 0, 0, 0, 4, 0, 4]);
        assert_eq!(&picts[1].data[10..24], &[0; 14]);
        assert_eq!(&picts[1].data[24..], b"bbbb");
    }

    #[test]
    fn hle_import_runner_open_res_file_infers_compact_quilt_frames_from_payload_pair() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b":Game Data:Graphics:Common:ColorSwatches.PICR",
        );

        let frame_count = 30usize;
        let mut data = vec![0u8; 16 + 18 + frame_count * 8 + frame_count * 23040];
        let img_offset = 16;
        let mut img_header = [0u8; 18];
        img_header[..7].copy_from_slice(b"r1.PICR");
        data[img_offset..img_offset + 18].copy_from_slice(&img_header);
        let frms_offset = img_offset + 18;
        let pict_offset = frms_offset + frame_count * 8;
        for frame in 0..frame_count {
            let frame_base = frms_offset + frame * 8;
            let top = (frame as i16) * 2;
            let bottom = top + 2;
            data[frame_base..frame_base + 8].copy_from_slice(&[
                0,
                top as u8,
                0,
                0,
                0,
                bottom as u8,
                0,
                0,
            ]);
            let pict_base = pict_offset + frame * 23040;
            data[pict_base..pict_base + 2].copy_from_slice(&(23040u16).to_be_bytes());
            data[pict_base + 2..pict_base + 24].fill(frame as u8);
        }

        let mut qdir = Vec::new();
        qdir.extend_from_slice(&qdir_record(
            b"#Img",
            1000,
            18,
            img_offset as u32,
            b"ColorSwatches.PICR",
        ));
        qdir.extend_from_slice(&qdir_record(
            b"frms",
            1000,
            (frame_count * 8) as u32,
            frms_offset as u32,
            b"ColorSwatches.PICR",
        ));
        qdir.extend_from_slice(&qdir_record(
            b"PICT",
            1000,
            (frame_count * 23040) as u32,
            pict_offset as u32,
            b"ColorSwatches.PICR",
        ));
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir,
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        let pict_type = u32::from_be_bytes(*b"PICT");
        let picts = loaded
            .vfs_resources
            .iter()
            .filter(|resource| resource.res_type == pict_type)
            .collect::<Vec<_>>();
        assert_eq!(picts.len(), frame_count);
        assert_eq!(picts[0].res_id, 1000);
        assert_eq!(
            picts[frame_count - 1].res_id,
            1000 + (frame_count as i16 - 1)
        );
    }

    #[test]
    fn hle_import_runner_open_res_file_infers_compact_quilt_frames_without_img_header() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b":Game Data:BaseGuage.PICR");

        let frame_count = 5usize;
        let height = 16usize;
        let width = 76usize;
        let row_bytes = 80usize;
        let frms_offset = 16;
        let pict_offset = frms_offset + frame_count * 8;
        let mut data = vec![0u8; pict_offset + frame_count * height * row_bytes];
        for frame in 0..frame_count {
            let frame_base = frms_offset + frame * 8;
            data[frame_base..frame_base + 8]
                .copy_from_slice(&[0, 0, 0, 0, 0, height as u8, 0, width as u8]);
            let pict_base = pict_offset + frame * height * row_bytes;
            for row in 0..height {
                let row_start = pict_base + row * row_bytes;
                data[row_start..row_start + width]
                    .fill((frame * height + row + 1) as u8);
                data[row_start + width..row_start + row_bytes].fill(0xa0 + frame as u8);
            }
            data[pict_base] = 0;
        }

        let mut qdir = Vec::new();
        qdir.extend_from_slice(&qdir_record(
            b"frms",
            1000,
            (frame_count * 8) as u32,
            frms_offset as u32,
            b"BaseGuage.PICR",
        ));
        qdir.extend_from_slice(&qdir_record(
            b"PICT",
            1000,
            (frame_count * height * row_bytes) as u32,
            pict_offset as u32,
            b"BaseGuage.PICR",
        ));
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir,
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_FIRST_FILE_REF_NUM as u16 as u32);
        let pixels = loaded.heap_cursor().saturating_add(0x10000);
        let pict_type = u32::from_be_bytes(*b"PICT");
        let picts = loaded
            .vfs_resources
            .iter()
            .filter(|resource| resource.res_type == pict_type)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(picts.len(), frame_count);
        let frame_rects = loaded
            .vfs_resources
            .iter()
            .filter(|resource| resource.res_type == u32::from_be_bytes(*b"frms"))
            .collect::<Vec<_>>();
        assert_eq!(frame_rects.len(), frame_count);
        for (frame, frame_rect) in frame_rects.into_iter().enumerate() {
            assert_eq!(frame_rect.res_id, 1000 + frame as i16);
            assert_eq!(
                frame_rect.data,
                [0, 0, 0, 0, 0, height as u8, 0, width as u8]
            );
        }
        for (frame, pict) in picts.iter().enumerate() {
            assert_eq!(pict.res_id, 1000 + frame as i16);
            assert_eq!(pict.data.len(), 24 + height * row_bytes);
            assert_eq!(u16::from_be_bytes([pict.data[0], pict.data[1]]), 1304);
            assert_eq!(
                &pict.data[2..10],
                &[0, 0, 0, 0, 0, height as u8, 0, width as u8]
            );
            assert_eq!(&pict.data[10..24], &[0; 14]);
            for row in 0..height {
                let row_start = 24 + row * row_bytes;
                let mut expected = vec![(frame * height + row + 1) as u8; width];
                if row == 0 {
                    expected[0] = 0;
                }
                assert_eq!(
                    &pict.data[row_start..row_start + width],
                    expected
                );
                assert_eq!(
                    &pict.data[row_start + width..row_start + row_bytes],
                    &[0xa0 + frame as u8; 4]
                );
            }
        }
        let img = loaded
            .vfs_resources
            .iter()
            .find(|resource| resource.res_type == u32::from_be_bytes(*b"#Img"))
            .expect("synthesized image header");
        assert_eq!(u16::from_be_bytes([img.data[6], img.data[7]]), 5);
        assert_eq!(u16::from_be_bytes([img.data[10], img.data[11]]), 1);

        let front_buffer = PpcFrontBuffer {
            base_addr: pixels,
            row_bytes: row_bytes as u32,
            width: row_bytes as u32,
            height: height as u32,
            depth: 8,
        };
        for (frame, pict) in picts.into_iter().enumerate() {
            let zero_is_opaque = ppc_quilt_picture_zero_is_opaque(
                &loaded.vfs_resources,
                pict.handle,
            );
            assert!(!zero_is_opaque);
            for offset in 0..height * row_bytes {
                assert!(loaded
                    .memory
                    .write_u8(pixels + offset as u32, 0xee)
                    .is_some());
            }
            assert!(ppc_draw_pict_bytes_to_16bpp(
                &mut loaded.memory,
                front_buffer,
                &pict.data,
                (0, 0, height as i16, width as i16),
                &loaded.screen_clut,
                0,
                zero_is_opaque,
            ));
            assert_eq!(loaded.memory.read_u8(pixels), Some(0));
            for row in 0..height {
                for column in 0..width {
                    let expected = if row == 0 && column == 0 {
                        0
                    } else {
                        (frame * height + row + 1) as u8
                    };
                    assert_eq!(
                        loaded
                            .memory
                            .read_u8(pixels + (row * row_bytes + column) as u32),
                        Some(expected)
                    );
                }
                for padding in width..row_bytes {
                    assert_eq!(
                        loaded
                            .memory
                            .read_u8(pixels + (row * row_bytes + padding) as u32),
                        Some(0xee)
                    );
                }
            }
        }
    }

    #[test]
    fn hle_import_runner_open_res_file_synthesizes_missing_quilt_img_header() {
        fn qdir_record(
            res_type: &[u8; 4],
            id: u32,
            len: u32,
            offset: u32,
            name: &[u8],
        ) -> [u8; 60] {
            let mut record = [0u8; 60];
            record[0..4].copy_from_slice(res_type);
            record[4..8].copy_from_slice(&id.to_be_bytes());
            record[8..12].copy_from_slice(&len.to_be_bytes());
            record[12..16].copy_from_slice(&offset.to_be_bytes());
            record[24..26].copy_from_slice(&1u16.to_be_bytes());
            record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
            record[28..28 + name.len()].copy_from_slice(name);
            record
        }

        let pef = synthetic_pef_with_import(b"OpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 128]);
        write_ppc_pstring(
            &mut loaded.memory,
            name_ptr,
            b":Game Data:Graphics:Common:Choices Background.PICR",
        );
        let mut data = vec![0u8; 128];
        data[16..26].copy_from_slice(b"background");
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir_record(b"PICT", 1000, 8, 16, b"Choices Background.PICR").to_vec(),
            attrs: 0,
        }])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            data: data.into(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "Game Data/Packed Bits".to_string(),
            creator: u32::from_be_bytes(*b"Game"),
            file_type: u32::from_be_bytes(*b"bits"),
            finder_flags: 0,
            resource_len: raw_fork.len() as u32,
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.resource_files.len(), 1);
        assert!(loaded.process_file_system.vfs_resources.iter().any(|resource| {
            resource.path == "Game Data/Graphics/Common/Choices Background.PICR"
                && resource.res_type == u32::from_be_bytes(*b"#Img")
                && resource.res_id == 1000
                && resource.data.len() == 18
        }));
    }

    #[test]
    fn hle_import_runner_open_res_file_wraps_large_raw_quilt_pict_payloads() {
        let height = 460usize;
        let row_bytes = 656usize;
        let mut resources = vec![
            PpcVfsResourceRecord {
                ref_num: 0,
                path: "Game Data/Graphics/Common/New Game Options.PICR".to_string(),
                res_type: u32::from_be_bytes(*b"frms"),
                res_id: 1000,
                name: b"New Game Options.PICR".to_vec(),
                data: [0u8, 0, 0, 0, 1, 204, 2, 128].to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
            PpcVfsResourceRecord {
                ref_num: 0,
                path: "Game Data/Graphics/Common/New Game Options.PICR".to_string(),
                res_type: u32::from_be_bytes(*b"PICT"),
                res_id: 1000,
                name: b"New Game Options.PICR".to_vec(),
                data: {
                    let mut data = vec![0u8; height * row_bytes];
                    data[0] = 0x11;
                    data[1] = 0x22;
                    data[row_bytes + 1] = 0x33;
                    data[row_bytes + 2] = 0x44;
                    data
                },
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
        ];

        ppc_wrap_quilt_raw_pict_frames(&mut resources);

        let pict = resources
            .iter()
            .find(|resource| resource.res_type == u32::from_be_bytes(*b"PICT"))
            .expect("wrapped PICT resource");
        assert_eq!(pict.data.len(), 24 + height * row_bytes);
        assert_eq!(pict.data[0..2], u16::MAX.to_be_bytes());
        assert_eq!(&pict.data[2..10], &[0, 0, 0, 0, 1, 204, 2, 128]);
        assert_eq!(&pict.data[10..24], &[0; 14]);
        assert_eq!(pict.data[24], 0x11);
        assert_eq!(pict.data[25], 0x22);
        assert_eq!(pict.data[24 + row_bytes + 1], 0x33);
        assert_eq!(pict.data[24 + row_bytes + 2], 0x44);
    }

    #[test]
    fn ppc_vfs_resource_index_ignores_closed_path_backed_resources() {
        let img_type = u32::from_be_bytes(*b"#Img");
        let resources = vec![
            PpcVfsResourceRecord {
                ref_num: PPC_CLOSED_RESOURCE_REF_NUM,
                path: "Closed/Sprite.PICR".to_string(),
                res_type: img_type,
                res_id: 1000,
                name: Vec::new(),
                data: b"closed".to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
            PpcVfsResourceRecord {
                ref_num: 0,
                path: "Application".to_string(),
                res_type: img_type,
                res_id: 1000,
                name: Vec::new(),
                data: b"app".to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
        ];

        let index = ppc_vfs_resource_index(&resources, 200, img_type, 1000, false).unwrap();
        assert_eq!(index, 1);
        assert_eq!(
            ppc_vfs_resource_index(&resources[0..1], 200, img_type, 1000, false),
            None
        );
    }

    #[test]
    fn hle_import_runner_handles_get1_ind_resource() {
        let pef = synthetic_pef_with_import(b"Get1IndResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.set_current_resource_refnum(PPC_FIRST_FILE_REF_NUM);
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "Game Control".to_string(),
            res_type: u32::from_be_bytes(*b"GCtl"),
            res_id: 10,
            name: Vec::new(),
            data: b"first".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM + 1,
            path: "Other".to_string(),
            res_type: u32::from_be_bytes(*b"GCtl"),
            res_id: 20,
            name: Vec::new(),
            data: b"other".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "Game Control".to_string(),
            res_type: u32::from_be_bytes(*b"GCtl"),
            res_id: 30,
            name: Vec::new(),
            data: b"second".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"GCtl");
        loaded.cpu.gpr[4] = 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let handle = loaded.cpu.gpr[3];
        assert_ne!(handle, 0);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[2].handle, handle);
        assert_eq!(
            loaded.memory.read_u32_be(handle),
            Some(PPC_HEAP_BASE + PPC_HEAP_ALIGNMENT)
        );
        assert_eq!(
            loaded.memory.read_u8(PPC_HEAP_BASE + PPC_HEAP_ALIGNMENT),
            Some(b's')
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"GCtl");
        loaded.cpu.gpr[4] = 3;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetIndResource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"GCtl");
        loaded.cpu.gpr[4] = 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_ne!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.process_file_system.vfs_resources[1].handle, loaded.cpu.gpr[3]);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    }

    #[test]
    fn hle_import_runner_fsp_open_res_file_materializes_raw_resource_fork() {
        let pef = synthetic_pef_with_import(b"FSpOpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let spec_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(spec_ptr, vec![0; 70]);
        write_ppc_fsspec(
            &mut loaded.memory,
            spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_ROOT_DIR_ID,
            b"rex.skeleton",
        );
        let alias_data =
            ppc_alias_record_bytes(PPC_BOOT_VOLUME_REF_NUM, 13720, b"rex.3df").unwrap();
        let raw_fork = serialize_resource_fork(&[
            ResourceForkEntry {
                res_type: *b"alis",
                id: 1000,
                name: Vec::new(),
                data: alias_data.clone(),
                attrs: 0,
            },
            ResourceForkEntry {
                res_type: *b"Hedr",
                id: 1000,
                name: Vec::new(),
                data: b"header".to_vec(),
                attrs: 0,
            },
        ])
        .unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "rex.skeleton".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"BIOp"),
            file_type: u32::from_be_bytes(*b"SkeP"),
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "rex.skeleton".to_string(),
            creator: u32::from_be_bytes(*b"BIOp"),
            file_type: u32::from_be_bytes(*b"SkeP"),
            finder_flags: 0,
            resource_len: u32::try_from(raw_fork.len()).unwrap(),
            raw_data: Some(raw_fork.into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "rex.skeleton".to_string(),
            res_type: u32::from_be_bytes(*b"alis"),
            res_id: 1000,
            name: Vec::new(),
            data: alias_data,
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FIRST_FILE_REF_NUM));
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(loaded.resource_files[0].path, "rex.skeleton");
        assert_eq!(loaded.process_file_system.vfs_resources.len(), 2);
        assert_eq!(loaded.process_file_system.vfs_resources[0].ref_num, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(loaded.process_file_system.vfs_resources[1].ref_num, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(
            loaded.process_file_system.vfs_resources[1].res_type,
            u32::from_be_bytes(*b"Hedr")
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1Resource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"Hedr");
        loaded.cpu.gpr[4] = 1000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_ne!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    }

    #[test]
    fn hle_import_runner_reopens_path_backed_ppc_resource_forks() {
        let pef = synthetic_pef_with_import(b"FSpCreateResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let spec_ptr = PPC_DATA_BASE + 0x1000;
        let data_ptr = PPC_DATA_BASE + 0x1100;
        let handle = PPC_DATA_BASE + 0x1140;
        let name_ptr = PPC_DATA_BASE + 0x1150;
        loaded.memory.add_region(spec_ptr, vec![0; 0x200]);
        write_ppc_fsspec(
            &mut loaded.memory,
            spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App HighScores",
        );
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"Nano");
        loaded.cpu.gpr[5] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[6] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.vfs_resource_files.len(), 1);
        assert_eq!(
            loaded.vfs_resource_files[0].path,
            "System Folder/Preferences/Test App HighScores"
        );
        assert_eq!(
            loaded.vfs_resource_files[0].creator,
            u32::from_be_bytes(*b"Nano")
        );
        assert_eq!(
            loaded.vfs_resource_files[0].file_type,
            u32::from_be_bytes(*b"pref")
        );
        assert_eq!(loaded.vfs_files.len(), 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpOpenResFile;
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 3; // fsRdWrPerm

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FIRST_FILE_REF_NUM));
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(loaded.resource_files.len(), 1);

        for (offset, byte) in b"score".iter().copied().enumerate() {
            loaded
                .memory
                .write_u8(data_ptr + offset as u32, byte)
                .unwrap();
        }
        loaded.memory.write_u32_be(handle, data_ptr).unwrap();
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Scores");
        test_handles!(loaded).push(PpcHandleRecord {
            handle,
            ptr: data_ptr,
            size: 5,
            capacity: 5,
        });
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AddResource;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[5] = 200;
        loaded.cpu.gpr[6] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources.len(), 1);
        assert_eq!(loaded.process_file_system.vfs_resources[0].ref_num, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].path,
            "System Folder/Preferences/Test App HighScores"
        );
        assert_eq!(loaded.process_file_system.vfs_resources[0].data, b"score");

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::CloseResFile;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert!(loaded.resource_files.is_empty());
        assert_eq!(*loaded.process_file_system.current_resource_file, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpOpenResFile;
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_FIRST_FILE_REF_NUM + 1)
        );
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM + 1);
        assert_eq!(loaded.process_file_system.vfs_resources[0].ref_num, PPC_FIRST_FILE_REF_NUM + 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1Resource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[4] = 200;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], handle);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    }

    #[test]
    fn hle_import_runner_updates_open_empty_path_backed_resource_file() {
        let pef = synthetic_pef_with_import(b"FSpOpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let first_spec_ptr = PPC_DATA_BASE + 0x1000;
        let second_spec_ptr = PPC_DATA_BASE + 0x1080;
        loaded.memory.add_region(first_spec_ptr, vec![0; 0x120]);
        write_ppc_fsspec(
            &mut loaded.memory,
            first_spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        write_ppc_fsspec(
            &mut loaded.memory,
            second_spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App HighScores",
        );
        loaded.cpu.gpr[3] = first_spec_ptr;
        loaded.cpu.gpr[4] = 3;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1));
        assert_eq!(loaded.test_resource_error(), PPC_FNF_ERR);
        assert!(loaded.resource_files.is_empty());
        assert!(loaded.vfs_resource_files.is_empty());

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpCreateResFile;
        loaded.cpu.gpr[3] = first_spec_ptr;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"Nano");
        loaded.cpu.gpr[5] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[6] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.vfs_resource_files.len(), 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpOpenResFile;
        loaded.cpu.gpr[3] = first_spec_ptr;
        loaded.cpu.gpr[4] = 3;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FIRST_FILE_REF_NUM));
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM);
        assert_eq!(loaded.resource_files.len(), 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpCreateResFile;
        loaded.cpu.gpr[3] = second_spec_ptr;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"Nano");
        loaded.cpu.gpr[5] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[6] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpOpenResFile;
        loaded.cpu.gpr[3] = second_spec_ptr;
        loaded.cpu.gpr[4] = 3;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.cpu.gpr[3],
            ppc_i16_result(PPC_FIRST_FILE_REF_NUM + 1)
        );
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM + 1);
        assert_eq!(loaded.resource_files.len(), 2);
        assert!(loaded.process_file_system.vfs_resources.is_empty());

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateResFile;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.set_test_resource_error(PPC_RES_F_NOT_FOUND_ERR);

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(*loaded.process_file_system.current_resource_file, PPC_FIRST_FILE_REF_NUM + 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = (PPC_FIRST_FILE_REF_NUM + 2) as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_RES_F_NOT_FOUND_ERR);
    }

    #[test]
    fn hle_import_runner_open_res_file_rejects_existing_data_file_without_resource_fork() {
        let pef = synthetic_pef_with_import(b"FSpOpenResFile");
        let mut loaded = load_pef_application(&pef).unwrap();
        let spec_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(spec_ptr, vec![0; 0x80]);
        write_ppc_fsspec(
            &mut loaded.memory,
            spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Gridz Preferences",
        );
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Gridz Preferences".to_string(),
            data: (Vec::new()).into(),
            creator: u32::from_be_bytes(*b"Grid"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0400,
            dirty: false,
        });
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 3;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1));
        assert_eq!(loaded.test_resource_error(), PPC_RES_F_NOT_FOUND_ERR);
        assert!(loaded.resource_files.is_empty());
        assert!(loaded.vfs_resource_files.is_empty());
    }

    #[test]
    fn hle_import_runner_handles_fs_read_zero_backed_file() {
        let pef = synthetic_pef_with_import(b"FSRead");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let count_ptr = scratch;
        let buffer_ptr = scratch + 4;
        loaded.memory.add_region(scratch, vec![0xaa; 16]);
        loaded.push_test_open_file(PpcFileRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            position: 0,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (vec![0, 1, 2, 3]).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.memory.write_u32_be(count_ptr, 4).unwrap();
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = count_ptr;
        loaded.cpu.gpr[5] = buffer_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(count_ptr), Some(4));
        for offset in 0..4 {
            assert_eq!(
                loaded.memory.read_u8(buffer_ptr + offset),
                Some(offset as u8)
            );
        }
        assert_eq!(loaded.files[0].position, 4);
    }

    #[test]
    fn hle_import_runner_fs_read_output_is_all_or_nothing() {
        let pef = synthetic_pef_with_import(b"FSRead");
        let mut loaded = load_pef_application(&pef).unwrap();
        let count_ptr = PPC_DATA_BASE + 0x1000;
        let buffer_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(count_ptr, vec![0; 4]);
        loaded.memory.add_region(buffer_ptr, vec![0xbb; 2]);
        loaded.push_test_open_file(PpcFileRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            position: 0,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"data".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.memory.write_u32_be(count_ptr, 4).unwrap();
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = count_ptr;
        loaded.cpu.gpr[5] = buffer_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u32_be(count_ptr), Some(4));
        assert_eq!(loaded.memory.read_u16_be(buffer_ptr), Some(0xbbbb));
        assert_eq!(loaded.files[0].position, 0);
    }

    #[test]
    fn hle_import_runner_tracks_ppc_file_position_and_set_eof() {
        let pef = synthetic_pef_with_import(b"SetFPos");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        let pos_out_ptr = scratch;
        loaded.memory.add_region(scratch, vec![0xaa; 16]);
        loaded.push_test_open_file(PpcFileRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            position: 0,
        });
        loaded
            .process_file_system
            .writable_refnums
            .insert(PPC_FIRST_FILE_REF_NUM as u16);
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"abcdef".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 1; // fsFromStart
        loaded.cpu.gpr[5] = 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.files[0].position, 2);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetFPos;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = pos_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(pos_out_ptr), Some(2));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetFPos;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 2; // fsFromLEOF
        loaded.cpu.gpr[5] = (-1i32) as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.files[0].position, 5);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 3; // fsFromMark
        loaded.cpu.gpr[5] = (-2i32) as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.files[0].position, 3);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetEOF;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_files[0].data, b"ab");
        assert!(loaded.vfs_files[0].dirty);
        assert_eq!(loaded.files[0].position, 2);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 5;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_files[0].data, b"ab\0\0\0");
        assert_eq!(loaded.files[0].position, 2);
        let exports = loaded.take_dirty_vfs_files();
        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].data, b"ab\0\0\0");
    }

    #[test]
    fn hle_import_runner_reports_bad_refnum_for_ppc_file_position_calls() {
        let pef = synthetic_pef_with_import(b"GetFPos");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0xaa; 16]);
        loaded.memory.write_u32_be(scratch, 0xdead_beef).unwrap();
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = scratch;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_RF_NUM_ERR));
        assert_eq!(loaded.memory.read_u32_be(scratch), Some(0xdead_beef));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetFPos;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_RF_NUM_ERR));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetEOF;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_RF_NUM_ERR));
    }

    #[test]
    fn hle_import_runner_fs_write_source_is_all_or_nothing() {
        let pef = synthetic_pef_with_import(b"FSWrite");
        let mut loaded = load_pef_application(&pef).unwrap();
        let count_ptr = PPC_DATA_BASE + 0x1000;
        let buffer_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(count_ptr, vec![0; 4]);
        loaded.memory.add_region(buffer_ptr, vec![b'x', b'y']);
        loaded.push_test_open_file(PpcFileRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            position: 1,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"old".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.memory.write_u32_be(count_ptr, 4).unwrap();
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = count_ptr;
        loaded.cpu.gpr[5] = buffer_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u32_be(count_ptr), Some(4));
        assert_eq!(loaded.vfs_files[0].data, b"old");
        assert!(!loaded.vfs_files[0].dirty);
        assert_eq!(loaded.files[0].position, 1);
    }

    #[test]
    fn hle_import_runner_persists_ppc_data_fork_writes_for_prefs_files() {
        let pef = synthetic_pef_with_import(b"FSpCreate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let spec_ptr = PPC_DATA_BASE + 0x1000;
        let ref_num_out_ptr = PPC_DATA_BASE + 0x1080;
        let count_ptr = PPC_DATA_BASE + 0x1084;
        let buffer_ptr = PPC_DATA_BASE + 0x1090;
        loaded.memory.add_region(spec_ptr, vec![0; 0x120]);
        write_ppc_fsspec(
            &mut loaded.memory,
            spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            PPC_PREFERENCES_DIR_ID,
            b"Test App Prefs",
        );
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"Nano");
        loaded.cpu.gpr[5] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[6] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.vfs_files.len(), 1);
        assert_eq!(loaded.vfs_files[0].creator, u32::from_be_bytes(*b"Nano"));
        assert_eq!(loaded.vfs_files[0].file_type, u32::from_be_bytes(*b"pref"));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSMakeFSSpec;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_PREFERENCES_DIR_ID;
        loaded.cpu.gpr[5] = spec_ptr + 6;
        loaded.cpu.gpr[6] = spec_ptr + 0x40;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpOpenDF;
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 3; // fsRdWrPerm
        loaded.cpu.gpr[5] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let ref_num = loaded.memory.read_u16_be(ref_num_out_ptr).unwrap() as i16;
        assert!(loaded
            .process_file_system
            .writable_refnums
            .contains(&(ref_num as u16)));

        for (offset, byte) in b"prefs".iter().copied().enumerate() {
            loaded
                .memory
                .write_u8(buffer_ptr + offset as u32, byte)
                .unwrap();
        }
        loaded.memory.write_u32_be(count_ptr, 5).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSWrite;
        loaded.cpu.gpr[3] = ref_num as u16 as u32;
        loaded.cpu.gpr[4] = count_ptr;
        loaded.cpu.gpr[5] = buffer_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(count_ptr), Some(5));
        assert_eq!(loaded.vfs_files[0].data, b"prefs");

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetEOF;
        loaded.cpu.gpr[3] = ref_num as u16 as u32;
        loaded.cpu.gpr[4] = count_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(count_ptr), Some(5));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSClose;
        loaded.cpu.gpr[3] = ref_num as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.files.len(), 0);
        assert!(!loaded
            .process_file_system
            .writable_refnums
            .contains(&(ref_num as u16)));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpOpenDF;
        loaded.cpu.gpr[3] = spec_ptr;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = ref_num_out_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let read_ref_num = loaded.memory.read_u16_be(ref_num_out_ptr).unwrap() as i16;
        loaded.memory.write_u32_be(count_ptr, 5).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSRead;
        loaded.cpu.gpr[3] = read_ref_num as u16 as u32;
        loaded.cpu.gpr[4] = count_ptr;
        loaded.cpu.gpr[5] = buffer_ptr + 16;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        for (offset, byte) in b"prefs".iter().copied().enumerate() {
            assert_eq!(
                loaded.memory.read_u8(buffer_ptr + 16 + offset as u32),
                Some(byte)
            );
        }

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::FSpDelete;
        loaded.cpu.gpr[3] = spec_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert!(loaded.vfs_files.is_empty());
        assert!(loaded.files.is_empty());
        assert_eq!(
            loaded.take_deleted_vfs_file_paths(),
            vec!["System Folder/Preferences/Test App Prefs".to_string()]
        );
        assert!(loaded.take_deleted_vfs_file_paths().is_empty());
    }

    #[test]
    fn hle_import_runner_persists_ppc_resource_writes_for_pref_resources() {
        let pef = synthetic_pef_with_import(b"AddResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x2000;
        let data_ptr = scratch;
        let handle = scratch + 0x40;
        let name_ptr = scratch + 0x50;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        for (offset, byte) in b"old".iter().copied().enumerate() {
            loaded
                .memory
                .write_u8(data_ptr + offset as u32, byte)
                .unwrap();
        }
        loaded.memory.write_u32_be(handle, data_ptr).unwrap();
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Scores");
        test_handles!(loaded).push(PpcHandleRecord {
            handle,
            ptr: data_ptr,
            size: 3,
            capacity: 3,
        });
        loaded.set_current_resource_refnum(PPC_FIRST_FILE_REF_NUM);
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[5] = 100;
        loaded.cpu.gpr[6] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            loaded.process_file_system.vfs_resources,
            vec![PpcVfsResourceRecord {
                ref_num: PPC_FIRST_FILE_REF_NUM,
                path: String::new(),
                res_type: u32::from_be_bytes(*b"pref"),
                res_id: 100,
                name: b"Scores".to_vec(),
                data: b"old".to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: PPC_RES_CHANGED_ATTR,
                handle,
            }]
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::WriteResource;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].data, b"old");
        assert_eq!(loaded.process_file_system.vfs_resources[0].attrs & PPC_RES_CHANGED_ATTR, 0);

        for (offset, byte) in b"new!".iter().copied().enumerate() {
            loaded
                .memory
                .write_u8(data_ptr + offset as u32, byte)
                .unwrap();
        }
        test_handles!(loaded)[0].size = 4;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ChangedResource;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_ne!(loaded.process_file_system.vfs_resources[0].attrs & PPC_RES_CHANGED_ATTR, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::WriteResource;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].data, b"new!");
        assert_eq!(loaded.process_file_system.vfs_resources[0].attrs & PPC_RES_CHANGED_ATTR, 0);

        for (offset, byte) in b"sync".iter().copied().enumerate() {
            loaded
                .memory
                .write_u8(data_ptr + offset as u32, byte)
                .unwrap();
        }
        test_handles!(loaded)[0].size = 4;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ChangedResource;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_ne!(loaded.process_file_system.vfs_resources[0].attrs & PPC_RES_CHANGED_ATTR, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateResFile;
        loaded.cpu.gpr[3] = PPC_FIRST_FILE_REF_NUM as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].data, b"sync");
        assert_eq!(loaded.process_file_system.vfs_resources[0].attrs & PPC_RES_CHANGED_ATTR, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::RemoveResource;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert!(loaded.process_file_system.vfs_resources.is_empty());
    }

    #[test]
    fn hle_import_runner_reads_back_ppc_vfs_resources() {
        let pef = synthetic_pef_with_import(b"GetResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x2100;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        let current_handle = scratch + 0x40;
        let fallback_handle = scratch + 0x50;
        loaded.set_current_resource_refnum(5);
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 7,
            path: String::new(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 100,
            name: b"Fallback".to_vec(),
            data: b"other".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: fallback_handle,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 5,
            path: String::new(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 100,
            name: b"Scores".to_vec(),
            data: b"current".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: PPC_RES_CHANGED_ATTR,
            handle: current_handle,
        });
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[4] = 100;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], current_handle);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1Resource;
        loaded.set_current_resource_refnum(9);
        loaded.set_test_resource_error(PPC_RES_NOT_FOUND_ERR);
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[4] = 100;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetResource;
        loaded.set_current_resource_refnum(9);
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[4] = 100;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], fallback_handle);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetResAttrs;
        loaded.cpu.gpr[3] = current_handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_RES_CHANGED_ATTR as u32);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HomeResFile;
        loaded.cpu.gpr[3] = current_handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 5);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetResInfo;
        let id_ptr = scratch;
        let type_ptr = scratch + 4;
        let name_ptr = scratch + 8;
        loaded.cpu.gpr[3] = current_handle;
        loaded.cpu.gpr[4] = id_ptr;
        loaded.cpu.gpr[5] = type_ptr;
        loaded.cpu.gpr[6] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.memory.read_u16_be(id_ptr), Some(100));
        assert_eq!(
            loaded.memory.read_u32_be(type_ptr),
            Some(u32::from_be_bytes(*b"pref"))
        );
        assert_eq!(loaded.memory.read_u8(name_ptr), Some(6));
        for (offset, byte) in b"Scores".iter().copied().enumerate() {
            assert_eq!(
                loaded.memory.read_u8(name_ptr + 1 + offset as u32),
                Some(byte)
            );
        }
    }

    #[test]
    fn hle_import_runner_handles_get_ind_string() {
        let pef = synthetic_pef_with_import(b"GetIndString");
        let mut loaded = load_pef_application(&pef).unwrap();
        let output = PPC_DATA_BASE + 0x2100;
        loaded.memory.add_region(output, vec![0xcc; 0x200]);
        loaded.set_current_resource_refnum(5);
        loaded.set_test_resource_error(PPC_RES_NOT_FOUND_ERR);
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 7,
            path: "fallback".to_string(),
            res_type: u32::from_be_bytes(*b"STR#"),
            res_id: 128,
            name: Vec::new(),
            data: [0, 1, 8, b'f', b'a', b'l', b'l', b'b', b'a', b'c', b'k'].to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 5,
            path: "current".to_string(),
            res_type: u32::from_be_bytes(*b"STR#"),
            res_id: 128,
            name: Vec::new(),
            data: [0, 2, 3, b'o', b'n', b'e', 3, b't', b'w', b'o'].to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = output;
        loaded.cpu.gpr[4] = 128;
        loaded.cpu.gpr[5] = 2;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, output).as_deref(),
            Some(&b"two"[..])
        );
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_ne!(loaded.process_file_system.vfs_resources[1].handle, 0);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.set_test_resource_error(PPC_RES_ATTR_ERR);
        loaded.cpu.gpr[3] = output;
        loaded.cpu.gpr[4] = 128;
        loaded.cpu.gpr[5] = 3;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(output), Some(0));
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.set_test_resource_error(PPC_RES_ATTR_ERR);
        loaded.cpu.gpr[3] = output;
        loaded.cpu.gpr[4] = 404;
        loaded.cpu.gpr[5] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u8(output), Some(0));
        assert_eq!(loaded.test_resource_error(), PPC_RES_ATTR_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.set_test_resource_error(PPC_NO_ERR);
        loaded.cpu.gpr[3] = PPC_DATA_BASE + 0x5000;
        loaded.cpu.gpr[4] = 128;
        loaded.cpu.gpr[5] = 1;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_PARAM_ERR);
    }

    #[test]
    fn hle_import_runner_get_string_returns_str_resource_handle() {
        let pef = synthetic_pef_with_import(b"GetString");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.set_current_resource_refnum(5);
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 5,
            path: "Application".to_string(),
            res_type: u32::from_be_bytes(*b"STR "),
            res_id: 128,
            name: Vec::new(),
            data: [5, b'H', b'e', b'l', b'l', b'o'].to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = 128;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let handle = loaded.cpu.gpr[3];
        assert_ne!(handle, 0);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);
        let pointer = loaded.memory.read_u32_be(handle).unwrap();
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, pointer).as_deref(),
            Some(&b"Hello"[..])
        );
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    }

    #[test]
    fn hle_import_runner_get_string_synthesizes_system_owner_name() {
        let pef = synthetic_pef_with_import(b"GetString");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.set_current_resource_refnum(5);
        loaded.cpu.gpr[3] = (-16096i16) as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let handle = loaded.cpu.gpr[3];
        assert_ne!(handle, 0);
        let pointer = loaded.memory.read_u32_be(handle).unwrap();
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, pointer).as_deref(),
            Some(&b"Macintosh User"[..])
        );
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources.len(), 1);
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].path,
            "__system__/STR "
        );
        assert_eq!(loaded.process_file_system.vfs_resources[0].data.len(), 32);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1Resource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        loaded.cpu.gpr[4] = (-16413i16) as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.process_file_system.vfs_resources.len(), 1);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    }

    #[test]
    fn hle_import_runner_get_res_info_outputs_are_all_or_nothing() {
        let pef = synthetic_pef_with_import(b"GetResInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x2100;
        let handle = scratch + 0x40;
        let id_ptr = scratch;
        let type_ptr = scratch + 4;
        let name_ptr = scratch + 8;
        loaded.memory.add_region(scratch, vec![0xcc; 12]);
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 5,
            path: String::new(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 100,
            name: b"Scores".to_vec(),
            data: b"current".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: PPC_RES_CHANGED_ATTR,
            handle,
        });
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = id_ptr;
        loaded.cpu.gpr[5] = type_ptr;
        loaded.cpu.gpr[6] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_PARAM_ERR);
        assert_eq!(loaded.memory.read_u16_be(id_ptr), Some(0xcccc));
        assert_eq!(loaded.memory.read_u32_be(type_ptr), Some(0xcccc_cccc));
        assert_eq!(loaded.memory.read_u32_be(name_ptr), Some(0xcccc_cccc));
    }

    #[test]
    fn hle_import_runner_materializes_seeded_ppc_resource_bytes_lazily() {
        let pef = synthetic_pef_with_import(b"GetResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 42,
            name: b"Prefs".to_vec(),
            data: b"seeded".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[4] = 42;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let handle = loaded.cpu.gpr[3];
        assert_ne!(handle, 0);
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);
        assert_eq!(test_handle_records!(loaded).len(), 1);
        let ptr = loaded.memory.read_u32_be(handle).unwrap();
        for (offset, byte) in b"seeded".iter().copied().enumerate() {
            assert_eq!(loaded.memory.read_u8(ptr + offset as u32), Some(byte));
        }

        let heap_cursor = loaded.heap_cursor();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[4] = 42;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], handle);
        assert_eq!(test_handle_records!(loaded).len(), 1);
        assert_eq!(loaded.heap_cursor(), heap_cursor);
    }

    #[test]
    fn hle_import_runner_close_res_file_recycles_loaded_resource_handles() {
        let pef = synthetic_pef_with_import(b"Get1Resource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let ref_num = PPC_FIRST_FILE_REF_NUM;
        loaded.set_current_resource_refnum(ref_num);
        loaded.push_resource_file(PpcResourceFileRecord {
            ref_num,
            path: "Frame.PICR".to_string(),
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num,
            path: "Frame.PICR".to_string(),
            res_type: u32::from_be_bytes(*b"PICT"),
            res_id: 1016,
            name: Vec::new(),
            data: vec![0x5a; 11_160],
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"PICT");
        loaded.cpu.gpr[4] = 1016;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        let resource_handle = loaded.cpu.gpr[3];
        let allocated_heap_cursor = loaded.heap_cursor();
        assert_ne!(resource_handle, 0);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, resource_handle);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::CloseResFile;
        loaded.cpu.gpr[3] = ref_num as u16 as u32;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert!(loaded.resource_files.is_empty());
        assert_eq!(*loaded.process_file_system.current_resource_file, 0);
        assert_eq!(loaded.process_file_system.vfs_resources[0].ref_num, PPC_CLOSED_RESOURCE_REF_NUM);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, 0);
        assert_eq!(loaded.memory.read_u32_be(resource_handle), Some(0));
        assert!(!loaded
            .handles()
            .iter()
            .any(|record| record.handle == resource_handle));
        assert!(loaded
            .free_handle_blocks()
            .iter()
            .any(|record| record.handle == resource_handle));
        assert!(!loaded
            .handle_states()
            .iter()
            .any(|record| record.handle == resource_handle));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewHandle { clear: false };
        loaded.cpu.gpr[3] = 11_000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], resource_handle);
        assert_eq!(loaded.heap_cursor(), allocated_heap_cursor);
    }

    #[test]
    fn hle_import_runner_release_resource_invalidates_clean_resource_handle() {
        let pef = synthetic_pef_with_import(b"GetResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"PICT"),
            res_id: 128,
            name: b"Title".to_vec(),
            data: b"seeded".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"PICT");
        loaded.cpu.gpr[4] = 128;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let released_handle = loaded.cpu.gpr[3];
        let released_ptr = loaded.memory.read_u32_be(released_handle).unwrap();
        let allocated_heap_cursor = loaded.heap_cursor();
        assert_ne!(released_handle, 0);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, released_handle);
        assert_eq!(test_handle_records!(loaded).len(), 1);
        loaded.replace_handle_states(vec![PpcHandleStateRecord {
            handle: released_handle,
            locked: true,
            high_locked: true,
            no_purge: true,
            resource: false,
        }]);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ReleaseResource;
        loaded.cpu.gpr[3] = released_handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], released_handle);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, 0);
        assert!(loaded
            .handles()
            .iter()
            .all(|record| record.handle != released_handle));
        assert!(loaded.handle_states().is_empty());
        assert_eq!(loaded.memory.read_u32_be(released_handle), Some(0));
        assert_eq!(loaded.free_handle_blocks().len(), 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetResource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"PICT");
        loaded.cpu.gpr[4] = 128;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let new_handle = loaded.cpu.gpr[3];
        assert_ne!(new_handle, 0);
        assert_eq!(new_handle, released_handle);
        assert_eq!(
            loaded.handle_states(),
            vec![PpcHandleStateRecord {
                handle: new_handle,
                locked: false,
                high_locked: false,
                no_purge: false,
                resource: true,
            }]
        );
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, new_handle);
        assert_eq!(test_handle_records!(loaded).len(), 1);
        assert!(loaded.free_handle_blocks().is_empty());
        assert_eq!(loaded.heap_cursor(), allocated_heap_cursor);
        let new_ptr = loaded.memory.read_u32_be(new_handle).unwrap();
        assert_eq!(new_ptr, released_ptr);
        for (offset, byte) in b"seeded".iter().copied().enumerate() {
            assert_eq!(loaded.memory.read_u8(new_ptr + offset as u32), Some(byte));
        }
    }

    #[test]
    fn hle_import_runner_release_resource_preserves_changed_resource() {
        let pef = synthetic_pef_with_import(b"ReleaseResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            b"changed",
        );
        assert_ne!(handle, 0);
        let ptr = loaded.memory.read_u32_be(handle).unwrap();
        loaded.replace_handle_states(vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: true,
            no_purge: true,
            resource: false,
        }]);
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"PICT"),
            res_id: 128,
            name: b"Title".to_vec(),
            data: b"seeded".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: PPC_RES_CHANGED_ATTR,
            handle,
        });
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);
        assert_eq!(loaded.memory.read_u32_be(handle), Some(ptr));
        assert!(test_handle_records!(loaded)
            .iter()
            .any(|record| record.handle == handle));
        assert_eq!(
            loaded.handle_states(),
            vec![PpcHandleStateRecord {
                handle,
                locked: true,
                high_locked: true,
                no_purge: true,
                resource: false,
            }]
        );
    }

    #[test]
    fn hle_import_runner_detach_resource_keeps_handle_but_removes_resource_binding() {
        let pef = synthetic_pef_with_import(b"DetachResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            b"detached",
        );
        assert_ne!(handle, 0);
        let ptr = loaded.memory.read_u32_be(handle).unwrap();
        loaded
            .process_memory_manager
            .0
            .borrow_mut()
            .set_process_handle_resource(handle, true);
        loaded
            .process_memory_manager
            .0
            .borrow_mut()
            .set_process_handle_purgeable(handle, false);
        assert_eq!(
            loaded
                .process_memory_manager
                .0
                .borrow()
                .state_for_handle(handle),
            Some(0x20)
        );
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"PICT"),
            res_id: 128,
            name: b"Title".to_vec(),
            data: b"seeded".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle,
        });
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, 0);
        assert_eq!(loaded.memory.read_u32_be(handle), Some(ptr));
        assert!(test_handle_records!(loaded)
            .iter()
            .any(|record| record.handle == handle));
        assert_eq!(
            loaded
                .process_memory_manager
                .0
                .borrow()
                .state_for_handle(handle),
            Some(0)
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HGetState;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetResAttrs;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
        assert_eq!(loaded.cpu.gpr[3], PPC_RES_CHANGED_ATTR as u32);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetHandleSize;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 8);
    }

    #[test]
    fn hle_import_runner_dispose_ctable_invalidates_tracked_handle() {
        let pef = synthetic_pef_with_import(b"DisposeCTable");
        let mut loaded = load_pef_application(&pef).unwrap();
        let handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &[0, 0, 0, 1, 0, 0, 0, 0],
        );
        assert_ne!(handle, 0);
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], handle);
        assert!(test_handle_records!(loaded)
            .iter()
            .all(|record| record.handle != handle));
        assert_eq!(loaded.memory.read_u32_be(handle), Some(0));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x8000;
        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(test_handle_records!(loaded).is_empty());
    }

    #[test]
    fn hle_import_runner_resource_lifetime_calls_report_invalid_or_changed_handles() {
        let pef = synthetic_pef_with_import(b"ReleaseResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x4000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);

        let handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            b"changed",
        );
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"PICT"),
            res_id: 128,
            name: b"Title".to_vec(),
            data: b"seeded".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: PPC_RES_CHANGED_ATTR,
            handle,
        });
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DetachResource;
        loaded.cpu.gpr[3] = handle;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x8000;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].handle, handle);
    }

    #[test]
    fn ppc_dirty_resource_fork_export_serializes_and_clears_dirty_state() {
        let pef = synthetic_pef();
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0x8000,
            dirty: true,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: "System Folder/Preferences/Test App HighScores".to_string(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 200,
            name: b"Scores".to_vec(),
            data: b"score".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });

        loaded
            .process_file_system
            .resource_manager
            .with_mut(|resource_manager| {
                ppc_publish_resource_fork_bytes(
                    &mut resource_manager.vfs_resource_files,
                    &resource_manager.vfs_resources,
                    true,
                );
            });
        let published = loaded
            .vfs_resource_files
            .fork("System Folder/Preferences/Test App HighScores")
            .expect("dirty parsed resource fork should publish before host export");
        let published_fork = crate::managers::resource::ResourceFork::parse(published).unwrap();
        assert_eq!(published_fork.get(*b"pref", 200).unwrap().data, b"score");
        assert!(loaded.vfs_resource_files[0].dirty);

        let exports = loaded.take_dirty_vfs_resource_forks();

        assert_eq!(exports.len(), 1);
        assert_eq!(
            exports[0].path,
            "System Folder/Preferences/Test App HighScores"
        );
        assert_eq!(exports[0].creator, u32::from_be_bytes(*b"Nano"));
        assert_eq!(exports[0].file_type, u32::from_be_bytes(*b"pref"));
        assert_eq!(exports[0].finder_flags, 0x0200);
        let fork = crate::managers::resource::ResourceFork::parse(&exports[0].data).unwrap();
        assert_eq!(fork.map_attrs(), 0x8000);
        let resource = fork.get(*b"pref", 200).unwrap();
        assert_eq!(resource.name.as_deref(), Some("Scores"));
        assert_eq!(resource.data, b"score");
        assert_eq!(
            loaded.vfs_resource_files[0].resource_len,
            u32::try_from(exports[0].data.len()).unwrap()
        );
        assert!(!loaded.vfs_resource_files[0].dirty);
        assert!(loaded.take_dirty_vfs_resource_forks().is_empty());
    }

    #[test]
    fn ppc_dirty_resource_fork_export_preserves_raw_archive_resource_bytes() {
        let pef = synthetic_pef();
        let mut loaded = load_pef_application(&pef).unwrap();
        let path = "System Folder/Preferences/Test App HighScores";
        let compressed_data = compressed_resource_bytes(4, &[0x02, b'A', b'B', b'C', b'D', 0xff]);
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: path.to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0x8000,
            dirty: true,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: path.to_string(),
            res_type: u32::from_be_bytes(*b"DATA"),
            res_id: 128,
            name: b"Packed".to_vec(),
            data: b"ABCD".to_vec(),
            raw_data: Some(compressed_data.clone()),
            raw_attrs: Some(0x21),
            attrs: 0x20,
            handle: 0,
        });

        let exports = loaded.take_dirty_vfs_resource_forks();

        assert_eq!(exports.len(), 1);
        let fork = crate::managers::resource::ResourceFork::parse(&exports[0].data).unwrap();
        assert_eq!(fork.map_attrs(), 0x8000);
        let resource = fork.get(*b"DATA", 128).unwrap();
        assert_eq!(resource.data, b"ABCD");
        assert_eq!(
            resource.raw_data.as_deref(),
            Some(compressed_data.as_slice())
        );
        assert_eq!(resource.raw_attrs, Some(0x21));
        assert_eq!(resource.attrs, 0x20);
    }

    #[test]
    fn ppc_dirty_resource_fork_metadata_export_preserves_raw_fork_bytes() {
        let pef = synthetic_pef();
        let mut loaded = load_pef_application(&pef).unwrap();
        let path = "System Folder/Preferences/Test App HighScores";
        let raw_fork = noncanonical_single_resource_fork_bytes(*b"DATA", 128, b"raw", 0x20, 0x4000);
        assert!(crate::managers::resource::ResourceFork::parse(&raw_fork).is_some());
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: path.to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0220,
            resource_len: u32::try_from(raw_fork.len()).unwrap(),
            raw_data: Some(raw_fork.clone().into()),
            map_attrs: 0x4000,
            dirty: true,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: path.to_string(),
            res_type: u32::from_be_bytes(*b"DATA"),
            res_id: 128,
            name: Vec::new(),
            data: b"raw".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0x20,
            handle: 0,
        });

        let exports = loaded.take_dirty_vfs_resource_forks();

        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].data, raw_fork);
        assert_eq!(exports[0].finder_flags, 0x0220);
        assert_eq!(
            loaded.vfs_resource_files[0].resource_len,
            u32::try_from(raw_fork.len()).unwrap()
        );
        assert!(!loaded.vfs_resource_files[0].dirty);
    }

    #[test]
    fn ppc_resource_fork_len_prefers_raw_archive_fork_bytes() {
        let path = "System Folder/Preferences/Test App HighScores";
        let raw_fork = noncanonical_single_resource_fork_bytes(*b"DATA", 128, b"raw", 0x20, 0x4000);
        let deterministic_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"DATA",
            id: 128,
            name: Vec::new(),
            data: b"raw".to_vec(),
            attrs: 0x20,
        }])
        .unwrap();
        assert_ne!(raw_fork.len(), deterministic_fork.len());
        let resource_files = vec![PpcVfsResourceFileRecord {
            path: path.to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            resource_len: u32::try_from(raw_fork.len()).unwrap(),
            raw_data: Some(raw_fork.clone().into()),
            map_attrs: 0x4000,
            dirty: false,
        }];
        let resources = vec![PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: path.to_string(),
            res_type: u32::from_be_bytes(*b"DATA"),
            res_id: 128,
            name: Vec::new(),
            data: b"raw".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0x20,
            handle: 0,
        }];

        assert_eq!(
            ppc_resource_fork_len_for_path(&resource_files, &resources, path),
            u32::try_from(raw_fork.len()).unwrap()
        );
    }

    #[test]
    fn ppc_resource_fork_len_uses_raw_archive_resource_bytes() {
        let path = "System Folder/Preferences/Test App HighScores";
        let compressed_data = compressed_resource_bytes(4, &[0x02, b'A', b'B', b'C', b'D', 0xff]);
        let raw_resource_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"DATA",
            id: 128,
            name: b"Packed".to_vec(),
            data: compressed_data.clone(),
            attrs: 0x21,
        }])
        .unwrap();
        let decompressed_resource_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"DATA",
            id: 128,
            name: b"Packed".to_vec(),
            data: b"ABCD".to_vec(),
            attrs: 0x20,
        }])
        .unwrap();
        assert_ne!(raw_resource_fork.len(), decompressed_resource_fork.len());
        let resource_files = vec![PpcVfsResourceFileRecord {
            path: path.to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: false,
        }];
        let resources = vec![PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: path.to_string(),
            res_type: u32::from_be_bytes(*b"DATA"),
            res_id: 128,
            name: b"Packed".to_vec(),
            data: b"ABCD".to_vec(),
            raw_data: Some(compressed_data),
            raw_attrs: Some(0x21),
            attrs: 0x20,
            handle: 0,
        }];

        assert_eq!(
            ppc_resource_fork_len_for_path(&resource_files, &resources, path),
            u32::try_from(raw_resource_fork.len()).unwrap()
        );
    }

    #[test]
    fn hle_import_runner_set_res_attrs_drops_raw_archive_resource_bytes() {
        let pef = synthetic_pef_with_import(b"SetResAttrs");
        let mut loaded = load_pef_application(&pef).unwrap();
        let path = "System Folder/Preferences/Test App HighScores";
        let handle = 0x2000;
        let compressed_data = compressed_resource_bytes(4, &[0x02, b'A', b'B', b'C', b'D', 0xff]);
        let raw_fork =
            noncanonical_single_resource_fork_bytes(*b"DATA", 128, b"ABCD", 0x21, 0x0000);
        loaded.push_vfs_resource_file(PpcVfsResourceFileRecord {
            path: path.to_string(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x0200,
            resource_len: u32::try_from(raw_fork.len()).unwrap(),
            raw_data: Some(raw_fork.clone().into()),
            map_attrs: 0,
            dirty: false,
        });
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: PPC_FIRST_FILE_REF_NUM,
            path: path.to_string(),
            res_type: u32::from_be_bytes(*b"DATA"),
            res_id: 128,
            name: b"Packed".to_vec(),
            data: b"ABCD".to_vec(),
            raw_data: Some(compressed_data),
            raw_attrs: Some(0x21),
            attrs: 0x20,
            handle,
        });
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 0x40;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].attrs, 0x40);
        assert_eq!(loaded.process_file_system.vfs_resources[0].raw_data, None);
        assert_eq!(loaded.process_file_system.vfs_resources[0].raw_attrs, None);
        assert_eq!(loaded.vfs_resource_files[0].raw_data, None);
        assert!(loaded.vfs_resource_files[0].dirty);

        let exports = loaded.take_dirty_vfs_resource_forks();
        assert_ne!(exports[0].data, raw_fork);
        let fork = crate::managers::resource::ResourceFork::parse(&exports[0].data).unwrap();
        let resource = fork.get(*b"DATA", 128).unwrap();

        assert_eq!(resource.data, b"ABCD");
        assert_eq!(resource.raw_data, None);
        assert_eq!(resource.raw_attrs, None);
        assert_eq!(resource.attrs, 0x40);
    }

    #[test]
    fn ppc_dirty_data_fork_export_and_deleted_paths_are_taken() {
        let pef = synthetic_pef();
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Test App Prefs".to_string(),
            data: (b"prefs".to_vec()).into(),
            creator: u32::from_be_bytes(*b"Nano"),
            file_type: u32::from_be_bytes(*b"pref"),
            finder_flags: 0x4000,
            dirty: true,
        });
        loaded.push_test_vfs_file(PpcVfsFileRecord {
            path: "System Folder/Preferences/Clean Prefs".to_string(),
            data: (b"clean".to_vec()).into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
        loaded.push_test_deleted_vfs_file_path("System Folder/Preferences/Old Prefs".to_string());

        let exports = loaded.take_dirty_vfs_files();

        assert_eq!(exports.len(), 1);
        assert_eq!(exports[0].path, "System Folder/Preferences/Test App Prefs");
        assert_eq!(exports[0].data, b"prefs");
        assert_eq!(exports[0].creator, u32::from_be_bytes(*b"Nano"));
        assert_eq!(exports[0].file_type, u32::from_be_bytes(*b"pref"));
        assert_eq!(exports[0].finder_flags, 0x4000);
        assert!(!loaded.vfs_files[0].dirty);
        assert!(loaded.take_dirty_vfs_files().is_empty());
        assert_eq!(
            loaded.take_deleted_vfs_file_paths(),
            vec!["System Folder/Preferences/Old Prefs".to_string()]
        );
        assert!(loaded.take_deleted_vfs_file_paths().is_empty());
    }

    #[test]
    fn hle_import_runner_counts_uniques_and_updates_ppc_vfs_resources() {
        let pef = synthetic_pef_with_import(b"CountResources");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.set_current_resource_refnum(5);
        for (ref_num, res_id, handle) in [(5, 128, 0x1000), (7, 129, 0x1004), (5, 130, 0x1008)] {
            loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
                ref_num,
                path: String::new(),
                res_type: u32::from_be_bytes(*b"pref"),
                res_id,
                name: Vec::new(),
                data: Vec::new(),
                raw_data: None,
                raw_attrs: None,
                attrs: PPC_RES_CHANGED_ATTR,
                handle,
            });
        }
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 5,
            path: String::new(),
            res_type: u32::from_be_bytes(*b"misc"),
            res_id: 128,
            name: Vec::new(),
            data: Vec::new(),
            raw_data: None,
            raw_attrs: None,
            attrs: PPC_RES_CHANGED_ATTR,
            handle: 0x100c,
        });
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 3);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Count1Resources;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 2);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UniqueID;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 131);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Unique1ID;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 129);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateResFile;
        loaded.cpu.gpr[3] = 5;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            loaded
                .vfs_resources
                .iter()
                .find(|record| record.ref_num == 5 && record.res_id == 128)
                .unwrap()
                .attrs
                & PPC_RES_CHANGED_ATTR,
            0
        );
        assert_ne!(
            loaded
                .vfs_resources
                .iter()
                .find(|record| record.ref_num == 7)
                .unwrap()
                .attrs
                & PPC_RES_CHANGED_ATTR,
            0
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateResFile;
        loaded.cpu.gpr[3] = 99;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_RES_F_NOT_FOUND_ERR);
    }

    #[test]
    fn hle_import_runner_mutates_ppc_vfs_resource_metadata() {
        let pef = synthetic_pef_with_import(b"SetResInfo");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x2200;
        let handle = scratch + 0x40;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        write_ppc_pstring(&mut loaded.memory, scratch, b"NewName");
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: 5,
            path: String::new(),
            res_type: u32::from_be_bytes(*b"pref"),
            res_id: 100,
            name: b"OldName".to_vec(),
            data: b"payload".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle,
        });
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 101;
        loaded.cpu.gpr[5] = scratch;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].res_id, 101);
        assert_eq!(loaded.process_file_system.vfs_resources[0].name, b"NewName");

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetResAttrs;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = u32::from(PPC_RES_PROTECTED_ATTR | PPC_RES_CHANGED_ATTR);

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].attrs,
            PPC_RES_PROTECTED_ATTR | PPC_RES_CHANGED_ATTR
        );

        write_ppc_pstring(&mut loaded.memory, scratch, b"Blocked");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetResInfo;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 102;
        loaded.cpu.gpr[5] = scratch;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_RES_ATTR_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].res_id, 101);
        assert_eq!(loaded.process_file_system.vfs_resources[0].name, b"NewName");
    }

    #[test]
    fn named_resource_lookups_follow_current_map_and_case_insensitive_names() {
        let pef = synthetic_pef_with_import(b"GetNamedResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Splash");
        let res_type = u32::from_be_bytes(*b"DATA");
        loaded.process_file_system.extend_vfs_resources([
            PpcVfsResourceRecord {
                ref_num: 7,
                path: "Earlier".to_string(),
                res_type,
                res_id: 100,
                name: b"Splash".to_vec(),
                data: b"earlier".to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
            PpcVfsResourceRecord {
                ref_num: 9,
                path: "Current".to_string(),
                res_type,
                res_id: 101,
                name: b"sPLASH".to_vec(),
                data: b"current".to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
        ]);
        loaded.set_current_resource_refnum(9);
        loaded.cpu.gpr[3] = res_type;
        loaded.cpu.gpr[4] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        let handle = loaded.cpu.gpr[3];
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            ppc_handle_bytes(&mut loaded.memory, &test_handle_records!(loaded), handle),
            Some(b"current".to_vec())
        );

        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Missing");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1NamedResource;
        loaded.cpu.gpr[3] = res_type;
        loaded.cpu.gpr[4] = name_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
    }

    #[test]
    fn get1_named_resource_reports_not_found_for_empty_current_map() {
        let pef = synthetic_pef_with_import(b"Get1NamedResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let name_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(name_ptr, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"First Run");
        loaded.set_current_resource_refnum(128);
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"pref");
        loaded.cpu.gpr[4] = name_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
    }

    #[test]
    fn get_icon_suite_loads_only_selected_family_members() {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", "GetIconSuite"),
            PpcImportDispatcherTarget::GetIconSuite
        );
        let pef = synthetic_pef_with_import(b"GetIconSuite");
        let mut loaded = load_pef_application(&pef).unwrap();
        let output = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(output, vec![0; 4]);
        let ref_num = *loaded.process_file_system.current_resource_file;
        loaded.process_file_system.extend_vfs_resources([
            PpcVfsResourceRecord {
                ref_num,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"ICN#"),
                res_id: 128,
                name: Vec::new(),
                data: vec![0x11; 256],
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
            PpcVfsResourceRecord {
                ref_num,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"ics#"),
                res_id: 128,
                name: Vec::new(),
                data: vec![0x22; 64],
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
            PpcVfsResourceRecord {
                ref_num,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"ics4"),
                res_id: 128,
                name: Vec::new(),
                data: vec![0x33; 128],
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
            PpcVfsResourceRecord {
                ref_num,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"ics8"),
                res_id: 128,
                name: Vec::new(),
                data: vec![0x44; 256],
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            },
        ]);
        loaded.cpu.gpr[3] = output;
        loaded.cpu.gpr[4] = 128;
        loaded.cpu.gpr[5] = 0x0000_ff00;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let suite = loaded.memory.read_u32_be(output).unwrap();
        assert_ne!(suite, 0);
        let entries = ppc_icon_suite_entries(&mut loaded.memory, suite).unwrap();
        assert_eq!(
            entries.iter().map(|entry| entry.0).collect::<Vec<_>>(),
            [
                u32::from_be_bytes(*b"ics#"),
                u32::from_be_bytes(*b"ics4"),
                u32::from_be_bytes(*b"ics8"),
            ]
        );
        assert!(entries.iter().all(|entry| entry.1 != 0));
        assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    }

    #[test]
    fn read_partial_resource_streams_empty_handles_and_reports_loaded_handles() {
        let pef = synthetic_pef_with_import(b"GetResource");
        let mut loaded = load_pef_application(&pef).unwrap();
        let buffer = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(buffer, vec![0; 16]);
        loaded.policy.set_res_load(false);
        let current_resource_refnum = *loaded.process_file_system.current_resource_file;
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: current_resource_refnum,
            path: "Game".to_string(),
            res_type: u32::from_be_bytes(*b"DATA"),
            res_id: 128,
            name: Vec::new(),
            data: b"abcdefgh".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"DATA");
        loaded.cpu.gpr[4] = 128;
        loaded.run_with_hle_imports(64);
        let handle = loaded.cpu.gpr[3];
        assert_ne!(handle, 0);
        assert_eq!(loaded.memory.read_u32_be(handle), Some(0));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ReadPartialResource;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 2;
        loaded.cpu.gpr[5] = buffer;
        loaded.cpu.gpr[6] = 4;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, buffer, 4),
            Some(b"cdef".to_vec())
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LoadResource;
        loaded.cpu.gpr[3] = handle;
        loaded.run_with_hle_imports(64);
        assert_ne!(loaded.memory.read_u32_be(handle), Some(0));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ReadPartialResource;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = buffer + 8;
        loaded.cpu.gpr[6] = 2;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.test_resource_error(), PPC_RESOURCE_IN_MEMORY_ERR);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, buffer + 8, 2),
            Some(b"ab".to_vec())
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[4] = 7;
        loaded.cpu.gpr[6] = 2;
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.test_resource_error(), PPC_INPUT_OUT_OF_BOUNDS_ERR);
    }

    #[test]
    fn classic_resource_convenience_calls_and_hcreate_use_the_vfs() {
        let pef = synthetic_pef_with_import(b"GetIcon");
        let mut loaded = load_pef_application(&pef).unwrap();
        for (res_type, res_id, data) in [
            (*b"ICON", 128, b"icon".as_slice()),
            (*b"PAT ", 129, b"pattern".as_slice()),
        ] {
            let current_resource_refnum = *loaded.process_file_system.current_resource_file;
            loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: "Game".to_string(),
                res_type: u32::from_be_bytes(res_type),
                res_id,
                name: Vec::new(),
                data: data.to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
        }
        loaded.cpu.gpr[3] = 128;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            ppc_handle_bytes(
                &mut loaded.memory,
                &test_handle_records!(loaded),
                loaded.cpu.gpr[3]
            ),
            Some(b"icon".to_vec())
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetPattern;
        loaded.cpu.gpr[3] = 129;
        loaded.run_with_hle_imports(64);
        assert_eq!(
            ppc_handle_bytes(
                &mut loaded.memory,
                &test_handle_records!(loaded),
                loaded.cpu.gpr[3]
            ),
            Some(b"pattern".to_vec())
        );

        let name_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(name_ptr, vec![0; 256]);
        write_ppc_pstring(&mut loaded.memory, name_ptr, b"Preferences");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HCreateResFile;
        loaded.cpu.gpr[3] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
        loaded.cpu.gpr[4] = PPC_ROOT_DIR_ID;
        loaded.cpu.gpr[5] = name_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.vfs_files[0].path, "Preferences");
        assert_eq!(loaded.vfs_resource_files[0].path, "Preferences");
        assert!(loaded.vfs_resource_files[0].dirty);
    }

    #[test]
    fn classic_resource_query_and_count_imports_use_the_vfs() {
        let pef = synthetic_pef_with_import(b"CountResources");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current_refnum = *loaded.process_file_system.current_resource_file;
        let other_refnum = current_refnum + 1;

        // Resource in current file
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: current_refnum,
            path: "App".to_string(),
            res_type: u32::from_be_bytes(*b"STR "),
            res_id: 128,
            name: b"First".to_vec(),
            data: b"hello".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });
        // Resource in another file
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: other_refnum,
            path: "Other".to_string(),
            res_type: u32::from_be_bytes(*b"STR "),
            res_id: 129,
            name: b"Second".to_vec(),
            data: b"world".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });

        // CountResources (all files): should be 2
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 2);

        // Count1Resources (current file only): should be 1
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Count1Resources;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 1);

        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: other_refnum,
            path: "Other".to_string(),
            res_type: u32::from_be_bytes(*b"PICT"),
            res_id: 128,
            name: b"Picture".to_vec(),
            data: Vec::new(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        });

        // CountTypes sees two distinct types; Count1Types sees only STR in
        // the current file, even though STR also occurs in another file.
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::CountTypes;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 2);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Count1Types;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 1);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        let type_output = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(type_output, vec![0xff; 4]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetIndType;
        loaded.cpu.gpr[3] = type_output;
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.memory.read_u32_be(type_output),
            Some(u32::from_be_bytes(*b"PICT"))
        );
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1IndType;
        loaded.cpu.gpr[3] = type_output;
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(
            loaded.memory.read_u32_be(type_output),
            Some(u32::from_be_bytes(*b"STR "))
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[4] = 2;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.memory.read_u32_be(type_output), Some(0));
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        // Unique1ID: candidate should be 129 (since 128 exists in current file)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Unique1ID;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 129);

        // UniqueID: candidate should be 130 (since 128 and 129 exist across files)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UniqueID;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 130);

        // GetIndResource: 1-based index 1 -> res_id 128
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetIndResource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        let handle1 = loaded.cpu.gpr[3];
        assert_ne!(handle1, 0);
        assert_eq!(
            ppc_handle_bytes(
                &mut loaded.memory,
                &test_handle_records!(loaded),
                handle1
            ),
            Some(b"hello".to_vec())
        );

        // Get1IndResource: 1-based index 1 -> res_id 128
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1IndResource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], handle1);

        // Get1IndResource: index 2 should not exist in current file
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Get1IndResource;
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
        loaded.cpu.gpr[4] = 2;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
    }

    #[test]
    fn classic_resource_metadata_and_mutation_imports_use_the_vfs() {
        let pef = synthetic_pef_with_import(b"GetResourceSizeOnDisk");
        let mut loaded = load_pef_application(&pef).unwrap();
        let current_refnum = *loaded.process_file_system.current_resource_file;
        let scratch = PPC_DATA_BASE + 0x2400;
        loaded.memory.add_region(scratch, vec![0; 0x200]);

        let handle = scratch + 0x20;
        loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
            ref_num: current_refnum,
            path: "App".to_string(),
            res_type: u32::from_be_bytes(*b"TEST"),
            res_id: 128,
            name: b"Sample".to_vec(),
            data: b"hello world payload".to_vec(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle,
        });

        // GetResourceSizeOnDisk: reports on-disk size
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 19);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

        // GetResAttrs: retrieves resource attributes
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetResAttrs;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // SetResAttrs: mutates resource attributes
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetResAttrs;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 0x20;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].attrs, 0x20);

        // HomeResFile: queries owning resource file refnum
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HomeResFile;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3] as i16, current_refnum);

        // GetResInfo: retrieves resource metadata
        let id_ptr = scratch + 0x80;
        let type_ptr = scratch + 0x84;
        let name_ptr = scratch + 0x88;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetResInfo;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = id_ptr;
        loaded.cpu.gpr[5] = type_ptr;
        loaded.cpu.gpr[6] = name_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.memory.read_u16_be(id_ptr), Some(128));
        assert_eq!(
            loaded.memory.read_u32_be(type_ptr),
            Some(u32::from_be_bytes(*b"TEST"))
        );
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, name_ptr).as_deref(),
            Some(&b"Sample"[..])
        );

        // ReadPartialResource: reads chunk from offset into buffer
        let buf_ptr = scratch + 0xc0;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ReadPartialResource;
        loaded.cpu.gpr[3] = handle;
        loaded.cpu.gpr[4] = 6;
        loaded.cpu.gpr[5] = buf_ptr;
        loaded.cpu.gpr[6] = 5;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, buf_ptr, 5),
            Some(b"world".to_vec())
        );

        // ChangedResource: marks changed attribute
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::ChangedResource;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].attrs & PPC_RES_CHANGED_ATTR,
            PPC_RES_CHANGED_ATTR
        );

        // RemoveResource: removes from vfs_resources
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::RemoveResource;
        loaded.cpu.gpr[3] = handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert!(loaded.process_file_system.vfs_resources.is_empty());

        // AddResource: adds new resource record
        let new_handle = ppc_alloc_handle_with_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            b"new resource data",
        );
        assert_ne!(new_handle, 0);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AddResource;
        loaded.cpu.gpr[3] = new_handle;
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"NEW_");
        loaded.cpu.gpr[5] = 200;
        loaded.cpu.gpr[6] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources.len(), 1);
        assert_eq!(loaded.process_file_system.vfs_resources[0].res_id, 200);

        // SetResInfo: update ID and name
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetResInfo;
        loaded.cpu.gpr[3] = new_handle;
        loaded.cpu.gpr[4] = 201;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(loaded.process_file_system.vfs_resources[0].res_id, 201);

        // WriteResource: clears changed attribute
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::WriteResource;
        loaded.cpu.gpr[3] = new_handle;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
        assert_eq!(
            loaded.process_file_system.vfs_resources[0].attrs & PPC_RES_CHANGED_ATTR,
            0
        );

        // UpdateResFile: flushes dirty resources
        loaded.cpu.pc = loaded.entry_pc;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::UpdateResFile;
        loaded.cpu.gpr[3] = current_refnum as u32;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    }
