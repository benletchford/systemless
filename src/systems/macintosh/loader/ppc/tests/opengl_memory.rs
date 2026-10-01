use super::*;

#[test]
fn glm_set_mode_accepts_documented_modes_and_rejects_unknown_mode() {
    for mode in 1..=4 {
        let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmSetMode");
        let mut loaded = load_pef_application(&pef).unwrap();
        assert_eq!(
            loaded.imports[0].dispatcher_target,
            PpcImportDispatcherTarget::GlmSetMode
        );
        loaded.cpu.gpr[3] = mode;

        let probe = loaded.run_with_hle_imports(64);

        assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.glm_mode, Some(mode));
        assert_eq!(loaded.glm_error, 0);
        assert_eq!(loaded.cpu.gpr[3], mode); // void function preserves r3
    }

    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmSetMode");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.glm_mode = Some(2);
    loaded.cpu.gpr[3] = 5;
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(loaded.glm_mode, Some(2));
    assert_eq!(loaded.glm_error, 1); // GLM_INVALID_ENUM
}

#[test]
fn glm_get_error_clears_the_pending_error() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmGetError");
    let mut loaded = load_pef_application(&pef).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::GlmGetError
    );
    loaded.glm_error = 1;

    let probe = loaded.run_with_hle_imports(64);

    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 1);
    assert_eq!(loaded.glm_error, 0);
}

#[test]
fn glm_set_func_records_native_callback_and_reports_invalid_arguments() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmSetFunc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let vector = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(vector, vec![0; 8]);
    loaded.memory.write_u32_be(vector, loaded.entry_pc).unwrap();
    loaded.memory.write_u32_be(vector + 4, loaded.rtoc).unwrap();
    assert_eq!(
        loaded.imports[0].dispatcher_target,
        PpcImportDispatcherTarget::GlmSetFunc
    );
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = vector;
    let probe = loaded.run_with_hle_imports(64);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.glm_callbacks[0].unwrap().entry, loaded.entry_pc);
    assert_eq!(loaded.glm_callbacks[0].unwrap().rtoc, loaded.rtoc);
    assert_eq!(loaded.glm_error, 0);

    let mut invalid = load_pef_application(&pef).unwrap();
    invalid.cpu.gpr[3] = 9;
    invalid.cpu.gpr[4] = vector;
    invalid.run_with_hle_imports(64);
    assert_eq!(invalid.glm_error, 1); // GLM_INVALID_ENUM

    let mut bad_pointer = load_pef_application(&pef).unwrap();
    bad_pointer.cpu.gpr[3] = 1;
    bad_pointer.cpu.gpr[4] = 0xdead_beef;
    bad_pointer.run_with_hle_imports(64);
    assert_eq!(bad_pointer.glm_error, 2); // GLM_INVALID_VALUE
}

#[test]
fn glm_override_malloc_invokes_guest_allocator_and_records_result() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmMalloc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let callback = PPC_DATA_BASE + 0x2000;
    let captured_argument = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(captured_argument, vec![0; 4]);
    loaded.memory.add_region(callback, vec![
        0x90, 0x62, 0x00, 0x00, // stw r3, 0(r2)
        0x38, 0x60, 0x12, 0x34, // li r3, 0x1234
        0x4e, 0x80, 0x00, 0x20, // blr
    ]);
    loaded.glm_mode = Some(1);
    loaded.glm_callbacks[0] = Some(PpcCallbackTarget {
        entry: callback,
        rtoc: captured_argument,
        proc_info: 0,
        routine_flags: 0,
    });
    loaded.cpu.gpr[3] = 4096;
    let probe = loaded.run_with_hle_imports(128);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234);
    assert_eq!(loaded.memory.read_u32_be(captured_argument), Some(4096));
    assert_eq!(loaded.glm_allocations.get(&0x1234), Some(&(true, 4096)));
    assert!(loaded.glm_callback_stack.is_empty());
    assert_eq!(loaded.glm_error, 0);
}

#[test]
fn glm_override_free_invokes_guest_callback_before_releasing_record() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmFree");
    let mut loaded = load_pef_application(&pef).unwrap();
    let callback = PPC_DATA_BASE + 0x2000;
    let captured_argument = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(captured_argument, vec![0; 4]);
    loaded.memory.add_region(callback, vec![
        0x90, 0x62, 0x00, 0x00, // stw r3, 0(r2)
        0x38, 0x60, 0x00, 0x00, // li r3, 0
        0x4e, 0x80, 0x00, 0x20, // blr
    ]);
    loaded.glm_mode = Some(1);
    loaded.glm_callbacks[1] = Some(PpcCallbackTarget {
        entry: callback,
        rtoc: captured_argument,
        proc_info: 0,
        routine_flags: 0,
    });
    loaded.glm_allocations.insert(0x1234, (true, 256));
    loaded.cpu.gpr[3] = 0x1234;
    let probe = loaded.run_with_hle_imports(128);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234);
    assert_eq!(loaded.memory.read_u32_be(captured_argument), Some(0x1234));
    assert!(loaded.glm_allocations.is_empty());
    assert!(loaded.glm_callback_stack.is_empty());
    assert_eq!(loaded.glm_error, 0);
}

