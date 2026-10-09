use super::*;
use crate::callback_manager::CallbackTaskArchitecture;
use crate::cpu::Register;
use crate::runner::{ActiveInterruptCallback, ActiveInterruptCallbackSource};
use crate::trap::dispatch::{PendingWaitNextEventReturn, VblTask};

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

    assert_eq!(runner.dispatcher.input_state.mouse_position(), (123, 456));
    for off in [0x0828u32, 0x082C, 0x0830] {
        assert_eq!(runner.bus.read_word(off), 123u16, "v at ${:04X}", off);
        assert_eq!(runner.bus.read_word(off + 2), 456u16, "h at ${:04X}", off);
    }
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
fn foreground_request_wakes_sleeping_event_loop_with_mask_and_interrupt_safety() {
    for masked in [false, true] {
        for interrupt in [false, true] {
            let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
            let event_ptr = 0x0020_0000;
            let result_ptr = 0x0020_0020;
            runner.dispatcher.set_sent_open_app_event_for_test(true);
            runner.dispatcher.application_size.with_mut(|size| {
                *size = Some(crate::loader::ApplicationSizeResource {
                    flags: 0x4800,
                    preferred_size: 0,
                    minimum_size: 0,
                })
            });
            runner.dispatcher.pending_wait_sleep_ticks = 3600;
            runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
                event_ptr,
                result_ptr,
                event_mask: if masked { 0x0008 } else { 0x8000 },
                mouse_rgn: 0,
                resume_pc: None,
                resume_sp: None,
            });
            if interrupt {
                runner.active_interrupt_callback = Some(ActiveInterruptCallback {
                    source: ActiveInterruptCallbackSource::Timer,
                    resume_pc: 0x10000,
                    resume_sp: 0x007F_FFC0,
                    d_regs: [0; 8],
                    a_regs: [0; 8],
                    sr: 0x2000,
                    ccr: 0,
                    restore_port: None,
                });
            }
            runner.request_foreground(false);
            if masked || interrupt {
                assert_eq!(runner.bus.read_word(event_ptr), 0);
                assert_eq!(runner.bus.read_word(result_ptr), 0);
                assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 3600);
                assert!(runner.dispatcher.pending_wait_next_event_return.is_some());
                runner.active_interrupt_callback = None;
                runner
                    .dispatcher
                    .pending_wait_next_event_return
                    .as_mut()
                    .unwrap()
                    .event_mask = 0x8000;
                runner.request_foreground(false);
            }
            assert_eq!(runner.bus.read_word(event_ptr), 15);
            assert_eq!(runner.bus.read_long(event_ptr + 2), 0x0100_0000);
            assert_eq!(runner.bus.read_word(result_ptr), 0xFFFF);
            assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
            assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
            assert!(
                runner
                    .process_context
                    .event_queue()
                    .with_ref(|queue| queue.activation.is_foreground()),
                "guest must handle suspend before its next yield"
            );
        }
    }
}
