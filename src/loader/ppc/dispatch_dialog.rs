//! Typed Dialog Manager dispatch for PowerPC imports.

use super::*;
use crate::trap::types::decode_mac_roman;

pub(super) const PPC_DIALOG_RECORD_SIZE: u32 = 256;
pub(super) const PPC_DIALOG_ITEMS_OFFSET: u32 = 156;
pub(super) const PPC_DIALOG_TEXT_HANDLE_OFFSET: u32 = 160;
pub(super) const PPC_DIALOG_EDIT_FIELD_OFFSET: u32 = 164;
pub(super) const PPC_DIALOG_EDIT_OPEN_OFFSET: u32 = 166;
pub(super) const PPC_DIALOG_DEFAULT_ITEM_OFFSET: u32 = 168;
pub(super) const PPC_DIALOG_RESOURCE_ID_OFFSET: u32 = 170;
// Host-private Dialog Manager state follows the documented DialogRecord. The
// System 7 cancel-item API has no canonical public record field.
pub(super) const PPC_DIALOG_CANCEL_ITEM_HLE_OFFSET: u32 = 172;
pub(super) const PPC_DIALOG_ALERT_HIT_HLE_OFFSET: u32 = 174;
pub(super) const PPC_DIALOG_ITEM_DISABLED: u8 = 0x80;
pub(super) const PPC_DIALOG_ITEM_USER_ITEM: u8 = 0;
pub(super) const PPC_DIALOG_ITEM_BUTTON: u8 = 4;
pub(super) const PPC_DIALOG_ITEM_CHECKBOX: u8 = 5;
pub(super) const PPC_DIALOG_ITEM_RADIO: u8 = 6;
pub(super) const PPC_DIALOG_ITEM_RESOURCE_CONTROL: u8 = 7;
pub(super) const PPC_DIALOG_ITEM_STATIC_TEXT: u8 = 8;
pub(super) const PPC_DIALOG_ITEM_EDIT_TEXT: u8 = 16;
pub(super) const PPC_DIALOG_ITEM_ICON: u8 = 32;
pub(super) const PPC_DIALOG_ITEM_PICTURE: u8 = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcDialogTemplate {
    pub(super) bounds: (i16, i16, i16, i16),
    pub(super) proc_id: i16,
    pub(super) visible: bool,
    pub(super) go_away: bool,
    pub(super) ref_con: u32,
    pub(super) items_id: i16,
    pub(super) title: Vec<u8>,
    pub(super) position: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcDialogItemView {
    pub(super) item_offset: usize,
    pub(super) item_type: u8,
    pub(super) rect: (i16, i16, i16, i16),
    pub(super) handle: u32,
    pub(super) payload: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PpcDialogCallbackCompletion {
    ReturnPreserve,
    Return(u32),
    Yield,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcDialogCallbackState {
    pub(super) import_pc: u32,
    pub(super) dialog: u32,
    pub(super) callbacks: Vec<(PpcCallbackTarget, u32)>,
    pub(super) next_callback: usize,
    pub(super) final_pc: u32,
    pub(super) restore_rtoc: u32,
    pub(super) completion: PpcDialogCallbackCompletion,
}

pub(super) struct PpcDialogDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) controls: &'a mut Vec<PpcControlRecord>,
    pub(super) gworlds: &'a mut Vec<PpcGWorldRecord>,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
    pub(super) current_gworld: &'a mut u32,
    pub(super) current_gdevice: &'a mut u32,
    pub(super) window_list: &'a SharedProcessWindowList,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) event_queue: &'a mut EventQueue,
    pub(super) dialog_callback_stack: &'a mut Vec<PpcDialogCallbackState>,
    pub(super) vfs_resources: &'a mut [PpcVfsResourceRecord],
    pub(super) current_resource_refnum: i16,
    pub(super) last_resource_error: &'a mut i16,
    pub(super) param_text: &'a SharedProcessDialogText,
    pub(super) tick_count: u32,
    pub(super) input: PpcInputSnapshot,
    pub(super) quickdraw_text_mode: i16,
    pub(super) quickdraw_text_size: i16,
    pub(super) quickdraw_fore_color: &'a mut PpcRgbColor,
    pub(super) quickdraw_back_color: &'a mut PpcRgbColor,
    pub(super) quickdraw_fore_indices: &'a mut HashMap<u32, u8>,
}

pub(super) fn dispatch_dialog_import(
    context: PpcDialogDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcDialogDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        controls,
        gworlds,
        screen_clut,
        color_manager_clut,
        current_gworld,
        current_gdevice,
        window_list,
        toolbox_startup,
        event_queue,
        dialog_callback_stack,
        vfs_resources,
        current_resource_refnum,
        last_resource_error,
        param_text,
        tick_count,
        input,
        quickdraw_text_mode,
        quickdraw_text_size,
        quickdraw_fore_color,
        quickdraw_back_color,
        quickdraw_fore_indices,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InitDialogs => {
            toolbox_startup.dialogs_initialized = true;
            toolbox_startup.dialog_resume_proc = cpu.gpr[3];
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetNewDialog => {
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] GetNewDialog id={} storage=${:08X} behind=${:08X} lr=${:08X}",
                    cpu.gpr[3] as u16 as i16, cpu.gpr[4], cpu.gpr[5], cpu.lr
                );
            }
            let dialog = ppc_get_new_dialog(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                controls,
                gworlds,
                window_list,
                *current_gdevice,
                vfs_resources,
                current_resource_refnum,
                last_resource_error,
                param_text,
            );
            if dialog != 0 {
                *current_gworld = dialog;
                *current_gdevice = ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
                if ppc_window_is_visible(memory, dialog) {
                    ppc_enqueue_window_update_event(event_queue, dialog, tick_count, input);
                }
            }
            Some(PpcImportAction::Return(dialog))
        }
        PpcImportDispatcherTarget::NewDialog => {
            let dialog = ppc_new_dialog(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                window_list,
                *current_gdevice,
            );
            let dialog = if dialog != 0
                && !ppc_initialize_dialog_items(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    param_text,
                    dialog,
                    vfs_resources,
                    current_resource_refnum,
                    last_resource_error,
                ) {
                0
            } else {
                dialog
            };
            if dialog != 0 {
                *current_gworld = dialog;
                *current_gdevice = ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
                if ppc_window_is_visible(memory, dialog) {
                    ppc_enqueue_window_update_event(event_queue, dialog, tick_count, input);
                }
            }
            Some(PpcImportAction::Return(dialog))
        }
        PpcImportDispatcherTarget::NewFeaturesDialog => {
            // Universal Interfaces 3.4.1 Dialogs.h declares the first nine
            // arguments identically to NewDialog and appends an Appearance
            // flags word. PPC native ABI therefore already leaves the DITL
            // handle in parameter-area slot 8, exactly where ppc_new_dialog
            // reads it; Systemless renders the standard non-themed dialog.
            let dialog = ppc_new_dialog(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                window_list,
                *current_gdevice,
            );
            let dialog = if dialog != 0
                && !ppc_initialize_dialog_items(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    param_text,
                    dialog,
                    vfs_resources,
                    current_resource_refnum,
                    last_resource_error,
                ) {
                0
            } else {
                dialog
            };
            if dialog != 0 {
                *current_gworld = dialog;
                *current_gdevice = ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
                if ppc_window_is_visible(memory, dialog) {
                    ppc_enqueue_window_update_event(event_queue, dialog, tick_count, input);
                }
            }
            Some(PpcImportAction::Return(dialog))
        }
        PpcImportDispatcherTarget::DisposeDialog => {
            let window = cpu.gpr[3];
            toolbox_startup.dispose_dialog_count =
                toolbox_startup.dispose_dialog_count.saturating_add(1);
            toolbox_startup.last_disposed_dialog = window;
            ppc_close_window(
                window,
                memory,
                process_memory_manager,
                window_list,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                current_gworld,
                current_gdevice,
                screen_clut,
                color_manager_clut,
                toolbox_startup,
                event_queue,
                tick_count,
                input,
                quickdraw_fore_color,
                quickdraw_back_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetDialogItem => {
            ppc_get_dialog_item(cpu, memory, handles);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetDialogItem => {
            ppc_set_dialog_item(cpu, memory, handles);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetDialogItemText => {
            // Macintosh Toolbox Essentials 1992, 6-130 through 6-131:
            // GetDialogItemText returns at most 255 bytes from the text-item
            // handle in its Str255 output parameter.
            let item_handle = cpu.gpr[3];
            let text_out_ptr = cpu.gpr[4];
            if text_out_ptr != 0 {
                let bytes = ppc_handle_bytes(memory, handles, item_handle).unwrap_or_default();
                let len = bytes.len().min(255);
                if ppc_memory_can_write_bytes(memory, text_out_ptr, len as u32 + 1) {
                    let _ = memory.write_u8(text_out_ptr, len as u8);
                    for (offset, byte) in bytes.iter().copied().take(len).enumerate() {
                        let _ = memory.write_u8(text_out_ptr + 1 + offset as u32, byte);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetDialogItemText => {
            // Macintosh Toolbox Essentials 1992, 6-131: SetDialogItemText
            // replaces the contents of a statText/editText item handle with
            // the supplied Str255. Drawing is handled by the PPC game's own
            // dialog/window rendering path; preserve this routine's void ABI.
            let item_handle = cpu.gpr[3];
            let text_ptr = cpu.gpr[4];
            let text = ppc_read_pstring_bytes(memory, text_ptr).unwrap_or_default();
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] SetDialogItemText handle=${item_handle:08X} text={:?}",
                    decode_mac_roman(&text)
                );
            }
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = allocator.resize_handle(
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                item_handle,
                text.len() as u32,
            );
            *last_mem_error = result;
            if *last_mem_error == PPC_NO_ERR {
                if let Some(ptr) = memory.read_u32_be(item_handle) {
                    for (offset, byte) in text.iter().copied().enumerate() {
                        let _ = memory.write_u8(ptr + offset as u32, byte);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetDialogDefaultItem => {
            // Appearance Manager 1.0 SetDialogDefaultItem records which item
            // Return activates and reports an OSErr.
            let dialog = cpu.gpr[3];
            let item = cpu.gpr[4] as u16;
            let result = if dialog != 0
                && memory
                    .write_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET, item)
                    .is_some()
            {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::SetDialogCancelItem => {
            // Macintosh Toolbox Essentials (1992), p. 6-165: the System 7
            // cancel item is Dialog Manager state rather than a public
            // DialogRecord field. Keep it in the HLE tail of our allocation.
            let dialog = cpu.gpr[3];
            let item = cpu.gpr[4] as u16;
            let result = if dialog != 0
                && memory
                    .write_u16_be(dialog + PPC_DIALOG_CANCEL_ITEM_HLE_OFFSET, item)
                    .is_some()
            {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DrawDialog => {
            if let Some(action) = ppc_resume_dialog_callbacks(cpu, memory, dialog_callback_stack) {
                return Some(action);
            }
            let dialog = cpu.gpr[3];
            *current_gworld = dialog;
            *current_gdevice = ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
            let bounds = ppc_dialog_global_bounds(memory, gworlds, dialog);
            let items = ppc_dialog_items_for_dialog(memory, handles, dialog);
            let _ = ppc_draw_dialog(
                memory,
                handles,
                controls,
                gworlds,
                screen_clut,
                vfs_resources,
                current_resource_refnum,
                dialog,
            );
            Some(match (bounds, items) {
                (Some(bounds), Some(items)) => ppc_begin_dialog_callbacks(
                    cpu,
                    memory,
                    dialog_callback_stack,
                    dialog,
                    &items,
                    bounds,
                    PpcDialogCallbackCompletion::ReturnPreserve,
                ),
                _ => PpcImportAction::ReturnPreserve,
            })
        }
        PpcImportDispatcherTarget::ModalDialog => Some(ppc_modal_dialog(
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            gworlds,
            current_gworld,
            current_gdevice,
            screen_clut,
            *quickdraw_fore_color,
            quickdraw_fore_indices,
            input,
            event_queue,
            dialog_callback_stack,
            vfs_resources,
            current_resource_refnum,
        )),
        PpcImportDispatcherTarget::SelectDialogItemText => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_select_dialog_item_text(
                Some(&mut allocator),
                None,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4] as u16 as usize,
                cpu.gpr[5] as u16,
                cpu.gpr[6] as u16,
                tick_count,
                quickdraw_text_mode,
                quickdraw_text_size,
                *quickdraw_fore_color,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ParamText => {
            param_text.with_mut(|slots| ppc_param_text(cpu, memory, slots));
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::AlertReturnDefault => {
            let alert_id = cpu.gpr[3] as u16 as i16;
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] Alert id={} params={:?}",
                    alert_id,
                    param_text
                        .iter()
                        .map(|bytes| decode_mac_roman(bytes))
                        .collect::<Vec<_>>()
                );
            }
            let mut dialog = gworlds.iter().rev().find_map(|record| {
                (memory.read_u16_be(record.port + PPC_CWINDOW_WINDOW_KIND_OFFSET) == Some(2)
                    && ppc_window_is_visible(memory, record.port)
                    && memory.read_u16_be(record.port + PPC_DIALOG_RESOURCE_ID_OFFSET)
                        == Some(alert_id as u16))
                .then_some(record.port)
            });
            if dialog.is_none() {
                let created = ppc_new_alert_dialog(
                    cpu,
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    gworlds,
                    window_list,
                    *current_gdevice,
                    vfs_resources,
                    current_resource_refnum,
                    last_resource_error,
                    alert_id,
                    param_text,
                );
                if created == 0 {
                    return Some(PpcImportAction::Return(ppc_i16_result(-1)));
                }
                *current_gworld = created;
                *current_gdevice = ppc_gworld_device(gworlds, created).unwrap_or(*current_gdevice);
                let _ = ppc_draw_dialog(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    screen_clut,
                    vfs_resources,
                    current_resource_refnum,
                    created,
                );
                dialog = Some(created);
            }
            let dialog = dialog.unwrap();
            *current_gworld = dialog;
            *current_gdevice = ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
            let mut modal_cpu = cpu.clone();
            modal_cpu.gpr[4] = dialog + PPC_DIALOG_ALERT_HIT_HLE_OFFSET;
            let action = ppc_modal_dialog(
                &mut modal_cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                controls,
                gworlds,
                current_gworld,
                current_gdevice,
                screen_clut,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
                input,
                event_queue,
                dialog_callback_stack,
                vfs_resources,
                current_resource_refnum,
            );
            if matches!(action, PpcImportAction::ReturnPreserve) {
                let hit = memory
                    .read_u16_be(dialog + PPC_DIALOG_ALERT_HIT_HLE_OFFSET)
                    .unwrap_or(1);
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                ppc_dispose_window(
                    &mut allocator,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    gworlds,
                    window_list,
                    current_gworld,
                    current_gdevice,
                    dialog,
                );
                if *current_gworld != PPC_MAIN_GWORLD {
                    ppc_enqueue_window_update_event(
                        event_queue,
                        *current_gworld,
                        tick_count,
                        input,
                    );
                }
                Some(PpcImportAction::Return(u32::from(hit)))
            } else {
                Some(action)
            }
        }
        PpcImportDispatcherTarget::DialogCompatibility(operation) => {
            Some(ppc_dispatch_dialog_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                controls,
                gworlds,
                screen_clut,
                current_gworld,
                current_gdevice,
                dialog_callback_stack,
                vfs_resources,
                current_resource_refnum,
            ))
        }
        _ => None,
    }
}

fn ppc_param_text(cpu: &mut PpcCpu, memory: &mut PpcSectionMem, param_text: &mut [Vec<u8>; 4]) {
    for (index, slot) in param_text.iter_mut().enumerate() {
        let ptr = cpu.gpr[3 + index];
        if ptr == 0 {
            continue;
        }
        if let Some(bytes) = ppc_read_pstring_bytes(memory, ptr) {
            *slot = bytes;
        }
    }
    if ppc_hle_trace_enabled() {
        let strings = param_text
            .iter()
            .map(|bytes| decode_mac_roman(bytes))
            .collect::<Vec<_>>();
        eprintln!("[PPC-TRACE] ParamText strings={:?}", strings);
    }
}

pub(super) fn ppc_apply_param_text<'a>(
    text: &'a [u8],
    param_text: &[Vec<u8>; 4],
) -> std::borrow::Cow<'a, [u8]> {
    if !text.contains(&b'^') {
        return std::borrow::Cow::Borrowed(text);
    }
    let mut expanded = Vec::with_capacity(text.len());
    let mut offset = 0;
    while offset < text.len() {
        if text[offset] == b'^' {
            if let Some(index) = text
                .get(offset + 1)
                .and_then(|byte| byte.checked_sub(b'0'))
                .filter(|index| usize::from(*index) < param_text.len())
            {
                expanded.extend_from_slice(&param_text[usize::from(index)]);
                offset += 2;
                continue;
            }
        }
        expanded.push(text[offset]);
        offset += 1;
    }
    std::borrow::Cow::Owned(expanded)
}

fn ppc_dialog_live_items(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    dialog: u32,
) -> Option<(u32, u32, Vec<u8>, Vec<PpcDialogItemView>)> {
    let handle = memory.read_u32_be(dialog.checked_add(PPC_DIALOG_ITEMS_OFFSET)?)?;
    let ptr = memory.read_u32_be(handle)?;
    let bytes = ppc_handle_bytes(memory, handles, handle)?;
    let items = ppc_parse_dialog_items(&bytes)?;
    Some((handle, ptr, bytes, items))
}

fn ppc_dialog_item_end(bytes: &[u8], item: &PpcDialogItemView) -> Option<usize> {
    let payload_len = usize::from(*bytes.get(item.item_offset.checked_add(13)?)?);
    Some((item.item_offset.checked_add(14 + payload_len)? + 1) & !1)
}

fn ppc_offset_ditl_items(bytes: &mut [u8], items: &[PpcDialogItemView], dv: i16, dh: i16) {
    for item in items {
        let offset = item.item_offset;
        for (coordinate_offset, delta) in [(4usize, dv), (6, dh), (8, dv), (10, dh)] {
            let Some(start) = offset.checked_add(coordinate_offset) else {
                continue;
            };
            let Some(pair) = bytes.get(start..start.saturating_add(2)) else {
                continue;
            };
            let value = i16::from_be_bytes([pair[0], pair[1]]).saturating_add(delta);
            if let Some(destination) = bytes.get_mut(start..start.saturating_add(2)) {
                destination.copy_from_slice(&value.to_be_bytes());
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn ppc_dispatch_dialog_compatibility(
    operation: PpcDialogCompatibilityOperation,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &[PpcControlRecord],
    gworlds: &mut Vec<PpcGWorldRecord>,
    screen_clut: &[[u16; 3]; 256],
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    dialog_callback_stack: &mut Vec<PpcDialogCallbackState>,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> PpcImportAction {
    let dialog = cpu.gpr[3];
    match operation {
        PpcDialogCompatibilityOperation::IsDialogEvent => {
            let result = ppc_read_dialog_event(memory, cpu.gpr[3])
                .and_then(|event| {
                    let dialog = ppc_dialog_for_event(memory, gworlds, event.what, event.message)?;
                    Some(match event.what {
                        6 | 8 => event.message == dialog,
                        1 => ppc_dialog_global_bounds(memory, gworlds, dialog).is_some_and(
                            |bounds| {
                                event.where_v >= bounds.0
                                    && event.where_v < bounds.2
                                    && event.where_h >= bounds.1
                                    && event.where_h < bounds.3
                            },
                        ),
                        _ => true,
                    })
                })
                .unwrap_or(false);
            PpcImportAction::Return(u32::from(result))
        }
        PpcDialogCompatibilityOperation::DialogSelect => {
            if let Some(action) = ppc_resume_dialog_callbacks(cpu, memory, dialog_callback_stack) {
                return action;
            }
            let event_ptr = cpu.gpr[3];
            let dialog_out_ptr = cpu.gpr[4];
            let item_hit_ptr = cpu.gpr[5];
            let Some(event) = ppc_read_dialog_event(memory, event_ptr) else {
                return PpcImportAction::Return(0);
            };
            let Some(dialog) = ppc_dialog_for_event(memory, gworlds, event.what, event.message)
            else {
                return PpcImportAction::Return(0);
            };
            let Some(bounds) = ppc_dialog_global_bounds(memory, gworlds, dialog) else {
                return PpcImportAction::Return(0);
            };
            let Some(items) = ppc_dialog_items_for_dialog(memory, handles, dialog) else {
                return PpcImportAction::Return(0);
            };
            if matches!(event.what, 6 | 8) && dialog_out_ptr != 0 {
                let _ = memory.write_u32_be(dialog_out_ptr, dialog);
            }
            match event.what {
                6 if event.message == dialog => {
                    *current_gworld = dialog;
                    *current_gdevice =
                        ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
                    let _ = ppc_draw_dialog(
                        memory,
                        handles,
                        controls,
                        gworlds,
                        screen_clut,
                        vfs_resources,
                        current_resource_refnum,
                        dialog,
                    );
                    ppc_begin_dialog_callbacks(
                        cpu,
                        memory,
                        dialog_callback_stack,
                        dialog,
                        &items,
                        bounds,
                        PpcDialogCallbackCompletion::Return(0),
                    )
                }
                1 if event.where_v >= bounds.0
                    && event.where_v < bounds.2
                    && event.where_h >= bounds.1
                    && event.where_h < bounds.3 =>
                {
                    let Some(hit) = ppc_dialog_item_at_global_point(
                        &items,
                        bounds,
                        event.where_v,
                        event.where_h,
                    ) else {
                        return PpcImportAction::Return(0);
                    };
                    if dialog_out_ptr != 0 {
                        let _ = memory.write_u32_be(dialog_out_ptr, dialog);
                    }
                    if item_hit_ptr != 0 {
                        let _ = memory.write_u16_be(item_hit_ptr, hit);
                    }
                    if items
                        .get(usize::from(hit).saturating_sub(1))
                        .is_some_and(|item| {
                            item.item_type & !PPC_DIALOG_ITEM_DISABLED == PPC_DIALOG_ITEM_EDIT_TEXT
                        })
                    {
                        let te_handle = memory
                            .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
                            .unwrap_or(0);
                        ppc_te_click(
                            memory,
                            handles,
                            te_handle,
                            event.where_v.saturating_sub(bounds.0),
                            event.where_h.saturating_sub(bounds.1),
                            event.modifiers & 0x0200 != 0,
                            event.when,
                        );
                    }
                    PpcImportAction::Return(1)
                }
                3 | 5 => {
                    let te_handle = memory
                        .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
                        .unwrap_or(0);
                    let edit_item = memory
                        .read_u16_be(dialog + PPC_DIALOG_EDIT_FIELD_OFFSET)
                        .unwrap_or(u16::MAX)
                        .saturating_add(1);
                    let character = event.message as u8;
                    let editable = edit_item != 0
                        && items
                            .get(usize::from(edit_item).saturating_sub(1))
                            .is_some_and(|item| {
                                item.item_type & !PPC_DIALOG_ITEM_DISABLED
                                    == PPC_DIALOG_ITEM_EDIT_TEXT
                            });
                    if !editable || !matches!(character, 0x08 | 0x20..=0x7e) {
                        return PpcImportAction::Return(0);
                    }
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    let result = ppc_te_key(
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        te_handle,
                        character,
                    );
                    *last_mem_error = result;
                    if *last_mem_error != PPC_NO_ERR {
                        return PpcImportAction::Return(0);
                    }
                    if dialog_out_ptr != 0 {
                        let _ = memory.write_u32_be(dialog_out_ptr, dialog);
                    }
                    if item_hit_ptr != 0 {
                        let _ = memory.write_u16_be(item_hit_ptr, edit_item);
                    }
                    PpcImportAction::Return(1)
                }
                _ => PpcImportAction::Return(0),
            }
        }
        PpcDialogCompatibilityOperation::CountDitl => PpcImportAction::Return(
            ppc_dialog_items_for_dialog(memory, handles, dialog)
                .and_then(|items| u32::try_from(items.len()).ok())
                .unwrap_or(0),
        ),
        PpcDialogCompatibilityOperation::FindDialogItem => {
            let point = cpu.gpr[4];
            let v = (point >> 16) as u16 as i16;
            let h = point as u16 as i16;
            let found = ppc_dialog_items_for_dialog(memory, handles, dialog)
                .and_then(|items| {
                    items.iter().enumerate().find_map(|(index, item)| {
                        let rect = item.rect;
                        (v >= rect.0 && v < rect.2 && h >= rect.1 && h < rect.3)
                            .then(|| u32::try_from(index + 1).unwrap_or(u32::MAX))
                    })
                })
                .unwrap_or(0);
            PpcImportAction::Return(found)
        }
        PpcDialogCompatibilityOperation::HideDialogItem
        | PpcDialogCompatibilityOperation::ShowDialogItem => {
            if let Some((_handle, ptr, _bytes, items)) =
                ppc_dialog_live_items(memory, handles, dialog)
            {
                let item_number = cpu.gpr[4] as u16 as usize;
                if let Some(item) = item_number
                    .checked_sub(1)
                    .and_then(|index| items.get(index))
                {
                    let left = item.rect.1;
                    let hide = operation == PpcDialogCompatibilityOperation::HideDialogItem;
                    let should_move = if hide { left < 0x2000 } else { left > 0x2000 };
                    if should_move {
                        let delta = if hide { 0x4000i16 } else { -0x4000i16 };
                        let item_addr = ptr + item.item_offset as u32;
                        let _ = memory
                            .write_u16_be(item_addr + 6, item.rect.1.wrapping_add(delta) as u16);
                        let _ = memory
                            .write_u16_be(item_addr + 10, item.rect.3.wrapping_add(delta) as u16);
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcDialogCompatibilityOperation::AppendDitl => {
            let Some((handle, _ptr, current_bytes, current_items)) =
                ppc_dialog_live_items(memory, handles, dialog)
            else {
                return PpcImportAction::ReturnPreserve;
            };
            let Some(mut appended_bytes) = ppc_handle_bytes(memory, handles, cpu.gpr[4]) else {
                return PpcImportAction::ReturnPreserve;
            };
            let Some(appended_items) = ppc_parse_dialog_items(&appended_bytes) else {
                return PpcImportAction::ReturnPreserve;
            };
            let method = cpu.gpr[5] as u16 as i16;
            let (dv, dh) = match method {
                1 => (
                    0,
                    gworlds
                        .iter()
                        .find(|gworld| gworld.port == dialog)
                        .map_or(0, |gworld| ppc_u32_to_i16_saturating(gworld.width)),
                ),
                2 => (
                    gworlds
                        .iter()
                        .find(|gworld| gworld.port == dialog)
                        .map_or(0, |gworld| ppc_u32_to_i16_saturating(gworld.height)),
                    0,
                ),
                value if value < 0 => current_items
                    .get(usize::from(value.unsigned_abs()).saturating_sub(1))
                    .map_or((0, 0), |item| (item.rect.0, item.rect.1)),
                _ => (0, 0),
            };
            ppc_offset_ditl_items(&mut appended_bytes, &appended_items, dv, dh);
            let total_count = current_items.len().saturating_add(appended_items.len());
            let mut combined = current_bytes;
            combined.extend_from_slice(appended_bytes.get(2..).unwrap_or_default());
            let count_minus_one = total_count.saturating_sub(1).min(i16::MAX as usize) as i16;
            combined[0..2].copy_from_slice(&count_minus_one.to_be_bytes());
            let size = u32::try_from(combined.len()).unwrap_or(u32::MAX);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result =
                allocator.resize_handle(memory, heap_cursor, last_mem_error, handles, handle, size);
            *last_mem_error = result;
            if result == PPC_NO_ERR {
                if let Some(ptr) = memory.read_u32_be(handle) {
                    let _ = memory.write_bytes(ptr, &combined);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcDialogCompatibilityOperation::ShortenDitl => {
            let Some((handle, _ptr, mut bytes, items)) =
                ppc_dialog_live_items(memory, handles, dialog)
            else {
                return PpcImportAction::ReturnPreserve;
            };
            let remove = usize::from(cpu.gpr[4] as u16).min(items.len());
            let retained = items.len().saturating_sub(remove);
            let end = if retained == 0 {
                2
            } else {
                ppc_dialog_item_end(&bytes, &items[retained - 1]).unwrap_or(bytes.len())
            };
            bytes.truncate(end);
            let count_minus_one = if retained == 0 {
                -1
            } else {
                retained.saturating_sub(1).min(i16::MAX as usize) as i16
            };
            bytes[0..2].copy_from_slice(&count_minus_one.to_be_bytes());
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = allocator.resize_handle(
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                handle,
                u32::try_from(bytes.len()).unwrap_or(u32::MAX),
            );
            *last_mem_error = result;
            if result == PPC_NO_ERR {
                if let Some(ptr) = memory.read_u32_be(handle) {
                    let _ = memory.write_bytes(ptr, &bytes);
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcDialogCompatibilityOperation::UpdateDialog => {
            if let Some(action) = ppc_resume_dialog_callbacks(cpu, memory, dialog_callback_stack) {
                return action;
            }
            *current_gworld = dialog;
            *current_gdevice = ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
            let bounds = ppc_dialog_global_bounds(memory, gworlds, dialog);
            let items = ppc_dialog_items_for_dialog(memory, handles, dialog);
            let _ = ppc_draw_dialog(
                memory,
                handles,
                controls,
                gworlds,
                screen_clut,
                vfs_resources,
                current_resource_refnum,
                dialog,
            );
            match (bounds, items) {
                (Some(bounds), Some(items)) => ppc_begin_dialog_callbacks(
                    cpu,
                    memory,
                    dialog_callback_stack,
                    dialog,
                    &items,
                    bounds,
                    PpcDialogCallbackCompletion::ReturnPreserve,
                ),
                _ => PpcImportAction::ReturnPreserve,
            }
        }
    }
}

fn ppc_dialog_be_i16(bytes: &[u8], offset: usize) -> Option<i16> {
    Some(i16::from_be_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset.checked_add(1)?)?,
    ]))
}

fn ppc_dialog_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes([
        *bytes.get(offset)?,
        *bytes.get(offset.checked_add(1)?)?,
        *bytes.get(offset.checked_add(2)?)?,
        *bytes.get(offset.checked_add(3)?)?,
    ]))
}

fn ppc_parse_dialog_template(bytes: &[u8]) -> Option<PpcDialogTemplate> {
    // Macintosh Toolbox Essentials (1992), pp. 6-113--6-114: a DLOG
    // contains the window rectangle, proc ID, visibility/go-away flags,
    // refCon, DITL ID, Pascal title, and (on System 7) an optional
    // positioning word after even-byte alignment.
    let bounds = (
        ppc_dialog_be_i16(bytes, 0)?,
        ppc_dialog_be_i16(bytes, 2)?,
        ppc_dialog_be_i16(bytes, 4)?,
        ppc_dialog_be_i16(bytes, 6)?,
    );
    let title_len = usize::from(*bytes.get(20)?);
    let title_end = 21usize.checked_add(title_len)?;
    let title = bytes.get(21..title_end)?.to_vec();
    let position_offset = (title_end + 1) & !1;
    let position = bytes
        .get(position_offset..position_offset.saturating_add(2))
        .and_then(|value| value.try_into().ok())
        .map(u16::from_be_bytes)
        .unwrap_or(0);
    Some(PpcDialogTemplate {
        bounds,
        proc_id: ppc_dialog_be_i16(bytes, 8)?,
        visible: *bytes.get(10)? != 0,
        go_away: *bytes.get(12)? != 0,
        ref_con: ppc_dialog_be_u32(bytes, 14)?,
        items_id: ppc_dialog_be_i16(bytes, 18)?,
        title,
        position,
    })
}

fn ppc_parse_dialog_items(bytes: &[u8]) -> Option<Vec<PpcDialogItemView>> {
    // Macintosh Toolbox Essentials (1992), pp. 6-120--6-121: DITL starts
    // with countMinusOne. Each even-aligned item has a four-byte handle,
    // Rect, type byte, length byte, and length bytes of item data.
    let count_minus_one = ppc_dialog_be_i16(bytes, 0)?;
    let count = if count_minus_one < 0 {
        0
    } else {
        usize::try_from(count_minus_one).ok()?.checked_add(1)?
    };
    let mut offset = 2usize;
    let mut items = Vec::with_capacity(count);
    for _ in 0..count {
        let item_type = *bytes.get(offset.checked_add(12)?)?;
        let payload_len = usize::from(*bytes.get(offset.checked_add(13)?)?);
        let payload_start = offset.checked_add(14)?;
        let payload_end = payload_start.checked_add(payload_len)?;
        items.push(PpcDialogItemView {
            item_offset: offset,
            item_type,
            rect: (
                ppc_dialog_be_i16(bytes, offset.checked_add(4)?)?,
                ppc_dialog_be_i16(bytes, offset.checked_add(6)?)?,
                ppc_dialog_be_i16(bytes, offset.checked_add(8)?)?,
                ppc_dialog_be_i16(bytes, offset.checked_add(10)?)?,
            ),
            handle: ppc_dialog_be_u32(bytes, offset)?,
            payload: bytes.get(payload_start..payload_end)?.to_vec(),
        });
        offset = (payload_end + 1) & !1;
    }
    Some(items)
}

fn ppc_position_dialog_bounds(
    bounds: (i16, i16, i16, i16),
    position: u16,
    gworlds: &[PpcGWorldRecord],
) -> (i16, i16, i16, i16) {
    let centered = matches!(
        position,
        0x280a | 0x300a | 0x380a | 0xa80a | 0xb00a | 0xb80a
    );
    if !centered {
        return bounds;
    }
    let screen = gworlds.iter().find(|record| record.port == PPC_MAIN_GWORLD);
    let screen_width = screen.map_or(PPC_MAIN_SCREEN_WIDTH, |record| record.width) as i32;
    let screen_height = screen.map_or(PPC_MAIN_SCREEN_HEIGHT, |record| record.height) as i32;
    let height = i32::from(bounds.2) - i32::from(bounds.0);
    let width = i32::from(bounds.3) - i32::from(bounds.1);
    let top = (screen_height - height).max(0) / 2;
    let left = (screen_width - width).max(0) / 2;
    (
        ppc_i32_to_i16_saturating(top),
        ppc_i32_to_i16_saturating(left),
        ppc_i32_to_i16_saturating(top + height),
        ppc_i32_to_i16_saturating(left + width),
    )
}

#[allow(clippy::too_many_arguments)]
fn ppc_new_alert_dialog(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    current_gdevice: u32,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
    alert_id: i16,
    param_text: &[Vec<u8>; 4],
) -> u32 {
    let Some(alert_index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"ALRT"),
        alert_id,
        false,
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let alert = &vfs_resources[alert_index].data;
    if alert.len() < 12 {
        *last_resource_error = PPC_PARAM_ERR;
        return 0;
    }
    let bounds = (
        i16::from_be_bytes([alert[0], alert[1]]),
        i16::from_be_bytes([alert[2], alert[3]]),
        i16::from_be_bytes([alert[4], alert[5]]),
        i16::from_be_bytes([alert[6], alert[7]]),
    );
    let items_id = i16::from_be_bytes([alert[8], alert[9]]);
    let stages = u16::from_be_bytes([alert[10], alert[11]]);
    let position = alert
        .get(12..14)
        .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
        .unwrap_or(0);
    let Some(ditl_index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"DITL"),
        items_id,
        false,
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let ditl_bytes = vfs_resources[ditl_index].data.clone();
    if ppc_parse_dialog_items(&ditl_bytes).is_none() {
        *last_resource_error = PPC_PARAM_ERR;
        return 0;
    }
    let items_handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        &ditl_bytes,
    );
    if items_handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    let bounds = ppc_position_dialog_bounds(bounds, position, gworlds);
    let scratch = ppc_process_heap_alloc(process_memory_manager, memory, heap_cursor, 9, true);
    let Some(items_slot) = ppc_parameter_area_slot_addr(cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    if scratch == 0
        || ppc_write_rect(memory, scratch, bounds.0, bounds.1, bounds.2, bounds.3).is_none()
        || memory.write_u8(scratch + 8, 0).is_none()
    {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    let saved_items_slot = memory.read_u32_be(items_slot).unwrap_or(0);
    if memory.write_u32_be(items_slot, items_handle).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    let mut dialog_cpu = cpu.clone();
    dialog_cpu.gpr[3] = 0;
    dialog_cpu.gpr[4] = scratch;
    dialog_cpu.gpr[5] = scratch + 8;
    dialog_cpu.gpr[6] = 1;
    dialog_cpu.gpr[7] = 1;
    dialog_cpu.gpr[8] = u32::MAX;
    dialog_cpu.gpr[9] = 0;
    dialog_cpu.gpr[10] = alert_id as u16 as u32;
    let dialog = ppc_new_dialog(
        &dialog_cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        window_list,
        current_gdevice,
    );
    let _ = memory.write_u32_be(items_slot, saved_items_slot);
    let dialog = if dialog != 0
        && !ppc_initialize_dialog_items(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            param_text,
            dialog,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        ) {
        0
    } else {
        dialog
    };
    if dialog != 0 {
        let first_stage = stages & 0x000f;
        let default_item = if first_stage & 0x0008 == 0 { 1 } else { 2 };
        let _ = memory.write_u16_be(dialog + PPC_DIALOG_RESOURCE_ID_OFFSET, alert_id as u16);
        let _ = memory.write_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET, default_item);
        let _ = memory.write_u16_be(dialog + PPC_DIALOG_ALERT_HIT_HLE_OFFSET, 0);
        *last_resource_error = PPC_NO_ERR;
    }
    dialog
}

#[allow(clippy::too_many_arguments)]
fn ppc_get_new_dialog(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    current_gdevice: u32,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
    param_text: &[Vec<u8>; 4],
) -> u32 {
    let dialog_id = cpu.gpr[3] as u16 as i16;
    let Some(dlog_index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"DLOG"),
        dialog_id,
        false,
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let Some(template) = ppc_parse_dialog_template(&vfs_resources[dlog_index].data) else {
        *last_resource_error = PPC_PARAM_ERR;
        return 0;
    };
    let Some(ditl_index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"DITL"),
        template.items_id,
        false,
    ) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let ditl_bytes = vfs_resources[ditl_index].data.clone();
    if ppc_parse_dialog_items(&ditl_bytes).is_none() {
        *last_resource_error = PPC_PARAM_ERR;
        return 0;
    }
    // GetNewDialog copies the DITL into an application-owned handle. Item
    // handles are then installed in that copy, so GetDialogItem and guest
    // mutations observe the same live list rather than the resource bytes.
    let items_handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        &ditl_bytes,
    );
    if items_handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    let bounds = ppc_position_dialog_bounds(template.bounds, template.position, gworlds);
    let scratch_size = 8u32.saturating_add(1 + template.title.len().min(255) as u32);
    let scratch = ppc_process_heap_alloc(
        process_memory_manager,
        memory,
        heap_cursor,
        scratch_size,
        true,
    );
    if scratch == 0
        || ppc_write_rect(memory, scratch, bounds.0, bounds.1, bounds.2, bounds.3).is_none()
        || !ppc_write_pstring_bytes(memory, scratch + 8, &template.title)
    {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    let Some(items_slot) = ppc_parameter_area_slot_addr(cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    let saved_items_slot = memory.read_u32_be(items_slot).unwrap_or(0);
    if memory.write_u32_be(items_slot, items_handle).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    let mut new_cpu = cpu.clone();
    new_cpu.gpr[3] = cpu.gpr[4];
    new_cpu.gpr[4] = scratch;
    new_cpu.gpr[5] = scratch + 8;
    new_cpu.gpr[6] = u32::from(template.visible);
    new_cpu.gpr[7] = template.proc_id as u16 as u32;
    new_cpu.gpr[8] = cpu.gpr[5];
    new_cpu.gpr[9] = u32::from(template.go_away);
    new_cpu.gpr[10] = template.ref_con;
    let dialog = ppc_new_dialog(
        &new_cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        window_list,
        current_gdevice,
    );
    let _ = memory.write_u32_be(items_slot, saved_items_slot);
    let dialog = if dialog != 0
        && !ppc_initialize_dialog_items(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            param_text,
            dialog,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        ) {
        0
    } else {
        dialog
    };
    if dialog != 0 {
        let _ = memory.write_u16_be(dialog + PPC_DIALOG_RESOURCE_ID_OFFSET, dialog_id as u16);
        *last_resource_error = PPC_NO_ERR;
    }
    dialog
}

#[allow(clippy::too_many_arguments)]
fn ppc_new_dialog(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    current_gdevice: u32,
) -> u32 {
    let requested_storage = cpu.gpr[3];
    let bounds_ptr = cpu.gpr[4];
    let title_ptr = cpu.gpr[5];
    let visible = cpu.gpr[6] != 0;
    let proc_id = cpu.gpr[7];
    let behind = cpu.gpr[8];
    let go_away = cpu.gpr[9] != 0;
    let ref_con = cpu.gpr[10];
    let items = ppc_parameter_area_slot_addr(cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
        .and_then(|addr| memory.read_u32_be(addr))
        .unwrap_or(0);
    if bounds_ptr == 0 {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }

    let storage = if requested_storage == 0 {
        ppc_process_heap_alloc(
            process_memory_manager,
            memory,
            heap_cursor,
            PPC_DIALOG_RECORD_SIZE,
            true,
        )
    } else if ppc_memory_can_write_bytes(memory, requested_storage, PPC_DIALOG_RECORD_SIZE) {
        let _ = memory.write_bytes(requested_storage, &vec![0; PPC_DIALOG_RECORD_SIZE as usize]);
        requested_storage
    } else {
        0
    };
    if storage == 0 {
        *last_mem_error = if requested_storage == 0 {
            PPC_MEM_FULL_ERR
        } else {
            PPC_PARAM_ERR
        };
        return 0;
    }

    let mut window_cpu = cpu.clone();
    window_cpu.gpr[3] = storage;
    window_cpu.gpr[4] = bounds_ptr;
    window_cpu.gpr[5] = title_ptr;
    window_cpu.gpr[6] = u32::from(visible);
    window_cpu.gpr[7] = proc_id;
    window_cpu.gpr[8] = behind;
    window_cpu.gpr[9] = u32::from(go_away);
    window_cpu.gpr[10] = ref_con;
    let mut allocator = PpcProcessAllocatorView {
        memory_manager: process_memory_manager,
    };
    let dialog = ppc_new_cwindow(
        &window_cpu,
        Some(&mut allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        window_list,
        current_gdevice,
    );
    if dialog == 0 {
        return 0;
    }

    let title = ppc_read_pstring_bytes(memory, title_ptr).unwrap_or_default();
    let mut title_string = Vec::with_capacity(title.len().saturating_add(1));
    title_string.push(title.len().min(255) as u8);
    title_string.extend(title.into_iter().take(255));
    let title_handle = allocator.allocate_handle_with_bytes(
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        &title_string,
    );
    if title_handle == 0
        || memory
            .write_u16_be(dialog + PPC_CWINDOW_WINDOW_KIND_OFFSET, 2)
            .is_none()
        || memory.write_u32_be(dialog + 134, title_handle).is_none()
        || memory
            .write_u32_be(dialog + PPC_DIALOG_ITEMS_OFFSET, items)
            .is_none()
        || memory
            .write_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET, 0)
            .is_none()
        || memory
            .write_u16_be(dialog + PPC_DIALOG_EDIT_FIELD_OFFSET, u16::MAX)
            .is_none()
        || memory
            .write_u16_be(dialog + PPC_DIALOG_EDIT_OPEN_OFFSET, 0)
            .is_none()
        || memory
            .write_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET, 1)
            .is_none()
    {
        *last_mem_error = if title_handle == 0 {
            PPC_MEM_FULL_ERR
        } else {
            PPC_PARAM_ERR
        };
        return 0;
    }

    // Macintosh Toolbox Essentials (1992), pp. 6-115--6-118: NewDialog's
    // first eight parameters construct its window; the ninth installs the
    // caller-owned DITL handle in the DialogRecord, whose editing state starts
    // closed with no selected edit field and item 1 as the default button.
    *last_mem_error = PPC_NO_ERR;
    dialog
}

#[allow(clippy::too_many_arguments)]
fn ppc_initialize_dialog_items(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    param_text: &[Vec<u8>; 4],
    dialog: u32,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> bool {
    let Some(items_handle) = memory
        .read_u32_be(dialog.wrapping_add(PPC_DIALOG_ITEMS_OFFSET))
        .filter(|handle| *handle != 0)
    else {
        return true;
    };
    let Some(ditl_bytes) = ppc_handle_bytes(memory, handles, items_handle) else {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    };
    let Some(items) = ppc_parse_dialog_items(&ditl_bytes) else {
        *last_resource_error = PPC_PARAM_ERR;
        return false;
    };
    let Some(items_ptr) = memory.read_u32_be(items_handle).filter(|ptr| *ptr != 0) else {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    };

    // Macintosh Toolbox Essentials (1992), pp. 6-26--6-42 and 6-120--6-121:
    // Dialog Manager copies the DITL, replaces each item's placeholder with
    // its live handle, and uses Control Manager records owned by the dialog
    // for buttons, checkboxes, radio buttons, and resource-defined controls.
    let mut first_edit = None;
    for (item_index, item) in items.into_iter().enumerate() {
        let base_type = item.item_type & !PPC_DIALOG_ITEM_DISABLED;
        let mut missing_resource = false;
        let item_handle = match base_type {
            PPC_DIALOG_ITEM_BUTTON | PPC_DIALOG_ITEM_CHECKBOX | PPC_DIALOG_ITEM_RADIO => {
                let proc_id = match base_type {
                    PPC_DIALOG_ITEM_CHECKBOX => 1,
                    PPC_DIALOG_ITEM_RADIO => 2,
                    _ => 0,
                };
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                ppc_new_control_record_values(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    dialog,
                    item.rect,
                    &item.payload,
                    true,
                    0,
                    0,
                    1,
                    proc_id,
                    0,
                )
            }
            PPC_DIALOG_ITEM_RESOURCE_CONTROL => {
                let resource_id = item
                    .payload
                    .get(..2)
                    .and_then(|bytes| bytes.try_into().ok())
                    .map(i16::from_be_bytes)
                    .unwrap_or(0);
                if let Some(index) = ppc_vfs_resource_index(
                    vfs_resources,
                    current_resource_refnum,
                    u32::from_be_bytes(*b"CNTL"),
                    resource_id,
                    false,
                ) {
                    let bytes = &vfs_resources[index].data;
                    if bytes.len() < 23 {
                        *last_resource_error = PPC_PARAM_ERR;
                        return false;
                    }
                    let title_len = usize::from(bytes[22]).min(bytes.len().saturating_sub(23));
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    ppc_new_control_record_values(
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        controls,
                        dialog,
                        item.rect,
                        &bytes[23..23 + title_len],
                        bytes[10] != 0,
                        i16::from_be_bytes([bytes[8], bytes[9]]),
                        i16::from_be_bytes([bytes[14], bytes[15]]),
                        i16::from_be_bytes([bytes[12], bytes[13]]),
                        i16::from_be_bytes([bytes[16], bytes[17]]),
                        u32::from_be_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]),
                    )
                } else {
                    missing_resource = true;
                    *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                    0
                }
            }
            PPC_DIALOG_ITEM_STATIC_TEXT => {
                // Macintosh Toolbox Essentials (1992), pp. 6-129--6-130:
                // ParamText replaces ^0..^3 in static-text items of every
                // subsequently created dialog or alert.
                let text = ppc_apply_param_text(&item.payload, param_text);
                ppc_process_alloc_handle_with_bytes(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    &text,
                )
            }
            PPC_DIALOG_ITEM_EDIT_TEXT => ppc_process_alloc_handle_with_bytes(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                &item.payload,
            ),
            PPC_DIALOG_ITEM_ICON | PPC_DIALOG_ITEM_PICTURE => {
                let resource_id = item
                    .payload
                    .get(..2)
                    .and_then(|bytes| bytes.try_into().ok())
                    .map(i16::from_be_bytes)
                    .unwrap_or(0);
                let resource_type = if base_type == PPC_DIALOG_ITEM_ICON {
                    u32::from_be_bytes(*b"ICON")
                } else {
                    u32::from_be_bytes(*b"PICT")
                };
                if let Some(index) = ppc_vfs_resource_index(
                    vfs_resources,
                    current_resource_refnum,
                    resource_type,
                    resource_id,
                    false,
                ) {
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
                } else {
                    missing_resource = true;
                    *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                    0
                }
            }
            _ => item.handle,
        };
        if item_handle == 0
            && !missing_resource
            && matches!(
                base_type,
                PPC_DIALOG_ITEM_BUTTON
                    | PPC_DIALOG_ITEM_CHECKBOX
                    | PPC_DIALOG_ITEM_RADIO
                    | PPC_DIALOG_ITEM_RESOURCE_CONTROL
                    | PPC_DIALOG_ITEM_STATIC_TEXT
                    | PPC_DIALOG_ITEM_EDIT_TEXT
                    | PPC_DIALOG_ITEM_ICON
                    | PPC_DIALOG_ITEM_PICTURE
            )
        {
            if *last_mem_error == PPC_NO_ERR && *last_resource_error == PPC_NO_ERR {
                *last_mem_error = PPC_MEM_FULL_ERR;
            }
            return false;
        }
        if base_type == PPC_DIALOG_ITEM_RESOURCE_CONTROL {
            ppc_initialize_popup_control(
                memory,
                controls,
                vfs_resources,
                current_resource_refnum,
                item_handle,
            );
        }
        if memory
            .write_u32_be(items_ptr.wrapping_add(item.item_offset as u32), item_handle)
            .is_none()
        {
            *last_mem_error = PPC_PARAM_ERR;
            return false;
        }
        if base_type == PPC_DIALOG_ITEM_EDIT_TEXT && first_edit.is_none() {
            first_edit = Some((item_index, item_handle, item.rect));
        }
    }
    if let Some((item_index, item_handle, rect)) = first_edit {
        let mut allocator = PpcProcessAllocatorView {
            memory_manager: process_memory_manager,
        };
        let te_handle = ppc_te_create_for_dialog_item(
            Some(&mut allocator),
            None,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            dialog,
            item_handle,
            rect,
            0,
            PPC_QD_TEXT_MODE_SRC_OR,
            PPC_QD_TEXT_SIZE_SYSTEM,
            PpcRgbColor {
                red: 0,
                green: 0,
                blue: 0,
            },
        );
        if te_handle == 0
            || memory
                .write_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET, te_handle)
                .is_none()
            || memory
                .write_u16_be(
                    dialog + PPC_DIALOG_EDIT_FIELD_OFFSET,
                    item_index.min(i16::MAX as usize) as u16,
                )
                .is_none()
            || memory
                .write_u16_be(dialog + PPC_DIALOG_EDIT_OPEN_OFFSET, 1)
                .is_none()
        {
            *last_mem_error = PPC_MEM_FULL_ERR;
            return false;
        }
    }
    *last_resource_error = PPC_NO_ERR;
    true
}

#[allow(clippy::too_many_arguments)]
fn ppc_te_create_for_dialog_item(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    mut legacy_handle_states: Option<&mut Vec<PpcHandleStateRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    dialog: u32,
    item_text_handle: u32,
    rect: (i16, i16, i16, i16),
    tick_count: u32,
    text_mode: i16,
    text_size: i16,
    fore_color: PpcRgbColor,
) -> u32 {
    let scratch = ppc_allocator_view_reserve_bytes(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        16,
        true,
    );
    if scratch == 0
        || ppc_write_rect(memory, scratch, rect.0, rect.1, rect.2, rect.3).is_none()
        || ppc_write_rect(memory, scratch + 8, rect.0, rect.1, rect.2, rect.3).is_none()
    {
        return 0;
    }
    let te_handle = ppc_te_initialize_record(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        scratch,
        scratch + 8,
        dialog,
        tick_count,
        text_mode,
        text_size,
        fore_color,
        false,
    );
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return 0;
    };
    let temporary_text_handle = memory
        .read_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET)
        .unwrap_or(0);
    let length = handles
        .iter()
        .find(|record| record.handle == item_text_handle)
        .map(|record| record.size.min(i16::MAX as u32) as u16)
        .unwrap_or(0);
    if memory
        .write_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET, item_text_handle)
        .is_none()
        || memory
            .write_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET, length)
            .is_none()
    {
        return 0;
    }
    ppc_te_forget_handle(
        allocator.as_deref_mut(),
        legacy_handle_states.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        temporary_text_handle,
    );
    if ppc_te_recalculate_layout(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        te_handle,
    ) != PPC_NO_ERR
    {
        return 0;
    }
    // Macintosh Toolbox Essentials (1992), pp. 6-135--6-137: when a
    // dialog opens its first editText item, Dialog Manager activates the
    // shared TERec and selects the item's initial text. The first typed
    // character therefore replaces resource placeholder/default text.
    if let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) {
        let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, 0);
        let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, length);
        let _ = memory.write_u16_be(te_ptr + PPC_TE_ACTIVE_OFFSET, 1);
        let _ = memory.write_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET, tick_count);
    }
    te_handle
}

#[allow(clippy::too_many_arguments)]
fn ppc_select_dialog_item_text(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    mut legacy_handle_states: Option<&mut Vec<PpcHandleStateRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    dialog: u32,
    item_number: usize,
    selection_start: u16,
    selection_end: u16,
    tick_count: u32,
    text_mode: i16,
    text_size: i16,
    fore_color: PpcRgbColor,
) {
    let Some(item_index) = item_number.checked_sub(1) else {
        return;
    };
    let Some(item) = ppc_dialog_items_for_dialog(memory, handles, dialog)
        .and_then(|items| items.get(item_index).cloned())
        .filter(|item| item.item_type & !PPC_DIALOG_ITEM_DISABLED == PPC_DIALOG_ITEM_EDIT_TEXT)
    else {
        return;
    };
    let current_field = memory
        .read_u16_be(dialog + PPC_DIALOG_EDIT_FIELD_OFFSET)
        .unwrap_or(u16::MAX) as usize;
    let mut te_handle = memory
        .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
        .unwrap_or(0);
    if current_field != item_index || ppc_te_record_ptr(memory, te_handle).is_none() {
        if let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) {
            // A DialogRecord's TERec borrows the live DITL text handle; detach
            // it before disposing the old edit record when focus changes.
            let _ = memory.write_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET, 0);
            ppc_te_dispose(
                allocator.as_deref_mut(),
                legacy_handle_states.as_deref_mut(),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                te_handle,
            );
        }
        te_handle = ppc_te_create_for_dialog_item(
            allocator,
            legacy_handle_states,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            dialog,
            item.handle,
            item.rect,
            tick_count,
            text_mode,
            text_size,
            fore_color,
        );
        if te_handle == 0 {
            return;
        }
        let _ = memory.write_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET, te_handle);
        let _ = memory.write_u16_be(
            dialog + PPC_DIALOG_EDIT_FIELD_OFFSET,
            item_index.min(i16::MAX as usize) as u16,
        );
        let _ = memory.write_u16_be(dialog + PPC_DIALOG_EDIT_OPEN_OFFSET, 1);
    }
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return;
    };
    let length = memory
        .read_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET)
        .unwrap_or(0);
    let start = selection_start.min(length);
    let end = selection_end.min(length);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, start);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, end);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_ACTIVE_OFFSET, 1);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET, tick_count);
}

