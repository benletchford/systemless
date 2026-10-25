use super::*;

fn compat(operation: PpcToolboxCompatOperation, loaded: &mut PpcLoadedApp) -> PpcImportAction {
    dispatch_toolbox_compat::ppc_dispatch_toolbox_compat(operation, &loaded.cpu, &mut loaded.memory)
}

#[test]
fn carbon_port_setters_write_the_fields_their_getters_read() {
    for (symbol, operation) in [
        ("SetPortBounds", PpcToolboxCompatOperation::SetPortBounds),
        (
            "SetPortGrafProcs",
            PpcToolboxCompatOperation::SetPortGrafProcs,
        ),
        ("SetPortPenMode", PpcToolboxCompatOperation::SetPortPenMode),
        ("IsPortColor", PpcToolboxCompatOperation::IsPortColor),
    ] {
        assert_eq!(
            dispatcher_target_for_import("CarbonLib", symbol),
            PpcImportDispatcherTarget::ToolboxCompat(operation)
        );
    }
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"SetPortBounds")).unwrap();
    let port = PPC_DATA_BASE + 0x1000;
    let rect = port + 0x100;
    loaded.memory.add_region(port, vec![0; 0x200]);
    ppc_write_rect(&mut loaded.memory, rect, 1, 2, 300, 400).unwrap();
    loaded.cpu.gpr[3] = port;
    loaded.cpu.gpr[4] = rect;
    compat(PpcToolboxCompatOperation::SetPortBounds, &mut loaded);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, port + 16),
        Some((1, 2, 300, 400))
    );
    loaded.cpu.gpr[4] = 0x0123_4560;
    compat(PpcToolboxCompatOperation::SetPortGrafProcs, &mut loaded);
    assert!(matches!(
        compat(PpcToolboxCompatOperation::GetPortGrafProcs, &mut loaded),
        PpcImportAction::Return(0x0123_4560)
    ));
    loaded.cpu.gpr[4] = 10;
    compat(PpcToolboxCompatOperation::SetPortPenMode, &mut loaded);
    assert!(matches!(
        compat(PpcToolboxCompatOperation::GetPortPenMode, &mut loaded),
        PpcImportAction::Return(10)
    ));

    assert!(matches!(
        compat(PpcToolboxCompatOperation::IsPortColor, &mut loaded),
        PpcImportAction::Return(0)
    ));
    loaded.memory.write_u16_be(port + 6, 0xc000).unwrap();
    assert!(matches!(
        compat(PpcToolboxCompatOperation::IsPortColor, &mut loaded),
        PpcImportAction::Return(1)
    ));
}

#[test]
fn carbon_set_port_clip_region_copies_into_the_ports_clip_region() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "SetPortClipRegion"),
        PpcImportDispatcherTarget::SetPortClipRegion
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewRgn")).unwrap();
    let port = *loaded.current_gworld;
    let rect = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(rect, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, rect, 5, 6, 70, 80).unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRgn);
    let region = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = region;
    loaded.cpu.gpr[4] = rect;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RectRgn);
    loaded.cpu.gpr[3] = port;
    loaded.cpu.gpr[4] = region;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetPortClipRegion);
    let clip = loaded.memory.read_u32_be(port + 28).unwrap();
    assert_ne!(clip, region);
    let master = loaded.memory.read_u32_be(clip).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, master + 2),
        Some((5, 6, 70, 80))
    );
}

