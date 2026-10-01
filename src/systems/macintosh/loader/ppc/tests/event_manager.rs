use super::dispatch_event::{
    PPC_APPLICATION_EVENT_TARGET_REF, PPC_EVENT_DISPATCHER_TARGET_REF, PPC_MAIN_EVENT_LOOP_REF,
    PPC_MAIN_EVENT_QUEUE_REF,
};
use super::*;

#[test]
fn carbon_event_refs_are_process_owned_and_release_invalidates_them() {
    for symbol in [
        "CreateEvent",
        "ReleaseEvent",
        "GetEventClass",
        "GetEventKind",
        "GetEventTime",
    ] {
        assert_ne!(
            dispatcher_target_for_import("CarbonLib", symbol),
            PpcImportDispatcherTarget::Unsupported
        );
    }
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"CreateEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let out_ref = PPC_DATA_BASE + 0x2400;
    loaded.memory.add_region(out_ref, vec![0; 4]);
    loaded.set_tick_count(120);
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"test");
    loaded.cpu.gpr[5] = 7;
    loaded.cpu.fpr[1] = 0.0f64.to_bits();
    loaded.cpu.gpr[8] = 0;
    loaded.cpu.gpr[9] = out_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CreateEvent);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let event_ref = loaded.memory.read_u32_be(out_ref).unwrap();
    loaded.cpu.gpr[3] = event_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetEventClass);
    assert_eq!(loaded.cpu.gpr[3], u32::from_be_bytes(*b"test"));
    loaded.cpu.gpr[3] = event_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetEventKind);
    assert_eq!(loaded.cpu.gpr[3], 7);
    loaded.cpu.gpr[3] = event_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetEventTime);
    let event_time = f64::from_bits(loaded.cpu.fpr[1]);
    assert!((2.0..3.0).contains(&event_time));
    loaded.cpu.gpr[3] = event_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::ReleaseEvent);
    assert!(loaded.toolbox_startup.carbon_events.is_empty());
    loaded.cpu.gpr[3] = event_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetEventClass);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn carbon_event_parameters_copy_replace_and_report_metadata() {
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"SetEventParameter");
    let mut loaded = load_pef_application(&pef).unwrap();
    let base = PPC_DATA_BASE + 0x2500;
    loaded.memory.add_region(base, vec![0; 64]);
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"test");
    loaded.cpu.gpr[5] = 7;
    loaded.cpu.fpr[1] = 1.0f64.to_bits();
    loaded.cpu.gpr[8] = 0;
    loaded.cpu.gpr[9] = base;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::CreateEvent);
    let event_ref = loaded.memory.read_u32_be(base).unwrap();
    loaded.memory.write_bytes(base + 4, &[1, 2, 3, 4]).unwrap();
    loaded.cpu.gpr[3] = event_ref;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"data");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"long");
    loaded.cpu.gpr[6] = 4;
    loaded.cpu.gpr[7] = base + 4;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetEventParameter);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded.memory.write_bytes(base + 4, &[9, 9, 9, 9]).unwrap();
    loaded.cpu.gpr[3] = event_ref;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"data");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"****");
    loaded.cpu.gpr[6] = base + 8;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = base + 12;
    loaded.cpu.gpr[9] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetEventParameter);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(base + 8),
        Some(u32::from_be_bytes(*b"long"))
    );
    assert_eq!(loaded.memory.read_u32_be(base + 12), Some(4));
    loaded.cpu.gpr[3] = event_ref;
    loaded.cpu.gpr[7] = 4;
    loaded.cpu.gpr[9] = base + 16;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetEventParameter);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, base + 16, 4),
        Some(vec![1, 2, 3, 4])
    );
    loaded.memory.write_bytes(base + 4, &[5, 6]).unwrap();
    loaded.cpu.gpr[3] = event_ref;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"data");
    loaded.cpu.gpr[5] = u32::from_be_bytes(*b"shor");
    loaded.cpu.gpr[6] = 2;
    loaded.cpu.gpr[7] = base + 4;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetEventParameter);
    assert_eq!(loaded.toolbox_startup.carbon_events[0].parameters.len(), 1);
    assert_eq!(
        loaded.toolbox_startup.carbon_events[0].parameters[0].data,
        vec![5, 6]
    );
    loaded.cpu.gpr[3] = event_ref;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"none");
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetEventParameter);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-9870));
}

