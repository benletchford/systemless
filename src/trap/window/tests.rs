use super::super::dispatch::{
    DialogItem, LoadedResources, PersistentDialogSnapshot, QueuedEvent, ResourceFileMap,
};
use super::super::test_helpers::{setup, TEST_SP};
use crate::cpu::{CpuOps, Register};
use crate::memory::MemoryBus;
use std::collections::HashMap;

// Helper: invoke dispatch_window as a toolbox trap.
fn dispatch(
    disp: &mut super::super::TrapDispatcher,
    trap_num: u16,
    cpu: &mut super::super::test_helpers::MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
) -> Option<crate::Result<()>> {
    disp.dispatch_window(true, trap_num, cpu, bus)
}

#[test]
fn attached_window_list_preserves_activation_across_traps() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.process_window_list_attached = true;
    let active = bus.alloc(256);
    let raised = bus.alloc(256);
    disp.window_list.replace(vec![active, raised]);
    disp.front_window = active;
    bus.write_byte(active + 110, 255);
    bus.write_byte(active + 111, 255);
    bus.write_byte(raised + 110, 255);
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, raised);
    disp.dispatch(0xA920, &mut cpu, &mut bus).unwrap();
    assert_eq!(disp.window_list.first(), Some(raised));

    // Even an unrelated trap must not silently activate the raised window.
    cpu.write_reg(Register::A7, sp);
    disp.dispatch(0xA975, &mut cpu, &mut bus).unwrap();
    assert_eq!(disp.front_window, active);
    assert_eq!(bus.read_byte(raised + 111), 0);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, raised);
    disp.dispatch(0xA91F, &mut cpu, &mut bus).unwrap();
    assert_eq!(disp.front_window, raised);
    assert_eq!(bus.read_byte(active + 111), 0);
    assert_eq!(bus.read_byte(raised + 111), 255);
    assert!(disp
        .event_queue
        .iter()
        .any(|event| event.what == 8 && event.message == active && event.modifiers & 1 == 0));

    // Invisible frontmost windows can be active too (NewWindow's ABI).
    bus.write_byte(raised + 110, 0);
    cpu.write_reg(Register::A7, sp);
    disp.dispatch(0xA975, &mut cpu, &mut bus).unwrap();
    assert_eq!(disp.front_window, raised);
}

#[test]
fn layer_dispatch_is_layer_returns_false_and_consumes_its_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::D0, 2);
    bus.write_long(sp, 0x003F_119E);
    bus.write_word(sp + 4, 0xFFFF);

    let result = dispatch(&mut disp, 0x029, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp + 4);
    assert_eq!(bus.read_word(sp + 4), 0);
}

fn make_window(bus: &mut crate::memory::MacMemoryBus, visible: bool, dirty: bool) -> u32 {
    let window_ptr = bus.alloc(160);
    bus.write_byte(
        window_ptr + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
        if visible { 0xFF } else { 0 },
    );
    let region_ptr = bus.alloc(10);
    bus.write_word(region_ptr, 10);
    let rect = if dirty {
        (0i16, 0i16, 10i16, 10i16)
    } else {
        (0, 0, 0, 0)
    };
    bus.write_word(region_ptr + 2, rect.0 as u16);
    bus.write_word(region_ptr + 4, rect.1 as u16);
    bus.write_word(region_ptr + 6, rect.2 as u16);
    bus.write_word(region_ptr + 8, rect.3 as u16);
    let handle = bus.alloc(4);
    bus.write_long(handle, region_ptr);
    bus.write_long(
        window_ptr + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET,
        handle,
    );
    window_ptr
}

#[test]
fn pending_update_event_selects_front_to_back_and_falls_back_when_list_is_empty() {
    let (mut disp, _cpu, mut bus) = setup();
    // Front-to-back selection: the front window is clean, the middle is
    // the first dirty one, the back is also dirty; the middle must win.
    let front = make_window(&mut bus, true, false);
    let middle = make_window(&mut bus, true, true);
    let back = make_window(&mut bus, true, true);
    disp.window_list.replace(vec![front, middle, back]);
    disp.front_window = front;
    let event = disp
        .pending_update_event(&bus, 0xFFFF)
        .expect("update event");
    assert_eq!(event.what, 6, "updateEvt");
    assert_eq!(event.message, middle, "first dirty window front-to-back");

    // An invisible dirty window ahead of a visible dirty one is skipped.
    let hidden = make_window(&mut bus, false, true);
    disp.window_list.replace(vec![hidden, back]);
    let event = disp
        .pending_update_event(&bus, 0xFFFF)
        .expect("update event");
    assert_eq!(event.message, back, "visibility still gates selection");

    // Empty list: the front window serves as the fallback...
    let lone = make_window(&mut bus, true, true);
    disp.window_list.replace(Vec::new());
    disp.front_window = lone;
    let event = disp
        .pending_update_event(&bus, 0xFFFF)
        .expect("fallback event");
    assert_eq!(event.message, lone, "front-window fallback on empty list");

    // ...and no fallback exists when it is unset, or when the update
    // bit is masked out.
    disp.front_window = 0;
    assert!(disp.pending_update_event(&bus, 0xFFFF).is_none());
    disp.front_window = lone;
    assert!(
        disp.pending_update_event(&bus, 0xFFBF).is_none(),
        "mask gates"
    );
}

fn install_wind_resource(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut crate::memory::MacMemoryBus,
    window_id: i16,
    bounds: (i16, i16, i16, i16),
    proc_id: i16,
    visible: bool,
    go_away: bool,
    ref_con: u32,
    title: &[u8],
) {
    install_positioned_wind_resource(
        disp, bus, window_id, bounds, proc_id, visible, go_away, ref_con, title, None,
    );
}

fn install_positioned_wind_resource(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut crate::memory::MacMemoryBus,
    window_id: i16,
    bounds: (i16, i16, i16, i16),
    proc_id: i16,
    visible: bool,
    go_away: bool,
    ref_con: u32,
    title: &[u8],
    position: Option<u16>,
) {
    let title_len = title.len().min(255) as u8;
    let mut wind_data = vec![0; 18 + 1 + title_len as usize];
    wind_data[0..2].copy_from_slice(&bounds.0.to_be_bytes());
    wind_data[2..4].copy_from_slice(&bounds.1.to_be_bytes());
    wind_data[4..6].copy_from_slice(&bounds.2.to_be_bytes());
    wind_data[6..8].copy_from_slice(&bounds.3.to_be_bytes());
    wind_data[8..10].copy_from_slice(&proc_id.to_be_bytes());
    wind_data[10] = if visible { 0xFF } else { 0x00 };
    wind_data[12] = if go_away { 0xFF } else { 0x00 };
    wind_data[14..18].copy_from_slice(&ref_con.to_be_bytes());
    wind_data[18] = title_len;
    wind_data[19..].copy_from_slice(&title[..title_len as usize]);
    if let Some(position) = position {
        if wind_data.len() % 2 != 0 {
            wind_data.push(0);
        }
        wind_data.extend_from_slice(&position.to_be_bytes());
    }

    let wind_ptr = bus.alloc(wind_data.len() as u32);
    bus.write_bytes(wind_ptr, &wind_data);

    let mut loaded = HashMap::new();
    loaded.insert((*b"WIND", window_id), wind_ptr);
    let file = ResourceFileMap {
        loaded,
        named: HashMap::new(),
        names_by_id: HashMap::new(),
        attrs: HashMap::new(),
        map_attrs: 0,
    };
    disp.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([(0, file)]),
        names: HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });
    disp.remember_resource_backing_data(0, *b"WIND", window_id, wind_data);
}

fn install_wdef_resource(
    disp: &mut super::super::TrapDispatcher,
    bus: &mut crate::memory::MacMemoryBus,
    wdef_id: i16,
) -> u32 {
    let proc_addr = bus.alloc(2);
    bus.write_word(proc_addr, 0x4E56); // plausible 68K LINK.W proc entry
    disp.with_resource_manager_mut(|resource_manager| {
        let resources = resource_manager
            .resources
            .get_or_insert_with(|| LoadedResources {
                files: HashMap::new(),
                names: HashMap::new(),
                search_order: vec![0],
                current_file: 0,
            });
        let file = resources.files.entry(0).or_default();
        file.loaded.insert((*b"WDEF", wdef_id), proc_addr);
        if !resources.search_order.contains(&0) {
            resources.search_order.push(0);
        }
    });
    proc_addr
}

fn setup_region_window() -> (
    super::super::TrapDispatcher,
    super::super::test_helpers::MockCpu,
    crate::memory::MacMemoryBus,
    u32,
) {
    let (mut disp, mut cpu, mut bus) = setup();
    let bounds_rect_ptr = 0x300000u32;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 200);
    bus.write_word(bounds_rect_ptr + 6, 300);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds_rect_ptr);

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewWindow should be handled");
    assert!(result.unwrap().is_ok(), "NewWindow should return");

    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
    disp.window_list.replace(vec![window_ptr]);
    disp.front_window = window_ptr;
    disp.current_port
        .with_mut(|current_port| *current_port = window_ptr);
    disp.validate_window_rect(&mut bus, window_ptr, (0, 0, 160, 260));

    (disp, cpu, bus, window_ptr)
}

fn make_region_handle(
    bus: &mut crate::memory::MacMemoryBus,
    handle_addr: u32,
    data_addr: u32,
    size: u16,
    bbox: (i16, i16, i16, i16),
) -> u32 {
    bus.write_long(handle_addr, data_addr);
    bus.write_word(data_addr, size);
    bus.write_word(data_addr + 2, bbox.0 as u16);
    bus.write_word(data_addr + 4, bbox.1 as u16);
    bus.write_word(data_addr + 6, bbox.2 as u16);
    bus.write_word(data_addr + 8, bbox.3 as u16);
    handle_addr
}

fn read_window_region_rect(
    bus: &crate::memory::MacMemoryBus,
    window_ptr: u32,
    offset: u32,
) -> (i16, i16, i16, i16) {
    let handle = bus.read_long(window_ptr + offset);
    let ptr = bus.read_long(handle);
    (
        bus.read_word(ptr + 2) as i16,
        bus.read_word(ptr + 4) as i16,
        bus.read_word(ptr + 6) as i16,
        bus.read_word(ptr + 8) as i16,
    )
}

fn make_v1_paintrect_picture(
    bus: &mut crate::memory::MacMemoryBus,
    frame: (i16, i16, i16, i16),
) -> u32 {
    let picture_data = bus.alloc(32);
    let mut cursor = picture_data + 10;
    bus.write_byte(cursor, 0x11); // versionOp
    cursor += 1;
    bus.write_byte(cursor, 0x01); // version 1
    cursor += 1;
    bus.write_byte(cursor, 0x31); // paintRect
    cursor += 1;
    for coordinate in [frame.0, frame.1, frame.2, frame.3] {
        bus.write_word(cursor, coordinate as u16);
        cursor += 2;
    }
    bus.write_byte(cursor, 0xFF); // EndOfPicture
    cursor += 1;
    bus.write_word(picture_data, (cursor - picture_data) as u16);
    bus.write_word(picture_data + 2, frame.0 as u16);
    bus.write_word(picture_data + 4, frame.1 as u16);
    bus.write_word(picture_data + 6, frame.2 as u16);
    bus.write_word(picture_data + 8, frame.3 as u16);

    let picture = bus.alloc(4);
    bus.write_long(picture, picture_data);
    picture
}

#[test]
fn fullscreen_window_erase_uses_resolved_screen_address_and_preserves_low_memory() {
    for hidden_menu in [false, true] {
        for missing_address in [false, true] {
            let (mut disp, mut cpu, mut bus) = setup();
            let screen = 0x0030_0000;
            disp.set_screen_mode_for_test(screen, 816, 800, 600, 8);
            disp.menu_bar_hidden = hidden_menu;
            bus.fill_bytes(screen, 816 * 600, 0x55);
            let probes = [
                (0x28, 0x1234_5678),
                (0x400, 0x2345_6789),
                (0xC00, 0x3456_789A),
            ];
            for (address, value) in probes {
                bus.write_long(address, value);
            }
            let window = bus.alloc(256);
            disp.init_cgraf_window(
                &mut bus,
                &mut cpu,
                window,
                if missing_address { 0 } else { screen },
                0,
                0,
                600,
                800,
                "",
                2,
                true,
                false,
                false,
                0,
            );
            for (address, value) in probes {
                assert_eq!(bus.read_long(address), value, "system cell ${address:04X}");
            }
            let pixmap = bus.read_long(bus.read_long(window + 2));
            assert_eq!(bus.read_long(pixmap), screen);
            let background = if hidden_menu { 0xFF } else { 0 };
            for offset in [0, 300 * 816 + 400, 599 * 816 + 799] {
                assert_eq!(bus.read_byte(screen + offset), background);
            }
            assert_eq!(
                bus.read_byte(screen + 800),
                0x55,
                "row padding is outside the window"
            );
        }
    }
}

#[test]
fn init_cgraf_window_starts_with_large_clip_region() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = false;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        21,
        1,
        599,
        799,
        "",
        2,
        true,
        true,
        false,
        0,
    );

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (0, 0, 578, 798),
        "visible window region should cover the content rect"
    );
    assert_eq!(
        read_window_region_rect(&bus, window_addr, 28),
        (-32767, -32767, 32767, 32767),
        "new windows should inherit QuickDraw's arbitrarily large default clipRgn"
    );
}

#[test]
fn init_cgraf_window_expands_near_fullscreen_plain_window_when_host_hides_menu_bar() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = true;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        21,
        1,
        599,
        799,
        "",
        2,
        true,
        true,
        false,
        0,
    );

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (-21, -1, 579, 799),
        "host-hidden menu bar should expose the full screen-backed PixMap"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
        ),
        (0, 0, 600, 800),
        "initial update region should be the expanded visible region in global coordinates"
    );
}

#[test]
fn init_cgraf_window_prepares_exact_fullscreen_plain_window_before_guest_hides_menu_bar() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        0,
        0,
        600,
        800,
        "",
        2,
        true,
        true,
        false,
        0,
    );

    assert_eq!(
            read_window_region_rect(&bus, window_addr, 24),
            (0, 0, 600, 800),
            "an exact full-screen plain window must accept drawing behind the menu before MBarHeight reaches zero"
        );
}

#[test]
fn oversized_dialog_keeps_its_content_region_when_moved() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        0,
        -1000,
        1024,
        1000,
        "",
        2,
        false,
        false,
        false,
        0,
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
        ),
        (0, -1000, 1024, 1000),
    );

    disp.move_window_to_global(&mut bus, window_addr, -600, -212, false);
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
        ),
        (-212, -600, 812, 1400),
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_STRUC_RGN_OFFSET
        ),
        (-213, -601, 813, 1401),
    );
}

#[test]
fn init_cgraf_window_stores_windowrecord_manager_regions_in_global_coordinates() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        100,
        200,
        300,
        500,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (0, 0, 200, 300),
        "visRgn remains in window-local coordinates"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
        ),
        (100, 200, 300, 500),
        "contRgn is stored in global coordinates"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_STRUC_RGN_OFFSET
        ),
        (99, 199, 301, 501),
        "plainDBox strucRgn uses its one-pixel WDEF frame in global coordinates"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
        ),
        (100, 200, 300, 500),
        "updateRgn is stored in global coordinates"
    );
}

#[test]
fn init_cgraf_window_seeds_dbox_structure_region_with_drawn_frame_margin() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        93,
        236,
        225,
        564,
        "",
        1,
        true,
        false,
        false,
        0,
    );

    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_STRUC_RGN_OFFSET
        ),
        (85, 228, 233, 572),
        "dBoxProc strucRgn must match the eight-pixel frame drawn by DrawDialog"
    );
}

#[test]
fn init_cgraf_window_sets_user_window_kind_independent_of_wdef_proc_id() {
    // Application WindowRecords use userKind=8 in windowKind; the WDEF
    // procID that drives chrome/variation behavior is stored separately.
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        100,
        200,
        300,
        500,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    assert_eq!(
        bus.read_word(window_addr + super::super::TrapDispatcher::WINDOW_KIND_OFFSET),
        super::super::TrapDispatcher::USER_WINDOW_KIND
    );
    assert_eq!(disp.window_proc_ids.get(&window_addr), Some(&2));
}

#[test]
fn movewindow_offsets_windowrecord_manager_regions_globally() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        100,
        200,
        300,
        500,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    disp.move_window_to_global(&mut bus, window_addr, 1800, 1600, false);

    assert_eq!(
        disp.window_global_port_rect(&bus, window_addr),
        (1600, 1800, 1800, 2100),
        "window-local origin should map to the new global MoveWindow point"
    );
    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (0, 0, 200, 300),
        "MoveWindow should not rewrite visRgn out of local coordinates"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
        ),
        (1600, 1800, 1800, 2100),
        "contRgn should move with the window in global coordinates"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_STRUC_RGN_OFFSET
        ),
        (1599, 1799, 1801, 2101),
        "plainDBox strucRgn should move with the window in global coordinates"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
        ),
        (1600, 1800, 1800, 2100),
        "pending updateRgn should move with the window in global coordinates"
    );
}

#[test]
fn showwindow_setorigin_preserves_expanded_near_fullscreen_clip_region() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = true;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        21,
        1,
        599,
        799,
        "",
        2,
        true,
        true,
        false,
        0,
    );

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);
    let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, (-86i16) as u16);
    bus.write_word(sp + 2, (-143i16) as u16);
    let result = disp.dispatch_quickdraw(true, 0x078, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 28),
        (-32767, -32767, 32767, 32767),
        "SetOrigin must not collapse a window's clipRgn"
    );
}

#[test]
fn hidden_window_setorigin_preserves_global_regions_before_showwindow() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        46,
        42,
        388,
        554,
        "",
        3,
        false,
        false,
        false,
        0,
    );

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (0, 0, 0, 0),
        "a newly hidden window must start with an empty visRgn"
    );

    disp.move_window_to_global(&mut bus, window_addr, 100, 100, false);
    let current_gdevice = *disp.current_gdevice;
    disp.set_current_port_state(&mut bus, &mut cpu, window_addr, Some(current_gdevice));

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, (-139i16) as u16);
    bus.write_word(sp + 2, (-143i16) as u16);
    let result = disp.dispatch_quickdraw(true, 0x078, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
        ),
        (100, 100, 442, 612),
        "SetOrigin must preserve the global content region"
    );

    let show_sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, show_sp);
    bus.write_long(show_sp, window_addr);
    let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (-139, -143, 203, 369),
        "ShowWindow must express the fixed content region in the current local coordinates"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET
        ),
        (100, 100, 442, 612),
        "ShowWindow must queue the unchanged global content region"
    );

    let begin_sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, begin_sp);
    bus.write_long(begin_sp, window_addr);
    let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (-139, -143, 203, 369),
        "BeginUpdate must convert the fixed global update region to local coordinates"
    );
}

#[test]
fn init_cgraf_window_custom_wdef_installs_window_def_proc_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let wdef_proc = install_wdef_resource(&mut disp, &mut bus, 200);
    let window_addr = bus.alloc(256);
    let proc_id = (200i16 << 4) | 3;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        34,
        2,
        114,
        473,
        "",
        proc_id,
        false,
        true,
        false,
        0,
    );

    let def_handle =
        bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_DEF_PROC_OFFSET);
    assert_ne!(def_handle, 0, "custom WDEF should be loaded into a handle");
    assert_eq!(
        bus.read_long(def_handle),
        wdef_proc,
        "windowDefProc should point at the loaded WDEF resource"
    );
    assert!(
        disp.window_uses_custom_def_proc(&bus, window_addr),
        "custom WDEF window should be classified from procID + windowDefProc"
    );
}

#[test]
fn init_cgraf_window_standard_wdef_installs_window_def_proc_handle() {
    // The Window Manager stores the resolved WDEF handle in the public
    // WindowRecord even for standard procIDs. Macintosh Toolbox
    // Essentials (1992), pp. 4-66 and 4-145.
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        34,
        2,
        114,
        473,
        "",
        12,
        false,
        true,
        false,
        0,
    );

    let def_handle =
        bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_DEF_PROC_OFFSET);
    assert_ne!(
        def_handle, 0,
        "standard WDEF should remain guest-visible through windowDefProc"
    );
    assert_ne!(
        bus.read_long(def_handle),
        0,
        "standard WDEF handle should be loaded"
    );
    assert_eq!(
        disp.resource_handle_files.get(&def_handle).copied(),
        Some(0),
        "standard WDEF should belong to the system resource file"
    );
}

#[test]
fn getnewcwindow_visible_custom_wdef_arms_wnew_wcalcrgns_then_wdraw_trampoline() {
    let (mut disp, mut cpu, mut bus) = setup();
    let proc_id = (200i16 << 4) | 3;
    install_wind_resource(
        &mut disp,
        &mut bus,
        600,
        (34, 2, 114, 473),
        proc_id,
        true,
        false,
        0,
        b"",
    );
    let wdef_proc = install_wdef_resource(&mut disp, &mut bus, 200);

    let sp = TEST_SP - 10;
    let return_pc = 0x1234_5678;
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::PC, return_pc);
    bus.write_long(sp, 0); // behind
    bus.write_long(sp + 4, 0); // wStorage
    bus.write_word(sp + 8, 600); // windowID
    bus.write_long(sp + 10, 0);

    let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    let window_ptr = bus.read_long(sp + 10);
    let tramp = disp.window_def_trampoline;
    assert_ne!(window_ptr, 0);
    assert_ne!(tramp, 0, "custom WDEF should allocate a trampoline");
    assert_eq!(cpu.read_reg(Register::PC), tramp);
    assert_eq!(cpu.read_reg(Register::A7), sp + 6);
    assert_eq!(
        bus.read_long(sp + 6),
        return_pc,
        "callback RTS should resume at the original trap return PC"
    );
    let def_handle =
        bus.read_long(window_ptr + super::super::TrapDispatcher::WINDOW_DEF_PROC_OFFSET);
    assert_ne!(def_handle, 0);
    assert_eq!(bus.read_long(def_handle), wdef_proc);
    let pixmap_handle = bus.read_long(window_ptr + 2);
    let pixmap = bus.read_long(pixmap_handle);
    assert_eq!(
        bus.read_long(window_ptr + 8),
        bus.read_long(pixmap + 6),
        "legacy WDEF callback should temporarily see portPixMap bounds"
    );
    assert_eq!(bus.read_long(window_ptr + 12), bus.read_long(pixmap + 10));
    assert_eq!(
        bus.read_word(tramp + 12),
        3,
        "variant must be low procID bits"
    );
    assert_eq!(bus.read_long(tramp + 16), window_ptr);
    assert_eq!(
        bus.read_word(tramp + 22),
        super::super::TrapDispatcher::WDEF_WNEW_MSG as u16
    );
    assert_eq!(bus.read_long(tramp + 26), 0);
    assert_eq!(bus.read_long(tramp + 32), wdef_proc);
    assert_eq!(bus.read_long(tramp + 38), (sp + 6).wrapping_sub(32));
    assert_eq!(
        bus.read_word(tramp + 46),
        0x4EF9,
        "first WDEF call should chain"
    );

    let calc_tramp = bus.read_long(tramp + 48);
    assert_ne!(calc_tramp, 0);
    assert_eq!(bus.read_word(calc_tramp + 12), 3);
    assert_eq!(bus.read_long(calc_tramp + 16), window_ptr);
    assert_eq!(
        bus.read_word(calc_tramp + 22),
        super::super::TrapDispatcher::WDEF_WCALC_RGNS_MSG as u16
    );
    assert_eq!(bus.read_long(calc_tramp + 26), 0);
    assert_eq!(bus.read_long(calc_tramp + 32), wdef_proc);
    assert_eq!(bus.read_long(calc_tramp + 38), (sp + 6).wrapping_sub(32));
    assert_eq!(
        bus.read_word(calc_tramp + 46),
        0x4EF9,
        "wCalcRgns should chain to wDraw"
    );

    let draw_tramp = bus.read_long(calc_tramp + 48);
    assert_ne!(draw_tramp, 0);
    assert_eq!(bus.read_word(draw_tramp + 12), 3);
    assert_eq!(bus.read_long(draw_tramp + 16), window_ptr);
    assert_eq!(
        bus.read_word(draw_tramp + 22),
        super::super::TrapDispatcher::WDEF_WDRAW_MSG as u16
    );
    assert_eq!(bus.read_long(draw_tramp + 26), 0);
    assert_eq!(bus.read_long(draw_tramp + 32), wdef_proc);
    assert_eq!(bus.read_long(draw_tramp + 38), (sp + 6).wrapping_sub(32));
    assert_eq!(bus.read_word(draw_tramp + 46), 0x23FC);
    assert_eq!(
        bus.read_long(draw_tramp + 48),
        0,
        "final callback should restore grafVars"
    );
    assert_eq!(bus.read_long(draw_tramp + 52), window_ptr + 8);
    assert_eq!(bus.read_word(draw_tramp + 56), 0x23FC);
    assert_eq!(
        bus.read_long(draw_tramp + 58),
        0,
        "final callback should restore chExtra and pnLocHFrac"
    );
    assert_eq!(bus.read_long(draw_tramp + 62), window_ptr + 12);
    assert_eq!(bus.read_word(draw_tramp + 66), 0x4E75);
    assert_eq!(
        *disp.current_port, disp.window_manager_cport,
        "wDraw should run in the color Window Manager port"
    );
}

// ---------------------------------------------------------------
// 1. InitWindows (0x112) -- no-op, returns Ok
// ---------------------------------------------------------------
#[test]
fn initwindows_procedure_call_preserves_stack_pointer() {
    let (mut disp, mut cpu, mut bus) = setup();
    let result = dispatch(&mut disp, 0x112, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    // SP unchanged (no stack parameters)
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn initwindows_initializes_lowmem_grayrgn_to_desktop_region() {
    // Macintosh Toolbox Essentials 1992, pp. 4-113..4-114:
    // GetGrayRgn returns the current desktop region from low-memory
    // GrayRgn, and Universal Headers define it at $09EE.
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let result = dispatch(&mut disp, 0x112, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    let gray_rgn = bus.read_long(0x09EE);
    assert_ne!(gray_rgn, 0, "InitWindows should publish GrayRgn");
    let gray_ptr = bus.read_long(gray_rgn);
    assert_ne!(gray_ptr, 0, "GrayRgn should be a valid region handle");
    assert_eq!(bus.read_word(gray_ptr), 10);

    let (_, _, width, height, _) = disp.screen_mode;
    assert_eq!(
        super::super::TrapDispatcher::region_handle_rect(&bus, gray_rgn),
        Some((20, 0, height as i16, width as i16))
    );
}

#[test]
fn getwmgrport_writes_window_manager_port_pointer_to_output_argument() {
    // Inside Macintosh Volume I (1985), p. I-282:
    // GetWMgrPort returns a pointer to the Window Manager port in wPort.
    let (mut disp, mut cpu, mut bus) = setup();

    let init = dispatch(&mut disp, 0x112, &mut cpu, &mut bus);
    assert!(init.is_some());
    assert!(init.unwrap().is_ok());

    disp.front_window = 0x00DE_AD00;
    let out_ptr = 0x300000u32;
    bus.write_long(out_ptr, 0xFFFF_FFFF);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, out_ptr);

    let result = dispatch(&mut disp, 0x110, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());

    let wmgr_port = bus.read_long(out_ptr);
    assert_ne!(wmgr_port, 0, "GetWMgrPort should write a non-NIL GrafPtr");
    assert_eq!(
        wmgr_port,
        bus.read_long(0x09DE),
        "GetWMgrPort should agree with low-memory WMgrPort"
    );
    assert_eq!(
        bus.read_word(wmgr_port + 6) & 0xC000,
        0,
        "GetWMgrPort must expose a basic GrafPort rowBytes field"
    );
    assert_ne!(
        wmgr_port, disp.front_window,
        "Window Manager port pointer should not alias the front window pointer"
    );

    let (screen_base, row_bytes, width, height, _) = disp.screen_mode;
    assert_eq!(
        bus.read_long(wmgr_port + 2),
        screen_base,
        "GrafPort.portBits.baseAddr should address the main screen"
    );
    assert_eq!(
        u32::from(bus.read_word(wmgr_port + 6)),
        row_bytes,
        "GrafPort.portBits.rowBytes should describe the main screen"
    );
    assert_eq!(bus.read_word(wmgr_port + 8) as i16, 0);
    assert_eq!(bus.read_word(wmgr_port + 10) as i16, 0);
    assert_eq!(
        bus.read_word(wmgr_port + 12) as i16,
        height as i16,
        "GrafPort.portBits.bounds.bottom should match screen height"
    );
    assert_eq!(
        bus.read_word(wmgr_port + 14) as i16,
        width as i16,
        "GrafPort.portBits.bounds.right should match screen width"
    );
    assert_eq!(bus.read_word(wmgr_port + 16) as i16, 0);
    assert_eq!(bus.read_word(wmgr_port + 18) as i16, 0);
    assert_eq!(
        bus.read_word(wmgr_port + 20) as i16,
        height as i16,
        "Window Manager portRect.bottom should match screen height"
    );
    assert_eq!(
        bus.read_word(wmgr_port + 22) as i16,
        width as i16,
        "Window Manager portRect.right should match screen width"
    );
}

#[test]
fn getwmgrport_consumes_output_pointer_argument() {
    // Inside Macintosh Volume I (1985), p. I-282:
    // PROCEDURE GetWMgrPort(VAR wPort: GrafPtr) consumes one pointer arg.
    let (mut disp, mut cpu, mut bus) = setup();
    let out_ptr = 0x300100u32;
    bus.write_long(out_ptr, 0xA5A5_A5A5);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, out_ptr);

    let result = dispatch(&mut disp, 0x110, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// ---------------------------------------------------------------
// 2. NewWindow (0x113) -- 26 bytes params, result at SP+26
// ---------------------------------------------------------------
#[test]
fn test_new_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Set up a bounds rect at 0x300000: top=40, left=0, bottom=342, right=512
    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40); // top
    bus.write_word(bounds_rect_ptr + 2, 0); // left
    bus.write_word(bounds_rect_ptr + 4, 342); // bottom
    bus.write_word(bounds_rect_ptr + 6, 512); // right

    // Push 26 bytes of params + 4 result onto stack. SP+18 = bounds_rect_ptr.
    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    // Zero-fill
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    // SP+18 = bounds_rect_ptr (4 bytes, big-endian)
    bus.write_long(sp + 18, bounds_rect_ptr);

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    // SP should be TEST_SP - 26 + 26 = TEST_SP, and result is at old_sp+26
    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP);

    // Result (window_ptr) is written at old sp + 26
    // Since the handler did bus.write_long(sp + param_size, window_ptr) before
    // advancing SP, the window_ptr lives at the current SP position.
    let window_ptr = bus.read_long(new_sp);
    assert_ne!(
        window_ptr, 0,
        "NewWindow should return a non-zero window pointer"
    );

    // front_window should be updated
    assert_eq!(disp.front_window, window_ptr);
}

