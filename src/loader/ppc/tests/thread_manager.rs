use super::*;

#[test]
fn threads_lib_get_current_thread_reaches_the_native_thread_manager() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"ThreadsLib",
        b"GetCurrentThread",
    ))
    .unwrap();
    let out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(out, vec![0xaa; 4]);
    loaded.cpu.gpr[3] = out;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(out), Some(2));
}

#[test]
fn threads_lib_set_thread_terminator_records_the_registered_callback() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"ThreadsLib",
        b"SetThreadTerminator",
    ))
    .unwrap();
    loaded.cpu.gpr[3..6].copy_from_slice(&[2, PPC_CODE_BASE + 0x100, 0x1234_5678]);
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(
        loaded
            .toolbox_startup
            .execution
            .calls()
            .thread_terminator(crate::guest_call::ExecutionTaskId::APPLICATION),
        Some((PPC_CODE_BASE + 0x100, 0x1234_5678))
    );

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 99;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-618));
}

#[test]
fn threads_lib_set_thread_switcher_keeps_in_and_out_callbacks_separate() {
    let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
        b"ThreadsLib",
        b"SetThreadSwitcher",
    ))
    .unwrap();
    let calls = loaded.guest_calls().shared_handle();
    for (switch_in, procedure, parameter) in [
        (false, PPC_CODE_BASE + 0x100, 0xaaaa),
        (true, PPC_CODE_BASE + 0x200, 0xbbbb),
    ] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3..7].copy_from_slice(&[2, procedure, parameter, u32::from(switch_in)]);
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
    }
    assert_eq!(
        calls.thread_switcher(crate::guest_call::ExecutionTaskId::APPLICATION, false),
        Some((PPC_CODE_BASE + 0x100, 0xaaaa))
    );
    assert_eq!(
        calls.thread_switcher(crate::guest_call::ExecutionTaskId::APPLICATION, true),
        Some((PPC_CODE_BASE + 0x200, 0xbbbb))
    );
}

#[test]
fn native_thread_terminator_runs_before_returning_the_thread_result() {
    use crate::guest_call::ExecutionTaskId;
    const ENTRY: u32 = PPC_CODE_BASE + 0x2000;
    const TERMINATOR: u32 = PPC_CODE_BASE + 0x2400;
    const MADE: u32 = PPC_DATA_BASE + 0x2000;
    const OUTPUT: u32 = PPC_DATA_BASE + 0x3000;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    loaded.memory.add_region(
        ENTRY,
        [0x3860_002au32, BLR]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(
        TERMINATOR,
        [
            0x3cc0_0000 | (OUTPUT >> 16),
            0x60c6_0000 | (OUTPUT & 0xffff),
            0x9066_0000,
            0x9086_0004,
            0x3860_0063,
            BLR,
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    loaded.memory.add_region(MADE, vec![0; 4]);
    loaded.memory.add_region(OUTPUT, vec![0; 12]);
    loaded.cpu.gpr[3..10].copy_from_slice(&[1, ENTRY, 17, 4096, 0, OUTPUT + 8, MADE]);
    assert_eq!(loaded.run_with_hle_imports(64).unsupported_import_index, None);
    let worker = ExecutionTaskId::from_thread_id(loaded.memory.read_u32_be(MADE).unwrap());
    loaded
        .guest_calls()
        .set_thread_terminator(worker, TERMINATOR, 0x1234_5678)
        .unwrap();
    let calls = loaded.guest_calls().shared_handle();
    assert!(calls
        .yield_native_thread(&mut loaded.cpu, worker.thread_id())
        .unwrap());
    for _ in 0..5 {
        loaded.run_with_hle_imports(256);
        if loaded.guest_calls().scheduling_state(worker).is_none() {
            break;
        }
    }
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(worker.thread_id()));
    assert_eq!(loaded.memory.read_u32_be(OUTPUT + 4), Some(0x1234_5678));
    assert_eq!(loaded.memory.read_u32_be(OUTPUT + 8), Some(42));
    assert_eq!(loaded.guest_calls().current_task(), ExecutionTaskId::APPLICATION);
}

#[test]
fn native_stack_space_import_uses_the_parked_classic_application_limit() {
    use crate::guest_call::{ExecutionTaskId, GuestCallTarget, PowerPcArguments};
    const OUTPUT: u32 = PPC_DATA_BASE + 0x5000;
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"ThreadCurrentStackSpace")).unwrap();
    loaded.memory.add_region(0, vec![0; 0x200]);
    loaded
        .memory
        .write_u32_be(crate::memory::globals::addr::APPL_LIMIT, 0x8000)
        .unwrap();
    loaded.memory.add_region(OUTPUT, vec![0; 4]);
    assert!(loaded.application_heap_limit() > 0x9000);
    assert!(loaded.guest_calls()
        .bind_task_entry_isa(ExecutionTaskId::APPLICATION, GuestIsa::M68k));
    let mut classic = crate::cpu::M68kCpu::new();
    classic.core.set_a(7, 0x9000);
    assert!(loaded.guest_calls().begin_m68k_to_powerpc(
        GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: loaded.entry_pc,
            rtoc: loaded.rtoc
        },
        PowerPcArguments::from_slice(&[1, OUTPUT]).unwrap(),
        0x1000,
        0x9004,
        None
    ));
    loaded.activate_powerpc_from_m68k(&mut classic).unwrap();
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(0x1000));
}