#[test]
fn carbon_queue_retains_orders_and_flushes_events() {
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"PostEventToQueue");
    let mut loaded = load_pef_application(&pef).unwrap();
    let out_ref = PPC_DATA_BASE + 0x2600;
    loaded.memory.add_region(out_ref, vec![0; 4]);
    let mut refs = Vec::new();
    for kind in [1, 2] {
        loaded.cpu.gpr[4] = u32::from_be_bytes(*b"test");
        loaded.cpu.gpr[5] = kind;
        loaded.cpu.fpr[1] = 1.0f64.to_bits();
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = out_ref;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::CreateEvent);
        refs.push(loaded.memory.read_u32_be(out_ref).unwrap());
    }
    for (event_ref, priority) in [(refs[0], 0), (refs[1], 2)] {
        loaded.cpu.gpr[3] = PPC_MAIN_EVENT_QUEUE_REF;
        loaded.cpu.gpr[4] = event_ref;
        loaded.cpu.gpr[5] = priority;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::PostEventToQueue);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        loaded.cpu.gpr[3] = event_ref;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::ReleaseEvent);
    }
    assert_eq!(loaded.toolbox_startup.carbon_event_queue[0].0, refs[1]);
    assert_eq!(loaded.toolbox_startup.carbon_event_queue[1].0, refs[0]);
    assert!(loaded
        .toolbox_startup
        .carbon_events
        .iter()
        .all(|event| event.reference_count == 1));
    loaded.cpu.gpr[3] = PPC_MAIN_EVENT_QUEUE_REF;
    loaded.cpu.gpr[4] = refs[0];
    loaded.cpu.gpr[5] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::PostEventToQueue);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-9860));
    loaded.cpu.gpr[3] = PPC_MAIN_EVENT_QUEUE_REF;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FlushEventQueue);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.toolbox_startup.carbon_event_queue.is_empty());
    assert!(loaded.toolbox_startup.carbon_events.is_empty());
}

#[test]
fn carbon_application_event_handlers_keep_process_owned_targets_and_type_specs() {
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"InstallEventHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::InstallEventHandler
    );
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetApplicationEventTarget,
    );
    let application_target = loaded.cpu.gpr[3];
    assert_eq!(application_target, PPC_APPLICATION_EVENT_TARGET_REF);
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetEventDispatcherTarget,
    );
    assert_eq!(loaded.cpu.gpr[3], PPC_EVENT_DISPATCHER_TARGET_REF);
    assert_ne!(application_target, loaded.cpu.gpr[3]);

    let type_list = PPC_DATA_BASE + 0x2000;
    let callback = PPC_DATA_BASE + 0x2100;
    let out_ref = PPC_DATA_BASE + 0x2200;
    loaded.memory.add_region(type_list, vec![0; 16]);
    loaded
        .memory
        .write_u32_be(type_list, u32::from_be_bytes(*b"keyb"))
        .unwrap();
    loaded.memory.write_u32_be(type_list + 4, 1).unwrap();
    loaded
        .memory
        .write_u32_be(type_list + 8, u32::from_be_bytes(*b"mous"))
        .unwrap();
    loaded.memory.write_u32_be(type_list + 12, 2).unwrap();
    loaded
        .memory
        .add_region(callback, vec![0x4e, 0x80, 0x00, 0x20]); // blr
    loaded.memory.add_region(out_ref, vec![0; 4]);
    loaded.cpu.gpr[3] = application_target;
    loaded.cpu.gpr[4] = callback;
    loaded.cpu.gpr[5] = 2;
    loaded.cpu.gpr[6] = type_list;
    loaded.cpu.gpr[7] = 0x1234_5678;
    loaded.cpu.gpr[8] = out_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InstallEventHandler);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let handler_ref = loaded.memory.read_u32_be(out_ref).unwrap();
    let handler = &loaded.toolbox_startup.carbon_event_handlers[0];
    assert_eq!(handler.handler_ref, handler_ref);
    assert_eq!(handler.target, application_target);
    assert_eq!(handler.callback.entry, callback);
    assert_eq!(handler.user_data, 0x1234_5678);
    assert_eq!(
        handler.event_types,
        vec![
            (u32::from_be_bytes(*b"keyb"), 1),
            (u32::from_be_bytes(*b"mous"), 2)
        ]
    );
    loaded.memory.write_u32_be(type_list, 0).unwrap();
    assert_eq!(
        loaded.toolbox_startup.carbon_event_handlers[0].event_types[0].0,
        u32::from_be_bytes(*b"keyb")
    );

    loaded.cpu.gpr[3] = handler_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RemoveEventHandler);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.toolbox_startup.carbon_event_handlers.is_empty());
    loaded.cpu.gpr[3] = handler_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RemoveEventHandler);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = callback;
    loaded.cpu.gpr[5] = 2;
    loaded.cpu.gpr[6] = type_list;
    loaded.cpu.gpr[8] = out_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InstallEventHandler);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    loaded.cpu.gpr[3] = application_target;
    loaded.cpu.gpr[4] = u32::MAX;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InstallEventHandler);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    loaded.cpu.gpr[4] = callback;
    loaded.cpu.gpr[6] = u32::MAX - 3;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::InstallEventHandler);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(loaded.toolbox_startup.carbon_event_handlers.is_empty());
    assert_eq!(loaded.memory.read_u32_be(out_ref), Some(handler_ref));
}

