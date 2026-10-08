use super::*;
use ppc::PpcCpu;
use std::collections::HashMap;

#[test]
fn ppc_exit_to_shell_halt_is_distinct_from_other_ppc_stops() {
    let mut exit = test_ppc_import_binding(7, "InterfaceLib", "ExitToShell");
    exit.dispatcher_target = PpcImportDispatcherTarget::ExitToShell;
    let imports = vec![exit];
    let halted = PpcRunResult::Halted {
        pc: PPC_HALT_PC,
        cycles: 4,
    };

    assert!(ppc_halted_by_exit_to_shell(&imports, halted, Some(7), None));
    assert!(!ppc_halted_by_exit_to_shell(
        &imports,
        halted,
        Some(7),
        Some(7)
    ));
    assert!(!ppc_halted_by_exit_to_shell(
        &imports,
        halted,
        Some(8),
        None
    ));
    let mut unsupported = test_ppc_import_binding(7, "InterfaceLib", "MysteryCall");
    unsupported.dispatcher_target = PpcImportDispatcherTarget::Unsupported;
    let duplicate_imports = vec![unsupported, imports[0].clone()];
    assert!(!ppc_halted_by_exit_to_shell(
        &duplicate_imports,
        halted,
        Some(7),
        None
    ));
    assert!(!ppc_halted_by_exit_to_shell(
        &imports,
        PpcRunResult::MemoryFault {
            pc: PPC_CODE_BASE,
            addr: 0,
            was_write: false,
            cycles: 4,
        },
        Some(7),
        None
    ));
}

#[test]
fn ppc_unimpl_histogram_key_names_unsupported_imports() {
    let imports = vec![test_ppc_import_binding(7, "InterfaceLib", "MysteryCall")];

    assert_eq!(
        ppc_unimpl_histogram_key(&imports, PpcRunResult::CycleLimit { cycles: 0 }, Some(7)),
        Some("import #7 InterfaceLib:MysteryCall".to_string())
    );
    assert_eq!(
        ppc_unimpl_histogram_key(&imports, PpcRunResult::CycleLimit { cycles: 0 }, Some(8)),
        Some("import #8 <unknown>".to_string())
    );
}

#[test]
fn ppc_unimpl_histogram_key_records_instruction_decode_errors() {
    assert_eq!(
        ppc_unimpl_histogram_key(
            &[],
            PpcRunResult::Unimplemented {
                pc: 0x0100_0000,
                error: ppc::PpcDecodeError::UnsupportedPrimaryOpcode(1),
                cycles: 12,
            },
            None,
        ),
        Some("instruction pc=$01000000 UnsupportedPrimaryOpcode(1)".to_string())
    );
}

#[test]
fn ppc_unimpl_histogram_formatter_sorts_by_count_then_key() {
    let mut histogram = HashMap::new();
    merge_ppc_unimpl_histogram(&mut histogram, "import #7 InterfaceLib:Foo".to_string());
    merge_ppc_unimpl_histogram(&mut histogram, "import #7 InterfaceLib:Foo".to_string());
    merge_ppc_unimpl_histogram(&mut histogram, "instruction pc=$01000000 Bar".to_string());

    assert_eq!(
        format_ppc_unimpl_histogram(&histogram, 2),
        "[PPC-UNIMPL-HIST] top 2 of 2 unsupported stops (3 total)\n\
             [PPC-UNIMPL-HIST]            2  import #7 InterfaceLib:Foo\n\
             [PPC-UNIMPL-HIST]            1  instruction pc=$01000000 Bar\n"
    );
}

