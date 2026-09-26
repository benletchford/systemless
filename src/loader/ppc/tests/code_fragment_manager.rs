use super::*;

#[test]
fn cfm_initializer_storage_reuses_only_completed_or_refused_allocations() {
    let calls = SharedGuestCallStack::default();
    let owner = PpcProcessMemoryManager::with_heap(PPC_HEAP_BASE, PPC_HEAP_BASE + 0x1000);
    let mut manager = owner.0.borrow_mut();
    let mut memory = PpcSectionMem::new();
    memory.add_region(0x1000, 0x4e80_0020u32.to_be_bytes().to_vec());
    memory.add_region(
        0x2000,
        [0x1000u32, 0x2100]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    memory.add_region(0x8000, vec![0; 128]);
    let mut cursor = PPC_HEAP_BASE;
    let mut allocate = |manager: &mut ProcessNativeMemoryManager, memory: &mut PpcSectionMem| {
        ppc_create_mem_fragment_init_block(
            Some(manager),
            memory,
            &mut cursor,
            PPC_HEAP_BASE + 0x1000,
            1,
            0x6000,
            123,
            "nested",
        )
        .unwrap()
    };
    let first = allocate(&mut manager, &mut memory);
    let mut cpu = PpcCpu::new();
    cpu.gpr[1] = 0x8000;
    cpu.lr = 0x4000;
    assert_eq!(
        ppc_activate_cfm_initializer(
            &mut cpu,
            &mut memory,
            &calls,
            &mut manager,
            0x2000,
            first,
            None,
        ),
        PpcImportAction::Continue
    );
    let second = allocate(&mut manager, &mut memory);
    assert_ne!(first, second);
    cpu.lr = 0x1000;
    assert_eq!(
        ppc_activate_cfm_initializer(
            &mut cpu,
            &mut memory,
            &calls,
            &mut manager,
            0x2000,
            second,
            None,
        ),
        PpcImportAction::Continue
    );
    cpu.pc = PPC_GUEST_CALL_RETURN_PC;
    assert!(calls.complete_powerpc_releasing_scratch(&mut cpu, &mut manager));
    assert_eq!(
        manager
            .native_ptr_records()
            .iter()
            .map(|p| p.ptr)
            .collect::<Vec<_>>(),
        vec![first]
    );
    let third = allocate(&mut manager, &mut memory);
    assert_eq!(third, second, "the live outer block must not be reused");
    let outer_name = first + PPC_CFM_INIT_BLOCK_SIZE;
    assert_eq!(
        ppc_read_pstring_bytes(&mut memory, outer_name),
        Some(b"nested".to_vec())
    );
    cpu.gpr[1] = u32::MAX;
    let before = calls.clone();
    assert_eq!(
        ppc_activate_cfm_initializer(
            &mut cpu,
            &mut memory,
            &calls,
            &mut manager,
            0x2000,
            third,
            None,
        ),
        PpcImportAction::Return(ppc_i16_result(PPC_FRAG_CORRUPT_ERR))
    );
    assert_eq!(calls, before);
    assert_eq!(
        manager
            .native_ptr_records()
            .iter()
            .map(|p| p.ptr)
            .collect::<Vec<_>>(),
        vec![first]
    );
    let fourth = allocate(&mut manager, &mut memory);
    assert_eq!(fourth, second);
    assert_eq!(
        ppc_activate_cfm_initializer(
            &mut cpu,
            &mut memory,
            &calls,
            &mut manager,
            u32::MAX - 3,
            fourth,
            None,
        ),
        PpcImportAction::Return(ppc_i16_result(PPC_FRAG_CORRUPT_ERR))
    );
    cpu.pc = PPC_GUEST_CALL_RETURN_PC;
    assert!(calls.complete_powerpc_releasing_scratch(&mut cpu, &mut manager));
    assert_eq!(cpu.pc, 0x4000);
    assert!(manager.native_ptr_records().is_empty());
    assert_eq!(manager.native_free_ptr_blocks().len(), 2);
}

#[test]
fn cfm_load_resumes_after_initialization_and_failed_loads_can_retry() {
    use crate::execution_kernel::ExecutionTaskState;
    for (use_memory, failed) in [(false, false), (false, true), (true, false), (true, true)] {
        let calls = SharedGuestCallStack::default();
        let worker = calls.create_task().unwrap();
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(calls.switch_to_task(worker));
        let owner = PpcProcessMemoryManager::with_heap(PPC_HEAP_BASE, PPC_HEAP_BASE + 0x10000);
        let mut manager = owner.0.borrow_mut();
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_HEAP_BASE, vec![0; 0x10000]);
        memory.add_region(0x5000, b"\x04test".to_vec());
        memory.add_region(0x6000, vec![0xa5; 64]);
        memory.add_region(0x8000, vec![0; 128]);
        let fragment = synthetic_pef_with_initializer();
        memory.add_region(0x9000, fragment.clone());
        let mut libraries = vec![PpcCfmLibraryFragment {
            name: "test".to_string(),
            bytes: synthetic_pef_with_initializer(),
        }];
        let mut connections = Vec::new();
        let mut next_connection = PPC_FIRST_CFM_CONNECTION_ID;
        let mut import_run_state =
            PpcImportRunState::from_parts(Vec::new(), 0, ppc_import_layout());
        let mut cursor = PPC_HEAP_BASE;
        let mut cpu = PpcCpu::new();
        cpu.gpr[1] = 0x8000;
        cpu.gpr[2] = 0x2200;
        cpu.lr = 0x4000;
        cpu.gpr[3] = 0x5000;
        cpu.gpr[4] = PPC_CFM_POWERPC_ARCH;
        cpu.gpr[5] = PPC_CFM_LOAD_LIB;
        cpu.gpr[6] = 0x6000;
        cpu.gpr[7] = 0x6004;
        cpu.gpr[8] = 0x6008;
        if use_memory {
            cpu.gpr[3] = 0x9000;
            cpu.gpr[4] = fragment.len() as u32;
            cpu.gpr[5] = 0x5000;
            cpu.gpr[6] = PPC_CFM_LOAD_LIB;
            cpu.gpr[7] = 0x6000;
            cpu.gpr[8] = 0x6004;
            cpu.gpr[9] = 0x6008;
        }
        let request = cpu.clone();
        macro_rules! load {
            ($cpu:expr) => {
                if use_memory {
                    ppc_get_mem_fragment(
                        $cpu,
                        &calls,
                        &mut manager,
                        &mut memory,
                        &mut cursor,
                        PPC_HEAP_BASE + 0x10000,
                        &mut connections,
                        &mut next_connection,
                        &mut import_run_state,
                    )
                } else {
                    ppc_get_shared_library(
                        $cpu,
                        &calls,
                        &mut manager,
                        &mut memory,
                        &mut cursor,
                        PPC_HEAP_BASE + 0x10000,
                        &mut connections,
                        &mut libraries,
                        &mut next_connection,
                        &mut import_run_state,
                    )
                }
            };
        }
        assert_eq!(load!(&mut cpu), PpcImportAction::Continue);
        let id = connections[0].id;
        assert!(calls.is_cfm_load_pending(CfmLoadId(id)));
        assert_eq!(import_run_state.total_count(), 1);
        assert_eq!(
            import_run_state.binding_cloned(0).unwrap().symbol_name,
            "TestImport"
        );
        assert_eq!(memory.read_u32_be(0x6000), Some(0xa5a5_a5a5));
        assert_eq!(memory.read_u32_be(0x6004), Some(0xa5a5_a5a5));
        let mut recursive = request.clone();
        assert_eq!(
            load!(&mut recursive),
            PpcImportAction::Return(ppc_i16_result(PPC_FRAG_INIT_LOOP))
        );
        assert_eq!(import_run_state.total_count(), 1);
        assert_eq!(connections.len(), 1);
        assert_eq!(manager.native_ptr_records().len(), 1);
        memory.add_region(0x5100, b"\x05inner".to_vec());
        libraries.push(PpcCfmLibraryFragment {
            name: "inner".to_string(),
            bytes: fragment.clone(),
        });
        let mut inner = request.clone();
        inner.gpr[if use_memory { 5 } else { 3 }] = 0x5100;
        assert_eq!(load!(&mut inner), PpcImportAction::Continue);
        let inner_id = connections[1].id;
        assert_ne!(inner_id, id);
        assert!(calls.is_cfm_load_pending(CfmLoadId(inner_id)));
        assert_eq!(import_run_state.total_count(), 2);
        assert_eq!(
            import_run_state.binding_cloned(0).unwrap().symbol_name,
            "TestImport"
        );
        assert_eq!(
            import_run_state.binding_cloned(1).unwrap().symbol_name,
            "TestImport"
        );
        inner.pc = PPC_GUEST_CALL_RETURN_PC;
        inner.gpr[3] = 1;
        assert!(calls.complete_powerpc_resuming_load(
            &mut inner,
            &mut manager,
            |operation, result| ppc_complete_cfm_load(
                operation,
                result,
                &mut memory,
                &mut connections
            )
        ));
        assert!(!calls.is_cfm_load_pending(CfmLoadId(inner_id)));
        assert!(calls.is_cfm_load_pending(CfmLoadId(id)));
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].id, id);
        assert_eq!(manager.native_ptr_records().len(), 1);
        // Give the prepared test fragment an initializer returning the chosen OSErr.
        memory
            .write_u32_be(cpu.pc, 0x3860_0000 | u32::from(failed))
            .unwrap();
        memory.write_u32_be(cpu.pc + 4, 0x4e80_0020).unwrap();
        assert_eq!(
            cpu.run_with_imports(&mut memory, 2, 0, 0xff00_0000, 0, |_, _, _| {
                PpcImportAction::Halt
            }),
            PpcRunResult::CycleLimit { cycles: 2 }
        );
        assert_eq!(cpu.pc, PPC_GUEST_CALL_RETURN_PC);
        let before = calls.clone();
        assert!(!calls.complete_powerpc_releasing_scratch(&mut cpu, &mut manager));
        assert_eq!(
            calls, before,
            "the manager operation cannot be discarded by a plain return"
        );
        assert!(calls.complete_powerpc_resuming_load(
            &mut cpu,
            &mut manager,
            |operation, result| ppc_complete_cfm_load(
                operation,
                result,
                &mut memory,
                &mut connections
            )
        ));
        assert_eq!(import_run_state.total_count(), 2);
        assert_eq!((cpu.pc, cpu.gpr[2]), (0x4000, 0x2200));
        assert!(!calls.is_cfm_load_pending(CfmLoadId(id)));
        assert!(manager.native_ptr_records().is_empty());
        if failed {
            assert_eq!(cpu.gpr[3], ppc_i16_result(PPC_FRAG_USER_INIT_PROC_ERR));
            assert!(connections.is_empty());
            assert_eq!(memory.read_u32_be(0x6000), Some(0xa5a5_a5a5));
            cpu = request.clone();
            assert_eq!(load!(&mut cpu), PpcImportAction::Continue);
            assert_eq!(import_run_state.total_count(), 3);
            assert!(connections[0].id > id);
            cpu.pc = PPC_GUEST_CALL_RETURN_PC;
            cpu.gpr[3] = 1;
            assert!(calls.complete_powerpc_resuming_load(
                &mut cpu,
                &mut manager,
                |operation, result| ppc_complete_cfm_load(
                    operation,
                    result,
                    &mut memory,
                    &mut connections
                )
            ));
            assert!(connections.is_empty());
            assert_eq!(import_run_state.total_count(), 3);
        } else {
            assert_eq!(cpu.gpr[3], 0);
            assert_eq!(import_run_state.total_count(), 2);
            assert!(import_run_state.binding_cloned(0).is_some());
            assert!(import_run_state.binding_cloned(1).is_some());
            assert_eq!(memory.read_u32_be(0x6000), Some(id));
            assert_eq!(memory.read_u32_be(0x6004), Some(connections[0].main_addr));
            cpu = request.clone();
            assert_eq!(load!(&mut cpu), PpcImportAction::Return(0));
            assert_eq!(connections.len(), 1);
            cpu.gpr[if use_memory { 7 } else { 6 }] = 0x7000;
            assert_eq!(
                load!(&mut cpu),
                PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR))
            );
            assert_eq!(
                connections[0].id, id,
                "bad outputs must not invalidate a reused ready connection"
            );
        }
        assert!(calls.is_empty());
        assert!(manager.native_ptr_records().is_empty());
    }
}

