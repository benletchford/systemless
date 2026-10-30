//! PowerPC CFM HLE import dispatcher.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use super::*;

pub(crate) struct PpcDispatchContext<'a> {
    pub(crate) binding: &'a PpcImportBinding,
    pub(crate) agl: &'a mut PpcAglState,
    pub(crate) cpu: &'a mut PpcCpu,
    pub(crate) memory: &'a mut PpcSectionMem,
    pub(crate) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(crate) heap_cursor: &'a mut u32,
    pub(crate) heap_limit: u32,
    pub(crate) stack_base: u32,
    pub(crate) stack_top: u32,
    pub(crate) native_heap_ceiling: u32,
    pub(crate) last_mem_error: &'a mut i16,
    pub(crate) tick_count: &'a mut u32,
    pub(crate) cycles_per_tick: u32,
    pub(crate) current_resource_refnum: &'a mut i16,
    pub(crate) last_resource_error: &'a mut i16,
    pub(crate) resource_policy: &'a SharedProcessResourcePolicy,
    pub(crate) native_exception_handler: &'a Cell<u32>,
    pub(crate) stdc_qsort_stack: &'a mut Vec<PpcQsortState>,
    pub(crate) dialog_callback_stack: &'a mut Vec<PpcDialogCallbackState>,
    pub(crate) collection_callback_stack: &'a mut Vec<PpcCollectionCallbackState>,
    pub(crate) picture_callback_stack: &'a mut Vec<dispatch_picture::PpcPictureCallbackState>,
    pub(crate) drag_tracking_stack: &'a mut Vec<PpcDragTrackingState>,
    pub(crate) apple_events: &'a mut PpcAppleEventState,
    pub(crate) cfm_connections: &'a mut Vec<PpcCfmConnection>,
    pub(crate) cfm_library_fragments: &'a mut Vec<PpcCfmLibraryFragment>,
    pub(crate) next_cfm_connection_id: &'a mut u32,
    pub(crate) import_run_state: &'a mut PpcImportRunState,
    pub(crate) controls: &'a mut Vec<PpcControlRecord>,
    pub(crate) aliases: &'a mut Vec<PpcAliasRecord>,
    pub(crate) gworlds: &'a mut Vec<PpcGWorldRecord>,
    pub(crate) gworld_pixel_states: &'a SharedProcessQuickDrawPixelStates,
    pub(crate) window_list: &'a SharedProcessWindowList,
    pub(crate) q3_objects: &'a mut Vec<PpcQ3ObjectRecord>,
    pub(crate) q3_object_refs: &'a mut Vec<PpcQ3ObjectReferenceRecord>,
    pub(crate) next_q3_object: &'a mut u32,
    pub(crate) q3_error_state: &'a mut PpcQ3ErrorState,
    pub(crate) q3_lifecycle: &'a mut PpcQ3LifecycleState,
    pub(crate) q3_memory_storages: &'a mut Vec<PpcQ3MemoryStorageRecord>,
    pub(crate) q3_files: &'a mut Vec<PpcQ3FileRecord>,
    pub(crate) q3_group_memberships: &'a mut Vec<PpcQ3GroupMembershipRecord>,
    pub(crate) q3_file_groups: &'a mut Vec<PpcQ3FileGroupRecord>,
    pub(crate) q3_views: &'a mut Vec<PpcQ3ViewStateRecord>,
    pub(crate) q3_submissions: &'a mut Vec<PpcQ3SubmissionRecord>,
    pub(crate) q3_view_transforms: &'a mut Vec<PpcQ3ViewTransformRecord>,
    pub(crate) q3_submission_transforms: &'a mut Vec<PpcQ3SubmissionTransformRecord>,
    pub(crate) q3_view_materials: &'a mut Vec<PpcQ3ViewMaterialRecord>,
    pub(crate) q3_submission_materials: &'a mut Vec<PpcQ3SubmissionMaterialRecord>,
    pub(crate) q3_submission_lights: &'a mut Vec<PpcQ3SubmissionLightRecord>,
    pub(crate) q3_view_state_stack: &'a mut Vec<PpcQ3ViewStateSnapshotRecord>,
    pub(crate) q3_completed_frames: &'a mut Vec<PpcQ3CompletedFrameRecord>,
    pub(crate) q3_retained_frames: &'a mut Vec<PpcQ3RetainedFrameRecord>,
    pub(crate) q3_state_only_completed_frame_batches:
        &'a mut Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    pub(crate) q3_fog_styles: &'a mut Vec<PpcQ3FogStyleRecord>,
    pub(crate) q3_attributes: &'a mut Vec<PpcQ3AttributeRecord>,
    pub(crate) q3_shader_uv_transforms: &'a mut Vec<PpcQ3ShaderUvTransformRecord>,
    pub(crate) q3_shader_boundaries: &'a mut Vec<PpcQ3ShaderBoundaryRecord>,
    pub(crate) q3_mipmap_textures: &'a mut Vec<PpcQ3MipmapTextureRecord>,
    pub(crate) q3_texture_shaders: &'a mut Vec<PpcQ3TextureShaderRecord>,
    pub(crate) q3_renderer_preferences: &'a mut Vec<PpcQ3RendererPreferenceRecord>,
    pub(crate) q3_draw_contexts: &'a mut Vec<PpcQ3DrawContextRecord>,
    pub(crate) q3_trimeshes: &'a mut Vec<PpcQ3TriMeshRecord>,
    pub(crate) q3_styles: &'a mut Vec<PpcQ3StyleRecord>,
    pub(crate) q3_cameras: &'a mut Vec<PpcQ3CameraRecord>,
    pub(crate) q3_lights: &'a mut Vec<PpcQ3LightRecord>,
    pub(crate) input_sprocket: &'a mut PpcInputSprocketState,
    pub(crate) input_sprocket_virtual_elements: &'a mut Vec<PpcInputSprocketVirtualElementRecord>,
    pub(crate) toolbox_startup: &'a mut PpcToolboxStartupState,
    pub(crate) quicktime: &'a mut PpcQuickTimeState,
    pub(crate) sound: &'a mut PpcSoundState,
    pub(crate) timer_tasks: &'a SharedProcessTimerTasks,
    pub(crate) vbl_tasks: &'a SharedProcessVblTasks,
    pub(crate) callback_scheduling: &'a SharedProcessCallbackScheduling,
    pub(crate) files: &'a mut Vec<PpcFileRecord>,
    pub(crate) writable_refnums: &'a mut HashSet<u16>,
    pub(crate) vfs_files: &'a mut ProcessVfsFileRecords,
    pub(crate) stdio_streams: &'a mut HashMap<u32, PpcStdioStreamRecord>,
    pub(crate) deleted_vfs_file_paths: &'a mut Vec<String>,
    pub(crate) resource_files: &'a mut Vec<PpcResourceFileRecord>,
    pub(crate) vfs_resource_files: &'a mut ProcessVfsResourceFileRecords,
    pub(crate) vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    pub(crate) next_file_ref_num: &'a mut i16,
    pub(crate) current_gworld: &'a mut u32,
    pub(crate) current_gdevice: &'a mut u32,
    pub(crate) quickdraw_op_colors: &'a SharedProcessQuickDrawOpColors,
    pub(crate) quickdraw_hilite_colors: &'a SharedProcessQuickDrawHiliteColors,
    pub(crate) screen_clut: &'a mut [[u16; 3]; 256],
    pub(crate) color_manager_clut: &'a mut [[u16; 3]; 256],
    pub(crate) display_gamma: &'a SharedProcessDisplayGamma,
    pub(crate) quickdraw_fore_color: &'a mut PpcRgbColor,
    pub(crate) quickdraw_fore_indices: &'a mut HashMap<u32, u8>,
    pub(crate) quickdraw_back_color: &'a mut PpcRgbColor,
    pub(crate) quickdraw_pen_h: &'a mut i16,
    pub(crate) quickdraw_pen_v: &'a mut i16,
    pub(crate) quickdraw_text_mode: &'a mut i16,
    pub(crate) quickdraw_text_size: &'a mut i16,
    pub(crate) cursor_state: &'a SharedProcessCursorState,
    pub(crate) vfs_volumes: &'a [PpcVfsVolumeRecord],
    pub(crate) vfs_directories: &'a mut Vec<PpcVfsDirectory>,
    pub(crate) next_vfs_dir_id: &'a mut u32,
    pub(crate) default_dir_id: u32,
    pub(crate) working_directories: &'a mut HashMap<i16, ProcessWorkingDirectory>,
    pub(crate) next_working_directory_ref_num: &'a mut i16,
    pub(crate) application_working_directory_ref_num: &'a mut i16,
    pub(crate) launched_app_path: Option<&'a str>,
    pub(crate) param_text: &'a SharedProcessDialogText,
    pub(crate) scrap: &'a mut PpcScrapState,
    pub(crate) list_manager: &'a mut ProcessListManagerState,
    pub(crate) collections: &'a SharedProcessCollectionManager,
    pub(crate) drag_manager: &'a SharedProcessDragManager,
    pub(crate) input: PpcInputSnapshot,
    pub(crate) event_queue: &'a mut EventQueue,
    pub(crate) draw_sprocket: &'a mut PpcDrawSprocketState,
}

