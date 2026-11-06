use super::*;

#[test]
fn hle_drag_flavors_validate_destinations_and_preserve_copy_bounds() {
    let pef = synthetic_pef_with_library_import(b"DragLib", b"NewDrag");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::New)
    );
    loaded.cpu.gpr[3] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::New),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(loaded
        .drag_manager
        .with_ref(|manager| manager.is_pristine()));
    let output = PPC_DATA_BASE + 0x1000;
    let source = PPC_DATA_BASE + 0x1100;
    let destination = PPC_DATA_BASE + 0x1200;
    let size = PPC_DATA_BASE + 0x1300;
    loaded.memory.add_region(output, vec![0; 8]);
    loaded.memory.add_region(source, b"native".to_vec());
    loaded.memory.add_region(destination, vec![0xA5; 8]);
    loaded.memory.add_region(size, vec![0; 4]);
    loaded.cpu.gpr[3] = output;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::New),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    let reference = loaded.memory.read_u32_be(output).unwrap();
    let flavor = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = 42;
    loaded.cpu.gpr[5] = flavor;
    loaded.cpu.gpr[6] = source;
    loaded.cpu.gpr[7] = 6;
    loaded.cpu.gpr[8] = 2;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::AddFlavor),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.memory.write_u32_be(size, 2).unwrap();
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = 42;
    loaded.cpu.gpr[5] = flavor;
    loaded.cpu.gpr[6] = destination;
    loaded.cpu.gpr[7] = size;
    loaded.cpu.gpr[8] = 2;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::GetFlavorData),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(size), Some(2));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, destination, 8),
        Some(vec![b't', b'i', 0xA5, 0xA5, 0xA5, 0xA5, 0xA5, 0xA5])
    );
    loaded.memory.write_u32_be(size, 4).unwrap();
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[6] = u32::MAX - 1;
    loaded.cpu.gpr[8] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::GetFlavorData),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(size), Some(4));
    // Apple Drag.h: a null data pointer places a promise; dataSize is ignored.
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = 43;
    loaded.cpu.gpr[5] = flavor;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = u32::MAX;
    loaded.cpu.gpr[8] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::AddFlavor),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(loaded.drag_manager.with_ref(|manager| manager
        .get(reference)
        .unwrap()
        .flavor(43, flavor)
        .unwrap()
        .data
        .is_none()));
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = 43;
    loaded.cpu.gpr[5] = flavor;
    loaded.cpu.gpr[6] = output;
    loaded.memory.write_u32_be(output, 0xA5A5A5A5).unwrap();
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::GetFlavorSize),
    );
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(crate::drag_manager::CANT_GET_FLAVOR_ERR)
    );
    assert_eq!(loaded.memory.read_u32_be(output), Some(0xA5A5A5A5));
    loaded.cpu.gpr[3] = reference;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::Dispose),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[6] = output;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::GetFlavorSize),
    );
    assert_eq!(
        loaded.cpu.gpr[3],
        ppc_i16_result(crate::drag_manager::BAD_DRAG_REF_ERR)
    );
}

#[test]
fn hle_drag_hilite_clips_boundary_and_restores_pixels() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"DragLib",
        b"ShowDragHilite",
    ))
    .unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRgn);
    let region = loaded.cpu.gpr[3];
    ppc_write_rgn_bbox(&mut loaded.memory, region, 40, 40, 60, 60).unwrap();
    let clip = loaded
        .memory
        .read_u32_be(PPC_MAIN_GWORLD + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
        .unwrap();
    ppc_write_rgn_bbox(&mut loaded.memory, clip, 40, 40, 60, 50).unwrap();
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    for y in 37..63 {
        for x in 37..63 {
            ppc_quickdraw_write_raw_pixel(&mut loaded.memory, surface.front_buffer, (x, y), 55);
        }
    }
    let color = ppc_current_hilite_color(
        &mut loaded.memory,
        PPC_MAIN_GWORLD,
        &loaded.quickdraw_hilite_colors,
    );
    let expected = ppc_quickdraw_surface_color_pixel(&mut loaded.memory, surface, color).unwrap();
    assert_ne!(expected, 55);
    let reference = loaded
        .drag_manager
        .with_mut(|manager| manager.new_drag().unwrap());
    loaded.cpu.gpr[3] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetGWorld);
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = region;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::ShowHilite),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    for x in [41, 42] {
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (x, 50)),
            Some(expected)
        );
    }
    for x in [40, 43, 58] {
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (x, 50)),
            Some(55)
        );
    }
    loaded.cpu.gpr[3] = reference;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::HideHilite),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    for y in 37..63 {
        for x in 37..63 {
            assert_eq!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, surface.front_buffer, (x, y)),
                Some(55)
            );
        }
    }
}

