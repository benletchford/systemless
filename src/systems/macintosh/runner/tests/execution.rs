use super::*;
use crate::callback_manager::CallbackTaskArchitecture;
use crate::execution_kernel::{ExecutionTaskId, ExecutionTaskState};
use crate::guest_call::{M68kResultTarget, M68kResume, PowerPcReturnState};
use crate::memory::globals::addr;
use crate::mixed_mode::special_case;
use ppc::PpcRunResult;
use std::collections::HashMap;

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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .complete_m68k(0x2002, 0x3000));
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
    runner
        .dispatcher
        .set_loaded_resources_for_test(LoadedResources {
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
fn relaunch_with_pending_execution_preserves_the_existing_engine() {
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
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.init_app(&app)));
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
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.init_app(&app)));
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
        assert!(ppc_app
            .toolbox_startup
            .execution
            .calls()
            .begin_m68k_to_powerpc(
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
        ppc_app
            .toolbox_startup
            .execution
            .calls()
            .activate_powerpc_from_m68k(&mut ppc_app.cpu, RETURN_PC)
            .unwrap();
        ppc_app.cpu.pc = RETURN_PC;
        ppc_app.cpu.gpr[3] = 0x1234_5678;
        assert!(ppc_app
            .toolbox_startup
            .execution
            .calls()
            .complete_powerpc_for_m68k(&mut ppc_app.cpu));
        let (task, call_id) = ppc_app
            .toolbox_startup
            .execution
            .calls()
            .pending_m68k_resume_owner()
            .unwrap();
        runner.m68k.cpu.core.set_d(1, 0xabcd_0000);
        runner.m68k.cpu.core.set_d(7, 0xcafe_babe);
        runner.m68k.cpu.core.set_ccr(0x11);
        assert!(ppc_app
            .guest_calls()
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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_m68k_to_powerpc(
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
    ppc_app
        .toolbox_startup
        .execution
        .calls()
        .activate_powerpc_from_m68k(&mut ppc_app.cpu, RETURN_PC)
        .unwrap();
    ppc_app.cpu.pc = RETURN_PC;
    ppc_app.cpu.gpr[3] = RESULT_VALUE;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .complete_powerpc_for_m68k(&mut ppc_app.cpu));

    // Model the caller parked by a nested native-to-68K transition. The
    // failed write must not consume either this context or its completion.
    let (task, call_id) = ppc_app
        .guest_calls()
        .pending_m68k_resume_owner()
        .expect("completed continuation owner");
    runner.m68k.cpu.core.set_d(0, PARKED_D0);
    assert!(ppc_app
        .guest_calls()
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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_some());

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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_some());

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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_some());

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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_none());
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
    let posted = runner.process_context.event_queue().get(0).unwrap();
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
    sound
        .manager
        .replace_pending_process_doublebacks(vec![PpcSoundDoubleBackRecord {
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
    sound
        .manager
        .replace_double_buffer_playbacks(vec![PpcSoundDoubleBufferPlaybackRecord {
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
fn ppc_sound_completion_preserves_callback_rnd_seed_update() {
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
