//! Typed QuickTime dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcQuickTimeDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) vfs_directories: &'a [PpcVfsDirectory],
    pub(super) vfs_files: &'a [PpcVfsFileRecord],
    pub(super) vfs_resource_files: &'a [PpcVfsResourceFileRecord],
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
    pub(super) current_resource_refnum: i16,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) current_gworld: u32,
    pub(super) quicktime: &'a mut PpcQuickTimeState,
    pub(super) sound: &'a mut PpcSoundState,
}

pub(super) fn dispatch_quicktime_import(
    context: PpcQuickTimeDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcQuickTimeDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        handles,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        vfs_resources,
        current_resource_refnum,
        gworlds,
        current_gworld,
        quicktime,
        sound,
    } = context;

    // Native continuations return to the import with LR equal to its PC.
    // A nested guest import has its ordinary caller LR and starts a new frame.
    if quicktime.callback_frames.last().is_some_and(|frame| {
        matches!(frame,
            quicktime::callbacks::ContinuationFrame::Delivery(frame)
                if frame.import_pc == cpu.pc && cpu.lr == cpu.pc)
    }) {
        let Some(quicktime::callbacks::ContinuationFrame::Delivery(frame)) =
            quicktime.callback_frames.pop()
        else {
            unreachable!("checked movie callback continuation");
        };
        return Some(deliver_quicktime_callbacks(
            cpu, memory, quicktime, sound, frame,
        ));
    }

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::QtAlignScreenRect => {
            Some(ppc_qt_align_screen_rect(cpu, memory))
        }
        PpcImportDispatcherTarget::QtEnterMovies => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_qt_enter_movies(quicktime),
        ))),
        PpcImportDispatcherTarget::QtExitMovies => {
            ppc_qt_exit_movies(quicktime);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QtGetMoviesError => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_qt_get_movies_error(quicktime)),
        )),
        PpcImportDispatcherTarget::QtGetMoviesStickyError => Some(PpcImportAction::Return(
            ppc_i16_result(ppc_qt_get_movies_sticky_error(quicktime)),
        )),
        PpcImportDispatcherTarget::QtClearMoviesStickyError => {
            ppc_qt_clear_movies_sticky_error(quicktime);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QtGetGraphicsImporterForFile => {
            let error = ppc_qt_get_graphics_importer_for_file(
                cpu,
                memory,
                vfs_directories,
                vfs_files,
                quicktime,
            );
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtOpenADefaultComponent => {
            let error = ppc_qt_open_default_component(cpu, memory, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtGraphicsImportSetDataHandle => {
            let error = ppc_qt_graphics_import_set_data_handle(cpu, memory, handles, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtGetImageDescriptionCTable => Some(PpcImportAction::Return(
            ppc_i16_result(quicktime::decompression::get_color_table(
                cpu,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                handles,
                vfs_resources,
                current_resource_refnum,
            )),
        )),
        PpcImportDispatcherTarget::QtDecompressSequenceBegin => {
            let result = quicktime::decompression::begin(
                cpu,
                memory,
                handles,
                gworlds,
                current_gworld,
                &mut quicktime.decompression_sequences,
                vfs_resources,
                current_resource_refnum,
            );
            Some(PpcImportAction::Return(ppc_i16_result(
                result.err().unwrap_or(PPC_NO_ERR),
            )))
        }
        PpcImportDispatcherTarget::QtCDSequenceEnd => {
            let result = quicktime.decompression_sequences.end(cpu.gpr[3]);
            Some(PpcImportAction::Return(ppc_i16_result(
                result.err().unwrap_or(PPC_NO_ERR),
            )))
        }
        PpcImportDispatcherTarget::QtDecompressSequenceFrame => {
            let callback = cpu.gpr[7];
            let restore_rtoc = cpu.gpr[2];
            let final_pc = cpu.lr;
            let completion = if callback == 0 || callback == u32::MAX {
                None
            } else {
                let target = memory.read_u32_be(callback).and_then(|procedure| {
                    ppc_resolve_callback_target(memory, procedure, restore_rtoc, None)
                });
                let refcon = callback
                    .checked_add(4)
                    .and_then(|address| memory.read_u32_be(address));
                match (target, refcon) {
                    (Some(target), Some(refcon)) => Some((target, refcon)),
                    _ => return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR))),
                }
            };
            let result = quicktime::decompression::frame(
                cpu,
                memory,
                gworlds,
                &mut quicktime.decompression_sequences,
            );
            let (error, completion_flags) = match result {
                Ok(flags) => (PPC_NO_ERR, flags),
                Err(error) => (error, 3),
            };
            let result = ppc_i16_result(error);
            if let Some((target, refcon)) = completion {
                if install_powerpc_call_arguments(
                    cpu,
                    memory,
                    &[result, u32::from(completion_flags), refcon],
                )
                .is_none()
                {
                    return Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)));
                }
                return GuestCallEffect::call_guest(
                    GuestCallRequest::new(GuestCallTarget {
                        isa: GuestIsa::PowerPc,
                        entry: target.entry,
                        rtoc: target.rtoc,
                    }),
                    GuestCallContinuation::to_powerpc(
                        PPC_GUEST_CALL_RETURN_PC,
                        final_pc,
                        restore_rtoc,
                        PpcNativeReturnGpr3::Set(result),
                    ),
                )
                .into_ppc_import_action();
            }
            Some(PpcImportAction::Return(result))
        }
        PpcImportDispatcherTarget::QtDecompressImage => Some(PpcImportAction::Return(
            ppc_i16_result(super::quicktime::decompression::decompress_image(
                cpu,
                memory,
                handles,
                current_gworld,
                vfs_resources,
                current_resource_refnum,
            )),
        )),
        PpcImportDispatcherTarget::QtGraphicsImportGetImageDescription => {
            let error = ppc_qt_graphics_import_get_image_description(
                cpu,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                handles,
                quicktime,
            );
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect => {
            let error = ppc_qt_graphics_import_get_bounds_rect(cpu, memory, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtGraphicsImportSetGWorld => {
            let error = ppc_qt_graphics_import_set_gworld(cpu, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtGraphicsImportDraw => {
            let error =
                ppc_qt_graphics_import_draw(cpu, memory, gworlds, current_gworld, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtOpenMovieFile => {
            let error = ppc_qt_open_movie_file(cpu, memory, vfs_directories, vfs_files, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtNewMovieFromFile => {
            let error = ppc_qt_new_movie_from_file(
                cpu,
                memory,
                vfs_files,
                vfs_resource_files,
                vfs_resources,
                quicktime,
                sound,
            );
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtGetMovieBox => {
            let error = ppc_qt_get_movie_box(cpu, memory, quicktime);
            let _ = ppc_qt_record_error(quicktime, error);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QtSetMovieBox => {
            let error = ppc_qt_set_movie_box(cpu, memory, quicktime);
            let _ = ppc_qt_record_error(quicktime, error);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QtSetMovieGWorld => {
            let error = ppc_qt_set_movie_gworld(cpu, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtStartMovie => {
            let before = quicktime_callback_rate(quicktime);
            let error = ppc_qt_start_movie(cpu, memory, gworlds, current_gworld, quicktime, sound);
            if error == PPC_NO_ERR {
                let after = quicktime_callback_rate(quicktime);
                quicktime.callbacks.mark_event(
                    PPC_QT_MOVIE + 0x10,
                    quicktime::callbacks::Event::Rate { before, after },
                );
            }
            Some(finish_quicktime_callbacks(
                cpu, memory, quicktime, sound, error, false, false,
            ))
        }
        PpcImportDispatcherTarget::QtStopMovie => {
            let before = quicktime_callback_rate(quicktime);
            let error = ppc_qt_stop_movie(cpu, quicktime, sound);
            if error == PPC_NO_ERR {
                quicktime.callbacks.mark_event(
                    PPC_QT_MOVIE + 0x10,
                    quicktime::callbacks::Event::Rate { before, after: 0 },
                );
            }
            Some(finish_quicktime_callbacks(
                cpu, memory, quicktime, sound, error, false, false,
            ))
        }
        PpcImportDispatcherTarget::QtMoviesTask => {
            let before = quicktime_callback_time(quicktime);
            let playing = quicktime.movie_started;
            let rate_before = quicktime_callback_rate(quicktime);
            let (error, drew) =
                ppc_qt_movies_task(cpu, memory, gworlds, current_gworld, quicktime, sound);
            if error == PPC_NO_ERR {
                ppc_qt_mark_drawing_complete(quicktime, drew);
            }
            if error == PPC_NO_ERR && playing {
                let after = quicktime_callback_time(quicktime);
                quicktime.callbacks.mark_time_due(
                    PPC_QT_MOVIE + 0x10,
                    before,
                    after,
                    quicktime.movie_time_scale,
                );
            }
            if error == PPC_NO_ERR && playing {
                let rate_after = quicktime_callback_rate(quicktime);
                quicktime.callbacks.mark_event(
                    PPC_QT_MOVIE + 0x10,
                    quicktime::callbacks::Event::Rate {
                        before: rate_before,
                        after: rate_after,
                    },
                );
                if !quicktime.movie_started {
                    quicktime.callbacks.mark_event(
                        PPC_QT_MOVIE + 0x10,
                        quicktime::callbacks::Event::Extremes {
                            start: false,
                            stop: true,
                        },
                    );
                }
            }
            Some(finish_quicktime_callbacks(
                cpu, memory, quicktime, sound, error, false, false,
            ))
        }
        PpcImportDispatcherTarget::QtDisposeMovie => {
            let error = if cpu.gpr[3] == PPC_QT_MOVIE {
                PPC_NO_ERR
            } else {
                PPC_PARAM_ERR
            };
            if error == PPC_NO_ERR && !quicktime.movie_disposed {
                quicktime
                    .callbacks
                    .mark_event(PPC_QT_MOVIE + 0x10, quicktime::callbacks::Event::Disposed);
            }
            Some(finish_quicktime_callbacks(
                cpu,
                memory,
                quicktime,
                sound,
                error,
                false,
                error == PPC_NO_ERR,
            ))
        }
        PpcImportDispatcherTarget::QtIsMovieDone => {
            let (error, done) = ppc_qt_is_movie_done(cpu, quicktime);
            let _ = ppc_qt_record_error(quicktime, error);
            Some(PpcImportAction::Return(u32::from(done)))
        }
        PpcImportDispatcherTarget::QtGoToBeginningOfMovie => {
            let before = quicktime_callback_time(quicktime);
            let error = ppc_qt_go_to_beginning_of_movie(cpu, quicktime, sound);
            if error == PPC_NO_ERR && before != quicktime_callback_time(quicktime) {
                quicktime
                    .callbacks
                    .mark_event(PPC_QT_MOVIE + 0x10, quicktime::callbacks::Event::Jump);
                quicktime.callbacks.mark_event(
                    PPC_QT_MOVIE + 0x10,
                    quicktime::callbacks::Event::Extremes {
                        start: true,
                        stop: false,
                    },
                );
            }
            Some(finish_quicktime_callbacks(
                cpu, memory, quicktime, sound, error, true, false,
            ))
        }
        PpcImportDispatcherTarget::QtGoToEndOfMovie => {
            let before = quicktime_callback_time(quicktime);
            let error = ppc_qt_go_to_end_of_movie(cpu, quicktime, sound);
            if error == PPC_NO_ERR && before != quicktime_callback_time(quicktime) {
                quicktime
                    .callbacks
                    .mark_event(PPC_QT_MOVIE + 0x10, quicktime::callbacks::Event::Jump);
                quicktime.callbacks.mark_event(
                    PPC_QT_MOVIE + 0x10,
                    quicktime::callbacks::Event::Extremes {
                        start: false,
                        stop: true,
                    },
                );
            }
            Some(finish_quicktime_callbacks(
                cpu, memory, quicktime, sound, error, true, false,
            ))
        }
        PpcImportDispatcherTarget::QtGetMovieDuration => {
            let (error, duration) = ppc_qt_get_movie_duration(cpu, quicktime);
            let _ = ppc_qt_record_error(quicktime, error);
            Some(PpcImportAction::Return(duration))
        }
        PpcImportDispatcherTarget::QtLoadMovieIntoRam => {
            let error = ppc_qt_load_movie_into_ram(cpu, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtCloseMovieFile => {
            let error = ppc_qt_close_movie_file(cpu, quicktime);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        _ => None,
    }
}

pub(super) fn quicktime_callback_time(quicktime: &PpcQuickTimeState) -> i64 {
    let duration = ppc_qt_movie_duration_value(quicktime);
    i64::try_from(ppc_qt_movie_position_in_scale(quicktime, duration)).unwrap_or(i64::MAX)
}

fn quicktime_callback_rate(quicktime: &PpcQuickTimeState) -> i32 {
    if quicktime.movie_started {
        quicktime.movie_playback_rate
    } else {
        0
    }
}

pub(super) fn quicktime_callback_progress(quicktime: &PpcQuickTimeState) -> (i64, i32, bool) {
    (
        quicktime_callback_time(quicktime),
        quicktime_callback_rate(quicktime),
        quicktime.movie_started,
    )
}

pub(super) fn finish_controller_callbacks(
    action: PpcImportAction,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
    before: (i64, i32, bool),
    service_drawing: bool,
) -> PpcImportAction {
    if !matches!(action, PpcImportAction::Return(0)) {
        return action;
    }
    let (before_time, before_rate, was_playing) = before;
    let (after_time, after_rate, is_playing) = quicktime_callback_progress(quicktime);
    if service_drawing {
        ppc_qt_mark_drawing_complete(quicktime, was_playing && quicktime.movie_active);
    }
    if was_playing {
        quicktime.callbacks.mark_time_due(
            PPC_QT_MOVIE + 0x10,
            before_time,
            after_time,
            quicktime.movie_time_scale,
        );
        if !is_playing {
            quicktime.callbacks.mark_event(
                PPC_QT_MOVIE + 0x10,
                quicktime::callbacks::Event::Extremes {
                    start: false,
                    stop: true,
                },
            );
        }
    }
    quicktime.callbacks.mark_event(
        PPC_QT_MOVIE + 0x10,
        quicktime::callbacks::Event::Rate {
            before: before_rate,
            after: after_rate,
        },
    );
    finish_quicktime_callbacks(cpu, memory, quicktime, sound, PPC_NO_ERR, false, false)
}

fn finish_quicktime_callbacks(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
    error: i16,
    preserve: bool,
    dispose_movie: bool,
) -> PpcImportAction {
    ppc_qt_record_error(quicktime, error);
    let result = if preserve {
        cpu.gpr[3]
    } else {
        ppc_i16_result(error)
    };
    let frame = quicktime::callbacks::DeliveryFrame {
        import_pc: cpu.pc,
        final_pc: cpu.lr,
        restore_rtoc: cpu.gpr[2],
        result,
        dispose_movie,
    };
    if error != PPC_NO_ERR {
        return PpcImportAction::Return(result);
    }
    deliver_quicktime_callbacks(cpu, memory, quicktime, sound, frame)
}

fn deliver_quicktime_callbacks(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    quicktime: &mut PpcQuickTimeState,
    sound: &mut PpcSoundState,
    frame: quicktime::callbacks::DeliveryFrame,
) -> PpcImportAction {
    while let Some((address, registration)) = quicktime.callbacks.take_ready() {
        let Some(target) =
            ppc_resolve_callback_target(memory, registration.procedure, registration.rtoc, None)
        else {
            continue;
        };
        if install_powerpc_call_arguments(cpu, memory, &[address, registration.refcon]).is_none() {
            break;
        }
        quicktime
            .callback_frames
            .push(quicktime::callbacks::ContinuationFrame::Delivery(frame));
        return GuestCallEffect::call_guest(
            GuestCallRequest::new(GuestCallTarget {
                isa: GuestIsa::PowerPc,
                entry: target.entry,
                rtoc: target.rtoc,
            }),
            GuestCallContinuation::to_powerpc(
                PPC_GUEST_CALL_RETURN_PC,
                frame.import_pc,
                frame.restore_rtoc,
                PpcNativeReturnGpr3::Set(frame.result),
            ),
        )
        .into_ppc_import_action()
        .unwrap();
    }
    cpu.lr = frame.final_pc;
    cpu.gpr[2] = frame.restore_rtoc;
    if frame.dispose_movie {
        quicktime.callbacks.retire_time_base(PPC_QT_MOVIE + 0x10);
        let saved = cpu.gpr[3];
        cpu.gpr[3] = PPC_QT_MOVIE;
        ppc_qt_dispose_movie(cpu, quicktime, sound);
        cpu.gpr[3] = saved;
    }
    PpcImportAction::Return(frame.result)
}

fn ppc_qt_align_screen_rect(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> PpcImportAction {
    // IM:QuickTime (1993), pp. 3-142, 3-146 and 3-155: global rectangle,
    // depth-specific standard alignment, then an optional application callback.
    let rect = cpu.gpr[3];
    let record = cpu.gpr[4];
    if rect == 0 {
        return PpcImportAction::ReturnPreserve;
    }
    if !ppc_memory_can_write_bytes(memory, rect, 8) {
        return PpcImportAction::Halt;
    }
    let callback = if record == 0 {
        None
    } else {
        let Some(callback) = memory
            .read_u32_be(record)
            .and_then(|procedure| ppc_resolve_callback_target(memory, procedure, cpu.gpr[2], None))
        else {
            return PpcImportAction::Halt;
        };
        let Some(refcon) = record
            .checked_add(4)
            .and_then(|address| memory.read_u32_be(address))
        else {
            return PpcImportAction::Halt;
        };
        if memory.read_u32_be(callback.entry).is_none() {
            return PpcImportAction::Halt;
        }
        Some((callback, refcon))
    };
    let Some((top, left, bottom, right)) = ppc_read_rect(memory, rect) else {
        return PpcImportAction::Halt;
    };
    // There is one display in the current machine profile. Offscreen worlds
    // are not screens; use the main device's live PixMap, not the current port.
    if top < bottom
        && left < right
        && bottom > 0
        && right > 0
        && i32::from(top) < ppc_main_screen_height() as i32
        && i32::from(left) < ppc_main_screen_width() as i32
    {
        let step: i32 = match memory.read_u16_be(PPC_MAIN_PIXMAP + 32) {
            Some(1 | 2) => 16,
            Some(4) => 8,
            Some(8 | 32) => 4, // The PowerPC profile is of 68040-or-later class.
            Some(16) => 2,
            _ => return PpcImportAction::Halt,
        };
        let aligned = (i32::from(left) + step / 2) & !(step - 1);
        let delta = (aligned - i32::from(left)) as i16;
        if ppc_write_rect(
            memory,
            rect,
            top,
            aligned as i16,
            bottom,
            right.wrapping_add(delta),
        )
        .is_none()
        {
            return PpcImportAction::Halt;
        }
    }
    let Some((callback, refcon)) = callback else {
        return PpcImportAction::ReturnPreserve;
    };
    let final_pc = cpu.lr;
    let restore_rtoc = cpu.gpr[2];
    if install_powerpc_call_arguments(cpu, memory, &[rect, refcon]).is_none() {
        return PpcImportAction::Halt;
    }
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: callback.entry,
            rtoc: callback.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            final_pc,
            restore_rtoc,
            PpcNativeReturnGpr3::Set(rect),
        ),
    )
    .into_ppc_import_action()
    .unwrap_or(PpcImportAction::Halt)
}
