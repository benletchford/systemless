use super::*;
use crate::cpu::Register;

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
        0xA972u16, 0xA973, 0xA974, 0xA975, 0xA976, 0xA9EB, 0xA9EC, 0xA870, 0xA871, 0xA917, 0xA924,
        0xA92C, 0xA9DA, 0xA8AD, 0xAC70, 0xAC71, 0xAD17, 0xAD24, 0xAD2C, 0xADDA, 0xACAD,
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
fn handle_locks_and_rejected_sound_commands_do_not_cancel_an_idle_probe() {
    // Bad Mojo's demo loop polls GetNextEvent, FindWindow, HLock and a
    // no-wait SndDoCommand that the full channel queue rejects until
    // frame-boundary audio servicing drains it. Lock calls and rejected
    // commands leave the proof intact; an accepted command, which
    // changes channel state, and MoveHHi, which may write resource
    // backing, cancel.
    let probe = |runner: &mut FixtureRunner| {
        let trap_pc = 0x0002_0000u32;
        runner.idle_cycle_probe = None;
        runner.idle_cycle_sites = [IdleCycleSiteRecord::default(); IDLE_CYCLE_SITE_SLOTS];
        runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
        runner.set_guest_tick_for_test(100);
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(!runner.try_exact_idle_cycle_fastfwd(trap_pc, 200, Some(105)));
        assert!(runner.idle_cycle_probe.is_some());
    };
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    probe(&mut runner);
    for opcode in [0xA029u16, 0xA02A, 0xA229, 0xA42A] {
        runner.note_idle_cycle_trap_result(opcode);
        assert!(
            runner.idle_cycle_probe.is_some(),
            "handle lock trap {opcode:04X} must not cancel the probe"
        );
    }
    // SndDoCommand pops to its OSErr result.
    runner.bus.write_word(0x0010_0000, (-203i16) as u16); // queueFull
    for opcode in [0xA803u16, 0xAC03] {
        runner.note_idle_cycle_trap_result(opcode);
        assert!(
            runner.idle_cycle_probe.is_some(),
            "rejected SndDoCommand {opcode:04X} must not cancel the probe"
        );
    }
    runner.bus.write_word(0x0010_0000, 0); // noErr: the command was taken
    runner.note_idle_cycle_trap_result(0xA803);
    assert!(
        runner.idle_cycle_probe.is_none(),
        "an accepted command must cancel"
    );

    probe(&mut runner);
    runner.note_idle_cycle_trap_result(0xA064); // MoveHHi
    assert!(runner.idle_cycle_probe.is_none(), "MoveHHi must cancel");
}

#[test]
fn counted_idle_passes_stop_before_the_update_would_set_different_flags() {
    // ADDQ.W #1 from 0x001C: results up to 0x7FFF keep N, Z, V and C clear.
    assert_eq!(passes_with_unchanged_flags(0x001C, 1, 2), 0x7FFF - 0x001C);
    // Upper half: stop before the carry out at 0xFFFF -> 0x0000.
    assert_eq!(passes_with_unchanged_flags(0x8005, 1, 2), 0xFFFF - 0x8005);
    // The observed pass itself crossed the sign bit or produced zero.
    assert_eq!(passes_with_unchanged_flags(0x8000, 1, 2), 0);
    assert_eq!(passes_with_unchanged_flags(0x0000, 1, 2), 0);
    // Byte and long counters, and a step of four.
    assert_eq!(passes_with_unchanged_flags(0x10, 4, 1), (0x7F - 0x10) / 4);
    assert_eq!(
        passes_with_unchanged_flags(0x0001_0000, 1, 4),
        0x7FFF_FFFF - 0x0001_0000
    );
    // SUBQ.W #1 (step 0xFFFF): stop before zero, or before crossing back
    // below the sign bit.
    assert_eq!(passes_with_unchanged_flags(0x0010, 0xFFFF, 2), 0x0F);
    assert_eq!(passes_with_unchanged_flags(0x8010, 0xFFFF, 2), 0x10);
    assert_eq!(passes_with_unchanged_flags(0x7FFF, 0xFFFF, 2), 0);
}

#[test]
fn only_add_or_subtract_immediate_to_memory_may_update_an_idle_counter() {
    assert_eq!(add_immediate_to_memory_width(0x526D), Some(2)); // ADDQ.W #1,d16(A5)
    assert_eq!(add_immediate_to_memory_width(0x52B9), Some(4)); // ADDQ.L #1,abs.L
    assert_eq!(add_immediate_to_memory_width(0x5310), Some(1)); // SUBQ.B #1,(A0)
    assert_eq!(add_immediate_to_memory_width(0x066D), Some(2)); // ADDI.W #imm,d16(A5)
    assert_eq!(add_immediate_to_memory_width(0x04B8), Some(4)); // SUBI.L #imm,abs.W
    assert_eq!(add_immediate_to_memory_width(0x5241), None); // ADDQ.W #1,D1
    assert_eq!(add_immediate_to_memory_width(0x5249), None); // ADDQ.W #1,A1
    assert_eq!(add_immediate_to_memory_width(0x51C8), None); // DBF D0 (size 3)
    assert_eq!(add_immediate_to_memory_width(0x527A), None); // PC-relative destination
    assert_eq!(add_immediate_to_memory_width(0x302D), None); // MOVE.W d16(A5),D0
}

