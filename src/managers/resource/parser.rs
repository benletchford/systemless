//! Resource fork parser
//!
//! Parses Macintosh resource fork format to extract CODE resources and others.
//!
//! Resource Fork Structure:
//! - Header (16 bytes): data offset, map offset, data length, map length
//! - Resource Data: length-prefixed data blocks
//! - Resource Map: type list, reference lists, names
//!
//! Reference: Inside Macintosh Volume I, I-126

use std::collections::{BTreeMap, HashMap};

/// Four-character resource type code
pub type ResourceType = [u8; 4];

/// Resource type for CODE segments
pub const CODE_TYPE: ResourceType = *b"CODE";

/// A single resource
#[derive(Debug, Clone)]
pub struct Resource {
    /// Resource type (e.g., 'CODE', 'DLOG', 'ICON')
    pub res_type: ResourceType,
    /// Resource ID
    pub id: i16,
    /// Offset of this resource's reference record from the start of the
    /// resource map.
    pub reference_offset: usize,
    /// Resource name (optional)
    pub name: Option<String>,
    /// Original resource-name bytes before host string conversion.
    pub name_bytes: Option<Vec<u8>>,
    /// Resource data
    pub data: Vec<u8>,
    /// Original compressed on-disk payload when decoding changed the data.
    pub raw_data: Option<Vec<u8>>,
    /// Original attributes paired with `raw_data`.
    pub raw_attrs: Option<u8>,
    /// Resource attributes
    pub attrs: u8,
}

/// Parsed resource fork
#[derive(Debug, Default, Clone)]
pub struct ResourceFork {
    /// All resources indexed by (type, id)
    resources: HashMap<(ResourceType, i16), Resource>,
    pub map_attrs: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceForkEntry {
    pub res_type: ResourceType,
    pub id: i16,
    pub name: Vec<u8>,
    pub data: Vec<u8>,
    pub attrs: u8,
}

impl ResourceFork {
    /// Get all resources map
    pub fn resources(&self) -> &HashMap<(ResourceType, i16), Resource> {
        &self.resources
    }

    pub fn map_attrs(&self) -> u16 {
        self.map_attrs
    }

    #[cfg(test)]
    pub(crate) fn from_test_resources(resources: Vec<(ResourceType, i16, Vec<u8>)>) -> Self {
        Self {
            map_attrs: 0,
            resources: resources
                .into_iter()
                .map(|(res_type, id, data)| {
                    (
                        (res_type, id),
                        Resource {
                            res_type,
                            id,
                            reference_offset: 0,
                            name: None,
                            name_bytes: None,
                            data,
                            raw_data: None,
                            raw_attrs: None,
                            attrs: 0,
                        },
                    )
                })
                .collect(),
        }
    }
}

impl ResourceFork {
    /// Return true when the bytes have a structurally valid resource-fork
    /// header and map. This is intentionally cheaper than `parse`: callers
    /// that only need to classify a fork can avoid copying every resource.
    pub fn has_valid_layout(data: &[u8]) -> bool {
        resource_fork_layout(data).is_some()
    }

    /// Return true when the raw resource fork map contains a CODE resource
    /// with the requested ID. This avoids parsing and copying every resource
    /// when callers only need executable detection.
    pub fn contains_code(data: &[u8], id: i16) -> bool {
        Self::contains_resource(data, CODE_TYPE, id)
    }

