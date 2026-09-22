use super::*;

pub(super) struct PpcFileDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) aliases: &'a mut Vec<PpcAliasRecord>,
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
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        handles,
        aliases,
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
        PpcImportDispatcherTarget::PBOpen => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_open(
                cpu,
                memory,
                vfs_directories,
                vfs_volumes,
                vfs_files,
                files,
                writable_refnums,
                next_file_ref_num,
                default_dir_id,
                *application_working_directory_ref_num,
                working_directories,
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
                working_directories,
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
        PpcImportDispatcherTarget::ResolveAlias => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_resolve_alias(cpu, memory, vfs_directories, handles, aliases),
        ))),
        PpcImportDispatcherTarget::UpdateAlias => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_update_alias(
                cpu,
                memory,
                last_mem_error,
                vfs_directories,
                handles,
                aliases,
            ))))
        }
        PpcImportDispatcherTarget::NewAlias => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_new_alias(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                aliases,
            ))))
        }
        _ => None,
    }
}

pub(super) fn ppc_dispatch_file_compatibility(
    operation: PpcFileCompatibilityOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut Vec<PpcFileRecord>,
    vfs_directories: &[PpcVfsDirectory],
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    application_working_directory_ref_num: &mut i16,
) -> PpcImportAction {
    match operation {
        PpcFileCompatibilityOperation::PbGetFPosSync => {
            let pb = cpu.gpr[3];
            let ref_num = memory.read_u16_be(pb + 24).map(|value| value as i16);
            let result = ref_num
                .and_then(|ref_num| files.iter().find(|file| file.ref_num == ref_num))
                .map_or(PPC_RF_NUM_ERR, |file| {
                    if memory.write_u32_be(pb + 46, file.position).is_some() {
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                });
            PpcImportAction::Return(ppc_i16_result(ppc_complete_pb(memory, pb, result)))
        }
        PpcFileCompatibilityOperation::PbHGetVolSync => {
            let pb = cpu.gpr[3];
            let working_directory = ppc_working_directory_info(
                0,
                *application_working_directory_ref_num,
                default_dir_id,
                working_directories,
                vfs_volumes,
            )
            .unwrap_or(ProcessWorkingDirectory {
                ref_num: PPC_BOOT_VOLUME_REF_NUM,
                volume_ref_num: PPC_BOOT_VOLUME_REF_NUM,
                dir_id: default_dir_id,
                proc_id: 0,
            });
            if let Some(name) = memory.read_u32_be(pb + 18).filter(|name| *name != 0) {
                let _ = ppc_write_pstring_bytes(
                    memory,
                    name,
                    ppc_volume_name_for_ref_num(working_directory.volume_ref_num, vfs_volumes),
                );
            }
            let _ = memory.write_u16_be(pb + 22, working_directory.ref_num as u16);
            let _ = memory.write_u32_be(pb + 28, working_directory.proc_id);
            let _ = memory.write_u16_be(pb + 32, working_directory.volume_ref_num as u16);
            let _ = memory.write_u32_be(pb + 48, working_directory.dir_id);
            PpcImportAction::Return(ppc_i16_result(ppc_complete_pb(memory, pb, PPC_NO_ERR)))
        }
        PpcFileCompatibilityOperation::PbOpenWdSync => {
            let pb = cpu.gpr[3];
            let requested_vref = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
            let requested_dir_id = memory.read_u32_be(pb + 48).unwrap_or(0);
            let proc_id = memory.read_u32_be(pb + 28).unwrap_or(0);
            let volume_ref_num = working_directories
                .get(&requested_vref)
                .map(|record| record.volume_ref_num)
                .or_else(|| {
                    (requested_vref == PPC_BOOT_VOLUME_REF_NUM
                        || vfs_volumes
                            .iter()
                            .any(|volume| volume.ref_num == requested_vref))
                    .then_some(requested_vref)
                })
                .unwrap_or(PPC_BOOT_VOLUME_REF_NUM);
            let effective_dir_id = if requested_dir_id <= 1 {
                working_directories
                    .get(&requested_vref)
                    .map(|record| record.dir_id)
                    .unwrap_or_else(|| {
                        ppc_resolve_directory_id(requested_vref, requested_dir_id, default_dir_id)
                    })
            } else {
                requested_dir_id
            };
            let result = if ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none() {
                PPC_FNF_ERR
            } else {
                let root_dir_id = if volume_ref_num == PPC_BOOT_VOLUME_REF_NUM {
                    PPC_ROOT_DIR_ID
                } else {
                    vfs_volumes
                        .iter()
                        .find(|volume| volume.ref_num == volume_ref_num)
                        .map(|volume| volume.root_dir_id)
                        .unwrap_or(PPC_ROOT_DIR_ID)
                };
                let wd_ref_num = if effective_dir_id == root_dir_id {
                    volume_ref_num
                } else if let Some(existing) = working_directories.values().find(|record| {
                    record.volume_ref_num == volume_ref_num
                        && record.dir_id == effective_dir_id
                        && record.proc_id == proc_id
                }) {
                    existing.ref_num
                } else {
                    let mut ref_num = *next_working_directory_ref_num;
                    while working_directories.contains_key(&ref_num) {
                        ref_num = ref_num.saturating_add(1);
                    }
                    *next_working_directory_ref_num = ref_num.saturating_add(1);
                    working_directories.insert(
                        ref_num,
                        ProcessWorkingDirectory {
                            ref_num,
                            volume_ref_num,
                            dir_id: effective_dir_id,
                            proc_id,
                        },
                    );
                    ref_num
                };
                let _ = memory.write_u16_be(pb + 22, wd_ref_num as u16);
                let _ = memory.write_u16_be(pb + 32, volume_ref_num as u16);
                let _ = memory.write_u32_be(pb + 48, effective_dir_id);
                PPC_NO_ERR
            };
            PpcImportAction::Return(ppc_i16_result(ppc_complete_pb(memory, pb, result)))
        }
        PpcFileCompatibilityOperation::PbGetWdInfoSync => {
            let pb = cpu.gpr[3];
            let input_vref = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
            let wd_index = memory.read_u16_be(pb + 26).unwrap_or(0) as i16;
            let info = if wd_index > 0 {
                let target_volume = (input_vref != 0).then(|| {
                    working_directories
                        .get(&input_vref)
                        .map(|record| record.volume_ref_num)
                        .unwrap_or(input_vref)
                });
                let mut records = working_directories
                    .values()
                    .copied()
                    .filter(|record| {
                        target_volume.is_none_or(|volume| record.volume_ref_num == volume)
                    })
                    .collect::<Vec<_>>();
                records.sort_by_key(|record| record.ref_num);
                records.get(wd_index as usize - 1).copied()
            } else {
                ppc_working_directory_info(
                    input_vref,
                    *application_working_directory_ref_num,
                    default_dir_id,
                    working_directories,
                    vfs_volumes,
                )
            };
            let result = if let Some(record) = info {
                let returned_ref_num = if wd_index > 0 {
                    record.volume_ref_num
                } else {
                    record.ref_num
                };
                let _ = memory.write_u16_be(pb + 22, returned_ref_num as u16);
                let _ = memory.write_u32_be(pb + 28, record.proc_id);
                let _ = memory.write_u16_be(pb + 32, record.volume_ref_num as u16);
                let _ = memory.write_u32_be(pb + 48, record.dir_id);
                PPC_NO_ERR
            } else {
                PPC_RF_NUM_ERR
            };
            PpcImportAction::Return(ppc_i16_result(ppc_complete_pb(memory, pb, result)))
        }
        PpcFileCompatibilityOperation::PbCloseWdSync => {
            let pb = cpu.gpr[3];
            let wd_ref_num = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
            let is_volume_ref_num = wd_ref_num == PPC_BOOT_VOLUME_REF_NUM
                || vfs_volumes
                    .iter()
                    .any(|volume| volume.ref_num == wd_ref_num);
            let closed = working_directories.remove(&wd_ref_num);
            let result = if is_volume_ref_num || closed.is_some() {
                if *application_working_directory_ref_num == wd_ref_num {
                    *application_working_directory_ref_num = closed
                        .map(|record| record.volume_ref_num)
                        .unwrap_or(wd_ref_num);
                }
                PPC_NO_ERR
            } else {
                PPC_RF_NUM_ERR
            };
            PpcImportAction::Return(ppc_i16_result(ppc_complete_pb(memory, pb, result)))
        }
        PpcFileCompatibilityOperation::PbHSetVolSync => {
            let pb = cpu.gpr[3];
            let requested_vref = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
            let requested_dir_id = memory.read_u32_be(pb + 48).unwrap_or(0);
            let volume_ref_num = if requested_vref == 0 {
                ppc_working_directory_info(
                    0,
                    *application_working_directory_ref_num,
                    default_dir_id,
                    working_directories,
                    vfs_volumes,
                )
                .map(|record| record.volume_ref_num)
                .unwrap_or(PPC_BOOT_VOLUME_REF_NUM)
            } else if let Some(record) = working_directories.get(&requested_vref) {
                record.volume_ref_num
            } else if requested_vref == PPC_BOOT_VOLUME_REF_NUM
                || vfs_volumes
                    .iter()
                    .any(|volume| volume.ref_num == requested_vref)
            {
                requested_vref
            } else {
                let result = ppc_complete_pb(memory, pb, PPC_NSV_ERR);
                return PpcImportAction::Return(ppc_i16_result(result));
            };
            let effective_dir_id = if requested_dir_id <= 1 {
                working_directories
                    .get(&requested_vref)
                    .map(|record| record.dir_id)
                    .unwrap_or_else(|| {
                        ppc_resolve_directory_id(requested_vref, requested_dir_id, default_dir_id)
                    })
            } else {
                requested_dir_id
            };
            let result = if ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none() {
                PPC_FNF_ERR
            } else {
                let root_dir_id = if volume_ref_num == PPC_BOOT_VOLUME_REF_NUM {
                    PPC_ROOT_DIR_ID
                } else {
                    vfs_volumes
                        .iter()
                        .find(|volume| volume.ref_num == volume_ref_num)
                        .map(|volume| volume.root_dir_id)
                        .unwrap_or(PPC_ROOT_DIR_ID)
                };
                *application_working_directory_ref_num = if effective_dir_id == root_dir_id {
                    volume_ref_num
                } else if let Some(existing) = working_directories.values().find(|record| {
                    record.volume_ref_num == volume_ref_num
                        && record.dir_id == effective_dir_id
                        && record.proc_id == 0
                }) {
                    existing.ref_num
                } else {
                    let mut ref_num = *next_working_directory_ref_num;
                    while working_directories.contains_key(&ref_num) {
                        ref_num = ref_num.saturating_add(1);
                    }
                    *next_working_directory_ref_num = ref_num.saturating_add(1);
                    working_directories.insert(
                        ref_num,
                        ProcessWorkingDirectory {
                            ref_num,
                            volume_ref_num,
                            dir_id: effective_dir_id,
                            proc_id: 0,
                        },
                    );
                    ref_num
                };
                let _ = memory.write_u32_be(
                    crate::memory::globals::addr::CUR_DIR_STORE,
                    effective_dir_id,
                );
                PPC_NO_ERR
            };
            PpcImportAction::Return(ppc_i16_result(ppc_complete_pb(memory, pb, result)))
        }
        PpcFileCompatibilityOperation::PbHGetVolParmsSync => PpcImportAction::Return(
            ppc_i16_result(ppc_complete_pb(memory, cpu.gpr[3], PPC_NO_ERR)),
        ),
        PpcFileCompatibilityOperation::PbHOpenRfSync
        | PpcFileCompatibilityOperation::PbCatSearchSync
        | PpcFileCompatibilityOperation::PbDirCreateSync => PpcImportAction::Return(
            ppc_i16_result(ppc_complete_pb(memory, cpu.gpr[3], PPC_FNF_ERR)),
        ),
        PpcFileCompatibilityOperation::OpenDf
        | PpcFileCompatibilityOperation::OpenRf
        | PpcFileCompatibilityOperation::Create
        | PpcFileCompatibilityOperation::FsOpen => {
            PpcImportAction::Return(ppc_i16_result(PPC_FNF_ERR))
        }
    }
}

pub(super) fn ppc_working_directory_info(
    wd_ref_num: i16,
    application_wd_ref_num: i16,
    default_dir_id: u32,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    vfs_volumes: &[PpcVfsVolumeRecord],
) -> Option<ProcessWorkingDirectory> {
    let effective_ref_num = if wd_ref_num == 0 {
        application_wd_ref_num
    } else {
        wd_ref_num
    };
    if let Some(record) = working_directories.get(&effective_ref_num) {
        return Some(*record);
    }
    if effective_ref_num == PPC_BOOT_VOLUME_REF_NUM {
        return Some(ProcessWorkingDirectory {
            ref_num: effective_ref_num,
            volume_ref_num: PPC_BOOT_VOLUME_REF_NUM,
            dir_id: if effective_ref_num == application_wd_ref_num {
                default_dir_id
            } else {
                PPC_ROOT_DIR_ID
            },
            proc_id: 0,
        });
    }
    vfs_volumes
        .iter()
        .find(|volume| volume.ref_num == effective_ref_num)
        .map(|volume| ProcessWorkingDirectory {
            ref_num: effective_ref_num,
            volume_ref_num: volume.ref_num,
            dir_id: if effective_ref_num == application_wd_ref_num {
                default_dir_id
            } else {
                volume.root_dir_id
            },
            proc_id: 0,
        })
}

pub(super) fn ppc_volume_name_for_ref_num<'a>(
    volume_ref_num: i16,
    vfs_volumes: &'a [PpcVfsVolumeRecord],
) -> &'a [u8] {
    if volume_ref_num == PPC_BOOT_VOLUME_REF_NUM {
        crate::trap::TrapDispatcher::boot_volume_name().as_bytes()
    } else {
        vfs_volumes
            .iter()
            .find(|volume| volume.ref_num == volume_ref_num)
            .map(|volume| volume.name.as_bytes())
            .unwrap_or_else(|| crate::trap::TrapDispatcher::boot_volume_name().as_bytes())
    }
}