#[test]
fn cfm_initializer_storage_is_released_when_load_outputs_become_readonly() {
    for invalid_register in [6, 7, 8] {
        let calls = SharedGuestCallStack::default();
        let owner = PpcProcessMemoryManager::with_heap(PPC_HEAP_BASE, PPC_HEAP_BASE + 0x10000);
        let mut manager = owner.0.borrow_mut();
        let mut memory = PpcSectionMem::new();
        memory.add_region(PPC_HEAP_BASE, vec![0; 0x10000]);
        let fragment = synthetic_pef_with_initializer();
        let mut cpu = PpcCpu::new();
        cpu.gpr[1] = 0x8000;
        memory.add_region(0x8000, vec![0; 128]);
        cpu.gpr[3] = 0x5000;
        cpu.gpr[4] = PPC_CFM_POWERPC_ARCH;
        cpu.gpr[5] = PPC_CFM_LOAD_LIB;
        cpu.gpr[6] = 0x6000;
        cpu.gpr[7] = 0x6004;
        cpu.gpr[8] = 0x6008;
        cpu.gpr[invalid_register] = 0x7000;
        memory.add_region(0x5000, b"\x06nested".to_vec());
        let mut libraries = vec![PpcCfmLibraryFragment {
            name: "nested".to_string(),
            bytes: fragment,
        }];
        memory.add_region(0x6000, vec![0; 64]);
        memory.add_region(0x7000, vec![0; 4]);
        let mut cursor = PPC_HEAP_BASE;
        let mut connections = Vec::new();
        let mut next_connection = PPC_FIRST_CFM_CONNECTION_ID;
        let mut import_run_state =
            PpcImportRunState::from_parts(Vec::new(), 0, ppc_import_layout());
        assert_eq!(
            ppc_get_shared_library(
                &mut cpu,
                &calls,
                &mut manager,
                &mut memory,
                &mut cursor,
                PPC_HEAP_BASE + 0x10000,
                &mut connections,
                &mut libraries,
                &mut next_connection,
                &mut import_run_state
            ),
            PpcImportAction::Continue
        );
        assert_eq!(
            connections.len(),
            1,
            "reached output publication after preparing the initializer"
        );
        assert!(calls.is_cfm_load_pending(CfmLoadId(connections[0].id)));
        assert_eq!(memory.read_u32_be(0x6000), Some(0));
        memory.add_readonly_region(0x7000, vec![0; 4]);
        cpu.pc = PPC_GUEST_CALL_RETURN_PC;
        cpu.gpr[3] = 0;
        assert!(calls.complete_powerpc_resuming_load(
            &mut cpu,
            &mut manager,
            |operation, result| ppc_complete_cfm_load(
                operation,
                result,
                &mut memory,
                &mut connections
            )
        ));
        assert_eq!(cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert!(connections.is_empty());
        assert!(manager.native_ptr_records().is_empty());
        assert_eq!(manager.native_free_ptr_blocks().len(), 1);
        assert!(calls.is_empty());
    }
}

#[test]
fn cfm_initializer_storage_allocation_failure_leaves_no_partial_block() {
    let owner = PpcProcessMemoryManager::with_heap(PPC_HEAP_BASE, PPC_HEAP_BASE + 64);
    let mut manager = owner.0.borrow_mut();
    manager.set_native_mem_error(-42);
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_HEAP_BASE, vec![0xa5; 64]);
    let mut cursor = PPC_HEAP_BASE;
    assert_eq!(
        ppc_create_mem_fragment_init_block(
            Some(&mut manager),
            &mut memory,
            &mut cursor,
            PPC_HEAP_BASE + 64,
            1,
            0x6000,
            123,
            &"x".repeat(40)
        ),
        Err(PPC_FRAG_NO_MEM)
    );
    assert_eq!(cursor, PPC_HEAP_BASE);
    assert_eq!(manager.native_heap_state().unwrap().last_mem_error, -42);
    assert_eq!(
        manager.native_heap_state().unwrap().heap_cursor,
        PPC_HEAP_BASE
    );
    assert!(manager.native_ptr_records().is_empty());
    assert!((0..64).all(|offset| memory.read_u8(PPC_HEAP_BASE + offset) == Some(0xa5)));
}

