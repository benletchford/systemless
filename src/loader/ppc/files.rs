//! PowerPC File Manager, Virtual File System, FSSpec, Alias, and Catalog emulation.

use ppc::{PpcCpu, PpcMemory};

use super::graphics::*;
use super::vfs::*;
use super::*;
use crate::process_context::*;
use crate::trap::types::decode_mac_roman;

pub(crate) const PPC_BOOT_VOLUME_REF_NUM: i16 = -1;
pub(crate) const PPC_ROOT_DIR_ID: u32 = 2;
pub(crate) const PPC_SYSTEM_FOLDER_DIR_ID: u32 = 16;
pub(crate) const PPC_PREFERENCES_DIR_ID: u32 = 17;
pub(crate) const PPC_FIRST_DYNAMIC_DIR_ID: u32 = 18;
pub(crate) const PPC_DIRECTORY_FILE_TYPE: u32 = u32::from_be_bytes(*b"fold");
pub(crate) const PPC_DIRECTORY_CREATOR: u32 = u32::from_be_bytes(*b"MACS");
pub(crate) const PPC_FSSPEC_SIZE: usize = 70;
pub(crate) const PPC_FSSPEC_MAX_NAME_LEN: usize = 63;
pub(crate) const PPC_ALIAS_RECORD_MAGIC: u32 = u32::from_be_bytes(*b"alis");
pub(crate) const PPC_ALIAS_RECORD_VERSION: u16 = 1;
#[cfg(test)]
pub(crate) const PPC_ALIAS_RECORD_HEADER_SIZE: usize = 16;
#[cfg(test)]
pub(crate) const PPC_ALIAS_RECORD_FSSPEC_OFFSET: usize = PPC_ALIAS_RECORD_HEADER_SIZE;
#[cfg(test)]
pub(crate) const PPC_ALIAS_RECORD_SIZE: usize = PPC_ALIAS_RECORD_HEADER_SIZE + PPC_FSSPEC_SIZE;
pub(crate) const PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE: usize = 150;
pub(crate) const PPC_CLASSIC_ALIAS_RECORD_VERSION: u16 = 2;
pub(crate) const PPC_CLASSIC_ALIAS_VOLUME_NAME_OFFSET: usize = 10;
pub(crate) const PPC_CLASSIC_ALIAS_FILE_NAME_OFFSET: usize = 50;
pub(crate) const PPC_CLASSIC_ALIAS_DIR_ID_OFFSET: usize = 114;
pub(crate) const PPC_CLASSIC_ALIAS_MYSTERY_WORDS_OFFSET: usize = 130;
pub(crate) const PPC_CLASSIC_ALIAS_TAIL_TAG: u16 = 0x0009;
pub(crate) const PPC_CLASSIC_ALIAS_FULL_PATH_TAG: u16 = 0x0002;
pub(crate) const PPC_CLASSIC_ALIAS_END_TAG: u16 = 0xffff;
pub(crate) const PPC_CLASSIC_ALIAS_TAIL_PAYLOAD_SIZE: usize = 168;
pub(crate) const PPC_CLASSIC_ALIAS_RECORD_SIZE: usize =
    PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE + 4 + PPC_CLASSIC_ALIAS_TAIL_PAYLOAD_SIZE + 4;

pub(super) fn ppc_fs_make_fsspec(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    default_dir_id: u32,
) -> i16 {
    let requested_vref = cpu.gpr[3] as u16 as i16;
    let requested_dir_id = cpu.gpr[4];
    let name_ptr = cpu.gpr[5];
    let spec_ptr = cpu.gpr[6];
    if spec_ptr == 0 {
        return PPC_PARAM_ERR;
    }

    let effective_vref = ppc_resolve_volume_ref_num(requested_vref);
    let effective_dir_id =
        ppc_resolve_directory_id(requested_vref, requested_dir_id, default_dir_id);
    let name_bytes = if name_ptr == 0 {
        Vec::new()
    } else {
        let Some(bytes) = ppc_read_pstring_bytes(memory, name_ptr) else {
            return PPC_PARAM_ERR;
        };
        bytes
    };

    let mut output_dir_id = effective_dir_id;
    let mut output_name_bytes = name_bytes.clone();
    let result = if !name_bytes.is_empty()
        && effective_dir_id > 1
        && ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none()
    {
        PPC_DIR_NF_ERR
    } else if let Some(target_path) = ppc_resolved_fsspec_target_path(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        effective_dir_id,
        &name_bytes,
    ) {
        if !name_bytes.is_empty() {
            output_dir_id = ppc_parent_dir_id_for_path(vfs_directories, &target_path);
            output_name_bytes = ppc_vfs_basename_bytes(&target_path);
        }
        PPC_NO_ERR
    } else {
        PPC_FNF_ERR
    };
    if ppc_write_fsspec(
        memory,
        spec_ptr,
        effective_vref,
        output_dir_id,
        &output_name_bytes,
    )
    .is_none()
    {
        return PPC_PARAM_ERR;
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] FSMakeFSSpec requested_vref={} requested_dir={} effective_vref={} effective_dir={} name=\"{}\" result={}",
            requested_vref,
            requested_dir_id,
            effective_vref,
            effective_dir_id,
            decode_mac_roman(&name_bytes),
            result
        );
    }
    result
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcCatalogEntry {
    pub(crate) path: String,
    pub(crate) name: Vec<u8>,
    pub(crate) is_directory: bool,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_pb_get_cat_info(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_volumes: &[PpcVfsVolumeRecord],
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    default_dir_id: u32,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, pb + 16, 2)
        || !ppc_memory_can_write_bytes(memory, pb + 22, 2)
    {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return PPC_PARAM_ERR;
    };
    let Some(vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(fdir_index) = memory.read_u16_be(pb + 28).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(requested_dir_id) = memory.read_u32_be(pb + 48) else {
        return PPC_PARAM_ERR;
    };
    let effective_dir_id = ppc_resolve_directory_id(vref, requested_dir_id, default_dir_id);
    // A negative index selects the directory by ID. ioNamePtr is output-only,
    // including when its buffer does not yet contain a valid Pascal string.
    // Inside Macintosh: Files (1992), pp. 2-191 and 3-36.
    let name_bytes = if fdir_index < 0 || name_ptr == 0 {
        Vec::new()
    } else {
        match ppc_read_pstring_bytes(memory, name_ptr) {
            Some(bytes) => bytes,
            None => return ppc_complete_pb(memory, pb, PPC_PARAM_ERR),
        }
    };
    let requested_name = decode_mac_roman(&name_bytes);
    let absolute_pathname = !requested_name.starts_with(':') && requested_name.contains(':');
    let pathname_volume = (!requested_name.starts_with(':'))
        .then(|| requested_name.split_once(':').map(|(volume, _)| volume))
        .flatten()
        .and_then(|name| {
            vfs_volumes
                .iter()
                .find(|volume| volume.name.eq_ignore_ascii_case(name))
        });
    let resolved_vref =
        pathname_volume.map_or_else(|| ppc_resolve_volume_ref_num(vref), |volume| volume.ref_num);
    if !absolute_pathname
        && effective_dir_id > 1
        && ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none()
    {
        return ppc_complete_pb(memory, pb, PPC_DIR_NF_ERR);
    }

    let Some(entry) = ppc_catalog_entry_for_lookup(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        effective_dir_id,
        &name_bytes,
        fdir_index,
    ) else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] PBGetCatInfo dir={} index={} name=\"{}\" -> {}",
                effective_dir_id,
                fdir_index,
                decode_mac_roman(&name_bytes),
                PPC_FNF_ERR
            );
        }
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };

    let filled = if entry.is_directory {
        vfs_directories
            .iter()
            .find(|directory| directory.path.eq_ignore_ascii_case(&entry.path))
            .and_then(|directory| {
                ppc_fill_directory_catalog_info(
                    memory,
                    pb,
                    directory,
                    vfs_directories,
                    vfs_files,
                    vfs_resource_files,
                )
            })
    } else {
        ppc_fill_file_catalog_info(
            memory,
            pb,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            vfs_resources,
            &entry.path,
        )
    };
    if filled.is_none() {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    if name_ptr != 0 && !ppc_write_pstring_bytes(memory, name_ptr, &entry.name) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    if memory.write_u16_be(pb + 22, resolved_vref as u16).is_none() {
        return PPC_PARAM_ERR;
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] PBGetCatInfo dir={} index={} name=\"{}\" -> 0 out=\"{}\"",
            effective_dir_id,
            fdir_index,
            decode_mac_roman(&name_bytes),
            decode_mac_roman(&entry.name)
        );
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_set_cat_info(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &mut [PpcVfsDirectory],
    vfs_files: &mut [PpcVfsFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    default_dir_id: u32,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return PPC_PARAM_ERR;
    };
    let Some(vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(fdir_index) = memory.read_u16_be(pb + 28).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(requested_dir_id) = memory.read_u32_be(pb + 48) else {
        return PPC_PARAM_ERR;
    };
    let effective_dir_id = ppc_resolve_directory_id(vref, requested_dir_id, default_dir_id);
    let name_bytes = if name_ptr == 0 {
        Vec::new()
    } else {
        match ppc_read_pstring_bytes(memory, name_ptr) {
            Some(bytes) => bytes,
            None => return ppc_complete_pb(memory, pb, PPC_PARAM_ERR),
        }
    };
    if effective_dir_id > 1
        && ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none()
    {
        return ppc_complete_pb(memory, pb, PPC_DIR_NF_ERR);
    }
    let Some((file_type, creator, finder_flags)) = ppc_read_finfo(memory, pb + 32) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(entry) = ppc_catalog_entry_for_lookup(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        effective_dir_id,
        &name_bytes,
        fdir_index,
    ) else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    if entry.is_directory {
        let Some(directory) = vfs_directories
            .iter_mut()
            .find(|directory| directory.path.eq_ignore_ascii_case(&entry.path))
        else {
            return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
        };
        directory.file_type = file_type;
        directory.creator = creator;
        directory.finder_flags = finder_flags;
        directory.dirty = true;
    } else {
        let mut found = false;
        for file in vfs_files
            .iter_mut()
            .filter(|record| record.path.eq_ignore_ascii_case(&entry.path))
        {
            file.file_type = file_type;
            file.creator = creator;
            file.finder_flags = finder_flags;
            file.dirty = true;
            found = true;
        }
        for file in vfs_resource_files
            .iter_mut()
            .filter(|record| record.path.eq_ignore_ascii_case(&entry.path))
        {
            file.file_type = file_type;
            file.creator = creator;
            file.finder_flags = finder_flags;
            file.dirty = true;
            found = true;
        }
        if !found {
            return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
        }
    }
    let resolved_vref = ppc_resolve_volume_ref_num(vref);
    if memory.write_u16_be(pb + 22, resolved_vref as u16).is_none() {
        return PPC_PARAM_ERR;
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_get_finfo(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    default_dir_id: u32,
    hfs: bool,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, pb + 16, 2)
        || !ppc_memory_can_write_bytes(memory, pb + 22, 2)
    {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return PPC_PARAM_ERR;
    };
    let Some(vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(fdir_index) = memory.read_u16_be(pb + 28).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let requested_dir_id = if hfs {
        let Some(dir_id) = memory.read_u32_be(pb + 48) else {
            return PPC_PARAM_ERR;
        };
        dir_id
    } else {
        0
    };
    let resolved_vref = ppc_resolve_volume_ref_num(vref);
    let effective_dir_id = ppc_resolve_directory_id(vref, requested_dir_id, default_dir_id);
    let name_bytes = if name_ptr == 0 {
        Vec::new()
    } else {
        match ppc_read_pstring_bytes(memory, name_ptr) {
            Some(bytes) => bytes,
            None => return ppc_complete_pb(memory, pb, PPC_PARAM_ERR),
        }
    };
    if effective_dir_id > 1
        && ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none()
    {
        return ppc_complete_pb(memory, pb, PPC_DIR_NF_ERR);
    }
    let Some(path) = ppc_pb_finfo_path_for_lookup(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        effective_dir_id,
        &name_bytes,
        fdir_index,
    ) else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    if ppc_fill_file_finfo_pb(
        memory,
        pb,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        vfs_resources,
        &path,
        hfs,
    )
    .is_none()
    {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    if name_ptr != 0 && !ppc_write_pstring_bytes(memory, name_ptr, &ppc_vfs_basename_bytes(&path)) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    if memory.write_u16_be(pb + 22, resolved_vref as u16).is_none() {
        return PPC_PARAM_ERR;
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_set_finfo(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut [PpcVfsFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    default_dir_id: u32,
    hfs: bool,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, pb + 16, 2)
        || !ppc_memory_can_write_bytes(memory, pb + 22, 2)
    {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return PPC_PARAM_ERR;
    };
    let Some(vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(fdir_index) = memory.read_u16_be(pb + 28).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let requested_dir_id = if hfs {
        let Some(dir_id) = memory.read_u32_be(pb + 48) else {
            return PPC_PARAM_ERR;
        };
        dir_id
    } else {
        0
    };
    let resolved_vref = ppc_resolve_volume_ref_num(vref);
    let effective_dir_id = ppc_resolve_directory_id(vref, requested_dir_id, default_dir_id);
    let name_bytes = if name_ptr == 0 {
        Vec::new()
    } else {
        match ppc_read_pstring_bytes(memory, name_ptr) {
            Some(bytes) => bytes,
            None => return ppc_complete_pb(memory, pb, PPC_PARAM_ERR),
        }
    };
    if effective_dir_id > 1
        && ppc_directory_path_for_id(vfs_directories, effective_dir_id).is_none()
    {
        return ppc_complete_pb(memory, pb, PPC_DIR_NF_ERR);
    }
    let Some((file_type, creator, finder_flags)) = ppc_read_finfo(memory, pb + 32) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(path) = ppc_pb_finfo_path_for_lookup(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        effective_dir_id,
        &name_bytes,
        fdir_index,
    ) else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    if !ppc_set_finfo_for_path(
        vfs_files,
        vfs_resource_files,
        &path,
        file_type,
        creator,
        finder_flags,
    ) {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    }
    if memory.write_u16_be(pb + 22, resolved_vref as u16).is_none() {
        return PPC_PARAM_ERR;
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_finfo_path_for_lookup(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    dir_id: u32,
    name_bytes: &[u8],
    fdir_index: i16,
) -> Option<String> {
    if fdir_index > 0 {
        let entry = ppc_catalog_child_by_index(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            dir_id,
            fdir_index as usize,
        )?;
        return (!entry.is_directory).then_some(entry.path);
    }
    if name_bytes.is_empty() {
        return None;
    }
    let normalized_name = ppc_normalize_vfs_path(&decode_mac_roman(name_bytes));
    if normalized_name.is_empty() {
        return None;
    }
    if let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) {
        let candidate = ppc_join_vfs_path(parent_path, &normalized_name);
        if let Some(path) = ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &candidate)
        {
            return Some(path);
        }
    }
    if normalized_name.contains('/') {
        if let Some(path) =
            ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &normalized_name)
        {
            return Some(path);
        }
    }
    ppc_vfs_file_or_resource_path_by_basename(vfs_files, vfs_resource_files, &normalized_name)
}

pub(super) fn ppc_vfs_file_or_resource_path(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> Option<String> {
    vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
        .map(|file| file.path.clone())
        .or_else(|| {
            vfs_resource_files
                .iter()
                .find(|file| file.path.eq_ignore_ascii_case(path))
                .map(|file| file.path.clone())
        })
}

pub(super) fn ppc_vfs_file_or_resource_path_by_basename(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    name: &str,
) -> Option<String> {
    let basename = name.rsplit('/').next().unwrap_or(name);
    let mut data_paths = vfs_files.iter().map(|file| &file.path).collect::<Vec<_>>();
    data_paths.sort_by_key(|path| path.to_ascii_lowercase());
    if let Some(path) = data_paths
        .into_iter()
        .find(|path| ppc_vfs_basename(path).eq_ignore_ascii_case(basename))
    {
        return Some(path.clone());
    }
    let mut resource_paths = vfs_resource_files
        .iter()
        .map(|file| &file.path)
        .collect::<Vec<_>>();
    resource_paths.sort_by_key(|path| path.to_ascii_lowercase());
    resource_paths
        .into_iter()
        .find(|path| ppc_vfs_basename(path).eq_ignore_ascii_case(basename))
        .cloned()
}

pub(super) fn ppc_vfs_basename(path: &str) -> &str {
    path.rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

pub(super) fn ppc_fill_file_finfo_pb(
    memory: &mut PpcSectionMem,
    pb: u32,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    path: &str,
    include_parent_dir_id: bool,
) -> Option<()> {
    let data_len = vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
        .map_or(0, |file| u32::try_from(file.data.len()).unwrap_or(u32::MAX));
    let resource_len = ppc_resource_fork_len_for_path(vfs_resource_files, vfs_resources, path);
    let (file_type, creator, finder_flags) =
        ppc_finder_info_for_path(vfs_files, vfs_resource_files, path);
    memory.write_u8(pb + 30, 0)?;
    memory.write_u8(pb + 31, 0)?;
    ppc_write_finfo(memory, pb + 32, file_type, creator, finder_flags)?;
    memory.write_u32_be(pb + 48, ppc_synthetic_file_id(path))?;
    memory.write_u32_be(pb + 54, data_len)?;
    memory.write_u32_be(pb + 58, data_len)?;
    memory.write_u32_be(pb + 64, resource_len)?;
    memory.write_u32_be(pb + 68, resource_len)?;
    memory.write_u32_be(pb + 72, 0)?;
    memory.write_u32_be(pb + 76, 0)?;
    if include_parent_dir_id {
        memory.write_u32_be(pb + 100, ppc_parent_dir_id_for_path(vfs_directories, path))?;
    }
    Some(())
}

pub(super) fn ppc_set_finfo_for_path(
    vfs_files: &mut [PpcVfsFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    path: &str,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
) -> bool {
    let mut found = false;
    for file in vfs_files
        .iter_mut()
        .filter(|record| record.path.eq_ignore_ascii_case(path))
    {
        file.file_type = file_type;
        file.creator = creator;
        file.finder_flags = finder_flags;
        file.dirty = true;
        found = true;
    }
    for file in vfs_resource_files
        .iter_mut()
        .filter(|record| record.path.eq_ignore_ascii_case(path))
    {
        file.file_type = file_type;
        file.creator = creator;
        file.finder_flags = finder_flags;
        file.dirty = true;
        found = true;
    }
    found
}

pub(super) fn ppc_pbh_get_v_info(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_volumes: &[PpcVfsVolumeRecord],
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 122) {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return PPC_PARAM_ERR;
    };
    let Some(vref_num) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(volume_index) = memory.read_u16_be(pb + 28).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let requested_name = if name_ptr == 0 {
        String::new()
    } else {
        ppc_read_pstring_bytes(memory, name_ptr)
            .map(|name| decode_mac_roman(&name))
            .unwrap_or_default()
    };
    let relative_pathname = requested_name.starts_with(':');
    let absolute_pathname = !relative_pathname && requested_name.contains(':');

    // Inside Macintosh: Files (1992), pp. 2-144--2-146: a positive
    // ioVolIndex enumerates the VCB queue, zero selects by name or reference,
    // and a negative index uses the standard name/reference search order.
    let boot_volume = || PpcVfsVolumeRecord {
        ref_num: PPC_BOOT_VOLUME_REF_NUM,
        name: crate::trap::TrapDispatcher::boot_volume_name().to_string(),
        root_dir_id: PPC_ROOT_DIR_ID,
        attributes: 0,
        file_count: 0,
        allocation_block_count: u16::MAX,
        allocation_block_size: 4096,
        clump_size: 4096,
        free_blocks: 0x8000,
        bitmap_start: 3,
        allocation_pointer: 4,
        allocation_start: 3,
        next_catalog_id: 1024,
        created_date: 0,
        modified_date: 0,
    };
    let volume_by_name = |name: &str| {
        let volume_name = name.split(':').next().unwrap_or(name);
        if volume_name.eq_ignore_ascii_case(crate::trap::TrapDispatcher::boot_volume_name()) {
            Some(boot_volume())
        } else {
            vfs_volumes
                .iter()
                .find(|volume| volume.name.eq_ignore_ascii_case(volume_name))
                .cloned()
        }
    };
    let volume_by_ref = |requested_ref: i16| {
        if requested_ref == PPC_BOOT_VOLUME_REF_NUM {
            Some(boot_volume())
        } else {
            vfs_volumes
                .iter()
                .find(|volume| volume.ref_num == requested_ref)
                .cloned()
        }
    };
    let selected = if volume_index == 0 {
        if vref_num != 0 {
            volume_by_ref(vref_num)
        } else if relative_pathname {
            Some(boot_volume())
        } else if !requested_name.is_empty() {
            volume_by_name(&requested_name)
        } else {
            Some(boot_volume())
        }
    } else if volume_index > 0 {
        if volume_index == 1 {
            Some(boot_volume())
        } else {
            vfs_volumes.get((volume_index - 2) as usize).cloned()
        }
    } else if absolute_pathname {
        volume_by_name(&requested_name)
    } else if vref_num != 0 {
        volume_by_ref(vref_num)
    } else if relative_pathname {
        Some(boot_volume())
    } else if !requested_name.is_empty() {
        volume_by_name(&requested_name)
    } else {
        Some(boot_volume())
    };
    let Some(volume) = selected else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] PBHGetVInfo index={volume_index} vref={vref_num} name={requested_name:?} -> nsvErr"
            );
        }
        return ppc_complete_pb(memory, pb, PPC_NSV_ERR);
    };

    let volume_name = encode_mac_roman_lossy(&volume.name);
    if name_ptr != 0 && !ppc_optional_pstring_output_can_write(memory, name_ptr, &volume_name) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    if name_ptr != 0 {
        let _ = ppc_write_pstring_bytes(memory, name_ptr, &volume_name);
    }

    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] PBHGetVInfo index={volume_index} vref={vref_num} name={requested_name:?} -> {:?} ({})",
            volume.name, volume.ref_num
        );
    }

    // Inside Macintosh: Files (1992), p. 2-238: HVolumeParam layout.
    let writes = [
        memory.write_u16_be(pb + 22, volume.ref_num as u16),
        memory.write_u32_be(pb + 30, volume.created_date),
        memory.write_u32_be(pb + 34, volume.modified_date),
        memory.write_u16_be(pb + 38, volume.attributes),
        memory.write_u16_be(pb + 40, volume.file_count),
        memory.write_u16_be(pb + 42, volume.bitmap_start),
        memory.write_u16_be(pb + 44, volume.allocation_pointer),
        memory.write_u16_be(pb + 46, volume.allocation_block_count),
        memory.write_u32_be(pb + 48, volume.allocation_block_size),
        memory.write_u32_be(pb + 52, volume.clump_size),
        memory.write_u16_be(pb + 56, volume.allocation_start),
        memory.write_u32_be(pb + 58, volume.next_catalog_id),
        memory.write_u16_be(pb + 62, volume.free_blocks),
        memory.write_u16_be(pb + 64, 0x4244),
        memory.write_u16_be(pb + 66, 1),
        memory.write_u16_be(pb + 68, (-33i16) as u16),
        memory.write_u16_be(pb + 70, 0),
        memory.write_u32_be(pb + 72, 0),
        memory.write_u16_be(pb + 76, 0),
        memory.write_u32_be(pb + 78, 0),
        memory.write_u32_be(pb + 82, 0),
        memory.write_u32_be(pb + 86, 0),
        memory.write_u32_be(pb + 90, PPC_SYSTEM_FOLDER_DIR_ID),
    ];
    if writes.iter().any(Option::is_none) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_complete_pb(memory: &mut PpcSectionMem, pb: u32, err: i16) -> i16 {
    if memory.write_u16_be(pb + 16, err as u16).is_none() {
        PPC_PARAM_ERR
    } else {
        err
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_pb_get_fcb_info(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &[PpcFileRecord],
    resource_files: &[PpcResourceFileRecord],
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    launched_app_path: Option<&str>,
) -> i16 {
    // Inside Macintosh: Files (1992), pp. 2-237--2-238: FCBPBRec selects an
    // open fork by ioRefNum when ioFCBIndx is zero, or enumerates open FCBs
    // when ioFCBIndx is positive.
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 62) {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return PPC_PARAM_ERR;
    };
    let Some(vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(requested_ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(fcb_index) = memory.read_u16_be(pb + 28).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    if fcb_index < 0 || (name_ptr != 0 && !ppc_memory_can_write_bytes(memory, name_ptr, 32)) {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    // Inside Macintosh: Files (1992), pp. 2-218--2-219: ioVRefNum limits
    // indexed FCB enumeration, but ioRefNum alone selects the open file when
    // ioFCBIndx is zero. Real applications consequently leave ioVRefNum
    // uninitialized for the latter form.
    if fcb_index > 0 && !matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        return ppc_complete_pb(memory, pb, PPC_NSV_ERR);
    }

    let selected = if fcb_index == 0 {
        if requested_ref_num == 0 {
            launched_app_path.map(|path| (0, path.to_string(), true, 0))
        } else if let Some(file) = files.iter().find(|file| file.ref_num == requested_ref_num) {
            Some((file.ref_num, file.path.clone(), false, file.position))
        } else {
            resource_files
                .iter()
                .find(|file| file.ref_num == requested_ref_num)
                .map(|file| (file.ref_num, file.path.clone(), true, 0))
        }
    } else {
        let mut open_forks = Vec::with_capacity(
            files.len() + resource_files.len() + usize::from(launched_app_path.is_some()),
        );
        if let Some(path) = launched_app_path {
            open_forks.push((0, path.to_string(), true, 0));
        }
        open_forks.extend(
            files
                .iter()
                .map(|file| (file.ref_num, file.path.clone(), false, file.position)),
        );
        open_forks.extend(
            resource_files
                .iter()
                .map(|file| (file.ref_num, file.path.clone(), true, 0)),
        );
        open_forks.into_iter().nth(fcb_index as usize - 1)
    };
    let Some((ref_num, path, is_resource_fork, position)) = selected else {
        return ppc_complete_pb(memory, pb, PPC_FN_OPN_ERR);
    };

    let logical_eof = if is_resource_fork {
        ppc_resource_fork_len_for_path(vfs_resource_files, vfs_resources, &path)
    } else {
        vfs_files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(&path))
            .map(|file| u32::try_from(file.data.len()).unwrap_or(u32::MAX))
            .unwrap_or(0)
    };
    let mut name = ppc_vfs_basename_bytes(&path);
    name.truncate(31);
    let flags = if is_resource_fork { 0x0200 } else { 0x0100 };
    let writes = [
        memory.write_u16_be(pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16),
        memory.write_u16_be(pb + 24, ref_num as u16),
        memory.write_u16_be(pb + 30, 0),
        memory.write_u32_be(pb + 32, ppc_synthetic_file_id(&path)),
        memory.write_u16_be(pb + 36, flags),
        memory.write_u16_be(pb + 38, 0),
        memory.write_u32_be(pb + 40, logical_eof),
        memory.write_u32_be(pb + 44, logical_eof),
        memory.write_u32_be(pb + 48, position.min(logical_eof)),
        memory.write_u16_be(pb + 52, PPC_BOOT_VOLUME_REF_NUM as u16),
        memory.write_u32_be(pb + 54, 0),
        memory.write_u32_be(pb + 58, ppc_parent_dir_id_for_path(vfs_directories, &path)),
    ];
    if writes.iter().any(Option::is_none)
        || (name_ptr != 0 && !ppc_write_pstring_bytes(memory, name_ptr, &name))
    {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] PBGetFCBInfo ref={} index={} -> ref={} path=\"{}\" resource={} eof={} position={}",
            requested_ref_num, fcb_index, ref_num, path, is_resource_fork, logical_eof, position
        );
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_catalog_entry_for_lookup(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    dir_id: u32,
    name_bytes: &[u8],
    fdir_index: i16,
) -> Option<PpcCatalogEntry> {
    if fdir_index > 0 {
        return ppc_catalog_child_by_index(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            dir_id,
            fdir_index as usize,
        );
    }
    if fdir_index < 0 {
        let directory = vfs_directories
            .iter()
            .find(|directory| directory.dir_id == dir_id)?;
        return Some(PpcCatalogEntry {
            path: directory.path.clone(),
            name: ppc_vfs_basename_bytes(&directory.path),
            is_directory: true,
        });
    }
    if name_bytes.is_empty() {
        let directory = vfs_directories
            .iter()
            .find(|directory| directory.dir_id == dir_id)?;
        return Some(PpcCatalogEntry {
            path: directory.path.clone(),
            name: ppc_vfs_basename_bytes(&directory.path),
            is_directory: true,
        });
    }

    let decoded_name = decode_mac_roman(name_bytes);
    let normalized_name = ppc_normalize_vfs_path(&decoded_name);
    if normalized_name.is_empty() {
        return None;
    }
    // Inside Macintosh: Files (1992), pp. 2-6--2-7: a pathname beginning
    // with a volume name is absolute, while a leading colon is relative.
    let absolute = !decoded_name.starts_with(':') && decoded_name.contains(':');
    let path = if absolute {
        // The boot volume name is part of an HFS absolute pathname, but VFS
        // records store paths relative to the volume root.
        if let Some((volume, relative)) = decoded_name.split_once(':') {
            if volume.eq_ignore_ascii_case(crate::trap::TrapDispatcher::boot_volume_name()) {
                ppc_normalize_vfs_path(relative)
            } else {
                normalized_name
            }
        } else {
            normalized_name
        }
    } else {
        let parent_path = ppc_directory_path_for_id(vfs_directories, dir_id)?;
        ppc_join_vfs_path(parent_path, &normalized_name)
    };
    if ppc_directory_id_for_path(vfs_directories, &path).is_some() {
        return Some(PpcCatalogEntry {
            name: ppc_vfs_basename_bytes(&path),
            path,
            is_directory: true,
        });
    }
    if ppc_vfs_file_index(vfs_files, &path).is_some()
        || ppc_vfs_resource_file_index(vfs_resource_files, &path).is_some()
    {
        return Some(PpcCatalogEntry {
            name: ppc_vfs_basename_bytes(&path),
            path,
            is_directory: false,
        });
    }
    None
}

pub(super) fn ppc_catalog_child_by_index(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    dir_id: u32,
    fdir_index: usize,
) -> Option<PpcCatalogEntry> {
    let parent_path = ppc_directory_path_for_id(vfs_directories, dir_id)?;
    let mut entries = Vec::new();
    for directory in vfs_directories
        .iter()
        .filter(|directory| directory.parent_dir_id == dir_id)
    {
        entries.push(PpcCatalogEntry {
            path: directory.path.clone(),
            name: ppc_vfs_basename_bytes(&directory.path),
            is_directory: true,
        });
    }
    for file in vfs_files {
        if let Some(name) = ppc_child_name_for_parent(parent_path, &file.path) {
            entries.push(PpcCatalogEntry {
                path: file.path.clone(),
                name: name.as_bytes().to_vec(),
                is_directory: false,
            });
        }
    }
    for fork in vfs_resource_files {
        if vfs_files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case(&fork.path))
        {
            continue;
        }
        if let Some(name) = ppc_child_name_for_parent(parent_path, &fork.path) {
            entries.push(PpcCatalogEntry {
                path: fork.path.clone(),
                name: name.as_bytes().to_vec(),
                is_directory: false,
            });
        }
    }
    entries.sort_by(|left, right| {
        let left_key = (
            left.name.to_ascii_lowercase(),
            u8::from(!left.is_directory),
            left.path.to_ascii_lowercase(),
        );
        let right_key = (
            right.name.to_ascii_lowercase(),
            u8::from(!right.is_directory),
            right.path.to_ascii_lowercase(),
        );
        left_key.cmp(&right_key)
    });
    entries.into_iter().nth(fdir_index.saturating_sub(1))
}

pub(super) fn ppc_child_name_for_parent<'a>(parent_path: &str, path: &'a str) -> Option<&'a str> {
    if parent_path.is_empty() {
        return if !path.is_empty() && !path.contains('/') {
            Some(path)
        } else {
            None
        };
    }
    let remainder = path.strip_prefix(parent_path)?.strip_prefix('/')?;
    if remainder.is_empty() || remainder.contains('/') {
        None
    } else {
        Some(remainder)
    }
}

pub(super) fn ppc_vfs_basename_bytes(path: &str) -> Vec<u8> {
    let basename = path
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path);
    encode_mac_roman_lossy(basename)
}

