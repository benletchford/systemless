use super::*;
use crate::memory::globals::{addr, DEFAULT_AUTO_KEY_RATE_TICKS, DEFAULT_AUTO_KEY_THRESHOLD_TICKS};

#[test]
fn keyboard_timing_getters_report_initialized_preferences_through_both_libraries() {
    for library in [b"InterfaceLib".as_slice(), b"CarbonLib".as_slice()] {
        for (symbol, expected) in [
            (
                b"LMGetKeyThresh".as_slice(),
                DEFAULT_AUTO_KEY_THRESHOLD_TICKS,
            ),
            (b"LMGetKeyRepThresh".as_slice(), DEFAULT_AUTO_KEY_RATE_TICKS),
        ] {
            let mut loaded =
                load_pef_application(&synthetic_pef_with_library_import(library, symbol)).unwrap();
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], u32::from(expected));
        }
    }
}

#[test]
fn keyboard_timing_getters_read_guest_changes_with_signed_short_results() {
    for library in [b"InterfaceLib".as_slice(), b"CarbonLib".as_slice()] {
        for (symbol, address, target) in [
            (
                b"LMGetKeyThresh".as_slice(),
                addr::KEY_THRESH,
                PpcImportDispatcherTarget::LMGetKeyThresh,
            ),
            (
                b"LMGetKeyRepThresh".as_slice(),
                addr::KEY_REP_THRESH,
                PpcImportDispatcherTarget::LMGetKeyRepThresh,
            ),
        ] {
            let mut loaded =
                load_pef_application(&synthetic_pef_with_library_import(library, symbol)).unwrap();
            for (word, expected) in [
                (0, 0),
                (37, 37),
                (0x7fff, 0x7fff),
                (0x8000, 0xffff_8000),
                (0xffff, u32::MAX),
            ] {
                loaded.memory.write_u16_be(address, word).unwrap();
                run_test_import(&mut loaded, target.clone());
                assert_eq!(loaded.cpu.gpr[3], expected);
            }
        }
    }
}

#[test]
fn keyboard_timing_setters_write_only_the_selected_shared_word() {
    for library in [b"InterfaceLib".as_slice(), b"CarbonLib".as_slice()] {
        for (symbol, address, other, target) in [
            (
                b"LMSetKeyThresh".as_slice(),
                addr::KEY_THRESH,
                addr::KEY_REP_THRESH,
                PpcImportDispatcherTarget::LMSetKeyThresh,
            ),
            (
                b"LMSetKeyRepThresh".as_slice(),
                addr::KEY_REP_THRESH,
                addr::KEY_THRESH,
                PpcImportDispatcherTarget::LMSetKeyRepThresh,
            ),
        ] {
            let mut loaded =
                load_pef_application(&synthetic_pef_with_library_import(library, symbol)).unwrap();
            loaded
                .memory
                .write_u16_be(addr::KEY_THRESH - 2, 0x5a5a)
                .unwrap();
            loaded
                .memory
                .write_u16_be(addr::KEY_REP_THRESH + 2, 0xa5a5)
                .unwrap();
            loaded.memory.write_u16_be(other, 0x6b6b).unwrap();
            loaded.cpu.gpr[3] = 0xdead_8001;
            run_test_import(&mut loaded, target.clone());
            assert_eq!(loaded.memory.read_u16_be(address), Some(0x8001));
            assert_eq!(loaded.memory.read_u16_be(other), Some(0x6b6b));
            assert_eq!(
                loaded.memory.read_u16_be(addr::KEY_THRESH - 2),
                Some(0x5a5a)
            );
            assert_eq!(
                loaded.memory.read_u16_be(addr::KEY_REP_THRESH + 2),
                Some(0xa5a5)
            );
        }
    }
}

#[test]
fn keyboard_accessors_share_preferences_with_the_classic_memory_adapter() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"LMGetKeyThresh")).unwrap();
    let mut classic_bus = MacMemoryBus::new(0x1000);
    let low_memory = classic_bus.shared_ram_region(0, 0x1000).unwrap();
    let mut context = ProcessContext::default();
    context.attach_memory(0, low_memory, &mut loaded.memory);

    classic_bus.write_word(addr::KEY_THRESH, 43);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetKeyThresh);
    assert_eq!(loaded.cpu.gpr[3], 43);
    classic_bus.write_word(addr::KEY_REP_THRESH, 0xfffe);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMGetKeyRepThresh);
    assert_eq!(loaded.cpu.gpr[3], 0xffff_fffe);

    loaded.cpu.gpr[3] = 51;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetKeyThresh);
    assert_eq!(classic_bus.read_word(addr::KEY_THRESH), 51);
    loaded.cpu.gpr[3] = 9;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetKeyRepThresh);
    assert_eq!(classic_bus.read_word(addr::KEY_REP_THRESH), 9);
}
