use super::*;

#[test]
fn select_window_preserves_classic_visible_region() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"SelectWindow")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1400;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    let window = create_test_cwindow(&mut loaded, scratch, (40, 50, 140, 250), 0, false, u32::MAX);
    loaded
        .memory
        .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 0xFF)
        .unwrap();

    loaded.cpu.gpr[3] = window;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SelectWindow);

    assert!(ppc_window_is_visible(&mut loaded.memory, window));
    let vis_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CGRAF_PORT_VIS_RGN_OFFSET)
        .unwrap();
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, vis_rgn),
        Some((0, 0, 100, 200))
    );
}

pub(crate) fn create_test_cwindow(
    loaded: &mut PpcLoadedApp,
    bounds_ptr: u32,
    bounds: (i16, i16, i16, i16),
    proc_id: i16,
    visible: bool,
    behind: u32,
) -> u32 {
    ppc_write_rect(&mut loaded.memory, bounds_ptr, bounds.0, bounds.1, bounds.2, bounds.3)
        .unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = u32::from(visible);
    loaded.cpu.gpr[7] = proc_id as u16 as u32;
    loaded.cpu.gpr[8] = behind;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0;
    run_test_import(loaded, PpcImportDispatcherTarget::NewCWindow);
    let window = loaded.cpu.gpr[3];
    assert_ne!(window, 0, "test NewCWindow must succeed");
    window
}

#[test]
fn window_visibility_recalculation_keeps_set_origin_coordinates() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"SizeWindow")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1400;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    let window = create_test_cwindow(&mut loaded, scratch, (40, 50, 140, 250), 0, true, u32::MAX);
    ppc_set_port_origin(&mut loaded.memory, window, 0, -360).unwrap();
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = 200;
    loaded.cpu.gpr[5] = 100;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SizeWindow);
    let vis_rgn = loaded.memory.read_u32_be(window + PPC_CGRAF_PORT_VIS_RGN_OFFSET).unwrap();
    assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, vis_rgn), Some((-360, 0, -260, 200)));
}

#[test]
fn carbon_get_port_bounds_copies_window_local_rect() {
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"GetPortBounds");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(loaded.imports[0].dispatcher_target, PpcImportDispatcherTarget::GetPortBounds);
    let scratch = PPC_DATA_BASE + 0x1400;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    let window = create_test_cwindow(&mut loaded, scratch, (40, 50, 140, 250), 0, true, u32::MAX);
    let out_rect = scratch + 16;
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = out_rect;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetPortBounds);

    assert_eq!(loaded.cpu.gpr[3], out_rect);
    assert_eq!(ppc_read_rect(&mut loaded.memory, out_rect), Some((0, 0, 100, 200)));

    ppc_write_rect(&mut loaded.memory, out_rect, 1, 2, 3, 4).unwrap();
    loaded.cpu.gpr[3] = scratch + 64; // mapped memory, but not a graphics port
    loaded.cpu.gpr[4] = out_rect;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetPortBounds);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(ppc_read_rect(&mut loaded.memory, out_rect), Some((1, 2, 3, 4)));
}

pub(crate) fn test_wind_resource(
    bounds: (i16, i16, i16, i16),
    proc_id: i16,
    visible: bool,
    go_away: bool,
    ref_con: u32,
    title: &[u8],
) -> Vec<u8> {
    let mut wind = Vec::with_capacity(19 + title.len());
    for coordinate in [bounds.0, bounds.1, bounds.2, bounds.3] {
        wind.extend_from_slice(&coordinate.to_be_bytes());
    }
    wind.extend_from_slice(&proc_id.to_be_bytes());
    wind.extend_from_slice(&u16::from(visible).to_be_bytes());
    wind.extend_from_slice(&u16::from(go_away).to_be_bytes());
    wind.extend_from_slice(&ref_con.to_be_bytes());
    wind.push(u8::try_from(title.len()).unwrap());
    wind.extend_from_slice(title);
    wind
}

#[test]
fn lm_set_cur_activate_updates_pending_activation_global() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetCurActivate"),
        PpcImportDispatcherTarget::LMSetCurActivate
    );
    let pef = synthetic_pef_with_import(b"LMSetCurActivate");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0x0123_4568;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetCurActivate);

    assert_eq!(
        loaded.memory.read_u32_be(crate::memory::globals::addr::CUR_ACTIVATE),
        Some(0x0123_4568)
    );
    assert_eq!(loaded.cpu.gpr[3], 0x0123_4568);

    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LMSetCurActivate);
    assert_eq!(
        loaded.memory.read_u32_be(crate::memory::globals::addr::CUR_ACTIVATE),
        Some(0)
    );
}

#[test]
fn front_window_transition_coalesces_pending_activation_pair() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0, vec![0; 0x1000]);
    let mut queue = VecDeque::new();

    ppc_enqueue_window_activation_transition(
        &mut memory,
        &mut queue,
        Some(0x1000),
        Some(0x2000),
        41,
    );
    ppc_enqueue_window_activation_transition(
        &mut memory,
        &mut queue,
        Some(0x2000),
        Some(0x3000),
        42,
    );

    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].what, 8);
    assert_eq!(queue[0].message, 0x2000);
    assert_eq!(queue[0].when, 42);
    assert_eq!(queue[0].modifiers & 1, 0);
    assert_eq!(queue[1].what, 8);
    assert_eq!(queue[1].message, 0x3000);
    assert_eq!(queue[1].when, 42);
    assert_eq!(queue[1].modifiers & 1, 1);
    assert_eq!(memory.read_u32_be(0x0A68), Some(0x2000));
    assert_eq!(memory.read_u32_be(0x0A64), Some(0x3000));
}

#[test]
fn find_window_distinguishes_the_menu_bar_from_the_desktop() {
    let pef = synthetic_pef_with_import(b"FindWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window_out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(window_out, vec![0xff; 4]);
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded
        .memory
        .write_u16_be(PPC_MBAR_HEIGHT_ADDR, 12)
        .unwrap();
    loaded.cpu.gpr[3] = (11 << 16) | 72;
    loaded.cpu.gpr[4] = window_out;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(window_out), Some(0));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = (12 << 16) | 72;
    loaded.cpu.gpr[4] = window_out;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(window_out), Some(0));
}

#[test]
fn find_window_routes_active_draw_sprocket_display_clicks_to_content() {
    let pef = synthetic_pef_with_import(b"FindWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window_out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(window_out, vec![0xff; 4]);
    loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
    loaded.cpu.gpr[3] = (120 << 16) | 220;
    loaded.cpu.gpr[4] = window_out;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::FindWindow);

    assert_eq!(loaded.cpu.gpr[3], 3);
    assert_eq!(loaded.memory.read_u32_be(window_out), Some(0));

    loaded.draw_sprocket.active_context = None;
    loaded.cpu.gpr[3] = (120 << 16) | 220;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FindWindow);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn find_window_uses_global_content_region_after_window_pixmap_rebind() {
    let pef = synthetic_pef_with_import(b"FindWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(scratch, vec![0; 32]);
    let window = create_test_cwindow(
        &mut loaded,
        scratch,
        (178, 253, 421, 547),
        1,
        true,
        u32::MAX,
    );
    let pixmap_handle = loaded.memory.read_u32_be(window + 2).unwrap();
    let pixmap = loaded.memory.read_u32_be(pixmap_handle).unwrap();
    ppc_write_rect(&mut loaded.memory, pixmap + 6, 0, 0, 600, 800).unwrap();
    let mut size_cpu = loaded.cpu.clone();
    size_cpu.gpr[3] = window;
    size_cpu.gpr[4] = 294;
    size_cpu.gpr[5] = 243;
    assert_eq!(
        ppc_size_window(&size_cpu, &mut loaded.memory, &mut loaded.gworlds),
        Some(())
    );
    loaded.draw_sprocket.active_context = Some(PPC_DSP_CONTEXT);
    loaded.cpu.gpr[3] = (307 << 16) | 412;
    loaded.cpu.gpr[4] = scratch + 16;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::FindWindow);

    assert_eq!(loaded.cpu.gpr[3], 3);
    assert_eq!(loaded.memory.read_u32_be(scratch + 16), Some(window));
}

#[test]
fn find_window_uses_visible_front_to_back_window_geometry_not_the_current_port() {
    let pef = synthetic_pef_with_import(b"FindWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1100;
    let bounds = scratch;
    let window_out = scratch + 16;
    loaded.memory.add_region(scratch, vec![0; 32]);

    ppc_write_rect(&mut loaded.memory, bounds, 40, 40, 140, 180).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    let back_window = loaded.cpu.gpr[3];
    assert_ne!(back_window, 0);

    ppc_write_rect(&mut loaded.memory, bounds, 60, 60, 160, 200).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    let front_window = loaded.cpu.gpr[3];
    assert_ne!(front_window, 0);

    // The current graphics port is not the Window Manager's z-order.
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded.cpu.gpr[3] = (80 << 16) | 80;
    loaded.cpu.gpr[4] = window_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FindWindow);
    assert_eq!(loaded.cpu.gpr[3], 3);
    assert_eq!(loaded.memory.read_u32_be(window_out), Some(front_window));

    loaded.cpu.gpr[3] = (50 << 16) | 50;
    loaded.cpu.gpr[4] = window_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FindWindow);
    assert_eq!(loaded.cpu.gpr[3], 3);
    assert_eq!(loaded.memory.read_u32_be(window_out), Some(back_window));

    loaded.cpu.gpr[3] = (300 << 16) | 300;
    loaded.cpu.gpr[4] = window_out;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FindWindow);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(window_out), Some(0));
}

#[test]
fn dispose_dialog_removes_it_from_the_window_list_and_restores_the_previous_window() {
    let pef = synthetic_pef_with_import(b"DisposeDialog");
    let mut loaded = load_pef_application(&pef).unwrap();
    let application_window = PPC_DATA_BASE + 0x1000;
    let dialog = PPC_DATA_BASE + 0x2000;

    for window in [application_window, dialog] {
        loaded
            .memory
            .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
        loaded
            .memory
            .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
            .unwrap();
        loaded.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: window,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: 0,
            gdevice: PPC_MAIN_GDEVICE,
            width: 100,
            height: 100,
            depth: 8,
            row_bytes: 100,
            pixels_locked: false,
            pixels_no_purge: false,
        });
    }
    loaded.window_list.extend([dialog, application_window]);
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = dialog);
    loaded.cpu.gpr[3] = dialog;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::DisposeDialog);

    assert_eq!(loaded.toolbox_startup.dispose_dialog_count, 1);
    assert_eq!(loaded.toolbox_startup.last_disposed_dialog, dialog);
    assert!(!loaded.gworlds.iter().any(|record| record.port == dialog));
    assert_eq!(*loaded.current_gworld, application_window);
    assert_eq!(
        ppc_front_visible_window(&mut loaded.memory, &loaded.gworlds),
        Some(application_window)
    );
}

#[test]
fn hle_import_runner_creates_window_title_and_zoom_state() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewWindow")).unwrap();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        64,
        true,
    );
    ppc_write_rect(&mut loaded.memory, scratch, 40, 50, 240, 350).unwrap();
    write_ppc_pstring(&mut loaded.memory, scratch + 8, b"Document");
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0x4455_6677;

    let probe = loaded.run_with_hle_imports(128);

    assert_eq!(probe.unsupported_import_index, None);
    let window = loaded.cpu.gpr[3];
    assert_ne!(window, 0);
    let title_handle = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_TITLE_HANDLE_OFFSET)
        .unwrap();
    let title = loaded.memory.read_u32_be(title_handle).unwrap();
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, title),
        Some(b"Document".to_vec())
    );
    let state_handle = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_STATE_HANDLE_OFFSET)
        .unwrap();
    let state = loaded.memory.read_u32_be(state_handle).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, state),
        Some((40, 50, 240, 350))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, state + 8),
        Some((
            20,
            0,
            ppc_main_screen_height() as i16,
            ppc_main_screen_width() as i16,
        ))
    );

    let mut zoom_cpu = loaded.cpu.clone();
    zoom_cpu.gpr[3] = window;
    zoom_cpu.gpr[4] = 8;
    let _ = ppc_zoom_window(&zoom_cpu, &mut loaded.memory, &mut loaded.gworlds);
    assert_eq!(
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, window),
        Some((
            20,
            0,
            ppc_main_screen_height() as i16,
            ppc_main_screen_width() as i16,
        ))
    );
}

#[test]
fn carbon_create_new_window_returns_hidden_document_window() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "CreateNewWindow"),
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CreateNewWindow)
    );

    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewWindow")).unwrap();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    ppc_write_rect(&mut loaded.memory, scratch, 40, 50, 240, 350).unwrap();
    loaded.cpu.gpr[3] = 6; // kDocumentWindowClass
    loaded.cpu.gpr[4] = 8; // kWindowCollapseBoxAttribute
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = scratch + 8;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CreateNewWindow),
    );

    assert_eq!(loaded.cpu.gpr[3], 0);
    let window = loaded.memory.read_u32_be(scratch + 8).unwrap();
    assert_ne!(window, 0);
    assert_eq!(loaded.memory.read_u8(window + PPC_CWINDOW_VISIBLE_OFFSET), Some(0));
    assert_eq!(loaded.memory.read_u8(window + PPC_CWINDOW_GO_AWAY_OFFSET), Some(0));
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, window + 16),
        Some((0, 0, 200, 300))
    );

    let window_count = loaded.gworlds.len();
    loaded.cpu.gpr[3] = 6;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CreateNewWindow),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.gworlds.len(), window_count);
}

#[test]
fn carbon_reposition_window_centers_hidden_window_on_main_screen() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "RepositionWindow"),
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RepositionWindow)
    );
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewWindow")).unwrap();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    let window = create_test_cwindow(
        &mut loaded,
        scratch,
        (40, 50, 240, 350),
        0,
        false,
        u32::MAX,
    );
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 1; // kWindowCenterOnMainScreen
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RepositionWindow),
    );

    assert_eq!(loaded.cpu.gpr[3], 0);
    let structure =
        ppc_window_global_structure_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
    let menu_height = i32::from(loaded.memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap());
    assert!(
        (i32::from(structure.1) + i32::from(structure.3) - ppc_main_screen_width() as i32).abs()
            <= 1
    );
    assert!(
        (i32::from(structure.0) + i32::from(structure.2)
            - menu_height
            - ppc_main_screen_height() as i32)
            .abs()
            <= 1
    );
    assert_eq!(
        loaded.memory.read_u8(window + PPC_CWINDOW_VISIBLE_OFFSET),
        Some(0)
    );

    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[5] = u32::MAX;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RepositionWindow),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(
        ppc_window_global_structure_bounds(&mut loaded.memory, &loaded.gworlds, window),
        Some(structure)
    );
}

#[test]
fn window_resource_parameters_preserve_compiled_bounds_and_title() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut wind = Vec::new();
    wind.extend_from_slice(&10i16.to_be_bytes());
    wind.extend_from_slice(&20i16.to_be_bytes());
    wind.extend_from_slice(&210i16.to_be_bytes());
    wind.extend_from_slice(&420i16.to_be_bytes());
    wind.extend_from_slice(&0u16.to_be_bytes());
    wind.extend_from_slice(&1u16.to_be_bytes());
    wind.extend_from_slice(&1u16.to_be_bytes());
    wind.extend_from_slice(&0x1234_5678u32.to_be_bytes());
    wind.push(4);
    wind.extend_from_slice(b"Game");

    let (bounds, title) = ppc_materialize_window_resource_parameters(
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &wind,
    )
    .unwrap();

    assert_eq!(
        ppc_read_rect(&mut loaded.memory, bounds),
        Some((10, 20, 210, 420))
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, title),
        Some(b"Game".to_vec())
    );
}

#[test]
fn front_window_uses_the_process_order_without_native_gworld_metadata() {
    let pef = synthetic_pef_with_import(b"FrontWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let back = PPC_DATA_BASE + 0x1000;
    let front = back + 0x200;
    loaded.memory.add_region(back, vec![0; 0x400]);
    loaded
        .memory
        .write_u8(back + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u8(front + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded.window_list.extend([front, back]);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], front);
    loaded.window_list.swap(0, 1);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], back);
}

#[test]
fn close_window_removes_process_membership_without_native_gworld_metadata() {
    let pef = synthetic_pef_with_import(b"CloseWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(window, vec![0; 0x200]);
    loaded
        .memory
        .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded.window_list.push(window);
    loaded.cpu.gpr[3] = window;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert!(loaded.window_list.is_empty());
    assert_eq!(loaded.memory.read_u32_be(0x09D6), Some(0));
}


#[test]
fn hle_import_runner_handles_new_cwindow_allocation() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let bounds_ptr = scratch;
    loaded.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 20, 10, 260, 330).unwrap();
    ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Document");
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0x1234_5678;
    let gworld_count = loaded.gworlds.len();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let window = loaded.cpu.gpr[3];
    assert_ne!(window, 0);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(*loaded.current_gworld, window);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
    assert_eq!(loaded.gworlds.len(), gworld_count + 1);
    let record = loaded
        .gworlds
        .iter()
        .find(|record| record.port == window)
        .unwrap();
    assert_eq!(record.width, 320);
    assert_eq!(record.height, 240);
    assert_eq!(record.depth, PPC_MAIN_PIXEL_DEPTH);
    assert_eq!(record.base_addr, PPC_MAIN_SCREEN_BASE);
    assert_eq!(record.row_bytes, ppc_main_screen_row_bytes());
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_PIXMAP + 42),
        Some(PPC_MAIN_CTABLE_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(record.pixmap + 42),
        Some(PPC_MAIN_CTABLE_HANDLE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_MAIN_CTABLE_HANDLE),
        Some(PPC_MAIN_CTABLE)
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_CTABLE + 4), Some(0x8000));
    assert_eq!(loaded.memory.read_u16_be(PPC_MAIN_CTABLE + 6), Some(255));
    assert_eq!(
        loaded.memory.read_u32_be(window + 2),
        Some(record.pixmap_handle)
    );
    assert_eq!(loaded.memory.read_u16_be(window + 6), Some(0xc000));
    assert_eq!(
        loaded
            .memory
            .read_u16_be(window + PPC_CWINDOW_WINDOW_KIND_OFFSET),
        Some(8)
    );
    assert_eq!(
        loaded.memory.read_u8(window + PPC_CWINDOW_VISIBLE_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded.memory.read_u8(window + PPC_CWINDOW_GO_AWAY_OFFSET),
        Some(1)
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, record.pixmap + 6),
        Some((-20, -10, 580, 790))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, window + 16),
        Some((0, 0, 240, 320))
    );
    let vis_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CGRAF_PORT_VIS_RGN_OFFSET)
        .unwrap();
    let clip_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
        .unwrap();
    let update_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET)
        .unwrap();
    let structure_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_STRUCTURE_RGN_OFFSET)
        .unwrap();
    let content_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_CONTENT_RGN_OFFSET)
        .unwrap();
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, vis_rgn),
        Some((0, 0, 240, 320))
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, clip_rgn),
        Some((i16::MIN, i16::MIN, i16::MAX, i16::MAX))
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, update_rgn),
        Some((0, 0, 0, 0))
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, structure_rgn),
        Some((1, 9, 262, 332))
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, content_rgn),
        Some((20, 10, 260, 330))
    );
    let def_proc_handle = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_DEF_PROC_OFFSET)
        .unwrap();
    let def_proc = loaded.memory.read_u32_be(def_proc_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(def_proc), Some(0));
    let title_handle = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_TITLE_HANDLE_OFFSET)
        .unwrap();
    let title = loaded.memory.read_u32_be(title_handle).unwrap();
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, title),
        Some(b"Document".to_vec())
    );
    assert_ne!(
        loaded
            .memory
            .read_u32_be(window + PPC_CWINDOW_STATE_HANDLE_OFFSET),
        Some(0)
    );
    assert_eq!(loaded.memory.read_u32_be(window + 152), Some(0x1234_5678));
    assert_eq!(
        loaded.current_front_buffer(),
        Some(PpcFrontBuffer {
            base_addr: record.base_addr,
            row_bytes: record.row_bytes,
            width: record.width,
            height: record.height,
            depth: record.depth,
        })
    );
}

#[test]
fn native_toolbox_theme_changes_chrome_without_changing_guest_geometry() {
    for depth in [1, 8, 16] {
        let pef = synthetic_pef_with_import(b"NewCWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        loaded.set_ui_theme(UiThemeId::SystemlessDefault);
        let front = ppc_live_front_buffer_for_gworld(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
        )
        .unwrap();
        let mut palette = if depth == 1 {
            UiThemeId::ClassicSystem7
        } else {
            UiThemeId::SystemlessDefault
        }
        .provider()
        .palette();
        if depth == 1 {
            palette.desktop_dark = Rgb8 { r: 0, g: 0, b: 0 };
            palette.desktop_light = Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            };
        }
        for (point, color) in [
            ((700, 400), palette.desktop_dark),
            ((701, 400), palette.desktop_light),
        ] {
            let expected =
                ppc_physical_screen_color_pixel(front, ppc_theme_rgb(color), &loaded.screen_clut)
                    .unwrap();
            assert_eq!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, point),
                Some(expected)
            );
        }
        let bounds_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(bounds_ptr, vec![0; 32]);
        let window = create_test_cwindow(
            &mut loaded,
            bounds_ptr,
            (100, 100, 260, 300),
            0,
            true,
            u32::MAX,
        );
        let content = loaded
            .memory
            .read_u32_be(window + PPC_CWINDOW_CONTENT_RGN_OFFSET)
            .unwrap();
        let geometry = ppc_read_rgn_bbox(&mut loaded.memory, content);
        let expected = ppc_physical_screen_color_pixel(
            front,
            ppc_theme_rgb(palette.frame_light),
            &loaded.screen_clut,
        )
        .unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (150, 84)),
            Some(expected)
        );
        assert_eq!(geometry, Some((100, 100, 260, 300)));
        ppc_standard_file_draw_button(
            &mut loaded.memory,
            front,
            &loaded.gworlds,
            (0, 0, 600, 800),
            (320, 100, 340, 180),
            b"Open",
            true,
            true,
        );
        let expected_button = ppc_physical_screen_color_pixel(
            front,
            ppc_theme_rgb(palette.frame_light),
            &loaded.screen_clut,
        )
        .unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (160, 325)),
            Some(expected_button)
        );
        let marker =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        ppc_quickdraw_write_raw_pixel(&mut loaded.memory, front, (150, 150), marker);
        loaded.set_ui_theme(UiThemeId::ClassicSystem7);
        ppc_draw_existing_window_frame(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.window_list,
            window,
            false,
        );
        assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, content), geometry);
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (150, 150)),
            Some(marker)
        );
    }
}

