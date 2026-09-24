use super::*;

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
