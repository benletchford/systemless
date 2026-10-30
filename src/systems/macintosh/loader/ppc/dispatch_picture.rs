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
    pub(super) files: &'a mut Vec<PpcFileRecord>,
    pub(super) vfs_files: &'a ProcessVfsFileRecords,
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) screen_clut: &'a [[u16; 3]; 256],
    pub(super) callback_stack: &'a mut Vec<PpcPictureCallbackState>,
    pub(super) toolbox_startup: &'a mut PpcToolboxStartupState,
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
        files,
        vfs_files,
        vfs_resources,
        gworlds,
        current_gworld,
        screen_clut,
        color_manager_clut,
        callback_stack,
        toolbox_startup,
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::GetPictInfo => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_get_pict_info(cpu, memory, handles),
        ))),
        PpcImportDispatcherTarget::DrawPictureFile => {
            let resumed = callback_stack.last().is_some_and(|frame| {
                frame.progress_refcon.is_some() && frame.import_pc == cpu.pc && cpu.lr == cpu.pc
            });
            if resumed {
                let mut frame = callback_stack.pop().unwrap();
                let result = cpu.gpr[3] as u16 as i16;
                if result != PPC_NO_ERR {
                    // QuickTime Components (1993), progress functions: any
                    // nonzero result aborts the operation with codecAbortErr.
                    frame.progress_error = -8967;
                }
                if frame.requested == 0 && frame.progress_error == PPC_NO_ERR {
                    frame.requested = 1;
                    match picture_call_progress(
                        cpu,
                        memory,
                        process_memory_manager,
                        heap_cursor,
                        heap_limit,
                        toolbox_startup,
                        &frame,
                        1,
                        0,
                    ) {
                        Ok(action) => {
                            callback_stack.push(frame);
                            return Some(action);
                        }
                        Err(error) => frame.progress_error = error,
                    }
                } else if frame.requested == 1 && frame.progress_error == PPC_NO_ERR {
                    cpu.gpr[3] = frame.picture;
                    cpu.gpr[4] = frame.destination;
                    if !ppc_draw_picture_bytes(
                        cpu,
                        memory,
                        vfs_resources,
                        gworlds,
                        frame.gworld,
                        screen_clut,
                        color_manager_clut,
                        &frame.bytes,
                    ) {
                        frame.progress_error = PPC_PARAM_ERR;
                    }
                }
                if frame.requested != 2 {
                    frame.requested = 2;
                    match picture_call_progress(
                        cpu,
                        memory,
                        process_memory_manager,
                        heap_cursor,
                        heap_limit,
                        toolbox_startup,
                        &frame,
                        2,
                        0,
                    ) {
                        Ok(action) => {
                            callback_stack.push(frame);
                            return Some(action);
                        }
                        Err(error) => frame.progress_error = error,
                    }
                }
                cpu.lr = frame.final_pc;
                cpu.gpr[2] = frame.rtoc;
                return Some(PpcImportAction::Return(ppc_i16_result(
                    frame.progress_error,
                )));
            }
            let result = (|| {
                // Universal Interfaces 3.4.1 ImageCompression.h.
                if ppc_read_rect(memory, cpu.gpr[4]).is_none() {
                    return Err(PPC_PARAM_ERR);
                }
                let progress = cpu.gpr[5];
                let progress_info = if matches!(progress, 0 | u32::MAX) {
                    None
                } else {
                    let procedure = memory.read_u32_be(progress).ok_or(PPC_PARAM_ERR)?;
                    let refcon = progress
                        .checked_add(4)
                        .and_then(|p| memory.read_u32_be(p))
                        .ok_or(PPC_PARAM_ERR)?;
                    if procedure == 0 {
                        None
                    } else {
                        let target = resolve_guest_procedure(
                            memory,
                            procedure,
                            cpu.gpr[2],
                            None,
                            GuestIsa::PowerPc,
                            GuestIsa::PowerPc,
                        )
                        .ok_or(PPC_PARAM_ERR)?;
                        let mapped = match target.isa {
                            GuestIsa::PowerPc => memory.read_u32_be(target.entry).is_some(),
                            GuestIsa::M68k => memory.read_u16_be(target.entry).is_some(),
                        };
                        if !mapped {
                            return Err(PPC_PARAM_ERR);
                        }
                        Some((target, refcon))
                    }
                };
                let ref_num = cpu.gpr[3] as u16 as i16;
                let file = files
                    .iter_mut()
                    .find(|file| file.ref_num == ref_num)
                    .ok_or(-51i16)?;
                let source = vfs_files
                    .iter()
                    .find(|item| item.path.eq_ignore_ascii_case(&file.path))
                    .ok_or(PPC_FNF_ERR)?;
                let data = source.data.as_ref();
                // A PICT file normally has a 512-byte header that is not
                // guaranteed to be zero; validate the stream at each offset
                // instead of interpreting plausible header bytes as a picture.
                let mut truncated = false;
                let mut picture = None;
                for offset in [512usize, 0] {
                    let Some(pict) = data.get(offset..) else {
                        continue;
                    };
                    if !matches!(ppc_qt_pict_record_offset_and_bounds(pict), Some((0, _))) {
                        continue;
                    }
                    match pict::picture_stream_progress(pict) {
                        pict::PictureStreamProgress::Complete(length) => {
                            picture = Some((offset, pict, length));
                            break;
                        }
                        pict::PictureStreamProgress::Need(_) => truncated = true,
                        pict::PictureStreamProgress::Invalid => {}
                    }
                }
                let (offset, pict, length) = picture.ok_or(if truncated {
                    PPC_EOF_ERR
                } else {
                    PPC_PARAM_ERR
                })?;
                file.position = u32::try_from(offset.saturating_add(length)).unwrap_or(u32::MAX);
                if let Some((target, refcon)) = progress_info {
                    let mut bytes = Vec::new();
                    bytes
                        .try_reserve_exact(length)
                        .map_err(|_| PPC_MEM_FULL_ERR)?;
                    bytes.extend_from_slice(&pict[..length]);
                    callback_stack
                        .try_reserve(1)
                        .map_err(|_| PPC_MEM_FULL_ERR)?;
                    let frame = PpcPictureCallbackState {
                        import_pc: cpu.pc,
                        final_pc: cpu.lr,
                        rtoc: cpu.gpr[2],
                        picture: cpu.gpr[3],
                        destination: cpu.gpr[4],
                        gworld: current_gworld,
                        target: PpcCallbackTarget {
                            entry: target.entry,
                            rtoc: target.rtoc,
                            proc_info: target.proc_info,
                            routine_flags: target.routine_flags,
                        },
                        progress_target: Some(target),
                        buffer: 0,
                        requested: 0,
                        bytes,
                        failed: false,
                        progress_refcon: Some(refcon),
                        progress_error: PPC_NO_ERR,
                    };
                    let action = picture_call_progress(
                        cpu,
                        memory,
                        process_memory_manager,
                        heap_cursor,
                        heap_limit,
                        toolbox_startup,
                        &frame,
                        0,
                        0,
                    )?;
                    callback_stack.push(frame);
                    Ok(action)
                } else if ppc_draw_picture_bytes(
                    cpu,
                    memory,
                    vfs_resources,
                    gworlds,
                    current_gworld,
                    screen_clut,
                    color_manager_clut,
                    &pict[..length],
                ) {
                    Ok(PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR)))
                } else {
                    Err(PPC_PARAM_ERR)
                }
            })();
            Some(result.unwrap_or_else(|error| PpcImportAction::Return(ppc_i16_result(error))))
        }
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
                        progress_target: None,
                        buffer,
                        requested: 0,
                        bytes: header,
                        failed: false,
                        progress_refcon: None,
                        progress_error: PPC_NO_ERR,
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
    progress_target: Option<GuestProcedure>,
    buffer: u32,
    requested: u32,
    bytes: Vec<u8>,
    failed: bool,
    progress_refcon: Option<u32>,
    progress_error: i16,
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