fn ppc_get_dialog_item(cpu: &mut PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) {
    let dialog = cpu.gpr[3];
    let item_number = cpu.gpr[4] as u16 as usize;
    let item_type_ptr = cpu.gpr[5];
    let item_handle_ptr = cpu.gpr[6];
    let item_rect_ptr = cpu.gpr[7];
    if !ppc_optional_output_can_write(memory, item_type_ptr, 2)
        || !ppc_optional_output_can_write(memory, item_handle_ptr, 4)
        || !ppc_optional_output_can_write(memory, item_rect_ptr, 8)
    {
        return;
    }
    let item = memory
        .read_u32_be(dialog.wrapping_add(PPC_DIALOG_ITEMS_OFFSET))
        .and_then(|items_handle| ppc_handle_bytes(memory, handles, items_handle))
        .and_then(|bytes| ppc_parse_dialog_items(&bytes))
        .and_then(|items| {
            item_number
                .checked_sub(1)
                .and_then(|index| items.get(index).cloned())
        });
    let Some(item) = item else {
        if item_type_ptr != 0 {
            let _ = memory.write_u16_be(item_type_ptr, 0);
        }
        if item_handle_ptr != 0 {
            let _ = memory.write_u32_be(item_handle_ptr, 0);
        }
        if item_rect_ptr != 0 {
            let _ = ppc_write_rect(memory, item_rect_ptr, 0, 0, 0, 0);
        }
        return;
    };
    if item_type_ptr != 0 {
        let _ = memory.write_u16_be(item_type_ptr, u16::from(item.item_type));
    }
    if item_handle_ptr != 0 {
        let _ = memory.write_u32_be(item_handle_ptr, item.handle);
    }
    if item_rect_ptr != 0 {
        let _ = ppc_write_rect(
            memory,
            item_rect_ptr,
            item.rect.0,
            item.rect.1,
            item.rect.2,
            item.rect.3,
        );
    }
}

