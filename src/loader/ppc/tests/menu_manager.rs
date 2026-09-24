use super::*;

pub(crate) fn draw_current_test_menu_bar(loaded: &mut PpcLoadedApp) -> bool {
    let menu_list = ppc_current_menu_list(&mut loaded.memory);
    let menu_color_bytes =
        ppc_menu_color_table_bytes(&mut loaded.memory, &test_handle_records!(loaded));
    ppc_draw_menu_bar_with_colors(
        &mut loaded.memory,
        &loaded.gworlds,
        menu_list,
        &loaded.screen_clut,
        MenuColorTable::new(&menu_color_bytes),
    )
}

pub(crate) struct CrossAbiMenuSelectFixture {
    pub(crate) app: PpcLoadedApp,
    pub(crate) root_menu: u32,
    pub(crate) root_record: u32,
    pub(crate) mdef_entry: u32,
    pub(crate) callback_marker: u32,
    pub(crate) title_h: i16,
}

pub(crate) fn cross_abi_menu_select_fixture() -> CrossAbiMenuSelectFixture {
    const ROOT_MENU_ID: i16 = 140;
    const CHILD_MENU_ID: i16 = 141;
    const TARGET_ITEM: i16 = 2;
    const MDEF_HANDLE: u32 = PPC_DATA_BASE + 0x7000;
    const MDEF_ENTRY: u32 = PPC_DATA_BASE + 0x7100;
    const CALLBACK_MARKER: u32 = PPC_DATA_BASE + 0x7200;
    const CALLBACK_VALUE: u32 = 0x68c0_ab1e;

    let pef = synthetic_pef_with_import(b"MenuSelect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let root_menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        ROOT_MENU_ID,
        b"File",
        b"Custom child;Disabled after callback",
    );
    let child_menu = install_test_popup_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1400,
        CHILD_MENU_ID,
        b"Child",
        b"Choice",
    );

    loaded.cpu.gpr[3] = root_menu;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 0x1b;
    ppc_set_item_cmd(
        &loaded.cpu,
        &mut loaded.memory,
        &test_handle_records!(loaded),
    );
    loaded.cpu.gpr[5] = CHILD_MENU_ID as u32;
    ppc_set_item_mark(
        &loaded.cpu,
        &mut loaded.memory,
        &test_handle_records!(loaded),
    );

    let root_record = loaded.memory.read_u32_be(root_menu).unwrap();
    let child_record = loaded.memory.read_u32_be(child_menu).unwrap();
    loaded.memory.write_u16_be(child_record + 2, 72).unwrap();
    loaded.memory.write_u16_be(child_record + 4, 32).unwrap();
    loaded.memory.add_region(MDEF_HANDLE, vec![0; 4]);
    loaded.memory.add_region(CALLBACK_MARKER, vec![0; 4]);

    // The resource-backed MDEF is real 68k guest code. It pushes the
    // Pascal DisableItem(rootMenu, 2) arguments, executes the A-line, then
    // leaves a marker only after the trap returns and closes its own
    // 18-byte MenuDefProc argument frame.
    let mut mdef = Vec::new();
    mdef.extend_from_slice(&0x2f3cu16.to_be_bytes()); // MOVE.L #rootMenu,-(SP)
    mdef.extend_from_slice(&root_menu.to_be_bytes());
    mdef.extend_from_slice(&0x3f3cu16.to_be_bytes()); // MOVE.W #2,-(SP)
    mdef.extend_from_slice(&(TARGET_ITEM as u16).to_be_bytes());
    mdef.extend_from_slice(&0xa93au16.to_be_bytes()); // DisableItem
    mdef.extend_from_slice(&0x23fcu16.to_be_bytes()); // MOVE.L #value,marker
    mdef.extend_from_slice(&CALLBACK_VALUE.to_be_bytes());
    mdef.extend_from_slice(&CALLBACK_MARKER.to_be_bytes());
    mdef.extend_from_slice(&0x4e74u16.to_be_bytes()); // RTD #18
    mdef.extend_from_slice(&0x0012u16.to_be_bytes());
    loaded.memory.add_region(MDEF_ENTRY, mdef.clone());
    loaded.memory.write_u32_be(MDEF_HANDLE, MDEF_ENTRY).unwrap();
    loaded
        .memory
        .write_u32_be(child_record + 6, MDEF_HANDLE)
        .unwrap();
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: "Cross-ABI menu fixture".to_string(),
        res_type: u32::from_be_bytes(*b"MDEF"),
        res_id: 512,
        name: Vec::new(),
        data: mdef,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: MDEF_HANDLE,
    });

    assert!(draw_current_test_menu_bar(&mut loaded));
    let title_h = STANDARD_MENU_BAR_FIRST_TITLE_LEFT + 2;
    loaded.cpu.gpr[3] = (u32::from(10u16) << 16) | u32::from(title_h as u16);

    CrossAbiMenuSelectFixture {
        app: loaded,
        root_menu,
        root_record,
        mdef_entry: MDEF_ENTRY,
        callback_marker: CALLBACK_MARKER,
        title_h,
    }
}

#[test]
fn menu_accessors_parse_item_count_text_and_command() {
    let menu_handle = 0x1000;
    let menu = 0x1100;
    let output = 0x1200;
    let menu_list_handle = 0x1300;
    let menu_list = 0x1400;
    let mut memory = PpcSectionMem::new();
    memory.add_region(menu_handle, vec![0; 4]);
    memory.add_region(menu, vec![0; 64]);
    memory.add_region(output, vec![0; 256]);
    memory.add_region(menu_list_handle, vec![0; 4]);
    memory.add_region(menu_list, vec![0; 18]);
    memory.write_u32_be(menu_handle, menu).unwrap();
    memory.write_u16_be(menu, 128).unwrap();
    memory.write_u32_be(menu + 10, 0b111).unwrap();
    memory.write_u32_be(menu_list_handle, menu_list).unwrap();
    memory.write_u16_be(menu_list, 6).unwrap();
    memory.write_u32_be(menu_list + 6, menu_handle).unwrap();
    memory.write_u8(menu + 14, 4).unwrap();
    memory.write_bytes(menu + 15, b"Test").unwrap();
    memory.write_u8(menu + 19, 3).unwrap();
    memory.write_bytes(menu + 20, b"Run").unwrap();
    memory.write_u8(menu + 24, b'R').unwrap();
    memory.write_u8(menu + 27, 4).unwrap();
    memory.write_bytes(menu + 28, b"Quit").unwrap();
    memory.write_u8(menu + 33, b'Q').unwrap();
    memory.write_u8(menu + 36, 0).unwrap();

    assert_eq!(ppc_count_menu_items(&mut memory, menu_handle), 2);

    let unterminated_handle = 0x1500;
    let unterminated_menu = 0x1600;
    memory.add_region(unterminated_handle, vec![0; 4]);
    memory.add_region(unterminated_menu, vec![0; 24]);
    memory
        .write_u32_be(unterminated_handle, unterminated_menu)
        .unwrap();
    memory
        .write_u32_be(unterminated_menu + 10, u32::MAX)
        .unwrap();
    memory.write_u8(unterminated_menu + 14, 0).unwrap();
    memory.write_u8(unterminated_menu + 15, 4).unwrap();
    memory.write_bytes(unterminated_menu + 16, b"Open").unwrap();
    memory
        .write_bytes(unterminated_menu + 20, &[0, b'O', 0, 0])
        .unwrap();
    assert_eq!(
        ppc_count_menu_items(&mut memory, unterminated_handle),
        0,
        "both adapters must reject the same unterminated MenuInfo sequence"
    );

    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = menu_handle;
    cpu.gpr[4] = 2;
    cpu.gpr[5] = output;
    ppc_get_menu_item_text(&cpu, &mut memory);
    assert_eq!(memory.read_u8(output), Some(4));
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, output + 1, 4),
        Some(b"Quit".to_vec())
    );

    ppc_get_item_cmd(&cpu, &mut memory);
    assert_eq!(memory.read_u16_be(output), Some(u16::from(b'Q')));
    assert_eq!(
        ppc_menu_key(&mut memory, menu_list_handle, b'r')
            .map_or(0, MenuKeySelection::packed_result),
        (128 << 16) | 1
    );

    cpu.gpr[5] = 0x1b;
    let handles = [PpcHandleRecord {
        handle: menu_handle,
        ptr: menu,
        size: 64,
        capacity: 64,
    }];
    ppc_set_item_cmd(&cpu, &mut memory, &handles);
    cpu.gpr[5] = output;
    ppc_get_item_cmd(&cpu, &mut memory);
    assert_eq!(memory.read_u16_be(output), Some(0x1b));
}

#[test]
fn menu_event_resolves_command_key_events_only() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"MenuEvent")).unwrap();
    install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"Game",
        b"Start/S",
    );
    let event = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(event, vec![0; 16]);
    loaded.memory.write_u16_be(event, 3).unwrap();
    loaded.memory.write_u32_be(event + 2, u32::from(b's')).unwrap();
    loaded.memory.write_u16_be(event + 14, 0x0100).unwrap();
    loaded.cpu.gpr[3] = event;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::MenuEvent);

    assert_eq!(loaded.cpu.gpr[3], (128 << 16) | 1);

    loaded.memory.write_u16_be(event + 14, 0).unwrap();
    loaded.cpu.gpr[3] = event;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MenuEvent);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

// IM:V 1986 pp. V-228--V-230 and MTE 1992 pp. 3-108--3-109:
// InsertMenu(-1) records a submenu in the hierarchical portion of the
// DynamicMenuList. It remains searchable, but never becomes a menu title.
#[test]
fn native_insert_menu_preserves_regular_and_hierarchical_partitions() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    let regular_title = scratch;
    let hierarchical_title = scratch + 0x20;
    let regular_items = scratch + 0x40;
    let hierarchical_items = scratch + 0x60;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        regular_title,
        b"File"
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        hierarchical_title,
        b"Recent"
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        regular_items,
        b"Open/O"
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        hierarchical_items,
        b"Document/D"
    ));

    let regular = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        128,
        regular_title,
    );
    let hierarchical = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        200,
        hierarchical_title,
    );
    assert_ne!(regular, 0);
    assert_ne!(hierarchical, 0);
    assert_eq!(
        ppc_insert_menu_items(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            regular,
            regular_items,
            i16::MAX,
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_insert_menu_items(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            hierarchical,
            hierarchical_items,
            i16::MAX,
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            regular,
            0,
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            hierarchical,
            -1,
        ),
        PPC_NO_ERR
    );

    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    assert_ne!(menu_list_handle, 0);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(crate::memory::globals::addr::MENU_LIST),
        Some(menu_list_handle)
    );
    let menu_list = loaded.memory.read_u32_be(menu_list_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(menu_list), Some(6));
    assert_eq!(loaded.memory.read_u32_be(menu_list + 6), Some(regular));
    assert_eq!(
        loaded.memory.read_u16_be(menu_list + 10),
        Some(STANDARD_MENU_BAR_FIRST_TITLE_LEFT as u16)
    );
    assert_eq!(loaded.memory.read_u16_be(menu_list + 12), Some(6));
    assert_eq!(loaded.memory.read_u32_be(menu_list + 14), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(menu_list + 18),
        Some(hierarchical)
    );
    assert_eq!(loaded.memory.read_u16_be(menu_list + 22), Some(0));
    assert_eq!(
        loaded
            .handles()
            .iter()
            .find(|record| record.handle == menu_list_handle)
            .map(|record| record.size),
        Some(24)
    );

    let definition = ppc_menu_list_definition(&mut loaded.memory, menu_list_handle).unwrap();
    assert_eq!(
        definition.regular,
        vec![PpcMenuListEntry {
            handle: regular,
            value: STANDARD_MENU_BAR_FIRST_TITLE_LEFT,
        }]
    );
    assert_eq!(
        definition.hierarchical,
        vec![PpcMenuListEntry {
            handle: hierarchical,
            value: 0,
        }]
    );
    let snapshot = ppc_guest_menu_snapshot(&mut loaded.memory, menu_list_handle);
    assert_eq!(snapshot.menus.len(), 2);
    assert!(!snapshot.menus[0].hierarchical);
    assert!(snapshot.menus[0].visible_in_menu_bar);
    assert!(snapshot.menus[1].hierarchical);
    assert!(!snapshot.menus[1].visible_in_menu_bar);
    assert_eq!(
        ppc_get_menu_handle(&mut loaded.memory, menu_list_handle, 200),
        hierarchical
    );
    assert_eq!(
        ppc_menu_key(&mut loaded.memory, menu_list_handle, b'd')
            .map_or(0, MenuKeySelection::packed_result),
        (200 << 16) | 1
    );
    let regular_command =
        ppc_menu_item_attribute_address(&mut loaded.memory, regular, 1, 1).unwrap();
    loaded.memory.write_u8(regular_command, b'D').unwrap();
    assert_eq!(
        ppc_menu_key(&mut loaded.memory, menu_list_handle, b'd')
            .map_or(0, MenuKeySelection::packed_result),
        (128 << 16) | 1,
        "regular menus must be searched before the hierarchical partition"
    );
    loaded.memory.write_u8(regular_command, b'O').unwrap();
    let command = ppc_menu_item_attribute_address(&mut loaded.memory, regular, 1, 1).unwrap();
    let mark = ppc_menu_item_attribute_address(&mut loaded.memory, regular, 1, 2).unwrap();
    loaded.memory.write_u8(command, 0x1B).unwrap();
    loaded.memory.write_u8(mark, 200).unwrap();
    assert_eq!(
        ppc_submenu_handle_for_item(&mut loaded.memory, menu_list_handle, regular, 1),
        Some(hierarchical),
        "submenu resolution must find the matching hierarchical entry"
    );
    loaded.memory.write_u16_be(menu_list + 2, 80).unwrap();
    loaded.memory.write_u16_be(menu_list + 10, 40).unwrap();
    assert_eq!(
        ppc_menu_handle_at_title_point(&mut loaded.memory, menu_list_handle, 39),
        None
    );
    assert_eq!(
        ppc_menu_handle_at_title_point(&mut loaded.memory, menu_list_handle, 40),
        Some((regular, 40)),
        "PPC title hits must use the live guest menuLeft"
    );
    assert_eq!(
        ppc_menu_handle_at_title_point(&mut loaded.memory, menu_list_handle, 79),
        Some((regular, 40))
    );
    assert_eq!(
        ppc_menu_handle_at_title_point(&mut loaded.memory, menu_list_handle, 80),
        None
    );
    loaded
        .memory
        .write_u16_be(menu_list + 2, definition.last_right as u16)
        .unwrap();
    loaded
        .memory
        .write_u16_be(menu_list + 10, definition.regular[0].value as u16)
        .unwrap();

    let first_non_title_h = definition.last_right.saturating_add(1);
    assert_eq!(
        ppc_menu_handle_at_title_point(
            &mut loaded.memory,
            menu_list_handle,
            first_non_title_h as u16 as u32,
        ),
        None
    );

    let menu_bar_bytes = 20 * ppc_main_screen_row_bytes();
    assert!(loaded
        .memory
        .write_bytes(PPC_MAIN_SCREEN_BASE, &vec![0x55; menu_bar_bytes as usize])
        .is_some());
    assert!(ppc_draw_menu_bar(
        &mut loaded.memory,
        &loaded.gworlds,
        menu_list_handle,
        &loaded.screen_clut,
    ));
    let with_hierarchical =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, menu_bar_bytes)
            .unwrap();
    let regular_only = ppc_alloc_menu_list_definition_handle(
        &PpcMenuListDefinition {
            last_right: definition.last_right,
            regular: definition.regular.clone(),
            ..PpcMenuListDefinition::default()
        },
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
    );
    assert_ne!(regular_only, 0);
    assert_eq!(
        ppc_submenu_handle_for_item(&mut loaded.memory, regular_only, regular, 1),
        None,
        "a detached or regular-only ID match must not resolve as a submenu"
    );
    assert!(loaded
        .memory
        .write_bytes(PPC_MAIN_SCREEN_BASE, &vec![0x55; menu_bar_bytes as usize])
        .is_some());
    assert!(ppc_draw_menu_bar(
        &mut loaded.memory,
        &loaded.gworlds,
        regular_only,
        &loaded.screen_clut,
    ));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, menu_bar_bytes,),
        Some(with_hierarchical)
    );

    let before_duplicate =
        ppc_menu_list_definition(&mut loaded.memory, menu_list_handle).unwrap();
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            hierarchical,
            0,
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_menu_list_definition(&mut loaded.memory, menu_list_handle),
        Some(before_duplicate),
        "reinserting a current menu must be a no-op"
    );
}

