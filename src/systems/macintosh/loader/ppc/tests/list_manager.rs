use super::*;

#[test]
fn native_list_manager_stores_cells_rows_selection_and_geometry_in_public_records() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LScroll"),
        PpcImportDispatcherTarget::LScroll
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LSize"),
        PpcImportDispatcherTarget::LSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LSetDrawingMode"),
        PpcImportDispatcherTarget::LSetDrawingMode
    );
    let pef = synthetic_pef_with_import(b"LNew");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let view_ptr = scratch;
    let bounds_ptr = scratch + 8;
    let text_ptr = scratch + 20;
    let output_ptr = scratch + 32;
    let length_ptr = scratch + 48;
    let cell_ptr = scratch + 52;
    loaded.memory.add_region(scratch, vec![0; 64]);
    ppc_write_rect(&mut loaded.memory, view_ptr, 10, 20, 90, 220).unwrap();
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
    loaded.memory.write_bytes(text_ptr, b"Pilot").unwrap();
    loaded.cpu.gpr[3] = view_ptr;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = (20u32 << 16) | 100;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[8] = 0;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0;
    loaded
        .memory
        .write_u32_be(
            ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                .unwrap(),
            1,
        )
        .unwrap();
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.unsupported_import_index, None);
    let list = loaded.cpu.gpr[3];
    let list_ptr = loaded.memory.read_u32_be(list).unwrap();
    assert_ne!(list, 0);
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, list_ptr + PPC_LIST_DATA_BOUNDS_OFFSET),
        Some((0, 0, 2, 2))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, list_ptr + PPC_LIST_VISIBLE_OFFSET),
        Some((0, 0, 2, 2))
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(list_ptr + PPC_LIST_MAX_INDEX_OFFSET),
        Some(8)
    );
    assert_eq!(
        loaded
            .list_manager
            .get_record(list)
            .map(|record| record.cell_size),
        Some((20, 100))
    );
    let vertical_scroll = loaded
        .memory
        .read_u32_be(list_ptr + PPC_LIST_VSCROLL_OFFSET)
        .unwrap();
    assert_ne!(vertical_scroll, 0);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(list_ptr + PPC_LIST_HSCROLL_OFFSET),
        Some(0)
    );
    let vertical_scroll_ptr = ppc_control_ptr(&mut loaded.memory, vertical_scroll).unwrap();
    assert_eq!(
        ppc_read_rect(
            &mut loaded.memory,
            vertical_scroll_ptr + PPC_CONTROL_RECT_OFFSET,
        ),
        Some((9, 220, 91, 236))
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(vertical_scroll_ptr + PPC_CONTROL_VISIBLE_OFFSET),
        Some(0)
    );
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LSetDrawingMode);
    assert!(loaded.list_manager.get_record(list).unwrap().draw_enabled);
    assert_eq!(
        loaded
            .memory
            .read_u8(vertical_scroll_ptr + PPC_CONTROL_VISIBLE_OFFSET),
        Some(0xff)
    );

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LSetDrawingMode);
    assert!(!loaded.list_manager.get_record(list).unwrap().draw_enabled);
    assert_eq!(loaded.memory.read_u8(vertical_scroll_ptr + PPC_CONTROL_VISIBLE_OFFSET), Some(0xff));
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LSetDrawingMode);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LSetCell;
    loaded.cpu.gpr[3] = text_ptr;
    loaded.cpu.gpr[4] = 5;
    loaded.cpu.gpr[5] = (1u32 << 16) | 1;
    loaded.cpu.gpr[6] = list;
    loaded.run_with_hle_imports(128);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LGetCell;
    loaded.memory.write_u16_be(length_ptr, 16).unwrap();
    loaded.cpu.gpr[3] = output_ptr;
    loaded.cpu.gpr[4] = length_ptr;
    loaded.cpu.gpr[5] = (1u32 << 16) | 1;
    loaded.cpu.gpr[6] = list;
    loaded.run_with_hle_imports(128);
    assert_eq!(loaded.memory.read_u16_be(length_ptr), Some(5));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, output_ptr, 5),
        Some(b"Pilot".to_vec())
    );

    loaded.memory.write_bytes(output_ptr, b"XXXXX").unwrap();
    loaded.memory.write_u16_be(length_ptr, 3).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.run_with_hle_imports(128);
    assert_eq!(loaded.memory.read_u16_be(length_ptr), Some(3));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, output_ptr, 5),
        Some(b"XXXXX".to_vec())
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LSetSelect;
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = (1u32 << 16) | 1;
    loaded.cpu.gpr[5] = list;
    loaded.run_with_hle_imports(128);
    loaded.memory.write_u16_be(cell_ptr, 0).unwrap();
    loaded.memory.write_u16_be(cell_ptr + 2, 0).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LGetSelect;
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = cell_ptr;
    loaded.cpu.gpr[5] = list;
    loaded.run_with_hle_imports(128);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(cell_ptr), Some(1));
    assert_eq!(loaded.memory.read_u16_be(cell_ptr + 2), Some(1));

    let selected_before_rejected_clicks = loaded.list_manager.get_record(list).unwrap().selected;
    loaded.cpu.gpr[3] = (9u32 << 16) | 20;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LClick);
    assert_eq!(
        loaded.list_manager.get_record(list).unwrap().selected, selected_before_rejected_clicks,
        "LClick must ignore points outside rView",
    );

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LActivate);
    assert_eq!(
        loaded
            .memory
            .read_u8(vertical_scroll_ptr + PPC_CONTROL_HILITE_OFFSET),
        Some(0xfe)
    );
    loaded.cpu.gpr[3] = (10u32 << 16) | 20;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LClick);
    assert_eq!(
        loaded.list_manager.get_record(list).unwrap().selected, selected_before_rejected_clicks,
        "LClick must ignore inactive lists",
    );
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LActivate);
    assert_eq!(
        loaded
            .memory
            .read_u8(vertical_scroll_ptr + PPC_CONTROL_HILITE_OFFSET),
        Some(0)
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::LAddRow;
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 2;
    loaded.cpu.gpr[5] = list;
    loaded.run_with_hle_imports(128);
    let list_ptr = loaded.memory.read_u32_be(list).unwrap();
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, list_ptr + PPC_LIST_DATA_BOUNDS_OFFSET),
        Some((0, 0, 5, 2))
    );
    assert_eq!(
        loaded.list_manager.get_record(list).map(|record| record.visible),
        Some((0, 0, 4, 2))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, list_ptr + PPC_LIST_VISIBLE_OFFSET),
        Some((0, 0, 4, 2))
    );

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LScroll);
    let list_ptr = loaded.memory.read_u32_be(list).unwrap();
    assert_eq!(
        loaded.list_manager.get_record(list).map(|record| record.visible),
        Some((1, 0, 5, 2))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, list_ptr + PPC_LIST_VISIBLE_OFFSET),
        Some((1, 0, 5, 2))
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(vertical_scroll_ptr + PPC_CONTROL_VALUE_OFFSET),
        Some(1)
    );

    loaded.cpu.gpr[3] = 100;
    loaded.cpu.gpr[4] = 40;
    loaded.cpu.gpr[5] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LSize);
    let list_ptr = loaded.memory.read_u32_be(list).unwrap();
    assert_eq!(
        loaded
            .list_manager
            .get_record(list)
            .map(|record| record.view_rect),
        Some((10, 20, 50, 120))
    );
    assert_eq!(
        loaded.list_manager.get_record(list).map(|record| record.visible),
        Some((1, 0, 3, 1))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, list_ptr + PPC_LIST_VIEW_OFFSET),
        Some((10, 20, 50, 120))
    );
    assert_eq!(
        ppc_read_rect(&mut loaded.memory, list_ptr + PPC_LIST_VISIBLE_OFFSET),
        Some((1, 0, 3, 1))
    );
    assert_eq!(
        ppc_read_rect(
            &mut loaded.memory,
            vertical_scroll_ptr + PPC_CONTROL_RECT_OFFSET,
        ),
        Some((9, 120, 51, 136))
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(vertical_scroll_ptr + PPC_CONTROL_MAX_OFFSET),
        Some(3)
    );

    loaded.cpu.gpr[3] = list;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::LDispose);
    assert_eq!(loaded.memory.read_u32_be(vertical_scroll), Some(0));
    assert!(!loaded.controls.contains_handle(vertical_scroll));
}

