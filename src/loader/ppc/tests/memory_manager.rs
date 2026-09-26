use super::*;

#[test]
fn native_operation_uses_canonical_allocator_without_slice_handoff() {
    let pef = synthetic_pef_with_import(b"NewHandle");
    let mut native = load_pef_application(&pef).unwrap();
    native.cpu.gpr[3] = 32;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::NewHandle { clear: false },
    );
    let handle = native.cpu.gpr[3];
    assert_ne!(handle, 0);

    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let memory_manager = context.memory_manager_handle();
    let expected_cursor = native.heap_cursor() + 16;
    {
        let mut manager = memory_manager.borrow_mut();
        manager.set_state_for_handle(handle, 0xa0);
        let mut canonical = manager.native_allocation(handle).unwrap();
        canonical.size = 24;
        canonical.capacity = 40;
        manager.set_native_allocation(canonical);
        manager.mutate_native_allocator(|allocator| {
            allocator.heap.heap_cursor = expected_cursor;
            allocator.ptrs.push(ProcessPtrRecord {
                ptr: PPC_HEAP_BASE + 4,
                size: 8,
            });
            allocator.free_ptr_blocks.push(ProcessPtrRecord {
                ptr: PPC_HEAP_BASE + 12,
                size: 4,
            });
            allocator.free_handle_blocks.push(ProcessHandleRecord {
                handle: PPC_HEAP_BASE + 16,
                ptr: PPC_HEAP_BASE + 20,
                size: 4,
                capacity: 8,
            });
        });
    }

    native.with_process_memory_manager(
        |_native, memory_manager| {
            assert_eq!(
                memory_manager
                    .native_heap_state()
                    .map(|heap| heap.heap_cursor),
                Some(expected_cursor)
            );
            let allocator = memory_manager.native_allocator().unwrap();
            assert_eq!(allocator.ptrs.last().map(|record| record.size), Some(8));
            assert_eq!(
                allocator.free_ptr_blocks.last().map(|record| record.size),
                Some(4)
            );
            assert_eq!(
                allocator
                    .free_handle_blocks
                    .last()
                    .map(|record| record.capacity),
                Some(8)
            );
            memory_manager.mutate_native_allocator(|allocator| {
                allocator.heap.heap_cursor += 8;
            });
            let allocation = memory_manager
                .native_handle_records()
                .iter()
                .find(|record| record.handle == handle)
                .expect("canonical allocation materialized for native handle");
            assert_eq!((allocation.size, allocation.capacity), (24, 40));
            let mut state = memory_manager.native_handle_state(handle);
            assert!(state.locked);
            assert!(state.no_purge);
            assert!(state.resource);
            state.locked = false;
            state.high_locked = false;
            state.no_purge = false;
            state.resource = false;
            memory_manager.set_native_handle_state(state);
        },
    );

    let manager = memory_manager.borrow();
    assert_eq!(manager.state_for_handle(handle), Some(0x40));
    assert_eq!(
        manager
            .native_allocator()
            .map(|allocator| allocator.heap.heap_cursor),
        Some(expected_cursor + 8)
    );
    assert_eq!(
        manager.native_allocation(handle),
        native
            .handles()
            .iter()
            .copied()
            .find(|record| record.handle == handle)
    );
}

#[test]
fn attached_native_memory_manager_survives_panic_without_runner_handoff() {
    let pef = synthetic_pef_with_import(b"NewPtr");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let memory_manager = context.memory_manager_handle().clone();

    let panic_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        native.with_process_memory_manager(|native, memory_manager| {
            native.cpu.gpr[3] = 24;
            let probe =
                native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(probe.handled_import_count, 1);
            panic!("simulated panic after a native Memory Manager import");
        });
    }));

    assert!(panic_result.is_err());
    let ptr = native.cpu.gpr[3];
    assert_ne!(ptr, 0);
    assert!(native.process_memory_manager.ptr_eq(&memory_manager));
    assert!(memory_manager
        .borrow()
        .native_allocator()
        .unwrap()
        .ptrs
        .iter()
        .any(|record| record.ptr == ptr && record.size == 24));
}

#[test]
fn detached_native_handle_state_remains_independent() {
    let handle = PPC_HEAP_BASE + 0x20;
    let record = ProcessHandleRecord {
        handle,
        ptr: PPC_HEAP_BASE + 0x40,
        size: 16,
        capacity: 16,
    };
    let stale_state = PpcHandleStateRecord {
        handle,
        locked: false,
        high_locked: false,
        no_purge: false,
        resource: false,
    };
    let mut memory_manager = ProcessMemoryManager::default();
    memory_manager.register_native_handle_records([(
        record,
        ppc_process_handle_state_bits(&[stale_state], handle),
    )]);
    let detached = memory_manager.detached_clone();

    memory_manager.lock_process_handle(handle, true);

    let state = memory_manager.native_handle_state(handle);
    assert!(state.locked);
    assert!(state.high_locked);
    assert!(!state.no_purge);
    assert!(!state.resource);
    assert_eq!(detached.state_for_handle(handle), Some(0x40));
    assert!(!detached.native_handle_state(handle).high_locked);
}

#[test]
fn resource_disposal_is_immediately_process_owned() {
    let pef = synthetic_pef_with_import(b"NewHandle");
    let mut native = load_pef_application(&pef).unwrap();
    native.cpu.gpr[3] = 24;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::NewHandle { clear: false },
    );
    let handle = native.cpu.gpr[3];
    let ptr = native.memory.read_u32_be(handle).unwrap();
    let shared_manager = native.process_memory_manager.0.clone();
    let mut memory_manager = shared_manager.borrow_mut();
    let detached = memory_manager.detached_clone();
    let allocator = memory_manager.native_allocator_snapshot().unwrap();
    let mut heap_cursor = allocator.heap.heap_cursor;
    let heap_limit = allocator.heap.heap_limit;
    let mut last_mem_error = allocator.heap.last_mem_error;
    let mut handles = memory_manager.native_handle_records().to_vec();
    let mut resources = [PpcVfsResourceRecord {
        ref_num: 1,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(*b"TEST"),
        res_id: 128,
        name: b"owned".to_vec(),
        data: vec![0; 24],
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle,
    }];
    let mut last_resource_error = PPC_RES_NOT_FOUND_ERR;
    native.cpu.gpr[3] = handle;

    ppc_release_resource(
        &mut native.cpu,
        &mut memory_manager,
        &mut native.memory,
        &mut heap_cursor,
        heap_limit,
        &mut last_mem_error,
        &mut handles,
        &mut resources,
        &mut last_resource_error,
    );

    assert_eq!(last_resource_error, PPC_NO_ERR);
    assert_eq!(resources[0].handle, 0);
    assert_eq!(native.memory.read_u32_be(handle), Some(0));
    assert_eq!(memory_manager.native_allocation(handle), None);
    assert_eq!(memory_manager.recover_handle(ptr), None);
    assert!(memory_manager
        .native_allocator()
        .unwrap()
        .free_handle_blocks
        .iter()
        .any(|record| record.handle == handle));
    assert!(handles.iter().all(|record| record.handle != handle));
    assert_eq!(
        detached.native_allocation(handle).map(|record| record.ptr),
        Some(ptr)
    );
    assert_eq!(detached.recover_handle(ptr), Some(handle));
}

#[test]
fn standalone_and_attached_memory_imports_share_process_manager_semantics() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut standalone = load_pef_application(&pef).unwrap();
    let mut attached = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    attached.attach_unconverted_process_services(&mut context);
    let memory_manager = context.memory_manager_handle().clone();

    standalone.cpu.gpr[3] = 24;
    attached.cpu.gpr[3] = 24;
    run_test_import(
        &mut standalone,
        PpcImportDispatcherTarget::NewPtr { clear: true },
    );
    run_test_import_with_process_memory_manager(
        &mut attached,
        PpcImportDispatcherTarget::NewPtr { clear: true },
        &memory_manager,
    );
    let ptr = standalone.cpu.gpr[3];
    assert_eq!(attached.cpu.gpr[3], ptr);
    assert_eq!(standalone.ptrs(), attached.ptrs());
    assert_eq!(standalone.heap_cursor(), attached.heap_cursor());
    assert_eq!(
        ppc_memory_read_bytes(&mut standalone.memory, ptr, 24),
        ppc_memory_read_bytes(&mut attached.memory, ptr, 24),
    );

    standalone.cpu.gpr[3] = ptr;
    attached.cpu.gpr[3] = ptr;
    run_test_import(&mut standalone, PpcImportDispatcherTarget::GetPtrSize);
    run_test_import_with_process_memory_manager(
        &mut attached,
        PpcImportDispatcherTarget::GetPtrSize,
        &memory_manager,
    );
    assert_eq!(standalone.cpu.gpr[3], 24);
    assert_eq!(attached.cpu.gpr[3], 24);

    standalone.cpu.gpr[3] = 13;
    attached.cpu.gpr[3] = 13;
    run_test_import(
        &mut standalone,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    run_test_import_with_process_memory_manager(
        &mut attached,
        PpcImportDispatcherTarget::NewHandle { clear: true },
        &memory_manager,
    );
    let handle = standalone.cpu.gpr[3];
    assert_eq!(attached.cpu.gpr[3], handle);
    assert_eq!(standalone.handles(), attached.handles());
    assert_eq!(standalone.heap_cursor(), attached.heap_cursor());

    standalone.cpu.gpr[3] = handle;
    attached.cpu.gpr[3] = handle;
    run_test_import(&mut standalone, PpcImportDispatcherTarget::HLock);
    run_test_import_with_process_memory_manager(
        &mut attached,
        PpcImportDispatcherTarget::HLock,
        &memory_manager,
    );
    assert_eq!(standalone.handle_states(), attached.handle_states());
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x80));

    standalone.cpu.gpr[3] = handle;
    standalone.cpu.gpr[4] = 48;
    attached.cpu.gpr[3] = handle;
    attached.cpu.gpr[4] = 48;
    run_test_import(&mut standalone, PpcImportDispatcherTarget::SetHandleSize);
    run_test_import_with_process_memory_manager(
        &mut attached,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(standalone.handles(), attached.handles());
    assert_eq!(standalone.heap_cursor(), attached.heap_cursor());
    assert_eq!(standalone.last_mem_error(), attached.last_mem_error());

    standalone.cpu.gpr[3] = handle;
    attached.cpu.gpr[3] = handle;
    run_test_import(&mut standalone, PpcImportDispatcherTarget::DisposeHandle);
    run_test_import_with_process_memory_manager(
        &mut attached,
        PpcImportDispatcherTarget::DisposeHandle,
        &memory_manager,
    );
    assert_eq!(standalone.handles(), attached.handles());
    assert_eq!(
        standalone.free_handle_blocks(),
        attached.free_handle_blocks()
    );
    assert_eq!(standalone.memory.read_u32_be(handle), Some(0));
    assert_eq!(attached.memory.read_u32_be(handle), Some(0));
}

#[test]
fn standalone_native_runs_retain_the_process_memory_manager() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut native = load_pef_application(&pef).unwrap();
    native.cpu.gpr[3] = 24;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let handle = native.cpu.gpr[3];
    let retained = native.process_memory_manager.0.clone();
    assert_eq!(retained.borrow().state_for_handle(handle), Some(0));

    retained.borrow_mut().set_state_for_handle(handle, 0xa0);
    native.cpu.gpr[3] = handle;
    run_test_import(&mut native, PpcImportDispatcherTarget::HGetState);

    assert_eq!(native.cpu.gpr[3], 0xa0);
    assert!(native.process_memory_manager.0.ptr_eq(&retained));
}

#[test]
fn ppc_handle_state_imports_preserve_process_owned_classic_handles() {
    let pef = synthetic_pef_with_import(b"HLock");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);
    let shared = native.memory.shared_view();
    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    classic_bus.attach_guest_address_space(shared);
    context.attach_classic_memory_bus(&mut classic_bus);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 19)
        .unwrap();
    classic_bus.write_byte(ptr, 0x5a);
    native.memory.add_region(ptr & !0x0fff, vec![0; 0x1000]);
    native.memory.write_u32_be(handle, ptr).unwrap();
    native.memory.write_u8(ptr, 0x5a).unwrap();
    let memory_manager = context.memory_manager_handle();
    // HLock and HGetState operate on the live relocatable block while the
    // stable handle continues to identify its master pointer. Inside
    // Macintosh: Memory (1992), pp. 1-18--1-19 and 2-45--2-49.
    classic.set_handle_state_bits(handle, 0x40);
    let detached = memory_manager.detached_clone();

    native.with_process_memory_manager(
        |native, memory_manager| {
            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HGetState;
            native.cpu.gpr[3] = handle;
            let probe =
                native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            assert_eq!(native.cpu.gpr[3], 0x40);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetHandleSize;
            native.cpu.gpr[3] = handle;
            let probe =
                native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(native.cpu.gpr[3], 19);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HLock;
            native.cpu.gpr[3] = handle;
            let probe =
                native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(probe.handled_import_count, 1);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HGetState;
            native.cpu.gpr[3] = handle;
            let probe =
                native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(native.cpu.gpr[3], 0xc0);
        },
    );

    assert_eq!(classic.handle_state_bits(handle), Some(0xc0));
    assert_eq!(detached.borrow().state_for_handle(handle), Some(0x40));
}

#[test]
fn ppc_empty_handle_releases_a_process_owned_classic_block_atomically() {
    let pef = synthetic_pef_with_import(b"EmptyHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);

    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, 0x2000)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 24)
        .unwrap();
    classic_bus.write_bytes(ptr, b"classic process handle");
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager.borrow_mut().set_state_for_handle(handle, 0xc0);
    let mut detached = native.clone();

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = handle;
        let probe =
            native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_MEM_PUR_ERR)
        );
        assert_eq!(PpcMemory::read_u32_be(&mut native.memory, handle), Some(ptr));
        assert_eq!(memory_manager.classic_allocation_size(ptr), Some(24));
        assert_eq!(memory_manager.recover_handle(ptr), Some(handle));
        assert_eq!(memory_manager.state_for_handle(handle), Some(0xc0));

        memory_manager.set_state_for_handle(handle, 0x40);
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = handle;
        let probe =
            native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NO_ERR)
        );
        assert_eq!(PpcMemory::read_u32_be(&mut native.memory, handle), Some(0));
        assert_eq!(memory_manager.classic_allocation_size(handle), Some(4));
        assert_eq!(memory_manager.classic_allocation_size(ptr), None);
        assert_eq!(memory_manager.recover_handle(ptr), None);
        assert_eq!(memory_manager.state_for_handle(handle), Some(0x40));
    });

    assert_eq!(classic_bus.read_long(handle), 0);
    assert_eq!(classic_bus.get_alloc_size(handle), Some(4));
    assert_eq!(classic_bus.get_alloc_size(ptr), None);
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(detached_manager.classic_allocation_size(handle), Some(4));
    assert_eq!(detached_manager.classic_allocation_size(ptr), Some(24));
    assert_eq!(detached_manager.recover_handle(ptr), Some(handle));
    assert_eq!(detached_manager.state_for_handle(handle), Some(0xc0));
}

