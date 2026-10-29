use super::*;

const SCRATCH: u32 = PPC_DATA_BASE + 0x1000;

fn loaded_with(symbol: &[u8]) -> PpcLoadedApp {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(symbol)).unwrap();
    loaded.memory.add_region(SCRATCH, vec![0; 0x400]);
    loaded
}

fn ae(operation: PpcAppleEventCompatibilityOperation) -> PpcImportDispatcherTarget {
    PpcImportDispatcherTarget::AppleEventCompatibility(operation)
}

fn system(operation: PpcSystemCompatibilityOperation) -> PpcImportDispatcherTarget {
    PpcImportDispatcherTarget::SystemCompatibility(operation)
}

fn toolbox(operation: PpcToolboxCompatOperation) -> PpcImportDispatcherTarget {
    PpcImportDispatcherTarget::ToolboxCompat(operation)
}

#[test]
fn system_imports_resolve_to_native_targets() {
    for symbol in [
        "AECoercePtr",
        "AEGetTheCurrentEvent",
        "AESizeOfNthItem",
        "CMAbort",
        "CMActivate",
        "CMEvent",
        "CMGetConnEnvirons",
        "CMGetToolName",
        "CMGetUserData",
        "CMReset",
        "CMResume",
        "CMSetUserData",
        "CountComponents",
        "GetComponentInfo",
        "CrsrDevButtons",
        "CrsrDevDisposeDevice",
        "CrsrDevFlush",
        "CrsrDevMove",
        "CrsrDevNewDevice",
        "CrsrDevSetButtons",
        "CrsrDevUnitsPerInch",
        "DMDisableDisplay",
        "DMEnableDisplay",
        "DateToSeconds",
        "DeferUserFn",
        "EGetInfo",
        "GetPageState",
        "InlineGetHandleSize",
        "LMGetApplLimit",
        "LMGetEventQueue",
        "LMGetROM85",
        "LMGetWMgrPort",
        "LMSetCursorNew",
        "LMSetMouseTemp",
        "LMSetRawMouseLocation",
        "MIDIConnectData",
        "MaxMemSys",
        "OpenSlot",
        "PrDlgMain",
        "PrJobInit",
        "ReplaceGestaltValue",
        "SerStatus",
        "SleepQInstall",
        "SleepQRemove",
        "SysBreak",
        "SystemEvent",
    ] {
        assert_ne!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::Unsupported,
            "{symbol}"
        );
    }
    for (symbol, target) in [
        (
            "InlineGetHandleSize",
            PpcImportDispatcherTarget::GetHandleSize,
        ),
        ("MaxMemSys", PpcImportDispatcherTarget::MaxMem),
        ("LMGetApplLimit", PpcImportDispatcherTarget::GetApplLimit),
        (
            "LMGetEventQueue",
            system(PpcSystemCompatibilityOperation::GetEvQHdr),
        ),
        (
            "SerStatus",
            toolbox(PpcToolboxCompatOperation::UnopenedDriver),
        ),
        (
            "DMDisableDisplay",
            PpcImportDispatcherTarget::ReturnError(-6220),
        ),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn apple_event_pointer_coercion_item_sizes_and_current_event() {
    let mut loaded = loaded_with(b"AECoercePtr");
    let (text, result) = (SCRATCH, SCRATCH + 0x10);
    loaded.memory.write_bytes(text, b"cows").unwrap();
    let text_type = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[3] = text_type;
    loaded.cpu.gpr[4] = text;
    loaded.cpu.gpr[5] = 4;
    loaded.cpu.gpr[6] = text_type;
    loaded.cpu.gpr[7] = result;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CoercePtr),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(result), Some(text_type));
    assert_ne!(loaded.memory.read_u32_be(result + 4), Some(0));

    loaded.cpu.gpr[3] = text_type;
    loaded.cpu.gpr[4] = text;
    loaded.cpu.gpr[5] = 4;
    loaded.cpu.gpr[6] = u32::from_be_bytes(*b"long");
    loaded.cpu.gpr[7] = SCRATCH + 0x20;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CoercePtr),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1700));
    assert_eq!(loaded.memory.read_u32_be(SCRATCH + 0x24), Some(0));

    let list = SCRATCH + 0x40;
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = list;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CreateList),
    );
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = result;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::PutDesc),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    let (type_out, size_out) = (SCRATCH + 0x60, SCRATCH + 0x64);
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = type_out;
    loaded.cpu.gpr[6] = size_out;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::SizeOfNthItem),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(type_out), Some(text_type));
    assert_eq!(loaded.memory.read_u32_be(size_out), Some(4));
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 2;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::SizeOfNthItem),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1701));

    let current = SCRATCH + 0x80;
    loaded
        .memory
        .write_u32_be(current + 4, 0xdead_beef)
        .unwrap();
    loaded.cpu.gpr[3] = current;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::GetTheCurrentEvent),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(current),
        Some(u32::from_be_bytes(*b"null"))
    );
    assert_eq!(loaded.memory.read_u32_be(current + 4), Some(0));
}

