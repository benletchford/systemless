//! Typed Gestalt Manager dispatch for PowerPC imports.

use super::*;
use crate::gestalt_manager::{
    evaluate_gestalt_query, evaluate_gestalt_query_parameters, evaluate_new_gestalt,
    is_builtin_gestalt_selector, GestaltEvaluationContext, NewGestaltAction,
};

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
            let eval_context =
                GestaltEvaluationContext::for_powerpc(toolbox_startup.physical_ram_size);
            let is_known = is_builtin_gestalt_selector(selector, &eval_context)
                || toolbox_startup.gestalt_values.contains_key(&selector);
            let action = evaluate_new_gestalt(selector, value, is_known);
            let error = match action {
                NewGestaltAction::DuplicateSelector => PPC_GESTALT_DUP_SELECTOR_ERR,
                NewGestaltAction::Register { selector, value } => {
                    toolbox_startup.gestalt_values.insert(selector, value);
                    PPC_NO_ERR
                }
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        _ => None,
    }
}

fn ppc_gestalt(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    toolbox_startup: &PpcToolboxStartupState,
) -> i16 {
    let selector = cpu.gpr[3];
    let response_ptr = cpu.gpr[4];
    let Ok(params) = evaluate_gestalt_query_parameters(selector, response_ptr) else {
        return PPC_PARAM_ERR;
    };

    let eval_context = GestaltEvaluationContext::for_powerpc(toolbox_startup.physical_ram_size);
    let dynamic_value = toolbox_startup.gestalt_values.get(&selector).copied();
    let query_eval = evaluate_gestalt_query(params.selector, &eval_context, dynamic_value);

    if memory
        .write_u32_be(params.response_ptr, query_eval.response)
        .is_none()
    {
        return PPC_PARAM_ERR;
    }

    if ppc_hle_trace_enabled() {
        if query_eval.error_code != PPC_NO_ERR {
            eprintln!(
                "[PPC-TRACE] Gestalt({:?}) -> gestaltUndefSelectorErr",
                ppc_res_type_text(selector)
            );
        } else {
            eprintln!(
                "[PPC-TRACE] Gestalt({:?}) -> ${:08X} err={} lr=${:08X}",
                ppc_res_type_text(selector),
                query_eval.response,
                query_eval.error_code,
                cpu.lr
            );
        }
    }

    query_eval.error_code
}
