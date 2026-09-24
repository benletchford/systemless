use super::*;
use crate::cfm::{
    CfmFindSymbol, CfmLoadId, CfmLoadOperation, CfmLoadOutputs, CfmMemory, CfmSymbolQuery,
};

pub(super) struct PpcCfmDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) guest_calls: &'a SharedGuestCallStack,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) cfm_connections: &'a mut Vec<PpcCfmConnection>,
    pub(super) cfm_library_fragments: &'a mut Vec<PpcCfmLibraryFragment>,
    pub(super) next_cfm_connection_id: &'a mut u32,
    pub(super) import_run_state: &'a mut PpcImportRunState,
}

pub(super) fn dispatch_cfm_import(context: PpcCfmDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcCfmDispatchContext {
        binding,
        cpu,
        guest_calls,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        cfm_connections,
        cfm_library_fragments,
        next_cfm_connection_id,
        import_run_state,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetSharedLibrary => Some(ppc_get_shared_library(
            cpu,
            guest_calls,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            cfm_connections,
            cfm_library_fragments,
            next_cfm_connection_id,
            import_run_state,
        )),
        PpcImportDispatcherTarget::FindSymbol => Some(ppc_find_symbol(
            cpu,
            memory,
            cfm_connections,
            import_run_state,
        )),
        PpcImportDispatcherTarget::CountSymbols | PpcImportDispatcherTarget::GetIndSymbol => {
            // PowerPC System Software (1994), pp. 3-25–3-26. Only the ABI
            // registers and return encoding belong in this adapter.
            let query = if binding.dispatcher_target == PpcImportDispatcherTarget::CountSymbols {
                CfmSymbolQuery::Count {
                    connection: cpu.gpr[3],
                    count: cpu.gpr[4],
                }
            } else {
                CfmSymbolQuery::Indexed {
                    connection: cpu.gpr[3],
                    index: cpu.gpr[4],
                    name: cpu.gpr[5],
                    address: cpu.gpr[6],
                    class: cpu.gpr[7],
                }
            };
            let result =
                query.complete(cfm_connections, |writes| memory.publish_cfm_outputs(writes));
            Some(PpcImportAction::Return(ppc_i16_result(
                result.err().map_or(0, |error| error.os_error()),
            )))
        }
        PpcImportDispatcherTarget::CloseConnection => {
            let result = crate::cfm::close_connection(
                cfm_connections,
                memory,
                cpu.gpr[3],
                crate::cfm::CfmMemory::publish_cfm_outputs,
            );
            Some(PpcImportAction::Return(ppc_i16_result(
                result.err().map_or(0, |error| error.os_error()),
            )))
        }
        PpcImportDispatcherTarget::GetMemFragment => Some(ppc_get_mem_fragment(
            cpu,
            guest_calls,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            cfm_connections,
            next_cfm_connection_id,
            import_run_state,
        )),
        _ => None,
    }
}

