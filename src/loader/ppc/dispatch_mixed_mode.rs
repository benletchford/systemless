use super::*;

pub(super) struct PpcMixedModeDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) cfm_connections: &'a mut Vec<PpcCfmConnection>,
    pub(super) next_cfm_connection_id: &'a mut u32,
    pub(super) import_run_state: &'a mut PpcImportRunState,
}

pub(super) fn dispatch_mixed_mode_import(
    context: PpcMixedModeDispatchContext<'_>,
) -> Option<Option<PpcImportAction>> {
    let PpcMixedModeDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        toolbox_startup,
        cfm_connections,
        next_cfm_connection_id,
        import_run_state,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::NewRoutineDescriptor => {
            Some(Some(PpcImportAction::Return(ppc_new_routine_descriptor(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            ))))
        }
        PpcImportDispatcherTarget::NewFatRoutineDescriptor => Some(Some(PpcImportAction::Return(
            ppc_new_fat_routine_descriptor(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            ),
        ))),
        PpcImportDispatcherTarget::DisposeRoutineDescriptor => {
            // DisposeRoutineDescriptor(theProcPtr: UniversalProcPtr): void.
            // PowerPC ABI: r3 carries the descriptor and is preserved on return.
            // The Mixed Mode Manager releases only creation-allocated heap storage.
            // Inside Macintosh: PowerPC System Software (1994), pp. 2-21, 2-41.
            let _ = process_memory_manager.dispose_native_ptr(cpu.gpr[3]);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            *last_mem_error = PPC_NO_ERR;
            Some(Some(PpcImportAction::ReturnPreserve))
        }
        PpcImportDispatcherTarget::CallUniversalProc => {
            let selector = match ppc_call_universal_proc_selector(cpu, memory, cpu.gpr[4]) {
                Ok(selector) => selector,
                Err(()) => return Some(None),
            };
            if let Some(crate::guest_procedure::GuestProcedureResolution::Prepare(request)) =
                crate::guest_procedure::inspect_guest_procedure(
                    memory,
                    cpu.gpr[3],
                    cpu.gpr[2],
                    selector,
                    GuestIsa::PowerPc,
                    GuestIsa::PowerPc,
                )
            {
                return Some(Some(ppc_prepare_resource_call(
                    cpu,
                    memory,
                    process_memory_manager,
                    &toolbox_startup.execution.calls(),
                    heap_cursor,
                    heap_limit,
                    cfm_connections,
                    next_cfm_connection_id,
                    import_run_state,
                    request,
                    selector,
                )));
            }
            Some(ppc_call_universal_proc(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
                GuestIsa::PowerPc,
            ))
        }
        PpcImportDispatcherTarget::CallOSTrapUniversalProc => {
            Some(ppc_call_os_trap_universal_proc(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
            ))
        }
        _ => None,
    }
}

fn ppc_new_routine_descriptor(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> u32 {
    let proc_ptr = cpu.gpr[3];
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] NewRoutineDescriptor proc=${proc_ptr:08X} words=({:08X?},{:08X?}) procInfo=${:08X} isa={}",
            memory.read_u32_be(proc_ptr),
            memory.read_u32_be(proc_ptr.wrapping_add(4)),
            cpu.gpr[4],
            cpu.gpr[5],
        );
    }
    if proc_ptr == 0 {
        return 0;
    }
    let proc_info = cpu.gpr[4];
    let isa = cpu.gpr[5] as u8;
    let descriptor_size = PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE + PPC_ROUTINE_RECORD_SIZE;
    let descriptor = ppc_alloc_routine_descriptor(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        descriptor_size,
        0,
    );
    if descriptor == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }

    let record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    let flags = if isa == PPC_ROUTINE_RECORD_POWERPC_ISA {
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA
    } else {
        0
    };
    let ok = ppc_write_routine_record(memory, record, proc_info, isa, flags, proc_ptr);
    if !ok {
        let _ = process_memory_manager.dispose_native_ptr(descriptor);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }

    *last_mem_error = PPC_NO_ERR;
    descriptor
}

fn ppc_new_fat_routine_descriptor(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
) -> u32 {
    let m68k_proc = cpu.gpr[3];
    let powerpc_proc = cpu.gpr[4];
    if m68k_proc == 0 || powerpc_proc == 0 {
        return 0;
    }
    let proc_info = cpu.gpr[5];
    let descriptor_size = PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE + (PPC_ROUTINE_RECORD_SIZE * 2);
    let descriptor = ppc_alloc_routine_descriptor(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        descriptor_size,
        1,
    );
    if descriptor == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }

    let m68k_record = descriptor + PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE;
    let ppc_record = m68k_record + PPC_ROUTINE_RECORD_SIZE;
    let ok = ppc_write_routine_record(
        memory,
        m68k_record,
        proc_info,
        PPC_ROUTINE_RECORD_M68K_ISA,
        0,
        m68k_proc,
    ) && ppc_write_routine_record(
        memory,
        ppc_record,
        proc_info,
        PPC_ROUTINE_RECORD_POWERPC_ISA,
        PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
        powerpc_proc,
    );
    if !ok {
        let _ = process_memory_manager.dispose_native_ptr(descriptor);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }

    *last_mem_error = PPC_NO_ERR;
    descriptor
}

fn ppc_alloc_routine_descriptor(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    descriptor_size: u32,
    routine_count: u16,
) -> u32 {
    let descriptor = process_memory_manager.new_native_ptr(memory, descriptor_size, true);
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
    if descriptor == 0 {
        return 0;
    }
    let ok = memory
        .write_u16_be(descriptor, PPC_MIXED_MODE_TRAP)
        .is_some()
        && memory
            .write_u8(descriptor + 2, PPC_ROUTINE_DESCRIPTOR_VERSION)
            .is_some()
        && memory.write_u8(descriptor + 3, 0).is_some()
        && memory.write_u32_be(descriptor + 4, 0).is_some()
        && memory.write_u8(descriptor + 8, 0).is_some()
        && memory.write_u8(descriptor + 9, 0).is_some()
        && memory
            .write_u16_be(descriptor + 10, routine_count)
            .is_some();
    if ok {
        descriptor
    } else {
        let _ = process_memory_manager.dispose_native_ptr(descriptor);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        0
    }
}

