//! Public-API smoke tests — confirms the crate's surface compiles
//! and runs in isolation from any consumer (systemless etc.).
//!
//! The deeper unit-test coverage of every instruction, memory
//! family, and run-loop variant currently lives in
//! `systemless/src/cpu/ppc/tests.rs` for historical reasons (the
//! tests predate the crate split). Splitting those tests between
//! the two crates is a follow-up.

use ppc::{
    decode, PpcCpu, PpcException, PpcFetchHistogram, PpcFetchedInstruction,
    PpcIllegalInstructionReason, PpcImportAction, PpcInstr, PpcMemory, PpcMemoryAccess,
    PpcRunResult, PpcSectionMem, PPC_MSR_FP_AVAILABLE_MASK,
};

/// Build a minimal D-form instruction word: opcd:6, rt:5, ra:5, d:16.
fn d_form(opcd: u8, rt: u8, ra: u8, d: i16) -> u32 {
    ((opcd as u32) << 26)
        | ((rt as u32 & 0x1F) << 21)
        | ((ra as u32 & 0x1F) << 16)
        | (d as u16 as u32)
}

fn d_form_compare(opcd: u8, bf: u8, l: bool, ra: u8, imm: u16) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((bf as u32 & 0x07) << 23)
        | (if l { 1u32 << 21 } else { 0 })
        | ((ra as u32 & 0x1F) << 16)
        | u32::from(imm)
}

#[test]
fn fresh_cpu_state_has_classic_mac_defaults() {
    let cpu = PpcCpu::new();
    assert_eq!(cpu.pc, 0);
    assert_eq!(cpu.cr, 0);
    assert_eq!(cpu.lr, 0);
    assert_eq!(cpu.ctr, 0);
    assert_eq!(cpu.xer, 0);
    assert_eq!(cpu.msr, PPC_MSR_FP_AVAILABLE_MASK);
    assert!(cpu.msr_fp_available());
    for &v in cpu.gpr.iter() {
        assert_eq!(v, 0);
    }
    for &v in cpu.fpr.iter() {
        assert_eq!(v, 0);
    }
}

#[test]
fn run_fp_instruction_with_msr_fp_clear_surfaces_unavailable_exception() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x100, 0xFC61_102Au32.to_be_bytes().to_vec()); // fadd f3,f1,f2

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.set_msr_fp_available(false);

    let result = cpu.run(&mut mem, 16, 0);

    assert_eq!(
        result,
        PpcRunResult::Exception {
            pc: 0x100,
            exception: PpcException::FloatingPointUnavailable,
            cycles: 0,
        }
    );
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn run_reserved_opcode_surfaces_illegal_instruction_exception() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x100, 0u32.to_be_bytes().to_vec());

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;

    let result = cpu.run(&mut mem, 16, 0);

    assert_eq!(
        result,
        PpcRunResult::Exception {
            pc: 0x100,
            exception: PpcException::IllegalInstruction {
                word: 0,
                reason: PpcIllegalInstructionReason::ReservedOpcode,
            },
            cycles: 0,
        }
    );
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn decode_addi_recognises_opcd_14() {
    let word = d_form(14, 3, 0, 42);
    let instr = decode(word).expect("addi must decode");
    assert!(matches!(instr, PpcInstr::Addi { .. }), "got {instr:?}");
}

#[test]
fn step_instruction_executes_addi_with_ra_zero_literal() {
    let mut cpu = PpcCpu::new();
    let word = d_form(14, 3, 0, 42); // addi r3, 0, 42
    let res = cpu.step_instruction(word);
    assert!(matches!(res, ppc::PpcStepResult::Stepped));
    assert_eq!(cpu.gpr[3], 42);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn run_executes_addi_blr_to_halt_pc() {
    // Two-instruction program at 0x100:
    //   addi r3, 0, 42
    //   blr            (returns to LR; LR = 0 sentinel)
    let mut mem = PpcSectionMem::new();
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&d_form(14, 3, 0, 42).to_be_bytes());
    code.extend_from_slice(&0x4E80_0020u32.to_be_bytes()); // blr
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.lr = 0;
    let res = cpu.run(&mut mem, 16, 0);
    assert!(matches!(res, PpcRunResult::Halted { pc: 0, .. }), "{res:?}");
    assert_eq!(cpu.gpr[3], 42);
}

