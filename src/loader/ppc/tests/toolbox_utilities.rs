use super::*;

#[test]
fn hle_import_runner_handles_legacy_bit_utilities() {
    let mut set = load_pef_application(&synthetic_pef_with_import(b"BitSet")).unwrap();
    let byte = PPC_HEAP_BASE;
    set.memory.add_region(byte, vec![0; 2]);
    set.cpu.gpr[3] = byte;
    set.cpu.gpr[4] = 9;
    let set_probe = set.run_with_hle_imports(64);
    assert_eq!(set_probe.unsupported_import_index, None);
    assert_eq!(set.memory.read_u8(byte + 1), Some(0x40));

    let mut clear = load_pef_application(&synthetic_pef_with_import(b"BitClr")).unwrap();
    clear.memory.add_region(byte, vec![0xff; 2]);
    clear.cpu.gpr[3] = byte;
    clear.cpu.gpr[4] = 9;
    let clear_probe = clear.run_with_hle_imports(64);
    assert_eq!(clear_probe.unsupported_import_index, None);
    assert_eq!(clear.memory.read_u8(byte + 1), Some(0xbf));

    let mut not = load_pef_application(&synthetic_pef_with_import(b"BitNot")).unwrap();
    not.cpu.gpr[3] = 0x0f0f_55aa;
    let not_probe = not.run_with_hle_imports(64);
    assert_eq!(not_probe.unsupported_import_index, None);
    assert_eq!(not.cpu.gpr[3], 0xf0f0_aa55);
}

#[test]
fn hle_import_runner_gets_and_sets_random_seed_low_memory_global() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetRndSeed"),
        PpcImportDispatcherTarget::LMSetRndSeed
    );
    let pef = synthetic_pef_with_import(b"LMSetRndSeed");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_RAND_SEED_ADDR),
        Some(0x1234_5678)
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LMGetRndSeed;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn hle_import_runner_handles_num_to_string() {
    let pef = synthetic_pef_with_import(b"NumToString");
    let mut loaded = load_pef_application(&pef).unwrap();
    let string_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(string_ptr, vec![0xaa; 32]);
    loaded.cpu.gpr[3] = (-12345i32) as u32;
    loaded.cpu.gpr[4] = string_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, string_ptr).as_deref(),
        Some(b"-12345".as_slice())
    );
}

#[test]
fn hle_import_runner_handles_string_to_num() {
    let pef = synthetic_pef_with_import(b"StringToNum");
    let mut loaded = load_pef_application(&pef).unwrap();
    let string_ptr = PPC_DATA_BASE + 0x1000;
    let number_ptr = PPC_DATA_BASE + 0x1040;
    loaded.memory.add_region(string_ptr, vec![0; 64]);
    loaded.memory.add_region(number_ptr, vec![0xaa; 4]);
    write_ppc_pstring(&mut loaded.memory, string_ptr, b" -12345x");
    loaded.cpu.gpr[3] = string_ptr;
    loaded.cpu.gpr[4] = number_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_ptr);
    assert_eq!(
        loaded.memory.read_u32_be(number_ptr),
        Some((-12345i32) as u32)
    );
}

#[test]
fn hle_import_runner_handles_equal_string() {
    let pef = synthetic_pef_with_import(b"EqualString");
    let mut loaded = load_pef_application(&pef).unwrap();
    let left_ptr = PPC_DATA_BASE + 0x1000;
    let right_ptr = PPC_DATA_BASE + 0x1040;
    loaded.memory.add_region(left_ptr, vec![0; 64]);
    loaded.memory.add_region(right_ptr, vec![0; 64]);
    write_ppc_pstring(&mut loaded.memory, left_ptr, b"Gridz");
    write_ppc_pstring(&mut loaded.memory, right_ptr, b"gridz");
    loaded.cpu.gpr[3] = left_ptr;
    loaded.cpu.gpr[4] = right_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = left_ptr;
    loaded.cpu.gpr[4] = right_ptr;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_handles_random() {
    let pef = synthetic_pef_with_import(b"Random");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .write_u32_be(PPC_RAND_SEED_ADDR, 12345)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_RAND_SEED_ADDR),
        Some(207482415)
    );
    assert_eq!(loaded.cpu.gpr[3], u32::from(207482415u32 as u16));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_ne!(loaded.cpu.gpr[3], u32::from(207482415u32 as u16));

    loaded
        .memory
        .write_u32_be(PPC_RAND_SEED_ADDR, 32768)
        .unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_bit_tst_uses_msb_first_bit_offsets() {
    let pef = synthetic_pef_with_import(b"BitTst");
    let mut loaded = load_pef_application(&pef).unwrap();
    let keymap = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(keymap, vec![0; 8]);
    // BitTst counts left-to-right from the high-order bit and accepts
    // offsets beyond the first byte. Inside Macintosh: Operating System
    // Utilities (1994), pp. 3-7 and 3-28.
    loaded.memory.write_u8(keymap, 0x20).unwrap();
    loaded.memory.write_u8(keymap + 6, 0x40).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::BitTst
    );

    for bit in [2, 49] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = keymap;
        loaded.cpu.gpr[4] = bit;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 1);
    }

    loaded.memory.write_u8(keymap, 0).unwrap();
    loaded.memory.write_u8(keymap + 6, 0).unwrap();
    for bit in [2, 49] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = keymap;
        loaded.cpu.gpr[4] = bit;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
}