fn ppc_set_dialog_item(cpu: &PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) {
    let dialog = cpu.gpr[3];
    let item_number = cpu.gpr[4] as u16 as usize;
    let item_type = cpu.gpr[5] as u16;
    let item_handle = cpu.gpr[6];
    let rect_ptr = cpu.gpr[7];
    let Some(items_handle) = memory.read_u32_be(dialog.wrapping_add(PPC_DIALOG_ITEMS_OFFSET))
    else {
        return;
    };
    let Some(items_ptr) = memory.read_u32_be(items_handle).filter(|ptr| *ptr != 0) else {
        return;
    };
    let Some(item) = ppc_handle_bytes(memory, handles, items_handle)
        .and_then(|bytes| ppc_parse_dialog_items(&bytes))
        .and_then(|items| {
            item_number
                .checked_sub(1)
                .and_then(|index| items.get(index).cloned())
        })
    else {
        return;
    };
    let item_addr = items_ptr.wrapping_add(item.item_offset as u32);
    let Some(rect) = ppc_read_rect(memory, rect_ptr) else {
        return;
    };
    // Macintosh Toolbox Essentials (1992), pp. 6-120--6-123: mutate the
    // live DITL item's handle, Rect, and type in place without drawing it.
    let _ = memory.write_u32_be(item_addr, item_handle);
    let _ = ppc_write_rect(memory, item_addr + 4, rect.0, rect.1, rect.2, rect.3);
    let _ = memory.write_u8(item_addr + 12, item_type as u8);
}

