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
        ("OTUtilityLib", "OTBufferDataSize"),
        ("OTUtilityLib", "OTReadBuffer"),
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

#[test]
fn open_transport_ddp_address_preserves_neighbors_and_argument_widths() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OTInitDDPAddress")).unwrap();
    let address = PPC_STACK_TOP - 32;
    loaded.memory.write_bytes(address - 1, &[0xA5; 10]).unwrap();
    loaded.cpu.gpr[3..8].copy_from_slice(&[address, 0x12345678, 0xABCD, 0x1234, 0x5678]);
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("OTATalkSharedLib", "OTInitDDPAddress"),
    );
    let bytes: Vec<_> = (0..10)
        .map(|i| loaded.memory.read_u8(address - 1 + i).unwrap())
        .collect();
    assert_eq!(bytes, [0xA5, 1, 0, 0x56, 0x78, 0xCD, 0x34, 0x78, 0, 0xA5]);
}

#[test]
fn open_transport_nbp_addresses_round_trip_escaped_components() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"OTSetNBPEntityFromAddress")).unwrap();
    let entity = PPC_STACK_TOP - 512;
    let source = entity + 128;
    let output = source + 128;
    let address = br"Game\:One:Myth\@Game@Zone\\Name";
    loaded.memory.write_bytes(source, address).unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[entity, source, address.len() as u32]);
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("OTATalkSharedLib", "OTSetNBPEntityFromAddress"),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    for (name, expected) in [
        ("OTExtractNBPName", &br"Game\:One"[..]),
        ("OTExtractNBPType", &br"Myth\@Game"[..]),
        ("OTExtractNBPZone", &br"Zone\\Name"[..]),
    ] {
        loaded.cpu.gpr[3..5].copy_from_slice(&[entity, output]);
        ot(ot_named(name), &mut loaded);
        let result = dispatch_stdc::ppc_std_c_string(&mut loaded.memory, output, 100);
        assert_eq!(result, expected);
    }
    loaded.cpu.gpr[3] = entity;
    assert_eq!(
        returned(ot(ot_named("OTGetNBPEntityLengthAsAddress"), &mut loaded)),
        Some(address.len() as u32)
    );
    loaded
        .memory
        .write_u8(output + address.len() as u32, 0xA5)
        .unwrap();
    loaded.cpu.gpr[3..5].copy_from_slice(&[output, entity]);
    assert_eq!(
        returned(ot(ot_named("OTSetAddressFromNBPEntity"), &mut loaded)),
        Some(address.len() as u32)
    );
    let result: Vec<_> = (0..address.len() as u32)
        .map(|i| loaded.memory.read_u8(output + i).unwrap())
        .collect();
    assert_eq!(result, address);
    assert_eq!(
        loaded.memory.read_u8(output + address.len() as u32),
        Some(0xA5)
    );
}

#[test]
fn open_transport_nbp_truncates_each_component_without_losing_following_fields() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"OTSetNBPEntityFromAddress")).unwrap();
    let entity = PPC_STACK_TOP - 512;
    let source = entity + 128;
    let output = source + 128;
    let mut address = vec![b'A'; 40];
    address.extend_from_slice(b":type@zone");
    loaded.memory.write_bytes(source, &address).unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[entity, source, address.len() as u32]);
    assert_eq!(
        returned(ot(ot_named("OTSetNBPEntityFromAddress"), &mut loaded)),
        Some(0)
    );
    for (name, expected) in [
        ("OTExtractNBPName", vec![b'A'; 32]),
        ("OTExtractNBPType", b"type".to_vec()),
        ("OTExtractNBPZone", b"zone".to_vec()),
    ] {
        loaded.cpu.gpr[3..5].copy_from_slice(&[entity, output]);
        ot(ot_named(name), &mut loaded);
        assert_eq!(
            dispatch_stdc::ppc_std_c_string(&mut loaded.memory, output, 100),
            expected
        );
    }
}

