//! Process-owned Internet Config instances for Carbon applications.

use super::*;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcInternetConfigState {
    instances: BTreeMap<u32, u32>,
    // InternetConfig.h: IC 2.5 automatically selects the default configuration.
    // The empty default preferences set starts at an opaque, stable seed.
    pub(super) preferences_seed: u32,
}

impl Default for PpcInternetConfigState {
    fn default() -> Self {
        Self {
            instances: BTreeMap::new(),
            preferences_seed: 1,
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
) -> Option<PpcImportAction> {
    let state = &mut toolbox_startup.internet_config;
    // Apple InternetConfig.h: ICInstance is an opaque, process-local pointer.
    // ICStart(ICInstance *, OSType), ICStop(ICInstance), and
    // ICGetSeed(ICInstance, long *) return a 32-bit OSStatus in r3.
    let result: i32 = match binding.dispatcher_target {
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
                        state.instances.insert(instance, signature);
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
                memory.write_u32_be(output, state.preferences_seed)?;
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
