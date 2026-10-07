use super::*;
use crate::callback_manager::CallbackTaskArchitecture;
use crate::cpu::Register;
use crate::runner::{
    default_realtime_instructions_per_tick, ActiveInterruptCallback, ActiveInterruptCallbackSource,
    DEFAULT_REALTIME_CPU_MHZ, DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND,
    DEFAULT_REALTIME_PPC_CPU_MHZ, DEFAULT_VBL_HZ,
};
use crate::trap::dispatch::{TimerTask, VblTask};

#[test]
fn deferred_task_runs_at_next_interrupt_with_task_in_a0_and_parameter_in_a1() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let task = runner.bus.alloc(24);
    let callback = 0x0004_1234;
    let parameter = 0x8765_4321;
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let marker = 0x0005_0000;
    runner.bus.write_word(callback, 0x23C8); // MOVE.L A0,marker
    runner.bus.write_long(callback + 2, marker);
    runner.bus.write_word(callback + 6, 0x23C9); // MOVE.L A1,marker+4
    runner.bus.write_long(callback + 8, marker + 4);
    runner.bus.write_word(callback + 12, 0x4E75); // RTS
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.bus.write_word(task + 4, 7);
    runner.bus.write_long(task + 8, callback);
    runner.bus.write_long(task + 12, parameter);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::A0, 0x1234_5678);
    runner
        .dispatcher
        .enqueue_deferred_task(&mut runner.bus, task);

    assert!(!runner.fire_deferred_task());
    runner.advance_guest_tick();

    let active = runner
        .active_interrupt_callback
        .expect("deferred callback should run");
    assert_eq!(active.source, ActiveInterruptCallbackSource::DeferredTask);
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);
    let trampoline = runner.deferred_task_trampoline;
    assert_eq!(runner.bus.read_word(trampoline), 0x207C);
    assert_eq!(runner.bus.read_long(trampoline + 2), task);
    assert_eq!(runner.bus.read_word(trampoline + 6), 0x227C);
    assert_eq!(runner.bus.read_long(trampoline + 8), parameter);
    assert_eq!(runner.bus.read_word(trampoline + 12), 0x4EB9);
    assert_eq!(runner.bus.read_long(trampoline + 14), callback);
    assert_eq!(runner.bus.read_word(trampoline + 18), 0x4E75);
    assert!(runner.dispatcher.deferred_tasks.is_empty());

    runner.run_steps(8, None);
    assert_eq!(runner.bus.read_long(marker), task);
    assert_eq!(runner.bus.read_long(marker + 4), parameter);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x1234_5678);
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
    // already-due second element runs in the same retrace, without charging
    // the newly rearmed first task another countdown.
    runner.bus.write_word(first_ptr + 10, 1);
    runner.active_interrupt_callback = None;
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    assert!(runner.fire_pending_vbl_task());

    assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), second_ptr);
    assert_eq!(runner.bus.read_word(first_ptr + 10), 1);
    assert!(!runner.dispatcher.vbl_tasks[0].pending);
    assert!(!runner.dispatcher.vbl_tasks[1].pending);
}

const VBL_BATCH_PC: u32 = 0x0002_0000;
const VBL_BATCH_SP: u32 = 0x007F_FFC0;
const VBL_BATCH_MARKER: u32 = 0x0005_0000;
const VBL_BATCH_ORDER: u32 = 0x0005_0020;
const VBL_BATCH_FOREGROUND: u32 = 0x0005_0024;