#[test]
fn retained_drag_yields_then_cancels_without_a_receiver() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"TrackDrag")).unwrap();
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewRgn);
    let region = loaded.cpu.gpr[3];
    ppc_write_rgn_bbox(&mut loaded.memory, region, 100, 100, 120, 120).unwrap();
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    for y in 100..150 {
        for x in 100..150 {
            ppc_quickdraw_write_raw_pixel(&mut loaded.memory, front, (x, y), 55);
        }
    }
    let event = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(event, vec![0; 16]);
    loaded.memory.write_u16_be(event + 10, 100).unwrap();
    loaded.memory.write_u16_be(event + 12, 100).unwrap();
    let reference = loaded
        .drag_manager
        .with_mut(|manager| manager.new_drag().unwrap());
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = region;
    let mut events = crate::event_queue::EventQueue::default();
    let clut = loaded.screen_clut.with_ref(|clut| *clut);
    let caller = loaded.cpu.lr;
    let stack_pointer = loaded.cpu.gpr[1];
    let mut input = PpcInputSnapshot {
        mouse_v: 100,
        mouse_h: 100,
        mouse_button: true,
        ..Default::default()
    };
    let action = dispatch_drag::track_drag(
        &mut loaded.cpu,
        &mut loaded.memory,
        &loaded.drag_manager,
        &mut loaded.drag_tracking_stack,
        &loaded.gworlds,
        &loaded.window_list,
        input,
        &mut events,
        &clut,
    );
    assert!(matches!(action, PpcImportAction::Yield(_)));
    assert_eq!(loaded.drag_tracking_stack.len(), 1);
    let black = ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &clut).unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 100)),
        Some(black)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (105, 105)),
        Some(55)
    );
    input.mouse_v = 110;
    input.mouse_h = 120;
    let action = dispatch_drag::track_drag(
        &mut loaded.cpu,
        &mut loaded.memory,
        &loaded.drag_manager,
        &mut loaded.drag_tracking_stack,
        &loaded.gworlds,
        &loaded.window_list,
        input,
        &mut events,
        &clut,
    );
    assert!(matches!(action, PpcImportAction::Yield(_)));
    assert_eq!(
        loaded
            .drag_manager
            .with_ref(|manager| manager.get(reference).unwrap().mouse),
        (110, 120)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 100)),
        Some(55)
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (120, 110)),
        Some(black)
    );
    input.mouse_button = false;
    let action = dispatch_drag::track_drag(
        &mut loaded.cpu,
        &mut loaded.memory,
        &loaded.drag_manager,
        &mut loaded.drag_tracking_stack,
        &loaded.gworlds,
        &loaded.window_list,
        input,
        &mut events,
        &clut,
    );
    assert!(matches!(action, PpcImportAction::Return(value) if value == ppc_i16_result(-128)));
    assert!(loaded.drag_tracking_stack.is_empty());
    for y in 100..150 {
        for x in 100..150 {
            assert_eq!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)),
                Some(55)
            );
        }
    }
    assert_eq!(loaded.cpu.lr, caller);
    assert_eq!(loaded.cpu.gpr[1], stack_pointer);
}