pub(super) fn ppc_dialog_items_for_dialog(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    dialog: u32,
) -> Option<Vec<PpcDialogItemView>> {
    let items_handle = memory.read_u32_be(dialog.checked_add(PPC_DIALOG_ITEMS_OFFSET)?)?;
    let bytes = ppc_handle_bytes(memory, handles, items_handle)?;
    ppc_parse_dialog_items(&bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PpcDialogEvent {
    what: u16,
    message: u32,
    when: u32,
    where_v: i16,
    where_h: i16,
    modifiers: u16,
}

fn ppc_read_dialog_event(memory: &mut PpcSectionMem, event_ptr: u32) -> Option<PpcDialogEvent> {
    Some(PpcDialogEvent {
        what: memory.read_u16_be(event_ptr)?,
        message: memory.read_u32_be(event_ptr.checked_add(2)?)?,
        when: memory.read_u32_be(event_ptr.checked_add(6)?)?,
        where_v: memory.read_u16_be(event_ptr.checked_add(10)?)? as i16,
        where_h: memory.read_u16_be(event_ptr.checked_add(12)?)? as i16,
        modifiers: memory.read_u16_be(event_ptr.checked_add(14)?)?,
    })
}

fn ppc_dialog_for_event(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    what: u16,
    message: u32,
) -> Option<u32> {
    // Inside Macintosh Volume I (1985), pp. I-416--I-417: update and
    // activate events name their window in `message`; other dialog events
    // are routed to the frontmost visible dialog.
    if matches!(what, 6 | 8)
        && memory.read_u16_be(message.checked_add(PPC_CWINDOW_WINDOW_KIND_OFFSET)?) == Some(2)
    {
        return Some(message);
    }
    gworlds.iter().rev().find_map(|record| {
        (memory.read_u16_be(record.port.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET)) == Some(2)
            && ppc_window_is_visible(memory, record.port))
        .then_some(record.port)
    })
}

pub(super) fn ppc_dialog_global_bounds(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    dialog: u32,
) -> Option<(i16, i16, i16, i16)> {
    let record = gworlds.iter().find(|record| record.port == dialog)?;
    let (pixel_top, pixel_left, _, _) = ppc_read_rect(memory, record.pixmap.checked_add(6)?)?;
    let top = pixel_top.saturating_neg();
    let left = pixel_left.saturating_neg();
    Some((
        top,
        left,
        top.saturating_add(ppc_u32_to_i16_saturating(record.height)),
        left.saturating_add(ppc_u32_to_i16_saturating(record.width)),
    ))
}

fn ppc_dialog_rect_to_global(
    bounds: (i16, i16, i16, i16),
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    (
        bounds.0.saturating_add(rect.0),
        bounds.1.saturating_add(rect.1),
        bounds.0.saturating_add(rect.2),
        bounds.1.saturating_add(rect.3),
    )
}

fn ppc_dialog_draw_callbacks(
    memory: &mut PpcSectionMem,
    items: &[PpcDialogItemView],
    bounds: (i16, i16, i16, i16),
    default_rtoc: u32,
) -> Vec<(PpcCallbackTarget, u32)> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            if item.item_type & !PPC_DIALOG_ITEM_DISABLED != PPC_DIALOG_ITEM_USER_ITEM
                || item.handle == 0
            {
                return None;
            }
            let rect = ppc_dialog_rect_to_global(bounds, item.rect);
            if rect.0 >= bounds.2 || rect.2 <= bounds.0 || rect.1 >= bounds.3 || rect.3 <= bounds.1
            {
                return None;
            }
            let target = ppc_resolve_callback_target(memory, item.handle, default_rtoc, None)?;
            memory.read_u32_be(target.entry)?;
            Some((target, u32::try_from(index + 1).unwrap_or(u32::MAX)))
        })
        .collect()
}

