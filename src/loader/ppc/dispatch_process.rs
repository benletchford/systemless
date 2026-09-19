//! Typed Process Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcProcessDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) vfs_directories: &'a [PpcVfsDirectory],
    pub(super) vfs_files: &'a [PpcVfsFileRecord],
    pub(super) vfs_resource_files: &'a [PpcVfsResourceFileRecord],
    pub(super) launched_app_path: Option<&'a str>,
}

pub(super) fn dispatch_process_import(
    context: PpcProcessDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcProcessDispatchContext {
        binding,
        cpu,
        memory,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        launched_app_path,
    } = context;

    let result = match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetCurrentProcess => ppc_get_current_process(cpu, memory),
        PpcImportDispatcherTarget::WakeUpProcess => ppc_wake_up_process(cpu, memory),
        PpcImportDispatcherTarget::SameProcess => ppc_same_process(cpu, memory),
        PpcImportDispatcherTarget::GetProcessInformation => ppc_get_process_information(
            cpu,
            memory,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            launched_app_path,
        ),
        _ => return None,
    };
    Some(PpcImportAction::Return(ppc_i16_result(result)))
}

fn ppc_get_current_process(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let psn_ptr = cpu.gpr[3];
    if psn_ptr == 0 || !ppc_memory_can_write_bytes(memory, psn_ptr, 8) {
        return PPC_PARAM_ERR;
    }
    if memory
        .write_u32_be(psn_ptr, ProcessSerialNumber::CURRENT.high)
        .is_none()
        || memory
            .write_u32_be(psn_ptr + 4, ProcessSerialNumber::CURRENT.low)
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

fn ppc_wake_up_process(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    // WakeUpProcess (_OSDispatch, selector $003C)
    // Makes a process suspended by WaitNextEvent eligible to receive CPU time.
    // FUNCTION WakeUpProcess (PSN: ProcessSerialNumber): OSErr;
    // Inside Macintosh: Processes (1994), pp. 2-27–2-28.
    // This runtime schedules one guest process, so waking the current process
    // is an ordering-neutral no-op; malformed pointers and unknown PSNs still
    // retain the Process Manager's error distinction.
    let psn_ptr = cpu.gpr[3];
    if psn_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(psn_high) = memory.read_u32_be(psn_ptr) else {
        return PPC_PARAM_ERR;
    };
    let Some(psn_low) = memory.read_u32_be(psn_ptr + 4) else {
        return PPC_PARAM_ERR;
    };
    if !ProcessSerialNumber::new(psn_high, psn_low).is_current() {
        return PPC_PROC_NOT_FOUND_ERR;
    }
    PPC_NO_ERR
}

fn ppc_same_process(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let first = cpu.gpr[3];
    let second = cpu.gpr[4];
    let result_ptr = cpu.gpr[5];
    if first == 0 || second == 0 || result_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(first_high) = memory.read_u32_be(first) else {
        return PPC_PARAM_ERR;
    };
    let Some(first_low) = memory.read_u32_be(first + 4) else {
        return PPC_PARAM_ERR;
    };
    let Some(second_high) = memory.read_u32_be(second) else {
        return PPC_PARAM_ERR;
    };
    let Some(second_low) = memory.read_u32_be(second + 4) else {
        return PPC_PARAM_ERR;
    };
    let first = ProcessSerialNumber::new(first_high, first_low);
    let second = ProcessSerialNumber::new(second_high, second_low);
    if memory
        .write_u8(result_ptr, u8::from(first == second))
        .is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

fn ppc_get_process_information(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    launched_app_path: Option<&str>,
) -> i16 {
    let psn_ptr = cpu.gpr[3];
    let info_ptr = cpu.gpr[4];
    if psn_ptr == 0
        || info_ptr == 0
        || !ppc_memory_can_write_bytes(memory, info_ptr, 60)
        || !ppc_memory_can_write_bytes(memory, psn_ptr, 8)
    {
        return PPC_PARAM_ERR;
    }
    let psn_high = memory.read_u32_be(psn_ptr).unwrap_or(u32::MAX);
    let psn_low = memory.read_u32_be(psn_ptr + 4).unwrap_or(u32::MAX);
    if !ProcessSerialNumber::new(psn_high, psn_low).is_current() {
        return PPC_PROC_NOT_FOUND_ERR;
    }

    let app = ppc_current_process_app_metadata(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        launched_app_path,
    );
    let name_ptr = memory.read_u32_be(info_ptr + 4).unwrap_or(0);
    if name_ptr != 0 && !ppc_write_pstring_bytes(memory, name_ptr, &app.name) {
        return PPC_PARAM_ERR;
    }
    let app_spec_ptr = memory.read_u32_be(info_ptr + 56).unwrap_or(0);
    if app_spec_ptr != 0
        && ppc_write_fsspec(
            memory,
            app_spec_ptr,
            PPC_BOOT_VOLUME_REF_NUM,
            app.parent_dir_id,
            &app.name,
        )
        .is_none()
    {
        return PPC_PARAM_ERR;
    }

    let process_size = PPC_STACK_TOP - PPC_CODE_BASE;
    let process_free_mem = process_size / 2;
    if memory
        .write_u32_be(info_ptr + 8, ProcessSerialNumber::CURRENT.high)
        .is_none()
        || memory
            .write_u32_be(info_ptr + 12, ProcessSerialNumber::CURRENT.low)
            .is_none()
        || memory.write_u32_be(info_ptr + 16, app.file_type).is_none()
        || memory.write_u32_be(info_ptr + 20, app.creator).is_none()
        || memory.write_u32_be(info_ptr + 24, 0).is_none()
        || memory.write_u32_be(info_ptr + 28, PPC_CODE_BASE).is_none()
        || memory.write_u32_be(info_ptr + 32, process_size).is_none()
        || memory
            .write_u32_be(info_ptr + 36, process_free_mem)
            .is_none()
        || memory.write_u32_be(info_ptr + 40, 0).is_none()
        || memory.write_u32_be(info_ptr + 44, 0).is_none()
        || memory.write_u32_be(info_ptr + 48, 0).is_none()
        || memory.write_u32_be(info_ptr + 52, 0).is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

struct PpcProcessAppMetadata {
    name: Vec<u8>,
    parent_dir_id: u32,
    file_type: u32,
    creator: u32,
}

fn ppc_current_process_app_metadata(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    launched_app_path: Option<&str>,
) -> PpcProcessAppMetadata {
    let appl_type = u32::from_be_bytes(*b"APPL");
    if let Some(path) = launched_app_path {
        if let Some(file) = vfs_files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(path) && file.file_type == appl_type)
        {
            return PpcProcessAppMetadata {
                name: ppc_vfs_basename_bytes(&file.path),
                parent_dir_id: ppc_parent_dir_id_for_path(vfs_directories, &file.path),
                file_type: file.file_type,
                creator: file.creator,
            };
        }
        if let Some(file) = vfs_resource_files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(path) && file.file_type == appl_type)
        {
            return PpcProcessAppMetadata {
                name: ppc_vfs_basename_bytes(&file.path),
                parent_dir_id: ppc_parent_dir_id_for_path(vfs_directories, &file.path),
                file_type: file.file_type,
                creator: file.creator,
            };
        }
    }
    if let Some(file) = vfs_files.iter().find(|file| file.file_type == appl_type) {
        return PpcProcessAppMetadata {
            name: ppc_vfs_basename_bytes(&file.path),
            parent_dir_id: ppc_parent_dir_id_for_path(vfs_directories, &file.path),
            file_type: file.file_type,
            creator: file.creator,
        };
    }
    if let Some(file) = vfs_resource_files
        .iter()
        .find(|file| file.file_type == appl_type)
    {
        return PpcProcessAppMetadata {
            name: ppc_vfs_basename_bytes(&file.path),
            parent_dir_id: ppc_parent_dir_id_for_path(vfs_directories, &file.path),
            file_type: file.file_type,
            creator: file.creator,
        };
    }
    PpcProcessAppMetadata {
        name: b"Application".to_vec(),
        parent_dir_id: PPC_ROOT_DIR_ID,
        file_type: appl_type,
        creator: u32::from_be_bytes(*b"????"),
    }
}