#[test]
fn glm_malloc_requires_override_callback_and_uses_guest_heap_otherwise() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmMalloc");
    let mut override_without_callback = load_pef_application(&pef).unwrap();
    override_without_callback.glm_mode = Some(1);
    override_without_callback.cpu.gpr[3] = 256;
    override_without_callback.run_with_hle_imports(64);
    assert_eq!(override_without_callback.cpu.gpr[3], 0);
    assert_eq!(override_without_callback.glm_error, 3); // GLM_INVALID_OPERATION
    assert!(override_without_callback.glm_allocations.is_empty());

    let mut system_heap = load_pef_application(&pef).unwrap();
    system_heap.cpu.gpr[3] = 256;
    system_heap.run_with_hle_imports(64);
    let pointer = system_heap.cpu.gpr[3];
    assert_ne!(pointer, 0);
    assert_eq!(system_heap.glm_allocations.get(&pointer), Some(&(false, 256)));
    assert!(system_heap.memory.write_u32_be(pointer, 0x1234_5678).is_some());
    assert_eq!(system_heap.glm_error, 0);
}

#[test]
fn glm_override_calloc_zeroes_guest_callback_storage() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmCalloc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let callback = PPC_DATA_BASE + 0x2000;
    let callback_data = PPC_DATA_BASE + 0x3000;
    let storage = PPC_DATA_BASE + 0x4000;
    loaded.memory.add_region(callback, vec![
        0x90, 0x62, 0x00, 0x00, // stw r3, 0(r2)
        0x80, 0x62, 0x00, 0x04, // lwz r3, 4(r2)
        0x4e, 0x80, 0x00, 0x20, // blr
    ]);
    loaded.memory.add_region(callback_data, vec![0; 8]);
    loaded.memory.write_u32_be(callback_data + 4, storage).unwrap();
    loaded.memory.add_region(storage, vec![0xff; 64]);
    loaded.glm_mode = Some(1);
    loaded.glm_callbacks[0] = Some(PpcCallbackTarget {
        entry: callback,
        rtoc: callback_data,
        proc_info: 0,
        routine_flags: 0,
    });
    loaded.cpu.gpr[3] = 3;
    loaded.cpu.gpr[4] = 4;
    let probe = loaded.run_with_hle_imports(128);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(callback_data), Some(12));
    assert_eq!(loaded.cpu.gpr[3], storage);
    assert!((0..12).all(|offset| loaded.memory.read_u8(storage + offset) == Some(0)));
    assert_eq!(loaded.memory.read_u8(storage + 12), Some(0xff));
    assert_eq!(loaded.glm_error, 0);

    let mut overflow = load_pef_application(&pef).unwrap();
    overflow.glm_mode = Some(1);
    overflow.cpu.gpr[3] = u32::MAX;
    overflow.cpu.gpr[4] = 2;
    overflow.run_with_hle_imports(64);
    assert_eq!(overflow.cpu.gpr[3], 0);
    assert_eq!(overflow.glm_error, 2); // GLM_INVALID_VALUE
}

#[test]
fn glm_override_realloc_copies_before_invoking_guest_free() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmRealloc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let alloc_callback = PPC_DATA_BASE + 0x2000;
    let free_callback = PPC_DATA_BASE + 0x2100;
    let callback_data = PPC_DATA_BASE + 0x3000;
    let old_storage = PPC_DATA_BASE + 0x4000;
    let new_storage = PPC_DATA_BASE + 0x5000;
    loaded.memory.add_region(alloc_callback, vec![
        0x90, 0x62, 0x00, 0x00, // stw r3, 0(r2)
        0x80, 0x62, 0x00, 0x04, // lwz r3, 4(r2)
        0x4e, 0x80, 0x00, 0x20, // blr
    ]);
    loaded.memory.add_region(free_callback, vec![
        0x90, 0x62, 0x00, 0x08, // stw r3, 8(r2)
        0x38, 0x60, 0x00, 0x00, // li r3, 0
        0x4e, 0x80, 0x00, 0x20, // blr
    ]);
    loaded.memory.add_region(callback_data, vec![0; 12]);
    loaded.memory.write_u32_be(callback_data + 4, new_storage).unwrap();
    loaded.memory.add_region(old_storage, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    loaded.memory.add_region(new_storage, vec![0; 16]);
    loaded.glm_mode = Some(1);
    loaded.glm_callbacks[0] = Some(PpcCallbackTarget {
        entry: alloc_callback,
        rtoc: callback_data,
        proc_info: 0,
        routine_flags: 0,
    });
    loaded.glm_callbacks[1] = Some(PpcCallbackTarget {
        entry: free_callback,
        rtoc: callback_data,
        proc_info: 0,
        routine_flags: 0,
    });
    loaded.glm_allocations.insert(old_storage, (true, 8));
    loaded.cpu.gpr[3] = old_storage;
    loaded.cpu.gpr[4] = 16;
    let probe = loaded.run_with_hle_imports(256);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(callback_data), Some(16));
    assert_eq!(loaded.memory.read_u32_be(callback_data + 8), Some(old_storage));
    assert_eq!(loaded.cpu.gpr[3], new_storage);
    assert!((0..8).all(|offset| {
        loaded.memory.read_u8(new_storage + offset) == Some((offset + 1) as u8)
    }));
    assert_eq!(loaded.glm_allocations.get(&new_storage), Some(&(true, 16)));
    assert!(!loaded.glm_allocations.contains_key(&old_storage));
    assert!(loaded.glm_callback_stack.is_empty());
    assert_eq!(loaded.glm_error, 0);
}

