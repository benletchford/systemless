use super::*;

#[test]
fn apple_event_compatibility_imports_pre_resolve_to_typed_operations() {
    for (symbol, operation) in [
        (
            "AECountItems",
            PpcAppleEventCompatibilityOperation::CountItems,
        ),
        (
            "AECreateAppleEvent",
            PpcAppleEventCompatibilityOperation::CreateAppleEvent,
        ),
        (
            "AECreateDesc",
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
        (
            "AECreateList",
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
        (
            "AEPutDesc",
            PpcAppleEventCompatibilityOperation::PutDesc,
        ),
        (
            "AEDisposeDesc",
            PpcAppleEventCompatibilityOperation::DisposeDesc,
        ),
        (
            "AEGetAttributePtr",
            PpcAppleEventCompatibilityOperation::GetAttributePtr,
        ),
        (
            "AEGetNthPtr",
            PpcAppleEventCompatibilityOperation::GetNthPtr,
        ),
        (
            "AEGetParamDesc",
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
        (
            "AEGetParamPtr",
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
        (
            "AESizeOfParam",
            PpcAppleEventCompatibilityOperation::SizeOfParam,
        ),
        (
            "AEPutParamDesc",
            PpcAppleEventCompatibilityOperation::PutParamDesc,
        ),
        (
            "AEPutParamPtr",
            PpcAppleEventCompatibilityOperation::PutParamPtr,
        ),
        ("AESend", PpcAppleEventCompatibilityOperation::Send),
    ] {
        assert_eq!(
            dispatcher_target_for_import("InterfaceLib", symbol),
            PpcImportDispatcherTarget::AppleEventCompatibility(operation),
            "unexpected Apple Event dispatch target for {symbol}",
        );
    }
}

#[test]
fn native_ppc_ae_put_desc_copies_items_into_lists() {
    let pef = synthetic_pef_with_import(b"AECreateList");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(scratch, vec![0; 0x100]);
    let list = scratch;
    let source = scratch + 8;
    let text = scratch + 0x40;
    loaded.memory.write_bytes(text, b"cows").unwrap();

    loaded.cpu.gpr[3] = 0;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = 0;
    loaded.cpu.gpr[6] = list;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateList,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[4] = text;
    loaded.cpu.gpr[5] = 4;
    loaded.cpu.gpr[6] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));

    loaded.cpu.gpr[3] = source;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::DisposeDesc,
        ),
    );
    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = scratch + 0x20;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CountItems,
        ),
    );
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x20), Some(1));

    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = PPC_TYPE_WILDCARD;
    loaded.cpu.gpr[6] = scratch + 0x24;
    loaded.cpu.gpr[7] = scratch + 0x28;
    loaded.cpu.gpr[8] = scratch + 0x2c;
    loaded.cpu.gpr[9] = 4;
    loaded.cpu.gpr[10] = scratch + 0x30;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetNthPtr,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x24), Some(PPC_TYPE_WILDCARD));
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x28), Some(u32::from_be_bytes(*b"TEXT")));
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, scratch + 0x2c, 4),
        Some(b"cows".to_vec()),
    );
    assert_eq!(loaded.memory.read_u32_be(scratch + 0x30), Some(4));

    loaded.cpu.gpr[3] = list;
    loaded.cpu.gpr[4] = 3;
    loaded.cpu.gpr[5] = list;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_ILLEGAL_INDEX));

    let non_list = scratch + 0x60;
    loaded.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    loaded.cpu.gpr[4] = text;
    loaded.cpu.gpr[5] = 4;
    loaded.cpu.gpr[6] = non_list;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    loaded.cpu.gpr[3] = non_list;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = list;
    run_test_import(
        &mut loaded,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutDesc,
        ),
    );
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_ERR_AE_WRONG_DATA_TYPE));
}

#[test]
fn native_ppc_apple_event_lists_are_countable_and_disposable() {
    for (is_record, expected_type) in [(false, *b"list"), (true, *b"reco")] {
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, b"TEXT".to_vec());
        memory.add_region(0x1100, vec![0xaa; 12]);
        let mut cpu = PpcCpu::new();
        cpu.gpr[3] = 0x1000;
        cpu.gpr[4] = 4;
        cpu.gpr[5] = u32::from(is_record);
        cpu.gpr[6] = 0x1100;
        let mut heap_cursor = 0x3000;
        let mut last_mem_error = PPC_NO_ERR;
        let mut memory_manager = ProcessMemoryManager::default();
        memory_manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: heap_cursor,
                heap_cursor,
                heap_limit: 0x5000,
                last_mem_error,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        let mut handles = Vec::new();
        let mut apple_events = PpcAppleEventState::default();
        let mut dispatch = |operation, cpu: &mut PpcCpu| {
            ppc_dispatch_apple_event_compatibility(
                operation,
                cpu,
                &mut memory_manager,
                &mut memory,
                &mut heap_cursor,
                0x5000,
                &mut last_mem_error,
                &mut handles,
                &mut apple_events,
            )
        };
        assert_eq!(
            dispatch(PpcAppleEventCompatibilityOperation::CreateList, &mut cpu),
            PpcImportAction::Return(0),
        );
        drop(dispatch);
        assert_eq!(memory.read_u32_be(0x1100), Some(u32::from_be_bytes(expected_type)));
        assert_ne!(memory.read_u32_be(0x1104), Some(0));

        cpu.gpr[3] = 0x1100;
        cpu.gpr[4] = 0x1108;
        assert_eq!(
            ppc_dispatch_apple_event_compatibility(
                PpcAppleEventCompatibilityOperation::CountItems,
                &mut cpu,
                &mut memory_manager,
                &mut memory,
                &mut heap_cursor,
                0x5000,
                &mut last_mem_error,
                &mut handles,
                &mut apple_events,
            ),
            PpcImportAction::Return(0),
        );
        assert_eq!(memory.read_u32_be(0x1108), Some(0));

        cpu.gpr[3] = 0x1100;
        assert_eq!(
            ppc_dispatch_apple_event_compatibility(
                PpcAppleEventCompatibilityOperation::DisposeDesc,
                &mut cpu,
                &mut memory_manager,
                &mut memory,
                &mut heap_cursor,
                0x5000,
                &mut last_mem_error,
                &mut handles,
                &mut apple_events,
            ),
            PpcImportAction::Return(0),
        );
        assert_eq!(memory.read_u32_be(0x1100), Some(0));
        assert_eq!(memory.read_u32_be(0x1104), Some(0));
        assert!(handles.is_empty());

        memory.write_bytes(0x1100, &[0xaa; 8]).unwrap();
        cpu.gpr[3] = 0x1000;
        cpu.gpr[4] = 5;
        cpu.gpr[5] = u32::from(is_record);
        cpu.gpr[6] = 0x1100;
        assert_eq!(
            ppc_dispatch_apple_event_compatibility(
                PpcAppleEventCompatibilityOperation::CreateList,
                &mut cpu,
                &mut memory_manager,
                &mut memory,
                &mut heap_cursor,
                0x5000,
                &mut last_mem_error,
                &mut handles,
                &mut apple_events,
            ),
            PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)),
        );
        assert_eq!(memory.read_u32_be(0x1100), Some(0));
        assert_eq!(memory.read_u32_be(0x1104), Some(0));
    }
}

