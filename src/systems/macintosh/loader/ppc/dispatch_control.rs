//! Typed Control Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcControlDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) controls: &'a mut Vec<PpcControlRecord>,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) current_gworld: u32,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) input: PpcInputSnapshot,
    pub(super) vfs_resources: &'a mut [PpcVfsResourceRecord],
    pub(super) current_resource_refnum: i16,
    pub(super) last_resource_error: &'a mut i16,
}

pub(super) fn dispatch_control_import(
    context: PpcControlDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcControlDispatchContext {
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
        current_gworld,
        toolbox_startup,
        input,
        vfs_resources,
        current_resource_refnum,
        last_resource_error,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::DrawControls => {
            let window = cpu.gpr[3];
            if memory.read_u16_be(window + PPC_CWINDOW_WINDOW_KIND_OFFSET) == Some(2) {
                // Dialog controls are represented by the live DITL, whose
                // renderer also translates dialog-local item coordinates.
                let _ = ppc_draw_dialog(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    screen_clut,
                    vfs_resources,
                    current_resource_refnum,
                    window,
                );
            } else {
                let _ = ppc_draw_window_controls(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    window,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::UpdateControls => {
            let window = cpu.gpr[3];
            let update_rgn = cpu.gpr[4];
            let _ = ppc_update_window_controls(
                memory,
                handles,
                controls,
                gworlds,
                vfs_resources,
                current_resource_refnum,
                window,
                update_rgn,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetControlTitle => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_set_control_title(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SetControlValue => {
            let control_handle = cpu.gpr[3];
            if let Some(control) = memory
                .read_u32_be(control_handle)
                .filter(|control| *control != 0)
            {
                let _ = memory.write_u16_be(
                    control.wrapping_add(PPC_CONTROL_VALUE_OFFSET),
                    cpu.gpr[4] as u16,
                );
                // Popup CDEF records repurpose contrlMin for the menu ID and
                // contrlMax for the title width (MTE, pp. 5-25--5-27), so
                // applying the ordinary range clamp would turn a valid item
                // value into a menu ID/title-width value.
                let is_popup = controls.iter().any(|record| {
                    record.handle == control_handle
                        && (1008..=1023).contains(&(record.proc_id & 0x0fff))
                });
                if !is_popup {
                    ppc_clamp_control_value(memory, control);
                }
                let _ = ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    control_handle,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HiliteControl => {
            let control_handle = cpu.gpr[3];
            if let Some(control) = memory
                .read_u32_be(control_handle)
                .filter(|control| *control != 0)
            {
                // Macintosh Toolbox Essentials (1992), p. 5-98: byte 17 of
                // ControlRecord is contrlHilite; 255 means inactive.
                let _ = memory.write_u8(control + 17, cpu.gpr[4] as u8);
                let owner = memory.read_u32_be(control + 4).unwrap_or(0);
                let dialog = if owner != 0 { owner } else { current_gworld };
                if !ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    control_handle,
                ) {
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
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LegacyControl(operation) => ppc_dispatch_legacy_control(
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
            toolbox_startup,
            input,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        ),
        _ => None,
    }
}

pub(super) fn ppc_set_control_title(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let control_handle = cpu.gpr[3];
    let title_ptr = cpu.gpr[4];
    let title = ppc_read_pstring_bytes(memory, title_ptr).unwrap_or_default();
    if control_handle == 0 || title_ptr == 0 {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    }
    let result = ppc_allocator_view_resize_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        control_handle,
        PPC_CONTROL_RECORD_SIZE,
    );
    if result != PPC_NO_ERR {
        *last_mem_error = result;
        return;
    }
    let Some(control) = memory
        .read_u32_be(control_handle)
        .filter(|control| *control != 0)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    };
    let wrote = ppc_write_pstring_bytes(
        memory,
        control.wrapping_add(PPC_CONTROL_TITLE_OFFSET),
        &title,
    );
    // Macintosh Toolbox Essentials (1992), pp. 5-73 and 5-96: a control's
    // Str255 title begins at byte 40 of its relocatable ControlRecord.
    *last_mem_error = if wrote { PPC_NO_ERR } else { PPC_PARAM_ERR };
}

#[allow(clippy::too_many_arguments)]
fn ppc_dispatch_popup_track_control(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    startup: &mut PpcToolboxStartupState,
    input: PpcInputSnapshot,
    control_handle: u32,
    start_v: i16,
    start_h: i16,
) -> Option<PpcImportAction> {
    let record = controls.iter().find(|record| {
        record.handle == control_handle && (1008..=1023).contains(&(record.proc_id & 0x0fff))
    })?;
    // TrackControl's CDEF must not begin a retained popup session for an
    // invisible, inactive, or missed control. Keep this guard in the helper
    // as well as at the dispatcher call site because direct PPC callers can
    // enter this adapter without first running FindControl. Macintosh
    // Toolbox Essentials (1992), pp. 5-67--5-69.
    ppc_control_part_at_point(memory, controls, control_handle, start_v, start_h)?;
    let control = ppc_control_ptr(memory, control_handle)?;
    let owner = memory.read_u32_be(control + PPC_CONTROL_OWNER_OFFSET)?;
    let surface = ppc_live_quickdraw_surface(memory, gworlds, owner)?;
    let menu_id = record.popup_menu_id;
    let current_menu_list = ppc_current_menu_list(memory);
    let menu_handle = ppc_get_menu_handle(memory, current_menu_list, menu_id);
    if menu_handle == 0 {
        return Some(PpcImportAction::Return(0));
    }

    // TrackControl receives a window-local start point, while the Menu
    // Manager's PopUpMenuSelect contract is expressed in global screen
    // coordinates. Reuse the standard retained popup session so CDEF-backed
    // controls get the same disabled/separator hit testing, save-under, and
    // repaint behavior as a direct PopUpMenuSelect call.
    let (control_top, control_left, _, _) =
        ppc_read_rect(memory, control + PPC_CONTROL_RECT_OFFSET)?;
    let (global_h, global_v) =
        surface.local_point((i32::from(control_left), i32::from(control_top)));
    let mut popup_cpu = cpu.clone();
    popup_cpu.gpr[3] = menu_handle;
    popup_cpu.gpr[4] = u32::from(ppc_i32_to_i16_saturating(global_v) as u16);
    popup_cpu.gpr[5] = u32::from(ppc_i32_to_i16_saturating(global_h) as u16);
    popup_cpu.gpr[6] = memory
        .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
        .unwrap_or(1) as u32;
    let menu_color_bytes = ppc_menu_color_table_bytes(memory, handles);
    let menu_colors = MenuColorTable::new(&menu_color_bytes);
    let action = ppc_dispatch_pop_up_menu_select(
        &popup_cpu,
        memory,
        gworlds,
        screen_clut,
        menu_colors,
        startup,
        input,
        vfs_resources,
        current_resource_refnum,
    );
    Some(match action {
        PpcImportAction::Return(result) => {
            let item = result as u16 as i16;
            if item > 0 && ppc_menu_item_is_selectable(memory, menu_handle, item) {
                let _ = memory.write_u16_be(control + PPC_CONTROL_VALUE_OFFSET, item as u16);
                let _ = ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    control_handle,
                );
                PpcImportAction::Return(ppc_i16_result(10))
            } else {
                PpcImportAction::Return(0)
            }
        }
        PpcImportAction::ReturnWithExtraCycles(result, extra_cycles) => {
            let item = result as u16 as i16;
            if item > 0 && ppc_menu_item_is_selectable(memory, menu_handle, item) {
                let _ = memory.write_u16_be(control + PPC_CONTROL_VALUE_OFFSET, item as u16);
                let _ = ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    control_handle,
                );
                PpcImportAction::ReturnWithExtraCycles(ppc_i16_result(10), extra_cycles)
            } else {
                PpcImportAction::ReturnWithExtraCycles(0, extra_cycles)
            }
        }
        action => action,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcLegacyControlOperation {
    AdvanceKeyboardFocus,
    AutoEmbedControl,
    ChangeControlPropertyAttributes,
    ClearKeyboardFocus,
    CountSubControls,
    CreateRootControl,
    DisableControl,
    DisposeControl,
    DragControl,
    DrawControlInCurrentPort,
    DrawOneControl,
    EmbedControl,
    EnableControl,
    FindControl,
    FindControlUnderMouse,
    GetBestControlRect,
    GetControlAction,
    GetControlBounds,
    GetControlByID,
    GetControlClickActivation,
    GetControlColorProc,
    GetControlCommandID,
    GetControlData,
    GetControlDataSize,
    GetControlFeatures,
    GetControlFocusPart,
    GetControlHilite,
    GetControlID,
    GetControlKind,
    GetControlMaximum,
    GetControlMinimum,
    GetControlOwner,
    GetControlProperty,
    GetControlPropertyAttributes,
    GetControlPropertySize,
    GetControlReference,
    GetControlRegion,
    GetControlTitle,
    GetControlValue,
    GetControlVariant,
    GetIndexedSubControl,
    GetKeyboardFocus,
    GetNewControl,
    GetRootControl,
    GetSuperControl,
    HandleControlClick,
    HandleControlKey,
    HideControl,
    IdleControls,
    IsControlDragTrackingEnabled,
    IsControlEnabled,
    IsControlHilited,
    IsControlVisible,
    IsValidControlHandle,
    KillControls,
    MoveControl,
    NewControl,
    RemoveControlProperty,
    ReverseKeyboardFocus,
    ScrollControlValues,
    SendControlMessage,
    SetControlAction,
    SetControlBounds,
    SetControlColorProc,
    SetControlCommandID,
    SetControlData,
    SetControlDragTrackingEnabled,
    SetControlFocusPart,
    SetControlID,
    SetControlMaximum,
    SetControlMinimum,
    SetControlProperty,
    SetControlReference,
    SetControlSupervisor,
    SetControlVisibility,
    SetKeyboardFocus,
    SetUpControlBackground,
    ShowControl,
    SizeControl,
    TestControl,
    TrackControl,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_dispatch_legacy_control(
    operation: PpcLegacyControlOperation,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
    input: PpcInputSnapshot,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> Option<PpcImportAction> {
    match operation {
        PpcLegacyControlOperation::NewControl => {
            let ref_con = ppc_parameter_area_slot_addr(cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT)
                .and_then(|address| memory.read_u32_be(address))
                .unwrap_or(0);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let handle = ppc_new_control_values(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                controls,
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[6] != 0,
                cpu.gpr[7] as u16 as i16,
                cpu.gpr[8] as u16 as i16,
                cpu.gpr[9] as u16 as i16,
                cpu.gpr[10] as u16 as i16,
                ref_con,
            );
            ppc_initialize_popup_control(
                memory,
                controls,
                vfs_resources,
                current_resource_refnum,
                handle,
            );
            if handle != 0 && cpu.gpr[6] != 0 {
                let _ = ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    handle,
                );
            }
            Some(PpcImportAction::Return(handle))
        }
        PpcLegacyControlOperation::GetNewControl => {
            let resource_id = cpu.gpr[3] as u16 as i16;
            let owner = cpu.gpr[4];
            let Some(index) = ppc_vfs_resource_index(
                vfs_resources,
                current_resource_refnum,
                u32::from_be_bytes(*b"CNTL"),
                resource_id,
                false,
            ) else {
                *last_resource_error = PPC_RES_NOT_FOUND_ERR;
                return Some(PpcImportAction::Return(0));
            };
            let bytes = vfs_resources[index].data.clone();
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let Some((bounds, title_ptr)) = ppc_materialize_control_resource_parameters(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                &bytes,
            ) else {
                *last_resource_error = PPC_PARAM_ERR;
                return Some(PpcImportAction::Return(0));
            };
            let handle = ppc_new_control_values(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                controls,
                owner,
                bounds,
                title_ptr,
                bytes[10] != 0,
                i16::from_be_bytes([bytes[8], bytes[9]]),
                i16::from_be_bytes([bytes[14], bytes[15]]),
                i16::from_be_bytes([bytes[12], bytes[13]]),
                i16::from_be_bytes([bytes[16], bytes[17]]),
                u32::from_be_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]),
            );
            *last_resource_error = if handle == 0 {
                PPC_PARAM_ERR
            } else {
                PPC_NO_ERR
            };
            ppc_initialize_popup_control(
                memory,
                controls,
                vfs_resources,
                current_resource_refnum,
                handle,
            );
            if handle != 0 && bytes[10] != 0 {
                let _ = ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    handle,
                );
            }
            Some(PpcImportAction::Return(handle))
        }
        PpcLegacyControlOperation::EmbedControl => {
            let control = cpu.gpr[3];
            let container = cpu.gpr[4];
            let result = ppc_embed_control(controls, control, container);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlSupervisor => {
            let control = cpu.gpr[3];
            let boss = cpu.gpr[4];
            let result = ppc_embed_control(controls, control, boss);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::AutoEmbedControl => {
            let control = cpu.gpr[3];
            let window = cpu.gpr[4];
            let root = ppc_window_root_control(memory, controls, window);
            let result = if let Some(root_handle) = root {
                ppc_embed_control(controls, control, root_handle)
            } else {
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetSuperControl => {
            let control = cpu.gpr[3];
            let out_super = cpu.gpr[4];
            let parent = ppc_control_parent(controls, control);
            let result = if parent != 0 {
                if out_super != 0 {
                    let _ = memory.write_u32_be(out_super, parent);
                }
                PPC_NO_ERR
            } else {
                if out_super != 0 {
                    let _ = memory.write_u32_be(out_super, 0);
                }
                PPC_ERR_CONTROL_IS_NOT_SUB_CONTROL
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::CountSubControls => {
            let control = cpu.gpr[3];
            let out_count = cpu.gpr[4];
            let count = ppc_count_sub_controls(controls, control);
            if out_count != 0 {
                let _ = memory.write_u16_be(out_count, count);
            }
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcLegacyControlOperation::GetIndexedSubControl => {
            let control = cpu.gpr[3];
            let index = cpu.gpr[4] as u16;
            let out_sub = cpu.gpr[5];
            let sub = ppc_indexed_sub_control(controls, control, index);
            let result = if let Some(sub_handle) = sub {
                if out_sub != 0 {
                    let _ = memory.write_u32_be(out_sub, sub_handle);
                }
                PPC_NO_ERR
            } else {
                if out_sub != 0 {
                    let _ = memory.write_u32_be(out_sub, 0);
                }
                PPC_ERR_CONTROL_IS_NOT_EMBEDDER
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::CreateRootControl => {
            let window = cpu.gpr[3];
            let out_control = cpu.gpr[4];
            let existing = ppc_window_root_control(memory, controls, window);
            if let Some(existing_handle) = existing {
                if out_control != 0 {
                    let _ = memory.write_u32_be(out_control, existing_handle);
                }
                Some(PpcImportAction::Return(ppc_i16_result(PPC_ERR_ROOT_ALREADY_EXISTS)))
            } else {
                let bounds = ppc_read_rect(memory, window.wrapping_add(PPC_CWINDOW_PORT_RECT_OFFSET))
                    .unwrap_or((0, 0, 400, 600));
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                let handle = ppc_new_control_record_values(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    window,
                    bounds,
                    b"",
                    true,
                    0,
                    0,
                    0,
                    0,
                    0,
                );
                if handle != 0 {
                    if let Some(rec) = controls.iter_mut().find(|r| r.handle == handle) {
                        rec.is_root = true;
                    }
                    if out_control != 0 {
                        let _ = memory.write_u32_be(out_control, handle);
                    }
                    Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
                } else {
                    if out_control != 0 {
                        let _ = memory.write_u32_be(out_control, 0);
                    }
                    Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
                }
            }
        }
        PpcLegacyControlOperation::GetRootControl => {
            let window = cpu.gpr[3];
            let out_control = cpu.gpr[4];
            let existing = ppc_window_root_control(memory, controls, window);
            let result = if let Some(root_handle) = existing {
                if out_control != 0 {
                    let _ = memory.write_u32_be(out_control, root_handle);
                }
                PPC_NO_ERR
            } else {
                if out_control != 0 {
                    let _ = memory.write_u32_be(out_control, 0);
                }
                PPC_ERR_NO_ROOT_CONTROL
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlProperty => {
            let control = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let size = cpu.gpr[6];
            let data_ptr = cpu.gpr[7];
            let data = if data_ptr != 0 && size > 0 {
                ppc_memory_read_bytes(memory, data_ptr, size).unwrap_or_default()
            } else {
                Vec::new()
            };
            ppc_set_control_property(controls, control, creator, tag, 0, data);
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcLegacyControlOperation::GetControlProperty => {
            let control = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let buffer_size = cpu.gpr[6] as usize;
            let actual_size_ptr = cpu.gpr[7];
            let data_ptr = cpu.gpr[8];
            let result = if let Some(prop) = ppc_get_control_property(controls, control, creator, tag) {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, prop.data.len() as u32);
                }
                if data_ptr != 0 && buffer_size > 0 {
                    let copy_len = buffer_size.min(prop.data.len());
                    let _ = memory.write_bytes(data_ptr, &prop.data[..copy_len]);
                }
                PPC_NO_ERR
            } else {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, 0);
                }
                PPC_CONTROL_PROPERTY_NOT_FOUND_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlPropertySize => {
            let control = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let actual_size_ptr = cpu.gpr[6];
            let result = if let Some(prop) = ppc_get_control_property(controls, control, creator, tag) {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, prop.data.len() as u32);
                }
                PPC_NO_ERR
            } else {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, 0);
                }
                PPC_CONTROL_PROPERTY_NOT_FOUND_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::RemoveControlProperty => {
            let control = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let removed = ppc_remove_control_property(controls, control, creator, tag);
            let result = if removed {
                PPC_NO_ERR
            } else {
                PPC_CONTROL_PROPERTY_NOT_FOUND_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlPropertyAttributes => {
            let control = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let attributes_ptr = cpu.gpr[6];
            let result = if let Some(prop) = ppc_get_control_property(controls, control, creator, tag) {
                if attributes_ptr != 0 {
                    let _ = memory.write_u32_be(attributes_ptr, prop.attributes);
                }
                PPC_NO_ERR
            } else {
                if attributes_ptr != 0 {
                    let _ = memory.write_u32_be(attributes_ptr, 0);
                }
                PPC_CONTROL_PROPERTY_NOT_FOUND_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::ChangeControlPropertyAttributes => {
            let control = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let to_set = cpu.gpr[6];
            let to_clear = cpu.gpr[7];
            let result = ppc_change_control_property_attributes(
                controls, control, creator, tag, to_set, to_clear,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::DisposeControl => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_dispose_control(
                Some(&mut allocator),
                None,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                controls,
                cpu.gpr[3],
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::KillControls => {
            let owner = cpu.gpr[3];
            let control_handles = ppc_window_control_handles(memory, owner);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            for handle in control_handles {
                ppc_dispose_control(
                    Some(&mut allocator),
                    None,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    handle,
                );
            }
            let _ = memory.write_u32_be(owner.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET), 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::GetControlValue
        | PpcLegacyControlOperation::GetControlMinimum
        | PpcLegacyControlOperation::GetControlMaximum => {
            let offset = match operation {
                PpcLegacyControlOperation::GetControlMinimum => PPC_CONTROL_MIN_OFFSET,
                PpcLegacyControlOperation::GetControlMaximum => PPC_CONTROL_MAX_OFFSET,
                PpcLegacyControlOperation::GetControlValue => PPC_CONTROL_VALUE_OFFSET,
                _ => unreachable!(),
            };
            let value = ppc_control_ptr(memory, cpu.gpr[3])
                .and_then(|control| memory.read_u16_be(control.wrapping_add(offset)))
                .unwrap_or(0) as i16;
            Some(PpcImportAction::Return(ppc_i16_result(value)))
        }
        PpcLegacyControlOperation::GetControlReference => {
            let value = ppc_control_ptr(memory, cpu.gpr[3])
                .and_then(|control| {
                    memory.read_u32_be(control.wrapping_add(PPC_CONTROL_REF_CON_OFFSET))
                })
                .unwrap_or(0);
            Some(PpcImportAction::Return(value))
        }
        PpcLegacyControlOperation::GetControlTitle => {
            if let Some(control) = ppc_control_ptr(memory, cpu.gpr[3]) {
                let title =
                    ppc_read_pstring_bytes(memory, control.wrapping_add(PPC_CONTROL_TITLE_OFFSET))
                        .unwrap_or_default();
                let _ = ppc_write_pstring_bytes(memory, cpu.gpr[4], &title);
            } else if cpu.gpr[4] != 0 {
                let _ = memory.write_u8(cpu.gpr[4], 0);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::GetControlAction => {
            let action = ppc_control_ptr(memory, cpu.gpr[3])
                .and_then(|control| memory.read_u32_be(control + PPC_CONTROL_ACTION_OFFSET))
                .unwrap_or(0);
            Some(PpcImportAction::Return(action))
        }
        PpcLegacyControlOperation::GetControlVariant => {
            let handle = cpu.gpr[3];
            let variant = controls
                .iter()
                .find(|record| record.handle == handle)
                .map_or(0, |record| record.proc_id & 0x0F);
            Some(PpcImportAction::Return(ppc_i16_result(variant)))
        }
        PpcLegacyControlOperation::SetControlMinimum
        | PpcLegacyControlOperation::SetControlMaximum => {
            if let Some(control) = ppc_control_ptr(memory, cpu.gpr[3]) {
                let value = cpu.gpr[4] as u16 as i16;
                let offset = if operation == PpcLegacyControlOperation::SetControlMinimum {
                    PPC_CONTROL_MIN_OFFSET
                } else {
                    PPC_CONTROL_MAX_OFFSET
                };
                let _ = memory.write_u16_be(control.wrapping_add(offset), value as u16);
                ppc_clamp_control_value(memory, control);
                let _ = ppc_draw_control(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    cpu.gpr[3],
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::SetControlReference => {
            if let Some(control) = ppc_control_ptr(memory, cpu.gpr[3]) {
                let _ = memory
                    .write_u32_be(control.wrapping_add(PPC_CONTROL_REF_CON_OFFSET), cpu.gpr[4]);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::SetControlAction => {
            if let Some(control) = ppc_control_ptr(memory, cpu.gpr[3]) {
                let _ = memory.write_u32_be(control + PPC_CONTROL_ACTION_OFFSET, cpu.gpr[4]);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::ShowControl | PpcLegacyControlOperation::HideControl => {
            if let Some(control) = ppc_control_ptr(memory, cpu.gpr[3]) {
                let visible = operation == PpcLegacyControlOperation::ShowControl;
                let _ = memory.write_u8(
                    control.wrapping_add(PPC_CONTROL_VISIBLE_OFFSET),
                    if visible { 0xff } else { 0 },
                );
                if visible {
                    let _ = ppc_draw_control(
                        memory,
                        handles,
                        controls,
                        gworlds,
                        vfs_resources,
                        current_resource_refnum,
                        cpu.gpr[3],
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::MoveControl | PpcLegacyControlOperation::SizeControl => {
            if let Some(control) = ppc_control_ptr(memory, cpu.gpr[3]) {
                let rect_addr = control.wrapping_add(PPC_CONTROL_RECT_OFFSET);
                if let Some((top, left, bottom, right)) = ppc_read_rect(memory, rect_addr) {
                    let width = right.saturating_sub(left);
                    let height = bottom.saturating_sub(top);
                    let result = if operation == PpcLegacyControlOperation::MoveControl {
                        let new_left = cpu.gpr[4] as u16 as i16;
                        let new_top = cpu.gpr[5] as u16 as i16;
                        ppc_write_rect(
                            memory,
                            rect_addr,
                            new_top,
                            new_left,
                            new_top.saturating_add(height),
                            new_left.saturating_add(width),
                        )
                    } else {
                        let new_width = cpu.gpr[4] as u16 as i16;
                        let new_height = cpu.gpr[5] as u16 as i16;
                        ppc_write_rect(
                            memory,
                            rect_addr,
                            top,
                            left,
                            top.saturating_add(new_height),
                            left.saturating_add(new_width),
                        )
                    };
                    if result.is_some() {
                        let _ = ppc_draw_control(
                            memory,
                            handles,
                            controls,
                            gworlds,
                            vfs_resources,
                            current_resource_refnum,
                            cpu.gpr[3],
                        );
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::FindControl => {
            let v = (cpu.gpr[3] >> 16) as u16 as i16;
            let h = cpu.gpr[3] as u16 as i16;
            let (handle, part) =
                ppc_find_control_at_point(memory, controls, cpu.gpr[4], v, h).unwrap_or((0, 0));
            if cpu.gpr[5] != 0 {
                memory.write_u32_be(cpu.gpr[5], handle)?;
            }
            Some(PpcImportAction::Return(ppc_i16_result(part)))
        }
        PpcLegacyControlOperation::TestControl => {
            let v = (cpu.gpr[4] >> 16) as u16 as i16;
            let h = cpu.gpr[4] as u16 as i16;
            let part = ppc_control_part_at_point(memory, controls, cpu.gpr[3], v, h).unwrap_or(0);
            if crate::trap::dispatch::trace_input_enabled() {
                eprintln!(
                    "[INPUT] PPC TestControl control=${:08X} point=({}, {}) -> {}",
                    cpu.gpr[3], v, h, part
                );
            }
            Some(PpcImportAction::Return(ppc_i16_result(part)))
        }
        PpcLegacyControlOperation::TrackControl => {
            // Macintosh Toolbox Essentials (1992), pp. 5-79--5-80:
            // -1 selects contrlAction; a second -1 invokes the popup CDEF.
            let action_proc = if cpu.gpr[5] == u32::MAX {
                ppc_control_ptr(memory, cpu.gpr[3])
                    .and_then(|control| memory.read_u32_be(control + PPC_CONTROL_ACTION_OFFSET))
                    .unwrap_or(0)
            } else {
                cpu.gpr[5]
            };
            let v = (cpu.gpr[4] >> 16) as u16 as i16;
            let h = cpu.gpr[4] as u16 as i16;
            let part = ppc_control_part_at_point(memory, controls, cpu.gpr[3], v, h).unwrap_or(0);
            if crate::trap::dispatch::trace_input_enabled() {
                eprintln!(
                    "[INPUT] PPC TrackControl control=${:08X} point=({}, {}) action=${:08X} -> {}",
                    cpu.gpr[3], v, h, cpu.gpr[5], part
                );
            }
            if part != 0 && action_proc == u32::MAX {
                if let Some(action) = ppc_dispatch_popup_track_control(
                    cpu,
                    memory,
                    handles,
                    controls,
                    gworlds,
                    screen_clut,
                    vfs_resources,
                    current_resource_refnum,
                    toolbox_startup,
                    input,
                    cpu.gpr[3],
                    v,
                    h,
                ) {
                    return Some(action);
                }
            }
            // Macintosh Toolbox Essentials (1992), pp. 5-79--5-80:
            // Arrow/page value changes belong to the action procedure.
            // A nil action only returns the hit part to the caller.
            if part == 129 {
                let _ = ppc_track_scroll_control_value(
                    memory,
                    handles,
                    controls,
                    gworlds,
                    vfs_resources,
                    current_resource_refnum,
                    cpu.gpr[3],
                    v,
                    h,
                );
                return Some(PpcImportAction::Return(ppc_i16_result(part)));
            }
            if part == 0 || action_proc == 0 || action_proc == u32::MAX {
                return Some(PpcImportAction::Return(ppc_i16_result(part)));
            }
            let restore_rtoc = cpu.gpr[2];
            let final_pc = cpu.lr;
            let target = ppc_resolve_callback_target(memory, action_proc, restore_rtoc, None)?;
            install_powerpc_call_arguments(cpu, memory, &[cpu.gpr[3], part as u16 as u32])?;
            Some(
                GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: target.entry,
                        rtoc: target.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        final_pc,
                        restore_rtoc,
                        PpcNativeReturnGpr3::Set(ppc_i16_result(part)),
                    ),
                )
                .into_ppc_import_action()?,
            )
        }
        PpcLegacyControlOperation::DrawOneControl => {
            let _ = ppc_draw_control(
                memory,
                handles,
                controls,
                gworlds,
                vfs_resources,
                current_resource_refnum,
                cpu.gpr[3],
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::IsControlVisible => {
            let control = cpu.gpr[3];
            let visible = ppc_control_visible(memory, control);
            Some(PpcImportAction::Return(u32::from(visible)))
        }
        PpcLegacyControlOperation::IsControlEnabled => {
            let control = cpu.gpr[3];
            let enabled = ppc_control_enabled(memory, control);
            Some(PpcImportAction::Return(u32::from(enabled)))
        }
        PpcLegacyControlOperation::EnableControl => {
            let control = cpu.gpr[3];
            let result = ppc_set_control_enabled(
                memory,
                handles,
                controls,
                gworlds,
                vfs_resources,
                current_resource_refnum,
                control,
                true,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::DisableControl => {
            let control = cpu.gpr[3];
            let result = ppc_set_control_enabled(
                memory,
                handles,
                controls,
                gworlds,
                vfs_resources,
                current_resource_refnum,
                control,
                false,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::IsControlHilited => {
            let control = cpu.gpr[3];
            let hilited = ppc_control_hilited(memory, control);
            Some(PpcImportAction::Return(u32::from(hilited)))
        }
        PpcLegacyControlOperation::GetControlHilite => {
            let control = cpu.gpr[3];
            let hilite = ppc_control_hilite_code(memory, control);
            Some(PpcImportAction::Return(u32::from(hilite)))
        }
        PpcLegacyControlOperation::IsValidControlHandle => {
            let control = cpu.gpr[3];
            let valid = ppc_is_valid_control_handle(memory, controls, control);
            Some(PpcImportAction::Return(u32::from(valid)))
        }
        PpcLegacyControlOperation::GetControlBounds => {
            let control = cpu.gpr[3];
            let out_rect = cpu.gpr[4];
            let result = ppc_get_control_bounds(memory, control, out_rect);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlBounds => {
            let control = cpu.gpr[3];
            let in_rect = cpu.gpr[4];
            let result = ppc_set_control_bounds(
                memory,
                handles,
                controls,
                gworlds,
                vfs_resources,
                current_resource_refnum,
                control,
                in_rect,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::IdleControls | PpcLegacyControlOperation::DragControl => {
            Some(PpcImportAction::Return(0))
        }
        PpcLegacyControlOperation::GetControlData => {
            let control = cpu.gpr[3];
            let part = cpu.gpr[4] as u16 as i16;
            let tag = cpu.gpr[5];
            let buffer_size = cpu.gpr[6] as usize;
            let buffer_ptr = cpu.gpr[7];
            let actual_size_ptr = cpu.gpr[8];
            let result = if let Some(data) = ppc_get_control_data(controls, control, part, tag) {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, data.len() as u32);
                }
                if buffer_ptr != 0 && buffer_size > 0 {
                    let copy_len = buffer_size.min(data.len());
                    let _ = memory.write_bytes(buffer_ptr, &data[..copy_len]);
                }
                PPC_NO_ERR
            } else {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, 0);
                }
                PPC_CONTROL_PROPERTY_NOT_FOUND_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlData => {
            let control = cpu.gpr[3];
            let part = cpu.gpr[4] as u16 as i16;
            let tag = cpu.gpr[5];
            let size = cpu.gpr[6];
            let data_ptr = cpu.gpr[7];
            let data = if data_ptr != 0 && size > 0 {
                ppc_memory_read_bytes(memory, data_ptr, size).unwrap_or_default()
            } else {
                Vec::new()
            };
            let result = ppc_set_control_data(controls, control, part, tag, data);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlDataSize => {
            let control = cpu.gpr[3];
            let part = cpu.gpr[4] as u16 as i16;
            let tag = cpu.gpr[5];
            let actual_size_ptr = cpu.gpr[6];
            let result = if let Some(data) = ppc_get_control_data(controls, control, part, tag) {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, data.len() as u32);
                }
                PPC_NO_ERR
            } else {
                if actual_size_ptr != 0 {
                    let _ = memory.write_u32_be(actual_size_ptr, 0);
                }
                PPC_CONTROL_PROPERTY_NOT_FOUND_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlFeatures => {
            let control = cpu.gpr[3];
            let out_features = cpu.gpr[4];
            let result = if control != 0
                && (ppc_control_ptr(memory, control).is_some()
                    || controls.iter().any(|r| r.handle == control))
            {
                if out_features != 0 {
                    let _ = memory.write_u32_be(out_features, 3);
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetBestControlRect => {
            let control = cpu.gpr[3];
            let out_rect = cpu.gpr[4];
            let out_baseline_offset = cpu.gpr[5];
            let result = ppc_get_best_control_rect(memory, control, out_rect, out_baseline_offset);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlVisibility => {
            let control = cpu.gpr[3];
            let visible = cpu.gpr[4] != 0;
            let do_draw = cpu.gpr[5] != 0;
            let result = ppc_set_control_visibility(
                memory,
                handles,
                controls,
                gworlds,
                vfs_resources,
                current_resource_refnum,
                control,
                visible,
                do_draw,
            );
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlColorProc => {
            let control = cpu.gpr[3];
            let proc = cpu.gpr[4];
            let result = if control != 0 {
                if let Some(record) = controls.iter_mut().find(|r| r.handle == control) {
                    record.color_proc = proc;
                } else {
                    controls.push(PpcControlRecord {
                        handle: control,
                        color_proc: proc,
                        ..Default::default()
                    });
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlColorProc => {
            let control = cpu.gpr[3];
            let out_proc = cpu.gpr[4];
            let result = if control != 0 {
                let proc = controls
                    .iter()
                    .find(|r| r.handle == control)
                    .map_or(0, |r| r.color_proc);
                if out_proc != 0 {
                    let _ = memory.write_u32_be(out_proc, proc);
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::DrawControlInCurrentPort => {
            let control = cpu.gpr[3];
            let _ = ppc_draw_control(
                memory,
                handles,
                controls,
                gworlds,
                vfs_resources,
                current_resource_refnum,
                control,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyControlOperation::SetUpControlBackground => {
            let control = cpu.gpr[3];
            let result = if control != 0
                && (ppc_control_ptr(memory, control).is_some()
                    || controls.iter().any(|r| r.handle == control))
            {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlOwner => {
            let control = cpu.gpr[3];
            let owner = ppc_control_ptr(memory, control)
                .and_then(|ptr| memory.read_u32_be(ptr.wrapping_add(PPC_CONTROL_OWNER_OFFSET)))
                .unwrap_or(0);
            Some(PpcImportAction::Return(owner))
        }
        PpcLegacyControlOperation::GetControlRegion => {
            let control = cpu.gpr[3];
            let part = cpu.gpr[4] as u16 as i16;
            let tag = cpu.gpr[5];
            let out_rgn = cpu.gpr[6];
            let result = ppc_get_control_region(memory, control, part, tag, out_rgn);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlID => {
            let control = cpu.gpr[3];
            let in_id = cpu.gpr[4];
            let result = if control != 0 && in_id != 0 {
                let signature = memory.read_u32_be(in_id).unwrap_or(0);
                let id = memory.read_u32_be(in_id.wrapping_add(4)).unwrap_or(0) as i32;
                if let Some(record) = controls.iter_mut().find(|r| r.handle == control) {
                    record.control_id = (signature, id);
                } else {
                    controls.push(PpcControlRecord {
                        handle: control,
                        control_id: (signature, id),
                        ..Default::default()
                    });
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlID => {
            let control = cpu.gpr[3];
            let out_id = cpu.gpr[4];
            let result = if control != 0 && out_id != 0 {
                let (signature, id) = controls
                    .iter()
                    .find(|r| r.handle == control)
                    .map_or((0, 0), |r| r.control_id);
                let _ = memory.write_u32_be(out_id, signature);
                let _ = memory.write_u32_be(out_id.wrapping_add(4), id as u32);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlCommandID => {
            let control = cpu.gpr[3];
            let command_id = cpu.gpr[4];
            let result = if control != 0 {
                if let Some(record) = controls.iter_mut().find(|r| r.handle == control) {
                    record.command_id = command_id;
                } else {
                    controls.push(PpcControlRecord {
                        handle: control,
                        command_id,
                        ..Default::default()
                    });
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlCommandID => {
            let control = cpu.gpr[3];
            let out_command_id = cpu.gpr[4];
            let result = if control != 0 {
                let command_id = controls
                    .iter()
                    .find(|r| r.handle == control)
                    .map_or(0, |r| r.command_id);
                if out_command_id != 0 {
                    let _ = memory.write_u32_be(out_command_id, command_id);
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetKeyboardFocus => {
            let window = cpu.gpr[3];
            let control = cpu.gpr[4];
            let _part = cpu.gpr[5] as u16 as i16;
            let result = ppc_set_keyboard_focus(memory, controls, window, control);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetKeyboardFocus => {
            let window = cpu.gpr[3];
            let out_control = cpu.gpr[4];
            let result = if window != 0 {
                let window_controls = ppc_window_control_handles(memory, window);
                let focused = controls
                    .iter()
                    .find(|r| window_controls.contains(&r.handle) && r.has_focus)
                    .map_or(0, |r| r.handle);
                if out_control != 0 {
                    let _ = memory.write_u32_be(out_control, focused);
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::AdvanceKeyboardFocus
        | PpcLegacyControlOperation::ReverseKeyboardFocus => {
            let window = cpu.gpr[3];
            let advance = operation == PpcLegacyControlOperation::AdvanceKeyboardFocus;
            let result = ppc_cycle_keyboard_focus(memory, controls, window, advance);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::ClearKeyboardFocus => {
            let window = cpu.gpr[3];
            let result = ppc_set_keyboard_focus(memory, controls, window, 0);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlByID => {
            let window = cpu.gpr[3];
            let in_id = cpu.gpr[4];
            let out_control = cpu.gpr[5];
            let result = if window != 0 && in_id != 0 && out_control != 0 {
                let signature = memory.read_u32_be(in_id).unwrap_or(0);
                let id = memory.read_u32_be(in_id.wrapping_add(4)).unwrap_or(0) as i32;
                let window_controls = ppc_window_control_handles(memory, window);
                let found = window_controls.into_iter().find(|&h| {
                    controls
                        .iter()
                        .find(|r| r.handle == h)
                        .is_some_and(|r| r.control_id == (signature, id))
                });
                if let Some(h) = found {
                    let _ = memory.write_u32_be(out_control, h);
                    PPC_NO_ERR
                } else {
                    -30580
                }
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::FindControlUnderMouse => {
            let v = (cpu.gpr[3] >> 16) as u16 as i16;
            let h = cpu.gpr[3] as u16 as i16;
            let window = cpu.gpr[4];
            let out_part = cpu.gpr[5];
            let (handle, part) =
                ppc_find_control_at_point(memory, controls, window, v, h).unwrap_or((0, 0));
            if out_part != 0 {
                let _ = memory.write_u16_be(out_part, part as u16);
            }
            Some(PpcImportAction::Return(handle))
        }
        PpcLegacyControlOperation::HandleControlClick => {
            let control_handle = cpu.gpr[3];
            let v = (cpu.gpr[4] >> 16) as u16 as i16;
            let h = cpu.gpr[4] as u16 as i16;
            let part = ppc_control_part_at_point(memory, controls, control_handle, v, h).unwrap_or(0);
            Some(PpcImportAction::Return(ppc_i16_result(part)))
        }
        PpcLegacyControlOperation::HandleControlKey => {
            let control_handle = cpu.gpr[3];
            let part = if control_handle != 0 && ppc_control_enabled(memory, control_handle) {
                1i16
            } else {
                0i16
            };
            Some(PpcImportAction::Return(ppc_i16_result(part)))
        }
        PpcLegacyControlOperation::GetControlClickActivation => {
            let control_handle = cpu.gpr[3];
            let out_result = cpu.gpr[6];
            let result = if control_handle != 0 && out_result != 0 {
                let is_active = ppc_control_enabled(memory, control_handle);
                let activation = if is_active { 1u32 } else { 2u32 };
                let _ = memory.write_u32_be(out_result, activation);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlKind => {
            let control_handle = cpu.gpr[3];
            let out_kind = cpu.gpr[4];
            let result = if control_handle != 0 && out_kind != 0 {
                let proc_id = controls
                    .iter()
                    .find(|r| r.handle == control_handle)
                    .map_or(0i16, |r| r.proc_id);
                let _ = memory.write_u32_be(out_kind, u32::from_be_bytes(*b"appl"));
                let _ = memory.write_u32_be(out_kind.wrapping_add(4), proc_id as u16 as u32);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlFocusPart => {
            let control_handle = cpu.gpr[3];
            let in_part = cpu.gpr[4] as i16;
            let result = if control_handle != 0 {
                if let Some(record) = controls.iter_mut().find(|r| r.handle == control_handle) {
                    record.focus_part = in_part;
                    record.has_focus = in_part != 0;
                } else {
                    controls.push(PpcControlRecord {
                        handle: control_handle,
                        focus_part: in_part,
                        has_focus: in_part != 0,
                        ..Default::default()
                    });
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::GetControlFocusPart => {
            let control_handle = cpu.gpr[3];
            let out_part = cpu.gpr[4];
            let result = if control_handle != 0 && out_part != 0 {
                let part = controls
                    .iter()
                    .find(|r| r.handle == control_handle)
                    .map_or(0i16, |r| r.focus_part);
                let _ = memory.write_u16_be(out_part, part as u16);
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SendControlMessage => {
            Some(PpcImportAction::Return(0))
        }
        PpcLegacyControlOperation::ScrollControlValues => {
            let control_handle = cpu.gpr[3];
            let delta_x = cpu.gpr[4] as i16;
            let delta_y = cpu.gpr[5] as i16;
            let result = if control_handle != 0 {
                if let Some(control) = memory.read_u32_be(control_handle).filter(|&c| c != 0) {
                    let current_val = memory
                        .read_u16_be(control.wrapping_add(PPC_CONTROL_VALUE_OFFSET))
                        .unwrap_or(0) as i16;
                    let delta = if delta_y != 0 { delta_y } else { delta_x };
                    let new_val = current_val.saturating_add(delta);
                    let _ = memory.write_u16_be(
                        control.wrapping_add(PPC_CONTROL_VALUE_OFFSET),
                        new_val as u16,
                    );
                    ppc_clamp_control_value(memory, control);
                    let _ = ppc_draw_control(
                        memory,
                        handles,
                        controls,
                        gworlds,
                        vfs_resources,
                        current_resource_refnum,
                        control_handle,
                    );
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::IsControlDragTrackingEnabled => {
            let control_handle = cpu.gpr[3];
            let out_tracks = cpu.gpr[4];
            let result = if control_handle != 0 && out_tracks != 0 {
                let tracks = controls
                    .iter()
                    .find(|r| r.handle == control_handle)
                    .is_some_and(|r| r.drag_tracking_enabled);
                let _ = memory.write_u8(out_tracks, if tracks { 1 } else { 0 });
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyControlOperation::SetControlDragTrackingEnabled => {
            let control_handle = cpu.gpr[3];
            let tracks = cpu.gpr[4] != 0;
            let result = if control_handle != 0 {
                if let Some(record) = controls.iter_mut().find(|r| r.handle == control_handle) {
                    record.drag_tracking_enabled = tracks;
                } else {
                    controls.push(PpcControlRecord {
                        handle: control_handle,
                        drag_tracking_enabled: tracks,
                        ..Default::default()
                    });
                }
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_new_control_values(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    owner: u32,
    bounds_ptr: u32,
    title_ptr: u32,
    visible: bool,
    value: i16,
    min: i16,
    max: i16,
    proc_id: i16,
    ref_con: u32,
) -> u32 {
    let Some(bounds) = ppc_read_rect(memory, bounds_ptr) else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    let title = ppc_read_pstring_bytes(memory, title_ptr).unwrap_or_default();
    ppc_new_control_record_values(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        controls,
        owner,
        bounds,
        &title,
        visible,
        value,
        min,
        max,
        proc_id,
        ref_con,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_new_control_record_values(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    owner: u32,
    bounds: (i16, i16, i16, i16),
    title: &[u8],
    visible: bool,
    value: i16,
    min: i16,
    max: i16,
    proc_id: i16,
    ref_con: u32,
) -> u32 {
    if owner == 0
        || memory
            .read_u32_be(owner.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET))
            .is_none()
    {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    let handle = ppc_allocator_view_allocate_handle(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        PPC_CONTROL_RECORD_SIZE,
        true,
    );
    let Some(control) = ppc_control_ptr(memory, handle) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    };
    let old_head = memory
        .read_u32_be(owner.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET))
        .unwrap_or(0);
    // Popup CDEF records repurpose contrlMin for the menu ID and contrlMax
    // for the title width, so their value is a menu item number rather than
    // an ordinary min/max control value. Macintosh Toolbox Essentials (1992),
    // pp. 5-25--5-27.
    let initial_value = if (1008..=1023).contains(&(proc_id & 0x0fff)) {
        value
    } else {
        value.clamp(min.min(max), min.max(max))
    };
    let wrote = memory
        .write_u32_be(control.wrapping_add(PPC_CONTROL_NEXT_OFFSET), old_head)
        .is_some()
        && memory
            .write_u32_be(control.wrapping_add(PPC_CONTROL_OWNER_OFFSET), owner)
            .is_some()
        && ppc_write_rect(
            memory,
            control + PPC_CONTROL_RECT_OFFSET,
            bounds.0,
            bounds.1,
            bounds.2,
            bounds.3,
        )
        .is_some()
        && memory
            .write_u8(
                control.wrapping_add(PPC_CONTROL_VISIBLE_OFFSET),
                if visible { 0xff } else { 0 },
            )
            .is_some()
        && memory
            .write_u8(control.wrapping_add(PPC_CONTROL_HILITE_OFFSET), 0)
            .is_some()
        && memory
            .write_u16_be(
                control.wrapping_add(PPC_CONTROL_VALUE_OFFSET),
                initial_value as u16,
            )
            .is_some()
        && memory
            .write_u16_be(control.wrapping_add(PPC_CONTROL_MIN_OFFSET), min as u16)
            .is_some()
        && memory
            .write_u16_be(control.wrapping_add(PPC_CONTROL_MAX_OFFSET), max as u16)
            .is_some()
        && memory
            .write_u32_be(control.wrapping_add(PPC_CONTROL_REF_CON_OFFSET), ref_con)
            .is_some()
        && ppc_write_pstring_bytes(
            memory,
            control.wrapping_add(PPC_CONTROL_TITLE_OFFSET),
            title,
        )
        && memory
            .write_u32_be(owner.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET), handle)
            .is_some();
    if !wrote {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    let popup = (1008..=1023).contains(&(proc_id & 0x0fff));
    // Macintosh Toolbox Essentials (1992), pp. 5-79--5-80.
    let _ = memory.write_u32_be(control + 32, if popup { u32::MAX } else { 0 });
    controls.retain(|record| record.handle != handle);
    controls.push(PpcControlRecord {
        handle,
        pointer: control,
        proc_id,
        popup_menu_id: if popup { min } else { 0 },
        popup_title_width: popup.then_some(max),
        active: true,
        font_style: None,
        is_root: false,
        parent: 0,
        sub_controls: Vec::new(),
        properties: Vec::new(),
        color_proc: 0,
        control_id: (0, 0),
        command_id: 0,
        has_focus: false,
        focus_part: 0,
        drag_tracking_enabled: false,
    });
    *last_mem_error = PPC_NO_ERR;
    handle
}

// GetNewControl converts popup creation parameters into the live item range.
// The resource value is title style, not the selected item; menu ID and title
// width are retained in PpcControlRecord. Macintosh Toolbox Essentials (1992),
// Creating Pop-Up Menus, pp. 5-25--5-27.
pub(super) fn ppc_initialize_popup_control(
    memory: &mut PpcSectionMem,
    controls: &[PpcControlRecord],
    resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    handle: u32,
) {
    let Some(record) = controls.iter().find(|record| {
        record.handle == handle && (1008..=1023).contains(&(record.proc_id & 0x0fff))
    }) else {
        return;
    };
    let Some(control) = ppc_control_ptr(memory, handle) else {
        return;
    };
    let menu_list = ppc_current_menu_list(memory);
    let menu = ppc_get_menu_handle(memory, menu_list, record.popup_menu_id);
    let count = if menu != 0 {
        usize::from(ppc_count_menu_items(memory, menu))
    } else {
        ppc_vfs_resource_index(
            resources,
            current_resource_refnum,
            u32::from_be_bytes(*b"MENU"),
            record.popup_menu_id,
            false,
        )
        .and_then(|index| ppc_decode_menu_items(&resources[index].data))
        .map_or(0, |(_, _, items)| items.len())
    };
    let count = count.min(i16::MAX as usize) as u16;
    let _ = memory.write_u16_be(control + PPC_CONTROL_VALUE_OFFSET, u16::from(count != 0));
    let _ = memory.write_u16_be(control + PPC_CONTROL_MIN_OFFSET, 1);
    let _ = memory.write_u16_be(control + PPC_CONTROL_MAX_OFFSET, count);
}

fn ppc_materialize_control_resource_parameters(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    bytes: &[u8],
) -> Option<(u32, u32)> {
    if bytes.len() < 23 {
        return None;
    }
    let title_len = usize::from(bytes[22]).min(bytes.len().saturating_sub(23));
    let scratch = ppc_allocator_view_reserve_bytes(
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        u32::try_from(8 + 1 + title_len).ok()?,
        true,
    );
    if scratch == 0 {
        return None;
    }
    memory.write_bytes(scratch, &bytes[..8])?;
    memory.write_u8(scratch + 8, title_len as u8)?;
    memory.write_bytes(scratch + 9, &bytes[23..23 + title_len])?;
    Some((scratch, scratch + 8))
}

pub(super) fn ppc_control_ptr(memory: &mut PpcSectionMem, handle: u32) -> Option<u32> {
    (handle != 0)
        .then(|| memory.read_u32_be(handle))
        .flatten()
        .filter(|control| *control != 0)
}

pub(super) fn ppc_window_control_handles(memory: &mut PpcSectionMem, owner: u32) -> Vec<u32> {
    let mut out = Vec::new();
    let mut handle = memory
        .read_u32_be(owner.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET))
        .unwrap_or(0);
    while handle != 0 && out.len() < 4096 && !out.contains(&handle) {
        out.push(handle);
        handle = ppc_control_ptr(memory, handle)
            .and_then(|control| memory.read_u32_be(control.wrapping_add(PPC_CONTROL_NEXT_OFFSET)))
            .unwrap_or(0);
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_dispose_control(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    legacy_free_handle_blocks: Option<&mut Vec<PpcHandleRecord>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    handle: u32,
) {
    let Some(control) = ppc_control_ptr(memory, handle) else {
        return;
    };
    let owner = memory
        .read_u32_be(control.wrapping_add(PPC_CONTROL_OWNER_OFFSET))
        .unwrap_or(0);
    let next = memory
        .read_u32_be(control.wrapping_add(PPC_CONTROL_NEXT_OFFSET))
        .unwrap_or(0);
    let head_addr = owner.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET);
    let head = memory.read_u32_be(head_addr).unwrap_or(0);
    if head == handle {
        let _ = memory.write_u32_be(head_addr, next);
    } else {
        for candidate in ppc_window_control_handles(memory, owner) {
            let Some(candidate_ptr) = ppc_control_ptr(memory, candidate) else {
                continue;
            };
            if memory.read_u32_be(candidate_ptr.wrapping_add(PPC_CONTROL_NEXT_OFFSET))
                == Some(handle)
            {
                let _ =
                    memory.write_u32_be(candidate_ptr.wrapping_add(PPC_CONTROL_NEXT_OFFSET), next);
                break;
            }
        }
    }
    if let Some(allocator) = allocator {
        let _ = allocator.dispose_handle(
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        );
    } else if let Some(free_handle_blocks) = legacy_free_handle_blocks {
        if let Some(index) = handles.iter().position(|record| record.handle == handle) {
            let record = handles.remove(index);
            let _ = memory.write_u32_be(handle, 0);
            free_handle_blocks.push(record);
        }
    }
    let parent = controls.iter().find(|r| r.handle == handle).map_or(0, |r| r.parent);
    if parent != 0 {
        if let Some(parent_rec) = controls.iter_mut().find(|r| r.handle == parent) {
            parent_rec.sub_controls.retain(|h| *h != handle);
        }
    }
    for record in controls.iter_mut() {
        if record.parent == handle {
            record.parent = 0;
        }
        record.sub_controls.retain(|h| *h != handle);
    }
    controls.retain(|record| record.handle != handle);
}

pub(super) const PPC_ERR_NO_ROOT_CONTROL: i16 = -30586;
pub(super) const PPC_ERR_ROOT_ALREADY_EXISTS: i16 = -30587;
pub(super) const PPC_ERR_CONTROL_IS_NOT_EMBEDDER: i16 = -30590;
pub(super) const PPC_ERR_CONTROL_IS_NOT_SUB_CONTROL: i16 = -30592;
pub(super) const PPC_ERR_CANT_EMBED_INTO_SELF: i16 = -30594;
pub(super) const PPC_CONTROL_PROPERTY_NOT_FOUND_ERR: i16 = -5604;

pub(super) fn ppc_window_root_control(
    memory: &mut PpcSectionMem,
    controls: &[PpcControlRecord],
    window: u32,
) -> Option<u32> {
    if window == 0 {
        return None;
    }
    for record in controls {
        if record.is_root {
            let ptr = if record.pointer != 0 {
                Some(record.pointer)
            } else {
                ppc_control_ptr(memory, record.handle)
            };
            if let Some(ptr) = ptr {
                let owner = memory
                    .read_u32_be(ptr.wrapping_add(PPC_CONTROL_OWNER_OFFSET))
                    .unwrap_or(0);
                if owner == window {
                    return Some(record.handle);
                }
            }
        }
    }
    None
}

pub(super) fn ppc_embed_control(
    controls: &mut Vec<PpcControlRecord>,
    control: u32,
    container: u32,
) -> i16 {
    if control == 0 {
        return PPC_PARAM_ERR;
    }
    if control == container {
        return PPC_ERR_CANT_EMBED_INTO_SELF;
    }
    let mut curr = container;
    while curr != 0 {
        if curr == control {
            return PPC_ERR_CANT_EMBED_INTO_SELF;
        }
        curr = controls
            .iter()
            .find(|r| r.handle == curr)
            .map_or(0, |r| r.parent);
    }

    let old_parent = controls
        .iter()
        .find(|r| r.handle == control)
        .map_or(0, |r| r.parent);
    if old_parent != 0 {
        if let Some(old_rec) = controls.iter_mut().find(|r| r.handle == old_parent) {
            old_rec.sub_controls.retain(|h| *h != control);
        }
    }

    if let Some(rec) = controls.iter_mut().find(|r| r.handle == control) {
        rec.parent = container;
    } else {
        controls.push(PpcControlRecord {
            handle: control,
            parent: container,
            ..Default::default()
        });
    }

    if container != 0 {
        if let Some(container_rec) = controls.iter_mut().find(|r| r.handle == container) {
            if !container_rec.sub_controls.contains(&control) {
                container_rec.sub_controls.push(control);
            }
        } else {
            controls.push(PpcControlRecord {
                handle: container,
                sub_controls: vec![control],
                ..Default::default()
            });
        }
    }
    PPC_NO_ERR
}

pub(super) fn ppc_control_parent(controls: &[PpcControlRecord], control: u32) -> u32 {
    controls
        .iter()
        .find(|r| r.handle == control)
        .map_or(0, |r| r.parent)
}

pub(super) fn ppc_count_sub_controls(controls: &[PpcControlRecord], control: u32) -> u16 {
    controls
        .iter()
        .find(|r| r.handle == control)
        .map_or(0, |r| r.sub_controls.len() as u16)
}

pub(super) fn ppc_indexed_sub_control(
    controls: &[PpcControlRecord],
    control: u32,
    index: u16,
) -> Option<u32> {
    if index == 0 {
        return None;
    }
    controls
        .iter()
        .find(|r| r.handle == control)
        .and_then(|r| r.sub_controls.get((index - 1) as usize).copied())
}

pub(super) fn ppc_set_control_property(
    controls: &mut Vec<PpcControlRecord>,
    control: u32,
    creator: u32,
    tag: u32,
    attributes: u32,
    data: Vec<u8>,
) {
    if control == 0 {
        return;
    }
    let record = if let Some(rec) = controls.iter_mut().find(|r| r.handle == control) {
        rec
    } else {
        controls.push(PpcControlRecord {
            handle: control,
            ..Default::default()
        });
        controls.last_mut().unwrap()
    };

    if let Some(prop) = record
        .properties
        .iter_mut()
        .find(|p| p.creator == creator && p.tag == tag)
    {
        prop.attributes = attributes;
        prop.data = data;
    } else {
        record.properties.push(ProcessControlProperty {
            creator,
            tag,
            attributes,
            data,
        });
    }
}

pub(super) fn ppc_get_control_property(
    controls: &[PpcControlRecord],
    control: u32,
    creator: u32,
    tag: u32,
) -> Option<&ProcessControlProperty> {
    controls
        .iter()
        .find(|r| r.handle == control)
        .and_then(|r| r.properties.iter().find(|p| p.creator == creator && p.tag == tag))
}

pub(super) fn ppc_remove_control_property(
    controls: &mut Vec<PpcControlRecord>,
    control: u32,
    creator: u32,
    tag: u32,
) -> bool {
    if let Some(record) = controls.iter_mut().find(|r| r.handle == control) {
        let before = record.properties.len();
        record.properties.retain(|p| !(p.creator == creator && p.tag == tag));
        record.properties.len() < before
    } else {
        false
    }
}

pub(super) fn ppc_change_control_property_attributes(
    controls: &mut Vec<PpcControlRecord>,
    control: u32,
    creator: u32,
    tag: u32,
    to_set: u32,
    to_clear: u32,
) -> i16 {
    if let Some(record) = controls.iter_mut().find(|r| r.handle == control) {
        if let Some(prop) = record
            .properties
            .iter_mut()
            .find(|p| p.creator == creator && p.tag == tag)
        {
            prop.attributes = (prop.attributes | to_set) & !to_clear;
            return PPC_NO_ERR;
        }
    }
    PPC_CONTROL_PROPERTY_NOT_FOUND_ERR
}

pub(super) fn ppc_control_visible(memory: &mut PpcSectionMem, handle: u32) -> bool {
    ppc_control_ptr(memory, handle)
        .and_then(|ptr| memory.read_u8(ptr.wrapping_add(PPC_CONTROL_VISIBLE_OFFSET)))
        .is_some_and(|vis| vis != 0)
}

pub(super) fn ppc_control_enabled(memory: &mut PpcSectionMem, handle: u32) -> bool {
    ppc_control_ptr(memory, handle)
        .and_then(|ptr| memory.read_u8(ptr.wrapping_add(PPC_CONTROL_HILITE_OFFSET)))
        .is_some_and(|hilite| hilite != 255)
}

pub(super) fn ppc_control_hilited(memory: &mut PpcSectionMem, handle: u32) -> bool {
    ppc_control_ptr(memory, handle)
        .and_then(|ptr| memory.read_u8(ptr.wrapping_add(PPC_CONTROL_HILITE_OFFSET)))
        .is_some_and(|hilite| hilite > 0 && hilite < 255)
}

pub(super) fn ppc_control_hilite_code(memory: &mut PpcSectionMem, handle: u32) -> u8 {
    ppc_control_ptr(memory, handle)
        .and_then(|ptr| memory.read_u8(ptr.wrapping_add(PPC_CONTROL_HILITE_OFFSET)))
        .unwrap_or(0)
}

pub(super) fn ppc_is_valid_control_handle(
    memory: &mut PpcSectionMem,
    controls: &[PpcControlRecord],
    handle: u32,
) -> bool {
    handle != 0
        && controls.iter().any(|r| r.handle == handle)
        && ppc_control_ptr(memory, handle).is_some()
}

pub(super) fn ppc_get_control_bounds(
    memory: &mut PpcSectionMem,
    handle: u32,
    out_rect: u32,
) -> i16 {
    let Some(control_ptr) = ppc_control_ptr(memory, handle) else {
        return PPC_PARAM_ERR;
    };
    if out_rect == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(bytes) = ppc_memory_read_bytes(memory, control_ptr.wrapping_add(PPC_CONTROL_RECT_OFFSET), 8) else {
        return PPC_PARAM_ERR;
    };
    if memory.write_bytes(out_rect, &bytes).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_set_control_bounds(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &mut [PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    handle: u32,
    in_rect: u32,
) -> i16 {
    let Some(control_ptr) = ppc_control_ptr(memory, handle) else {
        return PPC_PARAM_ERR;
    };
    if in_rect == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(bytes) = ppc_memory_read_bytes(memory, in_rect, 8) else {
        return PPC_PARAM_ERR;
    };
    if memory.write_bytes(control_ptr.wrapping_add(PPC_CONTROL_RECT_OFFSET), &bytes).is_none() {
        return PPC_PARAM_ERR;
    }
    if ppc_control_visible(memory, handle) {
        let _ = ppc_draw_control(
            memory,
            handles,
            controls,
            gworlds,
            vfs_resources,
            current_resource_refnum,
            handle,
        );
    }
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_set_control_enabled(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &mut [PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    handle: u32,
    enabled: bool,
) -> i16 {
    let Some(control_ptr) = ppc_control_ptr(memory, handle) else {
        return PPC_PARAM_ERR;
    };
    let current_hilite = memory
        .read_u8(control_ptr.wrapping_add(PPC_CONTROL_HILITE_OFFSET))
        .unwrap_or(0);
    let new_hilite = if enabled {
        if current_hilite == 255 {
            0
        } else {
            current_hilite
        }
    } else {
        255
    };
    if memory.write_u8(control_ptr.wrapping_add(PPC_CONTROL_HILITE_OFFSET), new_hilite).is_none() {
        return PPC_PARAM_ERR;
    }
    if ppc_control_visible(memory, handle) {
        let _ = ppc_draw_control(
            memory,
            handles,
            controls,
            gworlds,
            vfs_resources,
            current_resource_refnum,
            handle,
        );
    }
    PPC_NO_ERR
}

pub(super) fn ppc_set_control_data(
    controls: &mut Vec<PpcControlRecord>,
    control: u32,
    _part: i16,
    tag: u32,
    data: Vec<u8>,
) -> i16 {
    if control == 0 {
        return PPC_PARAM_ERR;
    }
    let record = if let Some(rec) = controls.iter_mut().find(|r| r.handle == control) {
        rec
    } else {
        controls.push(PpcControlRecord {
            handle: control,
            ..Default::default()
        });
        controls.last_mut().unwrap()
    };

    if let Some(prop) = record.properties.iter_mut().find(|p| p.tag == tag) {
        prop.data = data;
    } else {
        record.properties.push(ProcessControlProperty {
            creator: 0,
            tag,
            attributes: 0,
            data,
        });
    }
    PPC_NO_ERR
}

pub(super) fn ppc_get_control_data(
    controls: &[PpcControlRecord],
    control: u32,
    _part: i16,
    tag: u32,
) -> Option<&[u8]> {
    controls
        .iter()
        .find(|r| r.handle == control)
        .and_then(|r| {
            r.properties
                .iter()
                .find(|p| p.creator == 0 && p.tag == tag)
                .or_else(|| r.properties.iter().find(|p| p.tag == tag))
        })
        .map(|p| p.data.as_slice())
}

pub(super) fn ppc_get_best_control_rect(
    memory: &mut PpcSectionMem,
    control: u32,
    out_rect: u32,
    out_baseline_offset: u32,
) -> i16 {
    let Some(control_ptr) = ppc_control_ptr(memory, control) else {
        return PPC_PARAM_ERR;
    };
    if out_rect != 0 {
        let Some(bytes) = ppc_memory_read_bytes(
            memory,
            control_ptr.wrapping_add(PPC_CONTROL_RECT_OFFSET),
            8,
        ) else {
            return PPC_PARAM_ERR;
        };
        if memory.write_bytes(out_rect, &bytes).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    if out_baseline_offset != 0 {
        if memory.write_u16_be(out_baseline_offset, 0).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_set_control_visibility(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &mut [PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    control: u32,
    visible: bool,
    do_draw: bool,
) -> i16 {
    let Some(control_ptr) = ppc_control_ptr(memory, control) else {
        return PPC_PARAM_ERR;
    };
    let _ = memory.write_u8(
        control_ptr.wrapping_add(PPC_CONTROL_VISIBLE_OFFSET),
        if visible { 0xff } else { 0 },
    );
    if visible && do_draw {
        let _ = ppc_draw_control(
            memory,
            handles,
            controls,
            gworlds,
            vfs_resources,
            current_resource_refnum,
            control,
        );
    }
    PPC_NO_ERR
}

pub(super) fn ppc_get_control_region(
    memory: &mut PpcSectionMem,
    control: u32,
    _part: i16,
    _tag: u32,
    out_rgn: u32,
) -> i16 {
    let Some(control_ptr) = ppc_control_ptr(memory, control) else {
        return PPC_PARAM_ERR;
    };
    if out_rgn == 0 {
        return PPC_PARAM_ERR;
    }
    let Some((top, left, bottom, right)) =
        ppc_read_rect(memory, control_ptr.wrapping_add(PPC_CONTROL_RECT_OFFSET))
    else {
        return PPC_PARAM_ERR;
    };
    if ppc_write_rgn_bbox(memory, out_rgn, top, left, bottom, right).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(super) fn ppc_set_keyboard_focus(
    memory: &mut PpcSectionMem,
    controls: &mut [PpcControlRecord],
    window: u32,
    control: u32,
) -> i16 {
    if window == 0 {
        return PPC_PARAM_ERR;
    }
    let window_controls = ppc_window_control_handles(memory, window);
    for handle in &window_controls {
        if let Some(record) = controls.iter_mut().find(|r| r.handle == *handle) {
            record.has_focus = false;
        }
    }
    if control != 0 && control != u32::MAX {
        if let Some(record) = controls.iter_mut().find(|r| r.handle == control) {
            record.has_focus = true;
        }
    }
    PPC_NO_ERR
}

pub(super) fn ppc_cycle_keyboard_focus(
    memory: &mut PpcSectionMem,
    controls: &mut [PpcControlRecord],
    window: u32,
    advance: bool,
) -> i16 {
    if window == 0 {
        return PPC_PARAM_ERR;
    }
    let window_controls = ppc_window_control_handles(memory, window);
    if window_controls.is_empty() {
        return PPC_NO_ERR;
    }
    let current_index = window_controls.iter().position(|handle| {
        controls
            .iter()
            .any(|r| r.handle == *handle && r.has_focus)
    });
    let next_index = match current_index {
        Some(idx) => {
            if advance {
                (idx + 1) % window_controls.len()
            } else if idx == 0 {
                window_controls.len() - 1
            } else {
                idx - 1
            }
        }
        None => {
            if advance {
                0
            } else {
                window_controls.len() - 1
            }
        }
    };
    let next_handle = window_controls[next_index];
    ppc_set_keyboard_focus(memory, controls, window, next_handle)
}

pub(super) fn ppc_clamp_control_value(memory: &mut PpcSectionMem, control: u32) {
    let value = memory
        .read_u16_be(control.wrapping_add(PPC_CONTROL_VALUE_OFFSET))
        .unwrap_or(0) as i16;
    let min = memory
        .read_u16_be(control.wrapping_add(PPC_CONTROL_MIN_OFFSET))
        .unwrap_or(0) as i16;
    let max = memory
        .read_u16_be(control.wrapping_add(PPC_CONTROL_MAX_OFFSET))
        .unwrap_or(0) as i16;
    let _ = memory.write_u16_be(
        control.wrapping_add(PPC_CONTROL_VALUE_OFFSET),
        value.clamp(min.min(max), min.max(max)) as u16,
    );
}

pub(super) fn ppc_control_part_at_point(
    memory: &mut PpcSectionMem,
    controls: &[PpcControlRecord],
    handle: u32,
    v: i16,
    h: i16,
) -> Option<i16> {
    let control = ppc_control_ptr(memory, handle)?;
    if !controls
        .iter()
        .find(|record| record.handle == handle)
        .is_some_and(|record| record.active)
    {
        return None;
    }
    if memory.read_u8(control + PPC_CONTROL_VISIBLE_OFFSET)? == 0
        || memory.read_u8(control + PPC_CONTROL_HILITE_OFFSET)? >= 0xfe
    {
        return None;
    }
    let (top, left, bottom, right) = ppc_read_rect(memory, control + PPC_CONTROL_RECT_OFFSET)?;
    if v < top || v >= bottom || h < left || h >= right {
        return None;
    }
    let proc_id = controls
        .iter()
        .find(|record| record.handle == handle)
        .map_or(0, |record| record.proc_id);
    match proc_id & 0x0fff {
        0 => Some(10),
        1 | 2 => Some(11),
        16 => {
            let vertical = bottom.saturating_sub(top) >= right.saturating_sub(left);
            let axis_start = if vertical { top } else { left };
            let axis_end = if vertical { bottom } else { right };
            let coordinate = if vertical { v } else { h };
            let arrow = (axis_end - axis_start).clamp(1, 16);
            if coordinate < axis_start.saturating_add(arrow) {
                return Some(20);
            }
            if coordinate >= axis_end.saturating_sub(arrow) {
                return Some(21);
            }
            let min = memory.read_u16_be(control + PPC_CONTROL_MIN_OFFSET)? as i16;
            let max = memory.read_u16_be(control + PPC_CONTROL_MAX_OFFSET)? as i16;
            let value = memory.read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)? as i16;
            let track_start = axis_start.saturating_add(arrow);
            let track_end = axis_end.saturating_sub(arrow);
            let track = i32::from(track_end.saturating_sub(track_start)).max(1);
            let thumb = 8i32.min(track);
            let span = i32::from(max).saturating_sub(i32::from(min)).max(1);
            let relative = i32::from(value)
                .saturating_sub(i32::from(min))
                .clamp(0, span);
            let thumb_start = i32::from(track_start)
                + relative.saturating_mul(track.saturating_sub(thumb)) / span;
            let coordinate = i32::from(coordinate);
            if coordinate < thumb_start {
                Some(22)
            } else if coordinate < thumb_start + thumb {
                Some(129)
            } else {
                Some(23)
            }
        }
        // Appearance Manager group boxes are decoration and must not swallow
        // clicks intended for the controls they enclose. The checkbox- and
        // popup-title variants keep their title hit area interactive; every
        // other part reports kControlNoPart. Appearance Manager 1.0,
        // ControlDefinitions.h kControlGroupBoxTextTitleProc (160) through
        // kControlGroupBoxSecondaryPopupButtonProc (166). Unverified: the
        // Mac OS 8 Control Manager Reference does not say which part a group
        // box reports for a click in its body.
        160..=166 => {
            let title_bottom = top.saturating_add(10);
            if matches!(proc_id & 0x0fff, 161 | 162 | 165 | 166)
                && v < title_bottom
                && h >= left.saturating_add(8)
            {
                Some(10)
            } else {
                Some(0)
            }
        }
        _ => Some(10),
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_track_scroll_control_value(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    handle: u32,
    v: i16,
    h: i16,
) -> Option<i16> {
    if controls
        .iter()
        .find(|record| record.handle == handle)?
        .proc_id
        & 0x0fff
        != 16
    {
        return None;
    }
    let part = ppc_control_part_at_point(memory, controls, handle, v, h)?;
    let control = ppc_control_ptr(memory, handle)?;
    let (top, left, bottom, right) = ppc_read_rect(memory, control + PPC_CONTROL_RECT_OFFSET)?;
    let vertical = bottom.saturating_sub(top) >= right.saturating_sub(left);
    let axis_start = if vertical { top } else { left };
    let axis_end = if vertical { bottom } else { right };
    let arrow = (axis_end - axis_start).clamp(1, 16);
    let track_start = axis_start.saturating_add(arrow);
    let track_end = axis_end.saturating_sub(arrow);
    let min = memory.read_u16_be(control + PPC_CONTROL_MIN_OFFSET)? as i16;
    let max = memory.read_u16_be(control + PPC_CONTROL_MAX_OFFSET)? as i16;
    let value = memory.read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)? as i16;
    let page = (i32::from(track_end.saturating_sub(track_start)) / 16).max(1);
    let next = match part {
        20 => i32::from(value) - 1,
        21 => i32::from(value) + 1,
        22 => i32::from(value) - page,
        23 => i32::from(value) + page,
        129 => {
            let track = i32::from(track_end.saturating_sub(track_start)).max(1);
            let thumb = 8i32.min(track);
            let travel = track.saturating_sub(thumb).max(1);
            let coord = if vertical { v } else { h };
            let rel = (i32::from(coord) - i32::from(track_start)).clamp(0, travel);
            i32::from(min) + (rel * i32::from(max.saturating_sub(min)) + travel / 2) / travel
        }
        _ => return None,
    };
    let next = next.clamp(i32::from(min.min(max)), i32::from(min.max(max))) as i16;
    if next != value {
        memory.write_u16_be(control + PPC_CONTROL_VALUE_OFFSET, next as u16)?;
        let _ = ppc_draw_control(
            memory,
            handles,
            controls,
            gworlds,
            vfs_resources,
            current_resource_refnum,
            handle,
        );
    }
    Some(part)
}

pub(super) fn ppc_find_control_at_point(
    memory: &mut PpcSectionMem,
    controls: &[PpcControlRecord],
    owner: u32,
    v: i16,
    h: i16,
) -> Option<(u32, i16)> {
    ppc_window_control_handles(memory, owner)
        .into_iter()
        .find_map(|handle| {
            ppc_control_part_at_point(memory, controls, handle, v, h).map(|part| (handle, part))
        })
}

pub(super) fn ppc_popup_control_selected_text(
    memory: &mut PpcSectionMem,
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    menu_id: i16,
    selected: usize,
) -> Vec<u8> {
    // A menu created by NewMenu has no MENU resource to consult. The live
    // MenuHandle is also authoritative after SetItem/AppendMenu mutate an
    // existing menu, so prefer its guest bytes and use the resource only as
    // the fallback for resource-backed menus that have not been materialized.
    let current_menu_list = ppc_current_menu_list(memory);
    let menu_handle = ppc_get_menu_handle(memory, current_menu_list, menu_id);
    if let Ok(item_number) = i16::try_from(selected) {
        if let Some((item_address, item_length)) = ppc_menu_item(memory, menu_handle, item_number) {
            return ppc_memory_read_bytes(
                memory,
                item_address.saturating_add(1),
                u32::from(item_length),
            )
            .unwrap_or_default();
        }
    }

    ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"MENU"),
        menu_id,
        false,
    )
    .and_then(|index| ppc_decode_menu_items(&vfs_resources[index].data))
    .and_then(|(_, _, items)| selected.checked_sub(1).and_then(|i| items.get(i).cloned()))
    .map(|item| item.text)
    .unwrap_or_default()
}

pub(super) fn ppc_popup_control_display_title(
    title: &[u8],
    available_width: i16,
    text_font: i16,
    text_size: i16,
) -> Vec<u8> {
    if available_width <= 0 {
        return Vec::new();
    }
    if ppc_text_bytes_advance_for_font(title, text_font, text_size) <= available_width {
        return title.to_vec();
    }

    let ellipsis = b"...";
    let ellipsis_width = ppc_text_bytes_advance_for_font(ellipsis, text_font, text_size);
    if ellipsis_width > available_width {
        return Vec::new();
    }

    let mut prefix = Vec::new();
    let mut prefix_width = 0i16;
    for byte in title {
        let byte_width = ppc_text_byte_advance_for_font(*byte, text_font, text_size);
        if prefix_width
            .saturating_add(byte_width)
            .saturating_add(ellipsis_width)
            > available_width
        {
            break;
        }
        prefix.push(*byte);
        prefix_width = prefix_width.saturating_add(byte_width);
    }
    prefix.extend_from_slice(ellipsis);
    prefix
}

pub(super) fn ppc_draw_control(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    handle: u32,
) -> bool {
    ppc_draw_control_inner(
        memory,
        handles,
        controls,
        gworlds,
        vfs_resources,
        current_resource_refnum,
        handle,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_draw_window_controls(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    window: u32,
) -> bool {
    // Macintosh Toolbox Essentials (1992), pp. 5-87--5-88: DrawControls
    // draws every visible control in reverse order of creation. NewControl
    // prepends to wControlList, so walking the list head-first preserves the
    // documented overlap order (the first-created control is frontmost).
    let head = memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET))
        .unwrap_or(0);
    let control_handles = crate::control_manager::control_draw_order(head, |handle| {
        ppc_control_ptr(memory, handle)
            .and_then(|control| memory.read_u32_be(control.wrapping_add(PPC_CONTROL_NEXT_OFFSET)))
    });
    let mut drew = false;
    for handle in control_handles {
        drew |= ppc_draw_control(
            memory,
            handles,
            controls,
            gworlds,
            vfs_resources,
            current_resource_refnum,
            handle,
        );
    }
    drew
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_update_window_controls(
    memory: &mut PpcSectionMem,
    handles: &[PpcHandleRecord],
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    window: u32,
    update_rgn: u32,
) -> bool {
    // Macintosh Toolbox Essentials (1992), p. 5-88: UpdateControls draws
    // those controls in theWindow whose bounding rectangles intersect updateRgn.
    if window == 0 || update_rgn == 0 {
        return false;
    }
    let Some((ut, ul, ub, ur)) = regions::ppc_read_rgn_bbox(memory, update_rgn) else {
        return false;
    };
    if ut >= ub || ul >= ur {
        return false;
    }
    let head = memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_CONTROL_LIST_OFFSET))
        .unwrap_or(0);
    let control_handles = crate::control_manager::control_draw_order(head, |handle| {
        ppc_control_ptr(memory, handle)
            .and_then(|control| memory.read_u32_be(control.wrapping_add(PPC_CONTROL_NEXT_OFFSET)))
    });
    let mut drew = false;
    for handle in control_handles {
        let Some(ctrl_ptr) = ppc_control_ptr(memory, handle) else {
            continue;
        };
        let Some((ct, cl, cb, cr)) =
            ppc_read_rect(memory, ctrl_ptr.wrapping_add(PPC_CONTROL_RECT_OFFSET))
        else {
            continue;
        };
        if cb <= ut || cr <= ul || ct >= ub || cl >= ur {
            continue;
        }
        drew |= ppc_draw_control(
            memory,
            handles,
            controls,
            gworlds,
            vfs_resources,
            current_resource_refnum,
            handle,
        );
    }
    drew
}

pub(super) fn ppc_blit_theme_bitmap(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    port: u32,
    top: i16,
    left: i16,
    bitmap: &ThemeBitmap,
) -> bool {
    ppc_blit_theme_bitmap_masked(memory, gworlds, port, top, left, bitmap, None)
}

pub(super) fn ppc_blit_theme_bitmap_masked(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    port: u32,
    top: i16,
    left: i16,
    bitmap: &ThemeBitmap,
    transparent: Option<Rgb8>,
) -> bool {
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, port) else {
        return false;
    };
    let clip_storage = memory
        .read_u32_be(port.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
        .and_then(|clip_rgn| ppc_region_storage(memory, clip_rgn));
    let vis_storage = memory
        .read_u32_be(port.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|vis_rgn| ppc_region_storage(memory, vis_rgn));
    let mut pixels = HashMap::new();
    let rgba = bitmap.rgba();
    let mut wrote = false;
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            let offset = ((y * bitmap.width() + x) * 4) as usize;
            let rgb = Rgb8 {
                r: rgba[offset],
                g: rgba[offset + 1],
                b: rgba[offset + 2],
            };
            if transparent == Some(rgb) {
                continue;
            }
            let pixel = *pixels.entry((rgb.r, rgb.g, rgb.b)).or_insert_with(|| {
                ppc_quickdraw_surface_color_pixel(
                    memory,
                    surface,
                    PpcRgbColor {
                        red: u16::from(rgb.r) * 0x0101,
                        green: u16::from(rgb.g) * 0x0101,
                        blue: u16::from(rgb.b) * 0x0101,
                    },
                )
                .unwrap_or(0)
            });
            let port_h = i32::from(left) + x as i32;
            let port_v = i32::from(top) + y as i32;
            let point = surface.local_point((port_h, port_v));
            if ppc_local_point_in_port_regions(
                surface,
                point,
                vis_storage.as_deref(),
                clip_storage.as_deref(),
            ) {
                wrote |= ppc_quickdraw_write_raw_pixel(memory, surface.front_buffer, point, pixel);
            }
        }
    }
    wrote
}

/// Blend control ink halfway toward the window background for the 50% gray
/// System 7 uses to dim an inactive control.
fn ppc_dim_control_color(
    ink: crate::ui_theme::Rgb8,
    background: crate::ui_theme::Rgb8,
) -> crate::ui_theme::Rgb8 {
    let blend = |a: u8, b: u8| ((u16::from(a) + u16::from(b)) / 2) as u8;
    crate::ui_theme::Rgb8 {
        r: blend(ink.r, background.r),
        g: blend(ink.g, background.g),
        b: blend(ink.b, background.b),
    }
}

/// Font, size, face and ink for a control title after applying the control's
/// Appearance Manager ControlFontStyleRec.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct PpcControlTitleStyle {
    pub(super) font: i16,
    pub(super) size: i16,
    pub(super) face: u8,
    pub(super) foreground: Option<PpcRgbColor>,
}

/// Resolve a ControlFontStyleRec against the system font the painter uses by
/// default. Negative font values (and theme font IDs under
/// kControlUseThemeFontIDMask) select the Appearance meta fonts; the small
/// and view fonts are Geneva 10, the Mac OS 8 small system font metric
/// (Mac OS 8 Human Interface Guidelines (1997), p. 69).
/// kControlAddFontSizeMask adds `size` to the resolved size, and
/// kControlUseForeColorMask applies only to static text controls (Mac OS 8
/// Control Manager Reference, Control Font Style Flag Constants). Mode,
/// justification and the back colour are retained but not drawn.
pub(super) fn ppc_control_title_style(
    proc_id: i16,
    style: Option<&crate::control_manager::ControlFontStyle>,
) -> PpcControlTitleStyle {
    const USE_FONT: u16 = 0x0001;
    const USE_FACE: u16 = 0x0002;
    const USE_SIZE: u16 = 0x0004;
    const USE_FORE_COLOR: u16 = 0x0008;
    const USE_THEME_FONT_ID: u16 = 0x0080;
    const ADD_FONT_SIZE: u16 = 0x0100;
    const STATIC_TEXT_PROC: i16 = 288;
    const SYSTEM_SIZE: i16 = 12;
    const GENEVA: i16 = 3;
    const BOLD: u8 = 0x01;

    let mut resolved = PpcControlTitleStyle {
        font: PPC_QD_TEXT_FONT_DEFAULT,
        size: PPC_QD_TEXT_SIZE_SYSTEM,
        face: 0,
        foreground: None,
    };
    let Some(style) = style else {
        return resolved;
    };
    let flags = style.flags as u16;
    if flags & USE_FONT != 0 {
        // Meta font IDs are -1..-4 in the font field, or theme font IDs
        // 0..3 when kControlUseThemeFontIDMask is set.
        let meta = if flags & USE_THEME_FONT_ID != 0 {
            Some(style.font)
        } else {
            (style.font < 0).then(|| -style.font - 1)
        };
        match meta {
            Some(0) => {}
            Some(1 | 3) => (resolved.font, resolved.size) = (GENEVA, 10),
            Some(2) => (resolved.font, resolved.size, resolved.face) = (GENEVA, 10, BOLD),
            Some(_) => {}
            None => resolved.font = style.font,
        }
    }
    if flags & USE_FACE != 0 {
        resolved.face = style.style as u8;
    }
    if flags & ADD_FONT_SIZE != 0 {
        let base = if resolved.size == PPC_QD_TEXT_SIZE_SYSTEM {
            SYSTEM_SIZE
        } else {
            resolved.size
        };
        resolved.size = base.saturating_add(style.size).max(1);
    } else if flags & USE_SIZE != 0 && style.size > 0 {
        resolved.size = style.size;
    }
    if flags & USE_FORE_COLOR != 0 && proc_id == STATIC_TEXT_PROC {
        resolved.foreground = Some(PpcRgbColor {
            red: style.foreground[0],
            green: style.foreground[1],
            blue: style.foreground[2],
        });
    }
    resolved
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_draw_control_inner(
    memory: &mut PpcSectionMem,
    _handles: &[PpcHandleRecord],
    controls: &[PpcControlRecord],
    gworlds: &[PpcGWorldRecord],
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    handle: u32,
    draw_dialog_popup: bool,
) -> bool {
    let Some(control) = ppc_control_ptr(memory, handle) else {
        return false;
    };
    if memory
        .read_u8(control + PPC_CONTROL_VISIBLE_OFFSET)
        .unwrap_or(0)
        == 0
    {
        return true;
    }
    let owner = memory
        .read_u32_be(control + PPC_CONTROL_OWNER_OFFSET)
        .unwrap_or(0);
    // The screen ports are not windows and have no visibility flag.
    if owner != 0
        && !matches!(owner, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
        && !ppc_window_is_visible(memory, owner)
    {
        // Controls in an invisible window draw nothing onscreen; ShowWindow
        // then generates an update event so the application draws the
        // content region, controls included. Inside Macintosh: Macintosh
        // Toolbox Essentials (1992), pp. 4-88 and 5-25. PPC drawing targets
        // the shared front buffer, so Control Manager calls made before
        // ShowWindow would otherwise paint onto the desktop.
        return true;
    }
    let Some((top, left, bottom, right)) = ppc_read_rect(memory, control + PPC_CONTROL_RECT_OFFSET)
    else {
        return false;
    };
    let palette = ppc_ui_theme(gworlds).provider().palette();
    let record = controls.iter().find(|record| record.handle == handle);
    let proc_id = record.map_or(0, |record| record.proc_id) & 0x0fff;
    // Appearance Manager DeactivateControl dims a control without touching
    // contrlHilite. Draw its frame and title with the same 50% blend the 68K
    // control manager uses for an inactive title, so a non-hittable control
    // also looks non-hittable.
    let active = record.is_none_or(|record| record.active);
    let palette = if active {
        palette
    } else {
        crate::ui_theme::UiThemePalette {
            frame_dark: ppc_dim_control_color(palette.frame_dark, palette.window_background),
            ..palette
        }
    };
    let mut frame_cpu = PpcCpu::new();
    frame_cpu.gpr[3] = control + PPC_CONTROL_RECT_OFFSET;
    let is_default = ppc_ui_theme(gworlds) != UiThemeId::ClassicSystem7
        && proc_id == 0
        && memory.read_u16_be(owner + PPC_CWINDOW_WINDOW_KIND_OFFSET) == Some(2)
        && ppc_dialog_items_for_dialog(memory, _handles, owner).is_some_and(|items| {
            let index = memory
                .read_u16_be(owner + crate::dialog_manager::DIALOG_DEFAULT_ITEM_OFFSET)
                .unwrap_or(1);
            items
                .get(usize::from(index.saturating_sub(1)))
                .is_some_and(|item| memory.read_u32_be(item.handle) == Some(control))
        });
    let themed = ppc_draw_themed_control(
        memory,
        gworlds,
        owner,
        control,
        proc_id,
        is_default,
        active,
        (top, left, bottom, right),
    );
    let framed = if let Some(drawn) = themed {
        drawn
    } else {
        match proc_id {
            0 => {
                let slot = memory.presentation();
                let detail =
                    ppc_live_quickdraw_surface(memory, gworlds, owner).and_then(|surface| {
                        if !matches!(surface.front_buffer.depth, 8 | 16) {
                            return None;
                        }
                        let foreground = ppc_quickdraw_surface_fore_pixel(
                            memory,
                            surface,
                            ppc_theme_rgb(palette.frame_dark),
                            None,
                        )? as u32;
                        let background = ppc_quickdraw_surface_fore_pixel(
                            memory,
                            surface,
                            ppc_theme_rgb(palette.window_background),
                            None,
                        )? as u32;
                        let clip = memory
                            .read_u32_be(owner + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
                            .and_then(|rgn| ppc_region_storage(memory, rgn));
                        let vis = memory
                            .read_u32_be(owner + PPC_CGRAF_PORT_VIS_RGN_OFFSET)
                            .and_then(|rgn| ppc_region_storage(memory, rgn));
                        let fb = surface.front_buffer;
                        slot.rounded_control_corners(
                            surface.local_rect((top, left, bottom, right)),
                            crate::control_manager::STANDARD_BUTTON_OVAL.into(),
                            1,
                            fb.depth as u16,
                            Some(background),
                            foreground,
                            |x, y, lane| {
                                if x < 0
                                    || y < 0
                                    || x >= fb.width as i32
                                    || y >= fb.height as i32
                                    || !ppc_local_point_in_port_regions(
                                        surface,
                                        (x, y),
                                        vis.as_deref(),
                                        clip.as_deref(),
                                    )
                                {
                                    return None;
                                }
                                let address = fb.base_addr
                                    + y as u32 * fb.row_bytes
                                    + x as u32 * (fb.depth / 8)
                                    + lane;
                                Some((address, memory.read_u8(address)?))
                            },
                        )
                    });
                frame_cpu.gpr[4] = crate::control_manager::STANDARD_BUTTON_OVAL as u32;
                frame_cpu.gpr[5] = crate::control_manager::STANDARD_BUTTON_OVAL as u32;
                let _ = ppc_paint_round_rect(
                    &frame_cpu,
                    memory,
                    gworlds,
                    owner,
                    ppc_theme_rgb(palette.window_background),
                    None,
                );
                let framed = ppc_frame_round_rect(
                    &frame_cpu,
                    memory,
                    gworlds,
                    owner,
                    ppc_theme_rgb(palette.frame_dark),
                    None,
                );
                slot.finish_rounded_control(detail, |address| memory.read_u8(address).unwrap_or(0));
                framed
            }
            1 => {
                // Macintosh Toolbox Essentials (1992), pp. 5-15--5-16: the
                // standard checkbox CDEF draws a compact indicator at the left
                // of the control title and marks it when contrlValue is nonzero.
                let layout =
                    crate::control_manager::standard_checkbox_layout((top, left, bottom, right));
                let (box_top, box_left, box_bottom, box_right) = layout.indicator;
                let indicator_size = box_bottom.saturating_sub(box_top);
                let mut wrote = ppc_paint_rect_bounds(
                    memory,
                    gworlds,
                    owner,
                    layout.indicator,
                    ppc_theme_rgb(palette.window_background),
                    None,
                );
                wrote |= ppc_line_to(
                    memory,
                    gworlds,
                    owner,
                    (box_left, box_top),
                    (box_right.saturating_sub(1), box_top),
                    ppc_theme_rgb(palette.frame_dark),
                    None,
                );
                wrote |= ppc_line_to(
                    memory,
                    gworlds,
                    owner,
                    (box_right.saturating_sub(1), box_top),
                    (box_right.saturating_sub(1), box_bottom.saturating_sub(1)),
                    ppc_theme_rgb(palette.frame_dark),
                    None,
                );
                wrote |= ppc_line_to(
                    memory,
                    gworlds,
                    owner,
                    (box_right.saturating_sub(1), box_bottom.saturating_sub(1)),
                    (box_left, box_bottom.saturating_sub(1)),
                    ppc_theme_rgb(palette.frame_dark),
                    None,
                );
                wrote |= ppc_line_to(
                    memory,
                    gworlds,
                    owner,
                    (box_left, box_bottom.saturating_sub(1)),
                    (box_left, box_top),
                    ppc_theme_rgb(palette.frame_dark),
                    None,
                );
                if memory
                    .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
                    .unwrap_or(0)
                    != 0
                {
                    crate::control_manager::for_each_standard_checkbox_mark_pixel(
                        indicator_size,
                        |x, y| {
                            wrote |= ppc_line_to(
                                memory,
                                gworlds,
                                owner,
                                (box_left.saturating_add(x), box_top.saturating_add(y)),
                                (box_left.saturating_add(x), box_top.saturating_add(y)),
                                ppc_theme_rgb(palette.frame_dark),
                                None,
                            );
                        },
                    );
                }
                wrote
            }
            2 => {
                // Inside Macintosh Volume I (1985), p. I-322: radioButProc is a
                // round indicator whose on state is a small filled black circle.
                let layout = crate::control_manager::standard_radio_button_layout((
                    top, left, bottom, right,
                ));
                let indicator = layout.indicator;
                let mut wrote = ppc_draw_oval_bounds(
                    memory,
                    gworlds,
                    owner,
                    indicator,
                    ppc_theme_rgb(palette.window_background),
                    None,
                    false,
                );
                wrote |= ppc_draw_oval_bounds(
                    memory,
                    gworlds,
                    owner,
                    indicator,
                    ppc_theme_rgb(palette.frame_dark),
                    None,
                    true,
                );
                if memory
                    .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
                    .unwrap_or(0)
                    != 0
                {
                    let (dot_top, dot_left, dot_bottom, dot_right) = indicator;
                    wrote |= ppc_draw_oval_bounds(
                        memory,
                        gworlds,
                        owner,
                        (
                            dot_top.saturating_add(3),
                            dot_left.saturating_add(3),
                            dot_bottom.saturating_sub(3),
                            dot_right.saturating_sub(3),
                        ),
                        ppc_theme_rgb(palette.frame_dark),
                        None,
                        false,
                    );
                }
                wrote
            }
            16 => {
                // Both CPU adapters submit the same ControlRecord state to the
                // architecture-neutral presentation provider. Only this final
                // guest-framebuffer blit remains adapter-specific.
                let value = memory
                    .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
                    .unwrap_or(0) as i16;
                let min = memory
                    .read_u16_be(control + PPC_CONTROL_MIN_OFFSET)
                    .unwrap_or(0) as i16;
                let max = memory
                    .read_u16_be(control + PPC_CONTROL_MAX_OFFSET)
                    .unwrap_or(0) as i16;
                let hilite = memory
                    .read_u8(control + PPC_CONTROL_HILITE_OFFSET)
                    .unwrap_or(0);
                let bitmap = render_scrollbar_bitmap(
                    ppc_ui_theme(gworlds),
                    right.saturating_sub(left),
                    bottom.saturating_sub(top),
                    value,
                    min,
                    max,
                    hilite,
                );
                ppc_blit_theme_bitmap(memory, gworlds, owner, top, left, &bitmap)
            }
            1008..=1023 => {
                // The standard pop-up CDEF is resource ID 63, whose proc IDs
                // occupy 63 << 4 through 63 << 4 | 15. Its contrlMin field is
                // the MENU resource ID and contrlValue is the one-based item.
                let dialog_bounds = memory
                    .read_u16_be(owner + PPC_CWINDOW_WINDOW_KIND_OFFSET)
                    .filter(|kind| *kind == 2)
                    .and_then(|_| ppc_dialog_global_bounds(memory, gworlds, owner));
                if dialog_bounds.is_some() && !draw_dialog_popup {
                    return true;
                }
                // Draw in the owning port so its visible and clipping regions
                // also apply to controls outside or partly outside a dialog.
                // Macintosh Toolbox Essentials (1992), Display Rectangles, ch. 6.
                let (draw_owner, (draw_top, draw_left, draw_bottom, draw_right)) =
                    (owner, (top, left, bottom, right));
                // popupMenuProc reserves `contrlMax` pixels for the label before
                // the button. A fixed-width popup uses the rest of the control
                // rect as its stable button width; omitting this offset makes
                // the PPC renderer paint the button over its label and gives its
                // selected title an incorrectly large content area. Macintosh
                // Toolbox Essentials (1992), pp. 5-25--5-27.
                let title_width = record
                    .and_then(|record| record.popup_title_width)
                    .unwrap_or(0)
                    .max(0);
                let draw_left = draw_left.saturating_add(title_width);
                let draw_top = draw_top.saturating_add(1);
                let draw_bottom = draw_bottom.saturating_sub(2);
                let draw_right = draw_right.saturating_sub(1);
                let mut wrote;
                let enabled = memory
                    .read_u8(control + PPC_CONTROL_HILITE_OFFSET)
                    .unwrap_or(0)
                    != 255;
                if ppc_draw_themed_control_rect(
                    memory,
                    gworlds,
                    draw_owner,
                    (draw_top, draw_left, draw_bottom, draw_right),
                    crate::ui_theme::ControlKind::PopupButton,
                    enabled,
                    false,
                ) {
                    wrote = true;
                } else {
                    wrote = ppc_paint_rect_bounds(
                        memory,
                        gworlds,
                        draw_owner,
                        (draw_top, draw_left, draw_bottom, draw_right),
                        ppc_theme_rgb(palette.window_background),
                        None,
                    );
                    for (start, end) in [
                        (
                            (draw_left, draw_top),
                            (draw_right.saturating_sub(1), draw_top),
                        ),
                        (
                            (draw_right.saturating_sub(1), draw_top),
                            (draw_right.saturating_sub(1), draw_bottom.saturating_sub(1)),
                        ),
                        (
                            (draw_right.saturating_sub(1), draw_bottom.saturating_sub(1)),
                            (draw_left, draw_bottom.saturating_sub(1)),
                        ),
                        (
                            (draw_left, draw_bottom.saturating_sub(1)),
                            (draw_left, draw_top),
                        ),
                    ] {
                        wrote |= ppc_line_to(
                            memory,
                            gworlds,
                            draw_owner,
                            start,
                            end,
                            ppc_theme_rgb(palette.frame_dark),
                            None,
                        );
                    }
                    let arrow_left = draw_right.saturating_sub(18).max(draw_left);
                    wrote |= ppc_line_to(
                        memory,
                        gworlds,
                        draw_owner,
                        (arrow_left, draw_top),
                        (arrow_left, draw_bottom.saturating_sub(1)),
                        ppc_theme_rgb(palette.frame_dark),
                        None,
                    );
                    let arrow_h = draw_right.saturating_sub(9);
                    let center_v =
                        draw_top.saturating_add(draw_bottom.saturating_sub(draw_top) / 2);
                    for offset in 0..3i16 {
                        wrote |= ppc_line_to(
                            memory,
                            gworlds,
                            draw_owner,
                            (
                                arrow_h.saturating_sub(offset),
                                center_v.saturating_sub(3 - offset),
                            ),
                            (
                                arrow_h.saturating_add(offset),
                                center_v.saturating_sub(3 - offset),
                            ),
                            ppc_theme_rgb(palette.frame_dark),
                            None,
                        );
                        wrote |= ppc_line_to(
                            memory,
                            gworlds,
                            draw_owner,
                            (
                                arrow_h.saturating_sub(offset),
                                center_v.saturating_add(3 - offset),
                            ),
                            (
                                arrow_h.saturating_add(offset),
                                center_v.saturating_add(3 - offset),
                            ),
                            ppc_theme_rgb(palette.frame_dark),
                            None,
                        );
                    }
                }
                let selected = memory
                    .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
                    .unwrap_or(0) as usize;
                let menu_id = record.map_or(0, |record| record.popup_menu_id);
                let selected_text = ppc_popup_control_selected_text(
                    memory,
                    vfs_resources,
                    current_resource_refnum,
                    menu_id,
                    selected,
                );
                let text_left = draw_left.saturating_add(5);
                let text_right = draw_right.saturating_sub(19).max(text_left);
                let display_title = ppc_popup_control_display_title(
                    &selected_text,
                    text_right.saturating_sub(text_left),
                    PPC_QD_TEXT_FONT_DEFAULT,
                    PPC_QD_TEXT_SIZE_SYSTEM,
                );
                if !display_title.is_empty() {
                    let _ = ppc_draw_text_bytes(
                        memory,
                        gworlds,
                        draw_owner,
                        (text_left, draw_top.saturating_add(14)),
                        PPC_QD_TEXT_FONT_DEFAULT,
                        PPC_QD_TEXT_SIZE_SYSTEM,
                        PPC_QD_TEXT_MODE_SRC_OR,
                        ppc_theme_rgb(palette.frame_dark),
                        None,
                        &display_title,
                    );
                }
                wrote
            }
            _ => ppc_frame_rect(
                &frame_cpu,
                memory,
                gworlds,
                owner,
                ppc_theme_rgb(palette.frame_dark),
                None,
            ),
        }
    };
    let title =
        ppc_read_pstring_bytes(memory, control + PPC_CONTROL_TITLE_OFFSET).unwrap_or_default();
    if !title.is_empty() {
        let title = title
            .into_iter()
            .flat_map(|byte| {
                if byte == 0xc9 {
                    vec![b'.', b'.', b'.']
                } else {
                    vec![byte]
                }
            })
            .collect::<Vec<_>>();
        let title_style = ppc_control_title_style(
            proc_id,
            record.and_then(|record| record.font_style.as_ref()),
        );
        let advance =
            ppc_text_width_bytes(title_style.font, title_style.size, title_style.face, &title);
        let metrics = get_font_metrics(title_style.font, title_style.size);
        let (centered_h, centered_v) = crate::control_manager::centered_control_label_origin(
            (top, left, bottom, right),
            advance,
            metrics.ascent,
            metrics.descent,
        );
        let popup = (1008..=1023).contains(&proc_id);
        let (title_h, title) = if popup {
            // Popup CDEF labels are right-aligned immediately before the
            // button's reserved title-width region, matching the 68K
            // draw_popup_control_label path. Keep the label out of the
            // selected-item content area.
            let title_width = record
                .and_then(|record| record.popup_title_width)
                .unwrap_or(0)
                .max(0);
            let popup_left = left.saturating_add(title_width);
            let text_right = popup_left.saturating_sub(6);
            let available_width = text_right.saturating_sub(left);
            (
                text_right.saturating_sub(advance).max(left),
                ppc_popup_control_display_title(
                    &title,
                    available_width,
                    title_style.font,
                    title_style.size,
                ),
            )
        } else {
            let title_h = match proc_id {
                0 => centered_h,
                1 => {
                    crate::control_manager::standard_checkbox_layout((top, left, bottom, right))
                        .label_left
                }
                2 => {
                    crate::control_manager::standard_radio_button_layout((top, left, bottom, right))
                        .label_left
                }
                160..=166 => left.saturating_add(8),
                _ => left.saturating_add(16),
            };
            (title_h, title)
        };
        let title_v = if (160..=166).contains(&proc_id) {
            // Group box titles straddle the top border instead of centring in
            // the box, so they do not overlap the enclosed controls.
            top.saturating_add(metrics.ascent)
                .saturating_sub(metrics.ascent.saturating_add(metrics.descent) / 2)
                .max(top)
        } else {
            centered_v.min(bottom.saturating_sub(1))
        };
        // An inactive control keeps the dimmed palette ink over its own
        // foreground colour, so it still reads as disabled.
        let title_color = title_style
            .foreground
            .filter(|_| active)
            .unwrap_or_else(|| ppc_theme_rgb(palette.frame_dark));
        let _ = ppc_draw_text_bytes_styled(
            memory,
            gworlds,
            owner,
            (title_h, title_v),
            title_style.font,
            title_style.size,
            PPC_QD_TEXT_MODE_SRC_OR,
            title_color,
            None,
            title_style.face,
            &title,
        );
    }
    framed
}
