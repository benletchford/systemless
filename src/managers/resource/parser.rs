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
    /// Parse a resource fork from raw data
    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 16 {
            return None;
        }

        // Resource fork header (16 bytes)
        // Inside Macintosh Volume I, I-126
        let data_offset = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let map_offset = u32::from_be_bytes([data[4], data[5], data[6], data[7]]) as usize;
        let data_length = u32::from_be_bytes([data[8], data[9], data[10], data[11]]) as usize;
        let map_length = u32::from_be_bytes([data[12], data[13], data[14], data[15]]) as usize;

        tracing::debug!(
            "Resource fork: data@0x{:04X} ({}), map@0x{:04X} ({})",
            data_offset,
            data_length,
            map_offset,
            map_length
        );

        if map_offset + map_length > data.len() || data_offset + data_length > data.len() {
            tracing::warn!("Resource fork header invalid");
            return None;
        }

        let map = &data[map_offset..map_offset + map_length];

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_type_constants() {
        assert_eq!(&CODE_TYPE, b"CODE");
    }
}
