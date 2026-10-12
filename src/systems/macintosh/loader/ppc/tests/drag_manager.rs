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
