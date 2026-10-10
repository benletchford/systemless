//! Typed Memory Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcMemoryDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) native_heap_ceiling: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) aliases: &'a mut Vec<PpcAliasRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) menu_generations: &'a mut HashMap<u32, u64>,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_memory_import(
    context: PpcMemoryDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcMemoryDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        native_heap_ceiling,
        last_mem_error,
        handles,
        aliases,
        vfs_resources,
        menu_generations,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::NewString => {
            // NewString copies the Pascal string into a relocatable block
            // sized to its actual length. Inside Macintosh I (1985), I-468.
            let source = cpu.gpr[3];
            let length = memory.read_u8(source)? as u32;
            let bytes = ppc_memory_read_bytes(memory, source, length + 1)?;
            let result = process_memory_manager.new_handle(
                ProcessNewHandleRequest::new((length + 1) as i32, false, ProcessHandleHeap::Current),
                ProcessNewHandleBackend::Native(memory),
            );
            *last_mem_error = result.error;
            ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
            if result.succeeded() {
                if let Some(record) = process_memory_manager.native_allocation(result.handle) {
                    handles.push(record);
                }
                let data = memory.read_u32_be(result.handle)?;
                memory.write_bytes(data, &bytes)?;
            }
            Some(PpcImportAction::Return(result.handle))
        }
        PpcImportDispatcherTarget::HoldMemory => {
            // HoldMemory (Ptr, Size): OSErr. Native guest memory is resident
            // for the process lifetime, so no page pin is required.
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::UnholdMemory => {
            // UnholdMemory (Ptr, Size): OSErr. Guest pages are backed by
            // resident process memory, so releasing a pin is a no-op.
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::NewPtr { clear } => {
            let size = cpu.gpr[3];
            let ptr = process_memory_manager.new_native_ptr(memory, size, clear);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] {} size={} lr=${:08X} -> ${:08X} heap=${:08X}..${:08X} err={}",
                    binding.symbol_name, size, cpu.lr, ptr, *heap_cursor, heap_limit, *last_mem_error
                );
            }
            Some(PpcImportAction::Return(ptr))
        }
        PpcImportDispatcherTarget::DisposePtr => {
            let ptr = cpu.gpr[3];
            if ppc_hle_trace_enabled() {
                eprintln!("[PPC-TRACE] DisposePtr ${ptr:08X} lr=${:08X}", cpu.lr);
            }
            if process_memory_manager.dispose_native_ptr(ptr).is_none() {
                process_memory_manager.dispose_classic_ptr_from_native_import(ptr);
            }
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetPtrSize => {
            let size = process_memory_manager.process_ptr_size_for_native_import(cpu.gpr[3]);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(size))
        }
        PpcImportDispatcherTarget::SetPtrSize => {
            *last_mem_error = process_memory_manager
                .set_process_ptr_size_for_native_import(memory, cpu.gpr[3], cpu.gpr[4]);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RecoverHandle => {
            let handle = process_memory_manager
                .recover_handle_from_master_pointer(cpu.gpr[3], |handle| {
                    PpcMemory::read_u32_be(memory, handle)
                })
                .unwrap_or(0);
            if handle != 0 {
                ppc_apply_process_native_handle(process_memory_manager, handles, handle);
            }
            Some(PpcImportAction::Return(handle))
        }
        PpcImportDispatcherTarget::BlockMove => {
            if ppc_hle_trace_enabled()
                && cpu.gpr[4] < PPC_MAIN_CTABLE + PPC_MAIN_CTABLE_SIZE
                && cpu.gpr[4].saturating_add(cpu.gpr[5]) > PPC_MAIN_CTABLE
            {
                eprintln!(
                    "[PPC-TRACE] BlockMove main-ctable src=${:08X} dst=${:08X} size={}",
                    cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]
                );
            }
            ppc_block_move(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::BlockZero => {
            ppc_block_zero(cpu, memory);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PtrToHand => {
            let source_ptr = cpu.gpr[3];
            let destination_handle_ptr = cpu.gpr[4];
            let size = cpu.gpr[5];
            let result = if destination_handle_ptr == 0
                || !ppc_memory_can_write_bytes(memory, destination_handle_ptr, 4)
            {
                PPC_PARAM_ERR
            } else if let Some(bytes) = ppc_memory_read_bytes(memory, source_ptr, size) {
                let handle = process_memory_manager.copy_bytes_to_new_native_handle(memory, &bytes);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                ppc_apply_process_native_handle(process_memory_manager, handles, handle);
                if handle == 0 {
                    let _ = memory.write_u32_be(destination_handle_ptr, 0);
                    *last_mem_error
                } else if memory
                    .write_u32_be(destination_handle_ptr, handle)
                    .is_none()
                {
                    process_memory_manager.set_native_mem_error(PPC_PARAM_ERR);
                    PPC_PARAM_ERR
                } else {
                    PPC_NO_ERR
                }
            } else {
                let _ = memory.write_u32_be(destination_handle_ptr, 0);
                PPC_PARAM_ERR
            };
            process_memory_manager.set_native_mem_error(result);
            *last_mem_error = result;
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::PtrToXHand => {
            // Inside Macintosh: Memory (1992), pp. 2-61--2-62: replace the
            // contents of an existing relocatable block without changing its
            // master pointer.
            let source_ptr = cpu.gpr[3];
            let destination_handle = cpu.gpr[4];
            let size = cpu.gpr[5];
            let result = if (size as i32) < 0 {
                PPC_MEM_FULL_ERR
            } else if destination_handle == 0 {
                PPC_NIL_HANDLE_ERR
            } else if let Some(bytes) = ppc_memory_read_bytes(memory, source_ptr, size) {
                let resize_result =
                    process_memory_manager.set_native_handle_size(memory, destination_handle, size);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                if resize_result != PPC_NO_ERR {
                    resize_result
                } else if bytes.is_empty() {
                    PPC_NO_ERR
                } else if memory
                    .read_u32_be(destination_handle)
                    .and_then(|ptr| memory.write_bytes(ptr, &bytes))
                    .is_some()
                {
                    PPC_NO_ERR
                } else {
                    PPC_PARAM_ERR
                }
            } else {
                PPC_PARAM_ERR
            };
            process_memory_manager.set_native_mem_error(result);
            *last_mem_error = result;
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::HandToHand => {
            let result = {
                let handle_variable = cpu.gpr[3];
                let result = if handle_variable == 0
                    || !ppc_memory_can_write_bytes(memory, handle_variable, 4)
                {
                    PPC_PARAM_ERR
                } else if let Some(source_handle) = memory.read_u32_be(handle_variable) {
                    // Inside Macintosh: Memory (1992), pp. 2-62--2-64: resolve
                    // process-owned handles through the canonical manager so
                    // HandToHand has identical semantics across CPU adapters.
                    match process_memory_manager
                        .copy_process_handle_from_native_import(memory, source_handle)
                    {
                        Ok(copy) => {
                            // The caller range was preflighted above. Process
                            // allocation may append writable mappings, but it
                            // cannot remove or downgrade one while this
                            // serialized import runs, so publication cannot
                            // fail after the copy has been committed.
                            memory
                                .write_u32_be(handle_variable, copy)
                                .expect("preflighted Handle variable remains writable");
                            ppc_apply_process_native_allocator(
                                process_memory_manager,
                                memory,
                                heap_cursor,
                                last_mem_error,
                            );
                            ppc_apply_process_native_handle(process_memory_manager, handles, copy);
                            PPC_NO_ERR
                        }
                        Err(error) if source_handle == PPC_MAIN_CTABLE_HANDLE => {
                            if let Some(source) = ppc_system_handle_record(memory, source_handle) {
                                if let Some(bytes) =
                                    ppc_memory_read_bytes(memory, source.ptr, source.size)
                                {
                                    let copy = process_memory_manager
                                        .copy_bytes_to_new_native_handle(memory, &bytes);
                                    if copy == 0 {
                                        process_memory_manager
                                            .native_heap_state()
                                            .map(|heap| heap.last_mem_error)
                                            .unwrap_or(PPC_MEM_FULL_ERR)
                                    } else {
                                        memory
                                            .write_u32_be(handle_variable, copy)
                                            .expect("preflighted Handle variable remains writable");
                                        ppc_apply_process_native_allocator(
                                            process_memory_manager,
                                            memory,
                                            heap_cursor,
                                            last_mem_error,
                                        );
                                        ppc_apply_process_native_handle(
                                            process_memory_manager,
                                            handles,
                                            copy,
                                        );
                                        PPC_NO_ERR
                                    }
                                } else {
                                    PPC_PARAM_ERR
                                }
                            } else {
                                error
                            }
                        }
                        Err(error) => error,
                    }
                } else {
                    PPC_PARAM_ERR
                };
                process_memory_manager.set_native_mem_error(result);
                result
            };
            *last_mem_error = result;
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::PtrAndHand => {
            // Inside Macintosh: Memory (1992), pp. 2-65--2-66; Memory
            // Errata (August 1998): the source may be any memory address.
            // Snapshot before resizing so a source inside the destination
            // remains valid when its backing block moves.
            let source = cpu.gpr[3];
            let destination = cpu.gpr[4];
            let size = cpu.gpr[5];
            let result = if (size as i32) < 0 {
                PPC_MEM_FULL_ERR
            } else if memory.read_u32_be(destination).is_none_or(|ptr| ptr == 0) {
                PPC_NIL_HANDLE_ERR
            } else if let Some(bytes) = ppc_memory_read_bytes(memory, source, size) {
                let result = process_memory_manager.append_process_handle_from_native_import(
                    memory,
                    destination,
                    &bytes,
                );
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                ppc_apply_process_native_handle(process_memory_manager, handles, destination);
                result
            } else {
                PPC_PARAM_ERR
            };
            process_memory_manager.set_native_mem_error(result);
            *last_mem_error = result;
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::HandAndHand => {
            let result = {
                let source_handle = cpu.gpr[3];
                let destination_handle = cpu.gpr[4];
                let source = handles
                    .iter()
                    .find(|record| record.handle == source_handle)
                    .copied()
                    .or_else(|| ppc_system_handle_record(memory, source_handle));
                let result = if let Some(source) = source {
                    if let Some(bytes) = ppc_memory_read_bytes(memory, source.ptr, source.size) {
                        let result = process_memory_manager.append_bytes_to_native_handle(
                            memory,
                            destination_handle,
                            &bytes,
                        );
                        ppc_apply_process_native_allocator(
                            process_memory_manager,
                            memory,
                            heap_cursor,
                            last_mem_error,
                        );
                        ppc_apply_process_native_handle(
                            process_memory_manager,
                            handles,
                            destination_handle,
                        );
                        result
                    } else {
                        PPC_PARAM_ERR
                    }
                } else {
                    PPC_NIL_HANDLE_ERR
                };
                process_memory_manager.set_native_mem_error(result);
                result
            };
            *last_mem_error = result;
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::NewHandle { clear }
        | PpcImportDispatcherTarget::NewHandleSys { clear } => {
            let size = cpu.gpr[3];
            let heap = if matches!(binding.dispatcher_target, PpcImportDispatcherTarget::NewHandleSys { .. }) {
                ProcessHandleHeap::System
            } else {
                ProcessHandleHeap::Current
            };
            let result = process_memory_manager.new_handle(
                ProcessNewHandleRequest::new(size as i32, clear, heap),
                ProcessNewHandleBackend::Native(memory),
            );
            *last_mem_error = result.error;
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if result.succeeded() {
                if let Some(record) = process_memory_manager.native_allocation(result.handle) {
                    handles.push(record);
                }
            }
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] NewHandle size={} clear={} lr=${:08X} -> ${:08X} heap=${:08X}..${:08X} err={}",
                    size,
                    clear,
                    cpu.lr,
                    result.handle,
                    *heap_cursor,
                    heap_limit,
                    *last_mem_error
                );
            }
            Some(PpcImportAction::Return(result.handle))
        }
        PpcImportDispatcherTarget::TempNewHandle => {
            // TempNewHandle is deliberately separate from the ordinary
            // NewHandle request/result service: its second argument is a
            // caller-owned result-code pointer and its allocation has
            // temporary-lifetime semantics. Inside Macintosh: Memory (1992),
            // pp. 2-67--2-68.
            let result_code_ptr = cpu.gpr[4];
            if result_code_ptr != 0 && memory.read_u16_be(result_code_ptr).is_none() {
                process_memory_manager.set_native_mem_error(PPC_PARAM_ERR);
                *last_mem_error = PPC_PARAM_ERR;
                return Some(PpcImportAction::Return(0));
            }
            let handle = process_memory_manager.new_native_handle(memory, cpu.gpr[3], false);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if let Some(record) = process_memory_manager.native_allocation(handle) {
                handles.push(record);
            }
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] TempNewHandle size={} lr=${:08X} -> ${:08X} heap=${:08X}..${:08X} err={}",
                    cpu.gpr[3], cpu.lr, handle, *heap_cursor, heap_limit, *last_mem_error
                );
            }
            if result_code_ptr != 0
                && memory
                    .write_u16_be(result_code_ptr, *last_mem_error as u16)
                    .is_none()
            {
                process_memory_manager.set_native_mem_error(PPC_PARAM_ERR);
                *last_mem_error = PPC_PARAM_ERR;
                return Some(PpcImportAction::Return(0));
            }
            Some(PpcImportAction::Return(handle))
        }
        PpcImportDispatcherTarget::TempHLock | PpcImportDispatcherTarget::TempHUnlock => {
            // Unlike HLock/HUnlock, these void routines report OSErr through
            // a second, caller-owned argument (Apple Memory Manager Reference).
            let handle = cpu.gpr[3];
            let result_code_ptr = cpu.gpr[4];
            let result = if result_code_ptr != 0 && memory.read_u16_be(result_code_ptr).is_none() {
                PPC_PARAM_ERR
            } else if handle == 0 || memory.read_u32_be(handle) == Some(0) {
                PPC_NIL_HANDLE_ERR
            } else if !ppc_is_valid_handle(memory, handles, handle) {
                PPC_MEM_WZ_ERR
            } else {
                if matches!(
                    binding.dispatcher_target,
                    PpcImportDispatcherTarget::TempHLock
                ) {
                    process_memory_manager.lock_process_handle(handle, false);
                } else {
                    process_memory_manager.unlock_process_handle(handle);
                }
                PPC_NO_ERR
            };
            process_memory_manager.set_native_mem_error(result);
            *last_mem_error = result;
            if result_code_ptr != 0 {
                let _ = memory.write_u16_be(result_code_ptr, result as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TempDisposeHandle => {
            // TempDisposeHandle(Handle, OSErr *): dispose like DisposeHandle,
            // then report MemError through the caller-owned result pointer.
            let handle = cpu.gpr[3];
            let result_code_ptr = cpu.gpr[4];
            if ppc_hle_trace_enabled() {
                eprintln!("[PPC-TRACE] TempDisposeHandle ${handle:08X} lr=${:08X}", cpu.lr);
            }
            toolbox_startup.menu_item_commands.forget(handle);
            toolbox_startup.menu_item_refcons.forget(handle);
            ppc_dispose_handle(
                handle,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                handles,
                aliases,
                vfs_resources,
                menu_generations,
                toolbox_startup,
            );
            if result_code_ptr != 0 {
                let _ = memory.write_u16_be(result_code_ptr, *last_mem_error as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DisposeHandle => {
            let handle = cpu.gpr[3];
            if ppc_hle_trace_enabled() {
                eprintln!("[PPC-TRACE] DisposeHandle ${handle:08X} lr=${:08X}", cpu.lr);
            }
            toolbox_startup.menu_item_commands.forget(handle);
            toolbox_startup.menu_item_refcons.forget(handle);
            ppc_dispose_handle(
                handle,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                handles,
                aliases,
                vfs_resources,
                menu_generations,
                toolbox_startup,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::EmptyHandle => {
            let handle = cpu.gpr[3];
            *last_mem_error =
                process_memory_manager.empty_process_handle_from_native_import(memory, handle);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            ppc_apply_process_native_handle(process_memory_manager, handles, handle);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HLock => {
            let handle = cpu.gpr[3];
            if ppc_is_valid_handle(memory, handles, handle) {
                process_memory_manager.lock_process_handle(handle, false);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HLockHi => {
            let handle = cpu.gpr[3];
            if ppc_is_valid_handle(memory, handles, handle) {
                process_memory_manager.lock_process_handle(handle, true);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HGetState => Some(PpcImportAction::Return({
            let handle = cpu.gpr[3];
            let valid = ppc_is_valid_handle(memory, handles, handle);
            let value = if valid {
                let mut value = process_memory_manager
                    .state_for_handle(handle)
                    .unwrap_or(0x40);
                if vfs_resources.iter().any(|record| record.handle == handle) {
                    value |= 0x20;
                }
                value
            } else {
                0
            };
            if ppc_hle_trace_enabled() {
                let ptr = memory.read_u32_be(handle).unwrap_or(0);
                eprintln!(
                    "[PPC-TRACE] HGetState pc=${:08X} lr=${:08X} handle=${:08X} ptr=${:08X} value=${:02X} tracked={}",
                    cpu.pc,
                    cpu.lr,
                    handle,
                    ptr,
                    value,
                    handles.iter().any(|record| record.handle == handle)
                );
            }
            u32::from(value)
        })),
        PpcImportDispatcherTarget::HSetState => {
            let handle = cpu.gpr[3];
            let ok = ppc_is_valid_handle(memory, handles, handle);
            if ok {
                process_memory_manager.restore_process_handle_state(handle, cpu.gpr[4] as u8);
            }
            if ppc_hle_trace_enabled() {
                let ptr = memory.read_u32_be(handle).unwrap_or(0);
                eprintln!(
                    "[PPC-TRACE] HSetState pc=${:08X} lr=${:08X} handle=${:08X} ptr=${:08X} value=${:02X} ok={}",
                    cpu.pc,
                    cpu.lr,
                    handle,
                    ptr,
                    cpu.gpr[4] as u8,
                    ok
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HUnlock => {
            let handle = cpu.gpr[3];
            if ppc_is_valid_handle(memory, handles, handle) {
                process_memory_manager.unlock_process_handle(handle);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MoveHHi => Some(PpcImportAction::ReturnPreserve),
        PpcImportDispatcherTarget::HNoPurge => {
            let handle = cpu.gpr[3];
            if ppc_is_valid_handle(memory, handles, handle) {
                process_memory_manager.set_process_handle_purgeable(handle, false);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HPurge => {
            let handle = cpu.gpr[3];
            if ppc_is_valid_handle(memory, handles, handle) {
                process_memory_manager.set_process_handle_purgeable(handle, true);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetZone => Some(PpcImportAction::Return(
            memory
                .read_u32_be(PPC_THE_ZONE_ADDR)
                .unwrap_or(PPC_APPLICATION_ZONE),
        )),
        PpcImportDispatcherTarget::SystemZone => Some(PpcImportAction::Return(PPC_SYSTEM_ZONE)),
        PpcImportDispatcherTarget::ApplicationZone => {
            Some(PpcImportAction::Return(PPC_APPLICATION_ZONE))
        }
        PpcImportDispatcherTarget::SetZone => {
            let _ = memory.write_u32_be(PPC_THE_ZONE_ADDR, cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InitZone => {
            // Inside Macintosh: Memory (1992), pp. 2-86--2-87. The native
            // PowerPC ABI passes pGrowZone, cMoreMasters, limitPtr, and
            // startPtr in r3-r6. Initialize the caller-visible Zone header
            // even though allocations continue to use Systemless's flat heap.
            ppc_init_zone_header(
                memory,
                cpu.gpr[6],
                cpu.gpr[5],
                cpu.gpr[4] as u16,
                cpu.gpr[3],
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetHandleSize => {
            let handle = cpu.gpr[3];
            let ptr = memory.read_u32_be(handle).unwrap_or(0);
            let size = process_memory_manager
                .process_handle_size_from_master_pointer(handle, ptr)
                .or_else(|| ppc_system_handle_record(memory, handle).map(|record| record.size));
            if size.is_some() {
                process_memory_manager.set_native_mem_error(PPC_NO_ERR);
            }
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if size.is_some() {
                ppc_apply_process_native_handle(process_memory_manager, handles, handle);
            }
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] GetHandleSize handle=${handle:08X} lr=${:08X} -> {} err={}",
                    cpu.lr,
                    size.unwrap_or(0),
                    *last_mem_error
                );
            }
            Some(PpcImportAction::Return(size.unwrap_or(0)))
        }
        PpcImportDispatcherTarget::SetHandleSize => {
            let handle = cpu.gpr[3];
            let size = cpu.gpr[4];
            *last_mem_error = process_memory_manager
                .set_process_handle_size_from_native_import(memory, handle, size);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if let Some(updated) = process_memory_manager.native_allocation(handle) {
                if let Some(record) = handles.iter_mut().find(|record| record.handle == handle) {
                    *record = updated;
                }
            }
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] SetHandleSize handle=${handle:08X} size={size} err={} ptr=${:08X}",
                    *last_mem_error,
                    memory.read_u32_be(handle).unwrap_or(0)
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MaxApplZone => {
            process_memory_manager.maximize_native_heap();
            *last_mem_error = PPC_NO_ERR;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FlushCodeCache => {
            // PowerPC separates its instruction and data caches, so an app
            // that writes code must call this before branching into it.
            // Inside Macintosh: PowerPC System Software (1994), MakeDataExecutable.
            memory.flush_instruction_cache();
            if ppc_hle_trace_enabled() {
                eprintln!("[PPC-TRACE] {}", binding.symbol_name);
            }
            // `FlushCodeCacheRange` reports an OSStatus; the void variants
            // ignore r3, which is volatile either way.
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::MoreMasters => {
            // Carbon MoreMasterPointers(inCount) requests one block containing
            // inCount masters; our handle pool grows on demand, so the count
            // imposes no allocation limit. Both APIs return void.
            // Apple Memory Manager Reference (2007), p. 58.
            process_memory_manager.request_native_master_pointers();
            *last_mem_error = PPC_NO_ERR;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetApplLimit => {
            // Inside Macintosh: Memory (1992), 2-84: the returned pointer is
            // the first byte beyond the expandable application heap.
            Some(PpcImportAction::Return(
                process_memory_manager.application_heap_limit(heap_limit),
            ))
        }
        PpcImportDispatcherTarget::SetApplLimit => {
            // The heap cannot be contracted below its current extent or
            // expanded into the fixed native stack mapping.
            let requested = cpu.gpr[3];
            let current_heap_cursor = process_memory_manager
                .native_heap_state()
                .map_or(*heap_cursor, |heap| heap.heap_cursor);
            if requested >= current_heap_cursor && requested <= native_heap_ceiling {
                process_memory_manager.set_application_heap_limit(requested);
                // Keep the classic low-memory slot as a projection of the
                // process value when the adapters share the process mapping.
                // Inside Macintosh: Memory (1992), pp. 2-83--2-85.
                let _ = memory.write_u32_be(crate::memory::globals::addr::APPL_LIMIT, requested);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HeapFreeBytes => {
            let free = ppc_heap_free_capacity(memory, *heap_cursor, heap_limit).0;
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] {} lr=${:08X} -> {} heap=${:08X}..${:08X}",
                    binding.symbol_name, cpu.lr, free, *heap_cursor, heap_limit
                );
            }
            Some(PpcImportAction::Return(free))
        }
        PpcImportDispatcherTarget::TempMaxMem => {
            // Temporary storage uses the process-native handle allocator.
            // Its query includes master-pointer overhead and leaves state intact.
            // Inside Macintosh: Memory (1992), pp. 2-79--2-80: grow is always zero.
            let grow = cpu.gpr[3];
            if grow != 0 && !ppc_memory_can_write_bytes(memory, grow, 4) {
                process_memory_manager.set_native_mem_error(PPC_PARAM_ERR);
                *last_mem_error = PPC_PARAM_ERR;
                return Some(PpcImportAction::Return(0));
            }
            let maximum = process_memory_manager.native_temporary_max_size(memory);
            if grow != 0 {
                memory.write_u32_be(grow, 0)?;
            }
            Some(PpcImportAction::Return(maximum))
        }
        PpcImportDispatcherTarget::MaxMem => {
            let free_ptr_blocks = process_memory_manager.native_free_ptr_blocks();
            let free =
                ppc_largest_free_ptr_block(memory, *heap_cursor, heap_limit, free_ptr_blocks);
            let grow_ptr = cpu.gpr[3];
            if grow_ptr != 0 {
                let _ = memory.write_u32_be(grow_ptr, 0);
            }
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] MaxMem lr=${:08X} -> {} heap=${:08X}..${:08X}",
                    cpu.lr, free, *heap_cursor, heap_limit
                );
            }
            Some(PpcImportAction::Return(free))
        }
        PpcImportDispatcherTarget::PurgeMem | PpcImportDispatcherTarget::PurgeMemSys => {
            let free_ptr_blocks = process_memory_manager.native_free_ptr_blocks();
            let free =
                ppc_largest_free_ptr_block(memory, *heap_cursor, heap_limit, free_ptr_blocks);
            *last_mem_error = if cpu.gpr[3] <= free {
                PPC_NO_ERR
            } else {
                PPC_MEM_FULL_ERR
            };
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MemError => {
            if ppc_hle_trace_enabled() && *last_mem_error != PPC_NO_ERR {
                eprintln!("[PPC-TRACE] MemError lr=${:08X} -> {}", cpu.lr, *last_mem_error);
            }
            Some(PpcImportAction::Return(ppc_i16_result(*last_mem_error)))
        }
        _ => None,
    }
}

/// Dispose a handle and drop every loader record that still names it.
#[allow(clippy::too_many_arguments)]
fn ppc_dispose_handle(
    handle: u32,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    aliases: &mut Vec<PpcAliasRecord>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    menu_generations: &mut HashMap<u32, u64>,
    toolbox_startup: &mut PpcToolboxStartupState,
) {
    let disposed = process_memory_manager.dispose_process_handle_from_native_import(memory, handle);
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
    if !disposed {
        return;
    }
    menu_generations.remove(&handle);
    handles.retain(|record| record.handle != handle);
    toolbox_startup
        .indexed_screen_ctables
        .retain(|pixmap_handle, ctable_handle| *pixmap_handle != handle && *ctable_handle != handle);
    aliases.retain(|record| record.handle != handle);
    for resource in vfs_resources.iter_mut().filter(|record| record.handle == handle) {
        resource.handle = 0;
    }
}
