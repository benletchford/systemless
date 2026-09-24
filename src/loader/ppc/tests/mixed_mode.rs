use super::*;

pub(crate) fn test_stack_proc_info(result_size: u32, parameter_sizes: &[u32]) -> u32 {
    let mut proc_info =
        PPC_PROCINFO_PASCAL_STACK_BASED | (result_size << PPC_PROCINFO_RESULT_SIZE_PHASE);
    for (index, size) in parameter_sizes.iter().copied().enumerate() {
        proc_info |= size
            << (PPC_PROCINFO_STACK_PARAMETER_PHASE
                + u32::try_from(index).unwrap() * PPC_PROCINFO_STACK_PARAMETER_WIDTH);
    }
    proc_info
}

pub(crate) fn test_register_proc_info(result_size: u32, parameters: &[(u32, u32)]) -> u32 {
    let mut proc_info =
        PPC_PROCINFO_REGISTER_BASED | (result_size << PPC_PROCINFO_RESULT_SIZE_PHASE);
    for (index, (which_register, size)) in parameters.iter().copied().enumerate() {
        let field = size | (which_register << PPC_PROCINFO_REGISTER_PARAMETER_WHICH_SHIFT);
        proc_info |= field
            << (PPC_PROCINFO_REGISTER_PARAMETER_PHASE
                + u32::try_from(index).unwrap() * PPC_PROCINFO_REGISTER_PARAMETER_WIDTH);
    }
    proc_info
}

pub(crate) fn test_register_proc_info_with_result_location(
    result_size: u32,
    result_location: u32,
    parameters: &[(u32, u32)],
) -> u32 {
    test_register_proc_info(result_size, parameters)
        | (result_location << PPC_PROCINFO_REGISTER_RESULT_LOCATION_PHASE)
}

pub(crate) fn test_dispatched_stack_proc_info(
    calling_convention: u32,
    result_size: u32,
    selector_size: u32,
    parameter_sizes: &[u32],
) -> u32 {
    let mut proc_info = calling_convention
        | (result_size << PPC_PROCINFO_RESULT_SIZE_PHASE)
        | (selector_size << PPC_PROCINFO_DISPATCHED_SELECTOR_SIZE_PHASE);
    for (index, size) in parameter_sizes.iter().copied().enumerate() {
        proc_info |= size
            << (PPC_PROCINFO_DISPATCHED_PARAMETER_PHASE
                + u32::try_from(index).unwrap() * PPC_PROCINFO_STACK_PARAMETER_WIDTH);
    }
    proc_info
}

pub(crate) fn install_test_powerpc_callback(
    loaded: &mut PpcLoadedApp,
    descriptor: u32,
    tvector: u32,
    callback_entry: u32,
    callback_rtoc: u32,
    proc_info: u32,
    callback_words: &[u32],
) {
    let mut callback = Vec::new();
    for word in callback_words {
        callback.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(callback_entry, callback);
    loaded.memory.add_region(callback_rtoc, vec![0; 0x100]);
    loaded.memory.add_region(descriptor, vec![0; 0x100]);
    loaded
        .memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    loaded
        .memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    loaded.memory.write_u16_be(descriptor + 10, 0).unwrap();
    assert!(ppc_write_routine_record(
        &mut loaded.memory,
        descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
        proc_info,
        PPC_ROUTINE_RECORD_POWERPC_ISA,
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        tvector,
    ));
    loaded.memory.write_u32_be(tvector, callback_entry).unwrap();
    loaded
        .memory
        .write_u32_be(tvector + 4, callback_rtoc)
        .unwrap();
}

pub(crate) fn install_test_m68k_callback(
    loaded: &mut PpcLoadedApp,
    descriptor: u32,
    callback_entry: u32,
    proc_info: u32,
    routine_flags: u16,
    callback_words: &[u16],
) {
    let mut callback = Vec::new();
    for word in callback_words {
        callback.extend_from_slice(&word.to_be_bytes());
    }
    loaded.set_heap_cursor(
        loaded
            .heap_cursor()
            .max(descriptor + 0x100)
            .max(callback_entry + u32::try_from(callback.len()).unwrap()),
    );
    loaded.memory.add_region(callback_entry, callback);
    loaded.memory.add_region(descriptor, vec![0; 0x100]);
    loaded
        .memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    loaded
        .memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    loaded.memory.write_u16_be(descriptor + 10, 0).unwrap();
    assert!(ppc_write_routine_record(
        &mut loaded.memory,
        descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
        proc_info,
        PPC_ROUTINE_RECORD_M68K_ISA,
        routine_flags,
        callback_entry,
    ));
}

#[test]
fn mixed_mode_storage_does_not_publish_a_partial_pair_on_allocation_failure() {
    let mut memory = PpcSectionMem::new();
    let storage = SharedProcessMixedModeM68kState::default();
    let initial_heap_cursor = 0x1000;
    let mut heap_cursor = initial_heap_cursor;
    let gateway_size = ppc_allocation_size(PPC_MIXED_MODE_M68K_GATEWAY_SIZE).unwrap();
    let heap_limit = initial_heap_cursor + gateway_size + 16;

    assert_eq!(
        ppc_mixed_mode_m68k_storage(
            None,
            &mut memory,
            &mut heap_cursor,
            heap_limit,
            &storage,
        ),
        None
    );
    assert_eq!(storage.storage_pair(), (0, 0));
    assert_eq!(heap_cursor, initial_heap_cursor);
}

#[test]
fn attached_mixed_mode_adapters_reuse_storage_without_allocating() {
    let pef = synthetic_pef_with_import(b"TestImport");
    let mut first = load_pef_application(&pef).unwrap();
    let mut second = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    first.attach_unconverted_process_services(&mut context);
    second.attach_unconverted_process_services(&mut context);
    assert!(first
        .toolbox_startup
        .mixed_mode_m68k
        .ptr_eq(&second.toolbox_startup.mixed_mode_m68k));

    let first_manager_handle = first.process_memory_manager.0.clone();
    let mut first_manager = first_manager_handle.borrow_mut();
    let initial_heap = first_manager.native_heap_state().unwrap();
    let mut heap_cursor = initial_heap.heap_cursor;
    let pair = ppc_mixed_mode_m68k_storage(
        Some(first_manager.native_mut()),
        &mut first.memory,
        &mut heap_cursor,
        initial_heap.heap_limit,
        &first.toolbox_startup.mixed_mode_m68k,
    )
    .unwrap();
    drop(first_manager);
    assert_eq!(second.toolbox_startup.mixed_mode_m68k.storage_pair(), pair);

    let second_manager_handle = second.process_memory_manager.0.clone();
    let mut second_manager = second_manager_handle.borrow_mut();
    let second_heap_cursor_before = second_manager.native_heap_state().unwrap().heap_cursor;
    let mut impossible_heap_cursor = 0;
    assert_eq!(
        ppc_mixed_mode_m68k_storage(
            Some(second_manager.native_mut()),
            &mut second.memory,
            &mut impossible_heap_cursor,
            0,
            &second.toolbox_startup.mixed_mode_m68k,
        ),
        Some(pair)
    );
    assert_eq!(impossible_heap_cursor, 0);
    assert_eq!(
        second_manager.native_heap_state().unwrap().heap_cursor,
        second_heap_cursor_before
    );
}

#[test]
fn mixed_mode_storage_rolls_back_native_allocator_on_failure() {
    let heap_base = 0x1000;
    let gateway_size = ppc_allocation_size(PPC_MIXED_MODE_M68K_GATEWAY_SIZE).unwrap();
    let heap_limit = heap_base + gateway_size + 16;
    let mut memory_manager = ProcessNativeMemoryManager::default();
    memory_manager.publish_native_allocator(
        ProcessNativeHeapState {
            heap_base,
            heap_cursor: heap_base,
            heap_limit,
            last_mem_error: PPC_NO_ERR,
            heap_maximized: false,
            master_pointer_blocks_requested: 0,
        },
        &[],
        &[],
        &[],
    );
    let allocator_before = memory_manager.native_allocator_snapshot();
    let mut memory = PpcSectionMem::new();
    let mut heap_cursor = heap_base;
    let storage = SharedProcessMixedModeM68kState::default();

    assert_eq!(
        ppc_mixed_mode_m68k_storage(
            Some(&mut memory_manager),
            &mut memory,
            &mut heap_cursor,
            heap_limit,
            &storage,
        ),
        None
    );
    assert_eq!(storage.storage_pair(), (0, 0));
    assert_eq!(heap_cursor, heap_base);
    assert_eq!(memory_manager.native_allocator_snapshot(), allocator_before);
    assert_eq!(
        memory_manager.native_heap_state().unwrap().heap_cursor,
        heap_base
    );
}

#[test]
fn import_bindings_classify_mixed_mode_imports() {
    for (symbol, target) in [
        (
            "NewRoutineDescriptor",
            PpcImportDispatcherTarget::NewRoutineDescriptor,
        ),
        (
            "NewFatRoutineDescriptor",
            PpcImportDispatcherTarget::NewFatRoutineDescriptor,
        ),
        (
            "DisposeRoutineDescriptor",
            PpcImportDispatcherTarget::DisposeRoutineDescriptor,
        ),
        (
            "CallUniversalProc",
            PpcImportDispatcherTarget::CallUniversalProc,
        ),
        (
            "CallOSTrapUniversalProc",
            PpcImportDispatcherTarget::CallOSTrapUniversalProc,
        ),
        (
            "NGetTrapAddress",
            PpcImportDispatcherTarget::NGetTrapAddress,
        ),
        (
            "GetToolTrapAddress",
            PpcImportDispatcherTarget::GetToolTrapAddress,
        ),
        (
            "GetOSTrapAddress",
            PpcImportDispatcherTarget::GetOSTrapAddress,
        ),
        (
            "SetToolTrapAddress",
            PpcImportDispatcherTarget::SetToolTrapAddress,
        ),
        (
            "SetOSTrapAddress",
            PpcImportDispatcherTarget::SetOSTrapAddress,
        ),
        (
            "NSetTrapAddress",
            PpcImportDispatcherTarget::NSetTrapAddress,
        ),
        ("LMGetCurrentA5", PpcImportDispatcherTarget::LMGetCurrentA5),
        ("VInstall", PpcImportDispatcherTarget::VInstall),
        ("VRemove", PpcImportDispatcherTarget::VRemove),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            target,
            "{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetTrapAddress"),
        PpcImportDispatcherTarget::Unsupported
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetTrapAddress"),
        PpcImportDispatcherTarget::Unsupported
    );
}