fn vbl_batch_test_runner(intervals: &[u16]) -> FixtureRunner {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    // Record the order seen by the first resumed foreground instruction, then
    // spin without overwriting that evidence if a callback runs too late.
    runner.bus.write_word(VBL_BATCH_PC, 0x23F9); // MOVE.L order,foreground
    runner.bus.write_long(VBL_BATCH_PC + 2, VBL_BATCH_ORDER);
    runner.bus.write_long(VBL_BATCH_PC + 6, VBL_BATCH_FOREGROUND);
    runner.bus.write_word(VBL_BATCH_PC + 10, 0x60FE); // BRA.S to self
    runner.m68k.cpu.write_reg(Register::PC, VBL_BATCH_PC);
    runner.m68k.cpu.write_reg(Register::A7, VBL_BATCH_SP);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    for (index, interval) in intervals.iter().copied().enumerate() {
        let index = index as u32;
        let task_ptr = 0x0020_2000 + index * 0x20;
        let callback = 0x0004_1000 + index * 0x40;
        let words = [
            0x52B9,
            (VBL_BATCH_MARKER >> 16) as u16,
            (VBL_BATCH_MARKER + index * 4) as u16, // ADDQ.L #1,calls[index]
            0x2039,
            (VBL_BATCH_ORDER >> 16) as u16,
            VBL_BATCH_ORDER as u16,
            0xE988, // LSL.L #4,D0
            0x0000,
            (index + 1) as u16, // ORI.B #index+1,D0
            0x23C0,
            (VBL_BATCH_ORDER >> 16) as u16,
            VBL_BATCH_ORDER as u16,
            0x317C,
            interval,
            10, // MOVE.W #interval,10(A0)
            0x4E75,
        ];
        for (offset, word) in words.into_iter().enumerate() {
            runner.bus.write_word(callback + offset as u32 * 2, word);
        }
        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, callback);
        runner.bus.write_word(task_ptr + 10, interval);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });
    }
    runner
}

fn run_vbl_batch_retrace(runner: &mut FixtureRunner, order: u32) {
    runner.bus.write_long(VBL_BATCH_ORDER, 0);
    runner.bus.write_long(VBL_BATCH_FOREGROUND, 0);
    runner.m68k.cpu.write_reg(Register::PC, VBL_BATCH_PC);
    runner.advance_guest_tick();
    let tick = runner.guest_tick();
    let (_, running) = runner.run_steps(150, Some(tick));
    assert!(running);
    assert_eq!(runner.guest_tick(), tick, "callbacks must share their retrace");
    assert_eq!(runner.bus.read_long(VBL_BATCH_ORDER), order);
    assert_eq!(
        runner.bus.read_long(VBL_BATCH_FOREGROUND),
        order,
        "all due VBL tasks must run in order before foreground work"
    );
    assert!(runner.active_interrupt_callback.is_none());
    assert!(runner.dispatcher.vbl_tasks.iter().all(|task| !task.pending));
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), VBL_BATCH_SP);
}

#[test]
fn three_rearming_vbl_tasks_all_run_once_per_retrace() {
    let mut runner = vbl_batch_test_runner(&[1, 1, 1]);
    for retrace in 1..=4 {
        run_vbl_batch_retrace(&mut runner, 0x123);
        for index in 0..3 {
            assert_eq!(runner.bus.read_long(VBL_BATCH_MARKER + index * 4), retrace);
        }
    }
}

#[test]
fn patched_cursor_task_does_not_suppress_due_vbl_batch() {
    let mut runner = vbl_batch_test_runner(&[1, 1, 1]);
    let cursor_callback = 0x0004_2000;
    runner.bus.write_word(cursor_callback, 0x23FC); // MOVE.L #9,order
    runner.bus.write_long(cursor_callback + 2, 9);
    runner.bus.write_long(cursor_callback + 6, VBL_BATCH_ORDER);
    runner.bus.write_word(cursor_callback + 10, 0x4E75);
    runner
        .bus
        .write_long(crate::memory::globals::addr::J_CRSR_TASK, cursor_callback);
    for _ in 0..3 {
        run_vbl_batch_retrace(&mut runner, 0x9123);
    }
}

#[test]
fn vbl_batch_drain_preserves_mixed_rearm_intervals() {
    let mut runner = vbl_batch_test_runner(&[1, 2, 3]);
    for (index, order) in [0x1, 0x12, 0x13, 0x12, 0x1, 0x123].into_iter().enumerate() {
        run_vbl_batch_retrace(&mut runner, order);
        let retrace = index as u32 + 1;
        for (task, interval) in [1, 2, 3].into_iter().enumerate() {
            assert_eq!(
                runner.bus.read_long(VBL_BATCH_MARKER + task as u32 * 4),
                retrace / interval
            );
        }
    }
}

#[test]
fn vbl_callback_can_remove_later_pending_task_without_stalling_batch() {
    let mut runner = vbl_batch_test_runner(&[1, 1, 1]);
    let first_callback_return = 0x0004_1000 + 30;
    // Replace only this synthetic fixture's RTS with VRemove(second).
    runner.bus.write_word(first_callback_return, 0x207C); // MOVEA.L #second,A0
    runner.bus.write_long(first_callback_return + 2, 0x0020_2020);
    runner.bus.write_word(first_callback_return + 6, 0xA034); // VRemove
    runner.bus.write_word(first_callback_return + 8, 0x4E75);
    run_vbl_batch_retrace(&mut runner, 0x13);
    assert_eq!(runner.bus.read_long(VBL_BATCH_MARKER + 4), 0);
    assert_eq!(runner.dispatcher.vbl_tasks.len(), 2);
}

