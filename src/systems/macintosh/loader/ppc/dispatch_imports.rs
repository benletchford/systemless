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
    pub(crate) input: PpcInputSnapshot,
    pub(crate) event_queue: &'a mut EventQueue,
    pub(crate) draw_sprocket: &'a mut PpcDrawSprocketState,
}

pub(crate) fn dispatch_supported_import(
    context: PpcDispatchContext<'_>,
) -> Option<PpcImportAction> {
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
        input,
        event_queue,
        draw_sprocket,
    } = context;
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
    if let Some(action) = dispatch_inputsprocket::dispatch_inputsprocket_import(
        dispatch_inputsprocket::PpcInputSprocketDispatchContext {
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
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_desk::dispatch_desk_import(dispatch_desk::PpcDeskDispatchContext { binding })
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
        PpcImportDispatcherTarget::Collection(_) => {
            unreachable!("collection imports return through dispatch_collection_import")
        }
        PpcImportDispatcherTarget::InstallExceptionHandler => {
            unreachable!("native exception imports return through dispatch_native_exception_import")
        }
        PpcImportDispatcherTarget::RegisterAppearanceClient
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
        | PpcImportDispatcherTarget::NewHandle { .. }
        | PpcImportDispatcherTarget::TempNewHandle
        | PpcImportDispatcherTarget::TempDisposeHandle
        | PpcImportDispatcherTarget::HoldMemory
        | PpcImportDispatcherTarget::UnholdMemory
        | PpcImportDispatcherTarget::DisposeHandle
        | PpcImportDispatcherTarget::EmptyHandle
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
        | PpcImportDispatcherTarget::MaxApplZone
        | PpcImportDispatcherTarget::MoreMasters
        | PpcImportDispatcherTarget::FlushCodeCache
        | PpcImportDispatcherTarget::GetApplLimit
        | PpcImportDispatcherTarget::SetApplLimit
        | PpcImportDispatcherTarget::HeapFreeBytes
        | PpcImportDispatcherTarget::MaxMem
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
        | PpcImportDispatcherTarget::SetItemMark
        | PpcImportDispatcherTarget::CheckItem
        | PpcImportDispatcherTarget::GetMenuBar
        | PpcImportDispatcherTarget::GetNewMBar
        | PpcImportDispatcherTarget::ClearMenuBar
        | PpcImportDispatcherTarget::SetMenuBar
        | PpcImportDispatcherTarget::GetMenuHandle
        | PpcImportDispatcherTarget::DrawMenuBar
        | PpcImportDispatcherTarget::InvalMenuBar
        | PpcImportDispatcherTarget::FlashMenuBar
        | PpcImportDispatcherTarget::HMGetHelpMenuHandle
        | PpcImportDispatcherTarget::HMGetBalloons
        | PpcImportDispatcherTarget::HiliteMenu
        | PpcImportDispatcherTarget::MenuNoop
        | PpcImportDispatcherTarget::MenuKey
        | PpcImportDispatcherTarget::MenuEvent
        | PpcImportDispatcherTarget::MenuChoice
        | PpcImportDispatcherTarget::GetMBarHeight
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
        | PpcImportDispatcherTarget::TEAutoView
        | PpcImportDispatcherTarget::TECopy { .. }
        | PpcImportDispatcherTarget::TEPaste { .. }
        | PpcImportDispatcherTarget::TETransferScrap { .. }
        | PpcImportDispatcherTarget::TEScrapHandle
        | PpcImportDispatcherTarget::TEScrapLength { .. } => {
            unreachable!("textedit imports return through dispatch_textedit_import")
        }
        PpcImportDispatcherTarget::DSpStartup
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
        | PpcImportDispatcherTarget::DSpContextReserve
        | PpcImportDispatcherTarget::DSpContextRelease
        | PpcImportDispatcherTarget::DSpContextSetState
        | PpcImportDispatcherTarget::DSpContextGetState
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
        | PpcImportDispatcherTarget::ISpElementListAddElements
        | PpcImportDispatcherTarget::ISpElementListGetNextEvent
        | PpcImportDispatcherTarget::ISpElementListFlush
        | PpcImportDispatcherTarget::ISpDevicesExtract
        | PpcImportDispatcherTarget::ISpDevicesExtractByClass
        | PpcImportDispatcherTarget::ISpDeviceGetDefinition
        | PpcImportDispatcherTarget::ISpDeviceGetElementList
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
        | PpcImportDispatcherTarget::CurResFile
        | PpcImportDispatcherTarget::UseResFile
        | PpcImportDispatcherTarget::OpenResFile
        | PpcImportDispatcherTarget::HOpenResFile
        | PpcImportDispatcherTarget::ResError
        | PpcImportDispatcherTarget::GetVol
        | PpcImportDispatcherTarget::GetWDInfo
        | PpcImportDispatcherTarget::HGetVol
        | PpcImportDispatcherTarget::HSetVol
        | PpcImportDispatcherTarget::FlushVol
        | PpcImportDispatcherTarget::PBFlushVol
        | PpcImportDispatcherTarget::PBHGetVInfo
        | PpcImportDispatcherTarget::GetVInfo
        | PpcImportDispatcherTarget::PBDTGetPath
        | PpcImportDispatcherTarget::PBDTGetCommentSync
        | PpcImportDispatcherTarget::PBGetFInfo
        | PpcImportDispatcherTarget::PBHGetFInfo
        | PpcImportDispatcherTarget::PBSetFInfo
        | PpcImportDispatcherTarget::PBHSetFInfo
        | PpcImportDispatcherTarget::FSpGetFInfo
        | PpcImportDispatcherTarget::GetFInfo
        | PpcImportDispatcherTarget::HGetFInfo
        | PpcImportDispatcherTarget::FSpSetFInfo
        | PpcImportDispatcherTarget::HSetFInfo
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
        | PpcImportDispatcherTarget::UpdateAlias
        | PpcImportDispatcherTarget::NewAlias
        | PpcImportDispatcherTarget::NewAliasMinimalFromFullPath
        | PpcImportDispatcherTarget::FileCompatibility(_) => {
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
        | PpcImportDispatcherTarget::CloseResFile
        | PpcImportDispatcherTarget::GetPicture
        | PpcImportDispatcherTarget::GetIconSuite
        | PpcImportDispatcherTarget::GetIcon
        | PpcImportDispatcherTarget::GetPattern
        | PpcImportDispatcherTarget::GetIndPattern
        | PpcImportDispatcherTarget::GetPixPat
        | PpcImportDispatcherTarget::GetIntlResource => {
            unreachable!("resource imports return through dispatch_resource_import")
        }
        PpcImportDispatcherTarget::InitGraf
        | PpcImportDispatcherTarget::GetForeColor
        | PpcImportDispatcherTarget::GetBackColor
        | PpcImportDispatcherTarget::ForeColor
        | PpcImportDispatcherTarget::BackColor
        | PpcImportDispatcherTarget::RGBForeColor
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
        | PpcImportDispatcherTarget::InsetRect => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::MathCeil
        | PpcImportDispatcherTarget::MathSqrt
        | PpcImportDispatcherTarget::MathExp
        | PpcImportDispatcherTarget::MathSin
        | PpcImportDispatcherTarget::MathCos
        | PpcImportDispatcherTarget::MathRound
        | PpcImportDispatcherTarget::MathRint
        | PpcImportDispatcherTarget::MathAsin
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
        | PpcImportDispatcherTarget::TextSize => {
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
        | PpcImportDispatcherTarget::PaintArc => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameRgn
        | PpcImportDispatcherTarget::PaintRgn
        | PpcImportDispatcherTarget::FillRgn
        | PpcImportDispatcherTarget::InvertRgn => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameRoundRect | PpcImportDispatcherTarget::PaintRoundRect => {
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
        | PpcImportDispatcherTarget::EmptyRgn
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
        PpcImportDispatcherTarget::CopyBits => {
            unreachable!("bit-transfer imports return through dispatch_bit_transfer_import")
        }
        PpcImportDispatcherTarget::OpenPoly
        | PpcImportDispatcherTarget::ClosePoly
        | PpcImportDispatcherTarget::KillPoly
        | PpcImportDispatcherTarget::FramePoly
        | PpcImportDispatcherTarget::PaintPoly
        | PpcImportDispatcherTarget::FillPoly => {
            unreachable!("Polygon Manager imports return through dispatch_polygon_import")
        }
        PpcImportDispatcherTarget::NewCWindow
        | PpcImportDispatcherTarget::GetNewCWindow
        | PpcImportDispatcherTarget::GetWRefCon
        | PpcImportDispatcherTarget::SetWRefCon
        | PpcImportDispatcherTarget::GetWindowPic
        | PpcImportDispatcherTarget::SetWindowPic
        | PpcImportDispatcherTarget::GetAuxWin
        | PpcImportDispatcherTarget::LMGetWindowList
        | PpcImportDispatcherTarget::LMSetWindowList
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
        PpcImportDispatcherTarget::ActivatePalette
        | PpcImportDispatcherTarget::NSetPalette
        | PpcImportDispatcherTarget::GetPalette => {
            unreachable!("palette imports return through dispatch_palette_import")
        }
        PpcImportDispatcherTarget::GetPort
        | PpcImportDispatcherTarget::GetPortBounds
        | PpcImportDispatcherTarget::SetPort
        | PpcImportDispatcherTarget::GetWindowPort
        | PpcImportDispatcherTarget::SetPortWindowPort
        | PpcImportDispatcherTarget::SetPortDialogPort
        | PpcImportDispatcherTarget::NewGWorld
        | PpcImportDispatcherTarget::UpdateGWorld
        | PpcImportDispatcherTarget::DisposeGWorld
        | PpcImportDispatcherTarget::QDError
        | PpcImportDispatcherTarget::GetGWorld
        | PpcImportDispatcherTarget::SetGWorld
        | PpcImportDispatcherTarget::GetGWorldDevice
        | PpcImportDispatcherTarget::GetGWorldPixMap
        | PpcImportDispatcherTarget::OpenPort
        | PpcImportDispatcherTarget::OpenCPort
        | PpcImportDispatcherTarget::CloseCPort
        | PpcImportDispatcherTarget::SetPortBits { .. }
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
        | PpcImportDispatcherTarget::Control
        | PpcImportDispatcherTarget::PBControl
        | PpcImportDispatcherTarget::PBStatus => {
            unreachable!("Device Manager imports return through dispatch_device_import")
        }
        PpcImportDispatcherTarget::Gestalt | PpcImportDispatcherTarget::NewGestaltValue => {
            unreachable!("Gestalt imports return through dispatch_gestalt_import")
        }
        PpcImportDispatcherTarget::GetSharedLibrary
        | PpcImportDispatcherTarget::FindSymbol
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
        | PpcImportDispatcherTarget::PutScrap
        | PpcImportDispatcherTarget::ZeroScrap
        | PpcImportDispatcherTarget::LoadScrap
        | PpcImportDispatcherTarget::UnloadScrap => {
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
        | PpcImportDispatcherTarget::OpenDeskAcc => {
            unreachable!("Desk Manager imports return through dispatch_desk_import")
        }
        PpcImportDispatcherTarget::AEInstallEventHandler
        | PpcImportDispatcherTarget::AEProcessAppleEvent => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::LNew
        | PpcImportDispatcherTarget::LDispose
        | PpcImportDispatcherTarget::LAddRow
        | PpcImportDispatcherTarget::LDelRow
        | PpcImportDispatcherTarget::LGetSelect
        | PpcImportDispatcherTarget::LSetSelect
        | PpcImportDispatcherTarget::LSetCell
        | PpcImportDispatcherTarget::LGetCell
        | PpcImportDispatcherTarget::LClick
        | PpcImportDispatcherTarget::LActivate
        | PpcImportDispatcherTarget::LSetDrawingMode
        | PpcImportDispatcherTarget::LScroll
        | PpcImportDispatcherTarget::LSize
        | PpcImportDispatcherTarget::LUpdate
        | PpcImportDispatcherTarget::LAutoScroll
        | PpcImportDispatcherTarget::LSearch => {
            unreachable!("list imports return through dispatch_list_import")
        }
        PpcImportDispatcherTarget::SysEnvirons
        | PpcImportDispatcherTarget::SVersion
        | PpcImportDispatcherTarget::EqualString
        | PpcImportDispatcherTarget::NumToString
        | PpcImportDispatcherTarget::StringToNum
        | PpcImportDispatcherTarget::Random
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
        | PpcImportDispatcherTarget::FontMetrics
        | PpcImportDispatcherTarget::GetFNum => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::AESetInteractionAllowed
        | PpcImportDispatcherTarget::AEGetInteractionAllowed => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::StdMemset
        | PpcImportDispatcherTarget::StdMemcmp
        | PpcImportDispatcherTarget::StdMemcpy
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
        | PpcImportDispatcherTarget::StdGetenv
        | PpcImportDispatcherTarget::StdSprintf => {
            unreachable!("stdc imports return through dispatch_stdc_import")
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
        | PpcImportDispatcherTarget::P2CStr
        | PpcImportDispatcherTarget::C2PStr
        | PpcImportDispatcherTarget::CopyCStringToPascal
        | PpcImportDispatcherTarget::CopyPascalStringToC
        | PpcImportDispatcherTarget::UpperText => {
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
        | PpcImportDispatcherTarget::CfBundleLoadExecutable => {
            unreachable!("Core Foundation imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::GetCurrentProcess
        | PpcImportDispatcherTarget::WakeUpProcess
        | PpcImportDispatcherTarget::SameProcess
        | PpcImportDispatcherTarget::GetProcessInformation => {
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
        PpcImportDispatcherTarget::QADeviceGetNextEngine => Some(PpcImportAction::Return(0)),
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
        | PpcImportDispatcherTarget::NewEventLoopTimerUPP
        | PpcImportDispatcherTarget::DisposeEventLoopTimerUPP
        | PpcImportDispatcherTarget::NewControlActionUPP
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
            ))
        }
        PpcImportDispatcherTarget::AppleTalkCompatibility(operation) => Some(
            dispatch_appletalk::ppc_dispatch_appletalk_compatibility(operation, cpu, memory),
        ),
        PpcImportDispatcherTarget::PrintingCompatibility(operation) => Some(
            dispatch_printing::ppc_dispatch_printing_compatibility(operation),
        ),
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
        | PpcImportDispatcherTarget::SndDisposeChannel
        | PpcImportDispatcherTarget::SndPlay
        | PpcImportDispatcherTarget::SndChannelStatus
        | PpcImportDispatcherTarget::SndGetInfo
        | PpcImportDispatcherTarget::SndSetInfo
        | PpcImportDispatcherTarget::ParseSndHeader
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
        PpcImportDispatcherTarget::SpeechCompatibility(operation) => {
            Some(ppc_dispatch_speech_compatibility(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::QuickTimeCompatibility(operation) => Some(
            dispatch_quicktime_compatibility(
                operation,
                cpu,
                memory,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                handles,
                quicktime,
                sound,
                files.as_slice(),
                vfs_files.as_slice(),
            ),
        ),
        PpcImportDispatcherTarget::InputSprocketCompatibility(_) => {
            unreachable!(
                "input sprocket compatibility imports return through dispatch_inputsprocket_import"
            )
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
            ))
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
        | PpcImportDispatcherTarget::GetMouse => {
            unreachable!("event imports return through dispatch_event_import")
        }
        PpcImportDispatcherTarget::LMGetMenuList
        | PpcImportDispatcherTarget::LMSetMenuHook
        | PpcImportDispatcherTarget::LMGetMenuFlash
        | PpcImportDispatcherTarget::LMGetPaintWhite
        | PpcImportDispatcherTarget::LMGetSysMap
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
        | PpcImportDispatcherTarget::LMGetRndSeed
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
        | PpcImportDispatcherTarget::GetDblTime
        | PpcImportDispatcherTarget::LMGetTime
        | PpcImportDispatcherTarget::SecondsToDate
        | PpcImportDispatcherTarget::Microseconds
        | PpcImportDispatcherTarget::AbsoluteToNanoseconds => {
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
        PpcImportDispatcherTarget::AglCreateContext => Some(PpcImportAction::Return(
            agl.create_context(cpu.gpr[3], cpu.gpr[4]),
        )),
        PpcImportDispatcherTarget::AglDestroyContext => Some(PpcImportAction::Return(u32::from(
            agl.destroy_context(cpu.gpr[3]),
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
            ppc_agl_swap_buffers(cpu, memory, agl, gworlds, window_list)
                .then_some(PpcImportAction::ReturnPreserve)
        }
        PpcImportDispatcherTarget::NoOpPreserve => Some(PpcImportAction::ReturnPreserve),
        PpcImportDispatcherTarget::ExitToShell => Some(PpcImportAction::Halt),
        PpcImportDispatcherTarget::GlmSetMode
        | PpcImportDispatcherTarget::GlmSetFunc
        | PpcImportDispatcherTarget::GlmMalloc
        | PpcImportDispatcherTarget::GlmCalloc
        | PpcImportDispatcherTarget::GlmRealloc
        | PpcImportDispatcherTarget::GlmFree
        | PpcImportDispatcherTarget::GlmPageFreeAll
        | PpcImportDispatcherTarget::GlmGetError => {
            unreachable!("OpenGL memory imports return through the fast dispatcher")
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

fn ppc_agl_window_surface(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    window_list: &SharedProcessWindowList,
    drawable: u32,
) -> Option<PpcFrontBuffer> {
    // Apple Technical Q&A OGL02 (2000): an AGLDrawable is a window CGrafPtr;
    // a DrawSprocket front buffer is not a valid AGL drawable.
    // https://leopard-adc.pepas.com/qa/ogl/ogl02.html
    if drawable == 0
        || matches!(drawable, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD)
        || !window_list.contains_window(drawable)
    {
        return None;
    }
    ppc_live_front_buffer_for_gworld(memory, gworlds, drawable)
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
    let Some(surface) = ppc_agl_window_surface(memory, gworlds, window_list, context.drawable)
    else {
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
        assert_eq!(ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]), 0);
        cpu.gpr[4] = 0;
        cpu.gpr[5] = 0;
        assert_eq!(ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]), 0);
        assert_eq!(agl.get_error(), classic_gl_agl::AGL_BAD_VALUE);
        assert_eq!(agl.get_error(), 0);

        // A valid request for an unavailable accelerated format is a clean
        // no-match, distinct from malformed guest inputs.
        memory.add_region(
            0x2000,
            [4u32, 73, 0]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        );
        cpu.gpr[5] = 0x2000;
        assert_eq!(ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]), 0);
        assert_eq!(agl.get_error(), 0);

        memory.add_region(
            0x3000,
            [99u32, 0]
                .iter()
                .flat_map(|word| word.to_be_bytes())
                .collect(),
        );
        cpu.gpr[5] = 0x3000;
        assert_eq!(ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]), 0);
        assert_eq!(agl.get_error(), classic_gl_agl::AGL_BAD_ATTRIBUTE);

        cpu.gpr[3] = 0x1000;
        cpu.gpr[4] = 1;
        cpu.gpr[5] = 0x2000;
        assert_eq!(ppc_agl_choose_pixel_format(&cpu, &mut memory, &mut agl, &[]), 0);
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
        assert!(ppc_agl_window_surface(&mut memory, &[world], &windows, port).is_some());
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
) -> PpcImportAction {
    let result_ptr = cpu.gpr[8];
    if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, 8) {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
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
        None,
        result_ptr,
        u32::from_be_bytes(*b"obj "),
        &data,
    );
    PpcImportAction::Return(ppc_i16_result(result))
}