#[test]
fn hle_import_runner_builds_new_routine_descriptor() {
    let pef = synthetic_pef_with_import(b"NewRoutineDescriptor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_CODE_BASE;
    loaded.cpu.gpr[4] = 0x1234_5678;
    loaded.cpu.gpr[5] = PPC_ROUTINE_RECORD_POWERPC_ISA as u32;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 8,
        }
    );
    let descriptor = loaded.cpu.gpr[3];
    assert_eq!(descriptor, PPC_HEAP_BASE);
    assert_eq!(
        loaded.heap_cursor(),
        PPC_HEAP_BASE + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE + PPC_ROUTINE_RECORD_SIZE
    );
    assert_eq!(
        loaded.memory.read_u16_be(descriptor),
        Some(PPC_MIXED_MODE_TRAP)
    );
    assert_eq!(
        loaded.memory.read_u8(descriptor + 2),
        Some(PPC_ROUTINE_DESCRIPTOR_VERSION)
    );
    assert_eq!(loaded.memory.read_u16_be(descriptor + 10), Some(0));
    let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    assert_eq!(loaded.memory.read_u32_be(record), Some(0x1234_5678));
    assert_eq!(
        loaded
            .memory
            .read_u8(record + PPC_ROUTINE_RECORD_ISA_OFFSET),
        Some(PPC_ROUTINE_RECORD_POWERPC_ISA)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(record + PPC_ROUTINE_RECORD_FLAGS_OFFSET),
        Some(PPC_ROUTINE_FLAG_USE_NATIVE_ISA)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET),
        Some(PPC_CODE_BASE)
    );
}

#[test]
fn hle_import_runner_builds_m68k_new_routine_descriptor_without_native_flag() {
    let pef = synthetic_pef_with_import(b"NewRoutineDescriptor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0x00c0_ffee;
    loaded.cpu.gpr[4] = 0x1234_5678;
    loaded.cpu.gpr[5] = PPC_ROUTINE_RECORD_M68K_ISA as u32;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 8,
        }
    );
    let descriptor = loaded.cpu.gpr[3];
    assert_eq!(
        loaded
            .ptrs()
            .iter()
            .find(|record| record.ptr == descriptor)
            .map(|record| record.size),
        Some(PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE + PPC_ROUTINE_RECORD_SIZE)
    );
    let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    assert_eq!(
        loaded
            .memory
            .read_u8(record + PPC_ROUTINE_RECORD_ISA_OFFSET),
        Some(PPC_ROUTINE_RECORD_M68K_ISA)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(record + PPC_ROUTINE_RECORD_FLAGS_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET),
        Some(0x00c0_ffee)
    );
}

#[test]
fn hle_import_runner_builds_new_fat_routine_descriptor() {
    let pef = synthetic_pef_with_import(b"NewFatRoutineDescriptor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 0x00c0_ffee;
    loaded.cpu.gpr[4] = PPC_CODE_BASE;
    loaded.cpu.gpr[5] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 8,
        }
    );
    let descriptor = loaded.cpu.gpr[3];
    assert_eq!(
        loaded
            .ptrs()
            .iter()
            .find(|record| record.ptr == descriptor)
            .map(|record| record.size),
        Some(PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE + (PPC_ROUTINE_RECORD_SIZE * 2))
    );
    assert_eq!(descriptor, PPC_HEAP_BASE);
    assert_eq!(
        loaded.heap_cursor(),
        PPC_HEAP_BASE
            + ppc_allocation_size(
                PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE + (PPC_ROUTINE_RECORD_SIZE * 2),
            )
            .unwrap()
    );
    assert_eq!(
        loaded.memory.read_u16_be(descriptor),
        Some(PPC_MIXED_MODE_TRAP)
    );
    assert_eq!(
        loaded.memory.read_u8(descriptor + 2),
        Some(PPC_ROUTINE_DESCRIPTOR_VERSION)
    );
    assert_eq!(loaded.memory.read_u16_be(descriptor + 10), Some(1));
    let m68k_record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    assert_eq!(loaded.memory.read_u32_be(m68k_record), Some(0x1234_5678));
    assert_eq!(
        loaded
            .memory
            .read_u8(m68k_record + PPC_ROUTINE_RECORD_ISA_OFFSET),
        Some(PPC_ROUTINE_RECORD_M68K_ISA)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(m68k_record + PPC_ROUTINE_RECORD_FLAGS_OFFSET),
        Some(0)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(m68k_record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET),
        Some(0x00c0_ffee)
    );
    let powerpc_record = m68k_record + PPC_ROUTINE_RECORD_SIZE;
    assert_eq!(loaded.memory.read_u32_be(powerpc_record), Some(0x1234_5678));
    assert_eq!(
        loaded
            .memory
            .read_u8(powerpc_record + PPC_ROUTINE_RECORD_ISA_OFFSET),
        Some(PPC_ROUTINE_RECORD_POWERPC_ISA)
    );
    assert_eq!(
        loaded
            .memory
            .read_u16_be(powerpc_record + PPC_ROUTINE_RECORD_FLAGS_OFFSET),
        Some(PPC_ROUTINE_FLAG_USE_NATIVE_ISA)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(powerpc_record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET),
        Some(PPC_CODE_BASE)
    );
}

#[test]
fn hle_import_runner_disposes_and_reuses_routine_descriptor_storage() {
    let pef = synthetic_pef_with_import(b"DisposeRoutineDescriptor");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewRoutineDescriptor;
    loaded.cpu.gpr[3] = 0x00c0_ffee;
    loaded.cpu.gpr[4] = 0x1234_5678;
    loaded.cpu.gpr[5] = PPC_ROUTINE_RECORD_M68K_ISA as u32;

    let create_probe = loaded.run_with_hle_imports(64);

    assert_eq!(create_probe.handled_import_count, 1);
    assert_eq!(create_probe.unsupported_import_index, None);
    let descriptor = loaded.cpu.gpr[3];
    let descriptor_size = PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE + PPC_ROUTINE_RECORD_SIZE;
    assert_eq!(
        loaded
            .ptrs()
            .iter()
            .find(|record| record.ptr == descriptor)
            .map(|record| record.size),
        Some(descriptor_size)
    );
    let heap_cursor = loaded.heap_cursor();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeRoutineDescriptor;
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = descriptor;

    let dispose_probe = loaded.run_with_hle_imports(64);

    assert_eq!(dispose_probe.handled_import_count, 1);
    assert_eq!(dispose_probe.unsupported_import_index, None);
    assert_eq!(
        dispose_probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 8,
        }
    );
    assert_eq!(loaded.cpu.gpr[3], descriptor);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert!(!loaded.ptrs().iter().any(|record| record.ptr == descriptor));
    assert!(loaded
        .free_ptr_blocks()
        .iter()
        .any(|record| record.ptr == descriptor && record.size == descriptor_size));

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewRoutineDescriptor;
    loaded.cpu.gpr[3] = 0x00c0_ffee;
    loaded.cpu.gpr[4] = 0x1234_5678;
    loaded.cpu.gpr[5] = PPC_ROUTINE_RECORD_M68K_ISA as u32;

    let recreate_probe = loaded.run_with_hle_imports(64);

    assert_eq!(recreate_probe.handled_import_count, 1);
    assert_eq!(recreate_probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], descriptor);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(loaded
        .ptrs()
        .iter()
        .any(|record| record.ptr == descriptor && record.size == descriptor_size));
    assert!(!loaded
        .free_ptr_blocks()
        .iter()
        .any(|record| record.ptr == descriptor));
}

#[test]
fn hle_import_runner_does_not_dispose_untracked_routine_descriptor() {
    let pef = synthetic_pef_with_import(b"DisposeRoutineDescriptor");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    let ptrs = loaded.ptrs().clone();
    let free_ptr_blocks = loaded.free_ptr_blocks().clone();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x40;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE + 0x40);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.ptrs(), ptrs);
    assert_eq!(loaded.free_ptr_blocks(), free_ptr_blocks);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn hle_import_runner_call_universal_proc_enters_native_ppc_descriptor_and_restores_rtoc() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_stack_proc_info(
        PPC_PROCINFO_SIZE_FOUR,
        &[PPC_PROCINFO_SIZE_FOUR, PPC_PROCINFO_SIZE_FOUR],
    );

    let stw_r3_0_r2 = (36u32 << 26) | (3u32 << 21) | (2u32 << 16);
    let stw_r4_4_r2 = (36u32 << 26) | (4u32 << 21) | (2u32 << 16) | 4;
    let stw_r2_8_r2 = (36u32 << 26) | (2u32 << 21) | (2u32 << 16) | 8;
    let addi_r3_r3_7 = (14u32 << 26) | (3u32 << 21) | (3u32 << 16) | 7;
    let mut callback = Vec::new();
    for word in [stw_r3_0_r2, stw_r4_4_r2, stw_r2_8_r2, addi_r3_r3_7, BLR] {
        callback.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(callback_entry, callback);
    loaded.memory.add_region(callback_rtoc, vec![0; 0x100]);
    loaded.memory.add_region(descriptor, vec![0; 0x100]);
    loaded
        .memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    loaded
        .memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    loaded.memory.write_u16_be(descriptor + 10, 0).unwrap();
    ppc_write_routine_record(
        &mut loaded.memory,
        descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
        proc_info,
        PPC_ROUTINE_RECORD_POWERPC_ISA,
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        tvector,
    );
    loaded.memory.write_u32_be(tvector, callback_entry).unwrap();
    loaded
        .memory
        .write_u32_be(tvector + 4, callback_rtoc)
        .unwrap();

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x10;
    loaded.cpu.gpr[6] = 0x20;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 14,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x17);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x10));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 4), Some(0x20));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 8),
        Some(callback_rtoc)
    );
    assert!(loaded.guest_calls().is_empty());
}

