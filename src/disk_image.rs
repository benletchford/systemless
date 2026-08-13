//! Read-only extraction helpers for classic Mac disk images.
//!
//! The runtime VFS is not a mounted HFS volume; it is a set of data-fork,
//! resource-fork, and Finder metadata maps. These helpers turn DC42/raw
//! HFS/HFS+ images into entries that can be seeded into that existing VFS.

use std::{
    io::{Read, Seek, SeekFrom},
    path::{Component, Path},
};

use hfs_reader::HfsVolume;

use crate::mac_roman::decode_mac_roman;

const HFS_SIGNATURE: u16 = 0x4244;
const HFS_PLUS_SIGNATURE: u16 = 0x482B;
const HFSX_SIGNATURE: u16 = 0x4858;
const MFS_SIGNATURE: u16 = 0xD2D7;
const DRIVER_DESCRIPTOR_SIGNATURE: u16 = 0x4552;
const APPLE_PARTITION_MAP_SIGNATURE: u16 = 0x504D;
const APPLE_HFS_PARTITION_TYPE: &[u8] = b"Apple_HFS";
const APPLE_HFSX_PARTITION_TYPE: &[u8] = b"Apple_HFSX";
const HFSPLUS_FORK_DATA: u8 = 0x00;
const HFSPLUS_FORK_RESOURCE: u8 = 0xFF;
const HFSPLUS_CATALOG_FILE_RECORD: u16 = 0x0002;
const HFSPLUS_FILE_USER_INFO_OFFSET: usize = 48;
const HFS_MDB_VOLUME_NAME_OFFSET: usize = 1024 + 36;
const HFS_MAX_VOLUME_NAME_LEN: usize = 27;

#[derive(Debug)]
pub struct DiskImageContents {
    pub volume_name: String,
    pub dirs: Vec<String>,
    pub files: Vec<DiskImageFile>,
}

#[derive(Debug)]
pub struct DiskImageFile {
    pub path: String,
    pub data: Vec<u8>,
    pub rsrc: Vec<u8>,
    pub file_type: [u8; 4],
    pub creator: [u8; 4],
    pub finder_flags: u16,
}

pub fn looks_like_dc42_or_hfs(bytes: &[u8]) -> bool {
    raw_filesystem_signature(bytes).is_some()
        || dc42_data_range(bytes)
            .and_then(|(start, end)| raw_filesystem_signature(&bytes[start..end]))
            .is_some()
        || apple_hfs_partition_range(bytes).is_some()
}

pub fn extract_dc42_or_hfs(bytes: &[u8]) -> Result<Option<DiskImageContents>, String> {
    if !looks_like_dc42_or_hfs(bytes) {
        return Ok(None);
    }

    let filesystem = filesystem_payload(bytes);
    match raw_filesystem_signature(filesystem) {
        Some(HFS_PLUS_SIGNATURE | HFSX_SIGNATURE) => {
            return extract_hfsplus(filesystem).map(Some);
        }
        Some(HFS_SIGNATURE | MFS_SIGNATURE) => {}
        Some(_) | None => {}
    }

    let volume =
        HfsVolume::parse(filesystem).map_err(|e| format!("failed to parse HFS image: {e}"))?;
    let volume_name = hfs_volume_name_from_mdb(filesystem)
        .or_else(|| clean_component(&volume.volume_name))
        .unwrap_or_else(|| "Disk Image".into());
    let mut dirs = vec![volume_name.clone()];

    for dir in &volume.dirs {
        if let Some(rel_path) = path_to_vfs_path(&dir.rel_path) {
            dirs.push(prefixed_path(&volume_name, &rel_path));
        }
    }

    let mut files = Vec::with_capacity(volume.files.len());
    for file in &volume.files {
        let Some(rel_path) = path_to_vfs_path(&file.rel_path) else {
            continue;
        };
        let path = prefixed_path(&volume_name, &rel_path);
        let data = volume
            .read_data_fork(file)
            .map_err(|e| format!("failed to read HFS data fork for {path}: {e}"))?;
        let rsrc = volume
            .read_rsrc_fork(file)
            .map_err(|e| format!("failed to read HFS resource fork for {path}: {e}"))?;

        files.push(DiskImageFile {
            path,
            data,
            rsrc,
            file_type: file.file_type,
            creator: file.creator,
            // hfs-reader exposes type/creator but not fdFlags yet.
            finder_flags: 0,
        });
    }

    dirs.sort_unstable();
    dirs.dedup();
    Ok(Some(DiskImageContents {
        volume_name,
        dirs,
        files,
    }))
}

fn raw_filesystem_signature(bytes: &[u8]) -> Option<u16> {
    let sig = bytes
        .get(1024..1026)
        .map(|sig| u16::from_be_bytes([sig[0], sig[1]]))?;
    matches!(
        sig,
        HFS_SIGNATURE | HFS_PLUS_SIGNATURE | HFSX_SIGNATURE | MFS_SIGNATURE
    )
    .then_some(sig)
}

