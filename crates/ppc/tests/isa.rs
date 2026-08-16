//! ISA-level tests for the PowerPC interpreter — every
//! mnemonic the decoder/dispatcher recognises, plus memory
//! bus + run loop coverage. Migrated from
//! systemless/src/cpu/ppc/tests.rs as part of the crate
//! split; the PEF-aware tests stay in systemless because
//! they exercise its Mac-specific loader and RunLoaded
//! extension trait.

use ppc::*;

fn assert_illegal_instruction(
    result: PpcStepResult,
    word: u32,
    reason: PpcIllegalInstructionReason,
) {
    assert_eq!(
        result,
        PpcStepResult::Exception(PpcException::IllegalInstruction { word, reason })
    );
}

#[test]
fn cr_field_reads_msb_first_nibbles() {
    // PowerPC numbers CR fields with field 0 = bits 0..3
    // (most significant). With MSB=0 bit ordering the most
    // significant CR field is the high nibble of cr.
    let mut cpu = PpcCpu::new();
    cpu.cr = 0xABCD_EF12;
    assert_eq!(cpu.cr_field(0), 0xA);
    assert_eq!(cpu.cr_field(1), 0xB);
    assert_eq!(cpu.cr_field(2), 0xC);
    assert_eq!(cpu.cr_field(3), 0xD);
    assert_eq!(cpu.cr_field(4), 0xE);
    assert_eq!(cpu.cr_field(5), 0xF);
    assert_eq!(cpu.cr_field(6), 0x1);
    assert_eq!(cpu.cr_field(7), 0x2);
}

#[test]
fn set_cr_field_only_modifies_the_target_nibble() {
    let mut cpu = PpcCpu::new();
    cpu.cr = 0xABCD_EF12;
    cpu.set_cr_field(3, 0x9);
    // CR3 nibble was 0xD, now 0x9 — other fields preserved.
    assert_eq!(cpu.cr, 0xABC9_EF12);
    cpu.set_cr_field(0, 0x5);
    assert_eq!(cpu.cr, 0x5BC9_EF12);
    cpu.set_cr_field(7, 0xF);
    assert_eq!(cpu.cr, 0x5BC9_EF1F);
}

#[test]
fn set_cr_field_masks_high_bits_of_value() {
    let mut cpu = PpcCpu::new();
    // Pass an 8-bit value with extra bits beyond nibble width;
    // verify only low 4 bits land.
    cpu.set_cr_field(0, 0xFF);
    assert_eq!(cpu.cr, 0xF000_0000);
}

#[test]
fn fpscr_field_reads_msb_first_nibbles() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr(0x1234_5678);
    assert_eq!(cpu.fpscr(), 0x1234_5678);
    assert_eq!(cpu.fpscr_field(0), 0x1);
    assert_eq!(cpu.fpscr_field(1), 0x2);
    assert_eq!(cpu.fpscr_field(2), 0x3);
    assert_eq!(cpu.fpscr_field(3), 0x4);
    assert_eq!(cpu.fpscr_field(4), 0x5);
    assert_eq!(cpu.fpscr_field(5), 0x6);
    assert_eq!(cpu.fpscr_field(6), 0x7);
    assert_eq!(cpu.fpscr_field(7), 0x8);
}

#[test]
fn set_fpscr_field_and_bit_modify_only_target_bits() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr(0xABCD_EF12);
    cpu.set_fpscr_field(3, 0x9);
    assert_eq!(cpu.fpscr(), 0xABC9_EF12);
    cpu.set_fpscr_bit(0, true);
    assert_eq!(cpu.fpscr(), 0xABC9_EF12);
    cpu.set_fpscr_bit(2, false);
    assert_eq!(cpu.fpscr(), 0x8BC9_EF12);
    assert!(cpu.fpscr_bit(0));
    assert!(!cpu.fpscr_bit(2));
}

#[test]
fn msr_fp_available_defaults_to_enabled_and_can_be_toggled() {
    let mut cpu = PpcCpu::new();
    assert_eq!(cpu.msr, PPC_MSR_FP_AVAILABLE_MASK);
    assert!(cpu.msr_bit(PPC_MSR_FP_AVAILABLE_BIT));
    assert!(cpu.msr_fp_available());

    cpu.set_msr_fp_available(false);
    assert_eq!(cpu.msr & PPC_MSR_FP_AVAILABLE_MASK, 0);
    assert!(!cpu.msr_bit(PPC_MSR_FP_AVAILABLE_BIT));
    assert!(!cpu.msr_fp_available());

    cpu.set_msr_bit(PPC_MSR_FP_AVAILABLE_BIT, true);
    assert_eq!(cpu.msr, PPC_MSR_FP_AVAILABLE_MASK);
    assert!(cpu.msr_fp_available());
}

#[test]
fn step_fp_instruction_with_msr_fp_clear_surfaces_unavailable_exception() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.fpr[1] = 1.5f64.to_bits();
    cpu.fpr[2] = 2.25f64.to_bits();
    cpu.fpr[3] = 0xDEAD_BEEF_CAFE_BABEu64;
    cpu.set_msr_fp_available(false);

    let result = cpu.step_instruction(a_form_fp(63, 3, 1, 2, 0, 21, false)); // fadd f3,f1,f2

    assert_eq!(
        result,
        PpcStepResult::Exception(PpcException::FloatingPointUnavailable)
    );
    assert_eq!(cpu.pc, 0x1000);
    assert_eq!(cpu.fpr[3], 0xDEAD_BEEF_CAFE_BABEu64);
    assert_eq!(cpu.fpscr(), 0);
}

#[test]
fn step_integer_instruction_with_msr_fp_clear_still_executes() {
    let mut cpu = PpcCpu::new();
    cpu.set_msr_fp_available(false);

    assert_eq!(
        cpu.step_instruction(d_form(14, 3, 0, 42)),
        PpcStepResult::Stepped
    );
    assert_eq!(cpu.gpr[3], 42);
    assert_eq!(cpu.pc, 4);
    assert!(!cpu.msr_fp_available());
}

// ----- Decoder + dispatcher tests -----

/// Build a D-form word from its fields. PowerPC bit numbering
/// is MSB=0; in host (LSB=0) bits this means OPCD goes in the
/// top 6, then RT, RA, then the 16-bit immediate.
fn d_form(opcd: u8, rt_or_rs: u8, ra: u8, imm: u16) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((rt_or_rs as u32 & 0x1F) << 21)
        | ((ra as u32 & 0x1F) << 16)
        | (imm as u32)
}

#[test]
fn decode_addi_extracts_d_form_fields() {
    // addi r3, r4, 0x1234 — OPCD=14, RT=3, RA=4, SI=0x1234
    let word = d_form(14, 3, 4, 0x1234);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Addi {
            rt: 3,
            ra: 4,
            si: 0x1234
        })
    );
}

#[test]
fn decode_addis_extracts_d_form_fields() {
    // addis r5, r0, -1 — OPCD=15, RT=5, RA=0, SI=0xFFFF (-1).
    let word = d_form(15, 5, 0, 0xFFFF);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Addis {
            rt: 5,
            ra: 0,
            si: -1
        })
    );
}

#[test]
fn decode_canonical_nop_word_decodes_to_ori() {
    // 0x60000000 = ori r0, r0, 0 — the canonical PowerPC nop
    // per ISA Book I §3.3.10.
    assert_eq!(
        decode(0x6000_0000),
        Ok(PpcInstr::Ori {
            ra: 0,
            rs: 0,
            ui: 0
        })
    );
}

#[test]
fn decode_unknown_opcode_returns_error() {
    // OPCD=0 is illegal/reserved per ISA Book I Table 12. The decoder
    // keeps returning a raw diagnostic error; execution classifies it
    // as an architected illegal-instruction exception.
    assert!(matches!(
        decode(0x0000_0000),
        Err(PpcDecodeError::UnsupportedPrimaryOpcode(0))
    ));
}

#[test]
fn decode_twi_extracts_to_ra_si() {
    let word = d_form(3, 31, 4, 0xFFFF);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Twi {
            to: 31,
            ra: 4,
            si: -1
        })
    );
}

#[test]
fn step_twi_traps_when_condition_true_and_leaves_pc() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[4] = 5;

    let res = cpu.step_instruction(d_form(3, 0x08, 4, 4));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::ProgramTrap {
            to: 0x08,
            left: 5,
            right: 4
        })
    );
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_twi_not_taken_advances_pc() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[4] = 5;

    let res = cpu.step_instruction(d_form(3, 0x10, 4, 4));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.pc, 0x104);
}

#[test]
fn decode_sc_and_step_surfaces_system_call_exception() {
    let word = 0x4400_0002;
    assert_eq!(decode(word), Ok(PpcInstr::Sc { lev: 0 }));

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x200;
    let res = cpu.step_instruction(word);

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::SystemCall { lev: 0 })
    );
    assert_eq!(cpu.pc, 0x200);
}

#[test]
fn step_addi_with_ra_eq_0_treats_operand_as_literal_zero() {
    // li r3, 42  ==  addi r3, 0, 42
    // Per §3.3.8, RA=0 means literal 0, NOT GPR0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[0] = 0xDEAD_BEEF; // would corrupt result if RA=0 read GPR0
    cpu.pc = 0x1000;
    let word = d_form(14, 3, 0, 42);
    let res = cpu.step_instruction(word);
    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[3], 42);
    assert_eq!(cpu.pc, 0x1004);
}

#[test]
fn step_addi_with_ra_nonzero_reads_gpr() {
    // addi r3, r4, 0x10 with GPR4 = 0x100 → GPR3 = 0x110.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x100;
    let word = d_form(14, 3, 4, 0x10);
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[3], 0x110);
}

#[test]
fn step_addi_sign_extends_negative_immediate() {
    // addi r3, r4, -8 → GPR3 = GPR4 - 8 (with wraparound).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let word = d_form(14, 3, 4, 0xFFF8); // -8 as i16
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[3], 0x0FF8);
}

#[test]
fn step_addis_shifts_immediate_left_16_then_extends() {
    // lis r5, 0x1234 == addis r5, 0, 0x1234 → GPR5 = 0x12340000.
    let mut cpu = PpcCpu::new();
    let word = d_form(15, 5, 0, 0x1234);
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[5], 0x1234_0000);
}

#[test]
fn decode_oris_extracts_d_form_fields() {
    let word = d_form(25, 3, 4, 0x1234);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Oris {
            ra: 4,
            rs: 3,
            ui: 0x1234
        })
    );
}

#[test]
fn decode_xori_extracts_d_form_fields() {
    let word = d_form(26, 5, 6, 0xFF00);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Xori {
            ra: 6,
            rs: 5,
            ui: 0xFF00
        })
    );
}

#[test]
fn decode_xoris_extracts_d_form_fields() {
    let word = d_form(27, 5, 6, 0xFFFF);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Xoris {
            ra: 6,
            rs: 5,
            ui: 0xFFFF
        })
    );
}

#[test]
fn decode_andi_dot_extracts_d_form_fields() {
    let word = d_form(28, 7, 8, 0x000F);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::AndiDot {
            ra: 8,
            rs: 7,
            ui: 0x000F
        })
    );
}

#[test]
fn decode_andis_dot_extracts_d_form_fields() {
    let word = d_form(29, 7, 8, 0x8000);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::AndisDot {
            ra: 8,
            rs: 7,
            ui: 0x8000
        })
    );
}

#[test]
fn step_oris_loads_high_half_of_immediate() {
    // oris RA, RS, UI sets RA = RS | (UI << 16). The
    // d_form helper takes (opcd, rt_or_rs, ra, imm), so the
    // second arg fills the RS slot and the third the RA slot.
    //
    //   r4 = 0x0000_BEEF
    //   oris r3, r4, 0xCAFE  →  r3 = 0xCAFE_BEEF
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x0000_BEEF;
    // RS=4, RA=3
    cpu.step_instruction(d_form(25, 4, 3, 0xCAFE));
    assert_eq!(cpu.gpr[3], 0xCAFE_BEEF);
}

#[test]
fn step_xori_zero_extends_immediate() {
    // xori r3, r4, 0xFF00 with GPR4 = 0xFFFF_FFFF →
    //   0xFFFF_FFFF ^ 0x0000_FF00 = 0xFFFF_00FF
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.step_instruction(d_form(26, 4, 3, 0xFF00));
    assert_eq!(cpu.gpr[3], 0xFFFF_00FF);
}

#[test]
fn step_xoris_shifts_immediate_left_16() {
    // xoris r3, r4, 0x8000 with GPR4 = 0 → 0x8000_0000
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0;
    cpu.step_instruction(d_form(27, 4, 3, 0x8000));
    assert_eq!(cpu.gpr[3], 0x8000_0000);
}

#[test]
fn step_andi_dot_masks_low_bits_and_sets_cr0() {
    // andi. r3, r4, 0xF with GPR4 = 0xCAFE_BABE → 0xE
    // (positive non-zero) → CR0.GT.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.step_instruction(d_form(28, 4, 3, 0xF));
    assert_eq!(cpu.gpr[3], 0xE);
    assert_eq!(cpu.cr_field(0), 0b0100);
}

#[test]
fn step_andi_dot_sets_eq_when_result_is_zero() {
    // andi. r3, r4, 0xF with GPR4 = 0xCAFE_BAB0 → 0x0 → EQ.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BAB0;
    cpu.step_instruction(d_form(28, 4, 3, 0xF));
    assert_eq!(cpu.gpr[3], 0);
    assert_eq!(cpu.cr_field(0), 0b0010);
}

#[test]
fn step_andis_dot_keeps_only_high_half_then_records_cr0() {
    // andis. r3, r4, 0xFF00 with GPR4 = 0xCAFE_BABE →
    //   0xCAFE_BABE & 0xFF00_0000 = 0xCA00_0000 (high bit set
    //   → signed-negative → CR0.LT).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.step_instruction(d_form(29, 4, 3, 0xFF00));
    assert_eq!(cpu.gpr[3], 0xCA00_0000);
    assert_eq!(cpu.cr_field(0), 0b1000);
}

