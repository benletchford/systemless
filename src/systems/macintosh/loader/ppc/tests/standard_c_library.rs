use super::*;

#[test]
fn stdclib_longjmp_restores_setjmp_context_and_converts_zero_to_one() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"StdCLib",
        b"__setjmp",
    ))
    .unwrap();
    let buffer = PPC_DATA_BASE + 0x2100;
    loaded.cpu.gpr[3] = buffer;
    loaded.cpu.gpr[5] = 0x1234_5678;
    loaded.cpu.cr = 0x8765_4321;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert!(loaded.toolbox_startup.stdc_jmpbufs.contains_key(&buffer));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = buffer;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.cr = 0;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::StdLongJmp;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.cpu.gpr[5], 0x1234_5678);
    assert_eq!(loaded.cpu.cr, 0x8765_4321);
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
}

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

#[test]
fn hle_import_runner_models_stdc_signal_install_ignore_and_default() {
    let pef = synthetic_pef_with_library_import(b"StdCLib", b"signal");
    let mut loaded = load_pef_application(&pef).unwrap();
    let signal = 16;
    let first_handler = 0x0302_9b38;
    let second_handler = 0x0302_9b3c;

    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = first_handler;
    let first = loaded.run_with_hle_imports(64);
    assert_eq!(first.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = second_handler;
    let second = loaded.run_with_hle_imports(64);
    assert_eq!(second.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], first_handler);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = 1; // SIG_IGN
    let ignored = loaded.run_with_hle_imports(64);
    assert_eq!(ignored.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], second_handler);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = 0; // SIG_DFL
    let defaulted = loaded.run_with_hle_imports(64);
    assert_eq!(defaulted.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = first_handler;
    let reinstalled = loaded.run_with_hle_imports(64);
    assert_eq!(reinstalled.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0; // invalid signal number
    loaded.cpu.gpr[4] = second_handler;
    let invalid = loaded.run_with_hle_imports(64);
    assert_eq!(invalid.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], u32::MAX);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = signal;
    loaded.cpu.gpr[4] = 0;
    let unchanged = loaded.run_with_hle_imports(64);
    assert_eq!(unchanged.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], first_handler);
}

#[test]
fn stdclib_floating_point_data_imports_bind_to_ieee_constants() {
    let double_constants = [
        (
            b"_DBL_EPSILON".as_slice(),
            PPC_IMPORT_STD_DBL_EPSILON,
            f64::EPSILON.to_bits(),
        ),
        (
            b"_DBL_MAX".as_slice(),
            PPC_IMPORT_STD_DBL_MAX,
            f64::MAX.to_bits(),
        ),
        (
            b"_DBL_MIN".as_slice(),
            PPC_IMPORT_STD_DBL_MIN,
            f64::MIN_POSITIVE.to_bits(),
        ),
    ];
    for (symbol, expected_address, expected_bits) in double_constants {
        let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
            b"StdCLib",
            symbol,
            1,
            &[sm_index_reloc(0x30, 0)],
        ));
        let mut loaded = load_pef_application(&pef).unwrap();

        assert_eq!(loaded.imports[0].class, 1);
        assert_eq!(loaded.imports[0].address, expected_address);
        assert_eq!(loaded.imports[0].tvector_address, None);
        assert_eq!(expected_address % 8, 0);
        assert_eq!(
            loaded.memory.read_u64_be(expected_address),
            Some(expected_bits)
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_DATA_BASE),
            Some(expected_address)
        );
    }

    let float_constants = [
        (
            b"_FLT_EPSILON".as_slice(),
            PPC_IMPORT_STD_FLT_EPSILON,
            f32::EPSILON.to_bits(),
        ),
        (
            b"_FLT_MAX".as_slice(),
            PPC_IMPORT_STD_FLT_MAX,
            f32::MAX.to_bits(),
        ),
        (
            b"_FLT_MIN".as_slice(),
            PPC_IMPORT_STD_FLT_MIN,
            f32::MIN_POSITIVE.to_bits(),
        ),
    ];
    for (symbol, expected_address, expected_bits) in float_constants {
        let pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
            b"StdCLib",
            symbol,
            1,
            &[sm_index_reloc(0x30, 0)],
        ));
        let mut loaded = load_pef_application(&pef).unwrap();

        assert_eq!(loaded.imports[0].class, 1);
        assert_eq!(loaded.imports[0].address, expected_address);
        assert_eq!(loaded.imports[0].tvector_address, None);
        assert_eq!(expected_address % 4, 0);
        assert_eq!(
            loaded.memory.read_u32_be(expected_address),
            Some(expected_bits)
        );
        assert_eq!(
            loaded.memory.read_u32_be(PPC_DATA_BASE),
            Some(expected_address)
        );
    }

    let occupied = [
        PPC_IMPORT_MATH_PI,
        PPC_IMPORT_MATH_FE_DFL_ENV,
        PPC_IMPORT_STD_DBL_EPSILON,
        PPC_IMPORT_STD_DBL_MAX,
        PPC_IMPORT_STD_DBL_MIN,
        PPC_IMPORT_STD_FLT_EPSILON,
        PPC_IMPORT_STD_FLT_MAX,
        PPC_IMPORT_STD_FLT_MIN,
        PPC_IMPORT_STD_ERRNO,
        PPC_IMPORT_STD_MAC_OS_ERR,
    ];
    assert!(occupied.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(PPC_IMPORT_STD_MAC_OS_ERR + 2 <= PPC_IMPORT_CTYPE_TABLE);
}

