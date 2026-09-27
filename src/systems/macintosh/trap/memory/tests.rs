use super::super::test_helpers::{setup, setup_with_trap_tables, MockCpu, TEST_SP};
use crate::cpu::{CpuOps, Register};
use crate::memory::globals::addr;
use crate::memory::{GuestAddressSpace, MemoryBus};
use crate::process_context::{
    ProcessContext, ProcessHandleRecord, ProcessNativeHeapState, ProcessPtrRecord,
};
use crate::trap::dispatch::{
    LoadedResources, ResourceFileMap, TrapTableProfile, TOOLBOX_TRAP_TABLE_BASE,
};
use crate::trap::manager::COME_FROM_PATCH_SIGNATURE;
use std::collections::HashMap;

// ==================== OS Traps (is_tool=false) ====================

fn call_trap_word(
    dispatcher: &mut super::super::TrapDispatcher,
    trap_word: u16,
    cpu: &mut MockCpu,
    bus: &mut crate::memory::MacMemoryBus,
) -> crate::Result<()> {
    dispatcher.dispatch(trap_word, cpu, bus)
}

#[test]
fn standalone_classic_traps_retain_the_process_memory_manager() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let memory_manager = dispatcher.process_memory_manager();

    cpu.write_reg(Register::D0, 24);
    dispatcher.current_trap_word = 0xA11E;
    dispatcher
        .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
        .expect("NewPtr should be handled")
        .unwrap();
    let ptr = cpu.read_reg(Register::A0);

    assert_ne!(ptr, 0);
    assert_eq!(
        memory_manager.borrow().classic_allocation_size(ptr),
        Some(24)
    );
    assert!(dispatcher.process_memory_manager().ptr_eq(&memory_manager));
}

#[test]
fn classic_allocation_traps_mutate_the_process_owned_heap() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    dispatcher.attach_unconverted_process_services(&mut context);

    cpu.write_reg(Register::D0, 24);
    let memory_manager = context.memory_manager_handle();
    dispatcher
        .with_process_state(|dispatcher| {
            dispatcher.current_trap_word = 0xA11E;
            dispatcher
                .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
                .expect("NewPtr should be handled")
        })
        .unwrap();
    let ptr = cpu.read_reg(Register::A0);
    assert_ne!(ptr, 0);
    assert_eq!(
        memory_manager.borrow().classic_allocation_size(ptr),
        Some(24)
    );

    cpu.write_reg(Register::D0, 13);
    let memory_manager = context.memory_manager_handle();
    dispatcher
        .with_process_state(|dispatcher| {
            dispatcher.current_trap_word = 0xA022;
            dispatcher
                .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
                .expect("NewHandle should be handled")
        })
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    let data_ptr = bus.read_long(handle);
    assert_ne!(handle, 0);
    assert_ne!(data_ptr, 0);
    assert_eq!(
        memory_manager.borrow().classic_allocation_size(handle),
        Some(4)
    );
    assert_eq!(
        memory_manager.borrow().classic_allocation_size(data_ptr),
        Some(13)
    );
    assert_eq!(
        memory_manager.borrow().handle_for_ptr(data_ptr),
        Some(handle)
    );

    cpu.write_reg(Register::A0, ptr);
    dispatcher.current_trap_word = 0xA021;
    dispatcher
        .dispatch_memory(false, 0x21, &mut cpu, &mut bus)
        .expect("GetPtrSize should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 24);

    cpu.write_reg(Register::A0, handle);
    dispatcher.current_trap_word = 0xA025;
    dispatcher
        .dispatch_memory(false, 0x25, &mut cpu, &mut bus)
        .expect("GetHandleSize should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 13);

    cpu.write_reg(Register::A0, ptr);
    let memory_manager = context.memory_manager_handle();
    dispatcher
        .with_process_state(|dispatcher| {
            dispatcher.current_trap_word = 0xA01F;
            dispatcher
                .dispatch_memory(false, 0x1F, &mut cpu, &mut bus)
                .expect("DisposePtr should be handled")
        })
        .unwrap();
    assert_eq!(memory_manager.borrow().classic_allocation_size(ptr), None);

    cpu.write_reg(Register::A0, handle);
    let memory_manager = context.memory_manager_handle();
    dispatcher
        .with_process_state(|dispatcher| {
            dispatcher.current_trap_word = 0xA023;
            dispatcher
                .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
                .expect("DisposeHandle should be handled")
        })
        .unwrap();
    assert_eq!(
        memory_manager.borrow().classic_allocation_size(handle),
        None
    );
    assert_eq!(
        memory_manager.borrow().classic_allocation_size(data_ptr),
        None
    );
}

#[test]
fn set_handle_size_trap_updates_native_process_allocation_immediately() {
    const HEAP_BASE: u32 = 0x0300_0000;
    let handle = HEAP_BASE;
    let old_ptr = HEAP_BASE + 0x10;
    let heap_cursor = HEAP_BASE + 0x40;
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut native = GuestAddressSpace::new();
    native.add_region(HEAP_BASE, vec![0; 0x1000]);
    let shared = native.shared_view();
    bus.attach_guest_address_space(shared);
    bus.write_long(handle, old_ptr);
    bus.write_bytes(old_ptr, b"original");

    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    let memory_manager = context.memory_manager_handle().clone();
    {
        let mut manager = memory_manager.borrow_mut();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle,
                ptr: old_ptr,
                size: 8,
                capacity: 16,
            },
            0,
        )]);
    }
    dispatcher.attach_unconverted_process_services(&mut context);

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 48);
    dispatcher.current_trap_word = 0xA024;
    dispatcher
        .dispatch_memory(false, 0x24, &mut cpu, &mut bus)
        .expect("SetHandleSize should be handled")
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(handle), heap_cursor);
    assert_eq!(bus.read_bytes(heap_cursor, 8), b"original");
    assert_eq!(
        memory_manager.borrow().native_allocation(handle),
        Some(ProcessHandleRecord {
            handle,
            ptr: heap_cursor,
            size: 48,
            capacity: 48,
        })
    );
    assert_eq!(
        memory_manager.borrow().recover_handle(heap_cursor),
        Some(handle)
    );
}

#[test]
fn reallocate_handle_trap_replaces_native_process_allocation_immediately() {
    // Memory (1992), pp. 2-52--2-53: reallocation replaces the data
    // block, gives it undefined contents, and clears lock/purge state
    // without changing the stable handle.
    const HEAP_BASE: u32 = 0x0300_0000;
    let handle = HEAP_BASE;
    let old_ptr = HEAP_BASE + 0x10;
    let heap_cursor = HEAP_BASE + 0x80;
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut native = GuestAddressSpace::new();
    native.add_region(HEAP_BASE, vec![0; 0x1000]);
    let shared = native.shared_view();
    bus.attach_guest_address_space(shared);
    bus.write_long(handle, old_ptr);
    bus.write_bytes(old_ptr, b"original");

    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    let memory_manager = context.memory_manager_handle().clone();
    {
        let mut manager = memory_manager.borrow_mut();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle,
                ptr: old_ptr,
                size: 8,
                capacity: 64,
            },
            0xE0,
        )]);
    }
    dispatcher.attach_unconverted_process_services(&mut context);

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 17);
    dispatcher.current_trap_word = 0xA027;
    dispatcher
        .dispatch_memory(false, 0x27, &mut cpu, &mut bus)
        .expect("ReallocateHandle should be handled")
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(handle), heap_cursor);
    assert_eq!(bus.read_bytes(heap_cursor, 17), vec![0xA5; 17]);
    assert_eq!(
        memory_manager.borrow().native_allocation(handle),
        Some(ProcessHandleRecord {
            handle,
            ptr: heap_cursor,
            size: 17,
            capacity: 17,
        })
    );
    assert_eq!(memory_manager.borrow().recover_handle(old_ptr), None);
    assert_eq!(
        memory_manager.borrow().recover_handle(heap_cursor),
        Some(handle)
    );
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x20));
    assert_eq!(bus.read_bytes(old_ptr, 8), b"original");
}

#[test]
fn reallocate_empty_ordinary_handle_preserves_unloaded_resource_entries() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.install_test_resource(&mut bus, *b"Prfl", 128, b"preferences");
    dispatcher.insert_resource_pointer_for_test(0, (*b"Prfl", 128), 0);
    dispatcher.insert_named_resource_for_test(0, (*b"Prfl", "Preferences".to_string()), (128, 0));

    dispatcher
        .dispatch_memory(false, 0x66, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let ordinary_handle = cpu.read_reg(Register::A0);
    assert_eq!(bus.read_long(ordinary_handle), 0);

    cpu.write_reg(Register::A0, ordinary_handle);
    cpu.write_reg(Register::D0, 32);
    dispatcher
        .dispatch_memory(false, 0x27, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_ne!(bus.read_long(ordinary_handle), 0);
    let file = &dispatcher.resources.as_ref().unwrap().files[&0];
    assert_eq!(file.loaded[&(*b"Prfl", 128)], 0);
    assert_eq!(file.named[&(*b"Prfl", "Preferences".to_string())], (128, 0));
}

#[test]
fn empty_handle_trap_updates_native_process_allocation_immediately() {
    // Memory (1992), pp. 2-51--2-52: EmptyHandle frees an unlocked
    // relocatable block, preserves its master pointer slot, and writes
    // NIL into that slot.
    const HEAP_BASE: u32 = 0x0300_0000;
    let handle = HEAP_BASE;
    let old_ptr = HEAP_BASE + 0x20;
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut native = GuestAddressSpace::new();
    native.add_region(HEAP_BASE, vec![0; 0x1000]);
    let shared = native.shared_view();
    bus.attach_guest_address_space(shared);
    bus.write_long(handle, old_ptr);
    bus.write_bytes(old_ptr, b"original");

    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    let memory_manager = context.memory_manager_handle().clone();
    {
        let mut manager = memory_manager.borrow_mut();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle,
                ptr: old_ptr,
                size: 8,
                capacity: 32,
            },
            0x60,
        )]);
    }
    dispatcher.attach_unconverted_process_services(&mut context);

    cpu.write_reg(Register::A0, handle);
    dispatcher.current_trap_word = 0xA02B;
    dispatcher
        .dispatch_memory(false, 0x2B, &mut cpu, &mut bus)
        .expect("EmptyHandle should be handled")
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(handle), 0);
    assert_eq!(
        memory_manager.borrow().native_allocation(handle),
        Some(ProcessHandleRecord {
            handle,
            ptr: 0,
            size: 0,
            capacity: 0,
        })
    );
    assert_eq!(memory_manager.borrow().recover_handle(old_ptr), None);
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x60));
    assert_eq!(
        memory_manager
            .borrow()
            .native_allocator()
            .and_then(|allocator| allocator.free_ptr_blocks.last())
            .copied(),
        Some(ProcessPtrRecord {
            ptr: old_ptr,
            size: 32,
        })
    );
}

#[test]
fn dispose_handle_trap_releases_native_process_allocation_immediately() {
    // Memory (1992), pp. 2-34--2-35: DisposeHandle releases both the
    // relocatable block and its master pointer for reuse.
    const HEAP_BASE: u32 = 0x0300_0000;
    let handle = HEAP_BASE;
    let ptr = HEAP_BASE + 0x20;
    let record = ProcessHandleRecord {
        handle,
        ptr,
        size: 8,
        capacity: 32,
    };
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut native = GuestAddressSpace::new();
    native.add_region(HEAP_BASE, vec![0; 0x1000]);
    let shared = native.shared_view();
    bus.attach_guest_address_space(shared);
    bus.write_long(handle, ptr);
    bus.write_bytes(ptr, b"original");

    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    let memory_manager = context.memory_manager_handle().clone();
    {
        let mut manager = memory_manager.borrow_mut();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(record, 0xE0)]);
    }
    dispatcher.attach_unconverted_process_services(&mut context);

    cpu.write_reg(Register::A0, handle);
    dispatcher.current_trap_word = 0xA023;
    dispatcher
        .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
        .expect("DisposeHandle should be handled")
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_long(handle), 0);
    assert_eq!(memory_manager.borrow().native_allocation(handle), None);
    assert_eq!(memory_manager.borrow().recover_handle(ptr), None);
    assert_eq!(memory_manager.borrow().state_for_handle(handle), None);
    assert_eq!(
        memory_manager
            .borrow()
            .native_allocator()
            .and_then(|allocator| allocator.free_handle_blocks.last())
            .copied(),
        Some(record)
    );
}

#[test]
fn handle_copy_traps_update_native_process_allocations_immediately() {
    // Inside Macintosh: Memory (1992), pp. 2-60--2-66: the handle-copy
    // routines preserve stable destination handles, and HandToHand creates
    // its copy in the source block's heap zone.
    const HEAP_BASE: u32 = 0x0300_0000;
    const SOURCE_HANDLE: u32 = HEAP_BASE;
    const SOURCE_PTR: u32 = HEAP_BASE + 0x20;
    const REPLACEMENT_PTR: u32 = 0x0030_0000;
    const APPEND_PTR: u32 = REPLACEMENT_PTR + 0x40;
    let source_record = ProcessHandleRecord {
        handle: SOURCE_HANDLE,
        ptr: SOURCE_PTR,
        size: 6,
        capacity: 16,
    };
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut native = GuestAddressSpace::new();
    native.add_region(HEAP_BASE, vec![0; 0x2000]);
    let shared = native.shared_view();
    bus.attach_guest_address_space(shared);
    bus.write_long(SOURCE_HANDLE, SOURCE_PTR);
    bus.write_bytes(SOURCE_PTR, b"native");

    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    let memory_manager = context.memory_manager_handle().clone();
    {
        let mut manager = memory_manager.borrow_mut();
        manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: HEAP_BASE + 0x2000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[],
            &[],
            &[],
        );
        manager.register_native_handle_records([(source_record, 0xE0)]);
    }
    let detached = memory_manager.detached_clone();
    dispatcher.attach_unconverted_process_services(&mut context);

    cpu.write_reg(Register::A0, SOURCE_HANDLE);
    dispatcher
        .dispatch_memory(true, 0x1E1, &mut cpu, &mut bus)
        .expect("HandToHand should be handled")
        .unwrap();
    let copy_handle = cpu.read_reg(Register::A0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_ne!(copy_handle, SOURCE_HANDLE);
    let copied = memory_manager
        .borrow()
        .native_allocation(copy_handle)
        .expect("HandToHand should publish the native copy immediately");
    assert_eq!(bus.read_bytes(copied.ptr, copied.size as usize), b"native");
    assert_eq!(
        memory_manager.borrow().state_for_handle(copy_handle),
        Some(0)
    );

    bus.write_bytes(REPLACEMENT_PTR, b"process-owned-bytes");
    cpu.write_reg(Register::A0, REPLACEMENT_PTR);
    cpu.write_reg(Register::A1, copy_handle);
    cpu.write_reg(Register::D0, 19);
    dispatcher
        .dispatch_memory(true, 0x1E2, &mut cpu, &mut bus)
        .expect("PtrToXHand should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);

    bus.write_bytes(APPEND_PTR, b"++");
    cpu.write_reg(Register::A0, APPEND_PTR);
    cpu.write_reg(Register::A1, copy_handle);
    cpu.write_reg(Register::D0, 2);
    dispatcher
        .dispatch_memory(true, 0x1EF, &mut cpu, &mut bus)
        .expect("PtrAndHand should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::A0, SOURCE_HANDLE);
    cpu.write_reg(Register::A1, copy_handle);
    dispatcher
        .dispatch_memory(true, 0x1E4, &mut cpu, &mut bus)
        .expect("HandAndHand should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    let appended = memory_manager
        .borrow()
        .native_allocation(copy_handle)
        .expect("concatenation should retain the native allocation record");
    assert_eq!(appended.size, 27);
    assert_eq!(bus.read_long(copy_handle), appended.ptr);
    assert_eq!(
        bus.read_bytes(appended.ptr, appended.size as usize),
        b"process-owned-bytes++native"
    );
    assert_eq!(
        memory_manager.borrow().recover_handle(appended.ptr),
        Some(copy_handle)
    );

    let detached = detached.borrow();
    assert_eq!(
        detached.native_allocation(SOURCE_HANDLE),
        Some(source_record)
    );
    assert_eq!(detached.native_allocation(copy_handle), None);
}

#[test]
fn classic_handle_metadata_is_process_visible_before_slice_return() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    dispatcher.attach_unconverted_process_services(&mut context);

    cpu.write_reg(Register::D0, 12);
    dispatcher.current_trap_word = 0xA022;
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .expect("NewHandle should be handled")
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    let ptr = bus.read_long(handle);
    assert_eq!(context.handle_for_ptr(ptr), Some(handle));

    cpu.write_reg(Register::A0, handle);
    dispatcher.current_trap_word = 0xA029;
    dispatcher
        .dispatch_memory(false, 0x29, &mut cpu, &mut bus)
        .expect("HLock should be handled")
        .unwrap();
    assert_eq!(context.memory_manager_mut().handle_state(handle), 0x80);
}

#[test]
fn classic_size_and_dispose_traps_observe_native_process_allocations() {
    const HEAP_BASE: u32 = 0x0300_0000;
    const HANDLE: u32 = HEAP_BASE;
    const HANDLE_PTR: u32 = HEAP_BASE + 0x10;
    const PTR: u32 = HEAP_BASE + 0x80;
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut context = ProcessContext::default();
    context.attach_classic_memory_bus(&mut bus);
    bus.write_long(HANDLE, HANDLE_PTR);
    {
        let mut memory_manager = context.memory_manager_mut();
        memory_manager.publish_native_allocator(
            ProcessNativeHeapState {
                heap_base: HEAP_BASE,
                heap_cursor: HEAP_BASE + 0x100,
                heap_limit: HEAP_BASE + 0x1000,
                last_mem_error: 0,
                heap_maximized: false,
                master_pointer_blocks_requested: 0,
            },
            &[ProcessPtrRecord { ptr: PTR, size: 37 }],
            &[],
            &[],
        );
        memory_manager.register_native_handle_records([(
            ProcessHandleRecord {
                handle: HANDLE,
                ptr: HANDLE_PTR,
                size: 48,
                capacity: 64,
            },
            0,
        )]);
    }
    dispatcher.attach_unconverted_process_services(&mut context);
    assert_eq!(bus.get_alloc_size(HANDLE_PTR), None);
    assert_eq!(bus.get_alloc_size(PTR), None);

    cpu.write_reg(Register::A0, HANDLE);
    dispatcher.current_trap_word = 0xA025;
    dispatcher
        .dispatch_memory(false, 0x25, &mut cpu, &mut bus)
        .expect("GetHandleSize should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 48);

    cpu.write_reg(Register::A0, PTR);
    dispatcher.current_trap_word = 0xA021;
    dispatcher
        .dispatch_memory(false, 0x21, &mut cpu, &mut bus)
        .expect("GetPtrSize should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 37);

    cpu.write_reg(Register::A0, PTR);
    cpu.write_reg(Register::D0, 20);
    dispatcher.current_trap_word = 0xA020;
    dispatcher
        .dispatch_memory(false, 0x20, &mut cpu, &mut bus)
        .expect("SetPtrSize should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        context.memory_manager_mut().process_ptr_size(&bus, PTR),
        Some(20)
    );

    cpu.write_reg(Register::A0, PTR);
    dispatcher.current_trap_word = 0xA01F;
    dispatcher
        .dispatch_memory(false, 0x1F, &mut cpu, &mut bus)
        .expect("DisposePtr should be handled")
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    let memory_manager = context.memory_manager_mut();
    assert_eq!(memory_manager.process_ptr_size(&bus, PTR), None);
    assert_eq!(
        memory_manager
            .native_allocator()
            .and_then(|allocator| allocator.free_ptr_blocks.last())
            .copied(),
        Some(ProcessPtrRecord { ptr: PTR, size: 20 })
    );
}

#[test]
fn memorydispatch_generated_selector_routes_are_sorted_unique_and_complete() {
    assert_eq!(super::MEMORY_DISPATCH_OPERATION_ROUTES.len(), 5);
    assert!(super::MEMORY_DISPATCH_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));
    assert_eq!(super::MEMORY_DISPATCH_A0_RESULT_OPERATION_ROUTES.len(), 1);

    let hold = super::memory_dispatch_operation_route(0xA05C, 0x0000).expect("HoldMemory route");
    assert_eq!(hold.routine_name, "HoldMemory");
    assert_eq!(
        hold.operation_id,
        "selector-operation:_MemoryDispatch:0x0000:d0-moveq-immediate:8"
    );

    let physical =
        super::memory_dispatch_operation_route(0xA15C, 0x0005).expect("GetPhysical route");
    assert_eq!(physical.routine_name, "GetPhysical");
    assert_eq!(
        physical.operation_id,
        "selector-operation:_MemoryDispatchA0Result:0x0005:d0-moveq-immediate:8"
    );

    assert!(super::memory_dispatch_operation_route(0xA05C, 0x0005).is_none());
    assert!(super::memory_dispatch_operation_route(0xA15C, 0x0000).is_none());
    assert!(super::memory_dispatch_operation_route(0xA05C, 0x0001_0000).is_none());
}

#[test]
fn hwpriv_generated_selector_routes_are_sorted_unique_and_complete() {
    assert_eq!(super::HWPRIV_OPERATION_ROUTES.len(), 3);
    assert!(super::HWPRIV_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    for trap_word in [0xA098, 0xA198] {
        let flush = super::hwpriv_operation_route(trap_word, 0x0001).expect("flush route");
        assert_eq!(flush.routine_name, "FlushInstructionCache");
        assert_eq!(
            flush.operation_id,
            "selector-operation:_HWPriv:0x0001:d0-moveq-immediate:8"
        );
    }

    assert!(super::hwpriv_operation_route(0xA298, 0x0001).is_none());
    assert!(super::hwpriv_operation_route(0xA098, 0x7001).is_none());
    assert!(super::hwpriv_operation_route(0xA098, 0x0001_0001).is_none());
}

#[test]
fn hwpriv_records_every_generated_operation_for_both_source_backed_trap_forms() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    for trap_word in [0xA098, 0xA198] {
        for route in super::HWPRIV_OPERATION_ROUTES {
            cpu.write_reg(Register::D0, u32::from(route.selector));
            call_trap_word(&mut dispatcher, trap_word, &mut cpu, &mut bus).unwrap();
            assert_eq!(
                dispatcher.current_selector_operation,
                Some(route.operation_id)
            );
        }
    }

    for (trap_word, selector) in [(0xA298, 0x0001), (0xA098, 0x7001), (0xA098, 0x0001_0001)] {
        cpu.write_reg(Register::D0, selector);
        call_trap_word(&mut dispatcher, trap_word, &mut cpu, &mut bus).unwrap();
        assert_eq!(dispatcher.current_selector_operation, None);
    }
}

#[test]
fn memorydispatch_records_every_generated_operation_for_its_exact_trap_form() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    for route in super::MEMORY_DISPATCH_OPERATION_ROUTES {
        cpu.write_reg(Register::D0, u32::from(route.selector));
        cpu.write_reg(Register::A0, 0);
        cpu.write_reg(Register::A1, 0);
        dispatcher.dispatch(0xA05C, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            dispatcher.current_selector_operation,
            Some(route.operation_id)
        );
    }

    let get_physical = &super::MEMORY_DISPATCH_A0_RESULT_OPERATION_ROUTES[0];
    cpu.write_reg(Register::D0, u32::from(get_physical.selector));
    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::A1, 0);
    dispatcher.dispatch(0xA15C, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        dispatcher.current_selector_operation,
        Some(get_physical.operation_id)
    );

    cpu.write_reg(Register::D0, 0x0005);
    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::A1, 0);
    dispatcher.dispatch(0xA05C, &mut cpu, &mut bus).unwrap();
    assert_eq!(dispatcher.current_selector_operation, None);

    cpu.write_reg(Register::D0, 0x0001_0000);
    dispatcher.dispatch(0xA05C, &mut cpu, &mut bus).unwrap();
    assert_eq!(dispatcher.current_selector_operation, None);
}

#[test]
fn test_new_ptr() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 256);
    let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewPtr should be handled");
    assert!(result.unwrap().is_ok(), "NewPtr should succeed");
    let ptr = cpu.read_reg(Register::A0);
    assert!(
        ptr >= 0x200000,
        "NewPtr should return a valid heap pointer, got ${:08X}",
        ptr
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "NewPtr should set D0 to 0 (noErr)"
    );
}

#[test]
fn negative_new_ptr_size_returns_mem_full_without_heap_wrap() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    bus.write_long(addr::J_CRSR_TASK, 0x1234_5678);
    cpu.write_reg(Register::D0, (-108i32) as u32);

    let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);

    assert!(result.is_some() && result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), (-108i32) as u32);
    assert_eq!(bus.read_long(addr::J_CRSR_TASK), 0x1234_5678);
}

#[test]
fn negative_new_handle_size_returns_mem_full_without_heap_wrap() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    bus.write_long(addr::J_CRSR_TASK, 0x1234_5678);
    cpu.write_reg(Register::D0, (-108i32) as u32);

    let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);

    assert!(result.is_some() && result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), (-108i32) as u32);
    assert_eq!(bus.read_long(addr::J_CRSR_TASK), 0x1234_5678);
}

#[test]
fn new_ptr_extends_stale_application_zone_past_successful_allocation() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let app_zone = 0x0020_0000;
    let old_limit = 0x0020_1000;
    let allocation_start = 0x0020_2000;
    let size = 0x100;
    let original_free = 0x800;

    bus.write_long(addr::APP_L_ZONE, app_zone);
    bus.write_long(addr::THE_ZONE, app_zone);
    bus.write_long(addr::HEAP_END, app_zone + 64);
    bus.write_long(addr::APPL_LIMIT, old_limit);
    bus.write_long(addr::BUF_PTR, old_limit);
    bus.write_long(app_zone, old_limit); // bkLim
    bus.write_long(app_zone + 12, original_free); // zcbFree
    bus.reserve_heap_until(allocation_start);

    // $A11E is the ordinary NewPtr entry point used by MPW clients.
    dispatcher.current_trap_word = 0xA11E;
    cpu.write_reg(Register::D0, size);
    let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewPtr should be handled");
    assert!(result.unwrap().is_ok(), "NewPtr should succeed");

    let ptr = cpu.read_reg(Register::A0);
    let allocation_end = ptr + size;
    assert_eq!(ptr, allocation_start);
    assert_eq!(
        bus.read_long(app_zone),
        allocation_end,
        "bkLim must remain immediately beyond every successful application-zone allocation"
    );
    assert_eq!(bus.read_long(addr::APPL_LIMIT), allocation_end);
    assert_eq!(bus.read_long(addr::BUF_PTR), allocation_end);
    assert_eq!(bus.read_long(addr::HEAP_END), allocation_end);
    assert_eq!(
        bus.read_long(app_zone + 12),
        original_free,
        "already-consumed backing span must not be added to zcbFree"
    );
    assert!(
        ptr >= app_zone && ptr < bus.read_long(app_zone),
        "a successful NewPtr result must pass the classic [zone, bkLim) validity test"
    );
}

#[test]
fn new_ptr_refuses_to_cross_the_active_68k_stack() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let allocation_start = 0x0020_2000;
    let stack_pointer = 0x0020_2800;
    let sentinel = 0x1234_5678;

    bus.reserve_heap_until(allocation_start);
    bus.write_long(stack_pointer, sentinel);
    cpu.write_reg(Register::A7, stack_pointer);
    cpu.write_reg(Register::D0, 0x1000);
    dispatcher.current_trap_word = 0xA11E;

    let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(result.is_some() && result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A0), 0);
    assert_eq!(cpu.read_reg(Register::D0), super::MEM_FULL_ERR);
    assert_eq!(bus.read_long(stack_pointer), sentinel);
}

#[test]
fn new_ptr_scribbles_every_byte_and_new_ptr_clear_zeroes_every_byte() {
    // Regular NewPtr leaves contents undefined; systemless scribbles 0xA5
    // so an application relying on zeroed memory misbehaves here as it
    // would on a real machine. NewPtrClear guarantees zeros. Both fills
    // must cover the whole block, first byte to last, at odd sizes.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    for (trap_word, size, expected) in [(0xA11Eu16, 0x1235u32, 0xA5u8), (0xA31E, 0x0FFF, 0x00)] {
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::D0, size);
        let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
        assert!(result.is_some() && result.unwrap().is_ok());
        let ptr = cpu.read_reg(Register::A0);
        assert!(ptr != 0);
        let block = bus.read_bytes(ptr, size as usize);
        assert!(
            block.iter().all(|&b| b == expected),
            "trap ${trap_word:04X} size {size:#x}: every byte must be {expected:#04x}"
        );
    }
}

#[test]
fn new_ptr_sys_does_not_extend_application_zone() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let app_zone = 0x0020_0000;
    let old_limit = 0x0020_1000;
    let allocation_start = 0x0020_2000;

    bus.write_long(addr::APP_L_ZONE, app_zone);
    bus.write_long(addr::THE_ZONE, app_zone);
    bus.write_long(addr::HEAP_END, old_limit);
    bus.write_long(addr::APPL_LIMIT, old_limit);
    bus.write_long(app_zone, old_limit); // bkLim
    bus.reserve_heap_until(allocation_start);

    dispatcher.current_trap_word = 0xA51E; // NewPtrSys
    cpu.write_reg(Register::D0, 0x100);
    let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(result.is_some() && result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A0), allocation_start);
    assert_eq!(
        bus.read_long(app_zone),
        old_limit,
        "system-heap allocations must not widen the application zone"
    );
    assert_eq!(bus.read_long(addr::APPL_LIMIT), old_limit);
    assert_eq!(bus.read_long(addr::HEAP_END), old_limit);
}

#[test]
fn test_new_handle() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 512);
    let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewHandle should be handled");
    assert!(result.unwrap().is_ok(), "NewHandle should succeed");
    let handle = cpu.read_reg(Register::A0);
    assert!(
        handle >= 0x200000,
        "NewHandle should return a valid handle address, got ${:08X}",
        handle
    );
    let ptr = bus.read_long(handle);
    assert!(
        ptr >= 0x200000,
        "NewHandle's handle should point to a valid ptr, got ${:08X}",
        ptr
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "NewHandle should set D0 to 0 (noErr)"
    );
}

#[test]
fn new_handle_clear_clears_stale_memerr_on_success() {
    // Inside Macintosh Volume IV, IV-80: Memory Manager routines store
    // the most recent result code in the low-memory MemErr word.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    bus.write_word(addr::MEM_ERR, 0x1E6C);
    dispatcher.current_trap_word = 0xA322; // NewHandleClear
    cpu.write_reg(Register::D0, 68);

    let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewHandleClear should be handled");
    assert!(result.unwrap().is_ok(), "NewHandleClear should succeed");
    assert_ne!(cpu.read_reg(Register::A0), 0, "allocation should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "D0 should report noErr");
    assert_eq!(
        bus.read_word(addr::MEM_ERR),
        0,
        "NewHandleClear must clear stale MemErr on success"
    );
}

#[test]
fn new_allocations_follow_clear_contracts() {
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 64);
    dispatcher.current_trap_word = 0xA01E; // NewPtr
    let ptr_result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(ptr_result.is_some() && ptr_result.unwrap().is_ok());
    let ptr = cpu.read_reg(Register::A0);
    assert_ne!(
        bus.read_long(ptr),
        0,
        "regular NewPtr allocations should not be zero-filled"
    );

    cpu.write_reg(Register::D0, 64);
    dispatcher.current_trap_word = 0xA31E; // NewPtrClear
    let clear_ptr_result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(clear_ptr_result.is_some() && clear_ptr_result.unwrap().is_ok());
    let clear_ptr = cpu.read_reg(Register::A0);
    assert_eq!(
        bus.read_long(clear_ptr),
        0,
        "NewPtrClear allocations should be zero-filled"
    );

    cpu.write_reg(Register::D0, 64);
    dispatcher.current_trap_word = 0xA022; // NewHandle
    let handle_result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(handle_result.is_some() && handle_result.unwrap().is_ok());
    let handle = cpu.read_reg(Register::A0);
    let data_ptr = bus.read_long(handle);
    assert_ne!(
        bus.read_long(data_ptr),
        0,
        "regular NewHandle allocations should not be zero-filled"
    );

    cpu.write_reg(Register::D0, 64);
    dispatcher.current_trap_word = 0xA322; // NewHandleClear
    let clear_handle_result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(clear_handle_result.is_some() && clear_handle_result.unwrap().is_ok());
    let clear_handle = cpu.read_reg(Register::A0);
    let clear_data_ptr = bus.read_long(clear_handle);
    assert_eq!(
        bus.read_long(clear_data_ptr),
        0,
        "NewHandleClear allocations should be zero-filled"
    );
}

#[test]
fn test_dispose_ptr() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x1F, &mut cpu, &mut bus);
    assert!(result.is_some(), "DisposePtr should be handled");
    assert!(result.unwrap().is_ok(), "DisposePtr should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DisposePtr should set D0 to 0"
    );
}

#[test]
fn test_dispose_handle() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
    assert!(result.is_some(), "DisposeHandle should be handled");
    assert!(result.unwrap().is_ok(), "DisposeHandle should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DisposeHandle should set D0 to 0"
    );
}

#[test]
fn dispose_resource_handle_preserves_resource_backing_for_later_lookup() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let data_ptr = bus.alloc(4);
    bus.write_bytes(data_ptr, &[0xDE, 0xAD, 0xBE, 0xEF]);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);

    dispatcher.insert_loaded_resource_handle_for_test(handle, (data_ptr, *b"RSRC", 7));
    dispatcher.insert_resource_handle_file_for_test(handle, 0);
    dispatcher.set_loaded_resources_for_test(LoadedResources {
        files: HashMap::from([(
            0,
            ResourceFileMap {
                loaded: HashMap::from([((*b"RSRC", 7), data_ptr)]),
                named: HashMap::new(),
                names_by_id: HashMap::new(),
                attrs: HashMap::new(),
                map_attrs: 0,
            },
        )]),
        names: HashMap::new(),
        search_order: vec![0],
        current_file: 0,
    });

    cpu.write_reg(Register::A0, handle);
    let result = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
    assert!(result.is_some(), "DisposeHandle should be handled");
    assert!(result.unwrap().is_ok(), "DisposeHandle should return");

    assert_eq!(
        bus.get_alloc_size(handle),
        None,
        "DisposeHandle should still release the master-pointer slot"
    );
    assert_eq!(
        bus.read_bytes(data_ptr, 4),
        vec![0xDE, 0xAD, 0xBE, 0xEF],
        "resource backing must remain valid after its handle is disposed"
    );
    assert!(!dispatcher.loaded_handles.contains_key(&handle));
    assert!(!dispatcher.resource_handle_files.contains_key(&handle));
    assert_eq!(
        dispatcher.find_loaded_resource_any(*b"RSRC", 7),
        Some((0, data_ptr)),
        "Resource Manager map should still point at live backing data"
    );

    let new_handle = dispatcher.get_or_create_resource_handle(&mut bus, *b"RSRC", 7, data_ptr);
    assert_ne!(new_handle, 0);
    assert_eq!(bus.read_long(new_handle), data_ptr);
}

#[test]
fn test_hlock() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x29, &mut cpu, &mut bus);
    assert!(result.is_some(), "HLock should be handled");
    assert!(result.unwrap().is_ok(), "HLock should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "HLock should set D0 to 0");
}

#[test]
fn test_hunlock() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x2A, &mut cpu, &mut bus);
    assert!(result.is_some(), "HUnlock should be handled");
    assert!(result.unwrap().is_ok(), "HUnlock should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "HUnlock should set D0 to 0");
}

#[test]
fn classic_handle_state_traps_mutate_process_manager_immediately() {
    // Memory (1992), pp. 2-46--2-51: handle state belongs to the process,
    // and HSetState changes lock/purge properties without clearing the
    // Resource Manager's ownership bit.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 16);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    let memory_manager = dispatcher.process_memory_manager();
    let detached = memory_manager.detached_clone();

    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x67, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    dispatcher
        .dispatch_memory(false, 0x29, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    dispatcher
        .dispatch_memory(false, 0x49, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0xE0));

    cpu.write_reg(Register::D0, 0);
    dispatcher
        .dispatch_memory(false, 0x6A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(memory_manager.borrow().state_for_handle(handle), Some(0x20));
    assert_eq!(
        detached.borrow().state_for_handle(handle),
        Some(0),
        "detached fresh NewHandle keeps the neutral initial state"
    );
}

#[test]
fn test_heapdispatch_sets_noerr_and_ccr_like_other_memory_dispatchers() {
    let (mut dispatcher, mut cpu, mut bus) = setup();

    // Seed the CCR with non-zero flags so we can prove the trap
    // rewrites it instead of leaving stale condition codes behind.
    cpu.set_ccr(0x1F);

    let result = dispatcher.dispatch_memory(false, 0xA4, &mut cpu, &mut bus);
    assert!(result.is_some(), "HeapDispatch should be handled");
    assert!(result.unwrap().is_ok(), "HeapDispatch should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HeapDispatch should set D0 to 0"
    );
    assert_eq!(
        cpu.get_ccr() & 0x1F,
        0x14,
        "HeapDispatch should leave X set and Z asserted for the noErr path"
    );
}

fn assert_nohw_trap_preserves_d0_and_stack_and_updates_ccr(
    dispatcher: &mut crate::trap::TrapDispatcher,
    cpu: &mut impl CpuOps,
    bus: &mut crate::memory::MacMemoryBus,
    trap_num: u16,
    trap_name: &str,
    d0_seed: u32,
    expected_ccr: u8,
) {
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::D0, d0_seed);
    cpu.set_ccr(0x1F);

    let result = dispatcher.dispatch_memory(false, trap_num, cpu, bus);
    assert!(result.is_some(), "{trap_name} should be handled");
    assert!(result.unwrap().is_ok(), "{trap_name} should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        d0_seed,
        "{trap_name} should preserve D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "{trap_name} should not consume a Pascal argument frame"
    );
    assert_eq!(
        cpu.get_ccr() & 0x1F,
        expected_ccr,
        "{trap_name} should mirror the preserved D0 into the dispatcher CCR"
    );
}

#[test]
fn vadbproc_preserves_d0_and_stack_and_updates_ccr() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    assert_nohw_trap_preserves_d0_and_stack_and_updates_ccr(
        &mut dispatcher,
        &mut cpu,
        &mut bus,
        0xAE,
        "VADBProc",
        0x1234_5678,
        0x10,
    );
}

