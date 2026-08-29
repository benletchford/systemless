//! Minimal classic HFS reader used by Systemless.
//!
//! This is an independent implementation based on published classic HFS
//! on-disk format documentation and Systemless's own behavioural requirements.
//! It is not derived from the source of the former `hfs-reader` dependency.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

use crate::mac_roman::decode_mac_roman;

const MDB_OFFSET: usize = 1024;
const HFS_SIGNATURE: u16 = 0x4244;
const ROOT_PARENT_CNID: u32 = 1;
const ROOT_CNID: u32 = 2;
const CATALOG_FILE_CNID: u32 = 4;
const DATA_FORK: u8 = 0;
const RESOURCE_FORK: u8 = 0xff;
const BT_HEADER_NODE: u8 = 1;
const BT_LEAF_NODE: u8 = 0xff;
const NODE_DESCRIPTOR_SIZE: usize = 14;
const BT_HEADER_RECORD_SIZE: usize = 106;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Extent {
    start_block: u16,
    block_count: u16,
}

type ExtentRecord = [Extent; 3];

#[derive(Clone, Debug)]
struct Fork {
    logical_size: u32,
    extents: ExtentRecord,
}

#[derive(Debug)]
pub(crate) struct HfsFileEntry {
    pub(crate) rel_path: PathBuf,
    pub(crate) file_type: [u8; 4],
    pub(crate) creator: [u8; 4],
    pub(crate) finder_flags: u16,
    cnid: u32,
    data_fork: Fork,
    resource_fork: Fork,
}

#[derive(Debug)]
pub(crate) struct HfsDirEntry {
    pub(crate) rel_path: PathBuf,
}

#[derive(Debug)]
pub(crate) struct HfsVolume<'a> {
    bytes: &'a [u8],
    allocation_block_size: u32,
    allocation_block_count: u16,
    allocation_start: u16,
    overflow: HashMap<ExtentKey, ExtentRecord>,
    pub(crate) volume_name: String,
    pub(crate) files: Vec<HfsFileEntry>,
    pub(crate) dirs: Vec<HfsDirEntry>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ExtentKey {
    fork_type: u8,
    file_cnid: u32,
    start_block: u16,
}

#[derive(Debug)]
struct RawDir {
    parent_cnid: u32,
    name: String,
}

#[derive(Debug)]
struct RawFile {
    parent_cnid: u32,
    name: String,
    cnid: u32,
    file_type: [u8; 4],
    creator: [u8; 4],
    finder_flags: u16,
    data_fork: Fork,
    resource_fork: Fork,
}

#[derive(Clone, Copy, Debug)]
struct VolumeLayout {
    allocation_block_size: u32,
    allocation_block_count: u16,
    allocation_start: u16,
}

impl<'a> HfsVolume<'a> {
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let mdb = bytes
            .get(MDB_OFFSET..MDB_OFFSET + 162)
            .ok_or_else(|| "truncated HFS master directory block".to_string())?;
        if be_u16(mdb, 0)? != HFS_SIGNATURE {
            return Err("invalid classic HFS signature".into());
        }

        let allocation_block_size = be_u32(mdb, 20)?;
        let allocation_block_count = be_u16(mdb, 18)?;
        let allocation_start = be_u16(mdb, 28)?;
        if allocation_block_size == 0 || !allocation_block_size.is_multiple_of(512) {
            return Err(format!(
                "invalid HFS allocation block size {allocation_block_size}"
            ));
        }
        if allocation_block_count == 0 {
            return Err("HFS volume has no allocation blocks".into());
        }
        let layout = VolumeLayout {
            allocation_block_size,
            allocation_block_count,
            allocation_start,
        };
        validate_allocation_area(bytes, layout)?;

        let volume_name = pascal_name(mdb, 36, 27, "volume name")?;
        let extents_fork = Fork {
            logical_size: be_u32(mdb, 130)?,
            extents: parse_extent_record(mdb, 134)?,
        };
        let catalog_fork = Fork {
            logical_size: be_u32(mdb, 146)?,
            extents: parse_extent_record(mdb, 150)?,
        };

        // The extents file must bootstrap from the three extents in the MDB.
        // If those do not contain it, fail explicitly instead of returning
        // incomplete fork data.
        let extents_bytes = read_initial_fork(bytes, layout, &extents_fork)
            .map_err(|error| format!("cannot bootstrap HFS extents overflow file: {error}"))?;
        let overflow = if extents_bytes.is_empty() {
            HashMap::new()
        } else {
            parse_extents_btree(&extents_bytes)?
        };

        let catalog_bytes = read_fork_from_parts(
            bytes,
            layout,
            &overflow,
            CATALOG_FILE_CNID,
            DATA_FORK,
            &catalog_fork,
        )
        .map_err(|error| format!("read HFS catalog file: {error}"))?;
        let (raw_dirs, raw_files) = parse_catalog_btree(&catalog_bytes)?;
        let dir_paths = reconstruct_dir_paths(&raw_dirs)?;