#[test]
fn an_idle_counter_is_verified_only_when_its_update_is_its_sole_reader() {
    use crate::memory::{AccessHit, AccessSource};
    let pc = 0x0061_7CAC;
    let hit = |write, address, len, source| AccessHit {
        write,
        address,
        len,
        source,
    };
    let rmw = AccessSource::Instruction(pc);
    let changes = [(0x0060_7724u32, 0x001C_0000u32, 0x001D_0000u32)];
    let opcode = |at: u32| if at == pc { 0x526D } else { 0x302D };
    let bytes = |address: u32| match address {
        0x0060_7724 => (0x00, 0x00),
        0x0060_7725 => (0x1C, 0x1D),
        _ => (0, 0),
    };
    let counter = IdleCounter {
        address: 0x0060_7724,
        width: 2,
        step: 1,
    };
    let pair = [
        hit(false, 0x0060_7724, 2, rmw),
        hit(true, 0x0060_7724, 2, rmw),
    ];
    assert_eq!(
        verified_idle_counter(&pair, &changes, opcode, bytes),
        Some(counter)
    );

    // Another instruction also reads it: its value can steer the loop.
    let other = AccessSource::Instruction(pc + 8);
    let read_elsewhere = [pair[0], pair[1], hit(false, 0x0060_7724, 2, other)];
    assert_eq!(
        verified_idle_counter(&read_elsewhere, &changes, opcode, bytes),
        None
    );
    // Host code (a trap) touched it.
    let host = [hit(false, 0x0060_7724, 2, AccessSource::Host), pair[1]];
    assert_eq!(verified_idle_counter(&host, &changes, opcode, bytes), None);
    // A load and a store by different instructions, or by a MOVE.
    let split = [pair[0], hit(true, 0x0060_7724, 2, other)];
    assert_eq!(verified_idle_counter(&split, &changes, opcode, bytes), None);
    let moved = [
        hit(false, 0x0060_7724, 2, other),
        hit(true, 0x0060_7724, 2, other),
    ];
    assert_eq!(verified_idle_counter(&moved, &changes, opcode, bytes), None);
    // Bad Mojo's ADDQ.L #1 on a long at $00607722: the bus stores only
    // the two low bytes that changed.
    let long_changes = [
        (0x0060_7720u32, 0x0000_0000u32, 0x0000_0000u32),
        (0x0060_7724, 0x0863_0000, 0x0864_0000),
    ];
    let long_changes = &long_changes[1..];
    let long_opcode = |at: u32| if at == pc { 0x52AD } else { 0x302D }; // ADDQ.L #1,d16(A5)
    let long_bytes = |address: u32| match address {
        0x0060_7724 => (0x08, 0x08),
        0x0060_7725 => (0x63, 0x64),
        _ => (0, 0),
    };
    let long_hits = [
        hit(false, 0x0060_7722, 4, rmw),
        hit(true, 0x0060_7724, 1, rmw),
        hit(true, 0x0060_7725, 1, rmw),
    ];
    assert_eq!(
        verified_idle_counter(&long_hits, long_changes, long_opcode, long_bytes),
        Some(IdleCounter {
            address: 0x0060_7722,
            width: 4,
            step: 1
        })
    );
    // A write outside the operand the instruction read.
    let stray = [long_hits[0], hit(true, 0x0060_7726, 1, rmw)];
    assert_eq!(
        verified_idle_counter(&stray, long_changes, long_opcode, long_bytes),
        None
    );
    // A changed byte the counter does not cover.
    let wider = [(0x0060_7724u32, 0x001C_0000u32, 0x001D_0001u32)];
    assert_eq!(verified_idle_counter(&pair, &wider, opcode, bytes), None);
}

#[test]
fn failed_counter_verifications_back_a_site_off_exponentially() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let site = 0x0002_0000u32;
    let resume = |runner: &FixtureRunner| {
        runner
            .idle_cycle_sites
            .iter()
            .find(|rec| rec.site == site)
            .unwrap()
            .counter_resume_tick
    };
    for (failure, wait) in [4u32, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 2048]
        .iter()
        .enumerate()
    {
        runner.note_idle_counter_verification(site, 1000, false);
        assert_eq!(resume(&runner), 1000 + wait, "failure {}", failure + 1);
    }
    runner.note_idle_counter_verification(site, 5000, true);
    assert_eq!(resume(&runner), 5000);
    runner.note_idle_counter_verification(site, 5000, false);
    assert_eq!(resume(&runner), 5004, "success resets the streak");
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
        ..IdleCycleSiteRecord::default()
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
        let result =
            runner
                .dispatcher
                .dispatch_dialog(true, 0x1DA, &mut runner.m68k.cpu, &mut runner.bus);
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
fn tracking_refire_freeze_policy_keeps_dialog_ticks_live() {
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
    for alert in [0xA985, 0xA986, 0xA987, 0xA988] {
        assert!(!tracking_refire_should_freeze_ticks(alert));
        assert!(tracking_refire_uses_dialog_callbacks(alert));
        assert!(tracking_refire_advances_gui_idle_tick(alert));
        assert!(tracking_refire_advances_gui_idle_tick(alert | 0x0400));
    }
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
    runner
        .dispatcher
        .input_state
        .set_mouse_button_for_test(true);
    runner
        .dispatcher
        .input_state
        .set_mouse_position_for_test((210, 248));

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
    runner
        .dispatcher
        .dialog_items
        .insert(dialog_ptr, Vec::new());

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
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));

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
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));

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
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));
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
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0010_0100));
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