pub(super) fn ppc_hget_vol(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    default_dir_id: u32,
    application_wd_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    vfs_volumes: &[PpcVfsVolumeRecord],
) -> i16 {
    let name_ptr = cpu.gpr[3];
    let vref_ptr = cpu.gpr[4];
    let dir_id_ptr = cpu.gpr[5];
    let Some(working_directory) = ppc_working_directory_info(
        0,
        application_wd_ref_num,
        default_dir_id,
        working_directories,
        vfs_volumes,
    ) else {
        return PPC_RF_NUM_ERR;
    };
    let volume_name = ppc_volume_name_for_ref_num(working_directory.volume_ref_num, vfs_volumes);
    if !ppc_optional_pstring_output_can_write(memory, name_ptr, volume_name)
        || !ppc_optional_output_can_write(memory, vref_ptr, 2)
        || !ppc_optional_output_can_write(memory, dir_id_ptr, 4)
    {
        return PPC_PARAM_ERR;
    }
    if name_ptr != 0 {
        let _ = ppc_write_pstring_bytes(memory, name_ptr, volume_name);
    }
    if vref_ptr != 0 {
        let _ = memory.write_u16_be(vref_ptr, working_directory.ref_num as u16);
    }
    if dir_id_ptr != 0 {
        let _ = memory.write_u32_be(dir_id_ptr, working_directory.dir_id);
    }
    PPC_NO_ERR
}