#[test]
fn retained_drag_invokes_native_tracking_and_receive_handlers() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"TrackDrag")).unwrap();
    let bounds = PPC_DATA_BASE + 0x1000;
    let title = PPC_DATA_BASE + 0x1100;
    let event = PPC_DATA_BASE + 0x1200;
    let tracking_output = PPC_DATA_BASE + 0x1300;
    let receive_output = PPC_DATA_BASE + 0x1400;
    let tracking_vector = PPC_DATA_BASE + 0x1500;
    let receive_vector = PPC_DATA_BASE + 0x1600;
    let tracking_code = PPC_CODE_BASE + 0x4000;
    let receive_code = PPC_CODE_BASE + 0x4100;
    loaded.memory.add_region(bounds, vec![0; 8]);
    loaded.memory.add_region(title, vec![1, b'X']);
    loaded.memory.add_region(event, vec![0; 16]);
    loaded.memory.add_region(tracking_output, vec![0; 12]);
    loaded.memory.add_region(receive_output, vec![0; 4]);
    ppc_write_rect(&mut loaded.memory, bounds, 80, 80, 200, 200).unwrap();
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = bounds;
    loaded.cpu.gpr[5] = title;
    loaded.cpu.gpr[6] = 1;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = u32::MAX;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewCWindow);
    let window = loaded.cpu.gpr[3];
    assert_ne!(window, 0);
    // Native callbacks store their documented argument registers, then return noErr.
    let tracking_words = [
        0x90650000u32,
        0x90850004,
        0x90C50008,
        0x38600000,
        0x4E800020,
    ];
    let receive_words = [0x90A40000u32, 0x38600000, 0x4E800020];
    loaded.memory.add_region(
        tracking_code,
        tracking_words
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(
        receive_code,
        receive_words
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(
        tracking_vector,
        [tracking_code, PPC_DATA_BASE]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(
        receive_vector,
        [receive_code, PPC_DATA_BASE]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    for (operation, vector, output) in [
        (
            PpcDragOperation::InstallTrackingHandler,
            tracking_vector,
            tracking_output,
        ),
        (
            PpcDragOperation::InstallReceiveHandler,
            receive_vector,
            receive_output,
        ),
    ] {
        loaded.cpu.gpr[3] = vector;
        loaded.cpu.gpr[4] = window;
        loaded.cpu.gpr[5] = output;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::Drag(operation));
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    loaded.memory.write_u16_be(event + 10, 100).unwrap();
    loaded.memory.write_u16_be(event + 12, 100).unwrap();
    let reference = loaded
        .drag_manager
        .with_mut(|manager| manager.new_drag().unwrap());
    loaded.cpu.gpr[3] = reference;
    loaded.cpu.gpr[4] = event;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::Drag(PpcDragOperation::Track);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 100,
        mouse_h: 100,
        mouse_button: true,
        ..Default::default()
    });
    let probe = loaded.run_with_hle_imports(1024);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.drag_tracking_stack.len(), 1);
    assert_eq!(loaded.memory.read_u32_be(tracking_output), Some(3));
    assert_eq!(loaded.memory.read_u32_be(tracking_output + 4), Some(window));
    assert_eq!(
        loaded.memory.read_u32_be(tracking_output + 8),
        Some(reference)
    );
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 110,
        mouse_h: 120,
        mouse_button: false,
        ..Default::default()
    });
    let probe = loaded.run_with_hle_imports(1024);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.drag_tracking_stack.is_empty());
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(receive_output), Some(reference));
    assert_eq!(loaded.memory.read_u32_be(tracking_output), Some(5));
}

