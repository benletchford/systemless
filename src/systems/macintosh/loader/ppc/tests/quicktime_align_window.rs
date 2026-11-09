use super::*;

fn fixture(library: &[u8], left: i16) -> (PpcLoadedApp, u32, u32) {
    let mut app =
        load_pef_application(&synthetic_pef_with_library_import(library, b"AlignWindow")).unwrap();
    let scratch = PPC_DATA_BASE + 0x5000;
    app.memory.add_region(scratch, vec![0; 0x800]);
    let window = create_test_cwindow(
        &mut app,
        scratch,
        (40, left, 140, left + 200),
        0,
        true,
        u32::MAX,
    );
    (app, scratch, window)
}

fn align(app: &mut PpcLoadedApp, window: u32, front: bool, rect: u32, callback: u32) {
    app.cpu.gpr[3..7].copy_from_slice(&[window, u32::from(front), rect, callback]);
    run_test_import(app, PpcImportDispatcherTarget::QtAlignWindow);
    assert_eq!(app.cpu.gpr[3], window); // void return preserves the original window argument
}

#[test]
fn quicktime_align_window_uses_live_depth_and_preserves_size_and_local_coordinates() {
    for library in [
        b"QuickTimeLib".as_slice(),
        b"Apple;Carbon;Multimedia".as_slice(),
    ] {
        for (depth, left, expected) in [
            (1, 51, 48),
            (2, 51, 48),
            (4, 51, 48),
            (8, 51, 52),
            (16, 51, 52),
            (32, 51, 52),
            (8, -5, -4),
            (8, 52, 52),
            (8, -400, -400),
        ] {
            let (mut app, _, window) = fixture(library, left);
            app.memory
                .write_u16_be(PPC_MAIN_PIXMAP + 32, depth)
                .unwrap();
            let local = ppc_read_rect(&mut app.memory, window + 16);
            align(&mut app, window, false, 0, 0);
            assert_eq!(ppc_read_rect(&mut app.memory, window + 16), local);
            assert_eq!(
                ppc_window_global_content_bounds(&mut app.memory, &app.gworlds, window),
                Some((40, expected, 140, expected + 200)),
                "depth={depth},left={left}"
            );
        }
    }
}

#[test]
fn quicktime_align_window_local_rectangle_and_front_match_real_move_window() {
    for front in [false, true] {
        let (mut app, scratch, window) = fixture(b"QuickTimeLib", 51);
        let other = create_test_cwindow(
            &mut app,
            scratch + 64,
            (180, 300, 280, 500),
            0,
            true,
            u32::MAX,
        );
        ppc_set_port_origin(&mut app.memory, window, 7, 9).unwrap();
        let local = ppc_read_rect(&mut app.memory, window + 16);
        let rect = scratch + 128;
        ppc_write_rect(&mut app.memory, rect, 19, 23, 39, 73).unwrap();
        align(&mut app, window, front, rect, 0);
        assert_eq!(ppc_read_rect(&mut app.memory, rect), Some((19, 23, 39, 73)));
        assert_eq!(ppc_read_rect(&mut app.memory, window + 16), local);
        assert_eq!(
            ppc_window_global_content_bounds(&mut app.memory, &app.gworlds, window),
            Some((40, 52, 140, 252))
        );
        assert_eq!(
            ppc_front_visible_process_window(&mut app.memory, &app.window_list),
            Some(if front { window } else { other })
        );
        // Compare full output with a separate normal Window Manager invocation.
        let (mut reference, scratch, ref_window) = fixture(b"QuickTimeLib", 51);
        create_test_cwindow(
            &mut reference,
            scratch + 64,
            (180, 300, 280, 500),
            0,
            true,
            u32::MAX,
        );
        ppc_set_port_origin(&mut reference.memory, ref_window, 7, 9).unwrap();
        reference.cpu.gpr[3..7].copy_from_slice(&[ref_window, 52, 40, u32::from(front)]);
        run_test_import(&mut reference, PpcImportDispatcherTarget::MoveWindow);
        let length = ppc_main_screen_width() * ppc_main_screen_height();
        assert_eq!(
            ppc_memory_read_bytes(&mut app.memory, PPC_MAIN_SCREEN_BASE, length)
                .expect("aligned window framebuffer is readable"),
            ppc_memory_read_bytes(&mut reference.memory, PPC_MAIN_SCREEN_BASE, length)
                .expect("MoveWindow reference framebuffer is readable")
        );
        assert_eq!(app.window_list.windows(), reference.window_list.windows());
    }
}

