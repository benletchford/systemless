//! Process-owned Internet Config instances for Carbon applications.

use super::*;
#[path = "internet_config_mappings.rs"]
mod mappings;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcInternetConfigState {
    instances: BTreeMap<u32, InternetConfigInstance>,
    // InternetConfig.h: IC 2.5 automatically selects the default configuration.
    // The empty default preferences set starts at an opaque, stable seed.
    pub(super) preferences_seed: u32,
    configuration_seeds: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InternetConfigInstance {
    signature: u32,
    path: String,
    permission: u8,
}

impl Default for PpcInternetConfigState {
    fn default() -> Self {
        Self {
            instances: BTreeMap::new(),
            preferences_seed: 1,
            configuration_seeds: BTreeMap::new(),
        }
    }
}

pub(super) fn dispatch_internet_config_import(
    binding: &PpcImportBinding,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    toolbox_startup: &mut PpcToolboxStartupState,
    vfs_files: &mut ProcessVfsFileRecords,
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_directories: &[PpcVfsDirectory],
    default_dir_id: u32,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
) -> Option<PpcImportAction> {
    let state = &mut toolbox_startup.internet_config;
    // Apple InternetConfig.h: ICInstance is an opaque, process-local pointer.
    // ICStart(ICInstance *, OSType), ICStop(ICInstance), and
    // ICGetSeed(ICInstance, long *) return a 32-bit OSStatus in r3.
    let result: i32 = match binding.dispatcher_target {
        PpcImportDispatcherTarget::ICCountMapEntries
        | PpcImportDispatcherTarget::ICGetIndMapEntry
        | PpcImportDispatcherTarget::ICGetMapEntry => {
            if !state.instances.contains_key(&cpu.gpr[3]) {
                i32::from(PPC_PARAM_ERR)
            } else {
                i32::from(mappings::dispatch(
                    &binding.dispatcher_target,
                    cpu,
                    memory,
                    process_memory_manager,
                ))
            }
        }
        PpcImportDispatcherTarget::ICStart => {
            let output = cpu.gpr[3];
            if output == 0 || !ppc_memory_can_write_bytes(memory, output, 4) {
                i32::from(PPC_PARAM_ERR)
            } else {
                memory.write_u32_be(output, 0)?;
                let instance = process_memory_manager.new_native_ptr(memory, 4, true);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                if instance == 0 {
                    i32::from(PPC_MEM_FULL_ERR)
                } else {
                    let signature = cpu.gpr[4];
                    if memory.write_u32_be(instance, signature).is_some()
                        && memory.write_u32_be(output, instance).is_some()
                    {
                        state.instances.insert(
                            instance,
                            InternetConfigInstance {
                                signature,
                                path: DEFAULT_CONFIG_PATH.into(),
                                permission: 0,
                            },
                        );
                        0
                    } else {
                        let _ = process_memory_manager.dispose_native_ptr(instance);
                        ppc_apply_process_native_allocator(
                            process_memory_manager,
                            memory,
                            heap_cursor,
                            last_mem_error,
                        );
                        i32::from(PPC_PARAM_ERR)
                    }
                }
            }
        }
        PpcImportDispatcherTarget::ICStop => {
            let instance = cpu.gpr[3];
            if state.instances.remove(&instance).is_none() {
                i32::from(PPC_PARAM_ERR)
            } else {
                let result = process_memory_manager.dispose_native_ptr(instance);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                if result.is_some() {
                    0
                } else {
                    i32::from(PPC_PARAM_ERR)
                }
            }
        }
        PpcImportDispatcherTarget::ICBegin => i32::from((|| {
            let permission = cpu.gpr[4] as u8;
            let Some(instance) = state.instances.get(&cpu.gpr[3]) else {
                return PPC_PARAM_ERR;
            };
            if instance.permission != 0 || !matches!(permission, 1 | 2) {
                return PPC_PARAM_ERR;
            }
            if permission == 2
                && state.instances.iter().any(|(id, other)| {
                    *id != cpu.gpr[3]
                        && other.permission == 2
                        && other.path.eq_ignore_ascii_case(&instance.path)
                })
            {
                return -671;
            }
            ensure_config_file(vfs_files, vfs_resource_files, &instance.path);
            state.instances.get_mut(&cpu.gpr[3]).unwrap().permission = permission;
            PPC_NO_ERR
        })()),
        PpcImportDispatcherTarget::ICEnd => match state.instances.get_mut(&cpu.gpr[3]) {
            Some(instance) if instance.permission != 0 => {
                instance.permission = 0;
                0
            }
            _ => i32::from(PPC_PARAM_ERR),
        },
        PpcImportDispatcherTarget::ICFindConfigFile => i32::from((|| {
            let Some(instance) = state.instances.get(&cpu.gpr[3]) else {
                return PPC_PARAM_ERR;
            };
            let count = cpu.gpr[4] as u16 as i16;
            if instance.permission != 0 || count < 0 || (count != 0 && cpu.gpr[5] == 0) {
                return PPC_PARAM_ERR;
            }
            let mut folders = Vec::new();
            for index in 0..count as u32 {
                let Some(ptr) = cpu.gpr[5].checked_add(index * 6) else {
                    return PPC_PARAM_ERR;
                };
                let Some(bytes) = ppc_memory_read_bytes(memory, ptr, 6) else {
                    return PPC_PARAM_ERR;
                };
                let vref = i16::from_be_bytes(bytes[..2].try_into().unwrap());
                if ppc_resolve_volume_ref_num(vref) != PPC_BOOT_VOLUME_REF_NUM {
                    return PPC_NSV_ERR;
                }
                let id = ppc_resolve_directory_id(
                    vref,
                    u32::from_be_bytes(bytes[2..].try_into().unwrap()),
                    default_dir_id,
                );
                let Some(path) = ppc_directory_path_for_id(vfs_directories, id) else {
                    return PPC_DIR_NF_ERR;
                };
                folders.push(path.to_string());
            }
            let Some(preferences) =
                ppc_directory_path_for_id(vfs_directories, PPC_PREFERENCES_DIR_ID)
            else {
                return PPC_DIR_NF_ERR;
            };
            folders.push(preferences.to_string());
            let mut selected = None;
            for folder in &folders {
                let default = if folder.is_empty() {
                    "Internet Preferences".to_string()
                } else {
                    format!("{folder}/Internet Preferences")
                };
                selected = vfs_resource_files
                    .iter()
                    .find(|file| {
                        file.path.eq_ignore_ascii_case(&default)
                            && file.file_type == u32::from_be_bytes(*b"ICAp")
                    })
                    .or_else(|| {
                        vfs_resource_files.iter().find(|file| {
                            file.file_type == u32::from_be_bytes(*b"ICAp")
                                && file
                                    .path
                                    .rsplit_once('/')
                                    .map_or("", |(parent, _)| parent)
                                    .eq_ignore_ascii_case(folder)
                        })
                    })
                    .map(|file| file.path.clone());
                if selected.is_some() {
                    break;
                }
            }
            let path = selected.unwrap_or_else(|| format!("{preferences}/Internet Preferences"));
            state.instances.get_mut(&cpu.gpr[3]).unwrap().path = path;
            PPC_NO_ERR
        })()),
        PpcImportDispatcherTarget::ICSetPref => i32::from((|| {
            let Some(instance) = state.instances.get(&cpu.gpr[3]) else {
                return PPC_PARAM_ERR;
            };
            if instance.permission == 1 {
                return -667;
            }
            if instance.permission == 0
                && state.instances.iter().any(|(id, other)| {
                    *id != cpu.gpr[3]
                        && other.permission == 2
                        && other.path.eq_ignore_ascii_case(&instance.path)
                })
            {
                return -671;
            }
            let path = instance.path.clone();
            let Some(key) =
                ppc_read_pascal_string(memory, cpu.gpr[4]).filter(|key| !key.is_empty())
            else {
                return PPC_PARAM_ERR;
            };
            let buffer = cpu.gpr[6];
            let size = cpu.gpr[7] as i32;
            if buffer != 0 && size < 0 {
                return PPC_PARAM_ERR;
            }
            let new_bytes = if buffer == 0 {
                None
            } else {
                match ppc_memory_read_bytes(memory, buffer, size as u32) {
                    Some(bytes) => Some(bytes),
                    None => return PPC_PARAM_ERR,
                }
            };
            ppc_materialize_resource_records_for_path(vfs_resource_files, vfs_resources, &path);
            let profile = preference_profile(vfs_resources, &path);
            let index = vfs_resources.iter().position(|record| {
                record.path.eq_ignore_ascii_case(&path)
                    && record.res_type == profile
                    && crate::text_utils::evaluate_equal_string(&record.name, &key, false, true)
            });
            let old = index
                .and_then(|index| {
                    let data = vfs_resources[index].data.as_ref();
                    (data.len() >= 4).then(|| {
                        (
                            u32::from_be_bytes(data[..4].try_into().unwrap()),
                            data[4..].to_vec(),
                        )
                    })
                })
                .unwrap_or((0, Vec::new()));
            let attributes = if cpu.gpr[5] == u32::MAX {
                old.0
            } else {
                cpu.gpr[5]
            };
            if old.0 & attributes & 1 != 0 && buffer != 0 {
                return -667;
            }
            let mut data = attributes.to_be_bytes().to_vec();
            data.extend_from_slice(new_bytes.as_deref().unwrap_or(&old.1));
            if let Some(index) = index {
                let handle = vfs_resources[index].handle;
                if handle != 0 {
                    let error = process_memory_manager.set_native_handle_size(
                        memory,
                        handle,
                        data.len() as u32,
                    );
                    ppc_apply_process_native_allocator(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        last_mem_error,
                    );
                    if error != PPC_NO_ERR {
                        return error;
                    }
                    let Some(ptr) = memory.read_u32_be(handle) else {
                        return PPC_PARAM_ERR;
                    };
                    if memory.write_bytes(ptr, &data).is_none() {
                        return PPC_PARAM_ERR;
                    }
                }
                vfs_resources[index].data = data.into();
                vfs_resources[index].raw_data = None;
                vfs_resources[index].raw_attrs = None;
            } else {
                let Some(id) = (128..=i16::MAX).find(|id| {
                    !vfs_resources.iter().any(|record| {
                        record.path.eq_ignore_ascii_case(&path)
                            && record.res_type == profile
                            && record.res_id == *id
                    })
                }) else {
                    return PPC_MEM_FULL_ERR;
                };
                vfs_resources.push(PpcVfsResourceRecord {
                    ref_num: 0,
                    path: path.clone(),
                    res_type: profile,
                    res_id: id,
                    name: key,
                    data: data.into(),
                    raw_data: None,
                    raw_attrs: None,
                    attrs: 0,
                    handle: 0,
                });
            }
            ensure_config_file(vfs_files, vfs_resource_files, &path);
            ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
            ppc_publish_resource_fork_bytes(vfs_resource_files, vfs_resources, true);
            if attributes & 2 == 0 {
                if path.eq_ignore_ascii_case(DEFAULT_CONFIG_PATH) {
                    state.preferences_seed = state.preferences_seed.wrapping_add(1);
                } else {
                    let seed = state
                        .configuration_seeds
                        .entry(path.to_ascii_lowercase())
                        .or_insert(1);
                    *seed = seed.wrapping_add(1);
                }
            }
            PPC_NO_ERR
        })()),
        PpcImportDispatcherTarget::ICFindPrefHandle => i32::from((|| {
            let handle = cpu.gpr[6];
            let attr_ptr = cpu.gpr[5];
            if !process_memory_manager
                .native_handle_records()
                .iter()
                .any(|record| record.handle == handle)
            {
                return PPC_PARAM_ERR;
            }
            let saved_state = process_memory_manager.state_for_handle(handle).unwrap_or(0);
            process_memory_manager.set_state_for_handle(handle, saved_state & !0x80);
            let result = (|| {
                if attr_ptr == 0 || !ppc_memory_can_write_bytes(memory, attr_ptr, 4) {
                    return PPC_PARAM_ERR;
                }
                let _ = memory.write_u32_be(attr_ptr, u32::MAX);
                let Some(instance) = state.instances.get(&cpu.gpr[3]) else {
                    return PPC_PARAM_ERR;
                };
                let Some(key) =
                    ppc_read_pascal_string(memory, cpu.gpr[4]).filter(|key| !key.is_empty())
                else {
                    return PPC_PARAM_ERR;
                };
                ensure_config_file(vfs_files, vfs_resource_files, &instance.path);
                ppc_materialize_resource_records_for_path(
                    vfs_resource_files,
                    vfs_resources,
                    &instance.path,
                );
                let (attributes, bytes) =
                    match preference_bytes(vfs_resources, &instance.path, &key) {
                        Ok(value) => value,
                        Err(error) => return error,
                    };
                let _ = memory.write_u32_be(attr_ptr, attributes);
                let error = process_memory_manager.set_native_handle_size(
                    memory,
                    handle,
                    bytes.len() as u32,
                );
                if error != PPC_NO_ERR {
                    return error;
                }
                let Some(pointer) = memory.read_u32_be(handle) else {
                    return PPC_PARAM_ERR;
                };
                if !bytes.is_empty() && memory.write_bytes(pointer, bytes).is_none() {
                    return PPC_PARAM_ERR;
                }
                PPC_NO_ERR
            })();
            if result != PPC_NO_ERR {
                let _ = process_memory_manager.set_native_handle_size(memory, handle, 0);
            }
            process_memory_manager.set_state_for_handle(handle, saved_state);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            result
        })()),
        PpcImportDispatcherTarget::ICGetPref => i32::from((|| {
            let attr_ptr = cpu.gpr[5];
            let buffer = cpu.gpr[6];
            let size_ptr = cpu.gpr[7];
            if attr_ptr == 0
                || size_ptr == 0
                || !ppc_memory_can_write_bytes(memory, attr_ptr, 4)
                || !ppc_memory_can_write_bytes(memory, size_ptr, 4)
            {
                return PPC_PARAM_ERR;
            }
            let capacity = memory.read_u32_be(size_ptr).unwrap() as i32;
            let _ = memory.write_u32_be(attr_ptr, u32::MAX);
            let _ = memory.write_u32_be(size_ptr, 0);
            if !state.instances.contains_key(&cpu.gpr[3]) || (buffer != 0 && capacity < 0) {
                return PPC_PARAM_ERR;
            }
            let Some(key) =
                ppc_read_pascal_string(memory, cpu.gpr[4]).filter(|key| !key.is_empty())
            else {
                return PPC_PARAM_ERR;
            };
            let path = &state.instances[&cpu.gpr[3]].path;
            ensure_config_file(vfs_files, vfs_resource_files, path);
            ppc_materialize_resource_records_for_path(vfs_resource_files, vfs_resources, path);
            let (attributes, bytes) = match preference_bytes(vfs_resources, path, &key) {
                Ok(preference) => preference,
                Err(error) => return error,
            };
            let count = if buffer == 0 {
                0
            } else {
                bytes.len().min(capacity as usize)
            };
            if count != 0 && !ppc_memory_can_write_bytes(memory, buffer, count as u32) {
                return PPC_PARAM_ERR;
            }
            if count != 0 {
                let _ = memory.write_bytes(buffer, &bytes[..count]);
            }
            let _ = memory.write_u32_be(attr_ptr, attributes);
            let _ = memory.write_u32_be(size_ptr, bytes.len() as u32);
            if buffer != 0 && count < bytes.len() {
                -670
            } else {
                PPC_NO_ERR
            }
        })()),
        PpcImportDispatcherTarget::ICParseURL => i32::from((|| {
            let hint = if cpu.gpr[4] == 0 {
                Vec::new()
            } else if let Some(hint) = ppc_read_pascal_string(memory, cpu.gpr[4]) {
                hint
            } else {
                return PPC_PARAM_ERR;
            };
            let length = cpu.gpr[6];
            let start_ptr = cpu.gpr[7];
            let end_ptr = cpu.gpr[8];
            let handle = cpu.gpr[9];
            if !state.instances.contains_key(&cpu.gpr[3])
                || cpu.gpr[5] == 0
                || (length as i32) <= 0
                || start_ptr == 0
                || end_ptr == 0
                || !ppc_memory_can_write_bytes(memory, start_ptr, 4)
                || !ppc_memory_can_write_bytes(memory, end_ptr, 4)
                || !process_memory_manager
                    .native_handle_records()
                    .iter()
                    .any(|record| {
                        record.handle == handle
                            && record.ptr != 0
                            && memory.read_u32_be(handle) == Some(record.ptr)
                    })
            {
                return PPC_PARAM_ERR;
            }
            let Some(data) = ppc_memory_read_bytes(memory, cpu.gpr[5], length) else {
                return PPC_PARAM_ERR;
            };
            let mut start = memory.read_u32_be(start_ptr).unwrap() as usize;
            let mut end = memory.read_u32_be(end_ptr).unwrap() as usize;
            let parsed = parse_url_bytes(&data, &hint, &mut start, &mut end);
            let _ = memory.write_u32_be(start_ptr, start as u32);
            let _ = memory.write_u32_be(end_ptr, end as u32);
            let bytes = match parsed {
                Ok(bytes) => bytes,
                Err(error) => return error,
            };
            let result =
                process_memory_manager.set_native_handle_size(memory, handle, bytes.len() as u32);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if result != PPC_NO_ERR {
                return result;
            }
            let Some(pointer) = memory.read_u32_be(handle) else {
                return PPC_PARAM_ERR;
            };
            if memory.write_bytes(pointer, &bytes).is_none() {
                return PPC_PARAM_ERR;
            }
            PPC_NO_ERR
        })()),
        PpcImportDispatcherTarget::ICGetComponentInstance => {
            let output = cpu.gpr[4];
            if !state.instances.contains_key(&cpu.gpr[3])
                || output == 0
                || !ppc_memory_can_write_bytes(memory, output, 4)
            {
                i32::from(PPC_PARAM_ERR)
            } else {
                // InternetConfig.h: a glue implementation returns nil and
                // badComponentInstance, not a fabricated Component Manager ref.
                memory.write_u32_be(output, 0)?;
                0x8000_8001u32 as i32
            }
        }
        PpcImportDispatcherTarget::ICGetSeed => {
            let instance = cpu.gpr[3];
            let output = cpu.gpr[4];
            if !state.instances.contains_key(&instance)
                || output == 0
                || !ppc_memory_can_write_bytes(memory, output, 4)
            {
                i32::from(PPC_PARAM_ERR)
            } else {
                // This is a database seed, shared by all live instances, not
                // an instance identifier. It remains stable until preferences change.
                let path = &state.instances[&instance].path;
                let seed = if path.eq_ignore_ascii_case(DEFAULT_CONFIG_PATH) {
                    state.preferences_seed
                } else {
                    state
                        .configuration_seeds
                        .get(&path.to_ascii_lowercase())
                        .copied()
                        .unwrap_or(1)
                };
                memory.write_u32_be(output, seed)?;
                0
            }
        }
        _ => return None,
    };
    Some(PpcImportAction::Return(result as u32))
}

// Selection and normalization follow IC 2.0.2's public-domain ICURLHandling.p.
// Keep classic MacRoman bytes and CR wrapping semantics; this is not a modern
// URL validator, and the original preserves path and scheme capitalization.
fn matching_bracket(byte: u8) -> Option<u8> {
    Some(match byte {
        b'<' => b'>',
        b'>' => b'<',
        b'[' => b']',
        b']' => b'[',
        b'(' => b')',
        b')' => b'(',
        b'{' => b'}',
        b'}' => b'{',
        b'"' | b'\'' => byte,
        0xd4 => 0xd5,
        0xd5 => 0xd4,
        0xd2 => 0xd3,
        0xd3 => 0xd2,
        _ => return None,
    })
}
fn opening_bracket(byte: u8) -> bool {
    matches!(byte, b'<' | b'[' | b'(' | b'{' | b'"' | b'\'' | 0xd4 | 0xd2)
}
fn closing_bracket(byte: u8) -> bool {
    matches!(byte, b'>' | b']' | b')' | b'}' | b'"' | b'\'' | 0xd5 | 0xd3)
}
fn horizontal_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

pub(super) fn parse_url_bytes(
    data: &[u8],
    hint: &[u8],
    start: &mut usize,
    end: &mut usize,
) -> Result<Vec<u8>, i16> {
    const NO_URL: i16 = -673;
    if data.is_empty() || *start > *end || *end > data.len() {
        return Err(PPC_PARAM_ERR);
    }
    let explicit = *start != *end;
    if !explicit {
        while *start > 0 {
            let byte = data[*start - 1];
            if horizontal_space(byte) || byte == b'\r' || opening_bracket(byte) {
                break;
            }
            *start -= 1;
        }
        if *start > 0 && opening_bracket(data[*start - 1]) {
            *start -= 1;
        }
        while *end < data.len() {
            let byte = data[*end];
            if horizontal_space(byte) || byte == b'\r' || closing_bracket(byte) {
                break;
            }
            *end += 1;
        }
        if *end < data.len() && closing_bracket(data[*end]) {
            *end += 1;
        }
        if let Some(first) = data
            .get(*start)
            .copied()
            .filter(|byte| opening_bracket(*byte))
        {
            let matching = matching_bracket(first).unwrap();
            while *end > 0 && *end <= data.len() && data[*end - 1] != matching {
                *end += 1;
            }
            if *end == 0 || *end > data.len() {
                return Err(NO_URL);
            }
        }
        if *end > 0 && closing_bracket(data[*end - 1]) {
            let matching = matching_bracket(data[*end - 1]).unwrap();
            loop {
                if data.get(*start) == Some(&matching) {
                    break;
                }
                if *start == 0 {
                    return Err(NO_URL);
                }
                *start -= 1;
            }
        }
    }
    while *start < data.len() && horizontal_space(data[*start]) {
        *start += 1;
    }
    while *end > 0 && horizontal_space(data[*end - 1]) {
        *end -= 1;
    }
    if *start >= *end {
        return Err(NO_URL);
    }
    let selected = &data[*start..*end];
    let mut url = Vec::with_capacity(selected.len());
    let mut index = 0;
    while index < selected.len() {
        if selected[index] == b'\r' {
            while url.last().copied().is_some_and(horizontal_space) {
                url.pop();
            }
            while index < selected.len()
                && (horizontal_space(selected[index]) || selected[index] == b'\r')
            {
                index += 1;
            }
        }
        if index < selected.len() {
            url.push(selected[index]);
            index += 1;
        }
    }
    if url.is_empty() {
        return Err(NO_URL);
    }
    if url.len() >= 2 && opening_bracket(url[0]) && matching_bracket(url[0]) == url.last().copied()
    {
        url.pop();
        url.remove(0);
    } else if !explicit && url.last() == Some(&b'.') {
        url.pop();
    }
    if url
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"URL:"))
    {
        url.drain(..4);
    }
    if !url.iter().take(256).any(|byte| *byte == b':') {
        let prefix: Vec<u8> = if url
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"www."))
        {
            b"http://".to_vec()
        } else if hint.is_empty() {
            return Err(NO_URL);
        } else {
            let mut prefix = hint.to_vec();
            if !prefix.contains(&b':') {
                prefix.push(b':');
            }
            prefix
        };
        let mut normalized = prefix;
        normalized.extend_from_slice(&url);
        return Ok(normalized);
    }
    Ok(url)
}