#[test]
fn cfm_initializer_effect_executes_and_returns_to_its_worker() {
    use crate::execution_kernel::ExecutionTaskState;
    for result in [0u16, 0xffff] {
        let calls = SharedGuestCallStack::default();
        let worker = calls.create_task().unwrap();
        assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
        assert!(calls.switch_to_task(worker));
        let mut memory = PpcSectionMem::new();
        memory.add_region(
            0x1000,
            [0x3860_0000 | u32::from(result), 0x4e80_0020]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        memory.add_region(
            0x2000,
            [0x1000u32, 0x2100]
                .into_iter()
                .flat_map(u32::to_be_bytes)
                .collect(),
        );
        memory.add_region(0x8000, vec![0xa5; 128]);
        let owner = PpcProcessMemoryManager::with_heap(PPC_HEAP_BASE, PPC_HEAP_BASE + 0x1000);
        let mut manager = owner.0.borrow_mut();
        let mut cursor = PPC_HEAP_BASE;
        let init_block = ppc_create_mem_fragment_init_block(
            Some(&mut manager),
            &mut memory,
            &mut cursor,
            PPC_HEAP_BASE + 0x1000,
            17,
            0x6000,
            123,
            "worker initializer",
        )
        .unwrap();
        assert_eq!(manager.native_ptr_records().len(), 1);
        assert_eq!(
            memory.read_u32_be(init_block + 28),
            Some(init_block + PPC_CFM_INIT_BLOCK_SIZE)
        );
        assert_eq!(memory.read_u32_be(init_block + 8), Some(17));
        let mut cpu = PpcCpu::new();
        cpu.gpr[1] = 0x8000;
        cpu.gpr[2] = 0x2200;
        cpu.lr = 0x4000;
        assert_eq!(
            ppc_activate_cfm_initializer(
                &mut cpu,
                &mut memory,
                &calls,
                &mut manager,
                0x2000,
                init_block,
                None,
            ),
            PpcImportAction::Continue
        );
        assert_eq!((cpu.gpr[3], cpu.gpr[12]), (init_block, 0x2000));
        assert_eq!(calls.current_task(), worker);
        assert_eq!(calls.task_depth(worker), 1);
        assert_eq!(
            cpu.run_with_imports(&mut memory, 2, 0, 0xff00_0000, 0, |_, _, _| {
                PpcImportAction::Halt
            }),
            PpcRunResult::CycleLimit { cycles: 2 }
        );
        assert_eq!(cpu.pc, PPC_GUEST_CALL_RETURN_PC);
        let before = calls.clone();
        assert!(!calls.complete_powerpc(&mut cpu));
        assert_eq!(
            calls, before,
            "a return without the allocator must retain its storage"
        );
        manager.set_native_mem_error(-108);
        assert!(calls.complete_powerpc_releasing_scratch(&mut cpu, &mut manager));
        assert!(manager.native_ptr_records().is_empty());
        assert_eq!(manager.native_free_ptr_blocks().len(), 1);
        assert_eq!(manager.native_heap_state().unwrap().last_mem_error, -108);
        assert!(!calls.complete_powerpc_releasing_scratch(&mut cpu, &mut manager));
        assert_eq!(manager.native_free_ptr_blocks().len(), 1);
        assert_eq!((cpu.pc, cpu.lr, cpu.gpr[2]), (0x4000, 0x4000, 0x2200));
        assert_eq!(
            cpu.gpr[3],
            ppc_i16_result(if result == 0 {
                PPC_NO_ERR
            } else {
                PPC_FRAG_USER_INIT_PROC_ERR
            })
        );
        assert!(calls.is_empty());
    }
}

#[test]
fn hle_import_runner_handles_get_shared_library_and_reuses_connection() {
    let pef = synthetic_pef_with_import(b"GetSharedLibrary");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    write_ppc_pstring(&mut loaded.memory, scratch, b"QuickDraw\xaa 3D");
    let conn_id_ptr = scratch + 32;
    let main_addr_ptr = scratch + 36;
    let err_name_ptr = scratch + 40;
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"pwpc");
    loaded.cpu.gpr[5] = 1; // kLoadLib
    loaded.cpu.gpr[6] = conn_id_ptr;
    loaded.cpu.gpr[7] = main_addr_ptr;
    loaded.cpu.gpr[8] = err_name_ptr;

    let first = loaded.run_with_hle_imports(64);

    assert_eq!(first.handled_import_count, 1);
    assert_eq!(first.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(conn_id_ptr),
        Some(PPC_FIRST_CFM_CONNECTION_ID)
    );
    assert_eq!(
        loaded.memory.read_u32_be(main_addr_ptr),
        Some(PPC_CFM_MAIN_STUB_BASE)
    );
    assert_eq!(loaded.memory.read_u8(err_name_ptr), Some(0));
    assert_eq!(loaded.cfm.as_ref().unwrap().connections.len(), 1);

    loaded.cpu.pc = loaded.entry_pc;
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.memory.write_u32_be(conn_id_ptr, 0).unwrap();
    loaded.memory.write_u32_be(main_addr_ptr, 0).unwrap();
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = u32::from_be_bytes(*b"pwpc");
    loaded.cpu.gpr[5] = 2; // kFindLib
    loaded.cpu.gpr[6] = conn_id_ptr;
    loaded.cpu.gpr[7] = main_addr_ptr;
    loaded.cpu.gpr[8] = err_name_ptr;

    let second = loaded.run_with_hle_imports(64);

    assert_eq!(second.handled_import_count, 1);
    assert_eq!(second.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(conn_id_ptr),
        Some(PPC_FIRST_CFM_CONNECTION_ID)
    );
    assert_eq!(
        loaded.memory.read_u32_be(main_addr_ptr),
        Some(PPC_CFM_MAIN_STUB_BASE)
    );
    assert_eq!(loaded.cfm.as_ref().unwrap().connections.len(), 1);
}

