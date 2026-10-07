use super::*;

#[test]
fn low_memory_ghost_window_accessor_reads_current_pointer() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"LMGetGhostWindow")).unwrap();
    loaded.memory.write_u32_be(crate::memory::globals::addr::GHOST_WINDOW, 0x1234_5678).unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetGhostWindow);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn low_memory_cur_deactive_accessors_roundtrip() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"LMSetCurDeactive")).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::LMSetCurDeactive
    );
    loaded.cpu.gpr[3] = 0x1234_5678;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetCurDeactive);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(crate::memory::globals::addr::CUR_DEACTIVE),
        Some(0x1234_5678)
    );
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetCurDeactive);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn hle_import_runner_unpacks_packbits_and_advances_pointer_variables() {
    let pef = synthetic_pef_with_import(b"UnpackBits");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert!(matches!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::UnpackBits
    ));
    let base = PPC_DATA_BASE + 0x7000;
    let source_variable = base;
    let destination_variable = base + 4;
    let source = base + 16;
    let destination = base + 64;
    loaded.memory.add_region(base, vec![0; 128]);
    loaded.memory.write_u32_be(source_variable, source).unwrap();
    loaded.memory.write_u32_be(destination_variable, destination).unwrap();
    // Three literals, three repeated bytes, a no-op, then two literals.
    loaded.memory.write_bytes(source, &[2, b'A', b'B', b'C', 0xfe, b'X', 0x80, 1, b'Y', b'Z']).unwrap();
    loaded.cpu.gpr[3] = source_variable;
    loaded.cpu.gpr[4] = destination_variable;
    loaded.cpu.gpr[5] = 8;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(ppc_memory_read_bytes(&mut loaded.memory, destination, 8), Some(b"ABCXXXYZ".to_vec()));
    assert_eq!(loaded.memory.read_u32_be(source_variable), Some(source + 10));
    assert_eq!(loaded.memory.read_u32_be(destination_variable), Some(destination + 8));
}

#[test]
fn hle_import_runner_unpackbits_rejects_unreadable_input_without_advancing() {
    let pef = synthetic_pef_with_import(b"UnpackBits");
    let mut loaded = load_pef_application(&pef).unwrap();
    let base = PPC_DATA_BASE + 0x7000;
    let source_variable = base;
    let destination_variable = base + 4;
    let source = base + 127;
    let destination = base + 64;
    loaded.memory.add_region(base, vec![0; 128]);
    loaded.memory.write_u32_be(source_variable, source).unwrap();
    loaded.memory.write_u32_be(destination_variable, destination).unwrap();
    loaded.memory.write_u8(source, 0).unwrap();
    loaded.cpu.gpr[3] = source_variable;
    loaded.cpu.gpr[4] = destination_variable;
    loaded.cpu.gpr[5] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.memory.read_u32_be(source_variable), Some(source));
    assert_eq!(loaded.memory.read_u32_be(destination_variable), Some(destination));
    assert_eq!(loaded.memory.read_u8(destination), Some(0));
}