#[test]
fn stdclib_error_globals_are_writable_process_scoped_data() {
    let errno_pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"StdCLib",
        b"errno",
        1,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut first = load_pef_application(&errno_pef).unwrap();
    assert_eq!(first.imports[0].class, 1);
    assert_eq!(first.imports[0].address, PPC_IMPORT_STD_ERRNO);
    assert_eq!(first.imports[0].tvector_address, None);
    assert_eq!(first.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(0));
    assert_eq!(
        first.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_STD_ERRNO)
    );
    assert!(first
        .memory
        .write_u32_be(PPC_IMPORT_STD_ERRNO, 34)
        .is_some());
    assert_eq!(first.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(34));

    let mut second = load_pef_application(&errno_pef).unwrap();
    assert_eq!(second.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(0));

    let mac_os_err_pef = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
        b"StdCLib",
        b"MacOSErr",
        1,
        &[sm_index_reloc(0x30, 0)],
    ));
    let mut loaded = load_pef_application(&mac_os_err_pef).unwrap();
    assert_eq!(loaded.imports[0].class, 1);
    assert_eq!(loaded.imports[0].address, PPC_IMPORT_STD_MAC_OS_ERR);
    assert_eq!(loaded.imports[0].tvector_address, None);
    assert_eq!(
        loaded.memory.read_u16_be(PPC_IMPORT_STD_MAC_OS_ERR),
        Some(0)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_DATA_BASE),
        Some(PPC_IMPORT_STD_MAC_OS_ERR)
    );
    assert!(loaded
        .memory
        .write_u16_be(PPC_IMPORT_STD_MAC_OS_ERR, 0xffce)
        .is_some());
    assert_eq!(
        loaded.memory.read_u16_be(PPC_IMPORT_STD_MAC_OS_ERR),
        Some(0xffce)
    );
}

#[test]
fn stdio_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("clearerr", PpcStdIoOperation::ClearErr),
        ("_filbuf", PpcStdIoOperation::FileBuffer),
        ("fgetc", PpcStdIoOperation::FileBuffer),
        ("getc", PpcStdIoOperation::FileBuffer),
        ("fputc", PpcStdIoOperation::FilePutChar),
        ("putc", PpcStdIoOperation::FilePutChar),
        ("ungetc", PpcStdIoOperation::FileUngetChar),
        ("fclose", PpcStdIoOperation::FileClose),
        ("feof", PpcStdIoOperation::FileEof),
        ("ferror", PpcStdIoOperation::FileError),
        ("fflush", PpcStdIoOperation::FileFlush),
        ("fopen", PpcStdIoOperation::FileOpen),
        ("fprintf", PpcStdIoOperation::FilePrintf),
        ("printf", PpcStdIoOperation::Printf),
        ("fread", PpcStdIoOperation::FileRead),
        ("fseek", PpcStdIoOperation::FileSeek),
        ("rewind", PpcStdIoOperation::FileRewind),
        ("ftell", PpcStdIoOperation::FileTell),
        ("fwrite", PpcStdIoOperation::FileWrite),
        ("_iob", PpcStdIoOperation::IoBuffer),
    ] {
        assert_eq!(
            dispatcher_target_for_import("StdCLib", symbol),
            PpcImportDispatcherTarget::StdIoCompatibility(operation),
        );
    }
}

