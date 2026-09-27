//! PowerPC Resource Manager support, resource loading, typed lookups, attributes,
//! modification, and resource fork serialization.

use super::*;
use crate::managers::resource::{serialize_resource_fork_with_attrs, ResourceForkEntry};
use std::collections::{BTreeSet, HashSet};

pub(crate) const PPC_ICON_SUITE_MAGIC: u32 = u32::from_be_bytes(*b"ISUT");

#[cfg(test)]
pub(crate) fn ppc_icon_suite_entries(
    memory: &mut PpcSectionMem,
    icon_suite: u32,
) -> Option<Vec<(u32, u32)>> {
    let suite = memory.read_u32_be(icon_suite).filter(|ptr| *ptr != 0)?;
    if memory.read_u32_be(suite)? != PPC_ICON_SUITE_MAGIC {
        return None;
    }
    let count = usize::from(memory.read_u16_be(suite + 8)?).min(64);
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let entry = suite.checked_add(10 + u32::try_from(index).ok()?.checked_mul(8)?)?;
        entries.push((memory.read_u32_be(entry)?, memory.read_u32_be(entry + 4)?));
    }
    Some(entries)
}

pub(crate) fn ppc_set_current_resource_refnum(
    memory: &mut PpcSectionMem,
    current_resource_refnum: &mut i16,
    refnum: i16,
) {
    *current_resource_refnum = refnum;
    let _ = memory.write_u16_be(PPC_CUR_RES_FILE_ADDR, refnum as u16);
}

pub(crate) fn ppc_serialized_resource_fork(
    file: &PpcVfsResourceFileRecord,
    vfs_resources: &[PpcVfsResourceRecord],
) -> Option<Vec<u8>> {
    if let Some(raw_data) = file.raw_data.as_ref() {
        return Some(raw_data.to_vec());
    }
    let entries = vfs_resources
        .iter()
        .filter(|resource| resource.path.eq_ignore_ascii_case(&file.path))
        .map(|resource| {
            let (data, attrs) = match (&resource.raw_data, resource.raw_attrs) {
                (Some(raw_data), Some(raw_attrs)) => (raw_data.clone(), raw_attrs),
                _ => (resource.data.clone(), resource.attrs),
            };
            ResourceForkEntry {
                res_type: resource.res_type.to_be_bytes(),
                id: resource.res_id,
                name: resource.name.clone(),
                data,
                attrs: (attrs & 0x00ff) as u8,
            }
        })
        .collect::<Vec<_>>();
    serialize_resource_fork_with_attrs(&entries, file.map_attrs)
}

pub(crate) fn ppc_publish_resource_fork_bytes(
    vfs_resource_files: &mut ProcessVfsResourceFileRecords,
    vfs_resources: &[PpcVfsResourceRecord],
    dirty_only: bool,
) {
    let indices = vfs_resource_files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| {
            (!file.path.is_empty() && (!dirty_only || file.dirty)).then_some(index)
        })
        .collect::<Vec<_>>();
    for index in indices {
        let Some(data) = ppc_serialized_resource_fork(&vfs_resource_files[index], vfs_resources)
        else {
            continue;
        };
        let path = vfs_resource_files[index].path.clone();
        vfs_resource_files[index].resource_len = u32::try_from(data.len()).unwrap_or(u32::MAX);
        vfs_resource_files.update_fork(&path, &data);
    }
}

fn ppc_resource_name_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            if left.is_ascii() && right.is_ascii() {
                left.eq_ignore_ascii_case(right)
            } else {
                left == right
            }
        })
}