#[test]
fn reverse_mixed_mode_activation_uses_the_native_worker_stack_and_retries_overflow() {
    use crate::guest_call::{ExecutionTaskId, GuestCallTarget, PowerPcArguments};
    const MADE: u32 = PPC_DATA_BASE + 0x2000;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    loaded.memory.add_region(MADE, vec![0; 4]);
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = PPC_CODE_BASE;
    loaded.cpu.gpr[5] = 17;
    loaded.cpu.gpr[6] = 4096;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = 0;
    loaded.cpu.gpr[9] = MADE;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let worker = ExecutionTaskId::from_thread_id(loaded.memory.read_u32_be(MADE).unwrap());
    assert!(loaded.toolbox_startup.execution.calls()
        .yield_native_thread(&mut loaded.cpu, worker.thread_id())
        .unwrap());
    let storage = loaded.guest_calls().thread_storage(worker).unwrap();
    let worker_sp = loaded.cpu.gpr[1];
    assert!(worker_sp < loaded.stack_base);
    assert!(loaded.guest_calls()
        .switch_to_task(ExecutionTaskId::APPLICATION));
    assert_eq!(
        loaded.guest_calls()
            .native_stack_bounds(loaded.stack_base, PPC_STACK_TOP),
        Some((storage.stack_base, storage.stack_limit))
    );
    assert!(loaded.guest_calls().switch_to_task(worker));
    assert!(loaded.guest_calls().begin_m68k_to_powerpc(
        GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: PPC_CODE_BASE + 0x1000,
            rtoc: PPC_DATA_BASE,
        },
        PowerPcArguments::from_slice(&[42]).unwrap(),
        0x0010_0000,
        0x0010_1000,
        None,
    ));
    let mut classic = crate::cpu::M68kCpu::new();
    classic.write_reg(crate::cpu::Register::PC, 0x0010_0000);
    loaded
        .memory
        .add_region(storage.stack_base - 64, vec![0; 64]);
    // Both surrounding addresses are mapped. Mapping
    // alone must not permit a frame outside this worker's allocation.
    for invalid_sp in [storage.stack_base + 16, storage.stack_limit + 16] {
        loaded.cpu.gpr[1] = invalid_sp;
        let before = loaded.cpu.clone();
        let sentinel = invalid_sp - 64;
        loaded.memory.write_u32_be(sentinel, 0xface_cafe).unwrap();
        assert!(loaded.activate_powerpc_from_m68k(&mut classic).is_none());
        assert_eq!(loaded.cpu.gpr, before.gpr);
        assert_eq!(loaded.cpu.pc, before.pc);
        assert_eq!(loaded.cpu.lr, before.lr);
        assert_eq!(loaded.cpu.cr, before.cr);
        assert_eq!(loaded.cpu.fpr, before.fpr);
        assert_eq!(classic.read_reg(crate::cpu::Register::PC), 0x0010_0000);
        assert_eq!(loaded.memory.read_u32_be(sentinel), Some(0xface_cafe));
        assert!(loaded.guest_calls().pending_powerpc_from_m68k().is_some());
        assert!(!loaded.guest_calls().has_parked_m68k_contexts());
    }
    loaded.cpu.gpr[1] = worker_sp;
    loaded.activate_powerpc_from_m68k(&mut classic).unwrap();
    let callback_sp = loaded.cpu.gpr[1];
    assert!(callback_sp >= storage.stack_base && callback_sp < worker_sp);
    assert_eq!(callback_sp & 15, 0);
    assert_eq!(loaded.memory.read_u32_be(callback_sp), Some(worker_sp));
    assert_eq!(loaded.cpu.gpr[3], 42);
    assert_eq!(loaded.cpu.pc, PPC_CODE_BASE + 0x1000);
    assert!(loaded.guest_calls().has_parked_m68k_contexts());
}

#[test]
fn reverse_mixed_mode_activation_builds_a_protected_native_parameter_area() {
    let pef = synthetic_pef();
    let mut loaded = load_pef_application(&pef).unwrap();
    let caller = loaded.cpu.clone();
    let values: Vec<u32> = (1..=crate::guest_call::MAX_POWERPC_GUEST_ARGUMENTS as u32)
        .map(|value| 0x1000_0000 | value)
        .collect();
    let arguments = crate::guest_call::PowerPcArguments::from_slice(&values).unwrap();
    assert!(loaded.guest_calls().begin_m68k_to_powerpc(
        crate::guest_call::GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: PPC_CODE_BASE + 0x1000,
            rtoc: PPC_DATA_BASE + 0x2000,
        },
        arguments,
        0x0010_0000,
        0x0010_1000,
        None,
    ));

    loaded
        .activate_powerpc_from_m68k(&mut crate::cpu::M68kCpu::new())
        .unwrap();

    let callback_sp = loaded.cpu.gpr[1];
    assert!(callback_sp < caller.gpr[1]);
    assert_eq!(callback_sp & 0x0f, 0);
    assert_eq!(
        loaded
            .memory
            .read_u32_be(callback_sp + PPC_LINKAGE_BACK_CHAIN_OFFSET),
        Some(caller.gpr[1])
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(callback_sp + PPC_LINKAGE_SAVED_CR_OFFSET),
        Some(caller.cr)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(callback_sp + PPC_LINKAGE_SAVED_LR_OFFSET),
        Some(caller.lr)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(callback_sp + PPC_LINKAGE_SAVED_RTOC_OFFSET),
        Some(caller.gpr[2])
    );
    assert_eq!(
        &loaded.cpu.gpr[3..=10],
        &values[..PPC_NATIVE_PARAMETER_GPR_COUNT]
    );
    for (index, value) in values.into_iter().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u32_be(ppc_parameter_area_slot_addr(callback_sp, index).unwrap()),
            Some(value)
        );
    }
    assert_eq!(loaded.cpu.pc, PPC_CODE_BASE + 0x1000);
    assert_eq!(loaded.cpu.lr, PPC_GUEST_CALL_RETURN_PC);
    assert_eq!(loaded.cpu.gpr[2], PPC_DATA_BASE + 0x2000);
}

#[test]
fn hle_import_runner_call_universal_proc_rejects_m68k_only_descriptor() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_stack_proc_info(PPC_PROCINFO_SIZE_FOUR, &[PPC_PROCINFO_SIZE_FOUR]);
    loaded.memory.add_region(descriptor, vec![0; 0x100]);
    loaded
        .memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    loaded
        .memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    loaded.memory.write_u16_be(descriptor + 10, 0).unwrap();
    assert!(ppc_write_routine_record(
        &mut loaded.memory,
        descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
        proc_info,
        PPC_ROUTINE_RECORD_M68K_ISA,
        0,
        0x00c0_ffee,
    ));

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 0);
    assert_eq!(probe.unsupported_import_index, Some(0));
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_IMPORT_TRAP_BASE,
            cycles: 4,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], descriptor);
}

#[test]
fn hle_import_runner_call_universal_proc_rejects_fake_trap_addresses() {
    // Inside Macintosh: PowerPC System Software (1994), pp. 1-67--1-68
    // and 2-42--2-43: CallUniversalProc accepts an actual universal
    // procedure pointer. A host-encoded trap word is not a procedure.
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let fake_address = 0x00F0_AADBu32;
    loaded.cpu.gpr[3] = fake_address;
    loaded.cpu.gpr[4] = test_stack_proc_info(PPC_PROCINFO_SIZE_TWO, &[]);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 0);
    assert_eq!(probe.unsupported_import_index, Some(0));
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_IMPORT_TRAP_BASE,
            cycles: 4,
        }
    );
    assert_eq!(loaded.cpu.gpr[3], fake_address);
}