#[test]
fn import_bindings_classify_stdclib_utility_imports() {
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "__setjmp"),
        PpcImportDispatcherTarget::StdSetJmp
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "longjmp"),
        PpcImportDispatcherTarget::StdLongJmp
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "sprintf"),
        PpcImportDispatcherTarget::StdSprintf
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "time"),
        PpcImportDispatcherTarget::StdTime
    );
    assert_eq!(
        dispatcher_target_for_import("StdCLib", "signal"),
        PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Signal)
    );
    for (symbol, operation) in [
        ("qsort", PpcStdCCompatibilityOperation::Qsort),
        ("sscanf", PpcStdCCompatibilityOperation::Sscanf),
        ("strftime", PpcStdCCompatibilityOperation::Strftime),
        ("vsprintf", PpcStdCCompatibilityOperation::Vsprintf),
    ] {
        assert_eq!(
            dispatcher_target_for_import("StdCLib", symbol),
            PpcImportDispatcherTarget::StdCCompatibility(operation),
        );
    }
}

#[test]
fn stdclib_localtime_uses_unsigned_msl_epoch_and_reuses_static_storage() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"localtime")).unwrap();
    let input = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(input, vec![0; 4]);
    for (seconds, fields) in [
        (0, [0, 0, 0, 1, 0, 0, 1, 0, 0]),
        (5_097_600, [0, 0, 0, 1, 2, 0, 4, 59, 0]),
        (3_160_771_200, [0, 0, 0, 29, 1, 100, 2, 59, 0]),
        (2_147_483_648, [8, 14, 3, 20, 0, 68, 6, 19, 0]),
        (u32::MAX, [15, 28, 6, 7, 1, 136, 4, 37, 0]),
    ] {
        loaded.memory.write_u32_be(input, seconds).unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = input;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], PPC_IMPORT_STD_TM);
        for (index, expected) in fields.into_iter().enumerate() {
            assert_eq!(
                loaded
                    .memory
                    .read_u32_be(PPC_IMPORT_STD_TM + index as u32 * 4),
                Some(expected)
            );
        }
    }
    for input in [0, 0xffff_fffc] {
        loaded.cpu.gpr[3] = input;
        run_test_import(&mut loaded, PpcImportDispatcherTarget::StdLocaltime);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
}

#[test]
fn stdclib_time_localtime_strftime_share_the_same_calendar() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"localtime")).unwrap();
    let input = PPC_DATA_BASE + 0x2100;
    let output = input + 0x100;
    let format = input + 0x200;
    loaded.memory.add_region(input, vec![0; 4]);
    loaded.memory.add_region(output, vec![0; 64]);
    loaded
        .memory
        .add_region(format, b"%Y-%m-%d %H:%M:%S %a %j\0".to_vec());
    loaded.cpu.gpr[3] = input;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::StdTime);
    assert_eq!(loaded.memory.read_u32_be(input), Some(loaded.cpu.gpr[3]));
    loaded.cpu.gpr[3] = input;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::StdLocaltime);
    let tm = loaded.cpu.gpr[3];
    loaded.cpu.gpr[3] = output;
    loaded.cpu.gpr[4] = 64;
    loaded.cpu.gpr[5] = format;
    loaded.cpu.gpr[6] = tm;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::StdCCompatibility(PpcStdCCompatibilityOperation::Strftime),
    );
    assert_eq!(
        ppc_std_c_string(&mut loaded.memory, output, 64),
        b"2024-01-01 00:00:00 Mon 001".to_vec()
    );
}