#[test]
fn full_screen_new_window_preserves_initial_update_event() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc((800 * 600) as u32);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);
    bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);

    let bounds_rect_ptr = 0x2F0000;
    bus.write_word(bounds_rect_ptr, 0);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 600);
    bus.write_word(bounds_rect_ptr + 6, 800);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 6, 0xFFFF_FFFF); // frontmost
    bus.write_word(sp + 10, 0); // documentProc
    bus.write_byte(sp + 12, 1); // visible
    bus.write_long(sp + 18, bounds_rect_ptr);

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
    assert!(disp
        .event_queue
        .iter()
        .any(|event| event.what == 6 && event.message == window_ptr));
    assert!(disp.pending_update_event(&bus, 1u16 << 6).is_some());
}

fn new_window_content_probe(visible: bool) -> u8 {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = 0x300000;
    let row_bytes = 512;
    disp.set_screen_mode_for_test(screen_base, row_bytes, 512, 342, 8);
    bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);

    // Pick a pixel well inside the content region, away from the frame.
    // A newly exposed visible window must erase it to the default white
    // background before NewWindow returns. A hidden window must leave it
    // untouched.
    let probe = screen_base + 100 * row_bytes + 150;
    bus.write_byte(probe, 0x42);

    let bounds_rect_ptr = 0x2F0000;
    bus.write_word(bounds_rect_ptr, 80);
    bus.write_word(bounds_rect_ptr + 2, 100);
    bus.write_word(bounds_rect_ptr + 4, 136);
    bus.write_word(bounds_rect_ptr + 6, 396);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 6, 0xFFFF_FFFF); // frontmost
    bus.write_word(sp + 10, 1); // dBoxProc
    bus.write_byte(sp + 12, u8::from(visible));
    bus.write_long(sp + 18, bounds_rect_ptr);

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewWindow should be handled");
    assert!(result.unwrap().is_ok(), "NewWindow should return");
    bus.read_byte(probe)
}

#[test]
fn visible_new_window_erases_exposed_content_before_returning() {
    assert_eq!(
        new_window_content_probe(true),
        0,
        "visible NewWindow content must be erased to the default white background"
    );
}

#[test]
fn hidden_new_window_does_not_erase_screen_content() {
    assert_eq!(
        new_window_content_probe(false),
        0x42,
        "hidden NewWindow must not alter the framebuffer"
    );
}

// NewWindow must honor the `visible` parameter at SP+12 per IM:I I-299.
fn run_new_window_with_visible(visible_arg: u16) -> (u32, u8) {
    let (mut disp, mut cpu, mut bus) = setup();

    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 342);
    bus.write_word(bounds_rect_ptr + 6, 512);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds_rect_ptr);
    // MPW C pushes Pascal BOOLEAN with the value at the even offset
    // of its 2-byte stack slot (high byte of word) — write_byte at
    // sp + 12, NOT write_word(sp + 12, 1) which would land in the
    // ignored low byte.
    bus.write_byte(sp + 12, visible_arg as u8);

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let new_sp = cpu.read_reg(Register::A7);
    let window_ptr = bus.read_long(new_sp);
    let visible_byte = bus.read_byte(window_ptr + 110u32);
    (window_ptr, visible_byte)
}

#[test]
fn new_window_with_visible_true_sets_visible_byte_to_ff() {
    let (window_ptr, visible_byte) = run_new_window_with_visible(1);
    assert_ne!(window_ptr, 0);
    assert_eq!(
        visible_byte, 0xFF,
        "NewWindow(visible=true) must mark window visible"
    );
}

#[test]
fn new_window_with_visible_false_sets_visible_byte_to_zero() {
    let (window_ptr, visible_byte) = run_new_window_with_visible(0);
    assert_ne!(window_ptr, 0);
    assert_eq!(
        visible_byte, 0x00,
        "NewWindow(visible=false) must mark window invisible per IM:I I-299"
    );
}

#[test]
fn hidden_new_window_preserves_current_port() {
    let (mut disp, mut cpu, mut bus) = setup();
    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 342);
    bus.write_word(bounds_rect_ptr + 6, 512);

    let create_window = |disp: &mut super::super::TrapDispatcher,
                         cpu: &mut super::super::test_helpers::MockCpu,
                         bus: &mut crate::memory::MacMemoryBus,
                         visible: bool| {
        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_byte(sp + 12, if visible { 1 } else { 0 });
        bus.write_long(sp + 6, 0xFFFF_FFFF);

        let result = dispatch(disp, 0x113, cpu, bus);
        assert!(result.is_some(), "NewWindow should be handled");
        assert!(result.unwrap().is_ok(), "NewWindow should return");
        bus.read_long(cpu.read_reg(Register::A7))
    };

    let base = create_window(&mut disp, &mut cpu, &mut bus, true);
    assert_eq!(*disp.current_port, base);

    let hidden = create_window(&mut disp, &mut cpu, &mut bus, false);
    assert_ne!(hidden, 0);
    assert_eq!(
        *disp.current_port, base,
        "hidden NewWindow must preserve the caller's current port"
    );
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::THE_PORT),
        base,
        "hidden NewWindow must preserve low-memory thePort"
    );
    let qd_globals = bus.read_long(cpu.read_reg(Register::A5));
    assert_eq!(
        bus.read_long(qd_globals),
        base,
        "hidden NewWindow must preserve qd.thePort"
    );
}

#[test]
fn backmost_visible_new_window_preserves_current_port() {
    let (mut disp, mut cpu, mut bus) = setup();
    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 342);
    bus.write_word(bounds_rect_ptr + 6, 512);

    let create_window = |disp: &mut super::super::TrapDispatcher,
                         cpu: &mut super::super::test_helpers::MockCpu,
                         bus: &mut crate::memory::MacMemoryBus,
                         behind: u32| {
        let sp = TEST_SP - 26;
        cpu.write_reg(Register::A7, sp);
        for i in 0..30u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_long(sp + 18, bounds_rect_ptr);
        bus.write_byte(sp + 12, 1);
        bus.write_long(sp + 6, behind);

        let result = dispatch(disp, 0x113, cpu, bus);
        assert!(result.is_some(), "NewWindow should be handled");
        assert!(result.unwrap().is_ok(), "NewWindow should return");
        bus.read_long(cpu.read_reg(Register::A7))
    };

    let base = create_window(&mut disp, &mut cpu, &mut bus, 0xFFFF_FFFF);
    assert_eq!(*disp.current_port, base);

    let back = create_window(&mut disp, &mut cpu, &mut bus, 0);
    assert_ne!(back, 0);
    assert_eq!(
        *disp.current_port, base,
        "visible NewWindow with behind=NIL must preserve the caller's current port"
    );
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::THE_PORT),
        base,
        "backmost NewWindow must preserve low-memory thePort"
    );
    let qd_globals = bus.read_long(cpu.read_reg(Register::A5));
    assert_eq!(
        bus.read_long(qd_globals),
        base,
        "backmost NewWindow must preserve qd.thePort"
    );
}

// NewWindow must honor the `behind` parameter at SP+6 per IM:I I-299:
//   behind == -1  → frontmost (default)
//   behind == NIL → backmost
//   behind == X   → immediately behind X
fn run_new_window_with_behind(behind: u32) -> (u32, Vec<u32>, u32) {
    let (mut disp, mut cpu, mut bus) = setup();

    // Pre-seed an existing window so `behind` has a meaningful
    // target for the middle-insert case.
    let existing = 0x200040u32;
    disp.window_list.replace(vec![existing]);
    disp.front_window = existing;
    bus.write_byte(existing + 110u32, 0xFF); // visible

    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 342);
    bus.write_word(bounds_rect_ptr + 6, 512);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds_rect_ptr);
    bus.write_byte(sp + 12, 1); // Pascal Boolean occupies the high byte.
    bus.write_long(sp + 6, behind);

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let new_sp = cpu.read_reg(Register::A7);
    let window_ptr = bus.read_long(new_sp);
    (window_ptr, disp.window_list.to_vec(), disp.front_window)
}

#[test]
fn new_window_behind_minus_one_places_new_window_at_front() {
    let (window_ptr, list, front) = run_new_window_with_behind(0xFFFFFFFF);
    assert_eq!(
        list[0], window_ptr,
        "behind=-1 must put the new window at the front"
    );
    assert_eq!(front, window_ptr);
}

#[test]
fn new_window_behind_nil_places_new_window_at_back() {
    let (window_ptr, list, front) = run_new_window_with_behind(0);
    let existing = 0x200040u32;
    assert_eq!(
        list,
        vec![existing, window_ptr],
        "behind=NIL must put the new window at the back"
    );
    assert_eq!(
        front, existing,
        "front_window must stay on the pre-existing visible window"
    );
}

// NewWindow / NewCWindow / GetNewWindow / GetNewCWindow must honor
// the Pascal `wStorage` parameter per IM:I I-299: "If wStorage is
// NIL, NewWindow allocates the necessary storage itself; otherwise
// it uses the storage pointed to by wStorage."
#[test]
fn new_window_uses_caller_supplied_storage() {
    let (mut disp, mut cpu, mut bus) = setup();

    let storage = bus.alloc(512);
    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 342);
    bus.write_word(bounds_rect_ptr + 6, 512);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds_rect_ptr);
    bus.write_long(sp + 22, storage); // wStorage
    bus.write_word(sp + 12, 1); // visible
    bus.write_long(sp + 6, 0xFFFFFFFF); // behind = -1 (frontmost)

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
    assert_eq!(
        window_ptr, storage,
        "NewWindow with non-NIL wStorage must return the caller's pointer"
    );
}

#[test]
fn new_window_nil_storage_still_allocates() {
    let (mut disp, mut cpu, mut bus) = setup();

    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 342);
    bus.write_word(bounds_rect_ptr + 6, 512);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds_rect_ptr);
    bus.write_long(sp + 22, 0); // wStorage = NIL
    bus.write_word(sp + 12, 1);
    bus.write_long(sp + 6, 0xFFFFFFFF);

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
    assert_ne!(
        window_ptr, 0,
        "NewWindow with NIL wStorage must fall back to bus.alloc"
    );
}

#[test]
fn new_window_publishes_windowlist_lowmem_global() {
    // Inside Macintosh Volume I, I-299/I-301: Window Manager calls
    // insert a new visible window into the window list. Assembly
    // callers can read the front pointer through low-memory WindowList.
    let (mut disp, mut cpu, mut bus) = setup();

    let bounds_rect_ptr: u32 = 0x300000;
    bus.write_word(bounds_rect_ptr, 40);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 342);
    bus.write_word(bounds_rect_ptr + 6, 512);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds_rect_ptr);
    bus.write_byte(sp + 12, 1); // visible
    bus.write_long(sp + 6, 0xFFFF_FFFF); // behind = frontmost

    let result = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));

    assert_eq!(
        bus.read_long(0x09D6),
        window_ptr,
        "low-memory WindowList must point at the front window"
    );
    assert_eq!(
        bus.read_long(window_ptr + 144),
        0,
        "single-window list should have a NIL nextWindow link"
    );
}

#[test]
fn closewindow_clears_windowlist_lowmem_when_last_window_removed() {
    // IM:I I-282/I-283 exposes the window list through low memory;
    // closing the final tracked window must publish NIL.
    let (mut disp, mut cpu, mut bus) = setup();
    let window_ptr = 0x200040u32;
    disp.window_list.replace(vec![window_ptr]);
    disp.front_window = window_ptr;
    bus.write_byte(window_ptr + 110u32, 0xFF);
    bus.write_long(0x09D6, window_ptr);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);

    let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        bus.read_long(0x09D6),
        0,
        "low-memory WindowList must be NIL after removing the last window"
    );
}

#[test]
fn get_new_window_uses_caller_supplied_storage() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (40, 0, 342, 512),
        0,
        true,
        false,
        0,
        b"Doc",
    );

    let storage = bus.alloc(512);
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128); // windowID
    bus.write_long(sp + 4, storage); // wStorage
    bus.write_long(sp, 0xFFFFFFFF); // behind = -1

    let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
    assert_eq!(
        window_ptr, storage,
        "GetNewWindow with non-NIL wStorage must return the caller's pointer"
    );
}

#[test]
fn new_cwindow_uses_caller_supplied_storage() {
    let (mut disp, mut cpu, mut bus) = setup();

    let storage = bus.alloc(512);
    let bounds_rect_ptr = 0x301200u32;
    bus.write_word(bounds_rect_ptr, 0);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 600);
    bus.write_word(bounds_rect_ptr + 6, 800);

    let sp = TEST_SP - 30;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 10, 2); // procID
    bus.write_byte(sp + 12, 1); // visible
    bus.write_long(sp + 18, bounds_rect_ptr);
    bus.write_long(sp + 22, storage); // wStorage
    bus.write_long(sp + 6, 0xFFFFFFFF); // behind = frontmost

    let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
    assert_eq!(
        window_ptr, storage,
        "NewCWindow with non-NIL wStorage must return the caller's pointer"
    );
}

#[test]
fn get_new_cwindow_uses_caller_supplied_storage() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (0, 0, 600, 800),
        2,
        true,
        false,
        0,
        b"CWin",
    );

    let storage = bus.alloc(512);
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128);
    bus.write_long(sp + 4, storage); // wStorage
    bus.write_long(sp, 0xFFFFFFFF); // behind = frontmost

    let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let window_ptr = bus.read_long(cpu.read_reg(Register::A7));
    assert_eq!(
        window_ptr, storage,
        "GetNewCWindow with non-NIL wStorage must return the caller's pointer"
    );
}

#[test]
fn new_window_behind_specific_window_inserts_just_after_target() {
    let existing = 0x200040u32;
    let (window_ptr, list, front) = run_new_window_with_behind(existing);
    assert_eq!(
        list,
        vec![existing, window_ptr],
        "behind=existing must insert the new window immediately behind it"
    );
    assert_eq!(front, existing);
}

// NewCWindow ($AA45), GetNewWindow ($A9BD), and GetNewCWindow ($AA46)
// must also honor the Pascal `behind` parameter. NewCWindow has the
// same stack layout as NewWindow (behind at SP+6). GetNewWindow /
// GetNewCWindow share a different 10-byte stack with behind at SP+0.
#[test]
fn new_cwindow_behind_nil_places_new_window_at_back() {
    let (mut disp, mut cpu, mut bus) = setup();
    let existing = 0x200040u32;
    disp.window_list.replace(vec![existing]);
    disp.front_window = existing;
    bus.write_byte(existing + 110u32, 0xFF);

    let bounds_rect_ptr = 0x301200u32;
    bus.write_word(bounds_rect_ptr, 0);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 600);
    bus.write_word(bounds_rect_ptr + 6, 800);

    let sp = TEST_SP - 30;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 10, 0); // procID
    bus.write_word(sp + 12, 1); // visible
    bus.write_long(sp + 18, bounds_rect_ptr);
    bus.write_long(sp + 6, 0); // behind = NIL (backmost)

    let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let new_sp = cpu.read_reg(Register::A7);
    let window_ptr = bus.read_long(new_sp);
    assert_eq!(
        disp.window_list,
        vec![existing, window_ptr],
        "NewCWindow(behind=NIL) must insert at the back"
    );
    assert_eq!(disp.front_window, existing);
}

#[test]
fn get_new_window_reads_behind_at_sp_plus_0() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (40, 0, 342, 512),
        0,
        true,
        false,
        0,
        b"Doc",
    );
    let existing = 0x200040u32;
    disp.window_list.replace(vec![existing]);
    disp.front_window = existing;
    bus.write_byte(existing + 110u32, 0xFF);

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128); // windowID
    bus.write_long(sp, 0); // behind = NIL

    let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let new_sp = cpu.read_reg(Register::A7);
    let window_ptr = bus.read_long(new_sp);
    assert_eq!(
        disp.window_list,
        vec![existing, window_ptr],
        "GetNewWindow(behind=NIL) must insert at the back"
    );
}

#[test]
fn get_new_cwindow_reads_behind_at_sp_plus_0() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (0, 0, 600, 800),
        2,
        true,
        false,
        0,
        b"CWin",
    );
    let existing = 0x200040u32;
    disp.window_list.replace(vec![existing]);
    disp.front_window = existing;
    bus.write_byte(existing + 110u32, 0xFF);

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128); // windowID
    bus.write_long(sp, existing); // behind = existing → insert after

    let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let new_sp = cpu.read_reg(Register::A7);
    let window_ptr = bus.read_long(new_sp);
    assert_eq!(
        disp.window_list,
        vec![existing, window_ptr],
        "GetNewCWindow(behind=existing) must insert immediately behind it"
    );
}

#[test]
fn apply_behind_parameter_refreshes_cached_front_window_state() {
    let (mut disp, _cpu, mut bus) = setup();
    let front = bus.alloc(200);
    let back = bus.alloc(200);

    for &(window, rect) in &[(front, (10, 20, 110, 220)), (back, (0, 0, 600, 800))] {
        bus.write_byte(
            window + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
            0xFF,
        );
        bus.write_word(window + 8, 0);
        bus.write_word(window + 10, 0);
        bus.write_word(window + 16, rect.0 as u16);
        bus.write_word(window + 18, rect.1 as u16);
        bus.write_word(window + 20, rect.2 as u16);
        bus.write_word(window + 22, rect.3 as u16);
    }

    disp.window_list.replace(vec![front, back]);
    disp.front_window = back;
    disp.window_bounds = (0, 0, 600, 800);

    disp.apply_behind_parameter(&mut bus, back, 0);

    assert_eq!(disp.front_window, front);
    assert_eq!(
        disp.window_bounds,
        (10, 20, 110, 220),
        "cached front-window geometry must follow the visible front after behind=NIL reorders"
    );
}

#[test]
fn apply_behind_parameter_invalidates_windows_clobbered_during_creation() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let existing = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        existing,
        screen_base,
        20,
        20,
        300,
        400,
        "Existing",
        4,
        true,
        false,
        false,
        0,
    );
    disp.validate_window_rect(&mut bus, existing, (0, 0, 280, 380));

    let created = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        created,
        screen_base,
        60,
        80,
        180,
        300,
        "Created",
        1,
        true,
        false,
        true,
        0,
    );
    disp.validate_window_rect(&mut bus, created, (0, 0, 120, 220));

    disp.apply_behind_parameter(&mut bus, created, existing);

    assert_eq!(disp.window_list, vec![existing, created]);
    let update = super::super::TrapDispatcher::region_handle_rect(
        &bus,
        bus.read_long(existing + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET),
    );
    assert!(
        update.is_some(),
        "the window left in front must redraw pixels clobbered by creation"
    );
    assert!(disp
        .event_queue
        .iter()
        .any(|event| event.what == 6 && event.message == existing));
}

#[test]
fn get_new_cwindow_frontmost_visible_queues_activate_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (0, 0, 600, 800),
        2,
        true,
        false,
        0,
        b"CWin",
    );
    let existing = 0x200040u32;
    disp.window_list.replace(vec![existing]);
    disp.front_window = existing;
    bus.write_byte(
        existing + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
        0xFF,
    );
    bus.write_byte(
        existing + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128); // windowID
    bus.write_long(sp, 0xFFFF_FFFF); // behind = -1/frontmost

    let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let new_sp = cpu.read_reg(Register::A7);
    let window_ptr = bus.read_long(new_sp);

    assert_eq!(
        disp.window_list.first(),
        Some(window_ptr),
        "GetNewCWindow(behind=-1) must keep the new visible window frontmost"
    );
    assert_eq!(disp.front_window, window_ptr);
    assert_eq!(
        bus.read_byte(existing + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0x00,
        "the previous front window should be unhilited"
    );
    assert_eq!(
        bus.read_byte(window_ptr + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0xFF,
        "the created front window should be hilited"
    );

    let activate_events: Vec<_> = disp
        .event_queue
        .iter()
        .filter(|event| event.what == 8)
        .collect();
    assert_eq!(activate_events.len(), 2);
    assert_eq!(activate_events[0].message, existing);
    assert_eq!(activate_events[0].modifiers & 1, 0);
    assert_eq!(activate_events[1].message, window_ptr);
    assert_eq!(activate_events[1].modifiers & 1, 1);
}

// ---------------------------------------------------------------
// 3. GetNewWindow (0x1BD) -- 10 bytes params, result at SP+10
// ---------------------------------------------------------------
#[test]
fn test_get_new_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (40, 0, 342, 512),
        0,
        true,
        false,
        0,
        b"Doc",
    );

    // Push 10 bytes of params: SP+0..SP+7 = behind(4)+wStorage(4), SP+8 = window_id(2)
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    // window_id at SP+8
    bus.write_word(sp + 8, 128);

    let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP);

    let window_ptr = bus.read_long(new_sp);
    assert_ne!(
        window_ptr, 0,
        "GetNewWindow should return a non-zero window pointer"
    );
    assert_eq!(disp.front_window, window_ptr);
}

#[test]
fn classic_wind_template_does_not_read_a_position_word_past_its_length() {
    let (_disp, _cpu, mut bus) = setup();
    let wind_ptr = bus.alloc(22);
    bus.write_word(wind_ptr + 4, 100);
    bus.write_word(wind_ptr + 6, 200);
    bus.write_byte(wind_ptr + 18, 0);
    bus.write_word(wind_ptr + 20, 0x280A);

    let template = super::super::TrapDispatcher::parse_wind_template(&bus, wind_ptr, 19)
        .expect("classic WIND template");
    assert_eq!(template.bounds, (0, 0, 100, 200));
    assert_eq!(template.title, "");
    assert_eq!(
        template.position, 0,
        "bytes beyond the classic resource length must not be parsed"
    );
}

#[test]
fn wind_template_preserves_mac_roman_title_bytes() {
    let (_disp, _cpu, mut bus) = setup();
    let wind_ptr = bus.alloc(24);
    bus.write_byte(wind_ptr + 18, 4);
    bus.write_bytes(wind_ptr + 19, b"DLB\xAA");

    let template = super::super::TrapDispatcher::parse_wind_template(&bus, wind_ptr, 23)
        .expect("WIND template with Mac Roman title");
    assert_eq!(template.title, "DLB™");
}

#[test]
fn get_new_window_constructors_apply_center_main_screen_positioning() {
    for trap_num in [0x1BD, 0x246] {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = bus.alloc((800 * 600) as u32);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        install_positioned_wind_resource(
            &mut disp,
            &mut bus,
            128,
            (42, 7, 436, 502),
            5,
            true,
            false,
            0,
            b"",
            Some(0x280A),
        );

        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        for i in 0..10u32 {
            bus.write_byte(sp + i, 0);
        }
        bus.write_word(sp + 8, 128);

        let result = dispatch(&mut disp, trap_num, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        let window_ptr = bus.read_long(TEST_SP);
        assert_ne!(window_ptr, 0);
        assert_eq!(
            disp.window_global_port_rect(&bus, window_ptr),
            (121, 152, 515, 647),
            "trap ${trap_num:03X} should center the complete WIND structure below the menu bar"
        );
    }
}

/// GetNewWindow creates an old-style GrafPort even when Color QuickDraw
/// and an 8bpp screen are available. Macintosh Toolbox Essentials (1992),
/// pp. 4-78..4-79, distinguishes it from GetNewCWindow on exactly this
/// contract.
#[test]
fn get_new_window_exposes_embedded_grafport_bitmap_on_color_screen() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (0, 0, 600, 800),
        2,
        true,
        false,
        0,
        b"CWin",
    );

    // Set up an 8bpp 800×600 screen mirroring the play-runner.
    let screen_base = bus.alloc((800 * 600) as u32);
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);

    // Standard GetNewWindow stack: SP+0..7 = behind+wStorage,
    // SP+8 = window_id.
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128);

    let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let new_sp = cpu.read_reg(Register::A7);
    let window_ptr = bus.read_long(new_sp);
    assert_ne!(window_ptr, 0);

    assert_eq!(bus.read_long(window_ptr + 2), screen_base);
    assert_eq!(bus.read_word(window_ptr + 6), 800);
    assert_eq!(bus.read_word(window_ptr + 8), 0);
    assert_eq!(bus.read_word(window_ptr + 10), 0);
    assert_eq!(bus.read_word(window_ptr + 12), 600);
    assert_eq!(bus.read_word(window_ptr + 14), 800);

    let private_pm_handle = disp.window_original_pixmaps[&window_ptr];
    let private_pm = bus.read_long(private_pm_handle);
    assert_eq!(bus.read_word(private_pm + 32), 8);
}

// ---------------------------------------------------------------
// 4. NewCWindow (0x245) -- 26 bytes of params + 4 result
// 68K Pascal stack (BOOLEAN = 2 bytes on A7):
//   SP+0: refCon(4) SP+4: goAwayFlag(2) SP+6: behind(4)
//   SP+10: procID(2) SP+12: visible(2)  SP+14: title(4)
//   SP+18: boundsRect(4) SP+22: wStorage(4) SP+26: result(4)
// ---------------------------------------------------------------
#[test]
fn test_new_cwindow_0x245() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::ForceHidden);
    let screen_base = bus.alloc((800 * 600) as u32);
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);
    // Pre-fill framebuffer with 0x42 so we can verify it gets erased
    bus.write_byte(screen_base, 0x42);
    bus.write_byte(screen_base + 1, 0x42);

    // Set up a bounds rect in memory
    let bounds_rect_ptr = 0x301200u32;
    bus.write_word(bounds_rect_ptr, 0); // top
    bus.write_word(bounds_rect_ptr + 2, 0); // left
    bus.write_word(bounds_rect_ptr + 4, 600); // bottom
    bus.write_word(bounds_rect_ptr + 6, 800); // right

    let sp = TEST_SP - 30; // 26 params + 4 result
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 10, 2); // procID = plainDBox
                                // Pascal BOOLEAN at SP+12 — value goes in HIGH byte (MPW C convention).
    bus.write_byte(sp + 12, 1); // visible
    bus.write_long(sp + 18, bounds_rect_ptr); // boundsRect at SP+18

    let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    // Handler writes result at sp+26, sets SP = sp+26
    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, sp + 26);

    let window_ptr = bus.read_long(new_sp);
    assert_ne!(
        window_ptr, 0,
        "NewCWindow should return a non-zero window pointer"
    );
    assert_eq!(disp.front_window, window_ptr);

    // Verify it creates a CGrafPort: portVersion at offset +6 should have 0xC000
    let port_version = bus.read_word(window_ptr + 6);
    assert_eq!(port_version, 0xC000, "NewCWindow should set CGrafPort flag");
    // In explicit kiosk mode the Mac desktop is hidden, so fullscreen
    // windows erase exposed framebuffer areas to the black host stage.
    assert_eq!(
        bus.read_byte(screen_base),
        255,
        "fullscreen NewCWindow should erase framebuffer to the black kiosk stage"
    );
}

#[test]
fn kiosk_new_cwindow_suppresses_initial_document_chrome_erase() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.set_menu_bar_policy(crate::runner::MenuBarPolicy::ForceHidden);
    let screen_base = bus.alloc((800 * 600) as u32);
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);

    let probe = screen_base + 10 * 800 + 10;
    bus.write_byte(probe, 0x42);

    let bounds_rect_ptr = 0x301200u32;
    bus.write_word(bounds_rect_ptr, 20); // below classic menu bar
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 600);
    bus.write_word(bounds_rect_ptr + 6, 800);

    let sp = TEST_SP - 30;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 10, 4); // noGrowDocProc
    bus.write_byte(sp + 12, 1); // visible
    bus.write_long(sp + 18, bounds_rect_ptr);

    let result = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        bus.read_byte(probe),
        0x42,
        "kiosk document-window creation must not white-erase the hidden desktop"
    );
}

// ---------------------------------------------------------------
// 5. GetNewCWindow (0x246) -- 10 bytes params (windowID, wStorage, behind)
// ---------------------------------------------------------------
#[test]
fn test_get_new_cwindow_0x246() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (0, 0, 600, 800),
        2,
        true,
        false,
        0,
        b"CWin",
    );

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128);

    let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP);

    let window_ptr = bus.read_long(new_sp);
    assert_ne!(
        window_ptr, 0,
        "GetNewCWindow 0x246 should return a non-zero window pointer"
    );
    assert_eq!(disp.front_window, window_ptr);

    let port_version = bus.read_word(window_ptr + 6);
    assert_eq!(port_version, 0xC000);
}

fn assert_get_new_window_reloads_released_wind(trap_num: u16) {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wind_resource(
        &mut disp,
        &mut bus,
        128,
        (20, 30, 220, 330),
        0,
        true,
        false,
        0,
        b"Reloaded",
    );

    let released_ptr = disp
        .with_resource_file_mut_for_test(0, |file| file.loaded.insert((*b"WIND", 128), 0))
        .flatten()
        .expect("installed WIND pointer");
    bus.free(released_ptr);

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 128);

    let result = dispatch(&mut disp, trap_num, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_ne!(
        bus.read_long(TEST_SP),
        0,
        "window constructor must rematerialize a released WIND template"
    );
}

#[test]
fn get_new_window_reloads_released_wind_resource() {
    assert_get_new_window_reloads_released_wind(0x1BD);
}

#[test]
fn get_new_cwindow_reloads_released_wind_resource() {
    assert_get_new_window_reloads_released_wind(0x246);
}