#[test]
fn ppc_dispose_handle_releases_classic_storage_and_preserves_stale_slot_rules() {
    let pef = synthetic_pef_with_import(b"DisposeHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);

    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, 0x2000)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    classic_bus.write_bytes(ptr, b"disposed classic");
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager.borrow_mut().set_state_for_handle(handle, 0x80);
    let mut detached = native.clone();

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = handle;
        let probe =
            native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NO_ERR)
        );
        assert_eq!(PpcMemory::read_u32_be(&mut native.memory, handle), Some(ptr));
        assert_eq!(memory_manager.classic_allocation_size(handle), None);
        assert_eq!(memory_manager.classic_allocation_size(ptr), None);
        assert_eq!(memory_manager.recover_handle(ptr), Some(handle));
        assert_eq!(memory_manager.state_for_handle(handle), None);

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::RecoverHandle;
        native.cpu.gpr[3] = ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], handle);
    });

    assert_eq!(classic_bus.read_long(handle), ptr);
    let reused = context
        .memory_manager_mut()
        .new_empty_classic_handle(&mut classic_bus)
        .unwrap();
    assert_eq!(reused, handle);
    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], 0);
        assert_eq!(memory_manager.recover_handle(ptr), None);
    });

    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(detached_manager.classic_allocation_size(handle), Some(4));
    assert_eq!(detached_manager.classic_allocation_size(ptr), Some(16));
    assert_eq!(detached_manager.recover_handle(ptr), Some(handle));
    assert_eq!(detached_manager.state_for_handle(handle), Some(0x80));
}

#[test]
fn ppc_handle_release_does_not_bypass_classic_resource_manager_ownership() {
    let pef = synthetic_pef_with_import(b"DisposeHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);

    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, 0x2000)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);
    classic_bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);
    let ptr = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 16);
    classic_bus.write_bytes(ptr, b"classic resource");
    let handle =
        classic.get_or_create_resource_handle(&mut classic_bus, *b"TEST", 128, ptr);
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager
        .borrow_mut()
        .set_process_handle_purgeable(handle, false);
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x20));

    native.with_process_memory_manager(|native, memory_manager| {
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
        native.cpu.gpr[3] = handle;
        let probe =
            native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NO_ERR)
        );
        assert_eq!(PpcMemory::read_u32_be(&mut native.memory, handle), Some(ptr));
        assert_eq!(memory_manager.classic_allocation_size(handle), Some(4));
        assert_eq!(memory_manager.classic_allocation_size(ptr), Some(16));
        assert_eq!(memory_manager.recover_handle(ptr), Some(handle));
        assert_eq!(memory_manager.state_for_handle(handle), Some(0x20));

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::EmptyHandle;
        native.cpu.gpr[3] = handle;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NIL_HANDLE_ERR)
        );
        assert_eq!(PpcMemory::read_u32_be(&mut native.memory, handle), Some(ptr));
        assert_eq!(memory_manager.classic_allocation_size(handle), Some(4));
        assert_eq!(memory_manager.classic_allocation_size(ptr), Some(16));
        assert_eq!(memory_manager.recover_handle(ptr), Some(handle));
        assert_eq!(memory_manager.state_for_handle(handle), Some(0x20));
    });

    assert_eq!(classic_bus.read_long(handle), ptr);
    assert_eq!(classic_bus.read_bytes(ptr, 16), b"classic resource");

    let mut classic_cpu = crate::trap::test_helpers::MockCpu::new();
    classic_cpu.write_reg(Register::A7, TEST_SP);
    classic_bus.write_long(TEST_SP, handle);
    assert!(classic
        .dispatch_toolbox(true, 0x1ad, &mut classic_cpu, &mut classic_bus)
        .unwrap()
        .is_ok());
    assert_eq!(classic_cpu.read_reg(Register::A7), TEST_SP + 4);
    assert_eq!(classic_bus.read_word(0x0a60), 0);
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0));

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
        native.cpu.gpr[3] = handle;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(memory_manager.classic_allocation_size(handle), None);
        assert_eq!(memory_manager.classic_allocation_size(ptr), None);
        assert_eq!(memory_manager.state_for_handle(handle), None);
    });
}

#[test]
fn ppc_dispose_handle_does_not_claim_an_external_pef_master_pointer() {
    let pef = synthetic_pef_with_loader_and_data(
        synthetic_loader(b"InterfaceLib", b"DisposeHandle"),
        &[0; 0x80],
    );
    let mut native = load_pef_application(&pef).unwrap();
    let handle = PPC_DATA_BASE + 0x40;
    let before = ppc_memory_read_bytes(&mut native.memory, PPC_DATA_BASE, 0x80).unwrap();

    native.with_process_memory_manager(|native, memory_manager| {
        let ptr = memory_manager.new_native_ptr(&mut native.memory, 32, true);
        assert_ne!(ptr, 0);
        PpcMemory::write_u32_be(&mut native.memory, handle, ptr).unwrap();
        let heap_cursor = memory_manager.native_heap_state().unwrap().heap_cursor;
        memory_manager.publish_external_native_resource_handle(handle, ptr, heap_cursor);
        let expected =
            ppc_memory_read_bytes(&mut native.memory, PPC_DATA_BASE, 0x80).unwrap();

        native.cpu.gpr[3] = handle;
        let probe =
            native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NO_ERR)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut native.memory, PPC_DATA_BASE, 0x80),
            Some(expected.clone())
        );
        assert!(memory_manager
            .native_allocator()
            .unwrap()
            .ptrs
            .iter()
            .any(|record| record.ptr == ptr && record.size == 32));
        assert_eq!(memory_manager.recover_handle(ptr), Some(handle));
        assert_eq!(memory_manager.state_for_handle(handle), Some(0x60));

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::EmptyHandle;
        native.cpu.gpr[3] = handle;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NIL_HANDLE_ERR)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut native.memory, PPC_DATA_BASE, 0x80),
            Some(expected)
        );
        assert_eq!(memory_manager.recover_handle(ptr), Some(handle));
        assert_eq!(memory_manager.state_for_handle(handle), Some(0x60));
    });

    let after = ppc_memory_read_bytes(&mut native.memory, PPC_DATA_BASE, 0x80).unwrap();
    assert_ne!(after, before);
    assert_eq!(&after[..0x40], &before[..0x40]);
    assert_eq!(&after[0x44..], &before[0x44..]);
}

#[test]
fn ppc_pointer_imports_observe_process_owned_classic_allocations() {
    let pef = synthetic_pef_with_import(b"GetPtrSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let ptr = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 37);
    assert_ne!(ptr, 0);
    classic_bus.fill_bytes(ptr, 37, 0x5a);
    assert_eq!(
        context
            .memory_manager_mut()
            .set_process_ptr_size(&mut classic_bus, ptr, 36),
        0
    );
    assert_eq!(classic_bus.read_byte(ptr + 36), 0);

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = ptr;
        let probe = native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(native.cpu.gpr[3], 36);

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposePtr;
        native.cpu.gpr[3] = ptr;
        let probe = native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(memory_manager.classic_allocation_size(ptr), None);
    });

    assert_eq!(classic_bus.get_alloc_size(ptr), None);
}

#[test]
fn ppc_set_handle_size_resizes_process_owned_classic_handle_across_isa() {
    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let original = (0x10..=0x1f).collect::<Vec<_>>();
    classic_bus.write_bytes(ptr, &original);
    let neighbor = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 16);
    classic_bus.fill_bytes(neighbor, 16, 0xa5);
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager
        .borrow_mut()
        .set_state_for_handle(handle, 0x40);
    let mut detached = native.clone();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 8;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(native.cpu.gpr[3], handle);
    assert_eq!(classic_bus.read_long(handle), ptr);
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(8));
    assert_eq!(
        classic_bus.read_bytes(ptr, 16),
        [original[..8].to_vec(), vec![0; 8]].concat()
    );
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), Some(handle));
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x40));
    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_NO_ERR)
    );

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 12;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(classic_bus.read_long(handle), ptr);
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(12));
    assert_eq!(
        classic_bus.read_bytes(ptr, 12),
        [original[..8].to_vec(), vec![0; 4]].concat()
    );
    classic_bus.write_bytes(ptr + 8, &[0x81, 0x82, 0x83, 0x84]);

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 48;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    let moved_ptr = classic_bus.read_long(handle);
    assert_ne!(moved_ptr, ptr);
    assert_eq!(classic_bus.get_alloc_size(ptr), None);
    assert_eq!(classic_bus.get_alloc_size(moved_ptr), Some(48));
    assert_eq!(
        classic_bus.read_bytes(moved_ptr, 12),
        [original[..8].to_vec(), vec![0x81, 0x82, 0x83, 0x84]].concat()
    );
    assert_eq!(classic_bus.read_bytes(neighbor, 16), vec![0xa5; 16]);
    assert_eq!(
        PpcMemory::read_u32_be(&mut native.memory, handle),
        Some(moved_ptr)
    );
    assert_eq!(
        memory_manager.borrow().classic_allocation_size(moved_ptr),
        Some(48)
    );
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), None);
    assert_eq!(
        memory_manager.borrow().handle_for_ptr(moved_ptr),
        Some(handle)
    );
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x40));
    assert_eq!(
        classic_bus.read_bytes(moved_ptr, 12),
        ppc_memory_read_bytes(&mut native.memory, moved_ptr, 12).unwrap()
    );

    native.cpu.gpr[3] = ptr;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::RecoverHandle,
        &memory_manager,
    );
    assert_eq!(native.cpu.gpr[3], 0);

    native.cpu.gpr[3] = moved_ptr;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::RecoverHandle,
        &memory_manager,
    );
    assert_eq!(native.cpu.gpr[3], handle);

    native.cpu.gpr[3] = handle;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::GetHandleSize,
        &memory_manager,
    );
    assert_eq!(native.cpu.gpr[3], 48);
    assert_eq!(
        classic_bus.read_bytes(moved_ptr, 12),
        ppc_memory_read_bytes(&mut native.memory, moved_ptr, 12).unwrap()
    );

    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    assert_eq!(detached_manager.classic_allocation_size(handle), Some(4));
    assert_eq!(detached_manager.classic_allocation_size(ptr), Some(16));
    assert_eq!(detached_manager.classic_allocation_size(moved_ptr), None);
    assert_eq!(detached_manager.handle_for_ptr(ptr), Some(handle));
    assert_eq!(detached_manager.handle_for_ptr(moved_ptr), None);
    assert_eq!(detached_manager.state_for_handle(handle), Some(0x40));
    assert_eq!(
        ppc_memory_read_bytes(&mut detached.memory, ptr, 16),
        Some(original)
    );
}

#[test]
fn ppc_set_handle_size_keeps_zero_size_empty_classic_handle_empty() {
    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    let handle = context
        .memory_manager_mut()
        .new_empty_classic_handle(&mut classic_bus)
        .unwrap();
    let memory_manager = context.memory_manager_handle().clone();
    let cursor = context.classic_heap_bump_ptr();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 0;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );

    assert_eq!(classic_bus.read_long(handle), 0);
    assert_eq!(classic_bus.get_alloc_size(handle), Some(4));
    assert_eq!(context.classic_heap_bump_ptr(), cursor);
    assert_eq!(memory_manager.borrow().handle_for_ptr(0), None);
    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_NO_ERR)
    );
}

#[test]
fn ppc_set_handle_size_preserves_classic_state_on_atomic_failure() {
    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 3 * 1024 * 1024, 0x4000);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let payload = (0xa0..=0xaf).collect::<Vec<_>>();
    classic_bus.write_bytes(ptr, &payload);
    let neighbor = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 16);
    classic_bus.fill_bytes(neighbor, 16, 0x5a);
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager
        .borrow_mut()
        .set_state_for_handle(handle, 0x40);
    let mut detached = native.clone();
    let handle_bytes = classic_bus.read_bytes(handle, 4);
    let cursor = context.classic_heap_bump_ptr();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 0x0010_0000;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_MEM_FULL_ERR)
    );
    assert_eq!(classic_bus.read_bytes(handle, 4), handle_bytes);
    assert_eq!(classic_bus.read_long(handle), ptr);
    assert_eq!(classic_bus.get_alloc_size(handle), Some(4));
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(16));
    assert_eq!(classic_bus.read_bytes(ptr, 16), payload);
    assert_eq!(classic_bus.read_bytes(neighbor, 16), vec![0x5a; 16]);
    assert_eq!(context.classic_heap_bump_ptr(), cursor);
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), Some(handle));
    assert_eq!(memory_manager.borrow().handle_for_ptr(neighbor), None);
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x40));

    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    assert_eq!(detached_manager.classic_allocation_size(ptr), Some(16));
    assert_eq!(
        ppc_memory_read_bytes(&mut detached.memory, ptr, 16),
        Some((0xa0..=0xaf).collect::<Vec<_>>())
    );
}

#[test]
fn ppc_set_handle_size_rejects_a_guest_mutated_classic_master_pointer() {
    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let payload = (0xb0..=0xbf).collect::<Vec<_>>();
    classic_bus.write_bytes(ptr, &payload);
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager
        .borrow_mut()
        .set_state_for_handle(handle, 0x40);
    let mut detached = native.clone();
    let invalid_ptr = 0xdead_beefu32;
    classic_bus.write_long(handle, invalid_ptr);
    let cursor = context.classic_heap_bump_ptr();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 48;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_MEM_WZ_ERR)
    );
    assert_eq!(classic_bus.read_long(handle), invalid_ptr);
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(16));
    assert_eq!(classic_bus.read_bytes(ptr, 16), payload);
    assert_eq!(context.classic_heap_bump_ptr(), cursor);
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), Some(handle));
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x40));

    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    assert_eq!(detached_manager.classic_allocation_size(ptr), Some(16));
    assert_eq!(detached_manager.handle_for_ptr(ptr), Some(handle));
}

#[test]
fn ppc_set_handle_size_rejects_another_classic_handles_data_block() {
    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let (other_handle, other_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 24)
        .unwrap();
    let original = (0x20..=0x2f).collect::<Vec<_>>();
    let other = (0x80..=0x97).collect::<Vec<_>>();
    classic_bus.write_bytes(ptr, &original);
    classic_bus.write_bytes(other_ptr, &other);
    classic_bus.write_long(handle, other_ptr);
    let memory_manager = context.memory_manager_handle().clone();
    let cursor = context.classic_heap_bump_ptr();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 48;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );

    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_MEM_WZ_ERR)
    );
    assert_eq!(classic_bus.read_long(handle), other_ptr);
    assert_eq!(classic_bus.read_long(other_handle), other_ptr);
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(16));
    assert_eq!(classic_bus.get_alloc_size(other_ptr), Some(24));
    assert_eq!(classic_bus.read_bytes(ptr, 16), original);
    assert_eq!(classic_bus.read_bytes(other_ptr, 24), other);
    assert_eq!(context.classic_heap_bump_ptr(), cursor);
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), Some(handle));
    assert_eq!(
        memory_manager.borrow().handle_for_ptr(other_ptr),
        Some(other_handle)
    );
}