#[test]
fn native_ppc_apple_event_descriptors_own_and_dispose_guest_data() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, b"payload".to_vec());
    memory.add_region(0x1100, vec![0xaa; 8]);
    let mut cpu = PpcCpu::new();
    cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    cpu.gpr[4] = 0x1000;
    cpu.gpr[5] = 7;
    cpu.gpr[6] = 0x1100;
    let mut heap_cursor = 0x3000;
    let mut last_mem_error = PPC_NO_ERR;
    let mut memory_manager = ProcessMemoryManager::default();
    memory_manager.publish_native_allocator(
        ProcessNativeHeapState {
            heap_base: heap_cursor,
            heap_cursor,
            heap_limit: 0x5000,
            last_mem_error,
            heap_maximized: false,
            master_pointer_blocks_requested: 0,
        },
        &[],
        &[],
        &[],
    );
    let mut handles = Vec::new();
    let mut apple_events = PpcAppleEventState::default();
    assert_eq!(
        ppc_dispatch_apple_event_compatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
            &mut cpu,
            &mut memory_manager,
            &mut memory,
            &mut heap_cursor,
            0x5000,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
        ),
        PpcImportAction::Return(0)
    );
    let data_handle = memory.read_u32_be(0x1104).unwrap();
    assert_eq!(
        memory.read_u32_be(0x1100),
        Some(u32::from_be_bytes(*b"TEXT"))
    );
    assert_eq!(
        ppc_handle_bytes(&mut memory, &handles, data_handle),
        Some(b"payload".to_vec())
    );

    cpu.gpr[3] = 0x1100;
    assert_eq!(
        ppc_dispatch_apple_event_compatibility(
            PpcAppleEventCompatibilityOperation::DisposeDesc,
            &mut cpu,
            &mut memory_manager,
            &mut memory,
            &mut heap_cursor,
            0x5000,
            &mut last_mem_error,
            &mut handles,
            &mut apple_events,
        ),
        PpcImportAction::Return(0)
    );
    assert_eq!(memory.read_u32_be(0x1100), Some(0));
    assert_eq!(memory.read_u32_be(0x1104), Some(0));
    assert!(handles.is_empty());
}

#[test]
fn apple_event_descriptor_handles_are_immediately_process_owned_and_cross_isa_visible() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"AECreateDesc");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = context.memory_manager_mut().detached_clone();
    let scratch = PPC_DATA_BASE + 0x2000;
    let source = scratch;
    let descriptor = scratch + 0x20;
    native.memory.add_region(scratch, vec![0; 0x40]);
    native.memory.write_bytes(source, b"payload").unwrap();
    native.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[4] = source;
    native.cpu.gpr[5] = 7;
    native.cpu.gpr[6] = descriptor;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );

    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);

    let handle = native.memory.read_u32_be(descriptor + 4).unwrap();
    let allocation = context
        .memory_manager_mut()
        .native_allocation(handle)
        .unwrap();
    assert_eq!(allocation.size, 7);
    assert_eq!(
        context.memory_manager_mut().recover_handle(allocation.ptr),
        Some(handle)
    );
    assert_eq!(detached.native_allocation(handle), None);

    let shared = native.memory.shared_view();
    let mut classic_bus = MacMemoryBus::new(0x2000);
    classic_bus.attach_guest_address_space(shared);
    context.attach_classic_memory_bus(&mut classic_bus);
    assert_eq!(classic_bus.read_bytes(allocation.ptr, 7), b"payload");
    classic_bus.write_byte(allocation.ptr, b'P');
    assert_eq!(native.memory.read_u8(allocation.ptr), Some(b'P'));
    native.memory.write_u8(allocation.ptr + 6, b'!').unwrap();
    assert_eq!(classic_bus.read_byte(allocation.ptr + 6), b'!');

    native.cpu.gpr[3] = descriptor;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::DisposeDesc,
        ),
    );

    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    assert_eq!(native.memory.read_u32_be(descriptor), Some(0));
    assert_eq!(native.memory.read_u32_be(descriptor + 4), Some(0));
    assert_eq!(native.memory.read_u32_be(handle), Some(0));
    let manager = context.memory_manager_mut();
    assert_eq!(manager.native_allocation(handle), None);
    assert_eq!(manager.recover_handle(allocation.ptr), None);
    assert!(manager
        .native_allocator_snapshot()
        .unwrap()
        .free_handle_blocks
        .iter()
        .any(|record| record.handle == handle));
    assert_eq!(detached.native_allocation(handle), None);
}

#[test]
fn apple_event_descriptor_allocation_failure_is_atomic() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"AECreateDesc");
    let mut native = load_pef_application(&pef).unwrap();
    native.set_heap_cursor(native.heap_limit().saturating_sub(8));
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let scratch = PPC_DATA_BASE + 0x2200;
    let descriptor = scratch + 0x20;
    native.memory.add_region(scratch, vec![0xaa; 0x40]);
    native.memory.write_bytes(scratch, b"payload").unwrap();
    let (before_allocator, before_handles) = {
        let manager = context.memory_manager_mut();
        (
            manager.native_allocator_snapshot().unwrap(),
            manager.native_handle_records().to_vec(),
        )
    };
    native.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[4] = scratch;
    native.cpu.gpr[5] = 7;
    native.cpu.gpr[6] = descriptor;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );

    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_MEM_FULL_ERR);
    assert_eq!(native.memory.read_u32_be(descriptor), Some(0xaaaa_aaaa));
    assert_eq!(native.memory.read_u32_be(descriptor + 4), Some(0xaaaa_aaaa));
    let manager = context.memory_manager_mut();
    let after_allocator = manager.native_allocator_snapshot().unwrap();
    assert_eq!(
        after_allocator.heap.heap_cursor,
        before_allocator.heap.heap_cursor
    );
    assert_eq!(
        after_allocator.heap.heap_limit,
        before_allocator.heap.heap_limit
    );
    assert_eq!(after_allocator.ptrs, before_allocator.ptrs);
    assert_eq!(
        after_allocator.free_ptr_blocks,
        before_allocator.free_ptr_blocks
    );
    assert_eq!(
        after_allocator.free_handle_blocks,
        before_allocator.free_handle_blocks
    );
    assert_eq!(manager.native_handle_records(), before_handles);
}