// MTE 1992 pp. 4-77..4-78: GetNewCWindow/GetNewWindow return NIL
// when the WIND template (or defproc) cannot be read.
#[test]
fn get_new_window_missing_wind_returns_nil() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 999); // missing WIND id

    let result = dispatch(&mut disp, 0x1BD, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_long(TEST_SP), 0, "missing WIND must return NIL");
}

#[test]
fn get_new_cwindow_missing_wind_returns_nil() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    for i in 0..10u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_word(sp + 8, 999); // missing WIND id

    let result = dispatch(&mut disp, 0x246, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_long(TEST_SP), 0, "missing WIND must return NIL");
}

// ---------------------------------------------------------------
// 6. CloseWindow (0x12D) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_close_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xDEAD0000); // window ptr

    let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn closewindow_front_promotion_highlights_next_visible_and_queues_activate_event() {
    // IM:I I-283: closing the front window promotes the window behind,
    // highlights it, and generates an activate event.
    let (mut disp, mut cpu, mut bus) = setup();
    let front = 0x200040u32;
    let next = 0x200140u32;
    disp.window_list.replace(vec![front, next]);
    disp.front_window = front;
    disp.current_port
        .with_mut(|current_port| *current_port = front);
    disp.window_bounds = (240, 450, 480, 650);
    disp.window_proc_id = 4;
    disp.window_title = "Player".to_string();
    disp.go_away_flag = true;
    for &base in &[front, next] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
        bus.write_byte(base + 110u32, 0xFF);
    }
    bus.write_byte(front + 111u32, 0xFF);
    bus.write_byte(next + 111u32, 0x00);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, front);

    let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(disp.front_window, next);
    assert_eq!(*disp.current_port, next);
    assert_eq!(
        disp.window_bounds,
        (10, 10, 50, 100),
        "CloseWindow front promotion must refresh cached render bounds \
             before later NewWindow calls redraw the previous front inactive"
    );
    assert_eq!(disp.window_title, "");
    assert!(!disp.go_away_flag);
    assert_eq!(bus.read_byte(front + 111u32), 0x00);
    assert_eq!(bus.read_byte(next + 111u32), 0xFF);
    assert_eq!(disp.window_list, vec![next]);
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 8 && event.message == next && (event.modifiers & 1) == 1),
        "CloseWindow front promotion must queue activate event for new front window"
    );
}

#[test]
fn closewindow_invalidates_document_exposed_behind_floating_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let document = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        document,
        screen_base,
        40,
        40,
        300,
        500,
        "Board",
        0,
        true,
        true,
        true,
        0,
    );
    let utility = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        utility,
        screen_base,
        80,
        100,
        220,
        420,
        "Scores",
        1,
        true,
        true,
        false,
        0,
    );
    disp.window_list.replace(vec![utility, document]);
    disp.sync_window_list_links(&mut bus);
    // Floating utilities may remain above an active document without
    // becoming the Window Manager's active front window.
    disp.front_window = document;
    disp.current_port
        .with_mut(|current_port| *current_port = document);
    disp.dialog_visible_snapshots.insert(
        utility,
        PersistentDialogSnapshot {
            bounds: (80, 100, 220, 420),
            pixels: vec![0xEE; 140 * 320].into(),
        },
    );
    let update_handle = bus.read_long(document + 122);
    super::super::TrapDispatcher::write_region_handle_rect(&mut bus, update_handle, None);
    disp.clear_queued_update_events(document);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, utility);
    let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    let update = disp
        .window_update_rect(&bus, document)
        .expect("closing the overlapping utility should expose document content");
    assert!(update.0 <= 80 && update.1 <= 100);
    assert!(update.2 >= 220 && update.3 >= 420);
    assert!(disp
        .event_queue
        .iter()
        .any(|event| { event.what == 6 && event.message == document }));
    assert!(!disp.dialog_visible_snapshots.contains_key(&utility));
}

// ---------------------------------------------------------------
// 7. DisposeWindow (0x114) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_dispose_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xDEAD0000);

    let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn disposewindow_front_promotion_highlights_next_visible_and_queues_activate_event() {
    // IM:I I-284: DisposeWindow calls CloseWindow; IM:I I-283 promotion
    // side effects still apply when disposing a front window.
    let (mut disp, mut cpu, mut bus) = setup();
    let front = 0x200040u32;
    let next = 0x200140u32;
    disp.window_list.replace(vec![front, next]);
    disp.front_window = front;
    disp.current_port
        .with_mut(|current_port| *current_port = front);
    disp.window_bounds = (240, 450, 480, 650);
    disp.window_proc_id = 4;
    disp.window_title = "Player".to_string();
    disp.go_away_flag = true;
    for &base in &[front, next] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
        bus.write_byte(base + 110u32, 0xFF);
    }
    bus.write_byte(front + 111u32, 0xFF);
    bus.write_byte(next + 111u32, 0x00);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, front);

    let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(disp.front_window, next);
    assert_eq!(*disp.current_port, next);
    assert_eq!(
        disp.window_bounds,
        (10, 10, 50, 100),
        "DisposeWindow front promotion must refresh cached render bounds \
             before later NewWindow calls redraw the previous front inactive"
    );
    assert_eq!(disp.window_title, "");
    assert!(!disp.go_away_flag);
    assert_eq!(bus.read_byte(front + 111u32), 0x00);
    assert_eq!(bus.read_byte(next + 111u32), 0xFF);
    assert_eq!(disp.window_list, vec![next]);
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 8 && event.message == next && (event.modifiers & 1) == 1),
        "DisposeWindow front promotion must queue activate event for new front window"
    );
}

#[test]
fn disposewindow_erases_visible_window_from_screen() {
    // IM:I I-284: DisposeWindow calls CloseWindow; IM:I I-283 says
    // CloseWindow removes the window from the screen and window list.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);
    disp.menu_bar_hidden = false;
    disp.fill_theme_desktop_rect(&mut bus, 0, 0, 600, 800);

    let content_probe = screen_base + 250 * 800 + 460;
    let right_frame_probe = screen_base + 300 * 800 + 651;
    let title_frame_probe = screen_base + 223 * 800 + 628;
    let desktop_content = bus.read_byte(content_probe);
    let desktop_right_frame = bus.read_byte(right_frame_probe);
    let desktop_title_frame = bus.read_byte(title_frame_probe);
    let window_addr = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        screen_base,
        240,
        450,
        480,
        650,
        "Player",
        4,
        true,
        true,
        true,
        0,
    );
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;
    disp.current_port
        .with_mut(|current_port| *current_port = window_addr);
    assert_ne!(
        bus.read_byte(content_probe),
        desktop_content,
        "precondition: visible window content covers the desktop pixel"
    );
    assert_ne!(
        bus.read_byte(right_frame_probe),
        desktop_right_frame,
        "precondition: visible window frame covers the right-edge desktop pixel"
    );
    assert_ne!(
        bus.read_byte(title_frame_probe),
        desktop_title_frame,
        "precondition: visible window frame covers the title-bar desktop pixel"
    );

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);

    let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(
        bus.read_byte(content_probe),
        desktop_content,
        "DisposeWindow must remove the visible window pixels from the screen"
    );
    assert_eq!(
        bus.read_byte(right_frame_probe),
        desktop_right_frame,
        "DisposeWindow must erase the visible right frame from the screen"
    );
    assert_eq!(
        bus.read_byte(title_frame_probe),
        desktop_title_frame,
        "DisposeWindow must erase the visible title frame from the screen"
    );
    assert_eq!(
        bus.read_byte(window_addr + 110u32),
        0x00,
        "disposed window should no longer be marked visible"
    );
    assert!(!disp.window_list.contains(&window_addr));
}

// ---------------------------------------------------------------
// 8. SelectWindow (0x11F) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_select_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Seed two windows so SelectWindow has a target to move to
    // front; a garbage pointer would short-circuit out of the
    // event-generating path.
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_a, win_b]);
    disp.front_window = win_a;
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0xFF);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_b);

    let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(disp.front_window, win_b);
}

// SelectWindow must hilite the new front, unhilite the old front,
// and queue deactivate+activate events per IM:I I-286.
#[test]
fn select_window_hilites_and_queues_activate_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_a, win_b]);
    disp.front_window = win_a;
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0xFF);
    // Start with A hilited (it's front).
    bus.write_byte(win_a + 111u32, 0xFF);
    bus.write_byte(win_b + 111u32, 0x00);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_b);

    let queue_len_before = disp.event_queue.len();
    let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(disp.front_window, win_b, "B must become front");
    assert_eq!(
        bus.read_byte(win_a + 111u32),
        0x00,
        "old front A must be unhilited"
    );
    assert_eq!(
        bus.read_byte(win_b + 111u32),
        0xFF,
        "new front B must be hilited"
    );
    assert_eq!(
        disp.event_queue.len() - queue_len_before,
        2,
        "exactly two events (deactivate A + activate B) must be queued"
    );
    // Find both events in the queue.
    let events: Vec<_> = disp.event_queue.iter().skip(queue_len_before).collect();
    assert_eq!(events[0].what, 8, "first must be activate-event class");
    assert_eq!(events[0].message, win_a);
    assert_eq!(events[0].modifiers & 1, 0, "A's event is deactivate");
    assert_eq!(events[1].what, 8);
    assert_eq!(events[1].message, win_b);
    assert_eq!(events[1].modifiers & 1, 1, "B's event is activate");
}

#[test]
fn pending_activation_slots_replace_superseded_window_transitions() {
    // Activate events go directly to the Event Manager rather than the
    // Operating System event queue, and the Window Manager exposes only
    // CurActivate and CurDeactive slots. Rapid front-window changes before
    // the next event poll must therefore replace, not accumulate, each
    // class of pending event.
    // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 2-51;
    // A/UX Toolbox: Macintosh ROM Interface (1990), Appendix D.
    let (mut disp, _cpu, mut bus) = setup();
    let first = 0x200040u32;
    let middle = 0x200140u32;
    let last = 0x200240u32;

    disp.queue_window_activation_event(&mut bus, first, false);
    disp.queue_window_activation_event(&mut bus, middle, true);
    disp.queue_window_activation_event(&mut bus, middle, false);
    disp.queue_window_activation_event(&mut bus, last, true);

    let events: Vec<_> = disp
        .event_queue
        .iter()
        .filter(|event| event.what == 8)
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].message, middle);
    assert_eq!(events[0].modifiers & 1, 0);
    assert_eq!(events[1].message, last);
    assert_eq!(events[1].modifiers & 1, 1);
    assert_eq!(
        bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_DEACTIVATE),
        middle
    );
    assert_eq!(
        bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_ACTIVATE),
        last
    );

    disp.acknowledge_window_activation_event(&mut bus, &events[0]);
    assert_eq!(
        bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_DEACTIVATE),
        0
    );
    assert_eq!(
        bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_ACTIVATE),
        last
    );
}

#[test]
fn invisible_frontmost_window_creation_preserves_active_window() {
    // The native Window Manager leaves pending activation unchanged when
    // NewWindow/NewCWindow creates an invisible window at the front.
    for trap in [0x113, 0x245] {
        let (mut disp, mut cpu, mut bus) = setup();
        let bounds = bus.alloc(8);
        bus.write_word(bounds, 40);
        bus.write_word(bounds + 2, 50);
        bus.write_word(bounds + 4, 180);
        bus.write_word(bounds + 6, 250);
        let mut old_front = 0;

        for visible in [true, false] {
            let sp = TEST_SP - 30;
            cpu.write_reg(Register::A7, sp);
            bus.write_bytes(sp, &[0; 30]);
            bus.write_long(sp + 6, u32::MAX);
            bus.write_byte(sp + 12, u8::from(visible));
            bus.write_long(sp + 18, bounds);
            dispatch(&mut disp, trap, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            let window = bus.read_long(cpu.read_reg(Register::A7));
            assert_ne!(window, 0);
            if visible {
                old_front = window;
                assert_eq!(disp.front_window, window);
                disp.event_queue.clear();
                bus.write_long(super::super::TrapDispatcher::LOWMEM_CUR_ACTIVATE, 0);
                bus.write_long(super::super::TrapDispatcher::LOWMEM_CUR_DEACTIVATE, 0);
                continue;
            }

            assert_eq!(disp.window_list.first(), Some(window));
            assert_eq!(disp.front_window, old_front);
            assert_ne!(bus.read_byte(old_front + 111), 0);
            assert_eq!(bus.read_byte(window + 111), 0);
            assert!(!disp.event_queue.iter().any(|event| event.what == 8));
            assert_eq!(
                bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_ACTIVATE),
                0
            );
            assert_eq!(
                bus.read_long(super::super::TrapDispatcher::LOWMEM_CUR_DEACTIVATE),
                0
            );
        }
    }
}

#[test]
fn select_window_already_front_is_idempotent() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    disp.window_list.replace(vec![win_a]);
    disp.front_window = win_a;
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_a + 111u32, 0xFF);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_a);

    let queue_len_before = disp.event_queue.len();
    let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        disp.event_queue.len(),
        queue_len_before,
        "SelectWindow on already-front window must not queue any events per IM:I I-286"
    );
}

#[test]
fn select_window_nil_is_safe() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);

    let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// ---------------------------------------------------------------
// 9. ShowWindow (0x115) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_show_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xDEAD0000);

    let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn showwindow_already_visible_does_not_requeue_full_update() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x200040u32;
    let (_cont_rgn, update_rgn) = setup_full_window_with_regions(&mut bus, window, 20, 0, 424, 627);
    bus.write_byte(window + 110, 0xFF);
    bus.write_byte(window + 111, 0xFF);
    disp.window_list.replace(vec![window]);
    disp.front_window = window;
    disp.current_port
        .with_mut(|current_port| *current_port = window);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window);

    let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert!(
        !disp
            .event_queue
            .iter()
            .any(|e| e.what == 6 && e.message == window),
        "ShowWindow on an already visible window must not queue a new updateEvt"
    );
    assert_eq!(
        (
            bus.read_word(update_rgn + 2) as i16,
            bus.read_word(update_rgn + 4) as i16,
            bus.read_word(update_rgn + 6) as i16,
            bus.read_word(update_rgn + 8) as i16,
        ),
        (0, 0, 0, 0),
        "ShowWindow must not expand an already visible window's updateRgn"
    );
}

#[test]
fn showwindow_hidden_window_sets_global_update_region() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        disp.screen_mode.0,
        100,
        200,
        300,
        500,
        "",
        2,
        false,
        false,
        false,
        0,
    );

    assert!(disp.window_update_rect(&bus, window).is_none());

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window);
    let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        disp.window_update_rect(&bus, window),
        Some((100, 200, 300, 500)),
        "ShowWindow should invalidate the revealed content in global coordinates"
    );
}

#[test]
fn showwindow_erases_newly_exposed_content_with_window_background() {
    // PaintOne erases exposed content before the application receives its
    // update event. Inside Macintosh Volume I (1985), pp. I-278, I-296.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        screen_base,
        100,
        200,
        300,
        500,
        "Hidden",
        4,
        false,
        false,
        true,
        0,
    );
    let probe = screen_base + 150 * 800 + 323;
    bus.write_byte(probe, 0x42);

    let previous_port = *disp.current_port;
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window);
    dispatch(&mut disp, 0x115, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_ne!(
        bus.read_byte(probe),
        0x42,
        "ShowWindow must erase stale pixels across the whole revealed content area"
    );
    assert_eq!(
        *disp.current_port, previous_port,
        "Window Manager painting must restore the application's current port"
    );
}

#[test]
fn showwindow_hidden_frontmost_window_queues_activate_event() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        disp.screen_mode.0,
        100,
        200,
        300,
        500,
        "",
        2,
        false,
        false,
        false,
        0,
    );
    disp.event_queue.clear();
    bus.write_byte(
        window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0x00,
    );
    assert_eq!(disp.front_window, window);
    assert_eq!(
        bus.read_byte(window + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET),
        0x00
    );

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window);
    let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        bus.read_byte(window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0xFF,
        "ShowWindow should hilite an invisible frontmost window"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 8 && event.message == window && (event.modifiers & 1) == 1),
        "ShowWindow must queue an activate event for an invisible frontmost window"
    );
}

#[test]
fn showwindow_hidden_first_window_becomes_frontwindow_even_if_cached_front_was_behind() {
    let (mut disp, mut cpu, mut bus) = setup();
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    let document = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        document,
        disp.screen_mode.0,
        120,
        120,
        320,
        520,
        "Document",
        8,
        true,
        false,
        true,
        0,
    );

    let dialog = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        dialog,
        disp.screen_mode.0,
        90,
        180,
        260,
        460,
        "Dialog",
        3,
        false,
        false,
        true,
        0,
    );
    disp.front_window = document;
    bus.write_byte(
        document + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );
    bus.write_byte(
        dialog + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0x00,
    );
    disp.event_queue.clear();

    assert_eq!(
        disp.window_list.first(),
        Some(dialog),
        "the hidden dialog must be first in Window Manager order"
    );
    assert_eq!(
        disp.front_window_for_trap(&bus),
        document,
        "FrontWindow skips the hidden first window before ShowWindow"
    );

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, dialog);
    let result = dispatch(&mut disp, 0x115, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    cpu.write_reg(Register::A7, TEST_SP);
    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        bus.read_long(TEST_SP),
        dialog,
        "FrontWindow must return the first visible window in the window list"
    );
    assert_eq!(
        disp.front_window, dialog,
        "ShowWindow should activate a newly visible window that is frontmost in list order"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 8 && event.message == dialog && (event.modifiers & 1) == 1),
        "ShowWindow must queue an activate event for the newly visible frontmost window"
    );
}

// ---------------------------------------------------------------
// 10. HideWindow (0x116) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_hide_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xDEAD0000);

    let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// HideWindow promotes the next visible window to front when the
// hidden window was the current front window, per IM:I I-286.
#[test]
fn hide_window_promotes_next_visible_to_front() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]); // b is front, a is behind
    disp.front_window = win_b;
    disp.current_port
        .with_mut(|current_port| *current_port = win_b);
    for &base in &[win_a, win_b] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
        bus.write_byte(base + 110u32, 0xFF);
    }

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_b); // hide the front

    let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(disp.front_window, win_a, "next visible must become front");
    assert_eq!(
        *disp.current_port, win_a,
        "current_port must follow new front when it was the hidden window"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 8 && event.message == win_b && (event.modifiers & 1) == 0),
        "HideWindow must queue a deactivate event for the hidden front window"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 8 && event.message == win_a && (event.modifiers & 1) == 1),
        "HideWindow must queue an activate event for the promoted front window"
    );
    assert_eq!(
        bus.read_byte(win_b + 110u32),
        0x00,
        "hidden window must be marked invisible"
    );
}

#[test]
fn hide_window_invalidates_visible_content_behind_it() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let back = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        screen_base,
        40,
        40,
        300,
        500,
        "Back",
        0,
        true,
        true,
        true,
        0,
    );
    let front = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        front,
        screen_base,
        80,
        100,
        220,
        420,
        "Front",
        1,
        true,
        true,
        false,
        0,
    );
    disp.window_list.replace(vec![front, back]);
    disp.sync_window_list_links(&mut bus);
    disp.front_window = front;
    disp.current_port
        .with_mut(|current_port| *current_port = front);
    let update_handle = bus.read_long(back + 122);
    super::super::TrapDispatcher::write_region_handle_rect(&mut bus, update_handle, None);
    disp.clear_queued_update_events(back);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, front);
    let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert!(disp.window_update_rect(&bus, back).is_some());
    assert!(disp
        .event_queue
        .iter()
        .any(|event| { event.what == 6 && event.message == back }));
}

#[test]
fn hide_window_skips_invisible_candidates() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    let win_c = 0x200240u32;
    disp.window_list.replace(vec![win_c, win_b, win_a]);
    disp.front_window = win_c;
    disp.current_port
        .with_mut(|current_port| *current_port = win_c);
    for &base in &[win_a, win_b, win_c] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }
    // Only c and a are visible; b is already hidden.
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0x00);
    bus.write_byte(win_c + 110u32, 0xFF);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_c); // hide c
    let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        disp.front_window, win_a,
        "must skip already-hidden b and pick a"
    );
}

#[test]
fn hide_window_already_hidden_window_does_not_erase_screen_pixels() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let hidden_window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        hidden_window,
        screen_base,
        42,
        2,
        334,
        502,
        "Hidden",
        8,
        false,
        false,
        false,
        0,
    );
    bus.write_byte(hidden_window + 110u32, 0x00);
    disp.front_window = 0;

    let probe = screen_base + 100 * 800 + 100;
    bus.write_byte(probe, 0x42);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, hidden_window);
    let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        bus.read_byte(probe),
        0x42,
        "HideWindow on an already hidden window must not erase exposed pixels"
    );
}

#[test]
fn hide_window_restores_saved_under_pixels_for_non_document_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    for y in 40..120u32 {
        for x in 40..180u32 {
            bus.write_byte(screen_base + y * 800 + x, 0xCC);
        }
    }

    let bounds = bus.alloc(8);
    bus.write_word(bounds, 50);
    bus.write_word(bounds + 2, 60);
    bus.write_word(bounds + 4, 100);
    bus.write_word(bounds + 6, 160);

    let sp = TEST_SP - 26;
    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds);
    bus.write_byte(sp + 12, 0xFF); // visible = TRUE
    bus.write_word(sp + 10, 1); // non-document window proc
    bus.write_long(sp + 6, 0xFFFF_FFFF); // front

    let created = dispatch(&mut disp, 0x245, &mut cpu, &mut bus);
    assert!(created.unwrap().is_ok());
    let window = bus.read_long(TEST_SP);
    assert_ne!(window, 0, "NewCWindow should return a window");

    let probe = screen_base + 70 * 800 + 80;
    bus.write_byte(probe, 0x11);

    let hide_sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, hide_sp);
    bus.write_long(hide_sp, window);
    let hidden = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
    assert!(hidden.unwrap().is_ok());

    assert_eq!(
        bus.read_byte(probe),
        0xCC,
        "HideWindow should restore the pixels saved under non-document windows"
    );
}

#[test]
fn hide_window_clears_visible_dialog_snapshot_before_chrome_redraw() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let dialog = bus.alloc(256);
    bus.write_word(dialog + 8, (-100i16) as u16);
    bus.write_word(dialog + 10, (-100i16) as u16);
    bus.write_word(dialog + 20, 180);
    bus.write_word(dialog + 22, 260);
    bus.write_byte(
        dialog + super::super::TrapDispatcher::WINDOW_VISIBLE_OFFSET,
        0xFF,
    );
    disp.window_list.replace(vec![dialog]);
    disp.front_window = dialog;
    disp.current_port
        .with_mut(|current_port| *current_port = dialog);
    disp.dialog_items
        .insert(dialog, vec![DialogItem::default()]);
    disp.dialog_visible_snapshots.insert(
        dialog,
        PersistentDialogSnapshot {
            bounds: (120, 120, 140, 180),
            pixels: vec![0xEE; 20 * 60].into(),
        },
    );

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, dialog);
    let hidden = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
    assert!(hidden.unwrap().is_ok());
    assert!(
        !disp.dialog_visible_snapshots.contains_key(&dialog),
        "HideWindow must stop compositing retained visible dialog pixels"
    );

    let probe = screen_base + 125 * 800 + 125;
    bus.write_byte(probe, 0x11);
    disp.redraw_chrome(&mut bus);
    assert_eq!(
        bus.read_byte(probe),
        0x11,
        "redraw_chrome must not repaint a hidden dialog snapshot"
    );
}

// DisposeWindow / CloseWindow route through untrack_window. The
// promotion logic must skip hidden windows, mirroring HideWindow's
// visible-only walk.
#[test]
fn dispose_window_skips_hidden_candidate_when_promoting_front() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    let win_c = 0x200240u32;
    // List: c front, b behind (hidden), a back-most (visible).
    disp.window_list.replace(vec![win_c, win_b, win_a]);
    disp.front_window = win_c;
    disp.current_port
        .with_mut(|current_port| *current_port = win_c);
    for &base in &[win_a, win_b, win_c] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0x00); // b hidden
    bus.write_byte(win_c + 110u32, 0xFF);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_c);

    // DisposeWindow ($A914, trap 0x114)
    let result = dispatch(&mut disp, 0x114, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        disp.front_window, win_a,
        "must skip hidden b and promote visible a to front"
    );
    assert_eq!(*disp.current_port, win_a);
}

#[test]
fn close_window_falls_back_to_first_entry_when_all_remaining_are_hidden() {
    // Defensive case: if every remaining window is hidden, fall
    // back to window_list.first() rather than 0 so the guest
    // doesn't see a bogus nil front_window mid-cleanup.
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]);
    disp.front_window = win_b;
    disp.current_port
        .with_mut(|current_port| *current_port = win_b);
    for &base in &[win_a, win_b] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }
    bus.write_byte(win_a + 110u32, 0x00); // a hidden
    bus.write_byte(win_b + 110u32, 0xFF);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_b);

    // CloseWindow ($A92D, trap 0x12D)
    let result = dispatch(&mut disp, 0x12D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        disp.front_window, win_a,
        "fallback must still promote to the only remaining entry (a)"
    );
}

#[test]
fn hide_window_non_front_leaves_front_untouched() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]);
    disp.front_window = win_b;
    disp.current_port
        .with_mut(|current_port| *current_port = win_b);
    for &base in &[win_a, win_b] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
        bus.write_byte(base + 110u32, 0xFF);
    }

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_a); // hide the NON-front window
    let result = dispatch(&mut disp, 0x116, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(disp.front_window, win_b, "front must not change");
    assert_eq!(*disp.current_port, win_b, "current_port must not change");
}

// IM:I I-285: ShowHide(TRUE) makes the target window visible but does
// not reorder windows or generate activate events.
#[test]
fn showhide_true_makes_target_visible_without_front_reorder_or_activate_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    let front_window = 0x200040u32;
    let target_window = 0x200140u32;
    let (_cont_rgn, update_rgn) =
        setup_full_window_with_regions(&mut bus, target_window, 12, 34, 80, 140);
    bus.write_byte(front_window + 110, 0xFF);
    bus.write_byte(front_window + 111, 0xFF);
    bus.write_byte(target_window + 110, 0x00);
    bus.write_byte(target_window + 111, 0x00);

    disp.window_list.replace(vec![front_window, target_window]);
    disp.front_window = front_window;
    disp.current_port
        .with_mut(|current_port| *current_port = front_window);

    let activate_before = disp.event_queue.iter().filter(|e| e.what == 8).count();

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1); // showFlag = TRUE in Pascal BOOLEAN high byte.
    bus.write_long(sp + 2, target_window);

    let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(
        bus.read_byte(target_window + 110),
        0xFF,
        "ShowHide(TRUE) must set visible byte"
    );
    assert_eq!(
        disp.front_window, front_window,
        "ShowHide must not change front-to-back ordering"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|e| e.what == 6 && e.message == target_window),
        "ShowHide(TRUE) should queue an update event for the shown window"
    );
    let activate_after = disp.event_queue.iter().filter(|e| e.what == 8).count();
    assert_eq!(
        activate_after, activate_before,
        "ShowHide must not generate activate/deactivate events"
    );
    assert_eq!(bus.read_word(update_rgn + 2) as i16, 12, "updateRgn.top");
    assert_eq!(bus.read_word(update_rgn + 4) as i16, 34, "updateRgn.left");
    assert_eq!(bus.read_word(update_rgn + 6) as i16, 80, "updateRgn.bottom");
    assert_eq!(bus.read_word(update_rgn + 8) as i16, 140, "updateRgn.right");
}

#[test]
fn showhide_true_sets_global_update_region_for_revealed_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        disp.screen_mode.0,
        100,
        200,
        300,
        500,
        "",
        2,
        false,
        false,
        false,
        0,
    );

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1);
    bus.write_long(sp + 2, window);

    let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        disp.window_update_rect(&bus, window),
        Some((100, 200, 300, 500)),
        "ShowHide(TRUE) should invalidate revealed content in global coordinates"
    );
}

#[test]
fn showhide_true_erases_content_and_recovers_stale_front_window_cache() {
    // ShowHide does not reorder or generate activation events, but showing
    // a window still runs the Window Manager's PaintOne erase and makes a
    // visible window available to its internal front-window state.
    // Inside Macintosh Volume I (1985), pp. I-278, I-285, I-296.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        screen_base,
        100,
        200,
        300,
        500,
        "Hidden",
        4,
        false,
        false,
        true,
        0,
    );
    disp.front_window = 0;
    disp.event_queue.clear();
    let probe = screen_base + 150 * 800 + 323;
    bus.write_byte(probe, 0x42);

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1);
    bus.write_long(sp + 2, window);
    dispatch(&mut disp, 0x108, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_ne!(
        bus.read_byte(probe),
        0x42,
        "ShowHide(TRUE) must erase stale pixels before the application's update"
    );
    assert_eq!(disp.front_window, window);
    assert!(
        disp.event_queue.iter().all(|event| event.what != 8),
        "ShowHide must not synthesize activate or deactivate events"
    );
}

// IM:I I-285: ShowHide(FALSE) makes the target window invisible but does
// not reorder windows or generate activate events.
#[test]
fn showhide_false_makes_target_invisible_without_front_reorder_or_activate_events() {
    let (mut disp, mut cpu, mut bus) = setup();
    let front_window = 0x200040u32;
    let target_window = 0x200140u32;
    setup_full_window_with_regions(&mut bus, target_window, 20, 30, 90, 180);
    bus.write_byte(front_window + 110, 0xFF);
    bus.write_byte(front_window + 111, 0xFF);
    bus.write_byte(target_window + 110, 0xFF);
    bus.write_byte(target_window + 111, 0x00);

    disp.window_list.replace(vec![front_window, target_window]);
    disp.front_window = front_window;
    disp.current_port
        .with_mut(|current_port| *current_port = front_window);
    disp.queue_window_update_event(target_window);
    assert!(
        disp.event_queue
            .iter()
            .any(|e| e.what == 6 && e.message == target_window),
        "test precondition: target has queued update event"
    );
    let activate_before = disp.event_queue.iter().filter(|e| e.what == 8).count();

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 0); // showFlag = FALSE.
    bus.write_long(sp + 2, target_window);

    let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(
        bus.read_byte(target_window + 110),
        0x00,
        "ShowHide(FALSE) must clear visible byte"
    );
    assert_eq!(
        disp.front_window, front_window,
        "ShowHide must not change front-to-back ordering"
    );
    assert!(
        !disp
            .event_queue
            .iter()
            .any(|e| e.what == 6 && e.message == target_window),
        "ShowHide(FALSE) should clear queued update events for hidden window"
    );
    let activate_after = disp.event_queue.iter().filter(|e| e.what == 8).count();
    assert_eq!(
        activate_after, activate_before,
        "ShowHide must not generate activate/deactivate events"
    );
}

