use super::*;

#[test]
fn ppc_fix_ratio_truncates_signed_results_and_saturates_zero_divisors() {
    assert_eq!(ppc_fix_ratio(3, 2), 0x0001_8000);
    assert_eq!(ppc_fix_ratio(-1, 3), -0x0000_5555);
    assert_eq!(ppc_fix_ratio(1, 0), i32::MAX);
    assert_eq!(ppc_fix_ratio(-1, 0), i32::MIN + 1);
}

#[test]
fn ppc_fixed_point_helpers_round_multiply_and_saturate_conversion() {
    assert_eq!(ppc_fix_mul(0x0001_8000, 0x0001_4000), 0x0001_e000);
    assert_eq!(ppc_fix_mul(-0x0001_8000, 0x0001_4000), -0x0001_e000);
    assert_eq!(ppc_long_to_fix(72), 0x0048_0000);
    assert_eq!(ppc_long_to_fix(0x8000), i32::MAX);
    assert_eq!(ppc_long_to_fix(-0x8001), i32::MIN);
}

#[test]
fn ppc_fix_round_rounds_to_nearest_halves_away_from_zero() {
    assert_eq!(ppc_fix_round(0x0001_4000), 1);
    assert_eq!(ppc_fix_round(0x0001_8000), 2);
    assert_eq!(ppc_fix_round(0x0001_c000), 2);
    assert_eq!(ppc_fix_round(-0x0001_4000), -1);
    assert_eq!(ppc_fix_round(-0x0001_8000), -2);
    assert_eq!(ppc_fix_round(i32::MIN), i16::MIN);
    assert_eq!(ppc_fix_round(0), 0);
}

#[test]
fn ppc_fix_and_frac_conversions() {
    assert_eq!(ppc_fix_to_frac(0x0001_0000), 0x4000_0000);
    assert_eq!(ppc_fix_to_frac(-0x0001_0000), -0x4000_0000);
    assert_eq!(ppc_fix_to_frac(0x0002_0000), i32::MAX);
    assert_eq!(ppc_fix_to_frac(-0x0002_0001), i32::MIN);

    assert_eq!(ppc_frac_to_fix(0x4000_0000), 0x0001_0000);
    assert_eq!(ppc_frac_to_fix(0x7000_0000), 0x0001_c000);
    assert_eq!(ppc_frac_to_fix(-0x4000_0000), -0x0001_0000);
    assert_eq!(ppc_frac_to_fix(0x2000_0000), 0x0000_8000);

    assert_eq!(ppc_f64_to_frac(1.0), 0x4000_0000);
    assert_eq!(ppc_f64_to_frac(-1.0), (-0x4000_0000i32) as u32);
    assert_eq!(ppc_f64_to_frac(0.5), 0x2000_0000);
    assert_eq!(ppc_f64_to_frac(3.0), 0x7fff_ffff);
}

#[test]
fn ppc_frac_arithmetic_and_trig() {
    assert_eq!(ppc_frac_mul(0x2000_0000, 0x2000_0000), 0x1000_0000);
    assert_eq!(ppc_frac_mul(-0x2000_0000, 0x2000_0000), -0x1000_0000);
    assert_eq!(
        ppc_frac_mul(0xa000_0000u32 as i32, 0x5333_3333),
        0x8333_3333u32 as i32
    );

    assert_eq!(ppc_frac_div(0x1000_0000, 0x2000_0000), 0x2000_0000);
    assert_eq!(ppc_frac_div(0x7ccc_cccd, 0x5333_3333), 0x6000_0000);
    assert_eq!(
        ppc_frac_div(0x8333_3333u32 as i32, 0x5333_3333),
        0xa000_0000u32 as i32
    );
    assert_eq!(ppc_frac_div(0x4000_0000, 0x1000_0000), i32::MAX);
    assert_eq!(ppc_frac_div(0x1000_0000, 0), i32::MAX);
    assert_eq!(ppc_frac_div(-0x1000_0000, 0), i32::MIN);

    assert_eq!(ppc_frac_sqrt(0x4000_0000), 0x4000_0000);
    assert_eq!(ppc_frac_sqrt(0x1000_0000), 0x2000_0000);
    assert_eq!(ppc_frac_sqrt(0x7d70_a3d7), 0x5999_999a);
    assert_eq!(ppc_frac_sqrt(0), 0);

    assert_eq!(ppc_frac_sin(0), 0);
    assert_eq!(ppc_frac_cos(0), 0x4000_0000);
    let pi_over_2_fixed = 102_944;
    let sin_pi_2 = ppc_frac_sin(pi_over_2_fixed);
    assert_eq!(sin_pi_2, 0x4000_0000);
    let cos_pi_2 = ppc_frac_cos(pi_over_2_fixed);
    assert_eq!(cos_pi_2, 0);
    assert_eq!(ppc_frac_sin(205_888), 0);
    assert_eq!(ppc_frac_cos(205_888), -0x4000_0000);

    assert_eq!(ppc_fix_atan2(1, 0), 0);
    assert_eq!(ppc_fix_atan2(0, 1), 102_944);
    assert_eq!(ppc_fix_atan2(-1, -1), -154_416);
}