pub(super) fn ppc_write_routine_record(
    memory: &mut PpcSectionMem,
    record: u32,
    proc_info: u32,
    isa: u8,
    flags: u16,
    proc_descriptor: u32,
) -> bool {
    memory.write_u32_be(record, proc_info).is_some()
        && memory.write_u8(record + 4, 0).is_some()
        && memory
            .write_u8(record + PPC_ROUTINE_RECORD_ISA_OFFSET, isa)
            .is_some()
        && memory
            .write_u16_be(record + PPC_ROUTINE_RECORD_FLAGS_OFFSET, flags)
            .is_some()
        && memory
            .write_u32_be(
                record + PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
                proc_descriptor,
            )
            .is_some()
        && memory.write_u32_be(record + 12, 0).is_some()
        && memory.write_u32_be(record + 16, 0).is_some()
}

fn ppc_call_universal_proc_arguments(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    proc_info: u32,
) -> Result<Option<Vec<u32>>, ()> {
    match proc_info & PPC_PROCINFO_CALLING_CONVENTION_MASK {
        PPC_PROCINFO_PASCAL_STACK_BASED
        | PPC_PROCINFO_C_STACK_BASED
        | PPC_PROCINFO_THINK_C_STACK_BASED => {
            ppc_call_universal_proc_stack_arguments(cpu, memory, proc_info).map(Some)
        }
        PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED
        | PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED => {
            ppc_call_universal_proc_dispatched_stack_arguments(cpu, memory, proc_info).map(Some)
        }
        PPC_PROCINFO_REGISTER_BASED => {
            ppc_call_universal_proc_register_arguments(cpu, memory, proc_info).map(Some)
        }
        PPC_PROCINFO_SPECIAL_CASE => {
            ppc_call_universal_proc_special_case_arguments(cpu, memory, proc_info)
        }
        _ => Ok(None),
    }
}

fn ppc_call_universal_proc_selector(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    proc_info: u32,
) -> Result<Option<u32>, ()> {
    match proc_info & PPC_PROCINFO_CALLING_CONVENTION_MASK {
        PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED
        | PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED => {
            let selector_size = (proc_info >> PPC_PROCINFO_DISPATCHED_SELECTOR_SIZE_PHASE) & 0x03;
            if selector_size == PPC_PROCINFO_SIZE_NONE {
                Ok(None)
            } else {
                let selector = ppc_call_universal_proc_vararg(cpu, memory, 0)?;
                Ok(Some(ppc_procinfo_sized_word(selector, selector_size)))
            }
        }
        _ => Ok(None),
    }
}

pub(super) fn ppc_call_universal_proc_return_gpr3(proc_info: u32) -> PpcNativeReturnGpr3 {
    match proc_info & PPC_PROCINFO_CALLING_CONVENTION_MASK {
        PPC_PROCINFO_PASCAL_STACK_BASED
        | PPC_PROCINFO_C_STACK_BASED
        | PPC_PROCINFO_THINK_C_STACK_BASED
        | PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED
        | PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED => {
            match (proc_info >> PPC_PROCINFO_RESULT_SIZE_PHASE) & 0x03 {
                PPC_PROCINFO_SIZE_ONE => PpcNativeReturnGpr3::Mask(0x0000_00ff),
                PPC_PROCINFO_SIZE_TWO => PpcNativeReturnGpr3::Mask(0x0000_ffff),
                _ => PpcNativeReturnGpr3::Preserve,
            }
        }
        PPC_PROCINFO_REGISTER_BASED => {
            let result_location = (proc_info >> PPC_PROCINFO_REGISTER_RESULT_LOCATION_PHASE) & 0x1f;
            match result_location {
                PPC_PROCINFO_REGISTER_CCR_C | PPC_PROCINFO_REGISTER_CCR_X => {
                    PpcNativeReturnGpr3::XerCa
                }
                PPC_PROCINFO_REGISTER_CCR_V => PpcNativeReturnGpr3::XerOv,
                PPC_PROCINFO_REGISTER_CCR_Z => PpcNativeReturnGpr3::CrBit(PPC_CR0_EQ_BIT),
                PPC_PROCINFO_REGISTER_CCR_N => PpcNativeReturnGpr3::CrBit(PPC_CR0_LT_BIT),
                _ => match (proc_info >> PPC_PROCINFO_RESULT_SIZE_PHASE) & 0x03 {
                    PPC_PROCINFO_SIZE_ONE => PpcNativeReturnGpr3::Mask(0x0000_00ff),
                    PPC_PROCINFO_SIZE_TWO => PpcNativeReturnGpr3::Mask(0x0000_ffff),
                    _ => PpcNativeReturnGpr3::Preserve,
                },
            }
        }
        PPC_PROCINFO_SPECIAL_CASE => {
            use crate::mixed_mode::NativeSpecialCaseResult;

            match crate::mixed_mode::native_special_case_signature(proc_info)
                .map(|signature| signature.result)
            {
                Some(NativeSpecialCaseResult::Boolean) => PpcNativeReturnGpr3::Mask(0x0000_00ff),
                Some(NativeSpecialCaseResult::Word) => PpcNativeReturnGpr3::Mask(0x0000_ffff),
                Some(NativeSpecialCaseResult::Void) | None => PpcNativeReturnGpr3::Preserve,
            }
        }
        _ => PpcNativeReturnGpr3::Preserve,
    }
}

fn ppc_call_universal_proc_stack_arguments(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    proc_info: u32,
) -> Result<Vec<u32>, ()> {
    let mut args = Vec::new();
    for index in 0..PPC_PROCINFO_MAX_STACK_PARAMETERS {
        let shift = PPC_PROCINFO_STACK_PARAMETER_PHASE
            + u32::try_from(index).map_err(|_| ())? * PPC_PROCINFO_STACK_PARAMETER_WIDTH;
        let size_code = (proc_info >> shift) & 0x03;
        if size_code == PPC_PROCINFO_SIZE_NONE {
            break;
        }
        let value = ppc_call_universal_proc_vararg(cpu, memory, index)?;
        args.push(ppc_procinfo_sized_word(value, size_code));
    }
    Ok(args)
}