fn filesystem_payload(bytes: &[u8]) -> &[u8] {
    dc42_data_range(bytes)
        .and_then(|(start, end)| bytes.get(start..end))
        .or_else(|| apple_hfs_partition_range(bytes).and_then(|(start, end)| bytes.get(start..end)))
        .unwrap_or(bytes)
}

fn hfs_volume_name_from_mdb(filesystem: &[u8]) -> Option<String> {
    if raw_filesystem_signature(filesystem) != Some(HFS_SIGNATURE) {
        return None;
    }
    // The HFS master directory block stores its authoritative volume name as
    // a Str27 Pascal string. Decode those bytes directly because hfs-reader's
    // host-path representation has already replaced non-ASCII characters.
    let name_len = *filesystem.get(HFS_MDB_VOLUME_NAME_OFFSET)? as usize;
    if name_len == 0 || name_len > HFS_MAX_VOLUME_NAME_LEN {
        return None;
    }
    let name_start = HFS_MDB_VOLUME_NAME_OFFSET + 1;
    let name = filesystem.get(name_start..name_start.checked_add(name_len)?)?;
    clean_component(&decode_mac_roman(name))
}

/// Locate the first supported HFS-family partition in an Apple Partition Map image.
/// The driver descriptor record supplies the physical block size; each
/// partition-map entry then identifies the partition start, length, data extent,
/// and type.
/// Inside Macintosh: Devices (1994), pp. 3-13–3-15, 3-25–3-27.
fn apple_hfs_partition_range(bytes: &[u8]) -> Option<(usize, usize)> {
    let block_size = read_u16_at(bytes, 2)? as usize;
    if read_u16_at(bytes, 0)? != DRIVER_DESCRIPTOR_SIGNATURE
        || block_size < 512
        || block_size % 512 != 0
    {
        return None;
    }

    let first_entry = block_size;
    if read_u16_at(bytes, first_entry)? != APPLE_PARTITION_MAP_SIGNATURE {
        return None;
    }
    let map_block_count = read_u32_at(bytes, first_entry + 4)? as usize;
    if map_block_count == 0 {
        return None;
    }

    for map_index in 1..=map_block_count {
        let entry = block_size.checked_mul(map_index)?;
        let entry_end = entry.checked_add(512)?;
        if entry_end > bytes.len() {
            break;
        }
        if read_u16_at(bytes, entry)? != APPLE_PARTITION_MAP_SIGNATURE {
            continue;
        }
        let partition_type = bytes.get(entry + 48..entry + 80)?;
        if !matches!(
            partition_type,
            field if fixed_apm_field_equals(field, APPLE_HFS_PARTITION_TYPE)
                || fixed_apm_field_equals(field, APPLE_HFSX_PARTITION_TYPE)
        ) {
            continue;
        }

        let partition_start_blocks = read_u32_at(bytes, entry + 8)? as usize;
        let partition_block_count = read_u32_at(bytes, entry + 12)? as usize;
        let data_start_blocks = read_u32_at(bytes, entry + 80)? as usize;
        let data_block_count = read_u32_at(bytes, entry + 84)? as usize;
        if data_block_count == 0
            || data_start_blocks > partition_block_count
            || data_block_count > partition_block_count.saturating_sub(data_start_blocks)
        {
            continue;
        }
        let filesystem_start_blocks = partition_start_blocks.checked_add(data_start_blocks)?;
        let start = filesystem_start_blocks.checked_mul(block_size)?;
        let end = filesystem_start_blocks
            .checked_add(data_block_count)?
            .checked_mul(block_size)?;
        if start >= end || end > bytes.len() {
            continue;
        }

        let filesystem = bytes.get(start..end)?;
        if matches!(
            raw_filesystem_signature(filesystem),
            Some(HFS_SIGNATURE | HFS_PLUS_SIGNATURE | HFSX_SIGNATURE)
        ) {
            return Some((start, end));
        }
    }

    None
}

fn fixed_apm_field_equals(field: &[u8], expected: &[u8]) -> bool {
    field
        .iter()
        .position(|&byte| byte == 0)
        .is_some_and(|terminator| field.get(..terminator) == Some(expected))
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let raw = bytes.get(offset..offset + 2)?;
    Some(u16::from_be_bytes([raw[0], raw[1]]))
}