    /// Return true when the raw resource fork map contains a resource with
    /// the requested type and ID, and the referenced data block is in bounds.
    pub fn contains_resource(data: &[u8], target_type: ResourceType, target_id: i16) -> bool {
        let Some(ResourceForkLayout {
            data_offset, map, ..
        }) = resource_fork_layout(data)
        else {
            return false;
        };

        let type_list_offset = u16::from_be_bytes([map[24], map[25]]) as usize;
        let num_types = u16::from_be_bytes([map[28], map[29]]) as usize + 1;
        if type_list_offset >= map.len() {
            return false;
        }

        for i in 0..num_types {
            let Some(entry_offset) = (2usize).checked_add(i.saturating_mul(8)) else {
                return false;
            };
            let Some(entry_end) = entry_offset.checked_add(8) else {
                return false;
            };
            let map_entry_offset = type_list_offset + entry_offset;
            let map_entry_end = type_list_offset + entry_end;
            if map_entry_end > map.len() {
                return false;
            }

            let res_type: ResourceType = [
                map[map_entry_offset],
                map[map_entry_offset + 1],
                map[map_entry_offset + 2],
                map[map_entry_offset + 3],
            ];
            if res_type != target_type {
                continue;
            }

            let num_resources =
                u16::from_be_bytes([map[map_entry_offset + 4], map[map_entry_offset + 5]]) as usize
                    + 1;
            let ref_list_offset =
                u16::from_be_bytes([map[map_entry_offset + 6], map[map_entry_offset + 7]]) as usize;
            let Some(ref_list_start) = type_list_offset.checked_add(ref_list_offset) else {
                return false;
            };
            if ref_list_start >= map.len() {
                return false;
            }

            for j in 0..num_resources {
                let Some(ref_offset) = ref_list_start.checked_add(j.saturating_mul(12)) else {
                    return false;
                };
                let Some(ref_end) = ref_offset.checked_add(12) else {
                    return false;
                };
                if ref_end > map.len() {
                    return false;
                }

                let id = i16::from_be_bytes([map[ref_offset], map[ref_offset + 1]]);
                if id != target_id {
                    continue;
                }

                let res_data_offset = ((map[ref_offset + 5] as usize) << 16)
                    | ((map[ref_offset + 6] as usize) << 8)
                    | (map[ref_offset + 7] as usize);
                return resource_data_block_in_bounds(data, data_offset, res_data_offset);
            }

            return false;
        }

        false
    }

