//! Typed Apple Event Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcAppleEventDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) apple_events: &'a mut PpcAppleEventState,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_apple_event_import(
    context: PpcAppleEventDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcAppleEventDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        apple_events,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::AEInstallEventHandler => {
            Some(ppc_install_apple_event_handler(cpu, memory, apple_events))
        }
        PpcImportDispatcherTarget::AEProcessAppleEvent => {
            let guest_call_depth = toolbox_startup.execution.calls().depth();
            Some(ppc_process_apple_event(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                apple_events,
                toolbox_startup,
                guest_call_depth,
            ))
        }
        PpcImportDispatcherTarget::AESetInteractionAllowed => {
            // Inside Macintosh: Interapplication Communication (1993),
            // 4-81 through 4-82: the three AEInteractAllowed values are 0..2.
            let level = cpu.gpr[3] as u8;
            let result = if level <= 2 {
                toolbox_startup.ae_interaction_allowed = level;
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::AEGetInteractionAllowed => {
            let output = cpu.gpr[3];
            let result = if output != 0
                && memory
                    .write_u8(output, toolbox_startup.ae_interaction_allowed)
                    .is_some()
            {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::AppleEventCompatibility(operation) => {
            Some(ppc_dispatch_apple_event_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            ))
        }
        _ => None,
    }
}

fn ppc_write_ae_desc(
    memory: &mut PpcSectionMem,
    result: u32,
    descriptor_type: u32,
    data_handle: u32,
) -> bool {
    result != 0
        && ppc_memory_can_write_bytes(memory, result, 8)
        && memory.write_u32_be(result, descriptor_type).is_some()
        && memory.write_u32_be(result + 4, data_handle).is_some()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_create_process_owned_ae_desc(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    _heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    result: u32,
    descriptor_type: u32,
    bytes: &[u8],
) -> i16 {
    if result == 0 || !ppc_memory_can_write_bytes(memory, result, 8) {
        return PPC_PARAM_ERR;
    }
    let snapshot = process_memory_manager.detached_clone();
    // Interapplication Communication (1993), pp. 3-13 and 4-39: an AEDesc's
    // data lives in a handle allocated in the client process's application
    // heap, and AEDisposeDesc deallocates that storage.
    let handle = process_memory_manager.copy_bytes_to_new_native_handle(memory, bytes);
    if handle == 0 {
        let error = process_memory_manager
            .native_heap_state()
            .map_or(PPC_MEM_FULL_ERR, |heap| heap.last_mem_error);
        process_memory_manager.restore_native_snapshot(snapshot);
        process_memory_manager.set_native_mem_error(error);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        return *last_mem_error;
    }
    if !ppc_write_ae_desc(memory, result, descriptor_type, handle) {
        let _ = memory.write_u32_be(handle, 0);
        process_memory_manager.restore_native_snapshot(snapshot);
        process_memory_manager.set_native_mem_error(PPC_PARAM_ERR);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        return PPC_PARAM_ERR;
    }
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
    ppc_apply_process_native_handle(process_memory_manager, handles, handle);
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_dispatch_apple_event_compatibility(
    operation: PpcAppleEventCompatibilityOperation,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> PpcImportAction {
    let result = match operation {
        PpcAppleEventCompatibilityOperation::CreateDesc => {
            let data_size = cpu.gpr[5];
            let Some(bytes) = ppc_memory_read_bytes(memory, cpu.gpr[4], data_size)
                .or_else(|| (data_size == 0).then(Vec::new))
            else {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            };
            if cpu.gpr[6] == 0 || !ppc_memory_can_write_bytes(memory, cpu.gpr[6], 8) {
                PPC_PARAM_ERR
            } else {
                ppc_create_process_owned_ae_desc(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    cpu.gpr[6],
                    cpu.gpr[3],
                    &bytes,
                )
            }
        }
        PpcAppleEventCompatibilityOperation::CreateAppleEvent => {
            let result_ptr = cpu.gpr[8];
            if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, 8) {
                PPC_PARAM_ERR
            } else {
                ppc_create_process_owned_ae_desc(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    result_ptr,
                    u32::from_be_bytes(*b"aevt"),
                    &[],
                )
            }
        }
        PpcAppleEventCompatibilityOperation::DisposeDesc => {
            let desc = cpu.gpr[3];
            if desc == 0 || !ppc_memory_can_write_bytes(memory, desc, 8) {
                PPC_PARAM_ERR
            } else {
                let handle = memory.read_u32_be(desc + 4).unwrap_or(0);
                if handle != 0 {
                    let _ = ppc_dispose_process_native_handle(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        handle,
                    );
                }
                let _ = memory.write_u32_be(desc, 0);
                let _ = memory.write_u32_be(desc + 4, 0);
                PPC_NO_ERR
            }
        }
        PpcAppleEventCompatibilityOperation::CountItems => {
            if memory.write_u32_be(cpu.gpr[4], 0).is_some() {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            }
        }
        PpcAppleEventCompatibilityOperation::GetParamDesc => {
            if cpu.gpr[6] != 0 {
                let _ = ppc_write_ae_desc(memory, cpu.gpr[6], 0, 0);
            }
            PPC_ERR_AE_DESC_NOT_FOUND
        }
        PpcAppleEventCompatibilityOperation::GetAttributePtr => {
            if cpu.gpr[6] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[6], 0);
            }
            if cpu.gpr[9] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[9], 0);
            }
            PPC_ERR_AE_DESC_NOT_FOUND
        }
        PpcAppleEventCompatibilityOperation::GetNthPtr => {
            if cpu.gpr[6] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[6], 0);
            }
            if cpu.gpr[7] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[7], 0);
            }
            if cpu.gpr[10] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[10], 0);
            }
            PPC_ERR_AE_DESC_NOT_FOUND
        }
        PpcAppleEventCompatibilityOperation::Send => {
            if cpu.gpr[4] != 0 {
                let _ = ppc_write_ae_desc(memory, cpu.gpr[4], 0, 0);
            }
            PPC_ERR_AE_EVENT_NOT_HANDLED
        }
        PpcAppleEventCompatibilityOperation::PutParamDesc
        | PpcAppleEventCompatibilityOperation::PutParamPtr => PPC_NO_ERR,
    };
    PpcImportAction::Return(ppc_i16_result(result))
}