        let mut dirs = raw_dirs
            .keys()
            .filter(|&&cnid| cnid != ROOT_CNID)
            .map(|cnid| {
                Ok(HfsDirEntry {
                    rel_path: dir_paths.get(cnid).cloned().ok_or_else(|| {
                        format!("missing reconstructed path for directory {cnid}")
                    })?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut files = raw_files
            .into_iter()
            .map(|file| {
                let parent = dir_paths.get(&file.parent_cnid).ok_or_else(|| {
                    format!(
                        "file CNID {} references missing parent CNID {}",
                        file.cnid, file.parent_cnid
                    )
                })?;
                let mut rel_path = parent.clone();
                rel_path.push(file.name);
                Ok(HfsFileEntry {
                    rel_path,
                    file_type: file.file_type,
                    creator: file.creator,
                    finder_flags: file.finder_flags,
                    cnid: file.cnid,
                    data_fork: file.data_fork,
                    resource_fork: file.resource_fork,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        dirs.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
        files.sort_by(|left, right| left.rel_path.cmp(&right.rel_path));
        Ok(Self {
            bytes,
            allocation_block_size,
            allocation_block_count,
            allocation_start,
            overflow,
            volume_name,
            files,
            dirs,
        })
    }

    pub(crate) fn read_data_fork(&self, file: &HfsFileEntry) -> Result<Vec<u8>, String> {
        self.read_fork(file.cnid, DATA_FORK, &file.data_fork)
    }

    pub(crate) fn read_rsrc_fork(&self, file: &HfsFileEntry) -> Result<Vec<u8>, String> {
        self.read_fork(file.cnid, RESOURCE_FORK, &file.resource_fork)
    }

    fn read_fork(&self, cnid: u32, fork_type: u8, fork: &Fork) -> Result<Vec<u8>, String> {
        read_fork_from_parts(
            self.bytes,
            VolumeLayout {
                allocation_block_size: self.allocation_block_size,
                allocation_block_count: self.allocation_block_count,
                allocation_start: self.allocation_start,
            },
            &self.overflow,
            cnid,
            fork_type,
            fork,
        )
    }
}

fn validate_allocation_area(bytes: &[u8], layout: VolumeLayout) -> Result<(), String> {
    let start = usize::from(layout.allocation_start)
        .checked_mul(512)
        .ok_or_else(|| "HFS allocation area offset overflow".to_string())?;
    let length = usize::from(layout.allocation_block_count)
        .checked_mul(
            usize::try_from(layout.allocation_block_size)
                .map_err(|_| "HFS allocation block size is too large".to_string())?,
        )
        .ok_or_else(|| "HFS allocation area length overflow".to_string())?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| "HFS allocation area end overflow".to_string())?;
    if end > bytes.len() {
        return Err(format!(
            "HFS allocation area ends at byte {end}, beyond volume length {}",
            bytes.len()
        ));
    }
    Ok(())
}

fn read_initial_fork(bytes: &[u8], layout: VolumeLayout, fork: &Fork) -> Result<Vec<u8>, String> {
    let logical_size = usize::try_from(fork.logical_size)
        .map_err(|_| "HFS fork logical size is too large".to_string())?;
    let mut output = Vec::with_capacity(logical_size);
    append_extent_record(bytes, layout, &fork.extents, logical_size, &mut output)?;
    if output.len() != logical_size {
        return Err(format!(
            "fork requires overflow extents (read {} of {logical_size} bytes)",
            output.len()
        ));
    }
    Ok(output)
}

fn read_fork_from_parts(
    bytes: &[u8],
    layout: VolumeLayout,
    overflow: &HashMap<ExtentKey, ExtentRecord>,
    file_cnid: u32,
    fork_type: u8,
    fork: &Fork,
) -> Result<Vec<u8>, String> {
    let logical_size = usize::try_from(fork.logical_size)
        .map_err(|_| "HFS fork logical size is too large".to_string())?;
    if logical_size == 0 {
        return Ok(Vec::new());
    }

    let mut output = Vec::with_capacity(logical_size);
    let mut start_block = extent_block_count(&fork.extents)?;
    append_extent_record(bytes, layout, &fork.extents, logical_size, &mut output)?;
    let mut seen = HashSet::new();
    while output.len() < logical_size {
        let key = ExtentKey {
            fork_type,
            file_cnid,
            start_block,
        };
        if !seen.insert(key) {
            return Err(format!(
                "cycle in overflow extents for file CNID {file_cnid}"
            ));
        }
        let record = overflow.get(&key).ok_or_else(|| {
            format!("missing overflow extent for file CNID {file_cnid} at fork block {start_block}")
        })?;
        let added_blocks = extent_block_count(record)?;
        if added_blocks == 0 {
            return Err(format!(
                "empty overflow extent for file CNID {file_cnid} at fork block {start_block}"
            ));
        }
        append_extent_record(bytes, layout, record, logical_size, &mut output)?;
        start_block = start_block
            .checked_add(added_blocks)
            .ok_or_else(|| "HFS fork block count overflow".to_string())?;
    }
    Ok(output)
}

fn append_extent_record(
    bytes: &[u8],
    layout: VolumeLayout,
    extents: &ExtentRecord,
    logical_size: usize,
    output: &mut Vec<u8>,
) -> Result<(), String> {
    for extent in extents {
        if extent.block_count == 0 || output.len() == logical_size {
            break;
        }
        let extent_end = extent
            .start_block
            .checked_add(extent.block_count)
            .ok_or_else(|| "HFS extent block range overflow".to_string())?;
        if extent_end > layout.allocation_block_count {
            return Err(format!(
                "HFS extent blocks {}..{} exceed allocation block count {}",
                extent.start_block, extent_end, layout.allocation_block_count
            ));
        }

        let allocation_start = usize::from(layout.allocation_start)
            .checked_mul(512)
            .ok_or_else(|| "HFS allocation area offset overflow".to_string())?;
        let block_size = usize::try_from(layout.allocation_block_size)
            .map_err(|_| "HFS allocation block size is too large".to_string())?;
        let start = usize::from(extent.start_block)
            .checked_mul(block_size)
            .and_then(|offset| allocation_start.checked_add(offset))
            .ok_or_else(|| "HFS extent offset overflow".to_string())?;
        let extent_len = usize::from(extent.block_count)
            .checked_mul(block_size)
            .ok_or_else(|| "HFS extent length overflow".to_string())?;
        let end = start
            .checked_add(extent_len)
            .ok_or_else(|| "HFS extent end overflow".to_string())?;
        let extent_bytes = bytes.get(start..end).ok_or_else(|| {
            format!(
                "HFS extent byte range {start}..{end} exceeds volume length {}",
                bytes.len()
            )
        })?;
        let remaining = logical_size - output.len();
        output.extend_from_slice(&extent_bytes[..extent_bytes.len().min(remaining)]);
    }
    Ok(())
}

fn extent_block_count(extents: &ExtentRecord) -> Result<u16, String> {
    let mut total = 0u16;
    let mut terminated = false;
    for extent in extents {
        if extent.block_count == 0 {
            terminated = true;
        } else {
            if terminated {
                return Err("non-empty HFS extent follows an empty extent".into());
            }
            total = total
                .checked_add(extent.block_count)
                .ok_or_else(|| "HFS extent block count overflow".to_string())?;
        }
    }
    Ok(total)
}

fn parse_extents_btree(bytes: &[u8]) -> Result<HashMap<ExtentKey, ExtentRecord>, String> {
    let mut overflow = HashMap::new();
    for record in btree_leaf_records(bytes, "extents overflow")? {
        let key_length = usize::from(
            *record
                .first()
                .ok_or_else(|| "empty record in HFS extents overflow B-tree".to_string())?,
        );
        if key_length < 7 {
            return Err(format!(
                "short HFS extent key length {key_length} (expected at least 7)"
            ));
        }
        let data_offset = align_even(
            1usize
                .checked_add(key_length)
                .ok_or_else(|| "HFS extent key length overflow".to_string())?,
        )?;
        let key = ExtentKey {
            fork_type: byte(record, 1)?,
            file_cnid: be_u32(record, 2)?,
            start_block: be_u16(record, 6)?,
        };
        let extents = parse_extent_record(record, data_offset)?;
        if overflow.insert(key, extents).is_some() {
            return Err(format!(
                "duplicate HFS overflow extent key for file CNID {}",
                key.file_cnid
            ));
        }
    }
    Ok(overflow)
}

fn parse_catalog_btree(bytes: &[u8]) -> Result<(HashMap<u32, RawDir>, Vec<RawFile>), String> {
    let mut dirs = HashMap::new();
    let mut files = Vec::new();
    let mut file_cnids = HashSet::new();
    for record in btree_leaf_records(bytes, "catalog")? {
        let key_length = usize::from(
            *record
                .first()
                .ok_or_else(|| "empty record in HFS catalog B-tree".to_string())?,
        );
        if key_length < 6 {
            return Err(format!(
                "short HFS catalog key length {key_length} (expected at least 6)"
            ));
        }
        let name_length = usize::from(byte(record, 6)?);
        if name_length > 31 || name_length > key_length - 6 {
            return Err(format!("invalid HFS catalog name length {name_length}"));
        }
        let name_bytes = record
            .get(7..7 + name_length)
            .ok_or_else(|| "truncated HFS catalog name".to_string())?;
        let name = decode_mac_roman(name_bytes);
        let parent_cnid = be_u32(record, 2)?;
        let data_offset = align_even(
            1usize
                .checked_add(key_length)
                .ok_or_else(|| "HFS catalog key length overflow".to_string())?,
        )?;
        let data = record
            .get(data_offset..)
            .ok_or_else(|| "truncated HFS catalog record".to_string())?;
        let record_type = byte(data, 0)?;
        match record_type {
            1 => {
                let cnid = be_u32(data, 6)?;
                if dirs.insert(cnid, RawDir { parent_cnid, name }).is_some() {
                    return Err(format!("duplicate HFS directory CNID {cnid}"));
                }
            }
            2 => {
                let cnid = be_u32(data, 20)?;
                if !file_cnids.insert(cnid) {
                    return Err(format!("duplicate HFS file CNID {cnid}"));
                }
                files.push(RawFile {
                    parent_cnid,
                    name,
                    cnid,
                    file_type: array_4(data, 4)?,
                    creator: array_4(data, 8)?,
                    // Finder Interface, Inside Macintosh: Macintosh Toolbox
                    // Essentials, pp. 7-32 and 7-47: fdFlags follows the
                    // four-byte type and creator fields in FInfo.
                    finder_flags: be_u16(data, 12)?,
                    data_fork: Fork {
                        logical_size: be_u32(data, 26)?,
                        extents: parse_extent_record(data, 74)?,
                    },
                    resource_fork: Fork {
                        logical_size: be_u32(data, 36)?,
                        extents: parse_extent_record(data, 86)?,
                    },
                });
            }
            // Thread records carry reverse lookup information that is not
            // needed when enumerating all file and directory records.
            3 | 4 => {}
            other => return Err(format!("unknown HFS catalog record type {other}")),
        }
    }
    if !dirs.contains_key(&ROOT_CNID) {
        return Err("HFS catalog is missing the root directory record".into());
    }
    Ok((dirs, files))
}

fn reconstruct_dir_paths(dirs: &HashMap<u32, RawDir>) -> Result<HashMap<u32, PathBuf>, String> {
    let root = dirs
        .get(&ROOT_CNID)
        .ok_or_else(|| "HFS catalog is missing the root directory record".to_string())?;
    if root.parent_cnid != ROOT_PARENT_CNID {
        return Err(format!(
            "HFS root directory references parent CNID {} instead of {ROOT_PARENT_CNID}",
            root.parent_cnid
        ));
    }

    let mut paths = HashMap::new();
    paths.insert(ROOT_CNID, PathBuf::new());
    for &cnid in dirs.keys() {
        if cnid == ROOT_CNID || paths.contains_key(&cnid) {
            continue;
        }
        let mut chain = Vec::new();
        let mut current = cnid;
        let mut visiting = HashSet::new();
        while !paths.contains_key(&current) {
            if !visiting.insert(current) {
                return Err(format!("cycle in HFS directory parents at CNID {current}"));
            }
            let dir = dirs.get(&current).ok_or_else(|| {
                format!("HFS directory CNID {cnid} references missing parent CNID {current}")
            })?;
            chain.push((current, dir.name.clone()));
            current = dir.parent_cnid;
        }

        let mut path = paths
            .get(&current)
            .cloned()
            .ok_or_else(|| "internal HFS path reconstruction error".to_string())?;
        while let Some((child_cnid, name)) = chain.pop() {
            path.push(name);
            paths.insert(child_cnid, path.clone());
        }
    }
    Ok(paths)
}

fn btree_leaf_records<'a>(bytes: &'a [u8], name: &str) -> Result<Vec<&'a [u8]>, String> {
    let header_node = bytes
        .get(..NODE_DESCRIPTOR_SIZE + BT_HEADER_RECORD_SIZE)
        .ok_or_else(|| format!("truncated HFS {name} B-tree header node"))?;
    if byte(header_node, 8)? != BT_HEADER_NODE {
        return Err(format!("invalid HFS {name} B-tree header node kind"));
    }
    let node_size = usize::from(be_u16(header_node, NODE_DESCRIPTOR_SIZE + 18)?);
    if !(512..=32768).contains(&node_size) || !node_size.is_power_of_two() {
        return Err(format!("invalid HFS {name} B-tree node size {node_size}"));
    }
    if bytes.len() < node_size || !bytes.len().is_multiple_of(node_size) {
        return Err(format!(
            "HFS {name} B-tree length {} is not a whole number of {node_size}-byte nodes",
            bytes.len()
        ));
    }

    let total_nodes = bytes.len() / node_size;
    let advertised_nodes = usize::try_from(be_u32(header_node, NODE_DESCRIPTOR_SIZE + 22)?)
        .map_err(|_| format!("HFS {name} B-tree node count is too large"))?;
    if advertised_nodes != total_nodes {
        return Err(format!(
            "HFS {name} B-tree header declares {advertised_nodes} nodes, but the file contains {total_nodes}"
        ));
    }
    let advertised_records = usize::try_from(be_u32(header_node, NODE_DESCRIPTOR_SIZE + 6)?)
        .map_err(|_| format!("HFS {name} B-tree record count is too large"))?;
    let mut node_number = usize::try_from(be_u32(header_node, NODE_DESCRIPTOR_SIZE + 10)?)
        .map_err(|_| format!("HFS {name} first leaf node is too large"))?;
    let mut seen = HashSet::new();
    let mut records = Vec::new();
    while node_number != 0 {
        if node_number >= total_nodes {
            return Err(format!(
                "HFS {name} leaf node {node_number} exceeds node count {total_nodes}"
            ));
        }
        if !seen.insert(node_number) {
            return Err(format!("cycle in HFS {name} B-tree leaf chain"));
        }
        let start = node_number
            .checked_mul(node_size)
            .ok_or_else(|| format!("HFS {name} node offset overflow"))?;
        let node = bytes
            .get(start..start + node_size)
            .ok_or_else(|| format!("truncated HFS {name} leaf node"))?;
        if byte(node, 8)? != BT_LEAF_NODE {
            return Err(format!(
                "HFS {name} node {node_number} in leaf chain is not a leaf"
            ));
        }
        let record_count = usize::from(be_u16(node, 10)?);
        let table_size = record_count
            .checked_add(1)
            .and_then(|count| count.checked_mul(2))
            .ok_or_else(|| format!("HFS {name} record table size overflow"))?;
        if NODE_DESCRIPTOR_SIZE + table_size > node_size {
            return Err(format!("HFS {name} leaf record table is too large"));
        }
        for index in 0..record_count {
            let record_start = node_record_offset(node, node_size, index)?;
            let record_end = node_record_offset(node, node_size, index + 1)?;
            if record_start < NODE_DESCRIPTOR_SIZE
                || record_start > record_end
                || record_end > node_size - table_size
            {
                return Err(format!(
                    "invalid HFS {name} leaf record bounds {record_start}..{record_end}"
                ));
            }
            records.push(&node[record_start..record_end]);
        }
        node_number = usize::try_from(be_u32(node, 0)?)
            .map_err(|_| format!("HFS {name} next leaf node is too large"))?;
    }
    if records.len() != advertised_records {
        return Err(format!(
            "HFS {name} B-tree header declares {advertised_records} leaf records, but the leaf chain contains {}",
            records.len()
        ));
    }
    Ok(records)
}

fn node_record_offset(node: &[u8], node_size: usize, index: usize) -> Result<usize, String> {
    let entry_bytes = index
        .checked_add(1)
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| "HFS B-tree record offset index overflow".to_string())?;
    let offset = node_size
        .checked_sub(entry_bytes)
        .ok_or_else(|| "HFS B-tree record offset underflow".to_string())?;
    Ok(usize::from(be_u16(node, offset)?))
}

fn parse_extent_record(bytes: &[u8], offset: usize) -> Result<ExtentRecord, String> {
    Ok([
        parse_extent(bytes, offset)?,
        parse_extent(bytes, offset + 4)?,
        parse_extent(bytes, offset + 8)?,
    ])
}

fn parse_extent(bytes: &[u8], offset: usize) -> Result<Extent, String> {
    Ok(Extent {
        start_block: be_u16(bytes, offset)?,
        block_count: be_u16(bytes, offset + 2)?,
    })
}

fn pascal_name(bytes: &[u8], offset: usize, max: usize, field: &str) -> Result<String, String> {
    let length = usize::from(byte(bytes, offset)?);
    if length == 0 || length > max {
        return Err(format!("invalid HFS {field} length {length}"));
    }
    let value = bytes
        .get(offset + 1..offset + 1 + length)
        .ok_or_else(|| format!("truncated HFS {field}"))?;
    Ok(decode_mac_roman(value))
}

fn align_even(value: usize) -> Result<usize, String> {
    value
        .checked_add(1)
        .map(|value| value & !1)
        .ok_or_else(|| "HFS record alignment overflow".to_string())
}

fn byte(bytes: &[u8], offset: usize) -> Result<u8, String> {
    bytes
        .get(offset)
        .copied()
        .ok_or_else(|| format!("truncated HFS structure at byte {offset}"))
}

fn be_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| format!("truncated HFS structure at byte {offset}"))?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| format!("truncated HFS structure at byte {offset}"))?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn array_4(bytes: &[u8], offset: usize) -> Result<[u8; 4], String> {
    bytes
        .get(offset..offset + 4)
        .and_then(|value| value.try_into().ok())
        .ok_or_else(|| format!("truncated HFS structure at byte {offset}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK_SIZE: usize = 512;
    const ALLOCATION_START_BLOCK: usize = 3;
    const ALLOCATION_START: usize = ALLOCATION_START_BLOCK * BLOCK_SIZE;

    #[test]
    fn parses_catalog_paths_metadata_and_both_forks() {
        let records = vec![
            dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID),
            // Deliberately place the child before its parent to verify that
            // path reconstruction is independent of catalog leaf order.
            dir_record(10, b"Caf\x8e", 11),
            dir_record(ROOT_CNID, b"Games", 10),
            file_record(
                11,
                b"Demo",
                20,
                *b"APPL",
                *b"TEST",
                0x4000,
                5,
                [extent(2, 1), Extent::default(), Extent::default()],
                4,
                [extent(3, 1), Extent::default(), Extent::default()],
            ),
        ];
        let mut bytes = volume_fixture(records, Vec::new(), 8);
        write_allocation_block(&mut bytes, 2, b"hello and ignored allocation padding");
        write_allocation_block(&mut bytes, 3, b"rsrc and ignored allocation padding");

        let volume = HfsVolume::parse(&bytes).expect("synthetic HFS volume should parse");

        assert_eq!(volume.volume_name, "Test");
        assert_eq!(volume.dirs.len(), 2);
        assert!(volume
            .dirs
            .iter()
            .any(|dir| dir.rel_path == PathBuf::from("Games")));
        assert!(volume
            .dirs
            .iter()
            .any(|dir| dir.rel_path == PathBuf::from("Games/Café")));
        let file = volume.files.first().expect("file should be enumerated");
        assert_eq!(file.rel_path, PathBuf::from("Games/Café/Demo"));
        assert_eq!(file.file_type, *b"APPL");
        assert_eq!(file.creator, *b"TEST");
        assert_eq!(file.finder_flags, 0x4000);
        assert_eq!(volume.read_data_fork(file).unwrap(), b"hello");
        assert_eq!(volume.read_rsrc_fork(file).unwrap(), b"rsrc");

        let image = super::super::extract_dc42_or_hfs(&bytes)
            .expect("disk-image extraction should succeed")
            .expect("classic HFS signature should be detected");
        assert_eq!(image.volume_name, "Test");
        assert!(image.dirs.contains(&"Test/Games/Café".to_string()));
        let extracted = image
            .files
            .iter()
            .find(|entry| entry.path == "Test/Games/Café/Demo")
            .expect("classic HFS file should enter the disk-image model");
        assert_eq!(extracted.data, b"hello");
        assert_eq!(extracted.rsrc, b"rsrc");
        assert_eq!(extracted.file_type, *b"APPL");
        assert_eq!(extracted.creator, *b"TEST");
        // Inside Macintosh: Macintosh Toolbox Essentials, p. 7-47 defines
        // isInvisible as Finder flag bit 14 (0x4000).
        assert_eq!(extracted.finder_flags, 0x4000);
    }

    #[test]
    fn rejects_invalid_or_truncated_mdb() {
        assert_eq!(
            HfsVolume::parse(&vec![0; MDB_OFFSET + 10])
                .unwrap_err()
                .to_string(),
            "truncated HFS master directory block"
        );

        let mut bytes = volume_fixture(
            vec![dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID)],
            Vec::new(),
            4,
        );
        bytes[MDB_OFFSET..MDB_OFFSET + 2].copy_from_slice(&0u16.to_be_bytes());
        assert_eq!(
            HfsVolume::parse(&bytes).unwrap_err(),
            "invalid classic HFS signature"
        );
    }

    #[test]
    fn rejects_corrupt_fork_extent_without_panicking() {
        let records = vec![
            dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID),
            file_record(
                ROOT_CNID,
                b"Bad",
                20,
                *b"TEXT",
                *b"ttxt",
                0,
                1,
                [extent(7, 2), Extent::default(), Extent::default()],
                0,
                [Extent::default(); 3],
            ),
        ];
        let bytes = volume_fixture(records, Vec::new(), 8);
        let volume = HfsVolume::parse(&bytes).expect("catalog should parse");

        let error = volume.read_data_fork(&volume.files[0]).unwrap_err();
        assert!(error.contains("exceed allocation block count 8"), "{error}");
    }

