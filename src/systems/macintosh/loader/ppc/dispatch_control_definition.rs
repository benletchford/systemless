//! Application control definitions entered through the process Mixed Mode gateway.

use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PpcControlDefinitionFrame {
    import_pc: u32,
    gpr: [u32; 32],
    fpr: [u64; 32],
    cr: u32,
    lr: u32,
    ctr: u32,
    xer: u32,
    fpscr: u32,
    port: u32,
    device: u32,
    completion: Option<PpcImportAction>,
    pending: VecDeque<(u32, u16, u32)>,
}

pub(super) fn ppc_application_control_definition(
    resources: &[PpcVfsResourceRecord],
    resource_file: i16,
    proc_id: i16,
) -> Option<usize> {
    if matches!(proc_id, 0 | 1 | 2 | 16) || (1008..=1023).contains(&proc_id) {
        return None;
    }
    // The entire unsigned procID identifies the resource; only its low four
    // bits are the variant. Inside Macintosh: Toolbox Essentials, Control
    // Manager, "Defining Your Own Control Definition Function".
    let index = ppc_vfs_resource_index(
        resources,
        resource_file,
        u32::from_be_bytes(*b"CDEF"),
        ((proc_id as u16) >> 4) as i16,
        false,
    )?;
    ppc_control_definition_is_callable(&resources[index].data).then_some(index)
}

fn ppc_control_definition_is_callable(bytes: &[u8]) -> bool {
    let Some(bytes) = bytes.get(..2) else {
        return false;
    };
    matches!(
        u16::from_be_bytes([bytes[0], bytes[1]]),
        0x4e56 | 0x48e7 | 0x4ef9 | 0x4efa | 0x6000..=0x60ff | PPC_MIXED_MODE_TRAP
    )
}

