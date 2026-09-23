    use super::super::dispatch::QueuedEvent;
    use super::super::test_helpers::{setup, setup_with_port, MockCpu, TEST_SP};
    use super::{
        alias_dispatch_operation_route, ppc_operation_route, quicktime_movie_metadata,
        slot_manager_operation_route, AE_ERR_ACCESSOR_NOT_FOUND, AE_ERR_DESC_NOT_FOUND,
        AE_ERR_HANDLER_NOT_FOUND, AE_ERR_NOT_AN_OBJECT_SPEC, AE_EVENT_ID_ANSWER,
        AE_KEY_COMPARE_PROC, AE_KEY_CONTAINER, AE_KEY_COUNT_PROC, AE_KEY_DESIRED_CLASS,
        AE_KEY_EVENT_CLASS_ATTR, AE_KEY_EVENT_ID_ATTR, AE_KEY_KEY_DATA, AE_KEY_KEY_FORM,
        AE_MANAGER_KEY_RECORDER_COUNT, AE_MANAGER_KEY_VERSION, AE_SEND_MODE_WAIT_REPLY,
        AE_TYPE_APPLE_EVENT, AE_TYPE_NULL, AE_TYPE_OBJECT_SPECIFIER, AE_TYPE_TYPE,
        AE_TYPE_WILDCARD, ALIAS_DISPATCH_OPERATION_ROUTES, PPC_OPERATION_ROUTES,
        SLOT_MANAGER_OPERATION_ROUTES, STANDARD_FILE_GET_DIALOG_HEIGHT,
        STANDARD_FILE_GET_DIALOG_WIDTH, STANDARD_FILE_GET_LIST_RECT, STANDARD_FILE_GET_SCROLL_RECT,
        STANDARD_FILE_GET_VOLUME_RECT, STANDARD_FILE_NAME_ITEM, STANDARD_FILE_PUT_DESKTOP_RECT,
        STANDARD_FILE_PUT_LIST_RECT, STANDARD_FILE_SAVE_RECT,
    };
    use crate::cpu::{CpuOps, Register};
    use crate::execution_kernel::{ExecutionTaskId, ExecutionTaskState};
    use crate::memory::globals::addr;
    use crate::memory::MacMemoryBus;
    use crate::memory::MemoryBus;
    use crate::trap::dispatch::{AeDescriptor, LoadedResources, ResourceFileMap, TrapDispatcher};
    use crate::trap::extended80::Extended80;
    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::sync::{Mutex, MutexGuard, OnceLock};

    static STANDARD_FILE_ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct StandardFileEnvGuard {
        previous: Option<OsString>,
        _lock: MutexGuard<'static, ()>,
    }

    fn write_test_fsspec(
        bus: &mut MacMemoryBus,
        spec_ptr: u32,
        vref: i16,
        dir_id: u32,
        name: &[u8],
    ) {
        bus.write_word(spec_ptr, vref as u16);
        bus.write_long(spec_ptr + 2, dir_id);
        let len = name.len().min(63) as u8;
        bus.write_byte(spec_ptr + 6, len);
        bus.write_bytes(spec_ptr + 7, &name[..usize::from(len)]);
    }

    fn write_test_fmswapfont_frame(bus: &mut MacMemoryBus, sp: u32) -> u32 {
        let in_rec = sp - 0x100;
        bus.write_word(in_rec, 3); // family
        bus.write_word(in_rec + 2, 12); // size
        bus.write_byte(in_rec + 4, 0); // face
        bus.write_byte(in_rec + 5, 1); // needBits
        bus.write_word(in_rec + 6, 0); // device
        bus.write_word(in_rec + 8, 1); // numer.v
        bus.write_word(in_rec + 10, 1); // numer.h
        bus.write_word(in_rec + 12, 1); // denom.v
        bus.write_word(in_rec + 14, 1); // denom.h
        bus.write_long(sp, in_rec); // CONST VAR inRec
        bus.write_long(sp + 4, 0); // result slot
        in_rec
    }

    impl Drop for StandardFileEnvGuard {
        fn drop(&mut self) {
            if let Some(previous) = self.previous.take() {
                // SAFETY: The guard holds the module-local environment
                // mutex while mutating this process-wide test variable.
                unsafe { std::env::set_var("SYSTEMLESS_STANDARD_GET_FILE", previous) };
            } else {
                // SAFETY: The guard holds the module-local environment
                // mutex while mutating this process-wide test variable.
                unsafe { std::env::remove_var("SYSTEMLESS_STANDARD_GET_FILE") };
            }
        }
    }

    fn set_standard_file_env(value: &str) -> StandardFileEnvGuard {
        let lock = STANDARD_FILE_ENV_LOCK.get_or_init(|| Mutex::new(()));
        let guard = lock.lock().unwrap();
        let previous = std::env::var_os("SYSTEMLESS_STANDARD_GET_FILE");
        // SAFETY: The guard holds the module-local environment mutex while
        // mutating this process-wide test variable.
        unsafe { std::env::set_var("SYSTEMLESS_STANDARD_GET_FILE", value) };
        StandardFileEnvGuard {
            previous,
            _lock: guard,
        }
    }

    fn clear_standard_file_env() -> StandardFileEnvGuard {
        let lock = STANDARD_FILE_ENV_LOCK.get_or_init(|| Mutex::new(()));
        let guard = lock.lock().unwrap();
        let previous = std::env::var_os("SYSTEMLESS_STANDARD_GET_FILE");
        // SAFETY: The guard holds the module-local environment mutex while
        // mutating this process-wide test variable.
        unsafe { std::env::remove_var("SYSTEMLESS_STANDARD_GET_FILE") };
        StandardFileEnvGuard {
            previous,
            _lock: guard,
        }
    }

    fn read_screen_pixel_1bpp(
        bus: &crate::memory::MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        x: i16,
        y: i16,
    ) -> bool {
        let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
        let bit = 7 - (x as u32 % 8);
        let addr = screen_base + byte_offset;
        (bus.read_byte(addr) & (1 << bit)) != 0
    }

    fn write_pascal_string(bus: &mut crate::memory::MacMemoryBus, addr: u32, text: &str) {
        let bytes = text.as_bytes();
        bus.write_byte(addr, bytes.len() as u8);
        for (idx, byte) in bytes.iter().enumerate() {
            bus.write_byte(addr + 1 + idx as u32, *byte);
        }
    }

    #[test]
    fn methoddispatch_returns_noerr_and_preserves_stack_pointer() {
        // Inside Macintosh Volume VI (OSL appendix):
        // MethodDispatch is the no-op dispatcher on selector 0.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x1234_5678);
        cpu.write_reg(Register::D1, 0x89AB_CDEF);

        let result = disp.dispatch_toolbox(true, 0x1F8, &mut cpu, &mut bus);
        assert!(result.is_some(), "MethodDispatch should be handled");
        assert!(result.unwrap().is_ok(), "MethodDispatch should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "MethodDispatch should return noErr"
        );
        assert_eq!(
            cpu.read_reg(Register::D1),
            0x89AB_CDEF,
            "MethodDispatch should preserve non-D0 registers"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "MethodDispatch should preserve the caller stack pointer"
        );
    }

    #[test]
    fn cfm_symbol_enumeration_uses_stack_selectors_and_atomic_pascal_results() {
        use crate::cfm::{CfmConnection, CfmExport, CfmState};
        use crate::memory::GuestAddressSpace;
        const STACK: u32 = 0x0100_0000;
        const OUTPUT: u32 = STACK + 0x100;
        let cfm = CfmState {
            connections: vec![CfmConnection {
                id: 7,
                library_name: "fragment".into(),
                main_addr: 0,
                init_addr: 0,
                term_addr: 0,
                exports: vec![CfmExport {
                    name: "Café™".into(),
                    class: 1,
                    address: 0x1234_5678,
                }],
            }],
            ..Default::default()
        };
        for selector in [5, 6, 7] {
            for fault in 0..7 {
                let (mut disp, mut cpu, mut bus) = setup();
                let mut memory = GuestAddressSpace::new();
                memory.add_region(STACK, vec![0xa5; 0x200]);
                bus.set_addressing_32_bit(true);
                bus.attach_guest_address_space(memory.shared_view());
                let result_slot = STACK
                    + match selector {
                        5 => 18,
                        6 => 10,
                        _ => 22,
                    };
                bus.write_word(STACK, selector);
                if selector == 5 {
                    let name = if fault == 2 {
                        b"\x01x".as_slice()
                    } else {
                        b"\x05Caf\x8e\xaa".as_slice()
                    };
                    for (i, byte) in name.iter().enumerate() {
                        bus.write_byte(OUTPUT + 48 + i as u32, *byte);
                    }
                    bus.write_long(STACK + 2, OUTPUT + 40);
                    bus.write_long(STACK + 6, OUTPUT + 32);
                    bus.write_long(STACK + 10, OUTPUT + 48);
                    bus.write_long(STACK + 14, if fault == 1 { 99 } else { 7 });
                } else if selector == 6 {
                    bus.write_long(STACK + 2, OUTPUT);
                    bus.write_long(STACK + 6, if fault == 1 { 99 } else { 7 });
                } else {
                    bus.write_long(STACK + 2, OUTPUT + 40);
                    bus.write_long(STACK + 6, OUTPUT + 32);
                    bus.write_long(STACK + 10, OUTPUT);
                    bus.write_long(STACK + 14, if fault == 2 { 0 } else { 1 });
                    bus.write_long(STACK + 18, if fault == 1 { 99 } else { 7 });
                }
                if fault == 3 {
                    memory.add_readonly_region(
                        OUTPUT + if selector == 5 { 32 } else { 0 },
                        vec![0xa5],
                    );
                }
                if fault == 4 {
                    memory.add_readonly_region(result_slot, vec![0xa5; 2]);
                }
                let initial_sp = if fault == 5 { u32::MAX - 7 } else { STACK };
                cpu.write_reg(Register::A7, initial_sp);
                cpu.write_reg(Register::D0, 0xdead_beef); // D0 is not the selector.
                cpu.write_reg(Register::D1, 0x1122_3344);
                cpu.write_reg(Register::A0, 0x5566_7788);
                let before: Vec<_> = (0..64).map(|i| bus.read_byte(OUTPUT + i)).collect();
                let result = if fault == 6 {
                    disp.dispatch(0xAA5A, &mut cpu, &mut bus)
                } else {
                    disp.dispatch_with_process_services(0xAA5A, &mut cpu, &mut bus, &cfm, None)
                };
                if fault == 6 {
                    assert!(matches!(
                        result,
                        Err(crate::Error::UnimplementedTrap(0xAA5A))
                    ));
                    assert_eq!(cpu.read_reg(Register::A7), initial_sp);
                    assert_eq!(cpu.read_reg(Register::D0), 0xdead_beef);
                } else {
                    assert!(result.is_ok());
                    let error: i16 = match fault {
                        1 => -2801,
                        2 if selector != 6 => -2802,
                        3..=5 => -50,
                        _ => 0,
                    };
                    assert_eq!(cpu.read_reg(Register::D0), error as i32 as u32);
                    assert_eq!(
                        cpu.read_reg(Register::A7),
                        if matches!(fault, 4 | 5) {
                            initial_sp
                        } else {
                            result_slot
                        }
                    );
                    if !matches!(fault, 4 | 5) {
                        assert_eq!(bus.read_word(result_slot), error as u16);
                    }
                    if error == 0 {
                        if selector == 6 {
                            assert_eq!(bus.read_long(OUTPUT), 1);
                        } else {
                            if selector == 7 {
                                assert_eq!(bus.read_byte(OUTPUT), 5);
                            }
                            assert_eq!(bus.read_long(OUTPUT + 32), 0x1234_5678);
                            assert_eq!(bus.read_byte(OUTPUT + 40), 1);
                        }
                    }
                }
                if matches!(fault, 1 | 3..=6) || (fault == 2 && selector != 6) {
                    assert_eq!(
                        (0..64)
                            .map(|i| bus.read_byte(OUTPUT + i))
                            .collect::<Vec<_>>(),
                        before
                    );
                }
                assert_eq!(cpu.read_reg(Register::D1), 0x1122_3344);
                assert_eq!(cpu.read_reg(Register::A0), 0x5566_7788);
            }
        }
    }

    #[test]
    fn codefragmentdispatch_returns_noerr_and_preserves_stack_pointer() {
        // Inside Macintosh: PowerPC System Software 1994, Code Fragment
        // Manager chapter. The selector-0 direct call is the safety-net path.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x1234_5678);

        let result = disp.dispatch_toolbox(true, 0x25A, &mut cpu, &mut bus);
        assert!(result.is_some(), "CodeFragmentDispatch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "CodeFragmentDispatch should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "CodeFragmentDispatch should return noErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "CodeFragmentDispatch should preserve the caller stack pointer"
        );
    }

    #[test]
    fn codefragmentdispatch_preserves_non_d0_registers_and_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D1, 0x2222_3333);
        cpu.write_reg(Register::A0, 0x4444_5555);
        cpu.write_reg(Register::A1, 0x6666_7777);
        let sp_before = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::D0, 0x0000_0000);
        let result = disp.dispatch_toolbox(true, 0x25A, &mut cpu, &mut bus);
        assert!(result.is_some(), "CodeFragmentDispatch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "CodeFragmentDispatch should return"
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::D1), 0x2222_3333);
        assert_eq!(cpu.read_reg(Register::A0), 0x4444_5555);
        assert_eq!(cpu.read_reg(Register::A1), 0x6666_7777);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    #[test]
    fn mixedmodedispatch_returns_noerr_and_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x1234_5678);
        cpu.write_reg(Register::D1, 0x2222_3333);
        cpu.write_reg(Register::A0, 0x4444_5555);
        cpu.write_reg(Register::A1, 0x6666_7777);

        let result = disp.dispatch_toolbox(true, 0x259, &mut cpu, &mut bus);
        assert!(result.is_some(), "MixedModeDispatch should be handled");
        assert!(result.unwrap().is_ok(), "MixedModeDispatch should succeed");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "MixedModeDispatch should return noErr"
        );
        assert_eq!(cpu.read_reg(Register::D1), 0x2222_3333);
        assert_eq!(cpu.read_reg(Register::A0), 0x4444_5555);
        assert_eq!(cpu.read_reg(Register::A1), 0x6666_7777);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    // ========== QuickDraw Text Traps (Toolbox) ==========

    #[test]
    fn spaceextra_sets_current_port_spextra_field() {
        // Inside Macintosh Volume I (1985), p. I-171:
        // SpaceExtra sets the current GrafPort's spExtra field.
        let (mut disp, mut cpu, mut bus) = setup();
        let port = bus.alloc(128);
        disp
            .current_port
            .with_mut(|current_port| *current_port = port);
        let extra = 0x0001_8000u32; // 1.5 Fixed
        bus.write_long(port + 76, 0xDEAD_BEEF);
        bus.write_long(TEST_SP, extra);

        let result = disp.dispatch_toolbox(true, 0x08E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(port + 76), extra);
    }

    #[test]
    fn spaceextra_consumes_fixed_argument_and_pops_four_bytes() {
        // Inside Macintosh Volume I (1985), p. I-171:
        // SpaceExtra(extra: Fixed) consumes one 4-byte Fixed argument.
        let (mut disp, mut cpu, mut bus) = setup();
        let port = bus.alloc(128);
        disp
            .current_port
            .with_mut(|current_port| *current_port = port);
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_long(sp_before, 0xFFFF_8000); // -0.5 Fixed

        let result = disp.dispatch_toolbox(true, 0x08E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 4);
    }

    #[test]
    fn piccomment_consumes_eight_byte_argument_frame() {
        // Inside Macintosh Volume I (1985), p. I-190:
        // PicComment(kind, dataSize, dataHandle) consumes an 8-byte frame.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_word(sp_before, 0x1234);
        bus.write_word(sp_before + 2, 0x0000);
        bus.write_long(sp_before + 4, 0);

        let result = disp.dispatch_toolbox(true, 0x0F2, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 8);
    }

    #[test]
    fn stuffhex_decodes_hex_pairs_into_destination_bytes() {
        // Inside Macintosh Volume I (1985), p. I-195:
        // StuffHex stores bits expressed as hexadecimal digits into a target structure.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src = bus.alloc(32);
        let dst = bus.alloc(16);
        let hex_bytes = *b"0102040810204080";
        let expected = [0x01u8, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80];

        bus.write_byte(src, hex_bytes.len() as u8);
        for (i, byte) in hex_bytes.iter().enumerate() {
            bus.write_byte(src + 1 + i as u32, *byte);
        }
        for i in 0..8u32 {
            bus.write_byte(dst + i, 0xCC);
        }
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, src);
        bus.write_long(sp + 4, dst);

        let result = disp.dispatch_toolbox(true, 0x066, &mut cpu, &mut bus);
        assert!(result.is_some(), "StuffHex should be handled");
        assert!(result.unwrap().is_ok(), "StuffHex should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        for (i, byte) in expected.iter().enumerate() {
            assert_eq!(bus.read_byte(dst + i as u32), *byte);
        }
    }

    #[test]
    fn stuffhex_consumes_thingptr_and_str255_arguments() {
        // Inside Macintosh Volume I (1985), p. I-195:
        // PROCEDURE StuffHex(thingPtr: Ptr; s: Str255) takes two pointer arguments.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);
        bus.write_word(sp + 8, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x066, &mut cpu, &mut bus);
        assert!(result.is_some(), "StuffHex should be handled");
        assert!(result.unwrap().is_ok(), "StuffHex should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 0xBEEF);
    }

    #[test]
    fn ipclistports_zero_request_works_before_ppcinit() {
        // Inside Macintosh: Interapplication Communication (1993),
        // pp. 7-39, 7-41 to 7-42, 7-57.
        let (mut disp, mut cpu, mut bus) = setup();
        let pre_pb = bus.alloc(64);
        let post_pb = bus.alloc(64);
        let port_name = bus.alloc(64);
        let location_name = bus.alloc(64);
        let buffer = bus.alloc(64);
        const IO_RESULT: u32 = 16;
        const START_INDEX: u32 = 40;
        const REQUEST_COUNT: u32 = 42;
        const ACTUAL_COUNT: u32 = 44;
        const PORT_NAME: u32 = 46;
        const LOCATION_NAME: u32 = 50;
        const BUFFER_PTR: u32 = 54;

        bus.write_bytes(port_name, &[0; 64]);
        bus.write_bytes(location_name, &[0; 64]);
        bus.write_bytes(buffer, &[0; 64]);

        bus.write_word(pre_pb + START_INDEX, 0);
        bus.write_word(pre_pb + REQUEST_COUNT, 0);
        bus.write_word(pre_pb + ACTUAL_COUNT, 0x1357);
        bus.write_word(pre_pb + IO_RESULT, 0x2468);
        bus.write_long(pre_pb + PORT_NAME, port_name);
        bus.write_long(pre_pb + LOCATION_NAME, location_name);
        bus.write_long(pre_pb + BUFFER_PTR, buffer);

        cpu.write_reg(Register::A0, pre_pb);
        cpu.write_reg(Register::D0, 0x000A);
        let pre = disp.dispatch_toolbox(false, 0x0DD, &mut cpu, &mut bus);
        assert!(pre.is_some(), "IPCListPorts should be handled");
        assert!(pre.unwrap().is_ok(), "IPCListPorts should return");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(pre_pb + IO_RESULT), 0);
        assert_eq!(bus.read_word(pre_pb + ACTUAL_COUNT), 0);
        assert!(!disp.ppc_initialized);

        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::D0, 0);
        let init = disp.dispatch_toolbox(false, 0x0DD, &mut cpu, &mut bus);
        assert!(init.is_some(), "PPCInit should be handled");
        assert!(init.unwrap().is_ok(), "PPCInit should return");
        assert!(disp.ppc_initialized);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        bus.write_word(post_pb + START_INDEX, 0);
        bus.write_word(post_pb + REQUEST_COUNT, 0);
        bus.write_word(post_pb + ACTUAL_COUNT, 0x1357);
        bus.write_word(post_pb + IO_RESULT, 0x2468);
        bus.write_long(post_pb + PORT_NAME, port_name);
        bus.write_long(post_pb + LOCATION_NAME, location_name);
        bus.write_long(post_pb + BUFFER_PTR, buffer);

        cpu.write_reg(Register::A0, post_pb);
        cpu.write_reg(Register::D0, 0x000A);
        let post = disp.dispatch_toolbox(false, 0x0DD, &mut cpu, &mut bus);
        assert!(
            post.is_some(),
            "IPCListPorts should be handled after PPCInit"
        );
        assert!(
            post.unwrap().is_ok(),
            "IPCListPorts should return after PPCInit"
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(post_pb + IO_RESULT), 0);
        assert_eq!(bus.read_word(post_pb + ACTUAL_COUNT), 0);
    }

    #[test]
    fn ppc_generated_selector_routes_are_sorted_unique_and_complete() {
        assert_eq!(PPC_OPERATION_ROUTES.len(), 1);
        assert!(PPC_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        let init = ppc_operation_route(0xA0DD, 0x0000).expect("PPCInit route");
        assert_eq!(init.routine_name, "PPCInit");
        assert_eq!(
            init.operation_id,
            "selector-operation:_PPC:0x0000:d0-moveq-immediate:8"
        );

        assert!(ppc_operation_route(0xA1DD, 0x0000).is_none());
        assert!(ppc_operation_route(0xA0DD, 0x7000).is_none());
        assert!(ppc_operation_route(0xA0DD, 0x0001_0000).is_none());
    }

    #[test]
    fn ppc_dispatch_records_ppcinit_and_rejects_unregistered_identity_forms() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::D0, 0x0000);
        disp.dispatch(0xA0DD, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            disp.current_selector_operation,
            Some(PPC_OPERATION_ROUTES[0].operation_id)
        );

        for (trap_word, selector) in [(0xA1DD, 0x0000), (0xA0DD, 0x7000), (0xA0DD, 0x0001_0000)] {
            cpu.write_reg(Register::D0, selector);
            disp.dispatch(trap_word, &mut cpu, &mut bus).unwrap();
            assert_eq!(disp.current_selector_operation, None);
        }
    }

    // Pack15 ($A831) — Picture Utilities
    #[test]
    fn pack15_newpictinfo_mints_distinct_nonzero_ids_and_dispospictinfo_returns_noerr() {
        // Inside Macintosh Volume VI (1991), pp. 18-11 and 18-14:
        // Pack15 uses a selector in D0; the stack carries only the
        // Pascal arguments and the caller's function result slot.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let slot1 = bus.alloc(12);
        let slot2 = bus.alloc(12);
        let id1;
        let id2;
        let newpictinfo_first_ok;
        let newpictinfo_second_ok;
        let dispospictinfo_ok;
        let dispospictinfo_double_noerr_ok;

        bus.write_long(slot1, 0x1111_1111);
        bus.write_long(slot1 + 4, 0xDEAD_BEEF);
        bus.write_long(slot1 + 8, 0x2222_2222);
        bus.write_long(slot2, 0x3333_3333);
        bus.write_long(slot2 + 4, 0xFEED_FACE);
        bus.write_long(slot2 + 8, 0x4444_4444);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0602);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_long(sp + 8, slot1 + 4);
        bus.write_word(sp + 12, 0xBEEF);

        let first = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(first.is_some(), "NewPictInfo should be handled");
        assert!(first.unwrap().is_ok(), "NewPictInfo should return");
        id1 = bus.read_long(slot1 + 4);
        newpictinfo_first_ok = (id1 != 0)
            && (bus.read_long(slot1) == 0x1111_1111)
            && (bus.read_long(slot1 + 8) == 0x2222_2222)
            && (cpu.read_reg(Register::A7) == sp + 12)
            && (bus.read_word(sp + 12) == 0xBEEF);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0602);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_long(sp + 8, slot2 + 4);
        bus.write_word(sp + 12, 0xCAFE);

        let second = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(second.is_some(), "NewPictInfo should be handled");
        assert!(second.unwrap().is_ok(), "NewPictInfo should return");
        id2 = bus.read_long(slot2 + 4);
        newpictinfo_second_ok = (id2 != 0)
            && (id2 != id1)
            && (bus.read_long(slot2) == 0x3333_3333)
            && (bus.read_long(slot2 + 8) == 0x4444_4444)
            && (cpu.read_reg(Register::A7) == sp + 12)
            && (bus.read_word(sp + 12) == 0xCAFE);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0206);
        bus.write_long(sp, id1);
        bus.write_word(sp + 4, 0xFACE);

        let third = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(third.is_some(), "DisposPictInfo should be handled");
        assert!(third.unwrap().is_ok(), "DisposPictInfo should return");
        dispospictinfo_ok = (cpu.read_reg(Register::A7) == sp + 4)
            && (cpu.read_reg(Register::D0) == 0)
            && (bus.read_word(sp + 4) == 0xFACE);

        assert!(
            newpictinfo_first_ok && newpictinfo_second_ok,
            "A831:newpictinfo_mints_distinct_nonzero_ids_and_preserves_stack"
        );
        assert!(
            dispospictinfo_ok,
            "A831:dispospictinfo_returns_noerr_and_preserves_stack"
        );

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0206);
        bus.write_long(sp, id1);
        bus.write_word(sp + 4, 0xD00D);

        let fourth = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(fourth.is_some(), "DisposPictInfo should be handled");
        assert!(fourth.unwrap().is_ok(), "DisposPictInfo should return");
        dispospictinfo_double_noerr_ok = (cpu.read_reg(Register::A7) == sp + 4)
            && (cpu.read_reg(Register::D0) == 0)
            && (bus.read_word(sp + 4) == 0xD00D);

        assert!(
            dispospictinfo_double_noerr_ok,
            "A831:dispospictinfo_returns_noerr_on_double_dispose"
        );
    }

    #[test]
    fn pack15_generated_routes_preserve_exact_word_values() {
        assert_eq!(super::PACK15_OPERATION_ROUTES.len(), 7);
        assert!(super::PACK15_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0206, "DisposePictInfo"),
            (0x0403, "RecordPictInfo"),
            (0x0404, "RecordPixMapInfo"),
            (0x0505, "RetrievePictInfo"),
            (0x0602, "NewPictInfo"),
            (0x0800, "GetPictInfo"),
            (0x0801, "GetPixMapInfo"),
        ] {
            let route = super::pack15_operation_route(0xA831, selector).expect("Pack15 route");
            assert_eq!(route.routine_name, routine_name);
            assert_eq!(
                route.operation_id,
                format!("selector-operation:_Pack15:0x{selector:04X}:d0-low-word-immediate:16")
            );
        }

        for (trap_word, selector) in [
            (0xA931, 0x0800),
            (0xAC31, 0x0800),
            (0xA830, 0x0800),
            (0xA831, 0x0000),
            (0xA831, 0x0802),
            (0xA831, 0x0008),
            (0xA831, 0x303C),
            (0xA831, 0x0600),
            (0xA831, 0xFFFF),
        ] {
            assert!(super::pack15_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack15_records_d0_low_word_identity_and_clears_stale_identity() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA831;

        // Low-word carrier accepts stale high D0 word (0xDEAD_0800 -> GetPictInfo)
        let info_buf = bus.alloc(104);
        bus.write_bytes(info_buf, &[0xAA; 104]);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xDEAD_0800);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_long(sp + 8, info_buf);
        bus.write_long(sp + 12, 0x1234_5678);
        bus.write_word(sp + 16, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("Pack15 GetPictInfo arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0800:d0-low-word-immediate:16")
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(sp + 16), 0xBEEF);
        assert_eq!(bus.read_bytes(info_buf, 104), vec![0; 104]);

        // Unknown selector clears stale identity
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0399);
        bus.write_bytes(sp, &[0x55; 6]);
        bus.write_word(sp + 6, 0xCAFE);

        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("Pack15 fallback arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0xCAFE);

        // Wrong trap form clears stale identity
        disp.current_selector_operation =
            Some("selector-operation:_Pack15:0x0800:d0-low-word-immediate:16");
        disp.current_trap_word = 0xA931;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0800);
        bus.write_long(sp + 8, info_buf);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("Pack15 GetPictInfo arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
    }

    #[test]
    fn pack15_preserves_behavior_across_all_seven_operations_and_lifecycle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA831;

        // 1. NewPictInfo (0x0602)
        let id_slot1 = bus.alloc(4);
        let id_slot2 = bus.alloc(4);
        bus.write_long(id_slot1, 0);
        bus.write_long(id_slot2, 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0602);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 16);
        bus.write_word(sp + 6, 1);
        bus.write_long(sp + 8, id_slot1);
        bus.write_word(sp + 12, 0x1111);

        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("NewPictInfo 1").is_ok());
        let id1 = bus.read_long(id_slot1);
        assert_ne!(id1, 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 0x1111);
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0602:d0-low-word-immediate:16")
        );

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0602);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 16);
        bus.write_word(sp + 6, 1);
        bus.write_long(sp + 8, id_slot2);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("NewPictInfo 2").is_ok());
        let id2 = bus.read_long(id_slot2);
        assert_ne!(id2, 0);
        assert_ne!(id2, id1);

        // 2. RecordPictInfo (0x0403)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0403);
        bus.write_long(sp, 0x2222_3333);
        bus.write_long(sp + 4, id1);
        bus.write_word(sp + 8, 0x2222);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("RecordPictInfo valid").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 0x2222);
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0403:d0-low-word-immediate:16")
        );

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0403);
        bus.write_long(sp, 0x2222_3333);
        bus.write_long(sp + 4, 0x9999_9999);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("RecordPictInfo invalid").is_ok());
        assert_eq!(cpu.read_reg(Register::D0) as i16, -11001);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        // 3. RecordPixMapInfo (0x0404)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0404);
        bus.write_long(sp, 0x4444_5555);
        bus.write_long(sp + 4, id2);
        bus.write_word(sp + 8, 0x3333);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("RecordPixMapInfo valid").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(sp + 8), 0x3333);
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0404:d0-low-word-immediate:16")
        );

        // 4. RetrievePictInfo (0x0505)
        let retrieve_buf = bus.alloc(104);
        bus.write_bytes(retrieve_buf, &[0xCC; 104]);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0505);
        bus.write_word(sp, 16);
        bus.write_long(sp + 2, retrieve_buf);
        bus.write_long(sp + 6, id1);
        bus.write_word(sp + 10, 0x4444);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("RetrievePictInfo valid").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0x4444);
        assert_eq!(bus.read_bytes(retrieve_buf, 104), vec![0; 104]);
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0505:d0-low-word-immediate:16")
        );

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0505);
        bus.write_word(sp, 16);
        bus.write_long(sp + 2, retrieve_buf);
        bus.write_long(sp + 6, 0x9999_9999);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("RetrievePictInfo invalid").is_ok());
        assert_eq!(cpu.read_reg(Register::D0) as i16, -11001);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);

        // 5. GetPictInfo (0x0800)
        let get_pict_buf = bus.alloc(104);
        bus.write_bytes(get_pict_buf, &[0xDD; 104]);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0800);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 16);
        bus.write_word(sp + 6, 1);
        bus.write_long(sp + 8, get_pict_buf);
        bus.write_long(sp + 12, 0x1111_2222);
        bus.write_word(sp + 16, 0x5555);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("GetPictInfo").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(sp + 16), 0x5555);
        assert_eq!(bus.read_bytes(get_pict_buf, 104), vec![0; 104]);
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0800:d0-low-word-immediate:16")
        );

        // 6. GetPixMapInfo (0x0801)
        let get_pixmap_buf = bus.alloc(104);
        bus.write_bytes(get_pixmap_buf, &[0xEE; 104]);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0801);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 16);
        bus.write_word(sp + 6, 1);
        bus.write_long(sp + 8, get_pixmap_buf);
        bus.write_long(sp + 12, 0x3333_4444);
        bus.write_word(sp + 16, 0x6666);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("GetPixMapInfo").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(sp + 16), 0x6666);
        assert_eq!(bus.read_bytes(get_pixmap_buf, 104), vec![0; 104]);
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0801:d0-low-word-immediate:16")
        );

        // 7. DisposePictInfo (0x0206)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0206);
        bus.write_long(sp, id1);
        bus.write_word(sp + 4, 0x7777);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("DisposePictInfo 1").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0x7777);
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack15:0x0206:d0-low-word-immediate:16")
        );

        // Subsequent RecordPictInfo with id1 fails with -11001
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0403);
        bus.write_long(sp, 0x2222_3333);
        bus.write_long(sp + 4, id1);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("RecordPictInfo post-dispose").is_ok());
        assert_eq!(cpu.read_reg(Register::D0) as i16, -11001);

        // Double dispose returns noErr (0)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0206);
        bus.write_long(sp, id1);
        let result = disp.dispatch_toolbox(true, 0x031, &mut cpu, &mut bus);
        assert!(result.expect("DisposePictInfo double").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // ColorBit ($A864) — IM:I I-174 says the trap writes `whichBit` to the
    // current grafPort's colrBit field. colrBit is a word-sized INTEGER at
    // GrafPort offset +88 (Imaging With QuickDraw 1994, p. 4-39).
    //
    // Regression coverage:
    //   tests::colorbit_writes_whichbit_value_to_current_port_colrbit_field_at_offset_88
    //   tests::colorbit_writes_max_31_value_to_current_port_colrbit_field
    //   tests::colorbit_zero_overwrites_previous_nonzero_colrbit_value
    //   tests::colorbit_consumes_two_byte_whichbit_argument_and_balances_stack
    #[test]
    fn colorbit_writes_whichbit_value_to_current_port_colrbit_field_at_offset_88() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        // Pre-poison adjacent fields to detect any over-write past the
        // 2-byte colrBit word at port+88.
        let port_ptr = 0x181000u32;
        bus.write_long(port_ptr + 84, 0x0000001E); // bkColor (whiteColor=30)
        bus.write_word(port_ptr + 88, 0); // colrBit initial value per IM:I I-174
        bus.write_word(port_ptr + 90, 0xC3A5); // patStretch sentinel

        bus.write_word(sp, 5);

        let result = disp.dispatch_toolbox(true, 0x064, &mut cpu, &mut bus);
        assert!(result.is_some(), "ColorBit must be a handled trap");
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_word(port_ptr + 88),
            5,
            "ColorBit(5) must write 5 to thePort^.colrBit"
        );
        assert_eq!(
            bus.read_word(port_ptr + 90),
            0xC3A5,
            "ColorBit must not over-write past the 2-byte colrBit slot"
        );
        assert_eq!(
            bus.read_long(port_ptr + 84),
            0x0000001E,
            "ColorBit must not corrupt the preceding bkColor field"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    #[test]
    fn colorbit_writes_max_31_value_to_current_port_colrbit_field() {
        // IM:I I-174: "the possible range of values for whichBit is 0 through 31"
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        let port_ptr = 0x181000u32;
        bus.write_word(port_ptr + 88, 0);
        bus.write_word(sp, 31);

        let result = disp.dispatch_toolbox(true, 0x064, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(port_ptr + 88), 31);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    #[test]
    fn colorbit_zero_overwrites_previous_nonzero_colrbit_value() {
        // Defeats a stub that doesn't actually write the field: after a
        // non-zero ColorBit, calling ColorBit(0) must clear it back to 0.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        let port_ptr = 0x181000u32;

        // First ColorBit(7) — sets colrBit to 7.
        bus.write_word(sp, 7);
        let r1 = disp.dispatch_toolbox(true, 0x064, &mut cpu, &mut bus);
        assert!(r1.unwrap().is_ok());
        assert_eq!(bus.read_word(port_ptr + 88), 7);

        // Reset SP for the second call.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0);
        let r2 = disp.dispatch_toolbox(true, 0x064, &mut cpu, &mut bus);
        assert!(r2.unwrap().is_ok());
        assert_eq!(bus.read_word(port_ptr + 88), 0);
    }

    #[test]
    fn colorbit_consumes_two_byte_whichbit_argument_and_balances_stack() {
        // Pascal PROCEDURE protocol: caller pushes 2-byte INTEGER whichBit;
        // trap pops 2 bytes; no function-result slot.
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 12);
        bus.write_word(sp + 2, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x064, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 2,
            "ColorBit must pop exactly the 2-byte INTEGER argument"
        );
        assert_eq!(
            bus.read_word(sp + 2),
            0xBEEF,
            "ColorBit must not over-write past the 2-byte arg slot"
        );
    }

    #[test]
    fn longmul_writes_signed_64bit_product_to_dest_hilong_lolong() {
        // Inside Macintosh Volume I (1985), p. I-472:
        // LongMul writes signed Int64Bit { hiLong, loLong } product output.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let dest = bus.alloc(8);
        let a: i32 = -123_456_789;
        let b: i32 = 42_424;
        let expected = (a as i64) * (b as i64);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, dest);
        bus.write_long(sp + 4, b as u32);
        bus.write_long(sp + 8, a as u32);

        let result = disp.dispatch_toolbox(true, 0x067, &mut cpu, &mut bus);
        assert!(result.is_some(), "LongMul should be handled");
        assert!(result.unwrap().is_ok(), "LongMul should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_long(dest), (expected >> 32) as u32);
        assert_eq!(bus.read_long(dest + 4), expected as u32);
    }

    #[test]
    fn longmul_consumes_a_b_and_dest_arguments() {
        // Inside Macintosh Volume I (1985), p. I-472:
        // PROCEDURE LongMul(a, b: LONGINT; VAR dest: Int64Bit) consumes 12 bytes.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 2);
        bus.write_long(sp + 8, 3);
        bus.write_word(sp + 12, 0xCAFE);

        let result = disp.dispatch_toolbox(true, 0x067, &mut cpu, &mut bus);
        assert!(result.is_some(), "LongMul should be handled");
        assert!(result.unwrap().is_ok(), "LongMul should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 0xCAFE);
    }

    // ========== Toolbox Event Traps ==========

    // GetNextEvent ($A970) — empty queue
    #[test]
    fn test_get_next_event_empty() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true); // suppress synthetic oapp event
        let sp = TEST_SP;
        // SP+0: event_ptr(4), SP+4: eventMask(2), SP+6: result(2)
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0xFFFF); // eventMask: all
        bus.write_word(sp + 6, 0xBEEF); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // No event: result = 0
        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    // GetNextEvent ($A970) — with mouseDown event
    #[test]
    fn test_get_next_event_with_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true); // suppress synthetic oapp event
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0xFFFF); // eventMask: all
        bus.write_word(sp + 6, 0x0000); // result placeholder

        // Push a mouseDown event
        disp.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        });

        let result = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // Event found: result = 0x0100
        assert_eq!(bus.read_word(sp + 6), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        // Event record: what field at event_ptr+0 (word) = 1 (mouseDown)
        assert_eq!(bus.read_word(event_ptr), 1);
    }

    #[test]
    fn get_next_event_delivers_visible_window_update_after_flushevents_drops_queue_entry() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);

        let bounds_rect_ptr = 0x300000u32;
        bus.write_word(bounds_rect_ptr, 40);
        bus.write_word(bounds_rect_ptr + 2, 40);
        bus.write_word(bounds_rect_ptr + 4, 140);
        bus.write_word(bounds_rect_ptr + 6, 220);

        let new_window_sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, new_window_sp);
        for i in 0..30u32 {
            bus.write_byte(new_window_sp + i, 0);
        }
        bus.write_long(new_window_sp + 18, bounds_rect_ptr);
        bus.write_byte(new_window_sp + 12, 0xFF); // visible = TRUE

        let result = disp.dispatch_window(true, 0x113, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewWindow should be handled");
        assert!(result.unwrap().is_ok(), "NewWindow should return");
        let window_ptr = bus.read_long(TEST_SP);
        assert_ne!(window_ptr, 0, "NewWindow should return a window");
        assert!(
            disp.event_queue
                .iter()
                .any(|event| event.what == 6 && event.message == window_ptr),
            "visible NewWindow should queue an updateEvt"
        );

        cpu.write_reg(Register::D0, 0x0000_FFFF);
        let flush = disp.dispatch_event(false, 0x32, &mut cpu, &mut bus);
        assert!(flush.is_some(), "FlushEvents should be handled");
        assert!(flush.unwrap().is_ok(), "FlushEvents should return");
        assert!(
            disp.event_queue.is_empty(),
            "FlushEvents($FFFF, 0) should remove queued events"
        );

        let event_ptr = 0x200000u32;
        let get_next_sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, get_next_sp);
        bus.write_long(get_next_sp, event_ptr);
        bus.write_word(get_next_sp + 4, 0xFFFF);
        bus.write_word(get_next_sp + 6, 0);

        let event = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(event.is_some(), "GetNextEvent should be handled");
        assert!(event.unwrap().is_ok(), "GetNextEvent should return");
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
        assert_eq!(
            bus.read_word(get_next_sp + 6),
            0x0100,
            "GetNextEvent should report the pending update event"
        );
        assert_eq!(
            bus.read_word(event_ptr),
            6,
            "event.what should be updateEvt"
        );
        assert_eq!(
            bus.read_long(event_ptr + 2),
            window_ptr,
            "event.message should carry the dirty WindowPtr"
        );

        let second_sp = TEST_SP - 8;
        cpu.write_reg(Register::A7, second_sp);
        bus.write_long(second_sp, event_ptr);
        bus.write_word(second_sp + 4, 0xFFFF);
        bus.write_word(second_sp + 6, 0xBEEF);

        let second = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(second.is_some(), "second GetNextEvent should be handled");
        assert!(second.unwrap().is_ok(), "second GetNextEvent should return");
        assert_eq!(
            bus.read_word(second_sp + 6),
            0,
            "flushed update recovery should be one-shot until a new update is queued"
        );
    }

    #[test]
    fn slotmanager_sreadinfo_selector_uses_a0_spblock_d0_selector_and_returns_oserr_in_d0() {
        // Inside Macintosh: Devices (1994), pp. 2-61 to 2-62:
        // _SlotManager selector $0010 (SReadInfo) uses A0=SpBlockPtr and
        // D0=selector on entry, and returns OSErr in D0 on exit.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_block_ptr = 0x0031_0000u32;
        let stack_ptr = 0x00F0_6100u32;
        let selector = 0x0010u32; // SReadInfo
        let sm_empty_slot = (-300i32) as u32;

        bus.write_long(sp_block_ptr, 0xA5A5_A5A5);
        bus.write_byte(sp_block_ptr + 49, 0x0A); // spSlot
        cpu.write_reg(Register::A0, sp_block_ptr);
        cpu.write_reg(Register::D0, selector);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_toolbox(false, 0x06E, &mut cpu, &mut bus);
        assert!(result.is_some(), "SlotManager should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SlotManager should return normally"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            sm_empty_slot,
            "SlotManager should return OSErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A0),
            sp_block_ptr,
            "SlotManager should consume but not rewrite A0 SpBlock pointer"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            stack_ptr,
            "SlotManager register calling convention should preserve A7"
        );
    }

    #[test]
    fn slotmanager_sversion_returns_rom_manager_version_and_reserved_pointer() {
        // Inside Macintosh: Devices (1994), pp. 2-30 to 2-31:
        // SVersion selector $0008 returns version 2 for the ROM-based Slot
        // Manager in spResult and reserves spsPointer for future information.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_block_ptr = 0x0031_0080u32;
        let stack_ptr = 0x00F0_6080u32;

        bus.write_long(sp_block_ptr, 0xDEAD_BEEF);
        bus.write_long(sp_block_ptr + 4, 0xCAFE_BABE);
        bus.write_long(sp_block_ptr + 8, 0x1122_3344);
        cpu.write_reg(Register::A0, sp_block_ptr);
        cpu.write_reg(Register::D0, 0x0008);
        cpu.write_reg(Register::A7, stack_ptr);

        let result = disp.dispatch_toolbox(false, 0x06E, &mut cpu, &mut bus);
        assert!(result.is_some(), "SlotManager should be handled");
        assert!(result.unwrap().is_ok(), "SVersion should return normally");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SVersion should return noErr"
        );
        assert_eq!(
            bus.read_long(sp_block_ptr),
            2,
            "spResult should be version 2"
        );
        assert_eq!(
            bus.read_long(sp_block_ptr + 4),
            0,
            "reserved spsPointer should be NIL"
        );
        assert_eq!(
            bus.read_long(sp_block_ptr + 8),
            0x1122_3344,
            "SVersion should not overwrite the rest of SpBlock"
        );
        assert_eq!(cpu.read_reg(Register::A0), sp_block_ptr);
        assert_eq!(cpu.read_reg(Register::A7), stack_ptr);
    }

    #[test]
    fn slotmanager_generated_selector_routes_are_sorted_unique_and_complete() {
        assert_eq!(SLOT_MANAGER_OPERATION_ROUTES.len(), 41);
        assert!(SLOT_MANAGER_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));
        let version = slot_manager_operation_route(0x0008).expect("SVersion route");
        assert_eq!(version.routine_name, "SVersion");
        assert_eq!(
            version.operation_id,
            "selector-operation:_SlotManager:0x0008:d0-moveq-immediate:8"
        );
        assert!(slot_manager_operation_route(0x0004).is_none());
        assert!(slot_manager_operation_route(0x0001_0008).is_none());
    }

    #[test]
    fn slotmanager_dispatch_records_every_generated_operation_and_rejects_unknown_identity() {
        let (mut disp, mut cpu, mut bus) = setup();
        for route in SLOT_MANAGER_OPERATION_ROUTES {
            cpu.write_reg(Register::A0, 0);
            cpu.write_reg(Register::D0, u32::from(route.selector));
            let result = disp.dispatch_toolbox(false, 0x06E, &mut cpu, &mut bus);
            assert!(result.is_some_and(|result| result.is_ok()));
            assert_eq!(disp.current_selector_operation, Some(route.operation_id));
        }

        cpu.write_reg(Register::D0, 0x0004);
        let result = disp.dispatch_toolbox(false, 0x06E, &mut cpu, &mut bus);
        assert!(result.is_some_and(|result| result.is_ok()));
        assert_eq!(disp.current_selector_operation, None);
    }

    #[test]
    fn slotmanager_sreadinfo_empty_slot_returns_smemptyslot() {
        // Inside Macintosh: Devices (1994), pp. 2-61 to 2-62:
        // SReadInfo result code smEmptySlot (-300) means "No card in this slot."
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_block_ptr = 0x0031_0100u32;
        let sm_empty_slot = (-300i32) as u32;

        for slot in [0x09u8, 0x0Au8] {
            bus.write_long(sp_block_ptr, 0x1122_3344);
            bus.write_byte(sp_block_ptr + 49, slot); // spSlot
            cpu.write_reg(Register::A0, sp_block_ptr);
            cpu.write_reg(Register::D0, 0x0010); // SReadInfo selector

            let result = disp.dispatch_toolbox(false, 0x06E, &mut cpu, &mut bus);
            assert!(result.is_some(), "SlotManager should be handled");
            assert!(
                result.unwrap().is_ok(),
                "SlotManager should return normally"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                sm_empty_slot,
                "SReadInfo selector should return smEmptySlot for empty slot {}",
                slot
            );
        }
    }

    #[test]
    fn slotmanager_writes_result_to_spblock_spresult_offset_zero() {
        // Inside Macintosh: Devices (1994), pp. 2-23 to 2-24:
        // SpBlock starts with spResult at offset 0.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_block_ptr = 0x0031_0200u32;
        let sm_empty_slot = (-300i32) as u32;

        bus.write_long(sp_block_ptr, 0xDEAD_BEEF); // spResult (offset 0)
        bus.write_long(sp_block_ptr + 4, 0xBEEF_DEAD); // spsPointer (offset 4)
        cpu.write_reg(Register::A0, sp_block_ptr);
        cpu.write_reg(Register::D0, 0x0010); // SReadInfo selector

        let result = disp.dispatch_toolbox(false, 0x06E, &mut cpu, &mut bus);
        assert!(result.is_some(), "SlotManager should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SlotManager should return normally"
        );
        assert_eq!(
            bus.read_long(sp_block_ptr),
            sm_empty_slot,
            "SlotManager should mirror the result into SpBlock.spResult"
        );
        assert_eq!(
            bus.read_long(sp_block_ptr + 4),
            0xBEEF_DEAD,
            "SlotManager should not clobber adjacent SpBlock fields"
        );
    }

    #[test]
    fn slotmanager_other_selector_leaves_spblock_result_untouched() {
        // Systemless's SlotManager HLE models the documented SReadInfo
        // selector for the empty-slot result and leaves SpBlock state
        // alone for other selectors that still collapse to smEmptySlot.
        // This pins the selector-specific writeback rule in the HLE.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_block_ptr = 0x0031_0300u32;
        let sm_empty_slot = (-300i32) as u32;

        bus.write_long(sp_block_ptr, 0xDEAD_BEEF);
        bus.write_long(sp_block_ptr + 4, 0xBEEF_DEAD);
        cpu.write_reg(Register::A0, sp_block_ptr);
        cpu.write_reg(Register::D0, 0x0000);

        let result = disp.dispatch_toolbox(false, 0x06E, &mut cpu, &mut bus);
        assert!(result.is_some(), "SlotManager should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SlotManager should return normally"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            sm_empty_slot,
            "SlotManager should still return smEmptySlot"
        );
        assert_eq!(
            bus.read_long(sp_block_ptr),
            0xDEAD_BEEF,
            "non-SReadInfo selectors should not rewrite SpBlock.spResult"
        );
        assert_eq!(
            bus.read_long(sp_block_ptr + 4),
            0xBEEF_DEAD,
            "non-SReadInfo selectors should not clobber adjacent SpBlock fields"
        );
    }

    // Inside Macintosh Volume I, I-257: events not designated by eventMask are
    // kept in the queue and a null event is returned when no designated event
    // is available.
    #[test]
    fn test_get_next_event_mask_miss_returns_null_and_preserves_queue() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true); // suppress synthetic oapp event
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0x0008); // keyDownMask (what=3)
        bus.write_word(sp + 6, 0xBEEF);

        disp.event_queue.push_back(QueuedEvent {
            what: 1, // mouseDown (not in keyDownMask)
            message: 0,
            when: 0,
            where_v: 44,
            where_h: 88,
            modifiers: 0,
        });

        let first = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(bus.read_word(event_ptr), 0); // nullEvt
        assert_eq!(disp.event_queue.len(), 1);
        assert_eq!(disp.event_queue.front().map(|event| event.what), Some(1));

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0x0002); // mouseDownMask (what=1)
        bus.write_word(sp + 6, 0x0000);

        let second = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(second.is_some());
        assert!(second.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0x0100);
        assert_eq!(bus.read_word(event_ptr), 1);
        assert!(disp.event_queue.is_empty());
    }

    fn test_region_handle(
        bus: &mut crate::memory::MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> u32 {
        let rgn_ptr = bus.alloc(10);
        let rgn_handle = bus.alloc(4);
        bus.write_long(rgn_handle, rgn_ptr);
        bus.write_word(rgn_ptr, 10);
        bus.write_word(rgn_ptr + 2, top as u16);
        bus.write_word(rgn_ptr + 4, left as u16);
        bus.write_word(rgn_ptr + 6, bottom as u16);
        bus.write_word(rgn_ptr + 8, right as u16);
        rgn_handle
    }

    // WaitNextEvent ($A860) — empty queue
    #[test]
    fn test_wait_next_event_empty() {
        let (mut disp, mut cpu, mut bus) = setup();
        // Mark the synthetic kAEOpenApplication as already sent so this tests
        // the normal empty-queue path.
        disp.set_sent_open_app_event_for_test(true);
        let sp = TEST_SP;
        // SP+0: mouseRgn(4), SP+4: sleep(4), SP+8: event_ptr(4), SP+12: eventMask(2), SP+14: result(2)
        bus.write_long(sp, 0); // mouseRgn
        bus.write_long(sp + 4, 60); // sleep
        bus.write_long(sp + 8, 0x200000); // event_ptr
        bus.write_word(sp + 12, 0xFFFF); // eventMask
        bus.write_word(sp + 14, 0xBEEF); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(disp.pending_wait_sleep_ticks, 60);
        assert_eq!(disp.current_tick(), 100);
        assert_eq!(bus.read_long(0x200000 + 6), 100);
    }

    #[test]
    fn test_wait_next_event_zero_mask_takes_null_event_path() {
        let (mut disp, mut cpu, mut bus) = setup();
        assert!(!disp
            .apple_event_launch_state
            .is_open_application_event_sent());
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;

        bus.write_word(event_ptr, 0x5555);
        bus.write_long(sp, 0); // mouseRgn
        bus.write_long(sp + 4, 1); // sleep
        bus.write_long(sp + 8, event_ptr);
        bus.write_word(sp + 12, 0); // eventMask: no event types selected
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_word(event_ptr), 0);
        assert!(!disp
            .apple_event_launch_state
            .is_open_application_event_sent());
        assert_eq!(disp.pending_wait_sleep_ticks, 1);
    }

    #[test]
    fn wait_next_event_mouse_rgn_outside_returns_mouse_moved_os_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);
        disp.input_state.set_mouse_position_for_test((50, 25));
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        let mouse_rgn = test_region_handle(&mut bus, 10, 20, 30, 40);

        bus.write_long(sp, mouse_rgn);
        bus.write_long(sp + 4, 60);
        bus.write_long(sp + 8, event_ptr);
        bus.write_word(sp + 12, 0x8000); // osMask, event type 15.
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_word(event_ptr), 15);
        assert_eq!(bus.read_long(event_ptr + 2), 0xFA00_0000);
        assert_eq!(bus.read_word(event_ptr + 10), 50);
        assert_eq!(bus.read_word(event_ptr + 12), 25);
        assert_eq!(disp.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn wait_next_event_mouse_rgn_inside_takes_null_sleep_path() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);
        disp.input_state.set_mouse_position_for_test((20, 25));
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        let mouse_rgn = test_region_handle(&mut bus, 10, 20, 30, 40);

        bus.write_long(sp, mouse_rgn);
        bus.write_long(sp + 4, 7);
        bus.write_long(sp + 8, event_ptr);
        bus.write_word(sp + 12, 0x8000); // osMask.
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(bus.read_word(event_ptr), 0);
        assert_eq!(disp.pending_wait_sleep_ticks, 7);
    }

    #[test]
    fn wait_next_event_empty_mouse_rgn_suppresses_mouse_moved_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);
        disp.input_state.set_mouse_position_for_test((50, 25));
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        let empty_mouse_rgn = test_region_handle(&mut bus, 0, 0, 0, 0);

        bus.write_long(sp, empty_mouse_rgn);
        bus.write_long(sp + 4, 9);
        bus.write_long(sp + 8, event_ptr);
        bus.write_word(sp + 12, 0x8000); // osMask.
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(bus.read_word(event_ptr), 0);
        assert_eq!(disp.pending_wait_sleep_ticks, 9);
    }

    #[test]
    fn wait_next_event_mouse_rgn_respects_event_mask() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true);
        disp.input_state.set_mouse_position_for_test((50, 25));
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        let mouse_rgn = test_region_handle(&mut bus, 10, 20, 30, 40);

        bus.write_long(sp, mouse_rgn);
        bus.write_long(sp + 4, 11);
        bus.write_long(sp + 8, event_ptr);
        bus.write_word(sp + 12, 0x0002); // mouseDownMask, not osMask.
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(bus.read_word(event_ptr), 0);
        assert_eq!(disp.pending_wait_sleep_ticks, 11);
    }

    // WaitNextEvent ($A860) — synthesizes kAEOpenApplication on first call
    #[test]
    fn test_wait_next_event_synthesizes_open_app() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.apple_event_launch_state
            .set_high_level_event_aware(true);
        assert!(!disp
            .apple_event_launch_state
            .is_open_application_event_sent());
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, 0); // mouseRgn
        bus.write_long(sp + 4, 0); // sleep
        bus.write_long(sp + 8, event_ptr); // event_ptr
        bus.write_word(sp + 12, 0x0400); // highLevelEventMask only (bit 10)
        bus.write_word(sp + 14, 0x0000); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // Should return TRUE (event found)
        assert_eq!(bus.read_word(sp + 14), 0x0100);
        // what = kHighLevelEvent (23)
        assert_eq!(bus.read_word(event_ptr), 23);
        // message = kCoreEventClass ('aevt' = 0x61657674)
        assert_eq!(bus.read_long(event_ptr + 2), 0x61657674);
        // where = kAEOpenApplication ('oapp' = 0x6F617070), packed as Point (v, h)
        assert_eq!(bus.read_word(event_ptr + 10), 0x6F61); // where.v
        assert_eq!(bus.read_word(event_ptr + 12), 0x7070); // where.h
                                                           // Flag should be set
        assert!(disp
            .apple_event_launch_state
            .is_open_application_event_sent());

        // Second call should NOT return synthetic event
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, event_ptr);
        bus.write_word(sp + 12, 0x0400);
        bus.write_word(sp + 14, 0x0000);

        let result2 = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result2.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 14), 0); // no event
    }

    #[test]
    fn wait_next_event_skips_open_app_for_high_level_unaware_application() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, event_ptr);
        bus.write_word(sp + 12, 0x0400);
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x060, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(bus.read_word(event_ptr), 0);
        assert!(!disp
            .apple_event_launch_state
            .is_open_application_event_sent());
    }

    // EventAvail ($A971) — with event (peeks, does not remove)
    #[test]
    fn test_event_avail_with_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true); // suppress synthetic oapp event
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        // SP+0: event_ptr(4), SP+4: eventMask(2), SP+6: result(2)
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0xFFFF); // all events
        bus.write_word(sp + 6, 0x0000);

        disp.event_queue.push_back(QueuedEvent {
            what: 1, // mouseDown
            message: 0,
            when: 0,
            where_v: 5,
            where_h: 15,
            modifiers: 0,
        });

        let result = disp.dispatch_toolbox(true, 0x171, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        // Event record should have what=1
        assert_eq!(bus.read_word(event_ptr), 1);
        // Event should still be in the queue (peek, not dequeue)
        assert_eq!(disp.event_queue.len(), 1);
    }

    // EventAvail ($A971) — empty queue
    #[test]
    fn test_event_avail_empty() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true); // suppress synthetic oapp event
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0xFFFF);
        bus.write_word(sp + 6, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x171, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(event_ptr), 0);
        assert_ne!(bus.read_word(event_ptr + 14) & 0x0080, 0);
    }

    // Inside Macintosh Volume I, I-257..I-259: EventAvail follows the same
    // eventMask filtering as GetNextEvent and does not dequeue non-designated
    // events.
    #[test]
    fn test_event_avail_mask_miss_returns_null_and_preserves_queue() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true); // suppress synthetic oapp event
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0x0008); // keyDownMask (what=3)
        bus.write_word(sp + 6, 0xBEEF);

        disp.event_queue.push_back(QueuedEvent {
            what: 1, // mouseDown
            message: 0x1234_5678,
            when: 0,
            where_v: 11,
            where_h: 22,
            modifiers: 0,
        });

        let result = disp.dispatch_toolbox(true, 0x171, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(bus.read_word(event_ptr), 0); // nullEvt
        assert_eq!(disp.event_queue.len(), 1);
        assert_eq!(disp.event_queue.front().map(|event| event.what), Some(1));
    }

    // Inside Macintosh Volume I, I-259: EventAvail reports an event without
    // removing it, so a following GetNextEvent can return that same event.
    #[test]
    fn test_event_avail_then_get_next_event_returns_same_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_sent_open_app_event_for_test(true); // suppress synthetic oapp event
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0x0002); // mouseDownMask
        bus.write_word(sp + 6, 0x0000);

        disp.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0xCAFEBABE,
            when: 0,
            where_v: 9,
            where_h: 19,
            modifiers: 0,
        });

        let peek = disp.dispatch_toolbox(true, 0x171, &mut cpu, &mut bus);
        assert!(peek.is_some());
        assert!(peek.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0x0100);
        assert_eq!(bus.read_word(event_ptr), 1);
        assert_eq!(bus.read_long(event_ptr + 2), 0xCAFEBABE);
        assert_eq!(disp.event_queue.len(), 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0x0002); // mouseDownMask
        bus.write_word(sp + 6, 0x0000);

        let dequeue = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(dequeue.is_some());
        assert!(dequeue.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0x0100);
        assert_eq!(bus.read_word(event_ptr), 1);
        assert_eq!(bus.read_long(event_ptr + 2), 0xCAFEBABE);
        assert!(disp.event_queue.is_empty());
    }

    #[test]
    fn test_event_avail_synthesizes_open_app_without_consuming() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.apple_event_launch_state
            .set_high_level_event_aware(true);
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0x0400); // highLevelEventMask
        bus.write_word(sp + 6, 0x0000);

        let result = disp.dispatch_toolbox(true, 0x171, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0x0100);
        assert_eq!(bus.read_word(event_ptr), 23);
        assert_eq!(bus.read_long(event_ptr + 2), 0x61657674);
        assert_eq!(disp.event_queue.len(), 1);
        assert_eq!(disp.event_queue.front().map(|event| event.what), Some(23));
    }

    #[test]
    fn test_get_next_event_synthesizes_open_app() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.apple_event_launch_state
            .set_high_level_event_aware(true);
        let sp = TEST_SP;
        let event_ptr = 0x200000u32;
        bus.write_long(sp, event_ptr);
        bus.write_word(sp + 4, 0x0400); // highLevelEventMask
        bus.write_word(sp + 6, 0x0000);

        let result = disp.dispatch_toolbox(true, 0x170, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0x0100);
        assert_eq!(bus.read_word(event_ptr), 23);
        assert_eq!(bus.read_long(event_ptr + 2), 0x61657674);
        assert!(disp.event_queue.is_empty());
    }

    // GetKeys ($A976)
    #[test]
    fn test_get_keys_reflects_pressed_keys() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let keys_ptr = 0x200100u32;

        // Press keys spanning several KeyMap bytes, including byte 15.
        disp.push_key_down(0x7B, 28);
        disp.push_key_down(0x31, 32);
        disp.push_key_down(0x26, b'j');
        disp.push_key_down(0x7E, 30);

        bus.write_long(sp, keys_ptr);
        let result = disp.dispatch_toolbox(true, 0x176, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        assert!(disp.key_is_down(0x7B));
        assert!(disp.key_is_down(0x31));
        assert!(disp.key_is_down(0x26));
        assert!(disp.key_is_down(0x7E));
        assert!(!disp.key_is_down(0x2E), "J must not alias the M key");
        assert_eq!(bus.read_byte(keys_ptr + 4), 0x40, "J key byte");
        assert_eq!(
            bus.read_byte(keys_ptr + 5),
            0,
            "M key byte should stay clear when J is down"
        );
        assert_eq!(bus.read_byte(keys_ptr + 6), 0x02, "space key byte");
        assert_eq!(bus.read_byte(keys_ptr + 15), 0x48, "left/up arrow key byte");
        assert_eq!(
            bus.read_byte(keys_ptr + 14),
            0,
            "unused arrow byte should stay clear"
        );

        // Release left arrow and verify it clears.
        disp.push_key_up(0x7B, 28);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, keys_ptr);
        let result = disp.dispatch_toolbox(true, 0x176, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert!(!disp.key_is_down(0x7B));
        assert!(disp.key_is_down(0x31));
        assert!(disp.key_is_down(0x26));
        assert!(disp.key_is_down(0x7E));
        assert_eq!(bus.read_byte(keys_ptr + 15), 0x40, "only up remains");

        // J ($26) and M ($2E) differ only by KeyMap byte. This catches a
        // byte-pair swap in clients that inspect the returned bytes directly.
        disp.push_key_up(0x26, b'j');
        disp.push_key_down(0x2E, b'm');
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, keys_ptr);
        let result = disp.dispatch_toolbox(true, 0x176, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert!(!disp.key_is_down(0x26));
        assert!(disp.key_is_down(0x2E));
        assert_eq!(bus.read_byte(keys_ptr + 4), 0, "J byte should be clear");
        assert_eq!(bus.read_byte(keys_ptr + 5), 0x40, "M key byte");
    }

    // GetMouse ($A972)
    #[test]
    fn test_get_mouse() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let pt_ptr = 0x200000u32;
        bus.write_long(sp, pt_ptr);

        disp.input_state.set_mouse_position_for_test((50, 100));
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2, 50);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2 + 2, 100);

        let result = disp.dispatch_toolbox(true, 0x172, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // Point: v at pt_ptr, h at pt_ptr+2
        assert_eq!(bus.read_word(pt_ptr), 50);
        assert_eq!(bus.read_word(pt_ptr + 2), 100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn get_mouse_returns_current_port_local_coordinates() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = TEST_SP;
        let port = 0x181000u32;
        let pt_ptr = 0x200000u32;
        bus.write_long(sp, pt_ptr);

        // Same geometry as a window/dialog at global (80,120) whose local
        // coordinate system still starts at (0,0).
        bus.write_word(port + 8, (-80i16) as u16);
        bus.write_word(port + 10, (-120i16) as u16);
        bus.write_word(port + 16, 0);
        bus.write_word(port + 18, 0);
        disp.input_state.set_mouse_position_for_test((95, 145));
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2, 95);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2 + 2, 145);

        let result = disp.dispatch_toolbox(true, 0x172, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(
            (
                bus.read_word(pt_ptr) as i16,
                bus.read_word(pt_ptr + 2) as i16
            ),
            (15, 25),
            "GetMouse should report the mouse in current-port local coordinates"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, pt_ptr);
        bus.write_word(port + 8, (-20i16) as u16);
        bus.write_word(port + 10, (-10i16) as u16);
        bus.write_word(port + 16, 80);
        bus.write_word(port + 18, 90);
        disp.input_state.set_mouse_position_for_test((100, 100));
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2, 100);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2 + 2, 100);

        let result = disp.dispatch_toolbox(true, 0x172, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(
            (
                bus.read_word(pt_ptr) as i16,
                bus.read_word(pt_ptr + 2) as i16
            ),
            (80, 90),
            "GetMouse should use portBits.bounds, not portRect.topLeft, after SetOrigin"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn get_mouse_reads_guest_updated_mouse_low_memory() {
        let (mut disp, mut cpu, mut bus) = setup_with_port();
        let sp = TEST_SP;
        let port = 0x181000u32;
        let pt_ptr = 0x200000u32;
        bus.write_long(sp, pt_ptr);

        // A host click left the dispatcher's cached position at (404,526),
        // then guest code recentered the classic Mouse global at (300,400).
        // BasiliskII's ROM GetMouse reads Mouse ($0830) and converts that
        // global point through the current GrafPort. Inside Macintosh
        // Volume I, I-259; Volume II, Appendix A, p. A-10.
        disp.input_state.set_mouse_position_for_test((404, 526));
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2, 300);
        bus.write_word(crate::memory::globals::addr::MOUSE_LOC2 + 2, 400);
        bus.write_word(port + 8, (-80i16) as u16);
        bus.write_word(port + 10, (-120i16) as u16);

        let result = disp.dispatch_toolbox(true, 0x172, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(
            (
                bus.read_word(pt_ptr) as i16,
                bus.read_word(pt_ptr + 2) as i16
            ),
            (220, 280),
            "GetMouse must honor a guest-updated Mouse global before converting to local coordinates"
        );
        assert_eq!(
            disp.input_state.mouse_position(),
            (300, 400),
            "GetMouse must synchronize the dispatcher with the guest-updated Mouse global"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // StillDown ($A973) — button pressed
    #[test]
    fn test_still_down_pressed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0x0000);

        disp.input_state.set_mouse_button_for_test(true);

        let result = disp.dispatch_toolbox(true, 0x173, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp), 0x0100);
        // SP unchanged for StillDown
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn test_still_down_ignores_queued_mouse_down_from_original_press() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0);

        disp.push_mouse_down(50, 100);

        let result = disp.dispatch_toolbox(true, 0x173, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_word(sp),
            0x0100,
            "StillDown should keep tracking while the original queued mouseDown is still pending"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn test_still_down_stops_after_mouse_up_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0xFFFF);

        disp.push_mouse_down(50, 100);
        disp.push_mouse_up(50, 100);

        let result = disp.dispatch_toolbox(true, 0x173, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    // StillDown ($A973) — button not pressed
    #[test]
    fn test_still_down_not_pressed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0xFFFF);

        disp.input_state.set_mouse_button_for_test(false);

        let result = disp.dispatch_toolbox(true, 0x173, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn test_wait_mouse_up_ignores_queued_mouse_down_from_original_press() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0);

        disp.push_mouse_down(50, 100);

        let result = disp.dispatch_toolbox(true, 0x177, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_word(sp),
            0x0100,
            "WaitMouseUp should keep tracking while only the original mouseDown is queued"
        );
        assert!(disp.event_queue.iter().any(|event| event.what == 1));
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn test_wait_mouse_up_false_removes_mouse_up_event() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0xFFFF);

        disp.push_mouse_down(50, 100);
        disp.push_mouse_up(50, 100);

        let result = disp.dispatch_toolbox(true, 0x177, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp), 0);
        assert!(
            disp.event_queue.iter().all(|event| event.what != 2),
            "WaitMouseUp should consume the queued mouseUp that ended tracking"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    // Button ($A974) — pressed (reads MBState $0172)
    #[test]
    fn test_button_pressed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0x0000);
        bus.write_byte(0x0172, 0x00); // MBState: button down

        let result = disp.dispatch_toolbox(true, 0x174, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    // Button ($A974) — not pressed (reads MBState $0172)
    #[test]
    fn test_button_not_pressed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0xFFFF);
        bus.write_byte(0x0172, 0x80); // MBState: button up

        let result = disp.dispatch_toolbox(true, 0x174, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn test_button_reports_internal_physical_press_when_mbstate_is_stale_up() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0);
        bus.write_byte(0x0172, 0x80); // stale until low-memory sync
        disp.push_mouse_down(50, 100);

        let result = disp.dispatch_toolbox(true, 0x174, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_word(sp),
            0x0100,
            "Button should observe the current internal physical press even if MBState is stale"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn test_button_ignores_paired_queued_mouse_down_after_release() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0xFFFF);
        bus.write_byte(0x0172, 0x80);
        disp.push_mouse_down(50, 100);
        disp.push_mouse_up(50, 100);

        let result = disp.dispatch_toolbox(true, 0x174, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(
            bus.read_word(sp),
            0,
            "a queued mouseDown paired with a later mouseUp must not create a phantom Button press"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    // Button reads MBState ($0172), which is updated by the runner at
    // each tick advance (VBL analog). After push_mouse_up the internal
    // mouse_button is false, but $0172 retains the pressed state until
    // the next tick — matching real hardware VBL latency.
    #[test]
    fn test_button_honors_stale_pressed_mbstate_until_tick_resync() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // Simulate: runner wrote $0172=0x00 on mouse-down
        bus.write_byte(0x0172, 0x00);
        disp.push_mouse_down(50, 100);
        let _ = disp.dequeue_event(&bus, 0xFFFF);
        disp.push_mouse_up(50, 100);

        // Internal state is released, but $0172 is still "pressed"
        // (runner hasn't advanced a tick yet).
        assert!(!disp.input_state.mouse_button_pressed());
        bus.write_word(sp, 0);
        let result = disp.dispatch_toolbox(true, 0x174, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp), 0x0100); // Button sees $0172 = pressed

        // After the runner advances a tick, it would write $0172 = 0x80.
        // Simulate that:
        bus.write_byte(0x0172, 0x80);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0xFFFF);
        let result = disp.dispatch_toolbox(true, 0x174, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp), 0); // Now Button sees released
    }

    // TickCount ($A975)
    #[test]
    fn test_tick_count() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // TickCount observes the guest-visible low-memory Ticks value through
        // the shared process clock. Seed both sides to model a process whose
        // adapters have just been attached.
        bus.write_long(0x016A, 12345);
        disp.set_tick_count_for_test(&mut bus, 12345);

        let result = disp.dispatch_toolbox(true, 0x175, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp), 12345);
        // SP unchanged
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn tickcount_observes_a_direct_low_memory_mutation_between_calls() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let first = 0x0001_2345;
        let second = 0x89AB_CDEF;

        disp.set_tick_count_for_test(&mut bus, first);
        let first_result = disp.dispatch_toolbox(true, 0x175, &mut cpu, &mut bus);
        assert!(first_result.is_some_and(|result| result.is_ok()));
        assert_eq!(bus.read_long(sp), first);

        // Low-memory Ticks is writable guest state. A subsequent call must
        // resolve the same shared semantic operation from the newly written
        // bytes, regardless of which adapter performed the store.
        bus.write_long(crate::memory::globals::addr::TICKS, second);
        cpu.write_reg(Register::A7, sp);
        let second_result = disp.dispatch_toolbox(true, 0x175, &mut cpu, &mut bus);
        assert!(second_result.is_some_and(|result| result.is_ok()));
        assert_eq!(bus.read_long(sp), second);
        assert_eq!(disp.current_tick(), second);
    }

    #[test]
    fn tickcount_returns_self_tick_count_and_does_not_advance_a7() {
        // Per Macintosh Toolbox Essentials 1992 p. 2-112 the Pascal
        // FUNCTION protocol pre-allocates a 4-byte LongInt result
        // slot at SP+0; the trap writes there and the caller pops
        // the result. Specifically the trap must NOT advance A7 —
        // doing so would corrupt the C compiler's expected stack
        // frame on return. This contract test pre-poisons SP+4 with
        // a sentinel and verifies the trap leaves it untouched while
        // writing the result to SP+0.
        //
        // Distinct from `test_tick_count` because that test confirms
        // the value and the SP-non-advance, while this one also
        // proves the trap does not write past the result slot — a
        // regression guard against any future "fix" that uses
        // bus.write_long(sp - 4, ...) or bus.write_long(sp + 4, ...).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0xDEAD_BEEF); // pre-poison result slot
        bus.write_long(sp + 4, 0xCAFE_BABE); // pre-poison past-result slot
        bus.write_long(sp.wrapping_sub(4), 0xFEED_FACE); // pre-poison pre-result slot
        disp.set_tick_count_for_test(&mut bus, 0x0000_4321);

        let result = disp.dispatch_toolbox(true, 0x175, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // The trap wrote the LongInt result to SP+0.
        assert_eq!(bus.read_long(sp), 0x0000_4321);
        // SP+4 sentinel preserved — no over-write past the result slot.
        assert_eq!(bus.read_long(sp + 4), 0xCAFE_BABE);
        // SP-4 sentinel preserved — no under-write before the result slot.
        assert_eq!(bus.read_long(sp.wrapping_sub(4)), 0xFEED_FACE);
        // A7 unchanged: Pascal FUNCTION result slot is consumed by caller.
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn tickcount_returns_monotonically_nondecreasing_across_two_calls() {
        // Per MTE 1992 p. 2-112: "The tick count is incremented during
        // the vertical retrace interrupt"; IM:I I-260 warns to use
        // ">= previous" comparisons. The shared process clock is advanced
        // only by the runner's vertical-retrace policy. This contract test
        // asserts that two consecutive dispatches with that clock advanced
        // between them return monotonic results.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.set_tick_count_for_test(&mut bus, 1_000);

        let r1 = disp.dispatch_toolbox(true, 0x175, &mut cpu, &mut bus);
        assert!(r1.unwrap().is_ok());
        let t1 = bus.read_long(sp);

        // Advance tick globally (mirroring what advance_guest_tick does
        // between trap dispatches in the runner).
        let next_tick = disp.current_tick().wrapping_add(1);
        disp.set_tick_count_for_test(&mut bus, next_tick);

        let r2 = disp.dispatch_toolbox(true, 0x175, &mut cpu, &mut bus);
        assert!(r2.unwrap().is_ok());
        let t2 = bus.read_long(sp);

        assert!(t2 >= t1, "TickCount must be monotonic: t1={t1} t2={t2}");
        assert_eq!(t1, 1_000);
        assert_eq!(t2, 1_001);
    }

    // ========== Scrap Manager Traps ==========

    // Inside Macintosh Volume I, I-457..I-459: InfoScrap returns ScrapStuff
    // fields, ZeroScrap increments scrapCount, and PutScrap contributes bytes
    // to scrapSize.
    #[test]
    fn infoscrap_reports_in_memory_scrapstate_and_entry_size() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0xDEAD_BEEF);
        let zero = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero.is_some());
        assert!(zero.unwrap().is_ok());
        assert_eq!(bus.read_long(sp), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp);

        let source = bus.alloc(3);
        bus.write_bytes(source, b"ABC");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, source);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0xDEAD_BEEF);
        let put = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put.is_some());
        assert!(put.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 12), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info.is_some());
        assert!(info.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp);

        let scrap_info = bus.read_long(sp);
        assert_ne!(scrap_info, 0);
        assert_eq!(bus.read_long(scrap_info), 12); // type(4)+len(4)+padded data(4)
        let scrap_handle = bus.read_long(scrap_info + 4);
        assert_ne!(
            scrap_handle, 0,
            "InfoScrap should expose a live Handle when the scrap is in memory"
        );
        let scrap_ptr = bus.read_long(scrap_handle);
        assert_ne!(
            scrap_ptr, 0,
            "non-empty in-memory scrap should have a non-NIL master-pointer target"
        );
        assert_eq!(bus.get_alloc_size(scrap_ptr), Some(12));
        assert_eq!(bus.read_word(scrap_info + 8), 1); // scrapCount after first ZeroScrap
        assert_eq!(bus.read_word(scrap_info + 10), 1); // positive = in memory (IM:I I-457)
        assert_eq!(bus.read_long(scrap_info + 12), 0); // scrapName = NIL in HLE
    }

    // Inside Macintosh Volume I, I-457: ScrapHandle is a handle to the desk
    // scrap when the scrap is in memory. The serialized bytes are laid out as
    // type(4) + length(4) + data + even-byte padding.
    #[test]
    fn infoscrap_scraphandle_serializes_current_entries() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0);
        let zero = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero.unwrap().is_ok());

        let source = bus.alloc(3);
        bus.write_bytes(source, b"ABC");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, source);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0);
        let put = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info.unwrap().is_ok());

        let scrap_info = bus.read_long(sp);
        let scrap_handle = bus.read_long(scrap_info + 4);
        assert_ne!(scrap_handle, 0);
        let scrap_ptr = bus.read_long(scrap_handle);
        assert_ne!(scrap_ptr, 0);

        let expected = [b'T', b'E', b'X', b'T', 0, 0, 0, 3, b'A', b'B', b'C', 0];
        assert_eq!(
            bus.read_bytes(scrap_ptr, expected.len()),
            expected.as_slice()
        );
        assert_eq!(bus.get_alloc_size(scrap_ptr), Some(expected.len() as u32));
    }

    // Inside Macintosh Volume I, I-458: ZeroScrap clears prior data and changes
    // scrapCount in the InfoScrap record.
    #[test]
    fn zeroscrap_clears_contents_and_changes_scrapcount() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0);
        let zero1 = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero1.unwrap().is_ok());

        let source = bus.alloc(1);
        bus.write_byte(source, b'X');
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, source);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 1);
        bus.write_long(sp + 12, 0);
        let put = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info_before = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info_before.unwrap().is_ok());
        let scrap_info = bus.read_long(sp);
        assert_eq!(bus.read_long(scrap_info), 10); // 8 + padded(1 -> 2)
        assert_eq!(bus.read_word(scrap_info + 8), 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD_BEEF);
        let zero2 = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero2.unwrap().is_ok());
        assert_eq!(bus.read_long(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info_after = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info_after.unwrap().is_ok());
        let scrap_info_after = bus.read_long(sp);
        assert_eq!(bus.read_long(scrap_info_after), 0);
        assert_eq!(bus.read_word(scrap_info_after + 8), 2);
    }

    // Inside Macintosh Volume I, I-459: GetScrap returns byte length on
    // success and reports data offset from start-of-scrap; NIL hDest queries
    // length/offset without copying.
    #[test]
    fn getscrap_with_nil_handle_returns_length_and_data_offset() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0);
        let zero = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero.unwrap().is_ok());

        let text_src = bus.alloc(1);
        bus.write_byte(text_src, b'A');
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, text_src);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 1);
        bus.write_long(sp + 12, 0);
        let put_text = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put_text.unwrap().is_ok());

        let pict_src = bus.alloc(3);
        bus.write_bytes(pict_src, b"XYZ");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, pict_src);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"PICT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0);
        let put_pict = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put_pict.unwrap().is_ok());

        let offset_ptr = bus.alloc(4);
        bus.write_long(offset_ptr, 0xFFFF_FFFF);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, offset_ptr);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"PICT"));
        bus.write_long(sp + 8, 0); // NIL handle query path
        bus.write_long(sp + 12, 0xDEAD_BEEF);
        let get = disp.dispatch_toolbox(true, 0x1FD, &mut cpu, &mut bus);
        assert!(get.is_some());
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 12), 3);
        assert_eq!(bus.read_long(offset_ptr), 18); // TEXT entry(10) + PICT header(8)
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    // Inside Macintosh Volume I, I-459: GetScrap returns noTypeErr (-102)
    // when no data of the requested type exists.
    #[test]
    fn getscrap_missing_type_returns_notypeerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        let offset_ptr = bus.alloc(4);
        bus.write_long(offset_ptr, 0x1234_5678);
        bus.write_long(sp, offset_ptr);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 0);
        bus.write_long(sp + 12, 0);

        let get = disp.dispatch_toolbox(true, 0x1FD, &mut cpu, &mut bus);
        assert!(get.is_some());
        assert!(get.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 12) as i32, -102);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    // Inside Macintosh Volume I, I-459 warning: duplicate PutScrap type entries
    // append, and GetScrap returns the first matching type.
    #[test]
    fn getscrap_duplicate_type_returns_first_occurrence() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0);
        let zero = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero.unwrap().is_ok());

        let old_src = bus.alloc(3);
        bus.write_bytes(old_src, b"OLD");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, old_src);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0);
        let put_old = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put_old.unwrap().is_ok());

        let new_src = bus.alloc(3);
        bus.write_bytes(new_src, b"NEW");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, new_src);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0);
        let put_new = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put_new.unwrap().is_ok());

        let offset_ptr = bus.alloc(4);
        let h_dest = bus.alloc(4);
        bus.write_long(h_dest, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, offset_ptr);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, h_dest);
        bus.write_long(sp + 12, 0xDEAD_BEEF);
        let get = disp.dispatch_toolbox(true, 0x1FD, &mut cpu, &mut bus);
        assert!(get.is_some());
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 12), 3);
        assert_eq!(bus.read_long(offset_ptr), 8);
        let data_ptr = bus.read_long(h_dest);
        assert_ne!(data_ptr, 0);
        let bytes = [
            bus.read_byte(data_ptr),
            bus.read_byte(data_ptr + 1),
            bus.read_byte(data_ptr + 2),
        ];
        assert_eq!(&bytes, b"OLD");
    }

    // Inside Macintosh Volume I, I-459: given an existing minimum-size
    // handle, GetScrap resizes it to hold the copied bytes and leaves the
    // copied block owned by that same handle.
    #[test]
    fn getscrap_existing_handle_resizes_copy_and_preserves_ownership() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0);
        let zero = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero.unwrap().is_ok());

        let text_src = bus.alloc(3);
        bus.write_bytes(text_src, b"OLD");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, text_src);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0);
        let put = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::D0, 1);
        let new_handle = disp.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
        assert!(new_handle.is_some());
        assert!(new_handle.unwrap().is_ok());
        let h_dest = cpu.read_reg(Register::A0);
        let old_ptr = bus.read_long(h_dest);
        assert_ne!(old_ptr, 0);
        bus.write_byte(old_ptr, b'Z');

        let offset_ptr = bus.alloc(4);
        bus.write_long(offset_ptr, 0xFFFF_FFFF);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, offset_ptr);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, h_dest);
        bus.write_long(sp + 12, 0xDEAD_BEEF);
        let get = disp.dispatch_toolbox(true, 0x1FD, &mut cpu, &mut bus);
        assert!(get.is_some());
        assert!(get.unwrap().is_ok());

        let data_ptr = bus.read_long(h_dest);
        assert_ne!(data_ptr, 0);
        assert_eq!(bus.read_long(sp + 12), 3);
        assert_eq!(bus.read_long(offset_ptr), 8);
        assert_eq!(bus.get_alloc_size(data_ptr), Some(3));
        assert_eq!(bus.read_bytes(data_ptr, 3), b"OLD");

        cpu.write_reg(Register::A0, data_ptr);
        let recover = disp.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
        assert!(recover.is_some());
        assert!(recover.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A0), h_dest);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Inside Macintosh Volume I, I-458: UnloadScrap/LoadScrap return noErr on
    // success and the ScrapStuff record reflects the resident/on-disk state.
    #[test]
    fn unloadscrap_and_loadscrap_return_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0);
        let zero = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero.is_some());
        assert!(zero.unwrap().is_ok());

        let source = bus.alloc(3);
        bus.write_bytes(source, b"ABC");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, source);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0);
        let put = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put.is_some());
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info_before = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info_before.is_some());
        assert!(info_before.unwrap().is_ok());
        let scrap_info_before = bus.read_long(sp);
        let scrap_handle_before = bus.read_long(scrap_info_before + 4);
        assert_ne!(scrap_handle_before, 0);
        assert_eq!(bus.read_word(scrap_info_before + 10), 1);
        assert_eq!(bus.read_long(scrap_info_before), 12);
        assert_eq!(
            bus.get_alloc_size(bus.read_long(scrap_handle_before)),
            Some(12)
        );

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD_BEEF);
        let unload = disp.dispatch_toolbox(true, 0x1FA, &mut cpu, &mut bus);
        assert!(unload.is_some());
        assert!(unload.unwrap().is_ok());
        assert_eq!(bus.read_long(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info_after_unload = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info_after_unload.is_some());
        assert!(info_after_unload.unwrap().is_ok());
        let scrap_info_after_unload = bus.read_long(sp);
        assert_eq!(bus.read_long(scrap_info_after_unload), 12);
        assert_eq!(bus.read_long(scrap_info_after_unload + 4), 0);
        assert_eq!(bus.read_word(scrap_info_after_unload + 10), 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD_BEEF);
        let load = disp.dispatch_toolbox(true, 0x1FB, &mut cpu, &mut bus);
        assert!(load.is_some());
        assert!(load.unwrap().is_ok());
        assert_eq!(bus.read_long(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info_after_load = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info_after_load.is_some());
        assert!(info_after_load.unwrap().is_ok());
        let scrap_info_after_load = bus.read_long(sp);
        let scrap_handle_after_load = bus.read_long(scrap_info_after_load + 4);
        assert_ne!(scrap_handle_after_load, 0);
        assert_eq!(bus.read_long(scrap_info_after_load), 12);
        assert_eq!(bus.read_word(scrap_info_after_load + 10), 1);
        assert_eq!(
            bus.get_alloc_size(bus.read_long(scrap_handle_after_load)),
            Some(12)
        );
    }

    // Inside Macintosh Volume I, I-458: if the clipboard destination is not
    // writable, UnloadScrap can fail without dropping the resident scrap.
    #[test]
    fn unloadscrap_returns_error_and_keeps_scrap_resident_when_clipboard_unwritable() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        disp.scrap.set_clipboard_writable(false);

        bus.write_long(sp, 0);
        let zero = disp.dispatch_toolbox(true, 0x1FC, &mut cpu, &mut bus);
        assert!(zero.is_some());
        assert!(zero.unwrap().is_ok());

        let source = bus.alloc(3);
        bus.write_bytes(source, b"ABC");
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, source);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEXT"));
        bus.write_long(sp + 8, 3);
        bus.write_long(sp + 12, 0);
        let put = disp.dispatch_toolbox(true, 0x1FE, &mut cpu, &mut bus);
        assert!(put.is_some());
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info_before = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info_before.is_some());
        assert!(info_before.unwrap().is_ok());
        let scrap_info_before = bus.read_long(sp);
        let scrap_handle_before = bus.read_long(scrap_info_before + 4);
        assert_ne!(scrap_handle_before, 0);
        assert_eq!(bus.read_word(scrap_info_before + 10), 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xDEAD_BEEF);
        let unload = disp.dispatch_toolbox(true, 0x1FA, &mut cpu, &mut bus);
        assert!(unload.is_some());
        assert!(unload.unwrap().is_ok());
        assert_ne!(bus.read_long(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0);
        let info_after_unload = disp.dispatch_toolbox(true, 0x1F9, &mut cpu, &mut bus);
        assert!(info_after_unload.is_some());
        assert!(info_after_unload.unwrap().is_ok());
        let scrap_info_after_unload = bus.read_long(sp);
        assert_eq!(
            bus.read_long(scrap_info_after_unload + 4),
            scrap_handle_before
        );
        assert_eq!(bus.read_word(scrap_info_after_unload + 10), 1);
        assert_eq!(bus.read_long(scrap_info_after_unload), 12);
    }

    // Inside Macintosh Volume I (1985), p. I-458: LoadScrap is a 0-arg Tool
    // Trap FUNCTION returning LONGINT (OSStatus) via the Pascal function
    // result slot at [SP+0]; the documented "already in memory" success
    // path returns noErr without consuming caller stack bytes. This test
    // pre-poisons the 4-byte result slot at SP+0 with a non-zero sentinel
    // and asserts the trap overwrites it with 0 (noErr) while leaving A7
    // untouched.
    #[test]
    fn loadscrap_writes_noerr_to_pascal_function_result_slot_and_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0xCAFE_F00D);
        bus.write_long(sp + 4, 0xBAAD_F00D);

        let result = disp.dispatch_toolbox(true, 0x1FB, &mut cpu, &mut bus);
        assert!(result.is_some(), "LoadScrap should be handled");
        assert!(result.unwrap().is_ok(), "LoadScrap should return cleanly");

        assert_eq!(
            bus.read_long(sp),
            0,
            "LoadScrap should write noErr to the 4-byte Pascal FUNCTION result slot at [SP+0]"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "LoadScrap should leave A7 unchanged (0-arg Tool Trap function)"
        );
        assert_eq!(
            bus.read_long(sp + 4),
            0xBAAD_F00D,
            "LoadScrap should not write past the 4-byte result slot"
        );
    }

    // ========== Printing Manager ==========

    #[test]
    fn prglue_generated_routes_preserve_exact_stack_long_values() {
        assert_eq!(super::PR_GLUE_OPERATION_ROUTES.len(), 23);
        assert!(super::PR_GLUE_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0400_0C00, "PrOpenDoc"),
            (0x0800_0484, "PrCloseDoc"),
            (0x1000_0808, "PrOpenPage"),
            (0x1800_040C, "PrClosePage"),
            (0x2004_0480, "PrintDefault"),
            (0x2A04_0484, "PrStlDialog"),
            (0x3204_0488, "PrJobDialog"),
            (0x3C04_040C, "PrStlInit"),
            (0x4404_0410, "PrJobInit"),
            (0x4A04_0894, "PrDlgMain"),
            (0x5204_0498, "PrValidate"),
            (0x5804_089C, "PrJobMerge"),
            (0x6005_1480, "PrPicFile"),
            (0x7007_0480, "PrGeneral"),
            (0x8000_0000, "PrDrvrOpen"),
            (0x8800_0000, "PrDrvrClose"),
            (0x9400_0000, "PrDrvrDCE"),
            (0x9A00_0000, "PrDrvrVers"),
            (0xA000_0E00, "PrCtlCall"),
            (0xBA00_0000, "PrError"),
            (0xC000_0200, "PrSetError"),
            (0xC800_0000, "PrOpen"),
            (0xD000_0000, "PrClose"),
        ] {
            let route =
                super::pr_glue_operation_route(0xA8FD, selector).expect("PrGlue operation route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA9FD, 0xC800_0000),
            (0xA8FD, 0xF100_0600),
            (0xA8FD, 0x0000_2F3C),
            (0xA8FD, 0x0000_00C8),
            (0xA8FD, 0x0000_00BA),
        ] {
            assert!(super::pr_glue_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn prglue_records_stack_long_identity_without_changing_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA8FD;
        bus.write_long(sp, 0xC800_0000);

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.expect("PrGlue arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_PrGlue:0xC8000000:stack-long-immediate:32")
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        disp.current_trap_word = 0xA9FD;
        cpu.write_reg(Register::A7, sp);
        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.expect("PrGlue arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        disp.current_trap_word = 0xA8FD;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xC800_0001);
        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.expect("PrGlue arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn prglue_selector_param_byte_count_controls_stack_pop() {
        // Inside Macintosh Volume V (1986), p. V-408:
        // _PrGlue selectors encode parameter-byte count in bits 15-8.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0xF100_0600); // routine=$F1, result=0, params=6
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        bus.write_word(sp + 8, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn prglue_propendoc_returns_nil_and_consumes_three_pointer_arguments() {
        // Inside Macintosh Volume V (1986), p. V-408:
        // PrOpenDoc selector is $04000C00 and signature takes 12 bytes of
        // arguments (THPrint, TPPrPort, Ptr) and returns TPPrPort.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0400_0C00);
        bus.write_long(sp + 4, 0x1000_2000);
        bus.write_long(sp + 8, 0x2000_3000);
        bus.write_long(sp + 12, 0x3000_4000);
        bus.write_long(sp + 16, 0xFFFF_FFFF); // result slot placeholder

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 16), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
    }

    #[test]
    fn prglue_prvalidate_returns_false_boolean_result() {
        // Inside Macintosh Volume V (1986), p. V-408:
        // PrValidate selector is $52040498 and returns a BOOLEAN.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x5204_0498);
        bus.write_long(sp + 4, 0x1234_5678); // hPrint
        bus.write_word(sp + 8, 0xFFFF); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn prglue_prstldialog_returns_true_boolean_result() {
        // Inside Macintosh Volume V (1986), p. V-408:
        // PrStlDialog selector is $2A040484 and returns a BOOLEAN.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x2A04_0484);
        bus.write_long(sp + 4, 0x1234_5678); // hPrint
        bus.write_word(sp + 8, 0); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), 1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn prglue_prjobdialog_returns_true_boolean_result() {
        // Inside Macintosh Volume V (1986), p. V-408:
        // PrJobDialog selector is $32040488 and returns a BOOLEAN.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x3204_0488);
        bus.write_long(sp + 4, 0x1234_5678); // hPrint
        bus.write_word(sp + 8, 0); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), 1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn prglue_prclosedoc_consumes_tpprport_argument_without_function_result_slot() {
        // Inside Macintosh Volume II (1985), p. II-160; Inside Macintosh
        // Volume V (1986), p. V-408:
        // PrCloseDoc takes one TPPrPort argument and returns no result.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0800_0484);
        bus.write_long(sp + 4, 0x0000_0000); // pPrPort = NIL
        bus.write_long(sp + 8, 0xCAFE_BABE); // sentinel past the argument frame

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_long(sp + 8), 0xCAFE_BABE);
    }

    #[test]
    fn prglue_printdefault_consumes_thprint_without_function_result_slot() {
        // Inside Macintosh: Imaging With QuickDraw (1994), p. 9-60:
        // PrintDefault is PROCEDURE PrintDefault(hPrint) with selector
        // $20040480. The selector's second byte is not a stack-result size.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x2004_0480);
        bus.write_long(sp + 4, 0x0010_2000); // hPrint
        bus.write_long(sp + 8, 0xCAFE_BABE); // saved-frame sentinel

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_long(sp + 8), 0xCAFE_BABE);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn prglue_propen_and_prclose_selector_pops_selector_long() {
        // Inside Macintosh Volume II (1985), p. II-151:
        // PrOpen and PrClose are procedures with no stack arguments,
        // but the raw selector dispatch still consumes the selector
        // long pushed for _PrGlue.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0xC800_0000);
        let open = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(open.is_some());
        assert!(open.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xD000_0000);
        let close = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(close.is_some());
        assert!(close.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn prglue_propen_and_prclose_preserve_print_error_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0xC000_0200);
        bus.write_word(sp + 4, 0x1357);
        let set_error = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(set_error.is_some());
        assert!(set_error.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xC800_0000);
        let open = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(open.is_some());
        assert!(open.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xD000_0000);
        let close = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(close.is_some());
        assert!(close.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xBA00_0000);
        bus.write_word(sp + 4, 0);
        let error = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(error.is_some());
        assert!(error.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0x1357);
    }

    #[test]
    fn prglue_prerror_returns_noerr_word_with_zero_result_bits_selector() {
        // Inside Macintosh Volume V (1986), p. V-408:
        // PrError selector is $BA000000 and returns INTEGER from the
        // Printing Manager error state.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0xBA00_0000);
        bus.write_word(sp + 4, 0xFFFF); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn prglue_prseterror_consumes_ierr_word_without_function_result_slot() {
        // Inside Macintosh Volume V (1986), p. V-408:
        // PrSetError selector is $C0000200 and takes one INTEGER argument.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0xC000_0200);
        bus.write_word(sp + 4, 0xABCD); // iErr
        bus.write_word(sp + 6, 0xCAFE); // sentinel after arguments

        let result = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0xCAFE);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn prglue_prseterror_updates_prerror_state_roundtrip() {
        // Inside Macintosh Volume II (1985), p. II-161: PrSetError
        // stores the shared PrintErr result code, and PrError returns it.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // First set a non-zero error and verify PrError reports it.
        bus.write_long(sp, 0xC000_0200);
        bus.write_word(sp + 4, 0x1234);
        let set_nonzero = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(set_nonzero.is_some());
        assert!(set_nonzero.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xBA00_0000);
        bus.write_word(sp + 4, 0xFFFF);
        let get_nonzero = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(get_nonzero.is_some());
        assert!(get_nonzero.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0x1234);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        // Reset to noErr and verify PrError follows the cleared state.
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xC000_0200);
        bus.write_word(sp + 4, 0x0000);
        let set_zero = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(set_zero.is_some());
        assert!(set_zero.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0xBA00_0000);
        bus.write_word(sp + 4, 0xFFFF);
        let get_zero = disp.dispatch_toolbox(true, 0x0FD, &mut cpu, &mut bus);
        assert!(get_zero.is_some());
        assert!(get_zero.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0x0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // ========== Utility Traps ==========

    #[test]
    fn bitshift_large_positive_count_zeroes_on_basiliskii() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 36);
        bus.write_long(sp + 2, 1);
        bus.write_long(sp + 6, 0xBEEFBEEF);

        let result = disp.dispatch_toolbox(true, 0x05C, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    // Random ($A861) — seed=12345
    #[test]
    fn test_random() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // globals_ptr is at bus.read_long(A5) = bus.read_long(0x180000) = 0x180004
        // randSeed is at globals_ptr - 126 = 0x180004 - 126 = 0x17FF86
        let seed_addr = 0x180004u32.wrapping_sub(126);
        bus.write_long(seed_addr, 12345);

        let result = disp.dispatch_toolbox(true, 0x061, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // Park-Miller: new_seed = (12345 * 16807) % 2147483647 = 207482415
        let new_seed = bus.read_long(seed_addr);
        assert_eq!(new_seed, 207482415);
        // Result = low 16 bits of new_seed = 207482415 & 0xFFFF = 36399
        let result_word = bus.read_word(sp);
        assert_eq!(result_word, 207482415u32 as u16); // 36399
                                                      // SP unchanged
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    // HiWord ($A86A) / LoWord ($A86B)
    // IM:I I-472 and OS Utils 1994 p.3-18: extract high/low word from LONGINT.
    #[test]
    fn hiword_returns_high_order_word_and_pops_long_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x89ABCDEF);
        bus.write_word(sp + 4, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x06A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0x89AB);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn loword_returns_low_order_word_and_pops_long_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x89ABCDEF);
        bus.write_word(sp + 4, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x06B, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0xCDEF);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // Random ($A861)
    // IM:I I-194 and OS Utils 1994 p.3-37: result depends solely on randSeed,
    // and the pseudorandom sequence is repeatable when randSeed is reset.
    #[test]
    fn random_reseeding_to_same_value_repeats_sequence_head() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let seed_addr = 0x180004u32.wrapping_sub(126);

        bus.write_long(seed_addr, 1);
        let first = disp.dispatch_toolbox(true, 0x061, &mut cpu, &mut bus);
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());
        let first_result = bus.read_word(sp);
        let first_seed = bus.read_long(seed_addr);
        assert_eq!(first_seed, 16807);

        let second = disp.dispatch_toolbox(true, 0x061, &mut cpu, &mut bus);
        assert!(second.is_some());
        assert!(second.unwrap().is_ok());
        let second_result = bus.read_word(sp);
        assert_ne!(second_result, first_result);

        bus.write_long(seed_addr, 1);
        let replay = disp.dispatch_toolbox(true, 0x061, &mut cpu, &mut bus);
        assert!(replay.is_some());
        assert!(replay.unwrap().is_ok());
        assert_eq!(bus.read_word(sp), first_result);
        assert_eq!(bus.read_long(seed_addr), first_seed);
    }

    // IM:I I-194 says Random returns -32767..32767 (not -32768).
    #[test]
    fn random_maps_minus_32768_slot_to_zero() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let seed_addr = 0x180004u32.wrapping_sub(126);

        // Chosen so Park-Miller update produces a low word of 0x8000.
        bus.write_long(seed_addr, 32768);

        let result = disp.dispatch_toolbox(true, 0x061, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(seed_addr), 550731776);
        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    // NOTE: The former test_pt_to_angle_cardinal_directions test exercised
    // a duplicate PtToAngle stub in toolbox.rs that popped 16 bytes and
    // treated Rect as an inline record. The canonical PtToAngle now lives
    // exclusively in quickdraw.rs (dispatch_quickdraw arm 0x0C3) with the
    // correct 12-byte frame (rect passed by pointer). Its contract tests
    // are in pttoangle_*.

    // GetIndString ($A9E6)
    #[test]
    fn test_get_ind_string() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let str_ptr = 0x200000u32;
        // SP+0: ...(4 bytes), SP+4: str_ptr(4)
        bus.write_long(sp, 0); // first 4 bytes (index, resID, etc.)
        bus.write_long(sp + 4, str_ptr);
        // Write non-zero to str_ptr to verify it gets cleared
        bus.write_byte(str_ptr, 0xFF);

        let result = disp.dispatch_toolbox(true, 0x1E6, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(str_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // Inside Macintosh Volume I (1985), p. I-468: GetIndString reads the
    // indexed string from a STR# resource via GetResource. A successful hit
    // must therefore copy the Pascal-string body and clear stale ResErr.
    #[test]
    fn get_ind_string_present_resource_copies_indexed_string_and_clears_reserr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let str_ptr = 0x200120u32;
        let data_ptr = bus.alloc(16);

        // STR# layout: count=2, then PString "ONE", then PString "TWO".
        bus.write_word(data_ptr, 2);
        bus.write_byte(data_ptr + 2, 3);
        bus.write_bytes(data_ptr + 3, b"ONE");
        bus.write_byte(data_ptr + 6, 3);
        bus.write_bytes(data_ptr + 7, b"TWO");

        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([
                (0, ResourceFileMap::default()),
                (
                    2,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"STR#", 128i16), data_ptr)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::new(),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: HashMap::new(),
            search_order: vec![0, 2],
            current_file: 2,
        });

        bus.write_byte(str_ptr, 0xFF);
        bus.write_word(0x0A60, 0xBEEF);
        bus.write_word(sp, 2); // index
        bus.write_word(sp + 2, 128u16); // STR# id
        bus.write_long(sp + 4, str_ptr);

        let result = disp.dispatch_toolbox(true, 0x1E6, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_pstring(str_ptr), b"TWO");
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // ========== Sound Manager ==========

    // SndPlay ($A805)
    #[test]
    fn test_snd_play() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // SP+0: async(2), SP+2: sndHdl(4), SP+6: chan(4), SP+10: result(2)
        bus.write_word(sp, 0); // async
        bus.write_long(sp + 2, 0); // sndHdl (nil = no sound to play)
        bus.write_long(sp + 6, 0); // chan
        bus.write_word(sp + 10, 0xBEEF); // result placeholder

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), (-204i16) as u16); // resProblem
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn test_snd_play_nil_chan_async_true_reclaims_internal_channel() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x200000u32;
        let snd_ptr = 0x200100u32;

        // Minimal format 2 'snd ' resource:
        //   +0 format=2, +2 refCount=0, +4 numCommands=0.
        bus.write_long(snd_handle, snd_ptr);
        bus.write_word(snd_ptr, 2);
        bus.write_word(snd_ptr + 2, 0);
        bus.write_word(snd_ptr + 4, 0);

        // IM:Sound 1994 p.2-122: if chan is NIL, async is ignored and play
        // is synchronous (must not fail solely because async=TRUE).
        bus.write_word(sp, 0xFFFF); // async = TRUE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0); // chan = NIL
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert!(disp.sound_manager.channels.is_empty());
    }

    #[test]
    fn test_snd_play_unloaded_handle_returns_resproblem() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x200000u32;

        // Handle master pointer is NIL => resource is not loaded.
        // IM:Sound 1994 p.2-122 says SndPlay returns resProblem (-204).
        bus.write_long(snd_handle, 0);
        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), (-204i16) as u16); // resProblem
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert!(disp.sound_manager.channels.is_empty());
    }

    #[test]
    fn test_snd_play_invalid_resource_format_returns_badformat() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let snd_handle = 0x200000u32;
        let snd_ptr = 0x200100u32;

        bus.write_long(snd_handle, snd_ptr);
        bus.write_word(snd_ptr, 3); // unsupported format (valid values: 1 or 2)
        bus.write_word(sp, 0); // async = FALSE
        bus.write_long(sp + 2, snd_handle);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_sound(true, 0x005, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // IM:Sound 1994 p.2-123 result codes: malformed/corrupt resource -> badFormat.
        assert_eq!(bus.read_word(sp + 10), (-206i16) as u16); // badFormat
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert!(disp.sound_manager.channels.is_empty());
    }

    // SysBeep ($A9C8)
    #[test]
    fn test_sys_beep() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 30); // duration param

        let result = disp.dispatch_sound(true, 0x1C8, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    // SystemClick ($A9B3)
    #[test]
    fn systemclick_uses_pointer_frame_and_preserves_pascal_registers() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_ptr = bus.alloc(16);
        let window_ptr = bus.alloc(4);
        for offset in 0..16 {
            bus.write_byte(event_ptr + offset, 0x40u8.wrapping_add(offset as u8));
        }
        bus.write_long(sp - 4, 0xDEAD_BEEF);
        bus.write_long(sp, window_ptr);
        bus.write_long(sp + 4, event_ptr);
        bus.write_long(sp + 8, 0xCAFE_BABE);
        bus.write_long(sp + 20, 0x5A5A_C0DE); // catches the former frame end
        let preserved = [
            (Register::D3, 0xD300_0003),
            (Register::D4, 0xD400_0004),
            (Register::D5, 0xD500_0005),
            (Register::D6, 0xD600_0006),
            (Register::D7, 0xD700_0007),
            (Register::A2, 0xA200_0002),
            (Register::A3, event_ptr),
            (Register::A4, 0xA400_0004),
            (Register::A5, 0xA500_0005),
            (Register::A6, 0xA600_0006),
        ];
        for (register, value) in preserved {
            cpu.write_reg(register, value);
        }

        let result = disp.dispatch_toolbox(true, 0x1B3, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp - 4), 0xDEAD_BEEF);
        assert_eq!(bus.read_long(sp), window_ptr);
        assert_eq!(bus.read_long(sp + 4), event_ptr);
        assert_eq!(bus.read_long(sp + 8), 0xCAFE_BABE);
        assert_eq!(bus.read_long(sp + 20), 0x5A5A_C0DE);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        for (register, value) in preserved {
            assert_eq!(
                cpu.read_reg(register),
                value,
                "stack-based SystemClick must preserve {register:?}"
            );
        }
    }

    #[test]
    fn systemclick_five_call_composition_advances_stack_by_forty() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        for call in 0..5u32 {
            let frame = sp + call * 8;
            bus.write_long(frame, 0x0024_0000 + call * 4); // WindowPtr
            bus.write_long(frame + 4, 0x0025_0000 + call * 16); // EventRecord pointer
        }
        bus.write_long(sp + 40, 0xCAFE_BABE);

        for call in 0..5u32 {
            let result = disp.dispatch_toolbox(true, 0x1B3, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::A7), sp + (call + 1) * 8);
        }
        assert_eq!(bus.read_long(sp + 40), 0xCAFE_BABE);
    }

    // SystemTask ($A9B4)
    #[test]
    fn systemtask_procedure_call_preserves_stack_pointer() {
        // IM:I 1985, p. I-442: PROCEDURE SystemTask;
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        assert!(
            !disp.system_task_has_periodic_work(),
            "the transparent path is valid only while no periodic DA/driver work is modeled"
        );

        let result = disp.dispatch_toolbox(true, 0x1B4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SystemTask is a no-argument procedure and must preserve A7"
        );
    }

    // SystemTask ($A9B4) — a 5-call composition catches per-call drift a
    // single call might mask.
    #[test]
    fn systemtask_five_call_composition_preserves_stack_pointer() {
        // IM:I 1985, p. I-442: PROCEDURE SystemTask;
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        for _ in 0..5 {
            let result = disp.dispatch_toolbox(true, 0x1B4, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SystemTask must preserve A7 across a 5-call composition"
        );
    }

    // OpenDeskAcc ($A9B6)
    #[test]
    fn opendeskacc_consumes_name_pointer_and_returns_zero_refnum_in_result_slot() {
        // IM:I 1985, p. I-440: FUNCTION OpenDeskAcc(theAcc: Str255): INTEGER;
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x240000u32;
        bus.write_pstring(name_ptr, b"Calculator");
        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF); // INTEGER function-result slot
        bus.write_word(sp + 6, 0xCAFE); // trailing sentinel

        let result = disp.dispatch_toolbox(true, 0x1B6, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(
            bus.read_word(sp + 4),
            0,
            "OpenDeskAcc HLE path writes 0 into the INTEGER result slot"
        );
        assert_eq!(
            bus.read_word(sp + 6),
            0xCAFE,
            "OpenDeskAcc must pop only the 4-byte name pointer argument"
        );
    }

    // OpenDeskAcc ($A9B6) — 5-call composition: net stack effect after each
    // Pascal FUNCTION call's epilogue is zero, so A7 returns to its
    // pre-composition value.
    #[test]
    fn opendeskacc_five_call_composition_preserves_stack_pointer() {
        // IM:I 1985, p. I-440: FUNCTION OpenDeskAcc(theAcc: Str255): INTEGER;
        let (mut disp, mut cpu, mut bus) = setup();
        let name_ptr = 0x240000u32;
        bus.write_pstring(name_ptr, b"NoSuchDA_A9B6!");

        // Each call: caller pushes 2-byte result placeholder + 4-byte name
        // ptr, dispatches, trap pops the 4-byte arg + writes the 2-byte
        // result slot, then caller pops the 2-byte result slot.
        let sp_before = cpu.read_reg(Register::A7);
        for _ in 0..5 {
            let call_sp = cpu.read_reg(Register::A7);
            bus.write_long(call_sp.wrapping_sub(4), name_ptr);
            bus.write_word(call_sp.wrapping_sub(6), 0xBEEF);
            cpu.write_reg(Register::A7, call_sp.wrapping_sub(6));

            let result = disp.dispatch_toolbox(true, 0x1B6, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());

            // Caller's epilogue pops the 2-byte INTEGER result slot.
            let post_sp = cpu.read_reg(Register::A7);
            cpu.write_reg(Register::A7, post_sp.wrapping_add(2));
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "OpenDeskAcc Pascal FUNCTION calling convention must preserve A7 across a 5-call composition"
        );
    }

    // CloseDeskAcc ($A9B7)
    #[test]
    fn closedeskacc_consumes_refnum_arg_and_writes_no_result() {
        // IM:I 1985, p. I-440: PROCEDURE CloseDeskAcc(refNum: INTEGER);
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0); // refNum=0 (clearly invalid — no-op path)
        bus.write_word(sp + 2, 0xCAFE); // trailing sentinel

        let result = disp.dispatch_toolbox(true, 0x1B7, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(
            bus.read_word(sp + 2),
            0xCAFE,
            "CloseDeskAcc must pop exactly 2 bytes and not overwrite trailing stack memory"
        );
    }

    // CloseDeskAcc ($A9B7) — 5-call composition: each call pops 2 bytes so
    // A7 advances by 10.
    #[test]
    fn closedeskacc_five_call_composition_advances_stack_by_ten() {
        // IM:I 1985, p. I-440: PROCEDURE CloseDeskAcc(refNum: INTEGER);
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        // Caller pre-pushes 5 × 2-byte refNum=0 arguments.
        for i in 0..5u32 {
            bus.write_word(sp_before.wrapping_sub(10 - 2 * i), 0);
        }
        cpu.write_reg(Register::A7, sp_before.wrapping_sub(10));

        for _ in 0..5 {
            let result = disp.dispatch_toolbox(true, 0x1B7, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "CloseDeskAcc must pop exactly 2 bytes per call (Pascal PROCEDURE)"
        );
    }

    // SystemMenu ($A9B5) — single call.
    // Per IM:I 1985, p. I-441: PROCEDURE SystemMenu(menuResult: LONGINT) pops
    // a 4-byte LONGINT argument and writes no result slot.
    #[test]
    fn systemmenu_procedure_call_pops_four_bytes_from_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0); // menuResult=0 (no DA matches menuID=0)
        bus.write_word(sp + 4, 0xCAFE); // trailing sentinel

        let result = disp.dispatch_toolbox(true, 0x1B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(
            bus.read_word(sp + 4),
            0xCAFE,
            "SystemMenu must pop exactly 4 bytes and not overwrite trailing stack memory"
        );
    }

    // SystemMenu ($A9B5) — 5-call composition.
    // Each call pops 4 bytes so A7 advances by 20 after 5 dispatches.
    #[test]
    fn systemmenu_five_call_composition_advances_stack_by_twenty() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        // Caller pre-pushes 5 × 4-byte LONGINT menuResult=0 arguments.
        for i in 0..5u32 {
            bus.write_long(sp_before.wrapping_sub(20 - 4 * i), 0);
        }
        cpu.write_reg(Register::A7, sp_before.wrapping_sub(20));

        for _ in 0..5 {
            let result = disp.dispatch_toolbox(true, 0x1B5, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
        }
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SystemMenu must pop exactly 4 bytes per call (Pascal PROCEDURE pop-LONGINT)"
        );
    }

    // SystemEdit ($A9C2)
    #[test]
    fn systemedit_consumes_editcmd_and_returns_false_boolean_result() {
        // IM:I 1985, p. I-441:
        // FUNCTION SystemEdit(editCmd: INTEGER): BOOLEAN;
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 3); // copyCmd
        bus.write_word(sp + 2, 0xFFFF); // BOOLEAN result slot sentinel
        bus.write_word(sp + 4, 0xCAFE); // trailing sentinel

        let result = disp.dispatch_toolbox(true, 0x1C2, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(
            bus.read_word(sp + 2),
            0,
            "SystemEdit should return FALSE when no desk accessory handles edit commands"
        );
        assert_eq!(
            bus.read_word(sp + 4),
            0xCAFE,
            "SystemEdit must pop only the 2-byte editCmd argument"
        );
    }

    // SystemEdit ($A9C2) — FALSE return + 2-byte arg pop for every standard
    // editCmd per IM:I I-441.
    #[test]
    fn systemedit_returns_false_for_every_standard_editcmd() {
        // IM:I 1985, p. I-441 table:
        //   0  undoCmd
        //   2  cutCmd
        //   3  copyCmd
        //   4  pasteCmd
        //   5  clearCmd
        // (1 is a historic gap.)
        for edit_cmd in [0u16, 2, 3, 4, 5] {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            bus.write_word(sp, edit_cmd);
            bus.write_word(sp + 2, 0xFFFF); // BOOLEAN result slot sentinel
            bus.write_word(sp + 4, 0xCAFE); // trailing sentinel

            let result = disp.dispatch_toolbox(true, 0x1C2, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp + 2,
                "SystemEdit(editCmd={edit_cmd}) must pop exactly 2 bytes"
            );
            assert_eq!(
                bus.read_word(sp + 2),
                0,
                "SystemEdit(editCmd={edit_cmd}) must return FALSE in the no-DA path"
            );
            assert_eq!(
                bus.read_word(sp + 4),
                0xCAFE,
                "SystemEdit(editCmd={edit_cmd}) must not overwrite trailing stack"
            );
        }
    }

    fn seed_synthetic_kchr(bus: &mut super::MacMemoryBus, trans_data: u32) {
        // Minimal KCHR layout:
        //   bytes 0..=1 = version
        //   bytes 2..=257 = table-selection index
        //   table-count word
        //   table 0 and table 1 = 128-byte character tables
        //   dead-key-count word
        //
        // Modifier byte 0 selects table 0; modifier byte 1 selects table 1.
        bus.write_word(trans_data, 0);
        for i in 0..256u32 {
            bus.write_byte(trans_data + 2 + i, 0);
        }
        bus.write_byte(trans_data + 2, 0);
        bus.write_byte(trans_data + 3, 1);

        bus.write_word(trans_data + 2 + 256, 2);
        let table0 = trans_data + 2 + 256 + 2;
        let table1 = table0 + 128;
        bus.write_byte(table0 + 2, b'Q');
        bus.write_byte(table0 + 3, b'W');
        bus.write_byte(table0 + 4, b'E');
        bus.write_byte(table0 + 5, b'R');
        bus.write_byte(table0 + 6, b'T');
        bus.write_byte(table1 + 2, b'Z');
        bus.write_byte(table1 + 3, b'X');
        bus.write_byte(table1 + 4, b'C');
        bus.write_byte(table1 + 5, b'V');
        bus.write_byte(table1 + 6, b'B');
        bus.write_word(table1 + 128, 0);
    }

    // KeyTrans ($A9C3)
    #[test]
    fn keytrans_consumes_state_keycode_transdata_arguments_and_writes_long_result_slot() {
        // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 2-110:
        // FUNCTION KeyTranslate(transData: Ptr; keycode: Integer;
        //                       VAR state: LongInt): LongInt;
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let state_ptr = 0x230000u32;
        let trans_data = 0x240000u32;
        seed_synthetic_kchr(&mut bus, trans_data);
        bus.write_long(state_ptr, 0x1234_5678);
        bus.write_long(sp, state_ptr);
        bus.write_word(sp + 4, 0x0002); // virtual keycode 2
        bus.write_long(sp + 6, trans_data);
        bus.write_long(sp + 10, 0xDEAD_BEEF); // result slot sentinel
        bus.write_word(sp + 14, 0xCAFE); // trailing sentinel

        let result = disp.dispatch_toolbox(true, 0x1C3, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(
            bus.read_long(sp + 10),
            0x0000_0051,
            "KeyTrans should write the translated character at the result slot"
        );
        assert_eq!(
            bus.read_word(sp + 14),
            0xCAFE,
            "KeyTrans must pop exactly state/keycode/transData arguments (10 bytes)"
        );
        assert_eq!(
            bus.read_long(state_ptr),
            0,
            "KeyTrans should clear pending state on the nominal path"
        );
    }

    #[test]
    fn keytrans_single_character_result_uses_charcode2_low_byte() {
        // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 2-111:
        // when one character is returned, it is in Character code 2
        // (low byte) of the 32-bit result.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let state_ptr = 0x230100u32;
        let trans_data = 0x240100u32;
        seed_synthetic_kchr(&mut bus, trans_data);
        bus.write_long(state_ptr, 0);
        bus.write_long(sp, state_ptr);
        bus.write_word(sp + 4, 0x0102); // modifier byte 1, keycode 2
        bus.write_long(sp + 6, trans_data);
        bus.write_long(sp + 10, 0);

        let result = disp.dispatch_toolbox(true, 0x1C3, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let translated = bus.read_long(sp + 10);
        assert_eq!(
            translated & 0x0000_00FF,
            0x5A,
            "single-character output should occupy Character code 2 (low byte)"
        );
        assert_eq!(
            translated & 0x00FF_0000,
            0,
            "Character code 1 byte should be zero when only one character is returned"
        );
    }

    #[test]
    fn keytrans_non_deadkey_path_clears_state_for_followup_calls() {
        // Inside Macintosh: Text (1993), C-19..C-20: state carries dead-key
        // context; nominal non-dead-key translation should leave no pending
        // state for the next call.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let state_ptr = 0x230200u32;
        let trans_data = 0x240200u32;
        seed_synthetic_kchr(&mut bus, trans_data);
        bus.write_long(state_ptr, 0xFFFF_0001); // stale nonzero value
        bus.write_long(sp, state_ptr);
        bus.write_word(sp + 4, 0x0003); // virtual keycode 3
        bus.write_long(sp + 6, trans_data);
        bus.write_long(sp + 10, 0);

        let result = disp.dispatch_toolbox(true, 0x1C3, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(
            bus.read_long(state_ptr),
            0,
            "KeyTrans nominal HLE path should clear pending dead-key state"
        );
        assert_eq!(
            bus.read_long(sp + 10),
            0x0000_0057,
            "KeyTrans should still return the translated character"
        );
    }

    // GetAppParms ($A9F5)
    #[test]
    fn getappparms_returns_curapname_curaprefnum_and_appparmhandle() {
        // IM:II 1985, p. II-58: GetAppParms returns CurApName, CurApRefNum,
        // and AppParmHandle through its three VAR output parameters.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ap_param_out = 0x210000u32;
        let ap_refnum_out = 0x210100u32;
        let ap_name_out = 0x210200u32;

        bus.write_pstring(addr::CUR_APNAME, b"Marathon");
        bus.write_word(addr::CUR_APREF_NUM, (-6i16) as u16);
        bus.write_long(addr::APP_PARM_HANDLE, 0x00AB_CDEF);

        bus.write_long(ap_param_out, 0xDEAD_BEEF);
        bus.write_word(ap_refnum_out, 0xBEEF);
        bus.write_pstring(ap_name_out, b"XXXX");

        bus.write_long(sp, ap_param_out);
        bus.write_long(sp + 4, ap_refnum_out);
        bus.write_long(sp + 8, ap_name_out);

        let result = disp.dispatch_toolbox(true, 0x1F5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        assert_eq!(bus.read_pstring(ap_name_out), b"Marathon".to_vec());
        assert_eq!(bus.read_word(ap_refnum_out), (-6i16) as u16);
        assert_eq!(bus.read_long(ap_param_out), 0x00AB_CDEF);
    }

    #[test]
    fn getappparms_consumes_three_var_pointer_arguments() {
        // IM:II 1985, p. II-58 signature:
        // GetAppParms(VAR Str255, VAR INTEGER, VAR Handle) -> 3 pointers.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, 0);

        let result = disp.dispatch_toolbox(true, 0x1F5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    #[test]
    fn unloadseg_consumes_routineaddr_pointer_argument() {
        // IM:II 1985, p. II-58:
        // PROCEDURE UnloadSeg(routineAddr: Ptr);
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x00AB_CDEF);
        bus.write_word(sp + 4, 0xBEEF); // sentinel after pointer argument

        let result = disp.dispatch_toolbox(true, 0x1F1, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0xBEEF);
    }

    #[test]
    fn unloadseg_noop_preserves_registered_segment_cache() {
        // Systemless's HLE keeps segment_map resident; UnloadSeg currently
        // contracts to pointer-pop + no mutation of registered segments.
        let (mut disp, mut cpu, mut bus) = setup();
        let seg_map = HashMap::from([(1i16, 0x0022_0000u32), (9i16, 0x0033_0000u32)]);
        disp.register_segments(seg_map.clone());

        let sp = TEST_SP;
        bus.write_long(sp, 0x0022_1000);
        let result = disp.dispatch_toolbox(true, 0x1F1, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(disp.segment_map, seg_map);
    }

    // ExitToShell ($A9F4)
    #[test]
    fn exittoshell_terminates_application_and_returns_halted_error() {
        // IM:II 1985, p. II-58: ExitToShell terminates the current app
        // and returns to the shell.
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_toolbox(true, 0x1F4, &mut cpu, &mut bus);
        assert!(result.is_some());
        let err = result.unwrap().unwrap_err();
        assert!(
            matches!(err, crate::Error::Halted),
            "ExitToShell should return Error::Halted"
        );
    }

    #[test]
    fn exittoshell_procedure_signature_consumes_no_stack_arguments() {
        // IM:II 1985, p. II-58: PROCEDURE ExitToShell; (no arguments).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_toolbox(true, 0x1F4, &mut cpu, &mut bus);
        assert!(result.is_some());
        let err = result.unwrap().unwrap_err();
        assert!(matches!(err, crate::Error::Halted));
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    #[test]
    fn exittoshell_halted_path_preserves_d0_and_a7() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x1234_5678);

        let result = disp.dispatch_toolbox(true, 0x1F4, &mut cpu, &mut bus);
        assert!(result.is_some(), "ExitToShell should be handled");
        assert!(
            matches!(result.unwrap().unwrap_err(), crate::Error::Halted),
            "ExitToShell should return Error::Halted"
        );
        assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    #[test]
    fn launch_legacy_cmdline_queues_existing_target_and_preserves_page_option() {
        // IM:II II-59..II-60: the legacy CmdLine record stores a pointer to
        // the Pascal application name at 0(A0) and CurPageOption at 4(A0).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let cmd_line = bus.alloc(8);
        let app_name = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("LaunchTargets");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = target_dir_id);
        disp.vfs
            .insert("LaunchTargets/Legacy Helper".to_string(), Vec::new());

        cpu.write_reg(Register::A0, cmd_line);
        cpu.write_reg(Register::D0, 0x1234_5678);
        for offset in 0..8u32 {
            bus.write_byte(cmd_line + offset, 0);
        }
        bus.write_long(cmd_line, app_name);
        bus.write_word(cmd_line + 4, 0xFFFF);
        write_pascal_string(&mut bus, app_name, "Legacy Helper");

        let result = disp.dispatch_toolbox(true, 0x1F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "Launch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "an existing legacy target should be queued for runner switching"
        );
        assert_eq!(cpu.read_reg(Register::A0), cmd_line);
        assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(bus.read_word(0x0936), 0xFFFF);
        assert_eq!(
            disp.launched_app_path(),
            Some("LaunchTargets/Legacy Helper")
        );
        assert_eq!(
            disp.take_pending_launch_application(false, false)
                .as_deref(),
            Some("LaunchTargets/Legacy Helper")
        );
    }

    #[test]
    fn launch_legacy_cmdline_missing_target_halts_without_clobbering_d0() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let cmd_line = bus.alloc(8);
        let app_name = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("LaunchTargets");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = target_dir_id);

        cpu.write_reg(Register::A0, cmd_line);
        cpu.write_reg(Register::D0, 0x89AB_CDEF);
        for offset in 0..8u32 {
            bus.write_byte(cmd_line + offset, 0);
        }
        bus.write_long(cmd_line, app_name);
        bus.write_word(cmd_line + 4, 1);
        write_pascal_string(&mut bus, app_name, "NoSuchApp");

        let result = disp.dispatch_toolbox(true, 0x1F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "Launch should be handled");
        assert!(matches!(result.unwrap().unwrap_err(), crate::Error::Halted));
        assert_eq!(cpu.read_reg(Register::A0), cmd_line);
        assert_eq!(cpu.read_reg(Register::D0), 0x89AB_CDEF);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(bus.read_word(0x0936), 1);
        assert_eq!(disp.launched_app_path(), Some("LaunchTargets/NoSuchApp"));
        assert!(
            disp.take_pending_launch_application(false, true).is_none(),
            "a missing legacy target must not be queued"
        );
    }

    #[test]
    fn launchapplication_launchcontinue_clear_records_target_app_path_and_halts() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let launch_pb = bus.alloc(64);
        let app_spec = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("LaunchTargets");

        cpu.write_reg(Register::A0, launch_pb);
        cpu.write_reg(Register::D0, 0x1234_5678);

        for offset in 0..64u32 {
            bus.write_byte(launch_pb + offset, 0);
        }
        bus.write_word(launch_pb + 6, 0x4C43); // extendedBlock
        bus.write_long(launch_pb + 8, 32); // extendedBlockLen
        bus.write_word(launch_pb + 12, 0);
        bus.write_word(launch_pb + 14, 0);
        bus.write_long(launch_pb + 16, app_spec);

        for offset in 0..32u32 {
            bus.write_byte(app_spec + offset, 0);
        }
        bus.write_word(app_spec, 0);
        bus.write_long(app_spec + 2, target_dir_id);
        write_pascal_string(&mut bus, app_spec + 6, "NoSuchApp");

        let result = disp.dispatch_toolbox(true, 0x1F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "LaunchApplication should be handled");
        assert!(
            matches!(result.unwrap().unwrap_err(), crate::Error::Halted),
            "LaunchApplication should return Error::Halted"
        );
        assert_eq!(cpu.read_reg(Register::A0), launch_pb);
        assert_eq!(cpu.read_reg(Register::D0) as i32, -43);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(bus.read_long(launch_pb + 20), 0);
        assert_eq!(bus.read_long(launch_pb + 24), 0);
        assert_eq!(bus.read_long(launch_pb + 28), 0);
        assert_eq!(bus.read_long(launch_pb + 32), 0);
        assert_eq!(bus.read_long(launch_pb + 36), 0);
        assert_eq!(disp.launched_app_path(), Some("LaunchTargets/NoSuchApp"));
        assert_eq!(*disp.default_dir_id, target_dir_id);
        assert_ne!(disp.app_wd_refnum, 0);
    }

    #[test]
    fn launchapplication_launchcontinue_set_records_target_app_path_and_returns() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let launch_pb = bus.alloc(64);
        let app_spec = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("LaunchTargets");

        cpu.write_reg(Register::A0, launch_pb);
        cpu.write_reg(Register::D0, 0x1234_5678);

        for offset in 0..64u32 {
            bus.write_byte(launch_pb + offset, 0);
        }
        bus.write_word(launch_pb + 6, 0x4C43); // extendedBlock
        bus.write_long(launch_pb + 8, 32); // extendedBlockLen
        bus.write_word(launch_pb + 12, 0);
        bus.write_word(launch_pb + 14, 0x4000); // launchContinue
        bus.write_long(launch_pb + 16, app_spec);

        for offset in 0..32u32 {
            bus.write_byte(app_spec + offset, 0);
        }
        bus.write_word(app_spec, 0);
        bus.write_long(app_spec + 2, target_dir_id);
        write_pascal_string(&mut bus, app_spec + 6, "NoSuchApp");

        let result = disp.dispatch_toolbox(true, 0x1F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "LaunchApplication should be handled");
        assert!(
            result.unwrap().is_ok(),
            "LaunchApplication should return when launchContinue is set"
        );
        assert_eq!(cpu.read_reg(Register::A0), launch_pb);
        assert_eq!(cpu.read_reg(Register::D0) as i32, -43);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(bus.read_long(launch_pb + 20), 0);
        assert_eq!(bus.read_long(launch_pb + 24), 0);
        assert_eq!(bus.read_long(launch_pb + 28), 0);
        assert_eq!(bus.read_long(launch_pb + 32), 0);
        assert_eq!(bus.read_long(launch_pb + 36), 0);
        assert_eq!(disp.launched_app_path(), Some("LaunchTargets/NoSuchApp"));
        assert_eq!(*disp.default_dir_id, target_dir_id);
        assert_ne!(disp.app_wd_refnum, 0);
    }

    #[test]
    fn launchapplication_existing_foreground_target_queues_runner_launch() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let launch_pb = bus.alloc(64);
        let app_spec = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("LaunchTargets");
        disp.vfs
            .insert("LaunchTargets/Register Helper".to_string(), Vec::new());

        cpu.write_reg(Register::A0, launch_pb);
        cpu.write_reg(Register::D0, 0x1234_5678);

        for offset in 0..64u32 {
            bus.write_byte(launch_pb + offset, 0);
        }
        bus.write_word(launch_pb + 6, 0x4C43); // extendedBlock
        bus.write_long(launch_pb + 8, 32); // extendedBlockLen
        bus.write_word(launch_pb + 12, 0);
        bus.write_word(launch_pb + 14, 0x4000); // launchContinue
        bus.write_long(launch_pb + 16, app_spec);

        for offset in 0..32u32 {
            bus.write_byte(app_spec + offset, 0);
        }
        bus.write_word(app_spec, 0);
        bus.write_long(app_spec + 2, target_dir_id);
        write_pascal_string(&mut bus, app_spec + 6, "Register Helper");

        let result = disp.dispatch_toolbox(true, 0x1F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "LaunchApplication should be handled");
        assert!(
            result.unwrap().is_ok(),
            "LaunchApplication should return while queued foreground app waits for Event Manager yield"
        );
        assert_eq!(cpu.read_reg(Register::A0), launch_pb);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(
            disp.launched_app_path(),
            Some("LaunchTargets/Register Helper")
        );
        assert!(
            disp.take_pending_launch_application(false, false).is_none(),
            "launchContinue target should not start until an Event Manager yield"
        );
        assert_eq!(
            disp.take_pending_launch_application(true, false).as_deref(),
            Some("LaunchTargets/Register Helper")
        );
    }

    #[test]
    fn launchapplication_existing_target_without_launchcontinue_queues_immediate_runner_launch() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let launch_pb = bus.alloc(64);
        let app_spec = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("LaunchTargets");
        disp.vfs
            .insert("LaunchTargets/Register Helper".to_string(), Vec::new());

        cpu.write_reg(Register::A0, launch_pb);
        cpu.write_reg(Register::D0, 0x1234_5678);

        for offset in 0..64u32 {
            bus.write_byte(launch_pb + offset, 0);
        }
        bus.write_word(launch_pb + 6, 0x4C43); // extendedBlock
        bus.write_long(launch_pb + 8, 32); // extendedBlockLen
        bus.write_word(launch_pb + 12, 0);
        bus.write_word(launch_pb + 14, 0);
        bus.write_long(launch_pb + 16, app_spec);

        for offset in 0..32u32 {
            bus.write_byte(app_spec + offset, 0);
        }
        bus.write_word(app_spec, 0);
        bus.write_long(app_spec + 2, target_dir_id);
        write_pascal_string(&mut bus, app_spec + 6, "Register Helper");

        let result = disp.dispatch_toolbox(true, 0x1F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "LaunchApplication should be handled");
        assert!(
            result.unwrap().is_ok(),
            "existing foreground launch target should return for runner-level app switching"
        );
        assert_eq!(cpu.read_reg(Register::A0), launch_pb);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(
            disp.launched_app_path(),
            Some("LaunchTargets/Register Helper")
        );
        assert_eq!(
            disp.take_pending_launch_application(false, false)
                .as_deref(),
            Some("LaunchTargets/Register Helper"),
            "non-launchContinue foreground target should be immediately serviceable"
        );
    }

    #[test]
    fn launchapplication_launchdontswitch_defers_existing_target_until_caller_exit() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let launch_pb = bus.alloc(64);
        let app_spec = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("LaunchTargets");
        disp.vfs
            .insert("LaunchTargets/Register Helper".to_string(), Vec::new());

        cpu.write_reg(Register::A0, launch_pb);
        cpu.write_reg(Register::D0, 0x1234_5678);

        for offset in 0..64u32 {
            bus.write_byte(launch_pb + offset, 0);
        }
        bus.write_word(launch_pb + 6, 0x4C43); // extendedBlock
        bus.write_long(launch_pb + 8, 32); // extendedBlockLen
        bus.write_word(launch_pb + 12, 0);
        bus.write_word(launch_pb + 14, 0x4200); // launchContinue | launchDontSwitch
        bus.write_long(launch_pb + 16, app_spec);

        for offset in 0..32u32 {
            bus.write_byte(app_spec + offset, 0);
        }
        bus.write_word(app_spec, 0);
        bus.write_long(app_spec + 2, target_dir_id);
        write_pascal_string(&mut bus, app_spec + 6, "Register Helper");

        let result = disp.dispatch_toolbox(true, 0x1F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "LaunchApplication should be handled");
        assert!(
            result.unwrap().is_ok(),
            "launchDontSwitch with launchContinue should return without foreground switching"
        );
        assert_eq!(cpu.read_reg(Register::A0), launch_pb);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(
            disp.launched_app_path(),
            Some("LaunchTargets/Register Helper")
        );
        assert!(
            disp.take_pending_launch_application(true, false).is_none(),
            "launchDontSwitch target must remain in the background while its caller runs"
        );
        assert_eq!(
            disp.take_pending_launch_application(false, true).as_deref(),
            Some("LaunchTargets/Register Helper"),
            "launchDontSwitch target should become runnable when its caller exits"
        );
    }

    #[test]
    fn chain_records_cmdline_path_and_curpageoption_before_halt() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        let cmd_line = bus.alloc(8);
        let app_name = bus.alloc(32);
        let target_dir_id = disp.ensure_vfs_directory("ChainTargets");

        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = target_dir_id);
        cpu.write_reg(Register::A0, cmd_line);
        cpu.write_reg(Register::D0, 0x1234_5678);

        for offset in 0..8u32 {
            bus.write_byte(cmd_line + offset, 0);
        }
        bus.write_long(cmd_line, app_name);
        bus.write_word(cmd_line + 4, 0x0001);

        write_pascal_string(&mut bus, app_name, "NoSuchApp");

        let result = disp.dispatch_toolbox(true, 0x1F3, &mut cpu, &mut bus);
        assert!(result.is_some(), "Chain should be handled");
        assert!(
            matches!(result.unwrap().unwrap_err(), crate::Error::Halted),
            "Chain should return Error::Halted"
        );
        assert_eq!(cpu.read_reg(Register::A0), cmd_line);
        assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
        assert_eq!(bus.read_word(0x0936), 0x0001);
        assert_eq!(disp.launched_app_path(), Some("ChainTargets/NoSuchApp"));
        assert_eq!(*disp.default_dir_id, target_dir_id);
        assert_ne!(disp.app_wd_refnum, 0);
    }

    // _SCSIDispatch ($A815)
    #[test]
    fn scsi_dispatch_generated_routes_preserve_exact_stack_word_values() {
        assert_eq!(super::SCSI_DISPATCH_OPERATION_ROUTES.len(), 13);
        assert!(super::SCSI_DISPATCH_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0000, "SCSIReset"),
            (0x0001, "SCSIGet"),
            (0x0002, "SCSISelect"),
            (0x0003, "SCSICmd"),
            (0x0004, "SCSIComplete"),
            (0x0005, "SCSIRead"),
            (0x0006, "SCSIWrite"),
            (0x0008, "SCSIRBlind"),
            (0x0009, "SCSIWBlind"),
            (0x000A, "SCSIStat"),
            (0x000B, "SCSISelAtn"),
            (0x000C, "SCSIMsgIn"),
            (0x000D, "SCSIMsgOut"),
        ] {
            let route = super::scsi_dispatch_operation_route(0xA815, selector)
                .expect("SCSIDispatch operation route");
            assert_eq!(route.routine_name, routine_name);
            let carrier = if selector == 0 {
                "stack-word-zero"
            } else {
                "stack-word-immediate"
            };
            assert_eq!(
                route.operation_id,
                format!("selector-operation:_SCSIDispatch:0x{selector:04X}:{carrier}:16")
            );
        }

        for (trap_word, selector) in [
            (0xA915, 0x0000),
            (0xA814, 0x000D),
            (0xA815, 0x0007), // no source/interface operation identity
            (0xA815, 0x000E), // adjacent unassigned selector
            (0xA815, 0x4267), // CLR.W -(SP) opcode spelling
            (0xA815, 0x3F3C), // MOVE.W immediate-to-stack opcode spelling
            (0xA815, 0x0100), // byte-swapped SCSIGet selector
        ] {
            assert!(super::scsi_dispatch_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn scsi_dispatch_records_stack_word_identity_without_changing_complete_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        for trap_word in [0xA815, 0xA915] {
            disp.current_trap_word = trap_word;
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D0, 0x1234_5678);
            bus.write_word(sp, 0x0004); // SCSIComplete
            bus.write_long(sp + 2, 0x1111_2222); // wait
            bus.write_long(sp + 6, 0x3333_4444); // message pointer
            bus.write_long(sp + 10, 0x5555_6666); // stat pointer
            bus.write_word(sp + 14, 0xBEEF); // OSErr result

            let result = disp.dispatch_toolbox(true, 0x015, &mut cpu, &mut bus);
            assert!(result.expect("SCSIDispatch arm").is_ok());
            assert_eq!(cpu.read_reg(Register::A7), sp + 14);
            assert_eq!(bus.read_word(sp + 14), 0);
            assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
            assert_eq!(bus.read_long(sp + 2), 0x1111_2222);
            assert_eq!(bus.read_long(sp + 6), 0x3333_4444);
            assert_eq!(bus.read_long(sp + 10), 0x5555_6666);

            let expected = (trap_word == 0xA815)
                .then_some("selector-operation:_SCSIDispatch:0x0004:stack-word-immediate:16");
            assert_eq!(disp.current_selector_operation, expected);
        }
    }

    #[test]
    fn scsi_dispatch_undefined_selectors_raise_ds_core_err() {
        // Inside Macintosh Volume V (1986), p. V-574, requires every
        // undefined SCSIDispatch selector to invoke the System Error Handler
        // with dsCoreErr (12).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        for selector in [0x0007, 0x000E, 0xFFFF] {
            disp.current_trap_word = 0xA815;
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D0, 0x1234_5678);
            bus.write_word(sp, selector);
            bus.write_long(sp + 2, 0xDEAD_BEEF);
            bus.write_word(addr::DS_ERR_CODE, 0xBEEF);

            let result = disp
                .dispatch_toolbox(true, 0x015, &mut cpu, &mut bus)
                .expect("SCSIDispatch arm");
            assert!(matches!(result, Err(crate::Error::Halted)));
            assert_eq!(bus.read_word(addr::DS_ERR_CODE), 12);
            assert_eq!(cpu.read_reg(Register::A7), sp);
            assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
            assert_eq!(bus.read_long(sp + 2), 0xDEAD_BEEF);
            assert_eq!(disp.current_selector_operation, None);
        }
    }

    #[test]
    fn scsidispatch_selector_zero_returns_noerr_and_pops_selector_word() {
        // Inside Macintosh Volume IV (1986), pp. IV-287 to IV-300:
        // selector 0 (SCSIReset) is a word-selector dispatch entry.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x1234_5678);
        bus.write_word(sp_before + 2, 0xFFFF);
        bus.write_word(sp_before, 0);

        let result = disp.dispatch_toolbox(true, 0x015, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 2);
        assert_eq!(bus.read_word(sp_before + 2), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
    }

    #[test]
    fn scsidispatch_selector_two_returns_noerr_and_pops_two_byte_argument_frame() {
        // Inside Macintosh Volume IV (1986), pp. IV-287 to IV-300:
        // selector 2 (SCSISelect) consumes its selector word plus one
        // 2-byte argument before the OSErr result slot.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x89AB_CDEF);
        bus.write_word(sp_before + 4, 0xFFFF);
        bus.write_word(sp_before, 2);
        bus.write_word(sp_before + 2, 0x1357);

        let result = disp.dispatch_toolbox(true, 0x015, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 4);
        assert_eq!(bus.read_word(sp_before + 4), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0x89AB_CDEF);
    }

    #[test]
    fn scsidispatch_selector_three_returns_noerr_and_pops_six_byte_argument_frame() {
        // Inside Macintosh Volume IV (1986), pp. IV-287 to IV-300:
        // selector 3 (SCSICmd) consumes a 4-byte pointer and a 2-byte count.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, 0x0BAD_F00D);
        bus.write_word(sp_before + 8, 0xFFFF);
        bus.write_word(sp_before, 3);
        bus.write_word(sp_before + 2, 0x1111);
        bus.write_word(sp_before + 4, 0x2222);
        bus.write_word(sp_before + 6, 0x3333);

        let result = disp.dispatch_toolbox(true, 0x015, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 8);
        assert_eq!(bus.read_word(sp_before + 8), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0x0BAD_F00D);
    }

    // Debugger ($A9FF)
    #[test]
    fn debugger_trap_is_parameterless_and_preserves_stack_pointer() {
        // Universal Interfaces Types.h declares Debugger() as a
        // parameterless one-word inline trap.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_toolbox(true, 0x1FF, &mut cpu, &mut bus);
        assert!(result.is_some(), "Debugger should be handled");
        assert!(result.unwrap().is_ok(), "Debugger should return");
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    #[test]
    fn debugger_without_installed_debugger_returns_to_caller() {
        // On a stock System 7 setup without MacsBug installed,
        // Debugger returns immediately to the caller.
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x1234_5678);
        cpu.write_reg(Register::A0, 0x00AA_5500);

        let result = disp.dispatch_toolbox(true, 0x1FF, &mut cpu, &mut bus);
        assert!(result.is_some(), "Debugger should be handled");
        assert!(result.unwrap().is_ok(), "Debugger should return to caller");
        assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
        assert_eq!(cpu.read_reg(Register::A0), 0x00AA_5500);
    }

    // ShutDwnPower ($A895)
    #[test]
    fn test_shut_dwn_power() {
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_toolbox(true, 0x095, &mut cpu, &mut bus);
        assert!(result.is_some());
        let err = result.unwrap().unwrap_err();
        assert!(
            matches!(err, crate::Error::Halted),
            "ShutDwnPower should return Error::Halted"
        );
    }

    // ShutDwnInstall ($A895 selector 3)
    // Inside Macintosh Volume V, V-589.
    // Universal Headers <ShutDown.h> declares the call as
    //   THREEWORDINLINE(0x3F3C, 0x0003, 0xA895)
    // — the compiler emits MOVE.W #3,-(A7) immediately before $A895, so
    // SP at trap entry holds selector(2) + flags(2) + proc(4) = 8 bytes.
    #[test]
    fn shutdwninstall_pops_eight_byte_argument_frame_and_returns() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_at_trap = TEST_SP;
        bus.write_word(sp_at_trap, 3); // selector
        bus.write_word(sp_at_trap + 2, 0x0001); // flags = sdOnPowerOff
        bus.write_long(sp_at_trap + 4, 0xCAFE_BABE); // proc (never invoked)
        cpu.write_reg(Register::A7, sp_at_trap);

        let result = disp.dispatch_toolbox(true, 0x095, &mut cpu, &mut bus);
        let inner = result.expect("_Shutdown must be a handled trap");
        assert!(
            inner.is_ok(),
            "ShutDwnInstall (selector 3) must return Ok, got {:?}",
            inner
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "sdInstall must report noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_at_trap + 8,
            "sdInstall must pop selector(2) + flags(2) + proc(4) = 8 bytes"
        );
    }

    // ShutDwnRemove ($A895 selector 4)
    // Inside Macintosh Volume V, V-590.
    // Universal Headers <ShutDown.h> declares the call as
    //   THREEWORDINLINE(0x3F3C, 0x0004, 0xA895)
    // — compiler emits MOVE.W #4,-(A7) immediately before $A895, so SP at
    // trap entry holds selector(2) + proc(4) = 6 bytes.
    #[test]
    fn shutdwnremove_pops_six_byte_argument_frame_and_returns() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_at_trap = TEST_SP;
        bus.write_word(sp_at_trap, 4); // selector
        bus.write_long(sp_at_trap + 2, 0xCAFE_BABE); // proc (never invoked)
        cpu.write_reg(Register::A7, sp_at_trap);

        let result = disp.dispatch_toolbox(true, 0x095, &mut cpu, &mut bus);
        let inner = result.expect("_Shutdown must be a handled trap");
        assert!(
            inner.is_ok(),
            "ShutDwnRemove (selector 4) must return Ok, got {:?}",
            inner
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "sdRemove must report noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_at_trap + 6,
            "sdRemove must pop selector(2) + proc(4) = 6 bytes"
        );
    }

    // SetFractEnable ($A814)
    //
    // Per Inside Macintosh Volume IV (1986), p. IV-32, SetFractEnable
    // writes the BOOLEAN argument byte verbatim into the FractEnable
    // low-memory global at $0BF4. MPW Pascal BOOLEAN convention places
    // the value byte in the HIGH byte of the 2-byte stack slot, so the
    // trap reads byte at SP+0 (not SP+1) and writes that byte to $0BF4
    // unchanged — TRUE → 0x01, FALSE → 0x00 (NOT a normalised 0xFF for
    // TRUE). These tests pre-poison $0BF4 with a distinct sentinel and
    // assert (a) the written byte exactly matches the input high-byte, (b) the
    // trap pops exactly 2 bytes (Pascal PROCEDURE protocol — no
    // function-result slot), and (c) memory adjacent to $0BF4 is
    // preserved (regression guard against any future "fix" that writes
    // a wider word or normalises the byte).
    #[test]
    fn setfractenable_true_writes_one_byte_verbatim_to_fract_enable_global() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // Pre-poison $0BF4 with a sentinel and the adjacent bytes with
        // distinct values to catch any over-write.
        bus.write_byte(0x0BF4, 0xA5);
        bus.write_byte(0x0BF3, 0x77);
        bus.write_byte(0x0BF5, 0x88);
        // Push BOOLEAN TRUE: value byte 0x01 in the HIGH byte of the
        // 2-byte stack slot.
        bus.write_byte(sp, 0x01);
        bus.write_byte(sp + 1, 0x00);

        let result = disp.dispatch_toolbox(true, 0x014, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetFractEnable must be a handled trap");
        assert!(result.unwrap().is_ok());

        // FractEnable byte at $0BF4 holds 0x01 — verbatim BOOLEAN high byte.
        assert_eq!(bus.read_byte(0x0BF4), 0x01);
        // Adjacent bytes preserved — no over-write past the single byte.
        assert_eq!(bus.read_byte(0x0BF3), 0x77);
        assert_eq!(bus.read_byte(0x0BF5), 0x88);
        // A7 advanced by exactly 2 — Pascal PROCEDURE pops the BOOLEAN arg.
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    #[test]
    fn setfractenable_false_writes_zero_byte_to_fract_enable_global() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // Pre-poison with a non-zero value so we can detect that the
        // trap actually wrote 0x00 (rather than no-op).
        bus.write_byte(0x0BF4, 0xC3);
        bus.write_byte(sp, 0x00);
        bus.write_byte(sp + 1, 0x00);

        let result = disp.dispatch_toolbox(true, 0x014, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        // FractEnable byte at $0BF4 cleared to 0x00.
        assert_eq!(bus.read_byte(0x0BF4), 0x00);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    #[test]
    fn setfractenable_consumes_two_byte_boolean_argument_and_balances_stack() {
        // Pascal PROCEDURE protocol: caller pushes one 2-byte BOOLEAN,
        // trap pops 2 bytes, no function-result slot. Net externally
        // observed A7 movement is +2 (the trap's pop). This test asserts
        // the exact pop count regardless of the BOOLEAN value to defeat
        // any future change that pops a different number of bytes
        // (e.g. 4 for a hypothetical LONGINT widening).
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_byte(sp, 0x01);
        bus.write_byte(sp + 1, 0x00);

        let result = disp.dispatch_toolbox(true, 0x014, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 2,
            "SetFractEnable must pop exactly the 2-byte BOOLEAN argument"
        );
    }

    // Delay ($A03B) — OS trap
    #[test]
    fn test_delay() {
        let (mut disp, mut cpu, mut bus) = setup();

        // A0 = numTicks (0 from setup), Ticks at $016A = 100 (from setup)
        // finalTicks = 100 + 0 = 100
        let result = disp.dispatch_toolbox(false, 0x3B, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::D0), 100); // finalTicks returned in D0
    }

    // ========== Sound Manager (extended) ==========

    // SoundDispatch ($A800)
    // Selector encoding: bits 31-24 = param_bytes/2, bits 23-16 = routine
    // Sound 1994, 2-256
    #[test]
    fn test_sound_dispatch() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // Selector goes in D0, not on the stack.
        // 0x00080008: param_bytes=0, routine=$08 (unknown → stub)
        cpu.write_reg(Register::D0, 0x00080008);

        let result = disp.dispatch_sound(true, 0x000, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // No params to pop, SP unchanged
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // SndNewChannel ($A807)
    #[test]
    fn test_snd_new_channel() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x300000u32;
        // Pascal stack (right-to-left push):
        //   SP+0:  userRoutine (4, ProcPtr)
        //   SP+4:  init (4, LongInt)
        //   SP+8:  synth (2, Integer)
        //   SP+10: chan ptr (4, VAR SndChannelPtr)
        //   SP+14: result (2, OSErr)
        // Sound 1994, 2-195
        bus.write_long(sp, 0); // userRoutine
        bus.write_long(sp + 4, 0); // init
        bus.write_word(sp + 8, 0); // synth
        bus.write_long(sp + 10, chan_ptr_ptr); // chan_ptr_ptr
        bus.write_word(sp + 14, 0xBEEF); // result placeholder

        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        // Verify a channel was allocated (chan_ptr_ptr should point to non-zero)
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert_ne!(chan_ptr, 0, "SndNewChannel should allocate a channel");
    }

    #[test]
    fn test_snd_new_channel_writes_callback_procptr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x300000u32;
        let user_routine = 0x00AB_CDEFu32;
        // Per IM:Sound 1994 p.2-195, SndNewChannel installs the caller's
        // callback procedure for callBackCmd processing.
        bus.write_long(sp, user_routine); // userRoutine
        bus.write_long(sp + 4, 0); // init
        bus.write_word(sp + 8, 0); // synth
        bus.write_long(sp + 10, chan_ptr_ptr); // chan VAR pointer
        bus.write_word(sp + 14, 0xBEEF); // result placeholder

        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert_ne!(chan_ptr, 0);
        // Sound channel callback field is at offset +8 in SndChannel.
        assert_eq!(bus.read_long(chan_ptr + 8), user_routine);
        assert_eq!(disp.sound_manager.channels.len(), 1);
        assert_eq!(disp.sound_manager.channels[0].callback_addr, user_routine);
    }

    // SndDisposeChannel ($A801)
    #[test]
    fn test_snd_dispose_channel() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // SP+0: quiet(2), SP+2: chan(4), SP+6: result(2)
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, 0);
        bus.write_word(sp + 6, 0xBEEF);

        let result = disp.dispatch_sound(true, 0x001, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    #[test]
    fn test_snd_dispose_channel_removes_allocated_channel() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x300000u32;
        // Allocate channel first (IM:Sound 1994 p.2-195).
        bus.write_long(sp, 0); // userRoutine
        bus.write_long(sp + 4, 0); // init
        bus.write_word(sp + 8, 0); // synth
        bus.write_long(sp + 10, chan_ptr_ptr); // chan VAR pointer
        bus.write_word(sp + 14, 0xBEEF); // result placeholder
        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert_ne!(chan_ptr, 0);
        assert_eq!(disp.sound_manager.channels.len(), 1);

        // Dispose the channel (IM:Sound 1994 p.2-196).
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // quietNow = TRUE
        bus.write_long(sp + 2, chan_ptr);
        bus.write_word(sp + 6, 0xBEEF);
        let result = disp.dispatch_sound(true, 0x001, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert!(disp.sound_manager.channels.is_empty());
    }

    // SndDoCommand ($A803)
    #[test]
    fn test_snd_do_command() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // SP+0: noWait(2), SP+2: cmd(4, ptr to SndCommand), SP+6: chan(4), SP+10: result(2)
        // Sound 1994, 2-130
        let cmd_addr = 0x200100u32;
        bus.write_word(cmd_addr, 0); // cmd = nullCmd
        bus.write_word(cmd_addr + 2, 0); // param1
        bus.write_long(cmd_addr + 4, 0); // param2
        bus.write_word(sp, 0); // noWait
        bus.write_long(sp + 2, cmd_addr); // cmd ptr
        bus.write_long(sp + 6, 0); // chan
        bus.write_word(sp + 10, 0xBEEF); // result placeholder

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), (-205i16) as u16);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn test_snd_do_command_callback_cmd_queues_pending_callback() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x300000u32;
        let user_routine = 0x00AB_CDEFu32;
        let cmd_addr = 0x200100u32;

        // Install channel callback via SndNewChannel (IM:Sound 1994 p.2-195).
        bus.write_long(sp, user_routine); // userRoutine
        bus.write_long(sp + 4, 0); // init
        bus.write_word(sp + 8, 0); // synth
        bus.write_long(sp + 10, chan_ptr_ptr); // chan VAR pointer
        bus.write_word(sp + 14, 0xBEEF);
        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert_ne!(chan_ptr, 0);

        // callBackCmd should schedule the channel callback procedure.
        // IM:Sound notes to issue callBackCmd with SndDoCommand (not
        // SndDoImmediate) so queue ordering is preserved.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(cmd_addr, crate::sound::cmd::CALLBACK);
        bus.write_word(cmd_addr + 2, 7);
        bus.write_long(cmd_addr + 4, 0x1111_2222);
        bus.write_word(sp, 0); // noWait
        bus.write_long(sp + 2, cmd_addr);
        bus.write_long(sp + 6, chan_ptr);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_sound(true, 0x003, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(disp.sound_manager.pending_sound_callbacks.len(), 1);
        match &disp.sound_manager.pending_sound_callbacks[0] {
            crate::sound::PendingSoundCallback::Command {
                architecture,
                callback_addr,
                chan_ptr: queued_chan_ptr,
                cmd,
            } => {
                assert_eq!(
                    *architecture,
                    crate::callback_manager::CallbackTaskArchitecture::M68k
                );
                assert_eq!(*callback_addr, user_routine);
                assert_eq!(*queued_chan_ptr, chan_ptr);
                assert_eq!(cmd.cmd, crate::sound::cmd::CALLBACK);
                assert_eq!(cmd.param1, 7);
                assert_eq!(cmd.param2, 0x1111_2222);
            }
            other => panic!("expected command callback, got {other:?}"),
        }
    }

    // SndDoImmediate ($A804)
    #[test]
    fn test_snd_do_immediate() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // SP+0: cmd(4, ptr to SndCommand), SP+4: chan(4), SP+8: result(2)
        // Sound 1994, 2-131
        let cmd_addr = 0x200100u32;
        bus.write_word(cmd_addr, 0); // cmd = nullCmd
        bus.write_word(cmd_addr + 2, 0); // param1
        bus.write_long(cmd_addr + 4, 0); // param2
        bus.write_long(sp, cmd_addr); // cmd ptr
        bus.write_long(sp + 4, 0); // chan
        bus.write_word(sp + 8, 0xBEEF); // result placeholder

        let result = disp.dispatch_sound(true, 0x004, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 8), (-205i16) as u16);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn test_snd_do_immediate_get_rate_writes_unity_fixed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let chan_ptr_ptr = 0x300000u32;
        let cmd_addr = 0x200100u32;
        let rate_out_addr = 0x200200u32;

        // Allocate a channel first so getRateCmd queries a real channel.
        bus.write_long(sp, 0); // userRoutine
        bus.write_long(sp + 4, 0); // init
        bus.write_word(sp + 8, 0); // synth
        bus.write_long(sp + 10, chan_ptr_ptr); // chan VAR pointer
        bus.write_word(sp + 14, 0xBEEF);
        let result = disp.dispatch_sound(true, 0x007, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let chan_ptr = bus.read_long(chan_ptr_ptr);
        assert_ne!(chan_ptr, 0);

        // IM:Sound documents getRateCmd writes the channel rate as a Fixed
        // through param2 when SndDoImmediate returns noErr.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(cmd_addr, crate::sound::cmd::GET_RATE);
        bus.write_word(cmd_addr + 2, 0);
        bus.write_long(cmd_addr + 4, rate_out_addr);
        bus.write_long(rate_out_addr, 0xDEAD_BEEFu32);
        bus.write_long(sp, cmd_addr);
        bus.write_long(sp + 4, chan_ptr);
        bus.write_word(sp + 8, 0xBEEF);

        let result = disp.dispatch_sound(true, 0x004, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 8), 0); // noErr
        assert_eq!(bus.read_long(rate_out_addr), 0x0001_0000); // unity rate
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // ========== Resource Manager (extended) ==========

    // OpenRFPerm ($A9C4) — file not found
    #[test]
    fn test_open_rf_perm_not_found() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // SP+0: perm(2), SP+2: vref(2), SP+4: name_ptr(4), SP+8: result(2)
        let name_ptr = 0x200000u32;
        bus.write_word(sp, 1); // perm = fsRdPerm
        bus.write_word(sp + 2, 0); // vRefNum
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0x0000); // result placeholder

        // Write Pascal string filename at name_ptr
        bus.write_pstring(name_ptr, b"NoSuchFile");

        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // result = -1 as u16 = 0xFFFF
        assert_eq!(bus.read_word(sp + 8), (-1i16) as u16);
        // D0 mirrors the FUNCTION result slot (-1 on failure).
        assert_eq!(cpu.read_reg(Register::D0), (-1i32) as u32);
        // ResErr at $0A60 = -43
        assert_eq!(bus.read_word(0x0A60), (-43i16) as u16);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn open_rf_perm_rejects_write_access_to_mounted_image_resources() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200000u32;
        let volume_ref =
            disp.mount_vfs_volume("Resource Disk", 0, 1, 1024, 512, 512, 900, 0, 0, 0, 0, 0, 0);
        disp.vfs_rsrc
            .insert("Resource Disk/Assets".to_string(), vec![]);
        disp.vfs_rsrc.insert("Assets".to_string(), vec![0x42]);
        bus.write_byte(sp, 3); // fsRdWrPerm
        bus.write_word(sp + 2, volume_ref as u16);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Assets");

        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.expect("OpenRFPerm arm").is_ok());
        assert_eq!(bus.read_word(sp + 8) as i16, -1);
        assert_eq!(cpu.read_reg(Register::D0) as i32, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, -44, "wPrErr");

        cpu.write_reg(Register::A7, sp);
        bus.write_byte(sp, 1); // fsRdPerm
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.expect("OpenRFPerm arm").is_ok());
        let ref_num = bus.read_word(sp + 8);
        assert_ne!(ref_num as i16, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, 0);
        assert_eq!(
            disp.resources
                .as_ref()
                .and_then(|resources| resources.names.get(&ref_num))
                .map(String::as_str),
            Some("Resource Disk/Assets")
        );
    }

    // IM:IV IV-17 and MTb 1993 1-64..1-66: successful OpenRFPerm returns a
    // file refnum and makes the newly opened file current.
    #[test]
    fn open_rf_perm_present_file_returns_refnum_and_sets_current_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200100u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);

        bus.write_word(sp, 1); // fsRdPerm
        bus.write_word(sp + 2, 0); // vRefNum
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF); // result placeholder
        bus.write_pstring(name_ptr, b"Shapes");

        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let refnum = bus.read_word(sp + 8);
        assert_ne!(refnum as i16, -1);
        assert_eq!(cpu.read_reg(Register::D0), refnum as u32);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(disp.current_resource_refnum(), refnum);
        assert_eq!(disp.resource_file_name(refnum), Some("Shapes"));
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // MTb 1993 p. 1-65: if already open, OpenRFPerm returns the same refnum
    // and does not make that file current.
    #[test]
    fn open_rf_perm_reopen_returns_existing_refnum_without_switching_current_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200200u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);
        disp.vfs_rsrc.insert("Sounds".to_string(), vec![]);

        // First open: Shapes.
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let shapes_ref = bus.read_word(sp + 8);
        assert_ne!(shapes_ref as i16, -1);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);

        // Second open: Sounds.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let sounds_ref = bus.read_word(sp + 8);
        assert_ne!(sounds_ref as i16, -1);
        assert_ne!(sounds_ref, shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);
        assert_eq!(cpu.read_reg(Register::D0), sounds_ref as u32);

        // Re-open Shapes: refnum re-used, current file unchanged.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), shapes_ref);
        assert_eq!(cpu.read_reg(Register::D0), shapes_ref as u32);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    #[test]
    fn open_rf_perm_read_after_write_reuses_existing_read_only_refnum() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200240u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);

        bus.write_word(sp, 0);
        bus.write_byte(sp, 3); // fsRdWrPerm
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let write_ref = bus.read_word(sp + 8);
        assert!(disp.write_refnums.contains(&write_ref));

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // fsRdPerm
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let read_ref = bus.read_word(sp + 8);
        assert_ne!(read_ref, write_ref);
        assert!(!disp.write_refnums.contains(&read_ref));

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1); // fsRdPerm
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 8), read_ref);
        assert_eq!(cpu.read_reg(Register::D0), read_ref as u32);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(
            disp.resources
                .as_ref()
                .unwrap()
                .names
                .values()
                .filter(|name| name.as_str() == "Shapes")
                .count(),
            2,
            "one write and one read-only access path should be enough"
        );
    }

    // OpenRFPerm should mirror the current refnum in D0 for both the first
    // open and the already-open reuse path.
    #[test]
    fn open_rf_perm_mirrors_refnum_in_d0_on_success_and_reopen() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200480u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);
        disp.vfs_rsrc.insert("Sounds".to_string(), vec![]);

        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let shapes_ref = bus.read_word(sp + 8);
        assert_ne!(shapes_ref as i16, -1);
        assert_eq!(cpu.read_reg(Register::D0), shapes_ref as u32);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let sounds_ref = bus.read_word(sp + 8);
        assert_ne!(sounds_ref as i16, -1);
        assert_eq!(cpu.read_reg(Register::D0), sounds_ref as u32);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), shapes_ref);
        assert_eq!(cpu.read_reg(Register::D0), shapes_ref as u32);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // MTb 1993 p. 1-65: if already open, OpenRFPerm returns the same refnum
    // and does not make that file current, even when a different file is
    // current at the time of the reopen.
    #[test]
    fn open_rf_perm_reopen_keeps_current_file_when_a_different_file_is_current() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200280u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);
        disp.vfs_rsrc.insert("Sounds".to_string(), vec![]);

        // Open Shapes.
        bus.write_word(sp, 1); // fsRdPerm
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let shapes_ref = bus.read_word(sp + 8);
        assert_ne!(shapes_ref as i16, -1);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);

        // Open Sounds so we can later make Shapes current again.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let sounds_ref = bus.read_word(sp + 8);
        assert_ne!(sounds_ref as i16, -1);
        assert_ne!(sounds_ref, shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);

        // Make Shapes current again, then reopen Sounds. The reopen must
        // return the original refnum and leave Shapes current.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, shapes_ref);
        let result = disp.dispatch_toolbox(true, 0x198, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.current_resource_refnum(), shapes_ref);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, name_ptr);
        bus.write_word(sp + 8, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x1C4, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), sounds_ref);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // Inside Macintosh Volume I (1985), p. I-115: OpenResFile opens the
    // named resource file, returns a refnum, and makes it current.
    #[test]
    fn open_res_file_present_returns_refnum_and_sets_current_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200250u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");

        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let refnum = bus.read_word(sp + 4);
        assert_ne!(refnum as i16, -1);
        assert_eq!(cpu.read_reg(Register::D0), refnum as u32);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(disp.current_resource_refnum(), refnum);
        assert_eq!(disp.resource_file_name(refnum), Some("Shapes"));
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn open_res_file_publishes_a_valid_fcb_and_preserves_other_access_paths() {
        use crate::memory::globals::addr;
        let (mut disp, mut cpu, mut bus) = setup();
        let original_buffer = bus.alloc(96);
        bus.fill_bytes(original_buffer, 96, 0);
        bus.write_word(original_buffer, 96);
        bus.write_long(original_buffer + 2, 1234);
        bus.write_long(addr::FCB_S_PTR, original_buffer);
        bus.write_word(addr::FS_FCB_LEN, 94);
        bus.write_word(addr::CUR_APREF_NUM, 2);
        disp.open_files.insert(96, "Other Data".to_string());
        disp.vfs_rsrc.insert("Profiles/Player".to_string(), vec![]);
        disp.set_vfs_entry_metadata("Profiles/Player", *b"SAVE", *b"TEST", 0);
        let metadata = disp.vfs_file_metadata("Profiles/Player").unwrap();
        let name_ptr = 0x200250;
        bus.write_pstring(name_ptr, b"Profiles:Player");
        bus.write_long(TEST_SP, name_ptr);

        disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let refnum = bus.read_word(TEST_SP + 4);
        let buffer = bus.read_long(addr::FCB_S_PTR);
        // Files 1992, 2-81–2-83: these are the same direct FCB/VCB
        // lookups used by classic runtime libraries to recover a volume.
        assert_eq!(refnum, 190, "skip application and data-fork access paths");
        assert_eq!(refnum % bus.read_word(addr::FS_FCB_LEN), 2);
        assert!(refnum + 94 <= bus.read_word(buffer));
        assert_eq!(bus.read_long(buffer + 2), 1234);
        let fcb = buffer + refnum as u32;
        assert_eq!(bus.read_long(fcb), metadata.file_id);
        assert_eq!(bus.read_word(fcb + 4), 0x0200);
        assert_eq!(bus.read_long(fcb + 50), u32::from_be_bytes(*b"SAVE"));
        assert_eq!(bus.read_long(fcb + 58), metadata.parent_dir_id);
        assert_eq!(bus.read_pstring(fcb + 62), b"Player");
        let vcb = bus.read_long(fcb + 20);
        assert_ne!(vcb, 0);
        assert_eq!(bus.read_word(vcb + 78) as i16, -1);

        bus.write_word(TEST_SP, refnum);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_bytes(fcb, 94), vec![0; 94]);
        assert_eq!(bus.read_long(buffer + 2), 1234);
        bus.write_long(TEST_SP, name_ptr);
        cpu.write_reg(Register::A7, TEST_SP);
        disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(TEST_SP + 4), refnum, "reuse a closed FCB");
    }

    #[test]
    fn open_res_file_reports_exhausted_fcb_table_without_changing_current_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs_rsrc.insert("Player".to_string(), vec![]);
        for index in 0..342 {
            disp.open_files
                .insert(2 + 94 * index, "Occupied".to_string());
        }
        let current = disp.current_resource_refnum();
        bus.write_pstring(0x200250, b"Player");
        bus.write_long(TEST_SP, 0x200250);
        disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(TEST_SP + 4) as i16, -1);
        assert_eq!(bus.read_word(0x0A60) as i16, -42);
        assert_eq!(disp.current_resource_refnum(), current);
        assert_eq!(disp.refnum_for_resource_file_name("Player"), None);
    }

    // Inside Macintosh Volume I (1985), p. I-115: reopening an already-open
    // resource file returns its refnum but does not make it current.
    #[test]
    fn open_res_file_reopen_returns_existing_refnum_without_switching_current_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200260u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);
        disp.vfs_rsrc.insert("Sounds".to_string(), vec![]);

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let shapes_ref = bus.read_word(sp + 4);
        assert_ne!(shapes_ref as i16, -1);
        assert_eq!(cpu.read_reg(Register::D0), shapes_ref as u32);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let sounds_ref = bus.read_word(sp + 4);
        assert_ne!(sounds_ref as i16, -1);
        assert_ne!(sounds_ref, shapes_ref);
        assert_eq!(cpu.read_reg(Register::D0), sounds_ref as u32);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), shapes_ref);
        assert_eq!(cpu.read_reg(Register::D0), shapes_ref as u32);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn open_res_file_canonical_reopen_reuses_existing_refnum_without_switching_current_file() {
        // IM:I I-115: already-open OpenResFile returns the existing refnum
        // and does not make that file current. This holds even when the
        // second open spells the same file via a more explicit path that
        // resolves to the same VFS resource fork.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200265u32;

        disp.vfs_rsrc.insert("Folder/Shapes".to_string(), vec![]);
        disp.vfs_rsrc.insert("Sounds".to_string(), vec![]);

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let shapes_ref = bus.read_word(sp + 4);
        assert_ne!(shapes_ref as i16, -1);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);
        assert_eq!(disp.resource_file_name(shapes_ref), Some("Folder/Shapes"));

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let sounds_ref = bus.read_word(sp + 4);
        assert_ne!(sounds_ref as i16, -1);
        assert_ne!(sounds_ref, shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"Unix:Folder:Shapes");
        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // Inside Macintosh Volume I (1985), p. I-115: on failure OpenResFile
    // returns -1 and ResError reports the underlying file-system error.
    #[test]
    fn open_res_file_missing_returns_minus_one_and_fnferr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200270u32;

        bus.write_long(sp, name_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_pstring(name_ptr, b"NoSuchOpenResFile");

        let result = disp.dispatch_toolbox(true, 0x197, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), (-1i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-1i32) as u32);
        assert_eq!(bus.read_word(0x0A60), (-43i16) as u16);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // MTb 1993 pp. 1-62..1-64: HOpenResFile opens the requested resource
    // fork, returns the refnum, and makes it current.
    #[test]
    fn hopenresfile_success_returns_refnum_and_sets_current_resource_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x2002f0u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);

        bus.write_word(sp, 1); // fsRdPerm
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0); // dirID
        bus.write_word(sp + 10, 0); // vRefNum
        bus.write_word(sp + 12, 0xBEEF); // result
        bus.write_pstring(name_ptr, b"Shapes");

        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let refnum = bus.read_word(sp + 12);
        assert_ne!(refnum as i16, -1);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(disp.current_resource_refnum(), refnum);
        assert_eq!(disp.resource_file_name(refnum), Some("Shapes"));
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    // MTb 1993 p. 1-63: HOpenResFile returns -1 and ResError reports the file
    // error when it can't open the requested resource fork.
    #[test]
    fn hopenresfile_missing_file_returns_minus_one_and_fnferr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200300u32;

        bus.write_word(sp, 1); // fsRdPerm
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0); // dirID
        bus.write_word(sp + 10, 0); // vRefNum
        bus.write_word(sp + 12, 0xBEEF); // result
        bus.write_pstring(name_ptr, b"NoSuchHOpenFile");

        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 12), (-1i16) as u16);
        assert_eq!(bus.read_word(0x0A60), (-43i16) as u16);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    // MTb 1993 p. 1-63: already-open HOpenResFile returns the existing refnum
    // and does not make that file current.
    #[test]
    fn hopenresfile_already_open_returns_same_refnum_without_switching_current_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200400u32;

        disp.vfs_rsrc.insert("Shapes".to_string(), vec![]);
        disp.vfs_rsrc.insert("Sounds".to_string(), vec![]);

        // First open: Shapes.
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let shapes_ref = bus.read_word(sp + 12);
        assert_ne!(shapes_ref as i16, -1);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);

        // Second open: Sounds.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let sounds_ref = bus.read_word(sp + 12);
        assert_ne!(sounds_ref as i16, -1);
        assert_ne!(sounds_ref, shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);

        // Re-open Shapes: refnum re-used, current file unchanged.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    #[test]
    fn hopenresfile_canonical_reopen_reuses_existing_refnum_without_switching_current_file() {
        // MTb 1993 p. 1-63: already-open HOpenResFile returns the existing
        // refnum and does not make that file current, even when the reopen
        // uses a canonicalized path spelling for the same fork.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x200410u32;

        disp.vfs_rsrc.insert("Folder/Shapes".to_string(), vec![]);
        disp.vfs_rsrc.insert("Sounds".to_string(), vec![]);

        bus.write_word(sp, 1);
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 0xBEEF);
        bus.write_pstring(name_ptr, b"Shapes");
        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let shapes_ref = bus.read_word(sp + 12);
        assert_ne!(shapes_ref as i16, -1);
        assert_eq!(disp.current_resource_refnum(), shapes_ref);
        assert_eq!(disp.resource_file_name(shapes_ref), Some("Folder/Shapes"));

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 0xBEEF);
        bus.write_pstring(name_ptr, b"Sounds");
        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        let sounds_ref = bus.read_word(sp + 12);
        assert_ne!(sounds_ref as i16, -1);
        assert_ne!(sounds_ref, shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 1);
        bus.write_long(sp + 2, name_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0);
        bus.write_word(sp + 12, 0xBEEF);
        bus.write_pstring(name_ptr, b"Unix:Folder:Shapes");
        let result = disp.dispatch_toolbox(true, 0x01A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), shapes_ref);
        assert_eq!(disp.current_resource_refnum(), sounds_ref);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    // CloseResFile ($A99A)
    #[test]
    fn test_close_res_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 2); // refNum

        let result = disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    #[test]
    fn closeresfile_releases_loaded_resource_memory_and_handles() {
        let (mut disp, mut cpu, mut bus) = setup();
        let refnum = 2;
        let data_ptr = bus.alloc(8);
        bus.write_bytes(data_ptr, &[0xAB; 8]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([
                (0, crate::trap::dispatch::ResourceFileMap::default()),
                (
                    refnum,
                    crate::trap::dispatch::ResourceFileMap {
                        loaded: std::collections::HashMap::from([((*b"PICT", 23002), data_ptr)]),
                        named: std::collections::HashMap::new(),
                        names_by_id: std::collections::HashMap::new(),
                        attrs: std::collections::HashMap::new(),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: std::collections::HashMap::from([(refnum, "BladeData".to_string())]),
            search_order: vec![0, refnum],
            current_file: refnum,
        });
        disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"PICT", 23002));
        disp.insert_resource_handle_file_for_test(handle, refnum);
        disp.track_handle_ptr(data_ptr, handle);
        bus.write_word(0x0A5A, refnum);

        let sp = TEST_SP;
        bus.write_word(sp, refnum);

        let result = disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(disp.current_resource_refnum(), 0);
        assert_eq!(bus.get_alloc_size(data_ptr), None);
        assert_eq!(bus.get_alloc_size(handle), None);
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert!(!disp.resource_handle_files.contains_key(&handle));
        assert_eq!(disp.handle_for_ptr(data_ptr), None);
        assert!(!disp.resources.as_ref().unwrap().files.contains_key(&refnum));
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn test_close_res_file_invalid_refnum_sets_basiliskii_resfnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([
                (0, crate::trap::dispatch::ResourceFileMap::default()),
                (2, crate::trap::dispatch::ResourceFileMap::default()),
            ]),
            names: std::collections::HashMap::new(),
            search_order: vec![0, 2],
            current_file: 2,
        });
        bus.write_word(0x0A5A, 2);
        bus.write_word(0x0A60, 0);

        let sp = TEST_SP;
        bus.write_word(sp, (-2i16) as u16);

        let result = disp.dispatch_toolbox(true, 0x19A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(disp.current_resource_refnum(), 2);
        assert_eq!(bus.read_word(0x0A5A), 2);
        assert_eq!(bus.read_word(0x0A60), (-193i16) as u16);
    }

    // UseResFile ($A998)
    #[test]
    fn test_use_res_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([
                (0, crate::trap::dispatch::ResourceFileMap::default()),
                (2, crate::trap::dispatch::ResourceFileMap::default()),
            ]),
            names: std::collections::HashMap::new(),
            search_order: vec![0, 2],
            current_file: 0,
        });
        let sp = TEST_SP;
        bus.write_word(sp, 2); // refNum

        let result = disp.dispatch_toolbox(true, 0x198, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(disp.current_resource_refnum(), 2);
        assert_eq!(bus.read_word(0x0A5A), 2);
    }

    #[test]
    fn test_use_res_file_invalid_refnum_preserves_current_and_sets_resfnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([
                (0, crate::trap::dispatch::ResourceFileMap::default()),
                (2, crate::trap::dispatch::ResourceFileMap::default()),
            ]),
            names: std::collections::HashMap::new(),
            search_order: vec![0, 2],
            current_file: 2,
        });
        bus.write_word(0x0A5A, 2);
        bus.write_word(0x0A60, 0);

        let sp = TEST_SP;
        bus.write_word(sp, (-2i16) as u16);

        let result = disp.dispatch_toolbox(true, 0x198, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(disp.current_resource_refnum(), 2);
        assert_eq!(bus.read_word(0x0A5A), 2);
        assert_eq!(bus.read_word(0x0A60), (-193i16) as u16);
    }

    #[test]
    fn test_use_res_file_with_cur_apref_num_maps_to_internal_app_file_zero() {
        use crate::memory::globals::addr;
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_loaded_resources_for_test(crate::trap::dispatch::LoadedResources {
            files: std::collections::HashMap::from([
                (0, crate::trap::dispatch::ResourceFileMap::default()),
                (96, crate::trap::dispatch::ResourceFileMap::default()),
            ]),
            names: std::collections::HashMap::new(),
            search_order: vec![96, 0],
            current_file: 96,
        });
        bus.write_word(addr::CUR_APREF_NUM, 2);
        bus.write_word(0x0A5A, 96);
        bus.write_word(0x0A60, 0);

        let sp = TEST_SP;
        bus.write_word(sp, 2); // guest FCB refnum matching CUR_APREF_NUM

        let result = disp.dispatch_toolbox(true, 0x198, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(disp.current_resource_refnum(), 0);
        assert_eq!(bus.read_word(0x0A5A), 0);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn uniqueid_scans_all_open_files_while_unique1id_uses_current_file_only() {
        // Inside Macintosh Volume I (1985), p. I-121:
        // UniqueID searches all open resource files for the type.
        // Inside Macintosh Volume IV (1986), p. IV-16:
        // Unique1ID applies the same rule to the current file only.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.install_test_resource_in_file(&mut bus, 0, *b"STR ", 128, &[0x11]);
        disp.install_test_resource_in_file(&mut bus, 2, *b"STR ", 129, &[0x22]);
        disp.with_resource_manager_mut(|resource_manager| {
            if let Some(resources) = resource_manager.resources.as_mut() {
                resources.current_file = 0;
            }
        });

        let sp = TEST_SP;
        bus.write_long(sp, u32::from_be_bytes(*b"STR "));
        bus.write_word(sp + 4, 0xBEEF);
        let uniqueid = disp.dispatch_toolbox(true, 0x1C1, &mut cpu, &mut bus);
        assert!(uniqueid.is_some());
        assert!(uniqueid.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 130);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, u32::from_be_bytes(*b"STR "));
        bus.write_word(sp + 4, 0xBEEF);
        let unique1id = disp.dispatch_toolbox(true, 0x010, &mut cpu, &mut bus);
        assert!(unique1id.is_some());
        assert!(unique1id.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 129);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn uniqueid_family_returns_128_when_requested_type_is_absent() {
        // Inside Macintosh Volume I (1985), p. I-121 and Volume IV (1986),
        // p. IV-16: both routines return an unused ID for the requested type.
        // In Systemless's HLE, candidate scans begin at 128.
        let (mut disp, mut cpu, mut bus) = setup();

        disp.install_test_resource_in_file(&mut bus, 0, *b"MENU", 200, &[0x33]);
        disp.install_test_resource_in_file(&mut bus, 3, *b"MENU", 201, &[0x44]);
        disp.with_resource_manager_mut(|resource_manager| {
            if let Some(resources) = resource_manager.resources.as_mut() {
                resources.current_file = 0;
            }
        });

        let sp = TEST_SP;
        bus.write_long(sp, u32::from_be_bytes(*b"STR "));
        bus.write_word(sp + 4, 0xBEEF);
        let uniqueid = disp.dispatch_toolbox(true, 0x1C1, &mut cpu, &mut bus);
        assert!(uniqueid.is_some());
        assert!(uniqueid.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 128);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, u32::from_be_bytes(*b"STR "));
        bus.write_word(sp + 4, 0xBEEF);
        let unique1id = disp.dispatch_toolbox(true, 0x010, &mut cpu, &mut bus);
        assert!(unique1id.is_some());
        assert!(unique1id.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 128);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // Inside Macintosh Volume I (1985), p. I-124: RmvResource removes the
    // current-file map reference but does not dispose handle memory.
    #[test]
    fn rmveresource_removes_current_file_reference_without_disposing_handle_data() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(6);
        bus.write_bytes(data_ptr, &[0x10, 0x20, 0x30, 0x40, 0x50, 0x60]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"TEST", 7), data_ptr)]),
                    named: HashMap::from([((*b"TEST", "Sample".to_string()), (7, data_ptr))]),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::from([((*b"TEST", 7), 0u8)]),
                    map_attrs: 0,
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"TEST", 7));
        disp.insert_resource_handle_file_for_test(handle, 0);

        let sp = TEST_SP;
        bus.write_long(sp, handle);
        let result = disp.dispatch_toolbox(true, 0x1AD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert!(!disp.resource_handle_files.contains_key(&handle));
        let file = &disp.resources.as_ref().unwrap().files[&0];
        assert!(!file.loaded.contains_key(&(*b"TEST", 7)));
        assert!(!file.attrs.contains_key(&(*b"TEST", 7)));
        assert!(file.named.is_empty());
    }

    // Inside Macintosh Volume I (1985), p. I-124: resProtected resources are
    // not removed and RmvResource returns rmvResFailed.
    #[test]
    fn rmveresource_protected_handle_returns_rmvresfailed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(4);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"PROT", 9), data_ptr)]),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::from([((*b"PROT", 9), 0x0008u8)]),
                    map_attrs: 0,
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"PROT", 9));
        disp.insert_resource_handle_file_for_test(handle, 0);

        let sp = TEST_SP;
        bus.write_long(sp, handle);
        let result = disp.dispatch_toolbox(true, 0x1AD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60) as i16, -196);
        assert!(disp.loaded_handles.contains_key(&handle));
        assert!(disp.resources.as_ref().unwrap().files[&0]
            .loaded
            .contains_key(&(*b"PROT", 9)));
    }

    // Inside Macintosh Volume I (1985), p. I-124: noncurrent-file resources
    // fail with rmvResFailed.
    #[test]
    fn rmveresource_noncurrent_file_handle_returns_rmvresfailed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(4);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([
                (0, ResourceFileMap::default()),
                (
                    2,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"OTHR", 3), data_ptr)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::from([((*b"OTHR", 3), 0u8)]),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: HashMap::new(),
            search_order: vec![0, 2],
            current_file: 0,
        });
        disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"OTHR", 3));
        disp.insert_resource_handle_file_for_test(handle, 2);

        let sp = TEST_SP;
        bus.write_long(sp, handle);
        let result = disp.dispatch_toolbox(true, 0x1AD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60) as i16, -196);
        assert!(disp.loaded_handles.contains_key(&handle));
        assert!(disp.resources.as_ref().unwrap().files[&2]
            .loaded
            .contains_key(&(*b"OTHR", 3)));
    }

    // Inside Macintosh Volume I (1985), p. I-124: non-resource handles return
    // rmvResFailed.
    #[test]
    fn rmveresource_non_resource_handle_returns_rmvresfailed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let fake_ptr = bus.alloc(4);
        let fake_handle = bus.alloc(4);
        bus.write_long(fake_handle, fake_ptr);

        let sp = TEST_SP;
        bus.write_long(sp, fake_handle);
        let result = disp.dispatch_toolbox(true, 0x1AD, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60) as i16, -196);
        assert_eq!(bus.read_long(fake_handle), fake_ptr);
    }

    // Inside Macintosh Volume I (1985), p. I-124: RmveReference is an
    // obsolete alias for RmveResource / RemoveResource and leaves the
    // handle data allocated.
    #[test]
    fn rmverereference_removes_current_file_reference_without_disposing_handle_data() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(6);
        bus.write_bytes(data_ptr, &[0x10, 0x20, 0x30, 0x40, 0x50, 0x60]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"TEST", 7), data_ptr)]),
                    named: HashMap::from([((*b"TEST", "Sample".to_string()), (7, data_ptr))]),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::from([((*b"TEST", 7), 0u8)]),
                    map_attrs: 0,
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"TEST", 7));
        disp.insert_resource_handle_file_for_test(handle, 0);

        let sp = TEST_SP;
        bus.write_long(sp, handle);
        let result = disp.dispatch_toolbox(true, 0x1AE, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(bus.read_long(handle), data_ptr);
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert!(!disp.resource_handle_files.contains_key(&handle));
        let file = &disp.resources.as_ref().unwrap().files[&0];
        assert!(!file.loaded.contains_key(&(*b"TEST", 7)));
        assert!(!file.attrs.contains_key(&(*b"TEST", 7)));
        assert!(file.named.is_empty());
    }

    // Inside Macintosh Volume I (1985), p. I-125: UpdateResFile flushes
    // changed state for the requested open resource file.
    #[test]
    fn updateresfile_clears_reschanged_bits_for_target_open_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let changed = super::super::TrapDispatcher::RES_CHANGED_ATTR as u8;

        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([
                (
                    0,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"CURR", 1), 0x1000)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::from([((*b"CURR", 1), changed)]),
                        map_attrs: 0,
                    },
                ),
                (
                    2,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"TARG", 2), 0x2000)]),
                        named: HashMap::new(),
                        names_by_id: HashMap::new(),
                        attrs: HashMap::from([((*b"TARG", 2), changed | 0x0008u8)]),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: HashMap::new(),
            search_order: vec![0, 2],
            current_file: 0,
        });

        let sp = TEST_SP;
        bus.write_word(sp, 2);
        let result = disp.dispatch_toolbox(true, 0x199, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(0x0A60), 0);

        let resources = disp.resources.as_ref().unwrap();
        let target_attrs = resources.files[&2].attrs[&(*b"TARG", 2)];
        let current_attrs = resources.files[&0].attrs[&(*b"CURR", 1)];
        assert_eq!(target_attrs & changed, 0);
        assert_eq!(target_attrs & 0x0008u8, 0x0008u8);
        assert_ne!(current_attrs & changed, 0);
    }

    // Inside Macintosh Volume I (1985), p. I-125: UpdateResFile on an unknown
    // refNum reports resFNotFound.
    #[test]
    fn updateresfile_invalid_refnum_returns_resfnotfound() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(0, ResourceFileMap::default())]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        let sp = TEST_SP;
        bus.write_word(sp, 99);
        bus.write_word(0x0A60, 0);
        let result = disp.dispatch_toolbox(true, 0x199, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(0x0A60) as i16, -193);
    }

    // Inside Macintosh Volume I (1985), p. I-126: SetResPurge takes one
    // BOOLEAN argument and therefore consumes one word from the stack.
    #[test]
    fn setrespurge_reads_high_byte_boolean_and_consumes_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0x00FF);
        bus.write_word(sp + 2, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x193, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert!(!disp.policy.res_purge());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(sp + 2), 0xBEEF);
    }

    // Inside Macintosh Volume I (1985), p. I-126: SetResPurge installs
    // or removes the resource purge handler based on install.
    #[test]
    fn setrespurge_toggles_resource_purge_install_flag() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x0100);
        let result = disp.dispatch_toolbox(true, 0x193, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert!(disp.policy.res_purge());

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x00FF);
        let result = disp.dispatch_toolbox(true, 0x193, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert!(!disp.policy.res_purge());
    }

    fn write_drag_region_frame(
        bus: &mut crate::memory::MacMemoryBus,
        sp: u32,
        start: (i16, i16),
        limit_rect_ptr: u32,
        slop_rect_ptr: u32,
        axis: i16,
    ) {
        bus.write_long(sp, 0);
        bus.write_word(sp + 4, axis as u16);
        bus.write_long(sp + 6, slop_rect_ptr);
        bus.write_long(sp + 10, limit_rect_ptr);
        bus.write_word(sp + 14, start.0 as u16);
        bus.write_word(sp + 16, start.1 as u16);
        bus.write_long(sp + 18, 0x300000);
        bus.write_long(sp + 22, 0xDEAD_BEEF);
    }

    fn write_test_rect(
        bus: &mut crate::memory::MacMemoryBus,
        ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        bus.write_word(ptr, rect.0 as u16);
        bus.write_word(ptr + 2, rect.1 as u16);
        bus.write_word(ptr + 4, rect.2 as u16);
        bus.write_word(ptr + 6, rect.3 as u16);
    }

    // IM:I I-302 + IM:I I-91: DragGrayRgn is the gray-outline alias of
    // DragTheRgn, and the outside-slop path returns $80008000.
    #[test]
    fn draggrayrgn_returns_no_drag_sentinel_outside_sloprect_and_consumes_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 22;
        cpu.write_reg(Register::A7, sp);
        let limit_rect = 0x240000;
        let slop_rect = 0x240008;
        write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
        write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
        write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
        disp.set_mouse_position(140, 20);

        let result = disp.dispatch_toolbox(true, 0x105, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), 0x8000_8000);
    }

    #[test]
    fn draggrayrgn_returns_offset_inside_sloprect() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 22;
        cpu.write_reg(Register::A7, sp);
        let limit_rect = 0x240000;
        let slop_rect = 0x240008;
        write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
        write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
        write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
        disp.set_mouse_position(30, 50);

        let result = disp.dispatch_toolbox(true, 0x105, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), 0x0014_001E);
    }

    #[test]
    fn draggrayrgn_retains_tracking_until_release_and_returns_live_offset() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP - 22;
        cpu.write_reg(Register::A7, sp);
        let limit_rect = 0x240000;
        let slop_rect = 0x240008;
        write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
        write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
        write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
        let region = test_region_handle(&mut bus, 5, 10, 45, 70);
        bus.write_long(sp + 18, region);

        disp.push_mouse_down(10, 20);
        let result = disp.dispatch_toolbox(true, 0x105, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert!(disp.region_tracking.is_some());

        disp.set_mouse_position(30, 50);
        let result = disp.dispatch_toolbox(true, 0x105, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            disp.region_tracking.as_ref().unwrap().outline_rect,
            Some((25, 40, 65, 100))
        );

        disp.push_mouse_up(30, 50);
        let result = disp.dispatch_toolbox(true, 0x105, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
        assert_eq!(bus.read_long(TEST_SP), 0x0014_001E);
        assert!(disp.region_tracking.is_none());
        assert!(disp.event_queue.iter().all(|event| event.what != 2));
    }

    // SetResLoad ($A99B) — Mac Pascal Boolean is in the high byte
    #[test]
    fn test_set_res_load_reads_high_byte_boolean() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // IM:More Macintosh Toolbox 1993, 1-79 plus MPW stack convention:
        // Boolean FALSE is $00 in the high byte. The low byte is padding and
        // must not turn the parameter true.
        disp.policy.set_res_load(true);
        bus.write_word(sp, 0x00FF);
        bus.write_word(0x0A60, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x19B, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert!(!disp.policy.res_load());
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);

        // TRUE is $01 in the high byte.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(0x0A60, 0xBEEF);
        bus.write_word(sp, 0x0100);
        let result = disp.dispatch_toolbox(true, 0x19B, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert!(disp.policy.res_load());
        assert_eq!(bus.read_word(0x0A60), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    // CountResources ($A99C)
    #[test]
    fn test_count_resources() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // SP+0: type(4)
        bus.write_long(sp, 0x49434E23); // 'ICN#'
        bus.write_word(sp + 4, 0xBEEF); // result placeholder

        let result = disp.dispatch_toolbox(true, 0x19C, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // Count1Resources ($A80D)
    #[test]
    fn test_count1_resources() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x49434E23); // 'ICN#'
        bus.write_word(sp + 4, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x00D, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn get_ind_resource_miss_writes_nil_handle_result() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.install_test_resource(&mut bus, *b"crsr", 128, &[0xAA, 0xBB]);

        for (trap, name) in [(0x19D, "GetIndResource"), (0x00E, "Get1IndResource")] {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::A0, 0xCAFE_BABE);
            cpu.write_reg(Register::D0, 0);
            bus.write_word(0x0A60, 0);
            bus.write_word(TEST_SP, 2); // Index is 1-based; only one crsr exists.
            bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"crsr"));
            bus.write_long(TEST_SP + 6, 0xDEAD_BEEF);

            let result = disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus);
            assert!(result.is_some(), "{name} should be handled");
            assert!(result.unwrap().is_ok(), "{name} should return normally");

            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6, "{name} SP");
            assert_eq!(cpu.read_reg(Register::A0), 0, "{name} A0");
            assert_eq!(bus.read_long(TEST_SP + 6), 0, "{name} result slot");
            assert_eq!(bus.read_word(0x0A60) as i16, -192, "{name} ResErr");
        }
    }

    #[test]
    fn get_ind_resource_keeps_unloaded_data_empty_when_loading_is_disabled() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(8);
        bus.write_bytes(data_ptr, &[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80]);
        disp.policy.set_res_load(false);
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"seg!", 1), data_ptr)]),
                    ..ResourceFileMap::default()
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"seg!"));
        bus.write_long(TEST_SP + 6, 0);

        let result = disp.dispatch_toolbox(true, 0x19D, &mut cpu, &mut bus);

        assert!(result.is_some() && result.unwrap().is_ok());
        let handle = bus.read_long(TEST_SP + 6);
        assert_ne!(handle, 0);
        assert_eq!(bus.read_long(handle), 0);
        assert!(!disp.resident_resources.contains(&(0, *b"seg!", 1)));
        assert_eq!(cpu.read_reg(Register::A0), handle);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 6);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    #[test]
    fn get_ind_resource_reloads_released_map_entry_when_loading_is_enabled() {
        let (mut disp, mut cpu, mut bus) = setup();
        let bytes = vec![0x10, 0x20, 0x30, 0x40];
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"CODE", 1), 0)]),
                    ..ResourceFileMap::default()
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        disp.remember_resource_backing_data(0, *b"CODE", 1, bytes.clone());
        let handle = disp.get_or_create_resource_handle_in_file(&mut bus, *b"CODE", 1, 0, 0);
        assert_eq!(bus.read_long(handle), 0);
        disp.policy.set_res_load(true);
        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"CODE"));
        bus.write_long(TEST_SP + 6, 0);

        let result = disp.dispatch_toolbox(true, 0x00E, &mut cpu, &mut bus);

        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(bus.read_long(TEST_SP + 6), handle);
        let ptr = bus.read_long(handle);
        assert_ne!(ptr, 0);
        assert_eq!(bus.read_bytes(ptr, bytes.len()), bytes);
        assert_eq!(disp.loaded_handles.get(&handle).unwrap().0, ptr);
        assert_eq!(disp.resource_handle_files.get(&handle), Some(&0));
    }

    #[test]
    fn get_ind_resource_preserves_resource_map_reference_order() {
        let (mut disp, mut cpu, mut bus) = setup();
        let first_ptr = bus.alloc(4);
        let second_ptr = bus.alloc(4);
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([
                        ((*b"CODE", 1), second_ptr),
                        ((*b"CODE", 2), first_ptr),
                    ]),
                    ..ResourceFileMap::default()
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        disp.with_resource_manager_mut(|resource_manager| {
            resource_manager
                .resource_file_order
                .insert(0, vec![(*b"CODE", 2), (*b"CODE", 1)]);
        });

        bus.write_word(TEST_SP, 1);
        bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"CODE"));
        bus.write_long(TEST_SP + 6, 0);

        let result = disp.dispatch_toolbox(true, 0x00E, &mut cpu, &mut bus);

        assert!(result.is_some() && result.unwrap().is_ok());
        let handle = bus.read_long(TEST_SP + 6);
        assert_eq!(bus.read_long(handle), first_ptr);
        assert_eq!(disp.loaded_handles.get(&handle).unwrap().2, 2);
    }

    #[test]
    fn get_ind_resource_preserves_data_loaded_before_set_res_load_false() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(8);
        bus.write_bytes(data_ptr, &[0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80]);
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"seg!", 1), data_ptr)]),
                    ..ResourceFileMap::default()
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        for res_load in [true, false] {
            disp.policy.set_res_load(res_load);
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_word(TEST_SP, 1);
            bus.write_long(TEST_SP + 2, u32::from_be_bytes(*b"seg!"));
            bus.write_long(TEST_SP + 6, 0);

            let result = disp.dispatch_toolbox(true, 0x19D, &mut cpu, &mut bus);
            assert!(result.is_some() && result.unwrap().is_ok());
            let handle = bus.read_long(TEST_SP + 6);
            assert_ne!(handle, 0);
            assert_eq!(bus.read_long(handle), data_ptr);
        }
        assert!(disp.resident_resources.contains(&(0, *b"seg!", 1)));
    }

    // GetNamedResource ($A9A1)
    #[test]
    fn test_get_named_resource_searches_resource_chain() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(16);
        bus.write_bytes(data_ptr, &[0x42; 16]);

        let mut loaded = HashMap::new();
        loaded.insert((*b"STR ", 500i16), data_ptr);
        let mut named = HashMap::new();
        named.insert((*b"STR ", "MyString".to_string()), (500i16, data_ptr));

        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([
                (0, ResourceFileMap::default()),
                (
                    2,
                    ResourceFileMap {
                        loaded,
                        named,
                        names_by_id: HashMap::new(),
                        attrs: HashMap::new(),
                        map_attrs: 0,
                    },
                ),
            ]),
            names: HashMap::new(),
            search_order: vec![0, 2],
            current_file: 2,
        });

        let name_addr = 0x200000u32;
        bus.write_byte(name_addr, 8);
        bus.write_bytes(name_addr + 1, b"MyString");

        let sp = TEST_SP;
        bus.write_long(sp, name_addr);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"STR "));

        let result = disp.dispatch_toolbox(true, 0x1A1, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(0x0A60), 0);
        let handle = bus.read_long(sp + 8);
        assert_ne!(handle, 0);
        assert_eq!(cpu.read_reg(Register::A0), handle);
    }

    #[test]
    fn get_named_resource_reloads_after_release() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data = [0x42, 0x43, 0x44, 0x45];
        let data_ptr =
            disp.install_named_test_resource_in_file(&mut bus, 0, *b"TEST", 500, "ReloadMe", &data);
        let name_addr = 0x300000u32;
        bus.write_byte(name_addr, 8);
        bus.write_bytes(name_addr + 1, b"ReloadMe");

        let sp = TEST_SP;
        bus.write_long(sp, name_addr);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEST"));
        let result = disp.dispatch_toolbox(true, 0x1A1, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let first_handle = bus.read_long(sp + 8);
        assert_eq!(bus.read_long(first_handle), data_ptr);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, first_handle);
        let result = disp.dispatch_resource(true, 0x1A3, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, name_addr);
        bus.write_long(sp + 4, u32::from_be_bytes(*b"TEST"));
        let result = disp.dispatch_toolbox(true, 0x1A1, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let second_handle = bus.read_long(sp + 8);
        let second_ptr = bus.read_long(second_handle);
        assert_ne!(second_handle, first_handle);
        assert_ne!(second_ptr, 0);
        assert_eq!(bus.read_bytes(second_ptr, data.len()), data);
    }

    // GetResAttrs ($A9A6) — handler now lives in resource.rs (see
    // dispatcher chain in dispatch.rs: dispatch_resource runs before
    // dispatch_toolbox). Test against dispatch_resource directly so the
    // assertion exercises the canonical path.
    #[test]
    fn test_get_res_attrs() {
        let (mut disp, mut cpu, mut bus) = setup();
        let data_ptr = bus.alloc(8);
        bus.write_bytes(data_ptr, &[0x11; 8]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);
        disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"TEST", 7));
        disp.insert_resource_handle_file_for_test(handle, 0);
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([((*b"TEST", 7), data_ptr)]),
                    named: HashMap::new(),
                    names_by_id: HashMap::new(),
                    attrs: HashMap::from([((*b"TEST", 7), 0x002Cu8)]),
                    map_attrs: 0,
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });

        let sp = TEST_SP;
        // SP+0: handle(4)
        bus.write_long(sp, handle);
        bus.write_word(sp + 4, 0xBEEF);

        let result = disp.dispatch_resource(true, 0x1A6, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0x002C);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(0x0A60), 0);
    }

    // ========== Misc Toolbox ==========

    // Inside Macintosh Volume I (1985), p. I-287: DrawGrowIcon draws
    // delimiter lines 15 pixels in from the right/bottom edges of portRect.
    #[test]
    fn drawgrowicon_draws_scrollbar_delimiter_lines_15_pixels_in_from_portrect_edges() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(64 * 342);
        disp.screen_mode = (screen_base, 64, 512, 342, 1);

        let window_ptr = bus.alloc(256);
        bus.write_word(window_ptr + 6, 0x0000); // GrafPort path, not CGrafPort
        bus.write_word(window_ptr + 8, 0); // portBits.bounds.top
        bus.write_word(window_ptr + 10, 0); // portBits.bounds.left
        bus.write_word(window_ptr + 16, 30); // portRect.top
        bus.write_word(window_ptr + 18, 20); // portRect.left
        bus.write_word(window_ptr + 20, 70); // portRect.bottom
        bus.write_word(window_ptr + 22, 100); // portRect.right

        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);

        let result = disp.dispatch_toolbox(true, 0x104, &mut cpu, &mut bus);
        assert!(result.is_some(), "DrawGrowIcon should be handled");
        assert!(result.unwrap().is_ok(), "DrawGrowIcon should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "DrawGrowIcon should pop one WindowPtr argument"
        );

        let sep_x = 100 - 15;
        let sep_y = 70 - 15;
        let row_bytes = 64;

        assert!(
            read_screen_pixel_1bpp(&bus, screen_base, row_bytes, sep_x, 30),
            "vertical delimiter should start at content top"
        );
        assert!(
            read_screen_pixel_1bpp(&bus, screen_base, row_bytes, sep_x, 69),
            "vertical delimiter should reach content bottom-1"
        );
        assert!(
            read_screen_pixel_1bpp(&bus, screen_base, row_bytes, 19, sep_y),
            "horizontal delimiter should start at content left-1"
        );
        assert!(
            read_screen_pixel_1bpp(&bus, screen_base, row_bytes, 101, sep_y),
            "horizontal delimiter should reach content right+1"
        );

        bus.write_byte(window_ptr + 111, 0xFF); // WindowRecord.hilited
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        let result = disp.dispatch_toolbox(true, 0x104, &mut cpu, &mut bus);
        assert!(result.is_some(), "active DrawGrowIcon should be handled");
        assert!(
            result.unwrap().is_ok(),
            "active DrawGrowIcon should succeed"
        );
        assert!(
            read_screen_pixel_1bpp(&bus, screen_base, row_bytes, 87, 68),
            "active document window should draw the longest diagonal grip"
        );
    }

    // Inside Macintosh Volume I (1985), p. I-287: DrawGrowIcon is a
    // PROCEDURE DrawGrowIcon(theWindow: WindowPtr).
    #[test]
    fn drawgrowicon_consumes_windowptr_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let window_ptr = 0x234000u32;
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);

        let result = disp.dispatch_toolbox(true, 0x104, &mut cpu, &mut bus);
        assert!(result.is_some(), "DrawGrowIcon should be handled");
        assert!(result.unwrap().is_ok(), "DrawGrowIcon should succeed");
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "DrawGrowIcon should pop one WindowPtr argument"
        );
    }

    // Munger ($A9E0)
    // IM:I 1985 p. I-468: replace first found target and return offset of
    // first byte past replacement.
    #[test]
    fn munger_replaces_first_occurrence_and_returns_offset_past_replacement() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ptr1 = 0x310000u32;
        let ptr2 = 0x310100u32;
        bus.write_bytes(ptr1, b"^0");
        bus.write_bytes(ptr2, b"Ace");

        let data_ptr = bus.alloc(9);
        bus.write_bytes(data_ptr, b"Hello, ^0");
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        bus.write_long(sp, 3); // len2
        bus.write_long(sp + 4, ptr2);
        bus.write_long(sp + 8, 2); // len1
        bus.write_long(sp + 12, ptr1);
        bus.write_long(sp + 16, 0); // offset
        bus.write_long(sp + 20, handle);
        bus.write_long(sp + 24, 0);

        let result = disp.dispatch_toolbox(true, 0x1E0, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 24), 10);
        assert_eq!(cpu.read_reg(Register::A7), sp + 24);

        let final_ptr = bus.read_long(handle);
        let final_size = bus.get_alloc_size(final_ptr).unwrap();
        assert_eq!(
            bus.read_bytes(final_ptr, final_size as usize),
            b"Hello, Ace"
        );
    }

    // XMunger ($A819) is a phantom trap word that BasiliskII exposes
    // as a no-op/no-pop stub.
    #[test]
    fn xmunger_phantom_noop_leaves_handle_unchanged_and_stack_unbalanced() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ptr1 = 0x312000u32;
        let ptr2 = 0x312100u32;
        bus.write_bytes(ptr1, b"^0");
        bus.write_bytes(ptr2, b"Ace");

        let data_ptr = bus.alloc(9);
        bus.write_bytes(data_ptr, b"Hello, ^0");
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        bus.write_long(sp, 3); // len2
        bus.write_long(sp + 4, ptr2);
        bus.write_long(sp + 8, 2); // len1
        bus.write_long(sp + 12, ptr1);
        bus.write_long(sp + 16, 0); // offset
        bus.write_long(sp + 20, handle);
        bus.write_long(sp + 24, 0);

        let result = disp.dispatch_toolbox(true, 0x019, &mut cpu, &mut bus);

        assert!(result.is_some(), "XMunger should be handled");
        assert!(result.unwrap().is_ok(), "XMunger should succeed");
        assert_eq!(bus.read_long(sp + 24), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(
            bus.read_long(handle),
            data_ptr,
            "XMunger should leave the handle pointer unchanged"
        );

        let final_ptr = bus.read_long(handle);
        let final_size = bus.get_alloc_size(final_ptr).unwrap();
        assert_eq!(bus.read_bytes(final_ptr, final_size as usize), b"Hello, ^0");
    }

    // IM:I 1985 p. I-469: if ptr2 is NIL, return match offset and leave
    // destination bytes unchanged.
    #[test]
    fn munger_search_only_mode_returns_match_offset_without_modifying_destination() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ptr1 = 0x310000u32;
        bus.write_bytes(ptr1, b"the");

        let initial = b"there's the apple";
        let data_ptr = bus.alloc(initial.len() as u32);
        bus.write_bytes(data_ptr, initial);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        bus.write_long(sp, 0); // len2
        bus.write_long(sp + 4, 0); // ptr2 = NIL
        bus.write_long(sp + 8, 3); // len1
        bus.write_long(sp + 12, ptr1);
        bus.write_long(sp + 16, 4); // offset
        bus.write_long(sp + 20, handle);
        bus.write_long(sp + 24, 0);

        let result = disp.dispatch_toolbox(true, 0x1E0, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 24), 8);
        assert_eq!(cpu.read_reg(Register::A7), sp + 24);

        let final_ptr = bus.read_long(handle);
        let final_size = bus.get_alloc_size(final_ptr).unwrap();
        assert_eq!(bus.read_bytes(final_ptr, final_size as usize), initial);
    }

    // IM:I 1985 p. I-469: len1 == 0 inserts replacement bytes at offset and
    // returns first byte past insertion.
    #[test]
    fn munger_len1_zero_inserts_replacement_at_offset() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ptr2 = 0x310100u32;
        bus.write_bytes(ptr2, b"X");

        let data_ptr = bus.alloc(5);
        bus.write_bytes(data_ptr, b"apple");
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        bus.write_long(sp, 1); // len2
        bus.write_long(sp + 4, ptr2);
        bus.write_long(sp + 8, 0); // len1
        bus.write_long(sp + 12, 0); // ptr1 ignored
        bus.write_long(sp + 16, 2); // offset
        bus.write_long(sp + 20, handle);
        bus.write_long(sp + 24, 0);

        let result = disp.dispatch_toolbox(true, 0x1E0, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 24), 3);
        assert_eq!(cpu.read_reg(Register::A7), sp + 24);

        let final_ptr = bus.read_long(handle);
        let final_size = bus.get_alloc_size(final_ptr).unwrap();
        assert_eq!(bus.read_bytes(final_ptr, final_size as usize), b"apXple");
    }

    // IM:I 1985 p. I-469: len2 == 0 with non-NIL ptr2 deletes the target
    // substring and returns the deletion offset.
    #[test]
    fn munger_len2_zero_deletes_target_and_returns_deletion_offset() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ptr1 = 0x310000u32;
        let ptr2 = 0x310100u32;
        bus.write_bytes(ptr1, b"123");
        bus.write_bytes(ptr2, b"z");

        let data_ptr = bus.alloc(9);
        bus.write_bytes(data_ptr, b"abc123def");
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        bus.write_long(sp, 0); // len2
        bus.write_long(sp + 4, ptr2); // ptr2 non-NIL keeps delete path
        bus.write_long(sp + 8, 3); // len1
        bus.write_long(sp + 12, ptr1);
        bus.write_long(sp + 16, 0); // offset
        bus.write_long(sp + 20, handle);
        bus.write_long(sp + 24, 0);

        let result = disp.dispatch_toolbox(true, 0x1E0, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 24), 3);
        assert_eq!(cpu.read_reg(Register::A7), sp + 24);

        let final_ptr = bus.read_long(handle);
        let final_size = bus.get_alloc_size(final_ptr).unwrap();
        assert_eq!(bus.read_bytes(final_ptr, final_size as usize), b"abcdef");
    }

    // IM:I 1985 p. I-469: returns a negative value when target is not found.
    #[test]
    fn munger_returns_negative_when_target_not_found() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ptr1 = 0x310000u32;
        let ptr2 = 0x310100u32;
        bus.write_bytes(ptr1, b"zz");
        bus.write_bytes(ptr2, b"A");

        let data_ptr = bus.alloc(5);
        bus.write_bytes(data_ptr, b"hello");
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        bus.write_long(sp, 1); // len2
        bus.write_long(sp + 4, ptr2);
        bus.write_long(sp + 8, 2); // len1
        bus.write_long(sp + 12, ptr1);
        bus.write_long(sp + 16, 0); // offset
        bus.write_long(sp + 20, handle);
        bus.write_long(sp + 24, 0);

        let result = disp.dispatch_toolbox(true, 0x1E0, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 24) as i32, -1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 24);

        let final_ptr = bus.read_long(handle);
        let final_size = bus.get_alloc_size(final_ptr).unwrap();
        assert_eq!(bus.read_bytes(final_ptr, final_size as usize), b"hello");
    }

    // BasiliskII/System 7.5 ROM: if the tail at offset only partially
    // matches the beginning of the target, return -1 and leave the
    // destination unchanged.
    #[test]
    fn munger_partial_tail_match_at_offset_returns_negative_and_leaves_tail_unchanged() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ptr1 = 0x310000u32;
        let ptr2 = 0x310100u32;
        bus.write_bytes(ptr1, b"abc");
        bus.write_bytes(ptr2, b"Z");

        let data_ptr = bus.alloc(2);
        bus.write_bytes(data_ptr, b"ab");
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);

        bus.write_long(sp, 1); // len2
        bus.write_long(sp + 4, ptr2);
        bus.write_long(sp + 8, 3); // len1
        bus.write_long(sp + 12, ptr1);
        bus.write_long(sp + 16, 0); // offset
        bus.write_long(sp + 20, handle);
        bus.write_long(sp + 24, 0);

        let result = disp.dispatch_toolbox(true, 0x1E0, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 24) as i32, -1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 24);

        let final_ptr = bus.read_long(handle);
        let final_size = bus.get_alloc_size(final_ptr).unwrap();
        assert_eq!(bus.read_bytes(final_ptr, final_size as usize), b"ab");
    }

    // PBOpenRF ($A00A) — OS trap, file not found
    #[test]
    fn test_pb_open_rf_not_found() {
        let (mut disp, mut cpu, mut bus) = setup();
        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);

        // Set up Pascal string filename at 0x310000
        let name_ptr = 0x310000u32;
        bus.write_pstring(name_ptr, b"MissingFile.rsrc");

        // Write name_ptr into param block at pb+18
        bus.write_long(pb + 18, name_ptr);
        // Clear ioResult at pb+16
        bus.write_word(pb + 16, 0);

        let result = disp.dispatch_toolbox(false, 0x0A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // ioResult at pb+16 = -43 (fnfErr)
        assert_eq!(bus.read_word(pb + 16), (-43i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-43i32) as u32);
    }

    #[test]
    fn test_pb_open_rf_current_permission_allows_fswrite() {
        let (mut disp, mut cpu, mut bus) = setup();
        let filename = "Installer Output";
        disp.vfs.insert(filename.to_string(), Vec::new());
        disp.vfs_rsrc.insert(filename.to_string(), Vec::new());

        let pb = 0x300000u32;
        let name_ptr = 0x310000u32;
        bus.write_pstring(name_ptr, filename.as_bytes());
        bus.write_long(pb + 18, name_ptr);
        bus.write_byte(pb + 27, 0); // fsCurPerm
        cpu.write_reg(Register::A0, pb);

        disp.dispatch_toolbox(false, 0x0A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let refnum = bus.read_word(pb + 24);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert!(disp.write_refnums.contains(&refnum));

        let write_buf = 0x310100u32;
        bus.write_bytes(write_buf, &[0xCA, 0xFE, 0xBA, 0xBE]);
        bus.write_word(pb + 24, refnum);
        bus.write_long(pb + 32, write_buf);
        bus.write_long(pb + 36, 4);
        bus.write_word(pb + 44, 1); // fsFromStart
        bus.write_long(pb + 46, 0);
        cpu.write_reg(Register::A0, pb);

        disp.dispatch_resource(false, 0x03, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(
            disp.vfs_rsrc.get(filename),
            Some(&vec![0xCA, 0xFE, 0xBA, 0xBE])
        );
    }

    // HCreateResFile ($A81B) — missing file creation plus PBOpenRF visibility.
    #[test]
    fn test_hcreate_res_file_creates_missing_file_and_resource_fork() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let name_ptr = 0x310000u32;
        bus.write_pstring(name_ptr, b"Prefs.RSRC");

        bus.write_long(sp, name_ptr);
        bus.write_long(sp + 4, 0); // dirID
        bus.write_word(sp + 8, 0); // vRefNum
        cpu.write_reg(Register::A7, sp);

        let result = disp.dispatch_toolbox(true, 0x01B, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(0x0A60), 0);
        assert!(disp.vfs.contains_key("Prefs.RSRC"));
        assert!(disp.vfs_rsrc.contains_key("Prefs.RSRC"));

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_ptr);
        bus.write_word(pb + 22, 0);
        bus.write_byte(pb + 27, 3); // fsRdWrPerm
        bus.write_long(pb + 48, 0);

        let result = disp.dispatch_toolbox(false, 0x0A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(pb + 16), 0);
        assert!(bus.read_word(pb + 24) > 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn test_pbh_open_rf_async_prefers_io_dir_id_resource_fork() {
        let (mut disp, mut cpu, mut bus) = setup();
        let target_dir_id = disp.ensure_vfs_directory("Game/Target Data");
        disp.ensure_vfs_directory("Game/Other Data");
        disp.vfs_rsrc
            .insert("Game/Other Data/Shared".to_string(), vec![0x11, 0x22]);
        disp.vfs_rsrc
            .insert("Game/Target Data/Shared".to_string(), vec![0xAA, 0xBB]);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        let name_ptr = 0x310000u32;
        bus.write_pstring(name_ptr, b"Shared");
        bus.write_long(pb + 18, name_ptr);
        bus.write_word(pb + 22, TrapDispatcher::boot_volume_ref_num_u16());
        bus.write_byte(pb + 27, 1); // fsRdPerm
        bus.write_long(pb + 48, target_dir_id);
        disp.current_trap_word = 0xA60A; // PBHOpenRFAsync

        let result = disp.dispatch_toolbox(false, 0x0A, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let refnum = bus.read_word(pb + 24);
        assert_eq!(bus.read_word(pb + 16), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            disp.open_files.get(&refnum),
            Some(&"__rsrc__Game/Target Data/Shared".to_string())
        );
        assert_eq!(
            disp.vfs.get("__rsrc__Game/Target Data/Shared").unwrap(),
            &vec![0xAA, 0xBB]
        );
    }

    #[test]
    fn test_hcreate_res_file_existing_resource_fork_returns_dupfnerr_and_preserves_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.vfs.insert("Prefs.RSRC".to_string(), vec![0x11, 0x22]);
        disp.vfs_rsrc
            .insert("Prefs.RSRC".to_string(), vec![0x33, 0x44]);

        let sp = TEST_SP;
        let name_ptr = 0x310100u32;
        bus.write_pstring(name_ptr, b"Prefs.RSRC");
        bus.write_long(sp, name_ptr);
        bus.write_long(sp + 4, 0);
        bus.write_word(sp + 8, 0);
        cpu.write_reg(Register::A7, sp);

        let result = disp.dispatch_toolbox(true, 0x01B, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(0x0A60) as i16, -48);
        assert_eq!(disp.vfs.get("Prefs.RSRC").unwrap(), &vec![0x11, 0x22]);
        assert_eq!(disp.vfs_rsrc.get("Prefs.RSRC").unwrap(), &vec![0x33, 0x44]);
    }

    // PBOpenRF on a freshly-PBCreate'd rsrc fork that's already had
    // FSWrite bytes appended must NOT clobber the in-memory rsrc bytes
    // (vfs[__rsrc__name]) with the snapshot from vfs_rsrc.
    #[test]
    fn test_pb_open_rf_preserves_in_progress_writes() {
        let (mut disp, mut cpu, mut bus) = setup();

        let name = "InstallerTemp";
        let rsrc_key = "__rsrc__InstallerTemp";
        // Simulate a previous open that wrote 4 bytes through the
        // open-files path. vfs has the new bytes; vfs_rsrc still has
        // the original empty snapshot.
        disp.vfs_rsrc.insert(name.to_string(), Vec::new());
        disp.vfs
            .insert(rsrc_key.to_string(), vec![0xCA, 0xFE, 0xBA, 0xBE]);

        let pb = 0x300000u32;
        cpu.write_reg(Register::A0, pb);
        let name_ptr = 0x310000u32;
        bus.write_pstring(name_ptr, name.as_bytes());
        bus.write_long(pb + 18, name_ptr);

        disp.dispatch_toolbox(false, 0x0A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            disp.vfs.get(rsrc_key).unwrap(),
            &vec![0xCA, 0xFE, 0xBA, 0xBE],
            "Re-opening must not clobber the in-progress rsrc-fork \
             writes with the on-disk snapshot"
        );
    }

    #[test]
    fn pack0_generated_routes_preserve_exact_stack_word_values() {
        assert_eq!(super::PACK0_OPERATION_ROUTES.len(), 26);
        assert!(super::PACK0_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0000, "LActivate"),
            (0x0004, "LAddColumn"),
            (0x0008, "LAddRow"),
            (0x000C, "LAddToCell"),
            (0x0010, "LAutoScroll"),
            (0x0014, "LCellSize"),
            (0x0018, "LClick"),
            (0x001C, "LClrCell"),
            (0x0020, "LDelColumn"),
            (0x0024, "LDelRow"),
            (0x0028, "LDispose"),
            (0x002C, "LSetDrawingMode"),
            (0x0030, "LDraw"),
            (0x0034, "LGetCellDataLocation"),
            (0x0038, "LGetCell"),
            (0x003C, "LGetSelect"),
            (0x0040, "LLastClick"),
            (0x0044, "LNew"),
            (0x0048, "LNextCell"),
            (0x004C, "LRect"),
            (0x0050, "LScroll"),
            (0x0054, "LSearch"),
            (0x0058, "LSetCell"),
            (0x005C, "LSetSelect"),
            (0x0060, "LSize"),
            (0x0064, "LUpdate"),
        ] {
            let route = super::pack0_operation_route(0xA9E7, selector).expect("Pack0 route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA9E8, 0x0000),
            (0xA8E7, 0x0044),
            (0xA9E7, 0x0002),
            (0xA9E7, 0x3F3C),
            (0xA9E7, 0x4400),
        ] {
            assert!(super::pack0_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack0_records_stack_carrier_identity_without_changing_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA9E7;
        bus.write_word(sp, 0x0000);
        bus.write_long(sp + 2, 0);
        bus.write_word(sp + 6, 0);

        let result = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(result.expect("Pack0 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack0:0x0000:stack-word-zero:16")
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0010);
        bus.write_long(sp + 2, 0);
        let result = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(result.expect("Pack0 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack0:0x0010:stack-word-immediate:16")
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);

        disp.current_trap_word = 0xA9E8;
        cpu.write_reg(Register::A7, sp);
        let result = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(result.expect("Pack0 arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    // Pack0 / List Manager ($A9E7) — LNew selector $0044
    // IM:IV 1986 p. IV-269: LNew returns ListHandle; selFlags=0 and active=TRUE.
    #[test]
    fn pack0_lnew_returns_non_nil_listhandle_with_default_selection_and_active_flags() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x350000u32;
        let data_bounds_ptr = 0x350100u32;
        let window_ptr = 0x210000u32;

        // view = (0,0)-(40,80), dataBounds = rows 0..2, cols 0..1
        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 40);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 2);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew selector
        bus.write_word(sp + 2, 0); // scrollVert = FALSE
        bus.write_word(sp + 4, 0); // scrollHoriz = FALSE
        bus.write_word(sp + 6, 0); // hasGrow = FALSE
        bus.write_word(sp + 8, 0x0100); // drawIt = TRUE
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 0); // default LDEF
        bus.write_word(sp + 16, 0); // cSize.v => default
        bus.write_word(sp + 18, 0); // cSize.h => default
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0); // result slot

        let result = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);

        let list_handle = bus.read_long(sp + 28);
        assert_ne!(list_handle, 0);
        let list_ptr = bus.read_long(list_handle);
        assert_ne!(list_ptr, 0);
        assert_eq!(
            bus.read_byte(list_ptr + 36),
            0,
            "selFlags should default to 0"
        );
        assert_eq!(
            bus.read_byte(list_ptr + 37),
            1,
            "lActive should default to TRUE"
        );
    }

    #[test]
    fn pack0_lactivate_updates_lactive_and_scrollbars() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x350000u32;
        let data_bounds_ptr = 0x350100u32;
        let window_ptr = 0x210000u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 40);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 2);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0x0100); // scrollVert = TRUE
        bus.write_word(sp + 4, 0); // scrollHoriz = FALSE
        bus.write_word(sp + 6, 0); // hasGrow = FALSE
        bus.write_word(sp + 8, 0); // drawIt = FALSE
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 0); // default LDEF
        bus.write_word(sp + 16, 0);
        bus.write_word(sp + 18, 0);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);

        let result = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);
        let list_ptr = bus.read_long(list_handle);
        let v_scroll_handle = bus.read_long(list_ptr + TrapDispatcher::LIST_VSCROLL_OFFSET);
        assert_ne!(v_scroll_handle, 0);
        let v_scroll_ptr = bus.read_long(v_scroll_handle);
        assert_eq!(bus.read_byte(list_ptr + 37), 1, "initial lActive must be 1");
        assert_eq!(
            bus.read_byte(v_scroll_ptr + 17),
            0,
            "initial contrlHilite must be 0"
        );

        // Drawing off retains scrollbar visibility on Mac OS 8.1.
        assert_eq!(bus.read_byte(v_scroll_ptr + 16), 0);
        for draw in [true, false] {
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x002C);
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, if draw { 0x0100 } else { 0 });
            assert!(disp
                .dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus)
                .unwrap()
                .is_ok());
            assert_eq!(bus.read_byte(v_scroll_ptr + 16), 255);
            assert_eq!(disp.list_states.get_record(list_handle).unwrap().draw_enabled, draw);
        }

        // Call LActivate(FALSE, list)
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0000); // LActivate selector
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0x0000); // act = FALSE
        let result = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(
            bus.read_byte(list_ptr + 37),
            0,
            "lActive must be 0 when deactivated"
        );
        assert_eq!(
            bus.read_byte(v_scroll_ptr + 17),
            255,
            "scrollbar contrlHilite must be 255 when deactivated"
        );

        // Call LActivate(TRUE, list)
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0000); // LActivate selector
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0x0100); // act = TRUE
        let result = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(
            bus.read_byte(list_ptr + 37),
            1,
            "lActive must be 1 when reactivated"
        );
        assert_eq!(
            bus.read_byte(v_scroll_ptr + 17),
            0,
            "scrollbar contrlHilite must be 0 when reactivated"
        );
    }

    #[test]
    fn lnew_pascal_boolean_order_creates_the_requested_vertical_scrollbar() {
        for trap in [0x1E7, 0x1E8] {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            let view_rect_ptr = 0x350000u32;
            let data_bounds_ptr = 0x350100u32;
            let window_ptr = 0x210000u32;
            bus.write_word(view_rect_ptr, 0);
            bus.write_word(view_rect_ptr + 2, 0);
            bus.write_word(view_rect_ptr + 4, 40);
            bus.write_word(view_rect_ptr + 6, 80);
            bus.write_word(data_bounds_ptr, 0);
            bus.write_word(data_bounds_ptr + 2, 0);
            bus.write_word(data_bounds_ptr + 4, 10);
            bus.write_word(data_bounds_ptr + 6, 1);

            bus.write_word(sp, 0x0044);
            bus.write_word(sp + 2, 0x0100); // scrollVert = TRUE
            bus.write_word(sp + 4, 0); // scrollHoriz = FALSE
            bus.write_word(sp + 6, 0); // hasGrow = FALSE
            bus.write_word(sp + 8, 0); // drawIt = FALSE
            bus.write_long(sp + 10, window_ptr);
            bus.write_word(sp + 14, 0);
            bus.write_word(sp + 16, 10);
            bus.write_word(sp + 18, 80);
            bus.write_long(sp + 20, data_bounds_ptr);
            bus.write_long(sp + 24, view_rect_ptr);
            bus.write_long(sp + 28, 0);

            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            let list_handle = bus.read_long(sp + 28);
            let list_ptr = bus.read_long(list_handle);
            let v_scroll = bus.read_long(list_ptr + 28);
            assert_ne!(v_scroll, 0);
            assert_eq!(bus.read_long(list_ptr + 32), 0);
            let control = bus.read_long(v_scroll);
            assert_eq!(bus.read_long(control + 4), window_ptr);
            assert_eq!(bus.read_word(control + 8) as i16, -1);
            assert_eq!(bus.read_word(control + 10) as i16, 80);
            assert_eq!(bus.read_word(control + 12) as i16, 41);
            assert_eq!(bus.read_word(control + 14) as i16, 96);
            assert_eq!(bus.read_byte(control + 16), 0);
            assert_eq!(bus.read_word(control + 18), 0);
            assert_eq!(bus.read_word(control + 20), 0);
            assert_eq!(bus.read_word(control + 22), 6);
            assert_eq!(bus.read_long(window_ptr + 140), v_scroll);
            assert!(!disp.list_states.get_record(list_handle).unwrap().draw_enabled);

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0050); // LScroll
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, 3); // dRows
            bus.write_word(sp + 8, 0); // dCols
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(list_ptr + 20), 3);
            assert_eq!(bus.read_word(control + 18), 3);

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0060); // LSize
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, 20); // listHeight
            bus.write_word(sp + 8, 80); // listWidth
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(control + 12), 21);
            assert_eq!(bus.read_word(control + 18), 3);
            assert_eq!(bus.read_word(control + 22), 8);

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0028);
            bus.write_long(sp + 2, list_handle);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_long(window_ptr + 140), 0);
        }
    }

    #[test]
    fn lupdate_redraws_the_list_scrollbar_beside_rview() {
        for trap in [0x1E7, 0x1E8] {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            let screen_base = 0x300000u32;
            let row_bytes = 256u32;
            let window_ptr = 0x210000u32;
            let view_rect_ptr = 0x350000u32;
            let data_bounds_ptr = 0x350100u32;

            disp.set_screen_mode_for_test(screen_base, row_bytes, 256, 192, 8);
            bus.write_long(0x0824, screen_base);
            bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
            disp.init_cgraf_window(
                &mut bus,
                &mut cpu,
                window_ptr,
                screen_base,
                20,
                30,
                120,
                190,
                "",
                0,
                true,
                false,
                false,
                0,
            );

            bus.write_word(view_rect_ptr, 10);
            bus.write_word(view_rect_ptr + 2, 10);
            bus.write_word(view_rect_ptr + 4, 50);
            bus.write_word(view_rect_ptr + 6, 90);
            bus.write_word(data_bounds_ptr, 0);
            bus.write_word(data_bounds_ptr + 2, 0);
            bus.write_word(data_bounds_ptr + 4, 8);
            bus.write_word(data_bounds_ptr + 6, 1);

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0044); // LNew
            bus.write_word(sp + 2, 0x0100); // scrollVert = TRUE
            bus.write_word(sp + 4, 0); // scrollHoriz = FALSE
            bus.write_word(sp + 6, 0); // hasGrow = FALSE
            bus.write_word(sp + 8, 0); // drawIt = FALSE
            bus.write_long(sp + 10, window_ptr);
            bus.write_word(sp + 14, 0);
            bus.write_word(sp + 16, 10);
            bus.write_word(sp + 18, 80);
            bus.write_long(sp + 20, data_bounds_ptr);
            bus.write_long(sp + 24, view_rect_ptr);
            bus.write_long(sp + 28, 0);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            let list_handle = bus.read_long(sp + 28);

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x002C); // LDoDraw
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, 0x0100);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            // rView is local to a window at (20,30), so its vertical scroll
            // bar occupies screen coordinates (29,120)-(71,136). Simulate an
            // update pass erasing that strip before List Manager redraws it.
            for y in 29u32..71 {
                for x in 120u32..136 {
                    bus.write_byte(screen_base + y * row_bytes + x, 0x7F);
                }
            }

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0064); // LUpdate
            bus.write_long(sp + 2, list_handle);
            bus.write_long(sp + 6, 0); // update region
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            let changed = (29u32..71)
                .flat_map(|y| (120u32..136).map(move |x| (y, x)))
                .filter(|&(y, x)| bus.read_byte(screen_base + y * row_bytes + x) != 0x7F)
                .count();
            assert!(
                changed > 40,
                "LUpdate should redraw the attached scrollbar in the window-local strip"
            );
        }
    }

    // Pack0 / List Manager ($A9E7) — LAddRow selector $0008
    // IM:IV 1986 p. IV-271: returns first added row and increases dataBounds.bottom.
    #[test]
    fn pack0_laddrow_returns_insert_row_and_extends_databounds_bottom() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x351000u32;
        let data_bounds_ptr = 0x351100u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 40);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 2);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, 0x210000);
        bus.write_word(sp + 14, 0);
        bus.write_word(sp + 16, 10);
        bus.write_word(sp + 18, 40);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);

        cpu.write_reg(Register::A7, sp);
        // Pascal calling convention: lHandle (last arg) is closest to the
        // selector, count (first arg) is at the deepest slot.
        bus.write_word(sp, 0x0008); // LAddRow
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 1); // rowNum
        bus.write_word(sp + 8, 1); // count
        bus.write_word(sp + 10, 0xBEEF); // INTEGER result slot
        let add = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(add.is_some());
        assert!(add.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), 1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);

        let list_ptr = bus.read_long(list_handle);
        let data_bounds_bottom = bus.read_word(list_ptr + 76) as i16;
        assert_eq!(data_bounds_bottom, 3);
    }

    // Pack0 / List Manager ($A9E7) — LAddToCell selector $000C
    // IM:IV 1986 p. IV-272: append bytes into an existing cell; invalid cells are ignored.
    #[test]
    fn pack0_laddtocell_appends_data_and_ignores_invalid_cells() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x352000u32;
        let data_bounds_ptr = 0x352100u32;
        let seed_ptr = 0x352200u32;
        let append_ptr = 0x352210u32;
        let valid_out_ptr = 0x352220u32;
        let valid_len_ptr = 0x352230u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 48);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 1);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, 0x210000);
        bus.write_word(sp + 14, 0);
        bus.write_word(sp + 16, 12);
        bus.write_word(sp + 18, 24);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);
        assert_ne!(list_handle, 0);

        bus.write_bytes(seed_ptr, b"A");
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0058); // LSetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 1);
        bus.write_long(sp + 12, seed_ptr);
        let set_seed = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(set_seed.is_some());
        assert!(set_seed.unwrap().is_ok());

        bus.write_bytes(append_ptr, b"BC");
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x000C); // LAddToCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 2);
        bus.write_long(sp + 12, append_ptr);
        let add = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(add.is_some());
        assert!(add.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);

        bus.write_word(valid_len_ptr, 8);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0038); // LGetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, valid_len_ptr);
        bus.write_long(sp + 14, valid_out_ptr);
        let get_valid = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(get_valid.is_some());
        assert!(get_valid.unwrap().is_ok());
        assert_eq!(bus.read_word(valid_len_ptr), 3);
        assert_eq!(bus.read_byte(valid_out_ptr), b'A');
        assert_eq!(bus.read_byte(valid_out_ptr + 1), b'B');
        assert_eq!(bus.read_byte(valid_out_ptr + 2), b'C');

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x000C); // LAddToCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 1); // invalid row
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 2);
        bus.write_long(sp + 12, append_ptr);
        let add_invalid = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(add_invalid.is_some());
        assert!(add_invalid.unwrap().is_ok());

        bus.write_word(valid_len_ptr, 8);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0038); // LGetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, valid_len_ptr);
        bus.write_long(sp + 14, valid_out_ptr);
        let get_invalid = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(get_invalid.is_some());
        assert!(get_invalid.unwrap().is_ok());
        assert_eq!(bus.read_word(valid_len_ptr), 3);
        assert_eq!(bus.read_byte(valid_out_ptr), b'A');
        assert_eq!(bus.read_byte(valid_out_ptr + 1), b'B');
        assert_eq!(bus.read_byte(valid_out_ptr + 2), b'C');

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0028); // LDispose
        bus.write_long(sp + 2, list_handle);
        let dispose = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(dispose.is_some());
        assert!(dispose.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    // Pack0 / List Manager ($A9E7) — LDelRow selector $0024
    // IM:IV 1986 p. IV-271: rows after rowNum shift by count; dataBounds.bottom decreases.
    #[test]
    fn pack0_ldelrow_deletes_rows_compacts_following_rows_and_reduces_databounds_bottom() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x352000u32;
        let data_bounds_ptr = 0x352100u32;
        let data_a_ptr = 0x352200u32;
        let data_b_ptr = 0x352210u32;
        let out_ptr = 0x352220u32;
        let out_len_ptr = 0x352230u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 40);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 2); // two rows: 0 and 1
        bus.write_word(data_bounds_ptr + 6, 1); // one column

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, 0x210000);
        bus.write_word(sp + 14, 0);
        bus.write_word(sp + 16, 10);
        bus.write_word(sp + 18, 40);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);

        // LSetCell row 0 = "A"
        bus.write_bytes(data_a_ptr, b"A");
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0058); // LSetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0); // row
        bus.write_word(sp + 8, 0); // col
        bus.write_word(sp + 10, 1); // dataLen
        bus.write_long(sp + 12, data_a_ptr);
        let set_a = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(set_a.is_some());
        assert!(set_a.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);

        // LSetCell row 1 = "B"
        bus.write_bytes(data_b_ptr, b"B");
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0058); // LSetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 1); // row
        bus.write_word(sp + 8, 0); // col
        bus.write_word(sp + 10, 1); // dataLen
        bus.write_long(sp + 12, data_b_ptr);
        let set_b = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(set_b.is_some());
        assert!(set_b.unwrap().is_ok());

        // Delete row 0; row 1 should compact into row 0.
        // Pascal order: lHandle deepest under selector, then rowNum, then count.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0024); // LDelRow
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0); // rowNum
        bus.write_word(sp + 8, 1); // count
        let del = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(del.is_some());
        assert!(del.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0038); // LGetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0); // row
        bus.write_word(sp + 8, 0); // col
        bus.write_long(sp + 10, out_len_ptr); // VAR dataLen
        bus.write_long(sp + 14, out_ptr); // dataPtr
        bus.write_word(out_len_ptr, 1); // max bytes to copy
        let get = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(get.is_some());
        assert!(get.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 18);
        assert_eq!(bus.read_word(out_len_ptr), 1);
        assert_eq!(bus.read_byte(out_ptr), b'B');

        let list_ptr = bus.read_long(list_handle);
        let data_bounds_bottom = bus.read_word(list_ptr + 76) as i16;
        assert_eq!(data_bounds_bottom, 1);
    }

    // Pack0 / List Manager ($A9E7) — LSetSelect selector $005C and
    // LGetSelect selector $003C.
    // IM:IV 1986 p. IV-273: selection toggles are driven by
    // LSetSelect(setIt,theCell,lHandle), and LGetSelect checks a
    // specific cell or searches from a probe cell forward.
    #[test]
    fn pack0_lsetselect_lgetselect_use_pascal_argument_order_and_selection_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x353400u32;
        let data_bounds_ptr = 0x353500u32;
        let query_cell_ptr = 0x353600u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 48);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 3);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, 0x210000);
        bus.write_word(sp + 14, 0);
        bus.write_word(sp + 16, 16);
        bus.write_word(sp + 18, 32);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);

        // LSetSelect(TRUE, Cell(1,0), hList) with Pascal order:
        // lHandle closest to selector, then Cell, then Boolean.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x005C); // LSetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 1); // cell.v = row
        bus.write_word(sp + 8, 0); // cell.h = col
        bus.write_word(sp + 10, 0x0100); // setIt = TRUE
        let set = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(set.is_some());
        assert!(set.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        // LGetSelect(FALSE, &Cell(1,0), hList) should report selected.
        bus.write_word(query_cell_ptr, 1);
        bus.write_word(query_cell_ptr + 2, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x003C); // LGetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, query_cell_ptr);
        bus.write_word(sp + 10, 0x0000); // next = FALSE
        bus.write_word(sp + 12, 0xBEEF);
        let get_exact = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(get_exact.is_some());
        assert!(get_exact.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(query_cell_ptr), 1);
        assert_eq!(bus.read_word(query_cell_ptr + 2), 0);

        // LGetSelect(TRUE, &Cell(0,0), hList) should advance the probe
        // to the selected cell at or after the starting point.
        bus.write_word(query_cell_ptr, 0);
        bus.write_word(query_cell_ptr + 2, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x003C); // LGetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, query_cell_ptr);
        bus.write_word(sp + 10, 0x0100); // next = TRUE
        bus.write_word(sp + 12, 0xBEEF);
        let get_next = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(get_next.is_some());
        assert!(get_next.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(query_cell_ptr), 1);
        assert_eq!(bus.read_word(query_cell_ptr + 2), 0);

        // LSetSelect(FALSE, Cell(1,0), hList) should clear the cell.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x005C); // LSetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 1);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 0x00FF); // setIt = FALSE; low byte is padding garbage
        let clear = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(clear.is_some());
        assert!(clear.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        bus.write_word(query_cell_ptr, 1);
        bus.write_word(query_cell_ptr + 2, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x003C); // LGetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, query_cell_ptr);
        bus.write_word(sp + 10, 0x0000); // next = FALSE
        bus.write_word(sp + 12, 0xBEEF);
        let get_cleared = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(get_cleared.is_some());
        assert!(get_cleared.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    // Pack0 / List Manager ($A9E7) — LClick selector $0018
    // IM:IV 1986 p. IV-273: TRUE on double-click in same cell; LLastClick reports clicked cell.
    #[test]
    fn pack0_lclick_same_cell_double_click_returns_true_and_lastclick_tracks_cell() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x353000u32;
        let data_bounds_ptr = 0x353100u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 40);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 2);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, 0x210000);
        bus.write_word(sp + 14, 0);
        bus.write_word(sp + 16, 10); // cSize.v
        bus.write_word(sp + 18, 20); // cSize.h
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0018); // LClick
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0); // modifiers
        bus.write_word(sp + 8, 5); // pt.v in row 0
        bus.write_word(sp + 10, 5); // pt.h in col 0
        bus.write_word(sp + 12, 0xBEEF); // Boolean result
        let first = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0018); // LClick
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 5);
        bus.write_word(sp + 10, 5);
        bus.write_word(sp + 12, 0xBEEF);
        let second = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(second.is_some());
        assert!(second.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0040); // LLastClick
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, 0xFFFF_FFFF); // result cell placeholder
        let last_click = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(last_click.is_some());
        assert!(last_click.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0); // row
        assert_eq!(bus.read_word(sp + 8), 0); // col
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    #[test]
    fn pack0_lclick_blank_view_area_clears_selection_and_lastclick_history() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x355000u32;
        let data_bounds_ptr = 0x355100u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 40);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 2);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, 0x210200);
        bus.write_word(sp + 14, 0);
        bus.write_word(sp + 16, 10); // cSize.v
        bus.write_word(sp + 18, 20); // cSize.h
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0018); // LClick
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0); // modifiers
        bus.write_word(sp + 8, 5); // pt.v in row 0
        bus.write_word(sp + 10, 5); // pt.h in col 0
        bus.write_word(sp + 12, 0xBEEF);
        let first = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0);

        for modifiers in [0x0100, 0x0200] {
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0018); // LClick
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, modifiers);
            bus.write_word(sp + 8, 35); // inside rView, below dataBounds
            bus.write_word(sp + 10, 5);
            bus.write_word(sp + 12, 0xBEEF);
            let modified_miss = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
            assert!(modified_miss.is_some());
            assert!(modified_miss.unwrap().is_ok());
            assert_eq!(bus.read_word(sp + 12), 0);
            assert!(disp
                .list_states
                .get_record(list_handle)
                .unwrap()
                .selected
                .contains(&(0, 0)));
        }

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0018); // LClick
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 35); // inside rView, below dataBounds
        bus.write_word(sp + 10, 5);
        bus.write_word(sp + 12, 0xBEEF);
        let miss = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(miss.is_some());
        assert!(miss.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0);

        let query_cell_ptr = 0x355200u32;
        bus.write_word(query_cell_ptr, 0);
        bus.write_word(query_cell_ptr + 2, 0);
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x003C); // LGetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, query_cell_ptr);
        bus.write_word(sp + 10, 0); // next = FALSE
        bus.write_word(sp + 12, 0xBEEF);
        let get_select = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(get_select.is_some());
        assert!(get_select.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0040); // LLastClick
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, 0xFFFF_FFFF);
        let last_click = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(last_click.is_some());
        assert!(last_click.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 6), 0xFFFF);
        assert_eq!(bus.read_word(sp + 8), 0xFFFF);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0018); // LClick
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 5);
        bus.write_word(sp + 10, 5);
        bus.write_word(sp + 12, 0xBEEF);
        let second = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(second.is_some());
        assert!(second.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0);
    }

    #[test]
    fn pack0_llastclick_before_any_click_returns_negative_cell() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x354000u32;
        let data_bounds_ptr = 0x354100u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 40);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 1);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, 0x210100);
        bus.write_word(sp + 14, 0);
        bus.write_word(sp + 16, 10);
        bus.write_word(sp + 18, 20);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);
        assert_ne!(list_handle, 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0040); // LLastClick
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, 0xBEEF_BEEF); // result cell placeholder
        let last_click = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(last_click.is_some());
        assert!(last_click.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_word(sp + 6), 0xFFFF);
        assert_eq!(bus.read_word(sp + 8), 0xFFFF);
    }

    #[test]
    fn pack0_lupdate_draws_visible_cell_text_and_restores_qd_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let screen_base = 0x300000u32;
        let row_bytes = 128u32;
        let window_ptr = 0x210000u32;
        let view_rect_ptr = 0x356000u32;
        let data_bounds_ptr = 0x356100u32;
        let data_ptr = 0x356200u32;

        disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 96, 8);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        for offset in 0..(row_bytes * 96) {
            bus.write_byte(screen_base + offset, 0);
        }
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_ptr,
            screen_base,
            0,
            0,
            96,
            128,
            "",
            0,
            true,
            true,
            false,
            0,
        );

        for y in 10..22 {
            for x in 10..100 {
                bus.write_byte(screen_base + y * row_bytes + x, 255);
            }
        }

        bus.write_word(view_rect_ptr, 10);
        bus.write_word(view_rect_ptr + 2, 10);
        bus.write_word(view_rect_ptr + 4, 22);
        bus.write_word(view_rect_ptr + 6, 100);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 1);
        bus.write_word(data_bounds_ptr + 6, 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0); // scrollVert = FALSE
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0x0100); // drawIt = TRUE
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 128); // custom LDEF id: fallback renderer still applies
        bus.write_word(sp + 16, 12);
        bus.write_word(sp + 18, 90);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);

        bus.write_bytes(data_ptr, b"Mission");
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0058); // LSetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 7);
        bus.write_long(sp + 12, data_ptr);
        let set_cell = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(set_cell.is_some());
        assert!(set_cell.unwrap().is_ok());

        disp.fg_color = (0x1111, 0x2222, 0x3333);
        disp.bg_color = (0xAAAA, 0xBBBB, 0xCCCC);
        disp.pn_loc = (7, 8);
        disp.tx_mode = 2;
        disp.tx_size = 12;
        disp.sync_current_port_draw_state(&mut bus);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0064); // LUpdate
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, 0); // update region
        let update = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(update.is_some());
        assert!(update.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);

        let mut white_pixels = 0usize;
        for y in 10..22 {
            for x in 10..100 {
                if bus.read_byte(screen_base + y * row_bytes + x) == 0 {
                    white_pixels += 1;
                }
            }
        }
        assert!(
            white_pixels > 0,
            "LUpdate should render opposite-colored text pixels into a dark visible cell"
        );
        assert_eq!(*disp.current_port, window_ptr);
        assert_eq!(disp.fg_color, (0x1111, 0x2222, 0x3333));
        assert_eq!(disp.bg_color, (0xAAAA, 0xBBBB, 0xCCCC));
        assert_eq!(disp.pn_loc, (7, 8));
        assert_eq!(disp.tx_mode, 2);
        assert_eq!(disp.tx_size, 12);
    }

    #[test]
    fn list_dispatchers_draw_and_hit_test_offset_window_cells_in_local_coordinates() {
        for trap in [0x1E7, 0x1E8] {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            let screen_base = 0x300000u32;
            let row_bytes = 192u32;
            let owner_port = 0x210000u32;
            let caller_port = 0x211000u32;
            let view_rect_ptr = 0x356400u32;
            let data_bounds_ptr = 0x356500u32;
            let data_ptr = 0x356600u32;
            let query_cell_ptr = 0x356700u32;
            let hilite = (0x0000, 0x8000, 0x0000);

            disp.set_screen_mode_for_test(screen_base, row_bytes, 192, 128, 8);
            disp.device_clut.replace([[0xFFFF, 0xFFFF, 0xFFFF]; 256]);
            disp.device_clut.set_entry(0, [0x0000, 0x0000, 0x0000]);
            disp.device_clut.set_entry(42, [hilite.0, hilite.1, hilite.2]);
            disp.quickdraw_hilite_colors
                .set_quickdraw_hilite_color(owner_port, hilite);
            bus.write_long(0x0824, screen_base);
            bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
            disp.init_cgraf_window(
                &mut bus,
                &mut cpu,
                owner_port,
                screen_base,
                20,
                30,
                100,
                170,
                "",
                0,
                true,
                false,
                false,
                0,
            );
            disp.init_cgraf_window(
                &mut bus,
                &mut cpu,
                caller_port,
                screen_base,
                2,
                3,
                18,
                23,
                "",
                0,
                true,
                false,
                false,
                0,
            );
            for offset in 0..(row_bytes * 128) {
                bus.write_byte(screen_base + offset, 255);
            }

            super::super::TrapDispatcher::write_rect_words(
                &mut bus,
                view_rect_ptr,
                (10, 10, 48, 100),
            );
            super::super::TrapDispatcher::write_rect_words(&mut bus, data_bounds_ptr, (0, 0, 4, 1));
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0044); // LNew
            bus.write_word(sp + 2, 0);
            bus.write_word(sp + 4, 0);
            bus.write_word(sp + 6, 0);
            bus.write_word(sp + 8, 0); // drawIt = FALSE while populating
            bus.write_long(sp + 10, owner_port);
            bus.write_word(sp + 14, 128); // missing custom LDEF uses standard fallback
            bus.write_word(sp + 16, 12);
            bus.write_word(sp + 18, 90);
            bus.write_long(sp + 20, data_bounds_ptr);
            bus.write_long(sp + 24, view_rect_ptr);
            bus.write_long(sp + 28, 0);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            let list_handle = bus.read_long(sp + 28);

            for (row, text) in [b"Alpha".as_slice(), b"Beta", b"Gamma", b"Delta"]
                .into_iter()
                .enumerate()
            {
                bus.write_bytes(data_ptr, text);
                cpu.write_reg(Register::A7, sp);
                bus.write_word(sp, 0x0058); // LSetCell
                bus.write_long(sp + 2, list_handle);
                bus.write_word(sp + 6, row as u16);
                bus.write_word(sp + 8, 0);
                bus.write_word(sp + 10, text.len() as u16);
                bus.write_long(sp + 12, data_ptr);
                disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                    .unwrap()
                    .unwrap();
            }

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x005C); // LSetSelect row 0
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, 0);
            bus.write_word(sp + 8, 0);
            bus.write_word(sp + 10, 0x0100);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x002C); // LDoDraw(TRUE)
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, 0x0100);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            disp.set_current_port_state(&mut bus, &mut cpu, caller_port, None);
            disp.fg_color = (0x1111, 0x2222, 0x3333);
            disp.bg_color = (0xAAAA, 0xBBBB, 0xCCCC);
            disp.pn_loc = (7, 8);
            disp.tx_mode = 2;
            disp.sync_current_port_draw_state(&mut bus);
            let owner_clip_handle = bus.read_long(owner_port + 28);
            bus.write_long(owner_port + 80, 0);
            bus.write_long(owner_port + 84, 0);
            disp.resolved_port_color_fields.insert(owner_port, 0x03);

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0064); // LUpdate
            bus.write_long(sp + 2, list_handle);
            bus.write_long(sp + 6, 0);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            // The owner starts at global (20,30), so local rView (10,10)
            // begins at screen (30,40). Every row must retain its own text
            // witness instead of collapsing at the view's lower edge.
            for (row, (top, bottom)) in [(30u32, 42u32), (42, 54), (54, 66)].into_iter().enumerate()
            {
                let witness_pixels = (top..bottom)
                    .flat_map(|y| (40u32..130).map(move |x| (y, x)))
                    .filter(|&(y, x)| {
                        let pixel = bus.read_byte(screen_base + y * row_bytes + x);
                        if row == 0 {
                            pixel != 255
                        } else {
                            pixel == 0
                        }
                    })
                    .count();
                assert!(witness_pixels > 0, "trap ${trap:03X} lost row at y={top}");
            }
            assert_eq!(bus.read_byte(screen_base + 31 * row_bytes + 125), 42);
            assert_eq!(*disp.current_port, caller_port);
            assert_eq!(disp.fg_color, (0x1111, 0x2222, 0x3333));
            assert_eq!(disp.bg_color, (0xAAAA, 0xBBBB, 0xCCCC));
            assert_eq!(disp.pn_loc, (7, 8));
            assert_eq!(disp.tx_mode, 2);
            assert_eq!(bus.read_long(owner_port + 28), owner_clip_handle);
            assert_eq!(bus.read_long(owner_port + 80), 0);
            assert_eq!(bus.read_long(owner_port + 84), 0);
            assert_eq!(
                disp.resolved_port_color_fields.get(&owner_port),
                Some(&0x03)
            );
            assert!((68u32..72).all(
                |y| (40u32..130).all(|x| bus.read_byte(screen_base + y * row_bytes + x) == 255)
            ));

            // LClick receives the same owner-local coordinate basis as rView.
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0018);
            bus.write_long(sp + 2, list_handle);
            bus.write_word(sp + 6, 0);
            bus.write_word(sp + 8, 28); // local center of row 1
            bus.write_word(sp + 10, 20);
            bus.write_word(sp + 12, 0xBEEF);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(sp + 12), 0);

            super::super::TrapDispatcher::write_point_words(&mut bus, query_cell_ptr, (1, 0));
            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x003C); // LGetSelect(FALSE, row 1)
            bus.write_long(sp + 2, list_handle);
            bus.write_long(sp + 6, query_cell_ptr);
            bus.write_word(sp + 10, 0);
            bus.write_word(sp + 12, 0xBEEF);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(sp + 12), 0x0100);

            cpu.write_reg(Register::A7, sp);
            bus.write_word(sp, 0x0040); // LLastClick
            bus.write_long(sp + 2, list_handle);
            bus.write_long(sp + 6, 0xFFFF_FFFF);
            disp.dispatch_toolbox(true, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(bus.read_word(sp + 6), 1);
            assert_eq!(bus.read_word(sp + 8), 0);
        }
    }

    #[test]
    fn pack0_lupdate_draws_selected_empty_cell_with_hilite_color() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let screen_base = 0x300000u32;
        let row_bytes = 128u32;
        let window_ptr = 0x210000u32;
        let view_rect_ptr = 0x356000u32;
        let data_bounds_ptr = 0x356100u32;
        let hilite = (0x0000, 0x8000, 0x0000);

        disp.set_screen_mode_for_test(screen_base, row_bytes, 128, 96, 8);
        disp.device_clut.replace([[0xFFFF, 0xFFFF, 0xFFFF]; 256]);
        disp.device_clut.set_entry(0, [0x0000, 0x0000, 0x0000]);
        disp.device_clut.set_entry(42, [hilite.0, hilite.1, hilite.2]);
        disp.quickdraw_hilite_colors
            .set_quickdraw_hilite_color(window_ptr, hilite);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        for offset in 0..(row_bytes * 96) {
            bus.write_byte(screen_base + offset, 255);
        }
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_ptr,
            screen_base,
            0,
            0,
            96,
            128,
            "",
            0,
            true,
            true,
            false,
            0,
        );

        bus.write_word(view_rect_ptr, 10);
        bus.write_word(view_rect_ptr + 2, 10);
        bus.write_word(view_rect_ptr + 4, 22);
        bus.write_word(view_rect_ptr + 6, 100);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 1);
        bus.write_word(data_bounds_ptr + 6, 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0); // scrollVert = FALSE
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0x0100); // drawIt = TRUE
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 128);
        bus.write_word(sp + 16, 12);
        bus.write_word(sp + 18, 90);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x005C); // LSetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 0x0100);
        let select = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(select.is_some());
        assert!(select.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0064); // LUpdate
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, 0);
        let update = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(update.is_some());
        assert!(update.unwrap().is_ok());

        let interior = bus.read_byte(screen_base + 11 * row_bytes + 95);
        assert_eq!(
            interior, 42,
            "selected list cells should paint with HiliteColor, not plain white"
        );
    }

    #[test]
    fn pack0_lupdate_custom_ldef_arms_draw_proc_trampoline() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x357000u32;
        let data_bounds_ptr = 0x357100u32;
        let window_ptr = 0x210000u32;
        let return_pc = 0x1234_5678;

        bus.write_word(view_rect_ptr, 10);
        bus.write_word(view_rect_ptr + 2, 20);
        bus.write_word(view_rect_ptr + 4, 22);
        bus.write_word(view_rect_ptr + 6, 120);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 1);
        bus.write_word(data_bounds_ptr + 6, 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0); // scrollVert = FALSE
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0x0100); // drawIt = TRUE
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 128);
        bus.write_word(sp + 16, 12);
        bus.write_word(sp + 18, 100);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);
        let list_ptr = bus.read_long(list_handle);

        let proc_addr = bus.alloc(2);
        bus.write_word(proc_addr, 0x4E56); // plausible 68k proc entry
        let proc_handle = bus.alloc(4);
        bus.write_long(proc_handle, proc_addr);
        bus.write_long(
            list_ptr + super::super::TrapDispatcher::LIST_DEF_PROC_OFFSET,
            proc_handle,
        );

        let cell_text_ptr = 0x357200u32;
        bus.write_bytes(cell_text_ptr, b"Hi");
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0058); // LSetCell
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 2);
        bus.write_long(sp + 12, cell_text_ptr);
        let set_cell = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(set_cell.is_some());
        assert!(set_cell.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x005C); // LSetSelect
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_word(sp + 10, 0x0100);
        let select = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(select.is_some());
        assert!(select.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_word(sp, 0x0064); // LUpdate
        bus.write_long(sp + 2, list_handle);
        bus.write_long(sp + 6, 0);
        let update = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(update.is_some());
        assert!(update.unwrap().is_ok());

        let tramp = cpu.read_reg(Register::PC);
        assert_eq!(tramp, disp.list_def_trampoline);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(bus.read_long(sp + 6), return_pc);
        assert_eq!(
            bus.read_word(tramp + 6),
            super::super::TrapDispatcher::LIST_LDRAW_MSG as u16
        );
        assert_eq!(bus.read_word(tramp + 10), 0x0100);
        assert_eq!(bus.read_long(tramp + 40), proc_addr);
        assert_eq!(bus.read_long(tramp + 34), list_handle);
        assert_eq!(bus.read_word(tramp + 26), 0);
        assert_eq!(bus.read_word(tramp + 30), 2);
        assert_eq!(bus.read_long(tramp + 20), 0);
        let cells_handle =
            bus.read_long(list_ptr + super::super::TrapDispatcher::LIST_CELLS_OFFSET);
        let cells_ptr = bus.read_long(cells_handle);
        assert_eq!(bus.read_bytes(cells_ptr, 2), b"Hi");
        let rect_ptr = bus.read_long(tramp + 14);
        assert_eq!(bus.read_word(rect_ptr), 10);
        assert_eq!(bus.read_word(rect_ptr + 2), 20);
        assert_eq!(bus.read_word(rect_ptr + 4), 22);
        assert_eq!(bus.read_word(rect_ptr + 6), 120);
        assert_eq!(bus.read_word(tramp + 54), 0x4E75);
    }

    #[test]
    fn pack0_lclick_custom_ldef_arms_hilite_for_selection_delta() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x357300u32;
        let data_bounds_ptr = 0x357400u32;
        let window_ptr = 0x210000u32;
        let return_pc = 0x2233_4455;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 20);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 2);
        bus.write_word(data_bounds_ptr + 6, 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0); // scrollVert = FALSE
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0x0100); // drawIt = TRUE
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 128);
        bus.write_word(sp + 16, 10);
        bus.write_word(sp + 18, 80);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);
        let list_ptr = bus.read_long(list_handle);

        let proc_addr = bus.alloc(2);
        bus.write_word(proc_addr, 0x4E56); // plausible 68k proc entry
        let proc_handle = bus.alloc(4);
        bus.write_long(proc_handle, proc_addr);
        bus.write_long(
            list_ptr + super::super::TrapDispatcher::LIST_DEF_PROC_OFFSET,
            proc_handle,
        );
        bus.write_byte(
            list_ptr + super::super::TrapDispatcher::LIST_SEL_FLAGS_OFFSET,
            0x80,
        );
        disp.list_states
            .with_record_mut(list_handle, |state| {
                state.selected.insert((0, 0));
            })
            .unwrap();

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_word(sp, 0x0018); // LClick
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0); // modifiers
        bus.write_word(sp + 8, 15); // pt.v in row 1
        bus.write_word(sp + 10, 5); // pt.h in col 0
        bus.write_word(sp + 12, 0xBEEF);
        let click = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(click.is_some());
        assert!(click.unwrap().is_ok());

        let first_tramp = disp.list_def_trampoline;
        assert_eq!(cpu.read_reg(Register::PC), first_tramp);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_long(sp + 8), return_pc);
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(
            bus.read_word(first_tramp + 6),
            super::super::TrapDispatcher::LIST_LHILITE_MSG as u16
        );
        assert_eq!(bus.read_word(first_tramp + 10), 0);
        assert_eq!(bus.read_long(first_tramp + 20), 0);
        assert_eq!(bus.read_word(first_tramp + 54), 0x4EF9);

        let second_tramp = bus.read_long(first_tramp + 56);
        assert_ne!(second_tramp, 0);
        assert_eq!(
            bus.read_word(second_tramp + 6),
            super::super::TrapDispatcher::LIST_LHILITE_MSG as u16
        );
        assert_eq!(bus.read_word(second_tramp + 10), 0x0100);
        assert_eq!(bus.read_long(second_tramp + 20), 0x0001_0000);
        assert_eq!(bus.read_word(second_tramp + 54), 0x4E75);
        let expected = [(1, 0)]
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            &disp.list_states.get_record(list_handle).unwrap().selected,
            &expected
        );
    }

    #[test]
    fn pack0_lclick_blank_view_area_arms_deselect_hilite() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x357500u32;
        let data_bounds_ptr = 0x357600u32;
        let window_ptr = 0x210100u32;
        let return_pc = 0x3344_5566;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 20);
        bus.write_word(view_rect_ptr + 6, 80);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 1);
        bus.write_word(data_bounds_ptr + 6, 1);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0); // scrollVert = FALSE
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0x0100); // drawIt = TRUE
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 128);
        bus.write_word(sp + 16, 10);
        bus.write_word(sp + 18, 80);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0);
        let create = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());
        let list_handle = bus.read_long(sp + 28);
        let list_ptr = bus.read_long(list_handle);

        let proc_addr = bus.alloc(2);
        bus.write_word(proc_addr, 0x4E56); // plausible 68k proc entry
        let proc_handle = bus.alloc(4);
        bus.write_long(proc_handle, proc_addr);
        bus.write_long(
            list_ptr + super::super::TrapDispatcher::LIST_DEF_PROC_OFFSET,
            proc_handle,
        );
        bus.write_byte(
            list_ptr + super::super::TrapDispatcher::LIST_SEL_FLAGS_OFFSET,
            0x80,
        );
        disp.list_states
            .with_record_mut(list_handle, |state| {
                state.selected.insert((0, 0));
            })
            .unwrap();

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        bus.write_word(sp, 0x0018); // LClick
        bus.write_long(sp + 2, list_handle);
        bus.write_word(sp + 6, 0); // modifiers
        bus.write_word(sp + 8, 15); // inside rView, below dataBounds
        bus.write_word(sp + 10, 5);
        bus.write_word(sp + 12, 0xBEEF);
        let click = disp.dispatch_toolbox(true, 0x1E7, &mut cpu, &mut bus);
        assert!(click.is_some());
        assert!(click.unwrap().is_ok());

        let trampoline = disp.list_def_trampoline;
        assert_eq!(cpu.read_reg(Register::PC), trampoline);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_long(sp + 8), return_pc);
        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(
            bus.read_word(trampoline + 6),
            super::super::TrapDispatcher::LIST_LHILITE_MSG as u16
        );
        assert_eq!(bus.read_word(trampoline + 10), 0);
        assert_eq!(bus.read_long(trampoline + 20), 0);
        assert_eq!(bus.read_word(trampoline + 54), 0x4E75);
        assert!(disp
            .list_states
            .get_record(list_handle)
            .unwrap()
            .selected
            .is_empty());
    }

    // Pack1 / List Manager ($A9E8) — LNew selector $0044
    // IM:IV 1986 pp. IV-269 to IV-270: LNew returns a live handle and initializes
    // selFlags=0 with lActive=TRUE.
    #[test]
    fn pack1_lnew_returns_non_nil_listhandle_with_default_selection_and_active_flags() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let view_rect_ptr = 0x356000u32;
        let data_bounds_ptr = 0x356100u32;
        let window_ptr = 0x210800u32;

        bus.write_word(view_rect_ptr, 0);
        bus.write_word(view_rect_ptr + 2, 0);
        bus.write_word(view_rect_ptr + 4, 96);
        bus.write_word(view_rect_ptr + 6, 96);
        bus.write_word(data_bounds_ptr, 0);
        bus.write_word(data_bounds_ptr + 2, 0);
        bus.write_word(data_bounds_ptr + 4, 1);
        bus.write_word(data_bounds_ptr + 6, 1);

        bus.write_word(sp, 0x0044); // LNew
        bus.write_word(sp + 2, 0);
        bus.write_word(sp + 4, 0);
        bus.write_word(sp + 6, 0);
        bus.write_word(sp + 8, 0);
        bus.write_long(sp + 10, window_ptr);
        bus.write_word(sp + 14, 0); // default LDEF
        bus.write_word(sp + 16, 16);
        bus.write_word(sp + 18, 16);
        bus.write_long(sp + 20, data_bounds_ptr);
        bus.write_long(sp + 24, view_rect_ptr);
        bus.write_long(sp + 28, 0xDEAD_BEEF);

        let result = disp.dispatch_toolbox(true, 0x1E8, &mut cpu, &mut bus);
        assert!(result.is_some(), "Pack1 should be handled");
        assert!(result.unwrap().is_ok(), "Pack1 should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);
        let list_handle = bus.read_long(sp + 28);
        assert_ne!(list_handle, 0);
        let list_ptr = bus.read_long(list_handle);
        assert_ne!(list_ptr, 0);
        assert_eq!(
            bus.read_byte(list_ptr + 36),
            0,
            "selFlags should default to 0"
        );
        assert_eq!(
            bus.read_byte(list_ptr + 37),
            1,
            "lActive should default to TRUE"
        );

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0028); // LDispose
        bus.write_long(sp + 2, list_handle);
        let dispose = disp.dispatch_toolbox(true, 0x1E8, &mut cpu, &mut bus);
        assert!(dispose.is_some(), "Pack1 should be handled");
        assert!(dispose.unwrap().is_ok(), "Pack1 should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert!(!disp.list_states.contains_handle(list_handle));
    }

    // Pack1 / List Manager ($A9E8) — LSearch selector $0054
    // IM:IV 1986 p. IV-274: nil-list fallback returns FALSE and leaves the probe cell unchanged.
    #[test]
    fn pack1_lsearch_nil_list_returns_false_and_preserves_probe_cell() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let probe_ptr = 0x357000u32;
        let data_ptr = 0x357100u32;

        bus.write_word(probe_ptr, 0x1122);
        bus.write_word(probe_ptr + 2, 0x3344);
        bus.write_bytes(data_ptr, b"Rose");

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0054); // LSearch
        bus.write_long(sp + 2, 0); // nil list handle
        bus.write_long(sp + 6, probe_ptr);
        bus.write_long(sp + 10, 0xDEAD_BEEF); // bogus callback pointer, must not be entered
        bus.write_word(sp + 14, 4);
        bus.write_long(sp + 16, data_ptr);
        bus.write_word(sp + 20, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x1E8, &mut cpu, &mut bus);
        assert!(result.is_some(), "Pack1 should be handled");
        assert!(result.unwrap().is_ok(), "Pack1 should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 20);
        assert_eq!(bus.read_word(sp + 20), 0);
        assert_eq!(bus.read_word(probe_ptr), 0x1122);
        assert_eq!(bus.read_word(probe_ptr + 2), 0x3344);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // PackBits / UnpackBits ($A8CF / $A8D0)
    // Inside Macintosh Volume I (1985), p. I-470.
    #[test]
    fn packbits_compresses_runs_and_advances_var_pointers() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr_ptr = 0x320000u32;
        let dst_ptr_ptr = 0x320004u32;
        let src_data = 0x320100u32;
        let dst_data = 0x320200u32;

        bus.write_bytes(src_data, &[0xAA, 0xAA, 0xAA, 0xAA]);
        bus.write_long(src_ptr_ptr, src_data);
        bus.write_long(dst_ptr_ptr, dst_data);
        bus.write_word(sp, 4); // srcBytes
        bus.write_long(sp + 2, dst_ptr_ptr); // VAR dstPtr
        bus.write_long(sp + 6, src_ptr_ptr); // VAR srcPtr

        let result = disp.dispatch_toolbox(true, 0x0CF, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_long(src_ptr_ptr), src_data + 4);
        assert_eq!(bus.read_long(dst_ptr_ptr), dst_data + 2);
        assert_eq!(bus.read_byte(dst_data), 0xFD); // -(4-1)
        assert_eq!(bus.read_byte(dst_data + 1), 0xAA);
    }

    #[test]
    fn packbits_literal_sequence_emits_literal_packet_and_pops_10_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr_ptr = 0x321000u32;
        let dst_ptr_ptr = 0x321004u32;
        let src_data = 0x321100u32;
        let dst_data = 0x321200u32;

        bus.write_bytes(src_data, &[0x10, 0x20, 0x30]);
        bus.write_long(src_ptr_ptr, src_data);
        bus.write_long(dst_ptr_ptr, dst_data);
        bus.write_word(sp, 3); // srcBytes
        bus.write_long(sp + 2, dst_ptr_ptr); // VAR dstPtr
        bus.write_long(sp + 6, src_ptr_ptr); // VAR srcPtr

        let result = disp.dispatch_toolbox(true, 0x0CF, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_long(src_ptr_ptr), src_data + 3);
        assert_eq!(bus.read_long(dst_ptr_ptr), dst_data + 4);
        assert_eq!(bus.read_byte(dst_data), 0x02); // literal len 3 => flag 2
        assert_eq!(bus.read_byte(dst_data + 1), 0x10);
        assert_eq!(bus.read_byte(dst_data + 2), 0x20);
        assert_eq!(bus.read_byte(dst_data + 3), 0x30);
    }

    #[test]
    fn unpackbits_expands_packbits_stream_and_advances_var_pointers() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr_ptr = 0x322000u32;
        let dst_ptr_ptr = 0x322004u32;
        let src_data = 0x322100u32;
        let dst_data = 0x322200u32;

        // Encodes bytes: [AA, AA, AA, AA, 55]
        // repeat packet (0xFD, 0xAA) + literal packet (0x00, 0x55).
        bus.write_bytes(src_data, &[0xFD, 0xAA, 0x00, 0x55]);
        bus.write_long(src_ptr_ptr, src_data);
        bus.write_long(dst_ptr_ptr, dst_data);
        bus.write_word(sp, 5); // dstBytes
        bus.write_long(sp + 2, dst_ptr_ptr); // VAR dstPtr
        bus.write_long(sp + 6, src_ptr_ptr); // VAR srcPtr

        let result = disp.dispatch_toolbox(true, 0x0D0, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_long(src_ptr_ptr), src_data + 4);
        assert_eq!(bus.read_long(dst_ptr_ptr), dst_data + 5);
        assert_eq!(
            bus.read_bytes(dst_data, 5),
            vec![0xAA, 0xAA, 0xAA, 0xAA, 0x55]
        );
    }

    // System 7+ Dispatch Manager stubs
    // Inside Macintosh: More Macintosh Toolbox (1993),
    // pp. 6-6/6-29/6-98, 5-18/5-71, and 7-12/7-66.

    #[test]
    fn componentdispatch_generated_routes_preserve_exact_moveq_values() {
        assert_eq!(super::COMPONENT_DISPATCH_OPERATION_ROUTES.len(), 26);
        assert!(super::COMPONENT_DISPATCH_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0001, "RegisterComponent"),
            (0x0002, "UnregisterComponent"),
            (0x0003, "CountComponents"),
            (0x0004, "FindNextComponent"),
            (0x0005, "GetComponentInfo"),
            (0x0006, "GetComponentListModSeed"),
            (0x0007, "OpenComponent"),
            (0x0008, "CloseComponent"),
            (0x000A, "GetComponentInstanceError"),
            (0x000B, "SetComponentInstanceError"),
            (0x000C, "GetComponentInstanceStorage"),
            (0x000D, "SetComponentInstanceStorage"),
            (0x000E, "GetComponentInstanceA5"),
            (0x000F, "SetComponentInstanceA5"),
            (0x0010, "GetComponentRefcon"),
            (0x0011, "SetComponentRefcon"),
            (0x0012, "RegisterComponentResource"),
            (0x0013, "CountComponentInstances"),
            (0x0014, "RegisterComponentResourceFile"),
            (0x0015, "OpenComponentResFile"),
            (0x0018, "CloseComponentResFile"),
            (0x001C, "CaptureComponent"),
            (0x001D, "UncaptureComponent"),
            (0x001E, "SetDefaultComponent"),
            (0x0021, "OpenDefaultComponent"),
            (0x0024, "DelegateComponentCall"),
        ] {
            let route = super::component_dispatch_operation_route(0xA82A, selector)
                .expect("ComponentDispatch route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA92A, 0x0003),
            (0xA82A, 0x0000),
            (0xA82A, 0x0009),
            (0xA82A, 0x002C),
            (0xA82A, 0x7003),
            (0xA82A, 0x1234_0003),
            (0xA82A, 0x0000_FFFF),
            (0xA82A, 0xFFFF_FFFF),
        ] {
            assert!(super::component_dispatch_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn componentdispatch_records_exact_internal_identity_without_changing_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA82A;
        cpu.write_reg(Register::D0, 0x0003);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0xDEAD_BEEF);

        let result = disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus);
        assert!(result.expect("ComponentDispatch arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_ComponentDispatch:0x0003:d0-moveq-immediate:8")
        );
        assert_eq!(bus.read_long(sp + 4), 2);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        disp.current_trap_word = 0xA92A;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0003);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus);
        assert!(result.expect("ComponentDispatch arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(bus.read_long(sp + 4), 2);
    }

    #[test]
    fn componentdispatch_call_path_pops_selector_instance_and_args_and_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0); // component call path

        // Inline component-call glue pushes [paramSize:callNum] at SP.
        bus.write_long(sp, 0x0004_0000);
        bus.write_long(sp + 4, 0xA5A5_5A5A); // sentinel argument data
        bus.write_long(sp + 8, 0x00C0_FFEE); // ComponentInstance
        bus.write_long(sp + 12, 0xDEAD_BEEF); // ComponentResult poison

        let result = disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus);
        assert!(result.is_some(), "ComponentDispatch should be handled");
        assert!(result.unwrap().is_ok(), "ComponentDispatch should return");
        assert_eq!(cpu.read_reg(Register::D0), 0, "stub should return noErr");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 12,
            "component call path should consume selector + instance + args"
        );
        assert_eq!(bus.read_long(sp), 0x0004_0000);
        assert_eq!(bus.read_long(sp + 4), 0xA5A5_5A5A);
        assert_eq!(bus.read_long(sp + 8), 0x00C0_FFEE);
        assert_eq!(bus.read_long(sp + 12), 0);
    }

    #[test]
    fn componentdispatch_opens_and_closes_movie_controller_instance() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let description = 0x342800u32;
        bus.write_long(description, u32::from_be_bytes(*b"play"));

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 4); // FindNextComponent
        bus.write_long(sp, description);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, 0xDEAD_BEEF);
        disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let component = bus.read_long(sp + 8);
        assert_ne!(component, 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 7); // OpenComponent
        bus.write_long(sp, component);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let instance = bus.read_long(sp + 4);
        assert_ne!(instance, 0);
        assert!(disp.synthetic_component_instances.contains(&instance));
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 8); // CloseComponent
        bus.write_long(sp, instance);
        bus.write_word(sp + 4, 0xBEEF);
        disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_word(sp + 4), 0);
        assert!(!disp.synthetic_component_instances.contains(&instance));
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn componentdispatch_internal_request_preserves_non_d0_registers_and_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0xFFFF_1234); // manager-internal call path
        cpu.write_reg(Register::D1, 0x1111_2222);
        cpu.write_reg(Register::A0, 0x3333_4444);
        cpu.write_reg(Register::A1, 0x5555_6666);
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_toolbox(true, 0x02A, &mut cpu, &mut bus);
        assert!(result.is_some(), "ComponentDispatch should be handled");
        assert!(result.unwrap().is_ok(), "ComponentDispatch should return");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::D1), 0x1111_2222);
        assert_eq!(cpu.read_reg(Register::A0), 0x3333_4444);
        assert_eq!(cpu.read_reg(Register::A1), 0x5555_6666);
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    #[test]
    fn textservicesdispatch_returns_noerr_and_preserves_stack_pointer_in_noop_path() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x1234_5678);
        cpu.write_reg(Register::D1, 0x1111_2222);
        cpu.write_reg(Register::A0, 0x3333_4444);
        cpu.write_reg(Register::A1, 0x5555_6666);
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_toolbox(true, 0x254, &mut cpu, &mut bus);
        assert!(result.is_some(), "TextServicesDispatch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "TextServicesDispatch should succeed"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "TextServicesDispatch should return noErr"
        );
        assert_eq!(cpu.read_reg(Register::D1), 0x1111_2222);
        assert_eq!(cpu.read_reg(Register::A0), 0x3333_4444);
        assert_eq!(cpu.read_reg(Register::A1), 0x5555_6666);
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "TextServicesDispatch should preserve the caller stack pointer"
        );
    }

    #[test]
    fn puticon_preserves_a7() {
        // PutIcon is an undocumented internal trap. The conservative HLE stub is
        // a no-op, so the proof checks that it preserves A7 and returns cleanly.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = TEST_SP;
        cpu.write_reg(Register::A7, sp_before);
        bus.write_long(sp_before, 0x1122_3344);
        bus.write_word(sp_before + 4, 0x5566);

        let result = disp.dispatch_toolbox(true, 0x1CA, &mut cpu, &mut bus);
        assert!(result.is_some(), "PutIcon should be handled");
        assert!(result.unwrap().is_ok(), "PutIcon should return");
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "PutIcon should preserve A7"
        );
        assert_eq!(
            bus.read_long(sp_before),
            0x1122_3344,
            "PutIcon should not modify the caller's stack word"
        );
        assert_eq!(
            bus.read_word(sp_before + 4),
            0x5566,
            "PutIcon should not modify the trailing stack halfword"
        );
    }

    #[test]
    fn pack13_generated_routes_preserve_exact_d0_low_word_values() {
        assert_eq!(super::PACK13_OPERATION_ROUTES.len(), 22);
        assert!(super::PACK13_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0100, "InitDBPack"),
            (0x020E, "DBKill"),
            (0x0210, "DBDisposeQuery"),
            (0x0215, "DBRemoveResultHandler"),
            (0x030F, "DBGetNewQuery"),
            (0x0403, "DBEnd"),
            (0x0408, "DBExec"),
            (0x0409, "DBState"),
            (0x040D, "DBUnGetItem"),
            (0x0413, "DBResultsToText"),
            (0x050B, "DBBreak"),
            (0x0514, "DBInstallResultHandler"),
            (0x0516, "DBGetResultHandler"),
            (0x0605, "DBGetSessionNum"),
            (0x0706, "DBSend"),
            (0x0811, "DBStartQuery"),
            (0x0A12, "DBGetQueryResults"),
            (0x0B07, "DBSendItem"),
            (0x0E02, "DBInit"),
            (0x0E0A, "DBGetErr"),
            (0x100C, "DBGetItem"),
            (0x1704, "DBGetConnInfo"),
        ] {
            let route =
                super::pack13_operation_route(0xA82F, selector).expect("Pack13 operation route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA830, 0x0100),
            (0xA72F, 0x1704),
            (0xA82F, 0x00FF),
            (0xA82F, 0x303C),
            (0xA82F, 0x0001),
            (0xA82F, 0x000E),
        ] {
            assert!(super::pack13_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack13_all_22_routes_pop_exact_param_bytes_and_return_expected_oserr() {
        for route in super::PACK13_OPERATION_ROUTES {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            let selector = route.selector as u16;
            let expected_err: i16 = if selector == 0x0100 { -812 } else { -813 };
            let param_bytes = (((selector >> 8) & 0xFF) as u32) * 2;
            let result_sp = sp + param_bytes;

            // Fill argument region with deterministic pattern
            let original_args: Vec<u8> = (0..param_bytes)
                .map(|i| (selector as u8).wrapping_add(i as u8).wrapping_add(0x11))
                .collect();
            for (offset, &byte) in original_args.iter().enumerate() {
                bus.write_byte(sp + offset as u32, byte);
            }

            // Poison the result slot and guard regions
            bus.write_word(result_sp, 0x55AA);
            bus.write_long(sp - 8, 0x1234_5678);
            bus.write_long(result_sp + 2, 0x8765_4321);

            disp.current_trap_word = 0xA82F;
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D0, 0xDEAD_0000 | u32::from(selector)); // stale high word

            let result = disp.dispatch_toolbox(true, 0x02F, &mut cpu, &mut bus);
            assert!(
                result.is_some(),
                "Pack13 should handle selector {selector:#06X}"
            );
            assert!(
                result.unwrap().is_ok(),
                "Pack13 selector {selector:#06X} should return Ok"
            );

            assert_eq!(
                disp.current_selector_operation,
                Some(route.operation_id),
                "operation ID for {selector:#06X}"
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                result_sp,
                "A7 should advance exactly by param_bytes ({param_bytes}) for {selector:#06X}"
            );
            assert_eq!(
                bus.read_word(result_sp),
                expected_err as u16,
                "result slot at SP+{param_bytes} for {selector:#06X}"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                expected_err as i32 as u32,
                "sign-extended D0 for {selector:#06X}"
            );

            // Verify arguments and guard memory are preserved
            for (offset, &byte) in original_args.iter().enumerate() {
                assert_eq!(
                    bus.read_byte(sp + offset as u32),
                    byte,
                    "argument byte at offset {offset} must remain invariant for {selector:#06X}"
                );
            }
            assert_eq!(
                bus.read_long(sp - 8),
                0x1234_5678,
                "underflow guard preserved for {selector:#06X}"
            );
            assert_eq!(
                bus.read_long(result_sp + 2),
                0x8765_4321,
                "overflow guard preserved for {selector:#06X}"
            );
        }
    }

    #[test]
    fn pack13_min_and_max_frames_preserve_arguments_and_hidden_version_word() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // 1. Min frame: InitDBPack ($0100, 1 word / 2 bytes)
        // Hidden version word $0004 pushed by Universal Interfaces inline macro
        disp.current_trap_word = 0xA82F;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xCAFE_0100);
        bus.write_word(sp, 0x0004); // DAM version 4 word
        bus.write_word(sp + 2, 0xBEEF); // OSErr result slot poison

        let result = disp.dispatch_toolbox(true, 0x02F, &mut cpu, &mut bus);
        assert!(result.expect("Pack13 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack13:0x0100:d0-low-word-immediate:16")
        );
        assert_eq!(
            bus.read_word(sp),
            0x0004,
            "InitDBPack hidden version word at SP+0 must remain unchanged"
        );
        assert_eq!(
            bus.read_word(sp + 2),
            (-812i16) as u16,
            "InitDBPack result slot at SP+2 must receive rcDBWrongVersion (-812)"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 2,
            "InitDBPack A7 must advance to result slot SP+2"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-812i32) as u32,
            "InitDBPack D0 must return sign-extended rcDBWrongVersion (-812)"
        );

        // 2. Max frame: DBGetConnInfo ($1704, 23 words / 46 bytes)
        let max_sp = TEST_SP;
        cpu.write_reg(Register::A7, max_sp);
        cpu.write_reg(Register::D0, 0xBEEF_1704);
        for i in 0..46 {
            bus.write_byte(max_sp + i, (i as u8) + 1);
        }
        bus.write_word(max_sp + 46, 0xCAFE); // result slot poison

        let result = disp.dispatch_toolbox(true, 0x02F, &mut cpu, &mut bus);
        assert!(result.expect("Pack13 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack13:0x1704:d0-low-word-immediate:16")
        );
        for i in 0..46 {
            assert_eq!(
                bus.read_byte(max_sp + i),
                (i as u8) + 1,
                "DBGetConnInfo arg byte at offset {i} must remain unchanged"
            );
        }
        assert_eq!(
            bus.read_word(max_sp + 46),
            (-813i16) as u16,
            "DBGetConnInfo result slot at SP+46 must receive rcDBPackNotInited (-813)"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            max_sp + 46,
            "DBGetConnInfo A7 must advance to result slot SP+46"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-813i32) as u32,
            "DBGetConnInfo D0 must return sign-extended rcDBPackNotInited (-813)"
        );
    }

    #[test]
    fn pack13_fail_closed_result_preserves_output_buffers() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let query_out = TEST_SP + 0x100;

        disp.current_trap_word = 0xA82F;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xABCD_030F);
        bus.write_long(sp, query_out);
        bus.write_word(sp + 4, 128);
        bus.write_word(sp + 6, 0xBEEF);
        bus.write_long(query_out, 0x1122_3344);

        let result = disp.dispatch_toolbox(true, 0x02F, &mut cpu, &mut bus);
        assert!(result.expect("Pack13 arm").is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(cpu.read_reg(Register::D0), (-813i32) as u32);
        assert_eq!(bus.read_word(sp + 6), (-813i16) as u16);
        assert_eq!(
            bus.read_long(query_out),
            0x1122_3344,
            "DBGetNewQuery must not touch its output buffer on rcDBPackNotInited"
        );
    }

    #[test]
    fn pack13_unknown_selector_and_trap_mismatch_preserve_memory_and_return_paramerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        for &unknown_selector in &[0x00FFu16, 0x303C, 0x0000, 0x0200, 0xFFFF] {
            disp.current_selector_operation = Some("stale-identity");
            disp.current_trap_word = 0xA82F;
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D0, 0xFACE_0000 | u32::from(unknown_selector));
            bus.write_long(sp, 0x1122_3344);
            bus.write_long(sp + 4, 0x5566_7788);

            let result = disp.dispatch_toolbox(true, 0x02F, &mut cpu, &mut bus);
            assert!(result.expect("Pack13 arm").is_ok());
            assert_eq!(
                disp.current_selector_operation, None,
                "unknown selector {unknown_selector:#06X} must clear stale operation identity"
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp,
                "unknown selector {unknown_selector:#06X} must preserve A7"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                (-50i32) as u32,
                "unknown selector {unknown_selector:#06X} must return sign-extended paramErr (-50)"
            );
            assert_eq!(
                bus.read_long(sp),
                0x1122_3344,
                "guest stack memory must remain invariant for unknown selector {unknown_selector:#06X}"
            );
            assert_eq!(
                bus.read_long(sp + 4),
                0x5566_7788,
                "guest stack memory must remain invariant for unknown selector {unknown_selector:#06X}"
            );
        }

        // Trap word mismatch: trap word 0xA830 with DAM selector 0x0100
        disp.current_selector_operation = Some("stale-identity");
        disp.current_trap_word = 0xA830;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0100);
        bus.write_long(sp, 0x1122_3344);

        let result = disp.dispatch_toolbox(true, 0x02F, &mut cpu, &mut bus);
        assert!(result.expect("Pack13 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation, None,
            "trap word mismatch must clear stale operation identity"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "trap word mismatch must preserve A7"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "trap word mismatch must return sign-extended paramErr (-50)"
        );
        assert_eq!(
            bus.read_long(sp),
            0x1122_3344,
            "guest stack memory must remain invariant on trap word mismatch"
        );
    }

    #[test]
    fn pack9_generated_routes_preserve_exact_d0_low_word_values() {
        assert_eq!(super::PACK9_OPERATION_ROUTES.len(), 1);
        assert_eq!(super::PACK9_OPERATION_ROUTES[0].selector, 0x0D00);
        assert_eq!(super::PACK9_OPERATION_ROUTES[0].routine_name, "PPCBrowser");
        assert_eq!(
            super::PACK9_OPERATION_ROUTES[0].operation_id,
            "selector-operation:_Pack9:0x0D00:d0-low-word-immediate:16"
        );

        let route = super::pack9_operation_route(0xA82B, 0x0D00).expect("Pack9 operation route");
        assert_eq!(route.routine_name, "PPCBrowser");
        assert_eq!(
            route.operation_id,
            "selector-operation:_Pack9:0x0D00:d0-low-word-immediate:16"
        );

        for (trap_word, selector) in [
            (0xA82C, 0x0D00),
            (0xA72B, 0x0D00),
            (0xA92B, 0x0D00),
            (0xAA2B, 0x0D00),
            (0xA02B, 0x0D00),
            (0xA82B, 0x0000),
            (0xA82B, 0x000D),
            (0xA82B, 0x303C),
            (0xA82B, 0x0100),
            (0xA82B, 0xFFFF),
        ] {
            assert!(super::pack9_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack9_ppcbrowser_dispatches_with_pascal_abi_and_returns_user_canceled_err() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let location_ptr = 0x0030_1000;
        let port_info_ptr = 0x0030_1100;
        let prompt_ptr = 0x0030_1200;
        let label_ptr = 0x0030_1300;
        let nbp_type_ptr = 0x0030_1400;
        disp.current_trap_word = 0xA82B;

        for offset in 0..0x100 {
            bus.write_byte(location_ptr + offset, 0xA1);
            bus.write_byte(port_info_ptr + offset, 0xB2);
            bus.write_byte(prompt_ptr + offset, 0xC3);
            bus.write_byte(label_ptr + offset, 0xD4);
            bus.write_byte(nbp_type_ptr + offset, 0xE5);
        }

        // Setup stack:
        // Result slot at sp + 26: 2-byte placeholder
        // Arguments from sp to sp + 26: 26 bytes total
        // SP + 0:  theLocNBPType (4 bytes)
        // SP + 4:  portFilter (4 bytes)
        // SP + 8:  thePortInfo (4 bytes)
        // SP + 12: theLocation (4 bytes)
        // SP + 16: defaultSpecified (2 bytes)
        // SP + 18: applListLabel (4 bytes)
        // SP + 22: prompt (4 bytes)
        // SP + 26: result slot (2 bytes)
        // SP + 28: trailing memory canary (4 bytes)
        bus.write_long(sp, nbp_type_ptr);
        bus.write_long(sp + 4, 0x2222_2222);
        bus.write_long(sp + 8, port_info_ptr);
        bus.write_long(sp + 12, location_ptr);
        bus.write_word(sp + 16, 0x0001);
        bus.write_long(sp + 18, label_ptr);
        bus.write_long(sp + 22, prompt_ptr);
        bus.write_word(sp + 26, 0x55AA);
        bus.write_long(sp + 28, 0xDEAD_BEEF);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xCAFE_0D00); // stale high word + selector $0D00

        let result = disp.dispatch_toolbox(true, 0x02B, &mut cpu, &mut bus);
        assert!(result.is_some(), "Pack9 should be handled");
        assert!(result.unwrap().is_ok(), "Pack9 should return");

        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack9:0x0D00:d0-low-word-immediate:16")
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 26,
            "Pack9 PPCBrowser should pop 26 argument bytes, leaving A7 at result slot"
        );
        assert_eq!(
            bus.read_word(sp + 26),
            0xFF80,
            "Pack9 PPCBrowser result slot must receive userCanceledErr (-128 / 0xFF80)"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0xFFFF_FF80,
            "Pack9 PPCBrowser D0 must mirror sign-extended userCanceledErr (-128)"
        );

        // Caller arguments and surrounding memory must remain unmutated (invariance)
        assert_eq!(bus.read_long(sp), nbp_type_ptr);
        assert_eq!(bus.read_long(sp + 4), 0x2222_2222);
        assert_eq!(bus.read_long(sp + 8), port_info_ptr);
        assert_eq!(bus.read_long(sp + 12), location_ptr);
        assert_eq!(bus.read_word(sp + 16), 0x0001);
        assert_eq!(bus.read_long(sp + 18), label_ptr);
        assert_eq!(bus.read_long(sp + 22), prompt_ptr);
        assert_eq!(bus.read_long(sp + 28), 0xDEAD_BEEF);
        assert!(!disp.ppc_initialized);

        for offset in 0..0x100 {
            assert_eq!(bus.read_byte(location_ptr + offset), 0xA1);
            assert_eq!(bus.read_byte(port_info_ptr + offset), 0xB2);
            assert_eq!(bus.read_byte(prompt_ptr + offset), 0xC3);
            assert_eq!(bus.read_byte(label_ptr + offset), 0xD4);
            assert_eq!(bus.read_byte(nbp_type_ptr + offset), 0xE5);
        }

        disp.ppc_initialized = true;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xBEEF_0D00);
        bus.write_word(sp + 26, 0x55AA);
        let result = disp.dispatch_toolbox(true, 0x02B, &mut cpu, &mut bus);
        assert!(result.expect("Pack9 arm").is_ok());
        assert!(disp.ppc_initialized);
        assert_eq!(cpu.read_reg(Register::A7), sp + 26);
        assert_eq!(bus.read_word(sp + 26), 0xFF80);
        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF_FF80);

        for offset in 0..0x100 {
            assert_eq!(bus.read_byte(location_ptr + offset), 0xA1);
            assert_eq!(bus.read_byte(port_info_ptr + offset), 0xB2);
            assert_eq!(bus.read_byte(prompt_ptr + offset), 0xC3);
            assert_eq!(bus.read_byte(label_ptr + offset), 0xD4);
            assert_eq!(bus.read_byte(nbp_type_ptr + offset), 0xE5);
        }
    }

    #[test]
    fn pack9_unknown_selector_fails_closed_and_preserves_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA82B;
        disp.current_selector_operation = Some("stale-identity");

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x1234_5678); // unknown selector
        bus.write_long(sp, 0xCAFE_BABE);

        let result = disp.dispatch_toolbox(true, 0x02B, &mut cpu, &mut bus);
        assert!(result.expect("Pack9 arm").is_ok());

        assert_eq!(
            disp.current_selector_operation, None,
            "Unknown selector identity must be cleared"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "Unknown selector must preserve A7"
        );
        assert_eq!(
            bus.read_long(sp),
            0xCAFE_BABE,
            "Unknown selector must preserve caller memory"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0xFFFF_FFCE,
            "Unknown selector must return paramErr (-50) in D0"
        );

        // Wrong trap form test
        disp.current_trap_word = 0xA82C;
        disp.current_selector_operation = Some("stale-identity");
        cpu.write_reg(Register::D0, 0x0000_0D00);
        let result = disp.dispatch_toolbox(true, 0x02B, &mut cpu, &mut bus);
        assert!(result.expect("Pack9 arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_long(sp), 0xCAFE_BABE);
        assert_eq!(cpu.read_reg(Register::D0), 0xFFFF_FFCE);
    }

    #[test]
    fn stackspace_os_trap_returns_free_space_and_preserves_stack_slot() {
        let (mut memory_disp, mut memory_cpu, mut memory_bus) = setup();
        let sp = TEST_SP;

        memory_cpu.write_reg(Register::A7, sp);
        memory_bus.write_long(sp, 0x1122_3344);

        let memory_result =
            memory_disp.dispatch_memory(false, 0x65, &mut memory_cpu, &mut memory_bus);

        assert!(memory_result.is_some(), "StackSpace should be handled");
        assert!(memory_result.unwrap().is_ok(), "StackSpace should return");
        assert_eq!(
            memory_cpu.read_reg(Register::A7),
            sp,
            "StackSpace should preserve the caller's stack pointer"
        );
        assert_eq!(
            memory_bus.read_long(sp),
            0x1122_3344,
            "StackSpace should not write a Pascal result slot on the caller stack"
        );
        assert!(
            memory_cpu.read_reg(Register::D0) > 0,
            "StackSpace should return a positive free stack space in D0"
        );
    }

    #[test]
    fn pack10_aliases_newemptyhandle_and_preserves_stack_slot() {
        let (mut toolbox_disp, mut toolbox_cpu, mut toolbox_bus) = setup();
        let (mut memory_disp, mut memory_cpu, mut memory_bus) = setup();
        let sp = TEST_SP;

        toolbox_cpu.write_reg(Register::A7, sp);
        memory_cpu.write_reg(Register::A7, sp);
        toolbox_bus.write_long(sp, 0x1122_3344);
        memory_bus.write_long(sp, 0x1122_3344);

        let toolbox_result =
            toolbox_disp.dispatch_toolbox(true, 0x02C, &mut toolbox_cpu, &mut toolbox_bus);
        let memory_result =
            memory_disp.dispatch_memory(false, 0x66, &mut memory_cpu, &mut memory_bus);

        assert!(toolbox_result.is_some(), "Pack10 should be handled");
        assert!(toolbox_result.unwrap().is_ok(), "Pack10 should return");
        assert!(memory_result.is_some(), "NewEmptyHandle should be handled");
        assert!(
            memory_result.unwrap().is_ok(),
            "NewEmptyHandle should return"
        );
        assert_eq!(
            toolbox_cpu.read_reg(Register::D0),
            memory_cpu.read_reg(Register::D0),
            "Pack10 should return the same D0 value as NewEmptyHandle"
        );
        assert_eq!(
            toolbox_cpu.read_reg(Register::A7),
            sp,
            "Pack10 should preserve the caller's stack pointer"
        );
        assert_eq!(
            toolbox_bus.read_long(sp),
            0x1122_3344,
            "Pack10 should not write a Pascal result slot on the caller stack"
        );
        assert_ne!(
            toolbox_cpu.read_reg(Register::A0),
            0,
            "Pack10 should return a non-NIL handle in A0"
        );
        assert_eq!(
            toolbox_bus.read_long(toolbox_cpu.read_reg(Register::A0)),
            0,
            "Pack10 should initialize the returned master pointer to NIL"
        );
        assert_ne!(
            memory_cpu.read_reg(Register::A0),
            0,
            "NewEmptyHandle should return a non-NIL handle in A0"
        );
        assert_eq!(
            memory_bus.read_long(memory_cpu.read_reg(Register::A0)),
            0,
            "NewEmptyHandle should initialize the returned master pointer to NIL"
        );
        assert_eq!(
            memory_cpu.read_reg(Register::A7),
            sp,
            "NewEmptyHandle should preserve the caller's stack pointer"
        );
        assert_eq!(
            memory_bus.read_long(sp),
            0x1122_3344,
            "NewEmptyHandle should not write a Pascal result slot on the caller stack"
        );
    }

    #[test]
    fn icon_dispatch_generated_routes_preserve_exact_d0_low_word_values() {
        assert_eq!(super::ICON_DISPATCH_OPERATION_ROUTES.len(), 1);
        let route =
            super::icon_dispatch_operation_route(0xABC9, 0x0000_0606).expect("LoadIconCache route");
        assert_eq!(route.selector, 0x0606);
        assert_eq!(route.routine_name, "LoadIconCache");
        assert_eq!(
            route.operation_id,
            "selector-operation:_IconDispatch:0x0606:d0-low-word-immediate:16"
        );
        assert_eq!(
            super::icon_dispatch_operation_route(0xABC9, 0xCAFE_0606),
            Some(route),
            "MOVE.W glue leaves the high word of D0 irrelevant to identity"
        );

        for (trap_word, selector) in [
            (0xABC8, 0x0606),
            (0xAAC9, 0x0606),
            (0xADC9, 0x0606),
            (0xAFC9, 0x0606),
            (0xABC9, 0x0604),
            (0xABC9, 0x0607),
            (0xABC9, u32::MAX),
        ] {
            assert!(super::icon_dispatch_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn icondispatch_geticonsuite_selects_available_family_members() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let output = 0x200800;
        let large = bus.alloc(128);
        let small = bus.alloc(128);
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([(
                0,
                ResourceFileMap {
                    loaded: HashMap::from([
                        ((*b"ICN#", 128), large),
                        ((*b"ics#", 128), small),
                    ]),
                    ..ResourceFileMap::default()
                },
            )]),
            names: HashMap::new(),
            search_order: vec![0],
            current_file: 0,
        });
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0501);
        bus.write_long(sp, 0x0000_0105); // large and small 1-bit, plus absent large 8-bit
        bus.write_word(sp + 4, 128);
        bus.write_long(sp + 6, output);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.expect("GetIconSuite dispatch").is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
        let suite = bus.read_long(output);
        assert_ne!(suite, 0);
        let data = bus.read_long(suite);
        assert_eq!(bus.read_bytes(data, 4), b"ISUT");
        assert_eq!(bus.read_word(data + 8), 2);
        assert_eq!(bus.read_bytes(data + 10, 4), b"ICN#");
        assert_eq!(bus.read_long(bus.read_long(data + 14)), large);
        assert_eq!(bus.read_bytes(data + 18, 4), b"ics#");
        assert_eq!(bus.read_long(bus.read_long(data + 22)), small);
    }

    #[test]
    fn icondispatch_geticonsuite_rejects_missing_output_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0105);
        bus.write_long(sp, 0xFFFF_FFFF);
        bus.write_word(sp + 4, 128);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.expect("GetIconSuite dispatch").is_ok());
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10) as i16, -50);
    }

    #[test]
    fn icondispatch_loadiconcache_records_then_clears_identity_without_changing_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xABC9;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xCAFE_0606);
        cpu.write_reg(Register::D1, 0x1122_3344);
        cpu.write_reg(Register::A0, 0x5566_7788);
        bus.write_long(sp, 0x0102_0304);
        bus.write_long(sp + 4, 0x0506_0708);
        bus.write_long(sp + 8, 0x090A_0B0C);

        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.expect("IconDispatch arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_IconDispatch:0x0606:d0-low-word-immediate:16")
        );
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(cpu.read_reg(Register::D1), 0x1122_3344);
        assert_eq!(cpu.read_reg(Register::A0), 0x5566_7788);
        assert_eq!(bus.read_long(sp), 0x0102_0304);
        assert_eq!(bus.read_long(sp + 4), 0x0506_0708);
        assert_eq!(bus.read_long(sp + 8), 0x090A_0B0C);

        disp.current_selector_operation = Some("stale-identity");
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0406);
        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.expect("IconDispatch unknown-selector arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        disp.current_selector_operation = Some("stale-identity");
        disp.current_trap_word = 0xAAC9;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0606);
        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.expect("IconDispatch wrong-trap-form arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    }

    #[test]
    fn icondispatch_selector_zero_returns_noerr_and_pops_eight_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0000); // selector 0 (NewIconSuite / no-op stub path)
        bus.write_long(sp, 0x1122_3344);
        bus.write_long(sp + 4, 0x5566_7788);

        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.is_some(), "IconDispatch should be handled");
        assert!(result.unwrap().is_ok(), "IconDispatch should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "selector 0 should return noErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 8,
            "selector 0 should pop eight bytes"
        );
        assert_eq!(bus.read_long(sp), 0x1122_3344);
        assert_eq!(bus.read_long(sp + 4), 0x5566_7788);
    }

    #[test]
    fn icondispatch_unsupported_selector_returns_param_err_and_pops_eight_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_BEEF); // unsupported in either byte order
        bus.write_long(sp, 0x1122_3344);
        bus.write_long(sp + 4, 0x5566_7788);

        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.is_some(), "IconDispatch should be handled");
        assert!(result.unwrap().is_ok(), "IconDispatch should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -50,
            "stub should return paramErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 8,
            "unsupported selector should still pop eight bytes"
        );
        assert_eq!(bus.read_long(sp), 0x1122_3344);
        assert_eq!(bus.read_long(sp + 4), 0x5566_7788);
    }

    #[test]
    fn icondispatch_selector_zero_preserves_non_d0_registers_and_pops_eight_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D1, 0x2222_3333);
        cpu.write_reg(Register::A0, 0x4444_5555);
        cpu.write_reg(Register::A1, 0x6666_7777);
        let sp_before = cpu.read_reg(Register::A7);

        cpu.write_reg(Register::D0, 0x0000_0000);
        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);
        assert!(result.is_some(), "IconDispatch should be handled");
        assert!(result.unwrap().is_ok(), "IconDispatch should return");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::D1), 0x2222_3333);
        assert_eq!(cpu.read_reg(Register::A0), 0x4444_5555);
        assert_eq!(cpu.read_reg(Register::A1), 0x6666_7777);
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 8);
    }

    #[test]
    fn icondispatch_public_selectors_derive_frames_after_byte_order_normalization() {
        let (mut disp, mut cpu, mut bus) = setup();

        for (selector, pop_bytes) in [(0x1306, 12), (0x0613, 12), (0x0005, 10), (0x0500, 10)] {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, selector);
            let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);

            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + pop_bytes);
        }
    }

    #[test]
    fn icondispatch_plotciconhandle_routes_to_legacy_renderer_and_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_061F); // byte-swapped $1F06 selector
        bus.write_long(sp, 0); // NIL CIconHandle is a safe no-op
        bus.write_word(sp + 4, 0x4000); // transform
        bus.write_word(sp + 6, 0); // alignment
        bus.write_long(sp + 8, 0); // destination Rect pointer
        bus.write_word(sp + 12, 0x7FFF); // reserved Pascal function result

        let result = disp.dispatch_toolbox(true, 0x3C9, &mut cpu, &mut bus);

        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 0);
    }

    #[test]
    fn translationdispatch_known_selector_returns_param_err_and_pops_four_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_001C); // GetFileTypesThatAppCanNativelyOpen
        bus.write_long(sp, 0x99AA_BBCC);
        bus.write_long(sp + 4, 0xDDEE_F011);

        let result = disp.dispatch_toolbox(true, 0x3FC, &mut cpu, &mut bus);
        assert!(result.is_some(), "TranslationDispatch should be handled");
        assert!(result.unwrap().is_ok(), "TranslationDispatch should return");
        assert_eq!(
            cpu.read_reg(Register::D0) as i16,
            -50,
            "stub should return paramErr"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 4,
            "stub should pop four bytes"
        );
        assert_eq!(bus.read_long(sp), 0x99AA_BBCC);
        assert_eq!(bus.read_long(sp + 4), 0xDDEE_F011);
    }

    #[test]
    fn translationdispatch_known_selector_preserves_non_d0_registers_and_pops_four_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        cpu.write_reg(Register::D0, 0x0000_0009);
        cpu.write_reg(Register::D1, 0x7777_8888);
        cpu.write_reg(Register::A0, 0x9999_AAAA);
        cpu.write_reg(Register::A1, 0xBBBB_CCCC);
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_toolbox(true, 0x3FC, &mut cpu, &mut bus);
        assert!(result.is_some(), "TranslationDispatch should be handled");
        assert!(result.unwrap().is_ok(), "TranslationDispatch should return");
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::D1), 0x7777_8888);
        assert_eq!(cpu.read_reg(Register::A0), 0x9999_AAAA);
        assert_eq!(cpu.read_reg(Register::A1), 0xBBBB_CCCC);
        assert_eq!(cpu.read_reg(Register::A7), sp_before + 4);
    }

    #[test]
    fn threaddispatch_begin_and_end_critical_roundtrip() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0xBEEF);

        cpu.write_reg(Register::D0, 0x0000_000B);
        let begin = disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus);
        assert!(
            begin.is_some(),
            "ThreadDispatch should handle ThreadBeginCritical"
        );
        assert!(begin.unwrap().is_ok(), "ThreadBeginCritical should return");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(disp.guest_calls.critical_depth(), 1);

        cpu.write_reg(Register::D0, 0x0000_000C);
        let end = disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus);
        assert!(
            end.is_some(),
            "ThreadDispatch should handle ThreadEndCritical"
        );
        assert!(end.unwrap().is_ok(), "ThreadEndCritical should return");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(disp.guest_calls.critical_depth(), 0);
    }

    /// `kPreemptiveThread` has no 68K implementation, so `NewThread` must
    /// refuse it rather than hand back an unschedulable ThreadID.
    /// MPW Interfaces/CIncludes/Threads.h.
    #[test]
    fn threaddispatch_newthread_rejects_preemptive_style_with_param_err() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let thread_made = TEST_SP + 0x400;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, thread_made);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, 0);
        bus.write_long(sp + 12, 4096);
        bus.write_long(sp + 16, 0);
        bus.write_long(sp + 20, 0x0004_2000);
        bus.write_long(sp + 24, 2); // kPreemptiveThread

        cpu.write_reg(Register::D0, 0x0000_0E03);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("ThreadDispatch should handle NewThread")
            .expect("NewThread should return");

        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);
        assert_eq!(bus.read_long(thread_made), 0, "no ThreadID on failure");
        assert!(disp.guest_calls.is_pristine());
    }

    #[test]
    fn threaddispatch_newthread_rejects_descriptor_before_allocating_and_preserves_task_id() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let thread_made = TEST_SP + 0x400;
        let entry = 0x0004_2000;
        let heap_before = bus.heap_bump_ptr();
        bus.write_word(
            entry,
            crate::guest_procedure::ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        );

        let invoke =
            |disp: &mut crate::trap::TrapDispatcher, cpu: &mut MockCpu, bus: &mut MacMemoryBus| {
                cpu.write_reg(Register::A7, sp);
                bus.write_long(sp, thread_made);
                bus.write_long(sp + 4, 0);
                bus.write_long(sp + 8, 0);
                bus.write_long(sp + 12, 4096);
                bus.write_long(sp + 16, 0x1234);
                bus.write_long(sp + 20, entry);
                bus.write_long(sp + 24, 1);
                cpu.write_reg(Register::D0, 0x0000_0E03);
                disp.dispatch_toolbox(true, 0x3F2, cpu, bus)
                    .expect("ThreadDispatch should handle NewThread")
                    .expect("NewThread should return");
            };

        invoke(&mut disp, &mut cpu, &mut bus);
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(bus.read_long(thread_made), 0);
        assert_eq!(bus.heap_bump_ptr(), heap_before);
        assert_eq!(disp.thread_return_trampoline, 0);
        assert!(disp.guest_calls.is_pristine());

        bus.write_word(entry, 0x4E75);
        invoke(&mut disp, &mut cpu, &mut bus);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(thread_made), 3);
    }

    /// `kApplicationThreadID` exists implicitly from launch, so
    /// `GetCurrentThread` names it and `GetThreadState` materialises its
    /// record instead of failing with threadNotFoundErr.
    #[test]
    fn threaddispatch_get_and_set_thread_state_track_the_ready_queue() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let out = TEST_SP + 0x400;

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out);
        cpu.write_reg(Register::D0, 0x0000_0206); // GetCurrentThread
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("GetCurrentThread handled")
            .expect("GetCurrentThread returns");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(
            bus.read_long(out),
            2,
            "kApplicationThreadID is the launch thread"
        );

        // GetThreadState(kCurrentThreadID, &state)
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out);
        bus.write_long(sp + 4, 1); // kCurrentThreadID
        cpu.write_reg(Register::D0, 0x0000_0407);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("GetThreadState handled")
            .expect("GetThreadState returns");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(
            bus.read_word(out),
            2,
            "the running thread reports kRunningThreadState"
        );
    }

    /// `SetThreadSwitcher` and `SetThreadTerminator` record their procs per
    /// thread, and the switch-in and switch-out slots stay independent.
    /// A Pascal Boolean occupies the high byte of its stack word.
    #[test]
    fn threaddispatch_switcher_and_terminator_are_recorded_per_thread() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0100); // inOrOut = true
        bus.write_long(sp + 2, 0x1111_2222); // switchProcParam
        bus.write_long(sp + 6, 0x0003_0000); // threadSwitcher
        bus.write_long(sp + 10, 2); // kApplicationThreadID
        cpu.write_reg(Register::D0, 0x0000_070A);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("SetThreadSwitcher handled")
            .expect("SetThreadSwitcher returns");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x3333_4444); // terminationProcParam
        bus.write_long(sp + 4, 0x0003_1000); // threadTerminator
        bus.write_long(sp + 8, 2);
        cpu.write_reg(Register::D0, 0x0000_0611);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("SetThreadTerminator handled")
            .expect("SetThreadTerminator returns");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        let thread = disp
            .guest_calls
            .cooperative_context(ExecutionTaskId::from_thread_id(2))
            .expect("the application thread record must exist");
        assert_eq!(thread.switch_in, (0x0003_0000, 0x1111_2222));
        assert_eq!(thread.switch_out, (0, 0), "switch-out stays unset");
        assert_eq!(thread.terminator, (0x0003_1000, 0x3333_4444));
    }

    /// `DisposeThread` retires a non-running thread, banks its stack for
    /// reuse, and drops it from the ready queue. `GetFreeThreadCount` then
    /// reports the pooled stack.
    #[test]
    fn threaddispatch_disposethread_pools_the_stack_for_reuse() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let thread_made = TEST_SP + 0x400;

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, thread_made);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, 0);
        bus.write_long(sp + 12, 4096);
        bus.write_long(sp + 16, 0);
        bus.write_long(sp + 20, 0x0004_2000);
        bus.write_long(sp + 24, 1); // kCooperativeThread
        cpu.write_reg(Register::D0, 0x0000_0E03);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("NewThread handled")
            .expect("NewThread returns");
        let new_id = bus.read_long(thread_made);
        assert!(
            disp.guest_calls
                .scheduling_state(ExecutionTaskId::from_thread_id(new_id))
                == Some(ExecutionTaskState::Ready)
        );

        // DisposeThread(threadToDump, threadResult, recycleThread)
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0100); // recycleThread = true
        bus.write_long(sp + 2, 0); // threadResult
        bus.write_long(sp + 6, new_id);
        cpu.write_reg(Register::D0, 0x0000_0504);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("DisposeThread handled")
            .expect("DisposeThread returns");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert!(!disp
            .guest_calls
            .cooperative_context(ExecutionTaskId::from_thread_id(new_id))
            .is_some());
        assert!(
            disp.guest_calls
                .scheduling_state(ExecutionTaskId::from_thread_id(new_id))
                != Some(ExecutionTaskState::Ready)
        );
        assert_eq!(disp.guest_calls.classic_thread_pool_count(0), 1);

        // GetFreeThreadCount(threadStyle, freeCount)
        let out = TEST_SP + 0x420;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out);
        bus.write_long(sp + 4, 1);
        cpu.write_reg(Register::D0, 0x0000_0402);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("GetFreeThreadCount handled")
            .expect("GetFreeThreadCount returns");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(out), 1, "the disposed stack is pooled");
    }

    /// `GetDefaultThreadStackSize` reports the size `NewThread` uses when
    /// passed 0, and refuses the preemptive style.
    #[test]
    fn thread_pool_creation_refuses_a_protected_pascal_result_before_allocating() {
        let (mut disp, mut cpu, mut bus) = setup();
        let before = bus.heap_bump_ptr();
        bus.write_long(TEST_SP, 1024);
        bus.write_word(TEST_SP + 4, 2);
        bus.write_long(TEST_SP + 6, 1);
        bus.protect_readonly_code(TEST_SP + 11, 1);
        cpu.write_reg(Register::D0, 0x0501);
        disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(disp.guest_calls.classic_thread_pool_count(0), 0);
        assert_eq!(bus.heap_bump_ptr(), before);
    }

    #[test]
    fn thread_pool_allocation_failure_rolls_back_and_retries_without_publishing_entries() {
        let (mut disp, mut cpu, mut bus) = setup();
        let base = bus.classic_heap_limit() - 1028;
        bus.reserve_heap_until(base);
        for (count, expected) in [(2, -108_i16), (1, 0)] {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, 0x0501);
            bus.write_long(TEST_SP, 1024);
            bus.write_word(TEST_SP + 4, count);
            bus.write_long(TEST_SP + 6, 1);
            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0) as i16, expected);
            assert_eq!(
                disp.guest_calls.classic_thread_pool_count(0),
                usize::from(expected == 0)
            );
            assert_eq!(
                bus.get_alloc_size(base),
                if expected == 0 { Some(1024) } else { None }
            );
        }
        assert_eq!(disp.guest_calls.create_task().unwrap().thread_id(), 3);
    }

    #[test]
    fn thread_pool_creation_adds_distinct_stacks_to_the_execution_owner() {
        let (mut disp, mut cpu, mut bus) = setup();
        for expected_count in [3, 6] {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, 0x0501);
            bus.write_long(TEST_SP, 1024);
            bus.write_word(TEST_SP + 4, 3);
            bus.write_long(TEST_SP + 6, 1);
            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0), 0);
            assert_eq!(
                disp.guest_calls.classic_thread_pool_count(1024),
                expected_count
            );
        }
        let mut stacks = Vec::new();
        while let Some(stack) = disp.guest_calls.take_classic_thread_stack(1024) {
            assert!(!stacks.contains(&stack));
            stacks.push(stack);
        }
        assert_eq!(stacks.len(), 6);
    }

    #[test]
    fn threaddispatch_default_stack_size_is_reported_and_refuses_preemptive() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let out = TEST_SP + 0x400;

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out);
        bus.write_long(sp + 4, 1); // kCooperativeThread
        cpu.write_reg(Register::D0, 0x0000_0413);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("GetDefaultThreadStackSize handled")
            .expect("GetDefaultThreadStackSize returns");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_long(out), 32 * 1024);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, out);
        bus.write_long(sp + 4, 2); // kPreemptiveThread
        cpu.write_reg(Register::D0, 0x0000_0413);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .expect("GetDefaultThreadStackSize handled")
            .expect("GetDefaultThreadStackSize returns");
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
    }

    #[test]
    fn classic_stack_space_query_uses_the_parked_native_application_limit() {
        use crate::guest_call::{GuestCallTarget, M68kRegisterState};
        use crate::guest_procedure::GuestIsa;
        let (mut disp, mut cpu, mut bus) = setup();
        let output = TEST_SP + 0x400;
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, 0x1000);
        disp.process_memory_manager()
            .borrow_mut()
            .set_application_heap_limit(0x8000);
        // A detached legacy owner has no entry metadata; the retained
        // outer native call still identifies its original stack.
        let mut native = ppc::PpcCpu::new();
        native.gpr[1] = 0x9000;
        let mut classic = crate::cpu::M68kCpu::new();
        assert!(disp.guest_calls.begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: 0x5000,
                rtoc: 0
            },
            0x5000,
            TEST_SP,
            0x7000,
            TEST_SP + 4,
            M68kRegisterState::default(),
            None,
            0x8000,
            0x2000,
            crate::guest_call::GuestCallReturnPolicy::Preserve
        ));
        disp.guest_calls
            .activate_m68k_parking(&mut classic, &mut native)
            .unwrap();
        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 0x0414);
        bus.write_long(TEST_SP, output);
        bus.write_long(TEST_SP + 4, 1);
        disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(output), 0x1000);
    }

    #[test]
    fn threaddispatch_stack_space_tracks_application_limit_and_native_workers() {
        let (mut disp, mut cpu, mut bus) = setup();
        let output = TEST_SP + 0x400;
        let invoke = |disp: &mut TrapDispatcher,
                      cpu: &mut MockCpu,
                      bus: &mut crate::memory::MacMemoryBus,
                      thread: u32| {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, 0x0414);
            bus.write_long(TEST_SP, output);
            bus.write_long(TEST_SP + 4, thread);
            disp.dispatch_toolbox(true, 0x3f2, cpu, bus)
                .unwrap()
                .unwrap();
            cpu.read_reg(Register::D0) as i16
        };
        for (alias, available) in [(0, 0x1200), (1, 0x800), (2, 0x400)] {
            bus.write_long(
                crate::memory::globals::addr::APPL_LIMIT,
                TEST_SP - available,
            );
            assert_eq!(invoke(&mut disp, &mut cpu, &mut bus, alias), 0);
            assert_eq!(bus.read_long(output), available);
        }
        bus.write_long(crate::memory::globals::addr::APPL_LIMIT, 0);
        assert_eq!(invoke(&mut disp, &mut cpu, &mut bus, 1), -619);
        assert_eq!(bus.read_long(output), 0x400);
        let mut native = ppc::PpcCpu::new();
        native.gpr[1] = 0x8700;
        let worker = disp
            .guest_calls
            .create_native_thread(
                crate::guest_call::NativeThreadContext {
                    context: native.capture_execution_context(),
                },
                crate::guest_call::ThreadStorage {
                    stack_base: 0x8000,
                    stack_limit: 0x9000,
                    ..Default::default()
                },
                true,
                |_| true,
            )
            .unwrap();
        assert_eq!(invoke(&mut disp, &mut cpu, &mut bus, worker.thread_id()), 0);
        assert_eq!(bus.read_long(output), 0x700);
        assert_eq!(invoke(&mut disp, &mut cpu, &mut bus, 0xdead), -618);
        assert_eq!(bus.read_long(output), 0x700);
    }

    #[test]
    fn threaddispatch_queries_reject_protected_output_without_partial_writes() {
        for selector in [0x0206, 0x0407, 0x0414] {
            let (mut disp, mut cpu, mut bus) = setup();
            bus.write_long(crate::memory::globals::addr::APPL_LIMIT, TEST_SP - 0x1000);
            let out = TEST_SP + 0x100;
            bus.write_long(out, 0xaabb_ccdd);
            bus.protect_readonly_code(out + 1, 1);
            bus.write_long(TEST_SP, out);
            if selector != 0x0206 {
                bus.write_long(TEST_SP + 4, 1);
            }
            cpu.write_reg(Register::D0, selector);
            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
            assert_eq!(bus.read_long(out), 0xaabb_ccdd);
        }
    }

    #[test]
    fn threaddispatch_invalid_state_does_not_partially_end_a_critical_section() {
        let (mut disp, mut cpu, mut bus) = setup();
        disp.guest_calls.begin_critical();
        cpu.write_reg(Register::A7, TEST_SP);
        bus.write_long(TEST_SP, 0);
        bus.write_word(TEST_SP + 4, 99);
        bus.write_long(TEST_SP + 6, 1);
        cpu.write_reg(Register::D0, 0x0512);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i16, -619);
        assert_eq!(bus.read_word(TEST_SP + 10), (-619_i16) as u16);
        assert_eq!(disp.guest_calls.critical_depth(), 1);
        assert_eq!(
            disp.guest_calls
                .scheduling_state(ExecutionTaskId::APPLICATION),
            Some(ExecutionTaskState::Running)
        );
    }

    #[test]
    fn thread_state_preflights_successor_and_return_write_before_ending_critical() {
        for missing_context in [true, false] {
            let (mut disp, mut cpu, mut bus) = setup();
            let worker = ExecutionTaskId::from_thread_id(3);
            assert!(disp.guest_calls.register_task(worker));
            assert!(disp
                .guest_calls
                .set_scheduling_state(worker, ExecutionTaskState::Ready));
            if !missing_context {
                assert!(disp
                    .guest_calls
                    .save_cooperative_context(worker, super::CooperativeThread::default()));
            }
            disp.guest_calls.begin_critical();
            bus.write_long(TEST_SP, 3);
            bus.write_word(TEST_SP + 4, 1);
            bus.write_long(TEST_SP + 6, 1);
            bus.write_word(TEST_SP + 10, 0x1234);
            if !missing_context {
                bus.protect_readonly_code(TEST_SP + 11, 1);
            }
            cpu.write_reg(Register::D0, 0x0512);
            let before = disp.guest_calls.clone();
            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0) as i16, -619);
            assert_eq!(disp.guest_calls, before);
            assert_eq!(disp.guest_calls.critical_depth(), 1);
        }
    }

    fn retire_classic_for_test<C: CpuOps>(
        disp: &mut TrapDispatcher,
        cpu: &mut C,
        bus: &mut MacMemoryBus,
        task: ExecutionTaskId,
        result: u32,
        recycle: bool,
    ) -> bool {
        let retirement = disp
            .guest_calls
            .retire_classic_thread(task, recycle, |saved| {
                saved.result_destination == 0
                    || bus.try_write_long(saved.result_destination, result)
            });
        let Ok(retirement) = retirement else {
            return false;
        };
        disp.apply_classic_retirement(cpu, bus, recycle, retirement);
        true
    }

    #[test]
    fn thread_retirement_rejects_partial_result_writes_and_can_retry() {
        for self_exit in [false, true] {
            let (mut disp, mut cpu, mut bus) = setup();
            let worker = ExecutionTaskId::from_thread_id(3);
            let application = ExecutionTaskId::APPLICATION;
            let result_slot = TEST_SP + 0x100;
            bus.write_long(result_slot, 0x1234_5678);
            bus.protect_readonly_code(result_slot + 2, 2);
            assert!(disp.guest_calls.register_task(worker));
            assert!(disp
                .guest_calls
                .set_scheduling_state(worker, ExecutionTaskState::Ready));
            let saved = super::CooperativeThread::default();
            let mut storage = crate::guest_call::ThreadStorage {
                result_destination: result_slot,
                stack_base: 0x1000,
                stack_limit: 0x2000,
                managed_pointer: false,
            };
            assert!(disp.guest_calls.set_thread_storage(worker, storage));
            assert!(disp
                .guest_calls
                .save_cooperative_context(worker, saved.clone()));
            let mut app = super::CooperativeThread::default();
            app.pc = 0x4321;
            app.a_regs[0] = 0x8765;
            assert!(disp.guest_calls.save_cooperative_context(application, app));
            if self_exit {
                assert!(disp.guest_calls.switch_to_task(worker));
            }
            cpu.write_reg(Register::PC, 0x1234);
            cpu.write_reg(Register::A0, 0xcafe_babe);
            let current = disp.guest_calls.current_task();
            let ready = disp.guest_calls.next_ready_task(None);
            let retired =
                retire_classic_for_test(&mut disp, &mut cpu, &mut bus, worker, 0xcafe_babe, false);
            assert!(!retired);
            assert_eq!(disp.guest_calls.current_task(), current);
            assert_eq!(disp.guest_calls.next_ready_task(None), ready);
            assert_eq!(
                disp.guest_calls.cooperative_context(worker),
                Some(saved.clone())
            );
            assert_eq!(bus.read_long(result_slot), 0x1234_5678);
            assert_eq!(cpu.read_reg(Register::PC), 0x1234);
            assert_eq!(cpu.read_reg(Register::A0), 0xcafe_babe);
            assert!(disp.guest_calls.classic_thread_pool_count(0) == 0);

            storage.result_destination = result_slot + 8;
            assert!(disp.guest_calls.set_thread_storage(worker, storage));
            let retired =
                retire_classic_for_test(&mut disp, &mut cpu, &mut bus, worker, 0xcafe_babe, false);
            assert!(retired);
            assert_eq!(disp.guest_calls.current_task(), application);
            assert!(disp.guest_calls.cooperative_context(worker).is_none());
            assert_eq!(disp.guest_calls.scheduling_state(worker), None);
            assert_eq!(bus.read_long(result_slot + 8), 0xcafe_babe);
            assert_eq!(disp.guest_calls.classic_thread_pool_count(0), 0);
            if self_exit {
                assert_eq!(cpu.read_reg(Register::PC), 0x4321);
                assert_eq!(cpu.read_reg(Register::A0), 0x8765);
            }
        }
    }

    #[test]
    fn thread_exit_without_a_successor_context_keeps_the_task_stack_and_result() {
        let (mut disp, mut cpu, mut bus) = setup();
        let worker = ExecutionTaskId::from_thread_id(3);
        let result_slot = TEST_SP + 0x100;
        bus.write_long(result_slot, 0x1234_5678);
        assert!(disp.guest_calls.register_task(worker));
        assert!(disp
            .guest_calls
            .set_scheduling_state(worker, ExecutionTaskState::Ready));
        let saved = super::CooperativeThread::default();
        assert!(disp.guest_calls.set_thread_storage(
            worker,
            crate::guest_call::ThreadStorage {
                result_destination: result_slot,
                stack_base: 0x1000,
                stack_limit: 0x2000,
                managed_pointer: false
            }
        ));
        assert!(disp
            .guest_calls
            .save_cooperative_context(worker, saved.clone()));
        assert!(disp.guest_calls.switch_to_task(worker));
        // The application is ready, but no adapter snapshot exists for it.
        cpu.write_reg(Register::A0, 0xcafe_babe);
        assert!(!retire_classic_for_test(
            &mut disp,
            &mut cpu,
            &mut bus,
            worker,
            0xcafe_babe,
            false,
        ));
        assert_eq!(disp.guest_calls.current_task(), worker);
        assert_eq!(disp.guest_calls.cooperative_context(worker), Some(saved));
        assert_eq!(bus.read_long(result_slot), 0x1234_5678);
        assert!(disp.guest_calls.classic_thread_pool_count(0) == 0);
    }

    #[test]
    fn threaddispatch_endcritical_underflow_returns_thread_protocol_err() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0000_000C);

        let result = disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "ThreadDispatch should handle ThreadEndCritical"
        );
        assert!(result.unwrap().is_ok(), "ThreadEndCritical should return");
        assert_eq!(cpu.read_reg(Register::D0) as i16, -619);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_word(sp), (-619i16) as u16);
        assert_eq!(disp.guest_calls.critical_depth(), 0);
    }

    #[test]
    fn threaddispatch_unsupported_selector_returns_param_err() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = cpu.read_reg(Register::A7);
        bus.write_word(sp, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0000_0000);
        cpu.write_reg(Register::D1, 0x1111_2222);
        cpu.write_reg(Register::A0, 0x3333_4444);
        cpu.write_reg(Register::A1, 0x5555_6666);

        let result = disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "ThreadDispatch should be handled");
        assert!(result.unwrap().is_ok(), "ThreadDispatch should return");
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::D1), 0x1111_2222);
        assert_eq!(cpu.read_reg(Register::A0), 0x3333_4444);
        assert_eq!(cpu.read_reg(Register::A1), 0x5555_6666);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_word(sp), (-50i16) as u16);
        assert_eq!(disp.guest_calls.critical_depth(), 0);
    }

    #[test]
    fn threaddispatch_newthread_uses_the_adopted_execution_namespace() {
        let (mut disp, mut cpu, mut bus) = setup();
        let process = crate::guest_call::SharedGuestCallStack::default();
        assert!(process.register_task(ExecutionTaskId::from_thread_id(40)));
        disp.guest_calls.attach_to(&process);
        let thread_made = bus.alloc(4);
        for expected in [41, 43] {
            cpu.write_reg(Register::A7, TEST_SP);
            bus.write_long(TEST_SP, thread_made);
            bus.write_long(TEST_SP + 4, 0);
            bus.write_long(TEST_SP + 8, 1); // kNewSuspend
            bus.write_long(TEST_SP + 12, 4096);
            bus.write_long(TEST_SP + 16, 0);
            bus.write_long(TEST_SP + 20, 0x0004_2000);
            bus.write_long(TEST_SP + 24, 1);
            cpu.write_reg(Register::D0, 0x0E03);
            disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0), 0);
            assert_eq!(bus.read_long(thread_made), expected);
            assert!(process
                .cooperative_context(ExecutionTaskId::from_thread_id(expected))
                .is_some());
            // Another creator shares the owner's namespace between classic calls.
            assert_eq!(process.create_task().unwrap().thread_id(), expected + 1);
        }
    }

    #[test]
    fn classic_newthread_obeys_pool_options_without_consuming_ids_on_refusal() {
        // Thread Manager (1999), pp. 48, 57–58.
        for (options, requested, expected_size, expected_error) in [
            (0, 1024, 1024, 0),
            (2, 1024, 1024, 0),
            (2, 1200, 2048, 0),
            (2 | 16, 1200, 0, -617),
            (2 | 16 | 4, 1200, 1200, 0),
            (2, 8192, 0, -617),
            (2 | 4, 8192, 8192, 0),
        ] {
            let (mut disp, mut cpu, mut bus) = setup();
            let made = TEST_SP + 0x100;
            let mut pooled = Vec::new();
            // Deliberately put larger stacks first to distinguish best fit
            // from first fit, including an exact match at the end.
            for size in [4096, 2048, 1024] {
                let base = bus.alloc(size);
                pooled.push((base, size));
                disp.guest_calls
                    .recycle_classic_thread_stack((base, base + size));
            }
            for (offset, value) in [
                (0, made),
                (4, 0),
                (8, options | 1),
                (12, requested),
                (16, 0xcafebabe),
                (20, 0x10000),
                (24, 1),
            ] {
                bus.write_long(TEST_SP + offset, value);
            }
            bus.write_long(made, u32::MAX);
            cpu.write_reg(Register::D0, 0x0e03);
            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0) as i16, expected_error);
            if expected_error != 0 {
                assert_eq!(bus.read_long(made), 0);
                assert!(!disp.guest_calls.has_live_workers());
                assert_eq!(disp.guest_calls.classic_thread_pool_count(0), 3);
                // A fresh request retries successfully using the unconsumed ID.
                cpu.write_reg(Register::A7, TEST_SP);
                cpu.write_reg(Register::D0, 0x0e03);
                bus.write_long(TEST_SP + 8, 1);
                disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                    .unwrap()
                    .unwrap();
                assert_eq!(cpu.read_reg(Register::D0), 0);
                assert_eq!(bus.read_long(made), 3);
                continue;
            }
            let task = ExecutionTaskId::from_thread_id(bus.read_long(made));
            let storage = disp.guest_calls.thread_storage(task).unwrap();
            assert_eq!(storage.stack_limit - storage.stack_base, expected_size);
            let selected = pooled.iter().find(|&&(base, _)| base == storage.stack_base);
            let should_reuse = options & 2 != 0 && expected_size != 1200 && expected_size != 8192;
            assert_eq!(selected.is_some(), should_reuse);
            assert_eq!(
                disp.guest_calls.classic_thread_pool_count(0),
                if should_reuse { 2 } else { 3 }
            );
            assert_eq!(
                disp.guest_calls.scheduling_state(task),
                Some(ExecutionTaskState::Stopped)
            );
            let context = disp.guest_calls.cooperative_context(task).unwrap();
            assert_eq!(bus.read_long(context.a_regs[7] + 4), 0xcafebabe);
        }
    }

    #[test]
    fn classic_thread_creation_refuses_bad_outputs_and_stacks_without_consuming_an_id() {
        for fault in 0..4 {
            let (mut disp, mut cpu, mut bus) = setup();
            let made = TEST_SP + 0x100;
            let stack = bus.alloc(1024);
            disp.guest_calls
                .recycle_classic_thread_stack((stack, stack + 1024));
            bus.write_long(stack + 1016, 0x11223344);
            bus.write_long(stack + 1020, 0x55667788);
            let stack_bytes = (bus.read_long(stack + 1016), bus.read_long(stack + 1020));
            bus.write_long(made, 0xaaaaaaaa);
            for (offset, value) in [
                (0, made),
                (4, 0),
                (8, 1 | 2 | 4),
                (12, if fault == 2 { 4 } else { 1024 }),
                (16, 0xcafebabe),
                (20, 0x10000),
                (24, 1),
            ] {
                bus.write_long(TEST_SP + offset, value);
            }
            match fault {
                0 => bus.protect_readonly_code(made + 2, 2),
                1 => bus.protect_readonly_code(stack + 1022, 2),
                3 => bus.protect_readonly_code(TEST_SP + 29, 1),
                _ => {}
            }
            cpu.write_reg(Register::D0, 0x0e03);
            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
            assert_eq!(bus.read_long(made), if fault == 0 { 0xaaaaaaaa } else { 0 });
            assert_eq!(disp.guest_calls.classic_thread_pool_count(0), 1);
            assert!(!disp.guest_calls.has_live_workers());
            assert_eq!(
                (bus.read_long(stack + 1016), bus.read_long(stack + 1020)),
                stack_bytes
            );
            // Retry with a valid output, return frame, size and stack. If the
            // pool stack itself is protected, reserve another stack for retry.
            let retry_sp = TEST_SP + 0x200;
            let retry_made = TEST_SP + 0x300;
            if fault == 1 {
                disp.guest_calls.take_classic_thread_stack(1024);
            }
            for (offset, value) in [
                (0, retry_made),
                (4, 0),
                (8, 1 | 2 | 4),
                (12, 1024),
                (16, 0xcafebabe),
                (20, 0x10000),
                (24, 1),
            ] {
                bus.write_long(retry_sp + offset, value);
            }
            cpu.write_reg(Register::A7, retry_sp);
            cpu.write_reg(Register::D0, 0x0e03);
            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0), 0);
            assert_eq!(bus.read_long(retry_made), 3);
            let worker = disp
                .guest_calls
                .cooperative_context(ExecutionTaskId::from_thread_id(3))
                .unwrap();
            assert_eq!(bus.read_long(worker.a_regs[7] + 4), 0xcafebabe);
            assert_eq!(
                bus.read_long(worker.a_regs[7]),
                disp.thread_return_trampoline
            );
        }
    }

    #[test]
    fn threaddispatch_newthread_accepts_default_stack_and_consumes_mpw_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let thread_made = bus.alloc(4);
        let entry = 0x0012_3456;
        let param = 0x0065_4321;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::A5, 0x00AA_5500);
        bus.write_long(sp, thread_made);
        bus.write_long(sp + 4, 0); // optional threadResult
        bus.write_long(sp + 8, 0x0000_0008); // kFPUNotNeeded
        bus.write_long(sp + 12, 0); // documented default stack size
        bus.write_long(sp + 16, param);
        bus.write_long(sp + 20, entry);
        bus.write_long(sp + 24, 0x0000_0001); // kCooperativeThread
        bus.write_word(sp + 28, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0000_0E03);

        let result = disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus);
        assert!(result.is_some(), "ThreadDispatch should handle NewThread");
        assert!(result.unwrap().is_ok(), "NewThread should return");
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);
        assert_eq!(bus.read_word(sp + 28), 0);
        assert_eq!(bus.read_long(thread_made), 3);
    }

    #[test]
    fn threaddispatch_yield_to_any_thread_roundtrips_complete_68k_contexts() {
        let (mut disp, _, mut bus) = setup();
        let mut cpu = crate::cpu::M68kCpu::new();
        cpu.core.set_sr(0x3010);
        cpu.core.fpr = std::array::from_fn(|i| m68k::fpu::FloatX80 {
            mantissa: 0x8000_0000_0000_0001 + i as u64,
            sign_exp: 0x7fff,
        });
        cpu.core.fpcr = 0x1234;
        cpu.core.fpsr = 0x5678;
        cpu.core.fpiar = 0x0010_2340;
        let initial_fpu = (
            cpu.core.fpr,
            cpu.core.fpcr,
            cpu.core.fpsr,
            cpu.core.fpiar,
            cpu.core.fpu_just_reset,
        );
        let new_sp = TEST_SP;
        let thread_made = bus.alloc(4);
        let entry = 0x0012_3456;
        let param = 0x0065_4321;

        cpu.write_reg(Register::A7, new_sp);
        cpu.write_reg(Register::A5, 0x00AA_5500);
        bus.write_long(new_sp, thread_made);
        bus.write_long(new_sp + 4, 0);
        bus.write_long(new_sp + 8, 0x0000_0008);
        bus.write_long(new_sp + 12, 0x0000_1800);
        bus.write_long(new_sp + 16, param);
        bus.write_long(new_sp + 20, entry);
        bus.write_long(new_sp + 24, 1);
        bus.write_word(new_sp + 28, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0E03);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let app_sp = new_sp + 28;
        let app_pc = 0x000F_0000;
        cpu.core.set_sr(0x851b);
        cpu.write_reg(Register::A7, app_sp);
        cpu.core.fpr.reverse();
        cpu.core.fpcr = 0x4321;
        cpu.core.fpsr = 0x8765;
        cpu.core.fpiar = 0x0010_9870;
        cpu.core.fpu_just_reset = false;
        let app_fpu = (
            cpu.core.fpr,
            cpu.core.fpcr,
            cpu.core.fpsr,
            cpu.core.fpiar,
            cpu.core.fpu_just_reset,
        );
        cpu.write_reg(Register::PC, app_pc);
        cpu.write_reg(Register::D3, 0xCAFE_BABE);
        disp.guest_calls.begin_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: 0x000E_0000,
                rtoc: 0,
            },
            0x000E_1000,
            app_sp,
        );
        bus.write_long(app_sp, 0); // YieldToAnyThread's synthetic ThreadID
        bus.write_word(app_sp + 4, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0205);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(disp.guest_calls.current_task().thread_id(), 3);
        assert_eq!(
            disp.guest_calls.depth(),
            0,
            "the worker must not inherit the application task's continuation"
        );
        disp.guest_calls.begin_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: 0x000D_0000,
                rtoc: 0,
            },
            0x000D_1000,
            cpu.read_reg(Register::A7),
        );
        assert_eq!(cpu.core.get_sr() & 0xff00, 0x3000);
        assert_eq!(
            (
                cpu.core.fpr,
                cpu.core.fpcr,
                cpu.core.fpsr,
                cpu.core.fpiar,
                cpu.core.fpu_just_reset
            ),
            initial_fpu
        );
        assert_eq!(cpu.read_reg(Register::PC), entry);
        assert_eq!(cpu.read_reg(Register::A5), 0x00AA_5500);
        let thread_sp = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_long(thread_sp), disp.thread_return_trampoline);
        assert_eq!(bus.read_long(thread_sp + 4), param);

        let thread_yield_sp = thread_sp - 8;
        cpu.write_reg(Register::A7, thread_yield_sp);
        bus.write_long(thread_yield_sp, 0);
        bus.write_word(thread_yield_sp + 4, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0205);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(disp.guest_calls.current_task().thread_id(), 2);
        assert_eq!(
            disp.guest_calls.depth(),
            1,
            "returning to the application must restore its continuation owner"
        );
        assert_eq!(
            disp.guest_calls
                .task_depth(crate::guest_call::ExecutionTaskId::from_thread_id(3)),
            1,
            "the suspended worker continuation must remain task-local"
        );
        assert_eq!(cpu.core.get_sr(), 0x851b);
        assert_eq!(
            (
                cpu.core.fpr,
                cpu.core.fpcr,
                cpu.core.fpsr,
                cpu.core.fpiar,
                cpu.core.fpu_just_reset
            ),
            app_fpu
        );
        assert_eq!(cpu.read_reg(Register::PC), app_pc);
        assert_eq!(cpu.read_reg(Register::A7), app_sp + 4);
        assert_eq!(cpu.read_reg(Register::D3), 0xCAFE_BABE);
        assert_eq!(bus.read_word(app_sp + 4), 0);

        // SetThreadState has a separate ABI path for saving the outgoing task.
        // Refresh every register while preserving its registered hooks.
        let app = ExecutionTaskId::from_thread_id(2);
        let mut saved = disp.guest_calls.cooperative_context(app).unwrap();
        saved.switch_in = (0x1000, 1);
        saved.switch_out = (0x2000, 2);
        saved.terminator = (0x3000, 3);
        assert!(disp.guest_calls.save_cooperative_context(app, saved));
        cpu.core.set_sr(0x3209);
        cpu.core.fpr.rotate_left(1);
        cpu.core.fpcr = 0x40;
        cpu.core.fpsr = 0x80;
        cpu.core.fpiar = 0x0012_1000;
        cpu.core.fpu_just_reset = true;
        let latest = cpu.capture_extended_context();
        cpu.write_reg(Register::A7, app_sp);
        bus.write_long(app_sp, 3);
        bus.write_word(app_sp + 4, 0); // ready
        bus.write_long(app_sp + 6, 1); // current thread
        cpu.write_reg(Register::D0, 0x0508);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(disp.guest_calls.current_task().thread_id(), 3);
        let worker_sp = cpu.read_reg(Register::A7) - 8;
        cpu.write_reg(Register::A7, worker_sp);
        bus.write_long(worker_sp, 2);
        cpu.write_reg(Register::D0, 0x0205);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(disp.guest_calls.current_task(), app);
        assert_eq!(cpu.capture_extended_context(), latest);
        assert_eq!(cpu.read_reg(Register::A7), app_sp + 10);
        let saved = disp.guest_calls.cooperative_context(app).unwrap();
        assert_eq!(saved.switch_in, (0x1000, 1));
        assert_eq!(saved.switch_out, (0x2000, 2));
        assert_eq!(saved.terminator, (0x3000, 3));
    }

    #[test]
    fn classic_yield_refuses_missing_successor_context_without_publishing_source() {
        let (mut disp, mut cpu, mut bus) = setup();
        let application = ExecutionTaskId::APPLICATION;
        let worker = ExecutionTaskId::from_thread_id(3);
        assert!(disp.guest_calls.register_task(worker));
        assert!(disp
            .guest_calls
            .set_scheduling_state(worker, ExecutionTaskState::Ready));
        let saved = crate::guest_call::CooperativeThread {
            d_regs: [0x1111_0000; 8],
            a_regs: [0x2222_0000; 8],
            pc: 0x3333_0000,
            ccr: 0x14,
            extended: None,
            switch_in: (0x4444_0000, 1),
            switch_out: (0x5555_0000, 2),
            terminator: (0x6666_0000, 3),
        };
        assert!(disp
            .guest_calls
            .save_cooperative_context(application, saved.clone()));
        cpu.write_reg(Register::PC, 0x1234_5678);
        cpu.write_reg(Register::D3, 0x89ab_cdef);
        cpu.write_reg(Register::A5, 0x7654_3210);
        let sp = TEST_SP;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, worker.thread_id());
        bus.write_word(sp + 4, 0xbeef);
        cpu.write_reg(Register::D0, 0x0205);
        let mut expected_live = crate::guest_call::CooperativeThread::capture(&cpu);
        expected_live.d_regs[0] = (-619i16) as u32;
        expected_live.a_regs[7] = sp.wrapping_add(4);
        let before_extended = cpu.capture_extended_context();
        let before_calls = disp.guest_calls.clone();

        disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::D0), (-619i16) as u32);
        assert_eq!(cpu.read_reg(Register::A7), sp.wrapping_add(4));
        assert_eq!(bus.read_word(sp + 4), (-619i16) as u16);
        assert_eq!(
            crate::guest_call::CooperativeThread::capture(&cpu),
            expected_live
        );
        assert_eq!(cpu.capture_extended_context(), before_extended);
        assert_eq!(disp.guest_calls, before_calls);
        assert_eq!(disp.guest_calls.current_task(), application);
        assert_eq!(
            disp.guest_calls.scheduling_state(worker),
            Some(ExecutionTaskState::Ready)
        );
        assert_eq!(
            disp.guest_calls.cooperative_context(application),
            Some(saved)
        );
        assert_eq!(disp.guest_calls.cooperative_context(worker), None);
        assert!(!disp.guest_calls.has_pending_task_handoff());
    }

    #[test]
    fn classic_yield_refuses_during_critical_sections_without_mutating_contexts() {
        for suggested in [0, 3] {
            let (mut disp, mut cpu, mut bus) = setup();
            let application = ExecutionTaskId::APPLICATION;
            let worker = ExecutionTaskId::from_thread_id(3);
            let worker_context = crate::guest_call::CooperativeThread {
                pc: 0x3000,
                a_regs: [0, 0, 0, 0, 0, 0, 0, 0x9000],
                ..Default::default()
            };
            assert!(disp.guest_calls.register_task(worker));
            assert!(disp
                .guest_calls
                .save_cooperative_context(worker, worker_context.clone()));
            assert!(disp
                .guest_calls
                .set_scheduling_state(worker, ExecutionTaskState::Ready));
            disp.guest_calls.begin_critical();
            cpu.write_reg(Register::PC, 0x1234_5678);
            cpu.write_reg(Register::D3, 0x89ab_cdef);
            let sp = TEST_SP;
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, suggested);
            bus.write_word(sp + 4, 0xbeef);
            cpu.write_reg(Register::D0, 0x0205);
            let mut expected_live = crate::guest_call::CooperativeThread::capture(&cpu);
            expected_live.d_regs[0] = (-619i16) as u32;
            expected_live.a_regs[7] = sp.wrapping_add(4);
            let before_extended = cpu.capture_extended_context();
            let before_application = disp.guest_calls.cooperative_context(application);
            let before_calls = disp.guest_calls.clone();

            disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();

            assert_eq!(cpu.read_reg(Register::D0), (-619i16) as u32);
            assert_eq!(cpu.read_reg(Register::A7), sp.wrapping_add(4));
            assert_eq!(bus.read_word(sp + 4), (-619i16) as u16);
            assert_eq!(
                crate::guest_call::CooperativeThread::capture(&cpu),
                expected_live
            );
            assert_eq!(cpu.capture_extended_context(), before_extended);
            assert_eq!(disp.guest_calls, before_calls);
            assert_eq!(disp.guest_calls.current_task(), application);
            assert_eq!(disp.guest_calls.critical_depth(), 1);
            assert_eq!(
                disp.guest_calls.cooperative_context(application),
                before_application
            );
            assert_eq!(
                disp.guest_calls.cooperative_context(worker),
                Some(worker_context)
            );
            assert!(!disp.guest_calls.has_pending_task_handoff());
        }
    }

    #[test]
    fn classic_yield_to_any_without_successor_returns_without_publishing_snapshot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = u32::MAX - 3;
        cpu.write_reg(Register::PC, 0x1234_5678);
        cpu.write_reg(Register::D3, 0x89ab_cdef);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0205);
        let mut expected_live = crate::guest_call::CooperativeThread::capture(&cpu);
        expected_live.d_regs[0] = 0;
        expected_live.a_regs[7] = 0;
        let before_extended = cpu.capture_extended_context();
        let before_application = disp
            .guest_calls
            .cooperative_context(ExecutionTaskId::APPLICATION);
        let before_calls = disp.guest_calls.clone();

        disp.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), 0);
        assert_eq!(
            crate::guest_call::CooperativeThread::capture(&cpu),
            expected_live
        );
        assert_eq!(cpu.capture_extended_context(), before_extended);
        assert_eq!(disp.guest_calls, before_calls);
        assert_eq!(
            disp.guest_calls.current_task(),
            ExecutionTaskId::APPLICATION
        );
        assert_eq!(
            disp.guest_calls
                .cooperative_context(ExecutionTaskId::APPLICATION),
            before_application
        );
        assert!(!disp.guest_calls.has_pending_task_handoff());
    }

    #[test]
    fn threaddispatch_getcurrentthread_reports_application_thread_and_consumes_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let current_thread = bus.alloc(4);
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, current_thread);
        bus.write_word(sp + 4, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0206);

        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(current_thread), 2);
        assert_eq!(bus.read_word(sp + 4), 0);
    }

    #[test]
    fn threaddispatch_thread_entry_return_delivers_result_and_resumes_application() {
        let (mut disp, mut cpu, mut bus) = setup();
        let new_sp = TEST_SP;
        let thread_made = bus.alloc(4);
        let thread_result = bus.alloc(4);

        cpu.write_reg(Register::A7, new_sp);
        bus.write_long(new_sp, thread_made);
        bus.write_long(new_sp + 4, thread_result);
        bus.write_long(new_sp + 8, 0x0000_0008);
        bus.write_long(new_sp + 12, 0x0000_1800);
        bus.write_long(new_sp + 16, 0x0065_4321);
        bus.write_long(new_sp + 20, 0x0012_3456);
        bus.write_long(new_sp + 24, 1);
        bus.write_word(new_sp + 28, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0E03);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let app_sp = new_sp + 28;
        cpu.write_reg(Register::PC, 0x000F_0000);
        bus.write_long(app_sp, 3);
        bus.write_word(app_sp + 4, 0xBEEF);
        cpu.write_reg(Register::D0, 0x0205);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        cpu.write_reg(Register::A0, 0xCAFE_BABE);
        cpu.write_reg(Register::D0, 0xFFFE);
        disp.dispatch_toolbox(true, 0x3F2, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(disp.guest_calls.current_task().thread_id(), 2);
        assert!(!disp
            .guest_calls
            .cooperative_context(ExecutionTaskId::from_thread_id(3))
            .is_some());
        assert_eq!(bus.read_long(thread_result), 0xCAFE_BABE);
        assert_eq!(cpu.read_reg(Register::PC), 0x000F_0000);
        assert_eq!(cpu.read_reg(Register::A7), app_sp + 4);
    }

    // DictionaryDispatch ($AA53)
    // Inside Macintosh: Text (1993), pp. 8-11, 8-21 to 8-24, 8-34.
    // InitializeDictionary is the public client call that creates the
    // internal B*-tree for a freshly created dictionary file. The 68K
    // Pascal wrapper pushes a 10-byte argument frame:
    //   FSSpecPtr (4) + maximumKeyLength (2) + keyAttributes (2) + script (2)
    #[test]
    fn dictionarydispatch_initialize_dictionary_selector_pops_ten_byte_frame_and_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let dict_spec = bus.alloc(64);

        cpu.write_reg(Register::D0, 0x0500);
        bus.write_long(sp, dict_spec);
        bus.write_word(sp + 4, 129);
        bus.write_word(sp + 6, 0x0010);
        bus.write_word(sp + 8, 0x0000);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x253, &mut cpu, &mut bus);
        assert!(result.is_some(), "DictionaryDispatch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "InitializeDictionary should return"
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
    }

    // Pack2 / DiskInit ($A9E9)
    // Inside Macintosh: Files (1992), pp. 5-15 to 5-21 and p. 5-24.
    #[test]
    fn pack2_dibadmount_selector_0000_returns_zero_and_pops_selector_plus_point_and_evtmessage() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x0000); // DIBadMount
        bus.write_word(sp + 2, 12); // where.v
        bus.write_word(sp + 4, 34); // where.h
        bus.write_long(sp + 6, 0x1122_3344); // evtMessage
        bus.write_word(sp + 10, 0xBEEF); // Integer result slot

        let result = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn pack2_diload_and_diunload_selectors_pop_selector_only_and_return_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x0002); // DILoad
        bus.write_word(sp + 2, 0xA55A); // sentinel after selector
        let sp_pre_load = cpu.read_reg(Register::A7);
        let load = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        let sp_post_load = cpu.read_reg(Register::A7);
        assert!(load.is_some());
        assert!(load.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(sp_post_load, sp_pre_load + 2);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(sp + 2), 0xA55A);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0004); // DIUnload
        bus.write_word(sp + 2, 0x5AA5); // sentinel after selector
        let sp_pre_unload = cpu.read_reg(Register::A7);
        let unload = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        let sp_post_unload = cpu.read_reg(Register::A7);
        assert!(unload.is_some());
        assert!(unload.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(sp_post_unload, sp_pre_unload + 2);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(sp + 2), 0x5AA5);
    }

    #[test]
    fn pack2_diformat_and_diverify_selectors_return_noerr_and_pop_drvnum_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x0006); // DIFormat
        bus.write_word(sp + 2, 7); // drvNum
        bus.write_word(sp + 4, 0xBEEF); // OSErr result slot
        let format = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(format.is_some());
        assert!(format.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0008); // DIVerify
        bus.write_word(sp + 2, 7); // drvNum
        bus.write_word(sp + 4, 0xBEEF); // OSErr result slot
        let verify = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(verify.is_some());
        assert!(verify.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn pack2_dizero_selector_000a_consumes_pascal_str255_by_value_and_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x000A); // DIZero
        bus.write_word(sp + 2, 3); // drvNum
        bus.write_byte(sp + 4, 5); // Str255 length
        bus.write_bytes(sp + 5, b"Disk0");
        bus.write_word(sp + 260, 0xBEEF); // OSErr result slot

        let result = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 260), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 260);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn pack2_selector_table_integrated_contract_covers_all_documented_selector_frames() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // DIBadMount: selector + Point + evtMessage + result slot.
        bus.write_word(sp, 0x0000);
        bus.write_word(sp + 2, 12);
        bus.write_word(sp + 4, 34);
        bus.write_long(sp + 6, 0x1122_3344);
        bus.write_word(sp + 10, 0xBEEF);
        let bad_mount = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(bad_mount.is_some());
        assert!(bad_mount.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);

        // DILoad / DIUnload: selector only.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0002);
        bus.write_word(sp + 2, 0xA55A);
        let load = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(load.is_some());
        assert!(load.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(sp + 2), 0xA55A);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0004);
        bus.write_word(sp + 2, 0x5AA5);
        let unload = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(unload.is_some());
        assert!(unload.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(sp + 2), 0x5AA5);

        // DIFormat / DIVerify: selector + drvNum + result slot.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0006);
        bus.write_word(sp + 2, 7);
        bus.write_word(sp + 4, 0xBEEF);
        let format = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(format.is_some());
        assert!(format.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x0008);
        bus.write_word(sp + 2, 7);
        bus.write_word(sp + 4, 0xBEEF);
        let verify = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(verify.is_some());
        assert!(verify.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        // DIZero: selector + drvNum + by-value Str255 + result slot.
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0x000A);
        bus.write_word(sp + 2, 3);
        bus.write_byte(sp + 4, 5);
        bus.write_bytes(sp + 5, b"Disk0");
        bus.write_word(sp + 260, 0xBEEF);
        let zero = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(zero.is_some());
        assert!(zero.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 260), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 260);
    }

    #[test]
    fn pack2_generated_routes_preserve_exact_stack_word_values() {
        assert_eq!(super::PACK2_OPERATION_ROUTES.len(), 6);
        assert!(super::PACK2_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0000, "DIBadMount"),
            (0x0002, "DILoad"),
            (0x0004, "DIUnload"),
            (0x0006, "DIFormat"),
            (0x0008, "DIVerify"),
            (0x000A, "DIZero"),
        ] {
            let route =
                super::pack2_operation_route(0xA9E9, selector).expect("Pack2 operation route");
            assert_eq!(route.routine_name, routine_name);
            assert_eq!(
                route.operation_id,
                format!("selector-operation:_Pack2:0x{selector:04X}:stack-word-via-d0-moveq-immediate:16")
            );
        }

        for (trap_word, selector) in [
            (0xA8E9, 0x0000),
            (0xADE9, 0x0000), // same slot with a different raw trap form
            (0xA9E8, 0x0006),
            (0xA9E5, 0x0000), // InitPack is a distinct trap
            (0xA9E9, 0x0001), // unassigned gap
            (0xA9E9, 0x0003), // unassigned gap
            (0xA9E9, 0x0005), // unassigned gap
            (0xA9E9, 0x0007), // unassigned gap
            (0xA9E9, 0x0009), // unassigned gap
            (0xA9E9, 0x000B), // unassigned gap
            (0xA9E9, 0x000C), // later UI 3.4 DIXFormat remains outside this 1992 generated slice
            (0xA9E9, 0x000E), // later UI 3.4 DIXZero remains outside this 1992 generated slice
            (0xA9E9, 0x0010), // later UI 3.4 DIReformat remains outside this 1992 generated slice
            (0xA9E9, 0x000F), // odd adjacent value
            (0xA9E9, 0x7000), // MOVEQ opcode setup spelling
            (0xA9E9, 0x3F00), // MOVE.W D0,-(SP) glue opcode spelling
            (0xA9E9, 0x0200), // byte-swapped DILoad selector
            (0xA9E9, 0x0600), // byte-swapped DIFormat selector
            (0xA9E9, 0x0A00), // byte-swapped DIZero selector
        ] {
            assert!(super::pack2_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack2_records_stack_word_identity_without_changing_behavior_and_clears_stale_identity() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // 1. Exact A9E9 records known stack selector without changing existing frame/result
        disp.current_trap_word = 0xA9E9;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::A0, 0x1234_0000);
        cpu.write_reg(Register::A1, 0x1234_0001);
        cpu.write_reg(Register::D0, 0x1234_5678);
        cpu.write_reg(Register::D1, 0x1234_0002);
        bus.write_word(sp, 0x0006); // DIFormat
        bus.write_word(sp + 2, 7); // drvNum
        bus.write_word(sp + 4, 0xBEEF); // OSErr result slot
        bus.write_long(sp + 6, 0xCAFE_BABE); // adjacent stack poison

        let result = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(result.expect("Pack2 DIFormat arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack2:0x0006:stack-word-via-d0-moveq-immediate:16")
        );
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A0), 0x1234_0000);
        assert_eq!(cpu.read_reg(Register::A1), 0x1234_0001);
        assert_eq!(cpu.read_reg(Register::D1), 0x1234_0002);
        assert_eq!(bus.read_long(sp + 6), 0xCAFE_BABE);

        // 2. Pre-seeded stale identity is actively cleared for an unassigned gap
        disp.current_selector_operation = Some("stale-identity");
        disp.current_trap_word = 0xA9E9;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x1234_5678);
        bus.write_word(sp, 0x0005); // unassigned gap
        bus.write_bytes(sp + 2, &[0xA5; 16]); // adjacent stack poison

        let result = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(result.expect("Pack2 unknown selector fallback").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_bytes(sp + 2, 16), vec![0xA5; 16]);

        // 3. Pre-seeded stale identity is actively cleared for a wrong raw trap form
        disp.current_selector_operation = Some("stale-identity");
        disp.current_trap_word = 0xADE9; // wrong raw trap form for canonical slot 0x1E9
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x1234_5678);
        bus.write_word(sp, 0x0006); // DIFormat selector
        bus.write_word(sp + 2, 7); // drvNum
        bus.write_word(sp + 4, 0xBEEF); // OSErr result slot
        bus.write_long(sp + 6, 0xCAFE_BABE);

        let result = disp.dispatch_toolbox(true, 0x1E9, &mut cpu, &mut bus);
        assert!(result.expect("Pack2 arm under alternate trap form").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_long(sp + 6), 0xCAFE_BABE);
    }

    // Pack3 / Standard File ($A9EA) — StandardGetFile selector $0006
    // IM:Files 1992 pp. 3-50 and 3-61: cancel sets sfGood to FALSE.
    #[test]
    fn standard_get_file_cancel_sets_sfgood_false_and_pops_16_bytes() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320000u32;
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_byte(reply_ptr + 1, 0xAB);
        bus.write_word(sp, 0x0006); // StandardGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 0);
        assert_eq!(bus.read_byte(reply_ptr + 1), 0xAB);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardGetFile selector $0006
    // IM:Files 1992 pp. 3-42 and 3-50: Open returns sfType and an FSSpec.
    #[test]
    fn standard_get_file_env_selection_returns_fsspec_and_pops_16_bytes() {
        let _env = set_standard_file_env("Tom Fighter Paris");
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320080u32;
        let pilots_dir = disp.ensure_vfs_directory("Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = pilots_dir);
        disp.vfs_rsrc
            .insert("Pilots/Tom Fighter Paris".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("Pilots/Tom Fighter Paris", *b"PIL ", *b"EVO!", 0x4000);
        bus.write_word(sp, 0x0006); // StandardGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_byte(reply_ptr + 1), 0);
        assert_eq!(bus.read_long(reply_ptr + 2), u32::from_be_bytes(*b"PIL "));
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_long(reply_ptr + 8), pilots_dir);
        assert_eq!(
            bus.read_pstring(reply_ptr + 12),
            b"Tom Fighter Paris".to_vec()
        );
        assert_eq!(bus.read_word(reply_ptr + 78), 0x4000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardGetFile selector $0006.
    // IM:Files 1992 p. 3-50 and IM:VI 26-22: a positive numTypes/typeList
    // filters the Open dialog's display list by Finder file type.
    #[test]
    fn standard_get_file_auto_selects_matching_current_directory_type() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320500u32;
        let type_list_ptr = 0x320600u32;
        let app_dir = disp.ensure_vfs_directory("Data Folder");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = app_dir);
        disp.vfs
            .insert("Data Folder/Selected".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("Data Folder/Selected", *b"DATA", *b"TEST", 0x4000);
        disp.vfs
            .insert("Other Folder/Selected".to_string(), vec![4, 5, 6]);
        disp.set_vfs_entry_metadata("Other Folder/Selected", *b"DATA", *b"TEST", 0x8000);
        bus.write_long(type_list_ptr, u32::from_be_bytes(*b"DATA"));
        bus.write_word(sp, 0x0006); // StandardGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, type_list_ptr); // typeList
        bus.write_word(sp + 10, 1); // numTypes
        bus.write_long(sp + 12, 0); // fileFilter

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_long(reply_ptr + 2), u32::from_be_bytes(*b"DATA"));
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_long(reply_ptr + 8), app_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Selected".to_vec());
        assert_eq!(bus.read_word(reply_ptr + 78), 0x4000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardGetFile selector $0006.
    // IM:Files 1992 p. 3-50: numTypes = -1 lets all file types pass.
    #[test]
    fn standard_get_file_minus_one_types_auto_selects_single_current_directory_file() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320900u32;
        let app_dir = disp.ensure_vfs_directory("Data Folder");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = app_dir);
        disp.vfs
            .insert("Data Folder/Only Choice".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("Data Folder/Only Choice", *b"Flux", *b"Geek", 0x2000);
        disp.vfs
            .insert("Other Folder/Only Choice".to_string(), vec![4, 5, 6]);
        disp.set_vfs_entry_metadata("Other Folder/Only Choice", *b"DATA", *b"TEST", 0);
        bus.write_word(sp, 0x0006); // StandardGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, 0); // typeList is ignored for numTypes = -1
        bus.write_word(sp + 10, (-1i16) as u16); // numTypes = all file types
        bus.write_long(sp + 12, 0); // fileFilter

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_long(reply_ptr + 2), u32::from_be_bytes(*b"Flux"));
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_long(reply_ptr + 8), app_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Only Choice".to_vec());
        assert_eq!(bus.read_word(reply_ptr + 78), 0x2000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardGetFile selector $0006.
    // Without real UI, all-types mode stays canceled when more than one
    // current-directory file would be displayed.
    #[test]
    fn standard_get_file_minus_one_types_cancels_ambiguous_current_directory_files() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320A00u32;
        let app_dir = disp.ensure_vfs_directory("Data Folder");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = app_dir);
        disp.vfs
            .insert("Data Folder/First".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("Data Folder/First", *b"Flux", *b"Geek", 0);
        disp.vfs
            .insert("Data Folder/Second".to_string(), vec![4, 5, 6]);
        disp.set_vfs_entry_metadata("Data Folder/Second", *b"DATA", *b"TEST", 0);
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_word(sp, 0x0006); // StandardGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, 0); // typeList
        bus.write_word(sp + 10, (-1i16) as u16); // numTypes = all file types
        bus.write_long(sp + 12, 0); // fileFilter

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardGetFile selector $0006.
    // IM:VI 26-22: numTypes = 0 filters out all files.
    #[test]
    fn standard_get_file_zero_types_still_cancels() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320700u32;
        let type_list_ptr = 0x320800u32;
        let app_dir = disp.ensure_vfs_directory("Data Folder");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = app_dir);
        disp.vfs
            .insert("Data Folder/Selected".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("Data Folder/Selected", *b"Flux", *b"Geek", 0);
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_long(type_list_ptr, u32::from_be_bytes(*b"Flux"));
        bus.write_word(sp, 0x0006); // StandardGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, type_list_ptr); // typeList
        bus.write_word(sp + 10, 0); // numTypes
        bus.write_long(sp + 12, 0); // fileFilter

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardGetFile selector $0006.
    // The retained GUI path must let browser users select restored pilot
    // files rather than canceling silently.
    #[test]
    fn standard_get_file_gui_tracking_opens_selected_pilots_file() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x320B00u32;
        let type_list_ptr = 0x320C00u32;
        let app_dir = disp.ensure_vfs_directory("EV Override 1.0.1");
        let pilots_dir = disp.ensure_vfs_directory("EV Override 1.0.1/Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = app_dir);
        disp.yield_for_ui = true;
        disp.vfs_rsrc
            .insert("EV Override 1.0.1/Pilots/Last Pilot".to_string(), vec![1]);
        disp.set_vfs_entry_metadata("EV Override 1.0.1/Pilots/Last Pilot", *b"PIL ", *b"EVO!", 0);
        disp.vfs_rsrc.insert(
            "EV Override 1.0.1/Pilots/Rick Hardslab".to_string(),
            vec![2, 3, 4],
        );
        disp.set_vfs_entry_metadata(
            "EV Override 1.0.1/Pilots/Rick Hardslab",
            *b"PIL ",
            *b"EVO!",
            0x4000,
        );
        bus.write_long(type_list_ptr, u32::from_be_bytes(*b"PIL "));
        bus.write_word(sp, 0x0006); // StandardGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, type_list_ptr); // typeList
        bus.write_word(sp + 10, 1); // numTypes
        bus.write_long(sp + 12, 0); // fileFilter

        let start = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(start.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert!(disp.is_standard_file_get_tracking());

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D, // Return opens the Pilots folder
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        let enter_pilots = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(enter_pilots.unwrap().is_ok());
        assert!(disp.is_standard_file_get_tracking());

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_7D1F, // ArrowDown
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        let select_second = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(select_second.unwrap().is_ok());
        assert!(disp.is_standard_file_get_tracking());

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D, // Return
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        let open = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(open.unwrap().is_ok());

        assert!(!disp.is_standard_file_get_tracking());
        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_long(reply_ptr + 2), u32::from_be_bytes(*b"PIL "));
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_long(reply_ptr + 8), pilots_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Rick Hardslab".to_vec());
        assert_eq!(bus.read_word(reply_ptr + 78), 0x4000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn standard_get_file_gui_empty_directory_uses_complete_classic_model() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x321000u32;
        let empty_dir = disp.ensure_vfs_directory("Empty Saves");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = empty_dir);
        disp.yield_for_ui = true;
        bus.write_word(sp, 0x0006);
        bus.write_long(sp + 2, reply_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, (-1i16) as u16);
        bus.write_long(sp + 12, 0);

        let start = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(start.unwrap().is_ok());

        let tracking = disp
            .standard_file_get_tracking
            .as_ref()
            .expect("Open dialog should remain active");
        let items = TrapDispatcher::standard_file_get_dialog_items(tracking);
        assert!(tracking.entries.is_empty());
        assert_eq!(
            tracking.bounds.2 - tracking.bounds.0,
            STANDARD_FILE_GET_DIALOG_HEIGHT
        );
        assert_eq!(
            tracking.bounds.3 - tracking.bounds.1,
            STANDARD_FILE_GET_DIALOG_WIDTH
        );
        assert!(items
            .iter()
            .any(|item| item.text == "Desktop" && item.item_type == 4));
        assert!(items
            .iter()
            .any(|item| item.text == "Cancel" && item.item_type == 4));
        assert!(items
            .iter()
            .any(|item| item.text == "Eject" && item.item_type == 0x84));
        assert!(items
            .iter()
            .any(|item| item.text == "Open" && item.item_type == 0x84));
        assert!(items
            .iter()
            .any(|item| item.rect == STANDARD_FILE_GET_VOLUME_RECT));
        assert!(items
            .iter()
            .any(|item| item.rect == STANDARD_FILE_GET_LIST_RECT));
        assert!(items
            .iter()
            .any(|item| item.rect == STANDARD_FILE_GET_SCROLL_RECT));
        assert!(!items.iter().any(|item| item.text == "No matching files."));
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_byte(reply_ptr), 0);
    }

    #[test]
    fn standard_get_file_gui_navigates_folder_then_cancels_with_original_abi() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x321100u32;
        let app_dir = disp.ensure_vfs_directory("Game");
        let saves_dir = disp.ensure_vfs_directory("Game/Saves");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = app_dir);
        disp.yield_for_ui = true;
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_word(sp, 0x0006);
        bus.write_long(sp + 2, reply_ptr);
        bus.write_long(sp + 6, 0);
        bus.write_word(sp + 10, (-1i16) as u16);
        bus.write_long(sp + 12, 0);

        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            disp.standard_file_get_tracking
                .as_ref()
                .unwrap()
                .current_dir_id,
            saves_dir
        );
        assert_eq!(cpu.read_reg(Register::A7), sp);

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_351B,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(!disp.is_standard_file_get_tracking());
        assert_eq!(bus.read_byte(reply_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
    }

    // Pack3 / Standard File ($A9EA) — SFGetFile selector $0002
    // IM:Files 1992 pp. 3-53 and 3-61; IM:I I-523..I-526.
    #[test]
    fn sf_get_file_cancel_sets_good_false_and_pops_28_bytes() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320100u32;
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_word(sp, 0x0002); // SFGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — SFGetFile selector $0002.
    // IM:Files 1992 p. 1-12: old SFReply.vRefNum is a WDRefNum.
    #[test]
    fn sf_get_file_env_selection_returns_working_directory_refnum_and_pops_28_bytes() {
        let _env = set_standard_file_env("Pilots/Tom Fighter Paris");
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320140u32;
        let pilots_dir = disp.ensure_vfs_directory("Pilots");
        disp.vfs
            .insert("Pilots/Tom Fighter Paris".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("Pilots/Tom Fighter Paris", *b"PIL ", *b"EVO!", 0);
        bus.write_word(sp, 0x0002); // SFGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let wd_ref = bus.read_word(reply_ptr + 6) as i16;
        let wd_info = disp
            .working_directory_info(wd_ref)
            .expect("SFGetFile should return a WDRefNum for the selected file directory");
        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_byte(reply_ptr + 1), 0);
        assert_eq!(bus.read_long(reply_ptr + 2), u32::from_be_bytes(*b"PIL "));
        assert_eq!(wd_info.dir_id, pilots_dir);
        assert_eq!(bus.read_word(reply_ptr + 8), 0);
        assert_eq!(
            bus.read_pstring(reply_ptr + 10),
            b"Tom Fighter Paris".to_vec()
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — SFGetFile selector $0002.
    // Old SFReply GUI selections return a WDRefNum for the selected file's
    // directory, matching the headless path.
    #[test]
    fn sf_get_file_gui_tracking_opens_selected_pilots_file_with_wdrefnum() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x320D00u32;
        let type_list_ptr = 0x320E00u32;
        let app_dir = disp.ensure_vfs_directory("Escape Velocity 1.0.5 ƒ");
        let pilots_dir = disp.ensure_vfs_directory("Escape Velocity 1.0.5 ƒ/Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = app_dir);
        disp.yield_for_ui = true;
        disp.vfs_rsrc.insert(
            "Escape Velocity 1.0.5 ƒ/Pilots/Ace".to_string(),
            vec![1, 2, 3],
        );
        disp.set_vfs_entry_metadata("Escape Velocity 1.0.5 ƒ/Pilots/Ace", *b"PIL ", *b"EV  ", 0);
        bus.write_long(type_list_ptr, u32::from_be_bytes(*b"PIL "));
        bus.write_word(sp, 0x0002); // SFGetFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 10, type_list_ptr); // typeList
        bus.write_word(sp + 14, 1); // numTypes
        bus.write_long(sp + 18, 0); // fileFilter
        bus.write_word(sp + 22, 84); // where.v
        bus.write_word(sp + 24, 222); // where.h

        let start = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(start.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert!(disp.is_standard_file_get_tracking());
        assert_eq!(
            disp.standard_file_get_tracking.as_ref().unwrap().bounds,
            (84, 222, 262, 578)
        );

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D, // Return opens the Pilots folder
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        let enter_pilots = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(enter_pilots.unwrap().is_ok());
        assert!(disp.is_standard_file_get_tracking());

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        let open = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(open.unwrap().is_ok());

        let wd_ref = bus.read_word(reply_ptr + 6) as i16;
        let wd_info = disp
            .working_directory_info(wd_ref)
            .expect("SFGetFile GUI path should return a WDRefNum");
        assert!(!disp.is_standard_file_get_tracking());
        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_long(reply_ptr + 2), u32::from_be_bytes(*b"PIL "));
        assert_eq!(wd_info.dir_id, pilots_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 10), b"Ace".to_vec());
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardPutFile selector $0005
    // Non-interactive runtimes accept the default filename in the current dir.
    #[test]
    fn standard_put_file_returns_default_name_fsspec_and_pops_14_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320180u32;
        let default_name_ptr = 0x320300u32;
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = 18);
        disp.app_wd_refnum.with_mut(|app_ref_num| {
            *app_ref_num = crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num();
        });
        bus.write_pstring(default_name_ptr, b"Twilight Save");
        bus.write_word(sp, 0x0005); // StandardPutFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, default_name_ptr); // defaultName

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_byte(reply_ptr + 1), 0);
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_long(reply_ptr + 8), 18);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Twilight Save".to_vec());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardPutFile selector $0005
    // IM:Files 1992 p. 3-61: sfReplacing is TRUE after the user verifies
    // a save name that duplicates an existing file.
    #[test]
    fn standard_put_file_sets_replacing_for_existing_current_directory_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x3201C0u32;
        let default_name_ptr = 0x320340u32;
        let pilots_dir = disp.ensure_vfs_directory("Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = pilots_dir);
        disp.app_wd_refnum.with_mut(|app_ref_num| {
            *app_ref_num = crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num();
        });
        disp.vfs_rsrc
            .insert("Pilots/Existing Pilot".to_string(), vec![1, 2, 3]);
        disp.set_vfs_entry_metadata("Pilots/Existing Pilot", *b"PIL ", *b"EVO!", 0);
        bus.write_pstring(default_name_ptr, b"Existing Pilot");
        bus.write_word(sp, 0x0005); // StandardPutFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, default_name_ptr); // defaultName

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_byte(reply_ptr + 1), 1);
        assert_eq!(bus.read_long(reply_ptr + 8), pilots_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Existing Pilot".to_vec());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — StandardPutFile selector $0005
    // IM:Files 1992 pp. 1-43 and 3-45: StandardPutFile presents the save
    // dialog and returns the user's chosen FSSpec after Save.
    #[test]
    fn standard_put_file_gui_tracking_accepts_typed_name_on_return() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x3201C0u32;
        let prompt_ptr = 0x320300u32;
        let default_name_ptr = 0x320340u32;
        let pilots_dir = disp.ensure_vfs_directory("Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = pilots_dir);
        disp.app_wd_refnum.with_mut(|app_ref_num| {
            *app_ref_num = crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num();
        });
        disp.yield_for_ui = true;

        bus.write_pstring(prompt_ptr, b"Create New Player\xC9");
        bus.write_pstring(default_name_ptr, b"Untitled");
        bus.write_word(sp, 0x0005); // StandardPutFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, default_name_ptr); // defaultName
        bus.write_long(sp + 10, prompt_ptr); // prompt

        let start = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(start.is_some());
        assert!(start.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert!(disp.is_standard_file_put_tracking());
        assert_eq!(
            disp.standard_file_put_tracking.as_ref().unwrap().prompt,
            "Create New Player…"
        );

        for byte in b"Rick" {
            disp.event_queue.push_back(QueuedEvent {
                what: 3,
                message: (u32::from(*byte) << 8) | u32::from(*byte),
                when: 0,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            });
            let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
            assert!(result.unwrap().is_ok());
            assert!(disp.is_standard_file_put_tracking());
        }

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        let accept = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(accept.unwrap().is_ok());
        assert!(!disp.is_standard_file_put_tracking());

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_byte(reply_ptr + 1), 0);
        assert_eq!(bus.read_long(reply_ptr + 8), pilots_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Rick".to_vec());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) + HighLevelFSDispatch ($AA52)
    // IM:Files 1992 pp. 3-45 and 2-329: StandardPutFile returns an FSSpec
    // suitable for subsequent File Manager calls such as FSpCreate.
    #[test]
    fn standard_put_file_gui_reply_can_drive_fspcreate_save_file() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x320600u32;
        let prompt_ptr = 0x320700u32;
        let default_name_ptr = 0x320740u32;
        let pilots_dir = disp.ensure_vfs_directory("Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = pilots_dir);
        disp.app_wd_refnum.with_mut(|app_ref_num| {
            *app_ref_num = crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num();
        });
        disp.yield_for_ui = true;

        bus.write_pstring(prompt_ptr, b"Pilot file:");
        bus.write_pstring(default_name_ptr, b"Untitled");
        bus.write_word(sp, 0x0005); // StandardPutFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 6, default_name_ptr); // defaultName
        bus.write_long(sp + 10, prompt_ptr); // prompt

        let start = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(start.unwrap().is_ok());
        assert!(disp.is_standard_file_put_tracking());

        for byte in b"Rick Hardslab" {
            disp.event_queue.push_back(QueuedEvent {
                what: 3,
                message: (u32::from(*byte) << 8) | u32::from(*byte),
                when: 0,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            });
            assert!(disp
                .dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
                .unwrap()
                .is_ok());
        }
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        assert!(disp
            .dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Rick Hardslab".to_vec());

        let create_sp = cpu.read_reg(Register::A7);
        let spec_ptr = reply_ptr + 6;
        bus.write_word(create_sp, 0); // scriptTag
        bus.write_long(create_sp + 2, u32::from_be_bytes(*b"PIL "));
        bus.write_long(create_sp + 6, u32::from_be_bytes(*b"EVO!"));
        bus.write_long(create_sp + 10, spec_ptr);
        cpu.write_reg(Register::D0, 4); // FSpCreate selector

        let create = disp
            .dispatch_resource(true, 0x252, &mut cpu, &mut bus)
            .expect("HighLevelFSDispatch should be handled");
        assert!(create.is_ok());

        assert_eq!(cpu.read_reg(Register::A7), create_sp + 14);
        assert_eq!(bus.read_word(create_sp + 14), 0);
        assert!(disp.vfs.contains_key("Pilots/Rick Hardslab"));
        assert!(disp.vfs_rsrc.contains_key("Pilots/Rick Hardslab"));
        let metadata = disp
            .vfs_file_metadata("Pilots/Rick Hardslab")
            .expect("created pilot metadata");
        assert_eq!(metadata.file_type, u32::from_be_bytes(*b"PIL "));
        assert_eq!(metadata.creator, u32::from_be_bytes(*b"EVO!"));
    }

    // Pack3 / Standard File ($A9EA) — SFPutFile selector $0001.
    // IM:Files 1992 pp. 3-44 and 3-48: the Save dialog selects both a
    // filename and a directory. Mounted images stay selectable for browsing,
    // but their hardware lock disables acceptance.
    #[test]
    fn sf_put_file_gui_locked_volume_requires_navigation_and_persists_cancelled_directory() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x321200u32;
        let original_name_ptr = 0x321300u32;
        let locked_vref =
            disp.mount_vfs_volume("Pathways Disk", 0, 1, 1024, 512, 512, 900, 0, 0, 0, 0, 0, 0);
        let locked_dir = disp
            .vfs_volume_for_ref_num(locked_vref)
            .expect("mounted volume")
            .root_dir_id;
        let saves_dir = disp.ensure_vfs_directory("Saved Games");
        let locked_wd = disp
            .open_working_directory(locked_vref, locked_dir, 0)
            .expect("mounted-volume working directory");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = locked_dir);
        disp.app_wd_refnum
            .with_mut(|app_ref_num| *app_ref_num = locked_wd);
        disp.yield_for_ui = true;
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_pstring(original_name_ptr, b"Pathways Save");
        bus.write_word(sp, 0x0001);
        bus.write_long(sp + 2, reply_ptr);
        bus.write_long(sp + 10, original_name_ptr);

        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let tracking = disp.standard_file_put_tracking.as_ref().unwrap();
        assert_eq!(tracking.current_dir_id, locked_dir);
        assert_eq!(tracking.name, "Pathways Save");
        let (_, _, label, writable) =
            disp.standard_file_put_directory_location(tracking.current_dir_id);
        assert_eq!(label, "Pathways Disk");
        assert!(!writable);
        let items = TrapDispatcher::standard_file_put_dialog_items(tracking, &label, writable);
        assert_eq!(items[STANDARD_FILE_NAME_ITEM as usize - 1].item_type, 16);
        assert_eq!(
            items[STANDARD_FILE_NAME_ITEM as usize - 1].text,
            "Pathways Save"
        );
        assert!(items
            .iter()
            .any(|item| item.text == "Save" && item.item_type == 0x84));

        // Return and a direct click on the disabled Save button are no-ops.
        for event in [
            QueuedEvent {
                what: 3,
                message: 0x0000_240D,
                when: 0,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            },
            QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: tracking.bounds.0
                    + (STANDARD_FILE_SAVE_RECT.0 + STANDARD_FILE_SAVE_RECT.2) / 2,
                where_h: tracking.bounds.1
                    + (STANDARD_FILE_SAVE_RECT.1 + STANDARD_FILE_SAVE_RECT.3) / 2,
                modifiers: 0,
            },
        ] {
            disp.event_queue.push_back(event);
            disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert!(disp.is_standard_file_put_tracking());
            assert_eq!(bus.read_byte(reply_ptr), 0xFF);
            assert_eq!(cpu.read_reg(Register::A7), sp);
        }

        let bounds = disp.standard_file_put_tracking.as_ref().unwrap().bounds;
        disp.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: bounds.0
                + (STANDARD_FILE_PUT_DESKTOP_RECT.0 + STANDARD_FILE_PUT_DESKTOP_RECT.2) / 2,
            where_h: bounds.1
                + (STANDARD_FILE_PUT_DESKTOP_RECT.1 + STANDARD_FILE_PUT_DESKTOP_RECT.3) / 2,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let tracking = disp.standard_file_put_tracking.as_ref().unwrap();
        assert_eq!(tracking.current_dir_id, 2);
        assert_eq!(tracking.name, "Pathways Save");
        assert_eq!(
            *disp.default_dir_id, locked_dir,
            "browsing is not persistent"
        );

        let saves_index = tracking
            .entries
            .iter()
            .position(|entry| entry.dir_id == saves_dir)
            .expect("Saved Games directory in Desktop list");
        let row_v = bounds.0
            + STANDARD_FILE_PUT_LIST_RECT.0
            + 2
            + saves_index as i16 * super::STANDARD_FILE_GET_ROW_HEIGHT
            + 5;
        let row_h = bounds.1 + STANDARD_FILE_PUT_LIST_RECT.1 + 8;
        for _ in 0..2 {
            disp.event_queue.push_back(QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: row_v,
                where_h: row_h,
                modifiers: 0,
            });
            disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
        }
        let tracking = disp.standard_file_put_tracking.as_ref().unwrap();
        assert_eq!(tracking.current_dir_id, saves_dir);
        assert_eq!(tracking.name, "Pathways Save");
        assert_eq!(
            *disp.default_dir_id, locked_dir,
            "navigation is not persistent"
        );

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_351B,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert!(!disp.is_standard_file_put_tracking());
        assert_eq!(bus.read_byte(reply_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
        assert_eq!(*disp.default_dir_id, saves_dir);
        assert_eq!(bus.read_long(addr::CUR_DIR_STORE), saves_dir);
        assert_eq!(
            bus.read_word(addr::SF_SAVE_DISK),
            (-super::super::dispatch::BOOT_VOLUME_REF_NUM) as u16
        );
        let persisted_wd = *disp.app_wd_refnum;
        assert_eq!(
            disp.working_directory_info(persisted_wd)
                .expect("persisted writable directory")
                .dir_id,
            saves_dir
        );
    }

    #[test]
    fn sf_put_file_gui_navigation_returns_selected_directory_wdref_for_pbcreate() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x321400u32;
        let original_name_ptr = 0x321500u32;
        let game_dir = disp.ensure_vfs_directory("Game");
        let saves_dir = disp.ensure_vfs_directory("Game/Saves");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = game_dir);
        disp.yield_for_ui = true;
        bus.write_pstring(original_name_ptr, b"New Save");
        bus.write_word(sp, 0x0001);
        bus.write_long(sp + 2, reply_ptr);
        bus.write_long(sp + 10, original_name_ptr);

        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let bounds = disp.standard_file_put_tracking.as_ref().unwrap().bounds;
        let row_v = bounds.0 + STANDARD_FILE_PUT_LIST_RECT.0 + 7;
        let row_h = bounds.1 + STANDARD_FILE_PUT_LIST_RECT.1 + 8;
        disp.event_queue.push_back(QueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: row_v,
            where_h: row_h,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_7D1F,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0x0100,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            disp.standard_file_put_tracking
                .as_ref()
                .unwrap()
                .current_dir_id,
            saves_dir
        );
        // The filename remains the active editable item after navigation.
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_5858,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(disp.standard_file_put_tracking.as_ref().unwrap().name, "X");
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let wd_ref = bus.read_word(reply_ptr + 6) as i16;
        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_pstring(reply_ptr + 10), b"X");
        assert_eq!(
            disp.working_directory_info(wd_ref)
                .expect("selected directory WDRefNum")
                .dir_id,
            saves_dir
        );

        let pb = 0x321600u32;
        let name_ptr = 0x321700u32;
        cpu.write_reg(Register::A0, pb);
        bus.write_long(pb + 18, name_ptr);
        bus.write_word(pb + 22, wd_ref as u16);
        bus.write_pstring(name_ptr, b"X");
        disp.dispatch_resource(false, 0x08, &mut cpu, &mut bus)
            .expect("PBCreate arm")
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, 0);
        assert!(disp.vfs.contains_key("Game/Saves/X"));
    }

    #[test]
    fn standard_put_file_gui_navigation_returns_modern_parent_and_seeds_next_dialog() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x321800u32;
        let default_name_ptr = 0x321900u32;
        let game_dir = disp.ensure_vfs_directory("Modern Game");
        let saves_dir = disp.ensure_vfs_directory("Modern Game/Saves");
        disp.vfs
            .insert("Modern Game/Saves/Modern Save".to_string(), vec![1]);
        disp.set_vfs_entry_metadata("Modern Game/Saves/Modern Save", *b"SAVE", *b"TEST", 0);
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = game_dir);
        disp.yield_for_ui = true;
        bus.write_pstring(default_name_ptr, b"Modern Save");
        bus.write_word(sp, 0x0005);
        bus.write_long(sp + 2, reply_ptr);
        bus.write_long(sp + 6, default_name_ptr);

        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.standard_file_put_tracking.as_mut().unwrap().selected = Some(0);
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_7D1F,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0x0100,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_byte(reply_ptr + 1), 1);
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_long(reply_ptr + 8), saves_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 12), b"Modern Save");

        cpu.write_reg(Register::A7, sp);
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_word(sp, 0x0005);
        bus.write_long(sp + 2, reply_ptr);
        bus.write_long(sp + 6, default_name_ptr);
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            disp.standard_file_put_tracking
                .as_ref()
                .unwrap()
                .current_dir_id,
            saves_dir
        );
        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_351B,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_byte(reply_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
    }

    #[test]
    fn sf_put_file_headless_falls_back_from_locked_volume_to_writable_boot_root() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x321A00u32;
        let original_name_ptr = 0x321B00u32;
        let volume_ref =
            disp.mount_vfs_volume("Locked Game", 0, 1, 1024, 512, 512, 900, 0, 0, 0, 0, 0, 0);
        let volume_root = disp
            .vfs_volume_for_ref_num(volume_ref)
            .expect("mounted volume")
            .root_dir_id;
        let mounted_wd = disp
            .open_working_directory(volume_ref, volume_root, 0)
            .expect("mounted-volume WDRefNum");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = volume_root);
        disp.app_wd_refnum
            .with_mut(|app_ref_num| *app_ref_num = mounted_wd);
        bus.write_pstring(original_name_ptr, b"Fallback Save");
        bus.write_word(sp, 0x0001);
        bus.write_long(sp + 2, reply_ptr);
        bus.write_long(sp + 10, original_name_ptr);

        disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_pstring(reply_ptr + 10), b"Fallback Save");
        assert_eq!(*disp.default_dir_id, 2);
        assert_eq!(bus.read_long(addr::CUR_DIR_STORE), 2);
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
    }

    // Pack3 / Standard File ($A9EA) — SFPutFile selector $0001
    // Old SFReply uses the original name and working-directory refnum.
    #[test]
    fn sf_put_file_returns_original_name_and_pops_22_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320380u32;
        let original_name_ptr = 0x320500u32;
        disp.app_wd_refnum.with_mut(|app_ref_num| {
            *app_ref_num = crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num();
        });
        bus.write_pstring(original_name_ptr, b"Old Save");
        bus.write_word(sp, 0x0001); // SFPutFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 10, original_name_ptr); // origName

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(bus.read_byte(reply_ptr + 1), 0);
        assert_eq!(
            bus.read_word(reply_ptr + 6),
            crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num_u16()
        );
        assert_eq!(bus.read_pstring(reply_ptr + 10), b"Old Save".to_vec());
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — SFPutFile selector $0001.
    // Files 1992 p. 1-12: SFReply.vRefNum is a WDRefNum encoding the
    // volume and parent directory of the selected file.
    #[test]
    fn sf_put_file_returns_working_directory_refnum_for_current_save_directory() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let reply_ptr = 0x320580u32;
        let original_name_ptr = 0x320680u32;
        let pilots_dir = disp.ensure_vfs_directory("Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = pilots_dir);
        disp.app_wd_refnum.with_mut(|app_ref_num| {
            *app_ref_num = crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num();
        });
        bus.write_pstring(original_name_ptr, b"Old Pilot");
        bus.write_word(sp, 0x0001); // SFPutFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 10, original_name_ptr); // origName

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let wd_ref = bus.read_word(reply_ptr + 6) as i16;
        let wd_info = disp
            .working_directory_info(wd_ref)
            .expect("SFPutFile should return a WDRefNum for the save directory");
        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(wd_info.dir_id, pilots_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 10), b"Old Pilot".to_vec());
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — SFPutFile selector $0001.
    // The retained GUI path must return the same selected-directory WDRefNum
    // as the headless path after the user accepts the save.
    #[test]
    fn sf_put_file_gui_tracking_returns_working_directory_refnum() {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc(800 * 600);
        disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
        let sp = TEST_SP;
        let reply_ptr = 0x320780u32;
        let original_name_ptr = 0x320880u32;
        let prompt_ptr = 0x3208C0u32;
        let pilots_dir = disp.ensure_vfs_directory("Pilots");
        disp.default_dir_id
            .with_mut(|default_dir_id| *default_dir_id = pilots_dir);
        disp.app_wd_refnum.with_mut(|app_ref_num| {
            *app_ref_num = crate::trap::dispatch::TrapDispatcher::boot_volume_ref_num();
        });
        disp.yield_for_ui = true;
        bus.write_pstring(original_name_ptr, b"Old Pilot");
        bus.write_pstring(prompt_ptr, b"Pilot file:");
        bus.write_word(sp, 0x0001); // SFPutFile selector
        bus.write_long(sp + 2, reply_ptr); // VAR reply
        bus.write_long(sp + 10, original_name_ptr); // origName
        bus.write_long(sp + 14, prompt_ptr); // prompt

        let start = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(start.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert!(disp.is_standard_file_put_tracking());

        disp.event_queue.push_back(QueuedEvent {
            what: 3,
            message: 0x0000_240D,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
        let accept = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(accept.unwrap().is_ok());

        let wd_ref = bus.read_word(reply_ptr + 6) as i16;
        let wd_info = disp
            .working_directory_info(wd_ref)
            .expect("SFPutFile GUI path should return a WDRefNum for the save directory");
        assert!(!disp.is_standard_file_put_tracking());
        assert_eq!(bus.read_byte(reply_ptr), 1);
        assert_eq!(wd_info.dir_id, pilots_dir);
        assert_eq!(bus.read_pstring(reply_ptr + 10), b"Old Pilot".to_vec());
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // Pack3 / Standard File ($A9EA) — CustomGetFile selector $0008
    // IM:Files 1992 pp. 3-51..3-54: reply pointer is still the VAR output.
    #[test]
    fn custom_get_file_cancel_uses_reply_pointer_at_sp_plus_28_and_pops_42_bytes() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let decoy_reply_ptr = 0x320180u32;
        let actual_reply_ptr = 0x320200u32;
        bus.write_byte(decoy_reply_ptr, 0xEE);
        bus.write_byte(actual_reply_ptr, 0xFF);
        bus.write_word(sp, 0x0008); // CustomGetFile selector
        bus.write_long(sp + 2, decoy_reply_ptr); // should be ignored
        bus.write_long(sp + 28, actual_reply_ptr); // VAR reply for selector $0008

        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(decoy_reply_ptr), 0xEE);
        assert_eq!(bus.read_byte(actual_reply_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 42);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn pack3_generated_routes_preserve_exact_stack_word_values() {
        assert_eq!(super::PACK3_OPERATION_ROUTES.len(), 8);
        assert!(super::PACK3_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0001, "SFPutFile"),
            (0x0002, "SFGetFile"),
            (0x0003, "SFPPutFile"),
            (0x0004, "SFPGetFile"),
            (0x0005, "StandardPutFile"),
            (0x0006, "StandardGetFile"),
            (0x0007, "CustomPutFile"),
            (0x0008, "CustomGetFile"),
        ] {
            let route =
                super::pack3_operation_route(0xA9EA, selector).expect("Pack3 operation route");
            assert_eq!(route.routine_name, routine_name);
            assert_eq!(
                route.operation_id,
                format!("selector-operation:_Pack3:0x{selector:04X}:stack-word-immediate:16")
            );
        }

        for (trap_word, selector) in [
            (0xA8EA, 0x0001),
            (0xADEA, 0x0001), // same slot with a different raw trap form
            (0xA9EB, 0x0001),
            (0xA9E5, 0x0001), // InitPack is a distinct trap
            (0xA9EA, 0x0000), // unassigned selector
            (0xA9EA, 0x0009), // unassigned selector
            (0xA9EA, 0x0010), // out-of-range selector
            (0xA9EA, 0x000F), // odd adjacent value
            (0xA9EA, 0x3F3C), // MOVE.W immediate-to-stack opcode spelling
            (0xA9EA, 0x0100), // byte-swapped SFPutFile selector
            (0xA9EA, 0x0600), // byte-swapped StandardGetFile selector
        ] {
            assert!(super::pack3_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack3_records_stack_word_identity_without_changing_standard_get_file_behavior() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        for trap_word in [0xA9EA, 0xADEA] {
            disp.current_trap_word = trap_word;
            let reply_ptr = 0x320000u32;
            bus.write_byte(reply_ptr, 0xFF);
            bus.write_byte(reply_ptr + 1, 0xAB);
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::A0, 0x1234_0000);
            cpu.write_reg(Register::A1, 0x1234_0001);
            cpu.write_reg(Register::D0, 0x1234_5678);
            cpu.write_reg(Register::D1, 0x1234_0002);
            bus.write_word(sp, 0x0006); // StandardGetFile selector
            bus.write_long(sp + 2, reply_ptr); // VAR reply
            bus.write_long(sp + 16, 0xCAFE_BABE); // adjacent stack poison

            let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
            assert!(result.expect("Pack3 arm").is_ok());
            assert_eq!(bus.read_byte(reply_ptr), 0);
            assert_eq!(bus.read_byte(reply_ptr + 1), 0xAB);
            assert_eq!(cpu.read_reg(Register::A7), sp + 16);
            assert_eq!(cpu.read_reg(Register::D0), 0);
            assert_eq!(cpu.read_reg(Register::A0), 0x1234_0000);
            assert_eq!(cpu.read_reg(Register::A1), 0x1234_0001);
            assert_eq!(cpu.read_reg(Register::D1), 0x1234_0002);
            assert_eq!(bus.read_long(sp + 16), 0xCAFE_BABE);

            let expected = (trap_word == 0xA9EA)
                .then_some("selector-operation:_Pack3:0x0006:stack-word-immediate:16");
            assert_eq!(disp.current_selector_operation, expected);
        }
    }

    #[test]
    fn pack3_unknown_selector_clears_known_identity_without_touching_its_frame() {
        let _env = clear_standard_file_env();
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA9EA;

        let reply_ptr = 0x320000u32;
        bus.write_byte(reply_ptr, 0xFF);
        bus.write_word(sp, 0x0006); // StandardGetFile
        bus.write_long(sp + 2, reply_ptr);
        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.expect("Pack3 StandardGetFile arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack3:0x0006:stack-word-immediate:16")
        );

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x1234_5678);
        bus.write_word(sp, 0x0009); // unassigned selector
        bus.write_bytes(sp + 2, &[0xA5; 16]);
        let result = disp.dispatch_toolbox(true, 0x1EA, &mut cpu, &mut bus);
        assert!(result.expect("Pack3 unknown-selector fallback").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_bytes(sp + 2, 16), vec![0xA5; 16]);
    }

    #[test]
    fn pack6_generated_routes_preserve_exact_stack_word_values() {
        assert_eq!(super::PACK6_OPERATION_ROUTES.len(), 9);
        assert!(super::PACK6_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x000E, "DateString"),
            (0x0010, "TimeString"),
            (0x0014, "LongDateString"),
            (0x0016, "LongTimeString"),
            (0x001A, "CompareText"),
            (0x001C, "IdenticalText"),
            (0x001E, "ScriptOrder"),
            (0x0020, "LanguageOrder"),
            (0x0022, "TextOrder"),
        ] {
            let route =
                super::pack6_operation_route(0xA9ED, selector).expect("Pack6 operation route");
            assert_eq!(route.routine_name, routine_name);
            assert_eq!(
                route.operation_id,
                format!("selector-operation:_Pack6:0x{selector:04X}:stack-word-immediate:16")
            );
        }

        for (trap_word, selector) in [
            (0xA8ED, 0x000E),
            (0xADED, 0x000E), // same slot with a different raw trap form
            (0xA9EC, 0x0022),
            (0xA9E5, 0x000E), // InitPack is a distinct trap
            (0xA9ED, 0x0000), // outside generated slice: IUDateString
            (0xA9ED, 0x0002), // outside generated slice: IUTimeString
            (0xA9ED, 0x0004), // outside generated slice: IUMetric / IsMetric
            (0xA9ED, 0x0006), // outside generated slice: IUGetIntl / GetIntlResource
            (0xA9ED, 0x0008), // outside generated slice: IUSetIntl / SetIntlResource
            (0xA9ED, 0x000A), // outside generated slice: IUMagString
            (0xA9ED, 0x000C), // outside generated slice: IUMagIDString
            (0xA9ED, 0x0012), // unassigned gap
            (0xA9ED, 0x0018), // outside generated slice: IUClearCache
            (0xA9ED, 0x0024), // outside generated slice: IUGetItlTable
            (0xA9ED, 0x0028), // TypeSelect glue is outside the manual intersection
            (0xA9ED, 0x000F), // odd adjacent value
            (0xA9ED, 0x3F3C), // MOVE.W immediate-to-stack opcode spelling
            (0xA9ED, 0x0E00), // byte-swapped DateString selector
        ] {
            assert!(super::pack6_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack6_records_stack_word_identity_without_changing_script_order_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        for trap_word in [0xA9ED, 0xADED] {
            disp.current_trap_word = trap_word;
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::A0, 0x1234_0000);
            cpu.write_reg(Register::A1, 0x1234_0001);
            cpu.write_reg(Register::D0, 0x1234_5678);
            cpu.write_reg(Register::D1, 0x1234_0002);
            bus.write_word(sp, 0x001E); // ScriptOrder
            bus.write_word(sp + 2, 5); // script2
            bus.write_word(sp + 4, 2); // script1
            bus.write_word(sp + 6, 0xBEEF); // Integer result
            bus.write_long(sp + 8, 0xCAFE_BABE); // adjacent stack poison

            let result = disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus);
            assert!(result.expect("Pack6 arm").is_ok());
            assert_eq!(cpu.read_reg(Register::A7), sp + 6);
            assert_eq!(bus.read_word(sp + 6), 0xFFFF);
            assert_eq!(cpu.read_reg(Register::D0), 0x0000_FFFF);
            assert_eq!(cpu.read_reg(Register::A0), 0x1234_0000);
            assert_eq!(cpu.read_reg(Register::A1), 0x1234_0001);
            assert_eq!(cpu.read_reg(Register::D1), 0x1234_0002);
            assert_eq!(bus.read_word(sp + 2), 5);
            assert_eq!(bus.read_word(sp + 4), 2);
            assert_eq!(bus.read_long(sp + 8), 0xCAFE_BABE);

            let expected = (trap_word == 0xA9ED)
                .then_some("selector-operation:_Pack6:0x001E:stack-word-immediate:16");
            assert_eq!(disp.current_selector_operation, expected);
        }
    }

    #[test]
    fn pack6_unknown_selector_clears_known_identity_without_touching_its_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA9ED;

        bus.write_word(sp, 0x001E); // ScriptOrder
        bus.write_word(sp + 2, 5);
        bus.write_word(sp + 4, 2);
        bus.write_word(sp + 6, 0xBEEF);
        let result = disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus);
        assert!(result.expect("Pack6 ScriptOrder arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack6:0x001E:stack-word-immediate:16")
        );

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x1234_5678);
        bus.write_word(sp, 0x0012); // unassigned gap
        bus.write_bytes(sp + 2, &[0xA5; 16]);
        let result = disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus);
        assert!(result.expect("Pack6 unknown-selector fallback").is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_bytes(sp + 2, 16), vec![0xA5; 16]);
    }

    // Pack6 / Intl Utilities ($A9ED) — IUMetric selector $0004
    // IM:I I-505: returns TRUE when metric, otherwise FALSE.
    #[test]
    fn iumetric_returns_false_in_result_slot_and_pops_selector_only() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA9ED;
        bus.write_word(sp, 0x0004); // IUMetric selector
        bus.write_word(sp + 2, 0xFFFF); // Boolean result slot

        let result = disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 2), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(disp.current_selector_operation, None);
    }

    // Pack6 / Intl Utilities ($A9ED) — IUGetIntl selector $0006.
    // IM:I pp. I-495 and I-505: return the requested INTL resource handle.
    #[test]
    fn iugetintl_returns_stable_us_system_resource_handles() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x0006);
        bus.write_word(sp + 2, 0);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let intl0_handle = bus.read_long(sp + 4);
        assert_ne!(intl0_handle, 0);
        assert_eq!(cpu.read_reg(Register::A0), intl0_handle);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(disp.resource_handle_files.get(&intl0_handle), Some(&0));
        assert_eq!(bus.get_alloc_size(bus.read_long(intl0_handle)), Some(32));

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp + 2, 1);
        disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let intl1_handle = bus.read_long(sp + 4);
        let intl1 = bus.read_long(intl1_handle);
        assert_ne!(intl1_handle, 0);
        assert_eq!(bus.get_alloc_size(intl1), Some(332));
        assert_eq!(bus.read_pstring(intl1), b"Sunday");
        assert_eq!(bus.read_pstring(intl1 + 7 * 16), b"January");
        assert_eq!(bus.read_word(intl1 + 330), 0x4E75);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp + 2, 0);
        disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_long(sp + 4), intl0_handle);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp + 2, 42);
        disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(bus.read_long(sp + 4), 0);
    }

    #[test]
    fn iugetintl_prefers_loaded_application_resource() {
        let (mut disp, mut cpu, mut bus) = setup();
        let override_ptr = bus.alloc(32);
        bus.fill_bytes(override_ptr, 32, 0xA5);
        disp.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([
                (0, ResourceFileMap::default()),
                (
                    1,
                    ResourceFileMap {
                        loaded: HashMap::from([((*b"INTL", 0), override_ptr)]),
                        ..ResourceFileMap::default()
                    },
                ),
            ]),
            names: HashMap::new(),
            search_order: vec![0, 1],
            current_file: 1,
        });

        bus.write_word(TEST_SP, 0x0006);
        bus.write_word(TEST_SP + 2, 0);
        disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        let handle = bus.read_long(TEST_SP + 4);
        assert_eq!(bus.read_long(handle), override_ptr);
        assert_eq!(disp.resource_handle_files.get(&handle), Some(&1));
        assert!(disp.system_intl_cache.is_empty());
    }

    // Pack6 / Intl Utilities ($A9ED) — IUMagIDString selector $000C
    // IM:I I-507: returns 0 for equal (ignoring secondary ordering), 1 otherwise.
    #[test]
    fn iumagidstring_case_insensitive_equal_returns_zero_and_pops_14_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let a_ptr = 0x330000u32;
        let b_ptr = 0x330100u32;
        bus.write_bytes(a_ptr, b"Rose");
        bus.write_bytes(b_ptr, b"rOsE");
        bus.write_word(sp, 0x000C); // IUMagIDString selector
        bus.write_word(sp + 2, 4); // bLen
        bus.write_word(sp + 4, 4); // aLen
        bus.write_long(sp + 6, b_ptr);
        bus.write_long(sp + 10, a_ptr);
        bus.write_word(sp + 14, 0xFFFF); // result slot

        let result = disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn iumagidstring_case_insensitive_mismatch_returns_one_and_pops_14_bytes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let a_ptr = 0x330200u32;
        let b_ptr = 0x330300u32;
        bus.write_bytes(a_ptr, b"Rose");
        bus.write_bytes(b_ptr, b"Moss");
        bus.write_word(sp, 0x000C); // IUMagIDString selector
        bus.write_word(sp + 2, 4); // bLen
        bus.write_word(sp + 4, 4); // aLen
        bus.write_long(sp + 6, b_ptr);
        bus.write_long(sp + 10, a_ptr);
        bus.write_word(sp + 14, 0xFFFF); // result slot

        let result = disp.dispatch_toolbox(true, 0x1ED, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 1);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(cpu.read_reg(Register::D0), 1);
    }

    // Pack12 / Color Picker ($A82E)
    // Inside Macintosh Volume VI (1991), pp. 19-10 to 19-13.
    #[test]
    fn pack12_fix2smallfract_selector_0001_returns_low_word_and_pops_selector_plus_fixed() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0x0001); // Fix2SmallFract selector
        bus.write_long(sp + 2, 0x0000_5678); // Fixed input in 0..1 range
        bus.write_word(sp + 6, 0xBEEF); // SmallFract result slot

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 6), 0x5678);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    #[test]
    fn pack7_cstr2dec_selector_0004_scans_c_string_and_consumes_mpw_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let string_ptr = 0x5000;
        let index_ptr = 0x5100;
        let decimal_ptr = 0x5200;
        let valid_prefix_ptr = 0x5300;

        bus.write_bytes(string_ptr, b"prefix-12.50e+2tail\0");
        bus.write_word(index_ptr, 6);
        bus.write_word(valid_prefix_ptr, 0xFFFF);
        bus.write_bytes(decimal_ptr, &[0xCC; 26]);

        // MPW 3.5's str2dec wrapper leaves the DecStr68K selector at
        // SP, followed by validPrefix*, decimal*, index*, and C string*.
        bus.write_word(sp, 4);
        bus.write_long(sp + 2, valid_prefix_ptr);
        bus.write_long(sp + 6, decimal_ptr);
        bus.write_long(sp + 10, index_ptr);
        bus.write_long(sp + 14, string_ptr);

        let result = disp.dispatch_toolbox(true, 0x1EE, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 18);
        assert_eq!(bus.read_word(index_ptr), 15);
        assert_eq!(bus.read_word(valid_prefix_ptr), 0);
        assert_eq!(bus.read_byte(decimal_ptr), 1);
        assert_eq!(bus.read_byte(decimal_ptr + 1), 0);
        assert_eq!(bus.read_word(decimal_ptr + 2), 0);
        assert_eq!(bus.read_byte(decimal_ptr + 4), 4);
        assert_eq!(bus.read_bytes(decimal_ptr + 5, 4), b"1250");
        assert_eq!(bus.read_byte(decimal_ptr + 25), 0);
    }

    fn run_fdec2str(
        disp: &mut TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut MacMemoryBus,
        style: u8,
        requested_digits: i16,
        negative: bool,
        exponent: i16,
        digits: &[u8],
    ) -> Vec<u8> {
        let sp = TEST_SP;
        let string_ptr = 0x5400;
        let decimal_ptr = 0x5500;
        let decform_ptr = 0x5600;

        bus.write_byte(decimal_ptr, u8::from(negative));
        bus.write_byte(decimal_ptr + 1, 0);
        bus.write_word(decimal_ptr + 2, exponent as u16);
        bus.write_byte(decimal_ptr + 4, digits.len() as u8);
        bus.write_bytes(decimal_ptr + 5, digits);
        bus.write_byte(decimal_ptr + 25, 0);
        bus.write_byte(decform_ptr, style);
        bus.write_byte(decform_ptr + 1, 0);
        bus.write_word(decform_ptr + 2, requested_digits as u16);
        bus.write_byte(string_ptr, 0xCC);

        // MPW SANEMacs.a FDEC2STR frame: selector, DecStr*, decimal*, decform*.
        bus.write_word(sp, 3);
        bus.write_long(sp + 2, string_ptr);
        bus.write_long(sp + 6, decimal_ptr);
        bus.write_long(sp + 10, decform_ptr);
        cpu.write_reg(Register::A7, sp);

        disp.dispatch_toolbox(true, 0x1EE, cpu, bus)
            .expect("Pack7 dispatch")
            .expect("FDEC2STR succeeds");
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        bus.read_pstring(string_ptr)
    }

    #[test]
    fn pack7_fdec2str_formats_fixed_style_without_discarding_exact_digits() {
        let (mut disp, mut cpu, mut bus) = setup();

        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, -2, b"1250"),
            b"12.50"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, true, -2, b"1250"),
            b"-12.50"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 4, false, -1, b"125"),
            b"12.5000"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, -3, b"9995"),
            b"9.995"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, -2, b"1000"),
            b"10.00"
        );
        // Num2Dec performs requested rounding before FDEC2STR. These are the
        // carried records produced at either side of a rounding boundary.
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, -2, b"999"),
            b"9.99"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, -2, b"1000"),
            b"10.00"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, -2, false, 0, b"1499"),
            b"1499"
        );
    }

    #[test]
    fn pack7_fdec2str_formats_float_style_with_padding_sign_and_exponent() {
        let (mut disp, mut cpu, mut bus) = setup();

        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, false, -2, b"1250"),
            b" 1.250e+1"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, true, -2, b"1250"),
            b"-1.250e+1"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 6, false, -2, b"1250"),
            b" 1.25000e+1"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 1, false, 9999, b"1"),
            b" 1e+9999"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, false, -3, b"1000"),
            b" 1.000e+0"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, false, -1, b"9999"),
            b" 9.999e+2"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, false, -1, b"1000"),
            b" 1.000e+2"
        );
    }

    #[test]
    fn pack7_fdec2str_formats_signed_zero_in_both_styles() {
        let (mut disp, mut cpu, mut bus) = setup();

        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, 999, b"0"),
            b"0.00"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, true, -999, b"0"),
            b"-0.00"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, false, 999, b"0"),
            b" 0.000e+0"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, true, -999, b"0"),
            b"-0.000e+0"
        );
    }

    #[test]
    fn pack7_fdec2str_formats_infinity_nan_and_invalid_records() {
        let (mut disp, mut cpu, mut bus) = setup();

        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, true, 0, b"I"),
            b"-INF"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 0, 4, false, 0, b"I"),
            b" INF"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, 0, b"N4021"),
            b"NAN(033)"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, 0, b"NFFFF"),
            b"NAN(255)"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 2, false, 0, b""),
            b"?"
        );
        assert_eq!(
            run_fdec2str(&mut disp, &mut cpu, &mut bus, 1, 80, false, 0, b"1"),
            b"?"
        );
    }

    #[test]
    fn pack12_smallfract2fix_selector_0002_returns_low_word_fixed_and_pops_selector_plus_smallfract(
    ) {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_word(sp, 0x0002); // SmallFract2Fix selector
        bus.write_word(sp + 2, 0x89AB); // SmallFract input
        bus.write_long(sp + 4, 0xDEAD_BEEF); // Fixed result slot

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 4), 0x0000_89AB);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn pack12_getcolor_selector_0009_returns_false_and_pops_selector_plus_arguments() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let out_ptr = 0x350000u32;
        let in_ptr = 0x350100u32;

        bus.write_word(in_ptr, 0x1234);
        bus.write_word(in_ptr + 2, 0x5678);
        bus.write_word(in_ptr + 4, 0x9ABC);
        bus.write_word(out_ptr, 0x1234);
        bus.write_word(out_ptr + 2, 0x5678);
        bus.write_word(out_ptr + 4, 0x9ABC);

        bus.write_word(sp, 0x0009); // GetColor selector
        bus.write_long(sp + 2, out_ptr); // outColor pointer
        bus.write_long(sp + 6, in_ptr); // inColor pointer
        bus.write_long(sp + 10, 0x350200); // prompt pointer
        bus.write_long(sp + 14, 0x000A_0014); // Point(where)
        bus.write_word(sp + 18, 0xFFFF); // Boolean result slot

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(out_ptr), 0x1234);
        assert_eq!(bus.read_word(out_ptr + 2), 0x5678);
        assert_eq!(bus.read_word(out_ptr + 4), 0x9ABC);
        assert_eq!(bus.read_word(sp + 18), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 18);
    }

    #[test]
    fn pack12_rgb2hsl_selector_0006_converts_pure_red_to_expected_hsl_words() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr = 0x350000u32;
        let dst_ptr = 0x350100u32;

        bus.write_word(src_ptr, 0xFFFF);
        bus.write_word(src_ptr + 2, 0x0000);
        bus.write_word(src_ptr + 4, 0x0000);
        bus.write_word(dst_ptr, 0xDEAD);
        bus.write_word(dst_ptr + 2, 0xBEEF);
        bus.write_word(dst_ptr + 4, 0xCAFE);

        bus.write_word(sp, 0x0006); // RGB2HSL selector
        bus.write_long(sp + 2, dst_ptr);
        bus.write_long(sp + 6, src_ptr);

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(dst_ptr), 0x0000);
        assert_eq!(bus.read_word(dst_ptr + 2), 0xFFFF);
        assert_eq!(bus.read_word(dst_ptr + 4), 0x8000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn pack12_hsv2rgb_selector_0007_converts_pure_red_hsv_to_rgb_words() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr = 0x350000u32;
        let dst_ptr = 0x350100u32;

        bus.write_word(src_ptr, 0x0000);
        bus.write_word(src_ptr + 2, 0xFFFF);
        bus.write_word(src_ptr + 4, 0xFFFF);
        bus.write_word(dst_ptr, 0xDEAD);
        bus.write_word(dst_ptr + 2, 0xBEEF);
        bus.write_word(dst_ptr + 4, 0xCAFE);

        bus.write_word(sp, 0x0007); // HSV2RGB selector
        bus.write_long(sp + 2, dst_ptr);
        bus.write_long(sp + 6, src_ptr);

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(dst_ptr), 0xFFFF);
        assert_eq!(bus.read_word(dst_ptr + 2), 0x0000);
        assert_eq!(bus.read_word(dst_ptr + 4), 0x0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn pack12_rgb2hsv_selector_0008_converts_pure_red_to_expected_hsv_words() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr = 0x350000u32;
        let dst_ptr = 0x350100u32;

        bus.write_word(src_ptr, 0xFFFF);
        bus.write_word(src_ptr + 2, 0x0000);
        bus.write_word(src_ptr + 4, 0x0000);
        bus.write_word(dst_ptr, 0xDEAD);
        bus.write_word(dst_ptr + 2, 0xBEEF);
        bus.write_word(dst_ptr + 4, 0xCAFE);

        bus.write_word(sp, 0x0008); // RGB2HSV selector
        bus.write_long(sp + 2, dst_ptr);
        bus.write_long(sp + 6, src_ptr);

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(dst_ptr), 0x0000);
        assert_eq!(bus.read_word(dst_ptr + 2), 0xFFFF);
        assert_eq!(bus.read_word(dst_ptr + 4), 0xFFFF);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn pack12_cmy2rgb_selector_0003_converts_each_smallfract_channel_to_complementary_rgb_word() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr = 0x350000u32;
        let dst_ptr = 0x350100u32;

        bus.write_word(src_ptr, 0x1111);
        bus.write_word(src_ptr + 2, 0x2222);
        bus.write_word(src_ptr + 4, 0x3333);
        bus.write_word(dst_ptr, 0xDEAD);
        bus.write_word(dst_ptr + 2, 0xBEEF);
        bus.write_word(dst_ptr + 4, 0xCAFE);

        bus.write_word(sp, 0x0003); // CMY2RGB selector
        bus.write_long(sp + 2, dst_ptr);
        bus.write_long(sp + 6, src_ptr);

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(dst_ptr), 0xEEEE);
        assert_eq!(bus.read_word(dst_ptr + 2), 0xDDDD);
        assert_eq!(bus.read_word(dst_ptr + 4), 0xCCCC);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    // Pack14 / Help Manager ($A830)
    // Public MPW declarations: HelpMgr.h selector-trap shims
    //   HMGetHelpMenuHandle(VAR mh: MenuHandle): OSErr;
    //   HMGetFont(VAR font: Integer): OSErr;
    // Inside Macintosh: More Macintosh Toolbox 1993, pp. 3-109 to 3-111;
    // selector table p. 3-173.
    #[test]
    fn pack14_generated_routes_preserve_exact_word_values() {
        assert_eq!(super::PACK14_OPERATION_ROUTES.len(), 21);
        assert!(super::PACK14_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0002, "HMRemoveBalloon"),
            (0x0003, "HMGetBalloons"),
            (0x0007, "HMIsBalloon"),
            (0x0104, "HMSetBalloons"),
            (0x0108, "HMSetFont"),
            (0x0109, "HMSetFontSize"),
            (0x010C, "HMSetDialogResID"),
            (0x0200, "HMGetHelpMenuHandle"),
            (0x020A, "HMGetFont"),
            (0x020B, "HMGetFontSize"),
            (0x020D, "HMSetMenuResID"),
            (0x0213, "HMGetDialogResID"),
            (0x0215, "HMGetBalloonWindow"),
            (0x0314, "HMGetMenuResID"),
            (0x040E, "HMBalloonRect"),
            (0x040F, "HMBalloonPict"),
            (0x0410, "HMScanTemplateItems"),
            (0x0711, "HMExtractHelpMsg"),
            (0x0B01, "HMShowBalloon"),
            (0x0E05, "HMShowMenuBalloon"),
            (0x1306, "HMGetIndHelpMsg"),
        ] {
            let route = super::pack14_operation_route(0xA830, selector).expect("Pack14 route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA930, 0x0104),
            (0xA830, 0x0000),
            (0xA830, 0x0105),
            (0xA830, 0x303C),
        ] {
            assert!(super::pack14_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack14_records_low_word_identity_without_changing_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA830;
        cpu.write_reg(Register::D0, 0xABCD_0104);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x030, &mut cpu, &mut bus);
        assert!(result.expect("Pack14 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack14:0x0104:d0-low-word-immediate:16")
        );
        assert_eq!(bus.read_word(sp + 2), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);

        disp.current_trap_word = 0xA930;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0104);
        bus.write_word(sp, 1);
        bus.write_word(sp + 2, 0xBEEF);
        let result = disp.dispatch_toolbox(true, 0x030, &mut cpu, &mut bus);
        assert!(result.expect("Pack14 arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
    }

    #[test]
    fn pack14_hmgethelpmenuhandle_writes_nil_and_returns_hmhelpmanagernotinited() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let mh_ptr = bus.alloc(4);

        cpu.write_reg(Register::D0, 0x0200);
        bus.write_long(mh_ptr, 0xDEAD_BEEF);
        bus.write_long(sp, mh_ptr);
        bus.write_word(sp + 4, 0xBEEF); // result slot poison

        let result = disp.dispatch_toolbox(true, 0x030, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(mh_ptr), 0);
        assert_eq!(bus.read_word(sp + 4), (-855i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-855i16) as i32 as u32);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn pack14_hmgetfont_writes_zero_and_returns_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let font_ptr = bus.alloc(2);

        cpu.write_reg(Register::D0, 0x020A);
        bus.write_word(font_ptr, 0x1357);
        bus.write_long(sp, font_ptr);
        bus.write_word(sp + 4, 0xBEEF); // result slot poison

        let result = disp.dispatch_toolbox(true, 0x030, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(font_ptr), 0);
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    #[test]
    fn pack14_hmgetballoons_uses_d0_selector_and_preserves_no_arg_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::D0, 0x0003);
        bus.write_word(sp, 0xBEEF); // Boolean result slot

        let result = disp.dispatch_toolbox(true, 0x030, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn pack14_hmsetballoons_pops_flag_arg_only() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::D0, 0x0104);
        bus.write_word(sp, 1); // flag
        bus.write_word(sp + 2, 0xBEEF); // OSErr result slot

        let result = disp.dispatch_toolbox(true, 0x030, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 2), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
    }

    #[test]
    fn pack12_rgb2cmy_selector_0004_converts_each_rgb_word_to_complementary_smallfract_channel() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr = 0x350200u32;
        let dst_ptr = 0x350300u32;

        bus.write_word(src_ptr, 0x1357);
        bus.write_word(src_ptr + 2, 0x2468);
        bus.write_word(src_ptr + 4, 0x369C);
        bus.write_word(dst_ptr, 0xDEAD);
        bus.write_word(dst_ptr + 2, 0xBEEF);
        bus.write_word(dst_ptr + 4, 0xCAFE);

        bus.write_word(sp, 0x0004); // RGB2CMY selector
        bus.write_long(sp + 2, dst_ptr);
        bus.write_long(sp + 6, src_ptr);

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(dst_ptr), 0xECA8);
        assert_eq!(bus.read_word(dst_ptr + 2), 0xDB97);
        assert_eq!(bus.read_word(dst_ptr + 4), 0xC963);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn pack12_hsl2rgb_selector_0005_converts_zero_saturation_to_grayscale_rgb_words() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let src_ptr = 0x350400u32;
        let dst_ptr = 0x350500u32;

        bus.write_word(src_ptr, 0x1357);
        bus.write_word(src_ptr + 2, 0x0000);
        bus.write_word(src_ptr + 4, 0x2468);
        bus.write_word(dst_ptr, 0xDEAD);
        bus.write_word(dst_ptr + 2, 0xBEEF);
        bus.write_word(dst_ptr + 4, 0xCAFE);

        bus.write_word(sp, 0x0005); // HSL2RGB selector
        bus.write_long(sp + 2, dst_ptr);
        bus.write_long(sp + 6, src_ptr);

        let result = disp.dispatch_toolbox(true, 0x02E, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(dst_ptr), 0x2468);
        assert_eq!(bus.read_word(dst_ptr + 2), 0x2468);
        assert_eq!(bus.read_word(dst_ptr + 4), 0x2468);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn aliasdispatch_generated_selector_routes_are_sorted_unique_and_complete() {
        assert_eq!(ALIAS_DISPATCH_OPERATION_ROUTES.len(), 7);
        assert!(ALIAS_DISPATCH_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        let new_alias = alias_dispatch_operation_route(0xA823, 0x0002).expect("NewAlias route");
        assert_eq!(new_alias.routine_name, "NewAlias");
        assert_eq!(
            new_alias.operation_id,
            "selector-operation:_AliasDispatch:0x0002:d0-moveq-immediate:8"
        );

        assert!(alias_dispatch_operation_route(0xA823, 0x7002).is_none());
        assert!(alias_dispatch_operation_route(0xA823, 0x0001_0002).is_none());
        assert!(alias_dispatch_operation_route(0xAA23, 0x0002).is_none());
    }

    #[test]
    fn aliasdispatch_records_every_generated_operation_and_rejects_opcode_spelling() {
        let (mut disp, mut cpu, mut bus) = setup();
        for route in ALIAS_DISPATCH_OPERATION_ROUTES {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, u32::from(route.selector));
            bus.write_bytes(TEST_SP, &[0; 32]);
            let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert_eq!(disp.current_selector_operation, Some(route.operation_id));
        }

        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 0x7002);
        bus.write_bytes(TEST_SP, &[0; 32]);
        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert_eq!(disp.current_selector_operation, None);

        cpu.write_reg(Register::A7, TEST_SP);
        cpu.write_reg(Register::D0, 0x0001_0002);
        bus.write_bytes(TEST_SP, &[0; 32]);
        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert_eq!(disp.current_selector_operation, None);
    }

    // AliasDispatch ($A823) / selector $0000 FindFolder
    // IM:VI 1991 pp. 9-43..9-44: FindFolder writes foundVRefNum and foundDirID.
    #[test]
    fn aliasdispatch_findfolder_preferences_type_returns_found_refs() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let found_dir_id_ptr = 0x340000u32;
        let found_vref_ptr = 0x340100u32;

        cpu.write_reg(Register::D0, 0x0000); // FindFolder selector
        bus.write_long(sp, found_dir_id_ptr);
        bus.write_long(sp + 4, found_vref_ptr);
        bus.write_word(sp + 8, 1); // createFolder = TRUE
        bus.write_long(sp + 10, u32::from_be_bytes(*b"pref"));
        bus.write_word(sp + 14, 0x8000); // kOnSystemDisk
        bus.write_word(sp + 16, 0xBEEF); // result slot

        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 16), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(found_vref_ptr), (-1i16) as u16);

        let found_dir_id = bus.read_long(found_dir_id_ptr);
        assert_ne!(found_dir_id, 0);
        assert_eq!(
            disp.directory_path_for_id(found_dir_id),
            Some("System Folder/Preferences")
        );
    }

    // AliasDispatch ($A823) / selector $0000 FindFolder
    // IM:VI 1991 pp. 9-42..9-44: kTemporaryFolderType ('temp') locates the
    // root-level Temporary Items folder and returns its vRefNum/dirID.
    #[test]
    fn aliasdispatch_findfolder_temporary_type_returns_temporary_items_dir() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let found_dir_id_ptr = 0x340200u32;
        let found_vref_ptr = 0x340300u32;

        cpu.write_reg(Register::D0, 0x0000); // FindFolder selector
        bus.write_long(sp, found_dir_id_ptr);
        bus.write_long(sp + 4, found_vref_ptr);
        bus.write_word(sp + 8, 1); // createFolder = TRUE
        bus.write_long(sp + 10, u32::from_be_bytes(*b"temp"));
        bus.write_word(sp + 14, 0x8000); // kOnSystemDisk
        bus.write_word(sp + 16, 0xBEEF); // result slot

        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 16), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(found_vref_ptr), (-1i16) as u16);

        let found_dir_id = bus.read_long(found_dir_id_ptr);
        assert_ne!(found_dir_id, 0);
        assert_eq!(
            disp.directory_path_for_id(found_dir_id),
            Some("Temporary Items")
        );
    }

    // AliasDispatch ($A823) / selector $0002 NewAlias
    // IM:VI 1991 pp. 27-12..27-13: NewAlias allocates storage and writes AliasHandle.
    #[test]
    fn aliasdispatch_newalias_returns_allocated_alias_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let alias_out_ptr = 0x341000u32;
        let target_spec_ptr = 0x341100u32;

        // FSSpec target record (vRefNum, dirID, name).
        bus.write_word(target_spec_ptr, (-1i16) as u16);
        bus.write_long(target_spec_ptr + 2, 2);
        bus.write_byte(target_spec_ptr + 6, 5);
        bus.write_bytes(target_spec_ptr + 7, b"Prefs");

        cpu.write_reg(Register::D0, 0x0002); // NewAlias selector
        bus.write_long(sp, alias_out_ptr);
        bus.write_long(sp + 4, target_spec_ptr);
        bus.write_long(sp + 8, 0); // fromFile = NIL
        bus.write_word(sp + 12, 0xBEEF); // result slot

        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 12), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        let alias_handle = bus.read_long(alias_out_ptr);
        assert_ne!(alias_handle, 0);
        let alias_data_ptr = bus.read_long(alias_handle);
        assert_ne!(alias_data_ptr, 0);
        let alias_size = bus.read_word(alias_data_ptr + 4) as usize;
        assert!(
            alias_size >= 150,
            "alias record should use the classic fixed header size"
        );
        assert_eq!(bus.read_word(alias_data_ptr + 6), 2);
        assert_eq!(bus.read_word(alias_data_ptr + 8), 0);
        assert_eq!(
            bus.read_pstring(alias_data_ptr + 10),
            b"MacintoshHD".to_vec()
        );
        assert_eq!(bus.read_pstring(alias_data_ptr + 50), b"Prefs".to_vec());
        assert_eq!(bus.read_word(alias_data_ptr + 150) as i16, 0);
        let parent_len = bus.read_word(alias_data_ptr + 152) as usize;
        let end_offset = 154 + parent_len + (parent_len % 2);
        assert_eq!(bus.read_word(alias_data_ptr + end_offset as u32) as i16, -1);
    }

    // AliasDispatch ($A823) / selector $0009 NewAliasMinimalFromFullPath
    // Universal Interfaces 2.0 Aliases.h: a full HFS path creates an
    // allocated AliasHandle through a five-parameter Pascal stack frame.
    #[test]
    fn aliasdispatch_newaliasminimalfromfullpath_builds_classic_alias_record() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let alias_out_ptr = 0x341200u32;
        let full_path_ptr = 0x341300u32;
        let full_path = b"MacintoshHD:Games:Civilization II:90.PICT";
        bus.write_bytes(full_path_ptr, full_path);

        cpu.write_reg(Register::D0, 0x0009);
        bus.write_long(sp, alias_out_ptr);
        bus.write_long(sp + 4, 0); // serverName
        bus.write_long(sp + 8, 0); // zoneName
        bus.write_long(sp + 12, full_path_ptr);
        bus.write_word(sp + 16, full_path.len() as u16);
        bus.write_word(sp + 18, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 18), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 18);
        let alias_handle = bus.read_long(alias_out_ptr);
        assert_ne!(alias_handle, 0);
        let alias_data_ptr = bus.read_long(alias_handle);
        assert_ne!(alias_data_ptr, 0);
        assert_eq!(
            bus.read_pstring(alias_data_ptr + 10),
            b"MacintoshHD".to_vec()
        );
        assert_eq!(bus.read_pstring(alias_data_ptr + 50), b"90.PICT".to_vec());
        assert_eq!(bus.read_word(alias_data_ptr + 150), 0);
        assert_eq!(bus.read_word(alias_data_ptr + 152), 15);
        assert_eq!(
            bus.read_bytes(alias_data_ptr + 154, 15),
            b"Civilization II".to_vec()
        );
    }

    // AliasDispatch ($A823) / selector $0003 ResolveAlias
    // Universal Interfaces 2.0 Aliases.h: resolve a created alias into its
    // target FSSpec and return wasChanged through the Pascal stack frame.
    #[test]
    fn aliasdispatch_resolvealias_finds_minimal_full_path_target() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let alias_out_ptr = 0x341400u32;
        let full_path_ptr = 0x341500u32;
        let target_ptr = 0x341600u32;
        let was_changed_ptr = 0x341700u32;
        let full_path = b"MacintoshHD:Games:Civilization II:90.PICT";
        bus.write_bytes(full_path_ptr, full_path);
        disp.vfs.insert(
            "Demo/Games/Civilization II/90.PICT".to_string(),
            b"picture".to_vec(),
        );

        cpu.write_reg(Register::D0, 0x0009);
        bus.write_long(sp, alias_out_ptr);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, 0);
        bus.write_long(sp + 12, full_path_ptr);
        bus.write_word(sp + 16, full_path.len() as u16);
        bus.write_word(sp + 18, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(create.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0003);
        bus.write_long(sp, was_changed_ptr);
        bus.write_long(sp + 4, target_ptr);
        bus.write_long(sp + 8, bus.read_long(alias_out_ptr));
        bus.write_long(sp + 12, 0); // fromFile
        bus.write_word(sp + 16, 0xBEEF);
        bus.write_byte(was_changed_ptr, 0xFF);

        let resolve = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(resolve.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_byte(was_changed_ptr), 0);
        assert_eq!(
            bus.read_word(target_ptr),
            super::super::dispatch::BOOT_VOLUME_REF_NUM as u16
        );
        assert_eq!(bus.read_pstring(target_ptr + 6), b"90.PICT".to_vec());
        let target_dir_id = bus.read_long(target_ptr + 2);
        assert_eq!(
            disp.directory_path_for_id(target_dir_id),
            Some("Demo/Games/Civilization II")
        );
    }

    // AliasDispatch ($A823) / selector $000C ResolveAliasFile
    // IM:VI 1991 pp. 9-30..9-31: non-alias input returns noErr and wasAliased=FALSE.
    #[test]
    fn aliasdispatch_resolvealiasfile_non_alias_returns_false_flags_and_preserves_spec() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let was_aliased_ptr = 0x342000u32;
        let target_is_folder_ptr = 0x342100u32;
        let spec_ptr = 0x342200u32;
        disp.vfs.insert("ReadMe!!".to_string(), b"hello".to_vec());

        // FSSpec input for a regular file path.
        bus.write_word(spec_ptr, (-1i16) as u16);
        bus.write_long(spec_ptr + 2, 2);
        bus.write_byte(spec_ptr + 6, 8);
        bus.write_bytes(spec_ptr + 7, b"ReadMe!!");
        let spec_before = bus.read_bytes(spec_ptr, 32);

        bus.write_byte(was_aliased_ptr, 0xFF);
        bus.write_byte(target_is_folder_ptr, 0xFF);

        cpu.write_reg(Register::D0, 0x000C); // ResolveAliasFile selector
        bus.write_long(sp, was_aliased_ptr);
        bus.write_long(sp + 4, target_is_folder_ptr);
        bus.write_word(sp + 8, 1); // resolveAliasChains = TRUE
        bus.write_long(sp + 10, spec_ptr);
        bus.write_word(sp + 14, 0xBEEF); // result slot

        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_byte(was_aliased_ptr), 0);
        assert_eq!(bus.read_byte(target_is_folder_ptr), 0);
        assert_eq!(bus.read_bytes(spec_ptr, 32), spec_before);
    }

    // AliasDispatch ($A823) / selector $000C ResolveAliasFile
    // IM:Files 1992 p. 4-21: ResolveAliasFile returns fnfErr when the input
    // FSSpec does not identify an existing file or directory.
    #[test]
    fn aliasdispatch_resolvealiasfile_missing_spec_returns_fnferr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let was_aliased_ptr = 0x342300u32;
        let target_is_folder_ptr = 0x342400u32;
        let spec_ptr = 0x342500u32;

        bus.write_word(spec_ptr, (-1i16) as u16);
        bus.write_long(spec_ptr + 2, 2);
        bus.write_byte(spec_ptr + 6, 7);
        bus.write_bytes(spec_ptr + 7, b"Missing");

        bus.write_byte(was_aliased_ptr, 0xFF);
        bus.write_byte(target_is_folder_ptr, 0xFF);

        cpu.write_reg(Register::D0, 0x000C);
        bus.write_long(sp, was_aliased_ptr);
        bus.write_long(sp + 4, target_is_folder_ptr);
        bus.write_word(sp + 8, 1);
        bus.write_long(sp + 10, spec_ptr);
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x023, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 14) as i16, -43);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_byte(was_aliased_ptr), 0);
        assert_eq!(bus.read_byte(target_is_folder_ptr), 0);
    }

    // Pack8 / Apple Events ($A816)
    // Inside Macintosh: Interapplication Communication 1993:
    // AEInstallEventHandler pp.4-62..4-64, AEProcessAppleEvent pp.4-66..4-67.

    #[test]
    fn pack8_generated_routes_preserve_exact_word_values() {
        assert_eq!(super::PACK8_OPERATION_ROUTES.len(), 54);
        assert!(super::PACK8_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x011E, "AESetInteractionAllowed"),
            (0x0204, "AEDisposeDesc"),
            (0x0219, "AEResetTimer"),
            (0x021A, "AEGetTheCurrentEvent"),
            (0x021B, "AEProcessAppleEvent"),
            (0x021D, "AEGetInteractionAllowed"),
            (0x022B, "AESuspendTheCurrentEvent"),
            (0x022C, "AESetTheCurrentEvent"),
            (0x023A, "AEDisposeToken"),
            (0x0405, "AEDuplicateDesc"),
            (0x0407, "AECountItems"),
            (0x040E, "AEDeleteItem"),
            (0x0413, "AEDeleteParam"),
            (0x0441, "AEManagerInfo"),
            (0x0500, "AEInstallSpecialHandler"),
            (0x0501, "AERemoveSpecialHandler"),
            (0x052D, "AEGetSpecialHandler"),
            (0x0536, "AEResolve"),
            (0x0603, "AECoerceDesc"),
            (0x0609, "AEPutDesc"),
            (0x0610, "AEPutParamDesc"),
            (0x061C, "AEInteractWithUser"),
            (0x0627, "AEPutAttributeDesc"),
            (0x0706, "AECreateList"),
            (0x0720, "AERemoveEventHandler"),
            (0x0723, "AERemoveCoercionHandler"),
            (0x0738, "AERemoveObjectAccessor"),
            (0x0812, "AEGetParamDesc"),
            (0x0818, "AEResumeTheCurrentEvent"),
            (0x0825, "AECreateDesc"),
            (0x0826, "AEGetAttributeDesc"),
            (0x0828, "AESizeOfAttribute"),
            (0x0829, "AESizeOfParam"),
            (0x082A, "AESizeOfNthItem"),
            (0x091F, "AEInstallEventHandler"),
            (0x0921, "AEGetEventHandler"),
            (0x0937, "AEInstallObjectAccessor"),
            (0x0939, "AEGetObjectAccessor"),
            (0x0A02, "AECoercePtr"),
            (0x0A08, "AEPutPtr"),
            (0x0A0B, "AEGetNthDesc"),
            (0x0A0F, "AEPutParamPtr"),
            (0x0A16, "AEPutAttributePtr"),
            (0x0A22, "AEInstallCoercionHandler"),
            (0x0B0D, "AEPutArray"),
            (0x0B14, "AECreateAppleEvent"),
            (0x0B24, "AEGetCoercionHandler"),
            (0x0C3B, "AECallObjectAccessor"),
            (0x0D0C, "AEGetArray"),
            (0x0D17, "AESend"),
            (0x0E11, "AEGetParamPtr"),
            (0x0E15, "AEGetAttributePtr"),
            (0x0E35, "AESetObjectCallbacks"),
            (0x100A, "AEGetNthPtr"),
        ] {
            let route = super::pack8_operation_route(0xA816, selector).expect("Pack8 route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA916, 0x091F),
            (0xA816, 0x081F),
            (0xA816, 0x0920),
            (0xA816, 0x303C),
            (0xA816, crate::trap::dispatch::LOADSEG_GETRESOURCE_SENTINEL),
            (0xA816, 0xFEFE),
        ] {
            assert!(super::pack8_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack8_records_low_word_identity_and_rejects_wrong_trap() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xA816;
        cpu.write_reg(Register::D0, 0xABCD_0720);
        for offset in 0..14 {
            bus.write_byte(sp + offset, 0);
        }
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.expect("Pack8 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack8:0x0720:d0-low-word-immediate:16")
        );

        disp.current_trap_word = 0xA916;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0720);
        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.expect("Pack8 arm").is_ok());
        assert_eq!(disp.current_selector_operation, None);
    }

    #[test]
    fn pack8_unhandled_selector_pops_param_words_and_returns_noerr() {
        // High byte of D0.W encodes parameter word count for Pack8 calls.
        // Keep this generic selector-pop contract pinned for stubbed routines.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::D0, 0x07FF); // 7 words params, unimplemented routine $FF
        for i in 0..14 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 14, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 14), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn pack8_aemanagerinfo_returns_version_and_recorder_count() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let result_ptr = 0x0030_0000u32;

        cpu.write_reg(Register::D0, 0x0441); // AEManagerInfo
        bus.write_long(sp, result_ptr);
        bus.write_long(sp + 4, AE_MANAGER_KEY_VERSION);
        bus.write_word(sp + 8, 0xBEEF);
        let version = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(version.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_long(result_ptr), 0x0101_0000);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0441);
        bus.write_long(sp, result_ptr);
        bus.write_long(sp + 4, AE_MANAGER_KEY_RECORDER_COUNT);
        bus.write_word(sp + 8, 0xBEEF);
        let count = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(count.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(bus.read_long(result_ptr), 0);
    }

    #[test]
    fn pack8_aeinstalleventhandler_installs_dispatch_entry_for_event_class_and_id() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"aevt");
        let event_id = u32::from_be_bytes(*b"oapp");
        let handler_ptr = 0x00AB_CDEEu32;
        let handler_refcon = 0x0102_0304u32;

        // Selector $091F => AEInstallEventHandler.
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler = FALSE
        bus.write_long(sp + 2, handler_refcon);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF); // OSErr result slot

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 18), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 18);
        assert_eq!(
            disp.ae_handlers
                .get(false, event_class, event_id)
                .map(|handler| (handler.procedure.original_pointer, handler.refcon)),
            Some((handler_ptr, handler_refcon))
        );
    }

    #[test]
    fn pack8_aeinstalleventhandler_replaces_existing_entry_for_same_event_key() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"aevt");
        let event_id = u32::from_be_bytes(*b"oapp");

        // Inside Macintosh: Interapplication Communication p.4-64:
        // an existing entry for the same class/ID is replaced.
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, 0x1111_2222); // refcon #1
        bus.write_long(sp + 6, 0x00AA_0000); // handler #1
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let first = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, 0x3333_4444); // refcon #2
        bus.write_long(sp + 6, 0x00BB_0002); // handler #2
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let second = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(second.is_some());
        assert!(second.unwrap().is_ok());

        assert_eq!(
            disp.ae_handlers
                .get(false, event_class, event_id)
                .map(|handler| (handler.procedure.original_pointer, handler.refcon)),
            Some((0x00BB_0002u32, 0x3333_4444u32))
        );
    }

    #[test]
    fn pack8_aegeteventhandler_returns_exact_dispatch_entry() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");
        let handler_ptr = 0x0040_2222u32;
        let handler_refcon = 0x0000_0FA2u32;
        let out_handler_ptr = 0x0030_0000u32;
        let out_refcon_ptr = 0x0030_0010u32;

        cpu.write_reg(Register::D0, 0x091F); // AEInstallEventHandler
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, handler_refcon);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0921); // AEGetEventHandler
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, out_refcon_ptr);
        bus.write_long(sp + 6, out_handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 18), 0);
        assert_eq!(bus.read_long(out_handler_ptr), handler_ptr);
        assert_eq!(bus.read_long(out_refcon_ptr), handler_refcon);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0921);
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, out_refcon_ptr);
        bus.write_long(sp + 6, out_handler_ptr);
        bus.write_long(sp + 10, AE_TYPE_WILDCARD);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let missing = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(missing.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 18), AE_ERR_HANDLER_NOT_FOUND as u16);
        assert_eq!(bus.read_long(out_handler_ptr), 0);
        assert_eq!(bus.read_long(out_refcon_ptr), 0);
    }

    #[test]
    fn pack8_special_handlers_and_object_callbacks_are_observable() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let function_class = u32::from_be_bytes(*b"selh");
        let handler_ptr = 0x0040_3333u32;
        let out_handler_ptr = 0x0030_0000u32;

        cpu.write_reg(Register::D0, 0x0500); // AEInstallSpecialHandler
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, handler_ptr);
        bus.write_long(sp + 6, function_class);
        bus.write_word(sp + 10, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x052D); // AEGetSpecialHandler
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, out_handler_ptr);
        bus.write_long(sp + 6, function_class);
        bus.write_word(sp + 10, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(bus.read_long(out_handler_ptr), handler_ptr);

        let count_proc = 0x0040_4444u32;
        let compare_proc = 0x0040_5555u32;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0E35); // AESetObjectCallbacks
        bus.write_long(sp, 0); // getErrDesc
        bus.write_long(sp + 4, 0); // adjustMarks
        bus.write_long(sp + 8, 0); // mark
        bus.write_long(sp + 12, 0); // markToken
        bus.write_long(sp + 16, 0); // disposeToken
        bus.write_long(sp + 20, count_proc);
        bus.write_long(sp + 24, compare_proc);
        bus.write_word(sp + 28, 0xBEEF);
        let callbacks = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(callbacks.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 28), 0);
        assert_eq!(
            disp.ae_special_handlers.get(&(false, AE_KEY_COUNT_PROC)),
            Some(&count_proc)
        );
        assert_eq!(
            disp.ae_special_handlers.get(&(false, AE_KEY_COMPARE_PROC)),
            Some(&compare_proc)
        );
    }

    #[test]
    fn pack8_coercion_handler_install_and_get_round_trip() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let from_type = u32::from_be_bytes(*b"whos");
        let to_type = u32::from_be_bytes(*b"whos");
        let handler_ptr = 0x0040_6666u32;
        let handler_refcon = 0x0102_0304u32;
        let out_handler_ptr = 0x0030_0000u32;
        let out_refcon_ptr = 0x0030_0010u32;
        let out_from_desc_ptr = 0x0030_0020u32;

        cpu.write_reg(Register::D0, 0x0A22); // AEInstallCoercionHandler
        bus.write_word(sp, 0); // isSysHandler
        bus.write_word(sp + 2, 0xFF00); // fromTypeIsDesc = TRUE
        bus.write_long(sp + 4, handler_refcon);
        bus.write_long(sp + 8, handler_ptr);
        bus.write_long(sp + 12, to_type);
        bus.write_long(sp + 16, from_type);
        bus.write_word(sp + 20, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0B24); // AEGetCoercionHandler
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, out_from_desc_ptr);
        bus.write_long(sp + 6, out_refcon_ptr);
        bus.write_long(sp + 10, out_handler_ptr);
        bus.write_long(sp + 14, to_type);
        bus.write_long(sp + 18, from_type);
        bus.write_word(sp + 22, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 22), 0);
        assert_eq!(bus.read_long(out_handler_ptr), handler_ptr);
        assert_eq!(bus.read_long(out_refcon_ptr), handler_refcon);
        assert_eq!(bus.read_byte(out_from_desc_ptr), 0xFF);
    }

    #[test]
    fn pack8_private_object_support_hash_table_round_trip() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let table_out_ptr = 0x0030_0000u32;
        let key_ptr = 0x0030_0010u32;
        let value_ptr = 0x0030_0020u32;
        let fetched_value_ptr = 0x0030_0030u32;
        let desired_class = u32::from_be_bytes(*b"cwin");
        let container_type = u32::from_be_bytes(*b"Toke");
        let accessor_ptr = 0x0040_2222u32;
        let accessor_refcon = 0x0102_0304u32;

        cpu.write_reg(Register::D0, 0x092E);
        bus.write_long(sp, table_out_ptr);
        bus.write_word(sp + 4, 0); // system table flag, ignored by table allocation
        bus.write_long(sp + 6, 0);
        bus.write_long(sp + 10, 0x0008_0008); // 8-byte key, 8-byte value
        bus.write_long(sp + 14, 16);
        bus.write_word(sp + 18, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 18), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 18);

        let table_handle = bus.read_long(table_out_ptr);
        assert_ne!(table_handle, 0);
        let table_data = bus.read_long(table_handle);
        assert_ne!(table_data, 0);
        assert_eq!(bus.get_alloc_size(table_data), Some(16));
        assert_eq!(bus.read_long(table_data + 4), 16);
        assert_eq!(bus.read_long(table_data + 8), 8);
        assert_eq!(bus.read_long(table_data + 12), 8);
        assert_eq!(disp.handle_for_ptr(table_data), Some(table_handle));

        bus.write_long(key_ptr, desired_class);
        bus.write_long(key_ptr + 4, container_type);
        bus.write_long(value_ptr, accessor_ptr);
        bus.write_long(value_ptr + 4, accessor_refcon);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0831);
        bus.write_long(sp, value_ptr);
        bus.write_long(sp + 4, key_ptr);
        bus.write_long(sp + 8, 0);
        bus.write_long(sp + 12, table_handle);
        bus.write_word(sp + 16, 0xBEEF);
        let insert = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(insert.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 16), 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0833);
        bus.write_long(sp, fetched_value_ptr);
        bus.write_long(sp + 4, key_ptr);
        bus.write_long(sp + 8, 0);
        bus.write_long(sp + 12, table_handle);
        bus.write_word(sp + 16, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(bus.read_long(fetched_value_ptr), accessor_ptr);
        assert_eq!(bus.read_long(fetched_value_ptr + 4), accessor_refcon);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0632);
        bus.write_long(sp, key_ptr);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, table_handle);
        bus.write_word(sp + 12, 0xBEEF);
        let remove = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(remove.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 12), 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0833);
        bus.write_long(sp, fetched_value_ptr);
        bus.write_long(sp + 4, key_ptr);
        bus.write_long(sp + 8, 0);
        bus.write_long(sp + 12, table_handle);
        bus.write_word(sp + 16, 0xBEEF);
        let missing = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(missing.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 16), AE_ERR_ACCESSOR_NOT_FOUND as u16);
    }

    #[test]
    fn pack8_descriptor_list_put_count_get_and_size_round_trip() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let list_desc = 0x0030_0000u32;
        let source_desc = 0x0030_0100u32;
        let result_desc = 0x0030_0200u32;
        let source_data = 0x0030_0300u32;
        let count_ptr = 0x0030_0400u32;
        let keyword_ptr = 0x0030_0410u32;
        let type_ptr = 0x0030_0420u32;
        let out_data = 0x0030_0430u32;
        let actual_size_ptr = 0x0030_0440u32;
        let value = 0x1122_3344u32;

        cpu.write_reg(Register::D0, 0x0706); // AECreateList(..., isRecord=FALSE)
        bus.write_long(sp, list_desc);
        bus.write_word(sp + 4, 0);
        bus.write_long(sp + 6, 0);
        bus.write_long(sp + 10, 0);
        bus.write_word(sp + 14, 0xBEEF);
        let create_list = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create_list.unwrap().is_ok());

        bus.write_long(source_data, value);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0825); // AECreateDesc
        bus.write_long(sp, source_desc);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, source_data);
        bus.write_long(sp + 12, u32::from_be_bytes(*b"long"));
        bus.write_word(sp + 16, 0xBEEF);
        let create_desc = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create_desc.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0609); // AEPutDesc append
        bus.write_long(sp, source_desc);
        bus.write_long(sp + 4, 0);
        bus.write_long(sp + 8, list_desc);
        bus.write_word(sp + 12, 0xBEEF);
        let put = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0407); // AECountItems
        bus.write_long(sp, count_ptr);
        bus.write_long(sp + 4, list_desc);
        bus.write_word(sp + 8, 0xBEEF);
        let count = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(count.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(bus.read_long(count_ptr), 1);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0A0B); // AEGetNthDesc
        bus.write_long(sp, result_desc);
        bus.write_long(sp + 4, keyword_ptr);
        bus.write_long(sp + 8, AE_TYPE_WILDCARD);
        bus.write_long(sp + 12, 1);
        bus.write_long(sp + 16, list_desc);
        bus.write_word(sp + 20, 0xBEEF);
        let get_desc = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get_desc.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 20), 0);
        assert_eq!(bus.read_long(keyword_ptr), AE_TYPE_WILDCARD);
        assert_eq!(bus.read_long(result_desc), u32::from_be_bytes(*b"long"));

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x100A); // AEGetNthPtr
        bus.write_long(sp, actual_size_ptr);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, out_data);
        bus.write_long(sp + 12, type_ptr);
        bus.write_long(sp + 16, keyword_ptr);
        bus.write_long(sp + 20, AE_TYPE_WILDCARD);
        bus.write_long(sp + 24, 1);
        bus.write_long(sp + 28, list_desc);
        bus.write_word(sp + 32, 0xBEEF);
        let get_ptr = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get_ptr.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 32), 0);
        assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"long"));
        assert_eq!(bus.read_long(actual_size_ptr), 4);
        assert_eq!(bus.read_long(out_data), value);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x082A); // AESizeOfNthItem
        bus.write_long(sp, actual_size_ptr);
        bus.write_long(sp + 4, type_ptr);
        bus.write_long(sp + 8, 1);
        bus.write_long(sp + 12, list_desc);
        bus.write_word(sp + 16, 0xBEEF);
        let size = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(size.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"long"));
        assert_eq!(bus.read_long(actual_size_ptr), 4);
    }

    #[test]
    fn pack8_aesizeofparam_and_attribute_report_type_and_size() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_desc = 0x0030_0000u32;
        let value_ptr = 0x0030_0100u32;
        let type_ptr = 0x0030_0200u32;
        let size_ptr = 0x0030_0300u32;
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");
        let direct_object = u32::from_be_bytes(*b"----");

        cpu.write_reg(Register::D0, 0x0B14); // AECreateAppleEvent
        bus.write_long(sp, event_desc);
        bus.write_long(sp + 4, 0);
        bus.write_word(sp + 8, 0xFFFF);
        bus.write_long(sp + 10, 0x0030_0400);
        bus.write_long(sp + 14, event_id);
        bus.write_long(sp + 18, event_class);
        bus.write_word(sp + 22, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create.unwrap().is_ok());

        bus.write_long(value_ptr, 0x5566_7788);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0A0F); // AEPutParamPtr
        bus.write_long(sp, 4);
        bus.write_long(sp + 4, value_ptr);
        bus.write_long(sp + 8, u32::from_be_bytes(*b"long"));
        bus.write_long(sp + 12, direct_object);
        bus.write_long(sp + 16, event_desc);
        bus.write_word(sp + 20, 0xBEEF);
        let put = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0829); // AESizeOfParam
        bus.write_long(sp, size_ptr);
        bus.write_long(sp + 4, type_ptr);
        bus.write_long(sp + 8, direct_object);
        bus.write_long(sp + 12, event_desc);
        bus.write_word(sp + 16, 0xBEEF);
        let size_param = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(size_param.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(bus.read_long(type_ptr), u32::from_be_bytes(*b"long"));
        assert_eq!(bus.read_long(size_ptr), 4);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0828); // AESizeOfAttribute
        bus.write_long(sp, size_ptr);
        bus.write_long(sp + 4, type_ptr);
        bus.write_long(sp + 8, AE_KEY_EVENT_ID_ATTR);
        bus.write_long(sp + 12, event_desc);
        bus.write_word(sp + 16, 0xBEEF);
        let size_attr = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(size_attr.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(bus.read_long(type_ptr), AE_TYPE_TYPE);
        assert_eq!(bus.read_long(size_ptr), 4);
    }

    #[test]
    fn pack8_aeprocessappleevent_dispatches_matching_event_to_installed_handler() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"aevt");
        let event_id = u32::from_be_bytes(*b"oapp");
        let handler_ptr = 0x0040_8000u32;
        let handler_refcon = 0xDEAD_BEEFu32;
        let event_record_ptr = 0x0032_0000u32;
        let return_pc = 0x00F0_1234u32;

        // Install OAPP handler first (selector $091F).
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, handler_refcon);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());

        // Selector $021B => AEProcessAppleEvent(eventRecord).
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::D0, 0x021B);
        bus.write_long(sp, event_record_ptr);
        bus.write_word(sp + 4, 0xBEEF); // OSErr result slot

        let process = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(process.is_some());
        assert!(process.unwrap().is_ok());

        // AEProcessAppleEvent dispatches to the matching handler.
        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        assert_eq!(cpu.read_reg(Register::A7), sp - 12);
        assert!(disp.fired_oapp_handler);

        let state = disp
            .ae_call_state
            .clone()
            .expect("expected in-flight AE call state");
        assert_eq!(state.return_pc, return_pc);
        assert_eq!(state.expected_sp_after_rtd, sp + 4);
        assert_eq!(
            bus.read_long(sp - 12),
            disp.ae_trampoline_addr
                .expect("trampoline should be allocated")
        );
        assert_eq!(bus.read_long(sp - 8), handler_refcon);
        assert_ne!(bus.read_long(sp - 4), 0, "reply AEDesc pointer");

        let passed_event_desc = bus.read_long(sp);
        let passed_reply_desc = bus.read_long(sp - 4);
        assert_eq!(
            state.owned_descriptors,
            Some((passed_event_desc, passed_reply_desc))
        );
        assert_eq!(bus.get_alloc_size(passed_event_desc), Some(8));
        assert_eq!(bus.get_alloc_size(passed_reply_desc), Some(8));
        assert_ne!(
            passed_event_desc, event_record_ptr,
            "handler must receive an AppleEvent AEDesc, not the source EventRecord"
        );
        assert_eq!(bus.read_long(passed_event_desc), event_class);
        assert_ne!(bus.read_long(passed_event_desc + 4), 0);
    }

    #[test]
    fn pack8_aeprocessappleevent_dispatches_double_wildcard_handler() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let handler_ptr = 0x0040_8800u32;
        let handler_refcon = 0xCAFE_BABEu32;
        let return_pc = 0x00F0_2468u32;

        // Install a typeWildCard/typeWildCard handler. Inside Macintosh
        // Volume VI, 6-28 to 6-29 says this entry receives all Apple events.
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, handler_refcon);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, AE_TYPE_WILDCARD);
        bus.write_long(sp + 14, AE_TYPE_WILDCARD);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::D0, 0x021B);
        bus.write_long(sp, 0x0032_0000);
        bus.write_word(sp + 4, 0xBEEF);
        let process = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(process.is_some());
        assert!(process.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        assert_eq!(cpu.read_reg(Register::A7), sp - 12);
        assert_eq!(bus.read_long(sp - 8), handler_refcon);
        assert!(disp.fired_oapp_handler);

        let state = disp
            .ae_call_state
            .clone()
            .expect("expected in-flight AE call state");
        let (event_desc, reply_desc) = state
            .owned_descriptors
            .expect("AEProcessAppleEvent owns its callback descriptors");
        let event_handle = bus.read_long(event_desc + 4);
        let event_data = bus.read_long(event_handle);
        assert_eq!(bus.get_alloc_size(event_desc), Some(8));
        assert_eq!(bus.get_alloc_size(reply_desc), Some(8));
        assert_eq!(bus.get_alloc_size(event_handle), Some(4));
        assert_eq!(bus.get_alloc_size(event_data), Some(8));
        assert_eq!(state.return_pc, return_pc);
        assert_eq!(state.expected_sp_after_rtd, sp + 4);
    }

    #[test]
    fn pack8_aecreateappleevent_records_event_class_and_id_attributes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_desc = 0x0030_0000u32;
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");

        // Selector $0B14 => AECreateAppleEvent(class, id, target,
        // returnID, transactionID, result).
        cpu.write_reg(Register::D0, 0x0B14);
        bus.write_long(sp, event_desc);
        bus.write_long(sp + 4, 0); // transactionID
        bus.write_word(sp + 8, 0xFFFF); // kAutoGenerateReturnID
        bus.write_long(sp + 10, 0x0030_0100); // target AEDesc pointer
        bus.write_long(sp + 14, event_id);
        bus.write_long(sp + 18, event_class);
        bus.write_word(sp + 22, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 22), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
        assert_eq!(bus.read_long(event_desc), AE_TYPE_APPLE_EVENT);
        assert_ne!(bus.read_long(event_desc + 4), 0);
        let stored = disp
            .ae_descriptor_state
            .events
            .get(&event_desc)
            .expect("AECreateAppleEvent should record event attributes");
        assert_eq!(stored.event_class, event_class);
        assert_eq!(stored.event_id, event_id);
    }

    #[test]
    fn pack8_aesend_dispatches_same_process_event_to_installed_handler() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");
        let handler_ptr = 0x0040_B000u32;
        let handler_refcon = 0x0000_0FA2u32;
        let event_desc = 0x0030_0000u32;
        let reply_desc = 0x0030_0100u32;
        let return_pc = 0x00F0_1357u32;
        let handler_result = (-1708i16) as u16; // errAEEventNotHandled

        // Install the command handler.
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, handler_refcon);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());

        // Create the AppleEvent descriptor that AESend will dispatch.
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0B14);
        bus.write_long(sp, event_desc);
        bus.write_long(sp + 4, 0); // transactionID
        bus.write_word(sp + 8, 0xFFFF); // kAutoGenerateReturnID
        bus.write_long(sp + 10, 0x0030_0200); // target AEDesc pointer
        bus.write_long(sp + 14, event_id);
        bus.write_long(sp + 18, event_class);
        bus.write_word(sp + 22, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create.is_some());
        assert!(create.unwrap().is_ok());

        // Selector $0D17 => AESend(event, reply, kAENoReply, ...).
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::D0, 0x0D17);
        bus.write_long(sp, 0); // filterProc
        bus.write_long(sp + 4, 0); // idleProc
        bus.write_long(sp + 8, 0xFFFF_FFFF); // kAEDefaultTimeout
        bus.write_word(sp + 12, 0); // kAENormalPriority
        bus.write_long(sp + 14, 1); // kAENoReply
        bus.write_long(sp + 18, reply_desc);
        bus.write_long(sp + 22, event_desc);
        bus.write_word(sp + 26, 0xBEEF);

        let send = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(send.is_some());
        assert!(send.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_long(sp + 10), disp.ae_trampoline_addr.unwrap());
        assert_eq!(bus.read_long(sp + 14), handler_refcon);
        assert_eq!(bus.read_long(sp + 18), reply_desc);
        assert_eq!(bus.read_long(sp + 22), event_desc);
        assert_eq!(bus.read_long(reply_desc), AE_TYPE_NULL);

        let state = disp
            .ae_call_state
            .clone()
            .expect("expected in-flight AESend handler call");
        assert_eq!(state.return_pc, return_pc);
        assert_eq!(state.expected_sp_after_rtd, sp + 26);
        assert_eq!(state.result_override, Some(0));

        // Handler failure is a reply concern; AESend itself reports noErr
        // once the same-process event has been delivered to the handler.
        bus.write_word(state.expected_sp_after_rtd, handler_result);
        cpu.write_reg(Register::A7, state.expected_sp_after_rtd);
        cpu.write_reg(Register::D0, 0xFEFE);
        let trampoline = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(trampoline.is_some());
        assert!(trampoline.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::A7), state.expected_sp_after_rtd);
        assert_eq!(bus.read_word(state.expected_sp_after_rtd), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(disp.ae_call_state.is_none());
    }

    #[test]
    fn pack8_aesend_wait_reply_provides_reply_event_to_handler() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let scratch_sp = TEST_SP - 0x100;
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");
        let direct_object = u32::from_be_bytes(*b"----");
        let handler_ptr = 0x0040_B000u32;
        let event_desc = 0x0030_0000u32;
        let reply_desc = 0x0030_0100u32;
        let target_desc = 0x0030_0200u32;
        let value_ptr = 0x0030_0300u32;
        let out_data = 0x0030_0400u32;
        let type_code_ptr = 0x0030_0500u32;
        let actual_size_ptr = 0x0030_0600u32;
        let return_pc = 0x00F0_1357u32;
        let reply_value = 0x1234_5678u32;

        cpu.write_reg(Register::D0, 0x091F); // AEInstallEventHandler
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, 0);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0B14); // AECreateAppleEvent
        bus.write_long(sp, event_desc);
        bus.write_long(sp + 4, 0);
        bus.write_word(sp + 8, 0xFFFF);
        bus.write_long(sp + 10, target_desc);
        bus.write_long(sp + 14, event_id);
        bus.write_long(sp + 18, event_class);
        bus.write_word(sp + 22, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::D0, 0x0D17); // AESend
        bus.write_long(sp, 0); // filterProc
        bus.write_long(sp + 4, 0); // idleProc
        bus.write_long(sp + 8, 0xFFFF_FFFF); // kAEDefaultTimeout
        bus.write_word(sp + 12, 0); // kAENormalPriority
        bus.write_long(sp + 14, AE_SEND_MODE_WAIT_REPLY);
        bus.write_long(sp + 18, reply_desc);
        bus.write_long(sp + 22, event_desc);
        bus.write_word(sp + 26, 0xBEEF);
        let send = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(send.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        assert_eq!(bus.read_long(reply_desc), AE_TYPE_APPLE_EVENT);
        let reply_event = disp
            .ae_descriptor_state
            .events
            .get(&reply_desc)
            .expect("wait-reply AESend should provide a reply AppleEvent");
        assert_eq!(reply_event.event_class, AE_TYPE_APPLE_EVENT);
        assert_eq!(reply_event.event_id, AE_EVENT_ID_ANSWER);

        bus.write_long(value_ptr, reply_value);
        cpu.write_reg(Register::A7, scratch_sp);
        cpu.write_reg(Register::D0, 0x0A0F); // AEPutParamPtr
        bus.write_long(scratch_sp, 4);
        bus.write_long(scratch_sp + 4, value_ptr);
        bus.write_long(scratch_sp + 8, AE_TYPE_TYPE);
        bus.write_long(scratch_sp + 12, direct_object);
        bus.write_long(scratch_sp + 16, reply_desc);
        bus.write_word(scratch_sp + 20, 0xBEEF);
        let put = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());
        assert_eq!(bus.read_word(scratch_sp + 20), 0);

        let state = disp
            .ae_call_state
            .clone()
            .expect("expected in-flight AESend handler call");
        bus.write_word(state.expected_sp_after_rtd, 0);
        cpu.write_reg(Register::A7, state.expected_sp_after_rtd);
        cpu.write_reg(Register::D0, 0xFEFE);
        let trampoline = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(trampoline.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        cpu.write_reg(Register::A7, scratch_sp);
        cpu.write_reg(Register::D0, 0x0E11); // AEGetParamPtr
        bus.write_long(scratch_sp, actual_size_ptr);
        bus.write_long(scratch_sp + 4, 4);
        bus.write_long(scratch_sp + 8, out_data);
        bus.write_long(scratch_sp + 12, type_code_ptr);
        bus.write_long(scratch_sp + 16, AE_TYPE_WILDCARD);
        bus.write_long(scratch_sp + 20, direct_object);
        bus.write_long(scratch_sp + 24, reply_desc);
        bus.write_word(scratch_sp + 28, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_word(scratch_sp + 28), 0);
        assert_eq!(bus.read_long(type_code_ptr), AE_TYPE_TYPE);
        assert_eq!(bus.read_long(actual_size_ptr), 4);
        assert_eq!(bus.read_long(out_data), reply_value);
    }

    #[test]
    fn pack8_aesend_nested_handler_restores_outer_ae_call_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");
        let handler_ptr = 0x0040_C000u32;
        let handler_refcon = 0x0000_0FA2u32;
        let event_desc = 0x0030_0000u32;
        let reply_desc = 0x0030_0100u32;
        let outer_event_desc = disp.new_process_classic_ptr(&mut bus, 8);
        let outer_reply_desc = disp.new_process_classic_ptr(&mut bus, 8);
        super::write_null_aedesc(&mut bus, outer_event_desc);
        super::write_null_aedesc(&mut bus, outer_reply_desc);
        let outer_state = crate::trap::dispatch::AeCallState {
            return_pc: 0x00F0_9999,
            expected_sp_after_rtd: 0x0010_0100,
            result_override: None,
            owned_descriptors: Some((outer_event_desc, outer_reply_desc)),
            resolve_state: None,
        };

        disp.ae_handlers.install(
            false,
            event_class,
            event_id,
            crate::process_context::ProcessAppleEventHandler {
                procedure: crate::guest_procedure::GuestProcedure::raw_m68k(handler_ptr),
                refcon: handler_refcon,
            },
        );
        disp.ae_descriptor_state.with_mut(|state| state.events.insert(
            event_desc,
            crate::trap::dispatch::SyntheticAppleEvent {
                event_class,
                event_id,
                params: HashMap::new(),
                items: Vec::new(),
            },
        ));
        disp.ae_call_state = Some(outer_state.clone());

        cpu.write_reg(Register::PC, 0x00F0_2468);
        cpu.write_reg(Register::D0, 0x0D17);
        bus.write_long(sp, 0); // filterProc
        bus.write_long(sp + 4, 0); // idleProc
        bus.write_long(sp + 8, 0xFFFF_FFFF); // timeout
        bus.write_word(sp + 12, 0); // priority
        bus.write_long(sp + 14, 1); // kAENoReply
        bus.write_long(sp + 18, reply_desc);
        bus.write_long(sp + 22, event_desc);
        bus.write_word(sp + 26, 0xBEEF);

        let send = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(send.is_some());
        assert!(send.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        assert_eq!(disp.ae_call_state_stack.len(), 1);
        assert_eq!(disp.ae_call_state_stack[0].return_pc, outer_state.return_pc);

        let inner_state = disp
            .ae_call_state
            .clone()
            .expect("expected nested AESend call state");
        bus.write_word(inner_state.expected_sp_after_rtd, 0);
        cpu.write_reg(Register::A7, inner_state.expected_sp_after_rtd);
        cpu.write_reg(Register::D0, 0xFEFE);
        let trampoline = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(trampoline.is_some());
        assert!(trampoline.unwrap().is_ok());

        let restored = disp
            .ae_call_state
            .as_ref()
            .expect("outer AE call state should be restored");
        assert_eq!(restored.return_pc, outer_state.return_pc);
        assert_eq!(
            restored.expected_sp_after_rtd,
            outer_state.expected_sp_after_rtd
        );
        assert_eq!(bus.get_alloc_size(outer_event_desc), Some(8));
        assert_eq!(bus.get_alloc_size(outer_reply_desc), Some(8));
        assert!(disp.ae_call_state_stack.is_empty());

        bus.write_word(outer_state.expected_sp_after_rtd, 0);
        cpu.write_reg(Register::A7, outer_state.expected_sp_after_rtd);
        cpu.write_reg(Register::D0, 0xFEFE);
        let outer_trampoline = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(outer_trampoline.unwrap().is_ok());
        assert_eq!(bus.get_alloc_size(outer_event_desc), None);
        assert_eq!(bus.get_alloc_size(outer_reply_desc), None);
        assert!(disp.ae_call_state.is_none());
    }

    #[test]
    fn pack8_aegetparamdesc_returns_putparamdesc_object_specifier() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_desc = 0x0030_0000u32;
        let record_desc = 0x0030_0100u32;
        let object_desc = 0x0030_0200u32;
        let result_desc = 0x0030_0300u32;
        let event_class = u32::from_be_bytes(*b"misc");
        let event_id = u32::from_be_bytes(*b"slct");
        let direct_object = u32::from_be_bytes(*b"----");

        // AECreateAppleEvent(..., event_desc)
        cpu.write_reg(Register::D0, 0x0B14);
        bus.write_long(sp, event_desc);
        bus.write_long(sp + 4, 0);
        bus.write_word(sp + 8, 0xFFFF);
        bus.write_long(sp + 10, 0x0030_0400);
        bus.write_long(sp + 14, event_id);
        bus.write_long(sp + 18, event_class);
        bus.write_word(sp + 22, 0xBEEF);
        let create_event = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create_event.unwrap().is_ok());

        // AECreateList(..., isRecord=TRUE, record_desc)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0706);
        bus.write_long(sp, record_desc);
        bus.write_word(sp + 4, 0x0100);
        bus.write_long(sp + 6, 0);
        bus.write_long(sp + 10, 0);
        bus.write_word(sp + 14, 0xBEEF);
        let create_record = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create_record.unwrap().is_ok());

        // AECoerceDesc(record_desc, typeObjectSpecifier, object_desc)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0603);
        bus.write_long(sp, object_desc);
        bus.write_long(sp + 4, AE_TYPE_OBJECT_SPECIFIER);
        bus.write_long(sp + 8, record_desc);
        bus.write_word(sp + 12, 0xBEEF);
        let coerce = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(coerce.unwrap().is_ok());
        assert_eq!(bus.read_long(object_desc), AE_TYPE_OBJECT_SPECIFIER);

        // AEPutParamDesc(event_desc, keyDirectObject, object_desc)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0610);
        bus.write_long(sp, object_desc);
        bus.write_long(sp + 4, direct_object);
        bus.write_long(sp + 8, event_desc);
        bus.write_word(sp + 12, 0xBEEF);
        let put = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        // AEGetParamDesc(event_desc, keyDirectObject, typeWildCard, result)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0812);
        bus.write_long(sp, result_desc);
        bus.write_long(sp + 4, AE_TYPE_WILDCARD);
        bus.write_long(sp + 8, direct_object);
        bus.write_long(sp + 12, event_desc);
        bus.write_word(sp + 16, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(bus.read_long(result_desc), AE_TYPE_OBJECT_SPECIFIER);
        assert!(disp.ae_descriptor_state.descriptors.contains_key(&result_desc));
    }

    #[test]
    fn pack8_aecreatedesc_uses_handle_backed_payload() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let source_data = 0x0030_0000u32;
        let result_desc = 0x0030_0100u32;
        let value = 0x0000_0001u32;

        bus.write_long(source_data, value);
        cpu.write_reg(Register::D0, 0x0825); // AECreateDesc
        bus.write_long(sp, result_desc);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, source_data);
        bus.write_long(sp + 12, u32::from_be_bytes(*b"long"));
        bus.write_word(sp + 16, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());

        let data_handle = bus.read_long(result_desc + 4);
        let data_ptr = bus.read_long(data_handle);
        assert_ne!(data_handle, 0);
        assert_ne!(data_ptr, 0);
        assert_eq!(bus.read_long(data_ptr), value);
        assert_eq!(disp.handle_for_ptr(data_ptr), Some(data_handle));
    }

    #[test]
    fn pack8_aerecord_fields_follow_shared_descriptor_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let record_desc = 0x0030_0000u32;
        let record_copy = 0x0030_0100u32;
        let value_ptr = 0x0030_0200u32;
        let type_code_ptr = 0x0030_0300u32;
        let out_data = 0x0030_0400u32;
        let actual_size_ptr = 0x0030_0500u32;
        let desired_class = u32::from_be_bytes(*b"cwin");

        cpu.write_reg(Register::D0, 0x0706); // AECreateList(..., isRecord=TRUE)
        bus.write_long(sp, record_desc);
        bus.write_word(sp + 4, 0x0100);
        bus.write_long(sp + 6, 0);
        bus.write_long(sp + 10, 0);
        bus.write_word(sp + 14, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create.unwrap().is_ok());

        // AE records are commonly copied by value. The copy shares the same
        // data handle, so adding a key to the copy must be visible through
        // the original descriptor record too.
        bus.write_long(record_copy, bus.read_long(record_desc));
        bus.write_long(record_copy + 4, bus.read_long(record_desc + 4));
        bus.write_long(value_ptr, desired_class);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0A0F); // AEPutKeyPtr
        bus.write_long(sp, 4);
        bus.write_long(sp + 4, value_ptr);
        bus.write_long(sp + 8, AE_TYPE_TYPE);
        bus.write_long(sp + 12, AE_KEY_DESIRED_CLASS);
        bus.write_long(sp + 16, record_copy);
        bus.write_word(sp + 20, 0xBEEF);
        let put = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0E11); // AEGetKeyPtr
        bus.write_long(sp, actual_size_ptr);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, out_data);
        bus.write_long(sp + 12, type_code_ptr);
        bus.write_long(sp + 16, AE_TYPE_WILDCARD);
        bus.write_long(sp + 20, AE_KEY_DESIRED_CLASS);
        bus.write_long(sp + 24, record_desc);
        bus.write_word(sp + 28, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 28), 0);
        assert_eq!(bus.read_long(type_code_ptr), AE_TYPE_TYPE);
        assert_eq!(bus.read_long(actual_size_ptr), 4);
        assert_eq!(bus.read_long(out_data), desired_class);
    }

    #[test]
    fn pack8_aedisposedesc_keeps_shared_record_backing_for_value_copies() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let record_desc = 0x0030_0000u32;
        let record_copy = 0x0030_0100u32;
        let value_ptr = 0x0030_0200u32;
        let type_code_ptr = 0x0030_0300u32;
        let out_data = 0x0030_0400u32;
        let actual_size_ptr = 0x0030_0500u32;
        let desired_class = u32::from_be_bytes(*b"cwin");

        cpu.write_reg(Register::D0, 0x0706); // AECreateList(..., isRecord=TRUE)
        bus.write_long(sp, record_desc);
        bus.write_word(sp + 4, 0x0100);
        bus.write_long(sp + 6, 0);
        bus.write_long(sp + 10, 0);
        bus.write_word(sp + 14, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create.unwrap().is_ok());

        let data_handle = bus.read_long(record_desc + 4);
        bus.write_long(record_copy, bus.read_long(record_desc));
        bus.write_long(record_copy + 4, data_handle);
        bus.write_long(value_ptr, desired_class);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0A0F); // AEPutKeyPtr
        bus.write_long(sp, 4);
        bus.write_long(sp + 4, value_ptr);
        bus.write_long(sp + 8, AE_TYPE_TYPE);
        bus.write_long(sp + 12, AE_KEY_DESIRED_CLASS);
        bus.write_long(sp + 16, record_copy);
        bus.write_word(sp + 20, 0xBEEF);
        let put = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(put.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0204); // AEDisposeDesc
        bus.write_long(sp, record_copy);
        bus.write_word(sp + 4, 0xBEEF);
        let dispose = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(dispose.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(bus.read_long(record_copy), AE_TYPE_NULL);
        assert_eq!(bus.read_long(record_copy + 4), 0);
        assert_eq!(bus.read_long(record_desc + 4), data_handle);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0E11); // AEGetKeyPtr
        bus.write_long(sp, actual_size_ptr);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, out_data);
        bus.write_long(sp + 12, type_code_ptr);
        bus.write_long(sp + 16, AE_TYPE_WILDCARD);
        bus.write_long(sp + 20, AE_KEY_DESIRED_CLASS);
        bus.write_long(sp + 24, record_desc);
        bus.write_word(sp + 28, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 28), 0);
        assert_eq!(bus.read_long(type_code_ptr), AE_TYPE_TYPE);
        assert_eq!(bus.read_long(actual_size_ptr), 4);
        assert_eq!(bus.read_long(out_data), desired_class);
    }

    #[test]
    fn pack8_aeresolve_dispatches_nested_object_accessors() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let object_specifier = 0x0030_0000u32;
        let final_token = 0x0030_0100u32;
        let handler_ptr = 0x0040_D000u32;
        let return_pc = 0x00F0_7777u32;
        let token_type = u32::from_be_bytes(*b"Toke");
        let docu = u32::from_be_bytes(*b"docu");
        let cwin = u32::from_be_bytes(*b"cwin");
        let form_absolute_position = u32::from_be_bytes(*b"indx");

        let mut inner_fields = HashMap::new();
        inner_fields.insert(
            AE_KEY_DESIRED_CLASS,
            AeDescriptor {
                desc_type: AE_TYPE_TYPE,
                data: docu.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        inner_fields.insert(
            AE_KEY_CONTAINER,
            AeDescriptor {
                desc_type: AE_TYPE_NULL,
                data: Vec::new(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        inner_fields.insert(
            AE_KEY_KEY_FORM,
            AeDescriptor {
                desc_type: u32::from_be_bytes(*b"enum"),
                data: form_absolute_position.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        inner_fields.insert(
            AE_KEY_KEY_DATA,
            AeDescriptor {
                desc_type: u32::from_be_bytes(*b"long"),
                data: 1u32.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        let inner_object = AeDescriptor {
            desc_type: AE_TYPE_OBJECT_SPECIFIER,
            data: Vec::new(),
            fields: inner_fields,
            items: Vec::new(),
        };

        let mut outer_fields = HashMap::new();
        outer_fields.insert(
            AE_KEY_DESIRED_CLASS,
            AeDescriptor {
                desc_type: AE_TYPE_TYPE,
                data: cwin.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        outer_fields.insert(AE_KEY_CONTAINER, inner_object);
        outer_fields.insert(
            AE_KEY_KEY_FORM,
            AeDescriptor {
                desc_type: u32::from_be_bytes(*b"enum"),
                data: form_absolute_position.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        outer_fields.insert(
            AE_KEY_KEY_DATA,
            AeDescriptor {
                desc_type: u32::from_be_bytes(*b"long"),
                data: 1u32.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        disp.write_ae_descriptor_value(
            &mut bus,
            object_specifier,
            AeDescriptor {
                desc_type: AE_TYPE_OBJECT_SPECIFIER,
                data: Vec::new(),
                fields: outer_fields,
                items: Vec::new(),
            },
        );

        cpu.write_reg(Register::D0, 0x0937); // AEInstallObjectAccessor
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, 0x1234_5678);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, AE_TYPE_WILDCARD);
        bus.write_long(sp + 14, AE_TYPE_WILDCARD);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::D0, 0x0536); // AEResolve
        bus.write_long(sp, final_token);
        bus.write_word(sp + 4, 0);
        bus.write_long(sp + 6, object_specifier);
        bus.write_word(sp + 10, 0xBEEF);
        let resolve = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(resolve.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        let first_sp = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_long(first_sp + 28), docu);
        assert_eq!(bus.read_long(bus.read_long(first_sp + 24)), AE_TYPE_NULL);

        let first_state = disp
            .ae_call_state
            .clone()
            .expect("expected first accessor call")
            .resolve_state
            .expect("expected resolve continuation");
        disp.write_ae_descriptor_value(
            &mut bus,
            first_state.current_token_desc,
            AeDescriptor {
                desc_type: token_type,
                data: 0x1111_2222u32.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        bus.write_word(first_state.result_slot, 0);
        cpu.write_reg(Register::A7, first_state.result_slot);
        cpu.write_reg(Register::D0, 0xFEFE);
        let tramp1 = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(tramp1.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        let second_sp = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_long(second_sp + 28), cwin);
        let container_desc = bus.read_long(second_sp + 24);
        assert_eq!(bus.read_long(container_desc), token_type);

        let second_state = disp
            .ae_call_state
            .clone()
            .expect("expected second accessor call")
            .resolve_state
            .expect("expected second resolve continuation");
        assert_eq!(second_state.current_token_desc, final_token);
        disp.write_ae_descriptor_value(
            &mut bus,
            second_state.current_token_desc,
            AeDescriptor {
                desc_type: token_type,
                data: 0x3333_4444u32.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        bus.write_word(second_state.result_slot, 0);
        cpu.write_reg(Register::A7, second_state.result_slot);
        cpu.write_reg(Register::D0, 0xFEFE);
        let tramp2 = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(tramp2.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(bus.read_long(final_token), token_type);
        assert!(disp.ae_call_state.is_none());
    }

    #[test]
    fn pack8_object_accessor_get_remove_and_direct_call_round_trip() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let desired_class = u32::from_be_bytes(*b"cwin");
        let container_class = u32::from_be_bytes(*b"docu");
        let container_type = u32::from_be_bytes(*b"Toke");
        let key_form = u32::from_be_bytes(*b"indx");
        let handler_ptr = 0x0040_D000u32;
        let handler_refcon = 0x0102_0304u32;
        let out_accessor_ptr = 0x0030_0000u32;
        let out_refcon_ptr = 0x0030_0010u32;
        let container_desc = 0x0030_0020u32;
        let key_data_desc = 0x0030_0030u32;
        let final_token = 0x0030_0040u32;
        let key_data_value = 0x0030_0050u32;
        let return_pc = 0x00F0_8888u32;

        cpu.write_reg(Register::D0, 0x0937); // AEInstallObjectAccessor
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, handler_refcon);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, container_type);
        bus.write_long(sp + 14, desired_class);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0939); // AEGetObjectAccessor
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, out_refcon_ptr);
        bus.write_long(sp + 6, out_accessor_ptr);
        bus.write_long(sp + 10, container_type);
        bus.write_long(sp + 14, desired_class);
        bus.write_word(sp + 18, 0xBEEF);
        let get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(get.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 18), 0);
        assert_eq!(bus.read_long(out_accessor_ptr), handler_ptr);
        assert_eq!(bus.read_long(out_refcon_ptr), handler_refcon);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0939);
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, out_refcon_ptr);
        bus.write_long(sp + 6, out_accessor_ptr);
        bus.write_long(sp + 10, container_type);
        bus.write_long(sp + 14, AE_TYPE_WILDCARD);
        bus.write_word(sp + 18, 0xBEEF);
        let wildcard_get = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(wildcard_get.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 18), AE_ERR_ACCESSOR_NOT_FOUND as u16);
        assert_eq!(bus.read_long(out_accessor_ptr), 0);
        assert_eq!(bus.read_long(out_refcon_ptr), 0);

        disp.write_ae_descriptor_value(
            &mut bus,
            container_desc,
            AeDescriptor {
                desc_type: container_type,
                data: 0x1111_2222u32.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        bus.write_long(key_data_value, 1);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0825); // AECreateDesc
        bus.write_long(sp, key_data_desc);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, key_data_value);
        bus.write_long(sp + 12, u32::from_be_bytes(*b"long"));
        bus.write_word(sp + 16, 0xBEEF);
        let create_key = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create_key.unwrap().is_ok());

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::D0, 0x0C3B); // AECallObjectAccessor
        bus.write_long(sp, final_token);
        bus.write_long(sp + 4, key_data_desc);
        bus.write_long(sp + 8, key_form);
        bus.write_long(sp + 12, container_class);
        bus.write_long(sp + 16, container_desc);
        bus.write_long(sp + 20, desired_class);
        bus.write_word(sp + 24, 0xBEEF);
        let call = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(call.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        let callback_sp = cpu.read_reg(Register::A7);
        assert_eq!(bus.read_long(callback_sp + 4), handler_refcon);
        assert_eq!(bus.read_long(callback_sp + 8), final_token);
        assert_eq!(
            bus.read_long(bus.read_long(callback_sp + 12)),
            u32::from_be_bytes(*b"long")
        );
        assert_eq!(bus.read_long(callback_sp + 16), key_form);
        assert_eq!(bus.read_long(callback_sp + 20), container_class);
        assert_eq!(
            bus.read_long(bus.read_long(callback_sp + 24)),
            container_type
        );
        assert_eq!(bus.read_long(callback_sp + 28), desired_class);

        let state = disp
            .ae_call_state
            .clone()
            .expect("expected direct accessor call")
            .resolve_state
            .expect("expected accessor continuation");
        assert_eq!(state.current_token_desc, final_token);
        disp.write_ae_descriptor_value(
            &mut bus,
            final_token,
            AeDescriptor {
                desc_type: container_type,
                data: 0x3333_4444u32.to_be_bytes().to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            },
        );
        bus.write_word(state.result_slot, 0);
        cpu.write_reg(Register::A7, state.result_slot);
        cpu.write_reg(Register::D0, 0xFEFE);
        let resume = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(resume.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(bus.read_word(sp + 24), 0);
        assert_eq!(bus.read_long(final_token), container_type);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0738); // AERemoveObjectAccessor
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, handler_ptr.wrapping_add(2));
        bus.write_long(sp + 6, container_type);
        bus.write_long(sp + 10, desired_class);
        bus.write_word(sp + 14, 0xBEEF);
        let wrong_remove = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(wrong_remove.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 14), AE_ERR_ACCESSOR_NOT_FOUND as u16);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0738);
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, 0);
        bus.write_long(sp + 6, container_type);
        bus.write_long(sp + 10, desired_class);
        bus.write_word(sp + 14, 0xBEEF);
        let remove = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(remove.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 14), 0);
        assert!(!disp
            .ae_object_accessors
            .contains_key(&(false, desired_class, container_type)));
    }

    #[test]
    fn pack8_aedisposetoken_falls_back_to_dispose_desc_without_callback() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let token_desc = 0x0030_0000u32;
        let token_data = 0x0030_0100u32;

        bus.write_long(token_data, 0x1122_3344);
        cpu.write_reg(Register::D0, 0x0825); // AECreateDesc
        bus.write_long(sp, token_desc);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, token_data);
        bus.write_long(sp + 12, u32::from_be_bytes(*b"Toke"));
        bus.write_word(sp + 16, 0xBEEF);
        let create = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(create.unwrap().is_ok());

        let data_handle = bus.read_long(token_desc + 4);
        let data_ptr = bus.read_long(data_handle);
        assert!(disp.ae_descriptor_state.descriptors.contains_key(&token_desc));
        assert!(disp.has_handle_ptr(data_ptr));

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x023A); // AEDisposeToken
        bus.write_long(sp, token_desc);
        bus.write_word(sp + 4, 0xBEEF);
        let dispose = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(dispose.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(bus.read_long(token_desc), AE_TYPE_NULL);
        assert_eq!(bus.read_long(token_desc + 4), 0);
        assert!(!disp.ae_descriptor_state.descriptors.contains_key(&token_desc));
        assert!(!disp.has_handle_ptr(data_ptr));
    }

    #[test]
    fn pack8_aegetparamdesc_missing_parameter_returns_err_and_null_desc() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_desc = 0x0030_0000u32;
        let result_desc = 0x0030_0100u32;

        bus.write_long(event_desc, AE_TYPE_APPLE_EVENT);
        bus.write_long(event_desc + 4, 0x0030_0200);
        disp.ae_descriptor_state.with_mut(|state| state.events.insert(
            event_desc,
            crate::trap::dispatch::SyntheticAppleEvent {
                event_class: AE_TYPE_APPLE_EVENT,
                event_id: u32::from_be_bytes(*b"oapp"),
                params: HashMap::new(),
                items: Vec::new(),
            },
        ));

        // Selector $0812 => AEGetParamDesc(event, keyDirectObject,
        // typeWildCard, result). The synthetic OAPP event has no direct
        // parameter, so the result AEDesc must be typeNull and the OSErr
        // must be errAEDescNotFound.
        cpu.write_reg(Register::D0, 0x0812);
        bus.write_long(sp, result_desc);
        bus.write_long(sp + 4, AE_TYPE_WILDCARD);
        bus.write_long(sp + 8, u32::from_be_bytes(*b"----"));
        bus.write_long(sp + 12, event_desc);
        bus.write_word(sp + 16, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(result_desc), AE_TYPE_NULL);
        assert_eq!(bus.read_long(result_desc + 4), 0);
        assert_eq!(bus.read_word(sp + 16), AE_ERR_DESC_NOT_FOUND as u16);
        assert_eq!(
            cpu.read_reg(Register::D0),
            AE_ERR_DESC_NOT_FOUND as i32 as u32
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
    }

    #[test]
    fn pack8_aegetattributeptr_returns_event_class_and_id_attributes() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_desc = 0x0030_0000u32;
        let type_code_ptr = 0x0030_0100u32;
        let data_ptr = 0x0030_0200u32;
        let actual_size_ptr = 0x0030_0300u32;
        let event_class = AE_TYPE_APPLE_EVENT;
        let event_id = u32::from_be_bytes(*b"oapp");

        bus.write_long(event_desc, AE_TYPE_APPLE_EVENT);
        bus.write_long(event_desc + 4, 0x0030_0400);
        disp.ae_descriptor_state.with_mut(|state| state.events.insert(
            event_desc,
            crate::trap::dispatch::SyntheticAppleEvent {
                event_class,
                event_id,
                params: HashMap::new(),
                items: Vec::new(),
            },
        ));

        // Selector $0E15 => AEGetAttributePtr(event, keyEventIDAttr,
        // typeWildCard, typeCode, dataPtr, maximumSize, actualSize).
        cpu.write_reg(Register::D0, 0x0E15);
        bus.write_long(sp, actual_size_ptr);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, data_ptr);
        bus.write_long(sp + 12, type_code_ptr);
        bus.write_long(sp + 16, AE_TYPE_WILDCARD);
        bus.write_long(sp + 20, AE_KEY_EVENT_ID_ATTR);
        bus.write_long(sp + 24, event_desc);
        bus.write_word(sp + 28, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 28), 0);
        assert_eq!(bus.read_long(type_code_ptr), AE_TYPE_TYPE);
        assert_eq!(bus.read_long(actual_size_ptr), 4);
        assert_eq!(bus.read_long(data_ptr), event_id);
        assert_eq!(cpu.read_reg(Register::A7), sp + 28);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        // Repeat for keyEventClassAttr to make both required attributes
        // observable through the same pointer-returning routine.
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0E15);
        bus.write_long(sp, actual_size_ptr);
        bus.write_long(sp + 4, 4);
        bus.write_long(sp + 8, data_ptr);
        bus.write_long(sp + 12, type_code_ptr);
        bus.write_long(sp + 16, AE_TYPE_WILDCARD);
        bus.write_long(sp + 20, AE_KEY_EVENT_CLASS_ATTR);
        bus.write_long(sp + 24, event_desc);
        bus.write_word(sp + 28, 0xBEEF);
        let class_result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(class_result.is_some());
        assert!(class_result.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 28), 0);
        assert_eq!(bus.read_long(type_code_ptr), AE_TYPE_TYPE);
        assert_eq!(bus.read_long(actual_size_ptr), 4);
        assert_eq!(bus.read_long(data_ptr), event_class);
    }

    #[test]
    fn pack8_aeresolve_non_object_spec_returns_err_and_null_token() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let object_specifier = 0x0030_0000u32;
        let token = 0x0030_0100u32;

        bus.write_long(object_specifier, AE_TYPE_NULL);
        bus.write_long(object_specifier + 4, 0);
        bus.write_long(token, 0xDEAD_BEEFu32);
        bus.write_long(token + 4, 0xCAFE_BABEu32);

        // Selector $0536 => AEResolve(objectSpecifier, callbackFlags,
        // token). A non-object-specifier input must not resolve
        // successfully; Inside Macintosh says AEResolve returns a null
        // descriptor in `theToken` if an error occurs.
        cpu.write_reg(Register::D0, 0x0536);
        bus.write_long(sp, token);
        bus.write_word(sp + 4, 0); // kAEIDoMinimum
        bus.write_long(sp + 6, object_specifier);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(token), AE_TYPE_NULL);
        assert_eq!(bus.read_long(token + 4), 0);
        assert_eq!(bus.read_word(sp + 10), AE_ERR_NOT_AN_OBJECT_SPEC as u16);
        assert_eq!(
            cpu.read_reg(Register::D0),
            AE_ERR_NOT_AN_OBJECT_SPEC as i32 as u32
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    #[test]
    fn pack8_aeprocessappleevent_returns_handler_result_code_to_caller() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"aevt");
        let event_id = u32::from_be_bytes(*b"oapp");
        let handler_ptr = 0x0040_9000u32;
        let return_pc = 0x00F0_5678u32;
        let handler_result = (-1708i16) as u16; // errAEEventNotHandled

        // Install handler.
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, 0x1111_2222);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());

        // Enter AEProcessAppleEvent path and dispatch handler.
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, return_pc);
        cpu.write_reg(Register::D0, 0x021B);
        bus.write_long(sp, 0x0032_1000); // EventRecord ptr
        bus.write_word(sp + 4, 0xBEEF); // OSErr slot
        let process = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(process.is_some());
        assert!(process.unwrap().is_ok());

        let state = disp
            .ae_call_state
            .clone()
            .expect("expected in-flight AE call state");

        let (event_desc, reply_desc) = state
            .owned_descriptors
            .expect("AEProcessAppleEvent owns its callback descriptors");
        let event_handle = bus.read_long(event_desc + 4);
        let event_data = bus.read_long(event_handle);
        assert_eq!(bus.get_alloc_size(event_desc), Some(8));
        assert_eq!(bus.get_alloc_size(reply_desc), Some(8));
        assert_eq!(bus.get_alloc_size(event_handle), Some(4));
        assert_eq!(bus.get_alloc_size(event_data), Some(8));

        // Simulate handler writing function result then returning through
        // trampoline (`MOVE.W #$FEFE, D0; _Pack8`).
        bus.write_word(state.expected_sp_after_rtd, handler_result);
        cpu.write_reg(Register::A7, state.expected_sp_after_rtd);
        cpu.write_reg(Register::D0, 0xFEFE);
        let trampoline = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(trampoline.is_some());
        assert!(trampoline.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::PC), return_pc);
        assert_eq!(cpu.read_reg(Register::A7), state.expected_sp_after_rtd);
        assert_eq!(bus.read_word(state.expected_sp_after_rtd), handler_result);
        assert_eq!(
            cpu.read_reg(Register::D0),
            handler_result as i16 as i32 as u32
        );
        assert!(disp.ae_call_state.is_none());
        assert_eq!(bus.get_alloc_size(event_desc), None);
        assert_eq!(bus.get_alloc_size(reply_desc), None);
        assert_eq!(bus.get_alloc_size(event_handle), None);
        assert_eq!(bus.get_alloc_size(event_data), None);
        assert!(!disp.ae_descriptor_state.events.contains_key(&event_desc));
        assert!(!disp.ae_descriptor_state.descriptors.contains_key(&event_desc));
    }

    #[test]
    fn pack8_aeprocessappleevent_callback_allocation_failure_is_atomic() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"aevt");
        let event_id = u32::from_be_bytes(*b"oapp");

        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0);
        bus.write_long(sp + 2, 0x1111_2222);
        bus.write_long(sp + 6, 0x0040_9000);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0);
        assert!(disp
            .dispatch_toolbox(true, 0x016, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        disp.ae_trampoline_addr = Some(0x0012_3456);
        let heap_limit = bus.application_memory_limit();
        let transient_descriptor = heap_limit - 12;
        bus.reserve_heap_until(transient_descriptor);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, 0x00f0_5678);
        cpu.write_reg(Register::D0, 0x021B);
        bus.write_long(sp, 0x0032_1000);
        bus.write_word(sp + 4, 0);
        assert!(disp
            .dispatch_toolbox(true, 0x016, &mut cpu, &mut bus)
            .unwrap()
            .is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), -108i32 as u32);
        assert_eq!(bus.read_word(sp + 4), (-108i16) as u16);
        assert_eq!(bus.get_alloc_size(transient_descriptor), None);
        assert!(disp.ae_descriptor_state.events.is_empty());
        assert!(disp.ae_descriptor_state.descriptors.is_empty());
        assert!(disp.ae_call_state.is_none());
    }

    #[test]
    fn pack8_aeprocessappleevent_dispatches_matching_event_on_repeated_calls() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let event_class = u32::from_be_bytes(*b"aevt");
        let event_id = u32::from_be_bytes(*b"oapp");
        let handler_ptr = 0x0040_A000u32;
        let handler_result = (-1708i16) as u16; // errAEEventNotHandled
        let first_return_pc = 0x00F0_1234u32;
        let second_return_pc = 0x00F0_5678u32;

        // Install handler.
        cpu.write_reg(Register::D0, 0x091F);
        bus.write_word(sp, 0); // isSysHandler
        bus.write_long(sp + 2, 0x1111_2222);
        bus.write_long(sp + 6, handler_ptr);
        bus.write_long(sp + 10, event_id);
        bus.write_long(sp + 14, event_class);
        bus.write_word(sp + 18, 0xBEEF);
        let install = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(install.is_some());
        assert!(install.unwrap().is_ok());

        // First AEProcessAppleEvent dispatch.
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, first_return_pc);
        cpu.write_reg(Register::D0, 0x021B);
        bus.write_long(sp, 0x0032_1000);
        bus.write_word(sp + 4, 0xBEEF);
        let process1 = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(process1.is_some());
        assert!(process1.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        assert!(disp.fired_oapp_handler);

        let state1 = disp
            .ae_call_state
            .clone()
            .expect("expected first in-flight AE call state");
        bus.write_word(state1.expected_sp_after_rtd, handler_result);
        cpu.write_reg(Register::A7, state1.expected_sp_after_rtd);
        cpu.write_reg(Register::D0, 0xFEFE);
        let tramp1 = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(tramp1.is_some());
        assert!(tramp1.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::PC), first_return_pc);
        assert_eq!(
            cpu.read_reg(Register::D0),
            handler_result as i16 as i32 as u32
        );
        assert!(disp.ae_call_state.is_none());

        // Second AEProcessAppleEvent dispatch should still fire.
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::PC, second_return_pc);
        cpu.write_reg(Register::D0, 0x021B);
        bus.write_long(sp, 0x0032_2000);
        bus.write_word(sp + 4, 0xBEEF);
        let process2 = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(process2.is_some());
        assert!(process2.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::PC), handler_ptr);
        assert!(disp.fired_oapp_handler);

        let state2 = disp
            .ae_call_state
            .clone()
            .expect("expected second in-flight AE call state");
        bus.write_word(state2.expected_sp_after_rtd, handler_result);
        cpu.write_reg(Register::A7, state2.expected_sp_after_rtd);
        cpu.write_reg(Register::D0, 0xFEFE);
        let tramp2 = disp.dispatch_toolbox(true, 0x016, &mut cpu, &mut bus);
        assert!(tramp2.is_some());
        assert!(tramp2.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::PC), second_return_pc);
        assert_eq!(
            cpu.read_reg(Register::D0),
            handler_result as i16 as i32 as u32
        );
        assert!(disp.ae_call_state.is_none());
    }

    fn write_styled_line_break_frame(
        bus: &mut MacMemoryBus,
        sp: u32,
        text_ptr: u32,
        text_len: u32,
        text_start: u32,
        text_end: u32,
        text_width_ptr: u32,
        text_offset_ptr: u32,
    ) {
        bus.write_long(sp, 0x821C_FFFE);
        bus.write_long(sp + 4, text_offset_ptr);
        bus.write_long(sp + 8, text_width_ptr);
        bus.write_long(sp + 12, 0); // flags
        bus.write_long(sp + 16, text_end);
        bus.write_long(sp + 20, text_start);
        bus.write_long(sp + 24, text_len);
        bus.write_long(sp + 28, text_ptr);
        bus.write_word(sp + 32, 0xBEEF); // StyledLineBreakCode result
    }

    #[test]
    fn script_util_generated_routes_preserve_exact_stack_long_values() {
        assert_eq!(super::SCRIPT_UTIL_OPERATION_ROUTES.len(), 21);
        assert!(super::SCRIPT_UTIL_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x800E_001C, "HiliteText"),
            (0x8012_FFE2, "NFindWord"),
            (0x8012_FFFC, "GetFormatOrder"),
            (0x8204_0022, "ParseTable"),
            (0x8204_FFF8, "InitDateCache"),
            (0x8204_FFFA, "IntlTokenize"),
            (0x8208_FFE0, "TruncString"),
            (0x820C_0026, "FindScriptRun"),
            (0x820C_FFDE, "TruncText"),
            (0x820C_FFE4, "ValidDate"),
            (0x820C_FFEC, "StringToFormatRec"),
            (0x820E_FFEE, "ToggleDate"),
            (0x8210_FFE6, "StringToExtended"),
            (0x8210_FFE8, "ExtendedToString"),
            (0x8210_FFEA, "FormatRecToString"),
            (0x8214_FFF4, "StringToTime"),
            (0x8214_FFF6, "StringToDate"),
            (0x821C_FFFE, "StyledLineBreak"),
            (0x8408_0024, "PortionText"),
            (0x8408_0028, "VisibleLength"),
            (0xC012_001A, "FindWordBreaks"),
        ] {
            let route = super::script_util_operation_route(0xA8B5, selector)
                .expect("ScriptUtil operation route");
            assert_eq!(route.routine_name, routine_name);
        }

        for (trap_word, selector) in [
            (0xA9B5, 0x800E_001C),
            (0xA8B4, 0xC012_001A),
            (0xA8B5, 0x820C_FFDC), // manual-only ReplaceText identity
            (0xA8B5, 0x8206_0010), // non-intersection CharacterByteType glue
            (0xA8B5, 0x1C00_0E80), // byte-swapped HiliteText selector
            (0xA8B5, 0x0000_001C), // legacy low-byte HiliteText selector
            (0xA8B5, 0x0000_800E), // partial selector halfword
        ] {
            assert!(super::script_util_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn script_util_records_stack_long_identity_without_changing_hilite_text_behavior() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let offsets_ptr = 0x362000u32;

        for trap_word in [0xA8B5, 0xA9B5] {
            disp.current_trap_word = trap_word;
            cpu.write_reg(Register::A7, sp);
            bus.write_long(sp, 0x800E_001C); // HiliteText
            bus.write_long(sp + 4, offsets_ptr);
            bus.write_word(sp + 8, 8); // secondOffset
            bus.write_word(sp + 10, 2); // firstOffset
            bus.write_word(sp + 12, 12); // textLength
            bus.write_long(sp + 14, 0x363000); // textPtr
            bus.write_bytes(offsets_ptr, &[0xA5; 12]);

            let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
            assert!(result.expect("ScriptUtil arm").is_ok());
            assert_eq!(bus.read_bytes(offsets_ptr, 12), vec![0; 12]);
            assert_eq!(cpu.read_reg(Register::A7), sp + 18);

            let expected = (trap_word == 0xA8B5)
                .then_some("selector-operation:_ScriptUtil:0x800E001C:stack-long-immediate:32");
            assert_eq!(disp.current_selector_operation, expected);
        }
    }

    // ScriptUtil ($A8B5) selector 0 FontScript
    // IM:V 1988 pp. V-288 and V-315
    #[test]
    fn scriptutil_fontscript_returns_smroman_and_pops_selector_long() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0000_0000); // selector 0 (FontScript)
        bus.write_word(sp + 4, 0xBEEF); // result slot

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0); // smRoman
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // ScriptUtil ($A8B5) selector 8 GetEnvirons
    // IM:V 1988 pp. V-288 and V-311
    #[test]
    fn scriptutil_getenvirons_returns_zero_long_and_pops_selector_plus_verb() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0000_0008); // selector 8
        bus.write_word(sp + 4, 0x1234); // verb
        bus.write_long(sp + 6, 0xDEAD_BEEF); // LongInt result slot

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 6), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    }

    // ScriptUtil ($A8B5) selector 10 SetEnvirons
    // IM:V 1988 pp. V-288 and V-311
    #[test]
    fn scriptutil_setenvirons_returns_noerr_and_pops_selector_verb_param() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0000_000A); // selector 10
        bus.write_word(sp + 4, 0x0001); // verb
        bus.write_long(sp + 6, 0xCAFE_BABE); // param
        bus.write_word(sp + 10, 0xBEEF); // OSErr result slot

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 10), 0); // noErr
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
    }

    // ScriptUtil ($A8B5) selector 12 GetScript
    // IM:V 1988 pp. V-288 and V-312..V-313
    #[test]
    fn scriptutil_getscript_returns_zero_long_and_pops_selector_script_verb() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0000_000C); // selector 12
        bus.write_word(sp + 4, 0); // smRoman
        bus.write_word(sp + 6, 1); // smScriptRight
        bus.write_long(sp + 8, 0xDEAD_BEEF); // LongInt result slot

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_long(sp + 8), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // ScriptUtil ($A8B5) selector 14 SetScript
    // IM:V 1988 pp. V-288 and V-312..V-313
    #[test]
    fn scriptutil_setscript_returns_noerr_and_pops_selector_script_verb_param() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0000_000E); // selector 14
        bus.write_word(sp + 4, 0); // smRoman
        bus.write_word(sp + 6, 1); // smScriptRight
        bus.write_long(sp + 8, 0); // param
        bus.write_word(sp + 12, 0xBEEF); // OSErr result slot

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 12), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    // ScriptUtil ($A8B5) selector 20 Pixel2Char
    // IM:V 1988 p. V-310
    // Pascal calling convention: leadingEdge VAR pointer is the LAST arg
    // pushed, so it lives at sp+4 just after the selector long; textBuf
    // is the FIRST arg pushed and lives deepest at sp+14.
    #[test]
    fn scriptutil_pixel2char_returns_offset_zero_and_clears_leadingedge() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let leading_edge_ptr = 0x362000u32;

        bus.write_long(sp, 0x0000_0014); // selector 20
        bus.write_long(sp + 4, leading_edge_ptr); // VAR leadingEdge (last arg)
        bus.write_word(sp + 8, 12); // pixelWidth
        bus.write_word(sp + 10, 0); // slop
        bus.write_word(sp + 12, 8); // textLen
        bus.write_long(sp + 14, 0x363000); // textBuf (first arg, deepest)
        bus.write_word(sp + 18, 0xBEEF); // INTEGER result
        bus.write_byte(leading_edge_ptr, 0xFF);

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_byte(leading_edge_ptr), 0); // FALSE
        assert_eq!(bus.read_word(sp + 18), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 18);
    }

    // ScriptUtil ($A8B5) selector 26 FindWord
    // IM:V 1988 pp. V-312..V-313
    // Pascal calling convention: VAR offsets pointer is the LAST arg
    // pushed (sp+4); textPtr is FIRST and deepest (sp+18). OffsetTable
    // is 12 bytes (ARRAY[0..2] OF OffPair) per IM:VI 33514.
    #[test]
    fn scriptutil_findword_zeros_offsettable_and_pops_selector_plus_args() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let offsets_ptr = 0x364000u32;

        bus.write_long(sp, 0x0000_001A); // selector 26
        bus.write_long(sp + 4, offsets_ptr); // VAR offsets (last arg)
        bus.write_long(sp + 8, 0x366000); // breaksPtr
        bus.write_word(sp + 12, 1); // leadingEdge
        bus.write_word(sp + 14, 4); // offset
        bus.write_word(sp + 16, 12); // textLength
        bus.write_long(sp + 18, 0x365000); // textPtr (first arg, deepest)
        bus.write_bytes(offsets_ptr, &[0xA5; 12]);

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_bytes(offsets_ptr, 12), vec![0; 12]);
        assert_eq!(cpu.read_reg(Register::A7), sp + 22);
    }

    // ScriptUtil ($A8B5) encoded selector $821CFFFE StyledLineBreak
    // Text 1993 pp. 5-79..5-81.
    #[test]
    fn scriptutil_styledlinebreak_full_style_run_fits_returns_overflow_and_decrements_width() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let text_ptr = 0x365000u32;
        let width_ptr = 0x366000u32;
        let offset_ptr = 0x366010u32;
        let text = b"SHORT";
        bus.write_bytes(text_ptr, text);
        let run_width =
            disp.scriptutil_measure_text_range_width(&bus, text_ptr, 0, text.len() as u32);
        let starting_width = run_width + 20;

        write_styled_line_break_frame(
            &mut bus,
            sp,
            text_ptr,
            text.len() as u32,
            0,
            text.len() as u32,
            width_ptr,
            offset_ptr,
        );
        bus.write_long(width_ptr, (starting_width as u32) << 16);
        bus.write_long(offset_ptr, 0xFFFF_FFFF);

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 32), 2); // smBreakOverflow
        assert_eq!(bus.read_long(offset_ptr), text.len() as u32);
        assert_eq!(bus.read_long(width_ptr), 20u32 << 16);
        assert_eq!(cpu.read_reg(Register::A7), sp + 32);
    }

    // ScriptUtil ($A8B5) encoded selector $821CFFFE StyledLineBreak
    // Text 1993 pp. 5-79..5-81.
    #[test]
    fn scriptutil_styledlinebreak_breaks_at_last_space_before_overflow() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let text_ptr = 0x365000u32;
        let width_ptr = 0x366000u32;
        let offset_ptr = 0x366010u32;
        let text = b"HELLO WORLD";
        bus.write_bytes(text_ptr, text);
        let available = disp.scriptutil_measure_text_range_width(&bus, text_ptr, 0, 7);
        let consumed = disp.scriptutil_measure_text_range_width(&bus, text_ptr, 0, 6);

        write_styled_line_break_frame(
            &mut bus,
            sp,
            text_ptr,
            text.len() as u32,
            0,
            text.len() as u32,
            width_ptr,
            offset_ptr,
        );
        bus.write_long(width_ptr, (available as u32) << 16);
        bus.write_long(offset_ptr, 0xFFFF_FFFF);

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 32), 0); // smBreakWord
        assert_eq!(bus.read_long(offset_ptr), 6); // after the space
        assert_eq!(
            bus.read_long(width_ptr),
            ((available - consumed) as u32) << 16
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 32);
    }

    // ScriptUtil ($A8B5) encoded selector $821CFFFE StyledLineBreak
    // Text 1993 pp. 5-79..5-81.
    #[test]
    fn scriptutil_styledlinebreak_first_long_word_breaks_on_character_boundary() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let text_ptr = 0x365000u32;
        let width_ptr = 0x366000u32;
        let offset_ptr = 0x366010u32;
        let text = b"ABCDEFGHIJ";
        bus.write_bytes(text_ptr, text);
        let available = disp.scriptutil_measure_text_range_width(&bus, text_ptr, 0, 3);

        write_styled_line_break_frame(
            &mut bus,
            sp,
            text_ptr,
            text.len() as u32,
            0,
            text.len() as u32,
            width_ptr,
            offset_ptr,
        );
        bus.write_long(width_ptr, (available as u32) << 16);
        bus.write_long(offset_ptr, 0xFFFF_FFFF);

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 32), 1); // smBreakChar
        assert_eq!(bus.read_long(offset_ptr), 3);
        assert_eq!(bus.read_long(width_ptr), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 32);
    }

    // ScriptUtil ($A8B5) selector $820CFFDE TruncText
    // IM:VI 1991 pp. 14-59..14-60 and Table C-3.
    #[test]
    fn scriptutil_trunctext_returns_not_truncated_and_pops_encoded_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let length_ptr = 0x366000u32;

        bus.write_long(sp, 0x820C_FFDE); // TruncText encoded selector
        bus.write_word(sp + 4, 0); // truncWhere
        bus.write_long(sp + 6, length_ptr); // VAR length
        bus.write_long(sp + 10, 0x367000); // textPtr
        bus.write_word(sp + 14, 80); // width
        bus.write_word(sp + 16, 0xBEEF); // INTEGER result
        bus.write_word(length_ptr, 12);

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 16), 0); // smNotTruncated
        assert_eq!(bus.read_word(length_ptr), 12);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
    }

    fn write_replace_text_frame(
        bus: &mut MacMemoryBus,
        sp: u32,
        base_handle: u32,
        substitution_handle: u32,
        key_ptr: u32,
    ) {
        bus.write_long(sp, 0x820C_FFDC);
        bus.write_long(sp + 4, key_ptr);
        bus.write_long(sp + 8, substitution_handle);
        bus.write_long(sp + 12, base_handle);
        bus.write_word(sp + 16, 0xBEEF);
    }

    fn replace_text_handles(
        bus: &mut MacMemoryBus,
        base: &[u8],
        substitution: &[u8],
    ) -> (u32, u32) {
        let base_ptr = bus.alloc(base.len() as u32);
        let base_handle = bus.alloc(4);
        let substitution_ptr = bus.alloc(substitution.len() as u32);
        let substitution_handle = bus.alloc(4);
        bus.write_bytes(base_ptr, base);
        bus.write_long(base_handle, base_ptr);
        bus.write_bytes(substitution_ptr, substitution);
        bus.write_long(substitution_handle, substitution_ptr);
        (base_handle, substitution_handle)
    }

    // ScriptUtil ($A8B5) encoded selector $820CFFDC ReplaceText
    // Text 1993 pp. 5-74..5-75 and Table D-3.
    #[test]
    fn scriptutil_replacetext_replaces_multiple_nonoverlapping_keys_and_grows_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let key_ptr = bus.alloc(16);
        let (base_handle, substitution_handle) = replace_text_handles(&mut bus, b"A^0B^0", b"LONG");
        let original_ptr = bus.read_long(base_handle);
        bus.write_pstring(key_ptr, b"^0");
        write_replace_text_frame(&mut bus, sp, base_handle, substitution_handle, key_ptr);

        disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus)
            .expect("ScriptUtil dispatch")
            .expect("ReplaceText succeeds");

        let updated_ptr = bus.read_long(base_handle);
        assert_ne!(updated_ptr, original_ptr);
        assert!(!disp.has_handle_ptr(original_ptr));
        assert_eq!(disp.handle_for_ptr(updated_ptr), Some(base_handle));
        assert_eq!(bus.read_word(sp + 16), 2);
        assert_eq!(cpu.read_reg(Register::D0), 2);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.get_alloc_size(updated_ptr), Some(10));
        assert_eq!(bus.read_bytes(updated_ptr, 10), b"ALONGBLONG");
    }

    #[test]
    fn scriptutil_replacetext_shrinks_handle_without_recursively_scanning_substitution() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let key_ptr = bus.alloc(16);
        let (base_handle, substitution_handle) = replace_text_handles(&mut bus, b"aaaa", b"a");
        let original_ptr = bus.read_long(base_handle);
        bus.write_pstring(key_ptr, b"aa");
        write_replace_text_frame(&mut bus, sp, base_handle, substitution_handle, key_ptr);

        disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus)
            .expect("ScriptUtil dispatch")
            .expect("ReplaceText succeeds");

        let updated_ptr = bus.read_long(base_handle);
        assert_eq!(updated_ptr, original_ptr);
        assert_eq!(disp.handle_for_ptr(updated_ptr), Some(base_handle));
        assert_eq!(bus.read_word(sp + 16), 2);
        assert_eq!(bus.get_alloc_size(updated_ptr), Some(2));
        assert_eq!(bus.read_bytes(updated_ptr, 2), b"aa");
    }

    #[test]
    fn scriptutil_replacetext_no_match_preserves_handle_and_contents() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let key_ptr = bus.alloc(16);
        let (base_handle, substitution_handle) =
            replace_text_handles(&mut bus, b"unchanged", b"replacement");
        let original_ptr = bus.read_long(base_handle);
        bus.write_pstring(key_ptr, b"missing");
        write_replace_text_frame(&mut bus, sp, base_handle, substitution_handle, key_ptr);

        disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus)
            .expect("ScriptUtil dispatch")
            .expect("ReplaceText succeeds");

        assert_eq!(bus.read_word(sp + 16), 0);
        assert_eq!(bus.read_long(base_handle), original_ptr);
        assert_eq!(bus.read_bytes(original_ptr, 9), b"unchanged");
    }

    #[test]
    fn scriptutil_replacetext_never_matches_trailing_byte_of_japanese_character() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let key_ptr = bus.alloc(16);
        let (base_handle, substitution_handle) =
            replace_text_handles(&mut bus, &[0x81, 0x40, 0x40], b"X");
        disp.tx_font = 0x4000; // first Japanese font-family ID
        bus.write_pstring(key_ptr, &[0x40]);
        write_replace_text_frame(&mut bus, sp, base_handle, substitution_handle, key_ptr);

        disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus)
            .expect("ScriptUtil dispatch")
            .expect("ReplaceText succeeds");

        let updated_ptr = bus.read_long(base_handle);
        assert_eq!(bus.read_word(sp + 16), 1);
        assert_eq!(bus.read_bytes(updated_ptr, 3), &[0x81, 0x40, b'X']);
    }

    #[test]
    fn scriptutil_replacetext_returns_nilhandleerr_through_encoded_result_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let key_ptr = bus.alloc(16);
        let (_, substitution_handle) = replace_text_handles(&mut bus, b"base", b"X");
        bus.write_pstring(key_ptr, b"a");
        write_replace_text_frame(&mut bus, sp, 0, substitution_handle, key_ptr);

        disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus)
            .expect("ScriptUtil dispatch")
            .expect("ReplaceText returns an error code");

        assert_eq!(bus.read_word(sp + 16) as i16, -109); // nilHandleErr
        assert_eq!(cpu.read_reg(Register::D0), (-109i32) as u32);
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
    }

    #[test]
    fn scriptutil_replacetext_empty_substitution_resizes_base_to_zero() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let key_ptr = bus.alloc(16);
        let (base_handle, substitution_handle) = replace_text_handles(&mut bus, b"aaaa", b"");
        let original_ptr = bus.read_long(base_handle);
        bus.write_pstring(key_ptr, b"aa");
        write_replace_text_frame(&mut bus, sp, base_handle, substitution_handle, key_ptr);

        disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus)
            .expect("ScriptUtil dispatch")
            .expect("ReplaceText succeeds");

        assert_eq!(bus.read_word(sp + 16), 2);
        assert_eq!(bus.read_long(base_handle), 0);
        assert_eq!(bus.get_alloc_size(original_ptr), None);
        assert!(!disp.has_handle_ptr(original_ptr));
    }

    // ScriptUtil ($A8B5) encoded selector fallback
    // IM:VI Table C-3 stores result size and argument byte count in the high word.
    #[test]
    fn scriptutil_unknown_encoded_selector_uses_stack_metadata() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x8204_ABCD); // 2-byte result, 4 arg bytes
        bus.write_long(sp + 4, 0xCAFE_BABE); // opaque args
        bus.write_word(sp + 8, 0xBEEF); // result slot

        let result = disp.dispatch_toolbox(true, 0x0B5, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 8), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // FMSwapFont ($A901)
    // IM:I 1985 pp. I-223 and I-225.
    #[test]
    fn fmswapfont_returns_fmoutptr_and_pops_fminput_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        write_test_fmswapfont_frame(&mut bus, sp);

        let result = disp.dispatch_toolbox(true, 0x101, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let fm_out_ptr = bus.read_long(sp + 4);
        assert_ne!(fm_out_ptr, 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // FMSwapFont ($A901)
    // IM:I 1985 p. I-225: FMOutput.errNum is 0 and the output record
    // carries a non-NIL fontHandle plus scaling fields.
    #[test]
    fn fmswapfont_writes_non_nil_font_handle_and_fixed_point_scaling_fields() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        write_test_fmswapfont_frame(&mut bus, sp);

        let result = disp.dispatch_toolbox(true, 0x101, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let fm_out_ptr = bus.read_long(sp + 4);
        assert_ne!(fm_out_ptr, 0);
        assert_eq!(bus.read_word(fm_out_ptr), 0); // errNum
        assert_ne!(bus.read_long(fm_out_ptr + 2), 0); // fontHandle is non-NIL
        assert_eq!(bus.read_byte(fm_out_ptr + 13), 9); // ascent for size 12
        assert_eq!(bus.read_byte(fm_out_ptr + 14), 1); // descent
        assert_eq!(bus.read_byte(fm_out_ptr + 15), 7); // widMax
        assert_eq!(bus.read_byte(fm_out_ptr + 16), 0); // leading
        assert_eq!(bus.read_word(fm_out_ptr + 18), 0x0100); // numer.v
        assert_eq!(bus.read_word(fm_out_ptr + 20), 0x0100); // numer.h
        assert_eq!(bus.read_word(fm_out_ptr + 22), 0x0100); // denom.v
        assert_eq!(bus.read_word(fm_out_ptr + 24), 0x0100); // denom.h
    }

    // FMSwapFont ($A901)
    // IM:I 1985 p. I-225: the output record's fontHandle is a handle to
    // the chosen font record, not the FMOutput block itself.
    #[test]
    fn fmswapfont_writes_distinct_non_nil_font_handle() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        write_test_fmswapfont_frame(&mut bus, sp);

        let result = disp.dispatch_toolbox(true, 0x101, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let fm_out_ptr = bus.read_long(sp + 4);
        assert_ne!(fm_out_ptr, 0);
        let font_handle = bus.read_long(fm_out_ptr + 2);
        assert_ne!(font_handle, 0);
        assert_ne!(font_handle, fm_out_ptr);
        assert_eq!(bus.read_word(fm_out_ptr + 18), 0x0100); // numer.v
        assert_eq!(bus.read_word(fm_out_ptr + 20), 0x0100); // numer.h
        assert_eq!(bus.read_word(fm_out_ptr + 22), 0x0100); // denom.v
        assert_eq!(bus.read_word(fm_out_ptr + 24), 0x0100); // denom.h
    }

    // FMSwapFont ($A901)
    // HLE retains a compact input signature in the auxiliary word of
    // the returned font-handle block for later font-manager consumers.
    #[test]
    fn fmswapfont_aux_font_handle_carries_input_signature() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        write_test_fmswapfont_frame(&mut bus, sp);

        let result = disp.dispatch_toolbox(true, 0x101, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let fm_out_ptr = bus.read_long(sp + 4);
        let font_handle = bus.read_long(fm_out_ptr + 2);
        assert_eq!(bus.read_long(font_handle + 4), 0x0003_000C);
    }

    // RealFont ($A902)
    // IM:I 1985 p. I-223: applFont always returns FALSE.
    #[test]
    fn realfont_applfont_always_returns_false_and_pops_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 12); // size
        bus.write_word(sp + 2, 1); // fontNum = applFont
        bus.write_word(sp + 4, 0xBEEF); // Boolean result slot

        let result = disp.dispatch_toolbox(true, 0x102, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // RealFont ($A902)
    // IM:I 1985 p. I-223: return TRUE if size is available; FALSE if scaling needed.
    #[test]
    fn realfont_known_bitmap_size_true_and_nonstandard_size_false() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 12); // size
        bus.write_word(sp + 2, 3); // Geneva
        bus.write_word(sp + 4, 0xBEEF);
        let first = disp.dispatch_toolbox(true, 0x102, &mut cpu, &mut bus);
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0x0100);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 11); // non-standard bitmap size in current HLE model
        bus.write_word(sp + 2, 3);
        bus.write_word(sp + 4, 0xBEEF);
        let second = disp.dispatch_toolbox(true, 0x102, &mut cpu, &mut bus);
        assert!(second.is_some());
        assert!(second.unwrap().is_ok());
        assert_eq!(bus.read_word(sp + 4), 0x0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // RealFont ($A902)
    // IM:I 1985 p. I-223 line 7309: "RealFont will always return
    // FALSE if you pass applFont in fontNum." Systemless HLE follows
    // the Apple-canonical rule; BasiliskII System 7.5 ROM diverges
    // (returns TRUE because applFont is bound to a real font at
    // boot) — see a902_diag_realfont diagnostic.
    #[test]
    fn realfont_applfont_returns_false_for_all_canonical_sizes_per_apple_spec() {
        let canonical_sizes: [u16; 6] = [9, 10, 12, 14, 18, 24];
        for &size in canonical_sizes.iter() {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            bus.write_word(sp, size);
            bus.write_word(sp + 2, 1); // applFont
            bus.write_word(sp + 4, 0xBEEF);
            let result = disp.dispatch_toolbox(true, 0x102, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
            assert_eq!(
                bus.read_word(sp + 4),
                0,
                "applFont must return FALSE at canonical size {size}",
            );
        }
    }

    // RealFont ($A902)
    // IM:I 1985 p. I-223 line 7307: "FALSE if the font has to be
    // scaled to that size." Systemless HLE returns FALSE for sizes
    // outside the canonical bitmap set {9, 10, 12, 14, 18, 24}
    // even for real bundled bitmap fonts. BasiliskII System 7.5
    // ROM diverges (treats any valid fontNum as truthy regardless
    // of size) — see a902_diag_realfont diagnostic.
    #[test]
    fn realfont_non_standard_size_returns_false_for_real_bitmap_font_per_apple_spec() {
        let non_standard_sizes: [u16; 5] = [8, 11, 13, 15, 100];
        for &size in non_standard_sizes.iter() {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            bus.write_word(sp, size);
            bus.write_word(sp + 2, 3); // Geneva
            bus.write_word(sp + 4, 0xBEEF);
            let result = disp.dispatch_toolbox(true, 0x102, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
            assert_eq!(
                bus.read_word(sp + 4),
                0,
                "Geneva at non-canonical size {size} must return FALSE",
            );
        }
    }

    // SetFontLock ($A903)
    // IM:I 1985 p. I-223: PROCEDURE SetFontLock(lockFlag: BOOLEAN).
    // Pops one 2-byte BOOLEAN argument; no function-result slot.
    #[test]
    fn setfontlock_true_pops_two_byte_boolean_argument_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x0100); // TRUE in high byte
        bus.write_word(sp + 2, 0xCAFE); // sentinel above pop window
        bus.write_word(sp + 4, 0xF00D); // additional sentinel
        let result = disp.dispatch_toolbox(true, 0x103, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        // Sentinels above the pop window survive — trap doesn't write
        // past the 2-byte argument slot.
        assert_eq!(bus.read_word(sp + 2), 0xCAFE);
        assert_eq!(bus.read_word(sp + 4), 0xF00D);
    }

    #[test]
    fn setfontlock_false_pops_two_byte_boolean_argument_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_word(sp, 0x0000); // FALSE
        bus.write_word(sp + 2, 0xBABE); // sentinel above pop window
        bus.write_word(sp + 4, 0xBEEF); // additional sentinel
        let result = disp.dispatch_toolbox(true, 0x103, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        // FALSE branch pops the same 2 bytes as TRUE (Pascal PROCEDURE
        // calling convention is value-independent).
        assert_eq!(bus.read_word(sp + 2), 0xBABE);
        assert_eq!(bus.read_word(sp + 4), 0xBEEF);
    }

    #[test]
    fn setfontlock_alternating_calls_have_net_sp_delta_zero() {
        // Eight alternating TRUE/FALSE calls — each pops exactly 2
        // bytes; net SP delta after the last pop equals starting SP
        // minus the cumulative argument bytes (verifies no value-
        // dependent per-call drift).
        let (mut disp, mut cpu, mut bus) = setup();
        let base = TEST_SP;
        let mut sp = base;
        for i in 0..8u32 {
            let value: u16 = if i & 1 == 0 { 0x0100 } else { 0x0000 };
            bus.write_word(sp, value);
            cpu.write_reg(Register::A7, sp);
            let result = disp.dispatch_toolbox(true, 0x103, &mut cpu, &mut bus);
            assert!(result.is_some());
            assert!(result.unwrap().is_ok());
            assert_eq!(cpu.read_reg(Register::A7), sp + 2);
            sp = cpu.read_reg(Register::A7);
        }
        assert_eq!(sp - base, 16);
    }

    // Fix2Frac ($A841)
    // Operating System Utilities 1994, p. 3-44; IM IV 1986, p. IV-65.
    #[test]
    fn fix2frac_returns_equivalent_fract_and_saturates_out_of_range_inputs() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // IM IV-65 example: Fix2Frac(X2Fix(1.75)) = $70000000.
        bus.write_long(sp, 0x0001_C000);
        bus.write_long(sp + 4, 0);
        let first = disp.dispatch_toolbox(true, 0x041, &mut cpu, &mut bus);
        assert!(first.is_some());
        assert!(first.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x7000_0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        // OS Utils 3-44: values above Fract max saturate to $7FFFFFFF.
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x0002_0000); // +2.0 Fixed
        bus.write_long(sp + 4, 0);
        let high = disp.dispatch_toolbox(true, 0x041, &mut cpu, &mut bus);
        assert!(high.is_some());
        assert!(high.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x7FFF_FFFF);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        // OS Utils 3-44: values below Fract min saturate to $80000000.
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, (-0x0002_0000i32) as u32); // -2.0 Fixed
        bus.write_long(sp + 4, 0);
        let low = disp.dispatch_toolbox(true, 0x041, &mut cpu, &mut bus);
        assert!(low.is_some());
        assert!(low.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x8000_0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // Frac2Fix ($A842)
    // Operating System Utilities 1994, p. 3-44; IM IV 1986, p. IV-65.
    #[test]
    fn frac2fix_matches_documented_positive_and_negative_examples() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // IM IV-65 example: Frac2Fix(X2Frac(1.75)) = $0001C000.
        bus.write_long(sp, 0x7000_0000);
        bus.write_long(sp + 4, 0);
        let pos = disp.dispatch_toolbox(true, 0x042, &mut cpu, &mut bus);
        assert!(pos.is_some());
        assert!(pos.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x0001_C000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        // IM IV-65 example: Frac2Fix(X2Frac(-1.75)) = $FFFE4000.
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x9000_0000);
        bus.write_long(sp + 4, 0);
        let neg = disp.dispatch_toolbox(true, 0x042, &mut cpu, &mut bus);
        assert!(neg.is_some());
        assert!(neg.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0xFFFE_4000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // Fix2X ($A843)
    // Operating System Utilities 1994, p. 3-45.
    #[test]
    fn fix2x_returns_extended_equivalent_and_pops_fixed_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x0001_C000); // 1.75 Fixed
        for i in 0..10 {
            bus.write_byte(sp + 4 + i, 0);
        }

        let result = disp.dispatch_toolbox(true, 0x043, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let ext = Extended80::read_from_bus(&bus, sp + 4);
        assert!((f64::from(ext) - 1.75).abs() < 1e-12);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // X2Fix ($A844)
    // Inside Macintosh: Operating System Utilities (1994), p. 3-45.
    #[test]
    fn x2fix_dereferences_extended_pointer_and_preserves_pascal_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ext_ptr = bus.alloc(10);
        Extended80::from(1.75).write_to_bus(&mut bus, ext_ptr);
        bus.write_long(sp, ext_ptr);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        bus.write_long(sp + 8, 0xCAFE_BABE);
        let preserved = [
            (Register::D3, 0xD300_0003),
            (Register::D4, 0xD400_0004),
            (Register::D5, 0xD500_0005),
            (Register::D6, 0xD600_0006),
            (Register::D7, 0xD700_0007),
            (Register::A2, 0xA200_0002),
            (Register::A3, ext_ptr),
            (Register::A4, 0xA400_0004),
            (Register::A5, 0xA500_0005),
            (Register::A6, 0xA600_0006),
        ];
        for (register, value) in preserved {
            cpu.write_reg(register, value);
        }

        let exact = disp.dispatch_toolbox(true, 0x044, &mut cpu, &mut bus);
        assert!(exact.is_some());
        assert!(exact.unwrap().is_ok());
        assert_eq!(bus.read_long(sp), ext_ptr);
        assert_eq!(bus.read_long(sp + 4), 0x0001_C000);
        assert_eq!(bus.read_long(sp + 8), 0xCAFE_BABE);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        for (register, value) in preserved {
            assert_eq!(
                cpu.read_reg(register),
                value,
                "stack-based X2Fix must preserve {register:?}"
            );
        }
    }

    #[test]
    fn x2fix_saturates_out_of_range_pointer_values() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ext_ptr = bus.alloc(10);

        Extended80::from(40000.0).write_to_bus(&mut bus, ext_ptr);
        bus.write_long(sp, ext_ptr);
        bus.write_long(sp + 4, 0);
        let high = disp.dispatch_toolbox(true, 0x044, &mut cpu, &mut bus);
        assert!(high.is_some());
        assert!(high.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x7FFF_FFFF);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        Extended80::from(-40000.0).write_to_bus(&mut bus, ext_ptr);
        bus.write_long(sp, ext_ptr);
        bus.write_long(sp + 4, 0);
        let low = disp.dispatch_toolbox(true, 0x044, &mut cpu, &mut bus);
        assert!(low.is_some());
        assert!(low.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x8000_0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // Frac2X ($A845)
    // Operating System Utilities 1994, p. 3-46.
    #[test]
    fn frac2x_returns_extended_equivalent_and_pops_fract_argument() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x7000_0000); // 1.75 Fract
        for i in 0..10 {
            bus.write_byte(sp + 4 + i, 0);
        }

        let result = disp.dispatch_toolbox(true, 0x045, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        let ext = Extended80::read_from_bus(&bus, sp + 4);
        assert!((f64::from(ext) - 1.75).abs() < 1e-12);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // X2Frac (0xA846)
    // Inside Macintosh Volume I, I-90–I-91; Operating System Utilities 1994, 3-46.
    #[test]
    fn x2frac_dereferences_extended_pointer_and_preserves_pascal_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ext_ptr = bus.alloc(10);
        Extended80::from(1.75).write_to_bus(&mut bus, ext_ptr);
        bus.write_long(sp, ext_ptr);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        bus.write_long(sp + 8, 0xCAFE_BABE);
        let preserved = [
            (Register::D3, 0xD300_0003),
            (Register::D4, 0xD400_0004),
            (Register::D5, 0xD500_0005),
            (Register::D6, 0xD600_0006),
            (Register::D7, 0xD700_0007),
            (Register::A2, 0xA200_0002),
            (Register::A3, ext_ptr),
            (Register::A4, 0xA400_0004),
            (Register::A5, 0xA500_0005),
            (Register::A6, 0xA600_0006),
        ];
        for (register, value) in preserved {
            cpu.write_reg(register, value);
        }

        let exact = disp.dispatch_toolbox(true, 0x046, &mut cpu, &mut bus);
        assert!(exact.is_some());
        assert!(exact.unwrap().is_ok());
        assert_eq!(bus.read_long(sp), ext_ptr);
        assert_eq!(bus.read_long(sp + 4), 0x7000_0000);
        assert_eq!(bus.read_long(sp + 8), 0xCAFE_BABE);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        for (register, value) in preserved {
            assert_eq!(
                cpu.read_reg(register),
                value,
                "stack-based X2Frac must preserve {register:?}"
            );
        }
    }

    #[test]
    fn x2frac_saturates_out_of_range_pointer_values() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let ext_ptr = bus.alloc(10);

        Extended80::from(3.0).write_to_bus(&mut bus, ext_ptr);
        bus.write_long(sp, ext_ptr);
        bus.write_long(sp + 4, 0);
        let high = disp.dispatch_toolbox(true, 0x046, &mut cpu, &mut bus);
        assert!(high.is_some());
        assert!(high.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x7FFF_FFFF);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        cpu.write_reg(Register::A7, sp);
        Extended80::from(-3.0).write_to_bus(&mut bus, ext_ptr);
        bus.write_long(sp, ext_ptr);
        bus.write_long(sp + 4, 0);
        let low = disp.dispatch_toolbox(true, 0x046, &mut cpu, &mut bus);
        assert!(low.is_some());
        assert!(low.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x8000_0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }

    // FracCos ($A847)
    // Inside Macintosh IV (1986), p. IV-64; OS Utils (1994), p. 3-42.
    #[test]
    fn fraccos_zero_radians_returns_plus_one_fract() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x0000_0000); // 0.0 radians in Fixed
        bus.write_long(sp + 4, 0);
        let result = disp.dispatch_toolbox(true, 0x047, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x4000_0000);
    }

    #[test]
    fn fraccos_consumes_fixed_argument_and_writes_result_slot() {
        // FUNCTION FracCos(x: Fixed): Fract.
        // One 4-byte Fixed argument consumed; 4-byte Fract result at post-pop [SP].
        // Inside Macintosh IV (1986), p. IV-64; OS Utils (1994), p. 3-42.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x0001_0000); // 1.0 radians in Fixed
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x047, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_ne!(
            bus.read_long(sp + 4),
            0xDEAD_BEEF,
            "FracCos should write a Fract result to the function-result slot"
        );
    }

    // FracSin ($A848)
    // Inside Macintosh IV (1986), p. IV-64; OS Utils (1994), p. 3-42.
    #[test]
    fn fracsin_zero_radians_returns_zero_fract() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x0000_0000); // 0.0 radians in Fixed
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x048, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x0000_0000);
    }

    #[test]
    fn fracsin_consumes_fixed_argument_and_writes_result_slot() {
        // FUNCTION FracSin(x: Fixed): Fract.
        // One 4-byte Fixed argument consumed; 4-byte Fract result at post-pop [SP].
        // Inside Macintosh IV (1986), p. IV-64; OS Utils (1994), p. 3-42.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x0001_0000); // 1.0 radians in Fixed
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x048, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_ne!(
            bus.read_long(sp + 4),
            0xDEAD_BEEF,
            "FracSin should write a Fract result to the function-result slot"
        );
    }

    // FracSqrt ($A849)
    // Inside Macintosh IV (1986), pp. IV-64..IV-65; OS Utils (1994), p. 3-41.
    #[test]
    fn fracsqrt_matches_documented_iv65_example_value() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // IM:IV IV-65 example: FracSqrt(X2Frac(1.96)) = $5999999A.
        // X2Frac(1.96) = $7D70A3D7.
        bus.write_long(sp, 0x7D70_A3D7);
        bus.write_long(sp + 4, 0);
        let result = disp.dispatch_toolbox(true, 0x049, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x5999_999A);
    }

    #[test]
    fn fracsqrt_interprets_input_as_unsigned_fract() {
        // IM:IV IV-64 and OS Utils 3-41: FracSqrt interprets x as unsigned
        // 0..4-2^-30, so bit 31 carries weight +2 instead of -2.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // $C0000000 is -1.0 as signed Fract but 3.0 in unsigned Fract domain.
        // sqrt(3.0) in Fract rounds to $6ED9EBA1.
        bus.write_long(sp, 0xC000_0000);
        bus.write_long(sp + 4, 0);
        let result = disp.dispatch_toolbox(true, 0x049, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 4), 0x6ED9_EBA1);
    }

    #[test]
    fn fracsqrt_consumes_fract_argument_and_writes_result_slot() {
        // FUNCTION FracSqrt(x: Fract): Fract.
        // One 4-byte Fract argument consumed; 4-byte Fract result at post-pop [SP].
        // Inside Macintosh IV (1986), p. IV-64; OS Utils (1994), p. 3-41.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0x4000_0000); // 1.0 Fract
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x049, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_ne!(
            bus.read_long(sp + 4),
            0xDEAD_BEEF,
            "FracSqrt should write a Fract result to the function-result slot"
        );
    }

    // FracMul ($A84A)
    // Operating System Utilities 1994, p. 3-40; IM IV 1986, p. IV-65.
    #[test]
    fn fracmul_matches_documented_examples_and_writes_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // IM IV-65: FracMul(X2Frac(1.50), X2Frac(1.30)) = $7CCCCCCD.
        // Stack at entry: SP+0=b, SP+4=a, SP+8=result slot.
        bus.write_long(sp, 0x5333_3333); // X2Frac(1.30)
        bus.write_long(sp + 4, 0x6000_0000); // X2Frac(1.50)
        bus.write_long(sp + 8, 0);
        let pos = disp.dispatch_toolbox(true, 0x04A, &mut cpu, &mut bus);
        assert!(pos.is_some());
        assert!(pos.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 8), 0x7CCC_CCCD);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        // IM IV-65: FracMul(X2Frac(-1.50), X2Frac(1.30)) = $83333333.
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x5333_3333); // X2Frac(1.30)
        bus.write_long(sp + 4, 0xA000_0000); // X2Frac(-1.50)
        bus.write_long(sp + 8, 0);
        let neg = disp.dispatch_toolbox(true, 0x04A, &mut cpu, &mut bus);
        assert!(neg.is_some());
        assert!(neg.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 8), 0x8333_3333);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // FracDiv ($A84B)
    // Operating System Utilities 1994, pp. 3-40 to 3-41; IM IV 1986, p. IV-65.
    #[test]
    fn fracdiv_matches_documented_examples_and_writes_result_slot() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // IM IV-65: FracDiv(X2Frac(1.95), X2Frac(1.30)) = $60000000.
        // Stack at entry: SP+0=b (denominator), SP+4=a (numerator), SP+8=result slot.
        bus.write_long(sp, 0x5333_3333); // X2Frac(1.30)
        bus.write_long(sp + 4, 0x7CCC_CCCD); // X2Frac(1.95)
        bus.write_long(sp + 8, 0);
        let pos = disp.dispatch_toolbox(true, 0x04B, &mut cpu, &mut bus);
        assert!(pos.is_some());
        assert!(pos.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 8), 0x6000_0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        // IM IV-65: FracDiv(X2Frac(-1.95), X2Frac(1.30)) = $A0000000.
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x5333_3333); // X2Frac(1.30)
        bus.write_long(sp + 4, 0x8333_3333); // X2Frac(-1.95)
        bus.write_long(sp + 8, 0);
        let neg = disp.dispatch_toolbox(true, 0x04B, &mut cpu, &mut bus);
        assert!(neg.is_some());
        assert!(neg.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 8), 0xA000_0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // FracDiv ($A84B) divide-by-zero saturation.
    // Operating System Utilities 1994, p. 3-41: when b==0, return
    // $80000000 if a is negative, else $7FFFFFFF (including 0/0).
    #[test]
    fn fracdiv_divide_by_zero_saturates_with_dividend_sign() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        bus.write_long(sp, 0); // b = 0
        bus.write_long(sp + 4, 0x4000_0000); // a = +1.0
        bus.write_long(sp + 8, 0);
        let pos = disp.dispatch_toolbox(true, 0x04B, &mut cpu, &mut bus);
        assert!(pos.is_some());
        assert!(pos.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 8), 0x7FFF_FFFF);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0); // b = 0
        bus.write_long(sp + 4, 0xC000_0000); // a = -1.0
        bus.write_long(sp + 8, 0);
        let neg = disp.dispatch_toolbox(true, 0x04B, &mut cpu, &mut bus);
        assert!(neg.is_some());
        assert!(neg.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 8), 0x8000_0000);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0); // b = 0
        bus.write_long(sp + 4, 0); // a = 0
        bus.write_long(sp + 8, 0);
        let zero = disp.dispatch_toolbox(true, 0x04B, &mut cpu, &mut bus);
        assert!(zero.is_some());
        assert!(zero.unwrap().is_ok());
        assert_eq!(bus.read_long(sp + 8), 0x7FFF_FFFF);
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // ControlStripDispatch ($AAF2) — selector in D0, Pascal arguments on
    // the stack. Universal Interfaces 3.4, ControlStrip.h.
    #[test]
    fn controlstripdispatch_visibility_query_returns_false_without_touching_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000);
        bus.write_word(sp, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2F2, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_word(sp), 0xBEEF);
    }

    #[test]
    fn controlstripdispatch_show_hide_consumes_one_boolean_word() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0101);
        bus.write_word(sp, 0x017F); // showIt = TRUE, with a nonzero pad byte
        bus.write_word(sp + 2, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2F2, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(sp + 2), 0xBEEF);
    }

    #[test]
    fn controlstripdispatch_startup_disk_query_reports_ready_without_touching_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0002);
        bus.write_word(sp, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2F2, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 1);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_word(sp), 0xBEEF);
    }

    #[test]
    fn controlstripdispatch_unknown_selector_returns_paramerr_and_consumes_encoded_args() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0700); // seven argument words, unsupported selector
        for offset in (0..14).step_by(2) {
            bus.write_word(sp + offset, 0xCAFE);
        }

        let result = disp.dispatch_toolbox(true, 0x2F2, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), (-50i16) as i32 as u32);
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(bus.read_word(sp + 12), 0xCAFE);
    }

    // CursorDeviceDispatch ($AADB) — selector in D0, Pascal args on stack.
    #[test]
    fn cursordevicenextdevice_consumes_pointer_arg_and_clears_result() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let device_slot = sp + 0x40;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_000B);
        bus.write_long(sp, device_slot);
        bus.write_word(sp + 4, 0xBEEF);
        bus.write_long(device_slot, 0x1111_2222);

        let result = disp.dispatch_toolbox(true, 0x2DB, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(bus.read_long(device_slot), 0);
    }

    #[test]
    fn cursordevicebuttonop_consumes_full_selector_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0006);
        bus.write_long(sp, 0x1111_2222); // ourDevice
        bus.write_word(sp + 4, 2); // buttonNumber
        bus.write_word(sp + 6, 1); // opcode
        bus.write_long(sp + 8, 0x3333_4444); // data
        bus.write_word(sp + 12, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2DB, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 0);
    }

    // Image Compression Manager Dispatch ($AAA3)
    #[test]
    fn image_compression_align_screen_rect_uses_eight_bit_grid() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let rect = sp + 0x40;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0008_004C);
        bus.write_long(sp, 0); // standard alignment
        bus.write_long(sp + 4, rect);
        bus.write_word(rect, 10); // top
        bus.write_word(rect + 2, 7); // left
        bus.write_word(rect + 4, 110); // bottom
        bus.write_word(rect + 6, 107); // right

        let result = disp.dispatch_toolbox(true, 0x2A3, &mut cpu, &mut bus);
        assert!(result.is_some(), "ImageCompressionDispatch should be handled");
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(rect), 10);
        assert_eq!(bus.read_word(rect + 2), 8);
        assert_eq!(bus.read_word(rect + 4), 110);
        assert_eq!(bus.read_word(rect + 6), 108);
    }

    #[test]
    fn image_compression_align_window_consumes_fourteen_byte_frame() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x000E_004D);
        bus.write_long(sp, 0); // standard alignment procedure
        bus.write_long(sp + 4, 0); // use window bounds
        bus.write_word(sp + 8, 0); // do not bring to front
        bus.write_long(sp + 10, 0); // NIL window is ignored safely

        let result = disp.dispatch_toolbox(true, 0x2A3, &mut cpu, &mut bus);
        assert!(result.is_some(), "ImageCompressionDispatch should be handled");
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 14);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn image_compression_align_screen_rect_preserves_aligned_rect() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let rect = sp + 0x40;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0008_004C);
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, rect);
        bus.write_word(rect, (-20i16) as u16);
        bus.write_word(rect + 2, (-8i16) as u16);
        bus.write_word(rect + 4, 20);
        bus.write_word(rect + 6, 92);

        let result = disp.dispatch_toolbox(true, 0x2A3, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(rect + 2) as i16, -8);
        assert_eq!(bus.read_word(rect + 6) as i16, 92);
    }

    #[test]
    fn image_compression_align_screen_rect_rejects_unimplemented_custom_proc() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0008_004C);
        bus.write_long(sp, 0x1234);
        bus.write_long(sp + 4, sp + 0x40);

        let result = disp.dispatch_toolbox(true, 0x2A3, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_err());
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }

    // Movie Toolbox Dispatch ($AAAA)
    #[test]
    fn movietoolboxdispatch_selector_in_d0_returns_noerr_and_preserves_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0001);
        cpu.write_reg(Register::D1, 0x1111_2222);
        cpu.write_reg(Register::A0, 0x3333_4444);
        cpu.write_reg(Register::A1, 0x5555_6666);
        bus.write_word(sp, 0xCAFE);
        bus.write_word(sp + 2, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "MovieToolboxDispatch should be handled");
        assert!(
            result.unwrap().is_ok(),
            "MovieToolboxDispatch should return"
        );
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::D1), 0x1111_2222);
        assert_eq!(cpu.read_reg(Register::A0), 0x3333_4444);
        assert_eq!(cpu.read_reg(Register::A1), 0x5555_6666);
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(bus.read_word(sp), 0);
        assert_eq!(bus.read_word(sp + 2), 0xBEEF);
    }

    #[test]
    fn movietoolboxdispatch_exit_movies_clears_movie_state_and_preserves_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0187); // NewMovie
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);
        let create = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(create.is_some(), "NewMovie should be handled");
        assert!(create.unwrap().is_ok(), "NewMovie should return");
        let movie = bus.read_long(sp + 4);
        assert!(disp.movie_states.contains_key(&movie));
        disp.movie_error = -2010;
        disp.movie_sticky_error = -2010;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0002); // ExitMovies
        bus.write_word(sp, 0xCAFE);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "ExitMovies should be handled");
        assert!(result.unwrap().is_ok(), "ExitMovies should return");
        assert_eq!(cpu.read_reg(Register::A7), sp);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(sp), 0xCAFE);
        assert!(disp.movie_states.is_empty());
        assert_eq!(disp.movie_error, 0);
        assert_eq!(disp.movie_sticky_error, 0);
    }

    #[test]
    fn movietoolboxdispatch_new_movie_returns_empty_movie_with_current_gworld() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp
            .current_port
            .with_mut(|current_port| *current_port = 0x0033_0000);
        disp.current_gdevice
            .with_mut(|current_gdevice| *current_gdevice = 0x0044_0000);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0187); // NewMovie
        bus.write_long(sp, 0x0000_0001);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "NewMovie should be handled");
        assert!(result.unwrap().is_ok(), "NewMovie should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);

        let movie = bus.read_long(sp + 4);
        assert_ne!(movie, 0);
        assert_eq!(bus.read_long(movie), u32::from_be_bytes(*b"MooV"));
        assert_eq!(bus.read_long(movie + 4), 0x0000_0001);
        assert_eq!(bus.read_long(movie + 8), 0x0033_0000);
        assert_eq!(bus.read_long(movie + 12), 0x0044_0000);
        let state = disp
            .movie_states
            .get(&movie)
            .expect("NewMovie should register MovieState");
        assert_eq!(state.box_rect, (0, 0, 120, 160));
        assert_eq!(state.gworld_port, 0x0033_0000);
        assert_eq!(state.gworld_gdh, 0x0044_0000);
        assert!(!state.active);
        assert_eq!(state.duration, 1);
    }

    #[test]
    fn movietoolboxdispatch_update_movie_pops_and_reports_validity() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0187); // NewMovie
        bus.write_long(sp, 0);
        bus.write_long(sp + 4, 0);
        let create = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(create.is_some(), "NewMovie should be handled");
        assert!(create.unwrap().is_ok(), "NewMovie should return");
        let movie = bus.read_long(sp + 4);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x001F); // UpdateMovie
        bus.write_long(sp, movie);
        bus.write_word(sp + 4, 0xBEEF);
        let valid = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(valid.is_some(), "UpdateMovie should be handled");
        assert!(valid.unwrap().is_ok(), "UpdateMovie should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(bus.read_word(sp + 4), 0);
        assert_eq!(disp.movie_error, 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x001F); // UpdateMovie
        bus.write_long(sp, 0x00FF_EE00);
        bus.write_word(sp + 4, 0xBEEF);
        let invalid = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(invalid.is_some(), "UpdateMovie should be handled");
        assert!(invalid.unwrap().is_ok(), "UpdateMovie should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), (-2010i16) as u32);
        assert_eq!(bus.read_word(sp + 4), (-2010i16) as u16);
        assert_eq!(disp.movie_error, -2010);
        assert_eq!(disp.movie_sticky_error, -2010);
    }

    #[test]
    fn movietoolboxdispatch_open_movie_file_opens_vfs_movie_and_writes_refnum() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let spec_ptr = 0x300000;
        let ref_num_ptr = 0x300100;
        let movies_dir = disp.ensure_vfs_directory("AmoebArena/Movies");
        disp.vfs.insert(
            "AmoebArena/Movies/C&G".to_string(),
            vec![0x6D, 0x6F, 0x6F, 0x76],
        );
        disp.vfs_rsrc
            .insert("AmoebArena/Movies/C&G".to_string(), Vec::new());
        disp.ensure_vfs_catalog();
        write_test_fsspec(&mut bus, spec_ptr, -1, movies_dir, b"C&G");
        bus.write_word(ref_num_ptr, 0xCAFE);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0192);
        bus.write_word(sp, 0x0100); // fsRdPerm as SignedByte in the high byte.
        bus.write_long(sp + 2, ref_num_ptr);
        bus.write_long(sp + 6, spec_ptr);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "MovieToolboxDispatch should be handled");
        assert!(result.unwrap().is_ok(), "OpenMovieFile should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        let refnum = bus.read_word(ref_num_ptr);
        assert_ne!(refnum, 0);
        assert_ne!(refnum, 0xCAFE);
        assert_eq!(
            disp.resource_file_name(refnum).map(str::to_owned),
            Some("AmoebArena/Movies/C&G".to_string())
        );
        assert_eq!(disp.current_resource_refnum(), refnum);
    }

    #[test]
    fn movietoolboxdispatch_open_movie_file_missing_sets_current_and_sticky_error() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let spec_ptr = 0x300000;
        let ref_num_ptr = 0x300100;
        let movies_dir = disp.ensure_vfs_directory("AmoebArena/Movies");
        disp.ensure_vfs_catalog();
        write_test_fsspec(&mut bus, spec_ptr, -1, movies_dir, b"Missing");
        bus.write_word(ref_num_ptr, 0xCAFE);
        disp.movie_error = 0;
        disp.movie_sticky_error = 0;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0192); // OpenMovieFile
        bus.write_word(sp, 0x0100); // fsRdPerm as SignedByte in the high byte.
        bus.write_long(sp + 2, ref_num_ptr);
        bus.write_long(sp + 6, spec_ptr);
        bus.write_word(sp + 10, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "OpenMovieFile should be handled");
        assert!(result.unwrap().is_ok(), "OpenMovieFile should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 10);
        assert_eq!(bus.read_word(sp + 10), (-43i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-43i16) as u32);
        assert_eq!(
            bus.read_word(ref_num_ptr),
            0xCAFE,
            "missing movie must not overwrite resRefNum"
        );
        assert_eq!(disp.movie_error, -43);
        assert_eq!(disp.movie_sticky_error, -43);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0003); // GetMoviesError
        bus.write_word(sp, 0xBEEF);
        let current = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(current.is_some(), "GetMoviesError should be handled");
        assert!(current.unwrap().is_ok(), "GetMoviesError should return");
        assert_eq!(bus.read_word(sp), (-43i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-43i16) as u32);
        assert_eq!(disp.movie_error, 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0004); // GetMoviesStickyError
        bus.write_word(sp, 0xBEEF);
        let sticky = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(sticky.is_some(), "GetMoviesStickyError should be handled");
        assert!(
            sticky.unwrap().is_ok(),
            "GetMoviesStickyError should return"
        );
        assert_eq!(bus.read_word(sp), (-43i16) as u16);
        assert_eq!(cpu.read_reg(Register::D0), (-43i16) as u32);
    }

    #[test]
    fn movie_data_fork_loader_checks_offsets_and_pascal_stack() {
        let (mut disp, mut cpu, mut bus) = setup();
        let mut mvhd = vec![0; 100];
        mvhd[12..16].copy_from_slice(&600u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&1200u32.to_be_bytes());
        let mut file = vec![0x77; 32];
        file.extend(quicktime_atom(*b"moov", &quicktime_atom(*b"mvhd", &mvhd)));
        disp.vfs.insert("score".to_string(), file);
        disp.open_files.insert(128, "score".to_string());
        for (offset, expected) in [(32, 0i16), (0, -2002), (u32::MAX, -2002)] {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, 0x01B3);
            bus.write_long(TEST_SP, 0x300010);
            bus.write_word(TEST_SP + 4, 1);
            bus.write_long(TEST_SP + 6, offset);
            bus.write_word(TEST_SP + 10, 128);
            bus.write_long(TEST_SP + 12, 0x300000);
            bus.write_long(TEST_SP + 18, 0xCAFE_BABE);
            disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 16);
            assert_eq!(bus.read_word(TEST_SP + 16) as i16, expected);
            assert_eq!(bus.read_long(TEST_SP + 18), 0xCAFE_BABE);
            if expected == 0 {
                let movie = bus.read_long(0x300000);
                assert_eq!(disp.movie_states[&movie].duration, 1200);
                assert_eq!(disp.movie_states[&movie].time_scale, 600);
                assert_eq!(bus.read_byte(0x300010), 0);
            } else {
                assert_eq!(bus.read_long(0x300000), 0);
            }
        }
    }

    #[test]
    fn movie_music_controls_stop_mute_seek_and_loop() {
        let (mut disp, mut cpu, mut bus) = setup();
        let movie = 0x300100;
        let mut state = super::MovieState::new(0, -1, 1, (0, 0, 0, 0), 600, 600);
        state.music = Some(vec![super::super::movie_media::MusicNote {
            start: 0.0,
            duration: 0.8,
            pitch: 60.0,
            velocity: 1.0,
            part: 0,
        }]);
        disp.movie_states.insert(movie, state);
        let call = |disp: &mut TrapDispatcher,
                    cpu: &mut MockCpu,
                    bus: &mut MacMemoryBus,
                    selector: u32| {
            cpu.write_reg(Register::A7, TEST_SP);
            cpu.write_reg(Register::D0, selector);
            disp.dispatch_toolbox(true, 0x2AA, cpu, bus)
                .unwrap()
                .unwrap();
        };
        bus.write_long(TEST_SP, movie);
        call(&mut disp, &mut cpu, &mut bus, 0x0012);
        assert_eq!(bus.read_long(TEST_SP + 4), movie);
        bus.write_long(TEST_SP, 1);
        bus.write_long(TEST_SP + 4, movie);
        call(&mut disp, &mut cpu, &mut bus, 0x00B2);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP + 8);
        bus.write_long(TEST_SP, movie);
        call(&mut disp, &mut cpu, &mut bus, 0x000B);
        let mut output = Vec::new();
        disp.mix_movie_music(&mut output, 2205);
        assert!(output.iter().any(|&b| b != 128));
        bus.write_word(TEST_SP, 0);
        bus.write_long(TEST_SP + 2, movie);
        call(&mut disp, &mut cpu, &mut bus, 0x002F);
        output.clear();
        disp.mix_movie_music(&mut output, 2205);
        assert!(output.iter().all(|&b| b == 128));
        bus.write_long(TEST_SP, movie);
        call(&mut disp, &mut cpu, &mut bus, 0x000D);
        assert_eq!(disp.movie_states[&movie].audio_time, 0.0);
        disp.mix_movie_music(&mut Vec::new(), 22050 + 2205);
        assert!((disp.movie_states[&movie].audio_time - 0.1).abs() < 1e-9);
        bus.write_long(TEST_SP, movie);
        call(&mut disp, &mut cpu, &mut bus, 0x000E);
        assert_eq!(disp.movie_states[&movie].current_time, 600);
        assert_eq!(disp.movie_states[&movie].audio_time, 600.0);
        bus.write_long(TEST_SP, movie);
        call(&mut disp, &mut cpu, &mut bus, 0x000C);
        output.clear();
        disp.mix_movie_music(&mut output, 2205);
        assert!(output.is_empty());
    }

    #[test]
    fn movietoolboxdispatch_new_movie_from_file_returns_movie_and_resource_id() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let movie_ptr = 0x300200;
        let res_id_ptr = 0x300204;
        let res_name_ptr = 0x300208;
        let changed_ptr = 0x300310;
        let refnum = 123;

        disp.register_empty_resource_file(refnum);
        disp.set_resource_file_name(refnum, "Movies/Intro");
        disp.install_named_test_resource_in_file(&mut bus, refnum, *b"moov", 128, "Intro", b"moov");
        bus.write_long(movie_ptr, 0);
        bus.write_word(res_id_ptr, 0);
        bus.write_byte(res_name_ptr, 0xEE);
        bus.write_byte(changed_ptr, 0xEE);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00F0);
        bus.write_long(sp, changed_ptr);
        bus.write_word(sp + 4, 1); // newMovieActive
        bus.write_long(sp + 6, res_name_ptr);
        bus.write_long(sp + 10, res_id_ptr);
        bus.write_word(sp + 14, refnum);
        bus.write_long(sp + 16, movie_ptr);
        bus.write_word(sp + 20, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "MovieToolboxDispatch should be handled");
        assert!(result.unwrap().is_ok(), "NewMovieFromFile should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 20);
        assert_eq!(bus.read_word(sp + 20), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_ne!(bus.read_long(movie_ptr), 0);
        assert_eq!(bus.read_word(res_id_ptr), 128);
        assert_eq!(bus.read_pstring(res_name_ptr), b"Intro");
        assert_eq!(bus.read_byte(changed_ptr), 0);
    }

    fn quicktime_atom(atom_type: [u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut atom = Vec::with_capacity(8 + payload.len());
        atom.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
        atom.extend_from_slice(&atom_type);
        atom.extend_from_slice(payload);
        atom
    }

    #[test]
    fn quicktime_movie_metadata_reads_mvhd_duration_and_tkhd_bounds() {
        let mut mvhd = vec![0; 20];
        mvhd[12..16].copy_from_slice(&600u32.to_be_bytes());
        mvhd[16..20].copy_from_slice(&240u32.to_be_bytes());

        let mut tkhd = vec![0; 84];
        tkhd[76..80].copy_from_slice(&(320u32 << 16).to_be_bytes());
        tkhd[80..84].copy_from_slice(&(200u32 << 16).to_be_bytes());

        let trak = quicktime_atom(*b"trak", &quicktime_atom(*b"tkhd", &tkhd));
        let mut moov_payload = quicktime_atom(*b"mvhd", &mvhd);
        moov_payload.extend_from_slice(&trak);
        let moov = quicktime_atom(*b"moov", &moov_payload);

        let (bounds, duration, time_scale) = quicktime_movie_metadata(&moov);
        assert_eq!(bounds, (0, 0, 200, 320));
        assert_eq!(duration, 240);
        assert_eq!(time_scale, 600);
    }

    fn create_test_movie_from_file(
        disp: &mut TrapDispatcher,
        cpu: &mut MockCpu,
        bus: &mut MacMemoryBus,
        sp: u32,
    ) -> u32 {
        let movie_ptr = 0x300400;
        let res_id_ptr = 0x300404;
        let res_name_ptr = 0x300408;
        let changed_ptr = 0x300510;
        let refnum = 124;

        disp.register_empty_resource_file(refnum);
        disp.set_resource_file_name(refnum, "Movies/Test");
        disp.install_named_test_resource_in_file(bus, refnum, *b"moov", 128, "Test", b"moov");
        bus.write_long(movie_ptr, 0);
        bus.write_word(res_id_ptr, 0);
        bus.write_byte(res_name_ptr, 0);
        bus.write_byte(changed_ptr, 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00F0);
        bus.write_long(sp, changed_ptr);
        bus.write_word(sp + 4, 1);
        bus.write_long(sp + 6, res_name_ptr);
        bus.write_long(sp + 10, res_id_ptr);
        bus.write_word(sp + 14, refnum);
        bus.write_long(sp + 16, movie_ptr);
        bus.write_word(sp + 20, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x2AA, cpu, bus);
        assert!(result.is_some(), "NewMovieFromFile should be handled");
        assert!(result.unwrap().is_ok(), "NewMovieFromFile should return");
        let movie = bus.read_long(movie_ptr);
        assert_ne!(movie, 0, "NewMovieFromFile should create a movie token");
        movie
    }

    #[test]
    fn movietoolboxdispatch_movie_spatial_selectors_pop_and_round_trip_box() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let movie = create_test_movie_from_file(&mut disp, &mut cpu, &mut bus, sp);
        let box_ptr = 0x300600;
        let out_box_ptr = 0x300620;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0016); // SetMovieGWorld
        bus.write_long(sp, 0); // gdh
        bus.write_long(sp + 4, 0x0022_0000); // port
        bus.write_long(sp + 8, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetMovieGWorld should be handled");
        assert!(result.unwrap().is_ok(), "SetMovieGWorld should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);

        bus.write_word(box_ptr, 7);
        bus.write_word(box_ptr + 2, 11);
        bus.write_word(box_ptr + 4, 107);
        bus.write_word(box_ptr + 6, 211);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00FA); // SetMovieBox
        bus.write_long(sp, box_ptr);
        bus.write_long(sp + 4, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetMovieBox should be handled");
        assert!(result.unwrap().is_ok(), "SetMovieBox should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        for offset in (0..8).step_by(2) {
            bus.write_word(out_box_ptr + offset, 0xEEEE);
        }
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00F9); // GetMovieBox
        bus.write_long(sp, out_box_ptr);
        bus.write_long(sp + 4, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMovieBox should be handled");
        assert!(result.unwrap().is_ok(), "GetMovieBox should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(bus.read_word(out_box_ptr), 7);
        assert_eq!(bus.read_word(out_box_ptr + 2), 11);
        assert_eq!(bus.read_word(out_box_ptr + 4), 107);
        assert_eq!(bus.read_word(out_box_ptr + 6), 211);
    }

    #[test]
    fn movietoolboxdispatch_movie_playback_selectors_pop_and_report_done() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let movie = create_test_movie_from_file(&mut disp, &mut cpu, &mut bus, sp);

        // Begin playback at tick 0.
        disp.set_tick_count_for_test(&mut bus, 0);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x000B); // StartMovie
        bus.write_long(sp, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "StartMovie should be handled");
        assert!(result.unwrap().is_ok(), "StartMovie should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        // Advance the guest clock well past the movie duration, then service:
        // MoviesTask is timeline-driven, so the movie completes only after real
        // elapsed time covers its duration.
        disp.set_tick_count_for_test(&mut bus, 100_000);
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0005); // MoviesTask
        bus.write_long(sp, 0); // maxMilliSecToUse: service once
        bus.write_long(sp + 4, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "MoviesTask should be handled");
        assert!(result.unwrap().is_ok(), "MoviesTask should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00DD); // IsMovieDone
        bus.write_long(sp, movie);
        bus.write_word(sp + 4, 0xBEEF);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "IsMovieDone should be handled");
        assert!(result.unwrap().is_ok(), "IsMovieDone should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0x0100);
    }

    #[test]
    fn movietoolboxdispatch_movie_preload_duration_and_volume_selectors_pop() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let movie = create_test_movie_from_file(&mut disp, &mut cpu, &mut bus, sp);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x002B); // GetMovieDuration
        bus.write_long(sp, movie);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMovieDuration should be handled");
        assert!(result.unwrap().is_ok(), "GetMovieDuration should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_ne!(bus.read_long(sp + 4), 0xDEAD_BEEF);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x000E); // GoToEndOfMovie
        bus.write_long(sp, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "GoToEndOfMovie should be handled");
        assert!(result.unwrap().is_ok(), "GoToEndOfMovie should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(
            disp.movie_states[&movie].current_time,
            disp.movie_states[&movie].duration
        );

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00F4); // SetMoviePreferredRate
        bus.write_long(sp, 0x0002_0000);
        bus.write_long(sp + 4, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetMoviePreferredRate should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SetMoviePreferredRate should return"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00F3); // GetMoviePreferredRate
        bus.write_long(sp, movie);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "GetMoviePreferredRate should be handled");
        assert!(
            result.unwrap().is_ok(),
            "GetMoviePreferredRate should return"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_long(sp + 4), 0x0002_0000);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x002F); // SetMovieVolume
        bus.write_word(sp, 0x0100);
        bus.write_long(sp + 2, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetMovieVolume should be handled");
        assert!(result.unwrap().is_ok(), "SetMovieVolume should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00F6); // SetMoviePreferredVolume
        bus.write_word(sp, 0x0080);
        bus.write_long(sp + 2, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "SetMoviePreferredVolume should be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "SetMoviePreferredVolume should return"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 6);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00F5); // GetMoviePreferredVolume
        bus.write_long(sp, movie);
        bus.write_word(sp + 4, 0xBEEF);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "GetMoviePreferredVolume should be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "GetMoviePreferredVolume should return"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 0x0080);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0006); // PrerollMovie
        bus.write_long(sp, 0x0001_0000); // rate
        bus.write_long(sp + 4, 0); // time
        bus.write_long(sp + 8, movie);
        bus.write_word(sp + 12, 0xBEEF);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "PrerollMovie should be handled");
        assert!(result.unwrap().is_ok(), "PrerollMovie should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 12);
        assert_eq!(bus.read_word(sp + 12), 0);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0007); // LoadMovieIntoRam
        bus.write_long(sp, 0); // flags
        bus.write_long(sp + 4, 1); // duration
        bus.write_long(sp + 8, 0); // time
        bus.write_long(sp + 12, movie);
        bus.write_word(sp + 16, 0xBEEF);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "LoadMovieIntoRam should be handled");
        assert!(result.unwrap().is_ok(), "LoadMovieIntoRam should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 16);
        assert_eq!(bus.read_word(sp + 16), 0);
    }

    #[test]
    fn movietoolboxdispatch_dispose_movie_pops_and_releases_movie_state() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let movie = create_test_movie_from_file(&mut disp, &mut cpu, &mut bus, sp);
        assert!(disp.movie_states.contains_key(&movie));

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0023); // DisposeMovie
        bus.write_long(sp, movie);
        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(result.is_some(), "DisposeMovie should be handled");
        assert!(result.unwrap().is_ok(), "DisposeMovie should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(!disp.movie_states.contains_key(&movie));
    }

    #[test]
    fn movietoolboxdispatch_dispose_movie_controller_consumes_component_instance() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x018B);
        bus.write_long(sp, 0x1234_5678);

        let result = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);

        assert!(result.is_some(), "DisposeMovieController should be handled");
        assert!(
            result.unwrap().is_ok(),
            "DisposeMovieController should return"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D0), 0);
    }

    #[test]
    fn movietoolboxdispatch_close_movie_file_closes_data_fork_and_writes_noerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let spec_ptr = 0x300800;
        let ref_num_ptr = 0x300900;
        let movies_dir = disp.ensure_vfs_directory("AmoebArena/Movies");
        disp.vfs.insert(
            "AmoebArena/Movies/C&G".to_string(),
            vec![0x6D, 0x6F, 0x6F, 0x76],
        );
        disp.ensure_vfs_catalog();
        write_test_fsspec(&mut bus, spec_ptr, -1, movies_dir, b"C&G");

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0192); // OpenMovieFile
        bus.write_word(sp, 0x0100);
        bus.write_long(sp + 2, ref_num_ptr);
        bus.write_long(sp + 6, spec_ptr);
        bus.write_word(sp + 10, 0xBEEF);
        let open = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(open.is_some(), "OpenMovieFile should be handled");
        assert!(open.unwrap().is_ok(), "OpenMovieFile should return");
        let refnum = bus.read_word(ref_num_ptr);
        assert!(disp.open_files.contains_key(&refnum));
        assert_eq!(
            disp.resource_file_name(refnum).map(str::to_owned),
            Some("AmoebArena/Movies/C&G".to_string())
        );
        let data_ptr = bus.alloc(8);
        bus.write_bytes(data_ptr, &[0xC0; 8]);
        let handle = bus.alloc(4);
        bus.write_long(handle, data_ptr);
        disp.insert_resource_pointer_for_test(refnum, (*b"PICT", 23002), data_ptr);
        disp.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"PICT", 23002));
        disp.insert_resource_handle_file_for_test(handle, refnum);
        disp.track_handle_ptr(data_ptr, handle);

        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x00D5); // CloseMovieFile
        bus.write_word(sp, refnum);
        bus.write_word(sp + 2, 0xBEEF);
        let close = disp.dispatch_toolbox(true, 0x2AA, &mut cpu, &mut bus);
        assert!(close.is_some(), "CloseMovieFile should be handled");
        assert!(close.unwrap().is_ok(), "CloseMovieFile should return");
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(bus.read_word(sp + 2), 0);
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert!(!disp.open_files.contains_key(&refnum));
        assert!(disp.resource_file_name(refnum).is_none());
        assert_eq!(bus.get_alloc_size(data_ptr), None);
        assert_eq!(bus.get_alloc_size(handle), None);
        assert!(!disp.loaded_handles.contains_key(&handle));
        assert!(!disp.resource_handle_files.contains_key(&handle));
        assert_eq!(disp.handle_for_ptr(data_ptr), None);
    }

    // Unhandled trap returns None
    #[test]
    fn test_unhandled_trap_returns_none() {
        let (mut disp, mut cpu, mut bus) = setup();

        let result = disp.dispatch_toolbox(true, 0xFFF, &mut cpu, &mut bus);
        assert!(result.is_none(), "Unhandled trap should return None");
    }

    // FixATan2 ($A818)
    // FUNCTION FixATan2(x, y: LongInt): Fixed;
    // Inside Macintosh Volume IV (1986), p. IV-65.
    //
    // Verifies the IM:IV IV-65 documented value bit-exactly in the
    // Systemless HLE: FixATan2(X2Fix(1.0), X2Fix(1.0)) = 0x0000C910. Pascal
    // LTR push: x first (lands at SP+4), y last (lands at SP+0). Trap pops
    // 8 arg bytes and writes the 4-byte Fixed result into the slot at
    // former SP+8.
    #[test]
    fn fixatan2_returns_im_documented_pi_over_four_for_one_one() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // Pascal LTR push: x first, y last → y at SP+0, x at SP+4.
        bus.write_long(sp, 0x0001_0000); // y = X2Fix(1.0)
        bus.write_long(sp + 4, 0x0001_0000); // x = X2Fix(1.0)
        bus.write_long(sp + 8, 0xDEAD_BEEF); // result-slot poison

        let result = disp.dispatch_toolbox(true, 0x018, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());

        // IM:IV IV-65: FixATan2(X2Fix(1.00), X2Fix(1.00)) = $0000C910
        assert_eq!(bus.read_long(sp + 8), 0x0000_C910);
        // 8 arg bytes consumed.
        assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    }

    // FixATan2 ($A818): scale invariance — only y/x ratio matters.
    // Per IM:IV IV-65 "arctan(type/type) -> Fixed" note.
    #[test]
    fn fixatan2_only_ratio_matters_scale_invariance() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // First call: raw LONGINT 1:1 ratio (y=2, x=2).
        bus.write_long(sp, 2);
        bus.write_long(sp + 4, 2);
        let _ = disp
            .dispatch_toolbox(true, 0x018, &mut cpu, &mut bus)
            .unwrap();
        let raw_result = bus.read_long(sp + 8);

        // Second call: Fixed 1:1 ratio (y=X2Fix(1), x=X2Fix(1)).
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, 0x0001_0000);
        bus.write_long(sp + 4, 0x0001_0000);
        let _ = disp
            .dispatch_toolbox(true, 0x018, &mut cpu, &mut bus)
            .unwrap();
        let fixed_result = bus.read_long(sp + 8);

        // Same ratio → same Fixed result; both equal IM:IV IV-65 documented value.
        assert_eq!(raw_result, fixed_result);
        assert_eq!(raw_result, 0x0000_C910);
    }

    #[test]
    fn fixround_consumes_fixed_parameter_and_writes_integer_result_slot() {
        // FixRound consumes one four-byte Fixed parameter and returns a
        // two-byte INTEGER. MPW emits CLR.W -(SP), MOVE.L x,-(SP), _FixRound,
        // MOVE.W (SP)+,Dn, so trap return must advance A7 by four bytes to the
        // caller-allocated result slot. Inside Macintosh Volume I, I-467.
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        bus.write_long(sp, 0x0000_8000); // 0.5 Fixed parameter
        bus.write_word(sp + 4, 0xBEEF); // INTEGER result-slot poison

        let result = disp.dispatch_toolbox(true, 0x06C, &mut cpu, &mut bus);
        assert!(result.expect("FixRound must be handled").is_ok());

        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(bus.read_word(sp + 4), 1);
        assert_eq!(
            bus.read_long(sp),
            0x0000_8000,
            "FixRound must not overwrite its Fixed parameter"
        );
    }

    // SysError ($A9C9) must halt the runner so the halt PC reports the
    // originating SysError call site instead of letting the game execute
    // past it into invalid territory.
    // PROCEDURE SysError(errorCode: INTEGER);
    // Inside Macintosh Volume II (1985), pp. II-358 to II-359;
    // Inside Macintosh: Operating System Utilities (1994), pp. 2-13 to 2-14.
    #[test]
    fn test_syserror_writes_ds_err_code_and_halts_runner() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        // Push errorCode (INTEGER, 16-bit) at SP.
        bus.write_word(sp, 0x002A); // dsCoreErr-style code; value irrelevant
        bus.write_word(crate::memory::globals::addr::DS_ERR_CODE, 0xBEEF);

        let result = disp.dispatch_toolbox(true, 0x1C9, &mut cpu, &mut bus);
        // Some(Err(Halted)) — handler matched AND signalled halt.
        let inner = result.expect("SysError must be a handled trap");
        assert!(
            matches!(inner, Err(crate::Error::Halted)),
            "SysError must return Err(Halted), got {:?}",
            inner
        );
        // Stack-discipline: errorCode (2 bytes) consumed, A7 advanced.
        assert_eq!(cpu.read_reg(Register::A7), sp + 2);
        assert_eq!(
            bus.read_word(crate::memory::globals::addr::DS_ERR_CODE),
            0x002A
        );
    }

    #[test]
    fn initresources_returns_minus_one_for_nominal_call() {
        let (mut disp, mut cpu, mut bus) = setup();

        cpu.write_reg(Register::A7, TEST_SP);
        let init = disp.dispatch_toolbox(true, 0x195, &mut cpu, &mut bus);
        assert!(init.is_some(), "InitResources should be handled");
        assert!(
            init.unwrap().is_ok(),
            "InitResources should return normally"
        );
        assert_eq!(bus.read_word(TEST_SP) as i16, -1);
        assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    }

    #[test]
    fn rsrczoneinit_preserves_stack_pointer() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);

        let result = disp.dispatch_toolbox(true, 0x196, &mut cpu, &mut bus);
        assert!(result.is_some(), "RsrcZoneInit should be handled");
        assert!(
            result.unwrap().is_ok(),
            "RsrcZoneInit should return normally"
        );
        assert_eq!(cpu.read_reg(Register::A7), sp_before);
    }

    #[test]
    fn pack11_generated_routes_preserve_exact_word_values() {
        assert_eq!(super::PACK11_OPERATION_ROUTES.len(), 30);
        assert!(super::PACK11_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0100, "InitEditionPack"),
            (0x0206, "UnRegisterSection"),
            (0x0208, "IsRegisteredSection"),
            (0x0210, "DeleteEditionContainerFile"),
            (0x0224, "GoToPublisherSection"),
            (0x0226, "GetLastEditionContainerUsed"),
            (0x022A, "GetEditionOpenerProc"),
            (0x022C, "SetEditionOpenerProc"),
            (0x0232, "NewSubscriberDialog"),
            (0x0236, "NewPublisherDialog"),
            (0x023A, "SectionOptionsDialog"),
            (0x0316, "CloseEdition"),
            (0x040C, "AssociateSection"),
            (0x0412, "OpenEdition"),
            (0x0422, "GetEditionInfo"),
            (0x050E, "CreateEditionContainerFile"),
            (0x052E, "CallEditionOpenerProc"),
            (0x0530, "CallFormatIOProc"),
            (0x0604, "RegisterSection"),
            (0x0618, "EditionHasFormat"),
            (0x061E, "GetEditionFormatMark"),
            (0x0620, "SetEditionFormatMark"),
            (0x0814, "OpenNewEdition"),
            (0x081A, "ReadEdition"),
            (0x081C, "WriteEdition"),
            (0x0A02, "NewSection"),
            (0x0A28, "GetStandardFormats"),
            (0x0B34, "NewSubscriberExpDialog"),
            (0x0B38, "NewPublisherExpDialog"),
            (0x0B3C, "SectionOptionsExpDialog"),
        ] {
            let route = super::pack11_operation_route(0xA82D, selector).expect("Pack11 route");
            assert_eq!(route.routine_name, routine_name);
            assert_eq!(
                route.operation_id,
                format!("selector-operation:_Pack11:0x{selector:04X}:d0-low-word-immediate:16")
            );
        }

        for (trap_word, selector) in [
            (0xA92D, 0x0100),
            (0xA82D, 0x0000),
            (0xA82D, 0x0101),
            (0xA82D, 0x0207),
            (0xA82D, 0x303C),
        ] {
            assert!(super::pack11_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn pack11_all_30_routes_pop_exact_param_bytes_and_return_expected_oserr() {
        for route in super::PACK11_OPERATION_ROUTES {
            let (mut disp, mut cpu, mut bus) = setup();
            let sp = TEST_SP;
            let selector = route.selector as u16;
            let expected_err: i16 = if selector == 0x0100 { 0 } else { -450 };
            let param_bytes = (((selector >> 8) & 0xFF) as u32) * 2;
            let result_sp = sp + param_bytes;

            // Fill argument region with deterministic pattern
            let original_args: Vec<u8> = (0..param_bytes)
                .map(|i| (selector as u8).wrapping_add(i as u8).wrapping_add(0x33))
                .collect();
            for (offset, &byte) in original_args.iter().enumerate() {
                bus.write_byte(sp + offset as u32, byte);
            }

            // Poison the result slot and guard regions
            bus.write_word(result_sp, 0x55AA);
            bus.write_long(sp - 8, 0x1234_5678);
            bus.write_long(result_sp + 2, 0x8765_4321);

            disp.current_trap_word = 0xA82D;
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D0, 0xDEAD_0000 | u32::from(selector)); // stale high word

            let result = disp.dispatch_toolbox(true, 0x02D, &mut cpu, &mut bus);
            assert!(
                result.is_some(),
                "Pack11 should handle selector {selector:#06X}"
            );
            assert!(
                result.unwrap().is_ok(),
                "Pack11 selector {selector:#06X} should return Ok"
            );

            assert_eq!(
                disp.current_selector_operation,
                Some(route.operation_id),
                "operation ID for {selector:#06X}"
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                result_sp,
                "A7 should advance exactly by param_bytes ({param_bytes}) for {selector:#06X}"
            );
            assert_eq!(
                bus.read_word(result_sp),
                expected_err as u16,
                "result slot at SP+{param_bytes} for {selector:#06X}"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                expected_err as i32 as u32,
                "sign-extended D0 for {selector:#06X}"
            );

            // Verify arguments and guard memory are preserved
            for (offset, &byte) in original_args.iter().enumerate() {
                assert_eq!(
                    bus.read_byte(sp + offset as u32),
                    byte,
                    "argument byte at offset {offset} must remain invariant for {selector:#06X}"
                );
            }
            assert_eq!(
                bus.read_long(sp - 8),
                0x1234_5678,
                "underflow guard preserved for {selector:#06X}"
            );
            assert_eq!(
                bus.read_long(result_sp + 2),
                0x8765_4321,
                "overflow guard preserved for {selector:#06X}"
            );
        }
    }

    #[test]
    fn pack11_min_and_max_frames_preserve_arguments_and_hidden_version_word() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        // 1. Min frame: InitEditionPack ($0100, 1 word / 2 bytes)
        // Hidden version word $0011 pushed by Universal Interfaces inline macro
        disp.current_trap_word = 0xA82D;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xCAFE_0100);
        bus.write_word(sp, 0x0011); // curEditionMgrVers
        bus.write_word(sp + 2, 0xBEEF); // OSErr result slot poison

        let result = disp.dispatch_toolbox(true, 0x02D, &mut cpu, &mut bus);
        assert!(result.expect("Pack11 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack11:0x0100:d0-low-word-immediate:16")
        );
        assert_eq!(
            bus.read_word(sp),
            0x0011,
            "InitEditionPack hidden version word at SP+0 must remain unchanged"
        );
        assert_eq!(
            bus.read_word(sp + 2),
            0,
            "InitEditionPack result slot at SP+2 must receive noErr (0)"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp + 2,
            "InitEditionPack A7 must advance to result slot SP+2"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "InitEditionPack D0 must return sign-extended noErr (0)"
        );

        // 2. Max frame: SectionOptionsExpDialog ($0B3C, 11 words / 22 bytes)
        let max_sp = TEST_SP;
        cpu.write_reg(Register::A7, max_sp);
        cpu.write_reg(Register::D0, 0xBEEF_0B3C);
        for i in 0..22 {
            bus.write_byte(max_sp + i, (i as u8) + 1);
        }
        bus.write_word(max_sp + 22, 0xCAFE); // result slot poison

        let result = disp.dispatch_toolbox(true, 0x02D, &mut cpu, &mut bus);
        assert!(result.expect("Pack11 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_Pack11:0x0B3C:d0-low-word-immediate:16")
        );
        for i in 0..22 {
            assert_eq!(
                bus.read_byte(max_sp + i),
                (i as u8) + 1,
                "SectionOptionsExpDialog arg byte at offset {i} must remain unchanged"
            );
        }
        assert_eq!(
            bus.read_word(max_sp + 22),
            (-450i16) as u16,
            "SectionOptionsExpDialog result slot at SP+22 must receive editionMgrInitErr (-450)"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            max_sp + 22,
            "SectionOptionsExpDialog A7 must advance to result slot SP+22"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-450i32) as u32,
            "SectionOptionsExpDialog D0 must return sign-extended editionMgrInitErr (-450)"
        );
    }

    #[test]
    fn pack11_fail_closed_result_preserves_output_buffers() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        let out_section_handle = TEST_SP + 0x100;

        disp.current_trap_word = 0xA82D;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xABCD_0A02); // NewSection ($0A02, 20 arg bytes)
                                                  // Pascal entry order: sectionH*, mode, ID, kind, document, container.
        bus.write_long(sp, out_section_handle);
        bus.write_word(sp + 4, 0);
        bus.write_long(sp + 6, 42);
        bus.write_word(sp + 10, 1);
        bus.write_long(sp + 12, 0x2000);
        bus.write_long(sp + 16, 0x1000);
        bus.write_word(sp + 20, 0xBEEF); // result slot
        bus.write_long(out_section_handle, 0x1122_3344);

        let result = disp.dispatch_toolbox(true, 0x02D, &mut cpu, &mut bus);
        assert!(result.expect("Pack11 arm").is_ok());
        assert_eq!(cpu.read_reg(Register::A7), sp + 20);
        assert_eq!(cpu.read_reg(Register::D0), (-450i32) as u32);
        assert_eq!(bus.read_word(sp + 20), (-450i16) as u16);
        assert_eq!(
            bus.read_long(out_section_handle),
            0x1122_3344,
            "NewSection must not touch its output buffer on editionMgrInitErr"
        );
    }

    #[test]
    fn pack11_unknown_selector_and_trap_mismatch_preserve_memory_and_return_paramerr() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;

        for &unknown_selector in &[0x00FFu16, 0x303C, 0x0000, 0x0200, 0x0207, 0x020A, 0xFFFF] {
            disp.current_selector_operation = Some("stale-identity");
            disp.current_trap_word = 0xA82D;
            cpu.write_reg(Register::A7, sp);
            cpu.write_reg(Register::D0, 0xFACE_0000 | u32::from(unknown_selector));
            bus.write_long(sp, 0x1122_3344);
            bus.write_long(sp + 4, 0x5566_7788);

            let result = disp.dispatch_toolbox(true, 0x02D, &mut cpu, &mut bus);
            assert!(result.expect("Pack11 arm").is_ok());
            assert_eq!(
                disp.current_selector_operation, None,
                "unknown selector {unknown_selector:#06X} must clear stale operation identity"
            );
            assert_eq!(
                cpu.read_reg(Register::A7),
                sp,
                "unknown selector {unknown_selector:#06X} must preserve A7"
            );
            assert_eq!(
                cpu.read_reg(Register::D0),
                (-50i32) as u32,
                "unknown selector {unknown_selector:#06X} must return sign-extended paramErr (-50)"
            );
            assert_eq!(
                bus.read_long(sp),
                0x1122_3344,
                "guest stack memory must remain invariant for unknown selector {unknown_selector:#06X}"
            );
            assert_eq!(
                bus.read_long(sp + 4),
                0x5566_7788,
                "guest stack memory must remain invariant for unknown selector {unknown_selector:#06X}"
            );
        }

        // Trap word mismatch: trap word 0xA92D with Pack11 selector 0x0100
        disp.current_selector_operation = Some("stale-identity");
        disp.current_trap_word = 0xA92D;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0100);
        bus.write_long(sp, 0x1122_3344);

        let result = disp.dispatch_toolbox(true, 0x02D, &mut cpu, &mut bus);
        assert!(result.expect("Pack11 arm").is_ok());
        assert_eq!(
            disp.current_selector_operation, None,
            "trap word mismatch must clear stale operation identity"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp,
            "trap word mismatch must preserve A7"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "trap word mismatch must return sign-extended paramErr (-50)"
        );
        assert_eq!(
            bus.read_long(sp),
            0x1122_3344,
            "guest stack memory must remain invariant on trap word mismatch"
        );
    }

    #[test]
    fn translation_dispatch_generated_routes_preserve_exact_register_values() {
        assert_eq!(super::TRANSLATION_DISPATCH_OPERATION_ROUTES.len(), 6);
        assert!(super::TRANSLATION_DISPATCH_OPERATION_ROUTES
            .windows(2)
            .all(|pair| pair[0].selector < pair[1].selector));

        for (selector, routine_name) in [
            (0x0001, "UpdateTranslationProgress"),
            (0x0002, "SetTranslationAdvertisement"),
            (0x0009, "ExtendFileTypeList"),
            (0x000C, "TranslateFile"),
            (0x001C, "GetFileTypesThatAppCanNativelyOpen"),
            (0x001E, "CanDocBeOpened"),
        ] {
            let route = super::translation_dispatch_operation_route(0xABFC, selector)
                .expect("TranslationDispatch route");
            assert_eq!(route.routine_name, routine_name);
            assert_eq!(
                route.operation_id,
                format!(
                    "selector-operation:_TranslationDispatch:0x{selector:04X}:d0-moveq-immediate:8"
                )
            );
        }

        for (trap_word, selector) in [
            (0xA8FC, 0x0001),
            (0xA9FC, 0x0009),
            (0xAAFC, 0x001C),
            (0xABFC, 0x0000),
            (0xABFC, 0x0003),
            (0xABFC, 0x0008),
            (0xABFC, 0x0010),
            (0xABFC, 0x0020),
            (0xABFC, 0x7001),
            (0xABFC, 0x701C),
            (0xABFC, 0x0001_0001),
            (0xABFC, 0xDEAD_001C),
            (0xABFC, 0xFFFF_0009),
            (0xABFC, 0x1C00),
        ] {
            assert!(super::translation_dispatch_operation_route(trap_word, selector).is_none());
        }
    }

    #[test]
    fn translation_dispatch_records_known_then_clears_unknown_and_preserves_invariants() {
        let (mut disp, mut cpu, mut bus) = setup();
        let sp = TEST_SP;
        disp.current_trap_word = 0xABFC;

        // Known selector: GetFileTypesThatAppCanNativelyOpen ($001C)
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_001C);
        cpu.write_reg(Register::D1, 0x1122_3344);
        cpu.write_reg(Register::D2, 0x5566_7788);
        cpu.write_reg(Register::A0, 0x99AA_BBCC);
        cpu.write_reg(Register::A1, 0xDDEE_FF00);
        cpu.write_reg(Register::A2, 0x1357_2468);
        bus.write_long(sp, 0xCAFE_BABE);
        bus.write_long(sp + 4, 0xDEAD_BEEF);
        bus.write_long(sp - 8, 0x0123_4567);

        let result = disp.dispatch_toolbox(true, 0x3FC, &mut cpu, &mut bus);
        assert!(result.is_some(), "TranslationDispatch should be handled");
        assert!(result.unwrap().is_ok(), "TranslationDispatch should return");
        assert_eq!(
            disp.current_selector_operation,
            Some("selector-operation:_TranslationDispatch:0x001C:d0-moveq-immediate:8")
        );
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D1), 0x1122_3344);
        assert_eq!(cpu.read_reg(Register::D2), 0x5566_7788);
        assert_eq!(cpu.read_reg(Register::A0), 0x99AA_BBCC);
        assert_eq!(cpu.read_reg(Register::A1), 0xDDEE_FF00);
        assert_eq!(cpu.read_reg(Register::A2), 0x1357_2468);
        assert_eq!(bus.read_long(sp), 0xCAFE_BABE);
        assert_eq!(bus.read_long(sp + 4), 0xDEAD_BEEF);
        assert_eq!(bus.read_long(sp - 8), 0x0123_4567);

        // Unknown selector ($0000): must clear operation identity and preserve invariants
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_0000);
        let result = disp.dispatch_toolbox(true, 0x3FC, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(cpu.read_reg(Register::D1), 0x1122_3344);
        assert_eq!(bus.read_long(sp), 0xCAFE_BABE);

        // Stale high word ($DEAD_001C): must clear operation identity
        disp.current_selector_operation = Some("stale-identity");
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0xDEAD_001C);
        let result = disp.dispatch_toolbox(true, 0x3FC, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);

        // Trap word mismatch (0xA8FC): must clear operation identity
        disp.current_selector_operation = Some("stale-identity");
        disp.current_trap_word = 0xA8FC;
        cpu.write_reg(Register::A7, sp);
        cpu.write_reg(Register::D0, 0x0000_001C);
        let result = disp.dispatch_toolbox(true, 0x3FC, &mut cpu, &mut bus);
        assert!(result.is_some());
        assert!(result.unwrap().is_ok());
        assert_eq!(disp.current_selector_operation, None);
        assert_eq!(cpu.read_reg(Register::D0) as i16, -50);
        assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    }