#[test]
fn scsi_atomic_generated_routes_preserve_exact_moveq_values() {
    assert_eq!(super::SCSI_ATOMIC_OPERATION_ROUTES.len(), 5);
    assert!(super::SCSI_ATOMIC_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    for (selector, routine_name) in [
        (1, "SCSIAction"),
        (2, "SCSIRegisterBus"),
        (3, "SCSIDeregisterBus"),
        (4, "SCSIReregisterBus"),
        (5, "SCSIKillXPT"),
    ] {
        let route =
            super::scsi_atomic_operation_route(0xA089, selector).expect("fixed SCSIAtomic route");
        assert_eq!(route.routine_name, routine_name);
    }

    for (trap_word, selector) in [
        (0xA189, 1),
        (0xA089, 0),
        (0xA089, 6),
        (0xA089, 0x0000_7001),
        (0xA089, 0x0001_0001),
    ] {
        assert!(super::scsi_atomic_operation_route(trap_word, selector).is_none());
    }
}

#[test]
fn scsi_atomic_dispatch_records_fixed_routes_only() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0);

    for route in super::SCSI_ATOMIC_OPERATION_ROUTES {
        cpu.write_reg(Register::D0, route.selector);
        dispatcher.dispatch(0xA089, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            dispatcher.current_selector_operation,
            Some(route.operation_id)
        );
    }

    for selector in [0, 6, 0x0000_7001, 0x0001_0001] {
        cpu.write_reg(Register::D0, selector);
        dispatcher.dispatch(0xA089, &mut cpu, &mut bus).unwrap();
        assert_eq!(dispatcher.current_selector_operation, None);
    }
}

#[test]
fn scsiaction_scsigetvirtualidinfo_missing_virtual_id_clears_exists_and_preserves_stack() {
    let (mut dispatcher, mut cpu, mut bus) = setup();

    const PB_SIZE: u32 = 40;
    const PB_LENGTH_OFFSET: usize = 6;
    const PB_FUNCTION_CODE_OFFSET: usize = 8;
    const PB_RESULT_OFFSET: usize = 10;
    const PB_OLD_CALL_ID_OFFSET: usize = 36;
    const PB_EXISTS_OFFSET: usize = 38;
    const SCSI_GET_VIRTUAL_ID_INFO_FUNCTION_CODE: u8 = 0x80;

    let scsi_pb = bus.alloc(PB_SIZE);
    let sp_before = cpu.read_reg(Register::A7);
    let mut i: usize = 0;

    while i < PB_SIZE as usize {
        bus.write_byte(scsi_pb + i as u32, 0);
        i += 1;
    }
    bus.write_word(scsi_pb + PB_LENGTH_OFFSET as u32, PB_SIZE as u16);
    bus.write_byte(
        scsi_pb + PB_FUNCTION_CODE_OFFSET as u32,
        SCSI_GET_VIRTUAL_ID_INFO_FUNCTION_CODE,
    );
    bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);
    bus.write_word(scsi_pb + PB_OLD_CALL_ID_OFFSET as u32, 0x1234);
    bus.write_byte(scsi_pb + PB_EXISTS_OFFSET as u32, 0xFF);

    cpu.write_reg(Register::A0, scsi_pb);
    cpu.write_reg(Register::D0, 0x0000_0001);

    let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
    assert!(result.is_some(), "SCSIAtomic should be handled");
    assert!(result.unwrap().is_ok(), "SCSIAtomic should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SCSIAtomic should return noErr"
    );
    assert_eq!(
        bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32),
        0,
        "SCSIAction should mirror noErr into scsiResult"
    );
    assert_eq!(
        bus.read_byte(scsi_pb + PB_EXISTS_OFFSET as u32),
        0,
        "SCSIGetVirtualIDInfo should clear scsiExists for a missing virtual ID"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SCSIAction should preserve StackSpace"
    );
}

#[test]
fn scsiaction_scsigetvirtualidinfo_rejects_nonzero_qlink_with_qlink_invalid() {
    // Inside Macintosh: Devices (1994), pp. 4-49 to 4-50 and 4-21:
    // qLink is part of the common SCSIAction header and must be 0.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    const PB_SIZE: u32 = 40;
    const PB_RESULT_OFFSET: usize = 10;
    const PB_EXISTS_OFFSET: usize = 38;

    let scsi_pb = bus.alloc(PB_SIZE);
    let sp_before = cpu.read_reg(Register::A7);

    for i in 0..PB_SIZE as usize {
        bus.write_byte(scsi_pb + i as u32, 0);
    }
    bus.write_long(scsi_pb, 0x0000_0001);
    bus.write_word(scsi_pb + 6, PB_SIZE as u16);
    bus.write_byte(scsi_pb + 8, 0x80);
    bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);
    bus.write_byte(scsi_pb + PB_EXISTS_OFFSET as u32, 0xFF);

    cpu.write_reg(Register::A0, scsi_pb);
    cpu.write_reg(Register::D0, 0x0000_0001);

    let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
    assert!(result.is_some(), "SCSIAtomic should be handled");
    assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -7881,
        "SCSIAtomic should return scsiQLinkInvalid for a nonzero qLink"
    );
    assert_eq!(
        bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32) as i16,
        -7881,
        "SCSIAction should mirror scsiQLinkInvalid into scsiResult"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SCSIAction should preserve StackSpace on qLink errors"
    );
    assert_eq!(
        bus.read_byte(scsi_pb + PB_EXISTS_OFFSET as u32),
        0xFF,
        "SCSIGetVirtualIDInfo should leave scsiExists untouched on qLink errors"
    );
}

#[test]
fn scsiaction_scsigetvirtualidinfo_rejects_short_parameter_block_with_length_error() {
    // Inside Macintosh: Devices (1994), pp. 4-49 to 4-50 and 4-21:
    // SCSIAction checks the parameter block length before attempting
    // to use SCSIGetVirtualIDInfo-specific fields.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    const PB_SIZE: u32 = 40;
    const PB_RESULT_OFFSET: usize = 10;
    const PB_EXISTS_OFFSET: usize = 38;

    let scsi_pb = bus.alloc(PB_SIZE);
    let sp_before = cpu.read_reg(Register::A7);

    for i in 0..PB_SIZE as usize {
        bus.write_byte(scsi_pb + i as u32, 0);
    }
    bus.write_word(scsi_pb + 6, 8);
    bus.write_byte(scsi_pb + 8, 0x80);
    bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);
    bus.write_byte(scsi_pb + PB_EXISTS_OFFSET as u32, 0xFF);

    cpu.write_reg(Register::A0, scsi_pb);
    cpu.write_reg(Register::D0, 0x0000_0001);

    let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
    assert!(result.is_some(), "SCSIAtomic should be handled");
    assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -7872,
        "SCSIAtomic should return scsiPBLengthError for a short parameter block"
    );
    assert_eq!(
        bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32) as i16,
        -7872,
        "SCSIAction should mirror scsiPBLengthError into scsiResult"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SCSIAction should preserve StackSpace on PB length errors"
    );
    assert_eq!(
        bus.read_byte(scsi_pb + PB_EXISTS_OFFSET as u32),
        0xFF,
        "SCSIGetVirtualIDInfo should leave scsiExists untouched on PB length errors"
    );
}

#[test]
fn scsiaction_scsigetvirtualidinfo_rejects_non_nil_completion_with_request_invalid() {
    // Inside Macintosh: Devices (1994), pp. 4-49 to 4-50 and 4-21:
    // SCSIGetVirtualIDInfo is synchronous, so scsiCompletion must be nil.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    const PB_SIZE: u32 = 40;
    const PB_RESULT_OFFSET: usize = 10;
    const PB_COMPLETION_OFFSET: usize = 16;

    let scsi_pb = bus.alloc(PB_SIZE);

    for i in 0..PB_SIZE as usize {
        bus.write_byte(scsi_pb + i as u32, 0);
    }
    bus.write_word(scsi_pb + 6, PB_SIZE as u16);
    bus.write_byte(scsi_pb + 8, 0x80);
    bus.write_long(scsi_pb + PB_COMPLETION_OFFSET as u32, 0x0000_1234);
    bus.write_word(scsi_pb + PB_RESULT_OFFSET as u32, 0x3FFF);

    cpu.write_reg(Register::A0, scsi_pb);
    cpu.write_reg(Register::D0, 0x0000_0001);

    let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
    assert!(result.is_some(), "SCSIAtomic should be handled");
    assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -7870,
        "SCSIAtomic should return scsiRequestInvalid for a non-nil completion"
    );
    assert_eq!(
        bus.read_word(scsi_pb + PB_RESULT_OFFSET as u32) as i16,
        -7870,
        "SCSIAction should mirror scsiRequestInvalid into scsiResult"
    );
}

#[test]
fn scsiaction_scsigetvirtualidinfo_rejects_nil_parameter_block_with_request_invalid() {
    // Inside Macintosh: Devices (1994), pp. 4-38 to 4-39 and p. 4-90:
    // SCSIAction takes a pointer to a SCSI Manager parameter block, so
    // a NIL scsiPB is an invalid request.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    let sp_before = cpu.read_reg(Register::A7);

    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::D0, 0x0000_0001);

    let result = dispatcher.dispatch_memory(false, 0x89, &mut cpu, &mut bus);
    assert!(result.is_some(), "SCSIAtomic should be handled");
    assert!(result.unwrap().is_ok(), "SCSIAtomic should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -7870,
        "SCSIAtomic should return scsiRequestInvalid for a NIL parameter block"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SCSIAction should preserve StackSpace on a NIL parameter block"
    );
}

#[test]
fn power_manager_modifier_forms_preserve_stack_and_manager_state() {
    // Inside Macintosh: Devices (1994), pp. 6-29--6-30 and 6-33--6-35;
    // Universal Interfaces 3.4 Power.h lines 650--701 and 733--791.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    dispatcher.set_tick_count_for_test(&mut bus, 0x1234_5678);
    dispatcher.dispatch(0xA285, &mut cpu, &mut bus).unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0x1234_5678);
    assert_eq!(dispatcher.power_idle_last_update_tick, 0x1234_5678);

    for selector in [1, 2] {
        cpu.write_reg(Register::D0, selector);
        dispatcher.dispatch(0xA585, &mut cpu, &mut bus).unwrap();
    }
    assert_eq!(dispatcher.power_idle_disable_count, 2);
    cpu.write_reg(Register::D0, 0);
    dispatcher.dispatch(0xA485, &mut cpu, &mut bus).unwrap();
    assert_eq!(dispatcher.power_idle_disable_count, 1);
    cpu.write_reg(Register::D0, u32::MAX);
    dispatcher.dispatch(0xA485, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        cpu.read_reg(Register::D0),
        crate::machine_profile::REFERENCE_MACHINE_PROFILE.realtime_cpu_mhz as u32
    );

    for (selector, a_power, b_power) in [
        (0x04u32, true, false),
        (0x00, true, true),
        (0xFFFF_FF84, false, true),
        (0xFFFF_FF80, false, false),
    ] {
        cpu.write_reg(Register::D0, selector);
        dispatcher.dispatch(0xA785, &mut cpu, &mut bus).unwrap();
        assert_eq!(dispatcher.serial_port_a_powered, a_power);
        assert_eq!(dispatcher.serial_port_b_powered, b_power);
    }
    assert_eq!(cpu.read_reg(Register::A7), sp_before);
}

#[test]
fn power_control_generated_routes_preserve_exact_moveq_values() {
    assert_eq!(super::IDLE_STATE_OPERATION_ROUTES.len(), 1);
    assert_eq!(super::SERIAL_POWER_OPERATION_ROUTES.len(), 5);
    assert!(super::SERIAL_POWER_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    let enable = super::power_control_operation_route(0xA485, 0).expect("EnableIdle route");
    assert_eq!(enable.routine_name, "EnableIdle");
    assert_eq!(
        enable.operation_id,
        "selector-operation:_IdleState:0x0000:d0-moveq-immediate:8"
    );

    let a_off = super::power_control_operation_route(0xA685, 0xFFFF_FF84).expect("AOff route");
    assert_eq!(a_off.routine_name, "AOff");
    assert_eq!(
        a_off.operation_id,
        "selector-operation:_SerialPower:0xFF84:d0-moveq-immediate:8"
    );

    assert!(super::power_control_operation_route(0xA585, 0).is_none());
    assert!(super::power_control_operation_route(0xA485, 1).is_none());
    assert!(super::power_control_operation_route(0xA485, u32::MAX).is_none());
    assert!(super::power_control_operation_route(0xA785, 0xFFFF_FF84).is_none());
    assert!(super::power_control_operation_route(0xA685, 0x0000_FF84).is_none());
    assert!(super::power_control_operation_route(0xA685, 0x0000_7084).is_none());
    assert!(super::power_control_operation_route(0xA685, 0x0001_0004).is_none());
}

#[test]
fn power_control_dispatch_records_fixed_routes_and_leaves_ranges_unregistered() {
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 0);
    dispatcher.dispatch(0xA485, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        dispatcher.current_selector_operation,
        Some(super::IDLE_STATE_OPERATION_ROUTES[0].operation_id)
    );

    for selector in [1, u32::MAX] {
        cpu.write_reg(Register::D0, selector);
        dispatcher.dispatch(0xA485, &mut cpu, &mut bus).unwrap();
        assert_eq!(dispatcher.current_selector_operation, None);
    }

    for route in super::SERIAL_POWER_OPERATION_ROUTES {
        cpu.write_reg(Register::D0, route.selector);
        dispatcher.dispatch(0xA685, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            dispatcher.current_selector_operation,
            Some(route.operation_id)
        );
    }

    for (trap_word, selector) in [
        (0xA585, 0),
        (0xA785, 0xFFFF_FF84),
        (0xA685, 0x0000_FF84),
        (0xA685, 0x0000_7084),
        (0xA685, 0x0001_0004),
    ] {
        cpu.write_reg(Register::D0, selector);
        dispatcher.dispatch(trap_word, &mut cpu, &mut bus).unwrap();
        assert_eq!(dispatcher.current_selector_operation, None);
    }
}

fn assert_no_hardware_trap_returns_noerr_and_preserves_stack(trap_num: u16, trap_name: &str) {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xA5A5_5A5A);

    cpu.write_reg(Register::D0, 0xFFFF_FFFF);
    let result = dispatcher.dispatch_memory(false, trap_num, &mut cpu, &mut bus);
    assert!(result.is_some(), "{trap_name} should be handled");
    assert!(result.unwrap().is_ok(), "{trap_name} should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "{trap_name} should return noErr"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "{trap_name} should preserve the caller stack pointer"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xA5A5_5A5A,
        "{trap_name} should not consume or rewrite stack arguments"
    );
}

#[test]
fn iopinfoaccess_returns_noerr_and_preserves_stack_pointer() {
    // The generic target has no IOP hardware, so the HLE path is the
    // same no-hardware compatibility return used by IOPMoveData.
    assert_no_hardware_trap_returns_noerr_and_preserves_stack(0x86, "IOPInfoAccess");
}

#[test]
fn iopmsgrequest_returns_noerr_and_preserves_stack_pointer() {
    assert_no_hardware_trap_returns_noerr_and_preserves_stack(0x87, "IOPMsgRequest");
}

#[test]
fn egretdispatch_returns_noerr_and_preserves_stack_pointer() {
    assert_no_hardware_trap_returns_noerr_and_preserves_stack(0x92, "EgretDispatch");
}

#[test]
fn hsetrbit_sets_resource_flag_for_valid_handle() {
    // Inside Macintosh: Memory (1992), pp. 2-49 to 2-50:
    // HSetRBit sets a handle's resource flag and returns noErr.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let data_ptr = bus.alloc(16);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);
    let sp_before = cpu.read_reg(Register::A7);

    cpu.write_reg(Register::A0, handle);
    let result = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut bus);
    assert!(result.is_some(), "HSetRBit should be handled");
    assert!(result.unwrap().is_ok(), "HSetRBit should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HSetRBit should return noErr for a valid handle"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "HSetRBit uses register calling convention and should preserve A7"
    );
    assert_eq!(
        dispatcher.handle_state_bits(handle).unwrap_or(0) & 0x20,
        0x20,
        "HSetRBit should set the resource bit in tracked handle state"
    );
}

#[test]
fn hsetrbit_nil_handle_returns_nilhandleerr_in_d0() {
    // Inside Macintosh: Memory (1992), pp. 2-49 to 2-50:
    // nilHandleErr (-109) is returned for a NIL master pointer.
    let (mut dispatcher, mut cpu, mut _bus) = setup();
    cpu.write_reg(Register::A0, 0);

    let result = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut _bus);
    assert!(result.is_some(), "HSetRBit should be handled");
    assert!(result.unwrap().is_ok(), "HSetRBit should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -109,
        "HSetRBit should return nilHandleErr for a NIL handle"
    );
}

#[test]
fn hclrrbit_clears_resource_flag_for_valid_handle() {
    // Inside Macintosh: Memory (1992), pp. 2-50 to 2-51:
    // HClrRBit clears a handle's resource flag and returns noErr.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let data_ptr = bus.alloc(16);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);
    dispatcher.set_handle_state_bits(handle, 0x20);
    let sp_before = cpu.read_reg(Register::A7);

    cpu.write_reg(Register::A0, handle);
    let result = dispatcher.dispatch_memory(false, 0x68, &mut cpu, &mut bus);
    assert!(result.is_some(), "HClrRBit should be handled");
    assert!(result.unwrap().is_ok(), "HClrRBit should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HClrRBit should return noErr for a valid handle"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "HClrRBit uses register calling convention and should preserve A7"
    );
    assert_eq!(
        dispatcher.handle_state_bits(handle).unwrap_or(0) & 0x20,
        0,
        "HClrRBit should clear the resource bit in tracked handle state"
    );
}

#[test]
fn hclrrbit_nil_handle_returns_nilhandleerr_in_d0() {
    // Inside Macintosh: Memory (1992), pp. 2-50 to 2-51:
    // nilHandleErr (-109) is returned for a NIL master pointer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0);

    let result = dispatcher.dispatch_memory(false, 0x68, &mut cpu, &mut bus);
    assert!(result.is_some(), "HClrRBit should be handled");
    assert!(result.unwrap().is_ok(), "HClrRBit should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -109,
        "HClrRBit should return nilHandleErr for a NIL handle"
    );
}

#[test]
fn hsetrbit_then_hgetstate_round_trips_resource_bit() {
    // Pins the documented HSetRBit ↔ HGetState round-trip contract
    // (BasiliskII System 7.5.3 ROM). Per IM:Memory 1992 p. 2-43, callers
    // observe handle state ONLY through HGetState ($A069); the master
    // pointer flag byte is not portable across 24/32-bit modes. This
    // test exercises that documented API: after HSetRBit on a freshly
    // allocated handle (whose pre-state is 0x00 per IM:Memory 1992
    // p. 2-27), HGetState must return the byte with bit 5 (0x20) set.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let data_ptr = bus.alloc(16);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);

    // Pre: HGetState reports clear state.
    cpu.write_reg(Register::A0, handle);
    let pre = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
    assert!(pre.is_some() && pre.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0) & 0x20,
        0,
        "fresh NewHandle should leave the resource bit clear per IM:Memory 1992 p. 2-27"
    );

    // HSetRBit on the handle.
    cpu.write_reg(Register::A0, handle);
    let set = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut bus);
    assert!(set.is_some() && set.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HSetRBit on a valid handle should return noErr"
    );

    // Post: HGetState reports the resource bit set.
    cpu.write_reg(Register::A0, handle);
    let post = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
    assert!(post.is_some() && post.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0) & 0x20,
        0x20,
        "HGetState should report the resource bit set after HSetRBit"
    );
}

#[test]
fn hclrrbit_after_hsetrbit_round_trips_resource_bit_to_zero() {
    // Symmetric counterpart to hsetrbit_then_hgetstate_round_trips_resource_bit:
    // pins the HClrRBit ↔ HGetState round-trip. After HSetRBit then HClrRBit
    // on the same handle, HGetState must report the resource bit
    // cleared (0x00) — per IM:Memory 1992 p. 2-50 "HClrRBit clears
    // the resource flag of a relocatable block."
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let data_ptr = bus.alloc(16);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);

    cpu.write_reg(Register::A0, handle);
    let set = dispatcher.dispatch_memory(false, 0x67, &mut cpu, &mut bus);
    assert!(set.is_some() && set.unwrap().is_ok());

    cpu.write_reg(Register::A0, handle);
    let mid = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
    assert!(mid.is_some() && mid.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0) & 0x20,
        0x20,
        "precondition: HSetRBit should leave the resource bit set"
    );

    cpu.write_reg(Register::A0, handle);
    let clear = dispatcher.dispatch_memory(false, 0x68, &mut cpu, &mut bus);
    assert!(clear.is_some() && clear.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HClrRBit on a valid handle should return noErr"
    );

    cpu.write_reg(Register::A0, handle);
    let post = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
    assert!(post.is_some() && post.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0) & 0x20,
        0,
        "HGetState should report the resource bit clear after HClrRBit"
    );
}

#[test]
fn readdatetime_returns_current_time_global_without_host_clock_reset() {
    // The runner advances low-memory Time ($020C). ReadDateTime should
    // expose that synthetic clock instead of replacing it with host time.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let secs = 0x1234_5678u32;
    let out_ptr = bus.alloc(4);
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(0x020C, secs);
    bus.write_long(out_ptr, 0);
    cpu.write_reg(Register::A0, out_ptr);

    let result = dispatcher.dispatch_memory(false, 0x39, &mut cpu, &mut bus);

    assert!(result.is_some(), "ReadDateTime should be handled");
    assert!(
        result.unwrap().is_ok(),
        "ReadDateTime should return cleanly"
    );
    assert_eq!(bus.read_long(out_ptr), secs);
    assert_eq!(bus.read_long(0x020C), secs);
    assert_eq!(cpu.read_reg(Register::D0), 0, "ReadDateTime returns noErr");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "ReadDateTime should use register calling convention and preserve A7"
    );
}

#[test]
fn setdatetime_updates_time_global_from_d0_seconds_argument() {
    // Inside Macintosh Volume II (1985), pp. II-378 to II-379:
    // _SetDateTime takes secs in D0 and updates the Time global.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let secs = 0x1234_5678u32;
    bus.write_long(0x020C, 0xDEAD_BEEF);
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::D0, secs);

    let result = dispatcher.dispatch_memory(false, 0x3A, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetDateTime should be handled");
    assert!(result.unwrap().is_ok(), "SetDateTime should return cleanly");
    assert_eq!(
        bus.read_long(0x020C),
        secs,
        "SetDateTime should copy D0 seconds into low-memory Time ($020C)"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SetDateTime should use register calling convention and preserve A7"
    );
}

#[test]
fn setdatetime_returns_noerr_result_code_in_d0_for_nominal_call() {
    // Inside Macintosh Volume II (1985), p. II-391 register summary:
    // _SetDateTime returns result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0x0102_0304);

    let result = dispatcher.dispatch_memory(false, 0x3A, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetDateTime should be handled");
    assert!(result.unwrap().is_ok(), "SetDateTime should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetDateTime nominal path should return noErr in D0"
    );
}

#[test]
fn setdatetime_returns_noerr_regardless_of_secs_input() {
    // Inside Macintosh Volume II (1985), pp. II-378..II-379 + II-391:
    // _SetDateTime takes D0=secs and returns D0=OSErr; both engines
    // return noErr on the nominal write path for any LongInt input.
    // Sweeps multiple secs values and checks the noErr return + A7
    // preservation.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let secs_inputs = [
        0u32, // 1904-01-01 epoch
        1,
        0x1234_5678,
        0x7FFF_FFFF, // max signed LongInt
        0xFFFF_FFFF, // sentinel high bits
    ];
    for &secs in &secs_inputs {
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::D0, secs);
        let result = dispatcher.dispatch_memory(false, 0x3A, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetDateTime should be handled");
        assert!(result.unwrap().is_ok(), "SetDateTime should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) & 0xFF,
            0,
            "SetDateTime should return noErr lowbyte for secs=0x{:08X}",
            secs,
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SetDateTime should preserve A7 (register-only ABI) for secs=0x{:08X}",
            secs,
        );
    }
}

#[test]
fn writeparam_returns_noerr_in_d0_for_nominal_call() {
    // Inside Macintosh Volume II (1985), pp. II-381 to II-382 and p. II-391:
    // _WriteParam returns result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0x01F8); // SysParam
    cpu.write_reg(Register::D0, 0xFFFF_FFFF); // MinusOne

    let result = dispatcher.dispatch_memory(false, 0x38, &mut cpu, &mut bus);
    assert!(result.is_some(), "WriteParam should be handled");
    assert!(result.unwrap().is_ok(), "WriteParam should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "WriteParam nominal path should return noErr in D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "WriteParam should use register calling convention and preserve A7"
    );
}

#[test]
fn writeparam_does_not_modify_low_memory_sysparam_copy() {
    // Inside Macintosh Volume II (1985), pp. II-381 to II-382:
    // WriteParam writes the existing low-memory SysParam copy to parameter RAM.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sys_param = 0x01F8u32;
    let mut before = [0u8; 20];
    for (i, slot) in before.iter_mut().enumerate() {
        let value = (i as u8).wrapping_mul(7).wrapping_add(3);
        bus.write_byte(sys_param + i as u32, value);
        *slot = value;
    }
    cpu.write_reg(Register::A0, sys_param);
    cpu.write_reg(Register::D0, 0xFFFF_FFFF);

    let result = dispatcher.dispatch_memory(false, 0x38, &mut cpu, &mut bus);
    assert!(result.is_some(), "WriteParam should be handled");
    assert!(result.unwrap().is_ok(), "WriteParam should return cleanly");
    for (i, expected) in before.iter().enumerate() {
        assert_eq!(
            bus.read_byte(sys_param + i as u32),
            *expected,
            "WriteParam should preserve low-memory SysParam byte {}",
            i
        );
    }
}

#[test]
fn writeparam_five_call_composition_preserves_stack_across_varying_minusone_register_state() {
    // 5 successive _WriteParam dispatches inside one A7-snapshot
    // sandwich with varying D0 entry
    // values (always logical "MinusOne" but seeded with varying high-byte
    // stale state to stress D0 input handling). Per IM:II II-381 +
    // IM:OSUtils 1994 p. 7-13 the OS-bit FUNCTION consumes no Pascal
    // stack arguments; A7 is unchanged across every call.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sys_param = 0x01F8u32;
    let sentinel = 0xBADC_0DE0u32;
    bus.write_long(sys_param + 16, sentinel); // SP+16 sentinel within SysParam record

    let a7_pre = cpu.read_reg(Register::A7);
    let d0_inputs = [
        0xFFFF_FFFFu32,
        0xFFFF_FFFEu32,
        0xFFFF_0000u32,
        0xFFFF_FFFFu32,
        0xFFFF_FF00u32,
    ];
    for d0_in in d0_inputs.iter().copied() {
        cpu.write_reg(Register::A0, sys_param);
        cpu.write_reg(Register::D0, d0_in);
        let result = dispatcher.dispatch_memory(false, 0x38, &mut cpu, &mut bus);
        assert!(result.is_some(), "WriteParam should be handled");
        assert!(result.unwrap().is_ok(), "WriteParam should return cleanly");
        assert_eq!(
                cpu.read_reg(Register::D0),
                0,
                "WriteParam nominal path should return noErr in D0 across varying MinusOne high-byte state"
            );
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        a7_pre,
        "Five WriteParam calls must net zero A7 delta (register-only OS-bit ABI)"
    );
    assert_eq!(
        bus.read_long(sys_param + 16),
        sentinel,
        "SysParam sentinel preserved across all five WriteParam calls"
    );
}

#[test]
fn initutil_returns_noerr_in_d0_for_nominal_call() {
    // Inside Macintosh Volume II (1985), pp. II-380 to II-381 and p. II-391:
    // _InitUtil exits with result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::D0, 0xFFFF_FFFF);

    let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitUtil should be handled");
    assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "InitUtil nominal path should return noErr in D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "InitUtil takes no stack arguments and should preserve A7"
    );
}

#[test]
fn initutil_sets_spvalid_byte_to_a8_on_success() {
    // Inside Macintosh Volume II (1985), pp. II-380 to II-381:
    // InitUtil initializes low-memory parameter RAM state from the clock chip.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    bus.write_byte(0x01F8, 0x00);

    let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitUtil should be handled");
    assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
    assert_eq!(
        bus.read_byte(0x01F8),
        0xA8,
        "InitUtil nominal path should mark SPValid ($01F8) as valid ($A8)"
    );
}

#[test]
fn initutil_rewrites_spvalid_when_pre_poisoned_to_invalid() {
    // Pre-poisons SPValid
    // ($01F8) with $5A (canonical "invalid" per IM:II II-380 — any
    // byte other than $A8 means parameter RAM has not been validated
    // since the last reset), dispatches _InitUtil, and verifies that
    // (a) D0 returns noErr (0), (b) SPValid was overwritten to $A8.
    // Pinning the rewrite-from-invalid path defeats no-op stubs that
    // would leave SPValid at its incoming value.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_byte(0x01F8, 0x5A);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitUtil should be handled");
    assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) & 0xFF,
        0,
        "InitUtil must return noErr (0) in D0 lowbyte after rewriting invalid SPValid"
    );
    assert_eq!(
        bus.read_byte(0x01F8),
        0xA8,
        "InitUtil must overwrite a pre-poisoned $5A SPValid with $A8 (valid stamp)"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "InitUtil takes no stack arguments and must preserve A7"
    );
}

#[test]
fn initutil_register_only_calling_convention_preserves_stack_pointer() {
    // Repeated register-only calls should not consume a stack frame
    // or move A7.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    for _ in 0..5 {
        let result = dispatcher.dispatch_memory(false, 0x3F, &mut cpu, &mut bus);
        assert!(result.is_some(), "InitUtil should be handled");
        assert!(result.unwrap().is_ok(), "InitUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0) & 0xFF,
            0,
            "InitUtil should return noErr in D0 lowbyte on each call"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InitUtil must preserve A7 across repeated register-only calls"
        );
    }
}

#[test]
fn initzone_uses_a0_parameter_block_and_returns_noerr_in_d0() {
    // Inside Macintosh: Memory (1992), pp. 2-86 to 2-87:
    // InitZone takes a parameter-block pointer in A0 and returns
    // the result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let param_block = bus.alloc(14);
    bus.write_long(param_block, 0x0030_0000); // startPtr
    bus.write_long(param_block + 4, 0x0038_0000); // limitPtr
    bus.write_word(param_block + 8, 4); // cMoreMasters
    bus.write_long(param_block + 10, 0); // pGrowZone
    cpu.write_reg(Register::A0, param_block);
    cpu.write_reg(Register::D0, 0xFACE_B00C);

    let result = dispatcher.dispatch_memory(false, 0x19, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitZone should be handled");
    assert!(result.unwrap().is_ok(), "InitZone should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "InitZone nominal path should return noErr in D0"
    );
}

#[test]
fn initzone_uses_register_calling_convention_without_stack_arguments() {
    // Inside Macintosh: Memory (1992), pp. 2-86 to 2-87:
    // InitZone uses A0 for its parameter block and does not document
    // a Pascal stack argument frame.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let param_block = bus.alloc(14);
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, param_block);

    let result = dispatcher.dispatch_memory(false, 0x19, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitZone should be handled");
    assert!(result.unwrap().is_ok(), "InitZone should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "InitZone should preserve A7 in its register calling convention"
    );
}

#[test]
fn initzone_initializes_zone_header_and_makes_startptr_current_zone() {
    // Inside Macintosh Volume II (1985), p. II-29:
    // InitZone creates a heap zone at startPtr..limitPtr, initializes
    // its visible header fields, and makes startPtr the current zone.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let param_block = bus.alloc(14);
    let start = 0x0030_0000;
    let limit = 0x0030_0400;
    let more_masters: u16 = 4;
    let grow_zone = 0x0012_3456;
    bus.write_long(param_block, start);
    bus.write_long(param_block + 4, limit);
    bus.write_word(param_block + 8, more_masters);
    bus.write_long(param_block + 10, grow_zone);
    bus.write_long(0x0118, 0x00AA_BBCC);
    cpu.write_reg(Register::A0, param_block);

    let result = dispatcher.dispatch_memory(false, 0x19, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitZone should be handled");
    assert!(result.unwrap().is_ok(), "InitZone should return cleanly");
    assert_eq!(bus.read_long(start), limit, "bkLim should equal limitPtr");
    assert_eq!(
        bus.read_long(start + 12),
        limit - start - (72 + (4 * more_masters as u32)),
        "zcbFree should match the documented initial free-byte formula"
    );
    assert_eq!(
        bus.read_long(start + 16),
        grow_zone,
        "gzProc should reflect pGrowZone from the parameter block"
    );
    assert_eq!(
        bus.read_word(start + 20),
        more_masters,
        "moreMast should reflect cMoreMasters from the parameter block"
    );
    assert_eq!(
        bus.read_long(0x0118),
        start,
        "InitZone should make startPtr the current zone via TheZone"
    );
}

#[test]
fn initapplzone_returns_noerr_result_code_in_d0() {
    // Inside Macintosh: Memory (1992), pp. 2-87 to 2-88:
    // InitApplZone returns a result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    let result = dispatcher.dispatch_memory(false, 0x2C, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitApplZone should be handled");
    assert!(
        result.unwrap().is_ok(),
        "InitApplZone should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "InitApplZone nominal path should return noErr in D0"
    );
}

#[test]
fn initapplzone_takes_no_arguments_and_preserves_stack_pointer() {
    // Inside Macintosh Volume II (1985), p. II-28:
    // InitApplZone is a no-argument procedure with D0 result code.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);

    let result = dispatcher.dispatch_memory(false, 0x2C, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitApplZone should be handled");
    assert!(
        result.unwrap().is_ok(),
        "InitApplZone should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "InitApplZone should preserve A7"
    );
}

#[test]
fn initapplzone_reinitializes_application_zone_and_makes_it_current() {
    // Inside Macintosh Volume II (1985), p. II-28:
    // InitApplZone initializes the application heap zone, clears its
    // grow-zone function, and makes ApplZone the current zone.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let appl_zone = 0x0020_0000;
    let appl_limit = 0x0028_0000;
    bus.write_long(0x02AA, appl_zone); // ApplZone
    bus.write_long(0x0130, appl_limit); // ApplLimit
    bus.write_long(0x0118, 0x00AA_BBCC); // TheZone
    bus.write_long(appl_zone + 16, 0xDEAD_BEEF); // gzProc

    let result = dispatcher.dispatch_memory(false, 0x2C, &mut cpu, &mut bus);
    assert!(result.is_some(), "InitApplZone should be handled");
    assert!(
        result.unwrap().is_ok(),
        "InitApplZone should return cleanly"
    );
    assert_eq!(
        bus.read_long(0x0118),
        appl_zone,
        "InitApplZone should make ApplZone the current zone"
    );
    assert_eq!(
        bus.read_long(appl_zone + 16),
        0,
        "InitApplZone should clear the application zone grow-zone pointer"
    );
    assert_eq!(
        bus.read_word(appl_zone + 20),
        64,
        "InitApplZone should restore the application zone moreMast increment"
    );
}

#[test]
fn setapplbase_uses_a0_startptr_and_returns_noerr_in_d0() {
    // Inside Macintosh: Memory (1992), pp. 2-88 to 2-89:
    // SetApplBase takes startPtr in A0 and returns result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x0040_0000);
    cpu.write_reg(Register::D0, 0x1234_5678);

    let result = dispatcher.dispatch_memory(false, 0x57, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetApplBase should be handled");
    assert!(result.unwrap().is_ok(), "SetApplBase should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetApplBase nominal path should return noErr in D0"
    );
}

#[test]
fn setapplbase_uses_register_calling_convention_without_stack_arguments() {
    // Inside Macintosh Volume II (1985), p. II-32 summary:
    // SetApplBase receives startPtr in A0 and should not pop stack args.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0x0040_0000);

    let result = dispatcher.dispatch_memory(false, 0x57, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetApplBase should be handled");
    assert!(result.unwrap().is_ok(), "SetApplBase should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SetApplBase should preserve A7"
    );
}

#[test]
fn setapplbase_returns_noerr_regardless_of_startptr_value() {
    // Inside Macintosh Volume II (1985), pp. II-28 to II-29:
    // SetApplBase returns noErr on the nominal path. Systemless HLE
    // does not model a relocatable application zone, so the trap
    // is a no-op that writes D0 = noErr regardless of startPtr.
    // Dispatching the trap with startPtr echoing the current
    // ApplZone low-memory pointer, this verifies that any other
    // startPtr value (NIL, sentinel, or arbitrary address) also
    // produces D0 = 0 in the HLE.
    let cases: [u32; 5] = [
        0x0000_0000, // NIL — "use default" per IM:II II-28
        0x0040_0000, // arbitrary 4MB address (matches the existing test)
        0xDEAD_BEEF, // sentinel (untouched by HLE)
        0x0000_0001, // tiny pointer
        0x7FFF_FFFE, // near-max signed pointer
    ];
    for (i, startptr) in cases.iter().enumerate() {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        cpu.write_reg(Register::A0, *startptr);
        cpu.write_reg(Register::D0, 0xCAFE_BABE);

        let result = dispatcher.dispatch_memory(false, 0x57, &mut cpu, &mut bus);
        assert!(result.is_some(), "SetApplBase[{i}] should be handled");
        assert!(
            result.unwrap().is_ok(),
            "SetApplBase[{i}] should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "SetApplBase[{i}] (startPtr=0x{startptr:08X}) should return noErr in D0"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "SetApplBase[{i}] should preserve A7"
        );
    }
}

#[test]
fn translate24to32_preserves_full_input_in_32bit_mode() {
    // BasiliskII System 7.5.3's default 32-bit-addressing path
    // returns the full input unchanged for tagged values.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0xAB12_3456);

    let result = dispatcher.dispatch_memory(false, 0x91, &mut cpu, &mut bus);
    assert!(result.is_some(), "Translate24To32 should be handled");
    assert!(
        result.unwrap().is_ok(),
        "Translate24To32 should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0xAB12_3456,
        "Translate24To32 should preserve the full tagged input in 32-bit mode"
    );
}

#[test]
fn swapmmumode_updates_mmu32bit_low_memory_flag() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    bus.write_byte(crate::memory::globals::addr::MMU32_BIT, 1);

    cpu.write_reg(Register::D0, 0);
    let result = dispatcher.dispatch_memory(false, 0x5D, &mut cpu, &mut bus);
    assert!(result.is_some(), "SwapMMUMode should be handled");
    assert!(result.unwrap().is_ok(), "SwapMMUMode should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        1,
        "default mode is true32b, so D0 returns the previous mode"
    );
    assert_eq!(
        bus.read_byte(crate::memory::globals::addr::MMU32_BIT),
        0,
        "MMU32Bit should mirror the requested 24-bit mode"
    );

    cpu.write_reg(Register::D0, 1);
    let result = dispatcher.dispatch_memory(false, 0x5D, &mut cpu, &mut bus);
    assert!(result.is_some(), "SwapMMUMode should be handled again");
    assert!(result.unwrap().is_ok(), "SwapMMUMode should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "D0 returns the previous false32b mode"
    );
    assert_eq!(
        bus.read_byte(crate::memory::globals::addr::MMU32_BIT),
        1,
        "MMU32Bit should mirror the requested 32-bit mode"
    );
}

#[test]
fn translate24to32_uses_d0_register_calling_convention_and_preserves_a7() {
    // Inside Macintosh: Memory (1992), p. 4-28:
    // Translate24To32 takes input in D0 and returns output in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::D0, 0x0012_3456);

    let result = dispatcher.dispatch_memory(false, 0x91, &mut cpu, &mut bus);
    assert!(result.is_some(), "Translate24To32 should be handled");
    assert!(
        result.unwrap().is_ok(),
        "Translate24To32 should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0x0012_3456,
        "Translate24To32 should return the same value for a 32-bit-clean address"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "Translate24To32 should preserve A7 in register calling convention"
    );
}

#[test]
fn vinstall_consumes_a0_taskptr_and_returns_oserr_in_d0() {
    // Inside Macintosh: Processes (1994), pp. 4-24 to 4-25:
    // VInstall takes task pointer in A0 and returns OSErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_long(task_ptr, 0xDEAD_BEEF);
    bus.write_word(task_ptr + 4, 1);
    bus.write_long(task_ptr + 6, 0x1234_5678);
    bus.write_word(task_ptr + 10, 2);
    bus.write_word(task_ptr + 12, 1);
    cpu.write_reg(Register::A0, task_ptr);

    let result = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
    assert!(result.is_some(), "VInstall should be handled");
    assert!(result.unwrap().is_ok(), "VInstall should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "VInstall should return noErr"
    );
    assert_eq!(
        bus.read_long(task_ptr),
        0,
        "single-task queue link should terminate at NIL"
    );
    assert_eq!(
        bus.read_long(super::VBL_QUEUE_HEADER + 2),
        dispatcher.callback_scheduling.system_vbl_queue_anchor(),
        "VInstall should keep the system-owned queue anchor at the head"
    );
    assert_eq!(
        bus.read_long(dispatcher.callback_scheduling.system_vbl_queue_anchor()),
        task_ptr,
        "the system-owned queue anchor should link to the application task"
    );
    assert_eq!(
        bus.read_long(super::VBL_QUEUE_HEADER + 6),
        task_ptr,
        "VInstall should publish the system queue tail in low memory"
    );
}

#[test]
fn vinstall_and_vremove_keep_low_memory_queue_header_and_links_coherent() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let first = bus.alloc(14);
    let second = bus.alloc(14);
    for task_ptr in [first, second] {
        bus.write_word(task_ptr + 4, 1);
        bus.write_long(task_ptr + 6, 0x1234_5678);
        bus.write_word(task_ptr + 10, 2);
        cpu.write_reg(Register::A0, task_ptr);
        dispatcher
            .dispatch_memory(false, 0x33, &mut cpu, &mut bus)
            .expect("VInstall should be handled")
            .expect("VInstall should return");
    }

    let anchor = dispatcher.callback_scheduling.system_vbl_queue_anchor();
    assert_ne!(anchor, 0);
    assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 2), anchor);
    assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 6), second);
    assert_eq!(bus.read_long(anchor), first);
    assert_eq!(bus.read_long(first), second);
    assert_eq!(bus.read_long(second), 0);

    cpu.write_reg(Register::A0, first);
    dispatcher
        .dispatch_memory(false, 0x34, &mut cpu, &mut bus)
        .expect("VRemove should be handled")
        .expect("VRemove should return");
    assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 2), anchor);
    assert_eq!(bus.read_long(super::VBL_QUEUE_HEADER + 6), second);
    assert_eq!(bus.read_long(anchor), second);
    assert_eq!(bus.read_long(second), 0);
}