#[test]
fn getkeys_lsb_bitmap_integrates_with_msb_first_bit_tst() {
    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let keymap = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(keymap, vec![0; 16]);
    let mut input = PpcInputSnapshot::default();
    for key_code in [0x00, 0x31] {
        let (byte, mask) = crate::trap::dispatch::key_map_byte_mask(key_code).unwrap();
        input.key_map[byte] |= mask;
    }
    loaded.set_input_snapshot(input);
    loaded.cpu.gpr[3] = keymap;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u8(keymap), Some(0x01));
    assert_eq!(loaded.memory.read_u8(keymap + 6), Some(0x02));

    // Inside Macintosh Volume I (1985), pp. I-259–I-260 maps virtual key
    // codes directly to KeyMap indexes. BitTst independently counts from
    // each byte's high-order bit (Operating System Utilities, 1994,
    // pp. 3-7 and 3-28), so a KeyMap key is tested at keyCode XOR 7.
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::BitTst;
    for (bit, expected) in [(7, 1), (54, 1), (0, 0), (49, 0)] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = keymap;
        loaded.cpu.gpr[4] = bit;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected, "BitTst offset {bit}");
    }
}

#[test]
fn hle_import_runner_handles_p2cstr() {
    let pef = synthetic_pef_with_import(b"p2cstr");
    let mut loaded = load_pef_application(&pef).unwrap();
    let string_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(string_ptr, vec![0xaa; 32]);
    write_ppc_pstring(&mut loaded.memory, string_ptr, b"Gridz");
    loaded.cpu.gpr[3] = string_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_ptr);
    assert_eq!(
        (0..6)
            .map(|offset| loaded.memory.read_u8(string_ptr + offset).unwrap())
            .collect::<Vec<_>>(),
        b"Gridz\0".to_vec()
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::C2PStr;
    loaded.cpu.gpr[3] = string_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], string_ptr);
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, string_ptr).as_deref(),
        Some(b"Gridz".as_slice())
    );
}

#[test]
fn hle_import_runner_upper_text_converts_only_the_requested_bytes() {
    let pef = synthetic_pef_with_import(b"UpperText");
    let mut loaded = load_pef_application(&pef).unwrap();
    let text_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(text_ptr, vec![b'a', b'z', 0x8e, b'!', b'q']);
    loaded.cpu.gpr[3] = text_ptr;
    loaded.cpu.gpr[4] = 3;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], text_ptr);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, text_ptr, 5),
        Some(vec![b'A', b'Z', 0x83, b'!', b'q'])
    );
}

#[test]
fn import_bindings_classify_toolbox_init_and_dialog_lifecycle_imports() {
    for (symbol, target) in [
        ("InitGraf", PpcImportDispatcherTarget::InitGraf),
        ("InitFonts", PpcImportDispatcherTarget::InitFonts),
        ("InitWindows", PpcImportDispatcherTarget::InitWindows),
        ("InitMenus", PpcImportDispatcherTarget::InitMenus),
        ("TEInit", PpcImportDispatcherTarget::TEInit),
        ("InitDialogs", PpcImportDispatcherTarget::InitDialogs),
        ("FlushEvents", PpcImportDispatcherTarget::FlushEvents),
        ("CloseDialog", PpcImportDispatcherTarget::CloseDialog),
        ("DisposeDialog", PpcImportDispatcherTarget::DisposeDialog),
        ("DisposDialog", PpcImportDispatcherTarget::DisposeDialog),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
}

#[test]
fn import_bindings_classify_timing_and_date_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDateTime"),
        PpcImportDispatcherTarget::GetDateTime
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetTime"),
        PpcImportDispatcherTarget::GetTime
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Delay"),
        PpcImportDispatcherTarget::Delay
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetDblTime"),
        PpcImportDispatcherTarget::GetDblTime
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetTime"),
        PpcImportDispatcherTarget::LMGetTime
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SecondsToDate"),
        PpcImportDispatcherTarget::SecondsToDate
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Secs2Date"),
        PpcImportDispatcherTarget::SecondsToDate
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Microseconds"),
        PpcImportDispatcherTarget::Microseconds
    );
}