pub(super) fn ppc_enqueue_open_application_event_if_needed(
    apple_events: &mut PpcAppleEventState,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    event_mask: u16,
    when: u32,
) {
    // Macintosh Toolbox Essentials (1992), pp. 2-30--2-32 and 5-90:
    // Finder posts kAEOpenApplication once at launch for applications whose
    // SIZE resource sets isHighLevelEventAware. The EventRecord carries the
    // core class in message and the event ID split across the Point fields.
    if event_mask & PPC_HIGH_LEVEL_EVENT_MASK == 0
        || !apple_events
            .apple_event_launch_state
            .claim_open_application_event()
    {
        return;
    }
    event_queue.push_front(PpcQueuedEvent {
        what: PPC_HIGH_LEVEL_EVENT,
        message: PPC_CORE_EVENT_CLASS,
        when,
        where_v: (PPC_OPEN_APPLICATION_EVENT >> 16) as u16 as i16,
        where_h: PPC_OPEN_APPLICATION_EVENT as u16 as i16,
        modifiers: 0,
    });
}

pub(super) fn ppc_install_apple_event_handler(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    apple_events: &mut PpcAppleEventState,
) -> PpcImportAction {
    // Inside Macintosh: Interapplication Communication (1993),
    // pp. 4-62--4-64. PowerPC UPPs can be routine descriptors or TVectors;
    // resolve them when installed so dispatch preserves their TOC value.
    let Some(procedure) = resolve_guest_procedure(
        memory,
        cpu.gpr[5],
        cpu.gpr[2],
        None,
        GuestIsa::PowerPc,
        GuestIsa::PowerPc,
    ) else {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    let mapped = match procedure.isa {
        GuestIsa::PowerPc => memory.read_u32_be(procedure.entry).is_some(),
        GuestIsa::M68k => memory.read_u16_be(procedure.entry).is_some(),
    };
    if cpu.gpr[5] & 1 != 0 || !mapped {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    apple_events.handlers.install(
        cpu.gpr[7] & 0xff != 0,
        cpu.gpr[3],
        cpu.gpr[4],
        ProcessAppleEventHandler {
            procedure,
            refcon: cpu.gpr[6],
        },
    );
    PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_process_apple_event(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    apple_events: &mut PpcAppleEventState,
    toolbox_startup: &mut PpcToolboxStartupState,
    resume_guest_call_depth: usize,
) -> PpcImportAction {
    // AEProcessAppleEvent receives the EventRecord returned by the Event
    // Manager and invokes the matching application handler with an AppleEvent
    // descriptor, reply descriptor, and its registration refcon (IM:IAC,
    // pp. 4-66--4-68).
    let event_record = cpu.gpr[3];
    let event = (event_record != 0).then(|| {
        let what = memory.read_u16_be(event_record)?;
        let event_class = memory.read_u32_be(event_record.checked_add(2)?)?;
        let event_id = u32::from(memory.read_u16_be(event_record.checked_add(10)?)?) << 16
            | u32::from(memory.read_u16_be(event_record.checked_add(12)?)?);
        Some((what, event_class, event_id))
    });
    let Some((PPC_HIGH_LEVEL_EVENT, event_class, event_id)) = event.flatten() else {
        return PpcImportAction::Return(ppc_i16_result(PPC_ERR_AE_EVENT_NOT_HANDLED));
    };
    let Some(handler) = apple_events
        .handlers
        .handler_for(event_class, event_id, PPC_TYPE_WILDCARD)
    else {
        return PpcImportAction::Return(ppc_i16_result(PPC_ERR_AE_EVENT_NOT_HANDLED));
    };

    let snapshot = process_memory_manager.detached_clone();
    // Interapplication Communication (1993), pp. 4-33 and 4-39: the server's
    // event and reply descriptors live in its application heap and the Apple
    // Event Manager disposes both after the installed handler returns.
    let descriptors = process_memory_manager.new_native_ptr(memory, 16, true);
    let mut event_data = Vec::with_capacity(8);
    event_data.extend_from_slice(&event_class.to_be_bytes());
    event_data.extend_from_slice(&event_id.to_be_bytes());
    let event_handle = (descriptors != 0)
        .then(|| process_memory_manager.copy_bytes_to_new_native_handle(memory, &event_data))
        .unwrap_or(0);
    let reply_handle = (event_handle != 0)
        .then(|| process_memory_manager.copy_bytes_to_new_native_handle(memory, &[]))
        .unwrap_or(0);
    if descriptors == 0
        || event_handle == 0
        || reply_handle == 0
        || !ppc_write_ae_desc(memory, descriptors, PPC_CORE_EVENT_CLASS, event_handle)
        || !ppc_write_ae_desc(memory, descriptors + 8, PPC_CORE_EVENT_CLASS, reply_handle)
        || install_powerpc_call_arguments(
            cpu,
            memory,
            &[descriptors, descriptors + 8, handler.refcon],
        )
        .is_none()
    {
        if event_handle != 0 {
            let _ = memory.write_u32_be(event_handle, 0);
        }
        if reply_handle != 0 {
            let _ = memory.write_u32_be(reply_handle, 0);
        }
        process_memory_manager.restore_native_snapshot(snapshot);
        process_memory_manager.set_native_mem_error(PPC_MEM_FULL_ERR);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        return PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR));
    }
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
    ppc_apply_process_native_handle(process_memory_manager, handles, event_handle);
    ppc_apply_process_native_handle(process_memory_manager, handles, reply_handle);
    apple_events
        .pending_dispatches
        .push(PpcAppleEventDispatchAllocation {
            resume_guest_call_depth,
            descriptors,
            event_handle,
            reply_handle,
        });
    match handler.procedure.isa {
        GuestIsa::PowerPc => GuestCallEffect::call_guest(
            GuestCallRequest::new(GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: handler.procedure.entry,
                rtoc: handler.procedure.rtoc,
            }),
            GuestCallContinuation::to_powerpc(
                PPC_GUEST_CALL_RETURN_PC,
                cpu.lr,
                cpu.gpr[2],
                PpcNativeReturnGpr3::Preserve,
            ),
        )
        .into_ppc_import_action()
        .expect("validated AppleEvent handler must be native PowerPC"),
        GuestIsa::M68k => {
            // AEEventHandlerProc is a Pascal function returning a two-byte
            // OSErr with three four-byte arguments. Interapplication
            // Communication (1993), pp. 4-12 and 4-66--4-68; PowerPC System
            // Software (1994), pp. 2-12--2-16.
            let proc_info = if handler.procedure.proc_info == 0 {
                0x0000_0fe0
            } else {
                handler.procedure.proc_info
            };
            let saved_mixed_mode_m68k = toolbox_startup.mixed_mode_m68k.snapshot();
            let action = ppc_begin_m68k_universal_proc(
                cpu,
                Some(process_memory_manager),
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
                handler.procedure,
                proc_info,
                None,
                vec![descriptors, descriptors + 8, handler.refcon],
                cpu.lr,
                PpcNativeReturnGpr3::Preserve,
            );
            if let Some(action) = action {
                action
            } else {
                apple_events.pending_dispatches.pop();
                toolbox_startup
                    .mixed_mode_m68k
                    .restore_snapshot(saved_mixed_mode_m68k);
                for handle in [event_handle, reply_handle] {
                    let _ = memory.write_u32_be(handle, 0);
                }
                process_memory_manager.restore_native_snapshot(snapshot);
                process_memory_manager.set_native_mem_error(PPC_MEM_FULL_ERR);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                handles.retain(|record| {
                    record.handle != event_handle && record.handle != reply_handle
                });
                PpcImportAction::Return(ppc_i16_result(PPC_MEM_FULL_ERR))
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_complete_apple_event_dispatch(
    apple_events: &mut PpcAppleEventState,
    guest_call_depth: usize,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let Some(dispatch) = apple_events
        .pending_dispatches
        .last()
        .filter(|dispatch| dispatch.resume_guest_call_depth == guest_call_depth)
        .copied()
    else {
        return;
    };
    apple_events.pending_dispatches.pop();
    for handle in [dispatch.event_handle, dispatch.reply_handle] {
        let _ = process_memory_manager.dispose_native_handle(memory, handle);
        handles.retain(|record| record.handle != handle);
    }
    let _ = process_memory_manager.dispose_native_ptr(dispatch.descriptors);
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
}