#[test]
fn classic_callback_task_traps_mutate_the_attached_process_registry() {
    let (mut classic, mut cpu, mut bus) = setup();
    let mut attached_view = super::super::TrapDispatcher::new();
    let mut context = ProcessContext::default();
    classic.attach_unconverted_process_services(&mut context);
    attached_view.attach_unconverted_process_services(&mut context);

    let timer = bus.alloc(22);
    bus.write_long(timer + 6, 0x1234_5678);
    cpu.write_reg(Register::A0, timer);
    classic.current_trap_word = 0xA058;
    classic
        .dispatch_memory(false, 0x58, &mut cpu, &mut bus)
        .expect("InsTime should be handled")
        .expect("InsTime should return");

    let vbl = bus.alloc(14);
    bus.write_word(vbl + 4, 1);
    bus.write_long(vbl + 6, 0x1234_5678);
    bus.write_word(vbl + 10, 2);
    cpu.write_reg(Register::A0, vbl);
    classic
        .dispatch_memory(false, 0x33, &mut cpu, &mut bus)
        .expect("VInstall should be handled")
        .expect("VInstall should return");

    assert!(classic.timer_tasks.ptr_eq(&attached_view.timer_tasks));
    assert!(classic.vbl_tasks.ptr_eq(&attached_view.vbl_tasks));
    assert_eq!(
        attached_view.timer_tasks[0].architecture,
        crate::callback_manager::CallbackTaskArchitecture::M68k
    );
    assert_eq!(
        attached_view.vbl_tasks[0].architecture,
        crate::callback_manager::CallbackTaskArchitecture::M68k
    );
    let detached_timers = attached_view.timer_tasks.clone();
    let detached_vbls = attached_view.vbl_tasks.clone();

    cpu.write_reg(Register::A0, timer);
    classic
        .dispatch_memory(false, 0x59, &mut cpu, &mut bus)
        .expect("RmvTime should be handled")
        .expect("RmvTime should return");
    cpu.write_reg(Register::A0, vbl);
    classic
        .dispatch_memory(false, 0x34, &mut cpu, &mut bus)
        .expect("VRemove should be handled")
        .expect("VRemove should return");

    assert!(attached_view.timer_tasks.is_empty());
    assert!(attached_view.vbl_tasks.is_empty());
    assert_eq!(detached_timers.len(), 1);
    assert_eq!(detached_vbls.len(), 1);
}

#[test]
fn vinstall_invalid_qtype_returns_vtyperr() {
    // Inside Macintosh: Processes (1994), p. 4-25:
    // VInstall returns vTypErr (-2) for invalid qType.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 0);
    cpu.write_reg(Register::A0, task_ptr);

    let result = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
    assert!(result.is_some(), "VInstall should be handled");
    assert!(result.unwrap().is_ok(), "VInstall should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -2,
        "VInstall should return vTypErr for invalid qType"
    );
}

#[test]
fn vremove_consumes_a0_taskptr_and_returns_oserr_in_d0() {
    // Inside Macintosh: Processes (1994), pp. 4-25 to 4-26:
    // VRemove takes task pointer in A0 and returns OSErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 1);
    bus.write_long(task_ptr + 6, 0x1234_5678);
    bus.write_word(task_ptr + 10, 2);
    bus.write_word(task_ptr + 12, 1);

    cpu.write_reg(Register::A0, task_ptr);
    let install = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
    assert!(install.is_some());
    assert!(install.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::A0, task_ptr);
    let remove = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
    assert!(remove.is_some(), "VRemove should be handled");
    assert!(remove.unwrap().is_ok(), "VRemove should return cleanly");
    assert_eq!(cpu.read_reg(Register::D0), 0, "VRemove should return noErr");
    assert_eq!(
        bus.read_long(task_ptr),
        0,
        "removed task should have qLink reset to NIL"
    );
}

#[test]
fn vremove_task_not_in_queue_returns_qerr() {
    // Inside Macintosh: Processes (1994), p. 4-26:
    // VRemove returns qErr (-1) when the task isn't in the queue.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 1);
    cpu.write_reg(Register::A0, task_ptr);

    let result = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
    assert!(result.is_some(), "VRemove should be handled");
    assert!(result.unwrap().is_ok(), "VRemove should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -1,
        "VRemove should return qErr for non-queued task"
    );
}

#[test]
fn vinstall_then_vremove_roundtrip_returns_noerr_on_both_and_qerr_on_second_remove() {
    // VInstall(valid) → noErr; immediate VRemove → noErr; second VRemove
    // of the same now-empty slot → qErr. Per IM:Processes 1994 p. 4-26
    // a second remove of an already-removed task hits the queue-not-found
    // path and returns qErr=-1.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 1);
    bus.write_long(task_ptr + 6, 0x0040_1000);
    bus.write_word(task_ptr + 10, 0x7FFF);
    bus.write_word(task_ptr + 12, 0);

    cpu.write_reg(Register::A0, task_ptr);
    let install = dispatcher.dispatch_memory(false, 0x33, &mut cpu, &mut bus);
    assert!(install.is_some());
    assert!(install.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "VInstall should return noErr"
    );

    cpu.write_reg(Register::A0, task_ptr);
    let remove1 = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
    assert!(remove1.is_some());
    assert!(remove1.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "first VRemove should return noErr"
    );

    cpu.write_reg(Register::A0, task_ptr);
    let remove2 = dispatcher.dispatch_memory(false, 0x34, &mut cpu, &mut bus);
    assert!(remove2.is_some());
    assert!(remove2.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -1,
        "second VRemove should return qErr (task no longer in queue)"
    );
}

#[test]
fn slotvinstall_consumes_a0_taskptr_d0_slot_and_returns_oserr_in_d0() {
    // Inside Macintosh: Processes (1994), pp. 4-22 to 4-23:
    // SlotVInstall takes task pointer in A0, slot in D0, and returns OSErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 1);
    bus.write_long(task_ptr + 6, 0x1234_5678);
    bus.write_word(task_ptr + 10, 2);
    bus.write_word(task_ptr + 12, 1);
    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 3);

    let result = dispatcher.dispatch_memory(false, 0x6F, &mut cpu, &mut bus);
    assert!(result.is_some(), "SlotVInstall should be handled");
    assert!(
        result.unwrap().is_ok(),
        "SlotVInstall should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SlotVInstall should return noErr"
    );
    assert_eq!(
        bus.read_long(task_ptr),
        0,
        "single-task queue link should terminate at NIL"
    );
}

#[test]
fn slotvinstall_negative_one_slot_returns_noerr() {
    // Inside Macintosh: Processes (1994), p. 4-23:
    // BasiliskII accepts slot = -1 for SlotVInstall and returns noErr.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 1);
    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, (-1i16) as u16 as u32);

    let result = dispatcher.dispatch_memory(false, 0x6F, &mut cpu, &mut bus);
    assert!(result.is_some(), "SlotVInstall should be handled");
    assert!(
        result.unwrap().is_ok(),
        "SlotVInstall should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SlotVInstall should return noErr for slot = -1"
    );
    assert_eq!(
        bus.read_long(task_ptr),
        0,
        "SlotVInstall should still install the task record"
    );
}

#[test]
fn slotvremove_consumes_a0_taskptr_d0_slot_and_returns_oserr_in_d0() {
    // Inside Macintosh: Processes (1994), pp. 4-23 to 4-24:
    // SlotVRemove takes task pointer in A0, slot in D0, and returns OSErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 1);
    bus.write_long(task_ptr + 6, 0x1234_5678);
    bus.write_word(task_ptr + 10, 2);
    bus.write_word(task_ptr + 12, 1);

    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 2);
    let install = dispatcher.dispatch_memory(false, 0x6F, &mut cpu, &mut bus);
    assert!(install.is_some());
    assert!(install.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::D0), 0);

    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 2);
    let remove = dispatcher.dispatch_memory(false, 0x70, &mut cpu, &mut bus);
    assert!(remove.is_some(), "SlotVRemove should be handled");
    assert!(remove.unwrap().is_ok(), "SlotVRemove should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SlotVRemove should return noErr"
    );
}

#[test]
fn slotvremove_task_not_in_queue_returns_qerr() {
    // Inside Macintosh: Processes (1994), p. 4-24:
    // SlotVRemove returns qErr (-1) when the task isn't in the queue.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 1);
    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 4);

    let result = dispatcher.dispatch_memory(false, 0x70, &mut cpu, &mut bus);
    assert!(result.is_some(), "SlotVRemove should be handled");
    assert!(result.unwrap().is_ok(), "SlotVRemove should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -1,
        "SlotVRemove should return qErr for non-queued task"
    );
}

#[test]
fn slotvremove_invalid_task_returns_vtyperr_without_event_fallback() {
    // Inside Macintosh: Processes (1994), p. 4-24: SlotVRemove returns
    // vTypErr (-2) when qType is not ORD(vType). `$A070` is not an Event
    // Manager alias; GetNextEvent is the Toolbox trap `$A970`.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = bus.alloc(14);
    bus.write_word(task_ptr + 4, 0);
    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 4);

    let result = dispatcher.dispatch(0xA070, &mut cpu, &mut bus);

    assert!(result.is_ok());
    assert_eq!(cpu.read_reg(Register::D0) as i16, -2);
    assert_eq!(
        dispatcher.current_trap_adapter,
        super::super::dispatch::TrapAdapterId::Memory
    );
}

#[test]
fn test_hpurge() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x49, &mut cpu, &mut bus);
    assert!(result.is_some(), "HPurge ($A049) should be handled");
    assert!(result.unwrap().is_ok(), "HPurge should succeed");
}

#[test]
fn movehhi_unlocked_handle_returns_noerr_and_preserves_master_pointer() {
    // Inside Macintosh Volume II (1985), p. II-44: MoveHHi moves the
    // relocatable block toward the top of the current heap zone and
    // reports success as noErr.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let data_ptr = bus.alloc(32);
    let handle = bus.alloc(4);
    bus.write_long(handle, data_ptr);
    cpu.write_reg(Register::A0, handle);

    let result = dispatcher.dispatch_memory(false, 0x64, &mut cpu, &mut bus);
    assert!(result.is_some(), "MoveHHi should be handled");
    assert!(result.unwrap().is_ok(), "MoveHHi should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "MoveHHi should return noErr");
    assert_eq!(
        bus.read_long(handle),
        data_ptr,
        "MoveHHi should keep the master pointer valid for the same handle"
    );
}

#[test]
fn test_get_handle_size() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    // First allocate a handle of size 256
    cpu.write_reg(Register::D0, 256);
    let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let handle = cpu.read_reg(Register::A0);
    // Now get its size
    cpu.write_reg(Register::A0, handle);
    let result = dispatcher.dispatch_memory(false, 0x25, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetHandleSize should be handled");
    assert!(result.unwrap().is_ok(), "GetHandleSize should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        256,
        "GetHandleSize should return 256 in D0"
    );
}

#[test]
fn get_handle_size_reports_loaded_resource_handle_logical_size() {
    // GetHandleSize reports a block's LOGICAL size, not the padded
    // physical one, and the Resource Manager allocates a loaded
    // resource's handle at the resource's exact byte count.
    // GetHandleSize ($A025)
    // FUNCTION GetHandleSize (h: Handle): Size;
    // Inside Macintosh Volume II, II-31; Memory 1992, 2-32.
    //
    // Rounding up to a longword hands callers that treat a resource as
    // an array one element too many. SimCity 2000 sizes its CREL
    // relocation table as GetHandleSize/2, and the phantom entry
    // relocated the CODE segment header, wedging its segment loader.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let bytes = vec![0x80; 50];
    let data_ptr = bus.alloc(bytes.len() as u32);
    bus.write_bytes(data_ptr, &bytes);
    dispatcher.remember_resource_backing_data(0, *b"snd ", 417, bytes);
    let handle =
        dispatcher.get_or_create_resource_handle_in_file(&mut bus, *b"snd ", 417, data_ptr, 0);

    cpu.write_reg(Register::A0, handle);
    let result = dispatcher.dispatch_memory(false, 0x25, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetHandleSize should be handled");
    assert!(result.unwrap().is_ok(), "GetHandleSize should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        50,
        "loaded resource handles should expose the resource's exact byte count"
    );
}

#[test]
fn get_handle_size_tracks_resized_resource_handle_logical_size() {
    // GetHandleSize reports the current logical size of the in-memory
    // relocatable block. GetResourceSizeOnDisk/SizeResource is the API
    // that reports the resource's unchanged on-disk size.
    // GetHandleSize ($A025)
    // FUNCTION GetHandleSize (h: Handle): Size;
    // Inside Macintosh: Memory (1992), pp. 2-39--2-40; More Macintosh
    // Toolbox (1993), p. 1-105.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let mut resource_bytes = vec![0x80; 199];
    resource_bytes[165..169].copy_from_slice(b"Loca");
    let data_ptr = bus.alloc(resource_bytes.len() as u32);
    bus.write_bytes(data_ptr, &resource_bytes);
    dispatcher.remember_resource_backing_data(0, *b"Xmnu", 131, resource_bytes);
    let handle =
        dispatcher.get_or_create_resource_handle_in_file(&mut bus, *b"Xmnu", 131, data_ptr, 0);
    assert_eq!(dispatcher.handle_state_bits(handle), Some(0x60));

    let resized_ptr = dispatcher.resize_resource_allocation(&mut bus, handle, data_ptr, 256);
    assert_ne!(resized_ptr, 0);
    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x25, &mut cpu, &mut bus)
        .expect("GetHandleSize should be handled")
        .expect("GetHandleSize should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 256);

    let first_source = bus.alloc(1);
    bus.write_byte(first_source, 0xFF);
    cpu.write_reg(Register::A0, first_source);
    cpu.write_reg(Register::A1, handle);
    cpu.write_reg(Register::D0, 1);
    dispatcher
        .dispatch_memory(true, 0x1EF, &mut cpu, &mut bus)
        .expect("PtrAndHand should be handled")
        .expect("PtrAndHand should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0);
    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x25, &mut cpu, &mut bus)
        .expect("GetHandleSize should be handled")
        .expect("GetHandleSize should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 257);

    let mut appended_record = [0u8; 34];
    for (index, byte) in appended_record.iter_mut().enumerate().skip(4) {
        *byte = index as u8;
    }
    let record_source = bus.alloc(appended_record.len() as u32);
    bus.write_bytes(record_source, &appended_record);
    cpu.write_reg(Register::A0, record_source);
    cpu.write_reg(Register::A1, handle);
    cpu.write_reg(Register::D0, appended_record.len() as u32);
    dispatcher
        .dispatch_memory(true, 0x1EF, &mut cpu, &mut bus)
        .expect("PtrAndHand should be handled")
        .expect("PtrAndHand should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(dispatcher.handle_state_bits(handle), Some(0x60));

    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x25, &mut cpu, &mut bus)
        .expect("GetHandleSize should be handled")
        .expect("GetHandleSize should succeed");
    let final_size = cpu.read_reg(Register::D0);
    assert_eq!(final_size, 291);
    let final_ptr = bus.read_long(handle);
    assert_eq!(
        bus.read_bytes(
            final_ptr + final_size - appended_record.len() as u32,
            appended_record.len()
        ),
        appended_record
    );
    assert_eq!(bus.read_bytes(final_ptr + 165, 4), b"Loca");
    assert_eq!(
        dispatcher
            .resource_backing_data
            .get(&(0, *b"Xmnu", 131))
            .map(Vec::len),
        Some(199)
    );
}

#[test]
fn test_get_handle_size_updates_ccr_from_low_word_of_d0() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let ptr = bus.alloc(0x1_0000);
    let handle = bus.alloc(4);
    bus.write_long(handle, ptr);
    cpu.write_reg(Register::A0, handle);
    cpu.ccr = 0x10;

    let result = dispatcher.dispatch_memory(false, 0x25, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetHandleSize should be handled");
    assert!(result.unwrap().is_ok(), "GetHandleSize should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0x1_0000);
    assert_eq!(
        cpu.ccr, 0x14,
        "GetHandleSize should emulate trap-dispatcher TST.W D0 and preserve X"
    );
}

#[test]
fn test_dispose_handle_sets_zero_flag_via_dispatcher_ccr() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let ptr = bus.alloc(8);
    let handle = bus.alloc(4);
    bus.write_long(handle, ptr);
    cpu.write_reg(Register::A0, handle);
    cpu.ccr = 0x18;

    let result = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
    assert!(result.is_some(), "DisposeHandle should be handled");
    assert!(result.unwrap().is_ok(), "DisposeHandle should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(
        cpu.ccr, 0x14,
        "DisposeHandle should emulate trap-dispatcher TST.W D0 and preserve X"
    );
}

#[test]
fn test_flush_code_cache() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let result = dispatcher.dispatch_memory(false, 0xBD, &mut cpu, &mut bus);
    assert!(result.is_some(), "FlushCodeCache should be handled");
    assert!(
        result.unwrap().is_ok(),
        "FlushCodeCache should succeed (no-op)"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "FlushCodeCache should return noErr in D0"
    );
}

#[test]
fn hwpriv_swapinstructioncache_selector_0000_returns_previous_state_boolean() {
    // Inside Macintosh: Memory (1992), p. 4-29:
    // SwapInstructionCache uses _HWPriv selector $0000 and returns
    // the previous instruction-cache state as a Boolean, while
    // installing the requested new state.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xDEAD_BEEF);
    cpu.write_reg(Register::A0, 0); // request FALSE
    cpu.write_reg(Register::D0, 0); // selector $0000

    let result = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
    assert!(result.is_some(), "HWPriv should be handled");
    assert!(
        result.unwrap().is_ok(),
        "HWPriv selector $0000 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        1,
        "SwapInstructionCache should return previous state Boolean (TRUE) in A0"
    );
    assert!(
        !dispatcher.instruction_cache_enabled,
        "SwapInstructionCache(FALSE) should install the requested disabled state"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "_HWPriv selector calls use register ABI and should not pop stack arguments"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xDEAD_BEEF,
        "stack top should remain untouched for register-dispatched _HWPriv calls"
    );
}

#[test]
fn hwpriv_swapinstructioncache_second_call_returns_false_after_prior_disable() {
    // The second SwapInstructionCache call should observe the state
    // installed by the first one.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0); // disable
    cpu.write_reg(Register::D0, 0);
    let first = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
    assert!(
        first.is_some(),
        "first SwapInstructionCache should be handled"
    );
    assert!(
        first.unwrap().is_ok(),
        "first SwapInstructionCache should return"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        1,
        "first SwapInstructionCache(FALSE) should report previous TRUE state in A0"
    );

    cpu.write_reg(Register::A0, 1); // re-enable
    cpu.write_reg(Register::D0, 0);
    let second = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
    assert!(
        second.is_some(),
        "second SwapInstructionCache should be handled"
    );
    assert!(
        second.unwrap().is_ok(),
        "second SwapInstructionCache should return"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "second SwapInstructionCache(TRUE) should report prior FALSE state in A0"
    );
    assert!(
        dispatcher.instruction_cache_enabled,
        "second SwapInstructionCache(TRUE) should restore the enabled state"
    );
}

#[test]
fn hwpriv_swapdatacache_selector_0002_returns_previous_state_boolean() {
    // Inside Macintosh: Memory (1992), p. 4-30:
    // SwapDataCache uses _HWPriv selector $0002 and returns
    // the previous data-cache state as a Boolean, while installing
    // the requested new state.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xCAFE_F00D);
    cpu.write_reg(Register::A0, 0); // request FALSE
    cpu.write_reg(Register::D0, 2); // selector $0002

    let result = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
    assert!(result.is_some(), "HWPriv should be handled");
    assert!(
        result.unwrap().is_ok(),
        "HWPriv selector $0002 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        1,
        "SwapDataCache should return previous state Boolean (TRUE) in A0"
    );
    assert!(
        !dispatcher.data_cache_enabled,
        "SwapDataCache(FALSE) should install the requested disabled state"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "_HWPriv selector calls use register ABI and should not pop stack arguments"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xCAFE_F00D,
        "stack top should remain untouched for register-dispatched _HWPriv calls"
    );
}

#[test]
fn hwpriv_swapdatacache_second_call_returns_false_after_prior_disable() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0); // disable
    cpu.write_reg(Register::D0, 2);
    let first = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
    assert!(first.is_some(), "first SwapDataCache should be handled");
    assert!(first.unwrap().is_ok(), "first SwapDataCache should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        1,
        "first SwapDataCache(FALSE) should report previous TRUE state in A0"
    );

    cpu.write_reg(Register::A0, 1); // re-enable
    cpu.write_reg(Register::D0, 2);
    let second = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
    assert!(second.is_some(), "second SwapDataCache should be handled");
    assert!(
        second.unwrap().is_ok(),
        "second SwapDataCache should return"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "second SwapDataCache(TRUE) should report prior FALSE state in A0"
    );
    assert!(
        dispatcher.data_cache_enabled,
        "second SwapDataCache(TRUE) should restore the enabled state"
    );
}

#[test]
fn hwpriv_flushcodecacherange_selector_0009_uses_a0_a1_and_returns_result_code() {
    // Inside Macintosh: Memory (1992), pp. 4-32 to 4-33:
    // FlushCodeCacheRange uses _HWPriv selector $0009 with
    // A0=address, A1=count, and returns result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xFACE_C0DE);
    cpu.write_reg(Register::A0, 0x0012_3400);
    cpu.write_reg(Register::A1, 0x0000_0400);
    cpu.write_reg(Register::D0, 9); // selector $0009

    let result = dispatcher.dispatch_memory(false, 0x98, &mut cpu, &mut bus);
    assert!(result.is_some(), "HWPriv should be handled");
    assert!(
        result.unwrap().is_ok(),
        "HWPriv selector $0009 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "FlushCodeCacheRange should return noErr in D0 on nominal HLE path"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0x0012_3400,
        "FlushCodeCacheRange should read address from A0 without clobbering it"
    );
    assert_eq!(
        cpu.read_reg(Register::A1),
        0x0000_0400,
        "FlushCodeCacheRange should read byte count from A1 without clobbering it"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "FlushCodeCacheRange should not consume a Pascal stack argument frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xFACE_C0DE,
        "stack top should remain untouched by register ABI call"
    );
}

#[test]
fn flushcodecache_is_parameterless_and_preserves_stack_pointer() {
    // Inside Macintosh: Memory (1992), p. 4-31:
    // FlushCodeCache is PROCEDURE FlushCodeCache; with no arguments.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xBEEF_1357);

    let result = dispatcher.dispatch_memory(false, 0xBD, &mut cpu, &mut bus);
    assert!(result.is_some(), "FlushCodeCache should be handled");
    assert!(
        result.unwrap().is_ok(),
        "FlushCodeCache should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "FlushCodeCache should not pop a stack argument frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xBEEF_1357,
        "FlushCodeCache should leave the caller stack untouched"
    );
}

#[test]
fn test_get_trap_address() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    cpu.write_reg(Register::D0, 0x0044);
    let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetTrapAddress should be handled");
    assert!(result.unwrap().is_ok(), "GetTrapAddress should succeed");
    let addr = cpu.read_reg(Register::A0);
    assert_ne!(addr, 0, "GetTrapAddress should return a routine address");
    assert_eq!(bus.read_word(addr), 0xA044);
    assert_eq!(bus.read_word(addr + 2), 0x4E75);

    bus.write_word(addr, 0);
    cpu.write_reg(Register::D0, 0x0044);
    dispatcher
        .dispatch_memory(false, 0x46, &mut cpu, &mut bus)
        .expect("GetTrapAddress should be handled")
        .expect("GetTrapAddress should succeed repeatedly");
    assert_eq!(cpu.read_reg(Register::A0), addr);
    assert_eq!(bus.read_word(addr), 0xA044);
    assert_eq!(bus.read_word(addr + 2), 0x4E75);
}

#[test]
fn restoring_an_os_trap_gateway_removes_the_installed_patch() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    dispatcher.current_trap_word = 0xA346;
    cpu.write_reg(Register::D0, 0x39);
    dispatcher
        .dispatch_memory(false, 0x46, &mut cpu, &mut bus)
        .expect("GetOSTrapAddress should be handled")
        .expect("GetOSTrapAddress should succeed");
    let original = cpu.read_reg(Register::A0);

    dispatcher.current_trap_word = 0xA247;
    cpu.write_reg(Register::D0, 0x39);
    cpu.write_reg(Register::A0, 0x0030_0000);
    dispatcher
        .dispatch_memory(false, 0x47, &mut cpu, &mut bus)
        .expect("SetOSTrapAddress should be handled")
        .expect("SetOSTrapAddress should install a patch");
    assert_eq!(
        dispatcher.native_trap_handler(&bus, 0xA039),
        Some(0x0030_0000)
    );

    cpu.write_reg(Register::A0, original);
    dispatcher
        .dispatch_memory(false, 0x47, &mut cpu, &mut bus)
        .expect("SetOSTrapAddress should be handled")
        .expect("SetOSTrapAddress should restore the original");
    assert!(dispatcher.native_trap_handler(&bus, 0xA039).is_none());
}

#[test]
fn changing_a_trap_head_does_not_cancel_an_active_native_invocation() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    let sp = 0x003F_FF00u32;
    dispatcher
        .install_trap_address(&mut bus, 0xA039, 0x0030_0000)
        .unwrap();
    cpu.write_reg(Register::PC, 0x0020_0002);
    cpu.write_reg(Register::A7, sp);
    dispatcher.dispatch(0xA039, &mut cpu, &mut bus).unwrap();
    assert_eq!(
        dispatcher
            .pending_native_trap_calls
            .get(&0xA039)
            .unwrap()
            .len(),
        1
    );

    dispatcher.current_trap_word = 0xA247;
    cpu.write_reg(Register::D0, 0x39);
    cpu.write_reg(Register::A0, 0x0031_0000);
    dispatcher
        .dispatch_memory(false, 0x47, &mut cpu, &mut bus)
        .expect("SetOSTrapAddress should be handled")
        .expect("SetOSTrapAddress should replace the table head");

    assert_eq!(
        dispatcher.native_trap_handler(&bus, 0xA039),
        Some(0x0031_0000)
    );
    assert_eq!(
        dispatcher
            .pending_native_trap_calls
            .get(&0xA039)
            .unwrap()
            .len(),
        1,
        "table mutation must not cancel an executing patch"
    );
}

#[test]
fn legacy_gettrapaddress_classifies_toolbox_trap_words_by_number() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    dispatcher.current_trap_word = 0xA146;
    dispatcher
        .install_trap_address(&mut bus, 0xA0A0, 0x0000_ADF2)
        .unwrap();
    cpu.write_reg(Register::D0, 0xA9A0);

    let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetTrapAddress should be handled");
    assert!(result.unwrap().is_ok(), "GetTrapAddress should succeed");

    let addr = cpu.read_reg(Register::A0);
    assert_ne!(
        addr, 0x0000_ADF2,
        "legacy GetTrapAddress must not resolve Toolbox trap $A9A0 through the OS table"
    );
    assert_eq!(
        bus.read_word(addr),
        0xADA0,
        "legacy GetTrapAddress must return a callable auto-pop Toolbox trampoline"
    );
}

#[test]
fn getostrapaddress_explicitly_uses_the_os_table() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    dispatcher.current_trap_word = 0xA346;
    dispatcher
        .install_trap_address(&mut bus, 0xA0A0, 0x0000_ADF2)
        .unwrap();
    cpu.write_reg(Register::D0, 0xA9A0);

    let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetOSTrapAddress should be handled");
    assert!(result.unwrap().is_ok(), "GetOSTrapAddress should succeed");
    assert_eq!(
        cpu.read_reg(Register::A0),
        0x0000_ADF2,
        "GetOSTrapAddress must mask the supplied trap word to the OS table"
    );
}

#[test]
fn trap_address_a0_variants_keep_new_os_and_new_tool_table_selection() {
    // Inside Macintosh: Operating System Utilities (1994), pp. 8-27--8-31:
    // bit 9 selects the new typed form, bit 10 then selects Toolbox, and
    // bit 8 independently controls A0 return handling. UI 3.4 Patches.h
    // lines 126--231 declares the new-OS and new-Tool entry points.
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    let supplied_trap = 0xA9A0;
    let os_key = 0xA0A0;
    let tool_key = 0xA9A0;
    dispatcher
        .install_trap_address(&mut bus, os_key, 0x0030_0000)
        .unwrap();
    dispatcher
        .install_trap_address(&mut bus, tool_key, 0x0031_0000)
        .unwrap();

    // Clear bit 8 from the declared getter words. The central route must
    // retain their table type even though the dispatcher later restores
    // A0 for a real no-A0-return invocation.
    for (trap_word, expected) in [(0xA246, 0x0030_0000), (0xA646, 0x0031_0000)] {
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::D0, supplied_trap);
        dispatcher
            .dispatch_memory(false, 0x46, &mut cpu, &mut bus)
            .expect("typed trap getter should be handled")
            .expect("typed trap getter should succeed");
        assert_eq!(
            cpu.read_reg(Register::A0),
            expected,
            "trap ${trap_word:04X}"
        );
    }

    // Add bit 8 to the declared setter words. It remains structural and
    // must not make either setter fall back to obsolete number inference.
    for (trap_word, key, handler) in [
        (0xA347, os_key, 0x0032_0000),
        (0xA747, tool_key, 0x0033_0000),
    ] {
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::D0, supplied_trap);
        cpu.write_reg(Register::A0, handler);
        dispatcher
            .dispatch_memory(false, 0x47, &mut cpu, &mut bus)
            .expect("typed trap setter should be handled")
            .expect("typed trap setter should succeed");
        assert_eq!(
            dispatcher.native_trap_handler(&bus, key),
            Some(handler),
            "trap ${trap_word:04X}"
        );
    }
}

#[test]
fn get_trap_address_does_not_misclassify_docking_dispatch_as_unimplemented() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher
        .materialize_trap_tables(&mut bus, crate::trap::dispatch::TrapTableProfile::M68k68040)
        .expect("trap table construction requires writable cells and system storage");

    cpu.write_reg(Register::D0, 0xAA57);
    let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
    assert!(
        result.is_some(),
        "GetTrapAddress should handle DockingDispatch"
    );
    assert!(result.unwrap().is_ok(), "GetTrapAddress should succeed");
    let docking_addr = cpu.read_reg(Register::A0);

    cpu.write_reg(Register::D0, 0xAA6E);
    let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
    assert!(
        result.is_some(),
        "GetTrapAddress should handle Unimplemented"
    );
    assert!(result.unwrap().is_ok(), "GetTrapAddress should succeed");
    let unimplemented_addr = cpu.read_reg(Register::A0);

    assert_ne!(
        docking_addr, unimplemented_addr,
        "the selected profile captures DockingDispatch as callable"
    );
}

#[test]
fn ab46_has_no_phantom_gettooltrapaddress_route() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0xA89F);
    cpu.write_reg(Register::A0, 0x1234_5678);

    let result = dispatcher.dispatch_memory(true, 0x346, &mut cpu, &mut bus);

    assert!(result.is_none());
    assert_eq!(cpu.read_reg(Register::A0), 0x1234_5678);
}

#[test]
fn test_block_move() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x300000u32;
    let dst = 0x310000u32;
    let data: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE];
    for (i, &b) in data.iter().enumerate() {
        bus.write_byte(src + i as u32, b);
    }
    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::A1, dst);
    cpu.write_reg(Register::D0, data.len() as u32);
    let result = dispatcher.dispatch_memory(false, 0x2E, &mut cpu, &mut bus);
    assert!(result.is_some(), "BlockMove should be handled");
    assert!(result.unwrap().is_ok(), "BlockMove should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "BlockMove should set D0 to 0"
    );
    for (i, &b) in data.iter().enumerate() {
        assert_eq!(
            bus.read_byte(dst + i as u32),
            b,
            "BlockMove should copy byte {} correctly (expected 0x{:02X})",
            i,
            b
        );
    }
}

#[test]
fn test_block_move_overlapping_ranges() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let base = 0x300000u32;
    let data: [u8; 8] = [0, 1, 2, 3, 4, 5, 6, 7];
    for (i, &b) in data.iter().enumerate() {
        bus.write_byte(base + i as u32, b);
    }

    cpu.write_reg(Register::A0, base);
    cpu.write_reg(Register::A1, base + 2);
    cpu.write_reg(Register::D0, 6);

    let result = dispatcher.dispatch_memory(false, 0x2E, &mut cpu, &mut bus);
    assert!(result.is_some(), "BlockMove should be handled");
    assert!(result.unwrap().is_ok(), "BlockMove should succeed");

    let expected: [u8; 8] = [0, 1, 0, 1, 2, 3, 4, 5];
    for (i, &b) in expected.iter().enumerate() {
        assert_eq!(
            bus.read_byte(base + i as u32),
            b,
            "BlockMove should behave like memmove for overlapping ranges"
        );
    }
}

#[test]
fn test_block_move_negative_count_is_noop() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x300000u32;
    let dst = 0x310000u32;
    bus.write_bytes(src, &[0xDE, 0xAD, 0xBE, 0xEF]);
    bus.write_bytes(dst, &[0x11, 0x22, 0x33, 0x44]);

    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::A1, dst);
    cpu.write_reg(Register::D0, 0xFFFF_FF81);

    let result = dispatcher.dispatch_memory(false, 0x2E, &mut cpu, &mut bus);
    assert!(result.is_some(), "BlockMove should be handled");
    assert!(result.unwrap().is_ok(), "BlockMove should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_bytes(dst, 4), vec![0x11, 0x22, 0x33, 0x44]);
}

#[test]
fn test_set_trap_address() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    // Set a native trap handler
    cpu.write_reg(Register::D0, 0x01FF);
    cpu.write_reg(Register::A0, 0x00400000); // handler address
    let result = dispatcher.dispatch_memory(false, 0x47, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetTrapAddress should be handled");
    assert!(result.unwrap().is_ok(), "SetTrapAddress should succeed");
    // Now GetTrapAddress should return the installed handler
    cpu.write_reg(Register::D0, 0x01FF);
    let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(
        cpu.read_reg(Register::A0),
        0x00400000,
        "GetTrapAddress should return the handler set by SetTrapAddress"
    );
}

#[test]
fn set_trap_address_does_not_promote_a_writable_signature_to_protected_code() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    let handler = bus.alloc(4);
    bus.write_long(handler, COME_FROM_PATCH_SIGNATURE);
    bus.write_word(addr::DS_ERR_CODE, 0xBEEF);
    dispatcher.current_trap_word = 0xA047;
    cpu.write_reg(Register::D0, 0x0175);
    cpu.write_reg(Register::A0, handler);

    let result = dispatcher
        .dispatch_memory(false, 0x47, &mut cpu, &mut bus)
        .expect("SetTrapAddress should be handled");

    assert!(result.is_ok());
    assert_eq!(bus.read_word(addr::DS_ERR_CODE), 0xBEEF);
    assert_eq!(dispatcher.native_trap_handler(&bus, 0xA975), Some(handler));
}