#[test]
fn ppc_set_handle_size_rejects_locked_classic_relocation_without_mutation() {
    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let payload = (0xc0..=0xcf).collect::<Vec<_>>();
    classic_bus.write_bytes(ptr, &payload);
    let neighbor = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 16);
    classic_bus.fill_bytes(neighbor, 16, 0x6b);
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager
        .borrow_mut()
        .set_state_for_handle(handle, 0x80);
    let mut detached = native.clone();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 8;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(classic_bus.read_long(handle), ptr);
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(8));
    assert_eq!(
        classic_bus.read_bytes(ptr, 16),
        [payload[..8].to_vec(), vec![0; 8]].concat()
    );
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x80));

    let before_handle = classic_bus.read_bytes(handle, 4);
    let before_data = classic_bus.read_bytes(ptr, 16);
    let before_cursor = context.classic_heap_bump_ptr();
    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 48;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_MEM_FULL_ERR)
    );
    assert_eq!(classic_bus.read_bytes(handle, 4), before_handle);
    assert_eq!(classic_bus.read_long(handle), ptr);
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(8));
    assert_eq!(classic_bus.read_bytes(ptr, 16), before_data);
    assert_eq!(classic_bus.read_bytes(neighbor, 16), vec![0x6b; 16]);
    assert_eq!(context.classic_heap_bump_ptr(), before_cursor);
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), Some(handle));
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x80));

    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    assert_eq!(detached_manager.classic_allocation_size(ptr), Some(16));
    assert_eq!(detached_manager.state_for_handle(handle), Some(0x80));
}

#[test]
fn ppc_set_handle_size_does_not_bypass_classic_resource_ownership() {
    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    classic_bus.write_byte(crate::memory::globals::addr::RES_LOAD, 1);
    let ptr = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 16);
    let payload = (0xd0..=0xdf).collect::<Vec<_>>();
    classic_bus.write_bytes(ptr, &payload);
    let handle = classic.get_or_create_resource_handle(&mut classic_bus, *b"TEST", 128, ptr);
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager
        .borrow_mut()
        .set_process_handle_purgeable(handle, false);
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x20));
    let mut detached = native.clone();
    let before_handle = classic_bus.read_bytes(handle, 4);
    let before_cursor = context.classic_heap_bump_ptr();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 48;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_NIL_HANDLE_ERR)
    );
    assert_eq!(classic_bus.read_bytes(handle, 4), before_handle);
    assert_eq!(classic_bus.read_long(handle), ptr);
    assert_eq!(classic_bus.get_alloc_size(handle), Some(4));
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(16));
    assert_eq!(classic_bus.read_bytes(ptr, 16), payload);
    assert_eq!(context.classic_heap_bump_ptr(), before_cursor);
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), Some(handle));
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x20));

    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    assert_eq!(detached_manager.classic_allocation_size(ptr), Some(16));
    assert_eq!(detached_manager.state_for_handle(handle), Some(0x20));
}

#[test]
fn ppc_set_handle_size_does_not_claim_an_external_pef_master_pointer() {
    let pef = synthetic_pef_with_loader_and_data(
        synthetic_loader(b"InterfaceLib", b"SetHandleSize"),
        &[0; 0x80],
    );
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    classic_bus.attach_guest_address_space(native.memory.shared_view());
    let handle = PPC_DATA_BASE + 0x40;
    let memory_manager = context.memory_manager_handle().clone();
    let ptr = native.with_process_memory_manager(|native, memory_manager| {
        let ptr = memory_manager.new_native_ptr(&mut native.memory, 32, true);
        assert_ne!(ptr, 0);
        native
            .memory
            .write_bytes(ptr, b"external PEF payload")
            .unwrap();
        PpcMemory::write_u32_be(&mut native.memory, handle, ptr).unwrap();
        let heap_cursor = memory_manager.native_heap_state().unwrap().heap_cursor;
        memory_manager.publish_external_native_resource_handle(handle, ptr, heap_cursor);
        ptr
    });
    let mut detached = native.clone();
    let before_handle = PpcMemory::read_u32_be(&mut native.memory, handle);
    let expected_pef = ppc_memory_read_bytes(&mut native.memory, PPC_DATA_BASE, 0x80).unwrap();
    let detached_pef =
        ppc_memory_read_bytes(&mut detached.memory, PPC_DATA_BASE, 0x80).unwrap();

    native.cpu.gpr[3] = handle;
    native.cpu.gpr[4] = 64;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::SetHandleSize,
        &memory_manager,
    );
    assert_eq!(
        memory_manager
            .borrow()
            .native_heap_state()
            .map(|heap| heap.last_mem_error),
        Some(PPC_NIL_HANDLE_ERR)
    );
    assert_eq!(
        PpcMemory::read_u32_be(&mut native.memory, handle),
        before_handle
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, PPC_DATA_BASE, 0x80),
        Some(expected_pef)
    );
    assert_eq!(memory_manager.borrow().native_allocation(handle), None);
    assert!(memory_manager
        .borrow()
        .native_ptr_records()
        .iter()
        .any(|record| record.ptr == ptr && record.size == 32));
    assert_eq!(memory_manager.borrow().handle_for_ptr(ptr), Some(handle));
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x60));
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, ptr, b"external PEF payload".len() as u32),
        Some(b"external PEF payload".to_vec())
    );

    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut detached.memory, PPC_DATA_BASE, 0x80),
        Some(detached_pef)
    );
    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(detached_manager.native_allocation(handle), None);
    assert!(detached_manager
        .native_ptr_records()
        .iter()
        .any(|record| record.ptr == ptr && record.size == 32));
    assert_eq!(
        ppc_memory_read_bytes(
            &mut detached.memory,
            ptr,
            b"external PEF payload".len() as u32,
        ),
        Some(b"external PEF payload".to_vec())
    );
}

#[test]
fn ppc_set_ptr_size_mutates_a_classic_pointer_without_moving_it() {
    let pef = synthetic_pef_with_import(b"SetPtrSize");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);

    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, 0x1000)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);

    let original = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 64);
    classic_bus.fill_bytes(original, 64, 0x6a);
    context
        .memory_manager_mut()
        .dispose_process_ptr(&mut classic_bus, original);
    let ptr = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 16);
    assert_eq!(ptr, original);
    classic_bus.fill_bytes(ptr, 16, 0x5a);
    let neighbor = context
        .memory_manager_mut()
        .new_classic_ptr(&mut classic_bus, 16);
    classic_bus.fill_bytes(neighbor, 16, 0xa5);
    let cursor = context.classic_heap_bump_ptr();
    let mut detached = native.clone();

    context
        .memory_manager_mut()
        .set_native_mem_error(PPC_PARAM_ERR);
    assert_eq!(
        context
            .memory_manager_mut()
            .set_process_ptr_size(&mut classic_bus, ptr, 12),
        PPC_NO_ERR
    );
    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::MemError;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    });
    assert_eq!(
        context
            .memory_manager_mut()
            .set_process_ptr_size(&mut classic_bus, ptr, 65),
        PPC_MEM_FULL_ERR
    );
    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::MemError;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetPtrSize;
    });

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = ptr;
        native.cpu.gpr[4] = 8;
        let probe = native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(native.cpu.gpr[3], ptr);
        assert_eq!(memory_manager.classic_allocation_size(ptr), Some(8));
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NO_ERR)
        );

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::MemError;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetPtrSize;

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = ptr;
        native.cpu.gpr[4] = 48;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], ptr);
        assert_eq!(memory_manager.classic_allocation_size(ptr), Some(48));
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_NO_ERR)
        );

        let bytes_before_failure = ppc_memory_read_bytes(&mut native.memory, ptr, 64).unwrap();
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = ptr;
        native.cpu.gpr[4] = 65;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], ptr);
        assert_eq!(memory_manager.classic_allocation_size(ptr), Some(48));
        assert_eq!(
            memory_manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error),
            Some(PPC_MEM_FULL_ERR)
        );
        assert_eq!(
            ppc_memory_read_bytes(&mut native.memory, ptr, 64),
            Some(bytes_before_failure)
        );

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::MemError;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    });

    assert_eq!(context.classic_heap_bump_ptr(), cursor);
    assert_eq!(classic_bus.get_alloc_size(ptr), Some(48));
    assert_eq!(classic_bus.read_bytes(ptr, 8), vec![0x5a; 8]);
    assert_eq!(classic_bus.read_bytes(ptr + 8, 8), vec![0; 8]);
    assert_eq!(classic_bus.read_bytes(neighbor, 16), vec![0xa5; 16]);
    assert_eq!(
        detached
            .process_memory_manager
            .0
            .borrow()
            .classic_allocation_size(ptr),
        Some(16)
    );
    assert_eq!(
        ppc_memory_read_bytes(&mut detached.memory, ptr, 16),
        Some(vec![0x5a; 16])
    );
}

#[test]
fn attaching_and_cloning_native_adapters_preserves_process_ownership() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut native = load_pef_application(&pef).unwrap();
    let standalone_memory_manager = native.process_memory_manager.0.clone();
    native.cpu.gpr[3] = 24;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::NewHandle { clear: true },
    );
    let handle = native.cpu.gpr[3];

    let mut detached = native.clone();
    assert!(!detached
        .process_memory_manager
        .0
        .ptr_eq(&native.process_memory_manager.0));

    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let process_memory_manager = context.memory_manager_handle().clone();
    assert!(native
        .process_memory_manager
        .ptr_eq(&process_memory_manager));
    assert!(!standalone_memory_manager.borrow().has_native_allocator());
    assert!(standalone_memory_manager
        .borrow()
        .native_handle_records()
        .is_empty());
    let attached_heap = process_memory_manager.borrow().native_heap_state().unwrap();
    let expected_free = ppc_heap_free_capacity(
        &mut native.memory,
        attached_heap.heap_cursor,
        attached_heap.heap_limit,
    )
    .0;
    assert_eq!(
        native.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(expected_free)
    );
    assert_eq!(
        native.memory.read_u32_be(PPC_SYSTEM_ZONE + 12),
        Some(expected_free)
    );
    let detached_heap_cursor = detached.heap_cursor();
    let detached_handle_record = detached.handles()[0];
    let attached_heap_cursor = native.heap_cursor() + 0x80;
    let attached_heap_limit = native.heap_limit() - 0x1000;

    process_memory_manager
        .borrow_mut()
        .set_native_handle_state(PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: true,
            no_purge: true,
            resource: true,
        });
    process_memory_manager
        .borrow_mut()
        .mutate_native_allocator(|allocator| {
            allocator.heap.heap_cursor = attached_heap_cursor;
            allocator.heap.heap_limit = attached_heap_limit;
            allocator.heap.last_mem_error = PPC_PARAM_ERR;
            allocator.heap.heap_maximized = true;
            allocator.heap.master_pointer_blocks_requested = 7;
            allocator.ptrs.push(ProcessPtrRecord {
                ptr: PPC_HEAP_BASE + 0x100,
                size: 16,
            });
            allocator.free_ptr_blocks.push(ProcessPtrRecord {
                ptr: PPC_HEAP_BASE + 0x200,
                size: 32,
            });
            allocator.free_handle_blocks.push(ProcessHandleRecord {
                handle: PPC_HEAP_BASE + 0x300,
                ptr: PPC_HEAP_BASE + 0x400,
                size: 24,
                capacity: 48,
            });
        });
    let mut attached_handle_record = process_memory_manager
        .borrow()
        .native_allocation(handle)
        .unwrap();
    attached_handle_record.size = 20;
    process_memory_manager
        .borrow_mut()
        .set_native_allocation(attached_handle_record);
    assert!(native.heap_maximized());
    assert_eq!(native.heap_cursor(), attached_heap_cursor);
    assert_eq!(native.heap_limit(), attached_heap_limit);
    assert_eq!(native.last_mem_error(), PPC_PARAM_ERR);
    assert_eq!(native.master_pointer_blocks_requested(), 7);
    assert!(!detached.heap_maximized());
    assert_eq!(detached.heap_cursor(), detached_heap_cursor);
    assert_eq!(detached.heap_limit(), detached.stack_base);
    assert_eq!(detached.last_mem_error(), PPC_NO_ERR);
    assert_eq!(detached.master_pointer_blocks_requested(), 0);
    assert_eq!(native.ptrs().last().map(|record| record.size), Some(16));
    assert_eq!(
        native.free_ptr_blocks().last().map(|record| record.size),
        Some(32)
    );
    assert!(detached.ptrs().is_empty());
    assert!(detached.free_ptr_blocks().is_empty());
    assert_eq!(
        native
            .free_handle_blocks()
            .last()
            .map(|record| record.capacity),
        Some(48)
    );
    assert!(detached.free_handle_blocks().is_empty());
    assert!(native.handle_states()[0].high_locked);
    assert!(!detached.handle_states()[0].high_locked);
    assert_eq!(native.handles()[0], attached_handle_record);
    assert_eq!(detached.handles()[0], detached_handle_record);
    native.cpu.gpr[3] = handle;
    run_test_import(&mut native, PpcImportDispatcherTarget::HGetState);
    assert_eq!(native.cpu.gpr[3], 0xa0);

    assert_eq!(
        detached
            .process_memory_manager
            .0
            .borrow()
            .state_for_handle(handle),
        Some(0)
    );
    detached.cpu.gpr[3] = handle;
    run_test_import(&mut detached, PpcImportDispatcherTarget::HGetState);
    assert_eq!(detached.cpu.gpr[3], 0);
}

#[test]
fn attaching_a_pristine_native_adapter_retains_the_populated_process_allocator() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut owner = load_pef_application(&pef).unwrap();
    owner.cpu.gpr[3] = 24;
    run_test_import(
        &mut owner,
        PpcImportDispatcherTarget::NewPtr { clear: true },
    );
    let ptr = owner.cpu.gpr[3];
    let mut context = ProcessContext::default();
    owner.attach_unconverted_process_services(&mut context);
    let canonical = context.memory_manager_handle().clone();
    let canonical_allocator = canonical.borrow().native_allocator_snapshot();

    let mut observer = load_pef_application(&pef).unwrap();
    let standalone = observer.process_memory_manager.0.clone();
    observer.attach_unconverted_process_services(&mut context);

    assert!(observer.process_memory_manager.ptr_eq(&canonical));
    assert_eq!(canonical.borrow().native_allocator_snapshot(), canonical_allocator);
    assert_eq!(canonical.borrow_mut().native_ptr_size(ptr), 24);
    assert!(!standalone.borrow().has_native_allocator());
    let heap = canonical.borrow().native_heap_state().unwrap();
    let expected_free =
        ppc_heap_free_capacity(&mut observer.memory, heap.heap_cursor, heap.heap_limit).0;
    assert_eq!(
        observer.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(expected_free)
    );
    assert_eq!(
        observer.memory.read_u32_be(PPC_SYSTEM_ZONE + 12),
        Some(expected_free)
    );
}

#[test]
fn attaching_two_populated_native_adapters_is_atomic() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut owner = load_pef_application(&pef).unwrap();
    owner.cpu.gpr[3] = 24;
    run_test_import(
        &mut owner,
        PpcImportDispatcherTarget::NewPtr { clear: true },
    );
    let mut context = ProcessContext::default();
    owner.attach_unconverted_process_services(&mut context);
    let canonical = context.memory_manager_handle().clone();

    let mut rejected = load_pef_application(&pef).unwrap();
    rejected.cpu.gpr[3] = 48;
    run_test_import(
        &mut rejected,
        PpcImportDispatcherTarget::NewPtr { clear: true },
    );
    let standalone = rejected.process_memory_manager.0.clone();
    let canonical_allocator = canonical.borrow().native_allocator_snapshot();
    let standalone_allocator = standalone.borrow().native_allocator_snapshot();
    let application_zone = rejected.memory.read_u32_be(PPC_APPLICATION_ZONE + 12);
    let system_zone = rejected.memory.read_u32_be(PPC_SYSTEM_ZONE + 12);

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        rejected.attach_unconverted_process_services(&mut context);
    }));

    assert!(result.is_err());
    assert!(!rejected.process_memory_manager.ptr_eq(&canonical));
    assert_eq!(canonical.borrow().native_allocator_snapshot(), canonical_allocator);
    assert_eq!(standalone.borrow().native_allocator_snapshot(), standalone_allocator);
    assert_eq!(
        rejected.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        application_zone
    );
    assert_eq!(
        rejected.memory.read_u32_be(PPC_SYSTEM_ZONE + 12),
        system_zone
    );
}