#[test]
fn native_draw_menu_bar_uses_live_shared_title_geometry() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 128, b"File", b"Open");
    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    let menu_list = loaded.memory.read_u32_be(menu_list_handle).unwrap();
    loaded.memory.write_u16_be(menu_list + 2, 90).unwrap();
    loaded.memory.write_u16_be(menu_list + 6 + 4, 50).unwrap();

    assert!(draw_current_test_menu_bar(&mut loaded));
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let black = ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut)
        .expect("black screen pixel");
    let mut contains_black = |left: i16, right: i16| {
        (2..18).any(|y| {
            (left..right).any(|x| {
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (i32::from(x), i32::from(y)),
                ) == Some(black)
            })
        })
    };

    assert!(
        !contains_black(
            STANDARD_MENU_BAR_FIRST_TITLE_LEFT + STANDARD_MENU_BAR_TITLE_ORIGIN_INSET,
            40,
        ),
        "drawing must not retain the independently reconstructed title origin"
    );
    assert!(
        contains_black(57, 80),
        "drawing must place title artwork from the live menuLeft"
    );
}

#[test]
fn native_draw_hilite_and_flash_menu_bar_use_live_shared_menu_colors() {
    let pef = synthetic_pef_with_import(b"DrawMenuBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 128, b"File", b"Open");
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

    let set_rgb = |entry: &mut [u8; MENU_COLOR_ENTRY_SIZE], offset: usize, rgb: [u16; 3]| {
        for (channel, value) in rgb.into_iter().enumerate() {
            let channel_offset = offset + channel * 2;
            entry[channel_offset..channel_offset + 2].copy_from_slice(&value.to_be_bytes());
        }
    };
    let bar_background = [0xffff, 0x1111, 0x1111];
    let inherited_title = [0x1111, 0xffff, 0x1111];
    let live_title = [0x1111, 0x1111, 0xffff];
    let title_background = [0xffff, 0xffff, 0xffff];
    let mut bar = test_menu_color_entry(0, 0, 0);
    set_rgb(&mut bar, 4, inherited_title);
    set_rgb(&mut bar, 22, bar_background);
    let mut title = test_menu_color_entry(128, 0, 0);
    set_rgb(&mut title, 4, inherited_title);
    set_rgb(&mut title, 10, title_background);
    let color_bytes = [bar.as_slice(), title.as_slice()].concat();
    let color_handle = ppc_ensure_menu_color_table_handle(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
    );
    assert_ne!(color_handle, 0);
    assert_eq!(
        ppc_replace_menu_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            color_handle,
            &color_bytes,
        ),
        PPC_NO_ERR,
    );

    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawMenuBar);
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let bar_pixel = ppc_physical_screen_color_pixel(
        front,
        ppc_menu_rgb(bar_background),
        &loaded.screen_clut,
    )
    .unwrap();
    let inherited_pixel = ppc_physical_screen_color_pixel(
        front,
        ppc_menu_rgb(inherited_title),
        &loaded.screen_clut,
    )
    .unwrap();
    let live_pixel =
        ppc_physical_screen_color_pixel(front, ppc_menu_rgb(live_title), &loaded.screen_clut)
            .unwrap();
    let title_background_pixel = ppc_physical_screen_color_pixel(
        front,
        ppc_menu_rgb(title_background),
        &loaded.screen_clut,
    )
    .unwrap();
    assert_ne!(bar_pixel, inherited_pixel);
    assert_ne!(bar_pixel, live_pixel);
    assert_ne!(bar_pixel, title_background_pixel);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (120, 5)),
        Some(bar_pixel),
        "DrawMenuBar did not use the menu-bar entry RGB4",
    );

    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    let region = ppc_menu_list_definition(&mut loaded.memory, menu_list_handle)
        .unwrap()
        .regular_title_regions()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (i32::from(region.left - 1), 2)),
        Some(title_background_pixel),
        "DrawMenuBar did not use the title entry RGB2 for its title cell",
    );
    let title_pixel = (1..19)
        .flat_map(|y| (region.left..region.right).map(move |x| (x, y)))
        .find(|&(x, y)| {
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (i32::from(x), i32::from(y)))
                == Some(inherited_pixel)
        })
        .expect("inherited green title ink");

    let color_data = loaded.memory.read_u32_be(color_handle).unwrap();
    for (channel, value) in live_title.into_iter().enumerate() {
        loaded
            .memory
            .write_u16_be(
                color_data + MENU_COLOR_ENTRY_SIZE as u32 + 4 + channel as u32 * 2,
                value,
            )
            .unwrap();
    }
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawMenuBar);
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            front,
            (i32::from(title_pixel.0), i32::from(title_pixel.1)),
        ),
        Some(live_pixel),
        "DrawMenuBar did not reread the guest-mutated title entry",
    );

    loaded.cpu.gpr[3] = 128;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteMenu);
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            front,
            (i32::from(title_pixel.0), i32::from(title_pixel.1)),
        ),
        Some(title_background_pixel),
        "HiliteMenu did not reverse the live title foreground",
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (i32::from(region.left - 1), 2),),
        Some(live_pixel),
        "HiliteMenu did not reverse the live title background",
    );

    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteMenu);
    let menu_bar_bytes = front.row_bytes * 20;
    let normal =
        ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes).unwrap();
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (120, 5)),
        Some(inherited_pixel),
        "FlashMenuBar did not reverse live RGB4 through default RGB1",
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            front,
            (i32::from(title_pixel.0), i32::from(title_pixel.1)),
        ),
        Some(live_pixel),
        "FlashMenuBar changed a title-specific RGB1 pixel",
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (i32::from(region.left - 1), 2)),
        Some(title_background_pixel),
        "FlashMenuBar changed a title-specific RGB2 pixel",
    );
    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
        Some(normal),
        "two live-color FlashMenuBar calls did not restore the menu bar",
    );
}

// MTE 1992 p. 3-131 and HIG 1992 p. 54: disabling item 0 dims the
// menu title without hiding it. IM:V 1986 p. V-142 specifies the GetGray
// midpoint and the standard definition procedure's patterned fallback.
#[test]
fn native_draw_menu_bar_dims_disabled_titles_with_shared_gray_policy() {
    let pef = synthetic_pef_with_import(b"DrawMenuBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 128, b"File", b"Open");
    let city = install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1400, 129, b"City", b"Map");
    ppc_set_menu_item_enabled(
        &mut loaded.memory,
        &test_handle_records!(loaded),
        city,
        0,
        false,
    );

    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    let regions = ppc_menu_list_definition(&mut loaded.memory, menu_list_handle)
        .unwrap()
        .regular_title_regions();
    assert_eq!(regions.len(), 2);
    let baseline = i32::from(ppc_menu_bar_title_baseline(20));
    let title_glyph_pixels = |title: &[u8], origin_x: i16| {
        let mut cursor = i32::from(origin_x);
        let mut pixels = Vec::new();
        for byte in title {
            let (glyph, data) = get_glyph(
                PPC_QD_TEXT_FONT_DEFAULT,
                PPC_QD_TEXT_SIZE_SYSTEM,
                char::from(*byte),
            )
            .unwrap();
            for row in 0..glyph.height as usize {
                for column in 0..glyph.width as usize {
                    let index = glyph.data_offset + row * glyph.width as usize + column;
                    if index < data.len() && data[index] >= 128 {
                        pixels.push((
                            cursor + i32::from(glyph.origin_x) + column as i32,
                            baseline + i32::from(glyph.origin_y) + row as i32,
                        ));
                    }
                }
            }
            cursor += i32::from(glyph.advance);
        }
        assert!(!pixels.is_empty());
        pixels
    };
    let file_pixels = title_glyph_pixels(b"File", regions[0].title_origin());
    let city_pixels = title_glyph_pixels(b"City", regions[1].title_origin());

    for depth in [1, 8] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawMenuBar);

        let front = ppc_live_front_buffer_for_gworld(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
        )
        .unwrap();
        let black =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        let white =
            ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &loaded.screen_clut).unwrap();
        let gray = ppc_physical_screen_color_pixel(
            front,
            ppc_menu_rgb(MenuColorTable::dimmed([0; 3], [u16::MAX; 3])),
            &loaded.screen_clut,
        )
        .unwrap();
        let read = |memory: &mut PpcSectionMem, point: &(i32, i32)| {
            ppc_quickdraw_read_pixel(memory, front, *point).unwrap()
        };

        assert!(
            file_pixels
                .iter()
                .all(|point| read(&mut loaded.memory, point) == black),
            "{depth}bpp enabled title did not remain solid black",
        );
        if depth == 8 {
            assert_ne!(gray, black);
            assert_ne!(gray, white);
            assert!(
                city_pixels
                    .iter()
                    .all(|point| read(&mut loaded.memory, point) == gray),
                "8bpp disabled title did not use the GetGray midpoint",
            );
        } else {
            let city_ink = city_pixels
                .iter()
                .map(|point| read(&mut loaded.memory, point))
                .collect::<Vec<_>>();
            assert!(city_ink.contains(&black));
            assert!(city_ink.contains(&white));
        }
    }
}

// IM:I 1985 p. I-281 and IM:V 1986 p. V-120: the main-screen desktop
// excludes the standard rounded outer corners beneath the menu bar.
#[test]
fn native_draw_menu_bar_uses_the_shared_rounded_corner_mask() {
    let pef = synthetic_pef_with_import(b"DrawMenuBar");
    let mut loaded = load_pef_application(&pef).unwrap();

    for depth in [1, 2, 4, 8, 16] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawMenuBar);

        let front = ppc_live_front_buffer_for_gworld(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
        )
        .unwrap();
        let black =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        let white =
            ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &loaded.screen_clut).unwrap();
        let mut rounded_corner_pixels = Vec::new();
        for_each_standard_menu_bar_corner_pixel(
            ppc_u32_to_i16_saturating(front.width),
            |x, y| rounded_corner_pixels.push((x, y)),
        );
        let right_start = i32::try_from(front.width).unwrap() - 6;
        for y in 0..5 {
            for x in (0..6).chain(right_start..i32::try_from(front.width).unwrap()) {
                let expected = if rounded_corner_pixels.contains(&(x as i16, y as i16)) {
                    black
                } else {
                    white
                };
                assert_eq!(
                    ppc_quickdraw_read_pixel(&mut loaded.memory, front, (x, y)),
                    Some(expected),
                    "{depth}bpp rounded corner mask pixel ({x}, {y})",
                );
            }
        }
    }
}

// MTE 1992 pp. 3-109--3-110: DeleteMenu resolves duplicate IDs in the
// hierarchical section before it considers regular menu-bar entries.
#[test]
fn native_delete_menu_prefers_hierarchical_id_collisions() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 0x40]);
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch,
        b"Regular"
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x20,
        b"Submenu"
    ));
    let regular = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        200,
        scratch,
    );
    let hierarchical = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        200,
        scratch + 0x20,
    );
    for (handle, before_id) in [(regular, 0), (hierarchical, -1)] {
        assert_eq!(
            ppc_insert_menu(
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                test_handles!(loaded),
                &mut free_handle_blocks,
                handle,
                before_id,
            ),
            PPC_NO_ERR
        );
    }
    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    assert_eq!(
        ppc_get_menu_handle(&mut loaded.memory, menu_list_handle, 200),
        hierarchical,
        "GetMenuHandle must resolve the hierarchical collision first"
    );

    assert_eq!(
        ppc_delete_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            menu_list_handle,
            200,
        ),
        PPC_NO_ERR
    );
    let after_hierarchical_delete =
        ppc_menu_list_definition(&mut loaded.memory, menu_list_handle).unwrap();
    assert_eq!(
        after_hierarchical_delete
            .regular_handles()
            .collect::<Vec<_>>(),
        vec![regular]
    );
    assert!(after_hierarchical_delete.hierarchical.is_empty());
    assert_eq!(
        ppc_get_menu_handle(&mut loaded.memory, menu_list_handle, 200),
        regular
    );

    assert_eq!(
        ppc_delete_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            menu_list_handle,
            200,
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_menu_list_handles(&mut loaded.memory, menu_list_handle),
        Some(Vec::new())
    );
}

#[test]
fn native_insert_menu_does_not_evict_entries_from_a_full_partition() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 0x40]);
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch,
        b"Existing"
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x20,
        b"Insert"
    ));
    let existing = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        200,
        scratch,
    );
    let insertion = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        201,
        scratch + 0x20,
    );
    let mut regular = vec![
        PpcMenuListEntry {
            handle: 0xdead_0000,
            value: 0,
        };
        MAX_MENU_LIST_ENTRIES - 1
    ];
    regular.push(PpcMenuListEntry {
        handle: existing,
        value: 0,
    });
    let full = PpcMenuListDefinition {
        regular,
        ..PpcMenuListDefinition::default()
    };
    let current = ppc_alloc_menu_list_definition_handle(
        &full,
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
    );
    assert_ne!(current, 0);
    ppc_set_current_menu_list(&mut loaded.memory, current);

    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            insertion,
            200,
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_menu_list_definition(&mut loaded.memory, current),
        Some(full)
    );
}

#[test]
fn quickdraw_compatibility_menu_item_icon_and_style_accessors_round_trip() {
    let scratch = PPC_DATA_BASE + 0x1000;
    for (symbol, operation, initial, replacement, attribute_offset) in [
        (b"GetItemIcon".as_slice(), PpcQuickDrawCompatibilityOperation::GetItemIcon, 7u8, 0u8, 0u32),
        (b"SetItemIcon".as_slice(), PpcQuickDrawCompatibilityOperation::SetItemIcon, 7u8, 11u8, 0u32),
        (b"GetItemStyle".as_slice(), PpcQuickDrawCompatibilityOperation::GetItemStyle, 3u8, 0u8, 3u32),
        (b"SetItemStyle".as_slice(), PpcQuickDrawCompatibilityOperation::SetItemStyle, 3u8, 0x25u8, 3u32),
    ] {
        let pef = synthetic_pef_with_import(symbol);
        let mut loaded = load_pef_application(&pef).unwrap();
        assert_eq!(
            loaded.imports[0].dispatcher_target,
            PpcImportDispatcherTarget::QuickDrawCompatibility(operation),
        );
        let item = if attribute_offset == 0 {
            b"Item^7".as_slice()
        } else {
            b"Item<B<I".as_slice()
        };
        let menu = install_test_menu(&mut loaded, scratch, 128, b"File", item);
        let output = scratch + 0x300;
        loaded.memory.add_region(output, vec![0xa5; 4]);
        loaded.cpu.gpr[3] = menu;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = if symbol.starts_with(b"Get") {
            output
        } else {
            u32::from(replacement)
        };
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let address =
            ppc_menu_item_attribute_address(&mut loaded.memory, menu, 1, attribute_offset)
                .unwrap();
        if symbol.starts_with(b"Get") {
            assert_eq!(loaded.memory.read_u8(output), Some(initial));
        } else {
            assert_eq!(loaded.memory.read_u8(address), Some(replacement));
        }

        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[4] = 99;
        if symbol.starts_with(b"Get") {
            loaded.memory.write_u8(output, 0xa5).unwrap();
            loaded.cpu.gpr[5] = output;
        }
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        if symbol.starts_with(b"Get") {
            assert_eq!(loaded.memory.read_u8(output), Some(0));
        } else {
            assert_eq!(loaded.memory.read_u8(address), Some(replacement));
        }
    }
}

#[test]
fn menu_bar_title_baseline_tracks_the_live_menu_bar_height() {
    let metrics = crate::quickdraw::text::get_font_metrics(0, 12);
    for height in [12, 20, 30] {
        let baseline = ppc_menu_bar_title_baseline(height);
        let top = baseline - metrics.ascent;
        let bottom = height - baseline - metrics.descent;
        assert!(
            (top - bottom).abs() <= 1,
            "title must be vertically centered"
        );
    }
    assert_eq!(ppc_menu_bar_system_mark_top(12), 0);
    assert_eq!(ppc_menu_bar_system_mark_top(20), 3);
    assert_eq!(ppc_menu_bar_system_mark_top(30), 8);
}

