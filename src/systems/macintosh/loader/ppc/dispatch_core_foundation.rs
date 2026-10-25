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
    /// For file URLs on the boot volume, the VFS path. Paths outside the boot
    /// volume keep their absolute POSIX form, which begins with `/` and so
    /// never names a VFS entry.
    path: String,
    is_directory: bool,
    retain_count: u32,
    /// The URL string of a URL whose scheme is not `file`.
    non_file: Option<String>,
}

// CoreFoundation/CFNumber.h: CFNumberType.
const CF_NUMBER_SINT8_TYPE: u32 = 1;
const CF_NUMBER_SINT16_TYPE: u32 = 2;
const CF_NUMBER_SINT32_TYPE: u32 = 3;
const CF_NUMBER_SINT64_TYPE: u32 = 4;
const CF_NUMBER_FLOAT32_TYPE: u32 = 5;
const CF_NUMBER_FLOAT64_TYPE: u32 = 6;
const CF_NUMBER_CHAR_TYPE: u32 = 7;
const CF_NUMBER_SHORT_TYPE: u32 = 8;
const CF_NUMBER_INT_TYPE: u32 = 9;
const CF_NUMBER_LONG_TYPE: u32 = 10;
const CF_NUMBER_LONG_LONG_TYPE: u32 = 11;
const CF_NUMBER_FLOAT_TYPE: u32 = 12;
const CF_NUMBER_DOUBLE_TYPE: u32 = 13;
const CF_NUMBER_CF_INDEX_TYPE: u32 = 14;

// CoreFoundation/CFURL.h: CFURLPathStyle.
const CF_URL_POSIX_PATH_STYLE: u32 = 0;
const CF_URL_HFS_PATH_STYLE: u32 = 1;

