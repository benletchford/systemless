//! Typed Gestalt Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcGestaltDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
}

pub(super) fn dispatch_gestalt_import(
    context: PpcGestaltDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcGestaltDispatchContext {
        binding,
        cpu,
        memory,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::Gestalt => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_gestalt(cpu, memory, toolbox_startup),
        ))),
        PpcImportDispatcherTarget::NewGestaltValue => {
            let selector = cpu.gpr[3];
            let value = cpu.gpr[4];
            let error = if ppc_gestalt_response(selector).is_some()
                || toolbox_startup.gestalt_values.contains_key(&selector)
            {
                PPC_GESTALT_DUP_SELECTOR_ERR
            } else {
                toolbox_startup.gestalt_values.insert(selector, value);
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        _ => None,
    }
}
