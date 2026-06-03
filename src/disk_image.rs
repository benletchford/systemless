//! Read-only extraction helpers for classic Mac disk images.
//!
//! The runtime VFS is not a mounted HFS volume; it is a set of data-fork,
//! resource-fork, and Finder metadata maps. These helpers turn DC42/raw HFS
//! images into entries that can be seeded into that existing VFS.

use std::path::{Component, Path};

use hfs_reader::HfsVolume;

#[derive(Debug)]
pub(crate) struct DiskImageContents {
    pub volume_name: String,
    pub dirs: Vec<String>,
    pub files: Vec<DiskImageFile>,
}

#[derive(Debug)]
pub(crate) struct DiskImageFile {
    pub path: String,
    pub data: Vec<u8>,
    pub rsrc: Vec<u8>,
    pub file_type: [u8; 4],
    pub creator: [u8; 4],
    pub finder_flags: u16,
}

pub(crate) fn looks_like_dc42_or_hfs(bytes: &[u8]) -> bool {
    raw_filesystem_signature(bytes).is_some()
        || dc42_data_range(bytes)
            .and_then(|(start, end)| raw_filesystem_signature(&bytes[start..end]))
            .is_some()
}

pub(crate) fn extract_dc42_or_hfs(bytes: &[u8]) -> Result<Option<DiskImageContents>, String> {
    if !looks_like_dc42_or_hfs(bytes) {
        return Ok(None);
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
    matches!(sig, 0x4244 | 0x482B | 0xD2D7).then_some(sig)
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
        bytes[1024..1026].copy_from_slice(&0x4244u16.to_be_bytes());

        assert!(looks_like_dc42_or_hfs(&bytes));
    }

    #[test]
    fn detects_dc42_wrapped_hfs_payload() {
        let bytes = dc42_with_payload_signature(0x4244);

        assert!(looks_like_dc42_or_hfs(&bytes));
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
}