#[test]
fn hilite_menu_selects_one_title_and_zero_restores_the_bar_at_supported_depths() {
    let pef = synthetic_pef_with_import(b"HiliteMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let title = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(title, vec![0; 16]);
    assert!(ppc_write_pstring_bytes(&mut loaded.memory, title, b"File"));
    let menu = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        128,
        title,
    );
    assert_ne!(menu, 0);
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            menu,
            0,
        ),
        PPC_NO_ERR
    );
    let title_glyph_pixels = {
        let (glyph, data) = get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, 'F').unwrap();
        let mut pixels = Vec::new();
        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let index = glyph.data_offset + row * glyph.width as usize + col;
                if index < data.len() && data[index] >= 128 {
                    pixels.push((
                        i32::from(glyph.origin_x) + col as i32,
                        i32::from(glyph.origin_y) + row as i32,
                    ));
                }
            }
        }
        assert!(!pixels.is_empty());
        pixels
    };
    for depth in [1, 2, 4, 8, 16] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        loaded.memory.write_u16_be(PPC_THE_MENU_ADDR, 0).unwrap();
        assert!(draw_current_test_menu_bar(&mut loaded));
        let front = ppc_live_front_buffer_for_gworld(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
        )
        .unwrap();
        assert_eq!(front.depth, depth);
        let menu_bar_bytes = front.row_bytes * 20;
        let normal =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes).unwrap();
        let black =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        let white =
            ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &loaded.screen_clut).unwrap();
        let title_origin = (
            i32::from(
                STANDARD_MENU_BAR_FIRST_TITLE_LEFT + STANDARD_MENU_BAR_TITLE_ORIGIN_INSET,
            ),
            i32::from(ppc_menu_bar_title_baseline(20)),
        );
        let normal_title_ink = title_glyph_pixels
            .iter()
            .filter(|&&(dx, dy)| {
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (title_origin.0 + dx, title_origin.1 + dy),
                ) == Some(black)
            })
            .count();
        assert_eq!(
            normal_title_ink,
            title_glyph_pixels.len(),
            "{depth}bpp menu title glyph did not draw in black",
        );

        loaded.cpu.gpr[3] = 128;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteMenu);

        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(128));
        let highlighted =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes).unwrap();
        assert_ne!(highlighted, normal, "{depth}bpp title did not highlight");
        let highlighted_title_ink = title_glyph_pixels
            .iter()
            .filter(|&&(dx, dy)| {
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (title_origin.0 + dx, title_origin.1 + dy),
                ) == Some(white)
            })
            .count();
        assert_eq!(
            highlighted_title_ink,
            title_glyph_pixels.len(),
            "{depth}bpp highlighted menu title glyph did not reverse to white",
        );

        loaded.cpu.gpr[3] = 128;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteMenu);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
            Some(highlighted),
            "{depth}bpp repeated HiliteMenu changed the title"
        );

        loaded.cpu.gpr[3] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::HiliteMenu);
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
            Some(normal),
            "{depth}bpp HiliteMenu(0) did not restore the menu bar"
        );
    }
}

// MTE 1992 pp. 3-141--3-142 and IM:V 1986 p. V-246: zero or an
// unknown ID reverses the complete bar, while a regular ID toggles that
// title and restores any different title first.
#[test]
fn native_flash_menu_bar_matches_whole_bar_and_title_semantics_at_supported_depths() {
    let pef = synthetic_pef_with_import(b"FlashMenuBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 128, b"File", b"Open");
    install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1400, 129, b"Edit", b"Cut");
    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    let regions = ppc_menu_list_definition(&mut loaded.memory, menu_list_handle)
        .unwrap()
        .regular_title_regions();
    assert_eq!(regions.len(), 2);

    for depth in [1, 2, 4, 8, 16] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        loaded.memory.write_u16_be(PPC_THE_MENU_ADDR, 0).unwrap();
        assert!(draw_current_test_menu_bar(&mut loaded));

        let front = ppc_live_front_buffer_for_gworld(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
        )
        .unwrap();
        let black =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        let white =
            ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &loaded.screen_clut).unwrap();
        let menu_bar_bytes = front.row_bytes * 20;
        let normal =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes).unwrap();
        let file_cell = (i32::from(regions[0].left.saturating_add(3)), 2);
        let edit_cell = (i32::from(regions[1].left.saturating_add(3)), 2);
        let bar_point = (200, 5);
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, file_cell),
            Some(white),
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, edit_cell),
            Some(white),
        );

        loaded.cpu.gpr[3] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, bar_point),
            Some(black),
            "{depth}bpp FlashMenuBar(0) did not reverse the bar background",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 0)),
            Some(black),
            "{depth}bpp FlashMenuBar(0) did not preserve the corner mask",
        );
        let whole_bar_flipped =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes).unwrap();
        assert_ne!(whole_bar_flipped, normal);

        loaded.cpu.gpr[3] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
            Some(normal.clone()),
            "{depth}bpp two whole-bar flashes did not restore the framebuffer",
        );

        loaded.cpu.gpr[3] = 999;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(loaded.cpu.gpr[3], 999);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
            Some(whole_bar_flipped),
            "{depth}bpp unknown menu ID did not flash the complete bar",
        );
        loaded.cpu.gpr[3] = 999;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
            Some(normal.clone()),
        );

        loaded.cpu.gpr[3] = 128;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(128));
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, file_cell),
            Some(black),
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, edit_cell),
            Some(white),
        );

        loaded.cpu.gpr[3] = 128;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
            Some(normal.clone()),
            "{depth}bpp flashing the selected title did not toggle it off",
        );

        loaded.cpu.gpr[3] = 128;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        loaded.cpu.gpr[3] = 129;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(129));
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, file_cell),
            Some(white),
            "{depth}bpp previous title was not restored",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, edit_cell),
            Some(black),
            "{depth}bpp requested title was not highlighted",
        );

        loaded.cpu.gpr[3] = 129;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::FlashMenuBar);
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, menu_bar_bytes),
            Some(normal),
            "{depth}bpp title switching did not restore the original bar",
        );
    }
}

#[test]
fn tracked_menu_draws_standard_item_chrome_and_dims_disabled_rows() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"File",
        b"Checked!\x12;Command/C;Disabled/C!\x12(;-;Parent/\x1b!\xc9",
    );
    ppc_calc_menu_size(&mut loaded.memory, menu);

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert!(draw_current_test_menu_bar(&mut loaded));
    let framebuffer_len = front.row_bytes * front.height;
    let before =
        ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len).unwrap();
    ppc_track_menu_while_held(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        &mut loaded.toolbox_startup,
        12,
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 10,
            mouse_h: 12,
            ..PpcInputSnapshot::default()
        },
    );
    let tracking = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
    let command_text_advance = ppc_text_bytes_advance_for_font(
        b"Command",
        PPC_QD_TEXT_FONT_DEFAULT,
        PPC_QD_TEXT_SIZE_SYSTEM,
    );
    assert!(
        tracking.popup_width >= command_text_advance.saturating_add(53),
        "command-key column overlaps item text"
    );
    let black =
        ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
    let white =
        ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &loaded.screen_clut).unwrap();
    let gray = ppc_physical_screen_color_pixel(
        front,
        ppc_menu_rgb(MenuColorTable::dimmed([0; 3], [u16::MAX; 3])),
        &loaded.screen_clut,
    )
    .unwrap();
    let glyph_pixels = |ch| {
        let (glyph, data) = get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, ch).unwrap();
        let mut pixels = Vec::new();
        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let index = glyph.data_offset + row * glyph.width as usize + col;
                if index < data.len() && data[index] >= 128 {
                    pixels.push((
                        i32::from(glyph.origin_x) + col as i32,
                        i32::from(glyph.origin_y) + row as i32,
                    ));
                }
            }
        }
        pixels
    };
    let assert_glyph_color =
        |memory: &mut PpcSectionMem, origin: (i32, i32), ch: char, pixel: u16| {
            let glyph = glyph_pixels(ch);
            assert!(!glyph.is_empty());
            assert!(glyph.iter().all(|(dx, dy)| {
                ppc_quickdraw_read_pixel(memory, front, (origin.0 + dx, origin.1 + dy))
                    == Some(pixel)
            }));
        };

    let row_top =
        |item: i16| tracking.popup_top + ppc_tracked_menu_item_offset(&tracking, item);
    let command_layout = standard_menu_item_layout(
        (
            tracking.popup_left,
            tracking.popup_left + tracking.popup_width,
        ),
        (row_top(2), ppc_tracked_menu_item_height(&tracking, 2)),
        StandardMenuIconKind::None,
        false,
        {
            let metrics = get_font_metrics(PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM);
            (metrics.ascent, metrics.descent)
        },
        true,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(command_layout.text_left),
            i32::from(command_layout.text_baseline),
        ),
        'C',
        black,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + 3),
            i32::from(row_top(1) + 11),
        ),
        '\u{2713}',
        black,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + tracking.popup_width - 25),
            i32::from(row_top(2) + 11),
        ),
        '\u{2318}',
        black,
    );
    let command_symbol_advance = i32::from(
        get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, '\u{2318}')
            .unwrap()
            .0
            .advance,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + tracking.popup_width - 25) + command_symbol_advance,
            i32::from(row_top(2) + 11),
        ),
        'C',
        black,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + 15),
            i32::from(row_top(3) + 11),
        ),
        'D',
        gray,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + 3),
            i32::from(row_top(3) + 11),
        ),
        '\u{2713}',
        gray,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + tracking.popup_width - 25),
            i32::from(row_top(3) + 11),
        ),
        '\u{2318}',
        gray,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + tracking.popup_width - 25) + command_symbol_advance,
            i32::from(row_top(3) + 11),
        ),
        'C',
        gray,
    );
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            front,
            (
                i32::from(tracking.popup_left + 10),
                i32::from(row_top(4) + 2),
            ),
        ),
        Some(gray)
    );
    let hierarchy_ink = (0..7i16)
        .flat_map(|dx| (-3..=3).map(move |dy| (dx, dy)))
        .filter(|(dx, dy)| {
            ppc_quickdraw_read_pixel(
                &mut loaded.memory,
                front,
                (
                    i32::from(tracking.popup_left + tracking.popup_width - 12 + *dx),
                    i32::from(row_top(5) + 8 + *dy),
                ),
            ) == Some(black)
        })
        .count();
    assert!(hierarchy_ink >= 7, "hierarchical indicator did not draw");

    let disabled_hover = PpcInputSnapshot {
        mouse_button: true,
        mouse_v: row_top(3) + 4,
        mouse_h: tracking.popup_left + 20,
        ..PpcInputSnapshot::default()
    };
    assert_eq!(
        ppc_menu_tracking_item(&mut loaded.memory, &tracking, disabled_hover),
        0
    );

    let selected = PpcInputSnapshot {
        mouse_button: true,
        mouse_v: row_top(1) + 4,
        mouse_h: tracking.popup_left + 20,
        ..PpcInputSnapshot::default()
    };
    ppc_track_menu_while_held(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        &mut loaded.toolbox_startup,
        12,
        selected,
    );
    assert_glyph_color(
        &mut loaded.memory,
        (
            i32::from(tracking.popup_left + 3),
            i32::from(row_top(1) + 11),
        ),
        '\u{2713}',
        white,
    );

    assert_eq!(
        ppc_finish_menu_bar_tracking(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            PpcInputSnapshot {
                mouse_v: tracking.popup_top + 8,
                mouse_h: tracking.popup_left - 1,
                ..PpcInputSnapshot::default()
            },
        ),
        Some(0)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
        Some(before)
    );
}

#[test]
fn native_popup_draws_from_the_live_shared_menu_color_table() {
    let pef = synthetic_pef_with_import(b"PopUpMenuSelect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let menu =
        install_test_popup_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 300, b"Popup", b"Color");
    loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = 1;
    loaded.cpu.gpr[6] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);

    let set_rgb = |entry: &mut [u8; MENU_COLOR_ENTRY_SIZE], offset: usize, rgb: [u16; 3]| {
        for (channel, value) in rgb.into_iter().enumerate() {
            let channel_offset = offset + channel * 2;
            entry[channel_offset..channel_offset + 2].copy_from_slice(&value.to_be_bytes());
        }
    };
    let menu_background = [0xffff, 0x1111, 0x1111];
    let item_name = [0x1111, 0x1111, 0xffff];
    let mut title = test_menu_color_entry(300, 0, 0);
    set_rgb(&mut title, 22, menu_background);
    let mut item = test_menu_color_entry(300, 1, 0);
    set_rgb(&mut item, 10, item_name);
    set_rgb(&mut item, 22, menu_background);
    let color_bytes = [title.as_slice(), item.as_slice()].concat();
    let color_handle = ppc_ensure_menu_color_table_handle(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
    );
    assert_ne!(color_handle, 0);
    assert_eq!(
        ppc_replace_menu_bytes(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            color_handle,
            &color_bytes,
        ),
        PPC_NO_ERR,
    );

    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PopUpMenuSelect;
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = menu;
    loaded.cpu.gpr[4] = 100;
    loaded.cpu.gpr[5] = 80;
    loaded.cpu.gpr[6] = 0;
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));

    let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let background_pixel = ppc_physical_screen_color_pixel(
        front,
        ppc_menu_rgb(menu_background),
        &loaded.screen_clut,
    )
    .unwrap();
    let name_pixel =
        ppc_physical_screen_color_pixel(front, ppc_menu_rgb(item_name), &loaded.screen_clut)
            .unwrap();
    assert_ne!(background_pixel, name_pixel);
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            front,
            (
                i32::from(tracking.popup_left + tracking.popup_width - 3),
                i32::from(tracking.popup_top + 2),
            ),
        ),
        Some(background_pixel),
    );

    let metrics = get_font_metrics(PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM);
    let layout = standard_menu_item_layout(
        (
            tracking.popup_left,
            tracking.popup_left + tracking.popup_width,
        ),
        (tracking.content_top, tracking.item_appearances[0].height),
        StandardMenuIconKind::None,
        false,
        (metrics.ascent, metrics.descent),
        false,
    );
    let (glyph, data) = get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, 'C').unwrap();
    let (dx, dy) = (0..glyph.height as usize)
        .flat_map(|row| (0..glyph.width as usize).map(move |column| (column, row)))
        .find(|(column, row)| {
            data.get(glyph.data_offset + row * glyph.width as usize + column)
                .is_some_and(|alpha| *alpha >= 128)
        })
        .unwrap();
    assert_eq!(
        ppc_quickdraw_read_pixel(
            &mut loaded.memory,
            front,
            (
                i32::from(layout.text_left) + i32::from(glyph.origin_x) + dx as i32,
                i32::from(layout.text_baseline) + i32::from(glyph.origin_y) + dy as i32,
            ),
        ),
        Some(name_pixel),
    );
}

#[test]
fn tracked_menu_renders_each_standard_text_style() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"File",
        b"Menu;Menu;Menu;Menu;Menu;Menu;Menu;Menu",
    );
    let styles = [
        0,
        QuickDrawTextStyle::BOLD_BIT,
        QuickDrawTextStyle::ITALIC_BIT,
        QuickDrawTextStyle::UNDERLINE_BIT,
        QuickDrawTextStyle::OUTLINE_BIT,
        QuickDrawTextStyle::SHADOW_BIT,
        QuickDrawTextStyle::CONDENSE_BIT,
        QuickDrawTextStyle::EXTEND_BIT,
    ];
    for (item, style) in (1i16..).zip(styles) {
        let address =
            ppc_menu_item_attribute_address(&mut loaded.memory, menu, item, 3).unwrap();
        loaded.memory.write_u8(address, style).unwrap();
    }

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let appearances = ppc_menu_item_appearances(&mut loaded.memory, menu, &[], 0);
    assert_eq!(
        appearances
            .iter()
            .map(|item| item.height)
            .collect::<Vec<_>>(),
        vec![16, 16, 16, 16, 16, 21, 16, 16],
    );
    let state = ppc_begin_tracked_menu_with_appearances(
        &mut loaded.memory,
        front,
        MenuTrackingKind::MenuBar,
        menu,
        11,
        20,
        20,
        120,
        ppc_menu_appearance_height(&appearances),
        0,
        appearances,
    )
    .unwrap();
    ppc_draw_tracked_menu(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        MenuColorTable::new(&[]),
        StandardMenuPaneKind::PullDown,
        &state,
        0,
    );
    let pixels_for_item = |memory: &mut PpcSectionMem, item: i16| {
        let top = state.popup_top + ppc_tracked_menu_item_offset(&state, item);
        let mut pixels = Vec::with_capacity(16 * 48);
        for y in 0..16i32 {
            for x in 0..48i32 {
                pixels.push(
                    ppc_quickdraw_read_pixel(
                        memory,
                        front,
                        (i32::from(state.popup_left + 18) + x, i32::from(top) + y),
                    )
                    .unwrap(),
                );
            }
        }
        pixels
    };
    let plain = pixels_for_item(&mut loaded.memory, 1);
    for item in 2..=8 {
        assert_ne!(
            pixels_for_item(&mut loaded.memory, item),
            plain,
            "style bit for item {item} did not change its rendered pixels",
        );
    }
    for item in 1..=8 {
        let top = state.popup_top + ppc_tracked_menu_item_offset(&state, item);
        assert_eq!(
            ppc_menu_tracking_item(
                &mut loaded.memory,
                &state,
                PpcInputSnapshot {
                    mouse_button: true,
                    mouse_v: top + ppc_tracked_menu_item_height(&state, item) / 2,
                    mouse_h: state.popup_left + 20,
                    ..PpcInputSnapshot::default()
                },
            ),
            item,
        );
    }
}

