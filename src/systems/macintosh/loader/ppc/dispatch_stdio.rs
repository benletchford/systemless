//! Typed Standard I/O dispatch for PowerPC imports.

use super::*;
use std::collections::{HashMap, HashSet};

pub const PPC_STDIO_IOB_ADDR: u32 = PPC_IMPORT_DATA_BASE + 0xa00;
pub const PPC_STDIO_FILE_SIZE: u32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcStdIoOperation {
    ClearErr,
    FileBuffer,
    FileClose,
    FileEof,
    FileError,
    FileFlush,
    FileOpen,
    FilePrintf,
    FileRead,
    FileSeek,
    FileTell,
    FileWrite,
    IoBuffer,
}

pub(crate) fn ppc_initial_stdio_streams() -> HashMap<u32, PpcStdioStreamRecord> {
    (0..3u32)
        .map(|index| {
            (
                PPC_STDIO_IOB_ADDR + index * PPC_STDIO_FILE_SIZE,
                PpcStdioStreamRecord {
                    ref_num: None,
                    path: None,
                    position: 0,
                    standard: true,
                    readable: index == 0,
                    writable: index != 0,
                    append: false,
                    closed: false,
                    eof: false,
                    error: false,
                },
            )
        })
        .collect()
}

fn ppc_stdio_stream_info<'a>(
    stream: u32,
    stdio_streams: &'a HashMap<u32, PpcStdioStreamRecord>,
    vfs_files: &[PpcVfsFileRecord],
) -> Option<(bool, Option<i16>, u32, Option<&'a str>, usize)> {
    let record = stdio_streams.get(&stream)?;
    if record.closed {
        return None;
    }
    if record.standard {
        return Some((true, record.ref_num, record.position, None, 0));
    }
    let path = record.path.as_ref()?;
    let data_len = vfs_files
        .iter()
        .find(|record| record.path.eq_ignore_ascii_case(path))?
        .data
        .len();
    Some((
        false,
        record.ref_num,
        record.position,
        Some(path.as_str()),
        data_len,
    ))
}

fn ppc_stdio_set_position(
    stream: u32,
    position: u32,
    stdio_streams: &mut HashMap<u32, PpcStdioStreamRecord>,
    files: &mut [PpcFileRecord],
) {
    let ref_num = stdio_streams.get_mut(&stream).and_then(|record| {
        record.position = position;
        record.ref_num
    });
    if let Some(ref_num) = ref_num {
        if let Some(file) = files.iter_mut().find(|file| file.ref_num == ref_num) {
            file.position = position;
        }
    }
}

fn ppc_stdio_set_error(stream: u32, stdio_streams: &mut HashMap<u32, PpcStdioStreamRecord>) {
    if let Some(record) = stdio_streams.get_mut(&stream) {
        record.error = true;
    }
}