#[test]
fn native_apple_event_parameters_round_trip_through_process_semantics() {
    let mut native = load_pef_application(&synthetic_pef_with_import(b"TestImport")).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let scratch = PPC_DATA_BASE + 0x2280;
    let source_bytes = scratch;
    let source_desc = scratch + 0x20;
    let event_desc = scratch + 0x40;
    let result_desc = scratch + 0x60;
    native.memory.add_region(scratch, vec![0; 0x90]);
    native
        .memory
        .write_bytes(source_bytes, b"shared value")
        .unwrap();

    native.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[4] = source_bytes;
    native.cpu.gpr[5] = 12;
    native.cpu.gpr[6] = source_desc;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );

    native.cpu.gpr[3] = u32::from_be_bytes(*b"misc");
    native.cpu.gpr[4] = u32::from_be_bytes(*b"slct");
    native.cpu.gpr[8] = event_desc;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateAppleEvent,
        ),
    );

    let keyword = u32::from_be_bytes(*b"----");
    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[4] = keyword;
    native.cpu.gpr[5] = source_desc;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::PutParamDesc,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);

    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[4] = keyword;
    native.cpu.gpr[5] = scratch + 0x70;
    native.cpu.gpr[6] = scratch + 0x74;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::SizeOfParam,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    assert_eq!(
        native.memory.read_u32_be(scratch + 0x70),
        Some(u32::from_be_bytes(*b"TEXT"))
    );
    assert_eq!(native.memory.read_u32_be(scratch + 0x74), Some(12));

    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[4] = u32::from_be_bytes(*b"none");
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::SizeOfParam,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_ERR_AE_DESC_NOT_FOUND);

    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[4] = keyword;
    native.cpu.gpr[5] = PPC_TYPE_WILDCARD;
    native.cpu.gpr[6] = result_desc;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamDesc,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    assert_eq!(
        native.memory.read_u32_be(result_desc),
        Some(u32::from_be_bytes(*b"TEXT"))
    );
    let result_handle = native.memory.read_u32_be(result_desc + 4).unwrap();
    let allocation = context
        .memory_manager_mut()
        .native_allocation(result_handle)
        .unwrap();
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, allocation.ptr, allocation.size),
        Some(b"shared value".to_vec())
    );

    let returned_type = scratch + 0x70;
    let returned_size = scratch + 0x74;
    let data_ptr = scratch + 0x78;
    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[4] = keyword;
    native.cpu.gpr[5] = PPC_TYPE_WILDCARD;
    native.cpu.gpr[6] = returned_type;
    native.cpu.gpr[7] = data_ptr;
    native.cpu.gpr[8] = 4;
    native.cpu.gpr[9] = returned_size;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_AE_BUFFER_IS_SMALL);
    assert_eq!(
        native.memory.read_u32_be(returned_type),
        Some(u32::from_be_bytes(*b"TEXT"))
    );
    assert_eq!(native.memory.read_u32_be(returned_size), Some(12));
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, data_ptr, 4),
        Some(b"shar".to_vec())
    );

    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[8] = 12;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, data_ptr, 12),
        Some(b"shared value".to_vec())
    );

    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[5] = u32::from_be_bytes(*b"long");
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_ERR_AE_COERCION_FAIL);

    native.cpu.gpr[3] = event_desc;
    native.cpu.gpr[4] = u32::from_be_bytes(*b"none");
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::GetParamPtr,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_ERR_AE_DESC_NOT_FOUND);
    assert_eq!(native.memory.read_u32_be(returned_type), Some(0));
    assert_eq!(native.memory.read_u32_be(returned_size), Some(0));
}

#[test]
fn native_ppc_object_support_initializes_before_creating_specifiers() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"AEObjectInit");
    let mut native = load_pef_application(&pef).unwrap();
    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AEObjectInit"),
        PpcImportDispatcherTarget::ObjectSupportInit,
    );
    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportInit);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(native.apple_events.object_support_initialized);
}

#[test]
fn native_ppc_ae_resolve_rejects_non_object_specifiers_with_null_token() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"AEResolve");
    let mut native = load_pef_application(&pef).unwrap();
    let target = PpcImportDispatcherTarget::ObjectSupportResolve;
    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AEResolve"),
        target,
    );
    let specifier = PPC_DATA_BASE + 0x2400;
    let token = PPC_DATA_BASE + 0x2410;
    native.memory.add_region(specifier, vec![0; 24]);
    native.memory.write_u32_be(token, 0xdead_beef);
    native.memory.write_u32_be(token + 4, 0x1234_5678);
    native.cpu.gpr[3] = specifier;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;
    run_test_import(&mut native, target.clone());
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(native.memory.read_u32_be(token), Some(PPC_TYPE_NULL));
    assert_eq!(native.memory.read_u32_be(token + 4), Some(0));

    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportInit);
    native.cpu.gpr[3] = specifier;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;
    run_test_import(&mut native, target.clone());
    assert_eq!(
        native.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_NOT_AN_OBJECT_SPEC),
    );
    assert_eq!(native.memory.read_u32_be(token), Some(PPC_TYPE_NULL));
    assert_eq!(native.memory.read_u32_be(token + 4), Some(0));

    native
        .memory
        .write_u32_be(specifier, u32::from_be_bytes(*b"long"));
    native.cpu.gpr[3] = specifier;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;
    run_test_import(&mut native, target);
    assert_eq!(
        native.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_NOT_AN_OBJECT_SPEC),
    );
}

