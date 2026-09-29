//! Typed Dialog Manager dispatch for PowerPC imports.

use super::*;
use crate::dialog_manager::{
    dialog_item_base_type, dialog_item_resource_type_u32, dialog_rect_to_global,
    dialog_target_for_event, dialog_text_rect, edit_text_frame_rect,
    evaluate_count_ditl, evaluate_count_ditl_parameters,
    evaluate_dialog_item_visibility_parameters, evaluate_dialog_select,
    evaluate_dialog_teardown_parameters, evaluate_draw_dialog_parameters,
    evaluate_find_dialog_item, evaluate_get_dialog_item,
    evaluate_get_dialog_item_as_control, evaluate_get_dialog_item_as_control_parameters,
    evaluate_get_dialog_item_parameters, evaluate_get_new_dialog_parameters,
    evaluate_get_std_filter_proc_parameters,
    evaluate_hide_dialog_item, evaluate_is_dialog_event_parameters,
    evaluate_modal_dialog_parameters,
    evaluate_select_dialog_item_text_parameters, evaluate_set_dialog_cancel_item_parameters,
    evaluate_set_dialog_default_item_parameters, evaluate_set_dialog_tracks_cursor_parameters,
    evaluate_show_dialog_item, evaluate_standard_alert_parameters, evaluate_update_dialog_parameters,
    evaluate_alert_parameters,
    extract_dialog_item_text_bytes, find_dialog_item_hit, global_to_dialog_local_point, offset_ditl_bytes, parse_ditl_items,
    position_dialog_bounds as unified_position_dialog_bounds, DialogItemHeader, DialogItemRecord,
    GetNewDialogParameters, ParamTextParameters, SelectDialogItemTextParameters,
    evaluate_error_sound_parameters, ErrorSoundParameters,
    evaluate_alert_dialog_record_init, evaluate_find_dialog_item_parameters_packed,
    evaluate_get_dialog_item_text, evaluate_get_dialog_item_text_parameters,
    evaluate_param_text_parameters, evaluate_set_dialog_item_text, evaluate_set_dialog_item_text_parameters,
    evaluate_std_filter_proc, evaluate_std_filter_proc_event, evaluate_std_filter_proc_parameters,
    evaluate_dialog_item_default_button_outline,
    evaluate_dialog_cancel_item, evaluate_dialog_filter_cancel,
    evaluate_dialog_default_item, evaluate_get_dialog_cancel_item_parameters,
    evaluate_get_dialog_default_item_parameters,
    evaluate_could_dialog_parameters, evaluate_free_dialog_parameters,
    evaluate_could_alert_parameters, evaluate_free_alert_parameters,
    evaluate_move_dialog_item_parameters, evaluate_move_dialog_item_rect,
    evaluate_size_dialog_item_parameters, evaluate_size_dialog_item_rect,
    evaluate_append_dialog_item_list_parameters, evaluate_appended_dialog_bounds,
    DIALOG_ALERT_HIT_OFFSET, DIALOG_CANCEL_ITEM_OFFSET, DIALOG_DEFAULT_ITEM_OFFSET,
    DIALOG_EDIT_FIELD_OFFSET, DIALOG_EDIT_OPEN_OFFSET, DIALOG_ICON_SIZE,
    DIALOG_INITIAL_EDIT_FIELD, DIALOG_INITIAL_EDIT_OPEN,
    DIALOG_ITEMS_OFFSET, DIALOG_ITEM_BUTTON, DIALOG_ITEM_CHECKBOX, DIALOG_ITEM_DISABLED_FLAG,
    DIALOG_ITEM_EDIT_TEXT, DIALOG_ITEM_ICON, DIALOG_ITEM_PICTURE, DIALOG_ITEM_RADIO,
    DIALOG_ITEM_RESOURCE_CONTROL, DIALOG_ITEM_STATIC_TEXT,
    DIALOG_RECORD_SIZE, DIALOG_RESOURCE_ID_OFFSET, DIALOG_STANDARD_ALERT_OUTPUT_OFFSET,
    DIALOG_STANDARD_ALERT_STACK_OFFSET, DIALOG_TEXT_HANDLE_OFFSET,
};
use crate::trap::types::decode_mac_roman;

#[cfg(test)]
pub(super) const PPC_DIALOG_RECORD_SIZE: u32 = DIALOG_RECORD_SIZE;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEMS_OFFSET: u32 = DIALOG_ITEMS_OFFSET;
#[cfg(test)]
pub(super) const PPC_DIALOG_TEXT_HANDLE_OFFSET: u32 = DIALOG_TEXT_HANDLE_OFFSET;
#[cfg(test)]
pub(super) const PPC_DIALOG_EDIT_FIELD_OFFSET: u32 = DIALOG_EDIT_FIELD_OFFSET;
#[cfg(test)]
pub(super) const PPC_DIALOG_EDIT_OPEN_OFFSET: u32 = DIALOG_EDIT_OPEN_OFFSET;
#[cfg(test)]
pub(super) const PPC_DIALOG_DEFAULT_ITEM_OFFSET: u32 = DIALOG_DEFAULT_ITEM_OFFSET;
#[cfg(test)]
pub(super) const PPC_DIALOG_CANCEL_ITEM_OFFSET: u32 = DIALOG_CANCEL_ITEM_OFFSET;
#[cfg(test)]
pub(super) const PPC_DIALOG_RESOURCE_ID_OFFSET: u32 = DIALOG_RESOURCE_ID_OFFSET;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_DISABLED: u8 = DIALOG_ITEM_DISABLED_FLAG;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_USER_ITEM: u8 = crate::dialog_manager::DIALOG_ITEM_USER_ITEM;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_BUTTON: u8 = DIALOG_ITEM_BUTTON;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_CHECKBOX: u8 = DIALOG_ITEM_CHECKBOX;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_RESOURCE_CONTROL: u8 = DIALOG_ITEM_RESOURCE_CONTROL;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_STATIC_TEXT: u8 = DIALOG_ITEM_STATIC_TEXT;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_EDIT_TEXT: u8 = DIALOG_ITEM_EDIT_TEXT;
#[cfg(test)]
pub(super) const PPC_DIALOG_ITEM_ICON: u8 = DIALOG_ITEM_ICON;

pub(super) type PpcDialogTemplate = crate::dialog_manager::DialogTemplate;