pub(super) fn ppc_fill_file_catalog_info(
    memory: &mut PpcSectionMem,
    pb: u32,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    path: &str,
) -> Option<()> {
    let data_len = vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
        .map_or(0, |file| u32::try_from(file.data.len()).unwrap_or(u32::MAX));
    let resource_len = ppc_resource_fork_len_for_path(vfs_resource_files, vfs_resources, path);
    let (file_type, creator, finder_flags) =
        ppc_finder_info_for_path(vfs_files, vfs_resource_files, path);
    memory.write_u8(pb + 30, 0)?;
    memory.write_u8(pb + 31, 0)?;
    ppc_write_finfo(memory, pb + 32, file_type, creator, finder_flags)?;
    memory.write_u32_be(pb + 48, ppc_synthetic_file_id(path))?;
    memory.write_u32_be(pb + 54, data_len)?;
    memory.write_u32_be(pb + 58, data_len)?;
    memory.write_u32_be(pb + 64, resource_len)?;
    memory.write_u32_be(pb + 68, resource_len)?;
    memory.write_u32_be(pb + 72, 0)?;
    memory.write_u32_be(pb + 76, 0)?;
    memory.write_u32_be(pb + 100, ppc_parent_dir_id_for_path(vfs_directories, path))?;
    Some(())
}

pub(super) fn ppc_resource_fork_len_for_path(
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    path: &str,
) -> u32 {
    let resource_file = vfs_resource_files
        .iter()
        .find(|fork| fork.path.eq_ignore_ascii_case(path));
    if let Some(raw_data) = resource_file.and_then(|fork| fork.raw_data.as_ref()) {
        return u32::try_from(raw_data.len()).unwrap_or(u32::MAX);
    }
    let entries = vfs_resources
        .iter()
        .filter(|resource| resource.path.eq_ignore_ascii_case(path))
        .map(|resource| {
            let (data, attrs) = match (&resource.raw_data, resource.raw_attrs) {
                (Some(raw_data), Some(raw_attrs)) => (raw_data.clone(), raw_attrs),
                _ => (resource.data.clone(), resource.attrs),
            };
            ResourceForkEntry {
                res_type: resource.res_type.to_be_bytes(),
                id: resource.res_id,
                name: resource.name.clone(),
                data,
                attrs: (attrs & 0x00ff) as u8,
            }
        })
        .collect::<Vec<_>>();
    if !entries.is_empty() {
        return serialize_resource_fork_with_attrs(
            &entries,
            resource_file.map_or(0, |fork| fork.map_attrs),
        )
        .map(|bytes| u32::try_from(bytes.len()).unwrap_or(u32::MAX))
        .unwrap_or(0);
    }
    resource_file.map_or(0, |fork| fork.resource_len)
}

pub(super) fn ppc_fill_directory_catalog_info(
    memory: &mut PpcSectionMem,
    pb: u32,
    directory: &PpcVfsDirectory,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
) -> Option<()> {
    memory.write_u8(pb + 30, 0x10)?;
    memory.write_u8(pb + 31, 0)?;
    ppc_write_finfo(
        memory,
        pb + 32,
        directory.file_type,
        directory.creator,
        directory.finder_flags,
    )?;
    memory.write_u32_be(pb + 48, directory.dir_id)?;
    let mut entry_count = 0usize;
    while ppc_catalog_child_by_index(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        directory.dir_id,
        entry_count.saturating_add(1),
    )
    .is_some()
    {
        entry_count = entry_count.saturating_add(1);
    }
    memory.write_u16_be(pb + 52, entry_count.min(u16::MAX as usize) as u16)?;
    memory.write_u32_be(pb + 72, 0)?;
    memory.write_u32_be(pb + 76, 0)?;
    memory.write_u32_be(pb + 100, directory.parent_dir_id)?;
    Some(())
}

pub(super) fn ppc_finder_info_for_path(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> (u32, u32, u16) {
    if let Some(file) = vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
    {
        return (file.file_type, file.creator, file.finder_flags);
    }
    if let Some(fork) = vfs_resource_files
        .iter()
        .find(|fork| fork.path.eq_ignore_ascii_case(path))
    {
        return (fork.file_type, fork.creator, fork.finder_flags);
    }
    (0, 0, 0)
}

pub(super) fn ppc_synthetic_file_id(path: &str) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in path.bytes().map(|byte| byte.to_ascii_lowercase()) {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    0x1000_0000 | (hash & 0x0fff_ffff)
}

pub(super) fn ppc_parent_dir_id_for_path(vfs_directories: &[PpcVfsDirectory], path: &str) -> u32 {
    let parent_path = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    ppc_directory_id_for_path(vfs_directories, parent_path).unwrap_or(PPC_ROOT_DIR_ID)
}

pub(super) fn ppc_get_finfo(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    default_dir_id: u32,
) -> i16 {
    let file_name_ptr = cpu.gpr[3];
    let vref_num = cpu.gpr[4] as u16 as i16;
    let finfo_ptr = cpu.gpr[5];
    if !matches!(vref_num, 0 | PPC_BOOT_VOLUME_REF_NUM)
        || finfo_ptr == 0
        || !ppc_memory_can_write_bytes(memory, finfo_ptr, 16)
    {
        return PPC_PARAM_ERR;
    }
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, file_name_ptr) else {
        return PPC_PARAM_ERR;
    };
    let path = ppc_resolved_fsspec_target_path(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        default_dir_id,
        &name_bytes,
    )
    .or_else(|| {
        ppc_resolved_fsspec_target_path(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            PPC_ROOT_DIR_ID,
            &name_bytes,
        )
    });
    let Some(path) = path else {
        let _ = ppc_write_finfo(memory, finfo_ptr, 0, 0, 0);
        return PPC_FNF_ERR;
    };
    let (file_type, creator, finder_flags) =
        ppc_finder_info_for_path(vfs_files, vfs_resource_files, &path);
    // Inside Macintosh, Volume II (1985), File Manager p. II-95: GetFInfo
    // takes a Str255 name and volume reference and returns the 16-byte FInfo.
    if ppc_write_finfo(memory, finfo_ptr, file_type, creator, finder_flags).is_none() {
        PPC_PARAM_ERR
    } else {
        PPC_NO_ERR
    }
}

pub(super) fn ppc_h_finfo_path(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vref_num: i16,
    requested_dir_id: u32,
    file_name_ptr: u32,
    default_dir_id: u32,
) -> Result<String, i16> {
    if !matches!(vref_num, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        return Err(PPC_NSV_ERR);
    }
    let name_bytes = ppc_read_pstring_bytes(memory, file_name_ptr).ok_or(PPC_PARAM_ERR)?;
    let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if name.is_empty() || name.contains('/') {
        return Err(PPC_BD_NAM_ERR);
    }
    let dir_id = ppc_resolve_directory_id(vref_num, requested_dir_id, default_dir_id);
    let parent = ppc_directory_path_for_id(vfs_directories, dir_id).ok_or(PPC_DIR_NF_ERR)?;
    let requested_path = ppc_join_vfs_path(parent, &name);
    ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &requested_path).ok_or(PPC_FNF_ERR)
}

pub(super) fn ppc_h_get_finfo(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    default_dir_id: u32,
) -> i16 {
    let finfo_ptr = cpu.gpr[6];
    if finfo_ptr == 0 || !ppc_memory_can_write_bytes(memory, finfo_ptr, 16) {
        return PPC_PARAM_ERR;
    }
    let path = match ppc_h_finfo_path(
        memory,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        cpu.gpr[3] as u16 as i16,
        cpu.gpr[4],
        cpu.gpr[5],
        default_dir_id,
    ) {
        Ok(path) => path,
        Err(err) => return err,
    };
    let (file_type, creator, finder_flags) =
        ppc_finder_info_for_path(vfs_files, vfs_resource_files, &path);
    // Inside Macintosh: Files (1992), pp. 2-175--2-176: HGetFInfo uses the
    // explicit directory ID and returns the original 16-byte FInfo record.
    if ppc_write_finfo(memory, finfo_ptr, file_type, creator, finder_flags).is_some() {
        PPC_NO_ERR
    } else {
        PPC_PARAM_ERR
    }
}

pub(super) fn ppc_h_set_finfo(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut [PpcVfsFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    default_dir_id: u32,
) -> i16 {
    let finfo_ptr = cpu.gpr[6];
    let Some((file_type, creator, finder_flags)) = ppc_read_finfo(memory, finfo_ptr) else {
        return PPC_PARAM_ERR;
    };
    let path = match ppc_h_finfo_path(
        memory,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        cpu.gpr[3] as u16 as i16,
        cpu.gpr[4],
        cpu.gpr[5],
        default_dir_id,
    ) {
        Ok(path) => path,
        Err(err) => return err,
    };
    if ppc_set_finfo_for_path(
        vfs_files,
        vfs_resource_files,
        &path,
        file_type,
        creator,
        finder_flags,
    ) {
        PPC_NO_ERR
    } else {
        PPC_FNF_ERR
    }
}

pub(super) fn ppc_fsp_get_finfo(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let finfo_ptr = cpu.gpr[4];
    if finfo_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => return err,
    };
    if let Some(directory) = vfs_directories
        .iter()
        .find(|directory| directory.path.eq_ignore_ascii_case(&path))
    {
        return if ppc_write_finfo(
            memory,
            finfo_ptr,
            directory.file_type,
            directory.creator,
            directory.finder_flags,
        )
        .is_some()
        {
            PPC_NO_ERR
        } else {
            PPC_PARAM_ERR
        };
    }
    if let Some(file) = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&path))
    {
        return if ppc_write_finfo(
            memory,
            finfo_ptr,
            file.file_type,
            file.creator,
            file.finder_flags,
        )
        .is_some()
        {
            PPC_NO_ERR
        } else {
            PPC_PARAM_ERR
        };
    }
    if let Some(file) = vfs_resource_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&path))
    {
        return if ppc_write_finfo(
            memory,
            finfo_ptr,
            file.file_type,
            file.creator,
            file.finder_flags,
        )
        .is_some()
        {
            PPC_NO_ERR
        } else {
            PPC_PARAM_ERR
        };
    }
    let _ = ppc_write_finfo(memory, finfo_ptr, 0, 0, 0);
    PPC_FNF_ERR
}

