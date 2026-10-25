//! Carbon FSRef File Manager calls (Files.h, Universal Interfaces 3.4 and
//! CarbonLib), layered over the FSSpec-based File Manager emulation.

use super::*;
use crate::trap::types::decode_mac_roman;

/// FSRef is 80 opaque bytes (Files.h). This runtime stores a magic tag and
/// then the object's FSSpec: vRefNum at +4, parent directory ID at +6, and a
/// Str63 name at +10. Every FSRef therefore names an FSSpec, so the FSSpec
/// routines do the File Manager work.
const PPC_FSREF_SIZE: u32 = 80;
const PPC_FSREF_MAGIC: &[u8; 4] = b"SyFR";
const PPC_FSREF_SPEC_OFFSET: u32 = 4;
/// fsRtParID, the parent of a volume's root directory (Files.h).
const PPC_FS_RT_PAR_ID: u32 = 1;
/// Temporary FSSpecs and counts live this far below the guest stack pointer,
/// memory that no guest frame uses while an HLE import runs.
const PPC_FSREF_SCRATCH_BELOW_SP: u32 = 0x800;
const PPC_FSREF_SCRATCH_SIZE: u32 = 0x200;
/// FSCatalogInfo is 144 bytes (Files.h).
const PPC_FS_CATALOG_INFO_SIZE: u32 = 144;
/// FSGetResourceForkName (Files.h) names the resource fork "RESOURCE_FORK".
const PPC_RESOURCE_FORK_NAME: &str = "RESOURCE_FORK";

// Files.h result codes.
const PPC_NOT_A_FILE_ERR: i16 = -1302;
const PPC_ERR_FS_NOT_A_FOLDER: i16 = -1407;
const PPC_ERR_FS_FORK_NOT_FOUND: i16 = -1409;
const PPC_ERR_FS_REFS_DIFFERENT: i16 = -1420;

// FSCatalogInfoBitmap bits (Files.h).
const PPC_FS_CAT_INFO_TEXT_ENCODING: u32 = 0x0000_0001;
const PPC_FS_CAT_INFO_NODE_FLAGS: u32 = 0x0000_0002;
const PPC_FS_CAT_INFO_VOLUME: u32 = 0x0000_0004;
const PPC_FS_CAT_INFO_PARENT_DIR_ID: u32 = 0x0000_0008;
const PPC_FS_CAT_INFO_NODE_ID: u32 = 0x0000_0010;
const PPC_FS_CAT_INFO_CREATE_DATE: u32 = 0x0000_0020;
const PPC_FS_CAT_INFO_PERMISSIONS: u32 = 0x0000_0400;
const PPC_FS_CAT_INFO_FINDER_INFO: u32 = 0x0000_0800;
const PPC_FS_CAT_INFO_FINDER_X_INFO: u32 = 0x0000_1000;
const PPC_FS_CAT_INFO_VALENCE: u32 = 0x0000_2000;
const PPC_FS_CAT_INFO_DATA_SIZES: u32 = 0x0000_4000;
const PPC_FS_CAT_INFO_RSRC_SIZES: u32 = 0x0000_8000;
const PPC_FS_CAT_INFO_SHARING_FLAGS: u32 = 0x0001_0000;
const PPC_FS_CAT_INFO_USER_PRIVS: u32 = 0x0002_0000;

// FSNodeFlags (Files.h).
const PPC_FS_NODE_RES_OPEN: u16 = 0x0004;
const PPC_FS_NODE_DATA_OPEN: u16 = 0x0008;
const PPC_FS_NODE_IS_DIRECTORY: u16 = 0x0010;
const PPC_FS_NODE_FORK_OPEN: u16 = 0x0080;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcFsRefOperation {
    FspMakeFsRef,
    MakeFsRefUnicode,
    CompareFsRefs,
    GetCatalogInfo,
    SetCatalogInfo,
    OpenFork,
    ReadFork,
    WriteFork,
    CloseFork,
    GetForkPosition,
    SetForkPosition,
    GetForkSize,
    SetForkSize,
    GetDataForkName,
    GetResourceForkName,
    DeleteObject,
    CreateFileUnicode,
    CreateDirectoryUnicode,
    RenameUnicode,
    FindFolder,
    ResolveAliasFile,
}

impl PpcFsRefOperation {
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "FSpMakeFSRef" => Self::FspMakeFsRef,
            "FSMakeFSRefUnicode" => Self::MakeFsRefUnicode,
            "FSCompareFSRefs" => Self::CompareFsRefs,
            "FSGetCatalogInfo" => Self::GetCatalogInfo,
            "FSSetCatalogInfo" => Self::SetCatalogInfo,
            "FSOpenFork" => Self::OpenFork,
            "FSReadFork" => Self::ReadFork,
            "FSWriteFork" => Self::WriteFork,
            "FSCloseFork" => Self::CloseFork,
            "FSGetForkPosition" => Self::GetForkPosition,
            "FSSetForkPosition" => Self::SetForkPosition,
            "FSGetForkSize" => Self::GetForkSize,
            "FSSetForkSize" => Self::SetForkSize,
            "FSGetDataForkName" => Self::GetDataForkName,
            "FSGetResourceForkName" => Self::GetResourceForkName,
            "FSDeleteObject" => Self::DeleteObject,
            "FSCreateFileUnicode" => Self::CreateFileUnicode,
            "FSCreateDirectoryUnicode" => Self::CreateDirectoryUnicode,
            "FSRenameUnicode" => Self::RenameUnicode,
            "FSFindFolder" => Self::FindFolder,
            "FSResolveAliasFile" | "FSResolveAliasFileWithMountFlags" => Self::ResolveAliasFile,
            _ => return None,
        })
    }
}

pub(super) struct PpcFsRefContext<'a> {
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
    pub(super) default_dir_id: u32,
    pub(super) vfs_volumes: &'a [PpcVfsVolumeRecord],
}

/// A resolved FSRef: its canonical FSSpec parts plus the VFS path.
#[derive(Debug, Clone)]
struct PpcFsRefObject {
    vref: i16,
    parent_dir_id: u32,
    name: Vec<u8>,
    path: String,
    dir_id: Option<u32>,
}