#[test]
fn ppc_to_m68k_special_cases_build_every_classic_input_layout() {
    use crate::guest_call::M68kResultSource;
    use crate::guest_procedure::GuestProcedureRepresentation;
    use crate::mixed_mode::special_case;

    const RECT: u32 = PPC_DATA_BASE + 0x4000;
    const RESULT: u32 = PPC_DATA_BASE + 0x4010;
    const ENTRY: u32 = 0x00c0_ffee;

    let a5 = PPC_DATA_BASE;
    let cases = vec![
        (
            special_case::HIGH_HOOK,
            vec![RECT, 0x1300],
            [0; 8],
            [0, 0, 0, 0x1300, 0, a5, 0],
            (1u8..=8).collect::<Vec<_>>(),
            false,
        ),
        (
            special_case::EOL_HOOK,
            vec![0xffff_fff2, 0x1300, 0x1400],
            [0xf2, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 0, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::WIDTH_HOOK,
            vec![0x80f2, 0xff01, 0x1000, 0x1300, 0x1400],
            [0x80f2, 0xff01, 0, 0, 0, 0, 0, 0],
            [0x1000, 0, 0, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::NWIDTH_HOOK,
            vec![
                0x80f2,
                0xff01,
                0xffff_8002,
                0xffff_8001,
                0x1000,
                0x1200,
                0x1300,
                0x1400,
            ],
            [0x80f2, 0xff01, 0x8001_8002, 0, 0, 0, 0, 0],
            [0x1000, 0, 0x1200, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::DRAW_HOOK,
            vec![0x80f2, 0xff01, 0x1000, 0x1300, 0x1400],
            [0x80f2, 0xff01, 0, 0, 0, 0, 0, 0],
            [0x1000, 0, 0, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::HIT_TEST_HOOK,
            vec![
                0x80f2, 0xff01, 0x8002, 0x1000, 0x1300, 0x1400, 0x1500, 0x1600, 0x1700,
            ],
            [0x80f2, 0xff01, 0x8002, 0, 0, 0, 0, 0],
            [0x1000, 0, 0, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::TE_FIND_WORD,
            vec![0x80f2, 0xffff_8002, 0x1300, 0x1400, 0x1500, 0x1600],
            [0x80f2, 0, 0x8002, 0, 0, 0, 0, 0],
            [0, 0, 0, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::PROTOCOL_HANDLER,
            vec![0x1000, 0x1100, 0x1200, 0x1300, 0x1400, 0xffff_ff01],
            [0, 0xff01, 0, 0, 0, 0, 0, 0],
            [0x1000, 0x1100, 0x1200, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::SOCKET_LISTENER,
            vec![
                0x1000,
                0x1100,
                0x1200,
                0x1300,
                0x1400,
                0xffff_fff2,
                0xffff_ff01,
            ],
            [0xf2, 0xff01, 0, 0, 0, 0, 0, 0],
            [0x1000, 0x1100, 0x1200, 0x1300, 0x1400, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::TE_RECALC,
            vec![0x1300, 0xffff_fffe, 0x1500, 0x1600, 0x1700],
            [0, 0, 0, 0, 0, 0, 0, 0xfffe],
            [0, 0, 0, 0x1300, 0, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::TE_DO_TEXT,
            vec![0x1300, 0x1234, 0x5678, 0xffff_fffe, 0x1500, 0x1600],
            [0, 0, 0, 0x1234, 0x5678, 0, 0, 0xfffe],
            [0, 0, 0, 0x1300, 0, a5, 0],
            vec![],
            false,
        ),
        (
            special_case::GNE_FILTER_PROC,
            vec![0x1100, RESULT],
            [1, 0, 0, 0, 0, 0, 0, 0],
            [0, 0x1100, 0, 0, 0, a5, 0],
            vec![0, 1],
            true,
        ),
        (
            special_case::MBAR_HOOK,
            vec![0xcafe_babe],
            [0; 8],
            [0, 0, 0, 0, 0, a5, 0],
            0xcafe_babeu32.to_be_bytes().to_vec(),
            false,
        ),
    ];

    for (selector, arguments, data, address, stack, has_stack_result) in cases {
        let pef = synthetic_pef();
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.memory.add_region(RECT, vec![0; 0x20]);
        for (offset, byte) in (1u8..=8).enumerate() {
            loaded
                .memory
                .write_u8(RECT + u32::try_from(offset).unwrap(), byte)
                .unwrap();
        }
        loaded.memory.write_u8(RESULT, 1).unwrap();
        let proc_info = PPC_PROCINFO_SPECIAL_CASE
            | (selector << crate::mixed_mode::special_case::SELECTOR_PHASE);
        let expected_arguments =
            crate::guest_call::PowerPcArguments::from_slice(&arguments).unwrap();
        let target = GuestProcedure {
            original_pointer: ENTRY,
            representation: GuestProcedureRepresentation::RawCode,
            isa: GuestIsa::M68k,
            entry: ENTRY,
            rtoc: 0,
            proc_info,
            routine_flags: 0,
        };
        let action = ppc_begin_m68k_universal_proc(
            &loaded.cpu,
            None,
            &mut loaded.memory,
            test_heap_cursor!(loaded),
            test_heap_limit!(loaded),
            &mut loaded.toolbox_startup,
            target,
            proc_info,
            None,
            arguments,
            PPC_HALT_PC,
            ppc_call_universal_proc_return_gpr3(proc_info),
        );
        assert!(
            matches!(action, Some(PpcImportAction::Halt)),
            "selector {selector}"
        );
        let pending = loaded.guest_calls().activate_m68k().unwrap();
        assert_eq!(pending.registers.data, data, "selector {selector}");
        assert_eq!(pending.registers.address, address, "selector {selector}");
        assert_eq!(
            pending.final_sp,
            pending.initial_sp + 4,
            "selector {selector}",
        );
        for (offset, expected) in stack.into_iter().enumerate() {
            assert_eq!(
                loaded
                    .memory
                    .read_u8(pending.initial_sp + 4 + u32::try_from(offset).unwrap()),
                Some(expected),
                "selector {selector} stack byte {offset}",
            );
        }
        let Some(M68kResultSource::SpecialCase {
            selector: result_selector,
            arguments: result_arguments,
            stack_result,
        }) = pending.result
        else {
            panic!("selector {selector} did not retain a special-case result")
        };
        assert_eq!(u32::from(result_selector), selector);
        assert_eq!(result_arguments, expected_arguments);
        assert_eq!(
            stack_result,
            has_stack_result.then_some(pending.initial_sp + 4)
        );
    }
}

#[test]
fn hle_import_runner_call_universal_proc_executes_m68k_pascal_descriptor_and_result() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let proc_info = test_stack_proc_info(PPC_PROCINFO_SIZE_FOUR, &[PPC_PROCINFO_SIZE_FOUR]);
    install_test_m68k_callback(
        &mut loaded,
        descriptor,
        callback_entry,
        proc_info,
        0,
        &[
            0x202f, 0x0004, // MOVE.L 4(SP),D0
            0x5e80, // ADDQ.L #7,D0
            0x2f40, 0x0008, // MOVE.L D0,8(SP)
            0x4e74, 0x0004, // RTD #4
        ],
    );
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let pending = loaded.guest_calls().activate_m68k().unwrap();
    assert_eq!(
        loaded.memory.read_u32_be(pending.initial_sp + 4),
        Some(0x1234_5678)
    );
    assert_eq!(pending.final_sp, pending.initial_sp + 8);
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_567f);
    assert!(loaded.guest_calls().is_empty());
}

#[test]
fn hle_import_runner_call_universal_proc_lays_out_m68k_pascal_stack_sizes() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let proc_info = test_stack_proc_info(
        PPC_PROCINFO_SIZE_NONE,
        &[
            PPC_PROCINFO_SIZE_ONE,
            PPC_PROCINFO_SIZE_TWO,
            PPC_PROCINFO_SIZE_FOUR,
        ],
    );
    install_test_m68k_callback(
        &mut loaded,
        descriptor,
        callback_entry,
        proc_info,
        0,
        &[0x4e74, 0x0008], // RTD #8
    );
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x1234_5678;
    loaded.cpu.gpr[6] = 0x8765_4321;
    loaded.cpu.gpr[7] = 0xcafe_babe;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    let pending = loaded.guest_calls().activate_m68k().unwrap();
    let frame = pending.initial_sp;
    assert_eq!(loaded.memory.read_u32_be(frame), Some(pending.return_pc));
    assert_eq!(loaded.memory.read_u32_be(frame + 4), Some(0xcafe_babe));
    assert_eq!(loaded.memory.read_u16_be(frame + 8), Some(0x4321));
    assert_eq!(loaded.memory.read_u8(frame + 10), Some(0));
    assert_eq!(loaded.memory.read_u8(frame + 11), Some(0x78));
    assert_eq!(pending.final_sp, frame + 12);
    drain_test_m68k_guest_calls(&mut loaded);
}

#[test]
fn hle_import_runner_call_universal_proc_distinguishes_mpw_and_think_c_bytes() {
    for (convention, byte_offset) in [
        (PPC_PROCINFO_C_STACK_BASED, 1),
        (PPC_PROCINFO_THINK_C_STACK_BASED, 0),
    ] {
        let pef = synthetic_pef_with_import(b"CallUniversalProc");
        let mut loaded = load_pef_application(&pef).unwrap();
        let descriptor = PPC_HEAP_BASE + 0x1000;
        let callback_entry = PPC_HEAP_BASE + 0x2000;
        let proc_info = test_stack_proc_info(
            PPC_PROCINFO_SIZE_FOUR,
            &[
                PPC_PROCINFO_SIZE_ONE,
                PPC_PROCINFO_SIZE_TWO,
                PPC_PROCINFO_SIZE_FOUR,
            ],
        ) | convention;
        install_test_m68k_callback(
            &mut loaded,
            descriptor,
            callback_entry,
            proc_info,
            0,
            &[0x7007, 0x4e75], // MOVEQ #7,D0; RTS
        );
        loaded.cpu.gpr[3] = descriptor;
        loaded.cpu.gpr[4] = proc_info;
        loaded.cpu.gpr[5] = 0x1234_5678;
        loaded.cpu.gpr[6] = 0x8765_4321;
        loaded.cpu.gpr[7] = 0xcafe_babe;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        let pending = loaded.guest_calls().activate_m68k().unwrap();
        let frame = pending.initial_sp;
        assert_eq!(loaded.memory.read_u8(frame + 4 + byte_offset), Some(0x78));
        assert_eq!(loaded.memory.read_u16_be(frame + 6), Some(0x4321));
        assert_eq!(loaded.memory.read_u32_be(frame + 8), Some(0xcafe_babe));
        assert_eq!(pending.final_sp, frame + 4);
        drain_test_m68k_guest_calls(&mut loaded);
        assert_eq!(loaded.cpu.gpr[3], 7);
    }
}

#[test]
fn hle_import_runner_call_universal_proc_maps_m68k_register_arguments_and_result() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let proc_info = test_register_proc_info_with_result_location(
        PPC_PROCINFO_SIZE_FOUR,
        4,
        &[(1, PPC_PROCINFO_SIZE_TWO), (7, PPC_PROCINFO_SIZE_FOUR)],
    );
    install_test_m68k_callback(
        &mut loaded,
        descriptor,
        callback_entry,
        proc_info,
        0,
        &[
            0x207c, 0xcafe, 0xbabe, // MOVEA.L #$CAFEBABE,A0
            0x4e75, // RTS
        ],
    );
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0xabcd_7654;
    loaded.cpu.gpr[6] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    let pending = loaded.guest_calls().activate_m68k().unwrap();
    assert_eq!(pending.registers.data[1], 0x7654);
    assert_eq!(pending.registers.address[3], 0x1234_5678);
    assert_eq!(pending.registers.address[5], PPC_DATA_BASE);
    assert_eq!(pending.final_sp, pending.initial_sp + 4);
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0xcafe_babe);
}

#[test]
fn hle_import_runner_call_universal_proc_places_m68k_dispatched_selectors() {
    for (convention, selector_register, flags) in [
        (PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED, Some(0), 0),
        (PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED, Some(1), 0),
        (
            PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED,
            None,
            PPC_ROUTINE_FLAG_DONT_PASS_SELECTOR,
        ),
        (PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED, None, 0),
    ] {
        let pef = synthetic_pef_with_import(b"CallUniversalProc");
        let mut loaded = load_pef_application(&pef).unwrap();
        let descriptor = PPC_HEAP_BASE + 0x1000;
        let callback_entry = PPC_HEAP_BASE + 0x2000;
        let proc_info = test_dispatched_stack_proc_info(
            convention,
            PPC_PROCINFO_SIZE_NONE,
            PPC_PROCINFO_SIZE_TWO,
            &[PPC_PROCINFO_SIZE_FOUR],
        );
        let stack_selector = convention == PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED;
        let cleanup = if stack_selector && flags == 0 { 6 } else { 4 };
        install_test_m68k_callback(
            &mut loaded,
            descriptor,
            callback_entry,
            proc_info,
            flags,
            &[0x4e74, cleanup],
        );
        loaded.cpu.gpr[3] = descriptor;
        loaded.cpu.gpr[4] = proc_info;
        loaded.cpu.gpr[5] = 0x1234_5678;
        loaded.cpu.gpr[6] = 0xcafe_babe;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        let pending = loaded.guest_calls().activate_m68k().unwrap();
        if let Some(register) = selector_register {
            assert_eq!(pending.registers.data[register], 0x5678);
        } else {
            assert_eq!(pending.registers.data[0], 0);
            assert_eq!(pending.registers.data[1], 0);
        }
        assert_eq!(
            loaded.memory.read_u32_be(pending.initial_sp + 4),
            Some(0xcafe_babe)
        );
        if stack_selector {
            assert_eq!(
                loaded.memory.read_u16_be(pending.initial_sp + 8),
                Some(0x5678)
            );
        }
        drain_test_m68k_guest_calls(&mut loaded);
    }
}

#[test]
fn hle_import_runner_call_universal_proc_decodes_stack_procinfo_sizes() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_stack_proc_info(
        PPC_PROCINFO_SIZE_FOUR,
        &[
            PPC_PROCINFO_SIZE_ONE,
            PPC_PROCINFO_SIZE_TWO,
            PPC_PROCINFO_SIZE_FOUR,
        ],
    );
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(36, 5, 2, 8),
            d_form_u(32, 6, 1, PPC_PARAMETER_AREA_OFFSET as u16),
            d_form_u(32, 7, 1, (PPC_PARAMETER_AREA_OFFSET + 4) as u16),
            d_form_u(32, 8, 1, (PPC_PARAMETER_AREA_OFFSET + 8) as u16),
            d_form_u(36, 6, 2, 12),
            d_form_u(36, 7, 2, 16),
            d_form_u(36, 8, 2, 20),
            BLR,
        ],
    );

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x1234_5678;
    loaded.cpu.gpr[6] = 0x8765_4321;
    loaded.cpu.gpr[7] = 0xcafe_babe;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 19,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x78);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x78));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 4), Some(0x4321));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 8),
        Some(0xcafe_babe)
    );
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 12), Some(0x78));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 16), Some(0x4321));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 20),
        Some(0xcafe_babe)
    );
}