#[test]
fn native_ppc_ae_resolve_collects_nested_containers_in_accessor_order() {
    fn specifier(desired: &[u8; 4], container: ProcessAeDescriptor) -> ProcessAeDescriptor {
        let mut fields = HashMap::new();
        fields.insert(
            u32::from_be_bytes(*b"want"),
            ProcessAeDescriptor {
                desc_type: PPC_TYPE_TYPE,
                data: desired.to_vec(),
                ..Default::default()
            },
        );
        fields.insert(
            u32::from_be_bytes(*b"form"),
            ProcessAeDescriptor {
                desc_type: PPC_TYPE_ENUMERATED,
                data: b"name".to_vec(),
                ..Default::default()
            },
        );
        fields.insert(
            u32::from_be_bytes(*b"seld"),
            ProcessAeDescriptor {
                desc_type: u32::from_be_bytes(*b"TEXT"),
                data: b"Example".to_vec(),
                ..Default::default()
            },
        );
        fields.insert(u32::from_be_bytes(*b"from"), container);
        ProcessAeDescriptor {
            desc_type: PPC_TYPE_OBJECT_SPECIFIER,
            fields,
            ..Default::default()
        }
    }
    let root = ProcessAeDescriptor {
        desc_type: PPC_TYPE_NULL,
        ..Default::default()
    };
    let inner = specifier(b"docu", root);
    let outer = specifier(b"cwin", inner);
    let (base, levels) = dispatch_apple_events::ppc_ae_collect_resolve_levels(&outer).unwrap();
    assert_eq!(base.desc_type, PPC_TYPE_NULL);
    assert_eq!(
        levels
            .iter()
            .map(|level| level.desired_class)
            .collect::<Vec<_>>(),
        [u32::from_be_bytes(*b"docu"), u32::from_be_bytes(*b"cwin"),]
    );
    assert_eq!(levels[0].key_data.data, b"Example");

    let mut malformed = outer.clone();
    malformed
        .fields
        .get_mut(&u32::from_be_bytes(*b"want"))
        .unwrap()
        .desc_type = u32::from_be_bytes(*b"TEXT");
    assert!(dispatch_apple_events::ppc_ae_collect_resolve_levels(&malformed).is_none());

    let mut default_container = outer;
    default_container
        .fields
        .remove(&u32::from_be_bytes(*b"from"));
    let (base, levels) =
        dispatch_apple_events::ppc_ae_collect_resolve_levels(&default_container).unwrap();
    assert_eq!(base.desc_type, PPC_TYPE_NULL);
    assert_eq!(levels.len(), 1);
}

