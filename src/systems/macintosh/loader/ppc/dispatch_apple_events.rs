//! Typed Apple Event Manager dispatch for PowerPC imports.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcAppleEventDispatchAllocation {
    pub(super) resume_guest_call_depth: usize,
    pub(super) descriptors: u32,
    pub(super) event_handle: u32,
    pub(super) reply_handle: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcObjectAccessor {
    pub(super) pointer: u32,
    pub(super) refcon: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcAeResolveLevel {
    pub(super) desired_class: u32,
    pub(super) key_form: u32,
    pub(super) key_data: ProcessAeDescriptor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcAeResolvePending {
    pub(super) resume_guest_call_depth: usize,
    pub(super) levels: Vec<PpcAeResolveLevel>,
    pub(super) index: usize,
    pub(super) container_class: u32,
    pub(super) container_type: u32,
    pub(super) container_handle: u32,
    pub(super) token_ptr: u32,
    pub(super) scratch_ptr: u32,
    pub(super) owned_handles: Vec<u32>,
    pub(super) intermediate_tokens: Vec<(u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcAeResolveCleanupPending {
    pub(super) resolution: PpcAeResolvePending,
    pub(super) result: i16,
    pub(super) current_token: (u32, u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcAeTokenDisposalPending {
    pub(super) resume_guest_call_depth: usize,
    pub(super) token_ptr: u32,
}

fn ppc_ae_field_u32(
    descriptor: &ProcessAeDescriptor,
    keyword: u32,
    expected_type: u32,
) -> Option<u32> {
    let field = descriptor.fields.get(&keyword)?;
    if field.desc_type != expected_type {
        return None;
    }
    let bytes = &field.data;
    Some(u32::from_be_bytes(bytes.get(..4)?.try_into().ok()?))
}

pub(super) fn ppc_ae_collect_resolve_levels(
    specifier: &ProcessAeDescriptor,
) -> Option<(ProcessAeDescriptor, Vec<PpcAeResolveLevel>)> {
    fn collect(
        specifier: &ProcessAeDescriptor,
        levels: &mut Vec<PpcAeResolveLevel>,
        depth: usize,
    ) -> Option<ProcessAeDescriptor> {
        // Object specifiers form a container chain. Bound traversal so a
        // malformed guest descriptor cannot exhaust the host stack.
        if specifier.desc_type != PPC_TYPE_OBJECT_SPECIFIER || depth >= 64 {
            return None;
        }
        let container = specifier
            .fields
            .get(&u32::from_be_bytes(*b"from"))
            .cloned()
            .unwrap_or_else(|| ProcessAeDescriptor {
                desc_type: PPC_TYPE_NULL,
                ..Default::default()
            });
        let base = if container.desc_type == PPC_TYPE_OBJECT_SPECIFIER {
            collect(&container, levels, depth + 1)?
        } else {
            container
        };
        levels.push(PpcAeResolveLevel {
            desired_class: ppc_ae_field_u32(
                specifier,
                u32::from_be_bytes(*b"want"),
                PPC_TYPE_TYPE,
            )?,
            key_form: ppc_ae_field_u32(
                specifier,
                u32::from_be_bytes(*b"form"),
                PPC_TYPE_ENUMERATED,
            )?,
            key_data: specifier.fields.get(&u32::from_be_bytes(*b"seld"))?.clone(),
        });
        Some(base)
    }

    // Inside Macintosh: Interapplication Communication (1993), 6-75--6-77:
    // resolve the innermost container before the outer object specifier.
    let mut levels = Vec::new();
    let base = collect(specifier, &mut levels, 0)?;
    Some((base, levels))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PpcAppleEventState {
    pub(crate) apple_event_launch_state: SharedProcessAppleEventLaunchState,
    pub(super) handlers: SharedProcessAppleEventHandlers,
    pub(super) descriptors: SharedProcessAppleEventDescriptors,
    pub(super) pending_dispatches: Vec<PpcAppleEventDispatchAllocation>,
    pub(super) pending_resolutions: Vec<PpcAeResolvePending>,
    pub(super) pending_resolve_cleanups: Vec<PpcAeResolveCleanupPending>,
    pub(super) pending_token_disposals: Vec<PpcAeTokenDisposalPending>,
    pub(super) object_support_initialized: bool,
    pub(super) object_accessors: HashMap<(bool, u32, u32), PpcObjectAccessor>,
    pub(super) object_callbacks: HashMap<u32, u32>,
}

impl PpcAppleEventState {
    pub(super) fn object_accessor_exact(
        &self,
        desired_class: u32,
        container_type: u32,
    ) -> Option<PpcObjectAccessor> {
        [false, true]
            .into_iter()
            .find_map(|system| {
                self.object_accessors
                    .get(&(system, desired_class, container_type))
            })
            .copied()
    }

    pub(super) fn object_accessor_for(
        &self,
        desired_class: u32,
        container_type: u32,
    ) -> Option<PpcObjectAccessor> {
        // Search the application table before the system table, then prefer
        // exact entries over wildcards (IAC 1993, pp. 6-21--6-24).
        for is_sys_handler in [false, true] {
            for (desired, container) in [
                (desired_class, container_type),
                (desired_class, PPC_TYPE_WILDCARD),
                (PPC_TYPE_WILDCARD, container_type),
                (PPC_TYPE_WILDCARD, PPC_TYPE_WILDCARD),
            ] {
                if let Some(accessor) =
                    self.object_accessors
                        .get(&(is_sys_handler, desired, container))
                {
                    return Some(*accessor);
                }
            }
        }
        None
    }
}

#[allow(clippy::too_many_arguments)]
fn ppc_start_object_accessor(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    toolbox_startup: &mut PpcToolboxStartupState,
    accessor: PpcObjectAccessor,
    arguments: [u32; 9],
) -> PpcImportAction {
    let Some(procedure) = resolve_guest_procedure(
        memory,
        accessor.pointer,
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
    if !mapped {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    match procedure.isa {
        GuestIsa::PowerPc => {
            if install_powerpc_call_arguments(cpu, memory, &arguments).is_none() {
                PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR))
            } else {
                GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: procedure.entry,
                        rtoc: procedure.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        cpu.lr,
                        cpu.gpr[2],
                        PpcNativeReturnGpr3::Preserve,
                    ),
                )
                .into_ppc_import_action()
                .expect("validated accessor must be native PowerPC")
            }
        }
        GuestIsa::M68k if procedure.proc_info != 0 => {
            let saved_mixed_mode_m68k = toolbox_startup.mixed_mode_m68k.snapshot();
            ppc_begin_m68k_universal_proc(
                cpu,
                Some(process_memory_manager),
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
                procedure,
                procedure.proc_info,
                None,
                arguments.to_vec(),
                cpu.lr,
                PpcNativeReturnGpr3::Preserve,
            )
            .unwrap_or_else(|| {
                toolbox_startup
                    .mixed_mode_m68k
                    .restore_snapshot(saved_mixed_mode_m68k);
                PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR))
            })
        }
        GuestIsa::M68k => PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)),
    }
}

#[allow(clippy::too_many_arguments)]
fn ppc_start_token_disposal_callback(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    toolbox_startup: &mut PpcToolboxStartupState,
    pointer: u32,
    token_ptr: u32,
) -> PpcImportAction {
    let Some(procedure) = resolve_guest_procedure(
        memory,
        pointer,
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
    if !mapped {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    match procedure.isa {
        GuestIsa::PowerPc => {
            if install_powerpc_call_arguments(cpu, memory, &[token_ptr]).is_none() {
                PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR))
            } else {
                GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: procedure.entry,
                        rtoc: procedure.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        cpu.lr,
                        cpu.gpr[2],
                        PpcNativeReturnGpr3::Preserve,
                    ),
                )
                .into_ppc_import_action()
                .expect("validated token disposal callback must be native PowerPC")
            }
        }
        GuestIsa::M68k if procedure.proc_info != 0 => {
            let saved_mixed_mode_m68k = toolbox_startup.mixed_mode_m68k.snapshot();
            ppc_begin_m68k_universal_proc(
                cpu,
                Some(process_memory_manager),
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
                procedure,
                procedure.proc_info,
                None,
                vec![token_ptr],
                cpu.lr,
                PpcNativeReturnGpr3::Preserve,
            )
            .unwrap_or_else(|| {
                toolbox_startup
                    .mixed_mode_m68k
                    .restore_snapshot(saved_mixed_mode_m68k);
                PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR))
            })
        }
        GuestIsa::M68k => PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)),
    }
}