#[test]
fn cfm_symbol_enumeration_imports_route_aliases_and_preserve_failed_outputs() {
    for library in [
        "InterfaceLib",
        "CodeFragmentMgr",
        "CarbonCore.vlib",
        "CFMPriv_CarbonCore",
    ] {
        for count in [false, true] {
            let name = if count {
                "CountSymbols"
            } else {
                "GetIndSymbol"
            };
            for fault in 0..4 {
                let mut loaded = load_pef_application(&synthetic_pef_with_library_import(
                    library.as_bytes(),
                    name.as_bytes(),
                ))
                .unwrap();
                const OUTPUT: u32 = PPC_HEAP_BASE + 0x100;
                loaded.memory.add_region(OUTPUT, vec![0xa5; 64]);
                loaded
                    .cfm
                    .as_mut()
                    .unwrap()
                    .connections
                    .push(PpcCfmConnection {
                        id: 7,
                        library_name: "fixture".into(),
                        main_addr: 0,
                        init_addr: 0,
                        term_addr: 0,
                        exports: vec![PpcCfmExport {
                            name: "Café™".into(),
                            class: 2,
                            address: 0x1234_5678,
                        }],
                    });
                loaded.cpu.gpr[3] = if fault == 1 { 99 } else { 7 };
                loaded.cpu.gpr[4] = if count {
                    OUTPUT
                } else if fault == 2 {
                    2
                } else {
                    1
                };
                loaded.cpu.gpr[5] = OUTPUT;
                loaded.cpu.gpr[6] = OUTPUT + 32;
                loaded.cpu.gpr[7] = OUTPUT + 40;
                if fault == 3 {
                    loaded
                        .memory
                        .add_readonly_region(OUTPUT + if count { 1 } else { 40 }, vec![0xa5]);
                }
                let probe = loaded.run_with_hle_imports(128);
                assert_eq!(probe.handled_import_count, 1, "{library} {name}");
                assert_eq!(probe.unsupported_import_index, None);
                let error: i16 = match fault {
                    1 => -2801,
                    2 if !count => -2802,
                    3 => -50,
                    _ => 0,
                };
                assert_eq!(loaded.cpu.gpr[3], error as i32 as u32);
                if error != 0 {
                    assert!((0..64).all(|i| loaded.memory.read_u8(OUTPUT + i) == Some(0xa5)));
                } else if count {
                    assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(1));
                } else {
                    assert_eq!(loaded.memory.read_u32_be(OUTPUT + 32), Some(0x1234_5678));
                    assert_eq!(loaded.memory.read_u8(OUTPUT + 40), Some(2));
                }
            }
        }
    }
}

pub(crate) fn synthetic_pef_with_enumerable_exports() -> Vec<u8> {
    let name = b"Caf\x8e\xaa";
    let strings_offset = 56usize;
    let hash_offset = strings_offset + name.len() + 1;
    let key_offset = hash_offset + 4;
    let symbol_offset = key_offset + 4;
    let mut loader = vec![0; symbol_offset + 10];
    write_i32(&mut loader, 0, -1);
    write_i32(&mut loader, 8, -1);
    write_i32(&mut loader, 16, -1);
    write_u32(&mut loader, 40, strings_offset as u32);
    write_u32(&mut loader, 44, hash_offset as u32);
    write_u32(&mut loader, 52, 1);
    loader[strings_offset..strings_offset + name.len()].copy_from_slice(name);
    write_u32(&mut loader, hash_offset, 1 << 18);
    write_u32(&mut loader, key_offset, (name.len() as u32) << 16);
    write_u32(&mut loader, symbol_offset, 1 << 24); // data, string offset zero
    write_u32(&mut loader, symbol_offset + 4, 0x1234_5678);
    write_u16(&mut loader, symbol_offset + 8, (-2i16) as u16); // absolute export
    synthetic_pef_with_loader_and_data(loader, &[0; 8])
}

fn synthetic_pef_with_single_export(
    name: &[u8],
    class: u8,
    value: u32,
    section_index: i16,
) -> Vec<u8> {
    let strings_offset = 56usize;
    let hash_offset = strings_offset + name.len() + 1;
    let key_offset = hash_offset + 4;
    let symbol_offset = key_offset + 4;
    let mut loader = vec![0; symbol_offset + 10];
    write_i32(&mut loader, 0, -1);
    write_i32(&mut loader, 8, -1);
    write_i32(&mut loader, 16, -1);
    write_u32(&mut loader, 40, strings_offset as u32);
    write_u32(&mut loader, 44, hash_offset as u32);
    write_u32(&mut loader, 52, 1);
    loader[strings_offset..strings_offset + name.len()].copy_from_slice(name);
    write_u32(&mut loader, hash_offset, 1 << 18);
    write_u32(&mut loader, key_offset, (name.len() as u32) << 16);
    write_u32(&mut loader, symbol_offset, u32::from(class) << 24);
    write_u32(&mut loader, symbol_offset + 4, value);
    write_u16(&mut loader, symbol_offset + 8, section_index as u16);
    synthetic_pef_with_loader_and_data(loader, &[0; 8])
}

#[test]
fn initial_application_imports_bind_bundled_library_data_and_tvectors() {
    for (symbol, class, value, section_index) in [
        (b"SharedData".as_slice(), 1, 0x1234_5678, -2),
        (b"SharedRoutine".as_slice(), 2, 0, 1),
    ] {
        let application = synthetic_pef_with_loader(synthetic_loader_with_symbol_class(
            b"BundledLibrary",
            symbol,
            class,
            &[sm_index_reloc(0x30, 0)],
        ));
        let library = synthetic_pef_with_single_export(symbol, class, value, section_index);
        let mut loaded = load_pef_application_with_config_and_optional_system_reservation(
            &application,
            PpcLoadConfig::default(),
            None,
            vec![PpcCfmLibraryFragment {
                name: "BundledLibrary".to_string(),
                bytes: library,
            }],
        )
        .unwrap();

        let binding_address = loaded.import_binding(0).unwrap().address;
        assert_ne!(binding_address, 0);
        assert_eq!(
            loaded.memory.read_u32_be(PPC_DATA_BASE),
            Some(binding_address)
        );
        assert_eq!(
            loaded.cfm.as_ref().unwrap().connections[0]
                .exports
                .iter()
                .find(|export| export.name == decode_mac_roman(symbol))
                .map(|export| (export.class, export.address)),
            Some((class, binding_address))
        );
        if section_index == -2 {
            assert_eq!(binding_address, value);
        } else {
            assert!(binding_address >= PPC_HEAP_BASE);
        }
    }
}

#[test]
fn initial_bundled_libraries_bind_dependencies_in_deterministic_order() {
    let application = synthetic_pef_with_library_import(b"RootLibrary", b"Missing");
    let root = PpcCfmLibraryFragment {
        name: "RootLibrary".to_string(),
        bytes: synthetic_pef_with_library_import(b"DependencyLibrary", b"SharedRoutine"),
    };
    let dependency = PpcCfmLibraryFragment {
        name: "DependencyLibrary".to_string(),
        bytes: synthetic_pef_with_single_export(b"SharedRoutine", 2, 0, 1),
    };

    for fragments in [
        vec![root.clone(), dependency.clone()],
        vec![dependency.clone(), root.clone()],
    ] {
        let mut loaded = load_pef_application_with_config_and_optional_system_reservation(
            &application,
            PpcLoadConfig::default(),
            None,
            fragments,
        )
        .unwrap();
        let cfm = loaded.cfm.as_ref().unwrap();

        assert_eq!(
            cfm.connections
                .iter()
                .map(|connection| connection.library_name.as_str())
                .collect::<Vec<_>>(),
            ["DependencyLibrary", "RootLibrary"]
        );
        let export_address = cfm.connections[0]
            .exports
            .iter()
            .find(|export| export.name == "SharedRoutine")
            .unwrap()
            .address;
        let dependency_binding = loaded
            .imports
            .iter()
            .find(|binding| binding.library_name == "DependencyLibrary")
            .unwrap();
        assert_eq!(dependency_binding.address, export_address);
        assert_eq!(
            loaded.memory.read_u32_be(PPC_APPLICATION_ZONE + 12),
            Some(
                ppc_heap_free_capacity(
                    &loaded.memory,
                    loaded.heap_cursor(),
                    test_heap_limit!(loaded),
                )
                .0
            )
        );
    }
}

