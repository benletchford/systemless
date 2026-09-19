use super::*;

#[test]
fn hle_import_runner_installs_replaces_and_removes_application_exception_handler() {
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "InstallExceptionHandler"),
        PpcImportDispatcherTarget::InstallExceptionHandler
    );
    assert_eq!(
        dispatcher_target_for_import("ProcessMgrSupport", "InstallExceptionHandler"),
        PpcImportDispatcherTarget::InstallExceptionHandler
    );
    let pef = synthetic_pef_with_import(b"InstallExceptionHandler");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handler_a = PPC_DATA_BASE + 0x1000;
    let handler_b = PPC_DATA_BASE + 0x1100;

    loaded.cpu.gpr[3] = handler_a;
    let first = loaded.run_with_hle_imports(64);
    assert_eq!(first.handled_import_count, 1);
    assert_eq!(first.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.native_exception_handler, handler_a);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = handler_b;
    let replacement = loaded.run_with_hle_imports(64);
    assert_eq!(replacement.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], handler_a);
    assert_eq!(loaded.native_exception_handler, handler_b);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0;
    let removal = loaded.run_with_hle_imports(64);
    assert_eq!(removal.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], handler_b);
    assert_eq!(loaded.native_exception_handler, 0);

    let isolated = load_pef_application(&pef).unwrap();
    assert_eq!(isolated.native_exception_handler, 0);
}

fn write_test_ppc_words(loaded: &mut PpcLoadedApp, addr: u32, words: &[u32]) {
    for (index, word) in words.iter().copied().enumerate() {
        loaded
            .memory
            .write_u32_be(addr + u32::try_from(index).unwrap() * 4, word)
            .unwrap();
    }
}

fn install_test_native_exception_handler(
    loaded: &mut PpcLoadedApp,
    handler_words: &[u32],
) -> (u32, u32, u32) {
    let tvector = PPC_DATA_BASE + 0x1000;
    let handler_entry = PPC_CODE_BASE + 0x1000;
    let handler_rtoc = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(tvector, vec![0; 8]);
    loaded
        .memory
        .add_region(handler_entry, vec![0; handler_words.len() * 4]);
    loaded.memory.add_region(handler_rtoc, vec![0; 0x100]);
    loaded.memory.write_u32_be(tvector, handler_entry).unwrap();
    loaded
        .memory
        .write_u32_be(tvector + 4, handler_rtoc)
        .unwrap();
    write_test_ppc_words(loaded, handler_entry, handler_words);
    loaded.native_exception_handler = tvector;
    (tvector, handler_entry, handler_rtoc)
}

fn install_test_fault_words(loaded: &mut PpcLoadedApp, words: &[u32]) -> u32 {
    let fault_pc = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(fault_pc, vec![0; words.len() * 4]);
    write_test_ppc_words(loaded, fault_pc, words);
    loaded.cpu.pc = fault_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    fault_pc
}