#[test]
fn carbon_event_loop_timer_fires_during_event_poll_and_can_be_removed() {
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "WaitNextEvent"),
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)
    );
    for symbol in [
        "GetMainEventLoop",
        "InstallEventLoopTimer",
        "RemoveEventLoopTimer",
    ] {
        let binding = PpcImportBindingPlan::prepare(
            vec![PefResolvedImport {
                library_index: 0,
                symbol_index: 0,
                library_name: "CarbonLib".to_string(),
                symbol_name: symbol.to_string(),
                class: 2,
                weak: true,
            }],
            1,
            0,
            ppc_import_layout(),
            &SystemlessPpcImportBindingPolicy,
        )
        .unwrap()
        .into_initial_bindings();
        assert_eq!(binding[0].address, PPC_IMPORT_TVECTOR_BASE);
    }

    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"InstallEventLoopTimer");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::InstallEventLoopTimer
    );
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetMainEventLoop);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_EVENT_LOOP_REF);
    let out_ref = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        4,
        true,
    );
    let user_data = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        4,
        true,
    );
    let callback = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        12,
        true,
    );
    // li r5,$1234; stw r5,0(r4); blr. r4 is the documented userData.
    loaded.memory.write_u32_be(callback, 0x38a0_1234).unwrap();
    loaded
        .memory
        .write_u32_be(callback + 4, 0x90a4_0000)
        .unwrap();
    loaded.memory.write_u32_be(callback + 8, BLR).unwrap();

    loaded.set_tick_count(10);
    loaded.set_clock_cycle_timing(1_000, 0);
    loaded.cpu.gpr[3] = PPC_MAIN_EVENT_LOOP_REF;
    loaded.cpu.fpr[1] = 0.0f64.to_bits();
    loaded.cpu.fpr[2] = 0.0f64.to_bits();
    loaded.cpu.gpr[8] = callback;
    loaded.cpu.gpr[9] = user_data;
    loaded.cpu.gpr[10] = out_ref;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InstallEventLoopTimer,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let timer_ref = loaded.memory.read_u32_be(out_ref).unwrap();
    assert_ne!(timer_ref, 0);
    assert_eq!(loaded.toolbox_startup.event_loop_timers.len(), 1);

    loaded.cpu.gpr[3] = PPC_MAIN_EVENT_LOOP_REF;
    loaded.cpu.gpr[10] = u32::MAX;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::InstallEventLoopTimer,
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.toolbox_startup.event_loop_timers.len(), 1);

    // An elapsed tick alone cannot fire a Carbon event-loop timer.
    assert!(loaded
        .fire_event_loop_timers_for_ticks(10, 1, 8, 64, false, false)
        .is_empty());
    assert_eq!(loaded.memory.read_u32_be(user_data), Some(0));

    loaded.set_tick_count(11);
    loaded.cpu.gpr[3] = u32::from(u16::MAX);
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent),
    );
    let polling_tick = loaded.toolbox_startup.event_loop_poll_until_tick.unwrap();
    assert!(super::dispatch_event::ppc_tick_is_due(
        polling_tick,
        loaded.toolbox_startup.event_loop_timers[0]
            .next_fire_tick
            .unwrap(),
    ));
    let probes = loaded.fire_event_loop_timers_for_ticks(
        polling_tick.wrapping_sub(1),
        1,
        8,
        64,
        false,
        false,
    );
    assert_eq!(probes.len(), 1);
    assert_eq!(probes[0].invocation.task_ptr, timer_ref);
    assert_eq!(loaded.memory.read_u32_be(user_data), Some(0x1234));
    assert_eq!(
        loaded.toolbox_startup.event_loop_timers[0].next_fire_tick,
        None
    );

    loaded.cpu.gpr[3] = timer_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RemoveEventLoopTimer);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.toolbox_startup.event_loop_timers.is_empty());
    loaded.cpu.gpr[3] = timer_ref;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::RemoveEventLoopTimer);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
}