#[test]
fn open_transport_nbp_component_setters_and_initialization_preserve_boundaries() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OTInitNBPEntity")).unwrap();
    let entity = PPC_STACK_TOP - 512;
    let source = entity + 128;
    let output = source + 128;
    loaded.memory.write_bytes(entity - 1, &[0xA5; 101]).unwrap();
    loaded.cpu.gpr[3] = entity;
    ot(ot_named("OTInitNBPEntity"), &mut loaded);
    assert_eq!(loaded.memory.read_u8(entity - 1), Some(0xA5));
    assert_eq!(loaded.memory.read_u8(entity + 99), Some(0xA5));
    for (setter, getter) in [
        ("OTSetNBPName", "OTExtractNBPName"),
        ("OTSetNBPType", "OTExtractNBPType"),
        ("OTSetNBPZone", "OTExtractNBPZone"),
    ] {
        loaded.memory.write_bytes(source, b"A:B@C\\D\0").unwrap();
        loaded.cpu.gpr[3..5].copy_from_slice(&[entity, source]);
        assert_eq!(returned(ot(ot_named(setter), &mut loaded)), Some(1));
        loaded.cpu.gpr[3..5].copy_from_slice(&[entity, output]);
        ot(ot_named(getter), &mut loaded);
        assert_eq!(
            dispatch_stdc::ppc_std_c_string(&mut loaded.memory, output, 100),
            br"A\:B\@C\\D"
        );
    }
    loaded.memory.write_bytes(source, &[b'X'; 33]).unwrap();
    loaded.memory.write_u8(source + 33, 0).unwrap();
    loaded.cpu.gpr[3..5].copy_from_slice(&[entity, source]);
    assert_eq!(returned(ot(ot_named("OTSetNBPName"), &mut loaded)), Some(0));
    loaded.cpu.gpr[3..5].copy_from_slice(&[entity, output]);
    ot(ot_named("OTExtractNBPName"), &mut loaded);
    assert_eq!(
        dispatch_stdc::ppc_std_c_string(&mut loaded.memory, output, 100),
        vec![b'X'; 32]
    );
    assert_eq!(loaded.memory.read_u8(entity + 99), Some(0xA5));
}

#[test]
fn open_transport_buffer_reads_span_blocks_and_preserve_the_chain() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OTReadBuffer")).unwrap();
    let base = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(base, vec![0; 512]);
    for (block, next, data, length) in [
        (base, base + 32, base + 128, 3),
        (base + 32, base + 64, 0, 0),
        (base + 64, 0, base + 131, 5),
    ] {
        loaded.memory.write_u32_be(block + 8, next).unwrap();
        loaded.memory.write_u32_be(block + 12, data).unwrap();
        loaded.memory.write_u32_be(block + 16, length).unwrap();
    }
    for (index, byte) in b"abcdefgh".iter().enumerate() {
        loaded
            .memory
            .write_u8(base + 128 + index as u32, *byte)
            .unwrap();
    }
    loaded.cpu.gpr[3] = base;
    assert_eq!(
        returned(ot(ot_named("OTBufferDataSize"), &mut loaded)),
        Some(8)
    );
    let info = base + 192;
    let output = base + 224;
    let length = base + 256;
    loaded.memory.write_u32_be(info, base).unwrap();
    loaded.memory.write_u8(info + 8, 0x5a).unwrap();
    for (requested, expected, done, cursor, offset) in [
        (0, b"".as_slice(), false, base, 0),
        (4, b"abcd".as_slice(), false, base + 64, 1),
        (8, b"efgh".as_slice(), true, 0, 0),
        (8, b"".as_slice(), true, 0, 0),
    ] {
        loaded.memory.write_u32_be(length, requested).unwrap();
        loaded.cpu.gpr[3] = info;
        loaded.cpu.gpr[4] = output;
        loaded.cpu.gpr[5] = length;
        assert_eq!(
            returned(ot(ot_named("OTReadBuffer"), &mut loaded)),
            Some(u32::from(done))
        );
        assert_eq!(
            loaded.memory.read_u32_be(length),
            Some(expected.len() as u32)
        );
        assert_eq!(loaded.memory.read_u32_be(info), Some(cursor));
        assert_eq!(loaded.memory.read_u32_be(info + 4), Some(offset));
        assert_eq!(loaded.memory.read_u8(info + 8), Some(0x5a));
        for (index, byte) in expected.iter().enumerate() {
            assert_eq!(loaded.memory.read_u8(output + index as u32), Some(*byte));
        }
    }
    assert_eq!(loaded.memory.read_u32_be(base + 8), Some(base + 32));
    assert_eq!(loaded.memory.read_u32_be(base + 16), Some(3));
}

