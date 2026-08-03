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

const HFS_SIGNATURE: u16 = 0x4244;
const HFS_PLUS_SIGNATURE: u16 = 0x482B;
const HFSX_SIGNATURE: u16 = 0x4858;
const MFS_SIGNATURE: u16 = 0xD2D7;
const HFSPLUS_FORK_DATA: u8 = 0x00;
const HFSPLUS_FORK_RESOURCE: u8 = 0xFF;
const HFSPLUS_CATALOG_FILE_RECORD: u16 = 0x0002;
const HFSPLUS_FILE_USER_INFO_OFFSET: usize = 48;
const MFS_SECTOR_SIZE: usize = 512;
const MFS_MDB_SECTOR: usize = 2;
const MFS_MDB_SIZE: usize = 1024;
const MFS_MDB_VOLUME_INFO_SIZE: usize = 64;
const MFS_EOF_BLOCK: u16 = 1;

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
        Some(MFS_SIGNATURE) => return extract_mfs(filesystem).map(Some),
        Some(HFS_SIGNATURE) => {}
        Some(_) | None => {}
    }

    let volume = HfsVolume::parse(bytes).map_err(|e| format!("failed to parse HFS image: {e}"))?;
    let volume_name = clean_component(&volume.volume_name).unwrap_or_else(|| "Disk Image".into());
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
        .unwrap_or(bytes)
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

#[derive(Clone, Copy, Debug)]
struct MfsVolumeHeader {
    dir_start_sector: usize,
    dir_length_sectors: usize,
    allocation_block_count: usize,
    allocation_block_size: usize,
    allocation_start_sector: usize,
    volume_name_offset: usize,
    volume_name_length: usize,
}