fn ppc_call_universal_proc_dispatched_stack_arguments(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    proc_info: u32,
) -> Result<Vec<u32>, ()> {
    let mut args = Vec::new();
    let mut source_index = 0usize;
    let selector_size = (proc_info >> PPC_PROCINFO_DISPATCHED_SELECTOR_SIZE_PHASE) & 0x03;
    if selector_size != PPC_PROCINFO_SIZE_NONE {
        let selector = ppc_call_universal_proc_vararg(cpu, memory, source_index)?;
        args.push(ppc_procinfo_sized_word(selector, selector_size));
        source_index = source_index.checked_add(1).ok_or(())?;
    }

    for index in 0..PPC_PROCINFO_MAX_DISPATCHED_STACK_PARAMETERS {
        let shift = PPC_PROCINFO_DISPATCHED_PARAMETER_PHASE
            + u32::try_from(index).map_err(|_| ())? * PPC_PROCINFO_STACK_PARAMETER_WIDTH;
        let size_code = (proc_info >> shift) & 0x03;
        if size_code == PPC_PROCINFO_SIZE_NONE {
            break;
        }
        let value = ppc_call_universal_proc_vararg(cpu, memory, source_index)?;
        args.push(ppc_procinfo_sized_word(value, size_code));
        source_index = source_index.checked_add(1).ok_or(())?;
    }
    Ok(args)
}

fn ppc_call_universal_proc_register_arguments(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    proc_info: u32,
) -> Result<Vec<u32>, ()> {
    let mut args = Vec::new();
    for index in 0..PPC_PROCINFO_MAX_REGISTER_PARAMETERS {
        let shift = PPC_PROCINFO_REGISTER_PARAMETER_PHASE
            + u32::try_from(index).map_err(|_| ())? * PPC_PROCINFO_REGISTER_PARAMETER_WIDTH;
        let field = (proc_info >> shift) & 0x1f;
        let size_code = field & PPC_PROCINFO_REGISTER_PARAMETER_SIZE_MASK;
        if size_code == PPC_PROCINFO_SIZE_NONE {
            break;
        }
        let _which_68k_register = (field >> PPC_PROCINFO_REGISTER_PARAMETER_WHICH_SHIFT)
            & PPC_PROCINFO_REGISTER_PARAMETER_WHICH_MASK;
        let value = ppc_call_universal_proc_vararg(cpu, memory, index)?;
        args.push(ppc_procinfo_sized_word(value, size_code));
    }
    Ok(args)
}

fn ppc_call_universal_proc_special_case_arguments(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    proc_info: u32,
) -> Result<Option<Vec<u32>>, ()> {
    let arg_count = crate::mixed_mode::native_special_case_signature(proc_info)
        .ok_or(())?
        .argument_count;
    let mut args = Vec::with_capacity(arg_count);
    for index in 0..arg_count {
        args.push(ppc_call_universal_proc_vararg(cpu, memory, index)?);
    }
    Ok(Some(args))
}

fn ppc_call_universal_proc_vararg(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    index: usize,
) -> Result<u32, ()> {
    if index < PPC_CALL_UNIVERSAL_PROC_REGISTER_VARARGS {
        return Ok(cpu.gpr[5 + index]);
    }

    let source_slot = PPC_CALL_UNIVERSAL_PROC_FIXED_WORD_PARAMETERS
        .checked_add(index)
        .ok_or(())?;
    let addr = ppc_parameter_area_slot_addr(cpu.gpr[1], source_slot).ok_or(())?;
    memory.read_u32_be(addr).ok_or(())
}

fn ppc_procinfo_sized_word(value: u32, size_code: u32) -> u32 {
    match size_code {
        PPC_PROCINFO_SIZE_ONE => value & 0x0000_00ff,
        PPC_PROCINFO_SIZE_TWO => value & 0x0000_ffff,
        PPC_PROCINFO_SIZE_FOUR => value,
        _ => 0,
    }
}

pub(super) fn ppc_parameter_area_slot_addr(sp: u32, slot: usize) -> Option<u32> {
    let slot = u32::try_from(slot).ok()?;
    let offset = PPC_PARAMETER_AREA_OFFSET.checked_add(slot.checked_mul(4)?)?;
    sp.checked_add(offset)
}

fn ppc_install_legacy_call_universal_proc_arguments(cpu: &mut PpcCpu) {
    for index in 0..PPC_CALL_UNIVERSAL_PROC_REGISTER_VARARGS {
        cpu.gpr[3 + index] = cpu.gpr[5 + index];
    }
}