#[test]
fn decode_slw_extracts_x_form_fields() {
    // slw r3, r4, r5 — RS=4, RA=3, RB=5, XO=24.
    let word = x_form(31, 4, 3, 5, 24, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Slw {
            ra: 3,
            rs: 4,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_srw_extracts_x_form_fields() {
    let word = x_form(31, 4, 3, 5, 536, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Srw {
            ra: 3,
            rs: 4,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_sraw_extracts_x_form_fields() {
    let word = x_form(31, 4, 3, 5, 792, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Sraw {
            ra: 3,
            rs: 4,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_srawi_extracts_x_form_fields() {
    // srawi r3, r4, 7 — XO=824, SH=7 in the RB slot.
    let word = x_form(31, 4, 3, 7, 824, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Srawi {
            ra: 3,
            rs: 4,
            sh: 7,
            rc: false
        })
    );
}

#[test]
fn step_slw_shifts_left_by_register() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x0000_BABE;
    cpu.gpr[5] = 8;
    cpu.step_instruction(x_form(31, 4, 3, 5, 24, false));
    assert_eq!(cpu.gpr[3], 0x00BA_BE00);
}

#[test]
fn step_slw_with_shift_geq_32_produces_zero() {
    // PowerPC behaviour for shift counts ≥ 32: result = 0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.gpr[5] = 32;
    cpu.step_instruction(x_form(31, 4, 3, 5, 24, false));
    assert_eq!(cpu.gpr[3], 0);
}

#[test]
fn step_srw_shifts_right_logical() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.gpr[5] = 4;
    cpu.step_instruction(x_form(31, 4, 3, 5, 536, false));
    assert_eq!(cpu.gpr[3], 0x0CAF_EBAB);
}

#[test]
fn step_srw_zero_fills_high_bits() {
    // Right shift of negative-pattern by 1 must zero-fill,
    // NOT sign-extend.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x8000_0000;
    cpu.gpr[5] = 1;
    cpu.step_instruction(x_form(31, 4, 3, 5, 536, false));
    assert_eq!(cpu.gpr[3], 0x4000_0000);
}

#[test]
fn step_sraw_sign_extends_on_right_shift() {
    // Right shift of 0x8000_0000 by 1 with sign extension →
    // 0xC000_0000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x8000_0000;
    cpu.gpr[5] = 1;
    cpu.step_instruction(x_form(31, 4, 3, 5, 792, false));
    assert_eq!(cpu.gpr[3], 0xC000_0000);
}

#[test]
fn step_sraw_sets_xer_ca_when_negative_and_bits_shifted_out() {
    // 0xFFFF_FFFE >> 1 (signed) = 0xFFFF_FFFF, low bit was 0,
    // so CA=0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFE;
    cpu.gpr[5] = 1;
    cpu.step_instruction(x_form(31, 4, 3, 5, 792, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
    assert_eq!((cpu.xer >> 29) & 1, 0);

    // 0xFFFF_FFFF >> 1 — now low bit is 1, so CA=1.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.gpr[5] = 1;
    cpu.step_instruction(x_form(31, 4, 3, 5, 792, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
    assert_eq!((cpu.xer >> 29) & 1, 1);
}

#[test]
fn step_sraw_with_shift_geq_32_fills_with_sign_bit() {
    // n >= 32: result is 32 sign bits, CA = sign bit.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x8000_0000;
    cpu.gpr[5] = 40;
    cpu.step_instruction(x_form(31, 4, 3, 5, 792, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
    assert_eq!((cpu.xer >> 29) & 1, 1);
}

#[test]
fn step_srawi_uses_5bit_immediate() {
    // srawi r3, r4, 8 with GPR4 = 0xFFFF_FF00 →
    //   0xFFFF_FFFF (sign-extended), CA = 0 (no 1-bits in low 8).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FF00;
    cpu.step_instruction(x_form(31, 4, 3, 8, 824, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
    assert_eq!((cpu.xer >> 29) & 1, 0);
}

#[test]
fn step_srawi_sets_ca_when_negative_with_bits_shifted_out() {
    // srawi r3, r4, 4 with GPR4 = 0xFFFF_FFFF → result is
    // 0xFFFF_FFFF, CA = 1 (negative + bits shifted out).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.step_instruction(x_form(31, 4, 3, 4, 824, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
    assert_eq!((cpu.xer >> 29) & 1, 1);
}

#[test]
fn step_srawi_zero_shift_clears_ca() {
    // srawi r3, r4, 0 → RA = RS, CA = 0 per §3.3.12.2.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.xer = 0xFFFF_FFFF; // pre-populated CA must clear
    cpu.step_instruction(x_form(31, 4, 3, 0, 824, false));
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);
    assert_eq!((cpu.xer >> 29) & 1, 0);
}

/// M-form word builder for `rlwinm`.
fn m_form_rlwinm(rs: u8, ra: u8, sh: u8, mb: u8, me: u8, rc: bool) -> u32 {
    (21u32 << 26)
        | ((rs as u32 & 0x1F) << 21)
        | ((ra as u32 & 0x1F) << 16)
        | ((sh as u32 & 0x1F) << 11)
        | ((mb as u32 & 0x1F) << 6)
        | ((me as u32 & 0x1F) << 1)
        | (if rc { 1 } else { 0 })
}

/// M-form word builder for `rlwimi` (OPCD = 20).
fn m_form_rlwimi(rs: u8, ra: u8, sh: u8, mb: u8, me: u8, rc: bool) -> u32 {
    (20u32 << 26)
        | ((rs as u32 & 0x1F) << 21)
        | ((ra as u32 & 0x1F) << 16)
        | ((sh as u32 & 0x1F) << 11)
        | ((mb as u32 & 0x1F) << 6)
        | ((me as u32 & 0x1F) << 1)
        | (if rc { 1 } else { 0 })
}

/// M-form word builder for `rlwnm` (OPCD = 23).
fn m_form_rlwnm(rs: u8, ra: u8, rb: u8, mb: u8, me: u8, rc: bool) -> u32 {
    (23u32 << 26)
        | ((rs as u32 & 0x1F) << 21)
        | ((ra as u32 & 0x1F) << 16)
        | ((rb as u32 & 0x1F) << 11)
        | ((mb as u32 & 0x1F) << 6)
        | ((me as u32 & 0x1F) << 1)
        | (if rc { 1 } else { 0 })
}

#[test]
fn decode_rlwimi_extracts_m_form_fields() {
    let word = m_form_rlwimi(3, 4, 5, 6, 7, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Rlwimi {
            ra: 4,
            rs: 3,
            sh: 5,
            mb: 6,
            me: 7,
            rc: false
        })
    );
}

#[test]
fn decode_rlwnm_extracts_m_form_fields() {
    let word = m_form_rlwnm(3, 4, 5, 6, 7, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Rlwnm {
            ra: 4,
            rs: 3,
            rb: 5,
            mb: 6,
            me: 7,
            rc: false
        })
    );
}

#[test]
fn step_rlwimi_inserts_field_preserving_bits_outside_mask() {
    // rlwimi r4, r3, 8, 16, 23
    //   pre: r3 = 0xCAFE_BABE, r4 = 0xAAAA_AAAA
    //   r' = ROTL32(0xCAFEBABE, 8) = 0xFEBABECA
    //   mask = MASK(16, 23) = 0x0000_FF00
    //   kept = r4 & ~mask  = 0xAAAA_00AA
    //   inserted = r' & mask = 0x0000_BE00
    //   result = 0xAAAA_BEAA
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0xAAAA_AAAA;
    cpu.step_instruction(m_form_rlwimi(3, 4, 8, 16, 23, false));
    assert_eq!(cpu.gpr[4], 0xAAAA_BEAA);
}

#[test]
fn step_rlwimi_implements_inslwi_pattern() {
    // inslwi r4, r3, 8, 16  (insert 8-bit field starting at
    // bit 16 of r4 from the high 8 bits of r3) ==
    // rlwimi r4, r3, 32-16, 16, 16+8-1 = rlwimi r4, r3, 16, 16, 23
    //
    // r3 = 0x000000AB → high 8 bits we want is "0xAB" stored
    // at bits 24..31. Rotate-left 16: 0x00AB_0000.
    // mask 16..23 = 0x0000_FF00. ANDed: 0x0000_0000 — no!
    //
    // Hmm — actually the inslwi extended mnemonic takes the
    // LEFT-justified field. Let me use a clearer example:
    //
    // r3 = 0xAB000000 (8 bits left-justified). rotate-left 16 →
    // 0x0000_AB00. mask 16..23 = 0x0000_FF00. result =
    // 0x0000_AB00. Combined with r4 = 0x12345678 → other bits
    // preserved. expect 0x1234_AB78.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xAB00_0000;
    cpu.gpr[4] = 0x1234_5678;
    cpu.step_instruction(m_form_rlwimi(3, 4, 16, 16, 23, false));
    assert_eq!(cpu.gpr[4], 0x1234_AB78);
}

#[test]
fn step_rlwnm_uses_rb_low_5_bits_as_rotate_amount() {
    // rlwnm r4, r3, r5, 0, 31  (no mask; pure rotate)
    //   r3 = 0x12345678, r5 = 4 → rotate-left 4
    //   result = 0x23456781
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1234_5678;
    cpu.gpr[5] = 4;
    cpu.step_instruction(m_form_rlwnm(3, 4, 5, 0, 31, false));
    assert_eq!(cpu.gpr[4], 0x2345_6781);
}

#[test]
fn step_rlwnm_masks_high_bits_of_rb() {
    // rlwnm uses only RB[27..31] as the rotate count. r5 =
    // 0x100 (bit 24 set) — only the low 5 bits matter, which
    // is 0, so result = unchanged.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1234_5678;
    cpu.gpr[5] = 0x100; // low 5 bits = 0
    cpu.step_instruction(m_form_rlwnm(3, 4, 5, 0, 31, false));
    assert_eq!(cpu.gpr[4], 0x1234_5678);
}

#[test]
fn decode_rlwinm_extracts_m_form_fields() {
    let word = m_form_rlwinm(3, 4, 5, 6, 7, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Rlwinm {
            ra: 4,
            rs: 3,
            sh: 5,
            mb: 6,
            me: 7,
            rc: false
        })
    );
}

#[test]
fn decode_rlwinm_dot_sets_rc_flag() {
    let word = m_form_rlwinm(3, 4, 5, 6, 7, true);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Rlwinm {
            ra: 4,
            rs: 3,
            sh: 5,
            mb: 6,
            me: 7,
            rc: true
        })
    );
}

#[test]
fn rlwinm_mask32_contiguous_msb0_range() {
    // MASK(0, 31) = all bits set.
    assert_eq!(PpcCpu::mask32(0, 31), 0xFFFF_FFFF);
    // MASK(0, 0) = single MSB bit (MSB=0 bit 0 = LSB-numbered 31).
    assert_eq!(PpcCpu::mask32(0, 0), 0x8000_0000);
    // MASK(31, 31) = LSB only.
    assert_eq!(PpcCpu::mask32(31, 31), 0x0000_0001);
    // MASK(8, 15) = the second-MSB byte.
    assert_eq!(PpcCpu::mask32(8, 15), 0x00FF_0000);
}

#[test]
fn rlwinm_mask32_wraps_when_mb_greater_than_me() {
    // MASK(28, 3) — wraps: bits 28..31 + 0..3 in MSB=0 =
    //   LSB-bits 0..3 + 28..31 = 0xF000_000F.
    assert_eq!(PpcCpu::mask32(28, 3), 0xF000_000F);
}

#[test]
fn step_rlwinm_implements_slwi_via_extended_mnemonic_pattern() {
    // slwi r3, r4, 4  ==  rlwinm r3, r4, 4, 0, 31-4 = rlwinm r3,r4,4,0,27
    // GPR4 = 0x0000_BABE → expected GPR3 = 0x000B_ABE0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x0000_BABE;
    // RS=4, RA=3, SH=4, MB=0, ME=27
    cpu.step_instruction(m_form_rlwinm(4, 3, 4, 0, 27, false));
    assert_eq!(cpu.gpr[3], 0x000B_ABE0);
}

#[test]
fn step_rlwinm_implements_srwi_via_extended_mnemonic_pattern() {
    // srwi r3, r4, 4  ==  rlwinm r3, r4, 32-4, 4, 31  =
    //                    rlwinm r3, r4, 28, 4, 31
    // GPR4 = 0xCAFE_BABE → expected GPR3 = 0x0CAF_EBAB.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.step_instruction(m_form_rlwinm(4, 3, 28, 4, 31, false));
    assert_eq!(cpu.gpr[3], 0x0CAF_EBAB);
}

#[test]
fn step_rlwinm_implements_clrlwi_n_clears_n_high_bits() {
    // clrlwi r3, r4, 8  ==  rlwinm r3, r4, 0, 8, 31
    // Clear top 8 bits.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.step_instruction(m_form_rlwinm(4, 3, 0, 8, 31, false));
    assert_eq!(cpu.gpr[3], 0x00FE_BABE);
}

#[test]
fn step_rlwinm_implements_clrrwi_n_clears_n_low_bits() {
    // clrrwi r3, r4, 8  ==  rlwinm r3, r4, 0, 0, 31-8 = 23
    // Clear low 8 bits.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.step_instruction(m_form_rlwinm(4, 3, 0, 0, 23, false));
    assert_eq!(cpu.gpr[3], 0xCAFE_BA00);
}

#[test]
fn step_rlwinm_implements_extlwi() {
    // extlwi r3, r4, 8, 16  == "extract 8 bits starting at bit
    // 16 of r4, left-justified into r3" == rlwinm r3,r4,16,0,7
    // GPR4 = 0xCAFE_BABE; bit 16..23 (MSB=0) = 0xBA → r3 high
    // byte = 0xBA, rest zero → 0xBA00_0000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.step_instruction(m_form_rlwinm(4, 3, 16, 0, 7, false));
    assert_eq!(cpu.gpr[3], 0xBA00_0000);
}

#[test]
fn step_rlwinm_dot_sets_cr0() {
    // rlwinm. r3, r4, 0, 0, 31 with GPR4 = 0 → r3=0 → CR0.EQ.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0;
    cpu.step_instruction(m_form_rlwinm(4, 3, 0, 0, 31, true));
    assert_eq!(cpu.gpr[3], 0);
    assert_eq!(cpu.cr_field(0), 0b0010);
}

#[test]
fn step_rlwinm_zero_extension_idiom() {
    // The standard "zero-extend low 16 bits" idiom:
    //   rlwinm r3, r4, 0, 16, 31    (mask 0x0000FFFF)
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xCAFE_BABE;
    cpu.step_instruction(m_form_rlwinm(4, 3, 0, 16, 31, false));
    assert_eq!(cpu.gpr[3], 0x0000_BABE);
}

#[test]
fn step_rlwinm_rotate_no_mask() {
    // rotlwi r3, r4, 4 == rlwinm r3, r4, 4, 0, 31
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1234_5678;
    cpu.step_instruction(m_form_rlwinm(4, 3, 4, 0, 31, false));
    // Rotate left 4: 0x2345_6781
    assert_eq!(cpu.gpr[3], 0x2345_6781);
}

#[test]
fn step_lis_ori_loads_full_32bit_constant() {
    // The canonical "load 32-bit constant" idiom:
    //   lis r3, 0xCAFE  ==  addis r3, 0, 0xCAFE
    //   ori r3, r3, 0xBABE
    // After these two: GPR3 = 0xCAFE_BABE.
    let mut cpu = PpcCpu::new();
    cpu.step_instruction(d_form(15, 3, 0, 0xCAFE)); // addis (lis)
    cpu.step_instruction(d_form(24, 3, 3, 0xBABE)); // ori
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);
}

#[test]
fn step_ori_acts_as_nop_when_all_zero() {
    // The canonical PowerPC nop (0x60000000): every
    // architectural register stays untouched except PC, which
    // advances by 4.
    let mut cpu = PpcCpu::new();
    cpu.gpr[7] = 0xCAFE_F00D;
    cpu.lr = 0x1234_5678;
    cpu.pc = 0x100;
    let res = cpu.step_instruction(0x6000_0000);
    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[7], 0xCAFE_F00D);
    assert_eq!(cpu.lr, 0x1234_5678);
    assert_eq!(cpu.pc, 0x104);
}

#[test]
fn step_ori_reads_rs_even_when_zero() {
    // ori r0, r0, 0xFF00 — unlike addi, ori does NOT have the
    // "RA=0 means literal 0" special case. With GPR0=0x12 the
    // result is GPR0 = 0x12 | 0xFF00 = 0xFF12.
    let mut cpu = PpcCpu::new();
    cpu.gpr[0] = 0x12;
    let word = d_form(24, 0, 0, 0xFF00); // RT=RS=0, RA=0
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[0], 0xFF12);
}

#[test]
fn step_reserved_opcode_surfaces_illegal_instruction() {
    // Word with OPCD=0 (reserved). step_instruction must surface an
    // illegal-instruction exception and leave registers / PC alone.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1234;
    cpu.pc = 0x1000;
    let res = cpu.step_instruction(0);
    assert_illegal_instruction(res, 0, PpcIllegalInstructionReason::ReservedOpcode);
    assert_eq!(cpu.gpr[3], 0x1234);
    assert_eq!(cpu.pc, 0x1000);
}

/// X-form word builder: OPCD | RS | RA | RB | XO | Rc.
fn x_form(opcd: u8, rs: u8, ra: u8, rb: u8, xo: u16, rc: bool) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((rs as u32 & 0x1F) << 21)
        | ((ra as u32 & 0x1F) << 16)
        | ((rb as u32 & 0x1F) << 11)
        | ((xo as u32 & 0x3FF) << 1)
        | (if rc { 1 } else { 0 })
}

#[test]
fn decode_cache_management_extracts_x_form_fields() {
    assert_eq!(
        decode(x_form(31, 0, 4, 5, 54, false)),
        Ok(PpcInstr::Dcbst { ra: 4, rb: 5 })
    );
    assert_eq!(
        decode(x_form(31, 0, 4, 5, 86, false)),
        Ok(PpcInstr::Dcbf { ra: 4, rb: 5 })
    );
    assert_eq!(
        decode(x_form(31, 7, 4, 5, 246, false)),
        Ok(PpcInstr::Dcbtst {
            ct: 7,
            ra: 4,
            rb: 5
        })
    );
    assert_eq!(
        decode(x_form(31, 6, 4, 5, 278, false)),
        Ok(PpcInstr::Dcbt {
            ct: 6,
            ra: 4,
            rb: 5
        })
    );
    assert_eq!(
        decode(x_form(31, 0, 4, 5, 982, false)),
        Ok(PpcInstr::Icbi { ra: 4, rb: 5 })
    );
    assert_eq!(
        decode(x_form(31, 0, 4, 5, 1014, false)),
        Ok(PpcInstr::Dcbz { ra: 4, rb: 5 })
    );
}

#[test]
fn decode_cache_management_rejects_reserved_field_forms() {
    let invalid_words = [
        x_form(31, 1, 4, 5, 54, false),
        x_form(31, 1, 4, 5, 86, false),
        x_form(31, 1, 4, 5, 982, false),
        x_form(31, 0, 4, 5, 54, true),
        x_form(31, 0, 4, 5, 86, true),
        x_form(31, 7, 4, 5, 246, true),
        x_form(31, 6, 4, 5, 278, true),
        x_form(31, 0, 4, 5, 982, true),
        x_form(31, 1, 4, 5, 1014, false),
        x_form(31, 0, 4, 5, 1014, true),
    ];

    for word in invalid_words {
        let secondary = ((word >> 1) & 0x3FF) as u16;
        assert_eq!(
            decode(word),
            Err(PpcDecodeError::UnsupportedSecondaryOpcode {
                primary: 31,
                secondary
            })
        );
    }
}

#[test]
fn step_dcbz_zeroes_32_byte_block_containing_effective_address() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.gpr[4] = 0x1010;
    cpu.gpr[5] = 0x13;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xAA; 64],
    };

    let res = cpu.step(&mut mem, x_form(31, 0, 4, 5, 1014, false));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.pc, 0x1004);
    assert_eq!(cpu.gpr[4], 0x1010);
    assert_eq!(cpu.gpr[5], 0x13);
    assert_eq!(&mem.data[0..32], &[0xAA; 32]);
    assert_eq!(&mem.data[32..64], &[0; 32]);
}

#[test]
fn step_dcbz_with_ra_eq_0_uses_literal_zero_base() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[0] = 0xDEAD_BEEF;
    cpu.gpr[5] = 0x1003;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xCC; 32],
    };

    let res = cpu.step(&mut mem, x_form(31, 0, 0, 5, 1014, false));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(mem.data, vec![0; 32]);
    assert_eq!(cpu.gpr[0], 0xDEAD_BEEF);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_dcbz_unmapped_block_start_surfaces_memory_fault() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x200;
    cpu.gpr[5] = 0x101F;
    let mut mem = VecMem {
        base: 0x1020,
        data: vec![0xDD; 32],
    };

    let res = cpu.step(&mut mem, x_form(31, 0, 0, 5, 1014, false));

    assert!(matches!(
        res,
        PpcStepResult::MemoryFault {
            addr: 0x1000,
            was_write: true
        }
    ));
    assert_eq!(mem.data, vec![0xDD; 32]);
    assert_eq!(cpu.pc, 0x200);
}

#[test]
fn step_cache_management_instructions_are_user_mode_noops() {
    let words = [
        x_form(31, 0, 4, 5, 54, false),
        x_form(31, 0, 4, 5, 86, false),
        x_form(31, 7, 4, 5, 246, false),
        x_form(31, 6, 4, 5, 278, false),
        x_form(31, 0, 4, 5, 982, false),
    ];

    for word in words {
        let mut cpu = PpcCpu::new();
        cpu.pc = 0x1000;
        cpu.lr = 0xCAFE_BABE;
        cpu.ctr = 0x1234_5678;
        cpu.xer = 0x2000_0000;
        cpu.cr = 0x9000_0000;
        cpu.gpr[4] = 0x2000;
        cpu.gpr[5] = 0x20;
        let gpr_before = cpu.gpr;

        assert_eq!(cpu.step_instruction(word), PpcStepResult::Stepped);
        assert_eq!(cpu.pc, 0x1004);
        assert_eq!(cpu.gpr, gpr_before);
        assert_eq!(cpu.lr, 0xCAFE_BABE);
        assert_eq!(cpu.ctr, 0x1234_5678);
        assert_eq!(cpu.xer, 0x2000_0000);
        assert_eq!(cpu.cr, 0x9000_0000);
    }
}

/// I-form word builder for branches.
fn i_form(displacement: i32, aa: bool, lk: bool) -> u32 {
    // displacement must be a multiple of 4 (low two bits are
    // AA/LK, taking the place of the implicit 0b00 suffix).
    debug_assert!(displacement & 0x3 == 0);
    let li_shifted_2 = (displacement as u32) & 0x03FF_FFFC;
    (18u32 << 26) | li_shifted_2 | (if aa { 0b10 } else { 0 }) | (if lk { 0b01 } else { 0 })
}

/// XL-form word builder for bclr/bcctr.
fn xl_form(opcd: u8, bo: u8, bi: u8, xo: u16, lk: bool) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((bo as u32 & 0x1F) << 21)
        | ((bi as u32 & 0x1F) << 16)
        | ((xo as u32 & 0x3FF) << 1)
        | (if lk { 1 } else { 0 })
}

#[test]
fn decode_or_extracts_x_form_fields() {
    // or r5, r3, r7 — OPCD=31, RS=3, RA=5, RB=7, XO=444, Rc=0.
    let word = x_form(31, 3, 5, 7, 444, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Or {
            ra: 5,
            rs: 3,
            rb: 7,
            rc: false
        })
    );
}

#[test]
fn decode_tw_extracts_to_ra_rb() {
    let word = x_form(31, 31, 4, 5, 4, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Tw {
            to: 31,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn step_tw_traps_on_unsigned_condition() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[4] = 1;
    cpu.gpr[5] = 0xFFFF_FFFF;

    let res = cpu.step_instruction(x_form(31, 0x02, 4, 5, 4, false));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::ProgramTrap {
            to: 0x02,
            left: 1,
            right: 0xFFFF_FFFF
        })
    );
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn decode_or_dot_sets_rc_bit() {
    // or. r5, r3, r7 — same as above but Rc=1.
    let word = x_form(31, 3, 5, 7, 444, true);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Or {
            ra: 5,
            rs: 3,
            rb: 7,
            rc: true
        })
    );
}

#[test]
fn decode_b_extracts_displacement_and_link_bit() {
    // bl +0x100 — OPCD=18, displacement=0x100, AA=0, LK=1.
    let word = i_form(0x100, false, true);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::B {
            displacement: 0x100,
            aa: false,
            lk: true
        })
    );
}

#[test]
fn decode_b_sign_extends_negative_displacement() {
    // b -0x1000 — backward branch.
    let word = i_form(-0x1000, false, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::B {
            displacement: -0x1000,
            aa: false,
            lk: false
        })
    );
}

#[test]
fn decode_blr_extracts_xl_form_fields() {
    // blr — bclr 20, 0 — OPCD=19, BO=20, BI=0, XO=16, LK=0.
    let word = xl_form(19, 20, 0, 16, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Bclr {
            bo: 20,
            bi: 0,
            lk: false
        })
    );
}

#[test]
fn decode_and_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 28, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::And {
            ra: 4,
            rs: 3,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_xor_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 316, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Xor {
            ra: 4,
            rs: 3,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_nor_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 124, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Nor {
            ra: 4,
            rs: 3,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_nand_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 476, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Nand {
            ra: 4,
            rs: 3,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_extsb_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 0, 954, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Extsb {
            ra: 4,
            rs: 3,
            rc: false
        })
    );
}

#[test]
fn decode_extsh_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 0, 922, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Extsh {
            ra: 4,
            rs: 3,
            rc: false
        })
    );
}

#[test]
fn decode_cntlzw_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 0, 26, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Cntlzw {
            ra: 4,
            rs: 3,
            rc: false
        })
    );
}

#[test]
fn decode_andc_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 60, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Andc {
            ra: 4,
            rs: 3,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_orc_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 412, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Orc {
            ra: 4,
            rs: 3,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_eqv_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 284, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Eqv {
            ra: 4,
            rs: 3,
            rb: 5,
            rc: false
        })
    );
}

/// XL-form word builder for CR-logical ops.
fn xl_cr_logical(bt: u8, ba: u8, bb: u8, xo: u16) -> u32 {
    (19u32 << 26)
        | ((bt as u32 & 0x1F) << 21)
        | ((ba as u32 & 0x1F) << 16)
        | ((bb as u32 & 0x1F) << 11)
        | ((xo as u32 & 0x3FF) << 1)
}

#[test]
fn decode_crand_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 257);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Crand {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn decode_cror_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 449);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Cror {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn decode_crxor_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 193);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Crxor {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn step_andc_masks_with_complement() {
    // andc r4, r3, r5 = r3 & ~r5
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_0000;
    cpu.gpr[5] = 0x00FF_FF00;
    cpu.step_instruction(x_form(31, 3, 4, 5, 60, false));
    // 0xFFFF0000 & ~0x00FFFF00 = 0xFFFF0000 & 0xFF0000FF = 0xFF000000
    assert_eq!(cpu.gpr[4], 0xFF00_0000);
}

#[test]
fn step_orc_or_with_complement() {
    // orc r4, r3, r5 = r3 | ~r5
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x0000_00FF;
    cpu.gpr[5] = 0xFFFF_FF00;
    cpu.step_instruction(x_form(31, 3, 4, 5, 412, false));
    // 0x000000FF | ~0xFFFFFF00 = 0x000000FF | 0x000000FF = 0x000000FF
    assert_eq!(cpu.gpr[4], 0x0000_00FF);
}

#[test]
fn step_eqv_xnor() {
    // eqv r4, r3, r5 = ~(r3 ^ r5)
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[5] = 0xCAFE_BABE;
    cpu.step_instruction(x_form(31, 3, 4, 5, 284, false));
    // ~(x ^ x) = ~0 = 0xFFFFFFFF
    assert_eq!(cpu.gpr[4], 0xFFFF_FFFF);
}

#[test]
fn step_crand_intersects_two_cr_bits() {
    // crand bt=2 (CR0.EQ), ba=0 (CR0.LT), bb=4 (CR1.LT).
    // Pre: CR0 has LT=1, EQ=0; CR1 has LT=1.
    let mut cpu = PpcCpu::new();
    cpu.set_cr_bit(0, true); // CR0.LT
    cpu.set_cr_bit(4, true); // CR1.LT
    cpu.step_instruction(xl_cr_logical(2, 0, 4, 257));
    assert!(cpu.cr_bit(2)); // EQ now set
}

#[test]
fn step_cror_acts_as_crmove_when_ba_eq_bb() {
    // crmove bt, ba == cror bt, ba, ba
    let mut cpu = PpcCpu::new();
    cpu.set_cr_bit(5, true); // CR1.GT
    cpu.step_instruction(xl_cr_logical(0, 5, 5, 449));
    // Bit 0 (CR0.LT) now copies bit 5.
    assert!(cpu.cr_bit(0));
}

#[test]
fn decode_crnand_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 225);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Crnand {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn decode_crnor_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 33);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Crnor {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn decode_creqv_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 289);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Creqv {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn decode_crandc_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 129);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Crandc {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn decode_crorc_extracts_xl_form_fields() {
    let word = xl_cr_logical(2, 0, 4, 417);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Crorc {
            bt: 2,
            ba: 0,
            bb: 4
        })
    );
}

#[test]
fn decode_mcrf_extracts_xl_form_fields() {
    // mcrf BF=2, BFA=5 — XO=0. Build the word manually.
    let bf = 2u32;
    let bfa = 5u32;
    let word = (19u32 << 26) | (bf << 23) | (bfa << 18);
    assert_eq!(decode(word), Ok(PpcInstr::Mcrf { bf: 2, bfa: 5 }));
}