#[test]
fn initialized_68k_trap_manager_reads_and_writes_guest_table_bytes() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher
        .materialize_trap_tables(&mut bus, TrapTableProfile::M68k68040)
        .expect("trap table construction requires writable cells and system storage");
    let trap_word = 0xA975;
    let table_entry = TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4;
    let installed = 0x0021_0000;
    let direct_guest_write = 0x0021_1000;

    assert_eq!(dispatcher.native_trap_handler(&bus, trap_word), None);

    dispatcher.current_trap_word = 0xA047;
    cpu.write_reg(Register::D0, 0x0175);
    cpu.write_reg(Register::A0, installed);
    dispatcher
        .dispatch_memory(false, 0x47, &mut cpu, &mut bus)
        .expect("SetTrapAddress should be handled")
        .expect("SetTrapAddress should write the initialized table");
    assert_eq!(bus.read_long(table_entry), installed);
    assert_eq!(
        dispatcher.native_trap_handler(&bus, trap_word),
        Some(installed)
    );

    // A guest/native store bypasses the setter and is immediately visible
    // through the same 68K Trap Manager getter and dispatch lookup.
    bus.write_long(table_entry, direct_guest_write);
    dispatcher.current_trap_word = 0xA146;
    cpu.write_reg(Register::D0, 0x0175);
    dispatcher
        .dispatch_memory(false, 0x46, &mut cpu, &mut bus)
        .expect("GetTrapAddress should be handled")
        .expect("GetTrapAddress should read the guest table");
    assert_eq!(cpu.read_reg(Register::A0), direct_guest_write);
    assert_eq!(
        dispatcher.native_trap_handler(&bus, trap_word),
        Some(direct_guest_write)
    );
}

#[test]
fn nsettrapaddress_newtool_bare_trap_number_installs_canonical_tool_handler() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    let handler_addr = 0x0040_0000;
    let os_entry = super::super::dispatch::OS_TRAP_TABLE_BASE + 0xF4 * 4;
    let original_os_entry = bus.read_long(os_entry);

    // `_SetTrapAddress newTool` accepts either an A-line instruction
    // or a bare trap number in D0. In either case the Toolbox table
    // key is the canonical A-line word later dispatched by the CPU.
    dispatcher.current_trap_word = 0xA647;
    cpu.write_reg(Register::D0, 0x01F4);
    cpu.write_reg(Register::A0, handler_addr);

    let result = dispatcher.dispatch_memory(false, 0x47, &mut cpu, &mut bus);
    assert!(result.is_some(), "NSetTrapAddress should be handled");
    assert!(result.unwrap().is_ok(), "NSetTrapAddress should succeed");

    assert_eq!(
        dispatcher.native_trap_handler(&bus, 0xA9F4),
        Some(handler_addr),
        "bare Toolbox trap numbers must install under their canonical A-line word"
    );
    assert_eq!(
        bus.read_long(os_entry),
        original_os_entry,
        "a typed Toolbox setter must leave the corresponding OS slot untouched"
    );
}

#[test]
fn test_strip_address() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0xAB12_3456);
    let result = dispatcher.dispatch_memory(false, 0x55, &mut cpu, &mut bus);
    assert!(result.is_some(), "StripAddress should be handled");
    assert!(
        result.unwrap().is_ok(),
        "StripAddress should succeed (no-op)"
    );
    assert_eq!(cpu.read_reg(Register::A0), 0xAB12_3456);

    bus.set_addressing_32_bit(false);
    let result = dispatcher.dispatch_memory(false, 0x55, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A0), 0x0012_3456);
}

#[test]
fn test_purge_space() {
    // PurgeSpace returns the free_heap_estimate value (clamped
    // to [24MB, 64MB]) in BOTH A0 and D0. With dispatcher-
    // default HEAP_END/APPL_LIMIT (typically uninitialized
    // = 0), the saturating_sub yields 0 → clamps up to the
    // 24MB floor. Pin both registers receive the same value
    // so the "no fragmentation, total == contiguous" HLE
    // invariant holds.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let result = dispatcher.dispatch_memory(false, 0x62, &mut cpu, &mut bus);
    assert!(result.is_some(), "PurgeSpace should be handled");
    assert!(result.unwrap().is_ok(), "PurgeSpace should succeed");
    let a0 = cpu.read_reg(Register::A0);
    let d0 = cpu.read_reg(Register::D0);
    assert_eq!(
        a0,
        24 * 1024 * 1024,
        "PurgeSpace must return free_heap_estimate floor (24MB) in A0 with default low-mem state"
    );
    assert_eq!(
        d0,
        24 * 1024 * 1024,
        "PurgeSpace must return free_heap_estimate floor (24MB) in D0 with default low-mem state"
    );
    assert_eq!(
        a0, d0,
        "PurgeSpace must return same value in A0 (total) and D0 (contiguous) — \
             no fragmentation in Systemless's flat allocator"
    );
}

#[test]
fn test_sys_environs_0x90() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let buf = 0x300000u32;
    cpu.write_reg(Register::A0, buf);
    cpu.write_reg(Register::D0, 2); // version
    let result = dispatcher.dispatch_memory(false, 0x90, &mut cpu, &mut bus);
    assert!(result.is_some(), "SysEnvirons (0x90) should be handled");
    assert!(result.unwrap().is_ok(), "SysEnvirons (0x90) should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SysEnvirons should set D0 to 0 (noErr)"
    );
    assert_eq!(bus.read_word(buf), 2, "environsVersion should be 2");
    assert_eq!(
        bus.read_word(buf + 2),
        crate::machine_profile::REFERENCE_MACHINE_PROFILE.gestalt_machine_type,
        "machineType should match the reference machine profile"
    );
    assert_eq!(
        bus.read_word(buf + 4),
        crate::machine_profile::REFERENCE_MACHINE_PROFILE.system_version_bcd,
        "systemVersion should match the reference machine profile"
    );
    assert_eq!(bus.read_word(buf + 6), 5, "processor should be 5 (68040)");
    assert_eq!(
        bus.read_byte(buf + 8),
        1,
        "hasFPU should be 1 (68040 has integrated FPU)"
    );
    assert_eq!(bus.read_byte(buf + 9), 1, "hasColorQD should be 1");
}

#[test]
fn test_flush_code_cache_0xbd() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let result = dispatcher.dispatch_memory(false, 0xBD, &mut cpu, &mut bus);
    assert!(result.is_some(), "FlushCodeCache ($A0BD) should be handled");
    assert!(
        result.unwrap().is_ok(),
        "FlushCodeCache should succeed (no-op)"
    );
}

#[test]
fn internalwait_routes_gettimeout_and_settimeout_selector_paths() {
    // Inside Macintosh Volume V (1986), p. V-356 and
    // Inside Macintosh: Operating System Utilities (1994), p. 7-13:
    // GetTimeout/SetTimeout are selector-driven wrappers over _InternalWait.
    for selector in [0u16, 1u16] {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let sp_before = cpu.read_reg(Register::A7);
        bus.write_word(sp_before, selector);
        bus.write_long(sp_before + 4, 0xACED_C0DE);
        cpu.write_reg(Register::A0, selector as u32);

        let result = dispatcher.dispatch_memory(false, 0x7F, &mut cpu, &mut bus);
        assert!(result.is_some(), "InternalWait should be handled");
        assert!(
            result.unwrap().is_ok(),
            "InternalWait should return cleanly"
        );
        assert_eq!(
            cpu.read_reg(Register::A7),
            sp_before,
            "InternalWait no-op path should not pop selector bytes"
        );
        assert_eq!(
            bus.read_word(sp_before),
            selector,
            "InternalWait should leave caller-provided selector storage intact"
        );
    }
}

#[test]
fn internalwait_stub_preserves_stack_pointer_in_noop_path() {
    // Inside Macintosh Volume V (1986), p. V-356:
    // Start Manager timeout entry points are wrappers over _InternalWait.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xC0DE_CAFE);
    cpu.write_reg(Register::A1, 0x00AB_CDEF);
    cpu.write_reg(Register::D1, 0x1357_9BDF);

    let result = dispatcher.dispatch_memory(false, 0x7F, &mut cpu, &mut bus);
    assert!(result.is_some(), "InternalWait should be handled");
    assert!(
        result.unwrap().is_ok(),
        "InternalWait should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "InternalWait no-op path should preserve stack pointer"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xC0DE_CAFE,
        "InternalWait should not mutate caller stack contents"
    );
    assert_eq!(
        cpu.read_reg(Register::A1),
        0x00AB_CDEF,
        "InternalWait no-op path should preserve A1"
    );
    assert_eq!(
        cpu.read_reg(Register::D1),
        0x1357_9BDF,
        "InternalWait no-op path should preserve D1"
    );
}

#[test]
fn internalwait_five_call_composition_preserves_stack_across_alternating_selectors() {
    // Five successive _InternalWait dispatches with alternating A0
    // selectors ($0000/$0001) and varying D0 count inputs
    // (0/1/15/20/31) preserve A7 in aggregate, with no per-call drift.
    // Per IM:Operating_System_Utils 1994 pp. 9-27 to 9-28 the trap
    // consumes no Pascal stack arguments regardless of selector or D0
    // input.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xDEAD_BEEF);

    let selectors: [u32; 5] = [0x0000, 0x0001, 0x0000, 0x0001, 0x0001];
    let counts: [u32; 5] = [0, 1, 15, 20, 31];
    for (selector, count) in selectors.iter().zip(counts.iter()) {
        cpu.write_reg(Register::A0, *selector);
        cpu.write_reg(Register::D0, *count);
        let result = dispatcher.dispatch_memory(false, 0x7F, &mut cpu, &mut bus);
        assert!(result.is_some(), "InternalWait should be handled");
        assert!(
            result.unwrap().is_ok(),
            "InternalWait should return cleanly"
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "InternalWait five-call composition should preserve A7 in aggregate"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xDEAD_BEEF,
        "InternalWait should not mutate caller stack contents across composition"
    );
}

#[test]
fn dtinstall_uses_a0_dttaskptr_register_calling_convention() {
    // Inside Macintosh Volume V (1986), p. V-467 and
    // Inside Macintosh: Processes (1994), pp. 6-12 to 6-13:
    // _DTInstall uses A0 for dtTaskPtr and D0 for result.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let dt_task_ptr = bus.alloc(24);
    bus.write_long(dt_task_ptr, 0);
    bus.write_word(dt_task_ptr + 4, super::DT_QTYPE);
    bus.write_word(dt_task_ptr + 6, 0);
    bus.write_long(dt_task_ptr + 8, 0x1234_5678);
    bus.write_long(dt_task_ptr + 12, 0);
    bus.write_long(dt_task_ptr + 16, 0);
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xFACE_B00C);
    cpu.write_reg(Register::A0, dt_task_ptr);
    cpu.write_reg(Register::D0, 0xFFFF_FFFE);

    let result = dispatcher.dispatch_memory(false, 0x82, &mut cpu, &mut bus);
    assert!(result.is_some(), "DTInstall should be handled");
    assert!(result.unwrap().is_ok(), "DTInstall should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A0),
        dt_task_ptr,
        "DTInstall should preserve A0 task pointer"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "DTInstall should not consume a Pascal stack frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xFACE_B00C,
        "DTInstall should leave caller stack contents untouched"
    );
}

#[test]
fn dtinstall_valid_record_returns_noerr_in_d0() {
    // Inside Macintosh Volume V (1986), p. V-467:
    // DTInstall returns noErr for nominal installs.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let dt_task_ptr = bus.alloc(24);
    bus.write_long(dt_task_ptr, 0);
    bus.write_word(dt_task_ptr + 4, super::DT_QTYPE);
    bus.write_word(dt_task_ptr + 6, 0);
    bus.write_long(dt_task_ptr + 8, 0x1234_5678);
    bus.write_long(dt_task_ptr + 12, 0);
    bus.write_long(dt_task_ptr + 16, 0);
    cpu.write_reg(Register::A0, dt_task_ptr);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    let result = dispatcher.dispatch_memory(false, 0x82, &mut cpu, &mut bus);
    assert!(result.is_some(), "DTInstall should be handled");
    assert!(result.unwrap().is_ok(), "DTInstall should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DTInstall should return noErr in D0 for a valid record"
    );
    let queue = crate::memory::globals::addr::DT_QUEUE;
    assert_eq!(bus.read_long(queue + 2), dt_task_ptr);
    assert_eq!(bus.read_long(queue + 6), dt_task_ptr);
    assert_eq!(dispatcher.deferred_tasks.len(), 1);
    let now = dispatcher.current_tick();
    assert_eq!(dispatcher.pop_ready_deferred_task(&mut bus, now), None);
    assert_eq!(
        dispatcher.pop_ready_deferred_task(&mut bus, now + 1),
        Some(dt_task_ptr)
    );
    assert_eq!(bus.read_long(queue + 2), 0);
    assert_eq!(bus.read_long(queue + 6), 0);
}

#[test]
fn dtinstall_queues_multiple_records_in_fifo_order() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let first = bus.alloc(24);
    let second = bus.alloc(24);
    let queue = crate::memory::globals::addr::DT_QUEUE;
    for task in [first, second] {
        bus.write_word(task + 4, super::DT_QTYPE);
        cpu.write_reg(Register::A0, task);
        dispatcher
            .dispatch_memory(false, 0x82, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
    }
    assert_eq!(bus.read_long(first), second);
    assert_eq!(bus.read_long(queue + 2), first);
    assert_eq!(bus.read_long(queue + 6), second);
    let ready = dispatcher.current_tick() + 1;
    assert_eq!(
        dispatcher.pop_ready_deferred_task(&mut bus, ready),
        Some(first)
    );
    assert_eq!(bus.read_long(queue + 2), second);
    assert_eq!(
        dispatcher.pop_ready_deferred_task(&mut bus, ready),
        Some(second)
    );
    assert_eq!(bus.read_long(queue + 2), 0);
    assert_eq!(bus.read_long(queue + 6), 0);
}

#[test]
fn dtinstall_invalid_qtype_returns_vtyperr_and_preserves_stack_pointer() {
    // Inside Macintosh Volume V (1986), p. V-467; and
    // Inside Macintosh: Processes (1994), pp. 6-12 to 6-13:
    // DTInstall returns vTypErr (-2) when qType is not ORD(dtQType).
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let dt_task_ptr = bus.alloc(24);
    bus.write_long(dt_task_ptr, 0);
    bus.write_word(dt_task_ptr + 4, 0);
    bus.write_word(dt_task_ptr + 6, 0);
    bus.write_long(dt_task_ptr + 8, 0x1234_5678);
    bus.write_long(dt_task_ptr + 12, 0);
    bus.write_long(dt_task_ptr + 16, 0);
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xABCD_0123);
    cpu.write_reg(Register::A0, dt_task_ptr);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);

    let result = dispatcher.dispatch_memory(false, 0x82, &mut cpu, &mut bus);
    assert!(result.is_some(), "DTInstall should be handled");
    assert!(result.unwrap().is_ok(), "DTInstall should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0) as i16,
        -2,
        "DTInstall should return vTypErr for invalid qType"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "DTInstall should preserve A7 on invalid qType"
    );
}

#[test]
fn maxapplzone_returns_noerr_and_preserves_appllimit_in_hle() {
    // Inside Macintosh Volume II (1985), p. II-30: MaxApplZone expands
    // the app heap up to ApplLimit and reports noErr on success.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let appl_limit = 0x00A0_0000u32;
    bus.write_long(crate::memory::globals::addr::HEAP_END, appl_limit);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);
    cpu.write_reg(Register::D0, 0xFFFF_FFEE);

    let result = dispatcher.dispatch_memory(false, 0x63, &mut cpu, &mut bus);
    assert!(result.is_some(), "MaxApplZone should be handled");
    assert!(
        result.unwrap().is_ok(),
        "MaxApplZone should succeed on nominal call"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "MaxApplZone should return noErr in D0 for assembly callers"
    );
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
        appl_limit,
        "MaxApplZone should not rewrite ApplLimit itself"
    );
}

#[test]
fn maxapplzone_updates_heapend_to_applimit_when_room_is_available() {
    // Inside Macintosh Volume II, II-30 and Memory 1992, 2-27 / 2-74..2-75:
    // MaxApplZone expands the application heap up to ApplLimit.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let heap_end = 0x00A0_0000u32;
    let appl_limit = heap_end + 0x1000;
    bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);

    let result = dispatcher.dispatch_memory(false, 0x63, &mut cpu, &mut bus);
    assert!(result.is_some(), "MaxApplZone should be handled");
    assert!(
        result.unwrap().is_ok(),
        "MaxApplZone should succeed on nominal call"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "MaxApplZone should return noErr in D0 for assembly callers"
    );
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::HEAP_END),
        appl_limit,
        "MaxApplZone should advance HeapEnd to the current ApplLimit"
    );
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
        appl_limit,
        "MaxApplZone should not rewrite ApplLimit itself"
    );
}

#[test]
fn resrvmem_uses_d0_cbneeded_and_returns_noerr() {
    // Inside Macintosh Volume II (1985), p. II-39: ResrvMem takes
    // cbNeeded in D0 and returns an OSErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0x0001_0000);

    let result = dispatcher.dispatch_memory(false, 0x40, &mut cpu, &mut bus);
    assert!(result.is_some(), "ResrvMem should be handled");
    assert!(result.unwrap().is_ok(), "ResrvMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "ResrvMem should return noErr in D0 on nominal calls"
    );
}

#[test]
fn moremasters_returns_noerr_and_followup_newhandle_succeeds() {
    // Inside Macintosh Volume II (1985), p. II-31: MoreMasters allocates
    // another master-pointer block and reports noErr on success.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0xFACE_F00D);

    let more_masters = dispatcher.dispatch_memory(false, 0x36, &mut cpu, &mut bus);
    assert!(more_masters.is_some(), "MoreMasters should be handled");
    assert!(
        more_masters.unwrap().is_ok(),
        "MoreMasters should succeed on nominal call"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "MoreMasters should return noErr in D0 for assembly callers"
    );

    cpu.write_reg(Register::D0, 64);
    let new_handle = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(
        new_handle.unwrap().is_ok(),
        "NewHandle should still succeed"
    );
    assert_ne!(
        cpu.read_reg(Register::A0),
        0,
        "MoreMasters no-op path should not block subsequent handle allocation"
    );
}

#[test]
fn sleep_queue_variants_use_a0_and_keep_links_ordered_when_bit_8_changes() {
    // Inside Macintosh: Devices (1994), pp. 6-18, 6-26, and 6-33 makes
    // the SleepQRec link manager-owned and defines ordered install/remove.
    // UI 3.4 Power.h lines 447--461 and 705--731 puts qRecPtr in A0.
    // OS Utilities 1994, pp. 8-10--8-14 makes bit 8 independently control
    // A0 restoration, so $A38A/$A58A retain their operation.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let first = bus.alloc(12);
    let second = bus.alloc(12);
    let third = bus.alloc(12);
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0x1357_9BDF);

    for (trap_word, q_rec_ptr) in [(0xA28A, first), (0xA38A, second), (0xA28A, third)] {
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, q_rec_ptr);
        dispatcher
            .dispatch_memory(false, 0x8A, &mut cpu, &mut bus)
            .expect("SleepQInstall should be handled")
            .expect("SleepQInstall should return cleanly");
    }
    assert_eq!(dispatcher.sleep_queue, [first, second, third]);
    assert_eq!(bus.read_long(first), second);
    assert_eq!(bus.read_long(second), third);
    assert_eq!(bus.read_long(third), 0);

    for (trap_word, q_rec_ptr) in [(0xA58A, second), (0xA48A, first), (0xA48A, third)] {
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, q_rec_ptr);
        dispatcher
            .dispatch_memory(false, 0x8A, &mut cpu, &mut bus)
            .expect("SleepQRemove should be handled")
            .expect("SleepQRemove should return cleanly");
        if q_rec_ptr == second {
            assert_eq!(dispatcher.sleep_queue, [first, third]);
            assert_eq!(bus.read_long(first), third);
        }
    }
    assert!(dispatcher.sleep_queue.is_empty());
    assert_eq!(cpu.read_reg(Register::A7), sp_before);
    assert_eq!(bus.read_long(sp_before), 0x1357_9BDF);
}

#[test]
fn sleep_trap_leaves_stack_untouched() {
    // Universal Interfaces 3.4 Traps.h line 794 identifies $A08A as the
    // distinct _Sleep operation. It has no SleepQRec argument frame.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0x1357_9BDF);
    dispatcher.current_trap_word = 0xA08A;

    let result = dispatcher.dispatch_memory(false, 0x8A, &mut cpu, &mut bus);
    assert!(result.is_some(), "Sleep should be handled");
    assert!(result.unwrap().is_ok(), "Sleep should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "Sleep should not pop a Pascal frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0x1357_9BDF,
        "Sleep should leave the stack top untouched"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "Sleep should report noErr in D0"
    );
}

#[test]
fn commtoolboxdispatch_countditl_returns_dialog_item_count_and_preserves_stack_pointer() {
    // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
    // _CommToolboxDispatch ($A08B) dispatches CountDITL via selector
    // $0403. The MPW C frame places
    // the 4-byte result slot at SP+0..3, the selector word at SP+4..5,
    // and the DialogPtr argument at SP+6..9.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08B;
    let dialog_ptr = bus.alloc(170);
    let items = vec![
        crate::trap::dispatch::DialogItem {
            item_type: 4,
            rect: (10, 10, 20, 20),
            text: "One".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        },
        crate::trap::dispatch::DialogItem {
            item_type: 8,
            rect: (20, 10, 30, 20),
            text: "Two".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        },
        crate::trap::dispatch::DialogItem {
            item_type: 16,
            rect: (30, 10, 40, 20),
            text: "Three".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        },
    ];
    dispatcher.dialog_items.insert(dialog_ptr, items);
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xCAFE_BABE);
    bus.write_word(sp_before + 4, 0x0403);
    bus.write_long(sp_before + 6, dialog_ptr);
    bus.write_word(sp_before + 10, 0x9BDF);

    let result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
    assert!(result.is_some(), "CommToolboxDispatch should be handled");
    assert!(
        result.unwrap().is_ok(),
        "CommToolboxDispatch should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "CountDITL should leave the caller stack pointer unchanged"
    );
    assert_eq!(
        bus.read_word(sp_before + 4),
        0x0403,
        "CountDITL should not overwrite the selector word on the caller stack"
    );
    assert_eq!(
        bus.read_long(sp_before + 6),
        dialog_ptr,
        "CountDITL should not overwrite the dialog pointer slot on the caller stack"
    );
    assert_eq!(
        bus.read_word(sp_before + 10),
        0x9BDF,
        "CountDITL should not overwrite the following stack word"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        3,
        "CountDITL should report the three dialog items we installed"
    );
}

#[test]
fn commtoolboxdispatch_countditl_empty_dialog_ignores_front_window() {
    // Macintosh Toolbox Essentials (1992), pp. 6-128 to 6-129:
    // CountDITL returns the number of items in the dialog passed as
    // theDialog, irrespective of which other window is frontmost.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08B;

    let empty_dialog = bus.alloc(170);
    let empty_items_handle = bus.alloc(4);
    let empty_ditl = bus.alloc(2);
    bus.write_long(empty_dialog + 156, empty_items_handle);
    bus.write_long(empty_items_handle, empty_ditl);
    bus.write_word(empty_ditl, u16::MAX);

    let front_dialog = bus.alloc(170);
    let front_items_handle = bus.alloc(4);
    let front_ditl = bus.alloc(2);
    bus.write_long(front_dialog + 156, front_items_handle);
    bus.write_long(front_items_handle, front_ditl);
    bus.write_word(front_ditl, 1); // two items
    dispatcher.front_window = front_dialog;

    let sp = cpu.read_reg(Register::A7);
    bus.write_long(sp, 0xCAFE_BABE);
    bus.write_word(sp + 4, 0x0403);
    bus.write_long(sp + 6, empty_dialog);

    dispatcher
        .dispatch_memory(false, 0x8B, &mut cpu, &mut bus)
        .expect("CommToolboxDispatch should be handled")
        .expect("CountDITL should return cleanly");

    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "CountDITL must not substitute the front window for an empty target dialog"
    );
    assert_eq!(cpu.read_reg(Register::A7), sp);
    assert_eq!(bus.read_long(sp), 0xCAFE_BABE);
    assert_eq!(bus.read_long(sp + 6), empty_dialog);
}

#[test]
fn commtoolboxdispatch_appendditl_overlay_appends_items_and_preserves_stack_pointer() {
    // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
    // selector $0402 dispatches AppendDITL through _CommToolboxDispatch.
    // Marathon 2's preferences glue uses selector at SP+4, overlay method
    // at SP+6, DITL handle at SP+8, and DialogPtr at SP+12.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08B;

    let dialog_ptr = bus.alloc(170);
    let items_handle = bus.alloc(4);
    let old_ditl_ptr = bus.alloc(18);
    let preserved_handle = 0x00AB_CDEF;
    bus.write_long(items_handle, old_ditl_ptr);
    bus.write_long(dialog_ptr + 156, items_handle);
    bus.write_word(old_ditl_ptr, 0);
    bus.write_long(old_ditl_ptr + 2, preserved_handle);
    bus.write_word(old_ditl_ptr + 6, 10);
    bus.write_word(old_ditl_ptr + 8, 20);
    bus.write_word(old_ditl_ptr + 10, 30);
    bus.write_word(old_ditl_ptr + 12, 60);
    bus.write_byte(old_ditl_ptr + 14, 4);
    bus.write_byte(old_ditl_ptr + 15, 2);
    bus.write_bytes(old_ditl_ptr + 16, b"OK");
    dispatcher.dialog_items.insert(
        dialog_ptr,
        vec![crate::trap::dispatch::DialogItem {
            item_type: 4,
            rect: (10, 20, 30, 60),
            text: "OK".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }],
    );

    let append_handle = bus.alloc(4);
    let append_ditl_ptr = bus.alloc(20);
    bus.write_long(append_handle, append_ditl_ptr);
    bus.write_word(append_ditl_ptr, 0);
    bus.write_long(append_ditl_ptr + 2, 0);
    bus.write_word(append_ditl_ptr + 6, 40);
    bus.write_word(append_ditl_ptr + 8, 50);
    bus.write_word(append_ditl_ptr + 10, 55);
    bus.write_word(append_ditl_ptr + 12, 110);
    bus.write_byte(append_ditl_ptr + 14, 8);
    bus.write_byte(append_ditl_ptr + 15, 4);
    bus.write_bytes(append_ditl_ptr + 16, b"Pane");

    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0x1357_2468);
    bus.write_word(sp_before + 4, 0x0402);
    bus.write_word(sp_before + 6, 0); // overlayDITL
    bus.write_long(sp_before + 8, append_handle);
    bus.write_long(sp_before + 12, dialog_ptr);
    bus.write_word(sp_before + 16, 0xBEEF);

    let result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
    assert!(result.is_some(), "AppendDITL should be handled");
    assert!(result.unwrap().is_ok(), "AppendDITL should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "AppendDITL must preserve the caller stack pointer"
    );
    assert_eq!(bus.read_word(sp_before + 4), 0x0402);
    assert_eq!(bus.read_word(sp_before + 16), 0xBEEF);
    assert_eq!(
        cpu.read_reg(Register::D0),
        2,
        "AppendDITL should mirror the new dialog item count in D0"
    );

    let new_ditl_ptr = bus.read_long(items_handle);
    assert_ne!(new_ditl_ptr, old_ditl_ptr);
    assert_eq!(bus.read_word(new_ditl_ptr), 1);
    assert_eq!(
        bus.read_long(new_ditl_ptr + 2),
        preserved_handle,
        "AppendDITL must preserve existing item handles in the copied DITL"
    );
    let appended_handle = bus.read_long(new_ditl_ptr + 18);
    assert_ne!(
        appended_handle, 0,
        "AppendDITL should initialize text storage for appended statText items"
    );
    assert_eq!(
        dispatcher.dialog_items.get(&dialog_ptr).unwrap()[1].text,
        "Pane"
    );
}

#[test]
fn commtoolboxdispatch_countditl_preserves_non_d0_registers() {
    // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
    // CountDITL is one selector behind _CommToolboxDispatch. The MPW
    // C frame places the 4-byte result slot at SP+0..3, selector at
    // SP+4..5, and the DialogPtr argument at SP+6..9.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08B;
    let dialog_ptr = bus.alloc(170);
    dispatcher.dialog_items.insert(
        dialog_ptr,
        vec![crate::trap::dispatch::DialogItem {
            item_type: 4,
            rect: (10, 10, 20, 20),
            text: "One".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }],
    );
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xDEAD_BEEF);
    bus.write_word(sp_before + 4, 0x0403);
    bus.write_long(sp_before + 6, dialog_ptr);
    cpu.write_reg(Register::A0, 0x00AB_C000);
    cpu.write_reg(Register::A1, 0x00AB_C100);
    cpu.write_reg(Register::D1, 0x5AA5_0F0F);

    let result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
    assert!(result.is_some(), "CommToolboxDispatch should be handled");
    assert!(
        result.unwrap().is_ok(),
        "CommToolboxDispatch should return cleanly"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0x00AB_C000,
        "CommToolboxDispatch stub should preserve A0"
    );
    assert_eq!(
        cpu.read_reg(Register::A1),
        0x00AB_C100,
        "CountDITL should preserve A1"
    );
    assert_eq!(
        cpu.read_reg(Register::D1),
        0x5AA5_0F0F,
        "CountDITL should preserve D1"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "CountDITL should preserve the caller stack pointer"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        1,
        "CountDITL should mirror the result into D0 for CCR update"
    );
}

#[test]
fn commtoolboxdispatch_selector_0403_returns_expected_counts_for_one_and_three_item_dialogs() {
    // Inside Macintosh Volume VI (1991), Appendix C table C-3 (p. C-4):
    // _CommToolboxDispatch ($A08B) dispatches CountDITL via selector
    // $0403. The MPW frame places the
    // 4-byte result slot at SP+0..3, selector at SP+4..5, and the
    // DialogPtr argument at SP+6..9. This test checks both a one-item
    // dialog and the existing three-item dialog.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08B;

    let one_dialog_ptr = bus.alloc(170);
    dispatcher.dialog_items.insert(
        one_dialog_ptr,
        vec![crate::trap::dispatch::DialogItem {
            item_type: 4,
            rect: (10, 10, 20, 20),
            text: "OK".to_string(),
            resource_id: 0,
            proc_ptr: 0,
            sel_start: 0,
            sel_end: 0,
        }],
    );

    let three_dialog_ptr = bus.alloc(170);
    dispatcher.dialog_items.insert(
        three_dialog_ptr,
        vec![
            crate::trap::dispatch::DialogItem {
                item_type: 4,
                rect: (10, 10, 20, 20),
                text: "One".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            crate::trap::dispatch::DialogItem {
                item_type: 8,
                rect: (20, 10, 30, 20),
                text: "Two".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
            crate::trap::dispatch::DialogItem {
                item_type: 16,
                rect: (30, 10, 40, 20),
                text: "Three".to_string(),
                resource_id: 0,
                proc_ptr: 0,
                sel_start: 0,
                sel_end: 0,
            },
        ],
    );

    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xDEAD_BEEF);
    bus.write_word(sp_before + 4, 0x0403);
    bus.write_long(sp_before + 6, one_dialog_ptr);
    let one_result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
    assert!(
        one_result.is_some(),
        "CommToolboxDispatch should be handled for a one-item dialog"
    );
    assert!(
        one_result.unwrap().is_ok(),
        "CommToolboxDispatch should return cleanly for a one-item dialog"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        1,
        "CommToolboxDispatch should report one item for the one-item dialog"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "CommToolboxDispatch should preserve the caller stack pointer for the one-item dialog"
    );

    cpu.write_reg(Register::A7, sp_before);
    bus.write_long(sp_before, 0xDEAD_BEEF);
    bus.write_word(sp_before + 4, 0x0403);
    bus.write_long(sp_before + 6, three_dialog_ptr);
    let three_result = dispatcher.dispatch_memory(false, 0x8B, &mut cpu, &mut bus);
    assert!(
        three_result.is_some(),
        "CommToolboxDispatch should be handled for a three-item dialog"
    );
    assert!(
        three_result.unwrap().is_ok(),
        "CommToolboxDispatch should return cleanly for a three-item dialog"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        3,
        "CommToolboxDispatch should report three items for the three-item dialog"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "CommToolboxDispatch should preserve the caller stack pointer for the three-item dialog"
    );
}

#[test]
fn debug_util_generated_routes_preserve_exact_moveq_values() {
    assert_eq!(super::DEBUG_UTIL_OPERATION_ROUTES.len(), 9);
    assert!(super::DEBUG_UTIL_OPERATION_ROUTES
        .windows(2)
        .all(|pair| pair[0].selector < pair[1].selector));

    for (selector, routine_name) in [
        (0, "DebuggerGetMax"),
        (1, "DebuggerEnter"),
        (2, "DebuggerExit"),
        (3, "DebuggerPoll"),
        (4, "GetPageState"),
        (5, "PageFaultFatal"),
        (6, "DebuggerLockMemory"),
        (7, "DebuggerUnlockMemory"),
        (8, "EnterSupervisorMode"),
    ] {
        let route = super::debug_util_operation_route(0xA08D, selector).expect("DebugUtil route");
        assert_eq!(route.routine_name, routine_name);
    }

    for (trap_word, selector) in [
        (0xA18D, 0),
        (0xA08D, 9),
        (0xA08D, 0x0000_7000),
        (0xA08D, 0x0001_0000),
    ] {
        assert!(super::debug_util_operation_route(trap_word, selector).is_none());
    }
}

#[test]
fn debug_util_dispatch_records_fixed_routes_only() {
    let (mut dispatcher, mut cpu, mut bus) = setup();

    for route in super::DEBUG_UTIL_OPERATION_ROUTES {
        cpu.write_reg(Register::D0, route.selector);
        dispatcher.dispatch(0xA08D, &mut cpu, &mut bus).unwrap();
        assert_eq!(
            dispatcher.current_selector_operation,
            Some(route.operation_id)
        );
    }

    for selector in [9, 0x0000_7000, 0x0001_0000] {
        cpu.write_reg(Register::D0, selector);
        dispatcher.dispatch(0xA08D, &mut cpu, &mut bus).unwrap();
        assert_eq!(dispatcher.current_selector_operation, None);
    }
}

#[test]
fn debugutil_debuggergetmax_returns_max_selector_and_preserves_stack_pointer() {
    // Inside Macintosh Volume VI (1991), p. 28-30 and Appendix C table C-3
    // (p. C-4): _DebugUtil is selector-dispatched in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08D;
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0x2468_ACF0);
    cpu.write_reg(Register::D0, 0x0000); // DebuggerGetMax selector

    let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
    assert!(result.is_some(), "DebugUtil should be handled");
    assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "DebugUtil should not consume a stack argument frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0x2468_ACF0,
        "stack top should remain untouched for DebugUtil selector dispatch"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        8,
        "DebugUtil selector $0000 should return the highest documented selector"
    );
}

#[test]
fn debugutil_other_selectors_preserve_non_d0_registers() {
    // Inside Macintosh Volume VI (1991), pp. 28-30..28-31 and Appendix C
    // table C-3 (p. C-4): debugger helper routines share _DebugUtil.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08D;
    cpu.write_reg(Register::A0, 0x0012_3000);
    cpu.write_reg(Register::A1, 0x0012_3F00);
    cpu.write_reg(Register::D1, 0x1234_5678);
    cpu.write_reg(Register::D0, 0x0006); // DebuggerLockMemory selector

    let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
    assert!(result.is_some(), "DebugUtil should be handled");
    assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A0),
        0x0012_3000,
        "DebugUtil selector $0006 should preserve A0"
    );
    assert_eq!(
        cpu.read_reg(Register::A1),
        0x0012_3F00,
        "DebugUtil selector $0006 should preserve A1"
    );
    assert_eq!(
        cpu.read_reg(Register::D1),
        0x1234_5678,
        "DebugUtil selector $0006 should preserve D1"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DebugUtil selector $0006 should rewrite only D0 to noErr"
    );
}

#[test]
fn debugutil_debuggerpoll_preserves_stack_sentinel_and_returns_noerr() {
    // Selector $0003 (DebuggerPoll) is part of the documented
    // DebugUtil table in Inside Macintosh Volume VI 1991,
    // Appendix C table C-3 (p. C-4).
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08D;
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0x1357_9BDF);
    cpu.write_reg(Register::D0, 0x0003);
    cpu.write_reg(Register::A0, 0x00FE_DCBA);
    cpu.write_reg(Register::A1, 0x00AB_CDEF);

    let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
    assert!(result.is_some(), "DebugUtil should be handled");
    assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "DebugUtil selector $0003 should not consume a stack frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0x1357_9BDF,
        "DebugUtil selector $0003 should leave the stack sentinel untouched"
    );
    assert_eq!(cpu.read_reg(Register::A0), 0x00FE_DCBA);
    assert_eq!(cpu.read_reg(Register::A1), 0x00AB_CDEF);
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DebugUtil selector $0003 should return noErr"
    );
}

#[test]
fn debugutil_debuggerpoll_updates_dispatcher_ccr_for_noerr() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08D;
    cpu.set_ccr(0x10);
    cpu.write_reg(Register::D0, 0x0003);

    let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
    assert!(result.is_some(), "DebugUtil should be handled");
    assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DebugUtil selector $0003 should return noErr"
    );
    assert_eq!(
        cpu.get_ccr(),
        0x14,
        "DebugUtil selector $0003 should mirror noErr into CCR without disturbing X"
    );
}

#[test]
fn debugutil_debuggergetmax_preserves_non_d0_registers() {
    // Selector $0000 should use the same register-only ABI and
    // leave the scratch registers alone while returning 8 in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08D;
    cpu.write_reg(Register::A0, 0x00AA_5500);
    cpu.write_reg(Register::A1, 0x00BB_6600);
    cpu.write_reg(Register::D1, 0x1234_5678);
    cpu.write_reg(Register::D0, 0x0000);

    let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
    assert!(result.is_some(), "DebugUtil should be handled");
    assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
    assert_eq!(cpu.read_reg(Register::A0), 0x00AA_5500);
    assert_eq!(cpu.read_reg(Register::A1), 0x00BB_6600);
    assert_eq!(cpu.read_reg(Register::D1), 0x1234_5678);
    assert_eq!(cpu.read_reg(Register::D0), 8);
}

#[test]
fn debugutil_five_call_composition_preserves_stack_across_documented_selectors() {
    // 5 successive
    // _DebugUtil dispatches with varying D0 selector inputs spanning
    // the documented IM:VI 1991 Appendix C table C-3 (p. C-4) range
    // ($0000 DebuggerGetMax / $0001 DebuggerEnter / $0003 DebuggerPoll
    // / $0005 PageFaultFatal / $0008 EnterSupervisorMode) preserve A7
    // in aggregate AND leave the SP+0 sentinel untouched.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08D;
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xDEAD_BEEF);

    let selectors: [u32; 5] = [0x0000, 0x0001, 0x0003, 0x0005, 0x0008];
    for selector in selectors {
        cpu.write_reg(Register::D0, selector);
        let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugUtil should be handled");
        assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "5-call DebugUtil composition should preserve A7 in aggregate"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xDEAD_BEEF,
        "SP+0 sentinel should survive 5-call DebugUtil composition"
    );
}