#[test]
fn native_ppc_ae_resolve_calls_nested_accessors_and_releases_scratch() {
    fn descriptor_u32(desc_type: u32, value: u32) -> ProcessAeDescriptor {
        ProcessAeDescriptor {
            desc_type,
            data: value.to_be_bytes().to_vec(),
            ..Default::default()
        }
    }
    fn specifier(desired: u32, container: ProcessAeDescriptor) -> ProcessAeDescriptor {
        let mut fields = HashMap::new();
        fields.insert(
            u32::from_be_bytes(*b"want"),
            descriptor_u32(PPC_TYPE_TYPE, desired),
        );
        fields.insert(
            u32::from_be_bytes(*b"form"),
            descriptor_u32(PPC_TYPE_ENUMERATED, u32::from_be_bytes(*b"name")),
        );
        fields.insert(u32::from_be_bytes(*b"from"), container);
        fields.insert(
            u32::from_be_bytes(*b"seld"),
            ProcessAeDescriptor {
                desc_type: u32::from_be_bytes(*b"TEXT"),
                data: b"Room".to_vec(),
                ..Default::default()
            },
        );
        ProcessAeDescriptor {
            desc_type: PPC_TYPE_OBJECT_SPECIFIER,
            fields,
            ..Default::default()
        }
    }

    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"AEResolve");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let scratch = PPC_DATA_BASE + 0x2900;
    let specifier_ptr = scratch;
    let tvector = scratch + 0x100;
    let entry = scratch + 0x120;
    let token = scratch + 0x200;
    native.memory.add_region(scratch, vec![0; 0x300]);
    native
        .memory
        .write_u32_be(specifier_ptr, PPC_TYPE_OBJECT_SPECIFIER)
        .unwrap();
    native.memory.write_u32_be(tvector, entry).unwrap();
    native
        .memory
        .write_u32_be(tvector + 4, 0x0200_1234)
        .unwrap();
    native.memory.write_u32_be(entry, 0x906a_0000).unwrap(); // stw r3,0(r10)
    native.memory.write_u32_be(entry + 4, 0x3860_0000).unwrap(); // li r3,0
    native.memory.write_u32_be(entry + 8, 0x4e80_0020).unwrap(); // blr
    let document = u32::from_be_bytes(*b"docu");
    let window = u32::from_be_bytes(*b"cwin");
    let base = ProcessAeDescriptor {
        desc_type: PPC_TYPE_NULL,
        ..Default::default()
    };
    let outer = specifier(window, specifier(document, base));
    native.apple_events.descriptors.with_mut(|state| {
        state.descriptors.insert(specifier_ptr, outer);
    });
    native.apple_events.object_support_initialized = true;
    for (desired, container_type) in [(document, PPC_TYPE_NULL), (window, document)] {
        native.apple_events.object_accessors.insert(
            (false, desired, container_type),
            dispatch_apple_events::PpcObjectAccessor {
                pointer: tvector,
                refcon: 0,
            },
        );
    }
    let handles_before = native.handles();
    let ptrs_before = native.ptrs();
    native.cpu.gpr[3] = specifier_ptr;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;

    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportResolve);

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(native.memory.read_u32_be(token), Some(window));
    assert!(
        native.apple_events.pending_resolutions.is_empty(),
        "pending={:?} pc={:#x} lr={:#x} depth={}",
        native.apple_events.pending_resolutions,
        native.cpu.pc,
        native.cpu.lr,
        native.guest_calls().depth(),
    );
    assert_eq!(native.handles(), handles_before);
    assert_eq!(native.ptrs(), ptrs_before);

    let source = scratch + 0x240;
    let allocated_token = scratch + 0x260;
    native.memory.write_bytes(source, b"Room").unwrap();
    native.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[4] = source;
    native.cpu.gpr[5] = 4;
    native.cpu.gpr[6] = allocated_token;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let intermediate_handle = native.memory.read_u32_be(allocated_token + 4).unwrap();
    assert!(context
        .memory_manager_mut()
        .native_allocation(intermediate_handle)
        .is_some());
    let first_tvector = scratch + 0x140;
    let first_entry = scratch + 0x1a0;
    native
        .memory
        .write_u32_be(first_tvector, first_entry)
        .unwrap();
    native
        .memory
        .write_u32_be(first_tvector + 4, 0x0200_1234)
        .unwrap();
    native
        .memory
        .write_u32_be(first_entry, 0x906a_0000)
        .unwrap(); // stw r3,0(r10)
    native
        .memory
        .write_u32_be(first_entry + 4, 0x3c80_0000 | (intermediate_handle >> 16))
        .unwrap(); // lis r4,handle@hi
    native
        .memory
        .write_u32_be(
            first_entry + 8,
            0x6084_0000 | (intermediate_handle & 0xffff),
        )
        .unwrap(); // ori r4,r4,handle@lo
    native
        .memory
        .write_u32_be(first_entry + 12, 0x908a_0004)
        .unwrap(); // stw r4,4(r10)
    native
        .memory
        .write_u32_be(first_entry + 16, 0x3860_0000)
        .unwrap(); // li r3,0
    native
        .memory
        .write_u32_be(first_entry + 20, 0x4e80_0020)
        .unwrap(); // blr
    native
        .apple_events
        .object_accessors
        .get_mut(&(false, document, PPC_TYPE_NULL))
        .unwrap()
        .pointer = first_tvector;
    let successful_dispose_tvector = scratch + 0x1c0;
    let successful_dispose_entry = scratch + 0x1e0;
    native
        .memory
        .write_u32_be(successful_dispose_tvector, successful_dispose_entry)
        .unwrap();
    native
        .memory
        .write_u32_be(successful_dispose_tvector + 4, 0x0200_1234)
        .unwrap();
    native
        .memory
        .write_u32_be(successful_dispose_entry, 0x3860_0000)
        .unwrap(); // li r3,0
    native
        .memory
        .write_u32_be(successful_dispose_entry + 4, 0x4e80_0020)
        .unwrap(); // blr
    native
        .apple_events
        .object_callbacks
        .insert(u32::from_be_bytes(*b"xtok"), successful_dispose_tvector);
    native.cpu.gpr[3] = specifier_ptr;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;
    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportResolve);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(native.memory.read_u32_be(token), Some(window));
    assert_eq!(
        context
            .memory_manager_mut()
            .native_allocation(intermediate_handle),
        None
    );
    assert_eq!(native.handles(), handles_before);
    assert_eq!(native.ptrs(), ptrs_before);
    assert!(native.apple_events.pending_resolve_cleanups.is_empty());
    native
        .apple_events
        .object_callbacks
        .remove(&u32::from_be_bytes(*b"xtok"));
    native
        .apple_events
        .object_accessors
        .get_mut(&(false, document, PPC_TYPE_NULL))
        .unwrap()
        .pointer = tvector;

    let dispose_tvector = scratch + 0x160;
    let dispose_entry = scratch + 0x180;
    native
        .memory
        .write_u32_be(dispose_tvector, dispose_entry)
        .unwrap();
    native
        .memory
        .write_u32_be(dispose_tvector + 4, 0x0200_1234)
        .unwrap();
    native
        .memory
        .write_u32_be(dispose_entry, 0x3860_1234)
        .unwrap(); // li r3,$1234
    native
        .memory
        .write_u32_be(dispose_entry + 4, 0x4e80_0020)
        .unwrap(); // blr
    native
        .apple_events
        .object_callbacks
        .insert(u32::from_be_bytes(*b"xtok"), dispose_tvector);
    native.cpu.gpr[3] = specifier_ptr;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;
    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportResolve);
    assert_eq!(native.cpu.gpr[3], 0x1234);
    assert_eq!(native.memory.read_u32_be(token), Some(PPC_TYPE_NULL));
    assert!(native.apple_events.pending_resolutions.is_empty());
    assert!(native.apple_events.pending_resolve_cleanups.is_empty());
    assert_eq!(native.handles(), handles_before);
    assert_eq!(native.ptrs(), ptrs_before);
    native
        .apple_events
        .object_callbacks
        .remove(&u32::from_be_bytes(*b"xtok"));

    native
        .apple_events
        .object_accessors
        .remove(&(false, window, document));
    native.cpu.gpr[3] = specifier_ptr;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;
    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportResolve);
    assert_eq!(
        native.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_ACCESSOR_NOT_FOUND)
    );
    assert_eq!(native.memory.read_u32_be(token), Some(PPC_TYPE_NULL));
    assert!(native.apple_events.pending_resolutions.is_empty());
    assert_eq!(native.handles(), handles_before);
    assert_eq!(native.ptrs(), ptrs_before);
}