fn read_u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    Some(u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn dc42_data_range(bytes: &[u8]) -> Option<(usize, usize)> {
    const DC42_HEADER_LEN: usize = 84;

    if bytes.len() < DC42_HEADER_LEN || bytes.get(82..84) != Some(&[0x01, 0x00]) {
        return None;
    }

    let name_len = bytes[0] as usize;
    if name_len > 63 {
        return None;
    }

    let data_size = u32::from_be_bytes(bytes[64..68].try_into().ok()?) as usize;
    if data_size == 0 || data_size % 512 != 0 {
        return None;
    }

    let data_end = DC42_HEADER_LEN.checked_add(data_size)?;
    (data_end <= bytes.len()).then_some((DC42_HEADER_LEN, data_end))
}

fn extract_hfsplus(bytes: &[u8]) -> Result<DiskImageContents, String> {
    let mut reader = std::io::Cursor::new(bytes);
    let volume = hfsplus::volume::VolumeHeader::parse(&mut reader)
        .map_err(|e| format!("failed to parse HFS+ image: {e}"))?;
    let catalog =
        hfsplus::btree::read_btree_header(&mut reader, &volume.catalog_file, volume.block_size)
            .map_err(|e| format!("failed to read HFS+ catalog B-tree: {e}"))?;
    let extents = if volume.extents_file.total_blocks == 0 {
        None
    } else {
        Some(
            hfsplus::btree::read_btree_header(&mut reader, &volume.extents_file, volume.block_size)
                .map_err(|e| format!("failed to read HFS+ extents B-tree: {e}"))?,
        )
    };

    // HFS+ stores the volume name in catalog thread records rather than the
    // volume header. Keep the same deterministic VFS mount shape as nameless
    // disk-image payloads instead of guessing from optional catalog metadata.
    let volume_name = "HFS+ Disk Image".to_string();
    let mut dirs = vec![volume_name.clone()];
    let mut files = Vec::new();
    collect_hfsplus_directory(
        &mut reader,
        &volume,
        &catalog,
        extents.as_ref(),
        &volume_name,
        hfsplus::catalog::CNID_ROOT_FOLDER,
        "",
        "",
        &mut dirs,
        &mut files,
    )?;

    dirs.sort_unstable();
    dirs.dedup();
    Ok(DiskImageContents {
        volume_name,
        dirs,
        files,
    })
}

#[allow(clippy::too_many_arguments)]
fn collect_hfsplus_directory<R: Read + Seek>(
    reader: &mut R,
    volume: &hfsplus::volume::VolumeHeader,
    catalog: &hfsplus::btree::BTreeHeaderRecord,
    extents: Option<&hfsplus::btree::BTreeHeaderRecord>,
    volume_name: &str,
    parent_cnid: u32,
    raw_dir: &str,
    vfs_dir: &str,
    dirs: &mut Vec<String>,
    files: &mut Vec<DiskImageFile>,
) -> Result<(), String> {
    let entries = hfsplus::catalog::list_directory(reader, volume, catalog, parent_cnid)
        .map_err(|e| format!("failed to list HFS+ directory {vfs_dir}: {e}"))?;

    for entry in entries {
        let Some(cleaned_name) = clean_component(&entry.name) else {
            continue;
        };
        let raw_path = join_path(raw_dir, &entry.name);
        let vfs_path = join_path(vfs_dir, &cleaned_name);

        match entry.kind {
            hfsplus::EntryKind::Directory => {
                dirs.push(prefixed_path(volume_name, &vfs_path));
                collect_hfsplus_directory(
                    reader,
                    volume,
                    catalog,
                    extents,
                    volume_name,
                    entry.cnid,
                    &raw_path,
                    &vfs_path,
                    dirs,
                    files,
                )?;
            }
            hfsplus::EntryKind::File | hfsplus::EntryKind::Symlink => {
                let lookup_path = format!("/{raw_path}");
                let (record, _) =
                    hfsplus::catalog::resolve_path(reader, volume, catalog, &lookup_path)
                        .map_err(|e| format!("failed to resolve HFS+ file {vfs_path}: {e}"))?;
                let hfsplus::catalog::CatalogRecord::File(file) = record else {
                    continue;
                };
                let path = prefixed_path(volume_name, &vfs_path);
                let metadata = hfsplus_file_finder_metadata(
                    reader,
                    catalog,
                    parent_cnid,
                    &entry.name,
                    &vfs_path,
                )?;
                let data = read_hfsplus_fork(
                    reader,
                    volume,
                    extents,
                    &file.data_fork,
                    file.file_id,
                    HFSPLUS_FORK_DATA,
                )
                .map_err(|e| format!("failed to read HFS+ data fork for {path}: {e}"))?;
                let rsrc = read_hfsplus_fork(
                    reader,
                    volume,
                    extents,
                    &file.resource_fork,
                    file.file_id,
                    HFSPLUS_FORK_RESOURCE,
                )
                .map_err(|e| format!("failed to read HFS+ resource fork for {path}: {e}"))?;

                files.push(DiskImageFile {
                    path,
                    data,
                    rsrc,
                    file_type: metadata.file_type,
                    creator: metadata.creator,
                    finder_flags: metadata.finder_flags,
                });
            }
        }
    }

    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct HfsPlusFinderMetadata {
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
}

impl Default for HfsPlusFinderMetadata {
    fn default() -> Self {
        Self {
            file_type: *b"????",
            creator: *b"????",
            finder_flags: 0,
        }
    }
}

fn hfsplus_file_finder_metadata<R: Read + Seek>(
    reader: &mut R,
    catalog: &hfsplus::btree::BTreeHeaderRecord,
    parent_cnid: u32,
    name: &str,
    vfs_path: &str,
) -> Result<HfsPlusFinderMetadata, String> {
    let name_utf16: Vec<u16> = name.encode_utf16().collect();
    let records = hfsplus::btree::scan_leaves(
        reader,
        catalog,
        catalog.first_leaf_node,
        &|record_data| {
            let Some((key_parent, key_name, _)) = hfsplus_catalog_key(record_data) else {
                return Some(false);
            };
            Some(key_parent == parent_cnid && key_name == name_utf16)
        },
        &|record_data| Ok(hfsplus_file_finder_metadata_from_record(record_data)),
    )
    .map_err(|e| format!("failed to read HFS+ Finder metadata for {vfs_path}: {e}"))?;

    Ok(records.into_iter().next().flatten().unwrap_or_default())
}

fn hfsplus_file_finder_metadata_from_record(record_data: &[u8]) -> Option<HfsPlusFinderMetadata> {
    let (_, _, record_offset) = hfsplus_catalog_key(record_data)?;
    let record = record_data.get(record_offset..)?;
    if record.len() < HFSPLUS_FILE_USER_INFO_OFFSET + 10 {
        return None;
    }
    let record_type = u16::from_be_bytes([record[0], record[1]]);
    if record_type != HFSPLUS_CATALOG_FILE_RECORD {
        return None;
    }
    let finder = &record[HFSPLUS_FILE_USER_INFO_OFFSET..];
    let file_type = [finder[0], finder[1], finder[2], finder[3]];
    let creator = [finder[4], finder[5], finder[6], finder[7]];
    let (file_type, creator) = if file_type == [0; 4] && creator == [0; 4] {
        (*b"????", *b"????")
    } else {
        (file_type, creator)
    };
    Some(HfsPlusFinderMetadata {
        file_type,
        creator,
        finder_flags: u16::from_be_bytes([finder[8], finder[9]]),
    })
}

fn hfsplus_catalog_key(record_data: &[u8]) -> Option<(u32, Vec<u16>, usize)> {
    let header = record_data.get(0..8)?;
    let key_length = u16::from_be_bytes([header[0], header[1]]) as usize;
    let parent_id = u32::from_be_bytes([header[2], header[3], header[4], header[5]]);
    let name_len = u16::from_be_bytes([header[6], header[7]]) as usize;
    let name_end = 8usize.checked_add(name_len.checked_mul(2)?)?;
    let name_bytes = record_data.get(8..name_end)?;
    let name = name_bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_be_bytes([chunk[0], chunk[1]]))
        .collect();
    let record_offset = (2usize.checked_add(key_length)? + 1) & !1;
    (record_offset <= record_data.len()).then_some((parent_id, name, record_offset))
}

fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}/{name}")
    }
}

