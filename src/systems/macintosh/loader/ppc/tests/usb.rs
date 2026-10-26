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

#[test]
fn usb_expert_reports_copy_pascal_messages_without_requiring_a_device() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBServicesLib",
        b"USBExpertStatus",
    ))
    .unwrap();
    let message = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(message, vec![3, b'U', b'S', b'B']);
    loaded.cpu.gpr[3] = u32::MAX;
    loaded.cpu.gpr[4] = message;
    loaded.cpu.gpr[5] = 42;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::USBExpertStatus);
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = (-6998i32) as u32;
    loaded.cpu.gpr[5] = message;
    loaded.cpu.gpr[6] = 77;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::USBExpertFatalError);
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u8(message + 1, b'X').unwrap();
    assert_eq!(
        loaded.toolbox_startup.usb_expert_log,
        vec![
            (u32::MAX, 0, b"USB".to_vec(), 42),
            (0, -6998, b"USB".to_vec(), 77),
        ]
    );
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = u32::MAX;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::USBExpertStatus);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.toolbox_startup.usb_expert_log.len(), 2);
}

#[test]
fn usb_discovery_and_configuration_validate_reserved_fields_and_direction_flags() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBServicesLib",
        b"USBFindNextPipe",
    ))
    .unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pb, vec![0; 72]);
    loaded.memory.write_u16_be(pb + 6, 72).unwrap();
    loaded.memory.write_u16_be(pb + 8, 0x0100).unwrap();
    loaded.memory.write_u32_be(pb + 20, 0x12345678).unwrap();
    for (symbol, flags) in [
        ("USBFindNextPipe", 3),
        ("USBConfigureInterface", 0),
        ("USBDeviceRequest", 16 | 128),
    ] {
        loaded.memory.write_u32_be(pb + 44, flags).unwrap();
        let before = ppc_memory_read_bytes(&mut loaded.memory, pb, 72).unwrap();
        loaded.cpu.gpr[3] = pb;
        run_test_import(
            &mut loaded,
            dispatcher_target_for_import("USBServicesLib", symbol),
        );
        assert_eq!(loaded.cpu.gpr[3], (-6998i32) as u32);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, pb, 72),
            Some(before)
        );
        if symbol != "USBDeviceRequest" {
            for offset in [32, 36, 40] {
                loaded.memory.write_u32_be(pb + offset, 1).unwrap();
                loaded.cpu.gpr[3] = pb;
                run_test_import(
                    &mut loaded,
                    dispatcher_target_for_import("USBServicesLib", symbol),
                );
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
                loaded.memory.write_u32_be(pb + offset, 0).unwrap();
            }
        }
    }
}

#[test]
fn usb_notification_tokens_own_registration_data_until_removed() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBManagerLib",
        b"USBInstallDeviceNotification",
    ))
    .unwrap();
    let area = PPC_DATA_BASE + 0x1000;
    let pb = area + 4;
    loaded.memory.add_region(area, vec![0xa5; 44]);
    // Real InputSprocket callers leave the first four bytes uninitialized.
    loaded.memory.write_u8(pb + 4, 3).unwrap();
    loaded.memory.write_u32_be(pb + 28, 0x12345678).unwrap();
    loaded.memory.write_u32_be(pb + 32, 77).unwrap();
    let original = ppc_memory_read_bytes(&mut loaded.memory, pb, 36).unwrap();
    loaded.cpu.gpr[3] = pb;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBInstallDeviceNotification,
    );
    assert_eq!(loaded.cpu.gpr[3], pb); // void API
    assert_eq!(loaded.memory.read_u32_be(pb + 20), Some(0));
    let first = loaded.memory.read_u32_be(pb + 24).unwrap();
    assert_ne!(first, 0);
    assert_eq!(
        loaded.toolbox_startup.usb_notifications,
        vec![(first, original)]
    );
    loaded.memory.write_u32_be(pb + 32, 88).unwrap();
    loaded.cpu.gpr[3] = pb;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBInstallDeviceNotification,
    );
    let second = loaded.memory.read_u32_be(pb + 24).unwrap();
    assert_ne!(first, second);
    assert_ne!(second, 0);
    assert_eq!(loaded.toolbox_startup.usb_notifications.len(), 2);
    for token in [first, second] {
        loaded.cpu.gpr[3] = token;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::USBRemoveDeviceNotification,
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        loaded.cpu.gpr[3] = token;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::USBRemoveDeviceNotification,
        );
        assert_ne!(loaded.cpu.gpr[3], 0);
    }
    assert!(loaded.toolbox_startup.usb_notifications.is_empty());
    assert_eq!(loaded.memory.read_u32_be(area), Some(0xa5a5a5a5));
    assert_eq!(loaded.memory.read_u32_be(area + 40), Some(0xa5a5a5a5));
    loaded.memory.write_u32_be(pb + 28, 0).unwrap();
    loaded.cpu.gpr[3] = pb;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBInstallDeviceNotification,
    );
    assert_eq!(
        loaded.memory.read_u32_be(pb + 20),
        Some(ppc_i16_result(PPC_PARAM_ERR))
    );
    assert_eq!(loaded.memory.read_u32_be(pb + 24), Some(0));
    assert!(loaded.toolbox_startup.usb_notifications.is_empty());
}

