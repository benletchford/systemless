//! File Manager calls that open resource forks by name, move, exchange and
//! copy catalog entries, and the remaining volume and desktop-database
//! parameter-block calls.

use super::*;

/// badMovErr: the destination is inside the directory being moved.
const PPC_BAD_MOV_ERR: i16 = -122;
/// volOnLinErr: the drive already holds a mounted volume.
const PPC_VOL_ON_LIN_ERR: i16 = -55;
/// nsDrvErr: no such drive.
const PPC_NS_DRV_ERR: i16 = -56;
/// notAFileErr: the catalog entry is a directory.
const PPC_NOT_A_FILE_ERR: i16 = -1302;
/// diffVolErr: the two specifications name different volumes.
const PPC_DIFF_VOL_ERR: i16 = -1303;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcFileManagerOperation {
    /// CloseWD(wdRefNum)
    CloseWd,
    /// CreateResFile(fileName)
    CreateResFile,
    /// CatMove(vRefNum, dirID, oldName, newDirID, newName)
    CatMove,
    /// PBCatMoveSync(paramBlock)
    PbCatMove,
    /// FSpCatMove(source, dest)
    FSpCatMove,
    /// FSpExchangeFiles(source, dest)
    FSpExchangeFiles,
    /// HOpenRF(vRefNum, dirID, fileName, permission, refNum)
    HOpenRf,
    /// PBOpenRF, PBHOpenRF and PBHOpenRFDeny.
    PbOpenRf {
        hierarchical: bool,
        deny_modes: bool,
    },
    /// PBHCopyFile(paramBlock)
    PbHCopyFile,
    /// PBHSetFLock(paramBlock)
    PbHSetFLock,
    /// PBHRstFLock(paramBlock)
    PbHRstFLock,
    /// PBDTOpenInform(paramBlock)
    PbDtOpenInform,
    /// PBDTSetComment(paramBlock)
    PbDtSetComment,
    /// PBEject(paramBlock)
    PbEject,
    /// PBMountVol(paramBlock)
    PbMountVol,
    /// fsdelete(fileName, vRefNum), the C-string glue for FSDelete.
    FsDeleteCString,
}

pub(super) struct PpcFileManagerContext<'a> {
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) files: &'a mut Vec<PpcFileRecord>,
    pub(super) writable_refnums: &'a mut HashSet<u16>,
    pub(super) vfs_files: &'a mut ProcessVfsFileRecords,
    pub(super) vfs_directories: &'a mut Vec<PpcVfsDirectory>,
    pub(super) deleted_vfs_file_paths: &'a mut Vec<String>,
    pub(super) vfs_resource_files: &'a mut ProcessVfsResourceFileRecords,
    pub(super) resource_files: &'a mut Vec<PpcResourceFileRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) next_file_ref_num: &'a mut i16,
    pub(super) last_resource_error: &'a mut i16,
    pub(super) default_dir_id: u32,
    pub(super) vfs_volumes: &'a [PpcVfsVolumeRecord],
    pub(super) working_directories: &'a mut HashMap<i16, ProcessWorkingDirectory>,
    pub(super) application_working_directory_ref_num: &'a mut i16,
}