fn read_hfsplus_fork<R: Read + Seek>(
    reader: &mut R,
    volume: &hfsplus::volume::VolumeHeader,
    extents: Option<&hfsplus::btree::BTreeHeaderRecord>,
    fork: &hfsplus::volume::ForkData,
    file_id: u32,
    fork_type: u8,
) -> Result<Vec<u8>, String> {
    let logical_size =
        usize::try_from(fork.logical_size).map_err(|_| "fork is too large".to_string())?;
    if logical_size == 0 {
        return Ok(Vec::new());
    }

    let mut out = Vec::with_capacity(logical_size);
    for extent in &fork.extents {
        if extent.block_count == 0 || out.len() >= logical_size {
            break;
        }
        read_hfsplus_extent(reader, volume.block_size, extent, logical_size, &mut out)?;
    }

    let mut start_block = fork.extents.iter().map(|extent| extent.block_count).sum();
    while out.len() < logical_size {
        let extents = extents.ok_or_else(|| "missing HFS+ extents B-tree".to_string())?;
        let overflow = hfsplus_overflow_extents(reader, extents, file_id, fork_type, start_block)?;
        if overflow.is_empty() {
            break;
        }

        for extent in overflow {
            if extent.block_count == 0 || out.len() >= logical_size {
                break;
            }
            read_hfsplus_extent(reader, volume.block_size, &extent, logical_size, &mut out)?;
            start_block = start_block.saturating_add(extent.block_count);
        }
    }

    if out.len() < logical_size {
        return Err(format!(
            "fork truncated: read {} of {} bytes",
            out.len(),
            logical_size
        ));
    }
    out.truncate(logical_size);
    Ok(out)
}

