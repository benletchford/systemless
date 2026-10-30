use super::*;

#[test]
fn ae_interact_with_user_allows_foreground_application() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AEInteractWithUser"),
        PpcImportDispatcherTarget::AEInteractWithUser,
    );
    let pef = synthetic_pef_with_import(b"AEInteractWithUser");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = u32::MAX; // Default timeout.
    loaded.cpu.gpr[4] = 0; // No Notification Manager record.
    loaded.cpu.gpr[5] = 0; // No idle callback.

    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEInteractWithUser);

    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
}

#[test]
fn ae_manager_info_returns_version_and_recorder_count() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AEManagerInfo"),
        PpcImportDispatcherTarget::AEManagerInfo,
    );
    let pef = synthetic_pef_with_import(b"AEManagerInfo");
    let mut loaded = load_pef_application(&pef).unwrap();
    let output = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(output, vec![0xff; 4]);
    loaded.cpu.gpr[4] = output;

    for (keyword, expected) in [(b"vers", 0x0101_0000), (b"recr", 0)] {
        loaded.cpu.gpr[3] = u32::from_be_bytes(*keyword);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AEManagerInfo);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(loaded.memory.read_u32_be(output), Some(expected));
    }

    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"vers");
    loaded.cpu.gpr[4] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::AEManagerInfo);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn ppc_launch_size_resource_enables_open_application_apple_event() {
    let pef = synthetic_pef_with_import(b"WaitNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut size = Vec::new();
    size.extend_from_slice(&ApplicationSizeResource::HIGH_LEVEL_EVENT_AWARE.to_be_bytes());
    size.extend_from_slice(&(2 * 1024 * 1024u32).to_be_bytes());
    size.extend_from_slice(&(1024 * 1024u32).to_be_bytes());
    loaded.seed_vfs_files_and_resources(
        Vec::new(),
        Vec::new(),
        vec![PpcVfsResourceRecord {
            ref_num: 0,
            path: "Test App".to_string(),
            res_type: u32::from_be_bytes(*b"SIZE"),
            res_id: -1,
            name: Vec::new(),
            data: size.into(),
            raw_data: None,
            raw_attrs: None,
            attrs: 0,
            handle: 0,
        }],
    );
    loaded.set_launched_app_path("Test App");

    assert!(loaded
        .apple_events
        .apple_event_launch_state
        .is_high_level_event_aware());
}

#[test]
fn ppc_apple_event_manager_delivers_oapp_and_calls_registered_native_handler() {
    let pef = synthetic_pef_with_import(b"AEInstallEventHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let handler_tvector = scratch;
    let handler_entry = scratch + 0x100;
    let event_record = scratch + 0x200;
    loaded.memory.add_region(scratch, vec![0; 0x300]);
    loaded
        .memory
        .write_u32_be(handler_tvector, handler_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_tvector + 4, 0x0200_1234)
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry, 0x3860_1234)
        .unwrap(); // li r3,$1234
    loaded
        .memory
        .write_u32_be(handler_entry + 4, 0x4e80_0020)
        .unwrap(); // blr
    loaded.cpu.gpr[3] = PPC_CORE_EVENT_CLASS;
    loaded.cpu.gpr[4] = PPC_OPEN_APPLICATION_EVENT;
    loaded.cpu.gpr[5] = handler_tvector;
    loaded.cpu.gpr[6] = 0xfeed_beef;
    loaded.cpu.gpr[7] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.apple_events.handlers.len(), 1);

    loaded
        .apple_events
        .apple_event_launch_state
        .set_high_level_event_aware(true);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target =
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent);
    loaded.cpu.gpr[3] = u32::from(PPC_HIGH_LEVEL_EVENT_MASK);
    loaded.cpu.gpr[4] = event_record;
    loaded.cpu.gpr[5] = 0;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u16_be(event_record),
        Some(PPC_HIGH_LEVEL_EVENT)
    );
    assert_eq!(
        loaded.memory.read_u32_be(event_record + 2),
        Some(PPC_CORE_EVENT_CLASS)
    );
    assert_eq!(
        u32::from(loaded.memory.read_u16_be(event_record + 10).unwrap()) << 16
            | u32::from(loaded.memory.read_u16_be(event_record + 12).unwrap()),
        PPC_OPEN_APPLICATION_EVENT
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AEProcessAppleEvent;
    loaded.cpu.gpr[3] = event_record;
    let handles_before_dispatch = loaded.handles();
    let ptrs_before_dispatch = loaded.ptrs();
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234);
    assert_eq!(loaded.cpu.gpr[2], loaded.rtoc);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.handles(), handles_before_dispatch);
    assert_eq!(loaded.ptrs(), ptrs_before_dispatch);
}

