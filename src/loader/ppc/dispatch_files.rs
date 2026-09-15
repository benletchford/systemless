use super::*;

pub(super) struct PpcFileDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) files: &'a mut Vec<PpcFileRecord>,
    pub(super) writable_refnums: &'a mut HashSet<u16>,
    pub(super) vfs_files: &'a mut ProcessVfsFileRecords,
    pub(super) vfs_directories: &'a [PpcVfsDirectory],
    pub(super) deleted_vfs_file_paths: &'a mut Vec<String>,
    pub(super) vfs_resource_files: &'a mut ProcessVfsResourceFileRecords,
    pub(super) resource_files: &'a mut Vec<PpcResourceFileRecord>,
    pub(super) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(super) default_dir_id: u32,
}

pub(super) fn dispatch_file_import(
    context: PpcFileDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcFileDispatchContext {
        binding,
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
        default_dir_id,
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
        PpcImportDispatcherTarget::PBCreate(operation) => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_pb_create(
                operation,
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                default_dir_id,
            )),
        )),
        PpcImportDispatcherTarget::FSpCreate => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fsp_create(cpu, memory, vfs_directories, vfs_files),
        ))),
        PpcImportDispatcherTarget::HCreate => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_h_create(cpu, memory, vfs_directories, vfs_files, default_dir_id),
        ))),
        PpcImportDispatcherTarget::Create => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_create(cpu, memory, vfs_directories, vfs_files, default_dir_id),
        ))),
        PpcImportDispatcherTarget::FSpDelete => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_fsp_delete(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                deleted_vfs_file_paths,
                files,
                vfs_resource_files,
                resource_files,
                vfs_resources,
            ),
        ))),
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
        _ => None,
    }
}
