//! Typed Gestalt Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcGestaltDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
}

pub(super) fn dispatch_gestalt_import(
    context: PpcGestaltDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcGestaltDispatchContext {
        binding,
        cpu,
        memory,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::Gestalt => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_gestalt(cpu, memory),
        ))),
        _ => None,
    }
}
