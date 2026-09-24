use super::*;

#[test]
fn attached_68k_and_powerpc_event_adapters_share_fifo_and_menu_bar_invalidation() {
    let (mut classic, _, _) = setup_with_port();
    let pef = synthetic_pef_with_import(b"InvalMenuBar");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();

    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    assert!(!context.event_queue().menu_bar_is_invalid());

    classic.event_queue.push_back(QueuedEvent {
        what: 1,
        message: 0x1111,
        when: 0,
        where_v: 10,
        where_h: 20,
        modifiers: 0,
    });
    native.event_queue.push_back(QueuedEvent {
        what: 2,
        message: 0x2222,
        when: 0,
        where_v: 30,
        where_h: 40,
        modifiers: 0,
    });
    classic.event_queue.invalidate_menu_bar();

    assert_eq!(context.event_queue().len(), 2);
    assert_eq!(native.event_queue.get(0).unwrap().message, 0x1111);
    assert_eq!(native.event_queue.get(1).unwrap().message, 0x2222);
    assert!(context.event_queue().menu_bar_is_invalid());

    assert_eq!(native.event_queue.pop_front().unwrap().message, 0x1111);
    assert_eq!(classic.event_queue.front().unwrap().message, 0x2222);
    assert!(native.event_queue.take_menu_bar_invalidation());

    assert_eq!(context.event_queue().len(), 1);
    assert!(!context.event_queue().menu_bar_is_invalid());
}

#[test]
fn attached_event_launch_state_shares_native_first_and_classic_first_oapp_once() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    assert!(classic
        .apple_event_launch_state
        .ptr_eq(&native.apple_events.apple_event_launch_state));
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);

    let native_event = PPC_DATA_BASE + 0x3000;
    native.memory.add_region(native_event, vec![0; 16]);
    let classic_event = 0x0020_0000;
    let detached = native.apple_events.apple_event_launch_state.clone();

    // Native-first: the classic gateway must observe the same claimed
    // OAPP after native GetNextEvent consumes it.
    classic
        .apple_event_launch_state
        .reset_for_launch(true);
    native.cpu.gpr[3] = u32::from(PPC_HIGH_LEVEL_EVENT_MASK);
    native.cpu.gpr[4] = native_event;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(native.cpu.gpr[3], 1);
    assert!(context.event_queue().is_empty());
    assert!(classic
        .apple_event_launch_state
        .is_open_application_event_sent());

    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_bus.write_long(TEST_SP, classic_event);
    classic_bus.write_word(TEST_SP + 4, u32::from(PPC_HIGH_LEVEL_EVENT_MASK) as u16);
    classic_bus.write_word(TEST_SP + 6, 0xbeef);
    assert!(classic
        .dispatch_toolbox(true, 0x170, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP + 6), 0);
    assert_eq!(classic_bus.read_word(classic_event), 0);
    assert!(context.event_queue().is_empty());

    // Classic-first: reset only the process launch state, then make the
    // native gateway poll after classic GetNextEvent consumed OAPP.
    classic
        .apple_event_launch_state
        .reset_for_launch(true);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_bus.write_long(TEST_SP, classic_event);
    classic_bus.write_word(TEST_SP + 4, u32::from(PPC_HIGH_LEVEL_EVENT_MASK) as u16);
    classic_bus.write_word(TEST_SP + 6, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x170, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP + 6), 0x0100);
    assert_eq!(classic_bus.read_word(classic_event), 23);
    assert!(context.event_queue().is_empty());

    native.cpu.gpr[3] = u32::from(PPC_HIGH_LEVEL_EVENT_MASK);
    native.cpu.gpr[4] = native_event;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(native.cpu.gpr[3], 0);
    assert_eq!(native.memory.read_u16_be(native_event), Some(0));
    assert!(context.event_queue().is_empty());

    assert!(!detached.is_high_level_event_aware());
    assert!(!detached.is_open_application_event_sent());
}

#[test]
fn attached_event_launch_state_shares_eventavail_peek_without_duplicate_oapp() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let pef = synthetic_pef_with_import(b"EventAvail");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);

    let native_event = PPC_DATA_BASE + 0x3100;
    native.memory.add_region(native_event, vec![0; 16]);
    let classic_event = 0x0020_0000;
    classic
        .apple_event_launch_state
        .reset_for_launch(true);

    native.cpu.gpr[3] = u32::from(PPC_HIGH_LEVEL_EVENT_MASK);
    native.cpu.gpr[4] = native_event;
    run_test_import(&mut native, PpcImportDispatcherTarget::EventAvail);
    assert_eq!(native.cpu.gpr[3], 1);
    assert_eq!(context.event_queue().len(), 1);
    assert_eq!(native.memory.read_u16_be(native_event), Some(23));

    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_bus.write_long(TEST_SP, classic_event);
    classic_bus.write_word(TEST_SP + 4, u32::from(PPC_HIGH_LEVEL_EVENT_MASK) as u16);
    classic_bus.write_word(TEST_SP + 6, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x171, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP + 6), 0x0100);
    assert_eq!(classic_bus.read_word(classic_event), 23);
    assert_eq!(context.event_queue().len(), 1);

    native.cpu.gpr[3] = u32::from(PPC_HIGH_LEVEL_EVENT_MASK);
    native.cpu.gpr[4] = native_event;
    run_test_import(&mut native, PpcImportDispatcherTarget::EventAvail);
    assert_eq!(native.cpu.gpr[3], 1);
    assert_eq!(context.event_queue().len(), 1);
    assert!(native
        .apple_events
        .apple_event_launch_state
        .is_open_application_event_sent());
}

#[test]
fn attached_event_launch_state_requires_awareness_and_high_level_mask() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);

    let native_event = PPC_DATA_BASE + 0x3200;
    native.memory.add_region(native_event, vec![0; 16]);
    let classic_event = 0x0020_0000;

    classic
        .apple_event_launch_state
        .reset_for_launch(false);
    native.cpu.gpr[3] = u32::from(PPC_HIGH_LEVEL_EVENT_MASK);
    native.cpu.gpr[4] = native_event;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(native.cpu.gpr[3], 0);
    assert!(!native
        .apple_events
        .apple_event_launch_state
        .is_open_application_event_sent());
    assert!(context.event_queue().is_empty());

    classic
        .apple_event_launch_state
        .reset_for_launch(true);
    native.cpu.gpr[3] = 0x0008;
    native.cpu.gpr[4] = native_event;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
    );
    assert_eq!(native.cpu.gpr[3], 0);
    assert!(!native
        .apple_events
        .apple_event_launch_state
        .is_open_application_event_sent());
    assert!(context.event_queue().is_empty());

    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_bus.write_long(TEST_SP, classic_event);
    classic_bus.write_word(TEST_SP + 4, 0x0008);
    classic_bus.write_word(TEST_SP + 6, 0xbeef);
    assert!(classic
        .dispatch_toolbox(true, 0x170, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP + 6), 0);
    assert!(!classic
        .apple_event_launch_state
        .is_open_application_event_sent());
    assert!(context.event_queue().is_empty());
}