#[test]
fn step_creqv_acts_as_crset_when_all_three_same() {
    // crset bt == creqv bt, bt, bt — always sets the bit.
    let mut cpu = PpcCpu::new();
    cpu.set_cr_bit(3, false); // pre-cleared
    cpu.step_instruction(xl_cr_logical(3, 3, 3, 289));
    assert!(cpu.cr_bit(3));
}

#[test]
fn step_crnor_with_ba_eq_bb_inverts_bit() {
    // crnot bt, ba == crnor bt, ba, ba — copy & invert.
    let mut cpu = PpcCpu::new();
    cpu.set_cr_bit(5, true);
    cpu.step_instruction(xl_cr_logical(0, 5, 5, 33));
    assert!(!cpu.cr_bit(0));
}

#[test]
fn step_crandc_clears_bit_when_complement_set() {
    // crandc bt, ba, bb = ba & ~bb. With ba=1, bb=1 → 0.
    let mut cpu = PpcCpu::new();
    cpu.set_cr_bit(4, true);
    cpu.set_cr_bit(8, true);
    cpu.step_instruction(xl_cr_logical(0, 4, 8, 129));
    assert!(!cpu.cr_bit(0));
    // With ba=1, bb=0 → 1.
    cpu.set_cr_bit(8, false);
    cpu.step_instruction(xl_cr_logical(1, 4, 8, 129));
    assert!(cpu.cr_bit(1));
}

#[test]
fn step_crnand_inverts_and() {
    let mut cpu = PpcCpu::new();
    cpu.set_cr_bit(0, true);
    cpu.set_cr_bit(1, true);
    cpu.step_instruction(xl_cr_logical(2, 0, 1, 225));
    assert!(!cpu.cr_bit(2)); // ~(1 & 1) = 0
    cpu.set_cr_bit(1, false);
    cpu.step_instruction(xl_cr_logical(2, 0, 1, 225));
    assert!(cpu.cr_bit(2)); // ~(1 & 0) = 1
}

#[test]
fn step_mcrf_copies_one_cr_field() {
    // CR1 = 0b0010 (EQ). mcrf 0, 1 → CR0 = 0b0010, others
    // untouched.
    let mut cpu = PpcCpu::new();
    cpu.cr = 0x0200_0000; // CR1 nibble = 0b0010, bit position 24..27
                          // Actually let me recompute: CR field 0 is bits 28..31
                          // (host bits) = high nibble of cr. CR field 1 is bits
                          // 24..27 = next nibble.
    cpu.cr = 0;
    cpu.set_cr_field(1, 0b0010);
    cpu.set_cr_field(7, 0b1111); // sentinel for "untouched fields preserved"
                                 // Build mcrf 0, 1: BF=0 (low 3 bits at host bits 23..25 stay
                                 // zero, so we can drop the explicit `(0 << 23)` term), BFA=1, XO=0.
    let word = (19u32 << 26) | (1u32 << 18);
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(0), 0b0010);
    assert_eq!(cpu.cr_field(1), 0b0010); // source unchanged
    assert_eq!(cpu.cr_field(7), 0b1111); // unrelated field preserved
}

#[test]
fn step_crxor_acts_as_crclr_when_all_three_same() {
    // crclr bt == crxor bt, bt, bt — always clears the bit.
    let mut cpu = PpcCpu::new();
    cpu.set_cr_bit(2, true); // CR0.EQ pre-populated
    cpu.step_instruction(xl_cr_logical(2, 2, 2, 193));
    assert!(!cpu.cr_bit(2));
}

#[test]
fn step_and_intersects_two_registers() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_0000;
    cpu.gpr[5] = 0x00FF_FF00;
    cpu.step_instruction(x_form(31, 3, 4, 5, 28, false));
    assert_eq!(cpu.gpr[4], 0x00FF_0000);
}

#[test]
fn step_xor_diff_two_registers() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[5] = 0xFFFF_FFFF;
    cpu.step_instruction(x_form(31, 3, 4, 5, 316, false));
    assert_eq!(cpu.gpr[4], 0x3501_4541);
}

#[test]
fn step_nor_acts_as_one_complement_when_rs_eq_rb() {
    // not r4, r3  ==  nor r4, r3, r3
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.step_instruction(x_form(31, 3, 4, 3, 124, false));
    assert_eq!(cpu.gpr[4], !0xCAFE_BABEu32);
}

#[test]
fn step_nand_inverts_and() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_0000;
    cpu.gpr[5] = 0xFFFF_FFFF;
    cpu.step_instruction(x_form(31, 3, 4, 5, 476, false));
    // ~(0xFFFF0000 & 0xFFFFFFFF) = ~0xFFFF0000 = 0x0000FFFF
    assert_eq!(cpu.gpr[4], 0x0000_FFFF);
}

#[test]
fn step_extsb_sign_extends_negative_byte() {
    // GPR3 = 0x000000FF (which is -1 as signed byte) →
    // GPR4 = 0xFFFFFFFF.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x0000_00FF;
    cpu.step_instruction(x_form(31, 3, 4, 0, 954, false));
    assert_eq!(cpu.gpr[4], 0xFFFF_FFFF);
}

#[test]
fn step_extsb_zero_extends_positive_byte() {
    // GPR3 = 0xCAFE_BA7F → low byte is 0x7F (positive
    // signed) → GPR4 = 0x0000_007F (high bits cleared).
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BA7F;
    cpu.step_instruction(x_form(31, 3, 4, 0, 954, false));
    assert_eq!(cpu.gpr[4], 0x0000_007F);
}

#[test]
fn step_extsh_sign_extends_negative_halfword() {
    // GPR3 = 0xCAFE_8000 → low halfword 0x8000 (-32768) →
    // GPR4 = 0xFFFF_8000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_8000;
    cpu.step_instruction(x_form(31, 3, 4, 0, 922, false));
    assert_eq!(cpu.gpr[4], 0xFFFF_8000);
}

#[test]
fn step_cntlzw_returns_count_for_high_bit_set() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x8000_0000;
    cpu.step_instruction(x_form(31, 3, 4, 0, 26, false));
    assert_eq!(cpu.gpr[4], 0);
}

#[test]
fn step_cntlzw_returns_32_for_zero() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0;
    cpu.step_instruction(x_form(31, 3, 4, 0, 26, false));
    assert_eq!(cpu.gpr[4], 32);
}

#[test]
fn step_cntlzw_intermediate_count() {
    // 0x0000_FFFF has 16 leading zeros.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x0000_FFFF;
    cpu.step_instruction(x_form(31, 3, 4, 0, 26, false));
    assert_eq!(cpu.gpr[4], 16);
}

#[test]
fn step_or_acts_as_register_move_when_rs_eq_rb() {
    // mr r5, r3 == or r5, r3, r3
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_F00D;
    cpu.pc = 0x100;
    let word = x_form(31, 3, 5, 3, 444, false);
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[5], 0xCAFE_F00D);
    assert_eq!(cpu.pc, 0x104);
}

#[test]
fn step_or_dot_with_zero_result_sets_eq_in_cr0() {
    // or. r5, r3, r4 with both source regs zero → result=0 →
    // CR0 nibble = 0b0010 (EQ).
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0;
    cpu.gpr[4] = 0;
    let word = x_form(31, 3, 5, 4, 444, true);
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[5], 0);
    assert_eq!(cpu.cr_field(0), 0b0010);
}

#[test]
fn step_or_dot_with_negative_result_sets_lt_in_cr0() {
    // or. r5, r3, r4 with high-bit-set result → CR0 nibble =
    // 0b1000 (LT).
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x8000_0000;
    cpu.gpr[4] = 0;
    let word = x_form(31, 3, 5, 4, 444, true);
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[5], 0x8000_0000);
    assert_eq!(cpu.cr_field(0), 0b1000);
}

#[test]
fn step_or_dot_with_positive_result_sets_gt_in_cr0() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x0000_0001;
    cpu.gpr[4] = 0;
    let word = x_form(31, 3, 5, 4, 444, true);
    cpu.step_instruction(word);
    assert_eq!(cpu.gpr[5], 0x1);
    assert_eq!(cpu.cr_field(0), 0b0100);
}

#[test]
fn step_b_relative_advances_pc_by_displacement() {
    // b +0x100 from PC=0x1000 → new PC = 0x1100.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.lr = 0xDEAD_BEEF; // must be untouched (LK=0)
    let word = i_form(0x100, false, false);
    cpu.step_instruction(word);
    assert_eq!(cpu.pc, 0x1100);
    assert_eq!(cpu.lr, 0xDEAD_BEEF);
}

#[test]
fn step_bl_saves_return_address_in_lr() {
    // bl +0x80 from PC=0x2000 — pc → 0x2080, lr → 0x2004.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x2000;
    let word = i_form(0x80, false, true);
    cpu.step_instruction(word);
    assert_eq!(cpu.pc, 0x2080);
    assert_eq!(cpu.lr, 0x2004);
}

#[test]
fn step_b_with_aa_branches_to_absolute_address() {
    // ba 0x10000 — aa=1, target ignores PC.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    let word = i_form(0x10000, true, false);
    cpu.step_instruction(word);
    assert_eq!(cpu.pc, 0x10000);
}

#[test]
fn step_b_negative_displacement_branches_backward() {
    // b -0x40 from PC=0x2000 → new PC = 0x1FC0.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x2000;
    let word = i_form(-0x40, false, false);
    cpu.step_instruction(word);
    assert_eq!(cpu.pc, 0x1FC0);
}

#[test]
fn step_blr_jumps_to_lr_word_aligned() {
    // blr — pc gets LR & ~0x3 (low 2 bits forced to zero).
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x4000;
    cpu.lr = 0x0001_2347; // low two bits set; must be cleared
    let word = xl_form(19, 20, 0, 16, false);
    cpu.step_instruction(word);
    assert_eq!(cpu.pc, 0x0001_2344);
}

#[test]
fn step_bclr_conditional_takes_branch_when_cr_bit_matches() {
    // beqlr (= bclr 12, 2) — branch to LR if CR0.EQ is set.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.lr = 0x4000;
    cpu.set_cr_field(0, 0b0010); // EQ
    let word = xl_form(19, 12, 2, 16, false);
    cpu.step_instruction(word);
    assert_eq!(cpu.pc, 0x4000);
}

#[test]
fn step_bclr_conditional_falls_through_when_cr_bit_clear() {
    // beqlr with CR0.EQ clear → fall through to PC+4.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.lr = 0x4000;
    cpu.set_cr_field(0, 0b1000); // LT, not EQ
    let word = xl_form(19, 12, 2, 16, false);
    cpu.step_instruction(word);
    assert_eq!(cpu.pc, 0x1004);
    // LR untouched (lk=0).
    assert_eq!(cpu.lr, 0x4000);
}

/// XFX-form word builder for mtspr / mfspr. `spr_decimal` is
/// the natural SPR number (1=XER, 8=LR, 9=CTR); the encoder
/// performs the high/low swap into instr bits 11..20.
fn xfx_form(opcd: u8, rt_or_rs: u8, spr_decimal: u16, xo: u16) -> u32 {
    let high_5 = (spr_decimal >> 5) & 0x1F;
    let low_5 = spr_decimal & 0x1F;
    ((opcd as u32 & 0x3F) << 26)
        | ((rt_or_rs as u32 & 0x1F) << 21)
        // low 5 bits of SPR → host bits 16..20 (instr MSB=0 16..20)
        | ((low_5 as u32) << 16)
        // high 5 bits of SPR → host bits 11..15
        | ((high_5 as u32) << 11)
        | ((xo as u32 & 0x3FF) << 1)
}

#[test]
fn decode_mtlr_extracts_spr_8_and_rs() {
    // `mtlr r0` is the canonical encoding 0x7C0803A6.
    let word = xfx_form(31, 0, 8, 467);
    assert_eq!(word, 0x7C08_03A6);
    assert_eq!(decode(word), Ok(PpcInstr::Mtspr { spr: 8, rs: 0 }));
}

#[test]
fn decode_mflr_extracts_spr_8_and_rt() {
    // `mflr r0` = 0x7C0802A6.
    let word = xfx_form(31, 0, 8, 339);
    assert_eq!(word, 0x7C08_02A6);
    assert_eq!(decode(word), Ok(PpcInstr::Mfspr { rt: 0, spr: 8 }));
}

#[test]
fn decode_mtctr_extracts_spr_9() {
    // `mtctr r3` — SPR=9.
    let word = xfx_form(31, 3, 9, 467);
    assert_eq!(decode(word), Ok(PpcInstr::Mtspr { spr: 9, rs: 3 }));
}

#[test]
fn decode_mfxer_extracts_spr_1() {
    let word = xfx_form(31, 4, 1, 339);
    assert_eq!(decode(word), Ok(PpcInstr::Mfspr { rt: 4, spr: 1 }));
}

#[test]
fn step_mtlr_writes_lr_from_gpr() {
    // mtlr r3 with GPR3 = 0x1000 → LR = 0x1000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    cpu.pc = 0x100;
    cpu.step_instruction(xfx_form(31, 3, 8, 467));
    assert_eq!(cpu.lr, 0x1000);
    assert_eq!(cpu.pc, 0x104);
}

#[test]
fn step_mflr_reads_lr_into_gpr() {
    // mflr r0 with LR = 0xCAFEBABE → GPR0 = 0xCAFEBABE.
    let mut cpu = PpcCpu::new();
    cpu.lr = 0xCAFE_BABE;
    cpu.step_instruction(xfx_form(31, 0, 8, 339));
    assert_eq!(cpu.gpr[0], 0xCAFE_BABE);
}

#[test]
fn step_mtctr_writes_ctr() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[5] = 0x1000;
    cpu.step_instruction(xfx_form(31, 5, 9, 467));
    assert_eq!(cpu.ctr, 0x1000);
}

#[test]
fn step_mfctr_reads_ctr() {
    let mut cpu = PpcCpu::new();
    cpu.ctr = 0xDEAD_BEEF;
    cpu.step_instruction(xfx_form(31, 7, 9, 339));
    assert_eq!(cpu.gpr[7], 0xDEAD_BEEF);
}

#[test]
fn step_mtspr_with_unrecognised_spr_returns_unimplemented() {
    // SPR=18 (data segment register) — not yet supported.
    // step_instruction must surface Unimplemented and leave
    // PC unchanged.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    let res = cpu.step_instruction(xfx_form(31, 0, 18, 467));
    assert!(matches!(res, PpcStepResult::Unimplemented(_)));
    assert_eq!(cpu.pc, 0x1000);
}

/// B-form word builder for `bc`. `displacement` must be a
/// multiple of 4 in [-32768, 32764] (16-bit BD || 0b00 range).
fn b_form(bo: u8, bi: u8, displacement: i32, aa: bool, lk: bool) -> u32 {
    debug_assert!(displacement & 0x3 == 0);
    debug_assert!((-32768..=32764).contains(&displacement));
    let bd_shifted_2 = (displacement as u32) & 0x0000_FFFC;
    (16u32 << 26)
        | ((bo as u32 & 0x1F) << 21)
        | ((bi as u32 & 0x1F) << 16)
        | bd_shifted_2
        | (if aa { 0b10 } else { 0 })
        | (if lk { 0b01 } else { 0 })
}

/// X-form word builder for compares — same shape as the
/// general X-form but the BF/L fields go into the
/// RT-shaped slot.
fn x_form_compare(opcd: u8, bf: u8, l: bool, ra: u8, rb: u8, xo: u16) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        // BF at MSB=0 6..8 → host 23..25
        | ((bf as u32 & 0x07) << 23)
        // L at MSB=0 10 → host 21
        | (if l { 1u32 << 21 } else { 0 })
        | ((ra as u32 & 0x1F) << 16)
        | ((rb as u32 & 0x1F) << 11)
        | ((xo as u32 & 0x3FF) << 1)
}

/// D-form word builder for compare-immediate — BF/L instead
/// of an RT slot.
fn d_form_compare(opcd: u8, bf: u8, l: bool, ra: u8, imm: u16) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((bf as u32 & 0x07) << 23)
        | (if l { 1u32 << 21 } else { 0 })
        | ((ra as u32 & 0x1F) << 16)
        | (imm as u32)
}

#[test]
fn decode_cmpi_extracts_d_form_fields() {
    // cmpwi cr0, r3, 5  ==  cmpi 0, 0, 3, 5
    let word = d_form_compare(11, 0, false, 3, 5);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Cmpi {
            bf: 0,
            l: false,
            ra: 3,
            si: 5
        })
    );
}

#[test]
fn decode_cmpli_extracts_d_form_fields() {
    let word = d_form_compare(10, 1, false, 4, 0xFF);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Cmpli {
            bf: 1,
            l: false,
            ra: 4,
            ui: 0xFF
        })
    );
}

#[test]
fn decode_cmp_extracts_x_form_fields() {
    // cmpw cr2, r3, r4  ==  cmp 2, 0, 3, 4 (XO=0)
    let word = x_form_compare(31, 2, false, 3, 4, 0);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Cmp {
            bf: 2,
            l: false,
            ra: 3,
            rb: 4
        })
    );
}

#[test]
fn decode_cmpl_extracts_x_form_fields() {
    let word = x_form_compare(31, 0, false, 3, 4, 32);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Cmpl {
            bf: 0,
            l: false,
            ra: 3,
            rb: 4
        })
    );
}

#[test]
fn decode_bc_extracts_b_form_fields() {
    // beq +0x10  ==  bc 12, 2, +0x10
    let word = b_form(12, 2, 0x10, false, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Bc {
            bo: 12,
            bi: 2,
            displacement: 0x10,
            aa: false,
            lk: false
        })
    );
}

#[test]
fn decode_bc_negative_displacement_sign_extended() {
    let word = b_form(16, 0, -0x100, false, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Bc {
            bo: 16,
            bi: 0,
            displacement: -0x100,
            aa: false,
            lk: false
        })
    );
}

#[test]
fn step_cmpi_writes_lt_when_ra_less_than_immediate() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 5;
    cpu.step_instruction(d_form_compare(11, 0, false, 3, 10));
    assert_eq!(cpu.cr_field(0), 0b1000);
}

#[test]
fn step_cmpi_writes_gt_when_ra_greater_than_immediate() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 100;
    cpu.step_instruction(d_form_compare(11, 0, false, 3, 10));
    assert_eq!(cpu.cr_field(0), 0b0100);
}

#[test]
fn step_cmpi_writes_eq_when_equal() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 42;
    cpu.step_instruction(d_form_compare(11, 0, false, 3, 42));
    assert_eq!(cpu.cr_field(0), 0b0010);
}

#[test]
fn step_cmpi_treats_immediate_as_signed() {
    // cmpwi cr0, r3, -1 with GPR3 = 1: signed comparison says
    // 1 > -1 → GT.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 1;
    cpu.step_instruction(d_form_compare(11, 0, false, 3, 0xFFFF));
    assert_eq!(cpu.cr_field(0), 0b0100);
}

#[test]
fn step_cmpli_treats_immediate_as_unsigned() {
    // cmplwi cr0, r3, 0xFFFF with GPR3 = 1: unsigned says
    // 1 < 65535 → LT.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 1;
    cpu.step_instruction(d_form_compare(10, 0, false, 3, 0xFFFF));
    assert_eq!(cpu.cr_field(0), 0b1000);
}

#[test]
fn step_cmpi_writes_to_chosen_cr_field() {
    // cmpwi cr3, r3, 5 — result lands in CR3, leaving CR0 zeroed.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 5;
    cpu.step_instruction(d_form_compare(11, 3, false, 3, 5));
    assert_eq!(cpu.cr_field(3), 0b0010);
    assert_eq!(cpu.cr_field(0), 0);
}

#[test]
fn step_cmp_signed_register_compare() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_FFFF; // -1 signed
    cpu.gpr[4] = 1;
    cpu.step_instruction(x_form_compare(31, 0, false, 3, 4, 0));
    // -1 < 1 (signed) → LT
    assert_eq!(cpu.cr_field(0), 0b1000);
}

#[test]
fn step_cmpl_unsigned_register_compare() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_FFFF; // ~4 billion unsigned
    cpu.gpr[4] = 1;
    cpu.step_instruction(x_form_compare(31, 0, false, 3, 4, 32));
    // 0xFFFFFFFF > 1 (unsigned) → GT
    assert_eq!(cpu.cr_field(0), 0b0100);
}