#[test]
fn ppc_ptr_imports_mutate_the_process_memory_manager_immediately() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut native = load_pef_application(&pef).unwrap();
    native.cpu.gpr[3] = 24;
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    native.with_process_memory_manager(
        |native, memory_manager| {
            let prior_ptr = memory_manager.new_native_ptr(&mut native.memory, 24, false);
            assert_eq!(prior_ptr, PPC_HEAP_BASE);
            let probe =
                native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(probe.handled_import_count, 1);
            let ptr = native.cpu.gpr[3];
            assert_eq!(ptr, PPC_HEAP_BASE + ppc_allocation_size(24).unwrap());
            assert_eq!(
                memory_manager
                    .native_allocator()
                    .and_then(|allocator| allocator.ptrs.last())
                    .copied(),
                Some(ProcessPtrRecord { ptr, size: 24 })
            );
            assert_eq!(
                memory_manager
                    .native_allocator()
                    .and_then(|allocator| allocator.ptrs.first())
                    .copied(),
                Some(ProcessPtrRecord {
                    ptr: prior_ptr,
                    size: 24,
                })
            );

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetPtrSize;
            native.cpu.gpr[3] = ptr;
            native.cpu.gpr[4] = 40;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(
                memory_manager
                    .native_allocator()
                    .and_then(|allocator| allocator.ptrs.last())
                    .copied(),
                Some(ProcessPtrRecord { ptr, size: 40 })
            );

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetPtrSize;
            native.cpu.gpr[3] = ptr;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 40);
            assert_eq!(
                memory_manager
                    .native_allocator()
                    .map(|allocator| allocator.heap.last_mem_error),
                Some(PPC_NO_ERR)
            );

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposePtr;
            native.cpu.gpr[3] = ptr;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            let allocator = memory_manager.native_allocator().unwrap();
            assert_eq!(
                allocator.ptrs,
                vec![ProcessPtrRecord {
                    ptr: prior_ptr,
                    size: 24,
                }]
            );
            assert_eq!(
                allocator.free_ptr_blocks,
                vec![ProcessPtrRecord { ptr, size: 40 }]
            );
        },
    );
}

#[test]
fn ppc_handle_imports_mutate_the_process_memory_manager_immediately() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut native = load_pef_application(&pef).unwrap();
    native.cpu.gpr[3] = 24;
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_dispatcher = TrapDispatcher::new();
    classic_dispatcher.attach_unconverted_process_services(&mut context);
    native.with_process_memory_manager(
        |native, memory_manager| {
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            let handle = native.cpu.gpr[3];
            let original = memory_manager.native_allocation(handle).unwrap();
            let detached_memory_manager = memory_manager.detached_clone();
            assert_eq!(original.size, 24);
            assert_eq!(classic_dispatcher.handle_state_bits(handle), Some(0));

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::RecoverHandle;
            native.cpu.gpr[3] = original.ptr;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], handle);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HLock;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(memory_manager.handle_state(handle), 0x80);
            assert_eq!(classic_dispatcher.handle_state_bits(handle), Some(0x80));
            assert_eq!(detached_memory_manager.state_for_handle(handle), Some(0));

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HLockHi;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert!(memory_manager.native_handle_state(handle).high_locked);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HNoPurge;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(memory_manager.handle_state(handle), 0x80);
            assert_eq!(classic_dispatcher.handle_state_bits(handle), Some(0x80));

            memory_manager.set_state_for_handle(handle, 0xa0);
            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HGetState;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 0xa0);
            assert!(
                (0..32).all(|offset| native.memory.read_u8(original.ptr + offset) == Some(0))
            );
            native
                .memory
                .write_bytes(original.ptr, b"process-owned")
                .unwrap();

            let blocking_ptr = memory_manager.new_native_ptr(&mut native.memory, 16, false);
            assert_ne!(blocking_ptr, 0);
            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetHandleSize;
            native.cpu.gpr[3] = handle;
            native.cpu.gpr[4] = 64;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            let grown = memory_manager.native_allocation(handle).unwrap();
            assert_ne!(grown.ptr, original.ptr);
            assert_eq!((grown.size, grown.capacity), (64, 64));
            assert_eq!(
                ppc_memory_read_bytes(&mut native.memory, grown.ptr, 13),
                Some(b"process-owned".to_vec())
            );
            assert_eq!(native.memory.read_u32_be(handle), Some(grown.ptr));
            assert_eq!(
                memory_manager
                    .native_handle_records()
                    .iter()
                    .find(|record| record.handle == handle)
                    .copied(),
                Some(grown)
            );

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::RecoverHandle;
            native.cpu.gpr[3] = original.ptr;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 0);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.cpu.gpr[3] = grown.ptr;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], handle);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetHandleSize;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 64);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::EmptyHandle;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(
                memory_manager
                    .native_heap_state()
                    .map(|heap| heap.last_mem_error),
                Some(-112)
            );
            assert_eq!(native.memory.read_u32_be(handle), Some(grown.ptr));
            assert_eq!(memory_manager.native_allocation(handle), Some(grown));

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HUnlock;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(memory_manager.state_for_handle(handle), Some(0x20));

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::EmptyHandle;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(
                memory_manager
                    .native_heap_state()
                    .map(|heap| heap.last_mem_error),
                Some(PPC_NO_ERR)
            );
            assert_eq!(native.memory.read_u32_be(handle), Some(0));
            assert_eq!(
                memory_manager.native_allocation(handle),
                Some(ProcessHandleRecord {
                    handle,
                    ptr: 0,
                    size: 0,
                    capacity: 0,
                })
            );
            assert_eq!(memory_manager.recover_handle(grown.ptr), None);
            assert!(memory_manager
                .native_allocator()
                .unwrap()
                .free_ptr_blocks
                .iter()
                .any(|record| record.ptr == grown.ptr && record.size == grown.capacity));

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
            native.cpu.gpr[3] = handle;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(memory_manager.native_allocation(handle), None);
            assert_eq!(native.memory.read_u32_be(handle), Some(0));
            assert!(!memory_manager
                .native_handle_records()
                .iter()
                .any(|record| record.handle == handle));
            assert!(memory_manager
                .native_allocator()
                .unwrap()
                .free_handle_blocks
                .iter()
                .any(|record| record.handle == handle && record.ptr == 0));

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::RecoverHandle;
            native.cpu.gpr[3] = grown.ptr;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 0);

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target =
                PpcImportDispatcherTarget::NewHandle { clear: true };
            native.cpu.gpr[3] = 32;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], handle);
            let reused = memory_manager.native_allocation(handle).unwrap();
            assert_ne!(reused.ptr, 0);
            assert_eq!((reused.size, reused.capacity), (32, 32));
            assert_eq!(
                ppc_memory_read_bytes(&mut native.memory, reused.ptr, 32),
                Some(vec![0; 32])
            );
            assert!(!memory_manager
                .native_allocator()
                .unwrap()
                .free_handle_blocks
                .iter()
                .any(|record| record.handle == handle));
        },
    );
}

#[test]
fn ppc_resource_imports_mutate_process_handle_state_across_isa() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut native = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    let name_ptr = scratch;
    native.memory.add_region(scratch, vec![0; 0x40]);
    write_ppc_pstring(&mut native.memory, name_ptr, b"Shared");
    native.set_current_resource_refnum(PPC_FIRST_FILE_REF_NUM);
    native.cpu.gpr[3] = 16;

    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic = TrapDispatcher::new();
    classic.attach_unconverted_process_services(&mut context);

    native.with_process_memory_manager(|native, memory_manager| {
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        let handle = native.cpu.gpr[3];
        assert_ne!(handle, 0);
        memory_manager.set_state_for_handle(handle, 0xc0);
        let detached = memory_manager.detached_clone();

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::AddResource;
        native.cpu.gpr[3] = handle;
        native.cpu.gpr[4] = u32::from_be_bytes(*b"TEST");
        native.cpu.gpr[5] = 128;
        native.cpu.gpr[6] = 0xdead_beef;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.test_resource_error(), PPC_ADD_RES_FAILED);
        assert_eq!(memory_manager.handle_state(handle), 0xc0);
        assert_eq!(classic.handle_state_bits(handle), Some(0xc0));

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[6] = name_ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.test_resource_error(), PPC_NO_ERR);
        assert_eq!(memory_manager.handle_state(handle), 0xe0);
        assert_eq!(classic.handle_state_bits(handle), Some(0xe0));
        assert_eq!(detached.state_for_handle(handle), Some(0xc0));

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target =
            PpcImportDispatcherTarget::NewHandle { clear: true };
        native.cpu.gpr[3] = 8;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        let replacement = native.cpu.gpr[3];
        memory_manager.set_state_for_handle(replacement, 0x80);

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::AddResource;
        native.cpu.gpr[3] = replacement;
        native.cpu.gpr[4] = u32::from_be_bytes(*b"TEST");
        native.cpu.gpr[5] = 128;
        native.cpu.gpr[6] = name_ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.test_resource_error(), PPC_NO_ERR);
        assert_eq!(memory_manager.handle_state(handle), 0xc0);
        assert_eq!(classic.handle_state_bits(handle), Some(0xc0));
        assert_eq!(memory_manager.handle_state(replacement), 0xa0);
        assert_eq!(classic.handle_state_bits(replacement), Some(0xa0));
        assert_eq!(detached.state_for_handle(replacement), None);

        native.set_current_resource_refnum(PPC_FIRST_FILE_REF_NUM + 1);
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::RemoveResource;
        native.cpu.gpr[3] = replacement;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.test_resource_error(), PPC_RMV_RES_FAILED);
        assert_eq!(memory_manager.handle_state(replacement), 0xa0);
        assert_eq!(classic.handle_state_bits(replacement), Some(0xa0));

        native.set_current_resource_refnum(PPC_FIRST_FILE_REF_NUM);
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.test_resource_error(), PPC_NO_ERR);
        assert_eq!(memory_manager.handle_state(handle), 0xc0);
        assert_eq!(classic.handle_state_bits(handle), Some(0xc0));
        assert_eq!(memory_manager.handle_state(replacement), 0x80);
        assert_eq!(classic.handle_state_bits(replacement), Some(0x80));
        assert_eq!(detached.state_for_handle(handle), Some(0xc0));
        assert!(native.process_file_system.vfs_resources.is_empty());
    });
}

#[test]
fn ppc_recover_handle_validates_reused_classic_master_pointer_slots() {
    let pef = synthetic_pef_with_import(b"RecoverHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, 0x4000)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);

    let (old_handle, old_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let mut detached = native.clone();

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = old_ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], old_handle);
    });

    context
        .memory_manager_mut()
        .dispose_classic_handle(&mut classic_bus, old_handle, true);
    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = old_ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], old_handle);
    });
    let (new_handle, new_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 32)
        .unwrap();
    assert_eq!(new_handle, old_handle);
    assert_ne!(new_ptr, old_ptr);

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = old_ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], 0);
        assert_eq!(memory_manager.recover_handle(old_ptr), None);

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.cpu.gpr[3] = new_ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], new_handle);
    });

    assert_eq!(
        detached
            .process_memory_manager
            .0
            .borrow()
            .recover_handle(old_ptr),
        Some(old_handle)
    );
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, old_handle),
        Some(old_ptr)
    );
}

#[test]
fn ppc_recover_handle_rejects_a_guest_mutated_classic_master_pointer_slot() {
    let pef = synthetic_pef_with_import(b"RecoverHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, 0x4000)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);

    let (handle, ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let mut detached = native.clone();
    classic_bus.write_long(handle, ptr + 4);

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], 0);
        assert_eq!(memory_manager.recover_handle(ptr), None);
    });

    assert_eq!(
        detached
            .process_memory_manager
            .0
            .borrow()
            .recover_handle(ptr),
        Some(handle)
    );
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
}

#[test]
fn ppc_recover_handle_rejects_a_reused_nil_classic_master_pointer_slot() {
    let pef = synthetic_pef_with_import(b"RecoverHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus = MacMemoryBus::new(8 * 1024 * 1024);
    context.attach_classic_memory_bus(&mut classic_bus);
    let classic_heap = classic_bus
        .shared_ram_region(0x0020_0000, 0x4000)
        .expect("classic adapter owns its heap range");
    context.attach_memory(0x0020_0000, classic_heap, &mut native.memory);

    let (old_handle, old_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    context
        .memory_manager_mut()
        .dispose_classic_handle(&mut classic_bus, old_handle, true);
    let empty_handle = context
        .memory_manager_mut()
        .new_empty_classic_handle(&mut classic_bus)
        .unwrap();
    assert_eq!(empty_handle, old_handle);
    assert_eq!(classic_bus.read_long(empty_handle), 0);

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = old_ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], 0);
        assert_eq!(memory_manager.recover_handle(old_ptr), None);
    });
}

#[test]
fn ppc_recover_handle_rejects_a_guest_mutated_native_master_pointer_slot() {
    let pef = synthetic_pef_with_import(b"RecoverHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let (handle, ptr) = native.with_process_memory_manager(|native, memory_manager| {
        let handle = memory_manager.new_native_handle(&mut native.memory, 24, true);
        let ptr = PpcMemory::read_u32_be(&mut native.memory, handle).unwrap();
        (handle, ptr)
    });
    let mut detached = native.clone();
    PpcMemory::write_u32_be(&mut native.memory, handle, 0).unwrap();

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], 0);
        assert_eq!(memory_manager.recover_handle(ptr), None);
    });

    assert_eq!(
        detached
            .process_memory_manager
            .0
            .borrow()
            .recover_handle(ptr),
        Some(handle)
    );
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle),
        Some(ptr)
    );
}

#[test]
fn ppc_recover_handle_rejects_a_directly_disposed_native_handle() {
    let pef = synthetic_pef_with_import(b"NewHandle");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);

    native.with_process_memory_manager(|native, memory_manager| {
        native.cpu.gpr[3] = 24;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        let handle = native.cpu.gpr[3];
        let record = memory_manager.native_allocation(handle).unwrap();

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
        native.cpu.gpr[3] = handle;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(PpcMemory::read_u32_be(&mut native.memory, handle), Some(0));
        assert_eq!(memory_manager.native_allocation(handle), None);
        assert_eq!(memory_manager.recover_handle(record.ptr), None);
        assert!(memory_manager
            .native_allocator()
            .unwrap()
            .free_handle_blocks
            .contains(&record));

        native.cpu.pc = native.entry_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::RecoverHandle;
        native.cpu.gpr[3] = record.ptr;
        native.run_with_process_memory_manager(64, false, false, memory_manager);
        assert_eq!(native.cpu.gpr[3], 0);
    });
}

