#![allow(unused_imports)]

use super::*;
use std::collections::BTreeMap;

#[test]
#[cfg(any())]
fn ppc_imports_are_recorded_in_oracle_events_when_enabled() {
    let key_map_ptr = PPC_DATA_BASE;
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_CODE_BASE, 0x4800_0002u32.to_be_bytes().to_vec());
    memory.add_region(PPC_DATA_BASE, vec![0; 32]);
    let mut cpu = PpcCpu::new();
    cpu.pc = PPC_IMPORT_TRAP_BASE;
    cpu.lr = PPC_CODE_BASE;
    cpu.gpr[1] = PPC_STACK_TOP - 64;
    cpu.gpr[2] = 0x1234_5678;
    cpu.gpr[3] = key_map_ptr;

    let app = LoadedApp::from_ppc(PpcLoadedApp {
        cpu,
        memory,
        entry_pc: PPC_IMPORT_TRAP_BASE,
        rtoc: 0x1234_5678,
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
        gworlds: Vec::new(),
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
        import_count: 1,
        imports: vec![PpcImportBinding {
            library_index: 0,
            symbol_index: 0,
            library_name: "InterfaceLib".into(),
            symbol_name: "GetKeys".into(),
            class: 0,
            weak: false,
            address: PPC_IMPORT_TRAP_BASE,
            tvector_address: None,
            trap_pc: PPC_IMPORT_TRAP_BASE,
            dispatcher_target: PpcImportDispatcherTarget::GetKeys,
        }],
        section_bases: Vec::new(),
        input: PpcInputSnapshot::default(),
        process_input: Default::default(),
        event_queue: Default::default(),
        window_list: Default::default(),
        process_memory_manager: PpcProcessMemoryManager::with_heap(PPC_HEAP_BASE, PPC_STACK_BASE),
        glm_mode: None,
        glm_error: 0,
        draw_sprocket: PpcDrawSprocketState::default(),
    });
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let output_dir = tempfile::tempdir().unwrap();
    runner
        .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
        .unwrap();

    let (_steps, _running) = runner.run_steps(16, None);

    let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
    let events = std::fs::read_to_string(events_path).unwrap();
    let ppc_event: serde_json::Value = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|value: &serde_json::Value| value["event"] == "ppc_import")
        .expect("oracle recording should include a PPC import event");
    assert_eq!(ppc_event["pc"], PPC_IMPORT_TRAP_BASE);
    assert_eq!(ppc_event["fields"]["import_index"], "0");
    assert_eq!(ppc_event["fields"]["library"], "InterfaceLib");
    assert_eq!(ppc_event["fields"]["symbol"], "GetKeys");
    assert_eq!(ppc_event["fields"]["rtoc"], "12345678");
    assert_eq!(ppc_event["fields"]["dispatcher_target"], "GetKeys");
    assert_eq!(ppc_event["fields"]["repeat_count"], "1");
}

#[test]
#[cfg(any())]
fn oracle_script_input_event_records_fields_without_snapshot() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let output_dir = tempfile::tempdir().unwrap();
    runner
        .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
        .unwrap();

    runner
        .record_oracle_script_input(BTreeMap::from([
            ("action".to_string(), "key_down".to_string()),
            ("key".to_string(), "space".to_string()),
            ("mac_key".to_string(), "31".to_string()),
            ("char_code".to_string(), "20".to_string()),
        ]))
        .unwrap();

    let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
    let events = std::fs::read_to_string(events_path).unwrap();
    let script_input: serde_json::Value = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|value: &serde_json::Value| value["event"] == "script_input")
        .expect("oracle recording should include a script_input event");
    assert_eq!(script_input["screen_event_count"], 0);
    assert_eq!(script_input["fields"]["action"], "key_down");
    assert_eq!(script_input["fields"]["key"], "space");
    assert_eq!(script_input["fields"]["mac_key"], "31");
    assert_eq!(script_input["fields"]["char_code"], "20");

    let snapshots_path = output_dir.path().join(crate::oracle::ORACLE_SNAPSHOTS_FILE);
    let snapshots = std::fs::read_to_string(snapshots_path).unwrap();
    assert!(
        snapshots.trim().is_empty(),
        "script input events should not create screen snapshots"
    );
}

