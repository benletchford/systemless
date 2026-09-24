use super::*;

#[test]
fn native_ppc_math_decimal_and_modf_compatibility_preserve_binary_values() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 64]);
    memory.add_region(0x1100, vec![0; 8]);
    assert!(ppc_decimal_write(&mut memory, 0x1000, -208_000.0, 6));
    assert_eq!(ppc_decimal_read(&mut memory, 0x1000), Some(-208_000.0));

    let mut cpu = PpcCpu::new();
    cpu.fpr[1] = (-12.75f64).to_bits();
    cpu.gpr[5] = 0x1100;
    assert_eq!(
        ppc_dispatch_math_compatibility(
            math_operation("modf"),
            &mut cpu,
            &mut memory,
        ),
        PpcImportAction::ReturnPreserve
    );
    let integer_bits = (u64::from(memory.read_u32_be(0x1100).unwrap()) << 32)
        | u64::from(memory.read_u32_be(0x1104).unwrap());
    assert_eq!(f64::from_bits(integer_bits), -12.0);
    assert_eq!(f64::from_bits(cpu.fpr[1]), -0.75);
}

fn math64_operation(symbol_name: &str) -> PpcMath64Operation {
    match dispatcher_target_for_import("Math64Lib", symbol_name) {
        PpcImportDispatcherTarget::Math64(operation) => operation,
        target => panic!("unexpected Math64 target for {symbol_name}: {target:?}"),
    }
}

fn math_operation(symbol_name: &str) -> PpcMathCompatibilityOperation {
    match dispatcher_target_for_import("MathLib", symbol_name) {
        PpcImportDispatcherTarget::MathCompatibility(operation) => operation,
        target => panic!("unexpected MathLib target for {symbol_name}: {target:?}"),
    }
}

#[test]
fn native_ppc_ldtox80_preserves_double_double_tail() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 32]);
    memory.write_u64_be(0x1000, 1.5f64.to_bits()).unwrap();
    memory
        .write_u64_be(0x1008, (2f64).powi(-60).to_bits())
        .unwrap();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    cpu.gpr[4] = 0x1010;
    assert_eq!(
        ppc_dispatch_math_compatibility(math_operation("ldtox80"), &mut cpu, &mut memory),
        PpcImportAction::ReturnPreserve,
    );
    assert_eq!(memory.read_u16_be(0x1010), Some(0x3fff));
    assert_eq!(memory.read_u64_be(0x1012), Some(0xC000_0000_0000_0008));
}

fn math64_set_gprs(cpu: &mut PpcCpu, high_register: usize, value: u64) {
    cpu.gpr[high_register] = (value >> 32) as u32;
    cpu.gpr[high_register + 1] = value as u32;
}

fn math64_read_memory(memory: &mut PpcSectionMem, address: u32) -> u64 {
    (u64::from(memory.read_u32_be(address).unwrap()) << 32)
        | u64::from(memory.read_u32_be(address + 4).unwrap())
}

#[test]
fn native_ppc_math64_maps_every_captured_export_to_the_specific_dispatcher() {
    let symbols = [
        "LongDoubleToSInt64",
        "LongDoubleToUInt64",
        "S32Set",
        "S64Absolute",
        "S64Add",
        "S64And",
        "S64BitwiseAnd",
        "S64BitwiseEor",
        "S64BitwiseNot",
        "S64BitwiseOr",
        "S64Compare",
        "S64Divide",
        "S64Eor",
        "S64Max",
        "S64Min",
        "S64Multiply",
        "S64Negate",
        "S64Not",
        "S64Or",
        "S64Set",
        "S64SetU",
        "S64ShiftLeft",
        "S64ShiftRight",
        "S64Subtract",
        "SInt64ToLongDouble",
        "SInt64ToUInt64",
        "U32SetU",
        "U64Add",
        "U64And",
        "U64BitwiseAnd",
        "U64BitwiseEor",
        "U64BitwiseNot",
        "U64BitwiseOr",
        "U64Compare",
        "U64Divide",
        "U64Eor",
        "U64Max",
        "U64Multiply",
        "U64Not",
        "U64Or",
        "U64Set",
        "U64SetU",
        "U64ShiftLeft",
        "U64ShiftRight",
        "U64Subtract",
        "UInt64ToLongDouble",
        "UInt64ToSInt64",
    ];
    assert_eq!(symbols.len(), 47);
    for symbol in symbols {
        let PpcImportDispatcherTarget::Math64(operation) =
            dispatcher_target_for_import("Math64Lib", symbol)
        else {
            panic!("{symbol} did not resolve to a Math64 operation");
        };
        assert_eq!(format!("{operation:?}"), symbol);
    }
}