#[test]
fn resource_fork_materialization_preserves_live_edits_and_other_paths() {
    let fork = serialize_resource_fork(&[
        ResourceForkEntry {
            res_type: *b"TEST",
            id: 1,
            name: vec![],
            data: b"raw".to_vec(),
            attrs: 0,
        },
        ResourceForkEntry {
            res_type: *b"TEST",
            id: 2,
            name: vec![],
            data: b"second".to_vec(),
            attrs: 0,
        },
        ResourceForkEntry {
            res_type: *b"DATA",
            id: 1,
            name: vec![],
            data: b"data".to_vec(),
            attrs: 0,
        },
    ])
    .unwrap();
    let files = [PpcVfsResourceFileRecord {
        path: "App".into(),
        creator: 0,
        file_type: 0,
        finder_flags: 0,
        resource_len: fork.len() as u32,
        raw_data: Some(fork.into()),
        map_attrs: 0,
        dirty: false,
    }];
    let edited = PpcVfsResourceRecord {
        ref_num: 7,
        path: "APP".into(),
        res_type: u32::from_be_bytes(*b"TEST"),
        res_id: 1,
        name: b"edited name".to_vec(),
        data: b"edited".to_vec(),
        raw_data: None,
        raw_attrs: None,
        attrs: 2,
        handle: 0x1234,
    };
    let mut other = edited.clone();
    other.path = "Other".into();
    other.res_id = 2;
    let mut resources = vec![edited, other];
    ppc_materialize_resource_records_for_path(&files, &mut resources, "app");
    ppc_materialize_resource_records_for_path(&files, &mut resources, "APP");
    assert_eq!(resources.len(), 4);
    assert_eq!(resources[0].data, b"edited");
    assert_eq!(resources[0].name, b"edited name");
    assert_eq!(
        (
            resources[0].ref_num,
            resources[0].attrs,
            resources[0].handle
        ),
        (7, 2, 0x1234)
    );
    assert_eq!(resources[1].path, "Other");
    assert_eq!(
        (resources[2].res_type, resources[2].res_id),
        (u32::from_be_bytes(*b"DATA"), 1)
    );
    assert_eq!(
        (resources[3].res_type, resources[3].res_id),
        (u32::from_be_bytes(*b"TEST"), 2)
    );
    assert_eq!(resources[3].data, b"second");
}

#[test]
fn ppc_resource_materialization_mutates_process_manager_immediately() {
    let pef = synthetic_pef_with_import(b"GetResource");
    let mut native = load_pef_application(&pef).unwrap();
    let external_handle = PPC_DATA_BASE + 0x2140;
    native.memory.add_region(external_handle, vec![0; 4]);
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_dispatcher = TrapDispatcher::new();
    classic_dispatcher.attach_unconverted_process_services(&mut context);
    native.with_process_memory_manager(
        |native, memory_manager| {
            let allocator = memory_manager.native_allocator_snapshot().unwrap();
            let mut heap_cursor = allocator.heap.heap_cursor;
            let heap_limit = allocator.heap.heap_limit;
            let mut last_mem_error = allocator.heap.last_mem_error;
            let mut handles = memory_manager.native_handle_records().to_vec();
            let mut resources = vec![PpcVfsResourceRecord {
                ref_num: 0,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"pref"),
                res_id: 42,
                name: b"Prefs".to_vec(),
                data: b"process resource".to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            }];
            let mut last_resource_error = PPC_NO_ERR;
            let canonical_ptr =
                memory_manager.new_native_ptr(&mut native.memory, 32, true);
            assert_ne!(canonical_ptr, 0);
            let cursor_after_canonical_allocation = memory_manager
                .native_heap_state()
                .unwrap()
                .heap_cursor;

            let handle = ppc_materialize_vfs_resource_handle(
                memory_manager,
                &mut native.memory,
                &mut heap_cursor,
                heap_limit,
                &mut last_mem_error,
                &mut handles,
                &mut resources,
                0,
                false,
                &mut last_resource_error,
            );
            assert_ne!(handle, 0);
            assert!(handle >= cursor_after_canonical_allocation);
            assert_eq!(memory_manager.native_allocation(handle).unwrap().ptr, 0);
            assert_eq!(memory_manager.state_for_handle(handle), Some(0x60));
            assert_eq!(classic_dispatcher.handle_state_bits(handle), Some(0x60));
            let detached = memory_manager.detached_clone();

            let loaded = ppc_materialize_vfs_resource_handle(
                memory_manager,
                &mut native.memory,
                &mut heap_cursor,
                heap_limit,
                &mut last_mem_error,
                &mut handles,
                &mut resources,
                0,
                true,
                &mut last_resource_error,
            );
            assert_eq!(loaded, handle);
            let record = memory_manager.native_allocation(handle).unwrap();
            let shared = native.memory.shared_view();
            let mut classic_bus = MacMemoryBus::new(0x2000);
            classic_bus.attach_guest_address_space(shared);
            assert_eq!(
                classic_bus.read_bytes(record.ptr, record.size as usize),
                b"process resource"
            );
            assert_eq!(memory_manager.recover_handle(record.ptr), Some(handle));
            assert_eq!(memory_manager.state_for_handle(handle), Some(0x60));
            assert_eq!(detached.native_allocation(handle).unwrap().ptr, 0);
            assert_eq!(detached.recover_handle(record.ptr), None);
            assert_eq!(last_mem_error, PPC_NO_ERR);
            assert_eq!(last_resource_error, PPC_NO_ERR);

            let mut external_resources = vec![PpcVfsResourceRecord {
                ref_num: 0,
                path: "Test App".to_string(),
                res_type: u32::from_be_bytes(*b"STR "),
                res_id: 128,
                name: b"External".to_vec(),
                data: b"PEF master pointer".to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: external_handle,
            }];
            let materialized = ppc_materialize_vfs_resource_handle(
                memory_manager,
                &mut native.memory,
                &mut heap_cursor,
                heap_limit,
                &mut last_mem_error,
                &mut handles,
                &mut external_resources,
                0,
                true,
                &mut last_resource_error,
            );
            assert_eq!(materialized, external_handle);
            assert_eq!(memory_manager.native_allocation(external_handle), None);
            let external_ptr = native.memory.read_u32_be(external_handle).unwrap();
            assert_ne!(external_ptr, 0);
            assert_eq!(
                memory_manager.recover_handle(external_ptr),
                Some(external_handle)
            );
            assert_eq!(memory_manager.state_for_handle(external_handle), Some(0x60));
            assert_eq!(
                classic_dispatcher.handle_state_bits(external_handle),
                Some(0x60)
            );
            assert_eq!(
                classic_bus.read_bytes(external_ptr, b"PEF master pointer".len()),
                b"PEF master pointer"
            );
            assert_eq!(detached.recover_handle(external_ptr), None);
        },
    );
}

#[test]
fn ppc_handle_copy_imports_mutate_process_memory_immediately() {
    let pef = synthetic_pef_with_import(b"PtrToHand");
    let mut native = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x2100;
    let source = scratch;
    let output = scratch + 0x20;
    let handle_variable = scratch + 0x24;
    native.memory.add_region(scratch, vec![0; 0x100]);
    native.memory.write_bytes(source, b"native").unwrap();
    let mut classic_bus = crate::memory::MacMemoryBus::new(0x2000);
    let shared = native.memory.shared_view();
    classic_bus.attach_guest_address_space(shared);

    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    native.with_process_memory_manager(
        |native, memory_manager| {
            native.cpu.gpr[3] = source;
            native.cpu.gpr[4] = output;
            native.cpu.gpr[5] = 6;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 0);
            let destination = native.memory.read_u32_be(output).unwrap();
            let original = memory_manager.native_allocation(destination).unwrap();
            assert_eq!(classic_bus.read_bytes(original.ptr, 6), b"native");
            assert_eq!(memory_manager.state_for_handle(destination), Some(0));

            native
                .memory
                .write_u32_be(handle_variable, destination)
                .unwrap();
            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HandToHand;
            native.cpu.gpr[3] = handle_variable;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 0);
            let source_copy = native.memory.read_u32_be(handle_variable).unwrap();
            assert_ne!(source_copy, destination);
            let copy = memory_manager.native_allocation(source_copy).unwrap();
            assert_eq!(classic_bus.read_bytes(copy.ptr, 6), b"native");
            assert_eq!(memory_manager.state_for_handle(source_copy), Some(0));

            native.cpu.pc = native.entry_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::HandAndHand;
            native.cpu.gpr[3] = source_copy;
            native.cpu.gpr[4] = destination;
            native.run_with_process_memory_manager(64, false, false, memory_manager);
            assert_eq!(native.cpu.gpr[3], 0);
            let appended = memory_manager.native_allocation(destination).unwrap();
            assert_eq!(appended.size, 12);
            assert_eq!(classic_bus.read_long(destination), appended.ptr);
            assert_eq!(classic_bus.read_bytes(appended.ptr, 12), b"nativenative");
            assert_eq!(
                memory_manager
                    .native_handle_records()
                    .iter()
                    .find(|record| record.handle == destination)
                    .copied(),
                Some(appended)
            );
            assert_eq!(
                memory_manager
                    .native_allocator()
                    .map(|allocator| allocator.heap.last_mem_error),
                Some(PPC_NO_ERR)
            );
        },
    );
}

#[test]
fn ppc_hand_to_hand_copies_a_process_owned_classic_handle_across_isa() {
    let pef = synthetic_pef_with_import(b"HandToHand");
    let mut native = load_pef_application(&pef).unwrap();
    let mut classic = TrapDispatcher::new();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    classic.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    classic_bus.attach_guest_address_space(native.memory.shared_view());

    let payload = b"classic HandToHand source";
    let (source_handle, source_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, payload.len() as u32)
        .unwrap();
    classic_bus.write_bytes(source_ptr, payload);
    let memory_manager = context.memory_manager_handle().clone();
    memory_manager
        .borrow_mut()
        .set_state_for_handle(source_handle, 0xe0);

    let handle_variable = PPC_DATA_BASE + 0x2100;
    native.memory.add_region(handle_variable, vec![0; 4]);
    native
        .memory
        .write_u32_be(handle_variable, source_handle)
        .unwrap();
    let mut detached = native.clone();

    native.cpu.gpr[3] = handle_variable;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::HandToHand,
        &memory_manager,
    );

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let copy_handle = native.memory.read_u32_be(handle_variable).unwrap();
    assert_ne!(copy_handle, source_handle);
    let copy_ptr = classic_bus.read_long(copy_handle);
    assert_ne!(copy_ptr, 0);
    assert_eq!(classic_bus.get_alloc_size(copy_handle), Some(4));
    assert_eq!(
        classic_bus.get_alloc_size(copy_ptr),
        Some(payload.len() as u32)
    );
    assert_eq!(classic_bus.read_bytes(copy_ptr, payload.len()), payload);
    assert_eq!(classic_bus.read_long(source_handle), source_ptr);
    assert_eq!(classic_bus.read_bytes(source_ptr, payload.len()), payload);
    assert_eq!(memory_manager.borrow().native_allocation(copy_handle), None);
    assert_eq!(memory_manager.borrow().handle_for_ptr(copy_ptr), Some(copy_handle));
    assert_eq!(memory_manager.borrow().handle_for_ptr(source_ptr), Some(source_handle));
    assert_eq!(memory_manager.borrow().state_for_handle(copy_handle), Some(0));
    assert_eq!(classic.handle_state_bits(copy_handle), Some(0));
    assert_eq!(memory_manager.borrow().state_for_handle(source_handle), Some(0xe0));

    let mut classic_cpu = crate::trap::test_helpers::MockCpu::new();
    classic_cpu.write_reg(Register::A0, copy_ptr);
    classic
        .dispatch_memory(false, 0x28, &mut classic_cpu, &mut classic_bus)
        .expect("68K RecoverHandle should be handled")
        .expect("68K RecoverHandle should succeed");
    assert_eq!(classic_cpu.read_reg(Register::A0), copy_handle);
    assert_eq!(classic_cpu.read_reg(Register::D0), 0);
    assert_eq!(classic.handle_for_ptr(copy_ptr), Some(copy_handle));

    let detached_manager = detached.process_memory_manager.0.borrow();
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, source_handle),
        Some(source_ptr)
    );
    assert_eq!(
        PpcMemory::read_u32_be(&mut detached.memory, handle_variable),
        Some(source_handle)
    );
    assert_eq!(detached_manager.classic_allocation_size(source_handle), Some(4));
    assert_eq!(
        detached_manager.classic_allocation_size(source_ptr),
        Some(payload.len() as u32)
    );
    assert_eq!(detached_manager.classic_allocation_size(copy_handle), None);
    assert_eq!(detached_manager.classic_allocation_size(copy_ptr), None);
    assert_eq!(detached_manager.native_allocation(copy_handle), None);
    assert_eq!(detached_manager.handle_for_ptr(source_ptr), Some(source_handle));
    assert_eq!(detached_manager.handle_for_ptr(copy_ptr), None);
    assert_eq!(detached_manager.state_for_handle(source_handle), Some(0xe0));
}

#[test]
fn ppc_hand_to_hand_rejects_a_foreign_classic_data_block_atomically() {
    let pef = synthetic_pef_with_import(b"HandToHand");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);

    let (source_handle, source_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 16)
        .unwrap();
    let (other_handle, other_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, 24)
        .unwrap();
    let source_bytes = (0x20..=0x2f).collect::<Vec<_>>();
    let other_bytes = (0x80..=0x97).collect::<Vec<_>>();
    classic_bus.write_bytes(source_ptr, &source_bytes);
    classic_bus.write_bytes(other_ptr, &other_bytes);
    classic_bus.write_long(source_handle, other_ptr);

    let handle_variable = PPC_DATA_BASE + 0x2100;
    native.memory.add_region(handle_variable, vec![0; 4]);
    native
        .memory
        .write_u32_be(handle_variable, source_handle)
        .unwrap();
    let memory_manager = context.memory_manager_handle().clone();
    let allocator_before = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator is attached");
    let handles_before = memory_manager.borrow().native_handle_records().to_vec();
    let classic_cursor_before = context.classic_heap_bump_ptr();

    native.cpu.gpr[3] = handle_variable;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::HandToHand,
        &memory_manager,
    );

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_MEM_WZ_ERR));
    assert_eq!(native.memory.read_u32_be(handle_variable), Some(source_handle));
    assert_eq!(classic_bus.read_long(source_handle), other_ptr);
    assert_eq!(classic_bus.read_long(other_handle), other_ptr);
    assert_eq!(classic_bus.get_alloc_size(source_ptr), Some(16));
    assert_eq!(classic_bus.get_alloc_size(other_ptr), Some(24));
    assert_eq!(classic_bus.read_bytes(source_ptr, 16), source_bytes);
    assert_eq!(classic_bus.read_bytes(other_ptr, 24), other_bytes);
    assert_eq!(context.classic_heap_bump_ptr(), classic_cursor_before);
    assert_eq!(memory_manager.borrow().handle_for_ptr(source_ptr), Some(source_handle));
    assert_eq!(memory_manager.borrow().handle_for_ptr(other_ptr), Some(other_handle));
    assert_eq!(memory_manager.borrow().native_handle_records(), handles_before);
    let allocator_after = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator remains attached");
    assert_eq!(allocator_after.heap.heap_cursor, allocator_before.heap.heap_cursor);
    assert_eq!(allocator_after.ptrs, allocator_before.ptrs);
    assert_eq!(allocator_after.free_ptr_blocks, allocator_before.free_ptr_blocks);
    assert_eq!(
        allocator_after.free_handle_blocks,
        allocator_before.free_handle_blocks
    );
    assert_eq!(allocator_after.heap.last_mem_error, PPC_MEM_WZ_ERR);
}