#[test]
fn attached_68k_and_powerpc_adapters_share_live_input_without_runner_copy() {
    let (mut classic, _, _) = setup_with_port();
    let pef = synthetic_pef_with_import(b"Button");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();

    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let detached = native.clone();

    classic.input_state.set_mouse_position_for_test((123, 456));
    classic.input_state.set_mouse_button_for_test(true);
    classic.input_state.set_key_map_byte_for_test(6, 0x20);

    assert_eq!(
        native.current_input_snapshot(),
        PpcInputSnapshot {
            key_map: {
                let mut key_map = [0; 16];
                key_map[6] = 0x20;
                key_map
            },
            mouse_button: true,
            mouse_v: 123,
            mouse_h: 456,
        }
    );
    let probe = native.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(native.cpu.gpr[3], 1);

    native.process_input.set_mouse_position_for_test((-20, 99));
    native.process_input.set_mouse_button_for_test(false);
    native.process_input.set_key_map_byte_for_test(1, 0x08);

    assert!(classic.input_state.ptr_eq(&native.process_input));
    assert_eq!(classic.input_state.mouse_position(), (-20, 99));
    assert!(!classic.input_state.mouse_button_pressed());
    assert_eq!(classic.input_state.key_map_snapshot()[1], 0x08);
    assert!(!native.process_input.ptr_eq(&detached.process_input));
    assert_eq!(detached.current_input_snapshot(), PpcInputSnapshot::default());
}

#[test]
fn attached_68k_and_powerpc_adapters_share_display_color_state_without_runner_copy() {
    let (mut classic, _, _) = setup_with_port();
    let pef = synthetic_pef_with_import(b"GetCTable");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();

    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let detached = native.clone();

    native.screen_clut.set_entry(7, [0x1111, 0x2222, 0x3333]);
    native.color_manager_clut.set_entry(9, [0x4444, 0x5555, 0x6666]);
    let mut native_gamma = native.display_gamma.table();
    native_gamma[1][42] = 0x7f;
    native.display_gamma.install(native_gamma);

    assert!(classic.device_clut.ptr_eq(&native.screen_clut));
    assert!(classic
        .color_manager_clut
        .ptr_eq(&native.color_manager_clut));
    assert!(classic.display_gamma.ptr_eq(&native.display_gamma));
    assert_eq!(classic.device_clut[7], [0x1111, 0x2222, 0x3333]);
    assert_eq!(classic.color_manager_clut[9], [0x4444, 0x5555, 0x6666]);
    assert_eq!(classic.display_gamma.table()[1][42], 0x7f);
    assert!(classic.display_gamma.is_explicit());

    classic.device_clut.set_entry(3, [0xaaaa, 0xbbbb, 0xcccc]);
    let mut classic_gamma = classic.display_gamma.table();
    classic_gamma[2][99] = 0x55;
    classic.display_gamma.install(classic_gamma);
    assert_eq!(native.screen_clut[3], [0xaaaa, 0xbbbb, 0xcccc]);
    assert_eq!(native.display_gamma.table()[2][99], 0x55);

    assert!(!native.screen_clut.ptr_eq(&detached.screen_clut));
    assert!(!native.display_gamma.ptr_eq(&detached.display_gamma));
    assert_eq!(
        detached.screen_clut,
        crate::display::standard_mac_8bpp_clut()
    );
    assert_eq!(
        detached.display_gamma.table(),
        crate::display::default_display_gamma()
    );
    assert!(!detached.display_gamma.is_explicit());
}

#[test]
fn attached_68k_and_powerpc_adapters_share_quickdraw_selection_without_runner_copy() {
    let (mut classic, _, mut classic_bus) = setup_with_port();
    let pef = synthetic_pef_with_import(b"GetGWorld");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();

    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let detached = native.clone();

    assert!(classic.current_port.ptr_eq(&native.current_gworld));
    assert!(classic
        .current_gdevice
        .ptr_eq(&native.current_gdevice));
    assert_eq!(*classic.current_port, PPC_MAIN_GWORLD);
    assert_eq!(*classic.current_gdevice, PPC_MAIN_GDEVICE);

    classic.main_gdevice_handle = 0;
    let classic_main_gdevice = classic.ensure_main_gdevice(&mut classic_bus);
    assert_ne!(classic_main_gdevice, PPC_MAIN_GDEVICE);
    assert_eq!(*classic.current_gdevice, PPC_MAIN_GDEVICE);

    native
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = 0x0030_0000);
    native
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = 0x0030_1000);
    assert_eq!(*classic.current_port, 0x0030_0000);
    assert_eq!(*classic.current_gdevice, 0x0030_1000);

    classic
        .current_port
        .with_mut(|current_port| *current_port = 0x0040_0000);
    classic
        .current_gdevice
        .with_mut(|current_gdevice| *current_gdevice = 0x0040_1000);
    assert_eq!(*native.current_gworld, 0x0040_0000);
    assert_eq!(*native.current_gdevice, 0x0040_1000);

    assert!(!native.current_gworld.ptr_eq(&detached.current_gworld));
    assert!(!native.current_gdevice.ptr_eq(&detached.current_gdevice));
    assert_eq!(*detached.current_gworld, PPC_MAIN_GWORLD);
    assert_eq!(*detached.current_gdevice, PPC_MAIN_GDEVICE);
}

#[test]
fn attached_quickdraw_port_draw_state_crosses_isa_immediately() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);
    let process_port = 0x0003_0000;
    initialize_test_cgraf_port(&mut classic_bus, process_port);
    native
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = process_port);

    let classic_color = 0x0002_8000;
    classic_bus.write_word(classic_color, 0x1111);
    classic_bus.write_word(classic_color + 2, 0x2222);
    classic_bus.write_word(classic_color + 4, 0x3333);
    classic_bus.write_word(TEST_SP, 23);
    classic_bus.write_word(TEST_SP + 2, 17);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x093, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    classic_bus.write_word(TEST_SP, 2);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x089, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    classic_bus.write_word(TEST_SP, 18);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x08a, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    classic_bus.write_long(TEST_SP, classic_color);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x214, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());

    let native_output = PPC_DATA_BASE + 0x2800;
    native.memory.add_region(native_output, vec![0; 16]);
    native.cpu.gpr[3] = native_output;
    run_test_import(&mut native, PpcImportDispatcherTarget::GetPen);
    assert_eq!(native.memory.read_u16_be(native_output), Some(23));
    assert_eq!(native.memory.read_u16_be(native_output + 2), Some(17));
    native.cpu.gpr[3] = native_output;
    run_test_import(&mut native, PpcImportDispatcherTarget::GetForeColor);
    assert_eq!(
        ppc_read_rgb_color(&mut native.memory, native_output),
        Some(PpcRgbColor {
            red: 0x1111,
            green: 0x2222,
            blue: 0x3333,
        })
    );
    assert_eq!(native.quickdraw_text_mode, 2);
    assert_eq!(native.quickdraw_text_size, 18);

    native.cpu.gpr[3] = 41;
    native.cpu.gpr[4] = 29;
    run_test_import(&mut native, PpcImportDispatcherTarget::MoveTo);
    let native_color = PpcRgbColor {
        red: 0xaaaa,
        green: 0xbbbb,
        blue: 0xcccc,
    };
    ppc_write_rgb_color(&mut native.memory, native_output, native_color).unwrap();
    native.cpu.gpr[3] = native_output;
    run_test_import(&mut native, PpcImportDispatcherTarget::RGBForeColor);
    native.cpu.gpr[3] = 4;
    run_test_import(&mut native, PpcImportDispatcherTarget::TextMode);

    let classic_output = 0x0002_8100;
    classic_bus.write_long(TEST_SP, classic_output);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x09a, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(classic_output), 29);
    assert_eq!(classic_bus.read_word(classic_output + 2), 41);
    classic_bus.write_long(TEST_SP, classic_output);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x219, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(classic_output), native_color.red);
    assert_eq!(classic_bus.read_word(classic_output + 2), native_color.green);
    assert_eq!(classic_bus.read_word(classic_output + 4), native_color.blue);
    assert_eq!(classic.tx_mode, 4);
}