#[test]
fn native_thread_stack_space_queries_current_suspended_and_classic_threads() {
    use crate::guest_call::{CooperativeThread, ExecutionTaskId, ThreadStorage};
    const OUTPUT: u32 = PPC_DATA_BASE + 0x5000;
    const MADE: u32 = OUTPUT + 8;
    const PARTIAL: u32 = OUTPUT + 0x1000;
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"ThreadCurrentStackSpace")).unwrap();
    loaded.memory.add_region(OUTPUT, vec![0xaa; 16]);
    loaded.memory.add_region(PARTIAL, vec![0x5a; 3]);
    let query = |loaded: &mut PpcLoadedApp, thread: u32, output: u32| {
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import("InterfaceLib", "ThreadCurrentStackSpace");
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = thread;
        loaded.cpu.gpr[4] = output;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        loaded.cpu.gpr[3] as i16
    };
    let main_sp = loaded.cpu.gpr[1];
    for alias in [0, 1, 2] {
        assert_eq!(query(&mut loaded, alias, OUTPUT), 0);
        assert_eq!(
            loaded.memory.read_u32_be(OUTPUT),
            Some(main_sp - loaded.application_heap_limit())
        );
    }
    let limit = loaded.application_heap_limit() - 0x400;
    loaded.imports[0].dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", "SetApplLimit");
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = limit;
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.application_heap_limit(), limit);
    assert_eq!(query(&mut loaded, 1, OUTPUT), 0);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(main_sp - limit));

    loaded.imports[0].dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", "NewThread");
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3..10].copy_from_slice(&[1, PPC_CODE_BASE, 0, 4096, 0, 0, MADE]);
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let worker = ExecutionTaskId::from_thread_id(loaded.memory.read_u32_be(MADE).unwrap());
    assert_eq!(query(&mut loaded, worker.thread_id(), OUTPUT), 0);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(4096 - 64));
    let mut classic = CooperativeThread::default();
    classic.a_regs[7] = 0x9300;
    let classic = loaded.guest_calls()
        .create_classic_thread(
            classic,
            ThreadStorage {
                stack_base: 0x9000,
                stack_limit: 0xa000,
                ..Default::default()
            },
            true,
            |_| true,
        )
        .unwrap();
    assert_eq!(query(&mut loaded, classic.thread_id(), OUTPUT), 0);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(0x300));
    assert!(loaded.toolbox_startup.execution.calls()
        .yield_native_thread(&mut loaded.cpu, worker.thread_id())
        .unwrap());
    let worker_sp = loaded.cpu.gpr[1];
    let storage = loaded.guest_calls().thread_storage(worker).unwrap();
    assert_eq!(query(&mut loaded, 1, OUTPUT), 0);
    assert_eq!(
        loaded.memory.read_u32_be(OUTPUT),
        Some(worker_sp - storage.stack_base)
    );
    assert_eq!(query(&mut loaded, 2, OUTPUT), 0);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(main_sp - limit));
    assert_eq!(
        crate::thread_manager::ThreadManager::new(loaded.guest_calls()).stack_space(
            2,
            GuestIsa::M68k,
            0x1234,
            |isa| {
                assert_eq!(isa, GuestIsa::PowerPc);
                limit
            },
        ),
        Ok(main_sp - limit),
    );
    loaded.cpu.gpr[1] = storage.stack_base - 4;
    assert_eq!(query(&mut loaded, 1, OUTPUT), 0);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(0));
    loaded.cpu.gpr[1] = storage.stack_limit + 4;
    assert_eq!(query(&mut loaded, 1, OUTPUT), -619);
    loaded.cpu.gpr[1] = worker_sp;
    assert_eq!(query(&mut loaded, 0xdead, OUTPUT), -618);
    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(0));
    assert_eq!(query(&mut loaded, 1, PARTIAL), -50);
    assert_eq!(
        ppc_memory_read_bytes(&mut loaded.memory, PARTIAL, 3),
        Some(vec![0x5a; 3])
    );
    assert_eq!(query(&mut loaded, 1, 0), -50);
    assert_eq!(loaded.guest_calls().current_task(), worker);
}