    #[test]
    fn rejects_missing_and_cyclic_directory_parents() {
        let missing = volume_fixture(
            vec![
                dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID),
                dir_record(99, b"Lost", 10),
            ],
            Vec::new(),
            4,
        );
        let error = HfsVolume::parse(&missing).unwrap_err();
        assert!(error.contains("missing parent CNID 99"), "{error}");

        let cyclic = volume_fixture(
            vec![
                dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID),
                dir_record(11, b"A", 10),
                dir_record(10, b"B", 11),
            ],
            Vec::new(),
            4,
        );
        let error = HfsVolume::parse(&cyclic).unwrap_err();
        assert!(error.contains("cycle in HFS directory parents"), "{error}");
    }

    #[test]
    fn rejects_duplicate_directory_cnids() {
        let bytes = volume_fixture(
            vec![
                dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID),
                dir_record(ROOT_CNID, b"One", 10),
                dir_record(ROOT_CNID, b"Two", 10),
            ],
            Vec::new(),
            4,
        );

        let error = HfsVolume::parse(&bytes).unwrap_err();
        assert!(error.contains("duplicate HFS directory CNID 10"), "{error}");
    }

    #[test]
    fn reads_fork_through_extents_overflow_btree() {
        let records = vec![
            dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID),
            file_record(
                ROOT_CNID,
                b"Split",
                20,
                *b"DATA",
                *b"TEST",
                0,
                3 * BLOCK_SIZE as u32 + 4,
                [extent(4, 1), extent(5, 1), extent(6, 1)],
                0,
                [Extent::default(); 3],
            ),
        ];
        let overflow = vec![extent_record(
            DATA_FORK,
            20,
            3,
            [extent(7, 1), Extent::default(), Extent::default()],
        )];
        let mut bytes = volume_fixture(records, overflow, 10);
        for (block, fill) in [(4, b'A'), (5, b'B'), (6, b'C')] {
            let data = vec![fill; BLOCK_SIZE];
            write_allocation_block(&mut bytes, block, &data);
        }
        write_allocation_block(&mut bytes, 7, b"tail");

        let volume = HfsVolume::parse(&bytes).expect("overflow volume should parse");
        let data = volume.read_data_fork(&volume.files[0]).unwrap();

        assert_eq!(data.len(), 3 * BLOCK_SIZE + 4);
        assert!(data[..BLOCK_SIZE].iter().all(|&byte| byte == b'A'));
        assert!(data[BLOCK_SIZE..2 * BLOCK_SIZE]
            .iter()
            .all(|&byte| byte == b'B'));
        assert!(data[2 * BLOCK_SIZE..3 * BLOCK_SIZE]
            .iter()
            .all(|&byte| byte == b'C'));
        assert_eq!(&data[3 * BLOCK_SIZE..], b"tail");
    }

    #[test]
    fn reports_unbootstrappable_extents_file_explicitly() {
        let mut bytes = volume_fixture(
            vec![dir_record(ROOT_PARENT_CNID, b"Test", ROOT_CNID)],
            Vec::new(),
            6,
        );
        put_u32(&mut bytes, MDB_OFFSET + 130, (2 * BLOCK_SIZE) as u32);
        put_extent(&mut bytes, MDB_OFFSET + 134, extent(2, 1));

        let error = HfsVolume::parse(&bytes).unwrap_err();
        assert!(
            error.contains("cannot bootstrap HFS extents overflow file")
                && error.contains("requires overflow extents"),
            "{error}"
        );
    }

    fn volume_fixture(
        catalog_records: Vec<Vec<u8>>,
        overflow_records: Vec<Vec<u8>>,
        allocation_block_count: u16,
    ) -> Vec<u8> {
        let mut bytes =
            vec![0; ALLOCATION_START + usize::from(allocation_block_count) * BLOCK_SIZE];
        put_u16(&mut bytes, MDB_OFFSET, HFS_SIGNATURE);
        put_u16(&mut bytes, MDB_OFFSET + 18, allocation_block_count);
        put_u32(&mut bytes, MDB_OFFSET + 20, BLOCK_SIZE as u32);
        put_u16(&mut bytes, MDB_OFFSET + 28, ALLOCATION_START_BLOCK as u16);
        bytes[MDB_OFFSET + 36] = 4;
        bytes[MDB_OFFSET + 37..MDB_OFFSET + 41].copy_from_slice(b"Test");

        let catalog_start = if overflow_records.is_empty() { 0 } else { 2 };
        put_u32(&mut bytes, MDB_OFFSET + 146, (2 * BLOCK_SIZE) as u32);
        put_extent(&mut bytes, MDB_OFFSET + 150, extent(catalog_start, 2));
        write_btree(
            &mut bytes[ALLOCATION_START + catalog_start as usize * BLOCK_SIZE..],
            catalog_records,
        );

        if !overflow_records.is_empty() {
            put_u32(&mut bytes, MDB_OFFSET + 130, (2 * BLOCK_SIZE) as u32);
            put_extent(&mut bytes, MDB_OFFSET + 134, extent(0, 2));
            write_btree(&mut bytes[ALLOCATION_START..], overflow_records);
        }
        bytes
    }

    fn write_btree(bytes: &mut [u8], records: Vec<Vec<u8>>) {
        let tree = bytes
            .get_mut(..2 * BLOCK_SIZE)
            .expect("fixture B-tree allocation");
        tree[8] = BT_HEADER_NODE;
        put_u16(tree, 10, 3);
        put_u32(tree, NODE_DESCRIPTOR_SIZE + 2, 1);
        put_u32(tree, NODE_DESCRIPTOR_SIZE + 6, records.len() as u32);
        put_u32(tree, NODE_DESCRIPTOR_SIZE + 10, 1);
        put_u32(tree, NODE_DESCRIPTOR_SIZE + 14, 1);
        put_u16(tree, NODE_DESCRIPTOR_SIZE + 18, BLOCK_SIZE as u16);
        put_u32(tree, NODE_DESCRIPTOR_SIZE + 22, 2);

        let leaf = &mut tree[BLOCK_SIZE..2 * BLOCK_SIZE];
        leaf[8] = BT_LEAF_NODE;
        leaf[9] = 1;
        put_u16(leaf, 10, records.len() as u16);
        let mut cursor = NODE_DESCRIPTOR_SIZE;
        for (index, record) in records.iter().enumerate() {
            let end = cursor + record.len();
            assert!(end <= BLOCK_SIZE - (records.len() + 1) * 2);
            leaf[cursor..end].copy_from_slice(record);
            put_u16(
                leaf,
                BLOCK_SIZE - (index + 1) * 2,
                cursor.try_into().unwrap(),
            );
            cursor = end;
        }
        put_u16(
            leaf,
            BLOCK_SIZE - (records.len() + 1) * 2,
            cursor.try_into().unwrap(),
        );
    }

    fn dir_record(parent_cnid: u32, name: &[u8], cnid: u32) -> Vec<u8> {
        let mut record = catalog_key(parent_cnid, name, 70);
        let data = catalog_data_mut(&mut record, name);
        data[0] = 1;
        put_u32(data, 6, cnid);
        record
    }

    #[allow(clippy::too_many_arguments)]
    fn file_record(
        parent_cnid: u32,
        name: &[u8],
        cnid: u32,
        file_type: [u8; 4],
        creator: [u8; 4],
        finder_flags: u16,
        data_size: u32,
        data_extents: ExtentRecord,
        resource_size: u32,
        resource_extents: ExtentRecord,
    ) -> Vec<u8> {
        let mut record = catalog_key(parent_cnid, name, 102);
        let data = catalog_data_mut(&mut record, name);
        data[0] = 2;
        data[4..8].copy_from_slice(&file_type);
        data[8..12].copy_from_slice(&creator);
        put_u16(data, 12, finder_flags);
        put_u32(data, 20, cnid);
        put_u32(data, 26, data_size);
        put_u32(data, 36, resource_size);
        put_extent_record(data, 74, data_extents);
        put_extent_record(data, 86, resource_extents);
        record
    }

    fn extent_record(
        fork_type: u8,
        file_cnid: u32,
        start_block: u16,
        extents: ExtentRecord,
    ) -> Vec<u8> {
        let mut record = vec![0; 20];
        record[0] = 7;
        record[1] = fork_type;
        put_u32(&mut record, 2, file_cnid);
        put_u16(&mut record, 6, start_block);
        put_extent_record(&mut record, 8, extents);
        record
    }

    fn catalog_key(parent_cnid: u32, name: &[u8], data_size: usize) -> Vec<u8> {
        assert!(name.len() <= 31);
        let key_length = 6 + name.len();
        let data_offset = (1 + key_length + 1) & !1;
        let mut record = vec![0; data_offset + data_size];
        record[0] = key_length as u8;
        put_u32(&mut record, 2, parent_cnid);
        record[6] = name.len() as u8;
        record[7..7 + name.len()].copy_from_slice(name);
        record
    }

    fn catalog_data_mut<'a>(record: &'a mut [u8], name: &[u8]) -> &'a mut [u8] {
        let data_offset = (1 + 6 + name.len() + 1) & !1;
        &mut record[data_offset..]
    }

    fn write_allocation_block(bytes: &mut [u8], block: u16, data: &[u8]) {
        assert!(data.len() <= BLOCK_SIZE);
        let start = ALLOCATION_START + usize::from(block) * BLOCK_SIZE;
        bytes[start..start + data.len()].copy_from_slice(data);
    }

    const fn extent(start_block: u16, block_count: u16) -> Extent {
        Extent {
            start_block,
            block_count,
        }
    }

    fn put_extent_record(bytes: &mut [u8], offset: usize, extents: ExtentRecord) {
        for (index, value) in extents.into_iter().enumerate() {
            put_extent(bytes, offset + index * 4, value);
        }
    }

    fn put_extent(bytes: &mut [u8], offset: usize, value: Extent) {
        put_u16(bytes, offset, value.start_block);
        put_u16(bytes, offset + 2, value.block_count);
    }

    fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
}