#[test]
fn bundled_gamesprockets_use_native_system_bindings() {
    for (library_name, symbol_name) in [
        ("DrawSprocketLib", "DSpStartup"),
        ("InputSprocketLib", "ISpGetVersion"),
    ] {
        let application =
            synthetic_pef_with_library_import(library_name.as_bytes(), symbol_name.as_bytes());
        let loaded = load_pef_application_with_config_and_optional_system_reservation(
            &application,
            PpcLoadConfig::default(),
            None,
            vec![PpcCfmLibraryFragment {
                name: library_name.to_string(),
                bytes: vec![0; 40],
            }],
        )
        .expect("native GameSprockets binding");
        assert!(loaded.imports.iter().any(|binding| {
            binding.library_name == library_name && binding.symbol_name == symbol_name
        }));
        assert!(!loaded
            .cfm
            .as_ref()
            .unwrap()
            .connections
            .iter()
            .any(|connection| connection.library_name == library_name));
    }
}

#[test]
fn initial_bundled_library_initializer_runs_before_application_main() {
    let application = synthetic_pef_with_library_import(b"BundledInitializer", b"Missing");
    let mut library = synthetic_pef_with_initializer();
    let code_offset = parse_pef_sections(&library).unwrap()[0].container_offset as usize;
    write_u32(&mut library, code_offset, d_form_u(14, 3, 0, 0));
    write_u32(&mut library, code_offset + 4, BLR);
    let mut loaded = load_pef_application_with_config_and_optional_system_reservation(
        &application,
        PpcLoadConfig::default(),
        None,
        vec![PpcCfmLibraryFragment {
            name: "BundledInitializer".to_string(),
            bytes: library,
        }],
    )
    .unwrap();

    assert_ne!(loaded.cpu.pc, loaded.entry_pc);
    assert_eq!(
        loaded.memory.system_code_isa(loaded.cpu.pc),
        Some(GuestIsa::PowerPc)
    );
    let probe = loaded.run_with_hle_imports(128);

    assert_eq!(probe.unsupported_import_index, Some(0));
    assert!(matches!(probe.result, PpcRunResult::Halted { .. }));
    assert_eq!(loaded.cpu.gpr[3], 0);
}

#[test]
fn cfm_reifies_real_pef_exports_with_section_absolute_and_reexported_addresses() {
    let fragment = synthetic_pef_with_exports();
    let mapped = vec![MappedSection {
        index: 1,
        section_kind: SECTION_KIND_UNPACKED_DATA,
        base: 0x0310_0000,
        bytes: vec![0; 8],
    }];
    let exports = crate::cfm::fragment::resolve_fragment_exports(
        &fragment,
        &mapped,
        &[0xcafe_babe, 0xdead_beef],
    )
    .unwrap();

    assert_eq!(
        exports,
        vec![
            PpcCfmExport {
                name: "tvector".to_string(),
                class: 2,
                address: 0x0310_0000,
            },
            PpcCfmExport {
                name: "absolute".to_string(),
                class: 1,
                address: 0x1234_5678,
            },
            PpcCfmExport {
                name: "reexport".to_string(),
                class: 2,
                address: 0xcafe_babe,
            },
        ]
    );
}

#[test]
fn get_shared_library_does_not_fabricate_unknown_connections() {
    let pef = synthetic_pef_with_import(b"GetSharedLibrary");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    write_ppc_pstring(&mut loaded.memory, scratch, b"MissingLibrary");
    let conn_id_ptr = scratch + 32;
    let main_addr_ptr = scratch + 36;
    let err_name_ptr = scratch + 40;
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = PPC_CFM_POWERPC_ARCH;
    loaded.cpu.gpr[5] = PPC_CFM_LOAD_LIB;
    loaded.cpu.gpr[6] = conn_id_ptr;
    loaded.cpu.gpr[7] = main_addr_ptr;
    loaded.cpu.gpr[8] = err_name_ptr;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FRAG_LIB_NOT_FOUND));
    assert!(loaded.cfm.as_ref().unwrap().connections.is_empty());
}

#[test]
fn get_shared_library_finds_statically_imported_hle_library_with_any_architecture() {
    let pef = synthetic_pef_with_import(b"GetSharedLibrary");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    write_ppc_pstring(&mut loaded.memory, scratch, b"InterfaceLib");
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = PPC_CFM_ANY_ARCH;
    loaded.cpu.gpr[5] = PPC_CFM_FIND_LIB;
    loaded.cpu.gpr[6] = scratch + 32;
    loaded.cpu.gpr[7] = scratch + 36;
    loaded.cpu.gpr[8] = scratch + 40;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(scratch + 32),
        Some(PPC_FIRST_CFM_CONNECTION_ID)
    );
    assert_eq!(loaded.cfm.as_ref().unwrap().connections.len(), 1);
}

#[test]
fn get_shared_library_requires_powerpc_architecture() {
    let pef = synthetic_pef_with_import(b"GetSharedLibrary");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    write_ppc_pstring(&mut loaded.memory, scratch, b"InterfaceLib");
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = 0;
    loaded.cpu.gpr[5] = PPC_CFM_LOAD_LIB;
    loaded.cpu.gpr[6] = scratch + 32;
    loaded.cpu.gpr[7] = scratch + 36;
    loaded.cpu.gpr[8] = scratch + 40;

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FRAG_ARCH_ERR));
    assert!(loaded.cfm.as_ref().unwrap().connections.is_empty());
}

#[test]
fn get_shared_library_find_only_does_not_prepare_seeded_fragment() {
    let pef = synthetic_pef_with_import(b"GetSharedLibrary");
    let mut loaded = load_pef_application(&pef).unwrap();
    loaded.seed_cfm_library_fragments(vec![PpcCfmLibraryFragment {
        name: "SeededLibrary".to_string(),
        bytes: synthetic_pef_with_exports(),
    }]);
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    write_ppc_pstring(&mut loaded.memory, scratch, b"SeededLibrary");
    loaded.cpu.gpr[3] = scratch;
    loaded.cpu.gpr[4] = PPC_CFM_POWERPC_ARCH;
    loaded.cpu.gpr[5] = PPC_CFM_FIND_LIB;
    loaded.cpu.gpr[6] = scratch + 32;
    loaded.cpu.gpr[7] = scratch + 36;
    loaded.cpu.gpr[8] = scratch + 40;
    let heap_before = loaded.heap_cursor();

    let probe = loaded.run_with_hle_imports(64);

    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_FRAG_LIB_NOT_FOUND));
    assert!(loaded.cfm.as_ref().unwrap().connections.is_empty());
    assert_eq!(loaded.heap_cursor(), heap_before);
}