#[test]
fn apple_event_handler_bundle_is_process_owned_cross_isa_and_depth_disposed() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = context.memory_manager_mut().detached_clone();
    let event_record = PPC_DATA_BASE + 0x2600;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let handlers = SharedProcessAppleEventHandlers::default();
    handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: resolve_guest_procedure(
                &mut native.memory,
                PPC_CODE_BASE,
                PPC_DATA_BASE,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap(),
            refcon: 0xfeed_beef,
        },
    );
    let mut apple_events = PpcAppleEventState {
        handlers,
        ..PpcAppleEventState::default()
    };
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    let handle_states = native.handle_states();
    native.cpu.gpr[3] = event_record;
    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert!(GuestCallEffect::from_ppc_import_action(action).is_some());
    assert_eq!(apple_events.pending_dispatches.len(), 1);
    let descriptors = native.cpu.gpr[3];
    assert_eq!(native.cpu.gpr[4], descriptors + 8);
    assert_eq!(native.cpu.gpr[5], 0xfeed_beef);
    let event_handle = native.memory.read_u32_be(descriptors + 4).unwrap();
    let reply_handle = native.memory.read_u32_be(descriptors + 12).unwrap();
    let (event_allocation, reply_allocation) = {
        let manager = context.memory_manager_mut();
        (
            manager.native_allocation(event_handle).unwrap(),
            manager.native_allocation(reply_handle).unwrap(),
        )
    };
    assert_eq!(event_allocation.size, 8);
    assert_eq!(reply_allocation.size, 0);
    assert_eq!(detached.native_allocation(event_handle), None);
    assert_eq!(detached.native_allocation(reply_handle), None);

    let shared = native.memory.shared_view();
    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(shared);
    context.attach_classic_memory_bus(&mut classic_bus);
    assert_eq!(
        classic_bus.read_bytes(event_allocation.ptr, 8),
        [
            PPC_CORE_EVENT_CLASS.to_be_bytes(),
            PPC_OPEN_APPLICATION_EVENT.to_be_bytes(),
        ]
        .concat()
    );
    classic_bus.write_byte(event_allocation.ptr, b'P');
    assert_eq!(native.memory.read_u8(event_allocation.ptr), Some(b'P'));
    native
        .memory
        .write_u8(event_allocation.ptr + 7, b'!')
        .unwrap();
    assert_eq!(classic_bus.read_byte(event_allocation.ptr + 7), b'!');

    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut apple_events,
            1,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
        assert!(manager.native_allocation(event_handle).is_some());
        assert!(manager.native_allocation(reply_handle).is_some());
    }
    assert_eq!(apple_events.pending_dispatches.len(), 1);

    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut apple_events,
            0,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
        assert_eq!(manager.native_allocation(event_handle), None);
        assert_eq!(manager.native_allocation(reply_handle), None);
        assert!(!manager
            .native_allocator_snapshot()
            .unwrap()
            .ptrs
            .iter()
            .any(|record| record.ptr == descriptors));
    }
    assert!(apple_events.pending_dispatches.is_empty());
    assert_eq!(native.memory.read_u32_be(event_handle), Some(0));
    assert_eq!(native.memory.read_u32_be(reply_handle), Some(0));
    assert!(handles
        .iter()
        .all(|record| { record.handle != event_handle && record.handle != reply_handle }));
    assert!(handle_states
        .iter()
        .all(|record| { record.handle != event_handle && record.handle != reply_handle }));
}