#[test]
fn tracked_menu_omits_trailing_separator_rows() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"Apple",
        b"About;-",
    );

    let appearances = ppc_menu_item_appearances(&mut loaded.memory, menu, &[], 0);
    assert_eq!(appearances.len(), 1);
    assert_eq!(ppc_menu_appearance_height(&appearances), 16);
    ppc_calc_menu_size(&mut loaded.memory, menu);
    let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
    assert_eq!(loaded.memory.read_u16_be(menu_ptr + 4), Some(16));
}

#[test]
fn native_calc_menu_size_uses_shared_macos_8_1_width_policy() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"File",
        b"Alpha;Parent/\x1b",
    );
    let parent_width = ppc_text_bytes_advance_for_font(
        b"Parent",
        PPC_QD_TEXT_FONT_DEFAULT,
        PPC_QD_TEXT_SIZE_SYSTEM,
    );
    assert_eq!(
        parent_width,
        TrapDispatcher::fb_measure_string("Parent", 0, 12),
        "PPC and 68k adapters should resolve the same bundled font metric",
    );
    assert_eq!(
        ppc_menu_item(&mut loaded.memory, menu, 2).and_then(|(address, length)| {
            ppc_memory_read_bytes(&mut loaded.memory, address + 1, u32::from(length))
        }),
        Some(b"Parent".to_vec()),
    );

    ppc_calc_menu_size(&mut loaded.memory, menu);

    let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
    assert_eq!(
        loaded.memory.read_u16_be(menu_ptr + 2),
        Some((parent_width + 64) as u16),
    );
    assert_eq!(loaded.memory.read_u16_be(menu_ptr + 4), Some(32));
}

#[test]
fn native_calc_menu_size_caps_oversized_menu_height_from_profile_geometry() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let description = (0..40).map(|_| "A").collect::<Vec<_>>().join(";");
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"File",
        description.as_bytes(),
    );

    ppc_calc_menu_size(&mut loaded.memory, menu);

    let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
    assert_eq!(loaded.memory.read_u16_be(menu_ptr + 4), Some(560));
}

#[test]
fn native_calc_menu_size_calls_powerpc_custom_mdef_size_message() {
    let pef = synthetic_pef_with_import(b"CalcMenuSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"Custom",
        b"Ignored",
    );
    let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
    let mdef_handle = PPC_DATA_BASE + 0x7000;
    let descriptor = PPC_DATA_BASE + 0x7100;
    let tvector = PPC_DATA_BASE + 0x7180;
    let callback_entry = PPC_CODE_BASE + 0x4000;
    let callback_rtoc = PPC_DATA_BASE + 0x7300;
    loaded.memory.add_region(mdef_handle, vec![0; 4]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        test_stack_proc_info(
            PPC_PROCINFO_SIZE_NONE,
            &[
                PPC_PROCINFO_SIZE_TWO,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
            ],
        ),
        &[
            d_form_u(32, 6, 4, 0),
            d_form_u(14, 7, 0, 123),
            d_form_u(44, 7, 6, 2),
            d_form_u(14, 7, 0, 45),
            d_form_u(44, 7, 6, 4),
            BLR,
        ],
    );
    loaded.memory.write_u32_be(mdef_handle, descriptor).unwrap();
    loaded
        .memory
        .write_u32_be(menu_ptr + 6, mdef_handle)
        .unwrap();

    loaded.cpu.gpr[3] = menu;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CalcMenuSize);

    assert_eq!(loaded.memory.read_u16_be(menu_ptr + 2), Some(123));
    assert_eq!(loaded.memory.read_u16_be(menu_ptr + 4), Some(45));
    assert!(loaded
        .process_memory_manager
        .0
        .borrow()
        .native_ptr_records()
        .iter()
        .all(|record| record.ptr != loaded.cpu.gpr[5]));
    assert!(loaded.guest_calls().is_empty());
}

#[test]
fn nested_native_mdef_preserves_outer_by_reference_arguments() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CalcMenuSize")).unwrap();
    let outer = install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 140, b"Outer", b"A");
    let inner = install_test_menu(&mut loaded, PPC_DATA_BASE + 0x2000, 141, b"Inner", b"B");
    let marker = PPC_DATA_BASE + 0x7800;
    loaded.memory.add_region(marker, vec![0; 4]);
    let outer_code = [
        0x9421_ffa0, // stwu r1,-96(r1)
        0x7c08_02a6, // mflr r0
        0x9001_0064, // stw r0,100(r1)
        0x90a1_0038, // stw r5,56(r1): retain outer menuRect pointer
        0x3920_1122, // li r9,0x1122
        0xb125_0000, // sth r9,0(r5)
        0x3c60_0000 | (inner >> 16),
        0x6063_0000 | (inner & 0xffff),
        0x3d80_0000 | (PPC_IMPORT_TRAP_BASE >> 16),
        0x618c_0000 | (PPC_IMPORT_TRAP_BASE & 0xffff),
        0x7d89_03a6, // mtctr r12
        0x4e80_0421, // bctrl: nested CalcMenuSize
        0x80a1_0038, // lwz r5,56(r1)
        0xa125_0000, // lhz r9,0(r5)
        0x3d40_0000 | (marker >> 16),
        0x614a_0000 | (marker & 0xffff),
        0x912a_0000, // stw r9,0(r10)
        0x8001_0064, // lwz r0,100(r1)
        0x7c08_03a6, // mtlr r0
        0x3821_0060, // addi r1,r1,96
        BLR,
    ];
    let inner_code = [0x3920_3344, 0xb125_0000, BLR];
    for (index, menu, code) in [
        (0, outer, outer_code.as_slice()),
        (1, inner, inner_code.as_slice()),
    ] {
        let storage = PPC_DATA_BASE + 0x7000 + index * 0x400;
        loaded.memory.add_region(storage, vec![0; 4]);
        install_test_powerpc_callback(
            &mut loaded,
            storage + 0x100,
            storage + 0x180,
            PPC_CODE_BASE + 0x4000 + index * 0x100,
            0,
            0x0000_ff80,
            code,
        );
        loaded
            .memory
            .write_u32_be(storage, storage + 0x100)
            .unwrap();
        let record = loaded.memory.read_u32_be(menu).unwrap();
        loaded.memory.write_u32_be(record + 6, storage).unwrap();
    }
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = outer;
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
    assert!(loaded.guest_calls().is_empty());
    assert_eq!(
        loaded.memory.read_u32_be(marker),
        Some(0x1122),
        "nested MDEF scratch must not alias its caller"
    );
}

#[test]
fn nested_classic_mdef_adapters_keep_live_workspaces_disjoint() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"CalcMenuSize")).unwrap();
    let menu = install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 140, b"Custom", b"A");
    let handle = PPC_DATA_BASE + 0x7000;
    let descriptor = handle + 0x100;
    loaded.memory.add_region(handle, vec![0; 4]);
    install_test_m68k_callback(
        &mut loaded,
        descriptor,
        PPC_CODE_BASE + 0x4000,
        MenuDefinitionInvocation::PASCAL_PROC_INFO,
        0,
        &[0x4e74, 18],
    );
    loaded.memory.write_u32_be(handle, descriptor).unwrap();
    let record = loaded.memory.read_u32_be(menu).unwrap();
    loaded.memory.write_u32_be(record + 6, handle).unwrap();
    let manager_handle = loaded.process_memory_manager.0.clone();
    let mut manager = manager_handle.borrow_mut();
    let initial_count = manager.native_ptr_records().len();
    let heap = manager.native_heap_state().unwrap();
    let mut cursor = heap.heap_cursor;
    let invocation = MenuDefinitionInvocation {
        message: crate::menu_manager::MenuDefinitionMessage::Size,
        menu_handle: menu,
        menu_rect: (1, 2, 3, 4),
        hit_point: 0,
        which_item: 0,
    };
    let mut intervals = Vec::new();
    for index in 0..2 {
        let action = ppc_dispatch_native_menu_definition(
            &mut loaded.cpu,
            Some(manager.native_mut()),
            &mut loaded.memory,
            &mut cursor,
            heap.heap_limit,
            &loaded.process_file_system.vfs_resources,
            &mut loaded.toolbox_startup,
            invocation,
            PPC_HALT_PC,
        )
        .unwrap();
        assert!(matches!(action, PpcImportAction::Halt));
        let pending = loaded.toolbox_startup.execution.calls().activate_m68k().unwrap();
        loaded
            .memory
            .write_u32_be(pending.initial_sp - 100, 0x1122 + index)
            .unwrap();
        intervals.push(pending);
    }
    let outer = intervals[0];
    let inner = intervals[1];
    assert_ne!(outer.entry, inner.entry);
    assert!(outer.final_sp <= inner.entry || inner.final_sp <= outer.entry);
    assert_eq!(
        loaded.memory.read_u32_be(outer.initial_sp - 100),
        Some(0x1122)
    );
    assert_eq!(
        loaded.memory.read_u32_be(inner.initial_sp - 100),
        Some(0x1123)
    );
    assert_eq!(manager.native_ptr_records().len(), initial_count + 2);
    for (index, pending) in intervals.into_iter().rev().enumerate() {
        assert!(loaded
            .toolbox_startup.execution.calls()
            .complete_m68k_operation_for_powerpc(
                pending.return_pc,
                pending.final_sp,
                None,
                &mut loaded.cpu,
                &mut loaded.memory,
                manager.native_mut(),
            ));
        assert_eq!(
            manager.native_ptr_records().len(),
            initial_count + 1 - index
        );
    }
    assert!(loaded.toolbox_startup.execution.calls().is_empty());
    assert_eq!(loaded.toolbox_startup.mixed_mode_m68k.storage_pair(), (0, 0));
}

#[test]
fn native_custom_mdef_adapter_marshals_shared_choose_invocation() {
    let pef = synthetic_pef_with_import(b"CalcMenuSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"Custom",
        b"Ignored",
    );
    let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
    let mdef_handle = PPC_DATA_BASE + 0x7000;
    let descriptor = PPC_DATA_BASE + 0x7100;
    let tvector = PPC_DATA_BASE + 0x7180;
    let callback_entry = PPC_CODE_BASE + 0x4000;
    let callback_rtoc = PPC_DATA_BASE + 0x7300;
    loaded.memory.add_region(mdef_handle, vec![0; 4]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        test_stack_proc_info(
            PPC_PROCINFO_SIZE_NONE,
            &[
                PPC_PROCINFO_SIZE_TWO,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
            ],
        ),
        &[BLR],
    );
    loaded.memory.write_u32_be(mdef_handle, descriptor).unwrap();
    loaded
        .memory
        .write_u32_be(menu_ptr + 6, mdef_handle)
        .unwrap();

    let invocation = MenuDefinitionInvocation {
        message: crate::menu_manager::MenuDefinitionMessage::Choose,
        menu_handle: menu,
        menu_rect: (20, 30, 120, 180),
        hit_point: 0x0050_0060,
        which_item: 4,
    };
    let final_pc = 0x1234_5678;
    let manager_handle = loaded.process_memory_manager.0.clone();
    let mut manager = manager_handle.borrow_mut();
    let heap = manager.native_heap_state().unwrap();
    let mut cursor = heap.heap_cursor;
    let action = ppc_dispatch_native_menu_definition(
        &mut loaded.cpu,
        Some(manager.native_mut()),
        &mut loaded.memory,
        &mut cursor,
        heap.heap_limit,
        &loaded.process_file_system.vfs_resources,
        &mut loaded.toolbox_startup,
        invocation,
        final_pc,
    )
    .expect("native custom MDEF should be callable");

    assert!(matches!(action, PpcImportAction::Continue));
    assert_eq!(loaded.cpu.pc, callback_entry);
    assert_eq!(loaded.cpu.gpr[2], callback_rtoc);
    assert_eq!(loaded.toolbox_startup.execution.calls().len(), 1);
    let scratch = loaded.cpu.gpr[5];
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, scratch, 10),
        Some(invocation.scratch_bytes().to_vec())
    );
    assert_eq!(
        &loaded.cpu.gpr[3..8],
        &[1, menu, scratch, invocation.hit_point, scratch + 8,]
    );
    loaded.cpu.pc = PPC_GUEST_CALL_RETURN_PC;
    assert!(loaded
        .toolbox_startup.execution.calls()
        .complete_powerpc_resuming_operation(
            &mut loaded.cpu,
            manager.native_mut(),
            |operation, result| {
                let crate::guest_call::ManagerContinuation::Menu(
                    crate::guest_call::MenuManagerContinuation::Definition(operation),
                ) = operation
                else {
                    panic!("MDEF continuation");
                };
                operation.complete(&mut loaded.memory);
                result
            },
        ));
    assert_eq!(loaded.cpu.pc, final_pc);
    assert!(manager
        .native_ptr_records()
        .iter()
        .all(|record| record.ptr != scratch));
}

#[test]
fn configured_native_startup_keeps_empty_menu_ownership_pristine() {
    let pef = synthetic_pef_with_import(b"InitMenus");
    for screen_depth in [1, 2, 4, 8, 16] {
        let mut loaded = load_pef_application_with_config(
            &pef,
            PpcLoadConfig {
                screen_depth,
                ..PpcLoadConfig::default()
            },
        )
        .unwrap();
        assert!(
            loaded.toolbox_startup.execution.calls().is_pristine(),
            "depth {screen_depth}"
        );
        let mut context = crate::process_context::ProcessContext::default();
        loaded.attach_unconverted_process_services(&mut context);
        assert!(loaded.toolbox_startup.execution.menu().is_none());
        assert!(loaded.guest_calls().is_pristine());
        assert!(context.menu_tracking().is_none());
    }
}