#[test]
fn copy_bits_does_not_overwrite_a_front_window() {
    // ClipAbove removes every front structure region before the Window
    // Manager asks a rear WDEF to draw. Macintosh Toolbox Essentials
    // (1992), pp. 4-106 and 4-118--4-119.
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let back = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (100, 100, 260, 300),
        0,
        true,
        u32::MAX,
    );
    let _front = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (120, 250, 280, 450),
        0,
        true,
        u32::MAX,
    );
    let front_buffer =
        ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let bitmap = PPC_DATA_BASE + 0x2000;
    let pixels = bitmap + 64;
    loaded.memory.add_region(bitmap, vec![0; 128]);
    loaded.memory.write_u32_be(bitmap, pixels).unwrap();
    loaded.memory.write_u16_be(bitmap + 4, 8).unwrap();
    ppc_write_rect(&mut loaded.memory, bitmap + 6, 0, 0, 8, 64).unwrap();
    ppc_write_rect(&mut loaded.memory, bitmap + 20, 0, 0, 8, 64).unwrap();
    ppc_write_rect(&mut loaded.memory, bitmap + 28, 50, 100, 58, 183).unwrap();
    loaded.cpu.gpr[3] = back;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetPort);
    for point in [(275, 150), (210, 150)] {
        assert!(ppc_quickdraw_write_raw_pixel(
            &mut loaded.memory, front_buffer, point, 0x7b,
        ));
    }
    loaded.cpu.gpr[3] = bitmap;
    loaded.cpu.gpr[4] = back + 2;
    loaded.cpu.gpr[5] = bitmap + 20;
    loaded.cpu.gpr[6] = bitmap + 28;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CopyBits);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front_buffer, (275, 150)),
        Some(0x7b),
        "CopyBits painted a back window through the front window"
    );
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front_buffer, (210, 150)),
        Some(0x7b),
        "CopyBits must still update the unobscured back window"
    );
}

#[test]
fn ppc_direct_chrome_redraw_does_not_paint_back_window_through_front_content() {
    // ClipAbove removes every front structure region before the Window
    // Manager asks a rear WDEF to draw. Macintosh Toolbox Essentials
    // (1992), pp. 4-106 and 4-118--4-119.
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let back = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (100, 100, 260, 300),
        0,
        true,
        u32::MAX,
    );
    let _front = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (120, 250, 280, 450),
        0,
        true,
        u32::MAX,
    );
    let front_buffer =
        ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let protected = (300, 150);
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front_buffer,
        protected,
        0x7b,
    ));

    ppc_draw_existing_window_frame(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.window_list,
        back,
        false,
    );

    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front_buffer, protected),
        Some(0x7b),
        "back-window chrome must be clipped by the front window's content",
    );
}

#[test]
fn hle_import_runner_new_cwindow_draws_document_frame_at_supported_depths() {
    for depth in [1, 2, 4, 8, 16] {
        let pef = synthetic_pef_with_import(b"NewCWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

        let front = ppc_live_front_buffer_for_gworld(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
        )
        .unwrap();
        let framebuffer_len = front.row_bytes * front.height;
        let pattern = (0..usize::try_from(framebuffer_len).unwrap())
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(depth as u8) ^ 0xa9)
            .collect::<Vec<_>>();
        loaded
            .memory
            .write_bytes(front.base_addr, &pattern)
            .unwrap();
        let outside = (98, 81);
        let outside_before =
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, outside).unwrap();

        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0; 32]);
        ppc_write_rect(&mut loaded.memory, scratch, 100, 100, 200, 300).unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Document");
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewCWindow;
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = u32::MAX;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;

        let probe = loaded.run_with_hle_imports(64);

        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        let window = loaded.cpu.gpr[3];
        let surface =
            ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, window).unwrap();
        let black =
            ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK)
                .unwrap();
        let white =
            ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE)
                .unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((-1, -18)),),
            Some(black),
            "{depth}bpp document top frame did not draw",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((30, -18)),),
            Some(white),
            "{depth}bpp title bar did not draw",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((30, -17)),),
            Some(black),
            "{depth}bpp active title stripes did not draw",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((8, -15)),),
            Some(black),
            "{depth}bpp close box did not draw",
        );
        let title_ink = (-12..-4)
            .flat_map(|v| (100..200).map(move |h| (h, v)))
            .filter(|&point| {
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point(point))
                    == Some(black)
            })
            .count();
        assert!(title_ink > 0, "{depth}bpp title text did not draw");
        assert_eq!(
            loaded.memory.read_u8(window + PPC_CWINDOW_HILITED_OFFSET),
            Some(1),
            "{depth}bpp front window was not activated",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, outside),
            Some(outside_before),
            "{depth}bpp frame changed a pixel outside the structure region",
        );

        let frame_before_title =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 20, b"X");
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = scratch + 20;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle),
        );
        let frame_after_title =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();
        assert_ne!(
            frame_after_title, frame_before_title,
            "{depth}bpp SetWTitle did not redraw visible chrome",
        );
        let title_handle = loaded
            .memory
            .read_u32_be(window + PPC_CWINDOW_TITLE_HANDLE_OFFSET)
            .unwrap();
        let title_ptr = loaded.memory.read_u32_be(title_handle).unwrap();
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, title_ptr),
            Some(b"X".to_vec()),
        );

        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::HighlightWindow),
        );
        assert_eq!(
            loaded.memory.read_u8(window + PPC_CWINDOW_HILITED_OFFSET),
            Some(0),
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((30, -17)),),
            Some(white),
            "{depth}bpp inactive title retained stripes",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((8, -15)),),
            Some(white),
            "{depth}bpp inactive title retained its close box",
        );

        loaded.cpu.gpr[4] = 1;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::HighlightWindow),
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((30, -17)),),
            Some(black),
            "{depth}bpp reactivated title did not restore stripes",
        );

        loaded.cpu.gpr[3] = window;
        if depth == 16 {
            assert!(ppc_quickdraw_write_raw_pixel(
                &mut loaded.memory,
                front,
                (150, 150),
                0x1234,
            ));
        }
        run_test_import(&mut loaded, PpcImportDispatcherTarget::HideWindow);
        if depth == 16 {
            assert_ne!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (150, 150)),
                Some(0x1234),
                "HideWindow left the hidden window's content on screen",
            );
        }
        let hidden_frame =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 20, b"Hidden");
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = scratch + 20;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle),
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(hidden_frame.clone()),
            "{depth}bpp SetWTitle repainted a hidden window",
        );
        loaded.cpu.gpr[3] = window;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ShowWindow);
        assert_ne!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(hidden_frame),
            "{depth}bpp ShowWindow did not paint the deferred title",
        );

        ppc_write_rect(&mut loaded.memory, scratch, 250, 350, 350, 550).unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Second");
        loaded.imports[0].symbol_name = "NewCWindow".to_string();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = u32::MAX;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
        let second = loaded.cpu.gpr[3];
        let second_surface =
            ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, second).unwrap();
        assert_eq!(
            loaded.memory.read_u8(window + PPC_CWINDOW_HILITED_OFFSET),
            Some(0),
            "{depth}bpp previous front window stayed active",
        );
        assert_eq!(
            loaded.memory.read_u8(second + PPC_CWINDOW_HILITED_OFFSET),
            Some(1),
            "{depth}bpp new front window was not activated",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((30, -17)),),
            Some(white),
            "{depth}bpp previous title was not visibly deactivated",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(
                &mut loaded.memory,
                front,
                second_surface.local_point((30, -17)),
            ),
            Some(black),
            "{depth}bpp new title was not visibly activated",
        );

        loaded.cpu.gpr[3] = window;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SelectWindow);
        assert_eq!(
            loaded.memory.read_u8(window + PPC_CWINDOW_HILITED_OFFSET),
            Some(1),
        );
        assert_eq!(
            loaded.memory.read_u8(second + PPC_CWINDOW_HILITED_OFFSET),
            Some(0),
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, surface.local_point((30, -17)),),
            Some(black),
            "{depth}bpp selected title was not visibly activated",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(
                &mut loaded.memory,
                front,
                second_surface.local_point((30, -17)),
            ),
            Some(white),
            "{depth}bpp prior title was not visibly deactivated by SelectWindow",
        );
    }
}

#[test]
fn hle_import_runner_kiosk_suppresses_document_chrome_but_keeps_dialog_frames() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
    loaded.toolbox_startup.host_menu_bar_hidden = true;

    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let framebuffer_len = front.row_bytes * front.height;
    let pattern = (0..usize::try_from(framebuffer_len).unwrap())
        .map(|index| (index as u8).wrapping_mul(37).wrapping_add(8) ^ 0xa9)
        .collect::<Vec<_>>();
    loaded
        .memory
        .write_bytes(front.base_addr, &pattern)
        .unwrap();

    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, scratch, 100, 100, 200, 300).unwrap();
    ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Game Window");
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0;

    let document_frame_point = (82, 99);
    let document_frame_before =
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, document_frame_point);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    let document = loaded.cpu.gpr[3];

    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, document_frame_point),
        document_frame_before,
    );
    assert_eq!(
        loaded.memory.read_u8(document + PPC_CWINDOW_HILITED_OFFSET),
        Some(1),
    );

    ppc_write_rect(&mut loaded.memory, scratch, 300, 300, 400, 500).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 1;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0;
    let dialog_frame_point = (299, 300);
    let dialog_frame_before =
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, dialog_frame_point);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);

    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, dialog_frame_point),
        dialog_frame_before,
    );
}

#[test]
fn ppc_paint_behind_hidden_menu_uses_classic_desktop_pattern() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.toolbox_startup.host_menu_bar_hidden = true;
    let front = ppc_live_front_buffer_for_gworld(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
    )
    .unwrap();
    ppc_write_rgn_bbox(
        &mut loaded.memory,
        PPC_GRAY_RGN_HANDLE,
        0,
        0,
        40,
        40,
    )
    .unwrap();
    let mut heap_cursor = loaded.heap_cursor();
    let heap_limit = loaded.heap_limit();
    let mut last_mem_error = loaded.last_mem_error();
    let mut handles = Vec::new();
    ppc_paint_behind(
        None,
        &mut loaded.memory,
        &loaded.gworlds,
        0,
        PPC_GRAY_RGN_HANDLE,
        true,
        &mut heap_cursor,
        heap_limit,
        &mut last_mem_error,
        &mut handles,
    );
    let black = ppc_physical_screen_color_pixel(
        front,
        ppc_standard_desktop_color(&loaded.gworlds, 0, 0),
        &loaded.screen_clut,
    )
    .unwrap();
    let white = ppc_physical_screen_color_pixel(
        front,
        ppc_standard_desktop_color(&loaded.gworlds, 1, 0),
        &loaded.screen_clut,
    )
    .unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 0)),
        Some(black),
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1, 0)),
        Some(white),
        "host menu suppression must not turn PaintBehind's desktop solid black",
    );
}

#[test]
fn ppc_window_removal_exposure_uses_pattern_when_host_hides_menu_bar() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let front = ppc_live_front_buffer_for_gworld(
        &mut loaded.memory,
        &loaded.gworlds,
        PPC_MAIN_GWORLD,
    )
    .unwrap();
    let mut event_queue = VecDeque::new();

    ppc_restore_window_removal_exposure(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.window_list,
        Some((0, 0, 2, 2)),
        true,
        &mut event_queue,
        0,
        PpcInputSnapshot::default(),
    );

    let black = ppc_physical_screen_color_pixel(
        front,
        ppc_standard_desktop_color(&loaded.gworlds, 0, 0),
        &loaded.screen_clut,
    )
    .unwrap();
    let white = ppc_physical_screen_color_pixel(
        front,
        ppc_standard_desktop_color(&loaded.gworlds, 1, 0),
        &loaded.screen_clut,
    )
    .unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 0)),
        Some(black),
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1, 0)),
        Some(white),
        "moving or closing a window must restore GrayRgn even when its menu bar is hidden",
    );
}

#[test]
fn ppc_window_geometry_exposure_preserves_newly_covered_pixels() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, scratch, 20, 20, 80, 80).unwrap();
    ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Window");
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    assert_ne!(loaded.cpu.gpr[3], 0);

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (40, 40),
        17,
    ));
    let mut event_queue = VecDeque::new();
    ppc_restore_window_removal_exposure(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.window_list,
        Some((0, 0, 100, 100)),
        true,
        &mut event_queue,
        0,
        PpcInputSnapshot::default(),
    );

    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (40, 40)),
        Some(17),
    );
    let desktop = ppc_physical_screen_color_pixel(
        front,
        ppc_standard_desktop_color(&loaded.gworlds, 0, 0),
        &loaded.screen_clut,
    )
    .unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 0)),
        Some(desktop),
    );
}

#[test]
fn hle_import_runner_track_go_away_retains_restores_and_uses_release_point() {
    for depth in [1, 2, 4, 8, 16] {
        let pef = synthetic_pef_with_import(b"NewCWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0; 32]);
        ppc_write_rect(&mut loaded.memory, scratch, 100, 100, 200, 300).unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Document");
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = u32::MAX;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
        let window = loaded.cpu.gpr[3];
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let framebuffer_len = front.row_bytes * front.height;
        let baseline =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();

        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LegacyWindow(
            PpcLegacyWindowOperation::TrackGoAway,
        );
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = (90u32 << 16) | 108;
        let original_sp = loaded.cpu.gpr[1];
        let original_start = loaded.cpu.gpr[4];
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 90,
            mouse_h: 108,
            ..PpcInputSnapshot::default()
        });

        let probe = loaded.run_with_hle_imports(64);

        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(loaded.cpu.pc, loaded.import_trap_base);
        assert_eq!(loaded.cpu.gpr[1], original_sp);
        assert_eq!(loaded.cpu.gpr[3], window);
        assert_eq!(loaded.cpu.gpr[4], original_start);
        assert!(loaded.toolbox_startup.go_away_tracking.is_some());
        assert_ne!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline.clone()),
            "{depth}bpp close-box feedback did not draw",
        );

        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 120,
            mouse_h: 140,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline.clone()),
            "{depth}bpp moving outside did not restore the close box",
        );

        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 90,
            mouse_h: 108,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        loaded.event_queue.push_back(PpcQueuedEvent {
            what: 2,
            message: 0,
            when: 0,
            where_v: 90,
            where_h: 108,
            modifiers: 0,
        });
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 90,
            mouse_h: 108,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.cpu.gpr[3], 1, "{depth}bpp inside release");
        assert!(loaded.toolbox_startup.go_away_tracking.is_none());
        assert!(!loaded.event_queue.iter().any(|event| event.what == 2));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline.clone()),
            "{depth}bpp accepted tracking did not restore pixels",
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = original_start;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 90,
            mouse_h: 108,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 120,
            mouse_h: 140,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.cpu.gpr[3], 0, "{depth}bpp outside release");
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline.clone()),
            "{depth}bpp cancelled tracking did not restore pixels",
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = (120u32 << 16) | 140;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 90,
            mouse_h: 108,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.cpu.gpr[3], 0, "{depth}bpp invalid start");
        assert!(loaded.toolbox_startup.go_away_tracking.is_none());
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline.clone()),
            "{depth}bpp invalid start changed pixels",
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = original_start;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 90,
            mouse_h: 108,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert!(loaded
            .memory
            .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 0)
            .is_some());
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.cpu.gpr[3], 0, "{depth}bpp hidden cancellation");
        assert!(loaded.toolbox_startup.go_away_tracking.is_none());
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline),
            "{depth}bpp hidden cancellation did not restore pixels",
        );
    }
}

#[test]
fn hle_import_runner_drag_window_retains_outline_and_moves_on_valid_release() {
    for depth in [1, 2, 4, 8, 16] {
        let pef = synthetic_pef_with_import(b"NewCWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0; 40]);
        ppc_write_rect(&mut loaded.memory, scratch, 100, 100, 200, 300).unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Document");
        ppc_write_rect(&mut loaded.memory, scratch + 24, 20, 0, 480, 640).unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = u32::MAX;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
        let window = loaded.cpu.gpr[3];
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let framebuffer_len = front.row_bytes * front.height;
        let pattern = (0..framebuffer_len)
            .map(|index| (index as u8).wrapping_mul(37).wrapping_add(depth as u8))
            .collect::<Vec<_>>();
        loaded
            .memory
            .write_bytes(front.base_addr, &pattern)
            .unwrap();
        let baseline =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();

        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LegacyWindow(
            PpcLegacyWindowOperation::DragWindow,
        );
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = (90u32 << 16) | 150;
        loaded.cpu.gpr[5] = scratch + 24;
        let original_sp = loaded.cpu.gpr[1];
        let original_args = [loaded.cpu.gpr[3], loaded.cpu.gpr[4], loaded.cpu.gpr[5]];
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 90,
            mouse_h: 150,
            ..PpcInputSnapshot::default()
        });

        let probe = loaded.run_with_hle_imports(64);

        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(loaded.cpu.pc, loaded.import_trap_base);
        assert_eq!(loaded.cpu.gpr[1], original_sp);
        assert_eq!(
            [loaded.cpu.gpr[3], loaded.cpu.gpr[4], loaded.cpu.gpr[5]],
            original_args,
        );
        assert!(loaded.toolbox_startup.drag_window_tracking.is_some());
        assert_ne!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline.clone()),
            "{depth}bpp initial drag outline did not draw",
        );

        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 120,
            mouse_h: 180,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(
            loaded
                .toolbox_startup
                .drag_window_tracking
                .as_ref()
                .unwrap()
                .outline,
            (111, 129, 232, 332),
        );
        loaded.event_queue.push_back(PpcQueuedEvent {
            what: 2,
            message: 0,
            when: 0,
            where_v: 130,
            where_h: 190,
            modifiers: 0,
        });
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 130,
            mouse_h: 190,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert!(loaded.toolbox_startup.drag_window_tracking.is_none());
        assert!(!loaded.event_queue.iter().any(|event| event.what == 2));
        assert_eq!(
            ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, window),
            Some((140, 140, 240, 340)),
            "{depth}bpp valid release did not apply the final delta",
        );
        assert_eq!(*loaded.current_gworld, window);
        let moved_frame =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();
        assert_ne!(
            moved_frame, baseline,
            "{depth}bpp accepted drag did not repaint the moved window",
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = (130u32 << 16) | 190;
        loaded.cpu.gpr[5] = scratch + 24;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 150,
            mouse_h: 210,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 10,
            mouse_h: 10,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(
            ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, window),
            Some((140, 140, 240, 340)),
            "{depth}bpp out-of-bounds release moved the window",
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(moved_frame.clone()),
            "{depth}bpp cancelled drag did not restore the framebuffer",
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        loaded.cpu.gpr[4] = (130u32 << 16) | 190;
        loaded.cpu.gpr[5] = scratch + 24;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 130,
            mouse_h: 190,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert!(loaded
            .memory
            .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 0)
            .is_some());
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert!(loaded.toolbox_startup.drag_window_tracking.is_none());
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(moved_frame),
            "{depth}bpp hidden cancellation did not restore the framebuffer",
        );
    }
}

#[test]
fn hle_import_runner_grow_window_retains_clamps_and_uses_release_size() {
    for depth in [1, 2, 4, 8, 16] {
        let pef = synthetic_pef_with_import(b"NewCWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

        let scratch = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(scratch, vec![0; 40]);
        ppc_write_rect(&mut loaded.memory, scratch, 100, 100, 200, 300).unwrap();
        ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Document");
        ppc_write_rect(&mut loaded.memory, scratch + 24, 50, 80, 180, 260).unwrap();
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = scratch;
        loaded.cpu.gpr[5] = scratch + 8;
        loaded.cpu.gpr[6] = 1;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = u32::MAX;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
        let window = loaded.cpu.gpr[3];
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let framebuffer_len = front.row_bytes * front.height;
        let pattern = (0..framebuffer_len)
            .map(|index| (index as u8).wrapping_mul(29).wrapping_add(depth as u8))
            .collect::<Vec<_>>();
        loaded
            .memory
            .write_bytes(front.base_addr, &pattern)
            .unwrap();
        let baseline =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();

        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LegacyWindow(
            PpcLegacyWindowOperation::GrowWindow,
        );
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        // Retain the event point even when the live cursor has moved.
        loaded.cpu.gpr[4] = (195u32 << 16) | 290;
        loaded.cpu.gpr[5] = scratch + 24;
        let original_sp = loaded.cpu.gpr[1];
        let original_args = [loaded.cpu.gpr[3], loaded.cpu.gpr[4], loaded.cpu.gpr[5]];
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 200,
            mouse_h: 300,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(loaded.cpu.gpr[1], original_sp);
        assert_eq!(
            [loaded.cpu.gpr[3], loaded.cpu.gpr[4], loaded.cpu.gpr[5]],
            original_args,
        );

        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 350,
            mouse_h: 500,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(
            loaded
                .toolbox_startup
                .grow_window_tracking
                .as_ref()
                .unwrap()
                .outline,
            (81, 99, 282, 362),
            "{depth}bpp held size did not clamp to maximums",
        );
        loaded.event_queue.push_back(PpcQueuedEvent {
            what: 2,
            message: 0,
            when: 0,
            where_v: 250,
            where_h: 380,
            modifiers: 0,
        });
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 250,
            mouse_h: 380,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.cpu.gpr[3], (155u32 << 16) | 260);
        assert!(loaded.toolbox_startup.grow_window_tracking.is_none());
        assert!(!loaded.event_queue.iter().any(|event| event.what == 2));
        assert_eq!(
            ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, window),
            Some((100, 100, 200, 300)),
            "{depth}bpp GrowWindow resized instead of returning a proposal",
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline.clone()),
            "{depth}bpp accepted grow did not restore the framebuffer",
        );

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window;
        // Retain the event point even when the live cursor has moved.
        loaded.cpu.gpr[4] = (195u32 << 16) | 290;
        loaded.cpu.gpr[5] = scratch + 24;
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 220,
            mouse_h: 320,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 195,
            mouse_h: 290,
            ..PpcInputSnapshot::default()
        });
        let probe = loaded.run_with_hle_imports(64);
        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(loaded.cpu.gpr[3], 0, "{depth}bpp unchanged size sentinel");
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(baseline),
            "{depth}bpp unchanged grow did not restore the framebuffer",
        );
    }
}

