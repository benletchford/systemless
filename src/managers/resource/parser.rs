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

use std::collections::HashMap;

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
    /// Resource data
    pub data: Vec<u8>,
    /// Resource attributes
    pub attrs: u8,
}

/// Parsed resource fork
#[derive(Debug, Default, Clone)]
pub struct ResourceFork {
    /// All resources indexed by (type, id)
    resources: HashMap<(ResourceType, i16), Resource>,
}

impl ResourceFork {
    /// Get all resources map
    pub fn resources(&self) -> &HashMap<(ResourceType, i16), Resource> {
        &self.resources
    }
}

impl ResourceFork {
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

        let mut fork = ResourceFork::default();

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
                let name = if name_offset != 0xFFFF {
                    let name_pos = name_list_offset + name_offset as usize;
                    if name_pos < map.len() {
                        let name_len = map[name_pos] as usize;
                        if name_pos + 1 + name_len <= map.len() {
                            Some(
                                String::from_utf8_lossy(
                                    &map[name_pos + 1..name_pos + 1 + name_len],
                                )
                                .into_owned(),
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

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
                let res_data = match super::compressed::decompress_if_needed(attrs, raw_res_data) {
                    Ok(Some(decompressed)) => {
                        tracing::debug!(
                            "    ID {}: decompressed {} -> {} bytes, attrs=0x{:02X}",
                            id,
                            res_len,
                            decompressed.len(),
                            attrs
                        );
                        decompressed
                    }
                    Ok(None) => raw_res_data.to_vec(),
                    Err(err) => {
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
                    data: res_data,
                    attrs,
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

        assert!(ResourceFork::contains_code(&fork, 0));
        assert!(ResourceFork::contains_resource(&fork, *b"CODE", 0));
        assert!(!ResourceFork::contains_code(&fork, 1));
        assert!(!ResourceFork::contains_resource(&fork, *b"PICT", 0));
    }

    #[test]
    fn contains_resource_rejects_out_of_bounds_resource_data() {
        let mut fork = make_single_resource_fork_bytes(*b"CODE", 0, &[1, 2, 3, 4]);
        fork[16..20].copy_from_slice(&64u32.to_be_bytes());

        assert!(!ResourceFork::contains_code(&fork, 0));
    }
}