#[test]
fn ppc_loaded_app_runs_through_fixture_runner() {
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_CODE_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
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
        process_file_system: SharedProcessFileSystem::from_state(
            ProcessFileSystemState {
                files: Default::default(),
                stdio_streams: ppc_initial_stdio_streams(),
                vfs_volumes: crate::process_context::SharedProcessValue::default(),
                vfs_directories: crate::process_context::SharedProcessValue::from_value(vec![
                    PpcVfsDirectory {
                        dir_id: 19,
                        parent_dir_id: 17,
                        path: "System Folder/Preferences/Test App Saves".to_string(),
                        creator: u32::from_be_bytes(*b"Nano"),
                        file_type: u32::from_be_bytes(*b"dir "),
                        finder_flags: 0x0080,
                        dirty: true,
                    },
                ]),
                next_vfs_dir_id: crate::process_context::SharedProcessValue::from_value(20),
                default_dir_id: crate::process_context::SharedProcessValue::from_value(2),
                vfs_files: vec![PpcVfsFileRecord {
                    path: "System Folder/Preferences/Test App Prefs".to_string(),
                    data: (b"prefs".to_vec()).into(),
                    creator: u32::from_be_bytes(*b"Nano"),
                    file_type: u32::from_be_bytes(*b"pref"),
                    finder_flags: 0x0200,
                    dirty: true,
                }]
                .into(),
                deleted_vfs_file_paths: vec!["System Folder/Preferences/Old Prefs".to_string()],
                resource_manager: Default::default(),
                next_file_ref_num: 128,
                ..ProcessFileSystemState::default()
            }
            .with_resources(
                Vec::new(),
                vec![PpcVfsResourceFileRecord {
                    path: "System Folder/Preferences/Test App HighScores".to_string(),
                    creator: u32::from_be_bytes(*b"Nano"),
                    file_type: u32::from_be_bytes(*b"pref"),
                    finder_flags: 0x0400,
                    resource_len: 0,
                    raw_data: None,
                    map_attrs: 0,
                    dirty: true,
                }],
                vec![PpcVfsResourceRecord {
                    ref_num: 128,
                    path: "System Folder/Preferences/Test App HighScores".to_string(),
                    res_type: u32::from_be_bytes(*b"pref"),
                    res_id: 200,
                    name: b"Scores".to_vec(),
                    data: b"score".to_vec(),
                    raw_data: None,
                    raw_attrs: None,
                    attrs: 0,
                    handle: 0,
                }],
            ),
        ),
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
    let output_dir = tempfile::tempdir().unwrap();
    let old_host_path = output_dir
        .path()
        .join("System Folder/Preferences/Old Prefs");
    std::fs::create_dir_all(old_host_path.parent().unwrap()).unwrap();
    std::fs::write(&old_host_path, b"old").unwrap();
    let old_rsrc_path = output_dir
        .path()
        .join("System Folder/Preferences/.rsrc/Old Prefs");
    std::fs::create_dir_all(old_rsrc_path.parent().unwrap()).unwrap();
    std::fs::write(&old_rsrc_path, b"old-rsrc").unwrap();
    runner.dispatcher_mut().output_dir = Some(output_dir.path().to_path_buf());
    runner.dispatcher_mut().vfs.insert(
        "System Folder/Preferences/Old Prefs".to_string(),
        b"old".to_vec(),
    );
    runner.dispatcher_mut().vfs_rsrc.insert(
        "System Folder/Preferences/Old Prefs".to_string(),
        b"old-rsrc".to_vec(),
    );
    runner.dispatcher_mut().set_vfs_entry_finfo(
        "System Folder/Preferences/Old Prefs",
        u32::from_be_bytes(*b"pref"),
        u32::from_be_bytes(*b"Nano"),
        0x4000,
    );
    runner
        .dispatcher_mut()
        .locked_files
        .insert("System Folder/Preferences/Old Prefs".to_string());

    let (steps, running) = runner.run_steps(8, None);

    assert_eq!(steps, 1);
    assert!(!running);
    assert!(runner.is_halted());
    assert_eq!(runner.halted_pc(), Some(PPC_HALT_PC));
    assert_eq!(runner.halted_sp(), Some(PPC_STACK_TOP - 64));
    assert_eq!(
        runner
            .dispatcher()
            .vfs
            .get("System Folder/Preferences/Test App Prefs")
            .map(Vec::as_slice),
        Some(b"prefs".as_slice())
    );
    let prefs_metadata = runner
        .dispatcher()
        .vfs_metadata
        .get("System Folder/Preferences/Test App Prefs")
        .copied()
        .expect("dirty PPC data fork should carry Finder metadata");
    assert_eq!(prefs_metadata.creator, u32::from_be_bytes(*b"Nano"));
    assert_eq!(prefs_metadata.file_type, u32::from_be_bytes(*b"pref"));
    assert_eq!(prefs_metadata.finder_flags, 0x0200);
    assert!(!runner
        .dispatcher()
        .vfs
        .contains_key("System Folder/Preferences/Old Prefs"));
    assert!(!runner
        .dispatcher()
        .vfs_rsrc
        .contains_key("System Folder/Preferences/Old Prefs"));
    assert!(!runner
        .dispatcher()
        .vfs_metadata
        .contains_key("System Folder/Preferences/Old Prefs"));
    assert!(!runner
        .dispatcher()
        .locked_files
        .contains("System Folder/Preferences/Old Prefs"));
    assert_eq!(
        std::fs::read(
            output_dir
                .path()
                .join("System Folder/Preferences/Test App Prefs")
        )
        .unwrap()
        .as_slice(),
        b"prefs".as_slice()
    );
    assert!(!old_host_path.exists());
    assert!(!old_rsrc_path.exists());
    let fork_bytes = runner
        .dispatcher()
        .vfs_rsrc
        .get("System Folder/Preferences/Test App HighScores")
        .expect("dirty PPC resource fork should sync to dispatcher VFS");
    let fork = ResourceFork::parse(fork_bytes).unwrap();
    assert_eq!(fork.get(*b"pref", 200).unwrap().data, b"score");
    let scores_metadata = runner
        .dispatcher()
        .vfs_metadata
        .get("System Folder/Preferences/Test App HighScores")
        .copied()
        .expect("dirty PPC resource fork should carry Finder metadata");
    assert_eq!(scores_metadata.creator, u32::from_be_bytes(*b"Nano"));
    assert_eq!(scores_metadata.file_type, u32::from_be_bytes(*b"pref"));
    assert_eq!(scores_metadata.finder_flags, 0x0400);
    let saves_directory = runner
        .dispatcher()
        .vfs_directories
        .iter()
        .find(|directory| directory.path == "System Folder/Preferences/Test App Saves")
        .expect("dirty PPC directory should remain in the shared catalogue");
    assert_eq!(
        TrapDispatcher::vfs_basename(&saves_directory.path),
        "Test App Saves"
    );
    assert!(output_dir
        .path()
        .join("System Folder/Preferences/Test App Saves")
        .is_dir());
    assert_eq!(
        std::fs::read(
            output_dir
                .path()
                .join("System Folder/Preferences/.rsrc/Test App HighScores")
        )
        .unwrap()
        .as_slice(),
        fork_bytes.as_slice()
    );
    assert!(!runner.native.application().unwrap().vfs_files[0].dirty);
    assert!(!runner.native.application().unwrap().vfs_directories[0].dirty);
    assert!(runner
        .native
        .application()
        .unwrap()
        .deleted_vfs_file_paths
        .is_empty());
    assert!(!runner.native.application().unwrap().vfs_resource_files[0].dirty);

    let prefs_path = "System Folder/Preferences/Test App Prefs";
    runner
        .native
        .application_mut()
        .unwrap()
        .with_test_vfs_file_mut(0, |file| {
            file.data
                .with_mut(|data| data.extend_from_slice(b"-native"));
        })
        .expect("seeded native preferences file");
    assert_eq!(
        runner.dispatcher().vfs.get(prefs_path).unwrap(),
        b"prefs-native",
        "classic File Manager view must observe native writes before another runner sync"
    );

    runner
        .dispatcher_mut()
        .vfs
        .with_entry_mut(prefs_path, |bytes| bytes.extend_from_slice(b"-classic"))
        .unwrap();
    let native_file = &runner.native.application().unwrap().vfs_files[0];
    let classic_file = runner.dispatcher().vfs.get_shared(prefs_path).unwrap();
    assert!(native_file.data.ptr_eq(classic_file));
    assert_eq!(native_file.data.as_slice(), b"prefs-native-classic");
}