#[test]
fn retained_drag_image_owns_masked_pixels_blends_moves_and_restores_background() {
    for (flags, component) in [(0u32, 42597u16), (1, 32767), (2, 16383), (3, 0)] {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            b"DragLib",
            b"SetDragImage",
        ))
        .unwrap();
        assert_eq!(
            loaded.imports[0].dispatcher_target,
            PpcImportDispatcherTarget::Drag(PpcDragOperation::SetImage)
        );
        let base = PPC_DATA_BASE + 0x2000;
        loaded.memory.add_region(base, vec![0; 256]);
        loaded.memory.write_u32_be(base, base + 32).unwrap();
        ppc_write_pixmap(&mut loaded.memory, base + 32, base + 96, 8, 0, 0, 1, 2, 32).unwrap();
        loaded.memory.write_u32_be(base + 96, 0x00ff0000).unwrap();
        loaded.memory.write_u32_be(base + 100, 0x000000ff).unwrap();
        loaded.memory.write_u32_be(base + 160, base + 168).unwrap();
        loaded.memory.write_u16_be(base + 168, 10).unwrap();
        ppc_write_rect(&mut loaded.memory, base + 170, 0, 0, 1, 1).unwrap();
        let reference = loaded
            .drag_manager
            .with_mut(|manager| manager.new_drag().unwrap());
        loaded.cpu.gpr[3] = reference;
        loaded.cpu.gpr[4] = base;
        loaded.cpu.gpr[5] = base + 160;
        loaded.cpu.gpr[6] = (100 << 16) | 100;
        loaded.cpu.gpr[7] = flags;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::Drag(PpcDragOperation::SetImage),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        let snapshot = loaded
            .drag_manager
            .with_ref(|manager| manager.get(reference).unwrap().image.clone().unwrap());
        assert_eq!(snapshot.pixels, vec![(0, 0, [0xffff, 0, 0])]);
        // Overwrite both guest inputs: feedback must own the original pixels/mask.
        loaded.memory.write_u32_be(base + 96, 0).unwrap();
        ppc_write_rect(&mut loaded.memory, base + 170, 0, 0, 0, 0).unwrap();
        loaded.cpu.gpr[3] = reference;
        loaded.cpu.gpr[7] = 0x80;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::Drag(PpcDragOperation::SetImage),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(
            loaded
                .drag_manager
                .with_ref(|manager| manager.get(reference).unwrap().image.clone()),
            Some(snapshot)
        );
        let clut = loaded.screen_clut.with_ref(|clut| *clut);
        let front =
            ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
                .unwrap();
        let white = ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &clut).unwrap();
        for (x, y) in [(100, 100), (101, 100), (120, 110), (121, 110)] {
            ppc_quickdraw_write_raw_pixel(&mut loaded.memory, front, (x, y), white);
        }
        let expected = ppc_physical_screen_color_pixel(
            front,
            PpcRgbColor {
                red: 0xffff,
                green: component,
                blue: component,
            },
            &clut,
        )
        .unwrap();
        let event = base + 192;
        loaded.memory.write_u16_be(event + 10, 100).unwrap();
        loaded.memory.write_u16_be(event + 12, 100).unwrap();
        loaded.cpu.gpr[3] = reference;
        loaded.cpu.gpr[4] = event;
        loaded.cpu.gpr[5] = 0;
        let mut input = PpcInputSnapshot {
            mouse_v: 100,
            mouse_h: 100,
            mouse_button: true,
            ..Default::default()
        };
        let mut events = crate::event_queue::EventQueue::default();
        assert!(matches!(
            dispatch_drag::track_drag(
                &mut loaded.cpu,
                &mut loaded.memory,
                &loaded.drag_manager,
                &mut loaded.drag_tracking_stack,
                &loaded.gworlds,
                &loaded.window_list,
                input,
                &mut events,
                &clut
            ),
            PpcImportAction::Yield(_)
        ));
        let parked = loaded.drag_tracking_stack.last().unwrap();
        assert!(parked.is_presentation_wait(loaded.cpu.pc));
        assert!(!parked.is_presentation_wait(loaded.cpu.pc.wrapping_add(4)));
        let mut unfinished = parked.clone();
        unfinished.snapshot_pending = true;
        assert!(!unfinished.is_presentation_wait(loaded.cpu.pc));
        unfinished.snapshot_pending = false;
        unfinished.phase = dispatch_drag::PpcDragPhase::Receiving;
        assert!(!unfinished.is_presentation_wait(loaded.cpu.pc));
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 100)),
            Some(expected)
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (101, 100)),
            Some(white)
        );
        input.mouse_v = 110;
        input.mouse_h = 120;
        assert!(matches!(
            dispatch_drag::track_drag(
                &mut loaded.cpu,
                &mut loaded.memory,
                &loaded.drag_manager,
                &mut loaded.drag_tracking_stack,
                &loaded.gworlds,
                &loaded.window_list,
                input,
                &mut events,
                &clut
            ),
            PpcImportAction::Yield(_)
        ));
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 100)),
            Some(white)
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (120, 110)),
            Some(expected)
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (121, 110)),
            Some(white)
        );
        input.mouse_button = false;
        assert!(
            matches!(dispatch_drag::track_drag(&mut loaded.cpu, &mut loaded.memory,
            &loaded.drag_manager, &mut loaded.drag_tracking_stack, &loaded.gworlds,
            &loaded.window_list, input, &mut events, &clut), PpcImportAction::Return(value) if value == ppc_i16_result(-128))
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (120, 110)),
            Some(white)
        );
        assert!(loaded.drag_tracking_stack.is_empty());
        loaded
            .drag_manager
            .with_mut(|manager| manager.dispose(reference))
            .unwrap();
        assert!(loaded
            .drag_manager
            .with_ref(|manager| manager.get(reference).is_err()));
    }
}

