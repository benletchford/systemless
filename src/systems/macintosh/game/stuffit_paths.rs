//! Recover component boundaries before mapping archive names into the VFS.
//!
//! StuffIt's flattened names use `/` both for hierarchy and for legal HFS
//! basename characters. Only the original entry metadata can disambiguate them.

use crate::trap::TrapDispatcher;
use std::collections::{HashMap, HashSet};
use stuffit::{ArchiveFormat, SitArchive, SitEntry};

fn bytes(data: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
    let end = offset
        .checked_add(length)
        .ok_or("StuffIt offset overflow")?;
    data.get(offset..end)
        .ok_or_else(|| "Truncated StuffIt path metadata".into())
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, String> {
    let value = bytes(data, offset, 2)?;
    Ok(u16::from_be_bytes([value[0], value[1]]))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, String> {
    let value = bytes(data, offset, 4)?;
    Ok(u32::from_be_bytes([value[0], value[1], value[2], value[3]]))
}

fn apply(entry: &mut SitEntry, components: &[String], folder: bool) -> Result<(), String> {
    if entry.name != components.join("/") || entry.is_folder != folder {
        return Err("StuffIt path metadata disagrees with decoded entry".into());
    }
    entry.name = components
        .iter()
        .map(|part| TrapDispatcher::encode_hfs_component_for_vfs(part))
        .collect::<Vec<_>>()
        .join("/");
    Ok(())
}

pub(super) fn parse(data: &[u8]) -> Result<SitArchive, String> {
    let mut archive = SitArchive::parse(data).map_err(|error| format!("{error:?}"))?;
    let Some(first) = archive.entries.first() else {
        return Ok(archive);
    };
    match first.format {
        ArchiveFormat::Classic => classic_paths(data, &mut archive.entries)?,
        ArchiveFormat::Sit5 => sit5_paths(data, &mut archive.entries)?,
    }
    Ok(archive)
}

fn classic_paths(data: &[u8], entries: &mut [SitEntry]) -> Result<(), String> {
    let total = u32_at(data, 6)? as usize;
    let mut offset = 22usize;
    let mut parents = Vec::new();
    let mut index = 0;
    while offset.checked_add(112).is_some_and(|end| end <= total) {
        let header = bytes(data, offset, 112)?;
        offset += 112;
        let methods = [header[0] & !0x90, header[1] & !0x90];
        if methods.contains(&0x21) && !methods.contains(&0x20) {
            parents.pop();
            continue;
        }
        let name =
            crate::mac_roman::decode_mac_roman(&header[3..3 + usize::from(header[2]).min(31)]);
        let mut components = parents.clone();
        components.push(name);
        let folder = methods.contains(&0x20);
        apply(
            entries.get_mut(index).ok_or("Extra StuffIt path entry")?,
            &components,
            folder,
        )?;
        index += 1;
        if folder {
            parents = components;
        } else {
            let length = u64::from(u32_at(header, 92)?) + u64::from(u32_at(header, 96)?);
            offset = offset
                .checked_add(usize::try_from(length).map_err(|_| "StuffIt fork size overflow")?)
                .ok_or("StuffIt offset overflow")?;
        }
    }
    if index != entries.len() {
        return Err("Missing StuffIt path entries".into());
    }
    Ok(())
}

fn sit5_paths(data: &[u8], entries: &mut [SitEntry]) -> Result<(), String> {
    let xor = if bytes(data, 83, 1)?[0] & 0x10 == 0 {
        0xA5A5A5A5
    } else {
        0
    };
    let mut offset = (u32_at(data, 88)? ^ xor) as usize;
    let mut directories: HashMap<u32, Vec<String>> = HashMap::new();
    let mut seen = HashSet::new();
    let mut index = 0;
    while index < entries.len() {
        if !seen.insert(offset) {
            return Err("Cyclic StuffIt path metadata".into());
        }
        let header = bytes(data, offset, 48)?;
        if u32_at(header, 0)? != 0xA5A5A5A5 {
            return Err("Invalid StuffIt path header".into());
        }
        let folder = header[9] & 0x40 != 0;
        let name_length = usize::from(u16_at(header, 30)?);
        if folder && name_length == 0 {
            let next = u32_at(header, 22)? as usize;
            offset = if next == 0 { offset + 48 } else { next };
            continue;
        }
        let name_offset = offset.checked_add(48).ok_or("StuffIt offset overflow")?;
        let name = String::from_utf8_lossy(bytes(data, name_offset, name_length)?).into_owned();
        let parent = u32_at(header, 26)? ^ xor;
        let mut components = directories.get(&parent).cloned().unwrap_or_default();
        components.push(name);
        apply(&mut entries[index], &components, folder)?;
        index += 1;
        if folder {
            directories.insert(offset as u32, components);
        }
        let metadata = offset
            .checked_add(usize::from(u16_at(header, 6)?))
            .ok_or("StuffIt offset overflow")?;
        let metadata_length = if header[4] == 1 { 36 } else { 32 };
        bytes(data, metadata, metadata_length)?;
        let mut next = metadata
            .checked_add(metadata_length)
            .ok_or("StuffIt offset overflow")?;
        if folder {
            let child = u32_at(header, 34)?;
            if child != 0 && child != u32::MAX {
                next = child as usize;
            }
        } else {
            let mut resource_length = 0u32;
            if u16_at(data, metadata)? & 1 != 0 {
                let resource = bytes(data, next, 14)?;
                resource_length = u32_at(resource, 4)?;
                next += 14;
                if header[9] & 0x20 != 0 {
                    next += usize::from(resource[13]);
                }
            }
            let length = u64::from(resource_length) + u64::from(u32_at(header, 38)?);
            next = next
                .checked_add(usize::try_from(length).map_err(|_| "StuffIt fork size overflow")?)
                .ok_or("StuffIt offset overflow")?;
            if next > data.len() {
                return Err("Truncated StuffIt forks".into());
            }
        }
        offset = next;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, folder: bool) -> SitEntry {
        SitEntry {
            name: name.into(),
            data_fork: if folder {
                vec![]
            } else {
                b"original data".to_vec()
            },
            resource_fork: if folder {
                vec![]
            } else {
                b"original resources".to_vec()
            },
            file_type: *b"TEXT",
            creator: *b"ttxt",
            is_folder: folder,
            data_method: 0,
            rsrc_method: 0,
            data_ulen: 0,
            rsrc_ulen: 0,
            finder_flags: 0,
            creation_date: 0,
            modification_date: 0,
            is_compressed: false,
            format: ArchiveFormat::Sit5,
        }
    }

    #[test]
    fn sit5_literal_slashes_preserve_components_and_forks() {
        let mut archive = SitArchive::new();
        for (name, folder) in [
            ("Modules", true),
            ("Modules/Galactica", true),
            ("Modules/Galactica/ordinary", false),
            ("Modules/Galactica!en", false),
            ("Folder!name", true),
            ("Folder!name/child", false),
        ] {
            archive.add_entry(entry(name, folder));
        }
        let mut data = archive.serialize().unwrap();
        // Change only original basename bytes, retaining identical lengths and
        // directory offsets. A same-prefix real folder must not absorb the file.
        for name in [b"Galactica!en".as_slice(), b"Folder!name".as_slice()] {
            let offsets: Vec<_> = data
                .windows(name.len())
                .enumerate()
                .filter_map(|(offset, value)| (value == name).then_some(offset))
                .collect();
            for offset in offsets {
                let position = name.iter().position(|byte| *byte == b'!').unwrap();
                data[offset + position] = b'/';
            }
        }
        let decoded = parse(&data).unwrap();
        let names: Vec<_> = decoded
            .entries
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();
        assert!(names.contains(&"Modules/Galactica/ordinary"));
        assert!(names.contains(&"Modules/Galactica\u{f02f}en"));
        assert!(names.contains(&"Folder\u{f02f}name/child"));
        for file in decoded.entries.iter().filter(|entry| !entry.is_folder) {
            let (data, resource) = file.decompressed_forks().unwrap();
            assert_eq!(data, b"original data");
            assert_eq!(resource, b"original resources");
        }
    }

    #[test]
    fn classic_literal_slashes_use_folder_markers() {
        fn header(name: &str, method: u8, data: &[u8]) -> Vec<u8> {
            let mut header = vec![0; 112];
            header[0] = method;
            header[1] = method;
            header[2] = name.len() as u8;
            header[3..3 + name.len()].copy_from_slice(name.as_bytes());
            header[88..92].copy_from_slice(&(data.len() as u32).to_be_bytes());
            header[96..100].copy_from_slice(&(data.len() as u32).to_be_bytes());
            let mut crc = 0u16;
            for byte in &header[..110] {
                crc ^= u16::from(*byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xA001
                    } else {
                        crc >> 1
                    };
                }
            }
            header[110..112].copy_from_slice(&crc.to_be_bytes());
            header.extend_from_slice(data);
            header
        }
        let mut data = vec![0; 22];
        data[..4].copy_from_slice(b"SIT!");
        data[10..14].copy_from_slice(b"rLau");
        data.extend(header("Folder/name", 0x20, &[]));
        data.extend(header("Galactica/en", 0, b"original"));
        data.extend(header("", 0x21, &[]));
        let size = data.len() as u32;
        data[6..10].copy_from_slice(&size.to_be_bytes());
        let decoded = parse(&data).unwrap();
        assert_eq!(
            decoded.entries[1].name,
            "Folder\u{f02f}name/Galactica\u{f02f}en"
        );
        assert_eq!(
            decoded.entries[1].decompressed_forks().unwrap().0,
            b"original"
        );
    }
}