pub(super) fn ppc_get_shared_library(
    cpu: &mut PpcCpu,
    guest_calls: &SharedGuestCallStack,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    cfm_connections: &mut Vec<PpcCfmConnection>,
    cfm_library_fragments: &mut Vec<PpcCfmLibraryFragment>,
    next_cfm_connection_id: &mut u32,
    import_run_state: &mut PpcImportRunState,
) -> PpcImportAction {
    let return_error = |error| PpcImportAction::Return(ppc_i16_result(error));
    let lib_name_ptr = cpu.gpr[3];
    let arch_type = cpu.gpr[4];
    let find_flags = cpu.gpr[5];
    let conn_id_ptr = cpu.gpr[6];
    let main_addr_ptr = cpu.gpr[7];
    let err_name_ptr = cpu.gpr[8];
    if lib_name_ptr == 0
        || conn_id_ptr == 0
        || main_addr_ptr == 0
        || !memory.preflight_writable_range(conn_id_ptr, 4)
        || !memory.preflight_writable_range(main_addr_ptr, 4)
        || (err_name_ptr != 0 && !memory.preflight_writable_range(err_name_ptr, 1))
    {
        return return_error(PPC_PARAM_ERR);
    }
    // Code Fragment Manager Reference, Architecture Constants: kAnyCFragArch
    // selects an available architecture (PowerPC in this execution context).
    if !matches!(arch_type, PPC_CFM_POWERPC_ARCH | PPC_CFM_ANY_ARCH) {
        return return_error(PPC_FRAG_ARCH_ERR);
    }
    if !matches!(
        find_flags,
        PPC_CFM_FIND_LIB | PPC_CFM_LOAD_LIB | PPC_CFM_LOAD_NEW_COPY
    ) {
        return return_error(PPC_FRAG_LIB_CONN_ERR);
    }

    let Some(lib_name_bytes) = ppc_read_pstring_bytes(memory, lib_name_ptr) else {
        return return_error(PPC_PARAM_ERR);
    };
    let lib_name = decode_mac_roman(&lib_name_bytes);
    if lib_name.trim().is_empty() {
        return return_error(PPC_FRAG_LIB_NOT_FOUND);
    }

    let existing_connection = if find_flags != PPC_CFM_LOAD_NEW_COPY {
        cfm_connections
            .iter()
            .find(|connection| connection.library_name.eq_ignore_ascii_case(&lib_name))
            .cloned()
    } else {
        None
    };
    // A statically imported system library is already connected by CFM.
    // HLE imports do not carry guest PEF fragments, so recognize their
    // existing binding when kFindLib asks for that connection.
    let statically_imported_hle = ppc_is_explicit_hle_cfm_library(&lib_name)
        && import_run_state
            .bindings()
            .iter()
            .any(|binding| binding.library_name.eq_ignore_ascii_case(&lib_name));
    if find_flags == PPC_CFM_FIND_LIB
        && existing_connection.is_none()
        && !statically_imported_hle
    {
        return return_error(PPC_FRAG_LIB_NOT_FOUND);
    }
    let created_connection = existing_connection.is_none();
    let mut initialization = None;
    let connection = match existing_connection {
        Some(connection) if guest_calls.is_cfm_load_pending(CfmLoadId(connection.id)) => {
            return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_INIT_LOOP));
        }
        Some(connection) => connection,
        None => {
            let id = *next_cfm_connection_id;
            if id == 0 || id > PPC_CFM_MAIN_STUB_COUNT {
                return return_error(PPC_FRAG_LIB_CONN_ERR);
            }
            let Some(next_id) = next_cfm_connection_id.checked_add(1) else {
                return return_error(PPC_FRAG_LIB_CONN_ERR);
            };
            let fragment = cfm_library_fragments
                .iter()
                .find(|fragment| fragment.name.eq_ignore_ascii_case(&lib_name))
                .cloned();
            let connection = if let Some(fragment) = fragment {
                let fragment_size = match u32::try_from(fragment.bytes.len()) {
                    Ok(size) => size,
                    Err(_) => return return_error(PPC_FRAG_NO_MEM),
                };
                let fragment_addr = ppc_process_heap_alloc(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    fragment_size,
                    false,
                );
                if fragment_addr == 0
                    || memory.write_bytes(fragment_addr, &fragment.bytes).is_none()
                {
                    return return_error(PPC_FRAG_NO_MEM);
                }
                let prepared = match ppc_prepare_mem_fragment(
                    &fragment.bytes,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    import_run_state,
                ) {
                    Ok(prepared) => prepared,
                    Err(error) => return return_error(error),
                };
                let connection = PpcCfmConnection {
                    id,
                    library_name: lib_name.clone(),
                    main_addr: prepared.main_addr,
                    init_addr: prepared.init_addr,
                    term_addr: prepared.term_addr,
                    exports: prepared.exports,
                };
                if prepared.init_addr != 0 {
                    let init_block = match ppc_create_mem_fragment_init_block(
                        Some(process_memory_manager),
                        memory,
                        heap_cursor,
                        heap_limit,
                        id,
                        fragment_addr,
                        fragment_size,
                        &lib_name,
                    ) {
                        Ok(block) => block,
                        Err(error) => return return_error(error),
                    };
                    initialization = Some((prepared.init_addr, init_block));
                }
                connection
            } else if ppc_is_explicit_hle_cfm_library(&lib_name) {
                // System libraries without a supplied PEF remain synthetic
                // CFM connections whose optional main routine is a no-op.
                PpcCfmConnection {
                    id,
                    library_name: lib_name.clone(),
                    main_addr: ppc_cfm_main_stub_addr(id),
                    init_addr: 0,
                    term_addr: 0,
                    exports: Vec::new(),
                }
            } else {
                return return_error(PPC_FRAG_LIB_NOT_FOUND);
            };
            cfm_connections.push(connection.clone());
            *next_cfm_connection_id = next_id;
            connection
        }
    };

    let operation = CfmLoadOperation {
        id: CfmLoadId(connection.id),
        created_connection,
        main_address: connection.main_addr,
        outputs: CfmLoadOutputs {
            connection: conn_id_ptr,
            main_address: main_addr_ptr,
            error_name: err_name_ptr,
        },
    };
    if let Some((init_addr, init_block)) = initialization {
        let action = ppc_activate_cfm_initializer(
            cpu,
            memory,
            guest_calls,
            process_memory_manager,
            init_addr,
            init_block,
            Some(operation),
        );
        if action != PpcImportAction::Continue {
            cfm_connections.retain(|connection| connection.id != operation.id.0);
        }
        return action;
    }
    PpcImportAction::Return(ppc_complete_cfm_load(operation, 0, memory, cfm_connections))
}