#[test]
fn close_connection_import_preserves_live_registry_on_protected_output_and_retries() {
    for partial in [false, true] {
        let mut loaded =
            load_pef_application(&synthetic_pef_with_import(b"CloseConnection")).unwrap();
        let pointer = PPC_HEAP_BASE + 0x1000;
        loaded.memory.write_u32_be(pointer, 77).unwrap();
        let connection = PpcCfmConnection {
            id: 77,
            library_name: "Library".to_string(),
            main_addr: 0x1234,
            init_addr: 0,
            term_addr: 0,
            exports: Vec::new(),
        };
        loaded.cfm.as_mut().unwrap().connections.push(connection);
        loaded
            .cfm
            .as_mut()
            .unwrap()
            .connections
            .push(PpcCfmConnection {
                id: 78,
                library_name: "Other".to_string(),
                main_addr: 0x5678,
                init_addr: 0,
                term_addr: 0,
                exports: Vec::new(),
            });
        if partial {
            loaded.memory.add_readonly_region(pointer + 3, vec![77]);
        } else {
            loaded
                .memory
                .add_readonly_region(pointer, 77u32.to_be_bytes().to_vec());
        }
        let before = loaded.cfm.clone();
        let entry = loaded.cpu.pc;
        loaded.cpu.gpr[3] = pointer;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
        assert_eq!(loaded.cfm, before);
        assert_eq!(loaded.memory.read_u32_be(pointer), Some(77));

        loaded.memory.write_u32_be(pointer + 8, 77).unwrap();
        loaded.cpu.pc = entry;
        loaded.cpu.gpr[3] = pointer + 8;
        let probe = loaded.run_with_hle_imports(64);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(loaded.cpu.gpr[3], 0);
        assert_eq!(loaded.memory.read_u32_be(pointer + 8), Some(0));
        let mut expected = before.unwrap();
        expected.connections.remove(0);
        assert_eq!(loaded.cfm.as_ref(), Some(&expected));
    }
}

#[test]
fn find_symbol_returns_real_export_class_before_hle_fallback() {
    let pef = synthetic_pef_with_import(b"FindSymbol");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    let symbol_ptr = scratch;
    let address_ptr = scratch + 32;
    let class_ptr = scratch + 36;
    write_ppc_pstring(&mut loaded.memory, symbol_ptr, b"RealExport");
    loaded
        .cfm
        .as_mut()
        .unwrap()
        .connections
        .push(PpcCfmConnection {
            id: 77,
            library_name: "RealLibrary".to_string(),
            main_addr: 0,
            init_addr: 0,
            term_addr: 0,
            exports: vec![PpcCfmExport {
                name: "RealExport".to_string(),
                class: 1,
                address: 0x0312_3456,
            }],
        });
    loaded.cpu.gpr[3] = 77;
    loaded.cpu.gpr[4] = symbol_ptr;
    loaded.cpu.gpr[5] = address_ptr;
    loaded.cpu.gpr[6] = class_ptr;
    let import_len_before = loaded.imports.len();
    let mut import_run_state = PpcImportRunState::from_parts(
        std::mem::take(&mut loaded.imports),
        loaded.import_count,
        ppc_import_layout(),
    );

    let action = ppc_find_symbol(
        &loaded.cpu,
        &mut loaded.memory,
        &loaded.cfm.as_ref().unwrap().connections,
        &mut import_run_state,
    );
    (loaded.imports, loaded.import_count) = import_run_state.into_parts();

    assert_eq!(action, PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)));
    assert_eq!(loaded.memory.read_u32_be(address_ptr), Some(0x0312_3456));
    assert_eq!(loaded.memory.read_u8(class_ptr), Some(1));
    assert_eq!(loaded.imports.len(), import_len_before);
}

#[test]
fn find_symbol_import_refuses_partial_outputs_retries_and_reuses_callable_bindings() {
    const OUTPUT: u32 = PPC_HEAP_BASE + 0x100;
    for dynamic in [false, true] {
        let mut loaded = load_pef_application(&synthetic_pef_with_import(b"FindSymbol")).unwrap();
        loaded.memory.add_region(OUTPUT, vec![0xa5; 128]);
        loaded.memory.add_readonly_region(OUTPUT + 8, vec![0xa5]);
        let symbol = if dynamic {
            b"TickCount".as_slice()
        } else {
            b"RealExport".as_slice()
        };
        write_ppc_pstring(&mut loaded.memory, OUTPUT + 32, symbol);
        loaded
            .cfm
            .as_mut()
            .unwrap()
            .connections
            .push(PpcCfmConnection {
                id: 7,
                library_name: if dynamic {
                    "InterfaceLib"
                } else {
                    "RealLibrary"
                }
                .into(),
                main_addr: 0,
                init_addr: 0,
                term_addr: 0,
                exports: if dynamic {
                    vec![]
                } else {
                    vec![PpcCfmExport {
                        name: "RealExport".into(),
                        class: 1,
                        address: 0x1234_5678,
                    }]
                },
            });
        if dynamic {
            loaded
                .cfm
                .as_mut()
                .unwrap()
                .connections
                .push(PpcCfmConnection {
                    id: 8,
                    library_name: "StdCLib".into(),
                    main_addr: 0,
                    init_addr: 0,
                    term_addr: 0,
                    exports: vec![],
                });
        }
        let original_count = loaded.import_count;
        let original_len = loaded.imports.len();
        let mut returned = None;
        for attempt in 0..3 {
            loaded.cpu.pc = loaded.entry_pc;
            loaded.cpu.lr = PPC_HALT_PC;
            loaded.cpu.gpr[3] = 7;
            loaded.cpu.gpr[4] = OUTPUT + 32;
            loaded.cpu.gpr[5] = OUTPUT;
            loaded.cpu.gpr[6] = OUTPUT + if attempt == 0 { 8 } else { 9 };
            let probe = loaded.run_with_hle_imports(128);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            if attempt == 0 {
                assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_PARAM_ERR));
                assert_eq!(loaded.memory.read_u32_be(OUTPUT), Some(0xa5a5_a5a5));
                assert_eq!(loaded.import_count, original_count);
                assert_eq!(loaded.imports.len(), original_len);
                if dynamic {
                    write_ppc_pstring(&mut loaded.memory, OUTPUT + 32, b"errno");
                    loaded.cpu.pc = loaded.entry_pc;
                    loaded.cpu.lr = PPC_HALT_PC;
                    loaded.cpu.gpr[3] = 8;
                    loaded.cpu.gpr[4] = OUTPUT + 32;
                    loaded.cpu.gpr[5] = OUTPUT + 16;
                    loaded.cpu.gpr[6] = OUTPUT + 24;
                    let unrelated = loaded.run_with_hle_imports(128);
                    assert_eq!(unrelated.handled_import_count, 1);
                    assert_eq!(unrelated.unsupported_import_index, None);
                    assert_eq!(loaded.cpu.gpr[3], 0);
                    assert_eq!(
                        loaded.memory.read_u32_be(OUTPUT + 16),
                        Some(PPC_IMPORT_STD_ERRNO)
                    );
                    assert_eq!(loaded.import_count, original_count);
                    assert_eq!(loaded.imports.len(), original_len);
                    write_ppc_pstring(&mut loaded.memory, OUTPUT + 32, b"TickCount");
                }
            } else {
                assert_eq!(loaded.cpu.gpr[3], 0);
                let address = loaded.memory.read_u32_be(OUTPUT).unwrap();
                if let Some(previous) = returned {
                    assert_eq!(address, previous);
                }
                returned = Some(address);
                assert_eq!(loaded.import_count, original_count + u32::from(dynamic));
                assert_eq!(loaded.imports.len(), original_len + usize::from(dynamic));
            }
        }
        if dynamic {
            let vector = returned.unwrap();
            loaded
                .memory
                .write_u32_be(crate::memory::globals::addr::TICKS, 0x1234)
                .unwrap();
            loaded.cpu.pc = loaded.memory.read_u32_be(vector).unwrap();
            loaded.cpu.gpr[2] = loaded.memory.read_u32_be(vector + 4).unwrap();
            loaded.cpu.lr = PPC_HALT_PC;
            let probe = loaded.run_with_hle_imports(64);
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(loaded.cpu.gpr[3], 0x1234);
            // An existing symbol remains available when the gateway pool is full.
            loaded.import_count = PPC_IMPORT_CAPACITY;
            let mut bindings = loaded.cfm_symbol_bindings();
            assert_eq!(
                crate::cfm::CfmSymbolBindings::prepare(&mut bindings, "InterfaceLib", "TickCount"),
                Ok((vector, 2))
            );
            crate::cfm::CfmSymbolBindings::commit(&mut bindings);
            drop(bindings);
            assert_eq!(loaded.import_count, PPC_IMPORT_CAPACITY);
            assert_eq!(loaded.imports.len(), original_len + 1);
        }
    }
}