#[test]
fn native_drag_image_preserves_custom_palettes_at_all_indexed_depths() {
    for depth in [1, 2, 4, 8] {
        let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
            b"DragLib",
            b"SetDragImage",
        ))
        .unwrap();
        let base = PPC_DATA_BASE + 0x2400;
        loaded.memory.add_region(base, vec![0; 256]);
        loaded.memory.write_u32_be(base, base + 32).unwrap();
        ppc_write_pixmap(
            &mut loaded.memory,
            base + 32,
            base + 96,
            if depth == 8 { 2 } else { 1 },
            0,
            0,
            1,
            2,
            depth,
        )
        .unwrap();
        loaded
            .memory
            .write_u32_be(base + 32 + 42, base + 16)
            .unwrap();
        loaded.memory.write_u32_be(base + 16, base + 128).unwrap();
        loaded.memory.write_u16_be(base + 134, 1).unwrap();
        loaded.memory.write_u16_be(base + 138, 0xffff).unwrap(); // index 0: red
        loaded.memory.write_u16_be(base + 144, 1).unwrap();
        loaded.memory.write_u16_be(base + 150, 0xffff).unwrap(); // index 1: blue
        if depth == 8 {
            loaded.memory.write_u8(base + 97, 1).unwrap();
        } else {
            loaded
                .memory
                .write_u8(base + 96, 1 << (8 - depth * 2))
                .unwrap();
        }
        let reference = loaded
            .drag_manager
            .with_mut(|manager| manager.new_drag().unwrap());
        loaded.cpu.gpr[3] = reference;
        loaded.cpu.gpr[4] = base;
        loaded.cpu.gpr[5] = 0;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = 3;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::Drag(PpcDragOperation::SetImage),
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        let pixels = loaded.drag_manager.with_ref(|manager| {
            manager
                .get(reference)
                .unwrap()
                .image
                .as_ref()
                .unwrap()
                .pixels
                .clone()
        });
        assert_eq!(
            pixels,
            vec![(0, 0, [0xffff, 0, 0]), (0, 1, [0, 0, 0xffff])],
            "depth {depth}"
        );
    }
}