#[test]
fn new_cwindow_manager_regions_follow_move_and_size() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, scratch, 20, 10, 260, 330).unwrap();
    ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Document");
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    let window = loaded.cpu.gpr[3];
    let structure_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_STRUCTURE_RGN_OFFSET)
        .unwrap();
    let content_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_CONTENT_RGN_OFFSET)
        .unwrap();

    let mut move_cpu = loaded.cpu.clone();
    move_cpu.gpr[3] = window;
    move_cpu.gpr[4] = 30;
    move_cpu.gpr[5] = 40;
    assert_eq!(
        ppc_move_window(&move_cpu, &mut loaded.memory, &mut loaded.gworlds),
        Some(())
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, content_rgn),
        Some((40, 30, 280, 350))
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, structure_rgn),
        Some((21, 29, 282, 352))
    );

    let mut size_cpu = loaded.cpu.clone();
    size_cpu.gpr[3] = window;
    size_cpu.gpr[4] = 200;
    size_cpu.gpr[5] = 100;
    assert_eq!(
        ppc_size_window(&size_cpu, &mut loaded.memory, &mut loaded.gworlds),
        Some(())
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, content_rgn),
        Some((40, 30, 140, 230))
    );
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, structure_rgn),
        Some((21, 29, 142, 232))
    );
}

#[test]
fn hidden_ppc_geometry_changes_do_not_repaint_visible_pixels() {
    let pef = synthetic_pef_with_import(b"MoveWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let visible = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (100, 100, 260, 300),
        0,
        true,
        u32::MAX,
    );
    let hidden = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (120, 120, 220, 220),
        0,
        false,
        u32::MAX,
    );
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    ppc_quickdraw_write_pixel(&mut loaded.memory, front, (160, 160), PPC_RGB_BLACK);
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (160, 160));
    loaded.set_event_queue(std::iter::empty());

    loaded.cpu.gpr[3] = hidden;
    loaded.cpu.gpr[4] = 140;
    loaded.cpu.gpr[5] = 140;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SizeWindow);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (160, 160)),
        before,
        "resizing a hidden window must not erase a visible window's content",
    );
    assert!(loaded.event_queue().is_empty());

    loaded.cpu.gpr[3] = hidden;
    loaded.cpu.gpr[4] = 300;
    loaded.cpu.gpr[5] = 300;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveWindow);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (160, 160)),
        before,
        "moving a hidden window must not erase a visible window's content",
    );
    assert!(loaded.event_queue().is_empty());
    assert_eq!(ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list), Some(visible));
}

#[test]
fn moving_window_with_guest_modified_port_rect_preserves_backing_dimensions() {
    let pef = synthetic_pef_with_import(b"MoveWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let window = create_test_cwindow(
        &mut loaded, bounds_ptr, (40, 30, 520, 670), 0, false, u32::MAX,
    );
    let pixmap_handle = loaded.memory.read_u32_be(window + 2).unwrap();
    let pixmap = loaded.memory.read_u32_be(pixmap_handle).unwrap();
    let before_pixel_bounds = ppc_read_rect(&mut loaded.memory, pixmap + 6).unwrap();
    // portRect is guest-writable and need not describe the allocated PixMap.
    ppc_write_rect(&mut loaded.memory, window + 16, 0, 0, 1600, 1800).unwrap();
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = 100;
    loaded.cpu.gpr[5] = 80;
    loaded.cpu.gpr[6] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveWindow);
    assert_eq!(ppc_read_rect(&mut loaded.memory, window + 16), Some((0, 0, 1600, 1800)));
    let record = loaded.gworlds.iter().find(|record| record.port == window).unwrap();
    assert_eq!((record.width, record.height), (640, 480));
    let after = ppc_read_rect(&mut loaded.memory, pixmap + 6).unwrap();
    assert_eq!(after.2 - after.0, before_pixel_bounds.2 - before_pixel_bounds.0);
    assert_eq!(after.3 - after.1, before_pixel_bounds.3 - before_pixel_bounds.1);
    assert_eq!((after.0, after.1), (-80, -100));
}

#[test]
fn disposing_hidden_ppc_window_does_not_repaint_exposed_pixels() {
    let pef = synthetic_pef_with_import(b"DisposeWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let _visible = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (100, 100, 260, 300),
        0,
        true,
        u32::MAX,
    );
    let hidden = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (120, 120, 220, 220),
        0,
        false,
        u32::MAX,
    );
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    ppc_quickdraw_write_pixel(&mut loaded.memory, front, (160, 160), PPC_RGB_BLACK);
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (160, 160));
    loaded.set_event_queue(std::iter::empty());

    loaded.cpu.gpr[3] = hidden;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow),
    );

    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (160, 160)),
        before,
        "disposing a hidden window must not repaint its former structure",
    );
    assert!(!loaded
        .event_queue()
        .iter()
        .any(|event| event.message == hidden));
    assert!(!loaded.window_list.contains(&hidden));
}

#[test]
fn ppc_move_window_front_true_reorders_and_activates_window() {
    let pef = synthetic_pef_with_import(b"MoveWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let back = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (100, 100, 260, 300),
        0,
        true,
        u32::MAX,
    );
    let front = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (120, 120, 280, 320),
        0,
        true,
        u32::MAX,
    );
    loaded.set_event_queue(std::iter::empty());
    assert_eq!(loaded.window_list.first(), Some(front));

    loaded.cpu.gpr[3] = back;
    loaded.cpu.gpr[4] = 100;
    loaded.cpu.gpr[5] = 100;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MoveWindow);

    assert_eq!(loaded.window_list.first(), Some(back));
    assert_eq!(
        ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list),
        Some(back)
    );
    assert_eq!(*loaded.current_gworld, back);
    assert_eq!(
        loaded.memory.read_u8(back + PPC_CWINDOW_HILITED_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded.memory.read_u8(front + PPC_CWINDOW_HILITED_OFFSET),
        Some(0)
    );
    assert_eq!(
        ppc_find_window_at_point(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.window_list,
            180,
            180,
            20,
        ),
        (3, back),
        "front=true must update hit testing as well as the list",
    );
}

#[test]
fn ppc_zoom_window_recalculates_visibility_and_queues_redraw() {
    let pef = synthetic_pef_with_import(b"ZoomWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let window = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (100, 100, 240, 300),
        8,
        true,
        u32::MAX,
    );
    let vis_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CGRAF_PORT_VIS_RGN_OFFSET)
        .unwrap();
    let old_vis = ppc_read_rgn_bbox(&mut loaded.memory, vis_rgn).unwrap();
    let update_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET)
        .unwrap();
    ppc_set_empty_rgn(&mut loaded.memory, update_rgn);
    loaded.set_event_queue(std::iter::empty());

    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ZoomWindow),
    );

    assert_eq!(
        ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, window),
        Some((20, 0, ppc_main_screen_height() as i16, ppc_main_screen_width() as i16)),
    );
    assert_ne!(
        ppc_read_rgn_bbox(&mut loaded.memory, vis_rgn),
        Some(old_vis),
        "ZoomWindow must recalculate the visible region",
    );
    let update = ppc_read_rgn_bbox(&mut loaded.memory, update_rgn).unwrap();
    assert!(update.0 < update.2 && update.1 < update.3);
    assert!(loaded
        .event_queue()
        .iter()
        .any(|event| event.what == 6 && event.message == window));
    assert_eq!(*loaded.current_gworld, window);
}

#[test]
fn ppc_zoom_front_promotion_preserves_promoted_window_pixels() {
    let pef = synthetic_pef_with_import(b"ZoomWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    let _formerly_front = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (100, 100, 260, 300),
        1,
        true,
        u32::MAX,
    );
    let _middle = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (120, 120, 280, 320),
        1,
        true,
        0,
    );
    let promoted = create_test_cwindow(
        &mut loaded,
        bounds_ptr,
        (150, 150, 300, 350),
        1,
        true,
        0,
    );
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let black = ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut)
        .unwrap();
    // The promoted dialog's top structure edge lies inside both older
    // windows' content rectangles. A back-to-front repaint must leave
    // this edge owned by the promoted window.
    let promoted_edge = (200, 142);
    loaded.set_event_queue(std::iter::empty());
    loaded.cpu.gpr[3] = promoted;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ZoomWindow),
    );

    assert_eq!(loaded.window_list.first(), Some(promoted));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, promoted_edge),
        Some(black),
        "front-promoted ZoomWindow frame must remain above older overlapping windows",
    );
}

#[test]
fn hle_import_runner_set_win_color_applies_content_color_and_queues_update() {
    let pef = synthetic_pef_with_import(b"SetWinColor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let table = [
        0, 0, 0, 1, // reserved seed
        0, 0, // reserved flags
        0, 0, // one entry
        0, 0, // wContentColor
        0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc,
    ];
    let table_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &table,
    );
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = table_handle;
    loaded
        .memory
        .write_u8(PPC_MAIN_GWORLD + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded.memory.write_u16_be(PPC_MAIN_SCREEN_BASE, 0).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(PPC_MAIN_GWORLD + PPC_CWINDOW_COLOR_TABLE_HANDLE_OFFSET),
        Some(table_handle)
    );
    assert_eq!(
        loaded.quickdraw_back_color,
        PpcRgbColor {
            red: 0x1234,
            green: 0x5678,
            blue: 0x9abc,
        }
    );
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let expected = ppc_quickdraw_surface_color_pixel(
        &mut loaded.memory,
        surface,
        loaded.quickdraw_back_color,
    )
    .unwrap();
    match surface.front_buffer.depth {
        8 => assert_eq!(
            loaded.memory.read_u8(PPC_MAIN_SCREEN_BASE),
            Some(expected as u8)
        ),
        16 => assert_eq!(
            loaded.memory.read_u16_be(PPC_MAIN_SCREEN_BASE),
            Some(expected)
        ),
        depth => panic!("unexpected synthetic screen depth {depth}"),
    }
    assert!(loaded
        .event_queue
        .iter()
        .any(|event| event.what == 6 && event.message == PPC_MAIN_GWORLD));
}

#[test]
fn hle_import_runner_new_cwindow_draws_dbox_structure_region() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 100, 100, 200, 300).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 1;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let window = loaded.cpu.gpr[3];
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, window).unwrap();
    let black =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    let white =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_WHITE).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (92, 92)),
        Some(black)
    );
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (94, 94)),
        Some(black)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (95, 95)),
        Some(black)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (150, 150)),
        Some(white)
    );
}

#[test]
fn hle_import_runner_updates_guest_window_visibility() {
    let window = PPC_DATA_BASE + 0x1000;
    let pef = synthetic_pef_with_import(b"HideWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded
        .memory
        .write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u8(window + PPC_CWINDOW_HILITED_OFFSET, 1)
        .unwrap();
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = window);
    loaded.cpu.gpr[3] = window;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u8(window + PPC_CWINDOW_VISIBLE_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u8(window + PPC_CWINDOW_HILITED_OFFSET),
        Some(0)
    );
    assert_eq!(*loaded.current_gworld, window);

    let pef = synthetic_pef_with_import(b"ShowHide");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded
        .memory
        .add_region(window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u8(window + PPC_CWINDOW_VISIBLE_OFFSET),
        Some(1)
    );
}

#[test]
fn hle_import_runner_new_cwindow_uses_current_screen_depth() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut depth_cpu = loaded.cpu.clone();
    depth_cpu.gpr[3] = PPC_MAIN_GDEVICE;
    depth_cpu.gpr[4] = 8;
    let mut last_mem_error = PPC_NO_ERR;
    assert_eq!(
        with_test_display_cluts!(loaded, |screen_clut, color_manager_clut| ppc_set_depth(
            &depth_cpu,
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut last_mem_error,
            test_handles!(loaded),
            &mut loaded.gworlds,
            &mut loaded.toolbox_startup,
            screen_clut,
            color_manager_clut,
        )),
        PPC_NO_ERR
    );

    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 240, 320).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    let window = loaded.cpu.gpr[3];
    let record = loaded
        .gworlds
        .iter()
        .find(|record| record.port == window)
        .unwrap();
    assert_eq!(record.depth, 8);
    assert_eq!(record.base_addr, PPC_MAIN_SCREEN_BASE);
    assert_eq!(record.row_bytes, ppc_main_screen_width() + 16);
    assert_eq!(loaded.memory.read_u16_be(record.pixmap + 32), Some(8));
}

#[test]
fn hle_import_runner_handles_new_cwindow_caller_storage() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let bounds_ptr = scratch;
    let storage_ptr = scratch + 0x100;
    loaded.memory.add_region(scratch, vec![0; 32]);
    loaded
        .memory
        .add_region(storage_ptr, vec![0xaa; PPC_CGRAF_PORT_SIZE as usize]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 20, 10, 260, 330).unwrap();
    let heap_cursor = loaded.heap_cursor();
    let required = ppc_heap_allocation_sequence_size(&[
        PPC_PIXMAP_SIZE,
        4,
        4,
        10,
        4,
        10,
        4,
        10,
        4,
        10,
        4,
        10,
        4,
        4,
        4,
        1,
        4,
        16,
    ])
    .unwrap();
    loaded.cpu.gpr[3] = storage_ptr;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 2;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0x1234_5678;
    let gworld_count = loaded.gworlds.len();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], storage_ptr);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(*loaded.current_gworld, storage_ptr);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
    assert_eq!(loaded.heap_cursor(), heap_cursor + required);
    assert_eq!(loaded.gworlds.len(), gworld_count + 1);
    let record = loaded
        .gworlds
        .iter()
        .find(|record| record.port == storage_ptr)
        .unwrap();
    assert_eq!(record.width, 320);
    assert_eq!(record.height, 240);
    assert_eq!(loaded.memory.read_u16_be(storage_ptr), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(storage_ptr + 2),
        Some(record.pixmap_handle)
    );
    assert_eq!(
        loaded.memory.read_u32_be(storage_ptr + 152),
        Some(0x1234_5678)
    );
}

#[test]
fn hle_import_runner_handles_get_new_cwindow() {
    let pef = synthetic_pef_with_import(b"GetNewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(storage_ptr, vec![0xaa; PPC_CGRAF_PORT_SIZE as usize]);
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(*b"WIND"),
        res_id: 1000,
        name: Vec::new(),
        data: test_wind_resource((40, 50, 240, 350), 0, false, true, 0x1234_5678, b"Document"),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 1000;
    loaded.cpu.gpr[4] = storage_ptr;
    loaded.cpu.gpr[5] = u32::MAX;
    let gworld_count = loaded.gworlds.len();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], storage_ptr);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(*loaded.current_gworld, storage_ptr);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
    assert_eq!(loaded.gworlds.len(), gworld_count + 1);
    let record = loaded
        .gworlds
        .iter()
        .find(|record| record.port == storage_ptr)
        .unwrap();
    assert_eq!(record.width, 300);
    assert_eq!(record.height, 200);
    assert_eq!(record.depth, PPC_MAIN_PIXEL_DEPTH);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, storage_ptr + 16),
        Some((0, 0, 200, 300))
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(storage_ptr + PPC_CWINDOW_VISIBLE_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(storage_ptr + PPC_CWINDOW_GO_AWAY_OFFSET),
        Some(1)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(storage_ptr + PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET),
        Some(0x1234_5678)
    );
    let title_handle = loaded
        .memory
        .read_u32_be(storage_ptr + PPC_CWINDOW_TITLE_HANDLE_OFFSET)
        .unwrap();
    let title = loaded.memory.read_u32_be(title_handle).unwrap();
    assert_eq!(
        ppc_read_pstring_bytes(&mut loaded.memory, title),
        Some(b"Document".to_vec())
    );
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);

    let update_rgn = loaded
        .memory
        .read_u32_be(storage_ptr + PPC_CWINDOW_UPDATE_RGN_OFFSET)
        .unwrap();
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, update_rgn),
        Some((0, 0, 0, 0))
    );
    loaded.cpu.gpr[3] = storage_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ShowWindow);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, update_rgn),
        Some((0, 0, 200, 300))
    );
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, storage_ptr).unwrap();
    let black =
        ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, PPC_RGB_BLACK).unwrap();
    assert_eq!(
        loaded
            .memory
            .read_u8(storage_ptr + PPC_CWINDOW_VISIBLE_OFFSET),
        Some(1)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            surface.front_buffer,
            surface.local_point((-1, -18)),
        ),
        Some(black),
        "ShowWindow did not draw the hidden WIND document frame",
    );
}

#[test]
fn hle_import_runner_get_new_cwindow_rejects_missing_and_malformed_wind_atomically() {
    let pef = synthetic_pef_with_import(b"GetNewCWindow");
    for (label, resource, expected_error) in [
        ("missing", None, PPC_RES_NOT_FOUND_ERR),
        (
            "truncated title",
            Some({
                let mut wind = test_wind_resource(
                    (40, 50, 240, 350),
                    0,
                    true,
                    true,
                    0x1234_5678,
                    b"Document",
                );
                wind.truncate(21);
                wind
            }),
            PPC_PARAM_ERR,
        ),
    ] {
        let mut loaded = load_pef_application(&pef).unwrap();
        let storage_ptr = PPC_DATA_BASE + 0x1000;
        let storage = vec![0xa5; PPC_CGRAF_PORT_SIZE as usize];
        loaded.memory.add_region(storage_ptr, storage.clone());
        if let Some(data) = resource {
            let current_resource_refnum = *loaded.process_file_system.current_resource_file;
            loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"WIND"),
                res_id: 1000,
                name: Vec::new(),
                data,
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
        }
        loaded.cpu.gpr[3] = 1000;
        loaded.cpu.gpr[4] = storage_ptr;
        loaded.cpu.gpr[5] = u32::MAX;
        let gworlds = loaded.gworlds.clone();

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.unsupported_import_index, None, "{label}");
        assert_eq!(loaded.cpu.gpr[3], 0, "{label}");
        assert_eq!(loaded.test_resource_error(), expected_error, "{label}");
        assert_eq!(loaded.gworlds, gworlds, "{label}");
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, storage_ptr, PPC_CGRAF_PORT_SIZE,),
            Some(storage),
            "{label}",
        );
    }
}

#[test]
fn hle_import_runner_get_new_cwindow_associates_matching_palette() {
    let pef = synthetic_pef_with_import(b"GetNewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let storage_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(storage_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(*b"WIND"),
        res_id: 1000,
        name: Vec::new(),
        data: test_wind_resource((20, 30, 220, 330), 0, true, true, 0, b"Palette"),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    let mut palette_resource = vec![0xaa; 48];
    palette_resource[..2].copy_from_slice(&2u16.to_be_bytes());
    palette_resource[16..22].copy_from_slice(&[0x11, 0x11, 0x22, 0x22, 0x33, 0x33]);
    palette_resource[22..24].copy_from_slice(&0x0002u16.to_be_bytes());
    palette_resource[24..26].copy_from_slice(&0u16.to_be_bytes());
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(*b"pltt"),
        res_id: 1000,
        name: Vec::new(),
        data: palette_resource,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 1000;
    loaded.cpu.gpr[4] = storage_ptr;
    loaded.cpu.gpr[5] = u32::MAX;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    let palette_handle = loaded
        .memory
        .read_u32_be(storage_ptr + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET)
        .unwrap();
    assert_ne!(palette_handle, 0);
    assert_eq!(
        loaded
            .memory
            .read_u16_be(storage_ptr + PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET),
        Some(1)
    );
    let palette = loaded.memory.read_u32_be(palette_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(palette), Some(2));
    assert_eq!(loaded.memory.read_u16_be(palette + 2), Some(0));
    assert_eq!(loaded.memory.read_u16_be(palette + 16), Some(0x1111));
    assert_eq!(loaded.memory.read_u16_be(palette + 18), Some(0x2222));
    assert_eq!(loaded.memory.read_u16_be(palette + 20), Some(0x3333));
    assert_eq!(loaded.screen_clut[1], [0x1111, 0x2222, 0x3333]);
}

#[test]
fn hle_import_runner_handles_set_w_ref_con() {
    let pef = synthetic_pef_with_import(b"SetWRefCon");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded
        .memory
        .write_u32_be(window_ptr + 152, 0x1111_2222)
        .unwrap();

    loaded.cpu.gpr[3] = window_ptr;
    loaded.cpu.gpr[4] = 0x1234_5678;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(window_ptr + 152),
        Some(0x1234_5678)
    );
}

#[test]
fn hle_import_runner_handles_get_w_ref_con() {
    let pef = synthetic_pef_with_import(b"GetWRefCon");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded
        .memory
        .write_u32_be(window_ptr + 152, 0x1234_5678)
        .unwrap();

    loaded.cpu.gpr[3] = window_ptr;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn hle_import_runner_handles_size_window() {
    let pef = synthetic_pef_with_import(b"SizeWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window_ptr = PPC_DATA_BASE + 0x1000;
    let pixmap_handle = PPC_DATA_BASE + 0x1100;
    let pixmap = PPC_DATA_BASE + 0x1200;
    loaded
        .memory
        .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded.memory.add_region(pixmap_handle, vec![0; 4]);
    loaded
        .memory
        .add_region(pixmap, vec![0; PPC_PIXMAP_SIZE as usize]);
    loaded
        .memory
        .write_u32_be(window_ptr + 2, pixmap_handle)
        .unwrap();
    loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
    ppc_write_rect(&mut loaded.memory, window_ptr + 16, 0, 0, 240, 320).unwrap();
    ppc_write_rect(&mut loaded.memory, pixmap + 6, -20, -10, 460, 630).unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: window_ptr,
        pixmap_handle,
        pixmap,
        base_addr: PPC_DATA_BASE + 0x2000,
        gdevice: PPC_MAIN_GDEVICE,
        width: 320,
        height: 240,
        depth: PPC_MAIN_PIXEL_DEPTH,
        row_bytes: 1280,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.cpu.gpr[3] = window_ptr;
    loaded.cpu.gpr[4] = 512;
    loaded.cpu.gpr[5] = 384;
    loaded.cpu.gpr[6] = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, window_ptr + 16),
        Some((0, 0, 384, 512))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, pixmap + 6),
        Some((-20, -10, 460, 630))
    );
    let record = loaded
        .gworlds
        .iter()
        .find(|record| record.port == window_ptr)
        .unwrap();
    assert_eq!(record.width, 512);
    assert_eq!(record.height, 384);
    assert_eq!(record.row_bytes, 1280);
}