#[test]
fn carbon_main_event_queue_ref_is_stable_and_flushes_pending_events() {
    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"GetMainEventQueue");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::GetMainEventQueue
    );
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetMainEventQueue);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_EVENT_QUEUE_REF);
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetMainEventQueue);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_EVENT_QUEUE_REF);

    let pef = synthetic_pef_with_library_import(b"CarbonLib", b"FlushEventQueue");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::FlushEventQueue
    );
    loaded.set_event_queue([
        PpcQueuedEvent {
            what: 3,
            message: 0x4120,
            when: 1,
            where_v: 20,
            where_h: 30,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 23,
            message: PPC_CORE_EVENT_CLASS,
            when: 1,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        },
    ]);

    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FlushEventQueue);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.event_queue().len(), 2);

    loaded.cpu.gpr[3] = PPC_MAIN_EVENT_QUEUE_REF;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::FlushEventQueue);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(loaded.event_queue().is_empty());
}

#[test]
fn event_avail_peeks_without_consuming_matching_event() {
    let queue = VecDeque::from([PpcQueuedEvent {
        what: 3,
        message: 0x0000_4120,
        when: 0,
        where_v: 120,
        where_h: 240,
        modifiers: 0x0080,
    }]);

    let event = ppc_peek_event(&queue, 1 << 3, PpcInputSnapshot::default(), false, 7);

    assert_eq!(event, (3, 0x0000_4120, 0, 120, 240, 0x0080, true));
    assert_eq!(queue.len(), 1);
}

#[test]
fn os_event_accessors_skip_toolbox_and_high_level_events() {
    let mut queue = VecDeque::from([
        PpcQueuedEvent {
            what: 6,
            message: 0x1000,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 23,
            message: PPC_CORE_EVENT_CLASS,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 3,
            message: 0x0000_4120,
            when: 0,
            where_v: 120,
            where_h: 240,
            modifiers: 0x0080,
        },
    ]);

    // Macintosh Toolbox Essentials (1992), pp. 2-97--2-99:
    // GetOSEvent and OSEventAvail return only low-level events from the
    // Operating System event queue, never update or high-level events.
    let event = ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), true, 7);

    assert_eq!(event, (3, 0x0000_4120, 0, 120, 240, 0x0080, true));
    assert_eq!(queue.len(), 2);
    assert_eq!(queue[0].what, 6);
    assert_eq!(queue[1].what, 23);
    assert_eq!(
        ppc_peek_event(&queue, u16::MAX, PpcInputSnapshot::default(), true, 7),
        (0, 0, 7, 0, 0, 0, false)
    );
}

#[test]
fn toolbox_event_accessors_apply_documented_event_priority() {
    let mut queue = VecDeque::from([
        PpcQueuedEvent {
            what: 6,
            message: 0x1000,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 23,
            message: PPC_CORE_EVENT_CLASS,
            when: 0,
            where_v: 0,
            where_h: 0,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: 120,
            where_h: 240,
            modifiers: 0,
        },
        PpcQueuedEvent {
            what: 8,
            message: 0x2000,
            when: 0,
            where_v: 10,
            where_h: 20,
            modifiers: 1,
        },
    ]);

    let event = ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), false, 7);
    assert_eq!(event.0, 8, "activate events have highest priority");
    let event = ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), false, 7);
    assert_eq!(event.0, 1, "user input precedes update events");
    let event = ppc_dequeue_event(&mut queue, u16::MAX, PpcInputSnapshot::default(), false, 7);
    assert_eq!(event.0, 6, "update events precede high-level events");
    assert_eq!(queue.front().map(|event| event.what), Some(23));
}