#[test]
fn ppc_hand_to_hand_rejects_a_disposed_classic_source_atomically() {
    let pef = synthetic_pef_with_import(b"HandToHand");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let mut classic_bus =
        attach_test_classic_heap(&mut native, &mut context, 8 * 1024 * 1024, 0x4000);
    let payload = b"disposed classic source";
    let (source_handle, source_ptr) = context
        .memory_manager_mut()
        .new_classic_handle(&mut classic_bus, payload.len() as u32)
        .unwrap();
    classic_bus.write_bytes(source_ptr, payload);
    let memory_manager = context.memory_manager_handle().clone();
    assert!(memory_manager
        .borrow_mut()
        .dispose_process_handle_from_native_import(&mut native.memory, source_handle));

    let stale_master = classic_bus.read_long(source_handle);
    let stale_bytes = classic_bus.read_bytes(source_ptr, payload.len());
    let handle_variable = PPC_DATA_BASE + 0x2100;
    native.memory.add_region(handle_variable, vec![0; 4]);
    native
        .memory
        .write_u32_be(handle_variable, source_handle)
        .unwrap();
    let classic_cursor_before = context.classic_heap_bump_ptr();
    let allocator_before = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator is attached");
    let handles_before = memory_manager.borrow().native_handle_records().to_vec();
    let reverse_before = memory_manager.borrow().handle_for_ptr(source_ptr);

    native.cpu.gpr[3] = handle_variable;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::HandToHand,
        &memory_manager,
    );

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NIL_HANDLE_ERR));
    assert_eq!(native.memory.read_u32_be(handle_variable), Some(source_handle));
    assert_eq!(classic_bus.read_long(source_handle), stale_master);
    assert_eq!(classic_bus.read_bytes(source_ptr, payload.len()), stale_bytes);
    assert_eq!(context.classic_heap_bump_ptr(), classic_cursor_before);
    assert_eq!(memory_manager.borrow().classic_allocation_size(source_handle), None);
    assert_eq!(memory_manager.borrow().classic_allocation_size(source_ptr), None);
    assert_eq!(memory_manager.borrow().handle_for_ptr(source_ptr), reverse_before);
    assert_eq!(memory_manager.borrow().state_for_handle(source_handle), None);
    assert_eq!(memory_manager.borrow().native_handle_records(), handles_before);
    let allocator_after = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator remains attached");
    assert_eq!(allocator_after.heap.heap_cursor, allocator_before.heap.heap_cursor);
    assert_eq!(allocator_after.ptrs, allocator_before.ptrs);
    assert_eq!(allocator_after.free_ptr_blocks, allocator_before.free_ptr_blocks);
    assert_eq!(
        allocator_after.free_handle_blocks,
        allocator_before.free_handle_blocks
    );
    assert_eq!(allocator_after.heap.last_mem_error, PPC_NIL_HANDLE_ERR);
}

#[test]
fn ppc_hand_to_hand_rejects_a_readonly_handle_variable_before_allocating() {
    let pef = synthetic_pef_with_import(b"HandToHand");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let memory_manager = context.memory_manager_handle().clone();
    let source_handle = memory_manager
        .borrow_mut()
        .copy_bytes_to_new_native_handle(&mut native.memory, b"source");
    assert_ne!(source_handle, 0);

    let handle_variable = PPC_DATA_BASE + 0x2100;
    native
        .memory
        .add_readonly_region(handle_variable, source_handle.to_be_bytes().to_vec());
    let allocator_before = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator is attached");
    let handles_before = memory_manager.borrow().native_handle_records().to_vec();

    native.cpu.gpr[3] = handle_variable;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::HandToHand,
        &memory_manager,
    );

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(native.memory.read_u32_be(handle_variable), Some(source_handle));
    assert_eq!(memory_manager.borrow().native_handle_records(), handles_before);
    let allocator_after = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator remains attached");
    assert_eq!(allocator_after.heap.heap_cursor, allocator_before.heap.heap_cursor);
    assert_eq!(allocator_after.ptrs, allocator_before.ptrs);
    assert_eq!(allocator_after.free_ptr_blocks, allocator_before.free_ptr_blocks);
    assert_eq!(
        allocator_after.free_handle_blocks,
        allocator_before.free_handle_blocks
    );
    assert_eq!(allocator_after.heap.last_mem_error, PPC_PARAM_ERR);
}

#[test]
fn ppc_hand_to_hand_preserves_the_toolbox_color_table_fallback() {
    let pef = synthetic_pef_with_import(b"HandToHand");
    let mut native = load_pef_application(&pef).unwrap();
    ppc_seed_main_color_table(&mut native.memory).unwrap();
    let expected = ppc_memory_read_bytes(
        &mut native.memory,
        PPC_MAIN_CTABLE,
        PPC_MAIN_CTABLE_SIZE,
    )
    .unwrap();
    let handle_variable = PPC_DATA_BASE + 0x2100;
    native.memory.add_region(handle_variable, vec![0; 4]);
    native
        .memory
        .write_u32_be(handle_variable, PPC_MAIN_CTABLE_HANDLE)
        .unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let memory_manager = context.memory_manager_handle().clone();

    native.cpu.gpr[3] = handle_variable;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::HandToHand,
        &memory_manager,
    );

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let copy_handle = native.memory.read_u32_be(handle_variable).unwrap();
    assert_ne!(copy_handle, PPC_MAIN_CTABLE_HANDLE);
    let copy = memory_manager
        .borrow()
        .native_allocation(copy_handle)
        .expect("Toolbox-owned data should retain the native fallback");
    assert_eq!(copy.size, PPC_MAIN_CTABLE_SIZE);
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, copy.ptr, copy.size),
        Some(expected)
    );
    assert_eq!(memory_manager.borrow().state_for_handle(copy_handle), Some(0));
}

#[test]
fn ppc_hand_to_hand_color_table_heap_failure_is_atomic() {
    let pef = synthetic_pef_with_import(b"HandToHand");
    let mut native = load_pef_application(&pef).unwrap();
    ppc_seed_main_color_table(&mut native.memory).unwrap();
    let handle_variable = PPC_DATA_BASE + 0x2100;
    native.memory.add_region(handle_variable, vec![0; 4]);
    native
        .memory
        .write_u32_be(handle_variable, PPC_MAIN_CTABLE_HANDLE)
        .unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let memory_manager = context.memory_manager_handle().clone();
    let allocator_before = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator is attached");
    memory_manager
        .borrow_mut()
        .set_native_heap_limit(allocator_before.heap.heap_cursor + PPC_HEAP_ALIGNMENT);

    native.cpu.gpr[3] = handle_variable;
    run_test_import_with_process_memory_manager(
        &mut native,
        PpcImportDispatcherTarget::HandToHand,
        &memory_manager,
    );

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(
        native.memory.read_u32_be(handle_variable),
        Some(PPC_MAIN_CTABLE_HANDLE)
    );
    assert!(memory_manager.borrow().native_handle_records().is_empty());
    let allocator_after = memory_manager
        .borrow()
        .native_allocator_snapshot()
        .expect("native allocator remains attached");
    assert_eq!(allocator_after.heap.heap_cursor, allocator_before.heap.heap_cursor);
    assert_eq!(allocator_after.ptrs, allocator_before.ptrs);
    assert_eq!(allocator_after.free_ptr_blocks, allocator_before.free_ptr_blocks);
    assert_eq!(
        allocator_after.free_handle_blocks,
        allocator_before.free_handle_blocks
    );
    assert_eq!(allocator_after.heap.last_mem_error, PPC_MEM_FULL_ERR);
}

#[test]
fn hle_import_runner_handles_compact_mem_as_heap_free_query() {
    let pef = synthetic_pef_with_import(b"CompactMem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let free = test_heap_limit!(loaded) - loaded.heap_cursor();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
}

#[test]
fn hle_import_runner_handles_purge_mem_as_heap_free_probe() {
    let pef = synthetic_pef_with_import(b"PurgeMem");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    let free = test_heap_limit!(loaded) - loaded.heap_cursor();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = free;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = free + 1;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free + 1);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::PurgeMemSys;
    loaded.cpu.gpr[3] = free;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn hle_import_runner_handles_mem_error() {
    let pef = synthetic_pef_with_import(b"MemError");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
}

#[test]
fn hle_import_runner_handles_block_move_with_overlap() {
    let pef = synthetic_pef_with_import(b"BlockMove");
    let mut loaded = load_pef_application(&pef).unwrap();
    let buffer_ptr = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(buffer_ptr, b"abcdefgh".to_vec());
    loaded.cpu.gpr[3] = buffer_ptr;
    loaded.cpu.gpr[4] = buffer_ptr + 2;
    loaded.cpu.gpr[5] = 6;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], buffer_ptr);
    for (offset, byte) in b"ababcdef".iter().copied().enumerate() {
        assert_eq!(
            loaded.memory.read_u8(buffer_ptr + offset as u32),
            Some(byte)
        );
    }
}

#[test]
fn hle_import_runner_handles_block_move_data_alias() {
    let pef = synthetic_pef_with_import(b"BlockMoveData");
    let mut loaded = load_pef_application(&pef).unwrap();
    let source_ptr = PPC_DATA_BASE + 0x1000;
    let dest_ptr = PPC_DATA_BASE + 0x1100;
    loaded.memory.add_region(source_ptr, b"Gridz".to_vec());
    loaded.memory.add_region(dest_ptr, vec![0; 5]);
    loaded.cpu.gpr[3] = source_ptr;
    loaded.cpu.gpr[4] = dest_ptr;
    loaded.cpu.gpr[5] = 5;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    for (offset, byte) in b"Gridz".iter().copied().enumerate() {
        assert_eq!(loaded.memory.read_u8(dest_ptr + offset as u32), Some(byte));
    }
}

#[test]
fn hle_import_runner_handles_handle_size_queries() {
    let pef = synthetic_pef_with_import(b"GetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    test_handles!(loaded).push(PpcHandleRecord {
        handle: PPC_HEAP_BASE,
        ptr: PPC_HEAP_BASE + 4,
        size: 123,
        capacity: 123,
    });
    loaded
        .memory
        .write_u32_be(PPC_HEAP_BASE, PPC_HEAP_BASE + 4)
        .unwrap();
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 123);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    let pef = synthetic_pef_with_import(b"GetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_MAIN_CTABLE_HANDLE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_MAIN_CTABLE_SIZE);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"abcdefghijkl",
    );
    assert_ne!(handle, 0);
    let old_ptr = loaded.memory.read_u32_be(handle).unwrap();
    let old_heap_cursor = loaded.heap_cursor();
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 48;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(test_handle_records!(loaded)[0].size, 48);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let new_ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_eq!(new_ptr, old_ptr);
    assert_eq!(test_handle_records!(loaded)[0].ptr, old_ptr);
    assert_eq!(
        loaded.heap_cursor(),
        old_heap_cursor + ppc_allocation_size(48).unwrap() - ppc_allocation_size(12).unwrap()
    );
    for (offset, byte) in b"abcdefghijkl".iter().copied().enumerate() {
        assert_eq!(loaded.memory.read_u8(new_ptr + offset as u32), Some(byte));
    }

    let pef = synthetic_pef_with_import(b"SetHandleSize");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"abcdefghijkl",
    );
    assert_ne!(handle, 0);
    let old_ptr = loaded.memory.read_u32_be(handle).unwrap();
    let blocker = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        16,
        true,
    );
    assert_ne!(blocker, 0);
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 48;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(test_handle_records!(loaded)[0].size, 48);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    let new_ptr = loaded.memory.read_u32_be(handle).unwrap();
    assert_ne!(new_ptr, old_ptr);
    assert_eq!(test_handle_records!(loaded)[0].ptr, new_ptr);
    assert!(handle < new_ptr || handle >= new_ptr + 48);
    for (offset, byte) in b"abcdefghijkl".iter().copied().enumerate() {
        assert_eq!(loaded.memory.read_u8(new_ptr + offset as u32), Some(byte));
    }
}

#[test]
fn hle_import_runner_dispose_handle_invalidates_tracked_handle() {
    let pef = synthetic_pef_with_import(b"DisposeHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"data",
    );
    assert_ne!(handle, 0);
    let heap_cursor = loaded.heap_cursor();
    loaded.aliases.push(PpcAliasRecord {
        handle,
        target_vref: PPC_BOOT_VOLUME_REF_NUM,
        target_dir_id: PPC_ROOT_DIR_ID,
        target_name: b"Target".to_vec(),
    });
    let current_resource_refnum = *loaded.process_file_system.current_resource_file;
    loaded.process_file_system.push_vfs_resource(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: "Test App".to_string(),
        res_type: u32::from_be_bytes(*b"PICT"),
        res_id: 128,
        name: b"Title".to_vec(),
        data: b"data".to_vec(),
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle,
    });

    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(test_handle_records!(loaded)
        .iter()
        .all(|record| record.handle != handle));
    assert_eq!(loaded.memory.read_u32_be(handle), Some(0));
    assert!(loaded.aliases.is_empty());
    assert_eq!(loaded.process_file_system.vfs_resources[0].handle, 0);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetHandleSize;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.last_mem_error(), PPC_NIL_HANDLE_ERR);
}

#[test]
fn hle_import_runner_tracks_handle_lock_and_no_purge_state() {
    let pef = synthetic_pef_with_import(b"HLock");
    let mut loaded = load_pef_application(&pef).unwrap();
    let handle = ppc_alloc_handle_with_bytes(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        test_handles!(loaded),
        b"lockable",
    );
    assert_ne!(handle, 0);
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: false,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HNoPurge;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HLockHi;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: true,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HUnlock;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: false,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HLock;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HPurge;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: false,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HNoPurge;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::MoveHHi;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: true,
            high_locked: false,
            no_purge: true,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HGetState;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x80);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::HSetState;
    loaded.cpu.gpr[3] = handle;
    loaded.cpu.gpr[4] = 0x40;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        loaded.handle_states(),
        vec![PpcHandleStateRecord {
            handle,
            locked: false,
            high_locked: false,
            no_purge: false,
            resource: false,
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x8000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.handle_states().len(), 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
    loaded.cpu.gpr[3] = handle;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.handle_states().is_empty());
    assert!(test_handle_records!(loaded)
        .iter()
        .all(|record| record.handle != handle));
    assert_eq!(loaded.memory.read_u32_be(handle), Some(0));
}

#[test]
fn hle_import_runner_dispose_handle_tolerates_unknown_handle() {
    let pef = synthetic_pef_with_import(b"DisposeHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = PPC_HEAP_BASE + 0x4000;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE + 0x4000);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(test_handle_records!(loaded).is_empty());
}

#[test]
fn hle_import_runner_handles_new_handle_clear_allocation() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.cpu.gpr[3] = 12;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(test_handle_records!(loaded).len(), 1);
    assert_eq!(test_handle_records!(loaded)[0].handle, handle);
    assert_eq!(handle, heap_cursor);
    assert_eq!(handle & (PPC_HEAP_ALIGNMENT - 1), 0);
    assert_eq!(
        test_handle_records!(loaded)[0].ptr,
        heap_cursor + PPC_HEAP_ALIGNMENT
    );
    assert_eq!(
        test_handle_records!(loaded)[0].ptr & (PPC_HEAP_ALIGNMENT - 1),
        0
    );
    assert_eq!(test_handle_records!(loaded)[0].size, 12);
    assert_eq!(
        loaded.memory.read_u32_be(handle),
        Some(heap_cursor + PPC_HEAP_ALIGNMENT)
    );
    assert_eq!(
        loaded.heap_cursor(),
        heap_cursor + ppc_allocation_size(4).unwrap() + ppc_allocation_size(12).unwrap()
    );
    for offset in 0..12 {
        assert_eq!(
            loaded
                .memory
                .read_u8(heap_cursor + PPC_HEAP_ALIGNMENT + offset),
            Some(0)
        );
    }
}

