use super::*;

#[test]
fn stdclib_time_uses_msl_epoch_and_optional_result_pointer() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"time");
    let mut loaded = load_pef_application(&pef).unwrap();
    let result_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(result_ptr, vec![0; 4]);
    loaded.cpu.gpr[3] = result_ptr;

    let probe = loaded.run_with_hle_imports(64);

    let expected = PPC_FIXED_MAC_TIME + 1_460 * 86_400;
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], expected);
    assert_eq!(loaded.memory.read_u32_be(result_ptr), Some(expected));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = 0;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], expected);
}

#[test]
fn hle_import_runner_formats_stdclib_sprintf_arguments() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"sprintf");
    let mut loaded = load_pef_application(&pef).unwrap();
    let destination = PPC_DATA_BASE + 0x1000;
    let format = PPC_DATA_BASE + 0x1100;
    let label = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(destination, vec![0xaa; 64]);
    loaded
        .memory
        .add_region(format, b"%s %03d %X %%\0".to_vec());
    loaded.memory.add_region(label, b"film\0".to_vec());
    loaded.cpu.gpr[3] = destination;
    loaded.cpu.gpr[4] = format;
    loaded.cpu.gpr[5] = label;
    loaded.cpu.gpr[6] = 7;
    loaded.cpu.gpr[7] = 0x2a;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 13);
    assert_eq!(
        ppc_std_c_string(&mut loaded.memory, destination, 64),
        b"film 007 2A %".to_vec()
    );
    assert_eq!(loaded.memory.read_u8(destination + 13), Some(0));
}

#[test]
fn hle_import_runner_qsort_calls_native_comparator_and_sorts_elements() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"qsort");
    let mut loaded = load_pef_application(&pef).unwrap();
    let array = PPC_DATA_BASE + 0x1000;
    let comparator = PPC_CODE_BASE + 0x1000;
    let values = [17u32, 3, 99, 3, 42];
    loaded.memory.add_region(array, vec![0; values.len() * 4]);
    for (index, value) in values.iter().copied().enumerate() {
        loaded
            .memory
            .write_u32_be(array + index as u32 * 4, value)
            .unwrap();
    }
    let mut callback = Vec::new();
    for word in [
        d_form_u(32, 5, 3, 0),          // lwz r5, 0(r3)
        d_form_u(32, 6, 4, 0),          // lwz r6, 0(r4)
        x_form(31, 3, 6, 5, 40, false), // subf r3, r6, r5
        BLR,
    ] {
        callback.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(comparator, callback);
    loaded.cpu.gpr[2] = 0x1234_5678;
    loaded.cpu.gpr[3] = array;
    loaded.cpu.gpr[4] = values.len() as u32;
    loaded.cpu.gpr[5] = 4;
    loaded.cpu.gpr[6] = comparator;

    let mut total_handled = 0u32;
    for _ in 0..128 {
        let probe = loaded.run_with_hle_imports(5);
        total_handled = total_handled.saturating_add(probe.handled_import_count);
        assert_eq!(probe.unsupported_import_index, None);
        match probe.result {
            PpcRunResult::CycleLimit { .. } => continue,
            PpcRunResult::Halted { .. } => break,
            result => panic!("unexpected qsort run result: {result:?}"),
        }
    }
    assert_eq!(loaded.cpu.pc, loaded.halt_pc);
    assert!(total_handled > 1);
    assert_eq!(loaded.cpu.gpr[2], 0x1234_5678);
    assert!(loaded.stdc_qsort_stack.is_empty());
    assert_eq!(
        (0..values.len())
            .map(|index| loaded.memory.read_u32_be(array + index as u32 * 4).unwrap())
            .collect::<Vec<_>>(),
        vec![3, 3, 17, 42, 99]
    );
}