pub(super) fn ppc_hset_vol(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    application_working_directory_ref_num: &mut i16,
) -> i16 {
    // Inside Macintosh: Files (1992), pp. 2-136--2-137. HSetVol accepts an
    // optional Pascal pathname plus a volume or working-directory reference
    // and directory ID, then changes the process-wide default directory.
    let name_ptr = cpu.gpr[3];
    let requested_vref = cpu.gpr[4] as u16 as i16;
    let requested_dir_id = cpu.gpr[5];
    let volume_ref_num = if requested_vref == 0 {
        ppc_working_directory_info(
            0,
            *application_working_directory_ref_num,
            default_dir_id,
            working_directories,
            vfs_volumes,
        )
        .map(|record| record.volume_ref_num)
        .unwrap_or(PPC_BOOT_VOLUME_REF_NUM)
    } else if let Some(record) = working_directories.get(&requested_vref) {
        record.volume_ref_num
    } else if requested_vref == PPC_BOOT_VOLUME_REF_NUM
        || vfs_volumes
            .iter()
            .any(|volume| volume.ref_num == requested_vref)
    {
        requested_vref
    } else {
        return PPC_NSV_ERR;
    };
    let name = if name_ptr == 0 {
        String::new()
    } else {
        let Some(bytes) = ppc_read_pstring_bytes(memory, name_ptr) else {
            return PPC_PARAM_ERR;
        };
        decode_mac_roman(&bytes)
    };

    let base_dir_id = if requested_dir_id <= 1 {
        working_directories
            .get(&requested_vref)
            .map(|record| record.dir_id)
            .unwrap_or_else(|| {
                ppc_resolve_directory_id(requested_vref, requested_dir_id, default_dir_id)
            })
    } else {
        requested_dir_id
    };
    let target_dir_id = if name.is_empty() {
        base_dir_id
    } else {
        let boot_volume = crate::trap::TrapDispatcher::boot_volume_name();
        let normalized = ppc_normalize_vfs_path(&name);
        if normalized.eq_ignore_ascii_case(boot_volume) {
            PPC_ROOT_DIR_ID
        } else {
            let relative = normalized
                .strip_prefix(boot_volume)
                .and_then(|suffix| suffix.strip_prefix('/'))
                .unwrap_or(&normalized);
            let Some(base_path) = ppc_directory_path_for_id(vfs_directories, base_dir_id) else {
                return PPC_FNF_ERR;
            };
            let joined = ppc_join_vfs_path(base_path, relative);
            ppc_directory_id_for_path(vfs_directories, &joined)
                .or_else(|| ppc_directory_id_for_path(vfs_directories, relative))
                .unwrap_or(0)
        }
    };
    if ppc_directory_path_for_id(vfs_directories, target_dir_id).is_none() {
        return PPC_FNF_ERR;
    }
    let root_dir_id = if volume_ref_num == PPC_BOOT_VOLUME_REF_NUM {
        PPC_ROOT_DIR_ID
    } else {
        vfs_volumes
            .iter()
            .find(|volume| volume.ref_num == volume_ref_num)
            .map(|volume| volume.root_dir_id)
            .unwrap_or(PPC_ROOT_DIR_ID)
    };
    *application_working_directory_ref_num = if target_dir_id == root_dir_id {
        volume_ref_num
    } else if let Some(existing) = working_directories.values().find(|record| {
        record.volume_ref_num == volume_ref_num
            && record.dir_id == target_dir_id
            && record.proc_id == 0
    }) {
        existing.ref_num
    } else {
        let mut ref_num = *next_working_directory_ref_num;
        while working_directories.contains_key(&ref_num) {
            ref_num = ref_num.saturating_add(1);
        }
        *next_working_directory_ref_num = ref_num.saturating_add(1);
        working_directories.insert(
            ref_num,
            ProcessWorkingDirectory {
                ref_num,
                volume_ref_num,
                dir_id: target_dir_id,
                proc_id: 0,
            },
        );
        ref_num
    };
    let _ = memory.write_u32_be(crate::memory::globals::addr::CUR_DIR_STORE, target_dir_id);
    PPC_NO_ERR
}

