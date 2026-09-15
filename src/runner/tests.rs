    use super::*;
    use crate::audio::AudioBackend;
    use crate::guest_call::CooperativeThread;
    use crate::loader::ppc::*;
    use crate::loader::{ApplicationSizeResource, Code0Header, LoadedApp};
    use crate::menu_manager::TrackedMenuPaneView;
    use crate::process_context::{
        PendingFileCompletion, ProcessFileSystemState, SharedProcessFileSystem,
        SharedProcessDisplayGamma, SharedProcessTickState, SharedProcessValue,
    };
    use crate::sound::{
        DoubleBufferState, PendingDoubleBackCallback, PendingSoundCallback, PlaybackKind,
        SndChannel, SndCommand, OUTPUT_RATE,
    };
    use crate::trap::dispatch::{
        DialogItem, DialogTrackingState, LoadedResources, PendingWaitNextEventReturn, QueuedEvent,
        ResourceFileMap, TimerTask, VblTask,
    };
    use crate::window_manager::WindowRect;
    use ppc::{PpcCpu, PpcNativeReturnGpr3};
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::rc::Rc;

    fn start_real_classic_menu_definition(runner: &mut FixtureRunner) -> u32 {
        use crate::memory::globals::addr;

        let menu = runner.bus.alloc(4);
        let record = runner.bus.alloc(64);
        let definition = runner.bus.alloc(2);
        let definition_handle = runner.bus.alloc(4);
        let entry = runner.bus.alloc(4);
        let stack = runner.bus.alloc(8);

        runner.bus.write_long(menu, record);
        runner.bus.write_word(record, 140);
        runner.bus.write_word(record + 2, 80);
        runner.bus.write_word(record + 4, 32);
        runner.bus.write_long(record + 6, definition_handle);
        runner.bus.write_long(record + 10, u32::MAX);
        runner.bus.write_bytes(
            record + 14,
            b"\x06Shared\x01A\x00\x00\x00\x00\x01B\x00\x00\x00\x00\x00",
        );
        runner.bus.write_word(definition, 0x60FE); // real guest MDEF parks while owned
        runner.bus.write_long(definition_handle, definition);
        runner.bus.write_word(stack, 0);
        runner.bus.write_long(stack + 2, menu);
        runner.m68k.cpu.write_reg(Register::A7, stack);
        runner
            .dispatcher
            .dispatch_menu(true, 0x135, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap()
            .unwrap();
        runner.dispatcher.menu_bar_hidden = false;
        runner.bus.write_word(addr::MBAR_HEIGHT, 20);
        runner.bus.write_word(addr::MENU_FLASH, 0);
        runner.dispatcher.draw_menu_bar_to_fb(&mut runner.bus);

        runner.bus.write_word(entry, 0xA93D); // MenuSelect
        runner.bus.write_word(entry + 2, 0x60FE); // park after the call
        runner.bus.write_word(stack, 10);
        runner.bus.write_word(stack + 2, 16);
        runner.bus.write_long(stack + 4, 0);
        runner.m68k.cpu.write_reg(Register::PC, entry);
        runner.m68k.cpu.write_reg(Register::A7, stack);
        runner.push_canonical_mouse_down(10, 16);

        for _ in 0..64 {
            assert!(runner.run_steps(1, None).1);
            if runner.process_context.menu_tracking().is_some()
                && runner.dispatcher.guest_calls.depth() != 0
            {
                return menu;
            }
        }
        panic!("classic MenuSelect did not enter its real guest MDEF continuation");
    }

    #[test]
    fn classic_runner_constructs_migrated_services_from_one_owner() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let process_handles = runner.process_context.migrated_handles();
        assert!(runner
            .dispatcher
            .is_constructed_from_migrated_handles(&process_handles));

        let tick_result = runner.bus.alloc(4);
        runner
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 0x1234_5678);
        runner.m68k.cpu.write_reg(Register::A7, tick_result);
        runner
            .dispatcher
            .dispatch(0xA975, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        assert_eq!(runner.bus.read_long(tick_result), 0x1234_5678);
        assert_eq!(process_handles.ticks.current_tick(), 0x1234_5678);

        let menu = start_real_classic_menu_definition(&mut runner);
        assert_eq!(
            runner
                .process_context
                .menu_tracking()
                .expect("real classic menu root remains active")
                .menu_handle,
            menu,
        );
        assert!(runner.dispatcher.guest_calls.depth() > 0);
        assert!(runner
            .dispatcher
            .is_constructed_from_migrated_handles(&process_handles));
    }

    #[test]
    fn independent_runners_keep_migrated_services_isolated() {
        let mut first = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let second = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let first_handles = first.process_context.migrated_handles();
        let second_handles = second.process_context.migrated_handles();

        assert!(!first_handles.ticks.ptr_eq(&second_handles.ticks));
        assert!(!first_handles.execution.ptr_eq(&second_handles.execution));

        let tick_result = first.bus.alloc(4);
        first
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 0x1020_3040);
        first.m68k.cpu.write_reg(Register::A7, tick_result);
        first
            .dispatcher
            .dispatch(0xA975, &mut first.m68k.cpu, &mut first.bus)
            .unwrap();
        assert_eq!(first_handles.ticks.current_tick(), 0x1020_3040);
        assert_ne!(second_handles.ticks.current_tick(), 0x1020_3040);

        start_real_classic_menu_definition(&mut first);
        assert!(first.process_context.menu_tracking().is_some());
        assert!(first.dispatcher.guest_calls.depth() > 0);
        assert!(second.process_context.menu_tracking().is_none());
        assert!(second.dispatcher.guest_calls.is_empty());
    }

    fn halted_ppc_adapter() -> PpcLoadedApp {
        halted_ppc_app_with_sound(PpcSoundState::default())
            .ppc
            .expect("halted native fixture")
    }

    #[test]
    fn native_application_adopts_detached_populated_services_before_publication() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let handles = runner.process_context.migrated_handles();
        let mut native = load_pef_application(&crate::loader::ppc::tests::synthetic_pef_with_import(
            b"TickCount",
        ))
        .unwrap();
        native.tick_state = SharedProcessTickState::from_value(41);
        native.cpu.gpr[3] = 0xfeed_face;
        let _menu = native.toolbox_startup.execution.enter_test_menu();
        native.toolbox_startup.execution.set_menu_state(Some(
            crate::menu_manager::test_process_menu_tracking(0x1234),
        ));
        let app = LoadedApp::from_ppc(native);

        runner.init_app(&app);

        assert_eq!(handles.ticks.current_tick(), 41);
        let (steps, _) = runner.run_steps(64, None);
        assert!(steps > 0);
        let installed = runner
            .native
            .adapter_mut(NativeEngineRole::Application)
            .expect("native application installed");
        assert!(installed.is_constructed_from_migrated_handles(&handles));
        assert_eq!(installed.cpu.gpr[3], 0);
        assert_eq!(handles.ticks.current_tick(), 0);
        assert_eq!(
            installed
                .memory
                .read_u32_be(crate::memory::globals::addr::TICKS),
            Some(0)
        );
        assert_eq!(
            runner
                .process_context
                .menu_tracking()
                .map(|state| state.menu_handle),
            Some(0x1234)
        );
    }

    #[test]
    fn native_application_accepts_shared_tick_identity_across_launch_sync() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(42, 1, 0);
        let handles = runner.process_context.migrated_handles();
        handles.ticks.set_tick(7);
        let mut native = load_pef_application(&crate::loader::ppc::tests::synthetic_pef_with_import(
            b"TickCount",
        ))
        .unwrap();
        native.tick_state = handles.ticks.shared_handle();
        native.toolbox_startup.execution =
            crate::guest_call::ExecutionMenuViews::shared_from(&handles.execution);

        runner.init_ppc_app(native);

        let installed = runner
            .native
            .adapter_mut(NativeEngineRole::Application)
            .expect("native application installed");
        assert!(installed.is_constructed_from_migrated_handles(&handles));
        assert_eq!(handles.ticks.current_tick(), 42);
        assert_eq!(
            installed
                .memory
                .read_u32_be(crate::memory::globals::addr::TICKS),
            Some(42)
        );
        let (steps, _) = runner.run_steps(64, None);
        assert!(steps > 0);
        let installed = runner
            .native
            .adapter_mut(NativeEngineRole::Application)
            .expect("native application retained");
        assert!(installed.is_constructed_from_migrated_handles(&handles));
        assert_eq!(installed.cpu.gpr[3], 42);
    }

    #[test]
    fn native_companion_joins_live_classic_execution_for_both_tick_identities() {
        for already_shared in [false, true] {
            run_classic_menu_select_with_powerpc_mdef_identity(false, already_shared);
        }
    }

    #[test]
    fn staged_native_companion_conflict_preserves_owner_and_retries_same_adapter() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let handles = runner.process_context.migrated_handles();
        handles.ticks.set_tick(41);
        let mut conflict = halted_ppc_adapter();
        conflict.tick_state = SharedProcessTickState::from_value(42);
        assert!(handles.execution.begin_m68k_to_powerpc(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::PowerPc,
                entry: conflict.entry_pc,
                rtoc: conflict.rtoc,
            },
            crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
            0x1000,
            0x2000,
            None,
        ));
        let before_calls = handles.execution.clone();
        runner.stage_ppc_companion(conflict);

        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runner.run_steps(1, None);
        }));
        assert!(refused.is_err());
        assert!(runner.native.has_staged_companion());
        assert!(runner.native.companion().is_none());
        assert_eq!(handles.ticks.current_tick(), 41);
        assert_eq!(handles.execution, before_calls);

        handles.ticks.set_tick(42);
        let (steps, _) = runner.run_steps(1, None);
        assert!(steps > 0);
        assert!(!runner.native.has_staged_companion());
        assert!(runner
            .native
            .adapter_mut(NativeEngineRole::Companion)
            .expect("retained staged adapter installs on retry")
            .is_constructed_from_migrated_handles(&handles));
    }

    #[test]
    fn native_application_relaunch_refuses_two_live_execution_owners_before_publication() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let menu = start_real_classic_menu_definition(&mut runner);
        let handles = runner.process_context.migrated_handles();
        let before_calls = handles.execution.clone();
        runner.process_context.cfm_mut().next_connection_id = 77;
        let before_cfm = runner.process_context.cfm_mut().clone();
        runner
            .dispatcher
            .apple_event_launch_state
            .reset_for_launch(true);
        let before_apple_events = runner.dispatcher.apple_event_launch_state.clone();
        runner
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 0x1122_3344);
        let mut installed = halted_ppc_adapter();
        installed.cpu.gpr[31] = 0xfeed_beef;
        assert!(runner
            .native
            .install(NativeEngineRole::Application, installed)
            .is_ok());
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let conflict = app.ppc.as_mut().unwrap();
        let _menu = conflict.toolbox_startup.execution.enter_test_menu();
        conflict.toolbox_startup.execution.set_menu_state(Some(
            crate::menu_manager::test_process_menu_tracking(0x5678),
        ));

        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runner.init_app(&app);
        }));

        assert!(refused.is_err());
        assert_eq!(
            runner.native.application().unwrap().cpu.gpr[31],
            0xfeed_beef
        );
        assert_eq!(handles.execution, before_calls);
        assert_eq!(*runner.process_context.cfm_mut(), before_cfm);
        assert_eq!(
            runner.dispatcher.apple_event_launch_state,
            before_apple_events
        );
        assert_eq!(
            runner.bus.read_long(crate::memory::globals::addr::TICKS),
            0x1122_3344
        );
        assert_eq!(
            runner
                .process_context
                .menu_tracking()
                .map(|state| state.menu_handle),
            Some(menu)
        );
    }

    #[test]
    fn native_application_relaunch_preflight_preserves_nonlive_process_state() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let handles = runner.process_context.migrated_handles();
        assert!(handles.execution.begin_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        ));
        assert!(handles.execution.complete_m68k(0x2002, 0x3000));
        assert!(handles.execution.is_empty());
        assert!(!handles.execution.is_pristine());
        assert!(runner.m68k.can_relaunch());
        assert!(runner.native.can_relaunch());
        let before_calls = handles.execution.clone();
        runner.process_context.cfm_mut().next_connection_id = 77;
        let before_cfm = runner.process_context.cfm_mut().clone();
        runner
            .dispatcher
            .apple_event_launch_state
            .reset_for_launch(true);
        let before_apple_events = runner.dispatcher.apple_event_launch_state.clone();
        runner
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 0x1122_3344);
        let mut installed = halted_ppc_adapter();
        installed.cpu.gpr[31] = 0xfeed_beef;
        assert!(runner
            .native
            .install(NativeEngineRole::Application, installed)
            .is_ok());

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let conflict = app.ppc.as_mut().unwrap();
        let _menu = conflict.toolbox_startup.execution.enter_test_menu();
        conflict.toolbox_startup.execution.set_menu_state(Some(
            crate::menu_manager::test_process_menu_tracking(0x5678),
        ));
        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            runner.init_app(&app);
        }));

        assert!(refused.is_err());
        assert_eq!(
            runner.native.application().unwrap().cpu.gpr[31],
            0xfeed_beef
        );
        assert_eq!(handles.execution, before_calls);
        assert_eq!(*runner.process_context.cfm_mut(), before_cfm);
        assert_eq!(
            runner.dispatcher.apple_event_launch_state,
            before_apple_events
        );
        assert_eq!(
            runner.bus.read_long(crate::memory::globals::addr::TICKS),
            0x1122_3344
        );
    }

    fn cfm_test_connection(id: u32) -> crate::cfm::CfmConnection {
        crate::cfm::CfmConnection {
            id,
            library_name: format!("existing-{id}"),
            main_addr: 0,
            init_addr: 0,
            term_addr: 0,
            exports: vec![],
        }
    }

    #[test]
    fn find_symbol_classic_lookup_stages_native_bindings_and_returns_callable_identity() {
        use crate::loader::ppc::tests::synthetic_pef_with_import;
        const OUTPUT: u32 = PPC_HEAP_BASE + 0x1000;
        const CODE: u32 = 0x18000;
        const STACK: u32 = 0x19000;
        let mut native =
            load_pef_application(&synthetic_pef_with_import(b"GetSharedLibrary")).unwrap();
        native.memory.add_region(OUTPUT, vec![0xa5; 256]);
        native
            .memory
            .write_bytes(OUTPUT + 32, b"\x0cInterfaceLib")
            .unwrap();
        native.cpu.gpr[3] = OUTPUT + 32;
        native.cpu.gpr[4] = u32::from_be_bytes(*b"pwpc");
        native.cpu.gpr[5] = 1;
        native.cpu.gpr[6] = OUTPUT;
        native.cpu.gpr[7] = OUTPUT + 4;
        native.cpu.gpr[8] = OUTPUT + 8;
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_ppc_companion(native);
        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        let native = context.adapter_mut();
        let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
            native.run_with_process_services(128, false, false, mm, cfm)
        });
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(native.cpu.gpr[3], 0);
        let id = native.memory.read_u32_be(OUTPUT).unwrap();
        let initial_count = native.import_count;
        native
            .memory
            .write_bytes(OUTPUT + 32, b"\x09TickCount")
            .unwrap();
        native.memory.add_readonly_region(OUTPUT + 64, vec![0xa5]);
        assert!(runner.native.restore(context).is_ok());
        for attempt in 0..3 {
            for (offset, word) in [0x3f3c, 5, 0xaa5a, 0x60fe].into_iter().enumerate() {
                runner.bus.write_word(CODE + offset as u32 * 2, word);
            }
            runner
                .bus
                .write_long(STACK + 2, OUTPUT + if attempt == 0 { 64 } else { 65 });
            runner.bus.write_long(STACK + 6, OUTPUT + 60);
            runner.bus.write_long(STACK + 10, OUTPUT + 32);
            runner.bus.write_long(STACK + 14, id);
            runner.m68k.cpu.write_reg(Register::PC, CODE);
            runner.m68k.cpu.write_reg(Register::A7, STACK + 2);
            runner.m68k.cpu.write_reg(Register::D0, 0xdead_beef);
            let (steps, running) = runner.run_steps(8, None);
            assert!(running && steps > 0);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), STACK + 18);
            assert_eq!(
                runner.bus.read_word(STACK + 18) as i16,
                if attempt == 0 { -50 } else { 0 }
            );
            let native = runner.native.companion().unwrap();
            assert_eq!(native.import_count, initial_count + u32::from(attempt != 0));
            if attempt == 0 {
                assert_eq!(runner.bus.read_long(OUTPUT + 60), 0xa5a5_a5a5);
            } else {
                assert_eq!(runner.bus.read_byte(OUTPUT + 65), 2);
            }
        }
        let address = runner.bus.read_long(OUTPUT + 60);
        runner
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 0x2345);
        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        let native = context.adapter_mut();
        native.cpu.pc = native.memory.read_u32_be(address).unwrap();
        native.cpu.gpr[2] = native.memory.read_u32_be(address + 4).unwrap();
        native.cpu.lr = PPC_HALT_PC;
        let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
            native.run_with_process_services(64, false, false, mm, cfm)
        });
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(native.cpu.gpr[3], 0x2345);
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::FindSymbol;
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = id;
        native.cpu.gpr[4] = OUTPUT + 32;
        native.cpu.gpr[5] = OUTPUT + 80;
        native.cpu.gpr[6] = OUTPUT + 84;
        let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
            native.run_with_process_services(64, false, false, mm, cfm)
        });
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(native.cpu.gpr[3], 0);
        assert_eq!(native.memory.read_u32_be(OUTPUT + 80), Some(address));
        assert_eq!(native.import_count, initial_count + 1);
    }

    #[test]
    #[cfg(not(feature = "debug"))]
    fn debugger_disabled_hooks_are_inert() {
        let mut runner = FixtureRunner::new(0x100000, FixtureRunnerConfig::default());
        runner.bus_mut().write_word(0x2_0000, 0x4E71); // NOP
        runner.cpu_mut().write_reg(Register::PC, 0x2_0000);
        assert!(!runner.debug_is_paused());
        assert_eq!(runner.debug_step_units_remaining(), None);
        assert!(!runner.debug_stop_at_m68k_breakpoint(0x2_0000));
        assert!(runner.debug_m68k_breakpoint_addresses().is_empty());
        let (steps, running) = runner.run_steps(1, None);
        assert_eq!(steps, 1);
        assert!(running);
        assert!(!runner.debug_is_paused());
    }

    #[test]
    #[cfg(feature = "debug")]
    fn debug_launch_invalidates_cached_references() {
        use crate::debug::{handle_debug_request, DebugError, DebugRequest};
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let request = DebugRequest::InSession {
            session: runner.debug.session(),
            generation: runner.debug.generation(),
            request: Box::new(DebugRequest::ListContexts),
        };
        runner.init_app(&app);
        assert!(matches!(
            handle_debug_request(&mut runner, request),
            Err(DebugError::StaleReference { .. })
        ));
    }

    #[test]
    #[cfg(feature = "debug")]
    fn debug_companion_inspection_uses_its_own_cpu() {
        use crate::debug::{
            handle_debug_request, ContextId, ContextSelector, DebugReply, DebugRequest,
        };
        let mut native = halted_ppc_app_with_sound(PpcSoundState::default())
            .ppc
            .take()
            .unwrap();
        native.cpu.gpr[3] = 0x12345678;
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_ppc_companion(native);
        let reply = handle_debug_request(
            &mut runner,
            DebugRequest::ReadRegisters {
                context: ContextSelector::explicit(ContextId(3)),
                registers: None,
            },
        )
        .unwrap();
        let DebugReply::Registers { context, registers } = reply else {
            panic!("registers");
        };
        assert_eq!(context, ContextId(3));
        assert_eq!(
            registers
                .iter()
                .find(|r| r.descriptor.name == "r3")
                .unwrap()
                .value
                .as_u64(),
            Some(0x12345678)
        );
        assert!(runner.debug_ppc_cpu().is_none());
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        assert_eq!(runner.run_pending_sound_work(100), (0, true));
        assert_eq!(runner.debug_ppc_companion_cpu().unwrap().gpr[3], 0x12345678);
    }

    #[test]
    fn companion_engine_installs_native_worker_context_and_hands_back_to_classic() {
        use crate::guest_call::{
            seed_pending_native_import_context, CooperativeThread, ExecutionTaskId,
            NativeThreadContext, ThreadStorage,
        };
        use crate::guest_procedure::GuestIsa;
        const ADDRESS: u32 = PPC_DATA_BASE + 0x5000;
        const A_RETURN: u32 = 0x5678;
        const A_FINAL: u32 = 0x6678;
        const B_RETURN: u32 = 0x7678;
        const B_FINAL: u32 = 0x8678;
        const LWARX_R12_R4_R5: u32 = (31 << 26) | (12 << 21) | (4 << 16) | (5 << 11) | (20 << 1);
        let native = halted_ppc_app_with_sound(PpcSoundState::default())
            .ppc
            .take()
            .unwrap();
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_ppc_companion(native);
        let calls = runner.dispatcher.guest_calls.shared_handle();
        assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::M68k));
        let mut classic = CooperativeThread::default();
        classic.pc = 0x1234;
        assert!(calls.save_cooperative_context(ExecutionTaskId::APPLICATION, classic));
        let pending_context = |trap_pc, return_pc, final_pc, rtoc, result| {
            let mut cpu = PpcCpu::new();
            let mut memory = PpcSectionMem::new();
            seed_pending_native_import_context(
                &mut cpu,
                &mut memory,
                trap_pc,
                return_pc,
                rtoc ^ 0xffff_0000,
                return_pc,
                final_pc,
                rtoc,
                PpcNativeReturnGpr3::Set(result),
            );
            cpu.capture_execution_context()
        };
        let worker_a = calls
            .create_native_thread(
                NativeThreadContext {
                    context: pending_context(0x5000, A_RETURN, A_FINAL, 0xaaaa_0002, 0xaaaa_0003),
                },
                ThreadStorage::default(),
                false,
                |_| true,
            )
            .unwrap();
        let worker_b = calls
            .create_native_thread(
                NativeThreadContext {
                    context: pending_context(0x7000, B_RETURN, B_FINAL, 0xbbbb_0002, 0xbbbb_0003),
                },
                ThreadStorage::default(),
                false,
                |_| true,
            )
            .unwrap();
        assert_eq!(calls.switch_from_classic(worker_a), Some(None));
        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        {
            let companion = context.adapter_mut();
            assert!(calls.prepare_native_task(&mut companion.cpu));
            assert_eq!(companion.cpu.pc, A_RETURN);
            assert!(calls
                .yield_native_thread(&mut companion.cpu, worker_b.thread_id())
                .unwrap());
            assert_eq!(companion.cpu.pc, B_RETURN);
            assert_eq!(
                companion.cpu.run_with_imports(
                    &mut companion.memory,
                    2,
                    B_FINAL,
                    0,
                    0,
                    |_, _, _| unreachable!()
                ),
                PpcRunResult::Halted {
                    pc: B_FINAL,
                    cycles: 1
                }
            );
            assert_eq!(
                (companion.cpu.gpr[2], companion.cpu.gpr[3]),
                (0xbbbb_0002, 0xbbbb_0003)
            );
            assert!(calls
                .yield_native_thread(&mut companion.cpu, worker_a.thread_id())
                .unwrap());
            assert_eq!(
                companion.cpu.run_with_imports(
                    &mut companion.memory,
                    2,
                    A_FINAL,
                    0,
                    0,
                    |_, _, _| unreachable!()
                ),
                PpcRunResult::Halted {
                    pc: A_FINAL,
                    cycles: 1
                }
            );
            assert_eq!(
                (companion.cpu.gpr[2], companion.cpu.gpr[3]),
                (0xaaaa_0002, 0xaaaa_0003)
            );
            companion
                .memory
                .add_region(ADDRESS, 0x5566_7788u32.to_be_bytes().to_vec());
            companion.cpu.gpr[4] = ADDRESS;
            assert_eq!(
                companion.cpu.step(&mut companion.memory, LWARX_R12_R4_R5),
                ppc::PpcStepResult::Stepped
            );
            assert_eq!(companion.cpu.reservation_address(), Some(ADDRESS));
            assert!(calls
                .yield_native_thread(&mut companion.cpu, worker_b.thread_id())
                .unwrap());
            assert_eq!(
                companion.cpu.run_with_imports(
                    &mut companion.memory,
                    2,
                    B_FINAL,
                    0,
                    0,
                    |_, _, _| unreachable!()
                ),
                PpcRunResult::Halted {
                    pc: B_FINAL,
                    cycles: 0
                }
            );
            assert_eq!(companion.cpu.reservation_address(), None);
            assert!(calls
                .yield_native_thread(&mut companion.cpu, ExecutionTaskId::APPLICATION.thread_id())
                .unwrap());
            assert!(calls.has_classic_task_handoff());
            assert_eq!(companion.cpu.reservation_address(), None);
        }
        assert!(runner.native.restore(context).is_ok());
        assert!(runner.native.application().is_none());
        assert!(runner.native.companion().is_some());
    }

    #[test]
    fn companion_new_thread_import_installs_a_fresh_worker_on_the_live_engine() {
        use crate::guest_call::{CooperativeThread, ExecutionTaskId};
        use crate::guest_procedure::GuestIsa;
        use crate::loader::ppc::tests::synthetic_pef_with_import;
        const MADE: u32 = PPC_DATA_BASE + 0x5000;
        const THREAD_RETURN: u32 = PPC_IMPORT_TRAP_BASE + (4096 + 1) * 4;
        let native = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
        let entry = native.entry_pc;
        let expected_rtoc = native.cpu.gpr[2];
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_ppc_companion(native);
        let calls = runner.dispatcher.guest_calls.shared_handle();
        assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::M68k));
        let worker;
        let live_time;
        {
            let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
            let companion = context.adapter_mut();
            companion.memory.add_region(MADE, vec![0; 4]);
            companion.cpu.msr = 0x5060_7080;
            companion.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
            companion.cpu.set_time_base(0xffff_ffff_0000_0000);
            companion.cpu.gpr[3] = 1;
            companion.cpu.gpr[4] = entry;
            companion.cpu.gpr[5] = 0x1234_5678;
            companion.cpu.gpr[6] = 4096;
            companion.cpu.gpr[7] = 0;
            companion.cpu.gpr[8] = 0;
            companion.cpu.gpr[9] = MADE;
            let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
                companion.run_with_process_services(64, false, false, mm, cfm)
            });
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(companion.cpu.gpr[3], 0);
            worker = ExecutionTaskId::from_thread_id(companion.memory.read_u32_be(MADE).unwrap());
            live_time = companion.cpu.time_base();
            assert!(runner.native.restore(context).is_ok());
        }
        let mut classic = CooperativeThread::default();
        classic.pc = 0x1234;
        assert!(calls.save_cooperative_context(ExecutionTaskId::APPLICATION, classic));
        assert_eq!(calls.switch_from_classic(worker), Some(None));
        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        {
            let companion = context.adapter_mut();
            assert!(calls.prepare_native_task(&mut companion.cpu));
            assert_eq!(companion.cpu.pc, entry);
            assert_eq!(companion.cpu.lr, THREAD_RETURN);
            assert_eq!(companion.cpu.gpr[1] & 15, 0);
            assert_eq!(companion.cpu.gpr[2], expected_rtoc);
            assert_eq!(companion.cpu.gpr[3], 0x1234_5678);
            assert_eq!(companion.cpu.msr, 0x5060_7080);
            assert_eq!(
                companion.cpu.alignment_policy,
                ppc::PpcAlignmentPolicy::EmulateData
            );
            assert_eq!(companion.cpu.time_base(), live_time);
            assert_eq!(companion.cpu.reservation_address(), None);
        }
        assert!(runner.native.restore(context).is_ok());
    }

    #[test]
    fn find_symbol_during_native_to_classic_callback_borrows_the_checked_out_adapter() {
        use crate::guest_call::{GuestCallTarget, M68kRegisterState, M68kResultSource};
        use crate::guest_procedure::GuestIsa;
        use ppc::PpcNativeReturnGpr3;
        const CALLBACK: u32 = 0x18000;
        const STACK: u32 = 0x19000;
        const RETURN: u32 = 0x1a000;
        const OUTPUT: u32 = 0x1b000;
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let native = app.ppc.as_mut().unwrap();
        let mut connection = cfm_test_connection(7);
        connection.library_name = "InterfaceLib".into();
        native.cfm.as_mut().unwrap().connections = vec![connection];
        native.memory.add_region(CALLBACK, vec![0; 0x4000]);
        native
            .memory
            .write_bytes(OUTPUT + 32, b"\x09TickCount")
            .unwrap();
        let mut words = vec![0x3f3c, 0]; // Pascal result slot.
        for argument in [7, OUTPUT + 32, OUTPUT, OUTPUT + 4] {
            words.extend([0x2f3c, (argument >> 16) as u16, argument as u16]);
        }
        words.extend([0x3f3c, 5, 0xaa5a, 0x548f, 0x4e75]);
        for (i, word) in words.into_iter().enumerate() {
            native
                .memory
                .write_u16_be(CALLBACK + i as u32 * 2, word)
                .unwrap();
        }
        native.memory.write_u32_be(STACK, RETURN).unwrap();
        assert!(native.guest_calls().begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: CALLBACK,
                rtoc: 0
            },
            CALLBACK,
            STACK,
            RETURN,
            STACK + 4,
            M68kRegisterState::default(),
            Some(M68kResultSource::Data(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let (steps, _) = runner.run_steps(64, None);
        assert!(steps > 0);
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert_eq!(runner.bus.read_byte(OUTPUT + 4), 2);
        let native = runner.native.application().unwrap();
        assert_eq!(native.imports.len(), 1);
        assert_eq!(native.imports[0].symbol_name, "TickCount");
        assert_eq!(runner.bus.read_long(OUTPUT), native.imports[0].address);
    }

    #[test]
    fn cfm_symbol_enumeration_observes_native_load_and_close_from_classic_execution() {
        use crate::loader::ppc::tests::{
            synthetic_pef_with_enumerable_exports, synthetic_pef_with_import,
        };
        const OUTPUT: u32 = PPC_HEAP_BASE + 0x1000;
        const FRAGMENT: u32 = PPC_HEAP_BASE + 0x2000;
        const CODE: u32 = 0x18000;
        const STACK: u32 = 0x19000;
        let fragment = synthetic_pef_with_enumerable_exports();
        let mut native =
            load_pef_application(&synthetic_pef_with_import(b"GetMemFragment")).unwrap();
        native.memory.add_region(OUTPUT, vec![0xa5; 256]);
        native.memory.add_region(FRAGMENT, fragment.clone());
        native.cpu.gpr[3] = FRAGMENT;
        native.cpu.gpr[4] = fragment.len() as u32;
        native.cpu.gpr[5] = 0;
        native.cpu.gpr[6] = 1;
        native.cpu.gpr[7] = OUTPUT;
        native.cpu.gpr[8] = OUTPUT + 4;
        native.cpu.gpr[9] = OUTPUT + 8;
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_ppc_companion(native);
        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        let native = context.adapter_mut();
        let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
            native.run_with_process_services(128, false, false, mm, cfm)
        });
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(native.cpu.gpr[3], 0);
        let id = native.memory.read_u32_be(OUTPUT).unwrap();
        assert_ne!(id, 0xa5a5_a5a5);
        assert_eq!(
            runner.process_context.cfm().connections[0].exports[0].name,
            "Café™"
        );
        assert!(runner.native.restore(context).is_ok());
        for selector in [5u16, 6, 7] {
            runner.bus.write_word(CODE, 0x3f3c); // MOVE.W #selector,-(SP), Apple inline glue.
            runner.bus.write_word(CODE + 2, selector);
            runner.bus.write_word(CODE + 4, 0xaa5a);
            runner.bus.write_word(CODE + 6, 0x60fe);
            runner.m68k.cpu.write_reg(Register::PC, CODE);
            runner.m68k.cpu.write_reg(Register::A7, STACK + 2);
            runner.m68k.cpu.write_reg(Register::D0, 0xdead_beef);
            if selector == 5 {
                for (i, byte) in b"\x05Caf\x8e\xaa".iter().enumerate() {
                    runner.bus.write_byte(OUTPUT + 96 + i as u32, *byte);
                }
                runner.bus.write_long(STACK + 2, OUTPUT + 64);
                runner.bus.write_long(STACK + 6, OUTPUT + 60);
                runner.bus.write_long(STACK + 10, OUTPUT + 96);
                runner.bus.write_long(STACK + 14, id);
            } else if selector == 6 {
                runner.bus.write_long(STACK + 2, OUTPUT + 16);
                runner.bus.write_long(STACK + 6, id);
            } else {
                runner.bus.write_long(STACK + 2, OUTPUT + 64);
                runner.bus.write_long(STACK + 6, OUTPUT + 60);
                runner.bus.write_long(STACK + 10, OUTPUT + 32);
                runner.bus.write_long(STACK + 14, 1);
                runner.bus.write_long(STACK + 18, id);
            }
            let (steps, running) = runner.run_steps(8, None);
            assert!(running && steps > 0);
            assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0);
            assert_eq!(
                runner.m68k.cpu.read_reg(Register::A7),
                STACK
                    + match selector {
                        5 => 18,
                        6 => 10,
                        _ => 22,
                    }
            );
        }
        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        let native = context.adapter_mut();
        assert_eq!(native.memory.read_u32_be(OUTPUT + 16), Some(1));
        assert_eq!(native.memory.read_u32_be(OUTPUT + 60), Some(0x1234_5678));
        assert_eq!(native.memory.read_u8(OUTPUT + 64), Some(1));
        assert_eq!(native.memory.read_u8(OUTPUT + 36), Some(0x8e));
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::CloseConnection;
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = OUTPUT;
        let probe = runner.process_context.with_memory_and_cfm(|mm, cfm| {
            native.run_with_process_services(128, false, false, mm, cfm)
        });
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(native.cpu.gpr[3], 0);
        assert!(runner.process_context.cfm().connections.is_empty());
        assert!(runner.native.restore(context).is_ok());
        runner.bus.write_word(CODE + 2, 6);
        runner.m68k.cpu.write_reg(Register::PC, CODE);
        runner.m68k.cpu.write_reg(Register::A7, STACK + 2);
        runner.bus.write_long(STACK + 2, OUTPUT + 16);
        runner.bus.write_long(STACK + 6, id);
        let _ = runner.run_steps(8, None);
        assert_eq!(runner.bus.read_word(STACK + 10) as i16, -2801);
        assert_eq!(
            runner.bus.read_long(OUTPUT + 16),
            1,
            "refused query preserves its previous output"
        );
    }

    #[test]
    fn runner_cfm_owns_native_connections_ids_and_library_seeds() {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut native = load_pef_application(
            &crate::loader::ppc::tests::synthetic_pef_with_import(b"GetSharedLibrary"),
        )
        .unwrap();
        const OUTPUT: u32 = PPC_HEAP_BASE + 0x200;
        native.memory.add_region(OUTPUT, vec![0; 128]);
        native
            .memory
            .write_bytes(OUTPUT, b"\x0cInterfaceLib")
            .unwrap();
        native.cpu.gpr[3] = OUTPUT;
        native.cpu.gpr[4] = u32::from_be_bytes(*b"pwpc");
        native.cpu.gpr[5] = 1;
        native.cpu.gpr[6] = OUTPUT + 64;
        native.cpu.gpr[7] = OUTPUT + 68;
        native.cpu.gpr[8] = OUTPUT + 72;
        native
            .cfm
            .as_mut()
            .unwrap()
            .connections
            .push(cfm_test_connection(3));
        native.cfm.as_mut().unwrap().next_connection_id = 7;
        native.seed_cfm_library_fragments(vec![PpcCfmLibraryFragment {
            name: "seeded library".into(),
            bytes: vec![1, 2, 3],
        }]);
        app.ppc = Some(native);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        assert!(runner.native.application().unwrap().cfm.is_none());
        assert_eq!(runner.process_context.cfm_mut().next_connection_id, 7);
        runner.run_steps(128, None);
        assert_eq!(runner.bus.read_long(OUTPUT + 64), 7);
        assert_eq!(runner.process_context.cfm_mut().next_connection_id, 8);
        assert_eq!(
            runner
                .process_context
                .cfm_mut()
                .connections
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>(),
            vec![3, 7]
        );
        assert_eq!(
            runner.process_context.cfm_mut().library_fragments[0].bytes,
            vec![1, 2, 3]
        );
        let registry = runner.process_context.cfm_mut().clone();
        let native = runner.native.application_mut().unwrap();
        assert!(native.cfm.is_none());
        let before_cpu = native.cpu.clone();
        let refused = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            native.run_with_hle_imports(64)
        }));
        assert!(refused.is_err());
        assert_eq!(native.cpu.gpr, before_cpu.gpr);
        assert_eq!(native.cpu.pc, before_cpu.pc);
        assert_eq!(*runner.process_context.cfm_mut(), registry);
        // The caller's launch blueprint remains an independent, unchanged seed.
        assert_eq!(
            app.ppc
                .as_ref()
                .unwrap()
                .cfm
                .as_ref()
                .unwrap()
                .next_connection_id,
            7
        );
        runner.init_app(&app);
        assert_eq!(runner.process_context.cfm_mut().next_connection_id, 7);
        assert_eq!(runner.process_context.cfm_mut().connections.len(), 1);
    }

    #[test]
    fn runner_cfm_is_used_by_timer_vbl_and_sound_callback_entries() {
        use crate::callback_manager::{CallbackTaskArchitecture, ProcessTimerTask, ProcessVblTask};
        const OUTPUT: u32 = PPC_HEAP_BASE + 0x200;
        for callback_kind in 0..4 {
            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let mut native = load_pef_application(
                &crate::loader::ppc::tests::synthetic_pef_with_import(b"CloseConnection"),
            )
            .unwrap();
            native.memory.add_region(OUTPUT, vec![0; 64]);
            native.memory.write_u32_be(OUTPUT, 3).unwrap();
            let callback = native.imports[0].address;
            native.cfm.as_mut().unwrap().connections =
                vec![cfm_test_connection(3), cfm_test_connection(9)];
            native.cfm.as_mut().unwrap().next_connection_id = 10;
            app.ppc = Some(native);
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.init_app(&app);
            let native = runner.native.application_mut().unwrap();
            let saved_cpu = native.cpu.clone();
            runner
                .process_context
                .with_memory_and_cfm(|memory_manager, cfm| match callback_kind {
                    0 => {
                        native.timer_tasks.push(ProcessTimerTask {
                            task_ptr: OUTPUT,
                            architecture: CallbackTaskArchitecture::PowerPc,
                            extended: false,
                            callback,
                            active: true,
                            fire_at_tick: 1,
                            fire_at_subtick: 0,
                            last_fired_tick: None,
                        });
                        let probes = native.fire_timer_tasks_for_ticks_with_process_services(
                            0,
                            1,
                            1,
                            64,
                            false,
                            false,
                            memory_manager,
                            cfm,
                        );
                        assert_eq!(probes.len(), 1);
                        assert_eq!(probes[0].invocation.unsupported_import_index, None);
                    }
                    1 => {
                        native.memory.write_u32_be(OUTPUT + 6, callback).unwrap();
                        native.memory.write_u16_be(OUTPUT + 10, 1).unwrap();
                        native.vbl_tasks.push(ProcessVblTask {
                            task_ptr: OUTPUT,
                            architecture: CallbackTaskArchitecture::PowerPc,
                            slot: None,
                            pending: false,
                        });
                        let probes = native.fire_vbl_tasks_for_ticks_with_process_services(
                            0,
                            1,
                            1,
                            64,
                            false,
                            false,
                            memory_manager,
                            cfm,
                        );
                        assert_eq!(probes.len(), 1);
                        assert_eq!(probes[0].invocation.unsupported_import_index, None);
                    }
                    2 => {
                        let probe = native.run_sound_completion_callback_with_process_services(
                            PpcSoundCompletionRecord {
                                file_playback_index: 0,
                                channel: OUTPUT,
                                completion: callback,
                                command: None,
                                tick: 0,
                                instruction_count: 0,
                                scheduled_tick: 0,
                                scheduled_instruction_count: 0,
                            },
                            64,
                            false,
                            false,
                            memory_manager,
                            cfm,
                        );
                        assert_eq!(probe.invocation.unsupported_import_index, None);
                    }
                    _ => {
                        let probe = native.run_sound_doubleback_callback_with_process_services(
                            PpcSoundDoubleBackRecord {
                                architecture: CallbackTaskArchitecture::PowerPc,
                                channel: OUTPUT,
                                header: 0,
                                exhausted_buffer: 0,
                                exhausted_buffer_index: 0,
                                callback,
                                tick: 0,
                                instruction_count: 0,
                            },
                            64,
                            false,
                            false,
                            memory_manager,
                            cfm,
                        );
                        assert_eq!(probe.invocation.unsupported_import_index, None);
                    }
                });
            assert!(native.cfm.is_none());
            assert_eq!(native.cpu.gpr, saved_cpu.gpr);
            assert_eq!(native.cpu.pc, saved_cpu.pc);
            assert_eq!(
                runner.bus.read_long(OUTPUT),
                0,
                "callback kind {callback_kind}"
            );
            assert_eq!(
                runner
                    .process_context
                    .cfm_mut()
                    .connections
                    .iter()
                    .map(|c| c.id)
                    .collect::<Vec<_>>(),
                vec![9]
            );
            assert_eq!(runner.process_context.cfm_mut().next_connection_id, 10);
        }
    }

    #[test]
    fn os_trap_address_gateway_executes_and_returns_through_the_68k_cpu() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.dispatcher.current_trap_word = 0xA346;
        runner.m68k.cpu.write_reg(Register::D0, 0x39);
        runner
            .dispatcher
            .dispatch_memory(false, 0x46, &mut runner.m68k.cpu, &mut runner.bus)
            .expect("GetOSTrapAddress should be handled")
            .expect("GetOSTrapAddress should succeed");
        let gateway = runner.m68k.cpu.read_reg(Register::A0);
        let output = 0x0020_0000u32;
        let return_pc = 0x0020_0100u32;
        let sp = 0x007F_FF00u32;
        runner.bus.write_long(addr::TIME, 0x1234_5678);
        runner.bus.write_word(return_pc, 0x4E71);
        runner.bus.write_long(sp, return_pc);
        runner.m68k.cpu.write_reg(Register::A0, output);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.m68k.cpu.write_reg(Register::PC, gateway);

        let (steps, running) = runner.run_steps(2, None);

        assert_eq!(steps, 2);
        assert!(running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0);
        assert_eq!(runner.bus.read_long(output), 0x1234_5678);
    }

    #[test]
    fn auto_pop_native_trap_patch_returns_through_the_68k_cpu_and_retires_its_frame() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let caller = 0x0020_0000u32;
        let glue = 0x0020_0100u32;
        let handler = 0x0020_0200u32;
        let sp = 0x007F_FF00u32;
        runner.bus.write_word(caller, 0x4EB9); // JSR absolute long
        runner.bus.write_long(caller + 2, glue);
        runner.bus.write_word(caller + 6, 0x4E71); // NOP after return
        runner.bus.write_word(glue, 0xAD75); // auto-pop TickCount
        runner.bus.write_word(handler, 0x4E75); // RTS
        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, 0xA975, handler)
            .unwrap();
        runner.m68k.cpu.write_reg(Register::PC, caller);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let (steps, running) = runner.run_steps(3, None);

        assert_eq!(steps, 3);
        assert!(running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), caller + 6);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert!(runner.dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn native_trap_patch_bypasses_the_tickcount_default_operation() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap = 0x0020_0000u32;
        let handler = 0x0020_0100u32;
        let sp = 0x007F_FF00u32;
        let result_sentinel = 0xCAFE_BABEu32;
        runner.bus.write_word(trap, 0xA975);
        runner.bus.write_word(trap + 2, 0x4E71);
        runner.bus.write_word(handler, 0x4E75);
        runner.bus.write_long(sp, result_sentinel);
        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, 0xA975, handler)
            .unwrap();
        runner.m68k.cpu.write_reg(Register::PC, trap);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let (steps, running) = runner.run_steps(2, None);

        assert_eq!(steps, 2);
        assert!(running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trap + 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert_eq!(runner.bus.read_long(sp), result_sentinel);
        assert!(runner.dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn native_os_patch_observes_full_word_and_returns_through_dispatcher_frame() {
        // The OS Trap Dispatcher supplies the actual A-line in D1's low word,
        // then restores D1/D2/A1/A2 while retaining D0 and an A0 result
        // selected by bit 8. Inside Macintosh: Operating System Utilities
        // (1994), pp. 8-11--8-13.
        const TRAP_WORD: u16 = 0xA739; // ReadDateTime slot with all OS bits set
        const TRAP_PC: u32 = 0x0020_0000;
        const HANDLER: u32 = 0x0020_0100;
        const OBSERVED_D1: u32 = 0x0020_0200;
        const SP: u32 = 0x007F_FF00;
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let original_d1 = 0xD1D1_BEEF;
        let original_d2 = 0xD2D2_BEEF;
        let original_a1 = 0xA1A1_BEEF;
        let original_a2 = 0xA2A2_BEEF;

        runner.bus.write_word(TRAP_PC, TRAP_WORD);
        runner.bus.write_word(TRAP_PC + 2, 0x4E71); // NOP after return
        runner.bus.write_word(HANDLER, 0x23C1); // MOVE.L D1,abs.l
        runner.bus.write_long(HANDLER + 2, OBSERVED_D1);
        runner.bus.write_word(HANDLER + 6, 0x203C); // MOVE.L #imm,D0
        runner.bus.write_long(HANDLER + 8, 0xCAFE_8000);
        runner.bus.write_word(HANDLER + 12, 0x223C); // MOVE.L #imm,D1
        runner.bus.write_long(HANDLER + 14, 0x1111_1111);
        runner.bus.write_word(HANDLER + 18, 0x243C); // MOVE.L #imm,D2
        runner.bus.write_long(HANDLER + 20, 0x2222_2222);
        runner.bus.write_word(HANDLER + 24, 0x207C); // MOVEA.L #imm,A0
        runner.bus.write_long(HANDLER + 26, 0xAAAA_AAAA);
        runner.bus.write_word(HANDLER + 30, 0x227C); // MOVEA.L #imm,A1
        runner.bus.write_long(HANDLER + 32, 0x1111_AAAA);
        runner.bus.write_word(HANDLER + 36, 0x247C); // MOVEA.L #imm,A2
        runner.bus.write_long(HANDLER + 38, 0x2222_AAAA);
        runner.bus.write_word(HANDLER + 42, 0x4E75); // RTS
        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, 0xA039, HANDLER)
            .unwrap();
        runner.m68k.cpu.write_reg(Register::PC, TRAP_PC);
        runner.m68k.cpu.write_reg(Register::A7, SP);
        runner.m68k.cpu.write_reg(Register::D1, original_d1);
        runner.m68k.cpu.write_reg(Register::D2, original_d2);
        runner.m68k.cpu.write_reg(Register::A0, 0xA0A0_BEEF);
        runner.m68k.cpu.write_reg(Register::A1, original_a1);
        runner.m68k.cpu.write_reg(Register::A2, original_a2);
        runner.m68k.cpu.core.set_ccr(0x1F);

        let (steps, running) = runner.run_steps(9, None);

        assert_eq!(steps, 9);
        assert!(running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), TRAP_PC + 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), SP);
        assert_eq!(runner.bus.read_long(OBSERVED_D1), 0xD1D1_A739);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0xCAFE_8000);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D1), original_d1);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D2), original_d2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0xAAAA_AAAA);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A1), original_a1);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A2), original_a2);
        assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x18);
        assert!(runner.dispatcher.pending_native_trap_calls.is_empty());
    }

    #[test]
    fn multiple_application_head_patches_execute_their_saved_old_chain() {
        use crate::memory::globals::addr;

        const TRAP_WORD: u16 = 0xA039; // ReadDateTime
        const TRAP_PC: u32 = 0x0020_0000;
        const FIRST_PATCH: u32 = 0x0020_0100;
        const SECOND_PATCH: u32 = 0x0020_0200;
        const OUTPUT: u32 = 0x0020_0300;
        const PATCH_COUNTER: u32 = 0x0020_0310;
        const SP: u32 = 0x007F_FF00;
        const PRESERVED_D2: u32 = 0xD2D2_BEEF;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .m68k
            .cpu
            .write_reg(Register::D0, u32::from(TRAP_WORD));
        runner
            .dispatcher
            .dispatch(0xA346, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        let original = runner.m68k.cpu.read_reg(Register::A0);

        runner.bus.write_word(FIRST_PATCH, 0x52B9); // ADDQ.L #1,abs.l
        runner.bus.write_long(FIRST_PATCH + 2, PATCH_COUNTER);
        runner.bus.write_word(FIRST_PATCH + 6, 0x4EF9); // JMP absolute long
        runner.bus.write_long(FIRST_PATCH + 8, original);
        runner
            .m68k
            .cpu
            .write_reg(Register::D0, u32::from(TRAP_WORD));
        runner.m68k.cpu.write_reg(Register::A0, FIRST_PATCH);
        runner
            .dispatcher
            .dispatch(0xA247, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();

        runner
            .m68k
            .cpu
            .write_reg(Register::D0, u32::from(TRAP_WORD));
        runner
            .dispatcher
            .dispatch(0xA346, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        let saved_first = runner.m68k.cpu.read_reg(Register::A0);
        assert_eq!(saved_first, FIRST_PATCH);
        runner.bus.write_word(SECOND_PATCH, 0x54B9); // ADDQ.L #2,abs.l
        runner.bus.write_long(SECOND_PATCH + 2, PATCH_COUNTER);
        runner.bus.write_word(SECOND_PATCH + 6, 0x4EF9); // JMP absolute long
        runner.bus.write_long(SECOND_PATCH + 8, saved_first);
        runner
            .m68k
            .cpu
            .write_reg(Register::D0, u32::from(TRAP_WORD));
        runner.m68k.cpu.write_reg(Register::A0, SECOND_PATCH);
        runner
            .dispatcher
            .dispatch(0xA247, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();

        runner.bus.write_word(TRAP_PC, TRAP_WORD);
        runner.bus.write_long(addr::TIME, 0x1234_5678);
        runner.m68k.cpu.write_reg(Register::PC, TRAP_PC);
        runner.m68k.cpu.write_reg(Register::A7, SP);
        runner.m68k.cpu.write_reg(Register::A0, OUTPUT);
        runner.m68k.cpu.write_reg(Register::D2, PRESERVED_D2);

        let (steps, running) = runner.run_steps(7, None);

        assert_eq!(steps, 7);
        assert!(running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), TRAP_PC + 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), SP);
        assert_eq!(runner.bus.read_long(PATCH_COUNTER), 3);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D2), PRESERVED_D2);
        assert_eq!(runner.bus.read_long(OUTPUT), 0x1234_5678);
        assert!(runner.dispatcher.pending_native_trap_calls.is_empty());

        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, TRAP_WORD, saved_first)
            .expect("first protected patch must restore");
        assert_eq!(
            runner
                .dispatcher
                .native_trap_handler(&runner.bus, TRAP_WORD),
            Some(FIRST_PATCH)
        );
        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, TRAP_WORD, original)
            .expect("original trap handler must restore");
        assert!(!runner
            .dispatcher
            .has_native_trap_patch(&runner.bus, TRAP_WORD));
    }

    #[test]
    fn four_bit_runner_publishes_consistent_screen_metadata() {
        use crate::memory::globals::addr;

        let config = FixtureRunnerConfig {
            addressing_32_bit: false,
            ..FixtureRunnerConfig::default()
        }
        .with_screen_depth(4)
        .expect("4-bit indexed mode should be supported");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
        let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
        let gdevice = runner.bus.read_long(gdevice_handle);
        let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
        let ctab = runner.bus.read_long(runner.bus.read_long(pixmap + 42));

        assert_eq!(runner.dispatcher.screen_mode.1, 416);
        assert_eq!(runner.dispatcher.screen_mode.4, 4);
        assert_eq!(runner.bus.read_word(addr::SCREEN_ROW), 416);
        assert_eq!(runner.bus.read_word(addr::SCREEN_BITS + 4), 416);
        assert_eq!(runner.bus.read_word(pixmap + 4), 0x8000 | 416);
        assert_eq!(runner.bus.read_word(pixmap + 32), 4);
        assert_eq!(runner.bus.read_word(pixmap + 36), 4);
        assert_eq!(runner.bus.read_word(ctab + 6), 15);
        assert_eq!(runner.bus.read_long(gdevice + 42), 0x0082);
        assert!(!runner.bus.addressing_32_bit());
        assert_eq!(runner.dispatcher.mmu_mode, 0);
    }

    #[test]
    fn one_bit_runner_publishes_consistent_screen_metadata() {
        use crate::memory::globals::addr;

        let config = FixtureRunnerConfig::default()
            .with_screen_depth(1)
            .expect("1-bit monochrome mode should be supported");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
        let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
        let gdevice = runner.bus.read_long(gdevice_handle);
        let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
        let ctab = runner.bus.read_long(runner.bus.read_long(pixmap + 42));

        assert_eq!(runner.dispatcher.screen_mode.1, 112);
        assert_eq!(runner.dispatcher.screen_mode.4, 1);
        assert_eq!(runner.bus.read_word(addr::SCREEN_ROW), 112);
        assert_eq!(runner.bus.read_word(addr::SCREEN_BITS + 4), 112);
        assert_eq!(runner.bus.read_word(pixmap + 4), 0x8000 | 112);
        assert_eq!(runner.bus.read_word(pixmap + 32), 1);
        assert_eq!(runner.bus.read_word(pixmap + 36), 1);
        assert_eq!(runner.bus.read_word(ctab + 6), 1);
        assert_eq!(runner.bus.read_long(gdevice + 42), 0x0080);
    }

    #[test]
    fn two_bit_runner_publishes_consistent_screen_metadata() {
        use crate::memory::globals::addr;

        let config = FixtureRunnerConfig::default()
            .with_screen_depth(2)
            .expect("2-bit indexed mode should be supported");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
        let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
        let gdevice = runner.bus.read_long(gdevice_handle);
        let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
        let ctab = runner.bus.read_long(runner.bus.read_long(pixmap + 42));

        assert_eq!(runner.configured_screen_depth(), 2);
        assert_eq!(runner.dispatcher.screen_mode.1, 208);
        assert_eq!(runner.dispatcher.screen_mode.4, 2);
        assert_eq!(runner.bus.read_word(addr::SCREEN_ROW), 208);
        assert_eq!(runner.bus.read_word(addr::SCREEN_BITS + 4), 208);
        assert_eq!(runner.bus.read_word(pixmap + 4), 0x8000 | 208);
        assert_eq!(runner.bus.read_word(pixmap + 32), 2);
        assert_eq!(runner.bus.read_word(pixmap + 36), 2);
        assert_eq!(runner.bus.read_word(ctab + 6), 3);
        assert_eq!(runner.bus.read_long(gdevice + 42), 0x0081);
    }

    #[test]
    fn runner_config_rejects_nonselectable_screen_depths() {
        assert!(FixtureRunnerConfig::default().with_screen_depth(3).is_err());
        assert!(FixtureRunnerConfig::default().with_screen_depth(5).is_err());
    }

    #[test]
    fn runner_config_preserves_architecture_defaults_until_depth_is_explicit() {
        let default_runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        assert_eq!(default_runner.configured_screen_depth(), 8);
        assert_eq!(default_runner.configured_powerpc_screen_depth(), 16);

        for depth in [1, 2, 4, 8] {
            let config = FixtureRunnerConfig::default()
                .with_screen_depth(depth)
                .expect("indexed depth should be supported");
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
            assert_eq!(
                runner.configured_powerpc_screen_depth(),
                16,
                "68K config depth {depth} must not become an implicit PPC override"
            );
            runner
                .set_powerpc_screen_depth(depth)
                .expect("PowerPC indexed depth should be supported");
            assert_eq!(runner.configured_screen_depth(), depth);
            assert_eq!(runner.configured_powerpc_screen_depth(), u32::from(depth));
        }

        let direct_nondefault = FixtureRunner::new(
            8 * 1024 * 1024,
            FixtureRunnerConfig {
                screen_depth: 4,
                ..FixtureRunnerConfig::default()
            },
        );
        assert_eq!(direct_nondefault.configured_powerpc_screen_depth(), 16);

        let mut explicit_eight =
            FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        explicit_eight.set_powerpc_screen_depth(8).unwrap();
        assert_eq!(explicit_eight.configured_powerpc_screen_depth(), 8);

        let mut default_runner =
            FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        assert!(default_runner.set_powerpc_screen_depth(3).is_err());
    }

    #[derive(Clone, Default)]
    struct CapturingAudioBackend {
        stereo_samples: Rc<RefCell<Vec<u8>>>,
    }

    impl CapturingAudioBackend {
        fn new() -> (Self, Rc<RefCell<Vec<u8>>>) {
            let stereo_samples = Rc::new(RefCell::new(Vec::new()));
            (
                Self {
                    stereo_samples: stereo_samples.clone(),
                },
                stereo_samples,
            )
        }
    }

    impl AudioBackend for CapturingAudioBackend {
        fn queue_samples(&mut self, samples: &[u8]) {
            self.stereo_samples.borrow_mut().extend(samples);
        }

        fn queue_stereo_samples(&mut self, samples: &[u8]) {
            self.stereo_samples.borrow_mut().extend(samples);
        }

        fn stop(&mut self) {}
    }

    fn test_region_handle(
        bus: &mut crate::memory::MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> u32 {
        let rgn_ptr = 0x0030_0100;
        let rgn_handle = 0x0030_0140;
        bus.write_long(rgn_handle, rgn_ptr);
        bus.write_word(rgn_ptr, 10);
        bus.write_word(rgn_ptr + 2, top as u16);
        bus.write_word(rgn_ptr + 4, left as u16);
        bus.write_word(rgn_ptr + 6, bottom as u16);
        bus.write_word(rgn_ptr + 8, right as u16);
        rgn_handle
    }

    #[test]
    fn vfs_file_snapshot_round_trips_both_forks_and_metadata() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher
            .vfs
            .insert("Pilots/Test Pilot".to_string(), vec![1, 2, 3]);
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Pilots/Test Pilot".to_string(), vec![4, 5, 6, 7]);
        runner
            .dispatcher
            .vfs
            .insert("__rsrc__Pilots/Test Pilot".to_string(), vec![0xEE]);
        runner
            .dispatcher
            .vfs
            .insert("Game Data/Shapes".to_string(), vec![0xAA; 1024]);
        runner
            .dispatcher
            .set_vfs_entry_metadata("Pilots/Test Pilot", *b"PIL ", *b"EVO!", 0x4000);
        runner
            .dispatcher
            .set_vfs_entry_metadata("Game Data/Shapes", *b"shap", *b"26.2", 0);

        let summaries = runner.vfs_file_summaries();
        assert_eq!(summaries.len(), 2);
        assert!(summaries
            .iter()
            .any(|summary| summary.path == "Game Data/Shapes"));

        let summaries = runner.vfs_file_summaries_where(|path| path.starts_with("Pilots/"));
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].path, "Pilots/Test Pilot");
        assert_eq!(summaries[0].data_len, 3);
        assert_eq!(summaries[0].resource_len, 4);
        assert_eq!(summaries[0].file_type, u32::from_be_bytes(*b"PIL "));
        assert_eq!(summaries[0].creator, u32::from_be_bytes(*b"EVO!"));

        let snapshot = runner
            .vfs_file_snapshot("Pilots/Test Pilot")
            .expect("snapshot");
        assert_eq!(snapshot.data_fork, vec![1, 2, 3]);
        assert_eq!(snapshot.resource_fork, vec![4, 5, 6, 7]);
        assert_eq!(snapshot.finder_flags, 0x4000);

        let mut restored = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        restored.import_vfs_file(&snapshot);
        assert_eq!(
            restored.vfs_file_snapshot("Pilots/Test Pilot"),
            Some(snapshot)
        );

        assert!(restored.remove_vfs_file("Pilots/Test Pilot"));
        assert_eq!(restored.vfs_file_snapshot("Pilots/Test Pilot"), None);
    }

    #[test]
    fn import_vfs_file_relative_to_launched_app_mounts_under_app_parent() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher
            .set_launched_app_path("EV Override/EV Override");
        let plugin = VfsFileSnapshot {
            path: "Warblade".to_string(),
            data_fork: Vec::new(),
            resource_fork: vec![1, 2, 3, 4],
            file_type: u32::from_be_bytes(*b"Op.f"),
            creator: u32::from_be_bytes(*b"Es.O"),
            finder_flags: 0x4000,
            created_date: 123,
            modified_date: 456,
        };

        runner
            .import_vfs_file_relative_to_launched_app("EV Plug-Ins", &plugin)
            .expect("relative plugin import");

        let mounted = runner
            .vfs_file_snapshot("EV Override/EV Plug-Ins/Warblade")
            .expect("mounted plugin snapshot");
        assert_eq!(mounted.resource_fork, plugin.resource_fork);
        assert_eq!(mounted.file_type, plugin.file_type);
        assert_eq!(mounted.creator, plugin.creator);
        assert_eq!(mounted.finder_flags, plugin.finder_flags);

        let parent_dir_id = runner
            .dispatcher
            .vfs_metadata
            .get("EV Override/EV Plug-Ins/Warblade")
            .expect("plugin metadata")
            .parent_dir_id;
        let entries = runner.dispatcher.list_vfs_catalog_entries(parent_dir_id);
        assert!(entries
            .iter()
            .any(|entry| !entry.is_directory && entry.name == "Warblade"));
    }

    fn make_resource_fork_bytes(resources: &[([u8; 4], i16, &[u8])]) -> Vec<u8> {
        let mut type_groups: Vec<([u8; 4], Vec<(i16, &[u8], u32)>)> = Vec::new();
        for (res_type, res_id, data) in resources {
            let group_idx = type_groups
                .iter()
                .position(|(existing_type, _)| existing_type == res_type)
                .unwrap_or_else(|| {
                    type_groups.push((*res_type, Vec::new()));
                    type_groups.len() - 1
                });
            type_groups[group_idx].1.push((*res_id, *data, 0));
        }
        type_groups.sort_by_key(|(res_type, _)| *res_type);
        for (_, entries) in &mut type_groups {
            entries.sort_by_key(|(res_id, _, _)| *res_id);
        }

        let data_offset = 16u32;
        let mut data_section = Vec::new();
        for (_, entries) in &mut type_groups {
            for (_, data, data_pos) in entries {
                *data_pos = data_section.len() as u32;
                data_section.extend_from_slice(&(data.len() as u32).to_be_bytes());
                data_section.extend_from_slice(data);
            }
        }

        let map_offset = data_offset + data_section.len() as u32;
        let type_list_offset = 30u16;
        let type_count = type_groups.len();
        let resource_count: usize = type_groups.iter().map(|(_, entries)| entries.len()).sum();
        let ref_lists_offset = 2 + type_count * 8;
        let name_list_offset = type_list_offset as usize + ref_lists_offset + resource_count * 12;
        let map_length = name_list_offset as u32;

        let mut bytes = vec![0u8; (map_offset + map_length) as usize];
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&data_offset.to_be_bytes());
        header[4..8].copy_from_slice(&map_offset.to_be_bytes());
        header[8..12].copy_from_slice(&(data_section.len() as u32).to_be_bytes());
        header[12..16].copy_from_slice(&map_length.to_be_bytes());
        bytes[0..16].copy_from_slice(&header);
        bytes[data_offset as usize..data_offset as usize + data_section.len()]
            .copy_from_slice(&data_section);

        let map_start = map_offset as usize;
        bytes[map_start..map_start + 16].copy_from_slice(&header);
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28]
            .copy_from_slice(&(name_list_offset as u16).to_be_bytes());
        bytes[map_start + 28..map_start + 30]
            .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2]
            .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());
        let mut next_ref_list_offset = ref_lists_offset;
        for (i, (res_type, entries)) in type_groups.iter().enumerate() {
            let type_entry = type_list_start + 2 + i * 8;
            bytes[type_entry..type_entry + 4].copy_from_slice(res_type);
            bytes[type_entry + 4..type_entry + 6]
                .copy_from_slice(&((entries.len() as u16) - 1).to_be_bytes());
            bytes[type_entry + 6..type_entry + 8]
                .copy_from_slice(&(next_ref_list_offset as u16).to_be_bytes());

            let ref_list_start = type_list_start + next_ref_list_offset;
            for (j, (res_id, _, data_pos)) in entries.iter().enumerate() {
                let ref_entry = ref_list_start + j * 12;
                bytes[ref_entry..ref_entry + 2].copy_from_slice(&(*res_id as u16).to_be_bytes());
                bytes[ref_entry + 2..ref_entry + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
                bytes[ref_entry + 4] = 0;
                let data_offset_bytes = data_pos.to_be_bytes();
                bytes[ref_entry + 5..ref_entry + 8].copy_from_slice(&data_offset_bytes[1..4]);
            }

            next_ref_list_offset += entries.len() * 12;
        }

        bytes
    }

    fn minimal_code0(above_a5: u32, below_a5: u32, jt_size: u32, jt_offset: u32) -> Vec<u8> {
        let mut code0 = Vec::with_capacity(16 + jt_size as usize);
        code0.extend_from_slice(&above_a5.to_be_bytes());
        code0.extend_from_slice(&below_a5.to_be_bytes());
        code0.extend_from_slice(&jt_size.to_be_bytes());
        code0.extend_from_slice(&jt_offset.to_be_bytes());
        code0.resize(16 + jt_size as usize, 0);
        code0
    }

    fn size_resource_bytes(flags: u16, preferred_size: u32, minimum_size: u32) -> Vec<u8> {
        let mut size = Vec::with_capacity(10);
        size.extend_from_slice(&flags.to_be_bytes());
        size.extend_from_slice(&preferred_size.to_be_bytes());
        size.extend_from_slice(&minimum_size.to_be_bytes());
        size
    }

    #[test]
    fn init_app_preserves_resources_allocated_before_zone_header() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let bgas = [0x4E, 0x56, 0xFF, 0xA6, 0x2D, 0x7A, 0x1C, 0x72];
        let fork_bytes = make_resource_fork_bytes(&[(*b"BGAS", 128, &bgas), (*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let (_, bgas_ptr) = runner
            .dispatcher
            .find_or_load_resource_any(&mut runner.bus, *b"BGAS", 128)
            .expect("BGAS resource loaded");
        assert_eq!(runner.bus.read_bytes(bgas_ptr, bgas.len()), bgas);

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_bytes(bgas_ptr, bgas.len()),
            bgas,
            "init_app must not overwrite resources loaded before zone setup"
        );
    }

    #[test]
    fn init_app_seeds_post_boot_ticks() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_long(addr::TICKS),
            DEFAULT_LAUNCH_TICKS,
            "fresh app launch should see a realistic nonzero post-boot TickCount"
        );
        assert_eq!(
            runner.guest_tick(),
            DEFAULT_LAUNCH_TICKS,
            "TickCount fast path must stay in sync with low-memory Ticks"
        );
    }

    #[test]
    fn init_app_applies_pinned_68k_launch_state() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(1234, 0x89AB_CDEF, 0x1122_3344_5566_7788);

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::TICKS), 1234);
        assert_eq!(runner.guest_tick(), 1234);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89AB_CDEF);
    }

    #[test]
    fn optional_68k_launch_state_uses_tick_floor_and_exact_seed() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_optional_launch_state(Some(10), Some(0x1020_3040), None);

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::TICKS), DEFAULT_LAUNCH_TICKS);
        assert_eq!(runner.guest_tick(), DEFAULT_LAUNCH_TICKS);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1020_3040);
    }

    #[test]
    fn init_ppc_app_applies_pinned_launch_state_to_both_memories() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(4321, 0x7654_3210, u64::MAX);
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let tick_entry = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
        runner.bus.write_long(tick_entry, 0x0021_1000);
        runner.bus.write_long(0x28, 0x0021_2000);
        runner.init_app(&app);
        assert_eq!(
            runner.dispatcher.trap_table_profile,
            Some(TrapTableProfile::PowerPc604)
        );
        assert_ne!(runner.bus.read_long(tick_entry), 0x0021_1000);
        assert!(runner.dispatcher.aline_vector_is_default(&runner.bus));

        assert_eq!(runner.bus.read_long(addr::TICKS), 4321);
        assert_eq!(runner.guest_tick(), 4321);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x7654_3210);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(
            ppc_app
                .memory
                .read_u32_be(crate::memory::globals::addr::TICKS),
            Some(4321)
        );
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(4321));
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::RND_SEED),
            Some(0x7654_3210)
        );
        let toolbox_entry = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
        let default_tick_count = runner.bus.read_long(toolbox_entry);
        assert_eq!(
            ppc_app.memory.read_u32_be(toolbox_entry),
            Some(default_tick_count),
            "PPC and 68k adapters must see one materialized trap table"
        );
        ppc_app
            .memory
            .write_u32_be(toolbox_entry, 0x0021_0000)
            .expect("write shared Toolbox trap entry");
        assert_eq!(runner.bus.read_long(toolbox_entry), 0x0021_0000);
        assert_eq!(ppc_app.cpu.time_base(), u64::MAX);
    }

    #[test]
    fn optional_ppc_seed_preserves_tick_and_time_base_defaults() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_optional_launch_state(None, Some(0x1020_3040), None);
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::TICKS), 0);
        assert_eq!(runner.guest_tick(), 0);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1020_3040);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(0));
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::RND_SEED),
            Some(0x1020_3040)
        );
        assert_eq!(ppc_app.cpu.time_base(), 0);
    }

    #[test]
    fn repeated_partial_launch_state_updates_preserve_prior_controls() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_optional_launch_state(Some(4321), None, Some(0x1122_3344_5566_7788));
        runner.set_optional_launch_state(None, Some(0x7654_3210), None);
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::TICKS), 4321);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x7654_3210);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(4321));
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::RND_SEED),
            Some(0x7654_3210)
        );
        assert_eq!(ppc_app.cpu.time_base(), 0x1122_3344_5566_7788);
    }

    #[test]
    fn native_launch_honors_a_larger_application_partition() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = crate::game::new_runner();
        runner.set_application_partition_size(Some(96 * 1024 * 1024));
        runner.init_app(&app);
        let limit = runner
            .native
            .application()
            .unwrap()
            .application_heap_limit();
        assert!(limit > PPC_STACK_TOP);
        assert_eq!(
            runner
                .bus
                .read_long(crate::memory::globals::addr::APPL_LIMIT),
            limit
        );
        // Sparse native mappings can live above the classic RAM backing.
        let address = limit - 16;
        assert!(address > runner.bus.ram_size());
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .add_region(address, vec![0x73; 16]);
        assert_eq!(runner.bus.read_byte(address), 0x73);
        runner.bus.write_byte(address, 0x41);
        assert_eq!(
            runner.native.application_mut().unwrap().memory.read_u8(address),
            Some(0x41)
        );
    }

    #[test]
    fn init_ppc_app_preserves_launch_defaults_without_override() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_app_start_time(0x1020_3040);
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::TICKS), 0);
        assert_eq!(runner.guest_tick(), 0);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1020_3040);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(
            ppc_app
                .memory
                .read_u32_be(crate::memory::globals::addr::TICKS),
            Some(0)
        );
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(0));
        assert_eq!(ppc_app.memory.read_u32_be(addr::TIME), Some(0x1020_3040));
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::RND_SEED),
            Some(0x1020_3040)
        );
        assert_eq!(ppc_app.cpu.time_base(), 0);
    }

    #[test]
    fn init_ppc_app_merges_detached_events_then_attaches_the_process_queue() {
        // Test case 1: Context has event and invalidation=true, detached PPC has event and invalidation=false
        {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner
                .process_context
                .shared_event_queue()
                .push_back(QueuedEvent {
                    what: 3,
                    message: 0x1111,
                    when: 0,
                    where_v: 10,
                    where_h: 20,
                    modifiers: 0x0100,
                });
            runner
                .process_context
                .shared_event_queue()
                .invalidate_menu_bar();

            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
            ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
            ppc_app.event_queue.push_back(QueuedEvent {
                what: 1,
                message: 0x2222,
                when: 0,
                where_v: 30,
                where_h: 40,
                modifiers: 0x0200,
            });

            runner.init_app(&app);

            let queue = runner.process_context.event_queue();
            assert_eq!(queue.len(), 2);
            assert_eq!(
                queue[0].message, 0x1111,
                "canonical event must remain in front"
            );
            assert_eq!(
                queue[1].message, 0x2222,
                "detached PPC event must be appended after canonical events"
            );
            assert!(
                queue.menu_bar_is_invalid(),
                "menu_bar_invalid should be true when context was true"
            );
            let native_queue = &runner.native.application().unwrap().event_queue;
            assert_eq!(native_queue.len(), 2);
            assert_eq!(native_queue[0].message, 0x1111);
            assert_eq!(native_queue[1].message, 0x2222);
            assert!(native_queue.menu_bar_is_invalid());
        }

        // Test case 2: Context has event and invalidation=false, detached PPC has event and invalidation=true
        {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner
                .process_context
                .shared_event_queue()
                .push_back(QueuedEvent {
                    what: 3,
                    message: 0x3333,
                    when: 0,
                    where_v: 10,
                    where_h: 20,
                    modifiers: 0x0100,
                });

            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
            ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
            ppc_app.event_queue.push_back(QueuedEvent {
                what: 1,
                message: 0x4444,
                when: 0,
                where_v: 30,
                where_h: 40,
                modifiers: 0x0200,
            });
            ppc_app.event_queue.invalidate_menu_bar();

            runner.init_app(&app);

            let queue = runner.process_context.event_queue();
            assert_eq!(queue.len(), 2);
            assert_eq!(
                queue[0].message, 0x3333,
                "canonical event must remain in front"
            );
            assert_eq!(
                queue[1].message, 0x4444,
                "detached PPC event must be appended after canonical events"
            );
            assert!(
                queue.menu_bar_is_invalid(),
                "menu_bar_invalid should be true when detached PPC was true"
            );
            let native_queue = &runner.native.application().unwrap().event_queue;
            assert_eq!(native_queue.len(), 2);
            assert_eq!(native_queue[0].message, 0x3333);
            assert_eq!(native_queue[1].message, 0x4444);
            assert!(native_queue.menu_bar_is_invalid());
        }
    }

    #[test]
    fn init_ppc_app_attaches_one_bidirectional_process_window_list() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.dispatcher.window_list.extend([0x1000, 0x2000]);
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        runner.init_app(&app);

        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        let detached = ppc_app.window_list.clone();
        assert_eq!(&*ppc_app.window_list, &[0x1000, 0x2000]);
        ppc_app.window_list.insert(0, 0x3000);
        assert_eq!(&*runner.dispatcher.window_list, &[0x3000, 0x1000, 0x2000]);
        runner
            .dispatcher
            .window_list
            .retain(|window| *window != 0x1000);
        assert_eq!(&*ppc_app.window_list, &[0x3000, 0x2000]);
        assert_eq!(&*detached, &[0x1000, 0x2000]);
    }

    fn write_snapshot_rect(bus: &mut MacMemoryBus, address: u32, rect: WindowRect) {
        for (index, value) in [rect.0, rect.1, rect.2, rect.3].into_iter().enumerate() {
            bus.write_word(address.wrapping_add(index as u32 * 2), value as u16);
        }
    }

    fn write_snapshot_region_bounds(bus: &mut MacMemoryBus, handle: u32, bounds: WindowRect) {
        let region = bus.read_long(handle);
        assert_ne!(region, 0, "window operation must allocate region data");
        bus.write_word(region, 10);
        write_snapshot_rect(bus, region.wrapping_add(2), bounds);
    }

    fn configure_snapshot_window(runner: &mut FixtureRunner, window: u32, color: bool) {
        runner.bus.write_byte(window.wrapping_add(110), 0xFF);
        runner.bus.write_byte(window.wrapping_add(111), 0xFF);
        write_snapshot_rect(&mut runner.bus, window.wrapping_add(16), (10, 20, 30, 40));
        if color {
            let pixmap_handle = runner.bus.read_long(window.wrapping_add(2));
            let pixmap = runner.bus.read_long(pixmap_handle);
            assert_ne!(pixmap, 0, "NewCWindow must install a PixMap");
            runner
                .bus
                .write_word(pixmap.wrapping_add(6), (-100i16) as u16);
            runner
                .bus
                .write_word(pixmap.wrapping_add(8), (-200i16) as u16);
        } else {
            runner
                .bus
                .write_word(window.wrapping_add(8), (-100i16) as u16);
            runner
                .bus
                .write_word(window.wrapping_add(10), (-200i16) as u16);
        }
        for (offset, bounds) in [
            (114, (100, 200, 140, 250)),
            (24, (5, 7, 15, 17)),
            (122, (101, 202, 111, 212)),
        ] {
            let handle = runner.bus.read_long(window.wrapping_add(offset));
            assert_ne!(handle, 0, "window operation must install region handle");
            write_snapshot_region_bounds(&mut runner.bus, handle, bounds);
        }
        runner.dispatcher.window_list.replace(vec![window]);
        runner
            .bus
            .write_long(crate::memory::globals::addr::GHOST_WINDOW, 0);
    }

    fn create_classic_snapshot_window(title: &[u8]) -> (FixtureRunner, u32) {
        const BOUNDS: u32 = 0x0030_0000;
        const TITLE: u32 = 0x0030_0100;
        const STACK: u32 = 0x0030_1000;
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        write_snapshot_rect(&mut runner.bus, BOUNDS, (20, 10, 260, 330));
        runner.bus.write_pstring(TITLE, title);
        for offset in 0..30 {
            runner.bus.write_byte(STACK + offset, 0);
        }
        runner.bus.write_byte(STACK + 4, 1);
        runner.bus.write_long(STACK + 6, u32::MAX);
        runner.bus.write_byte(STACK + 12, 1);
        runner.bus.write_long(STACK + 14, TITLE);
        runner.bus.write_long(STACK + 18, BOUNDS);
        runner.m68k.cpu.write_reg(Register::A7, STACK);
        runner
            .dispatcher
            .dispatch(0xA913, &mut runner.m68k.cpu, &mut runner.bus)
            .expect("classic NewWindow must return");
        let window = runner.bus.read_long(STACK + 26);
        assert_ne!(window, 0);
        (runner, window)
    }

    fn create_native_snapshot_window(title: &[u8]) -> (FixtureRunner, u32) {
        use crate::loader::ppc::tests::synthetic_pef_with_import;
        const SCRATCH: u32 = PPC_DATA_BASE + 0x1000;
        let mut native = load_pef_application(&synthetic_pef_with_import(b"NewCWindow")).unwrap();
        native.memory.add_region(SCRATCH, vec![0; 512]);
        for (index, value) in [20i16, 10, 260, 330].into_iter().enumerate() {
            native
                .memory
                .write_u16_be(SCRATCH + index as u32 * 2, value as u16)
                .unwrap();
        }
        native
            .memory
            .write_u8(SCRATCH + 16, title.len() as u8)
            .unwrap();
        native.memory.write_bytes(SCRATCH + 17, title).unwrap();
        native.cpu.gpr[3] = 0;
        native.cpu.gpr[4] = SCRATCH;
        native.cpu.gpr[5] = SCRATCH + 16;
        native.cpu.gpr[6] = 1;
        native.cpu.gpr[7] = 0;
        native.cpu.gpr[8] = u32::MAX;
        native.cpu.gpr[9] = 1;
        native.cpu.gpr[10] = 0;
        let probe = native.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let window = native.cpu.gpr[3];
        assert_ne!(window, 0);

        let app = LoadedApp::from_ppc(native);
        let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        (runner, window)
    }

    // Retain the two divergent expressions from the deleted native projector so
    // the integration test proves the common projection changes their result.
    // This is source-model evidence; it is not presented as a captured old run.
    fn deleted_native_visibility_and_visible_region_model(
        app: &mut PpcLoadedApp,
        window: u32,
    ) -> (bool, Option<WindowRect>) {
        let visible = app.memory.read_u8(window.wrapping_add(110)) == Some(1);
        let port = (
            app.memory.read_u16_be(window.wrapping_add(16)).unwrap_or(0) as i16,
            app.memory.read_u16_be(window.wrapping_add(18)).unwrap_or(0) as i16,
            app.memory.read_u16_be(window.wrapping_add(20)).unwrap_or(0) as i16,
            app.memory.read_u16_be(window.wrapping_add(22)).unwrap_or(0) as i16,
        );
        let pixmap = app
            .memory
            .read_u32_be(window.wrapping_add(2))
            .and_then(|handle| app.memory.read_u32_be(handle));
        let origin = pixmap.map(|pixmap| {
            (
                app.memory.read_u16_be(pixmap.wrapping_add(6)).unwrap_or(0) as i16,
                app.memory.read_u16_be(pixmap.wrapping_add(8)).unwrap_or(0) as i16,
            )
        });
        let global_port = origin.map(|origin| {
            (
                port.0.saturating_sub(origin.0),
                port.1.saturating_sub(origin.1),
                port.2.saturating_sub(origin.0),
                port.3.saturating_sub(origin.1),
            )
        });
        let visible_region = app
            .memory
            .read_u32_be(window.wrapping_add(24))
            .and_then(|handle| app.memory.read_u32_be(handle))
            .map(|region| {
                (
                    app.memory.read_u16_be(region.wrapping_add(2)).unwrap_or(0) as i16,
                    app.memory.read_u16_be(region.wrapping_add(4)).unwrap_or(0) as i16,
                    app.memory.read_u16_be(region.wrapping_add(6)).unwrap_or(0) as i16,
                    app.memory.read_u16_be(region.wrapping_add(8)).unwrap_or(0) as i16,
                )
            })
            .zip(global_port)
            .map(|(rect, port)| {
                (
                    rect.0.saturating_add(port.0),
                    rect.1.saturating_add(port.1),
                    rect.2.saturating_add(port.0),
                    rect.3.saturating_add(port.1),
                )
            });
        (visible, visible_region)
    }

    #[test]
    fn runner_window_snapshot_matches_classic_and_native_window_operations() {
        let title = b"Caf\x8e";
        let (mut classic, classic_window) = create_classic_snapshot_window(title);
        let initial_classic = classic.window_stack_snapshot();

        let (mut native, native_window) = create_native_snapshot_window(title);
        let initial_native = native.window_stack_snapshot();
        assert_eq!(initial_classic.len(), 1);
        assert_eq!(initial_native.len(), 1);
        let classic_created = &initial_classic[0];
        let native_created = &initial_native[0];
        assert_eq!(classic_created.title, native_created.title);
        assert_eq!(classic_created.bounds, native_created.bounds);
        assert_eq!(
            classic_created.structure_bounds,
            native_created.structure_bounds
        );
        assert_eq!(
            classic_created.visible_region,
            native_created.visible_region
        );
        assert_eq!(classic_created.visible, native_created.visible);
        assert_eq!(classic_created.active, native_created.active);
        // Preserve the routes' existing initial-update policy: the classic
        // NewWindow route invalidates its content immediately, while the native
        // NewCWindow import currently does not. Snapshot unification must expose
        // that difference without taking ownership of window-creation behavior.
        assert_eq!(classic_created.update_region, Some((20, 10, 260, 330)));
        assert_eq!(native_created.update_region, None);

        configure_snapshot_window(&mut classic, classic_window, false);
        let classic_snapshot = classic.window_stack_snapshot();

        configure_snapshot_window(&mut native, native_window, true);
        let deleted_native_model = deleted_native_visibility_and_visible_region_model(
            native.native.application_mut().expect("native app"),
            native_window,
        );
        assert_eq!(deleted_native_model, (false, Some((115, 227, 125, 237))));
        let native_snapshot = native.window_stack_snapshot();

        assert_eq!(classic_snapshot, native_snapshot);
        assert_eq!(native_snapshot[0].bounds, (110, 220, 130, 240));
        assert_eq!(
            native_snapshot[0].visible_region,
            Some((105, 207, 115, 217))
        );
        assert!(native_snapshot[0].visible);
        assert!(native_snapshot[0].active);
        assert_eq!(
            native_snapshot[0].title,
            crate::mac_roman::decode_mac_roman(title)
        );
    }

    fn write_minimal_snapshot_window(
        bus: &mut MacMemoryBus,
        window: u32,
        title_handle: u32,
        title: u32,
        bytes: &[u8],
    ) {
        bus.write_word(window.wrapping_add(6), 0);
        write_snapshot_rect(bus, window.wrapping_add(16), (1, 2, 11, 22));
        bus.write_byte(window.wrapping_add(110), 0xFF);
        bus.write_long(window.wrapping_add(134), title_handle);
        bus.write_long(title_handle, title);
        bus.write_pstring(title, bytes);
    }

    #[test]
    fn runner_snapshot_routes_flat_and_native_sparse_records_through_one_bus() {
        const FLAT: u32 = 0x0002_0000;
        const SPARSE: u32 = 0x0188_0000;
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("native app")
            .memory
            .add_region(SPARSE, vec![0; 0x1000]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        write_minimal_snapshot_window(&mut runner.bus, FLAT, FLAT + 0x200, FLAT + 0x300, b"Flat");
        write_minimal_snapshot_window(
            &mut runner.bus,
            SPARSE,
            SPARSE + 0x200,
            SPARSE + 0x300,
            b"Sparse",
        );
        runner.dispatcher.window_list.replace(vec![FLAT, SPARSE]);

        assert_eq!(
            runner
                .native
                .application_mut()
                .unwrap()
                .memory
                .read_u8(SPARSE + 110),
            Some(0xFF)
        );
        let result = runner.window_stack_snapshot();
        assert_eq!(
            result
                .iter()
                .map(|window| window.title.as_str())
                .collect::<Vec<_>>(),
            ["Flat", "Sparse"]
        );
        assert!(result.iter().all(|window| window.visible));
    }

    #[test]
    fn mixed_isa_window_snapshot_uses_shared_process_state() {
        use crate::guest_call::{ExecutionTaskId, NativeThreadContext, ThreadStorage};
        use crate::guest_procedure::GuestIsa;
        use crate::loader::ppc::tests::synthetic_pef_with_import;
        const CLASSIC_CODE: u32 = 0x0031_0000;
        const CLASSIC_STACK: u32 = 0x0031_1000;
        let (mut runner, window) = create_classic_snapshot_window(b"Mixed");
        let mut companion =
            load_pef_application(&synthetic_pef_with_import(b"YieldToThread")).unwrap();
        companion.cpu.gpr[3] = ExecutionTaskId::APPLICATION.thread_id();
        let worker_context = companion.cpu.capture_execution_context();
        runner.init_ppc_companion(companion);
        let calls = runner.dispatcher.guest_calls.shared_handle();
        assert!(calls.bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::M68k));
        let worker = calls
            .create_native_thread(
                NativeThreadContext {
                    context: worker_context,
                },
                ThreadStorage::default(),
                false,
                |_| true,
            )
            .unwrap();

        let mut context = runner.native.take(NativeEngineRole::Companion).unwrap();
        context
            .adapter_mut()
            .memory
            .write_u8(window + 110, 0x7F)
            .unwrap();
        context.adapter_mut().window_list.replace(vec![window]);
        assert!(runner.native.restore(context).is_ok());
        assert_eq!(runner.bus.read_byte(window + 110), 0x7F);
        assert_eq!(&*runner.dispatcher.window_list, &[window]);

        for (index, word) in [
            0x303c, 0x0205, // MOVE.W #YieldToThread,D0
            0xabf2, // ThreadDispatch to the native worker
            0x60fe, // BRA.S -2 after the native import yields back
        ]
        .into_iter()
        .enumerate()
        {
            runner.bus.write_word(CLASSIC_CODE + index as u32 * 2, word);
        }
        runner.bus.write_long(CLASSIC_STACK, worker.thread_id());
        runner.bus.write_word(CLASSIC_STACK + 4, 0xBEEF);
        runner.m68k.cpu.write_reg(Register::PC, CLASSIC_CODE);
        runner.m68k.cpu.write_reg(Register::A7, CLASSIC_STACK);

        let (steps, running) = runner.run_steps(64, None);
        assert!(steps > 0 && running);
        assert_eq!(calls.current_task(), worker);
        assert!(calls.has_pending_task_handoff());
        let (steps, running) = runner.run_steps(64, None);
        assert!(steps > 0 && running);
        assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
        assert_eq!(runner.bus.read_word(CLASSIC_STACK + 4), 0);
        assert!(!calls.has_pending_task_handoff());
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_CODE + 6);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), CLASSIC_STACK + 4);
        let companion = runner.native.companion().unwrap();
        assert_eq!(companion.cpu.gpr[3], 0);
        // synthetic_code calls the import trap with bctrl at +12, so the
        // native yield saves its successful return at the encoded LR (+16).
        assert_eq!(companion.cpu.pc, PPC_CODE_BASE + 16);

        let result = runner.window_stack_snapshot();
        assert_eq!(runner.bus.read_byte(window + 110), 0x7F);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "Mixed");
        assert!(result[0].visible);
        assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
    }

    #[test]
    fn window_snapshot_poll_is_read_only() {
        let (mut runner, window) = create_native_snapshot_window(b"Read only");
        configure_snapshot_window(&mut runner, window, true);
        let list = (*runner.dispatcher.window_list).clone();
        let events = runner.event_manager_snapshot();
        let m68k = CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core);
        let calls = runner.dispatcher.guest_calls.shared_handle();
        let calls_before = calls.clone();
        let native = &runner.native.application().unwrap().cpu;
        let native_context = native.capture_execution_context();
        let native_alignment = native.alignment_policy;
        let native_time = native.time_base();
        let native_reservation = native.reservation_address();

        runner.bus.begin_uncapped_write_probe();
        let first = runner.window_stack_snapshot();
        let second = runner.window_stack_snapshot();
        assert!(runner.bus.finish_write_probe_unchanged());
        assert_eq!(first, second);
        assert_eq!(&*runner.dispatcher.window_list, list.as_slice());
        assert_eq!(runner.event_manager_snapshot(), events);
        assert_eq!(
            CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core),
            m68k
        );
        assert_eq!(calls, calls_before);
        let after = &runner.native.application().unwrap().cpu;
        assert_eq!(
            after.capture_execution_context().architectural(),
            native_context.architectural()
        );
        assert_eq!(after.alignment_policy, native_alignment);
        assert_eq!(after.time_base(), native_time);
        assert_eq!(after.reservation_address(), native_reservation);
    }
    #[test]
    fn ppc_slice_keeps_ticks_coherent_across_runner_and_guest_memory() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app
            .memory
            .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
            .expect("rewrite entry as infinite branch");
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(41, 0x1234_5678, 0);
        runner.init_app(&app);
        runner.set_instructions_per_tick(1_000_000);
        runner.tick_budget = 1;

        let (steps, running) = runner.run_steps(1, None);

        assert_eq!(steps, 1);
        assert!(running);
        assert_eq!(runner.bus.read_long(addr::TICKS), 42);
        assert_eq!(runner.guest_tick(), 42);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(
            ppc_app
                .memory
                .read_u32_be(crate::memory::globals::addr::TICKS),
            Some(42)
        );
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(42));
    }

    #[test]
    fn process_tick_advance_observes_direct_low_memory_mutation() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(41, 0x1234_5678, 0);
        runner.init_app(&app);

        // Ticks is writable guest state. The next vertical-retrace update
        // resolves the shared semantic clock from the directly-written cell
        // before advancing it, so both adapters continue from the same value.
        runner.bus.write_long(addr::TICKS, 9_000);
        assert_eq!(runner.advance_guest_tick(), 9_001);
        assert_eq!(runner.guest_tick(), 9_001);
        assert_eq!(runner.bus.read_long(addr::TICKS), 9_001);
        assert_eq!(
            runner
                .native
                .application_mut()
                .expect("PPC app installed")
                .memory
                .read_u32_be(addr::TICKS),
            Some(9_001)
        );
    }

    #[test]
    fn ppc_direct_ticks_store_survives_the_next_native_slice() {
        use crate::memory::globals::addr;

        const WRITE_TICKS: u32 = PPC_CODE_BASE + 0x100;
        const DIRECT_TICKS: u32 = 0xCAFE_BABE;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app.cpu.pc = WRITE_TICKS;
        ppc_app.memory.add_region(
            WRITE_TICKS,
            [
                0x3C60_CAFEu32, // lis r3,$CAFE
                0x6063_BABE,    // ori r3,r3,$BABE
                0x3C80_0000,    // lis r4,0
                0x6084_016A,    // ori r4,r4,$016A
                0x9064_0000,    // stw r3,0(r4)
                0x4800_0000,    // b .
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(41, 0x1234_5678, 0);
        runner.init_app(&app);
        runner.set_instructions_per_tick(100);
        runner.tick_budget = 100;

        let (steps, running) = runner.run_steps(5, None);
        assert_eq!(steps, 5);
        assert!(running);
        assert_eq!(runner.bus.read_long(addr::TICKS), DIRECT_TICKS);
        assert_eq!(
            runner.guest_tick(),
            DIRECT_TICKS,
            "the native slice boundary must import a direct guest Ticks store"
        );

        // The next PPC slice prepares its HLE clock from the shared low-memory
        // bytes. It must not restore the dispatcher's stale compatibility
        // scalar over a direct store made by native guest code.
        let (steps, running) = runner.run_steps(1, None);
        assert_eq!(steps, 1);
        assert!(running);
        assert_eq!(runner.bus.read_long(addr::TICKS), DIRECT_TICKS);
        assert_eq!(runner.guest_tick(), DIRECT_TICKS);
        assert_eq!(
            runner
                .native
                .application_mut()
                .expect("PPC app installed")
                .memory
                .read_u32_be(addr::TICKS),
            Some(DIRECT_TICKS)
        );
    }

    #[test]
    fn ppc_slice_direct_ticks_store_crossing_one_vbl_uses_host_epoch() {
        use crate::memory::globals::addr;

        const WRITE_TICKS: u32 = PPC_CODE_BASE + 0x100;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x200;
        const TASK_PTR: u32 = PPC_DATA_BASE + 0x3000;
        const START_TICK: u32 = 41;

        // The PPC guest rewinds or jumps the writable Ticks cell, then the
        // host cycle budget crosses exactly one VBL.  Callback delivery must
        // follow that guest baseline by one tick; it must not subtract the
        // arbitrary store from the slice's pre-execution snapshot.
        for direct_tick in [7, 0xCAFE_BABE] {
            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
            ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
            ppc_app.cpu.pc = WRITE_TICKS;
            ppc_app.memory.add_region(
                WRITE_TICKS,
                [
                    0x3C60_0000u32 | (direct_tick >> 16), // lis r3, direct_tick
                    0x6063_0000 | (direct_tick & 0xffff), // ori r3, direct_tick
                    0x3C80_0000,                          // lis r4, 0
                    0x6084_016A,                          // ori r4, $016A
                    0x9064_0000,                          // stw r3,0(r4)
                    0x4800_0000,                          // b .
                ]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            );
            ppc_app.memory.add_region(
                CALLBACK,
                [
                    0x3880_BEEFu32, // li r4,$BEEF
                    0xB083_000E,    // sth r4,14(r3): callback marker
                    0x4E80_0020,    // blr
                ]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            );
            ppc_app.memory.add_region(TASK_PTR, vec![0; 16]);
            ppc_app.memory.write_u32_be(TASK_PTR + 6, CALLBACK).unwrap();
            ppc_app.memory.write_u16_be(TASK_PTR + 10, 1).unwrap();
            ppc_app.vbl_tasks.push(PpcVblTaskRecord {
                task_ptr: TASK_PTR,
                architecture: CallbackTaskArchitecture::PowerPc,
                slot: None,
                pending: false,
            });
            ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.set_launch_state(START_TICK, 0x1234_5678, 0);
            runner.init_app(&app);
            runner.set_instructions_per_tick(5);
            runner.tick_budget = 5;

            let (steps, running) = runner.run_steps(5, None);

            assert!(running);
            assert!(steps >= 5);
            let expected_tick = direct_tick.wrapping_add(1);
            assert_eq!(runner.guest_tick(), expected_tick);
            assert_eq!(runner.bus.read_long(addr::TICKS), expected_tick);
            let ppc_app = runner.native.application_mut().expect("PPC app installed");
            assert_eq!(
                ppc_app.memory.read_u32_be(addr::TICKS),
                Some(expected_tick),
                "the stale pre-slice candidate must not overwrite direct Ticks"
            );
            assert_eq!(ppc_app.memory.read_u16_be(TASK_PTR + 10), Some(0));
            assert_eq!(ppc_app.memory.read_u16_be(TASK_PTR + 14), Some(0xBEEF));
        }
    }

    #[test]
    fn ppc_slice_direct_ticks_store_replays_three_vbl_epochs_in_order() {
        use crate::memory::globals::addr;

        const WRITE_TICKS: u32 = PPC_CODE_BASE + 0x100;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x200;
        const TASK_BASE: u32 = PPC_DATA_BASE + 0x3000;
        const START_TICK: u32 = 41;

        // Each task starts one count later than the previous task. The common
        // callback records the Ticks value it observes, so exactly one task
        // fires at each of the three host VBL epochs.
        for direct_tick in [7, 0xCAFE_BABE] {
            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
            ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
            ppc_app.cpu.pc = WRITE_TICKS;
            ppc_app.memory.add_region(
                WRITE_TICKS,
                [
                    0x3C60_0000u32 | (direct_tick >> 16), // lis r3, direct_tick
                    0x6063_0000 | (direct_tick & 0xffff), // ori r3, direct_tick
                    0x3C80_0000,                          // lis r4, 0
                    0x6084_016A,                          // ori r4, $016A
                    0x9064_0000,                          // stw r3,0(r4)
                    0x4800_0000,                          // b .
                ]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            );
            ppc_app.memory.add_region(
                CALLBACK,
                [
                    0x3880_016Au32, // li r4,$016A
                    0x80A4_0000,    // lwz r5,0(r4): read guest Ticks
                    0x90A3_0010,    // stw r5,16(r3): record on task
                    0x4E80_0020,    // blr
                ]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
            );
            ppc_app.memory.add_region(TASK_BASE, vec![0; 0x80]);
            for (task_offset, count) in [(0, 1), (0x20, 2), (0x40, 3)] {
                let task_ptr = TASK_BASE + task_offset;
                ppc_app.memory.write_u32_be(task_ptr + 6, CALLBACK).unwrap();
                ppc_app.memory.write_u16_be(task_ptr + 10, count).unwrap();
                ppc_app.vbl_tasks.push(PpcVblTaskRecord {
                    task_ptr,
                    architecture: CallbackTaskArchitecture::PowerPc,
                    slot: None,
                    pending: false,
                });
            }
            ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.set_launch_state(START_TICK, 0x1234_5678, 0);
            runner.init_app(&app);
            runner.set_instructions_per_tick(5);
            runner.tick_budget = 5;

            for epoch in 1..=3 {
                let (steps, running) = runner.run_steps(5, None);
                assert!(running);
                assert!(steps >= 5);
                let expected_tick = direct_tick.wrapping_add(epoch);
                assert_eq!(runner.guest_tick(), expected_tick);
                assert_eq!(runner.bus.read_long(addr::TICKS), expected_tick);
            }

            let ppc_app = runner.native.application_mut().expect("PPC app installed");
            for (task_offset, epoch) in [(0, 1), (0x20, 2), (0x40, 3)] {
                let task_ptr = TASK_BASE + task_offset;
                assert_eq!(ppc_app.memory.read_u16_be(task_ptr + 10), Some(0));
                assert_eq!(
                    ppc_app.memory.read_u32_be(task_ptr + 16),
                    Some(direct_tick.wrapping_add(epoch)),
                    "callback must observe host epoch {epoch}"
                );
            }
        }
    }

    #[test]
    fn mixed_isa_tickcount_and_lmgetticks_observe_shared_low_memory_bytes() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        ppc_app
            .memory
            .add_region(PPC_IMPORT_TRAP_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
        let mut lmget_ticks = test_ppc_import_binding(0, "InterfaceLib", "LMGetTicks");
        lmget_ticks.address = PPC_IMPORT_TRAP_BASE;
        lmget_ticks.trap_pc = PPC_IMPORT_TRAP_BASE;
        lmget_ticks.dispatcher_target = PpcImportDispatcherTarget::TickCount;
        ppc_app.import_count = 1;
        ppc_app.imports = vec![lmget_ticks];

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(100, 0x1234_5678, 0);
        runner.init_app(&app);

        let sp = crate::trap::test_helpers::TEST_SP;
        runner.m68k.cpu.write_reg(Register::A7, sp);
        let first = 0x1020_3040;
        runner.bus.write_long(addr::TICKS, first);
        let first_result =
            runner
                .dispatcher
                .dispatch_toolbox(true, 0x175, &mut runner.m68k.cpu, &mut runner.bus);
        assert!(first_result.is_some_and(|result| result.is_ok()));
        assert_eq!(runner.bus.read_long(sp), first);

        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app installed");
        let ppc_app = native_context.adapter_mut();
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(first));

        // Write through the native view between ABI calls. The shared low
        // memory overlay makes the new bytes immediately visible to the 68K
        // bus and to the next native LMGetTicks import.
        let second = 0x5566_7788;
        ppc_app
            .memory
            .write_u32_be(addr::TICKS, second)
            .expect("shared Ticks write");
        runner.m68k.cpu.write_reg(Register::A7, sp);
        let second_result =
            runner
                .dispatcher
                .dispatch_toolbox(true, 0x175, &mut runner.m68k.cpu, &mut runner.bus);
        assert!(second_result.is_some_and(|result| result.is_ok()));
        assert_eq!(runner.bus.read_long(sp), second);

        let third = 0xAABB_CCDD;
        runner.bus.write_long(addr::TICKS, third);
        ppc_app.cpu.pc = PPC_IMPORT_TRAP_BASE;
        ppc_app.cpu.lr = PPC_HALT_PC;
        let probe = runner
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| {
                ppc_app.run_with_process_services(64, false, false, memory_manager, cfm)
            });
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(ppc_app.cpu.gpr[3], third);
        assert_eq!(runner.guest_tick(), third);
    }

    #[test]
    fn ppc_slice_shares_low_memory_time_after_sixtieth_tick() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app
            .memory
            .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
            .expect("rewrite entry as infinite branch");
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_app_start_time(0x1020_3040);
        runner.set_launch_state(59, 1, 0);
        runner.init_app(&app);
        runner.set_instructions_per_tick(1);

        let (steps, running) = runner.run_steps(1, None);

        assert_eq!(steps, 1);
        assert!(running);
        assert_eq!(runner.bus.read_long(addr::TICKS), 60);
        assert_eq!(runner.bus.read_long(addr::TIME), 0x1020_3041);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(60));
        assert_eq!(ppc_app.memory.read_u32_be(addr::TIME), Some(0x1020_3041));
    }

    #[test]
    fn tick_count_poll_fast_forward_returns_for_due_ppc_vbl_callback() {
        use crate::loader::ppc::PpcVblTaskRecord;
        use crate::memory::globals::addr;

        const LOOP: u32 = PPC_CODE_BASE + 0x1000;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x2000;
        const TASK: u32 = PPC_DATA_BASE + 0x3000;
        const CALLBACK_TICK: u32 = PPC_DATA_BASE + 0x3100;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        ppc_app.cpu.pc = LOOP;
        ppc_app.memory.add_region(
            LOOP,
            [
                0x3d80_01f0u32, // lis r12,$01f0
                0x618c_0000,    // ori r12,r12,0
                0x7d89_03a6,    // mtctr r12
                0x4e80_0421,    // bctrl
                0x4bff_fffc,    // b -4
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(
            CALLBACK,
            [
                0x8060_016au32, // lwz r3,$016a(0)
                0x3c80_0200,    // lis r4,$0200
                0x9064_3100,    // stw r3,$3100(r4)
                0x4e80_0020,    // blr
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(TASK, vec![0; 0x104]);
        ppc_app.memory.write_u32_be(TASK + 6, CALLBACK).unwrap();
        ppc_app.memory.write_u16_be(TASK + 10, 1).unwrap();
        ppc_app.vbl_tasks.push(PpcVblTaskRecord {
            task_ptr: TASK,
            architecture: CallbackTaskArchitecture::PowerPc,
            slot: None,
            pending: false,
        });
        let mut tick_count = test_ppc_import_binding(0, "InterfaceLib", "LMGetTicks");
        tick_count.address = PPC_IMPORT_TRAP_BASE;
        tick_count.trap_pc = PPC_IMPORT_TRAP_BASE;
        tick_count.dispatcher_target = PpcImportDispatcherTarget::TickCount;
        ppc_app.import_count = 1;
        ppc_app.imports = vec![tick_count];

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_optional_launch_state(Some(41), None, None);
        runner.init_app(&app);
        runner.set_instructions_per_tick(1_000);

        let (_steps, running) = runner.run_steps(1_000, None);

        assert!(running);
        assert_eq!(runner.bus.read_long(addr::TICKS), 42);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(ppc_app.memory.read_u32_be(CALLBACK_TICK), Some(42));
    }

    #[test]
    fn batched_ppc_vbl_callbacks_read_their_own_tick_from_low_memory() {
        use crate::loader::ppc::PpcVblTaskRecord;
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_app_start_time(0x1020_3040);
        runner.set_optional_launch_state(Some(41), None, None);
        runner.init_app(&app);
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app installed");
        let mut ppc_app = native_context.adapter_mut();
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;

        for (index, count) in [1u16, 2].into_iter().enumerate() {
            let task = PPC_DATA_BASE + 0x1000 + index as u32 * 0x100;
            let callback = task + 0x20;
            ppc_app.memory.add_region(task, vec![0; 0x60]);
            ppc_app.memory.write_u32_be(task + 6, callback).unwrap();
            ppc_app.memory.write_u16_be(task + 10, count).unwrap();
            for (offset, instruction) in [
                0x8060_016au32, // lwz r3,$016a(0)
                0x4e80_0020,    // blr
            ]
            .into_iter()
            .enumerate()
            {
                ppc_app
                    .memory
                    .write_u32_be(callback + offset as u32 * 4, instruction)
                    .unwrap();
            }
            ppc_app.vbl_tasks.push(PpcVblTaskRecord {
                task_ptr: task,
                architecture: CallbackTaskArchitecture::PowerPc,
                slot: None,
                pending: false,
            });
        }

        let (vbl, timer) = runner
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| {
                FixtureRunner::fire_ppc_tick_callbacks(
                    &mut ppc_app,
                    memory_manager,
                    cfm,
                    41,
                    0x1020_3040,
                    2,
                    64,
                    false,
                    false,
                )
            });

        assert_eq!(vbl.len(), 2);
        assert!(timer.is_empty());
        assert_eq!(vbl[0].invocation.tick, 42);
        assert_eq!(vbl[0].invocation.end_r3, 42);
        assert_eq!(vbl[1].invocation.tick, 43);
        assert_eq!(vbl[1].invocation.end_r3, 43);
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(43));
    }

    #[test]
    fn ppc_slice_shares_rnd_seed_in_both_directions() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app
            .memory
            .write_u32_be(PPC_CODE_BASE, 0x3C60_1234) // lis r3,$1234
            .unwrap();
        ppc_app.memory.add_region(
            PPC_CODE_BASE + 4,
            [
                0x6063_5678u32, // ori r3,r3,$5678
                0x3880_0156,    // li r4,$0156
                0x9064_0000,    // stw r3,0(r4)
                0x4800_0000,    // b .
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(0, 1, 0);
        runner.init_app(&app);
        runner.set_instructions_per_tick(100);

        let (steps, running) = runner.run_steps(64, None);
        assert!(steps > 0);
        assert!(running);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x1234_5678);

        runner.bus.write_long(addr::RND_SEED, 0x89AB_CDEF);
        let (steps, running) = runner.run_steps(1, None);
        assert_eq!(steps, 1);
        assert!(running);
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::RND_SEED),
            Some(0x89AB_CDEF)
        );
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89AB_CDEF);
    }

    #[test]
    fn ppc_process_low_memory_has_immediate_bidirectional_visibility() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_optional_launch_state(None, Some(1), None);
        runner.init_app(&app);
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app installed");
        let ppc_app = native_context.adapter_mut();

        for (address, ppc_value, runner_value) in [
            (addr::RND_SEED, 0x1234_5678, 0x89ab_cdef),
            (addr::TICKS, 0x1122_3344, 0x5566_7788),
            (addr::TIME, 0x1020_3040, 0x5060_7080),
        ] {
            ppc_app.memory.write_u32_be(address, ppc_value).unwrap();
            assert_eq!(runner.bus.read_long(address), ppc_value);

            runner.bus.write_long(address, runner_value);
            assert_eq!(ppc_app.memory.read_u32_be(address), Some(runner_value));
        }

        assert_eq!(
            runner.bus.read_word(addr::MENU_FLASH),
            crate::memory::globals::DEFAULT_MENU_FLASH_COUNT
        );
        ppc_app.memory.write_u16_be(addr::MENU_FLASH, 1).unwrap();
        assert_eq!(runner.bus.read_word(addr::MENU_FLASH), 1);
        runner.bus.write_word(addr::MENU_FLASH, 2);
        assert_eq!(ppc_app.memory.read_u16_be(addr::MENU_FLASH), Some(2));

        ppc_app
            .memory
            .write_u32_be(addr::MENU_DISABLE, 0x0080_0002)
            .unwrap();
        assert_eq!(runner.bus.read_long(addr::MENU_DISABLE), 0x0080_0002);
        runner.bus.write_long(addr::MENU_DISABLE, 0x0081_0003);
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::MENU_DISABLE),
            Some(0x0081_0003),
        );

        // Addresses outside the old hand-maintained global list belong to the
        // same process too, including compatibility RAM used by 68K callbacks.
        for address in [0x0000_5000, 0x000f_0000] {
            ppc_app.memory.write_u32_be(address, 0x1234_5678).unwrap();
            assert_eq!(runner.bus.read_long(address), 0x1234_5678);
            runner.bus.write_long(address, 0x89ab_cdef);
            assert_eq!(ppc_app.memory.read_u32_be(address), Some(0x89ab_cdef));
        }
    }

    #[test]
    fn ppc_process_memory_holes_share_runner_ram_without_overlaying_pef_mappings() {
        const FIRST_HOLE: u32 = 0x0018_0000;
        const PEF_MAPPING: u32 = 0x0020_0000;
        const SECOND_HOLE: u32 = PEF_MAPPING + 4;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("synthetic PPC app");
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        ppc_app
            .memory
            .add_readonly_region(PEF_MAPPING, 0x1234_5678u32.to_be_bytes().to_vec());

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.bus.write_long(PEF_MAPPING, 0xaabb_ccdd);
        assert_eq!(
            runner.process_context.memory_ranges(),
            vec![
                (0, PROCESS_LOW_MEMORY_SIZE as usize),
                (PROCESS_LOW_MEMORY_SIZE, 0x0010_0000),
                (SECOND_HOLE, (8 * 1024 * 1024 - SECOND_HOLE) as usize),
            ]
        );

        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app installed");
        let ppc_app = native_context.adapter_mut();
        ppc_app
            .memory
            .write_u32_be(FIRST_HOLE, 0x0102_0304)
            .unwrap();
        assert_eq!(runner.bus.read_long(FIRST_HOLE), 0x0102_0304);
        runner.bus.write_long(SECOND_HOLE, 0x0506_0708);
        assert_eq!(ppc_app.memory.read_u32_be(SECOND_HOLE), Some(0x0506_0708));

        assert_eq!(ppc_app.memory.read_u32_be(PEF_MAPPING), Some(0x1234_5678));
        // Ordinary PEF mappings stay authoritative for the process lifetime,
        // including while the native adapter is parked outside execution.
        assert_eq!(runner.bus.read_long(PEF_MAPPING), 0x1234_5678);
        runner.bus.write_long(PEF_MAPPING, 0);
        assert_eq!(runner.bus.read_long(PEF_MAPPING), 0x1234_5678);
        assert_eq!(ppc_app.memory.read_u32_be(PEF_MAPPING), Some(0x1234_5678));
    }

    #[test]
    fn ppc_input_globals_have_immediate_bidirectional_visibility() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app installed");
        let ppc_app = native_context.adapter_mut();

        assert_eq!(
            runner.bus.read_word(addr::SYS_EVT_MASK),
            crate::memory::globals::DEFAULT_SYS_EVT_MASK
        );
        assert_eq!(
            ppc_app.memory.read_u16_be(addr::SYS_EVT_MASK),
            Some(crate::memory::globals::DEFAULT_SYS_EVT_MASK)
        );
        ppc_app
            .memory
            .write_u16_be(addr::SYS_EVT_MASK, 0xffdf)
            .unwrap();
        assert_eq!(runner.bus.read_word(addr::SYS_EVT_MASK), 0xffdf);
        runner.bus.write_word(addr::SYS_EVT_MASK, 0x1234);
        assert_eq!(ppc_app.memory.read_u16_be(addr::SYS_EVT_MASK), Some(0x1234));

        let mut detached = ppc_app.memory.clone();
        detached.write_u16_be(addr::SYS_EVT_MASK, 0xabcd).unwrap();
        assert_eq!(runner.bus.read_word(addr::SYS_EVT_MASK), 0x1234);
        assert_eq!(ppc_app.memory.read_u16_be(addr::SYS_EVT_MASK), Some(0x1234));

        ppc_app.memory.write_u8(addr::MB_STATE, 0).unwrap();
        assert_eq!(runner.bus.read_byte(addr::MB_STATE), 0);
        runner.bus.write_byte(addr::MB_STATE, 0x80);
        assert_eq!(ppc_app.memory.read_u8(addr::MB_STATE), Some(0x80));

        let ppc_key_map = [
            0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88,
        ];
        ppc_app
            .memory
            .write_bytes(addr::KEY_MAP_LM, &ppc_key_map)
            .unwrap();
        assert_eq!(runner.bus.read_bytes(addr::KEY_MAP_LM, 16), ppc_key_map);

        let runner_key_map = [
            0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11, 0x01, 0x02, 0x04, 0x08, 0x10, 0x20,
            0x40, 0x80,
        ];
        runner.bus.write_bytes(addr::KEY_MAP_LM, &runner_key_map);
        let mut observed_key_map = [0; 16];
        ppc_app
            .memory
            .read_bytes_into(addr::KEY_MAP_LM, &mut observed_key_map)
            .unwrap();
        assert_eq!(observed_key_map, runner_key_map);

        let ppc_points = [
            0x01, 0x02, 0x03, 0x04, 0x11, 0x12, 0x13, 0x14, 0x21, 0x22, 0x23, 0x24,
        ];
        ppc_app
            .memory
            .write_bytes(addr::M_TEMP, &ppc_points)
            .unwrap();
        assert_eq!(runner.bus.read_bytes(addr::M_TEMP, 12), ppc_points);

        let runner_points = [
            0x24, 0x23, 0x22, 0x21, 0x14, 0x13, 0x12, 0x11, 0x04, 0x03, 0x02, 0x01,
        ];
        runner.bus.write_bytes(addr::M_TEMP, &runner_points);
        let mut observed_points = [0; 12];
        ppc_app
            .memory
            .read_bytes_into(addr::M_TEMP, &mut observed_points)
            .unwrap();
        assert_eq!(observed_points, runner_points);

        let native_key_map = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ];
        ppc_app.set_input_snapshot(PpcInputSnapshot {
            key_map: native_key_map,
            mouse_button: true,
            mouse_v: 123,
            mouse_h: 456,
        });
        assert_eq!(runner.bus.read_byte(addr::MB_STATE), 0);
        assert_eq!(runner.bus.read_bytes(addr::KEY_MAP_LM, 16), native_key_map);
        for point_addr in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
            assert_eq!(runner.bus.read_word(point_addr), 123);
            assert_eq!(runner.bus.read_word(point_addr + 2), 456);
        }

        runner.push_key_down(6, b'z');
        runner.push_mouse_down(-7, 321);
        let mut host_key_map = [0; 16];
        ppc_app
            .memory
            .read_bytes_into(addr::KEY_MAP_LM, &mut host_key_map)
            .unwrap();
        assert_eq!(host_key_map, runner.dispatcher.input_state.key_map);
        assert_eq!(ppc_app.memory.read_u8(addr::MB_STATE), Some(0));
        for point_addr in [addr::M_TEMP, addr::MOUSE_LOC, addr::MOUSE_LOC2] {
            assert_eq!(ppc_app.memory.read_u16_be(point_addr), Some((-7i16) as u16));
            assert_eq!(ppc_app.memory.read_u16_be(point_addr + 2), Some(321));
        }

        ppc_app
            .memory
            .write_u32_be(addr::THE_ZONE, 0x1122_3344)
            .unwrap();
        assert_eq!(runner.bus.read_long(addr::THE_ZONE), 0x1122_3344);
        runner.bus.write_long(addr::THE_ZONE, 0x5566_7788);
        assert_eq!(
            ppc_app.memory.read_u32_be(addr::THE_ZONE),
            Some(0x5566_7788)
        );
    }

    #[test]
    fn init_app_propagates_size_high_level_event_capability() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let unaware_size = size_resource_bytes(0, 0x0008_0000, 0x0008_0000);
        let unaware_fork_bytes = make_resource_fork_bytes(&[
            (*b"CODE", 0, code0.as_slice()),
            (*b"SIZE", -1, unaware_size.as_slice()),
        ]);
        let unaware_fork =
            ResourceFork::parse(&unaware_fork_bytes).expect("parse unaware app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let unaware_app = runner.load_app(&unaware_fork).expect("load unaware app");
        runner.init_app(&unaware_app);
        assert!(!runner
            .dispatcher
            .apple_event_launch_state
            .is_high_level_event_aware());

        let aware_size = size_resource_bytes(
            ApplicationSizeResource::HIGH_LEVEL_EVENT_AWARE,
            0x0008_0000,
            0x0008_0000,
        );
        let aware_fork_bytes = make_resource_fork_bytes(&[
            (*b"CODE", 0, code0.as_slice()),
            (*b"SIZE", -1, aware_size.as_slice()),
        ]);
        let aware_fork = ResourceFork::parse(&aware_fork_bytes).expect("parse aware app fork");
        let aware_app = runner.load_app(&aware_fork).expect("load aware app");
        runner.init_app(&aware_app);
        assert!(runner
            .dispatcher
            .apple_event_launch_state
            .is_high_level_event_aware());
    }

    #[test]
    fn init_app_seeds_classic_double_click_interval() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_long(addr::DOUBLE_TIME),
            DEFAULT_DOUBLE_TIME_TICKS,
            "a zero DoubleTime makes every application-level double-click test fail"
        );
    }

    #[test]
    fn load_app_places_resources_above_large_loaded_image() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0x001D_0000, 0x0340, 0, 0);
        let marker = [0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78];
        let fork_bytes =
            make_resource_fork_bytes(&[(*b"BGAS", 128, &marker), (*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        assert!(
            app.loaded_image_end > APP_HEAP_FLOOR,
            "fixture should force the loaded image across the default heap floor"
        );

        let heap_start = app_heap_start_for_loaded_app(&app);
        let (_, marker_ptr) = runner
            .dispatcher
            .find_or_load_resource_any(&mut runner.bus, *b"BGAS", 128)
            .expect("BGAS resource loaded");
        assert!(
            marker_ptr >= heap_start + APP_ZONE_HEADER_SIZE,
            "resource data must be allocated after the relocated zone header"
        );
        assert_eq!(runner.bus.read_bytes(marker_ptr, marker.len()), marker);

        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::APP_L_ZONE), heap_start);
        assert_eq!(
            runner.bus.read_long(addr::HEAP_END),
            heap_start + APP_ZONE_HEADER_SIZE
        );
        assert_eq!(
            runner.bus.read_bytes(marker_ptr, marker.len()),
            marker,
            "launch initialization must not clobber resources for large loaded images"
        );
    }

    #[test]
    fn load_app_records_application_size_resource_id_minus_one() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let size = size_resource_bytes(0x0080, 0x0030_0000, 0x0020_0000);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert_eq!(
            app.size_resource,
            Some(ApplicationSizeResource {
                flags: 0x0080,
                preferred_size: 0x0030_0000,
                minimum_size: 0x0020_0000,
            })
        );
    }

    #[test]
    fn load_app_prefers_valid_size_resource_id_zero() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let original = size_resource_bytes(0x0040, 0x0030_0000, 0x0020_0000);
        let finder_override = size_resource_bytes(0x0080, 0x0050_0000, 0x0040_0000);
        let fork_bytes = make_resource_fork_bytes(&[
            (*b"CODE", 0, &code0),
            (*b"SIZE", -1, &original),
            (*b"SIZE", 0, &finder_override),
        ]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(16 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert_eq!(
            app.size_resource,
            Some(ApplicationSizeResource {
                flags: 0x0080,
                preferred_size: 0x0050_0000,
                minimum_size: 0x0040_0000,
            })
        );
    }

    #[test]
    fn load_app_falls_back_to_size_resource_id_minus_one_when_id_zero_is_invalid() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let original = size_resource_bytes(0x0040, 0x0030_0000, 0x0020_0000);
        let invalid_override = size_resource_bytes(0x0080, 0x0010_0000, 0x0020_0000);
        let fork_bytes = make_resource_fork_bytes(&[
            (*b"CODE", 0, &code0),
            (*b"SIZE", -1, &original),
            (*b"SIZE", 0, &invalid_override),
        ]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert_eq!(
            app.size_resource,
            Some(ApplicationSizeResource {
                flags: 0x0040,
                preferred_size: 0x0030_0000,
                minimum_size: 0x0020_0000,
            })
        );
    }

    #[test]
    fn load_app_relocates_large_size_partition_a5_above_application_zone() {
        let minimum_partition = 4_812_800;
        let preferred_partition = 6_348_800;
        let below_a5 = 0x7AF4;
        let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
        let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert!(
            app_image_start_for_loaded_app(&app) > APP_HEAP_FLOOR + APP_ZONE_HEADER_SIZE,
            "relocated app image must leave room for the visible app-zone header"
        );
        assert!(
            app.a5_base - APP_HEAP_FLOOR >= minimum_partition - APP_STACK_SAFETY_MARGIN,
            "large SIZE partitions should place A5 high enough for direct A5-zone memory checks"
        );
        assert!(
            app.a5_base - APP_HEAP_FLOOR >= 750 * 1024,
            "Spectre-style startup gates compare A5 - GetZone against a 750K floor"
        );
    }

    #[test]
    fn large_size_partition_does_not_overwrite_synthetic_callbacks() {
        let below_a5 = 0x7AF4;
        let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
        let partition = 63 * 1024 * 1024;
        let size = size_resource_bytes(0x5880, partition, partition);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
        let callback = runner.bus.alloc_synthetic(2);
        runner.bus.write_word(callback, 0x4E75);

        let app = runner.load_app(&fork).expect("load app");

        assert_eq!(runner.bus.read_word(callback), 0x4E75);
        assert!(
            app.loaded_image_end + APP_HIGH_MEMORY_RESERVE <= runner.bus.application_memory_limit(),
            "the loaded image must leave room for the application heap and stack"
        );
        assert!(app.initial_sp < callback);
    }

    #[test]
    fn load_app_relocates_exact_2mb_size_partition() {
        let below_a5 = 0x68E8;
        let code0 = minimal_code0(0x0D18, below_a5, 0, 0);
        let size = size_resource_bytes(0x5880, 0x0020_0000, 0x0020_0000);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert!(app_image_start_for_loaded_app(&app) > APP_HEAP_FLOOR);
        assert!(app.a5_base - APP_HEAP_FLOOR >= 0x0020_0000 - APP_STACK_SAFETY_MARGIN);
    }

    #[test]
    fn load_app_relocates_sub_2mb_size_partition() {
        let preferred_partition = 1_843_200;
        let minimum_partition = 768_000;
        let below_a5 = 29_116;
        let code0 = minimal_code0(3_816, below_a5, 0, 0);
        let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"SIZE", -1, &size)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");

        assert!(app_image_start_for_loaded_app(&app) > APP_HEAP_FLOOR);
        assert!(
            app.a5_base - APP_HEAP_FLOOR >= preferred_partition - APP_STACK_SAFETY_MARGIN,
            "sub-2 MiB SIZE partitions must still govern the classic A5 layout"
        );
    }

    #[test]
    fn size_partition_does_not_cap_shared_resource_fork_materialization() {
        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let size = size_resource_bytes(0x5880, 4_194_304, 3_584_000);
        let large_resource = vec![0xA5; 6_481_428];
        let fork_bytes = make_resource_fork_bytes(&[
            (*b"CODE", 0, &code0),
            (*b"SHAP", 128, &large_resource),
            (*b"SIZE", -1, &size),
        ]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        // Systemless currently materializes resource-fork data in the shared
        // bus. A classic resource map can instead leave nonpreloaded data on
        // disk until requested, so a guest SIZE limit cannot bound this shared
        // storage until it has a separate arena. More Macintosh Toolbox
        // (1993), pp. 1-8 to 1-9.
        let (_, resource_ptr) = runner
            .dispatcher
            .find_or_load_resource_any(&mut runner.bus, *b"SHAP", 128)
            .expect("large resource registered");
        assert_ne!(
            resource_ptr, 0,
            "large resource data must remain allocatable"
        );
        assert_eq!(runner.bus.read_byte(resource_ptr), 0xA5);
        assert_eq!(
            runner
                .bus
                .read_byte(resource_ptr + large_resource.len() as u32 - 1),
            0xA5
        );
    }

    #[test]
    fn init_app_exposes_low_visible_zone_for_relocated_size_partition() {
        use crate::memory::globals::addr;

        let minimum_partition = 4_812_800;
        let preferred_partition = 6_348_800;
        let below_a5 = 0x7AF4;
        let code0 = minimal_code0(0x11F8, below_a5, 0, 0);
        let marker = [0xCA, 0xFE, 0xBA, 0xBE, 0x12, 0x34, 0x56, 0x78];
        let size = size_resource_bytes(0x5880, preferred_partition, minimum_partition);
        let fork_bytes = make_resource_fork_bytes(&[
            (*b"BGAS", 128, &marker),
            (*b"CODE", 0, &code0),
            (*b"SIZE", -1, &size),
        ]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(32 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let image_start = app_image_start_for_loaded_app(&app);
        let (_, marker_ptr) = runner
            .dispatcher
            .find_or_load_resource_any(&mut runner.bus, *b"BGAS", 128)
            .expect("BGAS resource loaded");
        let marker_end = marker_ptr + marker.len() as u32;
        assert!(
            marker_end <= image_start || marker_ptr >= app.loaded_image_end,
            "resource allocation must not overlap the relocated image"
        );

        runner.init_app(&app);

        assert_eq!(runner.bus.read_long(addr::APP_L_ZONE), APP_HEAP_FLOOR);
        assert_eq!(runner.bus.read_long(addr::THE_ZONE), APP_HEAP_FLOOR);
        assert_eq!(
            runner.bus.read_long(addr::HEAP_END),
            APP_HEAP_FLOOR + APP_ZONE_HEADER_SIZE
        );
        assert_eq!(
            runner.bus.read_long(APP_HEAP_FLOOR),
            runner.bus.read_long(addr::APPL_LIMIT)
        );
        assert!(
            app.a5_base - runner.bus.read_long(addr::THE_ZONE) >= 750 * 1024,
            "GetZone-visible partition span should satisfy direct startup memory gates"
        );
        assert_eq!(runner.bus.read_bytes(marker_ptr, marker.len()), marker);
    }

    #[test]
    fn load_app_patches_mpw_far_jump_table_offsets_from_segment_start() {
        // Inside Macintosh: Processes 1994, p. 7-8: a loaded MPW jump-table
        // entry keeps the routine offset from the beginning of the segment.
        let mut code0 = minimal_code0(40, 0x2000, 8, 32);
        code0[16..24].copy_from_slice(&[
            0x00, 0x01, // segment 1
            0xA9, 0xF0, // far-model unloaded LoadSeg trap
            0x00, 0x00, 0x00, 0x28, // first routine immediately after the far header
        ]);

        let mut code1 = vec![0u8; 0x30];
        code1[0] = 0xFF;
        code1[1] = 0xFF;
        code1[0x28] = 0x4E;
        code1[0x29] = 0x75;

        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let jt_base = app.a5_base + app.code0_header.jump_table_offset;
        let code1_base = app.segment_bases[&1];

        assert_eq!(runner.bus.read_word(jt_base), 1);
        assert_eq!(runner.bus.read_word(jt_base + 2), 0x4EF9);
        assert_eq!(
            runner.bus.read_long(jt_base + 4),
            code1_base + 0x28,
            "MPW far offsets must not be adjusted by the 40-byte header twice"
        );
    }

    #[test]
    fn load_app_executes_far_jump_table_routine_above_64k() {
        let offset = 0x0001_A114u32;
        let mut code0 = minimal_code0(40, 0x2000, 8, 32);
        code0[16..20].copy_from_slice(&[0x00, 0x01, 0xA9, 0xF0]);
        code0[20..24].copy_from_slice(&offset.to_be_bytes());

        let mut code1 = vec![0; offset as usize + 4];
        code1[..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        // Distinct routines expose accidental truncation during execution.
        code1[0xA114..0xA118].copy_from_slice(&[0x70, 0x01, 0x4E, 0x75]);
        code1[offset as usize..].copy_from_slice(&[0x70, 0x2A, 0x4E, 0x75]);
        let bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
        let fork = ResourceFork::parse(&bytes).expect("synthetic far-model resource fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = runner.load_app(&fork).expect("load far-model application");
        let jt = app.a5_base + app.code0_header.jump_table_offset;

        runner.cpu_mut().write_reg(Register::PC, jt + 2);
        runner.step(); // JMP through the patched slot.
        runner.step(); // MOVEQ from the selected routine.
        assert_eq!(runner.cpu().read_reg(Register::D0), 42);
        assert_eq!(app.jump_table[0].offset, offset);
        assert_eq!(runner.bus.read_long(jt + 4), app.segment_bases[&1] + offset);
    }

    #[test]
    fn load_app_applies_mpw_far_a5_and_pc_relocations() {
        let mut code0 = minimal_code0(40, 0x2000, 8, 32);
        code0[16..24].copy_from_slice(&[
            0x00, 0x01, // segment 1
            0xA9, 0xF0, // far-model unloaded LoadSeg trap
            0x00, 0x00, 0x00, 0x28,
        ]);

        let mut code1 = vec![0u8; 0x130];
        code1[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        code1[20..24].copy_from_slice(&0x50u32.to_be_bytes());
        code1[28..32].copy_from_slice(&0x54u32.to_be_bytes());
        code1[0x28..0x2A].copy_from_slice(&[0x4E, 0xB9]); // JSR.L absolute
        code1[0x2A..0x2E].copy_from_slice(&0x100u32.to_be_bytes());
        code1[0x30..0x32].copy_from_slice(&[0x20, 0x79]); // MOVEA.L absolute
        code1[0x32..0x36].copy_from_slice(&0x40u32.to_be_bytes());
        code1[0x100..0x104].copy_from_slice(&[0x70, 0x01, 0x4E, 0x75]);
        code1[0x128..0x12C].copy_from_slice(&[0x70, 0x2A, 0x4E, 0x75]);
        code1[0x50..0x54].copy_from_slice(&[
            0x19, // A5 relocation at byte offset 0x32
            0x00, 0x00, 0x00,
        ]);
        code1[0x54..0x57].copy_from_slice(&[
            0x15, // PC relocation at byte offset 0x2A
            0x00, 0x00,
        ]);

        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0), (*b"CODE", 1, &code1)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load app");
        let code1_base = app.segment_bases[&1];

        assert_eq!(
            runner.bus.read_long(code1_base + 0x2A),
            code1_base + MpwFarSegmentHeader::SIZE as u32 + 0x100,
            "PC relocation stream must add the loaded code address after the far header"
        );
        runner.cpu_mut().write_reg(Register::PC, code1_base + 0x28);
        runner.step(); // JSR using the relocated code-relative address.
        runner.step(); // MOVEQ at the destination, not 40 bytes before it.
        assert_eq!(runner.cpu().read_reg(Register::D0), 42);
        assert_eq!(
            runner.bus.read_long(code1_base + 0x32),
            app.a5_base + 0x40,
            "A5 relocation stream must add the current A5"
        );
        assert_eq!(
            runner
                .bus
                .read_long(code1_base + MpwFarSegmentHeader::CURRENT_A5_OFFSET),
            app.a5_base
        );
        assert_eq!(
            runner
                .bus
                .read_long(code1_base + MpwFarSegmentHeader::LOAD_ADDRESS_OFFSET),
            code1_base
        );
    }

    #[test]
    fn load_app_applies_retro68_code_and_data_relocations() {
        let mut code0 = minimal_code0(40, 0x2000, 8, 32);
        code0[16..24].copy_from_slice(&[
            0x00, 0x01, // segment 1
            0xA9, 0xF0, // far-model unloaded LoadSeg trap
            0x00, 0x00, 0x00, 0x28,
        ]);

        let mut code1 = vec![0u8; 0x60];
        code1[0..2].copy_from_slice(&0xFFFFu16.to_be_bytes());
        for (offset, value) in [0x20u32, 0x30, 0x40, 0x50, 0x60].into_iter().enumerate() {
            let start = MpwFarSegmentHeader::SIZE + offset * 4;
            code1[start..start + 4].copy_from_slice(&value.to_be_bytes());
        }
        let rela1 = [
            0x04, // absolute offset 0, code base
            0x11, // absolute offset 4, data base
            0x12, // absolute offset 8, bss base
            0x13, // absolute offset 12, jump-table base
            0x00, // absolute pass terminator
            0x44, // PC-relative offset 16, code base
            0x00, // PC-relative pass terminator
        ];

        let mut data = Vec::new();
        for value in [0x10u32, 0x20, 0x30, 0x40] {
            data.extend_from_slice(&value.to_be_bytes());
        }
        let rela0 = [
            0x04, // absolute offset 0, code base
            0x11, // absolute offset 4, data base
            0x12, // absolute offset 8, bss base
            0x13, // absolute offset 12, jump-table base
            0x00, // absolute pass terminator
            0x00, // empty PC-relative pass
        ];

        let fork_bytes = make_resource_fork_bytes(&[
            (*b"CODE", 0, &code0),
            (*b"CODE", 1, &code1),
            (*b"DATA", 0, &data),
            (*b"RELA", 0, &rela0),
            (*b"RELA", 1, &rela1),
        ]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic Retro68 app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        let app = runner.load_app(&fork).expect("load Retro68 app");
        let code1_base = app.segment_bases[&1];
        let code_body = code1_base + MpwFarSegmentHeader::SIZE as u32;
        let data_base = app.a5_base - app.code0_header.below_a5;

        assert_eq!(runner.bus.read_long(code_body), code1_base + 0x20);
        assert_eq!(runner.bus.read_long(code_body + 4), app.a5_base + 0x30);
        assert_eq!(runner.bus.read_long(code_body + 8), app.a5_base + 0x40);
        assert_eq!(runner.bus.read_long(code_body + 12), app.a5_base + 0x50);
        assert_eq!(
            runner.bus.read_long(code_body + 16),
            0x28,
            "the first PC-relative record must immediately follow the first terminator"
        );
        assert_eq!(runner.bus.read_long(data_base), 0x10);
        assert_eq!(runner.bus.read_long(data_base + 4), app.a5_base + 0x20);
        assert_eq!(runner.bus.read_long(data_base + 8), app.a5_base + 0x30);
        assert_eq!(runner.bus.read_long(data_base + 12), app.a5_base + 0x40);
        assert_eq!(
            runner
                .bus
                .read_long(code1_base + MpwFarSegmentHeader::CURRENT_A5_OFFSET),
            app.a5_base
        );
        assert_eq!(
            runner
                .bus
                .read_long(code1_base + MpwFarSegmentHeader::LOAD_ADDRESS_OFFSET),
            code1_base
        );
    }

    #[test]
    fn invalid_retro68_relocations_do_not_partially_modify_memory() {
        let mut runner = FixtureRunner::new(1024 * 1024, FixtureRunnerConfig::default());
        let target = 0x1000;
        runner.bus.write_long(target, 0x20);

        let result = apply_retro68_rela_relocations(
            &mut runner.bus,
            target,
            4,
            &[
                0x04, // valid absolute relocation at offset 0
                0x00, // absolute pass terminator
                0x08, // invalid PC-relative relocation at offset 1
                0x00,
            ],
            [0x100, 0, 0, 0],
        );

        assert_eq!(
            result,
            Err(Retro68RelocationError::TargetOutOfBounds {
                offset: 1,
                target_size: 4,
            })
        );
        assert_eq!(runner.bus.read_long(target), 0x20);
    }

    #[test]
    fn event_yield_services_pending_launch_application_from_vfs() {
        use crate::memory::globals::addr;

        let current_code0 = minimal_code0(0, 0x2000, 0, 0);
        let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
        let current_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &current_code0)]);
        let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
        let current_fork =
            ResourceFork::parse(&current_fork_bytes).expect("parse current app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner
            .dispatcher
            .vfs
            .insert("Apps/Main App".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Main App".to_string(), current_fork_bytes);
        runner
            .dispatcher
            .vfs
            .insert("Apps/Register Helper".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
        runner.dispatcher.ensure_vfs_catalog();
        runner.dispatcher.set_launched_app_path("Apps/Main App");

        let app = runner.load_app(&current_fork).expect("load current app");
        runner.init_app(&app);
        let tick_count_entry = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
        runner.bus.write_long(tick_count_entry, 0x0020_1000);
        assert!(runner.dispatcher.has_native_trap_patch(&runner.bus, 0xA975));
        runner.bus.write_long(addr::TICKS, 1234);
        runner.bus.write_long(addr::TIME, 0x1020_3040);
        runner.bus.write_long(addr::RND_SEED, 0x89AB_CDEF);
        runner.set_guest_tick_for_test(1234);
        runner
            .dispatcher
            .queue_pending_launch_application("Apps/Register Helper", true);

        assert!(
            !runner.service_pending_launch_application(false, false),
            "launchContinue target must wait for an Event Manager yield"
        );
        let switched = runner.service_pending_launch_application(true, false);

        assert!(
            switched,
            "event yield should service the queued helper launch"
        );
        assert!(
            !runner.is_halted(),
            "queued helper launch should not halt the runner"
        );
        assert_eq!(
            runner.dispatcher.launched_app_path(),
            Some("Apps/Register Helper")
        );
        assert_eq!(
            runner.bus.read_long(addr::TICKS),
            1234,
            "Process Manager launch must preserve system TickCount"
        );
        assert_eq!(runner.bus.read_long(addr::TIME), 0x1020_3040);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89AB_CDEF);
        assert!(
            !runner.dispatcher.has_native_trap_patch(&runner.bus, 0xA975),
            "application trap patches must be torn down at a foreground launch"
        );
        let cur_ap_len = runner.bus.read_byte(addr::CUR_APNAME) as usize;
        let cur_ap_name = String::from_utf8(
            (0..cur_ap_len)
                .map(|i| runner.bus.read_byte(addr::CUR_APNAME + 1 + i as u32))
                .collect(),
        )
        .expect("CurApName is ASCII");
        assert_eq!(cur_ap_name, "Register Helper");
        assert!(
            runner.dispatcher.vfs.contains_key("Apps/Main App"),
            "archive VFS entries must survive the foreground app switch"
        );
        assert!(
            runner
                .dispatcher
                .vfs_rsrc
                .contains_key("Apps/Register Helper"),
            "launched app resource fork must remain available after the switch"
        );
    }

    #[test]
    fn immediate_pending_launch_application_switches_from_vfs_without_event_yield() {
        use crate::memory::globals::addr;

        let current_code0 = minimal_code0(0, 0x2000, 0, 0);
        let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
        let current_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &current_code0)]);
        let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
        let current_fork =
            ResourceFork::parse(&current_fork_bytes).expect("parse current app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner
            .dispatcher
            .vfs
            .insert("Apps/Main App".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Main App".to_string(), current_fork_bytes);
        runner
            .dispatcher
            .vfs
            .insert("Apps/Register Helper".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
        runner.dispatcher.ensure_vfs_catalog();
        runner.dispatcher.set_launched_app_path("Apps/Main App");

        let app = runner.load_app(&current_fork).expect("load current app");
        runner.init_app(&app);
        runner.bus.write_long(addr::TICKS, 4321);
        runner.bus.write_long(addr::TIME, 0x5060_7080);
        runner.bus.write_long(addr::RND_SEED, 0x7654_3210);
        runner.set_guest_tick_for_test(4321);
        runner
            .dispatcher
            .queue_pending_launch_application("Apps/Register Helper", false);

        let switched = runner.service_pending_launch_application(false, false);

        assert!(
            switched,
            "immediate pending launch should not require an Event Manager yield"
        );
        assert!(
            !runner.is_halted(),
            "immediate queued helper launch should not halt the runner"
        );
        assert_eq!(
            runner.dispatcher.launched_app_path(),
            Some("Apps/Register Helper")
        );
        assert_eq!(
            runner.bus.read_long(addr::TICKS),
            4321,
            "foreground app switch must preserve system TickCount"
        );
        assert_eq!(runner.bus.read_long(addr::TIME), 0x5060_7080);
        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x7654_3210);
        let cur_ap_len = runner.bus.read_byte(addr::CUR_APNAME) as usize;
        let cur_ap_name = String::from_utf8(
            (0..cur_ap_len)
                .map(|i| runner.bus.read_byte(addr::CUR_APNAME + 1 + i as u32))
                .collect(),
        )
        .expect("CurApName is ASCII");
        assert_eq!(cur_ap_name, "Register Helper");
    }

    #[test]
    fn init_app_writes_cur_ap_name_as_mac_roman() {
        use crate::memory::globals::addr;

        let code0 = minimal_code0(0, 0x2000, 0, 0);
        let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
        let fork = ResourceFork::parse(&fork_bytes).expect("parse app fork");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher
            .set_launched_app_path("Games/Shufflepuck Café v1.0");

        let app = runner.load_app(&fork).expect("load app");
        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_pstring(addr::CUR_APNAME),
            b"Shufflepuck Caf\x8E v1.0",
            "CurApName is a classic Str31 and must preserve MacRoman filenames"
        );
        assert_eq!(
            crate::trap::dispatch::TrapDispatcher::read_pb_filename(&runner.bus, addr::CUR_APNAME),
            "Shufflepuck Café v1.0"
        );
    }

    #[test]
    fn fixture_runner_defaults_to_classic_theme() {
        let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        assert_eq!(runner.ui_theme_id(), UiThemeId::ClassicSystem7);
        assert_eq!(
            runner.dispatcher().ui_theme_id(),
            UiThemeId::ClassicSystem7
        );
        assert_eq!(runner.ui_theme().id(), UiThemeId::ClassicSystem7);
        assert_eq!(
            runner.theme_metrics_mode(),
            ThemeMetricsMode::ClassicGuestMetrics
        );
        assert!(runner.uses_classic_guest_metrics());
    }

    #[test]
    fn fixture_runner_accepts_explicit_classic_theme_without_themed_metrics() {
        let runner = FixtureRunner::new(
            8 * 1024 * 1024,
            FixtureRunnerConfig {
                ui_theme: UiThemeId::ClassicSystem7,
                theme_metrics_mode: ThemeMetricsMode::ClassicGuestMetrics,
                ..FixtureRunnerConfig::default()
            },
        );
        let systemless = UiThemeId::SystemlessDefault.provider();

        assert_eq!(runner.ui_theme_id(), UiThemeId::ClassicSystem7);
        assert_eq!(runner.dispatcher().ui_theme_id(), UiThemeId::ClassicSystem7);
        assert_eq!(runner.ui_theme().id(), UiThemeId::ClassicSystem7);
        assert!(runner.uses_classic_guest_metrics());
        assert_eq!(runner.ui_theme().menu_metrics(), systemless.menu_metrics());
        assert_eq!(
            runner.ui_theme().control_metrics(),
            systemless.control_metrics()
        );
        assert_ne!(runner.ui_theme().palette(), systemless.palette());
    }

    #[test]
    fn fixture_runner_can_select_classic_theme_before_guest_initialization() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.set_ui_theme(UiThemeId::ClassicSystem7);

        assert_eq!(runner.ui_theme_id(), UiThemeId::ClassicSystem7);
        assert_eq!(runner.dispatcher().ui_theme_id(), UiThemeId::ClassicSystem7);
        assert!(runner.uses_classic_guest_metrics());
    }

    fn write_double_buffer(bus: &mut MacMemoryBus, ptr: u32, samples: &[u8]) {
        bus.write_long(ptr, samples.len() as u32);
        bus.write_long(ptr + 4, 0x0000_0001);
        for (offset, sample) in samples.iter().copied().enumerate() {
            bus.write_byte(ptr + 16 + offset as u32, sample);
        }
    }

    #[test]
    fn headless_run_steps_does_not_implicitly_mix_audio() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        for offset in (0..16).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71); // NOP
        }
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);

        let mut chan = SndChannel::new(0x0039_38C8, false);
        chan.play_buffer(
            vec![0x90, 0x91, 0x92],
            OUTPUT_RATE << 16,
            PlaybackKind::Buffer,
            0,
        );
        runner.dispatcher.sound_manager.add_channel(chan);

        let (steps, running) = runner.run_steps(2, None);

        assert!(running);
        assert_eq!(steps, 2);
        assert_eq!(
            runner.audio_buffer_len(),
            0,
            "plain headless stepping must not consume sound buffers"
        );
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 0);

        runner.mix_audio(2);
        assert_eq!(runner.drain_audio(), vec![0x90, 0x91]);
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 2);
    }

    #[test]
    fn headless_run_steps_advances_playback_needed_for_sound_callbacks() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        runner.bus.write_word(program_start, 0x60FE); // BRA.S *
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.set_instructions_per_tick(1);

        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0001_1000;
        let callback_cmd = SndCommand {
            cmd: crate::sound::cmd::CALLBACK,
            param1: 0x1234,
            param2: 0x0056_7890,
        };
        let mut chan = SndChannel::new(chan_ptr, false);
        chan.callback_addr = callback_addr;
        chan.play_buffer(vec![0x90; 500], OUTPUT_RATE << 16, PlaybackKind::Buffer, 0);
        chan.queue_callback(callback_cmd.clone());
        runner.dispatcher.sound_manager.add_channel(chan);

        let (steps, running) = runner.run_steps(2, None);

        assert!(running);
        assert_eq!(steps, 2);
        assert_eq!(
            runner
                .dispatcher
                .sound_manager
                .pending_sound_callbacks
                .len(),
            1
        );
        assert!(matches!(
            &runner.dispatcher.sound_manager.pending_sound_callbacks[0],
            PendingSoundCallback::Command {
                architecture: actual_architecture,
                callback_addr: actual_callback,
                chan_ptr: actual_channel,
                cmd,
            } if *actual_architecture == CallbackTaskArchitecture::M68k
                && *actual_callback == callback_addr
                && *actual_channel == chan_ptr
                && cmd.cmd == callback_cmd.cmd
                && cmd.param1 == callback_cmd.param1
                && cmd.param2 == callback_cmd.param2
        ));
        assert_eq!(
            runner.audio_buffer_len(),
            500,
            "the complete callback-gated buffer should be captured before completion"
        );
    }

    #[test]
    fn host_audio_backend_receives_silence_while_sound_manager_idle() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
        runner.set_audio(Box::new(audio_backend));

        runner.mix_audio(4);

        assert_eq!(
            stereo_samples.borrow().as_slice(),
            &[0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80]
        );
        assert_eq!(
            runner.audio_buffer_len(),
            0,
            "host silence must not become captured guest audio"
        );
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 0);
    }

    #[test]
    fn host_audio_backend_receives_low_rate_sample_hold_output() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let (audio_backend, stereo_samples) = CapturingAudioBackend::new();
        runner.set_audio(Box::new(audio_backend));

        let mut chan = SndChannel::new(0x0039_38C8, false);
        chan.play_buffer(
            vec![0x90, 0xA0],
            (OUTPUT_RATE / 2) << 16,
            PlaybackKind::Buffer,
            0,
        );
        runner.dispatcher.sound_manager.add_channel(chan);

        runner.mix_audio(4);

        assert_eq!(
            stereo_samples.borrow().as_slice(),
            &[
                0x90, 0x90, // source[0] at position 0.0
                0x90, 0x90, // source[0] held at position 0.5
                0xA0, 0xA0, // source[1] at position 1.0
                0xA0, 0xA0, // source[1] held at position 1.5
            ],
            "GUI/host audio path must receive the sample-hold low-rate output, not the old linear midpoint"
        );
        assert_eq!(runner.dispatcher.sound_manager.debug_samples_mixed, 4);
    }

    fn dialog_tracking_for_test(filter_proc: u32, item_hit_ptr: u32) -> DialogTrackingState {
        DialogTrackingState {
            dialog_ptr: 0x0020_0000,
            bounds: (0, 0, 32, 32),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new().into(),
            stack_ptr: 0,
            item_hit_ptr,
            rendered_pixels: Vec::new().into(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        }
    }

    fn test_ppc_import_binding(symbol_index: u32, library: &str, symbol: &str) -> PpcImportBinding {
        PpcImportBinding {
            library_index: 0,
            symbol_index,
            library_name: library.to_string(),
            symbol_name: symbol.to_string(),
            class: 0,
            weak: false,
            address: 0,
            tvector_address: None,
            trap_pc: 0,
            dispatcher_target: PpcImportDispatcherTarget::Unsupported,
        }
    }

    #[test]
    fn ppc_exit_to_shell_halt_is_distinct_from_other_ppc_stops() {
        let mut exit = test_ppc_import_binding(7, "InterfaceLib", "ExitToShell");
        exit.dispatcher_target = PpcImportDispatcherTarget::ExitToShell;
        let imports = vec![exit];
        let halted = PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 4,
        };

        assert!(ppc_halted_by_exit_to_shell(&imports, halted, Some(7), None));
        assert!(!ppc_halted_by_exit_to_shell(
            &imports,
            halted,
            Some(7),
            Some(7)
        ));
        assert!(!ppc_halted_by_exit_to_shell(
            &imports,
            halted,
            Some(8),
            None
        ));
        let mut unsupported = test_ppc_import_binding(7, "InterfaceLib", "MysteryCall");
        unsupported.dispatcher_target = PpcImportDispatcherTarget::Unsupported;
        let duplicate_imports = vec![unsupported, imports[0].clone()];
        assert!(!ppc_halted_by_exit_to_shell(
            &duplicate_imports,
            halted,
            Some(7),
            None
        ));
        assert!(!ppc_halted_by_exit_to_shell(
            &imports,
            PpcRunResult::MemoryFault {
                pc: PPC_CODE_BASE,
                addr: 0,
                was_write: false,
                cycles: 4,
            },
            Some(7),
            None
        ));
    }

    #[test]
    fn ppc_unimpl_histogram_key_names_unsupported_imports() {
        let imports = vec![test_ppc_import_binding(7, "InterfaceLib", "MysteryCall")];

        assert_eq!(
            ppc_unimpl_histogram_key(&imports, PpcRunResult::CycleLimit { cycles: 0 }, Some(7)),
            Some("import #7 InterfaceLib:MysteryCall".to_string())
        );
        assert_eq!(
            ppc_unimpl_histogram_key(&imports, PpcRunResult::CycleLimit { cycles: 0 }, Some(8)),
            Some("import #8 <unknown>".to_string())
        );
    }

    #[test]
    fn ppc_unimpl_histogram_key_records_instruction_decode_errors() {
        assert_eq!(
            ppc_unimpl_histogram_key(
                &[],
                PpcRunResult::Unimplemented {
                    pc: 0x0100_0000,
                    error: ppc::PpcDecodeError::UnsupportedPrimaryOpcode(1),
                    cycles: 12,
                },
                None,
            ),
            Some("instruction pc=$01000000 UnsupportedPrimaryOpcode(1)".to_string())
        );
    }

    #[test]
    fn ppc_unimpl_histogram_formatter_sorts_by_count_then_key() {
        let mut histogram = HashMap::new();
        merge_ppc_unimpl_histogram(&mut histogram, "import #7 InterfaceLib:Foo".to_string());
        merge_ppc_unimpl_histogram(&mut histogram, "import #7 InterfaceLib:Foo".to_string());
        merge_ppc_unimpl_histogram(&mut histogram, "instruction pc=$01000000 Bar".to_string());

        assert_eq!(
            format_ppc_unimpl_histogram(&histogram, 2),
            "[PPC-UNIMPL-HIST] top 2 of 2 unsupported stops (3 total)\n\
             [PPC-UNIMPL-HIST]            2  import #7 InterfaceLib:Foo\n\
             [PPC-UNIMPL-HIST]            1  instruction pc=$01000000 Bar\n"
        );
    }

    fn halted_ppc_app_with_sound(sound: PpcSoundState) -> LoadedApp {
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_CODE_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
        memory.add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        let mut cpu = PpcCpu::new();
        cpu.pc = PPC_CODE_BASE;
        cpu.lr = PPC_HALT_PC;
        cpu.gpr[1] = PPC_STACK_TOP - 64;

        LoadedApp::from_ppc(PpcLoadedApp {
            cpu,
            memory,
            entry_pc: PPC_CODE_BASE,
            rtoc: 0,
            stack_base: PPC_STACK_BASE,
            stack_size: PPC_STACK_SIZE,
            stack_pointer: PPC_STACK_TOP - 64,
            tick_state: SharedProcessTickState::default(),
            clock_cycles_per_tick: 1,
            clock_cycle_phase: 0,
            native_exception_handler: 0,
            native_exception_stack: Vec::new(),
            stdc_qsort_stack: Vec::new(),
            dialog_callback_stack: Vec::new(),
            apple_events: Default::default(),
            cfm: Some(crate::cfm::CfmState::default()),
            controls: Default::default(),
            screen_clut: SharedProcessValue::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
            display_gamma: SharedProcessDisplayGamma::default(),
            process_quickdraw_port_state_attached: false,
            color_manager_clut: SharedProcessValue::from_value(
                TrapDispatcher::standard_mac_8bpp_clut(),
            ),
            aliases: Vec::new(),
            gworlds: Vec::new(),
            gworld_pixel_states: crate::process_context::SharedProcessQuickDrawPixelStates::default(
            ),
            q3_objects: Vec::new(),
            q3_object_refs: Vec::new(),
            next_q3_object: 0,
            q3_error_state: Default::default(),
            q3_lifecycle: Default::default(),
            q3_memory_storages: Vec::new(),
            q3_files: Vec::new(),
            q3_group_memberships: Vec::new(),
            q3_file_groups: Vec::new(),
            q3_views: Vec::new(),
            q3_submissions: Vec::new(),
            q3_view_transforms: Vec::new(),
            q3_submission_transforms: Vec::new(),
            q3_view_materials: Vec::new(),
            q3_submission_materials: Vec::new(),
            q3_submission_lights: Vec::new(),
            q3_view_state_stack: Vec::new(),
            q3_completed_frames: Vec::new(),
            q3_retained_frames: Vec::new(),
            q3_state_only_completed_frame_batches: Vec::new(),
            q3_fog_styles: Vec::new(),
            q3_attributes: Vec::new(),
            q3_shader_uv_transforms: Vec::new(),
            q3_shader_boundaries: Vec::new(),
            q3_mipmap_textures: Vec::new(),
            q3_texture_shaders: Vec::new(),
            q3_renderer_preferences: Vec::new(),
            q3_draw_contexts: Vec::new(),
            q3_trimeshes: Vec::new(),
            q3_styles: Vec::new(),
            q3_cameras: Vec::new(),
            q3_lights: Vec::new(),
            input_sprocket: Default::default(),
            input_sprocket_virtual_elements: Vec::new(),
            toolbox_startup: Default::default(),
            quicktime: Default::default(),
            sound,
            timer_tasks: Default::default(),
            vbl_tasks: Default::default(),
            callback_scheduling: Default::default(),
            process_file_system: ppc_initial_process_file_system(),
            current_gworld: SharedProcessValue::from_value(PPC_MAIN_GWORLD),
            current_gdevice: SharedProcessValue::from_value(PPC_MAIN_GDEVICE),
            quickdraw_op_colors: SharedProcessValue::default(),
            quickdraw_hilite_colors: SharedProcessValue::default(),
            quickdraw_fore_color: PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            },
            quickdraw_fore_indices: Default::default(),
            quickdraw_back_color: PpcRgbColor {
                red: 0xffff,
                green: 0xffff,
                blue: 0xffff,
            },
            quickdraw_pen_h: 0,
            quickdraw_pen_v: 0,
            quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
            quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
            cursor_state: crate::process_context::SharedProcessCursorState::default(),
            param_text: Default::default(),
            scrap: Default::default(),
            list_manager: Default::default(),
            halt_pc: PPC_HALT_PC,
            import_trap_base: PPC_IMPORT_TRAP_BASE,
            import_count: 0,
            imports: Vec::new(),
            section_bases: Vec::new(),
            input: PpcInputSnapshot::default(),
            process_input: Default::default(),
            event_queue: Default::default(),
            window_list: Default::default(),
            process_memory_manager: PpcProcessMemoryManager::with_heap(
                PPC_HEAP_BASE,
                PPC_STACK_BASE,
            ),
            draw_sprocket: PpcDrawSprocketState::default(),
        })
    }

    fn queue_ppc_sound_completion(sound: &mut PpcSoundState, channel: u32, completion: u32) {
        sound.file_playbacks.push(PpcSoundFilePlaybackRecord {
            channel,
            ref_num: 0,
            resource_id: 0,
            buffer_size: 0,
            buffer: 0,
            selection: 0,
            completion,
            completion_command: None,
            async_play: true,
            aiff: None,
            decoded_aiff: None,
        });
        sound
            .manager
            .queue_sound_callback(PendingSoundCallback::FileCompletion {
                architecture: CallbackTaskArchitecture::PowerPc,
                callback_addr: completion,
                chan_ptr: channel,
            });
    }

    #[test]
    fn ppc_initialization_attaches_both_cpu_adapters_to_one_guest_call_stack() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        runner.dispatcher.guest_calls.begin_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000,
        );
        let ppc_app = runner.native.application_mut().expect("PPC app");
        assert_eq!(ppc_app.toolbox_startup.execution.calls().len(), 1);
        assert!(ppc_app.toolbox_startup.execution.calls().complete_m68k(0x2002, 0x3000));
        assert!(runner.dispatcher.guest_calls.is_empty());
    }

    #[test]
    fn ppc_initialization_attaches_both_cpu_adapters_to_one_sound_manager() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        {
            let ppc_app = runner.native.application_mut().expect("PPC app");
            assert!(ppc_app
                .sound
                .manager
                .ptr_eq(&runner.dispatcher.sound_manager));
            ppc_app.sound.manager.register_channel(
                0x0050_1000,
                false,
                0x0012_3456,
                CallbackTaskArchitecture::M68k,
            );
            ppc_app.sound.manager.set_default_output_volume(0x0000_8000);
        }

        let classic_callback = runner
            .dispatcher
            .sound_manager
            .with_channel_mut(0x0050_1000, |channel| {
                channel.set_volume(0x0000_4000);
                channel.callback_addr
            })
            .expect("native channel visible to classic adapter");
        assert_eq!(classic_callback, 0x0012_3456);
        runner
            .dispatcher
            .sound_manager
            .set_sys_beep_volume(0x0000_2000);

        let ppc_app = runner.native.application().expect("PPC app");
        assert_eq!(ppc_app.sound.manager.default_output_volume(), 0x0000_8000);
        assert_eq!(ppc_app.sound.manager.sys_beep_volume(), 0x0000_2000);
        assert!(ppc_app
            .sound
            .manager
            .channels
            .iter()
            .any(|channel| channel.guest_ptr == 0x0050_1000));
    }

    #[test]
    fn ppc_initialization_shares_current_resource_file_and_detaches_clones() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.dispatcher.set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([
                (5, ResourceFileMap::default()),
                (9, ResourceFileMap::default()),
            ]),
            names: HashMap::new(),
            search_order: vec![5, 9],
            current_file: 5,
        });
        runner.init_app(&app);

        assert_eq!(runner.dispatcher.current_resource_refnum(), 5);
        assert_eq!(
            runner
                .native
                .application()
                .expect("PPC app")
                .current_resource_refnum(),
            5
        );

        runner
            .dispatcher
            .set_current_resource_refnum(&mut runner.bus, 9);
        assert_eq!(
            runner
                .native
                .application()
                .expect("PPC app")
                .current_resource_refnum(),
            9
        );
        assert_eq!(runner.bus.read_word(0x0A5A), 9);

        runner
            .native
            .application_mut()
            .expect("PPC app")
            .set_current_resource_refnum(5);
        assert_eq!(runner.dispatcher.current_resource_refnum(), 5);
        assert_eq!(runner.bus.read_word(0x0A5A), 5);

        let mut detached = runner.native.application().expect("PPC app").clone();
        runner
            .native
            .application_mut()
            .expect("PPC app")
            .set_current_resource_refnum(9);
        assert_eq!(detached.current_resource_refnum(), 5);
        detached.set_current_resource_refnum(7);
        assert_eq!(runner.dispatcher.current_resource_refnum(), 9);
        assert_eq!(runner.bus.read_word(0x0A5A), 9);

        runner
            .native
            .application_mut()
            .expect("PPC app")
            .set_current_resource_refnum(5);
        assert!(runner
            .dispatcher
            .close_resource_file_refnum(&mut runner.bus, 5));
        assert_eq!(runner.dispatcher.current_resource_refnum(), 9);
        assert_eq!(runner.bus.read_word(0x0A5A), 9);
    }

    #[test]
    fn universal_proc_preserves_native_isa_for_protected_transition_vectors() {
        use crate::loader::ppc::tests::synthetic_pef_with_import;

        const VECTOR: u32 = 0x0180_0000;
        const ENTRY: u32 = VECTOR + 8;
        const RESULT: u32 = 0x1234_5678;
        for installed in [false, true] {
            for protected in [false, true] {
                let mut native =
                    load_pef_application(&synthetic_pef_with_import(b"CallUniversalProc")).unwrap();
                let words = [ENTRY, PPC_DATA_BASE, 0x3c60_1234, 0x6063_5678, 0x4e80_0020];
                let bytes = words.into_iter().flat_map(u32::to_be_bytes).collect();
                if protected {
                    native
                        .memory
                        .publish_system_code(GuestIsa::PowerPc, VECTOR, bytes)
                        .unwrap();
                } else {
                    native.memory.add_readonly_region(VECTOR, bytes);
                }
                native.cpu.gpr[3] = VECTOR;
                native.cpu.gpr[4] = 0x30; // Pascal, no arguments, long result.
                if installed {
                    let mut runner =
                        FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
                    runner.init_app(&LoadedApp::from_ppc(native));
                    let (_, running) = runner.run_steps(128, None);
                    assert!(!running, "installed protected={protected}");
                    assert_eq!(runner.native.application_mut().unwrap().cpu.gpr[3], RESULT);
                } else {
                    let probe = native.run_with_hle_imports(128);
                    assert_eq!(probe.unsupported_import_index, None);
                    assert_eq!(native.cpu.pc, native.halt_pc);
                    assert_eq!(
                        native.cpu.gpr[3], RESULT,
                        "standalone protected={protected}"
                    );
                }
            }
        }
    }

    #[test]
    fn native_system_code_survives_large_process_ram_attachment() {
        use crate::loader::ppc::tests::synthetic_pef_with_import;

        let mut native = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
        let pools = [
            PPC_IMPORT_TVECTOR_BASE,
            PPC_IMPORT_TRAP_BASE,
            PPC_CFM_MAIN_STUB_BASE,
        ];
        let words = pools.map(|base| native.memory.read_u32_be(base).unwrap());
        // Call the imported transition vector, as compiled PEF glue does.
        // Inside Macintosh: PowerPC System Software (1994), pp. 1-27--1-28.
        let code = [
            0x3d80_0000 | (PPC_IMPORT_TVECTOR_BASE >> 16), // lis r12, vector@h
            0x618c_0000 | (PPC_IMPORT_TVECTOR_BASE & 0xffff), // ori r12,r12,vector@l
            0x800c_0000,                                   // lwz r0,0(r12)
            0x804c_0004,                                   // lwz r2,4(r12)
            0x7c09_03a6,                                   // mtctr r0
            0x4e80_0420,                                   // bctr
        ];
        const ENTRY: u32 = 0x0180_0000;
        native
            .memory
            .add_readonly_region(ENTRY, code.into_iter().flat_map(u32::to_be_bytes).collect());
        native.entry_pc = ENTRY;
        native.cpu.pc = ENTRY;
        native.cpu.lr = native.halt_pc;
        let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
        let (reservation_base, _) = runner.bus.synthetic_reservation_range().unwrap();
        runner.set_launch_state(17, 1, 0);
        runner.init_app(&LoadedApp::from_ppc(native));
        let native = runner.native.application_mut().unwrap();
        for (base, word) in pools.into_iter().zip(words) {
            assert_eq!(native.memory.read_u32_be(base), Some(word));
            assert!(native.memory.shared_view().is_shared_readonly_range(base, 1));
            assert_eq!(native.memory.write_u32_be(base, 0), None);
        }
        assert!(native
            .memory
            .shared_view()
            .is_shared_readonly_range(reservation_base, 1));
        assert_eq!(native.memory.write_u8(reservation_base, 0xff), None);
        let (steps, running) = runner.run_steps(64, None);
        assert!(steps >= 6);
        assert!(!running);
        let native = runner.native.application_mut().unwrap();
        assert_eq!(native.cpu.pc, native.halt_pc);
        assert_eq!(native.cpu.gpr[3], 17);
    }

    #[test]
    fn ppc_initialization_attaches_both_cpu_adapters_to_one_native_menu_selection() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        assert!(runner
            .dispatcher
            .pending_native_menu_selection
            .stage((128, 2)));
        let ppc_app = runner.native.application_mut().expect("PPC app");
        assert_eq!(
            ppc_app
                .toolbox_startup
                .pending_native_menu_selection
                .snapshot(),
            Some((128, 2))
        );
        assert_eq!(
            ppc_app.toolbox_startup.pending_native_menu_selection.take(),
            Some((128, 2))
        );
        assert!(runner.dispatcher.pending_native_menu_selection.is_none());

        assert!(ppc_app
            .toolbox_startup
            .pending_native_menu_selection
            .stage((129, 3)));
        assert_eq!(
            runner.dispatcher.pending_native_menu_selection.take(),
            Some((129, 3))
        );
        assert!(ppc_app
            .toolbox_startup
            .pending_native_menu_selection
            .is_none());
    }

    #[test]
    fn native_menu_select_observes_68k_disable_item_after_mdef_returns() {
        use crate::loader::ppc::tests::cross_abi_menu_select_fixture;
        use crate::memory::globals::addr;

        const CALLBACK_VALUE: u32 = 0x68c0_ab1e;
        const ROOT_MENU_ID: i16 = 140;
        const TARGET_ITEM: i16 = 2;

        let fixture = cross_abi_menu_select_fixture();
        let root_menu = fixture.root_menu;
        let root_record = fixture.root_record;
        let callback_marker = fixture.callback_marker;
        let title_h = fixture.title_h;
        let app = LoadedApp::from_ppc(fixture.app);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        // This fixture pre-renders classic menu pixels before attaching to the runner.
        runner.set_ui_theme(UiThemeId::ClassicSystem7);
        runner.init_app(&app);

        let framebuffer_before = {
            let native = runner.native.application_mut().expect("native app");
            let front = native.current_front_buffer().expect("front buffer");
            let mut framebuffer = Vec::with_capacity((front.row_bytes * front.height) as usize);
            let mut row = vec![0; front.row_bytes as usize];
            for y in 0..front.height {
                native
                    .read_front_buffer_row(front, y, &mut row)
                    .expect("front-buffer row");
                framebuffer.extend_from_slice(&row);
            }
            framebuffer
        };
        assert_ne!(
            runner
                .native
                .application_mut()
                .unwrap()
                .memory
                .read_u32_be(root_record + 10)
                .unwrap()
                & (1 << TARGET_ITEM),
            0,
            "the target row must begin enabled"
        );

        runner.push_canonical_mouse_down(10, title_h);
        let root_rect = (0..16)
            .find_map(|_| {
                let (_, running) = runner.run_steps(512, None);
                assert!(
                    running,
                    "native MenuSelect halted before opening the root menu"
                );
                runner
                    .process_context
                    .menu_tracking()
                    .filter(|tracking| tracking.menu_handle == root_menu)
                    .map(|tracking| tracking.dropdown_rect())
            })
            .expect("native MenuSelect should retain the canonical root handle");

        runner
            .dispatcher
            .set_mouse_position(root_rect.0 + 8, root_rect.1 + 16);
        runner.sync_mouse_position_lowmem();
        let callback_completed = (0..32).any(|_| {
            let (_, running) = runner.run_steps(512, None);
            assert!(
                running,
                "native MenuSelect halted during the 68k MDEF callback"
            );
            let callback_value = runner
                .native
                .application_mut()
                .unwrap()
                .memory
                .read_u32_be(callback_marker);
            callback_value == Some(CALLBACK_VALUE) && runner.dispatcher.guest_calls.depth() == 0
        });
        assert!(
            callback_completed,
            "the real 68k MDEF callback did not return"
        );
        let tracking = runner
            .process_context
            .menu_tracking()
            .expect("native interaction should remain retained");
        assert_eq!(tracking.menu_handle, root_menu);
        assert_eq!(
            runner
                .native
                .application_mut()
                .unwrap()
                .memory
                .read_u32_be(root_menu),
            Some(root_record),
            "a fixed-size 68k mutation must preserve the native handle allocation"
        );
        assert_eq!(
            runner
                .native
                .application_mut()
                .unwrap()
                .memory
                .read_u32_be(root_record + 10)
                .unwrap()
                & (1 << TARGET_ITEM),
            0,
            "the 68k DisableItem trap must mutate the live native MenuInfo"
        );

        let target_v = root_rect.0 + 24;
        let target_h = root_rect.1 + 16;
        runner.dispatcher.set_mouse_position(target_v, target_h);
        runner.sync_mouse_position_lowmem();
        let raw_choice = (u32::from(ROOT_MENU_ID as u16) << 16) | u32::from(TARGET_ITEM as u16);
        let native_observed_disabled_row = (0..16).any(|_| {
            let (_, running) = runner.run_steps(512, None);
            assert!(running, "native MenuSelect halted before the mouse release");
            runner.bus.read_long(addr::MENU_DISABLE) == raw_choice
                && runner
                    .process_context
                    .menu_tracking()
                    .is_some_and(|tracking| {
                        tracking.menu_handle == root_menu && tracking.highlighted_item == 0
                    })
        });
        assert!(
            native_observed_disabled_row,
            "native tracking did not reread the live disabled enableFlags"
        );

        runner.push_canonical_mouse_up(target_v, target_h);
        for _ in 0..16 {
            let (_, running) = runner.run_steps(512, None);
            if !running {
                break;
            }
        }
        assert!(runner.is_halted());
        assert!(runner.process_context.menu_tracking().is_none());
        assert!(runner.dispatcher.guest_calls.is_empty());

        let native = runner
            .native
            .application_mut()
            .expect("native app retained");
        assert_eq!(native.cpu.gpr[3], 0, "disabled rows cannot be selected");
        let framebuffer_after = {
            let front = native.current_front_buffer().expect("front buffer");
            let mut framebuffer = Vec::with_capacity((front.row_bytes * front.height) as usize);
            let mut row = vec![0; front.row_bytes as usize];
            for y in 0..front.height {
                native
                    .read_front_buffer_row(front, y, &mut row)
                    .expect("front-buffer row");
                framebuffer.extend_from_slice(&row);
            }
            framebuffer
        };
        assert_eq!(
            framebuffer_after, framebuffer_before,
            "the retained interaction must restore its saved presentation"
        );

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::MenuChoice;
        let probe = runner
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| {
                native.run_with_process_services(64, false, false, memory_manager, cfm)
            });
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(native.cpu.gpr[3], raw_choice);
    }

    #[test]
    fn native_menu_select_observes_growing_68k_append_menu_after_mdef_returns() {
        use crate::loader::ppc::tests::cross_abi_menu_select_fixture;

        const APPEND_STRING: u32 = crate::loader::ppc::PPC_DATA_BASE + 0x7300;
        const CALLBACK_VALUE: u32 = 0x68c0_ab1e;
        const ROOT_MENU_ID: i16 = 140;
        const TARGET_ITEM: i16 = 2;
        const APPENDED_TEXT: &[u8] =
            b"Cross-ABI growth must remain visible through the original native MenuHandle";

        let mut fixture = cross_abi_menu_select_fixture();
        let root_menu = fixture.root_menu;
        let original_record = fixture.root_record;
        let original_handle_record = fixture
            .app
            .handles()
            .iter()
            .copied()
            .find(|record| record.handle == root_menu)
            .expect("native root-menu allocation");
        let callback_marker = fixture.callback_marker;
        let title_h = fixture.title_h;

        let mut append_string = Vec::with_capacity(APPENDED_TEXT.len() + 1);
        append_string.push(APPENDED_TEXT.len() as u8);
        append_string.extend_from_slice(APPENDED_TEXT);
        fixture.app.memory.add_region(APPEND_STRING, append_string);

        // The real 68k MDEF grows the native MenuInfo with AppendMenu. A
        // relocatable block can move during SetHandleSize, but its master
        // pointer and Handle identity remain authoritative. Inside Macintosh:
        // Memory (1992), pp. 1-16--1-17 and 2-40--2-41.
        let mut mdef = Vec::new();
        mdef.extend_from_slice(&0x4ab9u16.to_be_bytes()); // TST.L marker
        mdef.extend_from_slice(&callback_marker.to_be_bytes());
        mdef.extend_from_slice(&0x660eu16.to_be_bytes()); // BNE.S after AppendMenu
        mdef.extend_from_slice(&0x2f3cu16.to_be_bytes()); // MOVE.L #rootMenu,-(SP)
        mdef.extend_from_slice(&root_menu.to_be_bytes());
        mdef.extend_from_slice(&0x2f3cu16.to_be_bytes()); // MOVE.L #appendString,-(SP)
        mdef.extend_from_slice(&APPEND_STRING.to_be_bytes());
        mdef.extend_from_slice(&0xa933u16.to_be_bytes()); // AppendMenu
        mdef.extend_from_slice(&0x23fcu16.to_be_bytes()); // MOVE.L #value,marker
        mdef.extend_from_slice(&CALLBACK_VALUE.to_be_bytes());
        mdef.extend_from_slice(&callback_marker.to_be_bytes());
        mdef.extend_from_slice(&0x4e74u16.to_be_bytes()); // RTD #18
        mdef.extend_from_slice(&0x0012u16.to_be_bytes());
        fixture.app.memory.add_region(fixture.mdef_entry, mdef);

        let app = LoadedApp::from_ppc(fixture.app);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        runner.push_canonical_mouse_down(10, title_h);
        let root_rect = (0..16)
            .find_map(|_| {
                runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
                let (_, running) = runner.run_steps(512, None);
                assert!(
                    running,
                    "native MenuSelect halted before opening the root menu"
                );
                runner
                    .process_context
                    .menu_tracking()
                    .filter(|tracking| tracking.menu_handle == root_menu)
                    .map(|tracking| tracking.dropdown_rect())
            })
            .expect("native MenuSelect should retain the canonical root handle");

        runner
            .dispatcher
            .set_mouse_position(root_rect.0 + 8, root_rect.1 + 16);
        runner.sync_mouse_position_lowmem();
        let callback_completed = (0..32).any(|_| {
            runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
            let (_, running) = runner.run_steps(512, None);
            assert!(
                running,
                "native MenuSelect halted during the growing 68k callback"
            );
            runner
                .native
                .application_mut()
                .unwrap()
                .memory
                .read_u32_be(callback_marker)
                == Some(CALLBACK_VALUE)
                && runner.dispatcher.guest_calls.depth() == 0
        });
        assert!(
            callback_completed,
            "the growing real 68k MDEF callback did not return"
        );

        let native = runner
            .native
            .application_mut()
            .expect("native app retained");
        let relocated_record = native
            .memory
            .read_u32_be(root_menu)
            .expect("live native root-menu master pointer");
        let handle_record = native
            .handles()
            .iter()
            .copied()
            .find(|record| record.handle == root_menu)
            .expect("updated native root-menu allocation");
        assert_eq!(handle_record.ptr, relocated_record);
        assert_eq!(handle_record.handle, root_menu);
        assert_eq!(handle_record.size, handle_record.capacity);
        assert!(handle_record.size > original_handle_record.size);
        assert_ne!(
            relocated_record, original_record,
            "the fixture must force relocation"
        );
        assert_eq!(
            runner.process_context.handle_for_ptr(relocated_record),
            Some(root_menu),
            "the process Memory Manager must publish the relocated native handle"
        );
        let mut menu_bytes = vec![0; handle_record.size as usize];
        native
            .memory
            .read_bytes_into(relocated_record, &mut menu_bytes)
            .expect("complete relocated MenuInfo bytes");
        let items = crate::menu_manager::MenuItems::decode(&menu_bytes)
            .expect("relocated native MenuInfo remains decodable");
        assert!(
            items.items.iter().any(|item| item.text == APPENDED_TEXT),
            "native decoding must observe the item appended by the 68k callback"
        );
        assert_eq!(native.last_mem_error(), 0);
        assert_eq!(
            runner.bus.read_word(crate::memory::globals::addr::MEM_ERR),
            0
        );

        let target_v = root_rect.0 + 24;
        let target_h = root_rect.1 + 16;
        runner.dispatcher.set_mouse_position(target_v, target_h);
        runner.sync_mouse_position_lowmem();
        let target_observed = (0..16).any(|_| {
            runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
            let (_, running) = runner.run_steps(512, None);
            assert!(running, "native MenuSelect halted before the mouse release");
            runner
                .process_context
                .menu_tracking()
                .is_some_and(|tracking| {
                    tracking.menu_handle == root_menu && tracking.highlighted_item == TARGET_ITEM
                })
        });
        assert!(
            target_observed,
            "native tracking did not reach the live regular row"
        );
        runner.push_canonical_mouse_up(target_v, target_h);
        for _ in 0..128 {
            runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
            let (_, running) = runner.run_steps(512, None);
            if !running {
                break;
            }
        }
        assert!(runner.is_halted());
        assert!(runner.process_context.menu_tracking().is_none());
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert_eq!(
            runner.native.application().unwrap().cpu.gpr[3],
            (u32::from(ROOT_MENU_ID as u16) << 16) | u32::from(TARGET_ITEM as u16),
            "native MenuSelect must continue and return the live regular row"
        );
    }

    #[test]
    fn parked_native_call_executes_68k_code_and_nested_traps_in_shared_memory() {
        const M68K_ENTRY: u32 = 0x0301_0000;
        const RESULT: u32 = 0x0302_0000;
        const STACK_BASE: u32 = 0x0303_0000;
        const INITIAL_SP: u32 = STACK_BASE + 0x80;
        const RETURN_PC: u32 = 0x0304_0000;

        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_launch_state(41, 1, 0);
        runner.init_app(&app);

        let ppc_app = runner.native.application_mut().expect("PPC app");
        ppc_app.memory.add_region(
            M68K_ENTRY,
            vec![
                0x59, 0x8f, // SUBQ.L #4,SP: TickCount result slot
                0xa9, 0x75, // TickCount
                0x20, 0x1f, // MOVE.L (SP)+,D0
                0x23, 0xc0, 0x03, 0x02, 0x00, 0x00, // MOVE.L D0,RESULT
                0x4e, 0x75, // RTS
            ],
        );
        ppc_app.memory.add_region(RESULT, vec![0; 4]);
        ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
        ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Data(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
        let (first_steps, first_running) = runner.run_steps(2, None);
        assert_eq!(first_steps, 2);
        assert!(first_running);
        assert_eq!(runner.bus.read_long(RESULT), 0);
        assert!(!runner.dispatcher.guest_calls.is_empty());

        let (steps, running) = runner.run_steps(64, None);

        assert_eq!(steps, 64);
        assert!(running);
        assert!(!runner.is_halted());
        assert!(runner.dispatcher.guest_calls.is_empty());
        let ppc_app = runner.native.application_mut().expect("PPC app retained");
        assert_eq!(ppc_app.memory.read_u32_be(RESULT), Some(41));
        assert_eq!(ppc_app.cpu.gpr[3], 41);
        assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
        assert_eq!(ppc_app.cpu.lr, PPC_CODE_BASE);
        assert_eq!(runner.bus.read_long(RESULT), 41);
    }

    #[test]
    #[cfg(feature = "debug")]
    fn debugger_controls_a_parked_native_to_68k_callback_and_preserves_its_return() {
        use crate::debug::{
            handle_debug_request, BreakpointSpec, ContextSelector, DebugExecutionState,
            DebugReply, DebugRequest, M68K_CONTEXT,
        };

        const M68K_ENTRY: u32 = 0x0301_4000;
        const STACK_BASE: u32 = 0x0303_4000;
        const INITIAL_SP: u32 = STACK_BASE + 0x80;
        const RETURN_PC: u32 = 0x0304_4000;

        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let ppc_app = runner.native.application_mut().expect("PPC app");
        ppc_app.memory.add_region(
            M68K_ENTRY,
            vec![
                0x4e, 0x71, // NOP
                0x4e, 0x71, // NOP
                0x4e, 0x75, // RTS
            ],
        );
        ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
        ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Data(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        handle_debug_request(
            &mut runner,
            DebugRequest::Step {
                context: ContextSelector::Active,
            },
        )
        .unwrap();
        let (stepped, running) = runner.run_steps(64, None);
        assert_eq!(stepped, 1);
        assert!(running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), M68K_ENTRY + 2);
        assert!(runner.debug.is_paused());
        assert!(!runner.dispatcher.guest_calls.is_empty());

        let DebugReply::Breakpoint(callback_breakpoint) = handle_debug_request(
            &mut runner,
            DebugRequest::SetBreakpoint {
                spec: BreakpointSpec::program_counter(u64::from(M68K_ENTRY + 2)),
            },
        )
        .unwrap()
        else {
            panic!("expected callback breakpoint");
        };
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        let (break_steps, _) = runner.run_steps(64, None);
        assert_eq!(break_steps, 0);
        assert!(matches!(
            runner.debug.execution_state(),
            DebugExecutionState::Paused { .. }
        ));
        handle_debug_request(
            &mut runner,
            DebugRequest::RemoveBreakpoint {
                id: callback_breakpoint.id,
            },
        )
        .unwrap();

        let mut return_spec = BreakpointSpec::program_counter(u64::from(RETURN_PC));
        return_spec.context = Some(M68K_CONTEXT);
        let DebugReply::Breakpoint(return_breakpoint) = handle_debug_request(
            &mut runner,
            DebugRequest::SetBreakpoint { spec: return_spec },
        )
        .unwrap()
        else {
            panic!("expected return-sentinel breakpoint");
        };
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        let (return_steps, _) = runner.run_steps(64, None);
        assert_eq!(return_steps, 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), RETURN_PC);
        assert!(runner.debug.is_paused());
        assert!(
            !runner.dispatcher.guest_calls.is_empty(),
            "the user breakpoint must win before the internal return retires"
        );

        handle_debug_request(
            &mut runner,
            DebugRequest::RemoveBreakpoint {
                id: return_breakpoint.id,
            },
        )
        .unwrap();
        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        let (_, running) = runner.run_steps(64, None);
        assert!(running);
        assert!(runner.dispatcher.guest_calls.is_empty());
        let DebugReply::Notifications(notifications) = handle_debug_request(
            &mut runner,
            DebugRequest::Notifications { after: 0 },
        )
        .unwrap()
        else {
            panic!("expected debugger notifications");
        };
        assert!(notifications.iter().any(|notification| matches!(
            notification.payload,
            crate::debug::NotificationPayload::ContextChanged {
                active: Some(crate::debug::PPC_CONTEXT)
            }
        )));
    }

    #[test]
    fn parked_powerpc_to_68k_call_obeys_guest_vector_10() {
        const M68K_ENTRY: u32 = 0x0301_1000;
        const HANDLER: u32 = 0x0301_1100;
        const STACK_BASE: u32 = 0x0303_1000;
        const INITIAL_SP: u32 = STACK_BASE + 0x80;
        const RETURN_PC: u32 = 0x0304_1000;
        const MARKER: u32 = 0xA10E_6040;

        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        let ppc_app = runner.native.application_mut().expect("PPC app");
        ppc_app.memory.add_region(
            M68K_ENTRY,
            vec![
                0xA9, 0x75, // TickCount: must enter the replacement vector
                0x4E, 0x75, // RTS
            ],
        );
        ppc_app.memory.add_region(
            HANDLER,
            vec![
                0x2C, 0x3C, 0xA1, 0x0E, 0x60, 0x40, // MOVE.L #MARKER,D6
                0x54, 0xAF, 0x00, 0x02, // ADDQ.L #2,2(SP)
                0x4E, 0x73, // RTE
            ],
        );
        ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
        ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Data(6)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

        // Interapplication Communication (1993), p. 1-87: an A-line causes
        // the processor to fetch vector 10 from `$28` and jump to it. The
        // parked 68k adapter must preserve that rule while PPC owns the app.
        runner.bus.write_long(0x28, HANDLER);
        let (_steps, running) = runner.run_steps(64, None);

        assert!(running);
        assert!(runner.dispatcher.guest_calls.is_empty());
        let ppc_app = runner.native.application().expect("PPC app retained");
        assert_eq!(ppc_app.cpu.gpr[3], MARKER);
        assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    }

    #[test]
    fn parked_native_special_case_executes_68k_and_writes_native_outputs() {
        use crate::guest_call::{M68kResultSource, PowerPcArguments};
        use crate::mixed_mode::special_case;

        const M68K_ENTRY: u32 = 0x0305_0000;
        const OUTPUTS: u32 = 0x0305_1000;
        const STACK_BASE: u32 = 0x0305_2000;
        const INITIAL_SP: u32 = STACK_BASE + 0x80;
        const RETURN_PC: u32 = 0x0305_3000;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.memory.add_region(
            M68K_ENTRY,
            [
                0x203c, 0x0001, 0x1111, // MOVE.L #$00011111,D0
                0x323c, 0x2222, // MOVE.W #$2222,D1
                0x343c, 0x0033, // MOVE.W #$0033,D2
                0x4e75, // RTS
            ]
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(OUTPUTS, vec![0; 8]);
        ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
        ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
        let arguments =
            PowerPcArguments::from_slice(&[0, 0, 0, 0, 0, 0, OUTPUTS, OUTPUTS + 2, OUTPUTS + 4])
                .unwrap();
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(M68kResultSource::SpecialCase {
                selector: u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
                arguments,
                stack_result: None,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Mask(0xff),
        ));

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let (steps, running) = runner.run_steps(64, None);

        assert!(steps > 0);
        assert!(running);
        assert!(!runner.is_halted());
        assert!(runner.dispatcher.guest_calls.is_empty());
        let ppc_app = runner.native.application_mut().expect("PPC app retained");
        assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
        assert_eq!(ppc_app.cpu.gpr[3], 1);
        assert_eq!(ppc_app.memory.read_u16_be(OUTPUTS), Some(0x1111));
        assert_eq!(ppc_app.memory.read_u16_be(OUTPUTS + 2), Some(0x2222));
        assert_eq!(ppc_app.memory.read_u8(OUTPUTS + 4), Some(0x33));
    }

    #[test]
    #[cfg(feature = "debug")]
    fn parked_native_call_can_enter_powerpc_through_a_68k_routine_descriptor() {
        use crate::guest_procedure::{
            ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
            ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        };
        use crate::mixed_mode::proc_info;

        const DESCRIPTOR: u32 = 0x0301_0000;
        const TVECTOR: u32 = 0x0301_0100;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        const CALLBACK_RTOC: u32 = 0x0301_0200;
        const M68K_STACK: u32 = 0x0302_0000;
        const INITIAL_SP: u32 = M68K_STACK + 0x80;
        const RETURN_PC: u32 = 0x0302_0100;
        const ARGUMENT: u32 = 0x10;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        ppc_app.memory.add_region(
            CALLBACK,
            [
                0x3863_0007u32, // addi r3,r3,7
                0x4e80_0020,    // blr
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        ppc_app.memory.add_region(DESCRIPTOR, vec![0; 0x400]);
        ppc_app.memory.add_region(M68K_STACK, vec![0; 0x200]);
        let proc_info = proc_info::PASCAL_STACK_BASED
            | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
            | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
        ppc_app
            .memory
            .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
            .unwrap();
        ppc_app
            .memory
            .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        ppc_app.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
        let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        ppc_app.memory.write_u32_be(record, proc_info).unwrap();
        ppc_app
            .memory
            .write_u8(
                record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_POWERPC_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u16_be(
                record + ROUTINE_RECORD_FLAGS_OFFSET,
                ROUTINE_FLAG_USE_NATIVE_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR)
            .unwrap();
        ppc_app.memory.write_u32_be(TVECTOR, CALLBACK).unwrap();
        ppc_app
            .memory
            .write_u32_be(TVECTOR + 4, CALLBACK_RTOC)
            .unwrap();
        ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
        ppc_app
            .memory
            .write_u32_be(INITIAL_SP + 4, ARGUMENT)
            .unwrap();
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: DESCRIPTOR,
                rtoc: 0,
            },
            DESCRIPTOR,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 8,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Memory {
                address: INITIAL_SP + 8,
                size: 4,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        crate::debug::handle_debug_request(&mut runner, crate::debug::DebugRequest::Pause).unwrap();
        crate::debug::handle_debug_request(
            &mut runner,
            crate::debug::DebugRequest::Step {
                context: crate::debug::ContextSelector::Active,
            },
        )
        .unwrap();
        let native_pc_before_step = runner.native.application().unwrap().cpu.pc;
        let (m68k_steps, m68k_running) = runner.run_steps(64, None);
        assert_eq!(m68k_steps, 1);
        assert!(m68k_running);
        assert!(runner.debug.is_paused());
        assert_eq!(
            runner.native.application().unwrap().cpu.pc,
            native_pc_before_step,
            "the step may establish a native continuation but must not execute it"
        );
        assert!(
            runner.dispatcher.guest_calls.has_powerpc_from_m68k(),
            "steps={m68k_steps} running={m68k_running} halted={} frames={} active={:?} pending_ppc={:?} pc=${:08x} sp=${:08x}",
            runner.is_halted(),
            runner.dispatcher.guest_calls.len(),
            runner.dispatcher.guest_calls.active_m68k(),
            runner
                .dispatcher
                .guest_calls
                .pending_powerpc_from_m68k(),
            runner.m68k.cpu.read_reg(Register::PC),
            runner.m68k.cpu.read_reg(Register::A7),
        );

        crate::debug::handle_debug_request(&mut runner, crate::debug::DebugRequest::Resume).unwrap();
        let (powerpc_steps, running) = runner.run_steps(64, None);
        assert!(powerpc_steps > 0);
        assert!(running);
        assert!(!runner.is_halted());
        assert!(runner.dispatcher.guest_calls.is_empty());
        let ppc_app = runner.native.application_mut().expect("PPC app retained");
        assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
        assert_eq!(ppc_app.cpu.gpr[3], ARGUMENT + 7);
        assert_eq!(
            ppc_app.memory.read_u32_be(INITIAL_SP + 8),
            Some(ARGUMENT + 7)
        );
        assert_eq!(ppc_app.memory.read_u32_be(ppc_app.cpu.gpr[1]), Some(0));
    }

    struct ClassicPowerPcMdefFixture {
        runner: FixtureRunner,
        menu: u32,
        record: u32,
        marker: u32,
        entry: u32,
        stack: u32,
    }

    fn classic_powerpc_mdef_fixture() -> ClassicPowerPcMdefFixture {
        classic_powerpc_mdef_fixture_with_tick_identity(false)
    }

    fn classic_powerpc_mdef_fixture_with_tick_identity(
        already_shared_tick: bool,
    ) -> ClassicPowerPcMdefFixture {
        use crate::guest_procedure::{
            ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
            ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        };
        const MENU: u32 = 0x0030_0000;
        const RECORD: u32 = MENU + 0x100;
        const MDEF: u32 = MENU + 0x200;
        const DESCRIPTOR: u32 = MENU + 0x300;
        const TVECTOR: u32 = MENU + 0x400;
        const MARKER: u32 = MENU + 0x500;
        const ENTRY: u32 = MENU + 0x600;
        const STACK: u32 = 0x0070_0000;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        let mut native = halted_ppc_app_with_sound(PpcSoundState::default())
            .ppc
            .take()
            .unwrap();
        native
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        native.memory.add_region(
            CALLBACK,
            [
                0x8124_0000u32, // lwz r9,0(r4): live MenuHandle
                0x3940_01b0,    // li r10,432
                0xb149_0002,    // sth r10,menuWidth(r9)
                0x3940_007b,    // li r10,123
                0xb149_0004,    // sth r10,menuHeight(r9)
                0x3d20_0000 | (MARKER >> 16),
                0x6129_0000 | (MARKER & 0xffff),
                0x8149_0000, // lwz r10,0(r9)
                0x394a_0001, // addi r10,r10,1
                0x9149_0000, // stw r10,0(r9)
                0x3940_0002, // li r10,2
                0xb147_0000, // sth r10,0(r7): chosen item
                0x4e80_0020, // blr
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );
        let classic = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007f_ffc0,
            size_resource: None,
        };
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        if already_shared_tick {
            native.tick_state = runner
                .process_context
                .migrated_handles()
                .ticks
                .shared_handle();
        }
        runner.stage_ppc_companion(native);
        runner.init_app(&classic);
        runner.bus.write_long(MENU, RECORD);
        runner.bus.write_word(RECORD, 140);
        runner.bus.write_long(RECORD + 6, MDEF);
        runner.bus.write_long(RECORD + 10, u32::MAX);
        runner.bus.write_long(MDEF, DESCRIPTOR);
        runner
            .bus
            .write_word(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
        runner
            .bus
            .write_byte(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION);
        runner.bus.write_word(DESCRIPTOR + 10, 0);
        let routine = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        runner.bus.write_long(routine, 0x0000_ff80);
        runner.bus.write_byte(
            routine + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_POWERPC_ISA,
        );
        runner.bus.write_word(
            routine + ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_FLAG_USE_NATIVE_ISA,
        );
        runner
            .bus
            .write_long(routine + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR);
        runner.bus.write_long(TVECTOR, CALLBACK);
        runner.bus.write_long(TVECTOR + 4, 0);
        runner.bus.write_word(ENTRY, 0xA948); // CalcMenuSize
        runner.bus.write_word(ENTRY + 2, 0x60fe); // park after the call
        runner.bus.write_long(STACK, MENU);
        runner.m68k.cpu.write_reg(Register::PC, ENTRY);
        runner.m68k.cpu.write_reg(Register::A7, STACK);
        ClassicPowerPcMdefFixture {
            runner,
            menu: MENU,
            record: RECORD,
            marker: MARKER,
            entry: ENTRY,
            stack: STACK,
        }
    }

    #[test]
    fn nested_classic_mdef_preserves_its_wrapper_arguments_and_caller_stack() {
        let ClassicPowerPcMdefFixture { mut runner, menu, record, marker, entry, stack } = classic_powerpc_mdef_fixture();
        let inner = menu + 0x1000;
        let inner_record = inner + 0x100;
        let inner_handle = inner + 0x200;
        let outer_code = menu + 0x2000;
        let inner_code = outer_code + 0x200;
        let outer_handle = runner.bus.read_long(record + 6);
        runner.bus.write_long(outer_handle, outer_code);
        runner.bus.write_long(inner, inner_record);
        runner.bus.write_word(inner_record, 141);
        runner.bus.write_long(inner_record + 6, inner_handle);
        runner.bus.write_long(inner_record + 10, u32::MAX);
        runner.bus.write_long(inner_handle, inner_code);
        for (address, words) in [
            (outer_code, vec![
                0x206f, 12, // MOVEA.L menuRect(SP),A0
                0x30bc, 0x1122, // MOVE.W #$1122,(A0)
                0x2f08, // retain outer rectangle pointer
                0x2f3c, (inner >> 16) as u16, inner as u16,
                0xa948, // nested CalcMenuSize
                0x205f, // restore outer pointer
                0x33d0, (marker >> 16) as u16, marker as u16,
                0x4e74, 18, // RTD #18
            ]),
            (inner_code, vec![0x206f, 12, 0x30bc, 0x3344, 0x4e74, 18]),
        ] {
            for (index, word) in words.into_iter().enumerate() {
                runner.bus.write_word(address + index as u32 * 2, word);
            }
        }
        for _ in 0..8 {
            let (_, running) = runner.run_steps(128, None);
            assert!(running);
        }
        assert_eq!(runner.bus.read_word(marker), 0x1122);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
        assert!(runner.dispatcher.guest_calls.is_empty());
    }

    fn fire_menu_test_timer(runner: &mut FixtureRunner, menu: u32, marker: u32) {
        let timer = menu + 0x3000;
        for (index, word) in [0x33fc, 1, ((marker + 4) >> 16) as u16, (marker + 4) as u16, 0x4e75].into_iter().enumerate() {
            runner.bus.write_word(timer + index as u32 * 2, word);
        }
        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: timer + 0x100,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: timer,
            active: true,
            fire_at_tick: 1,
            fire_at_subtick: 1_000_000,
            last_fired_tick: None,
        });
        runner.fire_timer_tasks(1);
    }

    #[test]
    fn timer_at_classic_mdef_return_preserves_callback_code_and_stack() {
        let ClassicPowerPcMdefFixture {
            mut runner,
            menu,
            marker,
            entry,
            stack,
            ..
        } = classic_powerpc_mdef_fixture();
        let frame = stack + 4 - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION;
        let mut reached_return = false;
        for _ in 0..512 {
            let return_instruction = if runner.bus.read_word(frame + 48) == 0x4e74 {
                frame + 48
            } else {
                frame + 54
            };
            if runner.m68k.cpu.read_reg(Register::PC) == return_instruction {
                reached_return = true;
                break;
            }
            let (_, running) = runner.run_steps(1, None);
            assert!(running);
        }
        assert!(
            reached_return,
            "callback must reach its final return instruction"
        );
        fire_menu_test_timer(&mut runner, menu, marker);
        assert!(runner.active_interrupt_callback.is_some());
        for _ in 0..8 {
            let (_, running) = runner.run_steps(128, None);
            assert!(running);
        }
        assert_eq!(runner.bus.read_word(marker + 4), 1);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
        assert!(runner.active_interrupt_callback.is_none());
        assert!(runner.dispatcher.guest_calls.is_empty());
    }

    #[test]
    fn classic_calc_menu_size_executes_powerpc_mdef_and_resumes_once() {
        let ClassicPowerPcMdefFixture {
            mut runner,
            record,
            marker,
            entry,
            stack,
            ..
        } = classic_powerpc_mdef_fixture();
        for _ in 0..8 {
            let (_, running) = runner.run_steps(128, None);
            assert!(running);
        }
        assert_eq!(
            runner.bus.read_long(marker),
            1,
            "PowerPC MDEF must run exactly once"
        );
        assert_eq!(runner.bus.read_word(record + 2), 432);
        assert_eq!(runner.bus.read_word(record + 4), 123);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
        assert!(runner.dispatcher.guest_calls.is_empty());
    }

    #[test]
    fn classic_menu_select_retains_powerpc_mdef_until_mouse_release() {
        run_classic_menu_select_with_powerpc_mdef(false);
    }

    #[test]
    fn timer_after_classic_mdef_return_preserves_pending_tracking_results() {
        run_classic_menu_select_with_powerpc_mdef(true);
    }

    fn run_classic_menu_select_with_powerpc_mdef(interrupt: bool) {
        run_classic_menu_select_with_powerpc_mdef_identity(interrupt, false);
    }

    fn run_classic_menu_select_with_powerpc_mdef_identity(interrupt: bool, already_shared_tick: bool) {
        use crate::memory::globals::addr;
        let ClassicPowerPcMdefFixture {
            mut runner,
            menu,
            record,
            marker,
            entry,
            stack,
        } = classic_powerpc_mdef_fixture_with_tick_identity(already_shared_tick);
        let migrated_handles = runner.process_context.migrated_handles();
        runner.dispatcher.menu_bar_hidden = false;
        runner.bus.write_word(addr::MBAR_HEIGHT, 20);
        runner.bus.write_word(addr::MENU_FLASH, 0);
        runner.bus.write_word(record + 2, 80);
        runner.bus.write_word(record + 4, 32);
        runner.bus.write_bytes(
            record + 14,
            b"\x06Custom\x01A\x00\x00\x00\x00\x01B\x00\x00\x00\x00\x00",
        );
        runner.bus.write_word(stack, 0);
        runner.bus.write_long(stack + 2, menu);
        runner
            .dispatcher
            .dispatch_menu(true, 0x135, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap()
            .unwrap();
        runner.dispatcher.draw_menu_bar_to_fb(&mut runner.bus);
        let original_port = *runner.dispatcher.current_port;
        runner.bus.write_word(entry, 0xA93D);
        runner.bus.write_word(stack, 10);
        runner.bus.write_word(stack + 2, 16);
        runner.bus.write_long(stack + 4, 0);
        runner.m68k.cpu.write_reg(Register::A7, stack);
        runner.push_canonical_mouse_down(10, 16);
        if interrupt {
            let mut parked = false;
            for _ in 0..512 {
                if runner.m68k.cpu.read_reg(Register::PC)
                    == stack - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION + 52
                    && runner.m68k.cpu.read_reg(Register::A7)
                        == stack - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
                {
                    parked = true;
                    break;
                }
                assert!(runner.run_steps(1, None).1);
            }
            assert!(parked, "MDEF return must keep its result reservation live");
            fire_menu_test_timer(&mut runner, menu, marker);
        }

        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
        }
        let rect = runner
            .process_context
            .menu_tracking()
            .expect("classic tracking remains active")
            .dropdown_rect();
        assert!(
            runner.bus.read_long(marker) > 0,
            "PowerPC draw callback ran"
        );
        let (v, h) = (rect.0 + 24, rect.1 + 16);
        runner.dispatcher.set_mouse_position(v, h);
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
        }
        runner.push_canonical_mouse_up(v, h);
        for _ in 0..16 {
            assert!(runner.run_steps(128, None).1);
            if runner.process_context.menu_tracking().is_none()
                && runner.dispatcher.guest_calls.is_empty()
            {
                break;
            }
        }
        assert!(runner.process_context.menu_tracking().is_none());
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert!(runner
            .native
            .adapter_mut(NativeEngineRole::Companion)
            .expect("mixed callback companion retained")
            .is_constructed_from_migrated_handles(&migrated_handles));
        assert_eq!(runner.bus.read_long(stack + 4), (140 << 16) | 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
        assert_eq!(*runner.dispatcher.current_port, original_port);
        if interrupt {
            assert_eq!(runner.bus.read_word(marker + 4), 1);
        }

        let completed_result = runner.bus.read_long(stack + 4);
        let completed_sp = runner.m68k.cpu.read_reg(Register::A7);
        let completed_pc = runner.m68k.cpu.read_reg(Register::PC);
        let completed_marker = runner.bus.read_long(marker);
        assert!(runner.run_steps(16, None).1);
        assert_eq!(runner.bus.read_long(stack + 4), completed_result);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), completed_sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), completed_pc);
        assert_eq!(runner.bus.read_long(marker), completed_marker);
        assert!(runner.process_context.menu_tracking().is_none());
        assert!(runner.dispatcher.guest_calls.is_empty());
    }

    #[test]
    fn classic_menu_wait_resumes_without_refiring_new_trap_patch() {
        for auto_pop in [false, true] {
            for custom in [false, true] {
                run_menu_patch_during_tracking(auto_pop, custom, false, false);
            }
        }
    }

    #[test]
    fn native_menu_hook_runs_classic_guest_code_and_releases_ownership() {
        use crate::loader::ppc::tests::native_menu_hook_fixture;
        use crate::memory::globals::addr;
        for cancel in [false, true] {
            let (mut native, _, _) = native_menu_hook_fixture();
            native.cpu.gpr[3] = (10 << 16) | 12;
            native.memory.write_u16_be(addr::MENU_FLASH, 0).unwrap();
            let original_sp = native.cpu.gpr[1];
            let original_return = native.cpu.lr;
            let app = LoadedApp::from_ppc(native);
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.set_ui_theme(UiThemeId::ClassicSystem7);
            runner.init_app(&app);
            let code = 0x0030_8000;
            let marker = code + 0x100;
            for (index, word) in [
                0x42a7, // CLR.L -(SP): inner result
                0x2f3c,
                0x01f4,
                0x01f4, // inner MenuSelect outside the menu bar
                0xa93d,
                0x23df,
                ((marker + 4) >> 16) as u16,
                (marker + 4) as u16,
                0x52b9,
                (marker >> 16) as u16,
                marker as u16,
                0x4e75,
            ]
            .into_iter()
            .enumerate()
            {
                runner.bus.write_word(code + index as u32 * 2, word);
            }
            runner.bus.write_long(marker + 4, u32::MAX);
            runner.bus.write_long(0x0a30, code);
            runner.push_canonical_mouse_down(10, 12);
            for _ in 0..8 {
                assert!(runner.run_steps(128, None).1);
            }
            assert!(
                runner.bus.read_long(marker) > 0,
                "native MenuSelect invoked the classic hook"
            );
            assert!(runner.process_context.menu_tracking().is_some());
            assert_eq!(
                runner.bus.read_long(marker + 4),
                0,
                "nested no-hit MenuSelect returned independently"
            );
            if cancel {
                runner.push_canonical_mouse_up(500, 500);
            } else {
                runner.push_canonical_mouse_up(28, 20);
            }
            for _ in 0..32 {
                runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
                if !runner.run_steps(128, None).1 {
                    break;
                }
            }
            assert!(runner.is_halted());
            assert!(runner.process_context.menu_tracking().is_none());
            assert!(runner.dispatcher.guest_calls.is_empty());
            let native = runner.native.application_mut().unwrap();
            assert_eq!(native.cpu.pc, original_return);
            assert_eq!(native.cpu.gpr[1], original_sp);
            assert_eq!(native.cpu.gpr[3], if cancel { 0 } else { (128 << 16) | 1 });
        }
    }

    #[test]
    fn classic_menu_hook_uses_owned_stack_frame_and_restores_registers() {
        for auto_pop in [false, true] {
            for native_hook in [false, true] {
                run_menu_patch_during_tracking(auto_pop, false, true, native_hook);
            }
        }
    }

    fn run_menu_patch_during_tracking(auto_pop: bool, custom: bool, hook: bool, native_hook: bool) {
        use crate::memory::globals::addr;
        let ClassicPowerPcMdefFixture {
            mut runner,
            menu,
            record,
            marker,
            entry,
            stack,
        } = classic_powerpc_mdef_fixture();
        runner.dispatcher.menu_bar_hidden = false;
        runner.bus.write_word(addr::MBAR_HEIGHT, 20);
        runner.bus.write_word(addr::MENU_FLASH, 0);
        if !custom {
            let code = runner
                .bus
                .alloc(crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM.len() as u32);
            runner
                .bus
                .write_bytes(code, &crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM);
            let handle = runner.bus.alloc(4);
            runner.bus.write_long(handle, code);
            runner.bus.write_long(record + 6, handle);
        }
        runner.bus.write_word(record + 2, 80);
        runner.bus.write_word(record + 4, 32);
        runner.bus.write_bytes(
            record + 14,
            b"\x06Custom\x01A\x00\x00\x00\x00\x01B\x00\x00\x00\x00\x00",
        );
        runner.bus.write_word(stack, 0);
        runner.bus.write_long(stack + 2, menu);
        runner
            .dispatcher
            .dispatch_menu(true, 0x135, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap()
            .unwrap();
        runner.dispatcher.draw_menu_bar_to_fb(&mut runner.bus);
        let original_port = *runner.dispatcher.current_port;
        let hook_port = runner.bus.alloc(170);
        let original_port_image = runner.bus.read_bytes(original_port, 170).to_vec();
        runner.bus.write_bytes(hook_port, &original_port_image);
        let parameters = stack + if auto_pop { 4 } else { 0 };
        let return_pc = entry + if auto_pop { 0x100 } else { 2 };
        runner.bus.write_word(return_pc, 0x60fe);
        runner
            .bus
            .write_word(entry, if auto_pop { 0xAD3D } else { 0xA93D });
        if auto_pop {
            runner.bus.write_long(stack, return_pc);
        }
        runner.bus.write_word(parameters, 10);
        runner.bus.write_word(parameters + 2, 16);
        runner.bus.write_long(parameters + 4, 0);
        runner.m68k.cpu.write_reg(Register::A7, stack);
        let hook_marker = runner.bus.alloc(4);
        let hook_after_yield_marker = runner.bus.alloc(4);
        let cooperative_switch = hook && !native_hook && !auto_pop;
        let mut hook_yield_resume_pc = None;
        let mut worker_yield_result = None;
        if hook {
            let mut hook_words = vec![
                0x7e63, // MOVEQ #99,D7
                0x2c7c, // MOVEA.L #value,A6
                0x1234,
                0x5678,
            ];
            if !native_hook {
                hook_words.extend([
                    0x2f3c,
                    (hook_port >> 16) as u16,
                    hook_port as u16,
                    0xa873, // SetPort(hook_port)
                ]);
            }
            hook_words.extend([
                0x52b9, // ADDQ.L #1,marker
                (hook_marker >> 16) as u16,
                hook_marker as u16,
            ]);
            if cooperative_switch {
                hook_words.extend([
                    0x558f, // SUBQ.L #2,SP: Pascal result word
                    0x42a7, // CLR.L -(SP): synthetic suggested ThreadID
                    0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                    0xabf2, // ThreadDispatch
                    0x548f, // ADDQ.L #2,SP: pop Pascal result
                ]);
                hook_words.extend([
                    0x52b9, // ADDQ.L #1,after-yield marker
                    (hook_after_yield_marker >> 16) as u16,
                    hook_after_yield_marker as u16,
                ]);
            }
            hook_words.extend([
                0x5279, // ADDQ.W #1,menuWidth
                ((record + 2) >> 16) as u16,
                (record + 2) as u16,
                0x4e75,
            ]);
            let code = runner.bus.alloc((hook_words.len() * 2) as u32);
            if cooperative_switch {
                let trap_index = hook_words
                    .iter()
                    .position(|word| *word == 0xabf2)
                    .unwrap();
                hook_yield_resume_pc = Some(code + (trap_index as u32 + 1) * 2);
            }
            for (index, word) in hook_words.into_iter().enumerate() {
                runner.bus.write_word(code + index as u32 * 2, word);
            }
            runner.bus.write_long(0x0a30, code);
            if native_hook {
                use crate::guest_procedure::{
                    ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
                    ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA,
                    ROUTINE_RECORD_FLAGS_OFFSET, ROUTINE_RECORD_ISA_OFFSET,
                    ROUTINE_RECORD_POWERPC_ISA, ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                };
                let native_code = runner.bus.alloc(48);
                for (index, word) in [
                    0x3ce0_0000 | (record >> 16),
                    0x60e7_0000 | (record & 0xffff),
                    0xa147_0002,
                    0x394a_0001,
                    0xb147_0002,
                    0x3d00_0000 | (hook_marker >> 16),
                    0x6108_0000 | (hook_marker & 0xffff),
                    0x8128_0000,
                    0x3929_0001,
                    0x9128_0000,
                    0x4e80_0020,
                ]
                .into_iter()
                .enumerate()
                {
                    runner.bus.write_long(native_code + index as u32 * 4, word);
                }
                let descriptor = runner.bus.alloc(64);
                let tvector = descriptor + 48;
                let record = descriptor + ROUTINE_DESCRIPTOR_HEADER_SIZE;
                runner
                    .bus
                    .write_word(descriptor, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
                runner
                    .bus
                    .write_byte(descriptor + 2, ROUTINE_DESCRIPTOR_VERSION);
                runner.bus.write_byte(
                    record + ROUTINE_RECORD_ISA_OFFSET,
                    ROUTINE_RECORD_POWERPC_ISA,
                );
                runner.bus.write_word(
                    record + ROUTINE_RECORD_FLAGS_OFFSET,
                    ROUTINE_FLAG_USE_NATIVE_ISA,
                );
                runner
                    .bus
                    .write_long(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, tvector);
                runner.bus.write_long(tvector, native_code);
                runner.bus.write_long(tvector + 4, 0);
                runner.bus.write_long(0x0a30, descriptor);
            }

            runner.m68k.cpu.write_reg(Register::D7, 0x77777777);
            runner.m68k.cpu.write_reg(Register::A6, 0x66666666);
        }
        if hook && !native_hook {
            let hook_pointer = runner.bus.read_long(0x0a30);
            runner.bus.write_long(0x0a30, 0);
            runner.push_canonical_mouse_down(10, 16);
            for _ in 0..8 {
                assert!(runner.run_steps(128, None).1);
                if runner
                    .dispatcher
                    .menu_tracking
                    .request_menu_hook(true)
                    .is_some()
                {
                    break;
                }
            }
            let key = runner
                .dispatcher
                .menu_tracking
                .request_menu_hook(true)
                .expect("held menu requests its classic hook");
            let call_depth = runner.dispatcher.guest_calls.depth();
            runner.bus.write_long(0x0a30, hook_pointer);
            let procedure = crate::guest_procedure::resolve_guest_procedure(
                &mut runner.bus,
                hook_pointer,
                0,
                None,
                GuestIsa::M68k,
                GuestIsa::M68k,
            )
            .expect("classic hook remains resolvable");
            assert_eq!(procedure.isa, GuestIsa::M68k);
            assert_eq!(procedure.entry, hook_pointer);
            let valid_sp = runner.m68k.cpu.read_reg(Register::A7);
            let frame_start = runner.bus.alloc(114);
            let frame_len = 114;
            runner
                .bus
                .protect_readonly_code(frame_start, frame_len as u32);
            assert!(runner.bus.is_guest_address_mapped(frame_start, frame_len));
            assert!(!runner.bus.is_guest_address_writable(frame_start, frame_len));
            let frame_snapshot = runner.bus.read_bytes(frame_start, frame_len).to_vec();
            runner
                .m68k
                .cpu
                .write_reg(Register::A7, frame_start + frame_len as u32);
            assert!(!runner.fire_menu_hook_proc(0xa93d));
            assert_eq!(runner.bus.read_bytes(frame_start, frame_len), frame_snapshot);
            assert_eq!(
                runner.dispatcher.menu_tracking.request_menu_hook(true),
                Some(key)
            );
            assert_eq!(runner.dispatcher.menu_tracking.context().classic_port, None);
            assert_eq!(runner.dispatcher.guest_calls.depth(), call_depth);
            runner.m68k.cpu.write_reg(Register::A7, valid_sp);
            if cooperative_switch {
                let worker = ExecutionTaskId::from_thread_id(3);
                let worker_entry = runner.bus.alloc(8);
                let worker_stack = runner.bus.alloc(64);
                let worker_sp = worker_stack + 58;
                for (index, word) in [
                    0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                    0xabf2, // ThreadDispatch back to the application
                    0x60fe, // BRA.S -2 if no successor is runnable
                ]
                .into_iter()
                .enumerate()
                {
                    runner.bus.write_word(worker_entry + index as u32 * 2, word);
                }
                runner.bus.write_long(worker_sp, 0);
                runner.bus.write_word(worker_sp + 4, 0xbeef);
                worker_yield_result = Some(worker_sp + 4);
                assert!(runner.dispatcher.guest_calls.register_task(worker));
                assert!(runner.dispatcher.guest_calls.set_thread_storage(
                    worker,
                    crate::guest_call::ThreadStorage {
                        stack_base: worker_stack,
                        stack_limit: worker_stack + 64,
                        ..Default::default()
                    }
                ));
                assert!(runner.dispatcher.guest_calls.save_cooperative_context(
                    worker,
                    CooperativeThread {
                        a_regs: [0, 0, 0, 0, 0, 0, 0, worker_sp],
                        pc: worker_entry,
                        ..Default::default()
                    }
                ));
                assert!(runner.dispatcher.guest_calls.set_scheduling_state(
                    worker,
                    crate::execution_kernel::ExecutionTaskState::Ready
                ));
            }
            assert!(runner.fire_menu_hook_proc(0xa93d));
            assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), Some(key));
            assert!(runner
                .dispatcher
                .menu_tracking
                .context()
                .classic_port
                .is_some());
            if cooperative_switch {
                runner.bus.write_long(0x0a30, 0);
                let worker = ExecutionTaskId::from_thread_id(3);
                for _ in 0..32 {
                    runner.run_steps(1, None);
                    if runner.dispatcher.guest_calls.current_task() == worker {
                        break;
                    }
                }
                assert_eq!(runner.dispatcher.guest_calls.current_task(), worker);
                assert_eq!(runner.bus.read_long(hook_marker), 1);
                assert_eq!(runner.bus.read_long(hook_after_yield_marker), 0);
                assert_eq!(runner.bus.read_byte(0x0172), 0);
                assert_eq!(*runner.dispatcher.current_port, hook_port);
                assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), None);
                assert!(runner.dispatcher.menu_tracking.menu_hook_is_pending(key));
                assert!(runner
                    .dispatcher
                    .menu_tracking
                    .ready_call(GuestIsa::M68k)
                    .is_none());
                let parked = runner
                    .dispatcher
                    .guest_calls
                    .cooperative_context(ExecutionTaskId::APPLICATION)
                    .expect("suspended hook context");
                assert_eq!(parked.pc, hook_yield_resume_pc.unwrap());
                for _ in 0..32 {
                    runner.run_steps(1, None);
                    if runner.dispatcher.guest_calls.current_task()
                        == ExecutionTaskId::APPLICATION
                    {
                        break;
                    }
                }
                assert_eq!(
                    runner.dispatcher.guest_calls.current_task(),
                    ExecutionTaskId::APPLICATION
                );
                assert_eq!(runner.bus.read_word(worker_yield_result.unwrap()), 0);
                assert_eq!(runner.m68k.cpu.read_reg(Register::PC), parked.pc);
                assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parked.a_regs[7]);
                assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), Some(key));
                for _ in 0..32 {
                    runner.run_steps(1, None);
                    assert!(
                        runner.process_context.menu_tracking().is_some(),
                        "held root vanished before hook receipt consumption: task={:?} pc={:08x} button={:02x}",
                        runner.dispatcher.guest_calls.current_task(),
                        runner.m68k.cpu.read_reg(Register::PC),
                        runner.bus.read_byte(0x0172),
                    );
                    if runner.bus.read_long(hook_after_yield_marker) == 1
                        && runner.dispatcher.menu_tracking.menu_hook_key().is_none()
                    {
                        break;
                    }
                }
                assert_eq!(runner.bus.read_long(hook_after_yield_marker), 1);
                assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), None);
                assert!(runner.process_context.menu_tracking().is_some());
            }
        } else {
            runner.push_canonical_mouse_down(10, 16);
        }

        if !cooperative_switch {
            for _ in 0..8 {
                assert!(runner.run_steps(128, None).1);
            }
        }
        if hook && !native_hook {
            assert_eq!(*runner.dispatcher.current_port, hook_port);
            if cooperative_switch {
                assert_eq!(runner.bus.read_long(hook_after_yield_marker), 1);
            }
        }
        let rect = runner
            .process_context
            .menu_tracking()
            .expect("classic tracking remains active")
            .dropdown_rect();
        if custom {
            assert!(
                runner.bus.read_long(marker) > 0,
                "PowerPC draw callback ran"
            );
        }
        let (v, h) = (rect.0 + 24, rect.1 + 16);
        runner.dispatcher.set_mouse_position(v, h);
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
        }
        runner.push_canonical_mouse_up(v, h);
        let patch_marker = runner.bus.alloc(4);
        let patch = runner.bus.alloc(12);
        for (index, word) in [
            0x23fc,
            0,
            1,
            (patch_marker >> 16) as u16,
            patch_marker as u16,
            0x4e75,
        ]
        .into_iter()
        .enumerate()
        {
            runner.bus.write_word(patch + index as u32 * 2, word);
        }
        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, 0xa93d, patch)
            .unwrap();
        for _ in 0..16 {
            assert!(runner.run_steps(128, None).1);
            assert_eq!(
                runner.bus.read_long(patch_marker),
                0,
                "a new MenuSelect patch intercepted the already-active interaction"
            );
            if runner.process_context.menu_tracking().is_none()
                && runner.dispatcher.guest_calls.is_empty()
            {
                break;
            }
        }
        assert!(runner.process_context.menu_tracking().is_none(),
            "tracking stayed live: pc={:08x}, sp={:08x}, patch={:08x}, marker={}, depth={}, pending={:?}",
            runner.m68k.cpu.read_reg(Register::PC), runner.m68k.cpu.read_reg(Register::A7), patch,
            runner.bus.read_long(patch_marker), runner.dispatcher.guest_calls.depth(),
            runner.dispatcher.menu_tracking.as_ref().and_then(|tracking| tracking.definition.as_ref()).and_then(|definition| definition.pending_invocation()));
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert_eq!(runner.bus.read_long(parameters + 4), (140 << 16) | 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parameters + 4);
        assert_eq!(*runner.dispatcher.current_port, original_port);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
        if hook {
            assert!(runner.bus.read_long(hook_marker) > 0, "the guest hook ran");
            assert!(runner.bus.read_word(record + 2) > 80);
            assert_eq!(runner.m68k.cpu.read_reg(Register::D7), 0x77777777);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A6), 0x66666666);
            assert!(runner.dispatcher.guest_calls.is_empty());
            assert!(runner.active_interrupt_callback.is_none());
        }

        if auto_pop {
            runner.bus.write_long(stack, return_pc);
        }
        runner.m68k.cpu.write_reg(Register::PC, entry);
        runner.m68k.cpu.write_reg(Register::A7, stack);
        assert!(runner.run_steps(128, None).1);
        assert_eq!(
            runner.bus.read_long(patch_marker),
            1,
            "fresh entries must still honor the new patch"
        );
    }

    #[test]
    fn standalone_classic_process_enters_powerpc_routine_descriptor_and_resumes_once() {
        use crate::guest_procedure::{
            ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
            ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        };
        use crate::mixed_mode::proc_info;

        const DESCRIPTOR: u32 = 0x0030_0000;
        const TVECTOR: u32 = 0x0030_0100;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        const CALLBACK_RTOC: u32 = 0x0030_0200;
        const M68K_RETURN: u32 = 0x0030_0300;
        const M68K_STACK: u32 = 0x0070_0000;
        const ARGUMENT: u32 = 0x1234_0000;

        let mut native = halted_ppc_app_with_sound(PpcSoundState::default())
            .ppc
            .take()
            .expect("native execution adapter");
        native
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        native.memory.add_region(
            CALLBACK,
            [
                0x3863_0007u32, // addi r3,r3,7
                0x4e80_0020,    // blr
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );

        let classic = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007f_ffc0,
            size_resource: None,
        };
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.stage_ppc_companion(native);
        runner.init_app(&classic);
        assert!(!runner.is_powerpc_app());

        let proc_info = proc_info::PASCAL_STACK_BASED
            | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
            | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
        runner
            .bus
            .write_word(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
        runner
            .bus
            .write_byte(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION);
        runner.bus.write_word(DESCRIPTOR + 10, 0);
        let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        runner.bus.write_long(record, proc_info);
        runner.bus.write_byte(
            record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_POWERPC_ISA,
        );
        runner.bus.write_word(
            record + ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_FLAG_USE_NATIVE_ISA,
        );
        runner
            .bus
            .write_long(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR);
        runner.bus.write_long(TVECTOR, CALLBACK);
        runner.bus.write_long(TVECTOR + 4, CALLBACK_RTOC);
        runner.bus.write_long(M68K_STACK, M68K_RETURN);
        runner.bus.write_long(M68K_STACK + 4, ARGUMENT);
        runner.bus.write_word(M68K_RETURN, 0x201f); // MOVE.L (SP)+,D0
        runner.bus.write_word(M68K_RETURN + 2, 0x4e71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, DESCRIPTOR);
        runner.m68k.cpu.write_reg(Register::A7, M68K_STACK);

        let (classic_steps, classic_running) = runner.run_steps(2, None);
        assert!(classic_steps > 0);
        assert!(classic_running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), DESCRIPTOR + 2);
        assert!(runner.dispatcher.guest_calls.has_powerpc_from_m68k());

        let (native_steps, native_running) = runner.run_steps(64, None);
        assert!(native_steps > 0);
        assert!(native_running);
        assert!(!runner.is_halted());
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), M68K_RETURN);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), M68K_STACK + 8);
        assert_eq!(runner.bus.read_long(M68K_STACK + 8), ARGUMENT + 7);

        let (resumed_steps, resumed_running) = runner.run_steps(1, None);
        assert_eq!(resumed_steps, 1);
        assert!(resumed_running);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), ARGUMENT + 7);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), M68K_STACK + 12);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), M68K_RETURN + 2);
        assert!(runner.dispatcher.guest_calls.is_empty());
    }

    #[test]
    fn raw_os_trap_patch_can_execute_a_native_routine_descriptor() {
        use crate::guest_call::{GuestCallTarget, M68kRegisterState, M68kResultSource};
        use crate::guest_procedure::{
            GuestIsa, ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
            ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA, ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, ROUTINE_RECORD_SIZE,
        };
        use crate::mixed_mode::proc_info;

        const TRAP: u16 = 0xA11E; // NewPtr
        const BYTE_COUNT: u32 = 0x1234;
        const DESCRIPTOR: u32 = 0x0301_0000;
        const TVECTOR: u32 = 0x0301_0100;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        const M68K_FALLBACK: u32 = 0x0301_0200;
        const M68K_ENTRY: u32 = 0x0301_0300;
        const M68K_STACK: u32 = 0x0302_0000;
        const INITIAL_SP: u32 = M68K_STACK + 0x80;
        const RETURN_PC: u32 = 0x0302_0100;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        ppc_app
            .memory
            .add_region(CALLBACK, 0x4e80_0020u32.to_be_bytes().to_vec()); // blr
        ppc_app.memory.add_region(DESCRIPTOR, vec![0; 0x400]);
        ppc_app.memory.add_region(M68K_STACK, vec![0; 0x200]);
        ppc_app
            .memory
            .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
            .unwrap();
        ppc_app
            .memory
            .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        ppc_app.memory.write_u16_be(DESCRIPTOR + 10, 1).unwrap();

        // NewPtr's documented register-based ProcInfo passes the actual trap
        // word from D1 first, then the allocation size from D0, and returns
        // the pointer in A0. Inside Macintosh: PowerPC System Software (1994),
        // pp. 1-67--1-68, Listing 1-14.
        let proc_info = proc_info::REGISTER_BASED
            | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
            | (4 << proc_info::REGISTER_RESULT_LOCATION_PHASE)
            | (6 << proc_info::REGISTER_PARAMETER_PHASE)
            | (3 << (proc_info::REGISTER_PARAMETER_PHASE + proc_info::REGISTER_PARAMETER_WIDTH));
        let m68k_record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        ppc_app.memory.write_u32_be(m68k_record, proc_info).unwrap();
        ppc_app
            .memory
            .write_u8(
                m68k_record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_M68K_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(
                m68k_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                M68K_FALLBACK,
            )
            .unwrap();
        let native_record = m68k_record + ROUTINE_RECORD_SIZE;
        ppc_app
            .memory
            .write_u32_be(native_record, proc_info)
            .unwrap();
        ppc_app
            .memory
            .write_u8(
                native_record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_POWERPC_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u16_be(
                native_record + ROUTINE_RECORD_FLAGS_OFFSET,
                ROUTINE_FLAG_USE_NATIVE_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(
                native_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                TVECTOR,
            )
            .unwrap();
        ppc_app.memory.write_u32_be(TVECTOR, CALLBACK).unwrap();
        ppc_app.memory.write_u32_be(TVECTOR + 4, 0).unwrap();
        ppc_app.memory.write_u16_be(M68K_FALLBACK, 0x4e75).unwrap();
        ppc_app.memory.write_u16_be(M68K_ENTRY, TRAP).unwrap();
        ppc_app.memory.write_u16_be(M68K_ENTRY + 2, 0x4e75).unwrap();
        ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
        let mut registers = M68kRegisterState::default();
        registers.data[0] = BYTE_COUNT;
        registers.data[1] = 0xdead_beef;
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            registers,
            Some(M68kResultSource::Address(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, TRAP, DESCRIPTOR)
            .expect("native descriptor patch must install");

        let (m68k_steps, m68k_running) = runner.run_steps(2, None);
        assert!(m68k_steps > 0);
        assert!(m68k_running);
        let pending = runner
            .dispatcher
            .guest_calls
            .pending_powerpc_from_m68k()
            .expect("native record should be pending");
        assert_eq!(pending.arguments.as_slice(), &[u32::from(TRAP), BYTE_COUNT]);

        let (steps, running) = runner.run_steps(64, None);

        assert!(steps > 0);
        assert!(running);
        assert!(!runner.is_halted());
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert!(runner.dispatcher.pending_native_trap_calls.is_empty());
        let ppc_app = runner.native.application().expect("PPC app retained");
        assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
        assert_eq!(ppc_app.cpu.gpr[3], u32::from(TRAP));
    }

    #[test]
    fn opposite_abi_thread_disposal_preserves_stack_provenance_and_retries_results() {
        const FRAME: u32 = 0x6000;
        const MADE: u32 = 0x7000;
        const RESULT: u32 = 0x7100;
        for (native_worker, recycle) in [(false, false), (false, true), (true, false), (true, true)]
        {
            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let loaded = app.ppc.as_mut().unwrap();
            loaded.entry_pc = PPC_IMPORT_TRAP_BASE;
            loaded.cpu.pc = PPC_IMPORT_TRAP_BASE;
            loaded.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
            let name = if native_worker {
                "NewThread"
            } else {
                "DisposeThread"
            };
            let mut binding = test_ppc_import_binding(0, "InterfaceLib", name);
            binding.dispatcher_target = if native_worker {
                PpcImportDispatcherTarget::NewThread
            } else {
                PpcImportDispatcherTarget::DisposeThread
            };
            binding.trap_pc = PPC_IMPORT_TRAP_BASE;
            loaded.import_count = 1;
            loaded.imports.push(binding);
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.init_app(&app);
            if native_worker {
                let cpu = &mut runner.native.application_mut().unwrap().cpu;
                cpu.lr = PPC_CODE_BASE;
                cpu.gpr[3] = 1;
                cpu.gpr[4] = PPC_CODE_BASE;
                cpu.gpr[5] = 0;
                cpu.gpr[6] = 1024;
                cpu.gpr[7] = 1;
                cpu.gpr[8] = RESULT;
                cpu.gpr[9] = MADE;
                assert!(runner.run_steps(32, None).1);
            } else {
                runner.m68k.cpu.write_reg(Register::A7, FRAME);
                runner.m68k.cpu.write_reg(Register::D0, 0x0e03);
                for (offset, value) in [
                    (0, MADE),
                    (4, RESULT),
                    (8, 1),
                    (12, 1024),
                    (16, 0),
                    (20, 0x8000),
                    (24, 1),
                ] {
                    runner.bus.write_long(FRAME + offset, value);
                }
                runner
                    .dispatcher
                    .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
                    .unwrap()
                    .unwrap();
                assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0);
            }
            let calls = runner.dispatcher.guest_calls.shared_handle();
            let worker = ExecutionTaskId::from_thread_id(runner.bus.read_long(MADE));
            assert_eq!(worker.thread_id(), 3);
            let mut storage = calls.thread_storage(worker).unwrap();
            assert_eq!(storage.managed_pointer, native_worker);
            assert_ne!(storage.stack_base, 0);
            runner.bus.write_long(RESULT, 0x12345678);
            storage.result_destination = u32::MAX - 1;
            assert!(calls.set_thread_storage(worker, storage));
            for (attempt, expected) in [(0, -619_i16), (1, 0), (2, -618)] {
                if attempt == 1 {
                    storage.result_destination = RESULT;
                    assert!(calls.set_thread_storage(worker, storage));
                }
                if native_worker {
                    runner.m68k.cpu.write_reg(Register::A7, FRAME);
                    runner.m68k.cpu.write_reg(Register::D0, 0x0504);
                    runner
                        .bus
                        .write_word(FRAME, if recycle { 0x0100 } else { 0 });
                    runner.bus.write_long(FRAME + 2, 0xcafebabe);
                    runner.bus.write_long(FRAME + 6, worker.thread_id());
                    runner
                        .dispatcher
                        .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
                        .unwrap()
                        .unwrap();
                    assert_eq!(runner.m68k.cpu.read_reg(Register::D0) as i16, expected);
                } else {
                    let cpu = &mut runner.native.application_mut().unwrap().cpu;
                    cpu.pc = PPC_IMPORT_TRAP_BASE;
                    cpu.lr = PPC_CODE_BASE;
                    cpu.gpr[3] = worker.thread_id();
                    cpu.gpr[4] = 0xcafebabe;
                    cpu.gpr[5] = u32::from(recycle);
                    assert!(runner.run_steps(32, None).1);
                    assert_eq!(
                        runner.native.application().unwrap().cpu.gpr[3] as i16,
                        expected
                    );
                }
                assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
                assert_eq!(calls.thread_storage(worker).is_some(), attempt == 0);
                assert_eq!(
                    runner.bus.read_long(RESULT),
                    if attempt == 0 { 0x12345678 } else { 0xcafebabe }
                );
                if native_worker {
                    let manager = runner.dispatcher.process_memory_manager();
                    let live = manager
                        .borrow_mut()
                        .native_mut()
                        .native_ptr_records()
                        .iter()
                        .any(|record| record.ptr == storage.stack_base);
                    assert_eq!(live, attempt == 0 || recycle);
                    assert_eq!(calls.classic_thread_pool_count(0), 0);
                } else {
                    assert_eq!(
                        runner.bus.get_alloc_size(storage.stack_base).is_some(),
                        attempt == 0 || recycle
                    );
                    assert_eq!(
                        calls.classic_thread_pool_count(0),
                        usize::from(attempt != 0 && recycle)
                    );
                }
            }
            // Request the recycled stack through its original ABI. A released
            // stack is unavailable, while recycled storage keeps its allocation
            // and acquires a fresh identity and result destination.
            if native_worker {
                let cpu = &mut runner.native.application_mut().unwrap().cpu;
                cpu.pc = PPC_IMPORT_TRAP_BASE;
                cpu.lr = PPC_CODE_BASE;
                cpu.gpr[3] = 1;
                cpu.gpr[4] = PPC_CODE_BASE;
                cpu.gpr[5] = 0xabcdef;
                cpu.gpr[6] = 1024;
                cpu.gpr[7] = 1 | 2 | 16;
                cpu.gpr[8] = RESULT + 4;
                cpu.gpr[9] = MADE;
                assert!(runner.run_steps(32, None).1);
                assert_eq!(
                    runner.native.application().unwrap().cpu.gpr[3] as i16,
                    if recycle { 0 } else { -617 }
                );
            } else {
                runner.m68k.cpu.write_reg(Register::A7, FRAME);
                runner.m68k.cpu.write_reg(Register::D0, 0x0e03);
                for (offset, value) in [
                    (0, MADE),
                    (4, RESULT + 4),
                    (8, 1 | 2 | 16),
                    (12, 1024),
                    (16, 0xabcdef),
                    (20, 0x8000),
                    (24, 1),
                ] {
                    runner.bus.write_long(FRAME + offset, value);
                }
                runner
                    .dispatcher
                    .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
                    .unwrap()
                    .unwrap();
                assert_eq!(
                    runner.m68k.cpu.read_reg(Register::D0) as i16,
                    if recycle { 0 } else { -617 }
                );
            }
            assert_eq!(runner.bus.read_long(MADE), if recycle { 4 } else { 0 });
            if recycle {
                let reused = calls
                    .thread_storage(ExecutionTaskId::from_thread_id(4))
                    .unwrap();
                assert_eq!(reused.stack_base, storage.stack_base);
                assert_eq!(reused.stack_limit, storage.stack_limit);
                assert_eq!(reused.managed_pointer, native_worker);
                assert_eq!(reused.result_destination, RESULT + 4);
                assert!(calls.thread_storage(worker).is_none());
            }
        }
    }

    #[test]
    fn dispose_thread_refuses_the_application_through_both_public_abis() {
        const FRAME: u32 = 0x6000;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let loaded = app.ppc.as_mut().unwrap();
        loaded.entry_pc = PPC_IMPORT_TRAP_BASE;
        loaded.cpu.pc = PPC_IMPORT_TRAP_BASE;
        loaded.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
        let mut binding = test_ppc_import_binding(0, "InterfaceLib", "DisposeThread");
        binding.dispatcher_target = PpcImportDispatcherTarget::DisposeThread;
        binding.trap_pc = PPC_IMPORT_TRAP_BASE;
        loaded.import_count = 1;
        loaded.imports.push(binding);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let calls = runner.dispatcher.guest_calls.shared_handle();
        let before = calls.clone();
        {
            let cpu = &mut runner.native.application_mut().unwrap().cpu;
            cpu.lr = PPC_CODE_BASE;
            cpu.gpr[3] = ExecutionTaskId::APPLICATION.thread_id();
            cpu.gpr[4] = 0xcafe_babe;
            cpu.gpr[5] = 0;
        }
        assert!(runner.run_steps(32, None).1);
        assert_eq!(
            runner.native.application().unwrap().cpu.gpr[3] as i16,
            crate::thread_manager::THREAD_PROTOCOL_ERR
        );
        assert_eq!(calls, before);

        runner.m68k.cpu.write_reg(Register::A7, FRAME);
        runner.bus.write_word(FRAME, 0);
        runner.bus.write_long(FRAME + 2, 0xcafe_babe);
        runner
            .bus
            .write_long(FRAME + 6, ExecutionTaskId::APPLICATION.thread_id());
        runner.bus.write_word(FRAME + 10, 0xbeef);
        runner.m68k.cpu.write_reg(Register::D0, 0x0504);
        runner
            .dispatcher
            .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap()
            .unwrap();
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::D0) as i16,
            crate::thread_manager::THREAD_PROTOCOL_ERR
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), FRAME + 10);
        assert_eq!(
            runner.bus.read_word(FRAME + 10) as i16,
            crate::thread_manager::THREAD_PROTOCOL_ERR
        );
        assert_eq!(calls, before);
    }

    #[test]
    fn classic_thread_return_trampoline_retries_refused_retirement_without_rts_fallthrough() {
        const FRAME: u32 = 0x6000;
        const YIELD_FRAME: u32 = 0x6100;
        const MADE: u32 = 0x7000;
        const RESULT: u32 = 0x7100;
        const ENTRY: u32 = 0x8000;
        const APP_PC: u32 = 0x9000;
        const PARAM: u32 = 0xdead_beef;
        const THREAD_RESULT: u32 = 0xcafe_babe;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.bus.write_word(ENTRY, 0x4e75); // RTS
        runner.m68k.cpu.write_reg(Register::A7, FRAME);
        for (offset, value) in [
            (0, MADE),
            (4, RESULT),
            (8, 0),
            (12, 1024),
            (16, PARAM),
            (20, ENTRY),
            (24, 1),
        ] {
            runner.bus.write_long(FRAME + offset, value);
        }
        runner.m68k.cpu.write_reg(Register::D0, 0x0e03);
        runner
            .dispatcher
            .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap()
            .unwrap();
        let worker = ExecutionTaskId::from_thread_id(runner.bus.read_long(MADE));
        let worker_context = runner
            .dispatcher
            .guest_calls
            .cooperative_context(worker)
            .unwrap();
        let worker_sp = worker_context.a_regs[7];
        let trampoline = runner.bus.read_long(worker_sp);
        assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);

        runner.m68k.cpu.write_reg(Register::PC, APP_PC);
        runner.m68k.cpu.write_reg(Register::A0, 0x1111_2222);
        runner.m68k.cpu.write_reg(Register::A7, YIELD_FRAME);
        runner.bus.write_long(YIELD_FRAME, worker.thread_id());
        runner.bus.write_word(YIELD_FRAME + 4, 0xbeef);
        runner.m68k.cpu.write_reg(Register::D0, 0x0205);
        runner
            .dispatcher
            .dispatch_toolbox(true, 0x3f2, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap()
            .unwrap();
        let application_context = runner
            .dispatcher
            .guest_calls
            .cooperative_context(ExecutionTaskId::APPLICATION)
            .unwrap();
        assert_eq!(runner.dispatcher.guest_calls.current_task(), worker);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), ENTRY);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp);

        runner.m68k.cpu.write_reg(Register::A0, THREAD_RESULT);
        runner.dispatcher.guest_calls.begin_critical();
        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp + 4);
        assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);

        for _ in 0..2 {
            assert!(matches!(
                runner.m68k.cpu.step(&mut runner.bus),
                crate::cpu::StepResult::Ok
            ));
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline + 4);
            assert_eq!(runner.m68k.cpu.read_reg(Register::D0) & 0xffff, 0xfffe);
            assert!(matches!(
                runner.m68k.cpu.step(&mut runner.bus),
                crate::cpu::StepResult::Aline(0xabf2)
            ));
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline + 6);
            let before = runner.dispatcher.guest_calls.clone();
            runner
                .dispatch_classic_with_process_services(0xabf2)
                .unwrap();
            assert_eq!(runner.dispatcher.guest_calls, before);
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp + 4);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A0), THREAD_RESULT);
            assert_eq!(runner.m68k.cpu.read_reg(Register::D0) as i16, -619);
            assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);
            assert_eq!(runner.bus.read_long(RESULT), 0);
        }

        let before_bounded_retry = runner.dispatcher.guest_calls.clone();
        let (steps, running) = runner.run_steps(8, None);
        assert_eq!(steps, 8);
        assert!(running);
        assert_eq!(runner.dispatcher.guest_calls, before_bounded_retry);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), worker_sp + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), THREAD_RESULT);
        assert_eq!(runner.bus.read_long(worker_sp + 4), PARAM);
        assert_eq!(runner.bus.read_long(RESULT), 0);

        assert!(runner.dispatcher.guest_calls.end_critical());
        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), trampoline + 4);
        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Aline(0xabf2)
        ));
        runner
            .dispatch_classic_with_process_services(0xabf2)
            .unwrap();

        assert_eq!(
            runner.dispatcher.guest_calls.current_task(),
            ExecutionTaskId::APPLICATION
        );
        assert_eq!(runner.bus.read_long(RESULT), THREAD_RESULT);
        assert_eq!(
            CooperativeThread::capture(&runner.m68k.cpu),
            application_context
        );
        assert_eq!(runner.dispatcher.guest_calls.scheduling_state(worker), None);
        assert_eq!(
            runner.dispatcher.guest_calls.cooperative_context(worker),
            None
        );
    }

    #[test]
    fn stopped_last_thread_waits_for_a_task_reference_wakeup() {
        use crate::cpu::CpuOps;
        use crate::execution_kernel::ExecutionTaskState;
        use crate::trap::test_helpers::{setup, TEST_SP};
        const CLASSIC_PC: u32 = 0x4000;
        const CLASSIC_SP: u32 = 0x5000;
        for native in [false, true] {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            if native {
                let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
                let loaded = app.ppc.as_mut().unwrap();
                loaded.entry_pc = PPC_IMPORT_TRAP_BASE;
                loaded.cpu.pc = PPC_IMPORT_TRAP_BASE;
                loaded.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
                loaded.memory.add_region(
                    PPC_CODE_BASE,
                    [0x3a800055_u32, 0x48000000]
                        .into_iter()
                        .flat_map(u32::to_be_bytes)
                        .collect(),
                );
                let mut binding = test_ppc_import_binding(0, "InterfaceLib", "SetThreadState");
                binding.dispatcher_target = PpcImportDispatcherTarget::SetThreadState;
                binding.trap_pc = PPC_IMPORT_TRAP_BASE;
                loaded.import_count = 1;
                loaded.imports.push(binding);
                runner.init_app(&app);
                let cpu = &mut runner.native.application_mut().unwrap().cpu;
                cpu.lr = PPC_CODE_BASE;
                cpu.gpr[3] = 1;
                cpu.gpr[4] = 1;
                cpu.gpr[5] = 0;
                cpu.gpr[20] = 0;
            } else {
                for (i, word) in [0x303cu16, 0x0508, 0xabf2, 0x7c55, 0x60fe]
                    .into_iter()
                    .enumerate()
                {
                    runner.bus.write_word(CLASSIC_PC + i as u32 * 2, word);
                }
                runner.bus.write_long(CLASSIC_SP, 0);
                runner.bus.write_word(CLASSIC_SP + 4, 1);
                runner.bus.write_long(CLASSIC_SP + 6, 1);
                runner.m68k.cpu.write_reg(Register::PC, CLASSIC_PC);
                runner.m68k.cpu.write_reg(Register::A7, CLASSIC_SP);
                runner.m68k.cpu.write_reg(Register::D6, 0);
            }
            let calls = runner.dispatcher.guest_calls.shared_handle();
            let (steps, running) = runner.run_steps(32, None);
            assert!(steps > 0 && running);
            assert_eq!(
                calls.scheduling_state(ExecutionTaskId::APPLICATION),
                Some(ExecutionTaskState::Stopped)
            );
            assert!(!runner.m68k.can_relaunch());
            for _ in 0..3 {
                assert_eq!(runner.run_steps(32, None), (0, true));
            }
            if native {
                let cpu = &runner.native.application().unwrap().cpu;
                assert_eq!(cpu.pc, PPC_CODE_BASE);
                assert_eq!(cpu.gpr[20], 0);
            } else {
                assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_PC + 6);
                assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0);
                assert_eq!(runner.m68k.cpu.read_reg(Register::A7), CLASSIC_SP + 10);
            }
            // An interrupt/completion edge may mark the stopped thread ready;
            // the wake call must not execute or replace the suspended CPU.
            let (mut wake, mut cpu, mut bus) = setup();
            wake.guest_calls = calls.shared_handle();
            bus.write_long(TEST_SP, ExecutionTaskId::APPLICATION.thread_id());
            bus.write_long(TEST_SP + 4, 2);
            cpu.write_reg(Register::D0, 0x0410);
            wake.dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
                .unwrap()
                .unwrap();
            assert_eq!(cpu.read_reg(Register::D0), 0);
            assert_eq!(
                calls.scheduling_state(ExecutionTaskId::APPLICATION),
                Some(ExecutionTaskState::Ready)
            );
            let (steps, running) = runner.run_steps(32, None);
            assert!(steps > 0 && running);
            assert!(calls.current_task_is_running());
            if native {
                assert_eq!(runner.native.application().unwrap().cpu.gpr[20], 0x55);
            } else {
                assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0x55);
            }
        }
    }

    #[test]
    fn native_yield_roundtrips_classic_worker_yield_and_retirement() {
        use crate::execution_kernel::ExecutionTaskState;
        use crate::guest_call::CooperativeThread;
        const CLASSIC_ENTRY: u32 = 0x0305_0000;
        const CLASSIC_SP: u32 = 0x0305_1100;
        const RESULT: u32 = 0x0305_2000;
        for retire in [false, true] {
            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let native = app.ppc.as_mut().unwrap();
            native.cpu.pc = PPC_IMPORT_TRAP_BASE;
            native.entry_pc = PPC_IMPORT_TRAP_BASE;
            native.cpu.lr = PPC_CODE_BASE;
            native.cpu.gpr[20] = 0x1122_3344;
            native.cpu.fpr[20] = 0x4009_21fb_5444_2d18;
            native.cpu.cr = 0x1357_2468;
            native.memory.add_region(PPC_IMPORT_TRAP_BASE, vec![0; 4]);
            native
                .memory
                .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
            native.memory.add_region(
                CLASSIC_ENTRY,
                [
                    0x7c55u16,
                    0x303c,
                    if retire { 0xfffe } else { 0x0205 },
                    0xabf2,
                    0x60fe,
                ]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect(),
            );
            native.memory.add_region(CLASSIC_SP, vec![0; 16]);
            native.memory.write_u32_be(CLASSIC_SP, 2).unwrap();
            native.memory.add_region(RESULT, vec![0; 4]);
            let mut binding = test_ppc_import_binding(0, "InterfaceLib", "YieldToAnyThread");
            binding.dispatcher_target = PpcImportDispatcherTarget::YieldToAnyThread;
            binding.trap_pc = PPC_IMPORT_TRAP_BASE;
            native.import_count = 1;
            native.imports.push(binding);
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.init_app(&app);
            // Launch installs its entry return convention; use a live continuation
            // that loops after the yield so it cannot halt the test's process.
            runner.native.application_mut().unwrap().cpu.lr = PPC_CODE_BASE;
            let calls = runner.dispatcher.guest_calls.shared_handle();
            let worker = calls.create_task().unwrap();
            let mut context = CooperativeThread::default();
            context.pc = CLASSIC_ENTRY;
            context.a_regs[0] = 0x5566_7788;
            context.a_regs[7] = CLASSIC_SP;
            assert!(calls.set_thread_storage(
                worker,
                crate::guest_call::ThreadStorage {
                    result_destination: RESULT,
                    ..Default::default()
                }
            ));
            assert!(calls.save_cooperative_context(worker, context));
            assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
            let (_, running) = runner.run_steps(64, None);
            assert!(running);
            assert_eq!(calls.current_task(), worker);
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_ENTRY);
            let (_, running) = runner.run_steps(64, None);
            assert!(running);
            assert_eq!(calls.current_task(), ExecutionTaskId::APPLICATION);
            assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0x55);
            assert!(calls.has_pending_task_handoff());
            assert!(!runner.m68k.can_relaunch());
            if retire {
                assert_eq!(calls.scheduling_state(worker), None);
                assert_eq!(runner.bus.read_long(RESULT), 0x5566_7788);
            }
            let (_, running) = runner.run_steps(64, None);
            assert!(running);
            assert!(!calls.has_pending_task_handoff());
            let native = runner.native.application().unwrap();
            assert_eq!(native.cpu.pc, PPC_CODE_BASE);
            assert_eq!(native.cpu.gpr[20], 0x1122_3344);
            assert_eq!(native.cpu.fpr[20], 0x4009_21fb_5444_2d18);
            assert_eq!(native.cpu.cr, 0x1357_2468);
        }
    }

    #[test]
    fn classic_worker_uses_native_application_engine_and_stops_at_its_return() {
        use crate::execution_kernel::ExecutionTaskState;
        use crate::guest_call::{GuestCallTarget, M68kResultTarget, PowerPcArguments};
        use crate::guest_procedure::GuestIsa;
        const ENTRY: u32 = PPC_CODE_BASE + 0x1000;
        const CLASSIC_RETURN: u32 = 0x0304_0000;
        const CLASSIC_SP: u32 = 0x0304_1080;
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let native = app.ppc.as_mut().unwrap();
        native
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
        native.memory.add_region(
            ENTRY,
            [0x3860_002au32, 0x4e80_0020]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        native.memory.add_region(CLASSIC_RETURN, vec![0x60, 0xfe]);
        native.memory.add_region(CLASSIC_SP - 0x80, vec![0; 0x100]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let native_pc = runner.native.application().unwrap().cpu.pc;
        let calls = runner.dispatcher.guest_calls.shared_handle();
        let worker = ExecutionTaskId::from_thread_id(3);
        assert!(calls.register_task(worker));
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(calls.switch_to_task(worker));
        runner.m68k.cpu.write_reg(Register::PC, CLASSIC_RETURN);
        runner.m68k.cpu.write_reg(Register::A7, CLASSIC_SP);
        assert!(calls.begin_m68k_to_powerpc(
            GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: ENTRY,
                rtoc: 0
            },
            PowerPcArguments::from_slice(&[]).unwrap(),
            CLASSIC_RETURN,
            CLASSIC_SP,
            Some(M68kResultTarget::Data { index: 0, size: 4 })
        ));
        let (steps, running) = runner.run_steps(64, None);
        assert!(steps > 0);
        assert!(
            running,
            "coalescing must not continue into the suspended application's halt"
        );
        assert!(
            calls.is_empty(),
            "the worker's native call must execute without a companion"
        );
        assert_eq!(calls.current_task(), worker);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 42);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), CLASSIC_RETURN);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), CLASSIC_SP);
        assert_eq!(runner.native.application().unwrap().cpu.pc, native_pc);
        assert_eq!(
            calls.execution_route(runner.native.availability()),
            ExecutionRoute::Classic
        );
    }

    #[test]
    fn nested_cross_isa_callback_survives_a_cooperative_task_switch() {
        for native_worker in [false, true] {
            use crate::guest_procedure::{
                ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
                ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA,
                ROUTINE_RECORD_FLAGS_OFFSET, ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA,
                ROUTINE_RECORD_POWERPC_ISA, ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            };
            use crate::mixed_mode::proc_info;

            const OUTER_ENTRY: u32 = 0x0301_0000;
            const OUTER_DESCRIPTOR: u32 = 0x0301_0100;
            const OUTER_TVECTOR: u32 = 0x0301_0200;
            const PPC_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
            const CALLBACK_RTOC: u32 = 0x0301_0300;
            const INNER_DESCRIPTOR: u32 = 0x0301_0400;
            const INNER_ENTRY: u32 = 0x0301_0500;
            const STACK_BASE: u32 = 0x0302_0000;
            const INITIAL_SP: u32 = STACK_BASE + 0x80;
            const OUTER_RETURN: u32 = 0x0302_0110;
            const WORKER_ENTRY: u32 = 0x0303_0000;
            const WORKER_STACK: u32 = 0x0303_1000;
            const WORKER_SP: u32 = WORKER_STACK + 0x10;
            const ARGUMENT: u32 = 0x1020_3040;

            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let ppc_app = app.ppc.as_mut().expect("PPC app");
            ppc_app
                .memory
                .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
            ppc_app.memory.add_region(
                OUTER_ENTRY,
                [
                    0x4ef9,
                    (OUTER_DESCRIPTOR >> 16) as u16,
                    OUTER_DESCRIPTOR as u16,
                ]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect(),
            );
            ppc_app.memory.add_region(
                INNER_ENTRY,
                [
                    0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                    0xabf2, // ThreadDispatch
                    0x598f, // SUBQ.L #4,SP (undo the selector frame pop)
                    0x4e75, // RTS through the PPC Mixed Mode gateway
                ]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect(),
            );
            ppc_app.memory.add_region(
                WORKER_ENTRY,
                [
                    0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                    0xabf2, // ThreadDispatch back to the application task
                    0x60fe, // BRA.S -2 if no successor is currently runnable
                ]
                .into_iter()
                .flat_map(u16::to_be_bytes)
                .collect(),
            );
            ppc_app.memory.add_region(OUTER_DESCRIPTOR, vec![0; 0x100]);
            ppc_app.memory.add_region(INNER_DESCRIPTOR, vec![0; 0x100]);
            ppc_app.memory.add_region(OUTER_TVECTOR, vec![0; 8]);
            ppc_app.memory.add_region(STACK_BASE, vec![0; 0x200]);
            ppc_app.memory.add_region(WORKER_STACK, vec![0; 0x40]);

            // A Pascal native-to-68K call owns one return long followed by its
            // argument. The descriptor consumes both, so the completion boundary
            // is exactly eight bytes above the initial stack pointer.
            ppc_app
                .memory
                .write_u32_be(INITIAL_SP, OUTER_RETURN)
                .unwrap();
            ppc_app
                .memory
                .write_u32_be(INITIAL_SP + 4, ARGUMENT)
                .unwrap();
            ppc_app.memory.write_u32_be(WORKER_SP, 0).unwrap();

            let proc_info = proc_info::PASCAL_STACK_BASED
                | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
            ppc_app
                .memory
                .write_u16_be(OUTER_DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
                .unwrap();
            ppc_app
                .memory
                .write_u8(OUTER_DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
                .unwrap();
            ppc_app
                .memory
                .write_u16_be(OUTER_DESCRIPTOR + 10, 0)
                .unwrap();
            let outer_record = OUTER_DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
            ppc_app
                .memory
                .write_u32_be(outer_record, proc_info)
                .unwrap();
            ppc_app
                .memory
                .write_u8(
                    outer_record + ROUTINE_RECORD_ISA_OFFSET,
                    ROUTINE_RECORD_POWERPC_ISA,
                )
                .unwrap();
            ppc_app
                .memory
                .write_u16_be(
                    outer_record + ROUTINE_RECORD_FLAGS_OFFSET,
                    ROUTINE_FLAG_USE_NATIVE_ISA,
                )
                .unwrap();
            ppc_app
                .memory
                .write_u32_be(
                    outer_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                    OUTER_TVECTOR,
                )
                .unwrap();
            ppc_app
                .memory
                .write_u32_be(OUTER_TVECTOR, PPC_CALLBACK)
                .unwrap();
            ppc_app
                .memory
                .write_u32_be(OUTER_TVECTOR + 4, CALLBACK_RTOC)
                .unwrap();

            let inner_proc_info = proc_info::PASCAL_STACK_BASED;
            ppc_app
                .memory
                .write_u16_be(INNER_DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
                .unwrap();
            ppc_app
                .memory
                .write_u8(INNER_DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
                .unwrap();
            ppc_app
                .memory
                .write_u16_be(INNER_DESCRIPTOR + 10, 0)
                .unwrap();
            let inner_record = INNER_DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
            ppc_app
                .memory
                .write_u32_be(inner_record, inner_proc_info)
                .unwrap();
            ppc_app
                .memory
                .write_u8(
                    inner_record + ROUTINE_RECORD_ISA_OFFSET,
                    ROUTINE_RECORD_M68K_ISA,
                )
                .unwrap();
            ppc_app
                .memory
                .write_u32_be(
                    inner_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                    INNER_ENTRY,
                )
                .unwrap();

            let callback_words = [
                0x7fe8_02a6, // MFLR R31
                0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
                0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
                0x3c80_0000, // LIS R4,0 (void ProcInfo)
                0x6084_0000, // ORI R4,R4,0
                0x38a0_0000, // LI R5,0 (unused for a void signature)
                ppc_test_relative_branch(PPC_CALLBACK + 6 * 4, PPC_IMPORT_TRAP_BASE) | 1,
                0x7fe8_03a6, // MTLR R31
                0x3863_0007, // ADDI R3,R3,7
                0x4e80_0020, // BLR
            ];
            ppc_app.memory.add_region(
                PPC_CALLBACK,
                callback_words
                    .into_iter()
                    .flat_map(u32::to_be_bytes)
                    .collect(),
            );
            let mut call_universal_proc =
                test_ppc_import_binding(0, "InterfaceLib", "CallUniversalProc");
            call_universal_proc.trap_pc = PPC_IMPORT_TRAP_BASE;
            call_universal_proc.dispatcher_target = PpcImportDispatcherTarget::CallUniversalProc;
            ppc_app.import_count = 1;
            ppc_app.imports = vec![call_universal_proc];
            if native_worker {
                let mut yielding = test_ppc_import_binding(1, "InterfaceLib", "YieldToThread");
                yielding.dispatcher_target = PpcImportDispatcherTarget::YieldToThread;
                yielding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
                ppc_app
                    .memory
                    .add_region(PPC_IMPORT_TRAP_BASE + 4, vec![0; 4]);
                ppc_app.imports.push(yielding);
                ppc_app.import_count = 2;
            }

            assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
                crate::guest_call::GuestCallTarget {
                    isa: crate::guest_procedure::GuestIsa::M68k,
                    entry: OUTER_ENTRY,
                    rtoc: 0,
                },
                OUTER_ENTRY,
                INITIAL_SP,
                OUTER_RETURN,
                INITIAL_SP + 8,
                crate::guest_call::M68kRegisterState::default(),
                None,
                PPC_CODE_BASE,
                0,
                PpcNativeReturnGpr3::Preserve,
            ));

            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.init_app(&app);
            if native_worker {
                let mut cpu = PpcCpu::new();
                cpu.pc = PPC_IMPORT_TRAP_BASE + 4;
                cpu.lr = PPC_CODE_BASE;
                cpu.gpr[1] = WORKER_SP;
                cpu.gpr[3] = ExecutionTaskId::APPLICATION.thread_id();
                let task = runner
                    .dispatcher
                    .guest_calls
                    .create_native_thread(
                        crate::guest_call::NativeThreadContext {
                            context: cpu.capture_execution_context(),
                        },
                        crate::guest_call::ThreadStorage {
                            result_destination: 0,
                            stack_base: 0,
                            stack_limit: 0,
                            managed_pointer: true,
                        },
                        true,
                        |_| true,
                    )
                    .unwrap();
                assert_eq!(task.thread_id(), 3);
            } else {
                assert!(runner
                    .dispatcher
                    .guest_calls
                    .register_task(ExecutionTaskId::from_thread_id(3)));
                assert!(runner.dispatcher.guest_calls.set_thread_storage(
                    ExecutionTaskId::from_thread_id(3),
                    crate::guest_call::ThreadStorage {
                        stack_base: WORKER_STACK,
                        stack_limit: WORKER_STACK + 0x40,
                        ..Default::default()
                    }
                ));
                runner.dispatcher.guest_calls.save_cooperative_context(
                    ExecutionTaskId::from_thread_id(3),
                    CooperativeThread {
                        d_regs: [0; 8],
                        a_regs: [0, 0, 0, 0, 0, 0, 0, WORKER_SP],
                        pc: WORKER_ENTRY,
                        ccr: 0,
                        extended: None,
                        switch_in: (0, 0),
                        switch_out: (0, 0),
                        terminator: (0, 0),
                    },
                );
            }
            assert!(runner.dispatcher.guest_calls.set_scheduling_state(
                ExecutionTaskId::from_thread_id(3),
                crate::execution_kernel::ExecutionTaskState::Ready
            ));

            let (_, running) = runner.run_steps(128, None);
            assert!(running);
            assert_eq!(runner.dispatcher.guest_calls.current_task().thread_id(), 3);
            assert_eq!(
            runner.dispatcher.guest_calls.m68k_context_bank().borrow().task_len(ExecutionTaskId::APPLICATION),
            1,
            "the application-owned nested 68K context must stay parked while its callback yields"
        );
            assert_eq!(
                runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow()
                    .task_len(ExecutionTaskId::from_thread_id(3)),
                0,
                "the worker must not consume the application's parked context"
            );

            let (_, running) = runner.run_steps(128, None);
            assert!(running);
            assert_eq!(runner.dispatcher.guest_calls.current_task().thread_id(), 2);
            if !native_worker {
                assert_eq!(
                    runner
                        .dispatcher
                        .guest_calls
                        .m68k_context_bank()
                        .borrow()
                        .task_len(ExecutionTaskId::APPLICATION),
                    1,
                    "returning from the worker must leave the nested application context parked"
                );
            }

            if !runner.dispatcher.guest_calls.is_empty() {
                let (_, running) = runner.run_steps(128, None);
                assert!(running);
            }
            assert_eq!(runner.dispatcher.guest_calls.current_task().thread_id(), 2);
            assert!(runner.dispatcher.guest_calls.is_empty());
            assert!(runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .is_empty());
            let ppc_app = runner.native.application().expect("PPC app retained");
            assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
            // The outer routine has a void ProcInfo, so its internal callback's
            // transient R3 value must not leak into the parked native caller.
            assert_eq!(ppc_app.cpu.gpr[3], 0);
        }
    }

    #[test]
    fn nested_cross_isa_calls_restore_each_68k_cpu_context() {
        use crate::guest_procedure::{
            ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
            ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA, ROUTINE_RECORD_POWERPC_ISA,
            ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
        };
        use crate::mixed_mode::proc_info;

        const DESCRIPTOR: u32 = 0x0301_0100;
        const TVECTOR: u32 = 0x0301_0200;
        const PPC_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        const CALLBACK_RTOC: u32 = 0x0301_0300;
        const M68K_INNER: u32 = 0x0301_0400;
        const INNER_DESCRIPTOR: u32 = 0x0301_0600;
        const M68K_STACK: u32 = 0x0302_0000;
        const INITIAL_SP: u32 = M68K_STACK + 0x80;
        const RETURN_PC: u32 = 0x0302_0100;
        const ARGUMENT: u32 = 0x10;
        const OUTER_D6: u32 = 0x1357_2468;
        const INNER_D6: u32 = 0xdead_beef;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app
            .memory
            .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);

        let inner_words = [
            0x2c3c,
            (INNER_D6 >> 16) as u16,
            INNER_D6 as u16, // MOVE.L #INNER_D6,D6
            0x202f,
            0x0004, // MOVE.L 4(SP),D0
            0x5e80, // ADDQ.L #7,D0
            0x2f40,
            0x0008, // MOVE.L D0,8(SP)
            0x4e74,
            0x0004, // RTD #4
        ];
        ppc_app.memory.add_region(
            M68K_INNER,
            inner_words.into_iter().flat_map(u16::to_be_bytes).collect(),
        );

        let proc_info = proc_info::PASCAL_STACK_BASED
            | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
            | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
        let first_branch_pc = PPC_CALLBACK + 6 * 4;
        let second_branch_pc = PPC_CALLBACK + 12 * 4;
        let callback_words = [
            0x7fe8_02a6, // MFLR R31
            0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
            0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
            0x3c80_0000 | (proc_info >> 16),
            0x6084_0000 | (proc_info & 0xffff),
            0x38a0_0000 | ARGUMENT, // LI R5,ARGUMENT
            ppc_test_relative_branch(first_branch_pc, PPC_IMPORT_TRAP_BASE) | 1, // BL CallUniversalProc
            0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
            0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
            0x3c80_0000 | (proc_info >> 16),
            0x6084_0000 | (proc_info & 0xffff),
            0x38a0_0000 | ARGUMENT, // LI R5,ARGUMENT
            ppc_test_relative_branch(second_branch_pc, PPC_IMPORT_TRAP_BASE) | 1, // BL CallUniversalProc
            0x7fe8_03a6,                                                          // MTLR R31
            0x3863_0007,                                                          // ADDI R3,R3,7
            0x4e80_0020,                                                          // BLR
        ];
        ppc_app.memory.add_region(
            PPC_CALLBACK,
            callback_words
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        ppc_app.memory.add_region(DESCRIPTOR, vec![0; 0x200]);
        ppc_app
            .memory
            .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
            .unwrap();
        ppc_app
            .memory
            .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        ppc_app.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
        let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        ppc_app.memory.write_u32_be(record, proc_info).unwrap();
        ppc_app
            .memory
            .write_u8(
                record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_POWERPC_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u16_be(
                record + ROUTINE_RECORD_FLAGS_OFFSET,
                ROUTINE_FLAG_USE_NATIVE_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR)
            .unwrap();
        ppc_app.memory.write_u32_be(TVECTOR, PPC_CALLBACK).unwrap();
        ppc_app
            .memory
            .write_u32_be(TVECTOR + 4, CALLBACK_RTOC)
            .unwrap();
        ppc_app.memory.add_region(INNER_DESCRIPTOR, vec![0; 0x100]);
        ppc_app
            .memory
            .write_u16_be(INNER_DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
            .unwrap();
        ppc_app
            .memory
            .write_u8(INNER_DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        ppc_app
            .memory
            .write_u16_be(INNER_DESCRIPTOR + 10, 0)
            .unwrap();
        let inner_record = INNER_DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        ppc_app
            .memory
            .write_u32_be(inner_record, proc_info)
            .unwrap();
        ppc_app
            .memory
            .write_u8(
                inner_record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_M68K_ISA,
            )
            .unwrap();
        ppc_app
            .memory
            .write_u32_be(
                inner_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                M68K_INNER,
            )
            .unwrap();
        ppc_app.memory.add_region(M68K_STACK, vec![0; 0x200]);
        ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
        ppc_app
            .memory
            .write_u32_be(INITIAL_SP + 4, ARGUMENT)
            .unwrap();

        let mut call_universal_proc =
            test_ppc_import_binding(0, "InterfaceLib", "CallUniversalProc");
        call_universal_proc.trap_pc = PPC_IMPORT_TRAP_BASE;
        call_universal_proc.dispatcher_target = PpcImportDispatcherTarget::CallUniversalProc;
        ppc_app.import_count = 1;
        ppc_app.imports = vec![call_universal_proc];
        let mut outer_registers = crate::guest_call::M68kRegisterState::default();
        outer_registers.data[6] = OUTER_D6;
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: DESCRIPTOR,
                rtoc: 0,
            },
            DESCRIPTOR,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 8,
            outer_registers,
            Some(crate::guest_call::M68kResultSource::Memory {
                address: INITIAL_SP + 8,
                size: 4,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let mut maximum_parked = 0;
        let mut inner_entries = 0;
        let mut checked_wrong_boundary = false;
        for iteration in 0..128 {
            let (steps, running) = runner.run_steps(1, None);
            assert!(
                running,
                "cross-ISA execution stopped at iteration {iteration} after {steps} steps: pc=${:08x} sp=${:08x} frames={} parked={} ppc_pc=${:08x}",
                runner.m68k.cpu.read_reg(Register::PC),
                runner.m68k.cpu.read_reg(Register::A7),
                runner.dispatcher.guest_calls.len(),
                runner.dispatcher.guest_calls.m68k_context_bank().borrow().len(),
                runner.native.application()
                    .map_or(0, |ppc_app| ppc_app.cpu.pc),
            );
            maximum_parked = maximum_parked.max(
                runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow()
                    .len(),
            );
            if !runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .is_empty()
                && runner.m68k.cpu.read_reg(Register::PC) == M68K_INNER + 6
            {
                inner_entries += 1;
            }
            if !checked_wrong_boundary
                && !runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow()
                    .is_empty()
            {
                let frame_count = runner.dispatcher.guest_calls.len();
                let mut native_context = runner
                    .native
                    .take(NativeEngineRole::Application)
                    .expect("PPC app");
                let mut ppc_app = native_context.adapter_mut();
                assert!(!runner.resume_m68k_after_powerpc(&mut ppc_app));
                assert_eq!(
                    runner
                        .dispatcher
                        .guest_calls
                        .m68k_context_bank()
                        .borrow()
                        .len(),
                    1
                );
                assert_eq!(ppc_app.toolbox_startup.execution.calls().len(), frame_count);
                runner
                    .native
                    .restore(native_context)
                    .unwrap_or_else(|_| panic!("native context lost its owner"));
                checked_wrong_boundary = true;
            }
            if runner.dispatcher.guest_calls.is_empty() {
                break;
            }
        }

        assert!(checked_wrong_boundary);
        assert_eq!(inner_entries, 2);
        assert_eq!(maximum_parked, 1);
        assert!(runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .is_empty());
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert_eq!(runner.m68k.cpu.core.d(6), OUTER_D6);
        let ppc_app = runner.native.application_mut().expect("PPC app retained");
        assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
        assert_eq!(ppc_app.cpu.gpr[3], ARGUMENT + 14);
        assert_eq!(
            ppc_app.memory.read_u32_be(INITIAL_SP + 8),
            Some(ARGUMENT + 14)
        );
    }

    #[test]
    fn reverse_powerpc_return_sets_only_the_selected_68k_ccr_bit() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.m68k.cpu.core.set_ccr(0x10);
        assert!(runner.dispatcher.guest_calls.begin_m68k_to_powerpc(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::PowerPc,
                entry: PPC_CODE_BASE,
                rtoc: 0,
            },
            crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
            0x0010_0000,
            0x0010_1000,
            Some(crate::guest_call::M68kResultTarget::Ccr { mask: 0x04 }),
        ));
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app");
        let mut ppc_app = native_context.adapter_mut();
        let return_pc = 0x01f0_4000;
        ppc_app.toolbox_startup.execution.calls()
            .activate_powerpc_from_m68k(&mut ppc_app.cpu, return_pc)
            .unwrap();
        ppc_app.cpu.pc = return_pc;
        ppc_app.cpu.gpr[3] = 1;
        assert!(ppc_app.toolbox_startup.execution.calls()
            .complete_powerpc_for_m68k(&mut ppc_app.cpu));

        assert!(runner.resume_m68k_after_powerpc(&mut ppc_app));

        assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x14);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x0010_0000);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_1000);
        assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
        runner
            .native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
    }

    #[test]
    fn reverse_special_case_results_restore_every_classic_output_layout() {
        use crate::mixed_mode::special_case;

        const SCRATCH: u32 = 0x0305_0000;
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app");
        let ppc_app = native_context.adapter_mut();
        ppc_app.memory.add_region(SCRATCH, vec![0; 8]);

        runner.m68k.cpu.core.set_ccr(0x13);
        for selector in [
            special_case::EOL_HOOK,
            special_case::PROTOCOL_HANDLER,
            special_case::SOCKET_LISTENER,
        ] {
            assert!(M68kExecution::apply_m68k_special_case_result(
                &mut runner.m68k.cpu,
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                SCRATCH,
                1,
            ));
            assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x17);
            assert!(M68kExecution::apply_m68k_special_case_result(
                &mut runner.m68k.cpu,
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                SCRATCH,
                0,
            ));
            assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x13);
        }

        runner.m68k.cpu.core.set_d(1, 0xaaaa_0000);
        for selector in [special_case::WIDTH_HOOK, special_case::NWIDTH_HOOK] {
            assert!(M68kExecution::apply_m68k_special_case_result(
                &mut runner.m68k.cpu,
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                SCRATCH,
                0x1234_5678,
            ));
            assert_eq!(runner.m68k.cpu.core.d(1), 0xaaaa_5678);
        }

        runner.m68k.cpu.core.set_d(1, 0xbbbb_0000);
        runner.m68k.cpu.core.set_d(2, 0xcccc_0000);
        ppc_app.memory.write_u16_be(SCRATCH, 0x1111).unwrap();
        ppc_app.memory.write_u16_be(SCRATCH + 2, 0x2222).unwrap();
        ppc_app.memory.write_u8(SCRATCH + 4, 1).unwrap();
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
            SCRATCH,
            1,
        ));
        assert_eq!(runner.m68k.cpu.core.d(0), 0x0001_1111);
        assert_eq!(runner.m68k.cpu.core.d(1), 0xbbbb_2222);
        assert_eq!(runner.m68k.cpu.core.d(2), 0xcccc_0001);

        runner.m68k.cpu.core.set_d(0, 0xaaaa_0000);
        runner.m68k.cpu.core.set_d(1, 0xbbbb_0000);
        ppc_app.memory.write_u16_be(SCRATCH, 0x3333).unwrap();
        ppc_app.memory.write_u16_be(SCRATCH + 2, 0x4444).unwrap();
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_FIND_WORD).unwrap(),
            SCRATCH,
            0,
        ));
        assert_eq!(runner.m68k.cpu.core.d(0), 0xaaaa_3333);
        assert_eq!(runner.m68k.cpu.core.d(1), 0xbbbb_4444);

        for (offset, value) in [(0, 0x5555), (2, 0x6666), (4, 0x7777)] {
            ppc_app
                .memory
                .write_u16_be(SCRATCH + offset, value)
                .unwrap();
        }
        runner.m68k.cpu.core.set_d(2, 0xaaaa_0000);
        runner.m68k.cpu.core.set_d(3, 0xbbbb_0000);
        runner.m68k.cpu.core.set_d(4, 0xcccc_0000);
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_RECALC).unwrap(),
            SCRATCH,
            0,
        ));
        assert_eq!(runner.m68k.cpu.core.d(2), 0xaaaa_5555);
        assert_eq!(runner.m68k.cpu.core.d(3), 0xbbbb_6666);
        assert_eq!(runner.m68k.cpu.core.d(4), 0xcccc_7777);

        ppc_app.memory.write_u32_be(SCRATCH, 0xcafe_babe).unwrap();
        ppc_app.memory.write_u16_be(SCRATCH + 4, 0x8888).unwrap();
        runner.m68k.cpu.core.set_d(0, 0xdddd_0000);
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_DO_TEXT).unwrap(),
            SCRATCH,
            0,
        ));
        assert_eq!(runner.m68k.cpu.core.a(0), 0xcafe_babe);
        assert_eq!(runner.m68k.cpu.core.d(0), 0xdddd_8888);

        runner.m68k.cpu.core.set_d(0, 0xeeee_0000);
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(special_case::MBAR_HOOK).unwrap(),
            SCRATCH,
            0x1234_9999,
        ));
        assert_eq!(runner.m68k.cpu.core.d(0), 0xeeee_9999);
        assert!(!M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            13,
            SCRATCH,
            0
        ));
        runner
            .native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
    }

    #[test]
    fn forward_special_case_results_restore_every_native_output_layout() {
        use crate::guest_call::PowerPcArguments;
        use crate::mixed_mode::special_case;

        const SCRATCH: u32 = 0x0306_0000;
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app");
        let ppc_app = native_context.adapter_mut();
        ppc_app.memory.add_region(SCRATCH, vec![0; 0x100]);
        let arguments = |values: &[u32]| PowerPcArguments::from_slice(values).unwrap();

        for selector in [special_case::HIGH_HOOK, special_case::DRAW_HOOK] {
            let values = vec![
                0;
                if selector == special_case::HIGH_HOOK {
                    2
                } else {
                    5
                }
            ];
            assert_eq!(
                runner.m68k.complete_m68k_special_case_result(
                    &mut ppc_app.memory,
                    u8::try_from(selector).unwrap(),
                    arguments(&values),
                    None,
                ),
                Ok(None),
            );
        }

        for selector in [
            special_case::EOL_HOOK,
            special_case::PROTOCOL_HANDLER,
            special_case::SOCKET_LISTENER,
        ] {
            let values = vec![
                0;
                match selector {
                    special_case::EOL_HOOK => 3,
                    special_case::PROTOCOL_HANDLER => 6,
                    special_case::SOCKET_LISTENER => 7,
                    _ => unreachable!(),
                }
            ];
            runner.m68k.cpu.core.set_ccr(0x04);
            assert_eq!(
                runner.m68k.complete_m68k_special_case_result(
                    &mut ppc_app.memory,
                    u8::try_from(selector).unwrap(),
                    arguments(&values),
                    None,
                ),
                Ok(Some(1)),
            );
            runner.m68k.cpu.core.set_ccr(0);
            assert_eq!(
                runner.m68k.complete_m68k_special_case_result(
                    &mut ppc_app.memory,
                    u8::try_from(selector).unwrap(),
                    arguments(&values),
                    None,
                ),
                Ok(Some(0)),
            );
        }

        runner.m68k.cpu.core.set_d(1, 0xaaaa_5678);
        for selector in [special_case::WIDTH_HOOK, special_case::NWIDTH_HOOK] {
            let values = vec![
                0;
                if selector == special_case::WIDTH_HOOK {
                    5
                } else {
                    8
                }
            ];
            assert_eq!(
                runner.m68k.complete_m68k_special_case_result(
                    &mut ppc_app.memory,
                    u8::try_from(selector).unwrap(),
                    arguments(&values),
                    None,
                ),
                Ok(Some(0x5678)),
            );
        }

        runner.m68k.cpu.core.set_d(0, 0x0001_1111);
        runner.m68k.cpu.core.set_d(1, 0xaaaa_2222);
        runner.m68k.cpu.core.set_d(2, 0xbbbb_0033);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
                arguments(&[0, 0, 0, 0, 0, 0, SCRATCH, SCRATCH + 2, SCRATCH + 4]),
                None,
            ),
            Ok(Some(1)),
        );
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH), Some(0x1111));
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 2), Some(0x2222));
        assert_eq!(ppc_app.memory.read_u8(SCRATCH + 4), Some(0x33));

        runner.m68k.cpu.core.set_d(0, 0xaaaa_4444);
        runner.m68k.cpu.core.set_d(1, 0xbbbb_5555);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::TE_FIND_WORD).unwrap(),
                arguments(&[0, 0, 0, 0, SCRATCH + 8, SCRATCH + 10]),
                None,
            ),
            Ok(None),
        );
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 8), Some(0x4444));
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 10), Some(0x5555));

        runner.m68k.cpu.core.set_d(2, 0xaaaa_6666);
        runner.m68k.cpu.core.set_d(3, 0xbbbb_7777);
        runner.m68k.cpu.core.set_d(4, 0xcccc_8888);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::TE_RECALC).unwrap(),
                arguments(&[0, 0, SCRATCH + 12, SCRATCH + 14, SCRATCH + 16]),
                None,
            ),
            Ok(None),
        );
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 12), Some(0x6666));
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 14), Some(0x7777));
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 16), Some(0x8888));

        runner.m68k.cpu.core.set_a(0, 0xcafe_babe);
        runner.m68k.cpu.core.set_d(0, 0xaaaa_9999);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::TE_DO_TEXT).unwrap(),
                arguments(&[0, 0, 0, 0, SCRATCH + 20, SCRATCH + 24]),
                None,
            ),
            Ok(None),
        );
        assert_eq!(ppc_app.memory.read_u32_be(SCRATCH + 20), Some(0xcafe_babe));
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 24), Some(0x9999));

        ppc_app.memory.write_u16_be(SCRATCH + 28, 1).unwrap();
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::GNE_FILTER_PROC).unwrap(),
                arguments(&[0, SCRATCH + 26]),
                Some(SCRATCH + 28),
            ),
            Ok(None),
        );
        assert_eq!(ppc_app.memory.read_u8(SCRATCH + 26), Some(1));

        // The callback writes multiple output locations as one ABI result.
        // A bad later destination must not leave an earlier output changed.
        ppc_app.memory.write_u16_be(SCRATCH + 30, 0xaaaa).unwrap();
        runner.m68k.cpu.core.set_d(0, 0x1111);
        runner.m68k.cpu.core.set_d(1, 0x2222);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::TE_FIND_WORD).unwrap(),
                arguments(&[0, 0, 0, 0, SCRATCH + 30, 0x0500_0000]),
                None,
            ),
            Err(()),
        );
        assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 30), Some(0xaaaa));

        runner.m68k.cpu.core.set_d(0, 0xaaaa_abcd);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::MBAR_HOOK).unwrap(),
                arguments(&[0]),
                None,
            ),
            Ok(Some(0xabcd)),
        );
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(special_case::HIGH_HOOK).unwrap(),
                arguments(&[]),
                None,
            ),
            Err(()),
        );
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                13,
                arguments(&[]),
                None,
            ),
            Err(()),
        );
        runner
            .native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
    }

    #[test]
    fn void_special_case_completion_preserves_nonzero_native_r3() {
        use crate::guest_call::{M68kResultSource, PowerPcArguments};
        use crate::mixed_mode::special_case;

        const M68K_ENTRY: u32 = 0x0306_1000;
        const INITIAL_SP: u32 = 0x0306_2000;
        const RETURN_PC: u32 = 0x0306_3000;
        const FINAL_SP: u32 = INITIAL_SP + 4;
        const NATIVE_R3: u32 = 0xCAFE_BABE;

        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app");
        let ppc_app = native_context.adapter_mut();
        ppc_app.cpu.gpr[3] = NATIVE_R3;
        let arguments = PowerPcArguments::from_slice(&[0, 0]).unwrap();
        assert!(ppc_app.toolbox_startup.execution.calls().begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            FINAL_SP,
            crate::guest_call::M68kRegisterState::default(),
            Some(M68kResultSource::SpecialCase {
                selector: u8::try_from(special_case::HIGH_HOOK).unwrap(),
                arguments,
                stack_result: None,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
        let pending = ppc_app.toolbox_startup.execution.calls().activate_m68k().unwrap();
        runner.m68k.cpu.write_reg(Register::PC, pending.return_pc);
        runner.m68k.cpu.write_reg(Register::A7, pending.final_sp);

        assert!(runner
            .process_context
            .with_memory_and_cfm(|manager, _| runner.m68k.complete_pending(
                &mut ppc_app.memory,
                &mut ppc_app.cpu,
                pending,
                manager
            )));
        assert_eq!(ppc_app.cpu.gpr[3], NATIVE_R3);
        assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
    }

    #[test]
    fn relaunch_with_pending_execution_preserves_the_existing_engine() {
        use crate::execution_kernel::ExecutionTaskState;
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let calls = runner.dispatcher.guest_calls.shared_handle();
        runner.m68k.cpu.write_reg(Register::PC, 0x1234);
        assert!(calls.begin_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: 0x1000,
                rtoc: 0,
            },
            0x2000,
            0x3000
        ));
        let rejected =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.init_app(&app)));
        assert!(rejected.is_err());
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x1234);
        assert!(!calls.is_empty());
        assert!(calls.complete_m68k(0x2002, 0x3000));
        runner.init_app(&app);
        assert!(runner.m68k.can_relaunch());
        let worker = calls
            .create_native_thread(
                crate::guest_call::NativeThreadContext {
                    context: PpcCpu::new().capture_execution_context(),
                },
                crate::guest_call::ThreadStorage {
                    result_destination: 0,
                    stack_base: 0,
                    stack_limit: 0,
                    managed_pointer: true,
                },
                true,
                |_| true,
            )
            .unwrap();
        let native_pc = runner.native.application().unwrap().cpu.pc;
        let rejected =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.init_app(&app)));
        assert!(rejected.is_err());
        assert_eq!(runner.native.application().unwrap().cpu.pc, native_pc);
        assert!(calls.scheduling_state(worker).is_some());
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        {
            let cpu = &mut runner.native.application_mut().unwrap().cpu;
            assert!(calls.yield_native_thread(cpu, worker.thread_id()).unwrap());
            assert!(calls
                .yield_native_thread(cpu, ExecutionTaskId::APPLICATION.thread_id())
                .unwrap());
            assert!(calls
                .retire_native_thread(worker, cpu, false, |_| true)
                .is_ok());
        }
        assert_eq!(calls.scheduling_state(worker), None);
        assert!(!calls.switch_to_task(worker));
        runner.set_launch_state(41, 1, u64::MAX);
        runner.init_app(&app);
        assert_eq!(
            runner.native.application().unwrap().cpu.time_base(),
            u64::MAX
        );
        {
            let cpu = &mut runner.native.application_mut().unwrap().cpu;
            assert_eq!(
                cpu.step_instruction((31 << 26) | (11 << 21) | (12 << 16) | (8 << 11) | (371 << 1)),
                ppc::PpcStepResult::Stepped
            );
            assert_eq!(cpu.gpr[11], u32::MAX);
            assert_eq!(cpu.time_base(), 0);
        }
        let mut replacement = ppc::PpcExecutionContext::fresh();
        replacement.architectural_mut().gpr[20] = 0xaabb_ccdd;
        let replacement = calls
            .create_native_thread(
                crate::guest_call::NativeThreadContext {
                    context: replacement,
                },
                crate::guest_call::ThreadStorage::default(),
                false,
                |_| true,
            )
            .unwrap();
        let cpu = &mut runner.native.application_mut().unwrap().cpu;
        assert!(calls
            .yield_native_thread(cpu, replacement.thread_id())
            .unwrap());
        assert_eq!(cpu.gpr[20], 0xaabb_ccdd);
        assert_eq!(cpu.time_base(), 0);
    }

    #[test]
    fn malformed_m68k_return_shapes_leave_registers_unchanged() {
        use crate::guest_call::{M68kResultTarget, M68kResume, PowerPcReturnState};
        let mut cpu = M68kCpu::new();
        let mut memory = PpcSectionMem::new();
        cpu.core.set_d(0, 0xabcd_1234);
        cpu.core.set_a(0, 0x1234_abcd);
        cpu.core.set_ccr(0x15);
        for target in [
            M68kResultTarget::Data { index: 8, size: 4 },
            M68kResultTarget::Address { index: 8, size: 4 },
            M68kResultTarget::Data { index: 0, size: 3 },
            M68kResultTarget::SpecialCase {
                selector: crate::mixed_mode::special_case::TE_FIND_WORD as u8,
                scratch: u32::MAX,
            },
        ] {
            assert!(!M68kExecution::apply_m68k_resume_result(
                &mut cpu,
                &mut memory,
                M68kResume {
                    return_pc: 0x1000,
                    final_sp: 0x2000,
                    result: Some(target),
                    powerpc: PowerPcReturnState { gpr3: 42 },
                }
            ));
            assert_eq!(cpu.core.d(0), 0xabcd_1234);
            assert_eq!(cpu.core.a(0), 0x1234_abcd);
            assert_eq!(cpu.core.get_ccr(), 0x15);
        }
    }

    #[test]
    fn parked_m68k_caller_receives_native_register_and_ccr_results() {
        use crate::guest_call::M68kResultTarget;
        use crate::mixed_mode::special_case;

        const RETURN_PC: u32 = 0x0306_5000;
        const FINAL_SP: u32 = 0x0306_6000;
        for target in [
            M68kResultTarget::Data { index: 2, size: 4 },
            M68kResultTarget::Address { index: 3, size: 4 },
            M68kResultTarget::Ccr { mask: 4 },
            M68kResultTarget::SpecialCase {
                selector: special_case::WIDTH_HOOK as u8,
                scratch: 0,
            },
        ] {
            let app = halted_ppc_app_with_sound(PpcSoundState::default());
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.init_app(&app);
            let mut native_context = runner.native.take(NativeEngineRole::Application).unwrap();
            let mut ppc_app = native_context.adapter_mut();
            assert!(ppc_app.toolbox_startup.execution.calls().begin_m68k_to_powerpc(
                crate::guest_call::GuestCallTarget {
                    isa: crate::guest_procedure::GuestIsa::PowerPc,
                    entry: PPC_CODE_BASE,
                    rtoc: 0,
                },
                crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
                RETURN_PC,
                FINAL_SP,
                Some(target),
            ));
            ppc_app.toolbox_startup.execution.calls()
                .activate_powerpc_from_m68k(&mut ppc_app.cpu, RETURN_PC)
                .unwrap();
            ppc_app.cpu.pc = RETURN_PC;
            ppc_app.cpu.gpr[3] = 0x1234_5678;
            assert!(ppc_app.toolbox_startup.execution.calls()
                .complete_powerpc_for_m68k(&mut ppc_app.cpu));
            let (task, call_id) = ppc_app.toolbox_startup.execution.calls().pending_m68k_resume_owner().unwrap();
            runner.m68k.cpu.core.set_d(1, 0xabcd_0000);
            runner.m68k.cpu.core.set_d(7, 0xcafe_babe);
            runner.m68k.cpu.core.set_ccr(0x11);
            assert!(ppc_app.guest_calls()
                .park_context(
                    &mut runner
                        .dispatcher
                        .guest_calls
                        .m68k_context_bank()
                        .borrow_mut(),
                    task,
                    call_id,
                    std::mem::take(&mut runner.m68k.cpu),
                )
                .is_ok());
            assert!(runner.resume_m68k_after_powerpc(&mut ppc_app), "{target:?}");
            match target {
                M68kResultTarget::Data { .. } => assert_eq!(runner.m68k.cpu.core.d(2), 0x1234_5678),
                M68kResultTarget::Address { .. } => {
                    assert_eq!(runner.m68k.cpu.core.a(3), 0x1234_5678)
                }
                M68kResultTarget::Ccr { .. } => assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x15),
                M68kResultTarget::SpecialCase { .. } => {
                    assert_eq!(runner.m68k.cpu.core.d(1), 0xabcd_5678)
                }
                _ => unreachable!(),
            }
            assert_eq!(runner.m68k.cpu.core.d(7), 0xcafe_babe);
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), RETURN_PC);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), FINAL_SP);
            assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
            assert!(runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .is_empty());
        }
    }

    #[test]
    fn failed_m68k_result_application_keeps_resume_and_parked_context_retryable() {
        use crate::guest_call::M68kResultTarget;

        const RESULT: u32 = 0x0306_4000;
        const RETURN_PC: u32 = 0x0306_5000;
        const FINAL_SP: u32 = 0x0306_6000;
        const RESULT_VALUE: u32 = 0x1234_5678;
        const PARKED_D0: u32 = 0xA11C_E001;

        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app");
        let mut ppc_app = native_context.adapter_mut();
        assert!(ppc_app.toolbox_startup.execution.calls().begin_m68k_to_powerpc(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::PowerPc,
                entry: PPC_CODE_BASE,
                rtoc: 0,
            },
            crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
            RETURN_PC,
            FINAL_SP,
            Some(M68kResultTarget::Memory {
                address: RESULT,
                size: 4,
            }),
        ));
        ppc_app.toolbox_startup.execution.calls()
            .activate_powerpc_from_m68k(&mut ppc_app.cpu, RETURN_PC)
            .unwrap();
        ppc_app.cpu.pc = RETURN_PC;
        ppc_app.cpu.gpr[3] = RESULT_VALUE;
        assert!(ppc_app.toolbox_startup.execution.calls()
            .complete_powerpc_for_m68k(&mut ppc_app.cpu));

        // Model the caller parked by a nested native-to-68K transition. The
        // failed write must not consume either this context or its completion.
        let (task, call_id) = ppc_app.guest_calls()
            .pending_m68k_resume_owner()
            .expect("completed continuation owner");
        runner.m68k.cpu.core.set_d(0, PARKED_D0);
        assert!(ppc_app.guest_calls()
            .park_context(
                &mut runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow_mut(),
                task,
                call_id,
                std::mem::take(&mut runner.m68k.cpu),
            )
            .is_ok());
        assert_eq!(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .task_len(task),
            1
        );
        assert!(ppc_app.toolbox_startup.execution.calls().peek_m68k_resume().is_some());

        assert!(!runner.resume_m68k_after_powerpc(&mut ppc_app));
        assert_eq!(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .task_len(task),
            1
        );
        assert!(ppc_app.toolbox_startup.execution.calls().peek_m68k_resume().is_some());

        ppc_app.memory.add_readonly_region(RESULT, vec![0xaa; 4]);
        assert!(!runner.resume_m68k_after_powerpc(&mut ppc_app));
        assert_eq!(ppc_app.memory.read_u32_be(RESULT), Some(0xaaaa_aaaa));
        assert_eq!(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .task_len(task),
            1
        );
        assert!(ppc_app.toolbox_startup.execution.calls().peek_m68k_resume().is_some());

        ppc_app.memory.add_region(RESULT, vec![0; 4]);
        assert!(runner.resume_m68k_after_powerpc(&mut ppc_app));
        assert_eq!(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .task_len(task),
            0
        );
        assert!(ppc_app.toolbox_startup.execution.calls().peek_m68k_resume().is_none());
        assert_eq!(ppc_app.memory.read_u32_be(RESULT), Some(RESULT_VALUE));
        assert_eq!(runner.m68k.cpu.core.d(0), PARKED_D0);
    }

    #[test]
    fn ppc_exit_to_shell_stops_before_tick_and_callback_phase() {
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        let mut sound = PpcSoundState::default();
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, CALLBACK);
        let mut app = halted_ppc_app_with_sound(sound);
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app
            .memory
            .add_region(CALLBACK, 0x4e80_0020u32.to_be_bytes().to_vec());
        ppc_app
            .memory
            .write_u32_be(
                PPC_CODE_BASE,
                ppc_test_relative_branch(PPC_CODE_BASE, PPC_IMPORT_TRAP_BASE),
            )
            .unwrap();
        let mut exit = test_ppc_import_binding(0, "InterfaceLib", "ExitToShell");
        exit.trap_pc = PPC_IMPORT_TRAP_BASE;
        exit.dispatcher_target = PpcImportDispatcherTarget::ExitToShell;
        ppc_app.import_count = 1;
        ppc_app.imports = vec![exit];
        ppc_app.vbl_tasks.push(PpcVblTaskRecord {
            task_ptr: PPC_DATA_BASE + 0x3000,
            architecture: CallbackTaskArchitecture::PowerPc,
            slot: None,
            pending: false,
        });
        ppc_app.timer_tasks.push(PpcTimerTaskRecord {
            task_ptr: PPC_DATA_BASE + 0x3100,
            architecture: CallbackTaskArchitecture::PowerPc,
            extended: false,
            callback: CALLBACK,
            active: true,
            fire_at_tick: 0,
            fire_at_subtick: 0,
            last_fired_tick: None,
        });

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.set_instructions_per_tick(1_000_000);
        runner.tick_budget = 1_000_000;
        let initial_tick = runner.guest_tick();
        let initial_screen_events = runner.dispatcher.screen_event_count;

        let (_steps, running) = runner.run_steps(64, None);

        assert!(!running);
        assert!(runner.halted_by_exit_to_shell());
        assert_eq!(runner.guest_tick(), initial_tick);
        assert_eq!(runner.guest_tick(), initial_tick);
        assert_eq!(runner.dispatcher.screen_event_count, initial_screen_events);
        let ppc_app = runner.native.application().expect("PPC app retained");
        assert_eq!(ppc_app.vbl_tasks.len(), 1);
        assert_eq!(ppc_app.timer_tasks[0].last_fired_tick, None);
        assert_eq!(ppc_app.sound.manager.pending_sound_callbacks.len(), 1);
        assert!(ppc_app.sound.completion_invocations.is_empty());
    }

    const PPC_SOUND_EVENT_RECORD: u32 = PPC_DATA_BASE + 0x2000;
    const PPC_SOUND_GET_EVENT_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const PPC_SOUND_POST_EVENT_CALLBACK: u32 = PPC_CODE_BASE + 0x1100;
    const PPC_SOUND_SET_EVENT_MASK_CALLBACK: u32 = PPC_CODE_BASE + 0x1200;

    fn ppc_test_relative_branch(from: u32, to: u32) -> u32 {
        0x4800_0000 | (to.wrapping_sub(from) & 0x03ff_fffc)
    }

    fn install_ppc_sound_event_boundary_fixture(app: &mut LoadedApp) {
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        let mut add_callback = |address: u32, mut words: Vec<u32>, import_index: u32| {
            let branch_address = address + u32::try_from(words.len()).unwrap() * 4;
            words.push(ppc_test_relative_branch(
                branch_address,
                PPC_IMPORT_TRAP_BASE + import_index * 4,
            ));
            ppc_app.memory.add_region(
                address,
                words.into_iter().flat_map(u32::to_be_bytes).collect(),
            );
        };
        add_callback(
            PPC_SOUND_GET_EVENT_CALLBACK,
            vec![
                0x3860_ffff, // li r3,-1
                0x3c80_0200, // lis r4,$0200
                0x6084_2000, // ori r4,r4,$2000
            ],
            0,
        );
        add_callback(
            PPC_SOUND_POST_EVENT_CALLBACK,
            vec![
                0x3860_0005, // li r3,5
                0x3c80_5566, // lis r4,$5566
                0x6084_7788, // ori r4,r4,$7788
            ],
            1,
        );
        add_callback(
            PPC_SOUND_SET_EVENT_MASK_CALLBACK,
            vec![0x3860_1234], // li r3,$1234
            2,
        );
        ppc_app
            .memory
            .add_region(PPC_SOUND_EVENT_RECORD, vec![0; 16]);
        ppc_app.import_count = 3;
        ppc_app.imports = [
            (
                "GetNextEvent",
                PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
            ),
            ("PostEvent", PpcImportDispatcherTarget::PostEvent),
            ("SetEventMask", PpcImportDispatcherTarget::SetEventMask),
        ]
        .into_iter()
        .enumerate()
        .map(|(index, (symbol, dispatcher_target))| {
            let index = u32::try_from(index).unwrap();
            PpcImportBinding {
                library_index: 0,
                symbol_index: index,
                library_name: "InterfaceLib".into(),
                symbol_name: symbol.into(),
                class: 0,
                weak: false,
                address: PPC_IMPORT_TRAP_BASE + index * 4,
                tvector_address: None,
                trap_pc: PPC_IMPORT_TRAP_BASE + index * 4,
                dispatcher_target,
            }
        })
        .collect();
        ppc_app.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: 0,
            gdevice: PPC_MAIN_GDEVICE,
            width: 512,
            height: 342,
            depth: 8,
            row_bytes: 512,
            pixels_locked: false,
            pixels_no_purge: false,
        });
        ppc_app.set_input_snapshot(PpcInputSnapshot {
            mouse_v: 17,
            mouse_h: 19,
            ..PpcInputSnapshot::default()
        });
    }

    fn assert_ppc_sound_event_boundary(
        mut app: LoadedApp,
        fire_callbacks: impl FnOnce(&mut FixtureRunner),
    ) {
        install_ppc_sound_event_boundary_fixture(&mut app);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let (offset_v, offset_h) = runner.ppc_viewport_offset();
        assert!(
            offset_v > 0 && offset_h > 0,
            "fixture must use a centered viewport"
        );
        runner
            .process_context
            .shared_event_queue()
            .push_back(QueuedEvent {
                what: 3,
                message: 0x1122_3344,
                when: 0,
                where_v: 120,
                where_h: 180,
                modifiers: 0x0080,
            });

        fire_callbacks(&mut runner);

        let ppc_app = runner.native.application_mut().expect("PPC app");
        assert_eq!(
            ppc_app.memory.read_u16_be(PPC_SOUND_EVENT_RECORD + 10),
            Some(120)
        );
        assert_eq!(
            ppc_app.memory.read_u16_be(PPC_SOUND_EVENT_RECORD + 12),
            Some(180)
        );
        assert_eq!(runner.process_context.event_queue().len(), 1);
        let posted = &runner.process_context.event_queue()[0];
        assert_eq!((posted.what, posted.message), (5, 0x5566_7788));
        assert_eq!((posted.where_v, posted.where_h), (17, 19));
        assert_eq!(
            runner
                .bus
                .read_word(crate::memory::globals::addr::SYS_EVT_MASK),
            0x1234
        );
        assert_eq!(
            ppc_app
                .memory
                .read_u16_be(crate::memory::globals::addr::SYS_EVT_MASK),
            Some(0x1234)
        );
    }

    #[test]
    fn ppc_sound_doublebacks_preserve_centered_viewport_event_coordinates() {
        let callback = |callback| PpcSoundDoubleBackRecord {
            architecture: CallbackTaskArchitecture::PowerPc,
            channel: 0x0300_1000,
            header: 0x0300_2000,
            exhausted_buffer: 0x0300_3000,
            exhausted_buffer_index: 0,
            callback,
            tick: 0,
            instruction_count: 0,
        };
        let sound = PpcSoundState::default();
        sound.manager.replace_pending_process_doublebacks(vec![
            callback(PPC_SOUND_GET_EVENT_CALLBACK),
            callback(PPC_SOUND_POST_EVENT_CALLBACK),
            callback(PPC_SOUND_SET_EVENT_MASK_CALLBACK),
        ]);
        let app = halted_ppc_app_with_sound(sound);

        assert_ppc_sound_event_boundary(app, FixtureRunner::fire_pending_ppc_sound_doublebacks);
    }

    #[test]
    fn ppc_sound_completions_preserve_centered_viewport_event_coordinates() {
        let mut sound = PpcSoundState::default();
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, PPC_SOUND_GET_EVENT_CALLBACK);
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, PPC_SOUND_POST_EVENT_CALLBACK);
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, PPC_SOUND_SET_EVENT_MASK_CALLBACK);
        let app = halted_ppc_app_with_sound(sound);

        assert_ppc_sound_event_boundary(app, FixtureRunner::fire_pending_ppc_sound_completions);
    }

    #[test]
    fn ppc_host_mouse_input_enters_the_shared_queue_in_global_coordinates() {
        use crate::memory::globals::addr;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        install_ppc_sound_event_boundary_fixture(&mut app);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let (offset_v, offset_h) = runner.ppc_viewport_offset();
        assert!(offset_v > 0 && offset_h > 0);

        runner.push_mouse_down(offset_v.saturating_add(12), offset_h.saturating_add(34));

        let event = runner
            .process_context
            .event_queue()
            .back()
            .expect("mouseDown");
        assert_eq!((event.where_v, event.where_h), (12, 34));
        let ppc_app = runner.native.application().expect("PPC app");
        let native_event = ppc_app.event_queue.back().expect("shared mouseDown");
        assert_eq!((native_event.where_v, native_event.where_h), (12, 34));
        assert_eq!(runner.bus.read_word(addr::M_TEMP), 12);
        assert_eq!(runner.bus.read_word(addr::M_TEMP + 2), 34);
    }

    #[test]
    fn ppc_event_queue_has_immediate_bidirectional_visibility() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        // Host input pushes directly into process context canonical queue
        runner.push_mouse_down(10, 20);
        assert_eq!(runner.process_context.event_queue().len(), 1);
        assert_eq!(
            (
                runner
                    .process_context
                    .event_queue()
                    .front()
                    .unwrap()
                    .where_v,
                runner
                    .process_context
                    .event_queue()
                    .front()
                    .unwrap()
                    .where_h
            ),
            (10, 20)
        );

        // The attached PPC adapter observes and mutates the canonical queue directly.
        {
            let app = runner.native.application_mut().unwrap();
            assert_eq!(app.event_queue.len(), 1);
            let event = app.event_queue.pop_front().unwrap();
            assert_eq!((event.where_v, event.where_h), (10, 20));
            app.event_queue.push_back(QueuedEvent {
                what: 2, // mouseUp
                message: 0,
                when: 0,
                where_v: 30,
                where_h: 40,
                modifiers: 0,
            });
        }

        // The mutation is immediately visible in ProcessContext without a sync copy.
        assert_eq!(runner.process_context.event_queue().len(), 1);
        assert_eq!(
            runner.process_context.event_queue().front().unwrap().what,
            2
        );

        // The attached 68K dispatcher observes the same queue continuously.
        assert_eq!(runner.dispatcher.event_queue.len(), 1);
        let event = runner.dispatcher.event_queue.pop_front().unwrap();
        assert_eq!((event.where_v, event.where_h), (30, 40));

        assert!(runner.process_context.event_queue().is_empty());
    }

    #[test]
    fn ppc_sound_doubleback_can_complete_a_large_refill() {
        const CALLBACK_CYCLES: usize = 600_000;
        const CHANNEL: u32 = 0x0300_1000;
        const HEADER: u32 = 0x0300_2000;
        const BUFFER: u32 = 0x0300_3000;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;

        let sound = PpcSoundState::default();
        sound.manager.replace_pending_process_doublebacks(vec![PpcSoundDoubleBackRecord {
            architecture: CallbackTaskArchitecture::PowerPc,
            channel: CHANNEL,
            header: HEADER,
            exhausted_buffer: BUFFER,
            exhausted_buffer_index: 0,
            callback: CALLBACK,
            tick: 1,
            instruction_count: 1,
        }]);
        let mut app = halted_ppc_app_with_sound(sound);
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        let mut callback = Vec::with_capacity((CALLBACK_CYCLES + 1) * 4);
        for _ in 0..CALLBACK_CYCLES {
            callback.extend_from_slice(&0x6000_0000u32.to_be_bytes()); // nop
        }
        callback.extend_from_slice(&0x4e80_0020u32.to_be_bytes()); // blr
        ppc_app.memory.add_region(CALLBACK, callback);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.fire_pending_ppc_sound_doublebacks();

        assert!(!runner.is_halted());
        let sound = &runner.native.application().expect("PPC app").sound;
        assert!(sound.manager.pending_process_doublebacks.is_empty());
        let invocation = sound
            .completion_invocations
            .last()
            .expect("doubleback invocation");
        assert!(invocation.cycles > 250_000);
        assert_eq!(
            invocation.result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                cycles: (CALLBACK_CYCLES + 1) as u64,
            }
        );
    }

    #[test]
    fn ppc_sound_doubleback_runaway_still_stops_at_the_watchdog() {
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        let sound = PpcSoundState::default();
        sound
            .manager
            .replace_pending_process_doublebacks(vec![PpcSoundDoubleBackRecord {
                architecture: CallbackTaskArchitecture::PowerPc,
                channel: 0x0300_1000,
                header: 0x0300_2000,
                exhausted_buffer: 0x0300_3000,
                exhausted_buffer_index: 0,
                callback: CALLBACK,
                tick: 1,
                instruction_count: 1,
            }]);
        let mut app = halted_ppc_app_with_sound(sound);
        app.ppc.as_mut().unwrap().memory.add_region(
            CALLBACK,
            0x4800_0000u32.to_be_bytes().to_vec(), // b .
        );
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.fire_pending_ppc_sound_doublebacks();

        assert!(runner.is_halted());
        assert_eq!(runner.halted_pc(), Some(CALLBACK));
        let invocation = runner
            .native
            .application()
            .unwrap()
            .sound
            .completion_invocations
            .last()
            .unwrap();
        assert!(matches!(invocation.result, PpcRunResult::CycleLimit { cycles } if cycles > 0));
    }

    #[test]
    fn ppc_sound_doubleback_callback_refills_and_plays_exhausted_buffer() {
        const CHANNEL: u32 = 0x0300_1000;
        const HEADER: u32 = 0x0300_2000;
        const BUFFER: u32 = 0x0300_3000;
        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        const SAMPLES: [u8; 4] = [0x80, 0x90, 0x70, 0xa0];

        let sound = PpcSoundState::default();
        sound.manager.replace_double_buffer_playbacks(vec![PpcSoundDoubleBufferPlaybackRecord {
            channel: CHANNEL,
            header: HEADER,
            buffers: [BUFFER, 0],
            callback: CALLBACK,
            callback_architecture: CallbackTaskArchitecture::PowerPc,
            sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            compression_id: 0,
            packet_size: 0,
            current_buffer_index: 0,
            callback_pending_mask: 0,
            active: true,
            host_initialized: false,
            host_buffer_loaded: false,
        }]);
        let mut app = halted_ppc_app_with_sound(sound);
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.memory.add_region(BUFFER, vec![0; 32]);

        // Inside Macintosh: Sound (1994), pp. 2-147 and 2-178: a doubleback
        // receives the exhausted SndDoubleBufferPtr and refills dbNumFrames,
        // dbFlags, and dbSoundData before setting dbBufferReady.
        let callback = [
            0x38a0_0004u32, // li r5,4
            0x90a4_0000,    // stw r5,0(r4): dbNumFrames
            0x3ca0_8090,    // lis r5,$8090
            0x60a5_70a0,    // ori r5,r5,$70A0
            0x90a4_0010,    // stw r5,16(r4): dbSoundData
            0x38a0_0005,    // li r5,5: dbBufferReady | dbLastBuffer
            0x90a4_0004,    // stw r5,4(r4): dbFlags
            0x4e80_0020,    // blr
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect::<Vec<_>>();
        ppc_app.memory.add_region(CALLBACK, callback);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.mix_audio(SAMPLES.len());

        assert_eq!(runner.drain_audio(), SAMPLES);
        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        let mut memory = ppc_app.memory.clone();
        assert_eq!(memory.read_u32_be(BUFFER), Some(SAMPLES.len() as u32));
        assert_eq!(memory.read_u32_be(BUFFER + 4), Some(0x04));
        assert_eq!(
            memory.read_u32_be(BUFFER + 16),
            Some(u32::from_be_bytes(SAMPLES))
        );
        assert_eq!(ppc_app.sound.completion_invocations.len(), 1);
        assert!(!ppc_app.sound.manager.double_buffer_playbacks[0].active);
        assert_eq!(
            runner.dispatcher().sound_manager.debug_samples_mixed,
            SAMPLES.len() as u64
        );
    }

    #[test]
    #[cfg(feature = "debug")]
    fn debugger_pause_defers_gui_ppc_sound_completion_until_resume() {
        use crate::debug::{handle_debug_request, DebugRequest};

        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        let mut sound = PpcSoundState::default();
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, CALLBACK);
        let mut app = halted_ppc_app_with_sound(sound);
        app.ppc.as_mut().unwrap().memory.add_region(
            CALLBACK,
            [0x3860_002au32, 0x4e80_0020] // li r3,42; blr
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        handle_debug_request(&mut runner, DebugRequest::Pause).unwrap();
        let tick = runner.guest_tick();
        let instructions = runner.total_instructions();
        for _ in 0..2 {
            assert_eq!(runner.run_gui_pending_sound_work(100), (0, true));
        }
        let native = runner.native.application().unwrap();
        assert_eq!(native.sound.manager.pending_sound_callbacks.len(), 1);
        assert!(native.sound.completion_invocations.is_empty());
        assert_eq!(runner.guest_tick(), tick);
        assert_eq!(runner.total_instructions(), instructions);

        handle_debug_request(&mut runner, DebugRequest::Resume).unwrap();
        runner.run_gui_pending_sound_work(100);
        let native = runner.native.application().unwrap();
        assert!(native.sound.manager.pending_sound_callbacks.is_empty());
        assert_eq!(native.sound.completion_invocations.len(), 1);
        assert_eq!(native.sound.completion_invocations[0].end_r3, 42);
        assert!(runner.total_instructions() > instructions);
    }

    #[test]
    fn ppc_sound_completion_preserves_callback_rnd_seed_update() {
        use crate::memory::globals::addr;

        const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        let mut sound = PpcSoundState::default();
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, CALLBACK);
        let mut app = halted_ppc_app_with_sound(sound);
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        ppc_app.memory.add_region(
            CALLBACK,
            [
                0x3c60_89abu32, // lis r3,$89ab
                0x6063_cdef,    // ori r3,r3,$cdef
                0x3880_0156,    // li r4,$0156
                0x9064_0000,    // stw r3,0(r4)
                0x4e80_0020,    // blr
            ]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
        );

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_optional_launch_state(None, Some(1), None);
        runner.init_app(&app);
        runner.fire_pending_ppc_sound_completions();

        assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89ab_cdef);
        assert_eq!(
            runner
                .native
                .application_mut()
                .expect("PPC app")
                .memory
                .read_u32_be(addr::RND_SEED),
            Some(0x89ab_cdef)
        );
    }

    #[test]
    fn ppc_sound_completions_see_ticks_advanced_by_prior_callback() {
        use crate::memory::globals::addr;

        const FIRST_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
        const SECOND_CALLBACK: u32 = PPC_CODE_BASE + 0x1100;
        let mut sound = PpcSoundState::default();
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, FIRST_CALLBACK);
        queue_ppc_sound_completion(&mut sound, 0x0300_1000, SECOND_CALLBACK);
        let mut app = halted_ppc_app_with_sound(sound);
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
        ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        ppc_app.memory.add_region(
            FIRST_CALLBACK,
            [0x6000_0000u32, 0x4e80_0020]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        ppc_app.memory.add_region(
            SECOND_CALLBACK,
            [0x8060_016au32, 0x4e80_0020] // lwz r3,$016a(0); blr
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_optional_launch_state(Some(41), None, None);
        runner.init_app(&app);
        runner.set_instructions_per_tick(2);
        runner.fire_pending_ppc_sound_completions();

        assert_eq!(runner.bus.read_long(addr::TICKS), 43);
        assert_eq!(runner.guest_tick(), 43);
        let ppc_app = runner.native.application_mut().expect("PPC app");
        assert_eq!(
            ppc_app
                .memory
                .read_u32_be(crate::memory::globals::addr::TICKS),
            Some(43)
        );
        assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(43));
        assert_eq!(ppc_app.sound.completion_invocations.len(), 2);
        assert_eq!(ppc_app.sound.completion_invocations[1].end_r3, 42);
    }

    #[test]
    fn ppc_loaded_app_runs_through_fixture_runner() {
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_CODE_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
        let mut cpu = PpcCpu::new();
        cpu.pc = PPC_CODE_BASE;
        cpu.lr = PPC_HALT_PC;
        cpu.gpr[1] = PPC_STACK_TOP - 64;

        let app = LoadedApp::from_ppc(PpcLoadedApp {
            cpu,
            memory,
            entry_pc: PPC_CODE_BASE,
            rtoc: 0,
            stack_base: PPC_STACK_BASE,
            stack_size: PPC_STACK_SIZE,
            stack_pointer: PPC_STACK_TOP - 64,
            tick_state: SharedProcessTickState::default(),
            clock_cycles_per_tick: 1,
            clock_cycle_phase: 0,
            native_exception_handler: 0,
            native_exception_stack: Vec::new(),
            stdc_qsort_stack: Vec::new(),
            dialog_callback_stack: Vec::new(),
            apple_events: Default::default(),
            cfm: Some(crate::cfm::CfmState::default()),
            controls: Default::default(),
            screen_clut: SharedProcessValue::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
            display_gamma: SharedProcessDisplayGamma::default(),
            process_quickdraw_port_state_attached: false,
            color_manager_clut: SharedProcessValue::from_value(
                TrapDispatcher::standard_mac_8bpp_clut(),
            ),
            aliases: Vec::new(),
            gworlds: Vec::new(),
            gworld_pixel_states: crate::process_context::SharedProcessQuickDrawPixelStates::default(
            ),
            q3_objects: Vec::new(),
            q3_object_refs: Vec::new(),
            next_q3_object: 0,
            q3_error_state: Default::default(),
            q3_lifecycle: Default::default(),
            q3_memory_storages: Vec::new(),
            q3_files: Vec::new(),
            q3_group_memberships: Vec::new(),
            q3_file_groups: Vec::new(),
            q3_views: Vec::new(),
            q3_submissions: Vec::new(),
            q3_view_transforms: Vec::new(),
            q3_submission_transforms: Vec::new(),
            q3_view_materials: Vec::new(),
            q3_submission_materials: Vec::new(),
            q3_submission_lights: Vec::new(),
            q3_view_state_stack: Vec::new(),
            q3_completed_frames: Vec::new(),
            q3_retained_frames: Vec::new(),
            q3_state_only_completed_frame_batches: Vec::new(),
            q3_fog_styles: Vec::new(),
            q3_attributes: Vec::new(),
            q3_shader_uv_transforms: Vec::new(),
            q3_shader_boundaries: Vec::new(),
            q3_mipmap_textures: Vec::new(),
            q3_texture_shaders: Vec::new(),
            q3_renderer_preferences: Vec::new(),
            q3_draw_contexts: Vec::new(),
            q3_trimeshes: Vec::new(),
            q3_styles: Vec::new(),
            q3_cameras: Vec::new(),
            q3_lights: Vec::new(),
            input_sprocket: Default::default(),
            input_sprocket_virtual_elements: Vec::new(),
            toolbox_startup: Default::default(),
            quicktime: Default::default(),
            sound: Default::default(),
            timer_tasks: Default::default(),
            vbl_tasks: Default::default(),
            callback_scheduling: Default::default(),
            process_file_system: SharedProcessFileSystem::from_state(
                ProcessFileSystemState {
                    files: Default::default(),
                    stdio_streams: ppc_initial_stdio_streams(),
                    vfs_volumes: crate::process_context::SharedProcessValue::default(),
                    vfs_directories: crate::process_context::SharedProcessValue::from_value(vec![
                        PpcVfsDirectory {
                            dir_id: 18,
                            parent_dir_id: 17,
                            path: "System Folder/Preferences/Test App Saves".to_string(),
                            creator: u32::from_be_bytes(*b"Nano"),
                            file_type: u32::from_be_bytes(*b"dir "),
                            finder_flags: 0x0080,
                            dirty: true,
                        },
                    ]),
                    next_vfs_dir_id: crate::process_context::SharedProcessValue::from_value(18),
                    default_dir_id: crate::process_context::SharedProcessValue::from_value(2),
                    vfs_files: vec![PpcVfsFileRecord {
                        path: "System Folder/Preferences/Test App Prefs".to_string(),
                        data: (b"prefs".to_vec()).into(),
                        creator: u32::from_be_bytes(*b"Nano"),
                        file_type: u32::from_be_bytes(*b"pref"),
                        finder_flags: 0x0200,
                        dirty: true,
                    }]
                    .into(),
                    deleted_vfs_file_paths: vec!["System Folder/Preferences/Old Prefs".to_string()],
                    resource_manager: Default::default(),
                    next_file_ref_num: 128,
                    ..ProcessFileSystemState::default()
                }
                .with_resources(
                    Vec::new(),
                    vec![PpcVfsResourceFileRecord {
                        path: "System Folder/Preferences/Test App HighScores".to_string(),
                        creator: u32::from_be_bytes(*b"Nano"),
                        file_type: u32::from_be_bytes(*b"pref"),
                        finder_flags: 0x0400,
                        resource_len: 0,
                        raw_data: None,
                        map_attrs: 0,
                        dirty: true,
                    }],
                    vec![PpcVfsResourceRecord {
                        ref_num: 128,
                        path: "System Folder/Preferences/Test App HighScores".to_string(),
                        res_type: u32::from_be_bytes(*b"pref"),
                        res_id: 200,
                        name: b"Scores".to_vec(),
                        data: b"score".to_vec(),
                        raw_data: None,
                        raw_attrs: None,
                        attrs: 0,
                        handle: 0,
                    }],
                ),
            ),
            current_gworld: SharedProcessValue::from_value(PPC_MAIN_GWORLD),
            current_gdevice: SharedProcessValue::from_value(PPC_MAIN_GDEVICE),
            quickdraw_op_colors: SharedProcessValue::default(),
            quickdraw_hilite_colors: SharedProcessValue::default(),
            quickdraw_fore_color: PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            },
            quickdraw_fore_indices: Default::default(),
            quickdraw_back_color: PpcRgbColor {
                red: 0xffff,
                green: 0xffff,
                blue: 0xffff,
            },
            quickdraw_pen_h: 0,
            quickdraw_pen_v: 0,
            quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
            quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
            cursor_state: crate::process_context::SharedProcessCursorState::default(),
            param_text: Default::default(),
            scrap: Default::default(),
            list_manager: Default::default(),
            halt_pc: PPC_HALT_PC,
            import_trap_base: PPC_IMPORT_TRAP_BASE,
            import_count: 0,
            imports: Vec::new(),
            section_bases: Vec::new(),
            input: PpcInputSnapshot::default(),
            process_input: Default::default(),
            event_queue: Default::default(),
            window_list: Default::default(),
            process_memory_manager: PpcProcessMemoryManager::with_heap(
                PPC_HEAP_BASE,
                PPC_STACK_BASE,
            ),
            draw_sprocket: PpcDrawSprocketState::default(),
        });
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let output_dir = tempfile::tempdir().unwrap();
        let old_host_path = output_dir
            .path()
            .join("System Folder/Preferences/Old Prefs");
        std::fs::create_dir_all(old_host_path.parent().unwrap()).unwrap();
        std::fs::write(&old_host_path, b"old").unwrap();
        let old_rsrc_path = output_dir
            .path()
            .join("System Folder/Preferences/.rsrc/Old Prefs");
        std::fs::create_dir_all(old_rsrc_path.parent().unwrap()).unwrap();
        std::fs::write(&old_rsrc_path, b"old-rsrc").unwrap();
        runner.dispatcher_mut().output_dir = Some(output_dir.path().to_path_buf());
        runner.dispatcher_mut().vfs.insert(
            "System Folder/Preferences/Old Prefs".to_string(),
            b"old".to_vec(),
        );
        runner.dispatcher_mut().vfs_rsrc.insert(
            "System Folder/Preferences/Old Prefs".to_string(),
            b"old-rsrc".to_vec(),
        );
        runner.dispatcher_mut().set_vfs_entry_finfo(
            "System Folder/Preferences/Old Prefs",
            u32::from_be_bytes(*b"pref"),
            u32::from_be_bytes(*b"Nano"),
            0x4000,
        );
        runner
            .dispatcher_mut()
            .locked_files
            .insert("System Folder/Preferences/Old Prefs".to_string());

        let (steps, running) = runner.run_steps(8, None);

        assert_eq!(steps, 1);
        assert!(!running);
        assert!(runner.is_halted());
        assert_eq!(runner.halted_pc(), Some(PPC_HALT_PC));
        assert_eq!(runner.halted_sp(), Some(PPC_STACK_TOP - 64));
        assert_eq!(
            runner
                .dispatcher()
                .vfs
                .get("System Folder/Preferences/Test App Prefs")
                .map(Vec::as_slice),
            Some(b"prefs".as_slice())
        );
        let prefs_metadata = runner
            .dispatcher()
            .vfs_metadata
            .get("System Folder/Preferences/Test App Prefs")
            .copied()
            .expect("dirty PPC data fork should carry Finder metadata");
        assert_eq!(prefs_metadata.creator, u32::from_be_bytes(*b"Nano"));
        assert_eq!(prefs_metadata.file_type, u32::from_be_bytes(*b"pref"));
        assert_eq!(prefs_metadata.finder_flags, 0x0200);
        assert!(!runner
            .dispatcher()
            .vfs
            .contains_key("System Folder/Preferences/Old Prefs"));
        assert!(!runner
            .dispatcher()
            .vfs_rsrc
            .contains_key("System Folder/Preferences/Old Prefs"));
        assert!(!runner
            .dispatcher()
            .vfs_metadata
            .contains_key("System Folder/Preferences/Old Prefs"));
        assert!(!runner
            .dispatcher()
            .locked_files
            .contains("System Folder/Preferences/Old Prefs"));
        assert_eq!(
            std::fs::read(
                output_dir
                    .path()
                    .join("System Folder/Preferences/Test App Prefs")
            )
            .unwrap()
            .as_slice(),
            b"prefs".as_slice()
        );
        assert!(!old_host_path.exists());
        assert!(!old_rsrc_path.exists());
        let fork_bytes = runner
            .dispatcher()
            .vfs_rsrc
            .get("System Folder/Preferences/Test App HighScores")
            .expect("dirty PPC resource fork should sync to dispatcher VFS");
        let fork = ResourceFork::parse(fork_bytes).unwrap();
        assert_eq!(fork.get(*b"pref", 200).unwrap().data, b"score");
        let scores_metadata = runner
            .dispatcher()
            .vfs_metadata
            .get("System Folder/Preferences/Test App HighScores")
            .copied()
            .expect("dirty PPC resource fork should carry Finder metadata");
        assert_eq!(scores_metadata.creator, u32::from_be_bytes(*b"Nano"));
        assert_eq!(scores_metadata.file_type, u32::from_be_bytes(*b"pref"));
        assert_eq!(scores_metadata.finder_flags, 0x0400);
        let saves_directory = runner
            .dispatcher()
            .vfs_directories
            .iter()
            .find(|directory| directory.path == "System Folder/Preferences/Test App Saves")
            .expect("dirty PPC directory should remain in the shared catalogue");
        assert_eq!(
            TrapDispatcher::vfs_basename(&saves_directory.path),
            "Test App Saves"
        );
        assert!(output_dir
            .path()
            .join("System Folder/Preferences/Test App Saves")
            .is_dir());
        assert_eq!(
            std::fs::read(
                output_dir
                    .path()
                    .join("System Folder/Preferences/.rsrc/Test App HighScores")
            )
            .unwrap()
            .as_slice(),
            fork_bytes.as_slice()
        );
        assert!(!runner.native.application().unwrap().vfs_files[0].dirty);
        assert!(!runner.native.application().unwrap().vfs_directories[0].dirty);
        assert!(runner
            .native
            .application()
            .unwrap()
            .deleted_vfs_file_paths
            .is_empty());
        assert!(!runner.native.application().unwrap().vfs_resource_files[0].dirty);

        let prefs_path = "System Folder/Preferences/Test App Prefs";
        runner
            .native
            .application_mut()
            .unwrap()
            .with_test_vfs_file_mut(0, |file| {
                file.data
                    .with_mut(|data| data.extend_from_slice(b"-native"));
            })
            .expect("seeded native preferences file");
        assert_eq!(
            runner.dispatcher().vfs.get(prefs_path).unwrap(),
            b"prefs-native",
            "classic File Manager view must observe native writes before another runner sync"
        );

        runner
            .dispatcher_mut()
            .vfs
            .with_entry_mut(prefs_path, |bytes| bytes.extend_from_slice(b"-classic"))
            .unwrap();
        let native_file = &runner.native.application().unwrap().vfs_files[0];
        let classic_file = runner.dispatcher().vfs.get_shared(prefs_path).unwrap();
        assert!(native_file.data.ptr_eq(classic_file));
        assert_eq!(native_file.data.as_slice(), b"prefs-native-classic");
    }

    #[test]
    fn ppc_double_buffer_playback_feeds_consecutive_host_audio_buffers() {
        let channel = 0x0500_1000;
        let header = PPC_DATA_BASE + 0x100;
        let buffer0 = PPC_DATA_BASE + 0x200;
        let buffer1 = PPC_DATA_BASE + 0x300;
        let expected = vec![0x80, 0x90, 0x70, 0xa0];
        let sound = PpcSoundState::default();
        sound.manager.replace_double_buffer_playbacks(vec![PpcSoundDoubleBufferPlaybackRecord {
            channel,
            header,
            buffers: [buffer0, buffer1],
            callback: 0,
            callback_architecture: CallbackTaskArchitecture::PowerPc,
            sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            compression_id: 0,
            packet_size: 0,
            current_buffer_index: 0,
            callback_pending_mask: 0,
            active: true,
            host_initialized: false,
            host_buffer_loaded: false,
        }]);
        let mut app = halted_ppc_app_with_sound(sound);
        {
            let ppc_app = app.ppc.as_mut().expect("PPC app");
            ppc_app.memory.add_region(PPC_DATA_BASE, vec![0; 0x400]);
            ppc_app.memory.write_u32_be(buffer0, 2).unwrap();
            ppc_app.memory.write_u32_be(buffer0 + 4, 0x01).unwrap();
            ppc_app
                .memory
                .write_bytes(buffer0 + 16, &expected[..2])
                .unwrap();
            ppc_app.memory.write_u32_be(buffer1, 2).unwrap();
            ppc_app.memory.write_u32_be(buffer1 + 4, 0x05).unwrap();
            ppc_app
                .memory
                .write_bytes(buffer1 + 16, &expected[2..])
                .unwrap();
        }
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        let (steps, running) = runner.run_steps_with_audio(8, None, expected.len());

        assert_eq!(steps, 1);
        assert!(!running);
        assert_eq!(runner.drain_audio(), expected);
        assert_eq!(
            runner.dispatcher().sound_manager.debug_double_buffer_count,
            1
        );
        assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 4);
        let channel = runner
            .dispatcher()
            .sound_manager
            .channels
            .iter()
            .find(|candidate| candidate.guest_ptr == channel)
            .expect("process Sound Manager channel");
        assert_eq!(channel.debug_double_buffer_loads, 2);
        assert_eq!(channel.debug_double_buffer_non_silent_loads, 2);
        assert_eq!(channel.debug_double_buffer_frames_loaded, 4);
        assert_eq!(channel.debug_double_buffer_non_silent_frames, 3);
        assert_eq!(channel.debug_double_buffer_captured_samples, expected);
        let playback = runner
            .native
            .application()
            .expect("PPC app should stay loaded")
            .sound
            .manager
            .double_buffer_playbacks[0];
        assert!(!playback.active);
        assert!(!playback.host_buffer_loaded);
        assert_eq!(playback.current_buffer_index, 1);
    }

    #[test]
    fn ppc_decoded_buffer_command_feeds_host_audio_buffer() {
        let channel = 0x0500_1000;
        let samples = vec![0x80, 0x90, 0x70, 0xa0];
        let sound = PpcSoundState::default();
        sound.manager.play_buffer_command_for_architecture(
            channel,
            samples.clone(),
            crate::sound::OUTPUT_RATE << 16,
            CallbackTaskArchitecture::M68k,
        );
        let app = halted_ppc_app_with_sound(sound);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        let (steps, running) = runner.run_steps_with_audio(8, None, samples.len());

        assert_eq!(steps, 1);
        assert!(!running);
        assert_eq!(runner.drain_audio(), samples);
        assert_eq!(runner.dispatcher().sound_manager.debug_cmd_count, 1);
        assert_eq!(runner.dispatcher().sound_manager.debug_buffer_cmd_count, 1);
        assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 4);
    }

    #[test]
    fn ppc_decoded_file_playback_feeds_host_audio_buffer() {
        let channel = 0x0500_1000;
        let callback_entry = PPC_CODE_BASE + 0x40;
        let completion = PPC_DATA_BASE + 0x20;
        let completion_tvector = PPC_DATA_BASE + 0x80;
        let samples = vec![0x80, 0x90, 0x70, 0x80];
        let mut preview = [0; 16];
        preview[..samples.len()].copy_from_slice(&samples);
        let mut app = halted_ppc_app_with_sound(PpcSoundState {
            manager: Default::default(),
            queued_commands: Vec::new(),
            immediate_commands: Vec::new(),
            file_playbacks: vec![PpcSoundFilePlaybackRecord {
                channel,
                ref_num: 128,
                resource_id: -1,
                buffer_size: 20_480,
                buffer: 0,
                selection: 0,
                completion,
                completion_command: None,
                async_play: true,
                aiff: None,
                decoded_aiff: Some(PpcDecodedAiffSamples {
                    sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                    sample_count: samples.len() as u32,
                    preview_len: samples.len() as u8,
                    preview,
                }),
            }],
            decoded_file_playbacks: vec![PpcDecodedAiffPlaybackRecord {
                file_playback_index: 0,
                channel,
                sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                samples: samples.clone(),
            }],
            completion_invocations: Vec::new(),
            sys_beep_count: 0,
            last_sys_beep_duration: 0,
            start_count: 1,
            pause_count: 0,
            stop_count: 0,
            double_buffer_play_count: 0,
            last_double_buffer_channel: 0,
            last_double_buffer_header: 0,
        });
        {
            let ppc_app = app.ppc.as_mut().expect("PPC app");
            ppc_app.sound.manager.play_file_buffer(
                channel,
                samples.clone(),
                crate::sound::OUTPUT_RATE << 16,
                Some((CallbackTaskArchitecture::PowerPc, completion)),
            );
            let stw_r3_0_r2 = (36u32 << 26) | (3u32 << 21) | (2u32 << 16);
            let mut callback_bytes = Vec::new();
            callback_bytes.extend_from_slice(&stw_r3_0_r2.to_be_bytes());
            callback_bytes.extend_from_slice(&0x4e80_0020u32.to_be_bytes());
            ppc_app.memory.add_region(callback_entry, callback_bytes);
            ppc_app.memory.add_region(PPC_DATA_BASE, vec![0; 0x100]);
            ppc_app.rtoc = PPC_DATA_BASE;
            ppc_app.cpu.gpr[2] = PPC_DATA_BASE;

            ppc_app
                .memory
                .write_u16_be(completion, 0xAAFE)
                .expect("write goMixedModeTrap");
            ppc_app
                .memory
                .write_u8(completion + 2, 7)
                .expect("write descriptor version");
            ppc_app
                .memory
                .write_u16_be(completion + 10, 0)
                .expect("write routine count");
            let record = completion + 12;
            ppc_app
                .memory
                .write_u8(record + 5, 1)
                .expect("write PowerPC ISA");
            ppc_app
                .memory
                .write_u16_be(record + 6, 0x0004)
                .expect("write routine flags");
            ppc_app
                .memory
                .write_u32_be(record + 8, completion_tvector)
                .expect("write proc descriptor");
            ppc_app
                .memory
                .write_u32_be(completion_tvector, callback_entry)
                .expect("write callback TVector entry");
            ppc_app
                .memory
                .write_u32_be(completion_tvector + 4, PPC_DATA_BASE)
                .expect("write callback TVector RTOC");
        }
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        let (steps, running) = runner.run_steps_with_audio(8, None, samples.len());

        assert_eq!(steps, 1);
        assert!(!running);
        assert_eq!(runner.drain_audio(), samples);
        assert_eq!(runner.dispatcher().sound_manager.debug_file_play_count, 1);
        assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 4);
        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        assert_eq!(ppc_app.sound.manager.file_playback_paused(channel), None);
        assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());
        let mut memory = ppc_app.memory.clone();
        assert_eq!(memory.read_u32_be(PPC_DATA_BASE), Some(channel));
        assert_eq!(ppc_app.sound.completion_invocations.len(), 1);
        let invocation = ppc_app.sound.completion_invocations[0];
        assert_eq!(invocation.file_playback_index, 0);
        assert_eq!(invocation.channel, channel);
        assert_eq!(invocation.completion, completion);
        assert_eq!(invocation.callback_entry, callback_entry);
        assert_eq!(invocation.callback_rtoc, PPC_DATA_BASE);
        assert_eq!(invocation.tick, 0);
        assert_eq!(invocation.instruction_count, 1);
        assert_eq!(invocation.scheduled_tick, invocation.tick);
        assert_eq!(
            invocation.scheduled_instruction_count,
            invocation.instruction_count
        );
        assert_eq!(invocation.cycles, 2);
        assert_eq!(
            invocation.result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                cycles: 2
            }
        );
        assert_eq!(invocation.unsupported_import_index, None);
    }

    #[test]
    fn ppc_decoded_file_playback_waits_for_process_audio_cursor() {
        let channel = 0x0500_1000;
        let callback_entry = PPC_CODE_BASE + 0x40;
        let samples = vec![0x80, 0x90, 0x70, 0x80];
        let mut preview = [0; 16];
        preview[..samples.len()].copy_from_slice(&samples);
        let mut app = halted_ppc_app_with_sound(PpcSoundState {
            manager: Default::default(),
            queued_commands: Vec::new(),
            immediate_commands: Vec::new(),
            file_playbacks: vec![PpcSoundFilePlaybackRecord {
                channel,
                ref_num: 128,
                resource_id: -1,
                buffer_size: 20_480,
                buffer: 0,
                selection: 0,
                completion: callback_entry,
                completion_command: None,
                async_play: true,
                aiff: None,
                decoded_aiff: Some(PpcDecodedAiffSamples {
                    sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                    sample_count: samples.len() as u32,
                    preview_len: samples.len() as u8,
                    preview,
                }),
            }],
            decoded_file_playbacks: vec![PpcDecodedAiffPlaybackRecord {
                file_playback_index: 0,
                channel,
                sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
                samples: samples.clone(),
            }],
            completion_invocations: Vec::new(),
            sys_beep_count: 0,
            last_sys_beep_duration: 0,
            start_count: 1,
            pause_count: 0,
            stop_count: 0,
            double_buffer_play_count: 0,
            last_double_buffer_channel: 0,
            last_double_buffer_header: 0,
        });
        {
            let ppc_app = app.ppc.as_mut().expect("PPC app");
            ppc_app.sound.manager.play_file_buffer(
                channel,
                samples.clone(),
                crate::sound::OUTPUT_RATE << 16,
                Some((CallbackTaskArchitecture::PowerPc, callback_entry)),
            );
            ppc_app
                .memory
                .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
                .expect("rewrite entry as infinite branch");
            let stw_r3_0_r2 = (36u32 << 26) | (3u32 << 21) | (2u32 << 16);
            let mut callback_bytes = Vec::new();
            callback_bytes.extend_from_slice(&stw_r3_0_r2.to_be_bytes());
            callback_bytes.extend_from_slice(&0x4e80_0020u32.to_be_bytes());
            ppc_app.memory.add_region(callback_entry, callback_bytes);
            ppc_app.memory.add_region(PPC_DATA_BASE, vec![0; 0x100]);
            ppc_app.rtoc = PPC_DATA_BASE;
            ppc_app.cpu.gpr[2] = PPC_DATA_BASE;
        }

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let (steps, running) = runner.run_steps_with_audio(1, None, 0);
        assert_eq!(steps, 1);
        assert!(running);
        {
            let ppc_app = runner
                .native
                .application()
                .expect("PPC app should stay loaded");
            assert_eq!(
                ppc_app.sound.manager.file_playback_paused(channel),
                Some(false)
            );
            assert!(ppc_app.sound.completion_invocations.is_empty());
        }

        let default_budget =
            crate::machine_profile::DEFAULT_HOST_EXECUTION_POLICY.scripted_instructions_per_tick;
        let (steps, running) = runner.run_steps_with_audio(default_budget as usize, None, 0);

        assert_eq!(steps, default_budget as usize);
        assert!(running);
        assert!(runner.drain_audio().is_empty());
        assert_eq!(runner.guest_tick(), 1);
        assert_eq!(runner.dispatcher().sound_manager.debug_file_play_count, 1);
        assert_eq!(runner.dispatcher().sound_manager.debug_samples_mixed, 0);
        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        assert_eq!(
            ppc_app.sound.manager.file_playback_paused(channel),
            Some(false)
        );
        assert!(ppc_app.sound.completion_invocations.is_empty());

        let (steps, running) = runner.run_steps_with_audio(1, None, samples.len());
        assert_eq!(steps, 1);
        assert!(running);
        assert_eq!(runner.drain_audio(), samples);
        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        assert_eq!(ppc_app.sound.manager.file_playback_paused(channel), None);
        assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());
        let mut memory = ppc_app.memory.clone();
        assert_eq!(memory.read_u32_be(PPC_DATA_BASE), Some(channel));
        assert_eq!(ppc_app.sound.completion_invocations.len(), 1);
        let invocation = ppc_app.sound.completion_invocations[0];
        assert_eq!(invocation.file_playback_index, 0);
        assert_eq!(invocation.channel, channel);
        assert_eq!(invocation.completion, callback_entry);
        assert_eq!(invocation.callback_entry, callback_entry);
        assert_eq!(invocation.callback_rtoc, PPC_DATA_BASE);
        assert_eq!(invocation.tick, 1);
        assert_eq!(invocation.scheduled_tick, invocation.tick);
        assert_eq!(
            invocation.scheduled_instruction_count,
            invocation.instruction_count
        );
        assert_eq!(invocation.cycles, 2);
        assert_eq!(
            invocation.result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                cycles: 2
            }
        );
        assert_eq!(invocation.unsupported_import_index, None);
    }

    #[test]
    fn ppc_decoded_file_playback_pause_resume_uses_process_cursor() {
        let channel = 0x0500_1000;
        let samples = vec![0x80, 0x90, 0x70, 0x80];
        let mut sound = PpcSoundState::default();
        sound.file_playbacks.push(PpcSoundFilePlaybackRecord {
            channel,
            ref_num: 128,
            resource_id: -1,
            buffer_size: 20_480,
            buffer: 0,
            selection: 0,
            completion: 0,
            completion_command: None,
            async_play: true,
            aiff: None,
            decoded_aiff: None,
        });
        sound.manager.play_file_buffer(
            channel,
            samples.clone(),
            crate::sound::OUTPUT_RATE << 16,
            Some((CallbackTaskArchitecture::PowerPc, 0)),
        );
        assert_eq!(sound.manager.toggle_file_paused(channel), Some(true));
        let app = halted_ppc_app_with_sound(sound);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        runner.mix_host_audio(samples.len());
        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        assert_eq!(
            ppc_app.sound.manager.file_playback_paused(channel),
            Some(true)
        );
        assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());

        assert_eq!(
            runner.dispatcher.sound_manager.toggle_file_paused(channel),
            Some(false)
        );
        runner.mix_host_audio(samples.len());

        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        assert_eq!(ppc_app.sound.manager.file_playback_paused(channel), None);
        assert!(ppc_app.sound.manager.pending_sound_callbacks.is_empty());
        assert!(ppc_app.sound.completion_invocations.is_empty());
    }

    #[test]
    fn ppc_system_event_mask_write_enables_injected_key_up_events() {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        app.ppc
            .as_mut()
            .expect("synthetic PPC app")
            .memory
            .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        runner
            .native
            .application_mut()
            .expect("PPC app installed")
            .memory
            .write_u16_be(crate::memory::globals::addr::SYS_EVT_MASK, 0xffdf)
            .unwrap();
        runner.push_key_down(0x7c, 29);
        runner.push_key_up(0x7c, 29);

        assert!(runner
            .process_context
            .event_queue()
            .iter()
            .any(|event| event.what == 4 && event.message == 0x0000_7c1d));
    }

    #[test]
    fn cursor_state_is_immediately_shared_between_cpu_adapters() {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);

        let mut data = [0; 32];
        data[0] = 0x80;
        let mut mask = [0; 32];
        mask[0] = 0xc0;
        let ppc_app = runner.native.application_mut().expect("PPC app installed");
        ppc_app
            .cursor_state
            .install(crate::display::CursorImage::mono(data, mask, 3, 4));
        ppc_app.cursor_state.hide();

        assert_eq!(runner.dispatcher.cursor_level(), -1);
        assert!(!runner.dispatcher.cursor_visible());
        assert_eq!(runner.dispatcher.cursor_data(), Some((data, mask, 3, 4)));

        runner.dispatcher.cursor_state.show();
        assert_eq!(
            runner
                .native
                .application()
                .expect("PPC app installed")
                .cursor_level(),
            0
        );
    }

    #[test]
    fn ppc_queue_sync_preserves_autokey_posted_during_tick_advance() {
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app
            .memory
            .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
            .unwrap(); // b .

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.set_instructions_per_tick(1);
        runner.push_key_down(0x00, b'a');

        for _ in 0..TrapDispatcher::AUTO_KEY_THRESHOLD_TICKS {
            let (steps, running) = runner.run_steps(1, None);
            assert_eq!(steps, 1);
            assert!(running);
        }

        assert!(runner
            .process_context
            .event_queue()
            .iter()
            .any(|event| { event.what == 5 && event.message == 0x0000_0061 }));
    }

    #[test]
    fn ppc_getkeys_reads_runner_key_map_with_classic_packed_bit_order() {
        let key_map_ptr = PPC_DATA_BASE;
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_CODE_BASE, 0x4800_0002u32.to_be_bytes().to_vec());
        memory.add_region(PPC_DATA_BASE, vec![0; 32]);
        let mut cpu = PpcCpu::new();
        cpu.pc = PPC_IMPORT_TRAP_BASE;
        cpu.lr = PPC_CODE_BASE;
        cpu.gpr[1] = PPC_STACK_TOP - 64;
        cpu.gpr[3] = key_map_ptr;

        let app = LoadedApp::from_ppc(PpcLoadedApp {
            cpu,
            memory,
            entry_pc: PPC_IMPORT_TRAP_BASE,
            rtoc: 0,
            stack_base: PPC_STACK_BASE,
            stack_size: PPC_STACK_SIZE,
            stack_pointer: PPC_STACK_TOP - 64,
            tick_state: SharedProcessTickState::default(),
            clock_cycles_per_tick: 1,
            clock_cycle_phase: 0,
            native_exception_handler: 0,
            native_exception_stack: Vec::new(),
            stdc_qsort_stack: Vec::new(),
            dialog_callback_stack: Vec::new(),
            apple_events: Default::default(),
            cfm: Some(crate::cfm::CfmState::default()),
            controls: Default::default(),
            screen_clut: SharedProcessValue::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
            display_gamma: SharedProcessDisplayGamma::default(),
            process_quickdraw_port_state_attached: false,
            color_manager_clut: SharedProcessValue::from_value(
                TrapDispatcher::standard_mac_8bpp_clut(),
            ),
            aliases: Vec::new(),
            gworlds: Vec::new(),
            gworld_pixel_states: crate::process_context::SharedProcessQuickDrawPixelStates::default(
            ),
            q3_objects: Vec::new(),
            q3_object_refs: Vec::new(),
            next_q3_object: 0,
            q3_error_state: Default::default(),
            q3_lifecycle: Default::default(),
            q3_memory_storages: Vec::new(),
            q3_files: Vec::new(),
            q3_group_memberships: Vec::new(),
            q3_file_groups: Vec::new(),
            q3_views: Vec::new(),
            q3_submissions: Vec::new(),
            q3_view_transforms: Vec::new(),
            q3_submission_transforms: Vec::new(),
            q3_view_materials: Vec::new(),
            q3_submission_materials: Vec::new(),
            q3_submission_lights: Vec::new(),
            q3_view_state_stack: Vec::new(),
            q3_completed_frames: Vec::new(),
            q3_retained_frames: Vec::new(),
            q3_state_only_completed_frame_batches: Vec::new(),
            q3_fog_styles: Vec::new(),
            q3_attributes: Vec::new(),
            q3_shader_uv_transforms: Vec::new(),
            q3_shader_boundaries: Vec::new(),
            q3_mipmap_textures: Vec::new(),
            q3_texture_shaders: Vec::new(),
            q3_renderer_preferences: Vec::new(),
            q3_draw_contexts: Vec::new(),
            q3_trimeshes: Vec::new(),
            q3_styles: Vec::new(),
            q3_cameras: Vec::new(),
            q3_lights: Vec::new(),
            input_sprocket: Default::default(),
            input_sprocket_virtual_elements: Vec::new(),
            toolbox_startup: Default::default(),
            quicktime: Default::default(),
            sound: Default::default(),
            timer_tasks: Default::default(),
            vbl_tasks: Default::default(),
            callback_scheduling: Default::default(),
            process_file_system: ppc_initial_process_file_system(),
            current_gworld: SharedProcessValue::from_value(PPC_MAIN_GWORLD),
            current_gdevice: SharedProcessValue::from_value(PPC_MAIN_GDEVICE),
            quickdraw_op_colors: SharedProcessValue::default(),
            quickdraw_hilite_colors: SharedProcessValue::default(),
            quickdraw_fore_color: PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            },
            quickdraw_fore_indices: Default::default(),
            quickdraw_back_color: PpcRgbColor {
                red: 0xffff,
                green: 0xffff,
                blue: 0xffff,
            },
            quickdraw_pen_h: 0,
            quickdraw_pen_v: 0,
            quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
            quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
            cursor_state: crate::process_context::SharedProcessCursorState::default(),
            param_text: Default::default(),
            scrap: Default::default(),
            list_manager: Default::default(),
            halt_pc: PPC_HALT_PC,
            import_trap_base: PPC_IMPORT_TRAP_BASE,
            import_count: 1,
            imports: vec![PpcImportBinding {
                library_index: 0,
                symbol_index: 0,
                library_name: "InterfaceLib".into(),
                symbol_name: "GetKeys".into(),
                class: 0,
                weak: false,
                address: PPC_IMPORT_TRAP_BASE,
                tvector_address: None,
                trap_pc: PPC_IMPORT_TRAP_BASE,
                dispatcher_target: PpcImportDispatcherTarget::GetKeys,
            }],
            section_bases: Vec::new(),
            input: PpcInputSnapshot::default(),
            process_input: Default::default(),
            event_queue: Default::default(),
            window_list: Default::default(),
            process_memory_manager: PpcProcessMemoryManager::with_heap(
                PPC_HEAP_BASE,
                PPC_STACK_BASE,
            ),
            draw_sprocket: PpcDrawSprocketState::default(),
        });
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        runner.push_key_down(0x00, b'a');
        runner.push_key_down(0x7b, 28);
        runner.push_key_down(0x31, b' ');

        let (steps, running) = runner.run_steps(16, None);

        assert!(!running);
        assert_eq!(steps, 2);
        let ppc_app = runner
            .native
            .application_mut()
            .expect("PPC app should stay loaded");
        assert_eq!(ppc_app.memory.read_u8(key_map_ptr), Some(0x01));
        assert_eq!(ppc_app.memory.read_u8(key_map_ptr + 6), Some(0x02));
        assert_eq!(ppc_app.memory.read_u8(key_map_ptr + 15), Some(0x08));
    }

    #[test]
    #[cfg(any())]
    fn ppc_imports_are_recorded_in_oracle_events_when_enabled() {
        let key_map_ptr = PPC_DATA_BASE;
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_CODE_BASE, 0x4800_0002u32.to_be_bytes().to_vec());
        memory.add_region(PPC_DATA_BASE, vec![0; 32]);
        let mut cpu = PpcCpu::new();
        cpu.pc = PPC_IMPORT_TRAP_BASE;
        cpu.lr = PPC_CODE_BASE;
        cpu.gpr[1] = PPC_STACK_TOP - 64;
        cpu.gpr[2] = 0x1234_5678;
        cpu.gpr[3] = key_map_ptr;

        let app = LoadedApp::from_ppc(PpcLoadedApp {
            cpu,
            memory,
            entry_pc: PPC_IMPORT_TRAP_BASE,
            rtoc: 0x1234_5678,
            stack_base: PPC_STACK_BASE,
            stack_size: PPC_STACK_SIZE,
            stack_pointer: PPC_STACK_TOP - 64,
            tick_state: SharedProcessTickState::default(),
            clock_cycles_per_tick: 1,
            clock_cycle_phase: 0,
            native_exception_handler: 0,
            native_exception_stack: Vec::new(),
            stdc_qsort_stack: Vec::new(),
            dialog_callback_stack: Vec::new(),
            apple_events: Default::default(),
            cfm: Some(crate::cfm::CfmState::default()),
            controls: Default::default(),
            screen_clut: SharedProcessValue::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
            display_gamma: SharedProcessDisplayGamma::default(),
            process_quickdraw_port_state_attached: false,
            color_manager_clut: SharedProcessValue::from_value(
                TrapDispatcher::standard_mac_8bpp_clut(),
            ),
            aliases: Vec::new(),
            gworlds: Vec::new(),
            gworld_pixel_states: crate::process_context::SharedProcessQuickDrawPixelStates::default(
            ),
            q3_objects: Vec::new(),
            q3_object_refs: Vec::new(),
            next_q3_object: 0,
            q3_error_state: Default::default(),
            q3_lifecycle: Default::default(),
            q3_memory_storages: Vec::new(),
            q3_files: Vec::new(),
            q3_group_memberships: Vec::new(),
            q3_file_groups: Vec::new(),
            q3_views: Vec::new(),
            q3_submissions: Vec::new(),
            q3_view_transforms: Vec::new(),
            q3_submission_transforms: Vec::new(),
            q3_view_materials: Vec::new(),
            q3_submission_materials: Vec::new(),
            q3_submission_lights: Vec::new(),
            q3_view_state_stack: Vec::new(),
            q3_completed_frames: Vec::new(),
            q3_retained_frames: Vec::new(),
            q3_state_only_completed_frame_batches: Vec::new(),
            q3_fog_styles: Vec::new(),
            q3_attributes: Vec::new(),
            q3_shader_uv_transforms: Vec::new(),
            q3_shader_boundaries: Vec::new(),
            q3_mipmap_textures: Vec::new(),
            q3_texture_shaders: Vec::new(),
            q3_renderer_preferences: Vec::new(),
            q3_draw_contexts: Vec::new(),
            q3_trimeshes: Vec::new(),
            q3_styles: Vec::new(),
            q3_cameras: Vec::new(),
            q3_lights: Vec::new(),
            input_sprocket: Default::default(),
            input_sprocket_virtual_elements: Vec::new(),
            toolbox_startup: Default::default(),
            quicktime: Default::default(),
            sound: Default::default(),
            timer_tasks: Default::default(),
            vbl_tasks: Default::default(),
            callback_scheduling: Default::default(),
            process_file_system: ppc_initial_process_file_system(),
            current_gworld: SharedProcessValue::from_value(PPC_MAIN_GWORLD),
            current_gdevice: SharedProcessValue::from_value(PPC_MAIN_GDEVICE),
            quickdraw_op_colors: SharedProcessValue::default(),
            quickdraw_hilite_colors: SharedProcessValue::default(),
            quickdraw_fore_color: PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            },
            quickdraw_fore_indices: Default::default(),
            quickdraw_back_color: PpcRgbColor {
                red: 0xffff,
                green: 0xffff,
                blue: 0xffff,
            },
            quickdraw_pen_h: 0,
            quickdraw_pen_v: 0,
            quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
            quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
            cursor_state: crate::process_context::SharedProcessCursorState::default(),
            param_text: Default::default(),
            scrap: Default::default(),
            list_manager: Default::default(),
            halt_pc: PPC_HALT_PC,
            import_trap_base: PPC_IMPORT_TRAP_BASE,
            import_count: 1,
            imports: vec![PpcImportBinding {
                library_index: 0,
                symbol_index: 0,
                library_name: "InterfaceLib".into(),
                symbol_name: "GetKeys".into(),
                class: 0,
                weak: false,
                address: PPC_IMPORT_TRAP_BASE,
                tvector_address: None,
                trap_pc: PPC_IMPORT_TRAP_BASE,
                dispatcher_target: PpcImportDispatcherTarget::GetKeys,
            }],
            section_bases: Vec::new(),
            input: PpcInputSnapshot::default(),
            process_input: Default::default(),
            event_queue: Default::default(),
            window_list: Default::default(),
            process_memory_manager: PpcProcessMemoryManager::with_heap(
                PPC_HEAP_BASE,
                PPC_STACK_BASE,
            ),
            draw_sprocket: PpcDrawSprocketState::default(),
        });
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let output_dir = tempfile::tempdir().unwrap();
        runner
            .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
            .unwrap();

        let (_steps, _running) = runner.run_steps(16, None);

        let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
        let events = std::fs::read_to_string(events_path).unwrap();
        let ppc_event: serde_json::Value = events
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .find(|value: &serde_json::Value| value["event"] == "ppc_import")
            .expect("oracle recording should include a PPC import event");
        assert_eq!(ppc_event["pc"], PPC_IMPORT_TRAP_BASE);
        assert_eq!(ppc_event["fields"]["import_index"], "0");
        assert_eq!(ppc_event["fields"]["library"], "InterfaceLib");
        assert_eq!(ppc_event["fields"]["symbol"], "GetKeys");
        assert_eq!(ppc_event["fields"]["rtoc"], "12345678");
        assert_eq!(ppc_event["fields"]["dispatcher_target"], "GetKeys");
        assert_eq!(ppc_event["fields"]["repeat_count"], "1");
    }

    #[test]
    #[cfg(any())]
    fn oracle_script_input_event_records_fields_without_snapshot() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let output_dir = tempfile::tempdir().unwrap();
        runner
            .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
            .unwrap();

        runner
            .record_oracle_script_input(BTreeMap::from([
                ("action".to_string(), "key_down".to_string()),
                ("key".to_string(), "space".to_string()),
                ("mac_key".to_string(), "31".to_string()),
                ("char_code".to_string(), "20".to_string()),
            ]))
            .unwrap();

        let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
        let events = std::fs::read_to_string(events_path).unwrap();
        let script_input: serde_json::Value = events
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .find(|value: &serde_json::Value| value["event"] == "script_input")
            .expect("oracle recording should include a script_input event");
        assert_eq!(script_input["screen_event_count"], 0);
        assert_eq!(script_input["fields"]["action"], "key_down");
        assert_eq!(script_input["fields"]["key"], "space");
        assert_eq!(script_input["fields"]["mac_key"], "31");
        assert_eq!(script_input["fields"]["char_code"], "20");

        let snapshots_path = output_dir.path().join(crate::oracle::ORACLE_SNAPSHOTS_FILE);
        let snapshots = std::fs::read_to_string(snapshots_path).unwrap();
        assert!(
            snapshots.trim().is_empty(),
            "script input events should not create screen snapshots"
        );
    }

    #[test]
    #[cfg(any())]
    fn oracle_input_sprocket_event_records_simple_state_without_snapshot() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let output_dir = tempfile::tempdir().unwrap();
        runner
            .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
            .unwrap();

        let mut input = PpcInputSnapshot::default();
        input.key_map[0] = 0x01;
        runner.record_input_sprocket_trace(&[PpcInputSprocketSimpleStateTraceEntry {
            import_index: 7,
            pc: 0x01f0_1234,
            element: 0x0200_1000,
            state_ptr: 0x0200_2000,
            state: 0x0000_0001,
            kind: 0x6275_746e,
            kind_name: "button".to_string(),
            fallback_state: 0,
            need_name: "Fire".to_string(),
            action_binding: "button/fire".to_string(),
            input,
            input_sprocket: PpcInputSprocketState {
                initialized: true,
                suspended: false,
                keyboard_active: true,
                mouse_active: true,
                configure_count: 0,
                virtual_element_count: 1,
                last_virtual_need_count: 1,
                last_virtual_needs_ptr: 0x0200_3000,
                last_virtual_elements_out_ptr: 0x0200_4000,
            },
        }]);

        let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
        let events = std::fs::read_to_string(events_path).unwrap();
        let input_sprocket: serde_json::Value = events
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .find(|value: &serde_json::Value| value["event"] == "input_sprocket")
            .expect("oracle recording should include an InputSprocket event");
        assert_eq!(input_sprocket["pc"], 0x01f0_1234);
        assert_eq!(input_sprocket["screen_event_count"], 0);
        assert_eq!(input_sprocket["fields"]["import_index"], "7");
        assert_eq!(input_sprocket["fields"]["element"], "02001000");
        assert_eq!(input_sprocket["fields"]["state_ptr"], "02002000");
        assert_eq!(input_sprocket["fields"]["state"], "00000001");
        assert_eq!(input_sprocket["fields"]["kind"], "6275746E");
        assert_eq!(input_sprocket["fields"]["kind_name"], "button");
        assert_eq!(input_sprocket["fields"]["need_name"], "Fire");
        assert_eq!(input_sprocket["fields"]["action_binding"], "button/fire");
        assert_eq!(
            input_sprocket["fields"]["key_map"],
            "01000000000000000000000000000000"
        );
        assert_eq!(input_sprocket["fields"]["mouse_button"], "false");
        assert_eq!(input_sprocket["fields"]["keyboard_active"], "true");

        let snapshots_path = output_dir.path().join(crate::oracle::ORACLE_SNAPSHOTS_FILE);
        let snapshots = std::fs::read_to_string(snapshots_path).unwrap();
        assert!(
            snapshots.trim().is_empty(),
            "InputSprocket events should not create screen snapshots"
        );
    }

    #[test]
    #[cfg(any())]
    fn oracle_draw_sprocket_event_records_swap_without_snapshot() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let output_dir = tempfile::tempdir().unwrap();
        runner
            .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
            .unwrap();

        runner.record_draw_sprocket_trace(&[PpcDrawSprocketTraceEntry {
            import_index: 12,
            pc: 0x01f0_3456,
            action: "swap_buffers".to_string(),
            result: 0,
            context: Some(0x0200_1000),
            requested_state: None,
            requested_frequency: None,
            requested_width: None,
            requested_height: None,
            requested_context_options: None,
            requested_display_depth_mask: None,
            requested_back_buffer_depth_mask: None,
            requested_display_depth: None,
            requested_back_buffer_depth: None,
            requested_page_count: None,
            can_user_select: None,
            fade_kind: None,
            fade_percent: None,
            fade_zero_red: None,
            fade_zero_green: None,
            fade_zero_blue: None,
            reserved_context: Some(0x0200_1000),
            active_context: Some(0x0200_1000),
            context_state: "active".to_string(),
            front_buffer_gworld: 0x00ab_cdef,
            back_buffer_gworld: 0x0012_3456,
            last_swap_context: Some(0x0200_1000),
            swap_count: 3,
            fade_count: 1,
            frequency: 60 << 16,
            width: 640,
            height: 480,
            context_options: 1,
            display_depth_mask: 16,
            back_buffer_depth_mask: 16,
            display_depth: 16,
            back_buffer_depth: 16,
            page_count: 2,
        }]);

        let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
        let events = std::fs::read_to_string(events_path).unwrap();
        let draw_sprocket: serde_json::Value = events
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .find(|value: &serde_json::Value| value["event"] == "draw_sprocket")
            .expect("oracle recording should include a DrawSprocket event");
        assert_eq!(draw_sprocket["pc"], 0x01f0_3456);
        assert_eq!(draw_sprocket["screen_event_count"], 0);
        assert_eq!(draw_sprocket["fields"]["import_index"], "12");
        assert_eq!(draw_sprocket["fields"]["action"], "swap_buffers");
        assert_eq!(draw_sprocket["fields"]["result"], "0");
        assert_eq!(draw_sprocket["fields"]["result_hex"], "0000");
        assert_eq!(draw_sprocket["fields"]["context"], "02001000");
        assert_eq!(draw_sprocket["fields"]["requested_state"], "none");
        assert_eq!(draw_sprocket["fields"]["requested_frequency"], "none");
        assert_eq!(draw_sprocket["fields"]["requested_width"], "none");
        assert_eq!(draw_sprocket["fields"]["requested_height"], "none");
        assert_eq!(draw_sprocket["fields"]["requested_context_options"], "none");
        assert_eq!(
            draw_sprocket["fields"]["requested_display_depth_mask"],
            "none"
        );
        assert_eq!(
            draw_sprocket["fields"]["requested_back_buffer_depth_mask"],
            "none"
        );
        assert_eq!(draw_sprocket["fields"]["requested_display_depth"], "none");
        assert_eq!(
            draw_sprocket["fields"]["requested_back_buffer_depth"],
            "none"
        );
        assert_eq!(draw_sprocket["fields"]["requested_page_count"], "none");
        assert_eq!(draw_sprocket["fields"]["can_user_select"], "none");
        assert_eq!(draw_sprocket["fields"]["fade_kind"], "none");
        assert_eq!(draw_sprocket["fields"]["fade_percent"], "none");
        assert_eq!(draw_sprocket["fields"]["fade_zero_red"], "none");
        assert_eq!(draw_sprocket["fields"]["fade_zero_green"], "none");
        assert_eq!(draw_sprocket["fields"]["fade_zero_blue"], "none");
        assert_eq!(draw_sprocket["fields"]["reserved_context"], "02001000");
        assert_eq!(draw_sprocket["fields"]["active_context"], "02001000");
        assert_eq!(draw_sprocket["fields"]["has_reserved_context"], "true");
        assert_eq!(draw_sprocket["fields"]["has_active_context"], "true");
        assert_eq!(draw_sprocket["fields"]["context_state"], "active");
        assert_eq!(draw_sprocket["fields"]["front_gworld"], "00ABCDEF");
        assert_eq!(draw_sprocket["fields"]["back_gworld"], "00123456");
        assert_eq!(draw_sprocket["fields"]["last_swap_context"], "02001000");
        assert_eq!(draw_sprocket["fields"]["swap_count"], "3");
        assert_eq!(draw_sprocket["fields"]["fade_count"], "1");
        assert_eq!(draw_sprocket["fields"]["frequency"], "3932160");
        assert_eq!(draw_sprocket["fields"]["width"], "640");
        assert_eq!(draw_sprocket["fields"]["height"], "480");
        assert_eq!(draw_sprocket["fields"]["context_options"], "1");
        assert_eq!(draw_sprocket["fields"]["display_depth_mask"], "16");
        assert_eq!(draw_sprocket["fields"]["back_buffer_depth_mask"], "16");
        assert_eq!(draw_sprocket["fields"]["display_depth"], "16");
        assert_eq!(draw_sprocket["fields"]["back_buffer_depth"], "16");
        assert_eq!(draw_sprocket["fields"]["page_count"], "2");

        let snapshots_path = output_dir.path().join(crate::oracle::ORACLE_SNAPSHOTS_FILE);
        let snapshots = std::fs::read_to_string(snapshots_path).unwrap();
        assert!(
            snapshots.trim().is_empty(),
            "DrawSprocket events should not create screen snapshots"
        );
    }

    #[test]
    fn ppc_gui_cpu_slice_defers_front_buffer_sync_until_composite() {
        let front_base = PPC_HEAP_BASE;
        let presented_base = PPC_HEAP_BASE + 4;
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_CODE_BASE, 0x4800_0002u32.to_be_bytes().to_vec());
        memory.add_region(front_base, vec![0x00, 0x1f, 0x00, 0x1f]);
        memory.add_region(presented_base, vec![0x7c, 0x00, 0x03, 0xe0]);
        let mut cpu = PpcCpu::new();
        cpu.pc = PPC_CODE_BASE;
        cpu.lr = PPC_HALT_PC;
        cpu.gpr[1] = PPC_STACK_TOP - 64;

        let app = LoadedApp::from_ppc(PpcLoadedApp {
            cpu,
            memory,
            entry_pc: PPC_CODE_BASE,
            rtoc: 0,
            stack_base: PPC_STACK_BASE,
            stack_size: PPC_STACK_SIZE,
            stack_pointer: PPC_STACK_TOP - 64,
            tick_state: SharedProcessTickState::default(),
            clock_cycles_per_tick: 1,
            clock_cycle_phase: 0,
            native_exception_handler: 0,
            native_exception_stack: Vec::new(),
            stdc_qsort_stack: Vec::new(),
            dialog_callback_stack: Vec::new(),
            apple_events: Default::default(),
            cfm: Some(crate::cfm::CfmState::default()),
            controls: Default::default(),
            screen_clut: SharedProcessValue::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
            display_gamma: SharedProcessDisplayGamma::default(),
            process_quickdraw_port_state_attached: false,
            color_manager_clut: SharedProcessValue::from_value(
                TrapDispatcher::standard_mac_8bpp_clut(),
            ),
            aliases: Vec::new(),
            gworlds: vec![
                PpcGWorldRecord {
                    ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                    port: PPC_MAIN_GWORLD,
                    pixmap_handle: 0,
                    pixmap: 0,
                    base_addr: front_base,
                    gdevice: PPC_MAIN_GDEVICE,
                    width: 2,
                    height: 1,
                    depth: 16,
                    row_bytes: 4,
                    pixels_locked: false,
                    pixels_no_purge: false,
                },
                PpcGWorldRecord {
                    ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                    port: PPC_DSP_BACK_GWORLD,
                    pixmap_handle: 0,
                    pixmap: 0,
                    base_addr: presented_base,
                    gdevice: PPC_MAIN_GDEVICE,
                    width: 2,
                    height: 1,
                    depth: 16,
                    row_bytes: 4,
                    pixels_locked: false,
                    pixels_no_purge: false,
                },
            ],
            gworld_pixel_states: crate::process_context::SharedProcessQuickDrawPixelStates::default(
            ),
            q3_objects: Vec::new(),
            q3_object_refs: Vec::new(),
            next_q3_object: 0,
            q3_error_state: Default::default(),
            q3_lifecycle: Default::default(),
            q3_memory_storages: Vec::new(),
            q3_files: Vec::new(),
            q3_group_memberships: Vec::new(),
            q3_file_groups: Vec::new(),
            q3_views: Vec::new(),
            q3_submissions: Vec::new(),
            q3_view_transforms: Vec::new(),
            q3_submission_transforms: Vec::new(),
            q3_view_materials: Vec::new(),
            q3_submission_materials: Vec::new(),
            q3_submission_lights: Vec::new(),
            q3_view_state_stack: Vec::new(),
            q3_completed_frames: Vec::new(),
            q3_retained_frames: Vec::new(),
            q3_state_only_completed_frame_batches: Vec::new(),
            q3_fog_styles: Vec::new(),
            q3_attributes: Vec::new(),
            q3_shader_uv_transforms: Vec::new(),
            q3_shader_boundaries: Vec::new(),
            q3_mipmap_textures: Vec::new(),
            q3_texture_shaders: Vec::new(),
            q3_renderer_preferences: Vec::new(),
            q3_draw_contexts: Vec::new(),
            q3_trimeshes: Vec::new(),
            q3_styles: Vec::new(),
            q3_cameras: Vec::new(),
            q3_lights: Vec::new(),
            input_sprocket: Default::default(),
            input_sprocket_virtual_elements: Vec::new(),
            toolbox_startup: Default::default(),
            quicktime: Default::default(),
            sound: Default::default(),
            timer_tasks: Default::default(),
            vbl_tasks: Default::default(),
            callback_scheduling: Default::default(),
            process_file_system: ppc_initial_process_file_system(),
            current_gworld: SharedProcessValue::from_value(PPC_MAIN_GWORLD),
            current_gdevice: SharedProcessValue::from_value(PPC_MAIN_GDEVICE),
            quickdraw_op_colors: SharedProcessValue::default(),
            quickdraw_hilite_colors: SharedProcessValue::default(),
            quickdraw_fore_color: PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            },
            quickdraw_fore_indices: Default::default(),
            quickdraw_back_color: PpcRgbColor {
                red: 0xffff,
                green: 0xffff,
                blue: 0xffff,
            },
            quickdraw_pen_h: 0,
            quickdraw_pen_v: 0,
            quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
            quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
            cursor_state: crate::process_context::SharedProcessCursorState::default(),
            param_text: Default::default(),
            scrap: Default::default(),
            list_manager: Default::default(),
            halt_pc: PPC_HALT_PC,
            import_trap_base: PPC_IMPORT_TRAP_BASE,
            import_count: 0,
            imports: Vec::new(),
            section_bases: Vec::new(),
            input: PpcInputSnapshot::default(),
            process_input: Default::default(),
            event_queue: Default::default(),
            window_list: Default::default(),
            process_memory_manager: PpcProcessMemoryManager::with_heap(
                PPC_HEAP_BASE + 8,
                PPC_STACK_BASE,
            ),
            draw_sprocket: PpcDrawSprocketState {
                front_buffer_gworld: PPC_DSP_BACK_GWORLD,
                back_buffer_gworld: PPC_MAIN_GWORLD,
                swap_count: 1,
                ..PpcDrawSprocketState::default()
            },
        });
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let initial_screen_mode = runner.dispatcher.screen_mode;

        let (steps, running) = runner.run_gui_cpu_slice(8, u32::MAX);

        assert_eq!(steps, 1);
        assert!(!running);
        assert_eq!(
            runner.dispatcher.screen_mode, initial_screen_mode,
            "CPU-only slices must not copy or resize the host framebuffer"
        );

        runner.composite_frame();

        let (host_base, row_bytes, width, height, depth) = runner.dispatcher.screen_mode;
        assert_eq!((row_bytes, width, height, depth), (4, 2, 1, 16));
        assert_eq!(runner.bus.read_word(host_base), 0x7c00);
        assert_eq!(runner.bus.read_word(host_base + 2), 0x03e0);
        assert_eq!(
            runner
                .bus
                .read_long(crate::memory::globals::addr::SCRN_BASE),
            host_base
        );
        assert_eq!(
            runner
                .bus
                .read_long(crate::memory::globals::addr::SCREEN_BITS),
            host_base
        );
        assert_eq!(
            runner
                .bus
                .read_word(crate::memory::globals::addr::SCREEN_BITS + 4),
            row_bytes as u16
        );

        let main_gdevice_handle = runner.dispatcher.main_gdevice_handle;
        assert_ne!(main_gdevice_handle, 0);
        let main_gdevice = runner.bus.read_long(main_gdevice_handle);
        let main_pixmap_handle = runner.bus.read_long(main_gdevice + 22);
        let main_pixmap = runner.bus.read_long(main_pixmap_handle);
        assert_eq!(runner.bus.read_long(main_pixmap), host_base);
        assert_eq!(
            runner.bus.read_word(main_pixmap + 4),
            0x8000 | row_bytes as u16
        );
        assert_eq!(runner.bus.read_word(main_pixmap + 10), height);
        assert_eq!(runner.bus.read_word(main_pixmap + 12), width);
        assert_eq!(runner.bus.read_word(main_pixmap + 30), 16);
        assert_eq!(runner.bus.read_word(main_pixmap + 32), depth);
        assert_eq!(runner.bus.read_word(main_pixmap + 34), 3);
        assert_eq!(runner.bus.read_word(main_pixmap + 36), 5);
        assert_eq!(runner.bus.read_long(main_pixmap + 42), 0);
        assert_eq!(runner.bus.read_word(main_gdevice + 4), 2);
        assert_eq!(runner.bus.read_word(main_gdevice + 38), height);
        assert_eq!(runner.bus.read_word(main_gdevice + 40), width);
        assert_eq!(
            runner.bus.read_long(main_gdevice + 42),
            u32::from(crate::display::classic_depth_mode(depth).unwrap())
        );

        let mut native_context = runner
            .native
            .take(NativeEngineRole::Application)
            .expect("PPC app should stay loaded");
        let mut ppc_app = native_context.adapter_mut();
        ppc_app.draw_sprocket.last_fade_percent = Some(0);
        ppc_app.draw_sprocket.last_fade_zero_color = None;
        assert_eq!(ppc_app.memory.read_u16_be(presented_base), Some(0x7c00));
        assert_eq!(ppc_app.memory.read_u16_be(presented_base + 2), Some(0x03e0));

        runner.sync_ppc_front_buffer_to_host(&mut ppc_app);

        assert_eq!(runner.bus.read_word(host_base), 0x0000);
        assert_eq!(runner.bus.read_word(host_base + 2), 0x0000);
        assert_eq!(ppc_app.memory.read_u16_be(presented_base), Some(0x7c00));
        assert_eq!(ppc_app.memory.read_u16_be(presented_base + 2), Some(0x03e0));
        runner
            .native
            .restore(native_context)
            .unwrap_or_else(|_| panic!("native context lost its owner"));
    }

    #[test]
    fn ppc_completed_q3_frame_renders_before_host_front_buffer_sync() {
        const TRIMESH_NUM_TRIANGLES_OFFSET: u32 = 4;
        const TRIMESH_TRIANGLES_OFFSET: u32 = 8;
        const TRIMESH_NUM_POINTS_OFFSET: u32 = 36;
        const TRIMESH_POINTS_OFFSET: u32 = 40;

        let front_base = PPC_HEAP_BASE;
        let trimesh_data = PPC_DATA_BASE;
        let triangles_ptr = PPC_DATA_BASE + 0x80;
        let points_ptr = PPC_DATA_BASE + 0xc0;
        let view = 0x0100_0000;
        let identity = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_CODE_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
        memory.add_region(front_base, vec![0; 8 * 16]);
        memory.add_region(PPC_DATA_BASE, vec![0; 0x200]);
        memory
            .write_u32_be(trimesh_data + TRIMESH_NUM_TRIANGLES_OFFSET, 1)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + TRIMESH_TRIANGLES_OFFSET, triangles_ptr)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + TRIMESH_NUM_POINTS_OFFSET, 3)
            .unwrap();
        memory
            .write_u32_be(trimesh_data + TRIMESH_POINTS_OFFSET, points_ptr)
            .unwrap();
        for (offset, value) in [
            (0, 0.0f32),
            (4, 0.0),
            (8, 0.0),
            (12, 0.0),
            (16, 0.9),
            (20, 0.0),
            (24, 0.9),
            (28, 0.0),
            (32, 0.0),
        ] {
            memory
                .write_u32_be(points_ptr + offset, value.to_bits())
                .unwrap();
        }
        memory.write_u32_be(triangles_ptr, 0).unwrap();
        memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
        memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
        let mut cpu = PpcCpu::new();
        cpu.pc = PPC_CODE_BASE;
        cpu.lr = PPC_HALT_PC;
        cpu.gpr[1] = PPC_STACK_TOP - 64;

        let app = LoadedApp::from_ppc(PpcLoadedApp {
            cpu,
            memory,
            entry_pc: PPC_CODE_BASE,
            rtoc: 0,
            stack_base: PPC_STACK_BASE,
            stack_size: PPC_STACK_SIZE,
            stack_pointer: PPC_STACK_TOP - 64,
            tick_state: SharedProcessTickState::default(),
            clock_cycles_per_tick: 1,
            clock_cycle_phase: 0,
            native_exception_handler: 0,
            native_exception_stack: Vec::new(),
            stdc_qsort_stack: Vec::new(),
            dialog_callback_stack: Vec::new(),
            apple_events: Default::default(),
            cfm: Some(crate::cfm::CfmState::default()),
            controls: Default::default(),
            screen_clut: SharedProcessValue::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
            display_gamma: SharedProcessDisplayGamma::default(),
            process_quickdraw_port_state_attached: false,
            color_manager_clut: SharedProcessValue::from_value(
                TrapDispatcher::standard_mac_8bpp_clut(),
            ),
            aliases: Vec::new(),
            gworlds: vec![PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: PPC_MAIN_GWORLD,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: front_base,
                gdevice: PPC_MAIN_GDEVICE,
                width: 8,
                height: 8,
                depth: 16,
                row_bytes: 16,
                pixels_locked: false,
                pixels_no_purge: false,
            }],
            gworld_pixel_states: crate::process_context::SharedProcessQuickDrawPixelStates::default(
            ),
            q3_objects: Vec::new(),
            q3_object_refs: Vec::new(),
            next_q3_object: 0,
            q3_error_state: Default::default(),
            q3_lifecycle: Default::default(),
            q3_memory_storages: Vec::new(),
            q3_files: Vec::new(),
            q3_group_memberships: Vec::new(),
            q3_file_groups: Vec::new(),
            q3_views: vec![PpcQ3ViewStateRecord {
                view,
                renderer: 0,
                light_group: 0,
                draw_context: 0,
                camera: 0,
                rendering_depth: 0,
                bounding_box_depth: 0,
                cancelled: false,
            }],
            q3_submissions: Vec::new(),
            q3_view_transforms: Vec::new(),
            q3_submission_transforms: Vec::new(),
            q3_view_materials: Vec::new(),
            q3_submission_materials: Vec::new(),
            q3_submission_lights: Vec::new(),
            q3_view_state_stack: Vec::new(),
            q3_completed_frames: vec![PpcQ3CompletedFrameRecord {
                view,
                submissions: vec![PpcQ3SubmissionRecord {
                    view,
                    kind: PpcQ3SubmissionKind::TriMesh,
                    primary: trimesh_data,
                    secondary: 0,
                }],
                submission_transforms: vec![PpcQ3SubmissionTransformRecord {
                    view,
                    kind: PpcQ3SubmissionKind::TriMesh,
                    primary: trimesh_data,
                    secondary: 0,
                    local_to_world: identity,
                }],
                submission_materials: vec![PpcQ3SubmissionMaterialRecord {
                    view,
                    kind: PpcQ3SubmissionKind::TriMesh,
                    primary: trimesh_data,
                    secondary: 0,
                    shader: 0,
                    illumination_type: u32::from_be_bytes(*b"phil"),
                    styles: Vec::new(),
                    fog_style: None,
                    attributes: Vec::new(),
                    shader_uv_transform: None,
                    shader_boundary: None,
                    texture_shader: None,
                    mipmap_texture: None,
                }],
                submission_lights: vec![PpcQ3SubmissionLightRecord {
                    view,
                    kind: PpcQ3SubmissionKind::TriMesh,
                    primary: trimesh_data,
                    secondary: 0,
                    light_group: 0,
                    lights: Vec::new(),
                }],
                retained_trimeshes: Vec::new(),
            }],
            q3_retained_frames: Vec::new(),
            q3_state_only_completed_frame_batches: Vec::new(),
            q3_fog_styles: Vec::new(),
            q3_attributes: Vec::new(),
            q3_shader_uv_transforms: Vec::new(),
            q3_shader_boundaries: Vec::new(),
            q3_mipmap_textures: Vec::new(),
            q3_texture_shaders: Vec::new(),
            q3_renderer_preferences: Vec::new(),
            q3_draw_contexts: Vec::new(),
            q3_trimeshes: Vec::new(),
            q3_styles: Vec::new(),
            q3_cameras: Vec::new(),
            q3_lights: Vec::new(),
            input_sprocket: Default::default(),
            input_sprocket_virtual_elements: Vec::new(),
            toolbox_startup: Default::default(),
            quicktime: Default::default(),
            sound: Default::default(),
            timer_tasks: Default::default(),
            vbl_tasks: Default::default(),
            callback_scheduling: Default::default(),
            process_file_system: ppc_initial_process_file_system(),
            current_gworld: SharedProcessValue::from_value(PPC_MAIN_GWORLD),
            current_gdevice: SharedProcessValue::from_value(PPC_MAIN_GDEVICE),
            quickdraw_op_colors: SharedProcessValue::default(),
            quickdraw_hilite_colors: SharedProcessValue::default(),
            quickdraw_fore_color: PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            },
            quickdraw_fore_indices: Default::default(),
            quickdraw_back_color: PpcRgbColor {
                red: 0xffff,
                green: 0xffff,
                blue: 0xffff,
            },
            quickdraw_pen_h: 0,
            quickdraw_pen_v: 0,
            quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
            quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
            cursor_state: crate::process_context::SharedProcessCursorState::default(),
            param_text: Default::default(),
            scrap: Default::default(),
            list_manager: Default::default(),
            halt_pc: PPC_HALT_PC,
            import_trap_base: PPC_IMPORT_TRAP_BASE,
            import_count: 0,
            imports: Vec::new(),
            section_bases: Vec::new(),
            input: PpcInputSnapshot::default(),
            process_input: Default::default(),
            event_queue: Default::default(),
            window_list: Default::default(),
            process_memory_manager: PpcProcessMemoryManager::with_heap(
                PPC_HEAP_BASE + 8 * 16,
                PPC_STACK_BASE,
            ),
            draw_sprocket: PpcDrawSprocketState::default(),
        });
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let (steps, running) = runner.run_steps(8, None);

        assert_eq!(steps, 1);
        assert!(!running);
        let (host_base, row_bytes, width, height, depth) = runner.dispatcher.screen_mode;
        assert_eq!((row_bytes, width, height, depth), (16, 8, 8, 16));
        assert_eq!(
            runner.bus.read_word(host_base + 4 * row_bytes + 4 * 2),
            0x4210 // Default diffuse grey, quantized to the 16-bit front buffer.
        );
        let ppc_app = runner
            .native
            .application()
            .expect("PPC app should stay loaded");
        assert!(ppc_app.q3_completed_frames.is_empty());
        assert_eq!(runner.q3_completed_frame_index, 1);
    }

    #[test]
    fn ppc_host_sync_excludes_scanline_padding_from_visible_rows() {
        for (depth, padded_row_bytes, visible_row_bytes) in [
            (1, 96, 80),
            (2, 176, 160),
            (4, 336, 320),
            (8, 656, 640),
            (16, 1296, 1280),
        ] {
            let front_buffer = PpcFrontBuffer {
                base_addr: 0x1000,
                row_bytes: padded_row_bytes,
                width: 640,
                height: 480,
                depth,
            };
            assert_eq!(
                FixtureRunner::ppc_front_buffer_visible_row_bytes(front_buffer),
                Some(visible_row_bytes),
                "{depth}bpp visible row"
            );
        }
    }

    #[test]
    fn ppc_packed_indexed_row_copy_preserves_neighbors_at_non_byte_offsets() {
        let mut one_bit = [0u8; 2];
        assert!(FixtureRunner::copy_ppc_packed_indexed_row(
            &[0b1010_0000],
            1,
            4,
            &mut one_bit,
            3,
        ));
        assert_eq!(one_bit, [0x14, 0x00]);

        let mut two_bit = [0xffu8; 3];
        assert!(FixtureRunner::copy_ppc_packed_indexed_row(
            &[0b00_01_10_11, 0b01_00_00_00],
            2,
            5,
            &mut two_bit,
            1,
        ));
        assert_eq!(two_bit, [0xc6, 0xdf, 0xff]);

        let mut four_bit = [0xaau8; 3];
        assert!(FixtureRunner::copy_ppc_packed_indexed_row(
            &[0x12, 0x30],
            4,
            3,
            &mut four_bit,
            1,
        ));
        assert_eq!(four_bit, [0xa1, 0x23, 0xaa]);
    }

    #[test]
    fn ppc_indexed_matte_repeats_darkest_representable_clut_index() {
        for depth in [1u16, 2, 4, 8] {
            let (clut, _) =
                TrapDispatcher::standard_mac_indexed_clut(depth).expect("standard indexed depth");
            assert_eq!(
                FixtureRunner::ppc_indexed_matte_byte(u32::from(depth), &clut),
                Some(0xff),
                "{depth}bpp standard black index"
            );
        }

        let mut clut = [[0u16; 3]; 256];
        clut[0] = [0xffff, 0xffff, 0xffff];
        clut[1] = [0xaaaa, 0xaaaa, 0xaaaa];
        clut[2] = [0x0000, 0x0000, 0x0000];
        clut[3] = [0x5555, 0x5555, 0x5555];
        assert_eq!(FixtureRunner::ppc_indexed_matte_byte(2, &clut), Some(0xaa));
    }

    #[test]
    fn ppc_host_clut_sync_grows_a_lower_depth_main_color_table() {
        let config = FixtureRunnerConfig::default()
            .with_screen_depth(1)
            .expect("1bpp mode");
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
        let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
        let gdevice = runner.bus.read_long(gdevice_handle);
        let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
        let color_table_handle = runner.bus.read_long(pixmap + 42);
        let old_color_table = runner.bus.read_long(color_table_handle);
        let (mut clut, _) = TrapDispatcher::standard_mac_indexed_clut(4).expect("4bpp CLUT");
        clut[7] = [0x1234, 0x5678, 0x9abc];

        assert!(runner.sync_ppc_host_indexed_color_table(4, &clut));

        let color_table = runner.bus.read_long(color_table_handle);
        assert_ne!(color_table, old_color_table);
        assert_eq!(runner.bus.get_alloc_size(old_color_table), None);
        assert_eq!(runner.bus.get_alloc_size(color_table), Some(8 + 16 * 8));
        assert_eq!(runner.bus.read_word(color_table + 6), 15);
        assert_eq!(
            [
                runner.bus.read_word(color_table + 8 + 7 * 8 + 2),
                runner.bus.read_word(color_table + 8 + 7 * 8 + 4),
                runner.bus.read_word(color_table + 8 + 7 * 8 + 6),
            ],
            clut[7]
        );
    }

    #[test]
    fn ppc_host_sync_restores_indexed_pm_table_across_direct_color_transitions() {
        const WIDTH: u32 = 8;
        const HEIGHT: u32 = 1;

        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        let ppc_app = app.ppc.as_mut().expect("PPC app");
        ppc_app.memory.add_region(PPC_HEAP_BASE, vec![0; 16]);
        ppc_app.set_heap_cursor(PPC_HEAP_BASE + 16);
        ppc_app.gworlds.push(PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: PPC_HEAP_BASE,
            gdevice: PPC_MAIN_GDEVICE,
            width: WIDTH,
            height: HEIGHT,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        });

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let gdevice_handle = runner.dispatcher.ensure_main_gdevice(&mut runner.bus);
        let gdevice = runner.bus.read_long(gdevice_handle);
        let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
        let indexed_ctab_handle = runner.bus.read_long(pixmap + 42);
        assert_ne!(indexed_ctab_handle, 0);

        runner.sync_ppc_front_buffer_to_host(ppc_app);

        assert_eq!(runner.bus.read_word(pixmap + 30), 16);
        assert_eq!(runner.bus.read_word(pixmap + 32), 16);
        assert_eq!(runner.bus.read_word(pixmap + 34), 3);
        assert_eq!(runner.bus.read_word(pixmap + 36), 5);
        assert_eq!(runner.bus.read_long(pixmap + 42), 0);
        assert_eq!(runner.bus.read_long(gdevice + 42), 0x0084);
        let host_mirror_base = runner.ppc_host_mirror_base;
        let host_mirror_capacity = runner.ppc_host_mirror_capacity;
        assert_eq!(runner.dispatcher.screen_mode.0, host_mirror_base);
        assert_eq!(host_mirror_capacity, 16);
        assert_eq!(runner.bus.get_alloc_size(host_mirror_base), Some(16));
        let canary = runner.bus.alloc(8);
        runner.bus.fill_bytes(canary, 8, 0x5a);

        let (two_bit_clut, _) = TrapDispatcher::standard_mac_indexed_clut(2).expect("2bpp CLUT");
        ppc_app.screen_clut.replace(two_bit_clut);
        ppc_app.color_manager_clut.replace(two_bit_clut);
        ppc_app.gworlds[0].depth = 2;
        ppc_app.gworlds[0].row_bytes = 2;

        runner.sync_ppc_front_buffer_to_host(ppc_app);

        assert_eq!(runner.bus.read_word(pixmap + 30), 0);
        assert_eq!(runner.bus.read_word(pixmap + 32), 2);
        assert_eq!(runner.bus.read_word(pixmap + 34), 1);
        assert_eq!(runner.bus.read_word(pixmap + 36), 2);
        assert_eq!(runner.bus.read_long(pixmap + 42), indexed_ctab_handle);
        let indexed_ctab = runner.bus.read_long(indexed_ctab_handle);
        assert_ne!(indexed_ctab, 0);
        assert_eq!(runner.bus.read_word(indexed_ctab + 6), 3);
        assert_eq!(runner.bus.read_long(gdevice + 42), 0x0081);
        let heap_after_first_transition = runner.bus.heap_bump_ptr();

        for _ in 0..8 {
            ppc_app.gworlds[0].depth = 16;
            ppc_app.gworlds[0].row_bytes = 16;
            runner.sync_ppc_front_buffer_to_host(ppc_app);
            assert_eq!(runner.dispatcher.screen_mode.0, host_mirror_base);
            assert_eq!(runner.ppc_host_mirror_base, host_mirror_base);
            assert_eq!(runner.ppc_host_mirror_capacity, host_mirror_capacity);

            ppc_app.gworlds[0].depth = 2;
            ppc_app.gworlds[0].row_bytes = 2;
            runner.sync_ppc_front_buffer_to_host(ppc_app);
            assert_eq!(runner.dispatcher.screen_mode.0, host_mirror_base);
            assert_eq!(runner.ppc_host_mirror_base, host_mirror_base);
            assert_eq!(runner.ppc_host_mirror_capacity, host_mirror_capacity);
        }

        ppc_app.gworlds[0].depth = 16;
        ppc_app.gworlds[0].row_bytes = 16;
        runner.sync_ppc_front_buffer_to_host(ppc_app);

        assert_eq!(runner.bus.read_word(pixmap + 30), 16);
        assert_eq!(runner.bus.read_word(pixmap + 32), 16);
        assert_eq!(runner.bus.read_word(pixmap + 34), 3);
        assert_eq!(runner.bus.read_word(pixmap + 36), 5);
        assert_eq!(runner.bus.read_long(pixmap + 42), 0);
        assert_eq!(runner.ppc_host_indexed_ctab_handle, indexed_ctab_handle);
        assert_eq!(runner.bus.read_word(indexed_ctab + 6), 3);
        assert_eq!(runner.bus.read_long(gdevice + 42), 0x0084);
        assert_eq!(runner.bus.get_alloc_size(host_mirror_base), Some(16));
        assert_eq!(runner.bus.read_bytes(canary, 8), vec![0x5a; 8]);
        assert_eq!(runner.bus.heap_bump_ptr(), heap_after_first_transition);
    }

    #[test]
    fn ppc_packed_front_buffers_sync_centered_pixels_with_process_color_state() {
        const WIDTH: u32 = 513;
        const HEIGHT: u32 = 342;
        const CANVAS_WIDTH: u32 = 800;
        const CANVAS_HEIGHT: u32 = 600;
        const DESTINATION_X: u32 = (CANVAS_WIDTH - WIDTH) / 2;
        const DESTINATION_Y: u32 = (CANVAS_HEIGHT - HEIGHT) / 2;

        for (depth, source_pixels) in [
            (1u16, [1u8, 0, 1, 0]),
            (2u16, [0u8, 1, 2, 3]),
            (4u16, [1u8, 2, 3, 0]),
        ] {
            let row_bytes = WIDTH.checked_mul(u32::from(depth)).unwrap().div_ceil(8);
            let mut pixels = vec![0u8; (row_bytes * HEIGHT) as usize];
            let field_mask = ((1u16 << depth) - 1) as u8;
            for (x, pixel) in source_pixels.into_iter().enumerate() {
                let bit = x as u32 * u32::from(depth);
                let shift = 8 - u32::from(depth) - (bit & 7);
                pixels[(bit / 8) as usize] |= (pixel & field_mask) << shift;
            }

            let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
            let ppc_app = app.ppc.as_mut().expect("PPC app");
            ppc_app.memory.add_region(PPC_HEAP_BASE, pixels);
            ppc_app.set_heap_cursor(PPC_HEAP_BASE + row_bytes * HEIGHT);
            ppc_app.gworlds.push(PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: PPC_MAIN_GWORLD,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: PPC_HEAP_BASE,
                gdevice: PPC_MAIN_GDEVICE,
                width: WIDTH,
                height: HEIGHT,
                depth: u32::from(depth),
                row_bytes,
                pixels_locked: false,
                pixels_no_purge: false,
            });
            let config = FixtureRunnerConfig::default()
                .with_screen_depth(depth)
                .expect("packed indexed mode");
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, config);
            let mut ppc_app = app.ppc.take().expect("PPC app");
            ppc_app.attach_unconverted_process_services(&mut runner.process_context);
            let (mut device_clut, _) =
                TrapDispatcher::standard_mac_indexed_clut(depth).expect("standard indexed depth");
            device_clut[0][0] = 0xfffe;
            let mut color_manager_clut = device_clut;
            color_manager_clut[0][1] = 0xfffd;
            ppc_app.screen_clut.replace(device_clut);
            ppc_app.color_manager_clut.replace(color_manager_clut);
            ppc_app
                .display_gamma
                .install(crate::display::linear_display_gamma());
            runner.sync_ppc_front_buffer_to_host(&mut ppc_app);

            let (base, host_row_bytes, width, height, host_depth) = runner.dispatcher.screen_mode;
            assert_eq!(
                (width, height, host_depth),
                (CANVAS_WIDTH as u16, CANVAS_HEIGHT as u16, depth)
            );
            assert_eq!(
                host_row_bytes,
                CANVAS_WIDTH * u32::from(depth) / 8,
                "{depth}bpp packed canvas stride"
            );
            assert_eq!(runner.dispatcher.device_clut, device_clut);
            assert_eq!(runner.dispatcher.color_manager_clut, color_manager_clut);
            assert_eq!(
                runner.dispatcher.device_gamma(),
                crate::display::linear_display_gamma()
            );
            let gdevice = runner.bus.read_long(runner.dispatcher.main_gdevice_handle);
            let pixmap = runner.bus.read_long(runner.bus.read_long(gdevice + 22));
            let color_table = runner.bus.read_long(runner.bus.read_long(pixmap + 42));
            assert_eq!(runner.bus.read_word(color_table + 6), (1u16 << depth) - 1);
            for index in [0u32, (1u32 << depth) - 1] {
                let entry = color_table + 8 + index * 8;
                assert_eq!(
                    [
                        runner.bus.read_word(entry + 2),
                        runner.bus.read_word(entry + 4),
                        runner.bus.read_word(entry + 6),
                    ],
                    device_clut[index as usize],
                    "{depth}bpp host GDevice CLUT entry {index}"
                );
            }

            let pixel_at = |x: u32| {
                let bit = x * u32::from(depth);
                let packed = runner
                    .bus
                    .read_byte(base + DESTINATION_Y * host_row_bytes + bit / 8);
                let shift = 8 - u32::from(depth) - (bit & 7);
                (packed >> shift) & field_mask
            };
            assert_eq!(pixel_at(DESTINATION_X - 1), field_mask);
            for (offset, expected) in source_pixels.into_iter().enumerate() {
                assert_eq!(
                    pixel_at(DESTINATION_X + offset as u32),
                    expected,
                    "{depth}bpp source pixel {offset}"
                );
            }
            assert_eq!(pixel_at(DESTINATION_X + WIDTH), field_mask);
        }
    }

    #[test]
    fn arrows_as_numpad_remaps_key_and_char_together() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_arrows_as_numpad(true);

        assert_eq!(runner.remap_key(0x7B, 28), (0x56, b'4'));
        assert_eq!(runner.remap_key(0x7C, 29), (0x58, b'6'));
        assert_eq!(runner.remap_key(0x7D, 31), (0x57, b'5'));
        assert_eq!(runner.remap_key(0x7E, 30), (0x5B, b'8'));
        assert_eq!(runner.remap_key(0x2E, b'm'), (0x2E, b'm'));
    }

    #[test]
    fn arrows_not_remapped_by_default() {
        let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        assert!(!runner.arrows_as_numpad());
        assert_eq!(runner.remap_key(0x7B, 28), (0x7B, 28));
        assert_eq!(runner.remap_key(0x7C, 29), (0x7C, 29));
        assert_eq!(runner.remap_key(0x2E, b'm'), (0x2E, b'm'));
    }

    #[test]
    fn key_events_sync_low_memory_keymap() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM), 0);
        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 4), 0);
        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 6), 0);
        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 15), 0);

        runner.push_key_down(0x00, b'a');
        runner.push_key_down(0x26, b'j');
        runner.push_key_down(0x31, b' ');
        runner.push_key_down(0x7E, 30);

        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM), 0x01);
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 4),
            0x40,
            "J key should be visible to byte/bit KeyMap readers"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 5),
            0,
            "J key should not alias M at KeyMapLM byte 5"
        );
        assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 6), 0x02);
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 15),
            0x40,
            "up arrow should be visible to byte/bit KeyMap readers"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 14),
            0,
            "up arrow should not be mirrored into the unused raw byte"
        );

        runner.push_key_up(0x26, b'j');

        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 4),
            0,
            "J key release should clear the low-memory mirror"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 5),
            0,
            "J key release should leave the M-key byte clear"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 15),
            0x40,
            "unrelated byte/bit down keys should remain mirrored"
        );
        assert_eq!(
            runner.bus.read_byte(addr::KEY_MAP_LM + 14),
            0,
            "unused raw byte should stay clear"
        );
    }

    #[test]
    fn caps_lock_latch_is_preserved_in_low_memory_keymap() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let caps_lock_byte = addr::KEY_MAP_LM + 7;

        runner.push_key_down(0x39, 0);
        assert_eq!(runner.bus.read_byte(caps_lock_byte) & 0x02, 0x02);
        runner.push_key_up(0x39, 0);
        assert_eq!(
            runner.bus.read_byte(caps_lock_byte) & 0x02,
            0x02,
            "physical release must keep the low-memory Caps Lock bit latched"
        );

        runner.push_key_down(0x39, 0);
        assert_eq!(runner.bus.read_byte(caps_lock_byte) & 0x02, 0);
        runner.push_key_up(0x39, 0);
        assert_eq!(runner.bus.read_byte(caps_lock_byte) & 0x02, 0);
    }

    #[test]
    fn init_app_zeroes_fresh_top_of_stack() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        let stack_seed_start = app.initial_sp.saturating_sub(0x8000);
        assert_eq!(
            runner.bus.read_long(stack_seed_start),
            0,
            "fresh process stack should match a newly initialized application partition"
        );
    }

    #[test]
    fn init_app_leaves_application_heap_room_below_appllimit() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        let heap_end = runner.bus.read_long(addr::HEAP_END);
        let appl_limit = runner.bus.read_long(addr::APPL_LIMIT);
        assert_eq!(
            heap_end,
            0x0020_0000 + APP_ZONE_HEADER_SIZE,
            "HeapEnd should expose the initial application-zone extent"
        );
        assert!(
            appl_limit.saturating_sub(heap_end) >= 2300 * 1024,
            "direct low-memory startup checks should see growable heap room below ApplLimit"
        );
    }

    #[test]
    fn init_app_honors_size_resource_preferred_partition_for_heap_reporting() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let preferred_partition = 3 * 1024 * 1024;
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: Some(ApplicationSizeResource {
                flags: 0x0080,
                preferred_size: preferred_partition,
                minimum_size: 2 * 1024 * 1024,
            }),
        };

        runner.init_app(&app);

        let expected_limit = 0x0020_0000 + preferred_partition - APP_STACK_SAFETY_MARGIN;
        let expected_free = expected_limit - (0x0020_0000 + APP_ZONE_HEADER_SIZE);
        assert_eq!(runner.bus.read_long(addr::APPL_LIMIT), expected_limit);
        assert_eq!(runner.bus.read_long(addr::BUF_PTR), expected_limit);
        assert_eq!(runner.bus.read_long(0x0020_0000), expected_limit);
        assert_eq!(runner.bus.read_long(0x0020_0000 + 12), expected_free);
        assert_eq!(
            crate::memory::app_heap_free_bytes(runner.bus()),
            expected_free
        );
        assert!(
            expected_free < crate::memory::APP_HEAP_COMPAT_FREE_FLOOR,
            "explicit SIZE partitions must bypass the compatibility floor"
        );
    }

    #[test]
    fn init_app_application_partition_override_takes_precedence_over_size_resource() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let size_partition = 3 * 1024 * 1024;
        let override_partition = 4 * 1024 * 1024;
        runner.set_application_partition_size(Some(override_partition));
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: Some(ApplicationSizeResource {
                flags: 0x0080,
                preferred_size: size_partition,
                minimum_size: 2 * 1024 * 1024,
            }),
        };

        runner.init_app(&app);

        let expected_limit = 0x0020_0000 + override_partition - APP_STACK_SAFETY_MARGIN;
        assert_eq!(runner.bus.read_long(addr::APPL_LIMIT), expected_limit);
        assert_eq!(
            crate::memory::app_heap_free_bytes(runner.bus()),
            expected_limit - (0x0020_0000 + APP_ZONE_HEADER_SIZE)
        );
    }

    #[test]
    fn init_app_ignores_too_small_application_partition_override() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_application_partition_size(Some(64 * 1024));
        assert_eq!(runner.application_partition_size(), None);
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_long(addr::APPL_LIMIT),
            app.initial_sp - APP_STACK_SAFETY_MARGIN,
            "invalid tiny overrides must fall back to the default launch limit"
        );
    }

    #[test]
    fn init_app_seeds_appparmhandle_with_empty_finder_information() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher_mut()
            .set_launched_app_path("Games/Armor Alley");
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        let handle = runner.bus.read_long(addr::APP_PARM_HANDLE);
        assert_ne!(
            handle, 0,
            "AppParmHandle should point at Finder launch information"
        );
        let data_ptr = runner.bus.read_long(handle);
        assert_ne!(
            data_ptr, 0,
            "Finder launch information handle should be loaded"
        );
        assert_eq!(
            runner.bus.get_alloc_size(data_ptr),
            Some(4),
            "empty Finder launch information is message/count only"
        );
        assert_eq!(
            runner.bus.read_word(data_ptr),
            0,
            "message should be appOpen for a normal application launch"
        );
        assert_eq!(
            runner.bus.read_word(data_ptr + 2),
            0,
            "normal application launch has no selected documents"
        );
    }

    #[test]
    fn init_app_seeds_current_application_fcb_low_memory_state() {
        use crate::memory::globals::addr;

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher_mut()
            .vfs_rsrc
            .insert("Games/Armor Alley".to_string(), vec![0xA5; 1234]);
        runner
            .dispatcher_mut()
            .set_vfs_entry_metadata("Games/Armor Alley", *b"APPL", *b"TEST", 0);
        runner
            .dispatcher_mut()
            .set_launched_app_path("Games/Armor Alley");
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_word(addr::CUR_APREF_NUM),
            APPLICATION_RESOURCE_REFNUM,
            "CurApRefNum should be the app resource fork access path"
        );
        assert_eq!(
            runner.bus.read_word(addr::FS_FCB_LEN),
            HFS_FCB_SIZE,
            "System 7 FCB size should be exposed for direct low-memory readers"
        );
        let fcb_buffer = runner.bus.read_long(addr::FCB_S_PTR);
        assert_ne!(fcb_buffer, 0, "FCBSPtr should point to an FCB buffer");
        assert_eq!(
            runner.bus.read_word(fcb_buffer),
            HFS_FCB_BUFFER_SIZE,
            "FCB buffer length should include the leading length word"
        );
        let fcb = fcb_buffer + APPLICATION_RESOURCE_REFNUM as u32;
        assert_eq!(
            runner.bus.read_word(fcb + 4),
            0x0200,
            "the application access path should describe a resource fork"
        );
        assert_eq!(runner.bus.read_long(fcb + 8), 1234);
        assert_eq!(runner.bus.read_long(fcb + 12), 1234);
        assert_eq!(runner.bus.read_long(fcb + 50), u32::from_be_bytes(*b"APPL"));
        assert_eq!(
            runner.bus.read_long(fcb + 58),
            *runner.dispatcher.default_dir_id
        );

        let vcb = runner.bus.read_long(fcb + 20);
        assert_ne!(vcb, 0, "fcbVPtr should point to the boot volume VCB");
        assert_eq!(runner.bus.read_long(addr::DEF_VCB_PTR), vcb);
        assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 2), vcb);
        assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 6), vcb);
        assert_eq!(runner.bus.read_word(vcb + 8), 0x4244);
        assert_eq!(
            runner.bus.read_word(vcb + 78) as i16,
            crate::trap::dispatch::BOOT_VOLUME_REF_NUM
        );
        assert_eq!(
            runner
                .dispatcher
                .open_files
                .get(&APPLICATION_RESOURCE_REFNUM),
            Some(&"__rsrc__Games/Armor Alley".to_string())
        );
    }

    #[test]
    fn init_app_sets_legacy_sound_driver_low_memory_defaults() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::SD_VOLUME),
            1,
            "SdVolume ($0260) should boot to the nonzero legacy compatibility value"
        );
        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::SOUND_LEVEL),
            0,
            "SoundLevel ($027F) is a distinct Sound Driver amplitude byte"
        );
        let sound_base = runner
            .bus
            .read_long(crate::memory::globals::addr::SOUND_BASE);
        assert_eq!(
            sound_base, 0x007F_7880,
            "SoundBase ($0266) should point at the 370-word legacy sound buffer in reserved display memory"
        );
        assert_eq!(
            runner.bus.read_byte(sound_base),
            0x80,
            "legacy SoundBase buffer starts at neutral amplitude"
        );
    }

    #[test]
    fn init_app_materializes_the_device_manager_unit_table() {
        use crate::memory::globals::{addr, DEFAULT_UNIT_TABLE_ENTRY_COUNT};

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        let table = runner.bus.read_long(addr::U_TABLE_BASE);
        assert_ne!(table, 0, "UTableBase should address the unit table");
        assert_eq!(
            runner.bus.read_word(addr::UNIT_NTRY_CNT),
            DEFAULT_UNIT_TABLE_ENTRY_COUNT
        );
        assert_eq!(
            runner.bus.get_alloc_size(table),
            Some(u32::from(DEFAULT_UNIT_TABLE_ENTRY_COUNT) * 4)
        );
        assert!(
            runner
                .bus
                .read_bytes(table, usize::from(DEFAULT_UNIT_TABLE_ENTRY_COUNT) * 4)
                .iter()
                .all(|&byte| byte == 0),
            "the unit table should start with nil DCE handles"
        );
    }

    #[test]
    fn init_app_seeds_mmu32bit_low_memory_flag() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::MMU32_BIT),
            1,
            "MMU32Bit ($0CB2) should mirror Systemless's default 32-bit addressing mode"
        );
    }

    #[test]
    fn init_app_can_start_in_twenty_four_bit_addressing_mode() {
        let mut runner = FixtureRunner::new(
            8 * 1024 * 1024,
            FixtureRunnerConfig {
                addressing_32_bit: false,
                ..FixtureRunnerConfig::default()
            },
        );
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert!(!runner.bus.addressing_32_bit());
        assert_eq!(runner.bus.ram_size(), 8 * 1024 * 1024);
        assert_eq!(runner.dispatcher.mmu_mode, 0);
        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::MMU32_BIT),
            0
        );
    }

    #[test]
    fn twenty_four_bit_runner_caps_guest_ram_at_sixteen_megabytes() {
        let runner = FixtureRunner::new(
            64 * 1024 * 1024,
            FixtureRunnerConfig {
                addressing_32_bit: false,
                ..FixtureRunnerConfig::default()
            },
        );

        assert_eq!(runner.bus.ram_size(), 0x0100_0000);
    }

    #[test]
    fn init_app_seeds_callable_swap_mmu_mode_trap_table_entry() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let entry = runner
            .bus
            .read_long(crate::memory::globals::addr::SWAP_MMU_MODE_TRAP);
        assert_ne!(entry, 0);
        assert_eq!(
            [runner.bus.read_word(entry), runner.bus.read_word(entry + 2),],
            [0xA05D, 0x4E75],
            "the $0574 OS trap-table entry should target SwapMMUMode followed by RTS"
        );

        let call_site = 0x0002_0000u32;
        let initial_sp = 0x007F_FE00u32;
        runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0574).W,A0
        runner.bus.write_word(call_site + 2, 0x0574);
        runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
        runner.bus.write_word(call_site + 6, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, call_site);
        runner.m68k.cpu.write_reg(Register::A7, initial_sp);
        runner.m68k.cpu.write_reg(Register::D0, 0);

        let (steps, running) = runner.run_steps(4, None);

        assert!(running);
        assert_eq!(steps, 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::D0),
            1,
            "SwapMMUMode should return the previous 32-bit mode in D0"
        );
        assert_eq!(
            runner
                .bus
                .read_byte(crate::memory::globals::addr::MMU32_BIT),
            0,
            "the indirect call should update the requested addressing mode"
        );
        runner.bus.write_long(0x0002_1000, 0x1234_5678);
        assert_eq!(
            runner.bus.read_long(0xAB02_1000),
            0x1234_5678,
            "SwapMMUMode must change actual guest address translation"
        );
    }

    #[test]
    fn init_app_seeds_callable_profile_come_from_head() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        let entry = crate::trap::dispatch::OS_TRAP_TABLE_BASE + 0x78 * 4;
        let previous_head = runner.bus.read_long(entry);
        runner
            .dispatcher
            .install_trap_address(&mut runner.bus, 0xA078, 0x0021_1000)
            .unwrap();
        runner.bus.write_long(0x28, 0x0021_2000);
        runner.init_app(&app);
        assert_eq!(
            runner.dispatcher.trap_table_profile,
            Some(TrapTableProfile::M68k68040)
        );
        assert_ne!(runner.bus.read_long(entry), previous_head);
        assert_ne!(
            runner.dispatcher.trap_table_address(&runner.bus, 0xA078),
            Some(0x0021_1000)
        );
        assert!(runner.dispatcher.aline_vector_is_default(&runner.bus));

        let head = runner
            .bus
            .read_long(crate::trap::dispatch::OS_TRAP_TABLE_BASE + 0x78 * 4);
        let successor = runner.bus.read_long(head + 4);
        assert_eq!(runner.bus.read_long(head), 0x6006_4EF9);
        runner.m68k.cpu.write_reg(Register::PC, head);

        let (steps, running) = runner.run_steps(3, None);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), successor);
    }

    #[test]
    fn profile_unimplemented_gateway_executes_system_error_12() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let gateway = runner
            .dispatcher
            .trap_table_address(&runner.bus, 0xAA6E)
            .unwrap();
        assert_eq!(runner.bus.read_word(gateway), 0xAE6E);
        let call_site = 0x0002_0000;
        let initial_sp = 0x007F_FE00;
        runner.bus.write_word(call_site, 0x4E90); // JSR (A0)
        runner.m68k.cpu.write_reg(Register::A0, gateway);
        runner.m68k.cpu.write_reg(Register::A7, initial_sp);
        runner.m68k.cpu.write_reg(Register::PC, call_site);

        let (steps, running) = runner.run_steps(2, None);

        assert_eq!(steps, 2);
        assert!(!running);
        assert_eq!(runner.dispatcher.current_trap_caller, Some(call_site + 2));
        assert_eq!(
            runner
                .bus
                .read_word(crate::memory::globals::addr::DS_ERR_CODE),
            12
        );
    }

    #[test]
    fn init_app_seeds_cursor_task_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };

        runner.init_app(&app);

        assert_eq!(
            runner.bus.read_word(CURSOR_TASK_NOOP_ADDR),
            0x4E75,
            "default cursor task target should be a callable RTS stub"
        );
        assert_eq!(
            runner
                .bus
                .read_long(crate::memory::globals::addr::J_CRSR_TASK),
            CURSOR_TASK_NOOP_ADDR,
            "JCrsrTask ($08EE) should boot to a callable no-op vector"
        );
    }

    #[test]
    fn init_app_seeds_callable_show_cursor_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let entry = runner
            .bus
            .read_long(crate::memory::globals::addr::J_SHOW_CURSOR);
        assert_ne!(entry, 0);
        assert_eq!(
            [runner.bus.read_word(entry), runner.bus.read_word(entry + 2),],
            [0xA853, 0x4E75],
            "JShowCursor should target ShowCursor followed by RTS"
        );

        let call_site = 0x0002_0000u32;
        let initial_sp = 0x007F_FE00u32;
        runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0804).W,A0
        runner.bus.write_word(call_site + 2, 0x0804);
        runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
        runner.bus.write_word(call_site + 6, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, call_site);
        runner.m68k.cpu.write_reg(Register::A7, initial_sp);
        runner.dispatcher.cursor_state.set_level_for_test(-1);

        let (steps, running) = runner.run_steps(4, None);

        assert!(running);
        assert_eq!(steps, 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
        assert_eq!(runner.dispatcher.cursor_level(), 0);
        assert!(runner.dispatcher.cursor_visible());
    }

    #[test]
    fn init_app_seeds_callable_init_cursor_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let entry = runner
            .bus
            .read_long(crate::memory::globals::addr::J_INIT_CRSR);
        assert_ne!(entry, 0);
        assert_eq!(
            [runner.bus.read_word(entry), runner.bus.read_word(entry + 2)],
            [0xA850, 0x4E75],
            "JInitCrsr should target InitCursor followed by RTS"
        );

        let call_site = 0x0002_0000u32;
        let initial_sp = 0x007F_FE00u32;
        runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0814).W,A0
        runner.bus.write_word(call_site + 2, 0x0814);
        runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
        runner.bus.write_word(call_site + 6, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, call_site);
        runner.m68k.cpu.write_reg(Register::A7, initial_sp);
        runner.dispatcher.cursor_state.set_level_for_test(-1);

        let (steps, running) = runner.run_steps(4, None);

        assert!(running);
        assert_eq!(steps, 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
        assert_eq!(runner.dispatcher.cursor_level(), 0);
        assert!(runner.dispatcher.cursor_visible());
    }

    #[test]
    fn init_app_seeds_callable_swap_font_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let swap_font_trampoline = runner
            .bus
            .read_long(crate::memory::globals::addr::J_SWAP_FONT);
        assert_ne!(swap_font_trampoline, 0);
        assert_eq!(
            [
                runner.bus.read_word(swap_font_trampoline),
                runner.bus.read_word(swap_font_trampoline + 2),
                runner.bus.read_word(swap_font_trampoline + 4),
            ],
            [0x205F, 0xA901, 0x4ED0]
        );

        let fm_input_sp = 0x007F_FE00u32;
        let fm_input = 0x0002_1000u32;
        let return_pc = 0x0002_0000u32;
        runner.bus.write_word(fm_input, 3); // family
        runner.bus.write_word(fm_input + 2, 12); // size
        runner.bus.write_byte(fm_input + 4, 0); // face
        runner.bus.write_byte(fm_input + 5, 1); // needBits
        runner.bus.write_word(fm_input + 6, 0); // device
        runner.bus.write_word(fm_input + 8, 1); // numer.v
        runner.bus.write_word(fm_input + 10, 1); // numer.h
        runner.bus.write_word(fm_input + 12, 1); // denom.v
        runner.bus.write_word(fm_input + 14, 1); // denom.h
        runner.bus.write_long(fm_input_sp, fm_input); // CONST VAR inRec
        runner.bus.write_long(fm_input_sp + 4, 0); // result slot
        runner.bus.write_long(fm_input_sp - 4, return_pc);
        runner.bus.write_word(return_pc, 0x4E71); // NOP
        runner
            .m68k
            .cpu
            .write_reg(Register::PC, swap_font_trampoline);
        runner.m68k.cpu.write_reg(Register::A7, fm_input_sp - 4);

        let (steps, running) = runner.run_steps(3, None);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), fm_input_sp + 4);
        assert_ne!(
            runner.bus.read_long(fm_input_sp + 4),
            0,
            "JSwapFont should return a non-NIL FMOutPtr through the Pascal result slot"
        );
    }

    #[test]
    fn init_app_seeds_callable_shield_cursor_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let shield_cursor_trampoline = runner
            .bus
            .read_long(crate::memory::globals::addr::J_SHIELD_CURSOR);
        assert_ne!(shield_cursor_trampoline, 0);
        assert_eq!(
            [
                runner.bus.read_word(shield_cursor_trampoline),
                runner.bus.read_word(shield_cursor_trampoline + 2),
                runner.bus.read_word(shield_cursor_trampoline + 4),
            ],
            [0x205F, 0xA855, 0x4ED0]
        );

        let args_sp = 0x007F_FE00u32;
        let return_pc = 0x0002_0000u32;
        runner.bus.write_word(args_sp, 100); // left
        runner.bus.write_word(args_sp + 2, 120); // top
        runner.bus.write_word(args_sp + 4, 500); // right
        runner.bus.write_word(args_sp + 6, 420); // bottom
        runner.bus.write_long(args_sp - 4, return_pc);
        runner.bus.write_word(return_pc, 0x4E71); // NOP
        runner
            .m68k
            .cpu
            .write_reg(Register::PC, shield_cursor_trampoline);
        runner.m68k.cpu.write_reg(Register::A7, args_sp - 4);

        let (steps, running) = runner.run_steps(3, None);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::A7),
            args_sp + 8,
            "JShieldCursor should consume its four Pascal INTEGER arguments"
        );
    }

    #[test]
    fn init_app_seeds_callable_hide_cursor_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = LoadedApp {
            ppc: None,
            code0_header: Code0Header {
                above_a5: 0,
                below_a5: 0x2000,
                jump_table_size: 0,
                jump_table_offset: 0,
            },
            a5_base: 0x0040_0000,
            jump_table: Vec::new(),
            segment_bases: HashMap::new(),
            loaded_image_end: 0,
            initial_sp: 0x007F_FFC0,
            size_resource: None,
        };
        runner.init_app(&app);

        let hide_cursor_trampoline = runner
            .bus
            .read_long(crate::memory::globals::addr::J_HIDE_CURSOR);
        assert_ne!(hide_cursor_trampoline, 0);
        assert_eq!(
            [
                runner.bus.read_word(hide_cursor_trampoline),
                runner.bus.read_word(hide_cursor_trampoline + 2),
                runner.bus.read_word(hide_cursor_trampoline + 4),
            ],
            [0x205F, 0xA852, 0x4ED0],
            "JHideCursor should pop the JSR return address, trap, and jump back"
        );

        let call_sp = 0x007F_FE00u32;
        let return_pc = 0x0002_0000u32;
        runner.bus.write_long(call_sp - 4, return_pc);
        runner.bus.write_word(return_pc, 0x4E71);
        runner
            .m68k
            .cpu
            .write_reg(Register::PC, hide_cursor_trampoline);
        runner.m68k.cpu.write_reg(Register::A7, call_sp - 4);

        let (steps, running) = runner.run_steps(3, None);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), call_sp);
        assert_eq!(runner.dispatcher().cursor_level(), -1);
    }

    #[test]
    fn cursor_task_noop_vector_does_not_fire_on_guest_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner.bus.write_long(
            crate::memory::globals::addr::J_CRSR_TASK,
            CURSOR_TASK_NOOP_ADDR,
        );
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.advance_guest_tick();

        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cursor_task_trampoline, 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
        assert_eq!(runner.bus.read_long(crate::memory::globals::addr::TICKS), 1);
    }

    #[test]
    fn cursor_task_callback_arms_interrupt_from_low_memory_vector() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = 0x0004_1234;

        runner
            .bus
            .write_long(crate::memory::globals::addr::J_CRSR_TASK, callback_addr);
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.write_reg(Register::D0, 0x1111_1111);
        runner.m68k.cpu.write_reg(Register::D7, 0x7777_7777);
        runner.m68k.cpu.write_reg(Register::A0, 0xAAAA_0000);
        runner.m68k.cpu.write_reg(Register::A6, 0xCCCC_0000);
        runner.m68k.cpu.core.set_ccr(0x04);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2004);

        runner.advance_guest_tick();

        let active = runner
            .active_interrupt_callback
            .expect("cursor task callback should have been armed");
        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::CursorTask
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);
        assert_eq!(active.a_regs[7], interrupted_sp);
        assert_eq!(active.a_regs[6], 0xCCCC_0000);
        assert_eq!(active.d_regs[0], 0x1111_1111);
        assert_eq!(active.d_regs[7], 0x7777_7777);
        assert_eq!(active.sr, 0x2004);
        assert_eq!(active.ccr, 0x04);
        assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2104);

        assert_ne!(runner.cursor_task_trampoline, 0);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            runner.cursor_task_trampoline
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp - 4);
        assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
        assert_eq!(runner.bus.read_word(runner.cursor_task_trampoline), 0x48E7);
        assert_eq!(
            runner.bus.read_word(runner.cursor_task_trampoline + 4),
            0x4EB9
        );
        assert_eq!(
            runner.bus.read_long(runner.cursor_task_trampoline + 6),
            callback_addr
        );
        assert_eq!(
            runner.bus.read_word(runner.cursor_task_trampoline + 10),
            0x4CDF
        );
        assert_eq!(
            runner.bus.read_word(runner.cursor_task_trampoline + 14),
            0x4E75
        );
    }

    #[test]
    fn cursor_task_defers_while_processor_priority_masks_level_one() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner
            .bus
            .write_long(crate::memory::globals::addr::J_CRSR_TASK, 0x0004_1234);
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2100);

        runner.advance_guest_tick();

        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.cursor_task_trampoline, 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn timer_callback_snapshot_preserves_interrupted_sp() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_8BAC;
        let interrupted_sp = 0x007F_FFC0;

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::D0, 0x1111_1111);
        runner.m68k.cpu.write_reg(Register::D7, 0x7777_7777);
        runner.m68k.cpu.write_reg(Register::A0, 0xAAAA_0000);
        runner.m68k.cpu.write_reg(Register::A6, 0xCCCC_0000);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_ccr(0x1F);
        runner.bus.write_word(0x0039_38C8 + 4, 0x8001);

        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: 0x0039_38C8,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: 0x0004_1234,
            active: true,
            fire_at_tick: 10,
            fire_at_subtick: 10_000_000,
            last_fired_tick: None,
        });

        runner.fire_timer_tasks(10);

        let active = runner
            .active_interrupt_callback
            .expect("timer callback should have been armed");

        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::Timer
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);
        assert_eq!(active.a_regs[7], interrupted_sp);
        assert_eq!(active.a_regs[6], 0xCCCC_0000);
        assert_eq!(active.d_regs[0], 0x1111_1111);
        assert_eq!(active.d_regs[7], 0x7777_7777);
        assert_eq!(active.sr & 0x001F, 0x001F);
        assert_eq!(active.ccr, 0x1F);
        assert_eq!(
            runner.bus.read_word(0x0039_38C8 + 4),
            1,
            "an expired Time Manager task must be inactive before tmAddr runs"
        );

        assert_ne!(runner.timer_trampoline, 0);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            runner.timer_trampoline
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp - 4);
        assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
    }

    #[test]
    fn timer_callback_fired_at_tick_cap_runs_before_yielding() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = 0x0002_0000;

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.tick_budget = 0;
        runner.bus.write_word(callback_addr, 0x4E75); // RTS
        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: 0x0039_38C8,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: callback_addr,
            active: true,
            fire_at_tick: 101,
            fire_at_subtick: 101_000_000,
            last_fired_tick: None,
        });

        let (steps, running) = runner.run_steps(1, Some(101));

        assert!(running);
        assert_eq!(
            steps, 1,
            "a timer fired while reaching the tick cap must get a CPU slice"
        );
        assert_eq!(runner.bus.read_long(0x016A), 101);
        assert!(runner.active_interrupt_callback.is_some());
        assert_ne!(runner.timer_trampoline, 0);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            runner.timer_trampoline + 4
        );
    }

    #[test]
    fn sub_vbl_timer_callback_fires_before_next_guest_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = 0x0002_0000;

        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S to self
        runner.bus.write_word(callback_addr, 0x4E75); // RTS
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.tick_budget = runner.instructions_per_tick as i32;
        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: 0x0039_38C8,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: callback_addr,
            active: true,
            fire_at_tick: 101,
            fire_at_subtick: 100_200_000,
            last_fired_tick: None,
        });

        let steps = runner.instructions_per_tick as usize / 4;
        let (executed, running) = runner.run_steps(steps, None);
        let (_, still_running) = runner.run_steps(1, None);

        assert!(running);
        assert!(still_running);
        assert_eq!(executed, steps);
        assert_eq!(runner.guest_tick(), 100);
        assert!(!runner.dispatcher.timer_tasks[0].active);
        assert_ne!(runner.timer_trampoline, 0);
    }

    #[test]
    fn timer_callback_return_runs_foreground_before_next_due_timer() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = 0x0002_0000;

        runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.bus.write_long(0x016A, 101);
        runner.set_guest_tick_for_test(101);
        runner.set_instructions_per_tick(1);
        runner.tick_budget = 0;
        runner.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::Timer,
            resume_pc: interrupted_pc,
            resume_sp: interrupted_sp,
            d_regs: [0; 8],
            a_regs: [0, 0, 0, 0, 0, 0, 0, interrupted_sp],
            sr: 0x2000,
            ccr: 0,
            restore_port: None,
        });
        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr: 0x0039_38C8,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: callback_addr,
            active: true,
            fire_at_tick: 102,
            fire_at_subtick: 102_000_000,
            last_fired_tick: None,
        });

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            interrupted_pc + 2,
            "resumed foreground instruction should run before the next timer interrupt"
        );
        assert_eq!(
            runner.guest_tick(),
            101,
            "returning from an interrupt must not immediately spend an exhausted budget on another tick"
        );
        assert!(runner.active_interrupt_callback.is_none());
        assert!(
            runner.dispatcher.timer_tasks[0].active,
            "the next due timer should remain queued until foreground code gets a slice"
        );
    }

    #[test]
    fn simultaneous_timer_callbacks_keep_undelivered_tasks_active() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.dispatcher.timer_tasks.extend([
            TimerTask {
                task_ptr: 0x0039_38C8,
                architecture: CallbackTaskArchitecture::M68k,
                extended: false,
                callback: 0x0002_0000,
                active: true,
                fire_at_tick: 10,
                fire_at_subtick: 10_000_000,
                last_fired_tick: None,
            },
            TimerTask {
                task_ptr: 0x0039_3900,
                architecture: CallbackTaskArchitecture::M68k,
                extended: false,
                callback: 0x0002_1000,
                active: true,
                fire_at_tick: 10,
                fire_at_subtick: 10_000_000,
                last_fired_tick: None,
            },
        ]);

        runner.fire_timer_tasks(10);

        assert!(!runner.dispatcher.timer_tasks[0].active);
        assert!(
            runner.dispatcher.timer_tasks[1].active,
            "a second task due on the same tick must remain queued"
        );

        // The delivered task may re-prime itself from its callback. It must not
        // jump ahead of an older task that is still waiting for delivery.
        runner.dispatcher.timer_tasks.with_mut(|timer_tasks| {
            timer_tasks[0].active = true;
            timer_tasks[0].fire_at_tick = 11;
            timer_tasks[0].fire_at_subtick = 11_000_000;
        });
        runner.active_interrupt_callback = None;
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.fire_timer_tasks(11);

        assert!(
            runner.dispatcher.timer_tasks[0].active,
            "the newly re-primed task must wait behind the older due task"
        );
        assert!(!runner.dispatcher.timer_tasks[1].active);
        assert_eq!(
            runner.bus.read_long(runner.timer_trampoline + 6),
            0x0039_3900
        );
    }

    #[test]
    fn self_reprimed_timer_can_fire_again_within_the_same_vbl() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0039_38C8;

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.dispatcher.timer_tasks.push(TimerTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: 0x0002_0000,
            active: true,
            fire_at_tick: 10,
            fire_at_subtick: 10_100_000,
            last_fired_tick: None,
        });

        runner.fire_timer_tasks_at(10_100_000);
        assert_eq!(runner.dispatcher.timer_tasks[0].last_fired_tick, Some(10));

        // Model the callback returning and re-priming itself for another
        // revised Time Manager deadline inside the same VBL.
        runner.active_interrupt_callback = None;
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.dispatcher.timer_tasks.with_mut(|timer_tasks| {
            timer_tasks[0].active = true;
            timer_tasks[0].fire_at_tick = 11;
            timer_tasks[0].fire_at_subtick = 10_300_000;
        });

        runner.fire_timer_tasks_at(10_300_000);
        assert!(
            runner.active_interrupt_callback.is_some(),
            "a revised Time Manager task must honor a new sub-VBL deadline"
        );
        assert!(!runner.dispatcher.timer_tasks[0].active);
        assert_eq!(runner.dispatcher.timer_tasks[0].last_fired_tick, Some(10));
    }

    #[test]
    fn sound_doubleback_callback_resume_restores_ccr_before_branch() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let header_ptr = 0x0020_0000;
        let exhausted_buf_ptr = 0x0020_1000;

        // BEQ.s -> MOVEQ #2,D0 path should be taken when Z is preserved.
        runner.bus.write_word(interrupted_pc, 0x6704);
        runner.bus.write_word(interrupted_pc + 2, 0x7001);
        runner.bus.write_word(interrupted_pc + 4, 0x6002);
        runner.bus.write_word(interrupted_pc + 6, 0x7002);
        runner.bus.write_word(interrupted_pc + 8, 0x4E71);

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.write_reg(Register::D0, 0);
        runner.m68k.cpu.core.set_ccr(0x04);

        runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
        runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .queue_doubleback_callback(PendingDoubleBackCallback {
                callback_addr: 0x0004_1234,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.fire_sound_doubleback_callbacks();

        let active = runner
            .active_interrupt_callback
            .expect("sound callback should have been armed");
        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::SoundDoubleBack
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);

        // Simulate the trampoline returning to interrupted code with CCR clobbered.
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_ccr(0);

        let (steps, running) = runner.run_steps(3, None);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 2);
        assert!(runner.active_interrupt_callback.is_none());
    }

    #[test]
    fn sound_doubleback_callback_trampoline_stacks_classic_pascal_order() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);
        let header_ptr = 0x0020_0000;
        let chan_ptr = 0x0039_38C8;
        let exhausted_buf_ptr = 0x0020_1000;

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
        runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .queue_doubleback_callback(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.fire_sound_doubleback_callbacks();
        let (_steps, running) = runner.run_steps(24, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        let saved_regs_sp = interrupted_sp - 4 - 32;
        assert_eq!(
            runner.bus.read_long(saved_regs_sp - 4),
            chan_ptr,
            "the first declared Pascal argument is pushed first"
        );
        assert_eq!(
            runner.bus.read_long(saved_regs_sp - 8),
            exhausted_buf_ptr,
            "the last declared Pascal argument is nearest the return address"
        );
    }

    #[test]
    fn mix_audio_loads_ready_double_buffer_without_boundary_silence() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(18);
        let buf1_ptr = runner.bus.alloc(18);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0x90, 0x91]);
        write_double_buffer(&mut runner.bus, buf1_ptr, &[0xA0, 0xA1]);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        crate::trap::TrapDispatcher::load_double_buffer_samples(
            &mut runner.bus,
            &mut chan,
            buf0_ptr,
            OUTPUT_RATE << 16,
            1,
            8,
        );
        runner.dispatcher.sound_manager.add_channel(chan);

        runner.mix_audio(3);

        assert_eq!(
            runner.audio_buffer,
            vec![0x90, 0x91, 0xA0],
            "host mixing must continue into the ready paired buffer, not emit boundary silence"
        );
        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "dbBufferReady stays set until the doubleback callback starts"
        );
        assert_eq!(
            runner.bus.read_long(buf1_ptr + 4) & 0x01,
            0x01,
            "the paired buffer is still marked ready while it is playing"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks.len(),
            1,
            "exhausting buffer 0 still queues its doubleback refill"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
            0
        );

        let chan = &runner.dispatcher.sound_manager.channels[0];
        assert!(chan.is_playing(), "buffer 1 should still be playing");
        let db = chan.double_buffer.as_ref().expect("double-buffer active");
        assert_eq!(db.current_buffer, 1);
        assert!(db.waiting_for_callback);
    }

    #[test]
    fn mix_audio_can_queue_other_doubleback_while_callback_is_active() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(17);
        let buf1_ptr = runner.bus.alloc(17);
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner.bus.write_word(interrupted_pc, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
        runner.bus.write_long(buf1_ptr, 1);
        runner.bus.write_long(buf1_ptr + 4, 0);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        crate::trap::TrapDispatcher::load_double_buffer_samples(
            &mut runner.bus,
            &mut chan,
            buf0_ptr,
            OUTPUT_RATE << 16,
            1,
            8,
        );
        runner.dispatcher.sound_manager.add_channel(chan);

        runner.mix_audio(1);
        assert_eq!(runner.dispatcher.sound_manager.pending_callbacks.len(), 1);
        assert!(
            runner.dispatcher.sound_manager.channels[0]
                .double_buffer
                .as_ref()
                .expect("double-buffer active")
                .waiting_for_callback
        );

        runner.fire_sound_doubleback_callbacks();
        assert!(matches!(
            runner
                .active_interrupt_callback
                .expect("doubleback callback should be active")
                .source,
            ActiveInterruptCallbackSource::SoundDoubleBack
        ));
        assert!(
            runner.dispatcher.sound_manager.channels[0]
                .double_buffer
                .as_ref()
                .expect("double-buffer active")
                .waiting_for_callback,
            "callback remains outstanding until guest refills a buffer"
        );

        runner.mix_audio(16);

        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks.len(),
            1,
            "the paired unready buffer may queue its own callback while buffer 0 is active"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index, 1,
            "buffer 0 must not be duplicated; buffer 1 gets the new callback"
        );
        let db = runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active");
        assert!(db.waiting_for_callback);
        assert_eq!(db.pending_callback_buffers, [true, true]);
    }

    #[test]
    fn mix_audio_does_not_load_ready_double_buffer_while_callback_is_active() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(17);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: true,
            pending_callback_buffers: [true, false],
        });
        runner.dispatcher.sound_manager.add_channel(chan);
        runner.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::SoundDoubleBack,
            resume_pc: 0x0001_0000,
            resume_sp: 0x007F_FFC0,
            d_regs: [0; 8],
            a_regs: [0; 8],
            sr: 0x2000,
            ccr: 0,
            restore_port: None,
        });

        runner.mix_audio(1);

        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "ready buffer must not be consumed before the callback returns"
        );
        assert!(
            !runner.dispatcher.sound_manager.channels[0].is_playing(),
            "callback-active buffer load should be deferred"
        );
        assert_eq!(
            runner.audio_buffer,
            vec![0x80],
            "the host stream stays alive with silence while waiting"
        );

        runner.active_interrupt_callback = None;
        runner.try_load_pending_double_buffers();

        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "returned callback buffer stays marked ready while playback owns it"
        );
        assert!(
            runner.dispatcher.sound_manager.channels[0].is_playing(),
            "returned callback makes the refilled buffer available to the mixer"
        );
        let db = runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active");
        assert_eq!(db.pending_callback_buffers, [false, false]);

        runner.mix_audio(1);
        assert_eq!(runner.audio_buffer, vec![0x80, 0xA0]);
    }

    #[test]
    fn try_load_pending_double_buffers_recovers_ready_alternate_after_underrun() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(18);
        let buf1_ptr = runner.bus.alloc(18);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0, 0xA1]);
        runner.bus.write_long(buf1_ptr, 2);
        runner.bus.write_long(buf1_ptr + 4, 0);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 1,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        runner.dispatcher.sound_manager.add_channel(chan);

        runner.try_load_pending_double_buffers();

        let chan = &runner.dispatcher.sound_manager.channels[0];
        assert!(chan.is_playing(), "ready alternate buffer should load");
        let db = chan.double_buffer.as_ref().expect("double-buffer active");
        assert_eq!(db.current_buffer, 0);
        assert!(
            !db.waiting_for_callback,
            "loading a ready buffer completes the outstanding refill wait"
        );
        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "loading a ready alternate must not clear dbBufferReady before playback exhausts"
        );
    }

    #[test]
    fn try_load_pending_double_buffers_does_not_replay_callback_pending_slot() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let chan_ptr = 0x0039_38C8;
        let callback_addr = 0x0004_1234;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(17);
        let buf1_ptr = runner.bus.alloc(17);

        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(header_ptr + 20, callback_addr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
        runner.bus.write_long(buf1_ptr, 1);
        runner.bus.write_long(buf1_ptr + 4, 0);

        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 0,
            callback_addr,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: true,
            pending_callback_buffers: [true, false],
        });
        runner.dispatcher.sound_manager.add_channel(chan);
        runner
            .dispatcher
            .sound_manager
            .queue_doubleback_callback(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.try_load_pending_double_buffers();

        assert!(
            !runner.dispatcher.sound_manager.channels[0].is_playing(),
            "an exhausted slot must not replay just because dbBufferReady remains set"
        );
        assert_eq!(
            runner.bus.read_long(buf0_ptr + 4) & 0x01,
            0x01,
            "the flag remains ready until fire_sound_doubleback_callbacks clears it"
        );
    }

    #[test]
    fn sound_command_callback_trampoline_passes_sndcommand_pointer() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(crate::sound::PendingSoundCallback::Command {
                architecture: CallbackTaskArchitecture::M68k,
                callback_addr: 0x0004_5678,
                chan_ptr: 0x0039_38C8,
                cmd: crate::sound::SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0x1234,
                    param2: 0x0001_43FC,
                },
            });

        runner.fire_sound_callbacks();

        let active = runner
            .active_interrupt_callback
            .expect("sound callback should have been armed");
        assert!(matches!(
            active.source,
            ActiveInterruptCallbackSource::SoundCallback
        ));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);

        let tramp = runner.sound_callback_trampoline;
        let cmd_ptr = tramp + 34;
        let saved_regs_sp = interrupted_sp - 4 - 32;
        assert_eq!(runner.bus.read_long(tramp + 6), 0x0039_38C8);
        assert_eq!(runner.bus.read_long(tramp + 12), cmd_ptr);
        assert_eq!(runner.bus.read_long(tramp + 18), 0x0004_5678);
        assert_eq!(runner.bus.read_long(tramp + 24), saved_regs_sp);
        assert_eq!(runner.bus.read_word(cmd_ptr), crate::sound::cmd::CALLBACK);
        assert_eq!(runner.bus.read_word(cmd_ptr + 2), 0x1234);
        assert_eq!(runner.bus.read_long(cmd_ptr + 4), 0x0001_43FC);
        assert_eq!(
            runner.bus.get_alloc_size(tramp),
            None,
            "Systemless-owned command callback trampoline must stay outside the guest heap"
        );
    }

    #[test]
    fn sound_command_callback_trampoline_does_not_perturb_guest_allocations() {
        let mut baseline = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let _baseline_callback = baseline.bus.alloc(2);
        let expected_next_guest_ptr = baseline.bus.alloc(64);

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let callback_addr = runner.bus.alloc(2);
        runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(crate::sound::PendingSoundCallback::Command {
                architecture: CallbackTaskArchitecture::M68k,
                callback_addr,
                chan_ptr: 0x0039_38C8,
                cmd: crate::sound::SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });

        runner.fire_sound_callbacks();
        let actual_next_guest_ptr = runner.bus.alloc(64);

        assert_eq!(
            actual_next_guest_ptr, expected_next_guest_ptr,
            "lazy callback setup must not consume application-visible heap space"
        );
    }

    #[test]
    fn file_completion_callback_uses_documented_registers_and_restores_foreground() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let parameter_block = runner.bus.alloc(64);
        let callback_addr = runner.bus.alloc(2);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS
        for offset in (0..20).step_by(2) {
            runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
        }
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.write_reg(Register::A0, 0x1111_1111);
        runner.m68k.cpu.write_reg(Register::D0, 0x2222_2222);
        runner
            .dispatcher
            .pending_file_completions
            .push_back(PendingFileCompletion {
                parameter_block,
                completion_addr: callback_addr,
                result: -39,
            });

        assert!(runner.fire_file_completion_callback());
        assert_eq!(runner.bus.read_word(parameter_block + 16) as i16, -39);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), parameter_block);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0) as i32, -39);
        assert!(matches!(
            runner.active_interrupt_callback.map(|active| active.source),
            Some(ActiveInterruptCallbackSource::FileCompletion)
        ));
        assert!(
            runner
                .bus
                .get_alloc_size(runner.file_completion_trampoline)
                .is_none(),
            "Systemless-owned completion trampoline must stay outside the guest heap"
        );

        let (_, running) = runner.run_steps(6, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x1111_1111);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x2222_2222);
        assert!(!runner.is_halted());
    }

    #[test]
    fn adb_mouse_callback_uses_documented_registers_and_restores_foreground() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);
        let data_area = runner.bus.alloc(16);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS
        for offset in (0..20).step_by(2) {
            runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
        }
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.write_reg(Register::A0, 0x1111_1111);
        runner.m68k.cpu.write_reg(Register::A1, 0x2222_2222);
        runner.m68k.cpu.write_reg(Register::A2, 0x3333_3333);
        runner.m68k.cpu.write_reg(Register::D0, 0x4444_4444);
        assert!(runner
            .dispatcher
            .adb
            .set_device_handler(3, callback_addr, data_area, false,));
        runner.dispatcher.adb.note_mouse_state((5, -10), true);

        assert!(runner.fire_adb_callback());
        let packet_ptr = runner.m68k.cpu.read_reg(Register::A0);
        assert_eq!(runner.bus.read_bytes(packet_ptr, 3), &[2, 5, 0xF6]);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A1), callback_addr);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A2), data_area);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x3C);
        assert!(matches!(
            runner.active_interrupt_callback.map(|active| active.source),
            Some(ActiveInterruptCallbackSource::Adb)
        ));
        assert!(
            runner
                .bus
                .get_alloc_size(runner.adb_callback_trampoline)
                .is_none(),
            "Systemless-owned ADB trampoline must stay outside the guest heap"
        );

        let (_, running) = runner.run_steps(6, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x1111_1111);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A1), 0x2222_2222);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A2), 0x3333_3333);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x4444_4444);
        assert!(!runner.is_halted());
    }

    #[test]
    fn sound_command_callback_trampoline_tolerates_one_long_pascal_cleanup() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(4);

        // Some Pascal callback epilogues pop one long argument by copying the
        // return address over it, then RTS.
        runner.bus.write_word(callback_addr, 0x2E9F); // MOVE.L (SP)+,(SP)
        runner.bus.write_word(callback_addr + 2, 0x4E75); // RTS
        runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(crate::sound::PendingSoundCallback::Command {
                architecture: CallbackTaskArchitecture::M68k,
                callback_addr,
                chan_ptr: 0x0039_38C8,
                cmd: crate::sound::SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0x1234,
                    param2: 0x0001_43FC,
                },
            });

        runner.fire_sound_callbacks();
        let (steps, running) = runner.run_steps(10, None);

        assert!(running, "callback trampoline should resume foreground code");
        assert_eq!(steps, 10);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc + 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(!runner.is_halted());
        assert_eq!(
            runner
                .bus
                .get_alloc_size(runner.sound_file_completion_trampoline),
            None,
            "Systemless-owned file completion trampoline must stay outside the guest heap"
        );
    }

    #[test]
    fn sound_file_completion_callback_trampoline_tolerates_c_style_cleanup() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping chan.
        runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(crate::sound::PendingSoundCallback::FileCompletion {
                architecture: CallbackTaskArchitecture::M68k,
                callback_addr,
                chan_ptr: 0x0039_38C8,
            });

        runner.fire_sound_callbacks();
        let (steps, running) = runner.run_steps(10, None);

        assert!(
            running,
            "file completion trampoline should resume foreground code"
        );
        assert_eq!(steps, 10);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(!runner.is_halted());
        assert_eq!(
            runner
                .bus
                .get_alloc_size(runner.sound_doubleback_trampoline),
            None,
            "Systemless-owned double-back trampoline must stay outside the guest heap"
        );
    }

    #[test]
    fn sound_doubleback_callback_trampoline_tolerates_c_style_cleanup() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);
        let header_ptr = 0x0020_0000;
        let exhausted_buf_ptr = 0x0020_1000;

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
        runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .queue_doubleback_callback(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 0,
            });

        runner.fire_sound_doubleback_callbacks();
        let (steps, running) = runner.run_steps(12, None);

        assert!(
            running,
            "doubleback trampoline should resume foreground code"
        );
        assert_eq!(steps, 12);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(!runner.is_halted());
    }

    #[test]
    fn run_pending_sound_work_does_not_advance_ticks_or_foreground_code() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.bus.write_long(0x016A, 41);
        runner.set_guest_tick_for_test(41);
        runner.set_instructions_per_tick(1);
        runner.tick_budget = 0;

        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(PendingSoundCallback::Command {
                architecture: CallbackTaskArchitecture::M68k,
                callback_addr,
                chan_ptr: 0x0039_38C8,
                cmd: SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });

        let (steps, running) = runner.run_pending_sound_work(32);

        assert!(running);
        assert!(steps > 0, "sound callback trampoline should execute");
        assert_eq!(
            runner.guest_tick(),
            41,
            "callback-only slices must not advance application-visible ticks"
        );
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            interrupted_pc,
            "sound callback service must stop before resumed foreground code runs"
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
        assert!(runner.active_interrupt_callback.is_none());
        assert!(!runner.has_pending_sound_work());
    }

    #[test]
    fn gui_cpu_slice_does_not_finalize_host_frame() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let callback_addr = runner.bus.alloc(2);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS
        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(PendingSoundCallback::Command {
                architecture: CallbackTaskArchitecture::M68k,
                callback_addr,
                chan_ptr: 0x0039_38C8,
                cmd: SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });

        let (steps, running) = runner.run_gui_cpu_slice(0, 0);

        assert!(running);
        assert_eq!(steps, 0);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "CPU-only GUI slices must not fire host-frame sound callbacks"
        );
        assert!(runner.has_pending_sound_work());
    }

    fn sound_chrome_runner() -> FixtureRunner {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let screen_base = 0x0040_0000;
        runner.dispatcher.screen_mode = (screen_base, 256, 256, 64, 8);
        runner.bus.write_long(0x0824, screen_base);
        runner.bus.write_word(0x0BAA, 20);
        // Menu titles come from the guest MenuList, not just the host cache.
        // Install a real menu so this oracle actually paints outline glyphs.
        let title = runner.bus.alloc(5);
        runner.bus.write_bytes(title, b"\x04File");
        let sp = 0x007F_FF80;
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_long(sp, title);
        runner.bus.write_word(sp + 4, 128);
        runner
            .dispatcher
            .dispatch(0xA931, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap(); // NewMenu
        let menu = runner.bus.read_long(sp + 6);
        assert_ne!(menu, 0);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_word(sp, 0);
        runner.bus.write_long(sp + 2, menu);
        runner
            .dispatcher
            .dispatch(0xA935, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap(); // InsertMenu
        runner.dispatcher.menu_bar_hidden = false;
        runner.prepare_text_presentation();
        runner.composite_frame();
        assert!(runner.bus.has_visible_outline_detail());
        let pixel = screen_base + 5 * 256 + 100;
        assert_ne!(runner.bus.read_byte(pixel), 0xAA);
        runner.bus.write_byte(pixel, 0xAA);
        runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_word(0x0001_0000, 0x4E71); // foreground NOP
        runner.set_guest_tick_for_test(41);
        runner.set_instructions_per_tick(1);
        runner.tick_budget = 0;
        runner
    }

    #[test]
    fn gui_sound_work_defers_chrome_but_matches_complete_slices() {
        for budget in [0, 1, 32] {
            let mut complete = sound_chrome_runner();
            let mut deferred = sound_chrome_runner();
            for runner in [&mut complete, &mut deferred] {
                let callback_addr = runner.bus.alloc(2);
                runner.bus.write_word(callback_addr, 0x4E75); // RTS
                for _ in 0..2 {
                    runner
                        .dispatcher
                        .sound_manager
                        .queue_sound_callback(PendingSoundCallback::Command {
                            architecture: CallbackTaskArchitecture::M68k,
                            callback_addr,
                            chan_ptr: 0x0039_38C8,
                            cmd: SndCommand {
                                cmd: crate::sound::cmd::CALLBACK,
                                param1: 0,
                                param2: 0,
                            },
                        });
                }
            }
            for _ in 0..64 {
                assert_eq!(
                    complete.run_pending_sound_work(budget),
                    deferred.run_gui_pending_sound_work(budget),
                );
                for reg in [Register::PC, Register::A7, Register::D0] {
                    assert_eq!(
                        complete.m68k.cpu.read_reg(reg),
                        deferred.m68k.cpu.read_reg(reg)
                    );
                }
                assert_eq!(deferred.guest_tick(), 41);
                assert_eq!(complete.guest_tick(), deferred.guest_tick());
                assert_eq!(
                    complete.has_pending_sound_work(),
                    deferred.has_pending_sound_work()
                );
                assert_eq!(
                    complete
                        .dispatcher
                        .sound_manager
                        .pending_sound_callbacks
                        .len(),
                    deferred
                        .dispatcher
                        .sound_manager
                        .pending_sound_callbacks
                        .len(),
                );
                let pixel = 0x0040_0000 + 5 * 256 + 100;
                assert_ne!(complete.bus.read_byte(pixel), 0xAA);
                assert_eq!(
                    deferred.bus.read_byte(pixel),
                    0xAA,
                    "sound slices must not repaint chrome"
                );
                if budget == 0 || !deferred.has_pending_sound_work() {
                    break;
                }
            }
            if budget > 0 {
                assert!(!deferred.has_pending_sound_work());
                assert_eq!(deferred.m68k.cpu.read_reg(Register::PC), 0x0001_0000);
                assert_eq!(deferred.m68k.cpu.read_reg(Register::A7), 0x007F_FFC0);
            }
            complete.composite_frame();
            deferred.composite_frame();
            assert!(
                deferred.bus.has_visible_outline_detail(),
                "fixture must exercise retained glyphs"
            );
            assert_eq!(
                complete.bus.save_pixel_bytes(0x0040_0000, 256 * 64),
                deferred.bus.save_pixel_bytes(0x0040_0000, 256 * 64),
                "logical pixels AND retained subpixel metadata must match",
            );
            let (cw, ch, complete_rgb, complete_draws) =
                complete.bus.outline_presentation_rgb().unwrap();
            let (dw, dh, deferred_rgb, deferred_draws) =
                deferred.bus.outline_presentation_rgb().unwrap();
            assert_eq!((cw, ch), (dw, dh));
            assert!(
                complete_rgb == deferred_rgb,
                "retained visible RGB must match"
            );
            // This cumulative counter is not visible state. Menu-bar caching
            // can eliminate glyph repainting in both paths; the overwritten
            // pixel assertions above still prove that only complete slices
            // restore chrome before the outer presentation pass.
            assert!(
                complete_draws >= deferred_draws,
                "deferred sound slices must not add glyph draws"
            );
            assert!(
                complete.bus.read_bytes(0, 8 * 1024 * 1024)
                    == deferred.bus.read_bytes(0, 8 * 1024 * 1024)
            );
        }
    }

    #[test]
    fn gui_sound_work_services_ready_double_buffers_even_with_zero_budget() {
        let mut runner = sound_chrome_runner();
        let chan_ptr = 0x0039_38C8;
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(18);
        let buf1_ptr = runner.bus.alloc(18);
        runner.bus.write_word(header_ptr, 1);
        runner.bus.write_word(header_ptr + 2, 8);
        runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0, 0xA1]);
        runner.bus.write_long(buf1_ptr, 2);
        runner.bus.write_long(buf1_ptr + 4, 0);
        let mut chan = SndChannel::new(chan_ptr, false);
        chan.double_buffer = Some(DoubleBufferState {
            header_ptr,
            current_buffer: 1,
            callback_addr: 0,
            chan_ptr,
            sample_rate: OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            last_buffer_seen: false,
            waiting_for_callback: false,
            pending_callback_buffers: [false; 2],
        });
        runner.dispatcher.sound_manager.add_channel(chan);
        assert_eq!(runner.run_gui_pending_sound_work(0), (0, true));
        let chan = &runner.dispatcher.sound_manager.channels[0];
        assert!(
            chan.is_playing(),
            "audio-only finalization must load a ready refill"
        );
        assert_eq!(chan.double_buffer.as_ref().unwrap().current_buffer, 0);
        assert_eq!(
            runner.audio_buffer_len(),
            0,
            "servicing is not an extra mix"
        );
        assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
        runner.mix_audio(2);
        assert_eq!(runner.audio_buffer, vec![0xA0, 0xA1]);
    }

    #[test]
    fn gui_sound_work_leaves_parked_chrome_validation_to_composition() {
        let mut runner = sound_chrome_runner();
        runner.park_proven_idle_cycle(0x0002_0000, 205);
        assert!(runner.idle_cycle_sleep.is_some());
        // Test the finalization policy separately from guest execution: a
        // zero-budget CPU slice can independently cancel an idle observation.
        runner.finish_host_frame(FrameFinalization::AudioOnly, 0, true);
        assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
        runner.composite_frame();
        assert_ne!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
        assert!(
            runner.idle_cycle_sleep.is_none(),
            "changed repaint must still revoke the park"
        );
        assert!(runner.bus.suspend_write_probe().is_none());
    }

    #[test]
    fn gui_sound_work_services_guest_written_queue_without_painting() {
        let mut runner = sound_chrome_runner();
        let chan_ptr = runner.bus.alloc(1088);
        runner
            .dispatcher
            .sound_manager
            .add_channel(SndChannel::new(chan_ptr, false));
        // Guest SndChannel: flags, qLength, qHead, qTail, then 8-byte commands.
        runner.bus.write_word(chan_ptr + 28, 0xFFFF);
        runner.bus.write_word(chan_ptr + 30, 128);
        runner.bus.write_word(chan_ptr + 32, 0);
        runner.bus.write_word(chan_ptr + 34, 1);
        runner
            .bus
            .write_word(chan_ptr + 36, crate::sound::cmd::VOLUME);
        runner.bus.write_word(chan_ptr + 38, 0);
        runner.bus.write_long(chan_ptr + 40, 0x0080_0040);
        assert_eq!(runner.run_gui_pending_sound_work(0), (0, true));
        assert_eq!(
            runner.bus.read_word(chan_ptr + 32),
            1,
            "guest queue must drain"
        );
        assert_eq!(
            runner.bus.read_word(chan_ptr + 28),
            0,
            "idle channel state must synchronize"
        );
        assert_eq!(
            runner.bus.read_word(chan_ptr + 20),
            0,
            "completed command must clear"
        );
        assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
    }

    #[test]
    fn run_steps_paces_pending_sound_doublebacks_to_one_per_slice() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let callback_addr = runner.bus.alloc(2);
        let header_ptr = runner.bus.alloc(24);
        let buf0_ptr = runner.bus.alloc(16);
        let buf1_ptr = runner.bus.alloc(16);

        runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
        for offset in (0..512).step_by(2) {
            runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
        }
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.bus.write_long(header_ptr + 12, buf0_ptr);
        runner.bus.write_long(header_ptr + 16, buf1_ptr);
        runner.bus.write_long(buf0_ptr + 4, 0x0000_0001);
        runner.bus.write_long(buf1_ptr + 4, 0x0000_0001);
        runner
            .dispatcher
            .sound_manager
            .queue_doubleback_callback(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 0,
            });
        runner
            .dispatcher
            .sound_manager
            .queue_doubleback_callback(PendingDoubleBackCallback {
                callback_addr,
                chan_ptr: 0x0039_38C8,
                header_ptr,
                exhausted_buffer_index: 1,
            });

        let (_steps, running) = runner.run_steps(96, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks.len(),
            1,
            "one CPU slice must not drain back-to-back doubleback interrupts"
        );
        assert_eq!(
            runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
            1
        );

        let (_steps, running) = runner.run_steps(96, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert!(
            runner.dispatcher.sound_manager.pending_callbacks.is_empty(),
            "the next CPU slice may dispatch the next pending doubleback"
        );
    }

    #[test]
    fn vbl_callback_arms_interrupt_with_task_ptr_in_a0() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.write_reg(Register::A0, 0xAAAA_0000);
        runner.m68k.cpu.core.set_ccr(0x04);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2004);

        runner.bus.write_word(task_ptr + 4, 1); // qType = vType
        runner.bus.write_long(task_ptr + 6, 0x0004_1234); // vblAddr
        runner.bus.write_word(task_ptr + 10, 1); // vblCount
        runner.bus.write_word(task_ptr + 12, 0); // vblPhase
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            slot: Some(9),
            pending: false,
        });

        runner.fire_vbl_tasks();

        let active = runner
            .active_interrupt_callback
            .expect("vbl callback should have been armed");
        assert!(matches!(active.source, ActiveInterruptCallbackSource::Vbl));
        assert_eq!(active.resume_pc, interrupted_pc);
        assert_eq!(active.resume_sp, interrupted_sp);
        assert_eq!(active.sr, 0x2004);
        assert_eq!(active.ccr, 0x04);
        assert_eq!(runner.bus.read_word(task_ptr + 10), 0);

        assert_ne!(runner.vbl_trampoline, 0);
        assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2104);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            runner.vbl_trampoline
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp - 4);
        assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
        assert_eq!(runner.bus.read_word(runner.vbl_trampoline + 4), 0x207C);
        assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), task_ptr);
        assert_eq!(
            runner.bus.read_long(runner.vbl_trampoline + 12),
            0x0004_1234
        );
    }

    #[test]
    fn simultaneous_vbl_callbacks_do_not_starve_later_queue_elements() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let first_ptr = 0x0020_2000;
        let second_ptr = 0x0020_2020;

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
        for (task_ptr, callback) in [(first_ptr, 0x0004_1234), (second_ptr, 0x0004_5678)] {
            runner.bus.write_word(task_ptr + 4, 1);
            runner.bus.write_long(task_ptr + 6, callback);
            runner.bus.write_word(task_ptr + 10, 1);
            runner.dispatcher.vbl_tasks.push(VblTask {
                task_ptr,
                architecture: CallbackTaskArchitecture::M68k,
                slot: None,
                pending: false,
            });
        }

        runner.fire_vbl_tasks();
        assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), first_ptr);
        assert!(runner.dispatcher.vbl_tasks[1].pending);

        // Model the first callback rescheduling itself every retrace. The
        // already-due second element must run before the first one can run
        // again.
        runner.bus.write_word(first_ptr + 10, 1);
        runner.active_interrupt_callback = None;
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
        runner.fire_vbl_tasks();

        assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), second_ptr);
        assert!(runner.dispatcher.vbl_tasks[0].pending);
        assert!(!runner.dispatcher.vbl_tasks[1].pending);
    }

    #[test]
    fn vbl_callback_defers_while_processor_priority_masks_level_one() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2100);

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, 0x0004_1234);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });

        runner.fire_vbl_tasks();

        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.bus.read_word(task_ptr + 10), 1);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn classic_sound_callback_router_leaves_powerpc_completion_pending() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(crate::sound::PendingSoundCallback::FileCompletion {
                architecture: CallbackTaskArchitecture::PowerPc,
                callback_addr: 0x0050_1000,
                chan_ptr: 0x0050_2000,
            });

        assert!(!runner.fire_sound_callbacks());
        assert!(!runner.has_pending_sound_work());
        let (steps, running) = runner.run_pending_sound_work(8);
        assert_eq!(steps, 0);
        assert!(running);
        assert!(matches!(
            runner
                .dispatcher
                .sound_manager
                .pending_sound_callbacks
                .as_slice(),
            [crate::sound::PendingSoundCallback::FileCompletion {
                architecture: CallbackTaskArchitecture::PowerPc,
                callback_addr: 0x0050_1000,
                chan_ptr: 0x0050_2000,
            }]
        ));
    }

    #[test]
    fn vbl_callback_restores_foreground_sr_after_return() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0002_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;
        let callback_addr = 0x0004_1234;

        runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
        runner.bus.write_word(callback_addr, 0x4E75); // VBL callback RTS
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2004);

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, callback_addr);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });

        runner.fire_vbl_tasks();
        assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2104);

        let (_steps, running) = runner.run_steps(8, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2004);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn shipped_host_execution_policy_preserves_public_defaults() {
        let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        assert_eq!(runner.instructions_per_tick(), 12_000);
        assert_eq!(DEFAULT_VBL_HZ, 60.15);
        assert_eq!(DEFAULT_REALTIME_CPU_MHZ, 25.0);
        assert_eq!(DEFAULT_REALTIME_PPC_CPU_MHZ, 120.0);
        assert_eq!(DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND, 25_000_000.0);
        assert_eq!(default_realtime_instructions_per_tick(false), 415_628);
        assert_eq!(default_realtime_instructions_per_tick(true), 1_995_012);
    }

    #[test]
    fn host_pacing_override_preserves_m68k_guest_profile_and_canonical_ticks() {
        const SYS_ENV: u32 = 0x0030_0000;
        const PROGRAM: u32 = 0x0001_0000;

        fn guest_profile(runner: &mut FixtureRunner) -> ([u32; 5], [u16; 3], u8, u32) {
            let mut gestalt = [0; 5];
            for (index, selector) in [*b"sysa", *b"cput", *b"proc", *b"fpu ", *b"mmu "]
                .into_iter()
                .enumerate()
            {
                runner
                    .m68k
                    .cpu
                    .write_reg(Register::D0, u32::from_be_bytes(selector));
                runner
                    .dispatcher
                    .dispatch(0xA1AD, &mut runner.m68k.cpu, &mut runner.bus)
                    .unwrap();
                gestalt[index] = runner.m68k.cpu.read_reg(Register::A0);
            }

            runner.m68k.cpu.write_reg(Register::A0, SYS_ENV);
            runner.m68k.cpu.write_reg(Register::D0, 2);
            runner
                .dispatcher
                .dispatch(0xA090, &mut runner.m68k.cpu, &mut runner.bus)
                .unwrap();
            let sys_environs = [
                runner.bus.read_word(SYS_ENV + 2),
                runner.bus.read_word(SYS_ENV + 4),
                runner.bus.read_word(SYS_ENV + 6),
            ];
            let has_fpu = runner.bus.read_byte(SYS_ENV + 8);

            runner.m68k.cpu.write_reg(Register::D0, u32::MAX);
            runner
                .dispatcher
                .dispatch(0xA485, &mut runner.m68k.cpu, &mut runner.bus)
                .unwrap();
            let cpu_speed = runner.m68k.cpu.read_reg(Register::D0);
            (gestalt, sys_environs, has_fpu, cpu_speed)
        }

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let default_guest_profile = guest_profile(&mut runner);
        assert_eq!(
            default_guest_profile,
            ([1, 4, 5, 3, 4], [20, 0x0810, 5], 1, 25)
        );

        runner.set_instructions_per_tick(3);
        assert_eq!(guest_profile(&mut runner), default_guest_profile);

        for offset in (0..14).step_by(2) {
            runner.bus.write_word(PROGRAM + offset, 0x4E71);
        }
        runner.m68k.cpu.write_reg(Register::PC, PROGRAM);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.set_guest_tick_for_test(0);
        runner
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 500);
        let tick_result = runner.m68k.cpu.read_reg(Register::A7);
        runner.bus.write_long(tick_result, 0);
        runner
            .dispatcher
            .dispatch(0xA975, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        assert_eq!(runner.bus.read_long(tick_result), 500);

        let (steps, running) = runner.run_steps(7, None);

        assert!(running);
        assert_eq!(steps, 7);
        assert_eq!(
            runner.bus.read_long(crate::memory::globals::addr::TICKS),
            502
        );
        runner.bus.write_long(tick_result, 0);
        runner
            .dispatcher
            .dispatch(0xA975, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        assert_eq!(runner.bus.read_long(tick_result), 502);
    }

    #[test]
    fn host_pacing_override_preserves_powerpc_guest_profile_and_tick_visibility() {
        use crate::loader::ppc::tests::synthetic_pef_with_import;

        const RESPONSE: u32 = PPC_HEAP_BASE + 0x1000;
        const SYS_ENV: u32 = RESPONSE + 0x100;

        fn guest_state(runner: &mut FixtureRunner) -> ([(u32, u32); 5], [u16; 4], [u8; 2], u32) {
            let mut context = runner
                .native
                .take(NativeEngineRole::Companion)
                .expect("PPC companion installed");
            let native = context.adapter_mut();
            let mut capabilities = [(0, 0); 5];

            native.cpu.pc = native.imports[0].trap_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::TickCount;
            let probe = runner
                .process_context
                .with_memory_and_cfm(|memory_manager, cfm| {
                    native.run_with_process_services(64, false, false, memory_manager, cfm)
                });
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            let tick_count = native.cpu.gpr[3];

            for (index, selector) in [*b"cput", *b"proc", *b"fpu ", *b"mmu ", *b"sysa"]
                .into_iter()
                .enumerate()
            {
                native.cpu.pc = native.imports[0].trap_pc;
                native.cpu.lr = PPC_HALT_PC;
                native.imports[0].dispatcher_target = PpcImportDispatcherTarget::Gestalt;
                native.cpu.gpr[3] = u32::from_be_bytes(selector);
                native.cpu.gpr[4] = RESPONSE;
                let probe = runner
                    .process_context
                    .with_memory_and_cfm(|memory_manager, cfm| {
                        native.run_with_process_services(64, false, false, memory_manager, cfm)
                    });
                assert_eq!(probe.handled_import_count, 1);
                assert_eq!(probe.unsupported_import_index, None);
                capabilities[index] = (
                    native.cpu.gpr[3],
                    native.memory.read_u32_be(RESPONSE).unwrap(),
                );
            }

            native.cpu.pc = native.imports[0].trap_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::SysEnvirons;
            native.cpu.gpr[3] = 2;
            native.cpu.gpr[4] = SYS_ENV;
            let probe = runner
                .process_context
                .with_memory_and_cfm(|memory_manager, cfm| {
                    native.run_with_process_services(64, false, false, memory_manager, cfm)
                });
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(native.cpu.gpr[3], 0);
            let sys_environs = [
                native.memory.read_u16_be(SYS_ENV).unwrap(),
                native.memory.read_u16_be(SYS_ENV + 2).unwrap(),
                native.memory.read_u16_be(SYS_ENV + 4).unwrap(),
                native.memory.read_u16_be(SYS_ENV + 6).unwrap(),
            ];
            let sys_environs_flags = [
                native.memory.read_u8(SYS_ENV + 8).unwrap(),
                native.memory.read_u8(SYS_ENV + 9).unwrap(),
            ];

            assert!(runner.native.restore(context).is_ok());
            (capabilities, sys_environs, sys_environs_flags, tick_count)
        }

        let mut native = load_pef_application(&synthetic_pef_with_import(b"Gestalt")).unwrap();
        native.memory.add_region(RESPONSE, vec![0; 0x110]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_ppc_companion(native);
        runner
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 700);

        let default_guest_state = guest_state(&mut runner);
        assert_eq!(
            default_guest_state,
            (
                [(0, 0x0104), (0, 2), (0, 3), (0, 4), ((-5551i32) as u32, 0)],
                [2, 20, 0x0810, 5],
                [1, 1],
                700,
            )
        );

        runner.set_instructions_per_tick(7);
        runner
            .bus
            .write_long(crate::memory::globals::addr::TICKS, 900);
        let paced_guest_state = guest_state(&mut runner);
        assert_eq!(paced_guest_state.0, default_guest_state.0);
        assert_eq!(paced_guest_state.1, default_guest_state.1);
        assert_eq!(paced_guest_state.2, default_guest_state.2);
        assert_eq!(paced_guest_state.3, 900);
    }

    #[test]
    fn custom_instructions_per_tick_controls_tick_cadence() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 14;

        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(3);

        let (steps, running) = runner.run_steps(7, None);

        assert!(running);
        assert_eq!(steps, 7);
        assert_eq!(runner.bus.read_long(0x016A), 2);
    }

    #[test]
    fn non_idle_hle_trap_cost_advances_tick_budget() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let rect = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA8A8); // _OffsetRect
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_word(sp, 1); // dv
        runner.bus.write_word(sp + 2, 2); // dh
        runner.bus.write_long(sp + 4, rect);
        runner.bus.write_word(rect, 10);
        runner.bus.write_word(rect + 2, 20);
        runner.bus.write_word(rect + 4, 30);
        runner.bus.write_word(rect + 6, 40);
        runner.bus.write_long(0x016A, 0);
        runner.set_guest_tick_for_test(0);
        runner.set_instructions_per_tick(5);

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.guest_tick(),
            1,
            "non-idle HLE traps should consume tick budget beyond the base instruction"
        );
        assert_eq!(runner.bus.read_word(rect), 11);
        assert_eq!(runner.bus.read_word(rect + 2), 22);
    }

    #[test]
    fn idle_hle_traps_do_not_apply_extra_tick_cost() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;

        runner.bus.write_word(base, 0xA975); // _TickCount
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 42);
        runner.set_guest_tick_for_test(42);
        runner.set_instructions_per_tick(5);

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.guest_tick(),
            42,
            "polling traps should not add synthetic HLE manager cost"
        );
        assert_eq!(runner.tick_budget, 4);
    }

    #[test]
    fn hle_trap_cost_stops_gui_slice_at_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let rect = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA8A8); // _OffsetRect
        runner.bus.write_word(base + 2, 0x4E71); // NOP that must wait for the next GUI slice
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_word(sp, 1);
        runner.bus.write_word(sp + 2, 2);
        runner.bus.write_long(sp + 4, rect);
        runner.bus.write_word(rect, 10);
        runner.bus.write_word(rect + 2, 20);
        runner.bus.write_word(rect + 4, 30);
        runner.bus.write_word(rect + 6, 40);
        runner.bus.write_long(0x016A, 0);
        runner.set_guest_tick_for_test(0);
        runner.set_instructions_per_tick(5);

        let (steps, running) = runner.run_gui_slice_with_audio(8, 1, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 1);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            base + 2,
            "the next guest instruction should be deferred once HLE cost reaches the GUI tick cap"
        );
    }

    /// Regression gate for the guest-owned TickCount invariant.
    /// `advance_guest_tick` and the unfreeze path update low-memory `$016A`;
    /// all semantic readers import those bytes before using host pacing state.
    /// Any future path that bypasses that import can desynchronize double-
    /// click detection, the TickCount handler, and diagnostic tick printouts.
    #[test]
    fn dispatcher_tick_count_stays_in_sync_with_bus() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 20;

        // NOPs keep the CPU stepping without producing traps that
        // could interfere with tick accounting.
        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        // Set both sides of the invariant to the same initial value.
        runner.set_guest_tick_for_test(0);
        runner.set_instructions_per_tick(3);

        // Step a few times; ticks should advance roughly every 3
        // instructions. After each run_steps, bus and dispatcher
        // must agree.
        for _ in 0..3 {
            let (_, running) = runner.run_steps(3, None);
            assert!(running);
            assert_eq!(
                runner.bus.read_long(0x016A),
                runner.guest_tick(),
                "guest low-memory Ticks ({}) diverged from semantic reader ({})",
                runner.bus.read_long(0x016A),
                runner.guest_tick(),
            );
        }
    }

    #[test]
    fn idle_snapshot_preserves_extended_cpu_state() {
        let mut cpu = m68k::CpuCore::new();
        cpu.fpr[0] = m68k::fpu::FloatX80::from_extended(0x3FFF, 0x8000_0000_0000_0000);
        let baseline = CpuArchitecturalSnapshot::capture(&cpu);

        cpu.change_of_flow = !cpu.change_of_flow;
        assert_eq!(
            baseline,
            CpuArchitecturalSnapshot::capture(&cpu),
            "internal flow bookkeeping must not participate in an idle-cycle proof"
        );

        cpu.fpr[0].mantissa ^= 1;
        assert_ne!(
            baseline,
            CpuArchitecturalSnapshot::capture(&cpu),
            "80-bit FPU precision must participate in an idle-cycle proof"
        );

        cpu.fpr[0].mantissa ^= 1;
        cpu.mmu_crp_aptr = 0x1234_5000;
        assert_ne!(
            baseline,
            CpuArchitecturalSnapshot::capture(&cpu),
            "canonical MMU state must participate in an idle-cycle proof"
        );

        cpu.mmu_crp_aptr = 0;
        cpu.prefetch_queue[1] = 0x4E71;
        assert_ne!(
            baseline,
            CpuArchitecturalSnapshot::capture(&cpu),
            "precise prefetch state must participate in an idle-cycle proof"
        );
    }

    #[test]
    fn exact_idle_cycle_requires_cpu_and_memory_repeat() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        let sp = 0x0010_0000u32;
        let scratch = 0x0020_0000u32;
        runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
        runner.m68k.cpu.core.ppc = trap_pc;
        runner.m68k.cpu.core.ir = 0xA975;
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_long(sp, 100);
        runner.bus.write_long(scratch, 0x1122_3344);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());
        assert!(runner.bus.fast_mem_window().is_none());

        // A complete guest iteration may use its stack and locals as long as
        // it restores every touched byte before returning to the boundary.
        runner.bus.write_long(scratch, 0xAABB_CCDD);
        runner.bus.write_long(scratch, 0x1122_3344);
        runner.note_idle_cycle_trap_result(0xA971); // null EventAvail result at SP

        assert!(runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert_eq!(runner.guest_tick(), 105);
        assert_eq!(runner.bus.read_long(0x016A), 105);
        assert_eq!(runner.bus.read_long(sp), 100);
        assert!(runner.idle_cycle_probe.is_none());
        assert!(runner.idle_cycle_sleep.is_some());
        assert!(
            runner.bus.fast_mem_window().is_none(),
            "the parked sleep keeps a write guard armed across the frontend boundary"
        );

        runner.dispatcher.set_sent_open_app_event_for_test(true);
        assert!(runner.try_resume_proven_idle_cycle(Some(110)));
        assert_eq!(runner.guest_tick(), 110);
        assert_eq!(runner.bus.read_long(sp), 100);
        assert!(runner.idle_cycle_sleep.is_some());

        runner.push_mouse_down(20, 30);
        assert!(
            !runner.try_resume_proven_idle_cycle(Some(115)),
            "new host input must revoke a proof before the guest event loop is skipped"
        );
        assert_eq!(runner.guest_tick(), 110);
        assert!(runner.idle_cycle_sleep.is_none());
        assert!(runner.bus.fast_mem_window().is_some());
    }

    #[test]
    fn busy_poller_overflows_one_journal_then_backs_off_for_the_tick() {
        // EV Override's boot and speed calibration poll TickCount between
        // bursts of real work. Such a site must cost at most one capped
        // journal per tick, never a journal that grows with the work.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        let work = 0x0030_0000u32;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.set_guest_tick_for_test(100);

        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());
        assert!(runner.bus.fast_mem_window().is_none());

        // The "cycle" writes far more than a wait ever does: the journal
        // counts 32-bit words, so this touches twice the cap in words.
        for offset in 0..(8 * crate::memory::bus::WRITE_PROBE_MAX_ENTRIES as u32) {
            runner.bus.write_byte(work + offset, 0xAA);
        }
        assert!(
            runner.bus.fast_mem_window().is_some(),
            "the bus voids an overflowing journal immediately, without waiting for the runner"
        );

        // Back at the site: the observation is dropped and the site is
        // marked busy for this tick, so later same-tick arrivals do not
        // re-arm a journal that would only overflow again.
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_none());
        assert!(runner.idle_cycle_site_is_busy(trap_pc, 100));
        for _ in 0..4 {
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(runner.idle_cycle_probe.is_none());
            assert!(runner.bus.fast_mem_window().is_some());
        }
        assert_eq!(
            runner.guest_tick(),
            100,
            "a busy site is never fast-forwarded"
        );

        // A new tick lifts the back-off: the site is observed afresh.
        runner.set_guest_tick_for_test(101);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());
        assert!(runner.bus.fast_mem_window().is_none());
    }

    #[test]
    fn a_site_that_keeps_failing_gets_two_probes_per_tick_then_none() {
        // The boot storm: EV Override started 928,781 probes in 17 s of
        // boot, 926,295 failing on changed memory, because a failed probe
        // was re-armed on the very next same-tick arrival. Now a site gets
        // a first probe and one retry per tick, then nothing until the
        // tick changes -- and every arrival in between costs no journal.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        let scratch = 0x0020_0000u32;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.set_guest_tick_for_test(100);

        // Arrival 1: baseline. Arrival 2: probe #1 armed.
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());
        // Work that does not restore memory; arrival 3 fails on memory and
        // re-arms once (probe #2).
        runner.bus.write_long(scratch, 0x1111_1111);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some(), "one retry is allowed");
        assert!(runner.bus.fast_mem_window().is_none());
        // More work; arrival 4 fails again: budget spent, no journal, and
        // every later same-tick arrival is a plain return.
        runner.bus.write_long(scratch, 0x2222_2222);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_none());
        assert!(runner.bus.fast_mem_window().is_some());
        for _ in 0..8 {
            runner.bus.write_long(scratch, 0x3333_3333);
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(runner.idle_cycle_probe.is_none());
            assert!(runner.bus.fast_mem_window().is_some());
        }
        assert_eq!(runner.guest_tick(), 100);

        // Next tick: the site is observed afresh, and a cycle that now
        // restores its writes proves and parks exactly as before.
        runner.set_guest_tick_for_test(101);
        runner.bus.write_long(0x016A, 101);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());
        runner.bus.write_long(scratch, 0xAAAA_AAAA);
        runner.bus.write_long(scratch, 0x3333_3333);
        runner.note_idle_cycle_trap_result(0xA971);
        assert!(runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert_eq!(runner.guest_tick(), 105);
        assert!(runner.idle_cycle_sleep.is_some());
    }

    #[test]
    fn journal_complete_traps_do_not_cancel_an_idle_probe() {
        // EV Override's crawl idles in a GetKeys/Button cycle interleaved
        // with SANE math; SimCity 2000's dialog loops poll LocalToGlobal,
        // the Window Manager queries and TEIdle. The proof must survive every
        // one of those traps, in the plain and the auto-pop encodings, and
        // still cancel on anything with unjournaled consequences.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.set_guest_tick_for_test(100);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());

        for opcode in [
            0xA972u16, 0xA973, 0xA974, 0xA975, 0xA976, 0xA9EB, 0xA9EC, 0xA870, 0xA871, 0xA917,
            0xA924, 0xA92C, 0xA9DA, 0xA8AD, 0xAC70, 0xAC71, 0xAD17, 0xAD24, 0xAD2C, 0xADDA,
            0xACAD,
        ] {
            runner.note_idle_cycle_trap_result(opcode);
            assert!(
                runner.idle_cycle_probe.is_some(),
                "journal-complete trap {opcode:04X} must not cancel the probe"
            );
        }
        // QDExtensions multiplexes on the D0 selector: the
        // GetGWorld/SetGWorld save/restore pair a poll loop brackets
        // its hit-testing with survives in both encodings.
        for selector in [0x0008_0005u32, 0x0008_0006] {
            runner.m68k.cpu.write_reg(Register::D0, selector);
            for opcode in [0xAB1Du16, 0xAF1D] {
                runner.note_idle_cycle_trap_result(opcode);
                assert!(
                    runner.idle_cycle_probe.is_some(),
                    "admitted QDExtensions selector {selector:08X} must not cancel the probe"
                );
            }
        }
        // MoveTo mirrors pnLoc into dispatcher state the journal cannot
        // see; anything with host-cached consequences must cancel.
        runner.note_idle_cycle_trap_result(0xA893);
        assert!(runner.idle_cycle_probe.is_none());
    }

    #[test]
    fn idle_cycle_backoff_expires_across_tick_wrap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.idle_cycle_sites[0] = IdleCycleSiteRecord {
            site: 0x20000,
            tick: u32::MAX - 1,
            probes: 0,
            cancel_streak: 2,
            resume_tick: (u32::MAX - 1).wrapping_add(4),
        };
        assert!(runner.idle_cycle_site_is_busy(0x20000, u32::MAX));
        assert!(runner.idle_cycle_site_is_busy(0x20000, 0));
        assert!(runner.idle_cycle_site_is_busy(0x20000, 1));
        assert!(!runner.idle_cycle_site_is_busy(0x20000, 2));
    }

    #[test]
    fn repeated_trap_cancels_back_a_site_off_and_a_closed_proof_resets_it() {
        // A poll loop can die to a foreign trap on every pass at
        // several sites. One cancel is routine; a streak engages an
        // exponential backoff so a doomed site stops paying the
        // armed-journal tax on every poll, and a probe that later
        // closes on its origin site clears the backoff again.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
        runner.m68k.cpu.core.ppc = trap_pc;
        runner.m68k.cpu.core.ir = 0xA975;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 100);

        for _ in 0..2 {
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(runner.idle_cycle_probe.is_some());
            runner.note_idle_cycle_trap_result(0xA893); // MoveTo cancels
            assert!(runner.idle_cycle_probe.is_none());
        }

        // Two consecutive trap cancels back the site off: no probe can
        // begin here while the backoff runs.
        for _ in 0..4 {
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        }
        assert!(runner.idle_cycle_probe.is_none());

        // Backoff expired (streak 2 = 4 ticks): probing resumes, and a
        // proof that closes on its origin resets the streak entirely.
        runner.bus.write_long(0x016A, 104);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());
        assert!(runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        let rec = runner
            .idle_cycle_sites
            .iter()
            .find(|rec| rec.site == trap_pc)
            .expect("site record");
        assert_eq!(rec.cancel_streak, 0);
        assert_eq!(rec.resume_tick, 0);
    }

    #[test]
    fn chrome_repaint_stays_out_of_an_armed_idle_journal() {
        // The runner repaints host-owned chrome every frame. Painted under an
        // armed idle-proof journal it would be recorded against the proof
        // (and at 8 bpp overflow it -- see
        // memory::bus::tests::suspended_write_probe_ignores_writes_and_rearms_intact);
        // the runner's wrapper must leave the journal armed and untouched.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let screen_base = 0x0040_0000u32;
        runner.dispatcher.screen_mode = (screen_base, 1024, 1024, 768, 8);
        runner.bus.write_long(0x0824, screen_base);
        runner.bus.write_word(0x0BAA, 20);

        runner.bus.begin_write_probe();
        runner.redraw_chrome_outside_idle_journal();
        assert!(!runner.bus.take_write_probe_overflow());
        assert!(
            runner.bus.suspend_write_probe().is_some(),
            "the journal must still be armed after the repaint"
        );
        runner.bus.cancel_write_probe();

        runner.bus.begin_write_probe();
        runner.redraw_chrome_outside_idle_journal();
        assert!(runner.bus.finish_write_probe_unchanged());
    }

    #[test]
    fn first_parked_repaint_revokes_the_park_when_guest_overwrote_a_chrome_pixel() {
        // Issue #1052: the park reuses its proof without re-execution, so a
        // suspended repaint that is not byte-identical (guest code scribbled
        // over chrome before entering the idle loop) would silently change
        // guest RAM relative to the proven state. It must revoke the park.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let screen_base = 0x0040_0000u32;
        runner.dispatcher.screen_mode = (screen_base, 1024, 1024, 768, 8);
        runner.bus.write_long(0x0824, screen_base);
        runner.bus.write_word(0x0BAA, 20);

        runner.dispatcher.menus.push(crate::trap::menu::Menu {
            id: 1,
            title: String::from("Apple"),
            items: Vec::new(),
            enabled: true,
            handle: 0,
            in_menu_bar: true,
            hierarchical: false,
            visible_in_menu_bar: true,
        });
        runner.dispatcher.front_window = 0;
        runner.dispatcher.fullscreen_locked = false;
        runner.dispatcher.menu_bar_hidden = false;

        // Establish the current chrome pixels, then scribble one menu-bar
        // pixel the way pre-idle guest drawing would.
        runner.redraw_chrome_outside_idle_journal();
        let pixel = screen_base + 5 * 1024 + 100;
        assert_ne!(
            runner.bus.read_byte(pixel),
            0xAA,
            "fixture: a painted menu-bar pixel must differ from the sentinel"
        );
        runner.bus.write_byte(pixel, 0xAA);

        runner.park_proven_idle_cycle(0x0002_0000, 205);
        assert!(runner.idle_cycle_sleep.is_some());

        runner.redraw_chrome_outside_idle_journal();
        assert_ne!(
            runner.bus.read_byte(pixel),
            0xAA,
            "the repaint repainted the scribbled chrome pixel"
        );
        assert!(
            runner.idle_cycle_sleep.is_none(),
            "a repaint that changed guest RAM must revoke the park"
        );
        assert!(
            runner.bus.suspend_write_probe().is_none(),
            "the revoked park's journal must be closed"
        );
    }

    #[test]
    fn byte_identical_parked_repaints_keep_the_park() {
        // The control for the revocation gate: chrome that is already
        // current repaints byte-identically, the park survives, and its
        // journal stays armed and unchanged.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let screen_base = 0x0040_0000u32;
        runner.dispatcher.screen_mode = (screen_base, 1024, 1024, 768, 8);
        runner.bus.write_long(0x0824, screen_base);
        runner.bus.write_word(0x0BAA, 20);

        runner.dispatcher.menus.push(crate::trap::menu::Menu {
            id: 1,
            title: String::from("Apple"),
            items: Vec::new(),
            enabled: true,
            handle: 0,
            in_menu_bar: true,
            hierarchical: false,
            visible_in_menu_bar: true,
        });
        runner.dispatcher.front_window = 0;
        runner.dispatcher.fullscreen_locked = false;
        runner.dispatcher.menu_bar_hidden = false;

        runner.redraw_chrome_outside_idle_journal();
        runner.park_proven_idle_cycle(0x0002_0000, 205);
        assert!(runner.idle_cycle_sleep.is_some());

        runner.redraw_chrome_outside_idle_journal();
        runner.redraw_chrome_outside_idle_journal();
        assert!(
            runner.idle_cycle_sleep.is_some(),
            "byte-identical repaints must keep the park"
        );
        let journal = runner
            .bus
            .suspend_write_probe()
            .expect("the park's journal must still be armed");
        runner.bus.resume_write_probe(journal);
        assert!(runner.bus.finish_write_probe_unchanged());
    }

    #[test]
    fn host_snapshot_tracks_window_list_and_pending_native_menu_selection() {
        let mut runner = FixtureRunner::new(1024 * 1024, FixtureRunnerConfig::default());
        let before = IdleCycleHostSnapshot::capture(&runner.dispatcher);
        runner.dispatcher.window_list.push(0x0012_3456);
        assert_ne!(before, IdleCycleHostSnapshot::capture(&runner.dispatcher));
        runner.dispatcher.window_list.pop();
        assert_eq!(before, IdleCycleHostSnapshot::capture(&runner.dispatcher));
        runner
            .dispatcher
            .pending_native_menu_selection
            .stage((3, 1));
        assert_ne!(before, IdleCycleHostSnapshot::capture(&runner.dispatcher));
    }

    #[test]
    fn window_and_textedit_mutators_still_cancel_an_idle_probe() {
        // The admission is a list of specific traps, not a range: the
        // neighbours that mutate host-mirrored window or TextEdit state
        // must go on cancelling.
        for opcode in [0xA918u16, 0xA91F, 0xA928, 0xA929, 0xA9D8, 0xA9D9, 0xA9DC] {
            let mut runner = FixtureRunner::new(1024 * 1024, FixtureRunnerConfig::default());
            let trap_pc = 0x0002_0000u32;
            runner.m68k.cpu.write_reg(Register::A7, 0x0008_0000);
            runner.set_guest_tick_for_test(100);
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(runner.idle_cycle_probe.is_some());
            runner.note_idle_cycle_trap_result(opcode);
            assert!(
                runner.idle_cycle_probe.is_none(),
                "{opcode:04X} must cancel the probe"
            );
        }
    }

    #[test]
    fn teidle_inside_a_proof_parks_when_idle_and_fails_on_memory_when_it_blinks() {
        // TEIdle is admitted because it either writes nothing or stamps
        // caretState/caretTime into guest RAM before it paints. Both halves
        // of that claim, through the real handler under a real probe.
        for (blink_due, expect_park) in [(false, true), (true, false)] {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            let trap_pc = 0x0002_0000u32;
            let sp = 0x0010_0000u32;
            let te_handle = 0x0020_0000u32;
            let te_rec = 0x0020_0100u32;
            runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
            runner.m68k.cpu.core.ppc = trap_pc;
            runner.m68k.cpu.core.ir = 0xA975; // the loop's TickCount anchor
            runner.bus.write_long(0x016A, 100);
            runner.set_guest_tick_for_test(100);
            runner.bus.write_long(te_handle, te_rec);
            runner.bus.write_word(te_rec + 0x20, 5); // selStart
            runner.bus.write_word(te_rec + 0x22, 5); // selEnd: an insertion point
            runner.bus.write_word(te_rec + 0x24, 1); // active
            runner
                .bus
                .write_long(te_rec + 0x34, if blink_due { 60 } else { 100 }); // caretTime
            runner.bus.write_word(te_rec + 0x38, 0); // caretState
                                                     // The argument slot holds hTE before the journal opens, so the
                                                     // loop's push below rewrites the same bytes.
            runner.bus.write_long(sp - 4, te_handle);
            runner.m68k.cpu.write_reg(Register::A7, sp);
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
            assert!(runner.idle_cycle_probe.is_some());

            let before = CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core);
            // One loop iteration: push hTE, call TEIdle (which pops it).
            runner.m68k.cpu.write_reg(Register::A7, sp - 4);
            runner.bus.write_long(sp - 4, te_handle);
            let result = runner.dispatcher.dispatch_dialog(
                true,
                0x1DA,
                &mut runner.m68k.cpu,
                &mut runner.bus,
            );
            assert!(result.unwrap().is_ok());
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
            runner.note_idle_cycle_trap_result(0xA9DA);
            assert!(runner.idle_cycle_probe.is_some(), "TEIdle is admitted");
            assert_eq!(
                before,
                CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core),
                "TEIdle must leave the architectural state as it found it"
            );
            assert_eq!(runner.bus.read_word(te_rec + 0x38), u16::from(blink_due));

            let parked = runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105));
            assert_eq!(parked, expect_park, "blink_due={blink_due}");
            assert_eq!(runner.idle_cycle_sleep.is_some(), expect_park);
        }
    }

    #[test]
    fn poll_anchor_covers_the_input_family_only() {
        for opcode in [0xA972u16, 0xA973, 0xA974, 0xA975, 0xA976, 0xAD76] {
            assert!(is_poll_anchor_trap(opcode), "{opcode:04X}");
        }
        for opcode in [0xA970u16, 0xA971, 0xA991, 0xA9EB, 0xA893, 0x4E71] {
            assert!(!is_poll_anchor_trap(opcode), "{opcode:04X}");
        }
    }

    #[test]
    fn input_poll_cycle_proves_and_parks_like_a_null_event_cycle() {
        // The crawl shape: a cycle anchored at a GetKeys site with no event
        // trap anywhere in it. The exact-state proof must park it to the
        // next tick exactly as it parks a null-event loop.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        let sp = 0x0010_0000u32;
        let keymap = 0x0020_0000u32;
        runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
        runner.m68k.cpu.core.ppc = trap_pc;
        runner.m68k.cpu.core.ir = 0xA976;
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        // The prior (pre-proof) iteration already left the KeyMap and the
        // SANE-computed scroll position in place; the journal compares
        // against exactly this state.
        runner.bus.write_long(keymap, 0);
        runner.bus.write_long(keymap + 16, 0x0001_0000);

        assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(105)));
        assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());

        // One full iteration: GetKeys rewrites the same all-zero KeyMap,
        // Button and TickCount report unchanged host state, SANE recomputes
        // the same scroll position into a scratch long.
        runner.bus.write_long(keymap, 0);
        runner.note_idle_cycle_trap_result(0xA976);
        runner.note_idle_cycle_trap_result(0xA974);
        runner.note_idle_cycle_trap_result(0xA975);
        runner.bus.write_long(keymap + 16, 0x0001_0000);
        runner.bus.write_long(keymap + 16, 0x0001_0000);
        runner.note_idle_cycle_trap_result(0xA9EB);
        assert!(
            runner.idle_cycle_probe.is_some(),
            "cycle traps kept the probe"
        );

        // At the frame's tick cap, a proven cycle parks (the GUI case:
        // sleep to the next frame instead of spinning out the cap).
        assert!(runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)));
        assert_eq!(runner.guest_tick(), 100);
        assert!(runner.idle_cycle_sleep.is_some());
    }

    #[test]
    fn period_two_poll_cycle_proves_and_advances() {
        // The measured EV Override crawl shape: the wait loop alternates
        // two polled keycodes through D5, so consecutive same-site
        // arrivals never match -- only every second one does. The proof
        // must hold its journal across the period and close on the
        // origin state.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);

        let set_d5 = |runner: &mut FixtureRunner, v: u32| runner.m68k.cpu.core.set_d(5, v);

        set_d5(&mut runner, 0x39);
        assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)));
        assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)));
        assert!(
            runner.idle_cycle_probe.is_some(),
            "probe armed at 0x39 state"
        );

        set_d5(&mut runner, 0x2C);
        assert!(
            !runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)),
            "mid-period arrival must not prove"
        );
        assert!(
            runner.idle_cycle_probe.is_some(),
            "mid-period arrival must keep the probe alive"
        );

        set_d5(&mut runner, 0x39);
        assert!(
            runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)),
            "origin state closes the period-2 proof and parks at the cap"
        );
        assert!(runner.idle_cycle_sleep.is_some());
    }

    #[test]
    fn aperiodic_state_walk_never_proves() {
        // A register marching through fresh values every pass is real
        // progress: the period tolerance must give up at the cap, not
        // fabricate a proof.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);

        for step in 0..24u32 {
            runner.m68k.cpu.core.set_d(5, 0x1000 + step);
            assert!(
                !runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)),
                "step {step} must not prove"
            );
        }
        assert!(runner.idle_cycle_sleep.is_none());
    }

    #[test]
    fn cycle_longer_than_period_cap_never_proves() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);

        runner.m68k.cpu.core.set_d(5, 0);
        assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)));
        assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)));
        for state in 1..=IDLE_CYCLE_MAX_PERIOD {
            runner.m68k.cpu.core.set_d(5, u32::from(state));
            assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)));
        }
        runner.m68k.cpu.core.set_d(5, 0);
        assert!(!runner.try_exact_null_event_cycle_fastfwd(trap_pc, Some(100)));
        assert!(runner.idle_cycle_sleep.is_none());
    }

    #[test]
    fn exact_null_event_cycle_supports_alternating_sites_headlessly() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let site_a = 0x0002_0000u32;
        let site_b = 0x0002_1000u32;
        runner.m68k.cpu.write_reg(Register::PC, site_a + 2);
        runner.m68k.cpu.core.ppc = site_a;
        runner.m68k.cpu.core.ir = 0xA970;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);

        assert!(!runner.try_exact_null_event_cycle_fastfwd(site_a, None));
        assert!(!runner.try_exact_null_event_cycle_fastfwd(site_b, None));
        assert_eq!(runner.idle_cycle_last_seen, Some((site_a, 100)));

        assert!(!runner.try_exact_null_event_cycle_fastfwd(site_a, None));
        assert!(runner.idle_cycle_probe.is_some());
        assert!(!runner.try_exact_null_event_cycle_fastfwd(site_b, None));
        assert!(runner.idle_cycle_probe.is_some());

        assert!(!runner.try_exact_null_event_cycle_fastfwd(site_a, None));
        assert_eq!(runner.guest_tick(), 101);
        assert!(runner.idle_cycle_probe.is_none());
        assert!(runner.idle_cycle_sleep.is_none());
    }

    #[test]
    fn exact_idle_cycle_rejects_an_architectural_cpu_change() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
        runner.m68k.cpu.core.ppc = trap_pc;
        runner.m68k.cpu.core.ir = 0xA970;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);

        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 101, Some(100)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 101, Some(100)));
        assert!(runner.idle_cycle_probe.is_some());

        runner.m68k.cpu.write_reg(Register::D3, 1);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 101, Some(100)));
        assert_eq!(runner.guest_tick(), 100);
        assert!(runner.idle_cycle_sleep.is_none());
        assert!(
            runner.idle_cycle_probe.is_some(),
            "a changed state may start a new observation but must not reuse the old proof"
        );
    }

    #[test]
    fn exact_null_event_cycle_covers_the_complete_guest_state_machine() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let code = 0x0002_0000u32;
        let event = 0x0020_0000u32;
        let delay_base = 0x0021_0000u32;
        let tick_base = 0x0022_0000u32;
        let flag_base = 0x0023_0000u32;
        let stack = 0x0010_0000u32;

        // loop:
        //   SUBQ.W  #2,A7                 ; Boolean result slot
        //   MOVE.W  #-1,-(A7)             ; every event type
        //   PEA      event
        //   _GetNextEvent
        //   TST.W   (A7)+
        //   PEA      event.where
        //   _GlobalToLocal
        //   _SystemTask
        //   SUBQ.W  #4,A7                 ; TickCount result slot
        //   _TickCount
        //   MOVE.W  16(A0),D0             ; event/timeout predicate
        //   EXT.L   D0
        //   ADD.L   32(A1),D0
        //   CMP.L   (A7)+,D0
        //   SLT     D0
        //   TST.W   48(A2)
        //   SNE     D1
        //   OR.B    D1,D0
        //   BEQ.W   loop
        //
        // The event record is overwritten with host coordinates and then
        // converted to local coordinates on every pass. The write journal
        // must compare final values, not reject the temporary overwrite. The
        // post-TickCount words deliberately resemble a compiler-generated
        // event/timeout predicate. The proof is based on the complete cycle's
        // observed state, not on recognizing that instruction sequence.
        for (offset, word) in [
            (0, 0x554F),
            (2, 0x3F3C),
            (4, 0xFFFF),
            (6, 0x4879),
            (8, (event >> 16) as u16),
            (10, event as u16),
            (12, 0xA970),
            (14, 0x4A5F),
            (16, 0x4879),
            (18, ((event + 10) >> 16) as u16),
            (20, (event + 10) as u16),
            (22, 0xA871),
            (24, 0xA9B4),
            (26, 0x594F),
            (28, 0xA975),
            (30, 0x3028),
            (32, 16),
            (34, 0x48C0),
            (36, 0xD0A9),
            (38, 32),
            (40, 0xB09F),
            (42, 0x5DC0),
            (44, 0x4A6A),
            (46, 48),
            (48, 0x56C1),
            (50, 0x8001),
            (52, 0x6700),
            (54, 0xFFCA),
        ] {
            runner.bus.write_word(code + offset, word);
        }
        runner.m68k.cpu.write_reg(Register::PC, code);
        runner.m68k.cpu.write_reg(Register::A7, stack);
        runner.m68k.cpu.write_reg(Register::A0, delay_base);
        runner.m68k.cpu.write_reg(Register::A1, tick_base);
        runner.m68k.cpu.write_reg(Register::A2, flag_base);
        runner.m68k.cpu.write_reg(Register::D0, 0);
        runner.m68k.cpu.write_reg(Register::D1, 0);
        runner.bus.write_word(delay_base + 16, 5);
        runner.bus.write_long(tick_base + 32, 100);
        runner.bus.write_word(flag_base + 48, 0);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.dispatcher.set_sent_open_app_event_for_test(true);

        let (_, running) = runner.run_steps_internal(
            1_000,
            Some(100),
            0,
            true,
            false,
            FrameFinalization::Deferred,
        );
        assert!(running);
        let sleep = runner
            .idle_cycle_sleep
            .as_ref()
            .expect("the complete null-event state machine should prove an identity cycle");
        assert_eq!(sleep.trap_pc, code + 12);
        assert_eq!(sleep.wake_tick, 101);

        assert!(!runner.try_resume_proven_idle_cycle(Some(101)));
        assert_eq!(runner.guest_tick(), 101);
        assert!(runner.idle_cycle_sleep.is_none());
        assert_eq!(runner.m68k.cpu.core.pc, code + 14);
    }

    #[test]
    fn proven_idle_cycle_stops_sleeping_at_its_known_dependency() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        let sp = 0x0010_0000u32;
        runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
        runner.m68k.cpu.core.ppc = trap_pc;
        runner.m68k.cpu.core.ir = 0xA975;
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.dispatcher.set_sent_open_app_event_for_test(true);

        runner.park_proven_idle_cycle(trap_pc, 103);
        assert!(!runner.try_resume_proven_idle_cycle(Some(110)));
        assert_eq!(runner.guest_tick(), 103);
        assert!(runner.idle_cycle_sleep.is_none());
        assert!(runner.bus.fast_mem_window().is_some());
    }

    #[test]
    fn exact_idle_cycle_rejects_changed_memory_and_non_quiescent_traps() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        let scratch = 0x0020_0000u32;
        runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
        runner.m68k.cpu.core.ppc = trap_pc;
        runner.m68k.cpu.core.ir = 0xA975;
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        runner.bus.write_byte(scratch, 1);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert_eq!(runner.guest_tick(), 100);

        runner.m68k.cpu.write_reg(Register::D0, 0x0004_0001); // LockPixels
        runner.note_idle_cycle_trap_result(0xAB1D); // non-admitted QDExtensions selector
        assert!(runner.idle_cycle_probe.is_none());
        assert!(runner.idle_cycle_last_seen.is_none());
        assert!(runner.bus.fast_mem_window().is_some());
    }

    #[test]
    fn ordinary_tick_advance_cancels_an_exact_idle_cycle_probe() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0002_0000u32;
        runner.m68k.cpu.write_reg(Register::PC, trap_pc + 2);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());

        runner.advance_guest_tick();

        assert!(runner.idle_cycle_probe.is_none());
        assert!(runner.idle_cycle_last_seen.is_none());
        assert!(runner.bus.fast_mem_window().is_some());
    }

    #[test]
    fn spin_fastfwd_template_f_saved_register_beq_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;

        // SUBQ.L #4,A7; _TickCount; MOVE.L (A7)+,D0
        // CMP.L D0,D7; BEQ.S back-to-SUBQ
        runner.bus.write_word(base, 0x598F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xBE80);
        runner.bus.write_word(base + 8, 0x67F6);
        runner.bus.write_word(base + 10, 0x4E71);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D7, 100);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 101);
        assert_eq!(runner.bus.read_long(sp - 4), 101);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(count, 0, "post-trap body remains for exact CPU execution");

        for _ in 0..3 {
            assert!(matches!(
                runner.m68k.cpu.step(&mut runner.bus),
                crate::cpu::StepResult::Ok
            ));
        }
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 101);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 10);
    }

    fn saved_register_deadline_wait(tick: u32, deadline: u32) -> FixtureRunner {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        for (offset, word) in [0x594F, 0xA975, 0x201F, 0xBA80, 0x62F6, 0x4E71]
            .into_iter()
            .enumerate()
        {
            runner.bus.write_word(base + offset as u32 * 2, word);
        }
        runner.bus.write_long(0x016A, tick);
        runner.set_guest_tick_for_test(tick);
        runner.m68k.cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        runner.m68k.cpu.write_reg(Register::D5, deadline);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(0x0010_0000, tick);
        runner
    }

    #[test]
    fn spin_fastfwd_saved_register_deadline_preserves_cpu_exit_state() {
        // SC2K's newspaper uses CMP.L D0,D5; BHI. Exercise both forms of
        // stack reservation and a deadline crossing the signed boundary.
        for preamble in [0x594F, 0x598F] {
            for (tick, deadline) in [(100, 101), (100, 107), (0x7FFF_FFFE, 0x8000_0001)] {
                let mut runner = saved_register_deadline_wait(tick, deadline);
                runner.bus.write_word(0x0001_0000, preamble);
                let mut count = 0;
                assert!(!runner.try_tickcount_spin_fastfwd(0x0001_0004, None, &mut count));
                assert_eq!(runner.guest_tick(), deadline);
                assert_eq!(runner.bus.read_long(0x0010_0000), deadline);
                assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0xDEAD_BEEF);
                assert_eq!(runner.m68k.cpu.read_reg(Register::D5), deadline);
                assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_0000);
                assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x0001_0004);
                assert_eq!(count, 0);
                // The real CPU executes MOVE/CMP/BHI, including stack and
                // condition flags, instead of synthesizing those side effects.
                for _ in 0..3 {
                    assert!(matches!(
                        runner.m68k.cpu.step(&mut runner.bus),
                        crate::cpu::StepResult::Ok
                    ));
                }
                assert_eq!(runner.m68k.cpu.read_reg(Register::D0), deadline);
                assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_0004);
                assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x0001_000A);
                assert_eq!(runner.m68k.cpu.core.get_sr() & 0x0F, 4);
            }
        }
    }

    #[test]
    fn spin_fastfwd_saved_register_deadline_rejects_expired_or_unsafe_loops() {
        for (tick, deadline) in [(100, 100), (101, 100), (u32::MAX, 0), (100, 1_000_101)] {
            let mut runner = saved_register_deadline_wait(tick, deadline);
            let mut count = 0;
            assert!(!runner.try_tickcount_spin_fastfwd(0x0001_0004, None, &mut count));
            assert_eq!(runner.guest_tick(), tick);
            assert_eq!(runner.bus.read_long(0x0010_0000), tick);
            assert_eq!(count, 0);
        }
        for (address, replacement) in [
            (0x0001_0000, 0x4E71), // Missing stack reservation.
            (0x0001_0002, 0xA976), // Different trap.
            (0x0001_0006, 0xB080), // MOVE clobbers the comparison register.
            (0x0001_0008, 0x62F4), // Branch includes extra, unchecked work.
            (0x0001_0008, 0x6200), // Extended branch encoding.
            (0x0001_0008, 0x6EF6), // Signed comparison has different semantics.
        ] {
            let mut runner = saved_register_deadline_wait(100, 105);
            runner.bus.write_word(address, replacement);
            runner.m68k.cpu.write_reg(Register::D0, 105);
            let mut count = 0;
            assert!(!runner.try_tickcount_spin_fastfwd(0x0001_0004, None, &mut count));
            assert_eq!(runner.guest_tick(), 100);
            assert_eq!(runner.bus.read_long(0x0010_0000), 100);
            assert_eq!(count, 0);
        }
        let mut runner = saved_register_deadline_wait(100, 105);
        runner.bus.write_word(0x0001_0006, 0xB080);
        runner.bus.write_word(0x0001_0008, 0x67F6);
        runner.m68k.cpu.write_reg(Register::D0, 100);
        let mut count = 0;
        assert!(!runner.try_tickcount_spin_fastfwd(0x0001_0004, None, &mut count));
        assert_eq!(
            runner.guest_tick(),
            100,
            "CMP D0,D0; BEQ cannot become unequal"
        );
    }

    #[test]
    fn spin_fastfwd_saved_register_deadline_honors_gui_cap_and_vbl_callbacks() {
        let mut runner = saved_register_deadline_wait(100, 105);
        runner.set_instructions_per_tick(1_000);
        runner.tick_budget = 777;
        let mut count = 0;
        assert!(runner.try_tickcount_spin_fastfwd(0x0001_0004, Some(102), &mut count));
        assert_eq!(runner.guest_tick(), 102);
        assert_eq!(runner.bus.read_long(0x0010_0000), 102);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x0001_0004);
        assert_eq!(runner.tick_budget, 1_000);
        assert_eq!(count, 0);

        // Advancing the clock must yield to a due VBL callback before the
        // deadline; its stack and resume PC belong to ordinary callback code.
        let mut runner = saved_register_deadline_wait(100, 105);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
        let task = 0x0020_2000;
        runner.bus.write_word(task + 4, 1);
        runner.bus.write_long(task + 6, 0x0004_1234);
        runner.bus.write_word(task + 10, 1);
        runner.bus.write_word(task + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr: task,
            architecture: CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });
        assert!(!runner.try_tickcount_spin_fastfwd(0x0001_0004, None, &mut count));
        assert_eq!(runner.guest_tick(), 101);
        assert_eq!(runner.bus.read_long(0x0010_0000), 100);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0xDEAD_BEEF);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            runner.vbl_trampoline
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x000F_FFFC);
        assert_eq!(runner.bus.read_long(0x000F_FFFC), 0x0001_0004);
        let active = runner
            .active_interrupt_callback
            .expect("VBL callback pending");
        assert!(matches!(active.source, ActiveInterruptCallbackSource::Vbl));
        assert_eq!(active.resume_pc, 0x0001_0004);
        assert_eq!(active.resume_sp, 0x0010_0000);
    }

    #[test]
    fn spin_fastfwd_template_f_rejects_stateful_branch_target() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;

        // The branch includes an ADDQ before the canonical TickCount preamble,
        // so skipping the loop would incorrectly discard stateful work.
        runner.bus.write_word(base, 0x52B8);
        runner.bus.write_word(base + 2, 0x0002);
        runner.bus.write_word(base + 4, 0x594F);
        runner.bus.write_word(base + 6, 0xA975);
        runner.bus.write_word(base + 8, 0x201F);
        runner.bus.write_word(base + 10, 0xBE80);
        runner.bus.write_word(base + 12, 0x67F2);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D7, 100);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        runner.try_tickcount_spin_fastfwd(base + 8, None, &mut count);

        assert_eq!(runner.guest_tick(), 100);
        assert_eq!(runner.bus.read_long(sp - 4), 100);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0);
    }

    #[test]
    fn spin_fastfwd_template_g_elapsed_frame_local_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let a5 = 0x0001_8000u32;
        let a6 = 0x0010_1000u32;
        let sp = 0x0010_0000u32;

        // SUBQ.L #4,A7; _TickCount; MOVE.L (A7)+,D0
        // MOVE.L D0,-4(A6); MOVE.L -4(A6),D0
        // SUB.L -36(A5),D0; MOVEA.W 8(A6),A0
        // CMPA.L D0,A0; BGT.S back-to-SUBQ
        runner.bus.write_word(base, 0x598F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0x2D40);
        runner.bus.write_word(base + 8, 0xFFFC);
        runner.bus.write_word(base + 10, 0x202E);
        runner.bus.write_word(base + 12, 0xFFFC);
        runner.bus.write_word(base + 14, 0x90AD);
        runner.bus.write_word(base + 16, 0xFFDC);
        runner.bus.write_word(base + 18, 0x306E);
        runner.bus.write_word(base + 20, 0x0008);
        runner.bus.write_word(base + 22, 0xB1C0);
        runner.bus.write_word(base + 24, 0x6EE6);
        runner.bus.write_word(base + 26, 0x4E71);

        runner.bus.write_long(a5 - 36, 400);
        runner.bus.write_word(a6 + 8, 5);
        runner.bus.write_long(0x016A, 401);
        runner.set_guest_tick_for_test(401);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.m68k.cpu.write_reg(Register::A6, a6);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 401);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 405);
        assert_eq!(runner.bus.read_long(sp - 4), 405);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(count, 0, "post-trap body remains for exact CPU execution");

        for _ in 0..7 {
            assert!(matches!(
                runner.m68k.cpu.step(&mut runner.bus),
                crate::cpu::StepResult::Ok
            ));
        }
        assert_eq!(runner.bus.read_long(a6 - 4), 405);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 5);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 5);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 26);
    }

    /// Regression gate for the TickCount spin fast-forward template A
    /// (classic MOVE+SUBQ+CMP+BHI with register target). Builds a
    /// synthetic spin body in RAM, calls the fast-forward directly,
    /// asserts the exit state matches what the guest loop's final
    /// fall-through iteration would produce.
    #[test]
    fn spin_fastfwd_template_a_advances_ticks_and_skips_loop() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Synthesised spin body:
        //   $base+0: SUBQ.W #4, A7   (0x594F)
        //   $base+2: _TickCount      (0xA975) ← trap fires before call site
        //   $base+4: MOVE.L (A7)+, D0 (0x201F)
        //   $base+6: SUBQ.L #1, D0   (0x5380)
        //   $base+8: CMP.L D0, D3    (0xB680)
        //   $base+10: BHI.S *-12     (0x62F4)
        //   $base+12: sentinel       (0x4E71 NOP)
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0x5380);
        runner.bus.write_word(base + 8, 0xB680);
        runner.bus.write_word(base + 10, 0x62F4);
        runner.bus.write_word(base + 12, 0x4E71);

        // Initial tick 100, target D3=500 so target_tick = 501.
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D3, 500);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let pc_after_trap = base + 4;
        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert!(!hit_cap, "no tick_cap was set, cap should not trip");
        assert_eq!(runner.guest_tick(), 501, "advanced to D3+imm");
        assert_eq!(runner.bus.read_long(0x016A), 501, "bus $016A in sync");
        // After fall-through: Dn = final_tick - imm = 501 - 1 = 500 (= D3).
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 500);
        // A7 += 4 (the popped tick slot).
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_0004);
        // PC past BHI (base + 12).
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 12);
        // 4 synthesised instructions accounted for.
        assert_eq!(count, 4);
    }

    #[test]
    fn spin_fastfwd_refills_instruction_budget_at_each_elapsed_guest_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Same template A shape as the witness above: the fast-forward
        // starts immediately after the TickCount trap.
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0x5380);
        runner.bus.write_word(base + 8, 0xB680);
        runner.bus.write_word(base + 10, 0x62F4);
        runner.bus.write_word(base + 12, 0x4E71);

        runner.set_instructions_per_tick(1_000);
        runner.tick_budget = 777;
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D3, 500);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 501);
        assert_eq!(runner.tick_budget, 1_000);
    }

    /// Rejection case — `MOVE.L (A7)+, D1` followed by `SUBQ.L #imm,
    /// D0` (different registers) must NOT match. Ensures the
    /// register-consistency check in `try_spin_template_a` guards
    /// against false positives where an unrelated MOVE happens to
    /// precede a SUBQ+CMP+BHI.
    #[test]
    fn spin_fastfwd_rejects_register_mismatch() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Same as template A but MOVE.L (A7)+ targets D1, while
        // SUBQ/CMP operate on D0. Template detector must reject.
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x221F); // MOVE.L (A7)+, D1 (NOT D0)
        runner.bus.write_word(base + 6, 0x5380); // SUBQ.L #1, D0
        runner.bus.write_word(base + 8, 0xB680); // CMP.L D0, D3
        runner.bus.write_word(base + 10, 0x62F4); // BHI.S

        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D3, 500);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        let pc_after_trap = base + 4;
        let mut count = 0usize;
        runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        // No change: template rejected, guest Ticks stays at 100.
        assert_eq!(runner.guest_tick(), 100);
        // PC stays where it was (we passed pc_after_trap but the
        // fast-forward must have returned without mutating PC).
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0);
        assert_eq!(count, 0);
    }

    /// Regression gate for spin fast-forward template B (memory target,
    /// BLS variant). Sets up the post-trap state with A6 pointing at
    /// a stack frame and a target tick stored at `-4(A6)`; asserts the
    /// matcher advances to the memory target and synthesises the
    /// correct exit.
    #[test]
    fn spin_fastfwd_template_b_memory_target_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // $base+0: SUBQ.W #4, A7   (0x594F) — pre-trap SP adjust
        // $base+2: _TickCount      (0xA975) — trap
        // $base+4: MOVE.L (A7)+, D0 (0x201F)
        // $base+6: CMP.L (-4, A6), D0 — opcode 0xB0AE, d16=0xFFFC
        //          (1011 000 010 101 110 = 0xB0AE; next word 0xFFFC = -4)
        // $base+10: BLS.S $base   (0x63F4) — back to the canonical preamble
        // $base+12: sentinel NOP  (0x4E71)
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0AE);
        runner.bus.write_word(base + 8, 0xFFFC);
        runner.bus.write_word(base + 10, 0x63F4);
        runner.bus.write_word(base + 12, 0x4E71);

        // Memory target at -4(A6). A6 points at mid-stack; -4(A6)
        // holds the target tick.
        let a6 = 0x0010_1000u32;
        runner.bus.write_long(a6.wrapping_sub(4), 400);

        // Initial state
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A6, a6);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let pc_after_trap = base + 4;
        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert!(!hit_cap);
        // target_tick = mem_target + 1 = 400 + 1 = 401.
        assert_eq!(runner.guest_tick(), 401);
        assert_eq!(runner.bus.read_long(0x016A), 401);
        // Template B exit: D0 = final_tick (no SUBQ).
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 401);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_0004);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 12);
        // Template B synthesises 3 instructions (MOVE, CMP, BLS).
        assert_eq!(count, 3);
    }

    /// A memory-target loop with stateful work before `_TickCount` must not be
    /// fast-forwarded because synthesising the exit would skip that work.
    #[test]
    fn spin_fastfwd_template_b_rejects_stateful_pretrap_body() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let counter = 0x0001_8000u32;
        let a6 = 0x0001_9000u32;
        let sp = 0x0010_0000u32;

        // ADDQ.L #1,(A0,D3.L*4); SUBQ.W #4,A7; _TickCount;
        // MOVE.L (A7)+,D0; CMP.L (-4,A6),D0; BLS.S back-to-ADDQ.
        runner.bus.write_word(base, 0x52B0);
        runner.bus.write_word(base + 2, 0x3C00);
        runner.bus.write_word(base + 4, 0x594F);
        runner.bus.write_word(base + 6, 0xA975);
        runner.bus.write_word(base + 8, 0x201F);
        runner.bus.write_word(base + 10, 0xB0AE);
        runner.bus.write_word(base + 12, 0xFFFC);
        runner.bus.write_word(base + 14, 0x63F0);
        runner.bus.write_word(base + 16, 0x4E71);

        runner.bus.write_long(counter, 7);
        runner.bus.write_long(a6 - 4, 400);
        runner.bus.write_long(0x016A, 100);
        runner.bus.write_long(sp, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A0, counter);
        runner.m68k.cpu.write_reg(Register::D3, 0);
        runner.m68k.cpu.write_reg(Register::A6, a6);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 8, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 100);
        assert_eq!(runner.bus.read_long(counter), 7);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert_eq!(count, 0);
    }

    #[test]
    fn spin_fastfwd_template_b_signed_ble_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let a5 = 0x0001_8000u32;
        let sp = 0x0010_0000u32;

        // Signed memory-target loop shape emitted by some classic compilers:
        //   SUBQ.W #4,A7; _TickCount; MOVE.L (A7)+,D0
        //   CMP.L (-16,A5),D0; BLE.S back-to-SUBQ
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0AD);
        runner.bus.write_word(base + 8, 0xFFF0);
        runner.bus.write_word(base + 10, 0x6FF4);
        runner.bus.write_word(base + 12, 0x4E71);

        runner.bus.write_long(a5 - 16, 400);
        runner.bus.write_long(0x016A, 100);
        runner.bus.write_long(sp, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 401);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 401);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 12);
        assert_eq!(count, 3);
    }

    #[test]
    fn spin_fastfwd_template_b_signed_blt_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let a5 = 0x0001_8000u32;
        let sp = 0x0010_0000u32;

        // Exclusive signed memory-target loop:
        //   SUBQ.W #4,A7; _TickCount; MOVE.L (A7)+,D0
        //   CMP.L (-16,A5),D0; BLT.S back-to-SUBQ
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0AD);
        runner.bus.write_word(base + 8, 0xFFF0);
        runner.bus.write_word(base + 10, 0x6DF4);
        runner.bus.write_word(base + 12, 0x4E71);

        runner.bus.write_long(a5 - 16, 400);
        runner.bus.write_long(0x016A, 100);
        runner.bus.write_long(sp, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 400);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 400);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 12);
        assert_eq!(count, 3);
    }

    #[test]
    fn spin_fastfwd_template_b_signed_ble_rejects_overflow_target() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let a5 = 0x0001_8000u32;
        let sp = 0x0010_0000u32;
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0AD);
        runner.bus.write_word(base + 8, 0xFFF0);
        runner.bus.write_word(base + 10, 0x6FF4);
        runner.bus.write_long(a5 - 16, i32::MAX as u32);
        runner.bus.write_long(0x016A, 100);
        runner.bus.write_long(sp, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let mut count = 0usize;
        runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert_eq!(runner.guest_tick(), 100);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert_eq!(count, 0);
    }

    /// Regression gate for the TickCount spin fast-forward absolute
    /// LongInt target variant:
    ///
    ///   SUBQ.W #4,A7
    ///   _TickCount
    ///   MOVE.L (A7)+,Dn
    ///   CMP.L  (xxx).L,Dn
    ///   BCS.S  back-to-SUBQ
    ///
    /// This is the same wait-until-Ticks-reaches-memory-target shape as
    /// template B, but older MPW/Think-era code may address the target
    /// through an absolute long global instead of an A-register frame.
    #[test]
    fn spin_fastfwd_template_c_absolute_long_target_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let target_addr = 0x0002_FE44u32;

        // $base+0:  SUBQ.W #4, A7      (0x594F)
        // $base+2:  _TickCount         (0xA975)
        // $base+4:  MOVE.L (A7)+, D0   (0x201F)
        // $base+6:  CMP.L (xxx).L, D0  (0xB0B9 + absolute long)
        // $base+12: BCS.S $base        (0x65F2; base+14-14)
        // $base+14: sentinel NOP
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0B9);
        runner.bus.write_long(base + 8, target_addr);
        runner.bus.write_word(base + 12, 0x65F2);
        runner.bus.write_word(base + 14, 0x4E71);

        runner.bus.write_long(target_addr, 400);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let pc_after_trap = base + 4;
        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 400);
        assert_eq!(runner.bus.read_long(0x016A), 400);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 400);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_0004);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 14);
        assert_eq!(count, 3);
    }

    #[test]
    fn spin_fastfwd_template_e_computed_signed_deadline_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let a5 = 0x0001_8000u32;
        let a6 = 0x0010_1000u32;
        let sp = 0x0010_0000u32;

        // Signed computed-deadline loop:
        //   SUBQ.W #4,A7; _TickCount
        //   MOVE.W (-2,A6),D0; EXT.L D0; ADD.L (-16,A5),D0
        //   CMP.L (A7)+,D0; BGT.S back-to-SUBQ
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x302E);
        runner.bus.write_word(base + 6, 0xFFFE);
        runner.bus.write_word(base + 8, 0x48C0);
        runner.bus.write_word(base + 10, 0xD0AD);
        runner.bus.write_word(base + 12, 0xFFF0);
        runner.bus.write_word(base + 14, 0xB09F);
        runner.bus.write_word(base + 16, 0x6EEE);
        runner.bus.write_word(base + 18, 0x4E71);

        runner.bus.write_word(a6 - 2, 5);
        runner.bus.write_long(a5 - 16, 400);
        runner.bus.write_long(0x016A, 100);
        runner.bus.write_long(sp - 4, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.m68k.cpu.write_reg(Register::A6, a6);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 405);
        assert_eq!(runner.bus.read_long(sp - 4), 405);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0, "post-trap body remains for exact CPU execution");

        for _ in 0..5 {
            assert!(matches!(
                runner.m68k.cpu.step(&mut runner.bus),
                crate::cpu::StepResult::Ok
            ));
        }
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 405);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 18);
    }

    #[test]
    fn spin_fastfwd_template_e_computed_signed_deadline_inclusive_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let a5 = 0x0001_8000u32;
        let a6 = 0x0010_1000u32;
        let sp = 0x0010_0000u32;

        // Inclusive signed computed-deadline loop:
        //   SUBQ.W #4,A7; _TickCount
        //   MOVE.W (-2,A6),D0; EXT.L D0; ADD.L (-16,A5),D0
        //   CMP.L (A7)+,D0; BGE.S back-to-SUBQ
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x302E);
        runner.bus.write_word(base + 6, 0xFFFE);
        runner.bus.write_word(base + 8, 0x48C0);
        runner.bus.write_word(base + 10, 0xD0AD);
        runner.bus.write_word(base + 12, 0xFFF0);
        runner.bus.write_word(base + 14, 0xB09F);
        runner.bus.write_word(base + 16, 0x6CEE);
        runner.bus.write_word(base + 18, 0x4E71);

        runner.bus.write_word(a6 - 2, 5);
        runner.bus.write_long(a5 - 16, 400);
        runner.bus.write_long(0x016A, 100);
        runner.bus.write_long(sp - 4, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.m68k.cpu.write_reg(Register::A6, a6);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 406);
        assert_eq!(runner.bus.read_long(sp - 4), 406);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0, "post-trap body remains for exact CPU execution");

        for _ in 0..5 {
            assert!(matches!(
                runner.m68k.cpu.step(&mut runner.bus),
                crate::cpu::StepResult::Ok
            ));
        }
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 405);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 18);
    }

    #[test]
    fn spin_fastfwd_template_e_rejects_inclusive_signed_overflow() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let a5 = 0x0001_8000u32;
        let a6 = 0x0010_1000u32;
        let sp = 0x0010_0000u32;

        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x302E);
        runner.bus.write_word(base + 6, 0xFFFE);
        runner.bus.write_word(base + 8, 0x48C0);
        runner.bus.write_word(base + 10, 0xD0AD);
        runner.bus.write_word(base + 12, 0xFFF0);
        runner.bus.write_word(base + 14, 0xB09F);
        runner.bus.write_word(base + 16, 0x6CEE);

        runner.bus.write_word(a6 - 2, 0);
        runner.bus.write_long(a5 - 16, i32::MAX as u32);
        runner.bus.write_long(0x016A, 100);
        runner.bus.write_long(sp - 4, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.m68k.cpu.write_reg(Register::A6, a6);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 100);
        assert_eq!(runner.bus.read_long(sp - 4), 100);
        assert_eq!(count, 0);
    }

    #[test]
    fn spin_fastfwd_template_e_rejects_mismatched_extension_register() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x302E); // MOVE.W (-2,A6),D0
        runner.bus.write_word(base + 6, 0xFFFE);
        runner.bus.write_word(base + 8, 0x48C1); // EXT.L D1, not D0
        runner.bus.write_word(base + 10, 0xD0AD);
        runner.bus.write_word(base + 12, 0xFFF0);
        runner.bus.write_word(base + 14, 0xB09F);
        runner.bus.write_word(base + 16, 0x6EEE);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);

        let mut count = 0usize;
        runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert_eq!(runner.guest_tick(), 100);
        assert_eq!(count, 0);
    }

    #[test]
    fn spin_fastfwd_template_d_bcc_stack_compare_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;

        // Stack-result BCC variant:
        //   CLR.L -(A7); _TickCount; CMP.L (A7)+,D7; BCC.S *-8
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x64F8);
        runner.bus.write_word(base + 8, 0x4E71);

        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D7, 500);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 501);
        assert_eq!(runner.bus.read_long(sp - 4), 501);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0, "CMP/BCC remain for exact CPU execution");

        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 8);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn spin_fastfwd_template_d_lemmings_beq_variant() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;

        // Exact loop emitted by Lemmings 1.5.2:
        //   CLR.L -(A7); _TickCount; CMP.L (A7)+,D7; BEQ.S *-8
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x67F8);
        runner.bus.write_word(base + 8, 0x4E71);

        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D7, 100);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 101);
        assert_eq!(runner.bus.read_long(sp - 4), 101);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0, "CMP/BEQ remain for exact CPU execution");

        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert!(matches!(
            runner.m68k.cpu.step(&mut runner.bus),
            crate::cpu::StepResult::Ok
        ));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 8);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn spin_fastfwd_template_d_beq_does_not_advance_after_tick_changed() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x67F8);

        runner.bus.write_long(0x016A, 101);
        runner.set_guest_tick_for_test(101);
        runner.m68k.cpu.write_reg(Register::D7, 100);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 101);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 101);
        assert_eq!(runner.bus.read_long(sp - 4), 101);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(count, 0);
    }

    #[test]
    fn spin_fastfwd_template_d_honors_gui_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        runner.bus.write_word(base, 0x42A7);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0xBE9F);
        runner.bus.write_word(base + 6, 0x64F8);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.set_instructions_per_tick(1_000);
        runner.tick_budget = 777;
        runner.m68k.cpu.write_reg(Register::D7, 500);
        runner.m68k.cpu.write_reg(Register::A7, sp - 4);
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.bus.write_long(sp - 4, 100);

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, Some(102), &mut count);

        assert!(hit_cap);
        assert_eq!(runner.guest_tick(), 102);
        assert_eq!(runner.bus.read_long(sp - 4), 102);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(
            runner.tick_budget,
            runner.instructions_per_tick() as i32,
            "a capped synthetic boundary must leave a fresh tick budget"
        );
    }

    #[test]
    fn spin_fastfwd_leaves_interrupt_callback_state_unsynthesized() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let target_addr = 0x0002_FE44u32;
        let task_ptr = 0x0020_2000u32;
        let sp = 0x0010_0000u32;

        // Same absolute-long TickCount spin as template C. The VBL task
        // becomes due during the accelerated tick advance, before the
        // loop reaches its target tick.
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0xB0B9);
        runner.bus.write_long(base + 8, target_addr);
        runner.bus.write_word(base + 12, 0x65F2);
        runner.bus.write_word(base + 14, 0x4E71);

        runner.bus.write_long(target_addr, 400);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.set_instructions_per_tick(1_000);
        runner.tick_budget = 777;
        runner.m68k.cpu.write_reg(Register::PC, base + 4);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.m68k.cpu.write_reg(Register::D0, 0xDEAD_BEEF);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
        runner.bus.write_long(sp, 100);

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, 0x0004_1234);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });

        let mut count = 0usize;
        let hit_cap = runner.try_tickcount_spin_fastfwd(base + 4, None, &mut count);

        assert!(!hit_cap);
        assert_eq!(runner.guest_tick(), 101);
        assert_eq!(runner.bus.read_long(0x016A), 101);
        assert_eq!(count, 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0xDEAD_BEEF);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            runner.vbl_trampoline
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp - 4);
        assert_eq!(runner.bus.read_long(sp - 4), base + 4);

        assert_eq!(
            runner.tick_budget,
            runner.instructions_per_tick() as i32,
            "an interrupted synthetic boundary must leave a fresh tick budget"
        );

        let active = runner
            .active_interrupt_callback
            .expect("VBL callback should remain active for normal resume handling");
        assert!(matches!(active.source, ActiveInterruptCallbackSource::Vbl));
        assert_eq!(active.resume_pc, base + 4);
        assert_eq!(active.resume_sp, sp);
    }

    /// Regression gates for the spin-fastfwd override. Tests the
    /// pure decision function so `OnceLock`-cached env vars don't
    /// interfere across tests.
    #[test]
    fn spin_fastfwd_gate_defaults_on_with_gui_cadence_guarded_by_tick_cap() {
        // Neither force_on nor force_off → default behaviour:
        //   headless (yield_for_ui = false) → enabled
        //   capped GUI → enabled; tick_cap preserves visible cadence
        //   uncapped GUI → disabled; it could otherwise batch visible ticks
        assert!(spin_wait_fastfwd_gate(false, false, false, false));
        assert!(spin_wait_fastfwd_gate(false, false, false, true));
        assert!(spin_wait_fastfwd_gate(false, false, true, true));
        assert!(!spin_wait_fastfwd_gate(false, false, true, false));
    }

    #[test]
    fn spin_fastfwd_gate_force_off_wins() {
        // force_off must dominate force_on and override the default in
        // either mode.
        assert!(!spin_wait_fastfwd_gate(false, true, false, false));
        assert!(!spin_wait_fastfwd_gate(false, true, true, true));
        assert!(!spin_wait_fastfwd_gate(true, true, false, true));
        assert!(!spin_wait_fastfwd_gate(true, true, true, false));
    }

    #[test]
    fn spin_fastfwd_gate_force_on_remains_enabled() {
        // The legacy force-on override remains accepted, including for an
        // uncapped GUI caller that defaults to disabled.
        assert!(spin_wait_fastfwd_gate(true, false, false, false));
        assert!(spin_wait_fastfwd_gate(true, false, true, false));
    }

    #[test]
    fn menu_flash_uses_frontend_time_while_application_ticks_are_frozen() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let mut tracking = crate::menu_manager::test_process_menu_tracking(128);
        tracking.set_flash_tick(100);
        tracking.begin_flash(3, 0x0080_0002);
        runner.process_context.set_menu_tracking(Some(tracking));
        runner.frozen_ticks = Some(100);
        runner.advance_menu_presentation_clock(std::time::Duration::from_millis(300));
        runner.run_steps_internal(0, Some(102), 0, true, false, FrameFinalization::Deferred);
        assert_eq!(runner.frozen_ticks, Some(100));
        assert_eq!(
            runner
                .process_context
                .with_menu_tracking_mut(|tracking| tracking.advance_flash())
                .unwrap(),
            crate::menu_manager::MenuFlashStep::Complete(0x0080_0002)
        );
    }

    #[test]
    fn tracking_refire_freeze_policy_keeps_modaldialog_ticks_live() {
        // Menu/control tracking may freeze app-visible ticks while the GUI
        // renders intermediate tracking frames.
        assert!(tracking_refire_should_freeze_ticks(0xA93D));
        assert!(tracking_refire_should_freeze_ticks(0xAD3D));
        assert!(tracking_refire_should_freeze_ticks(0xA80B));
        assert!(tracking_refire_should_freeze_ticks(0xAC0B));
        assert!(tracking_refire_should_freeze_ticks(0xA968));
        assert!(tracking_refire_should_freeze_ticks(0xAD68));
        assert!(tracking_refire_should_freeze_ticks(0xA91E));
        assert!(tracking_refire_should_freeze_ticks(0xAD1E));
        assert!(tracking_refire_should_freeze_ticks(0xA925));
        assert!(tracking_refire_should_freeze_ticks(0xAD25));
        assert!(tracking_refire_should_freeze_ticks(0xA905));
        assert!(tracking_refire_should_freeze_ticks(0xAD05));
        assert!(tracking_refire_should_freeze_ticks(0xA926));
        assert!(tracking_refire_should_freeze_ticks(0xAD26));

        // ModalDialog must keep ticks/VBL/sound callbacks live. EV's pilot
        // dialog flow plays music through this path.
        assert!(!tracking_refire_should_freeze_ticks(0xA991));
        assert!(!tracking_refire_should_freeze_ticks(0xAD91));

        assert!(tracking_refire_uses_dialog_callbacks(0xA991));
        assert!(!tracking_refire_uses_dialog_callbacks(0xA9EA));
        assert!(tracking_refire_advances_gui_idle_tick(0xA991));
        assert!(tracking_refire_advances_gui_idle_tick(0xA9EA));
    }

    #[test]
    fn trackcontrol_refire_allows_guest_scrollbar_callback_to_execute() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let trap_pc = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let marker = runner.bus.alloc(2);
        let action_proc = runner.bus.alloc(12);
        let ctrl_ptr = runner.bus.alloc(40);
        let ctrl_handle = runner.bus.alloc(4);

        runner.bus.write_word(trap_pc, 0xA968); // _TrackControl
        runner.bus.write_word(action_proc, 0x33FC); // MOVE.W #$7A5A,marker
        runner.bus.write_word(action_proc + 2, 0x7A5A);
        runner.bus.write_long(action_proc + 4, marker);
        runner.bus.write_word(action_proc + 8, 0x4E74); // RTD #6
        runner.bus.write_word(action_proc + 10, 6);
        runner.bus.write_long(ctrl_handle, ctrl_ptr);
        runner.bus.write_word(ctrl_ptr + 8, 60);
        runner.bus.write_word(ctrl_ptr + 10, 240);
        runner.bus.write_word(ctrl_ptr + 12, 220);
        runner.bus.write_word(ctrl_ptr + 14, 256);
        runner.bus.write_byte(ctrl_ptr + 16, 0xFF);
        runner.bus.write_word(ctrl_ptr + 18, 40);
        runner.bus.write_word(ctrl_ptr + 20, 0);
        runner.bus.write_word(ctrl_ptr + 22, 100);
        runner.dispatcher.control_manager.set_proc_id(ctrl_ptr, 16);
        runner.dispatcher.input_state.set_mouse_button_for_test(true);
        runner.dispatcher.input_state.set_mouse_position_for_test((210, 248));

        runner.bus.write_long(sp, action_proc);
        runner.bus.write_word(sp + 4, 210);
        runner.bus.write_word(sp + 6, 248);
        runner.bus.write_long(sp + 8, ctrl_handle);
        runner.bus.write_word(sp + 12, 0xBEEF);
        runner.m68k.cpu.write_reg(Register::PC, trap_pc);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let (_steps, running) = runner.run_steps(20, None);

        assert!(running);
        assert_eq!(runner.bus.read_word(marker), 0x7A5A);
        assert!(runner.dispatcher.is_control_tracking());
        assert!(!runner.dispatcher.is_control_action_callback_pending());
    }

    #[test]
    fn standard_file_refires_yield_before_unrelated_modeless_callbacks() {
        for (selector, pop_total) in [(0x0002u16, 28u32), (0x0006, 16)] {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            let base = 0x0001_0000u32;
            let sp = 0x0010_0000u32;
            let reply_ptr = runner.bus.alloc(80);
            let proc_addr = 0x0001_1000u32;

            runner.bus.write_word(base, 0xA9EA); // _Pack3
            runner.m68k.cpu.write_reg(Register::PC, base);
            runner.m68k.cpu.write_reg(Register::A7, sp);
            runner.bus.write_word(sp, selector);
            runner.bus.write_long(sp + 2, reply_ptr);
            if selector == 0x0002 {
                runner.bus.write_long(sp + 10, 0); // typeList
                runner.bus.write_word(sp + 14, 0); // numTypes
            } else {
                runner.bus.write_long(sp + 6, 0); // typeList
                runner.bus.write_word(sp + 10, 0); // numTypes
            }
            runner.bus.write_word(proc_addr, 0x4E56); // plausible modeless draw proc
            runner
                .dispatcher
                .modeless_dialog_draw_proc_queue
                .push_back((0, proc_addr, 1));

            let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

            assert!(running);
            assert_eq!(steps, 1);
            assert!(runner.dispatcher.is_standard_file_get_tracking());
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
            assert!(runner.active_interrupt_callback.is_none());
            assert_eq!(
                runner.dispatcher.modeless_dialog_draw_proc_queue.len(),
                1,
                "selector ${selector:04X} must not consume another manager's callback"
            );

            let (idle_steps, idle_running) = runner.run_gui_slice_with_audio(16, 3, 0);
            assert!(idle_running);
            assert_eq!(idle_steps, 3);
            assert_eq!(runner.guest_tick(), 3);
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base);

            runner.dispatcher.modeless_dialog_draw_proc_queue.clear();
            runner
                .process_context
                .shared_event_queue()
                .push_back(QueuedEvent {
                    what: 3,
                    message: 0x0000_351B, // Escape
                    when: 0,
                    where_v: 0,
                    where_h: 0,
                    modifiers: 0,
                });
            let (cancel_steps, cancel_running) = runner.run_gui_slice_with_audio(1, 3, 0);

            assert!(cancel_running);
            assert_eq!(cancel_steps, 1);
            assert!(!runner.dispatcher.is_standard_file_get_tracking());
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 2);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + pop_total);
            assert_eq!(runner.bus.read_byte(reply_ptr), 0);
        }
    }

    #[test]
    fn modal_dialog_refire_still_schedules_its_draw_callback() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let proc_addr = 0x0001_1000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(proc_addr, 0x4E56); // plausible userItem draw proc
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        let mut tracking = dialog_tracking_for_test(0, 0);
        tracking.draw_proc_queue.push_back((proc_addr, 1));
        tracking.draw_procs_done = false;
        tracking.rendered_pixels_final = false;
        runner.dispatcher.dialog_tracking = Some(tracking);

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert!(matches!(
            runner.active_interrupt_callback,
            Some(ActiveInterruptCallback {
                source: ActiveInterruptCallbackSource::DialogDrawProc,
                ..
            })
        ));
        assert_ne!(runner.m68k.cpu.read_reg(Register::PC), base);
    }

    #[test]
    fn modal_dialog_refire_preserves_application_cdef_callback_redirection() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let side_effect = 0x0001_2000u32;
        let dialog_ptr = runner.bus.alloc(200);
        let control_handle = runner.bus.alloc(4);
        let control_ptr = runner.bus.alloc(36);
        let cdef_handle = runner.bus.alloc(4);
        let cdef_proc = runner.bus.alloc(20);

        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(cdef_proc, 0x4E56); // LINK A6,#0
        runner.bus.write_word(cdef_proc + 2, 0);
        runner.bus.write_word(cdef_proc + 4, 0x33FC); // MOVE.W #$CAFE,(abs).L
        runner.bus.write_word(cdef_proc + 6, 0xCAFE);
        runner.bus.write_long(cdef_proc + 8, side_effect);
        runner.bus.write_word(cdef_proc + 12, 0x4E5E); // UNLK A6
        runner.bus.write_word(cdef_proc + 14, 0x4E74); // RTD #12
        runner.bus.write_word(cdef_proc + 16, 12);
        runner.bus.write_long(cdef_handle, cdef_proc);

        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            0,
            100,
            120,
            220,
            360,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.dispatcher.front_window = dialog_ptr;
        runner.dispatcher.window_bounds = (100, 120, 220, 360);
        runner.dispatcher.dialog_items.insert(dialog_ptr, Vec::new());

        runner.bus.write_long(control_handle, control_ptr);
        runner.bus.write_long(control_ptr, 0);
        runner.bus.write_long(control_ptr + 4, dialog_ptr);
        runner.bus.write_word(control_ptr + 8, 10);
        runner.bus.write_word(control_ptr + 10, 10);
        runner.bus.write_word(control_ptr + 12, 50);
        runner.bus.write_word(control_ptr + 14, 80);
        runner.bus.write_byte(control_ptr + 16, 255);
        runner.bus.write_byte(control_ptr + 17, 0);
        runner.bus.write_long(control_ptr + 24, cdef_handle);
        runner.bus.write_long(dialog_ptr + 140, control_handle);
        runner
            .dispatcher
            .control_manager
            .register(control_handle, control_ptr, 160 << 4, 0);

        let item_hit = runner.bus.alloc(2);
        runner.bus.write_long(sp, item_hit);
        runner.bus.write_long(sp + 4, 0);
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            runner.dispatcher.control_def_trampoline,
            "ModalDialog must not replace the CDEF callback entry with its refire PC"
        );

        let (_steps, running) = runner.run_steps(64, None);
        assert!(running);
        assert_eq!(
            runner.bus.read_word(side_effect),
            0xCAFE,
            "the application CDEF must execute before ModalDialog refires"
        );
    }

    #[test]
    fn tracking_refire_survives_async_callback_injection() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;

        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        let mut tracking = dialog_tracking_for_test(0, 0);
        tracking.flash_remaining = 6;
        tracking.flash_delay = 3;
        runner.dispatcher.dialog_tracking = Some(tracking);

        // Model a timer/VBL callback that was injected after the tracking
        // trap's A-line instruction advanced PC to base + 2. The callback
        // must return to that post-trap PC before the runner re-fires A991.
        runner.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::Timer,
            resume_pc: base + 2,
            resume_sp: sp,
            d_regs: [0; 8],
            a_regs: [0, 0, 0, 0, 0, 0, 0, sp],
            sr: 0x2000,
            ccr: 0,
            restore_port: None,
        });

        let (steps, running) = runner.run_steps(2, None);

        assert!(running);
        assert_eq!(steps, 2);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "active={:?} deferred={:?} pc=${:08X} sp=${:08X}",
            runner.active_interrupt_callback.map(|active| (
                active.source,
                active.resume_pc,
                active.resume_sp
            )),
            runner.deferred_tracking_refire_pc,
            runner.m68k.cpu.read_reg(Register::PC),
            runner.m68k.cpu.read_reg(Register::A7),
        );
        assert!(runner.deferred_tracking_refire_pc.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base);
    }

    #[test]
    fn gui_modaldialog_idle_refire_advances_one_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.set_guest_tick_for_test(0);
        runner.set_instructions_per_tick(1_000_000);
        runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(0, 0));

        let (steps, running) = runner.run_gui_slice_with_audio(1, 1, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 1);
        assert_eq!(runner.tick_budget, runner.instructions_per_tick() as i32);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base);
    }

    #[test]
    fn time_driven_modal_wait_avoids_instruction_budget_refire_storm() {
        fn modal_runner() -> FixtureRunner {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            runner.bus.write_word(0x10000, 0xA991);
            runner.m68k.cpu.write_reg(Register::PC, 0x10000);
            runner.m68k.cpu.write_reg(Register::A7, 0x100000);
            runner.set_guest_tick_for_test(0);
            runner.bus.write_long(0x016A, 0);
            runner.set_instructions_per_tick(10_000);
            runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(0, 0));
            runner
        }
        let mut diagnostic = modal_runner();
        let mut timed = modal_runner();
        let (diagnostic_steps, diagnostic_running) = diagnostic.run_steps(20_000, Some(2));
        let (timed_steps, timed_running) = timed.run_gui_cpu_slice(20_000, 2);
        assert!(diagnostic_running && timed_running);
        assert_eq!(diagnostic.guest_tick(), 2);
        assert_eq!(timed.guest_tick(), 2);
        assert!(
            diagnostic_steps > 1_000,
            "legacy mode consumes synthetic refires"
        );
        assert_eq!(timed_steps, 2, "one idle refire per simulated tick");
        for register in [Register::PC, Register::A7] {
            assert_eq!(
                diagnostic.m68k.cpu.read_reg(register),
                timed.m68k.cpu.read_reg(register)
            );
        }
    }

    #[test]
    fn gui_modaldialog_idle_refire_runs_until_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.set_guest_tick_for_test(0);
        runner.set_instructions_per_tick(1_000_000);
        runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(0, 0));

        let (steps, running) = runner.run_gui_slice_with_audio(16, 2, 0);

        assert!(running);
        assert_eq!(steps, 2);
        assert_eq!(runner.guest_tick(), 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base);
    }

    #[test]
    fn gui_modaldialog_refire_unfreezes_prior_control_tracking() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 182);
        runner.set_guest_tick_for_test(182);
        runner.frozen_ticks = Some(182);
        runner.set_instructions_per_tick(1_000_000);
        runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(0, 0));

        let (steps, running) = runner.run_gui_slice_with_audio(16, 184, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.frozen_ticks, None);
        assert_eq!(runner.guest_tick(), 184);
        assert_eq!(runner.guest_tick(), 184);
    }

    #[test]
    fn gui_modaldialog_null_filter_fires_at_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let filter_proc = 0x0001_1000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.set_guest_tick_for_test(0);
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 0);
        assert_ne!(runner.m68k.cpu.read_reg(Register::PC), base);
        assert!(!runner.has_pending_sound_work());
        assert!(
            !runner
                .dispatcher
                .dialog_tracking
                .as_ref()
                .unwrap()
                .rendered_pixels_final
        );
    }

    #[test]
    fn gui_modaldialog_update_filter_fires_at_tick_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let filter_proc = 0x0001_1000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.set_guest_tick_for_test(0);
        runner
            .process_context
            .shared_event_queue()
            .push_back(QueuedEvent {
                what: 6,
                message: 0,
                when: 0,
                where_v: 0,
                where_h: 0,
                modifiers: 0,
            });
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.guest_tick(), 0);
        assert_ne!(runner.m68k.cpu.read_reg(Register::PC), base);
        assert!(
            !runner
                .dispatcher
                .dialog_tracking
                .as_ref()
                .unwrap()
                .rendered_pixels_final
        );
    }

    #[test]
    fn gui_modaldialog_mouse_down_goes_to_filter_before_default_handling() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let filter_proc = 0x0001_1000u32;
        runner.bus.write_word(base, 0xA991); // _ModalDialog
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.bus.write_long(0x016A, 0);
        runner.set_guest_tick_for_test(0);
        runner
            .process_context
            .shared_event_queue()
            .push_back(QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 12,
                where_h: 24,
                modifiers: 0,
            });
        let mut tracking = dialog_tracking_for_test(filter_proc, 0x0010_0100);
        tracking.items.push(DialogItem {
            item_type: 4,
            rect: (8, 16, 20, 30),
            text: String::from("OK"),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        });
        runner.dispatcher.dialog_tracking = Some(tracking);

        let (steps, running) = runner.run_gui_slice_with_audio(1, 0, 0);

        assert!(running);
        assert_eq!(steps, 1);
        assert_ne!(runner.m68k.cpu.read_reg(Register::PC), base);
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 1);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 12);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 24);
        assert!(
            runner.process_context.event_queue().is_empty(),
            "the filter callback should consume a queued button mouseDown event"
        );
    }

    #[test]
    fn modaldialog_filter_null_event_is_paced_per_guest_tick() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0001_1000u32;
        let dialog_ptr = 0x0020_0000u32;
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));
        runner.set_guest_tick_for_test(42);
        runner.bus.write_long(0x016A, 42);

        assert!(runner.should_fire_dialog_filter_proc());

        runner.dialog_filter_last_null_event_tick = Some((dialog_ptr, 42));
        assert!(
            !runner.should_fire_dialog_filter_proc(),
            "a synthetic null event should not refire twice in the same guest tick"
        );

        runner.set_guest_tick_for_test(43);
        runner.bus.write_long(0x016A, 43);
        assert!(
            runner.should_fire_dialog_filter_proc(),
            "the next guest tick should allow another ModalDialog null-event filter call"
        );
    }

    #[test]
    fn modaldialog_filter_real_events_bypass_null_event_pacing() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0001_1000u32;
        let dialog_ptr = 0x0020_0000u32;
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));
        runner.set_guest_tick_for_test(42);
        runner.bus.write_long(0x016A, 42);
        runner.dialog_filter_last_null_event_tick = Some((dialog_ptr, 42));
        runner
            .process_context
            .shared_event_queue()
            .push_back(QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 12,
                where_h: 34,
                modifiers: 0,
            });

        assert!(
            runner.should_fire_dialog_filter_proc(),
            "mouse/key/update events must still enter the filter immediately"
        );

        runner.process_context.shared_event_queue().clear();
        let window_ptr = runner.bus.alloc(170);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            window_ptr,
            0,
            100,
            120,
            220,
            360,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = window_ptr;
        runner.dialog_filter_last_null_event_tick = Some((window_ptr, 42));

        assert!(
            runner.should_fire_dialog_filter_proc(),
            "a pending updateEvt for the active dialog must bypass null-event pacing"
        );
    }

    #[test]
    fn spin_fastfwd_rejects_wrong_branch_target() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0x594F);
        runner.bus.write_word(base + 2, 0xA975);
        runner.bus.write_word(base + 4, 0x201F);
        runner.bus.write_word(base + 6, 0x5380);
        runner.bus.write_word(base + 8, 0xB680);
        // BHI.S with disp8 = 0xF6 (= -10, not -12). Target would
        // land at base+8, not at the SUBQ.W #4, A7 at base+0.
        runner.bus.write_word(base + 10, 0x62F6);

        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.m68k.cpu.write_reg(Register::D3, 500);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        let pc_after_trap = base + 4;
        let mut count = 0usize;
        runner.try_tickcount_spin_fastfwd(pc_after_trap, None, &mut count);

        assert_eq!(runner.guest_tick(), 100);
        assert_eq!(count, 0);
    }

    #[test]
    fn tickcount_runner_uses_canonical_dispatch_and_accounting() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        // Plain TickCount call: SUBQ.W #4, A7 ; _TickCount ; NOP
        runner.bus.write_word(base, 0x594F); // SUBQ.W #4, A7 (reserve LONGINT slot)
        runner.bus.write_word(base + 2, 0xA975); // _TickCount
        runner.bus.write_word(base + 4, 0x4E71); // NOP (sentinel)
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.set_guest_tick_for_test(0x1234_5678);
        runner.bus.write_long(0x016A, 0x1234_5678);
        runner.set_instructions_per_tick(1_000_000);

        let before_traps = runner.dispatcher.trap_count;
        // Two steps: the SUBQ first, then canonical trap dispatch.
        let (steps, running) = runner.run_steps(2, None);
        assert!(
            running,
            "runner should not halt on a canonical TickCount trap"
        );
        assert_eq!(steps, 2);
        assert_eq!(runner.bus.read_long(0x000F_FFFC), 0x1234_5678);
        assert_eq!(runner.dispatcher.trap_count - before_traps, 1);
    }

    #[test]
    fn halted_by_exit_to_shell_classifies_clean_application_quit() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        runner.bus.write_word(base, 0xA9F4); // _ExitToShell
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let (_steps, running) = runner.run_steps(1, None);

        assert!(!running, "ExitToShell should stop the runner");
        assert!(runner.is_halted());
        assert_eq!(runner.halted_trap(), Some(0xA9F4));
        assert!(
            runner.halted_by_exit_to_shell(),
            "ExitToShell halt must be classified as a clean application exit"
        );
    }

    #[test]
    fn unimplemented_trap_halts_at_the_faulting_instruction() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        runner.bus.write_word(base, 0xAFFE);
        runner.bus.write_word(base + 2, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);

        let (steps, running) = runner.run_steps(1, None);

        assert_eq!(steps, 1);
        assert!(
            !running,
            "an unclassified HLE row must fail closed (pc=${:08X})",
            runner.m68k.cpu.read_reg(Register::PC)
        );
        assert!(runner.is_halted());
        assert_eq!(runner.halted_pc(), Some(base));
        assert_eq!(runner.halted_trap(), Some(0xAFFE));
        assert_eq!(runner.halted_sp(), Some(sp));
    }

    #[test]
    fn exit_to_shell_activates_launch_target_queued_until_event_yield() {
        let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
        let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;

        runner
            .dispatcher
            .vfs
            .insert("Apps/Register Helper".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
        runner.dispatcher.ensure_vfs_catalog();
        runner
            .dispatcher
            .queue_pending_launch_application("Apps/Register Helper", true);
        runner.bus.write_word(base, 0xA9F4); // _ExitToShell
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let (_steps, running) = runner.run_steps(1, None);

        assert!(
            running,
            "ExitToShell should activate a valid queued launch target"
        );
        assert!(!runner.is_halted());
        assert_eq!(
            runner.dispatcher.launched_app_path(),
            Some("Apps/Register Helper")
        );
    }

    #[test]
    fn exit_to_shell_launches_best_application_created_by_installer() {
        let app_code0 = minimal_code0(0, 0x2000, 0, 0);
        let app_fork = make_resource_fork_bytes(&[(*b"CODE", 0, &app_code0)]);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;

        runner
            .dispatcher
            .vfs
            .insert("Existing/Previous Game".to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert("Existing/Previous Game".to_string(), app_fork.clone());
        runner.dispatcher.set_vfs_entry_finfo(
            "Existing/Previous Game",
            u32::from_be_bytes(*b"APPL"),
            u32::from_be_bytes(*b"GAME"),
            0,
        );
        runner.arm_installer_handoff();
        for path in ["Installed/Register", "Installed/Main Game"] {
            runner.dispatcher.vfs.insert(path.to_string(), Vec::new());
            runner
                .dispatcher
                .vfs_rsrc
                .insert(path.to_string(), app_fork.clone());
            runner.dispatcher.set_vfs_entry_finfo(
                path,
                u32::from_be_bytes(*b"APPL"),
                u32::from_be_bytes(*b"GAME"),
                0,
            );
        }
        runner.bus.write_word(base, 0xA9F4); // _ExitToShell
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let (_steps, running) = runner.run_steps(1, None);

        assert!(running, "installer exit should activate the installed game");
        assert!(!runner.is_halted());
        assert_eq!(
            runner.dispatcher.launched_app_path(),
            Some("Installed/Main Game")
        );
    }

    #[test]
    fn halted_by_exit_to_shell_rejects_invalid_pc_halts() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner
            .m68k
            .cpu
            .write_reg(Register::PC, runner.bus.ram_size());
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

        let (_steps, running) = runner.run_steps(1, None);

        assert!(!running, "invalid PC should stop the runner");
        assert!(runner.is_halted());
        assert_eq!(runner.halted_trap(), None);
        assert!(
            !runner.halted_by_exit_to_shell(),
            "invalid-PC halts must not be reported as clean application exits"
        );
    }

    #[test]
    fn ptinrect_runner_dispatch_matches_pascal_stack_contract() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let rect = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA8AD); // _PtInRect
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.set_instructions_per_tick(1_000_000);

        runner.bus.write_long(sp, rect);
        runner.bus.write_word(sp + 4, 20); // pt.v
        runner.bus.write_word(sp + 6, 30); // pt.h
        runner.bus.write_word(rect, 10); // top
        runner.bus.write_word(rect + 2, 25); // left
        runner.bus.write_word(rect + 4, 40); // bottom
        runner.bus.write_word(rect + 6, 50); // right

        let before_traps = runner.dispatcher.trap_count;
        let before_game = runner.dispatcher.game_trap_count;

        let (steps, running) = runner.run_steps(1, None);

        assert!(
            running,
            "runner should not halt on canonical PtInRect dispatch"
        );
        assert_eq!(steps, 1);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + 8);
        assert_eq!(runner.bus.read_word(sp + 8), 0x0100);
        assert_eq!(runner.dispatcher.trap_count - before_traps, 1);
        assert_eq!(runner.dispatcher.game_trap_count - before_game, 1);
    }

    #[test]
    fn eventavail_runner_dispatch_peeks_without_dequeueing() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let sp = 0x0010_0000u32;
        let event = 0x0020_0000u32;

        runner.bus.write_word(base, 0xA971); // _EventAvail
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.set_instructions_per_tick(1_000_000);
        runner.bus.write_long(sp, event);
        runner.bus.write_word(sp + 4, 0x0008); // keyDownMask
        runner.push_key_down(0x31, b' ');

        let before_traps = runner.dispatcher.trap_count;
        let before_game = runner.dispatcher.game_trap_count;

        let (steps, running) = runner.run_steps(1, None);

        assert!(
            running,
            "runner should not halt on canonical EventAvail dispatch"
        );
        assert_eq!(steps, 1);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 2);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + 6);
        assert_eq!(runner.bus.read_word(sp + 6), 0x0100);
        assert_eq!(runner.bus.read_word(event), 3);
        assert_eq!(
            runner.bus.read_long(event + 2),
            (0x31u32 << 8) | u32::from(b' ')
        );
        assert_eq!(
            runner.process_context.event_queue().len(),
            1,
            "EventAvail must not dequeue the matching event"
        );
        assert_eq!(runner.dispatcher.trap_count - before_traps, 1);
        assert_eq!(
            runner.dispatcher.game_trap_count, before_game,
            "EventAvail remains excluded from game_trap_count as an idle trap"
        );
    }

    #[test]
    fn tick_progress_persists_across_multiple_run_slices() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 12;

        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(5);

        let (steps1, running1) = runner.run_steps(3, None);
        let (steps2, running2) = runner.run_steps(3, None);

        assert!(running1);
        assert!(running2);
        assert_eq!(steps1, 3);
        assert_eq!(steps2, 3);
        assert_eq!(runner.bus.read_long(0x016A), 1);
    }

    #[test]
    fn tick_override_breaks_once_target_tick_is_reached() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let program_words = 16;

        for offset in (0..program_words).step_by(2) {
            runner.bus.write_word(program_start + offset, 0x4E71);
        }

        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_instructions_per_tick(4);

        let (steps, running) = runner.run_steps_with_audio(16, Some(0), 0);

        assert!(running);
        assert_eq!(steps, 3);
        assert_eq!(runner.bus.read_long(0x016A), 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_advance_in_headless_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 3;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 3);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_capped_to_zero_in_headless() {
        // `cap=Some(0)` is the scripted default — `WaitNextEvent`
        // sleep is treated as a zero-cost return (matching real Mac OS
        // where WNE doesn't directly tick; only the VBL hardware
        // interrupt does).
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_wait_sleep_cap_in_headless(Some(0));
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (_steps, _running) = runner.run_steps(1, None);

        // Zero ticks advanced (cap=0).
        assert_eq!(runner.bus.read_long(0x016A), 0);
        // But pending sleep is cleared so the game resumes immediately.
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_capped_in_headless_when_opt_in() {
        // Headless callers (e.g. scripted harnesses) can opt in to a
        // per-WNE-call sleep tick cap matching GUI mode, preventing
        // tick counts from racing ahead of real-Mac VBL pacing during
        // event-loop-heavy gameplay.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_wait_sleep_cap_in_headless(Some(1));
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        // Only 1 tick advanced (cap), not the full 60.
        assert_eq!(runner.bus.read_long(0x016A), 1);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert_eq!(runner.wait_sleep_cap_in_headless(), Some(1));
    }

    #[test]
    fn pending_wait_sleep_ticks_suspends_foreground_until_gui_tick_cap() {
        // In GUI mode (tick_override=Some), WNE sleep advances VBL/timer time
        // up to the current frame cap but keeps the foreground app suspended
        // until the requested sleep expires. This prevents sleep=60 loops from
        // receiving 60 null events per second. Inside Macintosh: Processes
        // 1994, p. 2-8.
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(
            steps, 0,
            "foreground code should not resume while WNE sleep remains pending"
        );
        assert_eq!(runner.bus.read_long(0x016A), 10);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
    }

    #[test]
    fn pending_wait_sleep_ticks_wakes_wait_next_event_with_queued_input() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.set_sent_open_app_event_for_test(true);
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });
        runner.push_mouse_down(123, 456);

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.bus.read_word(event_ptr),
            1,
            "queued mouseDown should replace the pending null EventRecord"
        );
        assert_eq!(runner.bus.read_word(event_ptr + 10), 123u16);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 456u16);
        assert_eq!(
            runner.bus.read_word(result_ptr),
            0xFFFF,
            "WaitNextEvent result slot should be rewritten to TRUE"
        );
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
    }

    #[test]
    fn push_mouse_down_wakes_pending_wait_next_event_immediately() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.set_sent_open_app_event_for_test(true);
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });

        runner.push_mouse_down(123, 456);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            1,
            "input injection should wake a sleeping WaitNextEvent before the next CPU slice"
        );
        assert_eq!(runner.bus.read_word(event_ptr + 10), 123u16);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 456u16);
        assert_eq!(runner.bus.read_word(result_ptr), 0xFFFF);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
    }

    #[test]
    fn set_mouse_position_wakes_pending_wait_next_event_with_mouse_moved_region() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;
        let mouse_rgn = test_region_handle(&mut runner.bus, 10, 20, 30, 40);

        runner.set_mouse_position(20, 25);
        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.set_sent_open_app_event_for_test(true);
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0x8000,
            mouse_rgn,
            resume_pc: None,
            resume_sp: None,
        });

        runner.set_mouse_position(50, 25);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            15,
            "mouse movement outside the pending mouseRgn should wake WaitNextEvent with osEvt"
        );
        assert_eq!(runner.bus.read_long(event_ptr + 2), 0xFA00_0000);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 50u16);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 25u16);
        assert_eq!(runner.bus.read_word(result_ptr), 0xFFFF);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
        assert_eq!(
            runner.dispatcher.debug_mouse_moved_event_count, 1,
            "async wake path should share the normal mouse-moved event accounting"
        );
    }

    #[test]
    fn set_mouse_position_leaves_pending_wait_next_event_asleep_without_event() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(program_start, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.tick_budget = 0;
        runner.bus.write_word(result_ptr, 0xFFFF);
        runner.dispatcher.set_sent_open_app_event_for_test(true);
        runner.dispatcher.write_event_record(
            &mut runner.bus,
            event_ptr,
            0xFFFF,
            0xABCD_EF01,
            0,
            1,
            2,
            3,
        );
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });

        runner.set_mouse_position(123, 456);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            0xFFFF,
            "mouse movement with no mouseRgn event should not rewrite the parked event record"
        );
        assert_eq!(runner.bus.read_long(event_ptr + 2), 0xABCD_EF01);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 1);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 2);
        assert_eq!(
            runner.bus.read_word(result_ptr),
            0xFFFF,
            "the pending WaitNextEvent result must remain untouched"
        );
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 60);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_some());
        assert_eq!(runner.bus.read_word(0x0828), 123u16);
        assert_eq!(runner.bus.read_word(0x082A), 456u16);

        let (steps, running) = runner.run_steps(1, Some(110));
        assert!(running);
        assert_eq!(
            steps, 0,
            "foreground code must remain suspended while WaitNextEvent is asleep"
        );
        assert_eq!(
            runner.bus.read_long(0x016A),
            110,
            "the original WaitNextEvent sleep should continue toward expiry"
        );
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
    }

    #[test]
    fn push_mouse_down_leaves_pending_wait_next_event_parked_during_interrupt_callback() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;

        runner.bus.write_word(result_ptr, 0);
        runner.dispatcher.set_sent_open_app_event_for_test(true);
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: None,
            resume_sp: None,
        });
        runner.active_interrupt_callback = Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::Timer,
            resume_pc: interrupted_pc,
            resume_sp: interrupted_sp,
            d_regs: [0; 8],
            a_regs: [0, 0, 0, 0, 0, 0, 0, interrupted_sp],
            sr: 0x2000,
            ccr: 0,
            restore_port: None,
        });

        runner.push_mouse_down(123, 456);

        assert_eq!(
            runner.bus.read_word(event_ptr),
            0,
            "input must not rewrite a foreground WaitNextEvent record while an interrupt callback is active"
        );
        assert_eq!(runner.bus.read_word(result_ptr), 0);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 60);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_some());
        assert!(
            runner
                .process_context
                .event_queue()
                .iter()
                .any(|event| event.what == 1 && event.where_v == 123 && event.where_h == 456),
            "the mouseDown should remain queued for the foreground event loop"
        );
    }

    #[test]
    fn pending_wait_next_event_drops_stale_return_after_foreground_moves_on() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let parked_pc = 0x0001_0000;
        let stale_pc = 0x0001_0010;
        let parked_sp = 0x007F_FFC0;
        let event_ptr = 0x0020_0000;
        let result_ptr = 0x0020_0020;

        runner.bus.write_word(stale_pc, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, stale_pc);
        runner.m68k.cpu.write_reg(Register::A7, parked_sp);
        runner.bus.write_word(result_ptr, 0xA582);
        runner.dispatcher.set_sent_open_app_event_for_test(true);
        runner
            .dispatcher
            .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 60;
        runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
            event_ptr,
            result_ptr,
            event_mask: 0xFFFF,
            mouse_rgn: 0,
            resume_pc: Some(parked_pc),
            resume_sp: Some(parked_sp),
        });
        runner.push_mouse_down(123, 456);

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(
            runner.bus.read_word(result_ptr),
            0xA582,
            "a stale WaitNextEvent return slot may now belong to a caller frame"
        );
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
        assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
        assert!(
            runner
                .process_context
                .event_queue()
                .iter()
                .any(|event| event.what == 1 && event.where_v == 123 && event.where_h == 456),
            "stale WNE cleanup should not silently consume a queued event"
        );
    }

    #[test]
    fn push_mouse_down_restores_foreground_budget_before_next_tick_cap_run() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.tick_budget = 0;

        runner.push_mouse_down(123, 456);

        let (steps, running) = runner.run_steps(1, Some(110));

        assert!(running);
        assert_eq!(
            steps, 1,
            "input injected at an exhausted tick boundary should let foreground code run"
        );
        assert_eq!(
            runner.bus.read_long(0x016A),
            100,
            "foreground input wake must not spend the next slice only advancing ticks"
        );
    }

    #[test]
    fn set_mouse_position_restores_foreground_budget_before_next_tick_cap_run() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 100);
        runner.set_guest_tick_for_test(100);
        runner.tick_budget = 0;

        runner.set_mouse_position(123, 456);

        let (steps, running) = runner.run_steps(1, Some(110));

        assert!(running);
        assert_eq!(
            steps, 1,
            "mouse movement at an exhausted tick boundary should let polling foreground code run"
        );
        assert_eq!(
            runner.bus.read_long(0x016A),
            100,
            "foreground mouse-move wake must not spend the next slice only advancing ticks"
        );
    }

    #[test]
    fn pending_wait_sleep_ticks_honors_app_owned_visible_dialog_snapshot_in_gui_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let dialog_ptr = 0x0020_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.dialog_visible_snapshots.insert(
            dialog_ptr,
            crate::trap::dispatch::PersistentDialogSnapshot {
                bounds: (10, 10, 40, 40),
                pixels: Vec::new().into(),
            },
        );
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(
            steps, 0,
            "app-owned visible dialogs must not collapse WaitNextEvent sleep before ModalDialog"
        );
        assert_eq!(runner.bus.read_long(0x016A), 10);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
    }

    #[test]
    fn pending_wait_sleep_ticks_honors_app_owned_visible_dialog_snapshot_in_headless_cap_zero() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let dialog_ptr = 0x0020_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.set_wait_sleep_cap_in_headless(Some(0));
        runner.dispatcher.dialog_visible_snapshots.insert(
            dialog_ptr,
            crate::trap::dispatch::PersistentDialogSnapshot {
                bounds: (10, 10, 40, 40),
                pixels: Vec::new().into(),
            },
        );
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(
            steps, 1,
            "headless cap zero must not collapse app-owned dialog sleep"
        );
        assert_eq!(runner.bus.read_long(0x016A), 60);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_collapses_retained_modaldialog_snapshot_in_gui_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;
        let dialog_ptr = 0x0020_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.dialog_visible_snapshots.insert(
            dialog_ptr,
            crate::trap::dispatch::PersistentDialogSnapshot {
                bounds: (10, 10, 40, 40),
                pixels: Vec::new().into(),
            },
        );
        runner.dispatcher.dialog_modal_entered.insert(dialog_ptr);
        runner.dispatcher.pending_wait_sleep_ticks = 60;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(
            steps, 1,
            "retained ModalDialog snapshots keep the existing app-yield path"
        );
        assert_eq!(runner.bus.read_long(0x016A), 0);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_wait_sleep_ticks_resumes_when_gui_sleep_expires_before_cap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_wait_sleep_ticks = 3;

        let (steps, running) = runner.run_steps(1, Some(10));

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 3);
        assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    }

    #[test]
    fn pending_delay_ticks_advance_in_gui_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let program_start = 0x0001_0000;

        runner.bus.write_word(program_start, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, program_start);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_delay_ticks = 3;

        let (steps, _running) = runner.run_steps(1, Some(10));

        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 3);
        assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 3);
    }

    #[test]
    fn dialog_callback_scratch_preserves_materialized_toolbox_trap_table() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let table_start = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE;
        let table_end =
            table_start + u32::from(crate::trap::dispatch::TOOLBOX_TRAP_TABLE_SLOTS) * 4;
        let scratch_start = runner.dialog_callback_scratch_base();
        let scratch_end = scratch_start + DIALOG_CALLBACK_SCRATCH_SIZE;
        assert!(scratch_end <= table_start || scratch_start >= table_end);

        const SHOW_WINDOW: u16 = 0xA915;
        let show_window_entry = table_start + u32::from(SHOW_WINDOW & 0x03FF) * 4;
        let original_show_window = runner.bus.read_long(show_window_entry);
        assert_eq!(
            runner
                .dispatcher
                .native_trap_handler(&runner.bus, SHOW_WINDOW),
            None
        );
        let filter_proc = 0x0004_2000u32;
        runner.bus.write_word(filter_proc, 0x4E56);
        runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));

        assert!(runner.fire_dialog_filter_proc());
        assert_eq!(
            runner.bus.read_long(show_window_entry),
            original_show_window
        );
        assert_eq!(
            runner
                .dispatcher
                .native_trap_handler(&runner.bus, SHOW_WINDOW),
            None
        );
    }

    #[test]
    fn dialog_filter_synthesized_null_event_uses_live_modifiers() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0004_2000u32;

        runner.bus.write_word(filter_proc, 0x4E56);
        runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.dispatcher.set_mouse_position(222, 333);
        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr: 0x0020_0000,
            bounds: (100, 200, 200, 360),
            title: String::new(),
            proc_id: 2,
            items: Vec::new(),
            default_item: 1,
            cancel_item: 2,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new().into(),
            stack_ptr: 0x007F_FFC0,
            item_hit_ptr: 0x0030_0000,
            rendered_pixels: Vec::new().into(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: std::collections::VecDeque::new(),
            draw_procs_done: true,
            rendered_pixels_final: true,
            filter_presentation_epoch: None,
            filter_proc,
            game_managed: true,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_filter_proc());
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 0);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 222);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 333);
        assert_eq!(
            runner.bus.read_word(event_ptr + 14),
            runner.dispatcher.current_event_modifiers()
        );
        assert_eq!(
            runner.dialog_filter_last_null_event_tick,
            Some((0x0020_0000, 0))
        );
    }

    #[test]
    fn dialog_filter_uses_active_dialog_pending_update_before_null_event() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0004_2000u32;
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(filter_proc, 0x4E56);
        runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            0,
            100,
            120,
            220,
            360,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.process_context.shared_event_queue().clear();
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = dialog_ptr;

        assert!(runner.fire_dialog_filter_proc());
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 6);
        assert_eq!(runner.bus.read_long(event_ptr + 2), dialog_ptr);
        assert_eq!(runner.dialog_filter_last_null_event_tick, None);
    }

    #[test]
    fn dialog_filter_paces_synthetic_update_without_starving_queued_input() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let filter_proc = 0x0004_2000u32;
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(filter_proc, 0x4E56);
        runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.bus.write_long(0x016A, 17);
        runner.set_guest_tick_for_test(17);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            0,
            100,
            120,
            220,
            360,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.process_context.shared_event_queue().clear();
        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = dialog_ptr;

        assert!(runner.fire_dialog_filter_proc());
        let event_ptr = runner.dialog_filter_event;
        assert_eq!(runner.bus.read_word(event_ptr), 6);
        assert_eq!(runner.bus.read_long(event_ptr + 2), dialog_ptr);

        runner.active_interrupt_callback = None;
        runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .last_filter_event = None;
        assert!(
            !runner.dialog_filter_has_real_event_pending(dialog_ptr),
            "the same invalid-region update should not refire indefinitely in one guest tick"
        );

        runner
            .process_context
            .shared_event_queue()
            .push_back(QueuedEvent {
                what: 1,
                message: 0,
                when: 0,
                where_v: 123,
                where_h: 234,
                modifiers: 0,
            });
        assert!(
            runner.dialog_filter_has_real_event_pending(dialog_ptr),
            "queued user input must bypass synthetic update pacing"
        );
        assert!(runner.fire_dialog_filter_proc());
        assert_eq!(runner.bus.read_word(event_ptr), 1);
        assert_eq!(runner.bus.read_word(event_ptr + 10), 123);
        assert_eq!(runner.bus.read_word(event_ptr + 12), 234);
        assert!(
            runner.process_context.event_queue().is_empty(),
            "the queued mouse event should be consumed by the filter call"
        );

        runner.active_interrupt_callback = None;
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .last_filter_event = None;
        runner.bus.write_long(0x016A, 18);
        runner.set_guest_tick_for_test(18);
        assert!(
            runner.dialog_filter_has_real_event_pending(dialog_ptr),
            "a still-invalid dialog can surface another update event on the next guest tick"
        );
    }

    #[test]
    fn dialog_filter_proc_leaves_dialog_port_current() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let main_port = runner.bus.alloc(170);
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            main_port,
            0,
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
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.dispatcher.set_current_port_state(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            main_port,
            None,
        );
        let filter_proc = runner.bus.alloc(8);
        runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry

        runner.dispatcher.dialog_tracking =
            Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
        runner
            .dispatcher
            .dialog_tracking
            .as_mut()
            .unwrap()
            .dialog_ptr = dialog_ptr;

        assert!(runner.fire_dialog_filter_proc());
        assert_eq!(*runner.dispatcher.current_port, dialog_ptr);
        assert_eq!(
            runner
                .active_interrupt_callback
                .as_ref()
                .and_then(|callback| callback.restore_port),
            None
        );

        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        let (_steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(*runner.dispatcher.current_port, dialog_ptr);
        assert!(runner.active_interrupt_callback.is_none());
    }

    #[test]
    fn nested_dialog_callbacks_restore_parent_trampoline_and_child_filter_result() {
        for child_filter in [false, true] {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            let foreground = 0x0001_0000;
            let parent = 0x0004_2000;
            let child = 0x0004_3000;
            let busy = 0x0005_0000;
            let sp = 0x007F_FFC0;
            runner.bus.write_word(foreground, 0x60FE);
            runner.m68k.cpu.write_reg(Register::PC, foreground);
            runner.m68k.cpu.write_reg(Register::A7, sp);
            runner.bus.write_byte(busy, 1);
            // Parent draws until the test releases it, then returns normally.
            for (i, word) in [0x4E56, 0, 0x4A39, 5, 0, 0x66F8, 0x4E5E, 0x4E75]
                .into_iter()
                .enumerate()
            {
                runner.bus.write_word(parent + i as u32 * 2, word);
            }
            assert!(runner.inject_dialog_draw_proc(parent, 1, 0x0020_0000, false));
            runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
            let parent_sp = runner.m68k.cpu.read_reg(Register::A7);
            let parent_trampoline = runner.dialog_draw_trampoline;
            let parent_saved_sp = runner.bus.read_long(parent_trampoline + 22);
            let code = if child_filter {
                // Pascal Boolean TRUE at 20(A6); callee pops three pointers.
                vec![0x4E56, 0, 0x1D7C, 1, 20, 0x4E5E, 0x4E74, 12]
            } else {
                vec![0x4E56, 0, 0x4E5E, 0x4E75]
            };
            for (i, word) in code.into_iter().enumerate() {
                runner.bus.write_word(child + i as u32 * 2, word);
            }
            if child_filter {
                runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(child, 0x0030_0000));
                assert!(runner.fire_dialog_filter_proc());
            } else {
                assert!(runner.inject_dialog_draw_proc(child, 2, 0x0020_1000, false));
            }
            assert_eq!(runner.nested_dialog_calls.len(), 1);
            runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
            assert!(runner.nested_dialog_calls.is_empty());
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parent_sp);
            assert_eq!(
                runner.bus.read_long(parent_trampoline + 22),
                parent_saved_sp
            );
            if child_filter {
                assert_eq!(
                    runner
                        .bus
                        .read_word(runner.dispatcher.dialog_filter_result_addr)
                        & 0x0100,
                    0x0100
                );
            }
            runner.bus.write_byte(busy, 0);
            runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
            assert!(runner.active_interrupt_callback.is_none());
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        }
    }

    #[test]
    fn sound_completion_interrupts_and_resumes_a_waiting_dialog_filter() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let foreground = 0x0001_0000;
        let filter = 0x0004_2000;
        let callback = 0x0004_3000;
        let busy = 0x0005_0000;
        let finished = busy + 1;
        let sp = 0x007F_FFC0;
        runner.bus.write_word(foreground, 0x60FE);
        runner.m68k.cpu.write_reg(Register::PC, foreground);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_byte(busy, 1);
        // LINK; wait: TST.B busy; BNE wait; ST finished; UNLK; RTD #12.
        let code = [
            0x4E56, 0, 0x4A39, 5, 0, 0x66F8, 0x50F9, 5, 1, 0x4E5E, 0x4E74, 12,
        ];
        for (i, word) in code.into_iter().enumerate() {
            runner.bus.write_word(filter + i as u32 * 2, word);
        }
        // Sound completion: CLR.B busy; RTS.
        for (i, word) in [0x4239, 5, 0, 0x4E75].into_iter().enumerate() {
            runner.bus.write_word(callback + i as u32 * 2, word);
        }
        let mut tracking = dialog_tracking_for_test(filter, 0x0030_0000);
        tracking.dialog_ptr = 0x0020_0000;
        runner.dispatcher.dialog_tracking = Some(tracking);
        assert!(runner.fire_dialog_filter_proc());
        runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
        let paused_pc = runner.m68k.cpu.read_reg(Register::PC);
        let paused_sp = runner.m68k.cpu.read_reg(Register::A7);
        let tick = runner.guest_tick();
        runner
            .dispatcher
            .sound_manager
            .queue_sound_callback(PendingSoundCallback::Command {
                architecture: CallbackTaskArchitecture::M68k,
                callback_addr: callback,
                chan_ptr: 0x0039_38C8,
                cmd: SndCommand {
                    cmd: crate::sound::cmd::CALLBACK,
                    param1: 0,
                    param2: 0,
                },
            });
        let (_, running) = runner.run_pending_sound_work(1000);
        assert!(running);
        assert_eq!(runner.bus.read_byte(busy), 0);
        assert_eq!(
            runner.bus.read_byte(finished),
            0,
            "audio service ran foreground code"
        );
        assert_eq!(runner.guest_tick(), tick);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), paused_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), paused_sp);
        assert!(matches!(
            runner.active_interrupt_callback.map(|c| c.source),
            Some(ActiveInterruptCallbackSource::DialogFilterProc)
        ));
        runner.run_gui_cpu_slice(100, tick + 1);
        assert_eq!(runner.bus.read_byte(finished), 0xFF);
        assert!(runner.active_interrupt_callback.is_none());
        assert!(runner.suspended_dialog_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn dialog_draw_callback_delay_respects_gui_deadlines_and_returns_final_ticks() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let foreground = 0x0001_0000;
        let callback = 0x0004_2000;
        let sp = 0x007F_FFC0;
        runner.bus.write_word(foreground, 0x60FE);
        runner.m68k.cpu.write_reg(Register::PC, foreground);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.set_instructions_per_tick(1_000_000);
        // LINK; MOVEA.L #2,A0; _Delay; MOVE.L D0,$50000; UNLK; RTS.
        for (i, word) in [
            0x4E56, 0, 0x207C, 0, 2, 0xA03B, 0x23C0, 5, 0, 0x4E5E, 0x4E75,
        ]
        .into_iter()
        .enumerate()
        {
            runner.bus.write_word(callback + i as u32 * 2, word);
        }
        assert!(runner.inject_dialog_draw_proc(callback, 1, 0x0020_0000, false));
        let tick = runner.guest_tick();
        runner.run_gui_cpu_slice(100, tick + 1);
        assert_eq!(runner.guest_tick(), tick + 1);
        assert_eq!(runner.dispatcher.pending_delay_ticks, 1);
        assert_eq!(runner.bus.read_long(0x0005_0000), 0);
        runner.run_gui_cpu_slice(100, tick + 2);
        runner.run_gui_cpu_slice(100, tick + 3);
        assert_eq!(runner.bus.read_long(0x0005_0000), tick + 2);
        assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    }

    #[test]
    fn dialog_callbacks_can_wait_for_ticks_across_gui_slices() {
        for filter in [false, true] {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            let foreground = 0x0001_0000;
            let proc_addr = 0x0004_2000;
            let sp = 0x007F_FFC0;
            runner.bus.write_word(foreground, 0x60FE); // BRA.S *
            runner.m68k.cpu.write_reg(Register::PC, foreground);
            runner.m68k.cpu.write_reg(Register::A7, sp);
            runner.instructions_per_tick = 32;
            runner.tick_budget = 32;
            // LINK A6,#0; MOVE.L Ticks,D0; wait: CMP.L Ticks,D0;
            // BEQ.S wait; UNLK A6; RTS (draw) / RTD #12 (filter).
            let code = [
                0x4E56,
                0,
                0x2038,
                0x016A,
                0xB0B8,
                0x016A,
                0x67FA,
                0x4E5E,
                if filter { 0x4E74 } else { 0x4E75 },
                12,
            ];
            for (i, word) in code.into_iter().enumerate() {
                runner.bus.write_word(proc_addr + i as u32 * 2, word);
            }
            if filter {
                let mut tracking = dialog_tracking_for_test(0, 0);
                tracking.dialog_ptr = 0x0020_0000;
                tracking.filter_proc = proc_addr;
                runner.dispatcher.dialog_tracking = Some(tracking);
                assert!(runner.fire_dialog_filter_proc());
            } else {
                assert!(runner.inject_dialog_draw_proc(proc_addr, 1, 0x0020_0000, false));
            }
            let tick = runner.guest_tick();
            let (_, running) = runner.run_gui_cpu_slice(500, tick + 1);
            assert!(running);
            assert_eq!(runner.guest_tick(), tick + 1, "filter={filter}");
            assert!(runner.active_interrupt_callback.is_some());
            let (_, running) = runner.run_gui_cpu_slice(500, tick + 2);
            assert!(running);
            assert!(
                runner.active_interrupt_callback.is_none(),
                "filter={filter}"
            );
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
        }
    }

    #[test]
    fn dialog_draw_proc_trampoline_passes_item_first_and_tolerates_plain_rts() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let dialog_ptr = 0x0020_0000u32;
        let proc_addr = 0x0004_2000u32;
        let item_no = 5i16;

        // Keep foreground execution stable after the callback returns.
        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        // MPW-style proc prologue shape. It returns with plain RTS, leaving
        // callback parameters on the stack; the trampoline must restore A7.
        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS

        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (0, 0, 64, 64),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new().into(),
            stack_ptr: interrupted_sp,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new().into(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
            draw_procs_done: false,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_draw_procs());
        let tramp = runner.dialog_draw_trampoline;
        assert_eq!(runner.bus.read_word(tramp), 0x48E7);
        assert_eq!(runner.bus.read_word(tramp + 4), 0x2F3C);
        assert_eq!(runner.bus.read_long(tramp + 6), dialog_ptr);
        assert_eq!(runner.bus.read_word(tramp + 10), 0x3F3C);
        assert_eq!(runner.bus.read_word(tramp + 12), item_no as u16);
        assert_eq!(runner.bus.read_word(tramp + 14), 0x4EB9);
        assert_eq!(runner.bus.read_long(tramp + 16), proc_addr);
        assert_eq!(runner.bus.read_word(tramp + 20), 0x4FF9);
        assert_eq!(runner.bus.read_long(tramp + 22), interrupted_sp - 36);

        let (_steps, running) = runner.run_steps(16, None);

        assert!(running);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "dialog callback should have resumed foreground code"
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn modal_dialog_draw_procs_drain_before_foreground_code_resumes() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let proc_1 = 0x0004_2000u32;
        let proc_2 = 0x0004_2100u32;

        // Model an application loop that does not immediately call
        // ModalDialog again after the first injected callback returns.
        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        for proc_addr in [proc_1, proc_2] {
            runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
            runner.bus.write_word(proc_addr + 2, 0x0000);
            runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
            runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
        }

        let mut tracking = dialog_tracking_for_test(0, 0);
        tracking.draw_proc_queue = VecDeque::from([(proc_1, 3), (proc_2, 4)]);
        tracking.draw_procs_done = false;
        tracking.rendered_pixels_final = false;
        runner.dispatcher.dialog_tracking = Some(tracking);

        assert!(runner.fire_dialog_draw_procs());
        runner.deferred_tracking_refire_pc = Some(interrupted_pc + 2);
        let (_steps, running) = runner.run_steps(128, None);

        assert!(running);
        let tracking = runner.dispatcher.dialog_tracking.as_ref().unwrap();
        assert!(tracking.draw_proc_queue.is_empty());
        assert!(tracking.draw_procs_done);
        assert!(runner.active_interrupt_callback.is_none());
        assert!(runner.deferred_tracking_refire_pc.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    }

    #[test]
    fn modeless_dialog_draw_proc_accepts_a5_relative_proc_ptr() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let a5 = 0x0020_0000u32;
        let proc_offset = 0x0000_4200u32;
        let proc_addr = a5 + proc_offset;
        let dialog_ptr = runner.bus.alloc(170);

        runner.bus.write_word(interrupted_pc, 0x60FE);
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.write_reg(Register::A5, a5);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
        runner
            .dispatcher
            .modeless_dialog_draw_proc_queue
            .push_back((dialog_ptr, proc_offset, 5));

        assert!(runner.fire_modeless_dialog_draw_proc());

        let tramp = runner.dialog_draw_trampoline;
        assert_eq!(runner.bus.read_long(tramp + 16), proc_addr);
        assert_eq!(
            runner.dispatcher.active_modeless_dialog_draw_proc,
            Some(dialog_ptr)
        );
    }

    #[test]
    fn modeless_dialog_draw_procs_drain_after_plain_trap() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let base = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let dialog_ptr = runner.bus.alloc(170);
        let proc_1 = 0x0004_2000u32;
        let proc_2 = 0x0004_2100u32;

        runner.bus.write_word(base, 0xA861); // _Random
        runner.bus.write_word(base + 2, 0x60FE); // BRA.S *-0
        runner.m68k.cpu.write_reg(Register::PC, base);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );

        for proc_addr in [proc_1, proc_2] {
            runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
            runner.bus.write_word(proc_addr + 2, 0x0000);
            runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
            runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
        }
        runner.dispatcher.modeless_dialog_draw_proc_queue =
            VecDeque::from([(dialog_ptr, proc_1, 3), (dialog_ptr, proc_2, 5)]);

        let (_steps, running) = runner.run_steps(128, None);

        assert!(running);
        assert!(runner.dispatcher.modeless_dialog_draw_proc_queue.is_empty());
        assert_eq!(runner.dispatcher.active_modeless_dialog_draw_proc, None);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "modeless draw callbacks should have returned to foreground code"
        );
    }

    #[test]
    fn dialog_draw_proc_does_not_restore_over_guest_selected_dialog_port() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let main_port = runner.bus.alloc(170);
        let dialog_ptr = runner.bus.alloc(170);
        let proc_addr = 0x0004_2000u32;
        let item_no = 5i16;

        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            main_port,
            0,
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
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );
        runner.dispatcher.set_current_port_state(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            main_port,
            None,
        );

        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x2F3C); // MOVE.L #dialog,-(SP)
        runner.bus.write_long(proc_addr + 6, dialog_ptr);
        runner.bus.write_word(proc_addr + 10, 0xA873); // _SetPort
        runner.bus.write_word(proc_addr + 12, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 14, 0x4E75); // RTS

        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (120, 180, 240, 420),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new().into(),
            stack_ptr: interrupted_sp,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new().into(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
            draw_procs_done: false,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_draw_procs());
        assert_eq!(
            runner
                .active_interrupt_callback
                .as_ref()
                .and_then(|callback| callback.restore_port),
            None
        );

        let (_steps, running) = runner.run_steps(32, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(
            *runner.dispatcher.current_port, dialog_ptr,
            "Dialog Manager must leave the dialog port current after the draw proc"
        );
    }

    #[test]
    fn dialog_draw_proc_restores_parent_grafport_state_and_clip() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let dialog_ptr = runner.bus.alloc(170);
        let other_port = runner.bus.alloc(170);
        let proc_addr = 0x0004_2000u32;
        let clip_rect = 0x0004_2100u32;

        runner.bus.write_word(interrupted_pc, 0x60FE);
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        for port in [dialog_ptr, other_port] {
            runner.dispatcher.init_cgraf_window(
                &mut runner.bus,
                &mut runner.m68k.cpu,
                port,
                0,
                120,
                180,
                240,
                420,
                "",
                2,
                true,
                false,
                false,
                0,
            );
        }
        runner.dispatcher.set_current_port_state(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            dialog_ptr,
            None,
        );
        let clip_handle = runner.bus.read_long(dialog_ptr + 28);
        let clip_ptr = runner.bus.read_long(clip_handle);
        let expected_clip: Vec<u8> = (0..10)
            .map(|i| runner.bus.read_byte(clip_ptr + i))
            .collect();

        runner.bus.write_word(clip_rect, 1);
        runner.bus.write_word(clip_rect + 2, 2);
        runner.bus.write_word(clip_rect + 4, 3);
        runner.bus.write_word(clip_rect + 6, 4);
        let words = [
            0x4E56,
            0x0000, // LINK A6,#0
            0x3F3C,
            0x0007, // MOVE.W #7,-(SP), height
            0x3F3C,
            0x0006, // MOVE.W #6,-(SP), width
            0xA89B, // _PenSize
            0x3F3C,
            0x000C, // MOVE.W #12,-(SP)
            0xA89C, // _PenMode
            0x2F3C,
            (clip_rect >> 16) as u16,
            clip_rect as u16, // rect pointer
            0xA87B,           // _ClipRect
            0x2F3C,
            (other_port >> 16) as u16,
            other_port as u16, // port pointer
            0xA873,            // _SetPort
            0x4E5E,
            0x4E75, // UNLK; RTS
        ];
        for (i, word) in words.into_iter().enumerate() {
            runner.bus.write_word(proc_addr + i as u32 * 2, word);
        }
        runner.dispatcher.modeless_dialog_draw_proc_queue =
            VecDeque::from([(dialog_ptr, proc_addr, 5)]);

        assert!(runner.fire_modeless_dialog_draw_proc());
        let (_steps, running) = runner.run_steps(64, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(*runner.dispatcher.current_port, dialog_ptr);
        assert_eq!(runner.bus.read_word(dialog_ptr + 52), 1);
        assert_eq!(runner.bus.read_word(dialog_ptr + 54), 1);
        assert_eq!(runner.bus.read_word(dialog_ptr + 56), 8);
        assert_eq!(runner.bus.read_long(dialog_ptr + 28), clip_handle);
        let restored_clip_ptr = runner.bus.read_long(clip_handle);
        assert_eq!(
            (0..10)
                .map(|i| runner.bus.read_byte(restored_clip_ptr + i))
                .collect::<Vec<_>>(),
            expected_clip
        );
    }

    #[test]
    fn dialog_draw_proc_pascal_stack_places_item_number_before_window_pointer() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000u32;
        let interrupted_sp = 0x007F_FFC0u32;
        let dialog_ptr = 0x0029_4240u32;
        let proc_addr = 0x0004_2000u32;
        let item_no = 2i16;
        let seen_item_addr = 0x0004_3000u32;
        let seen_dialog_addr = 0x0004_3004u32;

        runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

        // PROCEDURE MyItem(theWindow: WindowPtr; itemNo: INTEGER);
        // Inside Macintosh Volume I, I-405. MPW Pascal prologues observe
        // itemNo at 8(A6) and theWindow at 10(A6).
        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x302E); // MOVE.W 8(A6),D0
        runner.bus.write_word(proc_addr + 6, 0x0008);
        runner.bus.write_word(proc_addr + 8, 0x33C0); // MOVE.W D0,(abs).L
        runner.bus.write_long(proc_addr + 10, seen_item_addr);
        runner.bus.write_word(proc_addr + 14, 0x222E); // MOVE.L 10(A6),D1
        runner.bus.write_word(proc_addr + 16, 0x000A);
        runner.bus.write_word(proc_addr + 18, 0x23C1); // MOVE.L D1,(abs).L
        runner.bus.write_long(proc_addr + 20, seen_dialog_addr);
        runner.bus.write_word(proc_addr + 24, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 26, 0x4E75); // RTS

        runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
            dialog_ptr,
            bounds: (120, 180, 240, 420),
            title: String::new(),
            proc_id: 1,
            items: Vec::new(),
            default_item: 0,
            cancel_item: 0,
            edit_text: String::new(),
            edit_item: 0,
            saved_pixels: Vec::new().into(),
            stack_ptr: interrupted_sp,
            item_hit_ptr: 0,
            rendered_pixels: Vec::new().into(),
            flash_remaining: 0,
            flash_delay: 0,
            flash_item: 0,
            edit_text_modified: false,
            draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
            draw_procs_done: false,
            rendered_pixels_final: false,
            filter_presentation_epoch: None,
            filter_proc: 0,
            game_managed: false,
            last_filter_event: None,
            popup_draws: Vec::new(),
            active_popup: None,
            active_button: None,
            active_user_item: None,
        });

        assert!(runner.fire_dialog_draw_procs());
        let (_steps, running) = runner.run_steps(48, None);

        assert!(running);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.bus.read_word(seen_item_addr) as i16, item_no);
        assert_eq!(runner.bus.read_long(seen_dialog_addr), dialog_ptr);
    }

    #[test]
    fn pending_delay_ticks_fire_vbl_tasks_in_headless_mode() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let interrupted_pc = 0x0001_0000;
        let interrupted_sp = 0x007F_FFC0;
        let task_ptr = 0x0020_2000;

        runner.bus.write_word(interrupted_pc, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
        runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
        runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
        runner.bus.write_long(0x016A, 0);
        runner.dispatcher.pending_delay_ticks = 1;

        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, 0x0004_1234);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.bus.write_word(task_ptr + 12, 0);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });

        let (steps, running) = runner.run_steps(1, None);

        assert!(running);
        assert_eq!(steps, 1);
        assert_eq!(runner.bus.read_long(0x016A), 1);
        assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 1);
        assert!(matches!(
            runner.active_interrupt_callback,
            Some(ActiveInterruptCallback {
                source: ActiveInterruptCallbackSource::Vbl,
                ..
            })
        ));
    }

    /// `set_mouse_position` updates both the dispatcher's tracked
    /// position and the six low-memory mouse globals (MTemp $0828,
    /// RawMouse $082C, Mouse $0830) so guest code that polls them
    /// directly sees the new coordinates without waiting for a click.
    /// Inside Macintosh Volume II, II-371.
    #[test]
    fn set_mouse_position_updates_dispatcher_and_low_mem_globals() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.set_mouse_position(123, 456);

        assert_eq!(runner.dispatcher.input_state.mouse_pos, (123, 456));
        for off in [0x0828u32, 0x082C, 0x0830] {
            assert_eq!(runner.bus.read_word(off), 123u16, "v at ${:04X}", off);
            assert_eq!(runner.bus.read_word(off + 2), 456u16, "h at ${:04X}", off);
        }
    }

    #[test]
    fn constructed_trap_tables_survive_companion_installation_and_observe_native_stores() {
        use crate::trap::dispatch::{OS_TRAP_TABLE_BASE, TOOLBOX_TRAP_TABLE_BASE};

        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let mut cells = Vec::new();
        for (base, count) in [(OS_TRAP_TABLE_BASE, 256), (TOOLBOX_TRAP_TABLE_BASE, 1024)] {
            for slot in 0..count {
                let cell = base + slot * 4;
                let handler = runner.bus.read_long(cell);
                assert_ne!(handler, 0);
                let instruction = runner.bus.read_word(handler);
                assert!(!runner.bus.try_write_word(handler, instruction ^ 0xFFFF));
                assert_eq!(runner.bus.read_word(handler), instruction);
                cells.push((cell, handler));
            }
        }
        let vectors = runner.dispatcher.trap_exception_vector_defaults.unwrap();
        let entry = TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4; // TickCount
        let default = runner.bus.read_long(entry);
        let patch = 0x0010_1000;
        let program = 0x0010_0000;
        runner.bus.write_word(patch, 0x4E75); // RTS
        runner.bus.write_word(program, 0xA975); // TickCount
        runner.bus.write_word(program + 2, 0x4E71); // NOP

        // The companion joins an existing process; it must not replace its
        // guest-written table cells or exception vectors with launch defaults.
        runner.bus.write_long(entry, patch);
        runner.bus.write_long(0x2C, patch);
        let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
        runner.init_ppc_companion(app.ppc.take().unwrap());
        assert_eq!(
            runner.dispatcher.trap_table_profile,
            Some(TrapTableProfile::M68k68040)
        );
        {
            let companion = runner
                .native
                .adapter_mut(NativeEngineRole::Companion)
                .unwrap();
            for (cell, handler) in cells {
                assert_eq!(
                    companion.memory.read_u32_be(cell),
                    Some(if cell == entry { patch } else { handler })
                );
            }
            assert_eq!(companion.memory.read_u32_be(0x28), Some(vectors[0]));
            assert_eq!(companion.memory.read_u32_be(0x2C), Some(patch));
            companion.memory.write_u32_be(entry, default).unwrap();
        }
        assert_eq!(
            runner.dispatcher.trap_table_address(&runner.bus, 0xA975),
            Some(default)
        );
        runner
            .native
            .adapter_mut(NativeEngineRole::Companion)
            .unwrap()
            .memory
            .write_u32_be(entry, patch)
            .unwrap();
        runner.m68k.cpu.write_reg(Register::D0, 0xA975);
        runner
            .dispatcher
            .dispatch(0xA746, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        assert_eq!(runner.m68k.cpu.read_reg(Register::A0), patch);
        runner.m68k.cpu.write_reg(Register::PC, program);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        assert_eq!(runner.run_steps(1, None), (1, true));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), patch);
        assert_eq!(runner.run_steps(1, None), (1, true));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), program + 2);
    }

    /// A-line execution reaches the Trap Dispatcher through vector 10 at
    /// `$28`; line-F reaches the Line 1111 emulator through vector 11 at
    /// `$2C`. Both cells are writable system globals, so replacing either one
    /// must expose the processor's format-0 frame to guest code rather than
    /// silently entering HLE. Inside Macintosh Volume I (1985), p. I-89;
    /// Inside Macintosh Volume III (1985), p. III-17.
    #[test]
    fn guest_line_vectors_receive_architectural_frames_and_restore_defaults() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let defaults = runner.dispatcher.trap_exception_vector_defaults.unwrap();
        let original_sp = 0x007F_FFC0;

        // MOVE.L #marker,D6; ADDQ.L #2,2(SP); RTE. The handler advances the
        // faulting PC in the format-0 frame before returning.
        let aline_handler = 0x0010_1000;
        runner.bus.write_word(aline_handler, 0x2C3C);
        runner.bus.write_long(aline_handler + 2, 0xA10E_0010);
        runner.bus.write_word(aline_handler + 6, 0x54AF);
        runner.bus.write_word(aline_handler + 8, 0x0002);
        runner.bus.write_word(aline_handler + 10, 0x4E73);
        runner.bus.write_long(0x28, aline_handler);

        let aline_program = 0x0010_0000;
        runner.bus.write_word(aline_program, 0xA975); // TickCount
        runner.bus.write_word(aline_program + 2, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, aline_program);
        runner.m68k.cpu.write_reg(Register::A7, original_sp);

        assert_eq!(runner.run_steps(1, None), (1, true));
        let aline_frame = original_sp - 8;
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), aline_handler);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), aline_frame);
        assert_eq!(runner.bus.read_long(aline_frame + 2), aline_program);
        assert_eq!(runner.bus.read_word(aline_frame + 6), 0x0028);
        assert_eq!(runner.run_steps(3, None), (3, true));
        assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0xA10E_0010);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), original_sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), aline_program + 2);

        // Restoring the generated vector re-enables the ordinary HLE path.
        runner.bus.write_long(0x28, defaults[0]);
        runner.m68k.cpu.write_reg(Register::PC, aline_program);
        let trap_count = runner.dispatcher.trap_count;
        assert_eq!(runner.run_steps(1, None), (1, true));
        assert_eq!(runner.dispatcher.trap_count, trap_count + 1);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), aline_program + 2);

        // Repeat the same architectural proof for an unsupported F-line word.
        let fline_handler = 0x0010_1100;
        runner.bus.write_word(fline_handler, 0x2A3C); // MOVE.L #marker,D5
        runner.bus.write_long(fline_handler + 2, 0xF11E_0011);
        runner.bus.write_word(fline_handler + 6, 0x54AF);
        runner.bus.write_word(fline_handler + 8, 0x0002);
        runner.bus.write_word(fline_handler + 10, 0x4E73);
        runner.bus.write_long(0x2C, fline_handler);

        let fline_program = 0x0010_0200;
        runner.bus.write_word(fline_program, 0xF000);
        runner.bus.write_word(fline_program + 2, 0x4E71);
        runner.m68k.cpu.write_reg(Register::PC, fline_program);
        runner.m68k.cpu.write_reg(Register::A7, original_sp);

        assert_eq!(runner.run_steps(1, None), (1, true));
        let fline_frame = original_sp - 8;
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), fline_handler);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), fline_frame);
        assert_eq!(runner.bus.read_long(fline_frame + 2), fline_program);
        assert_eq!(runner.bus.read_word(fline_frame + 6), 0x002C);
        assert_eq!(runner.run_steps(3, None), (3, true));
        assert_eq!(runner.m68k.cpu.read_reg(Register::D5), 0xF11E_0011);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), original_sp);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), fline_program + 2);
        runner.bus.write_long(0x2C, defaults[1]);
    }

    /// FNOP is a valid 68040 coprocessor instruction, not a Line 1111
    /// exception. Keeping a replacement vector 11 installed while it executes
    /// proves opcode classification happens before exception delegation.
    #[test]
    fn valid_68040_fpu_opcode_does_not_enter_guest_vector_11() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let fline_handler = 0x0010_1100;
        runner.bus.write_word(fline_handler, 0x2E3C); // MOVE.L #sentinel,D7
        runner.bus.write_long(fline_handler + 2, 0xBADF_11E0);
        runner.bus.write_word(fline_handler + 6, 0x4E73);
        runner.bus.write_long(0x2C, fline_handler);

        let program = 0x0010_0000;
        runner.bus.write_word(program, 0xF280); // FNOP
        runner.bus.write_word(program + 2, 0x0000);
        runner.bus.write_word(program + 4, 0x4E71); // NOP
        runner.m68k.cpu.write_reg(Register::PC, program);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.m68k.cpu.write_reg(Register::D7, 0x1357_2468);

        assert_eq!(runner.run_steps(1, None), (1, true));
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), program + 4);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x007F_FFC0);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D7), 0x1357_2468);
    }

    /// Running a `DIVU.W D0,D1` with `D0 = 0` must not halt the
    /// runner. The `load_app_generic` loader installs an RTE stub at
    /// `$00FE` and points vector 5 (`$14`) at it; the m68k crate's
    /// zero-divide trap stacks the *next* PC and jumps to that vector,
    /// so RTE-ing returns past the DIVU and execution continues.
    /// Inside Macintosh Volume I, I-103 (Exception Vector Table);
    /// M68000PRM ("If the source operand is zero, the result of the
    /// operation is unpredictable").
    #[test]
    fn zero_divide_rte_handler_resumes_after_divu_by_zero() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        // Mirror what load_app_generic installs: RTE stub + vector.
        runner.bus.write_word(0x00FE, 0x4E73); // RTE
        runner.bus.write_long(0x0014, 0x0000_00FE);

        let prog = 0x0010_0000u32;
        runner.bus.write_word(prog, 0x82C0); // DIVU.W D0, D1
        runner.bus.write_word(prog + 2, 0x4E71); // NOP
        runner.bus.write_word(prog + 4, 0x4E71); // NOP

        runner.m68k.cpu.write_reg(Register::PC, prog);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.m68k.cpu.write_reg(Register::D0, 0);
        runner.m68k.cpu.write_reg(Register::D1, 100);

        // 1 step: DIVU.W traps, vectors to $00FE.
        // 2nd step: RTE at $00FE pops SR/PC, returns past DIVU.
        // 3rd step: NOP at prog+2.
        let (steps, running) = runner.run_steps(3, None);

        assert!(running, "runner must not halt on zero-divide");
        assert_eq!(steps, 3);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            prog + 4,
            "PC must advance past the DIVU+NOP without re-entering the trap"
        );
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::D1),
            100,
            "DIVU by zero must leave the destination register unchanged"
        );
    }

    /// CHK exception (vector 6) shares the same `$00FE` RTE stub as
    /// the zero-divide handler. A `CHK.W #5, D0` with `D0 = 100`
    /// exceeds the bound and triggers the trap; on a real Mac the
    /// handler calls SysError, on Systemless we silently RTE so D0 is
    /// preserved and the next instruction runs.
    /// Inside Macintosh Volume I, I-103.
    #[test]
    fn chk_rte_handler_resumes_after_bounds_violation() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.bus.write_word(0x00FE, 0x4E73); // RTE
        runner.bus.write_long(0x0018, 0x0000_00FE); // CHK vector

        let prog = 0x0010_0000u32;
        runner.bus.write_word(prog, 0x41BC); // CHK.W #imm, D0
        runner.bus.write_word(prog + 2, 0x0005); // imm = 5
        runner.bus.write_word(prog + 4, 0x4E71); // NOP

        runner.m68k.cpu.write_reg(Register::PC, prog);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.m68k.cpu.write_reg(Register::D0, 100);

        // 1 step: CHK fires (100 > 5), vectors to $00FE.
        // 2nd step: RTE pops SR/PC, returns past CHK.
        // 3rd step: NOP executes.
        let (steps, running) = runner.run_steps(3, None);

        assert!(running, "runner must not halt on CHK bounds violation");
        assert_eq!(steps, 3);
        assert_eq!(
            runner.m68k.cpu.read_reg(Register::PC),
            prog + 6,
            "PC must advance past CHK (4 bytes) + NOP (2 bytes)"
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 100);
    }

    /// TRAPV (vector 7) shares the `$00FE` RTE stub. Pre-set the V
    /// flag in CCR via the m68k API and execute TRAPV; the trap fires
    /// because V is set, vectors to the RTE stub, and resumes at the
    /// next instruction. Inside Macintosh Volume I, I-103.
    #[test]
    fn trapv_rte_handler_resumes_when_v_flag_is_set() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.bus.write_word(0x00FE, 0x4E73); // RTE
        runner.bus.write_long(0x001C, 0x0000_00FE); // TRAPV vector

        let prog = 0x0010_0000u32;
        runner.bus.write_word(prog, 0x4E76); // TRAPV
        runner.bus.write_word(prog + 2, 0x4E71); // NOP

        runner.m68k.cpu.write_reg(Register::PC, prog);
        runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
        runner.m68k.cpu.core.set_ccr(0x02); // V flag set

        // 1: TRAPV traps; 2: RTE; 3: NOP.
        let (steps, running) = runner.run_steps(3, None);

        assert!(running, "runner must not halt on TRAPV");
        assert_eq!(steps, 3);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), prog + 4);
    }

    /// `set_mouse_position` does NOT modify MBState ($0172) — it's a
    /// move-without-button-change, so the button-state byte should
    /// retain its prior value. The default at runner construction is
    /// 0x80 (button up).
    #[test]
    fn set_mouse_position_leaves_mb_state_untouched() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.bus.write_byte(0x0172, 0x80);
        runner.set_mouse_position(50, 60);
        assert_eq!(runner.bus.read_byte(0x0172), 0x80);

        runner.bus.write_byte(0x0172, 0x00);
        runner.set_mouse_position(70, 80);
        assert_eq!(runner.bus.read_byte(0x0172), 0x00);
    }

    /// `push_mouse_down` must update MBState ($0172) to 0x00 (button
    /// pressed) immediately AND sync the position globals so guest
    /// code that polls these bytes directly sees the click without
    /// waiting for the next tick advance.
    /// Inside Macintosh Volume I, I-258 (MTemp/RawMouse/Mouse);
    /// Inside Macintosh Volume II, II-371 (MBState polling).
    #[test]
    fn push_mouse_down_writes_mb_state_pressed_and_position() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.bus.write_byte(0x0172, 0x80); // start "button up"

        runner.push_mouse_down(123, 456);

        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x00,
            "MBState must be 0x00 (pressed) immediately after push_mouse_down"
        );
        // All three position globals must mirror the click site so
        // games that poll them directly (Mouse $0830 etc.) see the
        // correct location, not the prior cursor-park position.
        assert_eq!(runner.bus.read_word(0x0828), 123u16);
        assert_eq!(runner.bus.read_word(0x082A), 456u16);
        assert_eq!(runner.bus.read_word(0x082C), 123u16);
        assert_eq!(runner.bus.read_word(0x082E), 456u16);
        assert_eq!(runner.bus.read_word(0x0830), 123u16);
        assert_eq!(runner.bus.read_word(0x0832), 456u16);
    }

    #[test]
    fn pending_mouse_down_count_tracks_queued_clicks() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        assert_eq!(runner.pending_mouse_down_count(), 0);

        runner.push_mouse_down(10, 20);
        assert_eq!(runner.pending_mouse_down_count(), 1);

        runner.process_context.shared_event_queue().pop_front();
        assert_eq!(runner.pending_mouse_down_count(), 0);
    }

    /// `push_mouse_up` must update MBState ($0172) to 0x80 (button
    /// released) immediately. On real hardware the ADB polls at ~200 Hz
    /// so the latency between physical release and MBState=0x80 is a
    /// few ms; deferring to advance_guest_tick (~16 ms) makes
    /// frame-rate-dependent games read the wrong button state for too
    /// many loop iterations after click-up. This test pins the
    /// immediate-sync contract documented at runner.rs `push_mouse_up`.
    #[test]
    fn push_mouse_up_writes_mb_state_released_immediately() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.push_mouse_down(10, 20);
        assert_eq!(runner.bus.read_byte(0x0172), 0x00);

        runner.push_mouse_up(10, 20);
        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x80,
            "MBState must flip back to 0x80 (released) immediately on push_mouse_up — \
             not deferred to the next tick"
        );
    }

    /// Regression: advance_guest_tick must NOT keep MBState at 0x00
    /// when both mouseDown and a paired mouseUp are queued and
    /// unconsumed. Polling-only games (Bonkheads-Deluxe class) never
    /// call GetNextEvent — the queue accumulates indefinitely.
    /// Pre-fix, the "any pending mouseDown → pressed" override left
    /// $0172 stuck at 0x00 forever, so Button() always returned TRUE
    /// and click detection broke silently. The fix counts unmatched
    /// mouseDowns (mouseDown count − mouseUp count) and only treats
    /// those as "still pressed".
    #[test]
    fn mb_state_releases_when_paired_mouseup_queued_but_unconsumed() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.push_mouse_down(10, 20);
        runner.push_mouse_up(10, 20);
        // Both events still queued (no GetNextEvent has run). Drive the
        // tick boundary that owns the MBState resync.
        runner.advance_guest_tick();

        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x80,
            "advance_guest_tick must release MBState to 0x80 once a \
             paired mouseUp is queued behind the mouseDown — even when \
             nothing has drained the event queue"
        );
        // Sanity-check the events ARE still in the queue (this test is
        // about MBState despite the unconsumed events, not about queue
        // state). Read it from the canonical process_context queue.
        assert!(
            runner
                .process_context
                .event_queue()
                .iter()
                .any(|e| e.what == 1),
            "mouseDown event must remain in the queue (would be drained by GetNextEvent)"
        );
        assert!(
            runner
                .process_context
                .event_queue()
                .iter()
                .any(|e| e.what == 2),
            "mouseUp event must remain in the queue"
        );
    }

    /// Mirror of `mb_state_releases_when_paired_mouseup_queued_but_unconsumed`:
    /// a SOLO mouseDown queued without a paired mouseUp must still pin
    /// MBState to 0x00 across tick boundaries. This preserves the
    /// original contract — code that hasn't yet started polling when
    /// the click was injected gets at least one TRUE pulse — without
    /// regressing into the stuck-pressed bug.
    #[test]
    fn mb_state_stays_pressed_with_solo_pending_mousedown() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

        runner.push_mouse_down(10, 20);
        runner.advance_guest_tick();
        assert_eq!(
            runner.bus.read_byte(0x0172),
            0x00,
            "MBState must stay pressed across a tick advance while only \
             a mouseDown is queued (no paired mouseUp yet)"
        );
    }

    #[test]
    fn menu_bar_policy_defaults_to_guest_control_and_supports_explicit_kiosk_modes() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
        assert!(
            runner.menu_bar_visible(),
            "library runners should permit guest menu chrome by default"
        );

        runner.set_menu_bar_policy(MenuBarPolicy::InitialKiosk);
        assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::InitialKiosk);
        assert!(!runner.menu_bar_visible());

        runner.set_menu_bar_visible(false);
        assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::ForceHidden);
        assert!(!runner.menu_bar_visible());

        runner.set_menu_bar_visible(true);
        assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
        assert!(runner.menu_bar_visible());
    }

    #[test]
    fn initial_kiosk_releases_after_guest_hides_and_reveals_menu_bar() {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_menu_bar_policy(MenuBarPolicy::InitialKiosk);
        runner.dispatcher.front_window = 1;

        runner
            .bus
            .write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        runner.force_advance_guest_tick();
        assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::InitialKiosk);
        assert!(!runner.menu_bar_visible());

        runner
            .bus
            .write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        runner.force_advance_guest_tick();
        assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
        assert!(runner.menu_bar_visible());
    }

    #[test]
    fn disassemble_at_decodes_known_opcodes_with_correct_advance() {
        // Pins the FixtureRunner::disassemble_at public-API helper.
        // This is the library-level entry point for pixel-divergence
        // and trap-misroute investigations: pair with
        // SYSTEMLESS_TRACE_FB_WRITE_RANGE to see what code lives at a
        // suspect PC.
        //
        // Seed three known instructions in guest RAM, disassemble,
        // and verify:
        //   1. each entry's PC advances by the previous size
        //   2. the mnemonic for $4E71 is "NOP" (well-known fixed
        //      instruction; no operand words to consume)
        //   3. an A-line trap word ($A8EC = CopyBits) comes back as
        //      "DC.W $A8EC" — the m68k crate's convention for opcodes
        //      it doesn't have a regular decoder for
        //   4. the size returned is at least 2 and at most 10 (the
        //      clamp guard that prevents a malformed opcode from
        //      consuming wrap-around amounts)
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let pc = 0x10000u32;
        // $4E71 NOP
        runner.bus.write_word(pc, 0x4E71);
        // $A8EC (CopyBits trap-line word)
        runner.bus.write_word(pc + 2, 0xA8EC);
        // $4E71 NOP again
        runner.bus.write_word(pc + 4, 0x4E71);
        let out = runner.disassemble_at(pc, 3);
        assert_eq!(
            out.len(),
            3,
            "disassemble_at must return exactly count entries"
        );
        assert_eq!(
            out[0].0, pc,
            "first entry's PC must equal the requested start"
        );
        assert!(
            out[0].1.contains("NOP"),
            "$4E71 must disassemble to NOP, got: {}",
            out[0].1
        );
        assert!(
            out[0].2 >= 2 && out[0].2 <= 10,
            "instruction size must be in clamp range [2, 10], got {}",
            out[0].2
        );
        assert_eq!(
            out[1].0,
            pc + out[0].2,
            "second entry's PC must equal first PC + first size"
        );
        assert!(
            out[1].1.contains("$A8EC"),
            "A-line trap $A8EC must surface in mnemonic (DC.W form), got: {}",
            out[1].1
        );
        assert!(
            out[2].1.contains("NOP"),
            "third entry must be the second NOP we seeded"
        );
    }

    #[test]
    fn disassemble_at_uses_the_configured_ram_boundary() {
        let mut runner = FixtureRunner::new(16 * 1024 * 1024, FixtureRunnerConfig::default());
        let pc = 12 * 1024 * 1024;
        runner.bus.write_word(pc, 0x4E71);

        let out = runner.disassemble_at(pc, 1);
        assert_eq!(out.len(), 1);
        assert!(
            out[0].1.contains("NOP"),
            "mapped RAM above 8 MiB must not be reported as unmapped"
        );
    }