#[test]
#[cfg(any())]
fn oracle_input_sprocket_event_records_simple_state_without_snapshot() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let output_dir = tempfile::tempdir().unwrap();
    runner
        .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
        .unwrap();

    let mut input = PpcInputSnapshot::default();
    input.key_map[0] = 0x01;
    runner.record_input_sprocket_trace(&[PpcInputSprocketSimpleStateTraceEntry {
        import_index: 7,
        pc: 0x01f0_1234,
        element: 0x0200_1000,
        state_ptr: 0x0200_2000,
        state: 0x0000_0001,
        kind: 0x6275_746e,
        kind_name: "button".to_string(),
        fallback_state: 0,
        need_name: "Fire".to_string(),
        action_binding: "button/fire".to_string(),
        input,
        input_sprocket: PpcInputSprocketState {
            initialized: true,
            suspended: false,
            keyboard_active: true,
            mouse_active: true,
            configure_count: 0,
            virtual_element_count: 1,
            last_virtual_need_count: 1,
            last_virtual_needs_ptr: 0x0200_3000,
            last_virtual_elements_out_ptr: 0x0200_4000,
        },
    }]);

    let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
    let events = std::fs::read_to_string(events_path).unwrap();
    let input_sprocket: serde_json::Value = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|value: &serde_json::Value| value["event"] == "input_sprocket")
        .expect("oracle recording should include an InputSprocket event");
    assert_eq!(input_sprocket["pc"], 0x01f0_1234);
    assert_eq!(input_sprocket["screen_event_count"], 0);
    assert_eq!(input_sprocket["fields"]["import_index"], "7");
    assert_eq!(input_sprocket["fields"]["element"], "02001000");
    assert_eq!(input_sprocket["fields"]["state_ptr"], "02002000");
    assert_eq!(input_sprocket["fields"]["state"], "00000001");
    assert_eq!(input_sprocket["fields"]["kind"], "6275746E");
    assert_eq!(input_sprocket["fields"]["kind_name"], "button");
    assert_eq!(input_sprocket["fields"]["need_name"], "Fire");
    assert_eq!(input_sprocket["fields"]["action_binding"], "button/fire");
    assert_eq!(
        input_sprocket["fields"]["key_map"],
        "01000000000000000000000000000000"
    );
    assert_eq!(input_sprocket["fields"]["mouse_button"], "false");
    assert_eq!(input_sprocket["fields"]["keyboard_active"], "true");

    let snapshots_path = output_dir.path().join(crate::oracle::ORACLE_SNAPSHOTS_FILE);
    let snapshots = std::fs::read_to_string(snapshots_path).unwrap();
    assert!(
        snapshots.trim().is_empty(),
        "InputSprocket events should not create screen snapshots"
    );
}