#[test]
fn showhide_false_invalidates_visible_content_behind_target() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let back = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        screen_base,
        40,
        40,
        300,
        500,
        "Back",
        0,
        true,
        true,
        true,
        0,
    );
    let target = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        target,
        screen_base,
        80,
        100,
        220,
        420,
        "Target",
        1,
        true,
        true,
        false,
        0,
    );
    disp.window_list.replace(vec![target, back]);
    disp.sync_window_list_links(&mut bus);
    disp.front_window = back;
    disp.current_port
        .with_mut(|current_port| *current_port = back);
    disp.dialog_visible_snapshots.insert(
        target,
        PersistentDialogSnapshot {
            bounds: (80, 100, 220, 420),
            pixels: vec![0xEE; 140 * 320].into(),
        },
    );
    let update_handle = bus.read_long(back + 122);
    super::super::TrapDispatcher::write_region_handle_rect(&mut bus, update_handle, None);
    disp.clear_queued_update_events(back);

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 0);
    bus.write_long(sp + 2, target);
    let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert!(disp.window_update_rect(&bus, back).is_some());
    assert!(disp
        .event_queue
        .iter()
        .any(|event| { event.what == 6 && event.message == back }));
    assert!(!disp.dialog_visible_snapshots.contains_key(&target));
}

// ---------------------------------------------------------------
// 11. SetWTitle (0x11A) -- pops 8 bytes
// ---------------------------------------------------------------
#[test]
fn test_set_wtitle() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    // SP+0: title_ptr(4), SP+4: window(4)
    bus.write_long(sp, 0x300000);
    bus.write_long(sp + 4, 0xDEAD0000);

    let result = dispatch(&mut disp, 0x11A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn setwtitle_decodes_mac_roman_for_window_chrome() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = bus.alloc(256);
    disp.front_window = window;
    let title = bus.alloc(8);
    bus.write_pstring(title, b"DLB\xAA");
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, title);
    bus.write_long(sp + 4, window);

    dispatch(&mut disp, 0x11A, &mut cpu, &mut bus)
        .expect("SetWTitle dispatch")
        .expect("SetWTitle");

    assert_eq!(disp.window_title, "DLB™");
    let handle = bus.read_long(window + super::super::TrapDispatcher::WINDOW_TITLE_HANDLE_OFFSET);
    assert_eq!(bus.read_pstring(bus.read_long(handle)), b"DLB\xAA");
}

#[test]
fn setwtitle_redraws_front_window_title_bar() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        60,
        30,
        220,
        370,
        "Old",
        0,
        true,
        true,
        false,
        0,
    );

    let (screen_base, row_bytes, _, _, _) = disp.screen_mode;
    let title_region = |bus: &crate::memory::MacMemoryBus| -> Vec<u8> {
        let mut bytes = Vec::new();
        for y in 41..59u32 {
            for x in 60..340u32 {
                bytes.push(bus.read_byte(screen_base + y * row_bytes + x));
            }
        }
        bytes
    };
    let before = title_region(&bus);

    let new_title = bus.alloc(32);
    bus.write_pstring(new_title, b"Player 1 | Opponent 0");
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, new_title);
    bus.write_long(sp + 4, window_addr);

    let result = dispatch(&mut disp, 0x11A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_ne!(
        title_region(&bus),
        before,
        "SetWTitle should repaint the visible front-window title bar"
    );
}

#[test]
fn redraw_chrome_reads_front_window_title_handle() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.menu_bar_hidden = false;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        60,
        30,
        220,
        370,
        "Old",
        0,
        true,
        true,
        false,
        0,
    );

    let (screen_base, row_bytes, _, _, _) = disp.screen_mode;
    let title_region = |bus: &crate::memory::MacMemoryBus| -> Vec<u8> {
        let mut bytes = Vec::new();
        for y in 41..59u32 {
            for x in 60..340u32 {
                bytes.push(bus.read_byte(screen_base + y * row_bytes + x));
            }
        }
        bytes
    };
    let before = title_region(&bus);

    let title_handle =
        bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_TITLE_HANDLE_OFFSET);
    let new_title = bus.alloc(32);
    bus.write_pstring(new_title, b"Player 1 | Opponent 0");
    bus.write_long(title_handle, new_title);

    disp.redraw_chrome(&mut bus);

    assert_ne!(
        title_region(&bus),
        before,
        "front-window chrome redraw should use the live WindowRecord titleHandle"
    );
}

#[test]
fn direct_chrome_redraw_does_not_paint_back_window_through_front_content() {
    // The Window Manager clips a back window's frame to windows above it.
    // Redrawing raw framebuffer chrome without that clip can leave stale
    // back-window borders inside the front window's content area.
    // Inside Macintosh Volume I (1985), pp. I-296..I-297.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.menu_bar_hidden = false;

    let back = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        screen_base,
        50,
        50,
        150,
        300,
        "Back",
        0,
        true,
        false,
        false,
        0,
    );
    let front = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        front,
        screen_base,
        50,
        30,
        570,
        370,
        "Front",
        4,
        true,
        false,
        false,
        0,
    );

    let protected = screen_base + 60 * 800 + 49;
    bus.write_byte(protected, 0x7B);

    let front_rect = disp.window_structure_rect(&mut bus, front).unwrap();
    let before = disp.save_screen_rect_pixels(&mut bus, front_rect).unwrap();
    for _ in 0..6 {
        disp.draw_single_window_chrome_inline(&mut bus, back, false);
        disp.draw_grow_icon(&mut bus, back);
    }
    let after = disp.save_screen_rect_pixels(&mut bus, front_rect).unwrap();
    assert_eq!(
        before, after,
        "repeated frame draws must preserve the entire front window"
    );

    assert_eq!(
        bus.read_byte(protected),
        0x7B,
        "back-window chrome must be clipped by the front window's content"
    );
}

#[test]
fn redraw_chrome_uses_visual_window_list_above_active_document() {
    // BringToFront can place a window visually above the active window
    // without activating it. WindowList, not the active-window cache,
    // owns that visual order. Macintosh Toolbox Essentials (1992),
    // pp. 4-65, 4-69, and 4-90.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);
    disp.menu_bar_hidden = false;

    let document = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        document,
        screen_base,
        80,
        50,
        220,
        300,
        "Document",
        0,
        true,
        false,
        false,
        0,
    );
    let utility = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        utility,
        screen_base,
        50,
        40,
        110,
        200,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    disp.front_window = document;
    disp.window_bounds = (80, 50, 220, 300);
    disp.window_proc_id = 0;
    disp.window_title = "Document".to_string();
    let protected = screen_base + 70 * 800 + 60;
    for value in [0x7B, 0x56] {
        bus.write_byte(protected, value);
        // The second paint reuses the document title. Its saved pixels
        // must still stay behind freshly updated utility-window content.
        disp.redraw_chrome(&mut bus);
        assert_eq!(
            bus.read_byte(protected),
            value,
            "active-document chrome must remain behind visual-front utility content"
        );
    }
}

#[test]
fn inactive_document_window_chrome_still_draws_title_text() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.menu_bar_hidden = false;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        60,
        30,
        220,
        370,
        "Inactive Title",
        0,
        true,
        true,
        false,
        0,
    );
    bus.write_byte(
        window_addr + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0,
    );
    disp.draw_single_window_chrome_inline(&mut bus, window_addr, false);

    let (screen_base, row_bytes, _, _, _) = disp.screen_mode;
    let has_title_pixels = (44..57u32)
        .any(|y| (145..255u32).any(|x| bus.read_byte(screen_base + y * row_bytes + x) != 0));
    assert!(
        has_title_pixels,
        "inactive document windows should retain visible title text"
    );
}

// ---------------------------------------------------------------
// 12. GetWTitle (0x119) -- pops 8 bytes, writes empty string
// ---------------------------------------------------------------
#[test]
fn test_get_wtitle() {
    let (mut disp, mut cpu, mut bus) = setup();

    let title_storage: u32 = 0x300000;
    // Write a non-zero byte so we can verify it gets overwritten
    bus.write_byte(title_storage, 0xFF);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    // SP+0: title_ptr(4), SP+4: window(4)
    bus.write_long(sp, title_storage);
    bus.write_long(sp + 4, 0xDEAD0000);

    let result = dispatch(&mut disp, 0x119, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    // Verify empty Pascal string written at title_storage
    assert_eq!(
        bus.read_byte(title_storage),
        0,
        "GetWTitle should write empty string (length 0)"
    );
}

// ---------------------------------------------------------------
// 13. FrontWindow (0x124) -- writes front_window at SP, SP unchanged
// ---------------------------------------------------------------
#[test]
fn test_front_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    // Real Mac semantics per IM:I I-274: FrontWindow returns the
    // frontmost VISIBLE window. Seed a window_list entry with its
    // visible byte set so the visible-only walk finds it.
    let win = 0x200040u32;
    disp.window_list.replace(vec![win]);
    disp.front_window = win;
    bus.write_byte(win + 110u32, 0xFF);

    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    let returned = bus.read_long(TEST_SP);
    assert_eq!(
        returned, win,
        "FrontWindow should return the current front visible window"
    );
}

// FrontWindow must return the frontmost VISIBLE window per IM:I
// I-274, not just self.front_window — BringToFront on a hidden
// window can leave front_window pointing at it.
#[test]
fn front_window_skips_hidden_and_returns_first_visible() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]); // b first, a behind
    disp.front_window = win_b;
    // b hidden, a visible.
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0x00);

    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let returned = bus.read_long(TEST_SP);
    assert_eq!(returned, win_a, "must skip hidden b and return a");
}

#[test]
fn front_window_skips_lowmem_ghost_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    let ghost = 0x200040u32;
    let doc = 0x200140u32;
    disp.window_list.replace(vec![ghost, doc]);
    disp.front_window = ghost;
    bus.write_byte(ghost + 110u32, 0xFF);
    bus.write_byte(doc + 110u32, 0xFF);
    bus.write_long(crate::memory::globals::addr::GHOST_WINDOW, ghost);

    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let returned = bus.read_long(TEST_SP);
    assert_eq!(
        returned, doc,
        "FrontWindow must ignore low-memory GhostWindow"
    );
}

#[test]
fn front_window_returns_nil_when_all_hidden() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]);
    disp.front_window = win_b;
    // Both hidden.
    bus.write_byte(win_a + 110u32, 0x00);
    bus.write_byte(win_b + 110u32, 0x00);

    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let returned = bus.read_long(TEST_SP);
    assert_eq!(
        returned, 0,
        "FrontWindow must return NIL when all windows are invisible"
    );
}

#[test]
fn front_window_returns_active_document_behind_custom_utility_layer() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wdef_resource(&mut disp, &mut bus, 200);

    let utility = bus.alloc(256);
    let document = bus.alloc(256);
    let utility_proc_id = (200i16 << 4) | 3;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        utility,
        disp.screen_mode.0,
        34,
        2,
        114,
        82,
        "",
        utility_proc_id,
        true,
        false,
        false,
        0,
    );
    bus.write_byte(
        utility + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        document,
        disp.screen_mode.0,
        40,
        86,
        473,
        622,
        "Document",
        8,
        false,
        false,
        true,
        0,
    );
    disp.apply_behind_parameter(&mut bus, document, utility);

    assert_eq!(
        disp.window_list,
        vec![utility, document],
        "custom utility window must remain visually in front"
    );
    assert_eq!(
        disp.front_window, document,
        "standard document behind a custom utility layer remains active"
    );
    assert_eq!(
        bus.read_byte(utility + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0x00,
        "floating utility should not keep active-window hilite"
    );
    assert_eq!(
        bus.read_byte(document + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0xFF,
        "document should be the active/hilited window"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        bus.read_long(TEST_SP),
        utility,
        "FrontWindow should skip the active document while it is still hidden"
    );

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1);
    bus.write_long(sp + 2, document);

    let result = dispatch(&mut disp, 0x108, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        disp.front_window, document,
        "ShowHide(TRUE) should not disturb the pending active document"
    );

    cpu.write_reg(Register::A7, TEST_SP);
    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        bus.read_long(TEST_SP),
        utility,
        "FrontWindow should return the first visible window in Window Manager order"
    );

    bus.write_long(crate::memory::globals::addr::GHOST_WINDOW, utility);
    cpu.write_reg(Register::A7, TEST_SP);
    let result = dispatch(&mut disp, 0x124, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        bus.read_long(TEST_SP),
        document,
        "GhostWindow should exclude the floating utility layer from FrontWindow"
    );

    let wnd_ptr_ptr = bus.alloc(4);
    let find_sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, find_sp);
    bus.write_long(find_sp, wnd_ptr_ptr);
    bus.write_word(find_sp + 4, 40);
    bus.write_word(find_sp + 6, 10);
    bus.write_word(find_sp + 8, 0);

    let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        bus.read_word(TEST_SP - 2),
        3,
        "FindWindow should still report content in the visual utility layer"
    );
    assert_eq!(
        bus.read_long(wnd_ptr_ptr),
        utility,
        "FindWindow should hit-test against visual layer order"
    );
}

#[test]
fn select_active_window_restores_it_to_visual_front() {
    let (mut disp, mut cpu, mut bus) = setup();
    install_wdef_resource(&mut disp, &mut bus, 200);
    let utility_proc_id = (200i16 << 4) | 3;

    let document = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        document,
        disp.screen_mode.0,
        80,
        80,
        360,
        500,
        "Document",
        8,
        true,
        false,
        true,
        0,
    );
    let palette = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        palette,
        disp.screen_mode.0,
        100,
        100,
        180,
        240,
        "Palette",
        2,
        true,
        false,
        false,
        0,
    );
    let utility = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        utility,
        disp.screen_mode.0,
        32,
        12,
        64,
        504,
        "",
        utility_proc_id,
        true,
        false,
        false,
        0,
    );
    disp.front_window = document;
    disp.sync_cached_front_window_render_state(&bus);
    disp.event_queue.clear();
    let update_handle =
        bus.read_long(document + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET);
    super::super::TrapDispatcher::write_region_handle_rect(&mut bus, update_handle, None);
    assert_eq!(disp.window_list, vec![utility, palette, document]);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, document);
    let result = dispatch(&mut disp, 0x11F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        disp.window_list,
        vec![document, utility, palette],
        "SelectWindow must restore the active window to the head of WindowList"
    );
    assert_eq!(disp.front_window, document);
    assert!(disp.window_has_pending_update(&bus, document));
    assert!(
        disp.event_queue.iter().all(|event| event.what != 8),
        "reordering an already-active document must not synthesize activation changes"
    );
}

// ---------------------------------------------------------------
// 14a. FindWindow (0x12C) -- menu bar click
// ---------------------------------------------------------------
#[test]
fn test_find_window_menu_bar() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    // Keep a non-NIL front window so this test can verify MTE 1992
    // p. 4-91 semantics: clicks not in a window must set theWindow=NIL.
    disp.front_window = 0xDEAD0000;

    let wnd_ptr_ptr: u32 = 0x300000;

    // SP+0: wnd_ptr_ptr(4), SP+4: pt(4), SP+8: result(2)
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, wnd_ptr_ptr); // VAR whichWindow
    bus.write_word(sp + 4, 10); // pt.v = 10 (in menu bar, < 20)
    bus.write_word(sp + 6, 100); // pt.h = 100
    bus.write_word(sp + 8, 0); // result placeholder

    let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    // SP should advance to old_sp + 8 (pops wnd_ptr_ptr + pt, leaves result)
    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP - 2);

    // part = 1 (inMenuBar) written at sp + 8 = TEST_SP - 10 + 8 = TEST_SP - 2
    let part = bus.read_word(new_sp);
    assert_eq!(
        part, 1,
        "FindWindow with pt.v=10 should return inMenuBar (1)"
    );

    let which_window = bus.read_long(wnd_ptr_ptr);
    assert_eq!(
        which_window, 0,
        "FindWindow inMenuBar must write NIL to whichWindow (MTE 1992 p. 4-91)"
    );
}

// ---------------------------------------------------------------
// 14b. FindWindow (0x12C) -- content click
// ---------------------------------------------------------------
#[test]
fn test_find_window_content() {
    let (mut disp, mut cpu, mut bus) = setup();

    let window_addr: u32 = 0x310000;
    setup_full_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);
    bus.write_byte(window_addr + 110, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let wnd_ptr_ptr: u32 = 0x300000;

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, wnd_ptr_ptr);
    bus.write_word(sp + 4, 100); // pt.v = 100 (not in menu bar)
    bus.write_word(sp + 6, 200); // pt.h = 200
    bus.write_word(sp + 8, 0);

    let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP - 2);

    let part = bus.read_word(new_sp);
    assert_eq!(
        part, 3,
        "FindWindow with a visible window under the point should return inContent (3)"
    );

    // Verify whichWindow was written
    let which_window = bus.read_long(wnd_ptr_ptr);
    assert_eq!(which_window, window_addr);
}

#[test]
fn findwindow_reports_ingoaway_for_active_document_close_box() {
    // MTE 1992 pp. 4-43 to 4-46: the active document window's close
    // region is reported as inGoAway (6), not inDrag (4).
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        disp.screen_mode.0,
        100,
        100,
        260,
        360,
        "Document",
        0,
        true,
        false,
        true,
        0,
    );
    disp.front_window = window;
    disp.window_list.replace(vec![window]);
    bus.write_byte(
        window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );

    let which_window = bus.alloc(4);
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, which_window);
    bus.write_word(sp + 4, 85); // inside [contentTop-18, contentTop)
    bus.write_word(sp + 6, 105); // inside [contentLeft, contentLeft+18)
    bus.write_word(sp + 8, 0);

    dispatch(&mut disp, 0x12C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    assert_eq!(bus.read_word(sp + 8), 6);
    assert_eq!(bus.read_long(which_window), window);
}

#[test]
fn findwindow_reports_ingoaway_for_active_document_behind_floating_utility() {
    // Floating utility windows stay visually above the active document;
    // they do not disable that document's close box. Macintosh Human
    // Interface Guidelines (1992), pp. 137, 144.
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    install_wdef_resource(&mut disp, &mut bus, 200);

    let utility = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        utility,
        disp.screen_mode.0,
        300,
        300,
        380,
        500,
        "",
        200i16 << 4,
        true,
        false,
        false,
        0,
    );
    let document = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        document,
        disp.screen_mode.0,
        100,
        100,
        260,
        360,
        "Document",
        4,
        true,
        false,
        true,
        0,
    );
    disp.apply_behind_parameter(&mut bus, document, utility);

    assert_eq!(disp.window_list, vec![utility, document]);
    assert_eq!(disp.front_window, document);
    assert_eq!(
        bus.read_byte(document + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0xFF
    );

    let which_window = bus.alloc(4);
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, which_window);
    bus.write_word(sp + 4, 85);
    bus.write_word(sp + 6, 105);
    bus.write_word(sp + 8, 0);

    dispatch(&mut disp, 0x12C, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(sp + 8), 6);
    assert_eq!(bus.read_long(which_window), document);
}

#[test]
fn find_window_walks_front_to_back_and_uses_port_bounds_for_global_hits() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let main_window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        main_window,
        screen_base,
        0,
        0,
        600,
        800,
        "",
        0,
        true,
        false,
        false,
        0,
    );

    let dialog_window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        dialog_window,
        screen_base,
        110,
        155,
        380,
        645,
        "",
        1,
        true,
        false,
        false,
        0,
    );

    assert_eq!(disp.window_list, vec![dialog_window, main_window]);
    // Regression shape: cached front-window fields can be restored by the
    // application while the Window Manager list still has a visible dialog
    // in front. FindWindow must use the list, not these stale caches.
    disp.front_window = main_window;
    disp.window_bounds = (0, 0, 600, 800);
    disp.window_proc_id = 0;

    let wnd_ptr_ptr: u32 = bus.alloc(4);
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, wnd_ptr_ptr);
    bus.write_word(sp + 4, 362); // global v inside dialog content
    bus.write_word(sp + 6, 491); // global h inside dialog content
    bus.write_word(sp + 8, 0);

    let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP - 2);
    assert_eq!(bus.read_word(new_sp), 3);
    assert_eq!(
        bus.read_long(wnd_ptr_ptr),
        dialog_window,
        "FindWindow must return the frontmost visible window under the global point"
    );
}

// FindWindow must return inDesk + whichWindow=NIL when the click is not
// in the menu bar and not in any application window (MTE 1992 p. 4-91).
#[test]
fn test_find_window_in_desk_returns_nil_window() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.menu_bar_hidden = false;
    let window_addr: u32 = 0x310000;
    setup_full_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);
    bus.write_byte(window_addr + 110, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let wnd_ptr_ptr: u32 = 0x300000;
    bus.write_long(wnd_ptr_ptr, 0xFFFFFFFF);

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, wnd_ptr_ptr);
    bus.write_word(sp + 4, 380); // pt.v outside window + below menu bar
    bus.write_word(sp + 6, 520); // pt.h outside window
    bus.write_word(sp + 8, 0);

    let result = dispatch(&mut disp, 0x12C, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let new_sp = cpu.read_reg(Register::A7);
    assert_eq!(new_sp, TEST_SP - 2);

    let part = bus.read_word(new_sp);
    assert_eq!(
        part, 0,
        "FindWindow outside window/menu bar should return inDesk (0)"
    );
    let which_window = bus.read_long(wnd_ptr_ptr);
    assert_eq!(
        which_window, 0,
        "FindWindow inDesk must write NIL to whichWindow (MTE 1992 p. 4-91)"
    );
}

// ---------------------------------------------------------------
// 15. BeginUpdate (0x122) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_begin_update() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xDEAD0000);

    let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// BeginUpdate must clip visRgn to visRgn∩updateRgn and clear updateRgn.
// MTE 1992 p. 4-106.
#[test]
fn beginupdate_intersects_visrgn_and_clears_updatergn() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (vis_rgn_data, _clip_rgn_data) =
        setup_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);
    let (_cont_rgn_data, update_rgn_data) =
        setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);

    // updateRgn = (40, 50, 140, 120), visRgn = (0, 0, 110, 210)
    bus.write_word(update_rgn_data + 2, 40);
    bus.write_word(update_rgn_data + 4, 50);
    bus.write_word(update_rgn_data + 6, 140);
    bus.write_word(update_rgn_data + 8, 120);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);

    let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    // Intersection should be (40,50,110,120).
    assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 40, "visRgn.top");
    assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 50, "visRgn.left");
    assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 110, "visRgn.bottom");
    assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 120, "visRgn.right");

    // updateRgn should be empty.
    assert_eq!(bus.read_word(update_rgn_data + 2), 0, "updateRgn.top");
    assert_eq!(bus.read_word(update_rgn_data + 4), 0, "updateRgn.left");
    assert_eq!(bus.read_word(update_rgn_data + 6), 0, "updateRgn.bottom");
    assert_eq!(bus.read_word(update_rgn_data + 8), 0, "updateRgn.right");
}

#[test]
fn beginupdate_without_pending_update_preserves_visrgn() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (vis_rgn_data, _clip_rgn_data) =
        setup_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);
    let (_cont_rgn_data, update_rgn_data) =
        setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);

    assert_eq!(
        (
            bus.read_word(update_rgn_data + 2),
            bus.read_word(update_rgn_data + 6)
        ),
        (0, 0),
        "test fixture should start with an empty updateRgn"
    );

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);

    let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 0);
    assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 0);
    assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 110);
    assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 210);
    assert!(
        !disp.saved_vis_regions.contains_key(&window_addr),
        "no-update BeginUpdate should not create a restore obligation"
    );
}

// Systems Twilight shifts the port origin before servicing the
// window update. BeginUpdate must intersect updateRgn in that shifted
// coordinate space so the stale pixels outside local (0,0) repaint.
#[test]
fn beginupdate_intersects_shifted_port_visrgn_with_shifted_updatergn() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (_cont_rgn_data, update_rgn_data) =
        setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 578, 798);
    let vis_rgn_data = 0x301000;

    bus.write_word(window_addr + 16, (-86i16) as u16);
    bus.write_word(window_addr + 18, (-143i16) as u16);
    bus.write_word(window_addr + 20, 492);
    bus.write_word(window_addr + 22, 655);
    bus.write_word(window_addr + 8, (-86i16) as u16);
    bus.write_word(window_addr + 10, (-143i16) as u16);
    bus.write_word(window_addr + 12, 492);
    bus.write_word(window_addr + 14, 655);
    bus.write_word(vis_rgn_data + 2, (-86i16) as u16);
    bus.write_word(vis_rgn_data + 4, (-143i16) as u16);
    bus.write_word(vis_rgn_data + 6, 492);
    bus.write_word(vis_rgn_data + 8, 655);
    bus.write_word(update_rgn_data + 2, 0);
    bus.write_word(update_rgn_data + 4, 0);
    bus.write_word(update_rgn_data + 6, 578);
    bus.write_word(update_rgn_data + 8, 798);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);

    let result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, -86);
    assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, -143);
    assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 492);
    assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 655);

    assert_eq!(bus.read_word(update_rgn_data + 2), 0, "updateRgn.top");
    assert_eq!(bus.read_word(update_rgn_data + 4), 0, "updateRgn.left");
    assert_eq!(bus.read_word(update_rgn_data + 6), 0, "updateRgn.bottom");
    assert_eq!(bus.read_word(update_rgn_data + 8), 0, "updateRgn.right");
}

// ---------------------------------------------------------------
// 16. EndUpdate (0x123) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_end_update() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xDEAD0000);

    let result = dispatch(&mut disp, 0x123, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// EndUpdate restores the pre-BeginUpdate visRgn (MTE 1992 p. 4-107).
#[test]
fn endupdate_restores_saved_visrgn_after_beginupdate() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (vis_rgn_data, _clip_rgn_data) =
        setup_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);
    let (_cont_rgn_data, update_rgn_data) =
        setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 110, 210);

    // Narrow visRgn during BeginUpdate via updateRgn intersection.
    bus.write_word(update_rgn_data + 2, 30);
    bus.write_word(update_rgn_data + 4, 40);
    bus.write_word(update_rgn_data + 6, 80);
    bus.write_word(update_rgn_data + 8, 100);

    let begin_sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, begin_sp);
    bus.write_long(begin_sp, window_addr);
    let begin_result = dispatch(&mut disp, 0x122, &mut cpu, &mut bus);
    assert!(begin_result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 30);
    assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 40);
    assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 80);
    assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 100);

    let end_sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, end_sp);
    bus.write_long(end_sp, window_addr);
    let end_result = dispatch(&mut disp, 0x123, &mut cpu, &mut bus);
    assert!(end_result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    // Restored original visRgn from before BeginUpdate.
    assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 0);
    assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 0);
    assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 110);
    assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 210);
}

#[test]
fn calcvis_consumes_windowpeek_argument() {
    // CalcVis takes one WindowPeek argument.
    // Inside Macintosh Volume I (1985), p. I-297;
    // Macintosh Toolbox Essentials (1992), p. 4-119.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);

    let result = dispatch(&mut disp, 0x109, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn calcvis_preserves_front_window_region_boxes_and_balances_stack() {
    // CalcVis leaves the frontmost window's region boxes unchanged.
    // Inside Macintosh Volume I (1985), p. I-297;
    // Macintosh Toolbox Essentials (1992), p. 4-119.
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (vis_rgn_data, clip_rgn_data) =
        setup_window_with_regions(&mut bus, window_addr, 10, 20, 110, 210);
    let (cont_rgn_data, _update_rgn_data) =
        setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 110, 210);

    let struc_rgn_data: u32 = 0x308000;
    let struc_rgn_handle: u32 = 0x308100;
    bus.write_word(struc_rgn_data, 10);
    bus.write_word(struc_rgn_data + 2, 10);
    bus.write_word(struc_rgn_data + 4, 20);
    bus.write_word(struc_rgn_data + 6, 110);
    bus.write_word(struc_rgn_data + 8, 210);
    bus.write_long(struc_rgn_handle, struc_rgn_data);
    bus.write_long(window_addr + 114, struc_rgn_handle);

    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 18);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);

    let result = dispatch(&mut disp, 0x109, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    for rgn_data in [cont_rgn_data, struc_rgn_data, vis_rgn_data, clip_rgn_data] {
        assert_eq!(bus.read_word(rgn_data + 2) as i16, 10, "rgn.top");
        assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "rgn.left");
        assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "rgn.bottom");
        assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "rgn.right");
    }
}

