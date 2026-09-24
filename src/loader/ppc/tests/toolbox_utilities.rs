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