#[test]
fn nested_native_menu_select_preserves_outer_tracking_and_return() {
    for (nested_point, nested_result) in
        [(u32::MAX, 0), ((10u32 << 16) | 12, (131u32 << 16) | 1)]
    {
        let pef = synthetic_pef_with_import(b"MenuSelect");
        let mut loaded = load_pef_application(&pef).unwrap();
        let menu = install_test_menu(
            &mut loaded,
            PPC_DATA_BASE + 0x1000,
            131,
            b"Custom",
            b"Opaque definition data",
        );
        let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
        loaded.memory.write_u16_be(menu_ptr + 2, 80).unwrap();
        loaded.memory.write_u16_be(menu_ptr + 4, 32).unwrap();
        let mdef_handle = PPC_DATA_BASE + 0x7000;
        let descriptor = PPC_DATA_BASE + 0x7100;
        let tvector = PPC_DATA_BASE + 0x7180;
        let callback_entry = PPC_CODE_BASE + 0x4000;
        loaded.memory.add_region(mdef_handle, vec![0; 4]);
        let marker = PPC_DATA_BASE + 0x7800;
        loaded.memory.add_region(marker, vec![0; 8]);
        loaded.memory.write_u32_be(marker + 4, u32::MAX).unwrap();
        let mut code = vec![
            0x9421_ffa0, // stwu r1,-96(r1)
            0x7c08_02a6, // mflr r0
            0x9001_0064, // stw r0,100(r1)
            0x90e1_0038, // save whichItem pointer
            0x3d20_0000 | (marker >> 16),
            0x6129_0000 | (marker & 0xffff),
            0x8109_0000, // lwz r8,0(r9)
            0x2c08_0000, // cmpwi r8,0
            0,           // bne: only the first callback starts a nested entry
            0x3900_0001,
            0x9109_0000,
            0x3c60_0000 | (nested_point >> 16),
            0x6063_0000 | (nested_point & 0xffff),
            0x3d80_0000 | (PPC_IMPORT_TRAP_BASE >> 16),
            0x618c_0000 | (PPC_IMPORT_TRAP_BASE & 0xffff),
            0x7d89_03a6,
            0x4e80_0421,
            0x3d20_0000 | (marker >> 16),
            0x6129_0000 | (marker & 0xffff),
            0x9069_0004, // record nested result
            0x80e1_0038,
            0x3900_0001,
            0xb107_0000, // choose first outer item
            0x8001_0064,
            0x7c08_03a6,
            0x3821_0060,
            BLR,
        ];
        code[8] = 0x4082_0000 | ((20 - 8) * 4);
        install_test_powerpc_callback(
            &mut loaded,
            descriptor,
            tvector,
            callback_entry,
            PPC_DATA_BASE + 0x7300,
            test_stack_proc_info(
                PPC_PROCINFO_SIZE_NONE,
                &[
                    PPC_PROCINFO_SIZE_TWO,
                    PPC_PROCINFO_SIZE_FOUR,
                    PPC_PROCINFO_SIZE_FOUR,
                    PPC_PROCINFO_SIZE_FOUR,
                    PPC_PROCINFO_SIZE_FOUR,
                ],
            ),
            &code,
        );
        loaded.memory.write_u32_be(mdef_handle, descriptor).unwrap();
        loaded
            .memory
            .write_u32_be(menu_ptr + 6, mdef_handle)
            .unwrap();

        loaded
            .current_gworld
            .with_mut(|current_gworld| *current_gworld = 0x1234_0000);
        loaded
            .current_gdevice
            .with_mut(|current_gdevice| *current_gdevice = 0x1234_1000);
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let restored_start = front.base_addr + 20 * front.row_bytes;
        let restored_len = front.row_bytes * (front.height - 20);
        let before =
            ppc_memory_read_bytes(&mut loaded.memory, restored_start, restored_len).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = (10u32 << 16) | 12;
        loaded
            .memory
            .write_u16_be(crate::memory::globals::addr::MENU_FLASH, 0)
            .unwrap();
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 28,
            mouse_h: 20,
            ..PpcInputSnapshot::default()
        });
        for _ in 0..16 {
            let probe = loaded.run_with_hle_imports(256);
            assert_eq!(probe.unsupported_import_index, None);
            if matches!(probe.result, PpcRunResult::Halted { .. }) {
                break;
            }
        }
        assert_eq!(
            loaded.memory.read_u32_be(marker + 4),
            Some(nested_result),
            "nested selection result"
        );
        assert_eq!(loaded.cpu.pc, PPC_HALT_PC, "outer caller return");
        assert_eq!(loaded.cpu.gpr[3], (131u32 << 16) | 1, "outer selection");
        assert!(loaded.guest_calls().is_empty());
        assert!(loaded.toolbox_startup.execution.menu().is_none());
        assert_eq!(*loaded.current_gworld, 0x1234_0000);
        assert_eq!(*loaded.current_gdevice, 0x1234_1000);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, restored_start, restored_len),
            Some(before),
            "nested menus restore the original pixels"
        );
    }
}

#[test]
fn native_menu_select_retains_custom_mdef_draw_and_choose_until_release() {
    let pef = synthetic_pef_with_import(b"MenuSelect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        131,
        b"Custom",
        b"Opaque definition data",
    );
    let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
    loaded.memory.write_u16_be(menu_ptr + 2, 80).unwrap();
    loaded.memory.write_u16_be(menu_ptr + 4, 32).unwrap();
    let mdef_handle = PPC_DATA_BASE + 0x7000;
    let descriptor = PPC_DATA_BASE + 0x7100;
    let tvector = PPC_DATA_BASE + 0x7180;
    let callback_entry = PPC_CODE_BASE + 0x4000;
    loaded.memory.add_region(mdef_handle, vec![0; 4]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        PPC_DATA_BASE + 0x7300,
        test_stack_proc_info(
            PPC_PROCINFO_SIZE_NONE,
            &[
                PPC_PROCINFO_SIZE_TWO,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
            ],
        ),
        &[d_form_u(14, 8, 0, 2), d_form_u(44, 8, 7, 0), BLR],
    );
    loaded.memory.write_u32_be(mdef_handle, descriptor).unwrap();
    loaded
        .memory
        .write_u32_be(menu_ptr + 6, mdef_handle)
        .unwrap();

    let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    let framebuffer_len = front.row_bytes * front.height;
    let before =
        ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len).unwrap();
    let return_address = loaded.cpu.lr;
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = 0x1234_0000);
    loaded
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = 0x1234_1000);
    loaded.cpu.gpr[3] = (10u32 << 16) | 12;
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        mouse_v: 28,
        mouse_h: 20,
        ..PpcInputSnapshot::default()
    });

    let tick = loaded.current_tick().wrapping_add(1);
    loaded.set_tick_count(tick);
    let probe = loaded.run_with_hle_imports(256);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(
        loaded
            .toolbox_startup
            .active_menu_definition()
            .unwrap()
            .which_item(),
        2
    );
    assert_eq!(*loaded.current_gworld, PPC_MAIN_GWORLD);
    assert_eq!(*loaded.current_gdevice, PPC_MAIN_GDEVICE);
    assert_ne!(
        ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
        Some(before.clone())
    );
    let tracking = loaded.toolbox_startup.execution.menu().snapshot().unwrap();

    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: false,
        mouse_v: 28,
        mouse_h: 20,
        ..PpcInputSnapshot::default()
    });
    let tick = loaded.current_tick().wrapping_add(1);
    loaded.set_tick_count(tick);
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(
        probe.handled_import_count, 0,
        "retained custom tracking bypasses public entry"
    );
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(
        loaded
            .toolbox_startup
            .execution.menu()
            .as_ref()
            .unwrap()
            .flash_remaining,
        6
    );
    let mut phases = vec![6];
    for _ in 0..64 {
        let tick = loaded.current_tick().wrapping_add(1);
        loaded.set_tick_count(tick);
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.handled_import_count, 0, "retained custom tracking bypasses public entry");
        let remaining = loaded
            .toolbox_startup
            .execution.menu()
            .as_ref()
            .map_or(0, |state| state.flash_remaining);
        if phases.last() != Some(&remaining) {
            phases.push(remaining);
        }
        if matches!(probe.result, PpcRunResult::Halted { .. }) {
            break;
        }
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    }
    assert_eq!(phases, [6, 5, 4, 3, 2, 1, 0]);
    assert_eq!(loaded.cpu.gpr[3], (131u32 << 16) | 2);
    assert_eq!(loaded.cpu.lr, return_address);
    assert!(loaded.toolbox_startup.execution.menu().is_none());
    assert_eq!(
        loaded.toolbox_startup.execution.menu().context().definition,
        None
    );
    assert_eq!(
        loaded.toolbox_startup.execution.menu().context().native_menu(),
        None
    );
    assert_eq!(*loaded.current_gworld, 0x1234_0000);
    assert_eq!(*loaded.current_gdevice, 0x1234_1000);
    let mut restored = Vec::new();
    for y in 0..i32::from(tracking.saved_height) {
        for x in 0..i32::from(tracking.saved_width) {
            restored.push(
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (
                        i32::from(tracking.popup_left) + x,
                        i32::from(tracking.popup_top) + y,
                    ),
                )
                .unwrap(),
            );
        }
    }
    assert_eq!(restored, *tracking.saved_pixels);
}

#[test]
fn native_menu_select_routes_hierarchical_child_through_its_custom_mdef() {
    let pef = synthetic_pef_with_import(b"MenuSelect");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    let root = install_test_menu(&mut loaded, scratch, 140, b"File", b"Custom child");
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x100,
        b"Child",
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x180,
        b"Opaque definition data",
    ));
    let child = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        141,
        scratch + 0x100,
    );
    assert_ne!(child, 0);
    assert_eq!(
        ppc_insert_menu_items(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            child,
            scratch + 0x180,
            i16::MAX,
        ),
        PPC_NO_ERR,
    );
    loaded.cpu.gpr[3] = root;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 0x1B;
    ppc_set_item_cmd(
        &loaded.cpu,
        &mut loaded.memory,
        &test_handle_records!(loaded),
    );
    loaded.cpu.gpr[5] = 141;
    ppc_set_item_mark(
        &loaded.cpu,
        &mut loaded.memory,
        &test_handle_records!(loaded),
    );
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            child,
            -1,
        ),
        PPC_NO_ERR,
    );

    let child_ptr = loaded.memory.read_u32_be(child).unwrap();
    loaded.memory.write_u16_be(child_ptr + 2, 72).unwrap();
    loaded.memory.write_u16_be(child_ptr + 4, 32).unwrap();
    let mdef_handle = PPC_DATA_BASE + 0x7000;
    let descriptor = PPC_DATA_BASE + 0x7100;
    let tvector = PPC_DATA_BASE + 0x7180;
    let callback_entry = PPC_CODE_BASE + 0x4000;
    loaded.memory.add_region(mdef_handle, vec![0; 4]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        PPC_DATA_BASE + 0x7300,
        test_stack_proc_info(
            PPC_PROCINFO_SIZE_NONE,
            &[
                PPC_PROCINFO_SIZE_TWO,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
                PPC_PROCINFO_SIZE_FOUR,
            ],
        ),
        &[d_form_u(14, 8, 0, 2), d_form_u(44, 8, 7, 0), BLR],
    );
    loaded.memory.write_u32_be(mdef_handle, descriptor).unwrap();
    loaded
        .memory
        .write_u32_be(child_ptr + 6, mdef_handle)
        .unwrap();

    let return_address = loaded.cpu.lr;
    loaded
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = 0x1234_0000);
    loaded
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = 0x1234_1000);
    loaded.cpu.gpr[3] = (10u32 << 16) | 12;
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        mouse_v: 10,
        mouse_h: 12,
        ..PpcInputSnapshot::default()
    });
    let probe = loaded.run_with_hle_imports(256);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    let root_rect = loaded
        .toolbox_startup
        .execution.menu()
        .as_ref()
        .unwrap()
        .dropdown_rect();

    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        mouse_v: root_rect.0 + 8,
        mouse_h: root_rect.1 + 16,
        ..PpcInputSnapshot::default()
    });
    let probe = loaded.run_with_hle_imports(512);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
    let child_rect = tracking.submenus[0].dropdown_rect();
    assert_eq!(child_rect.3 - child_rect.1, 72);
    assert_eq!(child_rect.2 - child_rect.0, 32);
    assert_eq!(
        tracking.active_definition_pane(),
        Some(MenuDefinitionPane::Submenu(0))
    );
    assert_eq!(tracking.active_definition().unwrap().menu_handle(), child);

    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        mouse_v: 10,
        mouse_h: 12,
        ..PpcInputSnapshot::default()
    });
    let probe = loaded.run_with_hle_imports(512);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert!(loaded
        .toolbox_startup
        .execution.menu()
        .as_ref()
        .unwrap()
        .submenus
        .is_empty());
    assert_eq!(
        loaded.toolbox_startup.execution.menu().context().caller_isa(),
        Some(GuestIsa::PowerPc)
    );
    assert_eq!(*loaded.current_gworld, 0x1234_0000);
    assert_eq!(*loaded.current_gdevice, 0x1234_1000);

    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        mouse_v: root_rect.0 + 8,
        mouse_h: root_rect.1 + 16,
        ..PpcInputSnapshot::default()
    });
    let probe = loaded.run_with_hle_imports(512);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(
        loaded
            .toolbox_startup
            .execution.menu()
            .as_ref()
            .unwrap()
            .active_definition_pane(),
        Some(MenuDefinitionPane::Submenu(0))
    );

    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        mouse_v: child_rect.0 + 8,
        mouse_h: child_rect.1 + 8,
        ..PpcInputSnapshot::default()
    });
    let probe = loaded.run_with_hle_imports(256);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    assert_eq!(
        loaded
            .toolbox_startup
            .active_menu_definition()
            .unwrap()
            .which_item(),
        2
    );

    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: false,
        mouse_v: child_rect.0 + 8,
        mouse_h: child_rect.1 + 8,
        ..PpcInputSnapshot::default()
    });
    let probe = loaded.run_with_hle_imports(256);
    assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
    loaded
        .toolbox_startup
        .execution
        .with_menu_state_mut(|state| {
            assert_eq!(state.flash_result, (141u32 << 16) | 2);
            state.flash_remaining = 1;
            state.flash_deadline = state.flash_tick.unwrap_or(0);
        })
        .unwrap();
    let probe = loaded.run_with_hle_imports(256);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(loaded.cpu.gpr[3], (141u32 << 16) | 2);
    assert_eq!(loaded.cpu.lr, return_address);
    assert!(loaded.toolbox_startup.execution.menu().is_none());
    assert_eq!(
        loaded.toolbox_startup.execution.menu().context().caller_isa(),
        None
    );
    assert_eq!(*loaded.current_gworld, 0x1234_0000);
    assert_eq!(*loaded.current_gdevice, 0x1234_1000);
}

#[test]
fn native_popup_menu_select_runs_custom_popup_draw_and_choose_sequence() {
    for (top, left, requested_item) in [
        (40i16, 30i16, 4i16),
        (-40, -30, -1),
        (i16::MIN, i16::MAX, 0),
    ] {
        let pef = synthetic_pef_with_import(b"PopUpMenuSelect");
        let mut loaded = load_pef_application(&pef).unwrap();
        let menu = install_test_popup_menu(
            &mut loaded,
            PPC_DATA_BASE + 0x1000,
            132,
            b"Custom",
            b"Opaque definition data",
        );
        let menu_ptr = loaded.memory.read_u32_be(menu).unwrap();
        let mdef_handle = PPC_DATA_BASE + 0x7000;
        let descriptor = PPC_DATA_BASE + 0x7100;
        let tvector = PPC_DATA_BASE + 0x7180;
        loaded.memory.add_region(mdef_handle, vec![0; 4]);
        install_test_powerpc_callback(
            &mut loaded,
            descriptor,
            tvector,
            PPC_CODE_BASE + 0x4000,
            PPC_DATA_BASE + 0x7300,
            test_stack_proc_info(
                PPC_PROCINFO_SIZE_NONE,
                &[
                    PPC_PROCINFO_SIZE_TWO,
                    PPC_PROCINFO_SIZE_FOUR,
                    PPC_PROCINFO_SIZE_FOUR,
                    PPC_PROCINFO_SIZE_FOUR,
                    PPC_PROCINFO_SIZE_FOUR,
                ],
            ),
            &[
                d_form_u(14, 8, 0, 40),
                d_form_u(44, 8, 5, 0),
                d_form_u(14, 8, 0, 30),
                d_form_u(44, 8, 5, 2),
                d_form_u(14, 8, 0, 72),
                d_form_u(44, 8, 5, 4),
                d_form_u(14, 8, 0, 110),
                d_form_u(44, 8, 5, 6),
                d_form_u(14, 8, 0, 2),
                d_form_u(44, 8, 7, 0),
                BLR,
            ],
        );
        loaded.memory.write_u32_be(mdef_handle, descriptor).unwrap();
        loaded
            .memory
            .write_u32_be(menu_ptr + 6, mdef_handle)
            .unwrap();
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let framebuffer_len = front.row_bytes * front.height;
        let before =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len).unwrap();

        loaded.cpu.gpr[3] = menu;
        loaded.cpu.gpr[4] = 0x1234_0000 | u32::from(top as u16);
        loaded.cpu.gpr[5] = 0x5678_0000 | u32::from(left as u16);
        loaded.cpu.gpr[6] = 0x9ABC_0000 | u32::from(requested_item as u16);
        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 52,
            mouse_h: 45,
            ..PpcInputSnapshot::default()
        });
        for _ in 0..32 {
            if loaded.cpu.pc == PPC_CODE_BASE + 0x4000 {
                break;
            }
            let tick = loaded.current_tick().wrapping_add(1);
            loaded.set_tick_count(tick);
            let probe = loaded.run_with_hle_imports(1);
            assert_eq!(probe.unsupported_import_index, None);
        }
        assert_eq!(loaded.cpu.pc, PPC_CODE_BASE + 0x4000);
        assert_eq!(loaded.cpu.gpr[3], 3);
        assert_eq!(loaded.cpu.gpr[4], menu);
        assert_eq!(
            loaded.cpu.gpr[6],
            (u32::from(top as u16) << 16) | u32::from(left as u16)
        );
        assert_eq!(
            loaded.memory.read_u16_be(loaded.cpu.gpr[7]),
            Some(requested_item as u16)
        );
        let tick = loaded.current_tick().wrapping_add(1);
        loaded.set_tick_count(tick);
        let probe = loaded.run_with_hle_imports(512);
        assert_eq!(probe.handled_import_count, 0, "retained custom tracking bypasses public entry");
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(
            loaded
                .toolbox_startup
                .active_menu_definition()
                .unwrap()
                .which_item(),
            2
        );
        assert_eq!(
            loaded
                .toolbox_startup
                .active_menu_definition()
                .unwrap()
                .menu_rect(),
            (40, 30, 72, 110)
        );

        loaded.set_input_snapshot(PpcInputSnapshot {
            mouse_button: false,
            mouse_v: 52,
            mouse_h: 45,
            ..PpcInputSnapshot::default()
        });
        let tick = loaded.current_tick().wrapping_add(1);
        loaded.set_tick_count(tick);
        let probe = loaded.run_with_hle_imports(512);
        assert_eq!(probe.handled_import_count, 0, "retained custom tracking bypasses public entry");
        assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        assert_eq!(
            loaded
                .toolbox_startup
                .execution.menu()
                .as_ref()
                .unwrap()
                .flash_remaining,
            6
        );
        let mut phases = vec![6];
        for _ in 0..64 {
            let tick = loaded.current_tick().wrapping_add(1);
            loaded.set_tick_count(tick);
            let probe = loaded.run_with_hle_imports(512);
            assert_eq!(
                probe.handled_import_count, 0,
                "retained custom tracking bypasses public entry"
            );
            let remaining = loaded
                .toolbox_startup
                .execution.menu()
                .as_ref()
                .map_or(0, |state| state.flash_remaining);
            if phases.last() != Some(&remaining) {
                phases.push(remaining);
            }
            if matches!(probe.result, PpcRunResult::Halted { .. }) {
                break;
            }
            assert!(matches!(probe.result, PpcRunResult::CycleLimit { .. }));
        }
        assert_eq!(phases, [6, 5, 4, 3, 2, 1, 0]);
        assert_eq!(loaded.cpu.gpr[3], (132u32 << 16) | 2);
        assert!(loaded.toolbox_startup.execution.menu().is_none());
        assert_eq!(
            loaded.toolbox_startup.execution.menu().context().definition,
            None
        );
        assert_eq!(
            loaded.toolbox_startup.execution.menu().context().native_popup(),
            None
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(before)
        );
    }
}