#[test]
fn stdclib_atof_converts_decimal_prefixes_in_the_floating_point_return_register() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"atof")).unwrap();
    let input = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(input, vec![0; 128]);
    for (text, expected) in [
        (" \t\n\r\x0b\x0c-12.5tail", -12.5f64),
        ("+.125e+2suffix", 12.5),
        ("42.", 42.0),
        ("1e-2", 0.01),
        ("1e+", 1.0),
        ("2.5.3", 2.5),
        ("-0", -0.0),
        ("-1e-9999", -0.0),
        ("1e9999", f64::INFINITY),
        ("4.9406564584124654e-324", f64::from_bits(1)),
        ("1.7976931348623157e308", f64::MAX),
        ("", 0.0),
        ("word", 0.0),
        ("-.", 0.0),
        ("+ 1", 0.0),
    ] {
        for (index, byte) in text.bytes().chain(std::iter::once(0)).enumerate() {
            loaded.memory.write_u8(input + index as u32, byte).unwrap();
        }
        loaded
            .memory
            .write_u32_be(PPC_IMPORT_STD_ERRNO, 33)
            .unwrap();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.gpr[3] = input;
        loaded.cpu.fpr[1] = 123f64.to_bits();
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.fpr[1], expected.to_bits(), "{text:?}");
        assert_eq!(loaded.cpu.gpr[3], input);
        assert_eq!(
            loaded.memory.read_u32_be(PPC_IMPORT_STD_ERRNO),
            Some(if matches!(text, "-1e-9999" | "1e9999") {
                34
            } else {
                33
            })
        );
    }
}

#[test]
fn stdclib_fscanf_preserves_unmatched_input_and_observes_stream_eof() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"fscanf")).unwrap();
    let base = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(base, vec![0; 512]);
    loaded.process_file_system.with_mut(|fs| {
        fs.vfs_files.push(PpcVfsFileRecord {
            path: "scan.txt".to_string(),
            data: b" 12 xyz!".to_vec().into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        })
    });
    let stream = base + 400;
    loaded.process_file_system.with_mut(|fs| {
        fs.stdio_streams.insert(
            stream,
            PpcStdioStreamRecord {
                pushback: None,
                ref_num: None,
                path: Some("scan.txt".to_string()),
                position: 0,
                standard: false,
                readable: true,
                writable: false,
                append: false,
                closed: false,
                eof: false,
                error: false,
            },
        )
    });
    let scan = |loaded: &mut PpcLoadedApp, format: &[u8]| {
        loaded.memory.write_bytes(base, format).unwrap();
        loaded.cpu.gpr[3..8].copy_from_slice(&[stream, base, base + 100, base + 104, base + 120]);
        run_test_import(
            loaded,
            PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileScanf),
        );
        loaded.cpu.gpr[3]
    };
    assert_eq!(scan(&mut loaded, b"%d %3[a-z]%n\0"), 2);
    assert_eq!(loaded.memory.read_u32_be(base + 100), Some(12));
    assert_eq!(loaded.memory.read_u32_be(base + 104), Some(0x78797a00));
    assert_eq!(loaded.memory.read_u32_be(base + 120), Some(7));
    assert_eq!(loaded.stdio_streams[&stream].position, 7);
    assert!(!loaded.stdio_streams[&stream].eof);
    assert_eq!(scan(&mut loaded, b"%d\0"), 0);
    assert_eq!(loaded.stdio_streams[&stream].position, 7);
    loaded
        .process_file_system
        .with_mut(|fs| fs.stdio_streams.get_mut(&stream).unwrap().pushback = Some(b'9'));
    assert_eq!(scan(&mut loaded, b"%1d\0"), 1);
    assert_eq!(loaded.memory.read_u32_be(base + 100), Some(9));
    assert_eq!(loaded.stdio_streams[&stream].position, 7);
    assert_eq!(scan(&mut loaded, b"%c\0"), 1);
    assert_eq!(loaded.memory.read_u8(base + 100), Some(b'!'));
    assert!(!loaded.stdio_streams[&stream].eof);
    assert_eq!(scan(&mut loaded, b"%d\0"), u32::MAX);
    assert!(loaded.stdio_streams[&stream].eof);
}

#[test]
fn stdclib_scanner_tracks_failed_prefixes_and_suppressed_conversions() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"sscanf")).unwrap();
    let destination = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(destination, vec![0; 32]);
    loaded.cpu.gpr[5] = destination;
    for (input, format, value, consumed, eof) in [
        (&b"+x"[..], &b"%d"[..], 0, 1, false),
        (&b"+"[..], &b"%d"[..], 0, 1, true),
        (&b"0xg"[..], &b"%x"[..], 0, 2, false),
        (&b"12"[..], &b"%*d%d"[..], 0, 2, true),
        (&b"a"[..], &b"%2c"[..], u32::MAX, 1, true),
        (&b"12"[..], &b"%2d"[..], 1, 2, false),
        (&b" %"[..], &b"%%"[..], 0, 2, false),
    ] {
        let result =
            dispatch_stdc::ppc_scan_formatted(&loaded.cpu, &mut loaded.memory, input, format);
        assert_eq!(
            (result.value, result.consumed, result.eof),
            (value, consumed, eof),
            "{input:?} {format:?}"
        );
    }
}