#[test]
fn native_thread_pool_imports_create_query_validate_and_supply_new_threads() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"CreateThreadPool")).unwrap();
    let output = PPC_DATA_BASE + 0x5000;
    loaded.memory.add_region(output, vec![0xaa; 4]);
    let invoke = |loaded: &mut PpcLoadedApp, name: &str, args: [u32; 3]| {
        loaded.imports[0].symbol_name = name.into();
        loaded.imports[0].dispatcher_target =
            dispatcher_target_for_import("InterfaceLib", name);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3..6].copy_from_slice(&args);
        let result = loaded.run_with_hle_imports(64);
        assert_eq!(result.unsupported_import_index, None);
        assert_eq!(result.handled_import_count, 1);
        loaded.cpu.gpr[3] as i16
    };
    assert_eq!(invoke(&mut loaded, "CreateThreadPool", [1, 3, 1024]), 0);
    assert_eq!(invoke(&mut loaded, "CreateThreadPool", [1, 1, 2048]), 0);
    assert_eq!(invoke(&mut loaded, "GetFreeThreadCount", [1, output, 0]), 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x0004aaaa));
    assert_eq!(
        invoke(&mut loaded, "GetSpecificFreeThreadCount", [1, 1536, output]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x0001aaaa));
    assert_eq!(
        invoke(&mut loaded, "GetFreeThreadCount", [0, output, 0]),
        -50
    );
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x0001aaaa));
    assert_eq!(
        invoke(&mut loaded, "GetFreeThreadCount", [1, output + 3, 0]),
        -50
    );
    assert_eq!(loaded.memory.read_u32_be(output), Some(0x0001aaaa));
    assert_eq!(
        invoke(&mut loaded, "CreateThreadPool", [1, 0xffff, 1024]),
        -50
    );
    assert_eq!(
        invoke(&mut loaded, "CreateThreadPool", [1, 1, u32::MAX]),
        -50
    );
    assert_eq!(
        invoke(&mut loaded, "GetDefaultThreadStackSize", [1, output, 0]),
        0
    );
    assert_eq!(loaded.memory.read_u32_be(output), Some(32 * 1024));
    // Pool creation assigns no task IDs; the first premade NewThread does.
    loaded.cpu.gpr[6] = 1024;
    loaded.cpu.gpr[7] = 1 | 2 | 16;
    loaded.cpu.gpr[8] = 0;
    loaded.cpu.gpr[9] = output;
    let entry = loaded.entry_pc;
    assert_eq!(invoke(&mut loaded, "NewThread", [1, entry, 0x1234]), 0);
    assert_eq!(loaded.memory.read_u32_be(output), Some(3));
    assert_eq!(invoke(&mut loaded, "GetFreeThreadCount", [1, output, 0]), 0);
    assert_eq!(loaded.memory.read_u16_be(output), Some(3));
    assert_eq!(loaded.guest_calls().thread_pool_count(GuestIsa::M68k, 0), 0);
}

#[test]
fn native_thread_creation_clears_the_output_on_failure_without_publishing_a_task() {
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    let made = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(made, vec![0xaa; 4]);
    loaded.cpu.gpr[3] = 2;
    loaded.cpu.gpr[4] = loaded.entry_pc;
    loaded.cpu.gpr[6] = 1024;
    loaded.cpu.gpr[9] = made;
    assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(loaded.memory.read_u32_be(made), Some(0));
    assert!(!loaded.guest_calls().has_live_workers());
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[7] = 2; // kUsePremadeThread, with an empty pool
    assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-617));
    assert_eq!(loaded.memory.read_u32_be(made), Some(0));
    assert!(!loaded.guest_calls().has_live_workers());
    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[7] = 2 | 4; // kCreateIfNeeded preserves the unconsumed ID
    assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.memory.read_u32_be(made), Some(3));
}

#[test]
fn native_thread_creation_rejects_descriptors_before_allocation_and_preserves_vector_state() {
    use crate::guest_call::ExecutionTaskId;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    let made = PPC_DATA_BASE + 0x1000;
    let descriptor = PPC_DATA_BASE + 0x2000;
    let target = loaded.entry_pc;
    let rtoc = PPC_DATA_BASE + 0x3000;
    loaded.memory.add_region(made, vec![0xaa; 4]);
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
    let heap_before = loaded.heap_cursor();

    let invoke = |loaded: &mut PpcLoadedApp| {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 1;
        loaded.cpu.gpr[4] = descriptor;
        loaded.cpu.gpr[5] = 0x1234;
        loaded.cpu.gpr[6] = 4096;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = made;
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
    };

    let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    let tvector = descriptor + 0x40;
    loaded.memory.write_u32_be(tvector, target).unwrap();
    loaded.memory.write_u32_be(tvector + 4, rtoc).unwrap();
    for (isa, procedure) in [
        (PPC_ROUTINE_RECORD_POWERPC_ISA, tvector),
        (PPC_ROUTINE_RECORD_M68K_ISA, PPC_CODE_BASE),
    ] {
        assert!(ppc_write_routine_record(
            &mut loaded.memory,
            record,
            0,
            isa,
            0,
            procedure,
        ));
        let callable = resolve_guest_procedure(
            &mut loaded.memory,
            descriptor,
            loaded.cpu.gpr[2],
            None,
            GuestIsa::PowerPc,
            GuestIsa::PowerPc,
        )
        .expect("the test descriptor must be callable through the generic resolver");
        let (expected_isa, expected_entry) = if isa == PPC_ROUTINE_RECORD_POWERPC_ISA {
            (GuestIsa::PowerPc, target)
        } else {
            (GuestIsa::M68k, PPC_CODE_BASE)
        };
        assert_eq!(callable.isa, expected_isa);
        assert_eq!(callable.entry, expected_entry);
        loaded.memory.write_u32_be(made, 0xaaaa_aaaa).unwrap();
        invoke(&mut loaded);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u32_be(made), Some(0));
        assert_eq!(loaded.heap_cursor(), heap_before);
        assert!(!loaded.guest_calls().has_live_workers());
    }

    loaded.memory.write_u32_be(descriptor, target).unwrap();
    loaded.memory.write_u32_be(descriptor + 4, rtoc).unwrap();
    invoke(&mut loaded);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let worker = ExecutionTaskId::from_thread_id(loaded.memory.read_u32_be(made).unwrap());
    assert_eq!(worker.thread_id(), 3);
    assert!(loaded.toolbox_startup.execution.calls()
        .yield_native_thread(&mut loaded.cpu, worker.thread_id())
        .unwrap());
    assert_eq!(loaded.cpu.pc, target);
    assert_eq!(loaded.cpu.gpr[2], rtoc);
    assert_eq!(loaded.cpu.gpr[3], 0x1234);
}

