use super::*;
use crate::callback_manager::CallbackTaskArchitecture;
use crate::cpu::Register;
use crate::runner::{ActiveInterruptCallbackSource, DIALOG_CALLBACK_SCRATCH_SIZE};
use crate::sound::{PendingSoundCallback, SndCommand};
use crate::trap::dispatch::{DialogTrackingState, QueuedEvent};
use std::collections::VecDeque;

pub(crate) fn dialog_tracking_for_test(filter_proc: u32, item_hit_ptr: u32) -> DialogTrackingState {
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

#[test]
fn dialog_filter_accepts_a_stack_result_reservation_prologue() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let callback = runner.bus.alloc(4);
    runner.bus.write_word(callback, 0x554F); // SUBQ.W #2,SP
    runner.bus.write_word(callback + 2, 0x206F); // MOVEA.L d16(SP),A0
    assert!(runner.looks_like_dialog_proc_entry(callback));

    runner.bus.write_word(callback, 0x0020); // Rect data, not code
    assert!(!runner.looks_like_dialog_proc_entry(callback));
}

#[test]
fn dialog_callback_scratch_preserves_materialized_toolbox_trap_table() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let table_start = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE;
    let table_end = table_start + u32::from(crate::trap::dispatch::TOOLBOX_TRAP_TABLE_SLOTS) * 4;
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
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));

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
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
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
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
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

    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
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