    /// Parse a resource fork from raw data
    pub fn parse(data: &[u8]) -> Option<Self> {
        let ResourceForkLayout {
            data_offset,
            map_offset,
            data_length,
            map_length,
            map,
        } = resource_fork_layout(data)?;

        tracing::debug!(
            "Resource fork: data@0x{:04X} ({}), map@0x{:04X} ({})",
            data_offset,
            data_length,
            map_offset,
            map_length
        );

        // Resource map structure (Inside Macintosh Volume I, I-127)
        // Offset 0-15: Copy of resource fork header (16 bytes)
        // Offset 16-19: Handle to next resource map (4 bytes) - not used in files
        // Offset 20-21: File reference number (2 bytes) - not used in files
        // Offset 22-23: Resource fork attributes (2 bytes)
        // Offset 24-25: Offset from map to type list (2 bytes)
        // Offset 26-27: Offset from map to name list (2 bytes)
        // Offset 28-29: Number of types minus 1 (2 bytes)

        if map.len() < 30 {
            tracing::warn!("Resource map too small");
            return None;
        }

        let type_list_offset = u16::from_be_bytes([map[24], map[25]]) as usize;
        let name_list_offset = u16::from_be_bytes([map[26], map[27]]) as usize;
        let num_types = u16::from_be_bytes([map[28], map[29]]) as usize + 1;

        tracing::debug!(
            "Resource map: {} types, type_list@{}, name_list@{}",
            num_types,
            type_list_offset,
            name_list_offset
        );

        let mut fork = ResourceFork {
            map_attrs: u16::from_be_bytes([map[22], map[23]]),
            ..ResourceFork::default()
        };

        // Parse type list
        // Each entry is 8 bytes: type (4), count-1 (2), offset to ref list (2)
        if type_list_offset >= map.len() {
            tracing::warn!("Resource map type_list_offset out of bounds");
            return None;
        }
        let type_list = &map[type_list_offset..];

        for i in 0..num_types {
            let entry_offset = 2 + i * 8; // Skip the count at start
            if entry_offset + 8 > type_list.len() {
                break;
            }

            let res_type: ResourceType = [
                type_list[entry_offset],
                type_list[entry_offset + 1],
                type_list[entry_offset + 2],
                type_list[entry_offset + 3],
            ];
            let num_resources =
                u16::from_be_bytes([type_list[entry_offset + 4], type_list[entry_offset + 5]])
                    as usize
                    + 1;
            let ref_list_offset =
                u16::from_be_bytes([type_list[entry_offset + 6], type_list[entry_offset + 7]])
                    as usize;

            let type_str = String::from_utf8_lossy(&res_type);
            tracing::trace!("  Type '{}': {} resources", type_str, num_resources);

            // Parse reference list for this type
            // Offset is relative to type list start
            let ref_list = &map[type_list_offset + ref_list_offset..];

            for j in 0..num_resources {
                let ref_offset = j * 12;
                if ref_offset + 12 > ref_list.len() {
                    break;
                }

                let id = i16::from_be_bytes([ref_list[ref_offset], ref_list[ref_offset + 1]]);
                let name_offset =
                    u16::from_be_bytes([ref_list[ref_offset + 2], ref_list[ref_offset + 3]]);
                let attrs = ref_list[ref_offset + 4];
                // Data offset is 3 bytes (24-bit)
                let res_data_offset = ((ref_list[ref_offset + 5] as usize) << 16)
                    | ((ref_list[ref_offset + 6] as usize) << 8)
                    | (ref_list[ref_offset + 7] as usize);
                let reference_offset = type_list_offset + ref_list_offset + ref_offset;

                // Get resource name if present
                let name_bytes = if name_offset != 0xFFFF {
                    let name_pos = name_list_offset + name_offset as usize;
                    if name_pos < map.len() {
                        let name_len = map[name_pos] as usize;
                        if name_pos + 1 + name_len <= map.len() {
                            Some(map[name_pos + 1..name_pos + 1 + name_len].to_vec())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };
                let name = name_bytes
                    .as_deref()
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned());

                // Get resource data
                let abs_data_offset = data_offset + res_data_offset;
                if abs_data_offset + 4 > data.len() {
                    continue;
                }

                // Resource data is length-prefixed (4-byte length)
                let res_len = u32::from_be_bytes([
                    data[abs_data_offset],
                    data[abs_data_offset + 1],
                    data[abs_data_offset + 2],
                    data[abs_data_offset + 3],
                ]) as usize;

                let res_data_start = abs_data_offset + 4;
                if res_data_start + res_len > data.len() {
                    continue;
                }

                let raw_res_data = &data[res_data_start..res_data_start + res_len];
                let mut stored_attrs = attrs;
                let mut stored_raw_data = None;
                let mut stored_raw_attrs = None;
                let res_data = match super::compressed::decompress_if_needed(attrs, raw_res_data) {
                    Ok(Some(decompressed)) => {
                        stored_raw_data = Some(raw_res_data.to_vec());
                        stored_raw_attrs = Some(attrs);
                        stored_attrs &= !super::compressed::COMPRESSED_RESOURCE_ATTR;
                        tracing::debug!(
                            "    ID {}: decompressed {} -> {} bytes, attrs=0x{:02X}->0x{:02X}",
                            id,
                            res_len,
                            decompressed.len(),
                            attrs,
                            stored_attrs
                        );
                        decompressed
                    }
                    Ok(None) => raw_res_data.to_vec(),
                    Err(err) => {
                        stored_raw_data = Some(raw_res_data.to_vec());
                        stored_raw_attrs = Some(attrs);
                        tracing::warn!(
                                "    ID {}: failed to decompress compressed resource ({} bytes, attrs=0x{:02X}): {:?}",
                                id,
                                res_len,
                                attrs,
                                err
                            );
                        raw_res_data.to_vec()
                    }
                };
                let res_data = match super::ajcp::decompress_if_needed(&res_data) {
                    Ok(Some(decompressed)) => {
                        stored_raw_data.get_or_insert_with(|| raw_res_data.to_vec());
                        stored_raw_attrs.get_or_insert(attrs);
                        tracing::debug!(
                            "    ID {}: ajcp decompressed {} -> {} bytes",
                            id,
                            res_data.len(),
                            decompressed.len()
                        );
                        decompressed
                    }
                    Ok(None) => res_data,
                    Err(err) => {
                        tracing::warn!(
                            "    ID {}: failed to decompress ajcp resource ({} bytes): {:?}",
                            id,
                            res_data.len(),
                            err
                        );
                        res_data
                    }
                };

                tracing::trace!("    ID {}: {} bytes, attrs=0x{:02X}", id, res_len, attrs);

                let resource = Resource {
                    res_type,
                    id,
                    reference_offset,
                    name,
                    name_bytes,
                    data: res_data,
                    raw_data: stored_raw_data,
                    raw_attrs: stored_raw_attrs,
                    attrs: stored_attrs,
                };

                fork.resources.insert((res_type, id), resource);
            }
        }

        Some(fork)
    }

    /// Get a resource by type and ID
    pub fn get(&self, res_type: ResourceType, id: i16) -> Option<&Resource> {
        self.resources.get(&(res_type, id))
    }

    /// Get all resources of a given type
    pub fn get_all(&self, res_type: ResourceType) -> Vec<&Resource> {
        self.resources
            .values()
            .filter(|r| r.res_type == res_type)
            .collect()
    }

    /// Get CODE resource by ID
    pub fn get_code(&self, id: i16) -> Option<&Resource> {
        self.get(CODE_TYPE, id)
    }

    /// Get all CODE resources
    pub fn get_all_code(&self) -> Vec<&Resource> {
        self.get_all(CODE_TYPE)
    }

    /// Get a resource by type and name (case-insensitive)
    pub fn get_named(&self, res_type: ResourceType, name: &str) -> Option<&Resource> {
        for res in self.resources.values() {
            if res.res_type == res_type {
                if let Some(ref res_name) = res.name {
                    if res_name.eq_ignore_ascii_case(name) {
                        return Some(res);
                    }
                }
            }
        }
        None
    }
}

struct ResourceForkLayout<'a> {
    data_offset: usize,
    map_offset: usize,
    data_length: usize,
    map_length: usize,
    map: &'a [u8],
}

fn resource_fork_layout(data: &[u8]) -> Option<ResourceForkLayout<'_>> {
    if data.len() < 16 {
        return None;
    }