#[test]
fn carbon_window_content_color_round_trips_and_defaults_to_white() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "GetWindowContentColor"),
        PpcImportDispatcherTarget::WindowContentColor { set: false }
    );
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"GetWindowContentColor")).unwrap();
    let (window, color) = (PPC_DATA_BASE + 0x1000, PPC_DATA_BASE + 0x1100);
    loaded.memory.add_region(window, vec![0; 0x200]);
    let get = |loaded: &mut PpcLoadedApp| {
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = color;
        run_test_import(
            loaded,
            PpcImportDispatcherTarget::WindowContentColor { set: false },
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        [0, 2, 4].map(|offset| loaded.memory.read_u16_be(color + offset).unwrap())
    };
    assert_eq!(get(&mut loaded), [0xffff; 3]);
    for (index, component) in [0x1111, 0x2222, 0x3333].into_iter().enumerate() {
        loaded
            .memory
            .write_u16_be(color + 2 * index as u32, component)
            .unwrap();
    }
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = color;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::WindowContentColor { set: true },
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_bytes(color, &[0; 6]).unwrap();
    assert_eq!(get(&mut loaded), [0x1111, 0x2222, 0x3333]);
}

#[test]
fn carbon_create_new_port_opens_a_current_color_port() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CreateNewPort"),
        PpcImportDispatcherTarget::CreateNewPort
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CreateNewPort")).unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CreateNewPort);
    let port = loaded.cpu.gpr[3];
    assert_ne!(port, 0);
    assert_eq!(*loaded.current_gworld, port);
    loaded.cpu.gpr[3] = port;
    assert!(matches!(
        compat(PpcToolboxCompatOperation::IsPortColor, &mut loaded),
        PpcImportAction::Return(1)
    ));
}

#[test]
fn carbon_menu_bar_hides_and_restores_its_height() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"HideMenuBar")).unwrap();
    loaded
        .memory
        .write_u16_be(PPC_MBAR_HEIGHT_ADDR, 20)
        .unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HideMenuBar);
    assert_eq!(loaded.memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR), Some(0));
    assert!(loaded.toolbox_startup.host_menu_bar_hidden);
    // Hiding twice keeps the original height.
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HideMenuBar);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ShowMenuBar);
    assert_eq!(loaded.memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR), Some(20));
    assert!(!loaded.toolbox_startup.host_menu_bar_hidden);
}

#[test]
fn carbon_low_memory_appl_zone_and_qd_gray_globals() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "LMSetApplZone"),
        PpcImportDispatcherTarget::SetApplZone
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"LMSetApplZone")).unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetApplZone);
    let original = loaded.cpu.gpr[3];
    assert_eq!(original, PPC_APPLICATION_ZONE);
    loaded.cpu.gpr[3] = PPC_SYSTEM_ZONE;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplZone);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetApplZone);
    assert_eq!(loaded.cpu.gpr[3], PPC_SYSTEM_ZONE);

    let pattern = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(pattern, vec![0; 8]);
    loaded.cpu.gpr[3] = pattern;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetQDGlobalsGray);
    assert_eq!(loaded.cpu.gpr[3], pattern);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, pattern, 8),
        Some(vec![0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55])
    );
}

#[test]
fn carbon_single_precision_classify_and_modff() {
    let target = |operation| PpcImportDispatcherTarget::MathCompatibility(operation);
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "__fpclassifyf"),
        target(PpcMathCompatibilityOperation::FpClassifyF)
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "modff"),
        target(PpcMathCompatibilityOperation::Modff)
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"modff")).unwrap();
    // 1e-40 is a normal double but a subnormal float.
    for (value, class) in [(1e-40, 5), (1.5, 4), (0.0, 3), (f64::INFINITY, 2)] {
        loaded.cpu.fpr[1] = f64::to_bits(value);
        run_test_import(
            &mut loaded,
            target(PpcMathCompatibilityOperation::FpClassifyF),
        );
        assert_eq!(loaded.cpu.gpr[3], class, "{value}");
    }
    let out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(out, vec![0; 4]);
    loaded.cpu.fpr[1] = (-2.75_f64).to_bits();
    loaded.cpu.gpr[4] = out;
    run_test_import(&mut loaded, target(PpcMathCompatibilityOperation::Modff));
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), -0.75);
    assert_eq!(loaded.memory.read_u32_be(out), Some((-2.0_f32).to_bits()));
}