#[test]
fn stdclib_scanner_floats_stop_at_token_boundaries() {
    for (input, width, expected, consumed) in [
        (&b"12-3"[..], usize::MAX, Some(12.0), 2),
        (&b"-1.25e+2!"[..], usize::MAX, Some(-125.0), 8),
        (&b"100ergs"[..], usize::MAX, None, 4),
        (&b"1e+"[..], usize::MAX, None, 3),
        (&b"12"[..], 1, Some(1.0), 1),
        (&b"0x1.8p+2!"[..], usize::MAX, Some(6.0), 8),
        (&b"-infinity!"[..], usize::MAX, Some(f64::NEG_INFINITY), 9),
        (&b"inf!"[..], usize::MAX, Some(f64::INFINITY), 3),
        (&b"infi!"[..], usize::MAX, None, 4),
    ] {
        assert_eq!(
            dispatch_stdc::ppc_scan_float(input, 0, width),
            (expected, consumed),
            "{input:?}"
        );
    }
    for (input, bits) in [
        (&b"0x1p-1074"[..], 1u64),
        (&b"0x1p-1075"[..], 0),
        (&b"0x1.8p-1075"[..], 1),
        (&b"-0x0p9999"[..], 1u64 << 63),
        (&b"0x1.00000000000008p0"[..], 1f64.to_bits()),
        (&b"0x1.0000000000000801p0"[..], 1f64.to_bits() + 1),
        (&b"0x1.fffffffffffffp1023"[..], f64::MAX.to_bits()),
        (&b"0x1p1024"[..], f64::INFINITY.to_bits()),
    ] {
        let (value, consumed) = dispatch_stdc::ppc_scan_float(input, 0, usize::MAX);
        assert_eq!(value.unwrap().to_bits(), bits, "{input:?}");
        assert_eq!(consumed, input.len());
    }
    let (value, consumed) = dispatch_stdc::ppc_scan_float(b"nan(payload)!", 0, usize::MAX);
    assert!(value.unwrap().is_nan());
    assert_eq!(consumed, 12);
}

#[test]
fn stdclib_scanner_rounds_float_destinations_like_classic_stdclib() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"sscanf")).unwrap();
    let destination = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(destination, vec![0; 8]);
    loaded.cpu.gpr[5] = destination;
    for (input, bits) in [
        (&b"1.0000000596046448"[..], 1f32.to_bits()),
        (&b"0x1.0000010000000001p0"[..], 1f32.to_bits()),
        (&b"0x1.000001p0"[..], 1f32.to_bits()),
        (&b"0x1p-149"[..], 1),
        (&b"0x1p-150"[..], 0),
        (&b"0x1.8p-150"[..], 1),
        (&b"0x1p128"[..], f32::INFINITY.to_bits()),
    ] {
        let result =
            dispatch_stdc::ppc_scan_formatted(&loaded.cpu, &mut loaded.memory, input, b"%f");
        assert_eq!(result.value, 1);
        assert_eq!(
            loaded.memory.read_u32_be(destination),
            Some(bits),
            "{input:?}"
        );
    }
}

#[test]
fn stdclib_scanner_writes_both_long_double_components() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"sscanf")).unwrap();
    let destination = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(destination, vec![0xaa; 20]);
    loaded.cpu.gpr[5] = destination;
    for (input, high, low) in [
        (&b"9007199254740993"[..], 9007199254740992.0f64, 1.0f64),
        (&b"-9007199254740993"[..], -9007199254740992.0, -1.0),
        (&b"0x1.0000000000000001p0"[..], 1.0, 2f64.powi(-64)),
        (
            &b"1.000000000000000000867361737988403547205962240695953369140625"[..],
            1.0,
            2f64.powi(-60),
        ),
        // The residual rounds to half an ulp: normalize the leading tie to even.
        (
            &b"0x1.00000000000008000000000000001p0"[..],
            1.0,
            2f64.powi(-53),
        ),
        (
            &b"0x1.fffffffffffff7ffffffffffffffffffp1023"[..],
            f64::INFINITY,
            0.0,
        ),
        (&b"1.25"[..], 1.25, 0.0),
        (&b"-0"[..], -0.0, 0.0),
    ] {
        let result =
            dispatch_stdc::ppc_scan_formatted(&loaded.cpu, &mut loaded.memory, input, b"%Lf");
        assert_eq!(result.value, 1);
        let expected = [high.to_bits().to_be_bytes(), low.to_bits().to_be_bytes()].concat();
        assert_eq!(
            ppc_memory_read_bytes(&mut loaded.memory, destination, 16).unwrap(),
            expected,
            "{input:?}"
        );
        assert_eq!(
            loaded.memory.read_u32_be(destination + 16),
            Some(0xaaaaaaaa)
        );
    }
}