#[test]
fn run_with_imports_fast_paths_exact_nop_and_blr_words() {
    let mut mem = PpcSectionMem::new();
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&0x6000_0000u32.to_be_bytes()); // nop
    code.extend_from_slice(&0x4E80_0020u32.to_be_bytes()); // blr
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.lr = 0;

    let res = cpu.run_with_imports(&mut mem, 16, 0, 0x4000, 0, |_idx, _cpu, _mem| {
        PpcImportAction::Halt
    });

    assert_eq!(res, PpcRunResult::Halted { pc: 0, cycles: 2 });
    assert_eq!(cpu.pc, 0);
}

#[test]
fn run_with_imports_fast_path_preserves_signed_cmpi_and_unsigned_cmpli() {
    let mut mem = PpcSectionMem::new();
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&d_form_compare(11, 0, false, 3, 0xFFFF).to_be_bytes()); // cmpwi cr0, r3, -1
    code.extend_from_slice(&d_form_compare(10, 1, false, 4, 0xFFFF).to_be_bytes()); // cmplwi cr1, r4, 0xFFFF
    code.extend_from_slice(&0x4E80_0020u32.to_be_bytes()); // blr
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.lr = 0;
    cpu.gpr[3] = 0xFFFF_FFFF;
    cpu.gpr[4] = 1;

    let res = cpu.run_with_imports(&mut mem, 16, 0, 0x4000, 0, |_idx, _cpu, _mem| {
        PpcImportAction::Halt
    });

    assert_eq!(res, PpcRunResult::Halted { pc: 0, cycles: 3 });
    assert_eq!(cpu.cr_field(0), 0b0010);
    assert_eq!(cpu.cr_field(1), 0b1000);
}

#[test]
fn run_with_imports_fast_path_executes_rlwimi_and_updates_cr0() {
    let mut mem = PpcSectionMem::new();
    let rlwimi =
        (20u32 << 26) | (4u32 << 21) | (3u32 << 16) | (8u32 << 11) | (8u32 << 6) | (15u32 << 1) | 1;
    let mut code = Vec::new();
    code.extend_from_slice(&rlwimi.to_be_bytes());
    code.extend_from_slice(&0x4E80_0020u32.to_be_bytes());
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[3] = 0xAAAA_5555;
    cpu.gpr[4] = 0x1234_5678;

    let result = cpu.run_with_imports(&mut mem, 16, 0, 0x4000, 0, |_idx, _cpu, _mem| {
        PpcImportAction::Halt
    });

    assert_eq!(result, PpcRunResult::Halted { pc: 0, cycles: 2 });
    assert_eq!(cpu.gpr[3], 0xAA56_5555);
    assert_eq!(cpu.cr_field(0), 0b1000);
}

#[test]
fn run_with_fetch_observer_records_successful_fetches() {
    let addi = d_form(14, 3, 0, 42);
    let blr = 0x4E80_0020u32;
    let mut mem = PpcSectionMem::new();
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&addi.to_be_bytes());
    code.extend_from_slice(&blr.to_be_bytes());
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.lr = 0;
    let mut trace = Vec::new();
    let res = cpu.run_with_fetch_observer(&mut mem, 16, 0, &mut trace);

    assert!(matches!(res, PpcRunResult::Halted { pc: 0, .. }), "{res:?}");
    assert_eq!(
        trace,
        vec![
            PpcFetchedInstruction {
                pc: 0x100,
                word: addi
            },
            PpcFetchedInstruction {
                pc: 0x104,
                word: blr
            },
        ]
    );

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.lr = 0;
    let mut histogram = PpcFetchHistogram::new();
    let res = cpu.run_with_fetch_observer(&mut mem, 16, 0, &mut histogram);

    assert!(matches!(res, PpcRunResult::Halted { pc: 0, .. }), "{res:?}");
    assert_eq!(histogram.total(), 2);
    assert_eq!(histogram.primary_count(14), 1);
    assert_eq!(histogram.primary_count(19), 1);
    assert_eq!(histogram.secondary_count(19, 16), 1);
    assert_eq!(histogram.word_count(addi), 1);
    assert_eq!(histogram.word_count(blr), 1);
    assert_eq!(histogram.pc_count(0x100), 1);
    assert_eq!(histogram.pc_count(0x104), 1);
}

