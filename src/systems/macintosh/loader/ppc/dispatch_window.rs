//! Typed Window Manager dispatch for PowerPC imports.

use super::*;
use std::collections::VecDeque;

pub(crate) const PPC_CWINDOW_PORT_RECT_OFFSET: u32 = 16;
pub(crate) const PPC_CWINDOW_WINDOW_KIND_OFFSET: u32 = crate::window_manager::WINDOW_KIND_OFFSET;
pub(crate) const PPC_CWINDOW_VISIBLE_OFFSET: u32 = crate::window_manager::WINDOW_VISIBLE_FLAG_OFFSET;
pub(crate) const PPC_CWINDOW_HILITED_OFFSET: u32 = crate::window_manager::WINDOW_HILITED_FLAG_OFFSET;
pub(crate) const PPC_CWINDOW_GO_AWAY_OFFSET: u32 = crate::window_manager::WINDOW_GO_AWAY_FLAG_OFFSET;
pub(crate) const PPC_CWINDOW_SPARE_OFFSET: u32 = crate::window_manager::WINDOW_SPARE_FLAG_OFFSET;
pub(crate) const PPC_CWINDOW_STRUCTURE_RGN_OFFSET: u32 = crate::window_manager::WINDOW_STRUCTURE_RGN_OFFSET;
pub(crate) const PPC_CWINDOW_CONTENT_RGN_OFFSET: u32 = crate::window_manager::WINDOW_CONTENT_RGN_OFFSET;
pub(crate) const PPC_CWINDOW_UPDATE_RGN_OFFSET: u32 = crate::window_manager::WINDOW_UPDATE_RGN_OFFSET;
pub(crate) const PPC_CWINDOW_NEXT_WINDOW_OFFSET: u32 = crate::window_manager::WINDOW_NEXT_WINDOW_OFFSET;
pub(crate) const PPC_CWINDOW_WINDOW_PIC_OFFSET: u32 = crate::window_manager::WINDOW_PIC_HANDLE_OFFSET;
pub(crate) const PPC_CWINDOW_DEF_PROC_OFFSET: u32 = crate::window_manager::WINDOW_DEF_PROC_HANDLE_OFFSET;
pub(crate) const PPC_CWINDOW_STATE_HANDLE_OFFSET: u32 = crate::window_manager::WINDOW_STATE_DATA_HANDLE_OFFSET;
pub(crate) const PPC_CWINDOW_TITLE_HANDLE_OFFSET: u32 = 134;
pub(crate) const PPC_CWINDOW_TITLE_WIDTH_OFFSET: u32 = crate::window_manager::WINDOW_TITLE_WIDTH_OFFSET;
pub(crate) const PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET: u32 = crate::window_manager::WINDOW_REF_CON_OFFSET;
pub(crate) const PPC_CWINDOW_COLOR_TABLE_HANDLE_OFFSET: u32 = 164;
pub(crate) const PPC_CWINDOW_CONTROL_LIST_OFFSET: u32 = 140;
pub(crate) const PPC_LM_WINDOW_LIST_ADDR: u32 = crate::window_manager::LM_WINDOW_LIST_ADDR;
pub(crate) const PPC_LM_AUX_WIN_HEAD_ADDR: u32 = crate::window_manager::LM_AUX_WIN_HEAD_ADDR;
pub(super) const PPC_WINDOW_PROPERTY_NOT_FOUND_ERR: i16 =
    crate::window_manager::WINDOW_PROPERTY_NOT_FOUND_ERR;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcGoAwayCall {
    pub(crate) window: u32,
    pub(crate) start_point: u32,
    pub(crate) stack_pointer: u32,
    pub(crate) return_address: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcGoAwayTrackingState {
    pub(crate) call: PpcGoAwayCall,
    pub(crate) surface: PpcQuickDrawSurface,
    pub(crate) saved_pixels: crate::memory::SavedPixels<u16>,
    pub(crate) highlighted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcDragWindowCall {
    pub(crate) window: u32,
    pub(crate) start_point: u32,
    pub(crate) bounds_ptr: u32,
    pub(crate) stack_pointer: u32,
    pub(crate) return_address: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcDragWindowTrackingState {
    pub(crate) call: PpcDragWindowCall,
    pub(crate) front_buffer: PpcFrontBuffer,
    pub(crate) original_content: (i16, i16, i16, i16),
    pub(crate) original_structure: (i16, i16, i16, i16),
    pub(crate) bounds: (i16, i16, i16, i16),
    pub(crate) outline: (i16, i16, i16, i16),
    pub(crate) saved_pixels: crate::memory::SavedPixels<(i32, i32, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcGrowWindowCall {
    pub(crate) window: u32,
    pub(crate) start_point: u32,
    pub(crate) size_rect_ptr: u32,
    pub(crate) stack_pointer: u32,
    pub(crate) return_address: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcGrowWindowTrackingState {
    pub(crate) call: PpcGrowWindowCall,
    pub(crate) front_buffer: PpcFrontBuffer,
    pub(crate) original_content: (i16, i16, i16, i16),
    pub(crate) size_limits: (i16, i16, i16, i16),
    pub(crate) outline: (i16, i16, i16, i16),
    pub(crate) saved_pixels: crate::memory::SavedPixels<(i32, i32, u16)>,
}

pub(super) struct PpcWindowDispatchContext<'a> {
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
    pub(super) window_list: &'a SharedProcessWindowList,
    pub(super) draw_sprocket: &'a PpcDrawSprocketState,
    pub(super) current_gworld: &'a mut u32,
    pub(super) current_gdevice: &'a mut u32,
    pub(super) quickdraw_fore_color: &'a mut PpcRgbColor,
    pub(super) quickdraw_fore_indices: &'a mut HashMap<u32, u8>,
    pub(super) quickdraw_back_color: &'a mut PpcRgbColor,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) input: PpcInputSnapshot,
    pub(super) tick_count: u32,
    pub(super) event_queue: &'a mut EventQueue,
    pub(super) vfs_resources: &'a mut [PpcVfsResourceRecord],
    pub(super) current_resource_refnum: i16,
    pub(super) last_resource_error: &'a mut i16,
}

pub(super) fn dispatch_window_import(
    context: PpcWindowDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcWindowDispatchContext {
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
        window_list,
        draw_sprocket,
        current_gworld,
        current_gdevice,
        quickdraw_fore_color,
        quickdraw_fore_indices,
        quickdraw_back_color,
        screen_clut,
        color_manager_clut,
        toolbox_startup,
        input,
        tick_count,
        event_queue,
        vfs_resources,
        current_resource_refnum,
        last_resource_error,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::InitWindows => {
            toolbox_startup.windows_initialized = true;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::NewCWindow => {
            let previous_front = ppc_front_visible_process_window(memory, window_list);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let window = ppc_new_window_from_cpu(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                window_list,
                *current_gdevice,
                toolbox_startup.host_menu_bar_hidden,
            );
            if window != 0 {
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                quickdraw_fore_indices.remove(&window);
                *current_gworld = window;
                *current_gdevice =
                    ppc_gworld_device(gworlds, *current_gworld).unwrap_or(*current_gdevice);
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
                let next_front = ppc_front_visible_process_window(memory, window_list);
                ppc_enqueue_window_activation_transition(
                    memory,
                    event_queue,
                    previous_front,
                    next_front,
                    tick_count,
                );
                if next_front != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
            }
            Some(PpcImportAction::Return(window))
        }
        PpcImportDispatcherTarget::GetNewCWindow => {
            let previous_front = ppc_front_visible_process_window(memory, window_list);
            let window = ppc_get_new_cwindow(
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
                vfs_resources,
                current_resource_refnum,
                last_resource_error,
                toolbox_startup.host_menu_bar_hidden,
            );
            if window != 0 {
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                quickdraw_fore_indices.remove(&window);
                *current_gworld = window;
                *current_gdevice =
                    ppc_gworld_device(gworlds, *current_gworld).unwrap_or(*current_gdevice);
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
                let next_front = ppc_front_visible_process_window(memory, window_list);
                ppc_enqueue_window_activation_transition(
                    memory,
                    event_queue,
                    previous_front,
                    next_front,
                    tick_count,
                );
                if next_front != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
            }
            Some(PpcImportAction::Return(window))
        }
        PpcImportDispatcherTarget::GetWRefCon => {
            let window = cpu.gpr[3];
            let ref_con = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_w_ref_con(window, ref_con);
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::SetWRefCon => {
            let window = cpu.gpr[3];
            let ref_con = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(
                memory,
                window.wrapping_add(PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET),
                4,
            );
            if let Ok(params) =
                crate::window_manager::evaluate_set_w_ref_con_parameters(window, ref_con, can_write)
            {
                let _ = memory.write_u32_be(
                    params
                        .window_ptr()
                        .wrapping_add(PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET),
                    params.ref_con(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetWindowPic => {
            let window = cpu.gpr[3];
            let pic_handle = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_WINDOW_PIC_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_pic(window, pic_handle);
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::SetWindowPic => {
            let window = cpu.gpr[3];
            let pic = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(
                memory,
                window.wrapping_add(PPC_CWINDOW_WINDOW_PIC_OFFSET),
                4,
            );
            if let Ok(params) =
                crate::window_manager::evaluate_set_window_pic_parameters(window, pic, can_write)
            {
                let _ = memory.write_u32_be(
                    params
                        .window_ptr()
                        .wrapping_add(PPC_CWINDOW_WINDOW_PIC_OFFSET),
                    params.pic_handle(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetAuxWin => {
            let window_ptr = cpu.gpr[3];
            let aw_ctable_ptr = cpu.gpr[4];
            let can_write = aw_ctable_ptr == 0
                || ppc_memory_can_write_bytes(memory, aw_ctable_ptr, 4);
            if let Ok(params) = crate::window_manager::evaluate_get_aux_win_parameters(
                window_ptr,
                aw_ctable_ptr,
                can_write,
            ) {
                let is_tracked = window_list.contains_window(params.window_ptr());
                let color_table = ppc_window_color_table_handle(memory, params.window_ptr());
                let (ctab, success) =
                    crate::window_manager::evaluate_get_aux_win(color_table, is_tracked);
                if params.out_aux_ctable_ptr() != 0 {
                    let _ = memory.write_u32_be(params.out_aux_ctable_ptr(), ctab);
                }
                Some(PpcImportAction::Return(if success { 1 } else { 0 }))
            } else {
                Some(PpcImportAction::Return(0))
            }
        }
        PpcImportDispatcherTarget::LMGetWindowList => {
            let first = window_list.first();
            let low_mem = memory.read_u32_be(PPC_LM_WINDOW_LIST_ADDR);
            let window = crate::window_manager::evaluate_lm_get_window_list(first, low_mem);
            Some(PpcImportAction::Return(window))
        }
        PpcImportDispatcherTarget::LMSetWindowList => {
            let window = cpu.gpr[3];
            let can_write = ppc_memory_can_write_bytes(memory, PPC_LM_WINDOW_LIST_ADDR, 4);
            if let Ok(params) =
                crate::window_manager::evaluate_lm_set_window_list_parameters(window, can_write)
            {
                if params.window_ptr() != 0 {
                    window_list.bring_to_front(params.window_ptr());
                }
                ppc_sync_process_window_list(memory, window_list);
                let _ = memory.write_u32_be(PPC_LM_WINDOW_LIST_ADDR, params.window_ptr());
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::LMGetAuxWinHead => {
            let aux_head = memory.read_u32_be(PPC_LM_AUX_WIN_HEAD_ADDR);
            let result = crate::window_manager::evaluate_lm_get_aux_win_head(aux_head);
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::LMSetAuxWinHead => {
            let aux_head = cpu.gpr[3];
            let can_write = ppc_memory_can_write_bytes(memory, PPC_LM_AUX_WIN_HEAD_ADDR, 4);
            if let Ok(params) =
                crate::window_manager::evaluate_lm_set_aux_win_head_parameters(aux_head, can_write)
            {
                let _ = memory.write_u32_be(PPC_LM_AUX_WIN_HEAD_ADDR, params.aux_head());
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SizeWindow => {
            if let Some(params) = crate::window_manager::evaluate_size_window_parameters(
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
                cpu.gpr[5] as u16 as i16,
                cpu.gpr[6] != 0,
            ) {
                let window = params.window_ptr();
                let was_visible = ppc_window_is_visible(memory, window);
                let previous_structure = ppc_window_global_structure_bounds(memory, gworlds, window);
                if ppc_size_window(cpu, memory, gworlds).is_some() {
                    ppc_recalculate_window_vis_regions(
                        process_memory_manager,
                        memory,
                        window_list,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                    );
                    let next_structure = ppc_window_global_structure_bounds(memory, gworlds, window);
                    ppc_repaint_window_geometry_transition(
                        memory,
                        gworlds,
                        window_list,
                        window,
                        was_visible,
                        previous_structure,
                        next_structure,
                        toolbox_startup.host_menu_bar_hidden,
                        event_queue,
                        tick_count,
                        input,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::MoveWindow => {
            if let Some(params) = crate::window_manager::evaluate_move_window_parameters(
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
                cpu.gpr[5] as u16 as i16,
                cpu.gpr[6] != 0,
            ) {
                let window = params.window_ptr();
                let was_visible = ppc_window_is_visible(memory, window);
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                let bring_to_front = params.bring_to_front();
                let previous_structure = ppc_window_global_structure_bounds(memory, gworlds, window);
                if ppc_move_window(cpu, memory, gworlds).is_some() {
                    if bring_to_front {
                        ppc_reorder_window(gworlds, window_list, window, 0, true);
                    }
                    ppc_recalculate_window_vis_regions(
                        process_memory_manager,
                        memory,
                        window_list,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                    );
                    let next_structure = ppc_window_global_structure_bounds(memory, gworlds, window);
                    ppc_repaint_window_geometry_transition(
                        memory,
                        gworlds,
                        window_list,
                        window,
                        was_visible,
                        previous_structure,
                        next_structure,
                        toolbox_startup.host_menu_bar_hidden,
                        event_queue,
                        tick_count,
                        input,
                    );
                    ppc_transition_front_window_chrome(
                        memory,
                        gworlds,
                        window_list,
                        previous_front,
                        toolbox_startup.host_menu_bar_hidden,
                    );
                    let next_front = ppc_front_visible_process_window(memory, window_list);
                    if bring_to_front && next_front == Some(window) {
                        *current_gworld = window;
                        *current_gdevice =
                            ppc_gworld_device(gworlds, *current_gworld).unwrap_or(*current_gdevice);
                        ppc_register_gdevice(toolbox_startup, *current_gdevice);
                        ppc_restore_port_colors(
                            memory,
                            *current_gworld,
                            quickdraw_fore_color,
                            quickdraw_back_color,
                        );
                        let _ = ppc_set_window_hilited(memory, window, true);
                    }
                    if next_front != previous_front {
                        let _ = ppc_activate_front_window_palette(
                            memory,
                            gworlds,
                            *current_gdevice,
                            screen_clut,
                            color_manager_clut,
                            toolbox_startup,
                        );
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ShowWindow => {
            if let Some(params) = crate::window_manager::evaluate_show_window_parameters(cpu.gpr[3]) {
                let window = params.window_ptr();
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                let was_visible = ppc_window_is_visible(memory, window);
                let _ = ppc_set_window_visible(memory, window, true);
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                ppc_transition_front_window_chrome(
                    memory,
                    gworlds,
                    window_list,
                    previous_front,
                    toolbox_startup.host_menu_bar_hidden,
                );
                let next_front = ppc_front_visible_process_window(memory, window_list);
                ppc_enqueue_window_activation_transition(
                    memory,
                    event_queue,
                    previous_front,
                    next_front,
                    tick_count,
                );
                if !was_visible {
                    if let Some(port_rect) = ppc_read_rect(memory, window.wrapping_add(16)) {
                        ppc_invalidate_window_local_rect(memory, window, port_rect);
                    }
                    // Macintosh Toolbox Essentials (1992), Window Manager,
                    // PaintOne: newly exposed content uses its window color table.
                    let content_color = ppc_window_color_table_handle(memory, window)
                        .and_then(|handle| ppc_window_content_color(memory, handle));
                    if let (Some(rect), Some(color)) = (
                        ppc_read_rect(memory, window.wrapping_add(PPC_CWINDOW_PORT_RECT_OFFSET)),
                        content_color,
                    ) {
                        let _ = ppc_paint_window_background_bounds(memory, gworlds, window, rect, color);
                    }
                    if ppc_front_visible_process_window(memory, window_list) != Some(window) {
                        ppc_draw_existing_window_frame(
                            memory,
                            gworlds,
                            window_list,
                            window,
                            toolbox_startup.host_menu_bar_hidden,
                        );
                    }
                    ppc_enqueue_window_update_event(event_queue, window, tick_count, input);
                }
                if ppc_front_visible_process_window(memory, window_list) != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
                if ppc_gworld_trace_enabled() {
                    eprintln!(
                        "[PPC-GWORLD-TRACE] ShowWindow window=${:08X} current=${:08X}",
                        window, *current_gworld
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::HideWindow => {
            if let Some(params) = crate::window_manager::evaluate_hide_window_parameters(cpu.gpr[3]) {
                let window = params.window_ptr();
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                let exposed = ppc_window_is_visible(memory, window)
                    .then(|| {
                        memory
                            .read_u32_be(window.wrapping_add(PPC_CWINDOW_STRUCTURE_RGN_OFFSET))
                            .and_then(|region| ppc_read_rgn_bbox(memory, region))
                    })
                    .flatten();
                let _ = ppc_set_window_visible(memory, window, false);
                let _ = ppc_set_window_hilited(memory, window, false);
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                let next_front = ppc_front_visible_process_window(memory, window_list);
                ppc_enqueue_window_activation_transition(
                    memory,
                    event_queue,
                    previous_front,
                    next_front,
                    tick_count,
                );
                ppc_transition_front_window_chrome(
                    memory,
                    gworlds,
                    window_list,
                    previous_front,
                    toolbox_startup.host_menu_bar_hidden,
                );
                ppc_restore_window_removal_exposure(
                    memory,
                    gworlds,
                    window_list,
                    exposed,
                    toolbox_startup.host_menu_bar_hidden,
                    event_queue,
                    tick_count,
                    input,
                );
                if next_front != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
                if ppc_gworld_trace_enabled() {
                    eprintln!(
                        "[PPC-GWORLD-TRACE] HideWindow window=${:08X} current=${:08X}",
                        window, *current_gworld
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ShowHide => {
            if let Some(params) = crate::window_manager::evaluate_show_hide_parameters(cpu.gpr[3], cpu.gpr[4] != 0) {
                let window = params.window_ptr();
                let visible = params.show();
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                let was_visible = ppc_window_is_visible(memory, window);
                let exposed = (was_visible && !visible)
                    .then(|| {
                        memory
                            .read_u32_be(window.wrapping_add(PPC_CWINDOW_STRUCTURE_RGN_OFFSET))
                            .and_then(|region| ppc_read_rgn_bbox(memory, region))
                    })
                    .flatten();
                let _ = ppc_set_window_visible(memory, window, visible);
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                ppc_transition_front_window_chrome(
                    memory,
                    gworlds,
                    window_list,
                    previous_front,
                    toolbox_startup.host_menu_bar_hidden,
                );
                let next_front = ppc_front_visible_process_window(memory, window_list);
                ppc_enqueue_window_activation_transition(
                    memory,
                    event_queue,
                    previous_front,
                    next_front,
                    tick_count,
                );
                ppc_restore_window_removal_exposure(
                    memory,
                    gworlds,
                    window_list,
                    exposed,
                    toolbox_startup.host_menu_bar_hidden,
                    event_queue,
                    tick_count,
                    input,
                );
                if visible && !was_visible {
                    if ppc_front_visible_process_window(memory, window_list) != Some(window) {
                        ppc_draw_existing_window_frame(
                            memory,
                            gworlds,
                            window_list,
                            window,
                            toolbox_startup.host_menu_bar_hidden,
                        );
                    }
                }
                if ppc_front_visible_process_window(memory, window_list) != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
                if ppc_gworld_trace_enabled() {
                    eprintln!(
                        "[PPC-GWORLD-TRACE] ShowHide window=${:08X} visible={} current=${:08X}",
                        window, visible, *current_gworld
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CloseWindow => {
            if let Some(params) = crate::window_manager::evaluate_close_window_parameters(cpu.gpr[3]) {
                let window = params.window_ptr();
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
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FrontWindow => {
            let front_visible = ppc_front_visible_process_window(memory, window_list);
            let window = crate::window_manager::evaluate_front_window(
                draw_sprocket.blanking_window,
                front_visible,
            );
            Some(PpcImportAction::Return(window))
        }
        PpcImportDispatcherTarget::SetWinColor => {
            if let Some(params) =
                crate::window_manager::evaluate_set_win_color_parameters(cpu.gpr[3], cpu.gpr[4])
            {
                let window = params.window_ptr();
                let color_table = params.color_table_ptr();
                let storage = if window == 0 { PPC_MAIN_GWORLD } else { window };
                if !ppc_window_is_dialog(memory, storage) {
                    let _ = memory.write_u32_be(
                        storage.wrapping_add(PPC_CWINDOW_COLOR_TABLE_HANDLE_OFFSET),
                        color_table,
                    );
                }
                if window != 0 {
                    if let Some(content_color) = ppc_window_content_color(memory, color_table) {
                        if window == *current_gworld {
                            *quickdraw_back_color = content_color;
                            let _ = ppc_write_port_rgb_color(
                                memory,
                                *current_gworld,
                                PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET,
                                content_color,
                            );
                        }
                        if ppc_window_is_visible(memory, window) {
                            if let Some(bounds) = ppc_read_rect(memory, window.wrapping_add(16)) {
                                let _ = ppc_paint_rect_bounds(
                                    memory,
                                    gworlds,
                                    window,
                                    bounds,
                                    content_color,
                                    None,
                                );
                            }
                        }
                    }
                    ppc_enqueue_window_update_event(event_queue, window, tick_count, input);
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PaintOne => {
            let params =
                crate::window_manager::evaluate_paint_one_parameters(cpu.gpr[3], cpu.gpr[4]);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_paint_one(
                Some(&mut allocator),
                memory,
                gworlds,
                params.window_ptr(),
                params.clobbered_rgn_ptr(),
                toolbox_startup.host_menu_bar_hidden,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::PaintBehind => {
            let params =
                crate::window_manager::evaluate_paint_behind_parameters(cpu.gpr[3], cpu.gpr[4]);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_paint_behind(
                Some(&mut allocator),
                memory,
                gworlds,
                params.window_ptr(),
                params.clobbered_rgn_ptr(),
                toolbox_startup.host_menu_bar_hidden,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::CalcVisBehind => {
            let params =
                crate::window_manager::evaluate_calc_vis_behind_parameters(cpu.gpr[3], cpu.gpr[4]);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_calc_vis_behind(
                Some(&mut allocator),
                memory,
                gworlds,
                params.window_ptr(),
                params.clobbered_rgn_ptr(),
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SelectWindow => {
            if let Some(params) = crate::window_manager::evaluate_select_window_parameters(cpu.gpr[3]) {
                let window = params.window_ptr();
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                ppc_reorder_window(gworlds, window_list, window, 0, true);
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                if previous_front != Some(window) {
                    ppc_transition_front_window_chrome(
                        memory,
                        gworlds,
                        window_list,
                        previous_front,
                        toolbox_startup.host_menu_bar_hidden,
                    );
                }
                *current_gworld = window;
                *current_gdevice =
                    ppc_gworld_device(gworlds, *current_gworld).unwrap_or(*current_gdevice);
                ppc_register_gdevice(toolbox_startup, *current_gdevice);
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
                let _ = ppc_set_window_hilited(memory, *current_gworld, true);
                if ppc_front_visible_process_window(memory, window_list) != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
            }
            if ppc_gworld_trace_enabled() {
                eprintln!(
                    "[PPC-GWORLD-TRACE] SelectWindow window=${:08X} current=${:08X} gdevice=${:08X}",
                    cpu.gpr[3], *current_gworld, *current_gdevice
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::GetWMgrPort => {
            if let Some(params) =
                crate::window_manager::evaluate_get_wmgr_port_parameters(cpu.gpr[3])
            {
                let port_ptr = params.port_ptr();
                if ppc_memory_can_write_bytes(memory, port_ptr, 4) {
                    let _ = memory.write_u32_be(port_ptr, PPC_MAIN_GWORLD);
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InvalRect => {
            if let Some(params) =
                crate::window_manager::evaluate_inval_rect_parameters(cpu.gpr[3])
            {
                let window = *current_gworld;
                if window != PPC_MAIN_GWORLD {
                    if let Some(rect) = ppc_read_rect(memory, params.rect_ptr()) {
                        ppc_invalidate_window_local_rect(memory, window, rect);
                        ppc_enqueue_window_update_event(event_queue, window, tick_count, input);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::InvalRgn => {
            if let Some(params) =
                crate::window_manager::evaluate_inval_rgn_parameters(cpu.gpr[3])
            {
                let window = *current_gworld;
                if window != PPC_MAIN_GWORLD {
                    if let Some(rect) = ppc_read_rgn_bbox(memory, params.rgn_ptr()) {
                        ppc_invalidate_window_local_rect(memory, window, rect);
                        ppc_enqueue_window_update_event(event_queue, window, tick_count, input);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ValidRect => {
            if let Some(params) =
                crate::window_manager::evaluate_valid_rect_parameters(cpu.gpr[3])
            {
                let window = *current_gworld;
                if window != PPC_MAIN_GWORLD {
                    if let Some(rect) = ppc_read_rect(memory, params.rect_ptr()) {
                        ppc_validate_window_local_rect(memory, window, rect);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::ValidRgn => {
            if let Some(params) =
                crate::window_manager::evaluate_valid_rgn_parameters(cpu.gpr[3])
            {
                let window = *current_gworld;
                if window != PPC_MAIN_GWORLD {
                    if let Some(rect) = ppc_read_rgn_bbox(memory, params.rgn_ptr()) {
                        ppc_validate_window_local_rect(memory, window, rect);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::BeginUpdate => {
            if let Some(params) =
                crate::window_manager::evaluate_begin_update_parameters(cpu.gpr[3])
            {
                let window = params.window_ptr();
                if gworlds.iter().any(|record| record.port == window) {
                    *current_gworld = window;
                    *current_gdevice =
                        ppc_gworld_device(gworlds, window).unwrap_or(*current_gdevice);
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::EndUpdate => {
            if let Some(params) =
                crate::window_manager::evaluate_end_update_parameters(cpu.gpr[3])
            {
                let window = params.window_ptr();
                let update_rgn = memory
                    .read_u32_be(window.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
                    .unwrap_or(0);
                let _ = ppc_set_empty_rgn(memory, update_rgn);
                event_queue.retain(|event| !(event.what == 6 && event.message == window));
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DrawGrowIcon => {
            if let Some(params) =
                crate::window_manager::evaluate_draw_grow_icon_parameters(cpu.gpr[3])
            {
                ppc_draw_grow_icon(memory, gworlds, window_list, params.window_ptr());
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::FindWindow => {
            let params = crate::window_manager::evaluate_find_window_parameters(cpu.gpr[3], cpu.gpr[4]);
            let v = params.v();
            let h = params.h();
            let window_out = params.window_out();
            let front = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD);
            let screen_width = front
                .map(|front| ppc_u32_to_i16_saturating(front.width))
                .unwrap_or(ppc_main_screen_width() as i16);
            let screen_height = front
                .map(|front| ppc_u32_to_i16_saturating(front.height))
                .unwrap_or(ppc_main_screen_height() as i16);
            let menu_bar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
            let fullscreen_context_active = draw_sprocket.active_context.is_some();
            let (in_screen, in_menu_bar) = crate::window_manager::evaluate_find_window_in_screen_and_menu_bar(
                v,
                h,
                screen_width,
                screen_height,
                menu_bar_height,
                fullscreen_context_active,
            );
            let (part, window) = if in_menu_bar {
                (1, 0)
            } else if in_screen {
                ppc_find_window_at_point(memory, gworlds, window_list, v, h, menu_bar_height)
            } else {
                (0, 0)
            };
            // Apple Game Sprockets Guide, DrawSprocket (1996), p. 2-14:
            // an active context's blanking window covers the entire display.
            let (part, window) = if in_screen && draw_sprocket.blanking_window.is_some() {
                (3, draw_sprocket.blanking_window.unwrap())
            } else if fullscreen_context_active && in_screen && part == 0 {
                (3, 0)
            } else {
                (part, window)
            };
            if window_out != 0 && ppc_memory_can_write_bytes(memory, window_out, 4) {
                let _ = memory.write_u32_be(window_out, window);
            }
            Some(PpcImportAction::Return(part as u32))
        }
        PpcImportDispatcherTarget::PinRect => {
            let params = crate::window_manager::evaluate_pin_rect_parameters(cpu.gpr[3], cpu.gpr[4]);
            let rect = if params.rect_ptr() != 0 {
                ppc_read_rect(memory, params.rect_ptr())
            } else {
                None
            };
            let (pinned_v, pinned_h) =
                crate::window_manager::evaluate_pin_rect(rect, params.pt_v(), params.pt_h());
            let result = ((pinned_v as u16 as u32) << 16) | (pinned_h as u16 as u32);
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::GetWVariant => {
            let window = cpu.gpr[3];
            let proc_id = if window != 0 {
                Some(ppc_window_proc_id(memory, window))
            } else {
                None
            };
            let variant = crate::window_manager::evaluate_get_w_variant(window, proc_id);
            Some(PpcImportAction::Return(variant as u16 as u32))
        }
        PpcImportDispatcherTarget::ClipAbove => {
            if let Some(params) =
                crate::window_manager::evaluate_clip_above_parameters(cpu.gpr[3])
            {
                let start_window = params.window_ptr();
                let target_port = if *current_gworld != 0 {
                    *current_gworld
                } else {
                    PPC_MAIN_GWORLD
                };
                let clip_rgn = memory
                    .read_u32_be(target_port.wrapping_add(PPC_CGRAF_PORT_CLIP_RGN_OFFSET))
                    .unwrap_or(0);
                if clip_rgn != 0 {
                    if let Some(mut clip_rect) = ppc_read_rgn_bbox(memory, clip_rgn) {
                        window_list.with_ref(|windows| {
                            if let Some(start_idx) = windows.iter().position(|&w| w == start_window)
                            {
                                for &front in windows.iter().take(start_idx) {
                                    if !ppc_window_is_visible(memory, front) {
                                        continue;
                                    }
                                    if let Some(front_rect) =
                                        ppc_window_global_structure_bounds(memory, gworlds, front)
                                    {
                                        clip_rect = ppc_rect_difference_bbox(clip_rect, front_rect)
                                            .unwrap_or((0, 0, 0, 0));
                                        if clip_rect == (0, 0, 0, 0) {
                                            break;
                                        }
                                    }
                                }
                            }
                        });
                        let _ = ppc_write_rgn_bbox(
                            memory,
                            clip_rgn,
                            clip_rect.0,
                            clip_rect.1,
                            clip_rect.2,
                            clip_rect.3,
                        );
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::SaveOld => Some(PpcImportAction::ReturnPreserve),
        PpcImportDispatcherTarget::DrawNew => {
            if let Some(params) =
                crate::window_manager::evaluate_draw_new_parameters(cpu.gpr[3], cpu.gpr[4] != 0)
            {
                if params.f_update() {
                    let window = params.window_ptr();
                    ppc_redraw_visible_window_frame(
                        memory,
                        gworlds,
                        window_list,
                        window,
                        toolbox_startup.host_menu_bar_hidden,
                    );
                    if let Some(content) = ppc_window_global_content_bounds(memory, gworlds, window) {
                        ppc_union_window_update_rect(memory, window, content);
                        ppc_enqueue_window_update_event(event_queue, window, tick_count, input);
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DragGrayRgn => {
            let params = crate::window_manager::evaluate_drag_gray_rgn_parameters(
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[6],
                cpu.gpr[7] as u16 as i16,
                cpu.gpr[8],
            );
            let mouse_v = input.mouse_v;
            let mouse_h = input.mouse_h;

            let in_slop = if params.slop_rect_ptr() != 0 {
                ppc_read_rect(memory, params.slop_rect_ptr())
                    .map(|rect| ppc_point_in_rect((mouse_v, mouse_h), rect))
                    .unwrap_or(true)
            } else {
                true
            };

            let result = crate::window_manager::evaluate_drag_gray_rgn_delta(
                in_slop,
                params.start_v(),
                params.start_h(),
                mouse_v,
                mouse_h,
            );
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::LegacyWindow(operation) => ppc_dispatch_legacy_window(
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
            window_list,
            current_gworld,
            current_gdevice,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_back_color,
            screen_clut,
            color_manager_clut,
            toolbox_startup,
            input,
            tick_count,
            event_queue,
            vfs_resources,
            current_resource_refnum,
            last_resource_error,
        ),
        _ => None,
    }
}

pub(super) fn ppc_window_proc_has_title_bar(proc_id: i16) -> bool {
    matches!(proc_id, 0 | 4 | 5 | 8 | 12 | 16)
}

pub(super) fn ppc_window_structure_bounds(
    proc_id: i16,
    content: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    let has_title_bar = ppc_window_proc_has_title_bar(proc_id);
    let border: i16 = if proc_id == 2 { 1 } else { 6 };
    if has_title_bar {
        crate::window_manager::standard_window_structure_bounds(content)
    } else {
        (
            content.0.saturating_sub(border),
            content.1.saturating_sub(border),
            content.2.saturating_add(border),
            content.3.saturating_add(border),
        )
    }
}

pub(super) fn ppc_window_proc_id(memory: &mut PpcSectionMem, window: u32) -> i16 {
    memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_DEF_PROC_OFFSET))
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|data| *data != 0)
        .and_then(|data| memory.read_u16_be(data))
        .unwrap_or(0) as i16
}

pub(super) fn ppc_update_window_manager_regions(
    memory: &mut PpcSectionMem,
    window: u32,
    content: (i16, i16, i16, i16),
) -> Option<()> {
    let content_rgn = memory
        .read_u32_be(window.checked_add(PPC_CWINDOW_CONTENT_RGN_OFFSET)?)
        .filter(|handle| ppc_rgn_ptr(memory, *handle).is_some());
    let structure_rgn = memory
        .read_u32_be(window.checked_add(PPC_CWINDOW_STRUCTURE_RGN_OFFSET)?)
        .filter(|handle| ppc_rgn_ptr(memory, *handle).is_some());
    let (Some(content_rgn), Some(structure_rgn)) = (content_rgn, structure_rgn) else {
        return Some(());
    };
    let structure = ppc_window_structure_bounds(ppc_window_proc_id(memory, window), content);
    ppc_write_rgn_bbox(
        memory,
        content_rgn,
        content.0,
        content.1,
        content.2,
        content.3,
    )?;
    ppc_write_rgn_bbox(
        memory,
        structure_rgn,
        structure.0,
        structure.1,
        structure.2,
        structure.3,
    )?;
    Some(())
}

pub(super) struct PpcNewCWindowParameters {
    pub storage_ptr: u32,
    pub bounds: (i16, i16, i16, i16),
    pub visible: bool,
    pub proc_id: i16,
    pub behind: u32,
    pub go_away: bool,
    pub ref_con: u32,
}

pub(super) fn ppc_new_cwindow(
    cpu: &PpcCpu,
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    current_gdevice: u32,
) -> u32 {
    let Some(params) = crate::window_manager::evaluate_new_window_parameters(
        cpu.gpr[3],
        cpu.gpr[4],
        cpu.gpr[5],
        cpu.gpr[6] != 0,
        cpu.gpr[7] as u16 as i16,
        cpu.gpr[8],
        cpu.gpr[9] != 0,
        cpu.gpr[10],
    ) else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };

    let Some((top, left, bottom, right)) = ppc_read_rect(memory, params.bounds_ptr()) else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    ppc_new_cwindow_with_parameters(
        PpcNewCWindowParameters {
            storage_ptr: params.storage_ptr(),
            bounds: (top, left, bottom, right),
            visible: params.visible(),
            proc_id: params.proc_id(),
            behind: params.behind(),
            go_away: params.go_away(),
            ref_con: params.ref_con(),
        },
        allocator,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        window_list,
        current_gdevice,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_new_cwindow_with_parameters(
    params: PpcNewCWindowParameters,
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    current_gdevice: u32,
) -> u32 {
    let PpcNewCWindowParameters {
        storage_ptr,
        bounds: (top, left, bottom, right),
        visible,
        proc_id,
        behind,
        go_away,
        ref_con,
    } = params;
    let (width, height) = ppc_rect_dimensions(top, left, bottom, right);
    // Inside Macintosh: Imaging With QuickDraw 1994, "Pixel Images": the
    // pixel map for a window's color graphics port always uses the pixel
    // depth, color table, and boundary rectangle of the main screen. Unlike
    // an offscreen GWorld, an onscreen window therefore points at screen RAM.
    let screen_pixmap_handle = memory
        .read_u32_be(current_gdevice)
        .and_then(|gdevice| memory.read_u32_be(gdevice.checked_add(22)?));
    let screen_color_table = screen_pixmap_handle
        .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle))
        .and_then(|pixmap| memory.read_u32_be(pixmap.checked_add(42)?))
        .filter(|handle| *handle != 0)
        .unwrap_or(PPC_MAIN_CTABLE_HANDLE);
    let screen_bits = screen_pixmap_handle
        .and_then(|pixmap_handle| ppc_read_pixmap_handle_bits(memory, pixmap_handle));
    let (base_addr, row_bytes, screen_top, screen_left, screen_bottom, screen_right, depth) =
        screen_bits
            .filter(|bits| matches!(bits.depth, 1 | 2 | 4 | 8 | 16 | 32))
            .map(|bits| {
                (
                    bits.base_addr,
                    bits.row_bytes,
                    bits.top,
                    bits.left,
                    bits.bottom,
                    bits.right,
                    bits.depth,
                )
            })
            .unwrap_or((
                PPC_MAIN_SCREEN_BASE,
                ppc_main_screen_row_bytes(),
                0,
                0,
                ppc_main_screen_height() as i16,
                ppc_main_screen_width() as i16,
                PPC_MAIN_PIXEL_DEPTH,
            ));
    if row_bytes == 0 {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    if storage_ptr != 0 && !ppc_memory_can_write_bytes(memory, storage_ptr, PPC_CGRAF_PORT_SIZE) {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }

    let has_heap = if storage_ptr != 0 {
        ppc_heap_can_alloc_sequence(
            memory,
            *heap_cursor,
            heap_limit,
            &[PPC_PIXMAP_SIZE, 4, 4, 10, 4, 10, 4, 10, 4, 10, 4, 10, 4, 4],
        )
    } else {
        ppc_heap_can_alloc_sequence(
            memory,
            *heap_cursor,
            heap_limit,
            &[
                PPC_PIXMAP_SIZE,
                4,
                PPC_CGRAF_PORT_SIZE,
                4,
                10,
                4,
                10,
                4,
                10,
                4,
                10,
                4,
                10,
                4,
                4,
            ],
        )
    };
    if !has_heap {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }

    let pixmap = ppc_allocator_view_reserve_bytes(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        PPC_PIXMAP_SIZE,
        true,
    );
    let pixmap_handle = ppc_allocator_view_reserve_bytes(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        4,
        true,
    );
    let port = if storage_ptr != 0 {
        storage_ptr
    } else {
        ppc_allocator_view_reserve_bytes(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            PPC_CGRAF_PORT_SIZE,
            true,
        )
    };
    let vis_rgn = ppc_allocator_view_new_rgn(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    let clip_rgn = ppc_allocator_view_new_rgn(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    let structure_rgn = ppc_allocator_view_new_rgn(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    let content_rgn = ppc_allocator_view_new_rgn(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    let update_rgn = ppc_allocator_view_new_rgn(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
    );
    let def_proc = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        4,
        true,
    );
    if pixmap == 0
        || pixmap_handle == 0
        || port == 0
        || vis_rgn == 0
        || clip_rgn == 0
        || structure_rgn == 0
        || content_rgn == 0
        || update_rgn == 0
        || def_proc == 0
    {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }

    // Inside Macintosh: Macintosh Toolbox Essentials (1992), Window Manager,
    // p. 4-18, and Inside Macintosh: Imaging With QuickDraw (1994), Basic
    // QuickDraw, pp. 2-9–2-10: a new window has a local origin of (0,0), while
    // the screen boundary rectangle is offset by the negative global origin.
    let local_bottom = ppc_u32_to_i16_saturating(height);
    let local_right = ppc_u32_to_i16_saturating(width);
    let pixel_top = ppc_i32_to_i16_saturating(i32::from(screen_top) - i32::from(top));
    let pixel_left = ppc_i32_to_i16_saturating(i32::from(screen_left) - i32::from(left));
    let pixel_bottom = ppc_i32_to_i16_saturating(i32::from(screen_bottom) - i32::from(top));
    let pixel_right = ppc_i32_to_i16_saturating(i32::from(screen_right) - i32::from(left));
    let structure = ppc_window_structure_bounds(proc_id, (top, left, bottom, right));
    let def_proc_data = memory.read_u32_be(def_proc).filter(|ptr| *ptr != 0);

    if memory
        .write_bytes(port, &[0; PPC_CGRAF_PORT_SIZE as usize])
        .is_none()
        || def_proc_data
            .and_then(|data| memory.write_u16_be(data, proc_id as u16))
            .is_none()
        || memory.write_u32_be(pixmap_handle, pixmap).is_none()
        || ppc_write_pixmap(
            memory,
            pixmap,
            base_addr,
            row_bytes,
            pixel_top,
            pixel_left,
            pixel_bottom,
            pixel_right,
            depth,
        )
        .is_none()
        || memory
            .write_u32_be(pixmap + 42, if depth <= 8 { screen_color_table } else { 0 })
            .is_none()
        || ppc_write_gworld_port(memory, port, pixmap_handle, 0, 0, local_bottom, local_right)
            .is_none()
        || memory
            .write_u32_be(port + PPC_CGRAF_PORT_VIS_RGN_OFFSET, vis_rgn)
            .is_none()
        || memory
            .write_u32_be(port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET, clip_rgn)
            .is_none()
        || memory
            .write_u32_be(port + PPC_CWINDOW_STRUCTURE_RGN_OFFSET, structure_rgn)
            .is_none()
        || memory
            .write_u32_be(port + PPC_CWINDOW_CONTENT_RGN_OFFSET, content_rgn)
            .is_none()
        || memory
            .write_u32_be(port + PPC_CWINDOW_UPDATE_RGN_OFFSET, update_rgn)
            .is_none()
        || memory
            .write_u32_be(port + PPC_CWINDOW_DEF_PROC_OFFSET, def_proc)
            .is_none()
        || ppc_write_rgn_bbox(memory, vis_rgn, 0, 0, local_bottom, local_right).is_none()
        || ppc_write_rgn_bbox(memory, clip_rgn, i16::MIN, i16::MIN, i16::MAX, i16::MAX).is_none()
        || ppc_write_rgn_bbox(memory, content_rgn, top, left, bottom, right).is_none()
        || ppc_write_rgn_bbox(
            memory,
            structure_rgn,
            structure.0,
            structure.1,
            structure.2,
            structure.3,
        )
        .is_none()
        || memory
            .write_u16_be(port + PPC_CWINDOW_WINDOW_KIND_OFFSET, 8)
            .is_none()
        || memory
            .write_u8(port + PPC_CWINDOW_VISIBLE_OFFSET, u8::from(visible))
            .is_none()
        || memory
            .write_u8(port + PPC_CWINDOW_HILITED_OFFSET, 0)
            .is_none()
        || memory
            .write_u8(port + PPC_CWINDOW_GO_AWAY_OFFSET, u8::from(go_away))
            .is_none()
        || memory
            .write_u32_be(port + PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET, ref_con)
            .is_none()
    {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }

    *last_mem_error = PPC_NO_ERR;
    gworlds.retain(|gworld| gworld.port != port);
    gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port,
        pixmap_handle,
        pixmap,
        base_addr,
        gdevice: current_gdevice,
        width,
        height,
        depth,
        row_bytes,
        pixels_locked: false,
        pixels_no_purge: true,
    });
    ppc_reorder_window(gworlds, window_list, port, behind, false);
    if visible && proc_id == 1 {
        ppc_draw_existing_window_frame(memory, gworlds, window_list, port, false);
    }
    if ppc_gworld_trace_enabled() {
        eprintln!(
            "[PPC-GWORLD-TRACE] NewCWindow port=${:08X} storage=${:08X} visible={} proc_id={} base=${:08X} pixmap=${:08X} handle=${:08X} gdevice=${:08X} global_bounds=({}, {}, {}, {}) port_rect=(0, 0, {}, {}) pixel_bounds=({}, {}, {}, {}) size={}x{}x{} row_bytes={} ref_con=${:08X}",
            port,
            storage_ptr,
            visible,
            proc_id,
            base_addr,
            pixmap,
            pixmap_handle,
            current_gdevice,
            top,
            left,
            bottom,
            right,
            local_bottom,
            local_right,
            pixel_top,
            pixel_left,
            pixel_bottom,
            pixel_right,
            width,
            height,
            depth,
            row_bytes,
            ref_con
        );
    }
    port
}

pub(super) fn ppc_draw_standard_window_frame(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
    _width: i16,
    _height: i16,
    go_away: bool,
) {
    // Macintosh Toolbox Essentials (1992), pp. 4-24--4-27: the standard
    // document WDEF owns an 18-pixel title bar and one-pixel structure frame.
    let active = memory
        .read_u8(window.wrapping_add(PPC_CWINDOW_HILITED_OFFSET))
        .unwrap_or(0)
        != 0;
    let proc_id = ppc_window_proc_id(memory, window);
    let Some(content) = memory
        .read_u32_be(window + PPC_CWINDOW_CONTENT_RGN_OFFSET)
        .and_then(|region| ppc_read_rgn_bbox(memory, region))
    else {
        return;
    };
    let title = memory
        .read_u32_be(window + PPC_CWINDOW_TITLE_HANDLE_OFFSET)
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|ptr| *ptr != 0)
        .and_then(|ptr| ppc_read_pstring_bytes(memory, ptr))
        .unwrap_or_default();
    let title_width =
        ppc_text_bytes_advance_for_font(&title, PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM);
    let title_metrics = get_font_metrics(PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM);
    let menu_bar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
    let chrome = crate::window_manager::standard_window_chrome(
        content,
        menu_bar_height,
        title_width,
        title_metrics.ascent,
        title_metrics.descent,
        !title.is_empty(),
        active,
        matches!(proc_id, 0 | 4 | 8 | 12),
        go_away,
        matches!(proc_id, 8 | 12),
    );
    let palette = ppc_ui_theme(gworlds).provider().palette();
    let _ = ppc_paint_rect_bounds(
        memory,
        gworlds,
        PPC_MAIN_GWORLD,
        chrome.background,
        ppc_theme_rgb(palette.frame_light),
        None,
    );
    for rect in chrome.ink.iter().copied() {
        let _ = ppc_paint_rect_bounds(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            rect,
            ppc_theme_rgb(palette.frame_dark),
            None,
        );
    }

    if active && ppc_ui_theme(gworlds) != UiThemeId::ClassicSystem7 {
        for rect in chrome.stripe_ink.iter().copied() {
            let _ = ppc_paint_rect_bounds(
                memory,
                gworlds,
                PPC_MAIN_GWORLD,
                rect,
                ppc_theme_rgb(palette.selection),
                None,
            );
        }
    }

    if !title.is_empty() {
        let _ = ppc_with_unclipped_screen_port(memory, |memory| {
            ppc_draw_text_bytes(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            (chrome.title_h, chrome.title_baseline),
            PPC_QD_TEXT_FONT_DEFAULT,
            PPC_QD_TEXT_SIZE_SYSTEM,
            PPC_QD_TEXT_MODE_SRC_OR,
            ppc_theme_rgb(palette.frame_dark),
            None,
            &title,
            )
        });
    }
}

pub(super) fn ppc_draw_grow_icon(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    window: u32,
) {
    if window == 0 || !matches!(ppc_window_proc_id(memory, window), 0 | 8) {
        return;
    }
    let Some(content) = memory
        .read_u32_be(window + PPC_CWINDOW_CONTENT_RGN_OFFSET)
        .and_then(|region| ppc_read_rgn_bbox(memory, region))
    else {
        return;
    };
    let active = memory
        .read_u8(window.wrapping_add(PPC_CWINDOW_HILITED_OFFSET))
        .unwrap_or(0)
        != 0;
    // DrawGrowIcon is clipped by windows above the target in the Window
    // Manager port. Preserve that occlusion around native screen-RAM draws.
    // Macintosh Toolbox Essentials (1992), pp. 4-106 and 4-111--4-112.
    let preserved_front_pixels =
        ppc_front_window_occlusion_pixels(memory, gworlds, window_list, window);
    let icon = crate::window_manager::standard_grow_icon(content, active);
    let _ = ppc_paint_rect_bounds(
        memory,
        gworlds,
        PPC_MAIN_GWORLD,
        icon.background,
        PPC_RGB_WHITE,
        None,
    );
    for rect in icon.ink {
        let _ = ppc_paint_rect_bounds(memory, gworlds, PPC_MAIN_GWORLD, rect, PPC_RGB_BLACK, None);
    }
    if let Some(saved) = preserved_front_pixels {
        for (index, (x, y, pixel)) in saved.pixels.iter().copied().enumerate() {
            let _ = ppc_quickdraw_write_raw_pixel(memory, saved.front_buffer, (x, y), pixel);
            ppc_restore_saved_detail(memory, saved.front_buffer, (x, y), &saved.pixels, index);
        }
    }
}

pub(super) fn ppc_draw_existing_window_frame(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    window: u32,
    host_menu_bar_hidden: bool,
) {
    let Some((top, left, bottom, right)) = ppc_read_rect(memory, window.wrapping_add(16)) else {
        return;
    };
    let height = bottom.saturating_sub(top);
    let width = right.saturating_sub(left);
    let proc_id = ppc_window_proc_id(memory, window);
    // Macintosh Toolbox Essentials (1992), pp. 4-10--4-12: the standard
    // WDEF owns document-window frame pixels. Kiosk presentation suppresses
    // those host-synthesized pixels without changing the guest's window
    // regions, ordering, or visibility. Dialog WDEFs remain visible.
    // ClipAbove excludes the complete structure region of every visible
    // window above the WDEF being drawn. Native WDEF chrome targets screen
    // RAM directly, so save those pixels and restore them after the raw draw.
    // Macintosh Toolbox Essentials (1992), pp. 4-106 and 4-118--4-119.
    let preserved_front_pixels =
        ppc_front_window_occlusion_pixels(memory, gworlds, window_list, window);
    if ppc_window_proc_has_title_bar(proc_id) && !host_menu_bar_hidden {
        let go_away = memory
            .read_u8(window.wrapping_add(PPC_CWINDOW_GO_AWAY_OFFSET))
            .unwrap_or(0)
            != 0;
        ppc_draw_standard_window_frame(memory, gworlds, window, width, height, go_away);
    } else if proc_id == 1 {
        ppc_draw_dialog_box_frame(memory, gworlds, window, height, width);
    }
    if let Some(saved) = preserved_front_pixels {
        for (index, (x, y, pixel)) in saved.pixels.iter().copied().enumerate() {
            let _ = ppc_quickdraw_write_raw_pixel(memory, saved.front_buffer, (x, y), pixel);
            ppc_restore_saved_detail(memory, saved.front_buffer, (x, y), &saved.pixels, index);
        }
    }
}

pub(crate) struct PpcOccludedWindowPixels {
    pub(crate) front_buffer: PpcFrontBuffer,
    pub(crate) pixels: crate::memory::SavedPixels<(i32, i32, u16)>,
}

pub(super) fn ppc_front_window_occlusion_pixels(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    window: u32,
) -> Option<PpcOccludedWindowPixels> {
    let front_buffer = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD)?;
    let target_structure = ppc_window_global_structure_bounds(memory, gworlds, window)?;
    let mut pixels = Vec::new();
    let occluders = window_list.with_ref(|windows| {
        crate::window_manager::window_occluders(windows.iter().copied(), window, |candidate| {
            ppc_window_is_visible(memory, candidate)
        })
    });
    for front in occluders {
        let Some(front_structure) = ppc_window_global_structure_bounds(memory, gworlds, front)
        else {
            continue;
        };
        let overlap = (
            target_structure.0.max(front_structure.0).max(0),
            target_structure.1.max(front_structure.1).max(0),
            target_structure
                .2
                .min(front_structure.2)
                .min(ppc_u32_to_i16_saturating(front_buffer.height)),
            target_structure
                .3
                .min(front_structure.3)
                .min(ppc_u32_to_i16_saturating(front_buffer.width)),
        );
        if overlap.0 >= overlap.2 || overlap.1 >= overlap.3 {
            continue;
        }
        for y in i32::from(overlap.0)..i32::from(overlap.2) {
            for x in i32::from(overlap.1)..i32::from(overlap.3) {
                if let Some(pixel) = ppc_quickdraw_read_pixel(memory, front_buffer, (x, y)) {
                    pixels.push((x, y, pixel));
                }
            }
        }
    }
    let mut saved = crate::memory::SavedPixels::from(pixels);
    for index in 0..saved.len() {
        let (x, y, _) = saved[index];
        ppc_capture_saved_detail(memory, front_buffer, (x, y), &mut saved, index);
    }
    Some(PpcOccludedWindowPixels {
        front_buffer,
        pixels: saved,
    })
}

pub(super) fn ppc_redraw_visible_window_frame(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    window: u32,
    host_menu_bar_hidden: bool,
) {
    if ppc_window_is_visible(memory, window) {
        ppc_draw_existing_window_frame(memory, gworlds, window_list, window, host_menu_bar_hidden);
    }
}

pub(super) fn ppc_window_global_structure_bounds(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
) -> Option<(i16, i16, i16, i16)> {
    let content = ppc_window_global_content_bounds(memory, gworlds, window)?;
    Some(ppc_window_structure_bounds(
        ppc_window_proc_id(memory, window),
        content,
    ))
}

pub(super) fn ppc_union_bounds(
    first: Option<(i16, i16, i16, i16)>,
    second: Option<(i16, i16, i16, i16)>,
) -> Option<(i16, i16, i16, i16)> {
    let valid = |rect: (i16, i16, i16, i16)| rect.0 < rect.2 && rect.1 < rect.3;
    match (
        first.filter(|rect| valid(*rect)),
        second.filter(|rect| valid(*rect)),
    ) {
        (Some(first), Some(second)) => Some((
            first.0.min(second.0),
            first.1.min(second.1),
            first.2.max(second.2),
            first.3.max(second.3),
        )),
        (Some(rect), None) | (None, Some(rect)) => Some(rect),
        (None, None) => None,
    }
}

pub(super) fn ppc_invalidate_window_global_rect(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
    global_rect: (i16, i16, i16, i16),
) {
    let Some(content) = ppc_window_global_content_bounds(memory, gworlds, window) else {
        return;
    };
    let intersection = (
        content.0.max(global_rect.0),
        content.1.max(global_rect.1),
        content.2.min(global_rect.2),
        content.3.min(global_rect.3),
    );
    if intersection.0 >= intersection.2 || intersection.1 >= intersection.3 {
        return;
    }
    let update_rgn = memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
        .unwrap_or(0);
    if ppc_rgn_ptr(memory, update_rgn).is_none() {
        return;
    }
    let local = (
        intersection.0.saturating_sub(content.0),
        intersection.1.saturating_sub(content.1),
        intersection.2.saturating_sub(content.0),
        intersection.3.saturating_sub(content.1),
    );
    ppc_union_window_update_rect(memory, window, local);
}

pub(super) fn ppc_union_window_update_rect(
    memory: &mut PpcSectionMem,
    window: u32,
    rect: (i16, i16, i16, i16),
) {
    if rect.0 >= rect.2 || rect.1 >= rect.3 {
        return;
    }
    let update_rgn = memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
        .unwrap_or(0);
    if ppc_rgn_ptr(memory, update_rgn).is_none() {
        return;
    }
    let existing = ppc_read_rgn_bbox(memory, update_rgn).unwrap_or((0, 0, 0, 0));
    let combined = if existing.0 >= existing.2 || existing.1 >= existing.3 {
        rect
    } else {
        (
            existing.0.min(rect.0),
            existing.1.min(rect.1),
            existing.2.max(rect.2),
            existing.3.max(rect.3),
        )
    };
    let _ = ppc_write_rgn_bbox(
        memory, update_rgn, combined.0, combined.1, combined.2, combined.3,
    );
}

pub(super) fn ppc_invalidate_window_local_rect(
    memory: &mut PpcSectionMem,
    window: u32,
    rect: (i16, i16, i16, i16),
) {
    let Some(content) = ppc_read_rect(memory, window.wrapping_add(16)) else {
        return;
    };
    let clipped = (
        rect.0.max(content.0),
        rect.1.max(content.1),
        rect.2.min(content.2),
        rect.3.min(content.3),
    );
    ppc_union_window_update_rect(memory, window, clipped);
}

pub(super) fn ppc_validate_window_local_rect(
    memory: &mut PpcSectionMem,
    window: u32,
    rect: (i16, i16, i16, i16),
) {
    let update_rgn = memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
        .unwrap_or(0);
    let Some(update) = ppc_read_rgn_bbox(memory, update_rgn)
        .filter(|current| current.0 < current.2 && current.1 < current.3)
    else {
        return;
    };
    if rect.0 <= update.0 && rect.1 <= update.1 && rect.2 >= update.2 && rect.3 >= update.3 {
        let _ = ppc_set_empty_rgn(memory, update_rgn);
    }
}

pub(super) fn ppc_standard_desktop_color(
    gworlds: &[PpcGWorldRecord],
    h: i32,
    v: i32,
) -> PpcRgbColor {
    if gworlds
        .iter()
        .any(|world| world.port == PPC_MAIN_GWORLD && world.depth == 1)
    {
        return if crate::window_manager::standard_desktop_pattern_is_ink(h, v) {
            PPC_RGB_BLACK
        } else {
            PPC_RGB_WHITE
        };
    }
    let palette = ppc_ui_theme(gworlds).provider().palette();
    ppc_theme_rgb(
        if crate::window_manager::standard_desktop_pattern_is_ink(h, v) {
            palette.desktop_dark
        } else {
            palette.desktop_light
        },
    )
}

fn ppc_standard_desktop_pixels(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    front_buffer: PpcFrontBuffer,
) -> Option<(u16, u16)> {
    let ink = ppc_standard_desktop_color(gworlds, 0, 0);
    let paper = ppc_standard_desktop_color(gworlds, 1, 0);
    // Color2Index depends on the destination ColorTable (Inside Macintosh:
    // Imaging With QuickDraw, 1994, pp. 4-81--4-82). Resolve the two desktop
    // pattern colors once per repaint instead of re-reading the table for
    // every exposed pixel.
    let surface = PpcQuickDrawSurface {
        front_buffer,
        top: 0,
        left: 0,
        ctable_handle: (front_buffer.base_addr == PPC_MAIN_SCREEN_BASE)
            .then_some(PPC_MAIN_CTABLE_HANDLE),
    };
    Some((
        ppc_quickdraw_surface_color_pixel(memory, surface, ink)?,
        ppc_quickdraw_surface_color_pixel(memory, surface, paper)?,
    ))
}

pub(super) fn ppc_repaint_window_geometry_transition(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    window: u32,
    was_visible: bool,
    previous_structure: Option<(i16, i16, i16, i16)>,
    next_structure: Option<(i16, i16, i16, i16)>,
    host_menu_bar_hidden: bool,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    when: u32,
    input: PpcInputSnapshot,
) {
    if !was_visible {
        return;
    }
    let Some(exposed) = ppc_union_bounds(previous_structure, next_structure) else {
        return;
    };
    ppc_restore_window_removal_exposure(
        memory,
        gworlds,
        window_list,
        Some(exposed),
        host_menu_bar_hidden,
        event_queue,
        when,
        input,
    );
    // Keep the moved/resized window's update region explicit even if its new
    // structure only touches the transition's edge. EndUpdate clears it
    // after the guest's updateEvt redraws the visible content.
    ppc_invalidate_window_global_rect(memory, gworlds, window, exposed);
}

pub(super) fn ppc_restore_window_removal_exposure(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    exposed: Option<(i16, i16, i16, i16)>,
    host_menu_bar_hidden: bool,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    when: u32,
    input: PpcInputSnapshot,
) {
    let Some(exposed) = exposed.filter(|rect| rect.0 < rect.2 && rect.1 < rect.3) else {
        return;
    };
    let Some(front_buffer) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) else {
        return;
    };

    // NewCWindow ports share the screen PixMap. Removing one therefore has
    // to restore the desktop pixels outside any remaining window before the
    // exposed windows receive their update events. This is the PowerPC
    // equivalent of the classic adapter's saved-under-pixels path.
    let menu_bar_height = if host_menu_bar_hidden {
        0
    } else {
        memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16
    };
    let paint = (
        exposed.0.max(menu_bar_height).max(0),
        exposed.1.max(0),
        exposed.2.min(ppc_main_screen_height() as i16),
        exposed.3.min(ppc_main_screen_width() as i16),
    );
    let covered = window_list
        .windows()
        .into_iter()
        .filter_map(|window| {
            ppc_window_is_visible(memory, window)
                .then(|| ppc_window_global_content_bounds(memory, gworlds, window))
                .flatten()
        })
        .collect::<Vec<_>>();
    if paint.0 < paint.2 && paint.1 < paint.3 {
        if let Some((ink, paper)) = ppc_standard_desktop_pixels(memory, gworlds, front_buffer) {
            for v in i32::from(paint.0)..i32::from(paint.2) {
                for h in i32::from(paint.1)..i32::from(paint.3) {
                    if covered.iter().any(|&(top, left, bottom, right)| {
                        i32::from(top) <= v
                            && v < i32::from(bottom)
                            && i32::from(left) <= h
                            && h < i32::from(right)
                    }) {
                        continue;
                    }
                    let pixel = if crate::window_manager::standard_desktop_pattern_is_ink(h, v) {
                        ink
                    } else {
                        paper
                    };
                    let _ = ppc_quickdraw_write_raw_pixel(memory, front_buffer, (h, v), pixel);
                }
            }
        }
    }

    // Repaint every remaining visible window whose structure intersects the
    // exposed area. The update events let guest code redraw only its visible
    // content, while redrawing the WDEF here restores title bars and frames.
    for window in window_list.windows() {
        if !ppc_window_is_visible(memory, window) {
            continue;
        }
        let Some(content) = ppc_window_global_content_bounds(memory, gworlds, window) else {
            continue;
        };
        let structure = ppc_window_structure_bounds(ppc_window_proc_id(memory, window), content);
        let intersects = structure.0 < exposed.2
            && exposed.0 < structure.2
            && structure.1 < exposed.3
            && exposed.1 < structure.3;
        if !intersects {
            continue;
        }
        ppc_redraw_visible_window_frame(memory, gworlds, window_list, window, host_menu_bar_hidden);
        ppc_invalidate_window_global_rect(memory, gworlds, window, exposed);
        ppc_enqueue_window_update_event(event_queue, window, when, input);
    }
}

pub(super) fn ppc_sync_process_window_list(
    memory: &mut PpcSectionMem,
    window_list: &SharedProcessWindowList,
) {
    const WINDOW_NEXT_WINDOW_OFFSET: u32 = PPC_CWINDOW_NEXT_WINDOW_OFFSET;
    const LOWMEM_WINDOW_LIST: u32 = 0x09D6;

    window_list.with_ref(|windows| {
        for (index, &window) in windows.iter().enumerate() {
            let next = windows.get(index + 1).copied().unwrap_or(0);
            if memory.read_u32_be(window.wrapping_add(WINDOW_NEXT_WINDOW_OFFSET)) != Some(next) {
                let _ = memory.write_u32_be(window.wrapping_add(WINDOW_NEXT_WINDOW_OFFSET), next);
            }
        }
        let head = windows.first().copied().unwrap_or(0);
        if memory.read_u32_be(LOWMEM_WINDOW_LIST) != Some(head) {
            let _ = memory.write_u32_be(LOWMEM_WINDOW_LIST, head);
        }
    });
}

pub(super) fn ppc_transition_front_window_chrome(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    previous_front: Option<u32>,
    host_menu_bar_hidden: bool,
) {
    let next_front = ppc_front_visible_process_window(memory, window_list);
    if previous_front == next_front {
        return;
    }
    let preserve_previous_chrome =
        next_front.is_some_and(|window| ppc_window_proc_id(memory, window) == 1);
    if let Some(previous) = previous_front {
        if !preserve_previous_chrome {
            let _ = ppc_set_window_hilited(memory, previous, false);
            ppc_redraw_visible_window_frame(
                memory,
                gworlds,
                window_list,
                previous,
                host_menu_bar_hidden,
            );
        }
    }
    if let Some(next) = next_front {
        let _ = ppc_set_window_hilited(memory, next, true);
        ppc_redraw_visible_window_frame(memory, gworlds, window_list, next, host_menu_bar_hidden);
    }
}

pub(super) fn ppc_recalculate_window_vis_regions(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    window_list: &SharedProcessWindowList,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let mut allocator = PpcProcessAllocatorView {
        memory_manager: process_memory_manager,
    };
    let front_to_back = window_list.windows();
    for window in front_to_back.iter().copied() {
        let Some(vis_rgn) = memory.read_u32_be(window + PPC_CGRAF_PORT_VIS_RGN_OFFSET) else {
            continue;
        };
        if !ppc_window_is_visible(memory, window) {
            let _ = ppc_set_empty_rgn(memory, vis_rgn);
            continue;
        }
        let Some(content_rgn) = memory.read_u32_be(window + PPC_CWINDOW_CONTENT_RGN_OFFSET) else {
            continue;
        };
        if ppc_copy_rgn(
            Some(&mut allocator),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            content_rgn,
            vis_rgn,
        ) != PPC_NO_ERR
        {
            continue;
        }
        let occluders = crate::window_manager::window_occluders(
            front_to_back.iter().copied(),
            window,
            |front| ppc_window_is_visible(memory, front),
        );
        for front in occluders {
            let Some(structure_rgn) = memory.read_u32_be(front + PPC_CWINDOW_STRUCTURE_RGN_OFFSET)
            else {
                continue;
            };
            let _ = ppc_region_boolean_op(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vis_rgn,
                structure_rgn,
                vis_rgn,
                PpcRegionBooleanOp::Difference,
            );
        }
        // visRgn is in port coordinates. The port PixMap's screen bounds
        // include any SetOrigin shift, while contRgn remains global.
        let pixmap = memory
            .read_u32_be(window + 2)
            .and_then(|handle| memory.read_u32_be(handle));
        if let Some((top, left, _, _)) = pixmap.and_then(|ptr| ppc_read_rect(memory, ptr + 6)) {
            let _ = ppc_offset_rgn(memory, vis_rgn, left, top);
        }
    }
}

pub(super) fn ppc_draw_dialog_box_frame(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
    height: i16,
    width: i16,
) {
    let palette = ppc_ui_theme(gworlds).provider().palette();
    // Macintosh Toolbox Essentials (1992), pp. 4-24--4-26: dBoxProc owns a
    // gray dialog surface and a structure region outside the content rectangle.
    // Initialize both before the application draws its dialog items.
    let _ = ppc_paint_rect_bounds(
        memory,
        gworlds,
        window,
        (0, 0, height, width),
        ppc_theme_rgb(palette.window_background),
        None,
    );
    let Some((top, left, bottom, right)) = memory
        .read_u32_be(window + PPC_CWINDOW_CONTENT_RGN_OFFSET)
        .and_then(|region| ppc_read_rgn_bbox(memory, region))
    else {
        return;
    };
    let outer_top = top.saturating_sub(8);
    let outer_left = left.saturating_sub(8);
    let outer_bottom = bottom.saturating_add(8);
    let outer_right = right.saturating_add(8);
    if ppc_draw_themed_dialog_frame(
        memory,
        gworlds,
        (top, left, bottom, right),
        (outer_top, outer_left, outer_bottom, outer_right),
        1,
    ) {
        return;
    }
    let _ = ppc_paint_rect_bounds(
        memory,
        gworlds,
        PPC_MAIN_GWORLD,
        (outer_top, outer_left, outer_bottom, outer_right),
        ppc_theme_rgb(palette.window_background),
        None,
    );
    for rect in [
        (top - 8, left - 8, top - 7, right + 8),
        (top - 8, left - 8, bottom + 8, left - 7),
        (top - 8, right + 6, bottom + 8, right + 8),
        (bottom + 6, left - 8, bottom + 8, right + 8),
        (top - 5, left - 5, top - 4, right + 5),
        (top - 4, left - 5, top - 3, right + 4),
        (top - 5, left - 5, bottom + 5, left - 4),
        (top - 5, left - 4, bottom + 4, left - 3),
        (top - 5, right + 3, bottom + 4, right + 4),
        (bottom + 3, left - 5, bottom + 4, right + 4),
    ] {
        let _ = ppc_paint_rect_bounds(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            rect,
            ppc_theme_rgb(palette.frame_dark),
            None,
        );
    }
}

pub(super) fn ppc_get_new_cwindow(
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
    vfs_resources: &[PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
    host_menu_bar_hidden: bool,
) -> u32 {
    let params = crate::window_manager::evaluate_get_new_window_parameters(
        cpu.gpr[3] as u16 as i16,
        cpu.gpr[4],
        cpu.gpr[5],
    );
    let window_id = params.window_id();
    let storage_ptr = params.storage_ptr();
    let behind = params.behind();
    let Some(resource) = ppc_vfs_resource_index(
        vfs_resources,
        current_resource_refnum,
        u32::from_be_bytes(*b"WIND"),
        window_id,
        false,
    )
    .and_then(|index| vfs_resources.get(index)) else {
        *last_resource_error = PPC_RES_NOT_FOUND_ERR;
        return 0;
    };
    let bytes = resource.data.clone();
    let mut allocator = PpcProcessAllocatorView {
        memory_manager: process_memory_manager,
    };
    let Some((bounds_ptr, title_ptr)) = ppc_materialize_window_resource_parameters(
        Some(&mut allocator),
        memory,
        heap_cursor,
        heap_limit,
        &bytes,
    ) else {
        *last_resource_error = PPC_PARAM_ERR;
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    let mut window_cpu = cpu.clone();
    window_cpu.gpr[3] = storage_ptr;
    window_cpu.gpr[4] = bounds_ptr;
    window_cpu.gpr[5] = title_ptr;
    window_cpu.gpr[6] = u32::from(u16::from_be_bytes([bytes[10], bytes[11]]) != 0);
    window_cpu.gpr[7] = u32::from(u16::from_be_bytes([bytes[8], bytes[9]]));
    window_cpu.gpr[8] = behind;
    window_cpu.gpr[9] = u32::from(u16::from_be_bytes([bytes[12], bytes[13]]) != 0);
    window_cpu.gpr[10] = u32::from_be_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);
    let window = ppc_new_window_from_cpu(
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
        host_menu_bar_hidden,
    );
    if window == 0 {
        *last_resource_error = PPC_PARAM_ERR;
        return 0;
    }

    // GetNewCWindow calls GetNewPalette with the window resource ID and
    // associates the resulting palette with the new window. If that palette
    // is absent, the application palette ('pltt' 0) is the default.
    // Inside Macintosh Volume VI (1991), pp. 20-18--20-19.
    let palette = ppc_copy_palette_resource(
        Some(&mut allocator),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        current_resource_refnum,
        window_id,
    );
    let palette = if palette == 0 && window_id != 0 {
        ppc_copy_palette_resource(
            Some(&mut allocator),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            0,
        )
    } else {
        palette
    };
    if palette != 0 {
        let _ = memory.write_u32_be(window + PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET, palette);
        let _ = memory.write_u16_be(window + PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET, 1);
    }
    *last_resource_error = PPC_NO_ERR;
    *last_mem_error = PPC_NO_ERR;
    window
}

pub(super) fn ppc_size_window_dimensions(
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
    window_ptr: u32,
    width: u32,
    height: u32,
) -> Option<()> {
    // Inside Macintosh: Imaging With QuickDraw (1994), Basic QuickDraw,
    // p. 2-46: PortSize changes only the port rectangle's size and preserves
    // the graphics port's coordinate system and boundary rectangle.
    let (top, left, _, _) = ppc_read_rect(memory, window_ptr.checked_add(16)?)?;
    let pixmap_handle = memory.read_u32_be(window_ptr.checked_add(2)?)?;
    let pixmap = memory.read_u32_be(pixmap_handle)?;
    let (pixel_top, pixel_left, _, _) = ppc_read_rect(memory, pixmap.checked_add(6)?)?;
    // Window geometry lives in contRgn's global coordinates. A client can
    // rebind the port PixMap to a display buffer before sizing the window.
    // Macintosh Toolbox Essentials (1992), p. 4-66.
    let (global_top, global_left) = memory
        .read_u32_be(window_ptr.checked_add(PPC_CWINDOW_CONTENT_RGN_OFFSET)?)
        .and_then(|region| ppc_read_rgn_bbox(memory, region))
        .map(|(global_top, global_left, _, _)| (global_top, global_left))
        .unwrap_or((
            top.saturating_sub(pixel_top),
            left.saturating_sub(pixel_left),
        ));
    let bottom = ppc_i32_to_i16_saturating(i32::from(top).saturating_add(height as i32));
    let right = ppc_i32_to_i16_saturating(i32::from(left).saturating_add(width as i32));

    ppc_write_rect(
        memory,
        window_ptr.checked_add(16)?,
        top,
        left,
        bottom,
        right,
    )?;
    if let Some(record) = gworlds.iter_mut().find(|record| record.port == window_ptr) {
        record.width = width;
        record.height = height;
    }
    ppc_update_window_manager_regions(
        memory,
        window_ptr,
        (
            global_top,
            global_left,
            ppc_i32_to_i16_saturating(i32::from(global_top).saturating_add(height as i32)),
            ppc_i32_to_i16_saturating(i32::from(global_left).saturating_add(width as i32)),
        ),
    )?;

    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] SizeWindow window=${:08X} width={} height={} bounds=({}, {}, {}, {})",
            window_ptr, width, height, top, left, bottom, right
        );
    }
    Some(())
}

pub(super) fn ppc_size_window(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
) -> Option<()> {
    let params = crate::window_manager::evaluate_size_window_parameters(
        cpu.gpr[3],
        cpu.gpr[4] as u16 as i16,
        cpu.gpr[5] as u16 as i16,
        cpu.gpr[6] != 0,
    )?;
    let (width, height) = params.clamped_dimensions();
    ppc_size_window_dimensions(memory, gworlds, params.window_ptr(), width, height)
}

pub(super) fn ppc_move_window_coordinates(
    memory: &mut PpcSectionMem,
    _gworlds: &mut [PpcGWorldRecord],
    window_ptr: u32,
    new_left: i16,
    new_top: i16,
) -> Option<()> {
    // Inside Macintosh: Macintosh Toolbox Essentials (1992), Window Manager,
    // "MoveWindow": moving a window changes its global location without
    // affecting the local coordinates of its upper-left corner. Shift the
    // screen boundary rectangle and leave the local port rectangle intact.
    let (port_top, port_left, port_bottom, port_right) =
        ppc_read_rect(memory, window_ptr.checked_add(16)?)?;
    let pixmap_handle = memory.read_u32_be(window_ptr.checked_add(2)?)?;
    let pixmap = memory.read_u32_be(pixmap_handle)?;
    let (pixel_top, pixel_left, pixel_bottom, pixel_right) =
        ppc_read_rect(memory, pixmap.checked_add(6)?)?;
    let pixel_height = i32::from(pixel_bottom) - i32::from(pixel_top);
    let pixel_width = i32::from(pixel_right) - i32::from(pixel_left);
    let new_pixel_top = ppc_i32_to_i16_saturating(i32::from(port_top) - i32::from(new_top));
    let new_pixel_left = ppc_i32_to_i16_saturating(i32::from(port_left) - i32::from(new_left));
    let new_pixel_bottom = ppc_i32_to_i16_saturating(i32::from(new_pixel_top) + pixel_height);
    let new_pixel_right = ppc_i32_to_i16_saturating(i32::from(new_pixel_left) + pixel_width);
    ppc_write_rect(
        memory,
        pixmap + 6,
        new_pixel_top,
        new_pixel_left,
        new_pixel_bottom,
        new_pixel_right,
    )?;
    ppc_update_window_manager_regions(
        memory,
        window_ptr,
        (
            new_top,
            new_left,
            ppc_i32_to_i16_saturating(
                i32::from(new_top).saturating_add(i32::from(port_bottom.saturating_sub(port_top))),
            ),
            ppc_i32_to_i16_saturating(
                i32::from(new_left).saturating_add(i32::from(port_right.saturating_sub(port_left))),
            ),
        ),
    )?;

    // portRect is guest-writable and describes local coordinates, not the
    // allocated backing surface. It may legitimately differ from the cached
    // GWorld dimensions. Moving translates PixMap bounds while preserving
    // their extent; it must neither resize the backing nor assert equality
    // between these independent rectangles.

    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] MoveWindow window=${:08X} left={} top={} port_rect=({}, {}, {}, {}) pixel_bounds=({}, {}, {}, {})",
            window_ptr,
            new_left,
            new_top,
            port_top,
            port_left,
            port_bottom,
            port_right,
            new_pixel_top,
            new_pixel_left,
            new_pixel_bottom,
            new_pixel_right
        );
    }
    Some(())
}

pub(super) fn ppc_move_window(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
) -> Option<()> {
    let params = crate::window_manager::evaluate_move_window_parameters(
        cpu.gpr[3],
        cpu.gpr[4] as u16 as i16,
        cpu.gpr[5] as u16 as i16,
        cpu.gpr[6] != 0,
    )?;
    ppc_move_window_coordinates(
        memory,
        gworlds,
        params.window_ptr(),
        params.h_global(),
        params.v_global(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_paint_one(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
    clobbered_rgn: u32,
    host_menu_bar_hidden: bool,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    if window == 0 {
        ppc_paint_behind(
            allocator.as_deref_mut(),
            memory,
            gworlds,
            0,
            clobbered_rgn,
            host_menu_bar_hidden,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
        );
        return;
    }
    let Some(record) = gworlds.iter().find(|record| record.port == window) else {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    };
    let Some((port_top, port_left, port_bottom, port_right)) =
        ppc_read_rect(memory, record.port.wrapping_add(16))
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    };
    let Some((pixel_top, pixel_left, _, _)) = ppc_read_rect(memory, record.pixmap.wrapping_add(6))
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    };
    let content = (
        port_top.saturating_sub(pixel_top),
        port_left.saturating_sub(pixel_left),
        port_bottom.saturating_sub(pixel_top),
        port_right.saturating_sub(pixel_left),
    );
    let clobbered = ppc_read_rgn_bbox(memory, clobbered_rgn)
        .filter(|rect| rect.0 < rect.2 && rect.1 < rect.3)
        .unwrap_or(content);
    let exposed = (
        content.0.max(clobbered.0),
        content.1.max(clobbered.1),
        content.2.min(clobbered.2),
        content.3.min(clobbered.3),
    );
    if exposed.0 >= exposed.2 || exposed.1 >= exposed.3 {
        *last_mem_error = PPC_NO_ERR;
        return;
    }

    let update_addr = record.port.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET);
    let mut update_rgn = memory.read_u32_be(update_addr).unwrap_or(0);
    if ppc_rgn_ptr(memory, update_rgn).is_none() {
        update_rgn = ppc_allocator_view_new_rgn(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
        );
        if update_rgn == 0 || memory.write_u32_be(update_addr, update_rgn).is_none() {
            *last_mem_error = PPC_MEM_FULL_ERR;
            return;
        }
    }
    let previous = ppc_read_rgn_bbox(memory, update_rgn).unwrap_or((0, 0, 0, 0));
    let combined = if previous.0 >= previous.2 || previous.1 >= previous.3 {
        exposed
    } else {
        (
            previous.0.min(exposed.0),
            previous.1.min(exposed.1),
            previous.2.max(exposed.2),
            previous.3.max(exposed.3),
        )
    };
    if ppc_write_rgn_bbox(
        memory, update_rgn, combined.0, combined.1, combined.2, combined.3,
    )
    .is_none()
    {
        *last_mem_error = PPC_PARAM_ERR;
        return;
    }

    // Macintosh Toolbox Essentials (1992), p. 4-118: PaintOne erases the
    // exposed content with the window's background before adding it to the
    // update region. A color window uses its content entry in the WCTab.
    // These Window Manager coordinates are global screen-buffer pixels.
    let background = ppc_window_color_table_handle(memory, window)
        .and_then(|handle| ppc_window_content_color(memory, handle))
        .unwrap_or(PPC_RGB_WHITE);
    let visible = memory
        .read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
        .and_then(|region| ppc_region_storage(memory, region));
    let (port_top, port_left) = ppc_live_quickdraw_surface(memory, gworlds, window)
        .map(|surface| (i32::from(surface.top), i32::from(surface.left)))
        .unwrap_or((0, 0));
    if let Some(front_buffer) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) {
        for v in i32::from(exposed.0)..i32::from(exposed.2) {
            for h in i32::from(exposed.1)..i32::from(exposed.3) {
                if visible.as_deref().is_some_and(|region| {
                    !ppc_point_in_region_storage(
                        region,
                        (h + port_left) as i16,
                        (v + port_top) as i16,
                    )
                }) {
                    continue;
                }
                let _ = ppc_quickdraw_write_pixel(memory, front_buffer, (h, v), background);
            }
        }
    }
    *last_mem_error = PPC_NO_ERR;
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_calc_vis_behind(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    start_window: u32,
    clobbered_rgn: u32,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let Some(clobbered) = ppc_read_rgn_bbox(memory, clobbered_rgn) else {
        return;
    };
    let mut windows = gworlds
        .iter()
        .rev()
        .filter(|record| {
            !matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
                && ppc_window_is_visible(memory, record.port)
        })
        .collect::<Vec<_>>();
    if start_window != 0 {
        if let Some(index) = windows
            .iter()
            .position(|record| record.port == start_window)
        {
            windows.drain(0..index);
        } else {
            windows.clear();
        }
    }

    for record in windows {
        let Some((port_top, port_left, port_bottom, port_right)) =
            ppc_read_rect(memory, record.port.wrapping_add(16))
        else {
            continue;
        };
        let Some((pixel_top, pixel_left, _, _)) =
            ppc_read_rect(memory, record.pixmap.wrapping_add(6))
        else {
            continue;
        };
        let global = (
            port_top.saturating_sub(pixel_top),
            port_left.saturating_sub(pixel_left),
            port_bottom.saturating_sub(pixel_top),
            port_right.saturating_sub(pixel_left),
        );
        let intersects = global.0 < clobbered.2
            && clobbered.0 < global.2
            && global.1 < clobbered.3
            && clobbered.1 < global.3;
        if !intersects {
            continue;
        }

        let mbar_height = memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i16;
        let local_menu_bottom = mbar_height.saturating_add(pixel_top);
        let local_top = port_top.max(local_menu_bottom);
        let vis_rgn_addr = record.port.wrapping_add(24);
        let mut vis_rgn = memory.read_u32_be(vis_rgn_addr).unwrap_or(0);
        if ppc_rgn_ptr(memory, vis_rgn).is_none() {
            vis_rgn = ppc_allocator_view_new_rgn(
                allocator.as_deref_mut(),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            if vis_rgn == 0 || memory.write_u32_be(vis_rgn_addr, vis_rgn).is_none() {
                *last_mem_error = PPC_MEM_FULL_ERR;
                return;
            }
        }
        if ppc_write_rgn_bbox(
            memory,
            vis_rgn,
            local_top,
            port_left,
            port_bottom,
            port_right,
        )
        .is_none()
        {
            *last_mem_error = PPC_PARAM_ERR;
            return;
        }
    }

    // Macintosh Toolbox Essentials (1992), p. 4-119: CalcVisBehind walks
    // startWindow and every window behind it whose content intersects the
    // clobbered desktop region, recalculating each window's local visRgn.
    *last_mem_error = PPC_NO_ERR;
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_paint_behind(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    start_window: u32,
    clobbered_rgn: u32,
    host_menu_bar_hidden: bool,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) {
    let Some(clobbered) = ppc_read_rgn_bbox(memory, clobbered_rgn) else {
        return;
    };
    let mut windows = gworlds
        .iter()
        .rev()
        .filter(|record| {
            !matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
                && ppc_window_is_visible(memory, record.port)
        })
        .collect::<Vec<_>>();
    if start_window != 0 {
        if let Some(index) = windows
            .iter()
            .position(|record| record.port == start_window)
        {
            windows.drain(0..index);
        } else {
            windows.clear();
        }
    } else {
        windows.clear();
    }

    for record in windows {
        let Some((port_top, port_left, port_bottom, port_right)) =
            ppc_read_rect(memory, record.port.wrapping_add(16))
        else {
            continue;
        };
        let Some((pixel_top, pixel_left, _, _)) =
            ppc_read_rect(memory, record.pixmap.wrapping_add(6))
        else {
            continue;
        };
        let content = (
            port_top.saturating_sub(pixel_top),
            port_left.saturating_sub(pixel_left),
            port_bottom.saturating_sub(pixel_top),
            port_right.saturating_sub(pixel_left),
        );
        let exposed = (
            content.0.max(clobbered.0),
            content.1.max(clobbered.1),
            content.2.min(clobbered.2),
            content.3.min(clobbered.3),
        );
        if exposed.0 >= exposed.2 || exposed.1 >= exposed.3 {
            continue;
        }

        let update_addr = record.port.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET);
        let mut update_rgn = memory.read_u32_be(update_addr).unwrap_or(0);
        if ppc_rgn_ptr(memory, update_rgn).is_none() {
            update_rgn = ppc_allocator_view_new_rgn(
                allocator.as_deref_mut(),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            if update_rgn == 0 || memory.write_u32_be(update_addr, update_rgn).is_none() {
                *last_mem_error = PPC_MEM_FULL_ERR;
                return;
            }
        }
        let update = ppc_read_rgn_bbox(memory, update_rgn).unwrap_or((0, 0, 0, 0));
        let combined = if update.0 >= update.2 || update.1 >= update.3 {
            exposed
        } else {
            (
                update.0.min(exposed.0),
                update.1.min(exposed.1),
                update.2.max(exposed.2),
                update.3.max(exposed.3),
            )
        };
        if ppc_write_rgn_bbox(
            memory, update_rgn, combined.0, combined.1, combined.2, combined.3,
        )
        .is_none()
        {
            *last_mem_error = PPC_PARAM_ERR;
            return;
        }
    }

    if start_window == 0 {
        // PaintBehind(NIL) reaches the desktop only where visible window
        // structures do not cover the clobbered region. Preserve pixels in
        // those windows; their content is repainted by their update events.
        let covered = gworlds
            .iter()
            .filter_map(|record| {
                if matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
                    || !ppc_window_is_visible(memory, record.port)
                {
                    return None;
                }
                ppc_window_global_structure_bounds(memory, gworlds, record.port)
            })
            .collect::<Vec<_>>();
        let desktop = if host_menu_bar_hidden {
            (
                0,
                0,
                ppc_main_screen_height() as i16,
                ppc_main_screen_width() as i16,
            )
        } else {
            ppc_read_rgn_bbox(memory, PPC_GRAY_RGN_HANDLE).unwrap_or((
                20,
                0,
                ppc_main_screen_height() as i16,
                ppc_main_screen_width() as i16,
            ))
        };
        let paint = (
            desktop.0.max(clobbered.0),
            desktop.1.max(clobbered.1),
            desktop.2.min(clobbered.2),
            desktop.3.min(clobbered.3),
        );
        if paint.0 < paint.2 && paint.1 < paint.3 {
            if let Some(front_buffer) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) {
                if let Some((ink, paper)) =
                    ppc_standard_desktop_pixels(memory, gworlds, front_buffer)
                {
                    for v in i32::from(paint.0)..i32::from(paint.2) {
                        for h in i32::from(paint.1)..i32::from(paint.3) {
                            if covered.iter().any(|&(top, left, bottom, right)| {
                                i32::from(top) <= v
                                    && v < i32::from(bottom)
                                    && i32::from(left) <= h
                                    && h < i32::from(right)
                            }) {
                                continue;
                            }
                            let pixel =
                                if crate::window_manager::standard_desktop_pattern_is_ink(h, v) {
                                    ink
                                } else {
                                    paper
                                };
                            let _ =
                                ppc_quickdraw_write_raw_pixel(memory, front_buffer, (h, v), pixel);
                        }
                    }
                }
            }
        }
    }

    // Macintosh Toolbox Essentials (1992), pp. 4-117--4-119: PaintBehind
    // paints the desktop for NIL and otherwise adds exposed content to the
    // update regions of startWindow and each window behind it, clipped to the
    // caller's clobbered region. PaintWhite is clear for this operation.
    *last_mem_error = PPC_NO_ERR;
}

pub(super) fn ppc_window_is_visible(memory: &mut PpcSectionMem, window: u32) -> bool {
    // Classic Window Manager calls write 0xFF for TRUE; native calls write 1.
    let raw = memory
        .read_u8(window.wrapping_add(PPC_CWINDOW_VISIBLE_OFFSET))
        .is_some_and(|visible| visible != 0);
    crate::window_manager::evaluate_is_window_visible(window, raw)
}

pub(super) fn ppc_front_visible_window(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
) -> Option<u32> {
    gworlds.iter().rev().map(|record| record.port).find(|port| {
        !matches!(*port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
            && ppc_window_is_visible(memory, *port)
    })
}

pub(super) fn ppc_front_visible_process_window(
    memory: &mut PpcSectionMem,
    window_list: &SharedProcessWindowList,
) -> Option<u32> {
    window_list.with_ref(|windows| {
        windows
            .iter()
            .copied()
            .find(|window| ppc_window_is_visible(memory, *window))
    })
}

pub(super) fn ppc_window_global_content_bounds(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
) -> Option<(i16, i16, i16, i16)> {
    // Inside Macintosh: Macintosh Toolbox Essentials (1992), p. 4-66:
    // contRgn is defined in global coordinates. The port's PixMap may be
    // rebound to a display surface without moving the window.
    if let Some(bounds) = memory
        .read_u32_be(window.checked_add(PPC_CWINDOW_CONTENT_RGN_OFFSET)?)
        .and_then(|region| ppc_read_rgn_bbox(memory, region))
    {
        return Some(bounds);
    }
    let (port_top, port_left, port_bottom, port_right) =
        ppc_read_rect(memory, window.checked_add(16)?)?;
    let surface = ppc_live_quickdraw_surface(memory, gworlds, window)?;
    Some((
        port_top.saturating_sub(surface.top),
        port_left.saturating_sub(surface.left),
        port_bottom.saturating_sub(surface.top),
        port_right.saturating_sub(surface.left),
    ))
}

pub(super) fn ppc_find_window_at_point(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    v: i16,
    h: i16,
    menu_bar_height: i16,
) -> (i16, u32) {
    let front_window = ppc_front_visible_process_window(memory, window_list);
    for window in window_list.windows() {
        if !ppc_window_is_visible(memory, window) {
            continue;
        }
        let Some((top, left, bottom, right)) =
            ppc_window_global_content_bounds(memory, gworlds, window)
        else {
            continue;
        };
        let is_front = front_window == Some(window);
        let proc_id = ppc_window_proc_id(memory, window);
        if is_front && matches!(proc_id, 0 | 8) {
            if v >= bottom.saturating_sub(15)
                && v < bottom
                && h >= right.saturating_sub(15)
                && h < right
            {
                return (5, window);
            }
        }
        if v >= top && v < bottom && h >= left && h < right {
            return (3, window);
        }
        let title_top = top.saturating_sub(18).max(menu_bar_height);
        if v >= title_top && v < top && h >= left && h < right {
            let go_away = memory
                .read_u8(window.wrapping_add(PPC_CWINDOW_GO_AWAY_OFFSET))
                .unwrap_or(0)
                != 0;
            if is_front && go_away && h < left.saturating_add(18) {
                return (6, window);
            }
            if is_front
                && matches!(proc_id, 8 | 12)
                && h >= right.saturating_sub(24)
                && h < right.saturating_sub(6)
            {
                return (7, window);
            }
            return (4, window);
        }
    }
    (0, 0)
}

pub(super) fn ppc_set_window_visible(
    memory: &mut PpcSectionMem,
    window: u32,
    visible: bool,
) -> bool {
    if window == 0 {
        return false;
    }
    // Inside Macintosh: Macintosh Toolbox Essentials (1992), pp. 4-65 and
    // 4-88--4-89: WindowRecord.visible is the guest-visible source of truth
    // updated by ShowWindow, HideWindow, and ShowHide.
    memory
        .write_u8(
            window.wrapping_add(PPC_CWINDOW_VISIBLE_OFFSET),
            u8::from(visible),
        )
        .is_some()
}

pub(super) fn ppc_set_window_hilited(
    memory: &mut PpcSectionMem,
    window: u32,
    hilited: bool,
) -> bool {
    if window == 0 {
        return false;
    }
    memory
        .write_u8(
            window.wrapping_add(PPC_CWINDOW_HILITED_OFFSET),
            u8::from(hilited),
        )
        .is_some()
}

fn ppc_window_is_dialog(memory: &mut PpcSectionMem, window: u32) -> bool {
    window != 0
        && memory.read_u16_be(window.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
            == Some(crate::dialog_manager::DIALOG_WINDOW_KIND)
}

fn ppc_window_color_table_handle(memory: &mut PpcSectionMem, window: u32) -> Option<u32> {
    // DialogRecord overlays the CWindowRecord extension at +164 with its
    // editField/editOpen state. Those bytes are never an AuxWin color table.
    if window == 0 || ppc_window_is_dialog(memory, window) {
        return None;
    }
    memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_COLOR_TABLE_HANDLE_OFFSET))
        .filter(|handle| *handle != 0)
}

pub(super) fn ppc_window_content_color(
    memory: &mut PpcSectionMem,
    color_table_handle: u32,
) -> Option<PpcRgbColor> {
    let color_table = memory.read_u32_be(color_table_handle)?;
    let last_entry = memory.read_u16_be(color_table + 6)? as i16;
    if last_entry < 0 {
        return None;
    }
    for entry_index in 0..=u32::from(last_entry as u16).min(4095) {
        let entry = color_table.checked_add(8 + entry_index * 8)?;
        if memory.read_u16_be(entry)? == 0 {
            return Some(PpcRgbColor {
                red: memory.read_u16_be(entry + 2)?,
                green: memory.read_u16_be(entry + 4)?,
                blue: memory.read_u16_be(entry + 6)?,
            });
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcLegacyWindowOperation {
    ActiveNonFloatingWindow,
    BringToFront,
    CalculateVisibleRegion,
    ChangeWindowAttributes,
    ChangeWindowPropertyAttributes,
    CheckUpdate,
    CreateNewWindow,
    DisposeWindow,
    DragWindow,
    GetAuxWin,
    GetNewWindow,
    GetNextWindow,
    GetPreviousWindow,
    GetUserFocusWindow,
    GetWindowAttributes,
    GetWindowBounds,
    GetWindowCancelButton,
    GetWindowContentRgn,
    GetWindowDefProc,
    GetWindowDefaultButton,
    GetWindowFeatures,
    GetWindowFromPort,
    GetWindowGoAwayFlag,
    GetWindowGreatestArea,
    GetWindowIdealUserState,
    GetWindowKind,
    GetWindowModality,
    GetWindowPic,
    GetWindowPort,
    GetWindowPortBounds,
    GetWindowProperty,
    GetWindowPropertyAttributes,
    GetWindowPropertySize,
    GetWindowProxyIcon,
    GetWindowRegion,
    GetWindowSpareFlag,
    GetWindowStandardState,
    GetWindowStructureRgn,
    GetWindowStructureWidths,
    GetWindowTitle,
    GetWindowUpdateRgn,
    GetWindowUserState,
    GetWRefCon,
    GetWVariant,
    GrowWindow,
    HighlightWindow,
    InvalWindowRect,
    InvalWindowRgn,
    IsWindowActive,
    IsWindowHilited,
    IsWindowModified,
    IsWindowPathSelectClick,
    IsWindowUpdatePending,
    IsWindowVisible,
    LMGetAuxWinHead,
    LMGetWindowList,
    LMSetAuxWinHead,
    LMSetWindowList,
    NewWindow,
    RemoveWindowProperty,
    RemoveWindowProxy,
    RepositionWindow,
    ReshapeCustomWindow,
    SendBehind,
    SetPortWindowPort,
    SetUserFocusWindow,
    SetWindowBounds,
    SetWindowCancelButton,
    SetWindowDefaultButton,
    SetWindowIdealUserState,
    SetWindowKind,
    SetWindowModality,
    SetWindowModified,
    SetWindowPic,
    SetWindowProperty,
    SetWindowProxyIcon,
    SetWindowStandardState,
    SetWindowTitle,
    SetWindowUserState,
    SetWRefCon,
    TrackBox,
    TrackGoAway,
    ValidWindowRect,
    ValidWindowRgn,
    ZoomWindow,
}

pub(super) fn ppc_dispatch_legacy_window(
    operation: PpcLegacyWindowOperation,
    cpu: &mut PpcCpu,
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
    quickdraw_fore_color: &mut PpcRgbColor,
    quickdraw_fore_indices: &mut HashMap<u32, u8>,
    quickdraw_back_color: &mut PpcRgbColor,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
    input: PpcInputSnapshot,
    when: u32,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    vfs_resources: &mut [PpcVfsResourceRecord],
    current_resource_refnum: i16,
    last_resource_error: &mut i16,
) -> Option<PpcImportAction> {
    match operation {
        PpcLegacyWindowOperation::CheckUpdate => {
            let params = crate::window_manager::evaluate_check_update_parameters(cpu.gpr[3]);
            // Macintosh Toolbox Essentials (1992), p. 4-116: scan the
            // visible windows front to back. Pictured windows are redrawn
            // internally; the first ordinary dirty window produces an event.
            for window in window_list.windows() {
                if !ppc_window_is_visible(memory, window) {
                    continue;
                }
                let update_region = memory
                    .read_u32_be(window + PPC_CWINDOW_UPDATE_RGN_OFFSET)
                    .unwrap_or(0);
                let dirty = ppc_read_rgn_bbox(memory, update_region)
                    .is_some_and(|(top, left, bottom, right)| top < bottom && left < right);
                if !dirty {
                    continue;
                }
                let picture = memory
                    .read_u32_be(window + PPC_CWINDOW_WINDOW_PIC_OFFSET)
                    .unwrap_or(0);
                if picture != 0 && memory.read_u32_be(picture).unwrap_or(0) != 0 {
                    let saved_r3 = cpu.gpr[3];
                    let saved_r4 = cpu.gpr[4];
                    cpu.gpr[3] = picture;
                    cpu.gpr[4] = window + 16;
                    let _ = ppc_draw_picture(
                        cpu,
                        memory,
                        handles,
                        vfs_resources,
                        gworlds,
                        window,
                        screen_clut,
                        color_manager_clut,
                    );
                    cpu.gpr[3] = saved_r3;
                    cpu.gpr[4] = saved_r4;
                    let _ = ppc_set_empty_rgn(memory, update_region);
                    if let Some(index) = event_queue
                        .iter()
                        .position(|event| event.what == 6 && event.message == window)
                    {
                        event_queue.remove(index);
                    }
                    continue;
                }
                let event_ptr = params.event_ptr();
                if event_ptr != 0 {
                    let _ = ppc_write_event_record(
                        memory,
                        event_ptr,
                        6,
                        window,
                        when,
                        input.mouse_v,
                        input.mouse_h,
                        0,
                    );
                }
                if let Some(index) = event_queue
                    .iter()
                    .position(|event| event.what == 6 && event.message == window)
                {
                    event_queue.remove(index);
                }
                return Some(PpcImportAction::Return(1));
            }
            Some(PpcImportAction::Return(0))
        }
        PpcLegacyWindowOperation::NewWindow => {
            let previous_front = ppc_front_visible_process_window(memory, window_list);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let window = ppc_new_window_from_cpu(
                cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                window_list,
                *current_gdevice,
                toolbox_startup.host_menu_bar_hidden,
            );
            if window != 0 {
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                *current_gworld = window;
                let next_front = ppc_front_visible_process_window(memory, window_list);
                ppc_enqueue_window_activation_transition(
                    memory,
                    event_queue,
                    previous_front,
                    next_front,
                    when,
                );
                if next_front != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
            }
            Some(PpcImportAction::Return(window))
        }
        PpcLegacyWindowOperation::CreateNewWindow => {
            // Carbon Window Manager, CreateNewWindow: the bounds describe the
            // content region, and the returned WindowRef starts hidden.
            // Apple, Handling Carbon Windows and Controls, "Window and Control Tasks".
            let params = match crate::window_manager::evaluate_create_new_window_parameters(
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[6],
            ) {
                Ok(params) => params,
                Err(err) => return Some(PpcImportAction::Return(ppc_i16_result(err))),
            };
            let window_class = params.window_class();
            let attributes = params.attributes();
            let bounds = params.bounds_ptr();
            let out_window = params.out_window_ptr();
            if ppc_read_rect(memory, bounds).is_none()
                || !ppc_memory_can_write_bytes(memory, out_window, 4)
            {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }

            let mut window_cpu = cpu.clone();
            window_cpu.gpr[3] = 0;
            window_cpu.gpr[4] = bounds;
            window_cpu.gpr[5] = 0;
            window_cpu.gpr[6] = 0;
            window_cpu.gpr[7] = if window_class == 13 { 2 } else { 0 };
            window_cpu.gpr[8] = u32::MAX;
            window_cpu.gpr[9] = u32::from(attributes & 1 != 0);
            window_cpu.gpr[10] = 0;
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let window = ppc_new_window_from_cpu(
                &window_cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                window_list,
                *current_gdevice,
                toolbox_startup.host_menu_bar_hidden,
            );
            if window == 0 {
                return Some(PpcImportAction::Return(ppc_i16_result(*last_mem_error)));
            }
            if attributes & (1 << 16) != 0 {
                toolbox_startup.windows_without_updates.insert(window);
            }
            let _ = memory.write_u32_be(out_window, window);
            ppc_recalculate_window_vis_regions(
                process_memory_manager,
                memory,
                window_list,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcLegacyWindowOperation::RepositionWindow => {
            // Apple, Handling Carbon Windows and Controls, "Window and Control
            // Tasks": RepositionWindow accepts a WindowPositionMethod.
            let params = match crate::window_manager::evaluate_reposition_window_parameters(
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5] as u16,
            ) {
                Ok(params) => params,
                Err(err) => return Some(PpcImportAction::Return(ppc_i16_result(err))),
            };
            let window = params.window_ptr();
            let parent_window = params.parent_window_ptr();
            let method = params.method();
            let Some(content) = ppc_window_global_content_bounds(memory, gworlds, window) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let Some(structure) = ppc_window_global_structure_bounds(memory, gworlds, window)
            else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let parent_structure = if parent_window != 0 {
                let Some(parent_bounds) =
                    ppc_window_global_structure_bounds(memory, gworlds, parent_window)
                else {
                    return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                };
                Some(parent_bounds)
            } else {
                None
            };
            let menu_height = i32::from(memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20));
            let screen_width = ppc_main_screen_width() as i32;
            let screen_height = ppc_main_screen_height() as i32;
            let Some(new_bounds) = crate::window_manager::evaluate_reposition_window_bounds(
                content,
                structure,
                parent_structure,
                method,
                screen_width,
                screen_height,
                menu_height,
            ) else {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            };
            let new_left = new_bounds.1;
            let new_top = new_bounds.0;
            let was_visible = ppc_window_is_visible(memory, window);
            let mut move_cpu = cpu.clone();
            move_cpu.gpr[4] = new_left as u16 as u32;
            move_cpu.gpr[5] = new_top as u16 as u32;
            if ppc_move_window(&move_cpu, memory, gworlds).is_none() {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            ppc_recalculate_window_vis_regions(
                process_memory_manager,
                memory,
                window_list,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            let next_structure = ppc_window_global_structure_bounds(memory, gworlds, window);
            ppc_repaint_window_geometry_transition(
                memory,
                gworlds,
                window_list,
                window,
                was_visible,
                Some(structure),
                next_structure,
                toolbox_startup.host_menu_bar_hidden,
                event_queue,
                when,
                input,
            );
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcLegacyWindowOperation::GetNewWindow => {
            let params = crate::window_manager::evaluate_get_new_window_parameters(
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[4],
                cpu.gpr[5],
            );
            let previous_front = ppc_front_visible_process_window(memory, window_list);
            let resource_id = params.window_id();
            let Some(index) = ppc_vfs_resource_index(
                vfs_resources,
                current_resource_refnum,
                u32::from_be_bytes(*b"WIND"),
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
            let Some((bounds, title)) = ppc_materialize_window_resource_parameters(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                &bytes,
            ) else {
                *last_resource_error = PPC_PARAM_ERR;
                return Some(PpcImportAction::Return(0));
            };
            let mut window_cpu = cpu.clone();
            window_cpu.gpr[3] = params.storage_ptr();
            window_cpu.gpr[4] = bounds;
            window_cpu.gpr[5] = title;
            window_cpu.gpr[6] = u32::from(u16::from_be_bytes([bytes[10], bytes[11]]) != 0);
            window_cpu.gpr[7] = u32::from(u16::from_be_bytes([bytes[8], bytes[9]]));
            window_cpu.gpr[8] = params.behind();
            window_cpu.gpr[9] = u32::from(u16::from_be_bytes([bytes[12], bytes[13]]) != 0);
            window_cpu.gpr[10] = u32::from_be_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);
            let window = ppc_new_window_from_cpu(
                &window_cpu,
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                window_list,
                *current_gdevice,
                toolbox_startup.host_menu_bar_hidden,
            );
            *last_resource_error = if window == 0 {
                PPC_PARAM_ERR
            } else {
                PPC_NO_ERR
            };
            if window != 0 {
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                *current_gworld = window;
                let next_front = ppc_front_visible_process_window(memory, window_list);
                ppc_enqueue_window_activation_transition(
                    memory,
                    event_queue,
                    previous_front,
                    next_front,
                    when,
                );
                if next_front != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
            }
            Some(PpcImportAction::Return(window))
        }
        PpcLegacyWindowOperation::GetWindowTitle => {
            let window = cpu.gpr[3];
            let out_title = cpu.gpr[4];
            if let Ok(params) = crate::window_manager::evaluate_get_window_title_parameters(window, out_title) {
                let title = memory
                    .read_u32_be(params.window_ptr().wrapping_add(PPC_CWINDOW_TITLE_HANDLE_OFFSET))
                    .filter(|handle| *handle != 0)
                    .and_then(|handle| memory.read_u32_be(handle))
                    .filter(|ptr| *ptr != 0)
                    .and_then(|ptr| ppc_read_pstring_bytes(memory, ptr))
                    .unwrap_or_default();
                if params.out_title_ptr() != 0 {
                    let _ = ppc_write_pstring_bytes(memory, params.out_title_ptr(), &title);
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::SetWindowTitle => {
            let window = cpu.gpr[3];
            let title = cpu.gpr[4];
            if let Ok(params) = crate::window_manager::evaluate_set_window_title_parameters(window, title) {
                let mut allocator = PpcProcessAllocatorView {
                    memory_manager: process_memory_manager,
                };
                let changed = ppc_set_window_title(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    params.window_ptr(),
                    params.title_ptr(),
                );
                if changed {
                    ppc_redraw_visible_window_frame(
                        memory,
                        gworlds,
                        window_list,
                        params.window_ptr(),
                        toolbox_startup.host_menu_bar_hidden,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::DisposeWindow => {
            let Some(params) = crate::window_manager::evaluate_dispose_window_parameters(cpu.gpr[3]) else {
                return Some(PpcImportAction::ReturnPreserve);
            };
            let window = params.window_ptr();
            toolbox_startup.windows_without_updates.remove(&window);
            let previous_front = ppc_front_visible_process_window(memory, window_list);
            let was_visible = ppc_window_is_visible(memory, window);
            let exposed = was_visible
                .then(|| {
                    memory
                        .read_u32_be(window.wrapping_add(PPC_CWINDOW_STRUCTURE_RGN_OFFSET))
                        .and_then(|region| ppc_read_rgn_bbox(memory, region))
                })
                .flatten();
            let disposed_palette = memory
                .read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
                .unwrap_or(0);
            let disposed_pixmap_handle = gworlds
                .iter()
                .find(|record| {
                    record.port == window
                        && !matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
                })
                .map(|record| record.pixmap_handle);
            let was_current = *current_gworld == window;
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
                window,
            );
            ppc_recalculate_window_vis_regions(
                process_memory_manager,
                memory,
                window_list,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            ppc_transition_front_window_chrome(
                memory,
                gworlds,
                window_list,
                previous_front,
                toolbox_startup.host_menu_bar_hidden,
            );
            ppc_restore_window_removal_exposure(
                memory,
                gworlds,
                window_list,
                exposed,
                toolbox_startup.host_menu_bar_hidden,
                event_queue,
                when,
                input,
            );
            if let Some(pixmap_handle) = disposed_pixmap_handle {
                toolbox_startup
                    .indexed_screen_ctables
                    .remove(&pixmap_handle);
                quickdraw_fore_indices.remove(&window);
                let still_associated = toolbox_startup.application_palette == disposed_palette
                    || gworlds.iter().any(|record| {
                        memory
                            .read_u32_be(
                                record
                                    .port
                                    .wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET),
                            )
                            .unwrap_or(0)
                            == disposed_palette
                    });
                if disposed_palette != 0 && !still_associated {
                    ppc_release_palette_allocations_and_restore(
                        memory,
                        toolbox_startup,
                        disposed_palette,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                    );
                }
            }
            if was_current && *current_gworld != window {
                ppc_restore_port_colors(
                    memory,
                    *current_gworld,
                    quickdraw_fore_color,
                    quickdraw_back_color,
                );
            }
            if ppc_front_visible_process_window(memory, window_list) != previous_front {
                let _ = ppc_activate_front_window_palette(
                    memory,
                    gworlds,
                    *current_gdevice,
                    screen_clut,
                    color_manager_clut,
                    toolbox_startup,
                );
            }
            if *current_gworld != PPC_MAIN_GWORLD {
                ppc_enqueue_window_update_event(event_queue, *current_gworld, when, input);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::HighlightWindow => {
            if let Some(params) = crate::window_manager::evaluate_highlight_window_parameters(
                cpu.gpr[3],
                cpu.gpr[4] != 0,
            ) {
                let _ = ppc_set_window_hilited(memory, params.window_ptr(), params.hilited());
                ppc_redraw_visible_window_frame(
                    memory,
                    gworlds,
                    window_list,
                    params.window_ptr(),
                    toolbox_startup.host_menu_bar_hidden,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::BringToFront => {
            if let Some(params) = crate::window_manager::evaluate_bring_to_front_parameters(cpu.gpr[3]) {
                let window = params.window_ptr();
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                ppc_reorder_window(gworlds, window_list, window, 0, true);
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                if ppc_front_visible_process_window(memory, window_list) != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::SendBehind => {
            if let Some(params) = crate::window_manager::evaluate_send_behind_parameters(
                cpu.gpr[3],
                cpu.gpr[4],
            ) {
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                ppc_reorder_window(gworlds, window_list, params.window_ptr(), params.behind_window_ptr(), false);
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                if ppc_front_visible_process_window(memory, window_list) != previous_front {
                    let _ = ppc_activate_front_window_palette(
                        memory,
                        gworlds,
                        *current_gdevice,
                        screen_clut,
                        color_manager_clut,
                        toolbox_startup,
                    );
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::DragWindow => Some(ppc_dispatch_drag_window(
            cpu,
            process_memory_manager,
            memory,
            gworlds,
            window_list,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            current_gworld,
            toolbox_startup,
            event_queue,
            screen_clut,
            when,
            input,
        )),
        PpcLegacyWindowOperation::GrowWindow => Some(ppc_dispatch_grow_window(
            cpu,
            memory,
            gworlds,
            toolbox_startup,
            event_queue,
            screen_clut,
            input,
        )),
        PpcLegacyWindowOperation::TrackBox => {
            let inside = if let Some(params) = crate::window_manager::evaluate_track_box_parameters(
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5] as u16 as i16,
            ) {
                ppc_window_part_contains_point(
                    memory,
                    gworlds,
                    params.window_ptr(),
                    params.part_code(),
                    input.mouse_v,
                    input.mouse_h,
                )
            } else {
                false
            };
            Some(PpcImportAction::Return(u32::from(inside)))
        }
        PpcLegacyWindowOperation::TrackGoAway => Some(ppc_dispatch_track_go_away(
            cpu,
            memory,
            gworlds,
            toolbox_startup,
            event_queue,
            input,
        )),
        PpcLegacyWindowOperation::ZoomWindow => {
            if let Some(params) = crate::window_manager::evaluate_zoom_window_parameters(
                cpu.gpr[3],
                cpu.gpr[4] as u16 as i16,
                cpu.gpr[5] != 0,
            ) {
                let window = params.window_ptr();
                let was_visible = ppc_window_is_visible(memory, window);
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                let previous_structure = ppc_window_global_structure_bounds(memory, gworlds, window);
                if ppc_zoom_window(cpu, memory, gworlds).is_some() {
                    if params.front() {
                        ppc_reorder_window(gworlds, window_list, window, 0, true);
                    }
                    ppc_recalculate_window_vis_regions(
                        process_memory_manager,
                        memory,
                        window_list,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                    );
                    let next_structure = ppc_window_global_structure_bounds(memory, gworlds, window);
                    ppc_repaint_window_geometry_transition(
                        memory,
                        gworlds,
                        window_list,
                        window,
                        was_visible,
                        previous_structure,
                        next_structure,
                        toolbox_startup.host_menu_bar_hidden,
                        event_queue,
                        when,
                        input,
                    );
                    ppc_transition_front_window_chrome(
                        memory,
                        gworlds,
                        window_list,
                        previous_front,
                        toolbox_startup.host_menu_bar_hidden,
                    );
                    let next_front = ppc_front_visible_process_window(memory, window_list);
                    if params.front() && next_front == Some(window) {
                        *current_gworld = window;
                        *current_gdevice =
                            ppc_gworld_device(gworlds, *current_gworld).unwrap_or(*current_gdevice);
                        ppc_register_gdevice(toolbox_startup, *current_gdevice);
                        ppc_restore_port_colors(
                            memory,
                            *current_gworld,
                            quickdraw_fore_color,
                            quickdraw_back_color,
                        );
                        let _ = ppc_set_window_hilited(memory, window, true);
                    }
                    if next_front != previous_front {
                        let _ = ppc_activate_front_window_palette(
                            memory,
                            gworlds,
                            *current_gdevice,
                            screen_clut,
                            color_manager_clut,
                            toolbox_startup,
                        );
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::CalculateVisibleRegion => {
            if let Some(params) = crate::window_manager::evaluate_calculate_visible_region_parameters(cpu.gpr[3]) {
                let window = params.window_ptr();
                let mut vis_rgn = memory
                    .read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET))
                    .unwrap_or(0);
                if vis_rgn == 0 {
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    vis_rgn = ppc_allocator_view_new_rgn(
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                    );
                    let _ = memory
                        .write_u32_be(window.wrapping_add(PPC_CGRAF_PORT_VIS_RGN_OFFSET), vis_rgn);
                }
                if vis_rgn != 0 {
                    let _ = ppc_rect_rgn(memory, vis_rgn, window.wrapping_add(16));
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::SetWindowDefaultButton => {
            let window = cpu.gpr[3];
            let control = cpu.gpr[4];
            let result = match crate::window_manager::evaluate_set_window_default_button_parameters(
                window,
                control,
            ) {
                Ok(params) => {
                    toolbox_startup.set_window_default_button(params.window_ptr(), params.control());
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowDefaultButton => {
            let window = cpu.gpr[3];
            let out_control = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_control, 4);
            let result = match crate::window_manager::evaluate_get_window_default_button_parameters(
                window,
                out_control,
                can_write,
            ) {
                Ok(params) => {
                    let button = toolbox_startup.window_default_button(params.window_ptr());
                    let _ = memory.write_u32_be(params.out_control_ptr(), button);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::SetWindowCancelButton => {
            let window = cpu.gpr[3];
            let control = cpu.gpr[4];
            let result = match crate::window_manager::evaluate_set_window_cancel_button_parameters(
                window,
                control,
            ) {
                Ok(params) => {
                    toolbox_startup.set_window_cancel_button(params.window_ptr(), params.control());
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowCancelButton => {
            let window = cpu.gpr[3];
            let out_control = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_control, 4);
            let result = match crate::window_manager::evaluate_get_window_cancel_button_parameters(
                window,
                out_control,
                can_write,
            ) {
                Ok(params) => {
                    let button = toolbox_startup.window_cancel_button(params.window_ptr());
                    let _ = memory.write_u32_be(params.out_control_ptr(), button);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::SetUserFocusWindow => {
            let window = cpu.gpr[3];
            let params = crate::window_manager::evaluate_set_user_focus_window_parameters(window);
            toolbox_startup.set_user_focus_window(params.window_ptr());
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcLegacyWindowOperation::GetUserFocusWindow => {
            let focused = toolbox_startup.user_focus_window();
            let window = crate::window_manager::evaluate_get_user_focus_window(
                focused,
                window_list.front_window(),
            );
            Some(PpcImportAction::Return(window))
        }
        PpcLegacyWindowOperation::GetWindowFromPort => {
            let port = cpu.gpr[3];
            let window = crate::window_manager::evaluate_get_window_from_port(port);
            Some(PpcImportAction::Return(window))
        }
        PpcLegacyWindowOperation::GetWindowRegion => {
            let window = cpu.gpr[3];
            let region_code = cpu.gpr[4];
            let io_rgn = cpu.gpr[5];
            let result = match crate::window_manager::evaluate_get_window_region_parameters(
                window,
                region_code,
                io_rgn,
            ) {
                Ok(params) => {
                    let bounds = if params.region_code() == crate::window_manager::WINDOW_STRUCTURE_RGN {
                        ppc_window_global_structure_bounds(memory, gworlds, params.window_ptr())
                    } else {
                        ppc_window_global_content_bounds(memory, gworlds, params.window_ptr())
                    };
                    let (top, left, bottom, right) = bounds.unwrap_or((0, 0, 100, 100));
                    let _ = ppc_write_rgn_bbox(memory, params.io_rgn(), top, left, bottom, right);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowFeatures => {
            let window = cpu.gpr[3];
            let out_features = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_features, 4);
            let result = match crate::window_manager::evaluate_get_window_features_parameters(
                window,
                out_features,
                can_write,
            ) {
                Ok(params) => {
                    let features =
                        crate::window_manager::evaluate_get_window_features(params.window_ptr());
                    let _ = memory.write_u32_be(params.out_features_ptr(), features);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowStructureWidths => {
            let window = cpu.gpr[3];
            let out_rect = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_rect, 8);
            let result = match crate::window_manager::evaluate_get_window_structure_widths_parameters(
                window,
                out_rect,
                can_write,
            ) {
                Ok(params) => {
                    let (top, left, bottom, right) =
                        crate::window_manager::evaluate_get_window_structure_widths(
                            params.window_ptr(),
                        );
                    let _ = memory.write_u16_be(params.out_rect_ptr(), top as u16);
                    let _ = memory.write_u16_be(params.out_rect_ptr().wrapping_add(2), left as u16);
                    let _ = memory.write_u16_be(params.out_rect_ptr().wrapping_add(4), bottom as u16);
                    let _ = memory.write_u16_be(params.out_rect_ptr().wrapping_add(6), right as u16);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowIdealUserState => {
            let window = cpu.gpr[3];
            let out_rect = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_rect, 8);
            let result = match crate::window_manager::evaluate_get_window_ideal_user_state_parameters(
                window,
                out_rect,
                can_write,
            ) {
                Ok(params) => {
                    let state = memory
                        .read_u32_be(params.window_ptr().wrapping_add(PPC_CWINDOW_STATE_HANDLE_OFFSET))
                        .filter(|handle| *handle != 0)
                        .and_then(|handle| memory.read_u32_be(handle))
                        .filter(|state| *state != 0);
                    let (top, left, bottom, right) = if let Some(s) = state {
                        ppc_read_rect(memory, s).unwrap_or(crate::window_manager::DEFAULT_WINDOW_USER_STATE)
                    } else if let Some((t, l, b, r)) = ppc_read_rect(memory, params.window_ptr().wrapping_add(crate::window_manager::WINDOW_PORT_RECT_OFFSET)) {
                        (t, l, b, r)
                    } else {
                        crate::window_manager::DEFAULT_WINDOW_USER_STATE
                    };
                    let _ = ppc_write_rect(memory, params.out_rect_ptr(), top, left, bottom, right);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::SetWindowIdealUserState => {
            let window = cpu.gpr[3];
            let in_rect = cpu.gpr[4];
            let can_read = ppc_memory_can_read_bytes(memory, in_rect, 8);
            let result = match crate::window_manager::evaluate_set_window_ideal_user_state_parameters(
                window,
                in_rect,
                can_read,
            ) {
                Ok(params) => {
                    if let Some((top, left, bottom, right)) = ppc_read_rect(memory, params.in_rect_ptr()) {
                        let state = memory
                            .read_u32_be(params.window_ptr().wrapping_add(PPC_CWINDOW_STATE_HANDLE_OFFSET))
                            .filter(|handle| *handle != 0)
                            .and_then(|handle| memory.read_u32_be(handle))
                            .filter(|state| *state != 0);
                        if let Some(s) = state {
                            let _ = ppc_write_rect(memory, s, top, left, bottom, right);
                        }
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowStandardState => {
            let window = cpu.gpr[3];
            let out_rect = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_rect, 8);
            let result = match crate::window_manager::evaluate_get_window_standard_state_parameters(
                window,
                out_rect,
                can_write,
            ) {
                Ok(params) => {
                    let state = memory
                        .read_u32_be(params.window_ptr().wrapping_add(PPC_CWINDOW_STATE_HANDLE_OFFSET))
                        .filter(|handle| *handle != 0)
                        .and_then(|handle| memory.read_u32_be(handle))
                        .filter(|state| *state != 0);
                    let (top, left, bottom, right) = if let Some(s) = state {
                        ppc_read_rect(memory, s.wrapping_add(8)).unwrap_or(crate::window_manager::DEFAULT_WINDOW_STANDARD_STATE)
                    } else {
                        crate::window_manager::DEFAULT_WINDOW_STANDARD_STATE
                    };
                    let _ = ppc_write_rect(memory, params.out_rect_ptr(), top, left, bottom, right);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::SetWindowStandardState => {
            let window = cpu.gpr[3];
            let in_rect = cpu.gpr[4];
            let can_read = ppc_memory_can_read_bytes(memory, in_rect, 8);
            let result = match crate::window_manager::evaluate_set_window_standard_state_parameters(
                window,
                in_rect,
                can_read,
            ) {
                Ok(params) => {
                    if let Some((top, left, bottom, right)) = ppc_read_rect(memory, params.in_rect_ptr()) {
                        let state = memory
                            .read_u32_be(params.window_ptr().wrapping_add(PPC_CWINDOW_STATE_HANDLE_OFFSET))
                            .filter(|handle| *handle != 0)
                            .and_then(|handle| memory.read_u32_be(handle))
                            .filter(|state| *state != 0);
                        if let Some(s) = state {
                            let _ = ppc_write_rect(memory, s.wrapping_add(8), top, left, bottom, right);
                        }
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::IsWindowHilited => {
            let window = cpu.gpr[3];
            let flag = if window != 0 {
                memory.read_u8(window.wrapping_add(PPC_CWINDOW_HILITED_OFFSET))
            } else {
                None
            };
            let hilited = crate::window_manager::evaluate_get_window_hilited(window, flag);
            Some(PpcImportAction::Return(u32::from(hilited)))
        }
        PpcLegacyWindowOperation::IsWindowVisible => {
            let window = cpu.gpr[3];
            let raw_visible = memory
                .read_u8(window.wrapping_add(PPC_CWINDOW_VISIBLE_OFFSET))
                .is_some_and(|visible| visible != 0);
            let visible = crate::window_manager::evaluate_is_window_visible(window, raw_visible);
            Some(PpcImportAction::Return(u32::from(visible)))
        }
        PpcLegacyWindowOperation::IsWindowModified => {
            let window = cpu.gpr[3];
            let is_mod = toolbox_startup.is_window_modified(window);
            let modified = crate::window_manager::evaluate_is_window_modified(window, is_mod);
            Some(PpcImportAction::Return(u32::from(modified)))
        }
        PpcLegacyWindowOperation::SetWindowModified => {
            let window = cpu.gpr[3];
            let modified = cpu.gpr[4] != 0;
            let result = match crate::window_manager::evaluate_set_window_modified_parameters(
                window,
                modified,
            ) {
                Ok(params) => {
                    toolbox_startup.set_window_modified(params.window_ptr(), params.modified());
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowProxyIcon => {
            let window = cpu.gpr[3];
            let out_icon = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_icon, 4);
            let result = match crate::window_manager::evaluate_get_window_proxy_icon_parameters(
                window,
                out_icon,
                can_write,
            ) {
                Ok(params) => {
                    let icon = toolbox_startup.window_proxy_icon(params.window_ptr());
                    let _ = memory.write_u32_be(params.out_icon_ptr(), icon);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::SetWindowProxyIcon => {
            let window = cpu.gpr[3];
            let icon = cpu.gpr[4];
            let result = match crate::window_manager::evaluate_set_window_proxy_icon_parameters(
                window,
                icon,
            ) {
                Ok(params) => {
                    toolbox_startup.set_window_proxy_icon(params.window_ptr(), params.icon());
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::RemoveWindowProxy => {
            let window = cpu.gpr[3];
            let result =
                match crate::window_manager::evaluate_remove_window_proxy_parameters(window) {
                    Ok(params) => {
                        toolbox_startup.remove_window_proxy_icon(params.window_ptr());
                        PPC_NO_ERR
                    }
                    Err(err) => err,
                };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::IsWindowPathSelectClick => {
            let window = cpu.gpr[3];
            let in_event = cpu.gpr[4];
            let modifiers = if in_event != 0 {
                memory.read_u16_be(in_event.wrapping_add(14)).unwrap_or(0)
            } else {
                0
            };
            let is_path_select = crate::window_manager::evaluate_is_window_path_select_click(
                window,
                modifiers,
                false,
            );
            Some(PpcImportAction::Return(u32::from(is_path_select)))
        }
        PpcLegacyWindowOperation::InvalWindowRect => {
            let window = cpu.gpr[3];
            let in_rect = cpu.gpr[4];
            let can_read = in_rect == 0 || ppc_memory_can_read_bytes(memory, in_rect, 8);
            let result = match crate::window_manager::evaluate_inval_window_rect_parameters(
                window,
                in_rect,
                can_read,
            ) {
                Ok(params) => {
                    let rect = if params.in_rect_ptr() != 0 {
                        ppc_read_rect(memory, params.in_rect_ptr())
                    } else {
                        ppc_read_rect(memory, params.window_ptr().wrapping_add(PPC_CWINDOW_PORT_RECT_OFFSET))
                    };
                    if let Some(r) = rect {
                        ppc_invalidate_window_local_rect(memory, params.window_ptr(), r);
                        ppc_enqueue_window_update_event(event_queue, params.window_ptr(), when, input);
                    }
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::InvalWindowRgn => {
            let window = cpu.gpr[3];
            let in_rgn = cpu.gpr[4];
            let result = match crate::window_manager::evaluate_inval_window_rgn_parameters(
                window,
                in_rgn,
            ) {
                Ok(params) => {
                    let rect = if params.in_rgn() != 0 {
                        ppc_read_rgn_bbox(memory, params.in_rgn())
                    } else {
                        ppc_read_rect(memory, params.window_ptr().wrapping_add(PPC_CWINDOW_PORT_RECT_OFFSET))
                    };
                    if let Some(r) = rect {
                        ppc_invalidate_window_local_rect(memory, params.window_ptr(), r);
                        ppc_enqueue_window_update_event(event_queue, params.window_ptr(), when, input);
                    }
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::ValidWindowRect => {
            let window = cpu.gpr[3];
            let in_rect = cpu.gpr[4];
            let can_read = in_rect == 0 || ppc_memory_can_read_bytes(memory, in_rect, 8);
            let result = match crate::window_manager::evaluate_valid_window_rect_parameters(
                window,
                in_rect,
                can_read,
            ) {
                Ok(params) => {
                    let rect = if params.in_rect_ptr() != 0 {
                        ppc_read_rect(memory, params.in_rect_ptr())
                    } else {
                        ppc_read_rect(memory, params.window_ptr().wrapping_add(PPC_CWINDOW_PORT_RECT_OFFSET))
                    };
                    if let Some(r) = rect {
                        ppc_validate_window_local_rect(memory, params.window_ptr(), r);
                        let update_rgn = memory
                            .read_u32_be(params.window_ptr().wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
                            .unwrap_or(0);
                        if ppc_read_rgn_bbox(memory, update_rgn).map_or(true, |(t, l, b, r)| t >= b || l >= r) {
                            event_queue.retain(|event| !(event.what == 6 && event.message == params.window_ptr()));
                        }
                    }
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::ValidWindowRgn => {
            let window = cpu.gpr[3];
            let in_rgn = cpu.gpr[4];
            let result = match crate::window_manager::evaluate_valid_window_rgn_parameters(
                window,
                in_rgn,
            ) {
                Ok(params) => {
                    let rect = if params.in_rgn() != 0 {
                        ppc_read_rgn_bbox(memory, params.in_rgn())
                    } else {
                        ppc_read_rect(memory, params.window_ptr().wrapping_add(PPC_CWINDOW_PORT_RECT_OFFSET))
                    };
                    if let Some(r) = rect {
                        ppc_validate_window_local_rect(memory, params.window_ptr(), r);
                        let update_rgn = memory
                            .read_u32_be(params.window_ptr().wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
                            .unwrap_or(0);
                        if ppc_read_rgn_bbox(memory, update_rgn).map_or(true, |(t, l, b, r)| t >= b || l >= r) {
                            event_queue.retain(|event| !(event.what == 6 && event.message == params.window_ptr()));
                        }
                    }
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowPortBounds => {
            let window = cpu.gpr[3];
            let out_bounds = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_bounds, 8);
            let result = match crate::window_manager::evaluate_get_window_port_bounds_parameters(
                window,
                out_bounds,
                can_write,
            ) {
                Ok(params) => {
                    let (top, left, bottom, right) = ppc_read_rect(
                        memory,
                        params.window_ptr().wrapping_add(crate::window_manager::WINDOW_PORT_RECT_OFFSET),
                    )
                    .unwrap_or((0, 0, 0, 0));
                    let _ = ppc_write_rect(memory, params.out_rect_ptr(), top, left, bottom, right);
                    params.out_rect_ptr()
                }
                Err(_) => 0,
            };
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::GetWindowBounds => {
            let window = cpu.gpr[3];
            let region_code = cpu.gpr[4];
            let out_rect = cpu.gpr[5];
            let can_write = ppc_memory_can_write_bytes(memory, out_rect, 8);
            let result = match crate::window_manager::evaluate_get_window_bounds_parameters(
                window,
                region_code,
                out_rect,
                can_write,
            ) {
                Ok(params) => {
                    let bounds = if params.region_code() == crate::window_manager::WINDOW_STRUCTURE_RGN {
                        ppc_window_global_structure_bounds(memory, gworlds, params.window_ptr())
                    } else {
                        ppc_window_global_content_bounds(memory, gworlds, params.window_ptr())
                    };
                    let (top, left, bottom, right) = bounds.unwrap_or(crate::window_manager::DEFAULT_WINDOW_USER_STATE);
                    let _ = ppc_write_rect(memory, params.out_rect_ptr(), top, left, bottom, right);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::SetWindowBounds => {
            let window = cpu.gpr[3];
            let region_code = cpu.gpr[4];
            let in_rect = cpu.gpr[5];
            let can_read = ppc_memory_can_read_bytes(memory, in_rect, 8);
            let result = match crate::window_manager::evaluate_set_window_bounds_parameters(
                window,
                region_code,
                in_rect,
                can_read,
            ) {
                Ok(params) => {
                    if let Some((top, left, bottom, right)) = ppc_read_rect(memory, params.in_rect_ptr()) {
                        let width = (right.saturating_sub(left)).max(1) as u32;
                        let height = (bottom.saturating_sub(top)).max(1) as u32;
                        let was_visible = ppc_window_is_visible(memory, params.window_ptr());
                        let previous_structure =
                            ppc_window_global_structure_bounds(memory, gworlds, params.window_ptr());
                        let _ = ppc_move_window_coordinates(memory, gworlds, params.window_ptr(), left, top);
                        let _ = ppc_size_window_dimensions(memory, gworlds, params.window_ptr(), width, height);
                        ppc_recalculate_window_vis_regions(
                            process_memory_manager,
                            memory,
                            window_list,
                            heap_cursor,
                            heap_limit,
                            last_mem_error,
                            handles,
                        );
                        let next_structure =
                            ppc_window_global_structure_bounds(memory, gworlds, params.window_ptr());
                        ppc_repaint_window_geometry_transition(
                            memory,
                            gworlds,
                            window_list,
                            params.window_ptr(),
                            was_visible,
                            previous_structure,
                            next_structure,
                            toolbox_startup.host_menu_bar_hidden,
                            event_queue,
                            when,
                            input,
                        );
                        PPC_NO_ERR
                    } else {
                        PPC_PARAM_ERR
                    }
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowGreatestArea => {
            let window = cpu.gpr[3];
            let in_rect = cpu.gpr[4];
            let out_greatest = cpu.gpr[5];
            let can_write = ppc_memory_can_write_bytes(memory, out_greatest, 8);
            let result = match crate::window_manager::evaluate_get_window_greatest_area_parameters(
                window,
                in_rect,
                out_greatest,
                can_write,
            ) {
                Ok(params) => {
                    let (top, left, bottom, right) = crate::window_manager::evaluate_get_window_greatest_area();
                    let _ = ppc_write_rect(memory, params.out_rect_ptr(), top, left, bottom, right);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowAttributes => {
            let window = cpu.gpr[3];
            let out_attrs = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_attrs, 4);
            let result = match crate::window_manager::evaluate_get_window_attributes_parameters(
                window,
                out_attrs,
                can_write,
            ) {
                Ok(params) => {
                    let attrs = toolbox_startup.window_attributes(params.window_ptr());
                    let _ = memory.write_u32_be(params.out_attributes_ptr(), attrs);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::ChangeWindowAttributes => {
            let window = cpu.gpr[3];
            let set_attrs = cpu.gpr[4];
            let clear_attrs = cpu.gpr[5];
            let result = match crate::window_manager::evaluate_change_window_attributes_parameters(
                window,
                set_attrs,
                clear_attrs,
            ) {
                Ok(params) => {
                    let current = toolbox_startup.window_attributes(params.window_ptr());
                    let updated = crate::window_manager::evaluate_change_window_attributes(
                        current,
                        params.set_attributes(),
                        params.clear_attributes(),
                    );
                    toolbox_startup.set_window_attributes(params.window_ptr(), updated);
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::ReshapeCustomWindow => {
            let window = cpu.gpr[3];
            let result = match crate::window_manager::evaluate_reshape_custom_window_parameters(window) {
                Ok(params) => {
                    let _ = params.window_ptr();
                    PPC_NO_ERR
                }
                Err(err) => err,
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::GetWindowUserState => {
            let window = cpu.gpr[3];
            let out_rect = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(memory, out_rect, 8);
            let params = match crate::window_manager::evaluate_get_window_user_state_parameters(
                window,
                out_rect,
                can_write,
            ) {
                Ok(p) => p,
                Err(err) => return Some(PpcImportAction::Return(ppc_i16_result(err))),
            };
            let state = memory
                .read_u32_be(
                    params
                        .window_ptr()
                        .wrapping_add(PPC_CWINDOW_STATE_HANDLE_OFFSET),
                )
                .filter(|handle| *handle != 0)
                .and_then(|handle| memory.read_u32_be(handle))
                .filter(|state| *state != 0);
            let (top, left, bottom, right) = if let Some(s) = state {
                ppc_read_rect(memory, s).unwrap_or((40, 40, 240, 340))
            } else if let Some((t, l, b, r)) = ppc_read_rect(
                memory,
                params
                    .window_ptr()
                    .wrapping_add(PPC_CWINDOW_PORT_RECT_OFFSET),
            ) {
                (t, l, b, r)
            } else {
                (40, 40, 240, 340)
            };
            let _ = ppc_write_rect(memory, params.out_rect_ptr(), top, left, bottom, right);
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcLegacyWindowOperation::SetWindowUserState => {
            let window = cpu.gpr[3];
            let in_rect = cpu.gpr[4];
            let can_read = ppc_memory_can_read_bytes(memory, in_rect, 8);
            let params = match crate::window_manager::evaluate_set_window_user_state_parameters(
                window,
                in_rect,
                can_read,
            ) {
                Ok(p) => p,
                Err(err) => return Some(PpcImportAction::Return(ppc_i16_result(err))),
            };
            if let Some((top, left, bottom, right)) = ppc_read_rect(memory, params.in_rect_ptr()) {
                let state = memory
                    .read_u32_be(
                        params
                            .window_ptr()
                            .wrapping_add(PPC_CWINDOW_STATE_HANDLE_OFFSET),
                    )
                    .filter(|handle| *handle != 0)
                    .and_then(|handle| memory.read_u32_be(handle))
                    .filter(|state| *state != 0);
                if let Some(s) = state {
                    let _ = ppc_write_rect(memory, s, top, left, bottom, right);
                }
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            } else {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            }
        }
        PpcLegacyWindowOperation::GetWindowGoAwayFlag => {
            let window = cpu.gpr[3];
            let flag = if window != 0 {
                memory.read_u8(window.wrapping_add(PPC_CWINDOW_GO_AWAY_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_go_away_flag(window, flag);
            Some(PpcImportAction::Return(if result { 1 } else { 0 }))
        }
        PpcLegacyWindowOperation::GetWindowSpareFlag => {
            let window = cpu.gpr[3];
            let flag = if window != 0 {
                memory.read_u8(window.wrapping_add(PPC_CWINDOW_SPARE_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_spare_flag(window, flag);
            Some(PpcImportAction::Return(if result { 1 } else { 0 }))
        }
        PpcLegacyWindowOperation::GetWindowKind => {
            let window = cpu.gpr[3];
            let kind = if window != 0 {
                memory.read_u16_be(window.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET)).map(|k| k as i16)
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_kind(window, kind);
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcLegacyWindowOperation::SetWindowKind => {
            let window = cpu.gpr[3];
            let kind = cpu.gpr[4] as i16;
            let params = crate::window_manager::evaluate_set_window_kind_parameters(window, kind);
            if params.window_ptr() != 0 {
                let _ = memory.write_u16_be(
                    params.window_ptr().wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET),
                    params.kind() as u16,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::GetWindowDefProc => {
            let window = cpu.gpr[3];
            let def_proc = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_DEF_PROC_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_def_proc(window, def_proc);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::GetWindowStructureRgn => {
            let window = cpu.gpr[3];
            let rgn = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_STRUCTURE_RGN_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_structure_rgn(window, rgn);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::GetWindowContentRgn => {
            let window = cpu.gpr[3];
            let rgn = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_CONTENT_RGN_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_content_rgn(window, rgn);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::GetWindowUpdateRgn => {
            let window = cpu.gpr[3];
            let rgn = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_update_rgn(window, rgn);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::IsWindowUpdatePending => {
            let window = cpu.gpr[3];
            let handle = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_UPDATE_RGN_OFFSET))
            } else {
                None
            };
            let bounds = handle.and_then(|h| {
                if h == 0 {
                    None
                } else {
                    let ptr = memory.read_u32_be(h)?;
                    if ptr == 0 {
                        None
                    } else {
                        let top = memory.read_u16_be(ptr.wrapping_add(2))? as i16;
                        let left = memory.read_u16_be(ptr.wrapping_add(4))? as i16;
                        let bottom = memory.read_u16_be(ptr.wrapping_add(6))? as i16;
                        let right = memory.read_u16_be(ptr.wrapping_add(8))? as i16;
                        Some((top, left, bottom, right))
                    }
                }
            });
            let pending =
                crate::window_manager::evaluate_is_window_update_pending(window, handle, bounds);
            Some(PpcImportAction::Return(if pending { 1 } else { 0 }))
        }
        PpcLegacyWindowOperation::GetNextWindow => {
            let window = cpu.gpr[3];
            let next = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_NEXT_WINDOW_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_next_window(window, next);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::GetPreviousWindow => {
            let window = cpu.gpr[3];
            let prev = window_list.with_ref(|windows| {
                crate::window_manager::evaluate_get_previous_window(window, windows)
            });
            Some(PpcImportAction::Return(prev))
        }
        PpcLegacyWindowOperation::IsWindowActive => {
            let window = cpu.gpr[3];
            let is_hilited = if window != 0 {
                let flag = memory.read_u8(window.wrapping_add(PPC_CWINDOW_HILITED_OFFSET));
                crate::window_manager::evaluate_get_window_hilited(window, flag)
            } else {
                false
            };
            let is_front = window != 0
                && window_list.with_ref(|windows| windows.first().copied() == Some(window));
            let active =
                crate::window_manager::evaluate_is_window_active(window, is_hilited, is_front);
            Some(PpcImportAction::Return(if active { 1 } else { 0 }))
        }
        PpcLegacyWindowOperation::ActiveNonFloatingWindow => {
            let active = window_list.with_ref(|windows| {
                crate::window_manager::evaluate_active_non_floating_window(windows, |w| {
                    let visible =
                        memory.read_u8(w.wrapping_add(PPC_CWINDOW_VISIBLE_OFFSET)).unwrap_or(0) != 0;
                    let kind = memory
                        .read_u16_be(w.wrapping_add(PPC_CWINDOW_WINDOW_KIND_OFFSET))
                        .unwrap_or(0) as i16;
                    visible && kind >= 0
                })
            });
            Some(PpcImportAction::Return(active))
        }
        PpcLegacyWindowOperation::GetWindowModality => {
            let window = cpu.gpr[3];
            let out_modal_kind_ptr = cpu.gpr[4];
            let out_unavailable_ptr = cpu.gpr[5];
            let can_write_kind = out_modal_kind_ptr != 0
                && memory.write_u32_be(out_modal_kind_ptr, 0).is_some();
            let can_write_unavail = out_unavailable_ptr != 0
                && memory.write_u32_be(out_unavailable_ptr, 0).is_some();
            match crate::window_manager::evaluate_get_window_modality_parameters(
                window,
                out_modal_kind_ptr,
                out_unavailable_ptr,
                can_write_kind,
                can_write_unavail,
            ) {
                Ok(params) => {
                    let (kind, unavail) = toolbox_startup
                        .window_modality
                        .get(&params.window_ptr())
                        .copied()
                        .unwrap_or((crate::window_manager::WINDOW_MODALITY_NONE, 0));
                    if params.out_modal_kind_ptr() != 0 {
                        let _ = memory.write_u32_be(params.out_modal_kind_ptr(), kind);
                    }
                    if params.out_unavailable_window_ptr() != 0 {
                        let _ = memory.write_u32_be(params.out_unavailable_window_ptr(), unavail);
                    }
                    Some(PpcImportAction::Return(0))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::SetWindowModality => {
            let window = cpu.gpr[3];
            let modal_kind = cpu.gpr[4];
            let unavailable_ptr = cpu.gpr[5];
            match crate::window_manager::evaluate_set_window_modality_parameters(
                window,
                modal_kind,
                unavailable_ptr,
            ) {
                Ok(params) => {
                    toolbox_startup.window_modality.insert(
                        params.window_ptr(),
                        (params.modal_kind(), params.unavailable_window_ptr()),
                    );
                    Some(PpcImportAction::Return(0))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::ChangeWindowPropertyAttributes => {
            let window = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let to_set = cpu.gpr[6];
            let to_clear = cpu.gpr[7];
            match crate::window_manager::evaluate_change_window_property_attributes_parameters(
                window, creator, tag, to_set, to_clear,
            ) {
                Ok(params) => {
                    let result = toolbox_startup
                        .window_properties
                        .get_mut(&params.window_ptr())
                        .map_or(PPC_WINDOW_PROPERTY_NOT_FOUND_ERR, |props| {
                            match crate::window_manager::change_window_property_attributes(
                                props,
                                params.creator(),
                                params.tag(),
                                params.to_set(),
                                params.to_clear(),
                            ) {
                                Ok(_) => PPC_NO_ERR,
                                Err(e) => e,
                            }
                        });
                    Some(PpcImportAction::Return(ppc_i16_result(result)))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::GetWindowProperty => {
            let window = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let buffer_size = cpu.gpr[6];
            let actual_size_ptr = cpu.gpr[7];
            let data_ptr = cpu.gpr[8];
            let can_write_actual_size = actual_size_ptr != 0
                && memory.write_u32_be(actual_size_ptr, 0).is_some();
            let can_write_data = data_ptr != 0 && buffer_size > 0;
            match crate::window_manager::evaluate_get_window_property_parameters(
                window,
                creator,
                tag,
                buffer_size,
                actual_size_ptr,
                data_ptr,
                can_write_actual_size,
                can_write_data,
            ) {
                Ok(params) => {
                    let maybe_prop = toolbox_startup
                        .window_properties
                        .get(&params.window_ptr())
                        .and_then(|props| {
                            crate::window_manager::get_window_property(
                                props,
                                params.creator(),
                                params.tag(),
                            )
                        });
                    let result = if let Some(prop) = maybe_prop {
                        if params.out_actual_size_ptr() != 0 {
                            let _ = memory
                                .write_u32_be(params.out_actual_size_ptr(), prop.data.len() as u32);
                        }
                        if params.out_data_ptr() != 0 && params.buffer_size() > 0 {
                            let copy_len = (params.buffer_size() as usize).min(prop.data.len());
                            let _ =
                                memory.write_bytes(params.out_data_ptr(), &prop.data[..copy_len]);
                        }
                        PPC_NO_ERR
                    } else {
                        if params.out_actual_size_ptr() != 0 {
                            let _ = memory.write_u32_be(params.out_actual_size_ptr(), 0);
                        }
                        PPC_WINDOW_PROPERTY_NOT_FOUND_ERR
                    };
                    Some(PpcImportAction::Return(ppc_i16_result(result)))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::GetWindowPropertyAttributes => {
            let window = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let attributes_ptr = cpu.gpr[6];
            let can_write_attributes = attributes_ptr != 0
                && memory.write_u32_be(attributes_ptr, 0).is_some();
            match crate::window_manager::evaluate_get_window_property_attributes_parameters(
                window,
                creator,
                tag,
                attributes_ptr,
                can_write_attributes,
            ) {
                Ok(params) => {
                    let maybe_prop = toolbox_startup
                        .window_properties
                        .get(&params.window_ptr())
                        .and_then(|props| {
                            crate::window_manager::get_window_property(
                                props,
                                params.creator(),
                                params.tag(),
                            )
                        });
                    let result = if let Some(prop) = maybe_prop {
                        if params.out_attributes_ptr() != 0 {
                            let _ = memory
                                .write_u32_be(params.out_attributes_ptr(), prop.attributes);
                        }
                        PPC_NO_ERR
                    } else {
                        if params.out_attributes_ptr() != 0 {
                            let _ = memory.write_u32_be(params.out_attributes_ptr(), 0);
                        }
                        PPC_WINDOW_PROPERTY_NOT_FOUND_ERR
                    };
                    Some(PpcImportAction::Return(ppc_i16_result(result)))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::GetWindowPropertySize => {
            let window = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let actual_size_ptr = cpu.gpr[6];
            let can_write_actual_size = actual_size_ptr != 0
                && memory.write_u32_be(actual_size_ptr, 0).is_some();
            match crate::window_manager::evaluate_get_window_property_size_parameters(
                window,
                creator,
                tag,
                actual_size_ptr,
                can_write_actual_size,
            ) {
                Ok(params) => {
                    let maybe_prop = toolbox_startup
                        .window_properties
                        .get(&params.window_ptr())
                        .and_then(|props| {
                            crate::window_manager::get_window_property(
                                props,
                                params.creator(),
                                params.tag(),
                            )
                        });
                    let result = if let Some(prop) = maybe_prop {
                        if params.out_actual_size_ptr() != 0 {
                            let _ = memory
                                .write_u32_be(params.out_actual_size_ptr(), prop.data.len() as u32);
                        }
                        PPC_NO_ERR
                    } else {
                        if params.out_actual_size_ptr() != 0 {
                            let _ = memory.write_u32_be(params.out_actual_size_ptr(), 0);
                        }
                        PPC_WINDOW_PROPERTY_NOT_FOUND_ERR
                    };
                    Some(PpcImportAction::Return(ppc_i16_result(result)))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::RemoveWindowProperty => {
            let window = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            match crate::window_manager::evaluate_remove_window_property_parameters(
                window, creator, tag,
            ) {
                Ok(params) => {
                    let removed = toolbox_startup
                        .window_properties
                        .get_mut(&params.window_ptr())
                        .is_some_and(|props| {
                            crate::window_manager::remove_window_property(
                                props,
                                params.creator(),
                                params.tag(),
                            )
                        });
                    let result = if removed {
                        PPC_NO_ERR
                    } else {
                        PPC_WINDOW_PROPERTY_NOT_FOUND_ERR
                    };
                    Some(PpcImportAction::Return(ppc_i16_result(result)))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::SetWindowProperty => {
            let window = cpu.gpr[3];
            let creator = cpu.gpr[4];
            let tag = cpu.gpr[5];
            let size = cpu.gpr[6];
            let data_ptr = cpu.gpr[7];
            let data = if data_ptr != 0 && size > 0 {
                ppc_memory_read_bytes(memory, data_ptr, size).unwrap_or_default()
            } else {
                Vec::new()
            };
            match crate::window_manager::evaluate_set_window_property_parameters(
                window, creator, tag, data,
            ) {
                Ok(params) => {
                    let props = toolbox_startup
                        .window_properties
                        .entry(params.window_ptr())
                        .or_default();
                    crate::window_manager::set_window_property(
                        props,
                        params.creator(),
                        params.tag(),
                        0,
                        params.into_data(),
                    );
                    Some(PpcImportAction::Return(0))
                }
                Err(err) => Some(PpcImportAction::Return(ppc_i16_result(err))),
            }
        }
        PpcLegacyWindowOperation::GetAuxWin => {
            let window_ptr = cpu.gpr[3];
            let aw_ctable_ptr = cpu.gpr[4];
            let can_write = aw_ctable_ptr == 0
                || ppc_memory_can_write_bytes(memory, aw_ctable_ptr, 4);
            if let Ok(params) = crate::window_manager::evaluate_get_aux_win_parameters(
                window_ptr,
                aw_ctable_ptr,
                can_write,
            ) {
                let is_tracked = window_list.contains_window(params.window_ptr());
                let color_table = ppc_window_color_table_handle(memory, params.window_ptr());
                let (ctab, success) =
                    crate::window_manager::evaluate_get_aux_win(color_table, is_tracked);
                if params.out_aux_ctable_ptr() != 0 {
                    let _ = memory.write_u32_be(params.out_aux_ctable_ptr(), ctab);
                }
                Some(PpcImportAction::Return(if success { 1 } else { 0 }))
            } else {
                Some(PpcImportAction::Return(0))
            }
        }
        PpcLegacyWindowOperation::GetWindowPic => {
            let window = cpu.gpr[3];
            let pic_handle = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CWINDOW_WINDOW_PIC_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_window_pic(window, pic_handle);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::GetWindowPort => {
            let window = cpu.gpr[3];
            let port = crate::window_manager::evaluate_get_window_port(window);
            Some(PpcImportAction::Return(port))
        }
        PpcLegacyWindowOperation::GetWRefCon => {
            let window = cpu.gpr[3];
            let ref_con = if window != 0 {
                memory.read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET))
            } else {
                None
            };
            let result = crate::window_manager::evaluate_get_w_ref_con(window, ref_con);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::GetWVariant => {
            let window = cpu.gpr[3];
            let proc_id = if window != 0 {
                Some(ppc_window_proc_id(memory, window))
            } else {
                None
            };
            let variant = crate::window_manager::evaluate_get_w_variant(window, proc_id);
            Some(PpcImportAction::Return(variant as u16 as u32))
        }
        PpcLegacyWindowOperation::LMGetAuxWinHead => {
            let aux_head = memory.read_u32_be(PPC_LM_AUX_WIN_HEAD_ADDR);
            let result = crate::window_manager::evaluate_lm_get_aux_win_head(aux_head);
            Some(PpcImportAction::Return(result))
        }
        PpcLegacyWindowOperation::LMGetWindowList => {
            let first = window_list.first();
            let low_mem = memory.read_u32_be(PPC_LM_WINDOW_LIST_ADDR);
            let window = crate::window_manager::evaluate_lm_get_window_list(first, low_mem);
            Some(PpcImportAction::Return(window))
        }
        PpcLegacyWindowOperation::LMSetAuxWinHead => {
            let aux_head = cpu.gpr[3];
            let can_write = ppc_memory_can_write_bytes(memory, PPC_LM_AUX_WIN_HEAD_ADDR, 4);
            if let Ok(params) =
                crate::window_manager::evaluate_lm_set_aux_win_head_parameters(aux_head, can_write)
            {
                let _ = memory.write_u32_be(PPC_LM_AUX_WIN_HEAD_ADDR, params.aux_head());
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::LMSetWindowList => {
            let window = cpu.gpr[3];
            let can_write = ppc_memory_can_write_bytes(memory, PPC_LM_WINDOW_LIST_ADDR, 4);
            if let Ok(params) =
                crate::window_manager::evaluate_lm_set_window_list_parameters(window, can_write)
            {
                if params.window_ptr() != 0 {
                    window_list.bring_to_front(params.window_ptr());
                }
                ppc_sync_process_window_list(memory, window_list);
                let _ = memory.write_u32_be(PPC_LM_WINDOW_LIST_ADDR, params.window_ptr());
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::SetPortWindowPort => {
            let window = cpu.gpr[3];
            let port = crate::window_manager::evaluate_set_port_window_port(window);
            *current_gworld = port;
            *current_gdevice =
                ppc_gworld_device(gworlds, *current_gworld).unwrap_or(*current_gdevice);
            ppc_register_gdevice(toolbox_startup, *current_gdevice);
            ppc_restore_port_colors(
                memory,
                *current_gworld,
                quickdraw_fore_color,
                quickdraw_back_color,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::SetWindowPic => {
            let window = cpu.gpr[3];
            let pic = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(
                memory,
                window.wrapping_add(PPC_CWINDOW_WINDOW_PIC_OFFSET),
                4,
            );
            if let Ok(params) =
                crate::window_manager::evaluate_set_window_pic_parameters(window, pic, can_write)
            {
                let _ = memory.write_u32_be(
                    params
                        .window_ptr()
                        .wrapping_add(PPC_CWINDOW_WINDOW_PIC_OFFSET),
                    params.pic_handle(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcLegacyWindowOperation::SetWRefCon => {
            let window = cpu.gpr[3];
            let ref_con = cpu.gpr[4];
            let can_write = ppc_memory_can_write_bytes(
                memory,
                window.wrapping_add(PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET),
                4,
            );
            if let Ok(params) =
                crate::window_manager::evaluate_set_w_ref_con_parameters(window, ref_con, can_write)
            {
                let _ = memory.write_u32_be(
                    params
                        .window_ptr()
                        .wrapping_add(PPC_CGRAF_PORT_WINDOW_REF_CON_OFFSET),
                    params.ref_con(),
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_new_window_from_cpu(
    cpu: &PpcCpu,
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    current_gdevice: u32,
    host_menu_bar_hidden: bool,
) -> u32 {
    let previous_front = ppc_front_visible_window(memory, gworlds);
    let window = ppc_new_cwindow(
        cpu,
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        window_list,
        current_gdevice,
    );
    if window == 0 {
        return 0;
    }
    let _ = memory.write_u32_be(window + PPC_CWINDOW_CONTROL_LIST_OFFSET, 0);
    if !ppc_set_window_title(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        window,
        cpu.gpr[5],
    ) {
        return 0;
    }
    let state_handle = ppc_allocator_view_allocate_handle(
        allocator.as_deref_mut(),
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        16,
        true,
    );
    let Some(state) = memory.read_u32_be(state_handle).filter(|state| *state != 0) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    };
    let initial = ppc_read_rect(memory, cpu.gpr[4]).unwrap_or((0, 0, 1, 1));
    let wrote = ppc_write_rect(memory, state, initial.0, initial.1, initial.2, initial.3).is_some()
        && ppc_write_rect(
            memory,
            state + 8,
            20,
            0,
            ppc_main_screen_height() as i16,
            ppc_main_screen_width() as i16,
        )
        .is_some()
        && memory
            .write_u32_be(window + PPC_CWINDOW_STATE_HANDLE_OFFSET, state_handle)
            .is_some();
    if !wrote {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    ppc_transition_front_window_chrome(
        memory,
        gworlds,
        window_list,
        previous_front,
        host_menu_bar_hidden,
    );
    if cpu.gpr[6] != 0 && ppc_front_visible_process_window(memory, window_list) != Some(window) {
        ppc_draw_existing_window_frame(memory, gworlds, window_list, window, host_menu_bar_hidden);
    }
    *last_mem_error = PPC_NO_ERR;
    window
}

pub(super) fn ppc_set_window_title(
    mut allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    window: u32,
    title_ptr: u32,
) -> bool {
    if window == 0 {
        *last_mem_error = PPC_PARAM_ERR;
        return false;
    }
    let title = if title_ptr == 0 {
        Vec::new()
    } else {
        ppc_read_pstring_bytes(memory, title_ptr).unwrap_or_default()
    };
    let size = u32::try_from(title.len().min(255) + 1).unwrap_or(256);
    let existing = memory
        .read_u32_be(window + PPC_CWINDOW_TITLE_HANDLE_OFFSET)
        .unwrap_or(0);
    let handle = if existing != 0 {
        if ppc_allocator_view_resize_handle(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            existing,
            size,
        ) != PPC_NO_ERR
        {
            *last_mem_error = PPC_MEM_FULL_ERR;
            return false;
        }
        existing
    } else {
        ppc_allocator_view_allocate_handle(
            allocator.as_deref_mut(),
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            size,
            true,
        )
    };
    let Some(data) = memory.read_u32_be(handle).filter(|data| *data != 0) else {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return false;
    };
    let wrote = ppc_write_pstring_bytes(memory, data, &title)
        && memory
            .write_u32_be(window + PPC_CWINDOW_TITLE_HANDLE_OFFSET, handle)
            .is_some()
        && memory
            .write_u16_be(
                window + PPC_CWINDOW_TITLE_WIDTH_OFFSET,
                ppc_text_bytes_advance_for_font(
                    &title,
                    PPC_QD_TEXT_FONT_DEFAULT,
                    PPC_QD_TEXT_SIZE_SYSTEM,
                ) as u16,
            )
            .is_some();
    *last_mem_error = if wrote { PPC_NO_ERR } else { PPC_PARAM_ERR };
    wrote
}

pub(super) fn ppc_materialize_window_resource_parameters(
    allocator: Option<&mut PpcProcessAllocatorView<'_>>,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    bytes: &[u8],
) -> Option<(u32, u32)> {
    if bytes.len() < 19 {
        return None;
    }
    let title_len = usize::from(bytes[18]);
    if bytes.len() < 19usize.checked_add(title_len)? {
        return None;
    }
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
    memory.write_bytes(scratch + 9, &bytes[19..19 + title_len])?;
    Some((scratch, scratch + 8))
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_close_window(
    window: u32,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    window_list: &SharedProcessWindowList,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    screen_clut: &mut [[u16; 3]; 256],
    color_manager_clut: &mut [[u16; 3]; 256],
    toolbox_startup: &mut PpcToolboxStartupState,
    event_queue: &mut EventQueue,
    tick_count: u32,
    input: PpcInputSnapshot,
    quickdraw_fore_color: &mut PpcRgbColor,
    quickdraw_back_color: &mut PpcRgbColor,
    quickdraw_fore_indices: &mut HashMap<u32, u8>,
) {
    let Some(params) = crate::window_manager::evaluate_close_window_parameters(window) else {
        return;
    };
    let window = params.window_ptr();
    let previous_front = ppc_front_visible_process_window(memory, window_list);
    let was_visible = ppc_window_is_visible(memory, window);
    let exposed = was_visible
        .then(|| {
            memory
                .read_u32_be(window.wrapping_add(PPC_CWINDOW_STRUCTURE_RGN_OFFSET))
                .and_then(|region| ppc_read_rgn_bbox(memory, region))
        })
        .flatten();
    if window != 0 {
        let closed_palette = memory
            .read_u32_be(window.wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET))
            .unwrap_or(0);
        let closed_pixmap_handle = gworlds
            .iter()
            .find(|gworld| {
                gworld.port == window
                    && !matches!(gworld.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
            })
            .map(|gworld| gworld.pixmap_handle);
        let was_current = *current_gworld == window;
        gworlds.retain(|gworld| {
            gworld.port == PPC_MAIN_GWORLD
                || gworld.port == PPC_DSP_BACK_GWORLD
                || gworld.port != window
        });
        window_list.with_mut(|windows| windows.retain(|candidate| *candidate != window));
        ppc_recalculate_window_vis_regions(
            process_memory_manager,
            memory,
            window_list,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
        );
        ppc_transition_front_window_chrome(
            memory,
            gworlds,
            window_list,
            previous_front,
            toolbox_startup.host_menu_bar_hidden,
        );
        if *current_gworld == window {
            *current_gworld =
                ppc_front_visible_process_window(memory, window_list).unwrap_or(PPC_MAIN_GWORLD);
            *current_gdevice =
                ppc_gworld_device(gworlds, *current_gworld).unwrap_or(PPC_MAIN_GDEVICE);
            if *current_gworld != PPC_MAIN_GWORLD {
                ppc_enqueue_window_update_event(event_queue, *current_gworld, tick_count, input);
            }
        }
        ppc_restore_window_removal_exposure(
            memory,
            gworlds,
            window_list,
            exposed,
            toolbox_startup.host_menu_bar_hidden,
            event_queue,
            tick_count,
            input,
        );
        if let Some(pixmap_handle) = closed_pixmap_handle {
            toolbox_startup
                .indexed_screen_ctables
                .remove(&pixmap_handle);
            quickdraw_fore_indices.remove(&window);
            let still_associated = toolbox_startup.application_palette == closed_palette
                || gworlds.iter().any(|record| {
                    memory
                        .read_u32_be(
                            record
                                .port
                                .wrapping_add(PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET),
                        )
                        .unwrap_or(0)
                        == closed_palette
                });
            if closed_palette != 0 && !still_associated {
                ppc_release_palette_allocations_and_restore(
                    memory,
                    toolbox_startup,
                    closed_palette,
                    *current_gdevice,
                    screen_clut,
                    color_manager_clut,
                );
            }
        }
        if was_current && *current_gworld != window {
            ppc_restore_port_colors(
                memory,
                *current_gworld,
                quickdraw_fore_color,
                quickdraw_back_color,
            );
        }
        if ppc_front_visible_process_window(memory, window_list) != previous_front {
            let _ = ppc_activate_front_window_palette(
                memory,
                gworlds,
                *current_gdevice,
                screen_clut,
                color_manager_clut,
                toolbox_startup,
            );
        }
    }
    if ppc_gworld_trace_enabled() {
        eprintln!(
            "[PPC-GWORLD-TRACE] CloseWindow window=${:08X} current=${:08X} remaining_gworlds={}",
            window,
            *current_gworld,
            gworlds.len()
        );
    }
}

pub(super) fn ppc_dispose_window(
    allocator: &mut PpcProcessAllocatorView<'_>,
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
    window: u32,
) {
    for control in ppc_window_control_handles(memory, window) {
        ppc_dispose_control(
            Some(allocator),
            None,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            control,
        );
    }
    for offset in [
        PPC_CGRAF_PORT_VIS_RGN_OFFSET,
        PPC_CGRAF_PORT_CLIP_RGN_OFFSET,
        PPC_CWINDOW_STRUCTURE_RGN_OFFSET,
        PPC_CWINDOW_CONTENT_RGN_OFFSET,
        PPC_CWINDOW_UPDATE_RGN_OFFSET,
        PPC_CWINDOW_DEF_PROC_OFFSET,
        PPC_CWINDOW_TITLE_HANDLE_OFFSET,
        PPC_CWINDOW_STATE_HANDLE_OFFSET,
    ] {
        let handle = memory.read_u32_be(window.wrapping_add(offset)).unwrap_or(0);
        let _ = allocator.dispose_handle(
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            handle,
        );
        let _ = memory.write_u32_be(window.wrapping_add(offset), 0);
    }
    gworlds.retain(|record| {
        record.port != window || matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
    });
    window_list.with_mut(|windows| windows.retain(|candidate| *candidate != window));
    if *current_gworld == window {
        *current_gworld =
            ppc_front_visible_process_window(memory, window_list).unwrap_or(PPC_MAIN_GWORLD);
        *current_gdevice = ppc_gworld_device(gworlds, *current_gworld).unwrap_or(PPC_MAIN_GDEVICE);
    }
}

pub(super) fn ppc_reorder_window(
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    window: u32,
    behind: u32,
    front: bool,
) {
    window_list.reorder(window, behind, front);

    let Some(index) = gworlds.iter().position(|record| {
        record.port == window && !matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
    }) else {
        return;
    };
    let record = gworlds.remove(index);
    if front {
        gworlds.push(record);
        return;
    }
    let insert = if behind == 0 {
        gworlds
            .iter()
            .rposition(|record| matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD))
            .map_or(0, |index| index + 1)
    } else {
        gworlds
            .iter()
            .position(|record| record.port == behind)
            .unwrap_or(gworlds.len())
    };
    gworlds.insert(insert, record);
}

pub(super) fn ppc_window_part_contains_point(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
    part: i16,
    v: i16,
    h: i16,
) -> bool {
    let Some((top, left, _, right)) = ppc_dialog_global_bounds(memory, gworlds, window) else {
        return false;
    };
    match part {
        6 => v >= top.saturating_sub(18) && v < top && h >= left && h < left.saturating_add(18),
        7 | 8 => {
            v >= top.saturating_sub(18) && v < top && h >= right.saturating_sub(18) && h < right
        }
        _ => false,
    }
}

pub(super) fn ppc_drag_window_call(cpu: &PpcCpu) -> PpcDragWindowCall {
    PpcDragWindowCall {
        window: cpu.gpr[3],
        start_point: cpu.gpr[4],
        bounds_ptr: cpu.gpr[5],
        stack_pointer: cpu.gpr[1],
        return_address: cpu.lr,
    }
}

pub(super) fn ppc_point_in_rect(point: (i16, i16), rect: (i16, i16, i16, i16)) -> bool {
    point.0 >= rect.0 && point.0 < rect.2 && point.1 >= rect.1 && point.1 < rect.3
}

pub(super) fn ppc_rect_intersection(
    a: (i16, i16, i16, i16),
    b: (i16, i16, i16, i16),
) -> Option<(i16, i16, i16, i16)> {
    let rect = (a.0.max(b.0), a.1.max(b.1), a.2.min(b.2), a.3.min(b.3));
    (rect.0 < rect.2 && rect.1 < rect.3).then_some(rect)
}

pub(super) fn ppc_rect_difference_bbox(
    src: (i16, i16, i16, i16),
    cut: (i16, i16, i16, i16),
) -> Option<(i16, i16, i16, i16)> {
    let Some(intersection) = ppc_rect_intersection(src, cut) else {
        return Some(src);
    };

    let mut remaining: Option<(i16, i16, i16, i16)> = None;
    for rect in [
        (src.0, src.1, intersection.0, src.3),
        (intersection.2, src.1, src.2, src.3),
        (intersection.0, src.1, intersection.2, intersection.1),
        (intersection.0, intersection.3, intersection.2, src.3),
    ] {
        if rect.0 < rect.2 && rect.1 < rect.3 {
            remaining = match remaining {
                Some(r) => Some((
                    r.0.min(rect.0),
                    r.1.min(rect.1),
                    r.2.max(rect.2),
                    r.3.max(rect.3),
                )),
                None => Some(rect),
            };
        }
    }
    remaining
}

pub(super) fn ppc_offset_rect_bounds(
    rect: (i16, i16, i16, i16),
    delta_v: i16,
    delta_h: i16,
) -> (i16, i16, i16, i16) {
    (
        rect.0.saturating_add(delta_v),
        rect.1.saturating_add(delta_h),
        rect.2.saturating_add(delta_v),
        rect.3.saturating_add(delta_h),
    )
}

pub(super) fn ppc_drag_outline_points(
    front: PpcFrontBuffer,
    rect: (i16, i16, i16, i16),
) -> Vec<(i32, i32)> {
    let (top, left, bottom, right) = (
        i32::from(rect.0),
        i32::from(rect.1),
        i32::from(rect.2),
        i32::from(rect.3),
    );
    if bottom <= top || right <= left {
        return Vec::new();
    }
    let width = i32::try_from(front.width).unwrap_or(i32::MAX);
    let height = i32::try_from(front.height).unwrap_or(i32::MAX);
    let mut points = Vec::new();
    for x in left.max(0)..right.min(width) {
        for y in [top, bottom - 1] {
            if y >= 0 && y < height {
                points.push((x, y));
            }
        }
    }
    for y in (top + 1).max(0)..(bottom - 1).min(height) {
        for x in [left, right - 1] {
            if x >= 0 && x < width {
                points.push((x, y));
            }
        }
    }
    points
}

pub(super) fn ppc_restore_drag_window_outline(
    memory: &mut PpcSectionMem,
    state: &PpcDragWindowTrackingState,
) {
    for (index, (x, y, pixel)) in state.saved_pixels.iter().copied().enumerate() {
        let _ = ppc_quickdraw_write_raw_pixel(memory, state.front_buffer, (x, y), pixel);
        ppc_restore_saved_detail(
            memory,
            state.front_buffer,
            (x, y),
            &state.saved_pixels,
            index,
        );
    }
}

pub(super) fn ppc_refresh_drag_window_outline(
    memory: &mut PpcSectionMem,
    screen_clut: &[[u16; 3]; 256],
    state: &mut PpcDragWindowTrackingState,
    mouse: (i16, i16),
) {
    let start = (
        (state.call.start_point >> 16) as u16 as i16,
        state.call.start_point as u16 as i16,
    );
    let outline = ppc_offset_rect_bounds(
        state.original_structure,
        mouse.0.wrapping_sub(start.0),
        mouse.1.wrapping_sub(start.1),
    );
    if !state.saved_pixels.is_empty() && state.outline == outline {
        return;
    }
    ppc_restore_drag_window_outline(memory, state);
    state.outline = outline;
    state.saved_pixels = ppc_drag_outline_points(state.front_buffer, state.outline)
        .into_iter()
        .filter_map(|(x, y)| {
            ppc_quickdraw_read_pixel(memory, state.front_buffer, (x, y)).map(|pixel| (x, y, pixel))
        })
        .collect::<Vec<_>>()
        .into();
    for index in 0..state.saved_pixels.len() {
        let (x, y, _) = state.saved_pixels[index];
        ppc_capture_saved_detail(
            memory,
            state.front_buffer,
            (x, y),
            &mut state.saved_pixels,
            index,
        );
    }
    let Some(black) =
        ppc_physical_screen_color_pixel(state.front_buffer, PPC_RGB_BLACK, screen_clut)
    else {
        return;
    };
    let Some(white) =
        ppc_physical_screen_color_pixel(state.front_buffer, PPC_RGB_WHITE, screen_clut)
    else {
        return;
    };
    for (x, y, _) in state.saved_pixels.iter().copied() {
        let pixel = if (x + y).rem_euclid(2) == 0 {
            black
        } else {
            white
        };
        let _ = ppc_quickdraw_write_raw_pixel(memory, state.front_buffer, (x, y), pixel);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn ppc_dispatch_drag_window(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    current_gworld: &mut u32,
    startup: &mut PpcToolboxStartupState,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    screen_clut: &[[u16; 3]; 256],
    when: u32,
    input: PpcInputSnapshot,
) -> PpcImportAction {
    // DragWindow owns a synchronous Window Manager loop and moves only a
    // gray structure-region outline until mouse-up. Macintosh Toolbox
    // Essentials (1992), pp. 4-94--4-95.
    let Some(params) = crate::window_manager::evaluate_drag_window_parameters(
        cpu.gpr[3],
        cpu.gpr[4],
        cpu.gpr[5],
    ) else {
        return PpcImportAction::ReturnPreserve;
    };
    let call = ppc_drag_window_call(cpu);
    if let Some(state) = startup.drag_window_tracking.as_ref() {
        if state.call != call {
            return PpcImportAction::ReturnPreserve;
        }
        let live_front = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD);
        let valid_window = gworlds.iter().any(|record| record.port == call.window)
            && ppc_window_is_visible(memory, call.window)
            && ppc_dialog_global_bounds(memory, gworlds, call.window)
                == Some(state.original_content);
        if live_front != Some(state.front_buffer) {
            startup.drag_window_tracking = None;
            return PpcImportAction::ReturnPreserve;
        }
        if !valid_window {
            let state = startup.drag_window_tracking.take().unwrap();
            ppc_restore_drag_window_outline(memory, &state);
            return PpcImportAction::ReturnPreserve;
        }

        if input.mouse_button {
            let mut state = startup.drag_window_tracking.take().unwrap();
            ppc_refresh_drag_window_outline(
                memory,
                screen_clut,
                &mut state,
                (input.mouse_v, input.mouse_h),
            );
            startup.drag_window_tracking = Some(state);
            return PpcImportAction::Yield(u64::MAX);
        }

        let state = startup.drag_window_tracking.take().unwrap();
        ppc_restore_drag_window_outline(memory, &state);
        if let Some(index) = event_queue.iter().position(|event| event.what == 2) {
            event_queue.remove(index);
        }
        let release = (input.mouse_v, input.mouse_h);
        let start = params.start_point();
        if release != start && ppc_point_in_rect(release, state.bounds) {
            let mut move_cpu = cpu.clone();
            move_cpu.gpr[4] = state
                .original_content
                .1
                .saturating_add(release.1.wrapping_sub(start.1))
                as u16 as u32;
            move_cpu.gpr[5] = state
                .original_content
                .0
                .saturating_add(release.0.wrapping_sub(start.0))
                as u16 as u32;
            move_cpu.gpr[6] = 1;
            if ppc_move_window(&move_cpu, memory, gworlds).is_some() {
                let previous_front = ppc_front_visible_process_window(memory, window_list);
                ppc_reorder_window(gworlds, window_list, call.window, 0, true);
                ppc_recalculate_window_vis_regions(
                    process_memory_manager,
                    memory,
                    window_list,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                );
                let next_structure =
                    ppc_window_global_structure_bounds(memory, gworlds, call.window);
                ppc_repaint_window_geometry_transition(
                    memory,
                    gworlds,
                    window_list,
                    call.window,
                    true,
                    Some(state.original_structure),
                    next_structure,
                    startup.host_menu_bar_hidden,
                    event_queue,
                    when,
                    input,
                );
                ppc_transition_front_window_chrome(
                    memory,
                    gworlds,
                    window_list,
                    previous_front,
                    startup.host_menu_bar_hidden,
                );
                *current_gworld = call.window;
            }
        }
        return PpcImportAction::ReturnPreserve;
    }

    if startup.execution.menu().is_some()
        || startup.go_away_tracking.is_some()
        || !input.mouse_button
    {
        return PpcImportAction::ReturnPreserve;
    }
    let (Some(bounds), Some(original_content), Some(front_buffer)) = (
        ppc_read_rect(memory, params.bounds_rect_ptr()),
        ppc_dialog_global_bounds(memory, gworlds, params.window_ptr()),
        ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD),
    ) else {
        return PpcImportAction::ReturnPreserve;
    };
    if !ppc_window_is_visible(memory, call.window) {
        return PpcImportAction::ReturnPreserve;
    }
    let original_structure =
        ppc_window_structure_bounds(ppc_window_proc_id(memory, call.window), original_content);
    let mut state = PpcDragWindowTrackingState {
        call,
        front_buffer,
        original_content,
        original_structure,
        bounds,
        outline: original_structure,
        saved_pixels: Vec::new().into(),
    };
    ppc_refresh_drag_window_outline(
        memory,
        screen_clut,
        &mut state,
        (input.mouse_v, input.mouse_h),
    );
    startup.drag_window_tracking = Some(state);
    PpcImportAction::Yield(u64::MAX)
}

pub(super) fn ppc_grow_window_call(cpu: &PpcCpu) -> PpcGrowWindowCall {
    PpcGrowWindowCall {
        window: cpu.gpr[3],
        start_point: cpu.gpr[4],
        size_rect_ptr: cpu.gpr[5],
        stack_pointer: cpu.gpr[1],
        return_address: cpu.lr,
    }
}

pub(super) fn ppc_restore_grow_window_outline(
    memory: &mut PpcSectionMem,
    state: &PpcGrowWindowTrackingState,
) {
    for (index, (x, y, pixel)) in state.saved_pixels.iter().copied().enumerate() {
        let _ = ppc_quickdraw_write_raw_pixel(memory, state.front_buffer, (x, y), pixel);
        ppc_restore_saved_detail(
            memory,
            state.front_buffer,
            (x, y),
            &state.saved_pixels,
            index,
        );
    }
}

pub(super) fn ppc_refresh_grow_window_outline(
    memory: &mut PpcSectionMem,
    screen_clut: &[[u16; 3]; 256],
    state: &mut PpcGrowWindowTrackingState,
    mouse: (i16, i16),
) {
    let (height, width) = crate::window_manager::grow_dimensions_from_drag(
        state.original_content,
        state.size_limits,
        (
            (state.call.start_point >> 16) as i16,
            state.call.start_point as i16,
        ),
        mouse,
    );
    let proposed_content = (
        state.original_content.0,
        state.original_content.1,
        state.original_content.0.saturating_add(height),
        state.original_content.1.saturating_add(width),
    );
    let outline = ppc_window_structure_bounds(
        ppc_window_proc_id(memory, state.call.window),
        proposed_content,
    );
    if !state.saved_pixels.is_empty() && state.outline == outline {
        return;
    }
    ppc_restore_grow_window_outline(memory, state);
    state.outline = outline;
    state.saved_pixels = ppc_drag_outline_points(state.front_buffer, state.outline)
        .into_iter()
        .filter_map(|(x, y)| {
            ppc_quickdraw_read_pixel(memory, state.front_buffer, (x, y)).map(|pixel| (x, y, pixel))
        })
        .collect::<Vec<_>>()
        .into();
    for index in 0..state.saved_pixels.len() {
        let (x, y, _) = state.saved_pixels[index];
        ppc_capture_saved_detail(
            memory,
            state.front_buffer,
            (x, y),
            &mut state.saved_pixels,
            index,
        );
    }
    let Some(black) =
        ppc_physical_screen_color_pixel(state.front_buffer, PPC_RGB_BLACK, screen_clut)
    else {
        return;
    };
    let Some(white) =
        ppc_physical_screen_color_pixel(state.front_buffer, PPC_RGB_WHITE, screen_clut)
    else {
        return;
    };
    for (x, y, _) in state.saved_pixels.iter().copied() {
        let pixel = if (x + y).rem_euclid(2) == 0 {
            black
        } else {
            white
        };
        let _ = ppc_quickdraw_write_raw_pixel(memory, state.front_buffer, (x, y), pixel);
    }
}

pub(super) fn ppc_dispatch_grow_window(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    startup: &mut PpcToolboxStartupState,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    screen_clut: &[[u16; 3]; 256],
    input: PpcInputSnapshot,
) -> PpcImportAction {
    // GrowWindow owns the mouse through release and returns a proposed size;
    // the caller applies that size separately with SizeWindow. Inside
    // Macintosh Volume I (1985), pp. I-297--I-299.
    let Some(params) = crate::window_manager::evaluate_grow_window_parameters(
        cpu.gpr[3],
        cpu.gpr[4],
        cpu.gpr[5],
    ) else {
        return PpcImportAction::Return(0);
    };
    let call = ppc_grow_window_call(cpu);
    if let Some(state) = startup.grow_window_tracking.as_ref() {
        if state.call != call {
            return PpcImportAction::Return(0);
        }
        let live_front = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD);
        let valid_window = gworlds.iter().any(|record| record.port == call.window)
            && ppc_window_is_visible(memory, call.window)
            && ppc_dialog_global_bounds(memory, gworlds, call.window)
                == Some(state.original_content);
        if live_front != Some(state.front_buffer) {
            startup.grow_window_tracking = None;
            return PpcImportAction::Return(0);
        }
        if !valid_window {
            let state = startup.grow_window_tracking.take().unwrap();
            ppc_restore_grow_window_outline(memory, &state);
            return PpcImportAction::Return(0);
        }
        if input.mouse_button {
            let mut state = startup.grow_window_tracking.take().unwrap();
            ppc_refresh_grow_window_outline(
                memory,
                screen_clut,
                &mut state,
                (input.mouse_v, input.mouse_h),
            );
            startup.grow_window_tracking = Some(state);
            return PpcImportAction::Yield(u64::MAX);
        }

        let state = startup.grow_window_tracking.take().unwrap();
        ppc_restore_grow_window_outline(memory, &state);
        if let Some(index) = event_queue.iter().position(|event| event.what == 2) {
            event_queue.remove(index);
        }
        let (height, width) = crate::window_manager::grow_dimensions_from_drag(
            state.original_content,
            state.size_limits,
            params.start_point(),
            (input.mouse_v, input.mouse_h),
        );
        let old_height = state
            .original_content
            .2
            .saturating_sub(state.original_content.0);
        let old_width = state
            .original_content
            .3
            .saturating_sub(state.original_content.1);
        let result = if height == old_height && width == old_width {
            0
        } else {
            (u32::from(height as u16) << 16) | u32::from(width as u16)
        };
        return PpcImportAction::Return(result);
    }

    if startup.execution.menu().is_some()
        || startup.go_away_tracking.is_some()
        || startup.drag_window_tracking.is_some()
        || !input.mouse_button
    {
        return PpcImportAction::Return(0);
    }
    let (Some(size_limits), Some(original_content), Some(front_buffer)) = (
        ppc_read_rect(memory, params.bounds_rect_ptr()),
        ppc_dialog_global_bounds(memory, gworlds, params.window_ptr()),
        ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD),
    ) else {
        return PpcImportAction::Return(0);
    };
    if !ppc_window_is_visible(memory, params.window_ptr()) {
        return PpcImportAction::Return(0);
    }
    let mut state = PpcGrowWindowTrackingState {
        call,
        front_buffer,
        original_content,
        size_limits,
        outline: ppc_window_structure_bounds(
            ppc_window_proc_id(memory, params.window_ptr()),
            original_content,
        ),
        saved_pixels: Vec::new().into(),
    };
    ppc_refresh_grow_window_outline(
        memory,
        screen_clut,
        &mut state,
        (input.mouse_v, input.mouse_h),
    );
    startup.grow_window_tracking = Some(state);
    PpcImportAction::Yield(u64::MAX)
}

pub(super) fn ppc_go_away_call(cpu: &PpcCpu) -> PpcGoAwayCall {
    PpcGoAwayCall {
        window: cpu.gpr[3],
        start_point: cpu.gpr[4],
        stack_pointer: cpu.gpr[1],
        return_address: cpu.lr,
    }
}

pub(super) fn ppc_go_away_window_is_trackable(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window: u32,
) -> bool {
    let proc_id = ppc_window_proc_id(memory, window);
    gworlds.iter().any(|record| record.port == window)
        && ppc_front_visible_window(memory, gworlds) == Some(window)
        && ppc_window_is_visible(memory, window)
        && memory
            .read_u8(window.wrapping_add(PPC_CWINDOW_HILITED_OFFSET))
            .unwrap_or(0)
            != 0
        && memory
            .read_u8(window.wrapping_add(PPC_CWINDOW_GO_AWAY_OFFSET))
            .unwrap_or(0)
            != 0
        && ppc_window_proc_has_title_bar(proc_id)
        && proc_id != 5
}

pub(super) fn ppc_go_away_highlight_pixels(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
) -> Option<crate::memory::SavedPixels<u16>> {
    let mut pixels = Vec::with_capacity(121);
    for v in -15..-4 {
        for h in 8..19 {
            pixels.push(ppc_quickdraw_read_pixel(
                memory,
                surface.front_buffer,
                surface.local_point((h, v)),
            )?);
        }
    }
    let mut pixels = crate::memory::SavedPixels::from(pixels);
    for (index, (h, v)) in (-15..-4)
        .flat_map(|v| (8..19).map(move |h| (h, v)))
        .enumerate()
    {
        ppc_capture_saved_detail(
            memory,
            surface.front_buffer,
            surface.local_point((h, v)),
            &mut pixels,
            index,
        );
    }
    Some(pixels)
}

pub(super) fn ppc_draw_go_away_tracking_feedback(
    memory: &mut PpcSectionMem,
    state: &PpcGoAwayTrackingState,
    highlighted: bool,
) {
    let mask = match state.surface.front_buffer.depth {
        1 => 0x0001,
        2 => 0x0003,
        4 => 0x000f,
        8 => 0x00ff,
        16 => 0x7fff,
        _ => return,
    };
    let mut saved = state.saved_pixels.clone();
    if highlighted {
        let lanes = (state.surface.front_buffer.depth / 8).max(1) as usize;
        saved.transform_detail(|offset, byte| {
            byte ^ (mask >> ((lanes - 1 - offset % lanes) * 8)) as u8
        });
    }
    for (index, ((h, v), pixel)) in (-15..-4)
        .flat_map(|v| (8..19).map(move |h| (h, v)))
        .zip(state.saved_pixels.iter().copied())
        .enumerate()
    {
        let value = if highlighted { pixel ^ mask } else { pixel };
        let _ = ppc_quickdraw_write_raw_pixel(
            memory,
            state.surface.front_buffer,
            state.surface.local_point((h, v)),
            value,
        );
        ppc_restore_saved_detail(
            memory,
            state.surface.front_buffer,
            state.surface.local_point((h, v)),
            &saved,
            index,
        );
    }
}

pub(super) fn ppc_dispatch_track_go_away(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    startup: &mut PpcToolboxStartupState,
    event_queue: &mut VecDeque<PpcQueuedEvent>,
    input: PpcInputSnapshot,
) -> PpcImportAction {
    // TrackGoAway owns a synchronous Window Manager tracking loop. Retain the
    // import frame until mouse-up and toggle the WDEF close-box feedback as
    // the pointer crosses its hit region. Macintosh Toolbox Essentials
    // (1992), pp. 4-103--4-104.
    let Some(params) = crate::window_manager::evaluate_track_go_away_parameters(
        cpu.gpr[3],
        cpu.gpr[4],
    ) else {
        return PpcImportAction::Return(0);
    };
    let call = ppc_go_away_call(cpu);
    if let Some(state) = startup.go_away_tracking.as_ref() {
        if state.call != call {
            return PpcImportAction::Return(0);
        }
        let live_surface = ppc_live_quickdraw_surface(memory, gworlds, call.window);
        if live_surface != Some(state.surface) {
            startup.go_away_tracking = None;
            return PpcImportAction::Return(0);
        }
        if !ppc_go_away_window_is_trackable(memory, gworlds, call.window) {
            let state = startup.go_away_tracking.take().unwrap();
            ppc_draw_go_away_tracking_feedback(memory, &state, false);
            return PpcImportAction::Return(0);
        }

        let inside = ppc_window_part_contains_point(
            memory,
            gworlds,
            call.window,
            6,
            input.mouse_v,
            input.mouse_h,
        );
        if input.mouse_button {
            let mut state = startup.go_away_tracking.take().unwrap();
            if state.highlighted != inside {
                ppc_draw_go_away_tracking_feedback(memory, &state, inside);
                state.highlighted = inside;
            }
            startup.go_away_tracking = Some(state);
            return PpcImportAction::Yield(u64::MAX);
        }

        let state = startup.go_away_tracking.take().unwrap();
        ppc_draw_go_away_tracking_feedback(memory, &state, false);
        if let Some(index) = event_queue.iter().position(|event| event.what == 2) {
            event_queue.remove(index);
        }
        return PpcImportAction::Return(u32::from(inside));
    }

    if startup.execution.menu().is_some()
        || !input.mouse_button
        || !ppc_go_away_window_is_trackable(memory, gworlds, params.window_ptr())
    {
        return PpcImportAction::Return(0);
    }
    let (start_v, start_h) = params.start_point();
    if !ppc_window_part_contains_point(memory, gworlds, params.window_ptr(), 6, start_v, start_h) {
        return PpcImportAction::Return(0);
    }
    let Some(surface) = ppc_live_quickdraw_surface(memory, gworlds, params.window_ptr()) else {
        return PpcImportAction::Return(0);
    };
    let Some(saved_pixels) = ppc_go_away_highlight_pixels(memory, surface) else {
        return PpcImportAction::Return(0);
    };
    let highlighted = ppc_window_part_contains_point(
        memory,
        gworlds,
        params.window_ptr(),
        6,
        input.mouse_v,
        input.mouse_h,
    );
    let state = PpcGoAwayTrackingState {
        call,
        surface,
        saved_pixels,
        highlighted,
    };
    ppc_draw_go_away_tracking_feedback(memory, &state, highlighted);
    startup.go_away_tracking = Some(state);
    PpcImportAction::Yield(u64::MAX)
}

pub(super) fn ppc_zoom_window(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
) -> Option<()> {
    let window = cpu.gpr[3];
    let part = cpu.gpr[4] as u16 as i16;
    let state = memory
        .read_u32_be(window.wrapping_add(PPC_CWINDOW_STATE_HANDLE_OFFSET))
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|state| *state != 0);
    let state = state?;
    let offset = if part == 8 { 8 } else { 0 };
    let (top, left, bottom, right) = ppc_read_rect(memory, state + offset)?;
    let mut move_cpu = cpu.clone();
    move_cpu.gpr[4] = left as u16 as u32;
    move_cpu.gpr[5] = top as u16 as u32;
    ppc_move_window(&move_cpu, memory, gworlds)?;
    let mut size_cpu = cpu.clone();
    size_cpu.gpr[4] = right.saturating_sub(left) as u16 as u32;
    size_cpu.gpr[5] = bottom.saturating_sub(top) as u16 as u32;
    ppc_size_window(&size_cpu, memory, gworlds)?;
    Some(())
}
