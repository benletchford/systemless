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
    pub(super) desktop_databases:
        &'a mut HashMap<i16, crate::systems::macintosh::desktop_manager::DesktopDatabase>,
    pub(super) desktop_file_metadata:
        &'a HashMap<String, crate::systems::macintosh::process_context::ProcessVfsMetadata>,
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
        desktop_databases,
        desktop_file_metadata,
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
        PpcImportDispatcherTarget::FSClose => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_fs_close(
                cpu,
                files,
                writable_refnums,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
            ))))
        }
        PpcImportDispatcherTarget::PBClose => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_close(
                cpu,
                memory,
                files,
                writable_refnums,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
            ))))
        }
        PpcImportDispatcherTarget::PBFlushFile => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_flush_file(cpu, memory, files),
        ))),
        PpcImportDispatcherTarget::FSRead => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fs_read(cpu, memory, files, vfs_files),
        ))),
        PpcImportDispatcherTarget::PBRead => {
            let pb = cpu.gpr[3];
            let result = ppc_pb_read(cpu, memory, files, vfs_files);
            if std::env::var_os("SYSTEMLESS_PPC_FILE_TRACE").is_some() {
                eprintln!(
                    "[PPC-FILE-TRACE] PBRead result={} completion={:?} ioResult={:?} ref={:?} buffer={:?} request={:?} actual={:?} mode={:?} offset={:?}",
                    result,
                    memory.read_u32_be(pb + 12),
                    memory.read_u16_be(pb + 16),
                    memory.read_u16_be(pb + 24),
                    memory.read_u32_be(pb + 32),
                    memory.read_u32_be(pb + 36),
                    memory.read_u32_be(pb + 40),
                    memory.read_u16_be(pb + 44),
                    memory.read_u32_be(pb + 46),
                );
            }
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
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
            ppc_h_create(
                cpu, memory, vfs_directories, vfs_files, default_dir_id,
                working_directories,
            ),
        ))),
        PpcImportDispatcherTarget::HRename => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_h_rename(
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
            ))))
        }
        PpcImportDispatcherTarget::Rename => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_rename(
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
            ))))
        }
        PpcImportDispatcherTarget::FSpRename => {
            // Files.h: FSpRename(const FSSpec *, ConstStr255Param newName) is
            // HRename with the spec's vRefNum (+0), parID (+2), and name (+6).
            let spec = cpu.gpr[3];
            let mut call = cpu.clone();
            call.gpr[3] = memory.read_u16_be(spec).unwrap_or(0) as i16 as i32 as u32;
            call.gpr[4] = memory.read_u32_be(spec + 2).unwrap_or(0);
            call.gpr[5] = spec + 6;
            call.gpr[6] = cpu.gpr[4];
            Some(PpcImportAction::Return(ppc_i16_result(ppc_h_rename(
                &call,
                memory,
                vfs_directories,
                vfs_files,
                deleted_vfs_file_paths,
                files,
                vfs_resource_files,
                resource_files,
                vfs_resources,
                default_dir_id,
            ))))
        }
        PpcImportDispatcherTarget::PBHRenameSync | PpcImportDispatcherTarget::PBRenameSync => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_rename_sync(
                matches!(binding.dispatcher_target, PpcImportDispatcherTarget::PBHRenameSync),
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
            ))))
        }
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
        PpcImportDispatcherTarget::FSpOpenRF => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_fsp_open_rf(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
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
        PpcImportDispatcherTarget::PBHOpenDF | PpcImportDispatcherTarget::PBHOpenDeny => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pbh_open_df(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                files,
                writable_refnums,
                next_file_ref_num,
                matches!(binding.dispatcher_target, PpcImportDispatcherTarget::PBHOpenDeny),
            ))))
        }
        PpcImportDispatcherTarget::CurResFile => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_guest_resource_refnum(memory, *current_resource_refnum),
        ))),
        PpcImportDispatcherTarget::UseResFile => {
            let refnum = ppc_internal_resource_refnum(memory, cpu.gpr[3] as u16 as i16);
            ppc_set_current_resource_refnum(
                memory,
                current_resource_refnum,
                refnum,
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
        PpcImportDispatcherTarget::OpenRFPerm => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_open_rf_perm(
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
                *application_working_directory_ref_num,
                working_directories,
                vfs_volumes,
            ))))
        }
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
        PpcImportDispatcherTarget::OpenWD => {
            let out = cpu.gpr[6];
            let result = if out == 0 || !ppc_memory_can_write_bytes(memory, out, 2) {
                PPC_PARAM_ERR
            } else {
                match ppc_open_working_directory(
                    cpu.gpr[3] as u16 as i16,
                    cpu.gpr[4],
                    cpu.gpr[5],
                    vfs_directories,
                    vfs_volumes,
                    default_dir_id,
                    working_directories,
                    next_working_directory_ref_num,
                ) {
                    Ok((wd_ref_num, _, _)) => {
                        let _ = memory.write_u16_be(out, wd_ref_num as u16);
                        PPC_NO_ERR
                    }
                    Err(error) => error,
                }
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::SetVol => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_set_vol(
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
                memory,
                vfs_volumes,
                working_directories,
                application_working_directory_ref_num,
            ),
        ))),
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
        PpcImportDispatcherTarget::PBHGetVInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pbh_get_v_info(
                cpu,
                memory,
                vfs_volumes,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
                working_directories,
            ))))
        }
        PpcImportDispatcherTarget::PBGetVolumeInfo => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_pb_get_volume_info(cpu, memory, vfs_volumes, working_directories),
            )))
        }
        PpcImportDispatcherTarget::FSGetVolumeInfo => {
            let volume_ref = cpu.gpr[3] as u16 as i16;
            let (index, actual, info) = (cpu.gpr[4], cpu.gpr[5], cpu.gpr[7]);
            let (name, root) = (cpu.gpr[8], cpu.gpr[9]);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_fs_get_volume_info(
                    memory,
                    vfs_volumes,
                    working_directories,
                    volume_ref,
                    index,
                    actual,
                    info,
                    name,
                    root,
                ),
            )))
        }
        PpcImportDispatcherTarget::GetVInfo => {
            // Inside Macintosh: Files (1992), p. 2-67: drive 0 selects the
            // default volume; the result includes its name, vRefNum, and
            // available bytes.
            let drive = cpu.gpr[3] as u16 as i16;
            let name_ptr = cpu.gpr[4];
            let ref_ptr = cpu.gpr[5];
            let free_ptr = cpu.gpr[6];
            let volume = if matches!(drive, 0 | 1 | PPC_BOOT_VOLUME_REF_NUM) {
                Some((
                    crate::trap::TrapDispatcher::boot_volume_name().to_string(),
                    PPC_BOOT_VOLUME_REF_NUM,
                    0x8000u32 * 4096,
                ))
            } else if drive > 1 {
                vfs_volumes.get((drive - 2) as usize).map(|volume| {
                    (
                        volume.name.clone(),
                        volume.ref_num,
                        u32::from(volume.free_blocks)
                            .saturating_mul(volume.allocation_block_size),
                    )
                })
            } else {
                vfs_volumes
                    .iter()
                    .find(|volume| volume.ref_num == drive)
                    .map(|volume| {
                        (
                            volume.name.clone(),
                            volume.ref_num,
                            u32::from(volume.free_blocks)
                                .saturating_mul(volume.allocation_block_size),
                        )
                    })
            };
            let result = if let Some((name, ref_num, free_bytes)) = volume {
                let name_bytes = encode_mac_roman_lossy(&name);
                if ref_ptr == 0
                    || free_ptr == 0
                    || !ppc_memory_can_write_bytes(memory, ref_ptr, 2)
                    || !ppc_memory_can_write_bytes(memory, free_ptr, 4)
                    || (name_ptr != 0
                        && !ppc_optional_pstring_output_can_write(memory, name_ptr, &name_bytes))
                {
                    PPC_PARAM_ERR
                } else {
                    if name_ptr != 0 {
                        let _ = ppc_write_pstring_bytes(memory, name_ptr, &name_bytes);
                    }
                    let _ = memory.write_u16_be(ref_ptr, ref_num as u16);
                    let _ = memory.write_u32_be(free_ptr, free_bytes);
                    PPC_NO_ERR
                }
            } else {
                PPC_NSV_ERR
            };
            if ppc_hle_trace_enabled() {
                eprintln!("[PPC-TRACE] GetVInfo drive={drive} -> {result}");
            }
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::GetVRefNum => {
            // Inside Macintosh: Files (1992), p. 2-138: GetVRefNum maps an
            // open file reference to its volume and returns rfNumErr for an
            // unknown reference. Open data and resource forks share the VFS
            // boot volume.
            let ref_num = cpu.gpr[3] as u16 as i16;
            let out = cpu.gpr[4];
            let result = if out == 0 || !ppc_memory_can_write_bytes(memory, out, 2) {
                PPC_PARAM_ERR
            } else if ref_num != 0
                && (files.iter().any(|file| file.ref_num == ref_num)
                    || resource_files.iter().any(|file| file.ref_num == ref_num))
            {
                let _ = memory.write_u16_be(out, PPC_BOOT_VOLUME_REF_NUM as u16);
                PPC_NO_ERR
            } else {
                PPC_RF_NUM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::PBDTGetPath => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_dt_get_path(cpu, memory, vfs_volumes, desktop_databases, false),
        ))),
        PpcImportDispatcherTarget::PBDTAddIconSync => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_dt_icon(
                cpu,
                memory,
                vfs_volumes,
                desktop_databases,
                true,
            ))))
        }
        PpcImportDispatcherTarget::PBDTGetIconSync => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_dt_icon(
                cpu,
                memory,
                vfs_volumes,
                desktop_databases,
                false,
            ))))
        }
        PpcImportDispatcherTarget::PBDTGetIconInfoSync => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_pb_dt_icon_info(cpu, memory, vfs_volumes, desktop_databases),
            )))
        }
        PpcImportDispatcherTarget::PBDTAddAPPLSync => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_dt_add_appl(
                cpu,
                memory,
                vfs_volumes,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                desktop_file_metadata,
                desktop_databases,
            ))))
        }
        PpcImportDispatcherTarget::PBDTGetAPPL => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_pb_dt_get_appl(
                cpu,
                memory,
                vfs_volumes,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                desktop_file_metadata,
                desktop_databases,
            ))))
        }
        PpcImportDispatcherTarget::PBDTGetCommentSync => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_pb_dt_get_comment(cpu, memory, vfs_volumes)),
        )),
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
        PpcImportDispatcherTarget::GetFInfoCString => {
            // The lowercase C glue converts the caller's name to a Pascal
            // string in place, makes the call, and converts it back.
            ppc_c2pstr(cpu, memory);
            let result = ppc_get_finfo(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
            );
            ppc_p2cstr(cpu, memory);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::SetFInfo { c_string } => {
            // Files.h: SetFInfo(fileName, vRefNum, const FInfo *) is
            // HSetFInfo in the default directory of vRefNum.
            let mut call = cpu.clone();
            call.gpr[3] = cpu.gpr[4];
            call.gpr[4] = 0;
            call.gpr[5] = cpu.gpr[3];
            call.gpr[6] = cpu.gpr[5];
            if c_string {
                ppc_c2pstr(cpu, memory);
            }
            let result = ppc_h_set_finfo(
                &call,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
            );
            if c_string {
                ppc_p2cstr(cpu, memory);
            }
            Some(PpcImportAction::Return(ppc_i16_result(result)))
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
        PpcImportDispatcherTarget::PBResolveFileIDRef => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_pb_resolve_file_id_ref(
                cpu,
                memory,
                vfs_volumes,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                working_directories,
            ),
        ))),
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
        PpcImportDispatcherTarget::ResolveAliasFile
        | PpcImportDispatcherTarget::ResolveAliasFileWithMountFlags => {
            let mount_flags = if matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::ResolveAliasFileWithMountFlags
            ) {
                cpu.gpr[7]
            } else {
                0
            };
            Some(PpcImportAction::Return(ppc_i16_result(ppc_resolve_alias_file(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
                mount_flags,
            ))))
        }
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
        PpcImportDispatcherTarget::FileManager(operation) => {
            Some(dispatch_file_manager::ppc_dispatch_file_manager(
                operation,
                dispatch_file_manager::PpcFileManagerContext {
                    cpu,
                    memory,
                    files,
                    desktop_databases,
                    writable_refnums,
                    vfs_files,
                    vfs_directories,
                    deleted_vfs_file_paths,
                    vfs_resource_files,
                    resource_files,
                    vfs_resources,
                    next_file_ref_num,
                    last_resource_error,
                    default_dir_id,
                    vfs_volumes,
                    working_directories,
                    application_working_directory_ref_num,
                },
            ))
        }
        PpcImportDispatcherTarget::FsRef(operation) => Some(PpcImportAction::Return(
            ppc_i16_result(dispatch_fsref::ppc_dispatch_fsref(
                operation,
                &mut dispatch_fsref::PpcFsRefContext {
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
                    default_dir_id,
                    vfs_volumes,
                },
            )),
        )),
        PpcImportDispatcherTarget::ResolveAlias => {
            Some(PpcImportAction::Return(ppc_i16_result(ppc_resolve_alias(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                handles,
                aliases,
            ))))
        }
        PpcImportDispatcherTarget::FollowFinderAlias => {
            // Aliases.h: FollowFinderAlias(fromFile, alias, logon, target,
            // wasChanged) is ResolveAlias with a mount-permission flag, and
            // the runtime never mounts volumes on demand.
            let mut call = cpu.clone();
            call.gpr[5] = cpu.gpr[6];
            call.gpr[6] = cpu.gpr[7];
            Some(PpcImportAction::Return(ppc_i16_result(ppc_resolve_alias(
                &mut call,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                handles,
                aliases,
            ))))
        }
        PpcImportDispatcherTarget::MatchAlias => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_match_alias(cpu, memory, vfs_directories, handles, aliases),
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
        PpcImportDispatcherTarget::NewAliasMinimal => {
            let mut call = cpu.clone();
            call.gpr[3] = 0;
            call.gpr[4] = cpu.gpr[3];
            call.gpr[5] = cpu.gpr[4];
            let result = ppc_new_alias(
                &mut call,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                aliases,
            );
            if result != PPC_NO_ERR && cpu.gpr[4] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[4], 0);
            }
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::NewAliasMinimalFromFullPath => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_new_alias_minimal_from_full_path(
                    cpu,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                ),
            )))
        }
        _ => None,
    }
}