#[test]
fn zoom_rects_advances_once_per_tick_and_restores_screen() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::Drag(PpcDragOperation::ZoomRects)
    );
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
    loaded.cpu.gpr[3] = from;
    loaded.cpu.gpr[4] = to;
    loaded.cpu.gpr[5] = 4;
    loaded.cpu.gpr[6] = 0;
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)).unwrap();
    let mut startup = PpcToolboxStartupState::default();
    macro_rules! run {
        ($tick:expr) => {
            dispatch_drag::zoom_rects(
                &loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                &mut startup,
                $tick,
                100,
            )
        };
    }
    assert!(matches!(run!(0), PpcImportAction::Yield(_)));
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(before)
    );
    assert!(matches!(run!(0), PpcImportAction::Yield(_)));
    assert!(matches!(run!(1), PpcImportAction::Yield(_)));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(before)
    );
    assert!(matches!(run!(2), PpcImportAction::Yield(_)));
    assert!(matches!(run!(3), PpcImportAction::Yield(_)));
    assert!(matches!(run!(4), PpcImportAction::Yield(_)));
    assert!(matches!(run!(5), PpcImportAction::Return(0)));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(before)
    );
}

#[test]
fn zoom_rects_catches_up_after_missed_ticks() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let initial = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10));
    let late = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (25, 25));
    let mut startup = PpcToolboxStartupState::default();
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            0,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            3,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        initial
    );
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (25, 25)),
        late
    );
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            5,
            100
        ),
        PpcImportAction::Return(0)
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (25, 25)),
        late
    );
}

#[test]
fn zoom_rects_catches_up_across_tick_wrap() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let initial = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10));
    let late = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (25, 25));
    let mut startup = PpcToolboxStartupState::default();
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            u32::MAX - 2,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            0,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        initial
    );
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (25, 25)),
        late
    );
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            2,
            100
        ),
        PpcImportAction::Return(0)
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (25, 25)),
        late
    );
}

#[test]
fn zoom_rects_nested_calls_restore_each_saved_frame() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)).unwrap();
    let mut startup = PpcToolboxStartupState::default();
    macro_rules! run {
        ($tick:expr) => {
            dispatch_drag::zoom_rects(
                &loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                &mut startup,
                $tick,
                100,
            )
        };
    }
    let outer_sp = loaded.cpu.gpr[1];
    assert!(matches!(run!(0), PpcImportAction::Yield(_)));
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(before)
    );
    loaded.cpu.gpr[1] = outer_sp - 128;
    assert!(matches!(run!(0), PpcImportAction::Yield(_)));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(before)
    );
    for tick in 1..=4 {
        assert!(matches!(run!(tick), PpcImportAction::Yield(_)));
    }
    assert!(matches!(run!(5), PpcImportAction::Return(0)));
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(before)
    );
    loaded.cpu.gpr[1] = outer_sp;
    for tick in 6..=9 {
        assert!(matches!(run!(tick), PpcImportAction::Yield(_)));
    }
    assert!(matches!(run!(10), PpcImportAction::Return(0)));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(before)
    );
}

#[test]
fn zoom_rects_allocation_failure_restores_pixels_and_unwinds() {
    for stage in [1, 2, 3] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects"))
                .unwrap();
        let from = PPC_DATA_BASE + 0x1000;
        let to = PPC_DATA_BASE + 0x1010;
        loaded.memory.add_region(from, vec![0; 32]);
        ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
        ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
        loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
        let front =
            ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
                .unwrap();
        let before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10));
        let mut startup = PpcToolboxStartupState::default();
        if stage < 3 {
            dispatch_drag::fail_zoom_reservation_for_test(stage);
        }
        let first = dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            0,
            100,
        );
        let result = if stage == 3 {
            assert!(matches!(first, PpcImportAction::Yield(_)));
            assert_ne!(
                ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
                before
            );
            dispatch_drag::fail_zoom_reservation_for_test(2);
            dispatch_drag::zoom_rects(
                &loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                &mut startup,
                1,
                100,
            )
        } else {
            first
        };
        assert!(
            matches!(result, PpcImportAction::Return(value) if value == ppc_i16_result(PPC_MEM_FULL_ERR))
        );
        assert!(startup.zoom_rect.is_empty());
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
            before
        );
    }
}