pub(super) fn ppc_invoke_prepared_resource(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    calls: &SharedGuestCallStack,
    operation: crate::execution_kernel::GuestProcedureInvocation,
    final_pc: u32,
) -> Result<(), i16> {
    let procedure = operation.procedure;
    if procedure.isa != GuestIsa::PowerPc {
        return Err(PPC_PARAM_ERR);
    }
    let effect = GuestCallEffect::call_guest(
        GuestCallRequest::for_task(
            operation.task,
            GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: procedure.entry,
                rtoc: procedure.rtoc,
            },
        )
        .with_powerpc_arguments(operation.arguments),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            final_pc,
            cpu.gpr[2],
            ppc_call_universal_proc_return_gpr3(operation.caller_proc_info),
        ),
    );
    if !calls.activate_powerpc_effect_with_operation(cpu, memory, effect, None, None) {
        return Err(PPC_PARAM_ERR);
    }
    cpu.gpr[12] = procedure.original_pointer;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_prepare_resource_call(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    calls: &SharedGuestCallStack,
    heap_cursor: &mut u32,
    heap_limit: u32,
    connections: &mut Vec<PpcCfmConnection>,
    next_id: &mut u32,
    import_run_state: &mut PpcImportRunState,
    request: CfmResourcePreparation,
    selector: Option<u32>,
) -> PpcImportAction {
    let result = (|| {
        if calls.is_resource_preparation_pending(request.record) {
            return Err(PPC_FRAG_INIT_LOOP);
        }
        let mut descriptor_header = [0; 12];
        let mut original_record = [0; 20];
        memory
            .read_bytes_into(request.descriptor, &mut descriptor_header)
            .ok_or(PPC_FRAG_CORRUPT_ERR)?;
        memory
            .read_bytes_into(request.record, &mut original_record)
            .ok_or(PPC_FRAG_CORRUPT_ERR)?;
        let caller_proc_info = cpu.gpr[4];
        let mut arguments = ppc_call_universal_proc_arguments(cpu, memory, caller_proc_info)
            .map_err(|_| PPC_PARAM_ERR)?
            .ok_or(PPC_PARAM_ERR)?;
        if selector.is_some()
            && request.routine_flags & PPC_ROUTINE_FLAG_DONT_PASS_SELECTOR != 0
            && !arguments.is_empty()
        {
            arguments.remove(0);
        }
        let arguments = crate::execution_kernel::GuestArgumentValues::from_slice(&arguments)
            .ok_or(PPC_PARAM_ERR)?;
        let parameter_start = cpu.gpr[1].checked_add(24).ok_or(PPC_PARAM_ERR)?;
        if !memory.preflight_writable_range(
            parameter_start,
            arguments.as_slice().len().max(8) as u32 * 4,
        ) || !memory.preflight_writable_range(request.record + 6, 2)
            || !memory.preflight_writable_range(request.record + 8, 4)
        {
            return Err(PPC_PARAM_ERR);
        }
        let available = if let Some(handle) = manager.handle_for_ptr(request.descriptor) {
            let saved_error = manager
                .native_heap_state()
                .map(|heap| heap.last_mem_error)
                .unwrap_or(0);
            let size = manager.process_handle_size_from_master_pointer(handle, request.descriptor);
            manager.set_native_mem_error(saved_error);
            Some(
                size.ok_or(PPC_FRAG_CORRUPT_ERR)?
                    .checked_sub(
                        request
                            .fragment_address
                            .checked_sub(request.descriptor)
                            .ok_or(PPC_FRAG_CORRUPT_ERR)?,
                    )
                    .ok_or(PPC_FRAG_CORRUPT_ERR)?,
            )
        } else {
            None
        };
        let fragment = crate::cfm::fragment::read_resource_fragment(
            memory,
            request.fragment_address,
            available,
        )
        .ok_or(PPC_FRAG_CORRUPT_ERR)?;
        let id = *next_id;
        if id == 0 || id > PPC_CFM_MAIN_STUB_COUNT {
            return Err(PPC_FRAG_LIB_CONN_ERR);
        }
        let prepared = ppc_prepare_mem_fragment(
            &fragment,
            manager,
            memory,
            heap_cursor,
            heap_limit,
            import_run_state,
        )?;
        if prepared.main_addr == 0 {
            return Err(PPC_FRAG_CORRUPT_ERR);
        }
        *next_id = id + 1;
        let name = format!("resource routine ${:08X}", request.record);
        connections.push(PpcCfmConnection {
            id,
            library_name: name.clone(),
            main_addr: prepared.main_addr,
            init_addr: prepared.init_addr,
            term_addr: prepared.term_addr,
            exports: prepared.exports,
        });
        let operation = CfmResourceCall {
            task: calls.current_task(),
            id: CfmLoadId(id),
            preparation: request,
            descriptor_header,
            original_record,
            main_address: prepared.main_addr,
            arguments,
            caller_proc_info,
        };
        let activate = (|| {
            if prepared.init_addr == 0 {
                let operation = operation
                    .complete(0, connections, memory)
                    .map_err(|error| error.os_error())?;
                return ppc_invoke_prepared_resource(cpu, memory, calls, operation, cpu.lr);
            }
            let block = ppc_create_mem_fragment_init_block(
                Some(manager),
                memory,
                heap_cursor,
                heap_limit,
                id,
                request.fragment_address,
                fragment.len() as u32,
                &name,
            )?;
            let invocation = crate::cfm::initialization_invocation(
                memory,
                operation.task,
                prepared.init_addr,
                block,
            );
            let activated = invocation.is_ok_and(|invocation| {
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
                        PpcNativeReturnGpr3::Preserve,
                    ),
                );
                calls.activate_powerpc_effect_with_operation(
                    cpu,
                    memory,
                    effect,
                    Some(block),
                    Some(crate::guest_call::ManagerContinuation::Cfm(
                        CfmOperation::Resource(operation),
                    )),
                )
            });
            if !activated {
                manager.release_native_scratch(block);
                return Err(PPC_FRAG_CORRUPT_ERR);
            }
            cpu.gpr[12] = prepared.init_addr;
            Ok(())
        })();
        if activate.is_err() {
            connections.retain(|connection| connection.id != id);
        }
        activate
    })();
    match result {
        Ok(()) => PpcImportAction::Continue,
        Err(error) => PpcImportAction::Return(ppc_i16_result(error)),
    }
}

