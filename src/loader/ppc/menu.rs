//! PowerPC Menu Manager state, layout, tracking, and MDEF dispatch.

use ppc::{PpcCpu, PpcMemory};

use super::dispatch_mixed_mode::ppc_begin_m68k_universal_proc_with_operation;
use super::dispatch_standard_file::{ppc_capture_saved_detail, ppc_restore_saved_detail};
use super::graphics::*;
use super::imports::*;
use super::theme::*;
use super::*;
use crate::event_queue::EventQueue;
use crate::menu_model::GuestMenuSnapshot;
use crate::process_context::*;
use crate::quickdraw::text::QuickDrawTextStyle;
use crate::ui_theme::UiThemeId;

pub(crate) use crate::menu_manager::*;
pub(crate) use crate::menu_manager::{
    MenuItem as PpcMenuItemDefinition, MenuList as PpcMenuListDefinition,
    TrackedMenuIcon as PpcTrackedMenuIcon,
    TrackedMenuItemAppearance as PpcTrackedMenuItemAppearance,
};
#[cfg(test)]
pub(crate) use crate::menu_manager::MenuListEntry as PpcMenuListEntry;

// TheMenu identifies the menu owning the selected item. For a hierarchical
// choice this is the submenu ID even though its originating regular title is
// visibly highlighted. Macintosh Toolbox Essentials (1992), pp. 3-115--3-119;
// Inside Macintosh Volume V (1986), pp. V-244 and V-571.
pub const PPC_THE_MENU_ADDR: u32 = crate::memory::globals::addr::THE_MENU;
pub const PPC_MBAR_HEIGHT_ADDR: u32 = 0x0000_0baa;

pub(crate) type PpcMenuTracking = ProcessMenuTrackingState;
pub(crate) type PpcSubmenuTracking = ProcessTrackedMenuPane;

pub(crate) fn ppc_current_menu_list(memory: &mut PpcSectionMem) -> u32 {
    memory
        .read_u32_be(crate::memory::globals::addr::MENU_LIST)
        .unwrap_or(0)
}

pub(crate) fn ppc_set_current_menu_list(memory: &mut PpcSectionMem, menu_list: u32) {
    let _ = memory.write_u32_be(crate::memory::globals::addr::MENU_LIST, menu_list);
}

pub(crate) fn ppc_prepare_menu_definition_port(
    startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
) {
    if startup.execution.menu().context().native_port.is_none() {
        startup.execution.with_menu_context_mut(|context| {
            context.native_port = Some((*current_gworld, *current_gdevice));
        });
    }
    *current_gworld = PPC_MAIN_GWORLD;
    *current_gdevice = PPC_MAIN_GDEVICE;
}

pub(crate) fn ppc_preserve_menu_callback_port(
    startup: &mut PpcToolboxStartupState,
    current_gworld: u32,
    current_gdevice: u32,
) {
    if startup.execution.menu().context().native_port.is_none() {
        startup.execution.with_menu_context_mut(|context| {
            context.native_port = Some((current_gworld, current_gdevice));
        });
    }
}

pub(crate) fn ppc_restore_menu_definition_port(
    startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
) {
    if let Some((gworld, gdevice)) = startup
        .execution
        .with_existing_menu_context_mut(|context| context.native_port.take())
        .flatten()
    {
        *current_gworld = gworld;
        *current_gdevice = gdevice;
    }
}

pub(crate) fn ppc_menu_handle_bytes(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    menu_handle: u32,
) -> Option<Vec<u8>> {
    let record = handles.iter().find(|record| record.handle == menu_handle)?;
    let ptr = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    ppc_memory_read_bytes(memory, ptr, record.size)
}

pub(crate) fn ppc_replace_menu_bytes_with_allocator(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    bytes: &[u8],
) -> i16 {
    ppc_allocator_view_replace_handle_bytes(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        menu_handle,
        bytes,
    )
}

#[cfg(test)]
pub(crate) fn ppc_replace_menu_bytes(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    bytes: &[u8],
) -> i16 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_replace_menu_bytes_with_allocator(
        None,
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
        menu_handle,
        bytes,
    )
}

pub(crate) fn ppc_mutate_menu_items_in_place(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    menu_handle: u32,
    mutation: impl FnOnce(&mut MenuItems) -> bool,
) -> bool {
    let Some(original) = ppc_menu_handle_bytes(memory, handles, menu_handle) else {
        return false;
    };
    let Some(mut items) = MenuItems::decode(&original) else {
        return false;
    };
    if !mutation(&mut items) {
        return false;
    }
    let Some(bytes) = items.rebuild(&original) else {
        return false;
    };
    let Some(menu) = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0) else {
        return false;
    };
    bytes.len() <= original.len() && memory.write_bytes(menu, &bytes).is_some()
}

pub(crate) fn ppc_decode_menu_items(bytes: &[u8]) -> Option<(usize, u32, Vec<PpcMenuItemDefinition>)> {
    let decoded = MenuItems::decode(bytes)?;
    Some((decoded.first_item, decoded.enable_flags, decoded.items))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_alloc_new_menu(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_proc: u32,
    menu_id: i16,
    title_ptr: u32,
) -> u32 {
    let title = ppc_read_pascal_string(memory, title_ptr).unwrap_or_default();
    // Macintosh Toolbox Essentials (1992), pp. 3-105--3-106: NewMenu
    // creates an empty standard MenuRecord, loads the standard MDEF if
    // necessary, and stores its Handle in MenuInfo.menuProc.
    let bytes = new_standard_menu_record(menu_id, menu_proc, &title);
    ppc_allocator_view_allocate_handle_with_bytes(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        &bytes,
    )
}

#[cfg(test)]
pub(crate) fn ppc_new_menu(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    _free_handle_blocks: &mut Vec<PpcHandleRecord>,
    menu_id: i16,
    title_ptr: u32,
) -> u32 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_alloc_new_menu(
        None,
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
        0,
        menu_id,
        title_ptr,
    )
}

pub(crate) fn ppc_menu_item_attribute_address(
    memory: &mut PpcSectionMem,
    menu_handle: u32,
    item: i16,
    attribute_offset: u32,
) -> Option<u32> {
    let (item_addr, item_len) = ppc_menu_item(memory, menu_handle, item)?;
    item_addr
        .checked_add(1 + u32::from(item_len))?
        .checked_add(attribute_offset)
}

pub(crate) fn ppc_get_item_cmd(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    let menu_handle = cpu.gpr[3];
    let item = cpu.gpr[4] as u16 as i16;
    let cmd_char_ptr = cpu.gpr[5];
    if cmd_char_ptr == 0 || !ppc_memory_can_write_bytes(memory, cmd_char_ptr, 2) {
        return;
    }
    let command = ppc_menu_item(memory, menu_handle, item)
        .and_then(|(item_addr, item_len)| {
            let command_offset = 2u32.checked_add(u32::from(item_len))?;
            memory.read_u8(item_addr.checked_add(command_offset)?)
        })
        .unwrap_or(0);
    let _ = memory.write_u16_be(cmd_char_ptr, u16::from(command));
}

pub(crate) fn ppc_set_item_cmd(cpu: &PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) {
    // Inside Macintosh Volume V (1986), p. V-244: SetItemCmd installs the
    // command-key byte, including $1B for a hierarchical submenu.
    ppc_mutate_menu_items_in_place(memory, handles, cpu.gpr[3], |items| {
        items.set_command(cpu.gpr[4] as u16 as i16, cpu.gpr[5] as u8)
    });
}

pub(crate) fn ppc_get_item_mark(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    let mark_ptr = cpu.gpr[5];
    if mark_ptr == 0 || !ppc_memory_can_write_bytes(memory, mark_ptr, 2) {
        return;
    }
    let mark = ppc_menu_item_attribute_address(memory, cpu.gpr[3], cpu.gpr[4] as u16 as i16, 2)
        .and_then(|address| memory.read_u8(address))
        .unwrap_or(0);
    let _ = memory.write_u16_be(mark_ptr, u16::from(mark));
}

pub(crate) fn ppc_count_menu_items(memory: &mut PpcSectionMem, menu_handle: u32) -> u16 {
    ppc_menu_items_from_memory(memory, menu_handle)
        .map(|items| items.item_count())
        .unwrap_or(0)
}

pub(crate) fn ppc_get_menu_item_text(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    let menu_handle = cpu.gpr[3];
    let item = cpu.gpr[4] as u16 as i16;
    let item_string = cpu.gpr[5];
    if item_string == 0 {
        return;
    }
    let text = ppc_menu_item(memory, menu_handle, item)
        .and_then(|(item_addr, item_len)| {
            ppc_memory_read_bytes(memory, item_addr.checked_add(1)?, u32::from(item_len))
        })
        .unwrap_or_default();
    let _ = ppc_write_pstring_bytes(memory, item_string, &text);
}

pub(crate) fn ppc_set_menu_item_text_with_allocator(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> i16 {
    let Some(original) = ppc_menu_handle_bytes(memory, handles, cpu.gpr[3]) else {
        return PPC_NIL_HANDLE_ERR;
    };
    let Some(mut items) = MenuItems::decode(&original) else {
        return PPC_PARAM_ERR;
    };
    let item_number = cpu.gpr[4] as u16 as i16;
    let text = ppc_read_pascal_string(memory, cpu.gpr[5]).unwrap_or_default();
    if !items.set_text(item_number, &text) {
        return PPC_NO_ERR;
    }
    let Some(bytes) = items.rebuild(&original) else {
        return PPC_PARAM_ERR;
    };
    ppc_replace_menu_bytes_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        cpu.gpr[3],
        &bytes,
    )
}

#[cfg(test)]
pub(crate) fn ppc_set_menu_item_text(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
) -> i16 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_set_menu_item_text_with_allocator(
        cpu,
        None,
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
    )
}

pub(crate) fn ppc_delete_menu_item_with_allocator(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    item_number: i16,
) -> i16 {
    let Some(original) = ppc_menu_handle_bytes(memory, handles, menu_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    let Some(menu_id) = original
        .get(..2)
        .map(|bytes| i16::from_be_bytes([bytes[0], bytes[1]]))
    else {
        return PPC_PARAM_ERR;
    };
    let Some(mut items) = MenuItems::decode(&original) else {
        return PPC_PARAM_ERR;
    };
    if !items.delete(item_number) {
        return PPC_NO_ERR;
    }
    let Some(bytes) = items.rebuild(&original) else {
        return PPC_PARAM_ERR;
    };
    let result = ppc_replace_menu_bytes_with_allocator(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        menu_handle,
        &bytes,
    );
    if result == PPC_NO_ERR {
        ppc_filter_menu_color_table_with_allocator(
            allocator,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            |id, item| id != menu_id || item != item_number,
        );
    }
    result
}

#[cfg(test)]
pub(crate) fn ppc_delete_menu_item(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    item_number: i16,
) -> i16 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_delete_menu_item_with_allocator(
        None,
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
        menu_handle,
        item_number,
    )
}

pub(crate) fn ppc_insert_menu_items_with_allocator(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    data_ptr: u32,
    after_item: i16,
) -> i16 {
    let Some(original) = ppc_menu_handle_bytes(memory, handles, menu_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    let Some(mut items) = MenuItems::decode(&original) else {
        return PPC_PARAM_ERR;
    };
    let data = ppc_read_pascal_string(memory, data_ptr).unwrap_or_default();
    if after_item == i16::MAX {
        items.append_specs(&data);
    } else {
        items.insert_specs(&data, after_item);
    }
    let Some(bytes) = items.rebuild(&original) else {
        return PPC_PARAM_ERR;
    };
    ppc_replace_menu_bytes_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        menu_handle,
        &bytes,
    )
}

#[cfg(test)]
pub(crate) fn ppc_insert_menu_items(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    data_ptr: u32,
    after_item: i16,
) -> i16 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_insert_menu_items_with_allocator(
        None,
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
        menu_handle,
        data_ptr,
        after_item,
    )
}

pub(crate) fn ppc_resource_menu_indices(
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    requested_type: u32,
) -> Vec<usize> {
    let font_type = u32::from_be_bytes(*b"FONT");
    let resource_types = if requested_type == font_type {
        [Some(u32::from_be_bytes(*b"FOND")), Some(font_type)]
    } else {
        [Some(requested_type), None]
    };
    let mut seen = std::collections::HashSet::new();
    let mut indices = Vec::new();
    for resource_type in resource_types.into_iter().flatten() {
        for current in [true, false] {
            for (index, resource) in resources.iter().enumerate() {
                if resource.res_type != resource_type
                    || resource.ref_num == PPC_CLOSED_RESOURCE_REF_NUM
                    || resource.name.is_empty()
                    || (resource.ref_num == current_resource_refnum) != current
                {
                    continue;
                }
                if seen.insert((resource_type, resource.res_id)) {
                    indices.push(index);
                }
            }
        }
    }
    indices
}

pub(crate) fn ppc_insert_resource_menu_names_with_allocator(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    names: Vec<Vec<u8>>,
    after_item: i16,
) -> i16 {
    let Some(original) = ppc_menu_handle_bytes(memory, handles, menu_handle) else {
        return PPC_NIL_HANDLE_ERR;
    };
    let Some(mut items) = MenuItems::decode(&original) else {
        return PPC_PARAM_ERR;
    };
    if !items.insert_resource_names(names, after_item) {
        return PPC_NO_ERR;
    }
    let Some(bytes) = items.rebuild(&original) else {
        return PPC_PARAM_ERR;
    };
    ppc_replace_menu_bytes_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        menu_handle,
        &bytes,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_insert_resource_menu(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    resource_policy: &SharedProcessResourcePolicy,
    last_resource_error: &mut i16,
    menu_handle: u32,
    requested_type: u32,
    after_item: i16,
) -> i16 {
    // AppendResMenu and InsertResMenu force SetResLoad(TRUE) and read every
    // matching resource before returning. Macintosh Toolbox Essentials
    // (1992), pp. 3-101--3-104.
    resource_policy.set_res_load(true);
    let _ = memory.write_u16_be(crate::memory::globals::addr::RES_LOAD, 0x0100);
    let indices = ppc_resource_menu_indices(resources, current_resource_refnum, requested_type);
    let mut names = Vec::with_capacity(indices.len());
    for index in indices {
        let name = resources[index].name.clone();
        if ppc_materialize_vfs_resource_handle(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            resources,
            index,
            true,
            last_resource_error,
        ) == 0
        {
            return *last_mem_error;
        }
        names.push(name);
    }
    if requested_type == u32::from_be_bytes(*b"FONT") {
        for &(id, name) in crate::quickdraw::fonts::FONT_NAMES {
            if id != crate::quickdraw::fonts::FONT_APPLICATION {
                names.push(crate::mac_roman::encode_mac_roman_lossy(name));
            }
        }
    }
    let mut allocator = PpcProcessAllocatorView {
        memory_manager: process_memory_manager,
    };
    ppc_insert_resource_menu_names_with_allocator(
        Some(&mut allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        menu_handle,
        names,
        after_item,
    )
}

pub(crate) fn ppc_set_menu_item_enabled(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    menu_handle: u32,
    item: i16,
    enabled: bool,
) {
    // A title or item already in the requested state needs no record rebuild.
    // Keep the decoded path below for actual changes and malformed records.
    if !(0..=31).contains(&item) {
        return;
    }
    if handles
        .iter()
        .any(|record| record.handle == menu_handle && record.size >= 14)
    {
        if let Some(menu) = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0) {
            if let Some(flags_addr) = menu.checked_add(10) {
                if let Some(flags) = memory.read_u32_be(flags_addr) {
                    let bit = 1u32 << u32::from(item as u16);
                    if (flags & bit != 0) == enabled {
                        return;
                    }
                }
            }
        }
    }
    ppc_mutate_menu_items_in_place(memory, handles, menu_handle, |items| {
        items.set_enabled(item, enabled)
    });
}

pub(crate) fn ppc_set_item_mark(cpu: &PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) {
    ppc_mutate_menu_items_in_place(memory, handles, cpu.gpr[3], |items| {
        items.set_mark(cpu.gpr[4] as u16 as i16, cpu.gpr[5] as u8)
    });
}

pub(crate) fn ppc_check_menu_item(cpu: &PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) {
    let menu_handle = cpu.gpr[3];
    let item = cpu.gpr[4] as u16 as i16;
    let checked = cpu.gpr[5] & 0xff != 0;
    // Inside Macintosh Volume I (1985), pp. I-345 and I-358: a compiled
    // MENU item stores icon, command-key, mark, and style bytes after its
    // Pascal text. CheckItem uses the standard checkmark glyph ($12).
    ppc_mutate_menu_items_in_place(memory, handles, menu_handle, |items| {
        items.set_mark(item, if checked { 0x12 } else { 0 })
    });
}

pub(crate) fn ppc_menu_resource_data<'a>(
    resources: &'a [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    resource_type: &[u8; 4],
    resource_id: i16,
) -> Option<&'a [u8]> {
    let index = ppc_vfs_resource_index(
        resources,
        current_resource_refnum,
        u32::from_be_bytes(*resource_type),
        resource_id,
        false,
    )?;
    Some(&resources.get(index)?.data)
}