pub(crate) fn ppc_get_resource(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
    current_only: bool,
    load_data: bool,
    last_resource_error: &mut i16,
) -> u32 {
    let res_type = cpu.gpr[3];
    let res_id = cpu.gpr[4] as u16 as i16;
    let index = match ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        res_type,
        res_id,
        current_only,
    ) {
        Some(index) => index,
        None => {
            let system_string = (!current_only && res_type == u32::from_be_bytes(*b"STR "))
                .then(|| crate::trap::TrapDispatcher::system_str_default_body(res_id))
                .flatten();
            if let Some(data) = system_string {
                vfs_resources.push(PpcVfsResourceRecord {
                    ref_num: 0,
                    path: "__system__/STR ".to_string(),
                    res_type,
                    res_id,
                    name: Vec::new(),
                    data: data.to_vec(),
                    raw_data: None,
                    raw_attrs: None,
                    attrs: 0,
                    handle: 0,
                });
                vfs_resources.len() - 1
            } else {
                // Classic Resource Manager lookups can return NIL without reporting
                // an error. Mac OS 8 does so for missing IDs with both GetResource
                // and Get1Resource; callers must test the returned handle.
                *last_resource_error = PPC_NO_ERR;
                if ppc_hle_trace_enabled() {
                    eprintln!(
                        "[PPC-TRACE] {}Resource('{}', {}) current_ref={} -> NULL err={}",
                        if current_only { "Get1" } else { "Get" },
                        ppc_res_type_text(res_type),
                        res_id,
                        current_resource_refnum,
                        PPC_NO_ERR
                    );
                }
                return 0;
            }
        }
    };
    let handle = ppc_materialize_vfs_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        index,
        load_data,
        last_resource_error,
    );
    if ppc_hle_trace_enabled() {
        let record = &vfs_resources[index];
        eprintln!(
            "[PPC-TRACE] {}Resource('{}', {}) current_ref={} -> handle=${:08X} home_ref={} path=\"{}\" size={}",
            if current_only { "Get1" } else { "Get" },
            ppc_res_type_text(res_type),
            res_id,
            current_resource_refnum,
            record.handle,
            record.ref_num,
            record.path,
            record.data.len()
        );
    }
    handle
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_get_named_resource(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    current_only: bool,
    load_data: bool,
    last_resource_error: &mut i16,
) -> u32 {
    let res_type = cpu.gpr[3];
    let Some(name) = ppc_read_pstring_bytes(memory, cpu.gpr[4]) else {
        *last_resource_error = PPC_PARAM_ERR;
        return 0;
    };
    let current_match = vfs_resources.iter().position(|record| {
        record.ref_num == current_resource_refnum
            && record.res_type == res_type
            && ppc_resource_name_eq(&record.name, &name)
    });
    let index = if current_only || current_match.is_some() {
        current_match
    } else {
        vfs_resources.iter().position(|record| {
            record.ref_num != PPC_CLOSED_RESOURCE_REF_NUM
                && record.res_type == res_type
                && ppc_resource_name_eq(&record.name, &name)
        })
    };
    let Some(index) = index else {
        let type_exists = vfs_resources.iter().any(|record| {
            record.res_type == res_type
                && if current_only {
                    record.ref_num == current_resource_refnum
                } else {
                    record.ref_num != PPC_CLOSED_RESOURCE_REF_NUM
                }
        });
        let current_map_is_empty = current_only
            && !vfs_resources
                .iter()
                .any(|record| record.ref_num == current_resource_refnum);
        // More Macintosh Toolbox (1993), pp. 1-75--1-76: a missing name
        // reports resNotFound, while an absent resource type in a populated
        // map returns NIL with noErr. Mac OS 8.1 reports resNotFound for a
        // newly created, wholly empty resource map; classic applications use
        // that result to distinguish first-run initialization from failure.
        *last_resource_error = if type_exists || current_map_is_empty {
            PPC_RES_NOT_FOUND_ERR
        } else {
            PPC_NO_ERR
        };
        return 0;
    };
    let handle = ppc_materialize_vfs_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        index,
        load_data,
        last_resource_error,
    );
    handle
}