fn ppc_call_universal_proc(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    toolbox_startup: &mut PpcToolboxStartupState,
    raw_isa: GuestIsa,
) -> Option<PpcImportAction> {
    let proc_ptr = cpu.gpr[3];
    let proc_info = cpu.gpr[4];
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] CallUniversalProc upp=${proc_ptr:08X} words=({:08X?},{:08X?}) procInfo=${proc_info:08X} sp=${:08X}",
            memory.read_u32_be(proc_ptr),
            memory.read_u32_be(proc_ptr.wrapping_add(4)),
            cpu.gpr[1],
        );
    }
    let final_pc = cpu.lr;
    let restore_rtoc = cpu.gpr[2];
    let return_gpr3 = ppc_call_universal_proc_return_gpr3(proc_info);
    let selector = ppc_call_universal_proc_selector(cpu, memory, proc_info).ok()?;
    let arguments = ppc_call_universal_proc_arguments(cpu, memory, proc_info).ok()?;
    if memory.read_u32_be(proc_ptr) == Some(0x40c0_007c)
        && memory.read_u32_be(proc_ptr.wrapping_add(4)) == Some(0x0700_4e75)
    {
        // Canonical 68K critical-section UPP:
        //   MOVE.W SR,D0; ORI.W #$0700,SR; RTS
        // Native PowerPC runtimes invoke it through Mixed Mode to mask
        // interrupts and retain the previous SR. Systemless has no guest
        // interrupt priority while executing PPC code, so return a stable
        // unmasked user SR token for the matching restore shim.
        return Some(PpcImportAction::Return(0));
    }
    if memory.read_u32_be(proc_ptr) == Some(0x46c0_4e75) {
        // Matching 68K restore UPP: MOVE.W D0,SR; RTS. The PPC HLE never
        // changed an interrupt mask, so consuming the saved token is a no-op.
        return Some(PpcImportAction::ReturnPreserve);
    }
    // System owners identify native transition vectors and direct 680x0
    // gateways explicitly; read-only storage alone cannot select an ISA.
    // Inside Macintosh: PowerPC System Software (1994), pp. 1-27--1-28,
    // 2-42--2-43.
    let raw_isa = memory.system_code_isa(proc_ptr).unwrap_or(raw_isa);
    let target = resolve_guest_procedure(
        memory,
        proc_ptr,
        restore_rtoc,
        selector,
        GuestIsa::PowerPc,
        raw_isa,
    )?;
    match target.isa {
        GuestIsa::PowerPc => {
            memory.read_u32_be(target.entry)?;
        }
        GuestIsa::M68k => {
            memory.read_u16_be(target.entry)?;
        }
    }

    match target.isa {
        GuestIsa::PowerPc => {
            match arguments {
                Some(mut args) => {
                    if selector.is_some()
                        && (target.routine_flags & PPC_ROUTINE_FLAG_DONT_PASS_SELECTOR) != 0
                        && !args.is_empty()
                    {
                        args.remove(0);
                    }
                    install_powerpc_call_arguments(cpu, memory, &args)?
                }
                None => ppc_install_legacy_call_universal_proc_arguments(cpu),
            }
            Some(
                GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: target.entry,
                        rtoc: target.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        final_pc,
                        restore_rtoc,
                        return_gpr3,
                    ),
                )
                .into_ppc_import_action()?,
            )
        }
        GuestIsa::M68k => ppc_begin_m68k_universal_proc(
            cpu,
            Some(process_memory_manager),
            memory,
            heap_cursor,
            heap_limit,
            toolbox_startup,
            target,
            proc_info,
            selector,
            arguments?,
            final_pc,
            return_gpr3,
        ),
    }
}

pub(super) const PPC_MIXED_MODE_M68K_GATEWAY_SIZE: u32 = 52;
pub(super) const PPC_MIXED_MODE_M68K_RETURN_OFFSET: u32 = 50;
pub(super) const PPC_MIXED_MODE_M68K_STACK_SIZE: u32 = 64 * 1024;

