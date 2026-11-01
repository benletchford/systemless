//! Lowercase C-string Toolbox glue, PPostEvent, Open Transport `errno`, and
//! writable code for self-modifying applications.
use super::*;
use crate::loader::ppc::pef_loader::ppc_imports_declare_code_writes;

#[test]
fn c_string_glue_and_ppostevent_bind_to_dispatchers() {
    for (symbol, target) in [
        ("PPostEvent", PpcImportDispatcherTarget::PPostEvent),
        ("equalstring", PpcImportDispatcherTarget::EqualCString),
        ("upperstring", PpcImportDispatcherTarget::UpperCString),
        ("drawstring", PpcImportDispatcherTarget::DrawCString),
        ("openresfile", PpcImportDispatcherTarget::OpenResFileCString),
    ] {
        assert_eq!(dispatcher_target_for_import("InterfaceLib", symbol), target);
    }
}

#[test]
fn equalstring_compares_c_strings_with_equalstring_rules() {
    let pef = synthetic_pef_with_import(b"equalstring");
    let mut loaded = load_pef_application(&pef).unwrap();
    let a = PPC_DATA_BASE + 0x1000;
    let b = a + 0x100;
    loaded.memory.add_region(a, vec![0; 0x200]);
    loaded.memory.write_bytes(a, b"Duke\0").unwrap();
    loaded.memory.write_bytes(b, b"DUKE\0").unwrap();
    loaded.cpu.gpr[3] = a;
    loaded.cpu.gpr[4] = b;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1, "case-insensitive match");

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = a;
    loaded.cpu.gpr[4] = b;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 0;
    let _ = loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0, "case-sensitive mismatch");
}

#[test]
fn upperstring_uppercases_a_c_string_in_place() {
    let pef = synthetic_pef_with_import(b"upperstring");
    let mut loaded = load_pef_application(&pef).unwrap();
    let s = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(s, vec![0; 0x40]);
    loaded.memory.write_bytes(s, b"e1l1.map\0x").unwrap();
    loaded.cpu.gpr[3] = s;
    loaded.cpu.gpr[4] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    let mut out = [0u8; 10];
    loaded.memory.read_bytes_into(s, &mut out).unwrap();
    assert_eq!(&out, b"E1L1.MAP\0x");
}

#[test]
fn ppostevent_posts_like_postevent_and_clears_the_element_pointer() {
    let pef = synthetic_pef_with_import(b"PPostEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let qel = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(qel, vec![0xff; 4]);
    loaded.cpu.gpr[3] = 3; // keyDown
    loaded.cpu.gpr[4] = 0x0000_2061;
    loaded.cpu.gpr[5] = qel;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(qel), Some(0));
}

#[test]
fn open_transport_errno_has_its_own_writable_storage() {
    let address = import_data_address_for("OTXTILib", "errno").unwrap();
    assert_ne!(Some(address), import_data_address_for("StdCLib", "errno"));
}

#[test]
fn code_cache_flush_imports_declare_self_modifying_code() {
    let import = |library: &str, symbol: &str| crate::loader::pef::PefResolvedImport {
        library_index: 0,
        symbol_index: 0,
        library_name: library.to_string(),
        symbol_name: symbol.to_string(),
        class: 2,
        weak: false,
    };
    assert!(!ppc_imports_declare_code_writes(&[import(
        "InterfaceLib",
        "DrawString"
    )]));
    for symbol in [
        "MakeDataExecutable",
        "FlushCodeCache",
        "FlushCodeCacheRange",
        "FlushInstructionCache",
    ] {
        assert!(ppc_imports_declare_code_writes(&[import(
            "InterfaceLib",
            symbol
        )]));
    }
}

#[test]
fn self_modifying_application_code_is_writable_and_others_stay_read_only() {
    let plain = load_pef_application(&synthetic_pef_with_import(b"DrawString")).unwrap();
    let flushing =
        load_pef_application(&synthetic_pef_with_import(b"MakeDataExecutable")).unwrap();
    for (mut loaded, writable) in [(plain, false), (flushing, true)] {
        let pc = loaded.entry_pc;
        let word = loaded.memory.read_u32_be(pc).unwrap();
        assert_eq!(loaded.memory.write_u32_be(pc, word).is_some(), writable);
    }
}