    // Resource fork header (16 bytes). Inside Macintosh Volume I, I-126.
    let data_offset = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    let map_offset = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
    let data_length = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
    let map_length = u32::from_be_bytes([data[12], data[13], data[14], data[15]]) as usize;

    let data_end = data_offset.checked_add(data_length)?;
    let map_end = map_offset.checked_add(map_length)?;
    if data_end > data.len() || map_end > data.len() {
        tracing::warn!("Resource fork header invalid");
        return None;
    }

    let map = &data[map_offset..map_end];
    if map.len() < 30 {
        tracing::warn!("Resource map too small");
        return None;
    }

    Some(ResourceForkLayout {
        data_offset,
        map_offset,
        data_length,
        map_length,
        map,
    })
}

fn resource_data_block_in_bounds(data: &[u8], data_offset: usize, res_data_offset: usize) -> bool {
    let Some(abs_data_offset) = data_offset.checked_add(res_data_offset) else {
        return false;
    };
    let Some(len_end) = abs_data_offset.checked_add(4) else {
        return false;
    };
    if len_end > data.len() {
        return false;
    }

    let res_len = u32::from_be_bytes([
        data[abs_data_offset],
        data[abs_data_offset + 1],
        data[abs_data_offset + 2],
        data[abs_data_offset + 3],
    ]) as usize;
    let Some(res_data_start) = abs_data_offset.checked_add(4) else {
        return false;
    };
    let Some(res_data_end) = res_data_start.checked_add(res_len) else {
        return false;
    };
    res_data_end <= data.len()
}

pub fn serialize_resource_fork(entries: &[ResourceForkEntry]) -> Option<Vec<u8>> {
    serialize_resource_fork_with_attrs(entries, 0)
}