#[test]
fn find_symbol_returns_the_static_stdclib_data_identity() {
    let pef = synthetic_pef_with_import(b"FindSymbol");
    let mut loaded = load_pef_application(&pef).unwrap();
    let scratch = PPC_HEAP_BASE;
    loaded.memory.add_region(scratch, vec![0; 128]);
    let symbol_ptr = scratch;
    let address_ptr = scratch + 32;
    let class_ptr = scratch + 36;
    loaded
        .cfm
        .as_mut()
        .unwrap()
        .connections
        .push(PpcCfmConnection {
            id: 78,
            library_name: "StdCLib".to_string(),
            main_addr: 0,
            init_addr: 0,
            term_addr: 0,
            exports: Vec::new(),
        });
    loaded.cpu.gpr[3] = 78;
    loaded.cpu.gpr[4] = symbol_ptr;
    loaded.cpu.gpr[5] = address_ptr;
    loaded.cpu.gpr[6] = class_ptr;
    let import_len_before = loaded.imports.len();
    let import_count_before = loaded.import_count;
    for (symbol, expected_address) in [
        (b"errno".as_slice(), PPC_IMPORT_STD_ERRNO),
        (b"MacOSErr".as_slice(), PPC_IMPORT_STD_MAC_OS_ERR),
    ] {
        write_ppc_pstring(&mut loaded.memory, symbol_ptr, symbol);
        let mut import_run_state = PpcImportRunState::from_parts(
            std::mem::take(&mut loaded.imports),
            loaded.import_count,
            ppc_import_layout(),
        );
        let action = ppc_find_symbol(
            &loaded.cpu,
            &mut loaded.memory,
            &loaded.cfm.as_ref().unwrap().connections,
            &mut import_run_state,
        );
        (loaded.imports, loaded.import_count) = import_run_state.into_parts();

        assert_eq!(action, PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)));
        assert_eq!(
            loaded.memory.read_u32_be(address_ptr),
            Some(expected_address)
        );
        assert_eq!(loaded.memory.read_u8(class_ptr), Some(1));
        assert_eq!(loaded.imports.len(), import_len_before);
        assert_eq!(loaded.import_count, import_count_before);
    }
}

#[test]
fn memory_fragment_preparation_rejects_without_publication_and_retries() {
    use crate::memory::{MacMemoryBus, MemoryBus};
    use crate::process_context::{ProcessNativeHeapState, ProcessNativeMemoryManager};

    const HEAP: u32 = 0x0300_0000;
    const LIMIT: u32 = HEAP + 0x1000;
    let loader = synthetic_loader_with_chunks(
        b"InterfaceLib",
        b"TickCount",
        &[run_reloc(0x23, 1), run_reloc(0x25, 1)],
    );
    let fragment = synthetic_pef_with_loader_and_data(loader, &[0; 12]);
    let heap_state = |cursor, limit| ProcessNativeHeapState {
        heap_base: HEAP,
        heap_cursor: cursor,
        heap_limit: limit,
        last_mem_error: 0,
        heap_maximized: false,
        master_pointer_blocks_requested: 0,
    };
    for refusal in 0..5 {
        let mut memory = PpcSectionMem::new();
        let mapped = if refusal == 0 {
            synthetic_code().len()
        } else {
            0x1000
        };
        memory.add_region(HEAP, vec![0xa5; mapped]);
        let mut classic = MacMemoryBus::new(0x10000);
        classic.set_addressing_32_bit(true);
        classic.attach_guest_address_space(memory.shared_view());
        let mut manager = ProcessNativeMemoryManager::default();
        if refusal != 1 {
            let state = match refusal {
                2 => heap_state(HEAP + 0x800, LIMIT),
                3 => heap_state(HEAP, HEAP + 0x20),
                // The planned end is ahead of the canonical cursor, but
                // the first section would overwrite an existing allocation.
                4 => heap_state(HEAP + 0x10, LIMIT),
                _ => heap_state(HEAP, LIMIT),
            };
            manager.publish_native_allocator(state, &[], &[], &[]);
        }
        let before_allocator = manager.native_allocator_snapshot();
        let before_update = manager.native_allocator_update();
        let mut cursor = HEAP;
        let mut import_run_state =
            PpcImportRunState::from_parts(Vec::new(), 0, ppc_import_layout());
        assert_eq!(
            ppc_prepare_mem_fragment(
                &fragment,
                &mut manager,
                &mut memory,
                &mut cursor,
                LIMIT,
                &mut import_run_state,
                &[],
            ),
            Err(PPC_FRAG_NO_ADDR_SPACE),
            "refusal {refusal}"
        );
        let mut bytes = vec![0; mapped];
        memory.read_bytes_into(HEAP, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            vec![0xa5; mapped],
            "refusal {refusal} changed section bytes"
        );
        assert_eq!(classic.read_long(HEAP), 0xa5a5_a5a5);
        assert_eq!(manager.native_allocator_snapshot(), before_allocator);
        assert_eq!(manager.native_allocator_update(), before_update);
        assert_eq!(cursor, HEAP);
        assert!(import_run_state.bindings().is_empty());
        assert_eq!(import_run_state.total_count(), 0);
        assert_eq!(import_run_state.binding_cloned(0), None);

        memory.add_region(HEAP, vec![0xa5; 0x1000]);
        manager.publish_native_allocator(heap_state(HEAP, LIMIT), &[], &[], &[]);
        let prepared = ppc_prepare_mem_fragment(
            &fragment,
            &mut manager,
            &mut memory,
            &mut cursor,
            LIMIT,
            &mut import_run_state,
            &[],
        )
        .unwrap();
        assert_eq!(manager.native_heap_state().unwrap().heap_cursor, cursor);
        assert!(cursor > HEAP);
        assert_eq!(import_run_state.total_count(), 1);
        assert_eq!(import_run_state.bindings().len(), 1);
        assert_eq!(
            import_run_state.binding_cloned(0).unwrap().symbol_name,
            "TickCount"
        );
        assert_eq!(memory.read_u32_be(prepared.main_addr), Some(HEAP));
        assert_eq!(classic.read_long(prepared.main_addr), HEAP);
        assert_eq!(
            memory.read_u32_be(prepared.main_addr + 8),
            Some(PPC_IMPORT_TVECTOR_BASE)
        );
    }
}