pub(super) fn ppc_dispatch_fsref(operation: PpcFsRefOperation, ctx: &mut PpcFsRefContext) -> i16 {
    let result = match operation {
        PpcFsRefOperation::FspMakeFsRef => ppc_fsp_make_fsref(ctx),
        PpcFsRefOperation::MakeFsRefUnicode => ppc_fs_make_fsref_unicode(ctx),
        PpcFsRefOperation::CompareFsRefs => ppc_fs_compare_fsrefs(ctx),
        PpcFsRefOperation::GetCatalogInfo => ppc_fs_get_catalog_info(ctx),
        PpcFsRefOperation::SetCatalogInfo => ppc_fs_set_catalog_info(ctx),
        PpcFsRefOperation::OpenFork => ppc_fs_open_fork(ctx),
        PpcFsRefOperation::ReadFork => ppc_fs_transfer_fork(ctx, false),
        PpcFsRefOperation::WriteFork => ppc_fs_transfer_fork(ctx, true),
        PpcFsRefOperation::CloseFork => ppc_fs_close_fork(ctx),
        PpcFsRefOperation::GetForkPosition => ppc_fs_get_fork_position(ctx),
        PpcFsRefOperation::SetForkPosition => ppc_fs_set_fork_position(ctx),
        PpcFsRefOperation::GetForkSize => ppc_fs_get_fork_size(ctx),
        PpcFsRefOperation::SetForkSize => ppc_fs_set_fork_size(ctx),
        PpcFsRefOperation::GetDataForkName => ppc_fs_get_fork_name(ctx, ""),
        PpcFsRefOperation::GetResourceForkName => ppc_fs_get_fork_name(ctx, PPC_RESOURCE_FORK_NAME),
        PpcFsRefOperation::DeleteObject => ppc_fs_delete_object(ctx),
        PpcFsRefOperation::CreateFileUnicode => ppc_fs_create_file_unicode(ctx),
        PpcFsRefOperation::CreateDirectoryUnicode => ppc_fs_create_directory_unicode(ctx),
        PpcFsRefOperation::RenameUnicode => ppc_fs_rename_unicode(ctx),
        PpcFsRefOperation::FindFolder => ppc_fs_find_folder(ctx),
        PpcFsRefOperation::ResolveAliasFile => ppc_fs_resolve_alias_file(ctx),
    };
    let result = result.unwrap_or_else(|error| error);
    if ppc_hle_trace_enabled() {
        eprintln!("[PPC-TRACE] FSRef {operation:?} -> {result}");
    }
    result
}

/// Combines a 64-bit CFM argument passed high word first in two GPRs.
fn ppc_fsref_i64_arg(cpu: &PpcCpu, high_register: usize) -> i64 {
    ((u64::from(cpu.gpr[high_register]) << 32) | u64::from(cpu.gpr[high_register + 1])) as i64
}