#[test]
fn null_vbl_callback_does_not_block_later_pending_task() {
    let mut runner = vbl_batch_test_runner(&[1, 1]);
    runner.bus.write_long(0x0020_2000 + 6, 0);
    run_vbl_batch_retrace(&mut runner, 0x2);
}

#[test]
fn due_timer_and_foreground_progress_after_finite_vbl_batch() {
    let mut runner = vbl_batch_test_runner(&[1, 1, 1]);
    let timer_callback = 0x0004_2000;
    let timer_marker = VBL_BATCH_MARKER + 12;
    runner.bus.write_word(timer_callback, 0x52B9); // ADDQ.L #1,timer_marker
    runner.bus.write_long(timer_callback + 2, timer_marker);
    runner.bus.write_word(timer_callback + 6, 0x4E75);
    let tick = runner.guest_tick() + 1;
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: 0x0020_2100,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: timer_callback,
        active: true,
        fire_at_tick: tick,
        fire_at_subtick: u64::from(tick) * 1_000_000,
        last_fired_tick: None,
    });
    run_vbl_batch_retrace(&mut runner, 0x123);
    let (_, running) = runner.run_steps(50, Some(tick));
    assert!(running);
    assert_eq!(runner.guest_tick(), tick);
    assert_eq!(runner.bus.read_long(timer_marker), 1);
}

#[test]
fn ordinary_slice_resumes_vbl_batch_after_audio_only_callback_return() {
    let mut runner = vbl_batch_test_runner(&[1, 1, 1]);
    runner.advance_guest_tick();
    let tick = runner.guest_tick();
    let (_, running) = runner.run_pending_sound_work(100);
    assert!(running);
    assert_eq!(runner.guest_tick(), tick);
    assert_eq!(runner.bus.read_long(VBL_BATCH_MARKER), 1);
    assert_eq!(runner.bus.read_long(VBL_BATCH_MARKER + 4), 0);
    assert_eq!(runner.bus.read_long(VBL_BATCH_FOREGROUND), 0);

    let (_, running) = runner.run_steps(100, Some(tick));
    assert!(running);
    assert_eq!(runner.guest_tick(), tick);
    assert_eq!(runner.bus.read_long(VBL_BATCH_ORDER), 0x123);
    assert_eq!(runner.bus.read_long(VBL_BATCH_FOREGROUND), 0x123);
}

#[test]
fn pending_vbl_batch_survives_audio_slice_starting_another_interrupt() {
    let mut runner = vbl_batch_test_runner(&[1, 1, 1]);
    let deferred_task = 0x0020_2200;
    let deferred_callback = 0x0004_2000;
    runner.bus.write_word(deferred_task + 4, 7);
    runner.bus.write_long(deferred_task + 8, deferred_callback);
    for offset in 0..30 {
        runner.bus.write_word(deferred_callback + offset * 2, 0x4E71);
    }
    runner.bus.write_word(deferred_callback + 60, 0x4E75);
    runner
        .dispatcher
        .enqueue_deferred_task(&mut runner.bus, deferred_task);
    runner.advance_guest_tick();
    let tick = runner.guest_tick();
    let (_, running) = runner.run_pending_sound_work(13);
    assert!(running);
    assert_eq!(
        runner.active_interrupt_callback.unwrap().source,
        ActiveInterruptCallbackSource::DeferredTask
    );
    assert_eq!(runner.bus.read_long(VBL_BATCH_ORDER), 0x1);
    assert_eq!(runner.bus.read_long(VBL_BATCH_FOREGROUND), 0);

    let (_, running) = runner.run_steps(150, Some(tick));
    assert!(running);
    assert_eq!(runner.guest_tick(), tick);
    assert_eq!(runner.bus.read_long(VBL_BATCH_ORDER), 0x123);
    assert_eq!(runner.bus.read_long(VBL_BATCH_FOREGROUND), 0x123);
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
            [(0, 0x0104), (0, 3), (0, 3), (0, 4), (0, 2)],
            [2, 67, 0x0900, 5],
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