#[test]
fn ppc_fixmath_imports_execute_through_synthetic_pefs() {
    // Universal Interfaces 3.4 FixMath.h declares these as InterfaceLib
    // exports; examples and boundaries are from Inside Macintosh IV-65
    // and Operating System Utilities (1994), pp. 3-40 through 3-46.
    let cases: [(
        &str,
        PpcImportDispatcherTarget,
        u32,
        u32,
        Option<f64>,
        Option<u32>,
        Option<f64>,
    ); 11] = [
        (
            "FixRound",
            PpcImportDispatcherTarget::FixRound,
            i32::MIN as u32,
            0,
            None,
            Some(0xffff_8000),
            None,
        ),
        (
            "Fix2Frac",
            PpcImportDispatcherTarget::Fix2Frac,
            0x0001_c000,
            0,
            None,
            Some(0x7000_0000),
            None,
        ),
        (
            "Frac2Fix",
            PpcImportDispatcherTarget::Frac2Fix,
            0x9000_0000,
            0,
            None,
            Some(0xfffe_4000),
            None,
        ),
        (
            "Frac2X",
            PpcImportDispatcherTarget::Frac2X,
            0x7000_0000,
            0,
            None,
            None,
            Some(1.75),
        ),
        (
            "X2Frac",
            PpcImportDispatcherTarget::X2Frac,
            0,
            0,
            Some(-3.0),
            Some(0x8000_0000),
            None,
        ),
        (
            "FracSin",
            PpcImportDispatcherTarget::FracSin,
            205_888,
            0,
            None,
            Some(0),
            None,
        ),
        (
            "FracCos",
            PpcImportDispatcherTarget::FracCos,
            205_888,
            0,
            None,
            Some(0xc000_0000),
            None,
        ),
        (
            "FracSqrt",
            PpcImportDispatcherTarget::FracSqrt,
            0xc000_0000,
            0,
            None,
            Some(0x6ed9_eba1),
            None,
        ),
        (
            "FracMul",
            PpcImportDispatcherTarget::FracMul,
            0x6000_0000,
            0x5333_3333,
            None,
            Some(0x7ccc_cccd),
            None,
        ),
        (
            "FracDiv",
            PpcImportDispatcherTarget::FracDiv,
            0x8333_3333,
            0,
            None,
            Some(0x8000_0000),
            None,
        ),
        (
            "FixATan2",
            PpcImportDispatcherTarget::FixATan2,
            1,
            1,
            None,
            Some(0x0000_c910),
            None,
        ),
    ];

    for (name, target, r3, r4, f1, expected_r3, expected_f1) in cases {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(name.as_bytes()))
            .expect("synthetic FixMath PEF should load");
        assert_eq!(loaded.imports[0].dispatcher_target, target, "{name}");
        loaded.cpu.gpr[3] = r3;
        loaded.cpu.gpr[4] = r4;
        loaded.cpu.fpr[1] = f1.unwrap_or_default().to_bits();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;

        let result = loaded.run_with_hle_imports(64);

        assert_eq!(result.handled_import_count, 1, "{name}");
        if let Some(expected) = expected_r3 {
            assert_eq!(loaded.cpu.gpr[3], expected, "{name}");
        }
        if let Some(expected) = expected_f1 {
            assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), expected, "{name}");
        }
    }
}