#[test]
fn communications_toolbox_and_component_queries_find_nothing() {
    let mut loaded = loaded_with(b"CMGetToolName");
    loaded.memory.write_u8(SCRATCH, 5).unwrap();
    loaded.cpu.gpr[3] = 7;
    loaded.cpu.gpr[4] = SCRATCH;
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("InterfaceLib", "CMGetToolName"),
    );
    assert_eq!(loaded.memory.read_u8(SCRATCH), Some(0));
    for (symbol, expected) in [
        ("CMGetUserData", 0),
        ("CMGetConnEnvirons", ppc_i16_result(-1)),
        ("CountComponents", 0),
        ("GetComponentInfo", ppc_i16_result(-3000)),
    ] {
        loaded.cpu.gpr[3] = 0;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("InterfaceLib", symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], expected, "{symbol}");
    }
}

#[test]
fn cursor_device_records_hold_settings_and_move_the_mouse_globals() {
    use crate::memory::globals::addr;
    let mut loaded = loaded_with(b"CrsrDevNewDevice");
    loaded.cpu.gpr[3] = SCRATCH;
    run_test_import(
        &mut loaded,
        system(PpcSystemCompatibilityOperation::CursorDeviceNew),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let device = loaded.memory.read_u32_be(SCRATCH).unwrap();
    assert_ne!(device, 0);

    // Adjacent byte-sized fields and the buttonOp array must survive
    // updates to cntButtons and buttons (Universal Interfaces CursorDevices.h).
    loaded.memory.write_bytes(device + 24, &[0xa5; 12]).unwrap();
    for (operation, value, offset) in [
        (
            PpcSystemCompatibilityOperation::CursorDeviceSetButtons,
            3,
            25,
        ),
        (PpcSystemCompatibilityOperation::CursorDeviceButtons, 1, 27),
    ] {
        loaded.cpu.gpr[3] = device;
        loaded.cpu.gpr[4] = value;
        run_test_import(&mut loaded, system(operation));
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u8(device + offset), Some(value as u8));
    }
    for offset in [24, 26, 28, 29, 30, 31, 32, 33, 34, 35] {
        assert_eq!(loaded.memory.read_u8(device + offset), Some(0xa5));
    }
    loaded.cpu.gpr[3] = device;
    loaded.cpu.gpr[4] = 400 << 16;
    run_test_import(
        &mut loaded,
        system(PpcSystemCompatibilityOperation::CursorDeviceUnitsPerInch),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(device + 20), Some(400 << 16));

    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map: [0; 16],
        mouse_button: true,
        mouse_v: 100,
        mouse_h: 200,
    });
    loaded.input_sprocket.device_mouse_h = Some(190);
    loaded.input_sprocket.device_mouse_v = Some(95);
    loaded.cpu.gpr[3] = device;
    loaded.cpu.gpr[4] = (-20i32) as u32;
    loaded.cpu.gpr[5] = 5;
    run_test_import(
        &mut loaded,
        system(PpcSystemCompatibilityOperation::CursorDeviceMove),
    );
    for global in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
        assert_eq!(loaded.memory.read_u32_be(global), Some((105 << 16) | 180));
    }

    loaded.cpu.gpr[3] = device;
    run_test_import(
        &mut loaded,
        system(PpcSystemCompatibilityOperation::CursorDeviceFlush),
    );
    assert_eq!(loaded.current_input_snapshot().mouse_v, 105);
    assert_eq!(loaded.current_input_snapshot().mouse_h, 180);
    assert!(loaded.current_input_snapshot().mouse_button);
    assert_eq!(loaded.input_sprocket.device_mouse_h, Some(180));
    assert_eq!(loaded.input_sprocket.device_mouse_v, Some(105));
    let input = loaded.current_input_snapshot();
    assert_eq!(
        ppc_isp_device_element_next_event(
            PPC_ISP_MOUSE_X_ELEMENT,
            input,
            &mut loaded.input_sprocket,
        ),
        Some(10 << 16)
    );
    assert_eq!(
        ppc_isp_device_element_next_event(
            PPC_ISP_MOUSE_X_ELEMENT,
            input,
            &mut loaded.input_sprocket,
        ),
        None
    );
    assert_eq!(
        loaded.memory.read_u32_be(addr::MOUSE_LOC2),
        Some((105 << 16) | 180)
    );

    loaded.cpu.gpr[3] = device;
    run_test_import(
        &mut loaded,
        system(PpcSystemCompatibilityOperation::CursorDeviceDispose),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
}

