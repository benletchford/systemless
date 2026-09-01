//! Quilt patchwork and archive resource decoding.
//!
//! Quilt is a resource packing library used by Green Dragon Productions (e.g. Gridz).
//! It packs resource payloads into a shared data fork file while referencing them
//! via a `qDir` resource in the companion resource file.

use crate::mac_roman::decode_mac_roman;
use crate::managers::resource::{serialize_resource_fork, ResourceFork, ResourceForkEntry};
use crate::process_context::ProcessForkMap;


/// 60-byte directory record inside a `qDir` resource.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuiltDirRecord {
    pub res_type: [u8; 4],
    pub id: i16,
    pub data_len: usize,
    pub data_offset: usize,
    pub flags: u16,
    pub name: Vec<u8>,
}

/// Parse all 60-byte records from a `qDir` resource payload.
pub fn parse_qdir_records(data: &[u8]) -> Vec<QuiltDirRecord> {
    let mut records = Vec::new();
    for chunk in data.chunks_exact(60) {
        let name_len = u16::from_be_bytes([chunk[26], chunk[27]]) as usize;
        if name_len == 0 || name_len > 32 {
            continue;
        }
        let name_bytes = &chunk[28..28 + name_len];
        records.push(QuiltDirRecord {
            res_type: [chunk[0], chunk[1], chunk[2], chunk[3]],
            id: u32::from_be_bytes([chunk[4], chunk[5], chunk[6], chunk[7]]) as u16 as i16,
            data_len: u32::from_be_bytes([chunk[8], chunk[9], chunk[10], chunk[11]]) as usize,
            data_offset: u32::from_be_bytes([chunk[12], chunk[13], chunk[14], chunk[15]]) as usize,
            flags: u16::from_be_bytes([chunk[24], chunk[25]]),
            name: name_bytes.to_vec(),
        });
    }
    records
}

/// Check if a resource fork contains a `qDir` Quilt directory.
pub fn is_quilt_resource_fork(fork: &ResourceFork) -> bool {
    fork.resources().values().any(|r| r.res_type == *b"qDir")
}