pub(super) fn ppc_fsp_set_finfo(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &mut [PpcVfsDirectory],
    vfs_files: &mut [PpcVfsFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let finfo_ptr = cpu.gpr[4];
    if finfo_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => return err,
    };
    let Some((file_type, creator, finder_flags)) = ppc_read_finfo(memory, finfo_ptr) else {
        return PPC_PARAM_ERR;
    };
    let mut found = false;
    for file in vfs_files
        .iter_mut()
        .filter(|record| record.path.eq_ignore_ascii_case(&path))
    {
        file.file_type = file_type;
        file.creator = creator;
        file.finder_flags = finder_flags;
        file.dirty = true;
        found = true;
    }
    for file in vfs_resource_files
        .iter_mut()
        .filter(|record| record.path.eq_ignore_ascii_case(&path))
    {
        file.file_type = file_type;
        file.creator = creator;
        file.finder_flags = finder_flags;
        file.dirty = true;
        found = true;
    }
    if found {
        PPC_NO_ERR
    } else if let Some(directory) = vfs_directories
        .iter_mut()
        .find(|directory| directory.path.eq_ignore_ascii_case(&path))
    {
        directory.file_type = file_type;
        directory.creator = creator;
        directory.finder_flags = finder_flags;
        directory.dirty = true;
        PPC_NO_ERR
    } else {
        PPC_FNF_ERR
    }
}

pub(super) fn ppc_write_finfo(
    memory: &mut PpcSectionMem,
    finfo_ptr: u32,
    file_type: u32,
    creator: u32,
    finder_flags: u16,
) -> Option<()> {
    memory.write_u32_be(finfo_ptr, file_type)?;
    memory.write_u32_be(finfo_ptr + 4, creator)?;
    memory.write_u16_be(finfo_ptr + 8, finder_flags)?;
    memory.write_u32_be(finfo_ptr + 10, 0)?;
    memory.write_u16_be(finfo_ptr + 14, 0)?;
    Some(())
}

pub(super) fn ppc_read_finfo(memory: &mut PpcSectionMem, finfo_ptr: u32) -> Option<(u32, u32, u16)> {
    Some((
        memory.read_u32_be(finfo_ptr)?,
        memory.read_u32_be(finfo_ptr + 4)?,
        memory.read_u16_be(finfo_ptr + 8)?,
    ))
}

pub(super) fn ppc_write_fsspec(
    memory: &mut PpcSectionMem,
    spec_ptr: u32,
    vref: i16,
    dir_id: u32,
    name_bytes: &[u8],
) -> Option<()> {
    memory.write_u16_be(spec_ptr, vref as u16)?;
    memory.write_u32_be(spec_ptr + 2, dir_id)?;
    let len = name_bytes.len().min(63);
    memory.write_u8(spec_ptr + 6, len as u8)?;
    for (offset, byte) in name_bytes.iter().take(len).enumerate() {
        memory.write_u8(spec_ptr + 7 + offset as u32, *byte)?;
    }
    Some(())
}

pub(super) fn ppc_fsspec_target_exists(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    dir_id: u32,
    name_bytes: &[u8],
) -> bool {
    ppc_resolved_fsspec_target_path(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        dir_id,
        name_bytes,
    )
    .is_some()
}

pub(super) fn ppc_resolved_fsspec_target_path(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    dir_id: u32,
    name_bytes: &[u8],
) -> Option<String> {
    if name_bytes.is_empty() {
        return ppc_directory_path_for_id(vfs_directories, dir_id).map(str::to_string);
    }
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) else {
        return None;
    };
    let name = decode_mac_roman(name_bytes);
    let normalized_name = ppc_normalize_vfs_path(&name);
    if normalized_name.is_empty() {
        return None;
    }
    let path = ppc_join_vfs_path(parent_path, &normalized_name);
    if let Some(path) = ppc_vfs_target_path(vfs_directories, vfs_files, vfs_resource_files, &path) {
        return Some(path);
    }
    if normalized_name.contains('/') {
        if let Some(path) = ppc_vfs_target_path(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            &normalized_name,
        ) {
            return Some(path);
        }
    }
    if dir_id != PPC_ROOT_DIR_ID {
        return None;
    }
    ppc_vfs_target_path_by_suffix_or_unique_basename(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        &normalized_name,
    )
}

pub(super) fn ppc_path_for_fsspec(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    spec_ptr: u32,
) -> Result<String, i16> {
    if spec_ptr == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let Some(dir_id) = memory.read_u32_be(spec_ptr + 2) else {
        return Err(PPC_PARAM_ERR);
    };
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, spec_ptr + 6) else {
        return Err(PPC_PARAM_ERR);
    };
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) else {
        return Err(PPC_DIR_NF_ERR);
    };
    let name = decode_mac_roman(&name_bytes);
    let normalized_name = ppc_normalize_vfs_path(&name);
    if normalized_name.is_empty() {
        return Err(PPC_PARAM_ERR);
    }
    Ok(ppc_join_vfs_path(parent_path, &normalized_name))
}

pub(super) fn ppc_existing_path_for_fsspec(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    spec_ptr: u32,
) -> Result<String, i16> {
    if spec_ptr == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let Some(dir_id) = memory.read_u32_be(spec_ptr + 2) else {
        return Err(PPC_PARAM_ERR);
    };
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, spec_ptr + 6) else {
        return Err(PPC_PARAM_ERR);
    };
    if name_bytes.is_empty() {
        return ppc_directory_path_for_id(vfs_directories, dir_id)
            .map(str::to_string)
            .ok_or(PPC_DIR_NF_ERR);
    }
    let name = decode_mac_roman(&name_bytes);
    let normalized_name = ppc_normalize_vfs_path(&name);
    if normalized_name.is_empty() {
        return Err(PPC_PARAM_ERR);
    }
    let mut fallback_path = None;
    if let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) {
        let path = ppc_join_vfs_path(parent_path, &normalized_name);
        if ppc_directory_id_for_path(vfs_directories, &path).is_some() {
            return Ok(path);
        }
        if let Some(path) = ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &path) {
            return Ok(path);
        }
        fallback_path = Some(path);
    }
    if let Some(path) = ppc_vfs_file_or_resource_path_by_suffix_or_unique_basename(
        vfs_files,
        vfs_resource_files,
        &normalized_name,
    ) {
        return Ok(path);
    }
    fallback_path.ok_or(PPC_DIR_NF_ERR)
}

pub(super) fn ppc_existing_data_path_for_fsspec(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    spec_ptr: u32,
) -> Result<String, i16> {
    if spec_ptr == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let Some(dir_id) = memory.read_u32_be(spec_ptr + 2) else {
        return Err(PPC_PARAM_ERR);
    };
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, spec_ptr + 6) else {
        return Err(PPC_PARAM_ERR);
    };
    if name_bytes.is_empty() {
        return Err(PPC_PARAM_ERR);
    }
    let name = decode_mac_roman(&name_bytes);
    let normalized_name = ppc_normalize_vfs_path(&name);
    if normalized_name.is_empty() {
        return Err(PPC_PARAM_ERR);
    }
    if let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) {
        let path = ppc_join_vfs_path(parent_path, &normalized_name);
        if let Some(index) = ppc_vfs_file_index(vfs_files, &path) {
            if vfs_files[index].data.is_empty() {
                if let Some(non_empty_path) =
                    ppc_unique_non_empty_vfs_file_path_by_basename(vfs_files, &normalized_name)
                {
                    return Ok(non_empty_path);
                }
            }
            return Ok(vfs_files[index].path.clone());
        }
        if dir_id == PPC_ROOT_DIR_ID {
            if let Some(path) = ppc_vfs_file_path_by_suffix_or_unique_basename(vfs_files, &path) {
                return Ok(path);
            }
        }
    }
    if dir_id == PPC_ROOT_DIR_ID {
        if let Some(path) =
            ppc_vfs_file_path_by_suffix_or_unique_basename(vfs_files, &normalized_name)
        {
            return Ok(path);
        }
    }
    Err(PPC_FNF_ERR)
}

pub(super) fn ppc_unique_non_empty_vfs_file_path_by_basename(
    vfs_files: &[PpcVfsFileRecord],
    path: &str,
) -> Option<String> {
    let basename = ppc_vfs_basename(path);
    let matches = vfs_files
        .iter()
        .filter(|file| !file.data.is_empty())
        .filter(|file| ppc_vfs_basename(&file.path).eq_ignore_ascii_case(basename))
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    ppc_unique_case_insensitive_path(matches)
}

pub(super) fn ppc_vfs_target_path(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> Option<String> {
    if let Some(directory) = vfs_directories
        .iter()
        .find(|directory| directory.path.eq_ignore_ascii_case(path))
    {
        return Some(directory.path.clone());
    }
    ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, path)
}

pub(super) fn ppc_vfs_target_path_by_suffix_or_unique_basename(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> Option<String> {
    let normalized_path = ppc_normalize_vfs_path(path);
    if normalized_path.is_empty() {
        return None;
    }
    let suffix = format!("/{}", normalized_path.to_ascii_lowercase());
    let all_paths = vfs_directories
        .iter()
        .map(|directory| directory.path.as_str())
        .chain(vfs_files.iter().map(|file| file.path.as_str()))
        .chain(vfs_resource_files.iter().map(|file| file.path.as_str()))
        .collect::<Vec<_>>();
    let suffix_matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| candidate.to_ascii_lowercase().ends_with(&suffix))
        .collect::<Vec<_>>();
    if let Some(path) = ppc_unique_case_insensitive_path(suffix_matches) {
        return Some(path);
    }

    let basename = ppc_vfs_basename(&normalized_path);
    let basename_matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| ppc_vfs_basename(candidate).eq_ignore_ascii_case(basename))
        .collect::<Vec<_>>();
    ppc_unique_case_insensitive_path(basename_matches)
}

pub(super) fn ppc_vfs_file_path_by_suffix_or_unique_basename(
    vfs_files: &[PpcVfsFileRecord],
    path: &str,
) -> Option<String> {
    let normalized_path = ppc_normalize_vfs_path(path);
    if normalized_path.is_empty() {
        return None;
    }
    let suffix = format!("/{}", normalized_path.to_ascii_lowercase());
    let all_paths = vfs_files
        .iter()
        .map(|file| file.path.as_str())
        .collect::<Vec<_>>();
    let suffix_matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| candidate.to_ascii_lowercase().ends_with(&suffix))
        .collect::<Vec<_>>();
    if let Some(path) = ppc_unique_case_insensitive_path(suffix_matches) {
        return Some(path);
    }

    let basename = ppc_vfs_basename(&normalized_path);
    let basename_matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| ppc_vfs_basename(candidate).eq_ignore_ascii_case(basename))
        .collect::<Vec<_>>();
    ppc_unique_case_insensitive_path(basename_matches)
}

pub(super) fn ppc_vfs_file_or_resource_path_by_suffix_or_unique_basename(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> Option<String> {
    let normalized_path = ppc_normalize_vfs_path(path);
    if normalized_path.is_empty() {
        return None;
    }
    let suffix = format!("/{}", normalized_path.to_ascii_lowercase());
    let all_paths = vfs_files
        .iter()
        .map(|file| file.path.as_str())
        .chain(vfs_resource_files.iter().map(|file| file.path.as_str()))
        .collect::<Vec<_>>();
    let suffix_matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| candidate.to_ascii_lowercase().ends_with(&suffix))
        .collect::<Vec<_>>();
    if let Some(path) = ppc_unique_case_insensitive_path(suffix_matches) {
        return Some(path);
    }

    let basename = ppc_vfs_basename(&normalized_path);
    let basename_matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| ppc_vfs_basename(candidate).eq_ignore_ascii_case(basename))
        .collect::<Vec<_>>();
    ppc_unique_case_insensitive_path(basename_matches)
}

pub(super) fn ppc_vfs_file_or_resource_path_by_nearest_basename(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
    context_path: Option<&str>,
) -> Option<String> {
    let context_path = context_path?;
    let normalized_path = ppc_normalize_vfs_path(path);
    let basename = ppc_vfs_basename(&normalized_path);
    if basename.is_empty() {
        return None;
    }
    let mut matches = vfs_files
        .iter()
        .map(|file| file.path.as_str())
        .chain(vfs_resource_files.iter().map(|file| file.path.as_str()))
        .filter(|candidate| ppc_vfs_basename(candidate).eq_ignore_ascii_case(basename))
        .collect::<Vec<_>>();
    matches.sort_by_key(|candidate| candidate.to_ascii_lowercase());
    matches.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    let common_prefix_len = |candidate: &str| {
        candidate
            .split('/')
            .zip(context_path.split('/'))
            .take_while(|(left, right)| left.eq_ignore_ascii_case(right))
            .count()
    };
    let best_prefix_len = matches
        .iter()
        .map(|candidate| common_prefix_len(candidate))
        .max()?;
    if best_prefix_len == 0 {
        return None;
    }
    let mut best = matches
        .into_iter()
        .filter(|candidate| common_prefix_len(candidate) == best_prefix_len);
    let selected = best.next()?;
    best.next().is_none().then(|| selected.to_string())
}

pub(super) fn ppc_vfs_file_or_resource_path_by_suffix(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> Option<String> {
    let normalized_path = ppc_normalize_vfs_path(path);
    if normalized_path.is_empty() {
        return None;
    }
    let suffix = format!("/{}", normalized_path.to_ascii_lowercase());
    let all_paths = vfs_files
        .iter()
        .map(|file| file.path.as_str())
        .chain(vfs_resource_files.iter().map(|file| file.path.as_str()))
        .collect::<Vec<_>>();
    let suffix_matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| candidate.to_ascii_lowercase().ends_with(&suffix))
        .collect::<Vec<_>>();
    ppc_unique_case_insensitive_path(suffix_matches)
}

pub(super) fn ppc_vfs_file_or_resource_path_by_parent_basename(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> Option<String> {
    let normalized_path = ppc_normalize_vfs_path(path);
    let (parent, basename) = normalized_path.rsplit_once('/')?;
    let parent_basename = ppc_vfs_basename(parent);
    if parent_basename.is_empty() || basename.is_empty() {
        return None;
    }
    let all_paths = vfs_files
        .iter()
        .map(|file| file.path.as_str())
        .chain(vfs_resource_files.iter().map(|file| file.path.as_str()))
        .collect::<Vec<_>>();
    let matches = all_paths
        .iter()
        .copied()
        .filter(|candidate| {
            let Some((candidate_parent, candidate_basename)) = candidate.rsplit_once('/') else {
                return false;
            };
            candidate_basename.eq_ignore_ascii_case(basename)
                && ppc_vfs_basename(candidate_parent).eq_ignore_ascii_case(parent_basename)
        })
        .collect::<Vec<_>>();
    ppc_unique_case_insensitive_path(matches)
}

pub(super) fn ppc_unique_case_insensitive_path(mut paths: Vec<&str>) -> Option<String> {
    paths.sort_by_key(|path| path.to_ascii_lowercase());
    let mut unique = Vec::new();
    for path in paths {
        if !unique
            .iter()
            .any(|candidate: &&str| candidate.eq_ignore_ascii_case(path))
        {
            unique.push(path);
        }
    }
    (unique.len() == 1).then(|| unique[0].to_string())
}

pub(super) fn ppc_vfs_file_index(vfs_files: &[PpcVfsFileRecord], path: &str) -> Option<usize> {
    vfs_files
        .iter()
        .position(|record| record.path.eq_ignore_ascii_case(path))
}

pub(super) fn ppc_vfs_file_mut<'a>(
    vfs_files: &'a mut [PpcVfsFileRecord],
    path: &str,
) -> Option<&'a mut PpcVfsFileRecord> {
    vfs_files
        .iter_mut()
        .find(|record| record.path.eq_ignore_ascii_case(path))
}

pub(super) fn ppc_vfs_resource_file_index(
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    path: &str,
) -> Option<usize> {
    if let Some(index) = vfs_resource_files
        .iter()
        .position(|record| record.path.eq_ignore_ascii_case(path))
    {
        return Some(index);
    }
    if !path.contains('/') {
        let basename = ppc_vfs_basename(path);
        return vfs_resource_files
            .iter()
            .position(|record| ppc_vfs_basename(&record.path).eq_ignore_ascii_case(basename));
    }
    None
}

pub(super) fn ppc_mark_resource_file_contents_dirty(
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    path: &str,
) {
    if path.is_empty() {
        return;
    }
    if let Some(record) = vfs_resource_files
        .iter_mut()
        .find(|record| record.path.eq_ignore_ascii_case(path))
    {
        record.raw_data = None;
        record.dirty = true;
    }
}

pub(super) fn ppc_resource_file_path(resource_files: &[PpcResourceFileRecord], ref_num: i16) -> String {
    resource_files
        .iter()
        .find(|record| record.ref_num == ref_num)
        .map(|record| record.path.clone())
        .unwrap_or_default()
}

pub(super) fn ppc_rebind_resources_for_path(
    vfs_resources: &mut [PpcVfsResourceRecord],
    path: &str,
    ref_num: i16,
) {
    for resource in vfs_resources
        .iter_mut()
        .filter(|resource| resource.path.eq_ignore_ascii_case(path))
    {
        resource.ref_num = ref_num;
    }
}

pub(super) fn ppc_register_vfs_resource_fonts(resources: &[PpcVfsResourceRecord]) {
    let fond_type = u32::from_be_bytes(*b"FOND");
    let font_type = u32::from_be_bytes(*b"FONT");
    let nfnt_type = u32::from_be_bytes(*b"NFNT");
    let sfnt_type = u32::from_be_bytes(*b"sfnt");

    for font in resources.iter().filter(|resource| {
        resource.res_type == font_type
            || resource.res_type == nfnt_type
            || resource.res_type == sfnt_type
    }) {
        let associations = resources
            .iter()
            .filter(|resource| {
                resource.path.eq_ignore_ascii_case(&font.path) && resource.res_type == fond_type
            })
            .filter_map(|fond| {
                crate::quickdraw::fonts::parse_fond_associations(fond.res_id, &fond.data)
            })
            .flatten()
            .filter(|association| association.font_resource_id == font.res_id)
            .collect::<Vec<_>>();

        if associations.is_empty() {
            if font.res_type == font_type {
                let _ =
                    crate::quickdraw::fonts::register_resource_font_strike(font.res_id, &font.data);
            }
            continue;
        }

        for association in associations {
            if association.style & 0x00ff != 0 {
                continue;
            }
            let registered = if font.res_type == sfnt_type {
                crate::quickdraw::fonts::register_resource_outline_font(
                    association.family_id,
                    &font.data,
                )
            } else {
                crate::quickdraw::fonts::register_resource_font_strike_for_family(
                    association.family_id,
                    association.size,
                    &font.data,
                )
            };
            if registered && std::env::var_os("SYSTEMLESS_TRACE_FONT_TRAPS").is_some() {
                eprintln!(
                    "[FONT] PPC FOND {} maps {}pt style ${:04X} to bitmap resource {}",
                    association.family_id,
                    association.size,
                    association.style,
                    association.font_resource_id,
                );
            }
        }
    }
}

pub(super) fn ppc_vfs_font_id_for_name(resources: &[PpcVfsResourceRecord], name: &str) -> Option<i16> {
    let fond_type = u32::from_be_bytes(*b"FOND");
    resources
        .iter()
        .filter(|resource| resource.res_type == fond_type)
        .find(|resource| {
            decode_mac_roman(&resource.name)
                .trim()
                .eq_ignore_ascii_case(name.trim())
        })
        .map(|resource| resource.res_id)
}

pub(super) fn ppc_vfs_font_name_for_id(resources: &[PpcVfsResourceRecord], font_id: i16) -> Option<String> {
    let fond_type = u32::from_be_bytes(*b"FOND");
    resources
        .iter()
        .find(|resource| resource.res_type == fond_type && resource.res_id == font_id)
        .map(|resource| decode_mac_roman(&resource.name))
}