#[test]
fn attached_quickdraw_port_state_is_per_port_and_detaches_with_clone() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);
    let main_port = 0x0003_0000;
    let second_port = 0x0003_1000;
    initialize_test_cgraf_port(&mut classic_bus, main_port);
    initialize_test_cgraf_port(&mut classic_bus, second_port);
    classic.cport_ports.insert(main_port);
    classic.cport_ports.insert(second_port);
    native
        .current_gworld
        .with_mut(|current_gworld| *current_gworld = main_port);
    native.cpu.gpr[3] = second_port;
    run_test_import(&mut native, PpcImportDispatcherTarget::SetPort);
    native.cpu.gpr[3] = 71;
    native.cpu.gpr[4] = 53;
    run_test_import(&mut native, PpcImportDispatcherTarget::MoveTo);

    native.cpu.gpr[3] = main_port;
    run_test_import(&mut native, PpcImportDispatcherTarget::SetPort);
    native.cpu.gpr[3] = 19;
    native.cpu.gpr[4] = 11;
    run_test_import(&mut native, PpcImportDispatcherTarget::MoveTo);

    classic_bus.write_long(TEST_SP, second_port);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x073, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    let classic_output = 0x0002_8200;
    classic_bus.write_long(TEST_SP, classic_output);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x09a, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(classic_output), 53);
    assert_eq!(classic_bus.read_word(classic_output + 2), 71);

    let mut detached = native.clone();
    detached
        .memory
        .write_u16_be(second_port + PPC_CGRAF_PORT_PN_LOC_OFFSET, 101)
        .unwrap();
    detached
        .memory
        .write_u16_be(second_port + PPC_CGRAF_PORT_PN_LOC_OFFSET + 2, 103)
        .unwrap();
    let detached_output = PPC_DATA_BASE + 0x2a00;
    detached.memory.add_region(detached_output, vec![0; 4]);
    detached.cpu.gpr[3] = detached_output;
    run_test_import(&mut detached, PpcImportDispatcherTarget::GetPen);
    assert_eq!(detached.memory.read_u16_be(detached_output), Some(101));
    assert_eq!(detached.memory.read_u16_be(detached_output + 2), Some(103));

    native.cpu.gpr[3] = detached_output;
    native.memory.add_region(detached_output, vec![0; 4]);
    run_test_import(&mut native, PpcImportDispatcherTarget::GetPen);
    assert_eq!(native.memory.read_u16_be(detached_output), Some(53));
    assert_eq!(native.memory.read_u16_be(detached_output + 2), Some(71));
}

#[test]
fn attached_quickdraw_pixel_states_cross_isa_and_detach_with_clone() {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 6-32--6-38:
    // the PixMapHandle state word is process-visible independently of
    // which CPU gateway performs LockPixels or SetPixelsState.
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"GetPixelsState")).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    assert!(classic
        .gworld_pixel_states
        .ptr_eq(&native.gworld_pixel_states));
    let detached = native.clone();
    assert!(!native
        .gworld_pixel_states
        .ptr_eq(&detached.gworld_pixel_states));

    let pmh = PPC_MAIN_PIXMAP_HANDLE;
    assert_eq!(native.gworld_pixel_states.quickdraw_pixel_state(pmh), 0);

    // Classic SetPixelsState publishes keepLocal + pixelsPurgeable to
    // the process registry; native GetPixelsState must observe both bits.
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_cpu.write_reg(Register::D0, 0x0008_000E);
    classic_bus.write_long(TEST_SP, (1 << 3) | (1 << 6));
    classic_bus.write_long(TEST_SP + 4, pmh);
    assert!(classic
        .dispatch_quickdraw(true, 0x31D, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(native.gworld_pixel_states.quickdraw_pixel_state(pmh), 0x48);

    native.cpu.gpr[3] = pmh;
    run_test_import(&mut native, PpcImportDispatcherTarget::LockPixels);
    assert_eq!(native.cpu.gpr[3], 1);
    assert_eq!(native.gworld_pixel_states.quickdraw_pixel_state(pmh), 0xc8);

    // The classic selector sees the native LockPixels transition through
    // the same attached process handle.
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_cpu.write_reg(Register::D0, 0x0004_000D);
    classic_bus.write_long(TEST_SP, pmh);
    classic_bus.write_long(TEST_SP + 4, 0);
    assert!(classic
        .dispatch_quickdraw(true, 0x31D, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_long(TEST_SP + 4), 0xc8);

    native.cpu.gpr[3] = pmh;
    run_test_import(&mut native, PpcImportDispatcherTarget::NoPurgePixels);
    assert_eq!(native.gworld_pixel_states.quickdraw_pixel_state(pmh), 0x88);
    assert_eq!(detached.gworld_pixel_states.quickdraw_pixel_state(pmh), 0);
}

#[test]
fn attached_quickdraw_error_crosses_isa_and_detaches_with_clone() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let mut detached = native.clone();

    assert!(classic
        .quickdraw_error
        .ptr_eq(&native.toolbox_startup.last_quickdraw_error));
    assert!(!classic
        .quickdraw_error
        .ptr_eq(&detached.toolbox_startup.last_quickdraw_error));

    native
        .toolbox_startup
        .last_quickdraw_error
        .with_mut(|error| *error = PPC_PARAM_ERR);
    classic_bus.write_word(TEST_SP, 0);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_quickdraw(true, 0x240, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP) as i16, PPC_PARAM_ERR);

    classic
        .quickdraw_error
        .with_mut(|error| *error = PPC_C_RES_ERR);
    run_test_import(&mut native, PpcImportDispatcherTarget::QDError);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_C_RES_ERR));

    run_test_import(&mut detached, PpcImportDispatcherTarget::QDError);
    assert_eq!(detached.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    detached
        .toolbox_startup
        .last_quickdraw_error
        .with_mut(|error| *error = PPC_MEM_FULL_ERR);
    assert_eq!(*classic.quickdraw_error, PPC_C_RES_ERR);
    assert_eq!(*native.toolbox_startup.last_quickdraw_error, PPC_C_RES_ERR);
}

#[test]
fn attached_ppc_event_queue_remains_shared_through_panic() {
    let pef = synthetic_pef_with_import(b"InvalMenuBar");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    context.shared_event_queue().push_back(QueuedEvent {
        what: 1,
        message: 0x1111,
        when: 0,
        where_v: 10,
        where_h: 20,
        modifiers: 0,
    });
    native.attach_unconverted_process_services(&mut context);

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        native.event_queue.push_back(QueuedEvent {
            what: 2,
            message: 0x2222,
            when: 0,
            where_v: 30,
            where_h: 40,
            modifiers: 0,
        });
        native.event_queue.invalidate_menu_bar();
        panic!("simulated panic inside PPC guest execution");
    }));

    assert!(panic_result.is_err());
    assert_eq!(context.event_queue().len(), 2);
    assert_eq!(context.event_queue().get(0).unwrap().message, 0x1111);
    assert_eq!(context.event_queue().get(1).unwrap().message, 0x2222);
    assert!(context.event_queue().menu_bar_is_invalid());
    assert_eq!(native.event_queue.len(), 2);
    assert!(native.event_queue.menu_bar_is_invalid());
}

