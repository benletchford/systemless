use super::*;

pub(super) struct PpcFileDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) files: &'a mut Vec<PpcFileRecord>,
    pub(super) writable_refnums: &'a mut HashSet<u16>,
    pub(super) vfs_files: &'a mut ProcessVfsFileRecords,
    pub(super) vfs_directories: &'a mut Vec<PpcVfsDirectory>,
    pub(super) next_vfs_dir_id: &'a mut u32,
    pub(super) deleted_vfs_file_paths: &'a mut Vec<String>,
    pub(super) vfs_resource_files: &'a mut ProcessVfsResourceFileRecords,
    pub(super) resource_files: &'a mut Vec<PpcResourceFileRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) next_file_ref_num: &'a mut i16,
    pub(super) current_resource_refnum: &'a mut i16,
    pub(super) last_resource_error: &'a mut i16,
    pub(super) default_dir_id: u32,
    pub(super) launched_app_path: Option<&'a str>,
    pub(super) vfs_volumes: &'a [PpcVfsVolumeRecord],
    pub(super) working_directories: &'a mut HashMap<i16, ProcessWorkingDirectory>,
    pub(super) next_working_directory_ref_num: &'a mut i16,
    pub(super) application_working_directory_ref_num: &'a mut i16,
}

pub(super) fn dispatch_file_import(context: PpcFileDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcFileDispatchContext {
        binding,
        cpu,
        memory,
        files,
        writable_refnums,
        vfs_files,
        vfs_directories,
        next_vfs_dir_id,
        deleted_vfs_file_paths,
        vfs_resource_files,
        resource_files,
        vfs_resources,
        next_file_ref_num,
        current_resource_refnum,
        last_resource_error,
        default_dir_id,
        launched_app_path,
        vfs_volumes,
        working_directories,
        next_working_directory_ref_num,
        application_working_directory_ref_num,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::FSClose => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fs_close(cpu, files, writable_refnums),
        ))),
        PpcImportDispatcherTarget::PBClose => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_close(cpu, memory, files, writable_refnums),
        ))),
        PpcImportDispatcherTarget::PBFlushFile => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_flush_file(cpu, memory, files),
        ))),
        PpcImportDispatcherTarget::FSRead => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fs_read(cpu, memory, files, vfs_files),
        ))),
        PpcImportDispatcherTarget::PBRead => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_read(cpu, memory, files, vfs_files),
        ))),
        PpcImportDispatcherTarget::FSWrite => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fs_write(cpu, memory, files, writable_refnums, vfs_files),
        ))),
        PpcImportDispatcherTarget::PBWrite => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_write(cpu, memory, files, writable_refnums, vfs_files),
        ))),
        PpcImportDispatcherTarget::GetEOF => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_get_eof(cpu, memory, files, vfs_files),
        ))),
        PpcImportDispatcherTarget::PBGetEOF => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_get_eof(cpu, memory, files, vfs_files),
        ))),
        PpcImportDispatcherTarget::SetEOF => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_set_eof(cpu, files, writable_refnums, vfs_files),
        ))),
        PpcImportDispatcherTarget::AllocContig => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_alloc_contig(cpu, memory, files, writable_refnums),
        ))),
        PpcImportDispatcherTarget::PBSetEOF => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_set_eof(cpu, memory, files, writable_refnums, vfs_files),
        ))),
        PpcImportDispatcherTarget::GetFPos => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_get_fpos(cpu, memory, files),
        ))),
        PpcImportDispatcherTarget::SetFPos => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_set_fpos(cpu, files, vfs_files),
        ))),
        PpcImportDispatcherTarget::PBSetFPos => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_set_fpos(cpu, memory, files, vfs_files),
        ))),
        PpcImportDispatcherTarget::PBCreate(operation) => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_create(
                operation,
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::FSpCreate => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fsp_create(cpu, memory, vfs_directories, vfs_files),
        ))),
        PpcImportDispatcherTarget::HCreate => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_h_create(cpu, memory, vfs_directories, vfs_files, default_dir_id),
        ))),
        PpcImportDispatcherTarget::Create => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_create(cpu, memory, vfs_directories, vfs_files, default_dir_id),
        ))),
        PpcImportDispatcherTarget::FSpDelete => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_fsp_delete(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                deleted_vfs_file_paths,
                files,
                vfs_resource_files,
                resource_files,
                vfs_resources,
            ))))
        }
        PpcImportDispatcherTarget::DeleteByName(operation) => {
            let result = ppc_delete_by_name(
                operation,
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                deleted_vfs_file_paths,
                files,
                vfs_resource_files,
                resource_files,
                vfs_resources,
                default_dir_id,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::FSOpen => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_fs_open(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                files,
                writable_refnums,
                next_file_ref_num,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::FSpCreateResFile => {
            ppc_fsp_create_res_file(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HCreateResFile => {
            ppc_h_create_res_file(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
                last_resource_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FSpOpenResFile => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fsp_open_res_file(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                resource_files,
                vfs_resources,
                next_file_ref_num,
                current_resource_refnum,
                last_resource_error,
            ),
        ))),
        PpcImportDispatcherTarget::FSpOpenDF => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_fsp_open_df(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                files,
                writable_refnums,
                next_file_ref_num,
            ))))
        }
        PpcImportDispatcherTarget::HOpen => {
            let result = ppc_h_open(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                files,
                writable_refnums,
                next_file_ref_num,
                default_dir_id,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::PBHOpenDF => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pbh_open_df(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                files,
                writable_refnums,
                next_file_ref_num,
            ))))
        }
        PpcImportDispatcherTarget::CurResFile => Some(PpcImportAction::Return(ppc_i16_result(
            *current_resource_refnum,
        ))),
        PpcImportDispatcherTarget::UseResFile => {
            ppc_set_current_resource_refnum(
                memory,
                current_resource_refnum,
                cpu.gpr[3] as u16 as i16,
            );
            *last_resource_error = 0;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::OpenResFile => Some(PpcImportAction::Return(ppc_open_res_file(
            cpu,
            memory,
            vfs_files,
            vfs_resource_files,
            resource_files,
            vfs_resources,
            next_file_ref_num,
            current_resource_refnum,
            last_resource_error,
            launched_app_path,
        ) as u16
            as u32)),
        PpcImportDispatcherTarget::HOpenResFile => {
            Some(PpcImportAction::Return(ppc_h_open_res_file(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                resource_files,
                vfs_resources,
                next_file_ref_num,
                current_resource_refnum,
                last_resource_error,
                default_dir_id,
            ) as u16 as u32))
        }
        PpcImportDispatcherTarget::ResError => Some(PpcImportAction::Return(ppc_i16_result(
            *last_resource_error,
        ))),
        PpcImportDispatcherTarget::GetVol => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_get_vol(
                cpu,
                memory,
                default_dir_id,
                *application_working_directory_ref_num,
                working_directories,
                vfs_volumes,
            ))))
        }
        PpcImportDispatcherTarget::GetWDInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_get_wd_info(
                cpu,
                memory,
                default_dir_id,
                *application_working_directory_ref_num,
                working_directories,
                vfs_volumes,
            ))))
        }
        PpcImportDispatcherTarget::HGetVol => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_hget_vol(
                cpu,
                memory,
                default_dir_id,
                *application_working_directory_ref_num,
                working_directories,
                vfs_volumes,
            ))))
        }
        PpcImportDispatcherTarget::HSetVol => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_hset_vol(
                cpu,
                memory,
                vfs_directories,
                vfs_volumes,
                default_dir_id,
                working_directories,
                next_working_directory_ref_num,
                application_working_directory_ref_num,
            ))))
        }
        PpcImportDispatcherTarget::FlushVol => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_flush_vol(cpu, memory),
        ))),
        PpcImportDispatcherTarget::PBFlushVol => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_complete_pb(memory, cpu.gpr[3], PPC_NO_ERR),
        ))),
        PpcImportDispatcherTarget::PBHGetVInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pbh_get_v_info(cpu, memory, vfs_volumes),
        ))),
        PpcImportDispatcherTarget::PBGetFInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_get_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
                default_dir_id,
                false,
            ))))
        }
        PpcImportDispatcherTarget::PBHGetFInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_get_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
                default_dir_id,
                true,
            ))))
        }
        PpcImportDispatcherTarget::PBSetFInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_set_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
                false,
            ))))
        }
        PpcImportDispatcherTarget::PBHSetFInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_set_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
                true,
            ))))
        }
        PpcImportDispatcherTarget::FSpGetFInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fsp_get_finfo(cpu, memory, vfs_directories, vfs_files, vfs_resource_files),
        ))),
        PpcImportDispatcherTarget::GetFInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_get_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::HGetFInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_h_get_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::FSpSetFInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fsp_set_finfo(cpu, memory, vfs_directories, vfs_files, vfs_resource_files),
        ))),
        PpcImportDispatcherTarget::HSetFInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_h_set_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::PBGetCatInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_get_cat_info(
                cpu,
                memory,
                vfs_volumes,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
                default_dir_id,
            ),
        ))),
        PpcImportDispatcherTarget::PBSetCatInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_set_cat_info(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
            ),
        ))),
        PpcImportDispatcherTarget::DirCreate => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_dir_create(
                cpu,
                memory,
                vfs_directories,
                next_vfs_dir_id,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::FSpDirCreate => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_fsp_dir_create(
                cpu,
                memory,
                vfs_directories,
                next_vfs_dir_id,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::FSMakeFSSpec => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_fs_make_fsspec(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::PBGetFCBInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_get_fcb_info(
                cpu,
                memory,
                files,
                resource_files,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
                launched_app_path,
            ),
        ))),
        PpcImportDispatcherTarget::FindFolder => {
            let folder_type = cpu.gpr[4];
            let found_vref_ptr = cpu.gpr[6];
            let found_dir_id_ptr = cpu.gpr[7];
            let found_dir_id = ppc_find_folder_dir_id(folder_type);
            if found_vref_ptr == 0
                || found_dir_id_ptr == 0
                || !ppc_memory_can_write_bytes(memory, found_vref_ptr, 2)
                || !ppc_memory_can_write_bytes(memory, found_dir_id_ptr, 4)
            {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else {
                let _ = memory.write_u16_be(found_vref_ptr, PPC_BOOT_VOLUME_REF_NUM as u16);
                let _ = memory.write_u32_be(found_dir_id_ptr, found_dir_id);
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            }
        }
        PpcImportDispatcherTarget::ResolveAliasFile => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_resolve_alias_file(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
            )),
        )),
        PpcImportDispatcherTarget::FileCompatibility(operation) => {
            Some(ppc_dispatch_file_compatibility(
                operation,
                cpu,
                memory,
                files,
                vfs_directories,
                vfs_volumes,
                default_dir_id,
                working_directories,
                next_working_directory_ref_num,
                application_working_directory_ref_num,
            ))
        }
        _ => None,
    }
}