#[test]
fn new_handle_abi_marshalling_shares_semantics_without_sharing_addresses() {
    fn classic_outcome(
        trap_word: u16,
        clear: bool,
        size: u32,
    ) -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let (mut dispatcher, mut cpu, mut bus) = crate::trap::test_helpers::setup();
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::D0, size);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .expect("classic NewHandle should be handled")
            .expect("classic NewHandle should return cleanly");

        let handle = cpu.read_reg(Register::A0);
        let error = cpu.read_reg(Register::D0) as i16;
        if handle == 0 {
            return (false, error, None, None, None);
        }

        let state = dispatcher.handle_state_bits(handle);
        let ptr = bus.read_long(handle);
        let memory_manager = dispatcher.process_memory_manager();
        let memory_manager = memory_manager.borrow();
        let allocation_size = memory_manager.classic_allocation_size(ptr);
        let contents_zero = clear.then(|| {
            bus.read_bytes(ptr, allocation_size.unwrap_or(0) as usize)
                .iter()
                .all(|&byte| byte == 0)
        });
        (true, error, state, allocation_size, contents_zero)
    }

    fn native_outcome(
        clear: bool,
        size: u32,
    ) -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let import = if clear {
            b"NewHandleClear".as_slice()
        } else {
            b"NewHandle".as_slice()
        };
        let pef = synthetic_pef_with_import(import);
        let mut loaded = load_pef_application(&pef).unwrap();
        loaded.cpu.gpr[3] = size;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::NewHandle { clear },
        );

        let handle = loaded.cpu.gpr[3];
        let error = loaded.last_mem_error();
        let (state, record) = {
            let memory_manager = loaded.process_memory_manager.0.borrow();
            (
                memory_manager.state_for_handle(handle),
                memory_manager.native_allocation(handle),
            )
        };
        let contents_zero = if clear {
            record.map(|record| {
                (0..record.size)
                    .all(|offset| loaded.memory.read_u8(record.ptr + offset) == Some(0))
            })
        } else {
            // Ordinary NewHandle contents are undefined. Do not compare
            // whatever byte pattern a backend happens to leave there.
            None
        };
        (
            handle != 0,
            error,
            state,
            record.map(|record| record.size),
            contents_zero,
        )
    }

    // The four classic trap variants (current/system × clear/non-clear)
    // must reach the same semantic operation as the native InterfaceLib
    // entry points. Handles and data pointers are intentionally omitted
    // from the compared outcome because each allocator owns its physical
    // address, alignment, and layout.
    for (trap_word, clear) in [
        (0xA022, false),
        (0xA322, true),
        (0xA422, false),
        (0xA622, true),
    ] {
        let classic = classic_outcome(trap_word, clear, 13);
        let native = native_outcome(clear, 13);
        assert_eq!(classic, native, "trap ${trap_word:04X} semantic outcome");
        assert_eq!(
            classic,
            (true, 0, Some(0), Some(13), clear.then_some(true))
        );
    }

    // Macintosh Size is signed. Both ABI adapters reject the same
    // negative value before entering an unsigned physical allocator.
    let classic = classic_outcome(0xA022, false, u32::MAX);
    let native = native_outcome(false, u32::MAX);
    assert_eq!(classic, native);
    assert_eq!(classic, (false, -108, None, None, None));

    fn classic_mem_full_outcome() -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let (mut dispatcher, mut cpu, mut bus) = crate::trap::test_helpers::setup();
        let heap_limit = bus.classic_heap_limit();
        bus.reserve_heap_until(heap_limit);
        dispatcher.current_trap_word = 0xA022;
        cpu.write_reg(Register::D0, 24);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .expect("classic NewHandle should be handled")
            .expect("classic NewHandle failure should stay in the ABI shim");
        (
            cpu.read_reg(Register::A0) != 0,
            cpu.read_reg(Register::D0) as i16,
            dispatcher.handle_state_bits(cpu.read_reg(Register::A0)),
            None,
            None,
        )
    }

    fn native_mem_full_outcome() -> (bool, i16, Option<u8>, Option<u32>, Option<bool>) {
        let pef = synthetic_pef_with_import(b"NewHandle");
        let mut loaded = load_pef_application(&pef).unwrap();
        let heap_cursor = loaded.heap_cursor();
        loaded.set_heap_limit(heap_cursor + 8);
        loaded.cpu.gpr[3] = 24;
        run_test_import(
            &mut loaded,
            PpcImportDispatcherTarget::NewHandle { clear: false },
        );
        let handle = loaded.cpu.gpr[3];
        let state = loaded
            .process_memory_manager
            .0
            .borrow()
            .state_for_handle(handle);
        (
            handle != 0,
            loaded.last_mem_error(),
            state,
            None,
            None,
        )
    }

    let classic = classic_mem_full_outcome();
    let native = native_mem_full_outcome();
    assert_eq!(classic, native);
    assert_eq!(classic, (false, -108, None, None, None));

    // TempNewHandle has a result-code pointer and temporary lifetime, so
    // it remains a distinct ABI route rather than silently inheriting the
    // ordinary NewHandle request/result operation above.
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TempNewHandle"),
        PpcImportDispatcherTarget::TempNewHandle
    );
}

#[test]
fn hle_import_runner_reuses_disposed_handle_capacity() {
    let pef = synthetic_pef_with_import(b"NewHandleClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 64;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    let handle = loaded.cpu.gpr[3];
    let ptr = loaded.memory.read_u32_be(handle).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.memory.write_u8(ptr, 0xff).unwrap();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposeHandle;
    loaded.cpu.gpr[3] = handle;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert!(test_handle_records!(loaded).is_empty());
    assert_eq!(loaded.free_handle_blocks().len(), 1);
    assert_eq!(loaded.free_handle_blocks()[0].capacity, 64);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewHandle { clear: true };
    loaded.cpu.gpr[3] = 16;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], handle);
    assert_eq!(loaded.memory.read_u32_be(handle), Some(ptr));
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(test_handle_records!(loaded)[0].size, 16);
    assert_eq!(test_handle_records!(loaded)[0].capacity, 64);
    assert!(loaded.free_handle_blocks().is_empty());
    for offset in 0..16 {
        assert_eq!(loaded.memory.read_u8(ptr + offset), Some(0));
    }
}

#[test]
fn hle_import_runner_handles_temp_new_handle_result_code() {
    let pef = synthetic_pef_with_import(b"TempNewHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let result_code_ptr = PPC_DATA_BASE;
    loaded.memory.add_region(result_code_ptr, vec![0xff; 4]);
    loaded.cpu.gpr[3] = 5;
    loaded.cpu.gpr[4] = result_code_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    let handle = loaded.cpu.gpr[3];
    assert_ne!(handle, 0);
    assert_eq!(
        loaded.memory.read_u16_be(result_code_ptr),
        Some(PPC_NO_ERR as u16)
    );
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
    assert_eq!(test_handle_records!(loaded).len(), 1);
    assert_eq!(test_handle_records!(loaded)[0].handle, handle);
    assert_eq!(test_handle_records!(loaded)[0].size, 5);
}

#[test]
fn hle_import_runner_temp_new_handle_heap_full_reports_result_code() {
    let pef = synthetic_pef_with_import(b"TempNewHandle");
    let mut loaded = load_pef_application(&pef).unwrap();
    let result_code_ptr = PPC_DATA_BASE;
    let heap_cursor = loaded.heap_cursor();
    loaded.memory.add_region(result_code_ptr, vec![0xff; 4]);
    loaded.set_heap_limit(heap_cursor + 8);
    loaded.cpu.gpr[3] = 4;
    loaded.cpu.gpr[4] = result_code_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded.memory.read_u16_be(result_code_ptr),
        Some(PPC_MEM_FULL_ERR as u16)
    );
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(test_handle_records!(loaded).is_empty());
}

#[test]
fn hle_import_runner_handles_max_mem_and_writes_zero_grow() {
    let pef = synthetic_pef_with_import(b"MaxMem");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded
        .memory
        .write_u32_be(PPC_DATA_BASE, 0xffff_ffff)
        .unwrap();
    let free = test_heap_limit!(loaded) - loaded.heap_cursor();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], free);
    assert_eq!(loaded.memory.read_u32_be(PPC_DATA_BASE), Some(0));
}

#[test]
fn hle_import_runner_handles_zone_queries() {
    let pef = synthetic_pef_with_import(b"GetZone");
    let mut loaded = load_pef_application(&pef).unwrap();

    for (target, expected) in [
        (PpcImportDispatcherTarget::GetZone, PPC_APPLICATION_ZONE),
        (PpcImportDispatcherTarget::SystemZone, PPC_SYSTEM_ZONE),
        (
            PpcImportDispatcherTarget::ApplicationZone,
            PPC_APPLICATION_ZONE,
        ),
    ] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.imports[0].dispatcher_target = target;

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], expected);
    }

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::SetZone;
    loaded.cpu.gpr[3] = 0x1234_5678;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetZone;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0x1234_5678);
}

#[test]
fn native_powerpc_launch_seeds_modern_application_zone() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"GetZone")).unwrap();

    assert_eq!(
        loaded.memory.read_u32_be(PPC_THE_ZONE_ADDR),
        Some(PPC_APPLICATION_ZONE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPL_ZONE_ADDR),
        Some(PPC_APPLICATION_ZONE)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_SYS_ZONE_ADDR),
        Some(PPC_SYSTEM_ZONE)
    );
    assert_eq!(
        loaded
            .memory
            .read_u8(PPC_APPLICATION_ZONE + PPC_ZONE_HEAP_TYPE_OFFSET),
        Some(PPC_ZONE_32_BIT_HEAP | PPC_ZONE_NEW_STYLE_HEAP)
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPLICATION_ZONE),
        Some(test_heap_limit!(loaded))
    );
    assert_eq!(
        loaded.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(test_heap_limit!(loaded) - loaded.heap_base())
    );
}

#[test]
fn hle_import_runner_init_zone_initializes_header_and_current_zone() {
    let pef = synthetic_pef_with_import(b"InitZone");
    let mut loaded = load_pef_application(&pef).unwrap();
    let start = 0x0308_0000;
    let limit = start + 0x400;
    loaded.memory.add_region(start, vec![0xaa; 0x400]);
    loaded.cpu.gpr[3] = 0x0012_3456;
    loaded.cpu.gpr[4] = 4;
    loaded.cpu.gpr[5] = limit;
    loaded.cpu.gpr[6] = start;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(start), Some(limit));
    assert_eq!(loaded.memory.read_u32_be(start + 4), Some(0));
    assert_eq!(loaded.memory.read_u32_be(start + 8), Some(start + 60));
    assert_eq!(loaded.memory.read_u32_be(start + 12), Some(0x3a8));
    assert_eq!(loaded.memory.read_u32_be(start + 16), Some(0x0012_3456));
    assert_eq!(loaded.memory.read_u16_be(start + 20), Some(4));
    assert_eq!(loaded.memory.read_u32_be(start + 36), Some(0x3a8));
    assert_eq!(loaded.memory.read_u32_be(start + 48), Some(start + 52));
    assert_eq!(loaded.memory.read_u32_be(PPC_THE_ZONE_ADDR), Some(start));
}

#[test]
fn hle_import_runner_handles_max_appl_zone_as_successful_noop() {
    let pef = synthetic_pef_with_import(b"MaxApplZone");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = 0xfeed_face;

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
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(loaded.heap_cursor(), PPC_HEAP_BASE);
    assert!(loaded.heap_maximized());
    assert_eq!(loaded.master_pointer_blocks_requested(), 0);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn native_import_application_limit_is_distinct_from_heap_ceiling() {
    let pef = synthetic_pef_with_import(b"SetApplLimit");
    let mut loaded = load_pef_application(&pef).unwrap();
    let native_heap_limit = loaded.heap_limit();
    let requested = loaded.heap_cursor() + 0x1000;
    assert!(requested < native_heap_limit);

    loaded.cpu.gpr[3] = requested;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    assert_eq!(loaded.heap_limit(), native_heap_limit);
    assert_eq!(loaded.application_heap_limit(), requested);

    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::GetApplLimit);
    assert_eq!(loaded.cpu.gpr[3], requested);
}

#[test]
fn native_import_allocations_honor_application_limit_within_physical_heap() {
    // Inside Macintosh: Memory (1992), pp. 2-42--2-44 and 2-83--2-85:
    // NewPtr must stay below the current application boundary, while
    // SetApplLimit may raise that boundary without changing the native
    // heap's physical mapping ceiling.
    let pef = synthetic_pef_with_import(b"NewPtr");
    let mut loaded = load_pef_application(&pef).unwrap();
    let initial_cursor = loaded.heap_cursor();
    let native_heap_ceiling = loaded.heap_limit();
    let lowered_limit = initial_cursor + (2 * PPC_HEAP_ALIGNMENT);
    assert!(lowered_limit < native_heap_ceiling);

    loaded.cpu.gpr[3] = lowered_limit;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    assert_eq!(loaded.application_heap_limit(), lowered_limit);
    assert_eq!(loaded.heap_limit(), native_heap_ceiling);

    loaded.cpu.gpr[3] = PPC_HEAP_ALIGNMENT;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    let first_ptr = loaded.cpu.gpr[3];
    assert_eq!(first_ptr, initial_cursor);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);

    // A second aligned allocation would cross the lowered boundary. The
    // failed import is transactional: the cursor and pointer records stay
    // unchanged while MemError reports memFullErr.
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
    assert_eq!(loaded.ptrs().len(), 1);

    // Raising ApplLimit re-enables the remaining physical heap space; the
    // native ceiling itself never changed during the logical update.
    loaded.cpu.gpr[3] = native_heap_ceiling;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::SetApplLimit);
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], initial_cursor + PPC_HEAP_ALIGNMENT);
    assert_eq!(loaded.heap_limit(), native_heap_ceiling);
}

#[test]
fn native_import_reusable_blocks_honor_application_limit() {
    // Inside Macintosh: Memory (1992), pp. 2-42--2-44: a disposed fixed
    // block can satisfy a later NewPtr, but the application limit still
    // bounds every address that the Memory Manager may reuse.
    let pef = synthetic_pef_with_import(b"NewPtr");
    let mut loaded = load_pef_application(&pef).unwrap();
    let initial_cursor = loaded.heap_cursor();
    let native_heap_ceiling = loaded.heap_limit();
    let lowered_limit = initial_cursor + (2 * PPC_HEAP_ALIGNMENT);
    let deferred_block = initial_cursor + (4 * PPC_HEAP_ALIGNMENT);

    loaded.with_process_memory_manager(|_, manager| {
        manager.set_application_heap_limit(lowered_limit);
        manager.mutate_native_allocator(|allocator| {
            allocator.free_ptr_blocks.push(ProcessPtrRecord {
                ptr: deferred_block,
                size: PPC_HEAP_ALIGNMENT,
            });
        });
    });

    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], initial_cursor);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);

    loaded.cpu.gpr[3] = 0;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::MaxMem);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_ALIGNMENT);

    // The same reusable block becomes eligible when the logical boundary
    // is raised; the physical native ceiling remains unchanged.
    loaded.with_process_memory_manager(|_, manager| {
        manager.set_application_heap_limit(native_heap_ceiling);
    });
    loaded.cpu.gpr[3] = 1;
    run_test_import(&mut loaded, PpcImportDispatcherTarget::NewPtr { clear: true });
    assert_eq!(loaded.cpu.gpr[3], deferred_block);
    assert_eq!(loaded.heap_cursor(), initial_cursor + PPC_HEAP_ALIGNMENT);
    assert_eq!(loaded.heap_limit(), native_heap_ceiling);
}