pub(super) fn ppc_materialize_resource_records_for_path(
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    path: &str,
) {
    let Some(raw_data) = vfs_resource_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
        .and_then(|file| file.raw_data.as_ref())
    else {
        return;
    };
    let Some(fork) = ResourceFork::parse(raw_data) else {
        return;
    };
    let mut sorted_resources = fork.resources().values().collect::<Vec<_>>();
    sorted_resources.sort_by_key(|resource| (resource.res_type, resource.id));
    // Existing records win over the raw fork (they may contain live edits).
    // Index their keys once rather than scanning the entire record list for
    // every resource each time this fork is materialized.
    let mut existing_keys = vfs_resources
        .iter()
        .filter(|resource| resource.path.eq_ignore_ascii_case(path))
        .map(|resource| (resource.res_type, resource.res_id))
        .collect::<std::collections::HashSet<_>>();
    for resource in sorted_resources {
        let res_type = u32::from_be_bytes(resource.res_type);
        if !existing_keys.insert((res_type, resource.id)) {
            continue;
        }
        vfs_resources.push(PpcVfsResourceRecord {
            ref_num: 0,
            path: path.to_string(),
            res_type,
            res_id: resource.id,
            name: resource.name_bytes.clone().unwrap_or_default(),
            data: resource.data.clone(),
            raw_data: resource.raw_data.clone(),
            raw_attrs: resource.raw_attrs.map(u16::from),
            attrs: u16::from(resource.attrs),
            handle: 0,
        });
    }
}

pub(super) fn ppc_materialize_quilt_resources_for_existing_path(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    path: &str,
) {
    let Some((_, mut resources)) =
        ppc_quilt_named_resource_records(vfs_files, vfs_resource_files, path)
    else {
        return;
    };
    resources.sort_by_key(|resource| (resource.res_type, resource.res_id));
    for mut resource in resources {
        if vfs_resources.iter().any(|existing| {
            existing.path.eq_ignore_ascii_case(path)
                && existing.res_type == resource.res_type
                && existing.res_id == resource.res_id
        }) {
            continue;
        }
        resource.path = path.to_string();
        vfs_resources.push(resource);
    }
}

pub(super) fn ppc_materialize_unique_named_resource_file(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    path: &str,
) -> bool {
    if ppc_vfs_resource_file_index(vfs_resource_files, path).is_some() {
        return true;
    }
    let basename = ppc_vfs_basename(path);
    let mut matches = Vec::new();
    for file in vfs_resource_files.iter() {
        let Some(raw_data) = file.raw_data.as_ref() else {
            continue;
        };
        let Some(fork) = ResourceFork::parse(raw_data) else {
            continue;
        };
        for resource in fork.resources().values() {
            let Some(name_bytes) = resource.name_bytes.as_ref() else {
                continue;
            };
            let name = decode_mac_roman(name_bytes);
            if ppc_vfs_basename(&ppc_normalize_vfs_path(&name)).eq_ignore_ascii_case(basename) {
                matches.push((file.clone(), resource.clone()));
            }
        }
    }
    let quilt_records = ppc_quilt_named_resource_records(vfs_files, vfs_resource_files, path);
    let prefer_quilt_records = basename.to_ascii_lowercase().ends_with(".picr")
        && quilt_records.as_ref().is_some_and(|(_, resources)| {
            let img_type = u32::from_be_bytes(*b"#Img");
            let pict_type = u32::from_be_bytes(*b"PICT");
            resources
                .iter()
                .any(|resource| resource.res_type == img_type || resource.res_type == pict_type)
        });
    let (source_file, mut resources) = if matches.is_empty() {
        match quilt_records {
            Some(records) => records,
            None => return false,
        }
    } else if prefer_quilt_records {
        match quilt_records {
            Some(records) => records,
            None => return false,
        }
    } else {
        let source_path = &matches[0].0.path;
        if matches
            .iter()
            .any(|(source_file, _)| !source_file.path.eq_ignore_ascii_case(source_path))
        {
            match quilt_records {
                Some(records) => records,
                None => return false,
            }
        } else {
            let source_file = matches[0].0.clone();
            let resources = matches
                .into_iter()
                .map(|(_, resource)| PpcVfsResourceRecord {
                    ref_num: 0,
                    path: path.to_string(),
                    res_type: u32::from_be_bytes(resource.res_type),
                    res_id: resource.id,
                    name: resource.name_bytes.unwrap_or_default(),
                    data: resource.data,
                    raw_data: resource.raw_data,
                    raw_attrs: resource.raw_attrs.map(u16::from),
                    attrs: u16::from(resource.attrs),
                    handle: 0,
                })
                .collect::<Vec<_>>();
            (source_file, resources)
        }
    };
    ppc_synthesize_quilt_img_resource_if_missing(path, &mut resources);
    let resource_len = resources
        .iter()
        .map(|resource| resource.data.len())
        .sum::<usize>();
    vfs_resource_files.push(PpcVfsResourceFileRecord {
        path: path.to_string(),
        creator: source_file.creator,
        file_type: source_file.file_type,
        finder_flags: source_file.finder_flags,
        resource_len: u32::try_from(resource_len).unwrap_or(u32::MAX),
        raw_data: None,
        map_attrs: source_file.map_attrs,
        dirty: false,
    });
    resources.sort_by_key(|resource| (resource.res_type, resource.res_id));
    for mut resource in resources {
        resource.path = path.to_string();
        vfs_resources.push(resource);
    }
    true
}

pub(super) fn ppc_synthesize_quilt_img_resource_if_missing(
    path: &str,
    resources: &mut Vec<PpcVfsResourceRecord>,
) {
    let img_type = u32::from_be_bytes(*b"#Img");
    if resources
        .iter()
        .any(|resource| resource.res_type == img_type)
    {
        return;
    }
    let pict_type = u32::from_be_bytes(*b"PICT");
    let frame_count = resources
        .iter()
        .filter(|resource| resource.res_type == pict_type)
        .count();
    if frame_count == 0 {
        return;
    }
    let mut data = vec![0u8; 18];
    data[0..2].copy_from_slice(&1u16.to_be_bytes());
    data[2..4].copy_from_slice(&1u16.to_be_bytes());
    data[4..6].copy_from_slice(&1u16.to_be_bytes());
    data[6..8].copy_from_slice(&(u16::try_from(frame_count).unwrap_or(u16::MAX)).to_be_bytes());
    data[8..10].copy_from_slice(&1u16.to_be_bytes());
    data[10..12].copy_from_slice(&1u16.to_be_bytes());

    resources.push(PpcVfsResourceRecord {
        ref_num: 0,
        path: path.to_string(),
        res_type: img_type,
        res_id: 1000,
        name: ppc_vfs_basename(path).as_bytes().to_vec(),
        data,
        raw_data: None,
        raw_attrs: None,
        attrs: 0,
        handle: 0,
    });
}

pub(super) fn ppc_quilt_named_resource_records(
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    target_path: &str,
) -> Option<(PpcVfsResourceFileRecord, Vec<PpcVfsResourceRecord>)> {
    let target_basename = ppc_vfs_basename(target_path);
    let versionless_target_basename = target_basename
        .trim_end_matches(|character: char| character.is_ascii_digit())
        .trim_end();
    let mut matches = Vec::new();
    let mut best_name_rank = 0u8;
    for resource_file in vfs_resource_files {
        let Some(raw_data) = resource_file.raw_data.as_ref() else {
            continue;
        };
        let Some(fork) = ResourceFork::parse(raw_data) else {
            continue;
        };
        let Some(data_file) = vfs_files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(&resource_file.path))
        else {
            continue;
        };
        for resource in fork.resources().values() {
            if resource.res_type != *b"qDir" {
                continue;
            }
            for record in resource.data.chunks_exact(60) {
                let name_len = u16::from_be_bytes([record[26], record[27]]) as usize;
                if name_len == 0 || name_len > 32 {
                    continue;
                }
                let name_bytes = &record[28..28 + name_len];
                let name = decode_mac_roman(name_bytes);
                let normalized_name = ppc_normalize_vfs_path(&name);
                let record_basename = ppc_vfs_basename(&normalized_name);
                let name_rank = if record_basename.eq_ignore_ascii_case(target_basename) {
                    2
                } else if versionless_target_basename.len() < target_basename.len()
                    && record_basename.eq_ignore_ascii_case(versionless_target_basename)
                {
                    1
                } else {
                    0
                };
                if name_rank == 0 || name_rank < best_name_rank {
                    continue;
                }
                if name_rank > best_name_rank {
                    matches.clear();
                    best_name_rank = name_rank;
                }
                let data_len =
                    u32::from_be_bytes([record[8], record[9], record[10], record[11]]) as usize;
                let data_offset =
                    u32::from_be_bytes([record[12], record[13], record[14], record[15]]) as usize;
                let Some(end) = data_offset.checked_add(data_len) else {
                    continue;
                };
                let Some(data) = data_file.data.get(data_offset..end) else {
                    continue;
                };
                matches.push((
                    resource_file.clone(),
                    PpcVfsResourceRecord {
                        ref_num: 0,
                        path: String::new(),
                        res_type: u32::from_be_bytes([record[0], record[1], record[2], record[3]]),
                        res_id: u32::from_be_bytes([record[4], record[5], record[6], record[7]])
                            as u16 as i16,
                        name: name_bytes.to_vec(),
                        data: data.to_vec(),
                        raw_data: None,
                        raw_attrs: None,
                        attrs: 0,
                        handle: 0,
                    },
                ));
            }
        }
    }
    if matches.is_empty() {
        return None;
    }
    let mut source_paths = matches
        .iter()
        .map(|(source_file, _)| source_file.path.clone())
        .collect::<Vec<_>>();
    source_paths.sort_by_key(|path| {
        let common_prefix = path
            .split('/')
            .zip(target_path.split('/'))
            .take_while(|(left, right)| left.eq_ignore_ascii_case(right))
            .count();
        let depth = path
            .split('/')
            .filter(|component| !component.is_empty())
            .count();
        std::cmp::Reverse((common_prefix, depth))
    });
    source_paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let source_path = source_paths.first()?;
    matches.retain(|(source_file, _)| source_file.path.eq_ignore_ascii_case(source_path));

    let source_file = matches.first()?.0.clone();
    let mut resources = matches
        .into_iter()
        .map(|(_, resource)| resource)
        .collect::<Vec<_>>();
    ppc_add_quilt_anam_picture_resources(vfs_files, &source_file, &mut resources);
    ppc_expand_compact_quilt_frames(&mut resources);
    ppc_wrap_quilt_raw_pict_frames(&mut resources);
    Some((source_file, resources))
}

pub(super) fn ppc_add_quilt_anam_picture_resources(
    vfs_files: &[PpcVfsFileRecord],
    source_file: &PpcVfsResourceFileRecord,
    resources: &mut Vec<PpcVfsResourceRecord>,
) {
    ppc_apply_quilt_animation_resource_names(resources);
    let pict_type = u32::from_be_bytes(*b"PICT");
    let img_type = u32::from_be_bytes(*b"#Img");
    if resources
        .iter()
        .any(|resource| resource.res_type == pict_type || resource.res_type == img_type)
    {
        return;
    }
    let anam_type = u32::from_be_bytes(*b"ANAM");
    let Some(picture_name) = resources
        .iter()
        .find(|resource| resource.res_type == anam_type)
        .and_then(|resource| {
            let len = resource
                .data
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(resource.data.len());
            let name = decode_mac_roman(&resource.data[..len]);
            let normalized = ppc_normalize_vfs_path(&name);
            normalized
                .to_ascii_lowercase()
                .ends_with(".picr")
                .then_some(normalized)
        })
    else {
        return;
    };
    let picture_basename = ppc_vfs_basename(&picture_name);
    let Some(raw_data) = source_file.raw_data.as_ref() else {
        return;
    };
    let Some(fork) = ResourceFork::parse(raw_data) else {
        return;
    };
    let Some(data_file) = vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(&source_file.path))
    else {
        return;
    };
    let existing = resources
        .iter()
        .map(|resource| (resource.res_type, resource.res_id))
        .collect::<Vec<_>>();
    for resource in fork.resources().values() {
        if resource.res_type != *b"qDir" {
            continue;
        }
        for record in resource.data.chunks_exact(60) {
            let name_len = u16::from_be_bytes([record[26], record[27]]) as usize;
            if name_len == 0 || name_len > 32 {
                continue;
            }
            let name_bytes = &record[28..28 + name_len];
            let name = decode_mac_roman(name_bytes);
            if !ppc_vfs_basename(&ppc_normalize_vfs_path(&name))
                .eq_ignore_ascii_case(picture_basename)
            {
                continue;
            }
            let res_type = u32::from_be_bytes([record[0], record[1], record[2], record[3]]);
            let res_id =
                u32::from_be_bytes([record[4], record[5], record[6], record[7]]) as u16 as i16;
            if existing.iter().any(|(existing_type, existing_id)| {
                *existing_type == res_type && *existing_id == res_id
            }) {
                continue;
            }
            let data_len =
                u32::from_be_bytes([record[8], record[9], record[10], record[11]]) as usize;
            let data_offset =
                u32::from_be_bytes([record[12], record[13], record[14], record[15]]) as usize;
            let Some(end) = data_offset.checked_add(data_len) else {
                continue;
            };
            let Some(data) = data_file.data.get(data_offset..end) else {
                continue;
            };
            resources.push(PpcVfsResourceRecord {
                ref_num: 0,
                path: String::new(),
                res_type,
                res_id,
                name: name_bytes.to_vec(),
                data: data.to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
        }
    }
}

pub(super) fn ppc_apply_quilt_animation_resource_names(resources: &mut [PpcVfsResourceRecord]) {
    let animation_list_type = u32::from_be_bytes(*b"Alst");
    let animation_name_type = u32::from_be_bytes(*b"ANAM");
    let names = resources
        .iter()
        .filter(|resource| resource.res_type == animation_name_type)
        .filter_map(|resource| {
            let len = resource
                .data
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(resource.data.len());
            let name = resource.data.get(..len)?;
            decode_mac_roman(name)
                .to_ascii_lowercase()
                .ends_with(".picr")
                .then(|| (resource.res_id, name.to_vec()))
        })
        .collect::<Vec<_>>();
    for resource in resources
        .iter_mut()
        .filter(|resource| resource.res_type == animation_list_type)
    {
        if let Some((_, name)) = names.iter().find(|(id, _)| *id == resource.res_id) {
            resource.name.clone_from(name);
        }
    }
}

pub(super) fn ppc_expand_compact_quilt_frames(resources: &mut Vec<PpcVfsResourceRecord>) {
    let img_type = u32::from_be_bytes(*b"#Img");
    let frms_type = u32::from_be_bytes(*b"frms");
    let pict_type = u32::from_be_bytes(*b"PICT");
    let header_frame_count = resources
        .iter()
        .find(|resource| resource.res_type == img_type && resource.data.len() >= 8)
        .map(|img| u16::from_be_bytes([img.data[6], img.data[7]]) as usize)
        .unwrap_or(0);
    let frms_indices = resources
        .iter()
        .enumerate()
        .filter(|(_, resource)| resource.res_type == frms_type)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let pict_indices = resources
        .iter()
        .enumerate()
        .filter(|(_, resource)| resource.res_type == pict_type)
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if frms_indices.len() != 1 || pict_indices.len() != 1 {
        return;
    }
    let frms = resources[frms_indices[0]].clone();
    let pict = resources[pict_indices[0]].clone();
    if pict.data.is_empty() {
        return;
    }
    let inferred_frame_count = frms.data.len() / 8;
    let frame_count = if header_frame_count > 1
        && frms.data.len() == header_frame_count * 8
        && pict.data.len() % header_frame_count == 0
    {
        header_frame_count
    } else if inferred_frame_count > 1 && frms.data.len() == inferred_frame_count * 8 {
        inferred_frame_count
    } else {
        return;
    };
    if pict.data.len() % frame_count != 0 {
        return;
    }
    let frame_pict_len = pict.data.len() / frame_count;
    let mut expanded = resources
        .iter()
        .filter(|resource| resource.res_type != frms_type && resource.res_type != pict_type)
        .cloned()
        .collect::<Vec<_>>();
    for frame_index in 0..frame_count {
        let Some(res_id) = 1000i16.checked_add(frame_index as i16) else {
            return;
        };
        let frame_offset = frame_index * 8;
        let pict_offset = frame_index * frame_pict_len;
        let mut frame_resource = frms.clone();
        frame_resource.res_id = res_id;
        frame_resource.data = frms.data[frame_offset..frame_offset + 8].to_vec();
        frame_resource.handle = 0;
        expanded.push(frame_resource);

        let mut pict_resource = pict.clone();
        pict_resource.res_id = res_id;
        pict_resource.data = pict.data[pict_offset..pict_offset + frame_pict_len].to_vec();
        pict_resource.handle = 0;
        expanded.push(pict_resource);
    }
    *resources = expanded;
}

pub(super) fn ppc_wrap_quilt_raw_pict_frames(resources: &mut [PpcVfsResourceRecord]) {
    let frms_type = u32::from_be_bytes(*b"frms");
    let pict_type = u32::from_be_bytes(*b"PICT");
    let frame_rects = resources
        .iter()
        .filter(|resource| resource.res_type == frms_type && resource.data.len() == 8)
        .map(|resource| (resource.res_id, resource.data.clone()))
        .collect::<Vec<_>>();
    if frame_rects.is_empty() {
        return;
    }
    for resource in resources.iter_mut() {
        if resource.res_type != pict_type {
            continue;
        }
        let Some((_, rect)) = frame_rects
            .iter()
            .find(|(res_id, _)| *res_id == resource.res_id)
        else {
            continue;
        };
        let top = i16::from_be_bytes([rect[0], rect[1]]);
        let bottom = i16::from_be_bytes([rect[4], rect[5]]);
        let height = i32::from(bottom) - i32::from(top);
        if height <= 0 || resource.data.len() % height as usize != 0 {
            continue;
        }
        let Some(pict_size) = resource.data.len().checked_add(24) else {
            continue;
        };
        let pict_size = u16::try_from(pict_size).unwrap_or(u16::MAX);
        let mut wrapped = Vec::with_capacity(usize::from(pict_size));
        wrapped.extend_from_slice(&pict_size.to_be_bytes());
        wrapped.extend_from_slice(rect);
        wrapped.extend_from_slice(&[0; 14]);
        wrapped.extend_from_slice(&resource.data);
        resource.data = wrapped;
        resource.handle = 0;
    }
}

pub(super) fn ppc_file_permission_allows_writing(permission: u8) -> bool {
    // Inside Macintosh: Files (1992), pp. 2-7--2-8: fsCurPerm grants
    // read/write when available, and fsWrPerm is synonymous with fsRdWrPerm.
    matches!(permission, 0 | 2 | 3 | 4)
}

pub(super) fn ppc_fsp_open_df(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut Vec<PpcVfsFileRecord>,
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    next_file_ref_num: &mut i16,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let permission = cpu.gpr[4] as u8;
    let ref_num_out_ptr = cpu.gpr[5];
    if ref_num_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, ref_num_out_ptr, 2) {
        return PPC_PARAM_ERR;
    }
    let path = match ppc_existing_data_path_for_fsspec(memory, vfs_directories, vfs_files, spec_ptr)
    {
        Ok(path) => path,
        Err(err) => return err,
    };
    let ref_num = *next_file_ref_num;
    let Some(next_ref_num) = next_file_ref_num.checked_add(1) else {
        return PPC_PARAM_ERR;
    };
    let Some(existing_index) = ppc_vfs_file_index(vfs_files, &path) else {
        return PPC_FNF_ERR;
    };
    let opened_data_len = vfs_files[existing_index].data.len();
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] FSpOpenDF path=\"{}\" permission={} data_len={} result={}",
            path, permission, opened_data_len, PPC_NO_ERR
        );
    }
    let _ = memory.write_u16_be(ref_num_out_ptr, ref_num as u16);
    files.push(PpcFileRecord {
        ref_num,
        path,
        position: 0,
    });
    if ppc_file_permission_allows_writing(permission) {
        writable_refnums.insert(ref_num as u16);
    }
    *next_file_ref_num = next_ref_num;
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_fsp_open_rf(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &[PpcVfsResourceRecord],
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    next_file_ref_num: &mut i16,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let permission = cpu.gpr[4] as u8;
    let ref_num_out_ptr = cpu.gpr[5];
    if ref_num_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, ref_num_out_ptr, 2) {
        return PPC_PARAM_ERR;
    }
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => return err,
    };
    let Some(resource_index) = ppc_vfs_resource_file_index(vfs_resource_files, &path) else {
        return PPC_FNF_ERR;
    };
    let path = vfs_resource_files[resource_index].path.clone();
    let open_path = format!("{PPC_OPEN_RESOURCE_FORK_PREFIX}{path}");
    if ppc_vfs_file_index(vfs_files, &open_path).is_none() {
        let bytes = vfs_resource_files
            .fork(&path)
            .cloned()
            .or_else(|| ppc_serialized_resource_fork(&vfs_resource_files[resource_index], vfs_resources))
            .unwrap_or_default();
        vfs_files.push(PpcVfsFileRecord {
            path: open_path.clone(),
            data: bytes.into(),
            creator: 0,
            file_type: 0,
            finder_flags: 0,
            dirty: false,
        });
    }
    let ref_num = *next_file_ref_num;
    let Some(next_ref_num) = next_file_ref_num.checked_add(1) else {
        return PPC_PARAM_ERR;
    };
    let _ = memory.write_u16_be(ref_num_out_ptr, ref_num as u16);
    files.push(PpcFileRecord {
        ref_num,
        path: open_path,
        position: 0,
    });
    if ppc_file_permission_allows_writing(permission) {
        writable_refnums.insert(ref_num as u16);
    }
    *next_file_ref_num = next_ref_num;
    if ppc_hle_trace_enabled() {
        eprintln!("[PPC-TRACE] FSpOpenRF path=\"{}\" permission={} -> ref={}", path, permission, ref_num);
    }
    PPC_NO_ERR
}