fn ppc_next_dialog_callback(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    dialog_callback_stack: &mut Vec<PpcDialogCallbackState>,
) -> PpcImportAction {
    loop {
        let Some(state) = dialog_callback_stack.last_mut() else {
            return PpcImportAction::ReturnPreserve;
        };
        if state.next_callback >= state.callbacks.len() {
            let state = dialog_callback_stack.pop().unwrap();
            cpu.lr = state.final_pc;
            cpu.gpr[2] = state.restore_rtoc;
            return match state.completion {
                PpcDialogCallbackCompletion::ReturnPreserve => PpcImportAction::ReturnPreserve,
                PpcDialogCallbackCompletion::Return(value) => PpcImportAction::Return(value),
                PpcDialogCallbackCompletion::Yield => PpcImportAction::Yield(u64::MAX),
            };
        }
        let (target, item_number) = state.callbacks[state.next_callback];
        let dialog = state.dialog;
        let restore_rtoc = state.restore_rtoc;
        state.next_callback += 1;
        if install_powerpc_call_arguments(cpu, memory, &[dialog, item_number]).is_none() {
            continue;
        }
        return GuestCallEffect::call_guest(
            GuestCallRequest::new(GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: target.entry,
                rtoc: target.rtoc,
            }),
            GuestCallContinuation::to_powerpc(
                PPC_GUEST_CALL_RETURN_PC,
                cpu.pc,
                restore_rtoc,
                PpcNativeReturnGpr3::Preserve,
            ),
        )
        .into_ppc_import_action()
        .expect("validated dialog callback must be native PowerPC");
    }
}