pub(super) fn ppc_get_vol(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    default_dir_id: u32,
    application_wd_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    vfs_volumes: &[PpcVfsVolumeRecord],
) -> i16 {
    let name_ptr = cpu.gpr[3];
    let vref_ptr = cpu.gpr[4];
    let Some(working_directory) = ppc_working_directory_info(
        0,
        application_wd_ref_num,
        default_dir_id,
        working_directories,
        vfs_volumes,
    ) else {
        return PPC_RF_NUM_ERR;
    };
    let volume_name = ppc_volume_name_for_ref_num(working_directory.volume_ref_num, vfs_volumes);
    if !ppc_optional_pstring_output_can_write(memory, name_ptr, volume_name)
        || !ppc_optional_output_can_write(memory, vref_ptr, 2)
    {
        return PPC_PARAM_ERR;
    }
    if name_ptr != 0 {
        let _ = ppc_write_pstring_bytes(memory, name_ptr, volume_name);
    }
    if vref_ptr != 0 {
        let _ = memory.write_u16_be(vref_ptr, working_directory.ref_num as u16);
    }
    PPC_NO_ERR
}

pub(super) fn ppc_get_wd_info(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    default_dir_id: u32,
    application_wd_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    vfs_volumes: &[PpcVfsVolumeRecord],
) -> i16 {
    let wd_ref_num = cpu.gpr[3] as u16 as i16;
    let vref_ptr = cpu.gpr[4];
    let dir_id_ptr = cpu.gpr[5];
    let proc_id_ptr = cpu.gpr[6];
    if !ppc_memory_can_write_bytes(memory, vref_ptr, 2)
        || !ppc_memory_can_write_bytes(memory, dir_id_ptr, 4)
        || !ppc_memory_can_write_bytes(memory, proc_id_ptr, 4)
    {
        return PPC_PARAM_ERR;
    }
    // Inside Macintosh: Files 1992, 2-182: GetWDInfo converts a process
    // working-directory reference into its volume, directory, and user ID.
    let Some(working_directory) = ppc_working_directory_info(
        wd_ref_num,
        application_wd_ref_num,
        default_dir_id,
        working_directories,
        vfs_volumes,
    ) else {
        return PPC_RF_NUM_ERR;
    };
    let _ = memory.write_u16_be(vref_ptr, working_directory.volume_ref_num as u16);
    let _ = memory.write_u32_be(dir_id_ptr, working_directory.dir_id);
    let _ = memory.write_u32_be(proc_id_ptr, working_directory.proc_id);
    PPC_NO_ERR
}

pub(super) fn ppc_flush_vol(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let name_ptr = cpu.gpr[3];
    if name_ptr != 0 && ppc_read_pstring_bytes(memory, name_ptr).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}
