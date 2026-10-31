use super::*;

#[test]
fn get_intl_resource_supplies_roman_numeric_format_from_system_file() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetIntlResource")).unwrap();
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetIntlResource);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    let data = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(loaded.memory.read_u8(data), Some(b'.'));
    assert_eq!(loaded.memory.read_u8(data + 1), Some(b','));
    assert_eq!(loaded.memory.read_u8(data + 9), Some(b'/'));
}

#[test]
fn import_bindings_classify_resource_cleanup_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ReleaseResource"),
        PpcImportDispatcherTarget::ReleaseResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DetachResource"),
        PpcImportDispatcherTarget::DetachResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HLock"),
        PpcImportDispatcherTarget::HLock
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HLockHi"),
        PpcImportDispatcherTarget::HLockHi
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HUnlock"),
        PpcImportDispatcherTarget::HUnlock
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MoveHHi"),
        PpcImportDispatcherTarget::MoveHHi
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HNoPurge"),
        PpcImportDispatcherTarget::HNoPurge
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HPurge"),
        PpcImportDispatcherTarget::HPurge
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TickCount"),
        PpcImportDispatcherTarget::TickCount
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetZone"),
        PpcImportDispatcherTarget::GetZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetZone"),
        PpcImportDispatcherTarget::SetZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InitZone"),
        PpcImportDispatcherTarget::InitZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SystemZone"),
        PpcImportDispatcherTarget::SystemZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ApplicZone"),
        PpcImportDispatcherTarget::ApplicationZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ApplicationZone"),
        PpcImportDispatcherTarget::ApplicationZone
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeCTable"),
        PpcImportDispatcherTarget::DisposeCTable
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "CloseComponent"),
        PpcImportDispatcherTarget::CloseComponent
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewRoutineDescriptor"),
        PpcImportDispatcherTarget::NewRoutineDescriptor
    );
}