/// MacErrors.h: missing comment or APPL entry in the desktop database.
const PPC_AFP_ITEM_NOT_FOUND: i16 = -5012;

/// PBDTAddIcon and PBDTGetIcon use the same packed DTPBRec fields.
fn ppc_pb_dt_icon(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    volumes: &[PpcVfsVolumeRecord],
    databases: &mut HashMap<i16, crate::systems::macintosh::desktop_manager::DesktopDatabase>,
    adding: bool,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 60) {
        return PPC_PARAM_ERR;
    }
    let Some(volume) = ppc_dt_volume_ref(memory.read_u16_be(pb + 24).unwrap_or(0), volumes) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    if adding {
        if let Some(attributes) = volumes
            .iter()
            .find(|candidate| candidate.ref_num == volume)
            .map(|candidate| candidate.attributes)
        {
            // Files.h: ioVAtrb bits 7 and 15 report hardware and software locks.
            if attributes & 0x0080 != 0 {
                return ppc_complete_pb(memory, pb, -44); // wPrErr
            }
            if attributes & 0x8000 != 0 {
                return ppc_complete_pb(memory, pb, -46); // vLckdErr
            }
        }
    }
    let buffer = memory.read_u32_be(pb + 32).unwrap_or(0);
    let length = memory.read_u32_be(pb + 36).unwrap_or(u32::MAX);
    let creator = memory.read_u32_be(pb + 52).unwrap_or(0);
    let file_type = memory.read_u32_be(pb + 56).unwrap_or(0);
    let icon_type = memory.read_u8(pb + 45).unwrap_or(0);
    if length > i32::MAX as u32 || memory.read_u32_be(pb + 28) != Some(0) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    if adding {
        if length != 0 && !ppc_memory_can_read_bytes(memory, buffer, length) {
            return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
        }
        let mut data = Vec::new();
        if data.try_reserve_exact(length as usize).is_err() {
            return ppc_complete_pb(memory, pb, PPC_MEM_FULL_ERR);
        }
        data.resize(length as usize, 0);
        if length != 0 && memory.read_bytes_into(buffer, &mut data).is_none() {
            for (offset, byte) in data.iter_mut().enumerate() {
                let Some(address) = buffer.checked_add(offset as u32) else {
                    return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
                };
                let Some(value) = memory.read_u8(address) else {
                    return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
                };
                *byte = value;
            }
        }
        let icon = crate::systems::macintosh::desktop_manager::DesktopIcon {
            creator,
            file_type,
            icon_type,
            data,
        };
        let result = databases.entry(volume).or_default().add_icon(icon);
        ppc_complete_pb(memory, pb, result.err().unwrap_or(PPC_NO_ERR))
    } else {
        let Some(icon) = databases
            .get(&volume)
            .and_then(|database| database.icon(creator, file_type, icon_type))
        else {
            return ppc_complete_pb(memory, pb, -5012); // afpItemNotFound
        };
        let count = icon.data.len().min(length as usize);
        if count != 0 && memory.write_bytes(buffer, &icon.data[..count]).is_none() {
            return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
        }
        let _ = memory.write_u32_be(pb + 40, icon.data.len() as u32);
        ppc_complete_pb(memory, pb, PPC_NO_ERR)
    }
}