#[test]
fn hle_import_runner_delivers_and_resumes_native_powerpc_exceptions() {
    for (fault_word, expected_kind) in [
        (0x7c80_0008, PPC_TRAP_EXCEPTION),
        (0x0000_0000, PPC_ILLEGAL_INSTRUCTION_EXCEPTION),
    ] {
        let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
        let fault_pc = install_test_fault_words(
            &mut loaded,
            &[
                fault_word,
                d_form_u(14, 7, 0, 0x1234),
                d_form_u(14, 0, 0, 0),
                xfx_form(31, 0, 8, 467),
                BLR,
            ],
        );
        let (_, _, handler_rtoc) = install_test_native_exception_handler(
            &mut loaded,
            &[
                d_form_u(36, 3, 2, 0),
                d_form_u(32, 4, 3, 4),
                d_form_u(36, 4, 2, 4),
                d_form_u(32, 6, 3, 0),
                d_form_u(36, 6, 2, 8),
                d_form_u(32, 5, 4, 20),
                d_form_u(14, 5, 5, 4),
                d_form_u(36, 5, 4, 20),
                d_form_u(14, 3, 0, 0),
                BLR,
            ],
        );
        let saved_sp = loaded.cpu.gpr[1];
        let saved_rtoc = loaded.cpu.gpr[2];
        loaded.cpu.gpr[6] = 0x6677_8899;
        loaded.cpu.fpr[9] = 0x4009_21fb_5444_2d18;
        loaded.cpu.cr = 0x1234_5678;
        loaded.cpu.xer = 0x8000_0000;

        let probe = loaded.run_with_hle_imports(128);

        assert!(matches!(probe.result, PpcRunResult::Halted { pc: 0, .. }));
        assert_eq!(loaded.cpu.gpr[7], 0x1234);
        assert_eq!(loaded.cpu.gpr[6], 0x6677_8899);
        assert_eq!(loaded.cpu.fpr[9], 0x4009_21fb_5444_2d18);
        assert_eq!(loaded.cpu.cr, 0x1234_5678);
        assert_eq!(loaded.cpu.xer, 0x8000_0000);
        assert_eq!(loaded.cpu.gpr[1], saved_sp);
        assert_eq!(loaded.cpu.gpr[2], saved_rtoc);
        assert!(loaded.native_exception_stack.is_empty());

        let information = loaded.memory.read_u32_be(handler_rtoc).unwrap();
        let machine_state = loaded.memory.read_u32_be(handler_rtoc + 4).unwrap();
        assert_ne!(information, 0);
        assert!(information < saved_sp);
        assert_eq!(
            loaded.memory.read_u32_be(handler_rtoc + 8),
            Some(expected_kind)
        );
        assert_eq!(loaded.memory.read_u32_be(information), Some(expected_kind));
        assert_eq!(
            loaded.memory.read_u32_be(information + 4),
            Some(machine_state)
        );
        assert_eq!(
            loaded.memory.read_u32_be(machine_state + 20),
            Some(fault_pc + 4)
        );
        let register_image = loaded.memory.read_u32_be(information + 8).unwrap();
        assert_eq!(
            loaded.memory.read_u32_be(register_image + 6 * 8 + 4),
            Some(0x6677_8899)
        );
        let fpu_image = loaded.memory.read_u32_be(information + 12).unwrap();
        assert_eq!(
            loaded.memory.read_u64_be(fpu_image + 9 * 8),
            Some(0x4009_21fb_5444_2d18)
        );
        assert_eq!(loaded.memory.read_u32_be(information + 16), Some(0));
        assert_ne!(loaded.memory.read_u32_be(information + 20), Some(0));
    }
}

fn install_test_unmapped_store(loaded: &mut PpcLoadedApp) -> u32 {
    let fault_pc = install_test_fault_words(
        loaded,
        &[
            d_form_u(36, 5, 4, 0),
            d_form_u(14, 7, 0, 0x1234),
            d_form_u(14, 0, 0, 0),
            xfx_form(31, 0, 8, 467),
            BLR,
        ],
    );
    loaded.cpu.gpr[4] = 0x1000_0000;
    loaded.cpu.gpr[5] = 0x5566_7788;
    fault_pc
}

pub(super) fn install_test_unmapped_load(loaded: &mut PpcLoadedApp) -> u32 {
    let fault_pc = install_test_fault_words(
        loaded,
        &[
            d_form_u(32, 5, 4, 0),
            d_form_u(14, 0, 0, 0),
            xfx_form(31, 0, 8, 467),
            BLR,
        ],
    );
    loaded.cpu.gpr[4] = 0x1000_0000;
    fault_pc
}