#[test]
fn attached_native_adapters_share_process_file_mutations_immediately() {
    let pef = synthetic_pef_with_import(b"GetEOF");
    let mut first = load_pef_application(&pef).unwrap();
    first.push_test_vfs_file(PpcVfsFileRecord {
        path: "Shared Data".to_string(),
        data: (b"first".to_vec()).into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    });
    let mut second = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();

    first.attach_unconverted_process_services(&mut context);
    second.attach_unconverted_process_services(&mut context);
    assert!(first
        .process_file_system
        .ptr_eq(&second.process_file_system));

    second
        .with_test_vfs_file_mut(0, |file| {
            file.data
                .with_mut(|data| data.extend_from_slice(b"-second"));
        })
        .expect("seeded shared file");
    second.push_test_deleted_vfs_file_path("Obsolete Data".to_string());
    second.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 128,
        path: "Shared Data".to_string(),
        res_type: u32::from_be_bytes(*b"TEST"),
        res_id: 1,
        name: b"Shared".to_vec(),
        data: b"resource".to_vec(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    second.vfs_directories.push(PpcVfsDirectory {
        dir_id: PPC_FIRST_DYNAMIC_DIR_ID,
        parent_dir_id: PPC_ROOT_DIR_ID,
        path: "Shared Folder".to_string(),
        creator: u32::from_be_bytes(*b"TEST"),
        file_type: u32::from_be_bytes(*b"fold"),
        finder_flags: 0x0400,
        dirty: true,
    });
    second
        .next_vfs_dir_id
        .with_mut(|next_dir_id| *next_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1);
    second
        .default_dir_id
        .with_mut(|default_dir_id| *default_dir_id = PPC_FIRST_DYNAMIC_DIR_ID);

    assert_eq!(first.vfs_files[0].data, b"first-second");
    assert_eq!(first.deleted_vfs_file_paths, ["Obsolete Data"]);
    assert_eq!(first.vfs_resources[0].data, b"resource");
    assert_eq!(first.vfs_directories.last().unwrap().path, "Shared Folder");
    assert_eq!(first.next_vfs_dir_id, PPC_FIRST_DYNAMIC_DIR_ID + 1);
    assert_eq!(first.default_dir_id, PPC_FIRST_DYNAMIC_DIR_ID);
}

#[test]
fn process_file_records_remain_canonical_during_native_execution_panic_cross_isa() {
    let pef = synthetic_pef_with_import(b"TestImport");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic = TrapDispatcher::new();
    classic.attach_unconverted_process_services(&mut context);

    native.push_test_open_file(crate::process_context::ProcessOpenFileRecord {
        ref_num: PPC_FIRST_FILE_REF_NUM,
        path: "Shared/Native.bin".to_string(),
        position: 4,
    });
    native.insert_test_stdio_stream(
        0x2200,
        crate::process_context::ProcessStdioStreamRecord {
            ref_num: Some(PPC_FIRST_FILE_REF_NUM),
            path: Some("Shared/Native.bin".to_string()),
            position: 4,
            standard: false,
            readable: true,
            writable: true,
            append: false,
            closed: false,
            eof: false,
            error: false,
        },
    );
    native.push_test_vfs_file(PpcVfsFileRecord {
        path: "Shared/Native.bin".to_string(),
        data: b"native".to_vec().into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    });
    native.push_test_deleted_vfs_file_path("Shared/Deleted.bin".to_string());

    assert_eq!(
        classic
            .open_files
            .get(&(PPC_FIRST_FILE_REF_NUM as u16))
            .map(String::as_str),
        Some("Shared/Native.bin")
    );
    assert_eq!(
        classic
            .vfs
            .get("Shared/Native.bin")
            .map(Vec::as_slice),
        Some(b"native".as_slice())
    );

    // Drive the native return-import path with an intentionally empty
    // process Memory Manager. The return handler expects a native heap,
    // so this reliably unwinds execution after the file records have
    // been made live. A process-owned collection must remain populated
    // even when a native execution slice exits abnormally.
    let action = GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: native.entry_pc,
            rtoc: native.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            native.entry_pc,
            native.rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
    .expect("native test request should adapt to the PPC ABI");
    assert!(matches!(
        native.toolbox_startup.execution.calls()
            .externalize_powerpc_action(&mut native.cpu, action),
        PpcImportAction::Continue
    ));
    native.cpu.pc = PPC_GUEST_CALL_RETURN_PC;
    let mut empty_memory_manager = ProcessMemoryManager::default();
    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        native.run_with_process_memory_manager(
            64,
            false,
            false,
            &mut empty_memory_manager,
        );
    }));
    assert!(panic_result.is_err());

    assert_eq!(native.files.len(), 1);
    assert_eq!(
        native.process_file_system.stdio_streams[&0x2200].position,
        4
    );
    assert_eq!(native.vfs_files[0].data, b"native");
    assert_eq!(native.deleted_vfs_file_paths, ["Shared/Deleted.bin"]);
    assert_eq!(
        classic
            .open_files
            .get(&(PPC_FIRST_FILE_REF_NUM as u16))
            .map(String::as_str),
        Some("Shared/Native.bin")
    );
}

#[test]
fn attached_native_adapters_share_process_resource_mutations_immediately() {
    let pef = synthetic_pef_with_import(b"TestImport");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic = TrapDispatcher::new();
    classic.attach_unconverted_process_services(&mut context);

    native.push_resource_file(PpcResourceFileRecord {
        ref_num: PPC_FIRST_FILE_REF_NUM,
        path: "Shared/Native.rsrc".to_string(),
    });
    native.push_vfs_resource_file(PpcVfsResourceFileRecord {
        path: "Shared/Native.rsrc".to_string(),
        creator: u32::from_be_bytes(*b"TEST"),
        file_type: u32::from_be_bytes(*b"rsrc"),
        finder_flags: 0x0200,
        resource_len: 4,
        raw_data: Some(b"fork".to_vec().into()),
        map_attrs: 0,
        dirty: true,
    });
    native.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: PPC_FIRST_FILE_REF_NUM,
        path: "Shared/Native.rsrc".to_string(),
        res_type: u32::from_be_bytes(*b"TEST"),
        res_id: 128,
        name: b"Shared".to_vec(),
        data: b"resource".to_vec(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0x2200,
    });
    native.set_test_next_file_ref_num(PPC_FIRST_FILE_REF_NUM + 1);

    assert_eq!(classic.resource_files[0].path, "Shared/Native.rsrc");
    assert_eq!(classic.vfs_resource_files[0].finder_flags, 0x0200);
    assert_eq!(
        classic
            .vfs_resource_files
            .fork("Shared/Native.rsrc")
            .map(AsRef::<[u8]>::as_ref),
        Some(b"fork".as_slice())
    );
    assert_eq!(classic.vfs_resources[0].data, b"resource");
    assert_eq!(
        classic.allocate_process_file_refnum(),
        (PPC_FIRST_FILE_REF_NUM + 1) as u16
    );
    assert_eq!(native.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 2);

    classic.with_resource_manager_mut(|resource_manager| {
        resource_manager.vfs_resources[0].attrs = 0x0040;
        resource_manager.vfs_resource_files[0].dirty = false;
    });
    assert_eq!(native.vfs_resources[0].attrs, 0x0040);
    assert!(!native.vfs_resource_files[0].dirty);
}