#[test]
fn input_snapshot_updates_powerpc_low_memory_device_state() {
    use crate::memory::globals::addr;

    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut key_map = [0; PPC_KEY_MAP_SIZE as usize];
    key_map[3] = 0x40;
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map,
        mouse_button: true,
        mouse_v: 123,
        mouse_h: 456,
    });

    assert_eq!(loaded.memory.read_u8(addr::MB_STATE), Some(0));
    assert_eq!(loaded.memory.read_u8(addr::KEY_MAP_LM + 3), Some(0x40));
    for point_addr in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
        assert_eq!(loaded.memory.read_u16_be(point_addr), Some(123));
        assert_eq!(loaded.memory.read_u16_be(point_addr + 2), Some(456));
    }

    loaded.set_input_snapshot(PpcInputSnapshot::default());
    assert_eq!(loaded.memory.read_u8(addr::MB_STATE), Some(0x80));
}

#[test]
fn hle_import_runner_posts_events_with_the_current_mouse_position() {
    let pef = synthetic_pef_with_import(b"PostEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(1_000, 0);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 123,
        mouse_h: 456,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 0x3120;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.event_queue(),
        &VecDeque::from([PpcQueuedEvent {
            what: 3,
            message: 0x3120,
            when: 42,
            where_v: 123,
            where_h: 456,
            modifiers: 0x0080,
        }])
    );
}

#[test]
fn hle_post_event_uses_current_button_and_modifier_state() {
    let pef = synthetic_pef_with_import(b"PostEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mut key_map = [0; PPC_KEY_MAP_SIZE as usize];
    for key_code in [0x37_u8, 0x38, 0x3A, 0x3B] {
        key_map[usize::from(key_code >> 3)] |= 1 << (key_code & 0x07);
    }
    let posted_at = 0x1020_3040;
    loaded.set_tick_count(posted_at);
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map,
        mouse_button: true,
        mouse_v: 123,
        mouse_h: 456,
    });
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 0xA1B2_C3D4;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::PostEvent);
    let expected_when = posted_at.wrapping_add(4);

    assert_eq!(
        loaded.event_queue().front(),
        Some(PpcQueuedEvent {
            what: 3,
            message: 0xA1B2_C3D4,
            when: expected_when,
            where_v: 123,
            where_h: 456,
            modifiers: 0x1B00,
        })
    );
    assert_eq!(
        loaded.toolbox_startup.event_queue_probe.post_result,
        Some(PPC_NO_ERR)
    );

    loaded.set_event_queue([]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        key_map,
        mouse_button: false,
        mouse_v: 123,
        mouse_h: 456,
    });
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 0x0102_0304;

    run_test_import(&mut loaded, PpcImportDispatcherTarget::PostEvent);

    let event = loaded
        .event_queue()
        .front()
        .expect("PostEvent must enqueue");
    assert_eq!(event.message, 0x0102_0304);
    assert_eq!(event.modifiers, 0x1B80);
    assert_eq!(event.when, expected_when);
}

#[test]
fn hle_import_runner_posts_events_through_sys_evt_mask_low_memory() {
    let pef = synthetic_pef_with_import(b"PostEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let mask_addr = crate::memory::globals::addr::SYS_EVT_MASK;
    assert_eq!(
        loaded.memory.read_u16_be(mask_addr),
        Some(crate::memory::globals::DEFAULT_SYS_EVT_MASK)
    );

    loaded.cpu.gpr[3] = 4;
    loaded.cpu.gpr[4] = 0x1234;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_EVT_NOT_ENB));
    assert!(loaded.event_queue().is_empty());

    loaded.memory.write_u16_be(mask_addr, 0xffff).unwrap();
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 4;
    loaded.cpu.gpr[4] = 0x5678;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.event_queue().front().map(|event| event.what),
        Some(4)
    );
    assert_eq!(
        loaded.event_queue().front().map(|event| event.message),
        Some(0x5678)
    );
}

