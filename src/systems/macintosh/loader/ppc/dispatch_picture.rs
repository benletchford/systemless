//! Typed Picture Manager dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcPictureDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) heap_limit: u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) callback_stack: &'a mut Vec<PpcPictureCallbackState>,
    pub(super) color_manager_clut: &'a mut [[u16; 3]; 256],
}

pub(super) fn dispatch_picture_import(
    context: PpcPictureDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcPictureDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        vfs_resources,
        gworlds,
        current_gworld,
        screen_clut,
        color_manager_clut,
        callback_stack,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetPictInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_get_pict_info(cpu, memory, handles),
        ))),
        PpcImportDispatcherTarget::DrawPicture => {
            let resumed = callback_stack
                .last()
                .is_some_and(|frame| frame.import_pc == cpu.pc && cpu.lr == cpu.pc);
            let mut frame = if resumed {
                let mut frame = callback_stack.pop().unwrap();
                if let Some(bytes) = ppc_memory_read_bytes(memory, frame.buffer, frame.requested) {
                    frame.bytes.extend_from_slice(&bytes);
                } else {
                    frame.failed = true;
                }
                Some(frame)
            } else {
                let procs = memory.read_u32_be(current_gworld + 104).unwrap_or(0);
                let callback = if procs != 0 {
                    memory.read_u32_be(procs + 44).unwrap_or(0)
                } else {
                    0
                };
                let target = (callback != 0)
                    .then(|| ppc_resolve_callback_target(memory, callback, cpu.gpr[2], None))
                    .flatten();
                target.and_then(|target| {
                    let bytes = ppc_handle_bytes(memory, handles, cpu.gpr[3])?;
                    let header = bytes.get(..10)?.to_vec();
                    let buffer = process_memory_manager.new_native_ptr(memory, 32767, true);
                    ppc_apply_process_native_allocator(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        last_mem_error,
                    );
                    (buffer != 0).then_some(PpcPictureCallbackState {
                        import_pc: cpu.pc,
                        final_pc: cpu.lr,
                        rtoc: cpu.gpr[2],
                        picture: cpu.gpr[3],
                        destination: cpu.gpr[4],
                        gworld: current_gworld,
                        target,
                        buffer,
                        requested: 0,
                        bytes: header,
                        failed: false,
                    })
                })
            };
            if let Some(mut frame) = frame.take() {
                let progress = pict::picture_stream_progress(&frame.bytes);
                if !frame.failed {
                    if let pict::PictureStreamProgress::Need(total) = progress {
                        // Bound corrupt streams and honour StdGetPic's signed short count.
                        if total <= 32 * 1024 * 1024 {
                            frame.requested = (total - frame.bytes.len()).min(32767) as u32;
                            if let Some(action) = picture_call_callback(cpu, memory, &frame) {
                                callback_stack.push(frame);
                                return Some(action);
                            }
                        }
                    }
                }
                cpu.lr = frame.final_pc;
                cpu.gpr[2] = frame.rtoc;
                cpu.gpr[3] = frame.picture;
                cpu.gpr[4] = frame.destination;
                if !frame.failed && matches!(progress, pict::PictureStreamProgress::Complete(_)) {
                    let _ = ppc_draw_picture_bytes(
                        cpu,
                        memory,
                        vfs_resources,
                        gworlds,
                        frame.gworld,
                        screen_clut,
                        color_manager_clut,
                        &frame.bytes,
                    );
                }
                process_memory_manager.dispose_native_ptr(frame.buffer);
                ppc_apply_process_native_allocator(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                );
                return Some(PpcImportAction::ReturnPreserve);
            }
            let _ = ppc_draw_picture(
                cpu,
                memory,
                handles,
                vfs_resources,
                gworlds,
                current_gworld,
                screen_clut,
                color_manager_clut,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::KillPicture => {
            let mut allocator = PpcProcessAllocatorView {
                memory_manager: process_memory_manager,
            };
            ppc_kill_picture(
                cpu,
                &mut allocator,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            );
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PpcPictureCallbackState {
    import_pc: u32,
    final_pc: u32,
    rtoc: u32,
    picture: u32,
    destination: u32,
    gworld: u32,
    target: PpcCallbackTarget,
    buffer: u32,
    requested: u32,
    bytes: Vec<u8>,
    failed: bool,
}

fn picture_call_callback(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    frame: &PpcPictureCallbackState,
) -> Option<PpcImportAction> {
    install_powerpc_call_arguments(cpu, memory, &[frame.buffer, frame.requested])?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: frame.target.entry,
            rtoc: frame.target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            cpu.pc,
            frame.rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
}