#[test]
fn native_ppc_math_compatibility_maps_each_export_to_a_typed_operation() {
    for (symbol, expected) in [
        ("dec2num", PpcMathCompatibilityOperation::Dec2Num),
        ("dec2str", PpcMathCompatibilityOperation::Dec2Str),
        (
            "feclearexcept",
            PpcMathCompatibilityOperation::FeClearExcept,
        ),
        ("fetestexcept", PpcMathCompatibilityOperation::FeTestExcept),
        ("floor", PpcMathCompatibilityOperation::Floor),
        ("ldtox80", PpcMathCompatibilityOperation::LdToX80),
        ("modf", PpcMathCompatibilityOperation::Modf),
        ("num2dec", PpcMathCompatibilityOperation::Num2Dec),
        ("str2dec", PpcMathCompatibilityOperation::Str2Dec),
    ] {
        assert_eq!(
            dispatcher_target_for_import("MathLib", symbol),
            PpcImportDispatcherTarget::MathCompatibility(expected),
            "{symbol}",
        );
    }
}

#[test]
fn native_ppc_math64_uses_hidden_results_and_wraps_signed_arithmetic() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 32]);
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    math64_set_gprs(&mut cpu, 4, i64::MAX as u64);
    math64_set_gprs(&mut cpu, 6, 1);
    assert_eq!(
        ppc_dispatch_math64(math64_operation("S64Add"), &mut cpu, &mut memory),
        PpcImportAction::ReturnPreserve,
    );
    assert_eq!(math64_read_memory(&mut memory, 0x1000), i64::MIN as u64);
    assert_eq!(cpu.gpr[3], 0x1000);

    math64_set_gprs(&mut cpu, 4, i64::MIN as u64);
    let _ = ppc_dispatch_math64(math64_operation("S64Absolute"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), i64::MIN as u64);

    math64_set_gprs(&mut cpu, 4, i64::MAX as u64);
    math64_set_gprs(&mut cpu, 6, 2);
    let _ = ppc_dispatch_math64(math64_operation("S64Multiply"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), -2i64 as u64);

    cpu.gpr[4] = 0x8000_0000;
    let _ = ppc_dispatch_math64(math64_operation("S64Set"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0xffff_ffff_8000_0000,
    );
    let _ = ppc_dispatch_math64(math64_operation("S64SetU"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0x0000_0000_8000_0000,
    );
    let _ = ppc_dispatch_math64(math64_operation("U64Set"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0xffff_ffff_8000_0000,
    );
    let _ = ppc_dispatch_math64(math64_operation("U64SetU"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0x0000_0000_8000_0000,
    );

    cpu.gpr[3] = 0x1234_5678;
    cpu.gpr[4] = 0x9abc_def0;
    assert_eq!(
        ppc_dispatch_math64(math64_operation("S32Set"), &mut cpu, &mut memory),
        PpcImportAction::Return(0x9abc_def0),
    );
    assert_eq!(
        ppc_dispatch_math64(math64_operation("U32SetU"), &mut cpu, &mut memory),
        PpcImportAction::Return(0x9abc_def0),
    );
}

#[test]
fn native_ppc_math64_divide_obeys_remainder_and_zero_rules() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0xaa; 32]);
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    cpu.gpr[8] = 0x1008;
    math64_set_gprs(&mut cpu, 4, -17i64 as u64);
    math64_set_gprs(&mut cpu, 6, 5);
    let _ = ppc_dispatch_math64(math64_operation("S64Divide"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), -3i64 as u64);
    assert_eq!(math64_read_memory(&mut memory, 0x1008), -2i64 as u64);

    math64_set_gprs(&mut cpu, 4, -17i64 as u64);
    math64_set_gprs(&mut cpu, 6, 0);
    let _ = ppc_dispatch_math64(math64_operation("S64Divide"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), i64::MIN as u64);
    assert_eq!(math64_read_memory(&mut memory, 0x1008), -17i64 as u64);

    cpu.gpr[8] = 0;
    memory.write_u64_be(0x1008, 0xaaaa_aaaa_aaaa_aaaa).unwrap();
    math64_set_gprs(&mut cpu, 4, 17);
    let _ = ppc_dispatch_math64(math64_operation("U64Divide"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), u64::MAX);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1008),
        0xaaaa_aaaa_aaaa_aaaa,
    );
}