#[test]
fn hle_import_runner_handles_event_button_and_exit_utilities() {
    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: false,
        mouse_v: 123,
        mouse_h: 456,
        ..PpcInputSnapshot::default()
    });
    loaded.set_clock_cycle_timing(1_000, 0);
    loaded.cpu.gpr[3] = 0xffff;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(0));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 2), Some(0));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 6), Some(0));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 10), Some(123));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 12), Some(456));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 14), Some(0));

    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1080;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.set_tick_count(42);
    loaded.set_clock_cycle_timing(1_000, 0);
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (0x31 << 8) | 0x20,
        when: 42,
        where_v: 240,
        where_h: 320,
        modifiers: 0x0080,
    }]);
    loaded.cpu.gpr[3] = 0x0008;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(3));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 2), Some(0x3120));
    assert_eq!(loaded.memory.read_u32_be(event_ptr + 6), Some(42));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 10), Some(240));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 12), Some(320));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 14), Some(0x0080));
    assert!(loaded.event_queue().is_empty());

    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1090;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 17,
        mouse_h: 19,
        ..PpcInputSnapshot::default()
    });
    loaded.set_event_queue([PpcQueuedEvent {
        what: 3,
        message: (0x31 << 8) | 0x20,
        when: 0,
        where_v: 240,
        where_h: 320,
        modifiers: 0x0080,
    }]);
    loaded.cpu.gpr[3] = 0x0002;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(0));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 10), Some(17));
    assert_eq!(loaded.memory.read_u16_be(event_ptr + 12), Some(19));
    assert_eq!(loaded.event_queue().len(), 1);

    let pef = synthetic_pef_with_import(b"WaitNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(event_ptr, vec![0xaa; 16]);
    loaded.imports[0].symbol_name = "GetNextEvent".to_string();
    loaded.cpu.gpr[3] = 0xffff;
    loaded.cpu.gpr[4] = event_ptr;
    loaded.cpu.gpr[5] = 10;
    loaded.cpu.gpr[6] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u16_be(event_ptr), Some(0));
    assert!(ppc_run_result_cycles(probe.result) >= PPC_Q3_IDLE_STATE_ONLY_FRAME_EXTRA_CYCLES);

    let pef = synthetic_pef_with_import(b"Button");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    let pef = synthetic_pef_with_import(b"StillDown");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    let pef = synthetic_pef_with_import(b"StillDown");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    });
    loaded.set_event_queue([PpcQueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 170,
        where_h: 352,
        modifiers: 0x0080,
    }]);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.event_queue().len(), 1);

    let pef = synthetic_pef_with_import(b"GetMouse");
    let mut loaded = load_pef_application(&pef).unwrap();
    let point_ptr = PPC_DATA_BASE + 0x1120;
    loaded.memory.add_region(point_ptr, vec![0xaa; 4]);
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 250,
        mouse_h: 320,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.gpr[3] = point_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(point_ptr), Some(250));
    assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(320));

    let pef = synthetic_pef_with_import(b"ExitToShell");
    let mut loaded = load_pef_application(&pef).unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
}

#[test]
fn get_mouse_returns_current_port_local_coordinates() {
    let pef = synthetic_pef_with_import(b"GetMouse");
    let mut loaded = load_pef_application(&pef).unwrap();
    let point_ptr = PPC_DATA_BASE + 0x1120;
    loaded.memory.add_region(point_ptr, vec![0xaa; 4]);
    ppc_write_rect(&mut loaded.memory, PPC_MAIN_PIXMAP + 6, -60, -80, 540, 720).unwrap();
    loaded.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 360,
        mouse_h: 580,
        ..PpcInputSnapshot::default()
    });
    loaded.cpu.gpr[3] = point_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(point_ptr), Some(300));
    assert_eq!(loaded.memory.read_u16_be(point_ptr + 2), Some(500));
}