#[test]
fn hle_import_runner_handles_move_window() {
    let pef = synthetic_pef_with_import(b"MoveWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window_ptr = PPC_DATA_BASE + 0x1000;
    let pixmap_handle = PPC_DATA_BASE + 0x1100;
    let pixmap = PPC_DATA_BASE + 0x1200;
    loaded
        .memory
        .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded.memory.add_region(pixmap_handle, vec![0; 4]);
    loaded
        .memory
        .add_region(pixmap, vec![0; PPC_PIXMAP_SIZE as usize]);
    loaded
        .memory
        .write_u32_be(window_ptr + 2, pixmap_handle)
        .unwrap();
    loaded.memory.write_u32_be(pixmap_handle, pixmap).unwrap();
    ppc_write_rect(&mut loaded.memory, window_ptr + 16, 0, 0, 384, 512).unwrap();
    ppc_write_rect(&mut loaded.memory, pixmap + 6, -20, -10, 460, 630).unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: window_ptr,
        pixmap_handle,
        pixmap,
        base_addr: PPC_DATA_BASE + 0x2000,
        gdevice: PPC_MAIN_GDEVICE,
        width: 512,
        height: 384,
        depth: PPC_MAIN_PIXEL_DEPTH,
        row_bytes: 1280,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.cpu.gpr[3] = window_ptr;
    loaded.cpu.gpr[4] = 30;
    loaded.cpu.gpr[5] = 40;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, window_ptr + 16),
        Some((0, 0, 384, 512))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, pixmap + 6),
        Some((-40, -30, 440, 610))
    );
    let record = loaded
        .gworlds
        .iter()
        .find(|record| record.port == window_ptr)
        .unwrap();
    assert_eq!(record.width, 512);
    assert_eq!(record.height, 384);
    assert_eq!(record.row_bytes, 1280);
}

#[test]
fn hle_import_runner_tracks_ppc_update_region_until_end_update() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, scratch, 20, 10, 260, 330).unwrap();
    ppc_write_pstring_bytes(&mut loaded.memory, scratch + 8, b"Document");
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = scratch;
    loaded.cpu.gpr[5] = scratch + 8;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    let window = loaded.cpu.gpr[3];
    let update_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET)
        .unwrap();
    let rect_ptr = scratch + 16;
    ppc_write_rect(&mut loaded.memory, rect_ptr, 4, 6, 40, 50).unwrap();

    loaded.cpu.gpr[3] = rect_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InvalRect);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, update_rgn),
        Some((4, 6, 40, 50)),
        "InvalRect must publish a concrete local update region",
    );

    loaded.cpu.gpr[3] = window;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::BeginUpdate);
    assert_eq!(*loaded.current_gworld, window);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::EndUpdate);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, update_rgn),
        Some((0, 0, 0, 0)),
        "EndUpdate must clear the committed update region",
    );
}

#[test]
fn hle_import_runner_check_update_reports_first_dirty_window() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CheckUpdate")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 48]);
    let window = create_test_cwindow(&mut loaded, scratch, (20, 10, 260, 330), 0, true, u32::MAX);
    let update = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET)
        .unwrap();
    ppc_write_rgn_bbox(&mut loaded.memory, update, 2, 3, 40, 50).unwrap();
    loaded.cpu.gpr[3] = scratch + 16;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CheckUpdate),
    );
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(scratch + 16), Some(6));
    assert_eq!(loaded.memory.read_u32_be(scratch + 18), Some(window));
    assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update), Some((2, 3, 40, 50)));

    ppc_set_empty_rgn(&mut loaded.memory, update).unwrap();
    loaded.cpu.gpr[3] = scratch + 16;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CheckUpdate),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn hle_import_runner_handles_select_window() {
    let pef = synthetic_pef_with_import(b"SelectWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let window_ptr = PPC_DATA_BASE + 0x1000;
    let gdevice = PPC_DATA_BASE + 0x2000;
    loaded
        .memory
        .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded
        .memory
        .write_u8(window_ptr + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: window_ptr,
        pixmap_handle: PPC_DATA_BASE + 0x1100,
        pixmap: PPC_DATA_BASE + 0x1200,
        base_addr: PPC_DATA_BASE + 0x3000,
        gdevice,
        width: 320,
        height: 240,
        depth: PPC_MAIN_PIXEL_DEPTH,
        row_bytes: 640,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = PPC_MAIN_GWORLD);
    loaded
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = PPC_MAIN_GDEVICE);
    loaded.cpu.gpr[3] = window_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(*loaded.current_gworld, window_ptr);
    assert_eq!(*loaded.current_gdevice, gdevice);
    assert_eq!(
        ppc_front_visible_window(&mut loaded.memory, &loaded.gworlds),
        Some(window_ptr)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(window_ptr + PPC_CWINDOW_HILITED_OFFSET),
        Some(1)
    );
}

#[test]
fn hle_import_runner_handles_close_window() {
    let pef = synthetic_pef_with_import(b"CloseWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let underlying_window = PPC_DATA_BASE + 0x0800;
    let window_ptr = PPC_DATA_BASE + 0x1000;
    let gdevice = PPC_DATA_BASE + 0x2000;
    loaded
        .memory
        .add_region(underlying_window, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded
        .memory
        .add_region(window_ptr, vec![0; PPC_CGRAF_PORT_SIZE as usize]);
    loaded
        .memory
        .write_u8(underlying_window + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded
        .memory
        .write_u8(window_ptr + PPC_CWINDOW_VISIBLE_OFFSET, 1)
        .unwrap();
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: underlying_window,
        pixmap_handle: PPC_DATA_BASE + 0x0900,
        pixmap: PPC_DATA_BASE + 0x0a00,
        base_addr: PPC_DATA_BASE + 0x4000,
        gdevice,
        width: 320,
        height: 240,
        depth: PPC_MAIN_PIXEL_DEPTH,
        row_bytes: 640,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: window_ptr,
        pixmap_handle: PPC_DATA_BASE + 0x1100,
        pixmap: PPC_DATA_BASE + 0x1200,
        base_addr: PPC_DATA_BASE + 0x3000,
        gdevice,
        width: 320,
        height: 240,
        depth: PPC_MAIN_PIXEL_DEPTH,
        row_bytes: 640,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    loaded.window_list.extend([window_ptr, underlying_window]);
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = window_ptr);
    loaded
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = gdevice);
    loaded.cpu.gpr[3] = window_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded
        .gworlds
        .iter()
        .any(|record| record.port == PPC_MAIN_GWORLD));
    assert!(loaded
        .gworlds
        .iter()
        .any(|record| record.port == PPC_DSP_BACK_GWORLD));
    assert!(!loaded
        .gworlds
        .iter()
        .any(|record| record.port == window_ptr));
    assert_eq!(*loaded.current_gworld, underlying_window);
    assert_eq!(*loaded.current_gdevice, gdevice);
    assert!(loaded
        .event_queue()
        .iter()
        .any(|event| event.what == 6 && event.message == underlying_window));
}

#[test]
fn hle_import_runner_new_cwindow_heap_full_does_not_partially_allocate() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let bounds_ptr = scratch;
    loaded.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 20, 10, 260, 330).unwrap();
    let heap_cursor = loaded.heap_cursor();
    let region_count = loaded.memory.region_count();
    let gworld_count = loaded.gworlds.len();
    let required = ppc_heap_allocation_sequence_size(&[
        PPC_PIXMAP_SIZE,
        4,
        PPC_CGRAF_PORT_SIZE,
        4,
        10,
        4,
        10,
        4,
        10,
    ])
    .unwrap();
    loaded.set_heap_limit(heap_cursor + required - 4);
    assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 2;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.memory.region_count(), region_count);
    assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
    assert_eq!(loaded.gworlds.len(), gworld_count);
    assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
}

#[test]
fn hle_import_runner_new_cwindow_param_err_does_not_partially_allocate() {
    let pef = synthetic_pef_with_import(b"NewCWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let bounds_ptr = scratch;
    loaded.memory.add_region(scratch, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 20, 10, 260, 330).unwrap();
    let heap_cursor = loaded.heap_cursor();
    let region_count = loaded.memory.region_count();
    let gworld_count = loaded.gworlds.len();
    assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
    loaded.cpu.gpr[3] = 0x06ff_0000;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 2;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 1;
    loaded.cpu.gpr[10] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.last_mem_error(), PPC_PARAM_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.memory.region_count(), region_count);
    assert_eq!(loaded.memory.read_u8(heap_cursor), Some(0));
    assert_eq!(loaded.gworlds.len(), gworld_count);
    assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
}

#[test]
fn calc_vis_behind_rebuilds_visible_window_regions_below_the_menu_bar() {
    let pef = synthetic_pef_with_import(b"CalcVisBehind");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 10, 40, 110, 240).unwrap();

    let mut window_cpu = loaded.cpu.clone();
    window_cpu.gpr[3] = 0;
    window_cpu.gpr[4] = bounds_ptr;
    window_cpu.gpr[6] = 1;
    let window = ppc_new_cwindow(
        &window_cpu,
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        &mut loaded.gworlds,
        &mut loaded.window_list,
        *loaded.current_gdevice,
    );
    assert_ne!(window, 0);

    let clobbered_rgn = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    ppc_write_rgn_bbox(&mut loaded.memory, clobbered_rgn, 0, 0, 200, 300).unwrap();
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = clobbered_rgn;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let vis_rgn = loaded.memory.read_u32_be(window + 24).unwrap();
    assert_ne!(vis_rgn, 0);
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, vis_rgn),
        Some((10, 0, 100, 200))
    );
}

#[test]
fn paint_behind_nil_draws_the_classic_desktop_inside_the_clobbered_region() {
    let pef = synthetic_pef_with_import(b"PaintBehind");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let clobbered_rgn = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    ppc_write_rgn_bbox(&mut loaded.memory, clobbered_rgn, 20, 0, 22, 2).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = clobbered_rgn;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 20)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(
            ppc_standard_desktop_color(&loaded.gworlds, 0, 0)
        )))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (1, 20)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(
            ppc_standard_desktop_color(&loaded.gworlds, 1, 0)
        )))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 21)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(
            ppc_standard_desktop_color(&loaded.gworlds, 1, 0)
        )))
    );
}

#[test]
fn paint_behind_nil_preserves_pixels_covered_by_a_visible_window() {
    let pef = synthetic_pef_with_import(b"PaintBehind");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 20, 0, 100, 100).unwrap();
    let mut window_cpu = loaded.cpu.clone();
    window_cpu.gpr[3] = 0;
    window_cpu.gpr[4] = bounds_ptr;
    window_cpu.gpr[6] = 1;
    let window = ppc_new_cwindow(
        &window_cpu,
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        &mut loaded.gworlds,
        &mut loaded.window_list,
        *loaded.current_gdevice,
    );
    assert_ne!(window, 0);
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    ppc_quickdraw_write_pixel(&mut loaded.memory, front, (50, 30), PPC_RGB_BLACK);
    let clobbered_rgn = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    ppc_write_rgn_bbox(&mut loaded.memory, clobbered_rgn, 20, 0, 40, 100).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = clobbered_rgn;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (50, 30)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(PPC_RGB_BLACK)))
    );
}

#[test]
fn paint_one_erases_exposed_content_and_updates_the_window_region() {
    let pef = synthetic_pef_with_import(b"PaintOne");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 30, 40, 130, 240).unwrap();

    let mut window_cpu = loaded.cpu.clone();
    window_cpu.gpr[3] = 0;
    window_cpu.gpr[4] = bounds_ptr;
    window_cpu.gpr[6] = 1;
    let window = ppc_new_cwindow(
        &window_cpu,
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        &mut loaded.gworlds,
        &mut loaded.window_list,
        *loaded.current_gdevice,
    );
    assert_ne!(window, 0);

    let clobbered_rgn = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    ppc_write_rgn_bbox(&mut loaded.memory, clobbered_rgn, 35, 45, 45, 55).unwrap();
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    ppc_quickdraw_write_pixel(&mut loaded.memory, front, (50, 40), PPC_RGB_BLACK);
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = clobbered_rgn;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let update_rgn = loaded
        .memory
        .read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET)
        .unwrap();
    assert_eq!(
        ppc_read_rgn_bbox(&mut loaded.memory, update_rgn),
        Some((35, 45, 45, 55))
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (50, 40)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(PPC_RGB_WHITE)))
    );
}

#[test]
fn paint_one_uses_color_window_content_instead_of_white() {
    let pef = synthetic_pef_with_import(b"PaintOne");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 100, 100).unwrap();
    let mut window_cpu = loaded.cpu.clone();
    window_cpu.gpr[3] = 0;
    window_cpu.gpr[4] = bounds_ptr;
    window_cpu.gpr[6] = 1;
    let window = ppc_new_cwindow(
        &window_cpu,
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        &mut loaded.gworlds,
        &mut loaded.window_list,
        *loaded.current_gdevice,
    );
    assert_ne!(window, 0);
    let table = [
        0, 0, 0, 1, // seed
        0, 0, // flags
        0, 0, // one entry
        0, 0, // wContentColor
        0, 0, 0, 0, 0, 0, // black
    ];
    let table_handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &table,
    );
    loaded.memory.write_u32_be(window + PPC_CWINDOW_COLOR_TABLE_HANDLE_OFFSET, table_handle).unwrap();
    let clobbered_rgn = ppc_new_rgn(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
    );
    ppc_write_rgn_bbox(&mut loaded.memory, clobbered_rgn, 0, 0, 20, 100).unwrap();
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    ppc_quickdraw_write_pixel(&mut loaded.memory, front, (50, 10), PPC_RGB_WHITE);
    loaded.cpu.gpr[3] = window;
    loaded.cpu.gpr[4] = clobbered_rgn;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (50, 10)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(PPC_RGB_BLACK)))
    );
}

#[test]
fn show_window_does_not_read_dialog_edit_state_as_a_color_table() {
    let pef = synthetic_pef_with_import(b"ShowWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut last_mem_error = loaded.last_mem_error();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(bounds_ptr, vec![0; 8]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 100, 100).unwrap();
    let mut window_cpu = loaded.cpu.clone();
    window_cpu.gpr[3] = 0;
    window_cpu.gpr[4] = bounds_ptr;
    window_cpu.gpr[6] = 0;
    window_cpu.gpr[7] = 1;
    let dialog = ppc_new_cwindow(
        &window_cpu,
        None,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        &mut last_mem_error,
        test_handles!(loaded),
        &mut loaded.gworlds,
        &mut loaded.window_list,
        *loaded.current_gdevice,
    );
    assert_ne!(dialog, 0);
    loaded
        .memory
        .write_u16_be(
            dialog + PPC_CWINDOW_WINDOW_KIND_OFFSET,
            crate::dialog_manager::DIALOG_WINDOW_KIND,
        )
        .unwrap();
    let black_table = [
        0, 0, 0, 1, // seed
        0, 0, // flags
        0, 0, // one entry
        0, 0, // wContentColor
        0, 0, 0, 0, 0, 0, // black
    ];
    let edit_state = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &black_table,
    );
    // DialogRecord.editField/editOpen occupy these bytes. If interpreted as
    // a WCTab handle, they would falsely paint the dialog black.
    loaded
        .memory
        .write_u32_be(
            dialog + crate::dialog_manager::DIALOG_EDIT_FIELD_OFFSET,
            edit_state,
        )
        .unwrap();
    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    ppc_quickdraw_write_pixel(&mut loaded.memory, front, (50, 50), PPC_RGB_WHITE);
    loaded.cpu.gpr[3] = dialog;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (50, 50)),
        Some(u16::from(ppc_rgb_color_to_8bpp_index(PPC_RGB_WHITE)))
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(dialog + crate::dialog_manager::DIALOG_EDIT_FIELD_OFFSET),
        Some(edit_state)
    );
}

#[test]
fn legacy_window_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("BringToFront", PpcLegacyWindowOperation::BringToFront),
        ("CalcVis", PpcLegacyWindowOperation::CalculateVisibleRegion),
        ("DisposeWindow", PpcLegacyWindowOperation::DisposeWindow),
        ("DragWindow", PpcLegacyWindowOperation::DragWindow),
        ("GetNewWindow", PpcLegacyWindowOperation::GetNewWindow),
        ("GetWTitle", PpcLegacyWindowOperation::GetWindowTitle),
        ("GrowWindow", PpcLegacyWindowOperation::GrowWindow),
        ("HiliteWindow", PpcLegacyWindowOperation::HighlightWindow),
        ("NewWindow", PpcLegacyWindowOperation::NewWindow),
        ("SendBehind", PpcLegacyWindowOperation::SendBehind),
        ("SetWTitle", PpcLegacyWindowOperation::SetWindowTitle),
        ("TrackBox", PpcLegacyWindowOperation::TrackBox),
        ("TrackGoAway", PpcLegacyWindowOperation::TrackGoAway),
        ("ZoomWindow", PpcLegacyWindowOperation::ZoomWindow),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::LegacyWindow(operation),
        );
    }
}

#[test]
fn import_bindings_classify_window_creation_and_disposal_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // InitWindows / initwindows
        assert_eq!(
            dispatcher_target_for_import(lib, "InitWindows"),
            PpcImportDispatcherTarget::InitWindows
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "initwindows"),
            PpcImportDispatcherTarget::InitWindows
        );

        // NewWindow / newwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "NewWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::NewWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "newwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::NewWindow)
        );

        // NewCWindow / newcwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "NewCWindow"),
            PpcImportDispatcherTarget::NewCWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "newcwindow"),
            PpcImportDispatcherTarget::NewCWindow
        );

        // GetNewWindow / getnewwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "GetNewWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetNewWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getnewwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetNewWindow)
        );

        // GetNewCWindow / getnewcwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "GetNewCWindow"),
            PpcImportDispatcherTarget::GetNewCWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getnewcwindow"),
            PpcImportDispatcherTarget::GetNewCWindow
        );

        // DisposeWindow / disposewindow / DisposWindow / disposwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "DisposeWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "disposewindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "DisposWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "disposwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow)
        );

        // CloseWindow / closewindow
        assert_eq!(
            dispatcher_target_for_import(lib, "CloseWindow"),
            PpcImportDispatcherTarget::CloseWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "closewindow"),
            PpcImportDispatcherTarget::CloseWindow
        );

        // CreateNewWindow / createnewwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "CreateNewWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CreateNewWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "createnewwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CreateNewWindow)
        );
    }
}

#[test]
fn window_creation_and_disposal_commands_dispatch_with_canonical_evaluation() {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        // 1. InitWindows
        {
            let pef = synthetic_pef_with_library_import(lib, b"InitWindows");
            let mut loaded = load_pef_application(&pef).unwrap();
            assert!(!loaded.toolbox_startup.windows_initialized);
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert!(loaded.toolbox_startup.windows_initialized);
        }

        // 2. NewWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"NewWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, bounds_ptr, 20, 20, 120, 220).unwrap();
            loaded.cpu.gpr[3] = 0; // storage
            loaded.cpu.gpr[4] = bounds_ptr;
            loaded.cpu.gpr[5] = 0; // title
            loaded.cpu.gpr[6] = 1; // visible
            loaded.cpu.gpr[7] = 0; // procID
            loaded.cpu.gpr[8] = u32::MAX; // behind
            loaded.cpu.gpr[9] = 1; // goAway
            loaded.cpu.gpr[10] = 0; // refCon
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let window = loaded.cpu.gpr[3];
            assert_ne!(window, 0);
            assert!(loaded.window_list.contains(&window));

            // NewWindow with bounds = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 3. NewCWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"NewCWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, bounds_ptr, 30, 30, 130, 230).unwrap();
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = bounds_ptr;
            loaded.cpu.gpr[5] = 0;
            loaded.cpu.gpr[6] = 1;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = u32::MAX;
            loaded.cpu.gpr[9] = 1;
            loaded.cpu.gpr[10] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let window = loaded.cpu.gpr[3];
            assert_ne!(window, 0);
            assert!(loaded.window_list.contains(&window));

            // NewCWindow with bounds = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 4. GetNewWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetNewWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let current_resource_refnum = *loaded.process_file_system.current_resource_file;
            loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"WIND"),
                res_id: 1100,
                name: Vec::new(),
                data: test_wind_resource((50, 60, 250, 360), 0, true, true, 0x1234_5678, b"Doc"),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
            loaded.cpu.gpr[3] = 1100;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = u32::MAX;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let window = loaded.cpu.gpr[3];
            assert_ne!(window, 0);
            assert!(loaded.window_list.contains(&window));
        }

        // 5. GetNewCWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetNewCWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let current_resource_refnum = *loaded.process_file_system.current_resource_file;
            loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
                ref_num: current_resource_refnum,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"WIND"),
                res_id: 1101,
                name: Vec::new(),
                data: test_wind_resource((60, 70, 260, 370), 0, true, true, 0x5678_1234, b"CDoc"),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
            loaded.cpu.gpr[3] = 1101;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = u32::MAX;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let window = loaded.cpu.gpr[3];
            assert_ne!(window, 0);
            assert!(loaded.window_list.contains(&window));
        }

        // 6. CloseWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"CloseWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 40, 140, 240), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "CloseWindow");
            assert!(loaded.window_list.contains(&window));
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert!(!loaded.window_list.contains(&window));

            // CloseWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 7. DisposeWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"DisposeWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 40, 140, 240), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "DisposeWindow");
            assert!(loaded.window_list.contains(&window));
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert!(!loaded.window_list.contains(&window));

            // DisposeWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 8. CreateNewWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"CreateNewWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let out_window_ptr = PPC_DATA_BASE + 0x1020;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, bounds_ptr, 50, 50, 250, 350).unwrap();
            loaded.memory.add_region(out_window_ptr, vec![0; 4]);
            loaded.cpu.gpr[3] = 6; // kDocumentWindowClass
            loaded.cpu.gpr[4] = 1; // attributes
            loaded.cpu.gpr[5] = bounds_ptr;
            loaded.cpu.gpr[6] = out_window_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0); // noErr
            let created_window = loaded.memory.read_u32_be(out_window_ptr).unwrap();
            assert_ne!(created_window, 0);
            assert!(loaded.window_list.contains(&created_window));

            // CreateNewWindow rejects invalid window_class
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 1;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = bounds_ptr;
            loaded.cpu.gpr[6] = out_window_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i32, -50);

            // CreateNewWindow rejects bounds = 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 6;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = 0;
            loaded.cpu.gpr[6] = out_window_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i32, -50);

            // CreateNewWindow rejects out_window = 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 6;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = bounds_ptr;
            loaded.cpu.gpr[6] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i32, -50);
        }
    }
}