pub(super) fn ppc_h_open(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    next_file_ref_num: &mut i16,
    default_dir_id: u32,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
) -> i16 {
    // Inside Macintosh: Files (1992), 2-181: HOpen takes vRefNum, dirID,
    // fileName, permission, and a returned file reference number. A working
    // directory reference can stand in for the volume reference; when dirID
    // is zero it also supplies the directory to search.
    let vref = cpu.gpr[3] as u16 as i16;
    let dir_id = cpu.gpr[4];
    let (volume_ref_num, effective_dir_id) = working_directories
        .get(&vref)
        .map(|record| {
            (
                record.volume_ref_num,
                if dir_id == 0 { record.dir_id } else { dir_id },
            )
        })
        .unwrap_or((vref, dir_id));
    ppc_open_data_fork_by_name(
        memory,
        vfs_directories,
        vfs_files,
        files,
        writable_refnums,
        next_file_ref_num,
        default_dir_id,
        volume_ref_num,
        effective_dir_id,
        cpu.gpr[5],
        cpu.gpr[6] as u8,
        cpu.gpr[7],
    )
}

pub(super) fn ppc_fs_open(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    next_file_ref_num: &mut i16,
    default_dir_id: u32,
) -> i16 {
    // Inside Macintosh: Files (1992), 2-179: FSOpen resolves its file name
    // relative to the current volume/directory and returns a refnum.
    ppc_open_data_fork_by_name(
        memory,
        vfs_directories,
        vfs_files,
        files,
        writable_refnums,
        next_file_ref_num,
        default_dir_id,
        0,
        0,
        cpu.gpr[3],
        cpu.gpr[4] as u8,
        cpu.gpr[5],
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_open_data_fork_by_name(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    next_file_ref_num: &mut i16,
    default_dir_id: u32,
    vref: i16,
    dir_id: u32,
    name_ptr: u32,
    permission: u8,
    ref_num_out_ptr: u32,
) -> i16 {
    if !matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM)
        || ref_num_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, ref_num_out_ptr, 2)
    {
        return PPC_PARAM_ERR;
    }
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, name_ptr) else {
        return PPC_PARAM_ERR;
    };
    let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if name.is_empty() {
        return PPC_PARAM_ERR;
    }
    let effective_dir_id = ppc_resolve_directory_id(vref, dir_id, default_dir_id);
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, effective_dir_id) else {
        return PPC_DIR_NF_ERR;
    };
    let requested_path = ppc_join_vfs_path(parent_path, &name);
    let path = ppc_vfs_file_index(vfs_files, &requested_path)
        .map(|index| vfs_files[index].path.clone())
        .or_else(|| ppc_vfs_file_path_by_suffix_or_unique_basename(vfs_files, &name));
    let Some(path) = path else {
        return PPC_FNF_ERR;
    };
    let ref_num = *next_file_ref_num;
    let Some(next_ref_num) = next_file_ref_num.checked_add(1) else {
        return PPC_PARAM_ERR;
    };
    if memory
        .write_u16_be(ref_num_out_ptr, ref_num as u16)
        .is_none()
    {
        return PPC_PARAM_ERR;
    }
    files.push(PpcFileRecord {
        ref_num,
        path,
        position: 0,
    });
    if ppc_file_permission_allows_writing(permission) {
        writable_refnums.insert(ref_num as u16);
    }
    *next_file_ref_num = next_ref_num;
    PPC_NO_ERR
}

pub(super) fn ppc_pbh_open_df(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut [PpcVfsFileRecord],
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    next_file_ref_num: &mut i16,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 52) {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, name_ptr) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(dir_id) = memory.read_u32_be(pb + 48) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(permission) = memory.read_u8(pb + 27) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if name.is_empty() {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    let exact_path = ppc_directory_path_for_id(vfs_directories, dir_id)
        .map(|parent| ppc_join_vfs_path(parent, &name));
    let path = exact_path
        .as_deref()
        .and_then(|path| {
            ppc_vfs_file_index(vfs_files, path).map(|index| vfs_files[index].path.clone())
        })
        .or_else(|| ppc_vfs_file_path_by_suffix_or_unique_basename(vfs_files, &name));
    let Some(path) = path else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    let ref_num = *next_file_ref_num;
    let Some(next_ref_num) = next_file_ref_num.checked_add(1) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };

    // Inside Macintosh: Files (1992), 2-183 through 2-184: PBHOpenDF uses
    // ioNamePtr/ioVRefNum/ioDirID to open a data fork and returns the access
    // path's file reference number in ioRefNum at offset 24.
    if memory.write_u16_be(pb + 24, ref_num as u16).is_none() {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    files.push(PpcFileRecord {
        ref_num,
        path,
        position: 0,
    });
    if ppc_file_permission_allows_writing(permission) {
        writable_refnums.insert(ref_num as u16);
    }
    *next_file_ref_num = next_ref_num;
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_open(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_volumes: &[PpcVfsVolumeRecord],
    vfs_files: &[PpcVfsFileRecord],
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    next_file_ref_num: &mut i16,
    default_dir_id: u32,
    application_working_directory_ref_num: i16,
    working_directories: &HashMap<i16, ProcessWorkingDirectory>,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 28) {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(requested_vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(permission) = memory.read_u8(pb + 27) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(working_directory) = dispatch_files::ppc_working_directory_info(
        requested_vref,
        application_working_directory_ref_num,
        default_dir_id,
        working_directories,
        vfs_volumes,
    ) else {
        return ppc_complete_pb(memory, pb, PPC_NSV_ERR);
    };
    let result = ppc_open_data_fork_by_name(
        memory,
        vfs_directories,
        vfs_files,
        files,
        writable_refnums,
        next_file_ref_num,
        default_dir_id,
        working_directory.volume_ref_num,
        working_directory.dir_id,
        name_ptr,
        permission,
        pb + 24,
    );
    ppc_complete_pb(memory, pb, result)
}

pub(crate) const PPC_OPEN_RESOURCE_FORK_PREFIX: &str = "\0resource-fork:";

pub(super) fn ppc_sync_open_resource_fork(
    ref_num: i16,
    files: &[PpcFileRecord],
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) {
    let Some(open_path) = files
        .iter()
        .find(|file| file.ref_num == ref_num)
        .map(|file| file.path.clone())
    else {
        return;
    };
    let Some(path) = open_path.strip_prefix(PPC_OPEN_RESOURCE_FORK_PREFIX) else {
        return;
    };
    if files
        .iter()
        .any(|file| file.ref_num != ref_num && file.path == open_path)
    {
        return;
    }
    let Some(index) = ppc_vfs_file_index(vfs_files, &open_path) else {
        return;
    };
    if vfs_files[index].dirty {
        let data = vfs_files[index].data.to_vec();
        vfs_resource_files.update_fork(path, &data);
        if let Some(resource_index) = ppc_vfs_resource_file_index(vfs_resource_files, path) {
            let resource_file = &mut vfs_resource_files[resource_index];
            resource_file.raw_data = Some(data.clone().into());
            resource_file.resource_len = u32::try_from(data.len()).unwrap_or(u32::MAX);
            resource_file.dirty = true;
        }
        vfs_resources.retain(|resource| !resource.path.eq_ignore_ascii_case(path));
    }
    vfs_files.retain(|record| !record.path.eq_ignore_ascii_case(&open_path));
}

pub(super) fn ppc_fs_close(
    cpu: &mut PpcCpu,
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> i16 {
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    if std::env::var_os("SYSTEMLESS_PPC_FILE_TRACE").is_some() {
        eprintln!("[PPC-FILE-TRACE] FSClose ref={} path={:?}", ref_num, files.iter().find(|file| file.ref_num == ref_num).map(|file| &file.path));
    }
    ppc_sync_open_resource_fork(ref_num, files, vfs_files, vfs_resource_files, vfs_resources);
    files.retain(|file| file.ref_num != ref_num);
    writable_refnums.remove(&(ref_num as u16));
    PPC_NO_ERR
}

pub(super) fn ppc_pb_close(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> i16 {
    let pb = cpu.gpr[3];
    let Some(ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    if !files.iter().any(|file| file.ref_num == ref_num) {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    }
    if std::env::var_os("SYSTEMLESS_PPC_FILE_TRACE").is_some() {
        eprintln!("[PPC-FILE-TRACE] PBClose ref={} path={:?}", ref_num, files.iter().find(|file| file.ref_num == ref_num).map(|file| &file.path));
    }
    ppc_sync_open_resource_fork(ref_num, files, vfs_files, vfs_resource_files, vfs_resources);
    files.retain(|file| file.ref_num != ref_num);
    writable_refnums.remove(&(ref_num as u16));
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_flush_file(cpu: &PpcCpu, memory: &mut PpcSectionMem, files: &[PpcFileRecord]) -> i16 {
    let pb = cpu.gpr[3];
    let Some(ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    // VFS writes are committed to their in-memory fork immediately, so the
    // synchronous flush only validates the open file and completes ioResult.
    let result = if files.iter().any(|file| file.ref_num == ref_num) {
        PPC_NO_ERR
    } else {
        PPC_RF_NUM_ERR
    };
    ppc_complete_pb(memory, pb, result)
}

pub(super) fn ppc_pb_position(mode: u16, offset: i32, mark: u32, eof: usize) -> Option<usize> {
    let base = match mode & 0x000f {
        0 => i64::from(mark),
        1 => 0,
        2 => i64::try_from(eof).ok()?,
        3 => i64::from(mark),
        _ => return None,
    };
    let delta = if mode & 0x000f == 0 {
        0
    } else {
        i64::from(offset)
    };
    usize::try_from(base.checked_add(delta)?).ok()
}

pub(super) fn ppc_pb_read(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut [PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 50) {
        return PPC_PARAM_ERR;
    }
    let Some(ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(buffer_ptr) = memory.read_u32_be(pb + 32) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(requested_count) = memory.read_u32_be(pb + 36) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(position_mode) = memory.read_u16_be(pb + 44) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(position_offset) = memory.read_u32_be(pb + 46).map(|value| value as i32) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(file_index) = files.iter().position(|file| file.ref_num == ref_num) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    let Some(vfs_file) = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&files[file_index].path))
    else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    let Some(start) = ppc_pb_position(
        position_mode,
        position_offset,
        files[file_index].position,
        vfs_file.data.len(),
    ) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    if start > vfs_file.data.len() {
        return ppc_complete_pb(memory, pb, PPC_POS_ERR);
    }
    let requested_len = usize::try_from(requested_count).unwrap_or(usize::MAX);
    let read_len = requested_len.min(vfs_file.data.len().saturating_sub(start));
    if read_len != 0
        && (buffer_ptr == 0 || !ppc_memory_can_write_bytes(memory, buffer_ptr, read_len as u32))
    {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    for (offset, byte) in vfs_file.data[start..start + read_len]
        .iter()
        .copied()
        .enumerate()
    {
        if memory.write_u8(buffer_ptr + offset as u32, byte).is_none() {
            return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
        }
    }
    let new_position = start.saturating_add(read_len).min(u32::MAX as usize) as u32;
    files[file_index].position = new_position;
    let _ = memory.write_u32_be(pb + 40, read_len as u32);
    let _ = memory.write_u32_be(pb + 46, new_position);
    let result = if read_len < requested_len {
        PPC_EOF_ERR
    } else {
        PPC_NO_ERR
    };
    if std::env::var_os("SYSTEMLESS_PPC_FILE_TRACE").is_some() {
        eprintln!("[PPC-FILE-TRACE] PBRead path=\"{}\" start={} requested={} read={} result={}", vfs_file.path, start, requested_len, read_len, result);
    }
    ppc_complete_pb(memory, pb, result)
}

pub(super) fn ppc_pb_write(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut [PpcFileRecord],
    writable_refnums: &HashSet<u16>,
    vfs_files: &mut [PpcVfsFileRecord],
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 50) {
        return PPC_PARAM_ERR;
    }
    let Some(ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(buffer_ptr) = memory.read_u32_be(pb + 32) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(requested_count) = memory.read_u32_be(pb + 36) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(position_mode) = memory.read_u16_be(pb + 44) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(position_offset) = memory.read_u32_be(pb + 46).map(|value| value as i32) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    if requested_count != 0
        && (buffer_ptr == 0 || !ppc_memory_can_read_bytes(memory, buffer_ptr, requested_count))
    {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    let Some(file_index) = files.iter().position(|file| file.ref_num == ref_num) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    if !writable_refnums.contains(&(ref_num as u16)) {
        return ppc_complete_pb(memory, pb, PPC_WR_PERM_ERR);
    }
    let Some(vfs_file_index) = vfs_files
        .iter()
        .position(|record| record.path.eq_ignore_ascii_case(&files[file_index].path))
    else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    let Some(start) = ppc_pb_position(
        position_mode,
        position_offset,
        files[file_index].position,
        vfs_files[vfs_file_index].data.len(),
    ) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Ok(write_len) = usize::try_from(requested_count) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(end) = start.checked_add(write_len) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let write_result = vfs_files[vfs_file_index].data.with_mut(|data| {
        data.resize(end, 0);
        for offset in 0..write_len {
            let Some(byte) = memory.read_u8(buffer_ptr + offset as u32) else {
                return Err(());
            };
            data[start + offset] = byte;
        }
        Ok(())
    });
    if write_result.is_err() {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    vfs_files[vfs_file_index].dirty = true;
    let new_position = end.min(u32::MAX as usize) as u32;
    files[file_index].position = new_position;
    let _ = memory.write_u32_be(pb + 40, requested_count);
    let _ = memory.write_u32_be(pb + 46, new_position);
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_get_eof(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &[PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> i16 {
    let pb = cpu.gpr[3];
    let Some(ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(file) = files.iter().find(|file| file.ref_num == ref_num) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    let Some(vfs_file) = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&file.path))
    else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    let eof = u32::try_from(vfs_file.data.len()).unwrap_or(u32::MAX);
    if memory.write_u32_be(pb + 28, eof).is_none() {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    }
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_set_eof(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut [PpcFileRecord],
    writable_refnums: &HashSet<u16>,
    vfs_files: &mut [PpcVfsFileRecord],
) -> i16 {
    let pb = cpu.gpr[3];
    let Some(ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(new_eof) = memory.read_u32_be(pb + 28) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(file_index) = files.iter().position(|file| file.ref_num == ref_num) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    if !writable_refnums.contains(&(ref_num as u16)) {
        return ppc_complete_pb(memory, pb, PPC_WR_PERM_ERR);
    }
    let Some(vfs_file) = vfs_files
        .iter_mut()
        .find(|record| record.path.eq_ignore_ascii_case(&files[file_index].path))
    else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    let Ok(new_len) = usize::try_from(new_eof) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    vfs_file.data.with_mut(|data| data.resize(new_len, 0));
    vfs_file.dirty = true;
    files[file_index].position = files[file_index].position.min(new_eof);
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

pub(super) fn ppc_pb_set_fpos(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut [PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> i16 {
    let pb = cpu.gpr[3];
    let Some(ref_num) = memory.read_u16_be(pb + 24).map(|value| value as i16) else {
        return PPC_PARAM_ERR;
    };
    let Some(position_mode) = memory.read_u16_be(pb + 44) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(position_offset) = memory.read_u32_be(pb + 46).map(|value| value as i32) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(file_index) = files.iter().position(|file| file.ref_num == ref_num) else {
        return ppc_complete_pb(memory, pb, PPC_RF_NUM_ERR);
    };
    let Some(eof) = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&files[file_index].path))
        .map(|record| record.data.len())
    else {
        return ppc_complete_pb(memory, pb, PPC_FNF_ERR);
    };
    let Some(position) = ppc_pb_position(
        position_mode,
        position_offset,
        files[file_index].position,
        eof,
    ) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Ok(position) = u32::try_from(position) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    files[file_index].position = position;
    let _ = memory.write_u32_be(pb + 46, position);
    ppc_complete_pb(memory, pb, PPC_NO_ERR)
}

// Inside Macintosh: Files (1992), 2-92 through 2-103 and the I/O parameter
// layout on 2-237 define PBRead/PBWrite/PBSetFPos/PBGetEOF/PBSetEOF/PBClose,
// including ioActCount, mark updates, and eofErr on a short read.

pub(super) fn ppc_fsp_create_res_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    last_resource_error: &mut i16,
) {
    let spec_ptr = cpu.gpr[3];
    let creator = cpu.gpr[4];
    let file_type = cpu.gpr[5];
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => {
            *last_resource_error = err;
            return;
        }
    };
    if ppc_vfs_resource_file_index(vfs_resource_files, &path).is_some() {
        *last_resource_error = PPC_DUP_FN_ERR;
        return;
    }
    if ppc_vfs_file_index(vfs_files, &path).is_none() {
        vfs_files.push(PpcVfsFileRecord {
            path: path.clone(),
            data: Vec::new().into(),
            creator,
            file_type,
            finder_flags: 0,
            dirty: true,
        });
    }
    vfs_resource_files.push(PpcVfsResourceFileRecord {
        path,
        creator,
        file_type,
        finder_flags: 0,
        resource_len: 0,
        raw_data: None,
        map_attrs: 0,
        dirty: true,
    });
    *last_resource_error = PPC_NO_ERR;
}

pub(super) fn ppc_h_create_res_file(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    default_dir_id: u32,
    last_resource_error: &mut i16,
) {
    let vref = cpu.gpr[3] as u16 as i16;
    if !matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        *last_resource_error = PPC_NSV_ERR;
        return;
    }
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, cpu.gpr[5]) else {
        *last_resource_error = PPC_PARAM_ERR;
        return;
    };
    let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if name.is_empty() {
        *last_resource_error = PPC_BD_NAM_ERR;
        return;
    }
    let dir_id = ppc_resolve_directory_id(vref, cpu.gpr[4], default_dir_id);
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) else {
        *last_resource_error = PPC_DIR_NF_ERR;
        return;
    };
    let path = ppc_join_vfs_path(parent_path, &name);
    if ppc_vfs_resource_file_index(vfs_resource_files, &path).is_some() {
        *last_resource_error = PPC_DUP_FN_ERR;
        return;
    }
    let (creator, file_type, finder_flags) =
        if let Some(index) = ppc_vfs_file_index(vfs_files, &path) {
            let file = &vfs_files[index];
            (file.creator, file.file_type, file.finder_flags)
        } else {
            vfs_files.push(PpcVfsFileRecord {
                path: path.clone(),
                data: Vec::new().into(),
                creator: 0,
                file_type: 0,
                finder_flags: 0,
                dirty: true,
            });
            (0, 0, 0)
        };
    // More Macintosh Toolbox (1993), pp. 1-56--1-57: HCreateResFile uses
    // vRefNum/dirID/name, creates a missing zero-length data fork, and leaves
    // the new empty resource fork closed until HOpenResFile is called.
    vfs_resource_files.push(PpcVfsResourceFileRecord {
        path,
        creator,
        file_type,
        finder_flags,
        resource_len: 0,
        raw_data: None,
        map_attrs: 0,
        dirty: true,
    });
    *last_resource_error = PPC_NO_ERR;
}

pub(super) fn ppc_fsp_open_res_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    next_file_ref_num: &mut i16,
    current_resource_refnum: &mut i16,
    last_resource_error: &mut i16,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let _permission = cpu.gpr[4] as u8;
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => {
            *last_resource_error = err;
            return -1;
        }
    };
    let resolved_path = if let Some(index) = ppc_vfs_resource_file_index(vfs_resource_files, &path)
    {
        vfs_resource_files[index].path.clone()
    } else {
        path
    };
    let path = resolved_path;
    let data_file_exists = vfs_files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case(&path));
    if ppc_vfs_resource_file_index(vfs_resource_files, &path).is_none() {
        *last_resource_error = if data_file_exists {
            PPC_RES_F_NOT_FOUND_ERR
        } else {
            PPC_FNF_ERR
        };
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] FSpOpenResFile path=\"{}\" permission={} -> {}",
                path, _permission, *last_resource_error
            );
        }
        return -1;
    }
    if let Some(existing) = resource_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&path))
    {
        ppc_set_current_resource_refnum(memory, current_resource_refnum, existing.ref_num);
        ppc_rebind_resources_for_path(vfs_resources, &path, existing.ref_num);
        *last_resource_error = PPC_NO_ERR;
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] FSpOpenResFile path=\"{}\" permission={} -> ref={} existing",
                path, _permission, existing.ref_num
            );
        }
        return existing.ref_num;
    }
    // Inside Macintosh: More Macintosh Toolbox, FSpOpenResFile: opening a
    // malformed resource map fails with mapReadErr. Raw File Manager writes
    // may replace a previously valid map with arbitrary bytes.
    if let Some(index) = ppc_vfs_resource_file_index(vfs_resource_files, &path) {
        if vfs_resource_files[index]
            .raw_data
            .as_ref()
            .is_some_and(|bytes| ResourceFork::parse(bytes).is_none())
        {
            *last_resource_error = PPC_MAP_READ_ERR;
            return -1;
        }
    }
    ppc_materialize_resource_records_for_path(vfs_resource_files, vfs_resources, &path);
    ppc_materialize_quilt_resources_for_existing_path(
        vfs_files,
        vfs_resource_files,
        vfs_resources,
        &path,
    );
    let ref_num = *next_file_ref_num;
    let Some(next_ref_num) = next_file_ref_num.checked_add(1) else {
        *last_resource_error = PPC_PARAM_ERR;
        return -1;
    };
    resource_files.push(PpcResourceFileRecord {
        ref_num,
        path: path.clone(),
    });
    ppc_rebind_resources_for_path(vfs_resources, &path, ref_num);
    *next_file_ref_num = next_ref_num;
    ppc_set_current_resource_refnum(memory, current_resource_refnum, ref_num);
    *last_resource_error = PPC_NO_ERR;
    if ppc_hle_trace_enabled() {
        let matching_resources = vfs_resources
            .iter()
            .filter(|resource| resource.path.eq_ignore_ascii_case(&path))
            .collect::<Vec<_>>();
        eprintln!(
            "[PPC-TRACE] FSpOpenResFile path=\"{}\" permission={} -> ref={} resources={}",
            path,
            _permission,
            ref_num,
            matching_resources.len()
        );
        for resource in matching_resources {
            eprintln!(
                "[PPC-TRACE]   resource '{}' id={} size={} attrs=${:02X} raw_size={}",
                ppc_res_type_text(resource.res_type),
                resource.res_id,
                resource.data.len(),
                resource.attrs,
                resource.raw_data.as_ref().map_or(0, Vec::len)
            );
        }
    }
    ref_num
}