pub(super) fn ppc_mixed_mode_m68k_storage(
    mut process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    storage: &SharedProcessMixedModeM68kState,
) -> Option<(u32, u32)> {
    match storage.storage_pair() {
        (0, 0) => {}
        (gateway, stack_top) if gateway != 0 && stack_top != 0 => {
            return Some((gateway, stack_top));
        }
        _ => return None,
    }

    // The gateway and stack are one process-owned allocation transaction. Do
    // not expose either address until both allocations and the gateway's
    // return marker have succeeded. Inside Macintosh: PowerPC System Software
    // (1994), pp. 2-12--2-20.
    let initial_heap_cursor = *heap_cursor;
    let native_snapshot = process_memory_manager
        .as_deref()
        .map(ProcessNativeMemoryManager::detached_clone);
    let pair = (|| {
        let gateway = if let Some(memory_manager) = process_memory_manager.as_deref_mut() {
            ppc_process_heap_alloc(
                memory_manager,
                memory,
                heap_cursor,
                PPC_MIXED_MODE_M68K_GATEWAY_SIZE,
                true,
            )
        } else {
            ppc_heap_alloc(
                memory,
                heap_cursor,
                heap_limit,
                PPC_MIXED_MODE_M68K_GATEWAY_SIZE,
                true,
            )
        };
        if gateway == 0 {
            return None;
        }
        memory.write_u16_be(
            gateway.checked_add(PPC_MIXED_MODE_M68K_RETURN_OFFSET)?,
            0x4e71,
        )?;

        let stack_base = if let Some(memory_manager) = process_memory_manager.as_deref_mut() {
            ppc_process_heap_alloc(
                memory_manager,
                memory,
                heap_cursor,
                PPC_MIXED_MODE_M68K_STACK_SIZE,
                true,
            )
        } else {
            ppc_heap_alloc(
                memory,
                heap_cursor,
                heap_limit,
                PPC_MIXED_MODE_M68K_STACK_SIZE,
                true,
            )
        };
        if stack_base == 0 {
            return None;
        }
        let stack_top = stack_base.checked_add(PPC_MIXED_MODE_M68K_STACK_SIZE)?;
        Some((gateway, stack_top))
    })();

    let Some((gateway, stack_top)) = pair else {
        *heap_cursor = initial_heap_cursor;
        if let (Some(memory_manager), Some(snapshot)) =
            (process_memory_manager.as_deref_mut(), native_snapshot)
        {
            memory_manager.restore_native_snapshot(snapshot);
        }
        let allocation_limit = process_memory_manager
            .as_deref()
            .and_then(|memory_manager| {
                memory_manager
                    .native_heap_state()
                    .map(|heap| memory_manager.native_allocation_limit(heap.heap_limit))
            })
            .unwrap_or(heap_limit);
        ppc_update_zone_free_bytes(memory, *heap_cursor, allocation_limit);
        return None;
    };

    storage.set_storage(gateway, stack_top);
    Some((gateway, stack_top))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_begin_m68k_universal_proc(
    cpu: &PpcCpu,
    process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    startup: &mut PpcToolboxStartupState,
    target: GuestProcedure,
    proc_info: u32,
    selector: Option<u32>,
    arguments: Vec<u32>,
    final_pc: u32,
    return_gpr3: PpcNativeReturnGpr3,
) -> Option<PpcImportAction> {
    ppc_begin_m68k_universal_proc_inner(
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        startup,
        target,
        proc_info,
        selector,
        arguments,
        final_pc,
        return_gpr3,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_begin_m68k_universal_proc_with_operation(
    cpu: &PpcCpu,
    process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    startup: &mut PpcToolboxStartupState,
    target: GuestProcedure,
    proc_info: u32,
    selector: Option<u32>,
    arguments: Vec<u32>,
    final_pc: u32,
    return_gpr3: PpcNativeReturnGpr3,
    operation: crate::guest_call::ManagerContinuation,
) -> Option<PpcImportAction> {
    ppc_begin_m68k_universal_proc_inner(
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        startup,
        target,
        proc_info,
        selector,
        arguments,
        final_pc,
        return_gpr3,
        Some(operation),
    )
}

#[allow(clippy::too_many_arguments)]
fn ppc_begin_m68k_universal_proc_inner(
    cpu: &PpcCpu,
    process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    startup: &mut PpcToolboxStartupState,
    target: GuestProcedure,
    proc_info: u32,
    selector: Option<u32>,
    arguments: Vec<u32>,
    final_pc: u32,
    return_gpr3: PpcNativeReturnGpr3,
    operation: Option<crate::guest_call::ManagerContinuation>,
) -> Option<PpcImportAction> {
    use crate::guest_call::{M68kRegisterState, M68kResultSource};

    // The 68k side follows the stack/register layouts in Inside Macintosh:
    // PowerPC System Software (1994), pp. 1-42--1-43 and 2-12--2-20. In
    // particular, Pascal results precede left-to-right parameters, MPW C and
    // THINK C parameters are pushed right-to-left, and THINK C alone places a
    // one-byte argument in the high byte of its aligned stack word. Special
    // cases use the bespoke layouts on pp. 2-30--2-32 and the native
    // prototypes in Universal Interfaces 3.4 TextEdit.h, AppleTalk.h,
    // Events.h, and Menus.h; Apple Technical Note 85 defines the GetNextEvent
    // filter's duplicated Boolean result.
    let convention = proc_info & PPC_PROCINFO_CALLING_CONVENTION_MASK;
    let result_size = if convention == PPC_PROCINFO_SPECIAL_CASE {
        PPC_PROCINFO_SIZE_NONE
    } else {
        (proc_info >> PPC_PROCINFO_RESULT_SIZE_PHASE) & 0x03
    };
    let mut registers = M68kRegisterState::default();
    // Native processes expose their compatibility globals through the
    // synthetic mini-A5 world returned by SetCurrentA5/LMGetCurrentA5.
    registers.address[5] = PPC_DATA_BASE;
    let mut stack_arguments = Vec::<(u32, u32)>::new();
    let mut special_stack = Vec::new();
    let mut special_result = None;
    let mut special_stack_result = false;

    match convention {
        PPC_PROCINFO_SPECIAL_CASE => {
            let signature = crate::mixed_mode::native_special_case_signature(proc_info)?;
            if arguments.len() != signature.argument_count {
                return None;
            }
            let selector = (proc_info >> crate::mixed_mode::special_case::SELECTOR_PHASE)
                & crate::mixed_mode::special_case::SELECTOR_MASK;
            let argument_record = crate::guest_call::PowerPcArguments::from_slice(&arguments)?;
            special_result = Some((u8::try_from(selector).ok()?, argument_record));
            use crate::mixed_mode::special_case;
            match selector {
                special_case::HIGH_HOOK => {
                    let rect = arguments[0];
                    for offset in 0..8 {
                        special_stack.push(memory.read_u8(rect.checked_add(offset)?)?);
                    }
                    registers.address[3] = arguments[1];
                }
                special_case::EOL_HOOK => {
                    registers.data[0] = arguments[0] & 0xff;
                    registers.address[3] = arguments[1];
                    registers.address[4] = arguments[2];
                }
                special_case::WIDTH_HOOK => {
                    registers.data[0] = arguments[0] & 0xffff;
                    registers.data[1] = arguments[1] & 0xffff;
                    registers.address[0] = arguments[2];
                    registers.address[3] = arguments[3];
                    registers.address[4] = arguments[4];
                }
                special_case::NWIDTH_HOOK => {
                    registers.data[0] = arguments[0] & 0xffff;
                    registers.data[1] = arguments[1] & 0xffff;
                    registers.data[2] = ((arguments[3] & 0xffff) << 16) | (arguments[2] & 0xffff);
                    registers.address[0] = arguments[4];
                    registers.address[2] = arguments[5];
                    registers.address[3] = arguments[6];
                    registers.address[4] = arguments[7];
                }
                special_case::DRAW_HOOK => {
                    registers.data[0] = arguments[0] & 0xffff;
                    registers.data[1] = arguments[1] & 0xffff;
                    registers.address[0] = arguments[2];
                    registers.address[3] = arguments[3];
                    registers.address[4] = arguments[4];
                }
                special_case::HIT_TEST_HOOK => {
                    registers.data[0] = arguments[0] & 0xffff;
                    registers.data[1] = arguments[1] & 0xffff;
                    registers.data[2] = arguments[2] & 0xffff;
                    registers.address[0] = arguments[3];
                    registers.address[3] = arguments[4];
                    registers.address[4] = arguments[5];
                }
                special_case::TE_FIND_WORD => {
                    registers.data[0] = arguments[0] & 0xffff;
                    registers.data[2] = arguments[1] & 0xffff;
                    registers.address[3] = arguments[2];
                    registers.address[4] = arguments[3];
                }
                special_case::PROTOCOL_HANDLER => {
                    registers.address[0..5].copy_from_slice(&arguments[0..5]);
                    registers.data[1] = arguments[5] & 0xffff;
                }
                special_case::SOCKET_LISTENER => {
                    registers.address[0..5].copy_from_slice(&arguments[0..5]);
                    registers.data[0] = arguments[5] & 0xff;
                    registers.data[1] = arguments[6] & 0xffff;
                }
                special_case::TE_RECALC => {
                    registers.address[3] = arguments[0];
                    registers.data[7] = arguments[1] & 0xffff;
                }
                special_case::TE_DO_TEXT => {
                    registers.address[3] = arguments[0];
                    registers.data[3] = arguments[1] & 0xffff;
                    registers.data[4] = arguments[2] & 0xffff;
                    registers.data[7] = arguments[3] & 0xffff;
                }
                special_case::GNE_FILTER_PROC => {
                    let initial_result = memory.read_u8(arguments[1])?;
                    registers.address[1] = arguments[0];
                    registers.data[0] = u32::from(initial_result);
                    special_stack.extend_from_slice(&u16::from(initial_result).to_be_bytes());
                    special_stack_result = true;
                }
                special_case::MBAR_HOOK => {
                    special_stack.extend_from_slice(&arguments[0].to_be_bytes());
                }
                _ => return None,
            }
        }
        PPC_PROCINFO_REGISTER_BASED => {
            let result_register = (proc_info >> PPC_PROCINFO_REGISTER_RESULT_LOCATION_PHASE) & 0x1f;
            if result_size == PPC_PROCINFO_SIZE_NONE
                && matches!(
                    result_register,
                    PPC_PROCINFO_REGISTER_CCR_C
                        | PPC_PROCINFO_REGISTER_CCR_V
                        | PPC_PROCINFO_REGISTER_CCR_Z
                        | PPC_PROCINFO_REGISTER_CCR_N
                        | PPC_PROCINFO_REGISTER_CCR_X
                )
            {
                return None;
            }
            if arguments.len() > PPC_PROCINFO_MAX_REGISTER_PARAMETERS {
                return None;
            }
            for (index, value) in arguments.into_iter().enumerate() {
                let shift = PPC_PROCINFO_REGISTER_PARAMETER_PHASE
                    + u32::try_from(index).ok()? * PPC_PROCINFO_REGISTER_PARAMETER_WIDTH;
                let field = (proc_info >> shift) & 0x1f;
                let register = (field >> PPC_PROCINFO_REGISTER_PARAMETER_WHICH_SHIFT)
                    & PPC_PROCINFO_REGISTER_PARAMETER_WHICH_MASK;
                ppc_set_m68k_input_register(&mut registers, register, value)?;
            }
        }
        PPC_PROCINFO_PASCAL_STACK_BASED
        | PPC_PROCINFO_C_STACK_BASED
        | PPC_PROCINFO_THINK_C_STACK_BASED => {
            let sizes = ppc_m68k_stack_argument_sizes(proc_info, false)?;
            if sizes.len() != arguments.len() {
                return None;
            }
            stack_arguments.extend(arguments.into_iter().zip(sizes));
        }
        PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED
        | PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED
        | PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED => {
            let sizes = ppc_m68k_stack_argument_sizes(proc_info, true)?;
            if sizes.len() != arguments.len() {
                return None;
            }
            let mut pairs: Vec<_> = arguments.into_iter().zip(sizes).collect();
            let selector_size = (proc_info >> PPC_PROCINFO_DISPATCHED_SELECTOR_SIZE_PHASE) & 0x03;
            let selector_pair = if selector_size == PPC_PROCINFO_SIZE_NONE {
                None
            } else if pairs.is_empty() {
                return None;
            } else {
                Some(pairs.remove(0))
            };
            let pass_selector = (target.routine_flags & PPC_ROUTINE_FLAG_DONT_PASS_SELECTOR) == 0;
            if pass_selector {
                if let Some((value, size)) = selector_pair {
                    match convention {
                        PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED
                        | PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED => {
                            registers.data[0] = value;
                        }
                        PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED => {
                            registers.data[1] = value;
                        }
                        PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED => {
                            pairs.insert(0, (value, size));
                        }
                        _ => unreachable!(),
                    }
                } else if selector.is_some() {
                    return None;
                }
            }
            stack_arguments = pairs;
        }
        _ => return None,
    }

    let (gateway, stack_top) = ppc_mixed_mode_m68k_storage(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        &startup.mixed_mode_m68k,
    )?;
    let return_pc = gateway.checked_add(PPC_MIXED_MODE_M68K_RETURN_OFFSET)?;
    if stack_top < 4 {
        return None;
    }

    let pascal = matches!(
        convention,
        PPC_PROCINFO_PASCAL_STACK_BASED
            | PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED
            | PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED
            | PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED
    );
    let think_c = convention == PPC_PROCINFO_THINK_C_STACK_BASED;
    let mut sp = stack_top;
    let mut result = special_result.map(|(selector, arguments)| M68kResultSource::SpecialCase {
        selector,
        arguments,
        stack_result: None,
    });
    let pascal_result_sp = if pascal && result_size != PPC_PROCINFO_SIZE_NONE {
        let value_address = ppc_reserve_m68k_stack_value(memory, &mut sp, result_size, false)?;
        result = Some(M68kResultSource::Memory {
            address: value_address,
            size: ppc_procinfo_value_size(result_size)?,
        });
        Some(sp)
    } else {
        None
    };

    let argument_bytes: u32 = stack_arguments.iter().try_fold(0u32, |total, (_, size)| {
        total.checked_add(ppc_m68k_stack_slot_size(*size)?)
    })?;
    if matches!(
        convention,
        PPC_PROCINFO_C_STACK_BASED
            | PPC_PROCINFO_THINK_C_STACK_BASED
            | PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED
    ) {
        for (value, size) in stack_arguments.into_iter().rev() {
            ppc_push_m68k_stack_value(memory, &mut sp, value, size, think_c)?;
        }
    } else {
        for (value, size) in stack_arguments {
            ppc_push_m68k_stack_value(memory, &mut sp, value, size, think_c)?;
        }
    }
    if !special_stack.is_empty() {
        sp = sp.checked_sub(u32::try_from(special_stack.len()).ok()?)?;
        for (offset, byte) in special_stack.into_iter().enumerate() {
            memory.write_u8(sp.checked_add(u32::try_from(offset).ok()?)?, byte)?;
        }
    }
    let parameter_sp = sp;
    if special_stack_result {
        let Some(M68kResultSource::SpecialCase { stack_result, .. }) = result.as_mut() else {
            return None;
        };
        *stack_result = Some(parameter_sp);
    }
    sp = sp.checked_sub(4)?;
    memory.write_u32_be(sp, return_pc)?;
    let initial_sp = sp;

    let final_sp = if pascal {
        pascal_result_sp.unwrap_or(stack_top)
    } else {
        parameter_sp
    };
    if pascal && final_sp != initial_sp.checked_add(4 + argument_bytes)? {
        return None;
    }
    if !pascal && result_size != PPC_PROCINFO_SIZE_NONE {
        result = match convention {
            PPC_PROCINFO_C_STACK_BASED
            | PPC_PROCINFO_THINK_C_STACK_BASED
            | PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED => Some(M68kResultSource::Data(0)),
            PPC_PROCINFO_REGISTER_BASED => {
                let register = (proc_info >> PPC_PROCINFO_REGISTER_RESULT_LOCATION_PHASE) & 0x1f;
                Some(ppc_m68k_result_register(register)?)
            }
            _ => return None,
        };
    }

    let target = crate::guest_call::GuestCallTarget {
        isa: target.isa,
        entry: target.entry,
        rtoc: target.rtoc,
    };
    let submitted = if let Some(operation) = operation {
        startup
            .execution
            .calls()
            .begin_powerpc_to_m68k_with_operation(
                target,
                target.entry,
                initial_sp,
                return_pc,
                final_sp,
                registers,
                result,
                final_pc,
                cpu.gpr[2],
                return_gpr3,
                operation,
            )
    } else {
        startup.execution.calls().begin_powerpc_to_m68k(
            target,
            target.entry,
            initial_sp,
            return_pc,
            final_sp,
            registers,
            result,
            final_pc,
            cpu.gpr[2],
            return_gpr3,
        )
    };
    if !submitted {
        return None;
    }
    Some(PpcImportAction::Halt)
}

fn ppc_m68k_stack_argument_sizes(proc_info: u32, dispatched: bool) -> Option<Vec<u32>> {
    let mut sizes = Vec::new();
    if dispatched {
        let selector_size = (proc_info >> PPC_PROCINFO_DISPATCHED_SELECTOR_SIZE_PHASE) & 0x03;
        if selector_size != PPC_PROCINFO_SIZE_NONE {
            sizes.push(selector_size);
        }
        for index in 0..PPC_PROCINFO_MAX_DISPATCHED_STACK_PARAMETERS {
            let shift = PPC_PROCINFO_DISPATCHED_PARAMETER_PHASE
                + u32::try_from(index).ok()? * PPC_PROCINFO_STACK_PARAMETER_WIDTH;
            let size = (proc_info >> shift) & 0x03;
            if size == PPC_PROCINFO_SIZE_NONE {
                break;
            }
            sizes.push(size);
        }
    } else {
        for index in 0..PPC_PROCINFO_MAX_STACK_PARAMETERS {
            let shift = PPC_PROCINFO_STACK_PARAMETER_PHASE
                + u32::try_from(index).ok()? * PPC_PROCINFO_STACK_PARAMETER_WIDTH;
            let size = (proc_info >> shift) & 0x03;
            if size == PPC_PROCINFO_SIZE_NONE {
                break;
            }
            sizes.push(size);
        }
    }
    Some(sizes)
}

fn ppc_set_m68k_input_register(
    registers: &mut crate::guest_call::M68kRegisterState,
    register: u32,
    value: u32,
) -> Option<()> {
    match register {
        0..=3 => registers.data[usize::try_from(register).ok()?] = value,
        4..=7 => registers.address[usize::try_from(register - 4).ok()?] = value,
        _ => return None,
    }
    Some(())
}

fn ppc_m68k_result_register(register: u32) -> Option<crate::guest_call::M68kResultSource> {
    use crate::guest_call::M68kResultSource;

    match register {
        0..=3 => Some(M68kResultSource::Data(u8::try_from(register).ok()?)),
        4..=7 => Some(M68kResultSource::Address(u8::try_from(register - 4).ok()?)),
        8..=11 => Some(M68kResultSource::Data(u8::try_from(register - 4).ok()?)),
        12..=14 => Some(M68kResultSource::Address(u8::try_from(register - 8).ok()?)),
        // Inside Macintosh: PowerPC System Software (1994), p. 2-14 notes
        // that the emulator-return transition clears the low five CCR bits.
        PPC_PROCINFO_REGISTER_CCR_C
        | PPC_PROCINFO_REGISTER_CCR_V
        | PPC_PROCINFO_REGISTER_CCR_Z
        | PPC_PROCINFO_REGISTER_CCR_N
        | PPC_PROCINFO_REGISTER_CCR_X => None,
        _ => None,
    }
}

fn ppc_procinfo_value_size(size: u32) -> Option<u8> {
    match size {
        PPC_PROCINFO_SIZE_ONE => Some(1),
        PPC_PROCINFO_SIZE_TWO => Some(2),
        PPC_PROCINFO_SIZE_FOUR => Some(4),
        _ => None,
    }
}

fn ppc_m68k_stack_slot_size(size: u32) -> Option<u32> {
    match size {
        PPC_PROCINFO_SIZE_ONE | PPC_PROCINFO_SIZE_TWO => Some(2),
        PPC_PROCINFO_SIZE_FOUR => Some(4),
        _ => None,
    }
}

fn ppc_reserve_m68k_stack_value(
    memory: &mut PpcSectionMem,
    sp: &mut u32,
    size: u32,
    high_byte: bool,
) -> Option<u32> {
    let slot_size = ppc_m68k_stack_slot_size(size)?;
    *sp = sp.checked_sub(slot_size)?;
    match size {
        PPC_PROCINFO_SIZE_ONE => {
            memory.write_u16_be(*sp, 0)?;
            Some(if high_byte { *sp } else { sp.checked_add(1)? })
        }
        PPC_PROCINFO_SIZE_TWO => {
            memory.write_u16_be(*sp, 0)?;
            Some(*sp)
        }
        PPC_PROCINFO_SIZE_FOUR => {
            memory.write_u32_be(*sp, 0)?;
            Some(*sp)
        }
        _ => None,
    }
}

fn ppc_push_m68k_stack_value(
    memory: &mut PpcSectionMem,
    sp: &mut u32,
    value: u32,
    size: u32,
    high_byte: bool,
) -> Option<()> {
    let address = ppc_reserve_m68k_stack_value(memory, sp, size, high_byte)?;
    match size {
        PPC_PROCINFO_SIZE_ONE => memory.write_u8(address, value as u8)?,
        PPC_PROCINFO_SIZE_TWO => memory.write_u16_be(address, value as u16)?,
        PPC_PROCINFO_SIZE_FOUR => memory.write_u32_be(address, value)?,
        _ => return None,
    }
    Some(())
}

fn ppc_call_os_trap_universal_proc(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Option<PpcImportAction> {
    if (cpu.gpr[4] & PPC_PROCINFO_CALLING_CONVENTION_MASK) != PPC_PROCINFO_REGISTER_BASED {
        return None;
    }
    ppc_call_universal_proc(
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        toolbox_startup,
        // Inside Macintosh: PowerPC System Software (1994), pp. 1-67 and
        // 2-42--2-43: unlike an ordinary native CallUniversalProc raw
        // pointer, an OS-trap universal pointer may be direct 680x0 code.
        GuestIsa::M68k,
    )
}