#[test]
fn debugutil_debuggerpoll_five_call_composition_preserves_stack_across_repeated_selector_three_calls(
) {
    // Five successive
    // DebuggerPoll calls preserve A7 in aggregate and continue to
    // return noErr across the repeated selector-3 path.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08D;
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xCAFE_BABE);

    for _ in 0..5 {
        cpu.write_reg(Register::D0, 0x0003);
        let result = dispatcher.dispatch_memory(false, 0x8D, &mut cpu, &mut bus);
        assert!(result.is_some(), "DebugUtil should be handled");
        assert!(result.unwrap().is_ok(), "DebugUtil should return cleanly");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "DebugUtil selector $0003 should return noErr"
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "five DebuggerPoll calls should preserve A7 in aggregate"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xCAFE_BABE,
        "five DebuggerPoll calls should leave the stack sentinel untouched"
    );
}

#[test]
fn deferuserfn_uses_register_calling_convention_without_stack_arguments() {
    // Inside Macintosh Volume VI (1991), p. 28-30; Inside Macintosh:
    // Memory (1992), p. 3-33: DeferUserFn uses D0(argument)/A0(function)
    // registers and returns result in D0. This test exercises the safe
    // fallback path for a non-callable placeholder pointer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08F;
    let sp_before = cpu.read_reg(Register::A7);
    let pc_before = 0x00BA_D000;
    bus.write_long(sp_before, 0xBADC_0DE0);
    cpu.write_reg(Register::A0, 0x00C0_FFEE);
    cpu.write_reg(Register::D0, 0x0012_3400);
    cpu.write_reg(Register::PC, pc_before);

    let result = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
    assert!(result.is_some(), "DeferUserFn should be handled");
    assert!(result.unwrap().is_ok(), "DeferUserFn should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "DeferUserFn should not pop a Pascal stack argument frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xBADC_0DE0,
        "stack top should remain untouched by DeferUserFn register ABI"
    );
    assert_eq!(
        cpu.read_reg(Register::PC),
        pc_before,
        "non-callable DeferUserFn fallback should not install a trampoline"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DeferUserFn stub should return noErr in D0"
    );
}

#[test]
fn deferuserfn_nominal_stub_returns_noerr() {
    // Inside Macintosh Volume VI (1991), p. 28-30: DeferUserFn returns an
    // OSErr in D0. This is the safe non-callable-pointer fallback.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08F;
    cpu.write_reg(Register::A0, 0x00D0_0000);
    cpu.write_reg(Register::D0, 0x00E0_0000);

    let result = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
    assert!(result.is_some(), "DeferUserFn should be handled");
    assert!(result.unwrap().is_ok(), "DeferUserFn should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DeferUserFn stub should report noErr for nominal calls"
    );
}

#[test]
fn deferuserfn_callable_pointer_installs_trampoline_and_returns_noerr() {
    // Valid callable-proc path: the trap should inject a trampoline,
    // pass the argument in A0, and return noErr in D0 before the
    // trampoline executes.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08F;

    let pc_after_trap = 0x00BA_D100;
    let sp_before = cpu.read_reg(Register::A7);
    let user_fn = bus.alloc(8);
    bus.write_word(user_fn, 0x4E56); // LINK A6,#0 — looks like a real proc
    bus.write_word(user_fn + 2, 0x0000);
    bus.write_word(user_fn + 4, 0x4E75); // RTS

    cpu.write_reg(Register::PC, pc_after_trap);
    cpu.write_reg(Register::A0, user_fn);
    cpu.write_reg(Register::D0, 0x00DE_ADBE);

    let result = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
    assert!(result.is_some(), "DeferUserFn should be handled");
    assert!(result.unwrap().is_ok(), "DeferUserFn should return cleanly");

    let tramp = dispatcher.defer_user_fn_trampoline;
    assert_ne!(tramp, 0, "DeferUserFn should allocate a trampoline");
    assert_eq!(
        cpu.read_reg(Register::PC),
        tramp,
        "trampoline PC should be installed"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before - 4,
        "DeferUserFn should push a return address for the trampoline"
    );
    assert_eq!(
        bus.read_long(sp_before - 4),
        pc_after_trap,
        "trampoline should resume at the post-trap PC"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "DeferUserFn should return noErr in D0"
    );
    assert_eq!(
        bus.read_word(tramp),
        0x48E7,
        "trampoline should save scratch registers"
    );
    assert_eq!(
        bus.read_word(tramp + 2),
        0xF0F0,
        "trampoline should save D0-D3/A0-A3"
    );
    assert_eq!(
        bus.read_word(tramp + 4),
        0x207C,
        "trampoline should load the argument into A0"
    );
    assert_eq!(
        bus.read_long(tramp + 6),
        0x00DE_ADBE,
        "trampoline should patch the argument literal"
    );
    assert_eq!(
        bus.read_word(tramp + 10),
        0x4EB9,
        "trampoline should JSR to the user function"
    );
    assert_eq!(
        bus.read_long(tramp + 12),
        user_fn,
        "trampoline should patch the callback address"
    );
    assert_eq!(
        bus.read_word(tramp + 16),
        0x4CDF,
        "trampoline should restore scratch registers"
    );
    assert_eq!(
        bus.read_word(tramp + 18),
        0x0F0F,
        "trampoline should restore D0-D3/A0-A3"
    );
    assert_eq!(
        bus.read_word(tramp + 20),
        0x7000,
        "trampoline should clear D0 to noErr"
    );
    assert_eq!(
        bus.read_word(tramp + 22),
        0x4E75,
        "trampoline should RTS back to the caller"
    );
}

#[test]
fn deferuserfn_five_call_composition_preserves_stack_across_varying_args() {
    // 5 successive _DeferUserFn
    // dispatches with varying (A0=userFunction, D0=argument) inputs
    // span the IM:Memory 1992 p. 3-33 register convention. Per-call
    // pop-discipline errors accumulate; the 5-call composition
    // catches drift even when a single call's discipline is correct.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08F;
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xBADC_0DE0);

    // Five (userFunction, argument) tuples spanning small/large
    // pointer values and zero-vs-nonzero arguments.
    let inputs = [
        (0x0040_0000u32, 0x0000_0000u32),
        (0x00C0_0000u32, 0x0000_0001u32),
        (0x0050_8000u32, 0xDEAD_BEEFu32),
        (0x00B0_C000u32, 0x0000_FFFFu32),
        (0x0080_0000u32, 0xCAFE_BABEu32),
    ];
    for (a0, d0) in inputs.iter().copied() {
        cpu.write_reg(Register::A0, a0);
        cpu.write_reg(Register::D0, d0);
        let r = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
        assert!(r.is_some(), "DeferUserFn should be handled");
        assert!(r.unwrap().is_ok(), "DeferUserFn should return cleanly");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "5-call DeferUserFn composition should preserve A7 in aggregate"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xBADC_0DE0,
        "SP+0 sentinel should survive 5-call DeferUserFn composition"
    );
}

#[test]
fn deferuserfn_three_call_composition_preserves_stack_across_varying_args() {
    // Three successive DeferUserFn dispatches with varying A0/D0
    // inputs should still preserve the Pascal stack discipline.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA08F;
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xBADC_0DE0);

    let user_fn = bus.alloc(8);
    bus.write_word(user_fn, 0x4E56); // LINK A6,#0 — looks like a real proc
    bus.write_word(user_fn + 2, 0x0000);
    bus.write_word(user_fn + 4, 0x4E75); // RTS

    let inputs = [
        (0x0040_0000u32, 0x0000_0000u32),
        (0x00C0_0000u32, 0x0000_0001u32),
        (0x0050_8000u32, 0xDEAD_BEEFu32),
    ];
    for (a0, d0) in inputs.iter().copied() {
        cpu.write_reg(Register::A0, a0);
        cpu.write_reg(Register::D0, d0);
        let r = dispatcher.dispatch_memory(false, 0x8F, &mut cpu, &mut bus);
        assert!(r.is_some(), "DeferUserFn should be handled");
        assert!(r.unwrap().is_ok(), "DeferUserFn should return cleanly");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "3-call DeferUserFn composition should preserve A7 in aggregate"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xBADC_0DE0,
        "SP+0 sentinel should survive 3-call DeferUserFn composition"
    );
}

#[test]
fn nminstall_returns_noerr_for_nominal_notification_request() {
    // Inside Macintosh Volume VI (1991), p. 24-10:
    // NMInstall returns noErr for valid notification requests.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let nm_rec = bus.alloc(36);
    bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
    cpu.write_reg(Register::A0, nm_rec);

    let result = dispatcher.dispatch_memory(false, 0x5E, &mut cpu, &mut bus);
    assert!(result.is_some(), "NMInstall should be handled");
    assert!(result.unwrap().is_ok(), "NMInstall should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "NMInstall should return noErr in D0 for nominal input"
    );
    assert_eq!(dispatcher.notification_requests, vec![nm_rec]);
    assert_eq!(bus.read_long(nm_rec), 0, "the queue tail qLink is NIL");
}

#[test]
fn nminstall_uses_a0_nmrecptr_register_calling_convention() {
    // Inside Macintosh Volume VI (1991), p. 24-10:
    // NMInstall takes NMRecPtr in A0 and returns OSErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let nm_rec = bus.alloc(36);
    bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xFEED_FACE);
    cpu.write_reg(Register::A0, nm_rec);

    let result = dispatcher.dispatch_memory(false, 0x5E, &mut cpu, &mut bus);
    assert!(result.is_some(), "NMInstall should be handled");
    assert!(result.unwrap().is_ok(), "NMInstall should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "NMInstall should not consume a Pascal stack argument frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xFEED_FACE,
        "stack top should remain untouched by register calling convention"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "NMInstall should return noErr"
    );
}

#[test]
fn nmremove_returns_noerr_for_nominal_notification_request() {
    // Inside Macintosh Volume VI (1991), p. 24-11:
    // NMRemove returns noErr for a successful request removal.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let nm_rec = bus.alloc(36);
    bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
    cpu.write_reg(Register::A0, nm_rec);
    dispatcher
        .dispatch_memory(false, 0x5E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let result = dispatcher.dispatch_memory(false, 0x5F, &mut cpu, &mut bus);
    assert!(result.is_some(), "NMRemove should be handled");
    assert!(result.unwrap().is_ok(), "NMRemove should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "NMRemove should return noErr in D0 for nominal input"
    );
    assert!(dispatcher.notification_requests.is_empty());
}

#[test]
fn nmremove_uses_a0_nmrecptr_register_calling_convention() {
    // Inside Macintosh Volume VI (1991), p. 24-11:
    // NMRemove takes NMRecPtr in A0 and returns OSErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let nm_rec = bus.alloc(36);
    bus.write_word(nm_rec + 4, 8); // qType = ORD(nmType)
    cpu.write_reg(Register::A0, nm_rec);
    dispatcher
        .dispatch_memory(false, 0x5E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xC0DE_CAFE);
    cpu.write_reg(Register::A0, nm_rec);

    let result = dispatcher.dispatch_memory(false, 0x5F, &mut cpu, &mut bus);
    assert!(result.is_some(), "NMRemove should be handled");
    assert!(result.unwrap().is_ok(), "NMRemove should return cleanly");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "NMRemove should not consume a Pascal stack argument frame"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0xC0DE_CAFE,
        "stack top should remain untouched by register calling convention"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "NMRemove should return noErr"
    );
}

#[test]
fn nminstall_rejects_invalid_queue_type() {
    // Inside Macintosh Volume VI (1991), p. 24-10: qType must be 8.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let nm_rec = bus.alloc(36);
    bus.write_word(nm_rec + 4, 7);
    cpu.write_reg(Register::A0, nm_rec);

    dispatcher
        .dispatch_memory(false, 0x5E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0) as u16 as i16, -299);
    assert!(dispatcher.notification_requests.is_empty());
}

#[test]
fn notification_queue_links_requests_and_nmremove_unlinks_them() {
    // Inside Macintosh Volume VI (1991), pp. 24-6 and 24-10 to 24-11:
    // NMRec is a QElem, and NMRemove reports qErr for a missing request.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let first = bus.alloc(36);
    let second = bus.alloc(36);
    bus.write_word(first + 4, 8);
    bus.write_word(second + 4, 8);

    for nm_rec in [first, second] {
        cpu.write_reg(Register::A0, nm_rec);
        dispatcher
            .dispatch_memory(false, 0x5E, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
    }
    assert_eq!(dispatcher.notification_requests, vec![first, second]);
    assert_eq!(bus.read_long(first), second);
    assert_eq!(bus.read_long(second), 0);

    cpu.write_reg(Register::A0, first);
    dispatcher
        .dispatch_memory(false, 0x5F, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(dispatcher.notification_requests, vec![second]);
    assert_eq!(bus.read_long(first), 0);

    dispatcher
        .dispatch_memory(false, 0x5F, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as u16 as i16, -1);
}

#[test]
fn nminstall_minus_one_response_automatically_removes_request() {
    // Inside Macintosh Volume VI (1991), pp. 24-7 to 24-8: nmResp=-1
    // selects the predefined response that removes the queue element.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let nm_rec = bus.alloc(36);
    bus.write_word(nm_rec + 4, 8);
    bus.write_long(nm_rec + 28, u32::MAX);
    cpu.write_reg(Register::A0, nm_rec);

    dispatcher
        .dispatch_memory(false, 0x5E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(dispatcher.notification_requests.is_empty());
    assert_eq!(bus.read_long(nm_rec), 0);
}

#[test]
fn nminstall_arms_pascal_response_with_notification_record_argument() {
    // Inside Macintosh Volume VI (1991), p. 24-8: MyResponse receives one
    // NMRecPtr parameter after the notification has been posted.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let nm_rec = bus.alloc(36);
    let response = bus.alloc(8);
    bus.write_word(nm_rec + 4, 8);
    bus.write_long(nm_rec + 28, response);
    bus.write_word(response, 0x4E56); // LINK A6,#0
    bus.write_word(response + 2, 0);
    bus.write_word(response + 4, 0x4E74); // RTD #4
    bus.write_word(response + 6, 4);
    let resume_pc = 0x00BA_D200;
    let initial_sp = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::PC, resume_pc);
    cpu.write_reg(Register::A0, nm_rec);

    dispatcher
        .dispatch_memory(false, 0x5E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let trampoline = cpu.read_reg(Register::PC);
    assert_ne!(trampoline, resume_pc);
    assert_eq!(cpu.read_reg(Register::A7), initial_sp - 4);
    assert_eq!(bus.read_long(initial_sp - 4), resume_pc);
    assert_eq!(bus.read_word(trampoline + 4), 0x2F3C);
    assert_eq!(bus.read_long(trampoline + 6), nm_rec);
    assert_eq!(bus.read_word(trampoline + 10), 0x4EB9);
    assert_eq!(bus.read_long(trampoline + 12), response);
    assert_eq!(dispatcher.notification_requests, vec![nm_rec]);
}

#[test]
fn notification_response_executes_and_can_be_explicitly_removed() {
    // Inside Macintosh Volume VI (1991), pp. 24-8 and 24-11: a response
    // procedure receives NMRecPtr and may leave the request queued until
    // the application explicitly passes that same pointer to NMRemove.
    let (mut dispatcher, _, mut bus) = setup();
    let mut cpu = crate::cpu::M68kCpu::new();
    let nm_rec = bus.alloc(36);
    let response = bus.alloc(24);
    bus.write_word(nm_rec + 4, 8);
    bus.write_long(nm_rec + 28, response);

    bus.write_word(response, 0x4E56); // LINK A6,#0
    bus.write_word(response + 2, 0);
    bus.write_word(response + 4, 0x206E); // MOVEA.L 8(A6),A0
    bus.write_word(response + 6, 8);
    bus.write_word(response + 8, 0x217C); // MOVE.L #marker,32(A0)
    bus.write_long(response + 10, 0x4E4D_5253);
    bus.write_word(response + 14, 32);
    bus.write_word(response + 16, 0x4E5E); // UNLK A6
    bus.write_word(response + 18, 0x4E74); // RTD #4
    bus.write_word(response + 20, 4);

    let resume_pc = bus.alloc(2);
    bus.write_word(resume_pc, 0x4E71); // NOP
    let initial_sp = TEST_SP;
    cpu.write_reg(Register::PC, resume_pc);
    cpu.write_reg(Register::A7, initial_sp);
    cpu.write_reg(Register::A0, nm_rec);
    dispatcher
        .dispatch_memory(false, 0x5E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    for _ in 0..16 {
        if cpu.read_reg(Register::PC) == resume_pc {
            break;
        }
        assert!(matches!(
            cpu.step(&mut bus),
            crate::cpu::StepResult::Ok { .. }
        ));
    }
    assert_eq!(cpu.read_reg(Register::PC), resume_pc);
    assert_eq!(cpu.read_reg(Register::A7), initial_sp);
    assert_eq!(bus.read_long(nm_rec + 32), 0x4E4D_5253);
    assert_eq!(dispatcher.notification_requests, vec![nm_rec]);

    cpu.write_reg(Register::A0, nm_rec);
    dispatcher
        .dispatch_memory(false, 0x5F, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert!(dispatcher.notification_requests.is_empty());
}

#[test]
fn lowertext_variant_converts_ascii_and_macroman_uppercase_to_lowercase() {
    // Inside Macintosh Volume VI (1991), p. 14-62: LowerText localizes
    // lowercase conversion for len bytes at A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let input = [b'A', 0x83, b'Z', b'!', 0x84, 0x8E];
    let ptr = bus.alloc(input.len() as u32);
    bus.write_bytes(ptr, &input);

    dispatcher.current_trap_word = 0xA056;
    cpu.write_reg(Register::A0, ptr);
    cpu.write_reg(Register::D0, 5);
    let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

    assert!(result.is_some(), "LowerText variant should be handled");
    assert!(result.unwrap().is_ok(), "LowerText should return");
    assert_eq!(
        bus.read_bytes(ptr, input.len()),
        vec![b'a', 0x8E, b'z', b'!', 0x96, 0x8E],
        "LowerText should lowercase only the requested len bytes"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "LowerText should return noErr in D0"
    );
}

#[test]
fn uprstring_variants_preserve_or_strip_mac_roman_marks() {
    // Inside Macintosh: Text (1993), pp. 5-64--5-65: the bare form has
    // diacSens=TRUE, while MARKS has diacSens=FALSE and strips marks.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    for (trap_word, expected) in [
        (0xA054u16, [0x83, 0x80, b'Q']),
        (0xA254u16, [b'E', b'A', b'Q']),
    ] {
        let ptr = bus.alloc(3);
        bus.write_bytes(ptr, &[0x8E, 0x8A, b'q']);
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 3);

        let result = dispatcher.dispatch_memory(false, 0x54, &mut cpu, &mut bus);

        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(bus.read_bytes(ptr, 3), expected, "trap ${trap_word:04X}");
        assert_eq!(cpu.read_reg(Register::A0), ptr);
    }
}

#[test]
fn uppertext_variant_converts_ascii_and_macroman_lowercase_to_uppercase() {
    // Inside Macintosh Volume VI (1991), p. 14-63: UpperText localizes
    // uppercase conversion for len bytes at A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let input = [b'a', 0x8E, b'z', b'?', 0x96, 0x8A];
    let ptr = bus.alloc(input.len() as u32);
    bus.write_bytes(ptr, &input);

    dispatcher.current_trap_word = 0xA456;
    cpu.write_reg(Register::A0, ptr);
    cpu.write_reg(Register::D0, 5);
    let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

    assert!(result.is_some(), "UpperText variant should be handled");
    assert!(result.unwrap().is_ok(), "UpperText should return");
    assert_eq!(
        bus.read_bytes(ptr, input.len()),
        vec![b'A', 0x83, b'Z', b'?', 0x84, 0x8A],
        "UpperText should uppercase only the requested len bytes"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "UpperText should return noErr in D0"
    );
}

#[test]
fn striptext_variant_strips_diacriticals_without_case_fold() {
    // Inside Macintosh Volume VI (1991), p. 14-63: StripText removes
    // diacritical marks without forcing uppercase.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let input = [0x8E, 0x83, 0x96, 0x84, b'Q', 0x9A];
    let ptr = bus.alloc(input.len() as u32);
    bus.write_bytes(ptr, &input);

    dispatcher.current_trap_word = 0xA256;
    cpu.write_reg(Register::A0, ptr);
    cpu.write_reg(Register::D0, 5);
    let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

    assert!(result.is_some(), "StripText variant should be handled");
    assert!(result.unwrap().is_ok(), "StripText should return");
    assert_eq!(
        bus.read_bytes(ptr, input.len()),
        vec![b'e', b'E', b'n', b'N', b'Q', 0x9A],
        "StripText should strip marks while preserving letter case"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "StripText should return noErr in D0"
    );
}

#[test]
fn stripuppertext_variant_strips_diacriticals_and_uppercases() {
    // Inside Macintosh Volume VI (1991), p. 14-63: StripUpperText strips
    // diacritical marks and uppercases text for len bytes at A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let input = [0x8E, 0x8A, b'x', 0x96, 0xCF, b'?'];
    let ptr = bus.alloc(input.len() as u32);
    bus.write_bytes(ptr, &input);

    dispatcher.current_trap_word = 0xA656;
    cpu.write_reg(Register::A0, ptr);
    cpu.write_reg(Register::D0, 5);
    let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);

    assert!(result.is_some(), "StripUpperText variant should be handled");
    assert!(result.unwrap().is_ok(), "StripUpperText should return");
    assert_eq!(
        bus.read_bytes(ptr, input.len()),
        vec![b'E', b'A', b'X', b'N', b'O', b'?'],
        "StripUpperText should strip marks and uppercase requested bytes"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "StripUpperText should return noErr in D0"
    );
}

#[test]
fn lowertext_family_variants_return_noerr_in_d0_for_nominal_calls() {
    // Inside Macintosh Volume VI (1991), p. 14-63: LowerText-family
    // result codes include noErr and resNotFound; nominal calls return noErr.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    for trap_word in [0xA056u16, 0xA256, 0xA456, 0xA656] {
        let ptr = bus.alloc(3);
        bus.write_bytes(ptr, b"Ae?");
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 3);
        let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "LowerText-family variant ${trap_word:04X} should be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "LowerText-family variant ${trap_word:04X} should return"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LowerText-family variant ${trap_word:04X} should return noErr in D0"
        );
    }
}

#[test]
fn lowertext_family_returns_noerr_in_d0_for_each_trap_word_variant() {
    // MPW Universal Headers declare the LowerText family as
    // void-returning, so C-side callers cannot sample D0 without
    // inline asm. This test pins D0=0 (noErr) on Systemless for each
    // $A056/$A256/$A456/$A656 dispatch over the input bytes
    // {0x41, 0x61, 0x83, 0x8E}, completing the round-trip contract.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    for trap_word in [0xA056u16, 0xA256, 0xA456, 0xA656] {
        let ptr = bus.alloc(4);
        bus.write_bytes(ptr, &[0x41, 0x61, 0x83, 0x8E]);
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 4);
        let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "LowerText-family variant ${trap_word:04X} dispatch must be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "LowerText-family variant ${trap_word:04X} dispatch must return Ok"
        );
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "LowerText-family variant ${trap_word:04X} must return noErr in D0 per IM:VI 14-63"
        );
    }
}

#[test]
fn lowertext_family_variants_match_documented_output_byte_sequences() {
    // Input: {0x41 'A', 0x61 'a', 0x83 'É', 0x8E 'é'} — 4 bytes.
    //
    // Documented outputs per IM:VI 14-62..14-63 and IM:IV IV-235:
    //   $A056 LowerText      → {0x61, 0x61, 0x8E, 0x8E}  "aa\x8E\x8E"
    //   $A456 UpperText      → {0x41, 0x41, 0x83, 0x83}  "AA\x83\x83"
    //   $A256 StripText      → {0x41, 0x61, 0x45, 0x65}  "AaEe"
    //   $A656 StripUpperText → {0x41, 0x41, 0x45, 0x45}  "AAEE"
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let cases: &[(u16, [u8; 4])] = &[
        (0xA056, [0x61, 0x61, 0x8E, 0x8E]),
        (0xA456, [0x41, 0x41, 0x83, 0x83]),
        (0xA256, [0x41, 0x61, 0x45, 0x65]),
        (0xA656, [0x41, 0x41, 0x45, 0x45]),
    ];
    for &(trap_word, expected) in cases {
        let ptr = bus.alloc(4);
        bus.write_bytes(ptr, &[0x41, 0x61, 0x83, 0x8E]);
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, ptr);
        cpu.write_reg(Register::D0, 4);
        let result = dispatcher.dispatch_memory(false, 0x56, &mut cpu, &mut bus);
        assert!(
            result.is_some(),
            "Variant ${trap_word:04X} dispatch must be handled"
        );
        assert!(
            result.unwrap().is_ok(),
            "Variant ${trap_word:04X} dispatch must return Ok"
        );
        assert_eq!(
            bus.read_bytes(ptr, 4),
            expected.to_vec(),
            "Variant ${trap_word:04X} must produce documented byte sequence"
        );
    }
}

#[test]
fn relstring_variants_apply_documented_case_and_marks_sensitivity() {
    // Inside Macintosh: Text (1993), pp. 5-60--5-61: the bare form
    // ignores case and marks; MARKS and CASE independently enable them.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let accented_lower = bus.alloc(1);
    let plain_lower = bus.alloc(1);
    let plain_upper = bus.alloc(1);
    bus.write_byte(accented_lower, 0x8E); // Mac Roman e acute
    bus.write_byte(plain_lower, b'e');
    bus.write_byte(plain_upper, b'E');

    for (trap_word, left, right, expected) in [
        (0xA050, accented_lower, plain_upper, 0),
        (0xA250, accented_lower, plain_upper, 1),
        (0xA450, accented_lower, plain_upper, 1),
        (0xA650, accented_lower, plain_upper, 1),
        (0xA050, plain_lower, plain_upper, 0),
        (0xA250, plain_lower, plain_upper, 0),
        (0xA450, plain_lower, plain_upper, 1),
        (0xA650, plain_lower, plain_upper, 1),
    ] {
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::A0, left);
        cpu.write_reg(Register::A1, right);
        cpu.write_reg(Register::D0, 0x0001_0001);
        let result = dispatcher.dispatch_memory(false, 0x50, &mut cpu, &mut bus);
        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(
            cpu.read_reg(Register::D0),
            expected,
            "trap ${trap_word:04X}"
        );
    }
}

#[test]
fn setvideodefault_roundtrips_defvideorec_through_getvideodefault() {
    // Inside Macintosh Volume V (1986), pp. V-354..V-355:
    // SetVideoDefault consumes DefVideoRec {sdSlot, sdSResource}, and
    // GetVideoDefault returns the stored default video record.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let set_pb = bus.alloc(2);
    bus.write_byte(set_pb, 0x0E);
    bus.write_byte(set_pb + 1, 0x2A);

    cpu.write_reg(Register::A0, set_pb);
    let set_result = dispatcher.dispatch_memory(false, 0x81, &mut cpu, &mut bus);
    assert!(set_result.is_some(), "SetVideoDefault should be handled");
    assert!(
        set_result.unwrap().is_ok(),
        "SetVideoDefault should succeed"
    );

    let get_pb = bus.alloc(2);
    cpu.write_reg(Register::A0, get_pb);
    let get_result = dispatcher.dispatch_memory(false, 0x80, &mut cpu, &mut bus);
    assert!(get_result.is_some(), "GetVideoDefault should be handled");
    assert!(
        get_result.unwrap().is_ok(),
        "GetVideoDefault should succeed"
    );
    assert_eq!(
        bus.read_byte(get_pb),
        0x0E,
        "GetVideoDefault should return sdSlot written by SetVideoDefault"
    );
    assert_eq!(
        bus.read_byte(get_pb + 1),
        0x2A,
        "GetVideoDefault should return sdSResource written by SetVideoDefault"
    );
}

#[test]
fn getvideodefault_writes_two_bytes_and_preserves_following_memory() {
    // Inside Macintosh Volume V (1986), p. V-354: DefVideoRec is two bytes
    // (sdSlot, sdSResource), so GetVideoDefault should write exactly that
    // record at A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = bus.alloc(4);
    bus.write_byte(pb, 0xAA);
    bus.write_byte(pb + 1, 0xBB);
    bus.write_byte(pb + 2, 0xC3);
    bus.write_byte(pb + 3, 0xD4);

    cpu.write_reg(Register::A0, pb);
    let result = dispatcher.dispatch_memory(false, 0x80, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetVideoDefault should be handled");
    assert!(result.unwrap().is_ok(), "GetVideoDefault should succeed");
    assert_eq!(
        bus.read_byte(pb),
        0,
        "Default sdSlot should be 0 when no explicit video default is set"
    );
    assert_eq!(
        bus.read_byte(pb + 1),
        0,
        "Default sdSResource should be 0 when no explicit video default is set"
    );
    assert_eq!(
        bus.read_byte(pb + 2),
        0xC3,
        "GetVideoDefault must not overwrite bytes beyond DefVideoRec"
    );
    assert_eq!(
        bus.read_byte(pb + 3),
        0xD4,
        "GetVideoDefault must preserve trailing bytes in caller memory"
    );
}

#[test]
fn setosdefault_roundtrips_sdostype_and_getosdefault_reports_reserved_zero() {
    // Inside Macintosh Volume V (1986), p. V-355: SetOSDefault specifies
    // sdOSType; sdReserved is reserved and should be 0 when read back via
    // GetOSDefault.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let set_pb = bus.alloc(2);
    bus.write_byte(set_pb, 0x7F);
    bus.write_byte(set_pb + 1, 0x05);

    cpu.write_reg(Register::A0, set_pb);
    let set_result = dispatcher.dispatch_memory(false, 0x83, &mut cpu, &mut bus);
    assert!(set_result.is_some(), "SetOSDefault should be handled");
    assert!(set_result.unwrap().is_ok(), "SetOSDefault should succeed");

    let get_pb = bus.alloc(2);
    cpu.write_reg(Register::A0, get_pb);
    let get_result = dispatcher.dispatch_memory(false, 0x84, &mut cpu, &mut bus);
    assert!(get_result.is_some(), "GetOSDefault should be handled");
    assert!(get_result.unwrap().is_ok(), "GetOSDefault should succeed");
    assert_eq!(
        bus.read_byte(get_pb),
        0,
        "GetOSDefault should report sdReserved as 0"
    );
    assert_eq!(
        bus.read_byte(get_pb + 1),
        0x05,
        "GetOSDefault should return sdOSType written by SetOSDefault"
    );
}

#[test]
fn setosdefault_preserves_caller_defosrec_input_bytes_read_only_a0() {
    // Inside Macintosh Volume V (1986), p. V-355: SetOSDefault's parameter
    // block direction arrows (`→` in both rows of the trap-macro summary)
    // document sdReserved + sdOSType as INPUTs supplied by the caller; the
    // trap copies them into the in-session default record without writing
    // back to the caller's buffer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = bus.alloc(4);
    bus.write_byte(pb, 0x00);
    bus.write_byte(pb + 1, 0x02);
    bus.write_byte(pb + 2, 0xCC);
    bus.write_byte(pb + 3, 0xCC);

    cpu.write_reg(Register::A0, pb);
    let result = dispatcher.dispatch_memory(false, 0x83, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetOSDefault should be handled");
    assert!(result.unwrap().is_ok(), "SetOSDefault should succeed");

    assert_eq!(
        bus.read_byte(pb),
        0x00,
        "SetOSDefault must not modify caller's sdReserved input byte"
    );
    assert_eq!(
        bus.read_byte(pb + 1),
        0x02,
        "SetOSDefault must not modify caller's sdOSType input byte"
    );
    assert_eq!(
        bus.read_byte(pb + 2),
        0xCC,
        "SetOSDefault must not clobber memory past DefOSRec at byte +2"
    );
    assert_eq!(
        bus.read_byte(pb + 3),
        0xCC,
        "SetOSDefault must not clobber memory past DefOSRec at byte +3"
    );
}

#[test]
fn getosdefault_writes_two_bytes_and_preserves_following_memory() {
    // Inside Macintosh Volume V (1986), p. V-355: DefOSRec is two bytes
    // (sdReserved, sdOSType), and Macintosh OS is represented by
    // sdOSType=1.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = bus.alloc(4);
    bus.write_byte(pb, 0xAA);
    bus.write_byte(pb + 1, 0xBB);
    bus.write_byte(pb + 2, 0xC3);
    bus.write_byte(pb + 3, 0xD4);

    cpu.write_reg(Register::A0, pb);
    let result = dispatcher.dispatch_memory(false, 0x84, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetOSDefault should be handled");
    assert!(result.unwrap().is_ok(), "GetOSDefault should succeed");
    assert_eq!(
        bus.read_byte(pb),
        0,
        "GetOSDefault should return sdReserved as 0"
    );
    assert_eq!(
        bus.read_byte(pb + 1),
        1,
        "GetOSDefault should default sdOSType to Macintosh OS (1)"
    );
    assert_eq!(
        bus.read_byte(pb + 2),
        0xC3,
        "GetOSDefault must not overwrite bytes beyond DefOSRec"
    );
    assert_eq!(
        bus.read_byte(pb + 3),
        0xD4,
        "GetOSDefault must preserve trailing bytes in caller memory"
    );
}

#[test]
fn test_free_mem() {
    // FreeMem returns `free_heap_estimate` clamped to [24MB, 64MB].
    // setup() leaves HEAP_END/APPL_LIMIT at 0 so the floor (24MB) wins.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);
    assert!(result.is_some(), "FreeMem should be handled");
    assert!(result.unwrap().is_ok(), "FreeMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        24 * 1024 * 1024,
        "FreeMem should return 24MB clamp floor in D0"
    );
}

#[test]
fn free_mem_honors_explicit_application_partition_below_compat_floor() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let heap_end = 0x0020_0000 + 64;
    let appl_limit = 0x0050_0000;
    bus.write_long(crate::memory::globals::addr::MEM_TOP, 32 * 1024 * 1024);
    bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);

    let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);

    assert!(result.is_some(), "FreeMem should be handled");
    assert!(result.unwrap().is_ok(), "FreeMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        appl_limit - heap_end,
        "FreeMem must report the actual small partition instead of the compatibility floor"
    );
}

#[test]
fn free_mem_uses_zone_zcbfree_after_maxapplzone_extends_heapend() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let app_zone = 0x0020_0000;
    let appl_limit = 0x0220_0000;
    let zcb_free = 0x01FF_DFC0;
    bus.write_long(crate::memory::globals::addr::MEM_TOP, 64 * 1024 * 1024);
    bus.write_long(crate::memory::globals::addr::APP_L_ZONE, app_zone);
    bus.write_long(crate::memory::globals::addr::THE_ZONE, app_zone);
    bus.write_long(crate::memory::globals::addr::HEAP_END, appl_limit);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);
    bus.write_long(app_zone, appl_limit); // bkLim
    bus.write_long(app_zone + 12, zcb_free); // zcbFree

    let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);

    assert!(result.is_some(), "FreeMem should be handled");
    assert!(result.unwrap().is_ok(), "FreeMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        zcb_free,
        "FreeMem must read the zone header's free-byte count after MaxApplZone"
    );
}

#[test]
fn free_mem_survives_setapplimit_then_maxapplzone_startup_sequence() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let app_zone = 0x0020_0000;
    let heap_end = app_zone + 64;
    let original_limit = 0x006A_E000;
    let lowered_limit = original_limit - 0x4000;
    let original_free = original_limit - app_zone - 64;
    let lowered_free = original_free - 0x4000;
    bus.write_long(crate::memory::globals::addr::MEM_TOP, 64 * 1024 * 1024);
    bus.write_long(crate::memory::globals::addr::APP_L_ZONE, app_zone);
    bus.write_long(crate::memory::globals::addr::THE_ZONE, app_zone);
    bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, original_limit);
    bus.write_long(app_zone, original_limit); // bkLim
    bus.write_long(app_zone + 12, original_free); // zcbFree

    cpu.write_reg(Register::A0, lowered_limit);
    let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetApplLimit should be handled");
    assert!(result.unwrap().is_ok(), "SetApplLimit should succeed");
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::APPL_LIMIT),
        lowered_limit,
        "SetApplLimit should lower ApplLimit before MaxApplZone"
    );
    assert_eq!(
        bus.read_long(app_zone),
        lowered_limit,
        "SetApplLimit should keep application-zone bkLim in sync"
    );
    assert_eq!(
        bus.read_long(app_zone + 12),
        lowered_free,
        "SetApplLimit should reduce zcbFree by the removed zone span"
    );

    let result = dispatcher.dispatch_memory(false, 0x63, &mut cpu, &mut bus);
    assert!(result.is_some(), "MaxApplZone should be handled");
    assert!(result.unwrap().is_ok(), "MaxApplZone should succeed");
    assert_eq!(
        bus.read_long(crate::memory::globals::addr::HEAP_END),
        lowered_limit,
        "MaxApplZone should expand HeapEnd to the lowered ApplLimit"
    );

    let result = dispatcher.dispatch_memory(false, 0x1C, &mut cpu, &mut bus);
    assert!(result.is_some(), "FreeMem should be handled");
    assert!(result.unwrap().is_ok(), "FreeMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        lowered_free,
        "FreeMem must not collapse to zero after SetApplLimit followed by MaxApplZone"
    );
}

#[test]
fn test_max_mem() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let result = dispatcher.dispatch_memory(false, 0x1D, &mut cpu, &mut bus);
    assert!(result.is_some(), "MaxMem should be handled");
    assert!(result.unwrap().is_ok(), "MaxMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        24 * 1024 * 1024,
        "MaxMem should return 24MB clamp floor in D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "MaxMem should return zero application-zone growth after MaxApplZone"
    );
}

#[test]
fn test_max_mem_returns_application_zone_growth_in_a0() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let zone = 0x180000;
    bus.write_long(crate::memory::globals::addr::APP_L_ZONE, zone);
    bus.write_long(zone, zone + 0x1000); // bkLim
    bus.write_long(crate::memory::globals::addr::HEAP_END, zone + 0x1000);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, zone + 0x3000);

    let result = dispatcher.dispatch_memory(false, 0x1D, &mut cpu, &mut bus);
    assert!(result.is_some(), "MaxMem should be handled");
    assert!(result.unwrap().is_ok(), "MaxMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::A0),
        0x2000,
        "MaxMem should return the application-zone growth allowance in A0"
    );
}

#[test]
fn max_mem_uses_heap_end_when_zone_header_covers_loaded_resources() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let zone = 0x180000;
    let heap_end = zone + 0x2000;
    let appl_limit = zone + 0x5000;
    bus.write_long(crate::memory::globals::addr::APP_L_ZONE, zone);
    bus.write_long(zone, appl_limit); // bkLim covers directly loaded resources
    bus.write_long(crate::memory::globals::addr::HEAP_END, heap_end);
    bus.write_long(crate::memory::globals::addr::APPL_LIMIT, appl_limit);

    let result = dispatcher.dispatch_memory(false, 0x1D, &mut cpu, &mut bus);
    assert!(result.is_some());
    assert!(result.unwrap().is_ok());
    assert_eq!(cpu.read_reg(Register::A0), appl_limit - heap_end);
}