#[test]
fn usb_device_descriptor_lookup_rejects_unknown_references_without_writing_output() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBManagerLib",
        b"USBGetDeviceDescriptor",
    ))
    .unwrap();
    let area = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(area, vec![0xa5; 32]);
    for reference in [0, 1, u32::MAX] {
        loaded.memory.write_u32_be(area, reference).unwrap();
        for size in [0, 1, 18, 20, u32::MAX] {
            let before = ppc_memory_read_bytes(&mut loaded.memory, area, 32).unwrap();
            loaded.cpu.gpr[3] = area;
            loaded.cpu.gpr[4] = area + 8;
            loaded.cpu.gpr[5] = size;
            run_test_import(
                &mut loaded,
                PpcImportDispatcherTarget::USBGetDeviceDescriptor,
            );
            assert_eq!(loaded.cpu.gpr[3], (-6998i32) as u32);
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, area, 32),
                Some(before)
            );
        }
    }
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBGetDeviceDescriptor,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn usb_deallocation_never_frees_non_usb_memory_or_calls_a_completion() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBServicesLib",
        b"USBDeallocMem",
    ))
    .unwrap();
    loaded.cpu.gpr[3] = 8;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::NewPtr { clear: false },
    );
    let native_pointer = loaded.cpu.gpr[3];
    assert_ne!(native_pointer, 0);
    loaded
        .memory
        .write_u32_be(native_pointer, 0x12345678)
        .unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pb, vec![0; 80]);
    loaded.memory.write_u16_be(pb + 6, 72).unwrap();
    loaded.memory.write_u16_be(pb + 8, 0x100).unwrap();
    loaded.memory.write_u32_be(pb + 72, 0xa5a5a5a5).unwrap();
    for (completion, expected) in [(0, -6984i32), (0x12345678, -6998), (u32::MAX, -6975)] {
        for buffer in [0, pb + 72, native_pointer, u32::MAX] {
            loaded.memory.write_u32_be(pb + 20, completion).unwrap();
            loaded.memory.write_u32_be(pb + 32, buffer).unwrap();
            let before = ppc_memory_read_bytes(&mut loaded.memory, pb, 80).unwrap();
            loaded.cpu.gpr[3] = pb;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::USBDeallocMem);
            assert_eq!(loaded.cpu.gpr[3], expected as u32);
            assert_eq!(
                ppc_memory_read_bytes(&mut loaded.memory, pb, 80),
                Some(before)
            );
            loaded.cpu.gpr[3] = native_pointer;
            run_test_import(&mut loaded, PpcImportDispatcherTarget::GetPtrSize);
            assert_eq!(loaded.cpu.gpr[3], 8);
            assert_eq!(loaded.memory.read_u32_be(native_pointer), Some(0x12345678));
        }
    }
}

#[test]
fn usb_notification_validation_does_not_allocate_tokens_on_failure() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"USBManagerLib",
        b"USBInstallDeviceNotification",
    ))
    .unwrap();
    let pb = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pb, vec![0; 36]);
    loaded.memory.write_u32_be(pb + 28, 0x12345678).unwrap();
    loaded.memory.write_u8(pb + 4, 4).unwrap();
    loaded.cpu.gpr[3] = pb;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBInstallDeviceNotification,
    );
    assert_eq!(loaded.memory.read_u32_be(pb + 20), Some((-6949i32) as u32));
    assert_eq!(loaded.memory.read_u32_be(pb + 24), Some(0));
    assert!(loaded.toolbox_startup.usb_notifications.is_empty());
    assert_eq!(loaded.toolbox_startup.usb_next_notification_token, 1);
    loaded.memory.write_u8(pb + 4, 0xff).unwrap();
    loaded.toolbox_startup.usb_next_notification_token = u32::MAX;
    loaded.cpu.gpr[3] = pb;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBInstallDeviceNotification,
    );
    assert_eq!(loaded.memory.read_u32_be(pb + 24), Some(u32::MAX));
    loaded.cpu.gpr[3] = pb;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::USBInstallDeviceNotification,
    );
    assert_eq!(
        loaded.memory.read_u32_be(pb + 20),
        Some(ppc_i16_result(PPC_MEM_FULL_ERR))
    );
    assert_eq!(loaded.memory.read_u32_be(pb + 24), Some(0));
    assert_eq!(loaded.toolbox_startup.usb_notifications.len(), 1);
}