#[test]
fn launched_application_path_crosses_adapters_and_native_imports_without_clone_aliasing() {
    let pef = synthetic_pef_with_import(b"LMGetCurApName");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic = TrapDispatcher::new();
    classic.attach_unconverted_process_services(&mut context);

    classic.set_launched_app_path("Apps/Classic App");
    assert_eq!(native.launched_app_path(), Some("Apps/Classic App"));
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LmGetCurApName,
        ),
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut native.memory, PPC_IMPORT_CUR_AP_NAME),
        Some(b"Classic App".to_vec())
    );

    native.set_launched_app_path("Apps/Native App");
    assert_eq!(classic.launched_app_path(), Some("Apps/Native App"));
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::SystemCompatibility(
            PpcSystemCompatibilityOperation::LmGetCurApName,
        ),
    );
    assert_eq!(
        ppc_read_pstring_bytes(&mut native.memory, PPC_IMPORT_CUR_AP_NAME),
        Some(b"Native App".to_vec())
    );

    let detached_clone = native.clone();
    native.set_launched_app_path("Apps/Current App");
    assert_eq!(classic.launched_app_path(), Some("Apps/Current App"));
    assert_eq!(detached_clone.launched_app_path(), Some("Apps/Native App"));
}

#[test]
fn process_resource_records_remain_canonical_during_native_execution_panic_cross_isa() {
    let pef = synthetic_pef_with_import(b"TestImport");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic = TrapDispatcher::new();
    classic.attach_unconverted_process_services(&mut context);

    native.push_resource_file(PpcResourceFileRecord {
        ref_num: PPC_FIRST_FILE_REF_NUM,
        path: "Shared/Native.rsrc".to_string(),
    });
    native.push_vfs_resource_file(PpcVfsResourceFileRecord {
        path: "Shared/Native.rsrc".to_string(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        resource_len: 0,
        raw_data: None,
        map_attrs: 0,
        dirty: false,
    });
    native.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: PPC_FIRST_FILE_REF_NUM,
        path: "Shared/Native.rsrc".to_string(),
        res_type: u32::from_be_bytes(*b"TEST"),
        res_id: 128,
        name: b"Shared".to_vec(),
        data: b"resource".to_vec(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    native.set_test_next_file_ref_num(PPC_FIRST_FILE_REF_NUM + 1);

    assert_eq!(classic.resource_files.len(), 1);
    assert_eq!(classic.vfs_resource_files.len(), 1);
    assert_eq!(classic.vfs_resources.len(), 1);
    assert_eq!(native.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 1);

    // The return handler expects a native heap. Running it with an empty
    // process Memory Manager reliably unwinds execution after the native
    // records are live. Process-owned Resource Manager records must stay
    // visible to the classic adapter even on this early exit.
    let action = GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: native.entry_pc,
            rtoc: native.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            native.entry_pc,
            native.rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
    .expect("native test request should adapt to the PPC ABI");
    assert!(matches!(
        native.toolbox_startup.execution.calls()
            .externalize_powerpc_action(&mut native.cpu, action),
        PpcImportAction::Continue
    ));
    native.cpu.pc = PPC_GUEST_CALL_RETURN_PC;
    let mut empty_memory_manager = ProcessMemoryManager::default();
    struct ResourceRecordProbe<'a> {
        classic: &'a TrapDispatcher,
        observed: &'a std::cell::Cell<bool>,
    }
    impl Drop for ResourceRecordProbe<'_> {
        fn drop(&mut self) {
            self.observed.set(
                self.classic.resource_files.len() == 1
                    && self.classic.vfs_resource_files.len() == 1
                    && self.classic.vfs_resources.len() == 1
                    && self.classic.vfs_resources[0].data == b"resource",
            );
        }
    }
    let observed_during_unwind = std::cell::Cell::new(false);
    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _probe = ResourceRecordProbe {
            classic: &classic,
            observed: &observed_during_unwind,
        };
        native.run_with_process_memory_manager(
            64,
            false,
            false,
            &mut empty_memory_manager,
        );
    }));
    assert!(panic_result.is_err());
    assert!(
        observed_during_unwind.get(),
        "classic Resource Manager lost native records during unwind"
    );

    assert_eq!(native.resource_files.len(), 1);
    assert_eq!(native.vfs_resource_files.len(), 1);
    assert_eq!(native.vfs_resources.len(), 1);
    assert_eq!(native.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 1);
    assert_eq!(classic.resource_files[0].path, "Shared/Native.rsrc");
    assert_eq!(classic.vfs_resource_files[0].path, "Shared/Native.rsrc");
    assert_eq!(classic.vfs_resources[0].data, b"resource");
    assert_eq!(
        classic.allocate_process_file_refnum(),
        (PPC_FIRST_FILE_REF_NUM + 1) as u16
    );
    assert_eq!(native.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 2);
}

#[test]
fn open_file_sessions_cross_cpu_adapters_and_detach_with_clone() {
    let pef = synthetic_pef_with_import(b"GetEOF");
    let mut native = load_pef_application(&pef).unwrap();
    native.push_test_open_file(crate::process_context::ProcessOpenFileRecord {
        ref_num: PPC_FIRST_FILE_REF_NUM,
        path: "Shared Data".to_string(),
        position: 4,
    });
    native
        .process_file_system
        .writable_refnums
        .insert(PPC_FIRST_FILE_REF_NUM as u16);
    native.set_test_next_file_ref_num(PPC_FIRST_FILE_REF_NUM + 1);

    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = native.clone();
    let mut classic = TrapDispatcher::new();
    classic.attach_unconverted_process_services(&mut context);

    assert!(classic.open_files.ptr_eq(&native.files));
    assert!(classic
        .write_refnums
        .ptr_eq(&native.process_file_system.writable_refnums));
    assert_eq!(
        classic
            .open_files
            .get(&(PPC_FIRST_FILE_REF_NUM as u16))
            .map(String::as_str),
        Some("Shared Data")
    );

    classic
        .file_positions
        .insert(PPC_FIRST_FILE_REF_NUM as u16, 9);
    assert_eq!(native.files[0].position, 9);

    classic
        .write_refnums
        .remove(&(PPC_FIRST_FILE_REF_NUM as u16));
    assert!(!native
        .process_file_system
        .writable_refnums
        .contains(&(PPC_FIRST_FILE_REF_NUM as u16)));

    native
        .with_test_open_file_mut(0, |file| file.position = 12)
        .expect("seeded native open file");
    assert_eq!(
        classic
            .file_positions
            .get(&(PPC_FIRST_FILE_REF_NUM as u16)),
        Some(&12)
    );

    let next_refnum = classic.allocate_process_file_refnum();
    assert_eq!(next_refnum, (PPC_FIRST_FILE_REF_NUM + 1) as u16);
    assert_eq!(native.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 2);
    classic
        .open_files
        .insert(next_refnum, "Classic Data".to_string());
    classic.file_positions.insert(next_refnum, 3);
    native
        .process_file_system
        .writable_refnums
        .insert(next_refnum);
    assert_eq!(native.files.last().unwrap().path, "Classic Data");
    assert_eq!(native.files.last().unwrap().position, 3);
    assert!(classic.write_refnums.contains(&next_refnum));

    assert_eq!(detached.files.len(), 1);
    assert_eq!(detached.files[0].position, 4);
    assert!(detached
        .process_file_system
        .writable_refnums
        .contains(&(PPC_FIRST_FILE_REF_NUM as u16)));
    assert!(!detached
        .process_file_system
        .writable_refnums
        .contains(&next_refnum));
    assert_eq!(detached.next_file_ref_num, PPC_FIRST_FILE_REF_NUM + 1);
}