#[test]
fn import_bindings_classify_text_and_conversion_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SysEnvirons"),
        PpcImportDispatcherTarget::SysEnvirons
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TextWidth"),
        PpcImportDispatcherTarget::TextWidth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "StringWidth"),
        PpcImportDispatcherTarget::StringWidth
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EqualString"),
        PpcImportDispatcherTarget::EqualString
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "X2Fix"),
        PpcImportDispatcherTarget::X2Fix
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NumToString"),
        PpcImportDispatcherTarget::NumToString
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "StringToNum"),
        PpcImportDispatcherTarget::StringToNum
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Random"),
        PpcImportDispatcherTarget::Random
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "p2cstr"),
        PpcImportDispatcherTarget::P2CStr
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "c2pstr"),
        PpcImportDispatcherTarget::C2PStr
    );
}

#[test]
fn hle_import_runner_handles_lm_get_current_a5() {
    let pef = synthetic_pef_with_import(b"LMGetCurrentA5");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_DATA_BASE);
}

#[test]
fn hle_import_runner_tracks_toolbox_startup_manager_state() {
    fn run_toolbox_import(
        loaded: &mut PpcLoadedApp,
        target: PpcImportDispatcherTarget,
        gpr3: u32,
        gpr4: u32,
    ) -> PpcHleRunProbe {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;
        loaded.cpu.gpr[3] = gpr3;
        loaded.cpu.gpr[4] = gpr4;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert!(
            matches!(
                probe.result,
                PpcRunResult::Halted {
                    pc: PPC_HALT_PC,
                    ..
                }
            ),
            "{:?}",
            probe.result
        );
        assert_eq!(loaded.cpu.gpr[3], gpr3);
        assert_eq!(loaded.cpu.gpr[4], gpr4);
        probe
    }

    let pef = synthetic_pef_with_import(b"InitGraf");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(loaded.toolbox_startup, PpcToolboxStartupState::default());

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitGraf,
        PPC_DATA_BASE + 0x40,
        0x1234_5678,
    );
    assert_eq!(loaded.toolbox_startup.init_graf_count, 1);
    assert_eq!(
        loaded.toolbox_startup.init_graf_global_ptr,
        PPC_DATA_BASE + 0x40
    );

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitFonts,
        0xfeed_face,
        0x1111_2222,
    );
    assert!(loaded.toolbox_startup.fonts_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitWindows,
        0xabcd_1234,
        0x2222_3333,
    );
    assert!(loaded.toolbox_startup.windows_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitMenus,
        0x8765_4321,
        0x3333_4444,
    );
    assert!(loaded.toolbox_startup.menus_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::TEInit,
        0x1357_2468,
        0x4444_5555,
    );
    assert!(loaded.toolbox_startup.text_edit_initialized);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::InitDialogs,
        0x2468_1357,
        0x5555_6666,
    );
    assert!(loaded.toolbox_startup.dialogs_initialized);
    assert_eq!(loaded.toolbox_startup.dialog_resume_proc, 0x2468_1357);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::FlushEvents,
        0x0001_ffff,
        0x0002_0002,
    );
    assert_eq!(loaded.toolbox_startup.flush_events_count, 1);
    assert_eq!(loaded.toolbox_startup.last_flush_event_mask, 0xffff);
    assert_eq!(loaded.toolbox_startup.last_flush_stop_mask, 0x0002);

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::SetEventMask,
        0x0000_ffdf,
        0,
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(crate::memory::globals::addr::SYS_EVT_MASK),
        Some(0xffdf)
    );

    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::DisposeDialog,
        PPC_HEAP_BASE + 0x80,
        0x6666_7777,
    );
    assert_eq!(loaded.toolbox_startup.dispose_dialog_count, 1);
    assert_eq!(
        loaded.toolbox_startup.last_disposed_dialog,
        PPC_HEAP_BASE + 0x80
    );
}

#[test]
fn native_lmgetdefltstack_reads_the_live_low_memory_long() {
    let pef = synthetic_pef_with_import(b"LMGetDefltStack");
    let mut loaded = load_pef_application(&pef).unwrap();
    let address = crate::memory::globals::addr::DEFLT_STACK;

    assert_eq!(
        loaded.memory.read_u32_be(address),
        Some(crate::memory::globals::DEFAULT_DEFLT_STACK_SIZE)
    );

    loaded.memory.write_u32_be(address, 0x1234_5678).unwrap();
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetDefltStack);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn native_lmgetcurstackbase_reads_the_live_low_memory_long() {
    let pef = synthetic_pef_with_import(b"LMGetCurStackBase");
    let mut loaded = load_pef_application(&pef).unwrap();
    let address = crate::memory::globals::addr::CUR_STACK_BASE;

    assert_eq!(loaded.memory.read_u32_be(address), Some(loaded.stack_base));

    loaded.memory.write_u32_be(address, 0x2345_6780).unwrap();
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetCurStackBase);
    assert_eq!(loaded.cpu.gpr[3], 0x2345_6780);
}