#[test]
fn native_thread_creation_preflights_output_and_projects_real_allocation_failure() {
    use crate::guest_call::ExecutionTaskId;

    let invoke = |loaded: &mut PpcLoadedApp, made: u32, size: u32| {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 1;
        loaded.cpu.gpr[4] = PPC_CODE_BASE;
        loaded.cpu.gpr[5] = 0x1234;
        loaded.cpu.gpr[6] = size;
        loaded.cpu.gpr[7] = 0;
        loaded.cpu.gpr[8] = 0;
        loaded.cpu.gpr[9] = made;
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
    };

    let mut protected = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    let partial_made = PPC_DATA_BASE + 0x1000;
    let valid_made = partial_made + 0x100;
    protected.memory.add_region(partial_made, vec![0xaa; 4]);
    protected
        .memory
        .add_readonly_region(partial_made + 3, vec![0xaa]);
    protected.memory.add_region(valid_made, vec![0; 4]);
    let protected_cursor = protected.heap_cursor();
    let protected_ptrs = protected.ptrs();
    let protected_mem_error = protected.last_mem_error();
    invoke(&mut protected, partial_made, 4096);
    assert_eq!(protected.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
    assert_eq!(
        ppc_memory_read_bytes(&mut protected.memory, partial_made, 4),
        Some(vec![0xaa; 4])
    );
    assert_eq!(protected.heap_cursor(), protected_cursor);
    assert_eq!(protected.ptrs(), protected_ptrs);
    assert_eq!(protected.last_mem_error(), protected_mem_error);
    assert!(!protected.guest_calls().has_live_workers());
    invoke(&mut protected, valid_made, 4096);
    assert_eq!(protected.cpu.gpr[3], 0);
    assert_eq!(protected.memory.read_u32_be(valid_made), Some(3));

    let mut exhausted = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    let made = PPC_DATA_BASE + 0x1000;
    exhausted.memory.add_region(made, vec![0xaa; 4]);
    let cursor = exhausted.heap_cursor();
    let ptrs = exhausted.ptrs();
    let free_ptrs = exhausted.free_ptr_blocks();
    let free_bytes = exhausted
        .memory
        .read_u32_be(PPC_APPLICATION_ZONE + 12)
        .unwrap();
    invoke(&mut exhausted, made, i32::MAX as u32);
    assert_eq!(exhausted.cpu.gpr[3], ppc_i16_result(PPC_MEM_FULL_ERR));
    assert_eq!(exhausted.memory.read_u32_be(made), Some(0));
    assert_eq!(exhausted.heap_cursor(), cursor);
    assert_eq!(exhausted.ptrs(), ptrs);
    assert_eq!(exhausted.free_ptr_blocks(), free_ptrs);
    assert_eq!(exhausted.last_mem_error(), PPC_MEM_FULL_ERR);
    assert_eq!(
        exhausted.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(free_bytes)
    );
    assert_eq!(
        exhausted.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
        Some(
            ppc_heap_free_capacity(
                &exhausted.memory,
                exhausted.heap_cursor(),
                test_heap_limit!(exhausted),
            )
            .0
        )
    );
    assert!(!exhausted.guest_calls().has_live_workers());

    invoke(&mut exhausted, made, 4096);
    assert_eq!(exhausted.cpu.gpr[3], 0);
    assert_eq!(
        exhausted.memory.read_u32_be(made),
        Some(ExecutionTaskId::from_thread_id(3).thread_id())
    );
    assert_eq!(exhausted.last_mem_error(), PPC_NO_ERR);
}

#[test]
fn native_thread_entry_returns_to_its_creator_and_retries_result_delivery() {
    use crate::guest_call::ExecutionTaskId;
    const ENTRY: u32 = PPC_CODE_BASE + 0x2000;
    const MADE: u32 = PPC_DATA_BASE + 0x2000;
    const RESULT: u32 = PPC_DATA_BASE + 0x3000;
    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"NewThread")).unwrap();
    loaded.memory.add_region(
        ENTRY,
        [0x3860_002au32, BLR]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    loaded.memory.add_region(MADE, vec![0; 4]);
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = ENTRY;
    loaded.cpu.gpr[5] = 17;
    loaded.cpu.gpr[6] = 4096;
    loaded.cpu.gpr[7] = 0;
    loaded.cpu.gpr[8] = RESULT; // deliberately unmapped until return retry
    loaded.cpu.gpr[9] = MADE;
    loaded.cpu.gpr[20] = 0x1234_5678;
    loaded.cpu.fpr[20] = 0x4009_21fb_5444_2d18;
    loaded.cpu.cr = 0x1357_2468;
    let probe = loaded.run_with_hle_imports(64);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    let worker = ExecutionTaskId::from_thread_id(loaded.memory.read_u32_be(MADE).unwrap());
    assert_eq!(worker.thread_id(), 3);
    let mut yielding = loaded.imports[0].clone();
    yielding.symbol_index = 1;
    yielding.symbol_name = "YieldToAnyThread".into();
    yielding.dispatcher_target =
        dispatcher_target_for_import("InterfaceLib", "YieldToAnyThread");
    yielding.trap_pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.imports.push(yielding);
    loaded.import_count = 2;
    loaded.cpu.pc = PPC_IMPORT_TRAP_BASE + 4;
    loaded.cpu.lr = PPC_HALT_PC;
    let creator = loaded.cpu.clone();
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.guest_calls().current_task(), worker);
    assert_eq!(loaded.cpu.pc, ENTRY);
    assert_eq!(loaded.cpu.gpr[2], creator.gpr[2]);
    assert_eq!(loaded.cpu.gpr[3], 17);
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.guest_calls().current_task(), worker);
    assert_eq!(loaded.cpu.pc, PPC_THREAD_RETURN_PC);
    assert_eq!(loaded.cpu.gpr[3], 42);
    loaded.memory.add_region(RESULT, vec![0; 4]);
    loaded.run_with_hle_imports(64);
    assert_eq!(
        loaded.guest_calls().current_task(),
        ExecutionTaskId::APPLICATION
    );
    assert_eq!(loaded.guest_calls().scheduling_state(worker), None);
    assert_eq!(loaded.memory.read_u32_be(RESULT), Some(42));
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
    assert_eq!(loaded.cpu.gpr[20], creator.gpr[20]);
    assert_eq!(loaded.cpu.gpr[1], creator.gpr[1]);
    assert_eq!(loaded.cpu.gpr[2], creator.gpr[2]);
    assert_eq!(loaded.cpu.fpr, creator.fpr);
    assert_eq!(loaded.cpu.cr, creator.cr);
    assert!(loaded.cpu.time_base() >= creator.time_base());
}