#[test]
fn hle_import_runner_tracks_more_masters_requests() {
    let pef = synthetic_pef_with_import(b"MoreMasters");
    let mut loaded = load_pef_application(&pef).unwrap();
    let heap_cursor = loaded.heap_cursor();
    loaded.set_last_mem_error(PPC_MEM_FULL_ERR);
    loaded.cpu.gpr[3] = 0xfeed_face;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xfeed_face);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(!loaded.heap_maximized());
    assert_eq!(loaded.master_pointer_blocks_requested(), 1);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 0xface_feed;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0xface_feed);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert_eq!(loaded.master_pointer_blocks_requested(), 2);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn hand_to_hand_duplicates_tracked_handle_storage() {
    let heap_base = 0x3000;
    let handle_variable = 0x3f00;
    let mut memory = PpcSectionMem::new();
    memory.add_region(heap_base, vec![0; 0x1000]);
    let mut heap_cursor = heap_base;
    let mut handles = Vec::new();
    let mut free_handles = Vec::new();
    let source = ppc_alloc_handle_with_bytes(
        &mut memory,
        &mut heap_cursor,
        heap_base + 0x1000,
        &mut handles,
        b"copy me",
    );
    memory.write_u32_be(handle_variable, source).unwrap();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = handle_variable;

    assert_eq!(
        ppc_hand_to_hand(
            &cpu,
            &mut memory,
            &mut heap_cursor,
            heap_base + 0x1000,
            &mut handles,
            &mut free_handles,
        ),
        PPC_NO_ERR
    );
    let copy = memory.read_u32_be(handle_variable).unwrap();
    assert_ne!(copy, source);
    let copy_data = memory.read_u32_be(copy).unwrap();
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, copy_data, 7),
        Some(b"copy me".to_vec())
    );
}

#[test]
fn hand_to_hand_duplicates_main_device_color_table() {
    let heap_base = 0x0300_0000;
    let heap_limit = heap_base + 0x4000;
    let handle_variable = heap_base + 0x3f00;
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_MAIN_CTABLE_HANDLE, vec![0; 4]);
    memory.add_region(PPC_MAIN_CTABLE, vec![0; PPC_MAIN_CTABLE_SIZE as usize]);
    memory.add_region(heap_base, vec![0; (heap_limit - heap_base) as usize]);
    ppc_seed_main_color_table(&mut memory).unwrap();
    memory
        .write_u32_be(handle_variable, PPC_MAIN_CTABLE_HANDLE)
        .unwrap();
    let mut heap_cursor = heap_base;
    let mut handles = Vec::new();
    let mut free_handles = Vec::new();
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = handle_variable;

    assert_eq!(
        ppc_hand_to_hand(
            &cpu,
            &mut memory,
            &mut heap_cursor,
            heap_limit,
            &mut handles,
            &mut free_handles,
        ),
        PPC_NO_ERR
    );
    let copy = memory.read_u32_be(handle_variable).unwrap();
    let copy_data = memory.read_u32_be(copy).unwrap();
    assert_ne!(copy, PPC_MAIN_CTABLE_HANDLE);
    assert_eq!(handles[0].size, PPC_MAIN_CTABLE_SIZE);
    assert_eq!(
        ppc_memory_read_bytes(&mut memory, copy_data, PPC_MAIN_CTABLE_SIZE),
        ppc_memory_read_bytes(&mut memory, PPC_MAIN_CTABLE, PPC_MAIN_CTABLE_SIZE)
    );
}

#[test]
fn legacy_memory_utility_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        ("BitClr", PpcLegacyMemoryUtilityOperation::BitClear),
        ("BitNot", PpcLegacyMemoryUtilityOperation::BitNot),
        ("BitSet", PpcLegacyMemoryUtilityOperation::BitSet),
        ("Fix2X", PpcLegacyMemoryUtilityOperation::FixToExtended),
        ("GetMyZone", PpcLegacyMemoryUtilityOperation::GetMyZone),
        ("HandleZone", PpcLegacyMemoryUtilityOperation::HandleZone),
        ("LockMemory", PpcLegacyMemoryUtilityOperation::LockMemory),
        ("MaxBlock", PpcLegacyMemoryUtilityOperation::MaxBlock),
        ("PurgeSpace", PpcLegacyMemoryUtilityOperation::PurgeSpace),
        ("ReserveMem", PpcLegacyMemoryUtilityOperation::ReserveMem),
        ("SetGrowZone", PpcLegacyMemoryUtilityOperation::SetGrowZone),
        ("StackSpace", PpcLegacyMemoryUtilityOperation::StackSpace),
        ("TempFreeMem", PpcLegacyMemoryUtilityOperation::TempFreeMem),
        ("UnlockMemory", PpcLegacyMemoryUtilityOperation::UnlockMemory),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::LegacyMemoryUtility(operation),
            "{symbol}"
        );
    }
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetPtrSize"),
        PpcImportDispatcherTarget::SetPtrSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "RecoverHandle"),
        PpcImportDispatcherTarget::RecoverHandle
    );
}

#[test]
fn reserve_mem_reports_whether_a_contiguous_block_is_available() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"ReserveMem")).unwrap();
    loaded.cpu.gpr[3] = 0;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = u32::MAX;
    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.last_mem_error(), PPC_MEM_FULL_ERR);
}

#[test]
fn set_ptr_size_grows_only_a_terminal_nonrelocatable_block() {
    let mut memory = PpcSectionMem::new();
    let heap_limit = PPC_HEAP_BASE + 0x1000;
    memory.add_region(PPC_HEAP_BASE, vec![0; 0x1000]);
    let mut manager = ProcessMemoryManager::default();
    manager.publish_native_allocator(
        ProcessNativeHeapState {
            heap_base: PPC_HEAP_BASE,
            heap_cursor: PPC_HEAP_BASE,
            heap_limit,
            last_mem_error: PPC_NO_ERR,
            heap_maximized: false,
            master_pointer_blocks_requested: 0,
        },
        &[],
        &[],
        &[],
    );
    let ptr = manager.new_native_ptr(&mut memory, 8, true);
    assert_ne!(ptr, 0);

    assert_eq!(
        manager.set_native_ptr_size(&mut memory, ptr, 24),
        PPC_NO_ERR
    );
    assert_eq!(manager.native_ptr_size(ptr), 24);
    assert_eq!(memory.read_u8(ptr + 23), Some(0));

    let second = manager.new_native_ptr(&mut memory, 8, true);
    assert_ne!(second, 0);
    assert_eq!(
        manager.set_native_ptr_size(&mut memory, ptr, 32),
        PPC_MEM_FULL_ERR
    );
    assert_eq!(manager.native_ptr_size(ptr), 24);
}

#[test]
fn import_bindings_classify_supported_memory_manager_imports() {
    let bindings = PpcImportBindingPlan::prepare(
        vec![
            PefResolvedImport {
                library_index: 0,
                symbol_index: 0,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "NewPtrClear".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 1,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "DisposePtr".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 2,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "MaxApplZone".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 3,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "MoreMasters".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 4,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "CompactMem".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 5,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "MaxMem".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 6,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "CurResFile".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 7,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "ResError".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 8,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "Gestalt".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 9,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "GetSharedLibrary".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 10,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "FindFolder".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 11,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "DirCreate".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 12,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "FSMakeFSSpec".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 13,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "SndSoundManagerVersion".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 14,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "ParamText".to_string(),
                class: 2,
                weak: false,
            },
            PefResolvedImport {
                library_index: 0,
                symbol_index: 15,
                library_name: "InterfaceLib".to_string(),
                symbol_name: "NoteAlert".to_string(),
                class: 2,
                weak: false,
            },
        ],
        16,
        0,
        ppc_import_layout(),
        &SystemlessPpcImportBindingPolicy,
    )
    .unwrap()
    .into_initial_bindings();

    assert_eq!(
        bindings[0].dispatcher_target,
        PpcImportDispatcherTarget::NewPtr { clear: true }
    );
    assert_eq!(
        bindings[1].dispatcher_target,
        PpcImportDispatcherTarget::DisposePtr
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetPtrSize"),
        PpcImportDispatcherTarget::GetPtrSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewHandle"),
        PpcImportDispatcherTarget::NewHandle { clear: false }
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "NewHandleClear"),
        PpcImportDispatcherTarget::NewHandle { clear: true }
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "TempNewHandle"),
        PpcImportDispatcherTarget::TempNewHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "BlockMove"),
        PpcImportDispatcherTarget::BlockMove
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PtrToHand"),
        PpcImportDispatcherTarget::PtrToHand
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DisposeHandle"),
        PpcImportDispatcherTarget::DisposeHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "EmptyHandle"),
        PpcImportDispatcherTarget::EmptyHandle
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "ReleaseResource"),
        PpcImportDispatcherTarget::ReleaseResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "DetachResource"),
        PpcImportDispatcherTarget::DetachResource
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "GetHandleSize"),
        PpcImportDispatcherTarget::GetHandleSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "SetHandleSize"),
        PpcImportDispatcherTarget::SetHandleSize
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PurgeMem"),
        PpcImportDispatcherTarget::PurgeMem
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "PurgeMemSys"),
        PpcImportDispatcherTarget::PurgeMemSys
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "MemError"),
        PpcImportDispatcherTarget::MemError
    );
    assert_eq!(
        bindings[2].dispatcher_target,
        PpcImportDispatcherTarget::MaxApplZone
    );
    assert_eq!(
        bindings[3].dispatcher_target,
        PpcImportDispatcherTarget::MoreMasters
    );
    assert_eq!(
        bindings[4].dispatcher_target,
        PpcImportDispatcherTarget::HeapFreeBytes
    );
    assert_eq!(
        dispatcher_target_for_import("InterfaceLib", "FreeMem"),
        PpcImportDispatcherTarget::HeapFreeBytes
    );
    assert_eq!(
        bindings[5].dispatcher_target,
        PpcImportDispatcherTarget::MaxMem
    );
    assert_eq!(
        bindings[6].dispatcher_target,
        PpcImportDispatcherTarget::CurResFile
    );
    assert_eq!(
        bindings[7].dispatcher_target,
        PpcImportDispatcherTarget::ResError
    );
    assert_eq!(
        bindings[8].dispatcher_target,
        PpcImportDispatcherTarget::Gestalt
    );
    assert_eq!(
        bindings[9].dispatcher_target,
        PpcImportDispatcherTarget::GetSharedLibrary
    );
    assert_eq!(
        bindings[10].dispatcher_target,
        PpcImportDispatcherTarget::FindFolder
    );
    assert_eq!(
        bindings[11].dispatcher_target,
        PpcImportDispatcherTarget::DirCreate
    );
    assert_eq!(
        bindings[12].dispatcher_target,
        PpcImportDispatcherTarget::FSMakeFSSpec
    );
    assert_eq!(
        bindings[13].dispatcher_target,
        PpcImportDispatcherTarget::SndSoundManagerVersion
    );
    assert_eq!(
        bindings[14].dispatcher_target,
        PpcImportDispatcherTarget::ParamText
    );
    assert_eq!(
        bindings[15].dispatcher_target,
        PpcImportDispatcherTarget::AlertReturnDefault
    );
}

#[test]
fn hle_import_runner_handles_new_ptr_clear_and_continues() {
    let pef = synthetic_pef_with_import(b"NewPtrClear");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = 24;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.last_import_index, Some(0));
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(
        probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: 8,
        }
    );
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);
    assert_eq!(loaded.last_mem_error(), 0);
    assert_eq!(loaded.cpu.gpr[3] & (PPC_HEAP_ALIGNMENT - 1), 0);
    assert_eq!(
        loaded.heap_cursor(),
        PPC_HEAP_BASE + ppc_allocation_size(24).unwrap()
    );
    for offset in 0..24 {
        assert_eq!(loaded.memory.read_u8(PPC_HEAP_BASE + offset), Some(0));
    }
    assert_eq!(
        loaded.ptrs(),
        vec![PpcPtrRecord {
            ptr: PPC_HEAP_BASE,
            size: 24
        }]
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::GetPtrSize;
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 24);
    assert_eq!(loaded.last_mem_error(), PPC_NO_ERR);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::DisposePtr;
    loaded.cpu.gpr[3] = PPC_HEAP_BASE;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert!(loaded.ptrs().is_empty());
    assert_eq!(
        loaded.free_ptr_blocks(),
        vec![PpcPtrRecord {
            ptr: PPC_HEAP_BASE,
            size: 24
        }]
    );
    let heap_cursor = loaded.heap_cursor();

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.imports[0].dispatcher_target = PpcImportDispatcherTarget::NewPtr { clear: true };
    loaded.cpu.gpr[3] = 12;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], PPC_HEAP_BASE);
    assert_eq!(loaded.heap_cursor(), heap_cursor);
    assert!(loaded.free_ptr_blocks().is_empty());
    assert_eq!(loaded.ptrs()[0].size, 12);
}

#[test]
fn hle_import_runner_ptr_to_hand_copies_bytes_into_a_new_handle() {
    let pef = synthetic_pef_with_import(b"PtrToHand");
    let mut loaded = load_pef_application(&pef).unwrap();
    let source_ptr = PPC_DATA_BASE + 0x1000;
    let destination_handle_ptr = source_ptr + 16;
    loaded.memory.add_region(source_ptr, vec![0; 32]);
    for (offset, byte) in b"Gridz".iter().copied().enumerate() {
        loaded
            .memory
            .write_u8(source_ptr + offset as u32, byte)
            .unwrap();
    }
    loaded.cpu.gpr[3] = source_ptr;
    loaded.cpu.gpr[4] = destination_handle_ptr;
    loaded.cpu.gpr[5] = 5;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let handle = loaded.memory.read_u32_be(destination_handle_ptr).unwrap();
    assert_ne!(handle, 0);
    assert_eq!(
        ppc_handle_bytes(&mut loaded.memory, &test_handle_records!(loaded), handle),
        Some(b"Gridz".to_vec())
    );
}

#[test]
fn hle_import_runner_holds_resident_native_memory() {
    let pef = synthetic_pef_with_import(b"HoldMemory");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.cpu.gpr[3] = PPC_DATA_BASE;
    loaded.cpu.gpr[4] = 4096;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
}

#[test]
fn system_arena_leaves_a_separate_resource_tail_after_max_block() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    loaded.reserve_ppc_system_storage();

    let limit = loaded.heap_limit();
    let arena = limit - 2 * 1024 * 1024 - PPC_SYSTEM_ALLOCATION_POOL_SIZE;
    let (total, largest) = ppc_heap_free_capacity(&loaded.memory, loaded.heap_cursor(), limit);
    assert!(loaded
        .memory
        .has_readonly_allocation_exclusion(arena, PPC_SYSTEM_ALLOCATION_POOL_SIZE));
    assert!(loaded.memory.read_u8(arena).is_some());
    assert_eq!(total - largest, 2 * 1024 * 1024);
}