#[test]
fn test_compact_mem() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 1024); // requested size
    let result = dispatcher.dispatch_memory(false, 0x4C, &mut cpu, &mut bus);
    assert!(result.is_some(), "CompactMem should be handled");
    assert!(result.unwrap().is_ok(), "CompactMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        24 * 1024 * 1024,
        "CompactMem should return 24MB clamp floor in D0"
    );
}

#[test]
fn test_set_handle_size() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    // First create a handle of size 128
    cpu.write_reg(Register::D0, 128);
    let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let handle = cpu.read_reg(Register::A0);
    let old_ptr = bus.read_long(handle);
    // Write some data into the block
    bus.write_long(old_ptr, 0xDEADBEEF);
    bus.write_long(old_ptr + 4, 0xCAFEBABE);
    // Resize to 2048
    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 2048);
    let result = dispatcher.dispatch_memory(false, 0x24, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetHandleSize should be handled");
    assert!(result.unwrap().is_ok(), "SetHandleSize should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetHandleSize should set D0 to 0 (noErr)"
    );
    // Handle should now point to new block with old data preserved
    let new_ptr = bus.read_long(handle);
    assert_eq!(
        bus.read_long(new_ptr),
        0xDEADBEEF,
        "Data should be preserved after resize"
    );
    assert_eq!(
        bus.read_long(new_ptr + 4),
        0xCAFEBABE,
        "Data should be preserved after resize"
    );
    // New block should be tracked with new size
    let new_size = bus.get_alloc_size(new_ptr).unwrap_or(0);
    assert_eq!(new_size, 2048, "New block should be 2048 bytes");
}

#[test]
fn set_handle_size_in_place_growth_updates_logical_size_before_later_move() {
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 1);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    let ptr = bus.read_long(handle);
    bus.write_byte(ptr, 0xAA);

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 3);
    dispatcher
        .dispatch_memory(false, 0x24, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0, "noErr expected");
    assert_eq!(
        bus.read_long(handle),
        ptr,
        "growth within the same bucket should keep the handle data pointer stable"
    );
    assert_eq!(
        bus.get_alloc_size(ptr),
        Some(3),
        "the logical handle size must be updated even when no move occurs"
    );

    bus.write_byte(ptr + 1, 0xBB);
    bus.write_byte(ptr + 2, 0xCC);

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 9);
    dispatcher
        .dispatch_memory(false, 0x24, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0, "noErr expected");
    let moved_ptr = bus.read_long(handle);
    assert_ne!(
        moved_ptr, ptr,
        "growth beyond the current bucket should move the relocatable block"
    );
    assert_eq!(
        bus.read_bytes(moved_ptr, 3),
        vec![0xAA, 0xBB, 0xCC],
        "later moves must copy the full logical size recorded by the in-place grow"
    );
    assert_eq!(bus.get_alloc_size(moved_ptr), Some(9));
}

#[test]
fn reallocate_handle_current_and_system_forms_replace_block_and_reset_state() {
    // Memory (1992), pp. 2-52--2-53: both entry points replace any
    // existing block with undefined contents and leave it unlocked and
    // unpurgeable. UI 3.4 MacMemory.h lines 1184--1202 supplies the exact
    // $A027/$A427 words; the emulator's flat heap makes their guest state
    // equivalent.
    for trap_word in [0xA027u16, 0xA427] {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        dispatcher.current_trap_word = trap_word;
        cpu.write_reg(Register::D0, 128);
        dispatcher
            .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        let handle = cpu.read_reg(Register::A0);
        let old_ptr = bus.read_long(handle);
        dispatcher.track_handle_ptr(old_ptr, handle);
        dispatcher.set_handle_state_bits(handle, 0xE0);

        cpu.write_reg(Register::A0, handle);
        cpu.write_reg(Register::D0, 17);
        let result = dispatcher.dispatch_memory(false, 0x27, &mut cpu, &mut bus);

        assert!(result.is_some() && result.unwrap().is_ok());
        assert_eq!(cpu.read_reg(Register::D0), 0);
        assert_eq!(cpu.read_reg(Register::A0), handle);
        let new_ptr = bus.read_long(handle);
        assert_ne!(new_ptr, old_ptr);
        assert_eq!(
            bus.get_alloc_size(old_ptr),
            None,
            "old block must be released"
        );
        assert_eq!(bus.get_alloc_size(new_ptr), Some(17));
        assert_eq!(bus.read_bytes(new_ptr, 17), vec![0xA5; 17]);
        assert_eq!(dispatcher.handle_for_ptr(old_ptr), None);
        assert_eq!(dispatcher.handle_for_ptr(new_ptr), Some(handle));
        assert_eq!(
            dispatcher.handle_state_bits(handle),
            Some(0x20),
            "lock/purge bits clear while the resource bit survives"
        );
    }
}

#[test]
fn empty_handle_preserves_locked_classic_block_then_releases_it_when_unlocked() {
    // Memory (1992), pp. 2-51--2-52: a locked block reports memPurErr
    // without changing the handle; an unlocked block is released while
    // the four-byte master pointer remains allocated and becomes NIL.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 24);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    let ptr = bus.read_long(handle);
    dispatcher.set_handle_state_bits(handle, 0xC0);

    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x2B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), (-112i32) as u32);
    assert_eq!(bus.read_word(addr::MEM_ERR), (-112i16) as u16);
    assert_eq!(bus.read_long(handle), ptr);
    assert_eq!(bus.get_alloc_size(ptr), Some(24));
    assert_eq!(dispatcher.handle_for_ptr(ptr), Some(handle));
    assert_eq!(dispatcher.handle_state_bits(handle), Some(0xC0));

    dispatcher.set_handle_state_bits(handle, 0x40);
    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x2B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(addr::MEM_ERR), 0);
    assert_eq!(bus.read_long(handle), 0);
    assert_eq!(bus.get_alloc_size(handle), Some(4));
    assert_eq!(bus.get_alloc_size(ptr), None);
    assert_eq!(dispatcher.handle_for_ptr(ptr), None);
    assert_eq!(dispatcher.handle_state_bits(handle), Some(0x40));

    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x2B, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), super::MEM_WZ_ERR);
}

#[test]
fn reallocate_handle_error_preserves_master_pointer_block_and_state() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    dispatcher.current_trap_word = 0xA027;
    cpu.write_reg(Register::D0, 32);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    let old_ptr = bus.read_long(handle);
    dispatcher.set_handle_state_bits(handle, 0xC0);

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, u32::MAX);
    dispatcher
        .dispatch_memory(false, 0x27, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), super::MEM_FULL_ERR);
    assert_eq!(bus.read_long(handle), old_ptr);
    assert_eq!(bus.get_alloc_size(old_ptr), Some(32));
    assert_eq!(dispatcher.handle_for_ptr(old_ptr), Some(handle));
    assert_eq!(dispatcher.handle_state_bits(handle), Some(0xC0));
}

#[test]
fn reallocate_handle_rejects_a_disposed_master_pointer_slot() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 8);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A0, handle);
    cpu.write_reg(Register::D0, 16);
    dispatcher
        .dispatch_memory(false, 0x27, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), super::MEM_WZ_ERR);
    assert_eq!(bus.get_alloc_size(handle), None);
}

#[test]
fn test_recover_handle() {
    // Per IM:V V-579, RecoverHandle searches the master pointer
    // table and returns the EXISTING handle that owns the given
    // ptr. Allocate a real handle, then verify RecoverHandle on
    // its data ptr returns that same handle.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 100);
    let _ = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus); // NewHandle
    let original_handle = cpu.read_reg(Register::A0);
    let data_ptr = bus.read_long(original_handle);

    cpu.write_reg(Register::A0, data_ptr);
    let result = dispatcher.dispatch_memory(false, 0x28, &mut cpu, &mut bus);
    assert!(result.is_some(), "RecoverHandle should be handled");
    assert!(result.unwrap().is_ok(), "RecoverHandle should succeed");
    assert_eq!(
        cpu.read_reg(Register::A0),
        original_handle,
        "RecoverHandle must return the existing handle, not a fresh copy"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "RecoverHandle should set D0 to 0"
    );
}

#[test]
fn test_dispose_then_recover_returns_stale_handle() {
    // On real Mac OS, DisposeHandle frees the master pointer slot but
    // does NOT zero it. RecoverHandle scans all slots (including freed
    // ones), so it still finds the stale data address and returns the
    // freed handle rather than nil. IM:V V-579.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 100);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let handle = cpu.read_reg(Register::A0);
    let data_ptr = bus.read_long(handle);
    cpu.write_reg(Register::A0, handle);
    dispatcher
        .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    cpu.write_reg(Register::A0, data_ptr);
    dispatcher
        .dispatch_memory(false, 0x28, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_ne!(
            cpu.read_reg(Register::A0),
            0,
            "RecoverHandle after DisposeHandle must return stale freed handle (not nil), per IM:V V-579 scan-all-slots behavior"
        );
}

#[test]
fn recover_handle_rejects_stale_mapping_after_master_pointer_reuse() {
    // A disposed slot remains discoverable only until the Memory Manager
    // reuses it. Once NewHandle overwrites the slot with a new data pointer,
    // a table scan can no longer recover the old pointer from that slot.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 100);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let old_handle = cpu.read_reg(Register::A0);
    let old_ptr = bus.read_long(old_handle);

    cpu.write_reg(Register::A0, old_handle);
    dispatcher
        .dispatch_memory(false, 0x23, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::D0, 200);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let new_handle = cpu.read_reg(Register::A0);
    assert_eq!(
        new_handle, old_handle,
        "the freed handle slot should be reused"
    );
    assert_ne!(bus.read_long(new_handle), old_ptr);

    cpu.write_reg(Register::A0, old_ptr);
    dispatcher
        .dispatch_memory(false, 0x28, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "RecoverHandle must not alias an old pointer to the handle now occupying its former slot"
    );
}

#[test]
fn test_get_ptr_size() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    // First allocate a pointer of size 512
    cpu.write_reg(Register::D0, 512);
    let result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let ptr = cpu.read_reg(Register::A0);
    // Now get its size
    cpu.write_reg(Register::A0, ptr);
    let result = dispatcher.dispatch_memory(false, 0x21, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetPtrSize should be handled");
    assert!(result.unwrap().is_ok(), "GetPtrSize should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        512,
        "GetPtrSize should return 512 in D0"
    );
}

// SetPtrSize must keep the pointer stable per IM:Memory 1992 p.2-44
// ("SetPtrSize doesn't move the pointer").
#[test]
fn test_set_ptr_size_shrink_preserves_pointer() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    // NewPtr(256)
    cpu.write_reg(Register::D0, 256);
    dispatcher
        .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let original_ptr = cpu.read_reg(Register::A0);
    assert_ne!(original_ptr, 0);
    // SetPtrSize(128)
    cpu.write_reg(Register::A0, original_ptr);
    cpu.write_reg(Register::D0, 128);
    dispatcher
        .dispatch_memory(false, 0x20, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i32, 0, "noErr expected");
    assert_eq!(
        bus.read_word(addr::MEM_ERR),
        0,
        "successful SetPtrSize must clear MemErr"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        original_ptr,
        "SetPtrSize must not move the pointer (IM:Memory 1992 p.2-44)"
    );
    // GetPtrSize should now read 128 at the ORIGINAL pointer.
    cpu.write_reg(Register::A0, original_ptr);
    dispatcher
        .dispatch_memory(false, 0x21, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        cpu.read_reg(Register::D0),
        128,
        "GetPtrSize on the original pointer must reflect the new logical size"
    );
}

#[test]
fn test_set_ptr_size_zero_allocation_can_grow_within_minimum_bucket() {
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 0);
    dispatcher
        .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let ptr = cpu.read_reg(Register::A0);
    assert_ne!(ptr, 0);

    cpu.write_reg(Register::A0, ptr);
    cpu.write_reg(Register::D0, 1);
    dispatcher
        .dispatch_memory(false, 0x20, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0, "noErr expected");
    assert_eq!(
        cpu.read_reg(Register::A0),
        ptr,
        "SetPtrSize within the minimum allocation bucket should not move the pointer"
    );
    assert_eq!(bus.get_alloc_size(ptr), Some(1));
}

// Attempting to grow a Ptr beyond its aligned capacity returns
// memFullErr rather than silently moving the pointer. This matches
// IM:Memory 1992's "SetPtrSize doesn't move the pointer" contract.
#[test]
fn test_set_ptr_size_grow_beyond_capacity_returns_memfullerr() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 128);
    dispatcher
        .dispatch_memory(false, 0x1E, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let ptr = cpu.read_reg(Register::A0);
    cpu.write_reg(Register::A0, ptr);
    cpu.write_reg(Register::D0, 4096); // way beyond aligned 128
    dispatcher
        .dispatch_memory(false, 0x20, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -108,
        "SetPtrSize beyond aligned capacity must return memFullErr (-108)"
    );
    assert_eq!(
        bus.read_word(addr::MEM_ERR) as i16,
        -108,
        "failed SetPtrSize must publish memFullErr through MemErr"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        ptr,
        "SetPtrSize on failure must leave A0 untouched"
    );
}

#[test]
fn test_ins_time() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000); // TMTask pointer
    let result = dispatcher.dispatch_memory(false, 0x58, &mut cpu, &mut bus);
    assert!(result.is_some(), "InsTime should be handled");
    assert!(result.unwrap().is_ok(), "InsTime should succeed (no-op)");
}

#[test]
fn ins_x_time_reprime_uses_the_previous_intended_expiry() {
    // Inside Macintosh: Processes (1994), pp. 3-8--3-9 and 3-19:
    // InsXTime selects the extended record. A nonzero tmWakeUp makes the
    // next PrimeTime delay relative to the preceding intended expiry,
    // not to callback latency; RmvTime/InsXTime may preserve that value.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = 0x200000;
    bus.write_long(0x016A, 100);
    bus.write_long(task_ptr + 6, 0x1234_5678);
    bus.write_long(task_ptr + 14, 0);

    dispatcher.current_trap_word = 0xA458;
    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x58, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(dispatcher.timer_tasks[0].extended);

    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 10);
    dispatcher
        .dispatch_memory(false, 0x5A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(dispatcher.timer_tasks[0].fire_at_subtick, 100_600_000);
    assert_ne!(bus.read_long(task_ptr + 14), 0);

    dispatcher
        .callback_scheduling
        .set_current_subtick(100_750_000);
    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x59, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert!(dispatcher.timer_tasks.is_empty());

    dispatcher.current_trap_word = 0xA458;
    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x58, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 10);
    dispatcher
        .dispatch_memory(false, 0x5A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(
        dispatcher.timer_tasks[0].fire_at_subtick, 101_200_000,
        "callback latency must not shift the extended task's frequency"
    );
    assert_eq!(
        dispatcher.callback_scheduling.extended_wakeup(task_ptr),
        Some(101_200_000)
    );

    dispatcher
        .callback_scheduling
        .set_current_subtick(102_000_000);
    dispatcher
        .timer_tasks
        .with_mut(|timer_tasks| timer_tasks[0].active = false);
    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 1);
    dispatcher
        .dispatch_memory(false, 0x5A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(
        dispatcher.timer_tasks[0].fire_at_subtick, 102_000_000,
        "an intended expiry in the past must receive an actual zero delay"
    );
    assert_eq!(
        dispatcher.callback_scheduling.extended_wakeup(task_ptr),
        Some(101_260_000),
        "the past intended expiry remains the drift-free base"
    );
}

#[test]
fn test_rmv_time() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x59, &mut cpu, &mut bus);
    assert!(result.is_some(), "RmvTime should be handled");
    assert!(result.unwrap().is_ok(), "RmvTime should succeed (no-op)");
}

#[test]
fn test_prime_time() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000); // TMTask pointer
    cpu.write_reg(Register::D0, 1000); // delay
    let result = dispatcher.dispatch_memory(false, 0x5A, &mut cpu, &mut bus);
    assert!(result.is_some(), "PrimeTime should be handled");
    assert!(result.unwrap().is_ok(), "PrimeTime should succeed (no-op)");
}

#[test]
fn test_prime_time_rounds_up_to_tick_boundary() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = 0x200000;

    bus.write_long(0x016A, 100);
    cpu.write_reg(Register::A0, task_ptr);
    let result = dispatcher.dispatch_memory(false, 0x58, &mut cpu, &mut bus);
    assert!(result.is_some(), "InsTime should be handled");
    assert!(result.unwrap().is_ok(), "InsTime should succeed");

    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, 33);
    let result = dispatcher.dispatch_memory(false, 0x5A, &mut cpu, &mut bus);
    assert!(result.is_some(), "PrimeTime should be handled");
    assert!(result.unwrap().is_ok(), "PrimeTime should succeed");

    let task = dispatcher
        .timer_tasks
        .iter()
        .find(|task| task.task_ptr == task_ptr)
        .expect("timer task should be queued");
    assert!(task.active, "PrimeTime should activate the task");
    assert_eq!(
        task.fire_at_tick, 102,
        "33 ms should round up to two 60 Hz ticks from TickCount 100"
    );
}

#[test]
fn test_prime_time_preserves_negative_microsecond_deadline() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = 0x200000;

    bus.write_long(0x016A, 100);
    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x58, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, (-3333_i32) as u32);
    dispatcher
        .dispatch_memory(false, 0x5A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let task = dispatcher
        .timer_tasks
        .iter()
        .find(|task| task.task_ptr == task_ptr)
        .expect("timer task should be queued");
    assert_eq!(task.fire_at_tick, 101);
    assert_eq!(
        task.fire_at_subtick, 100_199_980,
        "3,333 microseconds should remain below one 60 Hz guest tick"
    );
}

#[test]
fn rmv_time_returns_active_task_remaining_time_as_negative_microseconds() {
    // Processes 1994 pp. 3-14 and 3-21: revised Time Manager RmvTime
    // reports unused time in tmCount, using negated microseconds when it
    // fits. This is the standard elapsed-time measurement contract.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = 0x200000;
    bus.write_long(0x016A, 100);

    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x58, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    cpu.write_reg(Register::A0, task_ptr);
    cpu.write_reg(Register::D0, (-10_000_i32) as u32);
    dispatcher
        .dispatch_memory(false, 0x5A, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    dispatcher
        .callback_scheduling
        .set_current_subtick(100_150_000); // 2,500 us elapsed
    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x59, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(task_ptr + 10) as i32, -7_500);
    assert_eq!(bus.read_word(task_ptr + 4) & 0x8000, 0);
    assert_eq!(dispatcher.timer_task_count(), 0);
}

#[test]
fn rmv_time_returns_zero_for_inactive_task() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let task_ptr = 0x200000;
    bus.write_long(task_ptr + 10, 0xDEAD_BEEF);
    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x58, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    cpu.write_reg(Register::A0, task_ptr);
    dispatcher
        .dispatch_memory(false, 0x59, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_long(task_ptr + 10), 0);
}

#[test]
fn test_control_set_mode_uses_vdpginfo_pointer() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = 0x310000u32;

    dispatcher.screen_mode.0 = 0x01F80000;
    bus.write_word(pb + 26, 2); // csCode = cscSetMode
    bus.write_long(pb + 28, record);
    bus.write_word(record, 0x83);
    bus.write_long(record + 2, 0x12345678);
    bus.write_word(record + 6, 0);
    bus.write_long(record + 8, 0xDEADBEEF);

    cpu.write_reg(Register::A0, pb);
    let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
    assert!(result.is_some(), "PBControl should be handled");
    assert!(result.unwrap().is_ok(), "PBControl should succeed");

    assert_eq!(
        bus.read_word(record + 6),
        0,
        "SetMode should return page 0 through csParam's VDPgInfo pointer"
    );
    assert_eq!(
        bus.read_long(record + 8),
        dispatcher.screen_mode.0,
        "SetMode should return the framebuffer base through csParam's VDPgInfo pointer"
    );
    assert_eq!(bus.read_long(record + 2), 0x12345678);
}

#[test]
fn test_control_set_mode_keeps_parameter_block_inline_fields_unchanged() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let pointer_shaped_csparam = 0x310000u32;

    dispatcher.screen_mode.0 = 0x01F80000;
    bus.write_word(pb + 26, 2); // csCode = cscSetMode
    bus.write_long(pb + 28, pointer_shaped_csparam);
    bus.write_word(pb + 34, 0xFFFF);
    bus.write_long(pb + 36, 0xDEADBEEF);
    bus.write_word(pointer_shaped_csparam, 0x83);
    bus.write_word(pointer_shaped_csparam + 6, 0);
    bus.write_long(pointer_shaped_csparam + 8, 0xCAFEBABE);

    cpu.write_reg(Register::A0, pb);
    let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
    assert!(result.is_some(), "PBControl should be handled");
    assert!(result.unwrap().is_ok(), "PBControl should succeed");

    assert_eq!(
        bus.read_word(pointer_shaped_csparam + 6),
        0,
        "SetMode should write page zero through the VDPgInfo pointer"
    );
    assert_eq!(
        bus.read_long(pointer_shaped_csparam + 8),
        dispatcher.screen_mode.0,
        "SetMode should write the framebuffer base through the VDPgInfo pointer"
    );
    assert_eq!(
        bus.read_word(pb + 34),
        0xFFFF,
        "SetMode should not rewrite unrelated inline words"
    );
    assert_eq!(
        bus.read_long(pb + 36),
        0xDEADBEEF,
        "SetMode should not rewrite unrelated inline longwords"
    );
}

#[test]
fn test_control_set_mode_does_not_write_past_short_stack_frame() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let frame_a6 = pb + 32;

    dispatcher.screen_mode.0 = 0x01F80000;
    bus.write_word(pb + 26, 2); // csCode = cscSetMode
    let record = 0x310000u32;
    bus.write_long(pb + 28, record);
    bus.write_word(record, 0x83);
    bus.write_long(frame_a6 + 4, 0x00ABCDEF); // saved return address

    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::A7, pb);
    cpu.write_reg(Register::A6, frame_a6);
    let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
    assert!(result.is_some(), "PBControl should be handled");
    assert!(result.unwrap().is_ok(), "PBControl should succeed");

    assert_eq!(
        bus.read_long(frame_a6 + 4),
        0x00ABCDEF,
        "SetMode should not write csBaseAddr past a short stack parameter block"
    );
}

#[test]
fn control_set_entries_selects_linear_transfer_for_direct_driver_palette() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = bus.alloc(8);
    let table = bus.alloc(8);

    bus.write_word(table, 7);
    bus.write_word(table + 2, 0x4444);
    bus.write_word(table + 4, 0x8888);
    bus.write_word(table + 6, 0xCCCC);
    bus.write_long(record, table);
    bus.write_word(record + 4, (-1i16) as u16);
    bus.write_word(record + 6, 0);
    bus.write_word(pb + 26, 3);
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(dispatcher.device_clut[7], [0x4444, 0x8888, 0xCCCC]);
    let gamma = dispatcher.device_gamma();
    assert_eq!(gamma[0][0x44], 0x44);
    assert_eq!(gamma[1][0x88], 0x88);
    assert_eq!(gamma[2][0xCC], 0xCC);
    assert!(!dispatcher.display_gamma.is_explicit());
}

#[test]
fn control_set_entries_preserves_explicit_gamma() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = bus.alloc(8);
    let table = bus.alloc(8);
    dispatcher.display_gamma.install([[0x42; 256]; 3]);

    bus.write_word(table, 7);
    bus.write_word(table + 2, 0x4444);
    bus.write_word(table + 4, 0x8888);
    bus.write_word(table + 6, 0xCCCC);
    bus.write_long(record, table);
    bus.write_word(record + 4, (-1i16) as u16);
    bus.write_word(record + 6, 0);
    bus.write_word(pb + 26, 3);
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(dispatcher.device_gamma(), [[0x42; 256]; 3]);
    assert!(dispatcher.display_gamma.is_explicit());
}

#[test]
fn control_set_gamma_installs_one_channel_table_from_vdgamma_record() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = bus.alloc(4);
    let table = bus.alloc(12 + 2 + 16);

    bus.write_word(table, 0); // gVersion
    bus.write_word(table + 2, 0); // gType
    bus.write_word(table + 4, 2); // gFormulaSize
    bus.write_word(table + 6, 1); // gChanCnt
    bus.write_word(table + 8, 16); // gDataCnt
    bus.write_word(table + 10, 4); // gDataWidth
    bus.write_word(table + 12, 0xA5A5); // skipped formula bytes
    for index in 0..16u32 {
        bus.write_byte(table + 14 + index, (index * 17) as u8);
    }
    bus.write_long(record, table);
    bus.write_word(pb + 26, 4); // cscSetGamma
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(bus.read_word(pb + 16), 0);
    for channel in dispatcher.device_gamma().iter() {
        assert_eq!(channel[0x00], 0x00);
        assert_eq!(channel[0x1F], 0x11);
        assert_eq!(channel[0xAB], 0xAA);
        assert_eq!(channel[0xFF], 0xFF);
    }
}

#[test]
fn control_set_gamma_keeps_three_channels_distinct() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = bus.alloc(4);
    let table = bus.alloc(12 + 3 * 256);

    bus.write_word(table, 0);
    bus.write_word(table + 2, 0);
    bus.write_word(table + 4, 0);
    bus.write_word(table + 6, 3);
    bus.write_word(table + 8, 256);
    bus.write_word(table + 10, 8);
    for index in 0..256u32 {
        bus.write_byte(table + 12 + index, index as u8);
        bus.write_byte(table + 12 + 256 + index, 255 - index as u8);
        bus.write_byte(table + 12 + 512 + index, 0x42);
    }
    bus.write_long(record, table);
    bus.write_word(pb + 26, 4);
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    let gamma = dispatcher.device_gamma();
    assert_eq!(gamma[0][0xA5], 0xA5);
    assert_eq!(gamma[1][0xA5], 0x5A);
    assert_eq!(gamma[2][0xA5], 0x42);
    dispatcher.device_clut.set_entry(7, [0xA5A5; 3]);
    let raw_clut = *dispatcher.device_clut;
    let palette =
        crate::display::argb_palette_from_clut_with_gamma(&dispatcher.device_clut, &gamma);
    assert_eq!(palette[7], 0xFFA55A42);
    assert_eq!(dispatcher.device_clut, raw_clut);
}

#[test]
fn control_set_gamma_rejects_malformed_table_without_changing_state() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = bus.alloc(4);
    let table = bus.alloc(12 + 16);
    let before = dispatcher.device_gamma();

    bus.write_word(table, 0);
    bus.write_word(table + 2, 0);
    bus.write_word(table + 4, 0);
    bus.write_word(table + 6, 2); // unsupported channel count
    bus.write_word(table + 8, 16);
    bus.write_word(table + 10, 4);
    bus.write_long(record, table);
    bus.write_word(pb + 26, 4);
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-50i32) as u32);
    assert_eq!(bus.read_word(pb + 16), (-50i16) as u16);
    assert_eq!(dispatcher.device_gamma(), before);
}

#[test]
fn control_set_gamma_rejects_truncated_allocated_table() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = bus.alloc(4);
    let table = bus.alloc(12 + 8);
    let before = dispatcher.device_gamma();

    bus.write_word(table, 0);
    bus.write_word(table + 2, 0);
    bus.write_word(table + 4, 0);
    bus.write_word(table + 6, 1);
    bus.write_word(table + 8, 16);
    bus.write_word(table + 10, 4);
    bus.write_long(record, table);
    bus.write_word(pb + 26, 4);
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), (-50i32) as u32);
    assert_eq!(dispatcher.device_gamma(), before);
}

#[test]
fn control_set_gamma_null_table_restores_linear_ramp() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = bus.alloc(4);
    dispatcher.display_gamma.set_implicit([[0x42; 256]; 3]);

    bus.write_long(record, 0);
    bus.write_word(pb + 26, 4);
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(cpu.read_reg(Register::D0), 0);
    for channel in dispatcher.device_gamma().iter() {
        assert_eq!(channel[0x00], 0x00);
        assert_eq!(channel[0x7F], 0x7F);
        assert_eq!(channel[0xFF], 0xFF);
    }
}

#[test]
fn test_microseconds() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    // setup() already sets TickCount at $016A to 100
    let ticks = 100u64;
    // 1 tick = 16,625 µs (1/60.15 Hz; Guide to Macintosh Family Hardware 2nd Ed., p. 6-798)
    let expected_usecs = ticks * 16_625;
    let result = dispatcher.dispatch_memory(false, 0x93, &mut cpu, &mut bus);
    assert!(result.is_some(), "Microseconds should be handled");
    assert!(result.unwrap().is_ok(), "Microseconds should succeed");
    // D0 = low 32 bits, A0 = high 32 bits (executor emustubs.cpp convention,
    // retained for Executor-style register-reader callers)
    let d0 = cpu.read_reg(Register::D0) as u64;
    let a0 = cpu.read_reg(Register::A0) as u64;
    let actual_usecs = (a0 << 32) | d0;
    assert_eq!(
        actual_usecs, expected_usecs,
        "Microseconds should return ticks*16625 = {} in D0(lo):A0(hi), got {}",
        expected_usecs, actual_usecs
    );
}

#[test]
fn microseconds_does_not_speculatively_write_through_a0_on_entry() {
    // The MPW Universal Headers `Timer.h` declares Microseconds as
    // FOURWORDINLINE($A193, $225F, $22C8, $2280). The trap itself
    // returns the count in registers (D0 = lo, A0 = hi); the
    // caller-side inline glue ($225F MOVEA.L (A7)+, A1; $22C8
    // MOVE.L A0, (A1)+; $2280 MOVE.L D0, (A1)) then writes the
    // 64-bit result through the caller's UnsignedWide pointer.
    //
    // Under that FOURWORDINLINE pattern, register A0 on entry to
    // the trap is uninitialised scratch — the buffer pointer is
    // still on the stack and won't be moved into A1 until *after*
    // the trap returns. The HLE must NOT speculatively write
    // through whatever value A0 happens to hold, because that
    // would corrupt unrelated guest memory in the common case.
    // Verify the HLE leaves the byte under A0 alone.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let unrelated = 0x322000u32;
    // Pre-fill the "unrelated" memory at the address that A0
    // happens to point to; a buggy HLE that speculatively writes
    // through A0 would clobber this sentinel.
    bus.write_long(unrelated, 0x11223344u32);
    bus.write_long(unrelated + 4, 0x55667788u32);
    cpu.write_reg(Register::A0, unrelated);

    let result = dispatcher.dispatch_memory(false, 0x93, &mut cpu, &mut bus);
    assert!(result.is_some(), "Microseconds should be handled");
    assert!(result.unwrap().is_ok(), "Microseconds should succeed");

    assert_eq!(
        bus.read_long(unrelated),
        0x11223344u32,
        "Microseconds must not write through whatever value A0 held on entry"
    );
    assert_eq!(
        bus.read_long(unrelated + 4),
        0x55667788u32,
        "Microseconds must not write past whatever value A0 held on entry"
    );
}

#[test]
fn readxpram_zero_fills_requested_count_and_returns_noerr() {
    // Inside Macintosh Volume V (1986), p. V-519: ReadXPRam uses D0
    // high-word count and A0 destination pointer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let dest = 0x322000u32;
    bus.write_bytes(dest, &[0xAA, 0xBB, 0xCC, 0xDD, 0xEE]);
    cpu.write_reg(Register::D0, (4u32 << 16) | 0x0012u32);
    cpu.write_reg(Register::A0, dest);

    let result = dispatcher.dispatch_memory(false, 0x51, &mut cpu, &mut bus);
    assert!(result.is_some(), "ReadXPRam should be handled");
    assert!(result.unwrap().is_ok(), "ReadXPRam should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "ReadXPRam should return noErr"
    );
    assert_eq!(bus.read_bytes(dest, 4), vec![0, 0, 0, 0]);
    assert_eq!(
        bus.read_byte(dest + 4),
        0xEE,
        "ReadXPRam should zero exactly count bytes"
    );
}

#[test]
fn readxpram_uses_d0_count_offset_and_a0_destptr_register_calling_convention() {
    // Inside Macintosh Volume V (1986), p. V-519: ReadXPRam register
    // calling convention does not consume stack arguments.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let dest = 0x322040u32;
    bus.write_bytes(dest, &[0x11, 0x22, 0x33, 0x44]);
    cpu.write_reg(Register::D0, (3u32 << 16) | 0x00FFu32);
    cpu.write_reg(Register::A0, dest);

    let result = dispatcher.dispatch_memory(false, 0x51, &mut cpu, &mut bus);
    assert!(result.is_some(), "ReadXPRam should be handled");
    assert!(result.unwrap().is_ok(), "ReadXPRam should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "ReadXPRam should preserve A7 in register-calling convention"
    );
    assert_eq!(bus.read_bytes(dest, 3), vec![0, 0, 0]);
    assert_eq!(bus.read_byte(dest + 3), 0x44);
}

#[test]
fn readxpram_five_call_composition_preserves_stack_across_varying_count_offset() {
    // 5 successive _ReadXPRam dispatches with varying D0 packed
    // (count << 16) | offset values must each preserve A7. Per
    // IM:V V-519 the register-only ABI takes A0 + D0 inputs and
    // returns OSErr in D0; no Pascal stack frame is consumed.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let dest = 0x322200u32;
    let sp_before = cpu.read_reg(Register::A7);

    let cases: [(u32, u32); 5] = [
        ((1u32 << 16) | 0x0014, dest),     // count=1 offset=20 (start of XPRAM)
        ((4u32 << 16) | 0x0080, dest + 8), // count=4 offset=128
        ((8u32 << 16) | 0x00FF, dest + 16), // count=8 offset=255 (last byte)
        (2u32 << 16, dest + 32),           // count=2 offset=0 (SysParam start)
        ((16u32 << 16) | 0x004B, dest + 48), // count=16 offset=75
    ];

    for (d0, a0) in cases.iter() {
        cpu.write_reg(Register::D0, *d0);
        cpu.write_reg(Register::A0, *a0);
        let result = dispatcher.dispatch_memory(false, 0x51, &mut cpu, &mut bus);
        assert!(result.is_some(), "ReadXPRam should be handled");
        assert!(result.unwrap().is_ok(), "ReadXPRam should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "ReadXPRam should return noErr regardless of count/offset"
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "5 successive ReadXPRam calls must preserve A7 in aggregate"
    );
}

#[test]
fn writexpram_noop_returns_noerr_in_hle_without_persistent_xpram() {
    // Inside Macintosh Volume V (1986), p. V-519: WriteXPRam uses D0
    // count/offset + A0 source pointer. HLE returns noErr without
    // persistent XPRAM storage.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x322080u32;
    bus.write_bytes(src, &[0x9A, 0xBC, 0xDE, 0xF0]);
    cpu.write_reg(Register::D0, (4u32 << 16) | 0x0042u32);
    cpu.write_reg(Register::A0, src);

    let result = dispatcher.dispatch_memory(false, 0x52, &mut cpu, &mut bus);
    assert!(result.is_some(), "WriteXPRam should be handled");
    assert!(result.unwrap().is_ok(), "WriteXPRam should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "WriteXPRam should return noErr"
    );
    assert_eq!(
        bus.read_bytes(src, 4),
        vec![0x9A, 0xBC, 0xDE, 0xF0],
        "WriteXPRam should not mutate caller source bytes"
    );
}

#[test]
fn writexpram_uses_d0_count_offset_and_a0_srcptr_register_calling_convention() {
    // Inside Macintosh Volume V (1986), p. V-519: WriteXPRam register
    // convention should preserve A7.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let src = 0x3220C0u32;
    bus.write_bytes(src, &[0x01, 0x23, 0x45]);
    cpu.write_reg(Register::D0, (3u32 << 16) | 0x0001u32);
    cpu.write_reg(Register::A0, src);

    let result = dispatcher.dispatch_memory(false, 0x52, &mut cpu, &mut bus);
    assert!(result.is_some(), "WriteXPRam should be handled");
    assert!(result.unwrap().is_ok(), "WriteXPRam should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "WriteXPRam should not consume stack arguments"
    );
}

#[test]
fn writexpram_five_call_composition_preserves_stack_and_source_across_varying_count_offset() {
    // 5 successive _WriteXPRam dispatches with varying D0 packed
    // (count << 16) | offset values must each preserve A7 AND
    // leave the caller's source bytes untouched. Per IM:V V-519
    // the register-only ABI treats A0 as a READ-ONLY source
    // pointer (the trap propagates the bytes OUT to the clock
    // chip) — the low-memory source buffer is the authoritative
    // input, never overwritten by the trap.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x322300u32;
    let sentinel: [u8; 16] = [
        0x4B, 0x58, 0x44, 0x50, 0x01, 0x02, 0x03, 0x04, 0xAA, 0xBB, 0xCC, 0xDD, 0x10, 0x20, 0x30,
        0x40,
    ];
    bus.write_bytes(src, &sentinel);
    let sp_before = cpu.read_reg(Register::A7);

    let cases: [u32; 5] = [
        (1u32 << 16) | 0x0014,  // count=1 offset=20
        (4u32 << 16) | 0x0080,  // count=4 offset=128
        (8u32 << 16) | 0x00FF,  // count=8 offset=255
        2u32 << 16,             // count=2 offset=0
        (16u32 << 16) | 0x004B, // count=16 offset=75
    ];

    for d0 in cases.iter() {
        cpu.write_reg(Register::D0, *d0);
        cpu.write_reg(Register::A0, src);
        let result = dispatcher.dispatch_memory(false, 0x52, &mut cpu, &mut bus);
        assert!(result.is_some(), "WriteXPRam should be handled");
        assert!(result.unwrap().is_ok(), "WriteXPRam should return");
        assert_eq!(
            cpu.read_reg(Register::D0),
            0,
            "WriteXPRam should return noErr regardless of count/offset"
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "5 successive WriteXPRam calls must preserve A7 in aggregate"
    );
    assert_eq!(
        bus.read_bytes(src, 16),
        sentinel.to_vec(),
        "5 successive WriteXPRam calls must leave the caller's source buffer untouched"
    );
}

#[test]
fn getdefaultstartup_fills_4_byte_defstartrec_through_a0() {
    // Inside Macintosh Volume V (1986), p. V-529: GetDefaultStartup
    // writes a DefStartRec through A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let ptr = 0x322100u32;
    bus.write_long(ptr, 0xDEAD_BEEF);
    cpu.write_reg(Register::A0, ptr);

    let result = dispatcher.dispatch_memory(false, 0x7D, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetDefaultStartup should be handled");
    assert!(result.unwrap().is_ok(), "GetDefaultStartup should return");
    assert_eq!(
        bus.read_long(ptr),
        0,
        "GetDefaultStartup should return the initial in-session DefStartRec in HLE"
    );
}

#[test]
fn getdefaultstartup_uses_a0_parameter_block_without_stack_arguments() {
    // Inside Macintosh Volume V (1986), p. V-529: GetDefaultStartup
    // takes A0 parameter block input.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0);

    let result = dispatcher.dispatch_memory(false, 0x7D, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetDefaultStartup should be handled");
    assert!(result.unwrap().is_ok(), "GetDefaultStartup should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "GetDefaultStartup should preserve A7"
    );
}

#[test]
fn setdefaultstartup_updates_getdefaultstartup_and_preserves_stack_pointer() {
    // Inside Macintosh Volume V (1986), p. V-529: SetDefaultStartup
    // consumes a DefStartRec from A0 and GetDefaultStartup should
    // return the same in-session bytes afterward.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let set_ptr = 0x322140u32;
    let get_ptr = 0x322180u32;
    bus.write_long(set_ptr, 0xA1B2_C3D4);
    bus.write_long(get_ptr, 0xDEAD_BEEF);
    cpu.write_reg(Register::A0, set_ptr);
    cpu.write_reg(Register::D0, 0x1357_9BDF);

    let result = dispatcher.dispatch_memory(false, 0x7E, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetDefaultStartup should be handled");
    assert!(result.unwrap().is_ok(), "SetDefaultStartup should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SetDefaultStartup should preserve A7"
    );
    assert_eq!(
        bus.read_long(set_ptr),
        0xA1B2_C3D4,
        "SetDefaultStartup should not mutate the caller DefStartRec"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0x1357_9BDF,
        "SetDefaultStartup should not clobber D0 in the PROCEDURE path"
    );

    cpu.write_reg(Register::A0, get_ptr);
    let result = dispatcher.dispatch_memory(false, 0x7D, &mut cpu, &mut bus);
    assert!(
        result.is_some(),
        "GetDefaultStartup should be handled after SetDefaultStartup"
    );
    assert!(
        result.unwrap().is_ok(),
        "GetDefaultStartup should return after SetDefaultStartup"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "GetDefaultStartup should preserve A7"
    );
    assert_eq!(
        bus.read_long(get_ptr),
        0xA1B2_C3D4,
        "GetDefaultStartup should return the record stored by SetDefaultStartup"
    );
}

#[test]
fn test_hnopurge() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x4A, &mut cpu, &mut bus);
    assert!(result.is_some(), "HNoPurge ($A04A) should be handled");
    assert!(result.unwrap().is_ok(), "HNoPurge should succeed (no-op)");
}

#[test]
fn test_hget_state() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    let result = dispatcher.dispatch_memory(false, 0x69, &mut cpu, &mut bus);
    assert!(result.is_some(), "HGetState should be handled");
    assert!(result.unwrap().is_ok(), "HGetState should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HGetState should return 0 in D0"
    );
}

#[test]
fn test_hset_state() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0x200000);
    cpu.write_reg(Register::D0, 0x40); // some state byte
    let result = dispatcher.dispatch_memory(false, 0x6A, &mut cpu, &mut bus);
    assert!(result.is_some(), "HSetState should be handled");
    assert!(result.unwrap().is_ok(), "HSetState should succeed (no-op)");
}

