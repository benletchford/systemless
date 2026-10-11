use super::*;
use crate::execution_native::NativeEngineRole;
use crate::loader::ppc::PpcSoundState;
use crate::process_context::PendingFileCompletion;
use crate::trap::dispatch::TrapTableProfile;

#[test]
fn notification_runner_delivers_direct_m68k_response_and_restores_native_context() {
    use crate::guest_procedure::*;
    use crate::mixed_mode::proc_info;
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&halted_ppc_app_with_sound(PpcSoundState::default()));
    let request = PPC_DATA_BASE + 0x6000;
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let callback = PPC_HEAP_BASE + 0x2000;
    let output = request + 40;
    let foreground;
    {
        let native = runner.native.application_mut().unwrap();
        native.memory.add_region(request, vec![0; 128]);
        native.memory.add_region(descriptor, vec![0; 128]);
        let mut code = Vec::new();
        for word in [0x202fu16, 4, 0x23c0, (output >> 16) as u16,
            output as u16, 0x4e74, 4] { code.extend_from_slice(&word.to_be_bytes()); }
        native.memory.add_region(callback, code);
        native.set_heap_cursor(native.heap_cursor().max(callback + 128));
        native.memory.write_u16_be(descriptor, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP).unwrap();
        native.memory.write_u8(descriptor + 2, ROUTINE_DESCRIPTOR_VERSION).unwrap();
        let record = descriptor + ROUTINE_DESCRIPTOR_HEADER_SIZE;
        native.memory.write_u32_be(record, proc_info::PASCAL_STACK_BASED
            | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE)).unwrap();
        native.memory.write_u8(record + ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA).unwrap();
        native.memory.write_u32_be(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, callback).unwrap();
        native.memory.write_u16_be(request + 4, 8).unwrap();
        native.memory.write_u32_be(request + 24, request + 64).unwrap();
        native.memory.write_u32_be(request + 28, descriptor).unwrap();
        native.memory.write_u32_be(request + 96, 0x4800_0000).unwrap();
        native.cpu.pc = request + 96;
        native.toolbox_startup.notification_requests.push(request);
        foreground = native.cpu.capture_execution_context();
    }
    let notice = runner.notification_snapshot().pop().unwrap();
    assert!(runner.complete_notification_delivery(&notice));
    assert!(!runner.complete_notification_delivery(&notice));
    assert!(runner.native.application_mut().unwrap().parked_interrupt_callback.is_some());
    let mut completed = false;
    for _ in 0..100 {
        assert!(runner.run_realtime_steps_with_audio(32, 0).1);
        let native = runner.native.application_mut().unwrap();
        if native.parked_interrupt_callback.is_none() {
            assert_eq!(native.memory.read_u32_be(output), Some(request));
            assert_eq!(native.cpu.capture_execution_context().architectural(), foreground.architectural());
            completed = true;
            break;
        }
    }
    assert!(completed, "direct Mixed Mode response did not return to native foreground");
}

#[test]
fn notification_native_runner_finishes_response_before_foreground_instructions() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&halted_ppc_app_with_sound(PpcSoundState::default()));
    let request = PPC_DATA_BASE + 0x6000;
    let callback = request + 128;
    let descriptor = callback + 128;
    let foreground_code = descriptor + 128;
    let foreground;
    {
        let native = runner.native.application_mut().unwrap();
        native.memory.add_region(request, vec![0; 512]);
        native.memory.write_u16_be(request + 4, 8).unwrap();
        native.memory.write_u32_be(request + 24, request + 64).unwrap();
        native.memory.write_u32_be(request + 28, descriptor).unwrap();
        native.memory.write_u32_be(request + 32, u32::MAX).unwrap();
        native.memory.write_u32_be(descriptor, callback).unwrap();
        native.memory.write_u32_be(descriptor + 4, native.rtoc).unwrap();
        for (index, instruction) in [0x3880_00c8u32, 0x3884_ffff, 0x2c04_0000,
            0x4082_fff8, 0x9083_0020, 0x4e80_0020].into_iter().enumerate() {
            native.memory.write_u32_be(callback + index as u32 * 4, instruction).unwrap();
        }
        // Foreground code changes a separate marker as soon as it executes.
        for (index, instruction) in [0x38a0_0001u32, 0x90a6_0000, 0x4800_0000]
            .into_iter().enumerate() {
            native.memory.write_u32_be(foreground_code + index as u32 * 4, instruction).unwrap();
        }
        native.cpu.pc = foreground_code;
        native.cpu.gpr[6] = request + 40;
        native.toolbox_startup.notification_requests.push(request);
        foreground = native.cpu.capture_execution_context();
    }
    let notice = runner.notification_snapshot().pop().unwrap();
    assert!(runner.complete_notification_delivery(&notice));
    assert!(!runner.complete_notification_delivery(&notice));
    assert!(runner.native.application_mut().unwrap().notification_response_context.is_some());
    let mut completed = false;
    for _ in 0..100 {
        let (_, running) = runner.run_realtime_steps_with_audio(32, 0);
        assert!(running);
        let native = runner.native.application_mut().unwrap();
        assert_eq!(native.memory.read_u32_be(request + 40), Some(0));
        assert_eq!(native.cpu.capture_execution_context().architectural(), foreground.architectural());
        if native.notification_response_context.is_none() {
            assert_eq!(native.memory.read_u32_be(request + 32), Some(0));
            completed = true;
            break;
        }
    }
    assert!(completed, "bounded notification response did not finish");
    assert!(runner.run_realtime_steps_with_audio(32, 0).1);
    assert_eq!(runner.native.application_mut().unwrap().memory.read_u32_be(request + 40), Some(1));
}