#[test]
fn native_yield_imports_preserve_state_when_refused_or_quiescent() {
    use crate::guest_call::{ExecutionTaskId, NativeThreadContext, ThreadStorage};

    for symbol in [b"YieldToAnyThread".as_slice(), b"YieldToThread".as_slice()] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(symbol)).unwrap();
        let mut worker_cpu = loaded.cpu.clone();
        worker_cpu.pc = PPC_CODE_BASE + 0x2000;
        worker_cpu.gpr[1] = PPC_DATA_BASE + 0x4000;
        let worker = loaded.guest_calls()
            .create_native_thread(
                NativeThreadContext {
                    context: worker_cpu.capture_execution_context(),
                },
                ThreadStorage::default(),
                false,
                |_| true,
            )
            .unwrap();
        loaded.guest_calls().begin_critical();
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = worker.thread_id();
        loaded.cpu.gpr[20] = 0x1234_5678;
        loaded.cpu.fpr[20] = 0x4009_21fb_5444_2d18;
        loaded.cpu.cr = 0x1357_2468;
        establish_loaded_reservation(&mut loaded, PPC_DATA_BASE);
        let before_calls = loaded.guest_calls().clone();

        let probe = loaded.run_with_hle_imports(64);

        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(-619));
        assert_eq!(loaded.cpu.gpr[20], 0x1234_5678);
        assert_eq!(loaded.cpu.fpr[20], 0x4009_21fb_5444_2d18);
        assert_eq!(loaded.cpu.cr, 0x1357_2468);
        assert_eq!(loaded.cpu.reservation_address(), Some(PPC_DATA_BASE));
        assert_eq!(loaded.guest_calls(), &before_calls);
        assert_eq!(
            loaded.guest_calls().current_task(),
            ExecutionTaskId::APPLICATION
        );
        assert_eq!(loaded.guest_calls().critical_depth(), 1);
    }

    let mut loaded = load_pef_application(&synthetic_pef_with_import(b"YieldToAnyThread")).unwrap();
    loaded.cpu.gpr[20] = 0x8765_4321;
    loaded.cpu.fpr[20] = 0x3ff0_0000_0000_0000;
    loaded.cpu.cr = 0x2468_1357;
    establish_loaded_reservation(&mut loaded, PPC_DATA_BASE);
    let before_calls = loaded.guest_calls().clone();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 0);
    assert_eq!(loaded.cpu.gpr[20], 0x8765_4321);
    assert_eq!(loaded.cpu.fpr[20], 0x3ff0_0000_0000_0000);
    assert_eq!(loaded.cpu.cr, 0x2468_1357);
    assert_eq!(loaded.cpu.reservation_address(), Some(PPC_DATA_BASE));
    assert_eq!(loaded.guest_calls(), &before_calls);
    assert_eq!(
        loaded.guest_calls().current_task(),
        ExecutionTaskId::APPLICATION
    );
}