pub(crate) fn ppc_get_ind_resource(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    current_only: bool,
    load_data: bool,
    last_resource_error: &mut i16,
) -> u32 {
    let res_type = cpu.gpr[3];
    let requested_index = cpu.gpr[4] as u16 as usize;
    if requested_index == 0 {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    }
    let Some(index) = vfs_resources
        .iter()
        .enumerate()
        .filter(|(_, record)| {
            record.res_type == res_type
                && (!current_only || record.ref_num == current_resource_refnum)
        })
        .nth(requested_index - 1)
        .map(|(index, _)| index)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] Get{}IndResource('{}', {}) current_ref={} -> NULL err={}",
                if current_only { "1" } else { "" },
                ppc_res_type_text(res_type),
                requested_index,
                current_resource_refnum,
                PPC_RES_NOT_FOUND_ERR
            );
        }
        return 0;
    };
    let handle = ppc_materialize_vfs_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        index,
        load_data,
        last_resource_error,
    );
    if ppc_hle_trace_enabled() {
        let record = &vfs_resources[index];
        eprintln!(
            "[PPC-TRACE] Get{}IndResource('{}', {}) lr=${:08X} current_ref={} -> handle=${:08X} id={} home_ref={} path=\"{}\" size={}",
            if current_only { "1" } else { "" },
            ppc_res_type_text(res_type),
            requested_index,
            cpu.lr,
            current_resource_refnum,
            handle,
            record.res_id,
            record.ref_num,
            record.path,
            record.data.len()
        );
    }
    handle
}

pub(crate) fn ppc_get_ind_string(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) {
    let string_ptr = cpu.gpr[3];
    let str_list_id = cpu.gpr[4] as u16 as i16;
    let index = cpu.gpr[5] as u16 as usize;
    let res_type = u32::from_be_bytes(*b"STR#");
    let Some(resource_index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        res_type,
        str_list_id,
        false,
    ) else {
        let _ = ppc_write_empty_pstring(memory, string_ptr);
        return;
    };
    let handle = ppc_materialize_vfs_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        resource_index,
        true,
        last_resource_error,
    );
    if handle == 0 {
        let _ = ppc_write_empty_pstring(memory, string_ptr);
        return;
    }
    let string = ppc_str_list_item(&vfs_resources[resource_index].data, index).unwrap_or_default();
    if !ppc_optional_pstring_output_can_write(memory, string_ptr, &string) {
        *last_resource_error = PPC_PARAM_ERR;
        return;
    }
    let _ = ppc_write_pstring_bytes(memory, string_ptr, &string);
}

pub(crate) fn ppc_materialize_vfs_resource_handle(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    _heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    index: usize,
    load_data: bool,
    last_resource_error: &mut i16,
) -> u32 {
    if vfs_resources[index].handle == 0 {
        // Inside Macintosh Volume I (1985), I-118 through I-120:
        // SetResLoad(FALSE) still returns a resource Handle, but its master
        // pointer remains NIL until LoadResource (or a later loading lookup).
        let data = load_data.then(|| vfs_resources[index].data.clone());
        let handle = process_memory_manager.new_native_resource_handle(memory, data.as_deref());
        ppc_apply_process_native_resource_handle(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
            handles,
            handle,
        );
        if handle == 0 {
            *last_mem_error = PPC_MEM_FULL_ERR;
            *last_resource_error = PPC_RES_NOT_FOUND_ERR;
            return 0;
        }
        vfs_resources[index].handle = handle;
    } else if load_data && memory.read_u32_be(vfs_resources[index].handle) == Some(0) {
        let data = vfs_resources[index].data.clone();
        let handle = vfs_resources[index].handle;
        if process_memory_manager.native_allocation(handle).is_some() {
            let result = process_memory_manager.load_native_resource_handle(memory, handle, &data);
            ppc_apply_process_native_resource_handle(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                handle,
            );
            if result != PPC_NO_ERR {
                *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                return 0;
            }
        } else {
            // Some callers publish a stable master pointer in an ordinary PEF
            // mapping. Keep that mapping authoritative while allocating its
            // resource data from the shared process heap.
            let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
            let ptr =
                ppc_process_heap_alloc(process_memory_manager, memory, heap_cursor, size, false);
            if ptr == 0
                || data
                    .iter()
                    .copied()
                    .enumerate()
                    .any(|(offset, byte)| memory.write_u8(ptr + offset as u32, byte).is_none())
                || memory.write_u32_be(handle, ptr).is_none()
            {
                *last_mem_error = PPC_MEM_FULL_ERR;
                *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                return 0;
            }
            process_memory_manager.publish_external_native_resource_handle(
                handle,
                ptr,
                *heap_cursor,
            );
        }
    }
    let handle = vfs_resources[index].handle;
    process_memory_manager.set_process_handle_purgeable(handle, true);
    process_memory_manager.set_process_handle_resource(handle, true);
    *last_mem_error = PPC_NO_ERR;
    *last_resource_error = PPC_NO_ERR;
    handle
}