#[test]
fn hle_import_runner_formats_stdclib_vsprintf_argument_list() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"vsprintf");
    let mut loaded = load_pef_application(&pef).unwrap();
    let destination = PPC_DATA_BASE + 0x1000;
    let format = PPC_DATA_BASE + 0x1100;
    let label = PPC_DATA_BASE + 0x1200;
    let arguments = PPC_DATA_BASE + 0x1300;
    loaded.memory.add_region(destination, vec![0xaa; 64]);
    loaded
        .memory
        .add_region(format, b"%s %04d %#x %.2f\0".to_vec());
    loaded.memory.add_region(label, b"film\0".to_vec());
    loaded.memory.add_region(arguments, vec![0; 20]);
    let double_bits = 3.25f64.to_bits();
    for (index, value) in [
        label,
        7,
        0x2a,
        (double_bits >> 32) as u32,
        double_bits as u32,
    ]
    .into_iter()
    .enumerate()
    {
        loaded
            .memory
            .write_u32_be(arguments + index as u32 * 4, value)
            .unwrap();
    }
    loaded.cpu.gpr[3] = destination;
    loaded.cpu.gpr[4] = format;
    loaded.cpu.gpr[5] = arguments;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 19);
    assert_eq!(
        ppc_std_c_string(&mut loaded.memory, destination, 64),
        b"film 0007 0x2a 3.25".to_vec()
    );
}

#[test]
fn hle_import_runner_formats_stdclib_strftime_c_locale_fields() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"strftime");
    let mut loaded = load_pef_application(&pef).unwrap();
    let destination = PPC_DATA_BASE + 0x1000;
    let format = PPC_DATA_BASE + 0x1100;
    let broken_down_time = PPC_DATA_BASE + 0x1200;
    loaded.memory.add_region(destination, vec![0xaa; 128]);
    loaded
        .memory
        .add_region(format, b"%a %b %e %Y %H:%M:%S %j %U %W %%\0".to_vec());
    loaded.memory.add_region(broken_down_time, vec![0; 36]);
    for (index, value) in [5i32, 4, 15, 29, 1, 100, 2, 59, 0].into_iter().enumerate() {
        loaded
            .memory
            .write_u32_be(broken_down_time + index as u32 * 4, value as u32)
            .unwrap();
    }
    loaded.cpu.gpr[3] = destination;
    loaded.cpu.gpr[4] = 128;
    loaded.cpu.gpr[5] = format;
    loaded.cpu.gpr[6] = broken_down_time;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 36);
    assert_eq!(
        ppc_std_c_string(&mut loaded.memory, destination, 128),
        b"Tue Feb 29 2000 15:04:05 060 09 09 %".to_vec()
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.gpr[3] = destination;
    loaded.cpu.gpr[4] = 8;
    loaded.cpu.gpr[5] = format;
    loaded.cpu.gpr[6] = broken_down_time;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u8(destination), Some(0));
}

#[test]
fn hle_import_runner_scans_stdclib_sscanf_fields_and_overflow_arguments() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"sscanf");
    let mut loaded = load_pef_application(&pef).unwrap();
    let input = PPC_DATA_BASE + 0x1000;
    let format = PPC_DATA_BASE + 0x1100;
    let outputs = PPC_DATA_BASE + 0x1200;
    loaded
        .memory
        .add_region(input, b"12,0x10 hi Z ff 77 -3tail\0".to_vec());
    loaded
        .memory
        .add_region(format, b"%d,%i %2s %c %x %u %hd%n\0".to_vec());
    loaded.memory.add_region(outputs, vec![0xaa; 64]);
    loaded.cpu.gpr[3] = input;
    loaded.cpu.gpr[4] = format;
    for (register, offset) in (5usize..=10).zip((0u32..).step_by(8)) {
        loaded.cpu.gpr[register] = outputs + offset;
    }
    let stack_short = ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], 8).unwrap();
    let stack_count = ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], 9).unwrap();
    loaded
        .memory
        .write_u32_be(stack_short, outputs + 48)
        .unwrap();
    loaded
        .memory
        .write_u32_be(stack_count, outputs + 56)
        .unwrap();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 7);
    assert_eq!(loaded.memory.read_u32_be(outputs), Some(12));
    assert_eq!(loaded.memory.read_u32_be(outputs + 8), Some(16));
    assert_eq!(ppc_std_c_string(&mut loaded.memory, outputs + 16, 8), b"hi");
    assert_eq!(loaded.memory.read_u8(outputs + 24), Some(b'Z'));
    assert_eq!(loaded.memory.read_u32_be(outputs + 32), Some(0xff));
    assert_eq!(loaded.memory.read_u32_be(outputs + 40), Some(77));
    assert_eq!(loaded.memory.read_u16_be(outputs + 48), Some(0xfffd));
    assert_eq!(loaded.memory.read_u32_be(outputs + 56), Some(21));
}