#[allow(clippy::too_many_arguments)]
fn ppc_resolve_value_handle(
    descriptor: &ProcessAeDescriptor,
    apple_events: &mut PpcAppleEventState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> Result<u32, i16> {
    if descriptor.desc_type == PPC_TYPE_NULL && descriptor.data.is_empty() {
        return Ok(0);
    }
    let handle = process_memory_manager.copy_bytes_to_new_native_handle(memory, &descriptor.data);
    if handle == 0 {
        let error = process_memory_manager
            .native_heap_state()
            .map_or(PPC_MEM_FULL_ERR, |heap| heap.last_mem_error);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        return Err(error);
    }
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
    ppc_apply_process_native_handle(process_memory_manager, handles, handle);
    apple_events.descriptors.with_mut(|state| {
        state.backing.insert(handle, descriptor.clone());
    });
    Ok(handle)
}

#[allow(clippy::too_many_arguments)]
fn ppc_finish_ae_resolve(
    pending: PpcAeResolvePending,
    error: i16,
    apple_events: &mut PpcAppleEventState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let final_handle = if error == PPC_NO_ERR {
        memory.read_u32_be(pending.token_ptr + 4).unwrap_or(0)
    } else {
        0
    };
    if error != PPC_NO_ERR {
        let _ = ppc_write_ae_desc(memory, pending.token_ptr, PPC_TYPE_NULL, 0);
    }
    let mut disposable = pending.owned_handles;
    disposable.sort_unstable();
    disposable.dedup();
    for handle in disposable {
        if handle == final_handle {
            continue;
        }
        apple_events.descriptors.with_mut(|state| {
            state.backing.remove(&handle);
        });
        let _ = process_memory_manager.dispose_native_handle(memory, handle);
        handles.retain(|record| record.handle != handle);
    }
    if pending.scratch_ptr != 0 {
        apple_events.descriptors.with_mut(|state| {
            state.descriptors.remove(&pending.scratch_ptr);
        });
        let _ = process_memory_manager.dispose_native_ptr(pending.scratch_ptr);
    }
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
}

#[allow(clippy::too_many_arguments)]
fn ppc_dispatch_ae_resolve_level(
    pending: &mut PpcAeResolvePending,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    apple_events: &mut PpcAppleEventState,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Result<PpcImportAction, i16> {
    let level = &pending.levels[pending.index];
    let accessor = apple_events
        .object_accessor_for(level.desired_class, pending.container_type)
        .ok_or(PPC_ERR_AE_ACCESSOR_NOT_FOUND)?;
    let key_handle = ppc_resolve_value_handle(
        &level.key_data,
        apple_events,
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
    )?;
    if key_handle != 0 {
        pending.owned_handles.push(key_handle);
    }
    let output = if pending.index + 1 == pending.levels.len() {
        pending.token_ptr
    } else {
        pending.scratch_ptr
    };
    if !ppc_write_ae_desc(memory, output, PPC_TYPE_NULL, 0) {
        return Err(PPC_PARAM_ERR);
    }
    let arguments = [
        level.desired_class,
        pending.container_type,
        pending.container_handle,
        pending.container_class,
        level.key_form,
        level.key_data.desc_type,
        key_handle,
        output,
        accessor.refcon,
    ];
    let action = ppc_start_object_accessor(
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        toolbox_startup,
        accessor,
        arguments,
    );
    match action {
        PpcImportAction::Return(error) => Err(error as u16 as i16),
        action => Ok(action),
    }
}

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
        PpcImportDispatcherTarget::ObjectSupportInit => {
            // AEObjectInit initializes the object support dispatch tables.
            // FUNCTION AEObjectInit: OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 6-77.
            apple_events.object_support_initialized = true;
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::ObjectSupportInstallAccessor => {
            // AEInstallObjectAccessor registers or replaces an accessor.
            // FUNCTION AEInstallObjectAccessor(desiredClass, containerType:
            //   DescType; theAccessor: AccessorProcPtr; accessorRefcon:
            //   LongInt; isSysHandler: Boolean): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 6-78.
            let pointer = cpu.gpr[5];
            let result =
                if !apple_events.object_support_initialized || pointer == 0 || pointer & 1 != 0 {
                    PPC_PARAM_ERR
                } else {
                    let key = (cpu.gpr[7] != 0, cpu.gpr[3], cpu.gpr[4]);
                    apple_events.object_accessors.insert(
                        key,
                        PpcObjectAccessor {
                            pointer,
                            refcon: cpu.gpr[6],
                        },
                    );
                    PPC_NO_ERR
                };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::ObjectSupportGetAccessor => {
            // AEGetObjectAccessor reads an exact entry from the selected
            // application or system table without removing it.
            // FUNCTION AEGetObjectAccessor(desiredClass, containerType:
            //   DescType; VAR theAccessor: AccessorProcPtr;
            //   VAR accessorRefcon: LongInt; isSysHandler: Boolean): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 6-82--6-83.
            let pointer_out = cpu.gpr[5];
            let refcon_out = cpu.gpr[6];
            let result = if !apple_events.object_support_initialized
                || !ppc_memory_can_write_bytes(memory, pointer_out, 4)
                || !ppc_memory_can_write_bytes(memory, refcon_out, 4)
            {
                PPC_PARAM_ERR
            } else {
                let key = (cpu.gpr[7] != 0, cpu.gpr[3], cpu.gpr[4]);
                if let Some(accessor) = apple_events.object_accessors.get(&key) {
                    let _ = memory.write_u32_be(pointer_out, accessor.pointer);
                    let _ = memory.write_u32_be(refcon_out, accessor.refcon);
                    PPC_NO_ERR
                } else {
                    let _ = memory.write_u32_be(pointer_out, 0);
                    let _ = memory.write_u32_be(refcon_out, 0);
                    PPC_ERR_AE_ACCESSOR_NOT_FOUND
                }
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::ObjectSupportCallAccessor => {
            // AECallObjectAccessor passes two AEDesc values by value. Their
            // type and handle occupy two words each; the registered refcon is
            // appended as the ninth word of the accessor's native PPC call.
            // FUNCTION AECallObjectAccessor(desiredClass: DescType;
            //   containerToken: AEDesc; containerClass, keyForm: DescType;
            //   keyData: AEDesc; VAR theToken: AEDesc): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 6-83;
            // PowerPC System Software (1994), 1-47--1-50.
            let token = cpu.gpr[10];
            let action = if !ppc_write_ae_desc(memory, token, PPC_TYPE_NULL, 0)
                || !apple_events.object_support_initialized
            {
                PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR))
            } else if let Some(accessor) =
                apple_events.object_accessor_exact(cpu.gpr[3], cpu.gpr[4])
            {
                let arguments = [
                    cpu.gpr[3],
                    cpu.gpr[4],
                    cpu.gpr[5],
                    cpu.gpr[6],
                    cpu.gpr[7],
                    cpu.gpr[8],
                    cpu.gpr[9],
                    token,
                    accessor.refcon,
                ];
                ppc_start_object_accessor(
                    cpu,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    toolbox_startup,
                    accessor,
                    arguments,
                )
            } else {
                PpcImportAction::Return(ppc_i16_result(PPC_ERR_AE_ACCESSOR_NOT_FOUND))
            };
            Some(action)
        }
        PpcImportDispatcherTarget::ObjectSupportDisposeToken => {
            // AEDisposeToken first calls the application's token-disposal
            // callback, then falls back to AEDisposeDesc when absent or when
            // the callback returns errAEEventNotHandled.
            // FUNCTION AEDisposeToken(VAR theToken: AEDesc): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 6-86--6-87.
            let token_ptr = cpu.gpr[3];
            if !apple_events.object_support_initialized
                || ppc_ae_descriptor(memory, &apple_events.descriptors, token_ptr).is_none()
            {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            if let Some(pointer) = apple_events
                .object_callbacks
                .get(&u32::from_be_bytes(*b"xtok"))
            {
                let depth = toolbox_startup.execution.calls().depth();
                let action = ppc_start_token_disposal_callback(
                    cpu,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    toolbox_startup,
                    *pointer,
                    token_ptr,
                );
                if !matches!(action, PpcImportAction::Return(_)) {
                    apple_events
                        .pending_token_disposals
                        .push(PpcAeTokenDisposalPending {
                            resume_guest_call_depth: depth,
                            token_ptr,
                        });
                }
                Some(action)
            } else {
                Some(ppc_dispatch_apple_event_compatibility(
                    PpcAppleEventCompatibilityOperation::DisposeDesc,
                    cpu,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    apple_events,
                ))
            }
        }
        PpcImportDispatcherTarget::ObjectSupportRemoveAccessor => {
            // AERemoveObjectAccessor removes a matching entry; a non-NIL
            // procedure pointer must match the registered accessor.
            // FUNCTION AERemoveObjectAccessor(desiredClass, containerType:
            //   DescType; theAccessor: AccessorProcPtr; isSysHandler: Boolean): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 6-84.
            let key = (cpu.gpr[6] != 0, cpu.gpr[3], cpu.gpr[4]);
            let result = if !apple_events.object_support_initialized {
                PPC_PARAM_ERR
            } else if apple_events
                .object_accessors
                .get(&key)
                .is_none_or(|accessor| cpu.gpr[5] != 0 && accessor.pointer != cpu.gpr[5])
            {
                PPC_ERR_AE_ACCESSOR_NOT_FOUND
            } else {
                apple_events.object_accessors.remove(&key);
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::ObjectSupportSetCallbacks => {
            // AESetObjectCallbacks installs the seven application callbacks.
            // FUNCTION AESetObjectCallbacks(myCompareProc, myCountProc,
            //   myDisposeTokenProc, myGetMarkTokenProc, myMarkProc,
            //   myAdjustMarksProc, myGetErrDescProc: ProcPtr): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 6-79--6-80.
            let callbacks = [
                (u32::from_be_bytes(*b"cmpr"), cpu.gpr[3]),
                (u32::from_be_bytes(*b"cont"), cpu.gpr[4]),
                (u32::from_be_bytes(*b"xtok"), cpu.gpr[5]),
                (u32::from_be_bytes(*b"mkid"), cpu.gpr[6]),
                (u32::from_be_bytes(*b"mark"), cpu.gpr[7]),
                (u32::from_be_bytes(*b"adjm"), cpu.gpr[8]),
                (u32::from_be_bytes(*b"indc"), cpu.gpr[9]),
            ];
            let result = if !apple_events.object_support_initialized
                || callbacks.iter().any(|(_, pointer)| pointer & 1 != 0)
            {
                PPC_PARAM_ERR
            } else {
                for (function_class, pointer) in callbacks {
                    if pointer != 0 {
                        apple_events
                            .object_callbacks
                            .insert(function_class, pointer);
                    }
                }
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::ObjectSupportResolve => {
            // AEResolve first validates the object specifier and clears the
            // token on error. Inside Macintosh: Interapplication Communication
            // (1993), 6-85.
            let token = cpu.gpr[5];
            let result = if !ppc_write_ae_desc(memory, token, PPC_TYPE_NULL, 0) {
                PPC_PARAM_ERR
            } else if !apple_events.object_support_initialized {
                PPC_PARAM_ERR
            } else {
                match ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                    .and_then(|descriptor| ppc_ae_collect_resolve_levels(&descriptor))
                {
                    None => PPC_ERR_AE_NOT_AN_OBJECT_SPEC,
                    Some((base, levels))
                        if apple_events
                            .object_accessor_for(levels[0].desired_class, base.desc_type)
                            .is_none() =>
                    {
                        PPC_ERR_AE_ACCESSOR_NOT_FOUND
                    }
                    Some((base, levels)) => {
                        let scratch_ptr = if levels.len() > 1 {
                            process_memory_manager.new_native_ptr(memory, 8, true)
                        } else {
                            0
                        };
                        if levels.len() > 1 && scratch_ptr == 0 {
                            PPC_MEM_FULL_ERR
                        } else {
                            ppc_apply_process_native_allocator(
                                process_memory_manager,
                                memory,
                                heap_cursor,
                                last_mem_error,
                            );
                            let base_handle = ppc_resolve_value_handle(
                                &base,
                                apple_events,
                                process_memory_manager,
                                memory,
                                heap_cursor,
                                last_mem_error,
                                handles,
                            );
                            let mut pending = PpcAeResolvePending {
                                resume_guest_call_depth: toolbox_startup.execution.calls().depth(),
                                levels,
                                index: 0,
                                container_class: base.desc_type,
                                container_type: base.desc_type,
                                container_handle: 0,
                                token_ptr: token,
                                scratch_ptr,
                                owned_handles: Vec::new(),
                                intermediate_tokens: Vec::new(),
                            };
                            let action = match base_handle {
                                Ok(handle) => {
                                    pending.container_handle = handle;
                                    if handle != 0 {
                                        pending.owned_handles.push(handle);
                                    }
                                    ppc_dispatch_ae_resolve_level(
                                        &mut pending,
                                        cpu,
                                        process_memory_manager,
                                        memory,
                                        heap_cursor,
                                        heap_limit,
                                        last_mem_error,
                                        handles,
                                        apple_events,
                                        toolbox_startup,
                                    )
                                }
                                Err(error) => Err(error),
                            };
                            match action {
                                Ok(action) => {
                                    apple_events.pending_resolutions.push(pending);
                                    return Some(action);
                                }
                                Err(error) => {
                                    ppc_finish_ae_resolve(
                                        pending,
                                        error,
                                        apple_events,
                                        process_memory_manager,
                                        memory,
                                        heap_cursor,
                                        last_mem_error,
                                        handles,
                                    );
                                    error
                                }
                            }
                        }
                    }
                }
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
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
        PpcImportDispatcherTarget::AEInteractWithUser => {
            // The runner has one foreground application; the request can
            // return immediately without a Notification Manager handoff.
            // Inside Macintosh: Interapplication Communication (1993), 4-83.
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::AEManagerInfo => {
            // AEManagerInfo (Pack8 selector $0441)
            // FUNCTION AEManagerInfo(keyword: AEKeyword; VAR result: LongInt): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 4-104.
            let value = match cpu.gpr[3] {
                keyword if keyword == u32::from_be_bytes(*b"vers") => 0x0101_0000,
                _ => 0,
            };
            let result = if cpu.gpr[4] != 0
                && memory.write_u32_be(cpu.gpr[4], value).is_some()
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
                apple_events,
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

pub(super) fn ppc_ae_descriptor(
    memory: &mut PpcSectionMem,
    state: &SharedProcessAppleEventDescriptors,
    address: u32,
) -> Option<ProcessAeDescriptor> {
    let desc_type = memory.read_u32_be(address)?;
    if desc_type == 0 {
        return None;
    }
    let handle = memory.read_u32_be(address + 4).unwrap_or(0);
    if let Some(descriptor) = state.backing.get(&handle) {
        let mut descriptor = descriptor.clone();
        descriptor.desc_type = desc_type;
        return Some(descriptor);
    }
    state
        .descriptors
        .get(&address)
        .cloned()
        .or_else(|| {
            state
                .events
                .contains_key(&address)
                .then_some(ProcessAeDescriptor {
                    desc_type,
                    data: Vec::new(),
                    fields: HashMap::new(),
                    items: Vec::new(),
                })
        })
        .or_else(|| {
            (handle == 0).then_some(ProcessAeDescriptor {
                desc_type,
                ..Default::default()
            })
        })
}

fn ppc_store_ae_descriptor_semantics(
    memory: &mut PpcSectionMem,
    state: &SharedProcessAppleEventDescriptors,
    address: u32,
    descriptor: ProcessAeDescriptor,
) {
    let handle = memory.read_u32_be(address + 4).unwrap_or(0);
    state.with_mut(|state| {
        if handle != 0 {
            state.backing.insert(handle, descriptor.clone());
        }
        state.descriptors.insert(address, descriptor);
    });
}

fn ppc_put_ae_list_item(
    items: &mut Vec<(u32, ProcessAeDescriptor)>,
    index: u32,
    value: ProcessAeDescriptor,
) -> i16 {
    let item = (PPC_TYPE_WILDCARD, value);
    if index == 0 || index as usize == items.len() + 1 {
        items.push(item);
    } else if index as usize <= items.len() {
        items[index as usize - 1] = item;
    } else {
        return PPC_ERR_AE_ILLEGAL_INDEX;
    }
    PPC_NO_ERR
}

fn ppc_sync_event_descriptor_backing(
    memory: &mut PpcSectionMem,
    state: &SharedProcessAppleEventDescriptors,
    address: u32,
) {
    let handle = memory.read_u32_be(address + 4).unwrap_or(0);
    state.with_mut(|state| {
        let Some(event) = state.events.get(&address).cloned() else {
            return;
        };
        let mut descriptor =
            state
                .descriptors
                .get(&address)
                .cloned()
                .unwrap_or(ProcessAeDescriptor {
                    desc_type: PPC_CORE_EVENT_CLASS,
                    data: Vec::new(),
                    fields: HashMap::new(),
                    items: Vec::new(),
                });
        descriptor.fields = event.params;
        descriptor.items = event.items;
        state.descriptors.insert(address, descriptor.clone());
        if handle != 0 {
            state.backing.insert(handle, descriptor);
        }
    });
}

fn ppc_copy_ae_bytes(
    memory: &mut PpcSectionMem,
    data: &[u8],
    data_ptr: u32,
    maximum_size: u32,
    actual_size_ptr: u32,
) -> i16 {
    if actual_size_ptr != 0
        && memory
            .write_u32_be(actual_size_ptr, data.len() as u32)
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    if data_ptr != 0 {
        let copy_len = data.len().min(maximum_size as usize);
        if memory.write_bytes(data_ptr, &data[..copy_len]).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    if maximum_size < data.len() as u32 {
        PPC_AE_BUFFER_IS_SMALL
    } else {
        PPC_NO_ERR
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_create_process_owned_ae_desc(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    _heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    descriptor_state: Option<&SharedProcessAppleEventDescriptors>,
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
    if let Some(state) = descriptor_state {
        state.with_mut(|state| {
            let descriptor = ProcessAeDescriptor {
                desc_type: descriptor_type,
                data: bytes.to_vec(),
                fields: HashMap::new(),
                items: Vec::new(),
            };
            state.descriptors.insert(result, descriptor.clone());
            state.backing.insert(handle, descriptor);
        });
    }
    PPC_NO_ERR
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcAppleEventCompatibilityOperation {
    CountItems,
    CreateAppleEvent,
    CreateDesc,
    CreateList,
    DisposeDesc,
    GetAttributePtr,
    GetNthPtr,
    GetParamDesc,
    GetParamPtr,
    SizeOfParam,
    PutParamDesc,
    PutParamPtr,
    PutDesc,
    Send,
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
    apple_events: &mut PpcAppleEventState,
) -> PpcImportAction {
    let result = match operation {
        PpcAppleEventCompatibilityOperation::CreateList => {
            // AECreateList (Pack8 selector $0706)
            // FUNCTION AECreateList(factoringPtr: Ptr; factoredSize: Size;
            //   isRecord: Boolean; VAR resultList: AEDescList): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 5-26.
            let factoring_ptr = cpu.gpr[3];
            let factored_size = cpu.gpr[4];
            let result_ptr = cpu.gpr[6];
            let _ = ppc_write_ae_desc(memory, result_ptr, 0, 0);
            if result_ptr == 0
                || !ppc_memory_can_write_bytes(memory, result_ptr, 8)
                || (factored_size != 0 && factored_size != 4 && factored_size < 8)
            {
                PPC_PARAM_ERR
            } else if let Some(bytes) = ppc_memory_read_bytes(memory, factoring_ptr, factored_size)
                .or_else(|| (factored_size == 0).then(Vec::new))
            {
                let desc_type = if cpu.gpr[5] & 0xff != 0 {
                    u32::from_be_bytes(*b"reco")
                } else {
                    u32::from_be_bytes(*b"list")
                };
                ppc_create_process_owned_ae_desc(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    Some(&apple_events.descriptors),
                    result_ptr,
                    desc_type,
                    &bytes,
                )
            } else {
                PPC_PARAM_ERR
            }
        }
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
                    Some(&apple_events.descriptors),
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
                let mut event_data = Vec::with_capacity(8);
                event_data.extend_from_slice(&cpu.gpr[3].to_be_bytes());
                event_data.extend_from_slice(&cpu.gpr[4].to_be_bytes());
                let result = ppc_create_process_owned_ae_desc(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    Some(&apple_events.descriptors),
                    result_ptr,
                    u32::from_be_bytes(*b"aevt"),
                    &event_data,
                );
                if result == PPC_NO_ERR {
                    apple_events.descriptors.with_mut(|state| {
                        state.events.insert(
                            result_ptr,
                            ProcessSyntheticAppleEvent {
                                event_class: cpu.gpr[3],
                                event_id: cpu.gpr[4],
                                params: HashMap::new(),
                                items: Vec::new(),
                            },
                        );
                    });
                }
                result
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
                apple_events.descriptors.with_mut(|state| {
                    state.events.remove(&desc);
                    state.descriptors.remove(&desc);
                    // Keep handle-keyed backing available to AEDesc records
                    // copied by value. The guest handle is single-owner and
                    // has already been disposed above.
                });
                let _ = memory.write_u32_be(desc, 0);
                let _ = memory.write_u32_be(desc + 4, 0);
                PPC_NO_ERR
            }
        }
        PpcAppleEventCompatibilityOperation::CountItems => {
            let count = apple_events
                .descriptors
                .events
                .get(&cpu.gpr[3])
                .map(|event| event.items.len())
                .or_else(|| {
                    ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                        .map(|descriptor| descriptor.items.len())
                });
            match count {
                Some(count) if memory.write_u32_be(cpu.gpr[4], count as u32).is_some() => {
                    PPC_NO_ERR
                }
                Some(_) => PPC_PARAM_ERR,
                None => {
                    let _ = memory.write_u32_be(cpu.gpr[4], 0);
                    PPC_ERR_AE_DESC_NOT_FOUND
                }
            }
        }
        PpcAppleEventCompatibilityOperation::GetParamDesc => {
            let value = apple_events
                .descriptors
                .events
                .get(&cpu.gpr[3])
                .and_then(|event| event.params.get(&cpu.gpr[4]).cloned())
                .or_else(|| {
                    ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                        .and_then(|descriptor| descriptor.fields.get(&cpu.gpr[4]).cloned())
                });
            if let Some(value) = value {
                if cpu.gpr[5] != PPC_TYPE_WILDCARD && cpu.gpr[5] != value.desc_type {
                    let _ = ppc_write_ae_desc(memory, cpu.gpr[6], 0, 0);
                    PPC_ERR_AE_COERCION_FAIL
                } else {
                    let result = ppc_create_process_owned_ae_desc(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        Some(&apple_events.descriptors),
                        cpu.gpr[6],
                        value.desc_type,
                        &value.data,
                    );
                    if result == PPC_NO_ERR {
                        ppc_store_ae_descriptor_semantics(
                            memory,
                            &apple_events.descriptors,
                            cpu.gpr[6],
                            value,
                        );
                    }
                    result
                }
            } else {
                if cpu.gpr[6] != 0 {
                    let _ = ppc_write_ae_desc(memory, cpu.gpr[6], 0, 0);
                }
                PPC_ERR_AE_DESC_NOT_FOUND
            }
        }
        PpcAppleEventCompatibilityOperation::GetParamPtr => {
            let value = apple_events
                .descriptors
                .events
                .get(&cpu.gpr[3])
                .and_then(|event| event.params.get(&cpu.gpr[4]).cloned())
                .or_else(|| {
                    ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                        .and_then(|descriptor| descriptor.fields.get(&cpu.gpr[4]).cloned())
                });
            if let Some(value) = value {
                if cpu.gpr[5] != PPC_TYPE_WILDCARD && cpu.gpr[5] != value.desc_type {
                    PPC_ERR_AE_COERCION_FAIL
                } else {
                    if cpu.gpr[6] != 0 {
                        let _ = memory.write_u32_be(cpu.gpr[6], value.desc_type);
                    }
                    ppc_copy_ae_bytes(memory, &value.data, cpu.gpr[7], cpu.gpr[8], cpu.gpr[9])
                }
            } else {
                if cpu.gpr[6] != 0 {
                    let _ = memory.write_u32_be(cpu.gpr[6], 0);
                }
                if cpu.gpr[9] != 0 {
                    let _ = memory.write_u32_be(cpu.gpr[9], 0);
                }
                PPC_ERR_AE_DESC_NOT_FOUND
            }
        }
        PpcAppleEventCompatibilityOperation::SizeOfParam => {
            // Inside Macintosh: Interapplication Communication (1993),
            // 4-89: report the stored parameter's descriptor type and
            // data length without coercing or copying its data.
            let value = apple_events
                .descriptors
                .events
                .get(&cpu.gpr[3])
                .and_then(|event| event.params.get(&cpu.gpr[4]).cloned())
                .or_else(|| {
                    ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                        .and_then(|descriptor| descriptor.fields.get(&cpu.gpr[4]).cloned())
                });
            if let Some(value) = value {
                let size = u32::try_from(value.data.len()).unwrap_or(u32::MAX);
                if cpu.gpr[5] != 0 {
                    let _ = memory.write_u32_be(cpu.gpr[5], value.desc_type);
                }
                if cpu.gpr[6] != 0 {
                    let _ = memory.write_u32_be(cpu.gpr[6], size);
                }
                PPC_NO_ERR
            } else {
                PPC_ERR_AE_DESC_NOT_FOUND
            }
        }
        PpcAppleEventCompatibilityOperation::GetAttributePtr => {
            let event_identity = apple_events
                .descriptors
                .events
                .get(&cpu.gpr[3])
                .map(|event| (event.event_class, event.event_id))
                .or_else(|| {
                    let descriptor =
                        ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])?;
                    (descriptor.data.len() >= 8).then(|| {
                        (
                            u32::from_be_bytes(descriptor.data[0..4].try_into().unwrap()),
                            u32::from_be_bytes(descriptor.data[4..8].try_into().unwrap()),
                        )
                    })
                });
            let value = event_identity.and_then(|event_identity| {
                let value = match cpu.gpr[4] {
                    PPC_KEY_EVENT_CLASS_ATTR => event_identity.0,
                    PPC_KEY_EVENT_ID_ATTR => event_identity.1,
                    _ => return None,
                };
                Some(ProcessAeDescriptor {
                    desc_type: PPC_TYPE_TYPE,
                    data: value.to_be_bytes().to_vec(),
                    fields: HashMap::new(),
                    items: Vec::new(),
                })
            });
            if let Some(value) = value {
                if cpu.gpr[5] != PPC_TYPE_WILDCARD && cpu.gpr[5] != value.desc_type {
                    PPC_ERR_AE_COERCION_FAIL
                } else {
                    if cpu.gpr[6] != 0 {
                        let _ = memory.write_u32_be(cpu.gpr[6], value.desc_type);
                    }
                    ppc_copy_ae_bytes(memory, &value.data, cpu.gpr[7], cpu.gpr[8], cpu.gpr[9])
                }
            } else {
                if cpu.gpr[6] != 0 {
                    let _ = memory.write_u32_be(cpu.gpr[6], 0);
                }
                if cpu.gpr[9] != 0 {
                    let _ = memory.write_u32_be(cpu.gpr[9], 0);
                }
                PPC_ERR_AE_DESC_NOT_FOUND
            }
        }
        PpcAppleEventCompatibilityOperation::GetNthPtr => {
            let index = cpu.gpr[4] as usize;
            let value = (index != 0)
                .then(|| {
                    apple_events
                        .descriptors
                        .events
                        .get(&cpu.gpr[3])
                        .and_then(|event| event.items.get(index - 1).cloned())
                        .or_else(|| {
                            ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                                .and_then(|descriptor| descriptor.items.get(index - 1).cloned())
                        })
                })
                .flatten();
            if let Some((keyword, value)) = value {
                if cpu.gpr[5] != PPC_TYPE_WILDCARD && cpu.gpr[5] != value.desc_type {
                    PPC_ERR_AE_COERCION_FAIL
                } else {
                    if cpu.gpr[6] != 0 {
                        let _ = memory.write_u32_be(cpu.gpr[6], keyword);
                    }
                    if cpu.gpr[7] != 0 {
                        let _ = memory.write_u32_be(cpu.gpr[7], value.desc_type);
                    }
                    ppc_copy_ae_bytes(memory, &value.data, cpu.gpr[8], cpu.gpr[9], cpu.gpr[10])
                }
            } else {
                for pointer in [cpu.gpr[6], cpu.gpr[7], cpu.gpr[10]] {
                    if pointer != 0 {
                        let _ = memory.write_u32_be(pointer, 0);
                    }
                }
                PPC_ERR_AE_DESC_NOT_FOUND
            }
        }
        PpcAppleEventCompatibilityOperation::Send => {
            if cpu.gpr[4] != 0 {
                let _ = ppc_write_ae_desc(memory, cpu.gpr[4], 0, 0);
            }
            PPC_ERR_AE_EVENT_NOT_HANDLED
        }
        PpcAppleEventCompatibilityOperation::PutDesc => {
            // AEPutDesc (Pack8 selector $0609)
            // FUNCTION AEPutDesc(theAEDescList: AEDescList; index: LongInt;
            //   theAEDesc: AEDesc): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), 5-30.
            if let Some(value) = ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[5]) {
                if apple_events.descriptors.events.contains_key(&cpu.gpr[3]) {
                    let result = apple_events.descriptors.with_mut(|state| {
                        let event = state.events.get_mut(&cpu.gpr[3]).unwrap();
                        ppc_put_ae_list_item(&mut event.items, cpu.gpr[4], value)
                    });
                    if result == PPC_NO_ERR {
                        ppc_sync_event_descriptor_backing(
                            memory,
                            &apple_events.descriptors,
                            cpu.gpr[3],
                        );
                    }
                    result
                } else if let Some(mut list) =
                    ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                {
                    if list.desc_type != u32::from_be_bytes(*b"list")
                        && list.desc_type != u32::from_be_bytes(*b"reco")
                    {
                        PPC_ERR_AE_WRONG_DATA_TYPE
                    } else {
                        let result = ppc_put_ae_list_item(&mut list.items, cpu.gpr[4], value);
                        if result == PPC_NO_ERR {
                            ppc_store_ae_descriptor_semantics(
                                memory,
                                &apple_events.descriptors,
                                cpu.gpr[3],
                                list,
                            );
                        }
                        result
                    }
                } else {
                    PPC_ERR_AE_DESC_NOT_FOUND
                }
            } else {
                PPC_ERR_AE_DESC_NOT_FOUND
            }
        }
        PpcAppleEventCompatibilityOperation::PutParamDesc
        | PpcAppleEventCompatibilityOperation::PutParamPtr => {
            let value = if operation == PpcAppleEventCompatibilityOperation::PutParamDesc {
                ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[5])
            } else {
                ppc_memory_read_bytes(memory, cpu.gpr[6], cpu.gpr[7])
                    .or_else(|| (cpu.gpr[7] == 0).then(Vec::new))
                    .map(|data| ProcessAeDescriptor {
                        desc_type: cpu.gpr[5],
                        data,
                        fields: HashMap::new(),
                        items: Vec::new(),
                    })
            };
            if let Some(value) = value {
                if apple_events.descriptors.events.contains_key(&cpu.gpr[3]) {
                    apple_events.descriptors.with_mut(|state| {
                        let event = state.events.get_mut(&cpu.gpr[3]).unwrap();
                        event.params.insert(cpu.gpr[4], value.clone());
                        if let Some((_, item)) = event
                            .items
                            .iter_mut()
                            .find(|(keyword, _)| *keyword == cpu.gpr[4])
                        {
                            *item = value;
                        } else {
                            event.items.push((cpu.gpr[4], value));
                        }
                    });
                    ppc_sync_event_descriptor_backing(
                        memory,
                        &apple_events.descriptors,
                        cpu.gpr[3],
                    );
                    PPC_NO_ERR
                } else if let Some(mut target) =
                    ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[3])
                {
                    target.fields.insert(cpu.gpr[4], value.clone());
                    if let Some((_, item)) = target
                        .items
                        .iter_mut()
                        .find(|(keyword, _)| *keyword == cpu.gpr[4])
                    {
                        *item = value;
                    } else {
                        target.items.push((cpu.gpr[4], value));
                    }
                    ppc_store_ae_descriptor_semantics(
                        memory,
                        &apple_events.descriptors,
                        cpu.gpr[3],
                        target,
                    );
                    PPC_NO_ERR
                } else {
                    PPC_ERR_AE_DESC_NOT_FOUND
                }
            } else {
                PPC_PARAM_ERR
            }
        }
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
    apple_events.descriptors.with_mut(|state| {
        let event_descriptor = ProcessAeDescriptor {
            desc_type: PPC_CORE_EVENT_CLASS,
            data: event_data,
            fields: HashMap::new(),
            items: Vec::new(),
        };
        let reply_descriptor = ProcessAeDescriptor {
            desc_type: PPC_CORE_EVENT_CLASS,
            data: Vec::new(),
            fields: HashMap::new(),
            items: Vec::new(),
        };
        state
            .descriptors
            .insert(descriptors, event_descriptor.clone());
        state
            .descriptors
            .insert(descriptors + 8, reply_descriptor.clone());
        state.backing.insert(event_handle, event_descriptor);
        state.backing.insert(reply_handle, reply_descriptor);
        state.events.insert(
            descriptors,
            ProcessSyntheticAppleEvent {
                event_class,
                event_id,
                params: HashMap::new(),
                items: Vec::new(),
            },
        );
        state.events.insert(
            descriptors + 8,
            ProcessSyntheticAppleEvent {
                event_class: PPC_CORE_EVENT_CLASS,
                event_id: 0,
                params: HashMap::new(),
                items: Vec::new(),
            },
        );
    });
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
                apple_events.descriptors.with_mut(|state| {
                    for descriptor in [descriptors, descriptors + 8] {
                        state.events.remove(&descriptor);
                        state.descriptors.remove(&descriptor);
                    }
                    state.backing.remove(&event_handle);
                    state.backing.remove(&reply_handle);
                });
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
    apple_events.descriptors.with_mut(|state| {
        for descriptor in [dispatch.descriptors, dispatch.descriptors + 8] {
            state.events.remove(&descriptor);
            state.descriptors.remove(&descriptor);
        }
        state.backing.remove(&dispatch.event_handle);
        state.backing.remove(&dispatch.reply_handle);
    });
    for handle in [dispatch.event_handle, dispatch.reply_handle] {
        let _ = process_memory_manager.dispose_native_handle(memory, handle);
        handles.retain(|record| record.handle != handle);
    }
    let _ = process_memory_manager.dispose_native_ptr(dispatch.descriptors);
    ppc_apply_process_native_allocator(process_memory_manager, memory, heap_cursor, last_mem_error);
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_complete_token_disposal(
    cpu: &mut PpcCpu,
    guest_call_depth: usize,
    apple_events: &mut PpcAppleEventState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> bool {
    if apple_events
        .pending_token_disposals
        .last()
        .is_none_or(|pending| pending.resume_guest_call_depth != guest_call_depth)
    {
        return false;
    }
    let pending = apple_events.pending_token_disposals.pop().unwrap();
    if cpu.gpr[3] as u16 as i16 == PPC_ERR_AE_EVENT_NOT_HANDLED {
        cpu.gpr[3] = pending.token_ptr;
        if let PpcImportAction::Return(result) = ppc_dispatch_apple_event_compatibility(
            PpcAppleEventCompatibilityOperation::DisposeDesc,
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            apple_events,
        ) {
            cpu.gpr[3] = result;
        }
    }
    true
}

fn ppc_dispose_intermediate_token_fallback(
    token: (u32, u32),
    apple_events: &mut PpcAppleEventState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let handle = token.1;
    if handle == 0 {
        return;
    }
    apple_events.descriptors.with_mut(|state| {
        state.backing.remove(&handle);
    });
    let _ = process_memory_manager.dispose_native_handle(memory, handle);
    handles.retain(|record| record.handle != handle);
}

#[allow(clippy::too_many_arguments)]
fn ppc_begin_ae_resolve_cleanup(
    mut pending: PpcAeResolvePending,
    mut result: i16,
    cpu: &mut PpcCpu,
    apple_events: &mut PpcAppleEventState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> PpcImportAction {
    while let Some(token) = pending.intermediate_tokens.pop() {
        if !ppc_write_ae_desc(memory, pending.scratch_ptr, token.0, token.1) {
            result = PPC_PARAM_ERR;
            ppc_dispose_intermediate_token_fallback(
                token,
                apple_events,
                process_memory_manager,
                memory,
                handles,
            );
            continue;
        }
        if let Some(pointer) = apple_events.object_callbacks.get(&u32::from_be_bytes(*b"xtok")) {
            let action = ppc_start_token_disposal_callback(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
                *pointer,
                pending.scratch_ptr,
            );
            match action {
                PpcImportAction::Return(error) => {
                    let error = error as u16 as i16;
                    if error != PPC_ERR_AE_EVENT_NOT_HANDLED && error != PPC_NO_ERR {
                        result = error;
                    }
                    ppc_dispose_intermediate_token_fallback(
                        token,
                        apple_events,
                        process_memory_manager,
                        memory,
                        handles,
                    );
                }
                action => {
                    apple_events
                        .pending_resolve_cleanups
                        .push(PpcAeResolveCleanupPending {
                            resolution: pending,
                            result,
                            current_token: token,
                        });
                    return action;
                }
            }
        } else {
            ppc_dispose_intermediate_token_fallback(
                token,
                apple_events,
                process_memory_manager,
                memory,
                handles,
            );
        }
    }
    ppc_finish_ae_resolve(
        pending,
        result,
        apple_events,
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
    );
    cpu.gpr[3] = ppc_i16_result(result);
    PpcImportAction::Continue
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_continue_ae_resolve_cleanup(
    cpu: &mut PpcCpu,
    guest_call_depth: usize,
    apple_events: &mut PpcAppleEventState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Option<PpcImportAction> {
    if apple_events
        .pending_resolve_cleanups
        .last()
        .is_none_or(|cleanup| cleanup.resolution.resume_guest_call_depth != guest_call_depth)
    {
        return None;
    }
    let cleanup = apple_events.pending_resolve_cleanups.pop().unwrap();
    let callback_error = cpu.gpr[3] as u16 as i16;
    let mut result = cleanup.result;
    if callback_error != PPC_NO_ERR && callback_error != PPC_ERR_AE_EVENT_NOT_HANDLED {
        result = callback_error;
    }
    if callback_error != PPC_NO_ERR
        || (cleanup.current_token.1 != 0
            && process_memory_manager
                .native_allocation(cleanup.current_token.1)
                .is_some())
    {
        ppc_dispose_intermediate_token_fallback(
            cleanup.current_token,
            apple_events,
            process_memory_manager,
            memory,
            handles,
        );
    } else if cleanup.current_token.1 != 0 {
        apple_events.descriptors.with_mut(|state| {
            state.backing.remove(&cleanup.current_token.1);
        });
    }
    Some(ppc_begin_ae_resolve_cleanup(
        cleanup.resolution,
        result,
        cpu,
        apple_events,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        toolbox_startup,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_continue_ae_resolve(
    cpu: &mut PpcCpu,
    guest_call_depth: usize,
    apple_events: &mut PpcAppleEventState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Option<PpcImportAction> {
    if apple_events
        .pending_resolutions
        .last()
        .is_none_or(|pending| pending.resume_guest_call_depth != guest_call_depth)
    {
        return None;
    }
    let mut pending = apple_events.pending_resolutions.pop().unwrap();
    let error = cpu.gpr[3] as u16 as i16;
    if error != PPC_NO_ERR || pending.index + 1 == pending.levels.len() {
        if error != PPC_NO_ERR {
            let output = if pending.index + 1 == pending.levels.len() {
                pending.token_ptr
            } else {
                pending.scratch_ptr
            };
            let token_type = memory.read_u32_be(output).unwrap_or(PPC_TYPE_NULL);
            let handle = memory.read_u32_be(output + 4).unwrap_or(0);
            if token_type != PPC_TYPE_NULL || handle != 0 {
                pending.intermediate_tokens.push((token_type, handle));
            }
        }
        return Some(ppc_begin_ae_resolve_cleanup(
            pending,
            error,
            cpu,
            apple_events,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            toolbox_startup,
        ));
    }

    let Some(container) = ppc_ae_descriptor(memory, &apple_events.descriptors, pending.scratch_ptr)
    else {
        return Some(ppc_begin_ae_resolve_cleanup(
            pending,
            PPC_ERR_AE_DESC_NOT_FOUND,
            cpu,
            apple_events,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            toolbox_startup,
        ));
    };
    let handle = memory.read_u32_be(pending.scratch_ptr + 4).unwrap_or(0);
    if container.desc_type != PPC_TYPE_NULL || handle != 0 {
        pending.intermediate_tokens.push((container.desc_type, handle));
    }
    pending.container_class = pending.levels[pending.index].desired_class;
    pending.container_type = container.desc_type;
    pending.container_handle = handle;
    pending.index += 1;
    match ppc_dispatch_ae_resolve_level(
        &mut pending,
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        apple_events,
        toolbox_startup,
    ) {
        Ok(action) => {
            apple_events.pending_resolutions.push(pending);
            Some(action)
        }
        Err(error) => {
            Some(ppc_begin_ae_resolve_cleanup(
                pending,
                error,
                cpu,
                apple_events,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                toolbox_startup,
            ))
        }
    }
}
