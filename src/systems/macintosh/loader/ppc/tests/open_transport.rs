use super::*;

fn ot(function: PpcOpenTransportFunction, loaded: &mut PpcLoadedApp) -> PpcImportAction {
    dispatch_open_transport::ppc_dispatch_open_transport(function, &loaded.cpu, &mut loaded.memory)
}

fn ot_named(name: &str) -> PpcOpenTransportFunction {
    match dispatcher_target_for_import("OTClientLib", name) {
        PpcImportDispatcherTarget::OpenTransport(function) => function,
        other => panic!("{name} resolved to {other:?}"),
    }
}

fn returned(action: PpcImportAction) -> Option<u32> {
    match action {
        PpcImportAction::Return(value) => Some(value),
        _ => None,
    }
}

#[test]
fn open_transport_resolves_in_every_client_namespace() {
    for (library, symbol) in [
        ("OTClientUtilLib", "InitOpenTransportPriv"),
        ("OTClientLib", "OTOpenEndpointPriv"),
        ("OTInetClientLib", "OTInetGetInterfaceInfo"),
        ("OTUtilityLib", "OTLIFOEnqueue"),
        ("Apple;Carbon;Networking", "OTOpenEndpointInContext"),
        ("CarbonLib", "InitOpenTransportInContext"),
    ] {
        assert!(
            matches!(
                dispatcher_target_for_import(library, symbol),
                PpcImportDispatcherTarget::OpenTransport(_)
            ),
            "{library}:{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("Apple;Carbon;Networking", "OTAllocMemInContext"),
        PpcImportDispatcherTarget::NewPtr { clear: false }
    );
    assert_eq!(
        dispatcher_target_for_import("OTClientLib", "OTNoSuchCall"),
        PpcImportDispatcherTarget::Unsupported
    );
}

#[test]
fn open_transport_initialises_but_opens_no_provider() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"OTOpenEndpointPriv")).unwrap();
    let error = PPC_STACK_TOP - 64;
    let not_found = dispatch_open_transport::PPC_OT_NOT_FOUND_ERR as u32;
    assert_eq!(
        returned(ot(ot_named("InitOpenTransportPriv"), &mut loaded)),
        Some(0)
    );

    loaded.cpu.gpr[3] = dispatch_open_transport::PPC_OT_CONFIGURATION;
    loaded.cpu.gpr[6] = error;
    assert_eq!(
        returned(ot(ot_named("OTOpenEndpointPriv"), &mut loaded)),
        Some(0)
    );
    assert_eq!(loaded.memory.read_u32_be(error), Some(not_found));

    loaded.cpu.gpr[5] = error + 4;
    assert_eq!(
        returned(ot(ot_named("OTOpenInternetServicesPriv"), &mut loaded)),
        Some(0)
    );
    assert_eq!(loaded.memory.read_u32_be(error + 4), Some(not_found));
    assert_eq!(
        returned(ot(ot_named("OTInetGetInterfaceInfo"), &mut loaded)),
        Some(not_found)
    );
    assert_eq!(
        returned(ot(ot_named("OTBind"), &mut loaded)),
        Some(dispatch_open_transport::PPC_OT_BAD_REFERENCE_ERR as u32)
    );
}

#[test]
fn open_transport_converts_internet_hosts() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"OTInetHostToString")).unwrap();
    let text = PPC_STACK_TOP - 64;
    let host = text + 32;
    loaded.cpu.gpr[3] = 0xC0A8_0102;
    loaded.cpu.gpr[4] = text;
    ot(ot_named("OTInetHostToString"), &mut loaded);
    let bytes: Vec<u8> = (0..12)
        .map(|offset| loaded.memory.read_u8(text + offset).unwrap())
        .collect();
    assert_eq!(&bytes, b"192.168.1.2\0");

    loaded.cpu.gpr[3] = text;
    loaded.cpu.gpr[4] = host;
    assert_eq!(
        returned(ot(ot_named("OTInetStringToHost"), &mut loaded)),
        Some(0)
    );
    assert_eq!(loaded.memory.read_u32_be(host), Some(0xC0A8_0102));
}

#[test]
fn open_transport_lists_and_atomics_follow_the_utilities_contract() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OTLIFOEnqueue")).unwrap();
    let list = PPC_STACK_TOP - 128;
    let (first, second) = (list + 16, list + 32);
    loaded.memory.write_u32_be(list, 0).unwrap();
    for link in [first, second] {
        loaded.cpu.gpr[3] = list;
        loaded.cpu.gpr[4] = link;
        ot(ot_named("OTLIFOEnqueue"), &mut loaded);
    }
    assert_eq!(loaded.memory.read_u32_be(list), Some(second));
    loaded.cpu.gpr[4] = first;
    assert_eq!(returned(ot(ot_named("OTRemoveLink"), &mut loaded)), Some(1));
    assert_eq!(loaded.memory.read_u32_be(second), Some(0));
    assert_eq!(
        returned(ot(ot_named("OTLIFODequeue"), &mut loaded)),
        Some(second)
    );
    assert_eq!(loaded.memory.read_u32_be(list), Some(0));

    let byte = list + 48;
    loaded.memory.write_u8(byte, 0).unwrap();
    loaded.cpu.gpr[3] = byte;
    loaded.cpu.gpr[4] = 3;
    assert_eq!(
        returned(ot(ot_named("OTAtomicSetBit"), &mut loaded)),
        Some(0)
    );
    assert_eq!(loaded.memory.read_u8(byte), Some(0x08));
    assert_eq!(
        returned(ot(ot_named("OTAtomicSetBit"), &mut loaded)),
        Some(1)
    );

    let counter = list + 52;
    loaded.memory.write_u32_be(counter, 41).unwrap();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = counter;
    assert_eq!(
        returned(ot(ot_named("OTAtomicAdd32"), &mut loaded)),
        Some(42)
    );
}