#[test]
fn import_bindings_classify_resource_write_imports() {
    for (symbol, target) in [
        ("AddResource", PpcImportDispatcherTarget::AddResource),
        (
            "ChangedResource",
            PpcImportDispatcherTarget::ChangedResource,
        ),
        ("WriteResource", PpcImportDispatcherTarget::WriteResource),
        ("RemoveResource", PpcImportDispatcherTarget::RemoveResource),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn import_bindings_classify_resource_read_imports() {
    for (symbol, target) in [
        ("OpenResFile", PpcImportDispatcherTarget::OpenResFile),
        ("GetResource", PpcImportDispatcherTarget::GetResource),
        ("Get1Resource", PpcImportDispatcherTarget::Get1Resource),
        (
            "GetNamedResource",
            PpcImportDispatcherTarget::GetNamedResource,
        ),
        (
            "Get1NamedResource",
            PpcImportDispatcherTarget::Get1NamedResource,
        ),
        ("GetIndResource", PpcImportDispatcherTarget::GetIndResource),
        (
            "Get1IndResource",
            PpcImportDispatcherTarget::Get1IndResource,
        ),
        ("GetIndString", PpcImportDispatcherTarget::GetIndString),
        ("getindstring", PpcImportDispatcherTarget::GetIndString),
        ("GetString", PpcImportDispatcherTarget::GetString),
        ("GetResAttrs", PpcImportDispatcherTarget::GetResAttrs),
        ("SetResAttrs", PpcImportDispatcherTarget::SetResAttrs),
        ("GetResInfo", PpcImportDispatcherTarget::GetResInfo),
        ("SetResInfo", PpcImportDispatcherTarget::SetResInfo),
        ("HomeResFile", PpcImportDispatcherTarget::HomeResFile),
        ("CountResources", PpcImportDispatcherTarget::CountResources),
        (
            "Count1Resources",
            PpcImportDispatcherTarget::Count1Resources,
        ),
        ("CountTypes", PpcImportDispatcherTarget::CountTypes),
        ("Count1Types", PpcImportDispatcherTarget::Count1Types),
        ("GetIndType", PpcImportDispatcherTarget::GetIndType),
        ("Get1IndType", PpcImportDispatcherTarget::Get1IndType),
        ("UniqueID", PpcImportDispatcherTarget::UniqueID),
        ("Unique1ID", PpcImportDispatcherTarget::Unique1ID),
        ("UpdateResFile", PpcImportDispatcherTarget::UpdateResFile),
        (
            "ReadPartialResource",
            PpcImportDispatcherTarget::ReadPartialResource,
        ),
        ("HCreateResFile", PpcImportDispatcherTarget::HCreateResFile),
        ("GetIcon", PpcImportDispatcherTarget::GetIcon),
        ("GetPattern", PpcImportDispatcherTarget::GetPattern),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn carbonlib_reuses_exact_interface_toolbox_symbols() {
    for symbol in ["GetIndString", "GetCurrentProcess", "WaitNextEvent"] {
        assert_eq!(
            dispatcher_target_for_import("CarbonLib", symbol),
            dispatcher_target_for_import("InterfaceLib", symbol),
            "{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "UnknownToolboxExport"),
        PpcImportDispatcherTarget::Unsupported
    );
    assert_eq!(
        dispatcher_target_for_import("OtherLibrary", "GetIndString"),
        PpcImportDispatcherTarget::Unsupported
    );
}

#[test]
fn hle_import_runner_handles_cur_res_file() {
    let pef = synthetic_pef_with_import(b"CurResFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_current_resource_refnum(7);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 7);
}

#[test]
fn hle_import_runner_handles_use_res_file_and_res_error_state() {
    let pef = synthetic_pef_with_import(b"UseResFile");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 5;
    loaded.set_test_resource_error(-192);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 5);
    assert_eq!(*loaded.process_file_system.current_resource_file, 5);
    assert_eq!(loaded.test_resource_error(), 0);
}

#[test]
fn hle_import_runner_handles_signed_res_error() {
    let pef = synthetic_pef_with_import(b"ResError");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_test_resource_error(-192);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn hle_import_runner_lm_get_sys_map_returns_the_hle_fallback_refnum() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetSysMap"),
        PpcImportDispatcherTarget::LMGetSysMap
    );
    let pef = synthetic_pef_with_import(b"LMGetSysMap");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xdead_beef;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_lm_get_cur_ap_ref_num_reads_signed_low_memory_word() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetCurApRefNum"),
        PpcImportDispatcherTarget::LMGetCurApRefNum
    );
    let pef = synthetic_pef_with_import(b"LMGetCurApRefNum");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .write_u16_be(crate::memory::globals::addr::CUR_APREF_NUM, 0xfffe)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-2));
}

#[test]
fn hle_import_runner_lm_get_hw_cfg_flags_reads_signed_low_memory_word() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetHWCfgFlags"),
        PpcImportDispatcherTarget::LMGetHWCfgFlags
    );
    let pef = synthetic_pef_with_import(b"LMGetHWCfgFlags");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.memory.write_u16_be(0x0b22, 0x8123).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(0x8123u16 as i16));
}

#[test]
fn hle_import_runner_get_vcbq_hdr_returns_low_memory_queue_address() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetVCBQHdr"),
        PpcImportDispatcherTarget::GetVCBQHdr
    );
    let pef = synthetic_pef_with_import(b"GetVCBQHdr");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], crate::memory::globals::addr::VCB_Q_HDR);
}

#[test]
fn hle_import_runner_get_drvq_hdr_returns_low_memory_queue_address() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDrvQHdr"),
        PpcImportDispatcherTarget::GetDrvQHdr
    );
    let pef = synthetic_pef_with_import(b"GetDrvQHdr");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], crate::memory::globals::addr::DRV_Q_HDR);
}

#[test]
fn hle_import_runner_reports_resource_autoload_state() {
    let pef = synthetic_pef_with_import(b"LMGetResLoad");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
}

