use super::*;
use crate::loader::LoadedApp;
use crate::memory::globals::addr;
use crate::process_context::{
    SharedProcessDisplayClut, SharedProcessDisplayGamma, SharedProcessGraphicsDevice,
    SharedProcessGraphicsPort, SharedProcessTickState,
};
use ppc::PpcCpu;
use std::collections::VecDeque;

#[test]
fn ppc_system_event_mask_write_enables_injected_key_up_events() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    app.ppc
        .as_mut()
        .expect("synthetic PPC app")
        .memory
        .add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    runner
        .native
        .application_mut()
        .expect("PPC app installed")
        .memory
        .write_u16_be(addr::SYS_EVT_MASK, 0xffdf)
        .unwrap();
    runner.push_key_down(0x7c, 29);
    runner.push_key_up(0x7c, 29);

    assert!(runner
        .process_context
        .event_queue()
        .iter()
        .any(|event| event.what == 4 && event.message == 0x0000_7c1d));
}

#[test]
fn ppc_queue_sync_preserves_autokey_posted_during_tick_advance() {
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app
        .memory
        .write_u32_be(PPC_CODE_BASE, 0x4800_0000)
        .unwrap(); // b .

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.set_instructions_per_tick(1);
    runner.push_key_down(0x00, b'a');

    for _ in 0..TrapDispatcher::AUTO_KEY_THRESHOLD_TICKS {
        let (steps, running) = runner.run_steps(1, None);
        assert_eq!(steps, 1);
        assert!(running);
    }

    assert!(runner
        .process_context
        .event_queue()
        .iter()
        .any(|event| { event.what == 5 && event.message == 0x0000_0061 }));
}

#[test]
fn ppc_getkeys_reads_runner_key_map_with_classic_packed_bit_order() {
    let key_map_ptr = PPC_DATA_BASE;
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_CODE_BASE, 0x4800_0002u32.to_be_bytes().to_vec());
    memory.add_region(PPC_DATA_BASE, vec![0; 32]);
    let mut cpu = PpcCpu::new();
    cpu.pc = PPC_IMPORT_TRAP_BASE;
    cpu.lr = PPC_CODE_BASE;
    cpu.gpr[1] = PPC_STACK_TOP - 64;
    cpu.gpr[3] = key_map_ptr;

    let app = LoadedApp::from_ppc(PpcLoadedApp {
        cpu,
        memory,
        entry_pc: PPC_IMPORT_TRAP_BASE,
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
        glm_callbacks: [None; 8],
        glm_callback_stack: Vec::new(),
        glm_allocations: HashMap::new(),
        glm_page_free_all_queue: VecDeque::new(),
        glm_error: 0,
        draw_sprocket: PpcDrawSprocketState::default(),
    });
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.push_key_down(0x00, b'a');
    runner.push_key_down(0x7b, 28);
    runner.push_key_down(0x31, b' ');

    let (steps, running) = runner.run_steps(16, None);

    assert!(!running);
    assert_eq!(steps, 2);
    let ppc_app = runner
        .native
        .application_mut()
        .expect("PPC app should stay loaded");
    assert_eq!(ppc_app.memory.read_u8(key_map_ptr), Some(0x01));
    assert_eq!(ppc_app.memory.read_u8(key_map_ptr + 6), Some(0x02));
    assert_eq!(ppc_app.memory.read_u8(key_map_ptr + 15), Some(0x08));
}

#[test]
fn arrows_as_numpad_remaps_key_and_char_together() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_arrows_as_numpad(true);

    assert_eq!(runner.remap_key(0x7B, 28), (0x56, b'4'));
    assert_eq!(runner.remap_key(0x7C, 29), (0x58, b'6'));
    assert_eq!(runner.remap_key(0x7D, 31), (0x57, b'5'));
    assert_eq!(runner.remap_key(0x7E, 30), (0x5B, b'8'));
    assert_eq!(runner.remap_key(0x2E, b'm'), (0x2E, b'm'));
}

#[test]
fn arrows_not_remapped_by_default() {
    let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    assert!(!runner.arrows_as_numpad());
    assert_eq!(runner.remap_key(0x7B, 28), (0x7B, 28));
    assert_eq!(runner.remap_key(0x7C, 29), (0x7C, 29));
    assert_eq!(runner.remap_key(0x2E, b'm'), (0x2E, b'm'));
}

#[test]
fn key_events_sync_low_memory_keymap() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM), 0);
    assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 4), 0);
    assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 6), 0);
    assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 15), 0);

    runner.push_key_down(0x00, b'a');
    runner.push_key_down(0x26, b'j');
    runner.push_key_down(0x31, b' ');
    runner.push_key_down(0x7E, 30);

    assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM), 0x01);
    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 4),
        0x40,
        "J key should be visible to byte/bit KeyMap readers"
    );
    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 5),
        0,
        "J key should not alias M at KeyMapLM byte 5"
    );
    assert_eq!(runner.bus.read_byte(addr::KEY_MAP_LM + 6), 0x02);
    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 15),
        0x40,
        "up arrow should be visible to byte/bit KeyMap readers"
    );
    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 14),
        0,
        "up arrow should not be mirrored into the unused raw byte"
    );

    runner.push_key_up(0x26, b'j');

    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 4),
        0,
        "J key release should clear the low-memory mirror"
    );
    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 5),
        0,
        "J key release should leave the M-key byte clear"
    );
    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 15),
        0x40,
        "unrelated byte/bit down keys should remain mirrored"
    );
    assert_eq!(
        runner.bus.read_byte(addr::KEY_MAP_LM + 14),
        0,
        "unused raw byte should stay clear"
    );
}

#[test]
fn caps_lock_latch_is_preserved_in_low_memory_keymap() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let caps_lock_byte = addr::KEY_MAP_LM + 7;

    runner.push_key_down(0x39, 0);
    assert_eq!(runner.bus.read_byte(caps_lock_byte) & 0x02, 0x02);
    runner.push_key_up(0x39, 0);
    assert_eq!(
        runner.bus.read_byte(caps_lock_byte) & 0x02,
        0x02,
        "physical release must keep the low-memory Caps Lock bit latched"
    );

    runner.push_key_down(0x39, 0);
    assert_eq!(runner.bus.read_byte(caps_lock_byte) & 0x02, 0);
    runner.push_key_up(0x39, 0);
    assert_eq!(runner.bus.read_byte(caps_lock_byte) & 0x02, 0);
}