#[test]
fn quicktime_align_window_native_callback_moves_then_restores_linkage_and_frees_scratch() {
    let (mut app, scratch, window) = fixture(b"QuickTimeLib", 51);
    let record = scratch + 128;
    let vector = scratch + 160;
    let witness = scratch + 192;
    let callback = PPC_CODE_BASE + 0x4000;
    app.memory.write_u32_be(record, vector).unwrap();
    app.memory.write_u32_be(record + 4, 5).unwrap();
    app.memory.write_u32_be(vector, callback).unwrap();
    app.memory.write_u32_be(vector + 4, 0x2345_6000).unwrap();
    let mut code = Vec::new();
    for word in [
        d_form_u(15, 6, 0, (witness >> 16) as u16),
        d_form_u(36, 2, 6, witness as u16), // observe callback TOC
        d_form_u(36, 3, 6, (witness + 4) as u16), // observe owned rectangle address
        d_form_u(40, 5, 3, 2),              // default-aligned left
        x_form(31, 5, 5, 4, 266, false),    // plus refcon
        d_form_u(44, 5, 3, 0),              // custom top (window dimensions must remain unchanged)
        d_form_u(14, 3, 0, 999),
        BLR,
    ] {
        code.extend_from_slice(&word.to_be_bytes());
    }
    app.memory.add_region(callback, code);
    app.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtAlignWindow;
    app.cpu.pc = app.entry_pc;
    app.cpu.lr = app.halt_pc;
    app.cpu.gpr[2] = 0x1234_5678;
    app.cpu.gpr[3..7].copy_from_slice(&[window, 0, 0, record]);
    let sp = app.cpu.gpr[1];
    for _ in 0..256 {
        let probe = app.run_with_hle_imports(5);
        assert_eq!(probe.unsupported_import_index, None);
        if matches!(probe.result, PpcRunResult::Halted { .. }) {
            break;
        }
    }
    assert_eq!(app.cpu.pc, app.halt_pc);
    assert_eq!(app.cpu.gpr[2], 0x1234_5678);
    assert_eq!(app.cpu.gpr[3], window);
    assert_eq!(app.cpu.gpr[1], sp);
    assert_eq!(app.memory.read_u32_be(witness), Some(0x2345_6000));
    assert_eq!(
        ppc_window_global_content_bounds(&mut app.memory, &app.gworlds, window),
        Some((57, 52, 157, 252))
    );
    let temporary = app.memory.read_u32_be(witness + 4).unwrap();
    assert!(!app
        .process_memory_manager
        .0
        .borrow()
        .native_ptr_records()
        .iter()
        .any(|r| r.ptr == temporary));
    assert!(app.toolbox_startup.align_window_frames.is_empty());
}

#[test]
fn quicktime_align_window_bad_inputs_do_not_move_or_activate_the_window() {
    for kind in 0..3 {
        let (mut app, scratch, window) = fixture(b"QuickTimeLib", 51);
        let original = ppc_window_global_content_bounds(&mut app.memory, &app.gworlds, window);
        let order = app.window_list.windows();
        let (bad_window, rect, procedure) = match kind {
            0 => (0xdead_beec, 0, 0),
            1 => (window, 0xffff_fffc, 0),
            _ => {
                app.memory.write_u32_be(scratch + 128, 0).unwrap();
                (window, 0, scratch + 128)
            }
        };
        app.cpu.pc = app.entry_pc;
        app.cpu.lr = app.halt_pc;
        app.imports[0].dispatcher_target = PpcImportDispatcherTarget::QtAlignWindow;
        app.cpu.gpr[3..7].copy_from_slice(&[bad_window, 1, rect, procedure]);
        let probe = app.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_ne!(app.cpu.pc, app.halt_pc);
        assert_eq!(
            ppc_window_global_content_bounds(&mut app.memory, &app.gworlds, window),
            original
        );
        assert_eq!(app.window_list.windows(), order);
        assert!(app.toolbox_startup.align_window_frames.is_empty());
    }
}
