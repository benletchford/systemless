//! Core Foundation string imports used by Carbon CFM applications.

use super::*;
use std::collections::BTreeMap;

// CoreFoundation/CFString.h: CFStringBuiltInEncodings.
const CF_STRING_ENCODING_MAC_ROMAN: u32 = 0;
const CF_STRING_ENCODING_ASCII: u32 = 0x0600;
const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const CF_STRING_OBJECT_SIZE: u32 = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpcCfString {
    value: String,
    retain_count: u32,
    constant: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PpcCfStringState {
    objects: BTreeMap<u32, PpcCfString>,
    constants: BTreeMap<Vec<u8>, u32>,
    loaded_bundles: BTreeMap<String, u32>,
}

impl PpcCfStringState {
    fn create(
        &mut self,
        value: String,
        constant: bool,
        process_memory_manager: &mut ProcessNativeMemoryManager,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        last_mem_error: &mut i16,
    ) -> u32 {
        let reference = process_memory_manager.new_native_ptr(memory, CF_STRING_OBJECT_SIZE, true);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
        if reference == 0 {
            return 0;
        }
        if memory
            .write_bytes(reference, &[0; CF_STRING_OBJECT_SIZE as usize])
            .is_none()
        {
            let _ = process_memory_manager.dispose_native_ptr(reference);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            return 0;
        }
        self.objects.insert(
            reference,
            PpcCfString {
                value,
                retain_count: 1,
                constant,
            },
        );
        reference
    }

    fn release(
        &mut self,
        reference: u32,
        process_memory_manager: &mut ProcessNativeMemoryManager,
        memory: &mut PpcSectionMem,
        heap_cursor: &mut u32,
        last_mem_error: &mut i16,
    ) {
        let Some(object) = self.objects.get_mut(&reference) else {
            return;
        };
        if object.constant && object.retain_count == 1 {
            return;
        }
        object.retain_count -= 1;
        if object.retain_count == 0 {
            self.objects.remove(&reference);
            let _ = process_memory_manager.dispose_native_ptr(reference);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
        }
    }
}

fn read_c_bytes(memory: &mut PpcSectionMem, address: u32) -> Option<Vec<u8>> {
    if address == 0 {
        return None;
    }
    let mut bytes = Vec::new();
    for offset in 0..1_048_576_u32 {
        let byte = memory.read_u8(address.checked_add(offset)?)?;
        if byte == 0 {
            return Some(bytes);
        }
        bytes.push(byte);
    }
    None
}

fn read_guest_bytes(memory: &mut PpcSectionMem, address: u32, length: u32) -> Option<Vec<u8>> {
    if (length > 0 && address == 0) || length > 1_048_576 {
        return None;
    }
    (0..length)
        .map(|offset| memory.read_u8(address.checked_add(offset)?))
        .collect()
}

fn decode_bytes(bytes: &[u8], encoding: u32) -> Option<String> {
    match encoding {
        CF_STRING_ENCODING_MAC_ROMAN => Some(crate::mac_roman::decode_mac_roman(bytes)),
        // CFString.h: high bytes in the ASCII encoding map to their
        // corresponding Unicode scalar values when creating a string.
        CF_STRING_ENCODING_ASCII => Some(bytes.iter().map(|&byte| char::from(byte)).collect()),
        CF_STRING_ENCODING_UTF8 => String::from_utf8(bytes.to_vec()).ok(),
        _ => None,
    }
}

fn encode_bytes(value: &str, encoding: u32) -> Option<Vec<u8>> {
    match encoding {
        CF_STRING_ENCODING_MAC_ROMAN => value
            .chars()
            .map(crate::mac_roman::encode_mac_roman_char)
            .collect(),
        CF_STRING_ENCODING_ASCII => value
            .chars()
            .map(|ch| ch.is_ascii().then_some(ch as u8))
            .collect(),
        CF_STRING_ENCODING_UTF8 => Some(value.as_bytes().to_vec()),
        _ => None,
    }
}

pub(super) fn dispatch_core_foundation_import(
    binding: &PpcImportBinding,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Option<PpcImportAction> {
    let state = &mut toolbox_startup.cf_strings;
    match binding.dispatcher_target {
        PpcImportDispatcherTarget::CfStringMakeConstantString => {
            // CFString.h: CFSTR returns a permanent, unowned reference.
            // Apple's CFString.c decodes this C string with MacRoman.
            let Some(bytes) = read_c_bytes(memory, cpu.gpr[3]) else {
                return Some(PpcImportAction::Return(0));
            };
            if let Some(&reference) = state.constants.get(&bytes) {
                return Some(PpcImportAction::Return(reference));
            }
            let value = crate::mac_roman::decode_mac_roman(&bytes);
            let reference = state.create(
                value,
                true,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if reference != 0 {
                state.constants.insert(bytes, reference);
            }
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfStringCreateWithCString => {
            // CFStringCreateWithCString(alloc, cStr, encoding) returns an
            // owned reference. The allocator does not change the string's
            // contents; guest references use process-owned native storage.
            let value =
                read_c_bytes(memory, cpu.gpr[4]).and_then(|bytes| decode_bytes(&bytes, cpu.gpr[5]));
            Some(PpcImportAction::Return(value.map_or(0, |value| {
                state.create(
                    value,
                    false,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                )
            })))
        }
        PpcImportDispatcherTarget::CfStringCreateWithPascalString => {
            // CFStringCreateWithPascalString(alloc, pStr, encoding).
            if cpu.gpr[4] == 0 {
                return Some(PpcImportAction::Return(0));
            }
            let value = memory.read_u8(cpu.gpr[4]).and_then(|length| {
                let bytes = (0..u32::from(length))
                    .map(|offset| memory.read_u8(cpu.gpr[4].checked_add(offset + 1)?))
                    .collect::<Option<Vec<_>>>()?;
                decode_bytes(&bytes, cpu.gpr[5])
            });
            Some(PpcImportAction::Return(value.map_or(0, |value| {
                state.create(
                    value,
                    false,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                )
            })))
        }
        PpcImportDispatcherTarget::CfStringCreateWithBytes => {
            // CFStringCreateWithBytes(alloc, bytes, numBytes, encoding,
            // isExternalRepresentation) copies the input into an owned string.
            let value = ((cpu.gpr[5] as i32) >= 0)
                .then(|| read_guest_bytes(memory, cpu.gpr[4], cpu.gpr[5]))
                .flatten()
                .and_then(|bytes| {
                    let bytes = if cpu.gpr[7] != 0
                        && cpu.gpr[6] == CF_STRING_ENCODING_UTF8
                        && bytes.starts_with(&[0xEF, 0xBB, 0xBF])
                    {
                        &bytes[3..]
                    } else {
                        &bytes
                    };
                    decode_bytes(bytes, cpu.gpr[6])
                });
            Some(PpcImportAction::Return(value.map_or(0, |value| {
                state.create(
                    value,
                    false,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                )
            })))
        }
        PpcImportDispatcherTarget::CfStringGetLength => {
            // CFString.h: length is the number of 16-bit Unicode code units.
            let length = state
                .objects
                .get(&cpu.gpr[3])
                .map_or(0, |object| object.value.encode_utf16().count() as u32);
            Some(PpcImportAction::Return(length))
        }
        PpcImportDispatcherTarget::CfStringGetCString => {
            // CFStringGetCString(str, buffer, bufferSize, encoding) writes
            // only a complete, zero-terminated conversion.
            let success = state
                .objects
                .get(&cpu.gpr[3])
                .and_then(|object| encode_bytes(&object.value, cpu.gpr[6]))
                .filter(|bytes| {
                    (cpu.gpr[5] as i32) > 0
                        && bytes
                            .len()
                            .checked_add(1)
                            .is_some_and(|size| size <= cpu.gpr[5] as usize)
                })
                .is_some_and(|mut bytes| {
                    bytes.push(0);
                    ppc_memory_can_write_bytes(memory, cpu.gpr[4], bytes.len() as u32)
                        && memory.write_bytes(cpu.gpr[4], &bytes).is_some()
                });
            Some(PpcImportAction::Return(u32::from(success)))
        }
        PpcImportDispatcherTarget::CfStringGetBytes => {
            // CFStringGetBytes(str, range, encoding, lossByte,
            // isExternalRepresentation, buffer, maxBufLen, usedBufLen).
            // Inside Macintosh: PowerPC System Software (1994), p. 1-29:
            // CFRange's two words occupy GPR4/GPR5; the ninth word is in
            // the caller's parameter area after GPR10.
            let used_buf_len = cpu.gpr[1]
                .checked_add(PPC_PARAMETER_AREA_OFFSET + 8 * 4)
                .and_then(|address| memory.read_u32_be(address))
                .unwrap_or(0);
            if !matches!(
                cpu.gpr[6],
                CF_STRING_ENCODING_MAC_ROMAN | CF_STRING_ENCODING_ASCII | CF_STRING_ENCODING_UTF8
            ) {
                if used_buf_len != 0 {
                    let _ = memory.write_u32_be(used_buf_len, 0);
                }
                return Some(PpcImportAction::Return(0));
            }
            let mut bytes = Vec::new();
            let mut converted = 0_u32;
            if let Some(object) = state.objects.get(&cpu.gpr[3]) {
                let units: Vec<u16> = object.value.encode_utf16().collect();
                let start = cpu.gpr[4] as i32;
                let length = cpu.gpr[5] as i32;
                if start >= 0 && length >= 0 {
                    let start = start as usize;
                    let end = start.checked_add(length as usize);
                    if let Some(range) = end.and_then(|end| units.get(start..end)) {
                        if cpu.gpr[8] != 0 && cpu.gpr[6] == CF_STRING_ENCODING_UTF8 {
                            bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
                        }
                        if cpu.gpr[9] != 0 && bytes.len() > (cpu.gpr[10] as i32).max(0) as usize {
                            bytes.clear();
                            if used_buf_len != 0 {
                                let _ = memory.write_u32_be(used_buf_len, 0);
                            }
                            return Some(PpcImportAction::Return(0));
                        }
                        for decoded in char::decode_utf16(range.iter().copied()) {
                            let Ok(ch) = decoded else {
                                break;
                            };
                            let encoded = encode_bytes(&ch.to_string(), cpu.gpr[6]).or_else(|| {
                                (cpu.gpr[7] != 0 && cpu.gpr[6] != CF_STRING_ENCODING_UTF8)
                                    .then_some(vec![cpu.gpr[7] as u8])
                            });
                            let Some(encoded) = encoded else {
                                break;
                            };
                            if cpu.gpr[9] != 0
                                && bytes.len().saturating_add(encoded.len())
                                    > (cpu.gpr[10] as i32).max(0) as usize
                            {
                                break;
                            }
                            bytes.extend_from_slice(&encoded);
                            converted += ch.len_utf16() as u32;
                        }
                    }
                }
            }
            if cpu.gpr[9] != 0
                && (!ppc_memory_can_write_bytes(memory, cpu.gpr[9], bytes.len() as u32)
                    || memory.write_bytes(cpu.gpr[9], &bytes).is_none())
            {
                return Some(PpcImportAction::Return(0));
            }
            if used_buf_len != 0 {
                let _ = memory.write_u32_be(used_buf_len, bytes.len() as u32);
            }
            Some(PpcImportAction::Return(converted))
        }
        PpcImportDispatcherTarget::CfStringGetSystemEncoding => {
            // CFStringGetSystemEncoding in classic Mac Roman environments.
            Some(PpcImportAction::Return(CF_STRING_ENCODING_MAC_ROMAN))
        }
        PpcImportDispatcherTarget::CfRetain => {
            if let Some(object) = state.objects.get_mut(&cpu.gpr[3]) {
                object.retain_count = object.retain_count.saturating_add(1);
                Some(PpcImportAction::Return(cpu.gpr[3]))
            } else {
                Some(PpcImportAction::Return(0))
            }
        }
        PpcImportDispatcherTarget::CfRelease => {
            state.release(
                cpu.gpr[3],
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CfGetRetainCount => Some(PpcImportAction::Return(
            state
                .objects
                .get(&cpu.gpr[3])
                .map_or(0, |object| object.retain_count),
        )),
        PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier => {
            // CFBundleGetBundleWithIdentifier only searches bundle objects
            // already loaded into this process. A CFM import does not by
            // itself load or identify a CFBundle.
            let reference = state
                .objects
                .get(&cpu.gpr[3])
                .and_then(|identifier| state.loaded_bundles.get(&identifier.value))
                .copied()
                .unwrap_or(0);
            Some(PpcImportAction::Return(reference))
        }
        _ => None,
    }
}