#[test]
fn step_cmpi_with_l_flag_surfaces_illegal_instruction_in_32bit_mode() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let word = d_form_compare(11, 0, true, 3, 0);
    let res = cpu.step_instruction(word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_cmpli_with_l_flag_surfaces_illegal_instruction_in_32bit_mode() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let word = d_form_compare(10, 0, true, 3, 0);
    let res = cpu.step_instruction(word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_cmp_with_l_flag_surfaces_illegal_instruction_in_32bit_mode() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let word = x_form_compare(31, 0, true, 3, 4, 0);
    let res = cpu.step_instruction(word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_cmpl_with_l_flag_surfaces_illegal_instruction_in_32bit_mode() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let word = x_form_compare(31, 0, true, 3, 4, 32);
    let res = cpu.step_instruction(word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_bc_branch_always_takes_branch() {
    // bc 20, 0, +0x100 — BO=20 (0b10100) is "branch always".
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.step_instruction(b_form(20, 0, 0x100, false, false));
    assert_eq!(cpu.pc, 0x1100);
}

#[test]
fn step_bc_beq_taken_when_eq_set() {
    // beq +0x40 == bc 12, 2, +0x40. Set CR0.EQ = 1 first.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.set_cr_field(0, 0b0010); // EQ
    cpu.step_instruction(b_form(12, 2, 0x40, false, false));
    assert_eq!(cpu.pc, 0x1040);
}

#[test]
fn step_bc_beq_not_taken_when_eq_clear() {
    // Same encoding, but CR0.EQ = 0 → fall through.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.set_cr_field(0, 0b1000); // LT, not EQ
    cpu.step_instruction(b_form(12, 2, 0x40, false, false));
    assert_eq!(cpu.pc, 0x1004);
}

#[test]
fn step_bc_bne_taken_when_eq_clear() {
    // bne +0x40 == bc 4, 2, +0x40 (BO=4: branch if CRBI=0).
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.set_cr_field(0, 0b0100); // GT, not EQ
    cpu.step_instruction(b_form(4, 2, 0x40, false, false));
    assert_eq!(cpu.pc, 0x1040);
}

#[test]
fn step_bc_bdnz_decrements_ctr_and_branches_when_nonzero() {
    // bdnz +0x10 == bc 16, 0, +0x10 (BO=0b10000: dec CTR,
    // branch if CTR != 0, ignore CR).
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.ctr = 3;
    cpu.step_instruction(b_form(16, 0, 0x10, false, false));
    assert_eq!(cpu.ctr, 2);
    assert_eq!(cpu.pc, 0x1010);
}

#[test]
fn step_bc_bdnz_falls_through_when_ctr_decremented_to_zero() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.ctr = 1;
    cpu.step_instruction(b_form(16, 0, 0x10, false, false));
    assert_eq!(cpu.ctr, 0);
    assert_eq!(cpu.pc, 0x1004);
}

#[test]
fn step_compare_then_branch_round_trip() {
    // Real "if r3 == 0 then jump" idiom:
    //   cmpwi cr0, r3, 0
    //   beq   +0x40
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.gpr[3] = 0;
    // cmpwi cr0, r3, 0
    cpu.step_instruction(d_form_compare(11, 0, false, 3, 0));
    assert_eq!(cpu.cr_field(0), 0b0010); // EQ
    assert_eq!(cpu.pc, 0x1004);
    // beq +0x40
    cpu.step_instruction(b_form(12, 2, 0x40, false, false));
    assert_eq!(cpu.pc, 0x1044);
}

/// XO-form word builder. `oe` is the bit-21 (MSB=0) flag;
/// `xo_9bit` is the 9-bit XO occupying bits 22..30. The
/// caller passes the natural 9-bit XO, e.g. 266 for `add`.
fn xo_form(opcd: u8, rt: u8, ra: u8, rb: u8, oe: bool, xo_9bit: u16, rc: bool) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((rt as u32 & 0x1F) << 21)
        | ((ra as u32 & 0x1F) << 16)
        | ((rb as u32 & 0x1F) << 11)
        | (if oe { 0x400 } else { 0 })   // bit 10 of host (MSB=0 bit 21)
        | ((xo_9bit as u32 & 0x1FF) << 1)
        | (if rc { 1 } else { 0 })
}

#[test]
fn decode_mulli_extracts_d_form_fields() {
    // mulli r3, r4, 7 — OPCD=7, RT=3, RA=4, SI=7.
    let word = d_form(7, 3, 4, 7);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Mulli {
            rt: 3,
            ra: 4,
            si: 7
        })
    );
}

#[test]
fn decode_add_extracts_xo_form_fields() {
    // add r5, r3, r4 — OPCD=31, RT=5, RA=3, RB=4, XO=266.
    let word = xo_form(31, 5, 3, 4, false, 266, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Add {
            rt: 5,
            ra: 3,
            rb: 4,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_add_with_oe_flag() {
    // addo r5, r3, r4 — OE=1 → 10-bit dispatch value 778.
    let word = xo_form(31, 5, 3, 4, true, 266, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Add {
            rt: 5,
            ra: 3,
            rb: 4,
            oe: true,
            rc: false
        })
    );
}

#[test]
fn decode_add_dot_with_rc_flag() {
    // add. r5, r3, r4 — Rc=1.
    let word = xo_form(31, 5, 3, 4, false, 266, true);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Add {
            rt: 5,
            ra: 3,
            rb: 4,
            oe: false,
            rc: true
        })
    );
}

#[test]
fn decode_subf_extracts_xo_form_fields() {
    let word = xo_form(31, 5, 3, 4, false, 40, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Subf {
            rt: 5,
            ra: 3,
            rb: 4,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn step_add_sums_two_registers() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x100;
    cpu.gpr[4] = 0x200;
    cpu.step_instruction(xo_form(31, 5, 3, 4, false, 266, false));
    assert_eq!(cpu.gpr[5], 0x300);
}

#[test]
fn step_add_wraps_on_overflow() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_FFFF;
    cpu.gpr[4] = 1;
    cpu.step_instruction(xo_form(31, 5, 3, 4, false, 266, false));
    assert_eq!(cpu.gpr[5], 0);
}

#[test]
fn step_add_dot_sets_cr0_from_signed_result() {
    // add. r5, r3, r4 with sum = 0 → CR0 EQ bit set.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1;
    cpu.gpr[4] = 0xFFFF_FFFF; // -1
    cpu.step_instruction(xo_form(31, 5, 3, 4, false, 266, true));
    assert_eq!(cpu.gpr[5], 0);
    assert_eq!(cpu.cr_field(0), 0b0010);
}

#[test]
fn step_addo_sets_ov_and_sticky_so_on_signed_overflow() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x7fff_ffff;
    cpu.gpr[4] = 1;
    let res = cpu.step_instruction(xo_form(31, 5, 3, 4, true, 266, true));
    assert!(matches!(res, PpcStepResult::Stepped));
    assert_eq!(cpu.gpr[5], 0x8000_0000);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
    assert_eq!(cpu.cr_field(0), 0b1001);
}

#[test]
fn step_addo_clears_ov_without_clearing_sticky_so_when_no_overflow() {
    let mut cpu = PpcCpu::new();
    cpu.xer = 0xc000_0000;
    cpu.gpr[3] = 1;
    cpu.gpr[4] = 2;
    cpu.step_instruction(xo_form(31, 5, 3, 4, true, 266, false));
    assert_eq!(cpu.gpr[5], 3);
    assert!(!cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn decode_neg_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 0, false, 104, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Neg {
            rt: 3,
            ra: 4,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_mullw_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 235, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Mullw {
            rt: 3,
            ra: 4,
            rb: 5,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_mulhw_extracts_xo_form_fields() {
    // mulhw has no OE form — its 9-bit XO=75 lives at the
    // low 9 bits of the 10-bit dispatch value (bit 21 / OE
    // is reserved zero).
    let word = xo_form(31, 3, 4, 5, false, 75, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Mulhw {
            rt: 3,
            ra: 4,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_mulhwu_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 11, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Mulhwu {
            rt: 3,
            ra: 4,
            rb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_divw_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 491, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Divw {
            rt: 3,
            ra: 4,
            rb: 5,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_divwu_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 459, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Divwu {
            rt: 3,
            ra: 4,
            rb: 5,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_addic_extracts_d_form_fields() {
    let word = d_form(12, 3, 4, 0x1234);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Addic {
            rt: 3,
            ra: 4,
            si: 0x1234
        })
    );
}

#[test]
fn decode_addic_dot_extracts_d_form_fields() {
    let word = d_form(13, 3, 4, 0x1234);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::AddicDot {
            rt: 3,
            ra: 4,
            si: 0x1234
        })
    );
}

#[test]
fn decode_subfic_extracts_d_form_fields() {
    let word = d_form(8, 3, 4, 0xFFFF);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Subfic {
            rt: 3,
            ra: 4,
            si: -1
        })
    );
}

#[test]
fn decode_addc_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 10, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Addc {
            rt: 3,
            ra: 4,
            rb: 5,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_adde_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 138, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Adde {
            rt: 3,
            ra: 4,
            rb: 5,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_subfc_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 8, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Subfc {
            rt: 3,
            ra: 4,
            rb: 5,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn decode_subfe_extracts_xo_form_fields() {
    let word = xo_form(31, 3, 4, 5, false, 136, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Subfe {
            rt: 3,
            ra: 4,
            rb: 5,
            oe: false,
            rc: false
        })
    );
}

#[test]
fn step_addic_clears_ca_on_no_carry() {
    // 0x100 + 0x200 = 0x300, no carry-out → CA = 0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x100;
    cpu.xer = 0xFFFF_FFFF; // pre-populated CA must clear
    cpu.step_instruction(d_form(12, 3, 4, 0x200));
    assert_eq!(cpu.gpr[3], 0x300);
    assert!(!cpu.xer_ca());
}

#[test]
fn step_addic_sets_ca_on_carry() {
    // 0xFFFF_FFFF + 1 = 0 with carry-out.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.step_instruction(d_form(12, 3, 4, 1));
    assert_eq!(cpu.gpr[3], 0);
    assert!(cpu.xer_ca());
}

#[test]
fn step_addic_dot_also_writes_cr0() {
    // 0xFFFF_FFFF + 1 → result 0 → CA=1 AND CR0.EQ=1.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.step_instruction(d_form(13, 3, 4, 1));
    assert_eq!(cpu.gpr[3], 0);
    assert!(cpu.xer_ca());
    assert_eq!(cpu.cr_field(0), 0b0010); // EQ
}

#[test]
fn step_subfic_subtracts_ra_from_immediate() {
    // subfic r3, r4, 0  with GPR4=5 → r3 = 0 - 5 = -5
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 5;
    cpu.step_instruction(d_form(8, 3, 4, 0));
    assert_eq!(cpu.gpr[3], (-5i32) as u32);
}

#[test]
fn step_addze_propagates_carry_in_with_zero_addend() {
    // addze r3, r4 with GPR4 = 5, CA = 1 → r3 = 6.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 5;
    cpu.set_xer_ca(true);
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 202, false));
    assert_eq!(cpu.gpr[3], 6);
}

#[test]
fn step_addze_with_no_carry_in_just_copies() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 5;
    cpu.set_xer_ca(false);
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 202, false));
    assert_eq!(cpu.gpr[3], 5);
}

#[test]
fn step_addzeo_sets_ov_when_carry_overflows_signed_result() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x7fff_ffff;
    cpu.set_xer_ca(true);
    cpu.step_instruction(xo_form(31, 3, 4, 0, true, 202, false));
    assert_eq!(cpu.gpr[3], 0x8000_0000);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_addme_subtracts_one_with_carry() {
    // addme r3, r4 = r4 + (-1) + CA.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 5;
    cpu.set_xer_ca(true);
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 234, false));
    assert_eq!(cpu.gpr[3], 5); // 5 + (-1) + 1 = 5
    cpu.gpr[4] = 5;
    cpu.set_xer_ca(false);
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 234, false));
    assert_eq!(cpu.gpr[3], 4); // 5 + (-1) + 0 = 4
}

#[test]
fn step_subfze_with_zero_input_and_carry_in() {
    // subfze r3, r4 = ~r4 + 0 + CA. With r4=0, CA=1 →
    //   ~0 + 0 + 1 = 0xFFFFFFFF + 1 = 0 (carry-out).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0;
    cpu.set_xer_ca(true);
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 200, false));
    assert_eq!(cpu.gpr[3], 0);
    assert!(cpu.xer_ca());
}

#[test]
fn step_subfzeo_sets_ov_for_negating_int_min() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x8000_0000;
    cpu.set_xer_ca(true);
    cpu.step_instruction(xo_form(31, 3, 4, 0, true, 200, false));
    assert_eq!(cpu.gpr[3], 0x8000_0000);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_subfme_with_zero_input_and_carry_in() {
    // subfme r3, r4 = ~r4 + (-1) + CA. With r4=0, CA=1:
    //   ~0 + 0xFFFFFFFF + 1 = 0xFFFFFFFF (carry-out).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0;
    cpu.set_xer_ca(true);
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 232, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
    assert!(cpu.xer_ca());
}

#[test]
fn step_addc_sets_ca_on_unsigned_overflow() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.gpr[5] = 1;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 10, false));
    assert_eq!(cpu.gpr[3], 0);
    assert!(cpu.xer_ca());
}

#[test]
fn step_addco_updates_ca_and_signed_overflow_independently() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x7fff_ffff;
    cpu.gpr[5] = 1;
    cpu.step_instruction(xo_form(31, 3, 4, 5, true, 10, false));
    assert_eq!(cpu.gpr[3], 0x8000_0000);
    assert!(!cpu.xer_ca());
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_adde_uses_carry_in_from_xer() {
    // GPR4 = 0xFFFFFFFE, GPR5 = 0, CA = 1 → result = 0xFFFFFFFF
    // (no carry-out).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFE;
    cpu.gpr[5] = 0;
    cpu.set_xer_ca(true);
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 138, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
    assert!(!cpu.xer_ca());
}

#[test]
fn step_addc_adde_compose_for_64bit_add() {
    // Real multi-precision add idiom:
    //   addc r5, r3, r7   ; low half: r5 = r3 + r7, sets CA
    //   adde r6, r4, r8   ; high half: r6 = r4 + r8 + CA
    //
    // Compute (r4:r3) + (r8:r7) where (r4:r3) = 0x00000001_FFFFFFFF
    //                                 (r8:r7) = 0x00000002_00000001
    //                                 expected: 0x00000004_00000000
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_FFFF; // low of operand A
    cpu.gpr[4] = 1; // high of operand A
    cpu.gpr[7] = 1; // low of operand B
    cpu.gpr[8] = 2; // high of operand B
                    // addc r5, r3, r7
    cpu.step_instruction(xo_form(31, 5, 3, 7, false, 10, false));
    // adde r6, r4, r8
    cpu.step_instruction(xo_form(31, 6, 4, 8, false, 138, false));
    assert_eq!(cpu.gpr[5], 0); // low half wrapped
    assert_eq!(cpu.gpr[6], 4); // high half = 1 + 2 + 1 (carry-in) = 4
}

#[test]
fn step_subfc_subtracts_with_borrow() {
    // subfc r3, r4, r5 with GPR4=10, GPR5=3 → r3 = 3 - 10 = -7,
    // CA = 0 (borrow).
    // Per PowerPC convention: subtraction uses ~RA + RB + 1.
    // CA = 1 means "no borrow" (the unsigned subtraction
    // didn't underflow); CA = 0 means borrow occurred.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 10;
    cpu.gpr[5] = 3;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 8, false));
    assert_eq!(cpu.gpr[3], (-7i32) as u32);
    // 3 - 10 borrows → CA cleared.
    assert!(!cpu.xer_ca());
}

#[test]
fn step_subfc_subfe_compose_for_64bit_subtract() {
    // (r4:r3) - (r8:r7) = (0x00000004_00000000) - (0x00000002_00000001)
    //                   = 0x00000001_FFFFFFFF
    //   subfc r5, r7, r3   ; low: r3 - r7  (subfc swaps! ~RA+RB+1)
    //   subfe r6, r8, r4   ; high: r4 - r8 - !CA
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x0000_0000; // low of A
    cpu.gpr[4] = 0x0000_0004; // high of A
    cpu.gpr[7] = 0x0000_0001; // low of B
    cpu.gpr[8] = 0x0000_0002; // high of B
                              // subfc r5, r7, r3   (computes r3 - r7 = -1 → 0xFFFFFFFF, borrow)
    cpu.step_instruction(xo_form(31, 5, 7, 3, false, 8, false));
    // subfe r6, r8, r4   (computes r4 - r8 - 1 = 4 - 2 - 1 = 1)
    cpu.step_instruction(xo_form(31, 6, 8, 4, false, 136, false));
    assert_eq!(cpu.gpr[5], 0xFFFF_FFFF);
    assert_eq!(cpu.gpr[6], 1);
}

#[test]
fn step_subfo_sets_ov_and_so_on_signed_overflow() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 1;
    cpu.gpr[4] = 0x8000_0000;
    cpu.step_instruction(xo_form(31, 5, 3, 4, true, 40, true));
    assert_eq!(cpu.gpr[5], 0x7fff_ffff);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
    assert_eq!(cpu.cr_field(0), 0b0101);
}

#[test]
fn step_subfco_keeps_ca_no_borrow_while_setting_signed_overflow() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xffff_ffff; // RA = -1
    cpu.gpr[5] = 0x7fff_ffff; // RB = i32::MAX
    cpu.step_instruction(xo_form(31, 3, 4, 5, true, 8, false));
    assert_eq!(cpu.gpr[3], 0x8000_0000);
    assert!(!cpu.xer_ca());
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_neg_two_complements() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 5;
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 104, false));
    assert_eq!(cpu.gpr[3], (-5i32) as u32);
}

#[test]
fn step_neg_of_int_min_yields_int_min() {
    // Per spec: if RA = i32::MIN, RT = i32::MIN (overflow
    // wraps when OE=0). i32::MIN is its own negation.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x8000_0000;
    cpu.step_instruction(xo_form(31, 3, 4, 0, false, 104, false));
    assert_eq!(cpu.gpr[3], 0x8000_0000);
}

#[test]
fn step_nego_sets_ov_for_int_min() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x8000_0000;
    cpu.step_instruction(xo_form(31, 3, 4, 0, true, 104, false));
    assert_eq!(cpu.gpr[3], 0x8000_0000);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_mullw_signed_low_32_bits() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 7;
    cpu.gpr[5] = 6;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 235, false));
    assert_eq!(cpu.gpr[3], 42);
}

#[test]
fn step_mullw_truncates_to_low_32_bits() {
    // 0x10000 * 0x10000 = 0x1_00000000 → low 32 = 0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x10000;
    cpu.gpr[5] = 0x10000;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 235, false));
    assert_eq!(cpu.gpr[3], 0);
}

#[test]
fn step_mullwo_sets_ov_when_product_does_not_fit_i32() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x10000;
    cpu.gpr[5] = 0x10000;
    cpu.step_instruction(xo_form(31, 3, 4, 5, true, 235, false));
    assert_eq!(cpu.gpr[3], 0);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_mulhw_returns_high_32_of_signed_product() {
    // 0x10000 * 0x10000 = 0x1_00000000 → high 32 = 1.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x10000;
    cpu.gpr[5] = 0x10000;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 75, false));
    assert_eq!(cpu.gpr[3], 1);
}

#[test]
fn step_mulhw_signed_negative_product_uses_signed_high() {
    // (-1) * 1 = -1 = 0xFFFFFFFF_FFFFFFFF as i64. high 32 =
    // 0xFFFFFFFF.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF; // -1 signed
    cpu.gpr[5] = 1;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 75, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
}

#[test]
fn step_mulhwu_treats_operands_as_unsigned() {
    // 0xFFFF_FFFF * 1 unsigned = 0x00000000_FFFFFFFF; high 32 = 0.
    // Compare with mulhw: signed gives 0xFFFFFFFF.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.gpr[5] = 1;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 11, false));
    assert_eq!(cpu.gpr[3], 0);
}

#[test]
fn step_divw_signed_division() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF; // -1
    cpu.gpr[5] = 1;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 491, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF); // -1 / 1 = -1
}

#[test]
fn step_divw_truncates_toward_zero() {
    // 7 / 2 = 3, NOT 3.5. PowerPC integer divide truncates.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 7;
    cpu.gpr[5] = 2;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 491, false));
    assert_eq!(cpu.gpr[3], 3);
    // -7 / 2 = -3 (truncates toward zero, NOT floor).
    cpu.gpr[4] = (-7i32) as u32;
    cpu.gpr[5] = 2;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 491, false));
    assert_eq!(cpu.gpr[3], (-3i32) as u32);
}

#[test]
fn step_divw_zero_divisor_does_not_panic() {
    // Per spec: divide-by-zero produces undefined RT. We
    // produce 0 rather than panicking on Rust's i32 / 0 UB.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 42;
    cpu.gpr[5] = 0;
    let res = cpu.step_instruction(xo_form(31, 3, 4, 5, false, 491, false));
    assert!(matches!(res, PpcStepResult::Stepped));
    assert_eq!(cpu.gpr[3], 0);
}

#[test]
fn step_divwo_sets_ov_on_zero_divisor() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 42;
    cpu.gpr[5] = 0;
    let res = cpu.step_instruction(xo_form(31, 3, 4, 5, true, 491, false));
    assert!(matches!(res, PpcStepResult::Stepped));
    assert_eq!(cpu.gpr[3], 0);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_divw_int_min_over_neg_one_does_not_panic() {
    // Per spec: 0x80000000 / -1 → undefined; we produce 0
    // (Rust's i32::MIN / -1 panics on overflow).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x8000_0000;
    cpu.gpr[5] = (-1i32) as u32;
    let res = cpu.step_instruction(xo_form(31, 3, 4, 5, false, 491, false));
    assert!(matches!(res, PpcStepResult::Stepped));
    assert_eq!(cpu.gpr[3], 0);
}

#[test]
fn step_divwu_unsigned_division() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0xFFFF_FFFF;
    cpu.gpr[5] = 2;
    cpu.step_instruction(xo_form(31, 3, 4, 5, false, 459, false));
    assert_eq!(cpu.gpr[3], 0x7FFF_FFFF);
}

#[test]
fn step_divwu_zero_divisor_does_not_panic() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 42;
    cpu.gpr[5] = 0;
    let res = cpu.step_instruction(xo_form(31, 3, 4, 5, false, 459, false));
    assert!(matches!(res, PpcStepResult::Stepped));
    assert_eq!(cpu.gpr[3], 0);
}

#[test]
fn step_divwuo_sets_ov_on_zero_divisor() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 42;
    cpu.gpr[5] = 0;
    let res = cpu.step_instruction(xo_form(31, 3, 4, 5, true, 459, false));
    assert!(matches!(res, PpcStepResult::Stepped));
    assert_eq!(cpu.gpr[3], 0);
    assert!(cpu.xer_ov());
    assert!(cpu.xer_so());
}

#[test]
fn step_subf_subtracts_ra_from_rb() {
    // subf r5, r3, r4 = GPR4 - GPR3.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x10;
    cpu.gpr[4] = 0x100;
    cpu.step_instruction(xo_form(31, 5, 3, 4, false, 40, false));
    assert_eq!(cpu.gpr[5], 0xF0);
}