pub(super) fn ppc_control_definition_resource(
    memory: &mut PpcSectionMem,
    resources: &[PpcVfsResourceRecord],
    resource_file: i16,
    record: &PpcControlRecord,
) -> Option<usize> {
    if matches!(record.proc_id, 0 | 1 | 2 | 16) || (1008..=1023).contains(&record.proc_id) {
        return None;
    }
    let handle = memory.read_u32_be(record.pointer + 24).unwrap_or(0);
    resources
        .iter()
        .position(|resource| {
            handle != 0
                && resource.handle == handle
                && ppc_control_definition_is_callable(&resource.data)
        })
        .or_else(|| ppc_application_control_definition(resources, resource_file, record.proc_id))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_control_definition_continuation(
    binding: &PpcImportBinding,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    controls: &mut [PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    resources: &mut [PpcVfsResourceRecord],
    resource_file: i16,
    last_resource_error: &mut i16,
    startup: &mut PpcToolboxStartupState,
    port: &mut u32,
    device: &mut u32,
    completion: Option<PpcImportAction>,
) -> Option<PpcImportAction> {
    let resumed = startup
        .control_definition_stack
        .last()
        .is_some_and(|frame| frame.import_pc == cpu.pc && cpu.lr == cpu.pc);
    let mut frame = if resumed && completion.is_none() {
        startup.control_definition_stack.pop().unwrap()
    } else if let Some(completion) = completion {
        if !controls.iter().any(|control| {
            control.native_definition_pending_init
                || !control.native_definition_draws.borrow().is_empty()
        }) {
            return Some(completion);
        }
        let mut handles = manager.native_handle_records().to_vec();
        let mut pending = VecDeque::new();
        let mut draws = Vec::new();
        for control in controls.iter_mut() {
            let index = ppc_control_definition_resource(memory, resources, resource_file, control);
            let initializing = control.native_definition_pending_init;
            control.native_definition_pending_init = false;
            if initializing || !control.native_definition_draws.borrow().is_empty() {
                if let Some(index) = index {
                    let handle = ppc_materialize_vfs_resource_handle(
                        manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        &mut handles,
                        resources,
                        index,
                        true,
                        last_resource_error,
                    );
                    if handle != 0 {
                        let _ = memory.write_u32_be(control.pointer + 24, handle);
                        if initializing {
                            pending.push_back((control.handle, 3, 0));
                        }
                    }
                }
            }
            for sequence in control.native_definition_draws.borrow_mut().drain(..) {
                draws.push((sequence, control.handle));
            }
        }
        draws.sort_by_key(|&(sequence, _)| sequence);
        pending.extend(draws.into_iter().map(|(_, handle)| (handle, 0, 0)));
        if pending.is_empty() {
            return Some(completion);
        }
        PpcControlDefinitionFrame {
            import_pc: cpu.pc,
            gpr: cpu.gpr,
            fpr: cpu.fpr,
            cr: cpu.cr,
            lr: cpu.lr,
            ctr: cpu.ctr,
            xer: cpu.xer,
            fpscr: cpu.fpscr,
            port: *port,
            device: *device,
            completion: Some(completion),
            pending,
        }
    } else {
        if !matches!(
            binding.dispatcher_target,
            PpcImportDispatcherTarget::LegacyControl(PpcLegacyControlOperation::DisposeControl)
                | PpcImportDispatcherTarget::CloseDialog
                | PpcImportDispatcherTarget::DisposeDialog
                | PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow)
        ) {
            return None;
        }
        let mut pending = VecDeque::new();
        for control in controls.iter() {
            let disposing = match binding.dispatcher_target {
                PpcImportDispatcherTarget::LegacyControl(
                    PpcLegacyControlOperation::DisposeControl,
                ) => control.handle == cpu.gpr[3],
                PpcImportDispatcherTarget::CloseDialog
                | PpcImportDispatcherTarget::DisposeDialog
                | PpcImportDispatcherTarget::LegacyWindow(
                    PpcLegacyWindowOperation::DisposeWindow,
                ) => {
                    memory.read_u32_be(control.pointer + PPC_CONTROL_OWNER_OFFSET)
                        == Some(cpu.gpr[3])
                }
                _ => false,
            };
            if disposing
                && ppc_control_definition_resource(memory, resources, resource_file, control)
                    .is_some()
                && memory.read_u32_be(control.pointer + 24).unwrap_or(0) != 0
            {
                pending.push_back((control.handle, 4, 0));
            }
        }
        if pending.is_empty() {
            return None;
        }
        PpcControlDefinitionFrame {
            import_pc: cpu.pc,
            gpr: cpu.gpr,
            fpr: cpu.fpr,
            cr: cpu.cr,
            lr: cpu.lr,
            ctr: cpu.ctr,
            xer: cpu.xer,
            fpscr: cpu.fpscr,
            port: *port,
            device: *device,
            completion: None,
            pending,
        }
    };

    while let Some((handle, message, parameter)) = frame.pending.pop_front() {
        let Some(control) = controls.iter().find(|control| control.handle == handle) else {
            continue;
        };
        let definition_handle = memory.read_u32_be(control.pointer + 24).unwrap_or(0);
        let entry = memory.read_u32_be(definition_handle).unwrap_or(0);
        if entry == 0 {
            continue;
        }
        let Some(target) = resolve_guest_procedure(
            memory,
            entry,
            frame.gpr[2],
            None,
            GuestIsa::PowerPc,
            GuestIsa::M68k,
        ) else {
            continue;
        };
        *port = memory
            .read_u32_be(control.pointer + PPC_CONTROL_OWNER_OFFSET)
            .unwrap_or(frame.port);
        if startup.init_graf_global_ptr != 0 {
            let _ = memory.write_u32_be(startup.init_graf_global_ptr, *port);
        }
        let arguments = vec![
            u32::from(control.proc_id as u16 & 15),
            handle,
            u32::from(message),
            parameter,
        ];
        cpu.lr = frame.import_pc;
        let action = match target.isa {
            GuestIsa::M68k => {
                // A native application's data section is not a classic A5
                // world. Give resource code a real QDGlobals pointer chain,
                // without overwriting any application data.
                if startup.control_definition_globals == 0 {
                    let globals = manager.new_native_ptr(memory, 134, true);
                    ppc_apply_process_native_allocator(
                        manager,
                        memory,
                        heap_cursor,
                        last_mem_error,
                    );
                    if globals == 0 {
                        break;
                    }
                    startup.control_definition_globals = globals;
                    let _ = ppc_init_graf(memory, gworlds, globals + 126);
                }
                let globals = startup.control_definition_globals;
                let the_port = if startup.init_graf_global_ptr != 0 {
                    startup.init_graf_global_ptr
                } else {
                    globals + 126
                };
                let _ = memory.write_u32_be(the_port, *port);
                let _ = memory.write_u32_be(globals + 130, the_port);
                dispatch_mixed_mode::ppc_begin_m68k_universal_proc_with_a5(
                    cpu,
                    Some(manager),
                    memory,
                    heap_cursor,
                    heap_limit,
                    startup,
                    target,
                    0x3bb0,
                    arguments,
                    frame.import_pc,
                    globals + 130,
                )
            }
            GuestIsa::PowerPc => {
                install_powerpc_call_arguments(cpu, memory, &arguments)?;
                GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: target.isa,
                        entry: target.entry,
                        rtoc: target.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        frame.import_pc,
                        frame.gpr[2],
                        PpcNativeReturnGpr3::Preserve,
                    ),
                )
                .into_ppc_import_action()
            }
        };
        if let Some(action) = action {
            startup.control_definition_stack.push(frame);
            return Some(action);
        }
    }
    cpu.gpr = frame.gpr;
    cpu.fpr = frame.fpr;
    cpu.cr = frame.cr;
    cpu.lr = frame.lr;
    cpu.ctr = frame.ctr;
    cpu.xer = frame.xer;
    cpu.fpscr = frame.fpscr;
    *port = frame.port;
    *device = frame.device;
    if startup.init_graf_global_ptr != 0 {
        let _ = memory.write_u32_be(startup.init_graf_global_ptr, *port);
    }
    frame.completion
}