fn ppc_fsref_scratch(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> Result<u32, i16> {
    let scratch = cpu.gpr[1].wrapping_sub(PPC_FSREF_SCRATCH_BELOW_SP);
    if ppc_memory_can_write_bytes(memory, scratch, PPC_FSREF_SCRATCH_SIZE) {
        Ok(scratch)
    } else {
        Err(PPC_PARAM_ERR)
    }
}

fn ppc_read_fsref(memory: &mut PpcSectionMem, ref_ptr: u32) -> Result<(i16, u32, Vec<u8>), i16> {
    if ref_ptr == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let magic = ppc_memory_read_bytes(memory, ref_ptr, 4).ok_or(PPC_PARAM_ERR)?;
    if magic.as_slice() != PPC_FSREF_MAGIC {
        return Err(PPC_PARAM_ERR);
    }
    ppc_read_fsspec_parts(memory, ref_ptr + PPC_FSREF_SPEC_OFFSET).ok_or(PPC_PARAM_ERR)
}

fn ppc_write_fsref_parts(
    memory: &mut PpcSectionMem,
    ref_ptr: u32,
    vref: i16,
    parent_dir_id: u32,
    name: &[u8],
) -> bool {
    ref_ptr != 0
        && ppc_memory_can_write_bytes(memory, ref_ptr, PPC_FSREF_SIZE)
        && ppc_zero_guest_bytes(memory, ref_ptr, PPC_FSREF_SIZE)
        && memory.write_bytes(ref_ptr, PPC_FSREF_MAGIC).is_some()
        && ppc_write_fsspec_parts(
            memory,
            ref_ptr + PPC_FSREF_SPEC_OFFSET,
            vref,
            parent_dir_id,
            name,
        )
}

fn ppc_write_fsref(memory: &mut PpcSectionMem, ref_ptr: u32, object: &PpcFsRefObject) -> bool {
    ppc_write_fsref_parts(
        memory,
        ref_ptr,
        object.vref,
        object.parent_dir_id,
        &object.name,
    )
}

fn ppc_write_fsref_spec(
    memory: &mut PpcSectionMem,
    spec_ptr: u32,
    object: &PpcFsRefObject,
) -> bool {
    ppc_memory_can_write_bytes(memory, spec_ptr, PPC_FSSPEC_SIZE as u32)
        && ppc_zero_guest_bytes(memory, spec_ptr, PPC_FSSPEC_SIZE as u32)
        && ppc_write_fsspec_parts(
            memory,
            spec_ptr,
            object.vref,
            object.parent_dir_id,
            &object.name,
        )
}

/// Reads a UniChar array with an explicit length as Mac Roman bytes.
fn ppc_read_unicode_name(
    memory: &mut PpcSectionMem,
    name_ptr: u32,
    name_len: u32,
) -> Result<Vec<u8>, i16> {
    if name_len > 255 || (name_len > 0 && name_ptr == 0) {
        return Err(PPC_PARAM_ERR);
    }
    let units = (0..name_len)
        .map(|index| memory.read_u16_be(name_ptr + 2 * index))
        .collect::<Option<Vec<_>>>()
        .ok_or(PPC_PARAM_ERR)?;
    Ok(encode_mac_roman_lossy(&String::from_utf16_lossy(&units)))
}

/// Reads a new catalog name; an HFS name has 1..=63 bytes without colons.
fn ppc_read_new_unicode_name(
    memory: &mut PpcSectionMem,
    name_ptr: u32,
    name_len: u32,
) -> Result<Vec<u8>, i16> {
    let name = ppc_read_unicode_name(memory, name_ptr, name_len)?;
    if name.is_empty() || name.len() > PPC_FSSPEC_MAX_NAME_LEN || name.contains(&b':') {
        return Err(PPC_BD_NAM_ERR);
    }
    Ok(name)
}

/// HFSUniStr255: a UInt16 length followed by up to 255 UniChars.
fn ppc_write_hfs_uni_str255(memory: &mut PpcSectionMem, ptr: u32, name: &str) -> bool {
    let units: Vec<u16> = name.encode_utf16().take(255).collect();
    if !ppc_memory_can_write_bytes(memory, ptr, 2 + 2 * units.len() as u32) {
        return false;
    }
    let _ = memory.write_u16_be(ptr, units.len() as u16);
    for (index, unit) in units.iter().enumerate() {
        let _ = memory.write_u16_be(ptr + 2 + 2 * index as u32, *unit);
    }
    true
}

fn ppc_fsref_volume_root(vfs_volumes: &[PpcVfsVolumeRecord], vref: i16) -> Option<(u32, String)> {
    if matches!(vref, 0 | PPC_BOOT_VOLUME_REF_NUM) {
        return Some((
            PPC_ROOT_DIR_ID,
            crate::trap::TrapDispatcher::boot_volume_name().to_string(),
        ));
    }
    vfs_volumes
        .iter()
        .find(|volume| volume.ref_num == vref)
        .map(|volume| (volume.root_dir_id, volume.name.clone()))
}

/// The canonical FSRef for an existing VFS path. A volume's root directory
/// is named by the volume name inside fsRtParID.
fn ppc_fsref_object_for_path(ctx: &PpcFsRefContext, vref: i16, path: &str) -> PpcFsRefObject {
    let dir_id = ppc_directory_id_for_path(ctx.vfs_directories, path);
    let root = ppc_fsref_volume_root(ctx.vfs_volumes, vref);
    let (parent_dir_id, mut name) = match (dir_id, root) {
        (Some(id), Some((root_dir_id, volume_name))) if id == root_dir_id => {
            (PPC_FS_RT_PAR_ID, encode_mac_roman_lossy(&volume_name))
        }
        _ => (
            ppc_parent_dir_id_for_path(ctx.vfs_directories, path),
            ppc_vfs_basename_bytes(path),
        ),
    };
    name.truncate(PPC_FSSPEC_MAX_NAME_LEN);
    PpcFsRefObject {
        vref,
        parent_dir_id,
        name,
        path: path.to_string(),
        dir_id,
    }
}

/// Resolves FSSpec parts to the existing object they name.
fn ppc_fsref_resolve(
    ctx: &PpcFsRefContext,
    (vref, parent_dir_id, name): (i16, u32, Vec<u8>),
) -> Result<PpcFsRefObject, i16> {
    let parent_dir_id = ppc_resolve_directory_id(vref, parent_dir_id, ctx.default_dir_id);
    let vref = ppc_resolve_volume_ref_num(vref);
    if parent_dir_id == PPC_FS_RT_PAR_ID {
        let (root_dir_id, _) = ppc_fsref_volume_root(ctx.vfs_volumes, vref).ok_or(PPC_NSV_ERR)?;
        let path = ppc_directory_path_for_id(ctx.vfs_directories, root_dir_id)
            .ok_or(PPC_DIR_NF_ERR)?
            .to_string();
        return Ok(ppc_fsref_object_for_path(ctx, vref, &path));
    }
    if ppc_directory_path_for_id(ctx.vfs_directories, parent_dir_id).is_none() {
        return Err(PPC_DIR_NF_ERR);
    }
    let path = ppc_resolved_fsspec_target_path(
        ctx.vfs_directories,
        ctx.vfs_files,
        ctx.vfs_resource_files,
        parent_dir_id,
        &name,
    )
    .ok_or(PPC_FNF_ERR)?;
    Ok(ppc_fsref_object_for_path(ctx, vref, &path))
}

fn ppc_fsref_resolve_ptr(ctx: &mut PpcFsRefContext, ref_ptr: u32) -> Result<PpcFsRefObject, i16> {
    let parts = ppc_read_fsref(ctx.memory, ref_ptr)?;
    ppc_fsref_resolve(ctx, parts)
}

fn ppc_fsref_resolve_directory_ptr(
    ctx: &mut PpcFsRefContext,
    ref_ptr: u32,
) -> Result<(PpcFsRefObject, u32), i16> {
    let object = ppc_fsref_resolve_ptr(ctx, ref_ptr)?;
    let dir_id = object.dir_id.ok_or(PPC_ERR_FS_NOT_A_FOLDER)?;
    Ok((object, dir_id))
}

fn ppc_fsref_write_output(
    ctx: &mut PpcFsRefContext,
    ref_ptr: u32,
    object: &PpcFsRefObject,
) -> Result<(), i16> {
    if ref_ptr != 0 && !ppc_write_fsref(ctx.memory, ref_ptr, object) {
        return Err(PPC_PARAM_ERR);
    }
    Ok(())
}

/// FSpMakeFSRef(const FSSpec *source, FSRef *newRef).
fn ppc_fsp_make_fsref(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let (spec_ptr, ref_ptr) = (ctx.cpu.gpr[3], ctx.cpu.gpr[4]);
    if ref_ptr == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let parts = ppc_read_fsspec_parts(ctx.memory, spec_ptr).ok_or(PPC_PARAM_ERR)?;
    let object = ppc_fsref_resolve(ctx, parts)?;
    if !ppc_write_fsref(ctx.memory, ref_ptr, &object) {
        return Err(PPC_PARAM_ERR);
    }
    Ok(PPC_NO_ERR)
}

/// FSMakeFSRefUnicode(const FSRef *parentRef, UniCharCount nameLength,
/// const UniChar *name, TextEncoding textEncodingHint, FSRef *newRef).
fn ppc_fs_make_fsref_unicode(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let (parent_ptr, name_len, name_ptr, out) = (
        ctx.cpu.gpr[3],
        ctx.cpu.gpr[4],
        ctx.cpu.gpr[5],
        ctx.cpu.gpr[7],
    );
    if out == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let (parent, dir_id) = ppc_fsref_resolve_directory_ptr(ctx, parent_ptr)?;
    let name = ppc_read_new_unicode_name(ctx.memory, name_ptr, name_len)?;
    let object = ppc_fsref_resolve(ctx, (parent.vref, dir_id, name))?;
    ppc_fsref_write_output(ctx, out, &object)?;
    Ok(PPC_NO_ERR)
}

/// FSCompareFSRefs(const FSRef *ref1, const FSRef *ref2).
fn ppc_fs_compare_fsrefs(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let (first, second) = (ctx.cpu.gpr[3], ctx.cpu.gpr[4]);
    let (first_vref, first_parent, first_name) = ppc_read_fsref(ctx.memory, first)?;
    let (second_vref, second_parent, second_name) = ppc_read_fsref(ctx.memory, second)?;
    let same = ppc_resolve_volume_ref_num(first_vref) == ppc_resolve_volume_ref_num(second_vref)
        && first_parent == second_parent
        && decode_mac_roman(&first_name).to_lowercase()
            == decode_mac_roman(&second_name).to_lowercase();
    Ok(if same {
        PPC_NO_ERR
    } else {
        PPC_ERR_FS_REFS_DIFFERENT
    })
}

fn ppc_fsref_data_len(ctx: &PpcFsRefContext, path: &str) -> u64 {
    ctx.vfs_files
        .iter()
        .find(|file| file.path.eq_ignore_ascii_case(path))
        .map_or(0, |file| file.data.len() as u64)
}

fn ppc_fsref_node_flags(ctx: &PpcFsRefContext, object: &PpcFsRefObject) -> u16 {
    if object.dir_id.is_some() {
        return PPC_FS_NODE_IS_DIRECTORY;
    }
    let resource_path = format!("{PPC_OPEN_RESOURCE_FORK_PREFIX}{}", object.path);
    let data_open = ctx
        .files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case(&object.path));
    let resource_open = ctx
        .files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case(&resource_path))
        || ctx
            .resource_files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case(&object.path));
    let mut flags = 0;
    if data_open {
        flags |= PPC_FS_NODE_DATA_OPEN | PPC_FS_NODE_FORK_OPEN;
    }
    if resource_open {
        flags |= PPC_FS_NODE_RES_OPEN | PPC_FS_NODE_FORK_OPEN;
    }
    flags
}