#[test]
fn stdclib_descriptors_share_bytes_but_keep_independent_positions() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"open")).unwrap();
    let base = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(base, vec![0; 512]);
    loaded
        .memory
        .write_bytes(base, b"descriptor-test\0")
        .unwrap();
    loaded.memory.write_bytes(base + 128, b"abc").unwrap();
    let call = |loaded: &mut PpcLoadedApp, operation, args: &[u32]| {
        loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
        run_test_import(loaded, PpcImportDispatcherTarget::StdDescriptor(operation));
        loaded.cpu.gpr[3]
    };
    use PpcDescriptorOperation as Op;
    let first = call(&mut loaded, Op::Open, &[base, 2 | 256 | 2048]);
    assert_eq!(first, 3);
    assert_eq!(call(&mut loaded, Op::Write, &[first, base + 128, 3]), 3);
    let second = call(&mut loaded, Op::Open, &[base, 2048]);
    assert_eq!(second, 4);
    assert_eq!(call(&mut loaded, Op::Read, &[second, base + 256, 8]), 3);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, base + 256, 3).unwrap(),
        b"abc"
    );
    assert_eq!(call(&mut loaded, Op::Read, &[second, base + 256, 8]), 0);
    assert_eq!(
        call(&mut loaded, Op::Write, &[second, base + 128, 1]),
        u32::MAX
    );
    assert_eq!(loaded.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(9));
    assert_eq!(call(&mut loaded, Op::Seek, &[first, 5, 0]), 5);
    assert_eq!(call(&mut loaded, Op::Write, &[first, base + 128, 1]), 1);
    assert_eq!(call(&mut loaded, Op::Seek, &[second, 0, 0]), 0);
    assert_eq!(call(&mut loaded, Op::Read, &[second, base + 256, 8]), 6);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, base + 256, 6).unwrap(),
        b"abc\0\0a"
    );
    assert_eq!(call(&mut loaded, Op::Close, &[first]), 0);
    assert_eq!(
        call(&mut loaded, Op::Read, &[first, base + 256, 1]),
        u32::MAX
    );
    let append = call(&mut loaded, Op::Open, &[base, 1 | 8 | 2048]);
    assert_eq!(append, first);
    assert_eq!(call(&mut loaded, Op::Seek, &[append, 0, 0]), 0);
    assert_eq!(call(&mut loaded, Op::Write, &[append, base + 129, 1]), 1);
    assert_eq!(call(&mut loaded, Op::Seek, &[second, -1i32 as u32, 2]), 6);
    assert_eq!(call(&mut loaded, Op::Read, &[second, base + 256, 1]), 1);
    assert_eq!(loaded.memory.read_u8(base + 256), Some(b'b'));
    assert_eq!(
        call(&mut loaded, Op::Open, &[base, 1 | 256 | 1024 | 512]),
        u32::MAX
    );
    assert_eq!(loaded.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(17));
    assert_eq!(call(&mut loaded, Op::Seek, &[second, 0, 2]), 7);
}