fn extract_mfs(bytes: &[u8]) -> Result<DiskImageContents, String> {
    let header = parse_mfs_volume_header(bytes)?;
    let volume_name_bytes = bytes
        .get(
            header.volume_name_offset + 1
                ..header.volume_name_offset + 1 + header.volume_name_length,
        )
        .ok_or_else(|| "MFS volume name extends beyond the volume information block".to_string())?;
    let volume_name = clean_component(&crate::trap::decode_mac_roman(volume_name_bytes))
        .unwrap_or_else(|| "MFS Disk Image".into());

    let allocation_map = read_mfs_allocation_map(bytes, &header)?;
    let dir_start = header
        .dir_start_sector
        .checked_mul(MFS_SECTOR_SIZE)
        .ok_or_else(|| "MFS directory offset overflow".to_string())?;
    let dir_length = header
        .dir_length_sectors
        .checked_mul(MFS_SECTOR_SIZE)
        .ok_or_else(|| "MFS directory length overflow".to_string())?;
    let dir_end = dir_start
        .checked_add(dir_length)
        .ok_or_else(|| "MFS directory end overflow".to_string())?;
    if dir_end > bytes.len() {
        return Err(format!(
            "MFS directory extends beyond the volume: end {dir_end}, size {}",
            bytes.len()
        ));
    }

    let mut dirs = vec![volume_name.clone()];
    let mut files = Vec::new();
    let file_count = read_mfs_u16(bytes, 12, "file count")? as usize;
    let mut offset = dir_start;
    let mut parsed_files = 0usize;
    while offset < dir_end && parsed_files < file_count {
        let flags = *bytes
            .get(offset)
            .ok_or_else(|| "MFS directory entry is out of bounds".to_string())?;
        if flags & 0x80 == 0 {
            break;
        }

        const MFS_FILE_ENTRY_HEADER_SIZE: usize = 51;
        let entry_end = offset
            .checked_add(MFS_FILE_ENTRY_HEADER_SIZE)
            .ok_or_else(|| "MFS file entry offset overflow".to_string())?;
        if entry_end > dir_end {
            return Err("MFS file entry header extends beyond the directory".into());
        }
        let name_length = bytes[offset + 50] as usize;
        if name_length > 31 {
            return Err(format!("MFS file name is too long: {name_length} bytes"));
        }
        let unaligned_entry_length = MFS_FILE_ENTRY_HEADER_SIZE
            .checked_add(name_length)
            .ok_or_else(|| "MFS file entry length overflow".to_string())?;
        let entry_length = (unaligned_entry_length + 1) & !1;
        let entry_end = offset
            .checked_add(entry_length)
            .ok_or_else(|| "MFS file entry end overflow".to_string())?;
        if entry_end > dir_end {
            return Err("MFS file name extends beyond the directory".into());
        }

        let name_bytes = &bytes[offset + 51..offset + 51 + name_length];
        let Some(name) = clean_component(&crate::trap::decode_mac_roman(name_bytes)) else {
            offset = entry_end;
            parsed_files += 1;
            continue;
        };
        let path = prefixed_path(&volume_name, &name);
        let data = read_mfs_fork(
            bytes,
            &header,
            &allocation_map,
            read_mfs_u16_at(bytes, offset + 22, "data fork start block")?,
            read_mfs_u32_at(bytes, offset + 24, "data fork length")?,
            read_mfs_u32_at(bytes, offset + 28, "data fork allocation length")?,
            &path,
            "data",
        )?;
        let rsrc = read_mfs_fork(
            bytes,
            &header,
            &allocation_map,
            read_mfs_u16_at(bytes, offset + 32, "resource fork start block")?,
            read_mfs_u32_at(bytes, offset + 34, "resource fork length")?,
            read_mfs_u32_at(bytes, offset + 38, "resource fork allocation length")?,
            &path,
            "resource",
        )?;

        files.push(DiskImageFile {
            path,
            data,
            rsrc,
            file_type: bytes[offset + 2..offset + 6]
                .try_into()
                .map_err(|_| "MFS Finder type is malformed".to_string())?,
            creator: bytes[offset + 6..offset + 10]
                .try_into()
                .map_err(|_| "MFS Finder creator is malformed".to_string())?,
            finder_flags: read_mfs_u16_at(bytes, offset + 10, "Finder flags")?,
        });
        offset = entry_end;
        parsed_files += 1;
    }

    dirs.sort_unstable();
    dirs.dedup();
    Ok(DiskImageContents {
        volume_name,
        dirs,
        files,
    })
}

fn parse_mfs_volume_header(bytes: &[u8]) -> Result<MfsVolumeHeader, String> {
    let mdb = MFS_MDB_SECTOR
        .checked_mul(MFS_SECTOR_SIZE)
        .ok_or_else(|| "MFS volume information offset overflow".to_string())?;
    if bytes.len() < mdb + MFS_MDB_SIZE {
        return Err(format!(
            "MFS volume information block is truncated: size {}, need {}",
            bytes.len(),
            mdb + MFS_MDB_SIZE
        ));
    }
    if read_mfs_u16_at(bytes, mdb, "signature")? != MFS_SIGNATURE {
        return Err("MFS signature is missing".into());
    }

    let dir_start_sector = read_mfs_u16_at(bytes, mdb + 14, "directory start sector")? as usize;
    let dir_length_sectors = read_mfs_u16_at(bytes, mdb + 16, "directory length")? as usize;
    let allocation_block_count =
        read_mfs_u16_at(bytes, mdb + 18, "allocation block count")? as usize;
    let allocation_block_size = read_mfs_u32_at(bytes, mdb + 20, "allocation block size")? as usize;
    let allocation_start_sector =
        read_mfs_u16_at(bytes, mdb + 28, "allocation start sector")? as usize;
    let volume_name_offset = mdb + 36;
    let volume_name_length = bytes[volume_name_offset] as usize;

    if allocation_block_count == 0 {
        return Err("MFS volume has no allocation blocks".into());
    }
    if allocation_block_size == 0 || allocation_block_size % MFS_SECTOR_SIZE != 0 {
        return Err(format!(
            "MFS allocation block size is invalid: {allocation_block_size}"
        ));
    }
    if volume_name_length > 27 {
        return Err(format!(
            "MFS volume name is too long: {volume_name_length} bytes"
        ));
    }
    let allocation_bytes = allocation_block_count
        .checked_mul(allocation_block_size)
        .ok_or_else(|| "MFS allocation area length overflow".to_string())?;
    let allocation_start = allocation_start_sector
        .checked_mul(MFS_SECTOR_SIZE)
        .ok_or_else(|| "MFS allocation area offset overflow".to_string())?;
    let allocation_end = allocation_start
        .checked_add(allocation_bytes)
        .ok_or_else(|| "MFS allocation area end overflow".to_string())?;
    if allocation_end > bytes.len() {
        return Err(format!(
            "MFS allocation area extends beyond the volume: end {allocation_end}, size {}",
            bytes.len()
        ));
    }
    let directory_end = dir_start_sector
        .checked_add(dir_length_sectors)
        .and_then(|sectors| sectors.checked_mul(MFS_SECTOR_SIZE))
        .ok_or_else(|| "MFS directory range overflow".to_string())?;
    if directory_end > bytes.len() {
        return Err(format!(
            "MFS directory extends beyond the volume: end {directory_end}, size {}",
            bytes.len()
        ));
    }

    Ok(MfsVolumeHeader {
        dir_start_sector,
        dir_length_sectors,
        allocation_block_count,
        allocation_block_size,
        allocation_start_sector,
        volume_name_offset,
        volume_name_length,
    })
}

