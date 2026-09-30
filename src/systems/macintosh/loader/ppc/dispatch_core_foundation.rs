//! Core Foundation imports used by Carbon CFM applications.

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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpcCfBundle {
    reference: u32,
    retain_count: u32,
    path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpcCfUrl {
    path: String,
    is_directory: bool,
    retain_count: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PpcCfStringState {
    objects: BTreeMap<u32, PpcCfString>,
    constants: BTreeMap<Vec<u8>, u32>,
    loaded_bundles: BTreeMap<String, u32>,
    main_bundle: Option<PpcCfBundle>,
    bundles: BTreeMap<u32, PpcCfBundle>,
    urls: BTreeMap<u32, PpcCfUrl>,
}

impl PpcCfStringState {
    #[cfg(test)]
    pub(super) fn url_path(&self, reference: u32) -> Option<&str> {
        self.urls.get(&reference).map(|url| url.path.as_str())
    }

    #[cfg(test)]
    pub(super) fn url_is_directory(&self, reference: u32) -> Option<bool> {
        self.urls.get(&reference).map(|url| url.is_directory)
    }

    fn create_url(
        &mut self,
        path: String,
        is_directory: bool,
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
        if reference != 0 {
            self.urls.insert(
                reference,
                PpcCfUrl {
                    path,
                    is_directory,
                    retain_count: 1,
                },
            );
        }
        reference
    }

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

fn bundle_info<'a>(bundle_path: &str, files: &'a ProcessVfsFileRecords) -> Option<&'a [u8]> {
    let info_path = format!("{bundle_path}/Contents/Info.plist");
    Some(
        files
            .iter()
            .find(|file| file.path.eq_ignore_ascii_case(&info_path))?
            .data
            .as_ref(),
    )
}

fn bundle_identifier(bundle_path: &str, files: &ProcessVfsFileRecords) -> Option<String> {
    bundle_info_string(bundle_path, files, "CFBundleIdentifier")
}

fn bundle_info_string(
    bundle_path: &str,
    files: &ProcessVfsFileRecords,
    key_name: &str,
) -> Option<String> {
    let text = std::str::from_utf8(bundle_info(bundle_path, files)?).ok()?;
    let key_tag = format!("<key>{key_name}</key>");
    let key = text.find(&key_tag)?;
    let following = &text[key + key_tag.len()..];
    let start = following.find("<string>")? + "<string>".len();
    let value = &following[start..];
    let end = value.find("</string>")?;
    let result = value[..end].trim();
    (!result.is_empty() && !result.contains('&')).then(|| result.to_string())
}

fn main_bundle_identifier(path: &str, files: &ProcessVfsFileRecords) -> Option<String> {
    bundle_identifier(bundle_root(path)?, files)
}

fn bundle_root(path: &str) -> Option<&str> {
    path.split_once("/Contents/").map(|(root, _)| root)
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
    vfs_files: &ProcessVfsFileRecords,
    launched_app_path: Option<&str>,
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
            } else if let Some(bundle) = state
                .main_bundle
                .as_mut()
                .filter(|bundle| bundle.reference == cpu.gpr[3])
            {
                bundle.retain_count = bundle.retain_count.saturating_add(1);
                Some(PpcImportAction::Return(bundle.reference))
            } else if let Some(url) = state.urls.get_mut(&cpu.gpr[3]) {
                url.retain_count = url.retain_count.saturating_add(1);
                Some(PpcImportAction::Return(cpu.gpr[3]))
            } else if let Some(bundle) = state.bundles.get_mut(&cpu.gpr[3]) {
                bundle.retain_count = bundle.retain_count.saturating_add(1);
                Some(PpcImportAction::Return(cpu.gpr[3]))
            } else {
                Some(PpcImportAction::Return(0))
            }
        }
        PpcImportDispatcherTarget::CfRelease => {
            if let Some(bundle) = state
                .main_bundle
                .as_mut()
                .filter(|bundle| bundle.reference == cpu.gpr[3])
            {
                bundle.retain_count = bundle.retain_count.saturating_sub(1).max(1);
            } else if let Some(url) = state.urls.get_mut(&cpu.gpr[3]) {
                url.retain_count -= 1;
                if url.retain_count == 0 {
                    state.urls.remove(&cpu.gpr[3]);
                    let _ = process_memory_manager.dispose_native_ptr(cpu.gpr[3]);
                    ppc_apply_process_native_allocator(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        last_mem_error,
                    );
                }
            } else if let Some(bundle) = state.bundles.get_mut(&cpu.gpr[3]) {
                bundle.retain_count -= 1;
                if bundle.retain_count == 0 {
                    state.bundles.remove(&cpu.gpr[3]);
                    state
                        .loaded_bundles
                        .retain(|_, reference| *reference != cpu.gpr[3]);
                    let _ = process_memory_manager.dispose_native_ptr(cpu.gpr[3]);
                    ppc_apply_process_native_allocator(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        last_mem_error,
                    );
                }
            } else {
                state.release(
                    cpu.gpr[3],
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CfGetRetainCount => Some(PpcImportAction::Return(
            state
                .objects
                .get(&cpu.gpr[3])
                .map(|object| object.retain_count)
                .or_else(|| {
                    state
                        .main_bundle
                        .as_ref()
                        .filter(|bundle| bundle.reference == cpu.gpr[3])
                        .map(|bundle| bundle.retain_count)
                })
                .or_else(|| state.urls.get(&cpu.gpr[3]).map(|url| url.retain_count))
                .or_else(|| {
                    state
                        .bundles
                        .get(&cpu.gpr[3])
                        .map(|bundle| bundle.retain_count)
                })
                .unwrap_or(0),
        )),
        PpcImportDispatcherTarget::CfBundleGetMainBundle => {
            if let Some(bundle) = &state.main_bundle {
                return Some(PpcImportAction::Return(bundle.reference));
            }
            let Some(path) = launched_app_path.filter(|path| !path.is_empty()) else {
                return Some(PpcImportAction::Return(0));
            };
            let reference =
                process_memory_manager.new_native_ptr(memory, CF_STRING_OBJECT_SIZE, true);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if reference == 0 {
                return Some(PpcImportAction::Return(0));
            }
            if let Some(identifier) = main_bundle_identifier(path, vfs_files) {
                state.loaded_bundles.insert(identifier, reference);
            }
            state.main_bundle = Some(PpcCfBundle {
                reference,
                retain_count: 1,
                path: path.to_string(),
            });
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl => {
            // CFBundleCopyPrivateFrameworksURL returns a caller-owned CFURL
            // for Contents/Frameworks, or NULL if that directory is absent.
            // Apple Core Foundation CFBundle Reference, Finding Locations in a Bundle.
            let root = state
                .main_bundle
                .as_ref()
                .filter(|bundle| bundle.reference == cpu.gpr[3])
                .and_then(|bundle| bundle_root(&bundle.path))
                .or_else(|| {
                    state
                        .bundles
                        .get(&cpu.gpr[3])
                        .map(|bundle| bundle.path.as_str())
                });
            let Some(root) = root else {
                return Some(PpcImportAction::Return(0));
            };
            let path = format!("{root}/Contents/Frameworks");
            let prefix = format!("{path}/");
            if !vfs_files.iter().any(|file| {
                file.path.eq_ignore_ascii_case(&path)
                    || file
                        .path
                        .get(..prefix.len())
                        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(&prefix))
            }) {
                return Some(PpcImportAction::Return(0));
            }
            let reference = state.create_url(
                path,
                true,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent => {
            // CFURLCreateCopyAppendingPathComponent(allocator, url,
            // pathComponent, isDirectory) returns an owned URL. Directory
            // URLs retain their directory status for relative resolution.
            // Apple Core Foundation CFURL Reference, Creating a CFURL.
            let Some(base) = state.urls.get(&cpu.gpr[4]) else {
                return Some(PpcImportAction::Return(0));
            };
            let Some(component) = state.objects.get(&cpu.gpr[5]) else {
                return Some(PpcImportAction::Return(0));
            };
            let component = component.value.as_str();
            let path = if component.is_empty() {
                base.path.clone()
            } else {
                format!("{}/{}", base.path.trim_end_matches('/'), component)
            };
            let reference = state.create_url(
                path,
                cpu.gpr[6] != 0,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfBundleCreate => {
            // CFBundleCreate(allocator, bundleURL) creates an owned bundle
            // from a directory URL. Bundle metadata comes from Info.plist.
            // Apple Core Foundation Bundle Programming Guide, Accessing a Bundle's Contents.
            let Some(path) = state
                .urls
                .get(&cpu.gpr[4])
                .filter(|url| url.is_directory)
                .map(|url| url.path.clone())
            else {
                return Some(PpcImportAction::Return(0));
            };
            let Some(info) = bundle_info(&path, vfs_files) else {
                return Some(PpcImportAction::Return(0));
            };
            let valid = info.starts_with(b"bplist00")
                || std::str::from_utf8(info)
                    .ok()
                    .is_some_and(|text| text.contains("<plist"));
            if !valid {
                return Some(PpcImportAction::Return(0));
            }
            if let Some(bundle) = state.main_bundle.as_mut().filter(|bundle| {
                bundle_root(&bundle.path).is_some_and(|root| root.eq_ignore_ascii_case(&path))
            }) {
                bundle.retain_count = bundle.retain_count.saturating_add(1);
                return Some(PpcImportAction::Return(bundle.reference));
            }
            if let Some(bundle) = state
                .bundles
                .values_mut()
                .find(|bundle| bundle.path.eq_ignore_ascii_case(&path))
            {
                bundle.retain_count = bundle.retain_count.saturating_add(1);
                return Some(PpcImportAction::Return(bundle.reference));
            }
            let reference =
                process_memory_manager.new_native_ptr(memory, CF_STRING_OBJECT_SIZE, true);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if reference == 0 {
                return Some(PpcImportAction::Return(0));
            }
            if let Some(identifier) = bundle_identifier(&path, vfs_files) {
                state.loaded_bundles.insert(identifier, reference);
            }
            state.bundles.insert(
                reference,
                PpcCfBundle {
                    reference,
                    retain_count: 1,
                    path,
                },
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfBundleLoadExecutable => {
            // CFBundleLoadExecutable returns true only after loading and
            // dynamically linking executable code. The classic CFM guest
            // cannot load an OS X Mach-O image, even when it shares the PPC
            // CPU type. Apple Core Foundation CFBundle Reference (2007),
            // "CFBundleLoadExecutable"; Carbon Porting Guide (2002),
            // "Preparing Your Code in OS X".
            let Some(bundle) = state.bundles.get(&cpu.gpr[3]).or_else(|| {
                state
                    .main_bundle
                    .as_ref()
                    .filter(|bundle| bundle.reference == cpu.gpr[3])
            }) else {
                return Some(PpcImportAction::Return(0));
            };
            let Some(executable) = bundle_info_string(&bundle.path, vfs_files, "CFBundleExecutable")
                .filter(|name| !name.contains(['/', ':']) && name != "." && name != "..")
            else {
                return Some(PpcImportAction::Return(0));
            };
            let executable_path = format!("{}/Contents/MacOS/{executable}", bundle.path);
            let Some(bytes) = vfs_files
                .iter()
                .find(|file| file.path.eq_ignore_ascii_case(&executable_path))
                .map(|file| file.data.as_ref())
            else {
                return Some(PpcImportAction::Return(0));
            };
            if matches!(
                bytes.get(..4),
                Some([0xfe, 0xed, 0xfa, 0xce] | [0xce, 0xfa, 0xed, 0xfe] | [0xca, 0xfe, 0xba, 0xbe])
            ) {
                return Some(PpcImportAction::Return(0));
            }
            None
        }
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
