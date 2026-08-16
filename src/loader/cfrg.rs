//! Code Fragment Resource (`'cfrg'`) parser.
//!
//! PowerPC applications advertise their native fragment through resource
//! type `'cfrg'`, ID 0. The record describes the fragment architecture,
//! usage, container location, data-fork offset, data-fork length, and
//! logical name. The PEF loader consumes the selected data-fork slice.

use crate::trap::types::decode_mac_roman;
use std::ops::Range;

pub const CFRG_VERSION: u32 = 1;
pub const ARCH_POWERPC: [u8; 4] = *b"pwpc";
pub const USAGE_LIB: u8 = 0;
pub const USAGE_APP: u8 = 1;
pub const USAGE_DROP_IN: u8 = 2;
pub const LOCATION_IN_MEMORY: u8 = 0;
pub const LOCATION_ON_DISK_FLAT: u8 = 1;
pub const LOCATION_ON_DISK_SEGMENTED: u8 = 2;
pub const WHOLE_FORK: u32 = 0;

const CFRG_HEADER_SIZE: usize = 32;
const CFRG_RECORD_FIXED_SIZE: usize = 42;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfrgResource {
    pub version: u32,
    pub fragments: Vec<CfrgFragment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfrgFragment {
    pub architecture: [u8; 4],
    pub update_level: u32,
    pub current_version: u32,
    pub oldest_definition_version: u32,
    pub app_stack_size: u32,
    pub app_library_directory: u16,
    pub usage: u8,
    pub location: u8,
    pub fragment_offset: u32,
    pub fragment_length: u32,
    pub record_length: u16,
    pub name: String,
}

impl CfrgFragment {
    pub fn is_powerpc_application_data_fork(self: &Self) -> bool {
        self.architecture == ARCH_POWERPC
            && self.usage == USAGE_APP
            && self.location == LOCATION_ON_DISK_FLAT
    }

    pub fn data_fork_range(&self, data_fork_len: usize) -> Option<Range<usize>> {
        if self.location != LOCATION_ON_DISK_FLAT {
            return None;
        }
        let start = usize::try_from(self.fragment_offset).ok()?;
        if start > data_fork_len {
            return None;
        }
        let end = if self.fragment_length == WHOLE_FORK {
            data_fork_len
        } else {
            start.checked_add(usize::try_from(self.fragment_length).ok()?)?
        };
        if end > data_fork_len || start > end {
            return None;
        }
        Some(start..end)
    }
}

pub fn parse_cfrg_resource(data: &[u8]) -> Option<CfrgResource> {
    if data.len() < CFRG_HEADER_SIZE {
        return None;
    }
    let version = read_u32(data, 8)?;
    let count = usize::try_from(read_u32(data, 28)?).ok()?;
    let mut offset = CFRG_HEADER_SIZE;
    let mut fragments = Vec::with_capacity(count);

    for _ in 0..count {
        if offset.checked_add(CFRG_RECORD_FIXED_SIZE)? > data.len() {
            return None;
        }
        let record_length = read_u16(data, offset + 40)?;
        let record_len = usize::from(record_length);
        if record_len < CFRG_RECORD_FIXED_SIZE {
            return None;
        }
        let record_end = offset.checked_add(record_len)?;
        if record_end > data.len() {
            return None;
        }

        let mut architecture = [0u8; 4];
        architecture.copy_from_slice(data.get(offset..offset + 4)?);
        let name = parse_fragment_name(&data[offset + CFRG_RECORD_FIXED_SIZE..record_end])?;

        fragments.push(CfrgFragment {
            architecture,
            update_level: read_u32(data, offset + 4)?,
            current_version: read_u32(data, offset + 8)?,
            oldest_definition_version: read_u32(data, offset + 12)?,
            app_stack_size: read_u32(data, offset + 16)?,
            app_library_directory: read_u16(data, offset + 20)?,
            usage: *data.get(offset + 22)?,
            location: *data.get(offset + 23)?,
            fragment_offset: read_u32(data, offset + 24)?,
            fragment_length: read_u32(data, offset + 28)?,
            record_length,
            name,
        });

        offset = record_end;
    }

    Some(CfrgResource { version, fragments })
}

pub fn select_powerpc_application_fragment(
    cfrg: &CfrgResource,
    data_fork_len: usize,
) -> Option<(&CfrgFragment, Range<usize>)> {
    cfrg.fragments.iter().find_map(|fragment| {
        if !fragment.is_powerpc_application_data_fork() {
            return None;
        }
        let range = fragment.data_fork_range(data_fork_len)?;
        Some((fragment, range))
    })
}

fn parse_fragment_name(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return Some(String::new());
    }
    let len = usize::from(bytes[0]);
    let end = 1usize.checked_add(len)?;
    if end > bytes.len() {
        return None;
    }
    Some(decode_mac_roman(&bytes[1..end]))
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        data.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_powerpc_application_data_fork_fragment() {
        let bytes = synthetic_cfrg(*b"pwpc", USAGE_APP, LOCATION_ON_DISK_FLAT, 0, WHOLE_FORK);

        let cfrg = parse_cfrg_resource(&bytes).unwrap();
        assert_eq!(cfrg.version, CFRG_VERSION);
        assert_eq!(cfrg.fragments.len(), 1);

        let fragment = &cfrg.fragments[0];
        assert_eq!(fragment.architecture, ARCH_POWERPC);
        assert_eq!(fragment.usage, USAGE_APP);
        assert_eq!(fragment.location, LOCATION_ON_DISK_FLAT);
        assert_eq!(fragment.name, "Test App\u{2122}");
        assert_eq!(fragment.data_fork_range(128).unwrap(), 0..128);
    }

    #[test]
    fn supports_nonzero_data_fork_fragment_range() {
        let bytes = synthetic_cfrg(*b"pwpc", USAGE_APP, LOCATION_ON_DISK_FLAT, 12, 64);
        let cfrg = parse_cfrg_resource(&bytes).unwrap();
        let (fragment, range) = select_powerpc_application_fragment(&cfrg, 100).unwrap();

        assert_eq!(fragment.fragment_offset, 12);
        assert_eq!(fragment.fragment_length, 64);
        assert_eq!(range, 12..76);
    }

    #[test]
    fn ignores_non_app_or_non_data_fork_fragments_for_app_selection() {
        for (usage, location) in [
            (USAGE_LIB, LOCATION_ON_DISK_FLAT),
            (USAGE_APP, LOCATION_IN_MEMORY),
            (USAGE_DROP_IN, LOCATION_ON_DISK_FLAT),
            (USAGE_APP, LOCATION_ON_DISK_SEGMENTED),
        ] {
            let bytes = synthetic_cfrg(*b"pwpc", usage, location, 0, WHOLE_FORK);
            let cfrg = parse_cfrg_resource(&bytes).unwrap();
            assert!(select_powerpc_application_fragment(&cfrg, 128).is_none());
        }
    }

    #[test]
    fn rejects_record_lengths_that_run_past_resource_end() {
        let mut bytes = synthetic_cfrg(*b"pwpc", USAGE_APP, LOCATION_ON_DISK_FLAT, 0, WHOLE_FORK);
        let fixed_record = CFRG_HEADER_SIZE + 40;
        bytes[fixed_record..fixed_record + 2].copy_from_slice(&128u16.to_be_bytes());

        assert!(parse_cfrg_resource(&bytes).is_none());
    }

    fn synthetic_cfrg(
        architecture: [u8; 4],
        usage: u8,
        location: u8,
        fragment_offset: u32,
        fragment_length: u32,
    ) -> Vec<u8> {
        let name = b"\x09Test App\xAA";
        let record_length = (CFRG_RECORD_FIXED_SIZE + name.len()) as u16;
        let mut bytes = vec![0u8; CFRG_HEADER_SIZE + usize::from(record_length)];
        write_u32(&mut bytes, 8, CFRG_VERSION);
        write_u32(&mut bytes, 28, 1);

        let record = CFRG_HEADER_SIZE;
        bytes[record..record + 4].copy_from_slice(&architecture);
        write_u32(&mut bytes, record + 16, 0x0003_2000);
        write_u16(&mut bytes, record + 20, 0);
        bytes[record + 22] = usage;
        bytes[record + 23] = location;
        write_u32(&mut bytes, record + 24, fragment_offset);
        write_u32(&mut bytes, record + 28, fragment_length);
        write_u16(&mut bytes, record + 40, record_length);
        bytes[record + CFRG_RECORD_FIXED_SIZE..].copy_from_slice(name);
        bytes
    }

    fn write_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_be_bytes());
    }
}