#[test]
fn native_ppc_object_accessor_registration_validates_and_replaces() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"AEInstallObjectAccessor");
    let mut native = load_pef_application(&pef).unwrap();
    let target = PpcImportDispatcherTarget::ObjectSupportInstallAccessor;
    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AEInstallObjectAccessor"),
        target,
    );
    let desired = u32::from_be_bytes(*b"docu");
    let container = u32::from_be_bytes(*b"capp");
    native.cpu.gpr[3] = desired;
    native.cpu.gpr[4] = container;
    native.cpu.gpr[5] = 0x1000;
    native.cpu.gpr[6] = 0x1234;
    native.cpu.gpr[7] = 0;
    run_test_import(&mut native, target.clone());
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(native.apple_events.object_accessors.is_empty());

    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportInit);
    native.cpu.gpr[3] = desired;
    native.cpu.gpr[4] = container;
    native.cpu.gpr[5] = 0;
    native.cpu.gpr[6] = 0x1234;
    native.cpu.gpr[7] = 0;
    run_test_import(&mut native, target.clone());
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(native.apple_events.object_accessors.is_empty());

    for (pointer, refcon) in [(0x1000, 0x1234), (0x2000, 0x5678)] {
        native.cpu.gpr[3] = desired;
        native.cpu.gpr[4] = container;
        native.cpu.gpr[5] = pointer;
        native.cpu.gpr[6] = refcon;
        native.cpu.gpr[7] = 0;
        run_test_import(&mut native, target.clone());
        assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
        let registered = native
            .apple_events
            .object_accessors
            .get(&(false, desired, container))
            .unwrap();
        assert_eq!((registered.pointer, registered.refcon), (pointer, refcon));
    }
    assert_eq!(native.apple_events.object_accessors.len(), 1);

    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AEGetObjectAccessor"),
        PpcImportDispatcherTarget::ObjectSupportGetAccessor,
    );
    let pointer_out = PPC_DATA_BASE + 0x2700;
    let refcon_out = pointer_out + 4;
    native.memory.add_region(pointer_out, vec![0; 8]);
    native.cpu.gpr[3] = desired;
    native.cpu.gpr[4] = container;
    native.cpu.gpr[5] = pointer_out;
    native.cpu.gpr[6] = refcon_out;
    native.cpu.gpr[7] = 0;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportGetAccessor,
    );
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(native.memory.read_u32_be(pointer_out), Some(0x2000));
    assert_eq!(native.memory.read_u32_be(refcon_out), Some(0x5678));

    native.cpu.gpr[3] = desired;
    native.cpu.gpr[4] = container;
    native.cpu.gpr[5] = pointer_out;
    native.cpu.gpr[6] = refcon_out;
    native.cpu.gpr[7] = 1;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportGetAccessor,
    );
    assert_eq!(
        native.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_ACCESSOR_NOT_FOUND)
    );
    assert_eq!(native.memory.read_u32_be(pointer_out), Some(0));
    assert_eq!(native.memory.read_u32_be(refcon_out), Some(0));

    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AERemoveObjectAccessor"),
        PpcImportDispatcherTarget::ObjectSupportRemoveAccessor,
    );
    native.cpu.gpr[3] = desired;
    native.cpu.gpr[4] = container;
    native.cpu.gpr[5] = 0x1000;
    native.cpu.gpr[6] = 0;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportRemoveAccessor,
    );
    assert_eq!(
        native.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_ACCESSOR_NOT_FOUND)
    );
    assert_eq!(native.apple_events.object_accessors.len(), 1);

    native.cpu.gpr[3] = desired;
    native.cpu.gpr[4] = container;
    native.cpu.gpr[5] = 0;
    native.cpu.gpr[6] = 0;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportRemoveAccessor,
    );
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert!(native.apple_events.object_accessors.is_empty());
}

#[test]
fn native_ppc_object_accessor_lookup_prefers_application_and_specific_entries() {
    let mut state = PpcAppleEventState::default();
    let desired = u32::from_be_bytes(*b"docu");
    let container = PPC_TYPE_NULL;
    for (key, pointer) in [
        ((true, desired, container), 0x1000),
        ((false, PPC_TYPE_WILDCARD, PPC_TYPE_WILDCARD), 0x2000),
        ((false, desired, PPC_TYPE_WILDCARD), 0x3000),
        ((false, desired, container), 0x4000),
    ] {
        state.object_accessors.insert(
            key,
            dispatch_apple_events::PpcObjectAccessor { pointer, refcon: 0 },
        );
    }
    assert_eq!(
        state
            .object_accessor_for(desired, container)
            .unwrap()
            .pointer,
        0x4000
    );
    assert_eq!(
        state
            .object_accessor_exact(desired, container)
            .unwrap()
            .pointer,
        0x4000
    );
    state.object_accessors.remove(&(false, desired, container));
    assert_eq!(
        state
            .object_accessor_for(desired, container)
            .unwrap()
            .pointer,
        0x3000
    );
    assert_eq!(
        state
            .object_accessor_exact(desired, container)
            .unwrap()
            .pointer,
        0x1000
    );
    state
        .object_accessors
        .remove(&(false, desired, PPC_TYPE_WILDCARD));
    assert_eq!(
        state
            .object_accessor_for(desired, container)
            .unwrap()
            .pointer,
        0x2000
    );
    state
        .object_accessors
        .remove(&(false, PPC_TYPE_WILDCARD, PPC_TYPE_WILDCARD));
    assert_eq!(
        state
            .object_accessor_for(desired, container)
            .unwrap()
            .pointer,
        0x1000
    );
    state.object_accessors.remove(&(true, desired, container));
    assert!(state.object_accessor_exact(desired, container).is_none());
}

#[test]
fn native_ppc_call_object_accessor_passes_refcon_as_ninth_word() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"AECallObjectAccessor");
    let mut native = load_pef_application(&pef).unwrap();
    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AECallObjectAccessor"),
        PpcImportDispatcherTarget::ObjectSupportCallAccessor,
    );
    let scratch = PPC_DATA_BASE + 0x2800;
    let tvector = scratch;
    let entry = scratch + 0x100;
    let token = scratch + 0x200;
    native.memory.add_region(scratch, vec![0; 0x300]);
    native.memory.write_u32_be(tvector, entry).unwrap();
    native
        .memory
        .write_u32_be(tvector + 4, 0x0200_1234)
        .unwrap();
    native.memory.write_u32_be(entry, 0x8061_0038).unwrap(); // lwz r3,56(r1)
    native.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap(); // blr
    native.apple_events.object_support_initialized = true;
    native.apple_events.object_accessors.insert(
        (false, u32::from_be_bytes(*b"docu"), PPC_TYPE_NULL),
        dispatch_apple_events::PpcObjectAccessor {
            pointer: tvector,
            refcon: 0x1234,
        },
    );
    native.cpu.gpr[3] = u32::from_be_bytes(*b"docu");
    native.cpu.gpr[4] = PPC_TYPE_NULL;
    native.cpu.gpr[5] = 0;
    native.cpu.gpr[6] = PPC_TYPE_NULL;
    native.cpu.gpr[7] = u32::from_be_bytes(*b"name");
    native.cpu.gpr[8] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[9] = 0;
    native.cpu.gpr[10] = token;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportCallAccessor,
    );

    assert_eq!(native.cpu.gpr[3], 0x1234);
    assert_eq!(native.cpu.gpr[2], native.rtoc);
    assert_eq!(native.memory.read_u32_be(token), Some(PPC_TYPE_NULL));

    native.memory.write_u32_be(entry, 0x910a_0000).unwrap(); // stw r8,0(r10)
    native.memory.write_u32_be(entry + 4, 0x3860_0000).unwrap(); // li r3,0
    native.memory.write_u32_be(entry + 8, 0x4e80_0020).unwrap(); // blr
    native.cpu.gpr[3] = u32::from_be_bytes(*b"docu");
    native.cpu.gpr[4] = PPC_TYPE_NULL;
    native.cpu.gpr[5] = 0;
    native.cpu.gpr[6] = PPC_TYPE_NULL;
    native.cpu.gpr[7] = u32::from_be_bytes(*b"name");
    native.cpu.gpr[8] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[9] = 0;
    native.cpu.gpr[10] = token;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportCallAccessor,
    );
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        native.memory.read_u32_be(token),
        Some(u32::from_be_bytes(*b"TEXT"))
    );
}