#[test]
fn native_list_manager_draws_cell_backgrounds_in_port_coordinates() {
    let pef = synthetic_pef_with_import(b"LNew");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x2200;
    let view_ptr = scratch;
    let bounds_ptr = scratch + 8;
    loaded.memory.add_region(scratch, vec![0; 24]);
    ppc_write_rect(&mut loaded.memory, view_ptr, 10, 20, 50, 120).unwrap();
    ppc_write_rect(&mut loaded.memory, bounds_ptr, 0, 0, 3, 1).unwrap();
    loaded.cpu.gpr[3] = view_ptr;
    loaded.cpu.gpr[4] = bounds_ptr;
    loaded.cpu.gpr[5] = (20u32 << 16) | 100;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = PPC_MAIN_GWORLD;
    loaded.cpu.gpr[8] = 0;
    loaded.cpu.gpr[9] = 0;
    loaded.cpu.gpr[10] = 0;
    loaded
        .memory
        .write_u32_be(
            ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                .unwrap(),
            0,
        )
        .unwrap();
    let probe = loaded.run_with_hle_imports(128);
    assert_eq!(probe.unsupported_import_index, None);
    let list = loaded.cpu.gpr[3];
    loaded
        .list_manager
        .with_record_mut(list, |record| {
            record.selected.insert((0, 0));
        })
        .unwrap();

    // NewCWindow gives an on-screen window a negative PixMap origin so
    // its local port coordinates map to its global screen position.
    ppc_set_port_origin(&mut loaded.memory, PPC_MAIN_GWORLD, -150, -40).unwrap();
    let surface =
        ppc_live_quickdraw_surface(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    assert_eq!((surface.top, surface.left), (-40, -150));
    let front = surface.front_buffer;
    let black =
        ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
    let white =
        ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &loaded.screen_clut).unwrap();
    assert!(ppc_fill_front_rect(
        &mut loaded.memory,
        front,
        (0, 0, front.height as i16, front.width as i16),
        PPC_RGB_WHITE,
    ));
    let record = loaded.list_manager.get_record(list).unwrap();
    ppc_list_draw(&mut loaded.memory, &loaded.gworlds, &record);

    // Row 0, column 0 is local (10,20)-(30,120), which maps to
    // screen (50,170)-(70,270) for this window origin.
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (260, 65)),
        Some(black),
        "selected list background must follow the port origin"
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (110, 25)),
        Some(white),
        "selected list background must not remain at port-local screen coordinates"
    );
}

