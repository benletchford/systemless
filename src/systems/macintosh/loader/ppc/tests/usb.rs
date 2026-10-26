use super::*;

#[test]
fn usb_word_conversion_and_request_fields_follow_the_guest_abi() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBServicesLib",
        b"USBToHostWord",
    ))
    .unwrap();
    for symbol in ["USBToHostWord", "HostToUSBWord"] {
        for (value, expected) in [(0, 0), (0x1234, 0x3412), (0xffff, 0xffff), (0xabcd_0100, 1)] {
            loaded.cpu.gpr[3] = value;
            run_test_import(
                &mut loaded,
                dispatcher_target_for_import("USBServicesLib", symbol),
            );
            assert_eq!(loaded.cpu.gpr[3], expected);
        }
    }
    for direction in 0..=1 {
        for kind in 0..=2 {
            for recipient in 0..=3 {
                loaded.cpu.gpr[3] = direction;
                loaded.cpu.gpr[4] = kind;
                loaded.cpu.gpr[5] = recipient;
                run_test_import(
                    &mut loaded,
                    dispatcher_target_for_import("USBServicesLib", "USBMakeBMRequestType"),
                );
                assert_eq!(loaded.cpu.gpr[3], direction * 128 + kind * 32 + recipient);
            }
        }
    }
}

#[test]
fn usb_enumeration_exhaustion_keeps_the_device_reference_and_outputs_unchanged() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBManagerLib",
        b"USBGetNextDeviceByClass",
    ))
    .unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xa5; 16]);
    for reference in [u32::MAX, 0, 0x1234] {
        loaded.memory.write_u32_be(output + 4, reference).unwrap();
        let before = ppc_memory_read_bytes(&mut loaded.memory, output, 16).unwrap();
        loaded.cpu.gpr[3] = output + 4;
        loaded.cpu.gpr[4] = output + 8;
        loaded.cpu.gpr[5] = 0xffff;
        loaded.cpu.gpr[6] = 0xffff;
        loaded.cpu.gpr[7] = 0xffff;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::USBGetNextDeviceByClass,
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-43));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, output, 16),
            Some(before)
        );
    }
    loaded.cpu.gpr[3] = output + 4;
    loaded.cpu.gpr[4] = output + 14;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBGetNextDeviceByClass,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u16_be(output + 14), Some(0xa5a5));
}

#[test]
fn usb_unknown_pipes_do_not_fabricate_state_or_modify_outputs() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBServicesLib",
        b"USBGetPipeStatusByReference",
    ))
    .unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xa5; 12]);
    for symbol in [
        "USBGetPipeStatusByReference",
        "USBAbortPipeByReference",
        "USBClearPipeStallByReference",
    ] {
        for reference in [0, u32::MAX, 0x1234] {
            loaded.cpu.gpr[3] = reference;
            loaded.cpu.gpr[4] = output + 4;
            run_test_import(
                &mut loaded,
                dispatcher_target_for_import("USBServicesLib", symbol),
            );
            assert_eq!(loaded.cpu.gpr[3], (-6997i32) as u32);
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, output, 12),
                Some(vec![0xa5; 12])
            );
        }
    }
}

#[test]
fn usb_allocation_rejects_bad_parameter_blocks_and_nonexistent_devices() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBServicesLib",
        b"USBAllocMem",
    ))
    .unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pb, vec![0; 80]);
    loaded.memory.write_u16_be(pb + 6, 72).unwrap();
    loaded.memory.write_u16_be(pb + 8, 0x0100).unwrap();
    loaded.memory.write_u32_be(pb + 20, 0x12345678).unwrap();
    loaded.memory.write_u32_be(pb + 28, 0x1234).unwrap();
    loaded.memory.write_u32_be(pb + 32, 0xa5a5a5a5).unwrap();
    loaded.memory.write_u32_be(pb + 36, 1024).unwrap();
    let before = ppc_memory_read_bytes(&mut loaded.memory, pb, 80).unwrap();
    let heap_before = *loaded.process_memory_manager.heap_cursor_mut();
    loaded.cpu.gpr[3] = pb;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::USBAllocMem);
    assert_eq!(loaded.cpu.gpr[3], (-6998i32) as u32);
    assert_eq!(
        *loaded.process_memory_manager.heap_cursor_mut(),
        heap_before
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, pb, 80),
        Some(before)
    );
    for (offset, value, error) in [
        (20, 0, -6984),
        (20, u32::MAX, -6984),
        (44, 0x80000000, -6983),
    ] {
        loaded.memory.write_u32_be(pb + 20, 0x12345678).unwrap();
        loaded.memory.write_u32_be(pb + offset, value).unwrap();
        loaded.cpu.gpr[3] = pb;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::USBAllocMem);
        assert_eq!(loaded.cpu.gpr[3], error as u32);
    }
    loaded.memory.write_u16_be(pb + 8, 0xffff).unwrap();
    loaded.cpu.gpr[3] = pb;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::USBAllocMem);
    assert_eq!(loaded.cpu.gpr[3], (-6986i32) as u32);
    loaded.memory.write_u16_be(pb + 6, 70).unwrap();
    loaded.cpu.gpr[3] = pb;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::USBAllocMem);
    assert_eq!(loaded.cpu.gpr[3], (-6985i32) as u32);
}

#[test]
fn usb_read_and_descriptor_requests_fail_immediately_without_touching_buffers() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBServicesLib",
        b"USBIntRead",
    ))
    .unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pb, vec![0; 96]);
    loaded.memory.write_u16_be(pb + 6, 72).unwrap();
    loaded.memory.write_u16_be(pb + 8, 0x0100).unwrap();
    loaded.memory.write_u32_be(pb + 20, 0x12345678).unwrap();
    loaded.memory.write_u32_be(pb + 28, 0x1234).unwrap();
    loaded.memory.write_u32_be(pb + 32, pb + 80).unwrap();
    loaded.memory.write_u32_be(pb + 36, 8).unwrap();
    loaded.memory.write_u32_be(pb + 80, 0xa5a5a5a5).unwrap();
    let before = ppc_memory_read_bytes(&mut loaded.memory, pb, 96).unwrap();
    for (symbol, error) in [
        ("USBIntRead", -6997i32),
        ("USBFindNextAssociatedDescriptor", -6978),
    ] {
        loaded.cpu.gpr[3] = pb;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("USBServicesLib", symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], error as u32);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pb, 96),
            Some(before.clone())
        );
    }
}
