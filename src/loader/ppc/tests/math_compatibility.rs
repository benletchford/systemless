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
    let binding = compatibility_binding(
        "MathLib",
        "modf",
        PpcImportDispatcherTarget::MathCompatibility,
    );
    assert_eq!(
        ppc_dispatch_math_compatibility(&binding, &mut cpu, &mut memory),
        PpcImportAction::ReturnPreserve
    );
    let integer_bits = (u64::from(memory.read_u32_be(0x1100).unwrap()) << 32)
        | u64::from(memory.read_u32_be(0x1104).unwrap());
    assert_eq!(f64::from_bits(integer_bits), -12.0);
    assert_eq!(f64::from_bits(cpu.fpr[1]), -0.75);
}

fn math64_binding(symbol_name: &str) -> PpcImportBinding {
    compatibility_binding("Math64Lib", symbol_name, PpcImportDispatcherTarget::Math64)
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
        assert_eq!(
            dispatcher_target_for_import("Math64Lib", symbol),
            PpcImportDispatcherTarget::Math64,
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
        ppc_dispatch_math64(&math64_binding("S64Add"), &mut cpu, &mut memory),
        PpcImportAction::ReturnPreserve,
    );
    assert_eq!(math64_read_memory(&mut memory, 0x1000), i64::MIN as u64);
    assert_eq!(cpu.gpr[3], 0x1000);

    math64_set_gprs(&mut cpu, 4, i64::MIN as u64);
    let _ = ppc_dispatch_math64(&math64_binding("S64Absolute"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), i64::MIN as u64);

    math64_set_gprs(&mut cpu, 4, i64::MAX as u64);
    math64_set_gprs(&mut cpu, 6, 2);
    let _ = ppc_dispatch_math64(&math64_binding("S64Multiply"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), -2i64 as u64);

    cpu.gpr[4] = 0x8000_0000;
    let _ = ppc_dispatch_math64(&math64_binding("S64Set"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0xffff_ffff_8000_0000,
    );
    let _ = ppc_dispatch_math64(&math64_binding("S64SetU"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0x0000_0000_8000_0000,
    );
    let _ = ppc_dispatch_math64(&math64_binding("U64Set"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0xffff_ffff_8000_0000,
    );
    let _ = ppc_dispatch_math64(&math64_binding("U64SetU"), &mut cpu, &mut memory);
    assert_eq!(
        math64_read_memory(&mut memory, 0x1000),
        0x0000_0000_8000_0000,
    );

    cpu.gpr[3] = 0x1234_5678;
    cpu.gpr[4] = 0x9abc_def0;
    assert_eq!(
        ppc_dispatch_math64(&math64_binding("S32Set"), &mut cpu, &mut memory),
        PpcImportAction::Return(0x9abc_def0),
    );
    assert_eq!(
        ppc_dispatch_math64(&math64_binding("U32SetU"), &mut cpu, &mut memory),
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
    let _ = ppc_dispatch_math64(&math64_binding("S64Divide"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), -3i64 as u64);
    assert_eq!(math64_read_memory(&mut memory, 0x1008), -2i64 as u64);

    math64_set_gprs(&mut cpu, 4, -17i64 as u64);
    math64_set_gprs(&mut cpu, 6, 0);
    let _ = ppc_dispatch_math64(&math64_binding("S64Divide"), &mut cpu, &mut memory);
    assert_eq!(math64_read_memory(&mut memory, 0x1000), i64::MIN as u64);
    assert_eq!(math64_read_memory(&mut memory, 0x1008), -17i64 as u64);

    cpu.gpr[8] = 0;
    memory.write_u64_be(0x1008, 0xaaaa_aaaa_aaaa_aaaa).unwrap();
    math64_set_gprs(&mut cpu, 4, 17);
    let _ = ppc_dispatch_math64(&math64_binding("U64Divide"), &mut cpu, &mut memory);
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
        let _ = ppc_dispatch_math64(&math64_binding("S64ShiftRight"), &mut cpu, &mut memory);
        assert_eq!(math64_read_memory(&mut memory, 0x1000), expected, "{shift}",);
    }

    math64_set_gprs(&mut cpu, 4, 1);
    for (shift, expected) in [(0, 1), (63, 1 << 63), (64, 0), (127, 0), (128, 1)] {
        cpu.gpr[6] = shift;
        let _ = ppc_dispatch_math64(&math64_binding("U64ShiftLeft"), &mut cpu, &mut memory);
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
        ppc_dispatch_math64(&math64_binding("S64Compare"), &mut cpu, &mut memory),
        PpcImportAction::Return(u32::MAX),
    );
    assert_eq!(
        ppc_dispatch_math64(&math64_binding("U64Compare"), &mut cpu, &mut memory),
        PpcImportAction::Return(1),
    );
    assert_eq!(
        ppc_dispatch_math64(&math64_binding("S64And"), &mut cpu, &mut memory),
        PpcImportAction::Return(1),
    );
    math64_set_gprs(&mut cpu, 3, 0);
    assert_eq!(
        ppc_dispatch_math64(&math64_binding("U64Not"), &mut cpu, &mut memory),
        PpcImportAction::Return(1),
    );

    cpu.gpr[3] = 0x1000;
    math64_set_gprs(&mut cpu, 4, 0xf0f0_ffff_0000_aaaa);
    math64_set_gprs(&mut cpu, 6, 0x0ff0_00ff_ffff_5555);
    let _ = ppc_dispatch_math64(&math64_binding("U64BitwiseEor"), &mut cpu, &mut memory);
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
            ppc_dispatch_math64(&math64_binding("SInt64ToLongDouble"), &mut cpu, &mut memory);
        cpu.gpr[3] = 0x1000;
        let _ =
            ppc_dispatch_math64(&math64_binding("LongDoubleToSInt64"), &mut cpu, &mut memory);
        assert_eq!(math64_read_memory(&mut memory, 0x1000), value as u64);
    }

    for value in [0, 1, u64::MAX - 1, u64::MAX] {
        math64_set_gprs(&mut cpu, 3, value);
        let _ =
            ppc_dispatch_math64(&math64_binding("UInt64ToLongDouble"), &mut cpu, &mut memory);
        cpu.gpr[3] = 0x1008;
        let _ =
            ppc_dispatch_math64(&math64_binding("LongDoubleToUInt64"), &mut cpu, &mut memory);
        assert_eq!(math64_read_memory(&mut memory, 0x1008), value);
    }
}

#[test]
fn native_ppc_math_floor_rounds_down_and_preserves_special_values() {
    let mut memory = PpcSectionMem::new();
    let mut cpu = PpcCpu::new();
    let binding = compatibility_binding(
        "MathLib",
        "floor",
        PpcImportDispatcherTarget::MathCompatibility,
    );
    for (input, expected) in [(-300.1f64, -301.0f64), (300.1, 300.0)] {
        cpu.fpr[1] = input.to_bits();
        assert_eq!(
            ppc_dispatch_math_compatibility(&binding, &mut cpu, &mut memory),
            PpcImportAction::ReturnPreserve
        );
        assert_eq!(f64::from_bits(cpu.fpr[1]), expected);
    }
    for input in [-0.0f64, f64::INFINITY, f64::NEG_INFINITY] {
        cpu.fpr[1] = input.to_bits();
        let _ = ppc_dispatch_math_compatibility(&binding, &mut cpu, &mut memory);
        assert_eq!(cpu.fpr[1], input.to_bits());
    }
}