#[test]
fn tracked_menu_resolves_icon_precedence_and_variable_rows_at_supported_depths() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let _menu = install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"File",
        b"Normal^1;Reduced^2/\x1d;Small^3/\x1e;Color^4;Script^5/\x1c",
    );
    loaded.set_current_resource_refnum(5);
    let record = |res_type: [u8; 4], res_id: i16, data: Vec<u8>| PpcVfsResourceRecord {
        ref_num: 5,
        path: "Menu Icons".to_owned(),
        res_type: u32::from_be_bytes(res_type),
        res_id,
        name: Vec::new(),
        data,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    };
    let mut normal_icon = vec![0; 128];
    normal_icon[0] = 0x80;
    let mut reduced_icon = vec![0; 128];
    reduced_icon[0] = 0x80;
    let mut small_icon = vec![0; 32];
    small_icon[0] = 0x80;
    loaded.process_file_system.extend_vfs_resources([
        record(*b"ICON", 257, normal_icon),
        record(*b"ICON", 258, reduced_icon),
        record(*b"SICN", 259, small_icon),
        record(*b"ICON", 260, vec![0xff; 128]),
        record(*b"cicn", 260, test_one_bit_cicon()),
        record(*b"cicn", 261, test_one_bit_cicon()),
    ]);

    for depth in [1, 2, 4, 8, 16] {
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
        assert!(draw_current_test_menu_bar(&mut loaded));
        let framebuffer_len = front.row_bytes * front.height;
        let before =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len)
                .unwrap();
        ppc_track_menu_while_held_with_resources(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            MenuColorTable::new(&[]),
            &mut loaded.toolbox_startup,
            12,
            PpcInputSnapshot {
                mouse_button: true,
                mouse_v: 10,
                mouse_h: 12,
                ..PpcInputSnapshot::default()
            },
            &loaded.process_file_system.vfs_resources,
            *loaded.process_file_system.current_resource_file,
        );
        let tracking = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        assert_eq!(
            tracking
                .item_appearances
                .iter()
                .map(|item| item.height)
                .collect::<Vec<_>>(),
            vec![34, 16, 16, 16, 16],
        );
        assert!(matches!(
            tracking.item_appearances[0].icon,
            Some(PpcTrackedMenuIcon::Icon { reduced: false, .. })
        ));
        assert!(matches!(
            tracking.item_appearances[1].icon,
            Some(PpcTrackedMenuIcon::Icon { reduced: true, .. })
        ));
        assert!(matches!(
            tracking.item_appearances[2].icon,
            Some(PpcTrackedMenuIcon::SmallIcon(_))
        ));
        assert!(matches!(
            tracking.item_appearances[3].icon,
            Some(PpcTrackedMenuIcon::CIcon(_))
        ));
        assert_eq!(
            tracking.item_appearances[4].icon_kind,
            StandardMenuIconKind::None,
            "$1C makes the icon byte a script code even when a matching cicn exists",
        );
        assert_eq!(tracking.item_appearances[4].icon, None);
        assert_eq!(tracking.popup_height, 98);
        let black =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        for item in 1..=3 {
            let top = tracking.popup_top + ppc_tracked_menu_item_offset(&tracking, item);
            assert_eq!(
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (i32::from(tracking.popup_left + 2), i32::from(top)),
                ),
                Some(black),
                "{depth}bpp item {item} icon did not draw",
            );
            assert_eq!(
                ppc_menu_tracking_item(
                    &mut loaded.memory,
                    &tracking,
                    PpcInputSnapshot {
                        mouse_button: true,
                        mouse_v: top + ppc_tracked_menu_item_height(&tracking, item) / 2,
                        mouse_h: tracking.popup_left + 20,
                        ..PpcInputSnapshot::default()
                    },
                ),
                item,
            );
        }
        if depth > 1 {
            let red = ppc_physical_screen_color_pixel(
                front,
                PpcRgbColor {
                    red: 0xffff,
                    green: 0,
                    blue: 0,
                },
                &loaded.screen_clut,
            )
            .unwrap();
            let top = tracking.popup_top + ppc_tracked_menu_item_offset(&tracking, 4);
            assert_eq!(
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (i32::from(tracking.popup_left + 2), i32::from(top)),
                ),
                Some(red),
                "{depth}bpp cicn pixel did not map through its embedded ColorTable",
            );
        }
        assert_eq!(
            ppc_finish_menu_bar_tracking(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                PpcInputSnapshot {
                    mouse_v: tracking.popup_top + 8,
                    mouse_h: tracking.popup_left - 1,
                    ..PpcInputSnapshot::default()
                },
            ),
            Some(0),
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, framebuffer_len),
            Some(before),
            "{depth}bpp icon menu did not restore its save-under",
        );
    }
}

#[test]
fn tracked_submenu_aligns_after_a_variable_height_icon_row() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    install_test_menu(
        &mut loaded,
        scratch,
        128,
        b"File",
        b"Icon^1;Parent/\x1b!\xc9",
    );
    loaded.memory.add_region(scratch + 0x200, vec![0; 0x200]);
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x200,
        b"Recent",
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x280,
        b"Child",
    ));
    let submenu = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        201,
        scratch + 0x200,
    );
    assert_eq!(
        ppc_insert_menu_items(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            submenu,
            scratch + 0x280,
            i16::MAX,
        ),
        PPC_NO_ERR,
    );
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            submenu,
            -1,
        ),
        PPC_NO_ERR,
    );
    loaded.set_current_resource_refnum(5);
    let mut icon = vec![0; 128];
    icon[0] = 0x80;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 5,
        path: "Menu Icons".to_owned(),
        res_type: u32::from_be_bytes(*b"ICON"),
        res_id: 257,
        name: Vec::new(),
        data: icon,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    ppc_track_menu_while_held_with_resources(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        MenuColorTable::new(&[]),
        &mut loaded.toolbox_startup,
        12,
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 10,
            mouse_h: 12,
            ..PpcInputSnapshot::default()
        },
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
    );
    let root = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
    assert_eq!(root.item_appearances[0].height, 34);
    let parent_top = root.popup_top + root.item_appearances[0].height;
    ppc_track_menu_while_held_with_resources(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        MenuColorTable::new(&[]),
        &mut loaded.toolbox_startup,
        12,
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: parent_top + 8,
            mouse_h: root.popup_left + 20,
            ..PpcInputSnapshot::default()
        },
        &loaded.process_file_system.vfs_resources,
        *loaded.process_file_system.current_resource_file,
    );
    let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
    let child = tracking.submenus.first().expect("submenu did not open");
    assert_eq!(tracking.highlighted_item, 2);
    assert_eq!(child.popup_top, parent_top);
}

#[test]
fn menu_tracking_round_trips_unaligned_popup_boundaries_at_supported_depths() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_test_menu(
        &mut loaded,
        PPC_DATA_BASE + 0x1000,
        128,
        b"File",
        b"Open/O!\x12;Disabled(;-;Parent/\x1b!\xc9",
    );
    let item_glyph_pixels = {
        let (glyph, data) = get_glyph(PPC_QD_TEXT_FONT_DEFAULT, 12, 'O').unwrap();
        let mut pixels = Vec::new();
        for row in 0..glyph.height as usize {
            for col in 0..glyph.width as usize {
                let index = glyph.data_offset + row * glyph.width as usize + col;
                if index < data.len() && data[index] >= 128 {
                    pixels.push((
                        i32::from(glyph.origin_x) + col as i32,
                        i32::from(glyph.origin_y) + row as i32,
                    ));
                }
            }
        }
        assert!(!pixels.is_empty());
        pixels
    };

    for depth in [1, 2, 4, 8, 16] {
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
        assert_eq!(front.depth, depth);
        let active_len = usize::try_from(front.row_bytes * front.height).unwrap();
        let pattern = (0..active_len)
            .map(|index| {
                (index as u8)
                    .wrapping_mul(37)
                    .wrapping_add((depth as u8).wrapping_mul(11))
                    ^ 0xa5
            })
            .collect::<Vec<_>>();
        loaded
            .memory
            .write_bytes(front.base_addr, &pattern)
            .unwrap();
        assert!(draw_current_test_menu_bar(&mut loaded));
        let before = ppc_memory_read_bytes(
            &mut loaded.memory,
            front.base_addr,
            u32::try_from(active_len).unwrap(),
        )
        .unwrap();

        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            PpcInputSnapshot {
                mouse_button: true,
                mouse_v: 10,
                mouse_h: 12,
                ..PpcInputSnapshot::default()
            },
        );
        let tracking = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        assert_eq!(tracking.front_buffer, Some(front.into()));
        assert_eq!(tracking.popup_left, STANDARD_MENU_BAR_FIRST_TITLE_LEFT);
        assert_eq!(tracking.saved_width, tracking.popup_width + 1);
        assert_eq!(tracking.saved_height, tracking.popup_height + 1);
        assert_eq!(
            tracking.saved_pixels.len(),
            usize::try_from(i32::from(tracking.saved_width) * i32::from(tracking.saved_height))
                .unwrap(),
        );
        if depth < 8 {
            let pixels_per_byte = 8 / depth;
            assert_ne!(
                u32::try_from(tracking.popup_left).unwrap() % pixels_per_byte,
                0,
                "{depth}bpp popup did not exercise an unaligned packed boundary",
            );
        }
        let opened = ppc_memory_read_bytes(
            &mut loaded.memory,
            front.base_addr,
            u32::try_from(active_len).unwrap(),
        )
        .unwrap();
        assert_ne!(opened, before, "{depth}bpp popup did not draw");
        let black =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        let white =
            ppc_physical_screen_color_pixel(front, PPC_RGB_WHITE, &loaded.screen_clut).unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(
                &mut loaded.memory,
                front,
                (
                    i32::from(tracking.popup_left.saturating_add(1)),
                    i32::from(tracking.popup_top),
                ),
            ),
            Some(white),
            "{depth}bpp attached pull-down should not draw a separate top border",
        );
        assert_eq!(
            ppc_quickdraw_read_pixel(
                &mut loaded.memory,
                front,
                (
                    i32::from(tracking.popup_left),
                    i32::from(tracking.popup_top),
                ),
            ),
            Some(black),
            "{depth}bpp attached pull-down should retain its left frame",
        );
        let item_origin = (
            i32::from(tracking.popup_left.saturating_add(15)),
            i32::from(tracking.popup_top.saturating_add(11)),
        );
        let normal_item_ink = item_glyph_pixels
            .iter()
            .filter(|&&(dx, dy)| {
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (item_origin.0 + dx, item_origin.1 + dy),
                ) == Some(black)
            })
            .count();
        assert_eq!(
            normal_item_ink,
            item_glyph_pixels.len(),
            "{depth}bpp popup item glyph did not draw in black",
        );

        let selected = PpcInputSnapshot {
            mouse_button: true,
            mouse_v: tracking.popup_top + 8,
            mouse_h: tracking.popup_left + 20,
            ..PpcInputSnapshot::default()
        };
        assert_eq!(
            ppc_menu_tracking_item(&mut loaded.memory, &tracking, selected),
            1,
        );
        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            selected,
        );
        let highlighted = ppc_memory_read_bytes(
            &mut loaded.memory,
            front.base_addr,
            u32::try_from(active_len).unwrap(),
        )
        .unwrap();
        assert_ne!(
            highlighted, opened,
            "{depth}bpp item highlight did not draw",
        );
        let highlighted_item_ink = item_glyph_pixels
            .iter()
            .filter(|&&(dx, dy)| {
                ppc_quickdraw_read_pixel(
                    &mut loaded.memory,
                    front,
                    (item_origin.0 + dx, item_origin.1 + dy),
                ) == Some(white)
            })
            .count();
        assert_eq!(
            highlighted_item_ink,
            item_glyph_pixels.len(),
            "{depth}bpp highlighted popup item glyph did not reverse to white",
        );

        let cancelled = PpcInputSnapshot {
            mouse_v: tracking.popup_top + 8,
            mouse_h: tracking.popup_left - 1,
            ..PpcInputSnapshot::default()
        };
        assert_eq!(
            ppc_finish_menu_bar_tracking(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                cancelled,
            ),
            Some(0),
        );
        assert!(loaded.toolbox_startup.execution.menu().is_none());
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                front.base_addr,
                u32::try_from(active_len).unwrap(),
            ),
            Some(before),
            "{depth}bpp cancellation did not round-trip the full framebuffer",
        );
    }
}