#[test]
fn hle_import_runner_reports_missing_international_resource_table() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetIntlResourceTable"),
        PpcImportDispatcherTarget::GetIntlResourceTable
    );
    let pef = synthetic_pef_with_import(b"GetIntlResourceTable");
    let mut loaded = load_pef_application(&pef).unwrap();
    let outputs = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(outputs, vec![0xaa; 16]);
    loaded.cpu.gpr[3] = 0; // Roman script
    loaded.cpu.gpr[4] = 0; // word-selection table
    loaded.cpu.gpr[5] = outputs;
    loaded.cpu.gpr[6] = outputs + 4;
    loaded.cpu.gpr[7] = outputs + 8;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(outputs), Some(0));
    assert_eq!(loaded.memory.read_u32_be(outputs + 4), Some(0));
    assert_eq!(loaded.memory.read_u32_be(outputs + 8), Some(0));
    assert_eq!(loaded.memory.read_u32_be(outputs + 12), Some(0xaaaa_aaaa));
}

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
fn iu_equal_pstring_uses_primary_roman_order_for_default_script() {
    let pef = synthetic_pef_with_import(b"IUEqualPString");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::IUEqualPString
    );
    let left_ptr = PPC_DATA_BASE + 0x1000;
    let right_ptr = PPC_DATA_BASE + 0x1040;
    loaded.memory.add_region(left_ptr, vec![0; 64]);
    loaded.memory.add_region(right_ptr, vec![0; 64]);
    write_ppc_pstring(&mut loaded.memory, left_ptr, b"Rose");
    write_ppc_pstring(&mut loaded.memory, right_ptr, b"ros\x8e");
    loaded.cpu.gpr[3] = left_ptr;
    loaded.cpu.gpr[4] = right_ptr;
    loaded.cpu.gpr[5] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::IUEqualPString);
    assert_eq!(loaded.cpu.gpr[3], 0);

    write_ppc_pstring(&mut loaded.memory, right_ptr, b"Rope");
    loaded.cpu.gpr[3] = left_ptr;
    loaded.cpu.gpr[4] = right_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::IUEqualPString);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = left_ptr;
    loaded.cpu.gpr[4] = right_ptr;
    loaded.cpu.gpr[5] = PPC_HEAP_BASE;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, Some(0));
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
fn toolbox_random_uses_quickdraw_globals_seed_after_init_graf() {
    let pef = synthetic_pef_with_import(b"Random");
    let mut loaded = load_pef_application(&pef).unwrap();
    let global_ptr = PPC_DATA_BASE + 0x2000;
    let quickdraw_seed = global_ptr - 126;
    loaded.memory.add_region(quickdraw_seed, vec![0; 130]);
    loaded.memory.write_u32_be(quickdraw_seed, 12345).unwrap();
    loaded.memory.write_u32_be(PPC_RAND_SEED_ADDR, 99).unwrap();
    loaded.toolbox_startup.init_graf_global_ptr = global_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.memory.read_u32_be(quickdraw_seed), Some(207482415));
    assert_eq!(loaded.memory.read_u32_be(PPC_RAND_SEED_ADDR), Some(99));
    assert_eq!(loaded.cpu.gpr[3], u32::from(207482415u32 as u16));
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
fn carbon_copy_c_string_to_pascal_handles_overlap_and_str255_limit() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CopyCStringToPascal"),
        PpcImportDispatcherTarget::CopyCStringToPascal
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CopyCStringToPascal"))
        .unwrap();
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 800]);

    loaded.memory.write_bytes(base, b"Hello\0").unwrap();
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = base + 16;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyCStringToPascal);
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, base + 16).as_deref(),
        Some(b"Hello".as_slice())
    );

    loaded.memory.write_bytes(base + 64, b"World\0").unwrap();
    loaded.cpu.gpr[3] = base + 64;
    loaded.cpu.gpr[4] = base + 64;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyCStringToPascal);
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, base + 64).as_deref(),
        Some(b"World".as_slice())
    );

    loaded.memory.write_bytes(base + 128, &vec![b'Q'; 300]).unwrap();
    loaded.memory.write_u8(base + 428, 0).unwrap();
    loaded.cpu.gpr[3] = base + 128;
    loaded.cpu.gpr[4] = base + 448;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyCStringToPascal);
    assert_eq!(loaded.memory.read_u8(base + 448), Some(255));
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, base + 448).unwrap(),
        vec![b'Q'; 255]
    );
}