fn read_hfsplus_extent<R: Read + Seek>(
    reader: &mut R,
    block_size: u32,
    extent: &hfsplus::volume::ExtentDescriptor,
    logical_size: usize,
    out: &mut Vec<u8>,
) -> Result<(), String> {
    let offset = u64::from(extent.start_block)
        .checked_mul(u64::from(block_size))
        .ok_or_else(|| "HFS+ extent offset overflow".to_string())?;
    let byte_len = u64::from(extent.block_count)
        .checked_mul(u64::from(block_size))
        .ok_or_else(|| "HFS+ extent length overflow".to_string())?;
    let remaining = logical_size.saturating_sub(out.len());
    let mut to_read = usize::try_from(byte_len)
        .unwrap_or(usize::MAX)
        .min(remaining);
    reader
        .seek(SeekFrom::Start(offset))
        .map_err(|e| format!("seek HFS+ extent: {e}"))?;

    while to_read > 0 {
        let chunk = to_read.min(64 * 1024);
        let start = out.len();
        out.resize(start + chunk, 0);
        reader
            .read_exact(&mut out[start..start + chunk])
            .map_err(|e| format!("read HFS+ extent: {e}"))?;
        to_read -= chunk;
    }

    Ok(())
}

fn hfsplus_overflow_extents<R: Read + Seek>(
    reader: &mut R,
    extents: &hfsplus::btree::BTreeHeaderRecord,
    file_id: u32,
    fork_type: u8,
    start_block: u32,
) -> Result<Vec<hfsplus::volume::ExtentDescriptor>, String> {
    let records = hfsplus::btree::scan_leaves(
        reader,
        extents,
        extents.first_leaf_node,
        &|record_data| {
            let key = hfsplus_extent_key(record_data)?;
            Some(key == (fork_type, file_id, start_block))
        },
        &|record_data| {
            let key_length = u16::from_be_bytes([record_data[0], record_data[1]]) as usize;
            let data_start = 2 + key_length;
            let data = record_data
                .get(data_start..data_start + 64)
                .ok_or_else(|| {
                    hfsplus::HfsPlusError::InvalidBTree("extent record too short".into())
                })?;
            let mut extents = Vec::with_capacity(8);
            for chunk in data.chunks_exact(8) {
                extents.push(hfsplus::volume::ExtentDescriptor {
                    start_block: u32::from_be_bytes(chunk[0..4].try_into().unwrap()),
                    block_count: u32::from_be_bytes(chunk[4..8].try_into().unwrap()),
                });
            }
            Ok(extents)
        },
    )
    .map_err(|e| format!("read HFS+ overflow extents: {e}"))?;

    Ok(records.into_iter().flatten().collect())
}

fn hfsplus_extent_key(record_data: &[u8]) -> Option<(u8, u32, u32)> {
    (record_data.len() >= 12).then(|| {
        (
            record_data[2],
            u32::from_be_bytes(record_data[4..8].try_into().unwrap()),
            u32::from_be_bytes(record_data[8..12].try_into().unwrap()),
        )
    })
}

fn path_to_vfs_path(path: &Path) -> Option<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            continue;
        };
        let Some(cleaned) = clean_component(&part.to_string_lossy()) else {
            continue;
        };
        parts.push(cleaned);
    }

    (!parts.is_empty()).then(|| parts.join("/"))
}

fn prefixed_path(volume_name: &str, rel_path: &str) -> String {
    if volume_name.is_empty() {
        rel_path.to_string()
    } else {
        format!("{volume_name}/{rel_path}")
    }
}