#[test]
fn calcvisbehind_consumes_startwindow_and_clobberedrgn_arguments() {
    // CalcVisBehind takes two pointer arguments and returns no result.
    // Inside Macintosh Volume I (1985), p. I-297;
    // Macintosh Toolbox Essentials (1992), p. 4-119.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_long(sp + 4, 0);

    let result = dispatch(&mut disp, 0x10A, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn calcvisbehind_recomputes_regions_for_startwindow_and_windows_behind_it() {
    // CalcVisBehind recalculates the visible-region state for
    // startWindow and the windows behind it that intersect clobberedRgn.
    // Inside Macintosh Volume I (1985), p. I-297;
    // Macintosh Toolbox Essentials (1992), p. 4-119.
    let (mut disp, mut cpu, mut bus) = setup();
    let front: u32 = 0x300000;
    let middle: u32 = 0x301000;
    let back: u32 = 0x302000;

    fn setup_window_regions(
        bus: &mut crate::memory::MacMemoryBus,
        window_addr: u32,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        base: u32,
    ) -> (u32, u32, u32, u32) {
        bus.write_word(window_addr + 16, top as u16);
        bus.write_word(window_addr + 18, left as u16);
        bus.write_word(window_addr + 20, bottom as u16);
        bus.write_word(window_addr + 22, right as u16);

        let vis_data = base;
        let vis_handle = base + 0x100;
        bus.write_word(vis_data, 10);
        bus.write_word(vis_data + 2, top as u16);
        bus.write_word(vis_data + 4, left as u16);
        bus.write_word(vis_data + 6, bottom as u16);
        bus.write_word(vis_data + 8, right as u16);
        bus.write_long(vis_handle, vis_data);
        bus.write_long(window_addr + 24, vis_handle);

        let clip_data = base + 0x200;
        let clip_handle = base + 0x300;
        bus.write_word(clip_data, 10);
        bus.write_word(clip_data + 2, top as u16);
        bus.write_word(clip_data + 4, left as u16);
        bus.write_word(clip_data + 6, bottom as u16);
        bus.write_word(clip_data + 8, right as u16);
        bus.write_long(clip_handle, clip_data);
        bus.write_long(window_addr + 28, clip_handle);

        let struc_data = base + 0x400;
        let struc_handle = base + 0x500;
        bus.write_word(struc_data, 10);
        bus.write_word(struc_data + 2, top as u16);
        bus.write_word(struc_data + 4, left as u16);
        bus.write_word(struc_data + 6, bottom as u16);
        bus.write_word(struc_data + 8, right as u16);
        bus.write_long(struc_handle, struc_data);
        bus.write_long(window_addr + 114, struc_handle);

        let cont_data = base + 0x600;
        let cont_handle = base + 0x700;
        bus.write_word(cont_data, 10);
        bus.write_word(cont_data + 2, top as u16);
        bus.write_word(cont_data + 4, left as u16);
        bus.write_word(cont_data + 6, bottom as u16);
        bus.write_word(cont_data + 8, right as u16);
        bus.write_long(cont_handle, cont_data);
        bus.write_long(window_addr + 118, cont_handle);

        (vis_data, clip_data, struc_data, cont_data)
    }

    let (front_vis, front_clip, front_struc, front_cont) =
        setup_window_regions(&mut bus, front, 10, 20, 110, 210, 0x310000);
    let (middle_vis, middle_clip, middle_struc, middle_cont) =
        setup_window_regions(&mut bus, middle, 10, 20, 110, 210, 0x320000);
    let (back_vis, back_clip, back_struc, back_cont) =
        setup_window_regions(&mut bus, back, 10, 20, 110, 210, 0x330000);

    disp.window_list.replace(vec![front, middle, back]);
    disp.sync_window_list_links(&mut bus);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 18);

    let clobbered_rgn_data: u32 = 0x303000;
    let clobbered_rgn_handle: u32 = 0x303100;
    bus.write_word(clobbered_rgn_data, 10);
    bus.write_word(clobbered_rgn_data + 2, 0);
    bus.write_word(clobbered_rgn_data + 4, 0);
    bus.write_word(clobbered_rgn_data + 6, 200);
    bus.write_word(clobbered_rgn_data + 8, 200);
    bus.write_long(clobbered_rgn_handle, clobbered_rgn_data);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, clobbered_rgn_handle);
    bus.write_long(sp + 4, middle);

    let result = dispatch(&mut disp, 0x10A, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    for (label, rgn_data) in [
        ("middle_cont", middle_cont),
        ("middle_vis", middle_vis),
        ("middle_clip", middle_clip),
    ] {
        assert_eq!(bus.read_word(rgn_data + 2) as i16, 18, "{}.top", label);
        assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "{}.left", label);
        assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "{}.bottom", label);
        assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "{}.right", label);
    }
    assert_eq!(
        bus.read_word(middle_struc + 2) as i16,
        18,
        "middle_struc.top"
    );
    assert_eq!(
        bus.read_word(middle_struc + 4) as i16,
        19,
        "middle_struc.left"
    );
    assert_eq!(
        bus.read_word(middle_struc + 6) as i16,
        112,
        "middle_struc.bottom"
    );
    assert_eq!(
        bus.read_word(middle_struc + 8) as i16,
        212,
        "middle_struc.right"
    );

    for (label, rgn_data) in [
        ("back_cont", back_cont),
        ("back_vis", back_vis),
        ("back_clip", back_clip),
    ] {
        assert_eq!(bus.read_word(rgn_data + 2) as i16, 18, "{}.top", label);
        assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "{}.left", label);
        assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "{}.bottom", label);
        assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "{}.right", label);
    }
    assert_eq!(bus.read_word(back_struc + 2) as i16, 18, "back_struc.top");
    assert_eq!(bus.read_word(back_struc + 4) as i16, 19, "back_struc.left");
    assert_eq!(
        bus.read_word(back_struc + 6) as i16,
        112,
        "back_struc.bottom"
    );
    assert_eq!(
        bus.read_word(back_struc + 8) as i16,
        212,
        "back_struc.right"
    );

    for (label, rgn_data) in [
        ("front_cont", front_cont),
        ("front_struc", front_struc),
        ("front_vis", front_vis),
        ("front_clip", front_clip),
    ] {
        assert_eq!(bus.read_word(rgn_data + 2) as i16, 10, "{}.top", label);
        assert_eq!(bus.read_word(rgn_data + 4) as i16, 20, "{}.left", label);
        assert_eq!(bus.read_word(rgn_data + 6) as i16, 110, "{}.bottom", label);
        assert_eq!(bus.read_word(rgn_data + 8) as i16, 210, "{}.right", label);
    }
}

#[test]
fn calc_vis_subtracts_front_window_structure_from_window_behind() {
    // CalcVis builds a window's visRgn from its content region minus the
    // structure regions of the windows in front of it, so drawing in the
    // back window cannot paint over the front one.
    // Inside Macintosh Volume I (1985), p. I-297.
    let (mut disp, mut cpu, mut bus) = setup();
    let back = bus.alloc(256);
    let front = bus.alloc(256);
    disp.menu_bar_hidden = true;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        disp.screen_mode.0,
        0,
        0,
        200,
        300,
        "",
        0,
        true,
        false,
        false,
        0,
    );
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        front,
        disp.screen_mode.0,
        50,
        60,
        90,
        160,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    assert_eq!(
        disp.window_list.first(),
        Some(front),
        "the newly created window should be frontmost"
    );

    let back_vis = bus.read_long(back + 24);
    assert!(
        super::super::TrapDispatcher::region_is_complex(&bus, back_vis),
        "the back window's visRgn should have a hole where the front window sits"
    );
    assert!(
        !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
        "a point under the front window must be outside the back window's visRgn"
    );
    assert!(
        super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 150, 250),
        "a point clear of the front window must stay inside the back window's visRgn"
    );
}

#[test]
fn begin_update_keeps_front_window_hole_out_of_the_update_clip() {
    // BeginUpdate intersects visRgn with updateRgn. The intersection has to
    // be a real region operation: visRgn already excludes windows in front,
    // and a bounding-box intersection would hand those pixels back and let
    // the window underneath repaint over a modal dialog.
    // Inside Macintosh Volume I (1985), p. I-292.
    let (mut disp, mut cpu, mut bus) = setup();
    let back = bus.alloc(256);
    let front = bus.alloc(256);
    disp.menu_bar_hidden = true;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        disp.screen_mode.0,
        0,
        0,
        200,
        300,
        "",
        0,
        true,
        false,
        false,
        0,
    );
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        front,
        disp.screen_mode.0,
        50,
        60,
        90,
        160,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    let back_update = bus.read_long(back + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET);
    super::super::TrapDispatcher::write_region_handle_rect(
        &mut bus,
        back_update,
        Some((0, 0, 200, 300)),
    );
    disp.begin_update_window(&mut bus, back);

    let back_vis = bus.read_long(back + 24);
    assert!(
        !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
        "BeginUpdate must not restore pixels the front window covers"
    );
    assert!(
        super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 150, 250),
        "BeginUpdate should keep the rest of the update area drawable"
    );

    disp.end_update_window(&mut bus, back);
    assert!(
        !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
        "EndUpdate should recompute visRgn via CalcVis, hole included"
    );
    assert!(
        super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 150, 250),
        "EndUpdate should restore the rest of the content region"
    );
}

#[test]
fn calcvisbehind_menu_clamp_uses_window_local_coordinates() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.menu_bar_hidden = false;
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        100,
        200,
        300,
        500,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    disp.window_list.replace(vec![window_addr]);
    disp.sync_window_list_links(&mut bus);

    let clobbered_rgn = super::super::TrapDispatcher::alloc_rect_region_handle(
        &mut bus,
        Some((100, 200, 300, 500)),
    );
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, clobbered_rgn);
    bus.write_long(sp + 4, window_addr);

    let result = dispatch(&mut disp, 0x10A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        read_window_region_rect(&bus, window_addr, 24),
        (0, 0, 200, 300),
        "a window below the menu bar should not be clipped by global MBarHeight in local coords"
    );
    assert_eq!(
        read_window_region_rect(
            &bus,
            window_addr,
            super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET
        ),
        (100, 200, 300, 500),
        "CalcVisBehind should preserve global content coordinates for nonzero window origins"
    );
}

#[test]
fn checkupdate_returns_true_and_writes_eventrecord_for_pending_update() {
    // CheckUpdate returns TRUE and writes an update EventRecord when a
    // visible window needs updating.
    // Inside Macintosh Volume I (1985), p. I-296;
    // Macintosh Toolbox Essentials (1992), p. 4-116.
    let (mut disp, mut cpu, mut bus) = setup();
    let event_ptr = bus.alloc(16);
    for i in 0..16 {
        bus.write_byte(event_ptr + i, 0xAA);
    }

    let update_window = 0x00A0_4000;
    disp.event_queue.push_back(QueuedEvent {
        what: 6,
        message: update_window,
        when: 0,
        where_v: 77,
        where_h: 123,
        modifiers: 0x4400,
    });

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, event_ptr);
    bus.write_word(sp + 4, 0);

    let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
    assert_eq!(bus.read_word(TEST_SP - 2), 0x0100, "result should be TRUE");
    assert_eq!(
        bus.read_word(event_ptr),
        6,
        "event.what should be updateEvt"
    );
    assert_eq!(
        bus.read_long(event_ptr + 2),
        update_window,
        "event.message should carry WindowPtr"
    );
    assert_eq!(bus.read_word(event_ptr + 10) as i16, 77, "event.where.v");
    assert_eq!(bus.read_word(event_ptr + 12) as i16, 123, "event.where.h");
    assert_eq!(bus.read_word(event_ptr + 14), 0x4400, "event.modifiers");
    assert!(
        disp.event_queue.is_empty(),
        "CheckUpdate should dequeue the consumed update event"
    );
}

#[test]
fn checkupdate_returns_false_when_no_update_is_pending() {
    // CheckUpdate returns FALSE when no visible window requires updating.
    // Inside Macintosh Volume I (1985), p. I-296;
    // Macintosh Toolbox Essentials (1992), p. 4-116.
    let (mut disp, mut cpu, mut bus) = setup();
    let event_ptr = bus.alloc(16);
    for i in 0..16 {
        bus.write_byte(event_ptr + i, 0xCC);
    }

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, event_ptr);
    bus.write_word(sp + 4, 0xFFFF);

    let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
    assert_eq!(bus.read_word(TEST_SP - 2), 0, "result should be FALSE");
    assert_eq!(bus.read_word(event_ptr), 0xCCCC, "event record untouched");
}

#[test]
fn checkupdate_redraws_window_picture_and_continues_to_next_update() {
    // CheckUpdate redraws dirty windows with a non-NIL windowPic itself
    // and continues scanning instead of returning their update events.
    // Macintosh Toolbox Essentials (1992), p. 4-116.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.menu_bar_hidden = true;
    let picture_window = bus.alloc(256);
    let screen_base = disp.screen_mode.0;
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        picture_window,
        screen_base,
        0,
        0,
        8,
        8,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    let picture = make_v1_paintrect_picture(&mut bus, (0, 0, 8, 8));
    bus.write_long(
        picture_window + super::super::TrapDispatcher::WINDOW_PIC_OFFSET,
        picture,
    );

    let ordinary_window = 0x00A0_4000;
    disp.event_queue.push_back(QueuedEvent {
        what: 6,
        message: ordinary_window,
        when: 0,
        where_v: 77,
        where_h: 123,
        modifiers: 0x4400,
    });

    let event_ptr = bus.alloc(16);
    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, event_ptr);
    bus.write_word(sp + 4, 0);

    let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_word(sp + 4), 0x0100);
    assert_eq!(bus.read_word(event_ptr), 6);
    assert_eq!(
        bus.read_long(event_ptr + 2),
        ordinary_window,
        "CheckUpdate should continue to the next ordinary dirty window"
    );
    assert!(
        !disp.window_has_pending_update(&bus, picture_window),
        "the automatic picture redraw should clear its update region"
    );
    assert_eq!(
        bus.read_byte(screen_base),
        0xFF,
        "the installed picture should be rendered into the window"
    );
    assert!(
        !disp
            .event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == picture_window),
        "the pictured window must not retain an application-visible update"
    );
}

#[test]
fn toolbox_events_deliver_front_update_before_redrawing_back_window_picture() {
    // Event Manager update scans use the same front-to-back CheckUpdate
    // ordering: stop at the first ordinary dirty window, then redraw a
    // pictured window behind it on the following scan.
    // Macintosh Toolbox Essentials (1992), p. 4-116.
    let (mut disp, mut cpu, mut bus) = setup();
    disp.menu_bar_hidden = true;
    let screen_base = disp.screen_mode.0;
    let pictured_back = bus.alloc(256);
    let ordinary_front = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        pictured_back,
        screen_base,
        0,
        0,
        8,
        8,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        ordinary_front,
        screen_base,
        20,
        20,
        28,
        28,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    let picture = make_v1_paintrect_picture(&mut bus, (0, 0, 8, 8));
    bus.write_long(
        pictured_back + super::super::TrapDispatcher::WINDOW_PIC_OFFSET,
        picture,
    );

    let (what, message, _, _, _, _, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1u16 << 6);
    assert!(has_event);
    assert_eq!(what, 6);
    assert_eq!(
        message, ordinary_front,
        "the front ordinary window should receive the first update"
    );
    assert!(
        disp.window_has_pending_update(&bus, pictured_back),
        "CheckUpdate must stop before redrawing a pictured window behind an ordinary update"
    );

    disp.begin_update_window(&mut bus, ordinary_front);
    disp.end_update_window(&mut bus, ordinary_front);
    let (what, _, _, _, _, _, has_event) =
        disp.dequeue_toolbox_event(&mut cpu, &mut bus, 1u16 << 6);
    assert!(!has_event);
    assert_eq!(what, 0);
    assert!(
        !disp.window_has_pending_update(&bus, pictured_back),
        "the following scan should redraw and clear the pictured window"
    );
    assert_eq!(
        bus.read_byte(screen_base),
        0xFF,
        "the back window picture should be rendered automatically"
    );
}

#[test]
fn checkupdate_nil_output_pointer_consumes_pending_update() {
    // A nil output pointer is defensive no-op territory in Systemless:
    // the pending update still gets consumed, but nothing is written
    // through the pointer.
    let (mut disp, mut cpu, mut bus) = setup();

    disp.event_queue.push_back(QueuedEvent {
        what: 6,
        message: 0x00A0_4000,
        when: 0,
        where_v: 12,
        where_h: 34,
        modifiers: 0x4400,
    });

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 0xFFFF);

    let result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
    assert_eq!(bus.read_word(TEST_SP - 2), 0x0100, "result should be TRUE");
    assert!(
        disp.event_queue.is_empty(),
        "CheckUpdate(nil) should still consume the pending update"
    );
}

#[test]
fn checkupdate_clears_false_after_validrect_drops_the_pending_update() {
    // CheckUpdate reports TRUE for an invalidated visible window and
    // returns FALSE once ValidRect clears that window's update region.
    let (mut disp, mut cpu, mut bus) = setup();
    let event_ptr = bus.alloc(16);
    let bounds_rect_ptr = bus.alloc(8);
    let sp = TEST_SP - 26;
    let wnd;

    bus.write_word(bounds_rect_ptr, 60);
    bus.write_word(bounds_rect_ptr + 2, 80);
    bus.write_word(bounds_rect_ptr + 4, 180);
    bus.write_word(bounds_rect_ptr + 6, 260);

    cpu.write_reg(Register::A7, sp);
    for i in 0..30u32 {
        bus.write_byte(sp + i, 0);
    }
    bus.write_long(sp + 18, bounds_rect_ptr);
    bus.write_byte(sp + 12, 1);
    bus.write_long(sp + 6, 0xFFFFFFFF);

    let new_window = dispatch(&mut disp, 0x113, &mut cpu, &mut bus);
    assert!(new_window.is_some(), "NewWindow should be handled");
    assert!(new_window.unwrap().is_ok(), "NewWindow should return");
    wnd = bus.read_long(cpu.read_reg(Register::A7));
    disp.validate_window_rect(&mut bus, wnd, (0, 0, 120, 180));

    bus.write_byte(event_ptr, 0xAA);
    bus.write_byte(event_ptr + 1, 0xAA);
    bus.write_word(event_ptr + 2, 0xAAAA);
    bus.write_long(event_ptr + 4, 0xAAAAAAAA);
    bus.write_word(event_ptr + 8, 0xAAAA);
    bus.write_word(event_ptr + 10, 0xAAAA);
    bus.write_word(event_ptr + 12, 0xAAAA);
    bus.write_word(event_ptr + 14, 0xAAAA);

    disp.invalidate_window_rect(&mut bus, wnd, (30, 20, 120, 90));

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, event_ptr);
    bus.write_word(sp + 4, 0xFFFF);

    let result_true = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(result_true.is_some(), "CheckUpdate should be handled");
    assert!(result_true.unwrap().is_ok(), "CheckUpdate should return");
    assert_eq!(bus.read_word(TEST_SP - 2), 0x0100, "result should be TRUE");
    assert_eq!(
        bus.read_word(event_ptr),
        6,
        "CheckUpdate should write updateEvt when the window is invalidated",
    );

    disp.validate_window_rect(&mut bus, wnd, (30, 20, 120, 90));

    bus.write_byte(event_ptr, 0xAA);
    bus.write_byte(event_ptr + 1, 0xAA);
    bus.write_word(event_ptr + 2, 0xAAAA);
    bus.write_long(event_ptr + 4, 0xAAAAAAAA);
    bus.write_word(event_ptr + 8, 0xAAAA);
    bus.write_word(event_ptr + 10, 0xAAAA);
    bus.write_word(event_ptr + 12, 0xAAAA);
    bus.write_word(event_ptr + 14, 0xAAAA);

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, event_ptr);
    bus.write_word(sp + 4, 0);

    let result_false = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(result_false.is_some(), "CheckUpdate should be handled");
    assert!(result_false.unwrap().is_ok(), "CheckUpdate should return");
    assert_eq!(bus.read_word(TEST_SP - 2), 0, "result should be FALSE");
    assert_eq!(
        bus.read_word(event_ptr),
        0xAAAA,
        "CheckUpdate should leave the caller's EventRecord untouched after ValidRect",
    );
}

// ---------------------------------------------------------------
// 17. SetWRefCon (0x118) -- pops 8 bytes
// ---------------------------------------------------------------
#[test]
fn test_set_wrefcon() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x12345678); // refCon
    bus.write_long(sp + 4, 0xDEAD0000); // window

    let result = dispatch(&mut disp, 0x118, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// ---------------------------------------------------------------
// 18. GetWRefCon (0x117) -- pops 4, writes 0 at SP+4
// ---------------------------------------------------------------
#[test]
fn test_get_wrefcon() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0xDEAD0000); // window

    // Write a non-zero value where the result will go to verify it gets zeroed
    bus.write_long(sp + 4, 0xFFFFFFFF);

    let result = dispatch(&mut disp, 0x117, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    // Result (0) written at old sp + 4 = TEST_SP
    let refcon = bus.read_long(TEST_SP);
    assert_eq!(refcon, 0, "GetWRefCon should return 0");
}

#[test]
fn setwincolor_consumes_arguments_sets_content_color_and_queues_update() {
    // Macintosh Toolbox Essentials (1992), pp. 4-114..4-115:
    // SetWinColor applies a window color table and redraws the window in
    // the new colors.
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        40,
        40,
        200,
        200,
        "",
        0,
        true,
        true,
        false,
        0,
    );
    let aux_handle_before = disp
        .window_aux_records
        .get(&window_addr)
        .copied()
        .expect("fresh CGraf window should have an AuxWin record");
    let aux_ptr_before = bus.read_long(aux_handle_before);
    let default_ctab =
        bus.read_long(aux_ptr_before + super::super::TrapDispatcher::AUX_WIN_CTABLE_OFFSET);
    disp.event_queue.clear();

    // One-entry WinCTab: entry value 0 (wContentColor) -> RGB.
    let wctab_ptr = bus.alloc(16);
    let wctab_handle = bus.alloc(4);
    bus.write_long(wctab_handle, wctab_ptr);
    bus.write_word(wctab_ptr + 6, 0); // ctSize = 0 (one entry)
    bus.write_word(wctab_ptr + 8, 0); // part id = wContentColor
    bus.write_word(wctab_ptr + 10, 0x1234);
    bus.write_word(wctab_ptr + 12, 0x5678);
    bus.write_word(wctab_ptr + 14, 0x9ABC);

    // Pascal stack order in this trap surface: second parameter at SP,
    // first parameter at SP+4.
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, wctab_handle); // newColorTable
    bus.write_long(sp + 4, window_addr); // theWindow

    let result = dispatch(&mut disp, 0x241, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetWinColor should be handled");
    assert!(result.unwrap().is_ok(), "SetWinColor should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "SetWinColor should consume two pointer arguments"
    );
    assert_eq!(
        bus.read_word(window_addr + 42),
        0x1234,
        "SetWinColor should write content red channel"
    );
    assert_eq!(
        bus.read_word(window_addr + 44),
        0x5678,
        "SetWinColor should write content green channel"
    );
    assert_eq!(
        bus.read_word(window_addr + 46),
        0x9ABC,
        "SetWinColor should write content blue channel"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == window_addr),
        "SetWinColor should queue an update event for the target window"
    );
    assert_eq!(
        disp.window_aux_records.get(&window_addr).copied(),
        Some(aux_handle_before),
        "SetWinColor should update the existing AuxWin handle in place"
    );
    assert_eq!(
        bus.read_long(aux_ptr_before + super::super::TrapDispatcher::AUX_WIN_CTABLE_OFFSET),
        wctab_handle,
        "SetWinColor should rewrite awCTable to the supplied WCTabHandle"
    );
    assert_ne!(
        default_ctab, wctab_handle,
        "fixture should replace the default AuxWin color table with a new handle"
    );
}

#[test]
fn getauxwin_returns_true_writes_aux_handle_and_pops_arguments_for_tracked_window() {
    // Fresh NewWindow/NewCWindow objects on BasiliskII already expose a
    // non-NIL AuxWin record; HLE tracks the same caller-observable state.
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        disp.screen_mode.0,
        40,
        40,
        200,
        200,
        "",
        0,
        true,
        true,
        false,
        0,
    );
    let expected_aux = disp
        .window_aux_records
        .get(&window_addr)
        .copied()
        .expect("fresh CGraf window should have an AuxWin record");
    let expected_aux_ptr = bus.read_long(expected_aux);

    let aw_out = bus.alloc(4);
    bus.write_long(aw_out, 0xDEAD_BEEF);

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, aw_out); // VAR awHndl
    bus.write_long(sp + 4, window_addr); // theWindow
    bus.write_word(sp + 8, 0xFFFF); // result sentinel

    let result = dispatch(&mut disp, 0x242, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetAuxWin should be handled");
    assert!(result.unwrap().is_ok(), "GetAuxWin should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP - 2,
        "GetAuxWin should consume two pointer arguments and leave result slot"
    );
    assert_eq!(
        bus.read_long(aw_out),
        expected_aux,
        "GetAuxWin should write the tracked AuxWinHandle"
    );
    assert_eq!(
        bus.read_word(TEST_SP - 2),
        0x0100,
        "GetAuxWin should return TRUE for tracked windows"
    );
    assert_eq!(
        bus.read_long(expected_aux_ptr + super::super::TrapDispatcher::AUX_WIN_OWNER_OFFSET),
        window_addr,
        "AuxWin record should point back to the tracked window"
    );
    assert_ne!(
        bus.read_long(expected_aux_ptr + super::super::TrapDispatcher::AUX_WIN_CTABLE_OFFSET),
        0,
        "AuxWin record should carry a non-NIL color table handle"
    );
}

#[test]
fn getauxwin_returns_false_writes_nil_and_pops_arguments_for_untracked_pointer() {
    // Macintosh Toolbox Essentials (1992), p. 4-115:
    // GetAuxWin reports FALSE when the queried window has no tracked
    // auxiliary window record.
    let (mut disp, mut cpu, mut bus) = setup();

    let aw_out = bus.alloc(4);
    bus.write_long(aw_out, 0xDEAD_BEEF);

    // FUNCTION result slot (Boolean) lives at SP+8 after two pointer args.
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, aw_out); // VAR awHndl
    bus.write_long(sp + 4, 0x00C0_FFEE); // theWindow
    bus.write_word(sp + 8, 0xFFFF); // result sentinel

    let result = dispatch(&mut disp, 0x242, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetAuxWin should be handled");
    assert!(result.unwrap().is_ok(), "GetAuxWin should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP - 2,
        "GetAuxWin should consume two pointer arguments and leave result slot"
    );
    assert_eq!(
        bus.read_long(aw_out),
        0,
        "GetAuxWin should write NIL when no aux-window record exists"
    );
    assert_eq!(
        bus.read_word(TEST_SP - 2),
        0,
        "GetAuxWin should return FALSE when no aux-window record exists"
    );
}