fn ppc_dt_volume_ref(desktop_ref: u16, volumes: &[PpcVfsVolumeRecord]) -> Option<i16> {
    match desktop_ref.checked_sub(0x7f00)? {
        0 => Some(PPC_BOOT_VOLUME_REF_NUM),
        index => volumes
            .get(usize::from(index - 1))
            .map(|volume| volume.ref_num),
    }
}

fn ppc_pb_dt_icon_info(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    volumes: &[PpcVfsVolumeRecord],
    databases: &HashMap<i16, crate::systems::macintosh::desktop_manager::DesktopDatabase>,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 60) {
        return PPC_PARAM_ERR;
    }
    let Some(volume) = ppc_dt_volume_ref(memory.read_u16_be(pb + 24).unwrap_or(0), volumes) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    if memory.read_u32_be(pb + 28) != Some(0) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    let index = memory.read_u16_be(pb + 26).unwrap_or(0) as i16;
    let creator = memory.read_u32_be(pb + 52).unwrap_or(0);
    let Some(icon) = usize::try_from(index).ok().and_then(|index| {
        databases
            .get(&volume)
            .and_then(|database| database.icon_info(creator, index))
    }) else {
        return ppc_complete_pb(memory, pb, -5012); // afpItemNotFound
    };
    let _ = memory.write_u32_be(pb + 40, icon.data.len() as u32);
    let _ = memory.write_u8(pb + 45, icon.icon_type);
    let _ = memory.write_u32_be(pb + 56, icon.file_type);
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

