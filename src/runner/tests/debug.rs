use super::*;
#[cfg(feature = "debug")]
use ppc::PpcNativeReturnGpr3;

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
#[cfg(feature = "debug")]
fn debugger_controls_a_parked_native_to_68k_callback_and_preserves_its_return() {
    use crate::debug::{
        handle_debug_request, BreakpointSpec, ContextSelector, DebugExecutionState, DebugReply,
        DebugRequest, M68K_CONTEXT,
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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
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
    .unwrap() else {
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
    .unwrap() else {
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
    let DebugReply::Notifications(notifications) =
        handle_debug_request(&mut runner, DebugRequest::Notifications { after: 0 }).unwrap()
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
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
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