#[test]
fn step_subf_with_rb_less_than_ra_wraps() {
    // subf r5, r3, r4 with GPR4 < GPR3 → unsigned wraparound.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x100;
    cpu.gpr[4] = 0x10;
    cpu.step_instruction(xo_form(31, 5, 3, 4, false, 40, false));
    assert_eq!(cpu.gpr[5], 0xFFFF_FF10);
}

#[test]
fn step_mulli_signed_multiplies_with_immediate() {
    // mulli r3, r4, 7 with GPR4 = 5 → GPR3 = 35.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 5;
    cpu.step_instruction(d_form(7, 3, 4, 7));
    assert_eq!(cpu.gpr[3], 35);
}

#[test]
fn step_mulli_handles_signed_negative_immediate() {
    // mulli r3, r4, -3 with GPR4 = 4 → GPR3 = -12 (0xFFFFFFF4).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 4;
    cpu.step_instruction(d_form(7, 3, 4, 0xFFFD)); // -3 as i16
    assert_eq!(cpu.gpr[3], (-12i32) as u32);
}

#[test]
fn step_mulli_truncates_to_low_32_bits() {
    // 0x80000 * 0x4000 = 0x2_00000000 → low 32 bits = 0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x80000;
    cpu.step_instruction(d_form(7, 3, 4, 0x4000)); // SI=0x4000=16384
    assert_eq!(cpu.gpr[3], 0);
}

/// Simple `Vec<u8>`-backed memory for the load/store tests.
/// Maps `[base .. base + data.len()]` into byte storage; any
/// access outside that window returns `None`.
struct VecMem {
    base: u32,
    data: Vec<u8>,
}

impl PpcMemory for VecMem {
    fn read_u8(&mut self, addr: u32) -> Option<u8> {
        let idx = addr.checked_sub(self.base)? as usize;
        self.data.get(idx).copied()
    }
    fn write_u8(&mut self, addr: u32, value: u8) -> Option<()> {
        let idx = addr.checked_sub(self.base)? as usize;
        *self.data.get_mut(idx)? = value;
        Some(())
    }
}

#[derive(Default)]
struct WriteObserverLog(Vec<(u32, u32, u32, u32, u32, u8)>);

impl PpcMemoryWriteObserver for WriteObserverLog {
    fn on_write(&mut self, pc: u32, lr: u32, rtoc: u32, sp: u32, addr: u32, value: u8) {
        self.0.push((pc, lr, rtoc, sp, addr, value));
    }
}

#[test]
fn run_with_import_observers_records_guest_store_bytes_with_context() {
    let mut mem = PpcSectionMem::new();
    let code = [
        d_form(36, 3, 4, 0), // stw r3, 0(r4)
        0x4e80_0020,         // blr
    ];
    mem.add_readonly_region(
        0x100,
        code.iter()
            .flat_map(|word| word.to_be_bytes())
            .collect::<Vec<_>>(),
    );
    mem.add_region(0x1000, vec![0; 8]);

    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.lr = 0;
    cpu.gpr[1] = 0x03ff_0000;
    cpu.gpr[2] = 0x0200_0000;
    cpu.gpr[3] = 0xaabb_ccdd;
    cpu.gpr[4] = 0x1000;
    let mut fetches = Vec::new();
    let mut writes = WriteObserverLog::default();

    let result = cpu.run_with_imports_and_observers(
        &mut mem,
        8,
        0,
        0x4000,
        0,
        &mut fetches,
        &mut writes,
        |_index, _cpu, _mem| unreachable!("no import slots configured"),
    );

    assert!(
        matches!(result, PpcRunResult::Halted { pc: 0, cycles: 2 }),
        "{result:?}"
    );
    assert_eq!(
        writes.0,
        vec![
            (0x100, 0, 0x0200_0000, 0x03ff_0000, 0x1000, 0xaa),
            (0x100, 0, 0x0200_0000, 0x03ff_0000, 0x1001, 0xbb),
            (0x100, 0, 0x0200_0000, 0x03ff_0000, 0x1002, 0xcc),
            (0x100, 0, 0x0200_0000, 0x03ff_0000, 0x1003, 0xdd),
        ]
    );
    assert_eq!(mem.read_u32_be(0x1000), Some(0xaabb_ccdd));
}

#[test]
fn decode_lwz_extracts_d_form_fields() {
    // lwz r3, 8(r1) — OPCD=32, RT=3, RA=1, D=8.
    let word = d_form(32, 3, 1, 8);
    assert_eq!(decode(word), Ok(PpcInstr::Lwz { rt: 3, ra: 1, d: 8 }));
}

#[test]
fn decode_stw_extracts_d_form_fields() {
    // stw r0, -4(r1) — OPCD=36, RS=0, RA=1, D=-4.
    let word = d_form(36, 0, 1, 0xFFFC); // -4 as i16
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stw {
            rs: 0,
            ra: 1,
            d: -4
        })
    );
}

#[test]
fn decode_stwu_extracts_d_form_fields() {
    let word = d_form(37, 1, 1, 0xFFE0); // stwu r1, -32(r1)
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stwu {
            rs: 1,
            ra: 1,
            d: -32
        })
    );
}

#[test]
fn step_lwz_reads_big_endian_from_memory() {
    // Memory at 0x1000 contains [0x12, 0x34, 0x56, 0x78].
    // lwz r3, 0(r4) with GPR4=0x1000 → GPR3 = 0x12345678.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x12, 0x34, 0x56, 0x78],
    };
    let res = cpu.step(&mut mem, d_form(32, 3, 4, 0));
    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[3], 0x1234_5678);
}

#[test]
fn step_lwz_with_ra_eq_0_treats_base_as_literal_zero() {
    // lwz r3, 0x1000(0) — RA=0, so the base is literal 0,
    // and the effective address is 0x1000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[0] = 0xDEAD_BEEF; // would corrupt EA if RA=0 read GPR0
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xCA, 0xFE, 0xF0, 0x0D],
    };
    let res = cpu.step(&mut mem, d_form(32, 3, 0, 0x1000));
    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[3], 0xCAFE_F00D);
}

#[test]
fn step_lwz_signed_negative_displacement() {
    // lwz r3, -4(r4) with GPR4=0x1004 → reads from 0x1000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1004;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x11, 0x22, 0x33, 0x44],
    };
    cpu.step(&mut mem, d_form(32, 3, 4, 0xFFFC));
    assert_eq!(cpu.gpr[3], 0x1122_3344);
}

#[test]
fn step_stw_writes_big_endian_to_memory() {
    // stw r3, 0(r4) with GPR3=0xCAFEBABE, GPR4=0x1000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    let res = cpu.step(&mut mem, d_form(36, 3, 4, 0));
    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(mem.data, vec![0xCA, 0xFE, 0xBA, 0xBE]);
}

#[test]
fn step_stwu_updates_ra_atomically_with_store() {
    // stwu r1, -16(r1) is the canonical "push stack frame"
    // idiom. With GPR1=0x2000 the EA is 0x1FF0; the previous
    // GPR1 value (0x2000) is stored there; then GPR1 is
    // updated to 0x1FF0.
    let mut cpu = PpcCpu::new();
    cpu.gpr[1] = 0x2000;
    let mut mem = VecMem {
        base: 0x1FF0,
        data: vec![0u8; 16],
    };
    let res = cpu.step(&mut mem, d_form(37, 1, 1, 0xFFF0));
    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[1], 0x1FF0);
    assert_eq!(&mem.data[..4], &[0x00, 0x00, 0x20, 0x00]);
}

#[test]
fn step_stwu_with_ra_eq_0_surfaces_illegal_instruction() {
    // The "u"-form requires a base register to update; RA=0
    // is invalid per the spec.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    let mut mem = VecMem {
        base: 0,
        data: vec![0u8; 8],
    };
    let word = d_form(37, 0, 0, 0);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x1000);
}

#[test]
fn step_lwz_unmapped_address_surfaces_memory_fault() {
    // GPR4=0x9000 — outside the [0x1000, 0x1004) window.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x9000;
    cpu.pc = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    let res = cpu.step(&mut mem, d_form(32, 3, 4, 0));
    assert!(matches!(
        res,
        PpcStepResult::MemoryFault {
            addr: 0x9000,
            was_write: false
        }
    ));
    // PC unchanged on fault.
    assert_eq!(cpu.pc, 0x1000);
}

#[test]
fn step_stw_unmapped_address_surfaces_memory_fault_with_write_flag() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xDEAD_BEEF;
    cpu.gpr[4] = 0x9000;
    cpu.pc = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    let res = cpu.step(&mut mem, d_form(36, 3, 4, 0));
    assert!(matches!(
        res,
        PpcStepResult::MemoryFault {
            addr: 0x9000,
            was_write: true
        }
    ));
    assert_eq!(cpu.pc, 0x1000);
}

#[test]
fn step_lwz_unaligned_address_surfaces_alignment_exception() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1001;
    cpu.pc = 0x200;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };

    let res = cpu.step(&mut mem, d_form(32, 3, 4, 0));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::Alignment {
            addr: 0x1001,
            size: 4,
            access: PpcMemoryAccess::Load
        })
    );
    assert_eq!(cpu.pc, 0x200);
}

#[test]
fn step_lwz_unaligned_address_can_be_emulated_by_policy() {
    let mut cpu = PpcCpu::new();
    cpu.alignment_policy = PpcAlignmentPolicy::EmulateData;
    cpu.gpr[4] = 0x1001;
    cpu.pc = 0x200;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x00, 0x12, 0x34, 0x56, 0x78],
    };

    let res = cpu.step(&mut mem, d_form(32, 3, 4, 0));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[3], 0x1234_5678);
    assert_eq!(cpu.pc, 0x204);
}

#[test]
fn step_sth_unaligned_address_surfaces_alignment_exception_without_write() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xDEAD_BEEF;
    cpu.gpr[4] = 0x1001;
    cpu.pc = 0x200;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };

    let res = cpu.step(&mut mem, d_form(44, 3, 4, 0));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::Alignment {
            addr: 0x1001,
            size: 2,
            access: PpcMemoryAccess::Store
        })
    );
    assert_eq!(mem.data, vec![0u8; 4]);
    assert_eq!(cpu.pc, 0x200);
}

#[test]
fn decode_lbz_extracts_d_form_fields() {
    let word = d_form(34, 3, 4, 8);
    assert_eq!(decode(word), Ok(PpcInstr::Lbz { rt: 3, ra: 4, d: 8 }));
}

#[test]
fn decode_lhz_extracts_d_form_fields() {
    let word = d_form(40, 3, 4, 0);
    assert_eq!(decode(word), Ok(PpcInstr::Lhz { rt: 3, ra: 4, d: 0 }));
}

#[test]
fn decode_stb_extracts_d_form_fields() {
    let word = d_form(38, 5, 1, 4);
    assert_eq!(decode(word), Ok(PpcInstr::Stb { rs: 5, ra: 1, d: 4 }));
}

#[test]
fn decode_sth_extracts_d_form_fields() {
    let word = d_form(44, 5, 1, 6);
    assert_eq!(decode(word), Ok(PpcInstr::Sth { rs: 5, ra: 1, d: 6 }));
}

#[test]
fn decode_bcctr_extracts_xl_form_fields() {
    // bctr — bcctr 20, 0 — OPCD=19, XO=528.
    let word = xl_form(19, 20, 0, 528, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Bcctr {
            bo: 20,
            bi: 0,
            lk: false
        })
    );
}

#[test]
fn step_lbz_zero_extends_loaded_byte() {
    // Memory at 0x1000 contains 0xCA. lbz r3, 0(r4) with
    // GPR4=0x1000 → GPR3 = 0x000000CA.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_FFFF; // pre-populated; must be overwritten
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xCA],
    };
    cpu.step(&mut mem, d_form(34, 3, 4, 0));
    assert_eq!(cpu.gpr[3], 0x0000_00CA);
}

#[test]
fn step_lhz_reads_big_endian_halfword_zero_extended() {
    // Memory: [0x12, 0x34] at 0x1000. lhz r3, 0(r4) → 0x1234.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_0000;
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x12, 0x34],
    };
    cpu.step(&mut mem, d_form(40, 3, 4, 0));
    assert_eq!(cpu.gpr[3], 0x0000_1234);
}

#[test]
fn step_stb_writes_low_byte_of_rs() {
    // GPR3 = 0xCAFEBABE. stb r3, 0(r4) writes only 0xBE.
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 1],
    };
    cpu.step(&mut mem, d_form(38, 3, 4, 0));
    assert_eq!(mem.data, vec![0xBE]);
}

#[test]
fn step_sth_writes_low_halfword_big_endian() {
    // GPR3 = 0xDEADBEEF. sth r3, 0(r4) writes [0xBE, 0xEF].
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xDEAD_BEEF;
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 2],
    };
    cpu.step(&mut mem, d_form(44, 3, 4, 0));
    assert_eq!(mem.data, vec![0xBE, 0xEF]);
}

#[test]
fn step_lbz_unmapped_address_surfaces_memory_fault() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x9000;
    cpu.pc = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    let res = cpu.step(&mut mem, d_form(34, 3, 4, 0));
    assert!(matches!(
        res,
        PpcStepResult::MemoryFault {
            addr: 0x9000,
            was_write: false
        }
    ));
    assert_eq!(cpu.pc, 0x1000);
}

#[test]
fn step_bctr_jumps_to_ctr_word_aligned() {
    // bctr — pc = CTR & ~3. Force the low two bits set to
    // verify they're cleared.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x4000;
    cpu.ctr = 0x0001_2347;
    cpu.step_instruction(xl_form(19, 20, 0, 528, false));
    assert_eq!(cpu.pc, 0x0001_2344);
}

#[test]
fn step_bctrl_saves_return_address_in_lr() {
    // bctrl sets LR before jumping (lk=1).
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x2000;
    cpu.ctr = 0x4000;
    cpu.step_instruction(xl_form(19, 20, 0, 528, true));
    assert_eq!(cpu.pc, 0x4000);
    assert_eq!(cpu.lr, 0x2004);
}

#[test]
fn step_bcctr_conditional_takes_branch_when_cr_bit_matches() {
    // beqctr (= bcctr 12, 2) — branch to CTR if CR0.EQ set.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.ctr = 0x4000;
    cpu.set_cr_field(0, 0b0010); // EQ
    cpu.step_instruction(xl_form(19, 12, 2, 528, false));
    assert_eq!(cpu.pc, 0x4000);
}

#[test]
fn step_bcctr_conditional_falls_through_when_cr_bit_clear() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.ctr = 0x4000;
    cpu.set_cr_field(0, 0b1000); // LT, not EQ
    cpu.step_instruction(xl_form(19, 12, 2, 528, false));
    assert_eq!(cpu.pc, 0x1004);
}

#[test]
fn step_bcctr_with_ctr_decrement_bo_surfaces_illegal_instruction() {
    // BO=0 means "decrement CTR + test CR" — the spec calls
    // this combination undefined for bcctr. The dispatcher
    // refuses cleanly.
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    cpu.ctr = 0x4000;
    let word = xl_form(19, 0, 0, 528, false);
    let res = cpu.step_instruction(word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x1000);
}

#[test]
fn decode_lfs_extracts_d_form_fields() {
    let word = d_form(48, 3, 1, 16);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lfs {
            frt: 3,
            ra: 1,
            d: 16
        })
    );
}

#[test]
fn decode_lfd_extracts_d_form_fields() {
    let word = d_form(50, 3, 1, 8);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lfd {
            frt: 3,
            ra: 1,
            d: 8
        })
    );
}

#[test]
fn decode_stfs_extracts_d_form_fields() {
    let word = d_form(52, 5, 1, 4);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stfs {
            frs: 5,
            ra: 1,
            d: 4
        })
    );
}

#[test]
fn decode_stfd_extracts_d_form_fields() {
    let word = d_form(54, 5, 1, 8);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stfd {
            frs: 5,
            ra: 1,
            d: 8
        })
    );
}

#[test]
fn decode_fp_d_form_update_extracts_fields() {
    assert_eq!(
        decode(d_form(49, 3, 1, 16)),
        Ok(PpcInstr::Lfsu {
            frt: 3,
            ra: 1,
            d: 16
        })
    );
    assert_eq!(
        decode(d_form(51, 4, 2, 0xFFF8)),
        Ok(PpcInstr::Lfdu {
            frt: 4,
            ra: 2,
            d: -8
        })
    );
    assert_eq!(
        decode(d_form(53, 5, 3, 4)),
        Ok(PpcInstr::Stfsu {
            frs: 5,
            ra: 3,
            d: 4
        })
    );
    assert_eq!(
        decode(d_form(55, 6, 4, 0xFFF0)),
        Ok(PpcInstr::Stfdu {
            frs: 6,
            ra: 4,
            d: -16
        })
    );
}

#[test]
fn step_lfs_loads_single_and_widens_to_double() {
    // Memory: float bits for 3.5f (= 0x40600000).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let bits = 3.5f32.to_bits();
    let mut mem = VecMem {
        base: 0x1000,
        data: bits.to_be_bytes().to_vec(),
    };
    cpu.step(&mut mem, d_form(48, 3, 4, 0));
    // The stored FPR should be the f64 representation of 3.5.
    assert_eq!(f64::from_bits(cpu.fpr[3]), 3.5);
}

#[test]
fn step_lfsu_loads_single_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let bits = 6.25f32.to_bits();
    let mut mem = VecMem {
        base: 0x1004,
        data: bits.to_be_bytes().to_vec(),
    };

    let res = cpu.step(&mut mem, d_form(49, 3, 4, 4));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(f64::from_bits(cpu.fpr[3]), 6.25);
    assert_eq!(cpu.gpr[4], 0x1004);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_lfd_loads_double_bit_pattern() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let value: f64 = 1.234_567_89;
    let mut mem = VecMem {
        base: 0x1000,
        data: value.to_bits().to_be_bytes().to_vec(),
    };
    cpu.step(&mut mem, d_form(50, 3, 4, 0));
    assert_eq!(cpu.fpr[3], value.to_bits());
}

#[test]
fn step_lfdu_loads_double_bit_pattern_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let value: f64 = -9.875;
    let mut mem = VecMem {
        base: 0x1008,
        data: value.to_bits().to_be_bytes().to_vec(),
    };

    let res = cpu.step(&mut mem, d_form(51, 3, 4, 8));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.fpr[3], value.to_bits());
    assert_eq!(cpu.gpr[4], 0x1008);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_lfd_unaligned_address_surfaces_alignment_exception() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1004;
    cpu.pc = 0x200;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 16],
    };

    let res = cpu.step(&mut mem, d_form(50, 3, 4, 0));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::Alignment {
            addr: 0x1004,
            size: 8,
            access: PpcMemoryAccess::Load
        })
    );
    assert_eq!(cpu.pc, 0x200);
}

#[test]
fn step_stfs_narrows_double_to_single_in_memory() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.fpr[5] = 2.5f64.to_bits();
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    cpu.step(&mut mem, d_form(52, 5, 4, 0));
    // Memory should contain the f32 bit pattern of 2.5.
    assert_eq!(mem.data, 2.5f32.to_bits().to_be_bytes().to_vec());
}

#[test]
fn step_stfsu_narrows_double_to_single_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.fpr[5] = (-2.75f64).to_bits();
    let mut mem = VecMem {
        base: 0x1004,
        data: vec![0u8; 4],
    };

    let res = cpu.step(&mut mem, d_form(53, 5, 4, 4));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(mem.data, (-2.75f32).to_bits().to_be_bytes().to_vec());
    assert_eq!(cpu.gpr[4], 0x1004);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_stfd_writes_double_bit_pattern() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let value: f64 = -1234.5678;
    cpu.fpr[5] = value.to_bits();
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };
    cpu.step(&mut mem, d_form(54, 5, 4, 0));
    assert_eq!(mem.data, value.to_bits().to_be_bytes().to_vec());
}

#[test]
fn step_stfdu_writes_double_bit_pattern_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1010;
    let value: f64 = 4096.125;
    cpu.fpr[5] = value.to_bits();
    let mut mem = VecMem {
        base: 0x1008,
        data: vec![0u8; 8],
    };

    let res = cpu.step(&mut mem, d_form(55, 5, 4, 0xFFF8));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(mem.data, value.to_bits().to_be_bytes().to_vec());
    assert_eq!(cpu.gpr[4], 0x1008);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_fp_d_form_update_with_ra_eq_0_surfaces_illegal_instruction() {
    for word in [
        d_form(49, 3, 0, 2),
        d_form(51, 3, 0, 2),
        d_form(53, 3, 0, 2),
        d_form(55, 3, 0, 2),
    ] {
        let mut cpu = PpcCpu::new();
        cpu.pc = 0x100;
        let mut mem = VecMem {
            base: 0,
            data: vec![0u8; 16],
        };

        let res = cpu.step(&mut mem, word);

        assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
        assert_eq!(cpu.pc, 0x100);
    }
}

/// A-form word builder for FP arithmetic. `xo_5` is the
/// 5-bit secondary opcode (e.g. 21 for `fadd`).
fn a_form_fp(opcd: u8, frt: u8, fra: u8, frb: u8, frc: u8, xo_5: u8, rc: bool) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((frt as u32 & 0x1F) << 21)
        | ((fra as u32 & 0x1F) << 16)
        | ((frb as u32 & 0x1F) << 11)
        | ((frc as u32 & 0x1F) << 6)
        | ((xo_5 as u32 & 0x1F) << 1)
        | (if rc { 1 } else { 0 })
}

/// X-form word builder for FP move/sign mnemonics (no FRA;
/// FRB lives in the standard X-form RB slot).
fn x_form_fp_move(opcd: u8, frt: u8, frb: u8, xo_10: u16, rc: bool) -> u32 {
    ((opcd as u32 & 0x3F) << 26)
        | ((frt as u32 & 0x1F) << 21)
        | ((frb as u32 & 0x1F) << 11)
        | ((xo_10 as u32 & 0x3FF) << 1)
        | (if rc { 1 } else { 0 })
}