fn write_date(memory: &mut PpcSectionMem, address: u32, fields: [u16; 6]) {
    for (index, field) in fields.into_iter().enumerate() {
        memory
            .write_u16_be(address + index as u32 * 2, field)
            .unwrap();
    }
}

#[test]
fn date_to_seconds_inverts_seconds_to_date() {
    let mut loaded = loaded_with(b"DateToSeconds");
    let (date, out) = (SCRATCH, SCRATCH + 0x20);
    for (fields, seconds) in [
        ([1904, 1, 1, 0, 0, 0], 0u32),
        ([2000, 1, 1, 0, 0, 0], 3_029_529_600),
        ([1999, 13, 1, 0, 0, 0], 3_029_529_600),
        ([1904, 3, 1, 1, 2, 3], 60 * 86_400 + 3_723),
    ] {
        write_date(&mut loaded.memory, date, fields);
        loaded.cpu.gpr[3] = date;
        loaded.cpu.gpr[4] = out;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DateToSeconds);
        assert_eq!(loaded.memory.read_u32_be(out), Some(seconds), "{fields:?}");
    }

    loaded.cpu.gpr[3] = 3_029_529_600 + 86_400 * 40 + 3_661;
    loaded.cpu.gpr[4] = date;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SecondsToDate);
    loaded.cpu.gpr[3] = date;
    loaded.cpu.gpr[4] = out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DateToSeconds);
    assert_eq!(
        loaded.memory.read_u32_be(out),
        Some(3_029_529_600 + 86_400 * 40 + 3_661)
    );
}

#[test]
fn replace_gestalt_value_requires_an_existing_selector() {
    let mut loaded = loaded_with(b"ReplaceGestaltValue");
    let response = SCRATCH;
    let custom = u32::from_be_bytes(*b"Xtst");
    loaded.cpu.gpr[3] = custom;
    loaded.cpu.gpr[4] = 7;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ReplaceGestaltValue);
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(PPC_GESTALT_UNDEF_SELECTOR_ERR)
    );

    loaded.cpu.gpr[3] = custom;
    loaded.cpu.gpr[4] = 7;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGestaltValue);
    let system_version = u32::from_be_bytes(*b"sysv");
    for selector in [custom, system_version] {
        loaded.cpu.gpr[3] = selector;
        loaded.cpu.gpr[4] = 0x1234;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ReplaceGestaltValue);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        loaded.cpu.gpr[3] = selector;
        loaded.cpu.gpr[4] = response;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::Gestalt);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(response), Some(0x1234));
    }
}