/// Writes the fork sizes as UInt64 logical and physical lengths; physical
/// lengths round up to 512-byte blocks.
fn ppc_write_fork_sizes(memory: &mut PpcSectionMem, ptr: u32, logical: u64) -> Option<()> {
    memory.write_u64_be(ptr, logical)?;
    memory.write_u64_be(ptr + 8, logical.div_ceil(512) * 512)
}

/// Fills the FSCatalogInfo fields selected by `which` (Files.h): nodeFlags
/// +0, volume +2, parentDirID +4, nodeID +8, sharingFlags +12,
/// userPrivileges +13, five UTCDateTimes from +16, permissions +56,
/// finderInfo +72, extFinderInfo +88, data sizes +104, resource sizes +120,
/// valence +136, and textEncodingHint +140.
fn ppc_fill_fs_catalog_info(
    ctx: &mut PpcFsRefContext,
    info: u32,
    which: u32,
    object: &PpcFsRefObject,
) -> Option<()> {
    let node_flags = ppc_fsref_node_flags(ctx, object);
    let directory = object.dir_id.and_then(|dir_id| {
        ctx.vfs_directories
            .iter()
            .find(|directory| directory.dir_id == dir_id)
            .cloned()
    });
    let (file_type, creator, finder_flags) = match &directory {
        Some(directory) => (
            directory.file_type,
            directory.creator,
            directory.finder_flags,
        ),
        None => ppc_finder_info_for_path(ctx.vfs_files, ctx.vfs_resource_files, &object.path),
    };
    let node_id = object
        .dir_id
        .unwrap_or_else(|| ppc_synthetic_file_id(&object.path));
    let valence = object.dir_id.map_or(0, |dir_id| {
        ppc_vfs_directory_valence(
            ctx.vfs_directories,
            ctx.vfs_files,
            ctx.vfs_resource_files,
            dir_id,
        )
        .map_or(0, u32::from)
    });
    let (data_len, resource_len) = if directory.is_some() {
        (0, 0)
    } else {
        (
            ppc_fsref_data_len(ctx, &object.path),
            u64::from(ppc_resource_fork_len_for_path(
                ctx.vfs_resource_files,
                ctx.vfs_resources,
                &object.path,
            )),
        )
    };
    let memory = &mut *ctx.memory;
    if which & PPC_FS_CAT_INFO_NODE_FLAGS != 0 {
        memory.write_u16_be(info, node_flags)?;
    }
    if which & PPC_FS_CAT_INFO_VOLUME != 0 {
        memory.write_u16_be(info + 2, object.vref as u16)?;
    }
    if which & PPC_FS_CAT_INFO_PARENT_DIR_ID != 0 {
        memory.write_u32_be(info + 4, object.parent_dir_id)?;
    }
    if which & PPC_FS_CAT_INFO_NODE_ID != 0 {
        memory.write_u32_be(info + 8, node_id)?;
    }
    if which & PPC_FS_CAT_INFO_SHARING_FLAGS != 0 {
        memory.write_u8(info + 12, 0)?;
    }
    if which & PPC_FS_CAT_INFO_USER_PRIVS != 0 {
        memory.write_u8(info + 13, 0)?;
    }
    // createDate, contentModDate, attributeModDate, accessDate and
    // backupDate use consecutive bits from kFSCatInfoCreateDate; the VFS
    // keeps no catalog dates.
    for (index, offset) in (16..56).step_by(8).enumerate() {
        if which & (PPC_FS_CAT_INFO_CREATE_DATE << index) != 0 {
            memory.write_bytes(info + offset, &[0; 8])?;
        }
    }
    if which & PPC_FS_CAT_INFO_PERMISSIONS != 0 {
        memory.write_bytes(info + 56, &[0; 16])?;
    }
    if which & PPC_FS_CAT_INFO_FINDER_INFO != 0 {
        if directory.is_some() {
            // Files.h FolderInfo: windowBounds, then finderFlags at +8 as in
            // FileInfo, then location and a reserved word.
            memory.write_bytes(info + 72, &[0; 16])?;
            memory.write_u16_be(info + 80, finder_flags)?;
        } else {
            ppc_write_finfo(memory, info + 72, file_type, creator, finder_flags)?;
        }
    }
    if which & PPC_FS_CAT_INFO_FINDER_X_INFO != 0 {
        memory.write_bytes(info + 88, &[0; 16])?;
    }
    if which & PPC_FS_CAT_INFO_DATA_SIZES != 0 {
        ppc_write_fork_sizes(memory, info + 104, data_len)?;
    }
    if which & PPC_FS_CAT_INFO_RSRC_SIZES != 0 {
        ppc_write_fork_sizes(memory, info + 120, resource_len)?;
    }
    if which & PPC_FS_CAT_INFO_VALENCE != 0 {
        memory.write_u32_be(info + 136, valence)?;
    }
    if which & PPC_FS_CAT_INFO_TEXT_ENCODING != 0 {
        // kTextEncodingMacRoman.
        memory.write_u32_be(info + 140, 0)?;
    }
    Some(())
}