#[test]
fn hle_import_runner_call_universal_proc_preserves_native_ppc_fpr_args_and_result() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let first = 1.25f64;
    let second = 2.5f64;
    let proc_info = test_stack_proc_info(
        PPC_PROCINFO_SIZE_NONE,
        &[PPC_PROCINFO_SIZE_FOUR, PPC_PROCINFO_SIZE_FOUR],
    );
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(54, 1, 2, 0),
            d_form_u(54, 2, 2, 8),
            a_form_fp(63, 1, 1, 2, 0, 21, false),
            d_form_u(54, 1, 2, 16),
            BLR,
        ],
    );

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0xaaaa_0001;
    loaded.cpu.gpr[6] = 0xbbbb_0002;
    loaded.cpu.fpr[1] = first.to_bits();
    loaded.cpu.fpr[2] = second.to_bits();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0xaaaa_0001);
    assert_eq!(
        loaded.memory.read_u64_be(callback_rtoc),
        Some(first.to_bits())
    );
    assert_eq!(
        loaded.memory.read_u64_be(callback_rtoc + 8),
        Some(second.to_bits())
    );
    assert_eq!(
        loaded.memory.read_u64_be(callback_rtoc + 16),
        Some((first + second).to_bits())
    );
    assert_eq!(f64::from_bits(loaded.cpu.fpr[1]), first + second);
}

#[test]
fn native_arguments_refuse_without_partial_register_or_stack_writes() {
    for failure in 0..5 {
        let mut cpu = PpcCpu::new();
        cpu.gpr.fill(0xfeed_beef);
        cpu.gpr[1] = match failure {
            3 => u32::MAX - 8,
            4 => u32::MAX - 39,
            _ => 0x8000,
        };
        cpu.pc = 0x1234;
        cpu.lr = 0x5678;
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x8018, vec![0xa5; 32]);
        match failure {
            1 => memory.add_readonly_region(0x8038, vec![0xa5; 4]),
            2 => {
                memory.add_region(0x8038, vec![0xa5; 4]);
                memory.add_readonly_region(0x803a, vec![0xa5; 1]);
            }
            _ => {}
        }
        let registers = cpu.gpr;
        let arguments = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        assert!(install_powerpc_call_arguments(&mut cpu, &mut memory, &arguments).is_none());
        assert_eq!(cpu.gpr, registers, "failure {failure}");
        assert_eq!((cpu.pc, cpu.lr), (0x1234, 0x5678));
        for offset in 0..32 {
            assert_eq!(memory.read_u8(0x8018 + offset), Some(0xa5));
        }

        // Retry across independently mapped words, including a spilled argument.
        let mut memory = PpcSectionMem::new();
        for slot in 0..9 {
            memory.add_region(0x8018 + slot * 4, vec![0xa5; 4]);
        }
        cpu.gpr[1] = 0x8000;
        assert!(install_powerpc_call_arguments(&mut cpu, &mut memory, &arguments).is_some());
        assert_eq!(&cpu.gpr[3..11], &arguments[..8]);
        for (slot, value) in arguments.iter().enumerate() {
            assert_eq!(memory.read_u32_be(0x8018 + slot as u32 * 4), Some(*value));
        }
        assert_eq!((cpu.pc, cpu.lr), (0x1234, 0x5678));
    }
}

#[test]
fn hle_import_runner_call_universal_proc_preserves_arguments_on_stack_refusal() {
    for traced in [false, true] {
        let pef = synthetic_pef_with_import(b"CallUniversalProc");
        let mut loaded = load_pef_application(&pef).unwrap();
        let descriptor = PPC_HEAP_BASE + 0x1000;
        let callback_rtoc = PPC_HEAP_BASE + 0x3000;
        let proc_info = test_stack_proc_info(PPC_PROCINFO_SIZE_FOUR, &[PPC_PROCINFO_SIZE_FOUR]);
        install_test_powerpc_callback(
            &mut loaded,
            descriptor,
            descriptor + 0x80,
            PPC_HEAP_BASE + 0x2000,
            callback_rtoc,
            proc_info,
            &[d_form_u(36, 3, 2, 0), BLR],
        );
        let start_pc = loaded.cpu.pc;
        let sp = loaded.cpu.gpr[1];
        loaded.memory.write_bytes(sp + 24, &[0xa5; 32]).unwrap();
        loaded.memory.add_readonly_region(sp + 52, vec![0xa5; 4]);
        loaded.cpu.gpr[3] = descriptor;
        loaded.cpu.gpr[4] = proc_info;
        loaded.cpu.gpr[5] = 0x1234_5678;
        let registers = loaded.cpu.gpr[3..11].to_vec();
        let calls = loaded.guest_calls().clone();
        let probe = loaded.run_with_hle_imports_with_trace(64, traced, false, None, None);
        assert_eq!(probe.unsupported_import_index, Some(0));
        assert_eq!(&loaded.cpu.gpr[3..11], registers.as_slice());
        assert_eq!(loaded.guest_calls(), &calls);
        for offset in 0..32 {
            assert_eq!(loaded.memory.read_u8(sp + 24 + offset), Some(0xa5));
        }
        assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0));

        // The same import and arguments can run after moving to a writable frame.
        loaded.cpu.pc = start_pc;
        loaded.cpu.gpr[1] = PPC_HEAP_BASE + 0x4000;
        let probe = loaded.run_with_hle_imports_with_trace(64, traced, false, None, None);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
        assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x1234_5678));
        assert!(loaded.guest_calls().is_empty());
    }
}

#[test]
fn hle_import_runner_call_universal_proc_clears_minimum_parameter_area_slots() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_stack_proc_info(PPC_PROCINFO_SIZE_FOUR, &[PPC_PROCINFO_SIZE_FOUR]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(32, 6, 1, PPC_PARAMETER_AREA_OFFSET as u16),
            d_form_u(32, 7, 1, (PPC_PARAMETER_AREA_OFFSET + 7 * 4) as u16),
            d_form_u(36, 6, 2, 8),
            d_form_u(36, 7, 2, 12),
            BLR,
        ],
    );

    let sp = loaded.cpu.gpr[1];
    for slot in 0..PPC_NATIVE_PARAMETER_GPR_COUNT {
        let addr = ppc_parameter_area_slot_addr(sp, slot).unwrap();
        loaded
            .memory
            .write_u32_be(addr, 0xdead_0000 | u32::try_from(slot).unwrap())
            .unwrap();
    }
    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 16,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 4), Some(0));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 8),
        Some(0x1234_5678)
    );
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 12), Some(0));
}

#[test]
fn hle_import_runner_call_universal_proc_reads_overflow_varargs() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_stack_proc_info(PPC_PROCINFO_SIZE_FOUR, &[PPC_PROCINFO_SIZE_FOUR; 9]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 10, 2, 4),
            d_form_u(
                32,
                11,
                1,
                (PPC_PARAMETER_AREA_OFFSET + PPC_NATIVE_PARAMETER_GPR_COUNT as u32 * 4) as u16,
            ),
            d_form_u(36, 11, 2, 8),
            BLR,
        ],
    );

    loaded.cpu.gpr[1] -= PPC_INITIAL_STACK_FRAME_SIZE;
    let sp = loaded.cpu.gpr[1];
    for (index, value) in [0x101, 0x102, 0x103, 0x104, 0x105, 0x106]
        .iter()
        .copied()
        .enumerate()
    {
        loaded.cpu.gpr[5 + index] = value;
    }
    for (vararg_index, value) in [(6, 0x107), (7, 0x108), (8, 0x109)] {
        let source_slot = PPC_CALL_UNIVERSAL_PROC_FIXED_WORD_PARAMETERS + vararg_index;
        let addr = ppc_parameter_area_slot_addr(sp, source_slot).unwrap();
        loaded.memory.write_u32_be(addr, value).unwrap();
    }
    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 14,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x101);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x101));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 4), Some(0x108));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 8), Some(0x109));
    assert_eq!(
        loaded.memory.read_u32_be(
            ppc_parameter_area_slot_addr(sp, PPC_NATIVE_PARAMETER_GPR_COUNT).unwrap()
        ),
        Some(0x109)
    );
}