#[test]
fn ppc_wide_fixmath_helpers_mutate_records_and_apply_documented_results() {
    // QuickDraw GX Environment and Utilities (1994), pp. 8-49--8-54.
    const TARGET: u32 = 0x2000;
    const SOURCE: u32 = 0x2008;
    const REMAINDER: u32 = 0x2010;
    let mut memory = PpcSectionMem::new();
    memory.add_region(TARGET, vec![0; 0x100]);

    ppc_write_wide(&mut memory, TARGET, 0x0000_0001_ffff_ffff).unwrap();
    ppc_write_wide(&mut memory, SOURCE, 2).unwrap();
    assert_eq!(ppc_wide_add(&mut memory, TARGET, SOURCE), TARGET);
    assert_eq!(
        ppc_read_wide(&mut memory, TARGET),
        Some(0x0000_0002_0000_0001)
    );
    assert_eq!(ppc_wide_subtract(&mut memory, TARGET, SOURCE), TARGET);
    assert_eq!(
        ppc_read_wide(&mut memory, TARGET),
        Some(0x0000_0001_ffff_ffff)
    );
    assert_eq!(ppc_wide_negate(&mut memory, TARGET), TARGET);
    assert_eq!(
        ppc_read_wide(&mut memory, TARGET),
        Some(-0x0000_0001_ffff_ffff)
    );

    ppc_write_wide(&mut memory, TARGET, 7).unwrap();
    assert_eq!(ppc_wide_shift(&mut memory, TARGET, 1), TARGET);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(4));
    ppc_write_wide(&mut memory, TARGET, -7).unwrap();
    ppc_wide_shift(&mut memory, TARGET, 1);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-3));
    ppc_wide_shift(&mut memory, TARGET, -2);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-12));

    ppc_write_wide(&mut memory, TARGET, 7).unwrap();
    assert_eq!(ppc_wide_bit_shift(&mut memory, TARGET, 1), TARGET);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(3));
    ppc_write_wide(&mut memory, TARGET, -7).unwrap();
    ppc_wide_bit_shift(&mut memory, TARGET, 1);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-4));
    ppc_wide_bit_shift(&mut memory, TARGET, -2);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-16));
    ppc_wide_bit_shift(&mut memory, TARGET, 64);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-16));
    ppc_wide_bit_shift(&mut memory, TARGET, 65);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-8));

    assert_eq!(
        ppc_wide_multiply(&mut memory, -2_000_000_000, 2, TARGET),
        TARGET
    );
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(-4_000_000_000));

    ppc_write_wide(&mut memory, TARGET, 10).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 3, REMAINDER), 3);
    assert_eq!(memory.read_u32_be(REMAINDER), Some(1));
    ppc_write_wide(&mut memory, TARGET, 11).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 2, 0), 6);
    ppc_write_wide(&mut memory, TARGET, -11).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 2, 0), -5);
    ppc_write_wide(&mut memory, TARGET, i64::MAX).unwrap();
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 1, REMAINDER), i32::MAX);
    assert_eq!(memory.read_u32_be(REMAINDER), Some(i32::MIN as u32));
    assert_eq!(ppc_wide_divide(&mut memory, TARGET, 1, u32::MAX), i32::MIN);

    ppc_write_wide(&mut memory, TARGET, 11).unwrap();
    assert_eq!(
        ppc_wide_wide_divide(&mut memory, TARGET, 2, REMAINDER),
        TARGET
    );
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(5));
    assert_eq!(memory.read_u32_be(REMAINDER), Some(1));
    ppc_write_wide(&mut memory, TARGET, 11).unwrap();
    ppc_wide_wide_divide(&mut memory, TARGET, 2, 0);
    assert_eq!(ppc_read_wide(&mut memory, TARGET), Some(6));

    ppc_write_wide(&mut memory, TARGET, 81).unwrap();
    assert_eq!(ppc_wide_square_root(&mut memory, TARGET), 9);
    memory.write_u32_be(TARGET, u32::MAX).unwrap();
    memory.write_u32_be(TARGET + 4, u32::MAX).unwrap();
    assert_eq!(ppc_wide_square_root(&mut memory, TARGET), u32::MAX);

    ppc_write_wide(&mut memory, TARGET, -1).unwrap();
    ppc_write_wide(&mut memory, SOURCE, 1).unwrap();
    assert_eq!(ppc_wide_compare(&mut memory, TARGET, SOURCE), -1);
    ppc_write_wide(&mut memory, SOURCE, -1).unwrap();
    assert_eq!(ppc_wide_compare(&mut memory, TARGET, SOURCE), 0);
}

