use super::*;
use crate::cpu::Register;

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