fn read_mfs_allocation_map(bytes: &[u8], header: &MfsVolumeHeader) -> Result<Vec<u16>, String> {
    let mdb = MFS_MDB_SECTOR * MFS_SECTOR_SIZE;
    let map_length = header.allocation_block_count.div_ceil(2) * 3;
    let map = bytes
        .get(mdb + MFS_MDB_VOLUME_INFO_SIZE..mdb + MFS_MDB_VOLUME_INFO_SIZE + map_length)
        .ok_or_else(|| "MFS allocation block map is truncated".to_string())?;
    let mut entries = Vec::with_capacity(header.allocation_block_count);
    for index in 0..header.allocation_block_count {
        let triplet = &map[(index / 2) * 3..(index / 2) * 3 + 3];
        let value = if index % 2 == 0 {
            u16::from(triplet[0]) << 4 | u16::from(triplet[1] >> 4)
        } else {
            u16::from(triplet[1] & 0x0F) << 8 | u16::from(triplet[2])
        };
        entries.push(value);
    }
    Ok(entries)
}

fn read_mfs_fork(
    bytes: &[u8],
    header: &MfsVolumeHeader,
    allocation_map: &[u16],
    start_block: u16,
    logical_length: u32,
    allocation_length: u32,
    path: &str,
    fork_name: &str,
) -> Result<Vec<u8>, String> {
    let logical_length = usize::try_from(logical_length)
        .map_err(|_| format!("MFS {fork_name} fork is too large for {path}"))?;
    let allocation_length = usize::try_from(allocation_length)
        .map_err(|_| format!("MFS {fork_name} allocation is too large for {path}"))?;
    if logical_length == 0 {
        return Ok(Vec::new());
    }
    if start_block < 2 {
        return Err(format!(
            "MFS {fork_name} fork for {path} has no valid start block"
        ));
    }
    if logical_length > allocation_length {
        return Err(format!(
            "MFS {fork_name} fork for {path} is longer than its allocation"
        ));
    }

    let mut data = Vec::with_capacity(logical_length);
    let mut seen = vec![false; allocation_map.len()];
    let mut current_block = start_block;
    while data.len() < logical_length {
        let index = usize::from(current_block)
            .checked_sub(2)
            .ok_or_else(|| format!("MFS {fork_name} fork for {path} has an invalid block chain"))?;
        if index >= allocation_map.len() {
            return Err(format!(
                "MFS {fork_name} fork for {path} points outside the allocation map"
            ));
        }
        if seen[index] {
            return Err(format!(
                "MFS {fork_name} fork for {path} contains an allocation cycle"
            ));
        }
        seen[index] = true;

        let block_offset = header
            .allocation_start_sector
            .checked_mul(MFS_SECTOR_SIZE)
            .and_then(|offset| offset.checked_add(index * header.allocation_block_size))
            .ok_or_else(|| format!("MFS {fork_name} block offset overflow for {path}"))?;
        let block_end = block_offset
            .checked_add(header.allocation_block_size)
            .ok_or_else(|| format!("MFS {fork_name} block end overflow for {path}"))?;
        if block_end > bytes.len() {
            return Err(format!(
                "MFS {fork_name} fork for {path} points beyond the volume"
            ));
        }
        let count = (logical_length - data.len()).min(header.allocation_block_size);
        data.extend_from_slice(&bytes[block_offset..block_offset + count]);
        if data.len() == logical_length {
            break;
        }

        let next_block = allocation_map[index];
        if next_block == MFS_EOF_BLOCK {
            return Err(format!(
                "MFS {fork_name} fork for {path} ends before its logical length"
            ));
        }
        if next_block < 2 {
            return Err(format!(
                "MFS {fork_name} fork for {path} has a free-block link"
            ));
        }
        current_block = next_block;
    }
    Ok(data)
}

