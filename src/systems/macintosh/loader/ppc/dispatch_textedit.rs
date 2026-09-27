//! Typed TextEdit dispatch for PowerPC imports.

use super::*;
use crate::event_queue::EventQueue;
use std::collections::HashMap;

pub(super) struct PpcTextEditDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(super) current_gworld: u32,
    pub(super) tick_count: u32,
    pub(super) quickdraw_text_mode: i16,
    pub(super) quickdraw_text_size: i16,
    pub(super) quickdraw_fore_color: &'a PpcRgbColor,
    pub(super) quickdraw_back_color: &'a PpcRgbColor,
    pub(super) quickdraw_fore_indices: &'a mut HashMap<u32, u8>,
    pub(super) scrap: &'a mut PpcScrapState,
    pub(super) gworlds: &'a mut [PpcGWorldRecord],
    pub(super) input: PpcInputSnapshot,
    pub(super) event_queue: &'a mut EventQueue,
}

pub(super) fn dispatch_textedit_import(
    context: PpcTextEditDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcTextEditDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        toolbox_startup,
        current_gworld,
        tick_count,
        quickdraw_text_mode,
        quickdraw_text_size,
        quickdraw_fore_color,
        quickdraw_back_color,
        quickdraw_fore_indices,
        scrap,
        gworlds,
        input,
        event_queue,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::TEInit => {
            toolbox_startup.text_edit_initialized = true;
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let handle = ppc_te_scrap_handle(
                Some(&mut allocator),
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
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TENew | PpcImportDispatcherTarget::TEStyleNew => {
            // Universal Interfaces exposes both constructors as native C
            // functions taking pointers to the destination and view Rects.
            // Inside Macintosh: Text (1993), pp. 2-78 and 2-85 through 2-86.
            let styled = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::TEStyleNew
            );
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let te_handle = ppc_te_initialize_record(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                cpu.gpr[4],
                current_gworld,
                tick_count,
                quickdraw_text_mode,
                quickdraw_text_size,
                *quickdraw_fore_color,
                styled,
            );
            scrap.text_edit.register(te_handle);
            *last_mem_error = if te_handle == 0 {
                PPC_MEM_FULL_ERR
            } else {
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(te_handle))
        }
        PpcImportDispatcherTarget::TESetStyle => {
            // Universal Interfaces TextEdit.h: TESetStyle's native PPC ABI
            // is (short mode, const TextStyle *, Boolean redraw, TEHandle).
            let te_handle = cpu.gpr[6];
            let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
                return Some(PpcImportAction::ReturnPreserve);
            };
            let mut start = usize::from(
                memory
                    .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
                    .unwrap_or(0),
            );
            let mut end = usize::from(
                memory
                    .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
                    .unwrap_or(0),
            );
            if end < start {
                std::mem::swap(&mut start, &mut end);
            }
            let insertion_point = start == end;
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let style_result = if insertion_point {
                ppc_te_set_null_style(memory, te_handle, cpu.gpr[3] as u16, cpu.gpr[4])
            } else {
                ppc_te_set_style_for_range(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    te_handle,
                    start,
                    end,
                    cpu.gpr[3] as u16,
                    cpu.gpr[4],
                )
            };
            if style_result && !insertion_point && cpu.gpr[5] != 0 {
                let _ = ppc_te_recalculate_layout(
                    Some(&mut allocator),
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    te_handle,
                );
                ppc_te_draw(
                    memory,
                    handles,
                    gworlds,
                    te_handle,
                    current_gworld,
                    *quickdraw_fore_color,
                    quickdraw_fore_indices,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEUseStyleScrap => {
            // TextEdit.h exposes the native PPC ABI as (rangeStart, rangeEnd,
            // StScrpHandle, redraw, TEHandle). A style scrap may describe
            // several relative runs; preserve its first complete style for
            // the requested range until multi-run scrap application is
            // represented by the native TextEdit model.
            let range_start = cpu.gpr[3] as i32;
            let range_end = cpu.gpr[4] as i32;
            let scrap_handle = cpu.gpr[5];
            let redraw = cpu.gpr[6] != 0;
            let te_handle = cpu.gpr[7];
            let scrap_ptr = memory
                .read_u32_be(scrap_handle)
                .filter(|ptr| *ptr != 0)
                .unwrap_or(0);
            let has_style = scrap_ptr != 0
                && memory
                    .read_u16_be(scrap_ptr + PPC_TE_SCRAP_N_STYLES_OFFSET)
                    .unwrap_or(0)
                    != 0;
            if has_style {
                let source = scrap_ptr + PPC_TE_SCRAP_STYLE_TAB_OFFSET;
                let text_style_ptr =
                    ppc_process_heap_alloc(process_memory_manager, memory, heap_cursor, 12, true);
                if text_style_ptr != 0 {
                    let font = memory
                        .read_u16_be(source + PPC_TE_SCRAP_STYLE_FONT_OFFSET)
                        .unwrap_or(0);
                    let face = memory
                        .read_u8(source + PPC_TE_SCRAP_STYLE_FACE_OFFSET)
                        .unwrap_or(0);
                    let size = memory
                        .read_u16_be(source + PPC_TE_SCRAP_STYLE_SIZE_OFFSET)
                        .unwrap_or(0);
                    let _ = memory.write_u16_be(text_style_ptr, font);
                    let _ = memory.write_u8(text_style_ptr + 2, face);
                    let _ = memory.write_u16_be(text_style_ptr + 4, size);
                    for offset in [0u32, 2, 4] {
                        let component = memory
                            .read_u16_be(source + PPC_TE_SCRAP_STYLE_COLOR_OFFSET + offset)
                            .unwrap_or(0);
                        let _ = memory.write_u16_be(text_style_ptr + 6 + offset, component);
                    }
                    let (start, end) = if range_end < range_start {
                        (range_end, range_start)
                    } else {
                        (range_start, range_end)
                    };
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    if ppc_te_set_style_for_range(
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        te_handle,
                        start.max(0) as usize,
                        end.max(0) as usize,
                        0x000f,
                        text_style_ptr,
                    ) {
                        let _ = ppc_te_recalculate_layout(
                            Some(&mut allocator),
                            memory,
                            heap_cursor,
                            heap_limit,
                            last_mem_error,
                            handles,
                            te_handle,
                        );
                        if redraw {
                            ppc_te_draw(
                                memory,
                                handles,
                                gworlds,
                                te_handle,
                                current_gworld,
                                *quickdraw_fore_color,
                                quickdraw_fore_indices,
                            );
                        }
                    }
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEContinuousStyle => {
            // TextEdit stores mode and TextStyle as caller-owned out
            // parameters. Return true only when every requested attribute is
            // continuous across the current selection.
            let result =
                ppc_te_continuous_style(memory, handles, cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]);
            Some(PpcImportAction::Return(u32::from(result)))
        }
        PpcImportDispatcherTarget::TEGetText => {
            // Inside Macintosh: Text (1993), p. 2-83: TEGetText returns the
            // CharsHandle stored in TERec.hText.
            let text_handle = memory
                .read_u32_be(cpu.gpr[3])
                .filter(|ptr| *ptr != 0)
                .and_then(|ptr| memory.read_u32_be(ptr + PPC_TE_HTEXT_OFFSET))
                .unwrap_or(0);
            Some(PpcImportAction::Return(text_handle))
        }
        PpcImportDispatcherTarget::TEDispose => {
            // Inside Macintosh: Text (1993), p. 2-79: dispose the TERec, its
            // text handle, and every auxiliary handle owned by a styled TERec.
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
                cpu.gpr[3],
            );
            scrap.text_edit.remove(&cpu.gpr[3]);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEActivate { active } => {
            // Inside Macintosh: Text (1993), pp. 2-49–2-50: activation changes
            // the TERec's active state; selection/caret drawing is refreshed by
            // TEUpdate and TEIdle.
            if let Some(te_ptr) = ppc_te_record_ptr(memory, cpu.gpr[3]) {
                let _ =
                    memory.write_u16_be(te_ptr + PPC_TE_ACTIVE_OFFSET, if active { 1 } else { 0 });
                let _ = memory.write_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET, tick_count);
                let _ = memory.write_u16_be(te_ptr + PPC_TE_CARET_STATE_OFFSET, 0);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TESetSelect => {
            // Text (1993), pp. 2-51–2-52: native parameters are start, end,
            // TEHandle; the public TERec stores the clamped offsets as words.
            if let Some(te_ptr) = ppc_te_record_ptr(memory, cpu.gpr[5]) {
                let length = u32::from(
                    memory
                        .read_u16_be(te_ptr + PPC_TE_LENGTH_OFFSET)
                        .unwrap_or(0),
                );
                let start = cpu.gpr[3].min(i16::MAX as u32).min(length);
                let end = cpu.gpr[4].min(i16::MAX as u32).min(length);
                let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET, start as u16);
                let _ = memory.write_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET, end as u16);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TESetText => {
            // Text (1993), pp. 2-50–2-51: copy the caller's byte range into
            // hText, resize it, recalculate lineStarts, and place the insertion
            // point after the copied text.
            let bytes = ppc_memory_read_bytes(memory, cpu.gpr[3], cpu.gpr[4]).unwrap_or_default();
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_te_set_text(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[5],
                &bytes,
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TECalText => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_te_recalculate_layout(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
            );
            *last_mem_error = result;
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEInsert { styled } => {
            // Text (1993), pp. 2-58 and 2-82: both forms replace the current
            // selection with a copy of the byte range. The styled form's hST
            // precedes the final TEHandle argument in the native ABI.
            let te_handle = if styled { cpu.gpr[6] } else { cpu.gpr[5] };
            let bytes = ppc_memory_read_bytes(memory, cpu.gpr[3], cpu.gpr[4]).unwrap_or_default();
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_te_replace_selection(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                te_handle,
                &bytes,
            );
            *last_mem_error = result;
            ppc_te_draw(
                memory,
                handles,
                gworlds,
                te_handle,
                current_gworld,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEDelete => {
            // Text (1993), p. 2-58: deleting is selection replacement with an
            // empty byte sequence and does not alter either scrap.
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_te_replace_selection(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                cpu.gpr[3],
                &[],
            );
            *last_mem_error = result;
            ppc_te_draw(
                memory,
                handles,
                gworlds,
                cpu.gpr[3],
                current_gworld,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEKey => {
            // Inside Macintosh: Text (1993), pp. 2-81--2-82: TEKey replaces
            // the selection, with Backspace deleting either that selection or
            // the preceding character, and moves the insertion point.
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
                cpu.gpr[4],
                cpu.gpr[3] as u8,
            );
            *last_mem_error = result;
            ppc_te_draw(
                memory,
                handles,
                gworlds,
                cpu.gpr[4],
                current_gworld,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEClick => {
            // Inside Macintosh: Text (1993), p. 2-85: retain mouse ownership
            // until release, expanding or shortening the selection as it moves.
            let te_handle = cpu.gpr[5];
            let Some(te_ptr) = ppc_te_record_ptr(memory, te_handle) else {
                scrap.text_edit.clear_click_tracking();
                return Some(PpcImportAction::ReturnPreserve);
            };
            let previous_selection = (
                memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET),
                memory.read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET),
            );
            let tracking = if let Some(mut tracking) = scrap.text_edit.take_click_tracking() {
                let port = memory
                    .read_u32_be(te_ptr + PPC_TE_IN_PORT_OFFSET)
                    .unwrap_or(0);
                let bounds = memory
                    .read_u32_be(port + 2)
                    .and_then(|handle| memory.read_u32_be(handle))
                    .and_then(|pixmap| ppc_read_rect(memory, pixmap + 6))
                    .unwrap_or((0, 0, 0, 0));
                let v = input.mouse_v.wrapping_add(bounds.0);
                let h = input.mouse_h.wrapping_add(bounds.1);
                let offset = ppc_te_point_to_offset(memory, handles, te_handle, v, h).unwrap_or(0);
                if tracking.last_point != (v, h) {
                    let _ = memory.write_u16_be(
                        te_ptr + PPC_TE_SEL_START_OFFSET,
                        tracking.anchor.min(offset) as u16,
                    );
                    let _ = memory.write_u16_be(
                        te_ptr + PPC_TE_SEL_END_OFFSET,
                        tracking.anchor.max(offset) as u16,
                    );
                    tracking.last_point = (v, h);
                }
                tracking
            } else {
                let v = (cpu.gpr[3] >> 16) as i16;
                let h = cpu.gpr[3] as i16;
                let offset = ppc_te_point_to_offset(memory, handles, te_handle, v, h).unwrap_or(0);
                let old_start = memory
                    .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
                    .unwrap_or(0) as usize;
                let old_end = memory
                    .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
                    .unwrap_or(0) as usize;
                let anchor = if cpu.gpr[4] != 0 {
                    if offset < old_start {
                        old_end
                    } else {
                        old_start
                    }
                } else {
                    offset
                };
                ppc_te_click(
                    memory,
                    handles,
                    te_handle,
                    v,
                    h,
                    cpu.gpr[4] != 0,
                    tick_count,
                );
                crate::text_edit::TextEditClickTracking {
                    handle: te_handle,
                    anchor,
                    native: true,
                    last_point: (v, h),
                }
            };
            if previous_selection
                != (
                    memory.read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET),
                    memory.read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET),
                )
            {
                let port = memory
                    .read_u32_be(te_ptr + PPC_TE_IN_PORT_OFFSET)
                    .unwrap_or(current_gworld);
                if let Some(view) = ppc_read_rect(memory, te_ptr + PPC_TE_VIEW_RECT_OFFSET) {
                    let background = ppc_port_rgb_colors(memory, port)
                        .map_or(*quickdraw_back_color, |colors| colors.1);
                    ppc_paint_rect_bounds(memory, gworlds, port, view, background, None);
                }
                ppc_te_draw(
                    memory,
                    handles,
                    gworlds,
                    te_handle,
                    current_gworld,
                    *quickdraw_fore_color,
                    quickdraw_fore_indices,
                );
            }
            if input.mouse_button {
                scrap.text_edit.retain_click_tracking(tracking);
                Some(PpcImportAction::Yield(u64::MAX))
            } else {
                if let Some(index) = event_queue.iter().position(|event| event.what == 2) {
                    event_queue.remove(index);
                }
                Some(PpcImportAction::ReturnPreserve)
            }
        }
        PpcImportDispatcherTarget::TEIdle => {
            // Text (1993), p. 2-51: TEIdle only blinks an insertion-point
            // caret in an active record. Keep its public timing/state fields
            // coherent even though the framebuffer redraw stays deterministic.
            if let Some(te_ptr) = ppc_te_record_ptr(memory, cpu.gpr[3]) {
                let active = memory
                    .read_u16_be(te_ptr + PPC_TE_ACTIVE_OFFSET)
                    .unwrap_or(0)
                    != 0;
                let start = memory
                    .read_u16_be(te_ptr + PPC_TE_SEL_START_OFFSET)
                    .unwrap_or(0);
                let end = memory
                    .read_u16_be(te_ptr + PPC_TE_SEL_END_OFFSET)
                    .unwrap_or(0);
                let previous = memory
                    .read_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET)
                    .unwrap_or(0);
                if active && start == end && tick_count.wrapping_sub(previous) >= 32 {
                    let caret = memory
                        .read_u16_be(te_ptr + PPC_TE_CARET_STATE_OFFSET)
                        .unwrap_or(0);
                    let _ = memory.write_u16_be(
                        te_ptr + PPC_TE_CARET_STATE_OFFSET,
                        if caret == 0 { 1 } else { 0 },
                    );
                    let _ = memory.write_u32_be(te_ptr + PPC_TE_CARET_TIME_OFFSET, tick_count);
                }
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEUpdate => {
            // Text (1993), p. 2-88: redraw the edit record within the caller's
            // update rectangle. The existing QuickDraw target clips writes to
            // its framebuffer; line layout comes from TECalText/TESetText.
            ppc_te_draw(
                memory,
                handles,
                gworlds,
                cpu.gpr[4],
                current_gworld,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TETextBox => {
            // Inside Macintosh: Text (1993), pp. 2-88--2-89: TETextBox uses
            // the current port's text state, wraps within the supplied local
            // rectangle, and honors the four public alignment constants.
            let bytes = if (cpu.gpr[4] as i32) < 0 {
                Vec::new()
            } else {
                ppc_memory_read_bytes(memory, cpu.gpr[3], cpu.gpr[4]).unwrap_or_default()
            };
            // TextBox clears its box before drawing, including empty text.
            // Imaging With QuickDraw (1994), Printing Hints, explicitly
            // describes TextBox calling EraseRect; Text (1993), pp. 2-88--2-89.
            if let Some(rect) = ppc_read_rect(memory, cpu.gpr[5]) {
                let _ = ppc_paint_rect_bounds(
                    memory,
                    gworlds,
                    current_gworld,
                    rect,
                    *quickdraw_back_color,
                    toolbox_startup
                        .quickdraw_back_indices
                        .get(&current_gworld)
                        .copied(),
                );
            }
            ppc_te_draw_text_box(
                memory,
                gworlds,
                current_gworld,
                &bytes,
                cpu.gpr[5],
                cpu.gpr[6] as u16 as i16,
                quickdraw_text_mode,
                quickdraw_text_size,
                *quickdraw_fore_color,
                quickdraw_fore_indices.get(&current_gworld).copied(),
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TESetAlignment => {
            if let Some(te_ptr) = ppc_te_record_ptr(memory, cpu.gpr[4]) {
                let _ = memory.write_u16_be(te_ptr + PPC_TE_JUST_OFFSET, cpu.gpr[3] as u16);
                ppc_te_draw(
                    memory,
                    handles,
                    gworlds,
                    cpu.gpr[4],
                    current_gworld,
                    *quickdraw_fore_color,
                    quickdraw_fore_indices,
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEGetHeight => Some(PpcImportAction::Return(ppc_te_get_height(
            memory,
            cpu.gpr[5],
            cpu.gpr[4] as i32,
            cpu.gpr[3] as i32,
        ))),
        PpcImportDispatcherTarget::TEGetPoint => Some(PpcImportAction::Return(ppc_te_get_point(
            memory,
            handles,
            cpu.gpr[4],
            cpu.gpr[3] as u16,
        ))),
        PpcImportDispatcherTarget::TEScroll { pinned } => {
            let moved = ppc_te_scroll(
                memory,
                cpu.gpr[5],
                cpu.gpr[3] as u16 as i16,
                cpu.gpr[4] as u16 as i16,
                pinned,
            );
            // Text (1993), p. 2-89: TEScroll scrolls the text within the
            // viewRect, leaving uncovered areas in the background color as
            // ScrollRect does. Erasing the viewRect before redrawing gives
            // the same image; redrawing over the old pixels would accumulate
            // srcOr text at every previous position.
            if let Some(te_ptr) = ppc_te_record_ptr(memory, cpu.gpr[5]).filter(|_| moved) {
                let port = memory
                    .read_u32_be(te_ptr + PPC_TE_IN_PORT_OFFSET)
                    .filter(|port| *port != 0 && gworlds.iter().any(|g| g.port == *port))
                    .unwrap_or(current_gworld);
                if let Some(view) = ppc_read_rect(memory, te_ptr + PPC_TE_VIEW_RECT_OFFSET) {
                    let _ = ppc_paint_rect_bounds(
                        memory,
                        gworlds,
                        port,
                        view,
                        *quickdraw_back_color,
                        toolbox_startup.quickdraw_back_indices.get(&port).copied(),
                    );
                }
            }
            ppc_te_draw(
                memory,
                handles,
                gworlds,
                cpu.gpr[5],
                current_gworld,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEAutoView => {
            // Inside Macintosh: Text (1993), pp. 2-90--2-91: TEAutoView
            // changes private feature state rather than a public TERec field.
            scrap
                .text_edit
                .set_feature_bit(cpu.gpr[4], 0, cpu.gpr[3] != 0);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TECopy { cut, dialog } => {
            let te_handle = if dialog {
                memory
                    .read_u32_be(cpu.gpr[3].wrapping_add(PPC_DIALOG_TEXT_HANDLE_OFFSET))
                    .unwrap_or(0)
            } else {
                cpu.gpr[3]
            };
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_te_copy_or_cut(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                te_handle,
                cut,
            );
            *last_mem_error = result;
            ppc_te_draw(
                memory,
                handles,
                gworlds,
                te_handle,
                current_gworld,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TEPaste { dialog } => {
            let te_handle = if dialog {
                memory
                    .read_u32_be(cpu.gpr[3].wrapping_add(PPC_DIALOG_TEXT_HANDLE_OFFSET))
                    .unwrap_or(0)
            } else {
                cpu.gpr[3]
            };
            let bytes = ppc_te_scrap_bytes(memory);
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let result = ppc_te_replace_selection(
                Some(&mut allocator),
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                te_handle,
                &bytes,
            );
            *last_mem_error = result;
            ppc_te_draw(
                memory,
                handles,
                gworlds,
                te_handle,
                current_gworld,
                *quickdraw_fore_color,
                quickdraw_fore_indices,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TETransferScrap { from_desktop } => {
            // Inside Macintosh: Text (1993), pp. 2-95--2-96: these functions
            // copy the TEXT flavor between TextEdit's private scrap and the
            // Scrap Manager's desktop scrap and return an OSErr.
            let result = if from_desktop {
                if let Some(flavor) = scrap.desktop.flavor(*b"TEXT") {
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    ppc_te_set_scrap_bytes(
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        &flavor.data,
                    )
                } else {
                    PPC_NO_TYPE_ERR
                }
            } else {
                let bytes = ppc_te_scrap_bytes(memory);
                scrap.desktop.initialize_and_append_entry(*b"TEXT", bytes);
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::TEScrapHandle => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            let handle = ppc_te_scrap_handle(
                Some(&mut allocator),
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
            Some(PpcImportAction::Return(handle))
        }
        PpcImportDispatcherTarget::TEScrapLength { set } => {
            if set {
                let requested = cpu.gpr[3] as i32;
                let existing = ppc_te_scrap_bytes(memory);
                *last_mem_error = if requested < 0 {
                    PPC_PARAM_ERR
                } else {
                    let mut resized = existing;
                    resized.resize((requested as usize).min(i16::MAX as usize), 0);
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    ppc_te_set_scrap_bytes(
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        &resized,
                    )
                };
                Some(PpcImportAction::ReturnPreserve)
            } else {
                let length = memory
                    .read_u16_be(crate::memory::globals::addr::TE_SCRP_LENGTH)
                    .unwrap_or(0);
                Some(PpcImportAction::Return(u32::from(length)))
            }
        }
        _ => None,
    }
}