#[test]
fn ppc_fetch_histogram_can_merge_budgeted_run_counts() {
    let addi = d_form(14, 3, 0, 42);
    let blr = 0x4E80_0020u32;
    let mut left = PpcFetchHistogram::new();
    left.record_fetch_at(Some(0x100), addi);
    let mut right = PpcFetchHistogram::new();
    right.record_fetch_at(Some(0x100), addi);
    right.record_fetch_at(Some(0x104), blr);

    left.merge_from(&right);

    assert_eq!(left.total(), 3);
    assert_eq!(left.primary_count(14), 2);
    assert_eq!(left.primary_count(19), 1);
    assert_eq!(left.secondary_count(19, 16), 1);
    assert_eq!(left.word_count(addi), 2);
    assert_eq!(left.word_count(blr), 1);
    assert_eq!(left.pc_count(0x100), 2);
    assert_eq!(left.pc_count(0x104), 1);
}

#[test]
fn ppc_fetch_histogram_summarizes_decoder_coverage() {
    let addi = d_form(14, 3, 0, 42);
    let unsupported_primary = 0;
    let unsupported_secondary = (31u32 << 26) | (999u32 << 1);
    let mut histogram = PpcFetchHistogram::new();
    histogram.record_fetch(addi);
    histogram.record_fetch(addi);
    histogram.record_fetch(unsupported_primary);
    histogram.record_fetch(unsupported_secondary);
    histogram.record_fetch(unsupported_secondary);

    let summary = histogram.decode_summary();

    assert_eq!(summary.total(), 5);
    assert_eq!(summary.decoded(), 2);
    assert_eq!(summary.unsupported(), 3);
    assert!(!summary.is_fully_decoded());
    assert_eq!(summary.unsupported_primary().get(&0), Some(&1));
    assert_eq!(summary.unsupported_secondary().get(&(31, 999)), Some(&2));
}

#[test]
fn run_surfaces_system_call_exception_without_counting_instruction() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x100, 0x4400_0002u32.to_be_bytes().to_vec());

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let res = cpu.run(&mut mem, 16, 0);

    assert_eq!(
        res,
        PpcRunResult::Exception {
            pc: 0x100,
            exception: PpcException::SystemCall { lev: 0 },
            cycles: 0
        }
    );
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn run_surfaces_unaligned_fetch_as_exception() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x100, vec![0u8; 8]);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x102;
    let res = cpu.run(&mut mem, 16, 0);

    assert_eq!(
        res,
        PpcRunResult::Exception {
            pc: 0x102,
            exception: PpcException::Alignment {
                addr: 0x102,
                size: 4,
                access: PpcMemoryAccess::InstructionFetch
            },
            cycles: 0
        }
    );
    assert_eq!(cpu.pc, 0x102);
}

#[test]
fn ppc_section_mem_round_trips_be_words() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0u8; 16]);
    mem.write_u32_be(0x1000, 0xDEAD_BEEF).unwrap();
    assert_eq!(mem.read_u32_be(0x1000), Some(0xDEAD_BEEF));
    // Unmapped read returns None.
    assert_eq!(mem.read_u32_be(0x9999), None);
}

#[test]
fn run_with_imports_dispatches_halt_and_return() {
    use ppc::PpcImportAction;
    // Caller jumps into the synthetic trap region:
    //   bl trap0
    //   ba 0
    let trap_base: u32 = 0x4000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    let ba_zero = 0x4800_0002u32; // ba 0
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&bl_word.to_be_bytes());
    code.extend_from_slice(&ba_zero.to_be_bytes());
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.lr = 0;
    let res = cpu.run_with_imports(&mut mem, 50, 0, trap_base, 1, |_idx, _cpu, _mem| {
        PpcImportAction::Return(0xCAFE_BABE)
    });
    assert!(matches!(res, PpcRunResult::Halted { pc: 0, .. }), "{res:?}");
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);
}