fn x_form_mcrfs(bf: u8, bfa: u8) -> u32 {
    (63u32 << 26) | ((bf as u32 & 0x07) << 23) | ((bfa as u32 & 0x07) << 18) | (64u32 << 1)
}

fn x_form_fpscr_bit(xo_10: u16, bt: u8, rc: bool) -> u32 {
    (63u32 << 26)
        | ((bt as u32 & 0x1F) << 21)
        | ((xo_10 as u32 & 0x03FF) << 1)
        | (if rc { 1 } else { 0 })
}

fn x_form_mtfsfi(bf: u8, u: u8, rc: bool) -> u32 {
    (63u32 << 26)
        | ((bf as u32 & 0x07) << 23)
        | ((u as u32 & 0x0F) << 12)
        | (134u32 << 1)
        | (if rc { 1 } else { 0 })
}

fn xfl_form_mtfsf(flm: u8, frb: u8, rc: bool) -> u32 {
    (63u32 << 26)
        | ((flm as u32) << 17)
        | ((frb as u32 & 0x1F) << 11)
        | (711u32 << 1)
        | (if rc { 1 } else { 0 })
}

#[test]
fn decode_fadd_extracts_a_form() {
    let word = a_form_fp(63, 3, 4, 5, 0, 21, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fadd {
            frt: 3,
            fra: 4,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fsub_extracts_a_form() {
    let word = a_form_fp(63, 3, 4, 5, 0, 20, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fsub {
            frt: 3,
            fra: 4,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fmul_uses_frc_slot() {
    // fmul takes FRC (not FRB). Build with frb=0, frc=7.
    let word = a_form_fp(63, 3, 4, 0, 7, 25, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fmul {
            frt: 3,
            fra: 4,
            frc: 7,
            rc: false
        })
    );
}

#[test]
fn decode_fdiv_extracts_a_form() {
    let word = a_form_fp(63, 3, 4, 5, 0, 18, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fdiv {
            frt: 3,
            fra: 4,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fneg_extracts_x_form() {
    let word = x_form_fp_move(63, 3, 5, 40, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fneg {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fmr_extracts_x_form() {
    let word = x_form_fp_move(63, 3, 5, 72, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fmr {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fabs_extracts_x_form() {
    let word = x_form_fp_move(63, 3, 5, 264, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fabs {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn step_fadd_double_precision() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 1.5f64.to_bits();
    cpu.fpr[5] = 2.25f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 5, 0, 21, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 3.75);
}

#[test]
fn step_fp_arithmetic_updates_fpscr_result_flags() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 1.5f64.to_bits();
    cpu.fpr[5] = 2.25f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 5, 0, 21, false));
    assert_eq!(cpu.fpscr_field(4), 0b0100); // positive result
    assert!(!cpu.fpscr_bit(15));
}

#[test]
fn step_fp_negative_zero_sets_fpscr_class_descriptor() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[5] = 0.0f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 40, false));
    assert_eq!(cpu.fpr[3], (-0.0f64).to_bits());
    assert_eq!(cpu.fpscr_field(4), 0b0010); // zero result
    assert!(cpu.fpscr_bit(15)); // negative zero class descriptor
}

#[test]
fn step_fp_record_form_copies_fpscr_exception_summary_to_cr1() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr_field(0, 0b1010);
    cpu.fpr[4] = 1.0f64.to_bits();
    cpu.fpr[5] = 2.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 5, 0, 21, true));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 3.0);
    assert_eq!(cpu.cr_field(1), 0b1010);
}

#[test]
fn step_fp_non_record_form_leaves_cr1_unchanged() {
    let mut cpu = PpcCpu::new();
    cpu.set_cr_field(1, 0b0110);
    cpu.fpr[4] = 1.0f64.to_bits();
    cpu.fpr[5] = 2.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 5, 0, 21, false));
    assert_eq!(cpu.cr_field(1), 0b0110);
    assert_eq!(cpu.fpscr_field(4), 0b0100);
}

#[test]
fn step_fsub_double_precision() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 10.0f64.to_bits();
    cpu.fpr[5] = 3.5f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 5, 0, 20, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 6.5);
}

#[test]
fn step_fmul_uses_frc() {
    // fmul r3, r4, r7 (FRC=7).
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 6.0f64.to_bits();
    cpu.fpr[7] = 7.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 0, 7, 25, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 42.0);
}

#[test]
fn step_fdiv_double_precision() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 10.0f64.to_bits();
    cpu.fpr[5] = 4.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 5, 0, 18, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 2.5);
}

#[test]
fn step_fdiv_by_zero_produces_infinity() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 1.0f64.to_bits();
    cpu.fpr[5] = 0.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 5, 0, 18, false));
    let v = f64::from_bits(cpu.fpr[3]);
    assert!(v.is_infinite() && v.is_sign_positive());
}

#[test]
fn step_fneg_toggles_sign_bit() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[5] = 3.5f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 40, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), -3.5);
    // Negative input → positive output.
    cpu.step_instruction(x_form_fp_move(63, 4, 3, 40, false));
    assert_eq!(f64::from_bits(cpu.fpr[4]), 3.5);
}

#[test]
fn step_fmr_copies_full_64_bits() {
    let mut cpu = PpcCpu::new();
    let bits = 0xCAFE_BABE_DEAD_BEEF;
    cpu.fpr[5] = bits;
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 72, false));
    assert_eq!(cpu.fpr[3], bits);
}

#[test]
fn decode_fadds_extracts_a_form_opcd_59() {
    let word = a_form_fp(59, 3, 4, 5, 0, 21, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fadds {
            frt: 3,
            fra: 4,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fmuls_uses_frc_slot() {
    let word = a_form_fp(59, 3, 4, 0, 7, 25, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fmuls {
            frt: 3,
            fra: 4,
            frc: 7,
            rc: false
        })
    );
}

#[test]
fn decode_fcmpu_extracts_bf_and_fp_regs() {
    // OPCD=63, BF=2, FRA=4, FRB=5, XO=0.
    // BF at MSB=0 6..8 → host 23..25.
    let word = (63u32 << 26) | (2u32 << 23) | (4u32 << 16) | (5u32 << 11);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fcmpu {
            bf: 2,
            fra: 4,
            frb: 5
        })
    );
}

#[test]
fn step_fadds_rounds_to_single_precision() {
    // 0.1 + 0.2 in single precision is approximately
    // 0.30000001192... — different from the double-precision
    // result. Verifying the result equals (0.1f32 + 0.2f32)
    // cast back to f64 confirms the rounding happened.
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 0.1f64.to_bits();
    cpu.fpr[5] = 0.2f64.to_bits();
    cpu.step_instruction(a_form_fp(59, 3, 4, 5, 0, 21, false));
    let result = f64::from_bits(cpu.fpr[3]);
    let expected = (0.1f64 + 0.2f64) as f32 as f64;
    assert_eq!(result, expected);
}

#[test]
fn step_fsubs_single_precision() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 1.5f64.to_bits();
    cpu.fpr[5] = 0.5f64.to_bits();
    cpu.step_instruction(a_form_fp(59, 3, 4, 5, 0, 20, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 1.0);
}

#[test]
fn step_fmuls_uses_frc_at_correct_slot() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 3.5f64.to_bits();
    cpu.fpr[7] = 4.0f64.to_bits();
    cpu.step_instruction(a_form_fp(59, 3, 4, 0, 7, 25, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 14.0);
}

#[test]
fn step_fdivs_single_precision() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 10.0f64.to_bits();
    cpu.fpr[5] = 4.0f64.to_bits();
    cpu.step_instruction(a_form_fp(59, 3, 4, 5, 0, 18, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 2.5);
}

#[test]
fn decode_fmadd_extracts_a_form_with_all_four_slots() {
    // fmadd FRT=3, FRA=4, FRC=5, FRB=6 — XO=29.
    let word = a_form_fp(63, 3, 4, 6, 5, 29, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fmadd {
            frt: 3,
            fra: 4,
            frc: 5,
            frb: 6,
            rc: false
        })
    );
}

#[test]
fn decode_fmsub_extracts_xo_28() {
    let word = a_form_fp(63, 3, 4, 6, 5, 28, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fmsub {
            frt: 3,
            fra: 4,
            frc: 5,
            frb: 6,
            rc: false
        })
    );
}

#[test]
fn decode_fnmadd_fnmsub_distinguish_from_fmadd_fmsub() {
    let nmadd = a_form_fp(63, 3, 4, 6, 5, 31, false);
    assert_eq!(
        decode(nmadd),
        Ok(PpcInstr::Fnmadd {
            frt: 3,
            fra: 4,
            frc: 5,
            frb: 6,
            rc: false
        })
    );
    let nmsub = a_form_fp(63, 3, 4, 6, 5, 30, false);
    assert_eq!(
        decode(nmsub),
        Ok(PpcInstr::Fnmsub {
            frt: 3,
            fra: 4,
            frc: 5,
            frb: 6,
            rc: false
        })
    );
}

#[test]
fn decode_fmadds_uses_opcd_59() {
    let word = a_form_fp(59, 3, 4, 6, 5, 29, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fmadds {
            frt: 3,
            fra: 4,
            frc: 5,
            frb: 6,
            rc: false
        })
    );
}

#[test]
fn decode_frsp_fctiw_fctiwz_extract_x_form() {
    let frsp = x_form_fp_move(63, 3, 5, 12, false);
    assert_eq!(
        decode(frsp),
        Ok(PpcInstr::Frsp {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
    let fctiw = x_form_fp_move(63, 3, 5, 14, false);
    assert_eq!(
        decode(fctiw),
        Ok(PpcInstr::Fctiw {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
    let fctiwz = x_form_fp_move(63, 3, 5, 15, false);
    assert_eq!(
        decode(fctiwz),
        Ok(PpcInstr::Fctiwz {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fpscr_environment_instructions_extract_fields() {
    assert_eq!(
        decode(x_form_mcrfs(2, 4)),
        Ok(PpcInstr::Mcrfs { bf: 2, bfa: 4 })
    );
    assert_eq!(
        decode(x_form_fpscr_bit(38, 3, true)),
        Ok(PpcInstr::Mtfsb1 { bt: 3, rc: true })
    );
    assert_eq!(
        decode(x_form_fpscr_bit(70, 25, false)),
        Ok(PpcInstr::Mtfsb0 { bt: 25, rc: false })
    );
    assert_eq!(
        decode(x_form_mtfsfi(7, 1, true)),
        Ok(PpcInstr::Mtfsfi {
            bf: 7,
            u: 1,
            rc: true
        })
    );
    assert_eq!(
        decode(xfl_form_mtfsf(0xE1, 5, true)),
        Ok(PpcInstr::Mtfsf {
            flm: 0xE1,
            frb: 5,
            rc: true
        })
    );
}

#[test]
fn step_fmadd_computes_fra_times_frc_plus_frb() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 2.0f64.to_bits();
    cpu.fpr[5] = 3.0f64.to_bits();
    cpu.fpr[6] = 7.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 6, 5, 29, false));
    // (2.0 * 3.0) + 7.0 = 13.0
    assert_eq!(f64::from_bits(cpu.fpr[3]), 13.0);
}

#[test]
fn step_fmsub_computes_fra_times_frc_minus_frb() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 2.0f64.to_bits();
    cpu.fpr[5] = 3.0f64.to_bits();
    cpu.fpr[6] = 7.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 6, 5, 28, false));
    // (2.0 * 3.0) - 7.0 = -1.0
    assert_eq!(f64::from_bits(cpu.fpr[3]), -1.0);
}

#[test]
fn step_fnmadd_negates_fmadd_result() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 2.0f64.to_bits();
    cpu.fpr[5] = 3.0f64.to_bits();
    cpu.fpr[6] = 7.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 6, 5, 31, false));
    // -((2 * 3) + 7) = -13
    assert_eq!(f64::from_bits(cpu.fpr[3]), -13.0);
}

#[test]
fn step_fnmsub_negates_fmsub_result() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 2.0f64.to_bits();
    cpu.fpr[5] = 3.0f64.to_bits();
    cpu.fpr[6] = 7.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 6, 5, 30, false));
    // -((2 * 3) - 7) = -(-1) = 1
    assert_eq!(f64::from_bits(cpu.fpr[3]), 1.0);
}

#[test]
fn step_frsp_rounds_double_to_single_precision() {
    let mut cpu = PpcCpu::new();
    // Use a value that rounds differently in f32 vs f64.
    cpu.fpr[5] = 0.1f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 12, false));
    let rounded = f64::from_bits(cpu.fpr[3]);
    let expected = 0.1f64 as f32 as f64;
    assert_eq!(rounded, expected);
    // Verify it's a different bit pattern than the original
    // double precision 0.1 (which it is — single-precision
    // 0.1 has fewer mantissa bits).
    assert_ne!(rounded, 0.1f64);
}

#[test]
fn step_fctiwz_truncates_toward_zero() {
    let mut cpu = PpcCpu::new();
    // 3.7 → 3 (truncate)
    cpu.fpr[5] = 3.7f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 15, false));
    assert_eq!(cpu.fpr[3] as u32, 3);
    // -3.7 → -3 (also truncate toward zero, NOT floor)
    cpu.fpr[5] = (-3.7f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 15, false));
    assert_eq!(cpu.fpr[3] as u32 as i32, -3);
}

#[test]
fn step_fctiw_uses_fpscr_round_to_nearest_even() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr_field(7, 0);

    cpu.fpr[5] = 2.5f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32 as i32, 2);

    cpu.fpr[5] = 3.5f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32 as i32, 4);

    cpu.fpr[5] = (-2.5f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32 as i32, -2);
}

#[test]
fn step_fctiw_uses_fpscr_directed_rounding_modes() {
    let mut cpu = PpcCpu::new();

    cpu.set_fpscr_field(7, 1);
    cpu.fpr[5] = (-3.7f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32 as i32, -3);

    cpu.set_fpscr_field(7, 2);
    cpu.fpr[5] = (-3.2f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32 as i32, -3);

    cpu.set_fpscr_field(7, 3);
    cpu.fpr[5] = (-3.2f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32 as i32, -4);
}

#[test]
fn step_fctiw_saturates_on_overflow() {
    let mut cpu = PpcCpu::new();
    // Value way larger than i32::MAX.
    cpu.fpr[5] = 1e20f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32, i32::MAX as u32);
    // Way smaller than i32::MIN.
    cpu.fpr[5] = (-1e20f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32, i32::MIN as u32);
    // NaN → 0 (we picked 0; spec says undefined).
    cpu.fpr[5] = f64::NAN.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 14, false));
    assert_eq!(cpu.fpr[3] as u32, 0);
}

#[test]
fn decode_fsqrt_extracts_a_form_xo_22() {
    // OPCD=63, FRT=3, FRB=5, XO=22.
    let word = a_form_fp(63, 3, 0, 5, 0, 22, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fsqrt {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fsqrts_extracts_opcd_59_xo_22() {
    let word = a_form_fp(59, 3, 0, 5, 0, 22, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fsqrts {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_fres_extracts_opcd_59_xo_24() {
    let word = a_form_fp(59, 3, 0, 5, 0, 24, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fres {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn decode_frsqrte_extracts_opcd_63_xo_26() {
    let word = a_form_fp(63, 3, 0, 5, 0, 26, true);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Frsqrte {
            frt: 3,
            frb: 5,
            rc: true
        })
    );
}

#[test]
fn decode_fnabs_extracts_x_form_xo_136() {
    let word = x_form_fp_move(63, 3, 5, 136, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fnabs {
            frt: 3,
            frb: 5,
            rc: false
        })
    );
}

#[test]
fn step_fsqrt_double_precision() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[5] = 16.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 0, 5, 0, 22, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 4.0);
    // sqrt of negative produces NaN (IEEE-754 default).
    cpu.fpr[5] = (-1.0f64).to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 0, 5, 0, 22, false));
    assert!(f64::from_bits(cpu.fpr[3]).is_nan());
}

#[test]
fn step_fsqrts_rounds_to_single() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[5] = 2.0f64.to_bits();
    cpu.step_instruction(a_form_fp(59, 3, 0, 5, 0, 22, false));
    let result = f64::from_bits(cpu.fpr[3]);
    let expected = (2.0f64.sqrt() as f32) as f64;
    assert_eq!(result, expected);
}

#[test]
fn step_fres_returns_single_precision_reciprocal() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[5] = 3.0f64.to_bits();
    cpu.step_instruction(a_form_fp(59, 3, 0, 5, 0, 24, false));
    let result = f64::from_bits(cpu.fpr[3]);
    let expected = ((1.0f64 / 3.0) as f32) as f64;
    assert_eq!(result, expected);
}

#[test]
fn step_frsqrte_returns_double_precision_reciprocal_sqrt() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr_field(0, 0b0101);
    cpu.fpr[5] = 4.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 0, 5, 0, 26, true));
    let result = f64::from_bits(cpu.fpr[3]);
    assert_eq!(result, 0.5);
    assert_eq!(cpu.fpscr_field(4), 0b0100);
    assert_eq!(cpu.cr_field(1), 0b0101);
}

#[test]
fn step_frsqrte_special_values_follow_ieee_defaults() {
    let mut cpu = PpcCpu::new();

    cpu.fpr[5] = 0.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 0, 5, 0, 26, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), f64::INFINITY);

    cpu.fpr[5] = (-0.0f64).to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 0, 5, 0, 26, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), f64::NEG_INFINITY);

    cpu.fpr[5] = (-4.0f64).to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 0, 5, 0, 26, false));
    assert!(f64::from_bits(cpu.fpr[3]).is_nan());
}

#[test]
fn step_fnabs_sets_sign_bit_unconditionally() {
    let mut cpu = PpcCpu::new();
    // Positive input → negative result.
    cpu.fpr[5] = 3.5f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 136, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), -3.5);
    // Negative input → still negative (sign bit was set; stays set).
    cpu.fpr[5] = (-3.5f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 136, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), -3.5);
}

#[test]
fn decode_fsel_extracts_a_form_xo_23() {
    // fsel FRT=3, FRA=4, FRC=5, FRB=6, XO=23.
    let word = a_form_fp(63, 3, 4, 6, 5, 23, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Fsel {
            frt: 3,
            fra: 4,
            frc: 5,
            frb: 6,
            rc: false
        })
    );
}

#[test]
fn step_fsel_picks_frc_when_fra_nonnegative() {
    // FRA=1.0 (>= 0) → FRT = FRC.
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 1.0f64.to_bits();
    cpu.fpr[5] = 100.0f64.to_bits(); // FRC
    cpu.fpr[6] = 200.0f64.to_bits(); // FRB
    cpu.step_instruction(a_form_fp(63, 3, 4, 6, 5, 23, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 100.0);
}

#[test]
fn step_fsel_picks_frb_when_fra_negative() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = (-1.0f64).to_bits();
    cpu.fpr[5] = 100.0f64.to_bits();
    cpu.fpr[6] = 200.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 6, 5, 23, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 200.0);
}

#[test]
fn step_fsel_treats_negative_zero_as_nonnegative() {
    // Per spec, the test is `FRA >= 0`. Rust's f64 `>=`
    // considers -0.0 == 0.0 as true, so -0.0 → pick FRC.
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = (-0.0f64).to_bits();
    cpu.fpr[5] = 100.0f64.to_bits();
    cpu.fpr[6] = 200.0f64.to_bits();
    cpu.step_instruction(a_form_fp(63, 3, 4, 6, 5, 23, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 100.0);
}

#[test]
fn step_fcmpu_writes_lt_gt_eq() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 1.0f64.to_bits();
    cpu.fpr[5] = 2.0f64.to_bits();
    // fcmpu cr0, fr4, fr5 — 1.0 < 2.0 → LT
    let word = (63u32 << 26) | (4u32 << 16) | (5u32 << 11);
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(0), 0b1000);
    assert_eq!(cpu.fpscr_field(4), 0b1000);
    assert!(!cpu.fpscr_bit(15));

    // 2.0 > 1.0 → GT
    cpu.fpr[4] = 2.0f64.to_bits();
    cpu.fpr[5] = 1.0f64.to_bits();
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(0), 0b0100);
    assert_eq!(cpu.fpscr_field(4), 0b0100);

    // 1.0 == 1.0 → EQ
    cpu.fpr[4] = 1.0f64.to_bits();
    cpu.fpr[5] = 1.0f64.to_bits();
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(0), 0b0010);
    assert_eq!(cpu.fpscr_field(4), 0b0010);
}

#[test]
fn step_fcmpu_with_nan_writes_unordered() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = f64::NAN.to_bits();
    cpu.fpr[5] = 1.0f64.to_bits();
    let word = (63u32 << 26) | (4u32 << 16) | (5u32 << 11);
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(0), 0b0001); // UNO
    assert_eq!(cpu.fpscr_field(4), 0b0001);
}

#[test]
fn step_fcmpu_writes_to_chosen_cr_field() {
    // fcmpu cr3 vs cr0.
    let mut cpu = PpcCpu::new();
    cpu.fpr[4] = 1.0f64.to_bits();
    cpu.fpr[5] = 1.0f64.to_bits();
    let word = (63u32 << 26) | (3u32 << 23) | (4u32 << 16) | (5u32 << 11);
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(3), 0b0010);
    assert_eq!(cpu.cr_field(0), 0); // untouched
}

#[test]
fn step_fabs_clears_sign_bit() {
    let mut cpu = PpcCpu::new();
    cpu.fpr[5] = (-7.5f64).to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 264, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 7.5);
    // Already-positive → unchanged.
    cpu.fpr[5] = 7.5f64.to_bits();
    cpu.step_instruction(x_form_fp_move(63, 3, 5, 264, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 7.5);
}