#[test]
fn classic_apple_event_handler_bundle_is_process_owned_cross_isa_and_disposed() {
    let (mut classic, mut classic_cpu, mut classic_bus) = setup_with_port();
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    native.attach_unconverted_process_services(&mut context);

    let ram_end = classic_bus.ram_size();
    let low_memory_end = 0x0010_0000.min(ram_end);
    let low_memory = classic_bus
        .shared_ram_region(0, low_memory_end)
        .expect("classic adapter owns low memory");
    context.attach_memory(0, low_memory, &mut native.memory);
    for (base, end) in native.memory.mapping_holes(low_memory_end, ram_end) {
        let memory = classic_bus
            .shared_ram_region(base, end - base)
            .expect("classic adapter owns the native mapping hole");
        context.attach_memory(base, memory, &mut native.memory);
    }

    let sp = TEST_SP;
    let event_class = u32::from_be_bytes(*b"aevt");
    let event_id = u32::from_be_bytes(*b"oapp");
    let handler = 0x0040_8000;
    classic_cpu.write_reg(Register::D0, 0x091F);
    classic_bus.write_word(sp, 0);
    classic_bus.write_long(sp + 2, 0xfeed_beef);
    classic_bus.write_long(sp + 6, handler);
    classic_bus.write_long(sp + 10, event_id);
    classic_bus.write_long(sp + 14, event_class);
    classic_bus.write_word(sp + 18, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(
        native
            .apple_events
            .handlers
            .get(false, event_class, event_id)
            .map(|registered| (
                registered.procedure.isa,
                registered.procedure.original_pointer,
                registered.refcon,
            )),
        Some((GuestIsa::M68k, handler, 0xfeed_beef))
    );

    let native_event_class = u32::from_be_bytes(*b"misc");
    let native_event_id = u32::from_be_bytes(*b"slct");
    native.cpu.gpr[3] = native_event_class;
    native.cpu.gpr[4] = native_event_id;
    native.cpu.gpr[5] = PPC_CODE_BASE;
    native.cpu.gpr[6] = 0x1234_5678;
    native.cpu.gpr[7] = 1;
    assert_eq!(
        ppc_install_apple_event_handler(&native.cpu, &mut native.memory, &mut native.apple_events,),
        PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
    );
    assert_eq!(
        classic
            .ae_handlers
            .get(true, native_event_class, native_event_id)
            .map(|registered| (registered.procedure.isa, registered.refcon)),
        Some((GuestIsa::PowerPc, 0x1234_5678))
    );

    classic_bus.write_word(0x0032_0000, PPC_HIGH_LEVEL_EVENT);
    classic_bus.write_long(0x0032_0002, event_class);
    classic_bus.write_long(0x0032_000a, event_id);
    classic_cpu.write_reg(Register::A7, sp);
    classic_cpu.write_reg(Register::PC, 0x00f0_1234);
    classic_cpu.write_reg(Register::D0, 0x021B);
    classic_bus.write_long(sp, 0x0032_0000);
    classic_bus.write_word(sp + 4, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());

    let state = classic
        .ae_call_state
        .clone()
        .expect("classic AppleEvent handler is in flight");
    let (event_desc, reply_desc) = state
        .owned_descriptors
        .expect("classic AppleEvent callback descriptors are process-owned");
    let event_handle = classic_bus.read_long(event_desc + 4);
    let event_data = classic_bus.read_long(event_handle);
    assert_eq!(native.memory.read_u32_be(event_desc), Some(event_class));
    assert_eq!(
        native.memory.read_u32_be(event_desc + 4),
        Some(event_handle)
    );
    assert_eq!(native.memory.read_u32_be(event_handle), Some(event_data));
    classic_bus.write_byte(event_data, b'C');
    assert_eq!(native.memory.read_u8(event_data), Some(b'C'));
    native.memory.write_u8(event_data + 7, b'!').unwrap();
    assert_eq!(classic_bus.read_byte(event_data + 7), b'!');

    classic_bus.write_word(state.expected_sp_after_rtd, 0);
    classic_cpu.write_reg(Register::A7, state.expected_sp_after_rtd);
    classic_cpu.write_reg(Register::D0, 0xFEFE);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_bus.get_alloc_size(event_desc), None);
    assert_eq!(classic_bus.get_alloc_size(reply_desc), None);
    assert_eq!(classic_bus.get_alloc_size(event_handle), None);
    assert_eq!(classic_bus.get_alloc_size(event_data), None);
    assert!(classic.ae_call_state.is_none());

    native.cpu.gpr[3] = event_class;
    native.cpu.gpr[4] = event_id;
    native.cpu.gpr[5] = PPC_CODE_BASE;
    native.cpu.gpr[6] = 0xcafe_babe;
    native.cpu.gpr[7] = 0;
    assert_eq!(
        ppc_install_apple_event_handler(&native.cpu, &mut native.memory, &mut native.apple_events,),
        PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
    );
    classic_cpu.write_reg(Register::A7, sp);
    classic_cpu.write_reg(Register::PC, 0x00f0_5678);
    classic_cpu.write_reg(Register::D0, 0x021B);
    classic_bus.write_long(sp, 0x0032_0000);
    classic_bus.write_word(sp + 4, 0);
    assert!(classic
        .dispatch_toolbox(true, 0x016, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    let pending = classic.guest_calls.pending_powerpc_from_m68k().unwrap();
    assert_eq!(pending.target.isa, GuestIsa::PowerPc);
    assert_eq!(pending.target.entry, PPC_CODE_BASE);
    assert_eq!(pending.arguments.as_slice()[2], 0xcafe_babe);
}
#[test]
fn apple_event_handler_bundle_allocation_failure_is_atomic() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    native.set_heap_cursor(native.heap_limit().saturating_sub(8));
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let event_record = PPC_DATA_BASE + 0x2700;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let handlers = SharedProcessAppleEventHandlers::default();
    handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: resolve_guest_procedure(
                &mut native.memory,
                PPC_CODE_BASE,
                PPC_DATA_BASE,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap(),
            refcon: 0,
        },
    );
    let mut apple_events = PpcAppleEventState {
        handlers,
        ..PpcAppleEventState::default()
    };
    let before_allocator = context
        .memory_manager_mut()
        .native_allocator_snapshot()
        .unwrap();
    let before_handles = context
        .memory_manager_mut()
        .native_handle_records()
        .to_vec();
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;

    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert_eq!(
        action,
        PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR))
    );
    assert!(apple_events.pending_dispatches.is_empty());
    let manager = context.memory_manager_mut();
    let after_allocator = manager.native_allocator_snapshot().unwrap();
    assert_eq!(
        after_allocator.heap.heap_cursor,
        before_allocator.heap.heap_cursor
    );
    assert_eq!(
        after_allocator.heap.heap_limit,
        before_allocator.heap.heap_limit
    );
    assert_eq!(after_allocator.ptrs, before_allocator.ptrs);
    assert_eq!(
        after_allocator.free_ptr_blocks,
        before_allocator.free_ptr_blocks
    );
    assert_eq!(
        after_allocator.free_handle_blocks,
        before_allocator.free_handle_blocks
    );
    assert_eq!(manager.native_handle_records(), before_handles);
}