#[test]
fn create_new_plain_window_preserves_bounds_and_suppresses_updates() {
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"CreateNewWindow");
    let mut loaded = load_pef_application(&pef).unwrap();
    let bounds_ptr = PPC_DATA_BASE + 0x1000;
    let out_window_ptr = PPC_DATA_BASE + 0x1020;
    loaded.memory.add_region(bounds_ptr, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 50, 50, 250, 350).unwrap();
    loaded.memory.add_region(out_window_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = 13; // kPlainWindowClass
    loaded.cpu.gpr[4] = 1 << 16; // kWindowNoUpdatesAttribute
    loaded.cpu.gpr[5] = bounds_ptr;
    loaded.cpu.gpr[6] = out_window_ptr;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let window = loaded.memory.read_u32_be(out_window_ptr).unwrap();
    assert_ne!(window, 0);
    assert_eq!(ppc_window_proc_id(&mut loaded.memory, window), 2);
    assert!(loaded.toolbox_startup.windows_without_updates.contains(&window));
    assert_eq!(ppc_window_structure_bounds(2, (50, 50, 250, 350)), (49, 49, 251, 351));
    let mut queue = VecDeque::new();
    queue.push_back(PpcQueuedEvent {
        what: 6,
        message: window,
        when: 0,
        where_v: 0,
        where_h: 0,
        modifiers: 0,
    });
    ppc_suppress_window_updates(
        &mut queue,
        &loaded.toolbox_startup.windows_without_updates,
    );
    assert!(queue.is_empty());
}

#[test]
fn import_bindings_classify_window_visibility_and_activation_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // ShowWindow / showwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "ShowWindow"),
            PpcImportDispatcherTarget::ShowWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "showwindow"),
            PpcImportDispatcherTarget::ShowWindow
        );

        // HideWindow / hidewindow
        assert_eq!(
            dispatcher_target_for_import(lib, "HideWindow"),
            PpcImportDispatcherTarget::HideWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "hidewindow"),
            PpcImportDispatcherTarget::HideWindow
        );

        // ShowHide / showhide
        assert_eq!(
            dispatcher_target_for_import(lib, "ShowHide"),
            PpcImportDispatcherTarget::ShowHide
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "showhide"),
            PpcImportDispatcherTarget::ShowHide
        );

        // SelectWindow / selectwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "SelectWindow"),
            PpcImportDispatcherTarget::SelectWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "selectwindow"),
            PpcImportDispatcherTarget::SelectWindow
        );

        // FrontWindow / frontwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "FrontWindow"),
            PpcImportDispatcherTarget::FrontWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "frontwindow"),
            PpcImportDispatcherTarget::FrontWindow
        );

        // BringToFront / bringtofront
        assert_eq!(
            dispatcher_target_for_import(lib, "BringToFront"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::BringToFront)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "bringtofront"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::BringToFront)
        );

        // SendBehind / sendbehind
        assert_eq!(
            dispatcher_target_for_import(lib, "SendBehind"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SendBehind)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "sendbehind"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SendBehind)
        );

        // HiliteWindow / hilitewindow / HighlightWindow / highlightwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "HiliteWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::HighlightWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "hilitewindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::HighlightWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "HighlightWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::HighlightWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "highlightwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::HighlightWindow)
        );
    }
}

#[test]
fn window_visibility_and_activation_commands_dispatch_with_canonical_evaluation() {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        // 1. ShowWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"ShowWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, false, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "ShowWindow");
            assert!(!ppc_window_is_visible(&mut loaded.memory, window));
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert!(ppc_window_is_visible(&mut loaded.memory, window));

            // ShowWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 2. HideWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"HideWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "HideWindow");
            assert!(ppc_window_is_visible(&mut loaded.memory, window));
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert!(!ppc_window_is_visible(&mut loaded.memory, window));

            // HideWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 3. ShowHide
        {
            let pef = synthetic_pef_with_library_import(lib, b"ShowHide");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "ShowHide");
            assert!(ppc_window_is_visible(&mut loaded.memory, window));

            // Hide with ShowHide(window, 0)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert!(!ppc_window_is_visible(&mut loaded.memory, window));

            // Show with ShowHide(window, 1)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert!(ppc_window_is_visible(&mut loaded.memory, window));

            // ShowHide with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 4. FrontWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"FrontWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let _w1 = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            let bounds2_ptr = PPC_DATA_BASE + 0x1040;
            loaded.memory.add_region(bounds2_ptr, vec![0; 32]);
            let w2 = create_test_cwindow(&mut loaded, bounds2_ptr, (40, 40, 140, 240), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "FrontWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], w2);

            // Blanking window takes precedence
            loaded.draw_sprocket.blanking_window = Some(0x1234);
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0x1234);
        }

        // 5. SelectWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"SelectWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let w1 = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            let bounds2_ptr = PPC_DATA_BASE + 0x1040;
            loaded.memory.add_region(bounds2_ptr, vec![0; 32]);
            let w2 = create_test_cwindow(&mut loaded, bounds2_ptr, (40, 40, 140, 240), 0, true, u32::MAX);
            assert_eq!(ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list), Some(w2));

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SelectWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = w1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list), Some(w1));
            assert_eq!(loaded.memory.read_u8(w1 + PPC_CWINDOW_HILITED_OFFSET), Some(1));

            // SelectWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 6. BringToFront
        {
            let pef = synthetic_pef_with_library_import(lib, b"BringToFront");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let w1 = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            let bounds2_ptr = PPC_DATA_BASE + 0x1040;
            loaded.memory.add_region(bounds2_ptr, vec![0; 32]);
            let w2 = create_test_cwindow(&mut loaded, bounds2_ptr, (40, 40, 140, 240), 0, true, u32::MAX);
            assert_eq!(ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list), Some(w2));

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "BringToFront");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = w1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list), Some(w1));

            // BringToFront with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 7. SendBehind
        {
            let pef = synthetic_pef_with_library_import(lib, b"SendBehind");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let w1 = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            let bounds2_ptr = PPC_DATA_BASE + 0x1040;
            loaded.memory.add_region(bounds2_ptr, vec![0; 32]);
            let w2 = create_test_cwindow(&mut loaded, bounds2_ptr, (40, 40, 140, 240), 0, true, u32::MAX);
            assert_eq!(ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list), Some(w2));

            // Send w2 to the back (behind = 0)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SendBehind");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = w2;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_front_visible_process_window(&mut loaded.memory, &loaded.window_list), Some(w1));

            // SendBehind with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = w1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 8. HiliteWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"HiliteWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "HiliteWindow");
            // Un-highlight window with HiliteWindow(window, 0)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.memory.read_u8(window + PPC_CWINDOW_HILITED_OFFSET), Some(0));

            // Re-highlight window with HiliteWindow(window, 1)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.memory.read_u8(window + PPC_CWINDOW_HILITED_OFFSET), Some(1));

            // HiliteWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }
    }
}

#[test]
fn import_bindings_classify_window_sizing_positioning_and_zooming_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // SizeWindow / sizewindow
        assert_eq!(
            dispatcher_target_for_import(lib, "SizeWindow"),
            PpcImportDispatcherTarget::SizeWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "sizewindow"),
            PpcImportDispatcherTarget::SizeWindow
        );

        // MoveWindow / movewindow
        assert_eq!(
            dispatcher_target_for_import(lib, "MoveWindow"),
            PpcImportDispatcherTarget::MoveWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "movewindow"),
            PpcImportDispatcherTarget::MoveWindow
        );

        // DragWindow / dragwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "DragWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DragWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "dragwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DragWindow)
        );

        // GrowWindow / growwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "GrowWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GrowWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "growwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GrowWindow)
        );

        // RepositionWindow / repositionwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "RepositionWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RepositionWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "repositionwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RepositionWindow)
        );

        // TrackBox / trackbox
        assert_eq!(
            dispatcher_target_for_import(lib, "TrackBox"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::TrackBox)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "trackbox"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::TrackBox)
        );

        // TrackGoAway / trackgoaway
        assert_eq!(
            dispatcher_target_for_import(lib, "TrackGoAway"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::TrackGoAway)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "trackgoaway"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::TrackGoAway)
        );

        // ZoomWindow / zoomwindow
        assert_eq!(
            dispatcher_target_for_import(lib, "ZoomWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ZoomWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "zoomwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ZoomWindow)
        );
    }

    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // CollapseWindow / collapsewindow
        assert_eq!(
            dispatcher_target_for_import(lib, "CollapseWindow"),
            PpcImportDispatcherTarget::CollapseWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "collapsewindow"),
            PpcImportDispatcherTarget::CollapseWindow
        );

        // IsWindowCollapsed / iswindowcollapsed
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowCollapsed"),
            PpcImportDispatcherTarget::IsWindowCollapsed
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowcollapsed"),
            PpcImportDispatcherTarget::IsWindowCollapsed
        );
    }
}

#[test]
fn window_sizing_positioning_and_zooming_commands_dispatch_with_canonical_evaluation() {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        // 1. MoveWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"MoveWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "MoveWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 60; // hGlobal
            loaded.cpu.gpr[5] = 70; // vGlobal
            loaded.cpu.gpr[6] = 1;  // front
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let bounds = ppc_window_global_content_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            assert_eq!(bounds.0, 70);
            assert_eq!(bounds.1, 60);

            // Null window handles gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 2. SizeWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"SizeWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SizeWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 300; // w
            loaded.cpu.gpr[5] = 200; // h
            loaded.cpu.gpr[6] = 1;   // fUpdate
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let bounds = ppc_window_global_content_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            assert_eq!(bounds.3 - bounds.1, 300);
            assert_eq!(bounds.2 - bounds.0, 200);

            // Null window handles gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 3. ZoomWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"ZoomWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (100, 100, 240, 300), 8, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "ZoomWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 8; // inZoomOut
            loaded.cpu.gpr[5] = 1; // front
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(
                ppc_dialog_global_bounds(&mut loaded.memory, &loaded.gworlds, window),
                Some((20, 0, ppc_main_screen_height() as i16, ppc_main_screen_width() as i16)),
            );

            // ZoomWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 8;
            loaded.cpu.gpr[5] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 4. RepositionWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"RepositionWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 240, 350), 0, false, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "RepositionWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = 1; // kWindowCenterOnMainScreen
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0); // noErr
        }

        // 5. TrackGoAway
        {
            let pef = synthetic_pef_with_library_import(lib, b"TrackGoAway");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (50, 50, 200, 300), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "TrackGoAway");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = (60u32 << 16) | 60;
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_button: false,
                mouse_v: 60,
                mouse_h: 60,
                ..PpcInputSnapshot::default()
            });
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // TrackGoAway with window = 0 returns 0 (false)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (60u32 << 16) | 60;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 6. TrackBox
        {
            let pef = synthetic_pef_with_library_import(lib, b"TrackBox");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (50, 50, 200, 300), 8, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "TrackBox");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = (60u32 << 16) | 60;
            loaded.cpu.gpr[5] = 8; // inZoomOut
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_button: false,
                mouse_v: 60,
                mouse_h: 60,
                ..PpcInputSnapshot::default()
            });
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // TrackBox with window = 0 returns 0 (false)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (60u32 << 16) | 60;
            loaded.cpu.gpr[5] = 8;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 7. DragWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"DragWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let bounds_limit_ptr = PPC_DATA_BASE + 0x1030;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(bounds_limit_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, bounds_limit_ptr, 0, 0, 1000, 1000).unwrap();
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (50, 50, 200, 300), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "DragWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = (60u32 << 16) | 100;
            loaded.cpu.gpr[5] = bounds_limit_ptr;
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_button: false,
                mouse_v: 60,
                mouse_h: 100,
                ..PpcInputSnapshot::default()
            });
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // DragWindow with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (60u32 << 16) | 100;
            loaded.cpu.gpr[5] = bounds_limit_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 8. GrowWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"GrowWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let size_rect_ptr = PPC_DATA_BASE + 0x1030;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(size_rect_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, size_rect_ptr, 50, 50, 500, 500).unwrap();
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (50, 50, 200, 300), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GrowWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = (195u32 << 16) | 295;
            loaded.cpu.gpr[5] = size_rect_ptr;
            loaded.set_input_snapshot(PpcInputSnapshot {
                mouse_button: false,
                mouse_v: 195,
                mouse_h: 295,
                ..PpcInputSnapshot::default()
            });
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // GrowWindow with window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (195u32 << 16) | 295;
            loaded.cpu.gpr[5] = size_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }
    }
}

#[test]
fn window_repositioning_methods_and_placement_dispatch_with_canonical_evaluation() {
    for lib in [
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        let lib_str = std::str::from_utf8(lib).unwrap();

        // 1. RepositionWindow across methods 1..=9
        for method in 1..=9 {
            let pef = synthetic_pef_with_library_import(lib, b"RepositionWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let parent_bounds_ptr = PPC_DATA_BASE + 0x1030;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(parent_bounds_ptr, vec![0; 32]);
            let parent_window = create_test_cwindow(&mut loaded, parent_bounds_ptr, (100, 150, 400, 550), 0, true, u32::MAX);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 240, 350), 0, false, parent_window);

            let parent_structure = ppc_window_global_structure_bounds(&mut loaded.memory, &loaded.gworlds, parent_window);
            let initial_content = (40, 50, 240, 350);
            let initial_structure = ppc_window_global_structure_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            let menu_h = i32::from(loaded.memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20));
            let screen_w = ppc_main_screen_width() as i32;
            let screen_h = ppc_main_screen_height() as i32;

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RepositionWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = parent_window;
            loaded.cpu.gpr[5] = method;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1, "lib {lib_str} method {method}");
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0, "RepositionWindow should succeed for method {method}");

            let bounds = ppc_window_global_content_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            let expected = crate::window_manager::evaluate_reposition_window_bounds(
                initial_content,
                initial_structure,
                parent_structure,
                method as u16,
                screen_w,
                screen_h,
                menu_h,
            ).unwrap();
            assert_eq!(bounds, expected, "lib {lib_str} method {method}");
        }

        // 2. RepositionWindow fallback without parent window (parent = 0)
        {
            let pef = synthetic_pef_with_library_import(lib, b"RepositionWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 240, 350), 0, false, u32::MAX);

            let initial_content = (40, 50, 240, 350);
            let initial_structure = ppc_window_global_structure_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            let menu_h = i32::from(loaded.memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20));
            let screen_w = ppc_main_screen_width() as i32;
            let screen_h = ppc_main_screen_height() as i32;

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RepositionWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0; // NULL parent window
            loaded.cpu.gpr[5] = 4; // kWindowCenterOnParentWindow -> falls back to main screen
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
            let bounds = ppc_window_global_content_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            let expected = crate::window_manager::evaluate_reposition_window_bounds(
                initial_content,
                initial_structure,
                None,
                4,
                screen_w,
                screen_h,
                menu_h,
            ).unwrap();
            assert_eq!(bounds, expected);
        }

        // 3. RepositionWindow rejects invalid method
        {
            let pef = synthetic_pef_with_library_import(lib, b"RepositionWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 240, 350), 0, false, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RepositionWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = 12; // invalid method
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i32, -50); // paramErr

            // RepositionWindow rejects window = 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i32, -50); // paramErr
        }

        // 4. CollapseWindow and IsWindowCollapsed dispatch
        {
            let pef = synthetic_pef_with_library_import(lib, b"CollapseWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 240, 350), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "CollapseWindow");
            // CollapseWindow(window, false) -> 0 (noErr)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // CollapseWindow(window, true) -> -4 (unimpErr)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i32, -4);

            // CollapseWindow(0, false) -> -50 (paramErr)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i32, -50);

            // IsWindowCollapsed(window) -> 0
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowCollapsed");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }
    }
}

#[test]
fn import_bindings_classify_window_invalidation_update_and_drawing_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // BeginUpdate / beginupdate
        assert_eq!(
            dispatcher_target_for_import(lib, "BeginUpdate"),
            PpcImportDispatcherTarget::BeginUpdate
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "beginupdate"),
            PpcImportDispatcherTarget::BeginUpdate
        );

        // EndUpdate / endupdate
        assert_eq!(
            dispatcher_target_for_import(lib, "EndUpdate"),
            PpcImportDispatcherTarget::EndUpdate
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "endupdate"),
            PpcImportDispatcherTarget::EndUpdate
        );

        // InvalRect / invalrect
        assert_eq!(
            dispatcher_target_for_import(lib, "InvalRect"),
            PpcImportDispatcherTarget::InvalRect
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "invalrect"),
            PpcImportDispatcherTarget::InvalRect
        );

        // InvalRgn / invalrgn
        assert_eq!(
            dispatcher_target_for_import(lib, "InvalRgn"),
            PpcImportDispatcherTarget::InvalRgn
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "invalrgn"),
            PpcImportDispatcherTarget::InvalRgn
        );

        // ValidRect / validrect
        assert_eq!(
            dispatcher_target_for_import(lib, "ValidRect"),
            PpcImportDispatcherTarget::ValidRect
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "validrect"),
            PpcImportDispatcherTarget::ValidRect
        );

        // ValidRgn / validrgn
        assert_eq!(
            dispatcher_target_for_import(lib, "ValidRgn"),
            PpcImportDispatcherTarget::ValidRgn
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "validrgn"),
            PpcImportDispatcherTarget::ValidRgn
        );

        // DrawGrowIcon / drawgrowicon
        assert_eq!(
            dispatcher_target_for_import(lib, "DrawGrowIcon"),
            PpcImportDispatcherTarget::DrawGrowIcon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "drawgrowicon"),
            PpcImportDispatcherTarget::DrawGrowIcon
        );

        // CheckUpdate / checkupdate
        assert_eq!(
            dispatcher_target_for_import(lib, "CheckUpdate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CheckUpdate)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "checkupdate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::CheckUpdate)
        );

        // PaintOne / paintone
        assert_eq!(
            dispatcher_target_for_import(lib, "PaintOne"),
            PpcImportDispatcherTarget::PaintOne
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "paintone"),
            PpcImportDispatcherTarget::PaintOne
        );

        // PaintBehind / paintbehind
        assert_eq!(
            dispatcher_target_for_import(lib, "PaintBehind"),
            PpcImportDispatcherTarget::PaintBehind
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "paintbehind"),
            PpcImportDispatcherTarget::PaintBehind
        );

        // CalcVis / calcvis
        assert_eq!(
            dispatcher_target_for_import(lib, "CalcVis"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::CalculateVisibleRegion,
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "calcvis"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::CalculateVisibleRegion,
            )
        );

        // CalcVisBehind / calcvisbehind
        assert_eq!(
            dispatcher_target_for_import(lib, "CalcVisBehind"),
            PpcImportDispatcherTarget::CalcVisBehind
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "calcvisbehind"),
            PpcImportDispatcherTarget::CalcVisBehind
        );
    }
}

#[test]
fn window_invalidation_update_and_drawing_commands_dispatch_with_canonical_evaluation() {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        // 1. InvalRect & ValidRect
        {
            let pef = synthetic_pef_with_library_import(lib, b"InvalRect");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let rect_ptr = PPC_DATA_BASE + 0x1030;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(rect_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, rect_ptr, 20, 20, 80, 80).unwrap();
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.current_gworld.with_mut(|cg| *cg = window);
            let update_rgn = loaded.memory.read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET).unwrap();
            ppc_set_empty_rgn(&mut loaded.memory, update_rgn).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "InvalRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((20, 20, 80, 80)));

            // ValidRect removes it
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "ValidRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 0, 0)));

            // Null rect_ptr is handled gracefully
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "InvalRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 2. InvalRgn & ValidRgn
        {
            let pef = synthetic_pef_with_library_import(lib, b"InvalRgn");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let rgn_ptr = PPC_DATA_BASE + 0x1040;
            let rgn_handle = PPC_DATA_BASE + 0x1060;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(rgn_ptr, vec![0; 32]);
            loaded.memory.add_region(rgn_handle, vec![0; 4]);
            loaded.memory.write_u32_be(rgn_handle, rgn_ptr).unwrap();
            ppc_write_rgn_bbox(&mut loaded.memory, rgn_handle, 15, 15, 75, 75).unwrap();

            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.current_gworld.with_mut(|cg| *cg = window);
            let update_rgn = loaded.memory.read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET).unwrap();
            ppc_set_empty_rgn(&mut loaded.memory, update_rgn).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "InvalRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((15, 15, 75, 75)));

            // ValidRgn removes it
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "ValidRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 0, 0)));

            // Null rgn_handle is handled gracefully
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "InvalRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 3. BeginUpdate & EndUpdate
        {
            let pef = synthetic_pef_with_library_import(lib, b"BeginUpdate");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            let update_rgn = loaded.memory.read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET).unwrap();
            ppc_write_rgn_bbox(&mut loaded.memory, update_rgn, 10, 10, 50, 50).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "BeginUpdate");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(*loaded.current_gworld, window);

            // EndUpdate empties update region
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "EndUpdate");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 0, 0)));

            // Null window handles gracefully
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "BeginUpdate");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "EndUpdate");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 4. DrawGrowIcon
        {
            let pef = synthetic_pef_with_library_import(lib, b"DrawGrowIcon");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "DrawGrowIcon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // Null window handles gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 5. CheckUpdate
        {
            let pef = synthetic_pef_with_library_import(lib, b"CheckUpdate");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let event_record_ptr = PPC_DATA_BASE + 0x1040;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(event_record_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);
            let update_rgn = loaded.memory.read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET).unwrap();
            ppc_write_rgn_bbox(&mut loaded.memory, update_rgn, 5, 5, 25, 25).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "CheckUpdate");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = event_record_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);
            assert_eq!(loaded.memory.read_u16_be(event_record_ptr), Some(6));
            assert_eq!(loaded.memory.read_u32_be(event_record_ptr + 2), Some(window));
        }

        // 6. CalcVis & CalcVisBehind
        {
            let pef = synthetic_pef_with_library_import(lib, b"CalcVis");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "CalcVis");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // CalcVis with window = 0 returns gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // CalcVisBehind
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "CalcVisBehind");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 7. PaintOne & PaintBehind
        {
            let pef = synthetic_pef_with_library_import(lib, b"PaintOne");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "PaintOne");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // PaintBehind
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "PaintBehind");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 8. ClipAbove
        {
            let pef = synthetic_pef_with_library_import(lib, b"ClipAbove");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "ClipAbove");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // Null window handles gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }

        // 9. SetWinColor
        {
            let pef = synthetic_pef_with_library_import(lib, b"SetWinColor");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            // Null color table handles gracefully
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SetWinColor");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
        }
    }
}