pub(super) fn ppc_find_symbol(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    cfm_connections: &[PpcCfmConnection],
    import_run_state: &mut PpcImportRunState,
) -> PpcImportAction {
    // PowerPC System Software (1994), pp. 3-24–3-25: decode the native ABI
    // here; shared CFM owns validation, export selection and publication.
    let request = CfmFindSymbol {
        connection: cpu.gpr[3],
        name: cpu.gpr[4],
        address: cpu.gpr[5],
        class: cpu.gpr[6],
    };
    let mut operation =
        import_run_state.symbol_binding_operation(&SystemlessPpcImportBindingPolicy);
    let result = request.complete(
        cfm_connections,
        memory,
        Some(&mut operation),
        |memory, writes| memory.publish_cfm_outputs(writes),
    );
    PpcImportAction::Return(ppc_i16_result(
        result.err().map_or(0, |error| error.os_error()),
    ))
}

pub(super) fn ppc_get_mem_fragment(
    cpu: &mut PpcCpu,
    guest_calls: &SharedGuestCallStack,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    cfm_connections: &mut Vec<PpcCfmConnection>,
    next_cfm_connection_id: &mut u32,
    import_run_state: &mut PpcImportRunState,
) -> PpcImportAction {
    // Inside Macintosh: PowerPC System Software (1994), pp. 3-21--3-22:
    // GetMemFragment binds an in-memory PEF and returns a connection ID plus
    // its callable main address.
    let mem_addr = cpu.gpr[3];
    let length = cpu.gpr[4];
    let frag_name_ptr = cpu.gpr[5];
    let find_flags = cpu.gpr[6];
    let conn_id_ptr = cpu.gpr[7];
    let main_addr_ptr = cpu.gpr[8];
    let err_name_ptr = cpu.gpr[9];
    if mem_addr == 0
        || length < 8
        || conn_id_ptr == 0
        || main_addr_ptr == 0
        || !ppc_memory_can_read_bytes(memory, mem_addr, length)
        || !ppc_memory_can_write_bytes(memory, conn_id_ptr, 4)
        || !ppc_memory_can_write_bytes(memory, main_addr_ptr, 4)
        || !ppc_optional_output_can_write(memory, err_name_ptr, 1)
    {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    if !matches!(
        find_flags,
        PPC_CFM_FIND_LIB | PPC_CFM_LOAD_LIB | PPC_CFM_LOAD_NEW_COPY
    ) {
        return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_LIB_CONN_ERR));
    }
    let Some(fragment) = ppc_memory_read_bytes(memory, mem_addr, length) else {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    if fragment.get(..8) != Some(b"Joy!peff") {
        return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_FORMAT_UNKNOWN));
    }
    let Some(header) = parse_pef_header(&fragment) else {
        return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_CORRUPT_ERR));
    };
    if header.architecture != *b"pwpc" {
        return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_ARCH_ERR));
    }
    let frag_name = if frag_name_ptr == 0 {
        format!("memory fragment ${mem_addr:08X}")
    } else {
        ppc_read_pstring_bytes(memory, frag_name_ptr)
            .map(|name| decode_mac_roman(&name))
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("memory fragment ${mem_addr:08X}"))
    };
    let existing_connection = if find_flags != PPC_CFM_LOAD_NEW_COPY {
        cfm_connections
            .iter()
            .find(|connection| connection.library_name.eq_ignore_ascii_case(&frag_name))
            .cloned()
    } else {
        None
    };
    if find_flags == PPC_CFM_FIND_LIB && existing_connection.is_none() {
        return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_LIB_NOT_FOUND));
    }
    let created_connection = existing_connection.is_none();
    let mut initialization = None;
    let connection = match existing_connection {
        Some(connection) if guest_calls.is_cfm_load_pending(CfmLoadId(connection.id)) => {
            return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_INIT_LOOP));
        }
        Some(connection) => connection,
        None => {
            let id = *next_cfm_connection_id;
            if id == 0 || id > PPC_CFM_MAIN_STUB_COUNT {
                return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_LIB_CONN_ERR));
            }
            let Some(next_id) = next_cfm_connection_id.checked_add(1) else {
                return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_LIB_CONN_ERR));
            };
            let prepared = match ppc_prepare_mem_fragment(
                &fragment,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                import_run_state,
            ) {
                Ok(prepared) => prepared,
                Err(error) => return PpcImportAction::Return(ppc_i16_result(error)),
            };
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] GetMemFragment name={frag_name:?} bytes={length} lr=${:08X} -> conn={id} main=${:08X} init=${:08X} term=${:08X} entry=${:08X} rtoc=${:08X} imports={}",
                    cpu.lr,
                    prepared.main_addr,
                    prepared.init_addr,
                    prepared.term_addr,
                    memory.read_u32_be(prepared.main_addr).unwrap_or(0),
                    memory.read_u32_be(prepared.main_addr.wrapping_add(4)).unwrap_or(0),
                    import_run_state.total_count(),
                );
            }
            let connection = PpcCfmConnection {
                id,
                library_name: frag_name,
                main_addr: prepared.main_addr,
                init_addr: prepared.init_addr,
                term_addr: prepared.term_addr,
                exports: prepared.exports,
            };
            if prepared.init_addr != 0 {
                let init_block = match ppc_create_mem_fragment_init_block(
                    Some(process_memory_manager),
                    memory,
                    heap_cursor,
                    heap_limit,
                    id,
                    mem_addr,
                    length,
                    &connection.library_name,
                ) {
                    Ok(init_block) => init_block,
                    Err(error) => return PpcImportAction::Return(ppc_i16_result(error)),
                };
                initialization = Some((prepared.init_addr, init_block));
            }
            cfm_connections.push(connection.clone());
            *next_cfm_connection_id = next_id;
            connection
        }
    };
    let operation = CfmLoadOperation {
        id: CfmLoadId(connection.id),
        created_connection,
        main_address: connection.main_addr,
        outputs: CfmLoadOutputs {
            connection: conn_id_ptr,
            main_address: main_addr_ptr,
            error_name: err_name_ptr,
        },
    };
    if let Some((init_addr, init_block)) = initialization {
        let action = ppc_activate_cfm_initializer(
            cpu,
            memory,
            guest_calls,
            process_memory_manager,
            init_addr,
            init_block,
            Some(operation),
        );
        if action != PpcImportAction::Continue {
            cfm_connections.retain(|connection| connection.id != operation.id.0);
        }
        return action;
    }
    PpcImportAction::Return(ppc_complete_cfm_load(operation, 0, memory, cfm_connections))
}