#[test]
fn catalogue_mutations_cross_cpu_adapters_without_runner_sync() {
    let pef = synthetic_pef_with_import(b"GetEOF");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let executing_directories = native.vfs_directories.shared_handle();
    let executing_volumes = native.vfs_volumes.shared_handle();
    let executing_next_volume_ref_num =
        native.next_vfs_volume_ref_num.shared_handle();
    let executing_next_dir_id = native.next_vfs_dir_id.shared_handle();
    let executing_default_dir_id = native.default_dir_id.shared_handle();
    let executing_working_directories = native.working_directories.shared_handle();
    let executing_application_wd_ref_num = native
        .application_working_directory_ref_num
        .shared_handle();
    let detached = native.clone();
    let mut classic = TrapDispatcher::new();
    classic.attach_unconverted_process_services(&mut context);
    assert!(classic.vfs_directories.ptr_eq(&executing_directories));
    assert!(classic.next_vfs_dir_id.ptr_eq(&executing_next_dir_id));

    native.vfs_directories.push(PpcVfsDirectory {
        dir_id: PPC_FIRST_DYNAMIC_DIR_ID,
        parent_dir_id: PPC_ROOT_DIR_ID,
        path: "Shared Folder".to_string(),
        creator: u32::from_be_bytes(*b"TEST"),
        file_type: u32::from_be_bytes(*b"fold"),
        finder_flags: 0x0400,
        dirty: true,
    });
    native.push_test_vfs_file(PpcVfsFileRecord {
        path: "Shared Folder/Native Data".to_string(),
        data: b"native".to_vec().into(),
        creator: u32::from_be_bytes(*b"TEST"),
        file_type: u32::from_be_bytes(*b"DATA"),
        finder_flags: 0x0200,
        dirty: true,
    });
    native.vfs_volumes.push(PpcVfsVolumeRecord {
        ref_num: -2,
        name: "Read Only".to_string(),
        root_dir_id: PPC_FIRST_DYNAMIC_DIR_ID,
        attributes: 0x0080,
        file_count: 1,
        allocation_block_count: 16,
        allocation_block_size: 4096,
        clump_size: 4096,
        free_blocks: 0,
        bitmap_start: 3,
        allocation_pointer: 4,
        allocation_start: 5,
        next_catalog_id: PPC_FIRST_DYNAMIC_DIR_ID + 1,
        created_date: 1,
        modified_date: 2,
    });
    native
        .next_vfs_dir_id
        .with_mut(|next_dir_id| *next_dir_id = PPC_FIRST_DYNAMIC_DIR_ID + 1);
    native
        .default_dir_id
        .with_mut(|default_dir_id| *default_dir_id = PPC_FIRST_DYNAMIC_DIR_ID);

    // Directory records are canonical process state, so the classic
    // adapter observes native mutations before any file compatibility
    // publication pass.
    let shared_directory = classic
        .directory_entry_for_id(PPC_FIRST_DYNAMIC_DIR_ID)
        .cloned()
        .expect("native directory should be visible to classic adapter");
    assert_eq!(shared_directory.path, "Shared Folder");
    assert_eq!(shared_directory.parent_dir_id, PPC_ROOT_DIR_ID);
    assert_eq!(shared_directory.creator, u32::from_be_bytes(*b"TEST"));
    assert_eq!(shared_directory.file_type, u32::from_be_bytes(*b"fold"));
    assert_eq!(shared_directory.finder_flags, 0x0400);
    assert!(shared_directory.dirty);

    // Volume records are canonical process state, so the classic adapter
    // observes this native mutation before any catalogue publication pass.
    assert_eq!(
        classic.vfs_volume_for_ref_num(-2).map(|volume| volume.name.as_str()),
        Some("Read Only")
    );
    native.publish_test_native_vfs_catalogue();

    assert_eq!(
        classic.directory_path_for_id(PPC_FIRST_DYNAMIC_DIR_ID),
        Some("Shared Folder")
    );
    let metadata = classic
        .vfs_file_metadata("Shared Folder/Native Data")
        .unwrap();
    assert_eq!(metadata.parent_dir_id, PPC_FIRST_DYNAMIC_DIR_ID);
    assert_eq!(metadata.creator, u32::from_be_bytes(*b"TEST"));
    assert_eq!(metadata.file_type, u32::from_be_bytes(*b"DATA"));
    assert_eq!(metadata.finder_flags, 0x0200);
    assert_eq!(*classic.default_dir_id, PPC_FIRST_DYNAMIC_DIR_ID);
    assert_eq!(classic.vfs_volume_for_ref_num(-2).unwrap().name, "Read Only");

    let classic_dir_id = classic.ensure_vfs_directory("Classic Folder");
    let classic_directory = native
        .vfs_directories
        .iter()
        .find(|directory| directory.dir_id == classic_dir_id)
        .cloned()
        .expect("classic-created directory should be visible to native adapter");
    assert_eq!(classic_directory.path, "Classic Folder");
    assert_eq!(classic_directory.parent_dir_id, PPC_ROOT_DIR_ID);
    assert_eq!(classic_directory.creator, u32::from_be_bytes(*b"MACS"));
    assert_eq!(classic_directory.file_type, u32::from_be_bytes(*b"fold"));
    assert!(classic_directory.dirty);
    let classic_volume_ref = classic.mount_vfs_volume(
        "Classic Disk",
        0,
        0,
        16,
        4096,
        4096,
        8,
        3,
        4,
        5,
        PPC_FIRST_DYNAMIC_DIR_ID + 3,
        10,
        11,
    );
    assert_eq!(
        native
            .process_file_system
            .vfs_volumes
            .iter()
            .find(|volume| volume.ref_num == classic_volume_ref)
            .map(|volume| volume.name.as_str()),
        Some("Classic Disk")
    );
    classic
        .vfs
        .insert("Classic Folder/Classic Data".to_string(), b"classic".to_vec());
    classic.set_vfs_entry_metadata(
        "Classic Folder/Classic Data",
        *b"TEXT",
        *b"ttxt",
        0x0100,
    );
    classic.set_launched_app_path("Classic Folder/Classic Data");
    let classic_wd_ref_num = *classic.app_wd_refnum;
    assert!(classic
        .working_directories
        .ptr_eq(&native.working_directories));
    assert!(classic
        .app_wd_refnum
        .ptr_eq(&native.application_working_directory_ref_num));
    assert_eq!(*executing_application_wd_ref_num, classic_wd_ref_num);
    assert_eq!(
        executing_working_directories[&classic_wd_ref_num].dir_id,
        classic_dir_id
    );

    let wd_vref_ptr = PPC_DATA_BASE + 0x2800;
    let wd_dir_id_ptr = wd_vref_ptr + 4;
    let wd_proc_id_ptr = wd_vref_ptr + 8;
    native.memory.add_region(wd_vref_ptr, vec![0; 12]);
    native.cpu.gpr[3] = classic_wd_ref_num as u16 as u32;
    native.cpu.gpr[4] = wd_vref_ptr;
    native.cpu.gpr[5] = wd_dir_id_ptr;
    native.cpu.gpr[6] = wd_proc_id_ptr;
    native
        .memory
        .write_u32_be(crate::memory::globals::addr::CUR_DIR_STORE, classic_dir_id)
        .unwrap();
    run_test_import(&mut native, PpcImportDispatcherTarget::GetWDInfo);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        native.memory.read_u16_be(wd_vref_ptr),
        Some(PPC_BOOT_VOLUME_REF_NUM as u16)
    );
    assert_eq!(native.memory.read_u32_be(wd_dir_id_ptr), Some(classic_dir_id));
    assert_eq!(native.memory.read_u32_be(wd_proc_id_ptr), Some(0));
    assert!(native.vfs_directories.iter().any(|directory| {
        directory.dir_id == classic_dir_id && directory.path == "Classic Folder"
    }));
    assert!(executing_directories.iter().any(|directory| {
        directory.dir_id == classic_dir_id && directory.path == "Classic Folder"
    }));
    assert!(executing_volumes.iter().any(|volume| {
        volume.ref_num == classic_volume_ref && volume.name == "Classic Disk"
    }));
    assert_eq!(*executing_next_volume_ref_num, classic_volume_ref - 1);
    assert_eq!(*executing_next_dir_id, *classic.next_vfs_dir_id);
    assert_eq!(*executing_default_dir_id, classic_dir_id);
    assert_eq!(*detached.next_vfs_volume_ref_num, -2);
    assert!(!detached
        .vfs_directories
        .iter()
        .any(|directory| directory.path == "Classic Folder"));
    assert!(!detached
        .vfs_volumes
        .iter()
        .any(|volume| volume.name == "Classic Disk"));
    assert_ne!(*detached.next_vfs_dir_id, *executing_next_dir_id);
    assert_ne!(*detached.default_dir_id, *executing_default_dir_id);
    assert!(detached.working_directories.is_empty());
    assert_eq!(*detached.application_working_directory_ref_num, -1);
    assert!(!detached
        .working_directories
        .ptr_eq(&executing_working_directories));
    run_test_import(&mut native, PpcImportDispatcherTarget::GetEOF);
    assert!(native.vfs_directories.ptr_eq(&executing_directories));
    assert!(native.vfs_volumes.ptr_eq(&executing_volumes));
    assert!(native
        .next_vfs_volume_ref_num
        .ptr_eq(&executing_next_volume_ref_num));
    assert!(native.next_vfs_dir_id.ptr_eq(&executing_next_dir_id));
    assert!(native.default_dir_id.ptr_eq(&executing_default_dir_id));
    let classic_file = native
        .vfs_files
        .iter()
        .find(|file| file.path == "Classic Folder/Classic Data")
        .unwrap();
    assert_eq!(classic_file.data, b"classic");
    assert_eq!(classic_file.file_type, u32::from_be_bytes(*b"TEXT"));
    assert_eq!(classic_file.creator, u32::from_be_bytes(*b"ttxt"));
    assert_eq!(classic_file.finder_flags, 0x0100);
    classic
        .vfs
        .with_entry_mut("Classic Folder/Classic Data", |bytes| {
            bytes.extend_from_slice(b"-shared");
        })
        .unwrap();
    assert_eq!(
        native
            .vfs_files
            .iter()
            .find(|file| file.path == "Classic Folder/Classic Data")
            .unwrap()
            .data,
        b"classic-shared"
    );

    native.cpu.gpr[3] = 0;
    native.cpu.gpr[4] = PPC_BOOT_VOLUME_REF_NUM as u16 as u32;
    native.cpu.gpr[5] = PPC_FIRST_DYNAMIC_DIR_ID;
    run_test_import(&mut native, PpcImportDispatcherTarget::HSetVol);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(*classic.default_dir_id, PPC_FIRST_DYNAMIC_DIR_ID);
    let native_wd_ref_num = *classic.app_wd_refnum;
    assert_eq!(
        classic.working_directories[&native_wd_ref_num].dir_id,
        PPC_FIRST_DYNAMIC_DIR_ID
    );
    assert_eq!(
        *native.application_working_directory_ref_num,
        native_wd_ref_num
    );

    classic
        .locked_files
        .insert("Shared Folder/Native Data".to_string());
    // Native Delete removes the canonical record before queueing its notice.
    native.process_file_system.with_mut(|file_system| {
        file_system.vfs_files.retain(|file| file.path != "Shared Folder/Native Data");
    });
    native.push_test_deleted_vfs_file_path("Shared Folder/Native Data".to_string());
    native.publish_test_native_vfs_catalogue();
    assert!(!classic
        .vfs_metadata
        .contains_key("Shared Folder/Native Data"));
    assert!(!classic
        .locked_files
        .contains("Shared Folder/Native Data"));

    assert!(classic.remove_vfs_path("Classic Folder/Classic Data"));
    assert!(!native
        .vfs_files
        .iter()
        .any(|file| file.path == "Classic Folder/Classic Data"));
}

