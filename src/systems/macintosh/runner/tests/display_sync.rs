use super::*;
use ppc::PpcCpu;

#[test]
fn classic_window_overlay_spans_clip_native_frame_rows() {
    let overlays = [(10, 20, 30, 50), (15, 45, 25, 70)];
    assert!(FixtureRunner::ppc_overlay_spans_for_row(&overlays, 9, 10, 60).is_empty());
    assert_eq!(
        FixtureRunner::ppc_overlay_spans_for_row(&overlays, 20, 10, 60),
        vec![(10, 40), (35, 60)]
    );
    assert_eq!(
        FixtureRunner::ppc_overlay_spans_for_row(&overlays, 29, 10, 60),
        vec![(10, 40)]
    );
}

#[test]
fn ppc_gui_cpu_slice_defers_front_buffer_sync_until_composite() {
    let front_base = PPC_HEAP_BASE;
    let presented_base = PPC_HEAP_BASE + 4;
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_CODE_BASE, 0x4800_0002u32.to_be_bytes().to_vec());
    memory.add_region(front_base, vec![0x00, 0x1f, 0x00, 0x1f]);
    memory.add_region(presented_base, vec![0x7c, 0x00, 0x03, 0xe0]);
    let mut cpu = PpcCpu::new();
    cpu.pc = PPC_CODE_BASE;
    cpu.lr = PPC_HALT_PC;
    cpu.gpr[1] = PPC_STACK_TOP - 64;

    let app = LoadedApp::from_ppc(PpcLoadedApp {
        cpu,
        memory,
        entry_pc: PPC_CODE_BASE,
        rtoc: 0,
        stack_base: PPC_STACK_BASE,
        stack_size: PPC_STACK_SIZE,
        stack_pointer: PPC_STACK_TOP - 64,
        launch_partition_storage: Default::default(),
        tick_state: SharedProcessTickState::default(),
        clock_cycles_per_tick: 1,
        clock_cycle_phase: 0,
        trap_default_gateways: Default::default(),
        native_exception_handler: 0,
        native_exception_stack: Vec::new(),
        stdc_qsort_stack: Vec::new(),
        dialog_callback_stack: Vec::new(),
        collection_callback_stack: Vec::new(),
        pending_file_completions: VecDeque::new(),
        file_completion_context: None,
        parked_interrupt_callback: None,
        interrupt_callback_parks: 0,
        apple_events: Default::default(),
        cfm: Some(crate::cfm::CfmState::default()),
        controls: Default::default(),
        screen_clut: SharedProcessDisplayClut::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
        display_gamma: SharedProcessDisplayGamma::default(),
        process_quickdraw_port_state_attached: false,
        color_manager_clut: SharedProcessDisplayClut::from_value(
            TrapDispatcher::standard_mac_8bpp_clut(),
        ),
        aliases: Vec::new(),
        agl: Default::default(),
        gworlds: vec![
            PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: PPC_MAIN_GWORLD,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: front_base,
                gdevice: PPC_MAIN_GDEVICE,
                width: 2,
                height: 1,
                depth: 16,
                row_bytes: 4,
                pixels_locked: false,
                pixels_no_purge: false,
            },
            PpcGWorldRecord {
                ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
                port: PPC_DSP_BACK_GWORLD,
                pixmap_handle: 0,
                pixmap: 0,
                base_addr: presented_base,
                gdevice: PPC_MAIN_GDEVICE,
                width: 2,
                height: 1,
                depth: 16,
                row_bytes: 4,
                pixels_locked: false,
                pixels_no_purge: false,
            },
        ],
        gworld_pixel_states: Default::default(),
        q3_objects: Vec::new(),
        q3_object_refs: Vec::new(),
        next_q3_object: 0,
        q3_error_state: Default::default(),
        q3_lifecycle: Default::default(),
        q3_memory_storages: Vec::new(),
        q3_files: Vec::new(),
        q3_group_memberships: Vec::new(),
        q3_file_groups: Vec::new(),
        q3_views: Vec::new(),
        q3_submissions: Vec::new(),
        q3_view_transforms: Vec::new(),
        q3_submission_transforms: Vec::new(),
        q3_view_materials: Vec::new(),
        q3_submission_materials: Vec::new(),
        q3_submission_lights: Vec::new(),
        q3_view_state_stack: Vec::new(),
        q3_completed_frames: Vec::new(),
        q3_retained_frames: Vec::new(),
        q3_state_only_completed_frame_batches: Vec::new(),
        q3_fog_styles: Vec::new(),
        q3_immediate_trimeshes: Default::default(),
        q3_attributes: Vec::new(),
        q3_shader_uv_transforms: Vec::new(),
        q3_shader_boundaries: Vec::new(),
        q3_mipmap_textures: Vec::new(),
        q3_texture_shaders: Vec::new(),
        q3_renderer_preferences: Vec::new(),
        q3_draw_contexts: Vec::new(),
        q3_trimeshes: Vec::new(),
        q3_styles: Vec::new(),
        q3_cameras: Vec::new(),
        q3_lights: Vec::new(),
        input_sprocket: Default::default(),
        input_sprocket_virtual_elements: Vec::new(),
        toolbox_startup: Default::default(),
        quicktime: Default::default(),
        sound: Default::default(),
        timer_tasks: Default::default(),
        vbl_tasks: Default::default(),
        callback_scheduling: Default::default(),
        process_file_system: ppc_initial_process_file_system(),
        current_gworld: SharedProcessGraphicsPort::from_value(PPC_MAIN_GWORLD),
        current_gdevice: SharedProcessGraphicsDevice::from_value(PPC_MAIN_GDEVICE),
        quickdraw_op_colors: Default::default(),
        quickdraw_hilite_colors: Default::default(),
        quickdraw_fore_color: PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0,
        },
        quickdraw_fore_indices: Default::default(),
        quickdraw_back_color: PpcRgbColor {
            red: 0xffff,
            green: 0xffff,
            blue: 0xffff,
        },
        quickdraw_pen_h: 0,
        quickdraw_pen_v: 0,
        quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
        quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
        cursor_state: crate::process_context::SharedProcessCursorState::default(),
        param_text: Default::default(),
        scrap: Default::default(),
        list_manager: Default::default(),
        collections: Default::default(),
        halt_pc: PPC_HALT_PC,
        import_trap_base: PPC_IMPORT_TRAP_BASE,
        import_count: 0,
        imports: Vec::new(),
        section_bases: Vec::new(),
        input: PpcInputSnapshot::default(),
        process_input: Default::default(),
        event_queue: Default::default(),
        window_list: Default::default(),
        process_memory_manager: PpcProcessMemoryManager::with_heap(
            PPC_HEAP_BASE + 8,
            PPC_STACK_BASE,
        ),
        glm_mode: None,
        glm_callbacks: [None; 8],
        glm_callback_stack: Vec::new(),
        glm_allocations: HashMap::new(),
        glm_page_free_all_queue: VecDeque::new(),
        glm_error: 0,
        draw_sprocket: PpcDrawSprocketState {
            front_buffer_gworld: PPC_DSP_BACK_GWORLD,
            back_buffer_gworld: PPC_MAIN_GWORLD,
            swap_count: 1,
            ..PpcDrawSprocketState::default()
        },
    });
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let initial_screen_mode = runner.dispatcher.screen_mode;

    let (steps, running) = runner.run_gui_cpu_slice(8, u32::MAX);

    assert_eq!(steps, 1);
    assert!(!running);
    assert_eq!(
        runner.dispatcher.screen_mode, initial_screen_mode,
        "CPU-only slices must not copy or resize the host framebuffer"
    );

    runner.composite_frame();

    let (host_base, row_bytes, width, height, depth) = runner.dispatcher.screen_mode;
    assert_eq!((row_bytes, width, height, depth), (4, 2, 1, 16));
    assert_eq!(runner.bus.read_word(host_base), 0x7c00);
    assert_eq!(runner.bus.read_word(host_base + 2), 0x03e0);
    assert_eq!(
        runner
            .bus
            .read_long(crate::memory::globals::addr::SCRN_BASE),
        host_base
    );
    assert_eq!(
        runner
            .bus
            .read_long(crate::memory::globals::addr::SCREEN_BITS),
        host_base
    );
    assert_eq!(
        runner
            .bus
            .read_word(crate::memory::globals::addr::SCREEN_BITS + 4),
        row_bytes as u16
    );

    let main_gdevice_handle = runner.dispatcher.main_gdevice_handle;
    assert_ne!(main_gdevice_handle, 0);
    let main_gdevice = runner.bus.read_long(main_gdevice_handle);
    let main_pixmap_handle = runner.bus.read_long(main_gdevice + 22);
    let main_pixmap = runner.bus.read_long(main_pixmap_handle);
    assert_eq!(runner.bus.read_long(main_pixmap), host_base);
    assert_eq!(
        runner.bus.read_word(main_pixmap + 4),
        0x8000 | row_bytes as u16
    );
    assert_eq!(runner.bus.read_word(main_pixmap + 10), height);
    assert_eq!(runner.bus.read_word(main_pixmap + 12), width);
    assert_eq!(runner.bus.read_word(main_pixmap + 30), 16);
    assert_eq!(runner.bus.read_word(main_pixmap + 32), depth);
    assert_eq!(runner.bus.read_word(main_pixmap + 34), 3);
    assert_eq!(runner.bus.read_word(main_pixmap + 36), 5);
    assert_eq!(runner.bus.read_long(main_pixmap + 42), 0);
    assert_eq!(runner.bus.read_word(main_gdevice + 4), 2);
    assert_eq!(runner.bus.read_word(main_gdevice + 38), height);
    assert_eq!(runner.bus.read_word(main_gdevice + 40), width);
    assert_eq!(
        runner.bus.read_long(main_gdevice + 42),
        u32::from(crate::display::classic_depth_mode(depth).unwrap())
    );

    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app should stay loaded");
    let mut ppc_app = native_context.adapter_mut();
    ppc_app.draw_sprocket.last_fade_percent = Some(0);
    ppc_app.draw_sprocket.last_fade_zero_color = None;
    assert_eq!(ppc_app.memory.read_u16_be(presented_base), Some(0x7c00));
    assert_eq!(ppc_app.memory.read_u16_be(presented_base + 2), Some(0x03e0));

    runner.sync_ppc_front_buffer_to_host(&mut ppc_app);

    assert_eq!(runner.bus.read_word(host_base), 0x0000);
    assert_eq!(runner.bus.read_word(host_base + 2), 0x0000);
    assert_eq!(ppc_app.memory.read_u16_be(presented_base), Some(0x7c00));
    assert_eq!(ppc_app.memory.read_u16_be(presented_base + 2), Some(0x03e0));
    runner
        .native
        .restore(native_context)
        .unwrap_or_else(|_| panic!("native context lost its owner"));
}