#[test]
fn getkeys_poll_fast_forward_requires_repeated_idle_caller() {
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    let mut memory = PpcSectionMem::new();
    memory.add_region(key_map_ptr, vec![0xaa; PPC_KEY_MAP_SIZE as usize]);
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = key_map_ptr;
    cpu.lr = 0x0100_46B8;
    let mut idle_poll_counts = HashMap::new();

    for _ in 0..PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action = dispatch_getkeys_import(
            &mut cpu,
            &mut memory,
            PpcInputSnapshot::default(),
            Some(&mut idle_poll_counts),
        );
        assert_eq!(action, PpcImportAction::ReturnPreserve);
    }
    let action = dispatch_getkeys_import(
        &mut cpu,
        &mut memory,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(
        action,
        PpcImportAction::ReturnPreserveWithExtraCycles(PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES)
    );
    for offset in 0..PPC_KEY_MAP_SIZE {
        assert_eq!(memory.read_u8(key_map_ptr + offset), Some(0));
    }

    cpu.lr = 0x0100_4734;
    let action = dispatch_getkeys_import(
        &mut cpu,
        &mut memory,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::ReturnPreserve);

    for _ in 0..=PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action =
            dispatch_getkeys_import(&mut cpu, &mut memory, PpcInputSnapshot::default(), None);
        assert_eq!(
            action,
            PpcImportAction::ReturnPreserve,
            "exact paths must not fast-forward GetKeys"
        );
    }

    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    let action = dispatch_getkeys_import(&mut cpu, &mut memory, input, Some(&mut idle_poll_counts));
    assert_eq!(action, PpcImportAction::ReturnPreserve);
    assert!(idle_poll_counts.is_empty());
}

#[test]
fn button_poll_fast_forward_requires_repeated_idle_caller() {
    let mut cpu = PpcCpu::new();
    cpu.lr = 0x0102_5D14;
    let mut idle_poll_counts = HashMap::new();

    for _ in 0..PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action = dispatch_button_import(
            &cpu,
            PpcInputSnapshot::default(),
            Some(&mut idle_poll_counts),
        );
        assert_eq!(action, PpcImportAction::Return(0));
    }
    let action = dispatch_button_import(
        &cpu,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(
        action,
        PpcImportAction::ReturnWithExtraCycles(0, PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES)
    );

    cpu.lr = 0x0102_5E00;
    let action = dispatch_button_import(
        &cpu,
        PpcInputSnapshot::default(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::Return(0));

    let action = dispatch_button_import(
        &cpu,
        PpcInputSnapshot {
            mouse_button: true,
            ..PpcInputSnapshot::default()
        },
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::Return(1));
    assert!(idle_poll_counts.is_empty());
}

#[test]
fn still_down_requires_pressed_button_without_pending_mouse_events() {
    let mut cpu = PpcCpu::new();
    cpu.lr = 0x0102_5D14;
    let mut idle_poll_counts = HashMap::new();
    let pressed = PpcInputSnapshot {
        mouse_button: true,
        ..PpcInputSnapshot::default()
    };

    let action = dispatch_still_down_import(&cpu, pressed, &VecDeque::new(), None);
    assert_eq!(action, PpcImportAction::Return(1));

    let event_queue = VecDeque::from([PpcQueuedEvent {
        what: 1,
        message: 0,
        when: 0,
        where_v: 172,
        where_h: 352,
        modifiers: 0x0080,
    }]);
    let action =
        dispatch_still_down_import(&cpu, pressed, &event_queue, Some(&mut idle_poll_counts));
    assert_eq!(action, PpcImportAction::Return(0));
    idle_poll_counts.clear();

    for _ in 0..PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD {
        let action = dispatch_still_down_import(
            &cpu,
            PpcInputSnapshot::default(),
            &VecDeque::new(),
            Some(&mut idle_poll_counts),
        );
        assert_eq!(action, PpcImportAction::Return(0));
    }
    let action = dispatch_still_down_import(
        &cpu,
        PpcInputSnapshot::default(),
        &VecDeque::new(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(
        action,
        PpcImportAction::ReturnWithExtraCycles(0, PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES)
    );

    cpu.lr = 0;
    let action = dispatch_still_down_import(
        &cpu,
        PpcInputSnapshot::default(),
        &VecDeque::new(),
        Some(&mut idle_poll_counts),
    );
    assert_eq!(action, PpcImportAction::Return(0));
    assert!(idle_poll_counts.is_empty());
}

#[test]
fn hle_import_runner_event_time_outputs_are_all_or_nothing() {
    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    loaded.set_input_snapshot(input);
    loaded.memory.add_region(key_map_ptr, vec![0xcc; 4]);
    loaded.cpu.gpr[3] = key_map_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for offset in 0..4 {
        assert_eq!(loaded.memory.read_u8(key_map_ptr + offset), Some(0xcc));
    }

    let pef = synthetic_pef_with_import(b"Microseconds");
    let mut loaded = load_pef_application(&pef).unwrap();
    let microseconds_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(microseconds_ptr, vec![0xdd; 4]);
    loaded.cpu.gpr[3] = microseconds_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.memory.read_u32_be(microseconds_ptr),
        Some(0xdddd_dddd)
    );

    let pef = synthetic_pef_with_import(b"GetDateTime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let secs_ptr = PPC_DATA_BASE + 0x1180;
    loaded.memory.add_region(secs_ptr, vec![0xbb; 2]);
    loaded.cpu.gpr[3] = secs_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(secs_ptr), Some(0xbbbb));

    let pef = synthetic_pef_with_import(b"GetNextEvent");
    let mut loaded = load_pef_application(&pef).unwrap();
    let event_ptr = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(event_ptr, vec![0xee; 4]);
    loaded.cpu.gpr[3] = 0xffff;
    loaded.cpu.gpr[4] = event_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(event_ptr), Some(0xeeee_eeee));
}

#[test]
fn import_bindings_classify_event_manager_imports() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetNextEvent"),
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent)
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "WaitNextEvent"),
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)
    );
    assert_eq!(
        dispatcher_target_for_import("CarbonLib", "WaitNextEvent"),
        PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::WaitNextEvent)
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetOSEvent"),
        PpcImportDispatcherTarget::GetOSEvent
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "OSEventAvail"),
        PpcImportDispatcherTarget::OSEventAvail
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PostEvent"),
        PpcImportDispatcherTarget::PostEvent
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "Button"),
        PpcImportDispatcherTarget::Button
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetKeys"),
        PpcImportDispatcherTarget::GetKeys
    );
}