pub(super) fn ppc_dispatch_file_manager(
    operation: PpcFileManagerOperation,
    context: PpcFileManagerContext<'_>,
) -> PpcImportAction {
    let PpcFileManagerContext {
        cpu,
        memory,
        files,
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
    } = context;
    let result = match operation {
        PpcFileManagerOperation::CloseWd => {
            // Files.h: CloseWD(wdRefNum) is PBCloseWD with ioVRefNum set.
            ppc_close_working_directory(
                cpu.gpr[3] as u16 as i16,
                vfs_volumes,
                working_directories,
                application_working_directory_ref_num,
            )
        }
        PpcFileManagerOperation::CreateResFile => {
            // Inside Macintosh: More Macintosh Toolbox (CreateResFile): the
            // file is created in the current directory of the default volume,
            // which is HCreateResFile(0, 0, fileName). The result is reported
            // through ResError.
            let mut call = cpu.clone();
            call.gpr[3] = 0;
            call.gpr[4] = 0;
            call.gpr[5] = cpu.gpr[3];
            ppc_h_create_res_file(
                &call,
                memory,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                default_dir_id,
                last_resource_error,
            );
            return PpcImportAction::ReturnPreserve;
        }
        PpcFileManagerOperation::CatMove | PpcFileManagerOperation::PbCatMove => {
            let pb = if operation == PpcFileManagerOperation::PbCatMove {
                let pb = cpu.gpr[3];
                if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 52) {
                    return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
                }
                Some(pb)
            } else {
                None
            };
            let (vref, source_dir, source_name, dest_dir, dest_name) = if let Some(pb) = pb {
                (
                    memory.read_u16_be(pb + 22).unwrap() as i16,
                    memory.read_u32_be(pb + 48).unwrap(),
                    memory.read_u32_be(pb + 18).unwrap(),
                    memory.read_u32_be(pb + 36).unwrap(),
                    memory.read_u32_be(pb + 28).unwrap(),
                )
            } else {
                (
                    cpu.gpr[3] as u16 as i16,
                    cpu.gpr[4],
                    cpu.gpr[5],
                    cpu.gpr[6],
                    cpu.gpr[7],
                )
            };
            // Inside Macintosh: Files, CatMove. Empty names identify the
            // directory itself; otherwise dirID names the parent directory.
            let mut resolve = |dir_id: u32, name_ptr: u32| -> Result<(i16, String), i16> {
                let (volume, directory) = ppc_volume_and_directory(
                    vref,
                    dir_id,
                    vfs_volumes,
                    default_dir_id,
                    *application_working_directory_ref_num,
                    working_directories,
                )?;
                if ppc_directory_path_for_id(vfs_directories, directory).is_none() {
                    return Err(PPC_DIR_NF_ERR);
                }
                let name = if name_ptr == 0 {
                    Vec::new()
                } else {
                    ppc_read_pstring_bytes(memory, name_ptr).ok_or(PPC_PARAM_ERR)?
                };
                let path = if !name.is_empty() && !name.contains(&b':') {
                    let parent = ppc_directory_path_for_id(vfs_directories, directory)
                        .ok_or(PPC_DIR_NF_ERR)?;
                    let path = ppc_join_vfs_path(parent, &decode_mac_roman(&name));
                    ppc_vfs_target_path(vfs_directories, vfs_files, vfs_resource_files, &path)
                } else {
                    ppc_resolved_fsspec_target_path(
                        vfs_directories,
                        vfs_files,
                        vfs_resource_files,
                        directory,
                        &name,
                    )
                }
                .ok_or(PPC_FNF_ERR)?;
                Ok((volume, path))
            };
            let source = resolve(source_dir, source_name);
            let dest = resolve(dest_dir, dest_name);
            let result = match (source, dest) {
                (Err(error), _) => error,
                (_, Err(PPC_FNF_ERR)) => PPC_DIR_NF_ERR,
                (_, Err(error)) => error,
                (Ok(_), Ok((_, dest_path)))
                    if ppc_directory_id_for_path(vfs_directories, &dest_path).is_none() =>
                {
                    PPC_BD_NAM_ERR
                }
                (Ok((source_vref, source_path)), Ok((dest_vref, dest_path))) => {
                    ppc_move_catalog_entry(
                        source_vref,
                        &source_path,
                        dest_vref,
                        &dest_path,
                        vfs_directories,
                        vfs_files,
                        deleted_vfs_file_paths,
                        files,
                        vfs_resource_files,
                        resource_files,
                        vfs_resources,
                    )
                }
            };
            if let Some(pb) = pb {
                ppc_complete_pb(memory, pb, result)
            } else {
                result
            }
        }
        PpcFileManagerOperation::FSpCatMove => ppc_fsp_cat_move(
            cpu.gpr[3],
            cpu.gpr[4],
            memory,
            vfs_directories,
            vfs_files,
            deleted_vfs_file_paths,
            files,
            vfs_resource_files,
            resource_files,
            vfs_resources,
        ),
        PpcFileManagerOperation::FSpExchangeFiles => ppc_fsp_exchange_files(
            cpu.gpr[3],
            cpu.gpr[4],
            memory,
            vfs_directories,
            vfs_files,
            deleted_vfs_file_paths,
            files,
            vfs_resource_files,
            resource_files,
            vfs_resources,
        ),
        PpcFileManagerOperation::HOpenRf => {
            // Inside Macintosh: Files (HOpenRF): the resource-fork form of
            // HOpen, with the same vRefNum, dirID, fileName, permission and
            // refNum parameters.
            match ppc_resource_fork_path_by_name(
                memory,
                vfs_directories,
                vfs_volumes,
                default_dir_id,
                *application_working_directory_ref_num,
                working_directories,
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[4],
                cpu.gpr[5],
            ) {
                Ok(path) => ppc_open_resource_fork_path(
                    memory,
                    &path,
                    cpu.gpr[6] as u8,
                    cpu.gpr[7],
                    vfs_files,
                    vfs_resource_files,
                    vfs_resources,
                    files,
                    writable_refnums,
                    next_file_ref_num,
                ),
                Err(error) => error,
            }
        }
        PpcFileManagerOperation::PbOpenRf {
            hierarchical,
            deny_modes,
        } => ppc_pb_open_rf(
            hierarchical,
            deny_modes,
            cpu.gpr[3],
            memory,
            files,
            writable_refnums,
            vfs_files,
            vfs_directories,
            vfs_resource_files,
            vfs_resources,
            next_file_ref_num,
            default_dir_id,
            vfs_volumes,
            working_directories,
            *application_working_directory_ref_num,
        ),
        PpcFileManagerOperation::PbHCopyFile => ppc_pbh_copy_file(
            cpu.gpr[3],
            memory,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            vfs_resources,
            default_dir_id,
            vfs_volumes,
            working_directories,
            *application_working_directory_ref_num,
        ),
        PpcFileManagerOperation::PbHSetFLock | PpcFileManagerOperation::PbHRstFLock => {
            // Inside Macintosh: Files (PBHSetFLock, PBHRstFLock): both name a
            // file with ioNamePtr, ioVRefNum and ioDirID. The runtime does not
            // model the software lock attribute, so writes are never refused
            // for a lock; the call still reports a missing file.
            let pb = cpu.gpr[3];
            if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 52) {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            }
            let result = ppc_pb_catalog_path(
                memory,
                pb,
                true,
                vfs_directories,
                vfs_volumes,
                default_dir_id,
                *application_working_directory_ref_num,
                working_directories,
            )
            .and_then(|(_, _, parent, name)| {
                let path = ppc_join_vfs_path(&parent, &name);
                if ppc_directory_id_for_path(vfs_directories, &path).is_some() {
                    Err(PPC_NOT_A_FILE_ERR)
                } else {
                    ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &path)
                        .ok_or(PPC_FNF_ERR)
                }
            })
            .map_or_else(|error| error, |_| PPC_NO_ERR);
            ppc_complete_pb(memory, pb, result)
        }
        PpcFileManagerOperation::PbDtOpenInform => {
            // More Macintosh Toolbox (PBDTOpenInform): PBDTGetPath that also
            // returns ioTagInfo. No inform flags are reported.
            let pb = cpu.gpr[3];
            let result = dispatch_files::ppc_pb_dt_get_path(cpu, memory, vfs_volumes);
            if result == PPC_NO_ERR {
                let _ = memory.write_u32_be(pb + 28, 0);
            }
            result
        }
        PpcFileManagerOperation::PbDtSetComment => ppc_pb_dt_set_comment(
            cpu.gpr[3],
            memory,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            vfs_volumes,
        ),
        PpcFileManagerOperation::PbEject => {
            // The runtime has no removable media; like Eject, PBEject on a
            // mounted volume has nothing to do.
            let pb = cpu.gpr[3];
            if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 24) {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            }
            let vref = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
            let result = if dispatch_files::ppc_working_directory_info(
                vref,
                *application_working_directory_ref_num,
                default_dir_id,
                working_directories,
                vfs_volumes,
            )
            .is_some()
            {
                PPC_NO_ERR
            } else {
                PPC_NSV_ERR
            };
            ppc_complete_pb(memory, pb, result)
        }
        PpcFileManagerOperation::PbMountVol => {
            // Inside Macintosh: Files (PBMountVol): ioVRefNum names a drive.
            // Every drive the runtime has already holds its mounted volume
            // (drive 1 is the startup volume, as GetVInfo numbers them), so
            // the result is volOnLinErr for a known drive and nsDrvErr for
            // any other.
            let pb = cpu.gpr[3];
            if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 24) {
                return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
            }
            let drive = memory.read_u16_be(pb + 22).unwrap_or(0) as i16;
            let known = drive == 1 || (drive > 1 && (drive as usize - 2) < vfs_volumes.len());
            let result = if known {
                PPC_VOL_ON_LIN_ERR
            } else {
                PPC_NS_DRV_ERR
            };
            ppc_complete_pb(memory, pb, result)
        }
        PpcFileManagerOperation::FsDeleteCString => {
            // The lowercase C glue converts the name to a Pascal string in
            // place, calls FSDelete, and converts it back.
            ppc_c2pstr(cpu, memory);
            let result = ppc_delete_by_name(
                PpcDeleteByNameOperation::LegacyHighLevel,
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
            ppc_p2cstr(cpu, memory);
            result
        }
    };
    PpcImportAction::Return(ppc_i16_result(result))
}