#[test]
fn menu_tracking_opens_selects_and_restores_hierarchical_submenu() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    install_test_menu(
        &mut loaded,
        scratch,
        128,
        b"File",
        b"Parent/\x1b!\xc9;Cancel",
    );
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x100,
        b"Recent",
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x180,
        b"First/F;Disabled(;Second/S",
    ));
    let submenu = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        201,
        scratch + 0x100,
    );
    assert_ne!(submenu, 0);
    assert_eq!(
        ppc_insert_menu_items(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            submenu,
            scratch + 0x180,
            i16::MAX,
        ),
        PPC_NO_ERR,
    );
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            submenu,
            -1,
        ),
        PPC_NO_ERR,
    );

    for depth in [1, 2, 4, 8, 16] {
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
        let active_len = usize::try_from(front.row_bytes * front.height).unwrap();
        let pattern = (0..active_len)
            .map(|index| (index as u8).wrapping_mul(29).wrapping_add(depth as u8))
            .collect::<Vec<_>>();
        loaded
            .memory
            .write_bytes(front.base_addr, &pattern)
            .unwrap();
        assert!(draw_current_test_menu_bar(&mut loaded));
        let before = ppc_memory_read_bytes(
            &mut loaded.memory,
            front.base_addr,
            u32::try_from(active_len).unwrap(),
        )
        .unwrap();

        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            PpcInputSnapshot {
                mouse_button: true,
                mouse_v: 10,
                mouse_h: 12,
                ..PpcInputSnapshot::default()
            },
        );
        let root = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        let parent_hover = PpcInputSnapshot {
            mouse_button: true,
            mouse_v: root.popup_top + 8,
            mouse_h: root.popup_left + 20,
            ..PpcInputSnapshot::default()
        };
        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            parent_hover,
        );
        let tracking = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        let child = tracking.submenus.first().expect("submenu did not open");
        assert_eq!(tracking.highlighted_item, 1);
        assert_eq!(child.menu_handle, submenu);
        assert_eq!(child.parent_item, 1);
        assert_eq!(
            child.popup_left,
            tracking.popup_left + tracking.popup_width - 4
        );
        assert_eq!(child.popup_top, tracking.popup_top + 7);

        let disabled_hover = PpcInputSnapshot {
            mouse_button: true,
            mouse_v: child.popup_top + 16 + 8,
            mouse_h: child.popup_left + 20,
            ..PpcInputSnapshot::default()
        };
        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            disabled_hover,
        );
        assert_eq!(
            loaded
                .toolbox_startup
                .execution.menu()
                .as_ref()
                .and_then(|state| state.submenus.first())
                .map(|child| child.highlighted_item),
            Some(0),
        );

        let selected = PpcInputSnapshot {
            mouse_v: child.popup_top + 8,
            mouse_h: child.popup_left + 20,
            ..PpcInputSnapshot::default()
        };
        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            PpcInputSnapshot {
                mouse_button: true,
                ..selected
            },
        );
        assert_eq!(
            ppc_finish_menu_bar_tracking(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                selected,
            ),
            Some((201 << 16) | 1),
        );
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(201));
        let highlighted = ppc_memory_read_bytes(
            &mut loaded.memory,
            front.base_addr,
            u32::try_from(active_len).unwrap(),
        )
        .unwrap();
        assert_ne!(
            highlighted, before,
            "{depth}bpp root title was not highlighted"
        );
        assert!(draw_current_test_menu_bar(&mut loaded));
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                front.base_addr,
                u32::try_from(active_len).unwrap(),
            ),
            Some(highlighted),
            "{depth}bpp DrawMenuBar lost the root title for submenu TheMenu",
        );
        let menu_list = ppc_current_menu_list(&mut loaded.memory);
        ppc_set_menu_title_highlight(
            &mut loaded.memory,
            &loaded.gworlds,
            menu_list,
            0,
            &loaded.screen_clut,
            false,
        );
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                front.base_addr,
                u32::try_from(active_len).unwrap(),
            ),
            Some(before.clone()),
            "{depth}bpp submenu selection did not restore the framebuffer",
        );

        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            PpcInputSnapshot {
                mouse_button: true,
                mouse_v: 10,
                mouse_h: 12,
                ..PpcInputSnapshot::default()
            },
        );
        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            parent_hover,
        );
        assert!(loaded
            .toolbox_startup
            .execution.menu()
            .as_ref()
            .is_some_and(|state| !state.submenus.is_empty()));
        assert_eq!(
            ppc_finish_menu_bar_tracking(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                PpcInputSnapshot {
                    mouse_v: 10,
                    mouse_h: i16::MAX,
                    ..PpcInputSnapshot::default()
                },
            ),
            Some(0),
        );
        assert_eq!(
            ppc_memory_read_bytes(
                &mut loaded.memory,
                front.base_addr,
                u32::try_from(active_len).unwrap(),
            ),
            Some(before),
            "{depth}bpp submenu cancellation did not restore the framebuffer",
        );
    }

    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    let menu_list = ppc_current_menu_list(&mut loaded.memory);
    let parent_menu = ppc_menu_list_definition(&mut loaded.memory, menu_list)
        .unwrap()
        .regular_handles()
        .next()
        .unwrap();
    let mut parent =
        ppc_begin_menu_bar_tracking(&mut loaded.memory, front, parent_menu, 400).unwrap();
    parent.popup_left = ppc_u32_to_i16_saturating(front.width).saturating_sub(10);
    let flipped =
        ppc_begin_submenu_tracking(&mut loaded.memory, menu_list, front, &parent, 1).unwrap();
    assert_eq!(
        flipped.popup_left,
        ppc_u32_to_i16_saturating(front.width) - flipped.popup_width - 8,
    );
    ppc_restore_tracked_menu(&mut loaded.memory, Some(front.into()), &flipped);
}

#[test]
fn menu_tracking_selects_nested_submenus_and_restores_every_depth() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    install_test_menu(
        &mut loaded,
        scratch,
        128,
        b"Game",
        b"Options/\x1b!\xc9;Cancel",
    );
    loaded.memory.add_region(scratch + 0x200, vec![0; 0x400]);
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x200,
        b"Options",
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x280,
        b"Speed/\x1b!\xca;Plain",
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x300,
        b"Speed",
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x380,
        b"Cycle/\x1b!\xc9;Fast/F",
    ));
    let options = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        201,
        scratch + 0x200,
    );
    let speed = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        202,
        scratch + 0x300,
    );
    for (menu, items) in [(options, scratch + 0x280), (speed, scratch + 0x380)] {
        assert_ne!(menu, 0);
        assert_eq!(
            ppc_insert_menu_items(
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                test_handles!(loaded),
                menu,
                items,
                i16::MAX,
            ),
            PPC_NO_ERR,
        );
        assert_eq!(
            ppc_insert_menu(
                &mut loaded.memory,
                test_heap_cursor!(loaded),
                test_heap_limit!(loaded),
                test_handles!(loaded),
                &mut free_handle_blocks,
                menu,
                -1,
            ),
            PPC_NO_ERR,
        );
    }
    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    for (selected, expected_root) in [(128, 128), (201, 128), (202, 128), (999, 0)] {
        assert_eq!(
            ppc_root_menu_id_for_selection(&mut loaded.memory, menu_list_handle, selected,),
            expected_root,
        );
    }

    for depth in [1, 2, 4, 8, 16] {
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
        let active_len = front.row_bytes * front.height;
        let pattern = (0..usize::try_from(active_len).unwrap())
            .map(|index| (index as u8).wrapping_mul(43).wrapping_add(depth as u8))
            .collect::<Vec<_>>();
        loaded
            .memory
            .write_bytes(front.base_addr, &pattern)
            .unwrap();
        assert!(draw_current_test_menu_bar(&mut loaded));
        let before =
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, active_len).unwrap();

        let track = |loaded: &mut PpcLoadedApp, point: (i16, i16)| {
            ppc_track_menu_while_held(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                12,
                PpcInputSnapshot {
                    mouse_button: true,
                    mouse_v: point.0,
                    mouse_h: point.1,
                    ..PpcInputSnapshot::default()
                },
            );
        };
        track(&mut loaded, (10, 12));
        let root = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        track(&mut loaded, (root.popup_top + 8, root.popup_left + 20));
        let first = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        assert_eq!(first.submenus.len(), 1);
        assert_eq!(first.submenus[0].menu_handle, options);
        let options_pane = first.submenus[0].clone();
        track(
            &mut loaded,
            (options_pane.popup_top + 8, options_pane.popup_left + 20),
        );
        let nested = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        assert_eq!(nested.submenus.len(), 2);
        assert_eq!(nested.submenus[0].highlighted_item, 1);
        assert_eq!(nested.submenus[1].menu_handle, speed);

        track(
            &mut loaded,
            (
                options_pane.popup_top + 16 + 8,
                options_pane.popup_left + 20,
            ),
        );
        let switched = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        assert_eq!(switched.submenus.len(), 1);
        assert_eq!(switched.submenus[0].highlighted_item, 2);
        track(
            &mut loaded,
            (options_pane.popup_top + 8, options_pane.popup_left + 20),
        );
        let reopened = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        assert_eq!(reopened.submenus.len(), 2);
        let speed_pane = reopened.submenus[1].clone();

        track(
            &mut loaded,
            (speed_pane.popup_top + 8, speed_pane.popup_left + 20),
        );
        let cycle = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
        assert_eq!(cycle.submenus.len(), 2, "circular submenu grew the chain");
        assert_eq!(cycle.submenus[1].highlighted_item, 1);
        assert_eq!(
            ppc_tracked_menu_selection(&mut loaded.memory, &cycle),
            None,
            "hierarchical parent became selectable",
        );

        let leaf = PpcInputSnapshot {
            mouse_v: speed_pane.popup_top + 16 + 8,
            mouse_h: speed_pane.popup_left + 20,
            ..PpcInputSnapshot::default()
        };
        track(&mut loaded, (leaf.mouse_v, leaf.mouse_h));
        assert_eq!(
            ppc_finish_menu_bar_tracking(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                leaf,
            ),
            Some((202 << 16) | 2),
        );
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(202));
        let menu_list = ppc_current_menu_list(&mut loaded.memory);
        ppc_set_menu_title_highlight(
            &mut loaded.memory,
            &loaded.gworlds,
            menu_list,
            0,
            &loaded.screen_clut,
            false,
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, front.base_addr, active_len),
            Some(before),
            "{depth}bpp nested menu did not restore every saved pane",
        );
    }

    loaded.cpu.gpr[3] = u32::from(b'f');
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MenuKey);
    assert_eq!(loaded.cpu.gpr[3], (202 << 16) | 2);
    assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(202));

    loaded
        .toolbox_startup
        .pending_native_menu_selection
        .stage((202, 2));
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MenuSelect);
    assert_eq!(loaded.cpu.gpr[3], (202 << 16) | 2);
    assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(202));
}

#[test]
fn short_menu_bar_system_mark_does_not_bleed_below_the_bar() {
    let pef = synthetic_pef_with_import(b"DrawMenuBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 128, &[0x14], b"");

    for depth in [1, 2, 4, 8, 16] {
        loaded.cpu.gpr[3] = PPC_MAIN_GDEVICE;
        loaded.cpu.gpr[4] = depth;
        loaded.cpu.gpr[5] = 1;
        loaded.cpu.gpr[6] = u32::from(depth != 1);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::SetDepth);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        loaded
            .memory
            .write_u16_be(PPC_MBAR_HEIGHT_ADDR, 12)
            .unwrap();

        let front = ppc_live_front_buffer_for_gworld(
            &mut loaded.memory,
            &loaded.gworlds,
            PPC_MAIN_GWORLD,
        )
        .unwrap();
        let active_len = usize::try_from(front.row_bytes * front.height).unwrap();
        let sentinel = (0..active_len)
            .map(|index| (index as u8).wrapping_mul(13) ^ 0x5a)
            .collect::<Vec<_>>();
        loaded
            .memory
            .write_bytes(front.base_addr, &sentinel)
            .unwrap();
        let below_bar = front.base_addr + front.row_bytes * 12;
        let below_bar_len = front.row_bytes * front.height.saturating_sub(12);
        let below_bar_before =
            ppc_memory_read_bytes(&mut loaded.memory, below_bar, below_bar_len).unwrap();

        assert!(draw_current_test_menu_bar(&mut loaded));

        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, below_bar, below_bar_len),
            Some(below_bar_before),
            "{depth}bpp system mark painted below MBarHeight=12",
        );
        let [red, green, blue] = crate::ui_art::RETRO_COMPUTER_MENU_MARK_PALETTE[0];
        let outline = ppc_physical_screen_color_pixel(
            front,
            PpcRgbColor { red, green, blue },
            &loaded.screen_clut,
        )
        .unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(
                &mut loaded.memory,
                front,
                (
                    i32::from(
                        STANDARD_MENU_BAR_FIRST_TITLE_LEFT
                            + STANDARD_MENU_BAR_TITLE_ORIGIN_INSET
                            + 1,
                    ),
                    0,
                ),
            ),
            Some(outline),
            "{depth}bpp system mark did not draw at its clipped top",
        );
        let black =
            ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, &loaded.screen_clut).unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, (0, 11)),
            Some(black),
            "{depth}bpp short menu bar lost its bottom border",
        );
    }
}

#[test]
fn menu_tracking_uses_the_live_main_set_port_pix_destination() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    install_test_menu(&mut loaded, PPC_DATA_BASE + 0x1000, 128, b"File", b"Open");

    let scratch = PPC_DATA_BASE + 0x20_000;
    let live_pixmap_handle = scratch;
    let live_pixmap = scratch + 0x10;
    let live_pixels = scratch + 0x100;
    let live_width = 128u32;
    let live_height = 96u32;
    let live_row_bytes = live_width;
    loaded.memory.add_region(scratch, vec![0; 0x4000]);
    loaded
        .memory
        .write_u32_be(live_pixmap_handle, live_pixmap)
        .unwrap();
    ppc_write_pixmap(
        &mut loaded.memory,
        live_pixmap,
        live_pixels,
        live_row_bytes,
        0,
        0,
        live_height as i16,
        live_width as i16,
        8,
    )
    .unwrap();
    loaded
        .memory
        .write_u32_be(live_pixmap + 42, PPC_MAIN_CTABLE_HANDLE)
        .unwrap();

    let cached = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
    assert_eq!(cached.base_addr, PPC_MAIN_SCREEN_BASE);
    let cached_len = cached.row_bytes * cached.height;
    loaded
        .memory
        .write_bytes(cached.base_addr, &vec![0x3c; cached_len as usize])
        .unwrap();
    let cached_before =
        ppc_memory_read_bytes(&mut loaded.memory, cached.base_addr, cached_len).unwrap();
    let live_len = live_row_bytes * live_height;
    let live_pattern = (0..live_len as usize)
        .map(|index| (index as u8).wrapping_mul(29) ^ 0x96)
        .collect::<Vec<_>>();
    loaded
        .memory
        .write_bytes(live_pixels, &live_pattern)
        .unwrap();
    ppc_set_port_bits(
        &mut loaded.memory,
        PPC_MAIN_GWORLD,
        live_pixmap_handle,
        true,
    );

    let live =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    assert_eq!(
        live,
        PpcFrontBuffer {
            base_addr: live_pixels,
            row_bytes: live_row_bytes,
            width: live_width,
            height: live_height,
            depth: 8,
        },
    );
    assert!(draw_current_test_menu_bar(&mut loaded));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, cached.base_addr, cached_len),
        Some(cached_before.clone()),
        "DrawMenuBar touched the stale cached main framebuffer",
    );
    let live_before =
        ppc_memory_read_bytes(&mut loaded.memory, live.base_addr, live_len).unwrap();

    ppc_track_menu_while_held(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        &mut loaded.toolbox_startup,
        12,
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 10,
            mouse_h: 12,
            ..PpcInputSnapshot::default()
        },
    );
    let tracking = loaded.toolbox_startup.execution.menu().snapshot().unwrap();
    assert_eq!(tracking.front_buffer, Some(live.into()));
    let selected = PpcInputSnapshot {
        mouse_button: true,
        mouse_v: tracking.popup_top + 8,
        mouse_h: tracking.popup_left + 20,
        ..PpcInputSnapshot::default()
    };
    ppc_track_menu_while_held(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        &mut loaded.toolbox_startup,
        12,
        selected,
    );
    assert_ne!(
        ppc_memory_read_bytes(&mut loaded.memory, live.base_addr, live_len),
        Some(live_before.clone()),
        "tracked popup did not draw into the live main PixMap",
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, cached.base_addr, cached_len),
        Some(cached_before.clone()),
        "tracked popup touched the stale cached main framebuffer",
    );

    assert_eq!(
        ppc_finish_menu_bar_tracking(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            PpcInputSnapshot {
                mouse_v: tracking.popup_top + 8,
                mouse_h: tracking.popup_left - 1,
                ..PpcInputSnapshot::default()
            },
        ),
        Some(0),
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, live.base_addr, live_len),
        Some(live_before),
        "live main PixMap did not round-trip after cancellation",
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, cached.base_addr, cached_len),
        Some(cached_before),
        "cancellation touched the stale cached main framebuffer",
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));
}