#[test]
fn native_ppc_dispose_token_falls_back_after_unhandled_callback() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"AEDisposeToken");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AEDisposeToken"),
        PpcImportDispatcherTarget::ObjectSupportDisposeToken,
    );
    let scratch = PPC_DATA_BASE + 0x2c00;
    let source = scratch;
    let token = scratch + 0x20;
    let tvector = scratch + 0x100;
    let entry = scratch + 0x120;
    native.memory.add_region(scratch, vec![0; 0x200]);
    native.memory.write_bytes(source, b"Token").unwrap();
    native.memory.write_u32_be(tvector, entry).unwrap();
    native
        .memory
        .write_u32_be(tvector + 4, 0x0200_1234)
        .unwrap();
    native.memory.write_u32_be(entry, 0x3860_f954).unwrap(); // li r3,-1708
    native.memory.write_u32_be(entry + 4, 0x4e80_0020).unwrap(); // blr
    native.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[4] = source;
    native.cpu.gpr[5] = 5;
    native.cpu.gpr[6] = token;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    let handle = native.memory.read_u32_be(token + 4).unwrap();
    assert!(context
        .memory_manager_mut()
        .native_allocation(handle)
        .is_some());
    native.apple_events.object_support_initialized = true;
    native
        .apple_events
        .object_callbacks
        .insert(u32::from_be_bytes(*b"xtok"), tvector);
    native.cpu.gpr[3] = token;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportDisposeToken,
    );

    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(native.memory.read_u32_be(token), Some(0));
    assert_eq!(native.memory.read_u32_be(token + 4), Some(0));
    assert_eq!(context.memory_manager_mut().native_allocation(handle), None);
    assert!(native.apple_events.pending_token_disposals.is_empty());
}

#[test]
fn native_ppc_object_callbacks_keep_nil_slots_and_replace_present_slots() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"AESetObjectCallbacks");
    let mut native = load_pef_application(&pef).unwrap();
    let target = PpcImportDispatcherTarget::ObjectSupportSetCallbacks;
    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "AESetObjectCallbacks"),
        target,
    );
    native.cpu.gpr[3..=9].copy_from_slice(&[0x1000, 0x2000, 0, 0, 0, 0, 0]);
    run_test_import(&mut native, target.clone());
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert!(native.apple_events.object_callbacks.is_empty());

    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportInit);
    native.cpu.gpr[3..=9].copy_from_slice(&[0x1000, 0x2000, 0, 0, 0, 0, 0]);
    run_test_import(&mut native, target.clone());
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        native
            .apple_events
            .object_callbacks
            .get(&u32::from_be_bytes(*b"cmpr")),
        Some(&0x1000),
    );
    assert_eq!(
        native
            .apple_events
            .object_callbacks
            .get(&u32::from_be_bytes(*b"cont")),
        Some(&0x2000),
    );

    native.cpu.gpr[3..=9].copy_from_slice(&[0, 0x3000, 0, 0, 0, 0, 0]);
    run_test_import(&mut native, target.clone());
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        native
            .apple_events
            .object_callbacks
            .get(&u32::from_be_bytes(*b"cmpr")),
        Some(&0x1000),
    );
    assert_eq!(
        native
            .apple_events
            .object_callbacks
            .get(&u32::from_be_bytes(*b"cont")),
        Some(&0x3000),
    );
    native.cpu.gpr[3..=9].copy_from_slice(&[0, 0, 0, 0x3001, 0, 0, 0]);
    run_test_import(&mut native, target);
    assert_eq!(native.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(native.apple_events.object_callbacks.len(), 2);
}

#[test]
fn object_support_creates_signed_offset_descriptors() {
    assert_eq!(
        dispatcher_target_for_import("ObjectSupportLib", "CreateOffsetDescriptor"),
        PpcImportDispatcherTarget::ObjectSupportCreateOffsetDescriptor,
    );
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"CreateOffsetDescriptor");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let descriptors = PPC_DATA_BASE + 0x2400;
    native.memory.add_region(descriptors, vec![0; 16]);

    for (index, offset) in [1i32, -1].into_iter().enumerate() {
        let result_ptr = descriptors + index as u32 * 8;
        native.cpu.gpr[3] = offset as u32;
        native.cpu.gpr[4] = result_ptr;
        run_test_import(
            &mut native,
            PpcImportDispatcherTarget::ObjectSupportCreateOffsetDescriptor,
        );
        assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
        let descriptor = dispatch_apple_events::ppc_ae_descriptor(
            &mut native.memory,
            &native.apple_events.descriptors,
            result_ptr,
        )
        .unwrap();
        assert_eq!(descriptor.desc_type, u32::from_be_bytes(*b"long"));
        assert_eq!(descriptor.data, offset.to_be_bytes());
        let handle = native.memory.read_u32_be(result_ptr + 4).unwrap();
        assert_eq!(context.memory_manager_mut().native_allocation(handle).unwrap().size, 4);
    }

    native.cpu.gpr[3] = 1;
    native.cpu.gpr[4] = 0;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportCreateOffsetDescriptor,
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_PARAM_ERR);
}