/// PBCloseWD: closes a working directory. A volume reference number is
/// accepted and ignored. Inside Macintosh: Files (PBCloseWD).
pub(super) fn ppc_close_working_directory(
    wd_ref_num: i16,
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    application_working_directory_ref_num: &mut i16,
) -> i16 {
    let is_volume_ref_num = wd_ref_num == PPC_BOOT_VOLUME_REF_NUM
        || vfs_volumes
            .iter()
            .any(|volume| volume.ref_num == wd_ref_num);
    let closed = working_directories.remove(&wd_ref_num);
    if is_volume_ref_num || closed.is_some() {
        if *application_working_directory_ref_num == wd_ref_num {
            *application_working_directory_ref_num = closed
                .map(|record| record.volume_ref_num)
                .unwrap_or(wd_ref_num);
        }
        PPC_NO_ERR
    } else {
        PPC_RF_NUM_ERR
    }
}

fn ppc_same_volume(left: i16, right: i16) -> bool {
    let normalize = |vref: i16| {
        if vref == 0 {
            PPC_BOOT_VOLUME_REF_NUM
        } else {
            vref
        }
    };
    normalize(left) == normalize(right)
}

/// Resolves a vRefNum (volume or working directory) and dirID pair to the
/// volume and directory it names. A zero dirID selects the working
/// directory's own directory.
fn ppc_volume_and_directory(
    vref: i16,
    dir_id: u32,
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    application_working_directory_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
) -> Result<(i16, u32), i16> {
    let Some(record) = dispatch_files::ppc_working_directory_info(
        vref,
        application_working_directory_ref_num,
        default_dir_id,
        working_directories,
        vfs_volumes,
    ) else {
        return Err(PPC_NSV_ERR);
    };
    Ok((
        record.volume_ref_num,
        if dir_id == 0 { record.dir_id } else { dir_id },
    ))
}