#[test]
fn open_transport_buffer_read_rejects_cycles_and_unmapped_payloads() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OTReadBuffer")).unwrap();
    let base = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(base, vec![0; 128]);
    for next in [base, 0] {
        loaded.memory.write_u32_be(base + 8, next).unwrap();
        loaded.memory.write_u32_be(base + 12, 0xfffffff0).unwrap();
        loaded.memory.write_u32_be(base + 16, 32).unwrap();
        loaded.memory.write_u32_be(base + 32, base).unwrap();
        loaded.memory.write_u32_be(base + 48, 32).unwrap();
        loaded.memory.write_u8(base + 64, 0x5a).unwrap();
        loaded.cpu.gpr[3] = base + 32;
        loaded.cpu.gpr[4] = base + 64;
        loaded.cpu.gpr[5] = base + 48;
        assert_eq!(returned(ot(ot_named("OTReadBuffer"), &mut loaded)), Some(1));
        assert_eq!(loaded.memory.read_u32_be(base + 48), Some(0));
        assert_eq!(loaded.memory.read_u32_be(base + 32), Some(base));
        assert_eq!(loaded.memory.read_u8(base + 64), Some(0x5a));
    }
}

#[test]
fn open_transport_nbp_string_addresses_preserve_exact_bytes_and_lengths() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"OTInitNBPAddress")).unwrap();
    let source = PPC_STACK_TOP - 512;
    let destination = source + 128;
    let name = b"Game\\:One:Type@Zone";
    loaded.memory.write_bytes(source, name).unwrap();
    loaded
        .memory
        .write_u8(source + name.len() as u32, 0)
        .unwrap();
    loaded
        .memory
        .write_bytes(destination - 1, &[0xa5; 64])
        .unwrap();
    loaded.cpu.gpr[3..5].copy_from_slice(&[destination, source]);
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("OTATalkSharedLib", "OTInitNBPAddress"),
    );
    assert_eq!(loaded.cpu.gpr[3], name.len() as u32 + 2);
    assert_eq!(loaded.memory.read_u16_be(destination), Some(0x0102));
    for (offset, &byte) in name.iter().enumerate() {
        assert_eq!(
            loaded.memory.read_u8(destination + 2 + offset as u32),
            Some(byte)
        );
    }
    assert_eq!(loaded.memory.read_u8(destination - 1), Some(0xa5));
    assert_eq!(
        loaded.memory.read_u8(destination + 2 + name.len() as u32),
        Some(0xa5)
    );
    // An explicit length copies embedded NULs and does not append a terminator.
    loaded.memory.write_bytes(source, b"A\0BC").unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[destination, source, 4]);
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("OTATalkSharedLib", "OTSetAddressFromNBPString"),
    );
    assert_eq!(loaded.cpu.gpr[3], 4);
    assert_eq!(
        loaded.memory.read_u32_be(destination),
        Some(u32::from_be_bytes(*b"A\0BC"))
    );
    // The -1 form uses strlen, and snapshotting supports overlapping buffers.
    loaded.cpu.gpr[3..6].copy_from_slice(&[source + 1, source, u32::MAX]);
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("OTATalkSharedLib", "OTSetAddressFromNBPString"),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u8(source + 1), Some(b'A'));
    loaded.cpu.gpr[3..6].copy_from_slice(&[destination, source, 0xffff_fffe]);
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("OTATalkSharedLib", "OTSetAddressFromNBPString"),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn open_transport_ddp_comparison_uses_field_wildcards_and_rejects_other_families() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"OTCompareDDPAddresses")).unwrap();
    let left = PPC_STACK_TOP - 64;
    let right = left + 16;
    let address = [1, 0, 0x12, 0x34, 5, 6, 7, 0];
    for (offset, replacement, expected) in [
        (7, 99, 1),
        (4, 0, 1),
        (5, 0, 1),
        (6, 0, 1),
        (4, 8, 0),
        (3, 0, 0),
        (1, 1, 0),
        (1, 2, 0),
    ] {
        loaded.memory.write_bytes(left, &address).unwrap();
        loaded.memory.write_bytes(right, &address).unwrap();
        loaded.memory.write_u8(right + offset, replacement).unwrap();
        for (a, b) in [(left, right), (right, left)] {
            loaded.cpu.gpr[3..5].copy_from_slice(&[a, b]);
            run_test_import(
                &mut loaded,
                dispatcher_target_for_import("OTATalkSharedLib", "OTCompareDDPAddresses"),
            );
            assert_eq!(loaded.cpu.gpr[3], expected, "field {offset}");
        }
    }
    loaded.memory.write_bytes(right, &address).unwrap();
    loaded.memory.write_u16_be(right + 2, 0).unwrap();
    loaded.cpu.gpr[3..5].copy_from_slice(&[left, right]);
    run_test_import(
        &mut loaded,
        dispatcher_target_for_import("OTATalkSharedLib", "OTCompareDDPAddresses"),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
}