#[test]
fn native_apple_event_dispatch_enters_registered_classic_handler() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let event_record = PPC_DATA_BASE + 0x2800;
    native.memory.add_region(event_record, vec![0; 16]);
    native
        .memory
        .write_u16_be(event_record, PPC_HIGH_LEVEL_EVENT)
        .unwrap();
    native
        .memory
        .write_u32_be(event_record + 2, PPC_CORE_EVENT_CLASS)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 10, (PPC_OPEN_APPLICATION_EVENT >> 16) as u16)
        .unwrap();
    native
        .memory
        .write_u16_be(event_record + 12, PPC_OPEN_APPLICATION_EVENT as u16)
        .unwrap();
    let classic_handler = 0x0000_4000;
    native.apple_events.handlers.install(
        false,
        PPC_CORE_EVENT_CLASS,
        PPC_OPEN_APPLICATION_EVENT,
        ProcessAppleEventHandler {
            procedure: crate::guest_procedure::GuestProcedure::raw_m68k(classic_handler),
            refcon: 0xfeed_beef,
        },
    );
    let mut heap_cursor = native.heap_cursor();
    let heap_limit = native.heap_limit();
    let mut last_mem_error = native.last_mem_error();
    let mut handles = native.handles();
    native.cpu.gpr[3] = event_record;

    let action = {
        let mut manager = context.memory_manager_mut();
        ppc_process_apple_event(
            &mut native.cpu,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            heap_limit,
            &mut last_mem_error,
            &mut handles,
            &mut native.apple_events,
            &mut native.toolbox_startup,
            0,
        )
    };

    assert_eq!(action, PpcImportAction::Halt);
    assert!(native.guest_calls().has_m68k_execution());
    let pending = native.guest_calls().activate_m68k().unwrap();
    assert_eq!(pending.entry, classic_handler);
    assert_eq!(native.apple_events.pending_dispatches.len(), 1);
    let dispatch = native.apple_events.pending_dispatches[0];
    let mut classic_descriptors = SharedProcessAppleEventDescriptors::default();
    context.attach_apple_event_descriptors(&mut classic_descriptors);
    let event = classic_descriptors
        .events
        .get(&dispatch.descriptors)
        .expect("classic adapter sees native callback event semantics");
    assert_eq!(event.event_class, PPC_CORE_EVENT_CLASS);
    assert_eq!(event.event_id, PPC_OPEN_APPLICATION_EVENT);
    assert!(classic_descriptors
        .descriptors
        .contains_key(&(dispatch.descriptors + 8)));

    {
        let mut manager = context.memory_manager_mut();
        ppc_complete_apple_event_dispatch(
            &mut native.cpu,
            &mut native.apple_events,
            0,
            &mut manager,
            &mut native.memory,
            &mut heap_cursor,
            &mut last_mem_error,
            &mut handles,
        );
    }
    assert!(!classic_descriptors
        .events
        .contains_key(&dispatch.descriptors));
    assert!(!classic_descriptors
        .descriptors
        .contains_key(&dispatch.descriptors));
}