#[test]
fn appearance_theme_background_frames_and_drawing_state() {
    for (symbol, target) in [
        (
            "ApplyThemeBackground",
            PpcImportDispatcherTarget::ApplyThemeBackground,
        ),
        (
            "DrawThemeEditTextFrame",
            PpcImportDispatcherTarget::DrawThemeEditTextFrame,
        ),
        (
            "DrawThemeFocusRect",
            PpcImportDispatcherTarget::DrawThemeFocusRect,
        ),
        (
            "GetThemeDrawingState",
            PpcImportDispatcherTarget::GetThemeDrawingState,
        ),
        (
            "GetControlPopupMenuID",
            PpcImportDispatcherTarget::LegacyControl(
                PpcLegacyControlOperation::GetControlPopupMenuId,
            ),
        ),
    ] {
        assert_eq!(dispatcher_target_for_import("CarbonLib", symbol), target);
    }
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"ApplyThemeBackground")).unwrap();
    let port = *loaded.current_gworld;
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 0x40]);
    ppc_write_rect(&mut loaded.memory, scratch, 10, 10, 30, 60).unwrap();

    // An unsupported depth is rejected before the background changes.
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 3;
    loaded.cpu.gpr[7] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ApplyThemeBackground);
    assert_eq!(loaded.cpu.gpr[3] as i16, -30567);
    // A monochrome device uses a white background.
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ApplyThemeBackground);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        [42, 44, 46].map(|offset| loaded.memory.read_u16_be(port + offset).unwrap()),
        [0xffff; 3]
    );

    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::DrawThemeEditTextFrame,
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawThemeFocusRect);
    assert_eq!(loaded.cpu.gpr[3], 0);

    let out = scratch + 0x20;
    loaded.cpu.gpr[3] = out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetThemeDrawingState);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let state = loaded.memory.read_u32_be(out).unwrap();
    assert_ne!(state, 0);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, state, 4),
        Some(b"tdst".to_vec())
    );
    assert_eq!(
        loaded.memory.read_u16_be(state + 4 + 6),
        loaded.memory.read_u16_be(port + 42)
    );
}

#[test]
fn list_manager_appends_to_cells_and_draws_one_cell() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "LAddToCell"),
        PpcImportDispatcherTarget::LAddToCell
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "LDraw"),
        PpcImportDispatcherTarget::LDraw
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"LNew")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let (view_ptr, bounds_ptr, text_ptr) = (scratch, scratch + 8, scratch + 20);
    loaded.memory.add_region(scratch, vec![0; 64]);
    ppc_write_rect(&mut loaded.memory, view_ptr, 10, 20, 90, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
    loaded.memory.write_bytes(text_ptr, b"Pilot").unwrap();
    loaded.cpu.gpr[3] = view_ptr;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = (20u32 << 16) | 100;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = PPC_MAIN_GWORLD;
    for register in 8..=10 {
        loaded.cpu.gpr[register] = 0;
    }
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LNew);
    let list = loaded.cpu.gpr[3];
    assert_ne!(list, 0);
    let add = |loaded: &mut PpcLoadedApp, length: u32, cell: u32| {
        loaded.cpu.gpr[3] = text_ptr;
        loaded.cpu.gpr[4] = length;
        loaded.cpu.gpr[5] = cell;
        loaded.cpu.gpr[6] = list;
        run_test_import(loaded, PpcImportDispatcherTarget::LAddToCell);
    };
    add(&mut loaded, 3, (1 << 16) | 1);
    add(&mut loaded, 5, (1 << 16) | 1);
    // A cell outside the list's data bounds is left alone.
    add(&mut loaded, 5, (5 << 16) | 5);
    let record = loaded.list_manager.get_record(list).unwrap();
    assert_eq!(record.cells.get(&(1, 1)).unwrap(), b"PilPilot");
    assert!(!record.cells.contains_key(&(5, 5)));

    loaded.cpu.gpr[3] = (1 << 16) | 1;
    loaded.cpu.gpr[4] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LDraw);
}