#[test]
fn step_mffs_returns_fpscr_into_target_fpr_and_records_cr1() {
    // mffs. FRT -- opcd=63, FRT, FRA/FRB ignored, XO=583, Rc=1.
    // Encoding: (63<<26) | (FRT<<21) | (XO<<1) | Rc
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr(0xA123_4567);
    cpu.fpr[3] = 0xDEAD_BEEF_CAFE_BABE;
    let word = (63u32 << 26) | (3u32 << 21) | (583u32 << 1) | 1;
    cpu.step_instruction(word);
    assert_eq!(cpu.fpr[3], 0xA123_4567);
    assert_eq!(cpu.cr_field(1), 0xA);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_mcrfs_copies_fpscr_field_to_cr_field() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr(0x1234_5678);
    cpu.step_instruction(x_form_mcrfs(2, 4));
    assert_eq!(cpu.cr_field(2), 0x5);
    assert_eq!(cpu.fpscr(), 0x1234_5678);
}

#[test]
fn step_mtfsb1_mtfsb0_modify_fpscr_bits_and_record_cr1() {
    let mut cpu = PpcCpu::new();
    cpu.step_instruction(x_form_fpscr_bit(38, 3, true));
    assert!(cpu.fpscr_bit(3));
    assert_eq!(cpu.fpscr_field(0), 0b0001);
    assert_eq!(cpu.cr_field(1), 0b0001);

    cpu.step_instruction(x_form_fpscr_bit(70, 3, true));
    assert!(!cpu.fpscr_bit(3));
    assert_eq!(cpu.fpscr_field(0), 0);
    assert_eq!(cpu.cr_field(1), 0);
}

#[test]
fn step_mtfsfi_writes_fpscr_field_and_record_cr1() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr(0xFFFF_FFFF);
    cpu.step_instruction(x_form_mtfsfi(7, 1, false));
    assert_eq!(cpu.fpscr_field(7), 1);

    cpu.step_instruction(x_form_mtfsfi(0, 0b1100, true));
    assert_eq!(cpu.fpscr_field(0), 0b1100);
    assert_eq!(cpu.cr_field(1), 0b1100);
}

#[test]
fn step_mtfsf_updates_selected_fpscr_fields_from_low_fpr_half() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr(0xFFFF_FFFF);
    cpu.fpr[5] = 0xDEAD_BEEF_1234_5678;

    cpu.step_instruction(xfl_form_mtfsf(0x91, 5, false));

    assert_eq!(cpu.fpscr(), 0x1FF4_FFF8);
}

#[test]
fn step_mtfsf_record_form_copies_new_exception_summary_to_cr1() {
    let mut cpu = PpcCpu::new();
    cpu.set_fpscr(0);
    cpu.fpr[5] = 0xA000_0000;

    cpu.step_instruction(xfl_form_mtfsf(0x80, 5, true));

    assert_eq!(cpu.fpscr_field(0), 0xA);
    assert_eq!(cpu.cr_field(1), 0xA);
}

#[test]
fn step_lfs_stfs_round_trip_preserves_single_precision_value() {
    // Load a single, store it back: verify the written
    // memory matches the source bytes (or close to — the
    // double->single narrowing is identity for values
    // expressible as f32).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let original = 42.125f32; // exactly representable as f32
    let mut mem = VecMem {
        base: 0x1000,
        data: {
            let mut v = original.to_bits().to_be_bytes().to_vec();
            v.extend_from_slice(&[0u8; 4]); // landing zone
            v
        },
    };
    cpu.step(&mut mem, d_form(48, 3, 4, 0)); // lfs f3, 0(r4)
    cpu.step(&mut mem, d_form(52, 3, 4, 4)); // stfs f3, 4(r4)
                                             // First 4 bytes = original; next 4 bytes = round-tripped.
    assert_eq!(mem.data[0..4], original.to_bits().to_be_bytes());
    assert_eq!(mem.data[4..8], original.to_bits().to_be_bytes());
}

#[test]
fn decode_lwzu_extracts_d_form_fields() {
    let word = d_form(33, 3, 1, 4);
    assert_eq!(decode(word), Ok(PpcInstr::Lwzu { rt: 3, ra: 1, d: 4 }));
}

#[test]
fn decode_lbzu_extracts_d_form_fields() {
    let word = d_form(35, 3, 1, 1);
    assert_eq!(decode(word), Ok(PpcInstr::Lbzu { rt: 3, ra: 1, d: 1 }));
}

#[test]
fn decode_lhzu_extracts_d_form_fields() {
    let word = d_form(41, 3, 1, 2);
    assert_eq!(decode(word), Ok(PpcInstr::Lhzu { rt: 3, ra: 1, d: 2 }));
}

#[test]
fn decode_lha_extracts_d_form_fields() {
    let word = d_form(42, 3, 1, 0);
    assert_eq!(decode(word), Ok(PpcInstr::Lha { rt: 3, ra: 1, d: 0 }));
}

#[test]
fn decode_lhau_extracts_d_form_fields() {
    let word = d_form(43, 3, 1, 2);
    assert_eq!(decode(word), Ok(PpcInstr::Lhau { rt: 3, ra: 1, d: 2 }));
}

#[test]
fn decode_stbu_extracts_d_form_fields() {
    let word = d_form(39, 3, 1, 1);
    assert_eq!(decode(word), Ok(PpcInstr::Stbu { rs: 3, ra: 1, d: 1 }));
}

#[test]
fn decode_sthu_extracts_d_form_fields() {
    let word = d_form(45, 3, 1, 2);
    assert_eq!(decode(word), Ok(PpcInstr::Sthu { rs: 3, ra: 1, d: 2 }));
}

#[test]
fn step_lwzu_loads_and_updates_ra() {
    // lwzu r3, 4(r4) with GPR4=0x1000 → reads 0x1004,
    // GPR3 = value, GPR4 = 0x1004 (atomic with the load).
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x00, 0x00, 0x00, 0x00, 0xCA, 0xFE, 0xBA, 0xBE],
    };
    cpu.step(&mut mem, d_form(33, 3, 4, 4));
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);
    assert_eq!(cpu.gpr[4], 0x1004);
}

#[test]
fn step_lwzu_with_ra_eq_0_surfaces_illegal_instruction() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x1000;
    let mut mem = VecMem {
        base: 0,
        data: vec![0u8; 8],
    };
    let word = d_form(33, 3, 0, 0);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x1000);
}

#[test]
fn step_lwzu_with_ra_eq_rt_surfaces_illegal_instruction() {
    // lwzu r3, 0(r3) — RA equals RT, instruction "form invalid".
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    cpu.pc = 0x100;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    let word = d_form(33, 3, 3, 0);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_lha_sign_extends_loaded_halfword() {
    // lha r3, 0(r4) with mem[0x1000..]=[0xFF, 0xFF] → r3 =
    // sign-extend(0xFFFF) = 0xFFFFFFFF.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xFF, 0xFF],
    };
    cpu.step(&mut mem, d_form(42, 3, 4, 0));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFF);
}

#[test]
fn step_lha_zero_extends_positive_halfword() {
    // 0x7FFF (positive max signed halfword) → 0x00007FFF.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x7F, 0xFF],
    };
    cpu.step(&mut mem, d_form(42, 3, 4, 0));
    assert_eq!(cpu.gpr[3], 0x0000_7FFF);
}

#[test]
fn step_lhau_loads_signed_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x00, 0x00, 0xFF, 0xFE], // (lhau jumps to +2)
    };
    cpu.step(&mut mem, d_form(43, 3, 4, 2));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFE); // sign-extended -2
    assert_eq!(cpu.gpr[4], 0x1002); // RA updated
}

#[test]
fn step_stbu_writes_byte_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    cpu.step(&mut mem, d_form(39, 3, 4, 2));
    assert_eq!(mem.data, vec![0x00, 0x00, 0xBE, 0x00]);
    assert_eq!(cpu.gpr[4], 0x1002);
}

#[test]
fn step_sthu_writes_halfword_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x0000_BEEF;
    cpu.gpr[4] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    cpu.step(&mut mem, d_form(45, 3, 4, 2));
    assert_eq!(mem.data, vec![0x00, 0x00, 0xBE, 0xEF]);
    assert_eq!(cpu.gpr[4], 0x1002);
}

#[test]
fn step_stbu_with_ra_eq_0_surfaces_illegal_instruction() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let mut mem = VecMem {
        base: 0,
        data: vec![0u8; 4],
    };
    let word = d_form(39, 3, 0, 0);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn decode_lmw_extracts_d_form_fields() {
    let word = d_form(46, 28, 1, 16);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lmw {
            rt: 28,
            ra: 1,
            d: 16
        })
    );
}

#[test]
fn decode_stmw_extracts_d_form_fields() {
    let word = d_form(47, 24, 1, 0xFFE0);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stmw {
            rs: 24,
            ra: 1,
            d: -32
        })
    );
}

#[test]
fn step_stmw_writes_consecutive_words() {
    // stmw r28, 0(r1) stores GPR28..GPR31 at SP+0..SP+12.
    let mut cpu = PpcCpu::new();
    cpu.gpr[1] = 0x1000;
    cpu.gpr[28] = 0x1111_1111;
    cpu.gpr[29] = 0x2222_2222;
    cpu.gpr[30] = 0x3333_3333;
    cpu.gpr[31] = 0x4444_4444;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 16],
    };
    cpu.step(&mut mem, d_form(47, 28, 1, 0));
    assert_eq!(
        mem.data,
        vec![
            0x11, 0x11, 0x11, 0x11, 0x22, 0x22, 0x22, 0x22, 0x33, 0x33, 0x33, 0x33, 0x44, 0x44,
            0x44, 0x44,
        ]
    );
}

#[test]
fn step_lmw_reads_consecutive_words_into_gprs() {
    // lmw r28, 0(r1) loads GPR28..GPR31 from SP+0..SP+12.
    let mut cpu = PpcCpu::new();
    cpu.gpr[1] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![
            0x11, 0x11, 0x11, 0x11, 0x22, 0x22, 0x22, 0x22, 0x33, 0x33, 0x33, 0x33, 0x44, 0x44,
            0x44, 0x44,
        ],
    };
    cpu.step(&mut mem, d_form(46, 28, 1, 0));
    assert_eq!(cpu.gpr[28], 0x1111_1111);
    assert_eq!(cpu.gpr[29], 0x2222_2222);
    assert_eq!(cpu.gpr[30], 0x3333_3333);
    assert_eq!(cpu.gpr[31], 0x4444_4444);
}

#[test]
fn step_stmw_lmw_round_trip_preserves_register_set() {
    // Save then restore the four highest GPRs through a
    // stack-style buffer.
    let mut cpu = PpcCpu::new();
    cpu.gpr[1] = 0x1000;
    cpu.gpr[28] = 0xCAFE_BABE;
    cpu.gpr[29] = 0xDEAD_BEEF;
    cpu.gpr[30] = 0x1234_5678;
    cpu.gpr[31] = 0x9876_5432;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 16],
    };
    // stmw r28, 0(r1)
    cpu.step(&mut mem, d_form(47, 28, 1, 0));
    // Clobber the registers.
    cpu.gpr[28] = 0;
    cpu.gpr[29] = 0;
    cpu.gpr[30] = 0;
    cpu.gpr[31] = 0;
    // lmw r28, 0(r1)
    cpu.step(&mut mem, d_form(46, 28, 1, 0));
    assert_eq!(cpu.gpr[28], 0xCAFE_BABE);
    assert_eq!(cpu.gpr[29], 0xDEAD_BEEF);
    assert_eq!(cpu.gpr[30], 0x1234_5678);
    assert_eq!(cpu.gpr[31], 0x9876_5432);
}

#[test]
fn step_lmw_with_ra_in_loaded_range_surfaces_illegal_instruction() {
    // lmw r1, 0(r1) — RA=1 IS in the loaded range
    // [1..=31]. Per spec, "invalid form".
    let mut cpu = PpcCpu::new();
    cpu.gpr[1] = 0x1000;
    cpu.pc = 0x100;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 128],
    };
    let word = d_form(46, 1, 1, 0);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn decode_lwzx_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 23, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lwzx {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_lbzx_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 87, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lbzx {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_lhzx_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 279, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lhzx {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_stwx_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 151, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stwx {
            rs: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_stbx_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 215, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stbx {
            rs: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_sthx_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 407, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Sthx {
            rs: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_byte_reversed_indexed_memory_extracts_x_form_fields() {
    assert_eq!(
        decode(x_form(31, 3, 4, 5, 534, false)),
        Ok(PpcInstr::Lwbrx {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
    assert_eq!(
        decode(x_form(31, 3, 4, 5, 790, false)),
        Ok(PpcInstr::Lhbrx {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
    assert_eq!(
        decode(x_form(31, 3, 4, 5, 662, false)),
        Ok(PpcInstr::Stwbrx {
            rs: 3,
            ra: 4,
            rb: 5
        })
    );
    assert_eq!(
        decode(x_form(31, 3, 4, 5, 918, false)),
        Ok(PpcInstr::Sthbrx {
            rs: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_byte_reversed_indexed_memory_rejects_record_forms() {
    for word in [
        x_form(31, 3, 4, 5, 534, true),
        x_form(31, 3, 4, 5, 790, true),
        x_form(31, 3, 4, 5, 662, true),
        x_form(31, 3, 4, 5, 918, true),
    ] {
        let secondary = ((word >> 1) & 0x3FF) as u16;
        assert_eq!(
            decode(word),
            Err(PpcDecodeError::UnsupportedSecondaryOpcode {
                primary: 31,
                secondary
            })
        );
    }
}

#[test]
fn decode_lwarx_stwcx_extract_x_form_fields() {
    assert_eq!(
        decode(x_form(31, 3, 4, 5, 20, false)),
        Ok(PpcInstr::Lwarx {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
    assert_eq!(
        decode(x_form(31, 6, 4, 5, 150, true)),
        Ok(PpcInstr::Stwcx {
            rs: 6,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_lwarx_stwcx_reject_invalid_record_bit() {
    assert_eq!(
        decode(x_form(31, 3, 4, 5, 20, true)),
        Err(PpcDecodeError::UnsupportedSecondaryOpcode {
            primary: 31,
            secondary: 20
        })
    );
    assert_eq!(
        decode(x_form(31, 6, 4, 5, 150, false)),
        Err(PpcDecodeError::UnsupportedSecondaryOpcode {
            primary: 31,
            secondary: 150
        })
    );
}

#[test]
fn step_lwzx_reads_at_ra_plus_rb() {
    // lwzx r3, r4, r5 with GPR4=0x1000, GPR5=4 → reads
    // mem32_be[0x1004].
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 4;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x00, 0x00, 0x00, 0x00, 0xCA, 0xFE, 0xBA, 0xBE],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 23, false));
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);
}

#[test]
fn step_lwzx_with_ra_eq_0_uses_literal_zero_base() {
    // lwzx r3, 0, r5 with GPR0=0xDEADBEEF, GPR5=0x1000 →
    // EA = 0 + GPR5 = 0x1000, NOT 0xDEADBEEF + 0x1000.
    let mut cpu = PpcCpu::new();
    cpu.gpr[0] = 0xDEAD_BEEF;
    cpu.gpr[5] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x12, 0x34, 0x56, 0x78],
    };
    cpu.step(&mut mem, x_form(31, 3, 0, 5, 23, false));
    assert_eq!(cpu.gpr[3], 0x1234_5678);
}

#[test]
fn step_lwarx_then_stwcx_success_writes_word_and_sets_eq() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 4;
    cpu.gpr[6] = 0x1234_5678;
    cpu.xer = 1 << 31;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0, 0, 0, 0, 0xCA, 0xFE, 0xBA, 0xBE],
    };

    let load_res = cpu.step(&mut mem, x_form(31, 3, 4, 5, 20, false));
    assert_eq!(load_res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);

    let store_res = cpu.step(&mut mem, x_form(31, 6, 4, 5, 150, true));
    assert_eq!(store_res, PpcStepResult::Stepped);

    assert_eq!(&mem.data[4..8], &[0x12, 0x34, 0x56, 0x78]);
    assert_eq!(cpu.cr_field(0), 0b0011);
    assert_eq!(cpu.pc, 8);
}

#[test]
fn step_stwcx_without_reservation_fails_and_does_not_write() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x200;
    cpu.gpr[3] = 0x1234_5678;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0;
    cpu.xer = 1 << 31;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xCA, 0xFE, 0xBA, 0xBE],
    };

    let res = cpu.step(&mut mem, x_form(31, 3, 4, 5, 150, true));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(mem.data, vec![0xCA, 0xFE, 0xBA, 0xBE]);
    assert_eq!(cpu.cr_field(0), 0b0001);
    assert_eq!(cpu.pc, 0x204);
}

#[test]
fn step_stwcx_mismatched_reservation_fails_and_clears_reservation() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0;
    cpu.gpr[6] = 4;
    cpu.gpr[7] = 0x1122_3344;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xAA, 0xAA, 0xAA, 0xAA, 0xBB, 0xBB, 0xBB, 0xBB],
    };

    assert_eq!(
        cpu.step(&mut mem, x_form(31, 3, 4, 5, 20, false)),
        PpcStepResult::Stepped
    );
    assert_eq!(
        cpu.step(&mut mem, x_form(31, 7, 4, 6, 150, true)),
        PpcStepResult::Stepped
    );
    assert_eq!(cpu.cr_field(0), 0);

    cpu.gpr[6] = 0;
    assert_eq!(
        cpu.step(&mut mem, x_form(31, 7, 4, 6, 150, true)),
        PpcStepResult::Stepped
    );

    assert_eq!(&mem.data[0..4], &[0xAA, 0xAA, 0xAA, 0xAA]);
    assert_eq!(&mem.data[4..8], &[0xBB, 0xBB, 0xBB, 0xBB]);
    assert_eq!(cpu.cr_field(0), 0);
    assert_eq!(cpu.pc, 12);
}

#[test]
fn step_lbzx_zero_extends_loaded_byte() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_FFFF;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 2;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x11, 0x22, 0xAB, 0x44],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 87, false));
    assert_eq!(cpu.gpr[3], 0x0000_00AB);
}

#[test]
fn step_lhzx_reads_big_endian_halfword() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xFFFF_0000;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xCA, 0xFE],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 279, false));
    assert_eq!(cpu.gpr[3], 0x0000_CAFE);
}

#[test]
fn step_lwbrx_loads_byte_reversed_word() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 4;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0, 0, 0, 0, 0x12, 0x34, 0x56, 0x78],
    };

    cpu.step(&mut mem, x_form(31, 3, 4, 5, 534, false));

    assert_eq!(cpu.gpr[3], 0x7856_3412);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_lhbrx_loads_byte_reversed_halfword_with_ra_zero_base() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[0] = 0xDEAD_BEEF;
    cpu.gpr[5] = 0x1000;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0x12, 0x34],
    };

    cpu.step(&mut mem, x_form(31, 3, 0, 5, 790, false));

    assert_eq!(cpu.gpr[3], 0x0000_3412);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_stwx_writes_at_ra_plus_rb() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 4;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 151, false));
    assert_eq!(&mem.data[4..8], &[0xCA, 0xFE, 0xBA, 0xBE]);
    assert_eq!(&mem.data[0..4], &[0u8; 4]); // unchanged
}

#[test]
fn step_stbx_writes_low_byte() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xDEAD_BEEF;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 1],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 215, false));
    assert_eq!(mem.data, vec![0xEF]);
}

#[test]
fn decode_lhax_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 343, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lhax {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_lwzux_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 55, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lwzux {
            rt: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn decode_stwux_extracts_x_form_fields() {
    let word = x_form(31, 3, 4, 5, 183, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Stwux {
            rs: 3,
            ra: 4,
            rb: 5
        })
    );
}

#[test]
fn step_lhax_sign_extends_loaded_halfword() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xFF, 0xFE],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 343, false));
    assert_eq!(cpu.gpr[3], 0xFFFF_FFFE);
}

#[test]
fn step_lwzux_loads_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 4;
    let mut mem = VecMem {
        base: 0x1000,
        data: {
            let mut v = vec![0u8; 4];
            v.extend_from_slice(&0xCAFEBABEu32.to_be_bytes());
            v
        },
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 55, false));
    assert_eq!(cpu.gpr[3], 0xCAFE_BABE);
    assert_eq!(cpu.gpr[4], 0x1004);
}

#[test]
fn step_stwux_writes_and_updates_ra() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1234_5678;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 4;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 183, false));
    assert_eq!(&mem.data[4..8], &[0x12, 0x34, 0x56, 0x78]);
    assert_eq!(cpu.gpr[4], 0x1004);
}

#[test]
fn step_lwzux_with_ra_eq_0_surfaces_illegal_instruction() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    let mut mem = VecMem {
        base: 0,
        data: vec![0u8; 8],
    };
    let word = x_form(31, 3, 0, 5, 55, false);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_stwux_with_ra_eq_0_surfaces_illegal_instruction() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[3] = 0x1234_5678;
    let mut mem = VecMem {
        base: 0,
        data: vec![0u8; 8],
    };
    let word = x_form(31, 3, 0, 5, 183, false);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(mem.data, vec![0u8; 8]);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_sthx_writes_low_halfword() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 2],
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 407, false));
    assert_eq!(mem.data, vec![0xBA, 0xBE]);
}