pub(super) fn ppc_complete_cfm_load(
    operation: CfmLoadOperation,
    initializer_result: u32,
    memory: &mut PpcSectionMem,
    connections: &mut Vec<PpcCfmConnection>,
) -> u32 {
    let result = operation.complete(initializer_result, connections, |writes| {
        memory.try_write_ranges_atomic(writes)
    });
    ppc_i16_result(result.err().map_or(PPC_NO_ERR, |error| error.os_error()))
}

pub(super) fn ppc_activate_cfm_initializer(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    guest_calls: &SharedGuestCallStack,
    memory_manager: &mut ProcessNativeMemoryManager,
    init_addr: u32,
    init_block: u32,
    operation: Option<CfmLoadOperation>,
) -> PpcImportAction {
    let invocation = crate::cfm::initialization_invocation(
        memory,
        guest_calls.current_task(),
        init_addr,
        init_block,
    );
    let Ok(invocation) = invocation else {
        memory_manager.release_native_scratch(init_block);
        return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_CORRUPT_ERR));
    };
    let effect = GuestCallEffect::call_guest(
        GuestCallRequest::for_task(
            invocation.task,
            GuestCallTarget {
                isa: invocation.procedure.isa,
                entry: invocation.procedure.entry,
                rtoc: invocation.procedure.rtoc,
            },
        )
        .with_powerpc_arguments(invocation.arguments),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            cpu.lr,
            cpu.gpr[2],
            if operation.is_some() {
                PpcNativeReturnGpr3::Preserve
            } else {
                PpcNativeReturnGpr3::ZeroOrSet {
                    zero: ppc_i16_result(PPC_NO_ERR),
                    nonzero: ppc_i16_result(PPC_FRAG_USER_INIT_PROC_ERR),
                }
            },
        ),
    );
    if !guest_calls.activate_powerpc_effect_with_scratch(
        cpu,
        memory,
        effect,
        Some(init_block),
        operation,
    ) {
        memory_manager.release_native_scratch(init_block);
        return PpcImportAction::Return(ppc_i16_result(PPC_FRAG_CORRUPT_ERR));
    }
    cpu.gpr[12] = init_addr;
    PpcImportAction::Continue
}

