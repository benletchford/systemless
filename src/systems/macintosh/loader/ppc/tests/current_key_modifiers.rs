use super::*;

#[test]
fn current_key_modifiers_tracks_hardware_keys_without_mouse_flags() {
    for library in [b"CarbonLib".as_slice(), b"InterfaceLib"] {
        let pef = synthetic_pef_with_library_import(library, b"GetCurrentKeyModifiers");
        let mut loaded = load_pef_application(&pef).unwrap();
        for (keys, expected) in [
            (Vec::<u8>::new(), 0),
            (vec![0x37], 0x0100),
            (vec![0x38], 0x0200),
            (vec![0x39], 0x0400),
            (vec![0x3a], 0x0800),
            (vec![0x3b], 0x1000),
            (vec![0x3c], 0x2200),
            (vec![0x3d], 0x4800),
            (vec![0x3e], 0x9000),
            (vec![0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e], 0xff00),
            (vec![0], 0),
            (vec![], 0),
        ] {
            for mouse_button in [false, true] {
                let mut input = PpcInputSnapshot {
                    mouse_button,
                    ..Default::default()
                };
                for key in &keys {
                    input.key_map[usize::from(*key) / 8] |= 1 << (*key % 8);
                }
                loaded.set_input_snapshot(input);
                loaded.cpu.pc = loaded.entry_pc;
                loaded.cpu.lr = PPC_HALT_PC;
                loaded.cpu.gpr[3] = 0xdeadbeef;
                let probe = loaded.run_with_hle_imports(64);
                assert_eq!(probe.handled_import_count, 1);
                assert_eq!(probe.unsupported_import_index, None);
                assert_eq!(
                    loaded.cpu.gpr[3], expected,
                    "keys {keys:?}, mouse {mouse_button}"
                );
            }
        }
    }
}