#[test]
fn test_memorydispatch_unlockmemory_returns_notlockederr_for_unlocked_range() {
    // Inside Macintosh: Memory (1992), 3-30: UnlockMemory returns
    // notLockedErr (-623) when the specified range is not locked.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 3); // UnlockMemory selector
    cpu.write_reg(Register::A0, 0x0020_0123);
    cpu.write_reg(Register::A1, 0x180);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(result.is_some(), "MemoryDispatch should handle selector 3");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 3 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-623i32) as u32,
        "UnlockMemory should return notLockedErr (-623) for unlocked pages"
    );
}

#[test]
fn test_memorydispatch_unholdmemory_returns_nothelderr_for_never_held_range() {
    // Inside Macintosh: Memory (1992), 3-25 to 3-27: UnholdMemory
    // returns notHeldErr (-621) when the requested pages were never held.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 1); // UnholdMemory selector
    cpu.write_reg(Register::A0, 0x0020_0456);
    cpu.write_reg(Register::A1, 0x200);
    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 1");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 1 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-621i32) as u32,
        "UnholdMemory should return notHeldErr (-621) for never-held pages"
    );
    assert_eq!(
        sp_pre, sp_post,
        "UnholdMemory should preserve the caller's stack pointer for never-held pages"
    );
}

#[test]
fn test_memorydispatch_holdmemory_round_trip_releases_idempotently() {
    // Inside Macintosh: Memory (1992), 3-25 to 3-27: HoldMemory
    // rounds the range to whole pages, and UnholdMemory remains
    // noErr on a range that has already been held and released.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let base = bus.alloc(0x3000);
    let hold_start = base + 0x11;
    let hold_count = 0x180u32;

    cpu.write_reg(Register::D0, 0); // HoldMemory selector
    cpu.write_reg(Register::A0, hold_start);
    cpu.write_reg(Register::A1, hold_count);
    let sp_pre = cpu.read_reg(Register::A7);
    let hold_once = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        hold_once.is_some(),
        "MemoryDispatch should handle selector 0"
    );
    assert!(hold_once.unwrap().is_ok(), "HoldMemory should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HoldMemory should return noErr on a live logical-RAM range"
    );

    cpu.write_reg(Register::D0, 0); // HoldMemory selector again
    cpu.write_reg(Register::A0, hold_start);
    cpu.write_reg(Register::A1, hold_count);
    let hold_twice = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        hold_twice.is_some(),
        "MemoryDispatch should handle selector 0"
    );
    assert!(
        hold_twice.unwrap().is_ok(),
        "HoldMemory should return on repeat"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HoldMemory should remain idempotent on the same page span"
    );

    cpu.write_reg(Register::D0, 1); // UnholdMemory selector
    cpu.write_reg(Register::A0, hold_start);
    cpu.write_reg(Register::A1, hold_count);
    let unhold_once = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        unhold_once.is_some(),
        "MemoryDispatch should handle selector 1"
    );
    assert!(unhold_once.unwrap().is_ok(), "UnholdMemory should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "UnholdMemory should return noErr after releasing one hold"
    );

    cpu.write_reg(Register::D0, 1); // UnholdMemory selector again
    cpu.write_reg(Register::A0, hold_start);
    cpu.write_reg(Register::A1, hold_count);
    let unhold_twice = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        unhold_twice.is_some(),
        "MemoryDispatch should handle selector 1"
    );
    assert!(
        unhold_twice.unwrap().is_ok(),
        "UnholdMemory should return on second release"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "UnholdMemory should return noErr while the second hold is still tracked"
    );

    cpu.write_reg(Register::D0, 1); // UnholdMemory selector a third time
    cpu.write_reg(Register::A0, hold_start);
    cpu.write_reg(Register::A1, hold_count);
    let unhold_third = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        unhold_third.is_some(),
        "MemoryDispatch should handle selector 1"
    );
    assert!(unhold_third.unwrap().is_ok(), "UnholdMemory should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "UnholdMemory should remain noErr after the last tracked hold is released"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "HoldMemory/UnholdMemory should preserve the caller's stack pointer"
    );
}

#[test]
fn test_memorydispatch_holdmemory_invalid_range_returns_noerr_and_preserves_stack() {
    // Inside Macintosh: Memory (1992), 3-26: HoldMemory returns
    // noErr on the BasiliskII-observed invalid-range path.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let invalid_start = bus.ram_size() + 0x1000;
    cpu.write_reg(Register::D0, 0); // HoldMemory selector
    cpu.write_reg(Register::A0, invalid_start);
    cpu.write_reg(Register::A1, 0x200);
    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 0");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 0 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HoldMemory should return noErr for BasiliskII invalid-range behavior"
    );
    assert_eq!(
        sp_pre, sp_post,
        "HoldMemory should preserve the caller's stack pointer for invalid ranges"
    );
}

#[test]
fn test_memorydispatch_unholdmemory_invalid_range_returns_noerr_and_preserves_stack() {
    // Inside Macintosh: Memory (1992), 3-27: UnholdMemory returns
    // noErr on the BasiliskII-observed invalid-range path.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let invalid_start = bus.ram_size() + 0x0800;
    cpu.write_reg(Register::D0, 1); // UnholdMemory selector
    cpu.write_reg(Register::A0, invalid_start);
    cpu.write_reg(Register::A1, 0x300);
    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 1");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 1 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "UnholdMemory should return noErr for BasiliskII invalid-range behavior"
    );
    assert_eq!(
        sp_pre, sp_post,
        "UnholdMemory should preserve the caller's stack pointer for invalid ranges"
    );
}

#[test]
fn test_memorydispatch_lockmemory_invalid_range_returns_paramerr() {
    // Inside Macintosh: Memory (1992), 3-28 and 3-29: LockMemory and
    // LockMemoryContiguous return paramErr (-50) for invalid ranges.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let invalid_start = bus.ram_size() + 0x0400;
    cpu.write_reg(Register::D0, 2); // LockMemory selector
    cpu.write_reg(Register::A0, invalid_start);
    cpu.write_reg(Register::A1, 0x500);
    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 2");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 2 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "LockMemory should return paramErr (-50) for non-RAM ranges"
    );
    assert_eq!(
        sp_pre, sp_post,
        "LockMemory should preserve the caller's stack pointer for invalid ranges"
    );
}

#[test]
fn test_memorydispatch_lockmemorycontiguous_invalid_range_returns_paramerr() {
    // Inside Macintosh: Memory (1992), 3-29: LockMemoryContiguous returns
    // paramErr (-50) for an invalid parameter list.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let invalid_start = bus.ram_size() + 0x1400;
    cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
    cpu.write_reg(Register::A0, invalid_start);
    cpu.write_reg(Register::A1, 0x280);
    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 4");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 4 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "LockMemoryContiguous should return paramErr (-50) for non-RAM ranges"
    );
    assert_eq!(
        sp_pre, sp_post,
        "LockMemoryContiguous should preserve the caller's stack pointer for invalid ranges"
    );
}

#[test]
fn test_memorydispatch_lockmemorycontiguous_zero_length_invalid_range_returns_paramerr() {
    // Inside Macintosh: Memory (1992), 3-29: LockMemoryContiguous returns
    // paramErr (-50) for an invalid parameter list. Zero-length ranges
    // still need a plausible logical-RAM start address.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let invalid_start = bus.ram_size() + 0x2000;
    cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
    cpu.write_reg(Register::A0, invalid_start);
    cpu.write_reg(Register::A1, 0);
    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(result.is_some(), "MemoryDispatch should handle selector 4");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 4 should return"
    );
    assert_eq!(
            cpu.read_reg(Register::D0),
            (-50i32) as u32,
            "LockMemoryContiguous should return paramErr (-50) for a zero-length range outside logical RAM"
        );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "LockMemoryContiguous should not consume a Pascal stack frame"
    );
}

#[test]
fn test_memorydispatch_unlockmemory_invalid_range_returns_paramerr() {
    // Inside Macintosh: Memory (1992), 3-30: UnlockMemory returns
    // paramErr (-50) for an invalid parameter list.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let invalid_start = bus.ram_size() + 0x2000;
    cpu.write_reg(Register::D0, 3); // UnlockMemory selector
    cpu.write_reg(Register::A0, invalid_start);
    cpu.write_reg(Register::A1, 0x180);
    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 3");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 3 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "UnlockMemory should return paramErr (-50) for non-RAM ranges"
    );
    assert_eq!(
        sp_pre, sp_post,
        "UnlockMemory should preserve the caller's stack pointer for invalid ranges"
    );
}

#[test]
fn test_memorydispatch_unlockmemory_reverses_lockmemorycontiguous_on_page_rounded_range() {
    // Inside Macintosh: Memory (1992), 3-29 to 3-30: LockMemoryContiguous
    // and UnlockMemory round ranges to page boundaries, and UnlockMemory
    // undoes both LockMemory and LockMemoryContiguous.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let block_start = bus.alloc(0x3000);
    let rounded_page = (block_start + 0x0FFF) & !0x0FFF;
    let lock_start = rounded_page + 0x10;
    let lock_count = 0x20u32;
    let unlock_start = rounded_page + 0x80;
    let unlock_count = 0x30u32;

    cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
    cpu.write_reg(Register::A0, lock_start);
    cpu.write_reg(Register::A1, lock_count);
    let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        lock_result.is_some(),
        "MemoryDispatch should handle selector 4"
    );
    assert!(
        lock_result.unwrap().is_ok(),
        "LockMemoryContiguous should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "LockMemoryContiguous should return noErr"
    );

    cpu.write_reg(Register::D0, 3); // UnlockMemory selector
    cpu.write_reg(Register::A0, unlock_start);
    cpu.write_reg(Register::A1, unlock_count);
    let unlock_once = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        unlock_once.is_some(),
        "MemoryDispatch should handle selector 3"
    );
    assert!(unlock_once.unwrap().is_ok(), "UnlockMemory should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "UnlockMemory should return noErr when undoing LockMemoryContiguous for the rounded page"
    );

    cpu.write_reg(Register::D0, 3); // UnlockMemory selector again
    cpu.write_reg(Register::A0, unlock_start);
    cpu.write_reg(Register::A1, unlock_count);
    let unlock_twice = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        unlock_twice.is_some(),
        "MemoryDispatch should handle selector 3"
    );
    assert!(
        unlock_twice.unwrap().is_ok(),
        "UnlockMemory should return on second attempt"
    );
    assert_eq!(
            cpu.read_reg(Register::D0),
            (-623i32) as u32,
            "UnlockMemory should return notLockedErr (-623) after the prior unlock released the rounded page"
        );
}

#[test]
fn test_memorydispatch_getphysical_requires_locked_range() {
    // Inside Macintosh: Memory (1992), 3-32: GetPhysical requires the
    // logical range to be locked and returns notLockedErr (-623) if not.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let table = bus.alloc(16);
    let logical_start = bus.alloc(512);
    let logical_count = 128u32;
    bus.write_long(table, logical_start);
    bus.write_long(table + 4, logical_count);
    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, table);
    cpu.write_reg(Register::A1, 1); // request one entry
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(result.is_some(), "MemoryDispatch should handle selector 5");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 5 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-623i32) as u32,
        "GetPhysical should return notLockedErr (-623) for unlocked ranges"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "GetPhysical should return translated-entry count 0 on lock failure"
    );
}

#[test]
fn test_memorydispatch_getphysical_invalid_logical_range_returns_paramerr() {
    // Inside Macintosh: Memory (1992), 3-32: GetPhysical returns
    // paramErr (-50) when asked to translate non-logical-RAM addresses.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let table = bus.alloc(16);
    let logical_start = bus.ram_size() + 0x4000;
    let logical_count = 256u32;
    bus.write_long(table, logical_start);
    bus.write_long(table + 4, logical_count);
    bus.write_long(table + 8, 0x1111_1111);
    bus.write_long(table + 12, 0x2222_2222);

    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, table);
    cpu.write_reg(Register::A1, 1);
    let sp_before = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_after = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 5");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 5 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "GetPhysical should return paramErr (-50) for non-RAM logical ranges"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "GetPhysical should return translated-entry count 0 on paramErr"
    );
    assert_eq!(
        sp_before, sp_after,
        "GetPhysical should preserve the caller's stack pointer on paramErr"
    );
    assert_eq!(
        bus.read_long(table + 8),
        0x1111_1111,
        "GetPhysical should preserve translation entries on paramErr"
    );
    assert_eq!(
        bus.read_long(table + 12),
        0x2222_2222,
        "GetPhysical should preserve translation entries on paramErr"
    );
}

#[test]
fn test_memorydispatch_getphysical_null_table_returns_paramerr() {
    // Inside Macintosh: Memory (1992), 3-32: GetPhysical returns paramErr
    // for an invalid parameter list (NIL translation-table pointer).
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::A1, 1);
    let sp_before = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_after = cpu.read_reg(Register::A7);
    assert!(result.is_some(), "MemoryDispatch should handle selector 5");
    assert!(
        result.unwrap().is_ok(),
        "MemoryDispatch selector 5 should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "GetPhysical should return paramErr (-50) for NIL translation table"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "GetPhysical should return translated-entry count 0 for NIL translation table"
    );
    assert_eq!(
        sp_before, sp_after,
        "GetPhysical should preserve the caller's stack pointer for NIL translation table"
    );
}

#[test]
fn test_memorydispatch_getphysical_fills_identity_mapping_when_locked() {
    // Inside Macintosh: Memory (1992), 3-31 to 3-32: on success,
    // GetPhysical writes translation entries and returns entry count.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let table = bus.alloc(16);
    let logical_start = bus.alloc(1024) + 37; // force non-page-aligned start
    let logical_count = 700u32;
    bus.write_long(table, logical_start);
    bus.write_long(table + 4, logical_count);

    cpu.write_reg(Register::D0, 2); // LockMemory selector
    cpu.write_reg(Register::A0, logical_start);
    cpu.write_reg(Register::A1, logical_count);
    let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        lock_result.is_some(),
        "MemoryDispatch should handle selector 2"
    );
    assert!(lock_result.unwrap().is_ok(), "LockMemory should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "LockMemory should return noErr before GetPhysical"
    );

    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, table);
    cpu.write_reg(Register::A1, 1);
    let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        get_result.is_some(),
        "MemoryDispatch should handle selector 5"
    );
    assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "GetPhysical should return noErr on locked range"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        1,
        "GetPhysical should report one translated physical entry"
    );
    assert_eq!(
        bus.read_long(table + 8),
        logical_start,
        "GetPhysical should emit identity physical start in entry[0]"
    );
    assert_eq!(
        bus.read_long(table + 12),
        logical_count,
        "GetPhysical should emit identity physical count in entry[0]"
    );
    assert_eq!(
            bus.read_long(table),
            logical_start,
            "GetPhysical should leave the logical start unchanged when the range fits in one physical entry"
        );
    assert_eq!(
            bus.read_long(table + 4),
            logical_count,
            "GetPhysical should leave the logical count unchanged when the range fits in one physical entry"
        );
}

#[test]
fn test_memorydispatch_getphysical_entrycount_zero_returns_paramerr_and_required_entries() {
    // BasiliskII returns paramErr (-50) and the required translated-entry
    // count for a locked non-empty range queried with physicalEntryCount=0,
    // while preserving the caller's stack pointer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let table = bus.alloc(16);
    let logical_start = bus.alloc(4096) + 99;
    let logical_count = 1536u32;
    bus.write_long(table, logical_start);
    bus.write_long(table + 4, logical_count);
    bus.write_long(table + 8, 0x1111_2222);
    bus.write_long(table + 12, 0x3333_4444);

    cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
    cpu.write_reg(Register::A0, logical_start);
    cpu.write_reg(Register::A1, logical_count);
    let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        lock_result.is_some(),
        "MemoryDispatch should handle selector 4"
    );
    assert!(
        lock_result.unwrap().is_ok(),
        "LockMemoryContiguous should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "LockMemoryContiguous should return noErr"
    );

    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, table);
    cpu.write_reg(Register::A1, 0); // ask only for required entry count
    let sp_pre = cpu.read_reg(Register::A7);
    let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(
        get_result.is_some(),
        "MemoryDispatch should handle selector 5"
    );
    assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "GetPhysical should return paramErr (-50) for entrycount=0 query"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        1,
        "GetPhysical should report one required translated entry for entrycount=0 query"
    );
    assert_eq!(
        sp_pre, sp_post,
        "GetPhysical should preserve the caller's stack pointer for entrycount=0 query"
    );
    assert_eq!(
        bus.read_long(table + 8),
        0x1111_2222,
        "GetPhysical(entrycount=0) should preserve physical entry address"
    );
    assert_eq!(
        bus.read_long(table + 12),
        0x3333_4444,
        "GetPhysical(entrycount=0) should preserve physical entry count"
    );
}

#[test]
fn test_memorydispatch_getphysical_entrycount_zero_on_locked_multpage_range_returns_two_and_preserves_table(
) {
    // Inside Macintosh: Memory (1992), 3-31: a zero-count GetPhysical
    // query reports the number of physical entries required for the
    // entire logical range and leaves the table untouched.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let table = bus.alloc(16);
    let logical_start = bus.alloc(8192) + 99;
    let logical_count = 5000u32;
    assert_eq!(
            super::vm_required_physical_entries(logical_start, logical_count),
            2,
            "zero-count GetPhysical should compute two required physical entries for the locked multi-page witness range"
        );
    bus.write_long(table, logical_start);
    bus.write_long(table + 4, logical_count);
    bus.write_long(table + 8, 0x5555_AAAA);
    bus.write_long(table + 12, 0x7777_BBBB);

    cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
    cpu.write_reg(Register::A0, logical_start);
    cpu.write_reg(Register::A1, logical_count);
    let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        lock_result.is_some(),
        "MemoryDispatch should handle selector 4"
    );
    assert!(
        lock_result.unwrap().is_ok(),
        "LockMemoryContiguous should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "LockMemoryContiguous should return noErr"
    );

    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, table);
    cpu.write_reg(Register::A1, 0); // ask only for required entry count
    let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        get_result.is_some(),
        "MemoryDispatch should handle selector 5"
    );
    assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "GetPhysical should return paramErr (-50) for entrycount=0 query"
    );
    assert_eq!(
            cpu.read_reg(Register::A0),
            2,
            "GetPhysical should report two required physical entries for this locked multi-page logical range"
        );
    assert_eq!(
        bus.read_long(table + 8),
        0x5555_AAAA,
        "GetPhysical(entrycount=0) should preserve physical entry address"
    );
    assert_eq!(
        bus.read_long(table + 12),
        0x7777_BBBB,
        "GetPhysical(entrycount=0) should preserve physical entry count"
    );
}

#[test]
fn test_memorydispatch_getphysical_entrycount_zero_on_empty_range_returns_paramerr_and_preserves_table(
) {
    // BasiliskII returns paramErr for an empty logical range queried
    // with physicalEntryCount=0 and leaves the table/A0 shape intact.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let table = bus.alloc(16);
    bus.write_long(table, 0x0010_0000);
    bus.write_long(table + 4, 0);
    bus.write_long(table + 8, 0xAAAA_BBBB);
    bus.write_long(table + 12, 0xCCCC_DDDD);

    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, table);
    cpu.write_reg(Register::A1, 0); // ask only for required entry count
    let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        get_result.is_some(),
        "MemoryDispatch should handle selector 5"
    );
    assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-50i32) as u32,
        "GetPhysical should return paramErr (-50) for empty-range entrycount=0 query"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        table,
        "GetPhysical(empty range) should preserve the A0 table pointer on return"
    );
    assert_eq!(
        bus.read_long(table + 8),
        0xAAAA_BBBB,
        "GetPhysical(empty range) should preserve physical entry address"
    );
    assert_eq!(
        bus.read_long(table + 12),
        0xCCCC_DDDD,
        "GetPhysical(empty range) should preserve physical entry count"
    );
}

#[test]
fn test_memorydispatch_getphysical_entrycount_zero_on_unlocked_range_returns_notlockederr() {
    // The logical range must stay locked while GetPhysical reports the
    // required physical-entry count, and the zero-count query still
    // preserves the caller's stack frame.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let table = bus.alloc(16);
    let logical_start = bus.alloc(4096) + 55;
    let logical_count = 1792u32;
    bus.write_long(table, logical_start);
    bus.write_long(table + 4, logical_count);
    bus.write_long(table + 8, 0x4444_5555);
    bus.write_long(table + 12, 0x6666_7777);

    cpu.write_reg(Register::D0, 4); // LockMemoryContiguous selector
    cpu.write_reg(Register::A0, logical_start);
    cpu.write_reg(Register::A1, logical_count);
    let lock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        lock_result.is_some(),
        "MemoryDispatch should handle selector 4"
    );
    assert!(
        lock_result.unwrap().is_ok(),
        "LockMemoryContiguous should return"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "LockMemoryContiguous should return noErr"
    );

    cpu.write_reg(Register::D0, 3); // UnlockMemory selector
    cpu.write_reg(Register::A0, logical_start);
    cpu.write_reg(Register::A1, logical_count);
    let unlock_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    assert!(
        unlock_result.is_some(),
        "MemoryDispatch should handle selector 3"
    );
    assert!(unlock_result.unwrap().is_ok(), "UnlockMemory should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "UnlockMemory should return noErr after releasing the range"
    );

    cpu.write_reg(Register::D0, 5); // GetPhysical selector
    cpu.write_reg(Register::A0, table);
    cpu.write_reg(Register::A1, 0); // ask only for required entry count
    let sp_pre = cpu.read_reg(Register::A7);
    let get_result = dispatcher.dispatch_memory(false, 0x5C, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);
    assert!(
        get_result.is_some(),
        "MemoryDispatch should handle selector 5"
    );
    assert!(get_result.unwrap().is_ok(), "GetPhysical should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        (-623i32) as u32,
        "GetPhysical should return notLockedErr (-623) for unlocked entrycount=0 query"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "GetPhysical should report zero translated entries for unlocked entrycount=0 query"
    );
    assert_eq!(
        sp_pre, sp_post,
        "GetPhysical should preserve the caller's stack pointer for unlocked entrycount=0 query"
    );
    assert_eq!(
        bus.read_long(table + 8),
        0x4444_5555,
        "GetPhysical(entrycount=0) should preserve physical entry address after unlock"
    );
    assert_eq!(
        bus.read_long(table + 12),
        0x6666_7777,
        "GetPhysical(entrycount=0) should preserve physical entry count after unlock"
    );
}

#[test]
fn countadbs_returns_number_of_connected_adb_devices_in_d0() {
    // Inside Macintosh Volume V (1986), p. V-372; Inside Macintosh:
    // Devices (1994), p. 5-42: CountADBs returns the number of ADB
    // devices by counting ADB device-table entries.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let result = dispatcher.dispatch_memory(false, 0x77, &mut cpu, &mut bus);
    assert!(result.is_some(), "CountADBs should be handled");
    assert!(result.unwrap().is_ok(), "CountADBs should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        2,
        "HLE CountADBs should report keyboard+mouse device count"
    );
}

#[test]
fn countadbs_takes_no_arguments_and_reports_no_error_codes() {
    // Inside Macintosh Volume V (1986), p. V-372; Inside Macintosh:
    // Devices (1994), p. 5-42: CountADBs has no arguments and no
    // separate error-code contract.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x77, &mut cpu, &mut bus);
    assert!(result.is_some(), "CountADBs should be handled");
    assert!(result.unwrap().is_ok(), "CountADBs should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "CountADBs should not consume stack parameters"
    );
    assert!(
        (cpu.read_reg(Register::D0) as i32) >= 0,
        "CountADBs should return only a nonnegative device-count result in D0"
    );
}

#[test]
fn getindadb_valid_index_returns_positive_adb_address() {
    // Inside Macintosh Volume V (1986), p. V-373; Inside Macintosh:
    // Devices (1994), p. 5-43: valid entry indexes return the current
    // ADB address as a positive function result.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::D0, 1);
    let first = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
    assert!(first.is_some(), "GetIndADB should be handled");
    assert!(first.unwrap().is_ok(), "GetIndADB(index=1) should return");
    assert!(
        (cpu.read_reg(Register::D0) as i32) > 0,
        "GetIndADB(index=1) should return a positive address"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        2,
        "HLE GetIndADB index 1 should map to keyboard ADB address 2"
    );

    cpu.write_reg(Register::D0, 2);
    let second = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
    assert!(second.is_some(), "GetIndADB should be handled");
    assert!(second.unwrap().is_ok(), "GetIndADB(index=2) should return");
    assert!(
        (cpu.read_reg(Register::D0) as i32) > 0,
        "GetIndADB(index=2) should return a positive address"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        3,
        "HLE GetIndADB index 2 should map to mouse ADB address 3"
    );
}

#[test]
fn getindadb_out_of_range_index_returns_negative_result() {
    // Inside Macintosh: Devices (1994), p. 5-43 (and IM:V V-373):
    // if GetIndADB cannot find the indexed entry, it returns a
    // negative function result.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let info = 0x320000u32;
    for i in 0..10u32 {
        bus.write_byte(info + i, 0xA5);
    }

    cpu.write_reg(Register::A0, info);
    cpu.write_reg(Register::D0, 0);
    let result = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetIndADB should be handled");
    assert!(result.unwrap().is_ok(), "GetIndADB should return");
    assert!(
        (cpu.read_reg(Register::D0) as i32) < 0,
        "Out-of-range GetIndADB index should return a negative result"
    );
}

#[test]
fn getindadb_writes_adbdatablock_for_valid_index() {
    // Inside Macintosh Volume V (1986), p. V-373; Inside Macintosh:
    // Devices (1994), p. 5-43: info receives an ADBDataBlock for the
    // selected entry.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let info = 0x320100u32;
    for i in 0..10u32 {
        bus.write_byte(info + i, 0xCC);
    }

    cpu.write_reg(Register::A0, info);
    cpu.write_reg(Register::D0, 1);
    let result = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetIndADB should be handled");
    assert!(result.unwrap().is_ok(), "GetIndADB should return");
    assert_eq!(
        bus.read_byte(info),
        2,
        "GetIndADB should report the extended-keyboard handler ID"
    );
    assert_eq!(
        bus.read_byte(info + 1),
        2,
        "GetIndADB should write origADBAddr for keyboard entry"
    );
    assert_eq!(
        bus.read_long(info + 2),
        0,
        "GetIndADB should write dbServiceRtPtr field in info block"
    );
    assert_eq!(
        bus.read_long(info + 6),
        0,
        "GetIndADB should write dbDataAreaAddr field in info block"
    );
}