fn clean_component(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .map(|ch| match ch {
            '/' | ':' | '\\' => '_',
            ch if ch.is_control() => '_',
            ch => ch,
        })
        .collect();
    let trimmed = cleaned.trim();
    (!trimmed.is_empty() && trimmed != "." && trimmed != "..").then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_hfs_mdb_volume_name_as_mac_roman() {
        let mut filesystem = vec![0; HFS_MDB_VOLUME_NAME_OFFSET + 9];
        filesystem[1024..1026].copy_from_slice(&HFS_SIGNATURE.to_be_bytes());
        filesystem[HFS_MDB_VOLUME_NAME_OFFSET] = 8;
        filesystem[HFS_MDB_VOLUME_NAME_OFFSET + 1..].copy_from_slice(b"TETRIS\xA51");

        assert_eq!(
            hfs_volume_name_from_mdb(&filesystem).as_deref(),
            Some("TETRIS•1")
        );
    }

    #[test]
    fn rejects_invalid_hfs_mdb_volume_name_bounds() {
        let mut filesystem = vec![0; HFS_MDB_VOLUME_NAME_OFFSET + 2];
        filesystem[1024..1026].copy_from_slice(&HFS_SIGNATURE.to_be_bytes());

        filesystem[HFS_MDB_VOLUME_NAME_OFFSET] = 28;
        assert_eq!(hfs_volume_name_from_mdb(&filesystem), None);

        filesystem[HFS_MDB_VOLUME_NAME_OFFSET] = 2;
        assert_eq!(hfs_volume_name_from_mdb(&filesystem), None);
    }

    #[test]
    fn detects_raw_hfs_volume_signature() {
        let mut bytes = vec![0; 2048];
        bytes[1024..1026].copy_from_slice(&HFS_SIGNATURE.to_be_bytes());

        assert!(looks_like_dc42_or_hfs(&bytes));
    }

    #[test]
    fn detects_raw_hfsplus_and_hfsx_volume_signatures() {
        for signature in [HFS_PLUS_SIGNATURE, HFSX_SIGNATURE] {
            let mut bytes = vec![0; 2048];
            bytes[1024..1026].copy_from_slice(&signature.to_be_bytes());

            assert!(looks_like_dc42_or_hfs(&bytes));
        }
    }

    #[test]
    fn detects_hfs_volume_inside_apple_partition_map() {
        const PARTITION_START: usize = 64;
        const PARTITION_BLOCKS: usize = 4;
        let mut bytes = apm_fixture(PARTITION_START + PARTITION_BLOCKS + 2, 2);
        write_apm_partition(
            &mut bytes,
            2,
            PARTITION_START,
            PARTITION_BLOCKS,
            0,
            PARTITION_BLOCKS,
            APPLE_HFS_PARTITION_TYPE,
        );
        bytes[PARTITION_START * APM_BLOCK_SIZE + 1024..PARTITION_START * APM_BLOCK_SIZE + 1026]
            .copy_from_slice(&HFS_SIGNATURE.to_be_bytes());

        assert_eq!(
            apple_hfs_partition_range(&bytes),
            Some((
                PARTITION_START * APM_BLOCK_SIZE,
                bytes.len() - 2 * APM_BLOCK_SIZE
            ))
        );
        assert!(looks_like_dc42_or_hfs(&bytes));
    }

    #[test]
    fn detects_hfsplus_partition_inside_mixed_apple_partition_map() {
        const PARTITION_START: usize = 32;
        const PARTITION_BLOCKS: usize = 16;
        const DATA_START: usize = 2;
        const DATA_BLOCKS: usize = 8;
        let mut bytes = apm_fixture(128, 3);
        write_apm_partition(&mut bytes, 2, 16, 8, 0, 8, b"Apple_Free");
        write_apm_partition(
            &mut bytes,
            3,
            PARTITION_START,
            PARTITION_BLOCKS,
            DATA_START,
            DATA_BLOCKS,
            APPLE_HFSX_PARTITION_TYPE,
        );
        let filesystem_start = (PARTITION_START + DATA_START) * APM_BLOCK_SIZE;
        bytes[filesystem_start + 1024..filesystem_start + 1026]
            .copy_from_slice(&HFS_PLUS_SIGNATURE.to_be_bytes());

        assert_eq!(
            apple_hfs_partition_range(&bytes),
            Some((
                filesystem_start,
                (PARTITION_START + DATA_START + DATA_BLOCKS) * APM_BLOCK_SIZE
            ))
        );
        assert!(looks_like_dc42_or_hfs(&bytes));
    }

    #[test]
    fn rejects_non_exact_apple_hfs_partition_type() {
        const PARTITION_START: usize = 16;
        const PARTITION_BLOCKS: usize = 8;
        let mut bytes = apm_fixture(PARTITION_START + PARTITION_BLOCKS + 2, 2);
        write_apm_partition(
            &mut bytes,
            2,
            PARTITION_START,
            PARTITION_BLOCKS,
            0,
            PARTITION_BLOCKS,
            b"Apple_HFS_backup",
        );
        bytes[PARTITION_START * APM_BLOCK_SIZE + 1024..PARTITION_START * APM_BLOCK_SIZE + 1026]
            .copy_from_slice(&HFS_SIGNATURE.to_be_bytes());

        assert!(apple_hfs_partition_range(&bytes).is_none());
        assert!(!looks_like_dc42_or_hfs(&bytes));
    }

    #[test]
    fn accepts_exact_apple_hfs_type_with_data_after_the_c_string() {
        const PARTITION_START: usize = 16;
        const PARTITION_BLOCKS: usize = 8;
        let mut bytes = apm_fixture(PARTITION_START + PARTITION_BLOCKS + 2, 2);
        write_apm_partition(
            &mut bytes,
            2,
            PARTITION_START,
            PARTITION_BLOCKS,
            0,
            PARTITION_BLOCKS,
            APPLE_HFS_PARTITION_TYPE,
        );
        let type_tail = 2 * APM_BLOCK_SIZE + 48 + APPLE_HFS_PARTITION_TYPE.len() + 1;
        bytes[type_tail] = 0xA5;
        bytes[PARTITION_START * APM_BLOCK_SIZE + 1024..PARTITION_START * APM_BLOCK_SIZE + 1026]
            .copy_from_slice(&HFS_SIGNATURE.to_be_bytes());

        assert!(apple_hfs_partition_range(&bytes).is_some());
    }

    #[test]
    fn apm_partition_types_are_exact_case_terminated_c_strings() {
        let mut exact = [0u8; 32];
        exact[..APPLE_HFS_PARTITION_TYPE.len()].copy_from_slice(APPLE_HFS_PARTITION_TYPE);
        assert!(fixed_apm_field_equals(&exact, APPLE_HFS_PARTITION_TYPE));

        let mut mixed_case = exact;
        mixed_case[0] = b'a';
        assert!(!fixed_apm_field_equals(
            &mixed_case,
            APPLE_HFS_PARTITION_TYPE
        ));

        let mut unterminated = [b'X'; 32];
        unterminated[..APPLE_HFS_PARTITION_TYPE.len()].copy_from_slice(APPLE_HFS_PARTITION_TYPE);
        assert!(!fixed_apm_field_equals(
            &unterminated,
            APPLE_HFS_PARTITION_TYPE
        ));
    }

    const APM_BLOCK_SIZE: usize = 512;

    fn apm_fixture(block_count: usize, map_block_count: u32) -> Vec<u8> {
        let mut bytes = vec![0; block_count * APM_BLOCK_SIZE];
        bytes[0..2].copy_from_slice(&DRIVER_DESCRIPTOR_SIGNATURE.to_be_bytes());
        bytes[2..4].copy_from_slice(&(APM_BLOCK_SIZE as u16).to_be_bytes());
        bytes[4..8].copy_from_slice(&(block_count as u32).to_be_bytes());

        let map_entry = APM_BLOCK_SIZE;
        bytes[map_entry..map_entry + 2]
            .copy_from_slice(&APPLE_PARTITION_MAP_SIGNATURE.to_be_bytes());
        bytes[map_entry + 4..map_entry + 8].copy_from_slice(&map_block_count.to_be_bytes());
        bytes[map_entry + 8..map_entry + 12].copy_from_slice(&1u32.to_be_bytes());
        bytes[map_entry + 12..map_entry + 16].copy_from_slice(&map_block_count.to_be_bytes());
        bytes[map_entry + 48..map_entry + 48 + 19].copy_from_slice(b"Apple_partition_map");
        bytes
    }

    fn write_apm_partition(
        bytes: &mut [u8],
        map_index: usize,
        partition_start: usize,
        partition_blocks: usize,
        data_start: usize,
        data_blocks: usize,
        partition_type: &[u8],
    ) {
        assert!(partition_type.len() <= 32);
        let entry = APM_BLOCK_SIZE * map_index;
        bytes[entry..entry + 2].copy_from_slice(&APPLE_PARTITION_MAP_SIGNATURE.to_be_bytes());
        bytes[entry + 4..entry + 8].copy_from_slice(&2u32.to_be_bytes());
        bytes[entry + 8..entry + 12].copy_from_slice(&(partition_start as u32).to_be_bytes());
        bytes[entry + 12..entry + 16].copy_from_slice(&(partition_blocks as u32).to_be_bytes());
        bytes[entry + 48..entry + 48 + partition_type.len()].copy_from_slice(partition_type);
        bytes[entry + 80..entry + 84].copy_from_slice(&(data_start as u32).to_be_bytes());
        bytes[entry + 84..entry + 88].copy_from_slice(&(data_blocks as u32).to_be_bytes());
    }

    #[test]
    fn detects_dc42_wrapped_hfs_payload() {
        let bytes = dc42_with_payload_signature(HFS_SIGNATURE);

        assert!(looks_like_dc42_or_hfs(&bytes));
    }

    #[test]
    fn extracts_hfsplus_data_fork_files() {
        let mut builder = hfsplus::testutil::HfsPlusImageBuilder::new();
        builder.add_file("hello.txt", b"hello hfs+", 0o100644);
        let bytes = builder.build();

        let image = extract_dc42_or_hfs(&bytes)
            .expect("HFS+ extraction should succeed")
            .expect("HFS+ signature should be detected");

        assert_eq!(image.volume_name, "HFS+ Disk Image");
        assert_eq!(image.dirs, vec!["HFS+ Disk Image".to_string()]);
        let file = image
            .files
            .iter()
            .find(|file| file.path == "HFS+ Disk Image/hello.txt")
            .expect("synthetic HFS+ file should be present");
        assert_eq!(file.data, b"hello hfs+");
        assert!(file.rsrc.is_empty());
        assert_eq!(file.file_type, *b"????");
        assert_eq!(file.creator, *b"????");
    }

    #[test]
    fn parses_hfsplus_catalog_finder_metadata() {
        let mut record = hfsplus_catalog_file_record("Star Trek JR Demo");
        let key_len = u16::from_be_bytes([record[0], record[1]]) as usize;
        let record_offset = (2 + key_len + 1) & !1;
        let finder = record_offset + HFSPLUS_FILE_USER_INFO_OFFSET;
        record[finder..finder + 4].copy_from_slice(b"APPL");
        record[finder + 4..finder + 8].copy_from_slice(b"MPLY");
        record[finder + 8..finder + 10].copy_from_slice(&0x0400u16.to_be_bytes());

        let metadata = hfsplus_file_finder_metadata_from_record(&record)
            .expect("file record metadata should parse");

        assert_eq!(
            hfsplus_catalog_key(&record)
                .expect("catalog key should parse")
                .0,
            42
        );
        assert_eq!(metadata.file_type, *b"APPL");
        assert_eq!(metadata.creator, *b"MPLY");
        assert_eq!(metadata.finder_flags, 0x0400);
    }

    #[test]
    fn empty_hfsplus_catalog_finder_codes_fall_back_to_unknown() {
        let record = hfsplus_catalog_file_record("Untyped");

        let metadata = hfsplus_file_finder_metadata_from_record(&record)
            .expect("file record metadata should parse");

        assert_eq!(metadata.file_type, *b"????");
        assert_eq!(metadata.creator, *b"????");
        assert_eq!(metadata.finder_flags, 0);
    }

    #[test]
    fn reads_hfsplus_resource_fork_inline_extents() {
        const BLOCK_SIZE: usize = 512;
        let mut bytes = vec![0u8; BLOCK_SIZE * 4];
        bytes[BLOCK_SIZE * 2..BLOCK_SIZE * 2 + 4].copy_from_slice(b"rsrc");
        let fork = hfsplus::volume::ForkData {
            logical_size: 4,
            clump_size: 0,
            total_blocks: 1,
            extents: {
                let mut extents = [hfsplus::volume::ExtentDescriptor::default(); 8];
                extents[0] = hfsplus::volume::ExtentDescriptor {
                    start_block: 2,
                    block_count: 1,
                };
                extents
            },
        };
        let volume = hfsplus::volume::VolumeHeader {
            signature: HFS_PLUS_SIGNATURE,
            version: 4,
            attributes: 0,
            last_mounted_version: 0,
            journal_info_block: 0,
            create_date: 0,
            modify_date: 0,
            backup_date: 0,
            checked_date: 0,
            file_count: 0,
            folder_count: 0,
            block_size: BLOCK_SIZE as u32,
            total_blocks: 4,
            free_blocks: 0,
            next_allocation: 0,
            rsrc_clump_size: 0,
            data_clump_size: 0,
            next_catalog_id: 0,
            write_count: 0,
            encoding_bitmap: 0,
            finder_info: [0; 8],
            allocation_file: hfsplus::volume::ForkData::default(),
            extents_file: hfsplus::volume::ForkData::default(),
            catalog_file: hfsplus::volume::ForkData::default(),
            attributes_file: hfsplus::volume::ForkData::default(),
            startup_file: hfsplus::volume::ForkData::default(),
            is_hfsx: false,
        };
        let mut reader = std::io::Cursor::new(bytes);

        let out = read_hfsplus_fork(&mut reader, &volume, None, &fork, 42, HFSPLUS_FORK_RESOURCE)
            .expect("inline resource fork should read");

        assert_eq!(out, b"rsrc");
    }

    #[test]
    fn rejects_dc42_like_data_without_filesystem_signature() {
        let bytes = dc42_with_payload_signature(0);

        assert!(!looks_like_dc42_or_hfs(&bytes));
    }

    fn dc42_with_payload_signature(signature: u16) -> Vec<u8> {
        const HEADER_LEN: usize = 84;
        const DATA_LEN: usize = 2048;

        let mut bytes = vec![0; HEADER_LEN + DATA_LEN];
        bytes[0] = 4;
        bytes[1..5].copy_from_slice(b"Test");
        bytes[64..68].copy_from_slice(&(DATA_LEN as u32).to_be_bytes());
        bytes[82..84].copy_from_slice(&[0x01, 0x00]);
        bytes[HEADER_LEN + 1024..HEADER_LEN + 1026].copy_from_slice(&signature.to_be_bytes());
        bytes
    }

    fn hfsplus_catalog_file_record(name: &str) -> Vec<u8> {
        let name_utf16: Vec<u16> = name.encode_utf16().collect();
        let key_len = 6 + name_utf16.len() * 2;
        let record_offset = (2 + key_len + 1) & !1;
        let mut record = vec![0u8; record_offset + 88];
        record[0..2].copy_from_slice(&(key_len as u16).to_be_bytes());
        record[2..6].copy_from_slice(&42u32.to_be_bytes());
        record[6..8].copy_from_slice(&(name_utf16.len() as u16).to_be_bytes());
        for (idx, ch) in name_utf16.iter().enumerate() {
            let start = 8 + idx * 2;
            record[start..start + 2].copy_from_slice(&ch.to_be_bytes());
        }
        record[record_offset..record_offset + 2]
            .copy_from_slice(&HFSPLUS_CATALOG_FILE_RECORD.to_be_bytes());
        record
    }
}