/// Resolves the parameter-block catalog name (ioNamePtr, ioVRefNum and, for
/// the hierarchical calls, ioDirID) to its volume, directory path and leaf
/// name. A name containing colons is a partial pathname below the directory.
#[allow(clippy::too_many_arguments)]
fn ppc_pb_catalog_path(
    memory: &mut PpcSectionMem,
    pb: u32,
    hierarchical: bool,
    vfs_directories: &[PpcVfsDirectory],
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    application_working_directory_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
) -> Result<(i16, u32, String, String), i16> {
    let name_ptr = memory.read_u32_be(pb + 18).ok_or(PPC_PARAM_ERR)?;
    let vref = memory.read_u16_be(pb + 22).ok_or(PPC_PARAM_ERR)? as i16;
    let dir_id = if hierarchical {
        memory.read_u32_be(pb + 48).ok_or(PPC_PARAM_ERR)?
    } else {
        0
    };
    ppc_catalog_path_by_name(
        memory,
        vfs_directories,
        vfs_volumes,
        default_dir_id,
        application_working_directory_ref_num,
        working_directories,
        vref,
        dir_id,
        name_ptr,
    )
}

#[allow(clippy::too_many_arguments)]
fn ppc_catalog_path_by_name(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    application_working_directory_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    vref: i16,
    dir_id: u32,
    name_ptr: u32,
) -> Result<(i16, u32, String, String), i16> {
    let (volume, dir_id) = ppc_volume_and_directory(
        vref,
        dir_id,
        vfs_volumes,
        default_dir_id,
        application_working_directory_ref_num,
        working_directories,
    )?;
    let name_bytes = ppc_read_pstring_bytes(memory, name_ptr).ok_or(PPC_PARAM_ERR)?;
    let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if name.is_empty() {
        return Err(PPC_BD_NAM_ERR);
    }
    let parent = ppc_directory_path_for_id(vfs_directories, dir_id).ok_or(PPC_DIR_NF_ERR)?;
    let full = ppc_join_vfs_path(parent, &name);
    let (parent, leaf) = full
        .rsplit_once('/')
        .map_or((String::new(), full.clone()), |(parent, leaf)| {
            (parent.to_string(), leaf.to_string())
        });
    let dir_id = ppc_directory_id_for_path(vfs_directories, &parent)
        .or_else(|| parent.is_empty().then_some(dir_id))
        .ok_or(PPC_DIR_NF_ERR)?;
    Ok((volume, dir_id, parent, leaf))
}