#[allow(clippy::too_many_arguments)]
fn picture_call_progress(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    heap_limit: u32,
    toolbox_startup: &mut PpcToolboxStartupState,
    frame: &PpcPictureCallbackState,
    message: u32,
    completeness: u32,
) -> Result<PpcImportAction, i16> {
    let target = frame.progress_target.ok_or(PPC_PARAM_ERR)?;
    let arguments = [
        message,
        completeness,
        frame.progress_refcon.ok_or(PPC_PARAM_ERR)?,
    ];
    match target.isa {
        GuestIsa::PowerPc => {
            install_powerpc_call_arguments(cpu, memory, &arguments).ok_or(PPC_PARAM_ERR)?;
            GuestCallEffect::call_guest(
                GuestCallRequest::new(GuestCallTarget {
                    isa: GuestIsa::PowerPc,
                    entry: target.entry,
                    rtoc: target.rtoc,
                }),
                GuestCallContinuation::to_powerpc(
                    PPC_GUEST_CALL_RETURN_PC,
                    cpu.pc,
                    frame.rtoc,
                    PpcNativeReturnGpr3::Preserve,
                ),
            )
            .into_ppc_import_action()
            .ok_or(PPC_PARAM_ERR)
        }
        GuestIsa::M68k => {
            let snapshot = toolbox_startup.mixed_mode_m68k.snapshot();
            let allocator_snapshot = process_memory_manager.detached_clone();
            let original_cursor = *heap_cursor;
            if dispatch_mixed_mode::ppc_mixed_mode_m68k_storage(
                Some(process_memory_manager),
                memory,
                heap_cursor,
                heap_limit,
                &toolbox_startup.mixed_mode_m68k,
            )
            .is_none()
            {
                return Err(PPC_MEM_FULL_ERR);
            }
            let action = dispatch_mixed_mode::ppc_begin_m68k_universal_proc(
                cpu,
                Some(process_memory_manager),
                memory,
                heap_cursor,
                heap_limit,
                toolbox_startup,
                target,
                0x0000_0fa0,
                None,
                arguments.to_vec(),
                cpu.pc,
                PpcNativeReturnGpr3::Preserve,
            );
            if action.is_none() {
                toolbox_startup.mixed_mode_m68k.restore_snapshot(snapshot);
                process_memory_manager.restore_native_snapshot(allocator_snapshot);
                *heap_cursor = original_cursor;
            }
            action.ok_or(PPC_PARAM_ERR)
        }
    }
}