fn ppc_dt_path_volume(
    path: &str,
    volumes: &[PpcVfsVolumeRecord],
    directories: &[PpcVfsDirectory],
) -> i16 {
    let path = path.to_ascii_lowercase();
    volumes
        .iter()
        .filter_map(|volume| {
            let root = ppc_directory_path_for_id(directories, volume.root_dir_id)?;
            let root = root.to_ascii_lowercase();
            (!root.is_empty() && path.starts_with(&(root.clone() + "/")))
                .then_some((root.len(), volume.ref_num))
        })
        .max_by_key(|(length, _)| *length)
        .map_or(PPC_BOOT_VOLUME_REF_NUM, |(_, volume)| volume)
}

#[allow(clippy::too_many_arguments)]
fn ppc_pb_dt_add_appl(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    volumes: &[PpcVfsVolumeRecord],
    directories: &[PpcVfsDirectory],
    files: &[PpcVfsFileRecord],
    resource_files: &[PpcVfsResourceFileRecord],
    metadata: &HashMap<String, crate::systems::macintosh::process_context::ProcessVfsMetadata>,
    databases: &mut HashMap<i16, crate::systems::macintosh::desktop_manager::DesktopDatabase>,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 56) {
        return PPC_PARAM_ERR;
    }
    let Some(volume) = ppc_dt_volume_ref(memory.read_u16_be(pb + 24).unwrap_or(0), volumes) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    if let Some(attributes) = volumes
        .iter()
        .find(|entry| entry.ref_num == volume)
        .map(|entry| entry.attributes)
    {
        if attributes & 0x0080 != 0 {
            return ppc_complete_pb(memory, pb, -44);
        }
        if attributes & 0x8000 != 0 {
            return ppc_complete_pb(memory, pb, -46);
        }
    }
    if memory.read_u32_be(pb + 28) != Some(0) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    let name_ptr = memory.read_u32_be(pb + 18).unwrap_or(0);
    let Some(name) = ppc_read_pstring(memory, name_ptr).filter(|name| !name.is_empty()) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let parent = memory.read_u32_be(pb + 48).unwrap_or(0);
    let Some(parent_path) = ppc_directory_path_for_id(directories, parent) else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    let path = ppc_join_vfs_path(parent_path, &name);
    let file_is_present = files.iter().any(|file| {
        file.path.eq_ignore_ascii_case(&path) && file.file_type == u32::from_be_bytes(*b"APPL")
    }) || resource_files.iter().any(|file| {
        file.path.eq_ignore_ascii_case(&path) && file.file_type == u32::from_be_bytes(*b"APPL")
    });
    let actual_volume = ppc_dt_path_volume(&path, volumes, directories);
    if !file_is_present || actual_volume != volume {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    }
    let Some(date) = metadata
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(&path))
        .map(|(_, entry)| entry.created_date)
    else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    databases.entry(volume).or_default().add_application(
        crate::systems::macintosh::desktop_manager::DesktopApplication {
            creator: memory.read_u32_be(pb + 52).unwrap_or(0),
            name,
            parent_dir_id: parent,
            creation_date: date,
        },
    );
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