/// Serialize resources with explicit resource-map attributes.
pub fn serialize_resource_fork_with_attrs(
    entries: &[ResourceForkEntry],
    map_attrs: u16,
) -> Option<Vec<u8>> {
    if entries.is_empty() {
        return Some(empty_resource_fork_bytes_with_attrs(map_attrs));
    }

    let mut sorted = entries.to_vec();
    sorted.sort_by_key(|entry| (entry.res_type, entry.id));

    let data_offset = 16usize;
    let mut data_area = Vec::new();
    let mut data_offsets = Vec::with_capacity(sorted.len());
    for entry in &sorted {
        if data_area.len() > 0x00ff_ffff {
            return None;
        }
        data_offsets.push(data_area.len());
        let len = u32::try_from(entry.data.len()).ok()?;
        data_area.extend_from_slice(&len.to_be_bytes());
        data_area.extend_from_slice(&entry.data);
    }

    let mut groups: BTreeMap<ResourceType, Vec<usize>> = BTreeMap::new();
    for (index, entry) in sorted.iter().enumerate() {
        groups.entry(entry.res_type).or_default().push(index);
    }

    let type_count = groups.len();
    let resource_count = sorted.len();
    let type_list_offset = 28usize;
    let ref_lists_start = 2usize.checked_add(type_count.checked_mul(8)?)?;
    let name_list_offset = type_list_offset
        .checked_add(ref_lists_start)?
        .checked_add(resource_count.checked_mul(12)?)?;

    let mut name_area = Vec::new();
    let mut name_offsets = vec![None; sorted.len()];
    for (index, entry) in sorted.iter().enumerate() {
        if !entry.name.is_empty() {
            let offset = u16::try_from(name_area.len()).ok()?;
            let name_len = entry.name.len().min(255);
            name_area.push(name_len as u8);
            name_area.extend_from_slice(&entry.name[..name_len]);
            name_offsets[index] = Some(offset);
        }
    }

    let map_length = name_list_offset.checked_add(name_area.len())?;
    let map_offset = data_offset.checked_add(data_area.len())?;
    let total_len = map_offset.checked_add(map_length)?;
    let data_length = u32::try_from(data_area.len()).ok()?;
    let map_offset_u32 = u32::try_from(map_offset).ok()?;
    let map_length_u32 = u32::try_from(map_length).ok()?;
    let name_list_offset_u16 = u16::try_from(name_list_offset).ok()?;

    let mut bytes = vec![0u8; total_len];
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&(data_offset as u32).to_be_bytes());
    header[4..8].copy_from_slice(&map_offset_u32.to_be_bytes());
    header[8..12].copy_from_slice(&data_length.to_be_bytes());
    header[12..16].copy_from_slice(&map_length_u32.to_be_bytes());
    bytes[0..16].copy_from_slice(&header);
    bytes[data_offset..data_offset + data_area.len()].copy_from_slice(&data_area);

    let map_start = map_offset;
    bytes[map_start..map_start + 16].copy_from_slice(&header);
    bytes[map_start + 22..map_start + 24].copy_from_slice(&map_attrs.to_be_bytes());
    bytes[map_start + 24..map_start + 26].copy_from_slice(&(type_list_offset as u16).to_be_bytes());
    bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset_u16.to_be_bytes());
    bytes[map_start + 28..map_start + 30]
        .copy_from_slice(&u16::try_from(type_count - 1).ok()?.to_be_bytes());

    let type_list_start = map_start + type_list_offset;
    let mut ref_cursor = ref_lists_start;
    for (type_index, (res_type, indexes)) in groups.iter().enumerate() {
        let entry_offset = type_list_start + 2 + type_index * 8;
        bytes[entry_offset..entry_offset + 4].copy_from_slice(res_type);
        bytes[entry_offset + 4..entry_offset + 6]
            .copy_from_slice(&u16::try_from(indexes.len() - 1).ok()?.to_be_bytes());
        bytes[entry_offset + 6..entry_offset + 8]
            .copy_from_slice(&u16::try_from(ref_cursor).ok()?.to_be_bytes());

        for index in indexes {
            let ref_offset = type_list_start + ref_cursor;
            let entry = &sorted[*index];
            bytes[ref_offset..ref_offset + 2].copy_from_slice(&(entry.id as u16).to_be_bytes());
            let name_offset = name_offsets[*index].unwrap_or(0xffff);
            bytes[ref_offset + 2..ref_offset + 4].copy_from_slice(&name_offset.to_be_bytes());
            bytes[ref_offset + 4] = entry.attrs;
            let data_offset = u32::try_from(data_offsets[*index]).ok()?;
            if data_offset > 0x00ff_ffff {
                return None;
            }
            let data_offset_bytes = data_offset.to_be_bytes();
            bytes[ref_offset + 5..ref_offset + 8].copy_from_slice(&data_offset_bytes[1..4]);
            ref_cursor += 12;
        }
    }

    let name_list_start = map_start + name_list_offset;
    bytes[name_list_start..name_list_start + name_area.len()].copy_from_slice(&name_area);

    Some(bytes)
}