fn memory_exception_resuming_handler() -> Vec<u32> {
    vec![
        d_form_u(36, 3, 2, 0),
        d_form_u(32, 4, 3, 4),
        d_form_u(32, 5, 3, 16),
        d_form_u(36, 5, 2, 4),
        d_form_u(32, 6, 3, 0),
        d_form_u(36, 6, 2, 8),
        d_form_u(32, 6, 5, 0),
        d_form_u(36, 6, 2, 12),
        d_form_u(32, 6, 5, 4),
        d_form_u(36, 6, 2, 16),
        d_form_u(32, 6, 5, 8),
        d_form_u(36, 6, 2, 20),
        d_form_u(32, 6, 5, 12),
        d_form_u(36, 6, 2, 24),
        d_form_u(32, 6, 4, 20),
        d_form_u(14, 6, 6, 4),
        d_form_u(36, 6, 4, 20),
        d_form_u(14, 3, 0, 0),
        BLR,
    ]
}

#[test]
fn hle_import_runner_delivers_and_resumes_unmapped_memory_exceptions() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let fault_pc = install_test_unmapped_store(&mut loaded);
    let (_, _, handler_rtoc) =
        install_test_native_exception_handler(&mut loaded, &memory_exception_resuming_handler());
    let saved_sp = loaded.cpu.gpr[1];

    let probe = loaded.run_with_hle_imports(128);

    assert!(matches!(probe.result, PpcRunResult::Halted { pc: 0, .. }));
    assert_eq!(loaded.cpu.gpr[7], 0x1234);
    assert_eq!(loaded.cpu.gpr[4], 0x1000_0000);
    assert_eq!(loaded.cpu.gpr[5], 0x5566_7788);
    assert_eq!(loaded.cpu.gpr[1], saved_sp);
    assert!(loaded.native_exception_stack.is_empty());

    let information = loaded.memory.read_u32_be(handler_rtoc).unwrap();
    let memory_information = loaded.memory.read_u32_be(handler_rtoc + 4).unwrap();
    assert_ne!(information, 0);
    assert_ne!(memory_information, 0);
    assert_eq!(loaded.memory.read_u32_be(handler_rtoc + 8), Some(4));
    assert_eq!(
        loaded.memory.read_u32_be(handler_rtoc + 12),
        Some(loaded.stack_base)
    );
    assert_eq!(
        loaded.memory.read_u32_be(handler_rtoc + 16),
        Some(0x1000_0000)
    );
    assert_eq!(loaded.memory.read_u32_be(handler_rtoc + 20), Some(5));
    assert_eq!(loaded.memory.read_u32_be(handler_rtoc + 24), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(information),
        Some(PPC_UNMAPPED_MEMORY_EXCEPTION)
    );
    assert_eq!(
        loaded.memory.read_u32_be(information + 16),
        Some(memory_information)
    );
    let machine_state = loaded.memory.read_u32_be(information + 4).unwrap();
    assert_eq!(
        loaded.memory.read_u32_be(machine_state + 20),
        Some(fault_pc + 4)
    );
}

#[test]
fn hle_import_runner_identifies_unmapped_memory_reads() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    install_test_unmapped_load(&mut loaded);
    let (_, _, handler_rtoc) =
        install_test_native_exception_handler(&mut loaded, &memory_exception_resuming_handler());

    let probe = loaded.run_with_hle_imports(128);

    assert!(matches!(probe.result, PpcRunResult::Halted { pc: 0, .. }));
    assert_eq!(
        loaded.memory.read_u32_be(handler_rtoc + 16),
        Some(0x1000_0000)
    );
    assert_eq!(
        loaded.memory.read_u32_be(handler_rtoc + 24),
        Some(PPC_READ_REFERENCE)
    );
}

#[test]
fn hle_import_runner_surfaces_unmapped_memory_after_nonzero_handler_result() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let fault_pc = install_test_unmapped_store(&mut loaded);
    install_test_native_exception_handler(&mut loaded, &[d_form_u(14, 3, 0, u16::MAX), BLR]);

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(
        probe.result,
        PpcRunResult::MemoryFault {
            pc,
            addr: 0x1000_0000,
            was_write: true,
            ..
        } if pc == fault_pc
    ));
    assert_eq!(loaded.cpu.pc, fault_pc);
    assert!(loaded.native_exception_stack.is_empty());
}