pub(super) fn ppc_open_res_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    next_file_ref_num: &mut i16,
    current_resource_refnum: &mut i16,
    last_resource_error: &mut i16,
    launched_app_path: Option<&str>,
) -> i16 {
    let name_ptr = cpu.gpr[3];
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, name_ptr) else {
        *last_resource_error = PPC_PARAM_ERR;
        return -1;
    };
    let normalized_name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if normalized_name.is_empty() {
        *last_resource_error = PPC_PARAM_ERR;
        return -1;
    }
    let path = if normalized_name.contains('/') {
        ppc_vfs_file_or_resource_path_by_suffix(vfs_files, vfs_resource_files, &normalized_name)
            .or_else(|| {
                ppc_vfs_file_or_resource_path_by_parent_basename(
                    vfs_files,
                    vfs_resource_files,
                    &normalized_name,
                )
            })
            .unwrap_or(normalized_name)
    } else {
        ppc_vfs_file_or_resource_path_by_suffix_or_unique_basename(
            vfs_files,
            vfs_resource_files,
            &normalized_name,
        )
        .or_else(|| {
            ppc_vfs_file_or_resource_path_by_nearest_basename(
                vfs_files,
                vfs_resource_files,
                &normalized_name,
                launched_app_path,
            )
        })
        .unwrap_or(normalized_name)
    };
    ppc_open_resource_path(
        memory,
        vfs_files,
        vfs_resource_files,
        resource_files,
        vfs_resources,
        next_file_ref_num,
        current_resource_refnum,
        last_resource_error,
        path,
        true,
        "OpenResFile",
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_h_open_res_file(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    next_file_ref_num: &mut i16,
    current_resource_refnum: &mut i16,
    last_resource_error: &mut i16,
    default_dir_id: u32,
) -> i16 {
    // Inside Macintosh: More Macintosh Toolbox (1993), pp. 1-62--1-64:
    // HOpenResFile identifies the resource fork by volume, directory ID, and
    // Pascal file name, returning -1 with the error available via ResError.
    let vref = cpu.gpr[3] as u16 as i16;
    if !matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        *last_resource_error = PPC_NSV_ERR;
        return -1;
    }
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, cpu.gpr[5]) else {
        *last_resource_error = PPC_PARAM_ERR;
        return -1;
    };
    let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if name.is_empty() {
        *last_resource_error = PPC_PARAM_ERR;
        return -1;
    }
    let dir_id = ppc_resolve_directory_id(vref, cpu.gpr[4], default_dir_id);
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) else {
        *last_resource_error = PPC_DIR_NF_ERR;
        return -1;
    };
    let requested_path = ppc_join_vfs_path(parent_path, &name);
    let path = ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &requested_path)
        .unwrap_or(requested_path);
    ppc_open_resource_path(
        memory,
        vfs_files,
        vfs_resource_files,
        resource_files,
        vfs_resources,
        next_file_ref_num,
        current_resource_refnum,
        last_resource_error,
        path,
        false,
        "HOpenResFile",
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_open_resource_path(
    memory: &mut PpcSectionMem,
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    next_file_ref_num: &mut i16,
    current_resource_refnum: &mut i16,
    last_resource_error: &mut i16,
    path: String,
    make_existing_current: bool,
    trace_name: &str,
) -> i16 {
    let resolved_path = if let Some(index) = ppc_vfs_resource_file_index(vfs_resource_files, &path)
    {
        vfs_resource_files[index].path.clone()
    } else {
        path
    };
    let path = resolved_path;
    let data_file_exists = vfs_files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case(&path));
    if ppc_vfs_resource_file_index(vfs_resource_files, &path).is_none() {
        if !data_file_exists
            && ppc_materialize_unique_named_resource_file(
                vfs_files,
                vfs_resource_files,
                vfs_resources,
                &path,
            )
        {
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] {} path=\"{}\" materialized named resource",
                    trace_name, path
                );
            }
        } else {
            *last_resource_error = if data_file_exists {
                PPC_RES_F_NOT_FOUND_ERR
            } else {
                PPC_FNF_ERR
            };
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] {} path=\"{}\" -> {}",
                    trace_name, path, *last_resource_error
                );
            }
            return -1;
        }
    }
    if let Some(existing) = resource_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&path))
    {
        if make_existing_current {
            ppc_set_current_resource_refnum(memory, current_resource_refnum, existing.ref_num);
        }
        ppc_rebind_resources_for_path(vfs_resources, &path, existing.ref_num);
        *last_resource_error = PPC_NO_ERR;
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] {} path=\"{}\" -> ref={} existing",
                trace_name, path, existing.ref_num
            );
        }
        return existing.ref_num;
    }
    ppc_materialize_resource_records_for_path(vfs_resource_files, vfs_resources, &path);
    ppc_materialize_quilt_resources_for_existing_path(
        vfs_files,
        vfs_resource_files,
        vfs_resources,
        &path,
    );
    let ref_num = *next_file_ref_num;
    let Some(next_ref_num) = next_file_ref_num.checked_add(1) else {
        *last_resource_error = PPC_PARAM_ERR;
        return -1;
    };
    resource_files.push(PpcResourceFileRecord {
        ref_num,
        path: path.clone(),
    });
    ppc_rebind_resources_for_path(vfs_resources, &path, ref_num);
    *next_file_ref_num = next_ref_num;
    ppc_set_current_resource_refnum(memory, current_resource_refnum, ref_num);
    *last_resource_error = PPC_NO_ERR;
    if ppc_hle_trace_enabled() {
        let resource_count = vfs_resources
            .iter()
            .filter(|resource| resource.path.eq_ignore_ascii_case(&path))
            .count();
        eprintln!(
            "[PPC-TRACE] {} path=\"{}\" -> ref={} resources={}",
            trace_name, path, ref_num, resource_count
        );
    }
    ref_num
}

pub(super) fn ppc_close_res_file(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: &mut i16,
    last_resource_error: &mut i16,
) {
    let ref_num = cpu.gpr[3] as u16 as i16;
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
    if *last_resource_error != PPC_NO_ERR {
        return;
    }
    // Inside Macintosh, Volume I (1985), p. I-115: CloseResFile updates the
    // file, then calls ReleaseResource for every resource in that file.
    // Detached resources have no associated record handle and remain owned by
    // the application.
    let resource_handles: Vec<u32> = vfs_resources
        .iter()
        .filter(|resource| resource.ref_num == ref_num && resource.handle != 0)
        .map(|resource| resource.handle)
        .collect();
    for handle in resource_handles {
        let _ = ppc_dispose_process_native_handle(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        );
    }
    resource_files.retain(|file| file.ref_num != ref_num);
    for resource in vfs_resources
        .iter_mut()
        .filter(|resource| resource.ref_num == ref_num)
    {
        resource.handle = 0;
        resource.ref_num = PPC_CLOSED_RESOURCE_REF_NUM;
    }
    if ref_num == *current_resource_refnum {
        let replacement = resource_files
            .last()
            .map(|file| file.ref_num)
            .unwrap_or_default();
        ppc_set_current_resource_refnum(memory, current_resource_refnum, replacement);
    }
    *last_resource_error = PPC_NO_ERR;
}

pub(super) fn ppc_get_eof(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    files: &[PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> i16 {
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    let eof_out_ptr = cpu.gpr[4];
    let Some(file) = files.iter().find(|file| file.ref_num == ref_num) else {
        return PPC_RF_NUM_ERR;
    };
    let eof = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&file.path))
        .map(|record| u32::try_from(record.data.len()).unwrap_or(u32::MAX))
        .unwrap_or(0);
    if eof_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, eof_out_ptr, 4) {
        PPC_PARAM_ERR
    } else {
        let _ = memory.write_u32_be(eof_out_ptr, eof);
        PPC_NO_ERR
    }
}

pub(super) fn ppc_alloc_contig(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    files: &[PpcFileRecord],
    writable_refnums: &HashSet<u16>,
) -> i16 {
    // Files (1992), pp. 2-119--2-120: AllocContig reserves physical file
    // blocks without changing logical EOF. The virtual filesystem has no
    // physical allocation map, so a valid open fork can satisfy the request.
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    let count_ptr = cpu.gpr[4];
    if !files.iter().any(|file| file.ref_num == ref_num) {
        return PPC_RF_NUM_ERR;
    }
    if !writable_refnums.contains(&(ref_num as u16)) {
        return PPC_WR_PERM_ERR;
    }
    if count_ptr == 0 || !ppc_memory_can_write_bytes(memory, count_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(super) fn ppc_set_eof(
    cpu: &mut PpcCpu,
    files: &mut [PpcFileRecord],
    writable_refnums: &HashSet<u16>,
    vfs_files: &mut [PpcVfsFileRecord],
) -> i16 {
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    let new_eof = cpu.gpr[4];
    let Some(path) = files
        .iter()
        .find(|file| file.ref_num == ref_num)
        .map(|file| file.path.clone())
    else {
        return PPC_RF_NUM_ERR;
    };
    if !writable_refnums.contains(&(ref_num as u16)) {
        return PPC_WR_PERM_ERR;
    }
    let Some(vfs_file) = vfs_files
        .iter_mut()
        .find(|record| record.path.eq_ignore_ascii_case(&path))
    else {
        return PPC_FNF_ERR;
    };
    let Ok(new_len) = usize::try_from(new_eof) else {
        return PPC_PARAM_ERR;
    };
    vfs_file.data.with_mut(|data| data.resize(new_len, 0));
    vfs_file.dirty = true;
    for file in files
        .iter_mut()
        .filter(|file| file.path.eq_ignore_ascii_case(&path))
    {
        if file.position > new_eof {
            file.position = new_eof;
        }
    }
    PPC_NO_ERR
}

pub(super) fn ppc_get_fpos(cpu: &mut PpcCpu, memory: &mut PpcSectionMem, files: &[PpcFileRecord]) -> i16 {
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    let pos_out_ptr = cpu.gpr[4];
    let Some(file) = files.iter().find(|file| file.ref_num == ref_num) else {
        return PPC_RF_NUM_ERR;
    };
    if pos_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, pos_out_ptr, 4) {
        PPC_PARAM_ERR
    } else {
        let _ = memory.write_u32_be(pos_out_ptr, file.position);
        PPC_NO_ERR
    }
}

pub(super) fn ppc_set_fpos(
    cpu: &mut PpcCpu,
    files: &mut [PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> i16 {
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    let pos_mode = cpu.gpr[4] as u16;
    let pos_offset = cpu.gpr[5] as i32 as i64;
    let Some(file) = files.iter_mut().find(|file| file.ref_num == ref_num) else {
        return PPC_RF_NUM_ERR;
    };
    let eof = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&file.path))
        .map_or(0i64, |record| {
            i64::try_from(record.data.len()).unwrap_or(i64::MAX)
        });
    let current = i64::from(file.position);
    let new_pos = match pos_mode {
        0 => current,
        1 => pos_offset,
        2 => eof.saturating_add(pos_offset),
        3 => current.saturating_add(pos_offset),
        _ => return PPC_PARAM_ERR,
    };
    let Ok(new_pos) = u32::try_from(new_pos) else {
        return PPC_PARAM_ERR;
    };
    file.position = new_pos;
    PPC_NO_ERR
}

pub(super) fn ppc_fs_read(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut [PpcFileRecord],
    vfs_files: &[PpcVfsFileRecord],
) -> i16 {
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    let count_ptr = cpu.gpr[4];
    let buffer_ptr = cpu.gpr[5];
    if count_ptr == 0 || !ppc_memory_can_write_bytes(memory, count_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    let Some(requested_count) = memory.read_u32_be(count_ptr) else {
        return PPC_PARAM_ERR;
    };
    let Some(file) = files.iter_mut().find(|file| file.ref_num == ref_num) else {
        return PPC_PARAM_ERR;
    };
    if requested_count > 0 && buffer_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(vfs_file) = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(&file.path))
    else {
        return PPC_FNF_ERR;
    };
    let start = usize::try_from(file.position)
        .unwrap_or(usize::MAX)
        .min(vfs_file.data.len());
    let requested_len = usize::try_from(requested_count).unwrap_or(usize::MAX);
    let available = vfs_file.data.len().saturating_sub(start);
    let read_len = requested_len.min(available);
    if read_len > 0
        && !ppc_memory_can_write_bytes(
            memory,
            buffer_ptr,
            u32::try_from(read_len).unwrap_or(u32::MAX),
        )
    {
        return PPC_PARAM_ERR;
    }
    for (offset, byte) in vfs_file.data[start..start + read_len]
        .iter()
        .copied()
        .enumerate()
    {
        let Ok(offset) = u32::try_from(offset) else {
            return PPC_PARAM_ERR;
        };
        if memory.write_u8(buffer_ptr + offset, byte).is_none() {
            return PPC_PARAM_ERR;
        };
    }
    let read_count = u32::try_from(read_len).unwrap_or(u32::MAX);
    file.position = file.position.saturating_add(read_count);
    let _ = memory.write_u32_be(count_ptr, read_count);
    if std::env::var_os("SYSTEMLESS_PPC_FILE_TRACE").is_some() {
        eprintln!("[PPC-FILE-TRACE] FSRead path=\"{}\" start={} requested={} read={} result={}", vfs_file.path, start, requested_len, read_len, if read_count < requested_count { PPC_EOF_ERR } else { PPC_NO_ERR });
    }
    if read_count < requested_count {
        PPC_EOF_ERR
    } else {
        PPC_NO_ERR
    }
}

pub(super) fn ppc_fs_write(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    files: &mut [PpcFileRecord],
    writable_refnums: &HashSet<u16>,
    vfs_files: &mut Vec<PpcVfsFileRecord>,
) -> i16 {
    let ref_num = ppc_ref_num_from_gpr(cpu.gpr[3]);
    let count_ptr = cpu.gpr[4];
    let buffer_ptr = cpu.gpr[5];
    if count_ptr == 0 || !ppc_memory_can_write_bytes(memory, count_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    let Some(requested_count) = memory.read_u32_be(count_ptr) else {
        return PPC_PARAM_ERR;
    };
    let Some(file) = files.iter_mut().find(|file| file.ref_num == ref_num) else {
        return PPC_RF_NUM_ERR;
    };
    if requested_count > 0 && buffer_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    if requested_count > 0 && !ppc_memory_can_read_bytes(memory, buffer_ptr, requested_count) {
        return PPC_PARAM_ERR;
    }
    if !writable_refnums.contains(&(ref_num as u16)) {
        return PPC_WR_PERM_ERR;
    }
    let Some(vfs_file) = ppc_vfs_file_mut(vfs_files, &file.path) else {
        return PPC_FNF_ERR;
    };
    let Ok(start) = usize::try_from(file.position) else {
        return PPC_PARAM_ERR;
    };
    let Ok(requested_len) = usize::try_from(requested_count) else {
        return PPC_PARAM_ERR;
    };
    let Some(end) = start.checked_add(requested_len) else {
        return PPC_PARAM_ERR;
    };
    let bytes = ppc_memory_read_bytes(memory, buffer_ptr, requested_count).unwrap_or_default();
    vfs_file.data.with_mut(|data| {
        if start > data.len() {
            data.resize(start, 0);
        }
        if end > data.len() {
            data.resize(end, 0);
        }
        data[start..end].copy_from_slice(&bytes);
    });
    vfs_file.dirty = true;
    file.position = file.position.saturating_add(requested_count);
    let _ = memory.write_u32_be(count_ptr, requested_count);
    PPC_NO_ERR
}

pub(super) fn ppc_fsp_create(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let creator = cpu.gpr[4];
    let file_type = cpu.gpr[5];
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => return err,
    };
    if ppc_vfs_file_index(vfs_files, &path).is_some() {
        return PPC_DUP_FN_ERR;
    }
    vfs_files.push(PpcVfsFileRecord {
        path,
        data: Vec::new().into(),
        creator,
        file_type,
        finder_flags: 0,
        dirty: true,
    });
    PPC_NO_ERR
}

pub(super) fn ppc_h_create(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    default_dir_id: u32,
) -> i16 {
    // Inside Macintosh: Files (1992), 2-169: HCreate identifies the parent
    // with a volume reference and directory ID, then supplies creator/type.
    ppc_create_data_fork_by_name(
        memory,
        vfs_directories,
        vfs_files,
        default_dir_id,
        cpu.gpr[3] as u16 as i16,
        cpu.gpr[4],
        cpu.gpr[5],
        cpu.gpr[6],
        cpu.gpr[7],
    )
}

pub(super) fn ppc_renamed_path(path: &str, old_path: &str, new_path: &str) -> Option<String> {
    if path.eq_ignore_ascii_case(old_path) {
        return Some(new_path.to_string());
    }
    let prefix = path.get(..old_path.len())?;
    let suffix = path.get(old_path.len()..)?;
    (prefix.eq_ignore_ascii_case(old_path) && suffix.starts_with('/'))
        .then(|| format!("{new_path}{suffix}"))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_h_rename(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &mut Vec<PpcVfsDirectory>,
    vfs_files: &mut ProcessVfsFileRecords,
    deleted_vfs_file_paths: &mut Vec<String>,
    files: &mut Vec<PpcFileRecord>,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    default_dir_id: u32,
) -> i16 {
    // Inside Macintosh: Files (1992), pp. 2-178--2-179:
    // FUNCTION HRename (vRefNum: Integer; dirID: LongInt;
    //                   oldName: Str255; newName: Str255): OSErr;
    // Rename within the source directory; open access paths remain valid.
    let vref = cpu.gpr[3] as u16 as i16;
    if !matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        return PPC_NSV_ERR;
    }
    let (Some(old_bytes), Some(new_bytes)) = (
        ppc_read_pstring_bytes(memory, cpu.gpr[5]),
        ppc_read_pstring_bytes(memory, cpu.gpr[6]),
    ) else {
        return PPC_PARAM_ERR;
    };
    let old_name = ppc_normalize_vfs_path(&decode_mac_roman(&old_bytes));
    let new_name = decode_mac_roman(&new_bytes);
    if old_name.is_empty() || new_name.is_empty() {
        return PPC_PARAM_ERR;
    }
    if new_name.contains([':', '/']) || matches!(new_name.as_str(), "." | "..") {
        return PPC_BD_NAM_ERR;
    }
    let dir_id = ppc_resolve_directory_id(vref, cpu.gpr[4], default_dir_id);
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) else {
        return PPC_DIR_NF_ERR;
    };
    let requested_path = ppc_join_vfs_path(parent_path, &old_name);
    let old_path = ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &requested_path)
        .or_else(|| {
            vfs_directories
                .iter()
                .find(|directory| directory.path.eq_ignore_ascii_case(&requested_path))
                .map(|directory| directory.path.clone())
        });
    let Some(old_path) = old_path else {
        return PPC_FNF_ERR;
    };
    let parent = old_path.rsplit_once('/').map_or("", |(parent, _)| parent);
    let new_path = ppc_join_vfs_path(parent, &new_name);
    if old_path.eq_ignore_ascii_case(&new_path) {
        return PPC_NO_ERR;
    }
    if ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &new_path).is_some()
        || vfs_directories
            .iter()
            .any(|directory| directory.path.eq_ignore_ascii_case(&new_path))
    {
        return PPC_DUP_FN_ERR;
    }

    let data_moves = vfs_files
        .iter()
        .filter_map(|file| {
            ppc_renamed_path(&file.path, &old_path, &new_path)
                .map(|new| (file.path.clone(), new))
        })
        .collect::<Vec<_>>();
    let resource_moves = vfs_resource_files
        .iter()
        .filter_map(|file| {
            ppc_renamed_path(&file.path, &old_path, &new_path)
                .map(|new| (file.path.clone(), new))
        })
        .collect::<Vec<_>>();
    for (old, new) in &data_moves {
        vfs_files.rename_path(old, new);
    }
    for (old, new) in &resource_moves {
        vfs_resource_files.rename_path(old, new);
    }
    for (old, _) in data_moves.iter().chain(resource_moves.iter()) {
        if !deleted_vfs_file_paths
            .iter()
            .any(|deleted| deleted.eq_ignore_ascii_case(old))
        {
            deleted_vfs_file_paths.push(old.clone());
        }
    }
    for directory in vfs_directories.iter_mut() {
        if let Some(renamed) = ppc_renamed_path(&directory.path, &old_path, &new_path) {
            directory.path = renamed;
            directory.dirty = true;
        }
    }
    for file in files.iter_mut() {
        if let Some(renamed) = ppc_renamed_path(&file.path, &old_path, &new_path) {
            file.path = renamed;
        }
    }
    for file in resource_files.iter_mut() {
        if let Some(renamed) = ppc_renamed_path(&file.path, &old_path, &new_path) {
            file.path = renamed;
        }
    }
    for resource in vfs_resources.iter_mut() {
        if let Some(renamed) = ppc_renamed_path(&resource.path, &old_path, &new_path) {
            resource.path = renamed;
        }
    }
    PPC_NO_ERR
}