#[allow(clippy::too_many_arguments)]
fn ppc_resource_fork_path_by_name(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    application_working_directory_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    vref: i16,
    dir_id: u32,
    name_ptr: u32,
) -> Result<String, i16> {
    let (_, _, parent, name) = ppc_catalog_path_by_name(
        memory,
        vfs_directories,
        vfs_volumes,
        default_dir_id,
        application_working_directory_ref_num,
        working_directories,
        vref,
        dir_id,
        name_ptr,
    )?;
    Ok(ppc_join_vfs_path(&parent, &name))
}

/// PBOpenRF, PBHOpenRF and PBHOpenRFDeny open a resource fork named by the
/// parameter block and return its reference number in ioRefNum (+24).
/// PBHOpenRFDeny's low ioDenyModes byte carries the read/write request in
/// bits 0 and 1; the deny bits have no other users to exclude.
/// Inside Macintosh: Files (PBOpenRF, PBHOpenRF, PBHOpenRFDeny).
#[allow(clippy::too_many_arguments)]
fn ppc_pb_open_rf(
    hierarchical: bool,
    deny_modes: bool,
    pb: u32,
    memory: &mut PpcSectionMem,
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_directories: &[PpcVfsDirectory],
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &[PpcVfsResourceRecord],
    next_file_ref_num: &mut i16,
    default_dir_id: u32,
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    application_working_directory_ref_num: i16,
) -> i16 {
    let size = if hierarchical { 52 } else { 28 };
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, size) {
        return PPC_PARAM_ERR;
    }
    let raw_permission = memory.read_u8(pb + 27).unwrap_or(0);
    let permission = if deny_modes {
        match raw_permission & 0x03 {
            0 => return ppc_complete_pb(memory, pb, PPC_PARAM_ERR),
            access => access,
        }
    } else {
        raw_permission
    };
    let result = match ppc_pb_catalog_path(
        memory,
        pb,
        hierarchical,
        vfs_directories,
        vfs_volumes,
        default_dir_id,
        application_working_directory_ref_num,
        working_directories,
    ) {
        Ok((_, _, parent, name)) => ppc_open_resource_fork_path(
            memory,
            &ppc_join_vfs_path(&parent, &name),
            permission,
            pb + 24,
            vfs_files,
            vfs_resource_files,
            vfs_resources,
            files,
            writable_refnums,
            next_file_ref_num,
        ),
        Err(error) => error,
    };
    ppc_complete_pb(memory, pb, result)
}

/// The path an FSSpec names, which must already exist as a file or a
/// directory, together with its volume reference number.
fn ppc_existing_fsspec_target(
    memory: &mut PpcSectionMem,
    spec: u32,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
) -> Result<(i16, String), i16> {
    let (vref, dir_id, name) = ppc_read_fsspec_parts(memory, spec).ok_or(PPC_PARAM_ERR)?;
    if ppc_directory_path_for_id(vfs_directories, dir_id).is_none() {
        return Err(PPC_DIR_NF_ERR);
    }
    let path = ppc_resolved_fsspec_target_path(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        dir_id,
        &name,
    )
    .ok_or(PPC_FNF_ERR)?;
    Ok((vref, path))
}