#[test]
fn ppc_special_handler_install_preserves_native_callbacks_and_system_scope() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "AEInstallSpecialHandler"),
        PpcImportDispatcherTarget::AEInstallSpecialHandler,
    );
    let pef = synthetic_pef_with_import(b"AEInstallSpecialHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    let class = u32::from_be_bytes(*b"phac");
    for (system, handler) in [(false, 0x1000), (true, 0x2000), (false, 0x3000)] {
        loaded.cpu.gpr[3] = class;
        loaded.cpu.gpr[4] = handler;
        loaded.cpu.gpr[5] = u32::from(system);
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AEInstallSpecialHandler);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        assert_eq!(
            loaded.apple_events.special_handlers.get(&(system, class)),
            Some(&handler),
        );
    }
    assert_eq!(loaded.apple_events.object_callbacks.get(&class), Some(&0x3000));
    assert_eq!(loaded.apple_events.special_handlers.get(&(true, class)), Some(&0x2000));
    assert_eq!(loaded.apple_events.special_handlers.get(&(false, class)), Some(&0x3000));
}

#[test]
fn ae_remove_event_handler_requires_the_installed_handler() {
    let pef = synthetic_pef_with_import(b"AEInstallEventHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let (handler_tvector, handler_entry) = (scratch, scratch + 0x100);
    loaded.memory.add_region(scratch, vec![0; 0x200]);
    loaded
        .memory
        .write_u32_be(handler_tvector, handler_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(handler_entry, 0x4e80_0020)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_CORE_EVENT_CLASS;
    loaded.cpu.gpr[4] = PPC_OPEN_APPLICATION_EVENT;
    loaded.cpu.gpr[5] = handler_tvector;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    assert_eq!(loaded.apple_events.handlers.len(), 1);

    for (handler, expected) in [
        (scratch + 0x40, ppc_i16_result(-1717)),
        (handler_tvector, ppc_i16_result(PPC_NO_ERR)),
        (handler_tvector, ppc_i16_result(-1717)),
    ] {
        loaded.cpu.gpr[3] = PPC_CORE_EVENT_CLASS;
        loaded.cpu.gpr[4] = PPC_OPEN_APPLICATION_EVENT;
        loaded.cpu.gpr[5] = handler;
        loaded.cpu.gpr[6] = 0;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::AERemoveEventHandler);
        assert_eq!(loaded.cpu.gpr[3], expected);
    }
    assert_eq!(loaded.apple_events.handlers.len(), 0);
}

fn self_addressed_send_fixture() -> (PpcLoadedApp, u32) {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"AESend")).unwrap();
    let scratch = PPC_DATA_BASE + 0x5000;
    loaded.memory.add_region(scratch, vec![0; 0x1000]);
    loaded.memory.write_u32_be(scratch + 0x44, 2).unwrap();
    let ae = PpcImportDispatcherTarget::AppleEventCompatibility;
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"psn ");
    loaded.cpu.gpr[4] = scratch + 0x40;
    loaded.cpu.gpr[5] = 8;
    loaded.cpu.gpr[6] = scratch;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CreateDesc),
    );
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"send");
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = 17;
    loaded.cpu.gpr[7] = 42;
    loaded.cpu.gpr[8] = scratch + 8;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CreateAppleEvent),
    );
    loaded
        .memory
        .write_bytes(scratch + 0x50, b"payload")
        .unwrap();
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"----");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[6] = scratch + 0x50;
    loaded.cpu.gpr[7] = 7;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::PutParamPtr),
    );
    (loaded, scratch)
}

