use super::*;

pub(super) struct PpcResourceDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) current_resource_refnum: &'a mut i16,
    pub(super) resource_policy: &'a SharedProcessResourcePolicy,
    pub(super) last_resource_error: &'a mut i16,
}

pub(super) fn dispatch_resource_import(
    context: PpcResourceDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcResourceDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        current_resource_refnum,
        resource_policy,
        last_resource_error,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::SetResLoad => {
            // Inside Macintosh Volume I (1985), I-118: SetResLoad controls
            // whether subsequent Resource Manager lookups load resource data.
            resource_policy.set_res_load(cpu.gpr[3] != 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LoadResource => {
            // Inside Macintosh Volume I (1985), I-120: LoadResource fills an
            // empty resource handle and reports resNotFound for other handles.
            let handle = cpu.gpr[3];
            if let Some(index) = vfs_resources
                .iter()
                .position(|resource| resource.handle == handle)
            {
                let _ = ppc_materialize_vfs_resource_handle(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    vfs_resources,
                    index,
                    true,
                    last_resource_error,
                );
            } else {
                *last_resource_error = PPC_RES_NOT_FOUND_ERR;
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetResource => {
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] GetResource type='{}' id={} current_refnum={}",
                    format_ppc_fourcc(cpu.gpr[3]),
                    cpu.gpr[4] as i16,
                    *current_resource_refnum
                );
            }
            Some(PpcImportAction::Return(ppc_get_resource(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                false,
                resource_policy.res_load(),
                last_resource_error,
            )))
        }
        PpcImportDispatcherTarget::Get1Resource => {
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] Get1Resource type='{}' id={} current_refnum={}",
                    format_ppc_fourcc(cpu.gpr[3]),
                    cpu.gpr[4] as i16,
                    *current_resource_refnum
                );
            }
            Some(PpcImportAction::Return(ppc_get_resource(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                true,
                resource_policy.res_load(),
                last_resource_error,
            )))
        }
        PpcImportDispatcherTarget::GetNamedResource
        | PpcImportDispatcherTarget::Get1NamedResource => {
            let current_only = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::Get1NamedResource
            );
            Some(PpcImportAction::Return(ppc_get_named_resource(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                current_only,
                resource_policy.res_load(),
                last_resource_error,
            )))
        }
        PpcImportDispatcherTarget::GetIndResource | PpcImportDispatcherTarget::Get1IndResource => {
            let current_only = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::Get1IndResource
            );
            Some(PpcImportAction::Return(ppc_get_ind_resource(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                current_only,
                resource_policy.res_load(),
                last_resource_error,
            )))
        }
        PpcImportDispatcherTarget::CountResources => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_count_resources(
                cpu,
                vfs_resources,
                *current_resource_refnum,
                false,
                last_resource_error,
            ),
        ))),
        PpcImportDispatcherTarget::Count1Resources => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_count_resources(
                cpu,
                vfs_resources,
                *current_resource_refnum,
                true,
                last_resource_error,
            )),
        )),
        PpcImportDispatcherTarget::UniqueID => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_unique_id(
                cpu,
                vfs_resources,
                *current_resource_refnum,
                false,
                last_resource_error,
            ))))
        }
        PpcImportDispatcherTarget::Unique1ID => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_unique_id(
                cpu,
                vfs_resources,
                *current_resource_refnum,
                true,
                last_resource_error,
            ))))
        }
        PpcImportDispatcherTarget::ReleaseResource => {
            ppc_release_resource(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DetachResource => {
            ppc_detach_resource(
                cpu,
                process_memory_manager,
                vfs_resources,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