#[test]
fn step_stwbrx_stores_byte_reversed_word() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1234_5678;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 4;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };

    cpu.step(&mut mem, x_form(31, 3, 4, 5, 662, false));

    assert_eq!(&mem.data[4..8], &[0x78, 0x56, 0x34, 0x12]);
    assert_eq!(&mem.data[0..4], &[0u8; 4]);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_sthbrx_stores_low_halfword_byte_reversed() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 2],
    };

    cpu.step(&mut mem, x_form(31, 3, 4, 5, 918, false));

    assert_eq!(mem.data, vec![0xBE, 0xBA]);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_lwbrx_unaligned_address_surfaces_alignment_exception() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[4] = 0x1002;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };

    let res = cpu.step(&mut mem, x_form(31, 3, 4, 5, 534, false));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::Alignment {
            addr: 0x1002,
            size: 4,
            access: PpcMemoryAccess::Load
        })
    );
    assert_eq!(cpu.gpr[3], 0);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_lwarx_unaligned_address_surfaces_alignment_exception() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[4] = 0x1002;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };

    let res = cpu.step(&mut mem, x_form(31, 3, 4, 5, 20, false));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::Alignment {
            addr: 0x1002,
            size: 4,
            access: PpcMemoryAccess::Load
        })
    );
    assert_eq!(cpu.gpr[3], 0);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_sthbrx_unaligned_address_surfaces_alignment_exception_without_write() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1001;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };

    let res = cpu.step(&mut mem, x_form(31, 3, 4, 5, 918, false));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::Alignment {
            addr: 0x1001,
            size: 2,
            access: PpcMemoryAccess::Store
        })
    );
    assert_eq!(mem.data, vec![0u8; 4]);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_stwcx_unaligned_address_surfaces_alignment_exception_without_write() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.cr = 0xA000_0000;
    cpu.gpr[3] = 0xCAFE_BABE;
    cpu.gpr[4] = 0x1002;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 8],
    };

    let res = cpu.step(&mut mem, x_form(31, 3, 4, 5, 150, true));

    assert_eq!(
        res,
        PpcStepResult::Exception(PpcException::Alignment {
            addr: 0x1002,
            size: 4,
            access: PpcMemoryAccess::Store
        })
    );
    assert_eq!(mem.data, vec![0u8; 8]);
    assert_eq!(cpu.cr, 0xA000_0000);
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn step_lwzx_unmapped_address_surfaces_memory_fault() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[4] = 0x9000;
    cpu.gpr[5] = 0;
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0u8; 4],
    };
    let res = cpu.step(&mut mem, x_form(31, 3, 4, 5, 23, false));
    assert!(matches!(
        res,
        PpcStepResult::MemoryFault {
            addr: 0x9000,
            was_write: false
        }
    ));
    assert_eq!(cpu.pc, 0x100);
}

#[test]
fn ppc_section_mem_routes_reads_to_the_correct_region() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);
    mem.add_region(0x4000, vec![0xCC, 0xDD]);
    assert_eq!(mem.read_u8(0x1000), Some(0xAA));
    assert_eq!(mem.read_u8(0x1001), Some(0xBB));
    assert_eq!(mem.read_u8(0x4000), Some(0xCC));
    assert_eq!(mem.read_u8(0x4001), Some(0xDD));
}

#[test]
fn ppc_section_mem_newer_overlays_win_after_cached_reads() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    assert_eq!(mem.read_u32_be(0x1000), Some(0xAABB_CCDD));

    mem.add_region(0x1001, vec![0x11, 0x22]);

    assert_eq!(mem.read_u8(0x1001), Some(0x11));
    assert_eq!(mem.read_u16_be(0x1001), Some(0x1122));
    assert_eq!(mem.read_u32_be(0x1000), Some(0xAA11_22DD));
}

#[test]
fn ppc_section_mem_overlap_span_cache_preserves_newest_region_priority() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA; 0x100]);
    mem.add_region(0x1080, vec![0xBB; 0x10]);

    assert_eq!(mem.read_u8(0x1000), Some(0xAA));
    assert_eq!(mem.read_u8(0x107F), Some(0xAA));
    assert_eq!(mem.read_u8(0x1080), Some(0xBB));
    assert_eq!(mem.read_u8(0x108F), Some(0xBB));
    assert_eq!(mem.read_u8(0x1090), Some(0xAA));
    assert_eq!(mem.read_u8(0x10FF), Some(0xAA));
    assert_eq!(mem.read_u8(0x1080), Some(0xBB));
}

#[test]
fn ppc_section_mem_instruction_fetch_cache_is_invalidated_by_new_overlays() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x1000, 0x6000_0000u32.to_be_bytes().to_vec());
    assert_eq!(mem.read_instruction_u32_be(0x1000), Some(0x6000_0000));
    assert_eq!(mem.read_instruction_u32_be(0x1000), Some(0x6000_0000));

    mem.add_readonly_region(0x1000, 0x4E80_0020u32.to_be_bytes().to_vec());

    assert_eq!(mem.read_instruction_u32_be(0x1000), Some(0x4E80_0020));
}

#[test]
fn ppc_section_mem_instruction_fetch_preserves_overlays_within_a_word() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x1000, vec![0x60, 0x00, 0x00, 0x00]);
    mem.add_readonly_region(0x1002, vec![0x12, 0x34]);

    assert_eq!(mem.read_instruction_u32_be(0x1000), Some(0x6000_1234));
    assert_eq!(mem.read_instruction_u32_be(0x1000), Some(0x6000_1234));
}

#[test]
fn ppc_section_mem_does_not_cache_instruction_fetches_from_writable_memory() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, 0x6000_0000u32.to_be_bytes().to_vec());
    assert_eq!(mem.read_instruction_u32_be(0x1000), Some(0x6000_0000));

    mem.write_u32_be(0x1000, 0x4E80_0020).unwrap();

    assert_eq!(mem.read_instruction_u32_be(0x1000), Some(0x4E80_0020));
}

#[test]
fn ppc_section_mem_returns_none_outside_any_region() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);
    assert_eq!(mem.read_u8(0x0FFF), None); // before first region
    assert_eq!(mem.read_u8(0x1002), None); // just past end
    assert_eq!(mem.read_u8(0x4000), None); // gap to nothing
}

#[test]
fn ppc_section_mem_read_bytes_into_copies_single_region_ranges() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    let mut dst = [0u8; 3];

    assert_eq!(mem.read_bytes_into(0x1001, &mut dst), Some(()));

    assert_eq!(dst, [0xBB, 0xCC, 0xDD]);
}

#[test]
fn ppc_section_mem_read_bytes_into_can_cross_adjacent_regions() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);
    mem.add_region(0x1002, vec![0xCC, 0xDD]);
    let mut dst = [0u8; 4];

    assert_eq!(mem.read_bytes_into(0x1000, &mut dst), Some(()));

    assert_eq!(dst, [0xAA, 0xBB, 0xCC, 0xDD]);
}

#[test]
fn ppc_section_mem_word_reads_can_cross_adjacent_regions() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);
    mem.add_region(0x1002, vec![0xCC, 0xDD, 0xEE, 0xFF]);

    assert_eq!(mem.read_u32_be(0x1000), Some(0xAABB_CCDD));
    assert_eq!(mem.read_u64_be(0x1000), None);
}

#[test]
fn ppc_section_mem_read_bytes_into_rejects_unmapped_ranges() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);
    let mut dst = [0u8; 3];

    assert_eq!(mem.read_bytes_into(0x1000, &mut dst), None);
}

#[test]
fn ppc_section_mem_write_bytes_copies_single_region_ranges() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);

    assert_eq!(mem.write_bytes(0x1001, &[0x11, 0x22]), Some(()));

    assert_eq!(mem.read_u32_be(0x1000), Some(0xAA11_22DD));
}

#[test]
fn ppc_section_mem_write_bytes_can_cross_adjacent_regions() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);
    mem.add_region(0x1002, vec![0xCC, 0xDD]);

    assert_eq!(mem.write_bytes(0x1001, &[0x11, 0x22]), Some(()));

    assert_eq!(mem.read_u32_be(0x1000), Some(0xAA11_22DD));
}

#[test]
fn ppc_section_mem_write_bytes_rejects_unmapped_ranges_without_partial_writes() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);

    assert_eq!(mem.write_bytes(0x1000, &[0x11, 0x22, 0x33]), None);

    assert_eq!(mem.read_u16_be(0x1000), Some(0xAABB));
}

#[test]
fn ppc_section_mem_write_bytes_rejects_readonly_ranges_without_partial_writes() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB]);
    mem.add_readonly_region(0x1002, vec![0xCC, 0xDD]);

    assert_eq!(mem.write_bytes(0x1001, &[0x11, 0x22]), None);

    assert_eq!(mem.read_u32_be(0x1000), Some(0xAABB_CCDD));
}

#[test]
fn ppc_section_mem_writable_span_reads_and_writes_with_cached_offsets() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    let span = mem.writable_span(0x1001, 3).unwrap();

    assert_eq!(mem.read_u16_be_in_span(span, 0), Some(0xBBCC));
    assert_eq!(mem.write_u16_be_in_span(span, 1, 0x1122), Some(()));

    assert_eq!(mem.read_u32_be(0x1000), Some(0xAABB_1122));
}

#[test]
fn ppc_section_mem_writable_span_rejects_readonly_and_out_of_span_offsets() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x1000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    assert_eq!(mem.writable_span(0x1000, 4), None);

    let mut mem = PpcSectionMem::new();
    mem.add_region(0x2000, vec![0xAA, 0xBB, 0xCC, 0xDD]);
    let span = mem.writable_span(0x2000, 2).unwrap();
    assert_eq!(mem.read_u16_be_in_span(span, 1), None);
    assert_eq!(mem.write_u16_be_in_span(span, 1, 0x1122), None);
    assert_eq!(mem.read_u32_be(0x2000), Some(0xAABB_CCDD));
}

#[test]
fn ppc_section_mem_readonly_region_rejects_writes() {
    let mut mem = PpcSectionMem::new();
    mem.add_readonly_region(0x1000, vec![0xAA, 0xBB]);
    assert_eq!(mem.read_u8(0x1000), Some(0xAA));
    // Write must be rejected.
    assert_eq!(mem.write_u8(0x1000, 0xFF), None);
    // Underlying byte unchanged.
    assert_eq!(mem.read_u8(0x1000), Some(0xAA));
}

#[test]
fn ppc_section_mem_writable_region_accepts_writes() {
    let mut mem = PpcSectionMem::new();
    mem.add_region(0x1000, vec![0u8; 4]);
    mem.write_u32_be(0x1000, 0xCAFE_BABE).unwrap();
    assert_eq!(mem.read_u32_be(0x1000), Some(0xCAFE_BABE));
}

#[test]
fn step_lswi_loads_consecutive_bytes_into_consecutive_gprs() {
    // lswi r5, r3, 7 — load 7 bytes from EA (r3) into r5..r6.
    // First 4 bytes pack into r5 big-endian; remaining 3 bytes
    // pack into r6 right-padded with zero.
    let mut mem = VecMem {
        base: 0x1000,
        data: vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x11, 0x00],
    };
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    // Encoding: opcd=31, RT=5, RA=3, NB=7, XO=597, Rc=0.
    let word = (31u32 << 26) | (5u32 << 21) | (3u32 << 16) | (7u32 << 11) | (597u32 << 1);
    cpu.step(&mut mem, word);
    assert_eq!(cpu.gpr[5], 0xAABB_CCDD);
    assert_eq!(cpu.gpr[6], 0xEEFF_1100);
}

#[test]
fn step_lswi_with_nb_zero_loads_32_bytes() {
    // NB == 0 is the special-case 32. 32 bytes = 8 registers.
    let mut data: Vec<u8> = Vec::with_capacity(32);
    for i in 0..32u8 {
        data.push(i.wrapping_add(0x40));
    }
    let mut mem = VecMem { base: 0x2000, data };
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x2000;
    // RT=8, RA=3, NB=0.
    let word = (31u32 << 26) | (8u32 << 21) | (3u32 << 16) | (597u32 << 1);
    cpu.step(&mut mem, word);
    // r8..r15 each receive 4 consecutive bytes.
    for k in 0..8u32 {
        let expected = ((0x40 + k * 4) << 24)
            | ((0x41 + k * 4) << 16)
            | ((0x42 + k * 4) << 8)
            | (0x43 + k * 4);
        assert_eq!(cpu.gpr[(8 + k) as usize], expected, "r{}", 8 + k);
    }
}

#[test]
fn step_lswi_wraps_register_index_after_31() {
    // RT=30, NB=12 → 3 registers needed: r30, r31, r0 (wrap).
    let mut mem = VecMem {
        base: 0x3000,
        data: vec![
            0x10, 0x11, 0x12, 0x13, 0x20, 0x21, 0x22, 0x23, 0x30, 0x31, 0x32, 0x33,
        ],
    };
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x3000;
    let word = (31u32 << 26) | (30u32 << 21) | (3u32 << 16) | (12u32 << 11) | (597u32 << 1);
    cpu.step(&mut mem, word);
    assert_eq!(cpu.gpr[30], 0x1011_1213);
    assert_eq!(cpu.gpr[31], 0x2021_2223);
    assert_eq!(cpu.gpr[0], 0x3031_3233);
}

#[test]
fn decode_lswx_stswx_extract_x_form_fields() {
    assert_eq!(
        decode(x_form(31, 5, 3, 7, 533, false)),
        Ok(PpcInstr::Lswx {
            rt: 5,
            ra: 3,
            rb: 7
        })
    );
    assert_eq!(
        decode(x_form(31, 6, 4, 8, 661, false)),
        Ok(PpcInstr::Stswx {
            rs: 6,
            ra: 4,
            rb: 8
        })
    );
}

#[test]
fn decode_lswx_stswx_reject_record_forms() {
    assert_eq!(
        decode(x_form(31, 5, 3, 7, 533, true)),
        Err(PpcDecodeError::UnsupportedSecondaryOpcode {
            primary: 31,
            secondary: 533
        })
    );
    assert_eq!(
        decode(x_form(31, 6, 4, 8, 661, true)),
        Err(PpcDecodeError::UnsupportedSecondaryOpcode {
            primary: 31,
            secondary: 661
        })
    );
}

#[test]
fn step_lswx_uses_xer_count_and_indexed_address() {
    let mut mem = VecMem {
        base: 0x5000,
        data: vec![0, 0, 0, 0, 0xA0, 0xA1, 0xA2, 0xA3, 0xB0, 0xB1],
    };
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x5000;
    cpu.gpr[7] = 4;
    cpu.gpr[5] = 0xFFFF_FFFF;
    cpu.gpr[6] = 0xFFFF_FFFF;
    cpu.xer = 6;

    cpu.step(&mut mem, x_form(31, 5, 3, 7, 533, false));

    assert_eq!(cpu.gpr[5], 0xA0A1_A2A3);
    assert_eq!(cpu.gpr[6], 0xB0B1_0000);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_lswx_xer_count_zero_is_noop_without_memory_access() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[3] = 0x9000;
    cpu.gpr[7] = 0x100;
    cpu.gpr[5] = 0xCAFE_BABE;
    cpu.xer = 0;

    let res = cpu.step_instruction(x_form(31, 5, 3, 7, 533, false));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.gpr[5], 0xCAFE_BABE);
    assert_eq!(cpu.pc, 0x104);
}

#[test]
fn step_stswx_uses_xer_count_and_indexed_address() {
    let mut mem = VecMem {
        base: 0x6000,
        data: vec![0u8; 12],
    };
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x6000;
    cpu.gpr[8] = 4;
    cpu.gpr[6] = 0x1122_3344;
    cpu.gpr[7] = 0x5566_7788;
    cpu.xer = 6;

    cpu.step(&mut mem, x_form(31, 6, 4, 8, 661, false));

    assert_eq!(&mem.data[0..4], &[0u8; 4]);
    assert_eq!(&mem.data[4..10], &[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
    assert_eq!(&mem.data[10..12], &[0u8; 2]);
    assert_eq!(cpu.pc, 4);
}

#[test]
fn step_stswx_xer_count_zero_is_noop_without_memory_access() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[4] = 0x9000;
    cpu.gpr[8] = 0x100;
    cpu.gpr[6] = 0x1122_3344;
    cpu.xer = 0;

    let res = cpu.step_instruction(x_form(31, 6, 4, 8, 661, false));

    assert_eq!(res, PpcStepResult::Stepped);
    assert_eq!(cpu.pc, 0x104);
}

#[test]
fn step_stswi_round_trips_with_lswi() {
    // Set up source registers, stswi them out, lswi them back
    // into different registers, verify equality.
    let mut mem = VecMem {
        base: 0x4000,
        data: vec![0u8; 16],
    };
    let mut cpu = PpcCpu::new();
    cpu.gpr[5] = 0xCAFE_BABE;
    cpu.gpr[6] = 0xDEAD_BEEF;
    cpu.gpr[7] = 0x1234_5678;
    cpu.gpr[10] = 0x4000;
    // stswi r5, r10, 12
    let stsw = (31u32 << 26) | (5u32 << 21) | (10u32 << 16) | (12u32 << 11) | (725u32 << 1);
    cpu.step(&mut mem, stsw);
    assert_eq!(cpu.pc, 4);
    // Bytes in memory should match the source big-endian.
    assert_eq!(mem.data[0..4], [0xCA, 0xFE, 0xBA, 0xBE]);
    assert_eq!(mem.data[4..8], [0xDE, 0xAD, 0xBE, 0xEF]);
    assert_eq!(mem.data[8..12], [0x12, 0x34, 0x56, 0x78]);
    // lswi r20, r10, 12 → reads back into r20..r22.
    cpu.pc = 0;
    let lsw = (31u32 << 26) | (20u32 << 21) | (10u32 << 16) | (12u32 << 11) | (597u32 << 1);
    cpu.step(&mut mem, lsw);
    assert_eq!(cpu.gpr[20], 0xCAFE_BABE);
    assert_eq!(cpu.gpr[21], 0xDEAD_BEEF);
    assert_eq!(cpu.gpr[22], 0x1234_5678);
}

#[test]
fn decode_lfsx_extracts_x_form_fields() {
    // lfsx FRT=3, RA=1, RB=4 — OPCD=31, XO=535.
    let word = x_form(31, 3, 1, 4, 535, false);
    assert_eq!(
        decode(word),
        Ok(PpcInstr::Lfsx {
            frt: 3,
            ra: 1,
            rb: 4
        })
    );
}

#[test]
fn decode_lfsux_lfdx_lfdux_stfsx_stfsux_stfdx_stfdux_extract_fields() {
    assert_eq!(
        decode(x_form(31, 3, 1, 4, 567, false)),
        Ok(PpcInstr::Lfsux {
            frt: 3,
            ra: 1,
            rb: 4
        })
    );
    assert_eq!(
        decode(x_form(31, 3, 1, 4, 599, false)),
        Ok(PpcInstr::Lfdx {
            frt: 3,
            ra: 1,
            rb: 4
        })
    );
    assert_eq!(
        decode(x_form(31, 3, 1, 4, 631, false)),
        Ok(PpcInstr::Lfdux {
            frt: 3,
            ra: 1,
            rb: 4
        })
    );
    assert_eq!(
        decode(x_form(31, 5, 1, 4, 663, false)),
        Ok(PpcInstr::Stfsx {
            frs: 5,
            ra: 1,
            rb: 4
        })
    );
    assert_eq!(
        decode(x_form(31, 5, 1, 4, 695, false)),
        Ok(PpcInstr::Stfsux {
            frs: 5,
            ra: 1,
            rb: 4
        })
    );
    assert_eq!(
        decode(x_form(31, 5, 1, 4, 727, false)),
        Ok(PpcInstr::Stfdx {
            frs: 5,
            ra: 1,
            rb: 4
        })
    );
    assert_eq!(
        decode(x_form(31, 5, 1, 4, 759, false)),
        Ok(PpcInstr::Stfdux {
            frs: 5,
            ra: 1,
            rb: 4
        })
    );
}

#[test]
fn step_lfsx_loads_single_at_indexed_addr_and_widens() {
    // EA = GPR[4] + GPR[5]. Memory at EA contains f32 bits for 7.25.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0x10;
    let bits = 7.25f32.to_bits();
    let mut mem = VecMem {
        base: 0x1010,
        data: bits.to_be_bytes().to_vec(),
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 535, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 7.25);
}

#[test]
fn step_lfsux_updates_ra_to_ea() {
    // EA = GPR[4] + GPR[5]; RA := EA.
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0x10;
    let bits = 1.5f32.to_bits();
    let mut mem = VecMem {
        base: 0x1010,
        data: bits.to_be_bytes().to_vec(),
    };
    cpu.step(&mut mem, x_form(31, 3, 4, 5, 567, false));
    assert_eq!(f64::from_bits(cpu.fpr[3]), 1.5);
    assert_eq!(cpu.gpr[4], 0x1010);
}

#[test]
fn step_lfsux_with_ra_eq_0_surfaces_illegal_instruction() {
    let mut cpu = PpcCpu::new();
    cpu.pc = 0x100;
    cpu.gpr[5] = 3;
    let mut mem = VecMem {
        base: 0,
        data: vec![0u8; 8],
    };
    let word = x_form(31, 3, 0, 5, 567, false);
    let res = cpu.step(&mut mem, word);
    assert_illegal_instruction(res, word, PpcIllegalInstructionReason::InvalidForm);
    assert_eq!(cpu.pc, 0x100);
    assert_eq!(cpu.fpr[3], 0);
}

#[test]
fn step_stfdx_writes_double_bit_pattern_at_indexed_addr() {
    let mut cpu = PpcCpu::new();
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 0x8;
    cpu.fpr[6] = 9876.5_f64.to_bits();
    let mut mem = VecMem {
        base: 0x1008,
        data: vec![0u8; 8],
    };
    cpu.step(&mut mem, x_form(31, 6, 4, 5, 727, false));
    let bits = u64::from_be_bytes(mem.data[..8].try_into().unwrap());
    assert_eq!(f64::from_bits(bits), 9876.5);
}

#[test]
fn step_fcmpo_writes_lt_gt_eq_like_fcmpu() {
    // fcmpo cr2, fr3, fr4 — opcd=63, BF=2, FRA=3, FRB=4, XO=32.
    let mut cpu = PpcCpu::new();
    cpu.fpr[3] = 1.0f64.to_bits();
    cpu.fpr[4] = 2.0f64.to_bits();
    let word = (63u32 << 26) | (2u32 << 23) | (3u32 << 16) | (4u32 << 11) | (32u32 << 1);
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(2), 0b1000); // LT

    cpu.fpr[3] = f64::NAN.to_bits();
    cpu.step_instruction(word);
    assert_eq!(cpu.cr_field(2), 0b0001); // UNO
}