fn ppc_resume_dialog_callbacks(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    dialog_callback_stack: &mut Vec<PpcDialogCallbackState>,
) -> Option<PpcImportAction> {
    (cpu.lr == cpu.pc
        && dialog_callback_stack
            .last()
            .is_some_and(|state| state.import_pc == cpu.pc))
    .then(|| ppc_next_dialog_callback(cpu, memory, dialog_callback_stack))
}

fn ppc_begin_dialog_callbacks(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    dialog_callback_stack: &mut Vec<PpcDialogCallbackState>,
    dialog: u32,
    items: &[PpcDialogItemView],
    bounds: (i16, i16, i16, i16),
    completion: PpcDialogCallbackCompletion,
) -> PpcImportAction {
    let callbacks = ppc_dialog_draw_callbacks(memory, items, bounds, cpu.gpr[2]);
    if callbacks.is_empty() {
        return match completion {
            PpcDialogCallbackCompletion::ReturnPreserve => PpcImportAction::ReturnPreserve,
            PpcDialogCallbackCompletion::Return(value) => PpcImportAction::Return(value),
            PpcDialogCallbackCompletion::Yield => PpcImportAction::Yield(u64::MAX),
        };
    }
    dialog_callback_stack.push(PpcDialogCallbackState {
        import_pc: cpu.pc,
        dialog,
        callbacks,
        next_callback: 0,
        final_pc: cpu.lr,
        restore_rtoc: cpu.gpr[2],
        completion,
    });
    ppc_next_dialog_callback(cpu, memory, dialog_callback_stack)
}

pub(super) fn ppc_fill_front_rect(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    rect: (i16, i16, i16, i16),
    color: PpcRgbColor,
) -> bool {
    let top = i32::from(rect.0).max(0).min(front.height as i32);
    let left = i32::from(rect.1).max(0).min(front.width as i32);
    let bottom = i32::from(rect.2).max(0).min(front.height as i32);
    let right = i32::from(rect.3).max(0).min(front.width as i32);
    if top >= bottom || left >= right {
        return false;
    }
    let pixel = match front.depth {
        depth @ (1 | 2 | 4 | 8) => {
            let fallback = TrapDispatcher::standard_mac_indexed_clut(depth as u16)
                .map(|(clut, _)| clut)
                .unwrap_or_else(TrapDispatcher::standard_mac_8bpp_clut);
            let clut = if front.base_addr == PPC_MAIN_SCREEN_BASE {
                ppc_read_ctable_clut(memory, PPC_MAIN_CTABLE_HANDLE, &fallback).unwrap_or(fallback)
            } else {
                fallback
            };
            u16::from(ppc_rgb_color_to_index_in_clut(
                color,
                &clut,
                ppc_indexed_depth_entry_count(depth).unwrap_or(1),
            ))
        }
        16 => ppc_rgb_color_to_rgb555(color),
        _ => return false,
    };
    let mut wrote = false;
    for y in top..bottom {
        for x in left..right {
            wrote |= ppc_quickdraw_write_raw_pixel(memory, front, (x, y), pixel);
        }
    }
    wrote
}

