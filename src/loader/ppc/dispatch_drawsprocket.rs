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
    pub(super) gworlds: &'a mut Vec<PpcGWorldRecord>,
    pub(super) gworld_allocations: &'a mut HashMap<u32, PpcGWorldAllocationRecord>,
    pub(super) current_gdevice: u32,
    pub(super) draw_sprocket: &'a mut PpcDrawSprocketState,
    pub(super) input: PpcInputSnapshot,
    pub(super) screen_clut: &'a mut [[u16; 3]; 256],
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
        gworlds,
        gworld_allocations,
        current_gdevice,
        draw_sprocket,
        input,
        screen_clut,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::DSpStartup => {
            draw_sprocket.started = true;
            Some(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
        }
        PpcImportDispatcherTarget::DSpShutdown => {
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
        PpcImportDispatcherTarget::DSpFindBestContext => {
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_dsp_find_best_context(memory, cpu.gpr[3], cpu.gpr[4], draw_sprocket),
            )))
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
                current_gdevice,
                draw_sprocket,
            )),
        )),
        PpcImportDispatcherTarget::DSpAltBufferGetCGrafPtr => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_alt_buffer_get_cgraf_ptr(cpu, memory, gworlds)),
        )),
        PpcImportDispatcherTarget::DSpContextReserve => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_reserve(cpu, memory, draw_sprocket, gworlds)),
        )),
        PpcImportDispatcherTarget::DSpContextRelease => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_release(cpu, draw_sprocket)),
        )),
        PpcImportDispatcherTarget::DSpContextSetState => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_dsp_context_set_state(cpu, draw_sprocket)),
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