/// FSGetCatalogInfo(const FSRef *ref, FSCatalogInfoBitmap whichInfo,
/// FSCatalogInfo *catalogInfo, HFSUniStr255 *outName, FSSpec *fsSpec,
/// FSRef *parentRef); every output may be nil.
fn ppc_fs_get_catalog_info(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let gpr = ctx.cpu.gpr;
    let (ref_ptr, which, info, out_name, spec_out, parent_out) =
        (gpr[3], gpr[4], gpr[5], gpr[6], gpr[7], gpr[8]);
    let object = ppc_fsref_resolve_ptr(ctx, ref_ptr)?;
    if info != 0 {
        if !ppc_memory_can_write_bytes(ctx.memory, info, PPC_FS_CATALOG_INFO_SIZE) {
            return Err(PPC_PARAM_ERR);
        }
        ppc_fill_fs_catalog_info(ctx, info, which, &object).ok_or(PPC_PARAM_ERR)?;
    }
    if out_name != 0
        && !ppc_write_hfs_uni_str255(ctx.memory, out_name, &decode_mac_roman(&object.name))
    {
        return Err(PPC_PARAM_ERR);
    }
    if spec_out != 0 && !ppc_write_fsref_spec(ctx.memory, spec_out, &object) {
        return Err(PPC_PARAM_ERR);
    }
    if parent_out != 0 {
        let parent_path = (object.parent_dir_id != PPC_FS_RT_PAR_ID)
            .then(|| ppc_directory_path_for_id(ctx.vfs_directories, object.parent_dir_id))
            .flatten()
            .map(str::to_string);
        match parent_path {
            Some(path) => {
                let parent = ppc_fsref_object_for_path(ctx, object.vref, &path);
                ppc_fsref_write_output(ctx, parent_out, &parent)?;
            }
            // A volume's root directory has no parent object.
            None => {
                if !ppc_memory_can_write_bytes(ctx.memory, parent_out, PPC_FSREF_SIZE)
                    || !ppc_zero_guest_bytes(ctx.memory, parent_out, PPC_FSREF_SIZE)
                {
                    return Err(PPC_PARAM_ERR);
                }
            }
        }
    }
    Ok(PPC_NO_ERR)
}

/// Applies FSCatalogInfo finderInfo (+72) through the FInfo set path.
fn ppc_fsref_apply_finder_info(
    ctx: &mut PpcFsRefContext,
    object: &PpcFsRefObject,
    info: u32,
) -> Result<(), i16> {
    if info == 0 {
        return Err(PPC_PARAM_ERR);
    }
    if let Some(dir_id) = object.dir_id {
        // A folder's FolderInfo carries only finderFlags, at +8.
        let finder_flags = ctx.memory.read_u16_be(info + 80).ok_or(PPC_PARAM_ERR)?;
        let directory = ctx
            .vfs_directories
            .iter_mut()
            .find(|directory| directory.dir_id == dir_id)
            .ok_or(PPC_DIR_NF_ERR)?;
        directory.finder_flags = finder_flags;
        directory.dirty = true;
        return Ok(());
    }
    let (file_type, creator, finder_flags) =
        ppc_read_finfo(ctx.memory, info + 72).ok_or(PPC_PARAM_ERR)?;
    if ppc_set_finfo_for_path(
        ctx.vfs_files,
        ctx.vfs_resource_files,
        &object.path,
        file_type,
        creator,
        finder_flags,
    ) {
        Ok(())
    } else {
        Err(PPC_FNF_ERR)
    }
}

/// FSSetCatalogInfo(const FSRef *ref, FSCatalogInfoBitmap whichInfo,
/// const FSCatalogInfo *catalogInfo). Only Finder information is stored;
/// the VFS keeps no dates, permissions, or extended Finder information.
fn ppc_fs_set_catalog_info(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let (ref_ptr, which, info) = (ctx.cpu.gpr[3], ctx.cpu.gpr[4], ctx.cpu.gpr[5]);
    let object = ppc_fsref_resolve_ptr(ctx, ref_ptr)?;
    if which & PPC_FS_CAT_INFO_FINDER_INFO != 0 {
        ppc_fsref_apply_finder_info(ctx, &object, info)?;
    }
    Ok(PPC_NO_ERR)
}

/// FSOpenFork(const FSRef *ref, UniCharCount forkNameLength,
/// const UniChar *forkName, SInt8 permissions, SInt16 *forkRefNum). The
/// empty name is the data fork and "RESOURCE_FORK" the resource fork.
fn ppc_fs_open_fork(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let gpr = ctx.cpu.gpr;
    let (ref_ptr, name_len, name_ptr, permission, out) = (gpr[3], gpr[4], gpr[5], gpr[6], gpr[7]);
    let object = ppc_fsref_resolve_ptr(ctx, ref_ptr)?;
    if object.dir_id.is_some() {
        return Err(PPC_NOT_A_FILE_ERR);
    }
    let fork_name = ppc_read_unicode_name(ctx.memory, name_ptr, name_len)?;
    let resource_fork = if fork_name.is_empty() {
        false
    } else if fork_name == PPC_RESOURCE_FORK_NAME.as_bytes() {
        true
    } else {
        return Err(PPC_ERR_FS_FORK_NOT_FOUND);
    };
    let spec = ppc_fsref_scratch(ctx.cpu, ctx.memory)?;
    if !ppc_write_fsref_spec(ctx.memory, spec, &object) {
        return Err(PPC_PARAM_ERR);
    }
    let mut call = ctx.cpu.clone();
    call.gpr[3] = spec;
    call.gpr[4] = permission as u8 as u32;
    call.gpr[5] = out;
    Ok(if resource_fork {
        ppc_fsp_open_rf(
            &mut call,
            ctx.memory,
            ctx.vfs_directories,
            ctx.vfs_files,
            ctx.vfs_resource_files,
            ctx.vfs_resources,
            ctx.files,
            ctx.writable_refnums,
            ctx.next_file_ref_num,
        )
    } else {
        ppc_fsp_open_df(
            &mut call,
            ctx.memory,
            ctx.vfs_directories,
            ctx.vfs_files,
            ctx.files,
            ctx.writable_refnums,
            ctx.next_file_ref_num,
        )
    })
}

