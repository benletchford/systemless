//! StdCLib descriptor I/O uses the Apple MPW ABI, not the MSL flag values.
//!
//! MPW FCntl.h: access mode occupies bits 0..1; append/resource/alias are
//! bits 3/4/5; create/truncate/exclusive/binary are bits 8/9/10/11;
//! no-alias-resolution is bit 14.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcDescriptorOperation {
    Open,
    Close,
    Read,
    Write,
    Seek,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DescriptorMode {
    pub readable: bool,
    pub writable: bool,
    pub append: bool,
    pub resource_fork: bool,
    pub resolve_alias: bool,
    pub alias_only: bool,
    pub create: bool,
    pub truncate: bool,
    pub exclusive: bool,
    pub binary: bool,
}

impl DescriptorMode {
    pub(super) fn from_mpw_flags(flags: u32) -> Result<Self, u32> {
        const VALID: u32 = 3 | 8 | 16 | 32 | 256 | 512 | 1024 | 2048 | 16384;
        let access = flags & 3;
        if flags & !VALID != 0 || access == 3 {
            return Err(22); // EINVAL, MPW errno.h.
        }
        let writable = access != 0;
        if (flags & 512 != 0 && !writable) || (flags & 32 != 0 && flags & (256 | 1024) != 0) {
            return Err(22);
        }
        Ok(Self {
            readable: access != 1,
            writable,
            append: flags & 8 != 0,
            resource_fork: flags & 16 != 0,
            resolve_alias: flags & 16384 == 0,
            alias_only: flags & 32 != 0,
            create: flags & 256 != 0,
            truncate: flags & 512 != 0,
            exclusive: flags & 1024 != 0,
            binary: flags & 2048 != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DescriptorRecord {
    /// Toolbox access path. Its position is authoritative for this descriptor.
    pub ref_num: i16,
    pub mode: DescriptorMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcDescriptorState {
    pub records: HashMap<u32, DescriptorRecord>,
    pub standard_closed: [bool; 3],
}

impl Default for PpcDescriptorState {
    fn default() -> Self {
        Self {
            records: HashMap::new(),
            standard_closed: [false; 3],
        }
    }
}

impl PpcDescriptorState {
    /// Descriptor numbers are process-local and are not Toolbox refnums.
    pub(super) fn available_descriptor(&self) -> Option<u32> {
        (0..=i32::MAX as u32).find(|descriptor| {
            (*descriptor >= 3 || self.standard_closed[*descriptor as usize])
                && !self.records.contains_key(descriptor)
        })
    }
}

fn descriptor_resolve_alias(
    mut path: String,
    context: &dispatch_imports::PpcDispatchContext<'_>,
) -> Result<String, u32> {
    let mut seen = Vec::<String>::new();
    while let Some(alias) =
        ppc_alias_record_for_path(context.vfs_directories, context.vfs_resources, &path)
    {
        if seen.len() == 32 || seen.iter().any(|old| old.eq_ignore_ascii_case(&path)) {
            return Err(22);
        }
        seen.push(path);
        path = ppc_resolved_fsspec_target_path(
            context.vfs_directories,
            context.vfs_files,
            context.vfs_resource_files,
            alias.target_dir_id,
            &alias.target_name,
        )
        .ok_or(2u32)?;
    }
    Ok(path)
}

fn descriptor_path(
    requested: &str,
    resolve_alias: bool,
    context: &dispatch_imports::PpcDispatchContext<'_>,
) -> Result<String, u32> {
    if requested.is_empty() {
        return Err(2);
    }
    let hfs = requested.contains(':');
    let (base, remainder, mut floor) = if hfs && !requested.starts_with(':') {
        let (name, remainder) = requested.split_once(':').unwrap();
        let root = if name.eq_ignore_ascii_case(crate::trap::TrapDispatcher::boot_volume_name()) {
            PPC_ROOT_DIR_ID
        } else {
            context
                .vfs_volumes
                .iter()
                .find(|volume| volume.name.eq_ignore_ascii_case(name))
                .map(|volume| volume.root_dir_id)
                .ok_or(2u32)?
        };
        let base = ppc_directory_path_for_id(context.vfs_directories, root).ok_or(2u32)?;
        (
            base,
            remainder,
            base.split('/').filter(|part| !part.is_empty()).count(),
        )
    } else if requested.starts_with('/') {
        ("", requested.trim_start_matches('/'), 0)
    } else {
        let directory = if context.default_dir_id == 0 {
            PPC_ROOT_DIR_ID
        } else {
            context.default_dir_id
        };
        let base = ppc_directory_path_for_id(context.vfs_directories, directory).ok_or(2u32)?;
        let floor = context
            .vfs_volumes
            .iter()
            .filter_map(|volume| {
                ppc_directory_path_for_id(context.vfs_directories, volume.root_dir_id)
            })
            .filter(|root| base == *root || base.starts_with(&format!("{root}/")))
            .map(|root| root.split('/').filter(|part| !part.is_empty()).count())
            .max()
            .unwrap_or(0);
        (
            base,
            requested.strip_prefix(':').unwrap_or(requested),
            floor,
        )
    };
    let mut parts = base
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let components = remainder
        .split(if hfs { ':' } else { '/' })
        .collect::<Vec<_>>();
    for (index, component) in components.iter().enumerate() {
        let last = index + 1 == components.len();
        if component.is_empty() && last {
            break;
        }
        let parent = (hfs && component.is_empty()) || (!hfs && *component == "..");
        if parent {
            if parts.len() <= floor {
                return Err(2);
            }
            parts.pop();
        } else if !component.is_empty() && (hfs || *component != ".") {
            parts.push((*component).to_string());
        }
        if !last {
            let path = parts.join("/");
            let resolved = if resolve_alias {
                descriptor_resolve_alias(path.clone(), context)?
            } else {
                path.clone()
            };
            if ppc_directory_id_for_path(context.vfs_directories, &resolved).is_none() {
                return Err(2);
            }
            if resolved != path {
                // A directory alias may cross volumes. Parent traversal then
                // stops at the target volume's root, not the alias's volume.
                floor = context
                    .vfs_volumes
                    .iter()
                    .filter_map(|volume| {
                        ppc_directory_path_for_id(context.vfs_directories, volume.root_dir_id)
                    })
                    .filter(|root| resolved == *root || resolved.starts_with(&format!("{root}/")))
                    .map(|root| root.split('/').filter(|part| !part.is_empty()).count())
                    .max()
                    .unwrap_or(0);
                parts = resolved
                    .split('/')
                    .filter(|part| !part.is_empty())
                    .map(str::to_string)
                    .collect();
            }
        }
    }
    Ok(parts.join("/"))
}

fn open_descriptor(context: &mut dispatch_imports::PpcDispatchContext<'_>) -> Result<u32, u32> {
    let mode = DescriptorMode::from_mpw_flags(context.cpu.gpr[4])?;
    let source = context.cpu.gpr[3];
    if source == 0 {
        return Err(14);
    }
    let mut bytes = Vec::new();
    for offset in 0..1024u32 {
        let address = source.checked_add(offset).ok_or(14u32)?;
        let byte = context.memory.read_u8(address).ok_or(14u32)?;
        if byte == 0 {
            break;
        }
        bytes.push(byte);
    }
    if bytes.len() == 1024 {
        return Err(22);
    }
    let requested = decode_mac_roman(&bytes);
    let mut path = descriptor_path(&requested, mode.resolve_alias, context)?;
    let exists = |path: &str, context: &dispatch_imports::PpcDispatchContext<'_>| {
        ppc_vfs_file_index(context.vfs_files, path).is_some()
            || ppc_vfs_resource_file_index(context.vfs_resource_files, path).is_some()
    };
    if mode.create && mode.exclusive && exists(&path, context) {
        return Err(17);
    }
    if mode.alias_only {
        if ppc_alias_record_for_path(context.vfs_directories, context.vfs_resources, &path)
            .is_none()
        {
            return Err(3); // MPW _fsFAccess: O_ALIAS requires an alias file.
        }
    } else if mode.resolve_alias {
        path = descriptor_resolve_alias(path, context)?;
    }
    if ppc_directory_id_for_path(context.vfs_directories, &path).is_some() {
        return Err(21);
    }
    if !exists(&path, context) && !mode.create {
        return Err(2);
    }
    let volume_read_only = context.vfs_volumes.iter().any(|volume| {
        volume.attributes & 0x8080 != 0
            && ppc_directory_path_for_id(context.vfs_directories, volume.root_dir_id).is_some_and(
                |root| {
                    path.eq_ignore_ascii_case(root)
                        || path
                            .to_ascii_lowercase()
                            .starts_with(&format!("{}/", root.to_ascii_lowercase()))
                },
            )
    });
    if volume_read_only && (mode.writable || !exists(&path, context)) {
        return Err(30); // EROFS, MPW errno.h.
    }
    if !exists(&path, context) {
        let parent = path
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        if ppc_directory_id_for_path(context.vfs_directories, parent).is_none() {
            return Err(2);
        }
    }
    context
        .toolbox_startup
        .descriptors
        .records
        .retain(|_, record| {
            context
                .files
                .iter()
                .any(|file| file.ref_num == record.ref_num)
        });
    for descriptor in 0..3u32 {
        let stream =
            dispatch_stdio::PPC_STDIO_IOB_ADDR + descriptor * dispatch_stdio::PPC_STDIO_FILE_SIZE;
        context.toolbox_startup.descriptors.standard_closed[descriptor as usize] = context
            .stdio_streams
            .get(&stream)
            .is_none_or(|record| record.closed);
    }
    let descriptor = context
        .toolbox_startup
        .descriptors
        .available_descriptor()
        .ok_or(24u32)?;
    // Reserve both identifiers before creating or truncating any file.
    let ref_num = *context.next_file_ref_num;
    let next_ref_num = ref_num.checked_add(1).ok_or(24u32)?;
    // MPW's standalone file backend creates nonbinary data files as TEXT/ttxt.
    // O_BINARY changes the creation metadata, not read/write byte translation.
    let (file_type, creator, finder_flags) = if exists(&path, context) {
        ppc_finder_info_for_path(context.vfs_files, context.vfs_resource_files, &path)
    } else if !mode.binary && !mode.resource_fork {
        (
            u32::from_be_bytes(*b"TEXT"),
            u32::from_be_bytes(*b"ttxt"),
            0,
        )
    } else {
        (0, 0, 0)
    };
    if ppc_vfs_file_index(context.vfs_files, &path).is_none() {
        context.vfs_files.push(PpcVfsFileRecord {
            path: path.clone(),
            data: Vec::new().into(),
            creator,
            file_type,
            finder_flags,
            dirty: true,
        });
    }
    let ref_num = if mode.resource_fork {
        if ppc_vfs_resource_file_index(context.vfs_resource_files, &path).is_none() {
            context.vfs_resource_files.push(PpcVfsResourceFileRecord {
                path: path.clone(),
                creator,
                file_type,
                finder_flags,
                resource_len: 0,
                raw_data: Some(Vec::new().into()),
                map_attrs: 0,
                dirty: false,
            });
        }
        ppc_open_resource_fork_access(
            &path,
            if mode.writable { 3 } else { 1 },
            context.vfs_files,
            context.vfs_resource_files,
            context.vfs_resources,
            context.files,
            context.writable_refnums,
            context.next_file_ref_num,
        )
        .map_err(|_| 5u32)?
    } else {
        context.files.push(PpcFileRecord {
            ref_num,
            path: path.clone(),
            position: 0,
        });
        if mode.writable {
            context.writable_refnums.insert(ref_num as u16);
        }
        *context.next_file_ref_num = next_ref_num;
        ref_num
    };
    if mode.truncate {
        let access = context
            .files
            .iter()
            .find(|file| file.ref_num == ref_num)
            .unwrap();
        let file = ppc_vfs_file_mut(context.vfs_files, &access.path).unwrap();
        file.data.with_mut(Vec::clear);
        file.dirty = true;
    }
    context
        .toolbox_startup
        .descriptors
        .records
        .insert(descriptor, DescriptorRecord { ref_num, mode });
    if descriptor < 3 {
        let stream =
            dispatch_stdio::PPC_STDIO_IOB_ADDR + descriptor * dispatch_stdio::PPC_STDIO_FILE_SIZE;
        let path = context
            .files
            .iter()
            .find(|file| file.ref_num == ref_num)
            .unwrap()
            .path
            .clone();
        context.stdio_streams.insert(
            stream,
            PpcStdioStreamRecord {
                pushback: None,
                ref_num: Some(ref_num),
                path: Some(path),
                position: 0,
                standard: false,
                readable: mode.readable,
                writable: mode.writable,
                append: mode.append,
                closed: false,
                eof: false,
                error: false,
            },
        );
    }
    Ok(descriptor)
}

pub(super) fn dispatch(
    operation: PpcDescriptorOperation,
    mut context: dispatch_imports::PpcDispatchContext<'_>,
) -> PpcImportAction {
    let descriptor = context.cpu.gpr[3];
    let result = perform(operation, &mut context);
    if descriptor < 3 && operation != PpcDescriptorOperation::Open {
        let stream =
            dispatch_stdio::PPC_STDIO_IOB_ADDR + descriptor * dispatch_stdio::PPC_STDIO_FILE_SIZE;
        if let Some(record) = context.stdio_streams.get_mut(&stream) {
            if operation == PpcDescriptorOperation::Close && result.is_ok() {
                record.closed = true;
            } else if let Some(access) = record
                .ref_num
                .and_then(|ref_num| context.files.iter().find(|file| file.ref_num == ref_num))
            {
                record.position = access.position;
            }
        }
    }
    match result {
        Ok(value) => PpcImportAction::Return(value),
        Err(error) => {
            let _ = context.memory.write_u32_be(PPC_IMPORT_STD_ERRNO, error);
            PpcImportAction::Return(u32::MAX)
        }
    }
}

fn perform(
    operation: PpcDescriptorOperation,
    context: &mut dispatch_imports::PpcDispatchContext<'_>,
) -> Result<u32, u32> {
    use PpcDescriptorOperation as Op;
    if operation == Op::Open {
        return open_descriptor(context);
    }
    let descriptor = context.cpu.gpr[3];
    let record = context
        .toolbox_startup
        .descriptors
        .records
        .get(&descriptor)
        .cloned();
    let Some(record) = record else {
        let stream = dispatch_stdio::PPC_STDIO_IOB_ADDR
            .wrapping_add(descriptor.wrapping_mul(dispatch_stdio::PPC_STDIO_FILE_SIZE));
        if descriptor >= 3
            || context.toolbox_startup.descriptors.standard_closed[descriptor as usize]
            || context
                .stdio_streams
                .get(&stream)
                .is_none_or(|record| record.closed)
        {
            return Err(9);
        }
        return match operation {
            Op::Close => {
                context.toolbox_startup.descriptors.standard_closed[descriptor as usize] = true;
                if let Some(record) = context.stdio_streams.get_mut(&stream) {
                    record.closed = true;
                }
                Ok(0)
            }
            Op::Read if descriptor == 0 => Ok(0), // No host stdin is attached to a guest process.
            Op::Write if descriptor != 0 => {
                use std::io::Write;
                let bytes =
                    ppc_memory_read_bytes(context.memory, context.cpu.gpr[4], context.cpu.gpr[5])
                        .ok_or(14u32)?;
                let result = if descriptor == 1 {
                    std::io::stdout().lock().write_all(&bytes)
                } else {
                    std::io::stderr().lock().write_all(&bytes)
                };
                result.map_err(|_| 5u32)?;
                Ok(bytes.len() as u32)
            }
            Op::Seek => Err(29),
            _ => Err(9),
        };
    };
    let index = context
        .files
        .iter()
        .position(|file| file.ref_num == record.ref_num)
        .ok_or(9u32)?;
    if operation == Op::Close {
        ppc_sync_open_resource_fork(
            record.ref_num,
            context.files,
            context.vfs_files,
            context.vfs_resource_files,
            context.vfs_resources,
        );
        context.files.retain(|file| file.ref_num != record.ref_num);
        context.writable_refnums.remove(&(record.ref_num as u16));
        context
            .toolbox_startup
            .descriptors
            .records
            .remove(&descriptor);
        return Ok(0);
    }
    let path = context.files[index].path.clone();
    let file = ppc_vfs_file_mut(context.vfs_files, &path).ok_or(9u32)?;
    let position = context.files[index].position as usize;
    match operation {
        Op::Seek => {
            let base = match context.cpu.gpr[5] {
                0 => 0,
                1 => position as i64,
                2 => file.data.len() as i64,
                _ => return Err(22),
            };
            let target = base + i64::from(context.cpu.gpr[4] as i32);
            if !(0..=i64::from(i32::MAX)).contains(&target) {
                return Err(22);
            }
            context.files[index].position = target as u32;
            Ok(target as u32)
        }
        Op::Read => {
            if !record.mode.readable {
                return Err(9);
            }
            let count = (context.cpu.gpr[5] as usize).min(file.data.len().saturating_sub(position));
            if count == 0 {
                return Ok(0);
            }
            if !ppc_memory_can_write_bytes(context.memory, context.cpu.gpr[4], count as u32) {
                return Err(14);
            }
            context
                .memory
                .write_bytes(context.cpu.gpr[4], &file.data[position..position + count])
                .ok_or(14u32)?;
            context.files[index].position += count as u32;
            Ok(count as u32)
        }
        Op::Write => {
            if !record.mode.writable {
                return Err(9);
            }
            let count = context.cpu.gpr[5] as usize;
            if count == 0 {
                return Ok(0);
            }
            let bytes = ppc_memory_read_bytes(context.memory, context.cpu.gpr[4], count as u32)
                .ok_or(14u32)?;
            let position = if record.mode.append {
                file.data.len()
            } else {
                position
            };
            let end = position
                .checked_add(count)
                .filter(|end| *end <= i32::MAX as usize)
                .ok_or(27u32)?;
            let result = file.data.with_mut(|data| {
                if end > data.len() {
                    data.try_reserve(end - data.len()).map_err(|_| 12u32)?;
                    data.resize(end, 0);
                }
                data[position..end].copy_from_slice(&bytes);
                Ok::<(), u32>(())
            });
            result?;
            file.dirty = true;
            context.files[index].position = end as u32;
            Ok(count as u32)
        }
        _ => unreachable!(),
    }
}