pub(super) fn ppc_create_mem_fragment_init_block(
    mut process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    connection_id: u32,
    mem_addr: u32,
    length: u32,
    fragment_name: &str,
) -> Result<u32, i16> {
    let block = crate::cfm::CfmInitBlock::in_memory(
        CfmLoadId(connection_id),
        mem_addr,
        length,
        fragment_name,
    );
    let size = block.size();
    let init_block = if let Some(manager) = process_memory_manager.as_deref_mut() {
        let ptr = manager.new_native_scratch(memory, size);
        if let Some(heap) = manager.native_heap_state() {
            *heap_cursor = heap.heap_cursor;
            ppc_update_zone_free_bytes(
                memory,
                heap.heap_cursor,
                manager.native_allocation_limit(heap.heap_limit),
            );
        }
        ptr
    } else {
        // Startup has not attached a process allocator yet; its loader-owned
        // storage remains part of the application image's lifetime.
        ppc_heap_alloc(memory, heap_cursor, heap_limit, size, true)
    };
    if init_block == 0 {
        return Err(PPC_FRAG_NO_MEM);
    }
    if block.publish(memory, init_block).is_err() {
        if let Some(manager) = process_memory_manager {
            manager.release_native_scratch(init_block);
        }
        return Err(PPC_FRAG_NO_ADDR_SPACE);
    }
    Ok(init_block)
}

pub(super) fn ppc_prepare_mem_fragment(
    fragment: &[u8],
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    import_run_state: &mut PpcImportRunState,
) -> Result<crate::cfm::fragment::CfmPreparedFragment, i16> {
    let imported_symbols = parse_pef_imported_symbols(fragment).ok_or(PPC_FRAG_CORRUPT_ERR)?;
    let resolved_imports = resolve_pef_imports(fragment).ok_or(PPC_FRAG_CORRUPT_ERR)?;
    let import_plan = import_run_state
        .plan_resolved(
            resolved_imports,
            imported_symbols.len(),
            &SystemlessPpcImportBindingPolicy,
        )
        .map_err(ppc_dynamic_import_error)?;
    let pending = import_run_state
        .stage_append(import_plan)
        .map_err(ppc_dynamic_import_error)?;

    let plan = crate::cfm::fragment::CfmFragmentPlan::prepare(
        fragment,
        pending.relocation_addresses(),
        *heap_cursor,
        heap_limit,
        PPC_HEAP_ALIGNMENT,
        |cursor, size, alignment| {
            ppc_aligned_heap_allocation_bounds(memory, cursor, heap_limit, size, alignment)
        },
    )
    .map_err(|error| error.os_error())?;
    let next_heap_cursor = plan.next_heap_cursor();
    let committed = process_memory_manager.commit_native_heap_cursor_with(
        *heap_cursor,
        next_heap_cursor,
        || plan.publish(memory),
    );
    if !committed {
        return Err(PPC_FRAG_NO_ADDR_SPACE);
    }
    *heap_cursor = next_heap_cursor;
    pending.commit();
    Ok(plan.into_fragment())
}

pub(super) fn ppc_cfm_main_stub_addr(connection_id: u32) -> u32 {
    PPC_CFM_MAIN_STUB_BASE + (connection_id.saturating_sub(1) * 4)
}

pub(super) fn ppc_is_explicit_hle_cfm_library(library_name: &str) -> bool {
    // CFM's synthetic fallback is reserved for compatibility namespaces
    // implemented by the HLE dispatcher. A missing arbitrary library must
    // remain fragLibNotFound so callers can distinguish it from a loaded PEF.
    matches!(
        library_name,
        "InterfaceLib"
            | "StdCLib"
            | "MathLib"
            | "SoundLib"
            | "SpeechLib"
            | "QuickTimeLib"
            | "InputSprocketLib"
            | "ObjectSupportLib"
            | "AppearanceLib"
            | "DisplayLib"
            | "DrawSprocketLib"
            | "3DfxGlideLib2.x"
            | "CodeFragmentMgr"
            | "CarbonCore.vlib"
            | "CFMPriv_CarbonCore"
    ) || is_quickdraw_3d_library(library_name)
        || is_quickdraw_3d_accelerator_library(library_name)
}