#[test]
fn zoom_rects_moves_feedback_to_replaced_screen_surface() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
    let old =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let original = ppc_quickdraw_read_pixel(&mut loaded.memory, old, (10, 10));
    let mut startup = PpcToolboxStartupState::default();
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            0,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, old, (10, 10)),
        original
    );
    let new_address = 0x5100_0000;
    loaded
        .memory
        .add_region(new_address, vec![0; (old.row_bytes * old.height) as usize]);
    loaded
        .memory
        .write_u32_be(PPC_MAIN_PIXMAP, new_address)
        .unwrap();
    let new_front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    assert_eq!(new_front.base_addr, new_address);
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, new_front, (15, 15));
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            1,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, old, (10, 10)),
        original
    );
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, new_front, (15, 15)),
        before
    );
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            5,
            100
        ),
        PpcImportAction::Return(0)
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, new_front, (15, 15)),
        before
    );
}

#[test]
fn zoom_rects_rejects_invalid_arguments_without_retaining_a_frame() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let mut startup = PpcToolboxStartupState::default();
    loaded.cpu.gpr[3] = 0;
    let action = dispatch_drag::zoom_rects(
        &loaded.cpu,
        &mut loaded.memory,
        &loaded.gworlds,
        &mut startup,
        0,
        100,
    );
    assert!(
        matches!(action, PpcImportAction::Return(value) if value == ppc_i16_result(PPC_PARAM_ERR))
    );
    assert!(startup.zoom_rect.is_empty());
}

#[test]
fn zoom_rects_draws_single_pixel_and_clips_without_double_inversion() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 0, 0, 1, 1).unwrap();
    ppc_write_rect(&mut loaded.memory, to, -3, -3, 2, 2).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 0)).unwrap();
    let mut startup = PpcToolboxStartupState::default();
    macro_rules! run {
        ($tick:expr) => {
            dispatch_drag::zoom_rects(
                &loaded.cpu,
                &mut loaded.memory,
                &loaded.gworlds,
                &mut startup,
                $tick,
                100,
            )
        };
    }
    assert!(matches!(run!(0), PpcImportAction::Yield(_)));
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 0)),
        Some(before)
    );
    for tick in 1..=4 {
        assert!(matches!(run!(tick), PpcImportAction::Yield(_)));
    }
    assert!(matches!(run!(5), PpcImportAction::Return(0)));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 0)),
        Some(before)
    );
}

#[test]
fn zoom_rects_preserves_pixels_redrawn_between_frames() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let before = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)).unwrap();
    let mut startup = PpcToolboxStartupState::default();
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            0,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    let overlay = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)).unwrap();
    let replacement = (0u16..=255)
        .find(|value| *value != before && *value != overlay)
        .unwrap();
    assert!(ppc_quickdraw_write_raw_pixel(
        &mut loaded.memory,
        front,
        (10, 10),
        replacement
    ));
    assert!(matches!(
        dispatch_drag::zoom_rects(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.gworlds,
            &mut startup,
            1,
            100
        ),
        PpcImportAction::Yield(_)
    ));
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        Some(replacement)
    );
}

#[test]
fn zoom_rects_guest_import_remains_parked_until_animation_finishes() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"DragLib", b"ZoomRects")).unwrap();
    let from = PPC_DATA_BASE + 0x1000;
    let to = PPC_DATA_BASE + 0x1010;
    loaded.memory.add_region(from, vec![0; 32]);
    ppc_write_rect(&mut loaded.memory, from, 10, 10, 20, 20).unwrap();
    ppc_write_rect(&mut loaded.memory, to, 30, 30, 40, 40).unwrap();
    loaded.cpu.gpr[3..7].copy_from_slice(&[from, to, 4, 0]);
    loaded.set_clock_cycle_timing(1_000_000, 0);
    loaded.set_tick_count(42);
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let original = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10));
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.result, PpcRunResult::CycleLimit { cycles: 64 });
    assert_eq!(loaded.cpu.pc, loaded.import_trap_base);
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        original
    );
    for tick in 43..=46 {
        loaded.set_tick_count(tick);
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.result, PpcRunResult::CycleLimit { cycles: 64 });
    }
    loaded.set_tick_count(47);
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (10, 10)),
        original
    );
}
