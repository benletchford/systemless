use super::*;
use crate::cpu::Register;

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