#[test]
fn native_ppc_math64_masks_shift_counts_to_seven_bits() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 8]);
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = 0x1000;
    math64_set_gprs(&mut cpu, 4, i64::MIN as u64);
    for (shift, expected) in [
        (0, i64::MIN as u64),
        (63, u64::MAX),
        (64, u64::MAX),
        (127, u64::MAX),
        (128, i64::MIN as u64),
    ] {
        cpu.gpr[6] = shift;
        let _ = ppc_dispatch_math64(math64_operation("S64ShiftRight"), &mut cpu, &mut memory);
        assert_eq!(math64_read_memory(&mut memory, 0x1000), expected, "{shift}",);
    }

    math64_set_gprs(&mut cpu, 4, 1);
    for (shift, expected) in [(0, 1), (63, 1 << 63), (64, 0), (127, 0), (128, 1)] {
        cpu.gpr[6] = shift;
        let _ = ppc_dispatch_math64(math64_operation("U64ShiftLeft"), &mut cpu, &mut memory);
        assert_eq!(math64_read_memory(&mut memory, 0x1000), expected, "{shift}",);
    }
}

#[test]
fn native_ppc_math64_distinguishes_scalar_boolean_compare_and_bitwise_results() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 8]);
    let mut cpu = PpcCpu::new();
    math64_set_gprs(&mut cpu, 3, u64::MAX);
    math64_set_gprs(&mut cpu, 5, 1);
    assert_eq!(
        ppc_dispatch_math64(math64_operation("S64Compare"), &mut cpu, &mut memory),
        PpcImportAction::Return(u32::MAX),
    );
    assert_eq!(
        ppc_dispatch_math64(math64_operation("U64Compare"), &mut cpu, &mut memory),
        PpcImportAction::Return(1),
    );
    assert_eq!(
        ppc_dispatch_math64(math64_operation("S64And"), &mut cpu, &mut memory),
        PpcImportAction::Return(1),
    );
    math64_set_gprs(&mut cpu, 3, 0);
    assert_eq!(
        ppc_dispatch_math64(math64_operation("U64Not"), &mut cpu, &mut memory),
        PpcImportAction::Return(1),
    );

    cpu.gpr[3] = 0x1000;
    math64_set_gprs(&mut cpu, 4, 0xf0f0_ffff_0000_aaaa);
    math64_set_gprs(&mut cpu, 6, 0x0ff0_00ff_ffff_5555);
    let _ = ppc_dispatch_math64(math64_operation("U64BitwiseEor"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0xff00_ff00_ffff_ffff,
    );
}

#[test]
fn native_ppc_math64_round_trips_integer_boundaries_through_double_double() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, vec![0; 16]);
    let mut cpu = PpcCpu::new();

    for value in [i64::MIN, -1, 0, i64::MAX - 1, i64::MAX] {
        math64_set_gprs(&mut cpu, 3, value as u64);
        let _ =
            ppc_dispatch_math64(math64_operation("SInt64ToLongDouble"), &mut cpu, &mut memory);
        cpu.gpr[3] = 0x1000;
        let _ =
            ppc_dispatch_math64(math64_operation("LongDoubleToSInt64"), &mut cpu, &mut memory);
        assert_eq!(math64_read_memory(&mut memory, 0x1000), value as u64);
    }

    for value in [0, 1, u64::MAX - 1, u64::MAX] {
        math64_set_gprs(&mut cpu, 3, value);
        let _ =
            ppc_dispatch_math64(math64_operation("UInt64ToLongDouble"), &mut cpu, &mut memory);
        cpu.gpr[3] = 0x1008;
        let _ =
            ppc_dispatch_math64(math64_operation("LongDoubleToUInt64"), &mut cpu, &mut memory);
        assert_eq!(math64_read_memory(&mut memory, 0x1008), value);
    }
}

#[test]
fn native_ppc_math_floor_rounds_down_and_preserves_special_values() {
    let mut memory = PpcSectionMem::new();
    let mut cpu = PpcCpu::new();
    for (input, expected) in [(-300.1f64, -301.0f64), (300.1, 300.0)] {
        cpu.fpr[1] = input.to_bits();
        assert_eq!(
            ppc_dispatch_math_compatibility(
                math_operation("floor"),
                &mut cpu,
                &mut memory,
            ),
            PpcImportAction::ReturnPreserve
        );
        assert_eq!(f64::from_bits(cpu.fpr[1]), expected);
    }
    for input in [-0.0f64, f64::INFINITY, f64::NEG_INFINITY] {
        cpu.fpr[1] = input.to_bits();
        let _ = ppc_dispatch_math_compatibility(
            math_operation("floor"),
            &mut cpu,
            &mut memory,
        );
        assert_eq!(cpu.fpr[1], input.to_bits());
    }
}

#[test]
fn hle_import_runner_handles_mathlib_ceil_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"ceil");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = (-300.1f64).to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), -300.0);
}