#[test]
fn get_wmgr_port_writes_main_gworld() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"GetWMgrPort");
    let mut loaded = load_pef_application(&pef).unwrap();
    let port_out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(port_out, vec![0; 8]);

    loaded.imports[0].dispatcher_target = dispatcher_target_for_import("InterfaceLib", "GetWMgrPort");
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = port_out;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(port_out), Some(PPC_MAIN_GWORLD));

    // Null pointer handles gracefully
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
}

#[test]
fn import_bindings_classify_window_property_title_picture_and_low_memory_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // GetWTitle / SetWTitle
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );

        // GetWRefCon / SetWRefCon
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWRefCon"),
            PpcImportDispatcherTarget::GetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwrefcon"),
            PpcImportDispatcherTarget::GetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowRefCon"),
            PpcImportDispatcherTarget::GetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowrefcon"),
            PpcImportDispatcherTarget::GetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWRefCon"),
            PpcImportDispatcherTarget::SetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwrefcon"),
            PpcImportDispatcherTarget::SetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowRefCon"),
            PpcImportDispatcherTarget::SetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowrefcon"),
            PpcImportDispatcherTarget::SetWRefCon
        );

        // GetWindowPic / SetWindowPic
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowPic"),
            PpcImportDispatcherTarget::GetWindowPic
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowpic"),
            PpcImportDispatcherTarget::GetWindowPic
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowPic"),
            PpcImportDispatcherTarget::SetWindowPic
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowpic"),
            PpcImportDispatcherTarget::SetWindowPic
        );

        // SetWinColor / GetAuxWin
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWinColor"),
            PpcImportDispatcherTarget::SetWinColor
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwincolor"),
            PpcImportDispatcherTarget::SetWinColor
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetAuxWin"),
            PpcImportDispatcherTarget::GetAuxWin
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getauxwin"),
            PpcImportDispatcherTarget::GetAuxWin
        );

        // LMGetWindowList / LMSetWindowList
        assert_eq!(
            dispatcher_target_for_import(lib, "LMGetWindowList"),
            PpcImportDispatcherTarget::LMGetWindowList
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "lmgetwindowlist"),
            PpcImportDispatcherTarget::LMGetWindowList
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "LMSetWindowList"),
            PpcImportDispatcherTarget::LMSetWindowList
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "lmsetwindowlist"),
            PpcImportDispatcherTarget::LMSetWindowList
        );
    }
}

#[test]
fn window_property_title_picture_and_low_memory_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        // 1. GetWTitle & SetWTitle
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetWTitle");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let title_in_ptr = PPC_DATA_BASE + 0x1100;
            let title_out_ptr = PPC_DATA_BASE + 0x1200;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(title_in_ptr, vec![0; 256]);
            loaded.memory.add_region(title_out_ptr, vec![0; 256]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            assert!(ppc_write_pstring_bytes(&mut loaded.memory, title_in_ptr, b"Test Window Title"));

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SetWTitle");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = title_in_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetWTitle");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = title_out_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(
                ppc_read_pstring_bytes(&mut loaded.memory, title_out_ptr).as_deref(),
                Some(b"Test Window Title".as_slice())
            );
        }

        // 2. GetWRefCon & SetWRefCon
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetWRefCon");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetWRefCon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SetWRefCon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0x1234_5678;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetWRefCon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
        }

        // 3. SetWindowPic & GetWindowPic
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetWindowPic");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetWindowPic");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SetWindowPic");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0xABCD_EF01;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetWindowPic");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0xABCD_EF01);
        }

        // 4. SetWinColor & GetAuxWin
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetAuxWin");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let aw_out = PPC_DATA_BASE + 0x1300;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(aw_out, vec![0; 16]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            // Fresh window is tracked, GetAuxWin returns TRUE
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetAuxWin");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = aw_out;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);

            // SetWinColor installs color table
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SetWinColor");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0x5555_4444;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // GetAuxWin returns color table
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetAuxWin");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = aw_out;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);
            assert_eq!(loaded.memory.read_u32_be(aw_out), Some(0x5555_4444));

            // Untracked window returns FALSE
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0x9999_8888;
            loaded.cpu.gpr[4] = aw_out;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
            assert_eq!(loaded.memory.read_u32_be(aw_out), Some(0));
        }

        // 5. LMGetWindowList & LMSetWindowList
        {
            let pef = synthetic_pef_with_library_import(lib, b"LMGetWindowList");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds1_ptr = PPC_DATA_BASE + 0x1000;
            let bounds2_ptr = PPC_DATA_BASE + 0x1400;
            loaded.memory.add_region(bounds1_ptr, vec![0; 32]);
            loaded.memory.add_region(bounds2_ptr, vec![0; 32]);
            let window1 = create_test_cwindow(&mut loaded, bounds1_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "LMGetWindowList");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], window1);

            let window2 = create_test_cwindow(&mut loaded, bounds2_ptr, (30, 30, 130, 230), 0, true, u32::MAX);

            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], window2);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "LMSetWindowList");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "LMGetWindowList");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], window1);
        }
    }
}

#[test]
fn import_bindings_classify_window_hit_testing_clipping_and_region_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // FindWindow
        assert_eq!(
            dispatcher_target_for_import(lib, "FindWindow"),
            PpcImportDispatcherTarget::FindWindow
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "findwindow"),
            PpcImportDispatcherTarget::FindWindow
        );

        // PinRect
        assert_eq!(
            dispatcher_target_for_import(lib, "PinRect"),
            PpcImportDispatcherTarget::PinRect
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "pinrect"),
            PpcImportDispatcherTarget::PinRect
        );

        // GetWVariant
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWVariant"),
            PpcImportDispatcherTarget::GetWVariant
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwvariant"),
            PpcImportDispatcherTarget::GetWVariant
        );

        // ClipAbove
        assert_eq!(
            dispatcher_target_for_import(lib, "ClipAbove"),
            PpcImportDispatcherTarget::ClipAbove
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "clipabove"),
            PpcImportDispatcherTarget::ClipAbove
        );

        // SaveOld
        assert_eq!(
            dispatcher_target_for_import(lib, "SaveOld"),
            PpcImportDispatcherTarget::SaveOld
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "saveold"),
            PpcImportDispatcherTarget::SaveOld
        );

        // DrawNew
        assert_eq!(
            dispatcher_target_for_import(lib, "DrawNew"),
            PpcImportDispatcherTarget::DrawNew
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "drawnew"),
            PpcImportDispatcherTarget::DrawNew
        );

        // DragGrayRgn
        assert_eq!(
            dispatcher_target_for_import(lib, "DragGrayRgn"),
            PpcImportDispatcherTarget::DragGrayRgn
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "draggrayrgn"),
            PpcImportDispatcherTarget::DragGrayRgn
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "DragTheRgn"),
            PpcImportDispatcherTarget::DragGrayRgn
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "dragthergn"),
            PpcImportDispatcherTarget::DragGrayRgn
        );
    }
}

#[test]
fn window_hit_testing_clipping_and_region_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        // 1. FindWindow
        {
            let pef = synthetic_pef_with_library_import(lib, b"FindWindow");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            let window_out = PPC_DATA_BASE + 0x1100;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            loaded.memory.add_region(window_out, vec![0xff; 4]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 140, 250), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "FindWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = (80 << 16) | 100;
            loaded.cpu.gpr[4] = window_out;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 3);
            assert_eq!(loaded.memory.read_u32_be(window_out), Some(window));

            // Point outside windows (in desktop area)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = (300 << 16) | 300;
            loaded.cpu.gpr[4] = window_out;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0); // inDesk
            assert_eq!(loaded.memory.read_u32_be(window_out), Some(0));
        }

        // 2. PinRect
        {
            let pef = synthetic_pef_with_library_import(lib, b"PinRect");
            let mut loaded = load_pef_application(&pef).unwrap();
            let rect_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(rect_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, rect_ptr, 20, 30, 100, 200).unwrap();

            // Point inside: returned unchanged
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "PinRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = rect_ptr;
            loaded.cpu.gpr[4] = (50 << 16) | 80;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], (50 << 16) | 80);

            // Point outside top-right: clamped to top (20) and right - 1 (199)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = rect_ptr;
            loaded.cpu.gpr[4] = (10 << 16) | 250;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], (20 << 16) | 199);

            // Point outside bottom-left: clamped to bottom - 1 (99) and left (30)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = rect_ptr;
            loaded.cpu.gpr[4] = (150 << 16) | 5;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], (99 << 16) | 30);

            // Null rect pointer returns point unchanged
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (50 << 16) | 80;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], (50 << 16) | 80);
        }

        // 3. GetWVariant
        {
            let pef = synthetic_pef_with_library_import(lib, b"GetWVariant");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0x0023, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "GetWVariant");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 3);
        }

        // 4. ClipAbove
        {
            let pef = synthetic_pef_with_library_import(lib, b"ClipAbove");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds1_ptr = PPC_DATA_BASE + 0x1000;
            let bounds2_ptr = PPC_DATA_BASE + 0x1100;
            loaded.memory.add_region(bounds1_ptr, vec![0; 32]);
            loaded.memory.add_region(bounds2_ptr, vec![0; 32]);

            let window2 = create_test_cwindow(&mut loaded, bounds2_ptr, (50, 50, 150, 150), 0, true, u32::MAX);
            let window1 = create_test_cwindow(&mut loaded, bounds1_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.current_gworld.with_mut(|cg| *cg = PPC_MAIN_GWORLD);
            let clip_rgn = loaded.memory.read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_CLIP_RGN_OFFSET).unwrap();
            let win1_struct = ppc_window_global_structure_bounds(&mut loaded.memory, &loaded.gworlds, window1).unwrap();
            ppc_write_rgn_bbox(&mut loaded.memory, clip_rgn, win1_struct.0, win1_struct.1, win1_struct.2 + 50, win1_struct.3).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "ClipAbove");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window2;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let updated_bbox = ppc_read_rgn_bbox(&mut loaded.memory, clip_rgn).unwrap();
            assert_eq!(updated_bbox, (win1_struct.2, win1_struct.1, win1_struct.2 + 50, win1_struct.3));
        }

        // 5. SaveOld & DrawNew
        {
            let pef = synthetic_pef_with_library_import(lib, b"SaveOld");
            let mut loaded = load_pef_application(&pef).unwrap();
            let bounds_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(bounds_ptr, vec![0; 32]);
            let window = create_test_cwindow(&mut loaded, bounds_ptr, (20, 20, 120, 220), 0, true, u32::MAX);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "SaveOld");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], window);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "DrawNew");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], window);

            // Null window handled gracefully
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 6. DragGrayRgn
        {
            let pef = synthetic_pef_with_library_import(lib, b"DragGrayRgn");
            let mut loaded = load_pef_application(&pef).unwrap();
            let slop_ptr = PPC_DATA_BASE + 0x1000;
            loaded.memory.add_region(slop_ptr, vec![0; 32]);
            ppc_write_rect(&mut loaded.memory, slop_ptr, 0, 0, 200, 200).unwrap();

            // Mouse inside slop: delta (mouse - start)
            loaded.process_input.set_mouse_state((70, 95), false);
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(std::str::from_utf8(lib).unwrap(), "DragGrayRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (50 << 16) | 60;
            loaded.cpu.gpr[5] = 0;
            loaded.cpu.gpr[6] = slop_ptr;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], (20 << 16) | 35);

            // Mouse outside slop: 0x8000_8000
            loaded.process_input.set_mouse_state((250, 250), false);
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (50 << 16) | 60;
            loaded.cpu.gpr[5] = 0;
            loaded.cpu.gpr[6] = slop_ptr;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0x8000_8000);

            // Null slop_rect_ptr: defaults to in-slop delta
            loaded.process_input.set_mouse_state((70, 95), false);
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = (50 << 16) | 60;
            loaded.cpu.gpr[5] = 0;
            loaded.cpu.gpr[6] = 0;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], (20 << 16) | 35);
        }
    }
}

#[test]
fn import_bindings_classify_window_default_cancel_button_focus_and_region_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // SetWindowDefaultButton
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowDefaultButton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowDefaultButton)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowdefaultbutton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowDefaultButton)
        );

        // GetWindowDefaultButton
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowDefaultButton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowDefaultButton)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowdefaultbutton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowDefaultButton)
        );

        // SetWindowCancelButton
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowCancelButton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowCancelButton)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowcancelbutton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowCancelButton)
        );

        // GetWindowCancelButton
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowCancelButton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowCancelButton)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowcancelbutton"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowCancelButton)
        );

        // SetUserFocusWindow
        assert_eq!(
            dispatcher_target_for_import(lib, "SetUserFocusWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetUserFocusWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setuserfocuswindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetUserFocusWindow)
        );

        // GetUserFocusWindow
        assert_eq!(
            dispatcher_target_for_import(lib, "GetUserFocusWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetUserFocusWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getuserfocuswindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetUserFocusWindow)
        );

        // GetWindowFromPort
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowFromPort"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFromPort)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowfromport"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFromPort)
        );

        // GetWindowRegion
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowRegion"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowRegion)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowregion"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowRegion)
        );

        // GetWindowFeatures
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowFeatures"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFeatures)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowfeatures"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFeatures)
        );

        // GetWindowStructureWidths
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowStructureWidths"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStructureWidths)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowstructurewidths"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStructureWidths)
        );
    }
}

#[test]
fn window_default_cancel_button_focus_and_region_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"SetWindowDefaultButton");
        let mut loaded = load_pef_application(&pef).unwrap();
        let bounds_ptr = PPC_DATA_BASE + 0x1000;
        let scratch_ptr = PPC_DATA_BASE + 0x1200;
        let rgn_ptr = PPC_DATA_BASE + 0x1300;
        let rgn_handle = PPC_DATA_BASE + 0x1340;
        loaded.memory.add_region(bounds_ptr, vec![0; 64]);
        loaded.memory.add_region(scratch_ptr, vec![0; 64]);
        loaded.memory.add_region(rgn_ptr, vec![0; 32]);
        loaded.memory.add_region(rgn_handle, vec![0; 4]);
        loaded.memory.write_u32_be(rgn_handle, rgn_ptr).unwrap();

        let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 140, 250), 0, true, u32::MAX);
        let control_1 = 0x2000_1000;
        let control_2 = 0x2000_2000;

        // 1. SetWindowDefaultButton & GetWindowDefaultButton
        {
            // Initial default button is 0
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowDefaultButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(0));

            // Set default button to control_1
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowDefaultButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = control_1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // Get default button returns control_1
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowDefaultButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(control_1));

            // Passing window = 0 returns paramErr (-50)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowDefaultButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = control_1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 2. SetWindowCancelButton & GetWindowCancelButton
        {
            // Initial cancel button is 0
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowCancelButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(0));

            // Set cancel button to control_2
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowCancelButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = control_2;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // Get cancel button returns control_2
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowCancelButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(control_2));

            // Clear cancel button with 0
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowCancelButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowCancelButton");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(0));
        }

        // 3. SetUserFocusWindow & GetUserFocusWindow
        {
            // Initial user focus window falls back to front window
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetUserFocusWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], window);

            // Set explicit user focus window
            let custom_focus = 0x3000_1000;
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetUserFocusWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = custom_focus;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // GetUserFocusWindow returns explicit focus
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetUserFocusWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], custom_focus);
        }

        // 4. GetWindowFromPort
        {
            // Valid window port returns itself
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowFromPort");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], window);

            // Null port returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 5. GetWindowRegion
        {
            // Structure region (kWindowStructureRgn = 32)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowRegion");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 32;
            loaded.cpu.gpr[5] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            let struct_bbox = ppc_read_rgn_bbox(&mut loaded.memory, rgn_handle).unwrap();
            let expected_struct = ppc_window_global_structure_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            assert_eq!(struct_bbox, expected_struct);

            // Content region (kWindowContentRgn = 33)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            let content_bbox = ppc_read_rgn_bbox(&mut loaded.memory, rgn_handle).unwrap();
            let expected_content = ppc_window_global_content_bounds(&mut loaded.memory, &loaded.gworlds, window).unwrap();
            assert_eq!(content_bbox, expected_content);

            // Invalid window = 0 returns paramErr (-50)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 32;
            loaded.cpu.gpr[5] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);

            // Invalid rgn_handle = 0 returns paramErr (-50)
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 32;
            loaded.cpu.gpr[5] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 6. GetWindowFeatures
        {
            let feat_ptr = scratch_ptr;
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowFeatures");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = feat_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(feat_ptr), Some(0x0000_0007));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = feat_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 7. GetWindowStructureWidths
        {
            let rect_ptr = scratch_ptr;
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowStructureWidths");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rect(&mut loaded.memory, rect_ptr), Some((20, 1, 1, 1)));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }
    }
}

#[test]
fn import_bindings_classify_window_proxy_icon_modified_and_state_rect_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // IsWindowHilited
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowHilited"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowHilited)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowhilited"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowHilited)
        );

        // IsWindowVisible
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowVisible"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowVisible)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowvisible"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowVisible)
        );

        // IsWindowModified
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowModified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowModified)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowmodified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowModified)
        );

        // SetWindowModified
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowModified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModified)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowmodified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModified)
        );

        // GetWindowProxyIcon
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowProxyIcon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProxyIcon)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowproxyicon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProxyIcon)
        );

        // SetWindowProxyIcon
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowProxyIcon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProxyIcon)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowproxyicon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProxyIcon)
        );

        // RemoveWindowProxy
        assert_eq!(
            dispatcher_target_for_import(lib, "RemoveWindowProxy"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RemoveWindowProxy)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "removewindowproxy"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RemoveWindowProxy)
        );

        // GetWindowIdealUserState
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowIdealUserState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowIdealUserState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowidealuserstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowIdealUserState)
        );

        // SetWindowIdealUserState
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowIdealUserState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowIdealUserState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowidealuserstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowIdealUserState)
        );

        // GetWindowStandardState
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowStandardState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStandardState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowstandardstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStandardState)
        );

        // SetWindowStandardState
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowStandardState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowStandardState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowstandardstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowStandardState)
        );

        // IsWindowPathSelectClick
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowPathSelectClick"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowPathSelectClick)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowpathselectclick"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowPathSelectClick)
        );
    }
}

#[test]
fn window_proxy_icon_modified_and_state_rect_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"IsWindowHilited");
        let mut loaded = load_pef_application(&pef).unwrap();
        let bounds_ptr = PPC_DATA_BASE + 0x1000;
        let scratch_ptr = PPC_DATA_BASE + 0x1200;
        let in_rect_ptr = PPC_DATA_BASE + 0x1300;
        loaded.memory.add_region(bounds_ptr, vec![0; 64]);
        loaded.memory.add_region(scratch_ptr, vec![0; 64]);
        loaded.memory.add_region(in_rect_ptr, vec![0; 64]);

        let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 140, 250), 0, true, u32::MAX);

        // 1. IsWindowHilited & IsWindowVisible
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowHilited");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);

            loaded.memory.write_u8(window + PPC_CWINDOW_HILITED_OFFSET, 0).unwrap();
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // IsWindowVisible
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowVisible");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);

            loaded.memory.write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 0).unwrap();
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
            loaded.memory.write_u8(window + PPC_CWINDOW_VISIBLE_OFFSET, 1).unwrap();

            // window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 2. IsWindowModified & SetWindowModified
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowModified");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // SetWindowModified(window, true)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowModified");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // IsWindowModified returns 1
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowModified");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);

            // SetWindowModified(window, false)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowModified");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // IsWindowModified returns 0
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowModified");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // window = 0 returns paramErr
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowModified");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 3. GetWindowProxyIcon, SetWindowProxyIcon, RemoveWindowProxy
        {
            let icon_val = 0x4943_4F4E; // 'ICON'
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowProxyIcon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(0));

            // SetWindowProxyIcon
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowProxyIcon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = icon_val;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // GetWindowProxyIcon returns icon_val
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowProxyIcon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(icon_val));

            // RemoveWindowProxy
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RemoveWindowProxy");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // GetWindowProxyIcon returns 0 again
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowProxyIcon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(0));

            // window = 0 returns paramErr
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowProxyIcon");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = icon_val;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 4. GetWindowIdealUserState & SetWindowIdealUserState
        {
            let custom_user_rect = (50, 60, 250, 360);
            ppc_write_rect(&mut loaded.memory, in_rect_ptr, custom_user_rect.0, custom_user_rect.1, custom_user_rect.2, custom_user_rect.3).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowIdealUserState");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = in_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowIdealUserState");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rect(&mut loaded.memory, scratch_ptr), Some(custom_user_rect));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 5. GetWindowStandardState & SetWindowStandardState
        {
            let custom_std_rect = (30, 20, 500, 700);
            ppc_write_rect(&mut loaded.memory, in_rect_ptr, custom_std_rect.0, custom_std_rect.1, custom_std_rect.2, custom_std_rect.3).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowStandardState");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = in_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowStandardState");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rect(&mut loaded.memory, scratch_ptr), Some(custom_std_rect));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 6. IsWindowPathSelectClick
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowPathSelectClick");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }
    }
}

#[test]
fn import_bindings_classify_window_invalidation_geometry_and_attribute_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // InvalWindowRect
        assert_eq!(
            dispatcher_target_for_import(lib, "InvalWindowRect"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::InvalWindowRect)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "invalwindowrect"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::InvalWindowRect)
        );

        // InvalWindowRgn
        assert_eq!(
            dispatcher_target_for_import(lib, "InvalWindowRgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::InvalWindowRgn)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "invalwindowrgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::InvalWindowRgn)
        );

        // ValidWindowRect
        assert_eq!(
            dispatcher_target_for_import(lib, "ValidWindowRect"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ValidWindowRect)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "validwindowrect"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ValidWindowRect)
        );

        // ValidWindowRgn
        assert_eq!(
            dispatcher_target_for_import(lib, "ValidWindowRgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ValidWindowRgn)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "validwindowrgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ValidWindowRgn)
        );

        // GetWindowPortBounds
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowPortBounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowPortBounds)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowportbounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowPortBounds)
        );

        // GetWindowBounds
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowBounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowBounds)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowbounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowBounds)
        );

        // SetWindowBounds
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowBounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowBounds)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowbounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowBounds)
        );

        // GetWindowGreatestArea
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowGreatestArea"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowGreatestArea)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowgreatestarea"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowGreatestArea)
        );

        // GetWindowAttributes
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowAttributes"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowAttributes)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowattributes"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowAttributes)
        );

        // ChangeWindowAttributes
        assert_eq!(
            dispatcher_target_for_import(lib, "ChangeWindowAttributes"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ChangeWindowAttributes)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "changewindowattributes"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ChangeWindowAttributes)
        );

        // ReshapeCustomWindow
        assert_eq!(
            dispatcher_target_for_import(lib, "ReshapeCustomWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ReshapeCustomWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "reshapecustomwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ReshapeCustomWindow)
        );
    }
}