#[test]
fn run_with_imports_fast_paths_cfm_tvector_import_stub() {
    let code_base = 0x1000;
    let toc_base = 0x2000;
    let stack_base = 0x3000;
    let tvector = 0x4000;
    let trap_base = 0x5000;
    let halt_pc = 0x6000;
    let caller_rtoc = toc_base;
    let import_rtoc = 0x7000;
    let trap_entry = trap_base + 4;
    let toc_slot_disp = 0x10u16;

    let mut code = Vec::new();
    for word in [
        0x8182_0000 | u32::from(toc_slot_disp), // lwz r12,disp(r2)
        0x9041_0014,                            // stw r2,20(r1)
        0x800C_0000,                            // lwz r0,0(r12)
        0x804C_0004,                            // lwz r2,4(r12)
        0x7C09_03A6,                            // mtctr r0
        0x4E80_0420,                            // bctr
    ] {
        code.extend_from_slice(&word.to_be_bytes());
    }

    let mut mem = PpcSectionMem::new();
    mem.add_region(code_base, code);
    mem.add_region(toc_base, vec![0; 0x40]);
    mem.add_region(stack_base, vec![0; 0x40]);
    mem.add_region(tvector, vec![0; 8]);
    mem.write_u32_be(toc_base + u32::from(toc_slot_disp), tvector)
        .unwrap();
    mem.write_u32_be(tvector, trap_entry).unwrap();
    mem.write_u32_be(tvector + 4, import_rtoc).unwrap();

    let mut cpu = PpcCpu::new();
    cpu.pc = code_base;
    cpu.lr = halt_pc;
    cpu.gpr[1] = stack_base;
    cpu.gpr[2] = caller_rtoc;

    let mut calls = 0u32;
    let result = cpu.run_with_imports(&mut mem, 8, halt_pc, trap_base, 2, |index, cpu, mem| {
        calls += 1;
        assert_eq!(calls, 1);
        assert_eq!(index, 1);
        assert_eq!(cpu.pc, trap_entry);
        assert_eq!(cpu.gpr[2], import_rtoc);
        assert_eq!(cpu.gpr[12], tvector);
        assert_eq!(cpu.ctr, trap_entry);
        assert_eq!(mem.read_u32_be(stack_base + 20), Some(caller_rtoc));
        PpcImportAction::Return(0xABCD)
    });

    assert!(matches!(
        result,
        PpcRunResult::Halted {
            pc,
            cycles: 7
        } if pc == halt_pc
    ));
    assert_eq!(cpu.pc, halt_pc);
    assert_eq!(cpu.gpr[2], import_rtoc);
    assert_eq!(cpu.gpr[3], 0xABCD);

    let second_import_rtoc = 0x8000;
    mem.write_u32_be(tvector, trap_base).unwrap();
    mem.write_u32_be(tvector + 4, second_import_rtoc).unwrap();
    cpu.pc = code_base;
    cpu.lr = halt_pc;
    cpu.gpr[2] = caller_rtoc;

    let result = cpu.run_with_imports(&mut mem, 8, halt_pc, trap_base, 2, |index, cpu, mem| {
        calls += 1;
        assert_eq!(calls, 2);
        assert_eq!(index, 0);
        assert_eq!(cpu.pc, trap_base);
        assert_eq!(cpu.gpr[2], second_import_rtoc);
        assert_eq!(cpu.gpr[12], tvector);
        assert_eq!(cpu.ctr, trap_base);
        assert_eq!(mem.read_u32_be(stack_base + 20), Some(caller_rtoc));
        PpcImportAction::Return(0xDCBA)
    });

    assert!(matches!(
        result,
        PpcRunResult::Halted {
            pc,
            cycles: 7
        } if pc == halt_pc
    ));
    assert_eq!(calls, 2);
    assert_eq!(cpu.pc, halt_pc);
    assert_eq!(cpu.gpr[2], second_import_rtoc);
    assert_eq!(cpu.gpr[3], 0xDCBA);
}

#[test]
fn run_with_imports_fetch_observer_skips_handled_import_slots() {
    use ppc::PpcImportAction;

    let trap_base: u32 = 0x4000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    let ba_zero = 0x4800_0002u32;
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&bl_word.to_be_bytes());
    code.extend_from_slice(&ba_zero.to_be_bytes());
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let mut trace = Vec::new();
    let res = cpu.run_with_imports_and_fetch_observer(
        &mut mem,
        50,
        0,
        trap_base,
        1,
        &mut trace,
        |_idx, _cpu, _mem| PpcImportAction::Return(0xCAFE_BABE),
    );

    assert!(matches!(res, PpcRunResult::Halted { pc: 0, .. }), "{res:?}");
    assert_eq!(
        trace,
        vec![
            PpcFetchedInstruction {
                pc: 0x100,
                word: bl_word
            },
            PpcFetchedInstruction {
                pc: 0x104,
                word: ba_zero
            },
        ]
    );
}

#[test]
fn run_with_imports_can_raise_structured_import_exception() {
    use ppc::PpcImportAction;

    let trap_base: u32 = 0x4000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    mem.add_readonly_region(0x100, bl_word.to_be_bytes().to_vec());

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let res = cpu.run_with_imports(&mut mem, 50, 0, trap_base, 1, |idx, _cpu, _mem| {
        PpcImportAction::RaiseException(PpcException::HostImportTrap { index: idx })
    });

    assert_eq!(
        res,
        PpcRunResult::Exception {
            pc: trap_base,
            exception: PpcException::HostImportTrap { index: 0 },
            cycles: 1,
        }
    );
    assert_eq!(cpu.pc, trap_base);
}