/// Resolves an FSReadFork-style positionMode and SInt64 offset to an
/// absolute fork offset. The low two bits are fsAtMark, fsFromStart,
/// fsFromLEOF, and fsFromMark, as for PBRead's ioPosMode (Files.h).
fn ppc_fork_target_offset(
    ctx: &PpcFsRefContext,
    ref_num: i16,
    mode: u16,
    offset: i64,
) -> Result<(i64, i64), i16> {
    let file = ctx
        .files
        .iter()
        .find(|file| file.ref_num == ref_num)
        .ok_or(PPC_RF_NUM_ERR)?;
    let eof = ppc_fsref_data_len(ctx, &file.path) as i64;
    let mark = i64::from(file.position);
    let target = match mode & 0x0003 {
        0 => Some(mark),
        1 => Some(offset),
        2 => eof.checked_add(offset),
        _ => mark.checked_add(offset),
    }
    .ok_or(PPC_POS_ERR)?;
    if target < 0 {
        return Err(PPC_POS_ERR);
    }
    Ok((target, eof))
}

fn ppc_fork_set_mark(ctx: &mut PpcFsRefContext, ref_num: i16, target: i64) -> Result<(), i16> {
    let position = u32::try_from(target).map_err(|_| PPC_POS_ERR)?;
    let file = ctx
        .files
        .iter_mut()
        .find(|file| file.ref_num == ref_num)
        .ok_or(PPC_RF_NUM_ERR)?;
    file.position = position;
    Ok(())
}

/// FSReadFork and FSWriteFork(SInt16 forkRefNum, UInt16 positionMode,
/// SInt64 positionOffset, ByteCount requestCount, void *buffer,
/// ByteCount *actualCount): position the mark, then transfer through
/// FSRead or FSWrite.
fn ppc_fs_transfer_fork(ctx: &mut PpcFsRefContext, write: bool) -> Result<i16, i16> {
    let ref_num = ppc_ref_num_from_gpr(ctx.cpu.gpr[3]);
    let mode = ctx.cpu.gpr[4] as u16;
    let offset = ppc_fsref_i64_arg(ctx.cpu, 5);
    let (request, buffer, actual_ptr) = (ctx.cpu.gpr[7], ctx.cpu.gpr[8], ctx.cpu.gpr[9]);
    if actual_ptr != 0 && !ppc_memory_can_write_bytes(ctx.memory, actual_ptr, 4) {
        return Err(PPC_PARAM_ERR);
    }
    let (target, _) = ppc_fork_target_offset(ctx, ref_num, mode, offset)?;
    ppc_fork_set_mark(ctx, ref_num, target)?;
    let count = ppc_fsref_scratch(ctx.cpu, ctx.memory)?;
    let _ = ctx.memory.write_u32_be(count, request);
    let mut call = ctx.cpu.clone();
    call.gpr[3] = ref_num as u16 as u32;
    call.gpr[4] = count;
    call.gpr[5] = buffer;
    let result = if write {
        ppc_fs_write(
            &mut call,
            ctx.memory,
            ctx.files,
            ctx.writable_refnums,
            ctx.vfs_files,
        )
    } else {
        ppc_fs_read(&mut call, ctx.memory, ctx.files, ctx.vfs_files)
    };
    let actual = if matches!(result, PPC_NO_ERR | PPC_EOF_ERR) {
        ctx.memory.read_u32_be(count).unwrap_or(0)
    } else {
        0
    };
    if actual_ptr != 0 {
        let _ = ctx.memory.write_u32_be(actual_ptr, actual);
    }
    Ok(result)
}

/// FSCloseFork(SInt16 forkRefNum).
fn ppc_fs_close_fork(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let ref_num = ppc_ref_num_from_gpr(ctx.cpu.gpr[3]);
    if !ctx.files.iter().any(|file| file.ref_num == ref_num) {
        return Err(PPC_RF_NUM_ERR);
    }
    Ok(ppc_fs_close(
        ctx.cpu,
        ctx.files,
        ctx.writable_refnums,
        ctx.vfs_files,
        ctx.vfs_resource_files,
        ctx.vfs_resources,
    ))
}

/// FSGetForkPosition(SInt16 forkRefNum, SInt64 *position).
fn ppc_fs_get_fork_position(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let ref_num = ppc_ref_num_from_gpr(ctx.cpu.gpr[3]);
    let out = ctx.cpu.gpr[4];
    let (mark, _) = ppc_fork_target_offset(ctx, ref_num, 0, 0)?;
    if out == 0 || ctx.memory.write_u64_be(out, mark as u64).is_none() {
        return Err(PPC_PARAM_ERR);
    }
    Ok(PPC_NO_ERR)
}

/// FSSetForkPosition(SInt16 forkRefNum, UInt16 positionMode,
/// SInt64 positionOffset): posErr before the start, eofErr past the
/// logical end of file.
fn ppc_fs_set_fork_position(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let ref_num = ppc_ref_num_from_gpr(ctx.cpu.gpr[3]);
    let mode = ctx.cpu.gpr[4] as u16;
    let offset = ppc_fsref_i64_arg(ctx.cpu, 5);
    let (target, eof) = ppc_fork_target_offset(ctx, ref_num, mode, offset)?;
    if target > eof {
        return Err(PPC_EOF_ERR);
    }
    ppc_fork_set_mark(ctx, ref_num, target)?;
    Ok(PPC_NO_ERR)
}

/// FSGetForkSize(SInt16 forkRefNum, SInt64 *forkSize).
fn ppc_fs_get_fork_size(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let ref_num = ppc_ref_num_from_gpr(ctx.cpu.gpr[3]);
    let out = ctx.cpu.gpr[4];
    let (_, eof) = ppc_fork_target_offset(ctx, ref_num, 0, 0)?;
    if out == 0 || ctx.memory.write_u64_be(out, eof as u64).is_none() {
        return Err(PPC_PARAM_ERR);
    }
    Ok(PPC_NO_ERR)
}