#[test]
fn hle_import_runner_call_universal_proc_decodes_register_procinfo() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_register_proc_info(
        PPC_PROCINFO_SIZE_FOUR,
        &[
            (1, PPC_PROCINFO_SIZE_TWO),
            (4, PPC_PROCINFO_SIZE_FOUR),
            (0, PPC_PROCINFO_SIZE_ONE),
        ],
    );
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(36, 5, 2, 8),
            BLR,
        ],
    );

    let sp = loaded.cpu.gpr[1];
    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0xabcd_7654;
    loaded.cpu.gpr[6] = 0xcafe_babe;
    loaded.cpu.gpr[7] = 0x1234_56ee;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 13,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x7654);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x7654));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 4),
        Some(0xcafe_babe)
    );
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 8), Some(0xee));
    assert_eq!(
        loaded
            .memory
            .read_u32_be(ppc_parameter_area_slot_addr(sp, 0).unwrap()),
        Some(0x7654)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(ppc_parameter_area_slot_addr(sp, 1).unwrap()),
        Some(0xcafe_babe)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(ppc_parameter_area_slot_addr(sp, 2).unwrap()),
        Some(0xee)
    );
}

#[test]
fn hle_import_runner_call_universal_proc_returns_register_procinfo_ccr_z_bit() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_register_proc_info_with_result_location(
        PPC_PROCINFO_SIZE_NONE,
        PPC_PROCINFO_REGISTER_CCR_Z,
        &[],
    );
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[d_form_u(28, 3, 3, 0), BLR],
    );

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 11,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 1);
}

#[test]
fn hle_import_runner_call_universal_proc_decodes_special_case_eol_hook() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = PPC_PROCINFO_SPECIAL_CASE
        | (crate::mixed_mode::special_case::EOL_HOOK << PPC_PROCINFO_RESULT_SIZE_PHASE);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(36, 5, 2, 8),
            d_form_u(32, 6, 1, PPC_PARAMETER_AREA_OFFSET as u16),
            d_form_u(32, 7, 1, (PPC_PARAMETER_AREA_OFFSET + 7 * 4) as u16),
            d_form_u(36, 6, 2, 12),
            d_form_u(36, 7, 2, 16),
            d_form_u(14, 3, 0, 0x0101),
            BLR,
        ],
    );

    let sp = loaded.cpu.gpr[1];
    for slot in 0..PPC_NATIVE_PARAMETER_GPR_COUNT {
        let addr = ppc_parameter_area_slot_addr(sp, slot).unwrap();
        loaded
            .memory
            .write_u32_be(addr, 0xfeed_0000 | u32::try_from(slot).unwrap())
            .unwrap();
    }
    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x1111_0001;
    loaded.cpu.gpr[6] = 0x2222_0002;
    loaded.cpu.gpr[7] = 0x3333_0003;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x1111_0001));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 4),
        Some(0x2222_0002)
    );
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 8),
        Some(0x3333_0003)
    );
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 12),
        Some(0x1111_0001)
    );
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 16), Some(0));
}

#[test]
fn hle_import_runner_call_universal_proc_passes_all_hit_test_arguments() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = PPC_PROCINFO_SPECIAL_CASE
        | (crate::mixed_mode::special_case::HIT_TEST_HOOK << PPC_PROCINFO_RESULT_SIZE_PHASE);
    let mut callback = Vec::new();
    for register in 3..=10 {
        callback.push(d_form_u(36, register, 2, u16::from(register - 3) * 4));
    }
    callback.extend([
        d_form_u(32, 11, 1, (PPC_PARAMETER_AREA_OFFSET + 8 * 4) as u16),
        d_form_u(36, 11, 2, 32),
        d_form_u(14, 3, 0, 0x0101),
        BLR,
    ]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &callback,
    );

    let arguments: Vec<u32> = (0..9).map(|index| 0x1000 + index).collect();
    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5..=10].copy_from_slice(&arguments[..6]);
    for (index, value) in arguments[6..].iter().copied().enumerate() {
        loaded
            .memory
            .write_u32_be(
                ppc_parameter_area_slot_addr(loaded.cpu.gpr[1], 8 + index).unwrap(),
                value,
            )
            .unwrap();
    }

    let probe = loaded.run_with_hle_imports(128);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 1);
    for (index, expected) in arguments.into_iter().enumerate() {
        assert_eq!(
            loaded
                .memory
                .read_u32_be(callback_rtoc + u32::try_from(index).unwrap() * 4),
            Some(expected)
        );
    }
}

#[test]
fn hle_import_runner_call_universal_proc_decodes_special_case_mbar_hook() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let menu_rect_ptr = PPC_DATA_BASE + 0x4000;
    let proc_info = PPC_PROCINFO_SPECIAL_CASE
        | (crate::mixed_mode::special_case::MBAR_HOOK << PPC_PROCINFO_RESULT_SIZE_PHASE);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(32, 4, 1, PPC_PARAMETER_AREA_OFFSET as u16),
            d_form_u(36, 4, 2, 4),
            d_form_u(14, 3, 0, 1),
            BLR,
        ],
    );

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = menu_rect_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc),
        Some(menu_rect_ptr)
    );
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 4),
        Some(menu_rect_ptr)
    );
}

#[test]
fn hle_import_runner_call_universal_proc_passes_gne_filter_output_pointer() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let event = PPC_DATA_BASE + 0x4000;
    let result = callback_rtoc + 0x100;
    let proc_info = PPC_PROCINFO_SPECIAL_CASE
        | (crate::mixed_mode::special_case::GNE_FILTER_PROC << PPC_PROCINFO_RESULT_SIZE_PHASE);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(14, 5, 0, 1),
            d_form_u(38, 5, 4, 0),
            BLR,
        ],
    );

    loaded.memory.write_u8(result, 0).unwrap();
    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = event;
    loaded.cpu.gpr[6] = result;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(event));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 4), Some(result));
    assert_eq!(loaded.memory.read_u8(result), Some(1));
}

#[test]
fn hle_import_runner_call_universal_proc_decodes_d0_dispatched_procinfo() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_dispatched_stack_proc_info(
        PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED,
        PPC_PROCINFO_SIZE_FOUR,
        PPC_PROCINFO_SIZE_ONE,
        &[PPC_PROCINFO_SIZE_TWO, PPC_PROCINFO_SIZE_FOUR],
    );
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(36, 5, 2, 8),
            d_form_u(32, 6, 1, PPC_PARAMETER_AREA_OFFSET as u16),
            d_form_u(32, 7, 1, (PPC_PARAMETER_AREA_OFFSET + 4) as u16),
            d_form_u(32, 8, 1, (PPC_PARAMETER_AREA_OFFSET + 8) as u16),
            d_form_u(36, 6, 2, 12),
            d_form_u(36, 7, 2, 16),
            d_form_u(36, 8, 2, 20),
            BLR,
        ],
    );

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x1234_5678;
    loaded.cpu.gpr[6] = 0x8765_4321;
    loaded.cpu.gpr[7] = 0xcafe_babe;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 19,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x78);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x78));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 4), Some(0x4321));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 8),
        Some(0xcafe_babe)
    );
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 12), Some(0x78));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 16), Some(0x4321));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 20),
        Some(0xcafe_babe)
    );
}

#[test]
fn hle_import_runner_call_universal_proc_decodes_stack_dispatched_overflow_varargs() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_dispatched_stack_proc_info(
        PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED,
        PPC_PROCINFO_SIZE_TWO,
        PPC_PROCINFO_SIZE_TWO,
        &[PPC_PROCINFO_SIZE_FOUR; 8],
    );
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 10, 2, 4),
            d_form_u(32, 11, 1, (PPC_PARAMETER_AREA_OFFSET + 8 * 4) as u16),
            d_form_u(36, 11, 2, 8),
            d_form_u(15, 3, 0, 0xface),
            d_form_u(24, 3, 3, 0xb00c),
            BLR,
        ],
    );

    loaded.cpu.gpr[1] -= PPC_INITIAL_STACK_FRAME_SIZE;
    let sp = loaded.cpu.gpr[1];
    for (index, value) in [
        0x1000_2222,
        0x2000_0001,
        0x2000_0002,
        0x2000_0003,
        0x2000_0004,
        0x2000_0005,
    ]
    .iter()
    .copied()
    .enumerate()
    {
        loaded.cpu.gpr[5 + index] = value;
    }
    for (vararg_index, value) in [(6, 0x2000_0006), (7, 0x2000_0007), (8, 0x2000_0008)] {
        let source_slot = PPC_CALL_UNIVERSAL_PROC_FIXED_WORD_PARAMETERS + vararg_index;
        let addr = ppc_parameter_area_slot_addr(sp, source_slot).unwrap();
        loaded.memory.write_u32_be(addr, value).unwrap();
    }
    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 16,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0xb00c);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x2222));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 4),
        Some(0x2000_0007)
    );
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 8),
        Some(0x2000_0008)
    );
    assert_eq!(
        loaded
            .memory
            .read_u32_be(ppc_parameter_area_slot_addr(sp, 8).unwrap()),
        Some(0x2000_0008)
    );
}