#[test]
fn standalone_native_thread_stays_stopped_until_ready() {
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"SetThreadStateEndCritical")).unwrap();
    loaded.guest_calls().begin_critical();
    loaded.cpu.gpr[3] = 1;
    loaded.cpu.gpr[4] = 1;
    loaded.cpu.gpr[5] = 0;
    assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
    assert!(!loaded.guest_calls().current_task_is_running());
    let pc = loaded.cpu.pc;
    let time = loaded.cpu.time_base();
    for _ in 0..3 {
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.result, PpcRunResult::CycleLimit { cycles: 0 });
        assert_eq!(probe.handled_import_count, 0);
        assert_eq!(loaded.cpu.pc, pc);
        assert_eq!(loaded.cpu.time_base(), time);
    }
    let manager = crate::thread_manager::ThreadManager::new(loaded.guest_calls());
    assert_eq!(manager.ready_given_task(manager.task_reference(), 2), 0);
    loaded.run_with_hle_imports(64);
    assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn native_thread_task_reference_routes_share_state_and_validate_requests() {
    use crate::guest_call::ExecutionTaskId;
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"SetThreadReadyGivenTaskRef"))
            .unwrap();
    let worker = ExecutionTaskId::from_thread_id(3);
    assert!(loaded.guest_calls().register_task(worker));
    for (reference, thread, result) in [
        (99, 3, -619),
        (2, 99, -618),
        (2, 3, 0),
        (2, 3, -619),
        (2, 2, -619),
    ] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = reference;
        loaded.cpu.gpr[4] = thread;
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(result));
        assert_eq!(
            loaded.guest_calls().current_task(),
            ExecutionTaskId::APPLICATION
        );
    }
    let mut query =
        load_pef_application(&synthetic_pef_with_import(b"GetThreadStateGivenTaskRef"))
            .unwrap();
    query.toolbox_startup.execution =
        ExecutionMenuViews::shared_from(loaded.guest_calls());
    let out = PPC_DATA_BASE + 0x1000;
    query.memory.add_region(out, vec![0xaa; 4]);
    for (reference, thread, result, state) in
        [(2, 3, 0, 0), (99, 3, -619, 0xaaaa), (2, 99, -618, 0xaaaa)]
    {
        query.cpu.pc = query.entry_pc;
        query.cpu.lr = PPC_HALT_PC;
        query.cpu.gpr[3] = reference;
        query.cpu.gpr[4] = thread;
        query.cpu.gpr[5] = out;
        query.memory.write_u32_be(out, 0xaaaaaaaa).unwrap();
        assert_eq!(query.run_with_hle_imports(64).handled_import_count, 1);
        assert_eq!(query.cpu.gpr[3], ppc_i16_result(result));
        assert_eq!(query.memory.read_u16_be(out), Some(state));
        assert_eq!(query.memory.read_u16_be(out + 2), Some(0xaaaa));
    }
    let mut reference =
        load_pef_application(&synthetic_pef_with_import(b"GetThreadCurrentTaskRef")).unwrap();
    reference.memory.add_region(out, vec![0; 4]);
    reference.cpu.gpr[3] = out;
    assert_eq!(reference.run_with_hle_imports(64).handled_import_count, 1);
    assert_eq!(reference.memory.read_u32_be(out), Some(2));
}

