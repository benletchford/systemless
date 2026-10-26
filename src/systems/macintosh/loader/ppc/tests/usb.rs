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