#[test]
fn attached_list_manager_mutations_and_lifetime_cross_isa_immediately() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"LNew")).unwrap();
    let (mut classic, _classic_cpu, _classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let scratch = PPC_DATA_BASE + 0x1800;
    let view_ptr = scratch;
    let bounds_ptr = scratch + 8;
    let classic_text_ptr = scratch + 20;
    let native_text_ptr = scratch + 32;
    let output_ptr = scratch + 48;
    let length_ptr = scratch + 64;
    native.memory.add_region(scratch, vec![0; 80]);
    ppc_write_rect(&mut native.memory, view_ptr, 10, 20, 50, 220).unwrap();
    ppc_write_rect(&mut native.memory, bounds_ptr, 0, 0, 2, 2).unwrap();
    native
        .memory
        .write_bytes(classic_text_ptr, b"Classic")
        .unwrap();
    native
        .memory
        .write_bytes(native_text_ptr, b"Native")
        .unwrap();

    native.cpu.gpr[3] = view_ptr;
    native.cpu.gpr[4] = bounds_ptr;
    native.cpu.gpr[5] = (20u32 << 16) | 100;
    native.cpu.gpr[6] = 0;
    native.cpu.gpr[7] = PPC_MAIN_GWORLD;
    native.cpu.gpr[8] = 0;
    native.cpu.gpr[9] = 0;
    native.cpu.gpr[10] = 0;
    native
        .memory
        .write_u32_be(
            ppc_parameter_area_slot_addr(native.cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                .unwrap(),
            0,
        )
        .unwrap();
    native.run_with_hle_imports(128);
    let first_list = native.cpu.gpr[3];
    assert!(classic.list_states.contains_handle(first_list));
    assert_eq!(
        native
            .list_manager
            .get_record(first_list)
            .map(|record| record.cell_size),
        Some((20, 100))
    );

    classic
        .list_states
        .with_record_mut(first_list, |classic_state| {
            classic_state.cells.insert((1, 1), b"Classic".to_vec());
            classic_state.selected.insert((1, 1));
            classic_state.draw_enabled = true;
        })
        .unwrap();

    native.memory.write_u16_be(length_ptr, 16).unwrap();
    native.cpu.gpr[3] = output_ptr;
    native.cpu.gpr[4] = length_ptr;
    native.cpu.gpr[5] = (1u32 << 16) | 1;
    native.cpu.gpr[6] = first_list;
    run_test_import(&mut native, PpcImportDispatcherTarget::LGetCell);
    assert_eq!(native.memory.read_u16_be(length_ptr), Some(7));
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, output_ptr, 7),
        Some(b"Classic".to_vec())
    );

    native.cpu.gpr[3] = native_text_ptr;
    native.cpu.gpr[4] = 6;
    native.cpu.gpr[5] = 0;
    native.cpu.gpr[6] = first_list;
    run_test_import(&mut native, PpcImportDispatcherTarget::LSetCell);
    assert_eq!(
        classic
            .list_states
            .get_record(first_list)
            .and_then(|record| record.cells.get(&(0, 0)).cloned())
            .as_deref(),
        Some(b"Native".as_slice())
    );

    native.cpu.gpr[3] = view_ptr;
    native.cpu.gpr[4] = bounds_ptr;
    native.cpu.gpr[5] = (20u32 << 16) | 100;
    native.cpu.gpr[6] = 0;
    native.cpu.gpr[7] = PPC_MAIN_GWORLD;
    native.cpu.gpr[8] = 0;
    native.cpu.gpr[9] = 0;
    native.cpu.gpr[10] = 0;
    run_test_import(&mut native, PpcImportDispatcherTarget::LNew);
    let second_list = native.cpu.gpr[3];
    assert_ne!(first_list, second_list);
    assert_eq!(classic.list_states.len(), 2);
    assert_eq!(
        native
            .list_manager
            .get_record(second_list)
            .map(|record| record.cell_size),
        Some((20, 100))
    );

    native.cpu.gpr[3] = first_list;
    run_test_import(&mut native, PpcImportDispatcherTarget::LDispose);
    assert!(!classic.list_states.contains_handle(first_list));
    assert!(classic.list_states.contains_handle(second_list));
}