#[test]
fn object_specifier_descriptors_are_immediately_process_owned() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"CreateObjSpecifier");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let detached = context.memory_manager_mut().detached_clone();
    let descriptor = PPC_DATA_BASE + 0x2400;
    native.memory.add_region(descriptor, vec![0; 8]);
    let container_ptr = PPC_DATA_BASE + 0x2410;
    let key_data_ptr = PPC_DATA_BASE + 0x2420;
    native.memory.add_region(container_ptr, vec![0; 24]);
    native.memory.write_u32_be(container_ptr, PPC_TYPE_NULL);
    native
        .memory
        .write_u32_be(key_data_ptr, u32::from_be_bytes(*b"TEXT"));
    native.apple_events.descriptors.with_mut(|state| {
        state.descriptors.insert(
            container_ptr,
            ProcessAeDescriptor {
                desc_type: PPC_TYPE_NULL,
                ..Default::default()
            },
        );
        state.descriptors.insert(
            key_data_ptr,
            ProcessAeDescriptor {
                desc_type: u32::from_be_bytes(*b"TEXT"),
                data: b"Example".to_vec(),
                ..Default::default()
            },
        );
    });
    native.cpu.gpr[3] = u32::from_be_bytes(*b"docu");
    native.cpu.gpr[4] = container_ptr;
    native.cpu.gpr[5] = u32::from_be_bytes(*b"name");
    native.cpu.gpr[6] = key_data_ptr;
    native.cpu.gpr[8] = descriptor;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportCompatibility,
    );

    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    assert_eq!(
        native.memory.read_u32_be(descriptor),
        Some(u32::from_be_bytes(*b"obj "))
    );
    let handle = native.memory.read_u32_be(descriptor + 4).unwrap();
    let allocation = context
        .memory_manager_mut()
        .native_allocation(handle)
        .unwrap();
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, allocation.ptr, allocation.size),
        Some(
            [
                u32::from_be_bytes(*b"docu").to_be_bytes(),
                u32::from_be_bytes(*b"name").to_be_bytes(),
                key_data_ptr.to_be_bytes(),
            ]
            .concat()
        )
    );
    assert_eq!(
        context.memory_manager_mut().recover_handle(allocation.ptr),
        Some(handle)
    );
    assert_eq!(detached.native_allocation(handle), None);
    let specifier = dispatch_apple_events::ppc_ae_descriptor(
        &mut native.memory,
        &native.apple_events.descriptors,
        descriptor,
    )
    .expect("created object specifier");
    assert_eq!(
        specifier.fields[&u32::from_be_bytes(*b"from")].desc_type,
        PPC_TYPE_NULL
    );
    assert_eq!(
        specifier.fields[&u32::from_be_bytes(*b"seld")].data,
        b"Example"
    );

    let token = PPC_DATA_BASE + 0x2430;
    native.memory.add_region(token, vec![0; 8]);
    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportInit);
    native.cpu.gpr[3] = descriptor;
    native.cpu.gpr[4] = 0;
    native.cpu.gpr[5] = token;
    run_test_import(&mut native, PpcImportDispatcherTarget::ObjectSupportResolve);
    assert_eq!(
        native.cpu.gpr[3],
        ppc_i16_result(PPC_ERR_AE_ACCESSOR_NOT_FOUND)
    );
    assert_eq!(native.memory.read_u32_be(token), Some(PPC_TYPE_NULL));
}

#[test]
fn object_specifier_disposes_inputs_when_requested() {
    let pef = synthetic_pef_with_library_import(b"ObjectSupportLib", b"CreateObjSpecifier");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let result = PPC_DATA_BASE + 0x2600;
    let container = result + 0x10;
    let key_data = result + 0x20;
    let source = result + 0x30;
    native.memory.add_region(result, vec![0; 0x40]);
    native.memory.write_u32_be(container, PPC_TYPE_NULL);
    native.memory.write_bytes(source, b"Example").unwrap();
    native.apple_events.descriptors.with_mut(|state| {
        state.descriptors.insert(
            container,
            ProcessAeDescriptor {
                desc_type: PPC_TYPE_NULL,
                ..Default::default()
            },
        );
    });
    native.cpu.gpr[3] = u32::from_be_bytes(*b"TEXT");
    native.cpu.gpr[4] = source;
    native.cpu.gpr[5] = 7;
    native.cpu.gpr[6] = key_data;
    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateDesc,
        ),
    );
    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    let input_handle = native.memory.read_u32_be(key_data + 4).unwrap();
    assert!(context
        .memory_manager_mut()
        .native_allocation(input_handle)
        .is_some());
    native.cpu.gpr[3] = u32::from_be_bytes(*b"docu");
    native.cpu.gpr[4] = container;
    native.cpu.gpr[5] = u32::from_be_bytes(*b"name");
    native.cpu.gpr[6] = key_data;
    native.cpu.gpr[7] = 1;
    native.cpu.gpr[8] = result;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::ObjectSupportCompatibility,
    );

    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    assert_eq!(native.memory.read_u32_be(container), Some(0));
    assert_eq!(native.memory.read_u32_be(key_data), Some(0));
    assert_eq!(
        context.memory_manager_mut().native_allocation(input_handle),
        None
    );
    let specifier = dispatch_apple_events::ppc_ae_descriptor(
        &mut native.memory,
        &native.apple_events.descriptors,
        result,
    )
    .expect("object specifier survives input disposal");
    assert_eq!(
        specifier.fields[&u32::from_be_bytes(*b"seld")].data,
        b"Example"
    );
}

#[test]
fn apple_event_records_use_process_owned_semantic_handles() {
    let pef = synthetic_pef_with_library_import(b"InterfaceLib", b"AECreateAppleEvent");
    let mut native = load_pef_application(&pef).unwrap();
    let mut context = ProcessContext::default();
    native.attach_unconverted_process_services(&mut context);
    let descriptor = PPC_DATA_BASE + 0x2500;
    native.memory.add_region(descriptor, vec![0; 8]);
    native.cpu.gpr[3] = u32::from_be_bytes(*b"misc");
    native.cpu.gpr[4] = u32::from_be_bytes(*b"slct");
    native.cpu.gpr[8] = descriptor;

    run_test_import(
        &mut native,
        PpcImportDispatcherTarget::AppleEventCompatibility(
            PpcAppleEventCompatibilityOperation::CreateAppleEvent,
        ),
    );

    assert_eq!(native.cpu.gpr[3] as u16 as i16, PPC_NO_ERR);
    assert_eq!(
        native.memory.read_u32_be(descriptor),
        Some(u32::from_be_bytes(*b"aevt"))
    );
    let handle = native.memory.read_u32_be(descriptor + 4).unwrap();
    let allocation = context
        .memory_manager_mut()
        .native_allocation(handle)
        .unwrap();
    assert_eq!(allocation.size, 8);
    assert_eq!(
        ppc_memory_read_bytes(&mut native.memory, allocation.ptr, allocation.size),
        Some(b"miscslct".to_vec())
    );
    assert_eq!(native.memory.read_u32_be(handle), Some(allocation.ptr));
    assert_eq!(
        context.memory_manager_mut().recover_handle(allocation.ptr),
        Some(handle)
    );
}