#[test]
fn carbon_copy_pascal_string_to_c_handles_overlap_and_str255_limit() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CopyPascalStringToC"),
        PpcImportDispatcherTarget::CopyPascalStringToC
    );
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CopyPascalStringToC")).unwrap();
    let base = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(base, vec![0; 800]);

    loaded.memory.write_bytes(base, b"\x05Hello").unwrap();
    loaded.cpu.gpr[3] = base;
    loaded.cpu.gpr[4] = base + 16;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyPascalStringToC);
    assert_eq!(
        (0..6)
            .map(|i| loaded.memory.read_u8(base + 16 + i))
            .collect::<Option<Vec<_>>>(),
        Some(b"Hello\0".to_vec())
    );

    loaded.memory.write_bytes(base + 64, b"\x05World").unwrap();
    loaded.cpu.gpr[3] = base + 64;
    loaded.cpu.gpr[4] = base + 64;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyPascalStringToC);
    assert_eq!(
        (0..6)
            .map(|i| loaded.memory.read_u8(base + 64 + i))
            .collect::<Option<Vec<_>>>(),
        Some(b"World\0".to_vec())
    );

    loaded.memory.write_u8(base + 128, 0).unwrap();
    loaded.memory.write_u8(base + 144, b'Q').unwrap();
    loaded.cpu.gpr[3] = base + 128;
    loaded.cpu.gpr[4] = base + 144;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyPascalStringToC);
    assert_eq!(loaded.memory.read_u8(base + 144), Some(0));

    let payload = vec![b'Z'; 255];
    loaded.memory.write_u8(base + 256, 255).unwrap();
    loaded.memory.write_bytes(base + 257, &payload).unwrap();
    loaded.cpu.gpr[3] = base + 256;
    loaded.cpu.gpr[4] = base + 512;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyPascalStringToC);
    assert_eq!(
        (0..255)
            .map(|i| loaded.memory.read_u8(base + 512 + i))
            .collect::<Option<Vec<_>>>(),
        Some(payload)
    );
    assert_eq!(loaded.memory.read_u8(base + 767), Some(0));
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
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "FlushEvents"),
        PpcImportDispatcherTarget::FlushEvents
    );
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

    for what in [3, 6, 1, 4] {
        loaded.event_queue.push_back(PpcQueuedEvent {
            what,
            message: 0,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        });
    }
    run_toolbox_import(
        &mut loaded,
        PpcImportDispatcherTarget::FlushEvents,
        0x0001_ffff,
        0x0002_0002,
    );
    assert_eq!(loaded.toolbox_startup.flush_events_count, 1);
    assert_eq!(loaded.toolbox_startup.last_flush_event_mask, 0xffff);
    assert_eq!(loaded.toolbox_startup.last_flush_stop_mask, 0x0002);
    assert_eq!(
        loaded.event_queue.iter().map(|event| event.what).collect::<Vec<_>>(),
        vec![6, 1, 4]
    );

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

#[test]
fn hle_import_runner_handles_date_and_time_utilities() {
    let pef = synthetic_pef_with_import(b"GetDateTime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let secs_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(secs_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = secs_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(secs_ptr),
        Some(PPC_FIXED_MAC_TIME)
    );

    let pef = synthetic_pef_with_import(b"LMGetTime");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_FIXED_MAC_TIME);

    for (seconds, expected) in [
        (0, [1904, 1, 1, 0, 0, 0, 6]),
        (
            59 * 86_400 + 23 * 3_600 + 59 * 60 + 58,
            [1904, 2, 29, 23, 59, 58, 2],
        ),
        (366 * 86_400, [1905, 1, 1, 0, 0, 0, 1]),
    ] {
        let pef = synthetic_pef_with_import(b"SecondsToDate");
        let mut loaded = load_pef_application(&pef).unwrap();
        let date_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(date_ptr, vec![0xaa; 14]);
        loaded.cpu.gpr[3] = seconds;
        loaded.cpu.gpr[4] = date_ptr;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        for (index, field) in expected.into_iter().enumerate() {
            assert_eq!(
                loaded.memory.read_u16_be(date_ptr + index as u32 * 2),
                Some(field),
                "field {index} for {seconds} seconds"
            );
        }
    }
}