#[test]
fn stdclib_descriptors_preserve_resource_forks_and_reject_failed_io_without_mutation() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"open")).unwrap();
    let base = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(base, vec![0; 512]);
    loaded.memory.write_bytes(base, b"fork-test\0").unwrap();
    loaded
        .memory
        .write_bytes(base + 64, b"absent-parent/child\0")
        .unwrap();
    loaded.memory.write_bytes(base + 128, b"fork").unwrap();
    let call = |loaded: &mut PpcLoadedApp, operation, args: &[u32]| {
        loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
        run_test_import(loaded, PpcImportDispatcherTarget::StdDescriptor(operation));
        loaded.cpu.gpr[3]
    };
    use PpcDescriptorOperation as Op;
    assert_eq!(
        call(&mut loaded, Op::Open, &[base + 64, 2 | 256 | 2048]),
        u32::MAX
    );
    assert_eq!(loaded.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(2));
    let first = call(&mut loaded, Op::Open, &[base, 2 | 16 | 256 | 2048]);
    assert_eq!(first, 3);
    assert_eq!(call(&mut loaded, Op::Write, &[first, base + 128, 4]), 4);
    let second = call(&mut loaded, Op::Open, &[base, 16 | 2048]);
    assert_eq!(second, 4);
    assert_eq!(
        call(&mut loaded, Op::Read, &[second, 0xffff_fffe, 4]),
        u32::MAX
    );
    assert_eq!(loaded.memory.read_u32_be(PPC_IMPORT_STD_ERRNO), Some(14));
    assert_eq!(call(&mut loaded, Op::Seek, &[second, 0, 1]), 0);
    assert_eq!(call(&mut loaded, Op::Close, &[first]), 0);
    assert_eq!(call(&mut loaded, Op::Read, &[second, base + 256, 4]), 4);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, base + 256, 4).unwrap(),
        b"fork"
    );
    assert_eq!(call(&mut loaded, Op::Close, &[second]), 0);
    let reopened = call(&mut loaded, Op::Open, &[base, 16 | 2048]);
    assert_eq!(call(&mut loaded, Op::Read, &[reopened, base + 256, 4]), 4);
    let data = call(&mut loaded, Op::Open, &[base, 2048]);
    assert_eq!(call(&mut loaded, Op::Read, &[data, base + 256, 4]), 0);
    assert_eq!(call(&mut loaded, Op::Close, &[0]), 0);
    let reused = call(&mut loaded, Op::Open, &[base, 2048]);
    assert_eq!(reused, 0);
    loaded.cpu.gpr[3] = dispatch_stdio::PPC_STDIO_IOB_ADDR + dispatch_stdio::PPC_STDIO_FILE_SIZE;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileClose),
    );
    assert_eq!(call(&mut loaded, Op::Write, &[1, base + 128, 4]), u32::MAX);
}

#[test]
fn stdclib_reopened_stdout_shares_position_with_descriptor_writes() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_library_import(b"StdCLib", b"open")).unwrap();
    let base = PPC_DATA_BASE + 0x2100;
    loaded.memory.add_region(base, vec![0; 512]);
    loaded
        .memory
        .write_bytes(base, b"redirected-output\0")
        .unwrap();
    loaded.memory.write_bytes(base + 128, b"AC").unwrap();
    let call = |loaded: &mut PpcLoadedApp, operation, args: &[u32]| {
        loaded.cpu.gpr[3..3 + args.len()].copy_from_slice(args);
        run_test_import(loaded, PpcImportDispatcherTarget::StdDescriptor(operation));
        loaded.cpu.gpr[3]
    };
    use PpcDescriptorOperation as Op;
    let stdout = dispatch_stdio::PPC_STDIO_IOB_ADDR + dispatch_stdio::PPC_STDIO_FILE_SIZE;
    loaded.cpu.gpr[3] = stdout;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FileClose),
    );
    assert_eq!(call(&mut loaded, Op::Open, &[base, 2 | 256 | 2048]), 1);
    assert_eq!(call(&mut loaded, Op::Write, &[1, base + 128, 1]), 1);
    loaded.cpu.gpr[3] = u32::from(b'B');
    loaded.cpu.gpr[4] = stdout;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FilePutChar),
    );
    assert_eq!(loaded.cpu.gpr[3], u32::from(b'B'));
    assert_eq!(call(&mut loaded, Op::Write, &[1, base + 129, 1]), 1);
    assert_eq!(call(&mut loaded, Op::Seek, &[1, 0, 0]), 0);
    assert_eq!(call(&mut loaded, Op::Read, &[1, base + 256, 3]), 3);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, base + 256, 3).unwrap(),
        b"ABC"
    );
    assert_eq!(call(&mut loaded, Op::Close, &[1]), 0);
    loaded.cpu.gpr[3] = u32::from(b'D');
    loaded.cpu.gpr[4] = stdout;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::StdIoCompatibility(PpcStdIoOperation::FilePutChar),
    );
    assert_eq!(loaded.cpu.gpr[3], u32::MAX);
}
