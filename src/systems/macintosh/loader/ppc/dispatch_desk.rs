//! Typed Desk Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcDeskDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
}

pub(super) fn dispatch_desk_import(context: PpcDeskDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcDeskDispatchContext { binding, cpu } = context;
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::SystemTask => {
            // Macintosh Toolbox Essentials (1992), pp. 2-94--2-95: services
            // desk-accessory windows and periodic driver actions. Native HLE exposes
            // neither class; device timing is advanced by the runner.
            let _action = crate::desk_manager::evaluate_system_task();
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SystemClick => {
            // Inside Macintosh Volume I (1985), pp. I-90--I-91 and I-440--I-441:
            // passes system-window mouse-down event to its desk accessory.
            let _params =
                crate::desk_manager::evaluate_system_click_parameters(cpu.gpr[3], cpu.gpr[4]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::OpenDeskAcc => {
            // Inside Macintosh: Devices (1994), p. 1-65: callers must ignore
            // this result unless a desk accessory was successfully opened.
            // There are no classic DRVR desk accessories in the PPC process.
            let params = crate::desk_manager::evaluate_open_desk_acc_parameters(cpu.gpr[3]);
            Some(PpcImportAction::Return(params.ref_num() as u32))
        }
        PpcImportDispatcherTarget::CloseDeskAcc => {
            // Inside Macintosh Volume I (1985), p. I-440: closes the desk accessory
            // identified by driver reference number.
            let _params =
                crate::desk_manager::evaluate_close_desk_acc_parameters(cpu.gpr[3] as i16);
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