/// A CFNumber's stored value. Floating-point values keep their IEEE bits so
/// the state stays `Eq`. No supported import creates CFNumber, CFArray, or
/// CFDictionary objects yet, so the accessors find none outside tests.
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PpcCfNumberValue {
    Integer(i64),
    Float(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpcCfArray {
    values: Vec<u32>,
    retain_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpcCfDictionary {
    entries: Vec<(u32, u32)>,
    retain_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PpcCfNumber {
    value: PpcCfNumberValue,
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
    arrays: BTreeMap<u32, PpcCfArray>,
    dictionaries: BTreeMap<u32, PpcCfDictionary>,
    numbers: BTreeMap<u32, PpcCfNumber>,
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

    #[cfg(test)]
    pub(super) fn insert_array(&mut self, reference: u32, values: Vec<u32>) {
        let array = PpcCfArray {
            values,
            retain_count: 1,
        };
        self.arrays.insert(reference, array);
    }

    #[cfg(test)]
    pub(super) fn insert_dictionary(&mut self, reference: u32, entries: Vec<(u32, u32)>) {
        let dictionary = PpcCfDictionary {
            entries,
            retain_count: 1,
        };
        self.dictionaries.insert(reference, dictionary);
    }

    #[cfg(test)]
    pub(super) fn insert_number(&mut self, reference: u32, value: PpcCfNumberValue) {
        let number = PpcCfNumber {
            value,
            retain_count: 1,
        };
        self.numbers.insert(reference, number);
    }

    /// The retain count of an array, dictionary, or number object.
    fn collection_retain_count(&mut self, reference: u32) -> Option<&mut u32> {
        if let Some(array) = self.arrays.get_mut(&reference) {
            return Some(&mut array.retain_count);
        }
        if let Some(dictionary) = self.dictionaries.get_mut(&reference) {
            return Some(&mut dictionary.retain_count);
        }
        self.numbers
            .get_mut(&reference)
            .map(|number| &mut number.retain_count)
    }

    /// Whether two dictionary keys are equal. CFDictionaryGetValue compares
    /// keys with the key callbacks' equality, which for CFType keys is
    /// CFEqual: CFStrings are equal when their characters are equal.
    fn keys_equal(&self, left: u32, right: u32) -> bool {
        if left == right {
            return true;
        }
        match (self.objects.get(&left), self.objects.get(&right)) {
            (Some(left), Some(right)) => left.value == right.value,
            _ => false,
        }
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
        let url = PpcCfUrl {
            path,
            is_directory,
            retain_count: 1,
            non_file: None,
        };
        self.insert_url(
            url,
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        )
    }

    fn insert_url(
        &mut self,
        url: PpcCfUrl,
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
            self.urls.insert(reference, url);
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

/// Joins path components, resolving `.` and `..`, onto `base`.
fn normalize_components<'a>(base: Vec<&'a str>, path: &'a str) -> Vec<&'a str> {
    let mut components = base;
    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                components.pop();
            }
            component => components.push(component),
        }
    }
    components
}

/// Maps components of an absolute POSIX path to a URL path. CarbonLib names
/// mounted volumes as top-level POSIX directories, so `/<boot volume>/a`
/// names VFS entry `a`.
fn url_path_from_posix_components(components: &[&str]) -> String {
    let volume = crate::trap::dispatch::BOOT_VOLUME_NAME;
    match components.split_first() {
        Some((first, rest)) if first.eq_ignore_ascii_case(volume) => rest.join("/"),
        _ => format!("/{}", components.join("/")),
    }
}

/// The POSIX components of a URL path.
fn posix_components(path: &str) -> Vec<&str> {
    match path.strip_prefix('/') {
        Some(outside) => outside.split('/').filter(|part| !part.is_empty()).collect(),
        None => std::iter::once(crate::trap::dispatch::BOOT_VOLUME_NAME)
            .chain(path.split('/').filter(|part| !part.is_empty()))
            .collect(),
    }
}

/// Converts a file system path in `style` to a URL path. Relative paths
/// resolve against `base`, a URL path naming a directory.
fn url_path_from_file_system_path(path: &str, style: u32, base: &str) -> Option<String> {
    match style {
        CF_URL_POSIX_PATH_STYLE => {
            let base = if path.starts_with('/') {
                Vec::new()
            } else {
                posix_components(base)
            };
            let components = normalize_components(base, path);
            Some(url_path_from_posix_components(&components))
        }
        CF_URL_HFS_PATH_STYLE => {
            // An HFS path is absolute when it contains a colon that is not
            // its first character; its first component names the volume.
            // Each extra leading colon of a relative path names a parent.
            let absolute = path.find(':').is_some_and(|index| index > 0);
            let mut components = if absolute {
                Vec::new()
            } else {
                posix_components(base)
            };
            let relative = if absolute {
                path
            } else {
                path.strip_prefix(':').unwrap_or(path)
            };
            let mut parts = relative.split(':').peekable();
            while let Some(part) = parts.next() {
                if !part.is_empty() {
                    components.push(part);
                } else if parts.peek().is_some() {
                    components.pop();
                }
            }
            Some(url_path_from_posix_components(&components))
        }
        // The Windows path style names no Mac OS volume.
        _ => None,
    }
}

/// Converts a URL path to a file system path in `style`, without a trailing
/// separator except after a bare HFS volume name.
fn file_system_path_from_url_path(path: &str, style: u32) -> Option<String> {
    let components = posix_components(path);
    match style {
        CF_URL_POSIX_PATH_STYLE => Some(format!("/{}", components.join("/"))),
        CF_URL_HFS_PATH_STYLE => match components.as_slice() {
            [] => None,
            [volume] => Some(format!("{volume}:")),
            components => Some(components.join(":")),
        },
        _ => None,
    }
}

/// Decodes `%XX` escapes as UTF-8, the encoding CFURL uses for file paths.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            decoded.push(u8::from_str_radix(hex, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// Whether `text` uses only characters RFC 2396 allows in a URL reference.
/// CFURLCreateWithString returns NULL for strings that need escaping.
fn is_url_string(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "-_.!~*'();/?:@&=+$,%#[]".contains(ch))
}

/// The scheme of an absolute URL string, per RFC 2396's `scheme` grammar.
fn url_scheme(text: &str) -> Option<&str> {
    let (scheme, _) = text.split_once(':')?;
    let mut chars = scheme.chars();
    let first = chars.next()?;
    let valid = first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'));
    valid.then_some(scheme)
}

/// The path portion of a URL string: after the scheme and authority, and
/// before any query or fragment.
fn url_string_path(text: &str) -> &str {
    let rest = match url_scheme(text) {
        Some(scheme) => &text[scheme.len() + 1..],
        None => text,
    };
    let rest = match rest.strip_prefix("//") {
        Some(authority) => authority.find('/').map_or("", |index| &authority[index..]),
        None => rest,
    };
    rest.split(['?', '#']).next().unwrap_or("")
}

/// Parses a `file:` URL string's path to a URL path, when its authority is
/// empty or `localhost`.
fn file_url_path(text: &str) -> Option<String> {
    let rest = &text[url_scheme(text)?.len() + 1..];
    let rest = match rest.strip_prefix("//") {
        Some(authority) => {
            let (host, path) = authority.split_at(authority.find('/').unwrap_or(authority.len()));
            if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
                return None;
            }
            path
        }
        None => rest,
    };
    let path = percent_decode(rest.split(['?', '#']).next().unwrap_or(""))?;
    if !path.starts_with('/') {
        return None;
    }
    let components = normalize_components(Vec::new(), &path);
    Some(url_path_from_posix_components(&components))
}

/// Resolves a relative URL reference against a base URL string, per the
/// RFC 2396 merge of a relative path with the base path's directory.
fn resolve_url_string(base: &str, relative: &str) -> String {
    let base = base.split(['?', '#']).next().unwrap_or(base);
    if relative.starts_with('/') {
        let path_start = base.len() - url_string_path(base).len();
        return format!("{}{relative}", &base[..path_start]);
    }
    let directory = base.rfind('/').map_or(base, |index| &base[..=index]);
    format!("{directory}{relative}")
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
    vfs_directories: &[PpcVfsDirectory],
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
        PpcImportDispatcherTarget::CfStringCreateWithCharacters => {
            // CFStringCreateWithCharacters(allocator, const UniChar *chars,
            // CFIndex numChars) copies UTF-16 code units into a new string.
            let count = cpu.gpr[5] as usize;
            let Some(bytes) = ppc_memory_read_bytes(memory, cpu.gpr[4], (count * 2) as u32)
                .or_else(|| (count == 0).then(Vec::new))
            else {
                return Some(PpcImportAction::Return(0));
            };
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                .collect();
            let value = String::from_utf16_lossy(&units);
            let reference = state.create(
                value,
                false,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfStringCompare => {
            // CFStringCompare(a, b, CFStringCompareFlags) returns
            // kCFCompareLessThan (-1), EqualTo (0), or GreaterThan (1);
            // kCFCompareCaseInsensitive (1) folds case.
            let (Some(a), Some(b)) = (
                state.objects.get(&cpu.gpr[3]),
                state.objects.get(&cpu.gpr[4]),
            ) else {
                return Some(PpcImportAction::Return(0));
            };
            let ordering = if cpu.gpr[5] & 1 != 0 {
                a.value.to_lowercase().cmp(&b.value.to_lowercase())
            } else {
                a.value.cmp(&b.value)
            };
            Some(PpcImportAction::Return(ordering as i32 as u32))
        }
        PpcImportDispatcherTarget::CfBooleanGetValue => {
            // Booleans only reach this runtime's guests from property lists,
            // which it does not decode into CFBoolean objects.
            Some(PpcImportAction::Return(0))
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
            } else if let Some(retain_count) = state.collection_retain_count(cpu.gpr[3]) {
                *retain_count = retain_count.saturating_add(1);
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
            } else if let Some(retain_count) = state.collection_retain_count(cpu.gpr[3]) {
                *retain_count -= 1;
                if *retain_count == 0 {
                    state.arrays.remove(&cpu.gpr[3]);
                    state.dictionaries.remove(&cpu.gpr[3]);
                    state.numbers.remove(&cpu.gpr[3]);
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
                .or_else(|| state.collection_retain_count(cpu.gpr[3]).copied())
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
        PpcImportDispatcherTarget::CfBundleCopyBundleUrl => {
            // CFBundleCopyBundleURL returns a caller-owned directory URL for
            // the bundle's top-level folder.
            let root = state
                .main_bundle
                .as_ref()
                .filter(|bundle| bundle.reference == cpu.gpr[3])
                .map(|bundle| {
                    bundle_root(&bundle.path)
                        .unwrap_or(&bundle.path)
                        .to_string()
                })
                .or_else(|| {
                    state
                        .bundles
                        .get(&cpu.gpr[3])
                        .map(|bundle| bundle.path.clone())
                });
            let Some(root) = root else {
                return Some(PpcImportAction::Return(0));
            };
            let reference = state.create_url(
                root,
                true,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfBundleCopyResourceUrl => {
            // CFBundleCopyResourceURL(bundle, resourceName, resourceType,
            // subDirName) finds Contents/Resources/[subDir/]name[.type], or
            // returns NULL when no such file exists.
            let root = state
                .main_bundle
                .as_ref()
                .filter(|bundle| bundle.reference == cpu.gpr[3])
                .and_then(|bundle| bundle_root(&bundle.path).map(str::to_string))
                .or_else(|| {
                    state
                        .bundles
                        .get(&cpu.gpr[3])
                        .map(|bundle| bundle.path.clone())
                });
            let string = |reference: u32| {
                state
                    .objects
                    .get(&reference)
                    .map(|object| object.value.clone())
            };
            let (Some(root), Some(name)) = (root, string(cpu.gpr[4])) else {
                return Some(PpcImportAction::Return(0));
            };
            let file = match string(cpu.gpr[5]).filter(|kind| !kind.is_empty()) {
                Some(kind) => format!("{name}.{kind}"),
                None => name,
            };
            let directory = match string(cpu.gpr[6]).filter(|sub| !sub.is_empty()) {
                Some(sub) => format!("{root}/Contents/Resources/{sub}"),
                None => format!("{root}/Contents/Resources"),
            };
            let path = format!("{directory}/{file}");
            let is_directory = vfs_directories
                .iter()
                .any(|entry| entry.path.eq_ignore_ascii_case(&path));
            if !is_directory
                && !vfs_files
                    .iter()
                    .any(|entry| entry.path.eq_ignore_ascii_case(&path))
            {
                return Some(PpcImportAction::Return(0));
            }
            let reference = state.create_url(
                path,
                is_directory,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfBundleGetFunctionPointerForName => {
            // A CFM application's main bundle and the bundles CFBundleCreate
            // opens here export no Mach-O symbols, so lookups find nothing,
            // as under CarbonLib on Mac OS 9.
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::CfUrlGetFsRef => {
            // CFURLGetFSRef(url, FSRef *) returns true when the URL names an
            // existing object.
            let Some(url) = state
                .urls
                .get(&cpu.gpr[3])
                .filter(|url| url.non_file.is_none())
            else {
                return Some(PpcImportAction::Return(0));
            };
            let exists = vfs_directories
                .iter()
                .any(|entry| entry.path.eq_ignore_ascii_case(&url.path))
                || vfs_files
                    .iter()
                    .any(|entry| entry.path.eq_ignore_ascii_case(&url.path));
            let written = exists
                && super::dispatch_fsref::ppc_write_fsref_parts(
                    memory,
                    cpu.gpr[4],
                    PPC_BOOT_VOLUME_REF_NUM,
                    ppc_parent_dir_id_for_path(vfs_directories, &url.path),
                    &ppc_vfs_basename_bytes(&url.path),
                );
            Some(PpcImportAction::Return(u32::from(written)))
        }
        PpcImportDispatcherTarget::CfUrlCreateFromFsRef => {
            // CFURLCreateFromFSRef(allocator, const FSRef *) returns a
            // caller-owned URL for the object the ref names.
            let Ok((_, parent, name)) = super::dispatch_fsref::ppc_read_fsref(memory, cpu.gpr[4])
            else {
                return Some(PpcImportAction::Return(0));
            };
            let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, parent) else {
                return Some(PpcImportAction::Return(0));
            };
            let name = crate::mac_roman::decode_mac_roman(&name);
            let path = if parent_path.is_empty() {
                name
            } else {
                format!("{parent_path}/{name}")
            };
            let is_directory = vfs_directories
                .iter()
                .any(|entry| entry.path.eq_ignore_ascii_case(&path));
            let reference = state.create_url(
                path,
                is_directory,
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
            } else if base.path.is_empty() {
                // The boot volume's root directory has the empty VFS path.
                component.to_string()
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
        PpcImportDispatcherTarget::CfArrayGetCount => {
            // CFArrayGetCount(theArray) returns the number of values.
            let count = state
                .arrays
                .get(&cpu.gpr[3])
                .map_or(0, |array| array.values.len() as u32);
            Some(PpcImportAction::Return(count))
        }
        PpcImportDispatcherTarget::CfArrayGetValueAtIndex => {
            // CFArrayGetValueAtIndex(theArray, idx) returns the value without
            // retaining it; an index outside 0..count has no value.
            let value = state
                .arrays
                .get(&cpu.gpr[3])
                .and_then(|array| array.values.get(cpu.gpr[4] as usize).copied())
                .unwrap_or(0);
            Some(PpcImportAction::Return(value))
        }
        PpcImportDispatcherTarget::CfDictionaryGetValue => {
            // CFDictionaryGetValue(theDict, key) returns the value for an
            // equal key without retaining it, or NULL when no key matches.
            let value = state
                .dictionaries
                .get(&cpu.gpr[3])
                .and_then(|dictionary| {
                    dictionary
                        .entries
                        .iter()
                        .find(|(key, _)| state.keys_equal(*key, cpu.gpr[4]))
                })
                .map_or(0, |(_, value)| *value);
            Some(PpcImportAction::Return(value))
        }
        PpcImportDispatcherTarget::CfNumberGetValue => {
            // CFNumberGetValue(number, theType, valuePtr) stores the value
            // converted to theType and returns true only when the conversion
            // is exact; an inexact conversion still stores the result.
            let Some(number) = state.numbers.get(&cpu.gpr[3]) else {
                return Some(PpcImportAction::Return(0));
            };
            let (bytes, exact) = match cf_number_bytes(number.value, cpu.gpr[4]) {
                Some(converted) => converted,
                None => return Some(PpcImportAction::Return(0)),
            };
            if cpu.gpr[5] == 0 || memory.write_bytes(cpu.gpr[5], &bytes).is_none() {
                return Some(PpcImportAction::Return(0));
            }
            Some(PpcImportAction::Return(u32::from(exact)))
        }
        PpcImportDispatcherTarget::CfUrlCreateWithFileSystemPath => {
            // CFURLCreateWithFileSystemPath(allocator, filePath, pathStyle,
            // isDirectory) returns an owned file URL. CarbonLib has no POSIX
            // working directory, so relative paths resolve against the
            // application's folder, the default directory at launch.
            let Some(path) = state
                .objects
                .get(&cpu.gpr[4])
                .map(|path| path.value.clone())
            else {
                return Some(PpcImportAction::Return(0));
            };
            let base = launched_app_path
                .and_then(|path| path.rsplit_once('/'))
                .map_or("", |(directory, _)| directory);
            let Some(path) = url_path_from_file_system_path(&path, cpu.gpr[5], base) else {
                return Some(PpcImportAction::Return(0));
            };
            let reference = state.create_url(
                path,
                cpu.gpr[6] & 0xff != 0,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfUrlCreateWithString => {
            // CFURLCreateWithString(allocator, URLString, baseURL) returns an
            // owned URL, or NULL when the string is not a valid URL. A
            // relative string resolves against baseURL when one is given.
            let Some(text) = state
                .objects
                .get(&cpu.gpr[4])
                .map(|text| text.value.clone())
                .filter(|text| is_url_string(text))
            else {
                return Some(PpcImportAction::Return(0));
            };
            let base = state.urls.get(&cpu.gpr[5]).cloned();
            let url = if url_scheme(&text).is_some() {
                cf_url_from_string(text)
            } else {
                match base {
                    Some(PpcCfUrl {
                        non_file: Some(base),
                        ..
                    }) => cf_url_from_string(resolve_url_string(&base, &text)),
                    Some(base) => {
                        let directory = if base.is_directory {
                            base.path.as_str()
                        } else {
                            base.path.rsplit_once('/').map_or("", |(parent, _)| parent)
                        };
                        let path = percent_decode(url_string_path(&text)).and_then(|path| {
                            url_path_from_file_system_path(
                                &path,
                                CF_URL_POSIX_PATH_STYLE,
                                directory,
                            )
                        });
                        path.map(|path| PpcCfUrl {
                            path,
                            is_directory: url_string_path(&text).ends_with('/'),
                            retain_count: 1,
                            non_file: None,
                        })
                    }
                    // Without a base, a relative reference stays relative.
                    None => cf_url_from_string(text),
                }
            };
            let Some(url) = url else {
                return Some(PpcImportAction::Return(0));
            };
            let reference = state.insert_url(
                url,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        PpcImportDispatcherTarget::CfUrlCopyFileSystemPath => {
            // CFURLCopyFileSystemPath(anURL, pathStyle) returns an owned
            // string of the URL's path in pathStyle, without a trailing
            // separator, or NULL when the path cannot be expressed.
            let Some(url) = state.urls.get(&cpu.gpr[3]) else {
                return Some(PpcImportAction::Return(0));
            };
            let path = match &url.non_file {
                Some(text) => percent_decode(url_string_path(text)).and_then(|path| {
                    if path.starts_with('/') {
                        let components = normalize_components(Vec::new(), &path);
                        let path = format!("/{}", components.join("/"));
                        file_system_path_from_url_path(&path, cpu.gpr[4])
                    } else {
                        // A relative URL's path stays relative.
                        let path = path.trim_end_matches('/');
                        match cpu.gpr[4] {
                            CF_URL_POSIX_PATH_STYLE => Some(path.to_string()),
                            CF_URL_HFS_PATH_STYLE => Some(format!(":{}", path.replace('/', ":"))),
                            _ => None,
                        }
                    }
                }),
                None => file_system_path_from_url_path(&url.path, cpu.gpr[4]),
            };
            let Some(path) = path else {
                return Some(PpcImportAction::Return(0));
            };
            let reference = state.create(
                path,
                false,
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            Some(PpcImportAction::Return(reference))
        }
        _ => None,
    }
}

/// Builds the URL an absolute or base-less URL string names.
fn cf_url_from_string(text: String) -> Option<PpcCfUrl> {
    let is_directory = url_string_path(&text).ends_with('/');
    let is_file = url_scheme(&text).is_some_and(|scheme| scheme.eq_ignore_ascii_case("file"));
    if is_file {
        if let Some(path) = file_url_path(&text) {
            return Some(PpcCfUrl {
                path,
                is_directory,
                retain_count: 1,
                non_file: None,
            });
        }
    }
    Some(PpcCfUrl {
        path: text.clone(),
        is_directory,
        retain_count: 1,
        non_file: Some(text),
    })
}

/// Converts a CFNumber value to the big-endian bytes of `number_type`, and
/// whether the conversion was exact.
fn cf_number_bytes(value: PpcCfNumberValue, number_type: u32) -> Option<(Vec<u8>, bool)> {
    let float = match value {
        PpcCfNumberValue::Integer(integer) => integer as f64,
        PpcCfNumberValue::Float(bits) => f64::from_bits(bits),
    };
    let integer = |bits: u32| -> (i64, bool) {
        let (min, max) = (-(1_i128 << (bits - 1)), (1_i128 << (bits - 1)) - 1);
        match value {
            PpcCfNumberValue::Integer(integer) => {
                let clamped = i128::from(integer).clamp(min, max) as i64;
                (clamped, clamped == integer)
            }
            PpcCfNumberValue::Float(_) => {
                let truncated = float.trunc();
                let clamped = (truncated as i128).clamp(min, max) as i64;
                (clamped, truncated == float && clamped as f64 == truncated)
            }
        }
    };
    Some(match number_type {
        CF_NUMBER_SINT8_TYPE | CF_NUMBER_CHAR_TYPE => {
            let (value, exact) = integer(8);
            ((value as i8).to_be_bytes().to_vec(), exact)
        }
        CF_NUMBER_SINT16_TYPE | CF_NUMBER_SHORT_TYPE => {
            let (value, exact) = integer(16);
            ((value as i16).to_be_bytes().to_vec(), exact)
        }
        // int, long, and CFIndex are 32 bits wide on PowerPC Mac OS.
        CF_NUMBER_SINT32_TYPE
        | CF_NUMBER_INT_TYPE
        | CF_NUMBER_LONG_TYPE
        | CF_NUMBER_CF_INDEX_TYPE => {
            let (value, exact) = integer(32);
            ((value as i32).to_be_bytes().to_vec(), exact)
        }
        CF_NUMBER_SINT64_TYPE | CF_NUMBER_LONG_LONG_TYPE => {
            let (value, exact) = integer(64);
            (value.to_be_bytes().to_vec(), exact)
        }
        CF_NUMBER_FLOAT32_TYPE | CF_NUMBER_FLOAT_TYPE => {
            let single = float as f32;
            let exact = f64::from(single) == float || float.is_nan();
            let exact = exact
                && !matches!(value, PpcCfNumberValue::Integer(integer) if single as i64 != integer);
            (single.to_be_bytes().to_vec(), exact)
        }
        CF_NUMBER_FLOAT64_TYPE | CF_NUMBER_DOUBLE_TYPE => {
            let exact =
                !matches!(value, PpcCfNumberValue::Integer(integer) if float as i64 != integer);
            (float.to_be_bytes().to_vec(), exact)
        }
        _ => return None,
    })
}