/// FSpCatMove moves a file or directory into the directory that `dest`
/// names, keeping its name. Inside Macintosh: Files (FSpCatMove, PBCatMove).
#[allow(clippy::too_many_arguments)]
fn ppc_fsp_cat_move(
    source: u32,
    dest: u32,
    memory: &mut PpcSectionMem,
    vfs_directories: &mut Vec<PpcVfsDirectory>,
    vfs_files: &mut ProcessVfsFileRecords,
    deleted_vfs_file_paths: &mut Vec<String>,
    files: &mut Vec<PpcFileRecord>,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> i16 {
    let (source_vref, source_path) = match ppc_existing_fsspec_target(
        memory,
        source,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
    ) {
        Ok(target) => target,
        Err(error) => return error,
    };
    let (dest_vref, dest_path) = match ppc_existing_fsspec_target(
        memory,
        dest,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
    ) {
        Ok(target) => target,
        Err(PPC_FNF_ERR) => return PPC_DIR_NF_ERR,
        Err(error) => return error,
    };
    ppc_move_catalog_entry(
        source_vref,
        &source_path,
        dest_vref,
        &dest_path,
        vfs_directories,
        vfs_files,
        deleted_vfs_file_paths,
        files,
        vfs_resource_files,
        resource_files,
        vfs_resources,
    )
}

#[allow(clippy::too_many_arguments)]
fn ppc_move_catalog_entry(
    source_vref: i16,
    source_path: &str,
    dest_vref: i16,
    dest_path: &str,
    vfs_directories: &mut Vec<PpcVfsDirectory>,
    vfs_files: &mut ProcessVfsFileRecords,
    deleted_vfs_file_paths: &mut Vec<String>,
    files: &mut Vec<PpcFileRecord>,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> i16 {
    if !ppc_same_volume(source_vref, dest_vref) {
        return PPC_DIFF_VOL_ERR;
    }
    let Some(dest_dir_id) = ppc_directory_id_for_path(vfs_directories, dest_path) else {
        return PPC_DIR_NF_ERR;
    };
    if ppc_renamed_path(dest_path, source_path, "").is_some() {
        return PPC_BAD_MOV_ERR;
    }
    let leaf = ppc_vfs_basename(source_path).to_string();
    let new_path = ppc_join_vfs_path(dest_path, &leaf);
    if new_path.eq_ignore_ascii_case(source_path) {
        return PPC_NO_ERR;
    }
    if ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &new_path).is_some()
        || ppc_directory_id_for_path(vfs_directories, &new_path).is_some()
    {
        return PPC_DUP_FN_ERR;
    }
    let moved_dir_id = ppc_directory_id_for_path(vfs_directories, source_path);
    ppc_move_vfs_path(
        source_path,
        &new_path,
        vfs_directories,
        vfs_files,
        deleted_vfs_file_paths,
        files,
        vfs_resource_files,
        resource_files,
        vfs_resources,
    );
    if let Some(dir_id) = moved_dir_id {
        if let Some(directory) = vfs_directories
            .iter_mut()
            .find(|directory| directory.dir_id == dir_id)
        {
            directory.parent_dir_id = dest_dir_id;
        }
    }
    PPC_NO_ERR
}

/// Catalog metadata that stays with the name when two files exchange forks.
fn ppc_catalog_metadata(
    path: &str,
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
) -> Option<(u32, u32, u16)> {
    vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
        .map(|file| (file.creator, file.file_type, file.finder_flags))
        .or_else(|| {
            vfs_resource_files
                .iter()
                .find(|file| file.path.eq_ignore_ascii_case(path))
                .map(|file| (file.creator, file.file_type, file.finder_flags))
        })
}

fn ppc_set_catalog_metadata(
    path: &str,
    (creator, file_type, finder_flags): (u32, u32, u16),
    vfs_files: &mut [PpcVfsFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
) {
    for file in vfs_files
        .iter_mut()
        .filter(|file| file.path.eq_ignore_ascii_case(path))
    {
        (file.creator, file.file_type, file.finder_flags) = (creator, file_type, finder_flags);
        file.dirty = true;
    }
    for file in vfs_resource_files
        .iter_mut()
        .filter(|file| file.path.eq_ignore_ascii_case(path))
    {
        (file.creator, file.file_type, file.finder_flags) = (creator, file_type, finder_flags);
        file.dirty = true;
    }
}

/// FSpExchangeFiles swaps the data and resource forks of two files on one
/// volume. Names, parent directories and Finder information stay with the
/// catalog entries, and open access paths follow the data they read.
/// Inside Macintosh: Files (FSpExchangeFiles, PBExchangeFiles).
#[allow(clippy::too_many_arguments)]
fn ppc_fsp_exchange_files(
    source: u32,
    dest: u32,
    memory: &mut PpcSectionMem,
    vfs_directories: &mut Vec<PpcVfsDirectory>,
    vfs_files: &mut ProcessVfsFileRecords,
    deleted_vfs_file_paths: &mut Vec<String>,
    files: &mut Vec<PpcFileRecord>,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> i16 {
    let mut targets = Vec::with_capacity(2);
    for spec in [source, dest] {
        match ppc_existing_fsspec_target(
            memory,
            spec,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
        ) {
            Ok((_, path)) if ppc_directory_id_for_path(vfs_directories, &path).is_some() => {
                return PPC_NOT_A_FILE_ERR;
            }
            Ok(target) => targets.push(target),
            Err(error) => return error,
        }
    }
    let (source_vref, source_path) = targets[0].clone();
    let (dest_vref, dest_path) = targets[1].clone();
    if !ppc_same_volume(source_vref, dest_vref) {
        return PPC_DIFF_VOL_ERR;
    }
    if source_path.eq_ignore_ascii_case(&dest_path) {
        return PPC_NO_ERR;
    }
    let source_metadata = ppc_catalog_metadata(&source_path, vfs_files, vfs_resource_files);
    let dest_metadata = ppc_catalog_metadata(&dest_path, vfs_files, vfs_resource_files);
    let open_fork = |path: &str| format!("{PPC_OPEN_RESOURCE_FORK_PREFIX}{path}");
    let swap_path = "\0exchange-files";
    let deleted_before = deleted_vfs_file_paths.clone();
    for (from, to) in [
        (source_path.clone(), swap_path.to_string()),
        (dest_path.clone(), source_path.clone()),
        (swap_path.to_string(), dest_path.clone()),
        (open_fork(&source_path), open_fork(swap_path)),
        (open_fork(&dest_path), open_fork(&source_path)),
        (open_fork(swap_path), open_fork(&dest_path)),
    ] {
        ppc_move_vfs_path(
            &from,
            &to,
            vfs_directories,
            vfs_files,
            deleted_vfs_file_paths,
            files,
            vfs_resource_files,
            resource_files,
            vfs_resources,
        );
    }
    // The host copies of both names are rewritten from their now-dirty
    // records, which also drops a fork that moved to the other name.
    *deleted_vfs_file_paths = deleted_before;
    for path in [&source_path, &dest_path] {
        if !deleted_vfs_file_paths
            .iter()
            .any(|deleted| deleted.eq_ignore_ascii_case(path))
        {
            deleted_vfs_file_paths.push(path.to_string());
        }
    }
    if let Some(metadata) = source_metadata {
        ppc_set_catalog_metadata(&source_path, metadata, vfs_files, vfs_resource_files);
    }
    if let Some(metadata) = dest_metadata {
        ppc_set_catalog_metadata(&dest_path, metadata, vfs_files, vfs_resource_files);
    }
    PPC_NO_ERR
}

/// PBHCopyFile duplicates a file, both forks and its Finder information,
/// into the directory ioNewDirID (optionally qualified by the partial
/// pathname ioNewName), naming the copy ioCopyName or, when that is nil, the
/// source name. Inside Macintosh: Files (PBHCopyFile).
#[allow(clippy::too_many_arguments)]
fn ppc_pbh_copy_file(
    pb: u32,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &[PpcVfsResourceRecord],
    default_dir_id: u32,
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
    application_working_directory_ref_num: i16,
) -> i16 {
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 52) {
        return PPC_PARAM_ERR;
    }
    let result = (|| {
        let (source_volume, _, parent, name) = ppc_pb_catalog_path(
            memory,
            pb,
            true,
            vfs_directories,
            vfs_volumes,
            default_dir_id,
            application_working_directory_ref_num,
            working_directories,
        )?;
        let requested = ppc_join_vfs_path(&parent, &name);
        if ppc_directory_id_for_path(vfs_directories, &requested).is_some() {
            return Err(PPC_NOT_A_FILE_ERR);
        }
        let source_path = ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &requested)
            .ok_or(PPC_FNF_ERR)?;
        let dest_vref = memory.read_u16_be(pb + 24).ok_or(PPC_PARAM_ERR)? as i16;
        let new_name_ptr = memory.read_u32_be(pb + 28).ok_or(PPC_PARAM_ERR)?;
        let copy_name_ptr = memory.read_u32_be(pb + 32).ok_or(PPC_PARAM_ERR)?;
        let new_dir_id = memory.read_u32_be(pb + 36).ok_or(PPC_PARAM_ERR)?;
        let (dest_volume, dest_dir_id) = ppc_volume_and_directory(
            dest_vref,
            new_dir_id,
            vfs_volumes,
            default_dir_id,
            application_working_directory_ref_num,
            working_directories,
        )?;
        if !ppc_same_volume(source_volume, dest_volume) {
            return Err(PPC_DIFF_VOL_ERR);
        }
        let mut dest_dir = ppc_directory_path_for_id(vfs_directories, dest_dir_id)
            .ok_or(PPC_DIR_NF_ERR)?
            .to_string();
        if new_name_ptr != 0 {
            let bytes = ppc_read_pstring_bytes(memory, new_name_ptr).ok_or(PPC_PARAM_ERR)?;
            let partial = ppc_normalize_vfs_path(&decode_mac_roman(&bytes));
            if !partial.is_empty() {
                dest_dir = ppc_join_vfs_path(&dest_dir, &partial);
            }
        }
        if ppc_directory_id_for_path(vfs_directories, &dest_dir).is_none() && !dest_dir.is_empty() {
            return Err(PPC_DIR_NF_ERR);
        }
        let copy_name = if copy_name_ptr == 0 {
            ppc_vfs_basename(&source_path).to_string()
        } else {
            let bytes = ppc_read_pstring_bytes(memory, copy_name_ptr).ok_or(PPC_PARAM_ERR)?;
            let name = decode_mac_roman(&bytes);
            if name.is_empty() || name.contains(':') {
                return Err(PPC_BD_NAM_ERR);
            }
            name
        };
        let dest_path = ppc_join_vfs_path(&dest_dir, &copy_name);
        if ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &dest_path).is_some()
            || ppc_directory_id_for_path(vfs_directories, &dest_path).is_some()
        {
            return Err(PPC_DUP_FN_ERR);
        }
        let data_copy = vfs_files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(&source_path))
            .map(|file| PpcVfsFileRecord {
                path: dest_path.clone(),
                data: file.data.to_vec().into(),
                creator: file.creator,
                file_type: file.file_type,
                finder_flags: file.finder_flags,
                dirty: true,
            });
        let resource_copy = vfs_resource_files
            .iter()
            .position(|file| file.path.eq_ignore_ascii_case(&source_path))
            .map(|index| {
                let file = &vfs_resource_files[index];
                let bytes = vfs_resource_files
                    .fork(&file.path)
                    .cloned()
                    .filter(|bytes| !bytes.is_empty())
                    .or_else(|| ppc_serialized_resource_fork(file, vfs_resources))
                    .unwrap_or_default();
                PpcVfsResourceFileRecord {
                    path: dest_path.clone(),
                    creator: file.creator,
                    file_type: file.file_type,
                    finder_flags: file.finder_flags,
                    resource_len: u32::try_from(bytes.len()).unwrap_or(u32::MAX),
                    raw_data: (!bytes.is_empty()).then(|| bytes.into()),
                    map_attrs: file.map_attrs,
                    dirty: true,
                }
            });
        if let Some(record) = data_copy {
            vfs_files.push(record);
        }
        if let Some(record) = resource_copy {
            vfs_resource_files.push(record);
        }
        Ok(())
    })();
    ppc_complete_pb(
        memory,
        pb,
        result.map_or_else(|error| error, |()| PPC_NO_ERR),
    )
}