#[test]
fn cloned_native_adapter_detaches_list_manager_state() {
    let original = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    original.list_manager.insert_record(
        0x0032_1000,
        PpcListRecord {
            handle: 0x0032_1000,
            cells_handle: 0x0032_2000,
            view_rect: (0, 0, 40, 100),
            data_bounds: (0, 0, 2, 1),
            cell_size: (20, 100),
            visible: (0, 0, 2, 1),
            port: PPC_MAIN_GWORLD,
            draw_enabled: true,
            active: true,
            cells: [((0, 0), b"Original".to_vec())].into(),
            selected: [(0, 0)].into(),
            last_click: (0, 0),
            last_click_tick: 10,
        },
    );
    let detached = original.clone();
    detached
        .list_manager
        .with_record_mut(0x0032_1000, |detached_record| {
            detached_record.cells.insert((0, 0), b"Detached".to_vec());
            detached_record.selected.clear();
        })
        .unwrap();
    detached.list_manager.insert_record(
        0x0032_3000,
        PpcListRecord {
            handle: 0x0032_3000,
            cells_handle: 0x0032_4000,
            view_rect: (0, 0, 20, 100),
            data_bounds: (0, 0, 1, 1),
            cell_size: (20, 100),
            visible: (0, 0, 1, 1),
            port: PPC_MAIN_GWORLD,
            draw_enabled: false,
            active: true,
            cells: Default::default(),
            selected: Default::default(),
            last_click: (-1, -1),
            last_click_tick: 0,
        },
    );

    assert_eq!(
        original
            .list_manager
            .get_record(0x0032_1000)
            .unwrap()
            .cells[&(0, 0)],
        b"Original"
    );
    assert!(original
        .list_manager
        .get_record(0x0032_1000)
        .unwrap()
        .selected
        .contains(&(0, 0)));
    assert!(!original.list_manager.contains_handle(0x0032_3000));
}