#[test]
fn native_thread_state_stop_wake_and_resume_preserves_the_caller() {
    use crate::execution_kernel::ExecutionTaskState;
    use crate::guest_call::{ExecutionTaskId, NativeThreadContext};
    for end_critical in [false, true] {
        let symbol = if end_critical {
            b"SetThreadStateEndCritical".as_slice()
        } else {
            b"SetThreadState".as_slice()
        };
        let mut loaded = load_pef_application(&synthetic_pef_with_import(symbol)).unwrap();
        let code = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(
            code,
            [0x3a800055_u32, 0x4e800020]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        let mut worker_cpu = loaded.cpu.clone();
        worker_cpu.pc = code;
        worker_cpu.lr = PPC_HALT_PC;
        worker_cpu.gpr[20] = 0;
        let worker = loaded.guest_calls()
            .create_native_thread(
                NativeThreadContext {
                    context: worker_cpu.capture_execution_context(),
                },
                crate::guest_call::ThreadStorage {
                    result_destination: 0,
                    stack_base: 0,
                    stack_limit: 0,
                    managed_pointer: true,
                },
                false,
                |_| true,
            )
            .unwrap();
        loaded.cpu.gpr[20] = 0x12345678;
        loaded.cpu.gpr[3] = 1;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = worker.thread_id();
        loaded.cpu.lr = PPC_HALT_PC;
        if end_critical {
            loaded.guest_calls().begin_critical();
        }
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
        assert_eq!(loaded.guest_calls().current_task(), worker);
        assert_eq!(
            loaded.guest_calls()
                .scheduling_state(ExecutionTaskId::APPLICATION),
            Some(ExecutionTaskState::Stopped)
        );
        assert_eq!(loaded.guest_calls().critical_depth(), 0);
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.cpu.gpr[20], 0x55);
        // Wake the creator without switching away from the worker.
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = ExecutionTaskId::APPLICATION.thread_id();
        loaded.cpu.gpr[4] = 0;
        loaded.cpu.gpr[5] = 0;
        if end_critical {
            loaded.guest_calls().begin_critical();
        }
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
        assert_eq!(loaded.guest_calls().current_task(), worker);
        assert_eq!(
            loaded.guest_calls()
                .scheduling_state(ExecutionTaskId::APPLICATION),
            Some(ExecutionTaskState::Ready)
        );
        // Stopping the worker resumes the creator's successful ABI return.
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = 1;
        loaded.cpu.gpr[4] = 1;
        loaded.cpu.gpr[5] = ExecutionTaskId::APPLICATION.thread_id();
        if end_critical {
            loaded.guest_calls().begin_critical();
        }
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
        assert_eq!(
            loaded.guest_calls().current_task(),
            ExecutionTaskId::APPLICATION
        );
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.cpu.gpr[20], 0x12345678);
        assert_eq!(loaded.cpu.pc, loaded.entry_pc + 16);
        loaded.run_with_hle_imports(64);
        assert_eq!(loaded.cpu.pc, PPC_HALT_PC);
        assert_eq!(
            loaded.guest_calls().scheduling_state(worker),
            Some(ExecutionTaskState::Stopped)
        );
    }
}

#[test]
fn native_thread_state_refusal_preserves_critical_depth_and_contexts() {
    use crate::execution_kernel::ExecutionTaskState;
    use crate::guest_call::ExecutionTaskId;
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"SetThreadStateEndCritical")).unwrap();
    let worker = ExecutionTaskId::from_thread_id(3);
    assert!(loaded.guest_calls().register_task(worker));
    assert!(loaded.guest_calls()
        .set_scheduling_state(worker, ExecutionTaskState::Ready));
    loaded.guest_calls().begin_critical();
    loaded.cpu.gpr[20] = 0x1234_5678;
    loaded.cpu.fpr[20] = 0x4009_21fb_5444_2d18;
    loaded.cpu.cr = 0x1357_2468;
    loaded.cpu.ctr = 0x2468_1357;
    loaded.cpu.xer = 0x89ab_cdef;
    loaded.cpu.fpscr = 0x1020_3040;
    loaded.cpu.msr = 0x5060_7080;
    loaded.cpu.alignment_policy = PpcAlignmentPolicy::EmulateData;
    establish_loaded_reservation(&mut loaded, PPC_DATA_BASE);
    // The ready identity has no saved execution context. Do not partially
    // end critical or stop the caller when successor preparation refuses.
    for (thread, state, error) in [(1, 1, -619), (99, 0, -618), (1, 99, -619), (3, 2, -619)] {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = thread;
        loaded.cpu.gpr[4] = state;
        loaded.cpu.gpr[5] = 3;
        let before = loaded.guest_calls().clone();
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(error));
        assert_eq!(loaded.guest_calls(), &before);
        assert_eq!(loaded.guest_calls().critical_depth(), 1);
        assert_eq!(loaded.cpu.gpr[20], 0x1234_5678);
        assert_eq!(loaded.cpu.fpr[20], 0x4009_21fb_5444_2d18);
        assert_eq!(loaded.cpu.cr, 0x1357_2468);
        assert_eq!(loaded.cpu.xer, 0x89ab_cdef);
        assert_eq!(loaded.cpu.fpscr, 0x1020_3040);
        assert_eq!(loaded.cpu.msr, 0x5060_7080);
        assert_eq!(loaded.cpu.alignment_policy, PpcAlignmentPolicy::EmulateData);
        assert_eq!(loaded.cpu.reservation_address(), Some(PPC_DATA_BASE));
    }
}