#[test]
fn run_with_imports_can_return_without_clobbering_gpr3() {
    use ppc::PpcImportAction;

    let trap_base: u32 = 0x4000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    let ba_zero = 0x4800_0002u32;
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&bl_word.to_be_bytes());
    code.extend_from_slice(&ba_zero.to_be_bytes());
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[3] = 0x1234_5678;
    let res = cpu.run_with_imports(&mut mem, 50, 0, trap_base, 1, |_idx, _cpu, _mem| {
        PpcImportAction::ReturnPreserve
    });

    assert!(matches!(res, PpcRunResult::Halted { pc: 0, .. }), "{res:?}");
    assert_eq!(cpu.gpr[3], 0x1234_5678);
}

#[test]
fn run_with_imports_can_return_value_with_extra_cycles() {
    use ppc::PpcImportAction;

    let trap_base: u32 = 0x4000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    let ba_zero = 0x4800_0002u32;
    let mut code: Vec<u8> = Vec::new();
    code.extend_from_slice(&bl_word.to_be_bytes());
    code.extend_from_slice(&ba_zero.to_be_bytes());
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let res = cpu.run_with_imports(&mut mem, 50, 0, trap_base, 1, |_idx, _cpu, _mem| {
        PpcImportAction::ReturnWithExtraCycles(0xCAFE_BABE, 12)
    });

    assert!(
        matches!(res, PpcRunResult::Halted { pc: 0, cycles: 15 }),
        "{res:?}"
    );
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);
}

#[test]
fn run_with_imports_can_enter_native_callback_and_restore_rtoc_across_budgets() {
    use ppc::{PpcImportAction, PpcNativeReturnGpr3};

    let trap_base: u32 = 0x4000;
    let callback_entry: u32 = 0x200;
    let callback_return_pc: u32 = 0x5000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    let ba_zero = 0x4800_0002u32;
    let mut caller: Vec<u8> = Vec::new();
    caller.extend_from_slice(&bl_word.to_be_bytes());
    caller.extend_from_slice(&ba_zero.to_be_bytes());
    mem.add_readonly_region(0x100, caller);

    let addi_r3_r3_1 = (14u32 << 26) | (3u32 << 21) | (3u32 << 16) | 1;
    let blr = 0x4e80_0020u32;
    let mut callback: Vec<u8> = Vec::new();
    callback.extend_from_slice(&addi_r3_r3_1.to_be_bytes());
    callback.extend_from_slice(&blr.to_be_bytes());
    mem.add_readonly_region(callback_entry, callback);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[2] = 0x1111_2222;
    let first = cpu.run_with_imports(&mut mem, 2, 0, trap_base, 1, |_idx, cpu, _mem| {
        let final_pc = cpu.lr;
        let restore_rtoc = cpu.gpr[2];
        cpu.gpr[3] = 0x41;
        PpcImportAction::CallNative {
            entry: callback_entry,
            rtoc: 0x3333_4444,
            return_pc: callback_return_pc,
            final_pc,
            restore_rtoc,
            return_gpr3: PpcNativeReturnGpr3::Preserve,
        }
    });
    assert!(
        matches!(first, PpcRunResult::CycleLimit { cycles: 2 }),
        "{first:?}"
    );
    assert_eq!(cpu.pc, callback_entry);
    assert_eq!(cpu.gpr[2], 0x3333_4444);

    let second = cpu.run_with_imports(&mut mem, 50, 0, trap_base, 1, |_idx, _cpu, _mem| {
        panic!("callback should not call imports")
    });

    assert!(
        matches!(second, PpcRunResult::Halted { pc: 0, .. }),
        "{second:?}"
    );
    assert_eq!(cpu.gpr[2], 0x1111_2222);
    assert_eq!(cpu.gpr[3], 0x42);
    assert_eq!(cpu.lr, 0x104);
}