#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(crate) fn ppc_dispatch_stdio_compatibility(
    operation: PpcStdIoOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    files: &mut Vec<PpcFileRecord>,
    vfs_files: &mut ProcessVfsFileRecords,
    next_file_ref_num: &mut i16,
    stdio_streams: &mut HashMap<u32, PpcStdioStreamRecord>,
) -> PpcImportAction {
    let mut writable_refnums = HashSet::new();
    ppc_dispatch_stdio_compatibility_with_manager(
        operation,
        cpu,
        None,
        memory,
        heap_cursor,
        heap_limit,
        files,
        &mut writable_refnums,
        vfs_files,
        next_file_ref_num,
        stdio_streams,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_dispatch_process_stdio_compatibility(
    operation: PpcStdIoOperation,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    vfs_files: &mut ProcessVfsFileRecords,
    next_file_ref_num: &mut i16,
    stdio_streams: &mut HashMap<u32, PpcStdioStreamRecord>,
) -> PpcImportAction {
    ppc_dispatch_stdio_compatibility_with_manager(
        operation,
        cpu,
        Some(process_memory_manager),
        memory,
        heap_cursor,
        heap_limit,
        files,
        writable_refnums,
        vfs_files,
        next_file_ref_num,
        stdio_streams,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_dispatch_stdio_compatibility_with_manager(
    operation: PpcStdIoOperation,
    cpu: &mut PpcCpu,
    mut process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    files: &mut Vec<PpcFileRecord>,
    writable_refnums: &mut HashSet<u16>,
    vfs_files: &mut ProcessVfsFileRecords,
    next_file_ref_num: &mut i16,
    stdio_streams: &mut HashMap<u32, PpcStdioStreamRecord>,
) -> PpcImportAction {
    let stream = cpu.gpr[3];
    match operation {
        PpcStdIoOperation::FileOpen => {
            let mode = ppc_std_c_string(memory, cpu.gpr[4], 16);
            let Some(&mode_kind @ (b'r' | b'w' | b'a')) = mode.first() else {
                return PpcImportAction::Return(0);
            };
            let mode_flags = &mode[1..];
            if !mode_flags.iter().all(|byte| matches!(byte, b'+' | b'b')) {
                return PpcImportAction::Return(0);
            }
            let plus_mode = mode_flags.contains(&b'+');
            let readable = mode_kind == b'r' || plus_mode;
            let writable = mode_kind != b'r' || plus_mode;
            let creates_if_missing = matches!(mode_kind, b'w' | b'a');
            let append_mode = mode_kind == b'a';
            let requested = decode_mac_roman(&ppc_std_c_string(memory, cpu.gpr[3], 1024));
            let normalized = ppc_normalize_vfs_path(&requested);
            if normalized.is_empty() {
                return PpcImportAction::Return(0);
            }
            let existing_path = ppc_vfs_file_or_resource_path(vfs_files, &[], &normalized)
                .or_else(|| ppc_vfs_file_or_resource_path_by_basename(vfs_files, &[], &normalized));
            let path = existing_path.or_else(|| creates_if_missing.then(|| normalized.clone()));
            let Some(path) = path else {
                return PpcImportAction::Return(0);
            };
            let ref_num = *next_file_ref_num;
            let Some(next_ref_num) = next_file_ref_num.checked_add(1) else {
                return PpcImportAction::Return(0);
            };
            let stream_ptr = if let Some(memory_manager) = process_memory_manager.as_deref_mut() {
                ppc_process_heap_alloc(
                    memory_manager,
                    memory,
                    heap_cursor,
                    PPC_STDIO_FILE_SIZE,
                    true,
                )
            } else {
                ppc_heap_alloc(memory, heap_cursor, heap_limit, PPC_STDIO_FILE_SIZE, true)
            };
            if stream_ptr == 0 {
                return PpcImportAction::Return(0);
            }
            if vfs_files
                .iter()
                .all(|file| !file.path.eq_ignore_ascii_case(&path))
            {
                vfs_files.push(PpcVfsFileRecord {
                    path: path.clone(),
                    data: Vec::new().into(),
                    creator: 0,
                    file_type: 0,
                    finder_flags: 0,
                    dirty: true,
                });
            }
            if mode_kind == b'w' {
                if let Some(file) = ppc_vfs_file_mut(vfs_files, &path) {
                    file.data.with_mut(Vec::clear);
                    file.dirty = true;
                }
            }
            let position = vfs_files
                .iter()
                .find(|file| file.path.eq_ignore_ascii_case(&path))
                .map(|file| {
                    if append_mode {
                        u32::try_from(file.data.len()).unwrap_or(u32::MAX)
                    } else {
                        0
                    }
                })
                .unwrap_or(0);
            files.push(PpcFileRecord {
                ref_num,
                path: path.clone(),
                position,
            });
            if writable {
                writable_refnums.insert(ref_num as u16);
            }
            stdio_streams.insert(
                stream_ptr,
                PpcStdioStreamRecord {
                    ref_num: Some(ref_num),
                    path: Some(path),
                    position,
                    standard: false,
                    readable,
                    writable,
                    append: append_mode,
                    closed: false,
                    eof: false,
                    error: false,
                },
            );
            *next_file_ref_num = next_ref_num;
            PpcImportAction::Return(stream_ptr)
        }
        PpcStdIoOperation::FileClose => {
            let Some(record) = stdio_streams.get_mut(&stream) else {
                return PpcImportAction::Return(u32::MAX);
            };
            if record.closed {
                return PpcImportAction::Return(u32::MAX);
            }
            record.closed = true;
            if !record.standard {
                if let Some(ref_num) = record.ref_num {
                    files.retain(|file| file.ref_num != ref_num);
                    writable_refnums.remove(&(ref_num as u16));
                }
            }
            PpcImportAction::Return(0)
        }
        PpcStdIoOperation::FileRead | PpcStdIoOperation::FileWrite => {
            let destination = cpu.gpr[3];
            let element_size = cpu.gpr[4];
            let element_count = cpu.gpr[5];
            let stream = cpu.gpr[6];
            if element_size == 0 || element_count == 0 {
                return PpcImportAction::Return(0);
            }
            let Some(total) = element_size.checked_mul(element_count) else {
                return PpcImportAction::Return(0);
            };
            let Some(record) = stdio_streams.get(&stream).filter(|record| !record.closed) else {
                return PpcImportAction::Return(0);
            };
            let permitted = if operation == PpcStdIoOperation::FileRead {
                record.readable
            } else {
                record.writable
            };
            if !permitted {
                ppc_stdio_set_error(stream, stdio_streams);
                return PpcImportAction::Return(0);
            }
            let Some((standard, _ref_num, position, path, data_len)) =
                ppc_stdio_stream_info(stream, stdio_streams, vfs_files)
            else {
                return PpcImportAction::Return(0);
            };
            if standard {
                if operation == PpcStdIoOperation::FileWrite
                    && !ppc_memory_can_read_bytes(memory, destination, total)
                {
                    ppc_stdio_set_error(stream, stdio_streams);
                    return PpcImportAction::Return(0);
                }
                return PpcImportAction::Return(if operation == PpcStdIoOperation::FileWrite {
                    element_count
                } else {
                    0
                });
            }
            if operation == PpcStdIoOperation::FileRead {
                let start = usize::try_from(position)
                    .unwrap_or(usize::MAX)
                    .min(data_len);
                let requested = usize::try_from(total).unwrap_or(usize::MAX);
                let read_len = requested.min(data_len.saturating_sub(start));
                if read_len > 0
                    && !ppc_memory_can_write_bytes(
                        memory,
                        destination,
                        u32::try_from(read_len).unwrap_or(u32::MAX),
                    )
                {
                    return PpcImportAction::Return(0);
                }
                let Some(data) = path.and_then(|path| {
                    vfs_files
                        .iter()
                        .find(|record| record.path.eq_ignore_ascii_case(path))
                        .map(|record| record.data.as_slice())
                }) else {
                    return PpcImportAction::Return(0);
                };
                if memory
                    .write_bytes(destination, &data[start..start + read_len])
                    .is_none()
                {
                    return PpcImportAction::Return(0);
                }
                let new_position = position.saturating_add(read_len as u32);
                ppc_stdio_set_position(stream, new_position, stdio_streams, files);
                if read_len < requested {
                    if let Some(record) = stdio_streams.get_mut(&stream) {
                        record.eof = true;
                    }
                }
                PpcImportAction::Return(
                    u32::try_from(read_len / element_size as usize).unwrap_or(0),
                )
            } else {
                if !ppc_memory_can_read_bytes(memory, destination, total) {
                    return PpcImportAction::Return(0);
                }
                let bytes = ppc_memory_read_bytes(memory, destination, total).unwrap_or_default();
                let Some(path) = stdio_streams
                    .get(&stream)
                    .and_then(|record| record.path.clone())
                else {
                    return PpcImportAction::Return(0);
                };
                let Some(file) = ppc_vfs_file_mut(vfs_files, &path) else {
                    return PpcImportAction::Return(0);
                };
                let position = if stdio_streams
                    .get(&stream)
                    .is_some_and(|record| record.append)
                {
                    u32::try_from(file.data.len()).unwrap_or(u32::MAX)
                } else {
                    position
                };
                let start = usize::try_from(position).unwrap_or(usize::MAX);
                let end = start.saturating_add(bytes.len());
                file.data.with_mut(|data| {
                    if start > data.len() {
                        data.resize(start, 0);
                    }
                    if end > data.len() {
                        data.resize(end, 0);
                    }
                    data[start..end].copy_from_slice(&bytes);
                });
                file.dirty = true;
                let new_position = u32::try_from(end).unwrap_or(u32::MAX);
                ppc_stdio_set_position(stream, new_position, stdio_streams, files);
                PpcImportAction::Return(element_count)
            }
        }
        PpcStdIoOperation::FileSeek => {
            let offset = cpu.gpr[4] as i32 as i64;
            let whence = cpu.gpr[5];
            let Some((standard, _ref_num, position, _path, data_len)) =
                ppc_stdio_stream_info(stream, stdio_streams, vfs_files)
            else {
                return PpcImportAction::Return(u32::MAX);
            };
            if standard {
                return PpcImportAction::Return(u32::MAX);
            }
            let base = match whence {
                0 => 0,
                1 => i64::from(position),
                2 => i64::try_from(data_len).unwrap_or(i64::MAX),
                _ => return PpcImportAction::Return(u32::MAX),
            };
            let Some(new_position) = base.checked_add(offset).filter(|p| *p >= 0) else {
                return PpcImportAction::Return(u32::MAX);
            };
            let Ok(new_position) = u32::try_from(new_position) else {
                return PpcImportAction::Return(u32::MAX);
            };
            ppc_stdio_set_position(stream, new_position, stdio_streams, files);
            if let Some(record) = stdio_streams.get_mut(&stream) {
                record.eof = false;
            }
            PpcImportAction::Return(0)
        }
        PpcStdIoOperation::FileTell => {
            let Some(record) = stdio_streams.get(&stream).filter(|record| !record.closed) else {
                return PpcImportAction::Return(u32::MAX);
            };
            PpcImportAction::Return(record.position)
        }
        PpcStdIoOperation::FileBuffer => {
            let Some(record) = stdio_streams.get(&stream).filter(|record| !record.closed) else {
                return PpcImportAction::Return(u32::MAX);
            };
            if !record.readable {
                ppc_stdio_set_error(stream, stdio_streams);
                return PpcImportAction::Return(u32::MAX);
            }
            let Some((_standard, _ref_num, position, path, _data_len)) =
                ppc_stdio_stream_info(stream, stdio_streams, vfs_files)
            else {
                return PpcImportAction::Return(u32::MAX);
            };
            let byte = path
                .and_then(|path| {
                    vfs_files
                        .iter()
                        .find(|record| record.path.eq_ignore_ascii_case(path))
                })
                .and_then(|record| record.data.get(usize::try_from(position).ok()?))
                .copied();
            let Some(byte) = byte else {
                if let Some(record) = stdio_streams.get_mut(&stream) {
                    record.eof = true;
                }
                return PpcImportAction::Return(u32::MAX);
            };
            let new_position = position.saturating_add(1);
            ppc_stdio_set_position(stream, new_position, stdio_streams, files);
            PpcImportAction::Return(u32::from(byte))
        }
        PpcStdIoOperation::FilePrintf => {
            const SCRATCH_SIZE: u32 = 16 * 1024;
            let Some(record) = stdio_streams.get(&stream).filter(|record| !record.closed) else {
                return PpcImportAction::Return(u32::MAX);
            };
            if !record.writable {
                ppc_stdio_set_error(stream, stdio_streams);
                return PpcImportAction::Return(u32::MAX);
            }
            let saved_heap_cursor = *heap_cursor;
            let process_scratch = process_memory_manager.is_some();
            let scratch = if let Some(memory_manager) = process_memory_manager.as_deref_mut() {
                memory_manager.native_scratch_bytes(memory, SCRATCH_SIZE, true)
            } else {
                ppc_heap_alloc(memory, heap_cursor, heap_limit, SCRATCH_SIZE, true)
            };
            if scratch == 0 {
                return PpcImportAction::Return(u32::MAX);
            }
            let mut sprintf_cpu = cpu.clone();
            sprintf_cpu.gpr[3] = scratch;
            let length = ppc_std_sprintf(&sprintf_cpu, memory);
            if !process_scratch {
                *heap_cursor = saved_heap_cursor;
            }
            if length >= SCRATCH_SIZE {
                ppc_stdio_set_error(stream, stdio_streams);
                return PpcImportAction::Return(u32::MAX);
            }
            let Some(bytes) = ppc_memory_read_bytes(memory, scratch, length) else {
                ppc_stdio_set_error(stream, stdio_streams);
                return PpcImportAction::Return(u32::MAX);
            };
            let Some((standard, _ref_num, position, _path, _data_len)) =
                ppc_stdio_stream_info(stream, stdio_streams, vfs_files)
            else {
                return PpcImportAction::Return(u32::MAX);
            };
            if standard {
                return PpcImportAction::Return(length);
            }
            let Some(path) = stdio_streams
                .get(&stream)
                .and_then(|record| record.path.clone())
            else {
                return PpcImportAction::Return(u32::MAX);
            };
            let Some(file) = ppc_vfs_file_mut(vfs_files, &path) else {
                ppc_stdio_set_error(stream, stdio_streams);
                return PpcImportAction::Return(u32::MAX);
            };
            let position = if stdio_streams
                .get(&stream)
                .is_some_and(|record| record.append)
            {
                u32::try_from(file.data.len()).unwrap_or(u32::MAX)
            } else {
                position
            };
            let start = usize::try_from(position).unwrap_or(usize::MAX);
            let end = start.saturating_add(bytes.len());
            file.data.with_mut(|data| {
                if start > data.len() {
                    data.resize(start, 0);
                }
                if end > data.len() {
                    data.resize(end, 0);
                }
                data[start..end].copy_from_slice(&bytes);
            });
            file.dirty = true;
            let new_position = u32::try_from(end).unwrap_or(u32::MAX);
            ppc_stdio_set_position(stream, new_position, stdio_streams, files);
            PpcImportAction::Return(length)
        }
        PpcStdIoOperation::FileFlush => {
            if stdio_streams
                .get(&stream)
                .filter(|record| !record.closed)
                .is_none()
            {
                return PpcImportAction::Return(u32::MAX);
            }
            PpcImportAction::Return(0)
        }
        PpcStdIoOperation::FileEof => {
            let Some(record) = stdio_streams.get(&stream).filter(|record| !record.closed) else {
                return PpcImportAction::Return(u32::MAX);
            };
            PpcImportAction::Return(u32::from(record.eof))
        }
        PpcStdIoOperation::FileError => {
            let Some(record) = stdio_streams.get(&stream).filter(|record| !record.closed) else {
                return PpcImportAction::Return(u32::MAX);
            };
            PpcImportAction::Return(u32::from(record.error))
        }
        PpcStdIoOperation::ClearErr => {
            let Some(record) = stdio_streams
                .get_mut(&stream)
                .filter(|record| !record.closed)
            else {
                return PpcImportAction::Return(u32::MAX);
            };
            record.eof = false;
            record.error = false;
            PpcImportAction::ReturnPreserve
        }
        PpcStdIoOperation::IoBuffer => PpcImportAction::Return(PPC_STDIO_IOB_ADDR),
    }
}