#[test]
fn hle_import_runner_thread_queries_observe_classic_task_state() {
    use crate::cpu::{CpuOps, Register};
    use crate::guest_call::ExecutionTaskId;
    use crate::trap::test_helpers::{setup, TEST_SP};
    let (mut classic, mut cpu, mut bus) = setup();
    let worker = ExecutionTaskId::from_thread_id(3);
    assert!(classic.guest_calls.register_task(worker));
    // SetThreadState(worker, ready, kNoThreadID) at the classic ABI edge.
    bus.write_long(TEST_SP, 0);
    bus.write_word(TEST_SP + 4, 0);
    bus.write_long(TEST_SP + 6, worker.thread_id());
    cpu.write_reg(Register::D0, 0x0508);
    classic
        .dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    let mut loaded =
        load_pef_application(&synthetic_pef_with_import(b"GetThreadState")).unwrap();
    loaded.toolbox_startup.execution =
        ExecutionMenuViews::shared_from(&classic.guest_calls);
    let out = PPC_DATA_BASE + 0x1000;
    loaded.memory.add_region(out, vec![0xaa; 4]);
    for (thread, expected_result, expected_state) in [(3, 0, 0), (1, 0, 2), (99, -618, 0xaaaa)]
    {
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = thread;
        loaded.cpu.gpr[4] = out;
        loaded.memory.write_u32_be(out, 0xaaaa_aaaa).unwrap();
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(expected_result));
        assert_eq!(loaded.memory.read_u16_be(out), Some(expected_state));
        assert_eq!(loaded.memory.read_u16_be(out + 2), Some(0xaaaa));
    }
    // The classic unknown-thread route returns the same error and leaves output intact.
    cpu.write_reg(Register::A7, TEST_SP);
    cpu.write_reg(Register::D0, 0x0407);
    bus.write_long(TEST_SP, TEST_SP + 0x100);
    bus.write_long(TEST_SP + 4, 99);
    bus.write_word(TEST_SP + 0x100, 0xaaaa);
    classic
        .dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0) as i16, -618);
    assert_eq!(bus.read_word(TEST_SP + 0x100), 0xaaaa);
}

#[test]
fn hle_import_runner_thread_outputs_reject_partial_mappings() {
    for symbol in [
        b"GetCurrentThread".as_slice(),
        b"MacGetCurrentThread".as_slice(),
        b"GetThreadState".as_slice(),
    ] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(symbol)).unwrap();
        let out = PPC_DATA_BASE + 0x1000;
        loaded.memory.add_region(out, vec![0xaa]);
        let state_query = symbol == b"GetThreadState";
        loaded.cpu.gpr[3] = if state_query { 1 } else { out };
        loaded.cpu.gpr[4] = out;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.memory.read_u8(out), Some(0xaa));

        loaded.memory.add_region(out, vec![0xaa; 4]);
        loaded.cpu.pc = loaded.entry_pc;
        loaded.cpu.lr = PPC_HALT_PC;
        loaded.cpu.gpr[3] = if state_query { 1 } else { out };
        loaded.cpu.gpr[4] = out;
        assert_eq!(loaded.run_with_hle_imports(64).handled_import_count, 1);
        assert_eq!(loaded.cpu.gpr[3], 0);
        if state_query {
            assert_eq!(loaded.memory.read_u32_be(out), Some(0x0002_aaaa));
        } else {
            assert_eq!(loaded.memory.read_u32_be(out), Some(2));
        }
    }
}

#[test]
fn hle_import_runner_thread_critical_sections_cross_both_abi_edges() {
    use crate::cpu::{CpuOps, Register};
    use crate::trap::test_helpers::{setup, TEST_SP};
    let (mut classic, mut cpu, mut bus) = setup();
    cpu.write_reg(Register::D0, 0x000b);
    classic
        .dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(classic.guest_calls.critical_depth(), 1);
    let mut end =
        load_pef_application(&synthetic_pef_with_import(b"ThreadEndCritical")).unwrap();
    end.toolbox_startup.execution = ExecutionMenuViews::shared_from(&classic.guest_calls);
    assert_eq!(end.run_with_hle_imports(64).handled_import_count, 1);
    assert_eq!(end.cpu.gpr[3], 0);
    assert_eq!(classic.guest_calls.critical_depth(), 0);
    end.cpu.pc = end.entry_pc;
    end.cpu.lr = PPC_HALT_PC;
    assert_eq!(end.run_with_hle_imports(64).handled_import_count, 1);
    assert_eq!(end.cpu.gpr[3], ppc_i16_result(-619));
    assert_eq!(classic.guest_calls.critical_depth(), 0);

    let mut begin =
        load_pef_application(&synthetic_pef_with_import(b"ThreadBeginCritical")).unwrap();
    begin.toolbox_startup.execution = ExecutionMenuViews::shared_from(&classic.guest_calls);
    assert_eq!(begin.run_with_hle_imports(64).handled_import_count, 1);
    assert_eq!(begin.cpu.gpr[3], 0);
    assert_eq!(classic.guest_calls.critical_depth(), 1);
    cpu.write_reg(Register::D0, 0x000c);
    cpu.write_reg(Register::A7, TEST_SP);
    classic
        .dispatch_toolbox(true, 0x3f2, &mut cpu, &mut bus)
        .unwrap()
        .unwrap();
    assert_eq!(cpu.read_reg(Register::D0), 0);
    assert_eq!(classic.guest_calls.critical_depth(), 0);
}