pub(super) fn ppc_frame_front_rect(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    rect: (i16, i16, i16, i16),
    color: PpcRgbColor,
    thickness: i16,
) -> bool {
    let mut wrote = false;
    for inset in 0..thickness.max(1) {
        let inset_rect = (
            rect.0.saturating_add(inset),
            rect.1.saturating_add(inset),
            rect.2.saturating_sub(inset),
            rect.3.saturating_sub(inset),
        );
        wrote |= ppc_fill_front_rect(
            memory,
            front,
            (
                inset_rect.0,
                inset_rect.1,
                inset_rect.0.saturating_add(1),
                inset_rect.3,
            ),
            color,
        );
        wrote |= ppc_fill_front_rect(
            memory,
            front,
            (
                inset_rect.2.saturating_sub(1),
                inset_rect.1,
                inset_rect.2,
                inset_rect.3,
            ),
            color,
        );
        wrote |= ppc_fill_front_rect(
            memory,
            front,
            (
                inset_rect.0,
                inset_rect.1,
                inset_rect.2,
                inset_rect.1.saturating_add(1),
            ),
            color,
        );
        wrote |= ppc_fill_front_rect(
            memory,
            front,
            (
                inset_rect.0,
                inset_rect.3.saturating_sub(1),
                inset_rect.2,
                inset_rect.3,
            ),
            color,
        );
    }
    wrote
}

fn ppc_frame_front_round_rect(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    rect: (i16, i16, i16, i16),
    oval: i16,
    thickness: i16,
    color: PpcRgbColor,
) -> bool {
    let slot = memory.presentation();
    let detail = if matches!(front.depth, 8 | 16) {
        let fallback = TrapDispatcher::standard_mac_8bpp_clut();
        let clut = if front.base_addr == PPC_MAIN_SCREEN_BASE {
            ppc_read_ctable_clut(memory, PPC_MAIN_CTABLE_HANDLE, &fallback).unwrap_or(fallback)
        } else {
            fallback
        };
        let foreground = if front.depth == 16 {
            u32::from(ppc_rgb_color_to_rgb555(color))
        } else {
            u32::from(ppc_rgb_color_to_index_in_clut(color, &clut, 256))
        };
        slot.rounded_control_corners(
            (rect.0.into(), rect.1.into(), rect.2.into(), rect.3.into()),
            oval.into(),
            thickness.into(),
            front.depth as u16,
            None,
            foreground,
            |x, y, lane| {
                if x < 0 || y < 0 || x >= front.width as i32 || y >= front.height as i32 {
                    return None;
                }
                let address = front.base_addr
                    + y as u32 * front.row_bytes
                    + x as u32 * (front.depth / 8)
                    + lane;
                Some((address, memory.read_u8(address)?))
            },
        )
    } else {
        None
    };
    let outer = Rect {
        top: rect.0,
        left: rect.1,
        bottom: rect.2,
        right: rect.3,
    };
    let inner = Rect {
        top: rect.0.saturating_add(thickness),
        left: rect.1.saturating_add(thickness),
        bottom: rect.2.saturating_sub(thickness),
        right: rect.3.saturating_sub(thickness),
    };
    let outer_spans = TrapDispatcher::compute_rrect_spans(&outer, oval, oval);
    let inner_spans = TrapDispatcher::compute_rrect_spans(
        &inner,
        oval.saturating_sub(thickness.saturating_mul(2)),
        oval.saturating_sub(thickness.saturating_mul(2)),
    );
    let mut wrote = false;
    for y in rect.0..rect.2 {
        let Some(&(outer_left, outer_right)) = outer_spans.get((y - rect.0) as usize) else {
            continue;
        };
        let inner_span = if y >= inner.top && y < inner.bottom {
            inner_spans.get((y - inner.top) as usize).copied()
        } else {
            None
        };
        let (inner_left, inner_right) = inner_span.unwrap_or((outer_right, outer_left));
        wrote |= ppc_fill_front_rect(
            memory,
            front,
            (
                y,
                outer_left,
                y.saturating_add(1),
                inner_left.min(outer_right),
            ),
            color,
        );
        wrote |= ppc_fill_front_rect(
            memory,
            front,
            (
                y,
                inner_right.max(outer_left),
                y.saturating_add(1),
                outer_right,
            ),
            color,
        );
    }
    slot.finish_rounded_control(detail, |address| memory.read_u8(address).unwrap_or(0));
    wrote
}

pub(super) fn ppc_dialog_text_lines(bytes: &[u8], max_width: i16) -> Vec<Vec<u8>> {
    crate::quickdraw::text::wrap_classic_text(bytes, max_width, |_, byte| {
        ppc_text_byte_advance_for_font(byte, PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM)
    })
    .into_iter()
    .map(|line| bytes[line.start..line.visible_end].to_vec())
    .collect()
}

pub(super) fn ppc_draw_dialog_text(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    rect: (i16, i16, i16, i16),
    bytes: &[u8],
    color: PpcRgbColor,
) {
    let max_width = rect.3.saturating_sub(rect.1).max(1);
    let lines = ppc_dialog_text_lines(bytes, max_width);
    for (index, line) in lines.iter().enumerate() {
        let baseline = rect
            .0
            .saturating_add(12)
            .saturating_add(i16::try_from(index).unwrap_or(i16::MAX).saturating_mul(16));
        if baseline >= rect.2 {
            break;
        }
        let _ = ppc_draw_text_bytes(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            (rect.1, baseline),
            PPC_QD_TEXT_FONT_DEFAULT,
            PPC_QD_TEXT_SIZE_SYSTEM,
            PPC_QD_TEXT_MODE_SRC_OR,
            color,
            None,
            line,
        );
    }
}

fn ppc_dialog_item_title(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    item: &PpcDialogItemView,
) -> Vec<u8> {
    let base_type = item.item_type & !PPC_DIALOG_ITEM_DISABLED;
    if matches!(
        base_type,
        PPC_DIALOG_ITEM_BUTTON | PPC_DIALOG_ITEM_CHECKBOX | PPC_DIALOG_ITEM_RADIO
    ) {
        return memory
            .read_u32_be(item.handle)
            .filter(|ptr| *ptr != 0)
            .and_then(|ptr| ppc_read_pstring_bytes(memory, ptr + PPC_CONTROL_TITLE_OFFSET))
            .unwrap_or_default();
    }
    ppc_handle_bytes(memory, handles, item.handle).unwrap_or_else(|| item.payload.clone())
}

pub(super) fn ppc_draw_dialog(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    dialog: u32,
) -> bool {
    let Some(front) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) else {
        return false;
    };
    let Some(bounds) = ppc_dialog_global_bounds(memory, gworlds, dialog) else {
        return false;
    };
    let Some(items) = ppc_dialog_items_for_dialog(memory, handles, dialog) else {
        return false;
    };
    // Macintosh Toolbox Essentials (1992), p. 6-142: DrawDialog redraws
    // items, controls, text, and user-item callbacks. It does not erase the
    // dialog surface: applications may have already drawn custom contents
    // outside those items. Window creation supplies the initial background.
    let palette = ppc_ui_theme(gworlds).provider().palette();
    let default_item = memory
        .read_u16_be(dialog.wrapping_add(PPC_DIALOG_DEFAULT_ITEM_OFFSET))
        .unwrap_or(1) as usize;
    for (index, item) in items.iter().enumerate() {
        let rect = ppc_dialog_rect_to_global(bounds, item.rect);
        match item.item_type & !PPC_DIALOG_ITEM_DISABLED {
            PPC_DIALOG_ITEM_BUTTON | PPC_DIALOG_ITEM_CHECKBOX | PPC_DIALOG_ITEM_RADIO => {
                // Macintosh Toolbox Essentials (1992), pp. 5-4--5-6 and
                // 6-26--6-42: DITL buttons, checkboxes, and radio buttons are
                // live Control Manager controls. Draw the materialized record
                // so its CDEF, value, visibility, and title determine the
                // result instead of treating every item as a push button.
                let _ = ppc_draw_control_inner(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    item.handle,
                    true,
                );
                if (item.item_type & !PPC_DIALOG_ITEM_DISABLED) == PPC_DIALOG_ITEM_BUTTON
                    && index + 1 == default_item
                    && ppc_ui_theme(gworlds) == UiThemeId::ClassicSystem7
                {
                    let outer = (
                        rect.0.saturating_sub(4),
                        rect.1.saturating_sub(4),
                        rect.2.saturating_add(4),
                        rect.3.saturating_add(4),
                    );
                    let oval = (outer.2.saturating_sub(outer.0) / 2 - 4).max(4);
                    let _ = ppc_frame_front_round_rect(
                        memory,
                        front,
                        outer,
                        oval,
                        3,
                        ppc_theme_rgb(palette.frame_dark),
                    );
                }
            }
            PPC_DIALOG_ITEM_STATIC_TEXT | PPC_DIALOG_ITEM_EDIT_TEXT => {
                let text = ppc_dialog_item_title(memory, handles, item);
                if (item.item_type & !PPC_DIALOG_ITEM_DISABLED) == PPC_DIALOG_ITEM_EDIT_TEXT {
                    let outer = (
                        rect.0.saturating_sub(3),
                        rect.1.saturating_sub(3),
                        rect.2.saturating_add(3),
                        rect.3.saturating_add(3),
                    );
                    let _ = ppc_frame_front_rect(
                        memory,
                        front,
                        outer,
                        ppc_theme_rgb(palette.frame_dark),
                        1,
                    );
                }
                let selected = if (item.item_type & !PPC_DIALOG_ITEM_DISABLED)
                    == PPC_DIALOG_ITEM_EDIT_TEXT
                    && memory
                        .read_u16_be(dialog + PPC_DIALOG_EDIT_FIELD_OFFSET)
                        .is_some_and(|field| usize::from(field) == index)
                {
                    memory
                        .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
                        .and_then(|handle| ppc_te_record_ptr(memory, handle))
                        .is_some_and(|te_ptr| {
                            memory
                                .read_u16_be(te_ptr + PPC_TE_ACTIVE_OFFSET)
                                .unwrap_or(0)
                                != 0
                                && memory
                                    .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
                                    .unwrap_or(0)
                                    < memory
                                        .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
                                        .unwrap_or(0)
                        })
                } else {
                    false
                };
                if selected {
                    let interior = (
                        rect.0,
                        rect.1,
                        rect.0.saturating_add(16).min(rect.2),
                        rect.3,
                    );
                    if !ppc_draw_themed_selection(memory, gworlds, PPC_MAIN_GWORLD, interior) {
                        let _ = ppc_fill_front_rect(
                            memory,
                            front,
                            interior,
                            ppc_theme_rgb(palette.frame_dark),
                        );
                    }
                }
                let text_rect = if matches!(
                    item.item_type & !PPC_DIALOG_ITEM_DISABLED,
                    PPC_DIALOG_ITEM_STATIC_TEXT | PPC_DIALOG_ITEM_EDIT_TEXT
                ) {
                    (rect.0, rect.1.saturating_add(1), rect.2, rect.3)
                } else {
                    rect
                };
                ppc_draw_dialog_text(
                    memory,
                    gworlds,
                    text_rect,
                    &text,
                    if selected && ppc_ui_theme(gworlds) == UiThemeId::ClassicSystem7 {
                        ppc_theme_rgb(palette.window_background)
                    } else {
                        ppc_theme_rgb(palette.frame_dark)
                    },
                );
            }
            PPC_DIALOG_ITEM_PICTURE => {
                if let Some(bytes) = ppc_handle_bytes(memory, handles, item.handle) {
                    let _ = ppc_draw_pict_bytes_to_16bpp(
                        memory,
                        front,
                        &bytes,
                        rect,
                        screen_clut,
                        0,
                        false,
                    );
                }
            }
            PPC_DIALOG_ITEM_ICON => {
                let _ =
                    ppc_frame_front_rect(memory, front, rect, ppc_theme_rgb(palette.frame_dark), 1);
            }
            PPC_DIALOG_ITEM_RESOURCE_CONTROL => {
                let _ = ppc_draw_control_inner(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    item.handle,
                    true,
                );
            }
            _ => {}
        }
    }
    // Resource-control DITL entries can retain the resource handle after the
    // Control Manager has materialized the live control. Draw live pop-up
    // controls owned by the dialog as a final pass so SetControlValue calls
    // made before the dialog is positioned cannot leave them missing or at
    // stale local coordinates.
    for record in controls {
        if !(1008..=1023).contains(&(record.proc_id & 0x0fff)) {
            continue;
        }
        let Some(control) = ppc_control_ptr(memory, record.handle) else {
            continue;
        };
        if memory.read_u32_be(control.wrapping_add(PPC_CONTROL_OWNER_OFFSET)) != Some(dialog) {
            continue;
        }
        let _ = ppc_draw_control_inner(
            memory,
            handles,
            controls,
            gworlds,
            vfs_resources,
            current_resource_refnum,
            record.handle,
            true,
        );
    }
    let _ = memory.write_u8(dialog + PPC_CWINDOW_VISIBLE_OFFSET, 1);
    true
}

