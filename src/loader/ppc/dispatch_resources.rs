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
    pub(super) resource_files: &'a mut Vec<PpcResourceFileRecord>,
    pub(super) vfs_resource_files: &'a mut ProcessVfsResourceFileRecords,
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
        resource_files,
        vfs_resource_files,
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
        PpcImportDispatcherTarget::GetIndString => {
            ppc_get_ind_string(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        // FUNCTION GetString (stringID: Integer): StringHandle;
        // Inside Macintosh: Text (1993), 5-49 (lines 15621-15642).
        // GetString is the Text Utilities wrapper around
        // GetResource('STR ', stringID), including its NIL-on-miss behavior.
        PpcImportDispatcherTarget::GetString => {
            let string_id = cpu.gpr[3];
            cpu.gpr[3] = u32::from_be_bytes(*b"STR ");
            cpu.gpr[4] = string_id;
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
        PpcImportDispatcherTarget::GetResAttrs => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_get_res_attrs(cpu, vfs_resources, last_resource_error),
        ))),
        PpcImportDispatcherTarget::SetResAttrs => {
            ppc_set_res_attrs(cpu, vfs_resource_files, vfs_resources, last_resource_error);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetResInfo => {
            ppc_get_res_info(cpu, memory, vfs_resources, last_resource_error);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetResourceSizeOnDisk => {
            // More Macintosh Toolbox (1993), 1-105: this reports the exact
            // on-disk resource size even when SetResLoad left its handle empty.
            let size = vfs_resources
                .iter()
                .find(|resource| resource.handle == cpu.gpr[3])
                .and_then(|resource| i32::try_from(resource.data.len()).ok());
            if let Some(size) = size {
                *last_resource_error = PPC_NO_ERR;
                Some(PpcImportAction::Return(size as u32))
            } else {
                *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                Some(PpcImportAction::Return(u32::MAX))
            }
        }
        PpcImportDispatcherTarget::SetResInfo => {
            ppc_set_res_info(
                cpu,
                memory,
                vfs_resource_files,
                vfs_resources,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HomeResFile => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_home_res_file(cpu, vfs_resources, last_resource_error),
        ))),
        PpcImportDispatcherTarget::UpdateResFile => {
            ppc_update_res_file(
                cpu,
                memory,
                handles,
                resource_files,
                vfs_resource_files,
                vfs_resources,
                *current_resource_refnum,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::AddResource => {
            *last_resource_error = ppc_add_resource(
                cpu,
                process_memory_manager,
                memory,
                handles,
                resource_files,
                vfs_resource_files,
                vfs_resources,
                *current_resource_refnum,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ChangedResource => {
            ppc_changed_resource(cpu, vfs_resource_files, vfs_resources, last_resource_error);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::WriteResource => {
            ppc_write_resource(
                cpu,
                memory,
                handles,
                vfs_resource_files,
                vfs_resources,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::RemoveResource => {
            ppc_remove_resource(
                cpu,
                process_memory_manager,
                vfs_resource_files,
                vfs_resources,
                *current_resource_refnum,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ReadPartialResource => {
            ppc_read_partial_resource(cpu, memory, vfs_resources, last_resource_error);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CloseResFile => {
            ppc_close_res_file(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                resource_files,
                vfs_resource_files,
                vfs_resources,
                current_resource_refnum,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}