pub(crate) fn dispatch_supported_import(
    mut context: PpcDispatchContext<'_>,
) -> Option<PpcImportAction> {
    if let Some(action) = dispatch_launch::dispatch(&mut context) {
        return Some(action);
    }
    if let PpcImportDispatcherTarget::StdDescriptor(operation) = context.binding.dispatcher_target {
        return Some(dispatch_descriptor::dispatch(operation, context));
    }
    let PpcDispatchContext {
        binding,
        agl,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        stack_base,
        stack_top,
        native_heap_ceiling,
        last_mem_error,
        tick_count,
        cycles_per_tick,
        current_resource_refnum,
        last_resource_error,
        resource_policy,
        native_exception_handler,
        stdc_qsort_stack,
        dialog_callback_stack,
        collection_callback_stack,
        picture_callback_stack,
        drag_tracking_stack,
        apple_events,
        cfm_connections,
        cfm_library_fragments,
        next_cfm_connection_id,
        import_run_state,
        controls,
        aliases,
        gworlds,
        gworld_pixel_states,
        window_list,
        q3_objects,
        q3_object_refs,
        next_q3_object,
        q3_error_state,
        q3_lifecycle,
        q3_memory_storages,
        q3_files,
        q3_group_memberships,
        q3_file_groups,
        q3_views,
        q3_submissions,
        q3_view_transforms,
        q3_submission_transforms,
        q3_view_materials,
        q3_submission_materials,
        q3_submission_lights,
        q3_view_state_stack,
        q3_completed_frames,
        q3_retained_frames,
        q3_state_only_completed_frame_batches,
        q3_fog_styles,
        q3_attributes,
        q3_shader_uv_transforms,
        q3_shader_boundaries,
        q3_mipmap_textures,
        q3_texture_shaders,
        q3_renderer_preferences,
        q3_draw_contexts,
        q3_trimeshes,
        q3_styles,
        q3_cameras,
        q3_lights,
        input_sprocket,
        input_sprocket_virtual_elements,
        toolbox_startup,
        quicktime,
        sound,
        timer_tasks,
        vbl_tasks,
        callback_scheduling,
        files,
        writable_refnums,
        vfs_files,
        stdio_streams,
        deleted_vfs_file_paths,
        resource_files,
        vfs_resource_files,
        vfs_resources,
        next_file_ref_num,
        current_gworld,
        current_gdevice,
        quickdraw_op_colors,
        quickdraw_hilite_colors,
        screen_clut,
        color_manager_clut,
        display_gamma,
        quickdraw_fore_color,
        quickdraw_fore_indices,
        quickdraw_back_color,
        quickdraw_pen_h,
        quickdraw_pen_v,
        quickdraw_text_mode,
        quickdraw_text_size,
        cursor_state,
        vfs_volumes,
        vfs_directories,
        next_vfs_dir_id,
        default_dir_id,
        working_directories,
        next_working_directory_ref_num,
        application_working_directory_ref_num,
        launched_app_path,
        param_text,
        scrap,
        list_manager,
        collections,
        drag_manager,
        input,
        event_queue,
        draw_sprocket,
    } = context;
    if matches!(binding.dispatcher_target,
        PpcImportDispatcherTarget::CloseWindow |
        PpcImportDispatcherTarget::LegacyWindow(PpcLegacyWindowOperation::DisposeWindow)) {
        if toolbox_startup.drag_hilite.as_ref().is_some_and(|hilite| hilite.port == cpu.gpr[3]) {
            dispatch_drag::hide_drag_hilite(memory, toolbox_startup);
        }
        drag_manager.with_mut(|manager| manager.remove_window(cpu.gpr[3]));
    }
    let _menu_root = (matches!(
        binding.dispatcher_target,
        PpcImportDispatcherTarget::MenuSelect | PpcImportDispatcherTarget::PopUpMenuSelect
    ))
    .then(|| {
        let call = match binding.dispatcher_target {
            PpcImportDispatcherTarget::PopUpMenuSelect => ppc_popup_menu_call(cpu),
            _ => ppc_menu_select_call(cpu, cpu.gpr[3]),
        };
        toolbox_startup.execution.enter_menu_call(call)
    });
    // The process registry is authoritative. Refresh the legacy vector
    // booleans at each native boundary so rendering/debugging code that still
    // reads them sees any classic-side transition before this import runs.
    ppc_sync_gworld_pixel_state_mirrors(gworlds, gworld_pixel_states);

    // Toolbox helpers use an import-scoped read view for guest ABI decoding.
    // Allocation ownership remains in the process Memory Manager, and the
    // next import observes its canonical records immediately.
    let handles = &mut process_memory_manager.native_handle_records().to_vec();
    if is_quickdraw_3d_library(&binding.library_name)
        && !matches!(
            binding.dispatcher_target,
            PpcImportDispatcherTarget::Q3ErrorGet
        )
        && q3_error_state.clear_on_next_q3_call
    {
        q3_error_state.clear();
    }

    if let Some(action) =
        dispatch_qd3d::dispatch_q3_core_import(dispatch_qd3d::PpcQ3CoreDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            q3_objects,
            next_q3_object,
            q3_error_state,
            q3_lifecycle,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_qd3d::dispatch_q3_storage_file_import(
        dispatch_qd3d::PpcQ3StorageFileDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            process_memory_manager,
            memory,
            stores: PpcQ3ObjectStores {
                q3_objects,
                q3_object_refs,
                q3_renderer_preferences,
                q3_files,
                q3_group_memberships,
                q3_file_groups,
                q3_views,
                q3_submissions,
                q3_view_transforms,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_submission_lights,
                q3_view_state_stack,
                q3_completed_frames,
                q3_retained_frames,
                q3_fog_styles,
                q3_memory_storages,
                q3_attributes,
                q3_shader_uv_transforms,
                q3_shader_boundaries,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_draw_contexts,
                q3_trimeshes,
                q3_styles,
                q3_cameras,
                q3_lights,
            },
            next_q3_object,
            q3_error_state,
            heap_cursor,
            heap_limit,
            last_mem_error,
            vfs_directories,
            vfs_files,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_qd3d::dispatch_q3_geometry_import(dispatch_qd3d::PpcQ3GeometryDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            process_memory_manager,
            memory,
            stores: PpcQ3ObjectStores {
                q3_objects,
                q3_object_refs,
                q3_renderer_preferences,
                q3_files,
                q3_group_memberships,
                q3_file_groups,
                q3_views,
                q3_submissions,
                q3_view_transforms,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_submission_lights,
                q3_view_state_stack,
                q3_completed_frames,
                q3_retained_frames,
                q3_fog_styles,
                q3_memory_storages,
                q3_attributes,
                q3_shader_uv_transforms,
                q3_shader_boundaries,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_draw_contexts,
                q3_trimeshes,
                q3_styles,
                q3_cameras,
                q3_lights,
            },
            next_q3_object,
            q3_error_state,
            heap_cursor,
            last_mem_error,
            gworlds,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_qd3d::dispatch_q3_group_view_import(dispatch_qd3d::PpcQ3GroupViewDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            stores: PpcQ3ObjectStores {
                q3_objects,
                q3_object_refs,
                q3_renderer_preferences,
                q3_files,
                q3_group_memberships,
                q3_file_groups,
                q3_views,
                q3_submissions,
                q3_view_transforms,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_submission_lights,
                q3_view_state_stack,
                q3_completed_frames,
                q3_retained_frames,
                q3_fog_styles,
                q3_memory_storages,
                q3_attributes,
                q3_shader_uv_transforms,
                q3_shader_boundaries,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_draw_contexts,
                q3_trimeshes,
                q3_styles,
                q3_cameras,
                q3_lights,
            },
            next_q3_object,
            q3_state_only_completed_frame_batches,
            gworlds,
            current_gworld: *current_gworld,
            q3_error_state,
            input_idle: input.is_idle(),
        })
    {
        return Some(action);
    }

    let mut current_menu_list = ppc_current_menu_list(memory);
    if let Some(action) = dispatch_cfm::dispatch_cfm_import(dispatch_cfm::PpcCfmDispatchContext {
        binding,
        cpu,
        guest_calls: &toolbox_startup.execution.calls(),
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        stack_base,
        cfm_connections,
        cfm_library_fragments,
        vfs_files,
        vfs_resource_files,
        vfs_directories,
        next_cfm_connection_id,
        import_run_state,
    }) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_files::dispatch_file_import(dispatch_files::PpcFileDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            handles,
            aliases,
            files,
            writable_refnums,
            vfs_files,
            vfs_directories,
            next_vfs_dir_id,
            deleted_vfs_file_paths,
            vfs_resource_files,
            resource_files,
            vfs_resources,
            next_file_ref_num,
            current_resource_refnum,
            last_resource_error,
            default_dir_id,
            launched_app_path,
            vfs_volumes,
            working_directories,
            next_working_directory_ref_num,
            application_working_directory_ref_num,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_resources::dispatch_resource_import(
        dispatch_resources::PpcResourceDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            resource_files,
            vfs_resource_files,
            vfs_resources,
            current_resource_refnum,
            resource_policy,
            last_resource_error,
        },
    ) {
        return Some(action);
    }

    if let Some(action) = dispatch_icon_services::dispatch_icon_services_import(
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        &mut toolbox_startup.icon_refs,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        vfs_resources,
        gworlds,
        *current_gworld,
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_stdc::dispatch_stdc_import(dispatch_stdc::PpcStdCDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            stdc_qsort_stack,
            stdc_signal_state: &mut toolbox_startup.stdc_signal_state,
            stdc_jmpbufs: &mut toolbox_startup.stdc_jmpbufs,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_core_foundation::dispatch_core_foundation_import(
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        toolbox_startup,
        vfs_files,
        vfs_directories,
        launched_app_path,
    ) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_regions::dispatch_region_import(dispatch_regions::PpcRegionDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            current_gworld: *current_gworld,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_polygons::dispatch_polygon_import(dispatch_polygons::PpcPolygonDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            current_gworld: *current_gworld,
            quickdraw_fore_color,
            quickdraw_back_color,
            quickdraw_fore_indices,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_bit_transfers::dispatch_bit_transfer_import(
        dispatch_bit_transfers::PpcBitTransferDispatchContext {
            binding,
            cpu,
            memory,
            gworlds,
            window_list,
            current_gworld: *current_gworld,
            current_gdevice: *current_gdevice,
            quickdraw_op_colors,
            color_manager_clut,
            quickdraw_fore_color: *quickdraw_fore_color,
            quickdraw_fore_index: quickdraw_fore_indices.get(current_gworld).copied(),
            quickdraw_back_color: *quickdraw_back_color,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }
    if let Some(action) = dispatch_graphics_devices::dispatch_graphics_device_import(
        dispatch_graphics_devices::PpcGraphicsDeviceDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            current_gdevice,
            toolbox_startup,
            screen_clut,
            color_manager_clut,
        },
    ) {
        return Some(action);
    }
    if let Some(action) = dispatch_color_tables::dispatch_color_table_import(
        dispatch_color_tables::PpcColorTableDispatchContext {
            binding,
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            current_gworld,
            quickdraw_hilite_colors,
            tick_count,
            current_gdevice,
            screen_clut,
            color_manager_clut,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_palettes::dispatch_palette_import(dispatch_palettes::PpcPaletteDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            gworlds,
            window_list,
            current_gdevice: *current_gdevice,
            screen_clut,
            color_manager_clut,
            toolbox_startup,
            event_queue,
            tick_count: *tick_count,
            input,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_gworlds::dispatch_gworld_import(dispatch_gworlds::PpcGWorldDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            gworld_pixel_states,
            current_gworld,
            current_gdevice,
            quickdraw_op_colors,
            quickdraw_hilite_colors,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_back_color,
            quickdraw_pen_h,
            quickdraw_pen_v,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_quickdraw::dispatch_quickdraw_import(
        dispatch_quickdraw::PpcQuickDrawDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            handles,
            gworlds,
            tick_count: *tick_count,
            current_gworld: *current_gworld,
            current_gdevice: *current_gdevice,
            quickdraw_op_colors,
            quickdraw_hilite_colors,
            screen_clut,
            color_manager_clut,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_back_color,
            quickdraw_pen_h,
            quickdraw_pen_v,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_picture::dispatch_picture_import(dispatch_picture::PpcPictureDispatchContext {
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
            current_gworld: *current_gworld,
            screen_clut,
            color_manager_clut,
            callback_stack: picture_callback_stack,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_fonts::dispatch_font_import(dispatch_fonts::PpcFontDispatchContext {
            binding,
            cpu,
            memory,
            toolbox_startup,
            gworlds,
            current_gworld: *current_gworld,
            quickdraw_text_mode,
            quickdraw_text_size,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_pen_h,
            quickdraw_pen_v,
            vfs_resources,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_sound::dispatch_sound_import(dispatch_sound::PpcSoundDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            current_resource_refnum: *current_resource_refnum,
            handles,
            files,
            vfs_files,
            vfs_resources,
            sound,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_event::dispatch_time_import(dispatch_event::PpcTimeDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            tick_count: *tick_count,
            cycles_per_tick,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_event::dispatch_event_import(dispatch_event::PpcEventDispatchContext {
            binding,
            cpu,
            memory,
            handles,
            gworlds,
            current_gworld: *current_gworld,
            current_menu_list,
            screen_clut,
            toolbox_startup,
            apple_events,
            event_queue,
            input,
            tick_count: *tick_count,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_low_memory::dispatch_low_memory_import(
        dispatch_low_memory::PpcLowMemoryDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            current_menu_list,
            default_dir_id,
        },
    ) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_math::dispatch_math_import(&binding.dispatcher_target, cpu, memory)
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_toolbox::dispatch_toolbox_import(dispatch_toolbox::PpcToolboxDispatchContext {
            binding,
            cpu,
            memory,
            init_graf_global_ptr: toolbox_startup.init_graf_global_ptr,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_memory::dispatch_memory_import(dispatch_memory::PpcMemoryDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            native_heap_ceiling,
            last_mem_error,
            handles,
            aliases,
            vfs_resources,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_menu::dispatch_menu_import(dispatch_menu::PpcMenuDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            last_resource_error,
            handles,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            resource_policy,
            toolbox_startup,
            current_menu_list: &mut current_menu_list,
            gworlds,
            screen_clut,
            current_gworld,
            current_gdevice,
            event_queue,
            input,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_threads::dispatch_thread_import(dispatch_threads::PpcThreadDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_textedit::dispatch_textedit_import(dispatch_textedit::PpcTextEditDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            toolbox_startup,
            current_gworld: *current_gworld,
            tick_count: *tick_count,
            quickdraw_text_mode: *quickdraw_text_mode,
            quickdraw_text_size: *quickdraw_text_size,
            quickdraw_fore_color,
            quickdraw_back_color,
            quickdraw_fore_indices,
            scrap,
            gworlds,
            input,
            event_queue,
        })
    {
        return Some(action);
    }
    let screen_bits = ppc_screen_bits_addr(toolbox_startup.init_graf_global_ptr)
        .filter(|ptr| *ptr != 0 && ppc_memory_can_write_bytes(memory, *ptr, 14));
    if let Some(action) = dispatch_drawsprocket::dispatch_drawsprocket_import(
        dispatch_drawsprocket::PpcDrawSprocketDispatchContext {
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
            gworld_allocations: &mut toolbox_startup.gworld_allocations,
            current_gworld,
            current_gdevice,
            draw_sprocket,
            input,
            screen_clut,
            screen_bits,
        },
    ) {
        return Some(action);
    }
    if let Some(action) = inputsprocket_drivers::dispatch_driver_import(
        binding, cpu, memory, process_memory_manager, heap_cursor,
        &mut toolbox_startup.input_sprocket_drivers,
        input_sprocket,
        input,
        input_sprocket_virtual_elements,
    ) {
        return Some(action);
    }
    if let Some(action) = dispatch_inputsprocket::dispatch_inputsprocket_import(
        dispatch_inputsprocket::PpcInputSprocketDispatchContext {
            drivers: &mut toolbox_startup.input_sprocket_drivers,
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            input_sprocket,
            input_sprocket_virtual_elements,
            input,
            tick_count: *tick_count,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
        },
    ) {
        return Some(action);
    }
    ppc_qt_service_clock(
        quicktime,
        sound,
        callback_scheduling
            .current_subtick()
            .max(u64::from(*tick_count) * crate::time_manager::SUBTICKS_PER_TICK),
    );
    if let Some(action) = dispatch_quicktime::dispatch_quicktime_import(
        dispatch_quicktime::PpcQuickTimeDispatchContext {
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
            current_resource_refnum: *current_resource_refnum,
            gworlds,
            current_gworld: *current_gworld,
            quicktime,
            sound,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_time::dispatch_time_import(dispatch_time::PpcTimeDispatchContext {
            binding,
            cpu,
            memory,
            timer_tasks,
            vbl_tasks,
            callback_scheduling,
            tick_count: *tick_count,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_standard_file::dispatch_standard_file_import(
        dispatch_standard_file::PpcStandardFileDispatchContext {
            binding,
            cpu,
            memory,
            startup: toolbox_startup,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            gworlds,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            vfs_volumes,
            default_dir_id,
            working_directories,
            next_working_directory_ref_num,
            event_queue,
            heap_limit,
            handles,
            apple_event_descriptors: &apple_events.descriptors,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_control::dispatch_control_import(dispatch_control::PpcControlDispatchContext {
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
            current_gworld: *current_gworld,
            toolbox_startup,
            input,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_list::dispatch_list_import(dispatch_list::PpcListDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            list_manager,
            gworlds,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            tick_count: *tick_count,
        })
    {
        return Some(action);
    }

    if let PpcImportDispatcherTarget::Drag(operation @ (PpcDragOperation::ShowHilite | PpcDragOperation::HideHilite)) = binding.dispatcher_target {
        let reference = cpu.gpr[3];
        let result = drag_manager.with_ref(|manager| manager.get(reference).map(|_| ()));
        let result = result.and_then(|_| {
            if operation == PpcDragOperation::ShowHilite {
                dispatch_drag::show_drag_hilite(memory, gworlds, *current_gworld,
                    quickdraw_hilite_colors, toolbox_startup, reference, cpu.gpr[4], cpu.gpr[5] as u8 != 0)
            } else {
                if toolbox_startup.drag_hilite.as_ref().is_some_and(|hilite| hilite.reference == reference) {
                    dispatch_drag::hide_drag_hilite(memory, toolbox_startup);
                }
                Ok(())
            }
        });
        return Some(PpcImportAction::Return(ppc_i16_result(result.err().unwrap_or(0))));
    }
    if binding.dispatcher_target == PpcImportDispatcherTarget::Drag(PpcDragOperation::Dispose)
        && toolbox_startup.drag_hilite.as_ref().is_some_and(|hilite| hilite.reference == cpu.gpr[3]) {
        dispatch_drag::hide_drag_hilite(memory, toolbox_startup);
    }
    if binding.dispatcher_target == PpcImportDispatcherTarget::Drag(PpcDragOperation::ZoomRects) {
        return Some(dispatch_drag::zoom_rects(
            cpu,
            memory,
            gworlds,
            toolbox_startup,
            *tick_count,
            cycles_per_tick,
        ));
    }
    if binding.dispatcher_target == PpcImportDispatcherTarget::Drag(PpcDragOperation::WaitMouseMoved) {
        let initial = ((cpu.gpr[3] >> 16) as u16 as i16, cpu.gpr[3] as u16 as i16);
        return Some(match crate::drag_manager::wait_mouse_moved(initial, (input.mouse_v, input.mouse_h), input.mouse_button) {
            crate::drag_manager::MouseMoveWaitResult::Moved => PpcImportAction::Return(1),
            crate::drag_manager::MouseMoveWaitResult::Released => PpcImportAction::Return(0),
            crate::drag_manager::MouseMoveWaitResult::Waiting => PpcImportAction::Yield(u64::MAX),
        });
    }
    if binding.dispatcher_target == PpcImportDispatcherTarget::Drag(PpcDragOperation::SetImage) {
        let result = dispatch_drag::set_drag_image(cpu, memory, drag_manager, gworlds, screen_clut);
        return Some(PpcImportAction::Return(ppc_i16_result(
            result.err().unwrap_or(0),
        )));
    }
    if binding.dispatcher_target == PpcImportDispatcherTarget::Drag(PpcDragOperation::Track) {
        return Some(dispatch_drag::track_drag(cpu, memory, drag_manager, drag_tracking_stack,
            gworlds, window_list, input, event_queue, screen_clut));
    }

    if let Some(action) = dispatch_drag::dispatch_drag_import(binding, cpu, memory, drag_manager) {
        return Some(action);
    }

    if let Some(action) = dispatch_collection::dispatch_collection_import(
        dispatch_collection::PpcCollectionDispatchContext {
            binding,
            cpu,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            collections,
            callback_stack: collection_callback_stack,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_theme_button::dispatch_theme_button(dispatch_theme_button::ThemeButtonContext {
            binding,
            cpu,
            memory,
            manager: process_memory_manager,
            heap_cursor,
            last_mem_error,
            state: &mut toolbox_startup.theme_buttons,
            gworlds,
            port: current_gworld,
            device: current_gdevice,
            fore_color: quickdraw_fore_color,
            back_color: quickdraw_back_color,
            pen_h: quickdraw_pen_h,
            pen_v: quickdraw_pen_v,
            text_mode: quickdraw_text_mode,
            text_size: quickdraw_text_size,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_appearance::dispatch_appearance_import(
        binding,
        cpu,
        memory,
        handles,
        controls,
        gworlds,
        vfs_resources,
        *current_resource_refnum,
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_dialog::dispatch_dialog_import(dispatch_dialog::PpcDialogDispatchContext {
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
            blanking_window: draw_sprocket.blanking_window,
            toolbox_startup,
            event_queue,
            dialog_callback_stack,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
            param_text,
            tick_count: *tick_count,
            input,
            quickdraw_text_mode: *quickdraw_text_mode,
            quickdraw_text_size: *quickdraw_text_size,
            quickdraw_fore_color,
            quickdraw_back_color,
            quickdraw_fore_indices,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_window::dispatch_window_import(dispatch_window::PpcWindowDispatchContext {
            cursor_state,
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
            tick_count: *tick_count,
            event_queue,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_display::dispatch_display_import(dispatch_display::PpcDisplayDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            toolbox_startup,
            screen_clut,
            color_manager_clut,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_devices::dispatch_device_import(dispatch_devices::PpcDeviceDispatchContext {
            binding,
            cpu,
            memory,
            current_gdevice: *current_gdevice,
            screen_clut,
            display_gamma,
            toolbox_startup,
            sound,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_desk::dispatch_desk_import(dispatch_desk::PpcDeskDispatchContext { binding, cpu })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_gestalt::dispatch_gestalt_import(dispatch_gestalt::PpcGestaltDispatchContext {
            binding,
            cpu,
            memory,
            toolbox_startup,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_cursor::dispatch_cursor_import(dispatch_cursor::PpcCursorDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
            cursor_state,
            gworlds,
            current_gworld: *current_gworld,
            screen_clut,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_apple_events::dispatch_apple_event_import(
        dispatch_apple_events::PpcAppleEventDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            apple_events,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_process::dispatch_process_import(dispatch_process::PpcProcessDispatchContext {
            binding,
            cpu,
            memory,
            stack_top,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            launched_app_path,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_scrap::dispatch_scrap_import(dispatch_scrap::PpcScrapDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            scrap,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_mixed_mode::dispatch_mixed_mode_import(
        dispatch_mixed_mode::PpcMixedModeDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            toolbox_startup,
            cfm_connections,
            next_cfm_connection_id,
            import_run_state,
        },
    ) {
        return action;
    }

    if let Some(action) = dispatch_native_exceptions::dispatch_native_exception_import(
        dispatch_native_exceptions::PpcNativeExceptionDispatchContext {
            binding,
            cpu,
            native_exception_handler,
        },
    ) {
        return Some(action);
    }

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::DrawThemeButton => {
            unreachable!("theme button imports return through dispatch_theme_button")
        }
        PpcImportDispatcherTarget::NSpInitialize => {
            // Apple MacErrors.h: kNSpOTNotPresentErr. NetSprocket cannot
            // initialize without Open Transport; do not advertise networking
            // or allocate an initialized instance when no transport exists.
            Some(PpcImportAction::Return((-30370i32) as u32))
        }
        PpcImportDispatcherTarget::Drag(_) => {
            unreachable!("drag imports return through dispatch_drag_import")
        }
        PpcImportDispatcherTarget::Collection(_) => {
            unreachable!("collection imports return through dispatch_collection_import")
        }
        PpcImportDispatcherTarget::InstallExceptionHandler => {
            unreachable!("native exception imports return through dispatch_native_exception_import")
        }
        PpcImportDispatcherTarget::RegisterAppearanceClient
        | PpcImportDispatcherTarget::IsThemeInColor
        | PpcImportDispatcherTarget::ActivateControl
        | PpcImportDispatcherTarget::DeactivateControl
        | PpcImportDispatcherTarget::IsControlActive
        | PpcImportDispatcherTarget::CollapseWindow
        | PpcImportDispatcherTarget::IsWindowCollapsed
        | PpcImportDispatcherTarget::UnregisterAppearanceClient
        | PpcImportDispatcherTarget::SetControlFontStyle => {
            unreachable!("appearance imports return through dispatch_appearance_import")
        }
        PpcImportDispatcherTarget::NewPtr { .. }
        | PpcImportDispatcherTarget::DisposePtr
        | PpcImportDispatcherTarget::GetPtrSize
        | PpcImportDispatcherTarget::SetPtrSize
        | PpcImportDispatcherTarget::RecoverHandle
        | PpcImportDispatcherTarget::BlockMove
        | PpcImportDispatcherTarget::BlockZero
        | PpcImportDispatcherTarget::PtrToHand
        | PpcImportDispatcherTarget::PtrToXHand
        | PpcImportDispatcherTarget::HandToHand
        | PpcImportDispatcherTarget::HandAndHand
        | PpcImportDispatcherTarget::PtrAndHand
        | PpcImportDispatcherTarget::NewHandle { .. }
        | PpcImportDispatcherTarget::NewString
        | PpcImportDispatcherTarget::SetString
        | PpcImportDispatcherTarget::NewHandleSys { .. }
        | PpcImportDispatcherTarget::TempNewHandle
        | PpcImportDispatcherTarget::TempDisposeHandle
        | PpcImportDispatcherTarget::TempHLock
        | PpcImportDispatcherTarget::TempHUnlock
        | PpcImportDispatcherTarget::HoldMemory
        | PpcImportDispatcherTarget::UnholdMemory
        | PpcImportDispatcherTarget::DisposeHandle
        | PpcImportDispatcherTarget::EmptyHandle
        | PpcImportDispatcherTarget::ReallocateHandle
        | PpcImportDispatcherTarget::GetHandleSize
        | PpcImportDispatcherTarget::SetHandleSize
        | PpcImportDispatcherTarget::HLock
        | PpcImportDispatcherTarget::HLockHi
        | PpcImportDispatcherTarget::HGetState
        | PpcImportDispatcherTarget::HSetState
        | PpcImportDispatcherTarget::HUnlock
        | PpcImportDispatcherTarget::MoveHHi
        | PpcImportDispatcherTarget::HNoPurge
        | PpcImportDispatcherTarget::HPurge
        | PpcImportDispatcherTarget::GetZone
        | PpcImportDispatcherTarget::SetZone
        | PpcImportDispatcherTarget::InitZone
        | PpcImportDispatcherTarget::SystemZone
        | PpcImportDispatcherTarget::ApplicationZone
        | PpcImportDispatcherTarget::GetApplZone
        | PpcImportDispatcherTarget::SetApplZone
        | PpcImportDispatcherTarget::MaxApplZone
        | PpcImportDispatcherTarget::MoreMasters
        | PpcImportDispatcherTarget::FlushCodeCache
        | PpcImportDispatcherTarget::GetApplLimit
        | PpcImportDispatcherTarget::SetApplLimit
        | PpcImportDispatcherTarget::HeapFreeBytes
        | PpcImportDispatcherTarget::MaxMem
        | PpcImportDispatcherTarget::TempMaxMem
        | PpcImportDispatcherTarget::TempTopMem
        | PpcImportDispatcherTarget::PurgeMem
        | PpcImportDispatcherTarget::PurgeMemSys
        | PpcImportDispatcherTarget::MemError => {
            unreachable!("memory imports return through dispatch_memory_import")
        }
        PpcImportDispatcherTarget::InitMenus
        | PpcImportDispatcherTarget::NewMenu
        | PpcImportDispatcherTarget::DisposeMenu
        | PpcImportDispatcherTarget::GetMenu
        | PpcImportDispatcherTarget::GetItemCmd
        | PpcImportDispatcherTarget::SetItemCmd
        | PpcImportDispatcherTarget::GetItemMark
        | PpcImportDispatcherTarget::CountMItems
        | PpcImportDispatcherTarget::GetMenuItemText
        | PpcImportDispatcherTarget::SetMenuItemText
        | PpcImportDispatcherTarget::SetMenuItemCommandID
        | PpcImportDispatcherTarget::GetMenuItemCommandID
        | PpcImportDispatcherTarget::SetMenuItemRefCon
        | PpcImportDispatcherTarget::GetMenuItemRefCon
        | PpcImportDispatcherTarget::DeleteMenuItem
        | PpcImportDispatcherTarget::CalcMenuSize
        | PpcImportDispatcherTarget::PopUpMenuSelect
        | PpcImportDispatcherTarget::InsertMenu
        | PpcImportDispatcherTarget::DeleteMenu
        | PpcImportDispatcherTarget::AppendMenu
        | PpcImportDispatcherTarget::InsertMenuItem
        | PpcImportDispatcherTarget::AppendResMenu
        | PpcImportDispatcherTarget::InsertResMenu
        | PpcImportDispatcherTarget::EnableMenuItem
        | PpcImportDispatcherTarget::DisableMenuItem
        | PpcImportDispatcherTarget::IsMenuItemEnabled
        | PpcImportDispatcherTarget::SetItemMark
        | PpcImportDispatcherTarget::CheckItem
        | PpcImportDispatcherTarget::GetMenuBar
        | PpcImportDispatcherTarget::GetNewMBar
        | PpcImportDispatcherTarget::ClearMenuBar
        | PpcImportDispatcherTarget::SetMenuBar
        | PpcImportDispatcherTarget::GetMenuHandle
        | PpcImportDispatcherTarget::SetMenuID
        | PpcImportDispatcherTarget::DrawMenuBar
        | PpcImportDispatcherTarget::InvalMenuBar
        | PpcImportDispatcherTarget::FlashMenuBar
        | PpcImportDispatcherTarget::HMGetHelpMenuHandle
        | PpcImportDispatcherTarget::HMGetBalloons
        | PpcImportDispatcherTarget::HMShowBalloon
        | PpcImportDispatcherTarget::SetMCEntries
        | PpcImportDispatcherTarget::HMIsBalloon
        | PpcImportDispatcherTarget::HMSetBalloons
        | PpcImportDispatcherTarget::HiliteMenu
        | PpcImportDispatcherTarget::MenuNoop
        | PpcImportDispatcherTarget::MenuKey
        | PpcImportDispatcherTarget::MenuEvent
        | PpcImportDispatcherTarget::MenuChoice
        | PpcImportDispatcherTarget::GetMBarHeight
        | PpcImportDispatcherTarget::HideMenuBar
        | PpcImportDispatcherTarget::ShowMenuBar
        | PpcImportDispatcherTarget::SetMBarHeight
        | PpcImportDispatcherTarget::MenuSelect => {
            unreachable!("menu imports return through dispatch_menu_import")
        }
        PpcImportDispatcherTarget::GetCurrentThread
        | PpcImportDispatcherTarget::MpCreateSemaphore
        | PpcImportDispatcherTarget::MpDeleteSemaphore
        | PpcImportDispatcherTarget::MpSignalSemaphore
        | PpcImportDispatcherTarget::MpWaitOnSemaphore
        | PpcImportDispatcherTarget::NewThreadEntryUPP
        | PpcImportDispatcherTarget::DisposeThreadEntryUPP
        | PpcImportDispatcherTarget::NewThreadTerminationUPP
        | PpcImportDispatcherTarget::DisposeThreadTerminationUPP
        | PpcImportDispatcherTarget::NewThreadSwitchUPP
        | PpcImportDispatcherTarget::DisposeThreadSwitchUPP
        | PpcImportDispatcherTarget::SetThreadTerminator
        | PpcImportDispatcherTarget::SetThreadSwitcher
        | PpcImportDispatcherTarget::GetThreadState
        | PpcImportDispatcherTarget::GetThreadCurrentTaskRef
        | PpcImportDispatcherTarget::GetThreadStateGivenTaskRef
        | PpcImportDispatcherTarget::SetThreadReadyGivenTaskRef
        | PpcImportDispatcherTarget::SetThreadState
        | PpcImportDispatcherTarget::SetThreadStateEndCritical
        | PpcImportDispatcherTarget::CreateThreadPool
        | PpcImportDispatcherTarget::GetFreeThreadCount
        | PpcImportDispatcherTarget::GetSpecificFreeThreadCount
        | PpcImportDispatcherTarget::GetDefaultThreadStackSize
        | PpcImportDispatcherTarget::ThreadCurrentStackSpace
        | PpcImportDispatcherTarget::NewThread
        | PpcImportDispatcherTarget::YieldToThread
        | PpcImportDispatcherTarget::MpYield
        | PpcImportDispatcherTarget::YieldToAnyThread
        | PpcImportDispatcherTarget::DisposeThread
        | PpcImportDispatcherTarget::ThreadBeginCritical
        | PpcImportDispatcherTarget::ThreadEndCritical => {
            unreachable!("thread imports return through dispatch_thread_import")
        }
        PpcImportDispatcherTarget::TEInit
        | PpcImportDispatcherTarget::TENew
        | PpcImportDispatcherTarget::TEStyleNew
        | PpcImportDispatcherTarget::TESetStyle
        | PpcImportDispatcherTarget::TEUseStyleScrap
        | PpcImportDispatcherTarget::TEContinuousStyle
        | PpcImportDispatcherTarget::TEGetText
        | PpcImportDispatcherTarget::TEDispose
        | PpcImportDispatcherTarget::TEActivate { .. }
        | PpcImportDispatcherTarget::TESetSelect
        | PpcImportDispatcherTarget::TESetClickLoop
        | PpcImportDispatcherTarget::TESetText
        | PpcImportDispatcherTarget::TECalText
        | PpcImportDispatcherTarget::TEInsert { .. }
        | PpcImportDispatcherTarget::TEDelete { .. }
        | PpcImportDispatcherTarget::TEKey
        | PpcImportDispatcherTarget::TEClick
        | PpcImportDispatcherTarget::TEIdle
        | PpcImportDispatcherTarget::TEUpdate
        | PpcImportDispatcherTarget::TETextBox
        | PpcImportDispatcherTarget::TESetAlignment
        | PpcImportDispatcherTarget::TEGetHeight
        | PpcImportDispatcherTarget::TEGetPoint
        | PpcImportDispatcherTarget::TEScroll { .. }
        | PpcImportDispatcherTarget::TESelView
        | PpcImportDispatcherTarget::TEGetOffset
        | PpcImportDispatcherTarget::TEGetStyle
        | PpcImportDispatcherTarget::TEAutoView
        | PpcImportDispatcherTarget::TEFeatureFlag
        | PpcImportDispatcherTarget::TECopy { .. }
        | PpcImportDispatcherTarget::TEPaste { .. }
        | PpcImportDispatcherTarget::TETransferScrap { .. }
        | PpcImportDispatcherTarget::TEScrapHandle
        | PpcImportDispatcherTarget::TEGetStyleScrapHandle
        | PpcImportDispatcherTarget::TEGetStyleHandle
        | PpcImportDispatcherTarget::TENumStyles
        | PpcImportDispatcherTarget::TESetStyleHandle
        | PpcImportDispatcherTarget::TEReplaceStyle
        | PpcImportDispatcherTarget::TEScrapLength { .. } => {
            unreachable!("textedit imports return through dispatch_textedit_import")
        }
        PpcImportDispatcherTarget::DSpStartup
        | PpcImportDispatcherTarget::DSpSetDebugMode
        | PpcImportDispatcherTarget::DSpGetVersion
        | PpcImportDispatcherTarget::DSpShutdown
        | PpcImportDispatcherTarget::DSpGetFirstContext
        | PpcImportDispatcherTarget::DSpGetNextContext
        | PpcImportDispatcherTarget::DSpProcessEvent
        | PpcImportDispatcherTarget::DSpBlitFastest
        | PpcImportDispatcherTarget::DSpCanUserSelectContext
        | PpcImportDispatcherTarget::DSpGetMouse
        | PpcImportDispatcherTarget::DSpFindContextFromPoint
        | PpcImportDispatcherTarget::DSpContextGlobalToLocal
        | PpcImportDispatcherTarget::DSpContextLocalToGlobal
        | PpcImportDispatcherTarget::DSpFindBestContext
        | PpcImportDispatcherTarget::DSpFindBestContextOnDisplayID
        | PpcImportDispatcherTarget::DSpUserSelectContext
        | PpcImportDispatcherTarget::DSpSetBlankingColor
        | PpcImportDispatcherTarget::DSpAltBufferNew
        | PpcImportDispatcherTarget::DSpAltBufferGetCGrafPtr
        | PpcImportDispatcherTarget::DSpAltBufferInvalRect
        | PpcImportDispatcherTarget::DSpContextReserve
        | PpcImportDispatcherTarget::DSpContextRelease
        | PpcImportDispatcherTarget::DSpContextSetState
        | PpcImportDispatcherTarget::DSpContextGetState
        | PpcImportDispatcherTarget::DSpContextGetMonitorFrequency
        | PpcImportDispatcherTarget::DSpContextGetDirtyRectGridUnits
        | PpcImportDispatcherTarget::DSpContextGetDirtyRectGridSize
        | PpcImportDispatcherTarget::DSpContextSetDirtyRectGridSize
        | PpcImportDispatcherTarget::DSpContextFadeGamma
        | PpcImportDispatcherTarget::DSpContextFadeGammaIn
        | PpcImportDispatcherTarget::DSpContextFadeGammaOut
        | PpcImportDispatcherTarget::DSpContextGetFrontBuffer
        | PpcImportDispatcherTarget::DSpContextGetBackBuffer
        | PpcImportDispatcherTarget::DSpContextSwapBuffers
        | PpcImportDispatcherTarget::DSpContextSetClutEntries
        | PpcImportDispatcherTarget::DSpContextGetClutEntries
        | PpcImportDispatcherTarget::DSpContextGetDisplayID
        | PpcImportDispatcherTarget::DSpContextGetAttributes
        | PpcImportDispatcherTarget::DSpContextGetFlattenedSize
        | PpcImportDispatcherTarget::DSpContextFlatten
        | PpcImportDispatcherTarget::DSpContextRestore
        | PpcImportDispatcherTarget::DSpContextSetVblProc
        | PpcImportDispatcherTarget::DSpContextIsBusy
        | PpcImportDispatcherTarget::DSpAltBufferDispose
        | PpcImportDispatcherTarget::DSpContextInvalBackBufferRect
        | PpcImportDispatcherTarget::DSpContextSetUnderlayAltBuffer => {
            unreachable!("drawsprocket imports return through dispatch_drawsprocket_import")
        }
        PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds
        | PpcImportDispatcherTarget::ISpElementListNew
        | PpcImportDispatcherTarget::ISpElementListDispose
        | PpcImportDispatcherTarget::ISpElementListAddElements
        | PpcImportDispatcherTarget::ISpElementListGetNextEvent
        | PpcImportDispatcherTarget::ISpElementListFlush
        | PpcImportDispatcherTarget::ISpDevicesExtract
        | PpcImportDispatcherTarget::ISpDevicesExtractByClass
        | PpcImportDispatcherTarget::ISpDeviceGetDefinition
        | PpcImportDispatcherTarget::ISpDeviceGetElementList
        | PpcImportDispatcherTarget::ISpGetGlobalElementList
        | PpcImportDispatcherTarget::ISpElementListExtract
        | PpcImportDispatcherTarget::ISpElementGetInfo
        | PpcImportDispatcherTarget::ISpElementGetConfigurationInfo
        | PpcImportDispatcherTarget::ISpElementGetSimpleState
        | PpcImportDispatcherTarget::ISpGetVersion
        | PpcImportDispatcherTarget::ISpStartup
        | PpcImportDispatcherTarget::ISpShutdown
        | PpcImportDispatcherTarget::ISpInit
        | PpcImportDispatcherTarget::ISpStop
        | PpcImportDispatcherTarget::ISpSuspend
        | PpcImportDispatcherTarget::ISpResume
        | PpcImportDispatcherTarget::ISpDevicesActivate
        | PpcImportDispatcherTarget::ISpDevicesDeactivate
        | PpcImportDispatcherTarget::ISpConfigure => {
            unreachable!("inputsprocket imports return through dispatch_inputsprocket_import")
        }
        PpcImportDispatcherTarget::QtEnterMovies
        | PpcImportDispatcherTarget::QtExitMovies
        | PpcImportDispatcherTarget::QtGetMoviesError
        | PpcImportDispatcherTarget::QtGetMoviesStickyError
        | PpcImportDispatcherTarget::QtClearMoviesStickyError
        | PpcImportDispatcherTarget::QtGetGraphicsImporterForFile
        | PpcImportDispatcherTarget::QtOpenADefaultComponent
        | PpcImportDispatcherTarget::QtGraphicsImportSetDataHandle
        | PpcImportDispatcherTarget::QtDecompressSequenceBegin
        | PpcImportDispatcherTarget::QtDecompressSequenceFrame
        | PpcImportDispatcherTarget::QtCDSequenceEnd
        | PpcImportDispatcherTarget::QtGetImageDescriptionCTable
        | PpcImportDispatcherTarget::QtDecompressImage
        | PpcImportDispatcherTarget::QtGraphicsImportGetImageDescription
        | PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect
        | PpcImportDispatcherTarget::QtGraphicsImportSetGWorld
        | PpcImportDispatcherTarget::QtGraphicsImportDraw
        | PpcImportDispatcherTarget::QtOpenMovieFile
        | PpcImportDispatcherTarget::QtNewMovieFromFile
        | PpcImportDispatcherTarget::QtGetMovieBox
        | PpcImportDispatcherTarget::QtSetMovieBox
        | PpcImportDispatcherTarget::QtSetMovieGWorld
        | PpcImportDispatcherTarget::QtStartMovie
        | PpcImportDispatcherTarget::QtStopMovie
        | PpcImportDispatcherTarget::QtMoviesTask
        | PpcImportDispatcherTarget::QtDisposeMovie
        | PpcImportDispatcherTarget::QtIsMovieDone
        | PpcImportDispatcherTarget::QtGoToBeginningOfMovie
        | PpcImportDispatcherTarget::QtGoToEndOfMovie
        | PpcImportDispatcherTarget::QtGetMovieDuration
        | PpcImportDispatcherTarget::QtLoadMovieIntoRam
        | PpcImportDispatcherTarget::QtCloseMovieFile => {
            unreachable!("quicktime imports return through dispatch_quicktime_import")
        }
        PpcImportDispatcherTarget::InsTime
        | PpcImportDispatcherTarget::InsXTime
        | PpcImportDispatcherTarget::PrimeTime
        | PpcImportDispatcherTarget::RmvTime
        | PpcImportDispatcherTarget::VInstall
        | PpcImportDispatcherTarget::VRemove
        | PpcImportDispatcherTarget::SlotVInstall
        | PpcImportDispatcherTarget::DTInstall
        | PpcImportDispatcherTarget::DeferUserFn
        | PpcImportDispatcherTarget::SlotVRemove => {
            unreachable!("time and vbl imports return through dispatch_time_import")
        }
        PpcImportDispatcherTarget::DrawGrowIcon => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::GetPictInfo
        | PpcImportDispatcherTarget::DrawPicture
        | PpcImportDispatcherTarget::KillPicture => {
            unreachable!("picture imports return through dispatch_picture_import")
        }
        PpcImportDispatcherTarget::InitCursor
        | PpcImportDispatcherTarget::GetQDGlobalsArrow
        | PpcImportDispatcherTarget::HideCursor
        | PpcImportDispatcherTarget::ShowCursor
        | PpcImportDispatcherTarget::ShieldCursor
        | PpcImportDispatcherTarget::CrsrDevNextDevice
        | PpcImportDispatcherTarget::CrsrDevMoveTo
        | PpcImportDispatcherTarget::GetCursor
        | PpcImportDispatcherTarget::SetCursor
        | PpcImportDispatcherTarget::GetCCursor
        | PpcImportDispatcherTarget::SetCCursor
        | PpcImportDispatcherTarget::DisposeCCursor
        | PpcImportDispatcherTarget::GetCIcon
        | PpcImportDispatcherTarget::PlotCIcon
        | PpcImportDispatcherTarget::PlotIconID
        | PpcImportDispatcherTarget::PlotCIconHandle
        | PpcImportDispatcherTarget::PlotIconSuite
        | PpcImportDispatcherTarget::DisposeCIcon => {
            unreachable!("cursor and cicon imports return through dispatch_cursor_import")
        }
        PpcImportDispatcherTarget::FSClose
        | PpcImportDispatcherTarget::PBClose
        | PpcImportDispatcherTarget::PBFlushFile
        | PpcImportDispatcherTarget::FSRead
        | PpcImportDispatcherTarget::PBRead
        | PpcImportDispatcherTarget::FSWrite
        | PpcImportDispatcherTarget::PBWrite
        | PpcImportDispatcherTarget::GetEOF
        | PpcImportDispatcherTarget::PBGetEOF
        | PpcImportDispatcherTarget::SetEOF
        | PpcImportDispatcherTarget::AllocContig
        | PpcImportDispatcherTarget::PBSetEOF
        | PpcImportDispatcherTarget::GetFPos
        | PpcImportDispatcherTarget::SetFPos
        | PpcImportDispatcherTarget::PBSetFPos
        | PpcImportDispatcherTarget::PBCreate(_)
        | PpcImportDispatcherTarget::FSpCreate
        | PpcImportDispatcherTarget::HCreate
        | PpcImportDispatcherTarget::HRename
        | PpcImportDispatcherTarget::FSpRename
        | PpcImportDispatcherTarget::Rename
        | PpcImportDispatcherTarget::PBHRenameSync
        | PpcImportDispatcherTarget::PBRenameSync
        | PpcImportDispatcherTarget::Create
        | PpcImportDispatcherTarget::FSpDelete
        | PpcImportDispatcherTarget::DeleteByName(_)
        | PpcImportDispatcherTarget::FSOpen
        | PpcImportDispatcherTarget::FSpCreateResFile
        | PpcImportDispatcherTarget::HCreateResFile
        | PpcImportDispatcherTarget::FSpOpenResFile
        | PpcImportDispatcherTarget::FSpOpenDF
        | PpcImportDispatcherTarget::FSpOpenRF
        | PpcImportDispatcherTarget::HOpen
        | PpcImportDispatcherTarget::PBOpen
        | PpcImportDispatcherTarget::PBHOpenDF
        | PpcImportDispatcherTarget::PBHOpenDeny
        | PpcImportDispatcherTarget::CurResFile
        | PpcImportDispatcherTarget::UseResFile
        | PpcImportDispatcherTarget::OpenResFile
        | PpcImportDispatcherTarget::OpenRFPerm
        | PpcImportDispatcherTarget::HOpenResFile
        | PpcImportDispatcherTarget::ResError
        | PpcImportDispatcherTarget::GetVol
        | PpcImportDispatcherTarget::GetWDInfo
        | PpcImportDispatcherTarget::OpenWD
        | PpcImportDispatcherTarget::SetVol
        | PpcImportDispatcherTarget::HGetVol
        | PpcImportDispatcherTarget::HSetVol
        | PpcImportDispatcherTarget::FlushVol
        | PpcImportDispatcherTarget::PBFlushVol
        | PpcImportDispatcherTarget::PBHGetVInfo
        | PpcImportDispatcherTarget::PBGetVolumeInfo
        | PpcImportDispatcherTarget::FSGetVolumeInfo
        | PpcImportDispatcherTarget::GetVInfo
        | PpcImportDispatcherTarget::GetVRefNum
        | PpcImportDispatcherTarget::PBDTGetPath
        | PpcImportDispatcherTarget::PBDTGetAPPL
        | PpcImportDispatcherTarget::PBDTGetCommentSync
        | PpcImportDispatcherTarget::PBGetFInfo
        | PpcImportDispatcherTarget::PBHGetFInfo
        | PpcImportDispatcherTarget::PBSetFInfo
        | PpcImportDispatcherTarget::PBHSetFInfo
        | PpcImportDispatcherTarget::FSpGetFInfo
        | PpcImportDispatcherTarget::GetFInfo
        | PpcImportDispatcherTarget::GetFInfoCString
        | PpcImportDispatcherTarget::SetFInfo { .. }
        | PpcImportDispatcherTarget::HGetFInfo
        | PpcImportDispatcherTarget::FSpSetFInfo
        | PpcImportDispatcherTarget::HSetFInfo
        | PpcImportDispatcherTarget::PBResolveFileIDRef
        | PpcImportDispatcherTarget::PBGetCatInfo
        | PpcImportDispatcherTarget::PBSetCatInfo
        | PpcImportDispatcherTarget::DirCreate
        | PpcImportDispatcherTarget::FSpDirCreate
        | PpcImportDispatcherTarget::FSMakeFSSpec
        | PpcImportDispatcherTarget::PBGetFCBInfo
        | PpcImportDispatcherTarget::FindFolder
        | PpcImportDispatcherTarget::ResolveAliasFile
        | PpcImportDispatcherTarget::ResolveAliasFileWithMountFlags
        | PpcImportDispatcherTarget::GetIconRefFromFile
        | PpcImportDispatcherTarget::GetIconRef
        | PpcImportDispatcherTarget::PlotIconRef
        | PpcImportDispatcherTarget::ReleaseIconRef
        | PpcImportDispatcherTarget::ResolveAlias
        | PpcImportDispatcherTarget::FollowFinderAlias
        | PpcImportDispatcherTarget::MatchAlias
        | PpcImportDispatcherTarget::UpdateAlias
        | PpcImportDispatcherTarget::NewAlias
        | PpcImportDispatcherTarget::NewAliasMinimal
        | PpcImportDispatcherTarget::NewAliasMinimalFromFullPath
        | PpcImportDispatcherTarget::FileCompatibility(_)
        | PpcImportDispatcherTarget::FileManager(_)
        | PpcImportDispatcherTarget::FsRef(_) => {
            unreachable!("file imports return through dispatch_file_import")
        }
        PpcImportDispatcherTarget::SetResLoad
        | PpcImportDispatcherTarget::LMGetResLoad
        | PpcImportDispatcherTarget::LoadResource
        | PpcImportDispatcherTarget::GetResource
        | PpcImportDispatcherTarget::Get1Resource
        | PpcImportDispatcherTarget::GetNamedResource
        | PpcImportDispatcherTarget::Get1NamedResource
        | PpcImportDispatcherTarget::GetIndResource
        | PpcImportDispatcherTarget::Get1IndResource
        | PpcImportDispatcherTarget::CountResources
        | PpcImportDispatcherTarget::Count1Resources
        | PpcImportDispatcherTarget::CountTypes
        | PpcImportDispatcherTarget::Count1Types
        | PpcImportDispatcherTarget::GetIndType
        | PpcImportDispatcherTarget::Get1IndType
        | PpcImportDispatcherTarget::UniqueID
        | PpcImportDispatcherTarget::Unique1ID
        | PpcImportDispatcherTarget::ReleaseResource
        | PpcImportDispatcherTarget::DetachResource
        | PpcImportDispatcherTarget::GetIndString
        | PpcImportDispatcherTarget::GetString
        | PpcImportDispatcherTarget::GetResAttrs
        | PpcImportDispatcherTarget::SetResAttrs
        | PpcImportDispatcherTarget::GetResInfo
        | PpcImportDispatcherTarget::GetResourceSizeOnDisk
        | PpcImportDispatcherTarget::SetResInfo
        | PpcImportDispatcherTarget::HomeResFile
        | PpcImportDispatcherTarget::UpdateResFile
        | PpcImportDispatcherTarget::AddResource
        | PpcImportDispatcherTarget::ChangedResource
        | PpcImportDispatcherTarget::WriteResource
        | PpcImportDispatcherTarget::RemoveResource
        | PpcImportDispatcherTarget::ReadPartialResource
        | PpcImportDispatcherTarget::WritePartialResource
        | PpcImportDispatcherTarget::SetResourceSize
        | PpcImportDispatcherTarget::CloseResFile
        | PpcImportDispatcherTarget::GetPicture
        | PpcImportDispatcherTarget::GetIconSuite
        | PpcImportDispatcherTarget::DisposeIconSuite
        | PpcImportDispatcherTarget::GetIcon
        | PpcImportDispatcherTarget::GetPattern
        | PpcImportDispatcherTarget::GetIndPattern
        | PpcImportDispatcherTarget::GetPixPat
        | PpcImportDispatcherTarget::GetIntlResource => {
            unreachable!("resource imports return through dispatch_resource_import")
        }
        PpcImportDispatcherTarget::InitGraf
        | PpcImportDispatcherTarget::GetQDGlobalsScreenBits
        | PpcImportDispatcherTarget::GetQDGlobalsWhite
        | PpcImportDispatcherTarget::GetQDGlobalsGray
        | PpcImportDispatcherTarget::ApplyThemeBackground
        | PpcImportDispatcherTarget::DrawThemeEditTextFrame
        | PpcImportDispatcherTarget::DrawThemeFocusRect
        | PpcImportDispatcherTarget::GetThemeDrawingState
        | PpcImportDispatcherTarget::GetForeColor
        | PpcImportDispatcherTarget::GetBackColor
        | PpcImportDispatcherTarget::GetGray
        | PpcImportDispatcherTarget::ForeColor
        | PpcImportDispatcherTarget::BackColor
        | PpcImportDispatcherTarget::SetThemePen
        | PpcImportDispatcherTarget::RGBForeColor
        | PpcImportDispatcherTarget::SaveFore
        | PpcImportDispatcherTarget::SaveBack
        | PpcImportDispatcherTarget::RestoreFore
        | PpcImportDispatcherTarget::RestoreBack
        | PpcImportDispatcherTarget::RGBBackColor
        | PpcImportDispatcherTarget::OpColor
        | PpcImportDispatcherTarget::HiliteColor
        | PpcImportDispatcherTarget::PmForeColor
        | PpcImportDispatcherTarget::PmBackColor
        | PpcImportDispatcherTarget::Color2Index
        | PpcImportDispatcherTarget::Index2Color
        | PpcImportDispatcherTarget::RGB2HSL
        | PpcImportDispatcherTarget::RGB2HSV
        | PpcImportDispatcherTarget::HSV2RGB
        | PpcImportDispatcherTarget::SetRect
        | PpcImportDispatcherTarget::Pt2Rect
        | PpcImportDispatcherTarget::SectRect
        | PpcImportDispatcherTarget::UnionRect
        | PpcImportDispatcherTarget::EqualRect
        | PpcImportDispatcherTarget::EmptyRect
        | PpcImportDispatcherTarget::SetPt
        | PpcImportDispatcherTarget::EqualPt
        | PpcImportDispatcherTarget::AddPt
        | PpcImportDispatcherTarget::SubPt
        | PpcImportDispatcherTarget::LocalToGlobal
        | PpcImportDispatcherTarget::GlobalToLocal
        | PpcImportDispatcherTarget::PtInRect
        | PpcImportDispatcherTarget::OffsetRect
        | PpcImportDispatcherTarget::MapRect
        | PpcImportDispatcherTarget::MapRgn
        | PpcImportDispatcherTarget::MapPt
        | PpcImportDispatcherTarget::InsetRect => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::MathCeil
        | PpcImportDispatcherTarget::MathSqrt
        | PpcImportDispatcherTarget::MathHypot
        | PpcImportDispatcherTarget::MathExp
        | PpcImportDispatcherTarget::MathSin
        | PpcImportDispatcherTarget::MathCos
        | PpcImportDispatcherTarget::MathRound
        | PpcImportDispatcherTarget::MathRint
        | PpcImportDispatcherTarget::MathAsin
        | PpcImportDispatcherTarget::MathAcos
        | PpcImportDispatcherTarget::MathTan
        | PpcImportDispatcherTarget::MathAtan
        | PpcImportDispatcherTarget::MathAtan2
        | PpcImportDispatcherTarget::MathPow
        | PpcImportDispatcherTarget::MathFmod
        | PpcImportDispatcherTarget::MathLog
        | PpcImportDispatcherTarget::MathLog10
        | PpcImportDispatcherTarget::MathDtox80
        | PpcImportDispatcherTarget::X2Fix
        | PpcImportDispatcherTarget::FixRatio
        | PpcImportDispatcherTarget::FixMul
        | PpcImportDispatcherTarget::FixDiv
        | PpcImportDispatcherTarget::Long2Fix
        | PpcImportDispatcherTarget::Fix2Long
        | PpcImportDispatcherTarget::FixRound
        | PpcImportDispatcherTarget::AngleFromSlope
        | PpcImportDispatcherTarget::Fix2Frac
        | PpcImportDispatcherTarget::Frac2Fix
        | PpcImportDispatcherTarget::Frac2X
        | PpcImportDispatcherTarget::X2Frac
        | PpcImportDispatcherTarget::FracSin
        | PpcImportDispatcherTarget::FracCos
        | PpcImportDispatcherTarget::FracSqrt
        | PpcImportDispatcherTarget::FracMul
        | PpcImportDispatcherTarget::FracDiv
        | PpcImportDispatcherTarget::FixATan2
        | PpcImportDispatcherTarget::WideAdd
        | PpcImportDispatcherTarget::WideSubtract
        | PpcImportDispatcherTarget::WideNegate
        | PpcImportDispatcherTarget::WideShift
        | PpcImportDispatcherTarget::WideBitShift
        | PpcImportDispatcherTarget::WideMultiply
        | PpcImportDispatcherTarget::WideDivide
        | PpcImportDispatcherTarget::WideWideDivide
        | PpcImportDispatcherTarget::WideCompare
        | PpcImportDispatcherTarget::WideSquareRoot => {
            unreachable!("math imports return through dispatch_math_import")
        }
        PpcImportDispatcherTarget::MoveTo
        | PpcImportDispatcherTarget::MovePortTo
        | PpcImportDispatcherTarget::PortSize
        | PpcImportDispatcherTarget::Move
        | PpcImportDispatcherTarget::LineTo
        | PpcImportDispatcherTarget::Line => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::DrawChar
        | PpcImportDispatcherTarget::DrawText
        | PpcImportDispatcherTarget::DrawString
        | PpcImportDispatcherTarget::TextFont
        | PpcImportDispatcherTarget::TextFace
        | PpcImportDispatcherTarget::TextMode
        | PpcImportDispatcherTarget::TextSize
        | PpcImportDispatcherTarget::CharExtra => {
            unreachable!("font and text imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::PaintRect
        | PpcImportDispatcherTarget::EraseRect
        | PpcImportDispatcherTarget::InvertRect
        | PpcImportDispatcherTarget::FrameRect
        | PpcImportDispatcherTarget::FillRect
        | PpcImportDispatcherTarget::FillCRect => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameOval
        | PpcImportDispatcherTarget::PaintOval
        | PpcImportDispatcherTarget::EraseOval
        | PpcImportDispatcherTarget::PaintArc
        | PpcImportDispatcherTarget::QuickDrawShape(_) => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameRgn
        | PpcImportDispatcherTarget::PaintRgn
        | PpcImportDispatcherTarget::EraseRgn
        | PpcImportDispatcherTarget::FillRgn
        | PpcImportDispatcherTarget::FillCRgn
        | PpcImportDispatcherTarget::InvertRgn => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameRoundRect
        | PpcImportDispatcherTarget::PaintRoundRect
        | PpcImportDispatcherTarget::EraseRoundRect
        | PpcImportDispatcherTarget::FillRoundRect => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::InvalRect
        | PpcImportDispatcherTarget::InvalRgn
        | PpcImportDispatcherTarget::ValidRect
        | PpcImportDispatcherTarget::ValidRgn
        | PpcImportDispatcherTarget::BeginUpdate
        | PpcImportDispatcherTarget::EndUpdate => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::ClipRect
        | PpcImportDispatcherTarget::GetClip
        | PpcImportDispatcherTarget::GetPortVisibleRegion
        | PpcImportDispatcherTarget::GetPortClipRegion
        | PpcImportDispatcherTarget::SetPortClipRegion
        | PpcImportDispatcherTarget::SetClip
        | PpcImportDispatcherTarget::BitMapToRegion
        | PpcImportDispatcherTarget::NewRgn
        | PpcImportDispatcherTarget::DisposeRgn
        | PpcImportDispatcherTarget::CopyRgn
        | PpcImportDispatcherTarget::OpenRgn
        | PpcImportDispatcherTarget::CloseRgn
        | PpcImportDispatcherTarget::SectRgn
        | PpcImportDispatcherTarget::UnionRgn
        | PpcImportDispatcherTarget::DiffRgn
        | PpcImportDispatcherTarget::XorRgn
        | PpcImportDispatcherTarget::SetEmptyRgn
        | PpcImportDispatcherTarget::SetRectRgn
        | PpcImportDispatcherTarget::RectRgn
        | PpcImportDispatcherTarget::OffsetRgn
        | PpcImportDispatcherTarget::InsetRgn
        | PpcImportDispatcherTarget::EmptyRgn
        | PpcImportDispatcherTarget::EqualRgn
        | PpcImportDispatcherTarget::PtInRgn
        | PpcImportDispatcherTarget::RectInRgn => {
            unreachable!("Region Manager imports return through dispatch_region_import")
        }
        PpcImportDispatcherTarget::GetPen
        | PpcImportDispatcherTarget::HidePen
        | PpcImportDispatcherTarget::ShowPen
        | PpcImportDispatcherTarget::PenSize
        | PpcImportDispatcherTarget::PenMode
        | PpcImportDispatcherTarget::PenNormal
        | PpcImportDispatcherTarget::PenPixPat
        | PpcImportDispatcherTarget::GetPenState
        | PpcImportDispatcherTarget::SetPenState => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::CopyBits
        | PpcImportDispatcherTarget::StdBits
        | PpcImportDispatcherTarget::UnpackBits => {
            unreachable!("bit-transfer imports return through dispatch_bit_transfer_import")
        }
        PpcImportDispatcherTarget::OpenPoly
        | PpcImportDispatcherTarget::ClosePoly
        | PpcImportDispatcherTarget::KillPoly
        | PpcImportDispatcherTarget::FramePoly
        | PpcImportDispatcherTarget::PaintPoly
        | PpcImportDispatcherTarget::FillPoly
        | PpcImportDispatcherTarget::ErasePoly => {
            unreachable!("Polygon Manager imports return through dispatch_polygon_import")
        }
        PpcImportDispatcherTarget::QtBeginFullScreen
        | PpcImportDispatcherTarget::QtEndFullScreen
        | PpcImportDispatcherTarget::NewCWindow
        | PpcImportDispatcherTarget::GetNewCWindow
        | PpcImportDispatcherTarget::GetWRefCon
        | PpcImportDispatcherTarget::SetWRefCon
        | PpcImportDispatcherTarget::GetWindowPic
        | PpcImportDispatcherTarget::SetWindowPic
        | PpcImportDispatcherTarget::GetAuxWin
        | PpcImportDispatcherTarget::LMGetWindowList
        | PpcImportDispatcherTarget::LMSetWindowList
        | PpcImportDispatcherTarget::LMGetAuxWinHead
        | PpcImportDispatcherTarget::LMSetAuxWinHead
        | PpcImportDispatcherTarget::SizeWindow
        | PpcImportDispatcherTarget::MoveWindow => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::ShowWindow
        | PpcImportDispatcherTarget::HideWindow
        | PpcImportDispatcherTarget::ShowHide
        | PpcImportDispatcherTarget::CloseWindow => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::CloseDialog | PpcImportDispatcherTarget::DisposeDialog => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::FrontWindow
        | PpcImportDispatcherTarget::SetWinColor
        | PpcImportDispatcherTarget::PaintOne
        | PpcImportDispatcherTarget::PaintBehind
        | PpcImportDispatcherTarget::CalcVisBehind => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::SelectWindow => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::InitPalettes
        | PpcImportDispatcherTarget::ActivatePalette
        | PpcImportDispatcherTarget::NSetPalette
        | PpcImportDispatcherTarget::GetPalette => {
            unreachable!("palette imports return through dispatch_palette_import")
        }
        PpcImportDispatcherTarget::GetPort
        | PpcImportDispatcherTarget::GetQDGlobalsThePort
        | PpcImportDispatcherTarget::GetPortPixMap
        | PpcImportDispatcherTarget::GetPortBounds
        | PpcImportDispatcherTarget::GetPortBitMapForCopyBits
        | PpcImportDispatcherTarget::SetPort
        | PpcImportDispatcherTarget::GetWindowPort
        | PpcImportDispatcherTarget::SetPortWindowPort
        | PpcImportDispatcherTarget::SetPortDialogPort
        | PpcImportDispatcherTarget::NewGWorld
        | PpcImportDispatcherTarget::UpdateGWorld
        | PpcImportDispatcherTarget::DisposeGWorld
        | PpcImportDispatcherTarget::QDError
        | PpcImportDispatcherTarget::QDDone
        | PpcImportDispatcherTarget::GetGWorld
        | PpcImportDispatcherTarget::SetGWorld
        | PpcImportDispatcherTarget::GetGWorldDevice
        | PpcImportDispatcherTarget::GetGWorldPixMap
        | PpcImportDispatcherTarget::OpenPort
        | PpcImportDispatcherTarget::OpenCPort
        | PpcImportDispatcherTarget::CreateNewPort
        | PpcImportDispatcherTarget::InitCPort
        | PpcImportDispatcherTarget::CloseCPort
        | PpcImportDispatcherTarget::SetPortBits { .. }
        | PpcImportDispatcherTarget::PixMap32Bit
        | PpcImportDispatcherTarget::GetPixBaseAddr
        | PpcImportDispatcherTarget::GetPixRowBytes
        | PpcImportDispatcherTarget::LockPixels
        | PpcImportDispatcherTarget::UnlockPixels
        | PpcImportDispatcherTarget::GetPixelsState
        | PpcImportDispatcherTarget::SetPixelsState
        | PpcImportDispatcherTarget::AllowPurgePixels
        | PpcImportDispatcherTarget::NoPurgePixels
        | PpcImportDispatcherTarget::SetOrigin => {
            unreachable!("GWorld imports return through dispatch_gworld_import")
        }
        PpcImportDispatcherTarget::GetWMgrPort => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::GetGDevice
        | PpcImportDispatcherTarget::SetGDevice
        | PpcImportDispatcherTarget::GetDeviceList
        | PpcImportDispatcherTarget::GetMainDevice
        | PpcImportDispatcherTarget::GetMaxDevice
        | PpcImportDispatcherTarget::DeviceLoop
        | PpcImportDispatcherTarget::GetNextDevice
        | PpcImportDispatcherTarget::TestDeviceAttribute
        | PpcImportDispatcherTarget::SetDeviceAttribute
        | PpcImportDispatcherTarget::HasDepth
        | PpcImportDispatcherTarget::SetDepth => {
            unreachable!("graphics-device imports return through dispatch_graphics_device_import")
        }
        PpcImportDispatcherTarget::GetSysFont
        | PpcImportDispatcherTarget::GetAppFont
        | PpcImportDispatcherTarget::GetDefFontSize
        | PpcImportDispatcherTarget::GetFontName => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::DMGetDisplayIDByGDevice
        | PpcImportDispatcherTarget::DMGetNameByAVID
        | PpcImportDispatcherTarget::DMGetGDeviceByDisplayID => {
            unreachable!("display manager imports return through dispatch_display_import")
        }
        PpcImportDispatcherTarget::GetCTable
        | PpcImportDispatcherTarget::GetCTSeed
        | PpcImportDispatcherTarget::MakeITable
        | PpcImportDispatcherTarget::CTabChanged
        | PpcImportDispatcherTarget::ProtectEntry
        | PpcImportDispatcherTarget::ReserveEntry
        | PpcImportDispatcherTarget::SaveEntries
        | PpcImportDispatcherTarget::RestoreEntries
        | PpcImportDispatcherTarget::SetEntries
        | PpcImportDispatcherTarget::RestoreDeviceClut
        | PpcImportDispatcherTarget::DisposeCTable
        | PpcImportDispatcherTarget::NewPixMap
        | PpcImportDispatcherTarget::DisposePixMap => {
            unreachable!("color-table imports return through dispatch_color_table_import")
        }
        PpcImportDispatcherTarget::FindWindow
        | PpcImportDispatcherTarget::PinRect
        | PpcImportDispatcherTarget::GetWVariant
        | PpcImportDispatcherTarget::ClipAbove
        | PpcImportDispatcherTarget::SaveOld
        | PpcImportDispatcherTarget::DrawNew
        | PpcImportDispatcherTarget::DragGrayRgn => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::GetDCtlEntry
        | PpcImportDispatcherTarget::GetADBInfo
        | PpcImportDispatcherTarget::AutoSleepControl
        | PpcImportDispatcherTarget::IsAutoSlpControlDisabled
        | PpcImportDispatcherTarget::OpenDriver
        | PpcImportDispatcherTarget::Status
        | PpcImportDispatcherTarget::Control
        | PpcImportDispatcherTarget::PBControl
        | PpcImportDispatcherTarget::PBStatus => {
            unreachable!("Device Manager imports return through dispatch_device_import")
        }
        PpcImportDispatcherTarget::Gestalt
        | PpcImportDispatcherTarget::NewGestaltValue
        | PpcImportDispatcherTarget::ReplaceGestaltValue => {
            unreachable!("Gestalt imports return through dispatch_gestalt_import")
        }
        PpcImportDispatcherTarget::GetSharedLibrary
        | PpcImportDispatcherTarget::FindSymbol
        | PpcImportDispatcherTarget::FragGetConnectionInfo
        | PpcImportDispatcherTarget::FragGetSectionInfo
        | PpcImportDispatcherTarget::CountSymbols
        | PpcImportDispatcherTarget::GetIndSymbol
        | PpcImportDispatcherTarget::CloseConnection
        | PpcImportDispatcherTarget::GetMemFragment
        | PpcImportDispatcherTarget::GetDiskFragment => {
            unreachable!("cfm imports return through dispatch_cfm_import")
        }
        PpcImportDispatcherTarget::StandardGetFile => {
            unreachable!("standard file imports return through dispatch_standard_file_import")
        }
        PpcImportDispatcherTarget::GetScrap
        | PpcImportDispatcherTarget::InfoScrap
        | PpcImportDispatcherTarget::PutScrap
        | PpcImportDispatcherTarget::ZeroScrap
        | PpcImportDispatcherTarget::LoadScrap
        | PpcImportDispatcherTarget::UnloadScrap
        | PpcImportDispatcherTarget::GetCurrentScrap
        | PpcImportDispatcherTarget::GetScrapFlavorSize
        | PpcImportDispatcherTarget::GetScrapFlavorData
        | PpcImportDispatcherTarget::PutScrapFlavor => {
            unreachable!("Scrap Manager imports return through dispatch_scrap_import")
        }
        PpcImportDispatcherTarget::DMGetFirstScreenDevice
        | PpcImportDispatcherTarget::DMGetNextScreenDevice
        | PpcImportDispatcherTarget::DMGetDisplayMode
        | PpcImportDispatcherTarget::DMCheckDisplayMode
        | PpcImportDispatcherTarget::DMSetDisplayMode
        | PpcImportDispatcherTarget::DMNewDisplayModeList
        | PpcImportDispatcherTarget::DMGetIndexedDisplayModeFromList
        | PpcImportDispatcherTarget::DMDisposeList
        | PpcImportDispatcherTarget::DMBeginConfigureDisplays
        | PpcImportDispatcherTarget::DMEndConfigureDisplays => {
            unreachable!("display manager imports return through dispatch_display_import")
        }
        PpcImportDispatcherTarget::GetNewDialog
        | PpcImportDispatcherTarget::NewDialog
        | PpcImportDispatcherTarget::NewFeaturesDialog
        | PpcImportDispatcherTarget::GetDialogItem
        | PpcImportDispatcherTarget::GetDialogItemAsControl
        | PpcImportDispatcherTarget::SetDialogItem
        | PpcImportDispatcherTarget::GetDialogItemText
        | PpcImportDispatcherTarget::SetDialogItemText
        | PpcImportDispatcherTarget::SetDialogDefaultItem
        | PpcImportDispatcherTarget::GetDialogDefaultItem
        | PpcImportDispatcherTarget::SetDialogCancelItem
        | PpcImportDispatcherTarget::GetDialogCancelItem
        | PpcImportDispatcherTarget::SetDialogTracksCursor
        | PpcImportDispatcherTarget::MoveDialogItem
        | PpcImportDispatcherTarget::SizeDialogItem
        | PpcImportDispatcherTarget::AppendDialogItemList
        | PpcImportDispatcherTarget::AutoSizeDialog
        | PpcImportDispatcherTarget::CouldDialog
        | PpcImportDispatcherTarget::FreeDialog
        | PpcImportDispatcherTarget::CouldAlert
        | PpcImportDispatcherTarget::FreeAlert
        | PpcImportDispatcherTarget::StdFilterProc
        | PpcImportDispatcherTarget::GetStdFilterProc
        | PpcImportDispatcherTarget::GetAlertStage
        | PpcImportDispatcherTarget::SetDialogFont
        | PpcImportDispatcherTarget::GetDialogPort
        | PpcImportDispatcherTarget::GetDialogWindow
        | PpcImportDispatcherTarget::GetDialogFromWindow
        | PpcImportDispatcherTarget::DrawDialog
        | PpcImportDispatcherTarget::ModalDialog => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::DrawControls | PpcImportDispatcherTarget::UpdateControls => {
            unreachable!("control imports return through dispatch_control_import")
        }
        PpcImportDispatcherTarget::SetControlTitle
        | PpcImportDispatcherTarget::SetControlValue
        | PpcImportDispatcherTarget::HiliteControl => {
            unreachable!("control imports return through dispatch_control_import")
        }
        PpcImportDispatcherTarget::InitFonts => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::InitWindows => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::MeasureText | PpcImportDispatcherTarget::RealFont => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::SelectDialogItemText
        | PpcImportDispatcherTarget::InitDialogs
        | PpcImportDispatcherTarget::ErrorSound => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::SystemTask
        | PpcImportDispatcherTarget::SystemClick
        | PpcImportDispatcherTarget::OpenDeskAcc
        | PpcImportDispatcherTarget::CloseDeskAcc => {
            unreachable!("Desk Manager imports return through dispatch_desk_import")
        }
        PpcImportDispatcherTarget::AEInstallEventHandler
        | PpcImportDispatcherTarget::AERemoveEventHandler
        | PpcImportDispatcherTarget::AEInstallSpecialHandler
        | PpcImportDispatcherTarget::AEProcessAppleEvent => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::LNew
        | PpcImportDispatcherTarget::LDispose
        | PpcImportDispatcherTarget::LAddRow
        | PpcImportDispatcherTarget::LDelRow
        | PpcImportDispatcherTarget::LGetSelect
        | PpcImportDispatcherTarget::LNextCell
        | PpcImportDispatcherTarget::LSetSelect
        | PpcImportDispatcherTarget::LSetCell
        | PpcImportDispatcherTarget::LAddToCell
        | PpcImportDispatcherTarget::LDraw
        | PpcImportDispatcherTarget::LGetCell
        | PpcImportDispatcherTarget::LClick
        | PpcImportDispatcherTarget::LActivate
        | PpcImportDispatcherTarget::LSetDrawingMode
        | PpcImportDispatcherTarget::LScroll
        | PpcImportDispatcherTarget::LSize
        | PpcImportDispatcherTarget::LGetCellDataLocation
        | PpcImportDispatcherTarget::LAddColumn
        | PpcImportDispatcherTarget::LCellSize
        | PpcImportDispatcherTarget::LRect
        | PpcImportDispatcherTarget::LUpdate
        | PpcImportDispatcherTarget::LAutoScroll
        | PpcImportDispatcherTarget::LSearch => {
            unreachable!("list imports return through dispatch_list_import")
        }
        PpcImportDispatcherTarget::SysEnvirons
        | PpcImportDispatcherTarget::SVersion
        | PpcImportDispatcherTarget::EqualString
        | PpcImportDispatcherTarget::RelString
        | PpcImportDispatcherTarget::InitEditionPackVersion
        | PpcImportDispatcherTarget::StuffHex
        | PpcImportDispatcherTarget::GetIntlResourceTable
        | PpcImportDispatcherTarget::NumToString
        | PpcImportDispatcherTarget::NumToCString
        | PpcImportDispatcherTarget::StringToNum
        | PpcImportDispatcherTarget::Random
        | PpcImportDispatcherTarget::SetQDGlobalsRandomSeed
        | PpcImportDispatcherTarget::GetQDGlobalsRandomSeed
        | PpcImportDispatcherTarget::BitAnd
        | PpcImportDispatcherTarget::BitOr
        | PpcImportDispatcherTarget::BitTst => {
            unreachable!("Toolbox Utilities imports return through dispatch_toolbox_import")
        }
        PpcImportDispatcherTarget::IUEqualPString => None,
        PpcImportDispatcherTarget::TextWidth
        | PpcImportDispatcherTarget::TruncString
        | PpcImportDispatcherTarget::StringWidth
        | PpcImportDispatcherTarget::CharWidth
        | PpcImportDispatcherTarget::GetFontInfo
        | PpcImportDispatcherTarget::StdText
        | PpcImportDispatcherTarget::StdTxMeas
        | PpcImportDispatcherTarget::IsOutline
        | PpcImportDispatcherTarget::OutlineMetrics
        | PpcImportDispatcherTarget::FontMetrics
        | PpcImportDispatcherTarget::FMSwapFont
        | PpcImportDispatcherTarget::GetFNum
        | PpcImportDispatcherTarget::FontToScript
        | PpcImportDispatcherTarget::SetPreserveGlyph
        | PpcImportDispatcherTarget::GetPreserveGlyph
        | PpcImportDispatcherTarget::SetOutlinePreferred
        | PpcImportDispatcherTarget::GetOutlinePreferred
        | PpcImportDispatcherTarget::IsMetric
        | PpcImportDispatcherTarget::VisibleLength => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::AESetInteractionAllowed
        | PpcImportDispatcherTarget::AEGetInteractionAllowed
        | PpcImportDispatcherTarget::AEInteractWithUser
        | PpcImportDispatcherTarget::AEManagerInfo => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::StdMemset
        | PpcImportDispatcherTarget::StdMemcmp
        | PpcImportDispatcherTarget::StdMemcpy
        | PpcImportDispatcherTarget::StdSetJmp
        | PpcImportDispatcherTarget::StdLongJmp
        | PpcImportDispatcherTarget::StdMemmove => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::StdMalloc
        | PpcImportDispatcherTarget::StdFree
        | PpcImportDispatcherTarget::StdCalloc
        | PpcImportDispatcherTarget::StdRealloc
        | PpcImportDispatcherTarget::StdStrcpy
        | PpcImportDispatcherTarget::StdStrncpy
        | PpcImportDispatcherTarget::StdStrcat
        | PpcImportDispatcherTarget::StdStrncat
        | PpcImportDispatcherTarget::StdStrcmp
        | PpcImportDispatcherTarget::StdStrncmp
        | PpcImportDispatcherTarget::StdStrlen
        | PpcImportDispatcherTarget::StdMemchr
        | PpcImportDispatcherTarget::StdStrchr
        | PpcImportDispatcherTarget::StdStrrchr
        | PpcImportDispatcherTarget::StdStrspn
        | PpcImportDispatcherTarget::StdStrcspn
        | PpcImportDispatcherTarget::StdStrpbrk
        | PpcImportDispatcherTarget::StdStrstr
        | PpcImportDispatcherTarget::StdAtoi
        | PpcImportDispatcherTarget::StdAtof
        | PpcImportDispatcherTarget::StdGetenv
        | PpcImportDispatcherTarget::StdSprintf => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::StdDescriptor(_) => {
            unreachable!("handled before context split")
        }
        PpcImportDispatcherTarget::StdIoCompatibility(operation) => {
            Some(ppc_dispatch_process_stdio_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                files,
                writable_refnums,
                vfs_files,
                next_file_ref_num,
                stdio_streams,
            ))
        }
        PpcImportDispatcherTarget::StdAbs
        | PpcImportDispatcherTarget::StdToupper
        | PpcImportDispatcherTarget::StdTolower
        | PpcImportDispatcherTarget::StdIsalnum
        | PpcImportDispatcherTarget::StdIsalpha
        | PpcImportDispatcherTarget::StdIsascii
        | PpcImportDispatcherTarget::StdIscntrl
        | PpcImportDispatcherTarget::StdIsdigit
        | PpcImportDispatcherTarget::StdIsgraph
        | PpcImportDispatcherTarget::StdIslower
        | PpcImportDispatcherTarget::StdIsprint
        | PpcImportDispatcherTarget::StdIspunct
        | PpcImportDispatcherTarget::StdIsspace
        | PpcImportDispatcherTarget::StdIsupper
        | PpcImportDispatcherTarget::StdIsxdigit
        | PpcImportDispatcherTarget::StdToascii
        | PpcImportDispatcherTarget::StdSrand
        | PpcImportDispatcherTarget::StdRand
        | PpcImportDispatcherTarget::StdTime
        | PpcImportDispatcherTarget::StdLocaltime
        | PpcImportDispatcherTarget::P2CStr
        | PpcImportDispatcherTarget::P2CStrCpy
        | PpcImportDispatcherTarget::C2PStrCpy
        | PpcImportDispatcherTarget::C2PStr
        | PpcImportDispatcherTarget::CopyCStringToPascal
        | PpcImportDispatcherTarget::CopyPascalStringToC
        | PpcImportDispatcherTarget::LowerText
        | PpcImportDispatcherTarget::StripText
        | PpcImportDispatcherTarget::UpperText
        | PpcImportDispatcherTarget::StripUpperText => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::CfStringMakeConstantString
        | PpcImportDispatcherTarget::CfStringCreateWithCString
        | PpcImportDispatcherTarget::CfStringCreateWithPascalString
        | PpcImportDispatcherTarget::CfStringCreateWithBytes
        | PpcImportDispatcherTarget::CfStringGetCString
        | PpcImportDispatcherTarget::CfStringGetBytes
        | PpcImportDispatcherTarget::CfStringGetLength
        | PpcImportDispatcherTarget::CfStringGetSystemEncoding
        | PpcImportDispatcherTarget::CfRetain
        | PpcImportDispatcherTarget::CfRelease
        | PpcImportDispatcherTarget::CfGetRetainCount
        | PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier
        | PpcImportDispatcherTarget::CfBundleGetMainBundle
        | PpcImportDispatcherTarget::CfBundleCopyPrivateFrameworksUrl
        | PpcImportDispatcherTarget::CfUrlCreateCopyAppendingPathComponent
        | PpcImportDispatcherTarget::CfBundleCreate
        | PpcImportDispatcherTarget::CfBundleCopyBundleUrl
        | PpcImportDispatcherTarget::CfBundleCopyResourceUrl
        | PpcImportDispatcherTarget::CfBundleGetFunctionPointerForName
        | PpcImportDispatcherTarget::CfUrlGetFsRef
        | PpcImportDispatcherTarget::CfUrlCreateFromFsRef
        | PpcImportDispatcherTarget::CfStringCompare
        | PpcImportDispatcherTarget::CfStringCreateWithCharacters
        | PpcImportDispatcherTarget::CfBooleanGetValue
        | PpcImportDispatcherTarget::CfBundleLoadExecutable
        | PpcImportDispatcherTarget::CfArrayGetCount
        | PpcImportDispatcherTarget::CfArrayGetValueAtIndex
        | PpcImportDispatcherTarget::CfDictionaryGetValue
        | PpcImportDispatcherTarget::CfNumberGetValue
        | PpcImportDispatcherTarget::CfUrlCreateWithFileSystemPath
        | PpcImportDispatcherTarget::CfUrlCreateWithString
        | PpcImportDispatcherTarget::CfUrlCopyFileSystemPath => {
            unreachable!("Core Foundation imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::ICStart
        | PpcImportDispatcherTarget::ICStop
        | PpcImportDispatcherTarget::ICGetSeed
        | PpcImportDispatcherTarget::ICGetComponentInstance
        | PpcImportDispatcherTarget::ICGetPref
        | PpcImportDispatcherTarget::ICCountMapEntries
        | PpcImportDispatcherTarget::ICGetIndMapEntry
        | PpcImportDispatcherTarget::ICGetMapEntry
        | PpcImportDispatcherTarget::ICFindPrefHandle
        | PpcImportDispatcherTarget::ICSetPref
        | PpcImportDispatcherTarget::ICBegin
        | PpcImportDispatcherTarget::ICEnd
        | PpcImportDispatcherTarget::ICFindConfigFile
        | PpcImportDispatcherTarget::ICParseURL => {
            dispatch_internet_config::dispatch_internet_config_import(
                binding,
                cpu,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                toolbox_startup,
                vfs_files,
                vfs_resource_files,
                vfs_directories,
                default_dir_id,
                vfs_resources,
            )
        }
        PpcImportDispatcherTarget::GetCurrentProcess
        | PpcImportDispatcherTarget::WakeUpProcess
        | PpcImportDispatcherTarget::SameProcess
        | PpcImportDispatcherTarget::GetProcessInformation
        | PpcImportDispatcherTarget::GetProcessBundleLocation => {
            unreachable!("Process Manager imports return through dispatch_process_import")
        }
        PpcImportDispatcherTarget::ParamText
        | PpcImportDispatcherTarget::AlertReturnDefault(_)
        | PpcImportDispatcherTarget::StandardAlert => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::Q3Initialize
        | PpcImportDispatcherTarget::Q3Exit
        | PpcImportDispatcherTarget::Q3GetVersion => {
            unreachable!("QuickDraw 3D core imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3MemoryStorageNew
        | PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer
        | PpcImportDispatcherTarget::Q3FSSpecStorageNew => {
            unreachable!("QuickDraw 3D storage imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3NewObject => {
            unreachable!("QuickDraw 3D core imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3FileNew => {
            unreachable!("QuickDraw 3D file imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ViewNew
        | PpcImportDispatcherTarget::Q3DisplayGroupNew
        | PpcImportDispatcherTarget::Q3OrderedDisplayGroupNew => {
            unreachable!("QuickDraw 3D group/view imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ErrorGet => {
            unreachable!("QuickDraw 3D core imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ObjectDispose
        | PpcImportDispatcherTarget::Q3ObjectDuplicate
        | PpcImportDispatcherTarget::Q3SharedGetReference
        | PpcImportDispatcherTarget::Q3SharedIsReferenced
        | PpcImportDispatcherTarget::Q3SharedGetType
        | PpcImportDispatcherTarget::Q3ShapeGetType
        | PpcImportDispatcherTarget::Q3ShapeGetLeafType
        | PpcImportDispatcherTarget::Q3ObjectIsDrawable
        | PpcImportDispatcherTarget::Q3ObjectIsType
        | PpcImportDispatcherTarget::Q3ObjectGetType
        | PpcImportDispatcherTarget::Q3ObjectGetLeafType
        | PpcImportDispatcherTarget::Q3GeometryGetType => {
            unreachable!("QuickDraw 3D object imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ShaderGetType => {
            unreachable!("QuickDraw 3D shader imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3GroupGetType
        | PpcImportDispatcherTarget::Q3RendererNewFromType
        | PpcImportDispatcherTarget::Q3RendererGetType
        | PpcImportDispatcherTarget::Q3RendererSync
        | PpcImportDispatcherTarget::Q3RendererFlush => {
            unreachable!("QuickDraw 3D object/renderer imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3InteractiveRendererSetDoubleBufferBypass
        | PpcImportDispatcherTarget::Q3InteractiveRendererSetPreferences
        | PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveContextHints
        | PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveContextHints
        | PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveDrawContexts
        | PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveTextureFilter => {
            unreachable!("QuickDraw 3D object/renderer imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3TextureShaderNew
        | PpcImportDispatcherTarget::Q3LambertIlluminationNew
        | PpcImportDispatcherTarget::Q3NullIlluminationNew
        | PpcImportDispatcherTarget::Q3PhongIlluminationNew
        | PpcImportDispatcherTarget::Q3TextureShaderGetTexture
        | PpcImportDispatcherTarget::Q3MipmapTextureNew
        | PpcImportDispatcherTarget::Q3MipmapTextureGetMipmap => {
            unreachable!(
                "QuickDraw 3D shader imports return through dispatch_q3_shader_style_import_fast"
            )
        }
        PpcImportDispatcherTarget::Q3StorageGetType
        | PpcImportDispatcherTarget::Q3MemoryStorageSet
        | PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer
        | PpcImportDispatcherTarget::Q3MemoryStorageSetBuffer
        | PpcImportDispatcherTarget::Q3MemoryStorageGetType => {
            unreachable!("QuickDraw 3D storage imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ViewAngleAspectCameraNew
        | PpcImportDispatcherTarget::Q3OrthographicCameraNew
        | PpcImportDispatcherTarget::Q3ViewPlaneCameraNew
        | PpcImportDispatcherTarget::Q3CameraGetPlacement
        | PpcImportDispatcherTarget::Q3CameraSetPlacement
        | PpcImportDispatcherTarget::Q3CameraGetRange
        | PpcImportDispatcherTarget::Q3CameraSetRange
        | PpcImportDispatcherTarget::Q3CameraGetViewPort
        | PpcImportDispatcherTarget::Q3CameraSetViewPort
        | PpcImportDispatcherTarget::Q3CameraGetWorldToView
        | PpcImportDispatcherTarget::Q3CameraGetViewToFrustum => {
            unreachable!("QuickDraw 3D scene imports return through dispatch_q3_scene_import_fast")
        }
        PpcImportDispatcherTarget::Q3LightGetType
        | PpcImportDispatcherTarget::Q3LightGetState
        | PpcImportDispatcherTarget::Q3LightSetState
        | PpcImportDispatcherTarget::Q3LightGetBrightness
        | PpcImportDispatcherTarget::Q3LightSetBrightness
        | PpcImportDispatcherTarget::Q3LightGetColor
        | PpcImportDispatcherTarget::Q3LightSetColor
        | PpcImportDispatcherTarget::Q3LightGetData
        | PpcImportDispatcherTarget::Q3LightSetData
        | PpcImportDispatcherTarget::Q3AmbientLightNew
        | PpcImportDispatcherTarget::Q3AmbientLightGetData
        | PpcImportDispatcherTarget::Q3AmbientLightSetData
        | PpcImportDispatcherTarget::Q3DirectionalLightNew
        | PpcImportDispatcherTarget::Q3DirectionalLightGetCastShadowsState
        | PpcImportDispatcherTarget::Q3DirectionalLightSetCastShadowsState
        | PpcImportDispatcherTarget::Q3DirectionalLightGetDirection
        | PpcImportDispatcherTarget::Q3DirectionalLightSetDirection
        | PpcImportDispatcherTarget::Q3DirectionalLightGetData
        | PpcImportDispatcherTarget::Q3DirectionalLightSetData => {
            unreachable!("QuickDraw 3D scene imports return through dispatch_q3_scene_import_fast")
        }
        PpcImportDispatcherTarget::Q3PointLightNew
        | PpcImportDispatcherTarget::Q3PointLightGetCastShadowsState
        | PpcImportDispatcherTarget::Q3PointLightSetCastShadowsState
        | PpcImportDispatcherTarget::Q3PointLightGetAttenuation
        | PpcImportDispatcherTarget::Q3PointLightSetAttenuation
        | PpcImportDispatcherTarget::Q3PointLightGetLocation
        | PpcImportDispatcherTarget::Q3PointLightSetLocation
        | PpcImportDispatcherTarget::Q3PointLightGetData
        | PpcImportDispatcherTarget::Q3PointLightSetData
        | PpcImportDispatcherTarget::Q3SpotLightNew
        | PpcImportDispatcherTarget::Q3SpotLightGetCastShadowsState
        | PpcImportDispatcherTarget::Q3SpotLightSetCastShadowsState
        | PpcImportDispatcherTarget::Q3SpotLightGetAttenuation
        | PpcImportDispatcherTarget::Q3SpotLightSetAttenuation
        | PpcImportDispatcherTarget::Q3SpotLightGetLocation
        | PpcImportDispatcherTarget::Q3SpotLightSetLocation
        | PpcImportDispatcherTarget::Q3SpotLightGetDirection
        | PpcImportDispatcherTarget::Q3SpotLightSetDirection
        | PpcImportDispatcherTarget::Q3SpotLightGetHotAngle
        | PpcImportDispatcherTarget::Q3SpotLightSetHotAngle
        | PpcImportDispatcherTarget::Q3SpotLightGetOuterAngle
        | PpcImportDispatcherTarget::Q3SpotLightSetOuterAngle
        | PpcImportDispatcherTarget::Q3SpotLightGetFallOff
        | PpcImportDispatcherTarget::Q3SpotLightSetFallOff
        | PpcImportDispatcherTarget::Q3SpotLightGetData
        | PpcImportDispatcherTarget::Q3SpotLightSetData => {
            unreachable!("QuickDraw 3D scene imports return through dispatch_q3_scene_import_fast")
        }
        PpcImportDispatcherTarget::Q3ShaderGetUVTransform
        | PpcImportDispatcherTarget::Q3ShaderSetUVTransform
        | PpcImportDispatcherTarget::Q3ShaderGetUBoundary
        | PpcImportDispatcherTarget::Q3ShaderSetUBoundary
        | PpcImportDispatcherTarget::Q3ShaderGetVBoundary
        | PpcImportDispatcherTarget::Q3ShaderSetVBoundary => {
            unreachable!(
                "QuickDraw 3D shader imports return through dispatch_q3_shader_style_import_fast"
            )
        }
        PpcImportDispatcherTarget::Q3BackfacingStyleNew
        | PpcImportDispatcherTarget::Q3BackfacingStyleGet
        | PpcImportDispatcherTarget::Q3BackfacingStyleSet
        | PpcImportDispatcherTarget::Q3InterpolationStyleNew
        | PpcImportDispatcherTarget::Q3InterpolationStyleGet
        | PpcImportDispatcherTarget::Q3InterpolationStyleSet
        | PpcImportDispatcherTarget::Q3FillStyleNew
        | PpcImportDispatcherTarget::Q3FillStyleGet
        | PpcImportDispatcherTarget::Q3FillStyleSet
        | PpcImportDispatcherTarget::Q3OrientationStyleNew
        | PpcImportDispatcherTarget::Q3OrientationStyleGet
        | PpcImportDispatcherTarget::Q3OrientationStyleSet => {
            unreachable!(
                "QuickDraw 3D style imports return through dispatch_q3_shader_style_import_fast"
            )
        }
        PpcImportDispatcherTarget::Q3TriMeshNew
        | PpcImportDispatcherTarget::Q3TriMeshGetData
        | PpcImportDispatcherTarget::Q3TriMeshSetData
        | PpcImportDispatcherTarget::Q3TriMeshEmptyData
        | PpcImportDispatcherTarget::Q3AttributeSetNew
        | PpcImportDispatcherTarget::Q3AttributeSetAdd
        | PpcImportDispatcherTarget::Q3AttributeSetGet
        | PpcImportDispatcherTarget::Q3AttributeSetClear
        | PpcImportDispatcherTarget::Q3AttributeSetContains
        | PpcImportDispatcherTarget::Q3AttributeSetGetNextAttributeType => {
            unreachable!("QuickDraw 3D geometry imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3StorageGetSize
        | PpcImportDispatcherTarget::Q3StorageGetData
        | PpcImportDispatcherTarget::Q3StorageSetData => {
            unreachable!("QuickDraw 3D storage imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3PixmapDrawContextNew
        | PpcImportDispatcherTarget::Q3MacDrawContextNew
        | PpcImportDispatcherTarget::Q3DrawContextGetPane => {
            unreachable!("QuickDraw 3D geometry imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3Vector3DNormalize
        | PpcImportDispatcherTarget::Q3Vector3DLength
        | PpcImportDispatcherTarget::Q3Vector2DNormalize
        | PpcImportDispatcherTarget::Q3Vector3DCross
        | PpcImportDispatcherTarget::Q3Point2DDistance
        | PpcImportDispatcherTarget::Q3Point3DDistance
        | PpcImportDispatcherTarget::Q3Point3DCrossProductTri
        | PpcImportDispatcherTarget::Q3BoundingBoxSetFromPoints3D
        | PpcImportDispatcherTarget::Q3Matrix3x3SetTranslate
        | PpcImportDispatcherTarget::Q3Matrix4x4SetIdentity
        | PpcImportDispatcherTarget::Q3Matrix4x4SetTranslate
        | PpcImportDispatcherTarget::Q3Matrix4x4SetScale
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateX
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateY
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateZ
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateXyz
        | PpcImportDispatcherTarget::Q3Matrix4x4Multiply
        | PpcImportDispatcherTarget::Q3Matrix4x4Transpose
        | PpcImportDispatcherTarget::Q3Matrix4x4Invert
        | PpcImportDispatcherTarget::Q3Point3DTransform
        | PpcImportDispatcherTarget::Q3Point3DTo3DTransformArray
        | PpcImportDispatcherTarget::Q3Point3DTo4DTransformArray
        | PpcImportDispatcherTarget::Q3Vector3DTransform
        | PpcImportDispatcherTarget::Q3MatrixTransformNew
        | PpcImportDispatcherTarget::Q3MatrixTransformSet
        | PpcImportDispatcherTarget::Q3TransformGetMatrix => {
            unreachable!("QuickDraw 3D math imports return through dispatch_q3_math_import_fast")
        }
        PpcImportDispatcherTarget::Q3FileSetStorage
        | PpcImportDispatcherTarget::Q3FileOpenRead
        | PpcImportDispatcherTarget::Q3FileReadObject
        | PpcImportDispatcherTarget::Q3FileIsEndOfFile
        | PpcImportDispatcherTarget::Q3FileClose => {
            unreachable!("QuickDraw 3D file imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3GroupAddObject
        | PpcImportDispatcherTarget::Q3GroupAddObjectBefore
        | PpcImportDispatcherTarget::Q3GroupCountObjects
        | PpcImportDispatcherTarget::Q3GroupGetFirstPosition
        | PpcImportDispatcherTarget::Q3GroupGetNextPosition
        | PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType
        | PpcImportDispatcherTarget::Q3GroupGetPositionObject
        | PpcImportDispatcherTarget::Q3GroupRemovePosition
        | PpcImportDispatcherTarget::Q3LightGroupNew => {
            unreachable!("QuickDraw 3D group imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ViewSetRenderer
        | PpcImportDispatcherTarget::Q3ViewGetRenderer
        | PpcImportDispatcherTarget::Q3ViewSetLightGroup
        | PpcImportDispatcherTarget::Q3ViewGetLightGroup
        | PpcImportDispatcherTarget::Q3ViewSetDrawContext
        | PpcImportDispatcherTarget::Q3ViewGetDrawContext
        | PpcImportDispatcherTarget::Q3ViewSetCamera
        | PpcImportDispatcherTarget::Q3ViewGetCamera
        | PpcImportDispatcherTarget::Q3ViewGetWorldToFrustumMatrixState
        | PpcImportDispatcherTarget::Q3ViewGetFrustumToWindowMatrixState
        | PpcImportDispatcherTarget::Q3ViewStartRendering
        | PpcImportDispatcherTarget::Q3ViewEndRendering
        | PpcImportDispatcherTarget::Q3ViewStartBoundingBox
        | PpcImportDispatcherTarget::Q3ViewEndBoundingBox
        | PpcImportDispatcherTarget::Q3ViewStartBoundingSphere
        | PpcImportDispatcherTarget::Q3ViewEndBoundingSphere
        | PpcImportDispatcherTarget::Q3ViewCancel => {
            unreachable!("QuickDraw 3D view imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ShaderSubmit | PpcImportDispatcherTarget::Q3StyleSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3BackfacingStyleSubmit
        | PpcImportDispatcherTarget::Q3InterpolationStyleSubmit
        | PpcImportDispatcherTarget::Q3FillStyleSubmit
        | PpcImportDispatcherTarget::Q3OrientationStyleSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3FogStyleSubmit
        | PpcImportDispatcherTarget::Q3TriMeshSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3MatrixTransformSubmit
        | PpcImportDispatcherTarget::Q3ResetTransformSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3PushSubmit
        | PpcImportDispatcherTarget::Q3PopSubmit
        | PpcImportDispatcherTarget::Q3ObjectSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::QADeviceGetFirstEngine => {
            Some(PpcImportAction::Return(PPC_QA_ENGINE))
        }
        // The accelerated engine is followed by Apple's software engine.
        PpcImportDispatcherTarget::QADeviceGetNextEngine => Some(PpcImportAction::Return(
            if cpu.gpr[4] == PPC_QA_ENGINE { PPC_QA_SOFTWARE_ENGINE } else { 0 },
        )),
        PpcImportDispatcherTarget::QAEngineGestalt => Some(PpcImportAction::Return(
            qd3d::ppc_qa_engine_gestalt(cpu, memory),
        )),
        PpcImportDispatcherTarget::CloseComponent => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_close_component(cpu, quicktime),
        ))),
        PpcImportDispatcherTarget::NewRoutineDescriptor
        | PpcImportDispatcherTarget::NewIOCompletionUPP
        | PpcImportDispatcherTarget::DisposeIOCompletionUPP
        | PpcImportDispatcherTarget::NewControlUserPaneDrawUPP
        | PpcImportDispatcherTarget::DisposeControlUserPaneDrawUPP
        | PpcImportDispatcherTarget::NewAEEventHandlerUPP
        | PpcImportDispatcherTarget::DisposeAEEventHandlerUPP
        | PpcImportDispatcherTarget::NewEventHandlerUPP
        | PpcImportDispatcherTarget::DisposeEventHandlerUPP
        | PpcImportDispatcherTarget::NewDMDisplayModeListIteratorUPP
        | PpcImportDispatcherTarget::DisposeDMDisplayModeListIteratorUPP
        | PpcImportDispatcherTarget::NewQTCallBackUPP
        | PpcImportDispatcherTarget::DisposeQTCallBackUPP
        | PpcImportDispatcherTarget::NewEventLoopTimerUPP
        | PpcImportDispatcherTarget::DisposeEventLoopTimerUPP
        | PpcImportDispatcherTarget::NewGrowZoneUPP
        | PpcImportDispatcherTarget::NewObjectSupportUPP { .. }
        | PpcImportDispatcherTarget::DisposeGrowZoneUPP
        | PpcImportDispatcherTarget::NewModalFilterUPP
        | PpcImportDispatcherTarget::DisposeModalFilterUPP
        | PpcImportDispatcherTarget::NewUserItemUPP
        | PpcImportDispatcherTarget::DisposeUserItemUPP
        | PpcImportDispatcherTarget::NewSndCallBackUPP
        | PpcImportDispatcherTarget::NewThemeButtonDrawUPP
        | PpcImportDispatcherTarget::NewThemeEraseUPP
        | PpcImportDispatcherTarget::NewControlActionUPP
        | PpcImportDispatcherTarget::NewTEClickLoopUPP
        | PpcImportDispatcherTarget::NewNavEventUPP
        | PpcImportDispatcherTarget::DisposeControlActionUPP
        | PpcImportDispatcherTarget::NewControlKeyFilterUPP
        | PpcImportDispatcherTarget::DisposeControlKeyFilterUPP
        | PpcImportDispatcherTarget::NewControlEditTextValidationUPP
        | PpcImportDispatcherTarget::DisposeControlEditTextValidationUPP
        | PpcImportDispatcherTarget::NewFatRoutineDescriptor
        | PpcImportDispatcherTarget::DisposeRoutineDescriptor
        | PpcImportDispatcherTarget::CallUniversalProc
        | PpcImportDispatcherTarget::CallOSTrapUniversalProc => {
            unreachable!("mixed mode imports return through dispatch_mixed_mode_import")
        }
        PpcImportDispatcherTarget::NGetTrapAddress
        | PpcImportDispatcherTarget::GetToolTrapAddress
        | PpcImportDispatcherTarget::GetOSTrapAddress => {
            let trap_word = cpu.gpr[3] as u16;
            let toolbox = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::GetToolTrapAddress
            ) || (matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::NGetTrapAddress
            ) && cpu.gpr[4] != 0);
            Some(PpcImportAction::Return(
                ppc_logical_trap_address(memory, trap_word, toolbox).unwrap_or(0),
            ))
        }
        PpcImportDispatcherTarget::SetToolTrapAddress
        | PpcImportDispatcherTarget::SetOSTrapAddress
        | PpcImportDispatcherTarget::NSetTrapAddress => {
            let toolbox = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::SetToolTrapAddress
            ) || (matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::NSetTrapAddress
            ) && cpu.gpr[5] != 0);
            Some(
                if ppc_set_logical_trap_address(memory, cpu.gpr[4] as u16, toolbox, cpu.gpr[3]) {
                    PpcImportAction::ReturnPreserve
                } else {
                    PpcImportAction::Halt
                },
            )
        }
        PpcImportDispatcherTarget::LegacyMemoryUtility(operation) => {
            ppc_dispatch_legacy_memory_utility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            )
        }
        PpcImportDispatcherTarget::LegacyControl(_) => {
            unreachable!("control imports return through dispatch_control_import")
        }
        PpcImportDispatcherTarget::LegacyWindow(_) => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::AppleEventCompatibility(_) => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::DialogCompatibility(_) => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::QuickDrawCompatibility(operation) => {
            Some(dispatch_quickdraw::ppc_dispatch_quickdraw_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                gworlds,
                *current_gworld,
                *current_gdevice,
                screen_clut,
                color_manager_clut,
                *quickdraw_fore_color,
                quickdraw_fore_indices.get(current_gworld).copied(),
                *quickdraw_back_color,
                toolbox_startup,
            ))
        }
        PpcImportDispatcherTarget::SystemCompatibility(operation) => {
            Some(dispatch_system::ppc_dispatch_system_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                launched_app_path,
                &mut toolbox_startup.kchr_cache_ptr,
            ))
        }
        PpcImportDispatcherTarget::OpenTransport(function) => Some(
            dispatch_open_transport::ppc_dispatch_open_transport(function, cpu, memory),
        ),
        PpcImportDispatcherTarget::ToolboxCompat(operation) => Some(
            dispatch_toolbox_compat::ppc_dispatch_toolbox_compat(operation, cpu, memory),
        ),
        PpcImportDispatcherTarget::WindowContentColor { set } => {
            Some(dispatch_toolbox_compat::ppc_window_content_color(
                set,
                cpu,
                memory,
                &mut toolbox_startup.window_content_colors,
            ))
        }
        PpcImportDispatcherTarget::DebugStr { c_string } => {
            // No low-level debugger is installed, so the call returns; the
            // message is the application's own assertion text, worth seeing.
            if ppc_hle_trace_enabled() {
                let message = if c_string {
                    dispatch_stdc::ppc_std_c_string(memory, cpu.gpr[3], 255)
                } else {
                    ppc_read_pstring_bytes(memory, cpu.gpr[3]).unwrap_or_default()
                };
                eprintln!(
                    "[PPC-TRACE] DebugStr lr=${:08X} {:?}",
                    cpu.lr,
                    decode_mac_roman(&message)
                );
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::TextEncoding(operation) => Some(
            dispatch_text_encoding::ppc_dispatch_text_encoding(
                operation,
                cpu,
                memory,
                &mut toolbox_startup.text_encoding,
            ),
        ),
        PpcImportDispatcherTarget::AppleTalkCompatibility(operation) => Some(
            dispatch_appletalk::ppc_dispatch_appletalk_compatibility(operation, cpu, memory),
        ),
        PpcImportDispatcherTarget::PrintingCompatibility(operation) => {
            Some(dispatch_printing::ppc_dispatch_printing_compatibility(
                operation,
                cpu,
                memory,
                toolbox_startup,
            ))
        }
        PpcImportDispatcherTarget::CarbonPrinting(operation) => {
            Some(dispatch_printing::ppc_dispatch_carbon_printing(
                operation,
                cpu,
                memory,
                toolbox_startup,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                handles,
            ))
        }
        PpcImportDispatcherTarget::NavGetDefaultDialogOptions => {
            Some(dispatch_standard_file::ppc_nav_get_default_dialog_options(
                cpu,
                memory,
                launched_app_path,
            ))
        }
        PpcImportDispatcherTarget::SlotCompatibility => {
            Some(ppc_dispatch_slot_compatibility(binding, cpu, memory))
        }
        PpcImportDispatcherTarget::StandardFileCompatibility(_) => {
            unreachable!("standard file imports return through dispatch_standard_file_import")
        }
        PpcImportDispatcherTarget::SysBeep
        | PpcImportDispatcherTarget::SndSoundManagerVersion
        | PpcImportDispatcherTarget::UnsignedFixedMulDiv
        | PpcImportDispatcherTarget::GetSoundOutputInfo
        | PpcImportDispatcherTarget::GetCompressionInfo
        | PpcImportDispatcherTarget::GetSoundVol
        | PpcImportDispatcherTarget::SetSoundVol
        | PpcImportDispatcherTarget::GetDefaultOutputVolume
        | PpcImportDispatcherTarget::SetDefaultOutputVolume
        | PpcImportDispatcherTarget::SndNewChannel
        | PpcImportDispatcherTarget::SndControl
        | PpcImportDispatcherTarget::SetupSndHeader
        | PpcImportDispatcherTarget::SndDisposeChannel
        | PpcImportDispatcherTarget::SndPlay
        | PpcImportDispatcherTarget::SndChannelStatus
        | PpcImportDispatcherTarget::SndGetInfo
        | PpcImportDispatcherTarget::SndSetInfo
        | PpcImportDispatcherTarget::ParseSndHeader
        | PpcImportDispatcherTarget::ParseAIFFHeader
        | PpcImportDispatcherTarget::SndDoCommand
        | PpcImportDispatcherTarget::SndDoImmediate
        | PpcImportDispatcherTarget::SndPlayDoubleBuffer
        | PpcImportDispatcherTarget::SndStartFilePlay
        | PpcImportDispatcherTarget::SndPauseFilePlay
        | PpcImportDispatcherTarget::SndStopFilePlay
        | PpcImportDispatcherTarget::GetSoundHeaderOffset
        | PpcImportDispatcherTarget::SoundInputCompatibility(_) => {
            unreachable!("sound imports return through dispatch_sound_import")
        }
        PpcImportDispatcherTarget::QtAtom(operation) => Some(dispatch_qt_atoms::dispatch(
            operation,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            handles,
        )),
        PpcImportDispatcherTarget::SpeechRecognition(operation) => {
            Some(dispatch_speech_recognition::dispatch(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::SpeechCompatibility(operation) => {
            Some(ppc_dispatch_speech_compatibility(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::SoundSprocket(function) => {
            Some(sound_sprocket::ppc_dispatch_sound_sprocket(
                function,
                cpu,
                memory,
                &mut sound.sound_sprocket,
            ))
        }
        PpcImportDispatcherTarget::QuickTimeCompatibility(operation) => {
            if let Some(action) = quicktime::controller::filter_controller_action(
                operation,
                cpu,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                quicktime,
            ) {
                return Some(action);
            }
            let service_callbacks = matches!(
                operation,
                PpcQuickTimeCompatibilityOperation::MCIdle
                    | PpcQuickTimeCompatibilityOperation::MCDoAction
                    | PpcQuickTimeCompatibilityOperation::SetMovieRate
                    | PpcQuickTimeCompatibilityOperation::SetMovieTime
            );
            let service_drawing = operation != PpcQuickTimeCompatibilityOperation::MCDoAction
                || matches!(cpu.gpr[4] as i16, 1 | 8);
            let before = dispatch_quicktime::quicktime_callback_progress(quicktime);
            let action = dispatch_quicktime_compatibility(
                operation,
                cpu,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                handles,
                gworlds.as_slice(),
                *current_gworld,
                quicktime,
                sound,
                files.as_slice(),
                vfs_files.as_slice(),
            );
            Some(if service_callbacks {
                dispatch_quicktime::finish_controller_callbacks(
                    action,
                    cpu,
                    memory,
                    quicktime,
                    sound,
                    before,
                    service_drawing,
                )
            } else {
                action
            })
        }
        PpcImportDispatcherTarget::InputSprocketDriver(_)
        | PpcImportDispatcherTarget::InputSprocketCompatibility(_) => {
            unreachable!(
                "input sprocket compatibility imports return through dispatch_inputsprocket_import"
            )
        }
        PpcImportDispatcherTarget::MathFp(function) => {
            Some(math_fp::ppc_dispatch_fp(function, cpu, memory))
        }
        PpcImportDispatcherTarget::MathCompatibility(operation) => {
            Some(ppc_dispatch_math_compatibility(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::Math64(operation) => {
            Some(ppc_dispatch_math64(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::StdCCompatibility(_) => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::ObjectSupportCompatibility => {
            Some(ppc_dispatch_object_support_compatibility(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                apple_events,
            ))
        }
        PpcImportDispatcherTarget::ObjectSupportCreateOffsetDescriptor => {
            // CreateOffsetDescriptor (ObjectSupportLib)
            // Creates a signed offset as a typeLongInteger descriptor.
            // FUNCTION CreateOffsetDescriptor(theOffset: LongInt; VAR theDescriptor: AEDesc): OSErr;
            // Inside Macintosh: Interapplication Communication (1993), pp. 6-72, 6-88.
            let result_ptr = cpu.gpr[4];
            let result = if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, 8) {
                PPC_PARAM_ERR
            } else {
                ppc_create_process_owned_ae_desc(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    heap_limit,
                    last_mem_error,
                    handles,
                    Some(&apple_events.descriptors),
                    result_ptr,
                    u32::from_be_bytes(*b"long"),
                    &cpu.gpr[3].to_be_bytes(),
                )
            };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::ObjectSupportInit
        | PpcImportDispatcherTarget::ObjectSupportInstallAccessor
        | PpcImportDispatcherTarget::ObjectSupportGetAccessor
        | PpcImportDispatcherTarget::ObjectSupportCallAccessor
        | PpcImportDispatcherTarget::ObjectSupportDisposeToken
        | PpcImportDispatcherTarget::ObjectSupportRemoveAccessor
        | PpcImportDispatcherTarget::ObjectSupportSetCallbacks
        | PpcImportDispatcherTarget::ObjectSupportResolve => {
            unreachable!("object support imports return through Apple Event dispatch")
        }
        PpcImportDispatcherTarget::Glide(operation) => {
            Some(dispatch_glide::dispatch(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::GlideSstQueryBoards => {
            // 3Dfx Glide 2.4 Reference Manual, grSstQueryBoards: the routine
            // returns FXFALSE when it detects no Voodoo Graphics subsystem.
            // Systemless exposes the generic QuickDraw 3D Accelerator path,
            // not a fabricated 3Dfx board, so leave hwConfig untouched.
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::FlushEvents
        | PpcImportDispatcherTarget::GetMainEventQueue
        | PpcImportDispatcherTarget::GetMainEventLoop
        | PpcImportDispatcherTarget::InstallEventLoopTimer
        | PpcImportDispatcherTarget::RemoveEventLoopTimer
        | PpcImportDispatcherTarget::GetApplicationEventTarget
        | PpcImportDispatcherTarget::GetEventDispatcherTarget
        | PpcImportDispatcherTarget::InstallEventHandler
        | PpcImportDispatcherTarget::RemoveEventHandler
        | PpcImportDispatcherTarget::CreateEvent
        | PpcImportDispatcherTarget::ReleaseEvent
        | PpcImportDispatcherTarget::RetainEvent
        | PpcImportDispatcherTarget::GetEventClass
        | PpcImportDispatcherTarget::GetEventKind
        | PpcImportDispatcherTarget::GetEventTime
        | PpcImportDispatcherTarget::SetEventParameter
        | PpcImportDispatcherTarget::GetEventParameter
        | PpcImportDispatcherTarget::PostEventToQueue
        | PpcImportDispatcherTarget::ReceiveNextEvent
        | PpcImportDispatcherTarget::SendEventToEventTarget
        | PpcImportDispatcherTarget::CallNextEventHandler
        | PpcImportDispatcherTarget::RunApplicationEventLoop
        | PpcImportDispatcherTarget::QuitApplicationEventLoop
        | PpcImportDispatcherTarget::InstallStandardEventHandler
        | PpcImportDispatcherTarget::GetCurrentEventTime
        | PpcImportDispatcherTarget::FlushEventQueue
        | PpcImportDispatcherTarget::SetEventMask
        | PpcImportDispatcherTarget::GetNextEvent(_)
        | PpcImportDispatcherTarget::GetOSEvent
        | PpcImportDispatcherTarget::EventAvail
        | PpcImportDispatcherTarget::OSEventAvail
        | PpcImportDispatcherTarget::PostEvent
        | PpcImportDispatcherTarget::Button
        | PpcImportDispatcherTarget::StillDown
        | PpcImportDispatcherTarget::WaitMouseUp
        | PpcImportDispatcherTarget::GetKeys
        | PpcImportDispatcherTarget::GetMouse
        | PpcImportDispatcherTarget::GetCurrentKeyModifiers
        | PpcImportDispatcherTarget::GetGlobalMouse => {
            unreachable!("event imports return through dispatch_event_import")
        }
        PpcImportDispatcherTarget::LMGetMenuList
        | PpcImportDispatcherTarget::LMSetMenuHook
        | PpcImportDispatcherTarget::LMSetCurActivate
        | PpcImportDispatcherTarget::LMGetCurDeactive
        | PpcImportDispatcherTarget::LMSetCurDeactive
        | PpcImportDispatcherTarget::LMGetMenuFlash
        | PpcImportDispatcherTarget::LMGetKeyThresh
        | PpcImportDispatcherTarget::LowMemoryAccessor(_)
        | PpcImportDispatcherTarget::LMSetKeyThresh
        | PpcImportDispatcherTarget::LMGetKeyRepThresh
        | PpcImportDispatcherTarget::LMSetKeyRepThresh
        | PpcImportDispatcherTarget::LMGetHiliteRGB
        | PpcImportDispatcherTarget::LMSetHiliteRGB
        | PpcImportDispatcherTarget::LMGetPaintWhite
        | PpcImportDispatcherTarget::LMGetHWCfgFlags
        | PpcImportDispatcherTarget::LMSetROMMapInsert
        | PpcImportDispatcherTarget::LMGetSysMap
        | PpcImportDispatcherTarget::LMGetBootDrive
        | PpcImportDispatcherTarget::LMSetBootDrive
        | PpcImportDispatcherTarget::LMGetCurApRefNum
        | PpcImportDispatcherTarget::GetVCBQHdr
        | PpcImportDispatcherTarget::GetDrvQHdr
        | PpcImportDispatcherTarget::LMGetSysEvtMask
        | PpcImportDispatcherTarget::LMSetSysEvtMask
        | PpcImportDispatcherTarget::LMGetDefltStack
        | PpcImportDispatcherTarget::LMGetCurStackBase
        | PpcImportDispatcherTarget::LMSetPaintWhite
        | PpcImportDispatcherTarget::LMSetResumeProc
        | PpcImportDispatcherTarget::LMGetResumeProc
        | PpcImportDispatcherTarget::LMSetACount
        | PpcImportDispatcherTarget::LMGetACount
        | PpcImportDispatcherTarget::LMSetANumber
        | PpcImportDispatcherTarget::LMGetANumber
        | PpcImportDispatcherTarget::LMSetDABeeper
        | PpcImportDispatcherTarget::LMGetDABeeper
        | PpcImportDispatcherTarget::LMGetDAStrings
        | PpcImportDispatcherTarget::LMSetDlgFont
        | PpcImportDispatcherTarget::LMGetDlgFont
        | PpcImportDispatcherTarget::SetMenuFlash
        | PpcImportDispatcherTarget::GetGrayRgn
        | PpcImportDispatcherTarget::LMSetGrayRgn
        | PpcImportDispatcherTarget::LMGetUTableBase
        | PpcImportDispatcherTarget::LMGetCurDirStore
        | PpcImportDispatcherTarget::LMSetCurDirStore
        | PpcImportDispatcherTarget::LMGetSFSaveDisk
        | PpcImportDispatcherTarget::LMSetSFSaveDisk
        | PpcImportDispatcherTarget::LMGetHiliteMode
        | PpcImportDispatcherTarget::LMSetHiliteMode
        | PpcImportDispatcherTarget::LMGetRndSeed
        | PpcImportDispatcherTarget::LMGetGhostWindow
        | PpcImportDispatcherTarget::LMGetCrsrBusy
        | PpcImportDispatcherTarget::LMSetRndSeed
        | PpcImportDispatcherTarget::SetCurrentA5
        | PpcImportDispatcherTarget::SetA5
        | PpcImportDispatcherTarget::LMGetCurrentA5 => {
            unreachable!("low-memory imports return through dispatch_low_memory_import")
        }
        PpcImportDispatcherTarget::TickCount
        | PpcImportDispatcherTarget::GetDateTime
        | PpcImportDispatcherTarget::ReadDateTime
        | PpcImportDispatcherTarget::ReadLocation
        | PpcImportDispatcherTarget::GetTime
        | PpcImportDispatcherTarget::Delay
        | PpcImportDispatcherTarget::GetCaretTime
        | PpcImportDispatcherTarget::GetDblTime
        | PpcImportDispatcherTarget::LMGetTime
        | PpcImportDispatcherTarget::SecondsToDate
        | PpcImportDispatcherTarget::DateToSeconds
        | PpcImportDispatcherTarget::Microseconds
        | PpcImportDispatcherTarget::OtElapsed { .. }
        | PpcImportDispatcherTarget::AbsoluteToNanoseconds
        | PpcImportDispatcherTarget::NanosecondsToAbsolute
        | PpcImportDispatcherTarget::AddAbsoluteToAbsolute
        | PpcImportDispatcherTarget::SubAbsoluteFromAbsolute
        | PpcImportDispatcherTarget::MPDelayUntil => {
            unreachable!("time imports return through dispatch_time_import")
        }
        PpcImportDispatcherTarget::ReturnError(error) => {
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        PpcImportDispatcherTarget::ReturnNoErr => Some(PpcImportAction::Return(0)),
        PpcImportDispatcherTarget::ReturnOne => Some(PpcImportAction::Return(1)),
        PpcImportDispatcherTarget::NewOTNotifyUPP => {
            // OpenTransport.h (Universal Interfaces 3.4.1): on classic PowerPC
            // systems, NewOTNotifyUPP(userRoutine) returns the routine pointer.
            Some(PpcImportAction::Return(cpu.gpr[3]))
        }
        PpcImportDispatcherTarget::AglChoosePixelFormat => Some(PpcImportAction::Return(
            ppc_agl_choose_pixel_format(cpu, memory, agl, gworlds),
        )),
        PpcImportDispatcherTarget::AglDescribePixelFormat => Some(PpcImportAction::Return(
            u32::from(ppc_agl_describe_pixel_format(cpu, memory, agl)),
        )),
        PpcImportDispatcherTarget::AglDestroyPixelFormat => {
            if !agl.destroy_pixel_format(cpu.gpr[3]) {
                agl.set_error(classic_gl_agl::AGL_BAD_PIXELFMT);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::AglGetError => Some(PpcImportAction::Return(agl.get_error())),
        PpcImportDispatcherTarget::AglCreateContext => {
            Some(PpcImportAction::Return(agl_macros::create_context(
                cpu,
                memory,
                process_memory_manager,
                agl,
                heap_cursor,
                last_mem_error,
            )))
        }
        PpcImportDispatcherTarget::AglDestroyContext => Some(PpcImportAction::Return(u32::from(
            agl_macros::destroy_context(
                cpu.gpr[3],
                memory,
                process_memory_manager,
                agl,
                heap_cursor,
                last_mem_error,
            ),
        ))),
        PpcImportDispatcherTarget::AglSetCurrentContext => Some(PpcImportAction::Return(
            u32::from(agl.set_current_context(cpu.gpr[3])),
        )),
        PpcImportDispatcherTarget::AglGetCurrentContext => {
            Some(PpcImportAction::Return(agl.current_context()))
        }
        PpcImportDispatcherTarget::AglSetDrawable => Some(PpcImportAction::Return(u32::from(
            ppc_agl_set_drawable(cpu, memory, agl, gworlds, window_list),
        ))),
        PpcImportDispatcherTarget::AglGetDrawable => Some(PpcImportAction::Return(
            agl.context(cpu.gpr[3])
                .map_or(0, |context| context.drawable),
        )),
        PpcImportDispatcherTarget::AglUpdateContext => Some(PpcImportAction::Return(u32::from(
            ppc_agl_update_context(cpu, memory, agl, gworlds, window_list),
        ))),
        PpcImportDispatcherTarget::AglSwapBuffers => {
            if !ppc_agl_swap_buffers(cpu, memory, agl, gworlds, window_list) {
                agl.set_error(classic_gl_agl::AGL_BAD_CONTEXT);
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::Agl(function) => {
            Some(ppc_agl_function(function, cpu, memory, agl, gworlds))
        }
        PpcImportDispatcherTarget::Gl(function) => {
            use super::classic_gl::functions::{GlFunction, GlNamed};
            let handle = agl.current_context();
            let result = match agl.context_mut(handle) {
                Some(context) => {
                    let framebuffer = context.framebuffer.as_mut();
                    super::classic_gl::call(&mut context.gl, framebuffer, function, cpu, memory)
                }
                None => super::classic_gl::GlResult::Void,
            };
            if matches!(function, GlFunction::Named(GlNamed::Flush | GlNamed::Finish)) {
                if let Some(surface) = agl
                    .context(handle)
                    .and_then(|context| ppc_agl_context_surface(memory, gworlds, window_list, context))
                {
                    agl.flush(handle, memory, surface);
                }
            }
            Some(match result {
                super::classic_gl::GlResult::Void => PpcImportAction::ReturnPreserve,
                super::classic_gl::GlResult::Value(value) => PpcImportAction::Return(value),
            })
        }
        PpcImportDispatcherTarget::Glu(function) => {
            let handle = agl.current_context();
            let result = match agl.context_mut(handle) {
                Some(context) => {
                    let framebuffer = context.framebuffer.as_mut();
                    super::classic_gl::glu::call(&mut context.gl, framebuffer, function, cpu, memory)
                }
                None => super::classic_gl::GlResult::Value(0),
            };
            Some(match result {
                super::classic_gl::GlResult::Void => PpcImportAction::ReturnPreserve,
                super::classic_gl::GlResult::Value(value) => PpcImportAction::Return(value),
            })
        }
        PpcImportDispatcherTarget::USBGetDeviceDescriptor => {
            // USB.h takes a pointer to a USBDeviceRef, not the reference itself.
            // With no attached devices no reference can resolve to a descriptor.
            // Reject it without writing a fictitious descriptor or its length.
            let reference = cpu.gpr[3];
            let error = if reference == 0 || !ppc_memory_can_read_bytes(memory, reference, 4) {
                i32::from(PPC_PARAM_ERR)
            } else {
                -6998 // kUSBUnknownDeviceErr
            };
            Some(PpcImportAction::Return(error as u32))
        }
        PpcImportDispatcherTarget::USBInstallDeviceNotification => {
            // USB DDK rev. 26, pp. 186–188: retain the registration until its
            // token is removed. No devices attach in this machine profile.
            // Ferazel's InputSprocket USB constructors (code offsets 0x34c
            // and 0x8c64) leave pbLength/pbVersion unset; use the actual layout.
            let pb = cpu.gpr[3];
            if pb != 0
                && ppc_memory_can_read_bytes(memory, pb, 36)
                && ppc_memory_can_write_bytes(memory, pb, 36)
            {
                let kind = memory.read_u8(pb + 4).unwrap_or(0xfe);
                let callback = memory.read_u32_be(pb + 28).unwrap_or(0);
                let token = toolbox_startup.usb_next_notification_token;
                let error = if !matches!(kind, 0..=3 | 0xff) {
                    -6949 // kUSBUnknownNotification (USB Manager errors, p. 188)
                } else if callback == 0 {
                    PPC_PARAM_ERR
                } else if token == 0 {
                    PPC_MEM_FULL_ERR
                } else {
                    PPC_NO_ERR
                };
                if error == PPC_NO_ERR {
                    let record = ppc_memory_read_bytes(memory, pb, 36).unwrap();
                    toolbox_startup.usb_notifications.push((token, record));
                    toolbox_startup.usb_next_notification_token = token.checked_add(1).unwrap_or(0);
                    let _ = memory.write_u32_be(pb + 24, token);
                } else {
                    let _ = memory.write_u32_be(pb + 24, 0);
                }
                let _ = memory.write_u32_be(pb + 20, ppc_i16_result(error));
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::USBRemoveDeviceNotification => {
            let records = &mut toolbox_startup.usb_notifications;
            let result =
                if let Some(index) = records.iter().position(|(token, _)| *token == cpu.gpr[3]) {
                    records.remove(index);
                    PPC_NO_ERR
                } else {
                    PPC_PARAM_ERR
                };
            Some(PpcImportAction::Return(ppc_i16_result(result)))
        }
        PpcImportDispatcherTarget::USBExpertStatus
        | PpcImportDispatcherTarget::USBExpertFatalError => {
            // USB DDK rev. 26, pp. 154 and 157: reporting does not validate
            // the device reference or unload the driver. The message is a P-string.
            let fatal = binding.dispatcher_target == PpcImportDispatcherTarget::USBExpertFatalError;
            let pointer = cpu.gpr[if fatal { 5 } else { 4 }];
            let message = if pointer == 0 {
                Some(Vec::new())
            } else {
                memory.read_u8(pointer).and_then(|length| {
                    pointer
                        .checked_add(1)
                        .and_then(|bytes| ppc_memory_read_bytes(memory, bytes, u32::from(length)))
                })
            };
            if let Some(message) = message {
                let log = &mut toolbox_startup.usb_expert_log;
                if log.len() == 256 {
                    log.remove(0);
                }
                log.push((
                    cpu.gpr[3],
                    if fatal { cpu.gpr[4] as i32 } else { 0 },
                    message,
                    cpu.gpr[if fatal { 6 } else { 5 }],
                ));
                Some(PpcImportAction::Return(0))
            } else {
                Some(PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR)))
            }
        }
        PpcImportDispatcherTarget::USBAllocMem
        | PpcImportDispatcherTarget::USBDeallocMem
        | PpcImportDispatcherTarget::USBConfigureInterface
        | PpcImportDispatcherTarget::USBFindNextPipe
        | PpcImportDispatcherTarget::USBDeviceRequest
        | PpcImportDispatcherTarget::USBIntRead
        | PpcImportDispatcherTarget::USBFindNextAssociatedDescriptor => {
            // USB DDK rev. 26, pp. 99 and 150–151. USBPB is 72 bytes
            // with mac68k alignment. Allocation requires a live USB reference;
            // immediate errors do not schedule the completion routine.
            let pb = cpu.gpr[3];
            let allowed_flags = match binding.dispatcher_target {
                PpcImportDispatcherTarget::USBFindNextPipe => 3,
                PpcImportDispatcherTarget::USBDeviceRequest => 16 | 32 | 64 | 128 | 256,
                PpcImportDispatcherTarget::USBConfigureInterface => 0,
                _ => 32,
            };
            let reserved_buffer = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::USBConfigureInterface
                    | PpcImportDispatcherTarget::USBFindNextPipe
            );
            let error: i32 = if pb == 0 || !ppc_memory_can_read_bytes(memory, pb, 72) {
                -6985
            } else if memory.read_u16_be(pb + 6).unwrap_or(0) < 72 {
                -6985
            } else if memory.read_u16_be(pb + 8) != Some(0x0100) {
                -6986
            } else if memory.read_u32_be(pb + 20) == Some(0)
                || (memory.read_u32_be(pb + 20) == Some(u32::MAX)
                    && binding.dispatcher_target != PpcImportDispatcherTarget::USBDeallocMem)
            {
                -6984
            } else if memory.read_u32_be(pb + 44).unwrap_or(0) & !allowed_flags != 0 {
                -6983
            } else if reserved_buffer
                && [32, 36, 40]
                    .iter()
                    .any(|offset| memory.read_u32_be(pb + offset) != Some(0))
            {
                i32::from(PPC_PARAM_ERR)
            } else {
                match binding.dispatcher_target {
                    // DDK pp. 151–152: synchronous cleanup bypasses reference
                    // validation, but must still refer to USBAllocMem storage.
                    // This profile has never allocated USB storage; reject every
                    // supplied buffer, including nil, without freeing an unrelated
                    // Memory Manager pointer or invoking a completion routine.
                    PpcImportDispatcherTarget::USBDeallocMem
                        if memory.read_u32_be(pb + 20) == Some(u32::MAX) =>
                    {
                        -6975
                    }
                    PpcImportDispatcherTarget::USBIntRead => -6997,
                    PpcImportDispatcherTarget::USBFindNextAssociatedDescriptor => -6978,
                    _ => -6998,
                }
            };
            Some(PpcImportAction::Return(error as u32))
        }
        // No USB device or pipe references exist in this machine profile.
        // USB DDK rev. 26, pp. 135–136: an unknown-pipe error leaves the
        // GetPipeStatus output untouched, rather than inventing an idle pipe.
        PpcImportDispatcherTarget::USBUnknownPipe => {
            Some(PpcImportAction::Return((-6997i32) as u32))
        }
        PpcImportDispatcherTarget::USBGetNextDeviceByClass => {
            // USB DDK API Reference rev. 26, p. 179: exhaustion returns
            // -43 and leaves deviceRef unchanged. This profile has no USB
            // devices or class-driver connections to enumerate.
            let valid = cpu.gpr[3] != 0
                && cpu.gpr[4] != 0
                && ppc_memory_can_read_bytes(memory, cpu.gpr[3], 4)
                && ppc_memory_can_write_bytes(memory, cpu.gpr[4], 4);
            Some(PpcImportAction::Return(ppc_i16_result(if valid {
                -43
            } else {
                PPC_PARAM_ERR
            })))
        }
        // USB.h: UInt16 endian conversion on the big-endian guest, and
        // UInt8 bmRequestType fields (direction/type/recipient).
        PpcImportDispatcherTarget::USBToHostWord => Some(PpcImportAction::Return(u32::from(
            (cpu.gpr[3] as u16).swap_bytes(),
        ))),
        PpcImportDispatcherTarget::USBMakeBMRequestType => Some(PpcImportAction::Return(
            ((cpu.gpr[3] & 1) << 7) | ((cpu.gpr[4] & 3) << 5) | (cpu.gpr[5] & 0x1f),
        )),
        PpcImportDispatcherTarget::NoOpPreserve => Some(PpcImportAction::ReturnPreserve),
        PpcImportDispatcherTarget::ExitToShell => Some(PpcImportAction::Halt),
        PpcImportDispatcherTarget::GlmSetMode
        | PpcImportDispatcherTarget::GlmSetFunc
        | PpcImportDispatcherTarget::GlmMalloc
        | PpcImportDispatcherTarget::GlmCalloc
        | PpcImportDispatcherTarget::GlmRealloc
        | PpcImportDispatcherTarget::GlmFree
        | PpcImportDispatcherTarget::GlmPageFreeAll
        | PpcImportDispatcherTarget::GlmGetError
        | PpcImportDispatcherTarget::GlmGetInteger => {
            unreachable!("OpenGL memory imports return through the fast dispatcher")
        }
        PpcImportDispatcherTarget::Rave(_) => {
            unreachable!("RAVE imports return through the fast dispatcher")
        }
        PpcImportDispatcherTarget::UnresolvedWeak | PpcImportDispatcherTarget::Unsupported => None,
    }
}

fn ppc_agl_choose_pixel_format(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    agl: &mut PpcAglState,
    gworlds: &[PpcGWorldRecord],
) -> u32 {
    // AGLPixelFormat aglChoosePixelFormat(const AGLDevice *gdevs,
    //     GLint ndev, const GLint *attribs);
    // Apple AGL/agl.h (Mac OS X 10.2.8 SDK).
    // https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX10.2.8.sdk/System/Library/Frameworks/AGL.framework/Versions/A/Headers/agl.h
    let count = cpu.gpr[4] as i32;
    if !(0..=16).contains(&count) {
        agl.set_error(classic_gl_agl::AGL_BAD_VALUE);
        return 0;
    }
    if count > 0 {
        if cpu.gpr[3] == 0 {
            agl.set_error(classic_gl_agl::AGL_BAD_GDEV);
            return 0;
        }
        for index in 0..count as u32 {
            let Some(device) = cpu.gpr[3]
                .checked_add(index * 4)
                .and_then(|address| memory.read_u32_be(address))
            else {
                agl.set_error(classic_gl_agl::AGL_BAD_POINTER);
                return 0;
            };
            if device == 0 || !gworlds.iter().any(|world| world.gdevice == device) {
                agl.set_error(classic_gl_agl::AGL_BAD_GDEV);
                return 0;
            }
        }
    }
    let request = match ppc_agl_read_pixel_format_request(memory, cpu.gpr[5]) {
        Ok(request) => request,
        Err(classic_gl_agl::PpcAglAttributeError::BadPointer) => {
            agl.set_error(classic_gl_agl::AGL_BAD_POINTER);
            return 0;
        }
        Err(
            classic_gl_agl::PpcAglAttributeError::UnsupportedAttribute(_)
            | classic_gl_agl::PpcAglAttributeError::Unterminated,
        ) => {
            agl.set_error(classic_gl_agl::AGL_BAD_ATTRIBUTE);
            return 0;
        }
    };
    agl.choose_pixel_format(request)
}

fn ppc_agl_describe_pixel_format(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    agl: &mut PpcAglState,
) -> bool {
    // GLboolean aglDescribePixelFormat(AGLPixelFormat pix, GLint attrib,
    //     GLint *value); Apple AGL/agl.h (Mac OS X 10.2.8 SDK).
    // https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX10.2.8.sdk/System/Library/Frameworks/AGL.framework/Versions/A/Headers/agl.h
    if agl.pixel_format(cpu.gpr[3]).is_none() {
        agl.set_error(classic_gl_agl::AGL_BAD_PIXELFMT);
        return false;
    }
    let Some(value) = agl.describe_pixel_format(cpu.gpr[3], cpu.gpr[4] as i32) else {
        agl.set_error(classic_gl_agl::AGL_BAD_ATTRIBUTE);
        return false;
    };
    if cpu.gpr[5] == 0 || memory.write_u32_be(cpu.gpr[5], value as u32).is_none() {
        agl.set_error(classic_gl_agl::AGL_BAD_POINTER);
        return false;
    }
    true
}


/// AGL macro calls name their renderer explicitly and do not change the
/// caller's current context. Temporarily gather its shared objects while
/// executing the same GL implementation used by ordinary library imports.
pub(super) fn ppc_agl_macro_call(
    name: &str,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    agl: &mut PpcAglState,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
) -> Option<PpcImportAction> {
    use super::classic_gl::functions::{GlFunction, GlNamed};
    let function = GlFunction::from_name(name)?;
    let previous = agl.current_context();
    let handle = cpu.gpr[3];
    if handle == 0 || !agl.set_current_context(handle) {
        agl.set_error(classic_gl_agl::AGL_BAD_CONTEXT);
        return Some(PpcImportAction::Return(0));
    }
    let context = agl.context_mut(handle).unwrap();
    let result = super::classic_gl::call_with_first_word(
        &mut context.gl,
        context.framebuffer.as_mut(),
        function,
        cpu,
        memory,
        1,
    );
    if matches!(
        function,
        GlFunction::Named(GlNamed::Flush | GlNamed::Finish)
    ) {
        if let Some(surface) = agl
            .context(handle)
            .and_then(|context| ppc_agl_context_surface(memory, gworlds, window_list, context))
        {
            agl.flush(handle, memory, surface);
        }
    }
    let _ = agl.set_current_context(previous);
    Some(match result {
        super::classic_gl::GlResult::Void => PpcImportAction::ReturnPreserve,
        super::classic_gl::GlResult::Value(value) => PpcImportAction::Return(value),
    })
}

fn ppc_agl_window_surface(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    drawable: u32,
) -> Option<PpcFrontBuffer> {
    // Apple Technical Q&A OGL02 (2000): an AGLDrawable is a window CGrafPtr;
    // require a registered window port, rejecting bare screen/offscreen GWorlds.
    // https://leopard-adc.pepas.com/qa/ogl/ogl02.html
    if drawable == 0
        || matches!(drawable, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
        || !window_list.contains_window(drawable)
    {
        return None;
    }
    // A window port's pixmap is the screen, with bounds offset by the
    // window's global position. The drawable is the port rectangle within
    // it, clipped to the pixmap. Imaging With QuickDraw (1994), pp. 2-38--2-40.
    let surface = ppc_live_quickdraw_surface(memory, gworlds, drawable)?;
    let (top, left, bottom, right) = ppc_read_rect(memory, drawable.checked_add(16)?)?;
    let frame = surface.front_buffer;
    let bytes_per_pixel = frame.depth / 8;
    let x0 = (i32::from(left) - i32::from(surface.left)).clamp(0, frame.width as i32) as u32;
    let y0 = (i32::from(top) - i32::from(surface.top)).clamp(0, frame.height as i32) as u32;
    let x1 = (i32::from(right) - i32::from(surface.left)).clamp(0, frame.width as i32) as u32;
    let y1 = (i32::from(bottom) - i32::from(surface.top)).clamp(0, frame.height as i32) as u32;
    if x1 <= x0 || y1 <= y0 || bytes_per_pixel == 0 {
        return None;
    }
    Some(PpcFrontBuffer {
        base_addr: frame
            .base_addr
            .checked_add(y0.checked_mul(frame.row_bytes)?)?
            .checked_add(x0.checked_mul(bytes_per_pixel)?)?,
        row_bytes: frame.row_bytes,
        width: x1 - x0,
        height: y1 - y0,
        depth: frame.depth,
    })
}

/// The pixels an AGL context presents to: its window, the main screen for a
/// full-screen context, or the client memory of an off-screen context.
fn ppc_agl_context_surface(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    context: &classic_gl_agl::PpcAglContext,
) -> Option<PpcFrontBuffer> {
    if let Some(surface) = context.offscreen {
        return Some(surface);
    }
    if context.fullscreen {
        return ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD);
    }
    ppc_agl_window_surface(memory, gworlds, window_list, context.drawable)
}

fn ppc_agl_function(
    function: classic_gl_agl::PpcAglFunction,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    agl: &mut PpcAglState,
    gworlds: &[PpcGWorldRecord],
) -> PpcImportAction {
    use classic_gl_agl::PpcAglFunction as F;
    // Signatures: Apple AGL/agl.h (Mac OS X 10.2.8 SDK), AGL 2.0.
    let boolean = |value: bool| PpcImportAction::Return(u32::from(value));
    match function {
        F::QueryRendererInfo => {
            let count = cpu.gpr[4] as i32;
            if count < 0 || (count > 0 && cpu.gpr[3] == 0) {
                agl.set_error(classic_gl_agl::AGL_BAD_GDEV);
                return PpcImportAction::Return(0);
            }
            PpcImportAction::Return(classic_gl_agl::AGL_RENDERER_INFO_HANDLE)
        }
        F::NextRendererInfo | F::NextPixelFormat => PpcImportAction::Return(0),
        F::DestroyRendererInfo => PpcImportAction::ReturnPreserve,
        F::DescribeRenderer => {
            if cpu.gpr[3] != classic_gl_agl::AGL_RENDERER_INFO_HANDLE {
                agl.set_error(classic_gl_agl::AGL_BAD_RENDINFO);
                return boolean(false);
            }
            let Some(value) = agl.describe_renderer(cpu.gpr[4] as i32) else {
                agl.set_error(classic_gl_agl::AGL_BAD_PROPERTY);
                return boolean(false);
            };
            if cpu.gpr[5] == 0 || memory.write_u32_be(cpu.gpr[5], value as u32).is_none() {
                agl.set_error(classic_gl_agl::AGL_BAD_POINTER);
                return boolean(false);
            }
            boolean(true)
        }
        F::DevicesOfPixelFormat => {
            // AGLDevice *aglDevicesOfPixelFormat(AGLPixelFormat, GLint *ndevs):
            // the single renderer drives the main screen device.
            if agl.pixel_format(cpu.gpr[3]).is_none() {
                agl.set_error(classic_gl_agl::AGL_BAD_PIXELFMT);
                return PpcImportAction::Return(0);
            }
            let list = super::classic_gl::GL_STRING_AREA + 0x4F0;
            let written = memory.write_u32_be(list, PPC_MAIN_GDEVICE).is_some()
                && (cpu.gpr[4] == 0 || memory.write_u32_be(cpu.gpr[4], 1).is_some());
            PpcImportAction::Return(if written { list } else { 0 })
        }
        F::GetVersion => {
            let (major, minor) = classic_gl_agl::AGL_VERSION;
            if cpu.gpr[3] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[3], major as u32);
            }
            if cpu.gpr[4] != 0 {
                let _ = memory.write_u32_be(cpu.gpr[4], minor as u32);
            }
            PpcImportAction::ReturnPreserve
        }
        F::Configure => {
            // AGL_FORMAT_CACHE_SIZE, AGL_CLEAR_FORMAT_CACHE, AGL_RETAIN_RENDERERS.
            let known = matches!(cpu.gpr[3], 501..=503);
            if !known {
                agl.set_error(classic_gl_agl::AGL_BAD_ENUM);
            }
            boolean(known)
        }
        F::Enable | F::Disable | F::IsEnabled => {
            let pname = cpu.gpr[4];
            let Some(context) = agl.context_mut(cpu.gpr[3]) else {
                agl.set_error(classic_gl_agl::AGL_BAD_CONTEXT);
                return boolean(false);
            };
            if !matches!(pname, 200 | 202 | 203 | 210 | 220 | 230 | 254 | 255) {
                agl.set_error(classic_gl_agl::AGL_BAD_ENUM);
                return boolean(false);
            }
            match function {
                F::Enable => {
                    context.enables.insert(pname);
                    boolean(true)
                }
                F::Disable => {
                    context.enables.remove(&pname);
                    boolean(true)
                }
                _ => boolean(context.enables.contains(&pname)),
            }
        }
        F::SetInteger | F::GetInteger => {
            let (handle, pname, pointer) = (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5]);
            let count = match pname {
                200 | 202 => 4,      // AGL_SWAP_RECT, AGL_BUFFER_RECT
                212 => 4,            // AGL_COLORMAP_ENTRY
                203 | 222 | 231..=236 | 501 => 1,
                _ => {
                    agl.set_error(classic_gl_agl::AGL_BAD_ENUM);
                    return boolean(false);
                }
            };
            let Some(context) = agl.context_mut(handle) else {
                agl.set_error(classic_gl_agl::AGL_BAD_CONTEXT);
                return boolean(false);
            };
            if pointer == 0 {
                agl.set_error(classic_gl_agl::AGL_BAD_POINTER);
                return boolean(false);
            }
            if function == F::SetInteger {
                let mut values = [0; 4];
                for (index, value) in values.iter_mut().enumerate().take(count) {
                    *value = memory.read_u32_be(pointer + index as u32 * 4).unwrap_or(0) as i32;
                }
                context.integers.insert(pname, values);
            } else {
                let values = context.integers.get(&pname).copied().unwrap_or_default();
                for (index, value) in values.iter().enumerate().take(count) {
                    let _ = memory.write_u32_be(pointer + index as u32 * 4, *value as u32);
                }
            }
            boolean(true)
        }
        F::ErrorString => {
            let text = match cpu.gpr[3] {
                0 => "no error",
                10000 => "invalid pixel format attribute",
                10001 => "invalid renderer property",
                10002 => "invalid pixel format",
                10003 => "invalid renderer info",
                10004 => "invalid context",
                10005 => "invalid drawable",
                10006 => "invalid graphics device",
                10007 => "invalid context state",
                10008 => "invalid numerical value",
                10009 => "invalid share context",
                10010 => "invalid enumerant",
                10011 => "invalid offscreen drawable",
                10012 => "invalid fullscreen drawable",
                10013 => "invalid window",
                10014 => "invalid pointer",
                10015 => "invalid code module",
                10016 => "memory allocation failure",
                _ => return PpcImportAction::Return(0),
            };
            let address = super::classic_gl::GL_STRING_AREA + 0x380;
            let mut bytes = text.as_bytes().to_vec();
            bytes.push(0);
            PpcImportAction::Return(if memory.write_bytes(address, &bytes).is_some() { address } else { 0 })
        }
        F::ResetLibrary => {
            *agl = PpcAglState::default();
            PpcImportAction::ReturnPreserve
        }
        F::UseFont => {
            let handle = cpu.gpr[3];
            let previous = agl.current_context();
            if handle == 0 || !agl.set_current_context(handle) {
                agl.set_error(classic_gl_agl::AGL_BAD_CONTEXT);
                return boolean(false);
            }
            let result = agl.context_mut(handle).unwrap().gl.use_font(
                cpu.gpr[4] as i16,
                cpu.gpr[5] as u8,
                cpu.gpr[6] as i32,
                cpu.gpr[7] as i32,
                cpu.gpr[8] as i32,
                cpu.gpr[9],
            );
            let _ = agl.set_current_context(previous);
            if let Err(error) = result {
                agl.set_error(error);
            }
            boolean(result.is_ok())
        }
        F::SetFullScreen => {
            let screen = ppc_live_front_buffer_for_gworld(memory, gworlds, PPC_MAIN_GWORLD);
            let (width, height) = (cpu.gpr[4], cpu.gpr[5]);
            let fits = screen.is_some_and(|screen| {
                (width == 0 || width == screen.width) && (height == 0 || height == screen.height)
            });
            if !fits || !agl.set_full_screen(cpu.gpr[3], screen) {
                agl.set_error(classic_gl_agl::AGL_BAD_FULLSCREEN);
                return boolean(false);
            }
            boolean(true)
        }
        F::SetOffScreen => {
            let (handle, width, height, row_bytes, base) =
                (cpu.gpr[3], cpu.gpr[4], cpu.gpr[5], cpu.gpr[6], cpu.gpr[7]);
            let pixel_bits = agl
                .context(handle)
                .and_then(|context| context.format.pixel_bits)
                .unwrap_or(32);
            let surface = PpcFrontBuffer {
                base_addr: base,
                row_bytes,
                width,
                height,
                depth: if pixel_bits <= 16 { 16 } else { 32 },
            };
            let ok = base != 0
                && row_bytes >= width.saturating_mul(surface.depth / 8)
                && agl.set_off_screen(handle, surface);
            if !ok {
                agl.set_error(classic_gl_agl::AGL_BAD_VALUE);
            }
            boolean(ok)
        }
        F::CopyContext => {
            let (source, destination) = (cpu.gpr[3], cpu.gpr[4]);
            let Some(raster) = agl.context(source).map(|context| context.gl.state.raster.clone()) else {
                agl.set_error(classic_gl_agl::AGL_BAD_CONTEXT);
                return boolean(false);
            };
            match agl.context_mut(destination) {
                Some(context) => {
                    context.gl.state.raster = raster;
                    boolean(true)
                }
                None => {
                    agl.set_error(classic_gl_agl::AGL_BAD_CONTEXT);
                    boolean(false)
                }
            }
        }
        F::SetVirtualScreen => boolean(cpu.gpr[4] == 0),
        F::GetVirtualScreen => PpcImportAction::Return(0),
    }
}

fn ppc_agl_set_drawable(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    agl: &mut PpcAglState,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
) -> bool {
    let drawable = cpu.gpr[4];
    let surface = (drawable != 0)
        .then(|| ppc_agl_window_surface(memory, gworlds, window_list, drawable))
        .flatten();
    if drawable != 0 && surface.is_none() {
        return false;
    }
    agl.set_drawable(cpu.gpr[3], drawable, surface)
}

fn ppc_agl_update_context(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    agl: &mut PpcAglState,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
) -> bool {
    let Some(context) = agl.context(cpu.gpr[3]) else {
        return false;
    };
    let drawable = context.drawable;
    let surface = (drawable != 0)
        .then(|| ppc_agl_window_surface(memory, gworlds, window_list, drawable))
        .flatten();
    agl.update_context(cpu.gpr[3], surface)
}

fn ppc_agl_swap_buffers(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    agl: &mut PpcAglState,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
) -> bool {
    let Some(context) = agl.context(cpu.gpr[3]) else {
        return false;
    };
    let Some(surface) = ppc_agl_context_surface(memory, gworlds, window_list, context) else {
        return false;
    };
    agl.swap_buffers(cpu.gpr[3], memory, surface)
}

#[cfg(test)]
mod agl_choose_tests {
    use super::super::classic_gl_agl::PpcAglPixelFormatRequest;
    use super::super::classic_gl_framebuffer::ClassicGlColorBuffer;
    use super::*;
    use ppc::PpcMemory;

    #[test]
    fn imported_agl_choose_uses_guest_attributes_and_tracks_lifetime() {
        assert_eq!(
            dispatcher_target_for_import("OpenGLLibrary", "aglChoosePixelFormat"),
            PpcImportDispatcherTarget::AglChoosePixelFormat
        );
        assert_eq!(
            dispatcher_target_for_import("OpenGLLibrary", "aglDestroyPixelFormat"),
            PpcImportDispatcherTarget::AglDestroyPixelFormat
        );
        assert_eq!(
            dispatcher_target_for_import("OpenGLLibrary", "aglDescribePixelFormat"),
            PpcImportDispatcherTarget::AglDescribePixelFormat
        );
        let mut memory = PpcSectionMem::new();
        memory.add_region(
            0x1000,
            [4u32, 5, 12, 24, 0]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        );
        let mut cpu = PpcCpu::new();
        cpu.gpr[5] = 0x1000;
        let mut agl = PpcAglState::default();
        let handle = ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]);
        assert_ne!(handle, 0);
        assert!(agl.pixel_format(handle).unwrap().request.double_buffered);
        memory.add_region(0x2000, vec![0xff; 4]);
        cpu.gpr[3] = handle;
        cpu.gpr[4] = 12; // AGL_DEPTH_SIZE
        cpu.gpr[5] = 0x2000;
        assert!(ppc_agl_describe_pixel_format(&cpu, &mut memory, &mut agl));
        assert_eq!(memory.read_u32_be(0x2000), Some(24));
        agl.destroy_pixel_format(handle);
        assert!(agl.pixel_format(handle).is_none());

        // A device constraint is not silently ignored.
        cpu.gpr[4] = 1;
        assert_eq!(
            ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]),
            0
        );
        assert_eq!(agl.get_error(), classic_gl_agl::AGL_BAD_POINTER);
    }

    #[test]
    fn agl_error_import_latches_first_error_and_clears_on_read() {
        assert_eq!(
            dispatcher_target_for_import("OpenGLLibrary", "aglGetError"),
            PpcImportDispatcherTarget::AglGetError
        );
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 16]);
        let mut cpu = PpcCpu::new();
        let mut agl = PpcAglState::default();
        cpu.gpr[4] = u32::MAX;
        assert_eq!(
            ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]),
            0
        );
        cpu.gpr[4] = 0;
        cpu.gpr[5] = 0;
        assert_eq!(
            ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]),
            0
        );
        assert_eq!(agl.get_error(), classic_gl_agl::AGL_BAD_VALUE);
        assert_eq!(agl.get_error(), 0);

        // A valid request for an unavailable stereo format is a clean
        // no-match, distinct from malformed guest inputs.
        memory.add_region(
            0x2000,
            [4u32, 6, 0]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        );
        cpu.gpr[5] = 0x2000;
        assert_eq!(
            ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]),
            0
        );
        assert_eq!(agl.get_error(), 0);

        memory.add_region(
            0x3000,
            [99u32, 0]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        );
        cpu.gpr[5] = 0x3000;
        assert_eq!(
            ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]),
            0
        );
        assert_eq!(agl.get_error(), classic_gl_agl::AGL_BAD_ATTRIBUTE);

        cpu.gpr[3] = 0x1000;
        cpu.gpr[4] = 1;
        cpu.gpr[5] = 0x2000;
        assert_eq!(
            ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]),
            0
        );
        assert_eq!(agl.get_error(), classic_gl_agl::AGL_BAD_GDEV);
    }

    #[test]
    fn agl_drawable_requires_window_port_and_swaps_into_its_guest_pixels() {
        let port = 0x2000;
        let world = PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: 0x1000,
            gdevice: PPC_MAIN_GDEVICE,
            width: 1,
            height: 1,
            depth: 16,
            row_bytes: 2,
            pixels_locked: false,
            pixels_no_purge: true,
        };
        let windows = SharedProcessWindowList::from_value(vec![port]);
        let mut memory = PpcSectionMem::new();
        memory.add_region(0x1000, vec![0; 2]);
        // A CGrafPort: portPixMap handle at +2, the color-port flags in the
        // portVersion word at +6, and portRect at +16. The PixMap describes
        // the one-pixel 16-bit window.
        let handle = 0x3000u32;
        let mut grafport = vec![0u8; 0x6C];
        grafport[2..6].copy_from_slice(&handle.to_be_bytes());
        grafport[6..8].copy_from_slice(&0xC000u16.to_be_bytes());
        grafport[20..24].copy_from_slice(&[0, 1, 0, 1]);
        memory.add_region(port, grafport);
        let mut pixmap_bytes = vec![0u8; 0x80];
        pixmap_bytes[0..4].copy_from_slice(&handle.wrapping_add(0x10).to_be_bytes());
        let record = &mut pixmap_bytes[0x10..];
        record[0..4].copy_from_slice(&0x1000u32.to_be_bytes());
        record[4..6].copy_from_slice(&0x8002u16.to_be_bytes());
        record[10..14].copy_from_slice(&[0, 1, 0, 1]);
        record[32..34].copy_from_slice(&16u16.to_be_bytes());
        memory.add_region(handle, pixmap_bytes);
        assert_eq!(
            ppc_agl_window_surface(&mut memory, &[world], &windows, port).map(|s| (s.base_addr, s.width, s.height)),
            Some((0x1000, 1, 1))
        );
        assert!(
            ppc_agl_window_surface(&mut memory, &[world], &windows, PPC_DSP_BACK_GWORLD,).is_none()
        );
        let mut agl = PpcAglState::default();
        let format = agl.choose_pixel_format(PpcAglPixelFormatRequest {
            rgba: true,
            double_buffered: true,
            ..Default::default()
        });
        let context = agl.create_context(format, 0);
        let mut cpu = PpcCpu::new();
        cpu.gpr[3] = context;
        cpu.gpr[4] = port;
        assert!(ppc_agl_set_drawable(
            &cpu,
            &mut memory,
            &mut agl,
            &[world],
            &windows
        ));
        assert!(agl
            .context_mut(context)
            .unwrap()
            .framebuffer
            .as_mut()
            .unwrap()
            .clear_color(ClassicGlColorBuffer::Back, [0, 255, 0, 255]));
        assert!(ppc_agl_swap_buffers(
            &cpu,
            &mut memory,
            &mut agl,
            &[world],
            &windows
        ));
        assert_eq!(memory.read_u16_be(0x1000), Some(0x03e0));
    }
}

fn ppc_dispatch_slot_compatibility(
    _binding: &PpcImportBinding,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    if cpu.gpr[3] != 0 {
        let _ = memory.write_u32_be(cpu.gpr[3], 0);
    }
    PpcImportAction::Return(ppc_i16_result(PPC_SM_NO_MORE_SRSRCS_ERR))
}

#[allow(clippy::too_many_arguments)]
fn ppc_dispatch_object_support_compatibility(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    apple_events: &mut PpcAppleEventState,
) -> PpcImportAction {
    let result_ptr = cpu.gpr[8];
    if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, 8) {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    let Some(container) =
        dispatch_apple_events::ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[4])
    else {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    let Some(key_data) =
        dispatch_apple_events::ppc_ae_descriptor(memory, &apple_events.descriptors, cpu.gpr[6])
    else {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    };
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&cpu.gpr[3].to_be_bytes());
    data.extend_from_slice(&cpu.gpr[5].to_be_bytes());
    data.extend_from_slice(&cpu.gpr[6].to_be_bytes());
    let result = ppc_create_process_owned_ae_desc(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        Some(&apple_events.descriptors),
        result_ptr,
        u32::from_be_bytes(*b"obj "),
        &data,
    );
    if result == PPC_NO_ERR {
        let desired_class = cpu.gpr[3];
        let key_form = cpu.gpr[5];
        let mut fields = HashMap::new();
        // Object specifier keywords and descriptor types follow
        // Interapplication Communication (1993), pp. 6-75--6-77.
        for (keyword, desc_type, value) in [
            (u32::from_be_bytes(*b"want"), PPC_TYPE_TYPE, desired_class),
            (u32::from_be_bytes(*b"form"), PPC_TYPE_ENUMERATED, key_form),
        ] {
            fields.insert(
                keyword,
                ProcessAeDescriptor {
                    attributes: Default::default(),
                    desc_type,
                    data: value.to_be_bytes().to_vec(),
                    fields: HashMap::new(),
                    items: Vec::new(),
                },
            );
        }
        fields.insert(u32::from_be_bytes(*b"from"), container);
        fields.insert(u32::from_be_bytes(*b"seld"), key_data);
        let handle = memory.read_u32_be(result_ptr + 4).unwrap_or(0);
        apple_events.descriptors.with_mut(|state| {
            if let Some(descriptor) = state.descriptors.get_mut(&result_ptr) {
                descriptor.fields = fields.clone();
            }
            if let Some(descriptor) = state.backing.get_mut(&handle) {
                descriptor.fields = fields;
            }
        });
        // CreateObjSpecifier owns its input descriptors when disposeInputs is
        // TRUE. Inside Macintosh: Interapplication Communication (1993), 6-93.
        if cpu.gpr[7] != 0 {
            let mut disposed_handles = Vec::new();
            for input in [cpu.gpr[4], cpu.gpr[6]] {
                let handle = memory.read_u32_be(input + 4).unwrap_or(0);
                if handle != 0 && !disposed_handles.contains(&handle) {
                    let _ = ppc_dispose_process_native_handle(
                        process_memory_manager,
                        memory,
                        heap_cursor,
                        heap_limit,
                        last_mem_error,
                        handles,
                        handle,
                    );
                    disposed_handles.push(handle);
                }
                let _ = memory.write_u32_be(input, 0);
                let _ = memory.write_u32_be(input + 4, 0);
                apple_events.descriptors.with_mut(|state| {
                    state.descriptors.remove(&input);
                    state.events.remove(&input);
                });
            }
        }
    }
    PpcImportAction::Return(ppc_i16_result(result))
}