#[test]
#[cfg(any())]
fn oracle_draw_sprocket_event_records_swap_without_snapshot() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let output_dir = tempfile::tempdir().unwrap();
    runner
        .enable_oracle_recording(output_dir.path(), crate::oracle::OracleSource::Systemless)
        .unwrap();

    runner.record_draw_sprocket_trace(&[PpcDrawSprocketTraceEntry {
        import_index: 12,
        pc: 0x01f0_3456,
        action: "swap_buffers".to_string(),
        result: 0,
        context: Some(0x0200_1000),
        requested_state: None,
        requested_frequency: None,
        requested_width: None,
        requested_height: None,
        requested_context_options: None,
        requested_display_depth_mask: None,
        requested_back_buffer_depth_mask: None,
        requested_display_depth: None,
        requested_back_buffer_depth: None,
        requested_page_count: None,
        can_user_select: None,
        fade_kind: None,
        fade_percent: None,
        fade_zero_red: None,
        fade_zero_green: None,
        fade_zero_blue: None,
        reserved_context: Some(0x0200_1000),
        active_context: Some(0x0200_1000),
        context_state: "active".to_string(),
        front_buffer_gworld: 0x00ab_cdef,
        back_buffer_gworld: 0x0012_3456,
        last_swap_context: Some(0x0200_1000),
        swap_count: 3,
        fade_count: 1,
        frequency: 60 << 16,
        width: 640,
        height: 480,
        context_options: 1,
        display_depth_mask: 16,
        back_buffer_depth_mask: 16,
        display_depth: 16,
        back_buffer_depth: 16,
        page_count: 2,
    }]);

    let events_path = output_dir.path().join(crate::oracle::ORACLE_EVENTS_FILE);
    let events = std::fs::read_to_string(events_path).unwrap();
    let draw_sprocket: serde_json::Value = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .find(|value: &serde_json::Value| value["event"] == "draw_sprocket")
        .expect("oracle recording should include a DrawSprocket event");
    assert_eq!(draw_sprocket["pc"], 0x01f0_3456);
    assert_eq!(draw_sprocket["screen_event_count"], 0);
    assert_eq!(draw_sprocket["fields"]["import_index"], "12");
    assert_eq!(draw_sprocket["fields"]["action"], "swap_buffers");
    assert_eq!(draw_sprocket["fields"]["result"], "0");
    assert_eq!(draw_sprocket["fields"]["result_hex"], "0000");
    assert_eq!(draw_sprocket["fields"]["context"], "02001000");
    assert_eq!(draw_sprocket["fields"]["requested_state"], "none");
    assert_eq!(draw_sprocket["fields"]["requested_frequency"], "none");
    assert_eq!(draw_sprocket["fields"]["requested_width"], "none");
    assert_eq!(draw_sprocket["fields"]["requested_height"], "none");
    assert_eq!(draw_sprocket["fields"]["requested_context_options"], "none");
    assert_eq!(
        draw_sprocket["fields"]["requested_display_depth_mask"],
        "none"
    );
    assert_eq!(
        draw_sprocket["fields"]["requested_back_buffer_depth_mask"],
        "none"
    );
    assert_eq!(draw_sprocket["fields"]["requested_display_depth"], "none");
    assert_eq!(
        draw_sprocket["fields"]["requested_back_buffer_depth"],
        "none"
    );
    assert_eq!(draw_sprocket["fields"]["requested_page_count"], "none");
    assert_eq!(draw_sprocket["fields"]["can_user_select"], "none");
    assert_eq!(draw_sprocket["fields"]["fade_kind"], "none");
    assert_eq!(draw_sprocket["fields"]["fade_percent"], "none");
    assert_eq!(draw_sprocket["fields"]["fade_zero_red"], "none");
    assert_eq!(draw_sprocket["fields"]["fade_zero_green"], "none");
    assert_eq!(draw_sprocket["fields"]["fade_zero_blue"], "none");
    assert_eq!(draw_sprocket["fields"]["reserved_context"], "02001000");
    assert_eq!(draw_sprocket["fields"]["active_context"], "02001000");
    assert_eq!(draw_sprocket["fields"]["has_reserved_context"], "true");
    assert_eq!(draw_sprocket["fields"]["has_active_context"], "true");
    assert_eq!(draw_sprocket["fields"]["context_state"], "active");
    assert_eq!(draw_sprocket["fields"]["front_gworld"], "00ABCDEF");
    assert_eq!(draw_sprocket["fields"]["back_gworld"], "00123456");
    assert_eq!(draw_sprocket["fields"]["last_swap_context"], "02001000");
    assert_eq!(draw_sprocket["fields"]["swap_count"], "3");
    assert_eq!(draw_sprocket["fields"]["fade_count"], "1");
    assert_eq!(draw_sprocket["fields"]["frequency"], "3932160");
    assert_eq!(draw_sprocket["fields"]["width"], "640");
    assert_eq!(draw_sprocket["fields"]["height"], "480");
    assert_eq!(draw_sprocket["fields"]["context_options"], "1");
    assert_eq!(draw_sprocket["fields"]["display_depth_mask"], "16");
    assert_eq!(draw_sprocket["fields"]["back_buffer_depth_mask"], "16");
    assert_eq!(draw_sprocket["fields"]["display_depth"], "16");
    assert_eq!(draw_sprocket["fields"]["back_buffer_depth"], "16");
    assert_eq!(draw_sprocket["fields"]["page_count"], "2");

    let snapshots_path = output_dir.path().join(crate::oracle::ORACLE_SNAPSHOTS_FILE);
    let snapshots = std::fs::read_to_string(snapshots_path).unwrap();
    assert!(
        snapshots.trim().is_empty(),
        "DrawSprocket events should not create screen snapshots"
    );
}