#[test]
fn hle_import_runner_get_new_mbar_rejects_a_truncated_declared_menu_sequence() {
    let pef = synthetic_pef_with_import(b"GetNewMBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(*b"MBAR"),
        res_id: 1001,
        name: Vec::new(),
        data: vec![0, 2, 0, 128],
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    loaded.cpu.gpr[3] = 1001;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
}

#[test]
fn hle_import_runner_builds_and_draws_mbar_resources() {
    fn menu_resource(menu_id: i16, title: &[u8]) -> Vec<u8> {
        let mut data = vec![0; 14];
        data[0..2].copy_from_slice(&menu_id.to_be_bytes());
        data[10..14].copy_from_slice(&u32::MAX.to_be_bytes());
        data.push(title.len() as u8);
        data.extend_from_slice(title);
        data.push(0);
        data
    }

    let pef = synthetic_pef_with_import(b"GetNewMBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    let ref_num = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.extend_vfs_resources([
        PpcVfsResourceRecord {
            ref_num,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"MBAR"),
            res_id: 1000,
            name: Vec::new(),
            data: vec![0, 2, 0, 128, 0, 129],
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        },
        PpcVfsResourceRecord {
            ref_num,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"MENU"),
            res_id: 128,
            name: Vec::new(),
            data: menu_resource(128, &[0x14]),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        },
        PpcVfsResourceRecord {
            ref_num,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"MENU"),
            res_id: 129,
            name: Vec::new(),
            data: {
                let mut data = menu_resource(129, b"File");
                data.pop();
                data.extend_from_slice(&[4, b'O', b'p', b'e', b'n', 0, 0, 0, 0, 0]);
                data
            },
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        },
    ]);
    loaded.cpu.gpr[3] = 1000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let menu_list_handle = loaded.cpu.gpr[3];
    ppc_set_current_menu_list(&mut loaded.memory, menu_list_handle);
    let menu_list = loaded.memory.read_u32_be(menu_list_handle).unwrap();
    assert_eq!(loaded.memory.read_u16_be(menu_list), Some(12));
    assert_eq!(loaded.memory.read_u16_be(menu_list + 4), Some(1000));
    let file_menu = ppc_get_menu_handle(&mut loaded.memory, menu_list_handle, 128);
    assert_ne!(file_menu, 0);
    assert_ne!(
        ppc_get_menu_handle(&mut loaded.memory, menu_list_handle, 129),
        0
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetMenu;
    loaded.cpu.gpr[3] = 128;
    let get_menu_probe = loaded.run_with_hle_imports(64);
    assert_eq!(get_menu_probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], file_menu);

    assert!(ppc_draw_menu_bar(
        &mut loaded.memory,
        &loaded.gworlds,
        menu_list_handle,
        &loaded.screen_clut,
    ));
    let white = ppc_rgb_color_to_8bpp_index(PPC_RGB_WHITE);
    let black = ppc_rgb_color_to_8bpp_index(PPC_RGB_BLACK);
    assert_eq!(loaded.memory.read_u8(PPC_MAIN_SCREEN_BASE), Some(black));
    assert_eq!(loaded.memory.read_u8(PPC_MAIN_SCREEN_BASE + 5), Some(white));
    let mark_outline = pict::closest_clut_index(
        crate::ui_art::RETRO_COMPUTER_MENU_MARK_PALETTE[0][0],
        crate::ui_art::RETRO_COMPUTER_MENU_MARK_PALETTE[0][1],
        crate::ui_art::RETRO_COMPUTER_MENU_MARK_PALETTE[0][2],
        &loaded.screen_clut,
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(PPC_MAIN_SCREEN_BASE + 3 * ppc_main_screen_row_bytes() + 19),
        Some(mark_outline)
    );
    assert_eq!(standard_menu_title_advance(&[0x14]), 11);
    let snapshot = loaded.guest_menu_snapshot();
    assert_eq!(snapshot.menus.len(), 2);
    assert_eq!(snapshot.menus[0].title, "Systemless");
    assert_eq!(snapshot.menus[1].title, "File");
    assert_eq!(snapshot.menus[1].items[0].text, "Open");
    assert!(loaded.queue_native_menu_selection(129, 1));
    assert_eq!(
        loaded.toolbox_startup.pending_native_menu_selection,
        Some((129, 1))
    );
    let popup_probe = PPC_MAIN_SCREEN_BASE + 20 * ppc_main_screen_row_bytes() + 35;
    let popup_before = loaded.memory.read_u8(popup_probe).unwrap();
    ppc_track_menu_while_held(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        &mut loaded.toolbox_startup,
        42,
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 10,
            mouse_h: 42,
            ..PpcInputSnapshot::default()
        },
    );
    assert!(loaded.toolbox_startup.execution.menu().is_some());
    assert_ne!(loaded.memory.read_u8(popup_probe), Some(popup_before));
    assert_eq!(
        ppc_finish_menu_bar_tracking(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            PpcInputSnapshot {
                mouse_v: 23,
                mouse_h: 42,
                ..PpcInputSnapshot::default()
            },
        ),
        Some((129 << 16) | 1)
    );
    assert_eq!(loaded.memory.read_u8(popup_probe), Some(popup_before));
    assert_eq!(
        loaded
            .memory
            .read_u8(PPC_MAIN_SCREEN_BASE + 19 * ppc_main_screen_row_bytes()),
        Some(black)
    );
    assert!((3..19).any(|y| {
        (42..100).any(|x| {
            loaded
                .memory
                .read_u8(PPC_MAIN_SCREEN_BASE + y * ppc_main_screen_row_bytes() + x)
                == Some(black)
        })
    }));
    assert_eq!(loaded.test_resource_error(), PPC_NO_ERR);
}

#[test]
fn menu_tracking_rejects_disabled_items_and_dividers_and_clears_cancellation() {
    let pef = synthetic_pef_with_import(b"NewMenu");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut free_handle_blocks = loaded.free_handle_blocks();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch,
        b"File"
    ));
    assert!(ppc_write_pstring_bytes(
        &mut loaded.memory,
        scratch + 0x20,
        b"Open;Disabled(;\x2d"
    ));
    let menu_handle = ppc_new_menu(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        &mut free_handle_blocks,
        128,
        scratch,
    );
    assert_ne!(menu_handle, 0);
    assert_eq!(
        ppc_insert_menu_items(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            menu_handle,
            scratch + 0x20,
            i16::MAX,
        ),
        PPC_NO_ERR
    );
    assert_eq!(
        ppc_insert_menu(
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            test_handles!(loaded),
            &mut free_handle_blocks,
            menu_handle,
            0,
        ),
        PPC_NO_ERR
    );
    let menu_list_handle = ppc_current_menu_list(&mut loaded.memory);
    assert!(ppc_draw_menu_bar(
        &mut loaded.memory,
        &loaded.gworlds,
        menu_list_handle,
        &loaded.screen_clut,
    ));
    let menu_bar_len = u32::from(loaded.memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap())
        * ppc_main_screen_row_bytes();
    let unhighlighted_bar =
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, menu_bar_len).unwrap();

    for (label, item) in [("disabled item", 2i16), ("divider", 3i16)] {
        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            PpcInputSnapshot {
                mouse_button: true,
                mouse_v: 10,
                mouse_h: 12,
                ..PpcInputSnapshot::default()
            },
        );
        let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
        let input = PpcInputSnapshot {
            mouse_button: true,
            mouse_v: tracking.popup_top + (item - 1) * 16 + 4,
            mouse_h: tracking.popup_left + 4,
            ..PpcInputSnapshot::default()
        };
        let background_point = (i32::from(input.mouse_h), i32::from(input.mouse_v));
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        let background = ppc_quickdraw_read_pixel(&mut loaded.memory, front, background_point);
        assert_eq!(
            ppc_menu_tracking_item(&mut loaded.memory, tracking, input),
            0,
            "{label} became the tracked selection"
        );
        ppc_track_menu_while_held(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            12,
            input,
        );
        let front = ppc_front_buffer_for_gworld(&loaded.gworlds, PPC_MAIN_GWORLD).unwrap();
        assert_eq!(
            ppc_quickdraw_read_pixel(&mut loaded.memory, front, background_point),
            background,
            "{label} was drawn highlighted"
        );
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(128));
        assert_eq!(
            ppc_finish_menu_bar_tracking(
                &mut loaded.memory,
                &loaded.gworlds,
                &loaded.screen_clut,
                &mut loaded.toolbox_startup,
                input,
            ),
            Some(0),
            "{label} was returned as a menu choice"
        );
        assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, menu_bar_len),
            Some(unhighlighted_bar.clone()),
            "{label} cancellation left the title highlighted"
        );
    }

    ppc_track_menu_while_held(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        &mut loaded.toolbox_startup,
        12,
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 10,
            mouse_h: 12,
            ..PpcInputSnapshot::default()
        },
    );
    let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
    let outside = PpcInputSnapshot {
        mouse_v: tracking.popup_top + 8,
        mouse_h: tracking.popup_left - 1,
        ..PpcInputSnapshot::default()
    };
    assert_eq!(
        ppc_finish_menu_bar_tracking(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            outside,
        ),
        Some(0)
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(0));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, menu_bar_len),
        Some(unhighlighted_bar.clone())
    );

    ppc_track_menu_while_held(
        &mut loaded.memory,
        &loaded.gworlds,
        &loaded.screen_clut,
        &mut loaded.toolbox_startup,
        12,
        PpcInputSnapshot {
            mouse_button: true,
            mouse_v: 10,
            mouse_h: 12,
            ..PpcInputSnapshot::default()
        },
    );
    let tracking = loaded.toolbox_startup.execution.menu().as_ref().unwrap();
    let enabled = PpcInputSnapshot {
        mouse_v: tracking.popup_top + 6,
        mouse_h: tracking.popup_left + 4,
        ..PpcInputSnapshot::default()
    };
    assert_eq!(
        ppc_finish_menu_bar_tracking(
            &mut loaded.memory,
            &loaded.gworlds,
            &loaded.screen_clut,
            &mut loaded.toolbox_startup,
            enabled,
        ),
        Some((128u32 << 16) | 1)
    );
    assert_eq!(loaded.memory.read_u16_be(PPC_THE_MENU_ADDR), Some(128));
    assert_ne!(
        ppc_memory_read_bytes(&mut loaded.memory, PPC_MAIN_SCREEN_BASE, menu_bar_len),
        Some(unhighlighted_bar),
        "a valid choice did not retain its documented title highlight"
    );
}

#[test]
fn import_bindings_classify_menu_bar_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetMenuBar"),
        PpcImportDispatcherTarget::GetMenuBar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNewMBar"),
        PpcImportDispatcherTarget::GetNewMBar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ClearMenuBar"),
        PpcImportDispatcherTarget::ClearMenuBar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetMenuBar"),
        PpcImportDispatcherTarget::SetMenuBar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetMenuHandle"),
        PpcImportDispatcherTarget::GetMenuHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DrawMenuBar"),
        PpcImportDispatcherTarget::DrawMenuBar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FlashMenuBar"),
        PpcImportDispatcherTarget::FlashMenuBar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetMenuFlash"),
        PpcImportDispatcherTarget::SetMenuFlash
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetMenuFlash"),
        PpcImportDispatcherTarget::LMGetMenuFlash
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetDefltStack"),
        PpcImportDispatcherTarget::LMGetDefltStack
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMGetCurStackBase"),
        PpcImportDispatcherTarget::LMGetCurStackBase
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "LMSetMenuFlash"),
        PpcImportDispatcherTarget::SetMenuFlash
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InvalMenuBar"),
        PpcImportDispatcherTarget::InvalMenuBar
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AppendResMenu"),
        PpcImportDispatcherTarget::AppendResMenu
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AddResMenu"),
        PpcImportDispatcherTarget::AppendResMenu
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InsertResMenu"),
        PpcImportDispatcherTarget::InsertResMenu
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "HMGetHelpMenuHandle"),
        PpcImportDispatcherTarget::HMGetHelpMenuHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MenuChoice"),
        PpcImportDispatcherTarget::MenuChoice
    );
}

#[test]
fn native_inval_menu_bar_defers_and_coalesces_one_draw_until_an_event_scan() {
    let pef = synthetic_pef_with_import(b"InvalMenuBar");
    let mut loaded = load_pef_application(&pef).unwrap();
    let front =
        ppc_live_front_buffer_for_gworld(&mut loaded.memory, &loaded.gworlds, PPC_MAIN_GWORLD)
            .unwrap();
    assert!(ppc_quickdraw_write_pixel(
        &mut loaded.memory,
        front,
        (100, 5),
        PPC_RGB_BLACK,
    ));
    let dirty = ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 5));

    run_test_import(&mut loaded, PpcImportDispatcherTarget::InvalMenuBar);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InvalMenuBar);
    assert!(loaded.event_queue.menu_bar_is_invalid());
    assert_eq!(loaded.toolbox_startup.menu_bar_draw_count, 0);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 5)),
        dirty,
        "InvalMenuBar must not draw synchronously"
    );

    let event = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(event, vec![0xaa; 16]);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = event;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::OSEventAvail);
    assert!(
        loaded.event_queue.menu_bar_is_invalid(),
        "low-level OS event scans must not consume Toolbox redraw work"
    );
    assert_eq!(loaded.toolbox_startup.menu_bar_draw_count, 0);

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = event;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert!(!loaded.event_queue.menu_bar_is_invalid());
    assert_eq!(loaded.toolbox_startup.menu_bar_draw_count, 1);
    assert_ne!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 5)),
        dirty,
        "the next Toolbox event scan must perform DrawMenuBar"
    );

    assert!(ppc_quickdraw_write_pixel(
        &mut loaded.memory,
        front,
        (100, 5),
        PPC_RGB_BLACK,
    ));
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = event;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(loaded.toolbox_startup.menu_bar_draw_count, 1);
    assert_eq!(
        ppc_quickdraw_read_pixel(&mut loaded.memory, front, (100, 5)),
        dirty,
        "the deferred redraw must be consumed exactly once"
    );

    run_test_import(&mut loaded, PpcImportDispatcherTarget::InvalMenuBar);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::DrawMenuBar);
    assert!(!loaded.event_queue.menu_bar_is_invalid());
    assert_eq!(loaded.toolbox_startup.menu_bar_draw_count, 2);
}

#[test]
fn process_owner_shares_one_retained_menu_continuation_between_adapters() {
    let (prepared, mut classic_cpu, mut classic_bus) = setup_with_port();
    let pef = synthetic_pef_with_import(b"MenuSelect");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    let mut classic = TrapDispatcher::new_with_migrated_handles(context.migrated_handles());
    classic
        .scrap
        .set_clipboard_writable(prepared.scrap.clipboard_writable_for_test());
    let (base, row_bytes, width, height, depth) = prepared.screen_mode;
    classic.set_screen_mode_for_test(base, row_bytes, width, height, depth);
    classic.read_tick_count(&classic_bus);
    classic.attach_unconverted_process_services(&mut context);
    let plan = native
        .preflight_migrated_services(&context, classic_bus.read_long(0x016a))
        .unwrap();
    native.commit_migrated_services(&context, plan);
    native.attach_unconverted_process_services(&mut context);

    let mut tracking = crate::menu_manager::test_process_menu_tracking(0x0012_3456);
    tracking.highlighted_item = 2;
    context.set_menu_tracking(Some(tracking));
    classic.menu_tracking.with_context_mut(|menu_context| {
        menu_context.call = Some(MenuTrackingCall {
            request: MenuTrackingRequest::MenuSelect { initial_point: 0 },
            origin: MenuTrackingOrigin::M68k {
                stack_pointer: TEST_SP,
                return_address: 0x1234,
            },
        });
    });
    // A drawing surface cannot turn a classic operation into a native one.
    let native_front = native.current_front_buffer().unwrap();
    context
        .with_menu_tracking_mut(|tracking| tracking.front_buffer = Some(native_front.into()))
        .unwrap();
    assert_eq!(
        classic
            .menu_tracking
            .as_ref()
            .map(|tracking| (tracking.menu_handle, tracking.highlighted_item)),
        Some((0x0012_3456, 2))
    );
    assert_eq!(
        context
            .menu_tracking()
            .map(|tracking| (tracking.menu_handle, tracking.highlighted_item)),
        Some((0x0012_3456, 2))
    );

    native.cpu.gpr[3] = 0;
    run_test_import(&mut native, PpcImportDispatcherTarget::MenuSelect);
    assert!(native.is_constructed_from_migrated_handles(&context.migrated_handles()));
    native
        .toolbox_startup
        .execution
        .with_menu_state_mut(|tracking| tracking.highlighted_item = 4)
        .unwrap();
    assert_eq!(classic.menu_tracking.as_ref().unwrap().highlighted_item, 4);
    assert_eq!(
        context
            .menu_tracking()
            .map(|tracking| tracking.highlighted_item),
        Some(4)
    );

    let call = ppc_menu_select_call(&native.cpu, 0);
    native
        .toolbox_startup
        .execution
        .with_menu_context_mut(|context| context.call = Some(call));
    // Nor can an absent native surface turn its caller into a classic one.
    context
        .with_menu_tracking_mut(|tracking| tracking.front_buffer = None)
        .unwrap();
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_bus.write_long(TEST_SP, 0);
    classic_bus.write_long(TEST_SP + 4, u32::MAX);
    assert!(classic
        .dispatch_menu(true, 0x13D, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_long(TEST_SP + 4), 0);
    assert_eq!(
        context.menu_tracking().map(|tracking| tracking.menu_handle),
        Some(0x0012_3456),
        "the classic slice must retain the process-owned continuation"
    );

    context.take_menu_tracking();
    assert!(classic.menu_tracking.is_none());
    assert!(native.toolbox_startup.execution.menu().is_none());
    assert!(native.is_constructed_from_migrated_handles(&context.migrated_handles()));
}