/// Normalize path for VFS lookups.
pub fn normalize_vfs_path(path: &str) -> String {
    path.replace(':', "/")
        .split('/')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// Extract the file basename from a VFS path.
pub fn vfs_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Extract parent directory path from a VFS path.
pub fn vfs_parent_path(path: &str) -> &str {
    match path.rfind('/') {
        Some(index) => &path[..index],
        None => "",
    }
}

/// Find matching Quilt resources for a target path from available VFS data and resource forks.
pub fn quilt_named_resource_records(
    vfs_files: &ProcessForkMap,
    vfs_resource_files: &ProcessForkMap,
    target_path: &str,
) -> Option<(String, Vec<ResourceForkEntry>)> {
    let normalized_target = normalize_vfs_path(target_path);
    let target_basename = vfs_basename(&normalized_target);
    let versionless_target_basename = target_basename
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();

    let mut matches = Vec::new();
    let mut best_name_rank = 0u8;

    for (rsrc_path, rsrc_data) in vfs_resource_files.iter() {
        let Some(fork) = ResourceFork::parse(rsrc_data) else {
            continue;
        };
        let Some((_, data_file_bytes)) = vfs_files
            .iter()
            .find(|(path, _)| path.eq_ignore_ascii_case(rsrc_path))
        else {
            continue;
        };

        for resource in fork.resources().values() {
            if resource.res_type != *b"qDir" {
                continue;
            }
            let records = parse_qdir_records(&resource.data);
            for record in records {
                let name_str = decode_mac_roman(&record.name);
                let normalized_name = normalize_vfs_path(&name_str);
                let record_basename = vfs_basename(&normalized_name);
                let name_rank = if record_basename.eq_ignore_ascii_case(target_basename) {
                    2
                } else if versionless_target_basename.len() < target_basename.len()
                    && record_basename.eq_ignore_ascii_case(versionless_target_basename)
                {
                    1
                } else {
                    0
                };

                if name_rank == 0 || name_rank < best_name_rank {
                    continue;
                }
                if name_rank > best_name_rank {
                    matches.clear();
                    best_name_rank = name_rank;
                }

                let Some(end) = record.data_offset.checked_add(record.data_len) else {
                    continue;
                };
                let Some(data) = data_file_bytes.get(record.data_offset..end) else {
                    continue;
                };

                matches.push((
                    rsrc_path.clone(),
                    ResourceForkEntry {
                        res_type: record.res_type,
                        id: record.id,
                        name: record.name,
                        data: data.to_vec(),
                        attrs: 0,
                    },
                ));
            }
        }
    }

    if matches.is_empty() {
        return None;
    }

    let mut source_paths = matches
        .iter()
        .map(|(source_file, _)| source_file.clone())
        .collect::<Vec<_>>();
    source_paths.sort_by_key(|path| {
        let common_prefix = path
            .split('/')
            .zip(normalized_target.split('/'))
            .take_while(|(left, right)| left.eq_ignore_ascii_case(right))
            .count();
        let depth = path.split('/').filter(|comp| !comp.is_empty()).count();
        std::cmp::Reverse((common_prefix, depth))
    });
    source_paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    let source_path = source_paths.first()?;
    matches.retain(|(src, _)| src.eq_ignore_ascii_case(source_path));



    let source_file_path = matches.first()?.0.clone();
    let mut resources = matches.into_iter().map(|(_, res)| res).collect::<Vec<_>>();

    add_quilt_anam_picture_resources(
        vfs_files,
        vfs_resource_files,
        &source_file_path,
        &mut resources,
    );
    expand_compact_quilt_frames(&mut resources);
    wrap_quilt_raw_pict_frames(&mut resources);
    Some((source_file_path, resources))
}

/// Resolve referenced animation picture resources (`ANAM` -> `.PICR`).
pub fn add_quilt_anam_picture_resources(
    vfs_files: &ProcessForkMap,
    vfs_resource_files: &ProcessForkMap,
    source_file_path: &str,
    resources: &mut Vec<ResourceForkEntry>,
) {
    apply_quilt_animation_resource_names(resources);
    let pict_type = *b"PICT";
    let img_type = *b"#Img";
    if resources
        .iter()
        .any(|res| res.res_type == pict_type || res.res_type == img_type)
    {
        return;
    }

    let anam_type = *b"ANAM";
    let Some(picture_name) = resources
        .iter()
        .find(|res| res.res_type == anam_type)
        .and_then(|res| {
            let len = res
                .data
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(res.data.len());
            let name = decode_mac_roman(&res.data[..len]);
            let normalized = normalize_vfs_path(&name);
            normalized
                .to_ascii_lowercase()
                .ends_with(".picr")
                .then_some(normalized)
        })
    else {
        return;
    };

    let picture_basename = vfs_basename(&picture_name);
    let Some(rsrc_data) = vfs_resource_files.get(source_file_path) else {
        return;
    };
    let Some(fork) = ResourceFork::parse(rsrc_data) else {
        return;
    };
    let Some(data_bytes) = vfs_files.get(source_file_path) else {
        return;
    };

    let existing = resources
        .iter()
        .map(|res| (res.res_type, res.id))
        .collect::<Vec<_>>();

    for resource in fork.resources().values() {
        if resource.res_type != *b"qDir" {
            continue;
        }
        let records = parse_qdir_records(&resource.data);
        for record in records {
            let name_str = decode_mac_roman(&record.name);
            let normalized_name = normalize_vfs_path(&name_str);
            if !vfs_basename(&normalized_name).eq_ignore_ascii_case(picture_basename) {
                continue;
            }
            if existing
                .iter()
                .any(|(t, id)| *t == record.res_type && *id == record.id)
            {
                continue;
            }
            let Some(end) = record.data_offset.checked_add(record.data_len) else {
                continue;
            };
            let Some(data) = data_bytes.get(record.data_offset..end) else {
                continue;
            };
            resources.push(ResourceForkEntry {
                res_type: record.res_type,
                id: record.id,
                name: record.name,
                data: data.to_vec(),
                attrs: 0,
            });
        }
    }
}

/// Copy animation picture names into `Alst` resources.
pub fn apply_quilt_animation_resource_names(resources: &mut [ResourceForkEntry]) {
    let animation_list_type = *b"Alst";
    let animation_name_type = *b"ANAM";
    let names = resources
        .iter()
        .filter(|res| res.res_type == animation_name_type)
        .filter_map(|res| {
            let len = res
                .data
                .iter()
                .position(|b| *b == 0)
                .unwrap_or(res.data.len());
            let name = res.data.get(..len)?;
            decode_mac_roman(name)
                .to_ascii_lowercase()
                .ends_with(".picr")
                .then(|| (res.id, name.to_vec()))
        })
        .collect::<Vec<_>>();

    for res in resources
        .iter_mut()
        .filter(|res| res.res_type == animation_list_type)
    {
        if let Some((_, name)) = names.iter().find(|(id, _)| *id == res.id) {
            res.name.clone_from(name);
        }
    }
}

/// Split compact packed multi-frame resources into individual `frms` and `PICT` entries.
pub fn expand_compact_quilt_frames(resources: &mut Vec<ResourceForkEntry>) {
    let img_type = *b"#Img";
    let frms_type = *b"frms";
    let pict_type = *b"PICT";

    let Some(img) = resources
        .iter()
        .find(|res| res.res_type == img_type && res.data.len() >= 8)
    else {
        return;
    };

    let frms_indices = resources
        .iter()
        .enumerate()
        .filter(|(_, res)| res.res_type == frms_type)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    let pict_indices = resources
        .iter()
        .enumerate()
        .filter(|(_, res)| res.res_type == pict_type)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();

    if frms_indices.len() != 1 || pict_indices.len() != 1 {
        return;
    }

    let frms = resources[frms_indices[0]].clone();
    let pict = resources[pict_indices[0]].clone();

    let header_frame_count = u16::from_be_bytes([img.data[6], img.data[7]]) as usize;
    let inferred_frame_count = frms.data.len() / 8;
    let frame_count = if header_frame_count > 1
        && frms.data.len() == header_frame_count * 8
        && pict.data.len() % header_frame_count == 0
    {
        header_frame_count
    } else if inferred_frame_count > 1 && frms.data.len() == inferred_frame_count * 8 {
        inferred_frame_count
    } else {
        return;
    };

    if pict.data.len() % frame_count != 0 {
        return;
    }
    let frame_pict_len = pict.data.len() / frame_count;

    let mut expanded = resources
        .iter()
        .filter(|res| res.res_type != frms_type && res.res_type != pict_type)
        .cloned()
        .collect::<Vec<_>>();

    for frame_index in 0..frame_count {
        let Some(res_id) = 1000i16.checked_add(frame_index as i16) else {
            return;
        };
        let frame_offset = frame_index * 8;
        let pict_offset = frame_index * frame_pict_len;

        let mut frame_resource = frms.clone();
        frame_resource.id = res_id;
        frame_resource.data = frms.data[frame_offset..frame_offset + 8].to_vec();
        expanded.push(frame_resource);

        let mut pict_resource = pict.clone();
        pict_resource.id = res_id;
        pict_resource.data = pict.data[pict_offset..pict_offset + frame_pict_len].to_vec();
        expanded.push(pict_resource);
    }

    *resources = expanded;
}

/// Wrap raw pixel payloads with a 24-byte QuickDraw Picture header.
///
/// Pixel values remain Quilt source-palette indices; renderers must remap
/// source index 0 (black) through the active destination palette.
pub fn wrap_quilt_raw_pict_frames(resources: &mut [ResourceForkEntry]) {
    let frms_type = *b"frms";
    let pict_type = *b"PICT";

    let frame_rects = resources
        .iter()
        .filter(|res| res.res_type == frms_type && res.data.len() == 8)
        .map(|res| (res.id, res.data.clone()))
        .collect::<Vec<_>>();

    if frame_rects.is_empty() {
        return;
    }

    for res in resources.iter_mut() {
        if res.res_type != pict_type {
            continue;
        }
        let Some((_, rect)) = frame_rects.iter().find(|(id, _)| *id == res.id) else {
            continue;
        };
        let top = i16::from_be_bytes([rect[0], rect[1]]);
        let bottom = i16::from_be_bytes([rect[4], rect[5]]);
        let height = i32::from(bottom) - i32::from(top);
        if height <= 0 || res.data.len() % height as usize != 0 {
            continue;
        }
        let Some(pict_size) = res.data.len().checked_add(24) else {
            continue;
        };
        let pict_size = u16::try_from(pict_size).unwrap_or(u16::MAX);
        let mut wrapped = Vec::with_capacity(usize::from(pict_size));
        wrapped.extend_from_slice(&pict_size.to_be_bytes());
        wrapped.extend_from_slice(rect);
        wrapped.extend_from_slice(&[0; 14]);
        wrapped.extend_from_slice(&res.data);
        res.data = wrapped;
    }
}

/// Synthesize a default `#Img` resource if missing when PICT frames are present.
pub fn synthesize_quilt_img_resource_if_missing(
    path: &str,
    resources: &mut Vec<ResourceForkEntry>,
) {
    let img_type = *b"#Img";
    if resources.iter().any(|res| res.res_type == img_type) {
        return;
    }
    let pict_type = *b"PICT";
    let frame_count = resources
        .iter()
        .filter(|res| res.res_type == pict_type)
        .count();
    if frame_count == 0 {
        return;
    }

    let mut data = vec![0u8; 18];
    data[0..2].copy_from_slice(&1u16.to_be_bytes());
    data[2..4].copy_from_slice(&1u16.to_be_bytes());
    data[4..6].copy_from_slice(&1u16.to_be_bytes());
    data[6..8].copy_from_slice(&(u16::try_from(frame_count).unwrap_or(u16::MAX)).to_be_bytes());
    data[8..10].copy_from_slice(&1u16.to_be_bytes());
    data[10..12].copy_from_slice(&1u16.to_be_bytes());

    resources.push(ResourceForkEntry {
        res_type: img_type,
        id: 1000,
        name: vfs_basename(path).as_bytes().to_vec(),
        data,
        attrs: 0,
    });
}

/// Materialize Quilt resources into existing VFS resource forks and synthesize virtual Quilt resource files.
/// Returns `(materialized_count, Vec<(synthesized_path, file_type, creator, finder_flags)>)`.
pub fn materialize_quilt_resources_for_vfs(
    vfs: &ProcessForkMap,
    vfs_rsrc: &mut ProcessForkMap,
) -> (usize, Vec<(String, [u8; 4], [u8; 4], u16)>) {
    let mut qdir_files = Vec::new();
    for (rsrc_path, rsrc_data) in vfs_rsrc.iter() {
        if let Some(fork) = ResourceFork::parse(rsrc_data) {
            if is_quilt_resource_fork(&fork) {
                if vfs.iter().any(|(p, _)| p.eq_ignore_ascii_case(rsrc_path)) {
                    qdir_files.push(rsrc_path.clone());
                }
            }
        }
    }



    if qdir_files.is_empty() {
        return (0, Vec::new());
    }

    let mut materialized_count = 0usize;
    let mut synthesized_files = Vec::new();

    // 1. Materialize Quilt resources into all existing files in vfs_rsrc
    let existing_paths = vfs_rsrc.keys().cloned().collect::<Vec<_>>();
    for path in &existing_paths {
        if let Some((_, quilt_entries)) = quilt_named_resource_records(vfs, vfs_rsrc, path) {
            let mut base_entries = vfs_rsrc
                .get(path)
                .and_then(|data| ResourceFork::parse(data))
                .map(|fork| {
                    fork.resources()
                        .values()
                        .map(|r| ResourceForkEntry {
                            res_type: r.res_type,
                            id: r.id,
                            name: r.name_bytes.clone().unwrap_or_default(),
                            data: r.data.clone(),
                            attrs: r.attrs,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();

            let mut added_any = false;
            for mut q_entry in quilt_entries {
                if !base_entries
                    .iter()
                    .any(|e| e.res_type == q_entry.res_type && e.id == q_entry.id)
                {
                    if q_entry.name.is_empty() {
                        q_entry.name = vfs_basename(path).as_bytes().to_vec();
                    }
                    base_entries.push(q_entry);
                    added_any = true;
                }
            }

            if added_any {
                synthesize_quilt_img_resource_if_missing(path, &mut base_entries);
                if let Some(new_fork_data) = serialize_resource_fork(&base_entries) {
                    vfs_rsrc.insert(path.clone(), new_fork_data);
                    materialized_count += 1;
                }
            }
        }
    }

    // 2. Discover all unique target names mentioned in all qDir records
    let mut named_targets = Vec::new();
    for qdir_path in &qdir_files {
        let Some(rsrc_data) = vfs_rsrc.get(qdir_path) else {
            continue;
        };
        let Some(fork) = ResourceFork::parse(rsrc_data) else {
            continue;
        };
        for res in fork.resources().values() {
            if res.res_type != *b"qDir" {
                continue;
            }
            for rec in parse_qdir_records(&res.data) {
                let name_str = decode_mac_roman(&rec.name);
                let normalized = normalize_vfs_path(&name_str);
                named_targets.push((qdir_path.clone(), normalized));
            }
        }
    }

    // 3. For any named targets that don't exist as separate files in vfs_rsrc, synthesize them
    for (qdir_path, target_name) in named_targets {
        let target_base = vfs_basename(&target_name);
        let already_exists = vfs_rsrc.keys().any(|existing| {
            let existing_base = vfs_basename(existing);
            existing_base.eq_ignore_ascii_case(target_base)
        });

        if !already_exists {
            let qdir_parent = vfs_parent_path(&qdir_path);
            let synth_path = if qdir_parent.is_empty() {
                target_name.clone()
            } else {
                format!("{}/{}", qdir_parent, target_name)
            };

            if let Some((_, mut quilt_entries)) =
                quilt_named_resource_records(vfs, vfs_rsrc, &synth_path)
            {
                synthesize_quilt_img_resource_if_missing(&synth_path, &mut quilt_entries);
                if let Some(new_fork_data) = serialize_resource_fork(&quilt_entries) {
                    vfs_rsrc.insert(synth_path.clone(), new_fork_data);
                    synthesized_files.push((synth_path, *b"bits", *b"Game", 0));
                    materialized_count += 1;
                }
            }
        }
    }

    (materialized_count, synthesized_files)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn qdir_record(
        res_type: &[u8; 4],
        id: u32,
        len: u32,
        offset: u32,
        name: &[u8],
    ) -> [u8; 60] {
        let mut record = [0u8; 60];
        record[0..4].copy_from_slice(res_type);
        record[4..8].copy_from_slice(&id.to_be_bytes());
        record[8..12].copy_from_slice(&len.to_be_bytes());
        record[12..16].copy_from_slice(&offset.to_be_bytes());
        record[24..26].copy_from_slice(&1u16.to_be_bytes());
        record[26..28].copy_from_slice(&(name.len() as u16).to_be_bytes());
        record[28..28 + name.len()].copy_from_slice(name);
        record
    }

    #[test]
    fn quilt_materialization_combines_base_and_quilt_resources() {
        let mut vfs = ProcessForkMap::default();
        let mut vfs_rsrc = ProcessForkMap::default();

        let mut bits = vec![0u8; 64];
        bits[16..21].copy_from_slice(b"state");

        let mut qdir = [0u8; 60];
        qdir[0..4].copy_from_slice(b"Stbl");
        qdir[4..8].copy_from_slice(&1003u32.to_be_bytes());
        qdir[8..12].copy_from_slice(&5u32.to_be_bytes());
        qdir[12..16].copy_from_slice(&16u32.to_be_bytes());
        qdir[24..26].copy_from_slice(&1u16.to_be_bytes());
        qdir[26..28].copy_from_slice(&12u16.to_be_bytes());
        qdir[28..40].copy_from_slice(b"Game Control");

        let bits_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir.to_vec(),
            attrs: 0,
        }])
        .unwrap();

        let target_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"PICT",
            id: 1000,
            name: b"Game Control 2".to_vec(),
            data: b"picture".to_vec(),
            attrs: 0,
        }])
        .unwrap();

        vfs.insert(
            "Gridz Data/Gridz Demo Bits".to_string(),
            bits,
        );
        vfs_rsrc.insert(
            "Gridz Data/Gridz Demo Bits".to_string(),
            bits_fork,
        );
        vfs_rsrc.insert(
            "Gridz Data/Control Files/Game Control 2".to_string(),
            target_fork,
        );

        let (count, _) = materialize_quilt_resources_for_vfs(&vfs, &mut vfs_rsrc);
        assert!(count >= 1);

        let target_rsrc = vfs_rsrc
            .get("Gridz Data/Control Files/Game Control 2")
            .expect("target file exists");
        let fork = ResourceFork::parse(target_rsrc).expect("valid resource fork");
        assert_eq!(fork.get(*b"PICT", 1000).unwrap().data, b"picture");
        assert_eq!(fork.get(*b"Stbl", 1003).unwrap().data, b"state");
    }

    #[test]
    fn quilt_materialization_synthesizes_virtual_picr_file() {
        let mut vfs = ProcessForkMap::default();
        let mut vfs_rsrc = ProcessForkMap::default();

        let mut data = vec![0u8; 128];
        data[16..21].copy_from_slice(b"#data");
        data[64..72].copy_from_slice(b"pictdata");
        let name = b"ColorSwatches.PICR";
        let mut qdir = Vec::new();
        qdir.extend_from_slice(&qdir_record(b"#Img", 1000, 5, 16, name));
        qdir.extend_from_slice(&qdir_record(b"PICT", 1000, 8, 64, name));
        let raw_fork = serialize_resource_fork(&[ResourceForkEntry {
            res_type: *b"qDir",
            id: 1000,
            name: b"Quilt Patchwork".to_vec(),
            data: qdir,
            attrs: 0,
        }])
        .unwrap();

        vfs.insert("Game Data/Packed Bits".to_string(), data);
        vfs_rsrc.insert("Game Data/Packed Bits".to_string(), raw_fork);

        let (count, _) = materialize_quilt_resources_for_vfs(&vfs, &mut vfs_rsrc);
        assert!(count >= 1);

        let synth_key = vfs_rsrc
            .keys()
            .find(|k| k.ends_with("ColorSwatches.PICR"))
            .expect("synthesized PICR file");
        let fork = ResourceFork::parse(vfs_rsrc.get(synth_key).unwrap()).unwrap();
        assert_eq!(fork.get(*b"#Img", 1000).unwrap().data, b"#data");
        assert_eq!(fork.get(*b"PICT", 1000).unwrap().data, b"pictdata");
    }
}