#[test]
fn detached_native_app_clones_keep_independent_file_systems() {
    let pef = synthetic_pef_with_import(b"GetEOF");
    let mut original = load_pef_application(&pef).unwrap();
    original.push_test_vfs_file(PpcVfsFileRecord {
        path: "Detached Data".to_string(),
        data: (b"before".to_vec()).into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    });
    original.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: 128,
        path: "Detached Data".to_string(),
        res_type: u32::from_be_bytes(*b"TEST"),
        res_id: 1,
        name: Vec::new(),
        data: b"resource".to_vec(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
    original.vfs_directories.push(PpcVfsDirectory {
        dir_id: PPC_FIRST_DYNAMIC_DIR_ID,
        parent_dir_id: PPC_ROOT_DIR_ID,
        path: "Detached Folder".to_string(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        dirty: false,
    });
    let detached = original.clone();

    original
        .with_test_vfs_file_mut(0, |file| {
            file.data.with_mut(|data| data.copy_from_slice(b"change"));
        })
        .expect("seeded detached file");
    original.with_resource_manager_mut(|resource_manager| {
        resource_manager.vfs_resources[0]
            .data
            .copy_from_slice(b"changed!");
    });
    original.vfs_directories.with_mut(|directories| {
        directories.last_mut().unwrap().path = "Changed Folder".to_string();
    });
    original
        .next_vfs_dir_id
        .with_mut(|next_dir_id| *next_dir_id += 1);

    assert!(!original
        .process_file_system
        .ptr_eq(&detached.process_file_system));
    assert_eq!(original.vfs_files[0].data, b"change");
    assert_eq!(detached.vfs_files[0].data, b"before");
    assert_eq!(original.vfs_resources[0].data, b"changed!");
    assert_eq!(detached.vfs_resources[0].data, b"resource");
    assert_eq!(original.vfs_directories.last().unwrap().path, "Changed Folder");
    assert_eq!(detached.vfs_directories.last().unwrap().path, "Detached Folder");
    assert_eq!(
        *original.next_vfs_dir_id,
        *detached.next_vfs_dir_id + 1
    );
}

#[test]
fn adopted_ppc_execution_pair_shares_and_snapshot_detaches_coherently() {
    let pef = synthetic_pef_with_import(b"MenuSelect");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
        0x0012_3456,
    )));
    let plan = native.preflight_migrated_services(&context, 0).unwrap();
    native.commit_migrated_services(&context, plan);
    let detached = native.clone();

    native
        .toolbox_startup
        .execution
        .with_menu_state_mut(|tracking| tracking.highlighted_item = 3)
        .unwrap();

    assert_eq!(
        context
            .menu_tracking()
            .map(|tracking| tracking.highlighted_item),
        Some(3)
    );
    assert_eq!(
        native
            .toolbox_startup
            .execution
            .menu()
            .as_ref()
            .unwrap()
            .highlighted_item,
        3
    );
    assert!(!native
        .toolbox_startup
        .execution
        .menu()
        .ptr_eq(detached.toolbox_startup.execution.menu()));
    assert_eq!(
        detached
            .toolbox_startup
            .execution
            .menu()
            .as_ref()
            .unwrap()
            .highlighted_item,
        1
    );
}