#[test]
fn hle_import_runner_call_universal_proc_selects_dispatched_routine_record() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let first_tvector = descriptor + 0x80;
    let second_tvector = descriptor + 0x88;
    let first_entry = PPC_HEAP_BASE + 0x2000;
    let second_entry = PPC_HEAP_BASE + 0x2100;
    let first_rtoc = PPC_HEAP_BASE + 0x3000;
    let second_rtoc = PPC_HEAP_BASE + 0x3100;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_dispatched_stack_proc_info(
        PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED,
        PPC_PROCINFO_SIZE_FOUR,
        PPC_PROCINFO_SIZE_ONE,
        &[PPC_PROCINFO_SIZE_FOUR],
    );
    let first_record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    let second_record = first_record + PPC_ROUTINE_RECORD_SIZE;
    loaded.memory.add_region(descriptor, vec![0; 0x100]);
    loaded.memory.add_region(first_rtoc, vec![0; 0x100]);
    loaded.memory.add_region(second_rtoc, vec![0; 0x100]);
    loaded
        .memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    loaded
        .memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    loaded.memory.write_u16_be(descriptor + 10, 1).unwrap();
    ppc_write_routine_record(
        &mut loaded.memory,
        first_record,
        proc_info,
        PPC_ROUTINE_RECORD_POWERPC_ISA,
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        first_tvector,
    );
    ppc_write_routine_record(
        &mut loaded.memory,
        second_record,
        proc_info,
        PPC_ROUTINE_RECORD_POWERPC_ISA,
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        second_tvector,
    );
    loaded
        .memory
        .write_u32_be(first_record + PPC_ROUTINE_RECORD_SELECTOR_OFFSET, 0x11)
        .unwrap();
    loaded
        .memory
        .write_u32_be(second_record + PPC_ROUTINE_RECORD_SELECTOR_OFFSET, 0x22)
        .unwrap();
    loaded
        .memory
        .write_u32_be(first_tvector, first_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(first_tvector + 4, first_rtoc)
        .unwrap();
    loaded
        .memory
        .write_u32_be(second_tvector, second_entry)
        .unwrap();
    loaded
        .memory
        .write_u32_be(second_tvector + 4, second_rtoc)
        .unwrap();
    let mut first_callback = Vec::new();
    for word in [
        d_form_u(36, 3, 2, 0),
        d_form_u(36, 4, 2, 4),
        d_form_u(15, 3, 0, 0x1111),
        d_form_u(24, 3, 3, 0x0011),
        BLR,
    ] {
        first_callback.extend_from_slice(&word.to_be_bytes());
    }
    let mut second_callback = Vec::new();
    for word in [
        d_form_u(36, 3, 2, 0),
        d_form_u(36, 4, 2, 4),
        d_form_u(15, 3, 0, 0x2222),
        d_form_u(24, 3, 3, 0x0022),
        BLR,
    ] {
        second_callback.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(first_entry, first_callback);
    loaded.memory.add_region(second_entry, second_callback);

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x22;
    loaded.cpu.gpr[6] = 0xcafe_babe;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 14,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x2222_0022);
    assert_eq!(loaded.memory.read_u32_be(first_rtoc), Some(0));
    assert_eq!(loaded.memory.read_u32_be(second_rtoc), Some(0x22));
    assert_eq!(
        loaded.memory.read_u32_be(second_rtoc + 4),
        Some(0xcafe_babe)
    );
}

#[test]
fn hle_import_runner_call_universal_proc_honors_dont_pass_selector() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_dispatched_stack_proc_info(
        PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED,
        PPC_PROCINFO_SIZE_FOUR,
        PPC_PROCINFO_SIZE_ONE,
        &[PPC_PROCINFO_SIZE_TWO, PPC_PROCINFO_SIZE_FOUR],
    );
    let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    loaded.memory.add_region(descriptor, vec![0; 0x100]);
    loaded.memory.add_region(callback_rtoc, vec![0; 0x100]);
    loaded
        .memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    loaded
        .memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    loaded.memory.write_u16_be(descriptor + 10, 0).unwrap();
    ppc_write_routine_record(
        &mut loaded.memory,
        record,
        proc_info,
        PPC_ROUTINE_RECORD_POWERPC_ISA,
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA | PPC_ROUTINE_FLAG_DONT_PASS_SELECTOR,
        tvector,
    );
    loaded
        .memory
        .write_u32_be(record + PPC_ROUTINE_RECORD_SELECTOR_OFFSET, 0x33)
        .unwrap();
    loaded.memory.write_u32_be(tvector, callback_entry).unwrap();
    loaded
        .memory
        .write_u32_be(tvector + 4, callback_rtoc)
        .unwrap();
    let mut callback = Vec::new();
    for word in [
        d_form_u(36, 3, 2, 0),
        d_form_u(36, 4, 2, 4),
        d_form_u(36, 5, 2, 8),
        BLR,
    ] {
        callback.extend_from_slice(&word.to_be_bytes());
    }
    loaded.memory.add_region(callback_entry, callback);

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x33;
    loaded.cpu.gpr[6] = 0xabcd_7654;
    loaded.cpu.gpr[7] = 0xcafe_babe;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 13,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x7654);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0x7654));
    assert_eq!(
        loaded.memory.read_u32_be(callback_rtoc + 4),
        Some(0xcafe_babe)
    );
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 8), Some(0));
}

#[test]
fn hle_import_runner_call_universal_proc_masks_procinfo_result_size() {
    let pef = synthetic_pef_with_import(b"CallUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_stack_proc_info(PPC_PROCINFO_SIZE_TWO, &[]);
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[d_form_u(15, 3, 0, 0xcafe), d_form_u(24, 3, 3, 0xbeef), BLR],
    );

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 12,
        }
    );
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0xbeef);
}

#[test]
fn hle_import_runner_call_ostrap_universal_proc_enters_native_ppc_descriptor() {
    let pef = synthetic_pef_with_import(b"CallOSTrapUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_rtoc = PPC_HEAP_BASE + 0x3000;
    let caller_rtoc = PPC_DATA_BASE;
    let proc_info = test_register_proc_info(
        PPC_PROCINFO_SIZE_FOUR,
        &[(1, PPC_PROCINFO_SIZE_TWO), (0, PPC_PROCINFO_SIZE_FOUR)],
    );
    install_test_powerpc_callback(
        &mut loaded,
        descriptor,
        tvector,
        callback_entry,
        callback_rtoc,
        proc_info,
        &[
            d_form_u(36, 3, 2, 0),
            d_form_u(36, 4, 2, 4),
            d_form_u(14, 3, 3, 5),
            BLR,
        ],
    );

    loaded.cpu.gpr[2] = caller_rtoc;
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0xffff_a11c;
    loaded.cpu.gpr[6] = 0x20;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(matches!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));
    assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0xa121);
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc), Some(0xa11c));
    assert_eq!(loaded.memory.read_u32_be(callback_rtoc + 4), Some(0x20));
}

#[test]
fn hle_import_runner_call_ostrap_universal_proc_enters_m68k_register_descriptor() {
    let pef = synthetic_pef_with_import(b"CallOSTrapUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let descriptor = PPC_HEAP_BASE + 0x1000;
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let proc_info = test_register_proc_info_with_result_location(
        PPC_PROCINFO_SIZE_TWO,
        0,
        &[(1, PPC_PROCINFO_SIZE_TWO)],
    );
    install_test_m68k_callback(
        &mut loaded,
        descriptor,
        callback_entry,
        proc_info,
        0,
        &[
            0x2001, // MOVE.L D1,D0
            0x5a80, // ADDQ.L #5,D0
            0x4e75, // RTS
        ],
    );
    loaded.cpu.gpr[3] = descriptor;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0xffff_a11c;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let pending = loaded.guest_calls().activate_m68k().unwrap();
    assert_eq!(pending.registers.data[1], 0xa11c);
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0xa121);
}

#[test]
fn hle_import_runner_call_ostrap_universal_proc_enters_raw_m68k_trap_code() {
    let pef = synthetic_pef_with_import(b"CallOSTrapUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let callback_entry = PPC_HEAP_BASE + 0x2000;
    let callback_words = [
        0x2001u16, // MOVE.L D1,D0
        0x5a80,    // ADDQ.L #5,D0
        0x4e75,    // RTS
    ];
    loaded.memory.add_region(
        callback_entry,
        callback_words
            .into_iter()
            .flat_map(u16::to_be_bytes)
            .collect(),
    );
    loaded.set_heap_cursor(loaded.heap_cursor().max(callback_entry + 6));
    let proc_info = test_register_proc_info_with_result_location(
        PPC_PROCINFO_SIZE_TWO,
        0,
        &[(1, PPC_PROCINFO_SIZE_TWO), (0, PPC_PROCINFO_SIZE_FOUR)],
    );
    loaded.cpu.gpr[3] = callback_entry;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0xffff_a11e;
    loaded.cpu.gpr[6] = 0x20;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let pending = loaded.guest_calls().activate_m68k().unwrap();
    assert_eq!(pending.entry, callback_entry);
    assert_eq!(pending.registers.data[1], 0xa11e);
    assert_eq!(pending.registers.data[0], 0x20);
    drain_test_m68k_guest_calls(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0xa123);
}

#[test]
fn hle_import_runner_call_ostrap_universal_proc_rejects_stack_procinfo() {
    let pef = synthetic_pef_with_import(b"CallOSTrapUniversalProc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let proc_info = test_stack_proc_info(PPC_PROCINFO_SIZE_FOUR, &[PPC_PROCINFO_SIZE_FOUR]);

    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x1000;
    loaded.cpu.gpr[4] = proc_info;
    loaded.cpu.gpr[5] = 0x20;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 0);
    assert_eq!(probe.unsupported_import_index, Some(0));
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_IMPORT_TRAP_BASE,
            cycles: 4,
        }
    );
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE + 0x1000);
}

#[test]
fn routine_descriptor_resolver_uses_powerpc_record_in_fat_descriptor() {
    let descriptor = PPC_DATA_BASE;
    let m68k_record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    let powerpc_record = m68k_record + PPC_ROUTINE_RECORD_SIZE;
    let tvector = descriptor + 0x80;
    let callback_entry = PPC_CODE_BASE + 0x40;
    let callback_rtoc = PPC_DATA_BASE + 0x200;
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_DATA_BASE, vec![0; 0x100]);
    memory.add_region(callback_entry, 0x4e80_0020u32.to_be_bytes().to_vec());

    memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    memory.write_u16_be(descriptor + 10, 1).unwrap();
    memory
        .write_u8(
            m68k_record + PPC_ROUTINE_RECORD_ISA_OFFSET,
            PPC_ROUTINE_RECORD_M68K_ISA,
        )
        .unwrap();
    memory
        .write_u8(
            powerpc_record + PPC_ROUTINE_RECORD_ISA_OFFSET,
            PPC_ROUTINE_RECORD_POWERPC_ISA,
        )
        .unwrap();
    memory
        .write_u16_be(
            powerpc_record + PPC_ROUTINE_RECORD_FLAGS_OFFSET,
            PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        )
        .unwrap();
    memory
        .write_u32_be(
            powerpc_record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            tvector,
        )
        .unwrap();
    memory.write_u32_be(tvector, callback_entry).unwrap();
    memory.write_u32_be(tvector + 4, callback_rtoc).unwrap();

    let target =
        ppc_resolve_callback_target(&mut memory, descriptor, PPC_DATA_BASE, None).unwrap();

    assert_eq!(
        target,
        PpcCallbackTarget {
            entry: callback_entry,
            rtoc: callback_rtoc,
            proc_info: 0,
            routine_flags: PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        }
    );
}