pub(crate) fn ppc_get_res_attrs(
    cpu: &mut PpcCpu,
    vfs_resources: &[PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) -> i16 {
    let handle = cpu.gpr[3];
    let Some(record) = vfs_resources.iter().find(|record| record.handle == handle) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return PPC_RES_CHANGED_ATTR as i16;
    };
    *last_resource_error = PPC_NO_ERR;
    record.attrs as i16
}

pub(crate) fn ppc_set_res_attrs(
    cpu: &mut PpcCpu,
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut [PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let attrs = cpu.gpr[4] as u16;
    let Some(record) = vfs_resources
        .iter_mut()
        .find(|record| record.handle == handle)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    record.attrs = attrs;
    ppc_drop_raw_resource_data(record);
    let path = record.path.clone();
    ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
    *last_resource_error = PPC_NO_ERR;
}

pub(crate) fn ppc_get_res_info(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_resources: &[PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let id_ptr = cpu.gpr[4];
    let type_ptr = cpu.gpr[5];
    let name_ptr = cpu.gpr[6];
    let Some(record) = vfs_resources.iter().find(|record| record.handle == handle) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    if !ppc_optional_output_can_write(memory, id_ptr, 2)
        || !ppc_optional_output_can_write(memory, type_ptr, 4)
        || !ppc_optional_pstring_output_can_write(memory, name_ptr, &record.name)
    {
        *last_resource_error = PPC_PARAM_ERR;
        return;
    }
    if id_ptr != 0 {
        let _ = memory.write_u16_be(id_ptr, record.res_id as u16);
    }
    if type_ptr != 0 {
        let _ = memory.write_u32_be(type_ptr, record.res_type);
    }
    if name_ptr != 0 {
        let _ = ppc_write_pstring_bytes(memory, name_ptr, &record.name);
    }
    *last_resource_error = PPC_NO_ERR;
}

pub(crate) fn ppc_set_res_info(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut [PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let new_id = cpu.gpr[4] as u16 as i16;
    let name_ptr = cpu.gpr[5];
    let Some(record) = vfs_resources
        .iter_mut()
        .find(|record| record.handle == handle)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    if (record.attrs & PPC_RES_PROTECTED_ATTR) != 0 {
        *last_resource_error = PPC_RES_ATTR_ERR;
        return;
    }
    let Some(name) = ppc_resource_name(memory, name_ptr) else {
        *last_resource_error = PPC_PARAM_ERR;
        return;
    };
    record.res_id = new_id;
    if name_ptr != 0 {
        record.name = name;
    }
    ppc_drop_raw_resource_data(record);
    let path = record.path.clone();
    ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
    *last_resource_error = PPC_NO_ERR;
}

pub(crate) fn ppc_home_res_file(
    cpu: &mut PpcCpu,
    vfs_resources: &[PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) -> i16 {
    let handle = cpu.gpr[3];
    let Some(record) = vfs_resources.iter().find(|record| record.handle == handle) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return -1;
    };
    *last_resource_error = PPC_NO_ERR;
    record.ref_num
}

pub(crate) fn ppc_count_resources(
    cpu: &mut PpcCpu,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    current_only: bool,
    last_resource_error: &mut i16,
) -> i16 {
    let res_type = cpu.gpr[3];
    let count = vfs_resources
        .iter()
        .filter(|record| {
            record.res_type == res_type
                && (!current_only || record.ref_num == current_resource_refnum)
        })
        .count();
    *last_resource_error = PPC_NO_ERR;
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Count{}Resources('{}') current_ref={} -> {}",
            if current_only { "1" } else { "" },
            ppc_res_type_text(res_type),
            current_resource_refnum,
            count
        );
    }
    i16::try_from(count).unwrap_or(i16::MAX)
}

pub(crate) fn ppc_count_resource_types(
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    current_only: bool,
    last_resource_error: &mut i16,
) -> i16 {
    // CountTypes / Count1Types count distinct types across the search chain
    // or in the current resource file, respectively.
    // Inside Macintosh: More Macintosh Toolbox 1993, 1-102
    let count = vfs_resources
        .iter()
        .filter(|record| !current_only || record.ref_num == current_resource_refnum)
        .map(|record| record.res_type)
        .collect::<HashSet<_>>()
        .len();
    *last_resource_error = PPC_NO_ERR;
    i16::try_from(count).unwrap_or(i16::MAX)
}

pub(crate) fn ppc_get_ind_resource_type(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    current_only: bool,
    last_resource_error: &mut i16,
) {
    // GetIndType / Get1IndType return the indexed type through a ResType
    // pointer, or four NUL bytes when the 1-based index is out of range.
    // Inside Macintosh: More Macintosh Toolbox 1993, 1-103–1-104
    let output = cpu.gpr[3];
    let index = cpu.gpr[4] as u16 as usize;
    let types: BTreeSet<_> = vfs_resources
        .iter()
        .filter(|record| !current_only || record.ref_num == current_resource_refnum)
        .map(|record| record.res_type)
        .collect();
    let res_type = index
        .checked_sub(1)
        .and_then(|index| types.into_iter().nth(index))
        .unwrap_or(0);
    *last_resource_error = if memory.write_u32_be(output, res_type).is_some() {
        PPC_NO_ERR
    } else {
        PPC_PARAM_ERR
    };
}

pub(crate) fn ppc_unique_id(
    cpu: &mut PpcCpu,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    current_only: bool,
    last_resource_error: &mut i16,
) -> i16 {
    let res_type = cpu.gpr[3];
    *last_resource_error = PPC_NO_ERR;
    for candidate in 128..=i16::MAX {
        if !vfs_resources.iter().any(|record| {
            record.res_type == res_type
                && record.res_id == candidate
                && (!current_only || record.ref_num == current_resource_refnum)
        }) {
            return candidate;
        }
    }
    i16::MAX
}

pub(crate) fn ppc_update_res_file(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    resource_files: &[PpcResourceFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) {
    let ref_num = cpu.gpr[3] as u16 as i16;
    let file_exists = ref_num == 0
        || ref_num == current_resource_refnum
        || resource_files.iter().any(|file| file.ref_num == ref_num)
        || vfs_resources.iter().any(|record| record.ref_num == ref_num);
    if !file_exists {
        *last_resource_error = PPC_RES_F_NOT_FOUND_ERR;
        return;
    }
    for record in vfs_resources
        .iter_mut()
        .filter(|record| record.ref_num == ref_num)
    {
        if (record.attrs & PPC_RES_CHANGED_ATTR) != 0 {
            if let Some(data) = ppc_handle_bytes(memory, handles, record.handle) {
                record.data = data;
            }
            ppc_drop_raw_resource_data(record);
        }
        record.attrs &= !PPC_RES_CHANGED_ATTR;
    }
    if ref_num != 0 {
        if let Some(path) = resource_files
            .iter()
            .find(|file| file.ref_num == ref_num)
            .map(|file| file.path.clone())
            .or_else(|| {
                vfs_resources
                    .iter()
                    .find(|record| record.ref_num == ref_num)
                    .map(|record| record.path.clone())
            })
        {
            ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
        }
    }
    *last_resource_error = PPC_NO_ERR;
}

pub(crate) fn ppc_vfs_resource_index(
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    res_type: u32,
    res_id: i16,
    current_only: bool,
) -> Option<usize> {
    let current_match = vfs_resources.iter().position(|record| {
        record.ref_num == current_resource_refnum
            && record.res_type == res_type
            && record.res_id == res_id
    });
    if current_only || current_match.is_some() {
        return current_match;
    }
    vfs_resources.iter().position(|record| {
        record.ref_num != PPC_CLOSED_RESOURCE_REF_NUM
            && record.res_type == res_type
            && record.res_id == res_id
    })
}

fn ppc_write_empty_pstring(memory: &mut PpcSectionMem, addr: u32) -> bool {
    addr == 0 || memory.write_u8(addr, 0).is_some()
}

fn ppc_str_list_item(data: &[u8], index: usize) -> Option<Vec<u8>> {
    if index == 0 || data.len() < 2 {
        return None;
    }
    let count = u16::from_be_bytes([data[0], data[1]]) as usize;
    if index > count {
        return None;
    }
    let mut offset = 2usize;
    for item in 1..=count {
        let len = *data.get(offset)? as usize;
        offset += 1;
        let end = offset.checked_add(len)?;
        let bytes = data.get(offset..end)?;
        if item == index {
            return Some(bytes.to_vec());
        }
        offset = end;
    }
    None
}

pub(crate) fn ppc_add_resource(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    resource_files: &[PpcResourceFileRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
) -> i16 {
    let handle = cpu.gpr[3];
    let res_type = cpu.gpr[4];
    let res_id = cpu.gpr[5] as u16 as i16;
    let name_ptr = cpu.gpr[6];
    if handle == 0 || vfs_resources.iter().any(|record| record.handle == handle) {
        return PPC_ADD_RES_FAILED;
    }
    let Some(data) = ppc_handle_bytes(memory, handles, handle) else {
        return PPC_ADD_RES_FAILED;
    };
    let Some(name) = ppc_resource_name(memory, name_ptr) else {
        return PPC_ADD_RES_FAILED;
    };
    let path = ppc_resource_file_path(resource_files, current_resource_refnum);
    let mut replaced_handles = Vec::new();
    vfs_resources.retain(|record| {
        let replaced = record.ref_num == current_resource_refnum
            && record.res_type == res_type
            && record.res_id == res_id;
        if replaced {
            replaced_handles.push(record.handle);
        }
        !replaced
    });
    for replaced_handle in replaced_handles {
        process_memory_manager.set_process_handle_resource(replaced_handle, false);
    }
    vfs_resources.push(PpcVfsResourceRecord {
        ref_num: current_resource_refnum,
        path: path.clone(),
        res_type,
        res_id,
        name,
        data,
        raw_data: None,
        raw_attrs: None,
        attrs: PPC_RES_CHANGED_ATTR,
        handle,
    });
    process_memory_manager.set_process_handle_resource(handle, true);
    ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
    PPC_NO_ERR
}

pub(crate) fn ppc_changed_resource(
    cpu: &mut PpcCpu,
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut [PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let Some(record) = vfs_resources
        .iter_mut()
        .find(|record| record.handle == handle)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    if (record.attrs & PPC_RES_PROTECTED_ATTR) != 0 {
        *last_resource_error = PPC_RES_ATTR_ERR;
        return;
    }
    record.attrs |= PPC_RES_CHANGED_ATTR;
    ppc_drop_raw_resource_data(record);
    let path = record.path.clone();
    ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
    *last_resource_error = PPC_NO_ERR;
}

pub(crate) fn ppc_write_resource(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut [PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let Some(index) = vfs_resources
        .iter()
        .position(|record| record.handle == handle)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    let attrs = vfs_resources[index].attrs;
    if (attrs & PPC_RES_PROTECTED_ATTR) != 0 || (attrs & PPC_RES_CHANGED_ATTR) == 0 {
        *last_resource_error = PPC_NO_ERR;
        return;
    }
    let Some(data) = ppc_handle_bytes(memory, handles, handle) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    vfs_resources[index].data = data;
    vfs_resources[index].attrs &= !PPC_RES_CHANGED_ATTR;
    vfs_resources[index].raw_data = None;
    vfs_resources[index].raw_attrs = None;
    let path = vfs_resources[index].path.clone();
    ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
    *last_resource_error = PPC_NO_ERR;
}

fn ppc_drop_raw_resource_data(record: &mut PpcVfsResourceRecord) {
    record.raw_data = None;
    record.raw_attrs = None;
}

pub(crate) fn ppc_remove_resource(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    vfs_resource_files: &mut [PpcVfsResourceFileRecord],
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let Some(index) = vfs_resources
        .iter()
        .position(|record| record.handle == handle)
    else {
        *last_resource_error = PPC_RMV_RES_FAILED;
        return;
    };
    let record = &vfs_resources[index];
    if record.ref_num != current_resource_refnum || (record.attrs & PPC_RES_PROTECTED_ATTR) != 0 {
        *last_resource_error = PPC_RMV_RES_FAILED;
        return;
    }
    let path = record.path.clone();
    vfs_resources.remove(index);
    process_memory_manager.set_process_handle_resource(handle, false);
    ppc_mark_resource_file_contents_dirty(vfs_resource_files, &path);
    *last_resource_error = PPC_NO_ERR;
}

pub(crate) fn ppc_release_resource(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let Some(record) = vfs_resources
        .iter_mut()
        .find(|record| record.handle == handle)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    if (record.attrs & PPC_RES_CHANGED_ATTR) != 0 {
        *last_resource_error = PPC_NO_ERR;
        return;
    }
    // More Macintosh Toolbox Essentials (1993), pp. 1-22 and 1-107:
    // ReleaseResource frees the resource data and invalidates its handle so
    // the application heap can satisfy later allocations from that storage.
    if ppc_dispose_process_native_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        handle,
    ) {
        record.handle = 0;
        *last_resource_error = PPC_NO_ERR;
    } else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
    }
}

pub(crate) fn ppc_detach_resource(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    vfs_resources: &mut [PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let Some(record) = vfs_resources
        .iter_mut()
        .find(|record| record.handle == handle)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    if (record.attrs & PPC_RES_CHANGED_ATTR) != 0 {
        *last_resource_error = PPC_NO_ERR;
        return;
    }
    record.handle = 0;
    process_memory_manager.set_process_handle_resource(handle, false);
    *last_resource_error = PPC_NO_ERR;
}

pub(crate) fn ppc_read_partial_resource(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    vfs_resources: &[PpcVfsResourceRecord],
    last_resource_error: &mut i16,
) {
    let handle = cpu.gpr[3];
    let offset = cpu.gpr[4] as i32;
    let buffer = cpu.gpr[5];
    let count = cpu.gpr[6] as i32;
    let Some(record) = vfs_resources
        .iter()
        .find(|record| record.handle == handle && record.ref_num != PPC_CLOSED_RESOURCE_REF_NUM)
    else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return;
    };
    if offset < 0 || count < 0 {
        *last_resource_error = PPC_INPUT_OUT_OF_BOUNDS_ERR;
        return;
    }
    let Ok(start) = usize::try_from(offset) else {
        *last_resource_error = PPC_INPUT_OUT_OF_BOUNDS_ERR;
        return;
    };
    let Ok(count) = usize::try_from(count) else {
        *last_resource_error = PPC_INPUT_OUT_OF_BOUNDS_ERR;
        return;
    };
    let Some(end) = start.checked_add(count) else {
        *last_resource_error = PPC_INPUT_OUT_OF_BOUNDS_ERR;
        return;
    };
    let Some(bytes) = record.data.get(start..end) else {
        *last_resource_error = PPC_INPUT_OUT_OF_BOUNDS_ERR;
        return;
    };
    if !bytes.is_empty() && memory.write_bytes(buffer, bytes).is_none() {
        *last_resource_error = PPC_PARAM_ERR;
        return;
    }
    // More Macintosh Toolbox (1993), pp. 1-69--1-70: partial reads use
    // resource-fork backing even for an empty SetResLoad(FALSE) handle, and
    // still copy bytes while reporting resourceInMemory for a loaded handle.
    *last_resource_error = if memory.read_u32_be(handle).unwrap_or(0) == 0 {
        PPC_NO_ERR
    } else {
        PPC_RESOURCE_IN_MEMORY_ERR
    };
}

fn ppc_resource_name(memory: &mut PpcSectionMem, name_ptr: u32) -> Option<Vec<u8>> {
    if name_ptr == 0 {
        return Some(Vec::new());
    }
    let len = usize::from(memory.read_u8(name_ptr)?);
    let mut name = Vec::with_capacity(len);
    for offset in 0..u32::try_from(len).ok()? {
        name.push(memory.read_u8(name_ptr.checked_add(1 + offset)?)?);
    }
    Some(name)
}