#[test]
fn ppc_wide_fixmath_imports_execute_through_synthetic_pefs() {
    // Universal Interfaces 3.4 FixMath.h declares these as InterfaceLib
    // exports; semantics are from QuickDraw GX Environment and Utilities
    // (1994), pp. 8-49 through 8-54.
    let cases = [
        ("WideAdd", PpcImportDispatcherTarget::WideAdd),
        ("WideSubtract", PpcImportDispatcherTarget::WideSubtract),
        ("WideNegate", PpcImportDispatcherTarget::WideNegate),
        ("WideShift", PpcImportDispatcherTarget::WideShift),
        ("WideBitShift", PpcImportDispatcherTarget::WideBitShift),
        ("WideMultiply", PpcImportDispatcherTarget::WideMultiply),
        ("WideDivide", PpcImportDispatcherTarget::WideDivide),
        ("WideWideDivide", PpcImportDispatcherTarget::WideWideDivide),
        ("WideSquareRoot", PpcImportDispatcherTarget::WideSquareRoot),
        ("WideCompare", PpcImportDispatcherTarget::WideCompare),
    ];

    for (name, target) in cases {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(name.as_bytes()))
            .expect("synthetic Wide FixMath PEF should load");
        assert_eq!(loaded.imports[0].dispatcher_target, target, "{name}");
        let scratch = PPC_DATA_BASE + 0x1000;
        let source = scratch + 8;
        let remainder = scratch + 16;
        loaded.memory.add_region(scratch, vec![0; 0x100]);
        ppc_write_wide(&mut loaded.memory, scratch, 9).unwrap();
        ppc_write_wide(&mut loaded.memory, source, 4).unwrap();
        loaded.cpu.gpr[3] = scratch;
        loaded.cpu.gpr[4] = source;
        loaded.cpu.gpr[5] = remainder;
        match name {
            "WideShift" | "WideBitShift" => loaded.cpu.gpr[4] = 1,
            "WideMultiply" => {
                loaded.cpu.gpr[3] = (-3i32) as u32;
                loaded.cpu.gpr[4] = 7;
                loaded.cpu.gpr[5] = scratch;
            }
            "WideDivide" | "WideWideDivide" => {
                loaded.cpu.gpr[4] = 2;
            }
            "WideSquareRoot" => {}
            _ => {}
        }
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;

        let result = loaded.run_with_hle_imports(64);

        assert_eq!(result.handled_import_count, 1, "{name}");
        match name {
            "WideAdd" => assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(13)),
            "WideSubtract" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(5))
            }
            "WideNegate" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(-9))
            }
            "WideShift" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(5))
            }
            "WideBitShift" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(4))
            }
            "WideMultiply" => {
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(-21))
            }
            "WideDivide" => {
                assert_eq!(loaded.cpu.gpr[3], 4);
                assert_eq!(loaded.memory.read_u32_be(remainder), Some(1));
            }
            "WideWideDivide" => {
                assert_eq!(loaded.cpu.gpr[3], scratch);
                assert_eq!(ppc_read_wide(&mut loaded.memory, scratch), Some(4));
                assert_eq!(loaded.memory.read_u32_be(remainder), Some(1));
            }
            "WideSquareRoot" => assert_eq!(loaded.cpu.gpr[3], 3),
            "WideCompare" => assert_eq!(loaded.cpu.gpr[3], 1),
            _ => unreachable!(),
        }
    }
}

#[test]
fn hle_import_runner_handles_x2fix_in_fpr1() {
    let pef = synthetic_pef_with_import(b"X2Fix");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.fpr[1] = 1.75f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x0001_c000);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.fpr[1] = 40000.0f64.to_bits();
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x7fff_ffff);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.fpr[1] = (-40000.0f64).to_bits();
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x8000_0000);
}

#[test]
fn hle_import_runner_converts_fixed_to_extended_result_register() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"Fix2X")).unwrap();
    loaded.cpu.gpr[3] = (-98_304i32) as u32;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), -1.5);
}