/// FSSetForkSize(SInt16 forkRefNum, UInt16 positionMode,
/// SInt64 positionOffset) sets the logical EOF through SetEOF.
fn ppc_fs_set_fork_size(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let ref_num = ppc_ref_num_from_gpr(ctx.cpu.gpr[3]);
    let mode = ctx.cpu.gpr[4] as u16;
    let offset = ppc_fsref_i64_arg(ctx.cpu, 5);
    let (target, _) = ppc_fork_target_offset(ctx, ref_num, mode, offset)?;
    let size = u32::try_from(target).map_err(|_| PPC_POS_ERR)?;
    let mut call = ctx.cpu.clone();
    call.gpr[3] = ref_num as u16 as u32;
    call.gpr[4] = size;
    Ok(ppc_set_eof(
        &mut call,
        ctx.files,
        ctx.writable_refnums,
        ctx.vfs_files,
    ))
}

/// FSGetDataForkName and FSGetResourceForkName(HFSUniStr255 *forkName).
fn ppc_fs_get_fork_name(ctx: &mut PpcFsRefContext, name: &str) -> Result<i16, i16> {
    let out = ctx.cpu.gpr[3];
    if out == 0 || !ppc_write_hfs_uni_str255(ctx.memory, out, name) {
        return Err(PPC_PARAM_ERR);
    }
    Ok(PPC_NO_ERR)
}

/// FSDeleteObject(const FSRef *ref): files delete through FSpDelete; a
/// directory must be empty (fBsyErr otherwise).
fn ppc_fs_delete_object(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let ref_ptr = ctx.cpu.gpr[3];
    let object = ppc_fsref_resolve_ptr(ctx, ref_ptr)?;
    if let Some(dir_id) = object.dir_id {
        let valence = ppc_vfs_directory_valence(
            ctx.vfs_directories,
            ctx.vfs_files,
            ctx.vfs_resource_files,
            dir_id,
        );
        if object.parent_dir_id == PPC_FS_RT_PAR_ID || valence != Some(0) {
            return Err(PPC_F_BSY_ERR);
        }
        ctx.vfs_directories
            .retain(|directory| directory.dir_id != dir_id);
        return Ok(PPC_NO_ERR);
    }
    let spec = ppc_fsref_scratch(ctx.cpu, ctx.memory)?;
    if !ppc_write_fsref_spec(ctx.memory, spec, &object) {
        return Err(PPC_PARAM_ERR);
    }
    let mut call = ctx.cpu.clone();
    call.gpr[3] = spec;
    Ok(ppc_fsp_delete(
        &mut call,
        ctx.memory,
        ctx.vfs_directories,
        ctx.vfs_files,
        ctx.deleted_vfs_file_paths,
        ctx.files,
        ctx.vfs_resource_files,
        ctx.resource_files,
        ctx.vfs_resources,
    ))
}

/// Writes the FSSpec for a new child of `parent_dir_id` into scratch memory.
fn ppc_fsref_child_spec(
    ctx: &mut PpcFsRefContext,
    vref: i16,
    parent_dir_id: u32,
    name: &[u8],
) -> Result<u32, i16> {
    let spec = ppc_fsref_scratch(ctx.cpu, ctx.memory)?;
    if !ppc_zero_guest_bytes(ctx.memory, spec, PPC_FSSPEC_SIZE as u32)
        || !ppc_write_fsspec_parts(ctx.memory, spec, vref, parent_dir_id, name)
    {
        return Err(PPC_PARAM_ERR);
    }
    Ok(spec)
}

/// Writes the optional newRef and newSpec outputs for a created object.
fn ppc_fsref_write_created(
    ctx: &mut PpcFsRefContext,
    vref: i16,
    parent_dir_id: u32,
    name: Vec<u8>,
    ref_out: u32,
    spec_out: u32,
) -> Result<PpcFsRefObject, i16> {
    let object = ppc_fsref_resolve(ctx, (vref, parent_dir_id, name))?;
    ppc_fsref_write_output(ctx, ref_out, &object)?;
    if spec_out != 0 && !ppc_write_fsref_spec(ctx.memory, spec_out, &object) {
        return Err(PPC_PARAM_ERR);
    }
    Ok(object)
}

/// FSCreateFileUnicode(const FSRef *parentRef, UniCharCount nameLength,
/// const UniChar *name, FSCatalogInfoBitmap whichInfo,
/// const FSCatalogInfo *catalogInfo, FSRef *newRef, FSSpec *newSpec).
fn ppc_fs_create_file_unicode(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let gpr = ctx.cpu.gpr;
    let (parent_ptr, name_len, name_ptr, which, info) = (gpr[3], gpr[4], gpr[5], gpr[6], gpr[7]);
    let (ref_out, spec_out) = (gpr[8], gpr[9]);
    let (parent, dir_id) = ppc_fsref_resolve_directory_ptr(ctx, parent_ptr)?;
    let name = ppc_read_new_unicode_name(ctx.memory, name_ptr, name_len)?;
    let finder_info = which & PPC_FS_CAT_INFO_FINDER_INFO != 0;
    let unknown = u32::from_be_bytes(*b"????");
    if finder_info && info == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let (file_type, creator) = if finder_info {
        let (file_type, creator, _) = ppc_read_finfo(ctx.memory, info + 72).ok_or(PPC_PARAM_ERR)?;
        (file_type, creator)
    } else {
        (unknown, unknown)
    };
    let spec = ppc_fsref_child_spec(ctx, parent.vref, dir_id, &name)?;
    let mut call = ctx.cpu.clone();
    call.gpr[3] = spec;
    call.gpr[4] = creator;
    call.gpr[5] = file_type;
    let result = ppc_fsp_create(&mut call, ctx.memory, ctx.vfs_directories, ctx.vfs_files);
    if result != PPC_NO_ERR {
        return Err(result);
    }
    let object = ppc_fsref_write_created(ctx, parent.vref, dir_id, name, ref_out, spec_out)?;
    if finder_info {
        ppc_fsref_apply_finder_info(ctx, &object, info)?;
    }
    Ok(PPC_NO_ERR)
}

