use super::*;

#[test]
fn global_mouse_reads_changing_input_without_current_port_transform() {
    for library in [b"CarbonLib".as_slice(), b"InterfaceLib"] {
        let pef = synthetic_pef_with_library_import(library, b"GetGlobalMouse");
        let mut loaded = load_pef_application(&pef).unwrap();
        let point = PPC_DATA_BASE + 0x1120;
        loaded.memory.add_region(point, vec![0xaa; 8]);
        ppc_write_rect(&mut loaded.memory, PPC_MAIN_PIXMAP + 6, -60, -80, 540, 720).unwrap();
        for (v, h) in [(360, 580), (-123, -456), (i16::MIN, i16::MAX)] {
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_v: v,
                mouse_h: h,
                ..Default::default()
            });
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = point;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], point, "void API preserves r3");
            assert_eq!(loaded.memory.read_u16_be(point), Some(v as u16));
            assert_eq!(loaded.memory.read_u16_be(point + 2), Some(h as u16));
            assert_eq!(loaded.memory.read_u32_be(point + 4), Some(0xaaaaaaaa));
        }
    }
}

#[test]
fn global_mouse_weak_binding_and_destination_bounds_are_preserved() {
    let bindings = PpcImportBindingPlan::prepare(
        vec![PefResolvedImport {
            library_index: 0,
            symbol_index: 0,
            library_name: "CarbonLib".into(),
            symbol_name: "GetGlobalMouse".into(),
            class: 2,
            weak: true,
        }],
        1,
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .unwrap()
    .into_initial_bindings();
    assert_eq!(
        bindings[0].dispatcher_target,
        PpcImportDispatcherTarget::GetGlobalMouse
    );
    assert_ne!(bindings[0].address, 0);
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"GetGlobalMouse");
    let mut loaded = load_pef_application(&pef).unwrap();
    let short = 0x60000;
    loaded.memory.add_region(short, vec![0xaa; 2]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 123,
        mouse_h: 456,
        ..Default::default()
    });
    for point in [0, short, u32::MAX] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = point;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], point);
        assert_eq!(
            loaded.memory.read_u16_be(short),
            Some(0xaaaa),
            "no partial Point write"
        );
    }
}