// ---------------------------------------------------------------
// Helper: set up a window structure at a given address with valid
// portRect and region handles for MoveWindow/SizeWindow tests.
// ---------------------------------------------------------------
// Full window setup including contRgn + updateRgn so tests can
// exercise the fUpdate=TRUE invalidation path. Returns the contRgn
// and updateRgn data pointers for inspection.
fn setup_full_window_with_regions(
    bus: &mut crate::memory::MacMemoryBus,
    window_addr: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> (u32, u32) {
    setup_window_with_regions(bus, window_addr, top, left, bottom, right);
    // contRgn at +118
    let cont_rgn_data: u32 = 0x303000;
    let cont_rgn_handle: u32 = 0x303100;
    bus.write_word(cont_rgn_data, 10);
    bus.write_word(cont_rgn_data + 2, top as u16);
    bus.write_word(cont_rgn_data + 4, left as u16);
    bus.write_word(cont_rgn_data + 6, bottom as u16);
    bus.write_word(cont_rgn_data + 8, right as u16);
    bus.write_long(cont_rgn_handle, cont_rgn_data);
    bus.write_long(window_addr + 118, cont_rgn_handle);
    // updateRgn at +122 — starts empty
    let update_rgn_data: u32 = 0x304000;
    let update_rgn_handle: u32 = 0x304100;
    bus.write_word(update_rgn_data, 10);
    bus.write_word(update_rgn_data + 2, 0);
    bus.write_word(update_rgn_data + 4, 0);
    bus.write_word(update_rgn_data + 6, 0);
    bus.write_word(update_rgn_data + 8, 0);
    bus.write_long(update_rgn_handle, update_rgn_data);
    bus.write_long(window_addr + 122, update_rgn_handle);
    (cont_rgn_data, update_rgn_data)
}

fn setup_window_with_regions(
    bus: &mut crate::memory::MacMemoryBus,
    window_addr: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> (u32, u32) {
    // portRect at window + 16..22
    bus.write_word(window_addr + 16, top as u16);
    bus.write_word(window_addr + 18, left as u16);
    bus.write_word(window_addr + 20, bottom as u16);
    bus.write_word(window_addr + 22, right as u16);

    // visRgn: data at 0x301000, handle at 0x301100
    let vis_rgn_data: u32 = 0x301000;
    let vis_rgn_handle: u32 = 0x301100;
    bus.write_word(vis_rgn_data, 10); // rgnSize
    bus.write_word(vis_rgn_data + 2, top as u16);
    bus.write_word(vis_rgn_data + 4, left as u16);
    bus.write_word(vis_rgn_data + 6, bottom as u16);
    bus.write_word(vis_rgn_data + 8, right as u16);
    bus.write_long(vis_rgn_handle, vis_rgn_data);
    bus.write_long(window_addr + 24, vis_rgn_handle);

    // clipRgn: data at 0x302000, handle at 0x302100
    let clip_rgn_data: u32 = 0x302000;
    let clip_rgn_handle: u32 = 0x302100;
    bus.write_word(clip_rgn_data, 10);
    bus.write_word(clip_rgn_data + 2, top as u16);
    bus.write_word(clip_rgn_data + 4, left as u16);
    bus.write_word(clip_rgn_data + 6, bottom as u16);
    bus.write_word(clip_rgn_data + 8, right as u16);
    bus.write_long(clip_rgn_handle, clip_rgn_data);
    bus.write_long(window_addr + 28, clip_rgn_handle);

    (vis_rgn_data, clip_rgn_data)
}

fn install_wstate_data(
    bus: &mut crate::memory::MacMemoryBus,
    window_addr: u32,
    user_state: (i16, i16, i16, i16),
    std_state: (i16, i16, i16, i16),
) {
    let data_ptr: u32 = 0x305000;
    let data_handle: u32 = 0x305100;
    bus.write_long(data_handle, data_ptr);
    bus.write_long(window_addr + 130, data_handle);

    bus.write_word(data_ptr, user_state.0 as u16);
    bus.write_word(data_ptr + 2, user_state.1 as u16);
    bus.write_word(data_ptr + 4, user_state.2 as u16);
    bus.write_word(data_ptr + 6, user_state.3 as u16);

    bus.write_word(data_ptr + 8, std_state.0 as u16);
    bus.write_word(data_ptr + 10, std_state.1 as u16);
    bus.write_word(data_ptr + 12, std_state.2 as u16);
    bus.write_word(data_ptr + 14, std_state.3 as u16);
}

// ---------------------------------------------------------------
// 19. MoveWindow (0x11B) -- moves window, updates portBits.bounds
//     portRect stays in local coords; portBits.bounds maps local→screen.
//     Stack: SP+0=front(2), SP+2=vGlobal(2), SP+4=hGlobal(2), SP+6=theWindow(4)
//     Pops 10 bytes.
// ---------------------------------------------------------------
#[test]
fn move_window_preserves_nonzero_local_origin() {
    for trap in [0x113, 0x245] {
        let (mut disp, mut cpu, mut bus) = setup();
        let bounds = bus.alloc(8);
        for (offset, value) in [(0, 57), (2, 105), (4, 343), (6, 458)] {
            bus.write_word(bounds + offset, value);
        }
        let sp = TEST_SP - 30;
        cpu.write_reg(Register::A7, sp);
        bus.write_bytes(sp, &[0; 30]);
        bus.write_long(sp + 6, u32::MAX);
        bus.write_long(sp + 18, bounds);
        dispatch(&mut disp, trap, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let window = bus.read_long(cpu.read_reg(Register::A7));
        disp.set_current_port_state(&mut bus, &mut cpu, window, None);
        cpu.write_reg(Register::A7, TEST_SP - 4);
        bus.write_word(TEST_SP - 4, (-152i16) as u16);
        bus.write_word(TEST_SP - 2, (-195i16) as u16);
        disp.dispatch_quickdraw(true, 0x078, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let local_before = disp.window_port_rect(&bus, window);
        let sp = TEST_SP - 10;
        cpu.write_reg(Register::A7, sp);
        bus.write_word(sp, 0);
        bus.write_word(sp + 2, 166);
        bus.write_word(sp + 4, 222);
        bus.write_long(sp + 6, window);
        dispatch(&mut disp, 0x11B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(disp.window_port_rect(&bus, window), local_before);
        assert_eq!(disp.port_bounds_top_left(&bus, window), (-318, -417));
        assert_eq!(
            disp.window_global_port_rect(&bus, window),
            (166, 222, 452, 575)
        );
    }
}

#[test]
fn test_move_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    let window_addr: u32 = 0x300000;
    let (vis_rgn_data, clip_rgn_data) =
        setup_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);

    // Push 10 bytes of params
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1); // front (boolean)
    bus.write_word(sp + 2, 100); // vGlobal = 100
    bus.write_word(sp + 4, 50); // hGlobal = 50
    bus.write_long(sp + 6, window_addr); // theWindow

    let result = dispatch(&mut disp, 0x11B, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    // portRect stays in local coords — unchanged
    assert_eq!(
        bus.read_word(window_addr + 16) as i16,
        40,
        "portRect.top unchanged"
    );
    assert_eq!(
        bus.read_word(window_addr + 18) as i16,
        0,
        "portRect.left unchanged"
    );
    assert_eq!(
        bus.read_word(window_addr + 20) as i16,
        342,
        "portRect.bottom unchanged"
    );
    assert_eq!(
        bus.read_word(window_addr + 22) as i16,
        512,
        "portRect.right unchanged"
    );

    // Bitmap bounds preserve portRect origin: top=40-vGlobal, left=-hGlobal
    // GrafPort portBits.bounds at offset 8..16
    assert_eq!(
        bus.read_word(window_addr + 8) as i16,
        -60,
        "portBits.bounds.top"
    );
    assert_eq!(
        bus.read_word(window_addr + 10) as i16,
        -50,
        "portBits.bounds.left"
    );

    // visRgn and clipRgn stay in local coords — unchanged
    assert_eq!(
        bus.read_word(vis_rgn_data + 2) as i16,
        40,
        "visRgn.top unchanged"
    );
    assert_eq!(
        bus.read_word(vis_rgn_data + 4) as i16,
        0,
        "visRgn.left unchanged"
    );
    assert_eq!(
        bus.read_word(clip_rgn_data + 2) as i16,
        40,
        "clipRgn.top unchanged"
    );
    assert_eq!(
        bus.read_word(clip_rgn_data + 4) as i16,
        0,
        "clipRgn.left unchanged"
    );
}

#[test]
fn moved_dialog_restores_background_at_its_destination() {
    check_moved_dialog_background(true);
}

#[test]
fn dialog_moved_before_showing_restores_destination_background() {
    check_moved_dialog_background(false);
}

fn check_moved_dialog_background(visible: bool) {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen = bus.alloc(320 * 240);
    bus.write_long(0x0824, screen);
    disp.screen_mode = (screen, 320, 320, 240, 8);
    let back = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus, &mut cpu, back, screen, 20, 20, 220, 300, "Back", 0, true, false, false, 0,
    );
    let dialog = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus, &mut cpu, dialog, screen, 100, 80, 170, 200, "", 1, visible, false, false, 0,
    );
    bus.write_word(dialog + 108, 2);
    disp.dialog_items.insert(dialog, Vec::new());
    disp.front_window = dialog;
    disp.window_bounds = (100, 80, 170, 200);
    // Distinct rows expose a background that is accidentally translated
    // along with the dialog. Keep both positions over the back window.
    for y in 20..220u32 {
        for x in 20..300u32 {
            bus.write_byte(screen + y * 320 + x, y as u8);
        }
    }
    let original = disp.save_dialog_pixels(&bus, (100, 80, 170, 200));
    disp.dialog_saved_pixels.insert(dialog, original);
    if visible {
        for y in 100..170u32 {
            for x in 80..200u32 {
                bus.write_byte(screen + y * 320 + x, 250);
            }
        }
    }

    disp.move_window_to_global(&mut bus, dialog, 100, 60, false);
    assert_eq!(
        bus.read_byte(screen + 80 * 320 + 120),
        if visible { 250 } else { 80 }
    );
    // Simulate showing and drawing the dialog after a hidden move.
    bus.write_byte(dialog + 110, 1);
    for y in 60..130u32 {
        for x in 100..220u32 {
            bus.write_byte(screen + y * 320 + x, 250);
        }
    }
    assert_eq!(bus.read_byte(screen + 155 * 320 + 90), 155);
    // CloseDialog must restore the row that was underneath the new
    // location, including the overlap with the old dialog rectangle.
    cpu.write_reg(Register::A7, TEST_SP);
    bus.write_long(TEST_SP, dialog);
    disp.dispatch_dialog(true, 0x182, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_byte(screen + 80 * 320 + 120), 80);
    assert_eq!(bus.read_byte(screen + 110 * 320 + 120), 110);
    assert_eq!(bus.read_byte(screen + 155 * 320 + 90), 155);
}

#[test]
fn move_window_restores_exposed_desktop_when_host_hides_menu_bar() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);
    disp.menu_bar_hidden = true;
    super::super::TrapDispatcher::fb_fill_pattern_rect(
        &mut bus,
        screen_base,
        800,
        8,
        800,
        600,
        0,
        0,
        600,
        800,
        [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
    );
    let old_content_probe = screen_base + 200 * 800 + 410;
    let desktop_pixel = disp.theme_pixel_index(&bus, disp.ui_theme().palette().desktop_light);

    let window_addr = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        screen_base,
        180,
        400,
        420,
        600,
        "Player",
        4,
        true,
        true,
        true,
        0,
    );

    assert_eq!(
        bus.read_byte(old_content_probe),
        0,
        "precondition: old window content area starts white"
    );

    disp.move_window_to_global(&mut bus, window_addr, 450, 230, true);

    assert_eq!(
        bus.read_byte(old_content_probe),
        desktop_pixel,
        "host menu suppression must restore the light-blue desktop"
    );
    let new_content_probe = screen_base + 250 * 800 + 460;
    assert_eq!(
        bus.read_byte(new_content_probe),
        0,
        "moving a visible window should preserve the window's screen pixels at the new position"
    );
}

#[test]
fn move_window_reveals_and_invalidates_background_content() {
    // MoveWindow/DragWindow must pair the screen copy with CalcVisBehind
    // and PaintBehind semantics. The back window's old occlusion hole is
    // removed and its newly exposed content is queued for application
    // redraw. Inside Macintosh Volume I, I-289, I-293, I-297.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(320 * 240);
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 320, 320, 240, 8);
    disp.menu_bar_hidden = true;

    let back = bus.alloc(256);
    let front = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        screen_base,
        20,
        20,
        220,
        300,
        "Back",
        0,
        true,
        false,
        false,
        0,
    );
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        front,
        screen_base,
        50,
        60,
        130,
        180,
        "Front",
        0,
        true,
        false,
        false,
        0,
    );
    let back_vis = bus.read_long(back + 24);
    assert!(
        !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 80, 100),
        "precondition: the front structure occludes the back window"
    );
    disp.validate_window_rect(&mut bus, back, (0, 0, 200, 280));
    disp.validate_window_rect(&mut bus, front, (0, 0, 80, 120));

    // PaintBehind clears PaintWhite: newly exposed application pixels
    // stay untouched until its update event is handled (IM:I I-297).
    let exposed_pixel = screen_base + 80 * 320 + 100;
    bus.write_byte(exposed_pixel, 173);

    disp.move_window_to_global(&mut bus, front, 190, 140, true);

    assert!(
        super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 80, 100),
        "the back visRgn must expose the front window's former location"
    );
    assert_eq!(
        bus.read_byte(exposed_pixel),
        173,
        "moving a window must not paint desktop over another window's content"
    );
    let back_update = bus.read_long(back + super::super::TrapDispatcher::WINDOW_UPDATE_RGN_OFFSET);
    assert!(
        super::super::TrapDispatcher::region_contains_point(&bus, back_update, 80, 100),
        "the newly exposed back content must be invalidated"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == back),
        "the back window must receive an update event"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == front),
        "the moved window must redraw any newly visible content"
    );
}

// ---------------------------------------------------------------
// 20. SizeWindow (0x11D) -- resizes window, updates portRect & regions
//     Stack: SP+0=fUpdate(2), SP+2=h(2), SP+4=w(2), SP+6=theWindow(4)
//     Pops 10 bytes.
// ---------------------------------------------------------------
#[test]
fn test_size_window() {
    let (mut disp, mut cpu, mut bus) = setup();

    let window_addr: u32 = 0x300000;
    let (vis_rgn_data, clip_rgn_data) =
        setup_window_with_regions(&mut bus, window_addr, 40, 0, 342, 512);

    // Push 10 bytes of params
    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1); // fUpdate
    bus.write_word(sp + 2, 480); // h = 480
    bus.write_word(sp + 4, 640); // w = 640
    bus.write_long(sp + 6, window_addr); // theWindow

    let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    // SizeWindow sets portRect to local coords: (0, 0, h, w)
    assert_eq!(bus.read_word(window_addr + 16) as i16, 0, "portRect.top");
    assert_eq!(bus.read_word(window_addr + 18) as i16, 0, "portRect.left");
    assert_eq!(
        bus.read_word(window_addr + 20) as i16,
        480,
        "portRect.bottom"
    );
    assert_eq!(
        bus.read_word(window_addr + 22) as i16,
        640,
        "portRect.right"
    );

    // visRgn updated in local coords (top kept from existing region)
    assert_eq!(bus.read_word(vis_rgn_data + 2) as i16, 40, "visRgn.top");
    assert_eq!(bus.read_word(vis_rgn_data + 4) as i16, 0, "visRgn.left");
    assert_eq!(bus.read_word(vis_rgn_data + 6) as i16, 480, "visRgn.bottom");
    assert_eq!(bus.read_word(vis_rgn_data + 8) as i16, 640, "visRgn.right");

    // clipRgn updated in local coords (top kept from existing region)
    assert_eq!(bus.read_word(clip_rgn_data + 2) as i16, 40, "clipRgn.top");
    assert_eq!(bus.read_word(clip_rgn_data + 4) as i16, 0, "clipRgn.left");
    assert_eq!(
        bus.read_word(clip_rgn_data + 6) as i16,
        480,
        "clipRgn.bottom"
    );
    assert_eq!(
        bus.read_word(clip_rgn_data + 8) as i16,
        640,
        "clipRgn.right"
    );
}

#[test]
fn size_window_recalculates_background_occlusion_region() {
    // IM:I 1985 pp. I-287 and I-297: resizing a background window must
    // recalculate its visRgn against the structure regions in front.
    let (mut disp, mut cpu, mut bus) = setup();
    let back = bus.alloc(256);
    let front = bus.alloc(256);
    disp.menu_bar_hidden = true;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        back,
        disp.screen_mode.0,
        0,
        0,
        200,
        300,
        "",
        0,
        true,
        false,
        false,
        0,
    );
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        front,
        disp.screen_mode.0,
        50,
        60,
        90,
        160,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    let back_vis = bus.read_long(back + 24);
    assert!(
        !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
        "the initial background visRgn must exclude the front window"
    );

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0);
    bus.write_word(sp + 2, 250);
    bus.write_word(sp + 4, 350);
    bus.write_long(sp + 6, back);
    let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert!(
        !super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 70, 100),
        "the resized background must still exclude the front window"
    );
    assert!(
        super::super::TrapDispatcher::region_contains_point(&bus, back_vis, 225, 325),
        "newly enlarged unobscured background pixels must become visible"
    );
}

// IM:I p.I-296 (with Rect-by-pointer calling convention on p.I-91):
// DragWindow moves the window by the release-point delta when the
// mouse-up location is inside the global boundsRect.
#[test]
fn dragwindow_moves_window_to_release_delta_inside_boundsrect() {
    let (mut disp, mut cpu, mut bus) = setup();

    let window_addr: u32 = 0x300000;
    setup_window_with_regions(&mut bus, window_addr, 0, 0, 100, 200);
    bus.write_word(window_addr + 6, 0); // GrafPort
    bus.write_word(window_addr + 8, (-40i16) as u16);
    bus.write_word(window_addr + 10, (-20i16) as u16);
    bus.write_word(window_addr + 12, 560);
    bus.write_word(window_addr + 14, 780);
    bus.write_byte(window_addr + 110, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;
    disp.window_bounds = (40, 20, 140, 220);

    let bounds_rect_ptr = 0x320000;
    bus.write_word(bounds_rect_ptr, 0);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 400);
    bus.write_word(bounds_rect_ptr + 6, 600);

    disp.push_mouse_down(50, 30);

    let sp = TEST_SP - 12;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, bounds_rect_ptr); // boundsRect pointer
    bus.write_long(sp + 4, 0x0032_001E); // startPt v=50, h=30
    bus.write_long(sp + 8, window_addr); // theWindow

    let result = dispatch(&mut disp, 0x125, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert!(disp.window_tracking.is_some());

    disp.set_mouse_position(70, 80);
    let result = dispatch(&mut disp, 0x125, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert_eq!(
        disp.window_tracking.as_ref().unwrap().outline_rect,
        (60, 70, 160, 270),
        "the retained gray outline follows the live mouse delta"
    );

    disp.push_mouse_up(70, 80);
    let result = dispatch(&mut disp, 0x125, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert!(disp.window_tracking.is_none());
    assert!(disp.event_queue.iter().all(|event| event.what != 2));
    assert_eq!(
        bus.read_word(window_addr + 8) as i16,
        -60,
        "portBits.bounds.top tracks moved global origin"
    );
    assert_eq!(
        bus.read_word(window_addr + 10) as i16,
        -70,
        "portBits.bounds.left tracks moved global origin"
    );
    assert_eq!(
        disp.window_bounds,
        (60, 70, 160, 270),
        "front-window hit-test bounds move with DragWindow"
    );
}

#[test]
fn dragwindow_latches_command_and_moves_without_activating_window() {
    // Command-dragging moves an inactive window without bringing it to
    // the front. The modifier is sampled at mouse-down, not release.
    // Macintosh Toolbox Essentials (1992), p. 4-92.
    let (mut disp, mut cpu, mut bus) = setup();
    let target: u32 = 0x300000;
    let front: u32 = 0x301000;
    setup_window_with_regions(&mut bus, target, 0, 0, 100, 200);
    setup_window_with_regions(&mut bus, front, 0, 0, 100, 200);
    for (window, top, left) in [(target, 40i16, 20i16), (front, 80, 100)] {
        bus.write_word(window + 6, 0);
        bus.write_word(window + 8, (-top) as u16);
        bus.write_word(window + 10, (-left) as u16);
        bus.write_word(window + 12, (600 - top) as u16);
        bus.write_word(window + 14, (800 - left) as u16);
        bus.write_byte(window + 110, 0xFF);
    }
    disp.window_list.replace(vec![front, target]);
    disp.front_window = front;
    disp.window_bounds = (80, 100, 180, 300);

    let bounds_rect_ptr = 0x320000;
    bus.write_word(bounds_rect_ptr, 0);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 400);
    bus.write_word(bounds_rect_ptr + 6, 600);
    let sp = TEST_SP - 12;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, bounds_rect_ptr);
    bus.write_long(sp + 4, 0x0032_001E);
    bus.write_long(sp + 8, target);

    disp.push_key_down(0x37, 0);
    disp.push_mouse_down(50, 30);
    assert!(dispatch(&mut disp, 0x125, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());
    disp.push_key_up(0x37, 0);
    disp.push_mouse_up(70, 80);
    assert!(dispatch(&mut disp, 0x125, &mut cpu, &mut bus)
        .unwrap()
        .is_ok());

    assert_eq!(disp.front_window, front);
    assert_eq!(disp.window_list, vec![front, target]);
    assert_eq!(bus.read_word(target + 8) as i16, -60);
    assert_eq!(bus.read_word(target + 10) as i16, -70);
}

// IM:I p.I-296 (with Rect-by-pointer calling convention on p.I-91):
// DragWindow consumes WindowPtr + Point + RectPtr and leaves the
// window in place when the release point is outside boundsRect.
#[test]
fn dragwindow_release_outside_boundsrect_leaves_window_unchanged() {
    let (mut disp, mut cpu, mut bus) = setup();

    let window_addr: u32 = 0x300000;
    for i in 0..32u32 {
        bus.write_byte(window_addr + i, (i as u8).wrapping_mul(7));
    }
    let before: Vec<u8> = (0..32u32).map(|i| bus.read_byte(window_addr + i)).collect();

    let bounds_rect_ptr = 0x320000;
    bus.write_word(bounds_rect_ptr, 0);
    bus.write_word(bounds_rect_ptr + 2, 0);
    bus.write_word(bounds_rect_ptr + 4, 400);
    bus.write_word(bounds_rect_ptr + 6, 500);

    disp.set_mouse_position(600, 700);

    let sp = TEST_SP - 12;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, bounds_rect_ptr); // boundsRect pointer
    bus.write_long(sp + 4, 0x0010_0020); // startPt (global Point)
    bus.write_long(sp + 8, window_addr); // theWindow

    let result = dispatch(&mut disp, 0x125, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    let after: Vec<u8> = (0..32u32).map(|i| bus.read_byte(window_addr + i)).collect();
    assert_eq!(after, before);
}

#[test]
fn hidden_menu_mode_draws_document_window_variant_frames() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);
    for proc_id in [0, 4, 8, 12, 16] {
        for offset in 0..800 * 600 {
            bus.write_byte(screen_base + offset, 0xAA);
        }
        disp.menu_bar_hidden = true;
        disp.front_window = 0;
        disp.window_proc_id = 0;
        disp.window_list.clear();
        disp.window_saved_under_pixels.clear();

        let window_addr = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            screen_base,
            180,
            400,
            420,
            600,
            "Player",
            proc_id,
            true,
            true,
            true,
            0,
        );

        assert_ne!(
                bus.read_byte(screen_base + 240 * 800 + 450),
                0xAA,
                "document window procID {proc_id} should erase its content even when the host menu bar is hidden"
            );
        assert_ne!(
            bus.read_byte(screen_base + 162 * 800 + 450),
            0xAA,
            "document window procID {proc_id} should still draw title-bar chrome"
        );
    }
}

#[test]
fn port_changed_resync_follows_a_port_rect_the_application_rewrote_itself() {
    // HyperCard resizes its card window by writing portRect directly and
    // calling PortChanged ($AB1D selector 9) rather than SizeWindow, so the
    // regions QuickDraw and the Window Manager clip against have to be
    // re-derived from the port. Myst Preview's card is 544x332 inside a
    // window NewWindow was told was 512x342.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);

    let window_addr = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window_addr,
        screen_base,
        134,
        128,
        476,
        640,
        "Card",
        4,
        true,
        false,
        false,
        0,
    );
    assert_eq!(bus.read_word(window_addr + 20) as i16, 342);
    assert_eq!(bus.read_word(window_addr + 22) as i16, 512);

    // The application rewrites portRect behind our back, then announces it.
    bus.write_word(window_addr + 20, 332);
    bus.write_word(window_addr + 22, 544);
    disp.resync_window_geometry_from_port_rect(&mut bus, window_addr);

    let vis_rgn = bus.read_long(bus.read_long(window_addr + 24));
    assert_eq!(bus.read_word(vis_rgn + 6) as i16, 332, "visRgn.bottom");
    assert_eq!(bus.read_word(vis_rgn + 8) as i16, 544, "visRgn.right");

    let cont_rect = super::super::TrapDispatcher::region_handle_rect(
        &bus,
        bus.read_long(window_addr + super::super::TrapDispatcher::WINDOW_CONT_RGN_OFFSET),
    )
    .expect("content region");
    assert_eq!(
        (cont_rect.2 - cont_rect.0, cont_rect.3 - cont_rect.1),
        (332, 544),
        "content region should take the port's size"
    );
    assert_eq!(
        disp.window_bounds, cont_rect,
        "cached front-window bounds should track the content region"
    );

    // Idempotent: a second announcement with nothing changed is a no-op.
    let before = disp.window_bounds;
    disp.resync_window_geometry_from_port_rect(&mut bus, window_addr);
    assert_eq!(disp.window_bounds, before);
}

#[test]
fn windows_created_wholly_off_screen_get_no_synthesised_chrome() {
    // An application that parks a window off-screen intends to drive its
    // content itself; real hardware draws that window's frame where it was
    // asked to, out of sight. HyperCard does this with its card window (its
    // NewWindow rect is at 16513,16528) and then blits the card straight
    // into the screen bitmap.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);

    let parked = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        parked,
        screen_base,
        16513,
        16528,
        16855,
        17040,
        "Card",
        4,
        true,
        false,
        false,
        0,
    );
    assert!(
        disp.windows_placed_offscreen.contains(&parked),
        "a window created entirely off-screen should be recorded as parked"
    );

    let on_screen = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        on_screen,
        screen_base,
        60,
        40,
        300,
        400,
        "Normal",
        4,
        true,
        false,
        false,
        0,
    );
    assert!(!disp.windows_placed_offscreen.contains(&on_screen));

    disp.untrack_window(&mut bus, parked);
    assert!(!disp.windows_placed_offscreen.contains(&parked));
}

#[test]
fn port_changed_resync_ignores_ports_that_are_not_tracked_windows() {
    let (mut disp, _cpu, mut bus) = setup();
    let port = bus.alloc(256);
    bus.write_word(port + 20, 332);
    bus.write_word(port + 22, 544);
    let before = disp.window_bounds;
    disp.resync_window_geometry_from_port_rect(&mut bus, port);
    assert_eq!(disp.window_bounds, before);
}

fn write_drag_region_frame(
    bus: &mut crate::memory::MacMemoryBus,
    sp: u32,
    start: (i16, i16),
    limit_rect_ptr: u32,
    slop_rect_ptr: u32,
    axis: i16,
) {
    bus.write_long(sp, 0); // actionProc
    bus.write_word(sp + 4, axis as u16);
    bus.write_long(sp + 6, slop_rect_ptr);
    bus.write_long(sp + 10, limit_rect_ptr);
    bus.write_word(sp + 14, start.0 as u16);
    bus.write_word(sp + 16, start.1 as u16);
    bus.write_long(sp + 18, 0x300000); // theRgn
    bus.write_long(sp + 22, 0xDEAD_BEEF);
}

fn write_test_rect(bus: &mut crate::memory::MacMemoryBus, ptr: u32, rect: (i16, i16, i16, i16)) {
    bus.write_word(ptr, rect.0 as u16);
    bus.write_word(ptr + 2, rect.1 as u16);
    bus.write_word(ptr + 4, rect.2 as u16);
    bus.write_word(ptr + 6, rect.3 as u16);
}

// IM:I I-302 + IM:I I-91: DragTheRgn is the custom-outline alias of
// DragGrayRgn, and the outside-slop path returns $80008000.
#[test]
fn dragthergn_returns_no_drag_sentinel_outside_sloprect_and_consumes_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 22;
    cpu.write_reg(Register::A7, sp);
    let limit_rect = 0x240000;
    let slop_rect = 0x240008;
    write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
    write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
    write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
    disp.set_mouse_position(140, 20);

    let result = dispatch(&mut disp, 0x126, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_long(TEST_SP), 0x8000_8000);
}

#[test]
fn dragthergn_returns_current_local_mouse_offset_inside_sloprect() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 22;
    cpu.write_reg(Register::A7, sp);

    let port = 0x210000;
    disp.current_port
        .with_mut(|current_port| *current_port = port);
    bus.write_word(port + 8, (-100i16) as u16);
    bus.write_word(port + 10, (-200i16) as u16);

    let limit_rect = 0x240000;
    let slop_rect = 0x240008;
    write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
    write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
    write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
    disp.set_mouse_position(130, 250); // local (30, 50)

    let result = dispatch(&mut disp, 0x126, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_long(TEST_SP), 0x0014_001E);
}

#[test]
fn dragthergn_pins_to_limitrect_and_honors_axis_constraint() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 22;
    cpu.write_reg(Register::A7, sp);

    let limit_rect = 0x240000;
    let slop_rect = 0x240008;
    write_test_rect(&mut bus, limit_rect, (0, 0, 30, 70));
    write_test_rect(&mut bus, slop_rect, (0, 0, 100, 100));
    write_drag_region_frame(&mut bus, sp, (10, 10), limit_rect, slop_rect, 1);
    disp.set_mouse_position(40, 90);

    let result = dispatch(&mut disp, 0x126, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(
        bus.read_long(TEST_SP),
        0x0000_003B,
        "hAxisOnly keeps vertical offset zero and clamps h to right-1"
    );
}

#[test]
fn dragthergn_uses_low_memory_dragpattern_for_retained_outline() {
    // _DragTheRgn differs from _DragGrayRgn by using the eight-byte
    // DragPattern global. Inside Macintosh Volume III (1985), Appendix D;
    // Macintosh Toolbox Essentials (1992), p. 4-98.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 22;
    cpu.write_reg(Register::A7, sp);
    let limit_rect = 0x240000;
    let slop_rect = 0x240008;
    write_test_rect(&mut bus, limit_rect, (0, 0, 100, 100));
    write_test_rect(&mut bus, slop_rect, (0, 0, 120, 120));
    write_drag_region_frame(&mut bus, sp, (10, 20), limit_rect, slop_rect, 0);
    let region = make_region_handle(&mut bus, 0x300000, 0x300020, 10, (5, 10, 45, 70));
    bus.write_long(sp + 18, region);
    let pattern = [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01];
    for (offset, byte) in pattern.iter().copied().enumerate() {
        bus.write_byte(0x0A34 + offset as u32, byte);
    }

    disp.push_mouse_down(10, 20);
    let result = dispatch(&mut disp, 0x126, &mut cpu, &mut bus);

    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert_eq!(
        disp.region_tracking.as_ref().unwrap().outline_pattern,
        pattern
    );
}

#[test]
fn stationary_drag_region_keeps_outline_until_motion_or_slop_exit() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);
    bus.enable_outline_presentation(disp.screen_mode, [[0; 3]; 256], 4);
    let sp = TEST_SP - 22;
    cpu.write_reg(Register::A7, sp);
    write_test_rect(&mut bus, 0x240000, (0, 0, 100, 100));
    write_test_rect(&mut bus, 0x240008, (0, 0, 120, 120));
    write_drag_region_frame(&mut bus, sp, (10, 20), 0x240000, 0x240008, 0);
    let region = make_region_handle(&mut bus, 0x300000, 0x300020, 10, (5, 10, 45, 70));
    bus.write_long(sp + 18, region);
    disp.push_mouse_down(10, 20);
    dispatch(&mut disp, 0x126, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let mut tracking = disp.region_tracking.take().unwrap();
    assert!(!tracking.outline_saved_pixels.is_empty());
    let epoch = bus.presentation_epoch();
    for _ in 0..100 {
        disp.refresh_region_drag_outline(&mut bus, &mut tracking, (10, 20));
    }
    assert_eq!(
        bus.presentation_epoch(),
        epoch,
        "stationary polling must not repaint"
    );
    disp.refresh_region_drag_outline(&mut bus, &mut tracking, (20, 30));
    assert_eq!(tracking.outline_rect, Some((15, 20, 55, 80)));
    disp.refresh_region_drag_outline(&mut bus, &mut tracking, (121, 30));
    assert_eq!(tracking.outline_rect, None);
    assert!(tracking.outline_saved_pixels.is_empty());
    disp.refresh_region_drag_outline(&mut bus, &mut tracking, (20, 30));
    assert_eq!(tracking.outline_rect, Some((15, 20, 55, 80)));
    assert!(!tracking.outline_saved_pixels.is_empty());
}

// IM:I p.I-294 (signature/call-frame summary on p.I-91):
// TrackGoAway returns TRUE only when mouse-up lands inside the go-away
// box; an invalid/no-tracking call returns FALSE and still consumes args.
#[test]
fn trackgoaway_returns_false_and_consumes_window_and_point_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x0012_0034); // thePt
    bus.write_long(sp + 4, 0x300000); // theWindow
    bus.write_word(sp + 8, 0xFFFF); // result sentinel

    let result = dispatch(&mut disp, 0x11E, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_word(TEST_SP), 0);
}