/// PBDTSetComment attaches a Finder comment to a file in the volume's
/// desktop database. The runtime's desktop database keeps no comments (as
/// PBDTGetComment reports), so a valid request is accepted and discarded.
/// More Macintosh Toolbox (PBDTSetComment).
fn ppc_pb_dt_set_comment(
    pb: u32,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_volumes: &[PpcVfsVolumeRecord],
) -> i16 {
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 52) {
        return PPC_PARAM_ERR;
    }
    let desktop_ref = memory.read_u16_be(pb + 24).unwrap_or(0);
    let valid_ref = desktop_ref >= 0x7f00 && usize::from(desktop_ref - 0x7f00) <= vfs_volumes.len();
    let result = if !valid_ref {
        PPC_RF_NUM_ERR
    } else {
        let name_ptr = memory.read_u32_be(pb + 18).unwrap_or(0);
        let dir_id = memory.read_u32_be(pb + 48).unwrap_or(0);
        let name = if name_ptr == 0 {
            Some(Vec::new())
        } else {
            ppc_read_pstring_bytes(memory, name_ptr)
        };
        match name {
            None => PPC_PARAM_ERR,
            Some(name) => {
                if ppc_fsspec_target_exists(
                    vfs_directories,
                    vfs_files,
                    vfs_resource_files,
                    dir_id,
                    &name,
                ) {
                    PPC_NO_ERR
                } else {
                    PPC_FNF_ERR
                }
            }
        }
    };
    ppc_complete_pb(memory, pb, result)
}