#[test]
fn dynamic_fragment_imports_resolve_existing_guest_library_exports() {
    let mut loaded = load_pef_application(&synthetic_pef()).unwrap();
    let fragment = synthetic_pef_with_library_import(b"Storm", b"__register_fragment");
    let connection = PpcCfmConnection {
        id: 7,
        library_name: "Storm".into(),
        main_addr: 0,
        init_addr: 0,
        term_addr: 0,
        exports: vec![PpcCfmExport {
            name: "__register_fragment".into(),
            class: 2,
            address: 0x0400_1000,
        }],
    };
    let mut cursor = loaded.heap_cursor();
    let limit = loaded.heap_limit();
    let mut imports = PpcImportRunState::from_parts(Vec::new(), 0, ppc_import_layout());
    let mut manager = loaded.process_memory_manager.0.borrow_mut();

    ppc_prepare_mem_fragment(
        &fragment,
        &mut manager,
        &mut loaded.memory,
        &mut cursor,
        limit,
        &mut imports,
        &[connection],
    )
    .unwrap();

    let binding = imports.binding_cloned(0).unwrap();
    assert_eq!(binding.address, 0x0400_1000);
    assert_eq!(binding.tvector_address, None);
}

#[test]
fn bundled_imports_bind_by_name_when_symbol_classes_differ() {
    let connection = PpcCfmConnection {
        id: 7,
        library_name: "Storm".into(),
        main_addr: 0,
        init_addr: 0,
        term_addr: 0,
        exports: vec![PpcCfmExport {
            name: "__register_fragment".into(),
            class: 2,
            address: 0x0400_1000,
        }],
    };
    let policy = PpcConnectedCfmBindingPolicy {
        connections: &[connection],
    };
    let import = PefResolvedImport {
        library_index: 0,
        symbol_index: 0,
        library_name: "Storm".into(),
        symbol_name: "__register_fragment".into(),
        class: 1,
        weak: false,
    };
    let plan = PpcImportBindingPlan::prepare(vec![import], 1, 0, ppc_import_layout(), &policy)
        .unwrap();
    assert_eq!(plan.relocation_addresses(), &[0x0400_1000]);
}

#[test]
fn hle_import_runner_loads_and_runs_a_memory_fragment_with_dynamic_imports() {
    let pef = synthetic_pef_with_import(b"GetMemFragment");
    let mut loaded = load_pef_application(&pef).unwrap();

    let chunks = [run_reloc(0x23, 1), run_reloc(0x25, 1)];
    let loader = synthetic_loader_with_chunks(b"InterfaceLib", b"TickCount", &chunks);
    let mut fragment = synthetic_pef_with_loader_and_data(loader, &[0; 12]);
    let code_offset = parse_pef_sections(&fragment).unwrap()[0].container_offset as usize;
    let dynamic_trap = PPC_IMPORT_TRAP_BASE + 4;
    write_u32(
        &mut fragment,
        code_offset,
        d_form_u(15, 12, 0, (dynamic_trap >> 16) as u16),
    );
    write_u32(
        &mut fragment,
        code_offset + 4,
        d_form_u(24, 12, 12, dynamic_trap as u16),
    );

    let fragment_addr = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        fragment.len() as u32,
        false,
    );
    assert_ne!(fragment_addr, 0);
    loaded.memory.write_bytes(fragment_addr, &fragment).unwrap();
    let scratch = ppc_heap_alloc(
        &mut loaded.memory,
        test_heap_cursor!(loaded),
        test_heap_limit!(loaded),
        128,
        true,
    );
    assert_ne!(scratch, 0);
    write_ppc_pstring(&mut loaded.memory, scratch, b"dynamic test fragment");
    let conn_id_ptr = scratch + 64;
    let main_addr_ptr = scratch + 68;
    let err_name_ptr = scratch + 72;

    loaded.cpu.gpr[3] = fragment_addr;
    loaded.cpu.gpr[4] = fragment.len() as u32;
    loaded.cpu.gpr[5] = scratch;
    loaded.cpu.gpr[6] = PPC_CFM_LOAD_NEW_COPY;
    loaded.cpu.gpr[7] = conn_id_ptr;
    loaded.cpu.gpr[8] = main_addr_ptr;
    loaded.cpu.gpr[9] = err_name_ptr;

    let load_probe = loaded.run_with_hle_imports(128);

    assert_eq!(load_probe.handled_import_count, 1);
    assert_eq!(load_probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], ppc_i16_result(PPC_NO_ERR));
    assert_eq!(
        loaded.memory.read_u32_be(conn_id_ptr),
        Some(PPC_FIRST_CFM_CONNECTION_ID)
    );
    let main_addr = loaded.memory.read_u32_be(main_addr_ptr).unwrap();
    assert_ne!(main_addr, 0);
    assert_ne!(main_addr, PPC_CFM_MAIN_STUB_BASE);
    assert_eq!(loaded.memory.read_u8(err_name_ptr), Some(0));
    assert_eq!(loaded.import_count, 2);
    assert_eq!(loaded.heap_cursor() & (PPC_HEAP_ALIGNMENT - 1), 0);
    let dynamic_import = loaded.import_binding(1).unwrap();
    assert_eq!(dynamic_import.symbol_name, "TickCount");
    assert_eq!(dynamic_import.address, PPC_IMPORT_TVECTOR_BASE + 8);
    assert_eq!(dynamic_import.trap_pc, dynamic_trap);
    assert_eq!(
        loaded.memory.read_u32_be(main_addr + 8),
        Some(PPC_IMPORT_TVECTOR_BASE + 8)
    );

    loaded.cpu.pc = loaded.memory.read_u32_be(main_addr).unwrap();
    loaded.cpu.gpr[2] = loaded.memory.read_u32_be(main_addr + 4).unwrap();
    loaded.cpu.lr = PPC_HALT_PC;
    loaded.set_tick_count(77);
    loaded.set_clock_cycle_timing(1_000, 0);
    let run_probe = loaded.run_with_hle_imports(128);

    assert_eq!(run_probe.handled_import_count, 1);
    assert_eq!(run_probe.last_import_index, Some(1));
    assert_eq!(run_probe.unsupported_import_index, None);
    assert_eq!(loaded.cpu.gpr[3], 77);
    assert!(matches!(
        run_probe.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            ..
        }
    ));

    let committed_imports = loaded.imports.clone();
    let committed_count = loaded.import_count;
    let assert_registry = |loaded: &PpcLoadedApp| {
        assert_eq!(loaded.imports, committed_imports);
        assert_eq!(loaded.import_count, committed_count);
        assert_eq!(loaded.import_binding(1).unwrap().symbol_name, "TickCount");
    };
    assert_registry(&loaded);

    loaded.cpu.pc = PPC_IMPORT_TRAP_BASE + committed_count * 4;
    let unsupported = loaded.run_with_hle_imports(1);
    assert_eq!(unsupported.unsupported_import_index, Some(committed_count));
    assert_registry(&loaded);

    native_exceptions::install_test_unmapped_load(&mut loaded);
    let fault = loaded.run_with_hle_imports(64);
    assert!(matches!(fault.result, PpcRunResult::MemoryFault { .. }));
    assert_registry(&loaded);

    loaded.cpu.pc = loaded.entry_pc;
    let cycle_limit = loaded.run_with_hle_imports(0);
    assert_eq!(cycle_limit.result, PpcRunResult::CycleLimit { cycles: 0 });
    assert_registry(&loaded);
}