#[test]
fn window_invalidation_geometry_and_attribute_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"InvalWindowRect");
        let mut loaded = load_pef_application(&pef).unwrap();
        let bounds_ptr = PPC_DATA_BASE + 0x1000;
        let scratch_ptr = PPC_DATA_BASE + 0x1200;
        let in_rect_ptr = PPC_DATA_BASE + 0x1300;
        let rgn_ptr = PPC_DATA_BASE + 0x1400;
        let rgn_handle = PPC_DATA_BASE + 0x1420;
        loaded.memory.add_region(bounds_ptr, vec![0; 64]);
        loaded.memory.add_region(scratch_ptr, vec![0; 64]);
        loaded.memory.add_region(in_rect_ptr, vec![0; 64]);
        loaded.memory.add_region(rgn_ptr, vec![0; 64]);
        loaded.memory.add_region(rgn_handle, vec![0; 4]);
        loaded.memory.write_u32_be(rgn_handle, rgn_ptr).unwrap();

        let window = create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 140, 250), 0, true, u32::MAX);
        let update_rgn = loaded.memory.read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET).unwrap();

        // 1. InvalWindowRect & ValidWindowRect
        {
            ppc_set_empty_rgn(&mut loaded.memory, update_rgn).unwrap();
            ppc_write_rect(&mut loaded.memory, in_rect_ptr, 10, 10, 50, 60).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "InvalWindowRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = in_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((10, 10, 50, 60)));

            // ValidWindowRect clears it
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ValidWindowRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = in_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 0, 0)));

            // in_rect = 0 invalidates whole portRect
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "InvalWindowRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 100, 200)));

            // in_rect = 0 validates whole portRect
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ValidWindowRect");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 0, 0)));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = in_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 2. InvalWindowRgn & ValidWindowRgn
        {
            ppc_set_empty_rgn(&mut loaded.memory, update_rgn).unwrap();
            ppc_write_rgn_bbox(&mut loaded.memory, rgn_handle, 15, 20, 65, 80).unwrap();

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "InvalWindowRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((15, 20, 65, 80)));

            // ValidWindowRgn clears it
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ValidWindowRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 0, 0)));

            // rgn_handle = 0 invalidates whole portRect
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "InvalWindowRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 100, 200)));

            // rgn_handle = 0 validates whole portRect
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ValidWindowRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rgn_bbox(&mut loaded.memory, update_rgn), Some((0, 0, 0, 0)));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = rgn_handle;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 3. GetWindowPortBounds
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowPortBounds");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], scratch_ptr);
            assert_eq!(ppc_read_rect(&mut loaded.memory, scratch_ptr), Some((0, 0, 100, 200)));

            // window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 4. GetWindowBounds & SetWindowBounds
        {
            // kWindowContentRgn = 33
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowBounds");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rect(&mut loaded.memory, scratch_ptr), Some((40, 50, 140, 250)));

            // SetWindowBounds
            let new_geom = (60, 70, 180, 290);
            ppc_write_rect(&mut loaded.memory, in_rect_ptr, new_geom.0, new_geom.1, new_geom.2, new_geom.3).unwrap();
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowBounds");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = in_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // Verify with GetWindowBounds
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowBounds");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rect(&mut loaded.memory, scratch_ptr), Some(new_geom));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);

            // out_rect = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);

            // SetWindowBounds in_rect = 0 returns paramErr
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowBounds");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);

            // SetWindowBounds window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = 33;
            loaded.cpu.gpr[5] = in_rect_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 5. GetWindowGreatestArea
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowGreatestArea");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rect(&mut loaded.memory, scratch_ptr), Some((0, 0, 480, 640)));

            // scratch_ptr = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 6. GetWindowAttributes & ChangeWindowAttributes
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowAttributes");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(scratch_ptr), Some(0x0000_0007));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);

            // ChangeWindowAttributes
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ChangeWindowAttributes");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            loaded.cpu.gpr[5] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 7. ReshapeCustomWindow
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ReshapeCustomWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }
    }
}

#[test]
fn import_bindings_classify_window_user_state_flags_and_kind_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // GetWindowDefProc
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowDefProc"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowDefProc)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowdefproc"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowDefProc)
        );

        // GetWindowGoAwayFlag
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowGoAwayFlag"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowGoAwayFlag)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowgoawayflag"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowGoAwayFlag)
        );

        // GetWindowKind
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowKind"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowKind)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowkind"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowKind)
        );

        // GetWindowSpareFlag
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowSpareFlag"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowSpareFlag)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowspareflag"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowSpareFlag)
        );

        // GetWindowUserState
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowUserState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowUserState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowuserstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowUserState)
        );

        // SetWindowKind
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowKind"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowKind)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowkind"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowKind)
        );

        // SetWindowUserState
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowUserState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowUserState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowuserstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowUserState)
        );
    }
}

#[test]
fn window_user_state_flags_and_kind_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"GetWindowKind");
        let mut loaded = load_pef_application(&pef).unwrap();
        let bounds_ptr = PPC_DATA_BASE + 0x1000;
        let scratch_ptr = PPC_DATA_BASE + 0x1100;
        let window_storage = PPC_DATA_BASE + 0x2000;
        loaded.memory.add_region(PPC_DATA_BASE + 0x1000, vec![0; 0x4000]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr, 40, 50, 240, 350).unwrap();

        // Create window
        let title_bytes = b"\x07TestWin";
        let title_ptr = PPC_DATA_BASE + 0x1200;
        loaded.memory.write_bytes(title_ptr, title_bytes).unwrap();
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "NewWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window_storage;
        loaded.cpu.gpr[4] = bounds_ptr;
        loaded.cpu.gpr[5] = title_ptr;
        loaded.cpu.gpr[6] = 1; // visible
        loaded.cpu.gpr[7] = 8; // zoomDocProc
        loaded.cpu.gpr[8] = 0xFFFF_FFFF;
        loaded.cpu.gpr[9] = 1; // goAwayFlag = true
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let window = loaded.cpu.gpr[3];
        assert_ne!(window, 0);

        // 1. GetWindowKind & SetWindowKind
        {
            // Default windowKind for new window is 8 (userKind)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowKind");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 8);

            // SetWindowKind to 2 (dialogKind)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowKind");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 2;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);

            // Verify with GetWindowKind
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowKind");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 2);

            // window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        }

        // 2. GetWindowGoAwayFlag
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowGoAwayFlag");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);

            // window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 3. GetWindowSpareFlag
        {
            // Default spareFlag is 0
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowSpareFlag");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // Write 1 to spareFlag byte (offset 113)
            let _ = loaded.memory.write_u8(window + 113, 1);
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 1);

            // window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 4. GetWindowUserState & SetWindowUserState
        {
            // SetWindowUserState
            let user_rect = (50, 60, 250, 360);
            ppc_write_rect(&mut loaded.memory, scratch_ptr, 50, 60, 250, 360).unwrap();
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowUserState");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = scratch_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // Read back with GetWindowUserState
            let out_ptr = PPC_DATA_BASE + 0x1300;
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowUserState");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = out_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(ppc_read_rect(&mut loaded.memory, out_ptr), Some(user_rect));

            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = out_ptr;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);

            // out_ptr = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            loaded.cpu.gpr[4] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }

        // 5. GetWindowDefProc
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowDefProc");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = window;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            // Default defProc handle
            let def_proc = loaded.memory.read_u32_be(window + 126).unwrap_or(0);
            assert_eq!(loaded.cpu.gpr[3], def_proc);

            // window = 0 returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }
    }
}

#[test]
fn import_bindings_classify_window_modality_activation_and_chain_traversal_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // ActiveNonFloatingWindow
        assert_eq!(
            dispatcher_target_for_import(lib, "ActiveNonFloatingWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ActiveNonFloatingWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "activenonfloatingwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ActiveNonFloatingWindow)
        );

        // GetNextWindow
        assert_eq!(
            dispatcher_target_for_import(lib, "GetNextWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetNextWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getnextwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetNextWindow)
        );

        // GetPreviousWindow
        assert_eq!(
            dispatcher_target_for_import(lib, "GetPreviousWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetPreviousWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getpreviouswindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetPreviousWindow)
        );

        // GetWindowContentRgn
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowContentRgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowContentRgn)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowcontentrgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowContentRgn)
        );

        // GetWindowModality
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowModality"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowModality)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowmodality"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowModality)
        );

        // GetWindowStructureRgn
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowStructureRgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStructureRgn)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowstructurergn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStructureRgn)
        );

        // GetWindowUpdateRgn
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowUpdateRgn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowUpdateRgn)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowupdatergn"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowUpdateRgn)
        );

        // IsWindowActive
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowActive"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowActive)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowactive"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowActive)
        );

        // IsWindowUpdatePending
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowUpdatePending"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowUpdatePending)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowupdatepending"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowUpdatePending)
        );

        // SetWindowModality
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowModality"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModality)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowmodality"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModality)
        );
    }
}

#[test]
fn window_modality_activation_and_chain_traversal_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"GetNextWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        let bounds_ptr1 = PPC_DATA_BASE + 0x1000;
        let bounds_ptr2 = PPC_DATA_BASE + 0x1050;
        let window_storage1 = PPC_DATA_BASE + 0x2000;
        let window_storage2 = PPC_DATA_BASE + 0x3000;
        let scratch_ptr = PPC_DATA_BASE + 0x1100;
        loaded.memory.add_region(PPC_DATA_BASE + 0x1000, vec![0; 0x5000]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr1, 40, 50, 240, 350).unwrap();
        ppc_write_rect(&mut loaded.memory, bounds_ptr2, 60, 70, 260, 370).unwrap();

        // 1. Create window 1
        let title1 = b"\x04Win1";
        let title_ptr1 = PPC_DATA_BASE + 0x1200;
        loaded.memory.write_bytes(title_ptr1, title1).unwrap();
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "NewWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window_storage1;
        loaded.cpu.gpr[4] = bounds_ptr1;
        loaded.cpu.gpr[5] = title_ptr1;
        loaded.cpu.gpr[6] = 1; // visible
        loaded.cpu.gpr[7] = 0; // documentProc
        loaded.cpu.gpr[8] = 0xFFFF_FFFF;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        let win1 = loaded.cpu.gpr[3];
        assert_ne!(win1, 0);

        // 2. Create window 2 behind window 1
        let title2 = b"\x04Win2";
        let title_ptr2 = PPC_DATA_BASE + 0x1250;
        loaded.memory.write_bytes(title_ptr2, title2).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window_storage2;
        loaded.cpu.gpr[4] = bounds_ptr2;
        loaded.cpu.gpr[5] = title_ptr2;
        loaded.cpu.gpr[6] = 1; // visible
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0; // behind window 1
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        let win2 = loaded.cpu.gpr[3];
        assert_ne!(win2, 0);

        // 3. GetNextWindow
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetNextWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], win2);

            // Win2 is last, so nextWindow is 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win2;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // Null window
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 4. GetPreviousWindow
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetPreviousWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win2;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], win1);

            // Win1 is front, so previous is 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);

            // Null window
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 5. IsWindowActive
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowActive");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 1);

            // Null window
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 6. ActiveNonFloatingWindow
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ActiveNonFloatingWindow");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], win1);
        }

        // 7. GetWindowStructureRgn, GetWindowContentRgn, GetWindowUpdateRgn
        {
            let struc_rgn = loaded.memory.read_u32_be(win1 + 114).unwrap_or(0);
            let cont_rgn = loaded.memory.read_u32_be(win1 + 118).unwrap_or(0);
            let update_rgn = loaded.memory.read_u32_be(win1 + 122).unwrap_or(0);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowStructureRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], struc_rgn);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowContentRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], cont_rgn);

            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowUpdateRgn");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], update_rgn);

            // Null window returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 8. IsWindowUpdatePending
        {
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "IsWindowUpdatePending");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            // Initially update region has empty bounds
            assert_eq!(loaded.cpu.gpr[3], 0);

            // Populate update region with non-empty bounds
            let update_rgn_handle = loaded.memory.read_u32_be(win1 + 122).unwrap();
            let update_rgn_ptr = loaded.memory.read_u32_be(update_rgn_handle).unwrap();
            ppc_write_rect(&mut loaded.memory, update_rgn_ptr + 2, 10, 10, 50, 50).unwrap();

            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 1);

            // Null window returns 0
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0);
        }

        // 9. GetWindowModality and SetWindowModality
        {
            let out_kind = scratch_ptr;
            let out_unavail = scratch_ptr + 4;

            // Initially modality is kWindowModalityNone (0)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowModality");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            loaded.cpu.gpr[4] = out_kind;
            loaded.cpu.gpr[5] = out_unavail;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(out_kind), Some(0));
            assert_eq!(loaded.memory.read_u32_be(out_unavail), Some(0));

            // Set win1 to kWindowModalityAppModal (2)
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowModality");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            loaded.cpu.gpr[4] = 2; // kWindowModalityAppModal
            loaded.cpu.gpr[5] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // Read back win1 modality
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowModality");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            loaded.cpu.gpr[4] = out_kind;
            loaded.cpu.gpr[5] = out_unavail;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(out_kind), Some(2));
            assert_eq!(loaded.memory.read_u32_be(out_unavail), Some(0));

            // Set win2 to kWindowModalityWindowModal (3) targeting win1
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowModality");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win2;
            loaded.cpu.gpr[4] = 3; // kWindowModalityWindowModal
            loaded.cpu.gpr[5] = win1;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);

            // Read back win2 modality
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowModality");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win2;
            loaded.cpu.gpr[4] = out_kind;
            loaded.cpu.gpr[5] = out_unavail;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i16, 0);
            assert_eq!(loaded.memory.read_u32_be(out_kind), Some(3));
            assert_eq!(loaded.memory.read_u32_be(out_unavail), Some(win1));

            // Error cases:
            // window = 0 returns paramErr
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 0;
            loaded.cpu.gpr[4] = out_kind;
            loaded.cpu.gpr[5] = out_unavail;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);

            // SetWindowModality with invalid kind returns paramErr
            loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowModality");
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = win1;
            loaded.cpu.gpr[4] = 99; // invalid modality
            loaded.cpu.gpr[5] = 0;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3] as i16, -50);
        }
    }
}

#[test]
fn import_bindings_classify_window_property_management_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // ChangeWindowPropertyAttributes
        assert_eq!(
            dispatcher_target_for_import(lib, "ChangeWindowPropertyAttributes"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ChangeWindowPropertyAttributes
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "changewindowpropertyattributes"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ChangeWindowPropertyAttributes
            )
        );

        // GetWindowProperty
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowProperty"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowproperty"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProperty)
        );

        // GetWindowPropertyAttributes
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowPropertyAttributes"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowPropertyAttributes
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowpropertyattributes"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowPropertyAttributes
            )
        );

        // GetWindowPropertySize
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowPropertySize"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowPropertySize)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowpropertysize"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowPropertySize)
        );

        // RemoveWindowProperty
        assert_eq!(
            dispatcher_target_for_import(lib, "RemoveWindowProperty"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RemoveWindowProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "removewindowproperty"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RemoveWindowProperty)
        );

        // SetWindowProperty
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowProperty"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProperty)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowproperty"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProperty)
        );
    }
}

#[test]
fn window_property_management_commands_dispatch_with_canonical_evaluation() {
    for lib in [b"InterfaceLib".as_slice(), b"AppearanceLib".as_slice(), b"CarbonLib".as_slice()] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"SetWindowProperty");
        let mut loaded = load_pef_application(&pef).unwrap();
        let bounds_ptr1 = PPC_DATA_BASE + 0x1000;
        let bounds_ptr2 = PPC_DATA_BASE + 0x1050;
        let window_storage1 = PPC_DATA_BASE + 0x2000;
        let window_storage2 = PPC_DATA_BASE + 0x3000;
        let scratch_ptr = PPC_DATA_BASE + 0x1100;
        let prop_data_in_ptr = PPC_DATA_BASE + 0x1200;
        let prop_data_out_ptr = PPC_DATA_BASE + 0x1300;
        let out_size_ptr = scratch_ptr;
        let out_attr_ptr = scratch_ptr + 4;

        loaded.memory.add_region(PPC_DATA_BASE + 0x1000, vec![0; 0x5000]);
        ppc_write_rect(&mut loaded.memory, bounds_ptr1, 40, 50, 240, 350).unwrap();
        ppc_write_rect(&mut loaded.memory, bounds_ptr2, 60, 70, 260, 370).unwrap();

        // 1. Create window 1
        let title1 = b"\x04Win1";
        let title_ptr1 = PPC_DATA_BASE + 0x1400;
        loaded.memory.write_bytes(title_ptr1, title1).unwrap();
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "NewWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window_storage1;
        loaded.cpu.gpr[4] = bounds_ptr1;
        loaded.cpu.gpr[5] = title_ptr1;
        loaded.cpu.gpr[6] = 1; // visible
        loaded.cpu.gpr[7] = 0; // documentProc
        loaded.cpu.gpr[8] = 0xFFFF_FFFF;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        let win1 = loaded.cpu.gpr[3];
        assert_ne!(win1, 0);

        // 2. Create window 2
        let title2 = b"\x04Win2";
        let title_ptr2 = PPC_DATA_BASE + 0x1450;
        loaded.memory.write_bytes(title_ptr2, title2).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = window_storage2;
        loaded.cpu.gpr[4] = bounds_ptr2;
        loaded.cpu.gpr[5] = title_ptr2;
        loaded.cpu.gpr[6] = 1; // visible
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0xFFFF_FFFF;
        loaded.cpu.gpr[9] = 1;
        loaded.cpu.gpr[10] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        let win2 = loaded.cpu.gpr[3];
        assert_ne!(win2, 0);

        let creator_test = 0x5445_5354; // 'TEST'
        let tag1 = 0x5441_4731; // 'TAG1'
        let tag2 = 0x5441_4732; // 'TAG2'

        // 3. Query non-existent property on win1 -> -5604 (errWindowPropertyNotFound)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 16;
        loaded.cpu.gpr[7] = out_size_ptr;
        loaded.cpu.gpr[8] = prop_data_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -5604);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(0));

        // 4. SetWindowProperty(win1, 'TEST', 'TAG1', 4, [0x11, 0x22, 0x33, 0x44])
        loaded.memory.write_bytes(prop_data_in_ptr, &[0x11, 0x22, 0x33, 0x44]).unwrap();
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 4;
        loaded.cpu.gpr[7] = prop_data_in_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // 5. GetWindowPropertySize(win1, 'TEST', 'TAG1', out_size_ptr) -> 4
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowPropertySize");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(4));

        // 6. GetWindowProperty(win1, 'TEST', 'TAG1', 16, out_size_ptr, prop_data_out_ptr) -> size 4, data
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 16;
        loaded.cpu.gpr[7] = out_size_ptr;
        loaded.cpu.gpr[8] = prop_data_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(4));
        let read_bytes = ppc_memory_read_bytes(&mut loaded.memory, prop_data_out_ptr, 4).unwrap();
        assert_eq!(read_bytes, [0x11, 0x22, 0x33, 0x44]);

        // 7. GetWindowProperty with buffer_size = 2 (truncated copy)
        loaded.memory.write_bytes(prop_data_out_ptr, &[0xAA; 4]).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 2; // only 2 bytes buffer
        loaded.cpu.gpr[7] = out_size_ptr;
        loaded.cpu.gpr[8] = prop_data_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(4)); // actual size is still 4
        let read_bytes = ppc_memory_read_bytes(&mut loaded.memory, prop_data_out_ptr, 4).unwrap();
        assert_eq!(read_bytes, [0x11, 0x22, 0xAA, 0xAA]); // only first 2 bytes overwritten

        // 8. Attributes get/change
        // Initial attributes are 0
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowPropertyAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = out_attr_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_attr_ptr), Some(0));

        // Change attributes: set 0x20
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "ChangeWindowPropertyAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 0x20; // to_set
        loaded.cpu.gpr[7] = 0;    // to_clear
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // Verify updated attributes
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowPropertyAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = out_attr_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_attr_ptr), Some(0x20));

        // 9. Add property TAG2 to win1 and property TAG1 to win2
        loaded.memory.write_bytes(prop_data_in_ptr, &[0xDE, 0xAD, 0xBE, 0xEF]).unwrap();
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag2;
        loaded.cpu.gpr[6] = 4;
        loaded.cpu.gpr[7] = prop_data_in_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.memory.write_bytes(prop_data_in_ptr, &[0xCA, 0xFE]).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win2;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 2;
        loaded.cpu.gpr[7] = prop_data_in_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // Verify isolation between win1 and win2
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowPropertySize");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(4));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win2;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(2));

        // 10. RemoveWindowProperty(win1, 'TEST', 'TAG1')
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RemoveWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // Query removed property -> -5604
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 16;
        loaded.cpu.gpr[7] = out_size_ptr;
        loaded.cpu.gpr[8] = prop_data_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -5604);

        // Removing already removed property returns -5604
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RemoveWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -5604);

        // Check that win1's tag2 and win2's tag1 are still present
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowPropertySize");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag2;
        loaded.cpu.gpr[6] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(4));

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win2;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = out_size_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_size_ptr), Some(2));

        // 11. Error handling: window == 0 returns paramErr (-50)
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 4;
        loaded.cpu.gpr[7] = prop_data_in_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        loaded.cpu.gpr[6] = 16;
        loaded.cpu.gpr[7] = out_size_ptr;
        loaded.cpu.gpr[8] = prop_data_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "RemoveWindowProperty");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = creator_test;
        loaded.cpu.gpr[5] = tag1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);
    }
}