#[test]
fn trackgoaway_retains_until_mouseup_and_returns_true_inside_close_box() {
    // MTE 1992 pp. 4-103 to 4-104: TrackGoAway keeps control while the
    // button is held, highlights inside the region, removes highlighting
    // at release, and returns TRUE for an inside release.
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        disp.screen_mode.0,
        100,
        100,
        260,
        360,
        "Document",
        0,
        true,
        false,
        true,
        0,
    );
    disp.front_window = window;
    disp.window_list.replace(vec![window]);
    bus.write_byte(
        window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 85);
    bus.write_word(sp + 2, 105);
    bus.write_long(sp + 4, window);
    bus.write_word(sp + 8, 0xDEAD);
    disp.push_mouse_down(85, 105);
    disp.event_queue.pop_front(); // application already received mouseDown

    dispatch(&mut disp, 0x11E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert!(disp.go_away_tracking.is_some());
    assert!(disp.is_tracking_refire(0xA91E));

    disp.push_mouse_up(85, 105);
    dispatch(&mut disp, 0x11E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    assert_eq!(bus.read_word(sp + 8), 0x0100);
    assert!(disp.go_away_tracking.is_none());
    assert!(disp.event_queue.iter().all(|event| event.what != 2));
}

#[test]
fn trackgoaway_unhighlights_and_returns_false_after_dragging_out() {
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        disp.screen_mode.0,
        100,
        100,
        260,
        360,
        "Document",
        0,
        true,
        false,
        true,
        0,
    );
    disp.front_window = window;
    disp.window_list.replace(vec![window]);
    bus.write_byte(
        window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 85);
    bus.write_word(sp + 2, 105);
    bus.write_long(sp + 4, window);
    disp.push_mouse_down(85, 105);
    disp.event_queue.pop_front();
    dispatch(&mut disp, 0x11E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    disp.set_mouse_position(130, 200);
    dispatch(&mut disp, 0x11E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(!disp.go_away_tracking.as_ref().unwrap().highlighted);

    disp.push_mouse_up(130, 200);
    dispatch(&mut disp, 0x11E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 8);
    assert_eq!(bus.read_word(sp + 8), 0);
}

// ---------------------------------------------------------------
// 23. GrowWindow (0x12B) -- pops to SP+12, writes 0 at SP+12
// ---------------------------------------------------------------
#[test]
fn growwindow_returns_zero_when_size_is_unchanged() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 12;
    cpu.write_reg(Register::A7, sp);
    for i in 0..12u32 {
        bus.write_byte(sp + i, 0);
    }
    // Write non-zero at result position
    bus.write_long(sp + 12, 0xFFFFFFFF);

    let result = dispatch(&mut disp, 0x12B, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    let grow_result = bus.read_long(TEST_SP);
    assert_eq!(grow_result, 0, "GrowWindow should return 0");
}

#[test]
fn growwindow_retains_clamps_and_returns_release_dimensions() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window: u32 = 0x250000;
    setup_window_with_regions(&mut bus, window, 0, 0, 100, 200);
    bus.write_word(window + 6, 0);
    bus.write_word(window + 8, (-40i16) as u16);
    bus.write_word(window + 10, (-20i16) as u16);
    bus.write_word(window + 12, 560);
    bus.write_word(window + 14, 780);
    bus.write_byte(window + 110, 0xFF);
    disp.window_list.replace(vec![window]);
    disp.front_window = window;
    disp.window_bounds = (40, 20, 140, 220);

    let size_rect = 0x280000;
    bus.write_word(size_rect, 50);
    bus.write_word(size_rect + 2, 80);
    bus.write_word(size_rect + 4, 180);
    bus.write_word(size_rect + 6, 260);
    let sp = TEST_SP - 12;
    cpu.write_reg(Register::A7, sp);
    cpu.write_reg(Register::A0, 0x1111_2222);
    cpu.write_reg(Register::A1, 0x3333_4444);
    cpu.write_reg(Register::D1, 0x5555_6666);
    bus.write_long(sp, size_rect);
    // The event point is inside the box; the live cursor has already moved.
    bus.write_long(sp + 4, (135u32 << 16) | 210);
    bus.write_long(sp + 8, window);
    bus.write_long(sp + 12, 0xDEAD_BEEF);
    disp.push_mouse_down(140, 220);
    disp.event_queue.pop_front();
    assert!(disp.window_visible(&bus, window));
    assert!(disp.window_tracking_button_down(&bus));
    let (screen_base, row_bytes, _, screen_height, _) = disp.screen_mode;
    let screen_len = row_bytes * u32::from(screen_height);
    let original_screen: Vec<u8> = (0..screen_len)
        .map(|offset| bus.read_byte(screen_base + offset))
        .collect();

    dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert!(disp.grow_window_tracking.is_some());
    assert!(disp.is_tracking_refire(0xA92B));
    assert!(disp.is_tracking_refire(0xAD2B));
    assert!(!disp.is_tracking_refire(0xA925));

    disp.set_mouse_position(400, 600);
    dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let tracking = disp.grow_window_tracking.as_ref().unwrap();
    let old_height = tracking.original_content_rect.2 - tracking.original_content_rect.0;
    let old_width = tracking.original_content_rect.3 - tracking.original_content_rect.1;
    assert_eq!(
        tracking.outline_rect.2 - tracking.original_outline_rect.2,
        180 - old_height
    );
    assert_eq!(
        tracking.outline_rect.3 - tracking.original_outline_rect.3,
        260 - old_width
    );
    assert!(!tracking.outline_saved_pixels.is_empty());

    disp.push_mouse_up(190, 300);
    dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert_eq!(bus.read_long(sp + 12), (155u32 << 16) | 260);
    assert!(disp.grow_window_tracking.is_none());
    assert!(disp.event_queue.iter().all(|event| event.what != 2));
    assert_eq!(cpu.read_reg(Register::A0), 0x1111_2222);
    assert_eq!(cpu.read_reg(Register::A1), 0x3333_4444);
    assert_eq!(cpu.read_reg(Register::D1), 0x5555_6666);
    assert_eq!(disp.window_bounds, (40, 20, 140, 220));
    assert_eq!(
        (0..screen_len)
            .map(|offset| bus.read_byte(screen_base + offset))
            .collect::<Vec<_>>(),
        original_screen
    );
}

#[test]
fn growwindow_restores_framebuffer_bytes_at_supported_depths() {
    for depth in [1u16, 2, 4, 8] {
        let (mut disp, mut cpu, mut bus) = setup();
        let screen_base = 0x340000;
        let screen_width = 320u16;
        let screen_height = 240u16;
        let row_bytes = (u32::from(screen_width) * u32::from(depth)).div_ceil(8);
        disp.set_screen_mode_for_test(screen_base, row_bytes, screen_width, screen_height, depth);
        bus.write_long(0x0824, screen_base);
        bus.write_word(0x0828, row_bytes as u16);
        let screen_len = row_bytes * u32::from(screen_height);
        for offset in 0..screen_len {
            bus.write_byte(
                screen_base + offset,
                (offset as u8).wrapping_mul(37).wrapping_add(depth as u8),
            );
        }
        let original_screen: Vec<u8> = (0..screen_len)
            .map(|offset| bus.read_byte(screen_base + offset))
            .collect();

        let window = 0x250000;
        setup_window_with_regions(&mut bus, window, 0, 0, 100, 200);
        bus.write_word(window + 6, 0);
        bus.write_word(window + 8, (-40i16) as u16);
        bus.write_word(window + 10, (-20i16) as u16);
        bus.write_word(window + 12, 200);
        bus.write_word(window + 14, 300);
        bus.write_byte(window + 110, 0xFF);
        disp.window_list.replace(vec![window]);
        disp.front_window = window;

        let size_rect = 0x280000;
        bus.write_word(size_rect, 50);
        bus.write_word(size_rect + 2, 80);
        bus.write_word(size_rect + 4, 180);
        bus.write_word(size_rect + 6, 260);
        let sp = TEST_SP - 12;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, size_rect);
        bus.write_long(sp + 4, 0x008C_00DC);
        bus.write_long(sp + 8, window);

        disp.push_mouse_down(140, 220);
        disp.event_queue.pop_front();
        dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.set_mouse_position(170, 260);
        dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        disp.push_mouse_up(170, 260);
        dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();

        assert_eq!(
            (0..screen_len)
                .map(|offset| bus.read_byte(screen_base + offset))
                .collect::<Vec<_>>(),
            original_screen,
            "GrowWindow must restore every framebuffer byte at {depth}bpp"
        );
    }
}

#[test]
fn growwindow_returns_zero_for_unchanged_size() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x250000;
    setup_window_with_regions(&mut bus, window, 0, 0, 100, 200);
    bus.write_word(window + 6, 0);
    bus.write_word(window + 8, (-40i16) as u16);
    bus.write_word(window + 10, (-20i16) as u16);
    bus.write_word(window + 12, 560);
    bus.write_word(window + 14, 780);
    bus.write_byte(window + 110, 0xFF);
    disp.window_list.replace(vec![window]);

    let size_rect = 0x280000;
    bus.write_word(size_rect, 50);
    bus.write_word(size_rect + 2, 80);
    bus.write_word(size_rect + 4, 180);
    bus.write_word(size_rect + 6, 260);
    let sp = TEST_SP - 12;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, size_rect);
    bus.write_long(sp + 4, 0x008C_00DC);
    bus.write_long(sp + 8, window);

    disp.push_mouse_down(140, 220);
    disp.event_queue.pop_front();
    dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    disp.push_mouse_up(140, 220);
    dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(sp + 12), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert!(disp.grow_window_tracking.is_none());
}

#[test]
fn growwindow_cancels_if_window_is_disposed_while_tracking() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x250000;
    setup_window_with_regions(&mut bus, window, 0, 0, 100, 200);
    bus.write_word(window + 6, 0);
    bus.write_byte(window + 110, 0xFF);
    disp.window_list.replace(vec![window]);

    let size_rect = 0x280000;
    bus.write_word(size_rect, 50);
    bus.write_word(size_rect + 2, 80);
    bus.write_word(size_rect + 4, 180);
    bus.write_word(size_rect + 6, 260);
    let sp = TEST_SP - 12;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, size_rect);
    bus.write_long(sp + 4, 0x0064_00C8);
    bus.write_long(sp + 8, window);

    disp.push_mouse_down(100, 200);
    disp.event_queue.pop_front();
    dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(disp.grow_window_tracking.is_some());

    disp.window_list.clear();
    dispatch(&mut disp, 0x12B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(sp + 12), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp + 12);
    assert!(disp.grow_window_tracking.is_none());
}

// IM:IV IV-50: TrackBox returns FALSE when mouse-up is outside the
// zoom box.
#[test]
fn trackbox_returns_false_when_zoom_tracking_does_not_end_inside_zoom_box() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0xFFFF); // partCode (inZoomOut)
    bus.write_long(sp + 2, 0x0012_0034); // thePt (global Point)
    bus.write_long(sp + 6, 0x200000); // theWindow
    bus.write_word(sp + 10, 0xFFFF); // function result placeholder

    let result = dispatch(&mut disp, 0x03B, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(
        bus.read_word(TEST_SP),
        0,
        "TrackBox should return FALSE in the result slot"
    );
}

// IM:IV IV-50 + IM:I I-90..I-91: TrackBox is stack-based and returns a
// BOOLEAN in the function-result slot after consuming its arguments.
#[test]
fn trackbox_consumes_windowptr_point_partcode_and_returns_boolean_on_stack() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 7); // partCode (inZoomIn)
    bus.write_long(sp + 2, 0x0008_0010); // thePt
    bus.write_long(sp + 6, 0x210000); // theWindow
    bus.write_word(sp + 10, 0x1234);

    let result = dispatch(&mut disp, 0x03B, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "TrackBox should pop 10 bytes of arguments"
    );
    assert_eq!(
        bus.read_word(TEST_SP),
        0,
        "TrackBox should write BOOLEAN FALSE to the result slot"
    );
}

// MTE 1992 pp. 4-53..4-54: the standard zoom WDEF creates WStateData
// and marks spareFlag when it initializes the window record.
#[test]
fn standard_zoom_window_creation_installs_wstate_data() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = bus.alloc(256);
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    let (_, _, screen_width, screen_height, _) = disp.screen_mode;

    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        window,
        disp.screen_mode.0,
        50,
        10,
        530,
        650,
        "Zoomable",
        12,
        true,
        false,
        true,
        0,
    );

    assert_eq!(
        bus.read_byte(window + super::super::TrapDispatcher::WINDOW_SPARE_FLAG_OFFSET),
        0xFF,
        "zoomNoGrow must set the WindowRecord zoom flag"
    );
    let state_handle =
        bus.read_long(window + super::super::TrapDispatcher::WINDOW_DATA_HANDLE_OFFSET);
    assert_ne!(state_handle, 0, "zoomNoGrow must allocate WStateData");
    let state = bus.read_long(state_handle);
    assert_ne!(state, 0, "WStateData handle must be dereferenceable");
    assert_eq!(
        (
            bus.read_word(state) as i16,
            bus.read_word(state + 2) as i16,
            bus.read_word(state + 4) as i16,
            bus.read_word(state + 6) as i16,
        ),
        (50, 10, 530, 650),
        "userState must begin at the requested global bounds"
    );
    assert_eq!(
        (
            bus.read_word(state + 8) as i16,
            bus.read_word(state + 10) as i16,
            bus.read_word(state + 12) as i16,
            bus.read_word(state + 14) as i16,
        ),
        (23, 3, screen_height as i16 - 3, screen_width as i16 - 3),
        "stdState must default to the gray region inset by three pixels"
    );
    bus.write_byte(
        window + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );
    assert_eq!(
        disp.standard_zoom_rect(&bus, window),
        Some((32, 635, 50, 650)),
        "the zoom hit region must share the rightmost scrollbar column"
    );
}

// IM:IV IV-50: partCode inZoomIn (7) chooses userState from WStateData.
#[test]
fn zoomwindow_inzoomin_uses_userstate_rect_from_wstatedata() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x300000u32;
    let front_window = 0x300100u32;
    setup_full_window_with_regions(&mut bus, window, 0, 0, 20, 20);
    install_wstate_data(&mut bus, window, (30, 40, 130, 190), (10, 12, 50, 70));
    bus.write_byte(window + 110, 0xFF);
    bus.write_byte(front_window + 110, 0xFF);
    disp.window_list.replace(vec![front_window, window]);
    disp.front_window = front_window;

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 0); // front = FALSE
    bus.write_word(sp + 2, 7); // inZoomIn
    bus.write_long(sp + 4, window);

    let result = dispatch(&mut disp, 0x03A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(bus.read_word(window + 16) as i16, 0, "portRect.top");
    assert_eq!(bus.read_word(window + 18) as i16, 0, "portRect.left");
    assert_eq!(bus.read_word(window + 20) as i16, 100, "portRect.bottom");
    assert_eq!(bus.read_word(window + 22) as i16, 150, "portRect.right");

    let cont_ptr = bus.read_long(bus.read_long(window + 118));
    assert_eq!(bus.read_word(cont_ptr + 2) as i16, 30, "contRgn.top");
    assert_eq!(bus.read_word(cont_ptr + 4) as i16, 40, "contRgn.left");
    assert_eq!(bus.read_word(cont_ptr + 6) as i16, 130, "contRgn.bottom");
    assert_eq!(bus.read_word(cont_ptr + 8) as i16, 190, "contRgn.right");
    assert_eq!(
        disp.front_window, front_window,
        "front=FALSE must preserve current front window"
    );
}

// IM:IV IV-50: partCode inZoomOut (8) chooses stdState from WStateData.
#[test]
fn zoomwindow_inzoomout_uses_stdstate_rect_from_wstatedata() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x300000u32;
    setup_full_window_with_regions(&mut bus, window, 0, 0, 20, 20);
    install_wstate_data(&mut bus, window, (12, 18, 52, 90), (50, 60, 170, 250));
    bus.write_byte(window + 110, 0xFF);
    disp.window_list.replace(vec![window]);
    disp.front_window = window;

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 0); // front = FALSE
    bus.write_word(sp + 2, 8); // inZoomOut
    bus.write_long(sp + 4, window);

    let result = dispatch(&mut disp, 0x03A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(bus.read_word(window + 16) as i16, 0, "portRect.top");
    assert_eq!(bus.read_word(window + 18) as i16, 0, "portRect.left");
    assert_eq!(bus.read_word(window + 20) as i16, 120, "portRect.bottom");
    assert_eq!(bus.read_word(window + 22) as i16, 190, "portRect.right");

    let cont_ptr = bus.read_long(bus.read_long(window + 118));
    assert_eq!(bus.read_word(cont_ptr + 2) as i16, 50, "contRgn.top");
    assert_eq!(bus.read_word(cont_ptr + 4) as i16, 60, "contRgn.left");
    assert_eq!(bus.read_word(cont_ptr + 6) as i16, 170, "contRgn.bottom");
    assert_eq!(bus.read_word(cont_ptr + 8) as i16, 250, "contRgn.right");
}

// IM:IV IV-50: front=TRUE brings the zoomed window to the front.
#[test]
fn zoomwindow_front_true_brings_window_to_front() {
    let (mut disp, mut cpu, mut bus) = setup();
    let old_front = 0x200040u32;
    let zoom_target = 0x200140u32;
    setup_full_window_with_regions(&mut bus, zoom_target, 0, 0, 20, 20);
    install_wstate_data(
        &mut bus,
        zoom_target,
        (20, 20, 120, 180),
        (40, 50, 140, 210),
    );
    bus.write_byte(old_front + 110, 0xFF);
    bus.write_byte(zoom_target + 110, 0xFF);
    bus.write_byte(old_front + 111, 0xFF);
    bus.write_byte(zoom_target + 111, 0x00);

    disp.window_list.replace(vec![old_front, zoom_target]);
    disp.front_window = old_front;

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1); // front = TRUE
    bus.write_word(sp + 2, 8); // inZoomOut
    bus.write_long(sp + 4, zoom_target);

    let result = dispatch(&mut disp, 0x03A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(
        disp.front_window, zoom_target,
        "front=TRUE must promote target"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|e| e.what == 8 && e.message == old_front && (e.modifiers & 1) == 0),
        "old front must receive deactivate event"
    );
    assert!(
        disp.event_queue
            .iter()
            .any(|e| e.what == 8 && e.message == zoom_target && (e.modifiers & 1) == 1),
        "new front must receive activate event"
    );
}

// ---------------------------------------------------------------
// 24. InvalRect (0x128) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_inval_rect() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x300000); // rect ptr

    let result = dispatch(&mut disp, 0x128, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// ---------------------------------------------------------------
// 25. ValidRect (0x12A) -- pops 4 bytes
// ---------------------------------------------------------------
#[test]
fn test_valid_rect() {
    let (mut disp, mut cpu, mut bus) = setup();

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x300000); // rect ptr

    let result = dispatch(&mut disp, 0x12A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// ---------------------------------------------------------------
// PaintBehind (0x10D) — inval-rects the clobbered region on every
// window at/behind startWindow.
// ---------------------------------------------------------------

#[test]
fn paintone_consumes_window_and_clobberedrgn_arguments() {
    // PaintOne takes WindowPeek plus RgnHandle arguments.
    // Inside Macintosh Volume I (1985), p. I-296;
    // Macintosh Toolbox Essentials (1992), p. 4-118.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_long(sp + 4, 0);

    let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn paintone_nil_clobberedrgn_invalidates_window_portrect() {
    // With NIL clobberedRgn, PaintOne repaints the whole window.
    // Inside Macintosh Volume I (1985), p. I-296;
    // Macintosh Toolbox Essentials (1992), p. 4-118.
    let (mut disp, mut cpu, mut bus) = setup();
    let win = 0x200040u32;
    let (_cont_rgn, update_rgn) = setup_full_window_with_regions(&mut bus, win, 10, 20, 50, 100);
    disp.window_list.replace(vec![win]);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0); // NIL clobberedRgn
    bus.write_long(sp + 4, win); // window

    let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_word(update_rgn + 2) as i16, 10, "updateRgn.top");
    assert_eq!(bus.read_word(update_rgn + 4) as i16, 20, "updateRgn.left");
    assert_eq!(bus.read_word(update_rgn + 6) as i16, 50, "updateRgn.bottom");
    assert_eq!(bus.read_word(update_rgn + 8) as i16, 100, "updateRgn.right");
    assert!(
        disp.event_queue
            .iter()
            .any(|event| event.what == 6 && event.message == win),
        "PaintOne should queue an update event for the target window"
    );
}

#[test]
fn paintone_erases_exposed_content_before_queuing_update() {
    // PaintOne erases exposed content with the background pattern and
    // adds it to the update region.
    // Inside Macintosh Volume I (1985), p. I-296.
    let (mut disp, mut cpu, mut bus) = setup();
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
    disp.screen_mode = (screen_base, 800, 800, 600, 8);

    let win = bus.alloc(256);
    let (_cont_rgn, update_rgn) = setup_full_window_with_regions(&mut bus, win, 10, 20, 50, 100);
    bus.write_byte(win + 110u32, 0xFF);
    disp.window_list.replace(vec![win]);
    disp.front_window = win;

    let probe = screen_base + 30 * 800 + 50;
    bus.write_byte(probe, 0x7B);

    let clobbered_ptr = bus.alloc(10);
    let clobbered_handle = bus.alloc(4);
    bus.write_long(clobbered_handle, clobbered_ptr);
    bus.write_word(clobbered_ptr, 10);
    bus.write_word(clobbered_ptr + 2, 15);
    bus.write_word(clobbered_ptr + 4, 25);
    bus.write_word(clobbered_ptr + 6, 30);
    bus.write_word(clobbered_ptr + 8, 60);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, clobbered_handle);
    bus.write_long(sp + 4, win);

    let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_ne!(
        bus.read_byte(probe),
        0x7B,
        "PaintOne must erase exposed content before the application redraws it"
    );
    assert_eq!(bus.read_word(update_rgn + 2) as i16, 15, "updateRgn.top");
    assert_eq!(bus.read_word(update_rgn + 4) as i16, 25, "updateRgn.left");
    assert_eq!(bus.read_word(update_rgn + 6) as i16, 30, "updateRgn.bottom");
    assert_eq!(bus.read_word(update_rgn + 8) as i16, 60, "updateRgn.right");
}

#[test]
fn paintone_empty_clobberedrgn_invalidates_window_portrect() {
    // An empty clobberedRgn currently falls back to the window's
    // portRect path, so CheckUpdate returns TRUE and writes the
    // target window into the event record.
    // Inside Macintosh Volume I (1985), p. I-296;
    // Macintosh Toolbox Essentials (1992), p. 4-118.
    let (mut disp, mut cpu, mut bus) = setup();
    let win = 0x200040u32;
    let (_cont_rgn, _update_rgn) = setup_full_window_with_regions(&mut bus, win, 10, 20, 50, 100);
    disp.window_list.replace(vec![win]);

    let empty_rgn = bus.alloc(10);
    let empty_handle = bus.alloc(4);
    bus.write_word(empty_rgn, 10);
    bus.write_word(empty_rgn + 2, 0);
    bus.write_word(empty_rgn + 4, 0);
    bus.write_word(empty_rgn + 6, 0);
    bus.write_word(empty_rgn + 8, 0);
    bus.write_long(empty_handle, empty_rgn);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, empty_handle);
    bus.write_long(sp + 4, win);

    let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    let event_ptr = bus.alloc(16);
    for i in 0..16 {
        bus.write_byte(event_ptr + i, 0xCC);
    }
    let sp2 = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp2);
    bus.write_long(sp2, event_ptr);
    bus.write_word(sp2 + 4, 0xFFFF);
    let check = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(check.is_some());
    assert!(check.unwrap().is_ok());
    assert_eq!(bus.read_word(TEST_SP - 2), 0x0100, "result should be TRUE");
    assert_eq!(
        bus.read_word(event_ptr),
        6,
        "event.what should be updateEvt"
    );
    assert_eq!(
        bus.read_long(event_ptr + 2),
        win,
        "event.message should carry WindowPtr"
    );
    assert!(
        disp.event_queue.is_empty(),
        "CheckUpdate should dequeue the consumed update event"
    );
}

#[test]
fn paint_one_zero_window_is_noop() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_long(sp + 4, 0); // NIL theWindow
    let result = dispatch(&mut disp, 0x10C, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn paintbehind_consumes_startwindow_and_clobberedrgn_arguments() {
    // Inside Macintosh Volume I (1985), p. I-293:
    // PaintBehind(startWindow, clobberedRgn) consumes two pointer args.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0); // NIL clobbered region
    bus.write_long(sp + 4, 0); // NIL startWindow
    let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn paintbehind_updates_startwindow_and_skips_invisible_windows_behind_it() {
    // Inside Macintosh Volume I (1985), p. I-293; Macintosh Toolbox
    // Essentials (1992), p. 4-118: PaintBehind repaints startWindow and
    // visible windows behind it.
    let (mut disp, mut cpu, mut bus) = setup();
    let front = bus.alloc(256);
    let middle = bus.alloc(256);
    let back = bus.alloc(256);

    fn setup_paintbehind_window(
        bus: &mut crate::memory::MacMemoryBus,
        window: u32,
        rect: (i16, i16, i16, i16),
    ) {
        bus.write_word(window + 16, rect.0 as u16);
        bus.write_word(window + 18, rect.1 as u16);
        bus.write_word(window + 20, rect.2 as u16);
        bus.write_word(window + 22, rect.3 as u16);
        let cont_rgn = super::super::TrapDispatcher::alloc_rect_region_handle(bus, Some(rect));
        let update_rgn = super::super::TrapDispatcher::alloc_rect_region_handle(bus, None);
        bus.write_long(window + 118, cont_rgn);
        bus.write_long(window + 122, update_rgn);
        bus.write_byte(window + 110, 0xFF);
    }

    for window in [front, middle, back] {
        setup_paintbehind_window(&mut bus, window, (0, 0, 200, 200));
    }
    bus.write_byte(back + 110u32, 0x00);
    disp.window_list.replace(vec![front, middle, back]);
    disp.front_window = front;

    let clobbered_ptr = bus.alloc(10);
    let clobbered_handle = bus.alloc(4);
    bus.write_long(clobbered_handle, clobbered_ptr);
    bus.write_word(clobbered_ptr, 10);
    bus.write_word(clobbered_ptr + 2, 50);
    bus.write_word(clobbered_ptr + 4, 60);
    bus.write_word(clobbered_ptr + 6, 120);
    bus.write_word(clobbered_ptr + 8, 130);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, clobbered_handle);
    bus.write_long(sp + 4, middle);

    let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(
        super::super::TrapDispatcher::region_handle_rect(&bus, bus.read_long(front + 122)),
        None,
        "front window should remain untouched"
    );
    assert_eq!(
        super::super::TrapDispatcher::region_handle_rect(&bus, bus.read_long(middle + 122)),
        Some((50, 60, 120, 130)),
        "startWindow should be invalidated"
    );
    assert_eq!(
        super::super::TrapDispatcher::region_handle_rect(&bus, bus.read_long(back + 122)),
        None,
        "hidden windows behind startWindow should be skipped"
    );
}

#[test]
fn paintbehind_converts_global_clobbered_rect_to_back_window_local_update() {
    // PaintBehind receives a Window Manager clobbered region in global
    // desktop coordinates. Back-window update regions are local to that
    // window's port; forwarding the global bbox directly over-invalidates
    // document windows whose origin is not (0,0).
    let (mut disp, mut cpu, mut bus) = setup();
    bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
    let screen_base = bus.alloc(800 * 600);
    bus.write_long(0x0824, screen_base);
    disp.set_screen_mode_for_test(screen_base, 800, 800, 600, 8);

    let main_window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        main_window,
        screen_base,
        99,
        86,
        538,
        713,
        "Main",
        4,
        true,
        false,
        false,
        0,
    );
    disp.validate_window_rect(&mut bus, main_window, (0, 0, 439, 627));

    let overlay_window = bus.alloc(256);
    disp.init_cgraf_window(
        &mut bus,
        &mut cpu,
        overlay_window,
        screen_base,
        271,
        283,
        348,
        517,
        "Overlay",
        1,
        true,
        false,
        true,
        0,
    );
    disp.validate_window_rect(&mut bus, overlay_window, (0, 0, 77, 234));
    assert_eq!(disp.window_list, vec![overlay_window, main_window]);

    let clobbered_handle = super::super::TrapDispatcher::alloc_rect_region_handle(
        &mut bus,
        Some((271, 283, 348, 517)),
    );
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, clobbered_handle);
    bus.write_long(sp + 4, overlay_window);

    let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    assert_eq!(
        super::super::TrapDispatcher::region_handle_rect(&bus, bus.read_long(main_window + 122),),
        Some((271, 283, 348, 517)),
        "PaintBehind must store the overlay's invalidated bounds in global updateRgn coordinates"
    );
}

