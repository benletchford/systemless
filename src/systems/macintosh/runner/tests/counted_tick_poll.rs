use super::*;

const BASE: u32 = 0x0020_0000;
const POST_TRAP: u32 = BASE + 6;

fn fixture(step: u16, counter: usize, sample: usize, deadline: usize) -> FixtureRunner {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let add = 0x5080 | ((step & 7) << 9) | counter as u16;
    for (i, word) in [
        add,
        0x594f,
        0xa975,
        0x201f | ((sample as u16) << 9),
        0xb080 | ((sample as u16) << 9) | deadline as u16,
        0x65f4,
    ]
    .into_iter()
    .enumerate()
    {
        runner.bus.write_word(BASE + i as u32 * 2, word);
    }
    runner.set_guest_tick_for_test(50);
    runner.set_instructions_per_tick(10_000);
    runner.m68k.cpu.core.set_a(7, 0x007f_ff04);
    runner.m68k.cpu.core.set_d(sample, 50);
    runner.m68k.cpu.core.set_d(deadline, 52);
    runner.m68k.cpu.core.pc = BASE;
    // An uncapped realtime call disables spin fast-forward. Retire the first
    // actual ADDQ, result allocation and canonical TickCount before comparing.
    assert_eq!(
        runner.run_steps_internal(3, None, 0, true, false, FrameFinalization::Deferred),
        (3, true)
    );
    assert_eq!(runner.m68k.cpu.core.pc, POST_TRAP);
    runner
}

#[test]
fn counted_clock_poll_matches_cpu_execution_registers_flags_memory_and_units() {
    for (step, counter, sample, deadline) in [(1, 3, 0, 4), (8, 7, 2, 1), (3, 0, 6, 5)] {
        for value in [16, 0x8000_0010] {
            let mut optimized = fixture(step, counter, sample, deadline);
            let mut ordinary = fixture(step, counter, sample, deadline);
            for runner in [&mut optimized, &mut ordinary] {
                runner.m68k.cpu.core.set_d(counter, value);
                runner
                    .m68k
                    .cpu
                    .core
                    .set_ccr(if value & 0x8000_0000 != 0 { 8 } else { 0 });
            }
            let mut count = 0;
            assert_eq!(
                optimized.try_counted_tick_poll(POST_TRAP, 600, &mut count, Some(51)),
                100
            );
            assert_eq!(count, 600);
            assert_eq!(
                ordinary.run_steps_internal(
                    count,
                    None,
                    0,
                    true,
                    false,
                    FrameFinalization::Deferred
                ),
                (600, true)
            );
            assert_eq!(
                CpuArchitecturalSnapshot::capture(&optimized.m68k.cpu.core),
                CpuArchitecturalSnapshot::capture(&ordinary.m68k.cpu.core)
            );
            assert_eq!(optimized.tick_budget, ordinary.tick_budget);
            assert_eq!(optimized.total_instructions, ordinary.total_instructions);
            assert_eq!(
                optimized.bus.ram_slice(0, optimized.bus.ram_size()),
                ordinary.bus.ram_slice(0, ordinary.bus.ram_size())
            );
        }
    }
}

#[test]
fn counted_clock_poll_stops_before_tick_timer_and_counter_flag_boundaries() {
    let mut runner = fixture(1, 3, 0, 4);
    runner.tick_budget = 61;
    let mut count = 0;
    assert_eq!(
        runner.try_counted_tick_poll(POST_TRAP, 1000, &mut count, Some(51)),
        10
    );
    assert_eq!(runner.tick_budget, 1);
    assert_eq!(runner.guest_tick(), 50);
    assert_eq!(runner.m68k.cpu.core.d(3), 11);
    // A pending subtick timer limits the skip before its first due instruction.
    let mut runner = fixture(1, 3, 0, 4);
    runner.tick_budget = 10_000;
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: 0x0030_0000,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: 0x0021_0000,
        active: true,
        fire_at_tick: 50,
        fire_at_subtick: 50_012_000,
        last_fired_tick: None,
    });
    let mut count = 0;
    assert_eq!(
        runner.try_counted_tick_poll(POST_TRAP, 1000, &mut count, Some(51)),
        19
    );
    assert_eq!(count, 114);
    assert!(!runner.m68k_timer_due_within(1));
    assert!(runner.m68k_timer_due_within(6));
    for value in [0x7fff_fffe, 0xffff_fffe] {
        let mut runner = fixture(1, 3, 0, 4);
        runner.m68k.cpu.core.set_d(3, value);
        runner
            .m68k
            .cpu
            .core
            .set_ccr(if value & 0x8000_0000 != 0 { 8 } else { 0 });
        let mut count = 0;
        assert_eq!(
            runner.try_counted_tick_poll(POST_TRAP, 1000, &mut count, Some(51)),
            1
        );
        assert_eq!(runner.m68k.cpu.core.d(3), value + 1);
    }
}

#[test]
fn counted_clock_poll_rejects_expiry_stale_sample_flags_changed_code_and_patch() {
    for case in 0..8 {
        let mut runner = fixture(1, 3, 0, 4);
        match case {
            0 => runner.m68k.cpu.core.set_d(4, 50),
            1 => runner.m68k.cpu.core.set_d(0, 49),
            2 => runner.m68k.cpu.core.set_ccr(4),
            3 => runner.bus.write_word(BASE + 10, 0x64f4),
            4 => runner.bus.write_word(BASE, 0x5284), // counter aliases deadline
            5 => runner.bus.write_long(runner.m68k.cpu.core.a(7), 49),
            6 => runner
                .dispatcher
                .install_trap_address(&mut runner.bus, 0xa975, 0x0021_0000)
                .unwrap(),
            _ => runner.tick_budget = 6,
        }
        let before = CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core);
        let budget = runner.tick_budget;
        let mut count = 0;
        assert_eq!(
            runner.try_counted_tick_poll(POST_TRAP, 1000, &mut count, Some(51)),
            0,
            "case {case}"
        );
        assert_eq!(count, 0);
        assert_eq!(runner.tick_budget, budget);
        assert_eq!(
            CpuArchitecturalSnapshot::capture(&runner.m68k.cpu.core),
            before
        );
    }
    let mut runner = fixture(1, 3, 0, 4);
    let mut count = 0;
    assert_eq!(
        runner.try_counted_tick_poll(POST_TRAP, 1000, &mut count, Some(50)),
        0
    );
    assert_eq!(
        runner.try_counted_tick_poll(POST_TRAP, 5, &mut count, Some(51)),
        0
    );
}

#[test]
#[cfg(feature = "debug")]
fn counted_clock_poll_preserves_instruction_breakpoints() {
    use crate::debug::{handle_debug_request, BreakpointSpec, DebugRequest};
    let mut runner = fixture(1, 3, 0, 4);
    handle_debug_request(
        &mut runner,
        DebugRequest::SetBreakpoint {
            spec: BreakpointSpec::program_counter(u64::from(BASE)),
        },
    )
    .unwrap();
    let mut count = 0;
    assert_eq!(
        runner.try_counted_tick_poll(POST_TRAP, 1000, &mut count, Some(51)),
        0
    );
    assert_eq!(count, 0);
}