/// PBDTGetAPPL: find the ioIndex-th application whose creator matches
/// ioFileCreator (+52), returning its name through ioNamePtr (+18) and its
/// parent directory in ioAPPLParID (+100). An ioIndex of 0 asks for the
/// most suitable copy, which is the first.
fn ppc_pb_dt_get_appl(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    volumes: &[PpcVfsVolumeRecord],
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    metadata: &HashMap<String, crate::systems::macintosh::process_context::ProcessVfsMetadata>,
    databases: &HashMap<i16, crate::systems::macintosh::desktop_manager::DesktopDatabase>,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 104) {
        return PPC_PARAM_ERR;
    }
    let name_ptr = memory.read_u32_be(pb + 18).unwrap_or(0);
    let index = memory.read_u16_be(pb + 26).unwrap_or(0) as i16;
    let creator = memory.read_u32_be(pb + 52).unwrap_or(0);
    let Some(volume) = ppc_dt_volume_ref(memory.read_u16_be(pb + 24).unwrap_or(0), volumes) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    let appl = u32::from_be_bytes(*b"APPL");
    let mut paths: Vec<&str> = vfs_files
        .iter()
        .filter(|file| file.file_type == appl && file.creator == creator)
        .map(|file| file.path.as_str())
        .chain(
            vfs_resource_files
                .iter()
                .filter(|file| file.file_type == appl && file.creator == creator)
                .map(|file| file.path.as_str()),
        )
        .filter(|path| ppc_dt_path_volume(path, volumes, vfs_directories) == volume)
        .collect();
    paths.sort_unstable();
    paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let mut combined = databases.get(&volume).cloned().unwrap_or_default();
    let mut implicit = crate::systems::macintosh::desktop_manager::DesktopDatabase::default();
    for path in paths {
        let creation_date = metadata
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(path))
            .map_or(0, |(_, entry)| entry.created_date);
        implicit.add_application(
            crate::systems::macintosh::desktop_manager::DesktopApplication {
                creator,
                name: path.rsplit('/').next().unwrap_or(path).to_owned(),
                parent_dir_id: ppc_parent_dir_id_for_path(vfs_directories, path),
                creation_date,
            },
        );
    }
    combined.merge_missing(implicit);
    let Some(entry) = usize::try_from(index).ok().and_then(|index| {
        if index == 0 {
            databases
                .get(&volume)
                .and_then(|database| database.application(creator, 0))
                .or_else(|| combined.application(creator, 0))
        } else {
            combined.application(creator, index)
        }
    }) else {
        return ppc_complete_pb(memory, pb, PPC_AFP_ITEM_NOT_FOUND);
    };
    let name = encode_mac_roman_lossy(&entry.name);
    if name_ptr != 0 && !ppc_write_pstring_bytes(memory, name_ptr, &name) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    let _ = memory.write_u32_be(pb + 28, entry.creation_date);
    let _ = memory.write_u32_be(pb + 100, entry.parent_dir_id);
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_dt_get_path(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_volumes: &[PpcVfsVolumeRecord],
    databases: &mut HashMap<i16, crate::systems::macintosh::desktop_manager::DesktopDatabase>,
    inform: bool,
) -> i16 {
    // DTPBRec uses the classic parameter-block header: ioNamePtr at +18,
    // ioVRefNum at +22, and ioDTRefNum at +24. More Macintosh Toolbox
    // (1993), pp. 9-6–9-9.
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, if inform { 32 } else { 26 }) {
        return PPC_PARAM_ERR;
    }
    let name_ptr = memory.read_u32_be(pb + 18).unwrap_or(0);
    let vref = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
    let requested_name = if name_ptr == 0 {
        None
    } else {
        let Some(name) = ppc_read_pstring(memory, name_ptr) else {
            return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
        };
        Some(name)
    };
    let boot_name = crate::trap::TrapDispatcher::boot_volume_name();
    let volume_index = if let Some(name) = requested_name.as_deref().filter(|name| !name.is_empty())
    {
        let volume_name = name.split(':').next().unwrap_or(name);
        if volume_name.eq_ignore_ascii_case(boot_name) {
            Some(0usize)
        } else {
            vfs_volumes
                .iter()
                .position(|volume| volume.name.eq_ignore_ascii_case(volume_name))
                .map(|index| index + 1)
        }
    } else if matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        Some(0)
    } else {
        vfs_volumes
            .iter()
            .position(|volume| volume.ref_num == vref)
            .map(|index| index + 1)
    };
    let Some(volume_index) = volume_index else {
        let _ = memory.write_u16_be(pb + 24, 0);
        return ppc_complete_pb(memory, pb, PPC_NSV_ERR);
    };
    let Some(desktop_ref) = u16::try_from(volume_index)
        .ok()
        .and_then(|index| 0x7f00u16.checked_add(index))
    else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(volume) = ppc_dt_volume_ref(desktop_ref, vfs_volumes) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let existed = databases.contains_key(&volume);
    databases.entry(volume).or_default();
    let _ = memory.write_u16_be(pb + 24, desktop_ref);
    if inform {
        let _ = memory.write_u32_be(pb + 28, u32::from(existed));
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

fn ppc_pb_dt_get_comment(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_volumes: &[PpcVfsVolumeRecord],
) -> i16 {
    // A newly created desktop database has no user comments. More Macintosh
    // Toolbox (1993), pp. 9-15–9-16, reports afpItemNotFound for absent data.
    const AFP_ITEM_NOT_FOUND: i16 = -5012;
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 44) {
        return PPC_PARAM_ERR;
    }
    let desktop_ref = memory.read_u16_be(pb + 24).unwrap_or(0);
    let valid_ref = desktop_ref >= 0x7f00 && usize::from(desktop_ref - 0x7f00) <= vfs_volumes.len();
    let _ = memory.write_u32_be(pb + 40, 0);
    ppc_complete_pb(
        memory,
        pb,
        if valid_ref {
            AFP_ITEM_NOT_FOUND
        } else {
            PPC_RF_NUM_ERR
        },
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcFileCompatibilityOperation {
    Create,
    FsOpen,
    OpenDf,
    OpenRf,
    PbCatSearchSync,
    PbCloseWdSync,
    PbDirCreateSync,
    PbGetFPosSync,
    PbGetWdInfoSync,
    PbHGetVolParmsSync,
    PbHGetVolSync,
    PbHSetVolSync,
    PbSetVolSync,
    PbOpenWdSync,
}

/// OpenWD (File Manager working-directory call)
/// Creates or reuses a working directory for the volume, directory and procID.
/// FUNCTION OpenWD (vRefNum: Integer; dirID: LongInt;
/// procID: LongInt; VAR wdRefNum: Integer): OSErr;
/// Inside Macintosh: Files (1992), pp. 2-180--2-181.
fn ppc_open_working_directory(
    requested_vref: i16,
    requested_dir_id: u32,
    proc_id: u32,
    vfs_directories: &[PpcVfsDirectory],
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
) -> Result<(i16, i16, u32), i16> {
    let volume_ref_num = if requested_vref == 0 {
        PPC_BOOT_VOLUME_REF_NUM
    } else if let Some(record) = working_directories.get(&requested_vref) {
        record.volume_ref_num
    } else if requested_vref == PPC_BOOT_VOLUME_REF_NUM
        || vfs_volumes
            .iter()
            .any(|volume| volume.ref_num == requested_vref)
    {
        requested_vref
    } else {
        return Err(PPC_NSV_ERR);
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
    if ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none() {
        return Err(PPC_FNF_ERR);
    }
    let root_dir_id = if volume_ref_num == PPC_BOOT_VOLUME_REF_NUM {
        PPC_ROOT_DIR_ID
    } else {
        vfs_volumes
            .iter()
            .find(|volume| volume.ref_num == volume_ref_num)
            .map(|volume| volume.root_dir_id)
            .ok_or(PPC_NSV_ERR)?
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
        let ref_num = (*next_working_directory_ref_num..=i16::MAX)
            .find(|candidate| !working_directories.contains_key(candidate))
            .ok_or(-121i16)?; // tmwdoErr
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
    Ok((wd_ref_num, volume_ref_num, effective_dir_id))
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
        PpcFileCompatibilityOperation::PbSetVolSync => {
            let pb = cpu.gpr[3];
            if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 24) {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            }
            let name = memory.read_u32_be(pb + 18).unwrap_or(0);
            let vref = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
            let result = ppc_set_vol(
                name,
                vref,
                memory,
                vfs_volumes,
                working_directories,
                application_working_directory_ref_num,
            );
            PpcImportAction::Return(ppc_i16_result(ppc_complete_pb(memory, pb, result)))
        }
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
            let result = match ppc_open_working_directory(
                requested_vref,
                requested_dir_id,
                proc_id,
                vfs_directories,
                vfs_volumes,
                default_dir_id,
                working_directories,
                next_working_directory_ref_num,
            ) {
                Ok((wd_ref_num, volume_ref_num, effective_dir_id)) => {
                    let _ = memory.write_u16_be(pb + 22, wd_ref_num as u16);
                    let _ = memory.write_u16_be(pb + 32, volume_ref_num as u16);
                    let _ = memory.write_u32_be(pb + 48, effective_dir_id);
                    PPC_NO_ERR
                }
                Err(error) => error,
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
            let result = dispatch_file_manager::ppc_close_working_directory(
                wd_ref_num,
                vfs_volumes,
                working_directories,
                application_working_directory_ref_num,
            );
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
        PpcFileCompatibilityOperation::PbCatSearchSync
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

/// SetVol (File Manager default-volume call)
/// Selects a volume root or an opened working directory as the process default.
/// FUNCTION SetVol (volName: StringPtr; vRefNum: Integer): OSErr;
/// Inside Macintosh: Files (1992), p. 2-135.
fn ppc_set_vol(
    name_ptr: u32,
    requested_vref: i16,
    memory: &mut PpcSectionMem,
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    application_working_directory_ref_num: &mut i16,
) -> i16 {
    let (target_ref_num, target_dir_id) = if let Some(record) = working_directories.get(&requested_vref)
    {
        (record.ref_num, record.dir_id)
    } else if requested_vref == PPC_BOOT_VOLUME_REF_NUM {
        (requested_vref, PPC_ROOT_DIR_ID)
    } else if let Some(volume) = vfs_volumes
        .iter()
        .find(|volume| volume.ref_num == requested_vref)
    {
        (volume.ref_num, volume.root_dir_id)
    } else if requested_vref != 0 {
        return PPC_NSV_ERR;
    } else {
        if name_ptr == 0 {
            return PPC_PARAM_ERR;
        }
        let Some(name) = ppc_read_pstring_bytes(memory, name_ptr) else {
            return PPC_BD_NAM_ERR;
        };
        let name = decode_mac_roman(&name);
        let name = name.trim_end_matches(':');
        if name.eq_ignore_ascii_case(crate::trap::TrapDispatcher::boot_volume_name()) {
            (PPC_BOOT_VOLUME_REF_NUM, PPC_ROOT_DIR_ID)
        } else if let Some(volume) = vfs_volumes
            .iter()
            .find(|volume| volume.name.eq_ignore_ascii_case(name))
        {
            (volume.ref_num, volume.root_dir_id)
        } else {
            return PPC_NSV_ERR;
        }
    };
    *application_working_directory_ref_num = target_ref_num;
    let _ = memory.write_u32_be(crate::memory::globals::addr::CUR_DIR_STORE, target_dir_id);
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