#[test]
fn aesend_direct_queue_and_wait_preserve_payload_reply_and_handler_errors() {
    for mode in [2, 3] {
        let (mut loaded, scratch) = self_addressed_send_fixture();
        let entry = scratch + 0x200;
        let observed = scratch + 0x100;
        for (index, symbol) in [(1, "AEGetParamPtr"), (2, "AEPutParamPtr")] {
            let mut binding = loaded.imports[0].clone();
            binding.symbol_index = index;
            binding.symbol_name = symbol.into();
            binding.dispatcher_target = dispatcher_target_for_import("InterfaceLib", symbol);
            binding.trap_pc = PPC_IMPORT_TRAP_BASE + index * 4;
            loaded.memory.add_region(binding.trap_pc, vec![0; 4]);
            loaded.imports.push(binding);
        }
        loaded.import_count = 3;
        // Preserve the PPC linkage area and outgoing parameter home area.
        let mut code = vec![
            0x7c08_02a6, // mflr r0
            d_form_u(36, 0, 1, 8),
            d_form_u(37, 1, 1, (-96i16) as u16),
            d_form_u(36, 5, 1, 56), // refcon = output buffer
            d_form_u(36, 4, 1, 60), // reply descriptor
            d_form_u(14, 6, 5, 0),
            d_form_u(14, 7, 5, 8),
            d_form_u(14, 9, 5, 4),
            d_form_u(14, 8, 0, 16),
            d_form_u(15, 4, 0, 0x2d2d),
            d_form_u(24, 4, 4, 0x2d2d),
            d_form_u(15, 5, 0, 0x5445),
            d_form_u(24, 5, 5, 0x5854),
        ];
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(32, 3, 1, 60),
            d_form_u(32, 6, 1, 56),
            d_form_u(32, 7, 6, 4),
            d_form_u(14, 6, 6, 8),
            d_form_u(15, 4, 0, 0x2d2d),
            d_form_u(24, 4, 4, 0x2d2d),
            d_form_u(15, 5, 0, 0x5445),
            d_form_u(24, 5, 5, 0x5854),
        ]);
        let call_pc = entry + code.len() as u32 * 4;
        code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 8).wrapping_sub(call_pc) & 0x03ff_fffc));
        code.extend([
            d_form_u(14, 1, 1, 96),
            d_form_u(32, 0, 1, 8),
            0x7c08_03a6,
            d_form_u(14, 3, 0, (-50i16) as u16),
            0x4e80_0020,
        ]);
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
        loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
        loaded
            .memory
            .write_u32_be(scratch + 0x84, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"send");
        loaded.cpu.gpr[5] = scratch + 0x80;
        loaded.cpu.gpr[6] = observed;
        loaded.cpu.gpr[7] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
        let before = loaded.handles().len();
        let pointers = loaded.ptrs();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        );
        loaded.cpu.gpr[3] = scratch + 8;
        loaded.cpu.gpr[4] = scratch + 16;
        loaded.cpu.gpr[5] = mode;
        loaded.cpu.gpr[6] = 0;
        loaded.cpu.gpr[7] = u32::MAX;
        let probe = loaded.run_with_hle_imports(256);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(probe.handled_import_count, 3);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, observed + 8, 7),
            Some(b"payload".to_vec())
        );
        assert!(loaded.apple_events.pending_dispatches.is_empty());
        assert_eq!(loaded.ptrs(), pointers);
        assert_eq!(loaded.handles().len(), before + 1);
        let reply = ppc_ae_descriptor(
            &mut loaded.memory,
            &loaded.apple_events.descriptors,
            scratch + 16,
        )
        .unwrap();
        assert_eq!(reply.data, b"aevtansr");
        assert_eq!(reply.fields[&u32::from_be_bytes(*b"----")].data, b"payload");
        assert_eq!(
            reply.fields[&u32::from_be_bytes(*b"errn")].data,
            (-50i32).to_be_bytes()
        );
        assert_eq!(
            reply.attributes[&u32::from_be_bytes(*b"rtid")].data,
            17i16.to_be_bytes()
        );
    }
}