#[test]
fn glm_realloc_preserves_native_heap_bytes() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmRealloc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let old_pointer = loaded
        .process_memory_manager
        .0
        .borrow_mut()
        .new_native_ptr(&mut loaded.memory, 8, false);
    assert_ne!(old_pointer, 0);
    loaded.memory.write_bytes(old_pointer, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    loaded.glm_allocations.insert(old_pointer, (false, 8));
    loaded.cpu.gpr[3] = old_pointer;
    loaded.cpu.gpr[4] = 16;
    let probe = loaded.run_with_hle_imports(128);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    let new_pointer = loaded.cpu.gpr[3];
    assert_ne!(new_pointer, 0);
    assert!((0..8).all(|offset| {
        loaded.memory.read_u8(new_pointer + offset) == Some((offset + 1) as u8)
    }));
    assert_eq!(loaded.glm_allocations.get(&new_pointer), Some(&(false, 16)));
    assert!(!loaded.glm_allocations.contains_key(&old_pointer));
    assert_eq!(loaded.glm_error, 0);
}

#[test]
fn glm_page_free_all_invokes_each_registered_guest_free_callback() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmPageFreeAll");
    let mut loaded = load_pef_application(&pef).unwrap();
    let callback = PPC_DATA_BASE + 0x2000;
    let callback_data = PPC_DATA_BASE + 0x3000;
    let first = PPC_DATA_BASE + 0x4000;
    let second = PPC_DATA_BASE + 0x5000;
    loaded.memory.add_region(callback, vec![
        0x80, 0x82, 0x00, 0x00, // lwz r4, 0(r2)
        0x38, 0x84, 0x00, 0x01, // addi r4, r4, 1
        0x90, 0x82, 0x00, 0x00, // stw r4, 0(r2)
        0x90, 0x62, 0x00, 0x04, // stw r3, 4(r2)
        0x4e, 0x80, 0x00, 0x20, // blr
    ]);
    loaded.memory.add_region(callback_data, vec![0; 8]);
    loaded.glm_callbacks[1] = Some(PpcCallbackTarget {
        entry: callback,
        rtoc: callback_data,
        proc_info: 0,
        routine_flags: 0,
    });
    loaded.glm_allocations.insert(first, (true, 64));
    loaded.glm_allocations.insert(second, (true, 64));
    let probe = loaded.run_with_hle_imports(512);
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(callback_data), Some(2));
    assert_eq!(loaded.memory.read_u32_be(callback_data + 4), Some(second));
    assert!(loaded.glm_allocations.is_empty());
    assert!(loaded.glm_page_free_all_queue.is_empty());
    assert!(loaded.glm_callback_stack.is_empty());
    assert_eq!(loaded.glm_error, 0);
}

#[test]
fn glm_realloc_null_uses_allocation_callback_without_free_callback() {
    let pef = synthetic_pef_with_library_import(b"OpenGLMemory", b"glmRealloc");
    let mut loaded = load_pef_application(&pef).unwrap();
    let callback = PPC_DATA_BASE + 0x2000;
    loaded.memory.add_region(callback, vec![0x38, 0x60, 0x12, 0x34, 0x4e, 0x80, 0x00, 0x20]);
    loaded.glm_mode = Some(1);
    loaded.glm_callbacks[0] = Some(PpcCallbackTarget {
        entry: callback,
        rtoc: loaded.rtoc,
        proc_info: 0,
        routine_flags: 0,
    });
    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 128;
    loaded.run_with_hle_imports(128);
    assert_eq!(loaded.cpu.gpr[3], 0x1234);
    assert_eq!(loaded.glm_allocations.get(&0x1234), Some(&(true, 128)));
    assert_eq!(loaded.glm_error, 0);
}