#[test]
fn closing_application_resource_file_releases_file_reference() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CloseResFile")).unwrap();
    loaded.set_launched_app_path("Games/Test App");
    loaded.initialize_application_resource_reference().unwrap();
    let app_ref = loaded.memory.read_u16_be(crate::memory::globals::addr::CUR_APREF_NUM).unwrap();
    loaded.push_resource_file(PpcResourceFileRecord {
        ref_num: 130,
        path: "Games/Other Resources".to_string(),
        writable: None,
    });
    loaded.set_current_resource_refnum(130);
    loaded.cpu.gpr[3] = u32::from(app_ref);
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
    assert_eq!(loaded.current_resource_refnum(), 130);
    assert!(!loaded.files.iter().any(|file| file.ref_num == app_ref as i16));
    loaded.cpu.gpr[3] = u32::from(app_ref);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CloseResFile);
    assert_eq!(loaded.test_resource_error(), PPC_RES_F_NOT_FOUND_ERR);
}

#[test]
fn launched_application_resource_reference_is_usable_by_file_manager() {
    let mut resource = load_pef_application(&synthetic_pef_with_import(b"LMGetCurMap")).unwrap();
    resource.set_launched_app_path("Games/Test App");
    resource.initialize_application_resource_reference().unwrap();
    let probe = resource.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    let app_ref = resource.cpu.gpr[3] as u16;
    assert_ne!(app_ref, 0, "the application map must have a guest file reference");
    assert_eq!(resource.memory.read_u16_be(crate::memory::globals::addr::CUR_APREF_NUM), Some(app_ref));
    assert_eq!(resource.memory.read_u16_be(PPC_CUR_RES_FILE_ADDR), Some(app_ref));

    let mut file = load_pef_application(&synthetic_pef_with_import(b"GetVRefNum")).unwrap();
    file.set_launched_app_path("Games/Test App");
    file.initialize_application_resource_reference().unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    file.memory.add_region(output, vec![0; 4]);
    file.cpu.gpr[3] = u32::from(app_ref);
    file.cpu.gpr[4] = output;
    let probe = file.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(file.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(file.memory.read_u16_be(output), Some(PPC_BOOT_VOLUME_REF_NUM as u16));
    file.cpu.gpr[3] = 0;
    file.cpu.gpr[4] = output;
    file.memory.write_u16_be(output, 0x1234).unwrap();
    run_test_import(&mut file, PpcImportDispatcherTarget::GetVRefNum);
    assert_eq!(file.cpu.gpr[3], ppc_i16_result(PPC_RF_NUM_ERR));
    assert_eq!(file.memory.read_u16_be(output), Some(0x1234));

    let mut switch = load_pef_application(&synthetic_pef_with_import(b"UseResFile")).unwrap();
    switch.set_launched_app_path("Games/Test App");
    switch.initialize_application_resource_reference().unwrap();
    switch.set_current_resource_refnum(130);
    switch.cpu.gpr[3] = u32::from(app_ref);
    let probe = switch.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(switch.current_resource_refnum(), 0);
    assert_eq!(switch.memory.read_u16_be(PPC_CUR_RES_FILE_ADDR), Some(app_ref));

    let mut eof = load_pef_application(&synthetic_pef_with_import(b"GetEOF")).unwrap();
    eof.seed_vfs_files_and_resources(Vec::new(), vec![PpcVfsResourceFileRecord {
        path: "Games/Test App".to_string(), creator: 0, file_type: 0, finder_flags: 0,
        resource_len: 4, raw_data: Some(vec![1, 2, 3, 4].into()), map_attrs: 0, dirty: false,
    }], Vec::new());
    eof.set_launched_app_path("Games/Test App");
    eof.initialize_application_resource_reference().unwrap();
    eof.memory.add_region(output, vec![0; 4]);
    eof.cpu.gpr[3] = u32::from(app_ref);
    eof.cpu.gpr[4] = output;
    let probe = eof.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(eof.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(eof.memory.read_u32_be(output), Some(4));

    let mut reopen = load_pef_application(&synthetic_pef_with_import(b"OpenResFile")).unwrap();
    reopen.set_launched_app_path("Games/Test App");
    reopen.initialize_application_resource_reference().unwrap();
    reopen.push_resource_file(PpcResourceFileRecord {
        ref_num: 130,
        path: "Games/Other Resources".to_string(),
        writable: None,
    });
    reopen.set_current_resource_refnum(130);
    let name = PPC_DATA_BASE + 0x2000;
    reopen.memory.add_region(name, b"\x08Test App".to_vec());
    reopen.cpu.gpr[3] = name;
    let probe = reopen.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(reopen.cpu.gpr[3], u32::from(app_ref));
    assert_eq!(reopen.current_resource_refnum(), 130);

    let mut home = load_pef_application(&synthetic_pef_with_import(b"HomeResFile")).unwrap();
    home.seed_vfs_files_and_resources(Vec::new(), Vec::new(), vec![PpcVfsResourceRecord {
        ref_num: 0, path: "Games/Test App".to_string(), res_type: u32::from_be_bytes(*b"TEST"),
        res_id: 128, name: Vec::new(), data: vec![1].into(), raw_data: None,
        raw_attrs: None, attrs: 0, handle: 0x1234,
    }]);
    home.set_launched_app_path("Games/Test App");
    home.initialize_application_resource_reference().unwrap();
    home.cpu.gpr[3] = 0x1234;
    let probe = home.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(home.cpu.gpr[3], u32::from(app_ref));
}

#[test]
fn hle_import_runner_lm_get_cur_map_reads_shared_signed_reference_without_error_changes() {
    assert_eq!(dispatcher_target_for_import("InterfaceLib", "LMGetCurMap"),
        PpcImportDispatcherTarget::CurResFile);
    assert_eq!(dispatcher_target_for_import("CarbonLib", "LMGetCurMap"),
        PpcImportDispatcherTarget::CurResFile);
    assert_eq!(dispatcher_target_for_import("OtherLibrary", "LMGetCurMap"),
        PpcImportDispatcherTarget::Unsupported);
    for reference in [7i16, -1, i16::MIN] {
        let pef = synthetic_pef_with_import(b"LMGetCurMap");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.set_current_resource_refnum(reference);
        loaded.set_test_resource_error(-192);
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], reference as i32 as u32);
        assert_eq!(*loaded.process_file_system.current_resource_file, reference);
        assert_eq!(loaded.test_resource_error(), -192);
    }
}