#[test]
fn aesend_no_reply_passes_a_null_descriptor_and_releases_temporaries() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let entry = scratch + 0x200;
    let observed = scratch + 0x100;
    let code = [
        d_form_u(32, 6, 4, 0),
        d_form_u(36, 6, 5, 0),
        d_form_u(32, 6, 4, 4),
        d_form_u(36, 6, 5, 4),
        d_form_u(14, 3, 0, 42),
        0x4e80_0020,
    ];
    for (index, word) in code.iter().enumerate() {
        loaded
            .memory
            .write_u32_be(entry + index as u32 * 4, *word)
            .unwrap();
    }
    loaded.memory.write_u32_be(scratch + 0x80, entry).unwrap();
    loaded
        .memory
        .write_u32_be(scratch + 0x84, loaded.rtoc)
        .unwrap();
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"send");
    loaded.cpu.gpr[5] = scratch + 0x80;
    loaded.cpu.gpr[6] = observed;
    loaded.cpu.gpr[7] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AEInstallEventHandler,
    );
    let handles = loaded.handles();
    let pointers = loaded.ptrs();
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = scratch + 16;
    loaded.cpu.gpr[5] = 1;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(observed), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(observed + 4), Some(0));
    assert_eq!(loaded.memory.read_u32_be(scratch + 16), Some(PPC_TYPE_NULL));
    assert_eq!(loaded.memory.read_u32_be(scratch + 20), Some(0));
    assert_eq!(loaded.handles(), handles);
    assert_eq!(loaded.ptrs(), pointers);
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    loaded.cpu.gpr[3] = scratch + 16;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"----");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[6] = scratch + 0x50;
    loaded.cpu.gpr[7] = 7;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-1707));
    assert_eq!(loaded.handles(), handles);
}

#[test]
fn aesend_rejects_missing_handlers_targets_and_invalid_modes_without_allocating() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let handles = loaded.handles();
    for (mode, expected) in [(0, -1710i16), (3, PPC_ERR_AE_EVENT_NOT_HANDLED)] {
        loaded.cpu.gpr[3] = scratch + 8;
        loaded.cpu.gpr[4] = scratch + 16;
        loaded.cpu.gpr[5] = mode;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AppleEventCompatibility(
                PpcAppleEventCompatibilityOperation::Send,
            ),
        );
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(expected));
        assert_eq!(loaded.handles(), handles);
    }
    let mut event = ppc_ae_descriptor(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
    )
    .unwrap();
    event
        .attributes
        .get_mut(&u32::from_be_bytes(*b"addr"))
        .unwrap()
        .data[7] = 9;
    ppc_store_ae_descriptor_semantics(
        &mut loaded.memory,
        &loaded.apple_events.descriptors,
        scratch + 8,
        event,
    );
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = scratch + 16;
    loaded.cpu.gpr[5] = 3;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::Send,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PROC_NOT_FOUND_ERR));
    assert_eq!(loaded.handles(), handles);
}