fn read_mfs_u16(bytes: &[u8], offset: usize, field: &str) -> Result<u16, String> {
    read_mfs_u16_at(bytes, MFS_MDB_SECTOR * MFS_SECTOR_SIZE + offset, field)
}

fn read_mfs_u16_at(bytes: &[u8], offset: usize, field: &str) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("MFS {field} is truncated"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn read_mfs_u32_at(bytes: &[u8], offset: usize, field: &str) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("MFS {field} is truncated"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
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
    fn extracts_mfs_data_and_resource_forks_with_finder_metadata() {
        let bytes = mfs_test_image();

        let image = extract_dc42_or_hfs(&bytes)
            .expect("MFS extraction should succeed")
            .expect("MFS signature should be detected");

        assert_eq!(image.volume_name, "Test Volume");
        assert_eq!(image.dirs, vec!["Test Volume".to_string()]);
        let file = image
            .files
            .iter()
            .find(|file| file.path == "Test Volume/Test App")
            .expect("synthetic MFS file should be present");
        assert_eq!(file.data, b"hello");
        assert_eq!(file.rsrc, b"PICT");
        assert_eq!(file.file_type, *b"APPL");
        assert_eq!(file.creator, *b"TEST");
        assert_eq!(file.finder_flags, 0x0400);
    }

    #[test]
    fn rejects_mfs_fork_that_ends_before_its_logical_length() {
        let mut bytes = mfs_test_image();
        bytes[2048 + 24..2048 + 28].copy_from_slice(&1025u32.to_be_bytes());
        bytes[2048 + 28..2048 + 32].copy_from_slice(&2048u32.to_be_bytes());

        let error = extract_dc42_or_hfs(&bytes).expect_err("truncated MFS fork should fail");
        assert!(error.contains("ends before its logical length"), "{error}");
    }

    #[test]
    fn rejects_mfs_fork_allocation_cycles() {
        let mut bytes = mfs_test_image();
        bytes[2048 + 24..2048 + 28].copy_from_slice(&2049u32.to_be_bytes());
        bytes[2048 + 28..2048 + 32].copy_from_slice(&3072u32.to_be_bytes());
        bytes[2048 + 32..2048 + 34].copy_from_slice(&0u16.to_be_bytes());
        bytes[2048 + 34..2048 + 38].copy_from_slice(&0u32.to_be_bytes());
        bytes[2048 + 38..2048 + 42].copy_from_slice(&0u32.to_be_bytes());
        set_mfs_abm(&mut bytes, 2, 3);
        set_mfs_abm(&mut bytes, 3, 2);

        let error = extract_dc42_or_hfs(&bytes).expect_err("cyclic MFS fork should fail");
        assert!(error.contains("allocation cycle"), "{error}");
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

    fn mfs_test_image() -> Vec<u8> {
        const IMAGE_SIZE: usize = 24 * 1024;
        const MDB: usize = MFS_MDB_SECTOR * MFS_SECTOR_SIZE;
        const DIRECTORY: usize = 4 * MFS_SECTOR_SIZE;
        const ALLOCATION_START: usize = 16 * MFS_SECTOR_SIZE;

        let mut bytes = vec![0u8; IMAGE_SIZE];
        bytes[MDB..MDB + 2].copy_from_slice(&MFS_SIGNATURE.to_be_bytes());
        bytes[MDB + 12..MDB + 14].copy_from_slice(&1u16.to_be_bytes());
        bytes[MDB + 14..MDB + 16].copy_from_slice(&4u16.to_be_bytes());
        bytes[MDB + 16..MDB + 18].copy_from_slice(&12u16.to_be_bytes());
        bytes[MDB + 18..MDB + 20].copy_from_slice(&8u16.to_be_bytes());
        bytes[MDB + 20..MDB + 24].copy_from_slice(&1024u32.to_be_bytes());
        bytes[MDB + 28..MDB + 30].copy_from_slice(&16u16.to_be_bytes());
        bytes[MDB + 36] = 11;
        bytes[MDB + 37..MDB + 48].copy_from_slice(b"Test Volume");
        set_mfs_abm(&mut bytes, 2, MFS_EOF_BLOCK);
        set_mfs_abm(&mut bytes, 3, MFS_EOF_BLOCK);

        bytes[DIRECTORY] = 0x80;
        bytes[DIRECTORY + 2..DIRECTORY + 6].copy_from_slice(b"APPL");
        bytes[DIRECTORY + 6..DIRECTORY + 10].copy_from_slice(b"TEST");
        bytes[DIRECTORY + 10..DIRECTORY + 12].copy_from_slice(&0x0400u16.to_be_bytes());
        bytes[DIRECTORY + 22..DIRECTORY + 24].copy_from_slice(&2u16.to_be_bytes());
        bytes[DIRECTORY + 24..DIRECTORY + 28].copy_from_slice(&5u32.to_be_bytes());
        bytes[DIRECTORY + 28..DIRECTORY + 32].copy_from_slice(&1024u32.to_be_bytes());
        bytes[DIRECTORY + 32..DIRECTORY + 34].copy_from_slice(&3u16.to_be_bytes());
        bytes[DIRECTORY + 34..DIRECTORY + 38].copy_from_slice(&4u32.to_be_bytes());
        bytes[DIRECTORY + 38..DIRECTORY + 42].copy_from_slice(&1024u32.to_be_bytes());
        bytes[DIRECTORY + 50] = 8;
        bytes[DIRECTORY + 51..DIRECTORY + 59].copy_from_slice(b"Test App");
        bytes[ALLOCATION_START..ALLOCATION_START + 5].copy_from_slice(b"hello");
        bytes[ALLOCATION_START + 1024..ALLOCATION_START + 1028].copy_from_slice(b"PICT");
        bytes
    }

    fn set_mfs_abm(bytes: &mut [u8], block: u16, value: u16) {
        let index = usize::from(block - 2);
        let offset = MFS_MDB_SECTOR * MFS_SECTOR_SIZE + MFS_MDB_VOLUME_INFO_SIZE + (index / 2) * 3;
        if index % 2 == 0 {
            bytes[offset] = (value >> 4) as u8;
            bytes[offset + 1] = (bytes[offset + 1] & 0x0F) | ((value as u8 & 0x0F) << 4);
        } else {
            bytes[offset + 1] = (bytes[offset + 1] & 0xF0) | ((value >> 8) as u8 & 0x0F);
            bytes[offset + 2] = value as u8;
        }
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