pub(crate) fn ppc_menu_item_appearance(
    memory: &mut PpcSectionMem,
    menu_handle: u32,
    item: i16,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> PpcTrackedMenuItemAppearance {
    let is_separator = ppc_menu_item_is_separator(memory, menu_handle, item);
    let icon = ppc_menu_item_attribute_address(memory, menu_handle, item, 0)
        .and_then(|address| memory.read_u8(address))
        .unwrap_or(0);
    let command = ppc_menu_item_attribute_address(memory, menu_handle, item, 1)
        .and_then(|address| memory.read_u8(address))
        .unwrap_or(0);
    let style = ppc_menu_item_attribute_address(memory, menu_handle, item, 3)
        .and_then(|address| memory.read_u8(address))
        .unwrap_or(0);
    // Resource access and bitmap ownership remain PowerPC adapter work; the
    // standard MDEF's resource priority, selector interpretation, and
    // geometry are shared with the 68k gateway. Macintosh Toolbox Essentials
    // (1992), pp. 3-45--3-46 and 3-62--3-63.
    let resource_id = standard_menu_icon_resource_id(icon, command);
    let color_icon = resource_id
        .and_then(|resource_id| {
            ppc_menu_resource_data(resources, current_resource_refnum, b"cicn", resource_id)
        })
        .and_then(|data| ColorIconLayout::decode(data).map(|layout| (data, layout)));
    let icon_kind = standard_menu_icon_kind(
        icon,
        command,
        color_icon.map(|(_data, layout)| (layout.width, layout.height)),
    );
    let tracked_icon = match icon_kind {
        StandardMenuIconKind::None => None,
        StandardMenuIconKind::Color { .. } => {
            color_icon.map(|(data, _layout)| PpcTrackedMenuIcon::CIcon(data.to_vec()))
        }
        StandardMenuIconKind::Small => resource_id
            .and_then(|resource_id| {
                ppc_menu_resource_data(resources, current_resource_refnum, b"SICN", resource_id)
            })
            .filter(|data| data.len() >= 32)
            .map(|data| PpcTrackedMenuIcon::SmallIcon(data.to_vec())),
        StandardMenuIconKind::Normal | StandardMenuIconKind::Reduced => resource_id
            .and_then(|resource_id| {
                ppc_menu_resource_data(resources, current_resource_refnum, b"ICON", resource_id)
            })
            .filter(|data| data.len() >= 128)
            .map(|data| PpcTrackedMenuIcon::Icon {
                data: data.to_vec(),
                reduced: icon_kind == StandardMenuIconKind::Reduced,
            }),
    };
    PpcTrackedMenuItemAppearance {
        height: if is_separator {
            STANDARD_MENU_SEPARATOR_HEIGHT
        } else {
            icon_kind.row_height(QuickDrawTextStyle::from_bits(style))
        },
        icon_kind,
        icon: tracked_icon,
    }
}

pub(crate) fn ppc_menu_item_appearances(
    memory: &mut PpcSectionMem,
    menu_handle: u32,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Vec<PpcTrackedMenuItemAppearance> {
    let mut items = (1..=ppc_count_menu_items(memory, menu_handle) as i16)
        .map(|item| {
            (
                ppc_menu_item_appearance(
                    memory,
                    menu_handle,
                    item,
                    resources,
                    current_resource_refnum,
                ),
                ppc_menu_item_is_separator(memory, menu_handle, item),
            )
        })
        .collect::<Vec<_>>();
    let laid_out = laid_out_menu_item_count(&items, |(_appearance, separator)| *separator);
    items.truncate(laid_out);
    items
        .into_iter()
        .map(|(appearance, _separator)| appearance)
        .collect()
}

pub(crate) fn ppc_tracked_menu_item_height(
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    item: i16,
) -> i16 {
    ppc_tracked_menu_rows(state).height(item, 16)
}

pub(crate) fn ppc_tracked_menu_item_offset(
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    item: i16,
) -> i16 {
    ppc_tracked_menu_rows(state).offset(item)
}

pub(crate) fn ppc_menu_appearance_height(appearances: &[PpcTrackedMenuItemAppearance]) -> i16 {
    ppc_menu_rows_for_appearances(appearances).total_height()
}

pub(crate) fn ppc_tracked_menu_rows(
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
) -> MenuRows {
    ppc_menu_rows_for_appearances(state.item_appearances())
}

pub(crate) fn ppc_menu_rows_for_appearances(appearances: &[PpcTrackedMenuItemAppearance]) -> MenuRows {
    MenuRows::new(appearances.iter().map(|appearance| MenuRow {
        height: appearance.height,
        selectable: true,
    }))
}

#[cfg(test)]
pub(crate) fn ppc_calc_menu_size(memory: &mut PpcSectionMem, menu_handle: u32) {
    ppc_calc_menu_size_with_resources(memory, menu_handle, &[], 0);
}

pub(crate) fn ppc_calc_menu_size_with_resources(
    memory: &mut PpcSectionMem,
    menu_handle: u32,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) {
    let Some(menu) = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0) else {
        return;
    };
    let appearances =
        ppc_menu_item_appearances(memory, menu_handle, resources, current_resource_refnum);
    let width = standard_menu_width((1..=appearances.len()).filter_map(|item| {
        let item = i16::try_from(item).ok()?;
        let (address, len) = ppc_menu_item(memory, menu_handle, item)?;
        let text = ppc_memory_read_bytes(memory, address + 1, u32::from(len)).unwrap_or_default();
        let command = ppc_menu_item_attribute_address(memory, menu_handle, item, 1)
            .and_then(|address| memory.read_u8(address))
            .unwrap_or(0);
        let icon = appearances
            .get(usize::try_from(item - 1).ok()?)
            .map(|appearance| appearance.icon_kind.width())
            .unwrap_or(0);
        Some(StandardMenuItemWidth {
            text: standard_menu_text_advance(&text),
            icon,
            command,
        })
    }));
    let rows = ppc_menu_rows_for_appearances(&appearances);
    let height = standard_menu_height(
        &rows,
        (ppc_main_screen_height() as i16)
            .saturating_sub(memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16),
    );
    let _ = memory.write_u16_be(menu + 2, width as u16);
    let _ = memory.write_u16_be(menu + 4, height as u16);
}

pub(crate) fn ppc_dispatch_calc_menu_size(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> PpcImportAction {
    let menu_handle = cpu.gpr[3];
    if let Some(action) = ppc_dispatch_native_menu_definition(
        cpu,
        Some(process_memory_manager),
        memory,
        heap_cursor,
        heap_limit,
        resources,
        toolbox_startup,
        MenuDefinitionInvocation::size(menu_handle),
        cpu.lr,
    ) {
        return action;
    }
    ppc_calc_menu_size_with_resources(memory, menu_handle, resources, current_resource_refnum);
    PpcImportAction::ReturnPreserve
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dispatch_native_menu_definition(
    cpu: &mut PpcCpu,
    process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    resources: &[PpcVfsResourceRecord],
    toolbox_startup: &mut PpcToolboxStartupState,
    invocation: MenuDefinitionInvocation,
    final_pc: u32,
) -> Option<PpcImportAction> {
    ppc_dispatch_native_menu_definition_with_return(
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        resources,
        toolbox_startup,
        invocation,
        final_pc,
        PpcNativeReturnGpr3::Preserve,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dispatch_native_menu_definition_with_return(
    cpu: &mut PpcCpu,
    mut process_memory_manager: Option<&mut ProcessNativeMemoryManager>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    _heap_limit: u32,
    resources: &[PpcVfsResourceRecord],
    toolbox_startup: &mut PpcToolboxStartupState,
    invocation: MenuDefinitionInvocation,
    final_pc: u32,
    return_gpr3: PpcNativeReturnGpr3,
) -> Option<PpcImportAction> {
    let menu_build = toolbox_startup.execution.calls().menu_bar_build();
    let tracking_root = toolbox_startup.execution.menu().entry_id();
    let final_pc = if menu_build.is_some() && invocation.message == MenuDefinitionMessage::Size {
        PPC_GUEST_CALL_RETURN_PC
    } else {
        final_pc
    };
    let target = ppc_menu_definition_target(cpu, memory, resources, invocation.menu_handle)?;
    let manager = process_memory_manager.as_deref_mut()?;
    // The emulated callback owns its wrapper and stack as well as its
    // by-reference arguments. Nested calls must not reuse a live gateway.
    let workspace_size = match target.isa {
        GuestIsa::PowerPc => 10,
        GuestIsa::M68k => PPC_MIXED_MODE_M68K_STACK_SIZE + 96,
    };
    let scratch = manager.new_native_scratch(memory, workspace_size);
    if scratch == 0 {
        return None;
    }
    if let Some(heap) = manager.native_heap_state() {
        *heap_cursor = heap.heap_cursor;
    }
    let completion = crate::menu_manager::MenuDefinitionCompletion::pending();
    let operation = crate::menu_manager::MenuDefinitionOperation {
        scratch,
        completion: completion.clone(),
    };
    let prepared = (|| {
        memory.write_bytes(scratch, &invocation.scratch_bytes())?;
        let call = invocation.call(scratch);
        match target.isa {
            GuestIsa::PowerPc => {
                let effect = GuestCallEffect::call_guest(
                    GuestCallRequest::for_task(
                        toolbox_startup.execution.calls().current_task(),
                        GuestCallTarget {
                            isa: target.isa,
                            entry: target.entry,
                            rtoc: target.rtoc,
                        },
                    )
                    .with_powerpc_arguments(
                        crate::guest_call::PowerPcArguments::from_slice(&call.native_arguments())?,
                    ),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        final_pc,
                        cpu.gpr[2],
                        return_gpr3,
                    ),
                );
                toolbox_startup
                    .execution
                    .calls()
                    .activate_powerpc_effect_with_operation(
                        cpu,
                        memory,
                        effect,
                        Some(scratch),
                        Some(crate::guest_call::ManagerContinuation::Menu(
                            crate::guest_call::MenuManagerContinuation::Definition(operation),
                        )),
                    )
                    .then_some(PpcImportAction::Continue)
            }
            GuestIsa::M68k => ppc_begin_m68k_menu_definition(
                cpu,
                memory,
                toolbox_startup,
                target,
                call,
                final_pc,
                return_gpr3,
                operation,
            ),
        }
    })();
    if prepared.is_none() {
        manager.release_native_scratch(scratch);
        return None;
    }
    if let Some(id) = tracking_root {
        toolbox_startup.execution.bind_menu_definition_completion(
            id,
            invocation,
            completion.clone(),
        );
    }
    if invocation.message == MenuDefinitionMessage::Size {
        if let Some(id) = menu_build {
            toolbox_startup
                .execution
                .calls()
                .bind_menu_bar_build_completion(id, invocation.menu_handle, completion);
        }
    }
    prepared
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_begin_m68k_menu_definition(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    target: GuestProcedure,
    call: crate::menu_manager::MenuDefinitionCall,
    final_pc: u32,
    return_gpr3: PpcNativeReturnGpr3,
    operation: crate::menu_manager::MenuDefinitionOperation,
) -> Option<PpcImportAction> {
    // `MyMenuDef` is a no-result Pascal stack routine with parameter sizes
    // 2, 4, 4, 4, and 4 bytes. The Mixed Mode ProcInfo encoding is therefore
    // $0000FF80. Macintosh Toolbox Essentials (1992), pp. 3-148--3-151;
    // Inside Macintosh: PowerPC System Software (1994), pp. 2-12--2-16.
    if target.proc_info != 0 && target.proc_info != MenuDefinitionInvocation::PASCAL_PROC_INFO {
        return None;
    }
    let stack_top = operation
        .scratch
        .checked_add(PPC_MIXED_MODE_M68K_STACK_SIZE + 96)?;
    let return_pc = PPC_GUEST_CALL_RETURN_PC;
    let frame =
        crate::execution_m68k::M68kMenuDefinitionFrame::new(call, target.entry, stack_top, false)?;
    memory.write_bytes(frame.entry, &frame.image)?;
    memory.write_u32_be(frame.entry - 4, return_pc)?;

    let mut registers = crate::guest_call::M68kRegisterState::default();
    registers.address[5] = PPC_DATA_BASE;
    let effect = GuestCallEffect::call_guest(
        GuestCallRequest::for_task(
            startup.execution.calls().current_task(),
            GuestCallTarget {
                isa: target.isa,
                entry: target.entry,
                rtoc: target.rtoc,
            },
        )
        .with_m68k_request(crate::guest_call::M68kCallRequest {
            entry: frame.entry,
            initial_sp: frame.entry - 4,
            final_sp: frame.entry,
            registers,
            result: None,
        }),
        GuestCallContinuation::to_powerpc(return_pc, final_pc, cpu.gpr[2], return_gpr3),
    );
    if !startup.execution.calls().begin_m68k_operation(
        effect,
        operation.scratch,
        crate::guest_call::ManagerContinuation::Menu(
            crate::guest_call::MenuManagerContinuation::Definition(operation),
        ),
    ) {
        return None;
    }
    Some(PpcImportAction::Halt)
}

pub(crate) fn ppc_menu_definition_target(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    resources: &[PpcVfsResourceRecord],
    menu_handle: u32,
) -> Option<GuestProcedure> {
    let (proc_ptr, resource_backed) =
        ppc_custom_menu_definition_proc(memory, resources, menu_handle)?;
    let raw_isa = if resource_backed {
        GuestIsa::M68k
    } else {
        GuestIsa::PowerPc
    };
    let target = resolve_guest_procedure(
        memory,
        proc_ptr,
        cpu.gpr[2],
        None,
        GuestIsa::PowerPc,
        raw_isa,
    )?;
    memory.read_u16_be(target.entry)?;
    Some(target)
}

pub(crate) fn ppc_custom_menu_definition_proc(
    memory: &mut PpcSectionMem,
    resources: &[PpcVfsResourceRecord],
    menu_handle: u32,
) -> Option<(u32, bool)> {
    let menu_ptr = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    let menu_proc = memory.read_u32_be(menu_ptr + 6).unwrap_or(0);
    let proc_ptr = memory.read_u32_be(menu_proc).unwrap_or(0);
    let resource_id = resources
        .iter()
        .find(|resource| {
            resource.res_type == u32::from_be_bytes(*b"MDEF") && resource.handle == menu_proc
        })
        .map(|resource| resource.res_id);
    let is_standard = resource_id == Some(0)
        || ppc_memory_read_bytes(memory, proc_ptr, STANDARD_MENU_DEFINITION_SHIM.len() as u32)
            .is_some_and(|bytes| bytes == STANDARD_MENU_DEFINITION_SHIM);
    if is_standard || menu_proc == 0 || proc_ptr == 0 {
        return None;
    }
    Some((proc_ptr, resource_id.is_some()))
}

pub(crate) fn ppc_menu_uses_guest_definition(
    memory: &mut PpcSectionMem,
    resources: &[PpcVfsResourceRecord],
    menu_handle: u32,
) -> bool {
    ppc_custom_menu_definition_proc(memory, resources, menu_handle).is_some()
}

pub(crate) fn ppc_menu_item_enabled(memory: &mut PpcSectionMem, menu_handle: u32, item: i16) -> bool {
    let Some(menu) = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0) else {
        return false;
    };
    let flags = memory.read_u32_be(menu + 10).unwrap_or(0);
    flags & 1 != 0 && (item > 31 || item > 0 && flags & (1u32 << item) != 0)
}

pub(crate) fn ppc_menu_item_is_separator(memory: &mut PpcSectionMem, menu_handle: u32, item: i16) -> bool {
    ppc_menu_item(memory, menu_handle, item)
        .is_some_and(|(address, len)| len == 1 && memory.read_u8(address + 1) == Some(b'-'))
}

pub(crate) fn ppc_menu_item_is_selectable(memory: &mut PpcSectionMem, menu_handle: u32, item: i16) -> bool {
    ppc_menu_item_enabled(memory, menu_handle, item)
        && !ppc_menu_item_is_separator(memory, menu_handle, item)
}

pub(crate) fn ppc_guest_menu_snapshot(memory: &mut PpcSectionMem, menu_list_handle: u32) -> GuestMenuSnapshot {
    let menu_list = ppc_menu_list_definition(memory, menu_list_handle).unwrap_or_default();
    menu_list.guest_snapshot(|menu_handle| {
        let menu = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
        Some(MenuSnapshotRecord {
            id: memory.read_u16_be(menu)? as i16,
            title: ppc_read_pascal_string(memory, menu + 14)?,
            items: ppc_menu_items_from_memory(memory, menu_handle)?,
        })
    })
}

pub(crate) fn ppc_menu_selection_result(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    menu_id: i16,
    item_number: i16,
) -> Option<u32> {
    ppc_guest_menu_snapshot(memory, menu_list_handle).selectable_result(menu_id, item_number)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_step_menu_tracking(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    input: PpcInputSnapshot,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Option<PpcImportAction> {
    let action = ppc_step_menu_tracking_body(
        cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        gworlds,
        screen_clut,
        toolbox_startup,
        current_gworld,
        current_gdevice,
        input,
        vfs_resources,
        current_resource_refnum,
    )?;
    if matches!(action, PpcImportAction::Yield(_)) {
        let Some(key) = toolbox_startup
            .execution
            .request_menu_hook(input.mouse_button)
        else {
            return Some(action);
        };
        let pointer = memory.read_u32_be(0x0a30).unwrap_or(0);
        if let Some(target) = resolve_guest_procedure(
            memory,
            pointer,
            cpu.gpr[2],
            None,
            GuestIsa::PowerPc,
            GuestIsa::M68k,
        ) {
            if target.proc_info == 0 {
                let return_value = PpcNativeReturnGpr3::Set(cpu.gpr[3]);
                let operation = crate::guest_call::MenuHookOperation::pending(key);
                let callback = match target.isa {
                    GuestIsa::PowerPc => {
                        let effect = GuestCallEffect::call_guest(
                            GuestCallRequest::for_task(
                                toolbox_startup.execution.calls().current_task(),
                                GuestCallTarget {
                                    isa: target.isa,
                                    entry: target.entry,
                                    rtoc: target.rtoc,
                                },
                            )
                            .with_powerpc_arguments(
                                crate::guest_call::PowerPcArguments::from_slice(&[])?,
                            ),
                            GuestCallContinuation::to_powerpc(
                                PPC_GUEST_CALL_RETURN_PC,
                                PPC_GUEST_CALL_RETURN_PC,
                                cpu.gpr[2],
                                return_value,
                            ),
                        );
                        toolbox_startup.execution.calls()
                            .activate_powerpc_effect_with_operation(
                                cpu,
                                memory,
                                effect,
                                None,
                                Some(crate::guest_call::ManagerContinuation::Menu(
                                    crate::guest_call::MenuManagerContinuation::Hook(
                                        operation.clone(),
                                    ),
                                )),
                            )
                            .then_some(PpcImportAction::Continue)
                    }
                    GuestIsa::M68k => ppc_begin_m68k_universal_proc_with_operation(
                        cpu,
                        Some(process_memory_manager),
                        memory,
                        heap_cursor,
                        heap_limit,
                        toolbox_startup,
                        target,
                        0,
                        None,
                        Vec::new(),
                        PPC_GUEST_CALL_RETURN_PC,
                        return_value,
                        crate::guest_call::ManagerContinuation::Menu(
                            crate::guest_call::MenuManagerContinuation::Hook(operation.clone()),
                        ),
                    ),
                };
                if let Some(callback) = callback {
                    assert!(toolbox_startup
                        .execution
                        .bind_menu_hook(key, operation.completion.clone()));
                    ppc_preserve_menu_callback_port(
                        toolbox_startup,
                        *current_gworld,
                        *current_gdevice,
                    );
                    return Some(callback);
                }
            }
        }
    }
    Some(action)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_step_menu_tracking_body(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    input: PpcInputSnapshot,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Option<PpcImportAction> {
    let call = toolbox_startup.execution.menu().context().call?;
    let MenuTrackingOrigin::PowerPc {
        stack_pointer,
        return_address,
    } = call.origin
    else {
        return None;
    };
    cpu.gpr[1] = stack_pointer;
    cpu.lr = return_address;
    match call.request {
        MenuTrackingRequest::MenuSelect { initial_point } => cpu.gpr[3] = initial_point,
        MenuTrackingRequest::PopUp(request) => {
            cpu.gpr[3] = request.menu_handle;
            cpu.gpr[4] = request.anchor.0 as u16 as u32;
            cpu.gpr[5] = request.anchor.1 as u16 as u32;
            cpu.gpr[6] = request.requested_item as u16 as u32;
        }
    }
    cpu.pc = PPC_GUEST_CALL_RETURN_PC;
    let handles = process_memory_manager.native_handle_records().to_vec();
    let current_menu_list = ppc_current_menu_list(memory);
    match call.request {
        MenuTrackingRequest::PopUp(_) => {
            let menu_color_bytes = ppc_menu_color_table_bytes(memory, &handles);
            let menu_colors = MenuColorTable::new(&menu_color_bytes);
            if toolbox_startup.execution.menu().context().caller_isa() == Some(GuestIsa::M68k) {
                // A classic MenuSelect owns this process continuation. A
                // nested native call must not consume its origin ABI frame.
                Some(PpcImportAction::Return(0))
            } else if toolbox_startup.active_menu_definition().is_some() {
                if toolbox_startup
                    .execution
                    .menu()
                    .context()
                    .native_popup()
                    .is_some()
                {
                    Some(ppc_continue_custom_popup_menu_tracking(
                        cpu,
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        toolbox_startup,
                        current_gworld,
                        current_gdevice,
                        input,
                        vfs_resources,
                    ))
                } else {
                    Some(PpcImportAction::Return(0))
                }
            } else if toolbox_startup.execution.menu().is_none() {
                if let Some(action) = ppc_begin_custom_popup_menu_tracking(
                    cpu,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    toolbox_startup,
                    current_gworld,
                    current_gdevice,
                    input,
                    vfs_resources,
                ) {
                    Some(action)
                } else {
                    Some(ppc_dispatch_pop_up_menu_select(
                        cpu,
                        memory,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        toolbox_startup,
                        input,
                        vfs_resources,
                        current_resource_refnum,
                    ))
                }
            } else {
                Some(ppc_dispatch_pop_up_menu_select(
                    cpu,
                    memory,
                    gworlds,
                    screen_clut,
                    menu_colors,
                    toolbox_startup,
                    input,
                    vfs_resources,
                    current_resource_refnum,
                ))
            }
        }
        MenuTrackingRequest::MenuSelect { .. } => {
            let menu_color_bytes = ppc_menu_color_table_bytes(memory, &handles);
            let menu_colors = MenuColorTable::new(&menu_color_bytes);
            if toolbox_startup.execution.menu().context().caller_isa() == Some(GuestIsa::M68k) {
                // A classic MenuSelect owns this process continuation. A
                // nested native call must not consume its origin ABI frame.
                Some(PpcImportAction::Return(0))
            } else if toolbox_startup.active_menu_definition().is_some() {
                Some(ppc_continue_custom_menu_bar_tracking(
                    cpu,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    gworlds,
                    screen_clut,
                    menu_colors,
                    toolbox_startup,
                    current_gworld,
                    current_gdevice,
                    input,
                    vfs_resources,
                    current_resource_refnum,
                ))
            } else if toolbox_startup
                .execution
                .menu()
                .as_ref()
                .is_some_and(|state| state.kind == MenuTrackingKind::MenuBar && state.is_flashing())
            {
                let mut state = toolbox_startup.execution.take_menu_state().unwrap();
                let step = state.advance_flash_at(
                    memory
                        .read_u32_be(crate::memory::globals::addr::TICKS)
                        .unwrap_or(0),
                );
                if matches!(step, MenuFlashStep::Wait | MenuFlashStep::Inactive) {
                    toolbox_startup.execution.set_menu_state(Some(state));
                    return Some(PpcImportAction::Yield(u64::MAX));
                }
                if let MenuFlashStep::Complete(result) = step {
                    toolbox_startup.execution.set_menu_state(Some(state));
                    let result = ppc_complete_menu_bar_tracking_with_colors(
                        memory,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        toolbox_startup,
                        result,
                    )
                    .unwrap_or(result);
                    ppc_restore_menu_definition_port(
                        toolbox_startup,
                        current_gworld,
                        current_gdevice,
                    );
                    return Some(PpcImportAction::Return(result));
                }
                if let Some(selected) = ppc_deepest_highlighted_menu_item(&state) {
                    ppc_redraw_standard_menu_tracking_flash(
                        memory,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        &state,
                        selected,
                        step == MenuFlashStep::Highlight(true),
                    );
                }
                toolbox_startup.execution.set_menu_state(Some(state));
                Some(PpcImportAction::Yield(u64::MAX))
            } else if toolbox_startup
                .execution
                .menu()
                .as_ref()
                .is_some_and(|state| state.kind != MenuTrackingKind::MenuBar)
            {
                Some(PpcImportAction::Return(0))
            } else if let Some((menu_id, item_number)) =
                toolbox_startup.pending_native_menu_selection.take()
            {
                let result =
                    ppc_menu_selection_result(memory, current_menu_list, menu_id, item_number)
                        .unwrap_or(0);
                let result = ppc_complete_menu_bar_tracking_with_colors(
                    memory,
                    gworlds,
                    screen_clut,
                    menu_colors,
                    toolbox_startup,
                    result,
                )
                .unwrap_or_else(|| {
                    ppc_set_menu_command_highlight_with_colors(
                        memory,
                        gworlds,
                        current_menu_list,
                        result,
                        None,
                        screen_clut,
                        menu_colors,
                        toolbox_startup.host_menu_bar_hidden,
                    );
                    result
                });
                ppc_restore_menu_definition_port(toolbox_startup, current_gworld, current_gdevice);
                Some(PpcImportAction::Return(result))
            } else if input.mouse_button {
                if toolbox_startup.execution.menu().is_none() {
                    if let Some(action) = ppc_begin_custom_menu_bar_tracking(
                        cpu,
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        toolbox_startup,
                        current_gworld,
                        current_gdevice,
                        cpu.gpr[3],
                        vfs_resources,
                    ) {
                        return Some(action);
                    }
                }
                ppc_track_menu_while_held_with_resources(
                    memory,
                    gworlds,
                    screen_clut,
                    menu_colors,
                    toolbox_startup,
                    cpu.gpr[3],
                    input,
                    vfs_resources,
                    current_resource_refnum,
                );
                if let Some(invocation) = toolbox_startup
                    .active_menu_definition()
                    .and_then(MenuDefinitionTracking::pending_invocation)
                {
                    toolbox_startup.execution.with_menu_context_mut(|context| {
                        context
                            .call
                            .get_or_insert(ppc_menu_select_call(cpu, cpu.gpr[3]));
                    });
                    ppc_prepare_menu_definition_port(
                        toolbox_startup,
                        current_gworld,
                        current_gdevice,
                    );
                    if let Some(action) = ppc_dispatch_native_menu_definition(
                        cpu,
                        Some(process_memory_manager),
                        memory,
                        heap_cursor,
                        heap_limit,
                        vfs_resources,
                        toolbox_startup,
                        invocation,
                        cpu.pc,
                    ) {
                        return Some(action);
                    }
                    if let Some(state) = toolbox_startup.execution.take_menu_state() {
                        ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                    }
                    toolbox_startup.clear_active_menu_definition();
                    toolbox_startup
                        .execution
                        .with_menu_context_mut(|context| context.clear_native_menu());
                    ppc_restore_menu_definition_port(
                        toolbox_startup,
                        current_gworld,
                        current_gdevice,
                    );
                    return Some(PpcImportAction::Return(0));
                }
                if toolbox_startup
                    .execution
                    .menu()
                    .as_ref()
                    .is_some_and(|state| state.kind == MenuTrackingKind::MenuBar)
                {
                    Some(PpcImportAction::Yield(u64::MAX))
                } else {
                    Some(PpcImportAction::Return(0))
                }
            } else {
                // MenuSelect owns one interaction from the supplied mouse-down
                // point until release. Even when the host observes release on
                // this first execution slice, create and finish the same
                // retained tracking state rather than deriving an item from a
                // separate fixed-height shortcut. Macintosh Toolbox Essentials
                // (1992), pp. 3-114--3-116.
                let mut tracking_updated = false;
                if toolbox_startup.execution.menu().is_none() {
                    if let Some(action) = ppc_begin_custom_menu_bar_tracking(
                        cpu,
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        toolbox_startup,
                        current_gworld,
                        current_gdevice,
                        cpu.gpr[3],
                        vfs_resources,
                    ) {
                        return Some(action);
                    }
                    ppc_track_menu_while_held_with_resources(
                        memory,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        toolbox_startup,
                        cpu.gpr[3],
                        input,
                        vfs_resources,
                        current_resource_refnum,
                    );
                    tracking_updated = true;
                }
                if !tracking_updated {
                    if let Some(mut state) = toolbox_startup.execution.take_menu_state() {
                        ppc_update_menu_tracking(
                            memory,
                            gworlds,
                            screen_clut,
                            menu_colors,
                            current_menu_list,
                            &mut state,
                            input,
                            vfs_resources,
                            current_resource_refnum,
                        );
                        toolbox_startup.execution.set_menu_state(Some(state));
                    }
                }
                if let Some(state) = toolbox_startup.execution.menu().as_ref() {
                    if let Some((menu_handle, item)) = ppc_tracked_menu_selection(memory, state) {
                        let menu_id = memory
                            .read_u32_be(menu_handle)
                            .filter(|ptr| *ptr != 0)
                            .and_then(|menu| memory.read_u16_be(menu))
                            .unwrap_or(0);
                        let result = (u32::from(menu_id) << 16) | u32::from(item as u16);
                        if result != 0 {
                            let tick = memory
                                .read_u32_be(crate::memory::globals::addr::TICKS)
                                .unwrap_or(0);
                            let flashes = memory
                                .read_u16_be(crate::memory::globals::addr::MENU_FLASH)
                                .unwrap_or(crate::memory::globals::DEFAULT_MENU_FLASH_COUNT);
                            if toolbox_startup
                                .execution
                                .begin_menu_flash(tick, flashes, result)
                                .unwrap()
                            {
                                return Some(PpcImportAction::Yield(u64::MAX));
                            }
                        }
                    }
                }
                let result = ppc_finish_menu_bar_tracking_with_colors(
                    memory,
                    gworlds,
                    screen_clut,
                    menu_colors,
                    toolbox_startup,
                    input,
                )
                .unwrap_or_else(|| {
                    ppc_set_menu_command_highlight_with_colors(
                        memory,
                        gworlds,
                        current_menu_list,
                        0,
                        None,
                        screen_clut,
                        menu_colors,
                        toolbox_startup.host_menu_bar_hidden,
                    );
                    0
                });
                ppc_restore_menu_definition_port(toolbox_startup, current_gworld, current_gdevice);
                Some(PpcImportAction::Return(result))
            }
        }
    }
}

pub(crate) fn ppc_menu_select_call(cpu: &PpcCpu, initial_point: u32) -> MenuTrackingCall {
    MenuTrackingCall {
        request: MenuTrackingRequest::MenuSelect { initial_point },
        origin: MenuTrackingOrigin::PowerPc {
            stack_pointer: cpu.gpr[1],
            return_address: cpu.lr,
        },
    }
}

pub(crate) fn ppc_popup_menu_call(cpu: &PpcCpu) -> MenuTrackingCall {
    MenuTrackingCall {
        request: MenuTrackingRequest::PopUp(PopupMenuRequest {
            menu_handle: cpu.gpr[3],
            anchor: (cpu.gpr[4] as u16 as i16, cpu.gpr[5] as u16 as i16),
            requested_item: cpu.gpr[6] as u16 as i16,
        }),
        origin: MenuTrackingOrigin::PowerPc {
            stack_pointer: cpu.gpr[1],
            return_address: cpu.lr,
        },
    }
}

pub(crate) fn ppc_popup_menu_is_inserted(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    menu_handle: u32,
) -> bool {
    ppc_menu_list_definition(memory, menu_list_handle).is_some_and(|menu_list| {
        menu_list
            .hierarchical_handles()
            .any(|candidate| candidate == menu_handle)
    })
}

pub(crate) fn ppc_popup_menu_layout_with_resources(
    memory: &mut PpcSectionMem,
    request: PopupMenuRequest,
    front: PpcFrontBuffer,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Option<(i16, i16, i16, i16, i16, i16)> {
    let menu_handle = request.menu_handle;
    let menu = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    let count = i16::try_from(ppc_count_menu_items(memory, menu_handle)).ok()?;
    if count <= 0 || front.width <= 1 || front.height <= 1 {
        return None;
    }

    ppc_calc_menu_size_with_resources(memory, menu_handle, resources, current_resource_refnum);
    let appearances =
        ppc_menu_item_appearances(memory, menu_handle, resources, current_resource_refnum);
    let rows = ppc_menu_rows_for_appearances(&appearances);
    let layout = standard_popup_menu_layout(
        &rows,
        i16::try_from(memory.read_u16_be(menu + 2)?.max(32)).ok()?,
        (
            ppc_u32_to_i16_saturating(front.width),
            ppc_u32_to_i16_saturating(front.height),
        ),
        request.anchor,
        request.requested_item,
    )?;

    Some((
        layout.left,
        layout.top,
        layout.width,
        layout.height,
        layout.highlighted_item,
        layout.content_top,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_begin_custom_popup_menu_tracking(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    input: PpcInputSnapshot,
    resources: &[PpcVfsResourceRecord],
) -> Option<PpcImportAction> {
    let menu_handle = cpu.gpr[3];
    let call = ppc_popup_menu_call(cpu);
    let menu_list = ppc_current_menu_list(memory);
    if !input.mouse_button || !ppc_popup_menu_is_inserted(memory, menu_list, menu_handle) {
        return None;
    }
    ppc_menu_definition_target(cpu, memory, resources, menu_handle)?;
    startup.execution.with_menu_context_mut(|context| {
        context.definition = Some(call.popup_request().unwrap().begin_definition());
        context.call.get_or_insert(call);
    });
    ppc_prepare_menu_definition_port(startup, current_gworld, current_gdevice);
    let invocation = startup
        .active_menu_definition()
        .and_then(MenuDefinitionTracking::pending_invocation)?;
    let action = ppc_dispatch_native_menu_definition(
        cpu,
        Some(process_memory_manager),
        memory,
        heap_cursor,
        heap_limit,
        resources,
        startup,
        invocation,
        cpu.pc,
    );
    if action.is_none() {
        startup.clear_active_menu_definition();
        startup
            .execution
            .with_menu_context_mut(|context| context.clear_native_popup());
        ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
    }
    action
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_continue_custom_popup_menu_tracking(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    input: PpcInputSnapshot,
    resources: &[PpcVfsResourceRecord],
) -> PpcImportAction {
    let Some(call) = startup.execution.menu().context().native_popup() else {
        startup.clear_active_menu_definition();
        ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
        return PpcImportAction::Return(0);
    };
    let completed = match startup
        .with_active_menu_definition_mut(MenuDefinitionTracking::complete_callback)
        .transpose()
    {
        Ok(completed) => completed.flatten(),
        Err(()) => {
            if let Some(state) = startup.execution.take_menu_state() {
                if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                    .map(MenuTrackingSurface::from)
                    == state.front_buffer
                {
                    ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                }
            }
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(0);
        }
    };

    if startup
        .execution
        .menu()
        .as_ref()
        .is_some_and(|state| state.is_flashing())
    {
        let tick = memory
            .read_u32_be(crate::memory::globals::addr::TICKS)
            .unwrap_or(0);
        let step = startup.execution.advance_menu_flash(tick).unwrap();
        if matches!(step, MenuFlashStep::Wait | MenuFlashStep::Inactive) {
            return PpcImportAction::Yield(u64::MAX);
        }
        if let MenuFlashStep::Complete(result) = step {
            if let Some(state) = startup.execution.take_menu_state() {
                if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                    .map(MenuTrackingSurface::from)
                    == state.front_buffer
                {
                    ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                }
            }
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(result);
        }
        let invocation = startup
            .active_menu_definition()
            .and_then(MenuDefinitionTracking::pending_invocation)
            .unwrap();
        if let Some(action) = ppc_dispatch_native_menu_definition(
            cpu,
            Some(process_memory_manager),
            memory,
            heap_cursor,
            heap_limit,
            resources,
            startup,
            invocation,
            cpu.pc,
        ) {
            return action;
        }
    }

    if completed == Some(MenuDefinitionMessage::PopUp) {
        let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(0);
        };
        let definition = startup.active_menu_definition().unwrap().clone();
        let (top, left, bottom, right) = definition.menu_rect();
        let Some(mut state) = ppc_begin_tracked_menu_with_appearances(
            memory,
            front,
            MenuTrackingKind::PopUp,
            definition.menu_handle(),
            left,
            top,
            top,
            right.saturating_sub(left),
            bottom.saturating_sub(top),
            0,
            Vec::new(),
        ) else {
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(0);
        };
        if ppc_draw_tracked_menu_chrome(
            memory,
            gworlds,
            screen_clut,
            menu_colors,
            StandardMenuPaneKind::PopUp,
            &state,
        )
        .is_none()
        {
            ppc_restore_menu_tracking(memory, state.front_buffer, &state);
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(0);
        }
        state.definition = startup
            .execution
            .with_menu_context_mut(|context| context.definition.take());
        startup.execution.set_menu_state(Some(state));
        startup
            .with_active_menu_definition_mut(|definition| definition.draw())
            .unwrap();
        let invocation = startup
            .active_menu_definition()
            .and_then(MenuDefinitionTracking::pending_invocation)
            .unwrap();
        if let Some(action) = ppc_dispatch_native_menu_definition(
            cpu,
            Some(process_memory_manager),
            memory,
            heap_cursor,
            heap_limit,
            resources,
            startup,
            invocation,
            cpu.pc,
        ) {
            return action;
        }
    } else {
        let hit_point = (u32::from(input.mouse_v as u16) << 16) | u32::from(input.mouse_h as u16);
        let choose = startup
            .with_active_menu_definition_mut(|definition| definition.choose(hit_point))
            .flatten();
        if let Some(invocation) = choose {
            if let Some(action) = ppc_dispatch_native_menu_definition(
                cpu,
                Some(process_memory_manager),
                memory,
                heap_cursor,
                heap_limit,
                resources,
                startup,
                invocation,
                cpu.pc,
            ) {
                return action;
            }
        } else if input.mouse_button {
            return PpcImportAction::Yield(u64::MAX);
        } else if let Some(definition) = startup.active_menu_definition().cloned() {
            let item = definition.which_item();
            let menu_id = memory
                .read_u32_be(definition.menu_handle())
                .filter(|ptr| *ptr != 0)
                .and_then(|menu| memory.read_u16_be(menu))
                .unwrap_or(0);
            let result = if item > 0 {
                (u32::from(menu_id) << 16) | u32::from(item as u16)
            } else {
                0
            };
            if result != 0 {
                let tick = memory
                    .read_u32_be(crate::memory::globals::addr::TICKS)
                    .unwrap_or(0);
                let flashes = memory
                    .read_u16_be(crate::memory::globals::addr::MENU_FLASH)
                    .unwrap_or(crate::memory::globals::DEFAULT_MENU_FLASH_COUNT);
                let flash_enabled = startup
                    .execution
                    .begin_menu_flash(tick, flashes, result)
                    .unwrap();
                if !flash_enabled {
                    if let Some(state) = startup.execution.take_menu_state() {
                        if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                            .map(MenuTrackingSurface::from)
                            == state.front_buffer
                        {
                            ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                        }
                    }
                    startup.clear_active_menu_definition();
                    startup
                        .execution
                        .with_menu_context_mut(|context| context.clear_native_popup());
                    ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
                    cpu.lr = call.origin.return_address();
                    return PpcImportAction::Return(result);
                }
                return PpcImportAction::Yield(u64::MAX);
            }
            if let Some(state) = startup.execution.take_menu_state() {
                if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                    .map(MenuTrackingSurface::from)
                    == state.front_buffer
                {
                    ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                }
            }
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(result);
        }
    }

    if let Some(state) = startup.execution.take_menu_state() {
        if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
            .map(MenuTrackingSurface::from)
            == state.front_buffer
        {
            ppc_restore_menu_tracking(memory, state.front_buffer, &state);
        }
    }
    startup.clear_active_menu_definition();
    startup
        .execution
        .with_menu_context_mut(|context| context.clear_native_popup());
    ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
    cpu.lr = call.origin.return_address();
    PpcImportAction::Return(0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dispatch_pop_up_menu_select(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    startup: &mut PpcToolboxStartupState,
    input: PpcInputSnapshot,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> PpcImportAction {
    let menu_handle = cpu.gpr[3];
    let call = ppc_popup_menu_call(cpu);
    let current_menu_list = ppc_current_menu_list(memory);

    if let Some(state) = startup.execution.menu().as_ref() {
        if startup.execution.menu().context().caller_isa() == Some(GuestIsa::M68k) {
            // A classic MenuSelect owns this process continuation. A nested
            // native call must not consume its save-under or origin ABI frame.
            return PpcImportAction::Return(0);
        }
        if state.kind != MenuTrackingKind::PopUp
            || startup.execution.menu().context().native_popup() != Some(call)
            || state.menu_handle != menu_handle
        {
            // A callback may enter a different modal menu routine while an
            // outer import is parked. Never let that nested call consume the
            // outer routine's save-under or eventual result.
            return PpcImportAction::Return(0);
        }

        let live_front = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD);
        if live_front.map(MenuTrackingSurface::from) != state.front_buffer {
            // A changed PixMap/depth makes the saved coordinates unsafe to
            // write. Abandon the overlay without touching either buffer.
            startup.execution.set_menu_state(None);
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            return PpcImportAction::Return(0);
        }
        if !ppc_popup_menu_is_inserted(memory, current_menu_list, menu_handle) {
            let Some(state) = startup.execution.take_menu_state() else {
                return PpcImportAction::Return(0);
            };
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
            ppc_restore_menu_tracking(memory, state.front_buffer, &state);
            return PpcImportAction::Return(0);
        }

        if state.is_flashing() {
            let Some(mut state) = startup.execution.take_menu_state() else {
                return PpcImportAction::Return(0);
            };
            let step = state.advance_flash_at(
                memory
                    .read_u32_be(crate::memory::globals::addr::TICKS)
                    .unwrap_or(0),
            );
            if matches!(step, MenuFlashStep::Wait | MenuFlashStep::Inactive) {
                startup.execution.set_menu_state(Some(state));
                return PpcImportAction::Yield(u64::MAX);
            }
            if let MenuFlashStep::Complete(result) = step {
                startup
                    .execution
                    .with_menu_context_mut(|context| context.clear_native_popup());
                ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                return PpcImportAction::Return(result);
            }
            let visible_item = if step == MenuFlashStep::Highlight(true) {
                state.highlighted_item
            } else {
                0
            };
            ppc_redraw_tracked_menu(
                memory,
                gworlds,
                screen_clut,
                menu_colors,
                &state,
                visible_item,
            );
            startup.execution.set_menu_state(Some(state));
            return PpcImportAction::Yield(u64::MAX);
        }

        let Some(mut state) = startup.execution.take_menu_state() else {
            return PpcImportAction::Return(0);
        };
        // Popup tracking uses the same retained MenuRows state as MenuSelect.
        // In particular, this updates content_top and the scrolling globals
        // while the pointer sits on an indicator; deriving only a hit item
        // leaves long popup menus visually stuck at their initial viewport.
        ppc_update_menu_tracking(
            memory,
            gworlds,
            screen_clut,
            menu_colors,
            current_menu_list,
            &mut state,
            input,
            resources,
            current_resource_refnum,
        );
        let highlighted_item = state.highlighted_item;
        if !input.mouse_button {
            if highlighted_item == 0 {
                startup
                    .execution
                    .with_menu_context_mut(|context| context.clear_native_popup());
                ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                return PpcImportAction::Return(0);
            }
            let menu_id = memory
                .read_u32_be(menu_handle)
                .filter(|ptr| *ptr != 0)
                .and_then(|menu| memory.read_u16_be(menu))
                .unwrap_or(0);
            let result = (u32::from(menu_id) << 16) | u32::from(highlighted_item as u16);
            state.set_flash_tick(
                memory
                    .read_u32_be(crate::memory::globals::addr::TICKS)
                    .unwrap_or(0),
            );
            let flash_enabled = state.begin_flash(
                memory
                    .read_u16_be(crate::memory::globals::addr::MENU_FLASH)
                    .unwrap_or(crate::memory::globals::DEFAULT_MENU_FLASH_COUNT),
                result,
            );
            if !flash_enabled {
                startup
                    .execution
                    .with_menu_context_mut(|context| context.clear_native_popup());
                ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                return PpcImportAction::Return(result);
            }
            ppc_redraw_tracked_menu(
                memory,
                gworlds,
                screen_clut,
                menu_colors,
                &state,
                highlighted_item,
            );
            startup.execution.set_menu_state(Some(state));
            return PpcImportAction::Yield(u64::MAX);
        }

        ppc_redraw_tracked_menu(
            memory,
            gworlds,
            screen_clut,
            menu_colors,
            &state,
            highlighted_item,
        );
        startup.execution.set_menu_state(Some(state));
        return PpcImportAction::Yield(u64::MAX);
    }

    if !input.mouse_button || !ppc_popup_menu_is_inserted(memory, current_menu_list, menu_handle) {
        return PpcImportAction::Return(0);
    }
    let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
        return PpcImportAction::Return(0);
    };
    let Some((popup_left, popup_top, popup_width, popup_height, requested_item, content_top)) =
        ppc_popup_menu_layout_with_resources(
            memory,
            call.popup_request().unwrap(),
            front,
            resources,
            current_resource_refnum,
        )
    else {
        return PpcImportAction::Return(0);
    };
    let highlighted_item = if ppc_menu_item_is_selectable(memory, menu_handle, requested_item) {
        requested_item
    } else {
        0
    };
    let appearances =
        ppc_menu_item_appearances(memory, menu_handle, resources, current_resource_refnum);
    let Some(state) = ppc_begin_tracked_menu_with_appearances(
        memory,
        front,
        MenuTrackingKind::PopUp,
        menu_handle,
        popup_left,
        popup_top,
        content_top,
        popup_width,
        popup_height,
        highlighted_item,
        appearances,
    ) else {
        return PpcImportAction::Return(0);
    };
    let rows = ppc_menu_tracking_rows(memory, &state);
    ppc_write_menu_scrolling_globals(memory, &rows, content_top);
    ppc_draw_tracked_menu(
        memory,
        gworlds,
        screen_clut,
        menu_colors,
        StandardMenuPaneKind::PopUp,
        &state,
        highlighted_item,
    );
    startup.execution.set_menu_state(Some(state));
    startup
        .execution
        .with_menu_context_mut(|context| {
            context.call.get_or_insert(call);
        });
    PpcImportAction::Yield(u64::MAX)
}

pub(crate) fn ppc_menu_key(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    key: u8,
) -> Option<MenuKeySelection> {
    ppc_menu_list_definition(memory, menu_list_handle)?
        .menu_key_selection(key, |menu_handle| ppc_menu_key_menu(memory, menu_handle))
}

pub(crate) fn ppc_menu_key_menu(memory: &mut PpcSectionMem, menu_handle: u32) -> Option<MenuKeyMenu> {
    let menu = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    let flags = memory.read_u32_be(menu + 10)?;
    let items = (1..=ppc_count_menu_items(memory, menu_handle))
        .map(|item| {
            let item = item as i16;
            MenuKeyItem {
                command: ppc_menu_item_attribute_address(memory, menu_handle, item, 1)
                    .and_then(|address| memory.read_u8(address))
                    .unwrap_or(0),
                mark: ppc_menu_item_attribute_address(memory, menu_handle, item, 2)
                    .and_then(|address| memory.read_u8(address))
                    .unwrap_or(0),
                enabled: item > 31 || flags & (1u32 << item) != 0,
            }
        })
        .collect();
    Some(MenuKeyMenu {
        id: memory.read_u16_be(menu)? as i16,
        enabled: flags & 1 != 0,
        items,
    })
}

pub(crate) fn ppc_menu_handle_at_title_point(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    initial_point: u32,
) -> Option<(u32, i16)> {
    let initial_v = (initial_point >> 16) as u16 as i16;
    let initial_h = initial_point as u16 as i16;
    let menu_bar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
    if initial_v < 0 || initial_v >= menu_bar_height {
        return None;
    }
    let (menu_handle, title_left) = ppc_menu_list_definition(memory, menu_list_handle)?
        .regular_title_at_horizontal(initial_h)?;
    memory
        .read_u32_be(menu_handle)
        .filter(|menu| *menu != 0)
        .map(|_| (menu_handle, title_left))
}

#[cfg(test)]
pub(crate) fn ppc_menu_tracking_hit(
    memory: &mut PpcSectionMem,
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    input: PpcInputSnapshot,
) -> Option<i16> {
    let rows = ppc_menu_tracking_rows(memory, state);
    let rect = (
        state.popup_top(),
        state.popup_left(),
        state.popup_top().saturating_add(state.popup_height()),
        state.popup_left().saturating_add(state.popup_width()),
    );
    let inside = input.mouse_v >= rect.0
        && input.mouse_v < rect.2
        && input.mouse_h >= rect.1
        && input.mouse_h < rect.3;
    (inside
        || rows
            .pointer_scroll_direction(rect, state.content_top(), (input.mouse_v, input.mouse_h))
            .is_some())
    .then(|| rows.tracking_item_at_point(rect, state.content_top(), (input.mouse_v, input.mouse_h)))
}

pub(crate) fn ppc_menu_tracking_rows(
    memory: &mut PpcSectionMem,
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
) -> MenuRows {
    MenuRows::new(
        state
            .item_appearances()
            .iter()
            .enumerate()
            .map(|(index, appearance)| {
                let item = i16::try_from(index + 1).unwrap_or(i16::MAX);
                MenuRow {
                    height: appearance.height,
                    selectable: ppc_menu_item_is_selectable(memory, state.menu_handle(), item),
                }
            }),
    )
}

pub(crate) fn ppc_write_menu_scrolling_globals(memory: &mut PpcSectionMem, rows: &MenuRows, content_top: i16) {
    ppc_write_menu_scrolling_bounds(
        memory,
        content_top,
        content_top.saturating_add(rows.total_height()),
    );
}

pub(crate) fn ppc_write_menu_scrolling_bounds(
    memory: &mut PpcSectionMem,
    content_top: i16,
    content_bottom: i16,
) {
    let _ = memory.write_u16_be(
        crate::memory::globals::addr::TOP_MENU_ITEM,
        content_top as u16,
    );
    let _ = memory.write_u16_be(
        crate::memory::globals::addr::AT_MENU_BOTTOM,
        content_bottom as u16,
    );
}

pub(crate) fn ppc_write_standard_menu_choice(memory: &mut PpcSectionMem, menu_handle: u32, item_number: i16) {
    let Some(menu_id) = memory
        .read_u32_be(menu_handle)
        .filter(|menu| *menu != 0)
        .and_then(|menu| memory.read_u16_be(menu))
        .map(|menu_id| menu_id as i16)
    else {
        return;
    };
    let _ = memory.write_u32_be(
        crate::memory::globals::addr::MENU_DISABLE,
        menu_choice_value(menu_id, item_number),
    );
}

#[cfg(test)]
pub(crate) fn ppc_menu_tracking_item(
    memory: &mut PpcSectionMem,
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    input: PpcInputSnapshot,
) -> i16 {
    ppc_menu_tracking_hit(memory, state, input).unwrap_or(0)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_begin_tracked_menu_with_appearances(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    kind: MenuTrackingKind,
    menu_handle: u32,
    popup_left: i16,
    popup_top: i16,
    content_top: i16,
    popup_width: i16,
    popup_height: i16,
    highlighted_item: i16,
    item_appearances: Vec<PpcTrackedMenuItemAppearance>,
) -> Option<PpcMenuTracking> {
    if popup_left < 0 || popup_top < 0 || popup_width <= 0 || popup_height <= 0 {
        return None;
    }
    let saved_width = popup_width.checked_add(1)?;
    let saved_height = popup_height.checked_add(1)?;
    let saved_right = i32::from(popup_left).checked_add(i32::from(saved_width))?;
    let saved_bottom = i32::from(popup_top).checked_add(i32::from(saved_height))?;
    if saved_right > i32::try_from(front.width).ok()?
        || saved_bottom > i32::try_from(front.height).ok()?
    {
        return None;
    }
    let saved_len =
        usize::try_from(i32::from(saved_width).checked_mul(i32::from(saved_height))?).ok()?;
    let mut saved_pixels = Vec::with_capacity(saved_len);
    for y in 0..i32::from(saved_height) {
        for x in 0..i32::from(saved_width) {
            saved_pixels.push(ppc_quickdraw_read_pixel(
                memory,
                front,
                (i32::from(popup_left) + x, i32::from(popup_top) + y),
            )?);
        }
    }
    let mut saved_pixels = crate::memory::SavedPixels::from(saved_pixels);
    for y in 0..i32::from(saved_height) {
        for x in 0..i32::from(saved_width) {
            ppc_capture_saved_detail(
                memory,
                front,
                (i32::from(popup_left) + x, i32::from(popup_top) + y),
                &mut saved_pixels,
                (y * i32::from(saved_width) + x) as usize,
            );
        }
    }
    Some(PpcMenuTracking {
        kind,
        menu_handle,
        popup_left,
        popup_top,
        content_top,
        scroll_direction: None,
        popup_width,
        popup_height,
        highlighted_item,
        definition: None,
        flash_remaining: 0,
        flash_tick: None,
        flash_deadline: 0,
        flash_result: 0,
        saved_width,
        saved_height,
        front_buffer: Some(front.into()),
        saved_pixels: saved_pixels.into(),
        item_appearances,
        submenus: Vec::new(),
    })
}

pub(crate) fn ppc_restore_tracked_menu(
    memory: &mut PpcSectionMem,
    surface: Option<MenuTrackingSurface>,
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
) {
    let Some(surface) = surface else {
        return;
    };
    if Some(surface) != state.front_buffer() {
        return;
    }
    let front = PpcFrontBuffer::from(surface);
    let mut index = 0usize;
    for y in 0..i32::from(state.saved_height()) {
        for x in 0..i32::from(state.saved_width()) {
            if let Some(pixel) = state.saved_pixels().get(index).copied() {
                let _ = ppc_quickdraw_write_raw_pixel(
                    memory,
                    front,
                    (
                        i32::from(state.popup_left()) + x,
                        i32::from(state.popup_top()) + y,
                    ),
                    pixel,
                );
                ppc_restore_saved_detail(
                    memory,
                    front,
                    (
                        i32::from(state.popup_left()) + x,
                        i32::from(state.popup_top()) + y,
                    ),
                    state.saved_pixels(),
                    index,
                );
            }
            index += 1;
        }
    }
}

pub(crate) fn ppc_restore_menu_tracking(
    memory: &mut PpcSectionMem,
    surface: Option<MenuTrackingSurface>,
    state: &PpcMenuTracking,
) {
    for submenu in state.submenus.iter().rev() {
        ppc_restore_tracked_menu(memory, surface, submenu);
    }
    ppc_restore_tracked_menu(memory, surface, state);
}


pub(crate) fn ppc_menu_rgb(rgb: [u16; 3]) -> PpcRgbColor {
    PpcRgbColor {
        red: rgb[0],
        green: rgb[1],
        blue: rgb[2],
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_draw_tracked_menu_icon(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    screen_clut: &[[u16; 3]; 256],
    icon: &PpcTrackedMenuIcon,
    top: i16,
    left: i16,
    content_pixel: u16,
) {
    // The shared standard-MDEF sampler owns monochrome resource validation,
    // reduced-ICON scaling, and SICN first-image selection. This adapter owns
    // retained resource bytes and framebuffer writes. Macintosh Toolbox
    // Essentials (1992), pp. 3-45--3-46 and 3-62--3-63.
    let monochrome = match icon {
        PpcTrackedMenuIcon::Icon { data, reduced } => Some((
            data,
            if *reduced {
                StandardMenuIconKind::Reduced
            } else {
                StandardMenuIconKind::Normal
            },
        )),
        PpcTrackedMenuIcon::SmallIcon(data) => Some((data, StandardMenuIconKind::Small)),
        PpcTrackedMenuIcon::CIcon(_) => None,
    };
    if let Some((data, kind)) = monochrome {
        let Some(layout) = MonochromeMenuIconLayout::for_kind(kind, Some(data.len())) else {
            return;
        };
        for y in 0..layout.height {
            for x in 0..layout.width {
                if layout
                    .sample_with(|offset| data.get(offset).copied(), x, y)
                    .unwrap_or(false)
                {
                    let _ = ppc_quickdraw_write_raw_pixel(
                        memory,
                        front,
                        (i32::from(left) + x as i32, i32::from(top) + y as i32),
                        content_pixel,
                    );
                }
            }
        }
        return;
    }

    // Compiled color icons retain their ColorTable mapping through the active
    // device table. Imaging With QuickDraw (1994), pp. 4-105--4-106.
    match icon {
        PpcTrackedMenuIcon::CIcon(data) => {
            let Some(layout) = ColorIconLayout::decode(data) else {
                return;
            };
            for y in 0..usize::try_from(layout.height).unwrap_or_default() {
                for x in 0..usize::try_from(layout.width).unwrap_or_default() {
                    if !layout
                        .mask_bit_with(|offset| data.get(offset).copied(), x, y)
                        .unwrap_or(false)
                    {
                        continue;
                    }
                    let pixel = if front.depth == 1 {
                        layout
                            .monochrome_bit_with(|offset| data.get(offset).copied(), x, y)
                            .filter(|set| *set)
                            .map(|_| content_pixel)
                    } else {
                        layout
                            .rgb_with(|offset| data.get(offset).copied(), x, y)
                            .map(|[red, green, blue]| PpcRgbColor { red, green, blue })
                            .and_then(|rgb| {
                                ppc_physical_screen_color_pixel(front, rgb, screen_clut)
                            })
                    };
                    if let Some(pixel) = pixel {
                        let _ = ppc_quickdraw_write_raw_pixel(
                            memory,
                            front,
                            (i32::from(left) + x as i32, i32::from(top) + y as i32),
                            pixel,
                        );
                    }
                }
            }
        }
        PpcTrackedMenuIcon::Icon { .. } | PpcTrackedMenuIcon::SmallIcon(_) => unreachable!(),
    }
}

/// Draw the Menu Manager-owned structure and erase a menu's contents before
/// either the standard or an application-defined MDEF draws its items.
/// Macintosh Toolbox Essentials (1992), pp. 3-90 and 3-148--3-150.
pub(crate) fn ppc_draw_tracked_menu_chrome(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    kind: StandardMenuPaneKind,
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
) -> Option<(PpcFrontBuffer, u16, u16)> {
    let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
        return None;
    };
    let menu_id = memory
        .read_u32_be(state.menu_handle())
        .filter(|ptr| *ptr != 0)
        .and_then(|menu| memory.read_u16_be(menu))
        .unwrap_or(0) as i16;
    let theme = ppc_ui_theme(gworlds);
    let background_rgb = if theme == UiThemeId::ClassicSystem7 {
        ppc_menu_rgb(menu_colors.dropdown_background(menu_id))
    } else {
        ppc_theme_rgb(theme.provider().palette().window_background)
    };
    let (Some(background), Some(black)) = (
        ppc_physical_screen_color_pixel(front, background_rgb, screen_clut),
        ppc_physical_screen_color_pixel(front, PPC_RGB_BLACK, screen_clut),
    ) else {
        return None;
    };
    let chrome = StandardMenuChrome::new(kind, state.dropdown_rect())?;
    for y in 0..i32::from(state.popup_height()) {
        for x in 0..i32::from(state.popup_width()) {
            let _ = ppc_quickdraw_write_raw_pixel(
                memory,
                front,
                (
                    i32::from(state.popup_left()) + x,
                    i32::from(state.popup_top()) + y,
                ),
                background,
            );
        }
    }
    chrome.for_each_frame_pixel(|x, y| {
        let _ = ppc_quickdraw_write_raw_pixel(memory, front, (i32::from(x), i32::from(y)), black);
    });
    // Macintosh Toolbox Essentials (1992), pp. 3-122--3-123: the standard
    // menu definition draws a one-pixel shadow below and to the right of the
    // menu. The saved rectangle includes those pixels so closing the menu is
    // byte-for-byte reversible even for packed destinations.
    chrome.for_each_shadow_pixel(|x, y| {
        let _ = ppc_quickdraw_write_raw_pixel(memory, front, (i32::from(x), i32::from(y)), black);
    });
    Some((front, background, black))
}

pub(crate) fn ppc_draw_tracked_menu(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    kind: StandardMenuPaneKind,
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    selected: i16,
) {
    let Some((front, background, black)) =
        ppc_draw_tracked_menu_chrome(memory, gworlds, screen_clut, menu_colors, kind, state)
    else {
        return;
    };
    let theme = ppc_ui_theme(gworlds);
    let palette = theme.provider().palette();
    let rect = state.dropdown_rect();
    let rows = ppc_menu_tracking_rows(memory, state);
    let (scroll_up, scroll_down) = rows.scroll_indicators(rect, state.content_top());
    let visible_item_top = state
        .popup_top()
        .saturating_add(if scroll_up { 16 } else { 0 });
    let visible_item_bottom = state
        .popup_top()
        .saturating_add(state.popup_height())
        .saturating_sub(if scroll_down { 16 } else { 0 });
    for y in 1..i32::from(state.popup_height()).saturating_sub(1) {
        let absolute_y = state
            .popup_top()
            .saturating_add(i16::try_from(y).unwrap_or(i16::MAX));
        let indicator = (scroll_up && absolute_y < visible_item_top)
            || (scroll_down && absolute_y >= visible_item_bottom);
        if indicator {
            for x in 1..i32::from(state.popup_width()).saturating_sub(1) {
                let _ = ppc_quickdraw_write_raw_pixel(
                    memory,
                    front,
                    (
                        i32::from(state.popup_left()) + x,
                        i32::from(state.popup_top()) + y,
                    ),
                    black,
                );
            }
        }
    }
    let popup_bottom = state.popup_top().saturating_add(state.popup_height());
    for item in 1..=i16::try_from(state.item_appearances().len()).unwrap_or(i16::MAX) {
        let Some((address, len)) = ppc_menu_item(memory, state.menu_handle(), item) else {
            continue;
        };
        let text = ppc_memory_read_bytes(memory, address + 1, u32::from(len)).unwrap_or_default();
        let row_top = state
            .content_top()
            .saturating_add(ppc_tracked_menu_item_offset(state, item));
        let row_height = ppc_tracked_menu_item_height(state, item);
        if row_top.saturating_add(row_height) <= state.popup_top() {
            continue;
        }
        if row_top >= popup_bottom.saturating_sub(1) {
            break;
        }
        if row_top < visible_item_top || row_top.saturating_add(row_height) > visible_item_bottom {
            continue;
        }
        let row_bottom = row_top.saturating_add(row_height).min(popup_bottom - 1);
        let command = memory.read_u8(address + 2 + u32::from(len)).unwrap_or(0);
        let mark = memory.read_u8(address + 3 + u32::from(len)).unwrap_or(0);
        let style = memory.read_u8(address + 4 + u32::from(len)).unwrap_or(0);
        let menu_id = memory
            .read_u32_be(state.menu_handle())
            .filter(|ptr| *ptr != 0)
            .and_then(|menu| memory.read_u16_be(menu))
            .unwrap_or(0) as i16;
        let item_colors = menu_colors.item_colors(menu_id, item);
        let is_separator = text == b"-";
        let is_hierarchical = command == 0x1b && mark != 0;
        let has_command_key = command > 0x20;
        let dimmed = !ppc_menu_item_enabled(memory, state.menu_handle(), item) || is_separator;
        let highlighted = item == selected && !dimmed;
        let dimmed_color = MenuColorTable::dimmed(item_colors.name, item_colors.background);
        let component = |foreground: [u16; 3]| {
            let rgb = if theme != UiThemeId::ClassicSystem7 {
                ppc_theme_components(if highlighted {
                    palette.frame_light
                } else if dimmed {
                    palette.accent
                } else {
                    palette.frame_dark
                })
            } else if highlighted {
                item_colors.background
            } else if dimmed && front.depth != 1 {
                dimmed_color
            } else {
                foreground
            };
            let rgb = ppc_menu_rgb(rgb);
            let pixel = ppc_physical_screen_color_pixel(front, rgb, screen_clut).unwrap_or(black);
            let explicit =
                ppc_indexed_depth_entry_count(front.depth).and_then(|_| u8::try_from(pixel).ok());
            (rgb, pixel, explicit)
        };
        let (mark_color, _mark_pixel, mark_index) = component(item_colors.mark);
        let (name_color, name_pixel, name_index) = component(item_colors.name);
        let (command_color, command_pixel, command_index) = component(item_colors.command);
        if highlighted {
            let selected_background = ppc_physical_screen_color_pixel(
                front,
                if theme == UiThemeId::ClassicSystem7 {
                    ppc_menu_rgb(item_colors.name)
                } else {
                    ppc_theme_rgb(palette.selection)
                },
                screen_clut,
            )
            .unwrap_or(black);
            for y in row_top..row_bottom {
                for x in state.popup_left().saturating_add(1)
                    ..state
                        .popup_left()
                        .saturating_add(state.popup_width())
                        .saturating_sub(1)
                {
                    let _ = ppc_quickdraw_write_raw_pixel(
                        memory,
                        front,
                        (i32::from(x), i32::from(y)),
                        selected_background,
                    );
                }
            }
        }
        let metrics = get_font_metrics(PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM);
        let appearance = usize::try_from(item.saturating_sub(1))
            .ok()
            .and_then(|index| state.item_appearances().get(index));
        let layout = standard_menu_item_layout(
            (
                state.popup_left(),
                state.popup_left().saturating_add(state.popup_width()),
            ),
            (row_top, row_height),
            appearance
                .map(|appearance| appearance.icon_kind)
                .unwrap_or(StandardMenuIconKind::None),
            mark != 0,
            (metrics.ascent, metrics.descent),
            kind == StandardMenuPaneKind::PullDown,
        );
        let text_baseline = layout.text_baseline;

        if is_separator {
            let separator_y = layout.separator_y;
            for x in 1..state.popup_width().saturating_sub(1) {
                if front.depth == 1
                    && !standard_menu_gray_pattern_is_ink(
                        state.popup_left().saturating_add(x),
                        separator_y,
                    )
                {
                    continue;
                }
                let _ = ppc_quickdraw_write_raw_pixel(
                    memory,
                    front,
                    (
                        i32::from(state.popup_left().saturating_add(x)),
                        i32::from(separator_y),
                    ),
                    name_pixel,
                );
            }
            continue;
        }

        // Standard menu items reserve a left mark/icon column. A submenu's
        // mark byte is its menu ID, so it is represented by the triangle on
        // the right instead of being drawn as a marking character.
        // Macintosh Toolbox Essentials (1992), pp. 3-12--3-13.
        if mark != 0 && !is_hierarchical {
            let mark_char = if mark == 0x12 {
                '\u{2713}'
            } else {
                char::from(mark)
            };
            ppc_draw_text_chars(
                memory,
                gworlds,
                PPC_MAIN_GWORLD,
                (layout.mark_left, text_baseline),
                PPC_QD_TEXT_FONT_DEFAULT,
                PPC_QD_TEXT_SIZE_SYSTEM,
                PPC_QD_TEXT_MODE_SRC_OR,
                mark_color,
                mark_index,
                None,
                std::iter::once(mark_char),
            );
        }

        if let Some(icon) = appearance.and_then(|appearance| appearance.icon.as_ref()) {
            ppc_draw_tracked_menu_icon(
                memory,
                front,
                screen_clut,
                icon,
                row_top,
                layout.icon_left,
                name_pixel,
            );
        }

        let mode = PPC_QD_TEXT_MODE_SRC_OR;
        let _ = ppc_draw_text_bytes_styled(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            (layout.text_left, text_baseline),
            PPC_QD_TEXT_FONT_DEFAULT,
            PPC_QD_TEXT_SIZE_SYSTEM,
            mode,
            name_color,
            name_index,
            style,
            &text,
        );

        if is_hierarchical {
            for_each_standard_hierarchy_indicator_pixel(
                layout.indicator_left,
                layout.indicator_mid_y,
                |x, y| {
                    let _ = ppc_quickdraw_write_raw_pixel(
                        memory,
                        front,
                        (i32::from(x), i32::from(y)),
                        command_pixel,
                    );
                },
            );
        } else if has_command_key {
            ppc_draw_text_chars(
                memory,
                gworlds,
                PPC_MAIN_GWORLD,
                (layout.command_left, text_baseline),
                PPC_QD_TEXT_FONT_DEFAULT,
                PPC_QD_TEXT_SIZE_SYSTEM,
                mode,
                command_color,
                command_index,
                None,
                ['\u{2318}', char::from(command)],
            );
        }

        // On a one-bit device the standard MDEF dims the complete item with
        // the 50-percent gray pattern because no intermediate color exists.
        // Macintosh Toolbox Essentials (1992), pp. 3-13 and 3-150.
        if dimmed && front.depth == 1 {
            for y in row_top..row_bottom {
                for x in state.popup_left().saturating_add(1)
                    ..state
                        .popup_left()
                        .saturating_add(state.popup_width())
                        .saturating_sub(1)
                {
                    if !standard_menu_gray_pattern_is_ink(x, y) {
                        let _ = ppc_quickdraw_write_raw_pixel(
                            memory,
                            front,
                            (i32::from(x), i32::from(y)),
                            background,
                        );
                    }
                }
            }
        }
    }

    // Scrolling indicators replace the first/last visible item positions.
    // Inside Macintosh Volume V (1986), pp. V-248--V-249.
    let center_x = state.popup_left().saturating_add(state.popup_width() / 2);
    if scroll_up {
        for_each_standard_scroll_up_indicator_pixel(center_x, state.popup_top(), |x, y| {
            let _ =
                ppc_quickdraw_write_raw_pixel(memory, front, (i32::from(x), i32::from(y)), black);
        });
    }
    if scroll_down {
        for_each_standard_scroll_down_indicator_pixel(
            center_x,
            state.popup_top().saturating_add(state.popup_height()),
            |x, y| {
                let _ = ppc_quickdraw_write_raw_pixel(
                    memory,
                    front,
                    (i32::from(x), i32::from(y)),
                    black,
                );
            },
        );
    }
}

pub(crate) fn ppc_redraw_tracked_menu(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    state: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    visible_item: i16,
) -> bool {
    let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
        return false;
    };
    if Some(MenuTrackingSurface::from(front)) != state.front_buffer() {
        return false;
    }
    ppc_restore_tracked_menu(memory, Some(front.into()), state);
    ppc_draw_tracked_menu(
        memory,
        gworlds,
        screen_clut,
        menu_colors,
        StandardMenuPaneKind::PopUp,
        state,
        visible_item,
    );
    true
}

pub(crate) fn ppc_submenu_handle_for_item(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    parent_menu_handle: u32,
    parent_item: i16,
) -> Option<u32> {
    let parent_items = ppc_menu_items_from_memory(memory, parent_menu_handle)?;
    ppc_menu_list_definition(memory, menu_list_handle)?.submenu_handle_for_item(
        &parent_items,
        parent_item,
        |handle| {
            memory
                .read_u32_be(handle)
                .filter(|menu| *menu != 0)
                .and_then(|menu| memory.read_u16_be(menu))
                .map(|menu_id| menu_id as i16)
        },
    )
}

pub(crate) fn ppc_menu_item_is_hierarchical(memory: &mut PpcSectionMem, menu_handle: u32, item: i16) -> bool {
    ppc_menu_items_from_memory(memory, menu_handle)
        .is_some_and(|items| items.item_is_hierarchical(item))
}

#[cfg(test)]
pub(crate) fn ppc_begin_submenu_tracking(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    front: PpcFrontBuffer,
    parent: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    parent_item: i16,
) -> Option<PpcSubmenuTracking> {
    let menu_handle =
        ppc_submenu_handle_for_item(memory, menu_list_handle, parent.menu_handle(), parent_item)?;
    ppc_begin_submenu_tracking_with_resources(
        memory,
        front,
        parent,
        parent_item,
        menu_handle,
        &[],
        0,
    )
}

pub(crate) fn ppc_begin_submenu_tracking_with_resources(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    parent: &impl TrackedMenuPaneView<
        MenuRef = u32,
        Surface = Option<MenuTrackingSurface>,
        Pixel = u16,
        Appearance = PpcTrackedMenuItemAppearance,
    >,
    parent_item: i16,
    menu_handle: u32,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Option<PpcSubmenuTracking> {
    let custom_definition = ppc_menu_uses_guest_definition(memory, resources, menu_handle);
    if !custom_definition {
        ppc_calc_menu_size_with_resources(memory, menu_handle, resources, current_resource_refnum);
    }
    let item_appearances = if custom_definition {
        Vec::new()
    } else {
        ppc_menu_item_appearances(memory, menu_handle, resources, current_resource_refnum)
    };
    let menu = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    let front_width = ppc_u32_to_i16_saturating(front.width);
    let front_height = ppc_u32_to_i16_saturating(front.height);
    let desired_width = if custom_definition {
        memory.read_u16_be(menu + 2)? as i16
    } else {
        i16::try_from(memory.read_u16_be(menu + 2)?.max(32)).ok()?
    };
    let desired_height = if custom_definition {
        memory.read_u16_be(menu + 4)? as i16
    } else {
        ppc_menu_appearance_height(&item_appearances)
    };
    let menu_bar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
    let layout = standard_submenu_layout(
        parent.dropdown_rect(),
        ppc_tracked_menu_item_offset(parent, parent_item),
        desired_width,
        desired_height,
        (front_width, front_height),
        menu_bar_height,
    )?;
    let popup_left = layout.left;
    let popup_top = layout.top;
    let popup_width = layout.width;
    let popup_height = layout.height;
    let saved_width = popup_width.checked_add(1)?;
    let saved_height = popup_height.checked_add(1)?;
    let saved_len =
        usize::try_from(i32::from(saved_width).checked_mul(i32::from(saved_height))?).ok()?;
    let mut saved_pixels = Vec::with_capacity(saved_len);
    for y in 0..i32::from(saved_height) {
        for x in 0..i32::from(saved_width) {
            saved_pixels.push(ppc_quickdraw_read_pixel(
                memory,
                front,
                (i32::from(popup_left) + x, i32::from(popup_top) + y),
            )?);
        }
    }
    let mut saved_pixels = crate::memory::SavedPixels::from(saved_pixels);
    for y in 0..i32::from(saved_height) {
        for x in 0..i32::from(saved_width) {
            ppc_capture_saved_detail(
                memory,
                front,
                (i32::from(popup_left) + x, i32::from(popup_top) + y),
                &mut saved_pixels,
                (y * i32::from(saved_width) + x) as usize,
            );
        }
    }
    Some(PpcSubmenuTracking {
        parent_item,
        menu_handle,
        popup_left,
        popup_top,
        content_top: layout.content_top,
        scroll_direction: None,
        popup_width,
        popup_height,
        highlighted_item: 0,
        definition: custom_definition
            .then(|| MenuDefinitionTracking::begin_draw(menu_handle, layout.rect())),
        saved_width,
        saved_height,
        front_buffer: Some(front.into()),
        saved_pixels: saved_pixels.into(),
        item_appearances,
    })
}

pub(crate) fn ppc_ensure_tracked_submenu(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    state: &mut PpcMenuTracking,
    request: SubmenuRequest<u32>,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) {
    let resolved_child = ppc_submenu_handle_for_item(
        memory,
        menu_list_handle,
        request.parent_handle,
        request.parent_item,
    );
    let (token, closed) = match state.reconcile_submenu(request, resolved_child) {
        SubmenuReconciliation::Stale | SubmenuReconciliation::Keep => return,
        SubmenuReconciliation::Closed {
            panes_deepest_first,
        } => {
            for submenu in panes_deepest_first {
                ppc_restore_tracked_menu(memory, submenu.front_buffer, &submenu);
            }
            return;
        }
        SubmenuReconciliation::Open {
            token,
            panes_deepest_first,
        } => (token, panes_deepest_first),
    };
    for submenu in closed {
        ppc_restore_tracked_menu(memory, submenu.front_buffer, &submenu);
    }
    let request = token.request();
    let submenu_handle = token.child_handle();
    let Some(front) = state.front_buffer.map(PpcFrontBuffer::from) else {
        return;
    };
    let submenu = match request.parent {
        MenuTrackingPane::Submenu(depth) => {
            let Some(parent) = state.submenus.get(depth).cloned() else {
                return;
            };
            ppc_begin_submenu_tracking_with_resources(
                memory,
                front,
                &parent,
                request.parent_item,
                submenu_handle,
                resources,
                current_resource_refnum,
            )
        }
        MenuTrackingPane::Root => ppc_begin_submenu_tracking_with_resources(
            memory,
            front,
            state,
            request.parent_item,
            submenu_handle,
            resources,
            current_resource_refnum,
        ),
    };
    if let Some(submenu) = submenu {
        if let Err(submenu) = state.install_submenu(token, submenu) {
            if !submenu.saved_pixels.is_empty() {
                ppc_restore_tracked_menu(memory, submenu.front_buffer, &submenu);
            }
        }
    }
}

pub(crate) fn ppc_draw_open_tracked_submenus(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    state: &PpcMenuTracking,
) {
    for submenu in &state.submenus {
        if submenu.definition.is_some() {
            let _ = ppc_draw_tracked_menu_chrome(
                memory,
                gworlds,
                screen_clut,
                menu_colors,
                StandardMenuPaneKind::Hierarchical,
                submenu,
            );
        } else {
            ppc_draw_tracked_menu(
                memory,
                gworlds,
                screen_clut,
                menu_colors,
                StandardMenuPaneKind::Hierarchical,
                submenu,
                submenu.highlighted_item,
            );
        }
    }
}

// The standard MDEF alternately highlights and unhighlights only the chosen
// leaf while its parent hierarchy remains open. MenuFlash supplies the repeat
// count. Macintosh Toolbox Essentials (1992), pp. 3-115 and 3-142.
pub(crate) fn ppc_redraw_standard_menu_tracking_flash(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    state: &PpcMenuTracking,
    selected: (u32, i16),
    visible: bool,
) {
    ppc_restore_menu_tracking(memory, state.front_buffer, state);
    let root_item = if !visible && selected.0 == state.menu_handle {
        0
    } else {
        state.highlighted_item
    };
    ppc_draw_tracked_menu(
        memory,
        gworlds,
        screen_clut,
        menu_colors,
        state.kind.into(),
        state,
        root_item,
    );
    for submenu in &state.submenus {
        let visible_item = if !visible && selected.0 == submenu.menu_handle {
            0
        } else {
            submenu.highlighted_item
        };
        ppc_draw_tracked_menu(
            memory,
            gworlds,
            screen_clut,
            menu_colors,
            StandardMenuPaneKind::Hierarchical,
            submenu,
            visible_item,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_update_menu_tracking(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    menu_list_handle: u32,
    state: &mut PpcMenuTracking,
    input: PpcInputSnapshot,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) {
    let point = (input.mouse_v, input.mouse_h);
    let root_rows = ppc_menu_tracking_rows(memory, state);
    let submenu_rows = state
        .submenus
        .iter()
        .map(|submenu| Some(ppc_menu_tracking_rows(memory, submenu)))
        .collect::<Vec<_>>();
    let Some(update) = state.track_standard_pointer(&root_rows, &submenu_rows, point) else {
        return;
    };
    let submenu_request = update.submenu_request();
    ppc_write_standard_menu_choice(memory, update.menu_handle, update.pointer.menu_choice_item);
    for submenu in update.closed_panes_deepest_first {
        ppc_restore_tracked_menu(memory, submenu.front_buffer, &submenu);
    }
    match update.pane {
        MenuTrackingPane::Root => ppc_draw_tracked_menu(
            memory,
            gworlds,
            screen_clut,
            menu_colors,
            state.kind.into(),
            state,
            update.pointer.item,
        ),
        MenuTrackingPane::Submenu(depth) if update.pointer.scrolled => {
            let submenu = &state.submenus[depth];
            ppc_draw_tracked_menu(
                memory,
                gworlds,
                screen_clut,
                menu_colors,
                StandardMenuPaneKind::Hierarchical,
                submenu,
                submenu.highlighted_item,
            );
        }
        MenuTrackingPane::Submenu(_) => {}
    }
    if let Some(request) = submenu_request {
        ppc_ensure_tracked_submenu(
            memory,
            menu_list_handle,
            state,
            request,
            resources,
            current_resource_refnum,
        );
    }
    ppc_write_menu_scrolling_bounds(memory, update.content_top, update.content_bottom);
    ppc_draw_open_tracked_submenus(memory, gworlds, screen_clut, menu_colors, state);
}

pub(crate) fn ppc_tracked_menu_selection(
    memory: &mut PpcSectionMem,
    state: &PpcMenuTracking,
) -> Option<(u32, i16)> {
    state.selection(|menu_handle, item| {
        ppc_menu_item_is_selectable(memory, menu_handle, item)
            && !ppc_menu_item_is_hierarchical(memory, menu_handle, item)
    })
}

pub(crate) fn ppc_deepest_highlighted_menu_item(state: &PpcMenuTracking) -> Option<(u32, i16)> {
    state
        .submenus
        .iter()
        .rev()
        .find_map(|submenu| {
            (submenu.highlighted_item > 0)
                .then_some((submenu.menu_handle, submenu.highlighted_item))
        })
        .or_else(|| {
            (state.highlighted_item > 0).then_some((state.menu_handle, state.highlighted_item))
        })
}

#[cfg(test)]
pub(crate) fn ppc_begin_menu_bar_tracking(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    menu_handle: u32,
    title_left: i16,
) -> Option<PpcMenuTracking> {
    ppc_begin_menu_bar_tracking_with_resources(memory, front, menu_handle, title_left, &[], 0)
}

pub(crate) fn ppc_begin_menu_bar_tracking_with_resources(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    menu_handle: u32,
    title_left: i16,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> Option<PpcMenuTracking> {
    ppc_calc_menu_size_with_resources(memory, menu_handle, resources, current_resource_refnum);
    let item_appearances =
        ppc_menu_item_appearances(memory, menu_handle, resources, current_resource_refnum);
    let menu = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    let menu_bar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
    let front_width = ppc_u32_to_i16_saturating(front.width);
    let front_height = ppc_u32_to_i16_saturating(front.height);
    let desired_width = i16::try_from(memory.read_u16_be(menu + 2)?.max(32)).unwrap_or(i16::MAX);
    let desired_height = ppc_menu_appearance_height(&item_appearances);
    let layout = standard_pull_down_menu_layout(
        desired_width,
        desired_height,
        (front_width, front_height),
        title_left,
        menu_bar_height,
    )?;
    ppc_begin_tracked_menu_with_appearances(
        memory,
        front,
        MenuTrackingKind::MenuBar,
        menu_handle,
        layout.left,
        layout.top,
        layout.content_top,
        layout.width,
        layout.height,
        0,
        item_appearances,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_begin_custom_menu_bar_tracking(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    initial_point: u32,
    resources: &[PpcVfsResourceRecord],
) -> Option<PpcImportAction> {
    let menu_list = ppc_current_menu_list(memory);
    let front = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)?;
    let (menu_handle, title_left) =
        ppc_menu_handle_at_title_point(memory, menu_list, initial_point)?;
    ppc_menu_definition_target(cpu, memory, resources, menu_handle)?;
    let menu = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    let menu_bar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
    let layout = standard_pull_down_menu_layout(
        memory.read_u16_be(menu + 2)? as i16,
        memory.read_u16_be(menu + 4)? as i16,
        (
            ppc_u32_to_i16_saturating(front.width),
            ppc_u32_to_i16_saturating(front.height),
        ),
        title_left,
        menu_bar_height,
    )?;
    let mut state = ppc_begin_tracked_menu_with_appearances(
        memory,
        front,
        MenuTrackingKind::MenuBar,
        menu_handle,
        layout.left,
        layout.top,
        layout.content_top,
        layout.width,
        layout.height,
        0,
        Vec::new(),
    )?;
    state.definition = Some(MenuDefinitionTracking::begin_draw(
        menu_handle,
        state.dropdown_rect(),
    ));
    let menu_id = memory.read_u16_be(menu).unwrap_or(0) as i16;
    ppc_set_menu_title_highlight_with_colors(
        memory,
        gworlds,
        menu_list,
        menu_id,
        screen_clut,
        menu_colors,
        startup.host_menu_bar_hidden,
    );
    ppc_draw_tracked_menu_chrome(
        memory,
        gworlds,
        screen_clut,
        menu_colors,
        StandardMenuPaneKind::PullDown,
        &state,
    )?;
    startup.execution.set_menu_state(Some(state));
    startup.execution.with_menu_context_mut(|context| {
        context
            .call
            .get_or_insert(ppc_menu_select_call(cpu, initial_point));
    });
    ppc_prepare_menu_definition_port(startup, current_gworld, current_gdevice);
    let invocation = startup
        .active_menu_definition()
        .and_then(MenuDefinitionTracking::pending_invocation)?;
    let action = ppc_dispatch_native_menu_definition(
        cpu,
        Some(process_memory_manager),
        memory,
        heap_cursor,
        heap_limit,
        resources,
        startup,
        invocation,
        cpu.pc,
    );
    if action.is_none() {
        if let Some(state) = startup.execution.take_menu_state() {
            ppc_restore_menu_tracking(memory, state.front_buffer, &state);
        }
        startup.clear_active_menu_definition();
        startup
            .execution
            .with_menu_context_mut(|context| context.clear_native_menu());
        ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
        ppc_set_menu_command_highlight_with_colors(
            memory,
            gworlds,
            menu_list,
            0,
            None,
            screen_clut,
            menu_colors,
            startup.host_menu_bar_hidden,
        );
    }
    action
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_continue_custom_menu_bar_tracking(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    startup: &mut PpcToolboxStartupState,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    input: PpcInputSnapshot,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> PpcImportAction {
    let Some(call) = startup.execution.menu().context().native_menu() else {
        startup.clear_active_menu_definition();
        ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
        return PpcImportAction::Return(0);
    };
    let Some(completed) = startup
        .with_active_menu_definition_mut(MenuDefinitionTracking::complete_callback)
    else {
        cpu.lr = call.origin.return_address();
        startup
            .execution
            .with_menu_context_mut(|context| context.clear_native_menu());
        ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
        return PpcImportAction::Return(0);
    };
    let completed = match completed {
        Ok(completed) => completed,
        Err(()) => {
            if let Some(state) = startup.execution.take_menu_state() {
                if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                    .map(MenuTrackingSurface::from)
                    == state.front_buffer
                {
                    ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                }
            }
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_menu());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(0);
        }
    };

    if completed == Some(MenuDefinitionMessage::Choose) {
        let active_submenu = startup.execution.menu().as_ref().and_then(|state| {
            match state.active_definition_pane() {
                Some(MenuDefinitionPane::Submenu(depth)) => state
                    .submenus
                    .get(depth)
                    .map(|submenu| submenu.dropdown_rect()),
                _ => None,
            }
        });
        if let Some((top, left, bottom, right)) = active_submenu {
            if input.mouse_v < top
                || input.mouse_v >= bottom
                || input.mouse_h < left
                || input.mouse_h >= right
            {
                let menu_list = ppc_current_menu_list(memory);
                if let Some(mut state) = startup.execution.take_menu_state() {
                    ppc_update_menu_tracking(
                        memory,
                        gworlds,
                        screen_clut,
                        menu_colors,
                        menu_list,
                        &mut state,
                        input,
                        resources,
                        current_resource_refnum,
                    );
                    startup.execution.set_menu_state(Some(state));
                }
                if startup.active_menu_definition().is_none() {
                    startup
                        .execution
                        .with_menu_context_mut(|context| context.clear_native_menu());
                    ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
                    cpu.lr = call.origin.return_address();
                    return PpcImportAction::Yield(u64::MAX);
                }
                if let Some(invocation) = startup
                    .active_menu_definition()
                    .and_then(MenuDefinitionTracking::pending_invocation)
                {
                    if let Some(action) = ppc_dispatch_native_menu_definition(
                        cpu,
                        Some(process_memory_manager),
                        memory,
                        heap_cursor,
                        heap_limit,
                        resources,
                        startup,
                        invocation,
                        cpu.pc,
                    ) {
                        return action;
                    }
                }
            }
        }
    }

    if startup
        .execution
        .menu()
        .as_ref()
        .is_some_and(|state| state.is_flashing())
    {
        let tick = memory
            .read_u32_be(crate::memory::globals::addr::TICKS)
            .unwrap_or(0);
        let step = startup.execution.advance_menu_flash(tick).unwrap();
        if matches!(step, MenuFlashStep::Wait | MenuFlashStep::Inactive) {
            return PpcImportAction::Yield(u64::MAX);
        }
        if let MenuFlashStep::Complete(result) = step {
            if let Some(state) = startup.execution.take_menu_state() {
                if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                    .map(MenuTrackingSurface::from)
                    == state.front_buffer
                {
                    ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                }
            }
            let menu_list = ppc_current_menu_list(memory);
            ppc_set_menu_command_highlight_with_colors(
                memory,
                gworlds,
                menu_list,
                result,
                None,
                screen_clut,
                menu_colors,
                startup.host_menu_bar_hidden,
            );
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_menu());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(result);
        }
        let invocation = startup
            .active_menu_definition()
            .and_then(MenuDefinitionTracking::pending_invocation)
            .unwrap();
        if let Some(action) = ppc_dispatch_native_menu_definition(
            cpu,
            Some(process_memory_manager),
            memory,
            heap_cursor,
            heap_limit,
            resources,
            startup,
            invocation,
            cpu.pc,
        ) {
            return action;
        }
    }

    let hit_point = (u32::from(input.mouse_v as u16) << 16) | u32::from(input.mouse_h as u16);
    let invocation = startup
        .with_active_menu_definition_mut(|definition| {
            definition.choose(hit_point)?;
            definition.pending_invocation()
        })
        .flatten();
    if let Some(invocation) = invocation {
        if let Some(action) = ppc_dispatch_native_menu_definition(
            cpu,
            Some(process_memory_manager),
            memory,
            heap_cursor,
            heap_limit,
            resources,
            startup,
            invocation,
            cpu.pc,
        ) {
            return action;
        }
        let menu_list = ppc_current_menu_list(memory);
        if let Some(state) = startup.execution.take_menu_state() {
            if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                .map(MenuTrackingSurface::from)
                == state.front_buffer
            {
                ppc_restore_menu_tracking(memory, state.front_buffer, &state);
            }
        }
        startup.clear_active_menu_definition();
        startup
            .execution
            .with_menu_context_mut(|context| context.clear_native_menu());
        ppc_set_menu_command_highlight_with_colors(
            memory,
            gworlds,
            menu_list,
            0,
            None,
            screen_clut,
            menu_colors,
            startup.host_menu_bar_hidden,
        );
        ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
        cpu.lr = call.origin.return_address();
        return PpcImportAction::Return(0);
    }
    if input.mouse_button {
        return PpcImportAction::Yield(u64::MAX);
    }

    let definition = startup.active_menu_definition().unwrap().clone();
    let item = definition.which_item();
    let menu_handle = definition.menu_handle();
    let menu_id = memory
        .read_u32_be(menu_handle)
        .filter(|ptr| *ptr != 0)
        .and_then(|menu| memory.read_u16_be(menu))
        .unwrap_or(0);
    let result = if item > 0 {
        (u32::from(menu_id) << 16) | u32::from(item as u16)
    } else {
        0
    };
    if result != 0 {
        let tick = memory
            .read_u32_be(crate::memory::globals::addr::TICKS)
            .unwrap_or(0);
        let flashes = memory
            .read_u16_be(crate::memory::globals::addr::MENU_FLASH)
            .unwrap_or(crate::memory::globals::DEFAULT_MENU_FLASH_COUNT);
        let flash_enabled = startup
            .execution
            .begin_menu_flash(tick, flashes, result)
            .unwrap();
        if !flash_enabled {
            if let Some(state) = startup.execution.take_menu_state() {
                if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
                    .map(MenuTrackingSurface::from)
                    == state.front_buffer
                {
                    ppc_restore_menu_tracking(memory, state.front_buffer, &state);
                }
            }
            let menu_list = ppc_current_menu_list(memory);
            ppc_set_menu_command_highlight_with_colors(
                memory,
                gworlds,
                menu_list,
                result,
                None,
                screen_clut,
                menu_colors,
                startup.host_menu_bar_hidden,
            );
            startup.clear_active_menu_definition();
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_menu());
            ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
            cpu.lr = call.origin.return_address();
            return PpcImportAction::Return(result);
        }
        return PpcImportAction::Yield(u64::MAX);
    }
    if let Some(state) = startup.execution.take_menu_state() {
        if ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
            .map(MenuTrackingSurface::from)
            == state.front_buffer
        {
            ppc_restore_menu_tracking(memory, state.front_buffer, &state);
        }
    }
    let menu_list = ppc_current_menu_list(memory);
    ppc_set_menu_command_highlight_with_colors(
        memory,
        gworlds,
        menu_list,
        result,
        None,
        screen_clut,
        menu_colors,
        startup.host_menu_bar_hidden,
    );
    startup.clear_active_menu_definition();
    startup
        .execution
        .with_menu_context_mut(|context| context.clear_native_menu());
    ppc_restore_menu_definition_port(startup, current_gworld, current_gdevice);
    cpu.lr = call.origin.return_address();
    PpcImportAction::Return(result)
}

#[cfg(test)]
pub(crate) fn ppc_track_menu_while_held(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    startup: &mut PpcToolboxStartupState,
    initial_point: u32,
    input: PpcInputSnapshot,
) {
    ppc_track_menu_while_held_with_resources(
        memory,
        gworlds,
        screen_clut,
        MenuColorTable::new(&[]),
        startup,
        initial_point,
        input,
        &[],
        0,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_track_menu_while_held_with_resources(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    startup: &mut PpcToolboxStartupState,
    initial_point: u32,
    input: PpcInputSnapshot,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) {
    let current_menu_list = ppc_current_menu_list(memory);
    let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) else {
        return;
    };
    if startup
        .execution
        .menu()
        .as_ref()
        .is_some_and(|state| state.kind != MenuTrackingKind::MenuBar)
    {
        return;
    }
    if startup.host_menu_bar_hidden {
        if let Some(previous) = startup.execution.take_menu_state() {
            if previous.front_buffer == Some(front.into()) {
                ppc_restore_menu_tracking(memory, Some(front.into()), &previous);
            }
        }
        let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, 0);
        return;
    }
    if let Some(previous) = startup.execution.take_menu_state() {
        if previous.front_buffer != Some(front.into()) {
            ppc_restore_menu_tracking(memory, previous.front_buffer, &previous);
            let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, 0);
            let _ = ppc_draw_menu_bar_with_colors(
                memory,
                gworlds,
                current_menu_list,
                screen_clut,
                menu_colors,
            );
            return;
        }
        // MenuSelect keeps tracking while the button is held. Crossing a
        // regular title closes the old dropdown and opens the new title's
        // menu, including a disabled menu whose items can only be examined.
        // Macintosh Toolbox Essentials (1992), pp. 3-6--3-7 and 3-115.
        let live_point = (u32::from(input.mouse_v as u16) << 16) | u32::from(input.mouse_h as u16);
        let switched = ppc_menu_handle_at_title_point(memory, current_menu_list, live_point)
            .filter(|(menu_handle, _)| *menu_handle != previous.menu_handle);
        if let Some((menu_handle, title_left)) = switched {
            ppc_restore_menu_tracking(memory, Some(front.into()), &previous);
            startup.execution.set_menu_state(
                ppc_begin_menu_bar_tracking_with_resources(
                    memory,
                    front,
                    menu_handle,
                    title_left,
                    resources,
                    current_resource_refnum,
                ),
            );
            startup
                .execution
                .with_menu_context_mut(|context| context.clear_native_popup());
        } else {
            startup.execution.set_menu_state(Some(previous));
        }
    } else {
        let Some((menu_handle, title_left)) =
            ppc_menu_handle_at_title_point(memory, current_menu_list, initial_point)
        else {
            return;
        };
        let Some(state) = ppc_begin_menu_bar_tracking_with_resources(
            memory,
            front,
            menu_handle,
            title_left,
            resources,
            current_resource_refnum,
        ) else {
            return;
        };
        startup.execution.set_menu_state(Some(state));
        startup
            .execution
            .with_menu_context_mut(|context| context.clear_native_popup());
    }
    let active_menu_id = startup.execution.menu().as_ref().and_then(|state| {
        memory
            .read_u32_be(state.menu_handle)
            .filter(|menu| *menu != 0)
            .and_then(|menu| memory.read_u16_be(menu))
            .map(|menu_id| menu_id as i16)
    });
    if let Some(active_menu_id) = active_menu_id {
        ppc_set_menu_title_highlight_with_colors(
            memory,
            gworlds,
            current_menu_list,
            active_menu_id,
            screen_clut,
            menu_colors,
            startup.host_menu_bar_hidden,
        );
    }
    if let Some(mut state) = startup.execution.take_menu_state() {
        // MenuSelect highlights the title and tracks the pointer until the
        // button is released, displaying the selected item inversely. A
        // hierarchical title opens its installed submenu while remaining
        // highlighted. Macintosh Toolbox Essentials (1992), pp. 3-38--3-39,
        // 3-43--3-45, and 3-122.
        ppc_update_menu_tracking(
            memory,
            gworlds,
            screen_clut,
            menu_colors,
            current_menu_list,
            &mut state,
            input,
            resources,
            current_resource_refnum,
        );
        startup.execution.set_menu_state(Some(state));
    }
}

pub(crate) fn ppc_finish_menu_bar_tracking_with_colors(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    startup: &mut PpcToolboxStartupState,
    _input: PpcInputSnapshot,
) -> Option<u32> {
    if startup
        .execution
        .menu()
        .as_ref()
        .is_some_and(|state| state.kind != MenuTrackingKind::MenuBar)
    {
        return None;
    }
    let state = startup.execution.menu().as_ref()?;
    let selection = (!startup.host_menu_bar_hidden)
        .then(|| ppc_tracked_menu_selection(memory, state))
        .flatten();
    let result = selection
        .and_then(|(menu_handle, item)| {
            memory
                .read_u32_be(menu_handle)
                .filter(|ptr| *ptr != 0)
                .and_then(|menu| memory.read_u16_be(menu))
                .map(|menu_id| (u32::from(menu_id) << 16) | u32::from(item as u16))
        })
        .unwrap_or(0);
    ppc_complete_menu_bar_tracking_with_colors(
        memory,
        gworlds,
        screen_clut,
        menu_colors,
        startup,
        result,
    )
}

pub(crate) fn ppc_complete_menu_bar_tracking_with_colors(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    startup: &mut PpcToolboxStartupState,
    result: u32,
) -> Option<u32> {
    let current_menu_list = ppc_current_menu_list(memory);
    let state = startup.execution.take_menu_state()?;
    if let Some(front) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD) {
        if Some(MenuTrackingSurface::from(front)) == state.front_buffer {
            ppc_restore_menu_tracking(memory, Some(front.into()), &state);
        }
    }
    // Macintosh Toolbox Essentials (1992), pp. 3-89 and 3-115--3-116: a
    // valid MenuSelect result leaves the originating regular title visibly
    // highlighted, but TheMenu contains the selected submenu's ID. Releasing
    // over a disabled item, divider, the menu bar, or no menu clears both.
    // Macintosh Toolbox Essentials (1992), pp. 3-115--3-116.
    ppc_set_menu_command_highlight_with_colors(
        memory,
        gworlds,
        current_menu_list,
        result,
        None,
        screen_clut,
        menu_colors,
        startup.host_menu_bar_hidden,
    );
    if result == 0 && !startup.host_menu_bar_hidden {
        let _ = ppc_draw_menu_bar_with_colors(
            memory,
            gworlds,
            current_menu_list,
            screen_clut,
            menu_colors,
        );
    }
    Some(result)
}

#[cfg(test)]
pub(crate) fn ppc_finish_menu_bar_tracking(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    startup: &mut PpcToolboxStartupState,
    input: PpcInputSnapshot,
) -> Option<u32> {
    if let Some(mut state) = startup.execution.take_menu_state() {
        let menu_list = ppc_current_menu_list(memory);
        ppc_update_menu_tracking(
            memory,
            gworlds,
            screen_clut,
            MenuColorTable::new(&[]),
            menu_list,
            &mut state,
            input,
            &[],
            0,
        );
        startup.execution.set_menu_state(Some(state));
    }
    ppc_finish_menu_bar_tracking_with_colors(
        memory,
        gworlds,
        screen_clut,
        MenuColorTable::new(&[]),
        startup,
        input,
    )
}

/// Materialize one resource-backed menu for both `GetMenu` and `GetNewMBar`.
/// Unlike raw Resource Manager lookup, `GetMenu` reads both the MENU and its
/// definition procedure into memory. Macintosh Toolbox Essentials (1992),
/// pp. 3-106--3-107 and 3-111--3-112.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_load_menu_resource(
    menu_id: i16,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> u32 {
    let menu_type = u32::from_be_bytes(*b"MENU");
    let Some(index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        menu_type,
        menu_id,
        false,
    ) else {
        *last_resource_error = PPC_NO_ERR;
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
        true,
        last_resource_error,
    );
    if handle != 0
        && !ppc_resolve_menu_definition(
            handle,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        )
    {
        return 0;
    }
    if handle != 0 {
        let mut allocator = PpcProcessAllocatorView {
            memory_manager: process_memory_manager,
        };
        ppc_load_menu_color_resource_with_allocator(
            menu_id,
            Some(&mut allocator),
            None,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
        );
    }
    handle
}

/// Load the MDEF resource whose Handle belongs in `MenuInfo.menuProc`.
/// `NewMenu` loads the standard definition procedure, while `GetMenu` loads
/// the definition resource named by the compiled MENU placeholder. Macintosh
/// Toolbox Essentials (1992), pp. 3-95--3-96 and 3-105--3-107.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_menu_definition_handle(
    mdef_id: i16,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> u32 {
    let mdef_type = u32::from_be_bytes(*b"MDEF");
    let index = match ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        mdef_type,
        mdef_id,
        false,
    ) {
        Some(index) => index,
        None if mdef_id == 0 => {
            vfs_resources.push(PpcVfsResourceRecord {
                ref_num: 0,
                path: "__system__/MDEF".to_string(),
                res_type: mdef_type,
                res_id: 0,
                name: Vec::new(),
                data: STANDARD_MENU_DEFINITION_SHIM.to_vec(),
                raw_data: None,
                raw_attrs: None,
                attrs: 0,
                handle: 0,
            });
            vfs_resources.len() - 1
        }
        None => {
            *last_resource_error = PPC_RES_NOT_FOUND_ERR;
            return 0;
        }
    };
    ppc_materialize_vfs_resource_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        index,
        true,
        last_resource_error,
    )
}

/// Replace a compiled MENU resource's MDEF-ID placeholder with the loaded
/// definition-procedure Handle. Inside Macintosh Volume I (1985), p. I-127;
/// Macintosh Toolbox Essentials (1992), pp. 3-106--3-107.
#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_resolve_menu_definition(
    menu_handle: u32,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> bool {
    let Some(menu_ptr) = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return false;
    };
    let Some(menu_proc) = memory.read_u32_be(menu_ptr + 6) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return false;
    };
    let mdef_type = u32::from_be_bytes(*b"MDEF");
    if menu_proc != 0
        && vfs_resources
            .iter()
            .any(|resource| resource.res_type == mdef_type && resource.handle == menu_proc)
    {
        return true;
    }
    let mdef_id = (menu_proc >> 16) as u16 as i16;
    let mdef_handle = ppc_menu_definition_handle(
        mdef_id,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        current_resource_refnum,
        last_resource_error,
    );
    if mdef_handle == 0 || memory.write_u32_be(menu_ptr + 6, mdef_handle).is_none() {
        if mdef_handle != 0 {
            *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        }
        return false;
    }
    true
}

pub(crate) fn ppc_menu_color_table_bytes(memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) -> Vec<u8> {
    let handle = memory
        .read_u32_be(crate::memory::globals::addr::MENU_C_INFO)
        .unwrap_or(0);
    ppc_menu_handle_bytes(memory, handles, handle).unwrap_or_default()
}

pub(crate) fn ppc_ensure_menu_color_table_handle_with_allocator(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_free_handle_blocks: Option<&mut Vec<PpcHandleRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let current = memory
        .read_u32_be(crate::memory::globals::addr::MENU_C_INFO)
        .unwrap_or(0);
    if current != 0 {
        return current;
    }
    let handle = if let Some(allocator) = allocator {
        allocator.allocate_handle_with_bytes(memory, heap_cursor, last_mem_error, handles, &[])
    } else {
        ppc_alloc_recyclable_handle_with_bytes(
            memory,
            heap_cursor,
            heap_limit,
            handles,
            legacy_free_handle_blocks.expect("legacy menu allocation requires a free list"),
            &[],
        )
    };
    if handle != 0 {
        let _ = memory.write_u32_be(crate::memory::globals::addr::MENU_C_INFO, handle);
    }
    handle
}

#[cfg(test)]
pub(crate) fn ppc_ensure_menu_color_table_handle(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    free_handle_blocks: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_ensure_menu_color_table_handle_with_allocator(
        None,
        Some(free_handle_blocks),
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_load_menu_color_resource_with_allocator(
    resource_id: i16,
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_free_handle_blocks: Option<&mut Vec<PpcHandleRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) {
    let Some(index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"mctb"),
        resource_id,
        false,
    ) else {
        return;
    };
    let incoming = compiled_menu_color_entries(&vfs_resources[index].data);
    if incoming.is_empty() {
        return;
    }
    let handle = ppc_ensure_menu_color_table_handle_with_allocator(
        allocator.as_deref_mut(),
        legacy_free_handle_blocks,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    if handle == 0 {
        return;
    }
    let current = ppc_menu_color_table_bytes(memory, handles);
    let merged = merge_menu_color_entries(&current, &incoming);
    let _ = ppc_replace_menu_bytes_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        handle,
        &merged,
    );
}

pub(crate) fn ppc_clear_menu_color_table_with_allocator(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    ppc_filter_menu_color_table_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        |_id, _item| false,
    );
}

pub(crate) fn ppc_filter_menu_color_table_with_allocator(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    keep: impl FnMut(i16, i16) -> bool,
) {
    let handle = memory
        .read_u32_be(crate::memory::globals::addr::MENU_C_INFO)
        .unwrap_or(0);
    if handle == 0 {
        return;
    }
    let current = ppc_menu_color_table_bytes(memory, handles);
    let filtered = filter_menu_color_entries(&current, keep);
    let _ = ppc_replace_menu_bytes_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        handle,
        &filtered,
    );
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_get_menu_bar_with_allocator(
    current_menu_list: u32,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_free_handle_blocks: Option<&mut Vec<PpcHandleRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let menu_list = if current_menu_list == 0 {
        PpcMenuListDefinition::default()
    } else {
        let Some(menu_list) = ppc_menu_list_definition(memory, current_menu_list) else {
            *last_mem_error = PPC_PARAM_ERR;
            return 0;
        };
        menu_list
    };
    let handle = ppc_alloc_menu_list_definition_handle_with_allocator(
        &menu_list,
        allocator,
        legacy_free_handle_blocks,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    *last_mem_error = if handle == 0 {
        PPC_MEM_FULL_ERR
    } else {
        PPC_NO_ERR
    };
    handle
}

pub(crate) fn ppc_alloc_menu_list_handle_with_allocator(
    menu_handles: &[u32],
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_free_handle_blocks: Option<&mut Vec<PpcHandleRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let mut menu_list =
        PpcMenuListDefinition::from_regular_handles(0, menu_handles.iter().copied());
    ppc_relayout_menu_list(memory, &mut menu_list);
    ppc_alloc_menu_list_definition_handle_with_allocator(
        &menu_list,
        allocator,
        legacy_free_handle_blocks,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    )
}

#[cfg(test)]
pub(crate) fn ppc_alloc_menu_list_handle(
    menu_handles: &[u32],
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    free_handle_blocks: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_alloc_menu_list_handle_with_allocator(
        menu_handles,
        None,
        Some(free_handle_blocks),
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
    )
}

pub(crate) fn ppc_alloc_menu_list_definition_handle_with_allocator(
    menu_list: &PpcMenuListDefinition,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_free_handle_blocks: Option<&mut Vec<PpcHandleRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let bytes = ppc_menu_list_bytes(menu_list);
    if let Some(allocator) = allocator {
        allocator.allocate_handle_with_bytes(memory, heap_cursor, last_mem_error, handles, &bytes)
    } else {
        ppc_alloc_recyclable_handle_with_bytes(
            memory,
            heap_cursor,
            heap_limit,
            handles,
            legacy_free_handle_blocks.expect("legacy menu allocation requires a free list"),
            &bytes,
        )
    }
}

#[cfg(test)]
pub(crate) fn ppc_alloc_menu_list_definition_handle(
    menu_list: &PpcMenuListDefinition,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    free_handle_blocks: &mut Vec<PpcHandleRecord>,
) -> u32 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_alloc_menu_list_definition_handle_with_allocator(
        menu_list,
        None,
        Some(free_handle_blocks),
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_get_new_mbar(
    mbar_id: i16,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    vfs_resources: &mut Vec<PpcVfsResourceRecord>,
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> u32 {
    let mbar_type = u32::from_be_bytes(*b"MBAR");
    let Some(mbar_index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        mbar_type,
        mbar_id,
        false,
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(mbar) = MenuBarResource::decode(&vfs_resources[mbar_index].data) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    // GetNewMBar saves the current menu list but clears MenuCInfo before it
    // calls GetMenu for each compiled menu ID. The rebuilt color table remains
    // current after the old menu list is restored. Macintosh Toolbox
    // Essentials (1992), pp. 3-111--3-112.
    {
        let mut allocator = PpcProcessAllocatorView {
            memory_manager: process_memory_manager,
        };
        ppc_clear_menu_color_table_with_allocator(
            Some(&mut allocator),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
        );
    }
    let menu_handles = mbar.load_regular_handles(|menu_id| {
        let handle = ppc_load_menu_resource(
            menu_id,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        );
        (handle != 0).then_some(handle)
    });

    // Macintosh Toolbox Essentials (1992), pp. 3-110--3-112 and 3-155:
    // GetNewMBar expands the MBAR's ordered MENU resource IDs into a new,
    // caller-owned menu-list handle; it does not install or draw that list.
    let mut menu_list = PpcMenuListDefinition::from_regular_handles(mbar_id, menu_handles);
    ppc_relayout_menu_list(memory, &mut menu_list);
    let mut allocator = PpcProcessAllocatorView {
        memory_manager: process_memory_manager,
    };
    let handle = ppc_alloc_menu_list_definition_handle_with_allocator(
        &menu_list,
        Some(&mut allocator),
        None,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    if handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    *last_resource_error = PPC_NO_ERR;
    handle
}

pub(crate) fn ppc_continue_menu_bar_build(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    startup: &mut PpcToolboxStartupState,
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> PpcImportAction {
    loop {
        let menu_handle = match startup.execution.calls()
            .advance_menu_bar_build(GuestIsa::PowerPc)
        {
            Some(MenuBarBuildResume::Size(handle)) => handle,
            Some(MenuBarBuildResume::Complete {
                result,
                origin: MenuBarCallOrigin::PowerPc { return_address },
            }) => {
                cpu.lr = return_address;
                return PpcImportAction::Return(result);
            }
            Some(MenuBarBuildResume::Waiting) => return PpcImportAction::Yield(u64::MAX),
            _ => return PpcImportAction::Return(0),
        };
        if let Some(action) = ppc_dispatch_native_menu_definition(
            cpu,
            Some(process_memory_manager),
            memory,
            heap_cursor,
            heap_limit,
            resources,
            startup,
            MenuDefinitionInvocation::size(menu_handle),
            cpu.pc,
        ) {
            return action;
        }
        ppc_calc_menu_size_with_resources(memory, menu_handle, resources, current_resource_refnum);
    }
}

#[cfg(test)]
pub(crate) fn ppc_menu_list_handles(memory: &mut PpcSectionMem, menu_list_handle: u32) -> Option<Vec<u32>> {
    Some(
        ppc_menu_list_definition(memory, menu_list_handle)?
            .handles()
            .collect(),
    )
}

pub(crate) fn ppc_menu_list_definition(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
) -> Option<PpcMenuListDefinition> {
    let list = memory
        .read_u32_be(menu_list_handle)
        .filter(|ptr| *ptr != 0)?;

    let regular_bytes = usize::from(memory.read_u16_be(list)?);
    if regular_bytes % 6 != 0 || regular_bytes / 6 > MAX_MENU_LIST_ENTRIES {
        return None;
    }
    let hierarchical_header = list.checked_add(6 + u32::try_from(regular_bytes).ok()?)?;
    let hierarchical_bytes = usize::from(memory.read_u16_be(hierarchical_header)?);
    if hierarchical_bytes % 6 != 0 || hierarchical_bytes / 6 > MAX_MENU_LIST_ENTRIES {
        return None;
    }
    let byte_count = 12usize
        .checked_add(regular_bytes)?
        .checked_add(hierarchical_bytes)?;
    let bytes = ppc_memory_read_bytes(memory, list, u32::try_from(byte_count).ok()?)?;
    PpcMenuListDefinition::decode(&bytes)
}

pub(crate) fn ppc_menu_list_bytes(menu_list: &PpcMenuListDefinition) -> Vec<u8> {
    menu_list.encode()
}

pub(crate) fn ppc_relayout_menu_list(memory: &mut PpcSectionMem, menu_list: &mut PpcMenuListDefinition) {
    menu_list.relayout_regular_titles(
        STANDARD_MENU_BAR_FIRST_TITLE_LEFT,
        STANDARD_MENU_BAR_TITLE_SPACING,
        |handle| {
            memory
                .read_u32_be(handle)
                .filter(|menu| *menu != 0)
                .and_then(|menu| ppc_read_pascal_string(memory, menu.checked_add(14)?))
                .map(|title| standard_menu_title_advance(&title))
                .unwrap_or(0)
        },
    );
}

pub(crate) fn ppc_replace_menu_list_definition_with_allocator(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_list_handle: u32,
    menu_list: &PpcMenuListDefinition,
) -> i16 {
    let bytes = ppc_menu_list_bytes(menu_list);
    ppc_replace_menu_bytes_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        menu_list_handle,
        &bytes,
    )
}

#[cfg(test)]
pub(crate) fn ppc_replace_menu_list_definition(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    menu_list_handle: u32,
    menu_list: &PpcMenuListDefinition,
) -> i16 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_replace_menu_list_definition_with_allocator(
        None,
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
        menu_list_handle,
        menu_list,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_insert_menu_with_allocator(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_free_handle_blocks: Option<&mut Vec<PpcHandleRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    before_id: i16,
) -> i16 {
    if memory
        .read_u32_be(menu_handle)
        .filter(|ptr| *ptr != 0)
        .is_none()
    {
        return PPC_PARAM_ERR;
    }
    let mut current_menu_list = ppc_current_menu_list(memory);
    if current_menu_list == 0 {
        current_menu_list = ppc_alloc_menu_list_handle_with_allocator(
            &[],
            allocator.as_deref_mut(),
            legacy_free_handle_blocks,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
        );
        if current_menu_list == 0 {
            return PPC_MEM_FULL_ERR;
        }
        ppc_set_current_menu_list(memory, current_menu_list);
    }
    let Some(mut menu_list) = ppc_menu_list_definition(memory, current_menu_list) else {
        return PPC_PARAM_ERR;
    };
    if !menu_list.insert(menu_handle, before_id, |candidate| {
        memory
            .read_u32_be(candidate)
            .and_then(|menu| memory.read_u16_be(menu))
            .map(|id| id as i16)
    }) {
        return PPC_NO_ERR;
    }
    ppc_relayout_menu_list(memory, &mut menu_list);
    ppc_replace_menu_list_definition_with_allocator(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        current_menu_list,
        &menu_list,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_insert_menu(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    free_handle_blocks: &mut Vec<PpcHandleRecord>,
    menu_handle: u32,
    before_id: i16,
) -> i16 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_insert_menu_with_allocator(
        None,
        Some(free_handle_blocks),
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
        menu_handle,
        before_id,
    )
}

pub(crate) fn ppc_delete_menu_with_allocator(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    menu_list_handle: u32,
    menu_id: i16,
) -> i16 {
    if menu_list_handle == 0 {
        return PPC_NO_ERR;
    }
    let Some(mut menu_list) = ppc_menu_list_definition(memory, menu_list_handle) else {
        return PPC_PARAM_ERR;
    };
    if menu_list
        .remove_by_id(menu_id, |candidate| {
            memory
                .read_u32_be(candidate)
                .and_then(|menu| memory.read_u16_be(menu))
                .map(|id| id as i16)
        })
        .is_none()
    {
        return PPC_NO_ERR;
    }
    ppc_relayout_menu_list(memory, &mut menu_list);
    let result = ppc_replace_menu_list_definition_with_allocator(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        menu_list_handle,
        &menu_list,
    );
    if result == PPC_NO_ERR {
        ppc_filter_menu_color_table_with_allocator(
            allocator,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            |id, _item| id != menu_id,
        );
    }
    result
}

#[cfg(test)]
pub(crate) fn ppc_delete_menu(
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    handles: &mut Vec<PpcHandleRecord>,
    menu_list_handle: u32,
    menu_id: i16,
) -> i16 {
    let mut last_mem_error = PPC_NO_ERR;
    ppc_delete_menu_with_allocator(
        None,
        memory,
        heap_cursor,
        heap_limit,
        &mut last_mem_error,
        handles,
        menu_list_handle,
        menu_id,
    )
}

pub(crate) fn ppc_get_menu_handle(memory: &mut PpcSectionMem, menu_list_handle: u32, menu_id: i16) -> u32 {
    let Some(menu_list) = ppc_menu_list_definition(memory, menu_list_handle) else {
        return 0;
    };
    menu_list
        .find_handle_by_id(menu_id, |handle| {
            memory
                .read_u32_be(handle)
                .filter(|menu| *menu != 0)
                .and_then(|menu| memory.read_u16_be(menu))
                .map(|id| id as i16)
        })
        .unwrap_or(0)
}

pub(crate) fn ppc_regular_menu_contains_id(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    menu_id: i16,
) -> bool {
    let Some(menu_list) = ppc_menu_list_definition(memory, menu_list_handle) else {
        return false;
    };
    let contains = menu_list.regular_handles().any(|menu_handle| {
        memory
            .read_u32_be(menu_handle)
            .filter(|menu| *menu != 0)
            .and_then(|menu| memory.read_u16_be(menu))
            .map(|id| id as i16)
            == Some(menu_id)
    });
    contains
}

pub(crate) fn ppc_root_menu_id_for_selection(
    memory: &mut PpcSectionMem,
    menu_list_handle: u32,
    selected_menu_id: i16,
) -> i16 {
    let Some(menu_list) = ppc_menu_list_definition(memory, menu_list_handle) else {
        return 0;
    };
    menu_list
        .owning_regular_menu(selected_menu_id, |menu_handle| {
            ppc_menu_key_menu(memory, menu_handle)
        })
        .map(|(_handle, menu_id)| menu_id)
        .unwrap_or(0)
}

pub(crate) fn ppc_flash_entire_menu_bar_with_colors(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
) -> bool {
    let Some(front_buffer) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
    else {
        return false;
    };
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    // StandardMBDF reverses the complete bar through MCEntry(0,0)'s RGB4
    // menu-bar background and RGB1 title foreground, preserving unrelated
    // indexed colors. Inside Macintosh Volume V (1986), pp. V-235 and V-246.
    let (Some(background), Some(foreground), Some(black)) = (
        ppc_physical_screen_color_pixel(
            front_buffer,
            ppc_menu_rgb(menu_colors.menu_bar_background()),
            screen_clut,
        ),
        ppc_physical_screen_color_pixel(
            front_buffer,
            ppc_menu_rgb(menu_colors.title_foreground(0)),
            screen_clut,
        ),
        ppc_physical_screen_color_pixel(front_buffer, PPC_RGB_BLACK, screen_clut),
    ) else {
        return false;
    };
    let height = front_buffer.height.min(u32::from(
        memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20),
    ));
    for y in 0..height as i32 {
        for x in 0..front_buffer.width as i32 {
            let Some(pixel) = ppc_quickdraw_read_pixel(memory, front_buffer, (x, y)) else {
                continue;
            };
            let reversed = standard_menu_highlighted_value(pixel, background, foreground);
            let _ = ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), reversed);
        }
    }
    // The outer-screen mask is not menu-bar content and therefore remains
    // black while FlashMenuBar(0) reverses the strip.
    for_each_standard_menu_bar_corner_pixel(
        ppc_u32_to_i16_saturating(front_buffer.width),
        |x, y| {
            let _ = ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (i32::from(x), i32::from(y)),
                black,
            );
        },
    );
    true
}

pub(crate) fn ppc_set_menu_title_highlight_with_colors(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    menu_list_handle: u32,
    requested_menu_id: i16,
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    menu_bar_hidden: bool,
) {
    let target_menu_id = if requested_menu_id != 0
        && ppc_regular_menu_contains_id(memory, menu_list_handle, requested_menu_id)
    {
        requested_menu_id
    } else {
        0
    };
    let previous_menu_id = memory.read_u16_be(PPC_THE_MENU_ADDR).unwrap_or(0) as i16;
    if previous_menu_id == target_menu_id {
        return;
    }
    let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, target_menu_id as u16);
    if !menu_bar_hidden {
        let _ = ppc_draw_menu_bar_with_colors(
            memory,
            gworlds,
            menu_list_handle,
            screen_clut,
            menu_colors,
        );
    }
}

pub(crate) fn ppc_set_menu_command_highlight_with_colors(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    menu_list_handle: u32,
    result: u32,
    known_root_menu_id: Option<i16>,
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
    menu_bar_hidden: bool,
) {
    let selected_menu_id = (result >> 16) as u16 as i16;
    let root_menu_id = known_root_menu_id.unwrap_or_else(|| {
        ppc_root_menu_id_for_selection(memory, menu_list_handle, selected_menu_id)
    });
    ppc_set_menu_title_highlight_with_colors(
        memory,
        gworlds,
        menu_list_handle,
        root_menu_id,
        screen_clut,
        menu_colors,
        menu_bar_hidden,
    );
    // The highlighted artwork belongs to the originating regular title, but
    // TheMenu identifies the menu that owns the chosen item, including a
    // hierarchical submenu. Macintosh Toolbox Essentials (1992), pp.
    // 3-115--3-119.
    let _ = memory.write_u16_be(PPC_THE_MENU_ADDR, selected_menu_id as u16);
}

#[cfg(test)]
pub(crate) fn ppc_set_menu_title_highlight(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    menu_list_handle: u32,
    requested_menu_id: i16,
    screen_clut: &[[u16; 3]; 256],
    menu_bar_hidden: bool,
) {
    ppc_set_menu_title_highlight_with_colors(
        memory,
        gworlds,
        menu_list_handle,
        requested_menu_id,
        screen_clut,
        MenuColorTable::new(&[]),
        menu_bar_hidden,
    );
}

pub(crate) fn ppc_reverse_menu_title_cell(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    region: MenuBarTitleRegion,
    menu_bar_height: i16,
    background: u16,
    foreground: u16,
    themed: Option<(u16, u16)>,
) {
    if menu_bar_height <= 1 {
        return;
    }
    let screen_width = ppc_u32_to_i16_saturating(front_buffer.width);
    let screen_height = ppc_u32_to_i16_saturating(front_buffer.height);
    // The standard MBDF's highlighted title rectangle extends beyond the
    // logical hit cell and excludes the menu bar's outer border. Inside
    // Macintosh Volume I (1985), p. I-356.
    let (top, left, bottom, right) = region.highlighted_rect(menu_bar_height);
    let left = left.max(0).min(screen_width);
    let right = right.max(0).min(screen_width);
    let bottom = bottom.min(screen_height);
    for y in top..bottom {
        for x in left..right {
            let Some(pixel) =
                ppc_quickdraw_read_pixel(memory, front_buffer, (i32::from(x), i32::from(y)))
            else {
                continue;
            };
            let reversed = if let Some((selected_background, selected_foreground)) = themed {
                if pixel == background {
                    selected_background
                } else if pixel == foreground {
                    selected_foreground
                } else {
                    pixel
                }
            } else {
                standard_menu_highlighted_value(pixel, background, foreground)
            };
            let _ = ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (i32::from(x), i32::from(y)),
                reversed,
            );
        }
    }
}

pub(crate) fn ppc_menu_bar_title_metrics() -> (i16, i16) {
    let (menu_font, numerator, denominator) =
        get_font_face_scale_ratio(PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM);
    let menu_ascent =
        ppc_scale_font_value(i32::from(menu_font.metrics.ascent), numerator, denominator);
    let menu_descent =
        ppc_scale_font_value(i32::from(menu_font.metrics.descent), numerator, denominator);
    (menu_ascent, menu_descent)
}

pub(crate) fn ppc_menu_bar_title_baseline(menu_bar_height: i16) -> i16 {
    let (menu_ascent, menu_descent) = ppc_menu_bar_title_metrics();
    // Keep the PowerPC menu title baseline identical to the 68k Menu
    // Manager when an application changes MBarHeight: center the live system
    // font metrics inside the bar instead of assuming the default 20 pixels.
    standard_menu_bar_title_baseline(menu_bar_height, menu_ascent, menu_descent)
}

pub(crate) fn ppc_menu_bar_system_mark_top(menu_bar_height: i16) -> i16 {
    let (menu_ascent, menu_descent) = ppc_menu_bar_title_metrics();
    standard_menu_bar_system_mark_top(menu_bar_height, menu_ascent, menu_descent)
}

pub(crate) fn ppc_apply_menu_title_dim_pattern(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    rect: (i16, i16, i16, i16),
    background: u16,
) {
    let screen_width = ppc_u32_to_i16_saturating(front_buffer.width);
    let screen_height = ppc_u32_to_i16_saturating(front_buffer.height);
    let (top, left, bottom, right) = rect;
    for y in top.max(0)..bottom.min(screen_height) {
        for x in left.max(0)..right.min(screen_width) {
            if standard_menu_gray_pattern_is_ink(x, y) {
                continue;
            }
            let _ = ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (i32::from(x), i32::from(y)),
                background,
            );
        }
    }
}

pub(crate) fn ppc_draw_menu_bar_with_colors(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    menu_list_handle: u32,
    screen_clut: &[[u16; 3]; 256],
    menu_colors: MenuColorTable<'_>,
) -> bool {
    let theme = ppc_ui_theme(gworlds);
    let palette = theme.provider().palette();
    let Some(front_buffer) = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD)
    else {
        return false;
    };
    if !matches!(front_buffer.depth, 1 | 2 | 4 | 8 | 16) {
        return false;
    }
    // The standard MBDF resolves the menu-bar background and each title's
    // foreground/background from the live MenuCInfo table. Inside Macintosh
    // Volume V (1986), pp. V-231--V-235 and V-249--V-250; Macintosh Toolbox
    // Essentials (1992), pp. 3-152--3-156.
    let (Some(bar_background), Some(black)) = (
        ppc_physical_screen_color_pixel(
            front_buffer,
            if theme == UiThemeId::ClassicSystem7 {
                ppc_menu_rgb(menu_colors.menu_bar_background())
            } else {
                ppc_theme_rgb(palette.window_background)
            },
            screen_clut,
        ),
        ppc_physical_screen_color_pixel(front_buffer, PPC_RGB_BLACK, screen_clut),
    ) else {
        return false;
    };
    let height = front_buffer.height.min(u32::from(
        memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20),
    )) as i32;
    if height == 0 {
        return true;
    }
    for y in 0..height {
        let pixel = if y == height - 1 {
            black
        } else {
            bar_background
        };
        for x in 0..front_buffer.width as i32 {
            let _ = ppc_quickdraw_write_raw_pixel(memory, front_buffer, (x, y), pixel);
        }
    }
    for_each_standard_menu_bar_corner_pixel(
        ppc_u32_to_i16_saturating(front_buffer.width),
        |x, y| {
            let _ = ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (i32::from(x), i32::from(y)),
                black,
            );
        },
    );

    let Some(menu_list) = ppc_menu_list_definition(memory, menu_list_handle) else {
        return true;
    };
    // DrawMenuBar redraws the title represented by TheMenu as highlighted
    // after drawing the remaining titles. For a submenu-valued TheMenu this
    // is the originating regular title. Inside Macintosh Volume V (1986),
    // p. V-250; Macintosh Toolbox Essentials (1992), pp. 3-115--3-119.
    let selected_menu_id = memory.read_u16_be(PPC_THE_MENU_ADDR).unwrap_or(0) as i16;
    let highlighted_menu_id =
        ppc_root_menu_id_for_selection(memory, menu_list_handle, selected_menu_id);
    struct PendingTitleHighlight {
        region: MenuBarTitleRegion,
        background: u16,
        foreground: u16,
        system_menu_mark: bool,
        title_h: i16,
        dim_pattern: Option<((i16, i16, i16, i16), u16)>,
    }

    let mut pending_highlight = None;
    let menu_bar_height = i16::try_from(height).unwrap_or(i16::MAX);
    let title_v = ppc_menu_bar_title_baseline(menu_bar_height);
    let (title_ascent, title_descent) = ppc_menu_bar_title_metrics();
    // Inside Macintosh Volume V (1986), pp. V-228--V-230: regular MenuList
    // entries carry their live menuLeft values and lastRight closes the final
    // title cell. The shared MenuList operation is therefore authoritative
    // for drawing as well as hit testing; do not reconstruct a second layout
    // from title widths in this PowerPC adapter.
    for region in menu_list.regular_title_regions() {
        if region.handle == 0 {
            continue;
        }
        let Some(menu) = memory.read_u32_be(region.handle).filter(|ptr| *ptr != 0) else {
            continue;
        };
        let menu_id = memory.read_u16_be(menu).unwrap_or(0) as i16;
        let title_foreground = if theme == UiThemeId::ClassicSystem7 {
            ppc_menu_rgb(menu_colors.title_foreground(menu_id))
        } else {
            ppc_theme_rgb(palette.frame_dark)
        };
        let title_background = if theme == UiThemeId::ClassicSystem7 {
            ppc_menu_rgb(menu_colors.title_background(menu_id))
        } else {
            ppc_theme_rgb(palette.window_background)
        };
        let dimmed_title = ppc_menu_rgb(MenuColorTable::dimmed(
            menu_colors.title_foreground(menu_id),
            menu_colors.title_background(menu_id),
        ));
        let (Some(title_foreground_pixel), Some(title_background_pixel), Some(dimmed_title_pixel)) = (
            ppc_physical_screen_color_pixel(front_buffer, title_foreground, screen_clut),
            ppc_physical_screen_color_pixel(front_buffer, title_background, screen_clut),
            ppc_physical_screen_color_pixel(front_buffer, dimmed_title, screen_clut),
        ) else {
            continue;
        };
        // MTE 1992 p. 3-131: DisableItem(menu, 0) disables the whole menu
        // title, and HIG 1992 p. 54 says an unavailable title remains visible
        // but is drawn in gray. Match the 68k standard definition procedure:
        // use the GetGray midpoint when the device can represent it, otherwise
        // fall back to the shared 50% gray pattern (IM:V 1986 p. V-142).
        let menu_enabled = memory.read_u32_be(menu + 10).unwrap_or(0) & 1 != 0;
        let title_ptr = menu.wrapping_add(14);
        let Some(title) = ppc_read_pascal_string(memory, title_ptr) else {
            continue;
        };
        let title_h = region.title_origin();
        let system_menu_mark = is_standard_system_menu_title(&title);
        let (cell_top, cell_left, cell_bottom, cell_right) =
            region.highlighted_rect(menu_bar_height);
        for y in cell_top.max(0)..cell_bottom.min(ppc_u32_to_i16_saturating(front_buffer.height)) {
            for x in cell_left.max(0)..cell_right.min(ppc_u32_to_i16_saturating(front_buffer.width))
            {
                let _ = ppc_quickdraw_write_raw_pixel(
                    memory,
                    front_buffer,
                    (i32::from(x), i32::from(y)),
                    title_background_pixel,
                );
            }
        }
        if system_menu_mark {
            ppc_draw_system_menu_mark(memory, front_buffer, screen_clut, title_h);
        } else {
            let title_ink = if menu_enabled {
                title_foreground
            } else {
                dimmed_title
            };
            let title_ink_pixel = if menu_enabled {
                title_foreground_pixel
            } else {
                dimmed_title_pixel
            };
            let explicit_index = ppc_indexed_depth_entry_count(front_buffer.depth)
                .and_then(|_| u8::try_from(title_ink_pixel).ok());
            ppc_draw_text_bytes(
                memory,
                gworlds,
                PPC_MAIN_GWORLD,
                (title_h, title_v),
                PPC_QD_TEXT_FONT_DEFAULT,
                PPC_QD_TEXT_SIZE_SYSTEM,
                PPC_QD_TEXT_MODE_SRC_OR,
                title_ink,
                explicit_index,
                &title,
            );
        }
        let highlighted = pending_highlight.is_none()
            && highlighted_menu_id != 0
            && menu_id == highlighted_menu_id;
        let dim_with_pattern = !menu_enabled
            && (system_menu_mark
                || dimmed_title_pixel == title_foreground_pixel
                || dimmed_title_pixel == title_background_pixel);
        let dim_pattern = dim_with_pattern.then(|| {
            let dim_top = if system_menu_mark {
                1
            } else {
                title_v.saturating_sub(title_ascent).max(0)
            };
            let dim_bottom = if system_menu_mark {
                menu_bar_height.saturating_sub(1)
            } else {
                title_v
                    .saturating_add(title_descent)
                    .min(menu_bar_height.saturating_sub(1))
            };
            (
                (
                    dim_top,
                    title_h,
                    dim_bottom,
                    title_h.saturating_add(standard_menu_title_advance(&title)),
                ),
                if highlighted {
                    title_foreground_pixel
                } else {
                    title_background_pixel
                },
            )
        });
        if highlighted {
            pending_highlight = Some(PendingTitleHighlight {
                region,
                background: title_background_pixel,
                foreground: title_foreground_pixel,
                system_menu_mark,
                title_h,
                dim_pattern,
            });
        } else if let Some((rect, background)) = dim_pattern {
            ppc_apply_menu_title_dim_pattern(memory, front_buffer, rect, background);
        }
    }
    if let Some(highlight) = pending_highlight {
        // Reverse the selected title only after every normal title has been
        // drawn. Adjacent MBDF cells overlap the three-pixel highlight
        // overhang, so drawing a later title after this step would erase the
        // selected title's right edge. Inside Macintosh Volume V (1986),
        // pp. V-235 and V-244.
        ppc_reverse_menu_title_cell(
            memory,
            front_buffer,
            highlight.region,
            menu_bar_height,
            highlight.background,
            highlight.foreground,
            if theme == UiThemeId::ClassicSystem7 {
                None
            } else {
                Some((
                    ppc_physical_screen_color_pixel(
                        front_buffer,
                        ppc_theme_rgb(palette.selection),
                        screen_clut,
                    )
                    .unwrap_or(highlight.foreground),
                    ppc_physical_screen_color_pixel(
                        front_buffer,
                        ppc_theme_rgb(palette.frame_light),
                        screen_clut,
                    )
                    .unwrap_or(highlight.background),
                ))
            },
        );
        if highlight.system_menu_mark && front_buffer.depth != 1 {
            ppc_draw_system_menu_mark(memory, front_buffer, screen_clut, highlight.title_h);
        }
        if let Some((rect, background)) = highlight.dim_pattern {
            ppc_apply_menu_title_dim_pattern(memory, front_buffer, rect, background);
        }
    }
    true
}

pub(crate) fn ppc_service_invalid_menu_bar(
    event_queue: &mut EventQueue,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    gworlds: &[PpcGWorldRecord],
    menu_list_handle: u32,
    screen_clut: &[[u16; 3]; 256],
    startup: &mut PpcToolboxStartupState,
) -> bool {
    if !event_queue.take_menu_bar_invalidation() {
        return false;
    }
    startup.menu_bar_draw_count = startup.menu_bar_draw_count.saturating_add(1);
    if !startup.host_menu_bar_hidden {
        let menu_color_bytes = ppc_menu_color_table_bytes(memory, handles);
        let _ = ppc_draw_menu_bar_with_colors(
            memory,
            gworlds,
            menu_list_handle,
            screen_clut,
            MenuColorTable::new(&menu_color_bytes),
        );
    }
    true
}

#[cfg(test)]
pub(crate) fn ppc_draw_menu_bar(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    menu_list_handle: u32,
    screen_clut: &[[u16; 3]; 256],
) -> bool {
    ppc_draw_menu_bar_with_colors(
        memory,
        gworlds,
        menu_list_handle,
        screen_clut,
        MenuColorTable::new(&[]),
    )
}

pub(crate) fn ppc_draw_system_menu_mark(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    screen_clut: &[[u16; 3]; 256],
    left: i16,
) {
    let palette = crate::ui_art::RETRO_COMPUTER_MENU_MARK_PALETTE.map(|rgb| {
        ppc_physical_screen_color_pixel(
            front_buffer,
            PpcRgbColor {
                red: rgb[0],
                green: rgb[1],
                blue: rgb[2],
            },
            screen_clut,
        )
        .unwrap_or(0)
    });
    let menu_bar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
    let top = ppc_menu_bar_system_mark_top(menu_bar_height);
    let menu_bar_bottom = menu_bar_height.saturating_sub(1).max(0);
    for (dy, row) in crate::ui_art::RETRO_COMPUTER_MENU_MARK_PIXELS
        .iter()
        .enumerate()
    {
        for (dx, palette_index) in row.iter().copied().enumerate() {
            if palette_index == 0 {
                continue;
            }
            let y = top.saturating_add(dy as i16);
            if y < 0 || y >= menu_bar_bottom {
                continue;
            }
            let _ = ppc_quickdraw_write_raw_pixel(
                memory,
                front_buffer,
                (i32::from(left) + dx as i32, i32::from(y)),
                palette[usize::from(palette_index - 1)],
            );
        }
    }
}

pub(crate) fn ppc_menu_items_from_memory(memory: &mut PpcSectionMem, menu_handle: u32) -> Option<MenuItems> {
    let menu = memory.read_u32_be(menu_handle).filter(|ptr| *ptr != 0)?;
    MenuItems::decode_with(|offset| {
        let offset = u32::try_from(offset).ok()?;
        memory.read_u8(menu.checked_add(offset)?)
    })
}

pub(crate) fn ppc_menu_first_item(memory: &mut PpcSectionMem, menu_handle: u32) -> Option<u32> {
    if menu_handle == 0 {
        return None;
    }
    let menu = memory.read_u32_be(menu_handle)?;
    if menu == 0 {
        return None;
    }
    // Macintosh Toolbox Essentials (1992), pp. 3-132 and 3-141, together
    // with the MENU layout in Inside Macintosh Volume I, p. I-345: the menu
    // title is followed by Pascal-string items and four attribute bytes.
    let title_len = u32::from(memory.read_u8(menu.checked_add(14)?)?);
    menu.checked_add(15u32.checked_add(title_len)?)
}

pub(crate) fn ppc_menu_item(memory: &mut PpcSectionMem, menu_handle: u32, item: i16) -> Option<(u32, u8)> {
    if item < 1 {
        return None;
    }
    let mut item_addr = ppc_menu_first_item(memory, menu_handle)?;
    for index in 1..=1024i16 {
        let item_len = memory.read_u8(item_addr)?;
        if item_len == 0 {
            return None;
        }
        if index == item {
            return Some((item_addr, item_len));
        }
        item_addr = item_addr.checked_add(5 + u32::from(item_len))?;
    }
    None
}