#[test]
fn mathlib_pi_tvector_import_binds_to_addressable_double_data() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"pi");
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.imports[0].class, 2);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_MATH_PI);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_eq!(
        loaded.memory.read_u64_be(PPC_IMPORT_MATH_PI),
        Some(std::f64::consts::PI.to_bits())
    );
}

#[test]
fn mathlib_default_environment_data_import_binds_to_zeroed_fenv() {
    let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"MathLib",
        b"_FE_DFL_ENV",
        1,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut loaded = load_pef_application(&pef).unwrap();

    assert_eq!(loaded.imports[0].class, 1);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_MATH_FE_DFL_ENV);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_ne!(PPC_IMPORT_MATH_FE_DFL_ENV, PPC_IMPORT_MATH_PI);
    assert_eq!(
        loaded.memory.read_u32_be(PPC_IMPORT_MATH_FE_DFL_ENV),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_MATH_FE_DFL_ENV)
    );
}

#[test]
fn native_ppc_math_ceil_preserves_special_values_and_reports_signaling_nan() {
    let mut cpu = PpcCpu::new();
    cpu.fpscr = 3;
    for input in [-0.0f64, f64::INFINITY, f64::NEG_INFINITY] {
        cpu.fpr[1] = input.to_bits();
        ppc_math_ceil(&mut cpu);
        assert_eq!(cpu.fpr[1], input.to_bits());
        assert_eq!(cpu.fpscr, 3);
    }

    let quiet_nan = 0x7ff8_0000_0000_0042;
    cpu.fpr[1] = quiet_nan;
    ppc_math_ceil(&mut cpu);
    assert_eq!(cpu.fpr[1], quiet_nan);
    assert_eq!(cpu.fpscr, 3);

    let signaling_nan = 0xfff0_0000_0000_0042;
    cpu.fpr[1] = signaling_nan;
    cpu.set_fpscr_bit(24, true);
    ppc_math_ceil(&mut cpu);
    assert_eq!(cpu.fpr[1], signaling_nan | 0x0008_0000_0000_0000);
    for bit in [0, 1, 2, 7, 24] {
        assert!(cpu.fpscr_bit(bit));
    }
}

#[test]
fn hle_import_runner_handles_mathlib_sqrt_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"sqrt");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = 9.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), 3.0);
}

#[test]
fn hle_import_runner_handles_mathlib_atan2_in_fprs() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"atan2");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.fpr[1] = 1.0f64.to_bits();
    loaded.cpu.fpr[2] = 1.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let result = f64::from_bits(loaded.cpu.fpr[1]);
    assert!((result - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
}

#[test]
fn hle_import_runner_handles_mathlib_fmod_in_fprs() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"fmod");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = (-5.5f64).to_bits();
    loaded.cpu.fpr[2] = 2.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), -1.5);

    loaded.cpu.fpr[1] = (-0.0f64).to_bits();
    loaded.cpu.fpr[2] = 3.0f64.to_bits();
    ppc_math_fmod(&mut loaded.cpu);
    assert_eq!(loaded.cpu.fpr[1], (-0.0f64).to_bits());
}

#[test]
fn hle_import_runner_handles_mathlib_atan_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"atan");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.fpr[1] = 1.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let result = f64::from_bits(loaded.cpu.fpr[1]);
    assert!((result - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
}

#[test]
fn hle_import_runner_handles_mathlib_log10_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"log10");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = 1000.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), 3.0);
}

#[test]
fn hle_import_runner_handles_mathlib_exp_in_fpr1() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"exp");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0xfeed_face;
    loaded.cpu.fpr[1] = 1.0f64.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    let result = f64::from_bits(loaded.cpu.fpr[1]);
    assert!((result - std::f64::consts::E).abs() < 1e-12);
}

#[test]
fn hle_import_runner_handles_mathlib_dtox80() {
    let pef = synthetic_pef_with_library_import(b"MathLib", b"dtox80");
    let mut loaded = load_pef_application(&pef).unwrap();
    let input = PPC_HEAP_BASE + 0x100;
    let output = PPC_HEAP_BASE + 0x108;
    let bits = (-1.5f64).to_bits();
    loaded.memory.write_u32_be(input, (bits >> 32) as u32);
    loaded.memory.write_u32_be(input + 4, bits as u32);
    loaded.cpu.gpr[3] = input;
    loaded.cpu.gpr[4] = output;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u16_be(output), Some(0xbfff));
    assert_eq!(loaded.memory.read_u32_be(output + 2), Some(0xc000_0000));
    assert_eq!(loaded.memory.read_u32_be(output + 6), Some(0));
}