pub(super) fn ppc_pb_create(
    operation: PpcParameterBlockCreateOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    default_dir_id: u32,
) -> i16 {
    let pb = cpu.gpr[3];
    if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 24) {
        return PPC_PARAM_ERR;
    }
    let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let Some(vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
        return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
    };
    let hierarchical = matches!(operation, PpcParameterBlockCreateOperation::Hierarchical);
    let dir_id = if hierarchical {
        let Some(dir_id) = memory.read_u32_be(pb + 48) else {
            return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
        };
        dir_id
    } else {
        0
    };
    // Inside Macintosh: Files (1992), 2-186 through 2-187: PBHCreate
    // creates closed, empty data and resource forks in ioDirID.
    let result = ppc_create_data_fork_by_name(
        memory,
        vfs_directories,
        vfs_files,
        default_dir_id,
        vref,
        dir_id,
        name_ptr,
        0,
        0,
    );
    ppc_complete_pb(memory, pb, result)
}

pub(super) fn ppc_create(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    default_dir_id: u32,
) -> i16 {
    // The non-hierarchical Create call resolves against the current directory.
    ppc_create_data_fork_by_name(
        memory,
        vfs_directories,
        vfs_files,
        default_dir_id,
        cpu.gpr[4] as u16 as i16,
        0,
        cpu.gpr[3],
        cpu.gpr[5],
        cpu.gpr[6],
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_create_data_fork_by_name(
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    default_dir_id: u32,
    vref: i16,
    dir_id: u32,
    name_ptr: u32,
    creator: u32,
    file_type: u32,
) -> i16 {
    if !matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        return PPC_NSV_ERR;
    }
    let Some(name_bytes) = ppc_read_pstring_bytes(memory, name_ptr) else {
        return PPC_PARAM_ERR;
    };
    let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
    if name.is_empty() {
        return PPC_PARAM_ERR;
    }
    let effective_dir_id = ppc_resolve_directory_id(vref, dir_id, default_dir_id);
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, effective_dir_id) else {
        return PPC_DIR_NF_ERR;
    };
    let path = ppc_join_vfs_path(parent_path, &name);
    if ppc_vfs_file_index(vfs_files, &path).is_some() {
        return PPC_DUP_FN_ERR;
    }
    vfs_files.push(PpcVfsFileRecord {
        path,
        data: Vec::new().into(),
        creator,
        file_type,
        finder_flags: 0,
        dirty: true,
    });
    PPC_NO_ERR
}

pub(super) fn ppc_fsp_delete(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    deleted_vfs_file_paths: &mut Vec<String>,
    files: &mut Vec<PpcFileRecord>,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => return err,
    };
    ppc_delete_vfs_path(
        &path,
        vfs_files,
        deleted_vfs_file_paths,
        files,
        vfs_resource_files,
        resource_files,
        vfs_resources,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_delete_by_name(
    operation: PpcDeleteByNameOperation,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &mut ProcessVfsFileRecords,
    deleted_vfs_file_paths: &mut Vec<String>,
    files: &mut Vec<PpcFileRecord>,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    default_dir_id: u32,
) -> i16 {
    let (vref, dir_id, name_ptr, pb) = if matches!(
        operation,
        PpcDeleteByNameOperation::LegacyParameterBlock
            | PpcDeleteByNameOperation::HierarchicalParameterBlock
    ) {
        let pb = cpu.gpr[3];
        if pb == 0 || !ppc_memory_can_write_bytes(memory, pb, 24) {
            return PPC_PARAM_ERR;
        }
        let Some(name_ptr) = memory.read_u32_be(pb + 18) else {
            return PPC_PARAM_ERR;
        };
        let Some(vref) = memory.read_u16_be(pb + 22).map(|value| value as i16) else {
            return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
        };
        let hierarchical = matches!(
            operation,
            PpcDeleteByNameOperation::HierarchicalParameterBlock
        );
        let dir_id = if hierarchical {
            let Some(dir_id) = memory.read_u32_be(pb + 48) else {
                return ppc_complete_pb(memory, pb, PPC_PARAM_ERR);
            };
            dir_id
        } else {
            0
        };
        (vref, dir_id, name_ptr, Some(pb))
    } else if matches!(operation, PpcDeleteByNameOperation::HierarchicalHighLevel) {
        (cpu.gpr[3] as u16 as i16, cpu.gpr[4], cpu.gpr[5], None)
    } else {
        (cpu.gpr[4] as u16 as i16, 0, cpu.gpr[3], None)
    };

    // Inside Macintosh: Files (1992), 2-102 and 2-189: PBDelete/PBHDelete
    // identify both forks by name; HDelete's high-level ABI is on 2-173.
    let result = if !matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        PPC_NSV_ERR
    } else if let Some(name_bytes) = ppc_read_pstring_bytes(memory, name_ptr) {
        let name = ppc_normalize_vfs_path(&decode_mac_roman(&name_bytes));
        if name.is_empty() {
            PPC_PARAM_ERR
        } else {
            let effective_dir_id = ppc_resolve_directory_id(vref, dir_id, default_dir_id);
            if let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, effective_dir_id)
            {
                let requested_path = ppc_join_vfs_path(parent_path, &name);
                let path =
                    ppc_vfs_file_or_resource_path(vfs_files, vfs_resource_files, &requested_path)
                        .or_else(|| {
                            ppc_vfs_file_or_resource_path_by_basename(
                                vfs_files,
                                vfs_resource_files,
                                &name,
                            )
                        });
                if let Some(path) = path {
                    ppc_delete_vfs_path(
                        &path,
                        vfs_files,
                        deleted_vfs_file_paths,
                        files,
                        vfs_resource_files,
                        resource_files,
                        vfs_resources,
                    )
                } else {
                    PPC_FNF_ERR
                }
            } else {
                PPC_DIR_NF_ERR
            }
        }
    } else {
        PPC_PARAM_ERR
    };
    pb.map_or(result, |pb| ppc_complete_pb(memory, pb, result))
}

pub(super) fn ppc_delete_vfs_path(
    path: &str,
    vfs_files: &mut ProcessVfsFileRecords,
    deleted_vfs_file_paths: &mut Vec<String>,
    files: &mut Vec<PpcFileRecord>,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    resource_files: &mut Vec<PpcResourceFileRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> i16 {
    let existed = vfs_files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case(path))
        || vfs_resource_files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case(path))
        || vfs_resources
            .iter()
            .any(|resource| resource.path.eq_ignore_ascii_case(path));
    if !existed {
        return PPC_FNF_ERR;
    }
    vfs_files.retain(|file| !file.path.eq_ignore_ascii_case(path));
    if !deleted_vfs_file_paths
        .iter()
        .any(|deleted| deleted.eq_ignore_ascii_case(&path))
    {
        deleted_vfs_file_paths.push(path.to_string());
    }
    files.retain(|file| !file.path.eq_ignore_ascii_case(&path));
    vfs_resource_files.retain(|file| !file.path.eq_ignore_ascii_case(&path));
    resource_files.retain(|file| !file.path.eq_ignore_ascii_case(&path));
    vfs_resources.retain(|resource| !resource.path.eq_ignore_ascii_case(&path));
    PPC_NO_ERR
}

pub(super) fn ppc_ref_num_from_gpr(value: u32) -> i16 {
    value as u16 as i16
}

pub(super) fn initial_ppc_vfs_directories() -> Vec<PpcVfsDirectory> {
    vec![
        PpcVfsDirectory {
            dir_id: PPC_ROOT_DIR_ID,
            parent_dir_id: 1,
            path: String::new(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        },
        PpcVfsDirectory {
            dir_id: PPC_SYSTEM_FOLDER_DIR_ID,
            parent_dir_id: PPC_ROOT_DIR_ID,
            path: "System Folder".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        },
        PpcVfsDirectory {
            dir_id: PPC_PREFERENCES_DIR_ID,
            parent_dir_id: PPC_SYSTEM_FOLDER_DIR_ID,
            path: "System Folder/Preferences".to_string(),
            creator: PPC_DIRECTORY_CREATOR,
            file_type: PPC_DIRECTORY_FILE_TYPE,
            finder_flags: 0,
            dirty: false,
        },
    ]
}

pub(super) fn ppc_dir_create(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &mut Vec<PpcVfsDirectory>,
    next_vfs_dir_id: &mut u32,
    default_dir_id: u32,
) -> i16 {
    let vref = cpu.gpr[3] as u16 as i16;
    let parent_dir_id = ppc_resolve_directory_id(vref, cpu.gpr[4], default_dir_id);
    let directory_name_ptr = cpu.gpr[5];
    let created_dir_id_ptr = cpu.gpr[6];
    if directory_name_ptr == 0 || created_dir_id_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(directory_name) = ppc_read_pstring(memory, directory_name_ptr) else {
        return PPC_PARAM_ERR;
    };
    let normalized_name = ppc_normalize_vfs_path(&directory_name);
    if normalized_name.is_empty() {
        return PPC_PARAM_ERR;
    }
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, parent_dir_id) else {
        return PPC_FNF_ERR;
    };
    let path = ppc_join_vfs_path(parent_path, &normalized_name);
    if let Some(existing) = ppc_directory_id_for_path(vfs_directories, &path) {
        let _ = memory.write_u32_be(created_dir_id_ptr, existing);
        return PPC_DUP_FN_ERR;
    }

    let dir_id = *next_vfs_dir_id;
    let Some(next_dir_id) = next_vfs_dir_id.checked_add(1) else {
        return PPC_PARAM_ERR;
    };
    if memory.write_u32_be(created_dir_id_ptr, dir_id).is_none() {
        return PPC_PARAM_ERR;
    }
    *next_vfs_dir_id = next_dir_id;
    vfs_directories.push(PpcVfsDirectory {
        dir_id,
        parent_dir_id,
        path,
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        finder_flags: 0,
        dirty: true,
    });
    PPC_NO_ERR
}

pub(super) fn ppc_fsp_dir_create(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &mut Vec<PpcVfsDirectory>,
    next_vfs_dir_id: &mut u32,
    default_dir_id: u32,
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let created_dir_id_ptr = cpu.gpr[5];
    if spec_ptr == 0 || created_dir_id_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let Some((vref, requested_dir_id, directory_name)) = ppc_read_fsspec_parts(memory, spec_ptr)
    else {
        return PPC_PARAM_ERR;
    };
    if directory_name.is_empty() {
        return PPC_PARAM_ERR;
    }
    let parent_dir_id = ppc_resolve_directory_id(vref, requested_dir_id, default_dir_id);
    let normalized_name = ppc_normalize_vfs_path(&decode_mac_roman(&directory_name));
    if normalized_name.is_empty() {
        return PPC_PARAM_ERR;
    }
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, parent_dir_id) else {
        return PPC_FNF_ERR;
    };
    let path = ppc_join_vfs_path(parent_path, &normalized_name);
    if let Some(existing) = ppc_directory_id_for_path(vfs_directories, &path) {
        let _ = memory.write_u32_be(created_dir_id_ptr, existing);
        return PPC_DUP_FN_ERR;
    }

    let dir_id = *next_vfs_dir_id;
    let Some(next_dir_id) = next_vfs_dir_id.checked_add(1) else {
        return PPC_PARAM_ERR;
    };
    if memory.write_u32_be(created_dir_id_ptr, dir_id).is_none() {
        return PPC_PARAM_ERR;
    }
    *next_vfs_dir_id = next_dir_id;
    vfs_directories.push(PpcVfsDirectory {
        dir_id,
        parent_dir_id,
        path,
        creator: PPC_DIRECTORY_CREATOR,
        file_type: PPC_DIRECTORY_FILE_TYPE,
        finder_flags: 0,
        dirty: true,
    });
    PPC_NO_ERR
}

pub(super) fn ppc_find_folder_dir_id(folder_type: u32) -> u32 {
    if folder_type == u32::from_be_bytes(*b"pref") {
        PPC_PREFERENCES_DIR_ID
    } else {
        PPC_ROOT_DIR_ID
    }
}

pub(super) fn ppc_new_alias(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    aliases: &mut Vec<PpcAliasRecord>,
) -> i16 {
    let _from_file_ptr = cpu.gpr[3];
    let target_ptr = cpu.gpr[4];
    let alias_out_ptr = cpu.gpr[5];
    if target_ptr == 0 || alias_out_ptr == 0 {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, alias_out_ptr, 4) {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };
    let Some((target_vref, target_dir_id, target_name)) = ppc_read_fsspec_parts(memory, target_ptr)
    else {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };
    let Some(alias_data) = ppc_alias_record_bytes(target_vref, target_dir_id, &target_name) else {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };

    let handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        &alias_data,
    );
    if handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return PPC_MEM_FULL_ERR;
    }
    if memory.write_u32_be(alias_out_ptr, handle).is_none() {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    }
    aliases.push(PpcAliasRecord {
        handle,
        target_vref: target_vref as i16,
        target_dir_id,
        target_name,
    });
    *last_mem_error = PPC_NO_ERR;
    PPC_NO_ERR
}

pub(super) fn ppc_write_fsspec_parts(
    memory: &mut PpcSectionMem,
    spec_ptr: u32,
    vref: i16,
    dir_id: u32,
    name: &[u8],
) -> bool {
    if spec_ptr == 0 || name.len() > PPC_FSSPEC_MAX_NAME_LEN {
        return false;
    }
    ppc_write_fsspec(memory, spec_ptr, vref, dir_id, name).is_some()
}

pub(super) fn ppc_read_fsspec_parts(memory: &mut PpcSectionMem, spec_ptr: u32) -> Option<(i16, u32, Vec<u8>)> {
    if spec_ptr == 0 {
        return None;
    }
    let vref = memory.read_u16_be(spec_ptr)? as i16;
    let dir_id = memory.read_u32_be(spec_ptr + 2)?;
    let name_len = usize::from(memory.read_u8(spec_ptr + 6)?);
    if name_len > PPC_FSSPEC_MAX_NAME_LEN {
        return None;
    }
    let mut name = Vec::with_capacity(name_len);
    for offset in 0..name_len {
        name.push(memory.read_u8(spec_ptr.checked_add(7 + offset as u32)?)?);
    }
    Some((vref, dir_id, name))
}

pub(super) fn ppc_alias_record_bytes(vref: i16, dir_id: u32, name: &[u8]) -> Option<Vec<u8>> {
    if name.len() > PPC_FSSPEC_MAX_NAME_LEN {
        return None;
    }
    let mut bytes = vec![0; PPC_CLASSIC_ALIAS_RECORD_SIZE];
    bytes[4..6].copy_from_slice(&(PPC_CLASSIC_ALIAS_RECORD_SIZE as u16).to_be_bytes());
    bytes[6..8].copy_from_slice(&PPC_CLASSIC_ALIAS_RECORD_VERSION.to_be_bytes());
    ppc_write_alias_pstring(
        &mut bytes,
        PPC_CLASSIC_ALIAS_VOLUME_NAME_OFFSET,
        27,
        b"Systemless",
    )?;
    ppc_write_alias_pstring(&mut bytes, PPC_CLASSIC_ALIAS_FILE_NAME_OFFSET, 63, name)?;
    bytes[PPC_CLASSIC_ALIAS_DIR_ID_OFFSET..PPC_CLASSIC_ALIAS_DIR_ID_OFFSET + 4]
        .copy_from_slice(&dir_id.to_be_bytes());

    let mystery_words = PPC_CLASSIC_ALIAS_MYSTERY_WORDS_OFFSET;
    bytes[mystery_words..mystery_words + 2].copy_from_slice(&0xffffu16.to_be_bytes());
    bytes[mystery_words + 2..mystery_words + 4].copy_from_slice(&0xffffu16.to_be_bytes());
    bytes[mystery_words + 6..mystery_words + 8].copy_from_slice(&17u16.to_be_bytes());

    let tail = PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE;
    bytes[tail..tail + 2].copy_from_slice(&PPC_CLASSIC_ALIAS_TAIL_TAG.to_be_bytes());
    bytes[tail + 2..tail + 4]
        .copy_from_slice(&(PPC_CLASSIC_ALIAS_TAIL_PAYLOAD_SIZE as u16).to_be_bytes());
    ppc_fill_classic_alias_tail_payload(
        &mut bytes[tail + 4..tail + 4 + PPC_CLASSIC_ALIAS_TAIL_PAYLOAD_SIZE],
    );
    let end = tail + 4 + PPC_CLASSIC_ALIAS_TAIL_PAYLOAD_SIZE;
    bytes[end..end + 2].copy_from_slice(&PPC_CLASSIC_ALIAS_END_TAG.to_be_bytes());

    // Classic AliasRecords are volume/name based. The in-memory side table
    // preserves the exact vRefNum for handles created during this run; decoded
    // archive-style records resolve to the boot volume.
    let _ = vref;
    Some(bytes)
}

#[cfg(test)]
pub(super) fn ppc_legacy_alias_record_bytes(vref: i16, dir_id: u32, name: &[u8]) -> Option<Vec<u8>> {
    if name.len() > PPC_FSSPEC_MAX_NAME_LEN {
        return None;
    }
    let mut bytes = vec![0; PPC_ALIAS_RECORD_SIZE];
    bytes[0..4].copy_from_slice(&PPC_ALIAS_RECORD_MAGIC.to_be_bytes());
    bytes[4..6].copy_from_slice(&(PPC_ALIAS_RECORD_SIZE as u16).to_be_bytes());
    bytes[6..8].copy_from_slice(&PPC_ALIAS_RECORD_VERSION.to_be_bytes());
    bytes[8..10].copy_from_slice(&(PPC_ALIAS_RECORD_FSSPEC_OFFSET as u16).to_be_bytes());
    bytes[10..12].copy_from_slice(&(PPC_FSSPEC_SIZE as u16).to_be_bytes());

    let fsspec = PPC_ALIAS_RECORD_FSSPEC_OFFSET;
    bytes[fsspec..fsspec + 2].copy_from_slice(&(vref as u16).to_be_bytes());
    bytes[fsspec + 2..fsspec + 6].copy_from_slice(&dir_id.to_be_bytes());
    bytes[fsspec + 6] = name.len() as u8;
    bytes[fsspec + 7..fsspec + 7 + name.len()].copy_from_slice(name);
    Some(bytes)
}

pub(super) fn ppc_write_alias_pstring(
    bytes: &mut [u8],
    offset: usize,
    max_len: usize,
    value: &[u8],
) -> Option<()> {
    if value.len() > max_len || offset.checked_add(1 + max_len)? > bytes.len() {
        return None;
    }
    bytes[offset] = value.len() as u8;
    bytes[offset + 1..offset + 1 + value.len()].copy_from_slice(value);
    Some(())
}