#[test]
fn aesend_direct_nested_handler_calls_release_their_own_temporaries() {
    let (mut loaded, scratch) = self_addressed_send_fixture();
    let ae = PpcImportDispatcherTarget::AppleEventCompatibility;
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"innr");
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = 18;
    loaded.cpu.gpr[7] = 42;
    loaded.cpu.gpr[8] = scratch + 24;
    run_test_import(
        &mut loaded,
        ae(PpcAppleEventCompatibilityOperation::CreateAppleEvent),
    );
    let mut nested_send = loaded.imports[0].clone();
    nested_send.symbol_index = 1;
    nested_send.symbol_name = "AESend".into();
    nested_send.dispatcher_target = ae(PpcAppleEventCompatibilityOperation::Send);
    nested_send.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.memory.add_region(nested_send.trap_pc, vec![0; 4]);
    loaded.imports.push(nested_send);
    loaded.import_count = 2;
    let outer = scratch + 0x200;
    let inner = scratch + 0x300;
    let mut outer_code = vec![
        0x7c08_02a6,
        d_form_u(36, 0, 1, 8),
        d_form_u(37, 1, 1, (-96i16) as u16),
        d_form_u(14, 3, 5, 24),
        d_form_u(14, 4, 5, 32),
        d_form_u(14, 5, 0, 1),
        d_form_u(14, 6, 0, 0),
        d_form_u(14, 7, 0, u16::MAX),
    ];
    let call_pc = outer + outer_code.len() as u32 * 4;
    outer_code.push(0x4800_0001 | ((PPC_IMPORT_TRAP_BASE + 4).wrapping_sub(call_pc) & 0x03ff_fffc));
    outer_code.extend([
        d_form_u(14, 1, 1, 96),
        d_form_u(32, 0, 1, 8),
        0x7c08_03a6,
        0x4e80_0020,
    ]);
    let inner_code = vec![
        d_form_u(14, 6, 0, 1),
        d_form_u(36, 6, 5, 0),
        d_form_u(14, 3, 0, 0),
        0x4e80_0020,
    ];
    for (entry, code) in [(outer, outer_code), (inner, inner_code)] {
        for (index, word) in code.iter().enumerate() {
            loaded
                .memory
                .write_u32_be(entry + index as u32 * 4, *word)
                .unwrap();
        }
    }
    for (tvector, entry, id, refcon) in [
        (scratch + 0x80, outer, *b"send", scratch),
        (scratch + 0x88, inner, *b"innr", scratch + 0x100),
    ] {
        loaded.memory.write_u32_be(tvector, entry).unwrap();
        loaded
            .memory
            .write_u32_be(tvector + 4, loaded.rtoc)
            .unwrap();
        loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEST");
        loaded.cpu.gpr[4] = u32::from_be_bytes(id);
        loaded.cpu.gpr[5] = tvector;
        loaded.cpu.gpr[6] = refcon;
        loaded.cpu.gpr[7] = 0;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::AEInstallEventHandler,
        );
    }
    let handles = loaded.handles().len();
    let pointers = loaded.ptrs();
    loaded.imports[0].dispatcher_target = ae(PpcAppleEventCompatibilityOperation::Send);
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = scratch + 8;
    loaded.cpu.gpr[4] = scratch + 16;
    loaded.cpu.gpr[5] = 3;
    loaded.cpu.gpr[6] = 0;
    loaded.cpu.gpr[7] = u32::MAX;
    let probe = loaded.run_with_hle_imports(256);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(probe.handled_import_count, 2);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x100), Some(1));
    assert!(loaded.apple_events.pending_dispatches.is_empty());
    assert_eq!(loaded.handles().len(), handles + 1);
    assert_eq!(loaded.ptrs(), pointers);
}