#[test]
fn routine_descriptor_resolver_handles_relative_powerpc_proc_descriptor() {
    let descriptor = PPC_DATA_BASE;
    let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    let callback_offset = 0x80;
    let callback_entry = descriptor + callback_offset;
    let default_rtoc = PPC_DATA_BASE + 0x200;
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_DATA_BASE, vec![0; 0x100]);

    memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .unwrap();
    memory
        .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    memory.write_u16_be(descriptor + 10, 0).unwrap();
    memory
        .write_u8(
            record + PPC_ROUTINE_RECORD_ISA_OFFSET,
            PPC_ROUTINE_RECORD_POWERPC_ISA,
        )
        .unwrap();
    memory
        .write_u16_be(
            record + PPC_ROUTINE_RECORD_FLAGS_OFFSET,
            PPC_ROUTINE_FLAG_USE_NATIVE_ISA | PPC_ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE,
        )
        .unwrap();
    memory
        .write_u32_be(
            record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            callback_offset,
        )
        .unwrap();
    memory.write_u32_be(callback_entry, 0x4e80_0020).unwrap();

    let target =
        ppc_resolve_callback_target(&mut memory, descriptor, default_rtoc, None).unwrap();

    assert_eq!(
        target,
        PpcCallbackTarget {
            entry: callback_entry,
            rtoc: default_rtoc,
            proc_info: 0,
            routine_flags: PPC_ROUTINE_FLAG_USE_NATIVE_ISA
                | PPC_ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE,
        }
    );
}

#[test]
fn native_universal_proc_prepares_resources_runs_initializers_and_retries_failures() {
    for initialization in [None, Some(0u16), Some(1u16)] {
        let mut loader =
            synthetic_loader_with_chunks(b"InterfaceLib", b"TestImport", &[run_reloc(0x23, 2)]);
        if initialization.is_some() {
            write_i32(&mut loader, 8, 1);
            write_u32(&mut loader, 12, 8);
        }
        let mut data = vec![0; 16];
        write_u32(&mut data, 8, 12);
        let mut fragment = synthetic_pef_with_loader_and_data(loader, &data);
        let code_offset = parse_pef_sections(&fragment).unwrap()[0].container_offset as usize;
        for (index, word) in [
            d_form_u(32, 11, 1, 56), // ninth argument
            0x7c63_5a14,             // add r3,r3,r11
            BLR,
            d_form_u(14, 3, 0, initialization.unwrap_or(0)),
            d_form_u(14, 4, 0, 0xBAD), // initializer clobbers argument registers
            BLR,
        ]
        .into_iter()
        .enumerate()
        {
            write_u32(&mut fragment, code_offset + index * 4, word);
        }
        let mut loaded =
            load_pef_application(&synthetic_pef_with_import(b"CallUniversalProc")).unwrap();
        let descriptor = PPC_HEAP_BASE + 0x1000;
        let fragment_address = descriptor + 0x100;
        loaded
            .memory
            .add_region(descriptor, vec![0; 0x100 + fragment.len()]);
        loaded
            .memory
            .write_bytes(fragment_address, &fragment)
            .unwrap();
        assert!(crate::cfm::fragment::read_resource_fragment(
            &mut loaded.memory,
            fragment_address,
            Some(fragment.len() as u32 - 1)
        )
        .is_none());
        assert_eq!(
            crate::cfm::fragment::read_resource_fragment(
                &mut loaded.memory,
                fragment_address,
                Some(fragment.len() as u32)
            ),
            Some(fragment.clone())
        );
        loaded.set_heap_cursor(descriptor + 0x100 + fragment.len() as u32 + 0x100);
        loaded
            .memory
            .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
            .unwrap();
        loaded
            .memory
            .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
            .unwrap();
        loaded.memory.write_u16_be(descriptor + 10, 0).unwrap();
        let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
        let proc_info =
            test_stack_proc_info(PPC_PROCINFO_SIZE_FOUR, &[PPC_PROCINFO_SIZE_FOUR; 9]);
        assert!(ppc_write_routine_record(
            &mut loaded.memory,
            record,
            proc_info,
            PPC_ROUTINE_RECORD_POWERPC_ISA,
            3,
            0x100
        ));
        loaded.cpu.gpr[1] -= PPC_INITIAL_STACK_FRAME_SIZE;
        let sp = loaded.cpu.gpr[1];
        let caller_rtoc = loaded.cpu.gpr[2];
        let first_id = loaded.cfm.as_ref().unwrap().next_connection_id;
        let prepare_call = |loaded: &mut PpcLoadedApp| {
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[2] = caller_rtoc;
            loaded.cpu.gpr[3] = descriptor;
            loaded.cpu.gpr[4] = proc_info;
            for index in 0..9 {
                let value = (index as u32 + 1) * 0x10;
                if index < 6 {
                    loaded.cpu.gpr[5 + index] = value;
                } else {
                    let slot = ppc_parameter_area_slot_addr(
                        sp,
                        PPC_CALL_UNIVERSAL_PROC_FIXED_WORD_PARAMETERS + index,
                    )
                    .unwrap();
                    loaded.memory.write_u32_be(slot, value).unwrap();
                }
            }
        };
        prepare_call(&mut loaded);
        if initialization.is_some() {
            for _ in 0..16 {
                if loaded.guest_calls().is_resource_preparation_pending(record) {
                    break;
                }
                let slice = loaded.run_with_hle_imports(1);
                assert_eq!(slice.unsupported_import_index, None);
            }
            assert!(loaded.guest_calls().is_resource_preparation_pending(record));
            assert_eq!(loaded.memory.read_u16_be(record + 6), Some(3));
            let request = match crate::guest_procedure::inspect_guest_procedure(
                &mut loaded.memory,
                descriptor,
                caller_rtoc,
                None,
                GuestIsa::PowerPc,
                GuestIsa::PowerPc,
            )
            .unwrap()
            {
                crate::guest_procedure::GuestProcedureResolution::Prepare(request) => request,
                _ => panic!("initializing resource became callable"),
            };
            let mut recursive_cpu = loaded.cpu.clone();
            let before_cpu = recursive_cpu.clone();
            let mut cursor = loaded.heap_cursor();
            let mut import_run_state = PpcImportRunState::from_parts(
                std::mem::take(&mut loaded.imports),
                loaded.import_count,
                ppc_import_layout(),
            );
            let guest_calls = loaded.guest_calls().shared_handle();
            let mut manager = loaded.process_memory_manager.0.borrow_mut();
            let limit = manager.native_heap_state().unwrap().heap_limit;
            let cfm = loaded.cfm.as_mut().unwrap();
            assert_eq!(
                ppc_prepare_resource_call(
                    &mut recursive_cpu,
                    &mut loaded.memory,
                    &mut manager,
                    &guest_calls,
                    &mut cursor,
                    limit,
                    &mut cfm.connections,
                    &mut cfm.next_connection_id,
                    &mut import_run_state,
                    request,
                    None
                ),
                PpcImportAction::Return(ppc_i16_result(PPC_FRAG_INIT_LOOP))
            );
            (loaded.imports, loaded.import_count) = import_run_state.into_parts();
            assert_eq!(recursive_cpu.gpr, before_cpu.gpr);
            assert_eq!(recursive_cpu.pc, before_cpu.pc);
            assert!(loaded.guest_calls().is_resource_preparation_pending(record));
        }
        let result = loaded.run_with_hle_imports(128);
        assert_eq!(result.unsupported_import_index, None);
        assert!(matches!(
            result.result,
            PpcRunResult::Halted {
                pc: PPC_HALT_PC,
                ..
            }
        ));
        assert_eq!(loaded.cpu.gpr[2], caller_rtoc);
        assert_eq!(loaded.cpu.gpr[1], sp);
        assert!(loaded.guest_calls().is_empty());
        if initialization == Some(1) {
            assert_eq!(
                loaded.cpu.gpr[3],
                ppc_i16_result(PPC_FRAG_USER_INIT_PROC_ERR)
            );
            assert_eq!(loaded.memory.read_u16_be(record + 6), Some(3));
            assert_eq!(loaded.memory.read_u32_be(record + 8), Some(0x100));
            assert!(!loaded
                .cfm
                .as_ref()
                .unwrap()
                .connections
                .iter()
                .any(|connection| connection.id == first_id));
            loaded
                .memory
                .write_u32_be(
                    fragment_address + code_offset as u32 + 12,
                    d_form_u(14, 3, 0, 0),
                )
                .unwrap();
            prepare_call(&mut loaded);
            let retry = loaded.run_with_hle_imports(128);
            assert_eq!(retry.unsupported_import_index, None);
            assert!(loaded.cfm.as_ref().unwrap().next_connection_id > first_id + 1);
        }
        assert_eq!(loaded.cpu.gpr[3], 0xA0);
        assert_eq!(loaded.memory.read_u16_be(record + 6), Some(0));
        let prepared_target = loaded.memory.read_u32_be(record + 8).unwrap();
        assert_ne!(prepared_target, 0x100);
        let cursor = loaded.heap_cursor();
        let next = loaded.cfm.as_ref().unwrap().next_connection_id;
        prepare_call(&mut loaded);
        let again = loaded.run_with_hle_imports(128);
        assert_eq!(again.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0xA0);
        assert_eq!(loaded.cfm.as_ref().unwrap().next_connection_id, next);
        assert_eq!(loaded.heap_cursor(), cursor);
    }
}