#[test]
fn imperialism_low_memory_getters_read_current_values() {
    use crate::memory::globals::addr;
    let mut loaded = loaded_with(b"LMGetFSFCBLen");
    assert_eq!(loaded.memory.read_u16_be(addr::FS_FCB_LEN), Some(4));
    for (symbol, address, value) in [
        ("LMGetDoubleTime", addr::DOUBLE_TIME, 0xfedc_ba98),
        ("LMGetGZMoveHnd", addr::GZ_MOVE_HND, 0x1234_5678),
        ("LMGetGZRootHnd", addr::GZ_ROOT_HND, 0x8765_4321),
    ] {
        let target = dispatcher_target_for_import("InterfaceLib", symbol);
        assert_ne!(target, PpcImportDispatcherTarget::Unsupported, "{symbol}");
        assert_eq!(dispatcher_target_for_import("CarbonLib", symbol), target);
        loaded.memory.write_u32_be(address, value).unwrap();
        run_test_import(&mut loaded, target.clone());
        assert_eq!(loaded.cpu.gpr[3], value, "{symbol}");
        loaded.memory.write_u32_be(address, 0).unwrap();
        run_test_import(&mut loaded, target.clone());
        assert_eq!(loaded.cpu.gpr[3], 0, "{symbol}");
    }
    for (symbol, address) in [
        ("LMGetFSFCBLen", addr::FS_FCB_LEN),
        ("LMGetTheMenu", addr::THE_MENU),
    ] {
        let target = dispatcher_target_for_import("InterfaceLib", symbol);
        assert_ne!(target, PpcImportDispatcherTarget::Unsupported, "{symbol}");
        assert_eq!(dispatcher_target_for_import("CarbonLib", symbol), target);
        loaded.memory.write_u16_be(address, 0x8001).unwrap();
        run_test_import(&mut loaded, target.clone());
        assert_eq!(loaded.cpu.gpr[3], 0xffff_8001, "{symbol}");
        loaded.memory.write_u16_be(address, 0x1234).unwrap();
        run_test_import(&mut loaded, target.clone());
        assert_eq!(loaded.cpu.gpr[3], 0x1234, "{symbol}");
    }
}

#[test]
fn low_memory_driver_paging_and_printing_calls_report_the_runtime_state() {
    use crate::memory::globals::addr;
    let mut loaded = loaded_with(b"LMGetROM85");
    loaded.memory.write_u16_be(addr::ROM85, 0x3fff).unwrap();
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LowMemoryAccessor(PpcLowMemoryAccessor::GetRom85),
    );
    assert_eq!(loaded.cpu.gpr[3], 0x3fff);
    for (accessor, global) in [
        (PpcLowMemoryAccessor::SetMouseTemp, addr::M_TEMP),
        (PpcLowMemoryAccessor::SetRawMouseLocation, addr::MOUSE_LOC),
    ] {
        loaded.cpu.gpr[3] = (12 << 16) | 34;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::LowMemoryAccessor(accessor),
        );
        assert_eq!(loaded.memory.read_u32_be(global), Some((12 << 16) | 34));
    }
    loaded.cpu.gpr[3] = 0xff;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LowMemoryAccessor(PpcLowMemoryAccessor::SetCursorNew),
    );
    assert_eq!(loaded.memory.read_u8(addr::CRSR_NEW), Some(0xff));

    run_test_import(&mut loaded, toolbox(PpcToolboxCompatOperation::WMgrPort));
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_GWORLD);
    run_test_import(
        &mut loaded,
        toolbox(PpcToolboxCompatOperation::GetPageState),
    );
    assert_eq!(loaded.cpu.gpr[3], 2);

    loaded.memory.write_u16_be(SCRATCH + 16, 0).unwrap();
    loaded.cpu.gpr[3] = SCRATCH;
    run_test_import(&mut loaded, toolbox(PpcToolboxCompatOperation::OpenSlot));
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_OPEN_ERR));
    assert_eq!(
        loaded.memory.read_u16_be(SCRATCH + 16),
        Some(PPC_OPEN_ERR as u16)
    );

    for symbol in ["PrJobInit", "PrDlgMain", "SystemEvent"] {
        loaded.cpu.gpr[3] = SCRATCH;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("InterfaceLib", symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], 0, "{symbol}");
    }

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DeferUserFn);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}