fn empty_resource_fork_bytes_with_attrs(map_attrs: u16) -> Vec<u8> {
    let data_offset = 16u32;
    let data_length = 0u32;
    let map_offset = 16u32;
    let map_length = 30u32;

    let mut bytes = vec![0u8; (map_offset + map_length) as usize];
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&data_offset.to_be_bytes());
    header[4..8].copy_from_slice(&map_offset.to_be_bytes());
    header[8..12].copy_from_slice(&data_length.to_be_bytes());
    header[12..16].copy_from_slice(&map_length.to_be_bytes());
    bytes[0..16].copy_from_slice(&header);

    let map_start = map_offset as usize;
    bytes[map_start..map_start + 16].copy_from_slice(&header);
    bytes[map_start + 22..map_start + 24].copy_from_slice(&map_attrs.to_be_bytes());
    bytes[map_start + 24..map_start + 26].copy_from_slice(&28u16.to_be_bytes());
    bytes[map_start + 26..map_start + 28].copy_from_slice(&30u16.to_be_bytes());
    bytes[map_start + 28..map_start + 30].copy_from_slice(&0xffffu16.to_be_bytes());

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_constants() {
        assert_eq!(&CODE_TYPE, b"CODE");
    }

    fn make_single_resource_fork_bytes(res_type: [u8; 4], res_id: i16, data: &[u8]) -> Vec<u8> {
        let data_offset = 16u32;
        let data_length = (4 + data.len()) as u32;
        let map_offset = data_offset + data_length;
        let type_list_offset = 30u16;
        let ref_list_offset = 10u16;
        let name_list_offset = 40u16;
        let map_length = 52u32;

        let mut bytes = vec![0u8; (map_offset + map_length) as usize];
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&data_offset.to_be_bytes());
        header[4..8].copy_from_slice(&map_offset.to_be_bytes());
        header[8..12].copy_from_slice(&data_length.to_be_bytes());
        header[12..16].copy_from_slice(&map_length.to_be_bytes());
        bytes[0..16].copy_from_slice(&header);

        let data_start = data_offset as usize;
        bytes[data_start..data_start + 4].copy_from_slice(&(data.len() as u32).to_be_bytes());
        bytes[data_start + 4..data_start + 4 + data.len()].copy_from_slice(data);

        let map_start = map_offset as usize;
        bytes[map_start..map_start + 16].copy_from_slice(&header);
        bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
        bytes[map_start + 26..map_start + 28].copy_from_slice(&name_list_offset.to_be_bytes());

        let type_list_start = map_start + type_list_offset as usize;
        bytes[type_list_start..type_list_start + 2].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 2..type_list_start + 6].copy_from_slice(&res_type);
        bytes[type_list_start + 6..type_list_start + 8].copy_from_slice(&0u16.to_be_bytes());
        bytes[type_list_start + 8..type_list_start + 10]
            .copy_from_slice(&ref_list_offset.to_be_bytes());

        let ref_list_start = map_start + type_list_offset as usize + ref_list_offset as usize;
        bytes[ref_list_start..ref_list_start + 2].copy_from_slice(&(res_id as u16).to_be_bytes());
        bytes[ref_list_start + 2..ref_list_start + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
        bytes[ref_list_start + 5..ref_list_start + 8].copy_from_slice(&0u32.to_be_bytes()[1..4]);

        bytes
    }

    #[test]
    fn contains_code_checks_resource_map_without_full_parse() {
        let fork = make_single_resource_fork_bytes(*b"CODE", 0, &[1, 2, 3, 4]);

        assert!(ResourceFork::has_valid_layout(&fork));
        assert!(ResourceFork::contains_code(&fork, 0));
        assert!(ResourceFork::contains_resource(&fork, *b"CODE", 0));
        assert!(!ResourceFork::contains_code(&fork, 1));
        assert!(!ResourceFork::contains_resource(&fork, *b"PICT", 0));
    }

    #[test]
    fn contains_resource_rejects_out_of_bounds_resource_data() {
        let mut fork = make_single_resource_fork_bytes(*b"CODE", 0, &[1, 2, 3, 4]);
        fork[16..20].copy_from_slice(&64u32.to_be_bytes());

        assert!(ResourceFork::has_valid_layout(&fork));
        assert!(!ResourceFork::contains_code(&fork, 0));
    }

    #[test]
    fn resource_fork_layout_rejects_truncated_maps() {
        let mut fork = make_single_resource_fork_bytes(*b"CODE", 0, &[1, 2, 3, 4]);
        fork.truncate(fork.len() - 1);

        assert!(!ResourceFork::has_valid_layout(&fork));
        assert!(!ResourceFork::contains_code(&fork, 0));
    }
}