#[test]
fn hle_import_runner_handles_get_keys() {
    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    loaded
        .memory
        .add_region(key_map_ptr, vec![0xaa; PPC_KEY_MAP_SIZE as usize]);
    loaded.cpu.gpr[3] = key_map_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for offset in 0..PPC_KEY_MAP_SIZE {
        assert_eq!(loaded.memory.read_u8(key_map_ptr + offset), Some(0));
    }

    let pef = synthetic_pef_with_import(b"GetKeys");
    let mut loaded = load_pef_application(&pef).unwrap();
    let key_map_ptr = PPC_DATA_BASE + 0x1000;
    let mut input = PpcInputSnapshot::default();
    input.key_map[(PPC_KEY_LEFT / 8) as usize] |= 1u8 << (PPC_KEY_LEFT % 8);
    loaded.set_input_snapshot(input);
    loaded
        .memory
        .add_region(key_map_ptr, vec![0; PPC_KEY_MAP_SIZE as usize]);
    loaded.cpu.gpr[3] = key_map_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_ne!(
        loaded
            .memory
            .read_u8(key_map_ptr + u32::from(PPC_KEY_LEFT / 8))
            .unwrap()
            & (1u8 << (PPC_KEY_LEFT % 8)),
        0
    );
}

#[test]
fn hle_run_mirrors_shared_process_input_into_powerpc_low_memory() {
    use crate::memory::globals::addr;

    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let mut key_map = [0; PPC_KEY_MAP_SIZE as usize];
    key_map[2] = 0x20;
    loaded.process_input.set_key_map_snapshot(key_map);
    loaded.process_input.set_mouse_state((115, 210), true);

    let _ = loaded.run_with_hle_imports(0);

    assert_eq!(loaded.memory.read_u8(addr::MB_STATE), Some(0));
    assert_eq!(loaded.memory.read_u8(addr::KEY_MAP_LM + 2), Some(0x20));
    assert_eq!(loaded.memory.read_u16_be(addr::MOUSE_LOC2), Some(115));
    assert_eq!(loaded.memory.read_u16_be(addr::MOUSE_LOC2 + 2), Some(210));
}