#[test]
fn hle_import_runner_classic_resource_and_print_error_accessors_preserve_signed_state() {
    for error in [0i16, -192, i16::MIN] {
        let mut resource = load_pef_application(&synthetic_pef_with_import(b"LMGetResErr")).unwrap();
        resource.set_test_resource_error(error);
        let probe = resource.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(resource.cpu.gpr[3], error as i32 as u32);
        assert_eq!(resource.test_resource_error(), error);
        let mut printing = load_pef_application(&synthetic_pef_with_import(b"LMGetPrintErr")).unwrap();
        printing.toolbox_startup.printing_error = error;
        let probe = printing.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(printing.cpu.gpr[3], error as i32 as u32);
        assert_eq!(printing.toolbox_startup.printing_error, error);
    }
}

#[test]
fn hle_import_runner_lm_get_boot_drive_reads_signed_low_memory_word() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetBootDrive"),
        PpcImportDispatcherTarget::LMGetBootDrive
    );
    let pef = synthetic_pef_with_import(b"LMGetBootDrive");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .write_u16_be(crate::memory::globals::addr::BOOT_DRIVE, (-1i16) as u16)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1));
}

#[test]
fn hle_import_runner_lm_set_boot_drive_writes_low_memory_word() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetBootDrive"),
        PpcImportDispatcherTarget::LMSetBootDrive
    );
    let pef = synthetic_pef_with_import(b"LMSetBootDrive");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = (-3i16) as u16 as u32;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(crate::memory::globals::addr::BOOT_DRIVE),
        Some((-3i16) as u16)
    );
}