#[test]
fn ppc_completed_q3_frame_renders_before_host_front_buffer_sync() {
    const TRIMESH_NUM_TRIANGLES_OFFSET: u32 = 4;
    const TRIMESH_TRIANGLES_OFFSET: u32 = 8;
    const TRIMESH_NUM_POINTS_OFFSET: u32 = 36;
    const TRIMESH_POINTS_OFFSET: u32 = 40;

    let front_base = PPC_HEAP_BASE;
    let trimesh_data = PPC_DATA_BASE;
    let triangles_ptr = PPC_DATA_BASE + 0x80;
    let points_ptr = PPC_DATA_BASE + 0xc0;
    let view = 0x0100_0000;
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_CODE_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
    memory.add_region(front_base, vec![0; 8 * 16]);
    memory.add_region(PPC_DATA_BASE, vec![0; 0x200]);
    memory
        .write_u32_be(trimesh_data + TRIMESH_NUM_TRIANGLES_OFFSET, 1)
        .unwrap();
    memory
        .write_u32_be(trimesh_data + TRIMESH_TRIANGLES_OFFSET, triangles_ptr)
        .unwrap();
    memory
        .write_u32_be(trimesh_data + TRIMESH_NUM_POINTS_OFFSET, 3)
        .unwrap();
    memory
        .write_u32_be(trimesh_data + TRIMESH_POINTS_OFFSET, points_ptr)
        .unwrap();
    for (offset, value) in [
        (0, 0.0f32),
        (4, 0.0),
        (8, 0.0),
        (12, 0.0),
        (16, 0.9),
        (20, 0.0),
        (24, 0.9),
        (28, 0.0),
        (32, 0.0),
    ] {
        memory
            .write_u32_be(points_ptr + offset, value.to_bits())
            .unwrap();
    }
    memory.write_u32_be(triangles_ptr, 0).unwrap();
    memory.write_u32_be(triangles_ptr + 4, 1).unwrap();
    memory.write_u32_be(triangles_ptr + 8, 2).unwrap();
    let mut cpu = PpcCpu::new();
    cpu.pc = PPC_CODE_BASE;
    cpu.lr = PPC_HALT_PC;
    cpu.gpr[1] = PPC_STACK_TOP - 64;

    let app = LoadedApp::from_ppc(PpcLoadedApp {
        cpu,
        memory,
        entry_pc: PPC_CODE_BASE,
        rtoc: 0,
        stack_base: PPC_STACK_BASE,
        stack_size: PPC_STACK_SIZE,
        stack_pointer: PPC_STACK_TOP - 64,
        launch_partition_storage: Default::default(),
        tick_state: SharedProcessTickState::default(),
        clock_cycles_per_tick: 1,
        clock_cycle_phase: 0,
        trap_default_gateways: Default::default(),
        native_exception_handler: 0,
        native_exception_stack: Vec::new(),
        stdc_qsort_stack: Vec::new(),
        dialog_callback_stack: Vec::new(),
        collection_callback_stack: Vec::new(),
        pending_file_completions: VecDeque::new(),
        file_completion_context: None,
        parked_interrupt_callback: None,
        interrupt_callback_parks: 0,
        apple_events: Default::default(),
        cfm: Some(crate::cfm::CfmState::default()),
        controls: Default::default(),
        screen_clut: SharedProcessDisplayClut::from_value(TrapDispatcher::standard_mac_8bpp_clut()),
        display_gamma: SharedProcessDisplayGamma::default(),
        process_quickdraw_port_state_attached: false,
        color_manager_clut: SharedProcessDisplayClut::from_value(
            TrapDispatcher::standard_mac_8bpp_clut(),
        ),
        aliases: Vec::new(),
        agl: Default::default(),
        gworlds: vec![PpcGWorldRecord {
            ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
            port: PPC_MAIN_GWORLD,
            pixmap_handle: 0,
            pixmap: 0,
            base_addr: front_base,
            gdevice: PPC_MAIN_GDEVICE,
            width: 8,
            height: 8,
            depth: 16,
            row_bytes: 16,
            pixels_locked: false,
            pixels_no_purge: false,
        }],
        gworld_pixel_states: Default::default(),
        q3_objects: Vec::new(),
        q3_object_refs: Vec::new(),
        next_q3_object: 0,
        q3_error_state: Default::default(),
        q3_lifecycle: Default::default(),
        q3_memory_storages: Vec::new(),
        q3_files: Vec::new(),
        q3_group_memberships: Vec::new(),
        q3_file_groups: Vec::new(),
        q3_views: vec![PpcQ3ViewStateRecord {
            view,
            renderer: 0,
            light_group: 0,
            draw_context: 0,
            camera: 0,
            rendering_depth: 0,
            bounding_box_depth: 0,
            cancelled: false,
        }],
        q3_submissions: Vec::new(),
        q3_view_transforms: Vec::new(),
        q3_submission_transforms: Vec::new(),
        q3_view_materials: Vec::new(),
        q3_submission_materials: Vec::new(),
        q3_submission_lights: Vec::new(),
        q3_view_state_stack: Vec::new(),
        q3_completed_frames: vec![PpcQ3CompletedFrameRecord {
            view,
            submissions: vec![PpcQ3SubmissionRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
            }],
            submission_transforms: vec![PpcQ3SubmissionTransformRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                local_to_world: identity,
            }],
            submission_materials: vec![PpcQ3SubmissionMaterialRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                shader: 0,
                illumination_type: u32::from_be_bytes(*b"phil"),
                styles: Vec::new(),
                fog_style: None,
                attributes: Vec::new(),
                shader_uv_transform: None,
                shader_boundary: None,
                texture_shader: None,
                mipmap_texture: None,
            }],
            submission_lights: vec![PpcQ3SubmissionLightRecord {
                view,
                kind: PpcQ3SubmissionKind::TriMesh,
                primary: trimesh_data,
                secondary: 0,
                light_group: 0,
                lights: Vec::new(),
            }],
            retained_trimeshes: Vec::new(),
        }],
        q3_retained_frames: Vec::new(),
        q3_state_only_completed_frame_batches: Vec::new(),
        q3_fog_styles: Vec::new(),
        q3_immediate_trimeshes: Default::default(),
        q3_attributes: Vec::new(),
        q3_shader_uv_transforms: Vec::new(),
        q3_shader_boundaries: Vec::new(),
        q3_mipmap_textures: Vec::new(),
        q3_texture_shaders: Vec::new(),
        q3_renderer_preferences: Vec::new(),
        q3_draw_contexts: Vec::new(),
        q3_trimeshes: Vec::new(),
        q3_styles: Vec::new(),
        q3_cameras: Vec::new(),
        q3_lights: Vec::new(),
        input_sprocket: Default::default(),
        input_sprocket_virtual_elements: Vec::new(),
        toolbox_startup: Default::default(),
        quicktime: Default::default(),
        sound: Default::default(),
        timer_tasks: Default::default(),
        vbl_tasks: Default::default(),
        callback_scheduling: Default::default(),
        process_file_system: ppc_initial_process_file_system(),
        current_gworld: SharedProcessGraphicsPort::from_value(PPC_MAIN_GWORLD),
        current_gdevice: SharedProcessGraphicsDevice::from_value(PPC_MAIN_GDEVICE),
        quickdraw_op_colors: Default::default(),
        quickdraw_hilite_colors: Default::default(),
        quickdraw_fore_color: PpcRgbColor {
            red: 0,
            green: 0,
            blue: 0,
        },
        quickdraw_fore_indices: Default::default(),
        quickdraw_back_color: PpcRgbColor {
            red: 0xffff,
            green: 0xffff,
            blue: 0xffff,
        },
        quickdraw_pen_h: 0,
        quickdraw_pen_v: 0,
        quickdraw_text_mode: PPC_QD_TEXT_MODE_SRC_OR,
        quickdraw_text_size: PPC_QD_TEXT_SIZE_SYSTEM,
        cursor_state: crate::process_context::SharedProcessCursorState::default(),
        param_text: Default::default(),
        scrap: Default::default(),
        list_manager: Default::default(),
        collections: Default::default(),
        halt_pc: PPC_HALT_PC,
        import_trap_base: PPC_IMPORT_TRAP_BASE,
        import_count: 0,
        imports: Vec::new(),
        section_bases: Vec::new(),
        input: PpcInputSnapshot::default(),
        process_input: Default::default(),
        event_queue: Default::default(),
        window_list: Default::default(),
        process_memory_manager: PpcProcessMemoryManager::with_heap(
            PPC_HEAP_BASE + 8 * 16,
            PPC_STACK_BASE,
        ),
        glm_mode: None,
        glm_callbacks: [None; 8],
        glm_callback_stack: Vec::new(),
        glm_allocations: HashMap::new(),
        glm_page_free_all_queue: VecDeque::new(),
        glm_error: 0,
        draw_sprocket: PpcDrawSprocketState::default(),
    });
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let (steps, running) = runner.run_steps(8, None);

    assert_eq!(steps, 1);
    assert!(!running);
    let (host_base, row_bytes, width, height, depth) = runner.dispatcher.screen_mode;
    assert_eq!((row_bytes, width, height, depth), (16, 8, 8, 16));
    assert_eq!(
        runner.bus.read_word(host_base + 4 * row_bytes + 4 * 2),
        0x4210 // Default diffuse grey, quantized to the 16-bit front buffer.
    );
    let ppc_app = runner
        .native
        .application()
        .expect("PPC app should stay loaded");
    assert!(ppc_app.q3_completed_frames.is_empty());
    assert_eq!(runner.q3_completed_frame_index, 1);
}