const DEFAULT_CONFIG_PATH: &str = "System Folder/Preferences/Internet Preferences";

// IC 2.0.2 ICCIGetPref: the active profile is selected by CURP/128;
// old preference files without CURP use ICRP. Each named preference begins
// with its four-byte ICAttr, followed by the caller's uninterpreted bytes.
fn preference_bytes<'a>(
    resources: &'a [PpcVfsResourceRecord],
    path: &str,
    key: &[u8],
) -> Result<(u32, &'a [u8]), i16> {
    let profile = preference_profile(resources, path);
    let record = resources
        .iter()
        .find(|record| {
            record.path.eq_ignore_ascii_case(path)
                && record.res_type == profile
                && crate::text_utils::evaluate_equal_string(&record.name, key, false, true)
        })
        .ok_or(-666i16)?;
    let data: &[u8] = record.data.as_ref();
    let attributes = data.get(..4).ok_or(-668i16)?;
    Ok((
        u32::from_be_bytes(attributes.try_into().unwrap()),
        &data[4..],
    ))
}

fn ensure_config_file(
    files: &mut ProcessVfsFileRecords,
    forks: &mut ProcessVfsResourceFileRecords,
    path: &str,
) {
    if !files
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case(path))
    {
        files.push(PpcVfsFileRecord {
            path: path.into(),
            data: Vec::new().into(),
            creator: u32::from_be_bytes(*b"ICAp"),
            file_type: u32::from_be_bytes(*b"ICAp"),
            finder_flags: 0,
            dirty: true,
        });
    }
    if !forks
        .iter()
        .any(|file| file.path.eq_ignore_ascii_case(path))
    {
        forks.push(PpcVfsResourceFileRecord {
            path: path.into(),
            creator: u32::from_be_bytes(*b"ICAp"),
            file_type: u32::from_be_bytes(*b"ICAp"),
            finder_flags: 0,
            resource_len: 0,
            raw_data: None,
            map_attrs: 0,
            dirty: true,
        });
    }
}

fn preference_profile(resources: &[PpcVfsResourceRecord], path: &str) -> u32 {
    resources
        .iter()
        .find(|record| {
            record.path.eq_ignore_ascii_case(path)
                && record.res_type == u32::from_be_bytes(*b"CURP")
                && record.res_id == 128
        })
        .and_then(|record| {
            let bytes: [u8; 4] = record.data.as_ref().try_into().ok()?;
            (bytes[..2] == *b"IC"
                && (b'R'..=b'T').contains(&bytes[2])
                && (b'P'..=b'Z').contains(&bytes[3]))
            .then_some(u32::from_be_bytes(bytes))
        })
        .unwrap_or(u32::from_be_bytes(*b"ICRP"))
}
