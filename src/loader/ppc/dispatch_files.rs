use super::*;

pub(super) struct PpcFileDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) files: &'a mut Vec<PpcFileRecord>,
    pub(super) writable_refnums: &'a mut HashSet<u16>,
    pub(super) vfs_files: &'a mut ProcessVfsFileRecords,
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
        _ => None,
    }
}