#[test]
fn getindadb_preserves_caller_memory_beyond_adbdatablock() {
    // Per IM:V V-373: the ADBDataBlock is documented as 10 bytes
    // (devType + origADBAddr + dbServiceRtPtr + dbDataAreaAddr).
    // Pin that the HLE write loop does not clobber caller memory
    // beyond the 10-byte window (trailing-sentinel preservation).
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let info = 0x320180u32;
    for i in 0..12u32 {
        bus.write_byte(info + i, 0xFF);
    }

    cpu.write_reg(Register::A0, info);
    cpu.write_reg(Register::D0, 1);
    let result = dispatcher.dispatch_memory(false, 0x78, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetIndADB should be handled");
    assert!(result.unwrap().is_ok(), "GetIndADB should return");
    assert_ne!(
        bus.read_byte(info),
        0xFF,
        "GetIndADB should overwrite the sentinel at offset 0"
    );
    assert_eq!(
        bus.read_byte(info + 10),
        0xFF,
        "GetIndADB must not clobber caller memory at offset 10"
    );
    assert_eq!(
        bus.read_byte(info + 11),
        0xFF,
        "GetIndADB must not clobber caller memory at offset 11"
    );
}

#[test]
fn getadbinfo_returns_noerr_for_nominal_address() {
    // Inside Macintosh Volume V (1986), p. V-369: GetADBInfo returns
    // an OSErr result code in D0; noErr indicates successful completion.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let info = 0x320200u32;
    cpu.write_reg(Register::A0, info);
    cpu.write_reg(Register::D0, 2); // keyboard ADB address

    let result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetADBInfo should be handled");
    assert!(result.unwrap().is_ok(), "GetADBInfo should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "GetADBInfo nominal path should return noErr in D0"
    );
}

#[test]
fn getadbinfo_writes_adbdatablock_fields_through_a0_parameter_block() {
    // Inside Macintosh Volume V (1986), p. V-369: A0 points to an
    // ADBDataBlock parameter block that GetADBInfo writes on success.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let info = 0x320240u32;
    for i in 0..12u32 {
        bus.write_byte(info + i, 0xCC);
    }

    cpu.write_reg(Register::A0, info);
    cpu.write_reg(Register::D0, 3); // mouse ADB address
    let result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetADBInfo should be handled");
    assert!(result.unwrap().is_ok(), "GetADBInfo should return");
    assert_eq!(
        bus.read_byte(info),
        1,
        "GetADBInfo should report the standard mouse handler ID"
    );
    assert_eq!(
        bus.read_byte(info + 1),
        3,
        "GetADBInfo should report the mouse's original address"
    );
    assert_eq!(
        bus.read_long(info + 2),
        0,
        "service-routine pointer field should be written"
    );
    assert_eq!(
        bus.read_long(info + 6),
        0,
        "data-area pointer field should be written"
    );
}

#[test]
fn getadbinfo_preserves_stack_pointer_for_nominal_address() {
    // Inside Macintosh Volume V (1986), p. V-369: GetADBInfo uses
    // a register-only OS trap calling convention and should not
    // consume a Pascal argument frame.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let info = 0x320280u32;
    cpu.write_reg(Register::A0, info);
    cpu.write_reg(Register::D0, 2); // keyboard ADB address
    let sp_before = cpu.read_reg(Register::A7);

    let result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetADBInfo should be handled");
    assert!(result.unwrap().is_ok(), "GetADBInfo should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "GetADBInfo should preserve A7"
    );
}

#[test]
fn setadbinfo_returns_noerr_for_nominal_address() {
    // Inside Macintosh Volume V (1986), p. V-369: SetADBInfo returns
    // an OSErr result code in D0; noErr indicates successful completion.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let info = 0x320280u32;
    bus.write_long(info, 0x00AA_5500);
    bus.write_long(info + 4, 0x00CC_7700);
    cpu.write_reg(Register::A0, info);
    cpu.write_reg(Register::D0, 2);

    let result = dispatcher.dispatch_memory(false, 0x7A, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetADBInfo should be handled");
    assert!(result.unwrap().is_ok(), "SetADBInfo should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetADBInfo nominal path should return noErr in D0"
    );

    cpu.write_reg(Register::A0, info + 16);
    cpu.write_reg(Register::D0, 2);
    let get_result = dispatcher.dispatch_memory(false, 0x79, &mut cpu, &mut bus);
    assert!(get_result.unwrap().is_ok(), "GetADBInfo should return");
    assert_eq!(
        bus.read_long(info + 18),
        0x00AA_5500,
        "GetADBInfo should return the installed service routine"
    );
    assert_eq!(
        bus.read_long(info + 22),
        0x00CC_7700,
        "GetADBInfo should return the installed data area"
    );
}

#[test]
fn setadbinfo_uses_a0_parameter_block_and_d0_address_register_calling_convention() {
    // Inside Macintosh Volume V (1986), p. V-369: SetADBInfo takes
    // A0=ADBSetInfoBlock pointer and D0=ADB address on entry.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::D0, 0x0F);

    let result = dispatcher.dispatch_memory(false, 0x7A, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetADBInfo should be handled");
    assert!(result.unwrap().is_ok(), "SetADBInfo should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SetADBInfo should not consume stack arguments"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetADBInfo should return noErr in D0 on nominal calls"
    );
}

#[test]
fn adbreinit_has_no_parameters_and_preserves_stack_pointer() {
    // Inside Macintosh Volume V (1986), p. V-367: ADBReInit has no
    // parameters.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0xDEAD_BEEF);

    let result = dispatcher.dispatch_memory(false, 0x7B, &mut cpu, &mut bus);
    assert!(result.is_some(), "ADBReInit should be handled");
    assert!(result.unwrap().is_ok(), "ADBReInit should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "ADBReInit should preserve A7 because it takes no stack arguments"
    );
}

#[test]
fn adbop_returns_noerr_when_command_queue_accepts_request() {
    // Inside Macintosh Volume V (1986), p. V-368: ADBOp returns
    // noErr when the command is accepted.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x3202C0u32;
    cpu.write_reg(Register::A0, pb);
    cpu.write_reg(Register::D0, 0x08); // command byte

    let result = dispatcher.dispatch_memory(false, 0x7C, &mut cpu, &mut bus);
    assert!(result.is_some(), "ADBOp should be handled");
    assert!(result.unwrap().is_ok(), "ADBOp should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "ADBOp nominal path should return noErr in D0"
    );
}

#[test]
fn adbop_uses_a0_parameter_block_and_d0_commandnum_without_stack_arguments() {
    // Inside Macintosh Volume V (1986), p. V-368: ADBOp uses
    // A0=parameter-block pointer and D0=commandNum on entry.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::D0, 0x0C); // Flush command byte shape

    let result = dispatcher.dispatch_memory(false, 0x7C, &mut cpu, &mut bus);
    assert!(result.is_some(), "ADBOp should be handled");
    assert!(result.unwrap().is_ok(), "ADBOp should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "ADBOp should not consume stack arguments"
    );
}

#[test]
fn adbop_repeated_calls_balance_stack_no_drift() {
    // 8 successive ADBOp dispatches with varied commandNum bytes
    // (Flush + Talk-register-0..2 + Listen-register-0..3) and
    // varied ADB device addresses (1..8) preserve A7 in aggregate.
    // Per IM:V V-368, ADBOp uses only A0+D0 inputs and returns its
    // result in D0; no per-call pop discipline error can
    // accumulate across the composition.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    let pb = 0x3202C0u32;
    let commands: [u32; 8] = [
        0x11, // device 1 Flush
        0x2C, // device 2 Talk r0
        0x3D, // device 3 Talk r1
        0x4E, // device 4 Talk r2
        0x58, // device 5 Listen r0
        0x69, // device 6 Listen r1
        0x7A, // device 7 Listen r2
        0x8B, // device 8 Listen r3
    ];
    for command in commands.iter() {
        cpu.write_reg(Register::A0, pb);
        cpu.write_reg(Register::D0, *command);
        let result = dispatcher.dispatch_memory(false, 0x7C, &mut cpu, &mut bus);
        assert!(result.is_some(), "ADBOp should be handled");
        assert!(result.unwrap().is_ok(), "ADBOp should return");
    }
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "8 successive ADBOp calls should preserve A7 in aggregate"
    );
}

#[test]
fn control_writes_ioresult_and_returns_noerr_in_d0() {
    // IM:Devices 1994 p. 1-77: _Control returns the OSErr result in D0
    // and uses CntrlParam.ioResult in the parameter block.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = 0x310000u32;
    bus.write_word(pb + 26, 2); // supported cscSetMode request
    bus.write_long(pb + 28, record);
    bus.write_word(record, 0x83);
    bus.write_word(record + 6, 0);
    // Write a non-zero value at ioResult (pb+16) to verify it gets cleared.
    bus.write_word(pb + 16, 0xFFFF);
    cpu.write_reg(Register::A0, pb);
    let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
    assert!(result.is_some(), "_Control should be handled");
    assert!(result.unwrap().is_ok(), "_Control should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "_Control should set D0 to 0");
    assert_eq!(
        bus.read_word(pb + 16),
        0,
        "_Control should set ioResult at pb+16 to 0"
    );
}

#[test]
fn unsupported_control_request_reports_error_without_fabricating_output() {
    // Inside Macintosh: Devices (1994), pp. 1-76--1-77: controlErr
    // means the driver does not respond to this control request; the OS
    // trap returns its result in D0 and the parameter block's ioResult.
    for code in [0, 21, 22, 0xFFFF] {
        let (mut dispatcher, mut cpu, mut bus) = setup();
        let pb = 0x300000;
        bus.write_word(pb + 26, code);
        bus.write_word(pb + 16, 0x1234);
        bus.write_bytes(pb + 28, &[0xA5; 22]);
        cpu.write_reg(Register::A0, pb);
        let sp = cpu.read_reg(Register::A7);
        dispatcher
            .dispatch_memory(false, 0x04, &mut cpu, &mut bus)
            .unwrap()
            .unwrap();
        assert_eq!(cpu.read_reg(Register::D0) as i32, -17);
        assert_eq!(bus.read_word(pb + 16) as i16, -17);
        assert_eq!(bus.read_bytes(pb + 28, 22), vec![0xA5; 22]);
        assert_eq!(cpu.read_reg(Register::A0), pb);
        assert_eq!(cpu.read_reg(Register::A7), sp);
    }
}

#[test]
fn control_nil_paramblock_returns_noerr_and_preserves_stack() {
    // IM:Devices 1994 p. 1-77 (assembly): _Control takes the param block
    // in A0 and only D0 is defined as affected on return.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0);
    let result = dispatcher.dispatch_memory(false, 0x04, &mut cpu, &mut bus);
    assert!(result.is_some(), "_Control should be handled");
    assert!(result.unwrap().is_ok(), "_Control should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "_Control with NIL param block should still return noErr in D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "_Control should not consume stack arguments in the A0-param-block calling convention"
    );
}

#[test]
fn status_writes_ioresult_and_returns_noerr_in_d0() {
    // IM:Devices 1994 p. 1-80: _Status returns result in D0 and reports
    // driver status through CntrlParam (including ioResult).
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    // Write a non-zero value at ioResult (pb+16) to verify it gets cleared
    bus.write_word(pb + 16, 0xFFFF);
    cpu.write_reg(Register::A0, pb);
    let result = dispatcher.dispatch_memory(false, 0x05, &mut cpu, &mut bus);
    assert!(result.is_some(), "_Status should be handled");
    assert!(result.unwrap().is_ok(), "_Status should succeed");
    assert_eq!(cpu.read_reg(Register::D0), 0, "_Status should set D0 to 0");
    assert_eq!(
        bus.read_word(pb + 16),
        0,
        "_Status should set ioResult at pb+16 to 0"
    );
}

#[test]
fn video_status_returns_current_mode_page_and_base() {
    // The video driver's GetMode status call returns a VDPgInfo record.
    // Designing Cards and Drivers for the Macintosh II and SE (1987),
    // pp. 9-17 and 9-27.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = 0x310000u32;
    dispatcher.screen_mode = (0x01F8_0000, 800, 800, 600, 8);
    bus.write_word(pb + 24, 0);
    bus.write_word(pb + 26, 2);
    bus.write_long(pb + 28, record);
    bus.write_word(record, 0xFFFF);
    bus.write_long(record + 2, 0x1234_5678);
    bus.write_word(record + 6, 0xFFFF);
    bus.write_long(record + 8, 0xDEAD_BEEF);
    cpu.write_reg(Register::A0, pb);
    let sp_before = cpu.read_reg(Register::A7);

    dispatcher
        .dispatch_memory(false, 0x05, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(record), 0x0083);
    assert_eq!(bus.read_long(record + 2), 0x1234_5678);
    assert_eq!(bus.read_word(record + 6), 0);
    assert_eq!(bus.read_long(record + 8), 0x01F8_0000);
    assert_eq!(bus.read_word(pb + 16), 0);
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(cpu.read_reg(Register::A7), sp_before);
}

#[test]
fn video_status_reports_one_page_for_getpages() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = 0x310000u32;
    bus.write_word(pb + 24, 0);
    bus.write_word(pb + 26, 4);
    bus.write_long(pb + 28, record);
    bus.write_word(record, 0x0083);
    bus.write_word(record + 6, 0xFFFF);
    cpu.write_reg(Register::A0, pb);

    dispatcher
        .dispatch_memory(false, 0x05, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();

    assert_eq!(bus.read_word(record), 0x0083);
    assert_eq!(bus.read_word(record + 6), 1);
    assert_eq!(bus.read_word(pb + 16), 0);
}

#[test]
fn video_status_reports_color_or_grayscale_personality() {
    // GetGray uses VDPgInfo.csMode, not the request's result code.
    // Designing Cards and Drivers for the Macintosh II and SE (1987),
    // p. 9-17.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let pb = 0x300000u32;
    let record = 0x310000u32;
    let gdh = dispatcher.ensure_main_gdevice(&mut bus);
    let gd = bus.read_long(gdh);
    bus.write_word(pb + 24, 0);
    bus.write_word(pb + 26, 6);
    bus.write_long(pb + 28, record);
    cpu.write_reg(Register::A0, pb);

    bus.write_word(record, 0xFFFF);
    dispatcher
        .dispatch_memory(false, 0x05, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(record), 0);

    bus.write_word(gd + 20, bus.read_word(gd + 20) & !1);
    dispatcher
        .dispatch_memory(false, 0x05, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(bus.read_word(record), 1);
    assert_eq!(bus.read_word(pb + 16), 0);
}

#[test]
fn status_nil_paramblock_returns_noerr_and_preserves_stack() {
    // IM:Devices 1994 p. 1-80 (assembly): _Status uses A0 for the
    // parameter block and returns the result in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    cpu.write_reg(Register::A0, 0);
    let result = dispatcher.dispatch_memory(false, 0x05, &mut cpu, &mut bus);
    assert!(result.is_some(), "_Status should be handled");
    assert!(result.unwrap().is_ok(), "_Status should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "_Status with NIL param block should still return noErr in D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "_Status should not consume stack arguments in the A0-param-block calling convention"
    );
}

#[test]
fn pbstatus_writes_same_oserr_to_d0_and_ioresult_preserving_stack() {
    // Pre-poisons pb.ioResult at pb+16 with 0x3FFF (neither noErr nor
    // any documented OSErr), dispatches _Status with a clearly-bogus
    // ioRefNum 9999, witnesses that the trap overwrote the sentinel
    // AND that D0 == ioResult (per Device Manager dispatcher
    // convention IM:II 1985 p. II-114) AND that A7 is preserved
    // across the call (register-only OS-bit FUNCTION calling
    // convention per IM:Devices 1994 p. 1-80).
    let (mut dispatcher, mut cpu, mut bus) = setup();

    let pb = 0x300300u32;
    cpu.write_reg(Register::A0, pb);
    bus.write_word(pb + 16, 0x3FFFu16); // pre-poison ioResult
    bus.write_word(pb + 24, 9999u16); // bogus ioRefNum

    let sp_pre = cpu.read_reg(Register::A7);
    let result = dispatcher.dispatch_memory(false, 0x05, &mut cpu, &mut bus);
    let sp_post = cpu.read_reg(Register::A7);

    assert!(result.is_some(), "_Status should be handled");
    assert!(result.unwrap().is_ok(), "_Status should succeed");

    let d0 = cpu.read_reg(Register::D0) as i16;
    let io_result = bus.read_word(pb + 16) as i16;
    assert_ne!(
        io_result, 0x3FFFi16,
        "ioResult sentinel must be overwritten"
    );
    assert_eq!(d0, io_result, "D0 == ioResult per dispatcher convention");
    assert_eq!(
        d0,
        (-21i16),
        "bogus ioRefNum should collapse to badUnitErr on the HLE path"
    );
    assert_eq!(sp_pre, sp_post, "A7 preserved (register-only ABI)");
}

#[test]
fn setapplimit_updates_appllimit_when_limit_is_not_below_heap_extent() {
    // Inside Macintosh: Memory (1992), pp. 2-84..2-85:
    // SetApplLimit sets the current application heap limit to zoneLimit.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let heap_end = 0x0030_0000u32;
    let appl_limit_before = 0x0038_0000u32;
    let requested_limit = 0x003C_0000u32;
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(0x0114, heap_end); // HEAP_END
    bus.write_long(0x0130, appl_limit_before); // APPL_LIMIT

    cpu.write_reg(Register::A0, requested_limit);
    let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetApplLimit should be handled");
    assert!(result.unwrap().is_ok(), "SetApplLimit should return");
    assert_eq!(
            bus.read_long(0x0130),
            requested_limit,
            "SetApplLimit should store zoneLimit in ApplLimit when zoneLimit is not below current heap extent"
        );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetApplLimit should return noErr in D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "SetApplLimit uses A0 parameter passing and should not consume stack arguments"
    );
}

#[test]
fn setapplimit_below_heap_extent_does_not_cut_back_appllimit() {
    // Inside Macintosh: Memory (1992), p. 2-85:
    // if the zone already extends beyond zoneLimit, no cut-back occurs.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let heap_end = 0x0038_0000u32;
    let appl_limit_before = 0x0038_0000u32;
    let requested_limit = 0x0030_0000u32;
    bus.write_long(0x0114, heap_end); // HEAP_END
    bus.write_long(0x0130, appl_limit_before); // APPL_LIMIT

    cpu.write_reg(Register::A0, requested_limit);
    let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetApplLimit should be handled");
    assert!(result.unwrap().is_ok(), "SetApplLimit should return");
    assert_eq!(
        bus.read_long(0x0130),
        appl_limit_before,
        "SetApplLimit must not reduce ApplLimit when heap already extends beyond requested limit"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetApplLimit should still return noErr when lower limit is ignored"
    );
}

#[test]
fn setapplimit_below_heap_extent_leaves_heapend_and_appllimit_unchanged() {
    // Lower-limit branch: a requested limit below HeapEnd must leave
    // both HeapEnd and ApplLimit unchanged while still returning noErr.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let heap_end = 0x0038_0000u32;
    let appl_limit_before = 0x003C_0000u32;
    let requested_limit = heap_end - 0x1000;
    bus.write_long(0x0114, heap_end); // HEAP_END
    bus.write_long(0x0130, appl_limit_before); // APPL_LIMIT

    cpu.write_reg(Register::A0, requested_limit);
    let result = dispatcher.dispatch_memory(false, 0x2D, &mut cpu, &mut bus);
    assert!(result.is_some(), "SetApplLimit should be handled");
    assert!(result.unwrap().is_ok(), "SetApplLimit should return");
    assert_eq!(
        bus.read_long(0x0130),
        appl_limit_before,
        "SetApplLimit must not reduce ApplLimit when the requested limit is below HeapEnd"
    );
    assert_eq!(
        bus.read_long(0x0114),
        heap_end,
        "SetApplLimit should not rewrite HeapEnd"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetApplLimit should return noErr when the lower-limit path is taken"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        TEST_SP,
        "SetApplLimit should preserve A7"
    );
}

#[test]
fn setzone_writes_thezone_and_getzone_roundtrips() {
    // Inside Macintosh: Memory (1992), pp. 2-80..2-81:
    // SetZone makes hz current; GetZone returns the current zone.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let new_zone = 0x0BAD_F00Du32;
    bus.write_long(0x0118, 0x00AA_BBCC);

    cpu.write_reg(Register::A0, new_zone);
    let set_result = dispatcher.dispatch_memory(false, 0x1B, &mut cpu, &mut bus);
    assert!(set_result.is_some(), "SetZone should be handled");
    assert!(set_result.unwrap().is_ok(), "SetZone should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "SetZone should return noErr in D0"
    );
    assert_eq!(
        bus.read_long(0x0118),
        new_zone,
        "SetZone should write TheZone low-memory global ($0118)"
    );

    let get_result = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
    assert!(get_result.is_some(), "GetZone should be handled");
    assert!(get_result.unwrap().is_ok(), "GetZone should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        new_zone,
        "GetZone should return the zone selected by SetZone"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "GetZone should return noErr in D0"
    );
}

#[test]
fn setzone_roundtrip_with_saved_original_restores_thezone() {
    // Inside Macintosh: Memory (1992), pp. 2-80..2-81:
    // GetZone reads TheZone; SetZone writes A0 to TheZone.
    // Save/test/restore pattern: save the original TheZone via
    // GetZone, switch to a
    // different zone via SetZone, witness both the TheZone write
    // and the GetZone roundtrip on the new value, then restore
    // the original via SetZone and confirm GetZone observes it.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let original_zone = 0x00AA_BBCCu32;
    let new_zone = 0x0BAD_F00Du32;
    bus.write_long(0x0118, original_zone);

    // Save original via GetZone.
    let get_orig = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
    assert!(get_orig.is_some(), "GetZone should be handled");
    assert!(get_orig.unwrap().is_ok(), "GetZone should return");
    let saved_zone = cpu.read_reg(Register::A0);
    assert_eq!(
        saved_zone, original_zone,
        "GetZone should return the original TheZone before any switch"
    );

    // SetZone(new_zone) — writes TheZone.
    cpu.write_reg(Register::A0, new_zone);
    let set_new = dispatcher.dispatch_memory(false, 0x1B, &mut cpu, &mut bus);
    assert!(set_new.is_some(), "SetZone should be handled");
    assert!(set_new.unwrap().is_ok(), "SetZone should return");
    assert_eq!(
        bus.read_long(0x0118),
        new_zone,
        "SetZone should write the new zone pointer into TheZone ($0118)"
    );

    // GetZone after SetZone(new_zone) returns new_zone.
    let get_new = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
    assert!(get_new.is_some(), "GetZone should be handled");
    assert!(get_new.unwrap().is_ok(), "GetZone should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        new_zone,
        "GetZone should return the zone selected by SetZone (roundtrip)"
    );

    // Restore: SetZone(saved_zone).
    cpu.write_reg(Register::A0, saved_zone);
    let set_back = dispatcher.dispatch_memory(false, 0x1B, &mut cpu, &mut bus);
    assert!(set_back.is_some(), "SetZone should be handled");
    assert!(set_back.unwrap().is_ok(), "SetZone should return");
    assert_eq!(
        bus.read_long(0x0118),
        original_zone,
        "SetZone should restore the original TheZone value"
    );

    // GetZone after restore returns the original.
    let get_restored = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
    assert!(get_restored.is_some(), "GetZone should be handled");
    assert!(get_restored.unwrap().is_ok(), "GetZone should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        original_zone,
        "GetZone after restore should return the original TheZone"
    );
}

#[test]
fn setgrowzone_register_only_calling_convention_preserves_stack() {
    // Per IM:Memory 1992 p. 2-56 SetGrowZone is an OS-bit PROCEDURE
    // with a register-only ABI (A0 input, D0 result, no Pascal
    // stack frame). The test pins:
    //   - Single SetGrowZone(NIL) preserves A7 and returns D0=noErr
    //   - 5-call composition mixing NIL → synthetic ProcPtr → NIL →
    //     synthetic ProcPtr → NIL preserves A7 cumulatively
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_pre = cpu.read_reg(Register::A7);

    // Single-call: SetGrowZone(NIL)
    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::D0, 0xDEAD_BEEF);
    let single = dispatcher.dispatch_memory(false, 0x4B, &mut cpu, &mut bus);
    assert!(single.is_some(), "SetGrowZone should be handled");
    assert!(single.unwrap().is_ok(), "SetGrowZone should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "A7 preserved across single SetGrowZone(NIL) call"
    );
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        0,
        "SetGrowZone returns noErr in D0"
    );

    // 5-call composition: NIL → synthetic ProcPtr → NIL → ...
    let inputs = [0u32, 0x0040_0000, 0, 0x0040_0040, 0];
    for &a0 in &inputs {
        cpu.write_reg(Register::A0, a0);
        cpu.write_reg(Register::D0, 0xCAFE_F00D);
        let r = dispatcher.dispatch_memory(false, 0x4B, &mut cpu, &mut bus);
        assert!(r.is_some(), "SetGrowZone should be handled");
        assert!(
            r.unwrap().is_ok(),
            "SetGrowZone should succeed for A0=${a0:08X}"
        );
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "SetGrowZone returns noErr in D0 on each call"
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "A7 preserved across 5-call composition with mixed inputs"
    );
}

#[test]
fn purgemem_register_only_calling_convention_preserves_stack() {
    // Per IM:Memory 1992 p. 2-73 PurgeMem is an OS-bit PROCEDURE
    // with a register-only ABI (D0 = cbNeeded input, D0 = result
    // code output, no Pascal stack frame). The test pins:
    //   - Single PurgeMem(0) preserves A7 and returns D0=noErr
    //   - 5-call composition cycling cbNeeded values
    //     0 -> 256 -> 0 -> 1024 -> 0 preserves A7 cumulatively
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_pre = cpu.read_reg(Register::A7);

    // Single-call: PurgeMem(0)
    cpu.write_reg(Register::D0, 0);
    let single = dispatcher.dispatch_memory(false, 0x4D, &mut cpu, &mut bus);
    assert!(single.is_some(), "PurgeMem should be handled");
    assert!(single.unwrap().is_ok(), "PurgeMem should succeed");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "A7 preserved across single PurgeMem(0) call"
    );
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        0,
        "PurgeMem returns noErr in D0"
    );

    // 5-call composition: cbNeeded = 0 -> 256 -> 0 -> 1024 -> 0
    let inputs = [0u32, 256, 0, 1024, 0];
    for &d0 in &inputs {
        cpu.write_reg(Register::D0, d0);
        let r = dispatcher.dispatch_memory(false, 0x4D, &mut cpu, &mut bus);
        assert!(r.is_some(), "PurgeMem should be handled");
        assert!(
            r.unwrap().is_ok(),
            "PurgeMem should succeed for D0=${d0:08X}"
        );
        assert_eq!(
            cpu.read_reg(Register::D0) as i32,
            0,
            "PurgeMem returns noErr in D0 on each call"
        );
    }

    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_pre,
        "A7 preserved across 5-call composition with mixed cbNeeded inputs"
    );
}

#[test]
fn getzone_returns_thezone_pointer_and_noerr() {
    // Inside Macintosh: Memory (1992), p. 2-80:
    // GetZone returns TheZone in A0 and a result code in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let zone = 0x1234_5678u32;
    bus.write_long(0x0118, zone);

    let result = dispatcher.dispatch_memory(false, 0x1A, &mut cpu, &mut bus);
    assert!(result.is_some(), "GetZone should be handled");
    assert!(result.unwrap().is_ok(), "GetZone should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        zone,
        "GetZone should return TheZone in A0"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "GetZone should return noErr in D0"
    );
}

#[test]
fn handlezone_valid_handle_returns_current_zone_pointer() {
    // Inside Macintosh: Memory (1992), pp. 2-82..2-83:
    // HandleZone returns zone pointer for relocatable block handles.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let zone = 0x2001_1000u32;
    bus.write_long(0x0118, zone);

    cpu.write_reg(Register::D0, 4);
    let new_handle = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(new_handle.is_some(), "NewHandle should be handled");
    assert!(new_handle.unwrap().is_ok(), "NewHandle should return");
    let handle = cpu.read_reg(Register::A0);
    assert_ne!(handle, 0, "NewHandle should allocate a non-NIL handle");

    cpu.write_reg(Register::A0, handle);
    let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandleZone should be handled");
    assert!(result.unwrap().is_ok(), "HandleZone should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        zone,
        "HandleZone should return current zone pointer in A0"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HandleZone should return noErr for a valid handle"
    );
}

#[test]
fn handlezone_empty_handle_returns_zone_pointer() {
    // Inside Macintosh: Memory (1992), p. 2-82 important note:
    // empty handles still return a zone pointer (master-pointer zone).
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let zone = 0x2002_2000u32;
    bus.write_long(0x0118, zone);

    let empty_result = dispatcher.dispatch_memory(false, 0x66, &mut cpu, &mut bus);
    assert!(empty_result.is_some(), "NewEmptyHandle should be handled");
    assert!(
        empty_result.unwrap().is_ok(),
        "NewEmptyHandle should return"
    );
    let empty_handle = cpu.read_reg(Register::A0);
    assert_ne!(empty_handle, 0, "NewEmptyHandle should return a handle");
    assert_eq!(
        bus.read_long(empty_handle),
        0,
        "NewEmptyHandle should initialize master pointer to NIL"
    );

    cpu.write_reg(Register::A0, empty_handle);
    let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandleZone should be handled");
    assert!(result.unwrap().is_ok(), "HandleZone should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        zone,
        "HandleZone(empty-handle) should return a zone pointer"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HandleZone(empty-handle) should return noErr"
    );
}

#[test]
fn handlezone_nil_handle_returns_noerr_in_d0() {
    // BasiliskII-observed divergence from Inside Macintosh: Memory
    // (1992), p. 2-83: NIL handles return noErr in D0.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0);
    let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandleZone should be handled");
    assert!(result.unwrap().is_ok(), "HandleZone should return");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        0,
        "HandleZone(NIL) should return noErr (BasiliskII divergence)"
    );
}

#[test]
fn handlezone_disposed_handle_returns_memwzerr() {
    // Inside Macintosh: Memory (1992), p. 2-83:
    // memWZErr (-111) indicates attempt to operate on a free block.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 4);
    let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(result.is_some(), "NewHandle should be handled");
    assert!(result.unwrap().is_ok(), "NewHandle should return");
    let handle = cpu.read_reg(Register::A0);
    assert_ne!(handle, 0, "NewHandle should allocate a handle");

    cpu.write_reg(Register::A0, handle);
    let dispose = dispatcher.dispatch_memory(false, 0x23, &mut cpu, &mut bus);
    assert!(dispose.is_some(), "DisposeHandle should be handled");
    assert!(dispose.unwrap().is_ok(), "DisposeHandle should return");
    assert_eq!(
        bus.get_alloc_size(handle),
        None,
        "DisposeHandle should free the master-pointer slot"
    );

    cpu.write_reg(Register::A0, handle);
    let result = dispatcher.dispatch_memory(false, 0x26, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandleZone should be handled");
    assert!(result.unwrap().is_ok(), "HandleZone should return");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -111,
        "HandleZone(disposed handle) should return memWZErr (-111)"
    );
}

#[test]
fn ptrzone_valid_pointer_returns_current_zone_pointer() {
    // Inside Macintosh: Memory (1992), p. 2-83:
    // PtrZone returns the zone containing a nonrelocatable block pointer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let zone = 0x2003_3000u32;
    bus.write_long(0x0118, zone);

    cpu.write_reg(Register::D0, 8);
    let ptr_result = dispatcher.dispatch_memory(false, 0x1E, &mut cpu, &mut bus);
    assert!(ptr_result.is_some(), "NewPtr should be handled");
    assert!(ptr_result.unwrap().is_ok(), "NewPtr should return");
    let ptr = cpu.read_reg(Register::A0);
    assert_ne!(ptr, 0, "NewPtr should return a non-NIL pointer");

    cpu.write_reg(Register::A0, ptr);
    let result = dispatcher.dispatch_memory(false, 0x48, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrZone should be handled");
    assert!(result.unwrap().is_ok(), "PtrZone should return");
    assert_eq!(
        cpu.read_reg(Register::A0),
        zone,
        "PtrZone should return current zone pointer in A0"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "PtrZone should return noErr for a valid pointer"
    );
}

#[test]
fn ptrzone_nil_pointer_returns_memwzerr() {
    // Inside Macintosh: Memory (1992), p. 2-83:
    // PtrZone returns memWZErr for free/invalid pointer blocks.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0);
    let result = dispatcher.dispatch_memory(false, 0x48, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrZone should be handled");
    assert!(result.unwrap().is_ok(), "PtrZone should return");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -111,
        "PtrZone(NIL) should return memWZErr (-111)"
    );
}

// ==================== Toolbox Traps (is_tool=true) ====================

#[test]
fn debugstr_consumes_message_pointer_and_preserves_registers() {
    // Universal Interfaces 3.4 MacTypes.h declares
    // DebugStr(ConstStr255Param) as ONEWORDINLINE($ABFF).
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0x0012_3400);
    cpu.write_reg(Register::D0, 0x1234_5678);
    cpu.write_reg(Register::A0, 0x00AA_5500);
    let result = dispatcher.dispatch_memory(true, 0x3FF, &mut cpu, &mut bus);
    assert!(result.is_some(), "DebugStr should be handled");
    assert!(result.unwrap().is_ok(), "DebugStr should return");
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before + 4,
        "_DebugStr should consume its Pascal-string pointer"
    );
    assert_eq!(
        cpu.read_reg(Register::D0),
        0x1234_5678,
        "_DebugStr no-op path should preserve D0"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0x00AA_5500,
        "_DebugStr no-op path should preserve A0"
    );
}

#[test]
fn repeated_debugstr_calls_consume_one_message_pointer_each() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let sp_before = cpu.read_reg(Register::A7);
    for index in 0..5 {
        let sp = cpu.read_reg(Register::A7);
        bus.write_long(sp, 0x0012_3400 + index);
        dispatcher
            .dispatch_memory(true, 0x3FF, &mut cpu, &mut bus)
            .expect("DebugStr should be handled")
            .expect("DebugStr should return");
    }
    assert_eq!(cpu.read_reg(Register::A7), sp_before + 20);
}

#[test]
fn gettooltrapaddress_trap_word_variant_returns_callable_trampoline() {
    let (mut dispatcher, mut cpu, mut bus) = setup_with_trap_tables();
    let sp_before = cpu.read_reg(Register::A7);
    bus.write_long(sp_before, 0x1234_5678);

    // Simulate the real trap-instruction variant ($A746) so
    // dispatch_memory(false, 0x46, ..) takes the A746 branch.
    dispatcher.current_trap_word = 0xA746;
    cpu.write_reg(Register::D0, 0x00EC); // CopyBits tool-trap number

    let result = dispatcher.dispatch_memory(false, 0x46, &mut cpu, &mut bus);
    assert!(
        result.is_some(),
        "GetToolTrapAddress variant should be handled"
    );
    assert!(
        result.unwrap().is_ok(),
        "GetToolTrapAddress variant should succeed"
    );

    let addr = cpu.read_reg(Register::A0);
    assert_ne!(addr, 0, "tool-trap trampoline address should be nonzero");
    assert_eq!(
        bus.read_word(addr),
        0xACEC,
        "trampoline must encode auto-pop canonical trap word for CopyBits"
    );
    assert_eq!(
        cpu.read_reg(Register::A7),
        sp_before,
        "GetToolTrapAddress variant should preserve A7"
    );
    assert_eq!(
        bus.read_long(sp_before),
        0x1234_5678,
        "GetToolTrapAddress variant should leave caller stack memory untouched"
    );
}

#[test]
fn test_hand_to_hand() {
    // Inside Macintosh: Memory (1992), p. 2-62:
    // HandToHand returns a new handle to copied data in A0/theHndl.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    // Create a source handle with data
    cpu.write_reg(Register::D0, 8);
    let result = dispatcher.dispatch_memory(false, 0x22, &mut cpu, &mut bus);
    assert!(result.unwrap().is_ok());
    let src_handle = cpu.read_reg(Register::A0);
    let src_ptr = bus.read_long(src_handle);
    bus.write_long(src_ptr, 0xDEADBEEF);
    bus.write_long(src_ptr + 4, 0xCAFEBABE);
    // Call HandToHand (0x1E1)
    cpu.write_reg(Register::A0, src_handle);
    let result = dispatcher.dispatch_memory(true, 0x1E1, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandToHand should be handled");
    assert!(result.unwrap().is_ok(), "HandToHand should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HandToHand should return noErr"
    );
    let new_handle = cpu.read_reg(Register::A0);
    assert_ne!(
        new_handle, src_handle,
        "HandToHand should return a different handle"
    );
    let new_ptr = bus.read_long(new_handle);
    assert_ne!(
        new_ptr, src_ptr,
        "HandToHand should allocate new data block"
    );
    assert_eq!(
        bus.read_long(new_ptr),
        0xDEADBEEF,
        "HandToHand should copy data"
    );
    assert_eq!(
        bus.read_long(new_ptr + 4),
        0xCAFEBABE,
        "HandToHand should copy data"
    );
}

#[test]
fn test_hand_to_hand_nil_handle_returns_nilhandleerr() {
    // Inside Macintosh: Memory (1992), p. 2-63:
    // nilHandleErr (-109) for NIL master pointer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::A0, 0);
    let result = dispatcher.dispatch_memory(true, 0x1E1, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandToHand should be handled");
    assert!(result.unwrap().is_ok(), "HandToHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -109,
        "HandToHand should return nilHandleErr (-109) for NIL handle"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "HandToHand should leave A0 as NIL on nilHandleErr"
    );
}

#[test]
fn test_ptr_to_hand() {
    // Inside Macintosh: Memory (1992), p. 2-60:
    // PtrToHand returns a newly created handle whose bytes match srcPtr.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x300000u32;
    let data: [u8; 6] = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
    for (i, &b) in data.iter().enumerate() {
        bus.write_byte(src + i as u32, b);
    }
    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::D0, data.len() as u32);
    let result = dispatcher.dispatch_memory(true, 0x1E3, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrToHand should be handled");
    assert!(result.unwrap().is_ok(), "PtrToHand should succeed");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "PtrToHand should set D0 to 0 (noErr)"
    );
    let handle = cpu.read_reg(Register::A0);
    assert!(
        handle >= 0x200000,
        "PtrToHand should return a valid handle in A0, got ${:08X}",
        handle
    );
    let ptr = bus.read_long(handle);
    assert!(
        ptr >= 0x200000,
        "PtrToHand handle should point to a valid pointer, got ${:08X}",
        ptr
    );
    for (i, &b) in data.iter().enumerate() {
        assert_eq!(
            bus.read_byte(ptr + i as u32),
            b,
            "PtrToHand should copy byte {} correctly (expected 0x{:02X})",
            i,
            b
        );
    }
    assert_eq!(
        dispatcher
            .process_memory_manager()
            .borrow()
            .recover_handle(ptr),
        Some(handle),
        "PtrToHand should publish the reverse handle index immediately"
    );
}

#[test]
fn test_ptr_to_xhand_copies_bytes_and_returns_destination_handle() {
    // Inside Macintosh: Memory (1992), pp. 2-61..2-62:
    // PtrToXHand copies into an existing handle and returns dstHndl in A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    let src = 0x310000u32;
    let src_data: [u8; 4] = [0x41, 0x42, 0x43, 0x44];
    for (i, &b) in src_data.iter().enumerate() {
        bus.write_byte(src + i as u32, b);
    }

    cpu.write_reg(Register::D0, 3);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let dst_handle = cpu.read_reg(Register::A0);

    let dst_ptr_before = bus.read_long(dst_handle);
    for i in 0..3 {
        bus.write_byte(dst_ptr_before + i as u32, 0x7A);
    }

    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::A1, dst_handle);
    cpu.write_reg(Register::D0, src_data.len() as u32);
    let result = dispatcher.dispatch_memory(true, 0x1E2, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrToXHand should be handled");
    assert!(result.unwrap().is_ok(), "PtrToXHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "PtrToXHand should return noErr"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        dst_handle,
        "PtrToXHand should return destination handle in A0"
    );

    let dst_ptr_after = bus.read_long(dst_handle);
    assert_eq!(
        bus.get_alloc_size(dst_ptr_after),
        Some(src_data.len() as u32),
        "PtrToXHand should update the handle's logical size to the requested byte count"
    );
    for (i, &b) in src_data.iter().enumerate() {
        assert_eq!(
            bus.read_byte(dst_ptr_after + i as u32),
            b,
            "PtrToXHand should copy source byte {}",
            i
        );
    }
}

#[test]
fn test_ptr_to_xhand_same_bucket_shrink_updates_logical_size() {
    // Inside Macintosh: Memory (1992), p. 2-61:
    // PtrToXHand makes dstHndl a handle to a copy of size bytes
    // beginning at srcPtr, even when the new logical size stays in
    // the same aligned heap bucket.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x320000u32;
    bus.write_byte(src, 0x5A);

    cpu.write_reg(Register::D0, 4);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let dst_handle = cpu.read_reg(Register::A0);
    let dst_ptr_before = bus.read_long(dst_handle);
    for i in 0..4 {
        bus.write_byte(dst_ptr_before + i as u32, 0x71 + i as u8);
    }

    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::A1, dst_handle);
    cpu.write_reg(Register::D0, 1);
    let result = dispatcher.dispatch_memory(true, 0x1E2, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrToXHand should be handled");
    assert!(result.unwrap().is_ok(), "PtrToXHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "PtrToXHand should return noErr"
    );

    let dst_ptr_after = bus.read_long(dst_handle);
    assert_eq!(
        bus.get_alloc_size(dst_ptr_after),
        Some(1),
        "PtrToXHand should shrink the handle's logical size inside the same aligned bucket"
    );
    assert_eq!(
        bus.read_byte(dst_ptr_after),
        0x5A,
        "PtrToXHand should preserve copied byte 0 when shrinking the destination"
    );
}

#[test]
fn test_ptr_to_xhand_nil_destination_returns_nilhandleerr() {
    // Inside Macintosh: Memory (1992), p. 2-62:
    // nilHandleErr (-109) for NIL destination handle.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x320000u32;
    bus.write_byte(src, 0x55);
    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::A1, 0);
    cpu.write_reg(Register::D0, 1);
    let result = dispatcher.dispatch_memory(true, 0x1E2, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrToXHand should be handled");
    assert!(result.unwrap().is_ok(), "PtrToXHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -109,
        "PtrToXHand should return nilHandleErr (-109) for NIL destination"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "PtrToXHand should return NIL destination handle in A0"
    );
}

#[test]
fn test_hand_and_hand_appends_source_to_destination_and_returns_destination_handle() {
    // Inside Macintosh: Memory (1992), pp. 2-64..2-65:
    // HandAndHand appends aHndl to bHndl, leaves aHndl unchanged, returns bHndl in A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 2);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let src_handle = cpu.read_reg(Register::A0);
    let src_ptr = bus.read_long(src_handle);
    bus.write_byte(src_ptr, b'A');
    bus.write_byte(src_ptr + 1, b'B');

    cpu.write_reg(Register::D0, 2);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let dst_handle = cpu.read_reg(Register::A0);
    let dst_ptr = bus.read_long(dst_handle);
    bus.write_byte(dst_ptr, b'C');
    bus.write_byte(dst_ptr + 1, b'D');

    cpu.write_reg(Register::A0, src_handle);
    cpu.write_reg(Register::A1, dst_handle);
    let result = dispatcher.dispatch_memory(true, 0x1E4, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandAndHand should be handled");
    assert!(result.unwrap().is_ok(), "HandAndHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "HandAndHand should return noErr"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        dst_handle,
        "HandAndHand should return destination handle in A0"
    );

    let new_dst_ptr = bus.read_long(dst_handle);
    assert_eq!(bus.get_alloc_size(new_dst_ptr), Some(4));
    assert_eq!(bus.read_byte(new_dst_ptr), b'C');
    assert_eq!(bus.read_byte(new_dst_ptr + 1), b'D');
    assert_eq!(bus.read_byte(new_dst_ptr + 2), b'A');
    assert_eq!(bus.read_byte(new_dst_ptr + 3), b'B');

    assert_eq!(
        bus.read_byte(src_ptr),
        b'A',
        "HandAndHand should leave source handle contents unchanged"
    );
    assert_eq!(
        bus.read_byte(src_ptr + 1),
        b'B',
        "HandAndHand should leave source handle contents unchanged"
    );
}

#[test]
fn test_hand_and_hand_nil_source_returns_nilhandleerr() {
    // Inside Macintosh: Memory (1992), p. 2-65:
    // nilHandleErr (-109) for NIL master pointer.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 1);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let dst_handle = cpu.read_reg(Register::A0);

    cpu.write_reg(Register::A0, 0);
    cpu.write_reg(Register::A1, dst_handle);
    let result = dispatcher.dispatch_memory(true, 0x1E4, &mut cpu, &mut bus);
    assert!(result.is_some(), "HandAndHand should be handled");
    assert!(result.unwrap().is_ok(), "HandAndHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -109,
        "HandAndHand should return nilHandleErr (-109) when source handle is NIL"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        dst_handle,
        "HandAndHand should still return destination handle in A0"
    );
}

#[test]
fn test_ptr_and_hand_appends_pointer_data_and_returns_destination_handle() {
    // Inside Macintosh: Memory (1992), pp. 2-65..2-66:
    // PtrAndHand appends bytes from pntr to hndl and returns hndl in A0.
    let (mut dispatcher, mut cpu, mut bus) = setup();

    cpu.write_reg(Register::D0, 3);
    dispatcher
        .dispatch_memory(false, 0x22, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    let dst_handle = cpu.read_reg(Register::A0);
    let dst_ptr = bus.read_long(dst_handle);
    bus.write_byte(dst_ptr, b'H');
    bus.write_byte(dst_ptr + 1, b'E');
    bus.write_byte(dst_ptr + 2, b'L');

    let src = 0x330000u32;
    bus.write_byte(src, b'L');
    bus.write_byte(src + 1, b'O');

    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::A1, dst_handle);
    cpu.write_reg(Register::D0, 2);
    let result = dispatcher.dispatch_memory(true, 0x1EF, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrAndHand should be handled");
    assert!(result.unwrap().is_ok(), "PtrAndHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0),
        0,
        "PtrAndHand should return noErr"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        dst_handle,
        "PtrAndHand should return destination handle in A0"
    );

    let new_dst_ptr = bus.read_long(dst_handle);
    assert_eq!(bus.get_alloc_size(new_dst_ptr), Some(5));
    assert_eq!(bus.read_byte(new_dst_ptr), b'H');
    assert_eq!(bus.read_byte(new_dst_ptr + 1), b'E');
    assert_eq!(bus.read_byte(new_dst_ptr + 2), b'L');
    assert_eq!(bus.read_byte(new_dst_ptr + 3), b'L');
    assert_eq!(bus.read_byte(new_dst_ptr + 4), b'O');
}

#[test]
fn test_ptr_and_hand_nil_destination_returns_nilhandleerr() {
    // Inside Macintosh: Memory (1992), p. 2-66:
    // nilHandleErr (-109) for NIL destination handle.
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let src = 0x340000u32;
    bus.write_byte(src, 0xAA);
    cpu.write_reg(Register::A0, src);
    cpu.write_reg(Register::A1, 0);
    cpu.write_reg(Register::D0, 1);
    let result = dispatcher.dispatch_memory(true, 0x1EF, &mut cpu, &mut bus);
    assert!(result.is_some(), "PtrAndHand should be handled");
    assert!(result.unwrap().is_ok(), "PtrAndHand should return");
    assert_eq!(
        cpu.read_reg(Register::D0) as i32,
        -109,
        "PtrAndHand should return nilHandleErr (-109) for NIL destination"
    );
    assert_eq!(
        cpu.read_reg(Register::A0),
        0,
        "PtrAndHand should return NIL destination handle in A0"
    );
}

// ==================== Unhandled trap returns None ====================

#[test]
fn test_unhandled_trap_returns_none() {
    let (mut dispatcher, mut cpu, mut bus) = setup();
    let result = dispatcher.dispatch_memory(false, 0xFFFF, &mut cpu, &mut bus);
    assert!(result.is_none(), "Unhandled trap should return None");
}
