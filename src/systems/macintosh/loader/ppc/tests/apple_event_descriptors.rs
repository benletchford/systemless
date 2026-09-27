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
    native.memory.add_region(scratch, vec![0; 0x80]);
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
    native.cpu.gpr[3] = u32::from_be_bytes(*b"docu");
    native.cpu.gpr[5] = u32::from_be_bytes(*b"name");
    native.cpu.gpr[6] = 0x1234_5678;
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
                0x1234_5678u32.to_be_bytes(),
            ]
            .concat()
        )
    );
    assert_eq!(
        context.memory_manager_mut().recover_handle(allocation.ptr),
        Some(handle)
    );
    assert_eq!(detached.native_allocation(handle), None);
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