#[test]
fn hle_import_runner_retains_unmapped_memory_delivery_between_slices() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    install_test_unmapped_store(&mut loaded);
    install_test_native_exception_handler(&mut loaded, &memory_exception_resuming_handler());

    let first = loaded.run_with_hle_imports(1);
    assert_eq!(first.result, PpcRunResult::CycleLimit { cycles: 1 });
    assert_eq!(loaded.native_exception_stack.len(), 1);

    let second = loaded.run_with_hle_imports(128);
    assert!(matches!(second.result, PpcRunResult::Halted { pc: 0, .. }));
    assert!(loaded.native_exception_stack.is_empty());
}

#[test]
fn hle_import_runner_rejects_malformed_unmapped_memory_exception_frame() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let fault_pc = install_test_unmapped_store(&mut loaded);
    install_test_native_exception_handler(
        &mut loaded,
        &[
            d_form_u(14, 4, 0, 0),
            d_form_u(36, 4, 3, 16),
            d_form_u(14, 3, 0, 0),
            BLR,
        ],
    );

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(
        probe.result,
        PpcRunResult::MemoryFault {
            pc,
            addr: 0x1000_0000,
            was_write: true,
            ..
        } if pc == fault_pc
    ));
    assert_eq!(loaded.cpu.pc, fault_pc);
    assert!(loaded.native_exception_stack.is_empty());
}

#[test]
fn hle_import_runner_surfaces_nonzero_native_exception_handler_result() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let fault_pc = install_test_fault_words(&mut loaded, &[0, BLR]);
    install_test_native_exception_handler(&mut loaded, &[d_form_u(14, 3, 0, u16::MAX), BLR]);

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(
        probe.result,
        PpcRunResult::Exception {
            pc,
            exception: PpcException::IllegalInstruction { word: 0, .. },
            ..
        } if pc == fault_pc
    ));
    assert_eq!(loaded.cpu.pc, fault_pc);
    assert!(loaded.native_exception_stack.is_empty());
}

#[test]
fn hle_import_runner_retains_native_exception_delivery_between_slices() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    install_test_fault_words(
        &mut loaded,
        &[
            0x7c80_0008,
            d_form_u(14, 0, 0, 0),
            xfx_form(31, 0, 8, 467),
            BLR,
        ],
    );
    install_test_native_exception_handler(
        &mut loaded,
        &[
            d_form_u(32, 4, 3, 4),
            d_form_u(32, 5, 4, 20),
            d_form_u(14, 5, 5, 4),
            d_form_u(36, 5, 4, 20),
            d_form_u(14, 3, 0, 0),
            BLR,
        ],
    );

    let first = loaded.run_with_hle_imports(1);
    assert_eq!(first.result, PpcRunResult::CycleLimit { cycles: 1 });
    assert_eq!(loaded.native_exception_stack.len(), 1);

    let second = loaded.run_with_hle_imports(64);
    assert!(matches!(second.result, PpcRunResult::Halted { pc: 0, .. }));
    assert!(loaded.native_exception_stack.is_empty());
}

#[test]
fn hle_import_runner_discards_native_exception_state_after_terminal_handler_fault() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    install_test_fault_words(&mut loaded, &[0]);
    let tvector = PPC_DATA_BASE + 0x1000;
    let unmapped_entry = PPC_CODE_BASE + 0x2000;
    loaded.memory.add_region(tvector, vec![0; 8]);
    loaded.memory.write_u32_be(tvector, unmapped_entry).unwrap();
    loaded.native_exception_handler = tvector;

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(
        probe.result,
        PpcRunResult::FetchFault { pc, .. } if pc == unmapped_entry
    ));
    assert!(loaded.native_exception_stack.is_empty());
}
