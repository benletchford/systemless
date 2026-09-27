//! Typed Desk Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcDeskDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
}

pub(super) fn dispatch_desk_import(context: PpcDeskDispatchContext<'_>) -> Option<PpcImportAction> {
    match context.binding.dispatcher_target {
        PpcImportDispatcherTarget::SystemTask | PpcImportDispatcherTarget::SystemClick => {
            // Macintosh Toolbox Essentials (1992), pp. 2-94--2-95: these
            // routines service desk-accessory windows and periodic driver
            // actions. Native HLE exposes neither class; device timing is
            // advanced by the runner, so the observable call is quiescent.
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::OpenDeskAcc => {
            // Inside Macintosh: Devices (1994), p. 1-65: callers must ignore
            // this result unless a desk accessory was successfully opened.
            // There are no classic DRVR desk accessories in the PPC process.
            Some(PpcImportAction::Return(0))
        }
        _ => None,
    }
}