#[test]
fn resource_materialization_honors_attributes_and_preserves_resident_state() {
    for (attrs, expected) in [(0, 0x20), (0x20, 0x60), (0x10, 0xa0)] {
        let mut native = load_pef_application(&synthetic_pef_with_import(b"GetResource")).unwrap();
        native.with_process_memory_manager(|native, manager| {
            let allocator = manager.native_allocator_snapshot().unwrap();
            let mut cursor = allocator.heap.heap_cursor;
            let limit = allocator.heap.heap_limit;
            let mut mem_error = PPC_NO_ERR;
            let mut res_error = PPC_NO_ERR;
            let mut handles = manager.native_handle_records().to_vec();
            let mut resources = vec![PpcVfsResourceRecord {
                ref_num: 0,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"TEST"),
                res_id: 42,
                name: Vec::new(),
                data: b"resource state".to_vec().into(),
                raw_data: None,
                raw_attrs: None,
                attrs,
                handle: 0,
            }];
            let handle = ppc_materialize_vfs_resource_handle(
                manager,
                &mut native.memory,
                &mut cursor,
                limit,
                &mut mem_error,
                &mut handles,
                &mut resources,
                0,
                true,
                &mut res_error,
            );
            assert_ne!(handle, 0);
            assert_eq!(res_error, PPC_NO_ERR);
            assert_eq!(
                manager.state_for_handle(handle),
                Some(expected),
                "attrs={attrs:02x}"
            );
            manager.set_process_handle_purgeable(handle, false);
            manager.lock_process_handle(handle, true);
            let pointer = native.memory.read_u32_be(handle).unwrap();
            let again = ppc_materialize_vfs_resource_handle(
                manager,
                &mut native.memory,
                &mut cursor,
                limit,
                &mut mem_error,
                &mut handles,
                &mut resources,
                0,
                true,
                &mut res_error,
            );
            assert_eq!(again, handle);
            assert_eq!(native.memory.read_u32_be(handle), Some(pointer));
            assert_eq!(
                manager.state_for_handle(handle),
                Some(0xa0),
                "resident state must survive lookup"
            );
        });
    }
}

#[test]
fn resource_materialization_applies_changed_attributes_only_when_loading_data() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"GetResource")).unwrap();
    native.with_process_memory_manager(|native, manager| {
        let allocator = manager.native_allocator_snapshot().unwrap();
        let mut cursor = allocator.heap.heap_cursor;
        let limit = allocator.heap.heap_limit;
        let mut mem_error = PPC_NO_ERR;
        let mut res_error = PPC_NO_ERR;
        let mut handles = manager.native_handle_records().to_vec();
        let mut resources = vec![PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"TEST"),
            res_id: 42,
            name: Vec::new(),
            data: b"deferred resource".to_vec().into(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0x20,
            handle: 0,
        }];
        macro_rules! lookup {
            ($load:expr) => {
                ppc_materialize_vfs_resource_handle(
                    manager,
                    &mut native.memory,
                    &mut cursor,
                    limit,
                    &mut mem_error,
                    &mut handles,
                    &mut resources,
                    0,
                    $load,
                    &mut res_error,
                )
            };
        }
        let handle = lookup!(false);
        assert_ne!(handle, 0);
        assert_eq!(native.memory.read_u32_be(handle), Some(0));
        assert_eq!(manager.state_for_handle(handle), Some(0x60));
        resources[0].attrs = 0x10;
        assert_eq!(lookup!(false), handle);
        assert_eq!(native.memory.read_u32_be(handle), Some(0));
        assert_eq!(manager.state_for_handle(handle), Some(0x60));
        assert_eq!(lookup!(true), handle);
        assert_ne!(native.memory.read_u32_be(handle), Some(0));
        assert_eq!(manager.state_for_handle(handle), Some(0xa0));
        manager.unlock_process_handle(handle);
        manager.set_process_handle_purgeable(handle, false);
        resources[0].attrs = 0x20;
        assert_eq!(lookup!(true), handle);
        assert_eq!(manager.state_for_handle(handle), Some(0x20));
        assert_eq!(
            manager.empty_native_handle(&mut native.memory, handle),
            PPC_NO_ERR
        );
        assert_eq!(native.memory.read_u32_be(handle), Some(0));
        assert_eq!(lookup!(true), handle);
        assert_eq!(manager.state_for_handle(handle), Some(0x60));
        assert_eq!(
            ppc_handle_bytes(&mut native.memory, &handles, handle).unwrap(),
            b"deferred resource"
        );
        assert_eq!(mem_error, PPC_NO_ERR);
        assert_eq!(res_error, PPC_NO_ERR);
    });
}