#[test]
fn paint_behind_nil_start_walks_whole_list() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]);
    // Minimum: a 10-byte rect region at 0x300020.
    let rgn_ptr = 0x300020u32;
    bus.write_word(rgn_ptr, 10);
    bus.write_word(rgn_ptr + 2, 5);
    bus.write_word(rgn_ptr + 4, 5);
    bus.write_word(rgn_ptr + 6, 30);
    bus.write_word(rgn_ptr + 8, 40);
    let rgn_handle = 0x300000u32;
    bus.write_long(rgn_handle, rgn_ptr);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, rgn_handle);
    bus.write_long(sp + 4, 0); // NIL startWindow
    let result = dispatch(&mut disp, 0x10D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn hilitewindow_consumes_windowptr_and_boolean_arguments() {
    // HiliteWindow takes one WindowPtr and one Boolean argument.
    // Inside Macintosh Volume I (1985), p. I-286;
    // Macintosh Toolbox Essentials (1992), p. 4-90.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1);
    bus.write_byte(sp + 1, 0x7F);
    bus.write_long(sp + 2, 0x200040);

    let result = dispatch(&mut disp, 0x11C, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn hilitewindow_sets_or_clears_hilited_state_byte() {
    // HiliteWindow sets or clears a window's highlighted state.
    // Inside Macintosh Volume I (1985), p. I-286;
    // Macintosh Toolbox Essentials (1992), p. 4-90.
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x200040u32;
    bus.write_byte(window + 110, 0x00); // hidden => skip chrome drawing
    bus.write_byte(window + 111, 0x11);

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1);
    bus.write_long(sp + 2, window);
    let result = dispatch(&mut disp, 0x11C, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(
        bus.read_byte(window + 111),
        0xFF,
        "window should be highlighted"
    );

    let sp2 = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp2);
    bus.write_byte(sp2, 0);
    bus.write_long(sp2 + 2, window);
    let result2 = dispatch(&mut disp, 0x11C, &mut cpu, &mut bus);
    assert!(result2.is_some());
    assert!(result2.unwrap().is_ok());
    assert_eq!(
        bus.read_byte(window + 111),
        0x00,
        "window should be unhighlighted"
    );
}

#[test]
fn bringtofront_consumes_windowptr_argument() {
    // BringToFront takes one WindowPtr argument.
    // Inside Macintosh Volume I (1985), p. I-286;
    // Macintosh Toolbox Essentials (1992), p. 4-90.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x200040);

    let result = dispatch(&mut disp, 0x120, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn bringtofront_repaints_exposed_content_and_preserves_visible_pixels() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen = bus.alloc(800 * 600);
    bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen);
    disp.set_screen_mode_for_test(screen, 800, 800, 600, 8);
    let target = bus.alloc(256);
    let front = bus.alloc(256);
    for (window, top, left, bottom, right) in
        [(target, 100, 100, 300, 400), (front, 150, 200, 350, 500)]
    {
        disp.init_cgraf_window(
            &mut bus, &mut cpu, window, screen, top, left, bottom, right, "Window", 4, true, false,
            true, 0,
        );
    }
    disp.window_list.replace(vec![front, target]);
    disp.front_window = front;
    disp.recalculate_window_vis_regions(&mut bus);
    disp.set_current_port_state(&mut bus, &mut cpu, front, None);
    let saved_device = *disp.current_gdevice;
    let saved_clip = bus.read_long(target + 28);
    // Window Manager repainting must not inherit the application's clip.
    super::super::TrapDispatcher::write_region_handle_rect(&mut bus, saved_clip, None);
    let exposed = screen + 200 * 800 + 220;
    let already_visible = screen + 120 * 800 + 120;
    let outside_target = screen + 250 * 800 + 450;
    for pixel in [exposed, already_visible, outside_target] {
        bus.write_byte(pixel, 0x7B);
    }
    cpu.write_reg(Register::A7, TEST_SP - 4);
    bus.write_long(TEST_SP - 4, target);
    dispatch(&mut disp, 0x120, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_byte(exposed), 0, "erase newly exposed content");
    assert_eq!(bus.read_byte(already_visible), 0x7B);
    assert_eq!(bus.read_byte(outside_target), 0x7B);
    assert_eq!(bus.read_long(target + 28), saved_clip);
    assert_eq!(*disp.current_port, front);
    assert_eq!(*disp.current_gdevice, saved_device);
    assert_eq!(disp.front_window, front, "preserve activation");
    assert_eq!(disp.window_list.first(), Some(target));
    assert!(disp.window_has_pending_update(&bus, target));
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn sendbehind_raises_below_a_palette_and_repaints_only_exposed_content() {
    let (mut disp, mut cpu, mut bus) = setup();
    let screen = bus.alloc(800 * 600);
    disp.set_screen_mode_for_test(screen, 800, 800, 600, 8);
    let target = bus.alloc(256);
    let front = bus.alloc(256);
    let palette = bus.alloc(256);
    for (window, top, left, bottom, right) in [
        (target, 100, 100, 300, 400),
        (front, 150, 200, 350, 500),
        (palette, 40, 250, 180, 300),
    ] {
        disp.init_cgraf_window(
            &mut bus, &mut cpu, window, screen, top, left, bottom, right, "Window", 4, true, false,
            true, 0,
        );
        disp.validate_window_rect(&mut bus, window, (0, 0, 600, 800));
    }
    disp.window_list.replace(vec![palette, front, target]);
    disp.front_window = front;
    disp.recalculate_window_vis_regions(&mut bus);
    // An application's temporary viewport is not the old stacking geometry.
    let vis = bus.read_long(target + 24);
    super::super::TrapDispatcher::write_region_handle_rect(&mut bus, vis, Some((0, 0, 1, 1)));
    let exposed = screen + 200 * 800 + 220;
    let already_visible = screen + 120 * 800 + 120;
    let covered_by_palette = screen + 160 * 800 + 270;
    for pixel in [exposed, already_visible, covered_by_palette] {
        bus.write_byte(pixel, 0x7B);
    }
    cpu.write_reg(Register::A7, TEST_SP - 8);
    bus.write_long(TEST_SP - 8, palette);
    bus.write_long(TEST_SP - 4, target);
    dispatch(&mut disp, 0x121, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(disp.window_list, vec![palette, target, front]);
    assert_eq!(bus.read_byte(exposed), 0);
    assert_eq!(bus.read_byte(already_visible), 0x7B);
    assert_eq!(bus.read_byte(covered_by_palette), 0x7B);
    assert!(disp.window_has_pending_update(&bus, target));
    assert_eq!(disp.front_window, front);
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn bringtofront_moves_window_to_front_of_window_list() {
    // BringToFront moves the target window to the beginning of the window list.
    // Inside Macintosh Volume I (1985), p. I-286;
    // Macintosh Toolbox Essentials (1992), p. 4-90.
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    let win_c = 0x200240u32;
    disp.window_list.replace(vec![win_a, win_b, win_c]);
    disp.front_window = win_a;
    bus.write_byte(
        win_a + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0xFF,
    );
    bus.write_byte(
        win_c + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET,
        0x00,
    );

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_c);

    let result = dispatch(&mut disp, 0x120, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(disp.window_list, vec![win_c, win_a, win_b]);
    assert_eq!(
        disp.front_window, win_a,
        "BringToFront must not change the active window"
    );
    assert_eq!(
        bus.read_byte(win_a + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0xFF,
        "BringToFront must not unhighlight the active window"
    );
    assert_eq!(
        bus.read_byte(win_c + super::super::TrapDispatcher::WINDOW_HILITED_OFFSET),
        0x00,
        "BringToFront must not highlight the reordered window"
    );
}

#[test]
fn setwindowpic_consumes_windowptr_and_pichandle_arguments() {
    // SetWindowPic takes WindowPtr and PicHandle pointer arguments.
    // Inside Macintosh Volume I (1985), p. I-293;
    // Macintosh Toolbox Essentials (1992), p. 4-110.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0x200000);
    bus.write_long(sp + 4, 0x200040);

    let result = dispatch(&mut disp, 0x12E, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn setwindowpic_stores_pic_handle_in_window_record() {
    // SetWindowPic stores a picture handle for later window-content drawing.
    // Inside Macintosh Volume I (1985), p. I-293;
    // Macintosh Toolbox Essentials (1992), p. 4-110.
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x200040u32;
    let pic = 0x300040u32;

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, pic);
    bus.write_long(sp + 4, window);

    let result = dispatch(&mut disp, 0x12E, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(bus.read_long(window + 148), pic);
}

#[test]
fn getwindowpic_consumes_windowptr_argument_and_writes_function_result_slot() {
    // GetWindowPic takes one WindowPtr argument and returns a PicHandle.
    // Inside Macintosh Volume I (1985), p. I-293;
    // Macintosh Toolbox Essentials (1992), p. 4-110.
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x200040u32;
    let expected_pic = 0x300080u32;
    bus.write_long(window + 148, expected_pic);

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window);
    bus.write_long(sp + 4, 0xFFFF_FFFF);

    let result = dispatch(&mut disp, 0x12F, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_long(TEST_SP), expected_pic);
}

#[test]
fn getwindowpic_returns_picture_handle_previously_set_by_setwindowpic() {
    // GetWindowPic returns the handle previously stored by SetWindowPic.
    // Inside Macintosh Volume I (1985), p. I-293;
    // Macintosh Toolbox Essentials (1992), p. 4-110.
    let (mut disp, mut cpu, mut bus) = setup();
    let window = 0x200040u32;
    let pic = 0x300000u32;

    let sp_set = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp_set);
    bus.write_long(sp_set, pic);
    bus.write_long(sp_set + 4, window);
    let set_result = dispatch(&mut disp, 0x12E, &mut cpu, &mut bus);
    assert!(set_result.is_some());
    assert!(set_result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    let sp_get = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp_get);
    bus.write_long(sp_get, window);
    let get_result = dispatch(&mut disp, 0x12F, &mut cpu, &mut bus);
    assert!(get_result.is_some());
    assert!(get_result.unwrap().is_ok());
    assert_eq!(bus.read_long(TEST_SP), pic);
}

// ---------------------------------------------------------------
// SendBehind (0x121) — reorders WindowList and transfers activation only
// when the moved window was active.
// ---------------------------------------------------------------

#[test]
fn sendbehind_consumes_windowptr_pair_arguments() {
    // SendBehind takes theWindow and behindWindow pointer arguments.
    // Inside Macintosh Volume I (1985), p. I-286;
    // Macintosh Toolbox Essentials (1992), p. 4-91.
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]);
    for base in [win_a, win_b] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_a);
    bus.write_long(sp + 4, win_b);

    let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn sendbehind_nil_behindwindow_moves_window_to_back_and_rederives_front() {
    // SendBehind with behindWindow = NIL moves the window behind all others.
    // Inside Macintosh Volume I (1985), p. I-286;
    // Macintosh Toolbox Essentials (1992), p. 4-91.
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]);
    disp.front_window = win_b;
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0xFF);
    bus.write_byte(win_a + 111u32, 0x00);
    bus.write_byte(win_b + 111u32, 0xFF);
    for base in [win_a, win_b] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_long(sp + 4, win_b);

    let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(disp.window_list, vec![win_a, win_b]);
    assert_eq!(disp.front_window, win_a);
    assert_eq!(bus.read_byte(win_b + 111u32), 0x00);
    assert_eq!(bus.read_byte(win_a + 111u32), 0xFF);
    assert!(disp
        .event_queue
        .iter()
        .any(|event| { event.what == 8 && event.message == win_b && (event.modifiers & 1) == 0 }));
    assert!(disp
        .event_queue
        .iter()
        .any(|event| { event.what == 8 && event.message == win_a && (event.modifiers & 1) != 0 }));
}

#[test]
fn sendbehind_inactive_window_preserves_active_window() {
    // BringToFront can leave an inactive window visually ahead of the
    // active one. Sending a different inactive window backward must not
    // silently activate that visual head. Macintosh Toolbox Essentials
    // (1992), pp. 4-90 to 4-91.
    let (mut disp, mut cpu, mut bus) = setup();
    let visual_front = 0x200040u32;
    let active = 0x200140u32;
    let target = 0x200240u32;
    disp.window_list.replace(vec![visual_front, active, target]);
    disp.front_window = active;
    for base in [visual_front, active, target] {
        bus.write_byte(base + 110u32, 0xFF);
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }
    bus.write_byte(visual_front + 111u32, 0x00);
    bus.write_byte(active + 111u32, 0xFF);
    bus.write_byte(target + 111u32, 0x00);
    disp.event_queue.clear();

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_long(sp + 4, target);

    let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(disp.window_list, vec![visual_front, active, target]);
    assert_eq!(disp.front_window, active);
    assert_eq!(bus.read_byte(visual_front + 111u32), 0x00);
    assert_eq!(bus.read_byte(active + 111u32), 0xFF);
    assert!(disp.event_queue.is_empty());
}

#[test]
fn send_behind_null_moves_window_to_back() {
    let (mut disp, mut cpu, mut bus) = setup();
    // Two fake windows already in the list, newest at front.
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_b, win_a]);
    disp.front_window = win_b;
    // Minimum portRect to satisfy the bounds read.
    for base in [win_a, win_b] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0); // behindWindow = NULL
    bus.write_long(sp + 4, win_b); // theWindow = B (currently front)

    let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    assert_eq!(
        disp.window_list,
        vec![win_a, win_b],
        "B must move to back of window_list"
    );
    assert_eq!(
        disp.front_window, win_a,
        "front_window must re-derive to the new head"
    );
}

#[test]
fn send_behind_specific_window_inserts_just_after_target() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    let win_c = 0x200240u32;
    disp.window_list.replace(vec![win_c, win_b, win_a]);
    disp.front_window = win_c;
    for base in [win_a, win_b, win_c] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, win_a); // behindWindow = A
    bus.write_long(sp + 4, win_c); // theWindow = C

    let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    // C must now sit immediately behind A, with B still in front.
    assert_eq!(disp.window_list, vec![win_b, win_a, win_c]);
    assert_eq!(disp.front_window, win_b);
}

// SendBehind's front re-derivation must skip hidden windows when
// picking the new front.
#[test]
fn send_behind_skips_hidden_candidate_when_promoting_front() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    let win_c = 0x200240u32;
    disp.window_list.replace(vec![win_c, win_b, win_a]); // c front
    disp.front_window = win_c;
    for &base in &[win_a, win_b, win_c] {
        bus.write_word(base + 16, 10);
        bus.write_word(base + 18, 10);
        bus.write_word(base + 20, 50);
        bus.write_word(base + 22, 100);
    }
    // Only c and a are visible; b (middle) is hidden.
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0x00);
    bus.write_byte(win_c + 110u32, 0xFF);

    let sp = TEST_SP - 8;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0); // behindWindow = NIL (move to back)
    bus.write_long(sp + 4, win_c); // theWindow = C

    let result = dispatch(&mut disp, 0x121, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    // List becomes [b, a, c]. b is hidden, so front must
    // promote to the first visible entry = a.
    assert_eq!(disp.window_list, vec![win_b, win_a, win_c]);
    assert_eq!(
        disp.front_window, win_a,
        "must skip hidden b and pick visible a"
    );
}

// ---------------------------------------------------------------
// InvalRgn (0x127) — forwards the region's bbox into
// invalidate_window_rect, mirroring InvalRect.
// ---------------------------------------------------------------
#[test]
fn inval_rgn_adds_region_bbox_to_update_region() {
    let (mut disp, mut cpu, mut bus, window_ptr) = setup_region_window();
    let rgn_handle = make_region_handle(&mut bus, 0x300000, 0x300020, 10, (20, 10, 110, 70));
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, rgn_handle);

    assert!(disp.window_update_rect(&bus, window_ptr).is_none());
    let result = dispatch(&mut disp, 0x127, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(
        disp.window_update_rect(&bus, window_ptr),
        Some((60, 10, 150, 70))
    );
}

#[test]
fn inval_rgn_ignores_region_handles_with_short_size_headers() {
    let (mut disp, mut cpu, mut bus, window_ptr) = setup_region_window();
    let rgn_handle = make_region_handle(&mut bus, 0x300000, 0x300020, 8, (20, 10, 110, 70));
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, rgn_handle);

    assert!(disp.window_update_rect(&bus, window_ptr).is_none());
    let result = dispatch(&mut disp, 0x129, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert!(disp.window_update_rect(&bus, window_ptr).is_none());
}

#[test]
fn valid_rgn_clears_region_bbox_from_update_region() {
    let (mut disp, mut cpu, mut bus, window_ptr) = setup_region_window();
    let rgn_handle = make_region_handle(&mut bus, 0x300000, 0x300020, 10, (20, 10, 110, 70));
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, rgn_handle);

    let inval = dispatch(&mut disp, 0x127, &mut cpu, &mut bus);
    assert!(inval.unwrap().is_ok());
    assert_eq!(
        disp.window_update_rect(&bus, window_ptr),
        Some((60, 10, 150, 70))
    );

    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, rgn_handle);
    let result = dispatch(&mut disp, 0x129, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert!(disp.window_update_rect(&bus, window_ptr).is_none());
}

// ---------------------------------------------------------------
// GetWVariant (0x00A) — returns low 4 bits of window's procID per
// IM:V V-208 + IM:I I-282 (window definition ID = 16*resourceID +
// variation_code). NewWindow / NewCWindow / GetNewWindow /
// GetNewCWindow populate the window_proc_ids sidetable; GetWVariant
// recovers procID from it and masks the low 4 bits.
// ---------------------------------------------------------------
#[test]
fn getwvariant_returns_variation_code_from_low_four_bits_of_proc_id() {
    let (mut disp, mut cpu, mut bus) = setup();
    let cases: &[(i16, i16)] = &[
        (0, 0),  // documentProc → variant 0
        (1, 1),  // dBoxProc → variant 1
        (4, 4),  // noGrowDocProc → variant 4
        (5, 5),  // movableDBoxProc → variant 5
        (16, 0), // rDocProc (WDEF resID 1, variant 0)
    ];
    for &(proc_id, expected) in cases {
        let window_ptr = 0x0040_0000u32 + (proc_id as u32) * 0x100;
        disp.window_proc_ids.insert(window_ptr, proc_id);
        let sp = TEST_SP - 4;
        cpu.write_reg(Register::A7, sp);
        bus.write_long(sp, window_ptr);
        bus.write_word(sp + 4, 0xBEEF);
        let result = dispatch(&mut disp, 0x00A, &mut cpu, &mut bus);
        assert!(result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::A7),
            TEST_SP,
            "GetWVariant must advance A7 by 4 (procID={})",
            proc_id
        );
        assert_eq!(
            bus.read_word(TEST_SP) as i16,
            expected,
            "GetWVariant must return low 4 bits of procID; got wrong value for procID={}",
            proc_id
        );
    }
}

#[test]
fn getwvariant_returns_zero_for_nil_window_ptr() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, 0);
    bus.write_word(sp + 4, 0xBEEF);
    let result = dispatch(&mut disp, 0x00A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(
        bus.read_word(TEST_SP),
        0,
        "GetWVariant on NIL WindowPtr must defensively return 0 (no crash)"
    );
}

#[test]
fn getwvariant_function_protocol_pops_windowptr_and_writes_integer_result() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_ptr = 0x0042_0000u32;
    disp.window_proc_ids.insert(window_ptr, 8); // zoomDocProc → variant 8
    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_ptr);
    bus.write_word(sp + 4, 0xCAFE);
    bus.write_word(sp + 6, 0xBABE);
    let result = dispatch(&mut disp, 0x00A, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "GetWVariant must advance A7 by exactly 4 (consume WindowPtr arg)"
    );
    assert_eq!(
        bus.read_word(TEST_SP) as i16,
        8,
        "GetWVariant must return the variant"
    );
    assert_eq!(
        bus.read_word(TEST_SP + 2),
        0xBABE,
        "GetWVariant must not write past the 2-byte INTEGER result slot"
    );
}

// ---------------------------------------------------------------
// Unhandled trap returns None
// ---------------------------------------------------------------
#[test]
fn test_unhandled_trap_returns_none() {
    let (mut disp, mut cpu, mut bus) = setup();
    let result = dispatch(&mut disp, 0xFFF, &mut cpu, &mut bus);
    assert!(result.is_none(), "Unhandled trap should return None");
}

// MoveWindow with front=TRUE must bring the window to the front and
// emit the same hilite/activate side effects as SelectWindow (IM:I
// I-287 says it's equivalent to SelectWindow).
#[test]
fn move_window_with_front_true_brings_to_front_and_activates() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_a, win_b]);
    disp.front_window = win_a;
    bus.write_byte(win_a + 110u32, 0xFF); // visible
    bus.write_byte(win_b + 110u32, 0xFF);
    bus.write_byte(win_a + 111u32, 0xFF); // hilited (front)
    bus.write_byte(win_b + 111u32, 0x00);
    // Minimum CGrafPort / pixmap handle at +2 so MoveWindow's
    // `is_cgraf` path exits cleanly. Use GrafPort (portVersion
    // high bit not set) to avoid the pixmap deref.
    bus.write_word(win_b + 6, 0x0000);
    bus.write_word(win_b + 20, 50); // portRect.bottom
    bus.write_word(win_b + 22, 100); // portRect.right

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    // Pascal BOOLEAN in high byte of 2-byte slot (MPW C
    // convention).
    bus.write_byte(sp, 1); // front = TRUE
    bus.write_word(sp + 2, 60); // vGlobal
    bus.write_word(sp + 4, 40); // hGlobal
    bus.write_long(sp + 6, win_b);

    let queue_len_before = disp.event_queue.len();
    let result = dispatch(&mut disp, 0x11B, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(disp.front_window, win_b, "B must become front");
    assert_eq!(bus.read_byte(win_a + 111u32), 0x00, "A unhilited");
    assert_eq!(bus.read_byte(win_b + 111u32), 0xFF, "B hilited");
    assert_eq!(
        disp.event_queue.len() - queue_len_before,
        2,
        "must queue deactivate A + activate B"
    );
}

#[test]
fn move_window_with_front_false_preserves_z_order() {
    let (mut disp, mut cpu, mut bus) = setup();
    let win_a = 0x200040u32;
    let win_b = 0x200140u32;
    disp.window_list.replace(vec![win_a, win_b]);
    disp.front_window = win_a;
    bus.write_byte(win_a + 110u32, 0xFF);
    bus.write_byte(win_b + 110u32, 0xFF);
    bus.write_byte(win_a + 111u32, 0xFF);
    bus.write_byte(win_b + 111u32, 0x00);
    bus.write_word(win_b + 6, 0x0000);
    bus.write_word(win_b + 20, 50);
    bus.write_word(win_b + 22, 100);

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0); // front = FALSE
    bus.write_word(sp + 2, 60);
    bus.write_word(sp + 4, 40);
    bus.write_long(sp + 6, win_b);

    let queue_len_before = disp.event_queue.len();
    let result = dispatch(&mut disp, 0x11B, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        disp.front_window, win_a,
        "front must NOT change when front=FALSE"
    );
    assert_eq!(
        disp.event_queue.len(),
        queue_len_before,
        "no activate events when front=FALSE"
    );
}

// SizeWindow(fUpdate=TRUE) must InvalRect the newly-exposed area
// when the window grows, per IM:I I-287.
#[test]
fn size_window_with_fupdate_true_invalidates_new_area() {
    let (mut disp, mut cpu, mut bus) = setup();
    // Window records below occupy $300000; frame drawing needs separate RAM.
    let (_, row_bytes, width, height, depth) = disp.screen_mode;
    disp.set_screen_mode_for_test(0x320000, row_bytes, width, height, depth);
    bus.write_long(crate::memory::globals::addr::SCRN_BASE, 0x320000);
    let window_addr: u32 = 0x300000;
    let (_cont_rgn, update_rgn) =
        setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 100, 100);
    // Mark window visible so invalidate_window_rect's clip
    // intersection picks up the content rect.
    bus.write_byte(window_addr + 110u32, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    // Pascal BOOLEAN in high byte (MPW C convention).
    bus.write_byte(sp, 1); // fUpdate = TRUE
    bus.write_word(sp + 2, 200); // h = 200 (was 100)
    bus.write_word(sp + 4, 200); // w = 200 (was 100)
    bus.write_long(sp + 6, window_addr);

    let queue_len_before = disp.event_queue.len();
    let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    // updateRgn bbox must match the new content rect.
    assert_eq!(bus.read_word(update_rgn + 2) as i16, 0, "updateRgn.top");
    assert_eq!(
        bus.read_word(update_rgn + 6) as i16,
        200,
        "updateRgn.bottom must match new h"
    );
    assert_eq!(
        bus.read_word(update_rgn + 8) as i16,
        200,
        "updateRgn.right must match new w"
    );
    // And an update event was queued.
    assert!(
        disp.event_queue.len() > queue_len_before,
        "fUpdate=TRUE must queue an update event"
    );
}

#[test]
fn size_window_with_fupdate_false_skips_invalidation() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (_cont_rgn, update_rgn) =
        setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 100, 100);
    bus.write_byte(window_addr + 110u32, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0); // fUpdate = FALSE
    bus.write_word(sp + 2, 200);
    bus.write_word(sp + 4, 200);
    bus.write_long(sp + 6, window_addr);

    let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    // updateRgn must remain empty (the bbox we seeded as 0,0,0,0).
    assert_eq!(bus.read_word(update_rgn + 6) as i16, 0);
    assert_eq!(bus.read_word(update_rgn + 8) as i16, 0);
}

#[test]
fn drawnew_consumes_windowpeek_and_update_arguments() {
    // DrawNew consumes a Boolean update flag and WindowPeek pointer.
    // Inside Macintosh Volume I (1985), p. I-296;
    // Macintosh Toolbox Essentials (1992), p. 4-117.
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 0);
    bus.write_long(sp + 2, 0);

    let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

// DrawNew update behavior reference:
// Inside Macintosh Volume I (1985), p. I-296;
// Macintosh Toolbox Essentials (1992), p. 4-117.
#[test]
fn drawnew_update_true_consumes_saveold_and_drawnew_arguments() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let _ = setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 60, 120);
    bus.write_byte(window_addr + 110u32, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);
    let save_old = dispatch(&mut disp, 0x10E, &mut cpu, &mut bus);
    assert!(save_old.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);

    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1); // fUpdate = TRUE
    bus.write_long(sp + 2, window_addr);

    let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn saveold_drawnew_true_uses_saved_regions_after_content_change() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (cont_rgn, update_rgn) =
        setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 60, 120);
    bus.write_byte(window_addr + 110u32, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let sp = TEST_SP - 4;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);
    let save_old = dispatch(&mut disp, 0x10E, &mut cpu, &mut bus);
    assert!(save_old.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    // Change the content region before DrawNew. If SaveOld snapshots the
    // old bounds, DrawNew(TRUE) should invalidate the union of the saved
    // and current rectangles instead of only the new one.
    bus.write_word(cont_rgn + 2, 30);
    bus.write_word(cont_rgn + 4, 40);
    bus.write_word(cont_rgn + 6, 80);
    bus.write_word(cont_rgn + 8, 140);
    bus.write_word(update_rgn + 2, 0);
    bus.write_word(update_rgn + 4, 0);
    bus.write_word(update_rgn + 6, 0);
    bus.write_word(update_rgn + 8, 0);

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 1);
    bus.write_byte(sp + 1, 0);
    bus.write_long(sp + 2, window_addr);
    let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
    assert_eq!(bus.read_word(update_rgn + 2) as i16, 10, "updateRgn.top");
    assert_eq!(bus.read_word(update_rgn + 4) as i16, 20, "updateRgn.left");
    assert_eq!(bus.read_word(update_rgn + 6) as i16, 80, "updateRgn.bottom");
    assert_eq!(bus.read_word(update_rgn + 8) as i16, 140, "updateRgn.right");
}

#[test]
fn drawnew_update_false_preserves_pending_update_and_checkupdate_returns_true() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (_cont_rgn, update_rgn) =
        setup_full_window_with_regions(&mut bus, window_addr, 10, 20, 60, 120);
    bus.write_word(update_rgn + 2, 20);
    bus.write_word(update_rgn + 4, 30);
    bus.write_word(update_rgn + 6, 90);
    bus.write_word(update_rgn + 8, 140);
    bus.write_byte(window_addr + 110u32, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_long(sp, window_addr);
    let save_old = dispatch(&mut disp, 0x10E, &mut cpu, &mut bus);
    assert!(save_old.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);

    cpu.write_reg(Register::A7, sp);
    bus.write_byte(sp, 0);
    bus.write_byte(sp + 1, 0x7F); // garbage low byte must be ignored
    bus.write_long(sp + 2, window_addr);

    let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);

    let event_ptr = bus.alloc(16);
    for i in 0..16 {
        bus.write_byte(event_ptr + i, 0xCC);
    }
    let check_sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, check_sp);
    bus.write_long(check_sp, event_ptr);
    bus.write_word(check_sp + 4, 0xFFFF);

    let check_result = dispatch(&mut disp, 0x111, &mut cpu, &mut bus);
    assert!(check_result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP - 2);
    assert_eq!(bus.read_word(TEST_SP - 2), 0x0100, "result should be TRUE");
    assert_eq!(
        bus.read_word(event_ptr),
        6,
        "event.what should be updateEvt"
    );
    assert_eq!(
        bus.read_long(event_ptr + 2),
        window_addr,
        "event.message should carry WindowPtr"
    );
}

#[test]
fn draw_new_nil_window_is_safe() {
    let (mut disp, mut cpu, mut bus) = setup();
    let sp = TEST_SP - 6;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1);
    bus.write_long(sp + 2, 0); // NIL window
    let result = dispatch(&mut disp, 0x10F, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A7), TEST_SP);
}

#[test]
fn size_window_shrinking_does_not_invalidate_even_with_fupdate_true() {
    let (mut disp, mut cpu, mut bus) = setup();
    let window_addr: u32 = 0x300000;
    let (_cont_rgn, update_rgn) =
        setup_full_window_with_regions(&mut bus, window_addr, 0, 0, 200, 200);
    bus.write_byte(window_addr + 110u32, 0xFF);
    disp.window_list.replace(vec![window_addr]);
    disp.front_window = window_addr;

    let sp = TEST_SP - 10;
    cpu.write_reg(Register::A7, sp);
    bus.write_word(sp, 1); // fUpdate = TRUE
    bus.write_word(sp + 2, 100); // h = 100 (shrinking from 200)
    bus.write_word(sp + 4, 100); // w = 100 (shrinking from 200)
    bus.write_long(sp + 6, window_addr);

    let result = dispatch(&mut disp, 0x11D, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());

    // updateRgn stays empty — shrinking uncovers nothing.
    assert_eq!(bus.read_word(update_rgn + 6) as i16, 0);
    assert_eq!(bus.read_word(update_rgn + 8) as i16, 0);
}

#[test]
fn default_window_color_table_uses_synthetic_wctb_0_with_white_content() {
    let (mut disp, _cpu, mut bus) = setup();
    let handle = disp.default_window_color_table_handle(&mut bus);
    assert_ne!(handle, 0);
    let ptr = bus.read_long(handle);
    assert_ne!(ptr, 0);
    let color = super::super::TrapDispatcher::window_content_color(&bus, handle);
    assert_eq!(color, Some((0xFFFF, 0xFFFF, 0xFFFF)));
}

#[test]
fn window_content_color_ignores_indexed_device_color_tables() {
    let (_disp, _cpu, mut bus) = setup();
    // Allocate a 256-entry device ColorTable (table_size = 255)
    let ctab_ptr = bus.alloc(8 + 256 * 8);
    bus.write_word(ctab_ptr + 6, 255);
    // Write arbitrary color at entry 0
    bus.write_word(ctab_ptr + 8, 0);
    bus.write_word(ctab_ptr + 10, 0x1234);
    bus.write_word(ctab_ptr + 12, 0x5678);
    bus.write_word(ctab_ptr + 14, 0x9ABC);
    let ctab_handle = bus.alloc(4);
    bus.write_long(ctab_handle, ctab_ptr);

    // A device CLUT is not a semantic window-part table.
    assert_eq!(
        super::super::TrapDispatcher::window_content_color(&bus, ctab_handle),
        None
    );
}