#[test]
fn notification_native_runner_completes_null_and_auto_remove_without_cpu_changes() {
    for response in [0, u32::MAX] {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        runner.init_app(&app);
        let request = PPC_DATA_BASE + 0x6000;
        let foreground;
        {
            let native = runner.native.application_mut().unwrap();
            native.memory.add_region(request, vec![0; 128]);
            native.memory.write_u16_be(request + 4, 8).unwrap();
            native.memory.write_u32_be(request + 24, request + 64).unwrap();
            native.memory.write_u32_be(request + 28, response).unwrap();
            native.toolbox_startup.notification_requests.push(request);
            foreground = native.cpu.capture_execution_context();
        }
        let notice = runner.notification_snapshot().pop().unwrap();
        let mut stale = notice.clone();
        stale.instance_id += 1;
        assert!(!runner.complete_notification_delivery(&stale));
        assert!(!runner.notification_snapshot().pop().unwrap().response_started);
        assert!(runner.complete_notification_delivery(&notice));
        assert!(!runner.complete_notification_delivery(&notice));
        if response == 0 {
            assert!(runner.notification_snapshot().pop().unwrap().response_started);
        } else { assert!(runner.notification_snapshot().is_empty()); }
        let native = runner.native.application_mut().unwrap();
        assert_eq!(native.cpu.capture_execution_context().architectural(), foreground.architectural());
        assert_eq!(native.memory.read_u32_be(request), Some(0));
    }
}

#[test]
fn notification_delivery_restores_full_guest_registers_and_condition_codes() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let resume_pc = 0x10000;
    let resume_sp = 0x7fffc0;
    for offset in (0..80).step_by(2) { runner.bus.write_word(resume_pc + offset, 0x4e71); }
    let request = runner.bus.alloc(36);
    let text = runner.bus.alloc(8);
    let callback = runner.bus.alloc(24);
    runner.bus.write_word(request + 4, 8);
    runner.bus.write_long(request + 24, text);
    runner.bus.write_long(request + 28, callback);
    for (index, word) in [0x4e56, 0, 0x2e3c, 0x1122, 0x3344,
        0x287c, 0x5566, 0x7788, 0x7000, 0x4e5e, 0x4e74, 4].into_iter().enumerate() {
        runner.bus.write_word(callback + index as u32 * 2, word);
    }
    runner.m68k.cpu.write_reg(Register::PC, resume_pc);
    runner.m68k.cpu.write_reg(Register::A7, resume_sp);
    runner.m68k.cpu.write_reg(Register::A0, request);
    runner.dispatcher.dispatch_memory(false, 0x5e, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap().unwrap();
    runner.m68k.cpu.write_reg(Register::D7, 0xabcdef12);
    runner.m68k.cpu.write_reg(Register::A4, 0x12345678);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2015);
    let notice = runner.notification_snapshot().pop().unwrap();
    assert!(!notice.response_started);
    assert!(runner.complete_notification_delivery(&notice));
    assert!(runner.notification_snapshot().pop().unwrap().response_started);
    assert!(!runner.callback_suspends_guest_clock());
    assert!(!runner.complete_notification_delivery(&notice));
    let (_, running) = runner.run_steps(24, None);
    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), resume_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D7), 0xabcdef12);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A4), 0x12345678);
    assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2015);
    assert!(!runner.complete_notification_delivery(&notice));
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