pub(super) fn ppc_fill_classic_alias_tail_payload(payload: &mut [u8]) {
    let words = [
        (0usize, 0x00a8u16),
        (1, 0x6166),
        (2, 0x706d),
        (5, 0x0003),
        (6, 0x0018),
        (7, 0x0039),
        (8, 0x0059),
        (9, 0x0075),
        (10, 0x0095),
        (11, 0x009e),
    ];
    for (index, value) in words {
        let offset = index * 2;
        if offset + 2 <= payload.len() {
            payload[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }
    }
    let zone_offset = 24usize;
    if zone_offset + 2 <= payload.len() {
        payload[zone_offset] = 1;
        payload[zone_offset + 1] = b'*';
    }
}

pub(super) fn ppc_alias_record_from_handle(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    alias_handle: u32,
    vfs_directories: Option<&[PpcVfsDirectory]>,
) -> Option<PpcAliasRecord> {
    let handle_record = handles
        .iter()
        .find(|record| record.handle == alias_handle)?;
    let handle_size = usize::try_from(handle_record.size).ok()?;
    let data_ptr = memory.read_u32_be(alias_handle)?;
    ppc_legacy_alias_record_from_data(memory, alias_handle, data_ptr, handle_size).or_else(|| {
        ppc_classic_alias_record_from_data(
            memory,
            alias_handle,
            data_ptr,
            handle_size,
            vfs_directories,
        )
    })
}

pub(super) fn ppc_legacy_alias_record_from_data(
    memory: &mut PpcSectionMem,
    alias_handle: u32,
    data_ptr: u32,
    handle_size: usize,
) -> Option<PpcAliasRecord> {
    let magic = memory.read_u32_be(data_ptr)?;
    if magic != PPC_ALIAS_RECORD_MAGIC {
        return None;
    }
    let record_size = usize::from(memory.read_u16_be(data_ptr + 4)?);
    let version = memory.read_u16_be(data_ptr + 6)?;
    let fsspec_offset = usize::from(memory.read_u16_be(data_ptr + 8)?);
    let fsspec_size = usize::from(memory.read_u16_be(data_ptr + 10)?);
    if version != PPC_ALIAS_RECORD_VERSION
        || record_size > handle_size
        || fsspec_size != PPC_FSSPEC_SIZE
        || fsspec_offset.checked_add(PPC_FSSPEC_SIZE)? > record_size
    {
        return None;
    }
    let fsspec_ptr = data_ptr.checked_add(u32::try_from(fsspec_offset).ok()?)?;
    let (target_vref, target_dir_id, target_name) = ppc_read_fsspec_parts(memory, fsspec_ptr)?;
    Some(PpcAliasRecord {
        handle: alias_handle,
        target_vref,
        target_dir_id,
        target_name,
    })
}

pub(super) fn ppc_classic_alias_record_from_data(
    memory: &mut PpcSectionMem,
    alias_handle: u32,
    data_ptr: u32,
    handle_size: usize,
    vfs_directories: Option<&[PpcVfsDirectory]>,
) -> Option<PpcAliasRecord> {
    if handle_size < PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE {
        return None;
    }
    let record_size = usize::from(memory.read_u16_be(data_ptr + 4)?);
    let version = memory.read_u16_be(data_ptr + 6)?;
    if version != PPC_CLASSIC_ALIAS_RECORD_VERSION
        || record_size < PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE
        || record_size > handle_size
    {
        return None;
    }

    let mut target_dir_id = memory
        .read_u32_be(data_ptr.checked_add(u32::try_from(PPC_CLASSIC_ALIAS_DIR_ID_OFFSET).ok()?)?)?;
    let mut target_name = ppc_read_fixed_pstring_bytes(
        memory,
        data_ptr.checked_add(u32::try_from(PPC_CLASSIC_ALIAS_FILE_NAME_OFFSET).ok()?)?,
        PPC_FSSPEC_MAX_NAME_LEN,
    )?;

    if target_dir_id == u32::MAX || target_name.is_empty() {
        if let Some((full_path_dir_id, full_path_name)) =
            ppc_classic_alias_full_path_target(memory, data_ptr, record_size, vfs_directories)
        {
            target_dir_id = full_path_dir_id;
            target_name = full_path_name;
        }
    }
    if target_dir_id == u32::MAX || target_name.len() > PPC_FSSPEC_MAX_NAME_LEN {
        return None;
    }

    Some(PpcAliasRecord {
        handle: alias_handle,
        target_vref: PPC_BOOT_VOLUME_REF_NUM,
        target_dir_id,
        target_name,
    })
}

pub(super) fn ppc_alias_record_from_bytes(
    bytes: &[u8],
    vfs_directories: Option<&[PpcVfsDirectory]>,
) -> Option<PpcAliasRecord> {
    ppc_legacy_alias_record_from_bytes(bytes)
        .or_else(|| ppc_classic_alias_record_from_bytes(bytes, vfs_directories))
}

pub(super) fn ppc_legacy_alias_record_from_bytes(bytes: &[u8]) -> Option<PpcAliasRecord> {
    let magic = ppc_slice_u32_be(bytes, 0)?;
    if magic != PPC_ALIAS_RECORD_MAGIC {
        return None;
    }
    let record_size = usize::from(ppc_slice_u16_be(bytes, 4)?);
    let version = ppc_slice_u16_be(bytes, 6)?;
    let fsspec_offset = usize::from(ppc_slice_u16_be(bytes, 8)?);
    let fsspec_size = usize::from(ppc_slice_u16_be(bytes, 10)?);
    if version != PPC_ALIAS_RECORD_VERSION
        || record_size > bytes.len()
        || fsspec_size != PPC_FSSPEC_SIZE
        || fsspec_offset.checked_add(PPC_FSSPEC_SIZE)? > record_size
    {
        return None;
    }
    let target_vref = ppc_slice_u16_be(bytes, fsspec_offset)? as i16;
    let target_dir_id = ppc_slice_u32_be(bytes, fsspec_offset + 2)?;
    let target_name =
        ppc_slice_fixed_pstring_bytes(bytes, fsspec_offset + 6, PPC_FSSPEC_MAX_NAME_LEN)?;
    Some(PpcAliasRecord {
        handle: 0,
        target_vref,
        target_dir_id,
        target_name,
    })
}

pub(super) fn ppc_classic_alias_record_from_bytes(
    bytes: &[u8],
    vfs_directories: Option<&[PpcVfsDirectory]>,
) -> Option<PpcAliasRecord> {
    if bytes.len() < PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE {
        return None;
    }
    let record_size = usize::from(ppc_slice_u16_be(bytes, 4)?);
    let version = ppc_slice_u16_be(bytes, 6)?;
    if version != PPC_CLASSIC_ALIAS_RECORD_VERSION
        || record_size < PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE
        || record_size > bytes.len()
    {
        return None;
    }
    let mut target_dir_id = ppc_slice_u32_be(bytes, PPC_CLASSIC_ALIAS_DIR_ID_OFFSET)?;
    let mut target_name = ppc_slice_fixed_pstring_bytes(
        bytes,
        PPC_CLASSIC_ALIAS_FILE_NAME_OFFSET,
        PPC_FSSPEC_MAX_NAME_LEN,
    )?;
    if target_dir_id == u32::MAX || target_name.is_empty() {
        if let Some((full_path_dir_id, full_path_name)) =
            ppc_classic_alias_full_path_target_from_bytes(bytes, record_size, vfs_directories)
        {
            target_dir_id = full_path_dir_id;
            target_name = full_path_name;
        }
    }
    if target_dir_id == u32::MAX || target_name.len() > PPC_FSSPEC_MAX_NAME_LEN {
        return None;
    }
    Some(PpcAliasRecord {
        handle: 0,
        target_vref: PPC_BOOT_VOLUME_REF_NUM,
        target_dir_id,
        target_name,
    })
}

pub(super) fn ppc_classic_alias_full_path_target_from_bytes(
    bytes: &[u8],
    record_size: usize,
    vfs_directories: Option<&[PpcVfsDirectory]>,
) -> Option<(u32, Vec<u8>)> {
    let mut offset = PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE;
    while offset.checked_add(4)? <= record_size {
        let tag = ppc_slice_u16_be(bytes, offset)?;
        if tag == PPC_CLASSIC_ALIAS_END_TAG {
            return None;
        }
        let len = usize::from(ppc_slice_u16_be(bytes, offset + 2)?);
        let payload_offset = offset.checked_add(4)?;
        let payload_end = payload_offset.checked_add(len)?;
        if payload_end > record_size {
            return None;
        }
        if tag == PPC_CLASSIC_ALIAS_FULL_PATH_TAG {
            return ppc_classic_alias_target_from_full_path(
                bytes.get(payload_offset..payload_end)?,
                vfs_directories,
            );
        }
        offset = payload_offset.checked_add((len + 1) & !1)?;
    }
    None
}

pub(super) fn ppc_slice_u16_be(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

pub(super) fn ppc_slice_u32_be(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

pub(super) fn ppc_slice_fixed_pstring_bytes(bytes: &[u8], offset: usize, max_len: usize) -> Option<Vec<u8>> {
    let len = usize::from(*bytes.get(offset)?);
    if len > max_len {
        return None;
    }
    Some(
        bytes
            .get(offset + 1..offset.checked_add(1 + len)?)?
            .to_vec(),
    )
}

pub(super) fn ppc_classic_alias_full_path_target(
    memory: &mut PpcSectionMem,
    data_ptr: u32,
    record_size: usize,
    vfs_directories: Option<&[PpcVfsDirectory]>,
) -> Option<(u32, Vec<u8>)> {
    let mut offset = PPC_CLASSIC_ALIAS_RECORD_HEADER_SIZE;
    while offset.checked_add(4)? <= record_size {
        let tag = memory.read_u16_be(data_ptr.checked_add(u32::try_from(offset).ok()?)?)?;
        if tag == PPC_CLASSIC_ALIAS_END_TAG {
            return None;
        }
        let len = usize::from(
            memory
                .read_u16_be(data_ptr.checked_add(u32::try_from(offset.checked_add(2)?).ok()?)?)?,
        );
        let payload_offset = offset.checked_add(4)?;
        let payload_end = payload_offset.checked_add(len)?;
        if payload_end > record_size {
            return None;
        }
        if tag == PPC_CLASSIC_ALIAS_FULL_PATH_TAG {
            let payload_ptr = data_ptr.checked_add(u32::try_from(payload_offset).ok()?)?;
            let mut path = Vec::with_capacity(len);
            for path_offset in 0..len {
                path.push(memory.read_u8(payload_ptr.checked_add(path_offset as u32)?)?);
            }
            return ppc_classic_alias_target_from_full_path(&path, vfs_directories);
        }
        offset = payload_offset.checked_add((len + 1) & !1)?;
    }
    None
}

pub(super) fn ppc_classic_alias_target_from_full_path(
    path: &[u8],
    vfs_directories: Option<&[PpcVfsDirectory]>,
) -> Option<(u32, Vec<u8>)> {
    let parts = path
        .split(|byte| *byte == b':')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts.len() < 2 {
        return None;
    }
    let target_name = parts.last()?.to_vec();
    if target_name.len() > PPC_FSSPEC_MAX_NAME_LEN {
        return None;
    }
    let parent_parts = &parts[1..parts.len().saturating_sub(1)];
    let parent_path = parent_parts
        .iter()
        .map(|part| decode_mac_roman(part))
        .collect::<Vec<_>>()
        .join("/");
    let normalized_parent_path = ppc_normalize_vfs_path(&parent_path);
    if let Some(vfs_directories) = vfs_directories {
        if let Some(dir_id) = ppc_directory_id_for_path(vfs_directories, &normalized_parent_path) {
            return Some((dir_id, target_name));
        }
    }
    let dir_id = if normalized_parent_path.is_empty() {
        PPC_ROOT_DIR_ID
    } else if normalized_parent_path.eq_ignore_ascii_case("System Folder") {
        PPC_SYSTEM_FOLDER_DIR_ID
    } else if normalized_parent_path.eq_ignore_ascii_case("System Folder/Preferences") {
        PPC_PREFERENCES_DIR_ID
    } else {
        return None;
    };
    Some((dir_id, target_name))
}

pub(super) fn ppc_read_fixed_pstring_bytes(
    memory: &mut PpcSectionMem,
    addr: u32,
    max_len: usize,
) -> Option<Vec<u8>> {
    let len = usize::from(memory.read_u8(addr)?);
    if len > max_len {
        return None;
    }
    let mut bytes = Vec::with_capacity(len);
    for offset in 0..len {
        bytes.push(memory.read_u8(addr.checked_add(1 + offset as u32)?)?);
    }
    Some(bytes)
}

pub(super) fn ppc_resolve_alias(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    handles: &[PpcHandleRecord],
    aliases: &[PpcAliasRecord],
) -> i16 {
    let _from_file_ptr = cpu.gpr[3];
    let alias_handle = cpu.gpr[4];
    let target_ptr = cpu.gpr[5];
    let was_changed_ptr = cpu.gpr[6];
    if alias_handle == 0 || target_ptr == 0 || was_changed_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, target_ptr, PPC_FSSPEC_SIZE as u32)
        || !ppc_memory_can_write_bytes(memory, was_changed_ptr, 1)
    {
        return PPC_PARAM_ERR;
    };
    let alias = aliases
        .iter()
        .find(|record| record.handle == alias_handle)
        .cloned()
        .or_else(|| {
            ppc_alias_record_from_handle(memory, handles, alias_handle, Some(vfs_directories))
        });
    let Some(alias) = alias else {
        return PPC_PARAM_ERR;
    };
    if !ppc_write_fsspec_parts(
        memory,
        target_ptr,
        alias.target_vref,
        alias.target_dir_id,
        &alias.target_name,
    ) || memory.write_u8(was_changed_ptr, 0).is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(super) fn ppc_update_alias(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    last_mem_error: &mut i16,
    vfs_directories: &[PpcVfsDirectory],
    handles: &mut [PpcHandleRecord],
    aliases: &mut Vec<PpcAliasRecord>,
) -> i16 {
    let _from_file_ptr = cpu.gpr[3];
    let target_ptr = cpu.gpr[4];
    let alias_handle = cpu.gpr[5];
    let was_changed_ptr = cpu.gpr[6];
    if target_ptr == 0 || alias_handle == 0 || was_changed_ptr == 0 {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, was_changed_ptr, 1) {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    }
    let Some((target_vref, target_dir_id, target_name)) = ppc_read_fsspec_parts(memory, target_ptr)
    else {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };
    let Some(alias_data) = ppc_alias_record_bytes(target_vref, target_dir_id, &target_name) else {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };
    let Some(handle_index) = handles
        .iter()
        .position(|record| record.handle == alias_handle)
    else {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };
    let existing_alias = aliases
        .iter()
        .find(|record| record.handle == alias_handle)
        .cloned()
        .or_else(|| {
            ppc_alias_record_from_handle(memory, handles, alias_handle, Some(vfs_directories))
        });
    let Some(existing_alias) = existing_alias else {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };
    let changed = existing_alias.target_vref != target_vref
        || existing_alias.target_dir_id != target_dir_id
        || existing_alias.target_name != target_name;
    if !changed {
        if memory.write_u8(was_changed_ptr, 0).is_none() {
            *last_mem_error = PPC_NO_ERR;
            return PPC_PARAM_ERR;
        }
        *last_mem_error = PPC_NO_ERR;
        return PPC_NO_ERR;
    }

    let handle_size = usize::try_from(handles[handle_index].size).unwrap_or(usize::MAX);
    if alias_data.len() > handle_size {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return PPC_MEM_FULL_ERR;
    }
    let Some(data_ptr) = memory.read_u32_be(alias_handle) else {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    };
    let alias_data_len = u32::try_from(alias_data.len()).unwrap_or(u32::MAX);
    if !ppc_memory_can_write_bytes(memory, data_ptr, alias_data_len) {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    }
    for (offset, byte) in alias_data.iter().copied().enumerate() {
        if memory.write_u8(data_ptr + offset as u32, byte).is_none() {
            *last_mem_error = PPC_NO_ERR;
            return PPC_PARAM_ERR;
        }
    }
    handles[handle_index].size = alias_data_len;
    if let Some(record) = aliases
        .iter_mut()
        .find(|record| record.handle == alias_handle)
    {
        record.target_vref = target_vref;
        record.target_dir_id = target_dir_id;
        record.target_name = target_name;
    } else {
        aliases.push(PpcAliasRecord {
            handle: alias_handle,
            target_vref,
            target_dir_id,
            target_name,
        });
    }
    if memory.write_u8(was_changed_ptr, 1).is_none() {
        *last_mem_error = PPC_NO_ERR;
        return PPC_PARAM_ERR;
    }
    *last_mem_error = PPC_NO_ERR;
    PPC_NO_ERR
}

pub(super) fn ppc_resolve_alias_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_resources: &[PpcVfsResourceRecord],
) -> i16 {
    let spec_ptr = cpu.gpr[3];
    let _resolve_alias_chains = cpu.gpr[4] != 0;
    let target_is_folder_ptr = cpu.gpr[5];
    let was_aliased_ptr = cpu.gpr[6];
    if target_is_folder_ptr == 0 || was_aliased_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let path = match ppc_path_for_fsspec(memory, vfs_directories, spec_ptr) {
        Ok(path) => path,
        Err(err) => return err,
    };
    let target_is_folder = ppc_directory_id_for_path(vfs_directories, &path).is_some();
    let known_file = target_is_folder
        || ppc_vfs_file_index(vfs_files, &path).is_some()
        || ppc_vfs_resource_file_index(vfs_resource_files, &path).is_some();
    if !known_file {
        return PPC_FNF_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, target_is_folder_ptr, 1)
        || !ppc_memory_can_write_bytes(memory, was_aliased_ptr, 1)
    {
        return PPC_PARAM_ERR;
    }
    if let Some(alias) = ppc_alias_record_for_path(vfs_directories, vfs_resources, &path) {
        if !ppc_memory_can_write_bytes(memory, spec_ptr, PPC_FSSPEC_SIZE as u32) {
            return PPC_PARAM_ERR;
        }
        let target_exists = ppc_fsspec_target_exists(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            alias.target_dir_id,
            &alias.target_name,
        );
        let target_is_folder =
            ppc_fsspec_target_is_folder(vfs_directories, alias.target_dir_id, &alias.target_name);
        if !ppc_write_fsspec_parts(
            memory,
            spec_ptr,
            alias.target_vref,
            alias.target_dir_id,
            &alias.target_name,
        ) || memory
            .write_u8(target_is_folder_ptr, u8::from(target_is_folder))
            .is_none()
            || memory.write_u8(was_aliased_ptr, 1).is_none()
        {
            return PPC_PARAM_ERR;
        }
        return if target_exists {
            PPC_NO_ERR
        } else {
            PPC_FNF_ERR
        };
    }
    if memory
        .write_u8(target_is_folder_ptr, u8::from(target_is_folder))
        .is_none()
        || memory.write_u8(was_aliased_ptr, 0).is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(super) fn ppc_alias_record_for_path(
    vfs_directories: &[PpcVfsDirectory],
    vfs_resources: &[PpcVfsResourceRecord],
    path: &str,
) -> Option<PpcAliasRecord> {
    vfs_resources
        .iter()
        .filter(|resource| {
            resource.path.eq_ignore_ascii_case(path) && resource.res_type == PPC_ALIAS_RECORD_MAGIC
        })
        .min_by_key(|resource| (u8::from(resource.res_id != 0), resource.res_id))
        .and_then(|resource| ppc_alias_record_from_bytes(&resource.data, Some(vfs_directories)))
}

pub(super) fn ppc_fsspec_target_is_folder(
    vfs_directories: &[PpcVfsDirectory],
    dir_id: u32,
    name_bytes: &[u8],
) -> bool {
    if name_bytes.is_empty() {
        return ppc_directory_path_for_id(vfs_directories, dir_id).is_some();
    }
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) else {
        return false;
    };
    let normalized_name = ppc_normalize_vfs_path(&decode_mac_roman(name_bytes));
    if normalized_name.is_empty() {
        return false;
    }
    let path = ppc_join_vfs_path(parent_path, &normalized_name);
    ppc_directory_id_for_path(vfs_directories, &path).is_some()
}

pub(super) fn ppc_resolve_directory_id(vref: i16, dir_id: u32, default_dir_id: u32) -> u32 {
    if dir_id <= 1 {
        if dir_id == 0 && vref == 0 {
            return default_dir_id;
        }
        if dir_id == 0 {
            return PPC_ROOT_DIR_ID;
        }
        return dir_id;
    }
    dir_id
}

pub(super) fn ppc_resolve_volume_ref_num(vref: i16) -> i16 {
    if vref == 0 {
        PPC_BOOT_VOLUME_REF_NUM
    } else {
        vref
    }
}

pub(super) fn ppc_normalize_vfs_path(name: &str) -> String {
    name.replace(':', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

pub(super) fn ppc_join_vfs_path(parent_path: &str, child_name: &str) -> String {
    if parent_path.is_empty() {
        child_name.to_string()
    } else {
        format!("{parent_path}/{child_name}")
    }
}

pub(super) fn ppc_directory_path_for_id(vfs_directories: &[PpcVfsDirectory], dir_id: u32) -> Option<&str> {
    vfs_directories
        .iter()
        .find(|directory| directory.dir_id == dir_id)
        .map(|directory| directory.path.as_str())
}

pub(super) fn ppc_directory_id_for_path(vfs_directories: &[PpcVfsDirectory], path: &str) -> Option<u32> {
    vfs_directories
        .iter()
        .find(|directory| directory.path.eq_ignore_ascii_case(path))
        .map(|directory| directory.dir_id)
}

