//! Typed QuickTime dispatch for PowerPC imports.

use super::*;

pub(super) struct PpcQuickTimeDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) vfs_directories: &'a [PpcVfsDirectory],
    pub(super) vfs_files: &'a [PpcVfsFileRecord],
    pub(super) vfs_resource_files: &'a [PpcVfsResourceFileRecord],
    pub(super) vfs_resources: &'a [PpcVfsResourceRecord],
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
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        vfs_resources,
        gworlds,
        current_gworld,
        quicktime,
        sound,
    } = context;

    match binding.dispatcher_target {
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
            let error = ppc_qt_start_movie(cpu, memory, gworlds, current_gworld, quicktime, sound);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtStopMovie => {
            let error = ppc_qt_stop_movie(cpu, quicktime, sound);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtMoviesTask => {
            let error = ppc_qt_movies_task(cpu, memory, gworlds, current_gworld, quicktime, sound);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtDisposeMovie => {
            let error = ppc_qt_dispose_movie(cpu, quicktime, sound);
            Some(PpcImportAction::Return(ppc_i16_result(
                ppc_qt_record_error(quicktime, error),
            )))
        }
        PpcImportDispatcherTarget::QtIsMovieDone => {
            let (error, done) = ppc_qt_is_movie_done(cpu, quicktime);
            let _ = ppc_qt_record_error(quicktime, error);
            Some(PpcImportAction::Return(u32::from(done)))
        }
        PpcImportDispatcherTarget::QtGoToBeginningOfMovie => {
            let error = ppc_qt_go_to_beginning_of_movie(cpu, quicktime, sound);
            let _ = ppc_qt_record_error(quicktime, error);
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::QtGoToEndOfMovie => {
            let error = ppc_qt_go_to_end_of_movie(cpu, quicktime, sound);
            let _ = ppc_qt_record_error(quicktime, error);
            Some(PpcImportAction::ReturnPreserve)
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
