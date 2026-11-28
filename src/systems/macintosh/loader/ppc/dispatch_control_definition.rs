//! Application control and window definitions entered through the process
//! Mixed Mode gateway.

use super::*;
use crate::systems::macintosh::window_definition::{self, WindowDefinitionEvent};

/// The record a queued definition message is sent for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PpcDefinitionTarget {
    /// A control, by its ControlHandle ('CDEF').
    Control(u32),
    /// A window, by its WindowPtr ('WDEF').
    Window(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PpcDefinitionFrame {
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
    pending: VecDeque<(PpcDefinitionTarget, u16, u32)>,
    /// The color window whose classic port fields hold its pixel map's
    /// bounds while its definition runs, with the fields to write back.
    lent_bounds: Option<(u32, [u32; 2])>,
}

impl PpcDefinitionFrame {
    fn return_lent_bounds(&mut self, memory: &mut PpcSectionMem) {
        if let Some((window, saved)) = self.lent_bounds.take() {
            let _ = memory.write_u32_be(window.wrapping_add(8), saved[0]);
            let _ = memory.write_u32_be(window.wrapping_add(12), saved[1]);
        }
    }
}

/// Queue the messages the Window Manager sends an application window
/// definition for `event`; the definition continuation sends them once the
/// import that queued them completes. Standard definitions are drawn natively.
pub(super) fn ppc_queue_window_definition(
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    window: u32,
    event: WindowDefinitionEvent,
) {
    let proc_id = ppc_window_proc_id(memory, window);
    if window == 0 || !window_definition::is_application_definition(proc_id) {
        return;
    }
    startup.window_definition_messages.extend(
        window_definition::messages(event)
            .iter()
            .map(|&(message, parameter)| (window, message, parameter)),
    );
}

/// The VFS resource of an application window definition the guest can call.
fn ppc_application_window_definition(
    memory: &mut PpcSectionMem,
    resources: &[PpcVfsResourceRecord],
    resource_file: i16,
    proc_id: i16,
) -> Option<usize> {
    if !window_definition::is_application_definition(proc_id) {
        return None;
    }
    let index = ppc_vfs_resource_index(
        resources,
        resource_file,
        u32::from_be_bytes(*b"WDEF"),
        window_definition::resource_id(proc_id),
        false,
    )?;
    ppc_definition_resource_is_callable(memory, &resources[index]).then_some(index)
}

pub(super) fn ppc_application_control_definition(
    memory: &mut PpcSectionMem,
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
    ppc_definition_resource_is_callable(memory, &resources[index]).then_some(index)
}

fn ppc_definition_resource_is_callable(
    memory: &mut PpcSectionMem,
    resource: &PpcVfsResourceRecord,
) -> bool {
    // A loaded definition's handle holds the bytes the Control Manager calls;
    // applications may patch a stub there after GetResource.
    let loaded = (resource.handle != 0)
        .then(|| memory.read_u32_be(resource.handle))
        .flatten()
        .filter(|ptr| *ptr != 0)
        .and_then(|ptr| memory.read_u16_be(ptr));
    let Some(word) = loaded.or_else(|| {
        let bytes = resource.data.get(..2)?;
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }) else {
        return false;
    };
    word == PPC_MIXED_MODE_TRAP || crate::guest_procedure::m68k_definition_entry_is_callable(word)
}

pub(super) fn ppc_control_definition_resource(
    memory: &mut PpcSectionMem,
    resources: &[PpcVfsResourceRecord],
    resource_file: i16,
    record: &PpcControlRecord,
) -> Option<usize> {
    let (kind, _) = crate::control_manager::standard_control_kind(record.proc_id);
    if matches!(kind, 0 | 1 | 2 | 16) || (1008..=1023).contains(&record.proc_id) {
        return None;
    }
    let handle = memory.read_u32_be(record.pointer + 24).unwrap_or(0);
    resources
        .iter()
        .position(|resource| handle != 0 && resource.handle == handle)
        .filter(|&index| ppc_definition_resource_is_callable(memory, &resources[index]))
        .or_else(|| {
            ppc_application_control_definition(memory, resources, resource_file, record.proc_id)
        })
}

/// Whether this import entry is a guest control or window definition
/// returning to the import that queued it, so the remaining queued messages
/// must be resumed before the import itself is dispatched again.
pub(super) fn ppc_definition_resume_pending(
    startup: &PpcToolboxStartupState,
    cpu: &PpcCpu,
) -> bool {
    startup
        .definition_stack
        .last()
        .is_some_and(|frame| frame.import_pc == cpu.pc && cpu.lr == cpu.pc)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_definition_continuation(
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
    let resumed = ppc_definition_resume_pending(startup, cpu);
    let mut frame = if resumed && completion.is_none() {
        startup.definition_stack.pop().unwrap()
    } else if let Some(completion) = completion {
        if startup.window_definition_messages.is_empty()
            && !controls.iter().any(|control| {
                control.native_definition_pending_init
                    || !control.native_definition_draws.borrow().is_empty()
            })
        {
            return Some(completion);
        }
        let mut handles = manager.native_handle_records().to_vec();
        // A window's frame comes before the controls drawn in it.
        let mut pending: VecDeque<_> = startup
            .window_definition_messages
            .drain(..)
            .map(|(window, message, parameter)| {
                (
                    PpcDefinitionTarget::Window(window),
                    message as u16,
                    parameter,
                )
            })
            .collect();
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
                            pending.push_back((PpcDefinitionTarget::Control(control.handle), 3, 0));
                        }
                    }
                }
            }
            for sequence in control.native_definition_draws.borrow_mut().drain(..) {
                draws.push((sequence, control.handle));
            }
        }
        draws.sort_by_key(|&(sequence, _)| sequence);
        pending.extend(
            draws
                .into_iter()
                .map(|(_, handle)| (PpcDefinitionTarget::Control(handle), 0, 0)),
        );
        if pending.is_empty() {
            return Some(completion);
        }
        PpcDefinitionFrame {
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
            lent_bounds: None,
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
                pending.push_back((PpcDefinitionTarget::Control(control.handle), 4, 0));
            }
        }
        if pending.is_empty() {
            return None;
        }
        PpcDefinitionFrame {
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
            lent_bounds: None,
        }
    };

    frame.return_lent_bounds(memory);
    while let Some((definition, message, parameter)) = frame.pending.pop_front() {
        // The definition handle, the record argument, the variation code and
        // the port the definition draws in.
        let (definition_handle, record, variation, definition_port) = match definition {
            PpcDefinitionTarget::Control(handle) => {
                let Some(control) = controls.iter().find(|control| control.handle == handle) else {
                    continue;
                };
                (
                    memory.read_u32_be(control.pointer + 24).unwrap_or(0),
                    handle,
                    control.proc_id as u16 & 15,
                    memory
                        .read_u32_be(control.pointer + PPC_CONTROL_OWNER_OFFSET)
                        .unwrap_or(frame.port),
                )
            }
            PpcDefinitionTarget::Window(window) => {
                let proc_id = ppc_window_proc_id(memory, window);
                let Some(index) =
                    ppc_application_window_definition(memory, resources, resource_file, proc_id)
                else {
                    continue;
                };
                let mut handles = manager.native_handle_records().to_vec();
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
                // Definitions draw in the Window Manager port, in global
                // coordinates (Toolbox Essentials (1992), p. 4-121).
                (
                    handle,
                    window,
                    window_definition::variation(proc_id) as u16,
                    PPC_MAIN_GWORLD,
                )
            }
        };
        // Materializing a definition resource can fail and return a nil
        // handle; never read through it.
        if definition_handle == 0 {
            continue;
        }
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
        *port = definition_port;
        if startup.init_graf_global_ptr != 0 {
            let _ = memory.write_u32_be(startup.init_graf_global_ptr, *port);
        }
        if let PpcDefinitionTarget::Window(window) = definition {
            if let Some(bounds) = window_definition::legacy_port_bounds(memory, window) {
                let _ = memory.write_u32_be(window + 8, bounds.lent[0]);
                let _ = memory.write_u32_be(window + 12, bounds.lent[1]);
                frame.lent_bounds = Some((window, bounds.saved));
            }
        }
        let arguments = vec![u32::from(variation), record, u32::from(message), parameter];
        // uppControlDefProcInfo and uppWindowDefProcInfo describe the same
        // Pascal signature.
        let proc_info = match definition {
            PpcDefinitionTarget::Control(_) => 0x3bb0,
            PpcDefinitionTarget::Window(_) => window_definition::PROC_INFO,
        };
        cpu.lr = frame.import_pc;
        let action = match target.isa {
            GuestIsa::M68k => {
                // A native application's data section is not a classic A5
                // world. Give resource code a real QDGlobals pointer chain,
                // without overwriting any application data.
                if startup.definition_globals == 0 {
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
                    startup.definition_globals = globals;
                    let _ = ppc_init_graf(memory, gworlds, globals + 126);
                }
                let globals = startup.definition_globals;
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
                    proc_info,
                    arguments,
                    frame.import_pc,
                    globals + 130,
                )
            }
            GuestIsa::PowerPc => {
                // Without room for the parameter area, skip the remaining
                // messages. The epilogue below still restores the caller's
                // port, its QDGlobals word and registers, and delivers the
                // completion.
                if install_powerpc_call_arguments(cpu, memory, &arguments).is_none() {
                    break;
                }
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
            startup.definition_stack.push(frame);
            return Some(action);
        }
        frame.return_lent_bounds(memory);
    }
    frame.return_lent_bounds(memory);
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
