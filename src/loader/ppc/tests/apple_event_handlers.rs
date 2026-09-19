use super::*;

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
            data: size,
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
}