/// FSCreateDirectoryUnicode(const FSRef *parentRef, UniCharCount nameLength,
/// const UniChar *name, FSCatalogInfoBitmap whichInfo,
/// const FSCatalogInfo *catalogInfo, FSRef *newRef, FSSpec *newSpec,
/// UInt32 *newDirID).
fn ppc_fs_create_directory_unicode(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let gpr = ctx.cpu.gpr;
    let (parent_ptr, name_len, name_ptr, which, info) = (gpr[3], gpr[4], gpr[5], gpr[6], gpr[7]);
    let (ref_out, spec_out, dir_id_out) = (gpr[8], gpr[9], gpr[10]);
    let (parent, parent_dir_id) = ppc_fsref_resolve_directory_ptr(ctx, parent_ptr)?;
    let name = ppc_read_new_unicode_name(ctx.memory, name_ptr, name_len)?;
    let spec = ppc_fsref_child_spec(ctx, parent.vref, parent_dir_id, &name)?;
    let created_dir_id = spec + PPC_FSSPEC_SIZE as u32 + 2;
    let mut call = ctx.cpu.clone();
    call.gpr[3] = spec;
    call.gpr[5] = created_dir_id;
    let result = ppc_fsp_dir_create(
        &mut call,
        ctx.memory,
        ctx.vfs_directories,
        ctx.next_vfs_dir_id,
        ctx.default_dir_id,
    );
    if result != PPC_NO_ERR {
        return Err(result);
    }
    let object = ppc_fsref_write_created(ctx, parent.vref, parent_dir_id, name, ref_out, spec_out)?;
    if which & PPC_FS_CAT_INFO_FINDER_INFO != 0 {
        ppc_fsref_apply_finder_info(ctx, &object, info)?;
    }
    if dir_id_out != 0 {
        let dir_id = ctx.memory.read_u32_be(created_dir_id).unwrap_or(0);
        ctx.memory
            .write_u32_be(dir_id_out, dir_id)
            .ok_or(PPC_PARAM_ERR)?;
    }
    Ok(PPC_NO_ERR)
}

/// FSRenameUnicode(const FSRef *ref, UniCharCount nameLength,
/// const UniChar *name, TextEncoding textEncodingHint, FSRef *newRef)
/// renames in place through HRename.
fn ppc_fs_rename_unicode(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let gpr = ctx.cpu.gpr;
    let (ref_ptr, name_len, name_ptr, ref_out) = (gpr[3], gpr[4], gpr[5], gpr[7]);
    let object = ppc_fsref_resolve_ptr(ctx, ref_ptr)?;
    let new_name = ppc_read_new_unicode_name(ctx.memory, name_ptr, name_len)?;
    let spec = ppc_fsref_scratch(ctx.cpu, ctx.memory)?;
    let new_name_ptr = spec + PPC_FSSPEC_SIZE as u32 + 2;
    if !ppc_write_fsref_spec(ctx.memory, spec, &object)
        || !ppc_write_pstring_bytes(ctx.memory, new_name_ptr, &new_name)
    {
        return Err(PPC_PARAM_ERR);
    }
    let mut call = ctx.cpu.clone();
    call.gpr[3] = object.vref as u16 as u32;
    call.gpr[4] = object.parent_dir_id;
    call.gpr[5] = spec + 6;
    call.gpr[6] = new_name_ptr;
    let result = ppc_h_rename(
        &call,
        ctx.memory,
        ctx.vfs_directories,
        ctx.vfs_files,
        ctx.deleted_vfs_file_paths,
        ctx.files,
        ctx.vfs_resource_files,
        ctx.resource_files,
        ctx.vfs_resources,
        ctx.default_dir_id,
    );
    if result != PPC_NO_ERR {
        return Err(result);
    }
    if ref_out != 0 {
        let renamed = ppc_fsref_resolve(ctx, (object.vref, object.parent_dir_id, new_name))?;
        ppc_fsref_write_output(ctx, ref_out, &renamed)?;
    }
    Ok(PPC_NO_ERR)
}

/// FSFindFolder(short vRefNum, OSType folderType, Boolean createFolder,
/// FSRef *foundRef): FindFolder's directory as an FSRef (Folders.h).
fn ppc_fs_find_folder(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let (folder_type, out) = (ctx.cpu.gpr[4], ctx.cpu.gpr[6]);
    if out == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let dir_id = ppc_find_folder_dir_id(folder_type);
    let path = ppc_directory_path_for_id(ctx.vfs_directories, dir_id)
        .ok_or(PPC_FNF_ERR)?
        .to_string();
    let object = ppc_fsref_object_for_path(ctx, PPC_BOOT_VOLUME_REF_NUM, &path);
    ppc_fsref_write_output(ctx, out, &object)?;
    Ok(PPC_NO_ERR)
}

/// FSResolveAliasFile(FSRef *theRef, Boolean resolveAliasChains,
/// Boolean *targetIsFolder, Boolean *wasAliased) and its WithMountFlags
/// form. The VFS has no alias files in FSRef space, so the ref resolves to
/// itself.
fn ppc_fs_resolve_alias_file(ctx: &mut PpcFsRefContext) -> Result<i16, i16> {
    let (ref_ptr, folder_out, aliased_out) = (ctx.cpu.gpr[3], ctx.cpu.gpr[5], ctx.cpu.gpr[6]);
    let object = ppc_fsref_resolve_ptr(ctx, ref_ptr)?;
    ppc_fsref_write_output(ctx, ref_ptr, &object)?;
    if folder_out != 0 {
        ctx.memory
            .write_u8(folder_out, u8::from(object.dir_id.is_some()))
            .ok_or(PPC_PARAM_ERR)?;
    }
    if aliased_out != 0 {
        ctx.memory.write_u8(aliased_out, 0).ok_or(PPC_PARAM_ERR)?;
    }
    Ok(PPC_NO_ERR)
}

/// Encodes a volume's root directory FSRef for FSGetVolumeInfo.
pub(super) fn ppc_write_volume_root_fsref(
    memory: &mut PpcSectionMem,
    ref_ptr: u32,
    vref: i16,
    volume_name: &str,
) -> bool {
    let mut name = encode_mac_roman_lossy(volume_name);
    name.truncate(PPC_FSSPEC_MAX_NAME_LEN);
    ppc_write_fsref_parts(memory, ref_ptr, vref, PPC_FS_RT_PAR_ID, &name)
}