#[test]
fn run_with_imports_can_mask_native_callback_return_gpr3() {
    use ppc::{PpcImportAction, PpcNativeReturnGpr3};

    let trap_base: u32 = 0x4000;
    let callback_entry: u32 = 0x200;
    let callback_return_pc: u32 = 0x5000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    let ba_zero = 0x4800_0002u32;
    let mut caller: Vec<u8> = Vec::new();
    caller.extend_from_slice(&bl_word.to_be_bytes());
    caller.extend_from_slice(&ba_zero.to_be_bytes());
    mem.add_readonly_region(0x100, caller);

    let mut callback: Vec<u8> = Vec::new();
    callback.extend_from_slice(&d_form(15, 3, 0, 0xcafeu16 as i16).to_be_bytes());
    callback.extend_from_slice(&d_form(24, 3, 3, 0xbeefu16 as i16).to_be_bytes());
    callback.extend_from_slice(&0x4e80_0020u32.to_be_bytes());
    mem.add_readonly_region(callback_entry, callback);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[2] = 0x1111_2222;
    let result = cpu.run_with_imports(&mut mem, 50, 0, trap_base, 1, |_idx, cpu, _mem| {
        let final_pc = cpu.lr;
        let restore_rtoc = cpu.gpr[2];
        PpcImportAction::CallNative {
            entry: callback_entry,
            rtoc: 0x3333_4444,
            return_pc: callback_return_pc,
            final_pc,
            restore_rtoc,
            return_gpr3: PpcNativeReturnGpr3::Mask(0x0000_ffff),
        }
    });

    assert!(
        matches!(result, PpcRunResult::Halted { pc: 0, .. }),
        "{result:?}"
    );
    assert_eq!(cpu.gpr[2], 0x1111_2222);
    assert_eq!(cpu.gpr[3], 0xbeef);
    assert_eq!(cpu.lr, 0x104);
}

#[test]
fn run_with_imports_can_return_native_callback_cr_bit_as_gpr3() {
    use ppc::{PpcImportAction, PpcNativeReturnGpr3};

    let trap_base: u32 = 0x4000;
    let callback_entry: u32 = 0x200;
    let callback_return_pc: u32 = 0x5000;
    let mut mem = PpcSectionMem::new();
    let bl_word =
        (18u32 << 26) | ((((trap_base as i32 - 0x100) >> 2) & 0x00FF_FFFF) as u32) << 2 | 0x1;
    let ba_zero = 0x4800_0002u32;
    let mut caller: Vec<u8> = Vec::new();
    caller.extend_from_slice(&bl_word.to_be_bytes());
    caller.extend_from_slice(&ba_zero.to_be_bytes());
    mem.add_readonly_region(0x100, caller);

    let mut callback: Vec<u8> = Vec::new();
    callback.extend_from_slice(&d_form(28, 3, 3, 0).to_be_bytes());
    callback.extend_from_slice(&0x4e80_0020u32.to_be_bytes());
    mem.add_readonly_region(callback_entry, callback);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[2] = 0x1111_2222;
    cpu.gpr[3] = 0x1234_5678;
    let result = cpu.run_with_imports(&mut mem, 50, 0, trap_base, 1, |_idx, cpu, _mem| {
        let final_pc = cpu.lr;
        let restore_rtoc = cpu.gpr[2];
        PpcImportAction::CallNative {
            entry: callback_entry,
            rtoc: 0x3333_4444,
            return_pc: callback_return_pc,
            final_pc,
            restore_rtoc,
            return_gpr3: PpcNativeReturnGpr3::CrBit(2),
        }
    });

    assert!(
        matches!(result, PpcRunResult::Halted { pc: 0, .. }),
        "{result:?}"
    );
    assert_eq!(cpu.gpr[2], 0x1111_2222);
    assert_eq!(cpu.gpr[3], 1);
    assert_eq!(cpu.lr, 0x104);
}

#[test]
fn run_with_imports_cycle_handler_reports_elapsed_cycles() {
    let trap_base = 0x4000;
    let mut mem = PpcSectionMem::new();
    let mut code = Vec::new();
    code.extend_from_slice(&0x6000_0000u32.to_be_bytes());
    code.extend_from_slice(&0x6000_0000u32.to_be_bytes());
    code.extend_from_slice(&0x4800_4002u32.to_be_bytes());
    mem.add_readonly_region(0x100, code);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let mut observed_cycles = None;
    let result = cpu.run_with_imports_and_cycle_handler(
        &mut mem,
        16,
        0,
        trap_base,
        1,
        |cycles, index, _cpu, _mem| {
            observed_cycles = Some((cycles, index));
            PpcImportAction::Halt
        },
    );

    assert_eq!(observed_cycles, Some((3, 0)));
    assert!(matches!(result, PpcRunResult::Halted { cycles: 3, .. }));
}