#[test]
fn import_bindings_classify_window_pic_refcon_variant_port_and_low_memory_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // GetWindowPic
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowPic"),
            PpcImportDispatcherTarget::GetWindowPic
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowpic"),
            PpcImportDispatcherTarget::GetWindowPic
        );

        // SetWindowPic
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowPic"),
            PpcImportDispatcherTarget::SetWindowPic
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowpic"),
            PpcImportDispatcherTarget::SetWindowPic
        );

        // GetWRefCon
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWRefCon"),
            PpcImportDispatcherTarget::GetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwrefcon"),
            PpcImportDispatcherTarget::GetWRefCon
        );

        // SetWRefCon
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWRefCon"),
            PpcImportDispatcherTarget::SetWRefCon
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwrefcon"),
            PpcImportDispatcherTarget::SetWRefCon
        );

        // GetWVariant & GetWindowVariant
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWVariant"),
            PpcImportDispatcherTarget::GetWVariant
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwvariant"),
            PpcImportDispatcherTarget::GetWVariant
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowVariant"),
            PpcImportDispatcherTarget::GetWVariant
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowvariant"),
            PpcImportDispatcherTarget::GetWVariant
        );

        // GetWindowPort
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowPort"),
            PpcImportDispatcherTarget::GetWindowPort
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowport"),
            PpcImportDispatcherTarget::GetWindowPort
        );

        // SetPortWindowPort
        assert_eq!(
            dispatcher_target_for_import(lib, "SetPortWindowPort"),
            PpcImportDispatcherTarget::SetPortWindowPort
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setportwindowport"),
            PpcImportDispatcherTarget::SetPortWindowPort
        );

        // GetAuxWin
        assert_eq!(
            dispatcher_target_for_import(lib, "GetAuxWin"),
            PpcImportDispatcherTarget::GetAuxWin
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getauxwin"),
            PpcImportDispatcherTarget::GetAuxWin
        );

        // LMGetWindowList
        assert_eq!(
            dispatcher_target_for_import(lib, "LMGetWindowList"),
            PpcImportDispatcherTarget::LMGetWindowList
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "lmgetwindowlist"),
            PpcImportDispatcherTarget::LMGetWindowList
        );

        // LMSetWindowList
        assert_eq!(
            dispatcher_target_for_import(lib, "LMSetWindowList"),
            PpcImportDispatcherTarget::LMSetWindowList
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "lmsetwindowlist"),
            PpcImportDispatcherTarget::LMSetWindowList
        );

        // LMGetAuxWinHead
        assert_eq!(
            dispatcher_target_for_import(lib, "LMGetAuxWinHead"),
            PpcImportDispatcherTarget::LMGetAuxWinHead
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "lmgetauxwinhead"),
            PpcImportDispatcherTarget::LMGetAuxWinHead
        );

        // LMSetAuxWinHead
        assert_eq!(
            dispatcher_target_for_import(lib, "LMSetAuxWinHead"),
            PpcImportDispatcherTarget::LMSetAuxWinHead
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "lmsetauxwinhead"),
            PpcImportDispatcherTarget::LMSetAuxWinHead
        );
    }
}

#[test]
fn window_pic_refcon_variant_port_and_low_memory_commands_dispatch_with_canonical_evaluation() {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"GetWRefCon");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1400;
        let bounds_ptr1 = scratch;
        let bounds_ptr2 = scratch + 0x200;
        let out_ctable_ptr = scratch + 0x400;
        loaded.memory.add_region(scratch, vec![0; 0x1000]);

        let win1 =
            create_test_cwindow(&mut loaded, bounds_ptr1, (40, 50, 140, 250), 0, true, u32::MAX);
        let win2 =
            create_test_cwindow(&mut loaded, bounds_ptr2, (60, 70, 160, 270), 19, true, u32::MAX);

        // 1. GetWindowPort & SetPortWindowPort
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowPort");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], win1);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetPortWindowPort");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        // 2. RefCon: GetWRefCon and SetWRefCon
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWRefCon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "SetWRefCon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = 0x1234_5678;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWRefCon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);

        // RefCon on NULL window
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 3. WindowPic: GetWindowPic and SetWindowPic
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowPic");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowPic");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = 0xCAFE_BABE;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowPic");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0xCAFE_BABE);

        // WindowPic on NULL window
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 4. Variant: GetWVariant & GetWindowVariant
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetWVariant");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win2;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 19 & 0x0F);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowVariant");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win2;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 19 & 0x0F);

        // 5. AuxWin: GetAuxWin
        loaded.imports[0].dispatcher_target = dispatcher_target_for_import(lib_str, "GetAuxWin");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        loaded.cpu.gpr[4] = out_ctable_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 1);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_ctable_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 6. Low-memory: LMGetWindowList and LMSetWindowList
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "LMGetWindowList");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], win2);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "LMSetWindowList");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "LMGetWindowList");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], win1);

        // 7. Low-memory: LMGetAuxWinHead and LMSetAuxWinHead
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "LMGetAuxWinHead");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        let initial_aux_head = loaded.cpu.gpr[3];

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "LMSetAuxWinHead");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0x5566_7788;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "LMGetAuxWinHead");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0x5566_7788);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "LMSetAuxWinHead");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = initial_aux_head;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
    }
}

#[test]
fn import_bindings_classify_window_proxy_icon_modified_features_and_structure_widths_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        // GetWindowFeatures
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowFeatures"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFeatures)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowfeatures"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFeatures)
        );

        // GetWindowStructureWidths
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowStructureWidths"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowStructureWidths
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowstructurewidths"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowStructureWidths
            )
        );

        // IsWindowModified
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowModified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowModified)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowmodified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::IsWindowModified)
        );

        // SetWindowModified
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowModified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModified)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowmodified"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowModified)
        );

        // GetWindowProxyIcon
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowProxyIcon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProxyIcon)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowproxyicon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowProxyIcon)
        );

        // SetWindowProxyIcon
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowProxyIcon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProxyIcon)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowproxyicon"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowProxyIcon)
        );

        // RemoveWindowProxy
        assert_eq!(
            dispatcher_target_for_import(lib, "RemoveWindowProxy"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RemoveWindowProxy)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "removewindowproxy"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::RemoveWindowProxy)
        );

        // IsWindowPathSelectClick
        assert_eq!(
            dispatcher_target_for_import(lib, "IsWindowPathSelectClick"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::IsWindowPathSelectClick
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "iswindowpathselectclick"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::IsWindowPathSelectClick
            )
        );
    }
}

#[test]
fn window_proxy_icon_modified_features_and_structure_widths_commands_dispatch_with_canonical_evaluation(
) {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"GetWindowFeatures");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1400;
        let bounds_ptr = scratch;
        let out_features_ptr = scratch + 0x200;
        let out_rect_ptr = scratch + 0x210;
        let out_icon_ptr = scratch + 0x220;
        let event_ptr = scratch + 0x230;
        loaded.memory.add_region(scratch, vec![0; 0x1000]);

        let win =
            create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 140, 250), 0, true, u32::MAX);

        // 1. GetWindowFeatures
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowFeatures");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_features_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(
            loaded.memory.read_u32_be(out_features_ptr),
            Some(0x0000_0007)
        );

        // GetWindowFeatures on NULL window -> paramErr (-50)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_features_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 2. GetWindowStructureWidths
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowStructureWidths");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(
            ppc_read_rect(&mut loaded.memory, out_rect_ptr),
            Some((20, 1, 1, 1))
        );

        // GetWindowStructureWidths on NULL window -> paramErr (-50)
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 3. IsWindowModified & SetWindowModified
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "IsWindowModified");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowModified");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "IsWindowModified");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 1);

        // Clear modified
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowModified");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "IsWindowModified");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // SetWindowModified on NULL window -> paramErr (-50)
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowModified");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 4. WindowProxyIcon: GetWindowProxyIcon, SetWindowProxyIcon, RemoveWindowProxy
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowProxyIcon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_icon_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_icon_ptr), Some(0));

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowProxyIcon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0xABCD_1234;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowProxyIcon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_icon_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_icon_ptr), Some(0xABCD_1234));

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "RemoveWindowProxy");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowProxyIcon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_icon_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_icon_ptr), Some(0));

        // Proxy icon operations on NULL window -> paramErr (-50)
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowProxyIcon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_icon_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowProxyIcon");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0xABCD_1234;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "RemoveWindowProxy");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 5. IsWindowPathSelectClick
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "IsWindowPathSelectClick");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = event_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
}

#[test]
fn import_bindings_classify_window_attributes_focus_buttons_and_reshape_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowAttributes"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowAttributes)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowattributes"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowAttributes)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "ChangeWindowAttributes"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ChangeWindowAttributes
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "changewindowattributes"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::ChangeWindowAttributes
            )
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "ReshapeCustomWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ReshapeCustomWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "reshapecustomwindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::ReshapeCustomWindow)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowDefaultButton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowDefaultButton
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowdefaultbutton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowDefaultButton
            )
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowDefaultButton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::SetWindowDefaultButton
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowdefaultbutton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::SetWindowDefaultButton
            )
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowCancelButton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowCancelButton
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowcancelbutton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::GetWindowCancelButton
            )
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowCancelButton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::SetWindowCancelButton
            )
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowcancelbutton"),
            PpcImportDispatcherTarget::LegacyWindow(
                PpcLegacyWindowOperation::SetWindowCancelButton
            )
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetUserFocusWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetUserFocusWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getuserfocuswindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetUserFocusWindow)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "SetUserFocusWindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetUserFocusWindow)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setuserfocuswindow"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetUserFocusWindow)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowFromPort"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFromPort)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowfromport"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowFromPort)
        );
    }
}

#[test]
fn window_attributes_focus_buttons_and_reshape_commands_dispatch_with_canonical_evaluation() {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"GetWindowAttributes");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1400;
        let bounds_ptr = scratch;
        let out_ptr = scratch + 0x200;
        let control_1 = 0x3000_1000;
        let control_2 = 0x3000_2000;
        loaded.memory.add_region(scratch, vec![0; 0x1000]);

        let win =
            create_test_cwindow(&mut loaded, bounds_ptr, (40, 50, 140, 250), 0, true, u32::MAX);

        // 1. GetWindowAttributes & ChangeWindowAttributes
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(
            loaded.memory.read_u32_be(out_ptr),
            Some(crate::window_manager::WINDOW_ATTRIBUTES_STANDARD)
        );

        // Error cases for GetWindowAttributes
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // ChangeWindowAttributes: add resizable (0x10), clear close box (0x01)
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "ChangeWindowAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = crate::window_manager::WINDOW_RESIZABLE_ATTRIBUTE;
        loaded.cpu.gpr[5] = crate::window_manager::WINDOW_CLOSE_BOX_ATTRIBUTE;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // Query attributes after mutation
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(
            loaded.memory.read_u32_be(out_ptr),
            Some((0x0007 | 0x0010) & !0x0001)
        );

        // Error case: ChangeWindowAttributes on window = 0
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "ChangeWindowAttributes");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 2. ReshapeCustomWindow
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "ReshapeCustomWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        // Error case: ReshapeCustomWindow on window = 0
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 3. SetWindowDefaultButton & GetWindowDefaultButton
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowDefaultButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ptr), Some(0));

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowDefaultButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = control_1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowDefaultButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ptr), Some(control_1));

        // Error cases for default button
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowDefaultButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = control_1;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowDefaultButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 4. SetWindowCancelButton & GetWindowCancelButton
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowCancelButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ptr), Some(0));

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowCancelButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = control_2;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowCancelButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ptr), Some(control_2));

        // Clear cancel button with 0
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowCancelButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowCancelButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(loaded.memory.read_u32_be(out_ptr), Some(0));

        // Error cases for cancel button
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowCancelButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = control_2;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowCancelButton");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 5. SetUserFocusWindow & GetUserFocusWindow
        loaded.window_list.push(win);
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetUserFocusWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], win);

        let explicit_focus = 0x4000_1000;
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetUserFocusWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = explicit_focus;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetUserFocusWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], explicit_focus);

        // Clear explicit focus with 0
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetUserFocusWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetUserFocusWindow");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], win);

        // 6. GetWindowFromPort
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowFromPort");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], win);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
}

#[test]
fn std_bits_copies_into_the_current_port_like_copy_bits() {
    // StdBits is the CopyBits bottleneck with the current port's bits as
    // destination; its first five arguments omit dstBits.
    fn blit(target: PpcImportDispatcherTarget) -> (Vec<Option<u16>>, [u32; 5]) {
        let std_bits = target == PpcImportDispatcherTarget::StdBits;
        let pef = synthetic_pef_with_import(b"NewCWindow");
        let mut loaded = load_pef_application(&pef).unwrap();
        let bounds_ptr = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(bounds_ptr, vec![0; 32]);
        let window = create_test_cwindow(
            &mut loaded,
            bounds_ptr,
            (100, 100, 260, 300),
            0,
            true,
            u32::MAX,
        );
        let front_buffer = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let bitmap = PPC_DATA_BASE + 0x2000;
        let pixels = bitmap + 64;
        loaded.memory.add_region(bitmap, vec![0; 128]);
        loaded.memory.write_u32_be(bitmap, pixels).unwrap();
        loaded.memory.write_u16_be(bitmap + 4, 8).unwrap();
        ppc_write_rect(&mut loaded.memory, bitmap + 6, 0, 0, 8, 64).unwrap();
        for row in 0..8 {
            loaded
                .memory
                .write_u32_be(pixels + row * 8, 0xf0f0_0ff0)
                .unwrap();
        }
        ppc_write_rect(&mut loaded.memory, bitmap + 20, 0, 0, 8, 64).unwrap();
        ppc_write_rect(&mut loaded.memory, bitmap + 28, 50, 100, 58, 164).unwrap();
        loaded.cpu.gpr[3] = window;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetPort);
        let points: Vec<_> = (0..8)
            .flat_map(|y| (0..40).map(move |x| (200 + x, 150 + y)))
            .collect();
        for &point in &points {
            assert!(ppc_quickdraw_write_raw_pixel(
                &mut loaded.memory,
                front_buffer,
                point,
                0x7b,
            ));
        }
        loaded.cpu.gpr[3] = bitmap;
        if std_bits {
            loaded.cpu.gpr[4] = bitmap + 20;
            loaded.cpu.gpr[5] = bitmap + 28;
            loaded.cpu.gpr[6] = 0;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0x1234_5678;
        } else {
            loaded.cpu.gpr[4] = window + 2;
            loaded.cpu.gpr[5] = bitmap + 20;
            loaded.cpu.gpr[6] = bitmap + 28;
            loaded.cpu.gpr[7] = 0;
            loaded.cpu.gpr[8] = 0;
        }
        let arguments = [
            loaded.cpu.gpr[4],
            loaded.cpu.gpr[5],
            loaded.cpu.gpr[6],
            loaded.cpu.gpr[7],
            loaded.cpu.gpr[8],
        ];
        run_test_import(&mut loaded, target);
        let after = [
            loaded.cpu.gpr[4],
            loaded.cpu.gpr[5],
            loaded.cpu.gpr[6],
            loaded.cpu.gpr[7],
            loaded.cpu.gpr[8],
        ];
        if std_bits {
            assert_eq!(
                after, arguments,
                "StdBits must restore its argument registers"
            );
        }
        let painted = points
            .iter()
            .map(|&point| ppc_quickdraw_read_pixel(&mut loaded.memory, front_buffer, point))
            .collect();
        (painted, arguments)
    }

    let (copy_bits, _) = blit(PpcImportDispatcherTarget::CopyBits);
    let (std_bits, _) = blit(PpcImportDispatcherTarget::StdBits);
    assert!(
        copy_bits.iter().any(|pixel| *pixel != Some(0x7b)),
        "the CopyBits reference must paint the destination"
    );
    assert_eq!(std_bits, copy_bits);
}

#[test]
fn import_bindings_classify_window_state_title_greatest_area_and_port_bounds_imports() {
    for lib in ["InterfaceLib", "AppearanceLib", "CarbonLib"] {
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowIdealUserState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowIdealUserState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowidealuserstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowIdealUserState)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowIdealUserState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowIdealUserState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowidealuserstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowIdealUserState)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowStandardState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStandardState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowstandardstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowStandardState)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowStandardState"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowStandardState)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowstandardstate"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowStandardState)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "GetWTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowTitle)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "SetWindowTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwindowtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "SetWTitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "setwtitle"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::SetWindowTitle)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowGreatestArea"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowGreatestArea)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowgreatestarea"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowGreatestArea)
        );

        assert_eq!(
            dispatcher_target_for_import(lib, "GetWindowPortBounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowPortBounds)
        );
        assert_eq!(
            dispatcher_target_for_import(lib, "getwindowportbounds"),
            PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::GetWindowPortBounds)
        );
    }
}

#[test]
fn window_state_title_greatest_area_and_port_bounds_commands_dispatch_with_canonical_evaluation() {
    for lib in [
        b"InterfaceLib".as_slice(),
        b"AppearanceLib".as_slice(),
        b"CarbonLib".as_slice(),
    ] {
        let lib_str = std::str::from_utf8(lib).unwrap();
        let pef = synthetic_pef_with_library_import(lib, b"GetWindowPortBounds");
        let mut loaded = load_pef_application(&pef).unwrap();
        let scratch = PPC_DATA_BASE + 0x1400;
        let bounds_ptr = scratch;
        let rect_ptr = scratch + 0x100;
        let title_in_ptr = scratch + 0x200;
        let title_out_ptr = scratch + 0x300;
        loaded.memory.add_region(scratch, vec![0; 0x1000]);

        let win = create_test_cwindow(&mut loaded, bounds_ptr, (30, 40, 230, 340), 0, true, u32::MAX);

        // 1. GetWindowPortBounds
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowPortBounds");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], rect_ptr);
        assert_eq!(ppc_read_rect(&mut loaded.memory, rect_ptr), Some((0, 0, 200, 300)));

        // Error cases for GetWindowPortBounds
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);

        // 2. GetWindowGreatestArea
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowGreatestArea");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(ppc_read_rect(&mut loaded.memory, rect_ptr), Some((0, 0, 480, 640)));

        // Error case: out_greatest = 0
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 3. SetWindowTitle & GetWindowTitle
        assert!(ppc_write_pstring_bytes(&mut loaded.memory, title_in_ptr, b"Canonical Title"));
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowTitle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = title_in_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowTitle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = title_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(
            ppc_read_pstring_bytes(&mut loaded.memory, title_out_ptr).as_deref(),
            Some(b"Canonical Title".as_slice())
        );

        // Error cases: SetWindowTitle & GetWindowTitle with window = 0
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowTitle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = title_in_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowTitle");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = title_out_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);

        // 4. SetWindowIdealUserState & GetWindowIdealUserState
        let ideal_rect = (55, 65, 255, 365);
        ppc_write_rect(&mut loaded.memory, rect_ptr, ideal_rect.0, ideal_rect.1, ideal_rect.2, ideal_rect.3).unwrap();
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowIdealUserState");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        let out_rect_ptr = scratch + 0x400;
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowIdealUserState");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(ppc_read_rect(&mut loaded.memory, out_rect_ptr), Some(ideal_rect));

        // Error cases: SetWindowIdealUserState / GetWindowIdealUserState with window = 0
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowIdealUserState");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        // 5. SetWindowStandardState & GetWindowStandardState
        let std_rect = (35, 25, 435, 595);
        ppc_write_rect(&mut loaded.memory, rect_ptr, std_rect.0, std_rect.1, std_rect.2, std_rect.3).unwrap();
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowStandardState");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "GetWindowStandardState");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = win;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, 0);
        assert_eq!(ppc_read_rect(&mut loaded.memory, out_rect_ptr), Some(std_rect));

        // Error cases: SetWindowStandardState / GetWindowStandardState with window = 0
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = out_rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);

        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import(lib_str, "SetWindowStandardState");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 0;
        loaded.cpu.gpr[4] = rect_ptr;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3] as i16, -50);
    }
}
#[test]
fn window_removal_discards_pending_updates_without_consuming_unrelated_events() {
    for operation in [
        PpcImportDispatcherTarget::CloseWindow,
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow),
    ] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_import(b"DisposeWindow")).unwrap();
        let scratch = PPC_DATA_BASE + 0x1400;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        let other = create_test_cwindow(
            &mut loaded,
            scratch,
            (100, 100, 260, 300),
            0,
            true,
            u32::MAX,
        );
        let removed = create_test_cwindow(
            &mut loaded,
            scratch,
            (120, 120, 220, 220),
            0,
            true,
            u32::MAX,
        );
        let event = |what, message| PpcQueuedEvent {
            what,
            message,
            when: 17,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        };
        loaded.set_event_queue([
            event(6, removed),
            event(3, removed),
            event(6, other),
            event(6, removed),
        ]);
        loaded.cpu.gpr[3] = removed;
        run_test_import(&mut loaded, operation);
        assert!(!loaded.window_list.contains(&removed));
        assert!(!loaded
            .event_queue()
            .iter()
            .any(|e| e.what == 6 && e.message == removed));
        assert!(loaded
            .event_queue()
            .iter()
            .any(|e| e.what == 3 && e.message == removed));
        assert!(loaded
            .event_queue()
            .iter()
            .any(|e| e.what == 6 && e.message == other));

        // Exercise the public event poll after removal: it must deliver a
        // surviving window's update, never the disposed window's reference.
        loaded.cpu.gpr[3] = 1 << 6;
        loaded.cpu.gpr[4] = scratch + 32;
        loaded.cpu.gpr[5] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        );
        assert_eq!(loaded.cpu.gpr[3], 1);
        assert_eq!(loaded.memory.read_u16_be(scratch + 32), Some(6));
        assert_eq!(loaded.memory.read_u32_be(scratch + 34), Some(other));
        assert!(loaded
            .event_queue()
            .iter()
            .any(|e| e.what == 3 && e.message == removed));
    }
}

#[test]
fn native_execution_does_not_infer_windows_from_offscreen_gworlds() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 16]);
    ppc_write_rect(&mut loaded.memory, scratch, 0, 0, 16, 16).unwrap();
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewGWorld);
    let port = loaded.memory.read_u32_be(scratch + 8).unwrap();
    assert_ne!(port, 0);
    assert!(loaded.window_list.is_empty());
    let address = port + PPC_CWINDOW_NEXT_WINDOW_OFFSET;
    loaded.memory.write_u32_be(address, 0x5a5a_a5a5).unwrap();

    run_test_import(&mut loaded, PpcImportDispatcherTarget::FrontWindow);

    assert!(loaded.window_list.is_empty());
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(0x09D6), Some(0));
    assert_eq!(loaded.memory.read_u32_be(address), Some(0x5a5a_a5a5));
}