fn ppc_dialog_item_at_global_point(
    items: &[PpcDialogItemView],
    bounds: (i16, i16, i16, i16),
    where_v: i16,
    where_h: i16,
) -> Option<u16> {
    items.iter().enumerate().find_map(|(index, item)| {
        if item.item_type & PPC_DIALOG_ITEM_DISABLED != 0 {
            return None;
        }
        let rect = ppc_dialog_rect_to_global(bounds, item.rect);
        (where_v >= rect.0 && where_v < rect.2 && where_h >= rect.1 && where_h < rect.3)
            .then(|| u16::try_from(index + 1).unwrap_or(u16::MAX))
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_track_dialog_popup(
    memory: &mut PpcSectionMem,
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    dialog: u32,
    item_rect: (i16, i16, i16, i16),
    input: PpcInputSnapshot,
) -> bool {
    let Some((record, control)) = controls.iter().find_map(|record| {
        if !(1008..=1023).contains(&(record.proc_id & 0x0fff)) {
            return None;
        }
        let control = ppc_control_ptr(memory, record.handle)?;
        (memory.read_u32_be(control + PPC_CONTROL_OWNER_OFFSET) == Some(dialog)
            && ppc_read_rect(memory, control + PPC_CONTROL_RECT_OFFSET) == Some(item_rect))
        .then_some((record, control))
    }) else {
        return false;
    };
    let Some(resource_index) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"MENU"),
        record.popup_menu_id,
        false,
    ) else {
        return false;
    };
    let Some((_, _, menu_items)) = ppc_decode_menu_items(&vfs_resources[resource_index].data)
    else {
        return false;
    };
    let selected = memory
        .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
        .unwrap_or(1)
        .max(1);
    let Some(dialog_bounds) = ppc_dialog_global_bounds(memory, gworlds, dialog) else {
        return false;
    };
    let global_rect = ppc_dialog_rect_to_global(dialog_bounds, item_rect);
    if input.mouse_h < global_rect.1 || input.mouse_h >= global_rect.3 {
        return false;
    }
    // Macintosh Toolbox Essentials (1992), pp. 3-104--3-105: the current
    // item is aligned with the pop-up control while the mouse tracks the
    // vertically stacked menu. Use the live pointer position so scripted and
    // interactive drags can choose a different row before the mouse-up event.
    let menu_top = i32::from(global_rect.0)
        .saturating_sub(i32::from(selected.saturating_sub(1)).saturating_mul(16));
    let relative_v = i32::from(input.mouse_v).saturating_sub(menu_top);
    if relative_v < 0 {
        return false;
    }
    let item = usize::try_from(relative_v / 16).unwrap_or(usize::MAX) + 1;
    if item == 0 || item > menu_items.len() || !menu_items[item - 1].enabled {
        return false;
    }
    let item = u16::try_from(item).unwrap_or(u16::MAX);
    if item == selected {
        return false;
    }
    memory
        .write_u16_be(control + PPC_CONTROL_VALUE_OFFSET, item)
        .is_some()
}

fn ppc_dialog_cancel_item(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    items: &[PpcDialogItemView],
) -> Option<u16> {
    items.iter().enumerate().find_map(|(index, item)| {
        if item.item_type & PPC_DIALOG_ITEM_DISABLED != 0
            || item.item_type & !PPC_DIALOG_ITEM_DISABLED != PPC_DIALOG_ITEM_BUTTON
        {
            return None;
        }
        let title = ppc_dialog_item_title(memory, handles, item);
        title
            .eq_ignore_ascii_case(b"cancel")
            .then(|| u16::try_from(index + 1).unwrap_or(u16::MAX))
    })
}

fn ppc_modal_dialog(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    screen_clut: &[[u16; 3]; 256],
    fore_color: PpcRgbColor,
    fore_indices: &HashMap<u32, u8>,
    input: PpcInputSnapshot,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    dialog_callback_stack: &mut Vec<PpcDialogCallbackState>,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
) -> PpcImportAction {
    if let Some(action) = ppc_resume_dialog_callbacks(cpu, memory, dialog_callback_stack) {
        return action;
    }
    let item_hit_ptr = cpu.gpr[4];
    if item_hit_ptr == 0 || !ppc_memory_can_write_bytes(memory, item_hit_ptr, 2) {
        return PpcImportAction::ReturnPreserve;
    }
    let dialog = if memory.read_u16_be(current_gworld.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
        == Some(2)
    {
        *current_gworld
    } else {
        gworlds
            .iter()
            .rev()
            .find(|record| {
                memory.read_u16_be(record.port.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
                    == Some(2)
                    && ppc_window_is_visible(memory, record.port)
            })
            .map(|record| record.port)
            .unwrap_or(0)
    };
    let Some(bounds) = ppc_dialog_global_bounds(memory, gworlds, dialog) else {
        return PpcImportAction::ReturnPreserve;
    };
    let Some(items) = ppc_dialog_items_for_dialog(memory, handles, dialog) else {
        return PpcImportAction::ReturnPreserve;
    };
    *current_gworld = dialog;
    *current_gdevice = ppc_gworld_device(gworlds, dialog).unwrap_or(*current_gdevice);
    let event = event_queue.pop_front();
    let mut handled_edit_event = false;
    let hit = match event.as_ref().map(|event| event.what) {
        Some(1) => event.as_ref().and_then(|event| {
            let hit =
                ppc_dialog_item_at_global_point(&items, bounds, event.where_v, event.where_h)?;
            let item = items.get(usize::from(hit).checked_sub(1)?)?;
            if item.item_type & !PPC_DIALOG_ITEM_DISABLED == PPC_DIALOG_ITEM_EDIT_TEXT {
                let te_handle = memory
                    .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
                    .unwrap_or(0);
                ppc_te_click(
                    memory,
                    handles,
                    te_handle,
                    event.where_v.saturating_sub(bounds.0),
                    event.where_h.saturating_sub(bounds.1),
                    event.modifiers & 0x0200 != 0,
                    0,
                );
                handled_edit_event = true;
                None
            } else {
                if item.item_type & !PPC_DIALOG_ITEM_DISABLED == PPC_DIALOG_ITEM_RESOURCE_CONTROL
                    && ppc_track_dialog_popup(
                        memory,
                        controls,
                        gworlds,
                        vfs_resources,
                        current_resource_refnum,
                        dialog,
                        item.rect,
                        input,
                    )
                {
                    let _ = ppc_draw_dialog(
                        memory,
                        handles,
                        controls,
                        gworlds,
                        screen_clut,
                        vfs_resources,
                        current_resource_refnum,
                        dialog,
                    );
                }
                Some(hit)
            }
        }),
        Some(3 | 5) => {
            let event = event.as_ref().unwrap();
            let character = event.message as u8;
            let key_code = (event.message >> 8) as u8;
            if matches!(character, b'\r' | 3)
                || matches!(key_code, PPC_KEY_RETURN | PPC_KEY_NUMPAD_ENTER)
            {
                memory
                    .read_u16_be(dialog + PPC_DIALOG_DEFAULT_ITEM_OFFSET)
                    .filter(|item| *item != 0)
            } else if character == 0x1b || key_code == PPC_KEY_ESCAPE {
                memory
                    .read_u16_be(dialog + PPC_DIALOG_CANCEL_ITEM_HLE_OFFSET)
                    .filter(|item| *item != 0)
                    .or_else(|| ppc_dialog_cancel_item(memory, handles, &items))
            } else if character.eq_ignore_ascii_case(&b'a') && event.modifiers & 0x0100 != 0 {
                let te_handle = memory
                    .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
                    .unwrap_or(0);
                if let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) {
                    let length = memory
                        .read_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET)
                        .unwrap_or(0);
                    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, 0);
                    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, length);
                    handled_edit_event = true;
                }
                None
            } else if matches!(character, 0x08 | 0x20..=0x7e) {
                let te_handle = memory
                    .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
                    .unwrap_or(0);
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                let result = ppc_te_key(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    te_handle,
                    character,
                );
                *last_mem_error = result;
                if result == PPC_NO_ERR {
                    handled_edit_event = true;
                    let _ = ppc_draw_dialog(
                        memory,
                        handles,
                        controls,
                        gworlds,
                        screen_clut,
                        vfs_resources,
                        current_resource_refnum,
                        dialog,
                    );
                    ppc_te_draw(
                        memory,
                        handles,
                        gworlds,
                        te_handle,
                        dialog,
                        fore_color,
                        fore_indices,
                    );
                }
                None
            } else {
                None
            }
        }
        Some(6) => {
            let _ = ppc_draw_dialog(
                memory,
                handles,
                controls,
                gworlds,
                screen_clut,
                vfs_resources,
                current_resource_refnum,
                dialog,
            );
            return ppc_begin_dialog_callbacks(
                cpu,
                memory,
                dialog_callback_stack,
                dialog,
                &items,
                bounds,
                PpcDialogCallbackCompletion::Yield,
            );
        }
        _ => None,
    };
    if let Some(hit) = hit {
        let _ = memory.write_u16_be(item_hit_ptr, hit);
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] ModalDialog dialog=${dialog:08X} filter=${:08X} -> item {}",
                cpu.gpr[3], hit
            );
        }
        PpcImportAction::ReturnPreserve
    } else {
        if handled_edit_event && ppc_hle_trace_enabled() {
            let text = memory
                .read_u32_be(dialog + PPC_DIALOG_TEXT_HANDLE_OFFSET)
                .and_then(|handle| ppc_te_text_bytes(memory, handles, handle))
                .unwrap_or_default();
            eprintln!(
                "[PPC-TRACE] ModalDialog dialog=${dialog:08X} edit={:?}",
                String::from_utf8_lossy(&text)
            );
        }
        // ModalDialog is synchronous. Keep the PC at the import until a host
        // event selects an enabled item; returning item 0 makes guest code
        // spin and advances its state contrary to the Toolbox contract.
        PpcImportAction::Yield(u64::MAX)
    }
}
