use super::*;

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
        run_test_import(&mut loaded, PpcImportDispatcherTarget::HideWindow);
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

    loaded.cpu.gpr[3] = storage_ptr;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ShowWindow);
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