pub(super) type PpcDialogItemView = DialogItemRecord;

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
            let eval = crate::dialog_manager::evaluate_init_dialogs(cpu.gpr[3]);
            toolbox_startup.dialogs_initialized = true;
            toolbox_startup.dialog_resume_proc = eval.resume_proc();
            let _ = memory.write_u32_be(crate::memory::globals::addr::RESUME_PROC, eval.resume_proc());
            let _ = memory.write_u32_be(crate::memory::globals::addr::DA_BEEPER, eval.da_beeper());
            let _ = memory.write_u16_be(crate::memory::globals::addr::ALERT_STAGE, eval.initial_alert_stage() as u16);
            for i in 0..eval.da_strings_count() as u32 {
                let _ = memory.write_u32_be(crate::memory::globals::addr::DA_STRINGS + i * 4, 0);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ErrorSound => {
            let eval: ErrorSoundParameters = evaluate_error_sound_parameters(cpu.gpr[3]);
            let _ = memory.write_u32_be(crate::memory::globals::addr::DA_BEEPER, eval.sound_proc());
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetNewDialog => {
            let params = evaluate_get_new_dialog_parameters(
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[4],
                cpu.gpr[5],
            );
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] GetNewDialog id={} storage=${:08X} behind=${:08X} lr=${:08X}",
                    params.dialog_id(),
                    params.storage(),
                    params.behind(),
                    cpu.lr
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
                params,
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
        PpcImportDispatcherTarget::CloseDialog | PpcImportDispatcherTarget::DisposeDialog => {
            let window = cpu.gpr[3];
            let is_dispose =
                binding.dispatcher_target == PpcImportDispatcherTarget::DisposeDialog;
            let Some(params) = evaluate_dialog_teardown_parameters(window, is_dispose) else {
                return Some(PpcImportAction::ReturnPreserve);
            };
            toolbox_startup.dispose_dialog_count =
                toolbox_startup.dispose_dialog_count.saturating_add(1);
            toolbox_startup.last_disposed_dialog = params.dialog_ptr();
            let items_handle = memory
                .read_u32_be(params.dialog_ptr().wrapping_add(DIALOG_ITEMS_OFFSET))
                .unwrap_or(0);
            let items = ppc_dialog_items_for_dialog(memory, handles, params.dialog_ptr()).unwrap_or_default();
            ppc_close_window(
                params.dialog_ptr(),
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
            ppc_release_dialog_storage(
                process_memory_manager,
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
                params.dialog_ptr(),
                items_handle,
                &items,
                params.dispose_record(),
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetDialogItem => {
            ppc_get_dialog_item(cpu, memory, handles);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetDialogItemAsControl => {
            let dialog = cpu.gpr[3];
            let item_number = cpu.gpr[4] as u16 as usize;
            let control_out = cpu.gpr[5];
            let can_write = ppc_memory_can_write_bytes(memory, control_out, 4);
            let params = match evaluate_get_dialog_item_as_control_parameters(
                dialog,
                item_number,
                control_out,
                can_write,
            ) {
                Ok(params) => params,
                Err(err) => return Some(PpcImportAction::Return(ppc_i16_result(err))),
            };
            let control_result = ppc_dialog_items_for_dialog(memory, handles, params.dialog_ptr())
                .and_then(|items| {
                    crate::dialog_manager::get_item_at_1_indexed(&items, params.item_number()).cloned()
                })
                .map(|item| evaluate_get_dialog_item_as_control(item.item_type, item.handle))
                .unwrap_or(Err(PPC_PARAM_ERR));
            let control = control_result.unwrap_or(0);
            let _ = memory.write_u32_be(params.control_out(), control);
            Some(PpcImportAction::Return(ppc_i16_result(if control_result.is_ok() {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            })))
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
            let can_write = ppc_memory_can_write_bytes(memory, text_out_ptr, 1);
            if let Some(params) =
                evaluate_get_dialog_item_text_parameters(item_handle, text_out_ptr, can_write)
            {
                let bytes = ppc_handle_bytes(memory, handles, params.item_handle()).unwrap_or_default();
                if let Some(eval) =
                    evaluate_get_dialog_item_text(params.item_handle(), params.text_out_ptr(), &bytes)
                {
                    if ppc_memory_can_write_bytes(memory, eval.text_out_ptr, u32::from(eval.len) + 1) {
                        let _ = memory.write_u8(eval.text_out_ptr, eval.len);
                        for (offset, byte) in eval.text.iter().copied().enumerate() {
                            let _ = memory.write_u8(eval.text_out_ptr + 1 + offset as u32, byte);
                        }
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
            if let Some(params) = evaluate_set_dialog_item_text_parameters(item_handle, text_ptr) {
                let raw_text = ppc_read_pstring_bytes(memory, params.text_ptr()).unwrap_or_default();
                if let Some(eval) =
                    evaluate_set_dialog_item_text(params.item_handle(), params.text_ptr(), &raw_text)
                {
                    if ppc_hle_trace_enabled() {
                        eprintln!(
                            "[PPC-TRACE] SetDialogItemText handle=${:08X} text={:?}",
                            eval.item_handle,
                            eval.text
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
                        eval.item_handle,
                        eval.byte_len() as u32,
                    );
                    *last_mem_error = result;
                    if *last_mem_error == PPC_NO_ERR {
                        if let Some(ptr) = memory.read_u32_be(eval.item_handle) {
                            for (offset, byte) in eval.bytes.iter().copied().enumerate() {
                                let _ = memory.write_u8(ptr + offset as u32, byte);
                            }
                        }
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetDialogDefaultItem => {
            // Appearance Manager 1.0 SetDialogDefaultItem records which item
            // Return activates and reports an OSErr.
            let dialog = cpu.gpr[3];
            let item = cpu.gpr[4] as u16 as i16;
            let result = evaluate_set_dialog_default_item_parameters(dialog, item);
            let os_err = match result {
                Ok(params) => {
                    if memory
                        .write_u16_be(
                            params.dialog_ptr() + DIALOG_DEFAULT_ITEM_OFFSET,
                            params.item_no() as u16,
                        )
                        .is_some()
                    {
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::GetDialogDefaultItem => {
            let dialog = cpu.gpr[3];
            let out_item_ptr = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_item_ptr, 2);
            let result =
                evaluate_get_dialog_default_item_parameters(dialog, out_item_ptr, can_write);
            let os_err = match result {
                Ok(params) => {
                    let configured = memory
                        .read_u16_be(params.dialog_ptr() + DIALOG_DEFAULT_ITEM_OFFSET)
                        .map(|item| item as i16);
                    let default_item = evaluate_dialog_default_item(configured);
                    if memory
                        .write_u16_be(params.out_default_item_ptr(), default_item as u16)
                        .is_some()
                    {
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::SetDialogCancelItem => {
            // Macintosh Toolbox Essentials (1992), p. 6-165: the System 7
            // cancel item is Dialog Manager state rather than a public
            // DialogRecord field. Keep it in the HLE tail of our allocation.
            let dialog = cpu.gpr[3];
            let item = cpu.gpr[4] as u16 as i16;
            let result = evaluate_set_dialog_cancel_item_parameters(dialog, item);
            let os_err = match result {
                Ok(params) => {
                    if memory
                        .write_u16_be(
                            params.dialog_ptr() + DIALOG_CANCEL_ITEM_OFFSET,
                            params.item_no() as u16,
                        )
                        .is_some()
                    {
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::GetDialogCancelItem => {
            let dialog = cpu.gpr[3];
            let out_item_ptr = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_item_ptr, 2);
            let result =
                evaluate_get_dialog_cancel_item_parameters(dialog, out_item_ptr, can_write);
            let os_err = match result {
                Ok(params) => {
                    let configured = memory
                        .read_u16_be(params.dialog_ptr() + DIALOG_CANCEL_ITEM_OFFSET)
                        .filter(|&item| item != 0)
                        .map(|item| item as i16);
                    let items = ppc_dialog_items_for_dialog(memory, handles, params.dialog_ptr())
                        .unwrap_or_default();
                    let cancel_item = ppc_dialog_cancel_item(memory, handles, &items, configured)
                        .unwrap_or(0);
                    if memory
                        .write_u16_be(params.out_cancel_item_ptr(), cancel_item)
                        .is_some()
                    {
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::SetDialogTracksCursor => {
            // SetDialogTracksCursor (DialogPtr, Boolean) returns OSErr.
            // Cursor tracking is performed by the host UI when applicable.
            let dialog = cpu.gpr[3];
            let tracks = cpu.gpr[4] != 0;
            let os_err = match evaluate_set_dialog_tracks_cursor_parameters(dialog, tracks) {
                Ok(_params) => PPC_NO_ERR,
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::MoveDialogItem => {
            let dialog = cpu.gpr[3];
            let item_no = cpu.gpr[4] as u16 as i16;
            let in_horiz = cpu.gpr[5] as u16 as i16;
            let in_vert = cpu.gpr[6] as u16 as i16;
            let os_err = match evaluate_move_dialog_item_parameters(dialog, item_no, in_horiz, in_vert) {
                Ok(params) => {
                    let live_items = ppc_dialog_live_items(memory, handles, params.dialog_ptr());
                    if let Some((_handle, ptr, _bytes, items)) = live_items {
                        if let Some(item) = crate::dialog_manager::get_item_at_1_indexed(&items, params.item_number()) {
                            let new_rect = evaluate_move_dialog_item_rect(item.rect, params.in_horiz(), params.in_vert());
                            let item_addr = ptr + item.item_offset as u32;
                            let rect_addr = item_addr + crate::dialog_manager::DITL_ITEM_RECT_OFFSET;
                            let _ = memory.write_u16_be(rect_addr, new_rect.0 as u16);
                            let _ = memory.write_u16_be(rect_addr + 2, new_rect.1 as u16);
                            let _ = memory.write_u16_be(rect_addr + 4, new_rect.2 as u16);
                            let _ = memory.write_u16_be(rect_addr + 6, new_rect.3 as u16);
                            if item.handle != 0 {
                                if let Some(control) = ppc_control_ptr(memory, item.handle) {
                                    let _ = ppc_write_rect(
                                        memory,
                                        control + PPC_CONTROL_RECT_OFFSET,
                                        new_rect.0,
                                        new_rect.1,
                                        new_rect.2,
                                        new_rect.3,
                                    );
                                }
                            }
                            PPC_NO_ERR
                        } else {
                            PPC_PARAM_ERR
                        }
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::SizeDialogItem => {
            let dialog = cpu.gpr[3];
            let item_no = cpu.gpr[4] as u16 as i16;
            let in_width = cpu.gpr[5] as u16 as i16;
            let in_height = cpu.gpr[6] as u16 as i16;
            let os_err = match evaluate_size_dialog_item_parameters(dialog, item_no, in_width, in_height) {
                Ok(params) => {
                    let live_items = ppc_dialog_live_items(memory, handles, params.dialog_ptr());
                    if let Some((_handle, ptr, _bytes, items)) = live_items {
                        if let Some(item) = crate::dialog_manager::get_item_at_1_indexed(&items, params.item_number()) {
                            let new_rect = evaluate_size_dialog_item_rect(item.rect, params.in_width(), params.in_height());
                            let item_addr = ptr + item.item_offset as u32;
                            let rect_addr = item_addr + crate::dialog_manager::DITL_ITEM_RECT_OFFSET;
                            let _ = memory.write_u16_be(rect_addr, new_rect.0 as u16);
                            let _ = memory.write_u16_be(rect_addr + 2, new_rect.1 as u16);
                            let _ = memory.write_u16_be(rect_addr + 4, new_rect.2 as u16);
                            let _ = memory.write_u16_be(rect_addr + 6, new_rect.3 as u16);
                            if item.handle != 0 {
                                if let Some(control) = ppc_control_ptr(memory, item.handle) {
                                    let _ = ppc_write_rect(
                                        memory,
                                        control + PPC_CONTROL_RECT_OFFSET,
                                        new_rect.0,
                                        new_rect.1,
                                        new_rect.2,
                                        new_rect.3,
                                    );
                                }
                            }
                            PPC_NO_ERR
                        } else {
                            PPC_PARAM_ERR
                        }
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::AppendDialogItemList => {
            let dialog = cpu.gpr[3];
            let ditl_id = cpu.gpr[4] as u16 as i16;
            let method = cpu.gpr[5] as u16 as i16;
            let os_err = match evaluate_append_dialog_item_list_parameters(dialog, ditl_id, method) {
                Ok(params) => {
                    let ditl_index = ppc_vfs_resource_index(
                        vfs_resources,
                        current_resource_refnum,
                        u32::from_be_bytes(*b"DITL"),
                        params.ditl_id(),
                        false,
                    );
                    if let Some(ditl_index) = ditl_index {
                        let mut appended_bytes = vfs_resources[ditl_index].data.clone();
                        if let Some(appended_items) = ppc_parse_dialog_items(&appended_bytes) {
                            let live_items = ppc_dialog_live_items(memory, handles, params.dialog_ptr());
                            if let Some((handle, _ptr, current_bytes, current_items)) = live_items {
                                let method = params.method();
                                let dialog_width = gworlds
                                    .iter()
                                    .find(|gworld| gworld.port == params.dialog_ptr())
                                    .map_or(0, |gworld| ppc_u32_to_i16_saturating(gworld.width));
                                let dialog_height = gworlds
                                    .iter()
                                    .find(|gworld| gworld.port == params.dialog_ptr())
                                    .map_or(0, |gworld| ppc_u32_to_i16_saturating(gworld.height));
                                let (dv, dh) = crate::dialog_manager::append_ditl_offset_delta(
                                    method,
                                    dialog_height,
                                    dialog_width,
                                    |item_no| {
                                        current_items
                                            .get(item_no.saturating_sub(1))
                                            .map(|item| (item.rect.0, item.rect.1))
                                    },
                                );
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
                                let result = allocator.resize_handle(
                                    memory,
                                    heap_cursor,
                                    last_mem_error,
                                    handles,
                                    handle,
                                    size,
                                );
                                *last_mem_error = result;
                                if result == PPC_NO_ERR {
                                    if let Some(ptr) = memory.read_u32_be(handle) {
                                        let _ = memory.write_bytes(ptr, &combined);
                                    }
                                }

                                if let Some(gworld) = gworlds.iter_mut().find(|gw| gw.port == params.dialog_ptr()) {
                                    let old_bounds = (0, 0, gworld.height as i16, gworld.width as i16);
                                    let new_bounds = evaluate_appended_dialog_bounds(
                                        old_bounds,
                                        appended_items.iter().map(|item| {
                                            (
                                                item.rect.0 + dv,
                                                item.rect.1 + dh,
                                                item.rect.2 + dv,
                                                item.rect.3 + dh,
                                            )
                                        }),
                                    );
                                    gworld.height = (new_bounds.2.max(old_bounds.2)) as u32;
                                    gworld.width = (new_bounds.3.max(old_bounds.3)) as u32;
                                    let _ = ppc_write_rect(
                                        memory,
                                        params.dialog_ptr() + 16,
                                        0,
                                        0,
                                        gworld.height as i16,
                                        gworld.width as i16,
                                    );
                                }
                                PPC_NO_ERR
                            } else {
                                PPC_PARAM_ERR
                            }
                        } else {
                            PPC_PARAM_ERR
                        }
                    } else {
                        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                        PPC_RES_NOT_FOUND_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::CouldDialog => {
            let dialog_id = cpu.gpr[3] as u16 as i16;
            let _params = evaluate_could_dialog_parameters(dialog_id);
            *last_resource_error = 0;
            let _ = memory.write_u16_be(crate::memory::globals::addr::RES_ERR, 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FreeDialog => {
            let dialog_id = cpu.gpr[3] as u16 as i16;
            let _params = evaluate_free_dialog_parameters(dialog_id);
            *last_resource_error = 0;
            let _ = memory.write_u16_be(crate::memory::globals::addr::RES_ERR, 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CouldAlert => {
            let alert_id = cpu.gpr[3] as u16 as i16;
            let _params = evaluate_could_alert_parameters(alert_id);
            *last_resource_error = 0;
            let _ = memory.write_u16_be(crate::memory::globals::addr::RES_ERR, 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FreeAlert => {
            let alert_id = cpu.gpr[3] as u16 as i16;
            let _params = evaluate_free_alert_parameters(alert_id);
            *last_resource_error = 0;
            let _ = memory.write_u16_be(crate::memory::globals::addr::RES_ERR, 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::StdFilterProc => Some(PpcImportAction::Return(
            ppc_standard_filter_proc(cpu, memory),
        )),
        PpcImportDispatcherTarget::GetStdFilterProc => {
            // Apple Dialog Manager Reference (2007), p. 38:
            // OSErr GetStdFilterProc(ModalFilterUPP *theProc).
            let out_proc = cpu.gpr[3];
            let can_write = ppc_memory_can_write_bytes(memory, out_proc, 4);
            let os_err = match evaluate_get_std_filter_proc_parameters(out_proc, can_write) {
                Ok(params) => {
                    if memory
                        .write_u32_be(params.out_proc(), PPC_STD_FILTER_TVECTOR)
                        .is_some()
                    {
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(os_err)))
        }
        PpcImportDispatcherTarget::DrawDialog => {
            if let Some(action) = ppc_resume_dialog_callbacks(cpu, memory, dialog_callback_stack) {
                return Some(action);
            }
            let Some(params) = evaluate_draw_dialog_parameters(cpu.gpr[3]) else {
                return Some(PpcImportAction::ReturnPreserve);
            };
            let dialog = params.dialog_ptr();
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
            if let Some(params) = evaluate_select_dialog_item_text_parameters(
                cpu.gpr[3],
                cpu.gpr[4] as u16 as usize,
                cpu.gpr[5] as u16 as i16,
                cpu.gpr[6] as u16 as i16,
            ) {
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
                    params,
                    tick_count,
                    quickdraw_text_mode,
                    quickdraw_text_size,
                    *quickdraw_fore_color,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ParamText => {
            let params = evaluate_param_text_parameters(
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[6],
            );
            param_text.with_mut(|slots| ppc_param_text(memory, &params, slots));
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::AlertReturnDefault(kind) => {
            let alert_id = cpu.gpr[3] as u16 as i16;
            let filter_proc = cpu.gpr[4];
            let params = evaluate_alert_parameters(alert_id, filter_proc, kind);
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] {} id={} filter=${:08X} params={:?}",
                    params.kind().name(),
                    params.alert_id(),
                    params.filter_proc(),
                    param_text
                        .iter()
                        .map(|bytes| decode_mac_roman(&bytes))
                        .collect::<Vec<_>>()
                );
            }
            let mut dialog = gworlds.iter().rev().find_map(|record| {
                let matches_call = memory
                    .read_u32_be(record.port + DIALOG_STANDARD_ALERT_OUTPUT_OFFSET)
                    .unwrap_or(0)
                    == 0
                    && memory.read_u16_be(record.port + DIALOG_RESOURCE_ID_OFFSET)
                        == Some(params.alert_id() as u16);
                (memory.read_u16_be(record.port + PPC_CWINDOW_WINDOW_KIND_OFFSET) == Some(2)
                    && ppc_window_is_visible(memory, record.port)
                    && matches_call)
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
                    params.alert_id(),
                    false,
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
            modal_cpu.gpr[3] = params.filter_proc();
            modal_cpu.gpr[4] = dialog + DIALOG_ALERT_HIT_OFFSET;
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
                    .read_u16_be(dialog + DIALOG_ALERT_HIT_OFFSET)
                    .unwrap_or(1);
                let items_handle = memory
                    .read_u32_be(dialog + DIALOG_ITEMS_OFFSET)
                    .unwrap_or(0);
                let items =
                    ppc_dialog_items_for_dialog(memory, handles, dialog).unwrap_or_default();
                ppc_close_window(
                    dialog,
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
                ppc_release_dialog_storage(
                    process_memory_manager,
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
                    items_handle,
                    &items,
                    true,
                );
                Some(PpcImportAction::Return(u32::from(hit)))
            } else {
                Some(action)
            }
        }
        PpcImportDispatcherTarget::StandardAlert => {
            // StandardAlert displays a modal alert and returns the chosen button
            // through outItemHit, with an OSErr in r3.
            // OSErr StandardAlert(AlertType, ConstStr255Param, ConstStr255Param,
            //                     const AlertStdAlertParamRec *, SInt16 *);
            // Apple Dialog Manager Reference, pp. 65, 75–76, 82–83.
            let output = cpu.gpr[7];
            let can_write = ppc_memory_can_write_bytes(memory, output, 2);
            let params = match evaluate_standard_alert_parameters(
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[6],
                output,
                true,
                can_write,
            ) {
                Ok(params) => params,
                Err(err) => return Some(PpcImportAction::Return(ppc_i16_result(err))),
            };
            if ppc_hle_trace_enabled() {
                eprintln!(
                    "[PPC-TRACE] StandardAlert id={} params={:?}",
                    params.alert_id(),
                    param_text
                        .iter()
                        .map(|bytes| decode_mac_roman(&bytes))
                        .collect::<Vec<_>>()
                );
            }
            let mut dialog = gworlds.iter().rev().find_map(|record| {
                let matches_call = memory.read_u32_be(record.port + DIALOG_STANDARD_ALERT_OUTPUT_OFFSET)
                    == Some(params.item_hit_out_ptr())
                    && memory.read_u32_be(record.port + DIALOG_STANDARD_ALERT_STACK_OFFSET)
                        == Some(cpu.gpr[1]);
                (memory.read_u16_be(record.port + PPC_CWINDOW_WINDOW_KIND_OFFSET) == Some(2)
                    && ppc_window_is_visible(memory, record.port)
                    && matches_call)
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
                    params.alert_id(),
                    true,
                    param_text,
                );
                if created == 0 {
                    return Some(PpcImportAction::Return(ppc_i16_result(*last_resource_error)));
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
            modal_cpu.gpr[3] = if params.alert_param_ptr() == 0 {
                0
            } else {
                memory.read_u32_be(params.alert_param_ptr() + 2).unwrap_or(0)
            };
            modal_cpu.gpr[4] = dialog + DIALOG_ALERT_HIT_OFFSET;
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
                    .read_u16_be(dialog + DIALOG_ALERT_HIT_OFFSET)
                    .unwrap_or(1);
                let items_handle = memory
                    .read_u32_be(dialog + DIALOG_ITEMS_OFFSET)
                    .unwrap_or(0);
                let items =
                    ppc_dialog_items_for_dialog(memory, handles, dialog).unwrap_or_default();
                ppc_close_window(
                    dialog,
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
                ppc_release_dialog_storage(
                    process_memory_manager,
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
                    items_handle,
                    &items,
                    true,
                );
                let _ = memory.write_u16_be(params.item_hit_out_ptr(), hit);
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
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

#[allow(clippy::too_many_arguments)]
fn ppc_release_dialog_storage(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    dialog: u32,
    items_handle: u32,
    items: &[PpcDialogItemView],
    dispose_record: bool,
) {
    if dialog == 0 {
        return;
    }

    // Macintosh Toolbox Essentials (1992), pp. 6-119--6-120: CloseDialog
    // releases the dialog's standard items and their supporting structures;
    // DisposeDialog additionally releases the copied DITL and DialogRecord.
    // A DialogRecord TERec borrows its active edit item's text handle, so
    // detach that handle before disposing the TERec and then release each
    // manager-created text item exactly once.
    let te_handle = memory
        .read_u32_be(dialog.wrapping_add(DIALOG_TEXT_HANDLE_OFFSET))
        .unwrap_or(0);
    if let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) {
        let _ = memory.write_u32_be(te_ptr + PPC_TE_HTEXT_OFFSET, 0);
    }
    {
        let mut allocator = PpcProcessAllocatorView {
            memory_manager: process_memory_manager,
        };
        ppc_te_dispose(
            Some(&mut allocator),
            None,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            te_handle,
        );

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

        let items_ptr = memory.read_u32_be(items_handle).unwrap_or(0);
        for item in items {
            if crate::dialog_manager::is_dialog_item_disposable_text(item.item_type) {
                let _ = allocator.dispose_handle(
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    item.handle,
                );
                if items_ptr != 0 {
                    let _ = memory.write_u32_be(items_ptr.wrapping_add(item.item_offset as u32), 0);
                }
            }
        }
        if dispose_record {
            let _ = allocator.dispose_handle(
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                items_handle,
            );
        }
    }

    let _ = memory.write_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET, 0);
    let _ = memory.write_u16_be(
        dialog + DIALOG_EDIT_FIELD_OFFSET,
        DIALOG_INITIAL_EDIT_FIELD as u16,
    );
    let _ = memory.write_u16_be(
        dialog + DIALOG_EDIT_OPEN_OFFSET,
        DIALOG_INITIAL_EDIT_OPEN as u16,
    );
    if dispose_record {
        let _ = process_memory_manager.dispose_native_ptr(dialog);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
    }
}

fn ppc_param_text(
    memory: &mut PpcSectionMem,
    params: &ParamTextParameters,
    param_text: &mut [Vec<u8>; 4],
) {
    let mut storage: [Option<Vec<u8>>; crate::dialog_manager::PARAM_TEXT_SLOT_COUNT] =
        [None, None, None, None];
    for (index, slot_storage) in storage.iter_mut().enumerate() {
        let ptr = params.param(index);
        if ptr != 0 {
            if let Some(bytes) = ppc_read_pstring_bytes(memory, ptr) {
                *slot_storage = Some(bytes);
            }
        }
    }
    let eval = crate::dialog_manager::evaluate_param_text([
        storage[0].as_deref(),
        storage[1].as_deref(),
        storage[2].as_deref(),
        storage[3].as_deref(),
    ]);
    eval.apply_to(param_text);
    if ppc_hle_trace_enabled() {
        let strings = eval.decoded_strings(param_text);
        eprintln!("[PPC-TRACE] ParamText strings={:?}", strings);
    }
}

pub(super) fn ppc_apply_param_text<'a>(
    text: &'a [u8],
    param_text: &[Vec<u8>; 4],
) -> std::borrow::Cow<'a, [u8]> {
    crate::dialog_manager::apply_param_text(text, param_text)
}

fn ppc_dialog_live_items(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    dialog: u32,
) -> Option<(u32, u32, Vec<u8>, Vec<PpcDialogItemView>)> {
    let handle = memory.read_u32_be(dialog.checked_add(DIALOG_ITEMS_OFFSET)?)?;
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
    offset_ditl_bytes(bytes, items.iter().map(|item| item.item_offset), dv, dh);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcDialogCompatibilityOperation {
    AppendDitl,
    CountDitl,
    DialogSelect,
    FindDialogItem,
    HideDialogItem,
    IsDialogEvent,
    ShortenDitl,
    ShowDialogItem,
    UpdateDialog,
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
            let Some(params) = evaluate_is_dialog_event_parameters(cpu.gpr[3])
            else {
                return PpcImportAction::Return(0);
            };
            let result = ppc_read_dialog_event(memory, params.event_ptr()).is_some_and(|event| {
                let dialog = ppc_dialog_for_event(memory, gworlds, event.what, event.message);
                let bounds = dialog.and_then(|d| ppc_dialog_global_bounds(memory, gworlds, d));
                crate::dialog_manager::is_dialog_event(
                    event.what,
                    event.message,
                    event.where_v,
                    event.where_h,
                    dialog,
                    bounds,
                )
            });
            PpcImportAction::Return(u32::from(result))
        }
        PpcDialogCompatibilityOperation::DialogSelect => {
            if let Some(action) = ppc_resume_dialog_callbacks(cpu, memory, dialog_callback_stack) {
                return action;
            }
            let Some(params) = crate::dialog_manager::evaluate_dialog_select_parameters(
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
            ) else {
                return PpcImportAction::Return(0);
            };
            let Some(event) = ppc_read_dialog_event(memory, params.event_ptr()) else {
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

            let active_edit = {
                let edit_field = memory
                    .read_u16_be(dialog + DIALOG_EDIT_FIELD_OFFSET)
                    .unwrap_or(u16::MAX);
                if edit_field != u16::MAX {
                    let edit_item = (edit_field as i16).saturating_add(1);
                    items
                        .get(usize::from(edit_item as u16).saturating_sub(1))
                        .map(|item| (edit_item, item.item_type))
                } else {
                    None
                }
            };

            let action = evaluate_dialog_select(
                event.what,
                event.message,
                event.where_v,
                event.where_h,
                Some(dialog),
                Some(bounds),
                active_edit,
                |v, h| {
                    ppc_dialog_item_at_global_point(memory, controls, &items, bounds, v, h)
                        .and_then(|hit| {
                            let idx = usize::from(hit).saturating_sub(1);
                            items.get(idx).map(|item| (hit as i16, item.item_type))
                        })
                },
            );

            if action.should_set_dialog_ptr() && params.has_dialog_out() {
                let _ = memory.write_u32_be(params.dialog_out_ptr(), dialog);
            }
            if let Some(hit) = action.item_hit() {
                if params.has_item_hit_out() {
                    let _ = memory.write_u16_be(params.item_hit_ptr(), hit as u16);
                }
            }

            match action {
                crate::dialog_manager::DialogSelectAction::Update { .. } => {
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
                crate::dialog_manager::DialogSelectAction::ItemHit {
                    item_no,
                    is_edit_text,
                    is_resource_control,
                    ..
                } => {
                    if is_edit_text {
                        let te_handle = memory
                            .read_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET)
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
                    } else if is_resource_control {
                        if let Some(item) =
                            items.get(usize::from(item_no as u16).saturating_sub(1))
                        {
                            let _ = ppc_track_scroll_control_value(
                                memory,
                                handles,
                                controls,
                                gworlds,
                                vfs_resources,
                                current_resource_refnum,
                                item.handle,
                                event.where_v.saturating_sub(bounds.0),
                                event.where_h.saturating_sub(bounds.1),
                            );
                        }
                    }
                    PpcImportAction::Return(1)
                }
                crate::dialog_manager::DialogSelectAction::KeyStroke {
                    character, ..
                } => {
                    let te_handle = memory
                        .read_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET)
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
                    if *last_mem_error != PPC_NO_ERR {
                        return PpcImportAction::Return(0);
                    }
                    PpcImportAction::Return(1)
                }
                _ => PpcImportAction::Return(0),
            }
        }
        PpcDialogCompatibilityOperation::CountDitl => {
            let count = evaluate_count_ditl_parameters(dialog).map_or(0, |params| {
                let items = ppc_dialog_items_for_dialog(memory, handles, params.dialog_ptr());
                evaluate_count_ditl(None, items.map_or(0, |i| i.len()))
            });
            PpcImportAction::Return(u32::from(count))
        }
        PpcDialogCompatibilityOperation::FindDialogItem => {
            // Inside Macintosh Volume IV, p. IV-60 and Macintosh Toolbox Essentials (1992), p. 6-125:
            // FindDialogItem takes dialog-local coordinates and returns the 0-indexed item number
            // of any item containing the point, whether enabled or disabled, or -1 if none match.
            // Control items use ppc_control_part_at_point so transparent group box bodies fall through.
            let found = evaluate_find_dialog_item_parameters_packed(dialog, cpu.gpr[4])
                .and_then(|params| {
                    ppc_dialog_items_for_dialog(memory, handles, params.dialog_ptr()).map(|items| {
                        evaluate_find_dialog_item(
                            items.iter().map(|item| (item.rect, item.item_type)),
                            params.pt_v(),
                            params.pt_h(),
                            |index| {
                                let item = &items[index];
                                if item.handle != 0
                                    && controls.iter().any(|record| record.handle == item.handle)
                                {
                                    ppc_control_part_at_point(
                                        memory,
                                        controls,
                                        item.handle,
                                        params.pt_v(),
                                        params.pt_h(),
                                    )
                                    .is_some_and(|part| part != 0)
                                } else {
                                    true
                                }
                            },
                        )
                    })
                })
                .unwrap_or(-1);
            PpcImportAction::Return(found as i32 as u32)
        }
        PpcDialogCompatibilityOperation::HideDialogItem
        | PpcDialogCompatibilityOperation::ShowDialogItem => {
            let item_number = cpu.gpr[4] as u16 as usize;
            if let Some(params) =
                evaluate_dialog_item_visibility_parameters(dialog, item_number)
            {
                if let Some((_handle, ptr, _bytes, items)) =
                    ppc_dialog_live_items(memory, handles, params.dialog_ptr())
                {
                    if let Some(item) =
                        crate::dialog_manager::get_item_at_1_indexed(&items, params.item_number())
                    {
                        let hide = operation == PpcDialogCompatibilityOperation::HideDialogItem;
                        let change = if hide {
                            evaluate_hide_dialog_item(item.item_type, item.rect)
                        } else {
                            evaluate_show_dialog_item(item.item_type, item.rect, None)
                        };
                        if let Some(change) = change {
                            let item_addr = ptr + item.item_offset as u32;
                            let _ = memory.write_u16_be(
                                item_addr + crate::dialog_manager::DITL_ITEM_RECT_OFFSET + 2,
                                change.new_rect().1 as u16,
                            );
                            let _ = memory.write_u16_be(
                                item_addr + crate::dialog_manager::DITL_ITEM_RECT_OFFSET + 6,
                                change.new_rect().3 as u16,
                            );
                        }
                    }
                }
            }
            PpcImportAction::ReturnPreserve
        }
        PpcDialogCompatibilityOperation::AppendDitl => {
            let Some(params) = crate::dialog_manager::evaluate_append_ditl_parameters(
                dialog,
                cpu.gpr[4],
                cpu.gpr[5] as u16 as i16,
            ) else {
                return PpcImportAction::ReturnPreserve;
            };
            let Some((handle, _ptr, current_bytes, current_items)) =
                ppc_dialog_live_items(memory, handles, params.dialog_ptr())
            else {
                return PpcImportAction::ReturnPreserve;
            };
            let Some(mut appended_bytes) = ppc_handle_bytes(memory, handles, params.ditl_handle()) else {
                return PpcImportAction::ReturnPreserve;
            };
            let Some(appended_items) = ppc_parse_dialog_items(&appended_bytes) else {
                return PpcImportAction::ReturnPreserve;
            };
            let method = params.method();
            let dialog_width = gworlds
                .iter()
                .find(|gworld| gworld.port == params.dialog_ptr())
                .map_or(0, |gworld| ppc_u32_to_i16_saturating(gworld.width));
            let dialog_height = gworlds
                .iter()
                .find(|gworld| gworld.port == params.dialog_ptr())
                .map_or(0, |gworld| ppc_u32_to_i16_saturating(gworld.height));
            let (dv, dh) = crate::dialog_manager::append_ditl_offset_delta(
                method,
                dialog_height,
                dialog_width,
                |item_no| {
                    current_items
                        .get(item_no.saturating_sub(1))
                        .map(|item| (item.rect.0, item.rect.1))
                },
            );
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
            let Some(params) = crate::dialog_manager::evaluate_shorten_ditl_parameters(
                dialog,
                usize::from(cpu.gpr[4] as u16),
            ) else {
                return PpcImportAction::ReturnPreserve;
            };
            let Some((handle, _ptr, mut bytes, items)) =
                ppc_dialog_live_items(memory, handles, params.dialog_ptr())
            else {
                return PpcImportAction::ReturnPreserve;
            };
            let (retained, count_minus_one) =
                crate::dialog_manager::shorten_ditl_counts(items.len(), params.number_items());
            let end = if retained == 0 {
                2
            } else {
                ppc_dialog_item_end(&bytes, &items[retained - 1]).unwrap_or(bytes.len())
            };
            bytes.truncate(end);
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
            let Some(params) = evaluate_update_dialog_parameters(dialog, cpu.gpr[4]) else {
                return PpcImportAction::ReturnPreserve;
            };
            let dialog = params.dialog_ptr();
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

fn ppc_parse_dialog_template(bytes: &[u8]) -> Option<PpcDialogTemplate> {
    crate::dialog_manager::parse_dialog_template(bytes)
}

fn ppc_parse_dialog_items(bytes: &[u8]) -> Option<Vec<PpcDialogItemView>> {
    parse_ditl_items(bytes)
}

fn ppc_position_dialog_bounds(
    bounds: (i16, i16, i16, i16),
    position: u16,
    gworlds: &[PpcGWorldRecord],
) -> (i16, i16, i16, i16) {
    let screen = gworlds.iter().find(|record| record.port == PPC_MAIN_GWORLD);
    let screen_width = screen.map_or(ppc_main_screen_width(), |record| record.width) as i32;
    let screen_height = screen.map_or(ppc_main_screen_height(), |record| record.height) as i32;
    unified_position_dialog_bounds(bounds, position, screen_width, screen_height)
}

type PpcAlertTemplate = ((i16, i16, i16, i16), Vec<u8>, u16, u16, u16, u32);

fn ppc_standard_alert_template(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> Result<PpcAlertTemplate, i16> {
    // AlertStdAlertParamRec uses classic two-byte structure alignment.
    // Apple Dialog Manager Reference, pp. 75–76, 82–85.
    let params = cpu.gpr[6];
    if cpu.gpr[3] > 3 || (params != 0 && memory.read_u16_be(params + 22).is_none()) {
        return Err(PPC_PARAM_ERR);
    }
    let read_text = |memory: &mut PpcSectionMem, ptr| {
        if ptr == 0 {
            Ok(Vec::new())
        } else {
            ppc_read_pstring_bytes(memory, ptr).ok_or(PPC_PARAM_ERR)
        }
    };
    let primary = read_text(memory, cpu.gpr[4])?;
    let secondary = read_text(memory, cpu.gpr[5])?;
    let mut labels = Vec::new();
    for (index, fallback) in [
        b"OK".as_slice(),
        b"Cancel".as_slice(),
        b"Don\xD5t Save".as_slice(),
    ]
    .into_iter()
    .enumerate()
    {
        let ptr = if params == 0 {
            if index == 0 {
                u32::MAX
            } else {
                0
            }
        } else {
            memory
                .read_u32_be(params + 6 + index as u32 * 4)
                .ok_or(PPC_PARAM_ERR)?
        };
        labels.push(if ptr == u32::MAX {
            fallback.to_vec()
        } else {
            read_text(memory, ptr)?
        });
    }
    let help = params != 0 && memory.read_u8(params + 1).unwrap_or(0) != 0;
    labels.push(if help { b"?".to_vec() } else { Vec::new() });
    let default_item = if params == 0 {
        1
    } else {
        memory.read_u16_be(params + 18).ok_or(PPC_PARAM_ERR)?
    };
    let cancel_item = if params == 0 {
        0
    } else {
        memory.read_u16_be(params + 20).ok_or(PPC_PARAM_ERR)?
    };
    for item in [default_item, cancel_item] {
        if item > 4 || (item != 0 && labels[usize::from(item - 1)].is_empty()) {
            return Err(PPC_PARAM_ERR);
        }
    }
    let movable = params != 0 && memory.read_u8(params).unwrap_or(0) != 0;
    let position = if params == 0 {
        0
    } else {
        memory.read_u16_be(params + 22).ok_or(PPC_PARAM_ERR)?
    };
    let width = 440i16;
    let text_left = if cpu.gpr[3] == crate::dialog_manager::ALERT_TYPE_PLAIN as u32 {
        20
    } else {
        64
    };
    let text_width = width - text_left - 20;
    let primary_height = (ppc_dialog_text_lines(&primary, text_width).len() as i16 * 16).max(16);
    let secondary_height = if secondary.is_empty() {
        0
    } else {
        ppc_dialog_text_lines(&secondary, text_width).len() as i16 * 16 + 8
    };
    let text_bottom = 20 + primary_height + secondary_height;
    let button_top = text_bottom.max(60) + 20;
    let height = button_top + 40;
    let mut items = vec![0, 0];
    let mut count = 0u16;
    let mut append = |kind: u8, rect: (i16, i16, i16, i16), bytes: &[u8]| {
        items.extend_from_slice(&0u32.to_be_bytes());
        for value in [rect.0, rect.1, rect.2, rect.3] {
            items.extend_from_slice(&value.to_be_bytes());
        }
        items.push(kind);
        items.push(bytes.len() as u8);
        items.extend_from_slice(bytes);
        if items.len() % 2 != 0 {
            items.push(0);
        }
        count += 1;
    };
    let mut right = width - 20;
    for label in &labels {
        if label.is_empty() {
            // Keep standard button IDs stable when optional buttons are absent.
            append(
                DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_DISABLED_FLAG,
                (0, 0, 0, 0),
                &[],
            );
        } else {
            let button_width =
                ppc_text_width_bytes(PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM, 0, label)
                    .saturating_add(24)
                    .max(60);
            append(
                DIALOG_ITEM_BUTTON,
                (button_top, right - button_width, button_top + 20, right),
                label,
            );
            right -= button_width + 12;
        }
    }
    append(
        DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_DISABLED_FLAG,
        (20, text_left, 20 + primary_height, width - 20),
        &primary,
    );
    append(
        DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_DISABLED_FLAG,
        (28 + primary_height, text_left, text_bottom, width - 20),
        &secondary,
    );
    if let Some(icon_id) = crate::dialog_manager::alert_icon_id(cpu.gpr[3] as u16) {
        append(
            DIALOG_ITEM_ICON | DIALOG_ITEM_DISABLED_FLAG,
            (20, 20, 20 + DIALOG_ICON_SIZE, 20 + DIALOG_ICON_SIZE),
            &icon_id.to_be_bytes(),
        );
    }
    items[..2].copy_from_slice(&(count - 1).to_be_bytes());
    let left = (ppc_main_screen_width() as i16 - width).max(0) / 2;
    let top = (ppc_main_screen_height() as i16 - height).max(0) / 3;
    Ok((
        (top, left, top + height, left + width),
        items,
        default_item,
        cancel_item,
        position,
        if movable { 5 } else { 1 },
    ))
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
    standard: bool,
    param_text: &SharedProcessDialogText,
) -> u32 {
    let (bounds, ditl_bytes, default_item, cancel_item, position, proc_id) = if standard {
        match ppc_standard_alert_template(cpu, memory) {
            Ok(template) => template,
            Err(error) => {
                *last_resource_error = error;
                return 0;
            }
        }
    } else {
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
        let Some(template) = crate::dialog_manager::parse_alert_template(alert) else {
            *last_resource_error = PPC_PARAM_ERR;
            return 0;
        };
        let current_stage = memory
            .read_u16_be(crate::memory::globals::addr::ALERT_STAGE)
            .unwrap_or(crate::dialog_manager::INITIAL_ALERT_STAGE);
        let eval = crate::dialog_manager::evaluate_alert_invocation(
            alert_id,
            template.stages,
            current_stage,
        );
        let _ = memory.write_u16_be(
            crate::memory::globals::addr::ALERT_STAGE,
            eval.next_stage(),
        );
        let _ = memory.write_u16_be(
            crate::memory::globals::addr::ANUMBER,
            eval.anumber(),
        );
        let Some(default_item) = eval.default_item() else {
            *last_resource_error = PPC_NO_ERR;
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
        (
            template.bounds,
            ditl_bytes,
            default_item as u16,
            0u16,
            template.position,
            1u32,
        )
    };
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
    dialog_cpu.gpr[7] = proc_id;
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
        let init = evaluate_alert_dialog_record_init(
            items_handle,
            alert_id,
            default_item as i16,
            cancel_item as i16,
        );
        let _ = memory.write_u16_be(dialog + DIALOG_RESOURCE_ID_OFFSET, init.alert_id() as u16);
        let _ = memory.write_u16_be(dialog + DIALOG_DEFAULT_ITEM_OFFSET, init.default_item() as u16);
        let _ = memory.write_u16_be(dialog + DIALOG_ALERT_HIT_OFFSET, init.alert_hit() as u16);
        let _ = memory.write_u16_be(dialog + DIALOG_CANCEL_ITEM_OFFSET, init.cancel_item() as u16);
        if standard {
            let _ = memory.write_u32_be(dialog + DIALOG_STANDARD_ALERT_OUTPUT_OFFSET, cpu.gpr[7]);
            let _ = memory.write_u32_be(dialog + DIALOG_STANDARD_ALERT_STACK_OFFSET, cpu.gpr[1]);
        }
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
    param_text: &SharedProcessDialogText,
    params: GetNewDialogParameters,
) -> u32 {
    let dialog_id = params.dialog_id();
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
    new_cpu.gpr[3] = params.storage();
    new_cpu.gpr[4] = scratch;
    new_cpu.gpr[5] = scratch + 8;
    new_cpu.gpr[6] = u32::from(template.visible);
    new_cpu.gpr[7] = template.proc_id as u16 as u32;
    new_cpu.gpr[8] = params.behind();
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
        let _ = memory.write_u16_be(dialog + DIALOG_RESOURCE_ID_OFFSET, dialog_id as u16);
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
    let proc_id = cpu.gpr[7] as u16 as i16;
    let behind = cpu.gpr[8];
    let go_away = cpu.gpr[9] != 0;
    let ref_con = cpu.gpr[10];
    let items = ppc_parameter_area_slot_addr(cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
        .and_then(|addr| memory.read_u32_be(addr))
        .unwrap_or(0);

    let Ok(params) = crate::dialog_manager::evaluate_new_dialog_parameters(
        requested_storage,
        bounds_ptr,
        title_ptr,
        visible,
        proc_id,
        behind,
        go_away,
        ref_con,
        items,
        true,
    ) else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };

    let storage = match params.storage_policy() {
        crate::dialog_manager::DialogStoragePolicy::AllocateNew => {
            let storage = process_memory_manager.new_native_ptr(memory, DIALOG_RECORD_SIZE, true);
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            storage
        }
        crate::dialog_manager::DialogStoragePolicy::CallerSupplied(requested) => {
            if ppc_memory_can_write_bytes(memory, requested, DIALOG_RECORD_SIZE) {
                let _ = memory.write_bytes(requested, &vec![0; DIALOG_RECORD_SIZE as usize]);
                requested
            } else {
                0
            }
        }
    };
    if storage == 0 {
        *last_mem_error = match params.storage_policy() {
            crate::dialog_manager::DialogStoragePolicy::AllocateNew => PPC_MEM_FULL_ERR,
            crate::dialog_manager::DialogStoragePolicy::CallerSupplied(_) => PPC_PARAM_ERR,
        };
        return 0;
    }

    let mut window_cpu = cpu.clone();
    window_cpu.gpr[3] = storage;
    window_cpu.gpr[4] = params.bounds_ptr();
    window_cpu.gpr[5] = params.title_ptr();
    window_cpu.gpr[6] = u32::from(params.is_visible());
    window_cpu.gpr[7] = params.proc_id() as u16 as u32;
    window_cpu.gpr[8] = params.behind();
    window_cpu.gpr[9] = u32::from(params.go_away());
    window_cpu.gpr[10] = params.ref_con();
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

    let title = ppc_read_pstring_bytes(memory, params.title_ptr()).unwrap_or_default();
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
    let init = crate::dialog_manager::evaluate_dialog_record_init(params.items());
    if title_handle == 0
        || memory
            .write_u16_be(
                dialog + PPC_CWINDOW_WINDOW_KIND_OFFSET,
                init.window_kind(),
            )
            .is_none()
        || memory.write_u32_be(dialog + 134, title_handle).is_none()
        || memory
            .write_u32_be(dialog + DIALOG_ITEMS_OFFSET, init.items_handle())
            .is_none()
        || memory
            .write_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET, init.text_handle())
            .is_none()
        || memory
            .write_u16_be(
                dialog + DIALOG_EDIT_FIELD_OFFSET,
                init.edit_field() as u16,
            )
            .is_none()
        || memory
            .write_u16_be(
                dialog + DIALOG_EDIT_OPEN_OFFSET,
                init.edit_open() as u16,
            )
            .is_none()
        || memory
            .write_u16_be(
                dialog + DIALOG_DEFAULT_ITEM_OFFSET,
                init.default_item() as u16,
            )
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
    param_text: &SharedProcessDialogText,
    dialog: u32,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> bool {
    let param_text = param_text.snapshot();
    let Some(items_handle) = memory
        .read_u32_be(dialog.wrapping_add(DIALOG_ITEMS_OFFSET))
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
        let base_type = dialog_item_base_type(item.item_type);
        let mut missing_resource = false;
        let item_handle = match base_type {
            DIALOG_ITEM_BUTTON | DIALOG_ITEM_CHECKBOX | DIALOG_ITEM_RADIO => {
                let proc_id =
                    crate::dialog_manager::dialog_item_control_proc_id(base_type).unwrap_or(0);
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
            DIALOG_ITEM_RESOURCE_CONTROL => {
                let resource_id = item.resource_id().unwrap_or(0);
                let resource_type = dialog_item_resource_type_u32(base_type).unwrap_or(0);
                if let Some(index) = ppc_vfs_resource_index(
                    vfs_resources,
                    current_resource_refnum,
                    resource_type,
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
            DIALOG_ITEM_STATIC_TEXT => {
                // Macintosh Toolbox Essentials (1992), pp. 6-129--6-130:
                // ParamText replaces ^0..^3 in static-text items of every
                // subsequently created dialog or alert.
                let text = ppc_apply_param_text(&item.payload, &param_text);
                ppc_process_alloc_handle_with_bytes(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    handles,
                    &text,
                )
            }
            DIALOG_ITEM_EDIT_TEXT => ppc_process_alloc_handle_with_bytes(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                handles,
                &item.payload,
            ),
            DIALOG_ITEM_ICON | DIALOG_ITEM_PICTURE => {
                let resource_id = item.resource_id().unwrap_or(0);
                let resource_type = dialog_item_resource_type_u32(base_type).unwrap_or(0);
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
                DIALOG_ITEM_BUTTON
                    | DIALOG_ITEM_CHECKBOX
                    | DIALOG_ITEM_RADIO
                    | DIALOG_ITEM_RESOURCE_CONTROL
                    | DIALOG_ITEM_STATIC_TEXT
                    | DIALOG_ITEM_EDIT_TEXT
                    | DIALOG_ITEM_ICON
                    | DIALOG_ITEM_PICTURE
            )
        {
            if *last_mem_error == PPC_NO_ERR && *last_resource_error == PPC_NO_ERR {
                *last_mem_error = PPC_MEM_FULL_ERR;
            }
            return false;
        }
        if base_type == DIALOG_ITEM_RESOURCE_CONTROL {
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
        if base_type == DIALOG_ITEM_EDIT_TEXT && first_edit.is_none() {
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
                .write_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET, te_handle)
                .is_none()
            || memory
                .write_u16_be(
                    dialog + DIALOG_EDIT_FIELD_OFFSET,
                    item_index.min(i16::MAX as usize) as u16,
                )
                .is_none()
            || memory
                .write_u16_be(dialog + DIALOG_EDIT_OPEN_OFFSET, 1)
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
    params: SelectDialogItemTextParameters,
    tick_count: u32,
    text_mode: i16,
    text_size: i16,
    fore_color: PpcRgbColor,
) {
    let Some(item_index) = params.item_index() else {
        return;
    };
    let Some(item) = ppc_dialog_items_for_dialog(memory, handles, params.dialog_ptr())
        .and_then(|items| items.get(item_index).cloned())
        .filter(|item| item.is_edit_text())
    else {
        return;
    };
    let current_field = memory
        .read_u16_be(params.dialog_ptr() + DIALOG_EDIT_FIELD_OFFSET)
        .unwrap_or(u16::MAX) as usize;
    let mut te_handle = memory
        .read_u32_be(params.dialog_ptr() + DIALOG_TEXT_HANDLE_OFFSET)
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
            params.dialog_ptr(),
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
        let _ = memory.write_u32_be(params.dialog_ptr() + DIALOG_TEXT_HANDLE_OFFSET, te_handle);
        let _ = memory.write_u16_be(
            params.dialog_ptr() + DIALOG_EDIT_FIELD_OFFSET,
            item_index.min(i16::MAX as usize) as u16,
        );
        let _ = memory.write_u16_be(params.dialog_ptr() + DIALOG_EDIT_OPEN_OFFSET, 1);
    }
    let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
        return;
    };
    let length = memory
        .read_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET)
        .unwrap_or(0) as usize;
    let Some(eval) = params.evaluate_selection(item.is_edit_text(), length) else {
        return;
    };
    let _ = memory.write_u16_be(params.dialog_ptr() + DIALOG_EDIT_FIELD_OFFSET, eval.edit_field);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, eval.sel_start);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, eval.sel_end);
    let _ = memory.write_u16_be(te_ptr + PPC_TE_ACTIVE_OFFSET, 1);
    let _ = memory.write_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET, tick_count);
}

fn ppc_get_dialog_item(cpu: &mut PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) {
    let dialog = cpu.gpr[3];
    let item_number = cpu.gpr[4] as u16 as usize;
    let item_type_ptr = cpu.gpr[5];
    let item_handle_ptr = cpu.gpr[6];
    let item_rect_ptr = cpu.gpr[7];
    let can_write_type = ppc_optional_output_can_write(memory, item_type_ptr, 2);
    let can_write_handle = ppc_optional_output_can_write(memory, item_handle_ptr, 4);
    let can_write_rect = ppc_optional_output_can_write(memory, item_rect_ptr, 8);
    let Some(params) = evaluate_get_dialog_item_parameters(
        dialog,
        item_number,
        item_type_ptr,
        item_handle_ptr,
        item_rect_ptr,
        can_write_type,
        can_write_handle,
        can_write_rect,
    ) else {
        return;
    };
    let header = ppc_dialog_items_for_dialog(memory, handles, params.dialog_ptr())
        .map(|items| evaluate_get_dialog_item(&items, params.item_number(), |item| item.header()))
        .unwrap_or(DialogItemHeader::ZERO);

    if params.item_type_ptr() != 0 {
        let _ = memory.write_u16_be(params.item_type_ptr(), header.item_type);
    }
    if params.item_handle_ptr() != 0 {
        let _ = memory.write_u32_be(params.item_handle_ptr(), header.handle);
    }
    if params.item_rect_ptr() != 0 {
        let _ = ppc_write_rect(
            memory,
            params.item_rect_ptr(),
            header.rect.0,
            header.rect.1,
            header.rect.2,
            header.rect.3,
        );
    }
}

fn ppc_set_dialog_item(cpu: &PpcCpu, memory: &mut PpcSectionMem, handles: &[PpcHandleRecord]) {
    let dialog = cpu.gpr[3];
    let item_number = cpu.gpr[4] as u16 as usize;
    let item_type = cpu.gpr[5] as u16;
    let item_handle = cpu.gpr[6];
    let rect_ptr = cpu.gpr[7];
    let Some(rect) = ppc_read_rect(memory, rect_ptr) else {
        return;
    };
    let Some(params) = crate::dialog_manager::evaluate_set_dialog_item_parameters(
        dialog,
        item_number,
        item_type,
        item_handle,
        rect,
    ) else {
        return;
    };
    let Some(items_handle) = memory.read_u32_be(params.dialog_ptr().wrapping_add(DIALOG_ITEMS_OFFSET)) else {
        return;
    };
    let Some(items_ptr) = memory.read_u32_be(items_handle).filter(|ptr| *ptr != 0) else {
        return;
    };
    let Some(item) = ppc_handle_bytes(memory, handles, items_handle)
        .and_then(|bytes| ppc_parse_dialog_items(&bytes))
        .and_then(|items| {
            crate::dialog_manager::get_item_at_1_indexed(&items, params.item_number()).cloned()
        })
    else {
        return;
    };
    let item_addr = items_ptr.wrapping_add(item.item_offset as u32);
    // Macintosh Toolbox Essentials (1992), pp. 6-120--6-123: mutate the
    // live DITL item's handle, Rect, and type in place without drawing it.
    let _ = memory.write_u32_be(item_addr + crate::dialog_manager::DITL_ITEM_HANDLE_OFFSET, params.item_handle());
    let _ = ppc_write_rect(
        memory,
        item_addr + crate::dialog_manager::DITL_ITEM_RECT_OFFSET,
        params.rect().0,
        params.rect().1,
        params.rect().2,
        params.rect().3,
    );
    let _ = memory.write_u8(item_addr + crate::dialog_manager::DITL_ITEM_TYPE_OFFSET, params.item_type());
}

pub(super) fn ppc_dialog_items_for_dialog(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    dialog: u32,
) -> Option<Vec<PpcDialogItemView>> {
    let items_handle = memory.read_u32_be(dialog.checked_add(DIALOG_ITEMS_OFFSET)?)?;
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

pub(super) fn ppc_standard_filter_proc(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    // Inside Macintosh Volume I (1985), p. I-415;
    // Macintosh Toolbox Essentials (1992), p. 6-144;
    // Apple Dialog Manager Reference (2007), p. 43:
    // the standard filter returns TRUE and sets itemHit to the default item for Return or Enter.
    let dialog = cpu.gpr[3];
    let event_ptr = cpu.gpr[4];
    let item_hit_ptr = cpu.gpr[5];
    let Some(params) = evaluate_std_filter_proc_parameters(dialog, event_ptr, item_hit_ptr) else {
        return 0;
    };
    let Some(event) = ppc_read_dialog_event(memory, params.event_ptr()) else {
        return 0;
    };
    let default_item = params
        .dialog_ptr()
        .checked_add(DIALOG_DEFAULT_ITEM_OFFSET)
        .and_then(|address| memory.read_u16_be(address))
        .map(|item| item as i16);
    let eval = evaluate_std_filter_proc_event(
        event.what,
        event.message,
        event.modifiers,
        default_item,
    );
    if let Some(item_hit) = eval.item_hit() {
        if memory
            .write_u16_be(params.item_hit_ptr(), item_hit as u16)
            .is_none()
        {
            return 0;
        }
    }
    eval.boolean_result()
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
    let front_dialog = gworlds.iter().rev().find_map(|record| {
        (memory.read_u16_be(record.port.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
            == Some(crate::dialog_manager::DIALOG_WINDOW_KIND)
            && ppc_window_is_visible(memory, record.port))
        .then_some(record.port)
    });
    dialog_target_for_event(
        what,
        message,
        |target| {
            memory.read_u16_be(target.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
                == Some(crate::dialog_manager::DIALOG_WINDOW_KIND)
        },
        front_dialog,
    )
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
    dialog_rect_to_global(bounds, rect)
}

fn ppc_dialog_draw_callbacks(
    memory: &mut PpcSectionMem,
    items: &[PpcDialogItemView],
    bounds: (i16, i16, i16, i16),
    default_rtoc: u32,
) -> Vec<(PpcCallbackTarget, u32)> {
    crate::dialog_manager::evaluate_dialog_user_item_numbers(
        items.iter().map(|item| (item.has_user_proc(), item.rect)),
        bounds,
        None,
    )
    .into_iter()
    .filter_map(|item_number| {
        let item = &items[item_number - 1];
        let target = ppc_resolve_callback_target(memory, item.handle, default_rtoc, None)?;
        memory.read_u32_be(target.entry)?;
        Some((target, u32::try_from(item_number).unwrap_or(u32::MAX)))
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

pub(super) fn ppc_frame_front_round_rect(
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
    let base_type = dialog_item_base_type(item.item_type);
    if matches!(
        base_type,
        DIALOG_ITEM_BUTTON | DIALOG_ITEM_CHECKBOX | DIALOG_ITEM_RADIO
    ) {
        return memory
            .read_u32_be(item.handle)
            .filter(|ptr| *ptr != 0)
            .and_then(|ptr| ppc_read_pstring_bytes(memory, ptr + PPC_CONTROL_TITLE_OFFSET))
            .unwrap_or_else(|| item.payload.clone());
    }
    let handle_bytes = ppc_handle_bytes(memory, handles, item.handle);
    extract_dialog_item_text_bytes(base_type, handle_bytes.as_deref(), &item.payload).to_vec()
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
        .read_u16_be(dialog.wrapping_add(DIALOG_DEFAULT_ITEM_OFFSET))
        .unwrap_or(1) as usize;
    for (index, item) in items.iter().enumerate() {
        // Imaging With QuickDraw (1994), pp. 2-20--2-21: drawing is clipped
        // to the port's visible region. Some applications deliberately keep
        // inactive DITL items beyond the DialogRecord's portRect; the native
        // dialog renderer targets the screen directly, so reject those items
        // here rather than letting their placeholder text escape the window.
        if !crate::dialog_manager::dialog_item_intersects_bounds(bounds, item.rect) {
            continue;
        }
        let rect = ppc_dialog_rect_to_global(bounds, item.rect);
        let base_type = dialog_item_base_type(item.item_type);
        match base_type {
            DIALOG_ITEM_BUTTON | DIALOG_ITEM_CHECKBOX | DIALOG_ITEM_RADIO => {
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
                if ppc_ui_theme(gworlds) == UiThemeId::ClassicSystem7 {
                    if let Some(outline) = evaluate_dialog_item_default_button_outline(
                        (index + 1) as i16,
                        default_item as i16,
                        item.item_type,
                        rect,
                    ) {
                        let _ = ppc_frame_front_round_rect(
                            memory,
                            front,
                            outline.outer_rect(),
                            outline.oval(),
                            outline.thickness(),
                            ppc_theme_rgb(palette.frame_dark),
                        );
                    }
                }
            }
            DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_EDIT_TEXT => {
                let text = ppc_dialog_item_title(memory, handles, item);
                if base_type == DIALOG_ITEM_EDIT_TEXT {
                    // Text (1993), p. 2-88: TEUpdate redraws within the view
                    // rectangle. Clear this manager-owned edit item before
                    // repainting so focus changes cannot retain a previous
                    // selection or duplicate glyphs in the field.
                    let _ = ppc_fill_front_rect(
                        memory,
                        front,
                        rect,
                        ppc_theme_rgb(palette.window_background),
                    );
                    let outer = edit_text_frame_rect(rect);
                    let _ = ppc_frame_front_rect(
                        memory,
                        front,
                        outer,
                        ppc_theme_rgb(palette.frame_dark),
                        1,
                    );
                }
                let selected = if base_type == DIALOG_ITEM_EDIT_TEXT
                    && memory
                        .read_u16_be(dialog + DIALOG_EDIT_FIELD_OFFSET)
                        .is_some_and(|field| usize::from(field) == index)
                {
                    memory
                        .read_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET)
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
                let text_rect =
                    if matches!(base_type, DIALOG_ITEM_STATIC_TEXT | DIALOG_ITEM_EDIT_TEXT) {
                        dialog_text_rect(rect)
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
            DIALOG_ITEM_PICTURE => {
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
            DIALOG_ITEM_ICON => {
                let _ =
                    ppc_frame_front_rect(memory, front, rect, ppc_theme_rgb(palette.frame_dark), 1);
            }
            DIALOG_ITEM_RESOURCE_CONTROL => {
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
    memory: &mut PpcSectionMem,
    controls: &[PpcControlRecord],
    items: &[PpcDialogItemView],
    bounds: (i16, i16, i16, i16),
    where_v: i16,
    where_h: i16,
) -> Option<u16> {
    // A control item with a live Control Manager record is returned only when
    // its CDEF reports a real part under the point. FindControl returns 0 for
    // an inactive control (Inside Macintosh: Macintosh Toolbox Essentials
    // (1992), p. 5-89); group boxes report kControlNoPart for their body (see
    // ppc_control_part_at_point; unverified). Both Some(0) and None skip the
    // item and let the click fall through to whatever it encloses. Items
    // without a live record still fall back to their DITL rect.
    //
    // Dialog-owned control records keep contrlRect in dialog-local
    // coordinates, so the global point is rebased before asking the CDEF.
    // FindDialogItem already passes an empty origin and therefore stays local.
    let (local_v, local_h) = global_to_dialog_local_point(bounds, where_v, where_h);
    let hit_idx = find_dialog_item_hit(
        items.iter().map(|item| (item.rect, item.item_type)),
        local_v,
        local_h,
        true,
        |index| {
            let item = &items[index];
            if item.handle != 0 && controls.iter().any(|record| record.handle == item.handle) {
                ppc_control_part_at_point(memory, controls, item.handle, local_v, local_h)
                    .is_some_and(|part| part != 0)
            } else {
                true
            }
        },
    )?;
    u16::try_from(hit_idx + 1).ok()
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
    configured_cancel: Option<i16>,
) -> Option<u16> {
    evaluate_dialog_cancel_item(
        configured_cancel,
        items
            .iter()
            .map(|item| (item.item_type, ppc_dialog_item_title(memory, handles, item))),
    )
    .to_u16()
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
    let filter_proc = cpu.gpr[3];
    let item_hit_ptr = cpu.gpr[4];
    let can_write = ppc_memory_can_write_bytes(memory, item_hit_ptr, 2);
    let Some(params) = evaluate_modal_dialog_parameters(filter_proc, item_hit_ptr, can_write) else {
        return PpcImportAction::ReturnPreserve;
    };
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
            let hit = ppc_dialog_item_at_global_point(
                memory,
                controls,
                &items,
                bounds,
                event.where_v,
                event.where_h,
            )?;
            let item = items.get(usize::from(hit).checked_sub(1)?)?;
            if item.is_edit_text() {
                let item_index = usize::from(hit).saturating_sub(1);
                let current_field = memory
                    .read_u16_be(dialog + DIALOG_EDIT_FIELD_OFFSET)
                    .unwrap_or(u16::MAX) as usize;
                if current_field != item_index {
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
                        SelectDialogItemTextParameters::new(
                            dialog,
                            usize::from(hit),
                            0,
                            i16::MAX,
                        ),
                        event.when,
                        PPC_QD_TEXT_MODE_SRC_OR,
                        PPC_QD_TEXT_SIZE_SYSTEM,
                        fore_color,
                    );
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
                let te_handle = memory
                    .read_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET)
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
                if item.is_resource_control()
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
            let decision = crate::dialog_manager::evaluate_modal_dialog_key(
                event.what,
                event.message,
                event.modifiers,
            );
            let default_item = memory
                .read_u16_be(dialog + DIALOG_DEFAULT_ITEM_OFFSET)
                .map(|item| item as i16);
            let filter_eval = evaluate_std_filter_proc(decision, default_item);
            if let Some(item_hit) = filter_eval.item_hit() {
                Some(item_hit as u16)
            } else if let Some(cancel_hit) = evaluate_dialog_filter_cancel(
                decision,
                if decision == crate::dialog_manager::DialogFilterDecision::TriggerCancelButton {
                    let configured_cancel = memory
                        .read_u16_be(dialog + DIALOG_CANCEL_ITEM_OFFSET)
                        .filter(|item| *item != 0)
                        .map(|item| item as i16);
                    ppc_dialog_cancel_item(memory, handles, &items, configured_cancel)
                        .map(|item| item as i16)
                } else {
                    None
                },
            ) {
                Some(cancel_hit as u16)
            } else if character.eq_ignore_ascii_case(&b'a') && event.modifiers & 0x0100 != 0 {
                let te_handle = memory
                    .read_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET)
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
                    .read_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET)
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
        let _ = memory.write_u16_be(params.item_hit_ptr(), hit);
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] ModalDialog dialog=${dialog:08X} filter=${:08X} -> item {}",
                params.filter_proc(), hit
            );
        }
        PpcImportAction::ReturnPreserve
    } else {
        if handled_edit_event && ppc_hle_trace_enabled() {
            let text = memory
                .read_u32_be(dialog + DIALOG_TEXT_HANDLE_OFFSET)
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