#[test]
fn adopted_ppc_execution_pair_remains_shared_through_panic() {
    let pef = synthetic_pef_with_import(b"MenuSelect");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    context.set_menu_tracking(Some(crate::menu_manager::test_process_menu_tracking(
        0x0012_3456,
    )));
    let plan = native.preflight_migrated_services(&context, 0).unwrap();
    native.commit_migrated_services(&context, plan);

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        native
            .toolbox_startup
            .execution
            .with_menu_state_mut(|tracking| tracking.highlighted_item = 5)
            .unwrap();
        panic!("simulated panic inside PPC guest execution");
    }));

    assert!(panic_result.is_err());
    assert_eq!(
        context
            .menu_tracking()
            .map(|tracking| tracking.highlighted_item),
        Some(5)
    );
    assert_eq!(
        native
            .toolbox_startup
            .execution
            .menu()
            .as_ref()
            .unwrap()
            .highlighted_item,
        5
    );
}

#[test]
fn attached_resource_policy_mutations_cross_isa_immediately() {
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    assert_eq!(
        native.memory.read_u16_be(crate::memory::globals::addr::RES_LOAD),
        Some(0x0100),
    );

    classic_bus.write_word(TEST_SP, 0x00ff);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_toolbox(true, 0x19b, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert!(!native.policy.res_load());

    native.cpu.gpr[3] = 0;
    run_test_import(&mut native, PpcImportDispatcherTarget::SetResLoad);
    assert_eq!(
        native.memory.read_u16_be(crate::memory::globals::addr::RES_LOAD),
        Some(0),
    );
    native.cpu.gpr[3] = 1;
    run_test_import(&mut native, PpcImportDispatcherTarget::SetResLoad);
    assert!(classic.policy.res_load());
    assert_eq!(
        native.memory.read_u16_be(crate::memory::globals::addr::RES_LOAD),
        Some(0x0100),
    );

    classic_bus.write_word(TEST_SP, 0x0100);
    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_toolbox(true, 0x193, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert!(native.policy.res_purge());
}

#[test]
fn attached_application_limit_mutations_cross_isa_immediately() {
    // Inside Macintosh: Memory (1992), pp. 2-83--2-85: SetApplLimit
    // changes the guest-visible application boundary without changing
    // the allocator's physical heap ceiling.
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);
    native.attach_unconverted_process_services(&mut context);

    let native_heap_ceiling = native.heap_limit();
    assert_eq!(
        classic_bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
        native.application_heap_limit(),
        "native attachment must synchronize the initial low-memory projection"
    );
    let requested = native.heap_cursor() + 0x1000;
    assert!(requested < native_heap_ceiling);
    native.cpu.gpr[3] = requested;
    run_test_import(&mut native, PpcImportDispatcherTarget::SetApplLimit);
    assert_eq!(native.application_heap_limit(), requested);
    assert_eq!(
        classic_bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
        requested,
        "native SetApplLimit must publish the process value through low memory"
    );

    let classic_requested = requested + 0x1000;
    classic_bus.write_long(crate::memory::globals::addr::HEAP_END, native.heap_cursor());
    classic_bus.write_long(
        crate::memory::globals::addr::APPL_LIMIT,
        requested,
    );
    classic_cpu.write_reg(Register::A0, classic_requested);
    assert!(classic
        .dispatch_memory(false, 0x2D, &mut classic_cpu, &mut classic_bus)
        .expect("SetApplLimit should be handled")
        .is_ok());

    // The immediately following native import is the nested cross-ISA
    // observation point: it must read process state, not a stale adapter
    // snapshot or the native allocator ceiling.
    assert_eq!(native.application_heap_limit(), classic_requested);
    native.cpu.gpr[3] = 0;
    run_test_import(&mut native, PpcImportDispatcherTarget::GetApplLimit);
    assert_eq!(native.cpu.gpr[3], classic_requested);
    assert_eq!(native.heap_limit(), native_heap_ceiling);
}

#[test]
fn attached_resource_errors_use_canonical_low_memory_cross_isa() {
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);
    let low_memory = classic_bus
        .shared_ram_region(0, 0x0010_0000)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);

    classic_bus.write_word(
        crate::memory::globals::addr::RES_ERR,
        PPC_RES_NOT_FOUND_ERR as u16,
    );
    run_test_import(&mut native, PpcImportDispatcherTarget::ResError);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_RES_NOT_FOUND_ERR));

    native.cpu.gpr[3] = 0xdead_beef;
    run_test_import(&mut native, PpcImportDispatcherTarget::LoadResource);
    assert_eq!(
        classic_bus.read_word(crate::memory::globals::addr::RES_ERR) as i16,
        PPC_RES_NOT_FOUND_ERR
    );

    classic_cpu.write_reg(Register::A7, TEST_SP);
    assert!(classic
        .dispatch_resource(true, 0x1af, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.read_word(TEST_SP) as i16, PPC_RES_NOT_FOUND_ERR);
}

#[test]
fn cloned_native_adapter_detaches_resource_policy_and_error_state() {
    let mut original =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    original.policy.set_res_load(false);
    original.policy.set_res_purge(true);
    original.set_test_resource_error(PPC_RES_NOT_FOUND_ERR);
    let mut detached = original.clone();

    detached.policy.set_res_load(true);
    detached.policy.set_res_purge(false);
    detached.set_test_resource_error(PPC_RES_F_NOT_FOUND_ERR);

    assert!(!original.policy.res_load());
    assert!(original.policy.res_purge());
    assert_eq!(original.test_resource_error(), PPC_RES_NOT_FOUND_ERR);
    assert!(detached.policy.res_load());
    assert!(!detached.policy.res_purge());
    assert_eq!(
        detached.test_resource_error(),
        PPC_RES_F_NOT_FOUND_ERR
    );
}

#[test]
fn attached_control_manager_metadata_crosses_isa_immediately() {
    let mut native =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let (mut classic, _classic_cpu, mut classic_bus) = setup_with_port();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let window = classic_bus.alloc(180);
    let (classic_handle, classic_pointer) = classic.create_control_record(
        &mut classic_bus,
        window,
        (10, 20, 30, 140),
        b"Mode",
        true,
        1,
        300,
        96,
        1009,
        0,
    );
    let classic_record = native
        .controls
        .records()
        .into_iter()
        .find(|record| record.handle == classic_handle)
        .unwrap();
    assert_eq!(classic_record.pointer, classic_pointer);
    assert_eq!(classic_record.proc_id, 1009);
    assert_eq!(classic_record.popup_menu_id, 300);
    assert_eq!(classic_record.popup_title_width, Some(96));

    native.controls.register(0x0030_1000, 0x0030_2000, 16, 0);
    assert_eq!(classic.control_manager.proc_id(0x0030_2000), 16);

    classic.dispose_control_handle(&mut classic_bus, classic_handle);
    assert!(!native.controls.contains_handle(classic_handle));
    native.controls.remove_handle(0x0030_1000);
    assert!(!classic.control_manager.contains_pointer(0x0030_2000));
}

#[test]
fn cloned_native_adapter_detaches_control_manager_metadata() {
    let original =
        load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    original
        .controls
        .register(0x0031_1000, 0x0031_2000, 1, 0);
    let detached = original.clone();

    detached.controls.set_proc_id(0x0031_2000, 2);
    detached
        .controls
        .register(0x0031_3000, 0x0031_4000, 16, 0);

    assert_eq!(original.controls.proc_id(0x0031_2000), 1);
    assert_eq!(detached.controls.proc_id(0x0031_2000), 2);
    assert!(!original.controls.contains_handle(0x0031_3000));
}
