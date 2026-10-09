//! Typed DrawSprocket dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcDrawSprocketDispatchContext<'a> {
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
    pub(super) event_queue: &'a mut EventQueue,
    pub(super) gworld_allocations: &'a mut HashMap<u32, PpcGWorldAllocationRecord>,
    pub(super) current_gworld: &'a mut u32,
    pub(super) current_gdevice: &'a mut u32,
    pub(super) draw_sprocket: &'a mut PpcDrawSprocketState,
    pub(super) input: PpcInputSnapshot,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
    /// QDGlobals.screenBits, when InitGraf has run.
    pub(super) screen_bits: Option<u32>,
}

pub(super) fn dispatch_drawsprocket_import(
    context: PpcDrawSprocketDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcDrawSprocketDispatchContext {
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
        event_queue,
        gworld_allocations,
        current_gworld,
        current_gdevice,
        draw_sprocket,
        input,
        screen_clut,
        screen_bits,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::DSpGetVersion => {
            // Classic DrawSprocket 1.7.5 final, returned as a four-byte NumVersion.
            // PowerPC passes the structure-result address in r3.
            if cpu.gpr[3] != 0 && ppc_memory_can_write_bytes(memory, cpu.gpr[3], 4) {
                let _ = memory.write_u32_be(cpu.gpr[3], 0x0175_8000);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::DSpSetDebugMode => {
            // The advertised DrawSprocket 1.7.5 final profile is a nondebugging
            // build. Apple Game Sprockets Guide, DSpSetDebugMode (p. 2-73),
            // explicitly ignores this Boolean option in nondebugging builds.
            // Do not enable debugging-only blanking or gamma behavior here.
            Some(PpcImportAction::Return(u32::from(PPC_NO_ERR as u16)))
        }
        PpcImportDispatcherTarget::DSpStartup => {
            draw_sprocket.started = true;
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::DSpShutdown => {
            if !ppc_dsp_restore_desktop(memory, gworlds, screen_clut, draw_sprocket) {
                return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
            }
            ppc_dsp_dispose_blanking_window(
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                controls,
                gworlds,
                window_list,
                event_queue,
                current_gworld,
                current_gdevice,
                draw_sprocket,
            );
            *draw_sprocket = PpcDrawSprocketState::default();
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::DSpGetFirstContext => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_get_first_context(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpGetNextContext => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_get_next_context(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpProcessEvent => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_process_event(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpBlitFastest => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_dsp_blit_fastest(cpu, memory, gworlds),
        ))),
        PpcImportDispatcherTarget::DSpCanUserSelectContext => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_can_user_select_context(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpGetMouse => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_dsp_get_mouse(cpu, memory, &input),
        ))),
        PpcImportDispatcherTarget::DSpFindContextFromPoint => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_find_context_from_point(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpContextGlobalToLocal => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_global_to_local(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpContextLocalToGlobal => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_global_to_local(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpFindBestContext => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_dsp_find_best_context(memory, cpu.gpr[3], cpu.gpr[4], draw_sprocket),
            )))
        }
        PpcImportDispatcherTarget::DSpFindBestContextOnDisplayID => {
            let result = if cpu.gpr[5] == PPC_DSP_DISPLAY_ID {
                ppc_dsp_find_best_context(memory, cpu.gpr[3], cpu.gpr[4], draw_sprocket)
            } else {
                PPC_DSP_CONTEXT_NOT_FOUND_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DSpUserSelectContext => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_user_select_context(cpu, memory, draw_sprocket)),
        )),
        PpcImportDispatcherTarget::DSpSetBlankingColor => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_set_blanking_color(cpu, memory, draw_sprocket)),
        )),
        PpcImportDispatcherTarget::DSpAltBufferNew => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_alt_buffer_new(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                gworlds,
                gworld_allocations,
                *current_gdevice,
                draw_sprocket,
            )),
        )),
        PpcImportDispatcherTarget::DSpAltBufferGetCGrafPtr => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_alt_buffer_get_cgraf_ptr(cpu, memory, gworlds)),
        )),
        PpcImportDispatcherTarget::DSpContextReserve => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_reserve(cpu, memory, draw_sprocket, gworlds)),
        )),
        PpcImportDispatcherTarget::DSpContextRelease => {
            let result = ppc_dsp_context_release(cpu, memory, gworlds, screen_clut, draw_sprocket);
            if result == PPC_NO_ERR {
                ppc_dsp_dispose_blanking_window(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    controls,
                    gworlds,
                    window_list,
                    event_queue,
                    current_gworld,
                    current_gdevice,
                    draw_sprocket,
                );
            }
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DSpContextSetState => {
            let result = ppc_dsp_context_set_state(
                cpu,
                memory,
                gworlds,
                screen_clut,
                screen_bits,
                draw_sprocket,
            );
            if result == PPC_NO_ERR {
                if draw_sprocket.active_context.is_some() && draw_sprocket.blanking_window.is_none()
                {
                    let (width, height) = (
                        ppc_u32_to_i16_saturating(draw_sprocket.context_attributes.width),
                        ppc_u32_to_i16_saturating(draw_sprocket.context_attributes.height),
                    );
                    let mut allocator = PpcProcessAllocatorView {
                        memory_manager: process_memory_manager,
                    };
                    let window = ppc_new_cwindow_with_parameters(
                        PpcNewCWindowParameters {
                            storage_ptr: 0,
                            bounds: (0, 0, height, width),
                            visible: true,
                            proc_id: 0,
                            behind: u32::MAX,
                            go_away: false,
                            ref_con: 0,
                        },
                        Some(&mut allocator),
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        gworlds,
                        window_list,
                        *current_gdevice,
                    );
                    if window == 0 {
                        let allocation_error = *last_mem_error;
                        let mut inactive_cpu = cpu.clone();
                        inactive_cpu.gpr[4] = PPC_DSP_CONTEXT_STATE_INACTIVE;
                        let _ = ppc_dsp_context_set_state(
                            &inactive_cpu,
                            memory,
                            gworlds,
                            screen_clut,
                            screen_bits,
                            draw_sprocket,
                        );
                        return Some(PpcImportAction::Return(ppc_i16_result(allocation_error)));
                    }
                    draw_sprocket.blanking_window = Some(window);
                } else if draw_sprocket.active_context.is_none() {
                    ppc_dsp_dispose_blanking_window(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        controls,
                        gworlds,
                        window_list,
                        event_queue,
                        current_gworld,
                        current_gdevice,
                        draw_sprocket,
                    );
                }
            }
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::DSpContextGetState => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_get_state(cpu, memory, draw_sprocket)),
        )),
        PpcImportDispatcherTarget::DSpContextFadeGamma => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_dsp_context_fade_gamma(cpu, memory, draw_sprocket, PpcDspGammaFadeKind::Manual),
            )))
        }
        PpcImportDispatcherTarget::DSpContextFadeGammaIn => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_dsp_context_fade_gamma(cpu, memory, draw_sprocket, PpcDspGammaFadeKind::In),
            )))
        }
        PpcImportDispatcherTarget::DSpContextFadeGammaOut => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_dsp_context_fade_gamma(cpu, memory, draw_sprocket, PpcDspGammaFadeKind::Out),
            )))
        }
        PpcImportDispatcherTarget::DSpContextGetFrontBuffer => {
            let front_buffer_out_ptr = cpu.gpr[4];
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_dsp_context_get_buffer(
                    cpu,
                    memory,
                    draw_sprocket.front_buffer_gworld,
                    front_buffer_out_ptr,
                ),
            )))
        }
        PpcImportDispatcherTarget::DSpContextGetBackBuffer => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_get_back_buffer(cpu, memory, draw_sprocket)),
        )),
        PpcImportDispatcherTarget::DSpContextSwapBuffers => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_dsp_context_swap_buffers(cpu, memory, draw_sprocket, gworlds),
            )))
        }
        PpcImportDispatcherTarget::DSpContextSetClutEntries => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_set_clut_entries(cpu, memory, screen_clut)),
        )),
        PpcImportDispatcherTarget::DSpContextGetClutEntries => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_get_clut_entries(cpu, memory, screen_clut)),
        )),
        PpcImportDispatcherTarget::DSpContextGetDisplayID => {
            let display_id_out_ptr = cpu.gpr[4];
            if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
                Some(PpcImportAction::Return(ppc_i16_result(error)))
            } else if display_id_out_ptr == 0
                || !ppc_memory_can_write_bytes(memory, display_id_out_ptr, 4)
            {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else if memory
                .write_u32_be(display_id_out_ptr, PPC_DSP_DISPLAY_ID)
                .is_none()
            {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            }
        }
        PpcImportDispatcherTarget::DSpContextGetAttributes => {
            let attributes_out_ptr = cpu.gpr[4];
            if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
                Some(PpcImportAction::Return(ppc_i16_result(error)))
            } else if attributes_out_ptr == 0
                || !ppc_memory_can_write_bytes(
                    memory,
                    attributes_out_ptr,
                    PPC_DSP_CONTEXT_ATTRIBUTES_SIZE,
                )
            {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else if ppc_write_dsp_context_attributes(
                memory,
                attributes_out_ptr,
                draw_sprocket.context_attributes,
            )
            .is_none()
            {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            } else {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
            }
        }
        PpcImportDispatcherTarget::DSpContextGetFlattenedSize => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_get_flattened_size(cpu, memory)),
        )),
        PpcImportDispatcherTarget::DSpContextFlatten => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_flatten(cpu, memory, draw_sprocket)),
        )),
        PpcImportDispatcherTarget::DSpContextRestore => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_restore(cpu, memory, draw_sprocket)),
        )),
        PpcImportDispatcherTarget::DSpContextSetVblProc => {
            let error = if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
                error
            } else {
                draw_sprocket.vbl_proc = Some(cpu.gpr[4]);
                draw_sprocket.vbl_refcon = Some(cpu.gpr[5]);
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        PpcImportDispatcherTarget::DSpContextIsBusy => {
            let error = if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
                error
            } else {
                let busy_out = cpu.gpr[4];
                if busy_out == 0 || !ppc_memory_can_write_bytes(memory, busy_out, 1) {
                    PPC_PARAM_ERR
                } else if memory.write_u8(busy_out, 0).is_none() {
                    PPC_PARAM_ERR
                } else {
                    PPC_NO_ERR
                }
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        PpcImportDispatcherTarget::DSpAltBufferDispose => {
            let alt_buffer = cpu.gpr[3];
            let error = if alt_buffer == 0 {
                PPC_PARAM_ERR
            } else {
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        PpcImportDispatcherTarget::DSpContextInvalBackBufferRect => {
            let error = if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
                error
            } else {
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        PpcImportDispatcherTarget::DSpContextSetUnderlayAltBuffer => {
            let error = if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
                error
            } else {
                PPC_NO_ERR
            };
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn ppc_dsp_dispose_blanking_window(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    controls: &mut Vec<PpcControlRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    window_list: &SharedProcessWindowList,
    event_queue: &mut EventQueue,
    current_gworld: &mut u32,
    current_gdevice: &mut u32,
    draw_sprocket: &mut PpcDrawSprocketState,
) {
    let Some(window) = draw_sprocket.blanking_window.take() else {
        return;
    };
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
        event_queue,
        current_gworld,
        current_gdevice,
        window,
    );
}
