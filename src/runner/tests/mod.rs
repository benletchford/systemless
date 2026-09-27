pub(super) use super::app_heap_start_for_loaded_app;
pub(super) use super::apply_retro68_rela_relocations;
pub(super) use super::DEFAULT_LAUNCH_TICKS;
use super::*;
use crate::audio::AudioBackend;
use crate::guest_call::CooperativeThread;
use crate::loader::ppc::*;
use crate::loader::{Code0Header, LoadedApp};
use crate::menu_manager::TrackedMenuPaneView;
use crate::process_context::{
    PendingFileCompletion, ProcessFileSystemState, SharedProcessDisplayClut,
    SharedProcessDisplayGamma, SharedProcessFileSystem, SharedProcessGraphicsDevice,
    SharedProcessGraphicsPort, SharedProcessTickState,
};
use crate::sound::{
    DoubleBufferState, PendingDoubleBackCallback, PendingSoundCallback, PlaybackKind, SndChannel,
    SndCommand, OUTPUT_RATE,
};
use crate::trap::dispatch::{
    DialogItem, DialogTrackingState, LoadedResources, PendingWaitNextEventReturn, QueuedEvent,
    ResourceFileMap, TimerTask, VblTask,
};
use ppc::{PpcCpu, PpcNativeReturnGpr3};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

#[test]
fn dialog_filter_accepts_a_stack_result_reservation_prologue() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let callback = runner.bus.alloc(4);
    runner.bus.write_word(callback, 0x554F); // SUBQ.W #2,SP
    runner.bus.write_word(callback + 2, 0x206F); // MOVEA.L d16(SP),A0
    assert!(runner.looks_like_dialog_proc_entry(callback));

    runner.bus.write_word(callback, 0x0020); // Rect data, not code
    assert!(!runner.looks_like_dialog_proc_entry(callback));
}

mod audio;
mod cfm;
mod debug;
mod display;
mod launch;
mod low_memory;
mod partition;
mod process_lifecycle;
mod relocation;
mod thread;
mod trap_patch;
mod vfs;
mod window;

fn test_region_handle(
    bus: &mut crate::memory::MacMemoryBus,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> u32 {
    let rgn_ptr = 0x0030_0100;
    let rgn_handle = 0x0030_0140;
    bus.write_long(rgn_handle, rgn_ptr);
    bus.write_word(rgn_ptr, 10);
    bus.write_word(rgn_ptr + 2, top as u16);
    bus.write_word(rgn_ptr + 4, left as u16);
    bus.write_word(rgn_ptr + 6, bottom as u16);
    bus.write_word(rgn_ptr + 8, right as u16);
    rgn_handle
}

pub(super) fn make_resource_fork_bytes(resources: &[([u8; 4], i16, &[u8])]) -> Vec<u8> {
    let mut type_groups: Vec<([u8; 4], Vec<(i16, &[u8], u32)>)> = Vec::new();
    for (res_type, res_id, data) in resources {
        let group_idx = type_groups
            .iter()
            .position(|(existing_type, _)| existing_type == res_type)
            .unwrap_or_else(|| {
                type_groups.push((*res_type, Vec::new()));
                type_groups.len() - 1
            });
        type_groups[group_idx].1.push((*res_id, *data, 0));
    }
    type_groups.sort_by_key(|(res_type, _)| *res_type);
    for (_, entries) in &mut type_groups {
        entries.sort_by_key(|(res_id, _, _)| *res_id);
    }

    let data_offset = 16u32;
    let mut data_section = Vec::new();
    for (_, entries) in &mut type_groups {
        for (_, data, data_pos) in entries {
            *data_pos = data_section.len() as u32;
            data_section.extend_from_slice(&(data.len() as u32).to_be_bytes());
            data_section.extend_from_slice(data);
        }
    }

    let map_offset = data_offset + data_section.len() as u32;
    let type_list_offset = 30u16;
    let type_count = type_groups.len();
    let resource_count: usize = type_groups.iter().map(|(_, entries)| entries.len()).sum();
    let ref_lists_offset = 2 + type_count * 8;
    let name_list_offset = type_list_offset as usize + ref_lists_offset + resource_count * 12;
    let map_length = name_list_offset as u32;

    let mut bytes = vec![0u8; (map_offset + map_length) as usize];
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&data_offset.to_be_bytes());
    header[4..8].copy_from_slice(&map_offset.to_be_bytes());
    header[8..12].copy_from_slice(&(data_section.len() as u32).to_be_bytes());
    header[12..16].copy_from_slice(&map_length.to_be_bytes());
    bytes[0..16].copy_from_slice(&header);
    bytes[data_offset as usize..data_offset as usize + data_section.len()]
        .copy_from_slice(&data_section);

    let map_start = map_offset as usize;
    bytes[map_start..map_start + 16].copy_from_slice(&header);
    bytes[map_start + 24..map_start + 26].copy_from_slice(&type_list_offset.to_be_bytes());
    bytes[map_start + 26..map_start + 28].copy_from_slice(&(name_list_offset as u16).to_be_bytes());
    bytes[map_start + 28..map_start + 30].copy_from_slice(&((type_count as u16) - 1).to_be_bytes());

    let type_list_start = map_start + type_list_offset as usize;
    bytes[type_list_start..type_list_start + 2]
        .copy_from_slice(&((type_count as u16) - 1).to_be_bytes());
    let mut next_ref_list_offset = ref_lists_offset;
    for (i, (res_type, entries)) in type_groups.iter().enumerate() {
        let type_entry = type_list_start + 2 + i * 8;
        bytes[type_entry..type_entry + 4].copy_from_slice(res_type);
        bytes[type_entry + 4..type_entry + 6]
            .copy_from_slice(&((entries.len() as u16) - 1).to_be_bytes());
        bytes[type_entry + 6..type_entry + 8]
            .copy_from_slice(&(next_ref_list_offset as u16).to_be_bytes());

        let ref_list_start = type_list_start + next_ref_list_offset;
        for (j, (res_id, _, data_pos)) in entries.iter().enumerate() {
            let ref_entry = ref_list_start + j * 12;
            bytes[ref_entry..ref_entry + 2].copy_from_slice(&(*res_id as u16).to_be_bytes());
            bytes[ref_entry + 2..ref_entry + 4].copy_from_slice(&0xFFFFu16.to_be_bytes());
            bytes[ref_entry + 4] = 0;
            let data_offset_bytes = data_pos.to_be_bytes();
            bytes[ref_entry + 5..ref_entry + 8].copy_from_slice(&data_offset_bytes[1..4]);
        }

        next_ref_list_offset += entries.len() * 12;
    }

    bytes
}

pub(super) fn minimal_code0(above_a5: u32, below_a5: u32, jt_size: u32, jt_offset: u32) -> Vec<u8> {
    let mut code0 = Vec::with_capacity(16 + jt_size as usize);
    code0.extend_from_slice(&above_a5.to_be_bytes());
    code0.extend_from_slice(&below_a5.to_be_bytes());
    code0.extend_from_slice(&jt_size.to_be_bytes());
    code0.extend_from_slice(&jt_offset.to_be_bytes());
    code0.resize(16 + jt_size as usize, 0);
    code0
}

pub(super) fn size_resource_bytes(flags: u16, preferred_size: u32, minimum_size: u32) -> Vec<u8> {
    let mut size = Vec::with_capacity(10);
    size.extend_from_slice(&flags.to_be_bytes());
    size.extend_from_slice(&preferred_size.to_be_bytes());
    size.extend_from_slice(&minimum_size.to_be_bytes());
    size
}

fn dialog_tracking_for_test(filter_proc: u32, item_hit_ptr: u32) -> DialogTrackingState {
    DialogTrackingState {
        dialog_ptr: 0x0020_0000,
        bounds: (0, 0, 32, 32),
        title: String::new(),
        proc_id: 1,
        items: Vec::new(),
        default_item: 0,
        cancel_item: 0,
        edit_text: String::new(),
        edit_item: 0,
        saved_pixels: Vec::new().into(),
        stack_ptr: 0,
        item_hit_ptr,
        rendered_pixels: Vec::new().into(),
        flash_remaining: 0,
        flash_delay: 0,
        flash_item: 0,
        edit_text_modified: false,
        draw_proc_queue: VecDeque::new(),
        draw_procs_done: true,
        rendered_pixels_final: true,
        filter_presentation_epoch: None,
        filter_proc,
        game_managed: false,
        last_filter_event: None,
        popup_draws: Vec::new(),
        active_popup: None,
        active_button: None,
        active_user_item: None,
    }
}

pub(super) fn test_ppc_import_binding(
    symbol_index: u32,
    library: &str,
    symbol: &str,
) -> PpcImportBinding {
    PpcImportBinding {
        library_index: 0,
        symbol_index,
        library_name: library.to_string(),
        symbol_name: symbol.to_string(),
        class: 0,
        weak: false,
        address: 0,
        tvector_address: None,
        trap_pc: 0,
        dispatcher_target: PpcImportDispatcherTarget::Unsupported,
    }
}

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

pub(super) fn halted_ppc_app_with_sound(sound: PpcSoundState) -> LoadedApp {
    let mut memory = PpcSectionMem::new();
    memory.add_region(PPC_CODE_BASE, 0x4e80_0020u32.to_be_bytes().to_vec());
    memory.add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    let mut cpu = PpcCpu::new();
    cpu.pc = PPC_CODE_BASE;
    cpu.lr = PPC_HALT_PC;
    cpu.gpr[1] = PPC_STACK_TOP - 64;

    LoadedApp::from_ppc(PpcLoadedApp {
        cpu,
        memory,
        entry_pc: PPC_CODE_BASE,
        rtoc: 0,
        stack_base: PPC_STACK_BASE,
        stack_size: PPC_STACK_SIZE,
        stack_pointer: PPC_STACK_TOP - 64,
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
        sound,
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
        process_memory_manager: PpcProcessMemoryManager::with_heap(PPC_HEAP_BASE, PPC_STACK_BASE),
        draw_sprocket: PpcDrawSprocketState::default(),
    })
}

pub(super) fn queue_ppc_sound_completion(sound: &mut PpcSoundState, channel: u32, completion: u32) {
    sound.file_playbacks.push(PpcSoundFilePlaybackRecord {
        channel,
        ref_num: 0,
        resource_id: 0,
        buffer_size: 0,
        buffer: 0,
        selection: 0,
        completion,
        completion_command: None,
        async_play: true,
        aiff: None,
        decoded_aiff: None,
    });
    sound
        .manager
        .queue_sound_callback(PendingSoundCallback::FileCompletion {
            architecture: CallbackTaskArchitecture::PowerPc,
            callback_addr: completion,
            chan_ptr: channel,
        });
}

#[test]
fn ppc_initialization_attaches_both_cpu_adapters_to_one_guest_call_stack() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    runner.dispatcher.guest_calls.begin_m68k(
        crate::guest_call::GuestCallTarget {
            isa: crate::guest_procedure::GuestIsa::M68k,
            entry: 0x1000,
            rtoc: 0,
        },
        0x2000,
        0x3000,
    );
    let ppc_app = runner.native.application_mut().expect("PPC app");
    assert_eq!(ppc_app.toolbox_startup.execution.calls().len(), 1);
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .complete_m68k(0x2002, 0x3000));
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn ppc_initialization_attaches_both_cpu_adapters_to_one_sound_manager() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    {
        let ppc_app = runner.native.application_mut().expect("PPC app");
        assert!(ppc_app
            .sound
            .manager
            .ptr_eq(&runner.dispatcher.sound_manager));
        ppc_app.sound.manager.register_channel(
            0x0050_1000,
            false,
            0x0012_3456,
            CallbackTaskArchitecture::M68k,
        );
        ppc_app.sound.manager.set_default_output_volume(0x0000_8000);
    }

    let classic_callback = runner
        .dispatcher
        .sound_manager
        .with_channel_mut(0x0050_1000, |channel| {
            channel.set_volume(0x0000_4000);
            channel.callback_addr
        })
        .expect("native channel visible to classic adapter");
    assert_eq!(classic_callback, 0x0012_3456);
    runner
        .dispatcher
        .sound_manager
        .set_sys_beep_volume(0x0000_2000);

    let ppc_app = runner.native.application().expect("PPC app");
    assert_eq!(ppc_app.sound.manager.default_output_volume(), 0x0000_8000);
    assert_eq!(ppc_app.sound.manager.sys_beep_volume(), 0x0000_2000);
    assert!(ppc_app
        .sound
        .manager
        .channels
        .iter()
        .any(|channel| channel.guest_ptr == 0x0050_1000));
}

#[test]
fn ppc_initialization_shares_current_resource_file_and_detaches_clones() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher
        .set_loaded_resources_for_test(LoadedResources {
            files: HashMap::from([
                (5, ResourceFileMap::default()),
                (9, ResourceFileMap::default()),
            ]),
            names: HashMap::new(),
            search_order: vec![5, 9],
            current_file: 5,
        });
    runner.init_app(&app);

    assert_eq!(runner.dispatcher.current_resource_refnum(), 5);
    assert_eq!(
        runner
            .native
            .application()
            .expect("PPC app")
            .current_resource_refnum(),
        5
    );

    runner
        .dispatcher
        .set_current_resource_refnum(&mut runner.bus, 9);
    assert_eq!(
        runner
            .native
            .application()
            .expect("PPC app")
            .current_resource_refnum(),
        9
    );
    assert_eq!(runner.bus.read_word(0x0A5A), 9);

    runner
        .native
        .application_mut()
        .expect("PPC app")
        .set_current_resource_refnum(5);
    assert_eq!(runner.dispatcher.current_resource_refnum(), 5);
    assert_eq!(runner.bus.read_word(0x0A5A), 5);

    let mut detached = runner.native.application().expect("PPC app").clone();
    runner
        .native
        .application_mut()
        .expect("PPC app")
        .set_current_resource_refnum(9);
    assert_eq!(detached.current_resource_refnum(), 5);
    detached.set_current_resource_refnum(7);
    assert_eq!(runner.dispatcher.current_resource_refnum(), 9);
    assert_eq!(runner.bus.read_word(0x0A5A), 9);

    runner
        .native
        .application_mut()
        .expect("PPC app")
        .set_current_resource_refnum(5);
    assert!(runner
        .dispatcher
        .close_resource_file_refnum(&mut runner.bus, 5));
    assert_eq!(runner.dispatcher.current_resource_refnum(), 9);
    assert_eq!(runner.bus.read_word(0x0A5A), 9);
}

#[test]
fn universal_proc_preserves_native_isa_for_protected_transition_vectors() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;

    const VECTOR: u32 = 0x0180_0000;
    const ENTRY: u32 = VECTOR + 8;
    const RESULT: u32 = 0x1234_5678;
    for installed in [false, true] {
        for protected in [false, true] {
            let mut native =
                load_pef_application(&synthetic_pef_with_import(b"CallUniversalProc")).unwrap();
            let words = [ENTRY, PPC_DATA_BASE, 0x3c60_1234, 0x6063_5678, 0x4e80_0020];
            let bytes = words.into_iter().flat_map(u32::to_be_bytes).collect();
            if protected {
                native
                    .memory
                    .publish_system_code(GuestIsa::PowerPc, VECTOR, bytes)
                    .unwrap();
            } else {
                native.memory.add_readonly_region(VECTOR, bytes);
            }
            native.cpu.gpr[3] = VECTOR;
            native.cpu.gpr[4] = 0x30; // Pascal, no arguments, long result.
            if installed {
                let mut runner =
                    FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
                runner.init_app(&LoadedApp::from_ppc(native));
                let (_, running) = runner.run_steps(128, None);
                assert!(!running, "installed protected={protected}");
                assert_eq!(runner.native.application_mut().unwrap().cpu.gpr[3], RESULT);
            } else {
                let probe = native.run_with_hle_imports(128);
                assert_eq!(probe.unsupported_import_index, None);
                assert_eq!(native.cpu.pc, native.halt_pc);
                assert_eq!(
                    native.cpu.gpr[3], RESULT,
                    "standalone protected={protected}"
                );
            }
        }
    }
}

#[test]
fn native_tick_count_import_observes_live_process_trap_patch() {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_POWERPC_ISA,
    };
    use crate::loader::ppc::tests::synthetic_pef_with_import;
    use crate::trap::manager::{TrapManager, TrapTableKind};

    const DESCRIPTOR: u32 = 0x0180_0000;
    const TVECTOR: u32 = DESCRIPTOR + 0x80;
    const ENTRY: u32 = DESCRIPTOR + 0x100;
    const RTOC: u32 = DESCRIPTOR + 0x200;
    const RESULT: u32 = 0x1234_5678;

    let native = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&LoadedApp::from_ppc(native));
    let native = runner.native.application_mut().expect("native application");
    native.memory.add_region(DESCRIPTOR, vec![0; 0x300]);
    native
        .memory
        .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    native
        .memory
        .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    native.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
    let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    native.memory.write_u32_be(record, 0x30).unwrap();
    native
        .memory
        .write_u8(record + 5, ROUTINE_RECORD_POWERPC_ISA)
        .unwrap();
    native
        .memory
        .write_u16_be(record + 6, ROUTINE_FLAG_USE_NATIVE_ISA)
        .unwrap();
    native.memory.write_u32_be(record + 8, TVECTOR).unwrap();
    native.memory.write_u32_be(TVECTOR, ENTRY).unwrap();
    native.memory.write_u32_be(TVECTOR + 4, RTOC).unwrap();
    for (offset, word) in [0x3c60_1234, 0x6063_5678, 0x4e80_0020]
        .into_iter()
        .enumerate()
    {
        native
            .memory
            .write_u32_be(ENTRY + u32::try_from(offset).unwrap() * 4, word)
            .unwrap();
    }
    let table_entry = TrapManager::table_address(0xA975, TrapTableKind::Toolbox);
    runner.bus.write_long(table_entry, DESCRIPTOR);
    let (_, running) = runner.run_steps(128, None);

    assert!(!running);
    let native = runner
        .native
        .application()
        .expect("native application retained");
    assert_eq!(native.cpu.gpr[3], RESULT);
    assert!(native.guest_calls().is_empty());
}

#[test]
fn native_tick_count_import_completes_live_classic_trap_patch() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;
    use crate::trap::manager::{TrapManager, TrapTableKind};

    const RESULT: u32 = 0x1234_5678;
    let native = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&LoadedApp::from_ppc(native));
    let handler = runner.bus.alloc(12);
    for (offset, word) in [
        0x2f7c, // MOVE.L #RESULT,4(SP), the Pascal result slot
        (RESULT >> 16) as u16,
        RESULT as u16,
        0x0004,
        0x4e75,
    ]
    .into_iter()
    .enumerate()
    {
        runner
            .bus
            .write_word(handler + u32::try_from(offset).unwrap() * 2, word);
    }
    let table_entry = TrapManager::table_address(0xA975, TrapTableKind::Toolbox);
    runner.bus.write_long(table_entry, handler);

    let (_, running) = runner.run_steps(256, None);

    assert!(!running);
    let native = runner
        .native
        .application()
        .expect("native application retained");
    assert_eq!(native.cpu.gpr[3], RESULT);
    assert!(native.guest_calls().is_empty());
}

#[test]
fn native_system_code_survives_large_process_ram_attachment() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;

    let mut native = load_pef_application(&synthetic_pef_with_import(b"TickCount")).unwrap();
    let pools = [
        PPC_IMPORT_TVECTOR_BASE,
        PPC_IMPORT_TRAP_BASE,
        PPC_CFM_MAIN_STUB_BASE,
    ];
    let words = pools.map(|base| native.memory.read_u32_be(base).unwrap());
    // Call the imported transition vector, as compiled PEF glue does.
    // Inside Macintosh: PowerPC System Software (1994), pp. 1-27--1-28.
    let code = [
        0x3d80_0000 | (PPC_IMPORT_TVECTOR_BASE >> 16), // lis r12, vector@h
        0x618c_0000 | (PPC_IMPORT_TVECTOR_BASE & 0xffff), // ori r12,r12,vector@l
        0x800c_0000,                                   // lwz r0,0(r12)
        0x804c_0004,                                   // lwz r2,4(r12)
        0x7c09_03a6,                                   // mtctr r0
        0x4e80_0420,                                   // bctr
    ];
    const ENTRY: u32 = 0x0180_0000;
    native
        .memory
        .add_readonly_region(ENTRY, code.into_iter().flat_map(u32::to_be_bytes).collect());
    native.entry_pc = ENTRY;
    native.cpu.pc = ENTRY;
    native.cpu.lr = native.halt_pc;
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    let (reservation_base, _) = runner.bus.synthetic_reservation_range().unwrap();
    runner.set_launch_state(17, 1, 0);
    runner.init_app(&LoadedApp::from_ppc(native));
    let native = runner.native.application_mut().unwrap();
    for (base, word) in pools.into_iter().zip(words) {
        assert_eq!(native.memory.read_u32_be(base), Some(word));
        assert!(native
            .memory
            .shared_view()
            .is_shared_readonly_range(base, 1));
        assert_eq!(native.memory.write_u32_be(base, 0), None);
    }
    assert!(native
        .memory
        .shared_view()
        .is_shared_readonly_range(reservation_base, 1));
    assert_eq!(native.memory.write_u8(reservation_base, 0xff), None);
    let (steps, running) = runner.run_steps(64, None);
    assert!(steps >= 6);
    assert!(!running);
    let native = runner.native.application_mut().unwrap();
    assert_eq!(native.cpu.pc, native.halt_pc);
    assert_eq!(native.cpu.gpr[3], 17);
}

#[test]
fn ppc_initialization_attaches_both_cpu_adapters_to_one_native_menu_selection() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    assert!(runner
        .dispatcher
        .pending_native_menu_selection
        .stage((128, 2)));
    let ppc_app = runner.native.application_mut().expect("PPC app");
    assert_eq!(
        ppc_app
            .toolbox_startup
            .pending_native_menu_selection
            .snapshot(),
        Some((128, 2))
    );
    assert_eq!(
        ppc_app.toolbox_startup.pending_native_menu_selection.take(),
        Some((128, 2))
    );
    assert!(runner.dispatcher.pending_native_menu_selection.is_none());

    assert!(ppc_app
        .toolbox_startup
        .pending_native_menu_selection
        .stage((129, 3)));
    assert_eq!(
        runner.dispatcher.pending_native_menu_selection.take(),
        Some((129, 3))
    );
    assert!(ppc_app
        .toolbox_startup
        .pending_native_menu_selection
        .is_none());
}

#[test]
fn native_menu_select_observes_68k_disable_item_after_mdef_returns() {
    use crate::loader::ppc::tests::cross_abi_menu_select_fixture;
    use crate::memory::globals::addr;

    const CALLBACK_VALUE: u32 = 0x68c0_ab1e;
    const ROOT_MENU_ID: i16 = 140;
    const TARGET_ITEM: i16 = 2;

    let fixture = cross_abi_menu_select_fixture();
    let root_menu = fixture.root_menu;
    let root_record = fixture.root_record;
    let callback_marker = fixture.callback_marker;
    let title_h = fixture.title_h;
    let app = LoadedApp::from_ppc(fixture.app);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    // This fixture pre-renders classic menu pixels before attaching to the runner.
    runner.set_ui_theme(UiThemeId::ClassicSystem7);
    runner.init_app(&app);

    let framebuffer_before = {
        let native = runner.native.application_mut().expect("native app");
        let front = native.current_front_buffer().expect("front buffer");
        let mut framebuffer = Vec::with_capacity((front.row_bytes * front.height) as usize);
        let mut row = vec![0; front.row_bytes as usize];
        for y in 0..front.height {
            native
                .read_front_buffer_row(front, y, &mut row)
                .expect("front-buffer row");
            framebuffer.extend_from_slice(&row);
        }
        framebuffer
    };
    assert_ne!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(root_record + 10)
            .unwrap()
            & (1 << TARGET_ITEM),
        0,
        "the target row must begin enabled"
    );

    runner.push_canonical_mouse_down(10, title_h);
    let root_rect = (0..16)
        .find_map(|_| {
            let (_, running) = runner.run_steps(512, None);
            assert!(
                running,
                "native MenuSelect halted before opening the root menu"
            );
            runner
                .process_context
                .menu_tracking()
                .filter(|tracking| tracking.menu_handle == root_menu)
                .map(|tracking| tracking.dropdown_rect())
        })
        .expect("native MenuSelect should retain the canonical root handle");

    runner
        .dispatcher
        .set_mouse_position(root_rect.0 + 8, root_rect.1 + 16);
    runner.sync_mouse_position_lowmem();
    let callback_completed = (0..32).any(|_| {
        let (_, running) = runner.run_steps(512, None);
        assert!(
            running,
            "native MenuSelect halted during the 68k MDEF callback"
        );
        let callback_value = runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(callback_marker);
        callback_value == Some(CALLBACK_VALUE) && runner.dispatcher.guest_calls.depth() == 0
    });
    assert!(
        callback_completed,
        "the real 68k MDEF callback did not return"
    );
    let tracking = runner
        .process_context
        .menu_tracking()
        .expect("native interaction should remain retained");
    assert_eq!(tracking.menu_handle, root_menu);
    assert_eq!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(root_menu),
        Some(root_record),
        "a fixed-size 68k mutation must preserve the native handle allocation"
    );
    assert_eq!(
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(root_record + 10)
            .unwrap()
            & (1 << TARGET_ITEM),
        0,
        "the 68k DisableItem trap must mutate the live native MenuInfo"
    );

    let target_v = root_rect.0 + 24;
    let target_h = root_rect.1 + 16;
    runner.dispatcher.set_mouse_position(target_v, target_h);
    runner.sync_mouse_position_lowmem();
    let raw_choice = (u32::from(ROOT_MENU_ID as u16) << 16) | u32::from(TARGET_ITEM as u16);
    let native_observed_disabled_row = (0..16).any(|_| {
        let (_, running) = runner.run_steps(512, None);
        assert!(running, "native MenuSelect halted before the mouse release");
        runner.bus.read_long(addr::MENU_DISABLE) == raw_choice
            && runner
                .process_context
                .menu_tracking()
                .is_some_and(|tracking| {
                    tracking.menu_handle == root_menu && tracking.highlighted_item == 0
                })
    });
    assert!(
        native_observed_disabled_row,
        "native tracking did not reread the live disabled enableFlags"
    );

    runner.push_canonical_mouse_up(target_v, target_h);
    for _ in 0..16 {
        let (_, running) = runner.run_steps(512, None);
        if !running {
            break;
        }
    }
    assert!(runner.is_halted());
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());

    let native = runner
        .native
        .application_mut()
        .expect("native app retained");
    assert_eq!(native.cpu.gpr[3], 0, "disabled rows cannot be selected");
    let framebuffer_after = {
        let front = native.current_front_buffer().expect("front buffer");
        let mut framebuffer = Vec::with_capacity((front.row_bytes * front.height) as usize);
        let mut row = vec![0; front.row_bytes as usize];
        for y in 0..front.height {
            native
                .read_front_buffer_row(front, y, &mut row)
                .expect("front-buffer row");
            framebuffer.extend_from_slice(&row);
        }
        framebuffer
    };
    assert_eq!(
        framebuffer_after, framebuffer_before,
        "the retained interaction must restore its saved presentation"
    );

    native.cpu.pc = native.entry_pc;
    native.cpu.lr = PPC_HALT_PC;
    native.imports[0].dispatcher_target = PpcImportDispatcherTarget::MenuChoice;
    let probe = runner
        .process_context
        .with_memory_and_cfm(|memory_manager, cfm| {
            native.run_with_process_services(64, false, false, memory_manager, cfm)
        });
    assert_eq!(probe.handled_import_count, 1);
    assert_eq!(probe.unsupported_import_index, None);
    assert_eq!(native.cpu.gpr[3], raw_choice);
}

#[test]
fn native_menu_select_observes_growing_68k_append_menu_after_mdef_returns() {
    use crate::loader::ppc::tests::cross_abi_menu_select_fixture;

    const APPEND_STRING: u32 = crate::loader::ppc::PPC_DATA_BASE + 0x7300;
    const CALLBACK_VALUE: u32 = 0x68c0_ab1e;
    const ROOT_MENU_ID: i16 = 140;
    const TARGET_ITEM: i16 = 2;
    const APPENDED_TEXT: &[u8] =
        b"Cross-ABI growth must remain visible through the original native MenuHandle";

    let mut fixture = cross_abi_menu_select_fixture();
    let root_menu = fixture.root_menu;
    let original_record = fixture.root_record;
    let original_handle_record = fixture
        .app
        .handles()
        .iter()
        .copied()
        .find(|record| record.handle == root_menu)
        .expect("native root-menu allocation");
    let callback_marker = fixture.callback_marker;
    let title_h = fixture.title_h;

    let mut append_string = Vec::with_capacity(APPENDED_TEXT.len() + 1);
    append_string.push(APPENDED_TEXT.len() as u8);
    append_string.extend_from_slice(APPENDED_TEXT);
    fixture.app.memory.add_region(APPEND_STRING, append_string);

    // The real 68k MDEF grows the native MenuInfo with AppendMenu. A
    // relocatable block can move during SetHandleSize, but its master
    // pointer and Handle identity remain authoritative. Inside Macintosh:
    // Memory (1992), pp. 1-16--1-17 and 2-40--2-41.
    let mut mdef = Vec::new();
    mdef.extend_from_slice(&0x4ab9u16.to_be_bytes()); // TST.L marker
    mdef.extend_from_slice(&callback_marker.to_be_bytes());
    mdef.extend_from_slice(&0x660eu16.to_be_bytes()); // BNE.S after AppendMenu
    mdef.extend_from_slice(&0x2f3cu16.to_be_bytes()); // MOVE.L #rootMenu,-(SP)
    mdef.extend_from_slice(&root_menu.to_be_bytes());
    mdef.extend_from_slice(&0x2f3cu16.to_be_bytes()); // MOVE.L #appendString,-(SP)
    mdef.extend_from_slice(&APPEND_STRING.to_be_bytes());
    mdef.extend_from_slice(&0xa933u16.to_be_bytes()); // AppendMenu
    mdef.extend_from_slice(&0x23fcu16.to_be_bytes()); // MOVE.L #value,marker
    mdef.extend_from_slice(&CALLBACK_VALUE.to_be_bytes());
    mdef.extend_from_slice(&callback_marker.to_be_bytes());
    mdef.extend_from_slice(&0x4e74u16.to_be_bytes()); // RTD #18
    mdef.extend_from_slice(&0x0012u16.to_be_bytes());
    fixture.app.memory.add_region(fixture.mdef_entry, mdef);

    let app = LoadedApp::from_ppc(fixture.app);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    runner.push_canonical_mouse_down(10, title_h);
    let root_rect = (0..16)
        .find_map(|_| {
            runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
            let (_, running) = runner.run_steps(512, None);
            assert!(
                running,
                "native MenuSelect halted before opening the root menu"
            );
            runner
                .process_context
                .menu_tracking()
                .filter(|tracking| tracking.menu_handle == root_menu)
                .map(|tracking| tracking.dropdown_rect())
        })
        .expect("native MenuSelect should retain the canonical root handle");

    runner
        .dispatcher
        .set_mouse_position(root_rect.0 + 8, root_rect.1 + 16);
    runner.sync_mouse_position_lowmem();
    let callback_completed = (0..32).any(|_| {
        runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
        let (_, running) = runner.run_steps(512, None);
        assert!(
            running,
            "native MenuSelect halted during the growing 68k callback"
        );
        runner
            .native
            .application_mut()
            .unwrap()
            .memory
            .read_u32_be(callback_marker)
            == Some(CALLBACK_VALUE)
            && runner.dispatcher.guest_calls.depth() == 0
    });
    assert!(
        callback_completed,
        "the growing real 68k MDEF callback did not return"
    );

    let native = runner
        .native
        .application_mut()
        .expect("native app retained");
    let relocated_record = native
        .memory
        .read_u32_be(root_menu)
        .expect("live native root-menu master pointer");
    let handle_record = native
        .handles()
        .iter()
        .copied()
        .find(|record| record.handle == root_menu)
        .expect("updated native root-menu allocation");
    assert_eq!(handle_record.ptr, relocated_record);
    assert_eq!(handle_record.handle, root_menu);
    assert_eq!(handle_record.size, handle_record.capacity);
    assert!(handle_record.size > original_handle_record.size);
    assert_ne!(
        relocated_record, original_record,
        "the fixture must force relocation"
    );
    assert_eq!(
        runner.process_context.handle_for_ptr(relocated_record),
        Some(root_menu),
        "the process Memory Manager must publish the relocated native handle"
    );
    let mut menu_bytes = vec![0; handle_record.size as usize];
    native
        .memory
        .read_bytes_into(relocated_record, &mut menu_bytes)
        .expect("complete relocated MenuInfo bytes");
    let items = crate::menu_manager::MenuItems::decode(&menu_bytes)
        .expect("relocated native MenuInfo remains decodable");
    assert!(
        items.items.iter().any(|item| item.text == APPENDED_TEXT),
        "native decoding must observe the item appended by the 68k callback"
    );
    assert_eq!(native.last_mem_error(), 0);
    assert_eq!(
        runner.bus.read_word(crate::memory::globals::addr::MEM_ERR),
        0
    );

    let target_v = root_rect.0 + 24;
    let target_h = root_rect.1 + 16;
    runner.dispatcher.set_mouse_position(target_v, target_h);
    runner.sync_mouse_position_lowmem();
    let target_observed = (0..16).any(|_| {
        runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
        let (_, running) = runner.run_steps(512, None);
        assert!(running, "native MenuSelect halted before the mouse release");
        runner
            .process_context
            .menu_tracking()
            .is_some_and(|tracking| {
                tracking.menu_handle == root_menu && tracking.highlighted_item == TARGET_ITEM
            })
    });
    assert!(
        target_observed,
        "native tracking did not reach the live regular row"
    );
    runner.push_canonical_mouse_up(target_v, target_h);
    for _ in 0..128 {
        runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
        let (_, running) = runner.run_steps(512, None);
        if !running {
            break;
        }
    }
    assert!(runner.is_halted());
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(
        runner.native.application().unwrap().cpu.gpr[3],
        (u32::from(ROOT_MENU_ID as u16) << 16) | u32::from(TARGET_ITEM as u16),
        "native MenuSelect must continue and return the live regular row"
    );
}

#[test]
fn parked_native_call_executes_68k_code_and_nested_traps_in_shared_memory() {
    const M68K_ENTRY: u32 = 0x0301_0000;
    const RESULT: u32 = 0x0302_0000;
    const STACK_BASE: u32 = 0x0303_0000;
    const INITIAL_SP: u32 = STACK_BASE + 0x80;
    const RETURN_PC: u32 = 0x0304_0000;

    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_launch_state(41, 1, 0);
    runner.init_app(&app);

    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.memory.add_region(
        M68K_ENTRY,
        vec![
            0x59, 0x8f, // SUBQ.L #4,SP: TickCount result slot
            0xa9, 0x75, // TickCount
            0x20, 0x1f, // MOVE.L (SP)+,D0
            0x23, 0xc0, 0x03, 0x02, 0x00, 0x00, // MOVE.L D0,RESULT
            0x4e, 0x75, // RTS
        ],
    );
    ppc_app.memory.add_region(RESULT, vec![0; 4]);
    ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Data(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
    let (first_steps, first_running) = runner.run_steps(2, None);
    assert_eq!(first_steps, 2);
    assert!(first_running);
    assert_eq!(runner.bus.read_long(RESULT), 0);
    assert!(!runner.dispatcher.guest_calls.is_empty());

    let (steps, running) = runner.run_steps(64, None);

    assert_eq!(steps, 64);
    assert!(running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application_mut().expect("PPC app retained");
    assert_eq!(ppc_app.memory.read_u32_be(RESULT), Some(41));
    assert_eq!(ppc_app.cpu.gpr[3], 41);
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.lr, PPC_CODE_BASE);
    assert_eq!(runner.bus.read_long(RESULT), 41);
}

#[test]
fn parked_powerpc_to_68k_call_obeys_guest_vector_10() {
    const M68K_ENTRY: u32 = 0x0301_1000;
    const HANDLER: u32 = 0x0301_1100;
    const STACK_BASE: u32 = 0x0303_1000;
    const INITIAL_SP: u32 = STACK_BASE + 0x80;
    const RETURN_PC: u32 = 0x0304_1000;
    const MARKER: u32 = 0xA10E_6040;

    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    let ppc_app = runner.native.application_mut().expect("PPC app");
    ppc_app.memory.add_region(
        M68K_ENTRY,
        vec![
            0xA9, 0x75, // TickCount: must enter the replacement vector
            0x4E, 0x75, // RTS
        ],
    );
    ppc_app.memory.add_region(
        HANDLER,
        vec![
            0x2C, 0x3C, 0xA1, 0x0E, 0x60, 0x40, // MOVE.L #MARKER,D6
            0x54, 0xAF, 0x00, 0x02, // ADDQ.L #2,2(SP)
            0x4E, 0x73, // RTE
        ],
    );
    ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(crate::guest_call::M68kResultSource::Data(6)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

    // Interapplication Communication (1993), p. 1-87: an A-line causes
    // the processor to fetch vector 10 from `$28` and jump to it. The
    // parked 68k adapter must preserve that rule while PPC owns the app.
    runner.bus.write_long(0x28, HANDLER);
    let (_steps, running) = runner.run_steps(64, None);

    assert!(running);
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.gpr[3], MARKER);
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
}

#[test]
fn parked_native_special_case_executes_68k_and_writes_native_outputs() {
    use crate::guest_call::{M68kResultSource, PowerPcArguments};
    use crate::mixed_mode::special_case;

    const M68K_ENTRY: u32 = 0x0305_0000;
    const OUTPUTS: u32 = 0x0305_1000;
    const STACK_BASE: u32 = 0x0305_2000;
    const INITIAL_SP: u32 = STACK_BASE + 0x80;
    const RETURN_PC: u32 = 0x0305_3000;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app.memory.add_region(
        M68K_ENTRY,
        [
            0x203c, 0x0001, 0x1111, // MOVE.L #$00011111,D0
            0x323c, 0x2222, // MOVE.W #$2222,D1
            0x343c, 0x0033, // MOVE.W #$0033,D2
            0x4e75, // RTS
        ]
        .into_iter()
        .flat_map(u16::to_be_bytes)
        .collect(),
    );
    ppc_app.memory.add_region(OUTPUTS, vec![0; 8]);
    ppc_app.memory.add_region(STACK_BASE, vec![0; 0x100]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    let arguments =
        PowerPcArguments::from_slice(&[0, 0, 0, 0, 0, 0, OUTPUTS, OUTPUTS + 2, OUTPUTS + 4])
            .unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            crate::guest_call::M68kRegisterState::default(),
            Some(M68kResultSource::SpecialCase {
                selector: u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
                arguments,
                stack_result: None,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Mask(0xff),
        ));

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let (steps, running) = runner.run_steps(64, None);

    assert!(steps > 0);
    assert!(running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    let ppc_app = runner.native.application_mut().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.gpr[3], 1);
    assert_eq!(ppc_app.memory.read_u16_be(OUTPUTS), Some(0x1111));
    assert_eq!(ppc_app.memory.read_u16_be(OUTPUTS + 2), Some(0x2222));
    assert_eq!(ppc_app.memory.read_u8(OUTPUTS + 4), Some(0x33));
}

struct ClassicPowerPcMdefFixture {
    runner: FixtureRunner,
    menu: u32,
    record: u32,
    marker: u32,
    entry: u32,
    stack: u32,
}

fn classic_powerpc_mdef_fixture() -> ClassicPowerPcMdefFixture {
    classic_powerpc_mdef_fixture_with_tick_identity(false)
}

fn classic_powerpc_mdef_fixture_with_tick_identity(
    already_shared_tick: bool,
) -> ClassicPowerPcMdefFixture {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    };
    const MENU: u32 = 0x0030_0000;
    const RECORD: u32 = MENU + 0x100;
    const MDEF: u32 = MENU + 0x200;
    const DESCRIPTOR: u32 = MENU + 0x300;
    const TVECTOR: u32 = MENU + 0x400;
    const MARKER: u32 = MENU + 0x500;
    const ENTRY: u32 = MENU + 0x600;
    const STACK: u32 = 0x0070_0000;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    let mut native = halted_ppc_app_with_sound(PpcSoundState::default())
        .ppc
        .take()
        .unwrap();
    native
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    native.memory.add_region(
        CALLBACK,
        [
            0x8124_0000u32, // lwz r9,0(r4): live MenuHandle
            0x3940_01b0,    // li r10,432
            0xb149_0002,    // sth r10,menuWidth(r9)
            0x3940_007b,    // li r10,123
            0xb149_0004,    // sth r10,menuHeight(r9)
            0x3d20_0000 | (MARKER >> 16),
            0x6129_0000 | (MARKER & 0xffff),
            0x8149_0000, // lwz r10,0(r9)
            0x394a_0001, // addi r10,r10,1
            0x9149_0000, // stw r10,0(r9)
            0x3940_0002, // li r10,2
            0xb147_0000, // sth r10,0(r7): chosen item
            0x4e80_0020, // blr
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );
    let classic = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007f_ffc0,
        size_resource: None,
    };
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    if already_shared_tick {
        native.tick_state = runner
            .process_context
            .migrated_handles()
            .ticks
            .shared_handle();
    }
    runner.stage_ppc_companion(native);
    runner.init_app(&classic);
    runner.bus.write_long(MENU, RECORD);
    runner.bus.write_word(RECORD, 140);
    runner.bus.write_long(RECORD + 6, MDEF);
    runner.bus.write_long(RECORD + 10, u32::MAX);
    runner.bus.write_long(MDEF, DESCRIPTOR);
    runner
        .bus
        .write_word(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
    runner
        .bus
        .write_byte(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION);
    runner.bus.write_word(DESCRIPTOR + 10, 0);
    let routine = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    runner.bus.write_long(routine, 0x0000_ff80);
    runner.bus.write_byte(
        routine + ROUTINE_RECORD_ISA_OFFSET,
        ROUTINE_RECORD_POWERPC_ISA,
    );
    runner.bus.write_word(
        routine + ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_FLAG_USE_NATIVE_ISA,
    );
    runner
        .bus
        .write_long(routine + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR);
    runner.bus.write_long(TVECTOR, CALLBACK);
    runner.bus.write_long(TVECTOR + 4, 0);
    runner.bus.write_word(ENTRY, 0xA948); // CalcMenuSize
    runner.bus.write_word(ENTRY + 2, 0x60fe); // park after the call
    runner.bus.write_long(STACK, MENU);
    runner.m68k.cpu.write_reg(Register::PC, ENTRY);
    runner.m68k.cpu.write_reg(Register::A7, STACK);
    ClassicPowerPcMdefFixture {
        runner,
        menu: MENU,
        record: RECORD,
        marker: MARKER,
        entry: ENTRY,
        stack: STACK,
    }
}

#[test]
fn nested_classic_mdef_preserves_its_wrapper_arguments_and_caller_stack() {
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        record,
        marker,
        entry,
        stack,
    } = classic_powerpc_mdef_fixture();
    let inner = menu + 0x1000;
    let inner_record = inner + 0x100;
    let inner_handle = inner + 0x200;
    let outer_code = menu + 0x2000;
    let inner_code = outer_code + 0x200;
    let outer_handle = runner.bus.read_long(record + 6);
    runner.bus.write_long(outer_handle, outer_code);
    runner.bus.write_long(inner, inner_record);
    runner.bus.write_word(inner_record, 141);
    runner.bus.write_long(inner_record + 6, inner_handle);
    runner.bus.write_long(inner_record + 10, u32::MAX);
    runner.bus.write_long(inner_handle, inner_code);
    for (address, words) in [
        (
            outer_code,
            vec![
                0x206f,
                12, // MOVEA.L menuRect(SP),A0
                0x30bc,
                0x1122, // MOVE.W #$1122,(A0)
                0x2f08, // retain outer rectangle pointer
                0x2f3c,
                (inner >> 16) as u16,
                inner as u16,
                0xa948, // nested CalcMenuSize
                0x205f, // restore outer pointer
                0x33d0,
                (marker >> 16) as u16,
                marker as u16,
                0x4e74,
                18, // RTD #18
            ],
        ),
        (inner_code, vec![0x206f, 12, 0x30bc, 0x3344, 0x4e74, 18]),
    ] {
        for (index, word) in words.into_iter().enumerate() {
            runner.bus.write_word(address + index as u32 * 2, word);
        }
    }
    for _ in 0..8 {
        let (_, running) = runner.run_steps(128, None);
        assert!(running);
    }
    assert_eq!(runner.bus.read_word(marker), 0x1122);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

fn fire_menu_test_timer(runner: &mut FixtureRunner, menu: u32, marker: u32) {
    let timer = menu + 0x3000;
    for (index, word) in [
        0x33fc,
        1,
        ((marker + 4) >> 16) as u16,
        (marker + 4) as u16,
        0x4e75,
    ]
    .into_iter()
    .enumerate()
    {
        runner.bus.write_word(timer + index as u32 * 2, word);
    }
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: timer + 0x100,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: timer,
        active: true,
        fire_at_tick: 1,
        fire_at_subtick: 1_000_000,
        last_fired_tick: None,
    });
    runner.fire_timer_tasks(1);
}

#[test]
fn timer_at_classic_mdef_return_preserves_callback_code_and_stack() {
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        marker,
        entry,
        stack,
        ..
    } = classic_powerpc_mdef_fixture();
    let frame = stack + 4 - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION;
    let mut reached_return = false;
    for _ in 0..512 {
        let return_instruction = if runner.bus.read_word(frame + 48) == 0x4e74 {
            frame + 48
        } else {
            frame + 54
        };
        if runner.m68k.cpu.read_reg(Register::PC) == return_instruction {
            reached_return = true;
            break;
        }
        let (_, running) = runner.run_steps(1, None);
        assert!(running);
    }
    assert!(
        reached_return,
        "callback must reach its final return instruction"
    );
    fire_menu_test_timer(&mut runner, menu, marker);
    assert!(runner.active_interrupt_callback.is_some());
    for _ in 0..8 {
        let (_, running) = runner.run_steps(128, None);
        assert!(running);
    }
    assert_eq!(runner.bus.read_word(marker + 4), 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn classic_calc_menu_size_executes_powerpc_mdef_and_resumes_once() {
    let ClassicPowerPcMdefFixture {
        mut runner,
        record,
        marker,
        entry,
        stack,
        ..
    } = classic_powerpc_mdef_fixture();
    for _ in 0..8 {
        let (_, running) = runner.run_steps(128, None);
        assert!(running);
    }
    assert_eq!(
        runner.bus.read_long(marker),
        1,
        "PowerPC MDEF must run exactly once"
    );
    assert_eq!(runner.bus.read_word(record + 2), 432);
    assert_eq!(runner.bus.read_word(record + 4), 123);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), entry + 2);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn classic_menu_select_retains_powerpc_mdef_until_mouse_release() {
    run_classic_menu_select_with_powerpc_mdef(false);
}

#[test]
fn timer_after_classic_mdef_return_preserves_pending_tracking_results() {
    run_classic_menu_select_with_powerpc_mdef(true);
}

fn run_classic_menu_select_with_powerpc_mdef(interrupt: bool) {
    run_classic_menu_select_with_powerpc_mdef_identity(interrupt, false);
}

pub(super) fn run_classic_menu_select_with_powerpc_mdef_identity(
    interrupt: bool,
    already_shared_tick: bool,
) {
    use crate::memory::globals::addr;
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        record,
        marker,
        entry,
        stack,
    } = classic_powerpc_mdef_fixture_with_tick_identity(already_shared_tick);
    let migrated_handles = runner.process_context.migrated_handles();
    runner.dispatcher.menu_bar_hidden = false;
    runner.bus.write_word(addr::MBAR_HEIGHT, 20);
    runner.bus.write_word(addr::MENU_FLASH, 0);
    runner.bus.write_word(record + 2, 80);
    runner.bus.write_word(record + 4, 32);
    runner.bus.write_bytes(
        record + 14,
        b"\x06Custom\x01A\x00\x00\x00\x00\x01B\x00\x00\x00\x00\x00",
    );
    runner.bus.write_word(stack, 0);
    runner.bus.write_long(stack + 2, menu);
    runner
        .dispatcher
        .dispatch_menu(true, 0x135, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap()
        .unwrap();
    runner.dispatcher.draw_menu_bar_to_fb(&mut runner.bus);
    let original_port = *runner.dispatcher.current_port;
    runner.bus.write_word(entry, 0xA93D);
    runner.bus.write_word(stack, 10);
    runner.bus.write_word(stack + 2, 16);
    runner.bus.write_long(stack + 4, 0);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    runner.push_canonical_mouse_down(10, 16);
    if interrupt {
        let mut parked = false;
        for _ in 0..512 {
            if runner.m68k.cpu.read_reg(Register::PC)
                == stack - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION + 52
                && runner.m68k.cpu.read_reg(Register::A7)
                    == stack - crate::execution_m68k::M68kMenuDefinitionFrame::RESERVATION
            {
                parked = true;
                break;
            }
            assert!(runner.run_steps(1, None).1);
        }
        assert!(parked, "MDEF return must keep its result reservation live");
        fire_menu_test_timer(&mut runner, menu, marker);
    }

    for _ in 0..8 {
        assert!(runner.run_steps(128, None).1);
    }
    let rect = runner
        .process_context
        .menu_tracking()
        .expect("classic tracking remains active")
        .dropdown_rect();
    assert!(
        runner.bus.read_long(marker) > 0,
        "PowerPC draw callback ran"
    );
    let (v, h) = (rect.0 + 24, rect.1 + 16);
    runner.dispatcher.set_mouse_position(v, h);
    for _ in 0..8 {
        assert!(runner.run_steps(128, None).1);
    }
    runner.push_canonical_mouse_up(v, h);
    for _ in 0..16 {
        assert!(runner.run_steps(128, None).1);
        if runner.process_context.menu_tracking().is_none()
            && runner.dispatcher.guest_calls.is_empty()
        {
            break;
        }
    }
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert!(runner
        .native
        .adapter_mut(NativeEngineRole::Companion)
        .expect("mixed callback companion retained")
        .is_constructed_from_migrated_handles(&migrated_handles));
    assert_eq!(runner.bus.read_long(stack + 4), (140 << 16) | 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert_eq!(*runner.dispatcher.current_port, original_port);
    if interrupt {
        assert_eq!(runner.bus.read_word(marker + 4), 1);
    }

    let completed_result = runner.bus.read_long(stack + 4);
    let completed_sp = runner.m68k.cpu.read_reg(Register::A7);
    let completed_pc = runner.m68k.cpu.read_reg(Register::PC);
    let completed_marker = runner.bus.read_long(marker);
    assert!(runner.run_steps(16, None).1);
    assert_eq!(runner.bus.read_long(stack + 4), completed_result);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), completed_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), completed_pc);
    assert_eq!(runner.bus.read_long(marker), completed_marker);
    assert!(runner.process_context.menu_tracking().is_none());
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn classic_menu_wait_resumes_without_refiring_new_trap_patch() {
    for auto_pop in [false, true] {
        for custom in [false, true] {
            run_menu_patch_during_tracking(auto_pop, custom, false, false);
        }
    }
}

#[test]
fn native_menu_hook_runs_classic_guest_code_and_releases_ownership() {
    use crate::loader::ppc::tests::native_menu_hook_fixture;
    use crate::memory::globals::addr;
    for cancel in [false, true] {
        let (mut native, _, _) = native_menu_hook_fixture();
        native.cpu.gpr[3] = (10 << 16) | 12;
        native.memory.write_u16_be(addr::MENU_FLASH, 0).unwrap();
        let original_sp = native.cpu.gpr[1];
        let original_return = native.cpu.lr;
        let app = LoadedApp::from_ppc(native);
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.set_ui_theme(UiThemeId::ClassicSystem7);
        runner.init_app(&app);
        let code = 0x0030_8000;
        let marker = code + 0x100;
        for (index, word) in [
            0x42a7, // CLR.L -(SP): inner result
            0x2f3c,
            0x01f4,
            0x01f4, // inner MenuSelect outside the menu bar
            0xa93d,
            0x23df,
            ((marker + 4) >> 16) as u16,
            (marker + 4) as u16,
            0x52b9,
            (marker >> 16) as u16,
            marker as u16,
            0x4e75,
        ]
        .into_iter()
        .enumerate()
        {
            runner.bus.write_word(code + index as u32 * 2, word);
        }
        runner.bus.write_long(marker + 4, u32::MAX);
        runner.bus.write_long(0x0a30, code);
        runner.push_canonical_mouse_down(10, 12);
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
        }
        assert!(
            runner.bus.read_long(marker) > 0,
            "native MenuSelect invoked the classic hook"
        );
        assert!(runner.process_context.menu_tracking().is_some());
        assert_eq!(
            runner.bus.read_long(marker + 4),
            0,
            "nested no-hit MenuSelect returned independently"
        );
        if cancel {
            runner.push_canonical_mouse_up(500, 500);
        } else {
            runner.push_canonical_mouse_up(28, 20);
        }
        for _ in 0..32 {
            runner.advance_menu_presentation_clock(std::time::Duration::from_millis(17));
            if !runner.run_steps(128, None).1 {
                break;
            }
        }
        assert!(runner.is_halted());
        assert!(runner.process_context.menu_tracking().is_none());
        assert!(runner.dispatcher.guest_calls.is_empty());
        let native = runner.native.application_mut().unwrap();
        assert_eq!(native.cpu.pc, original_return);
        assert_eq!(native.cpu.gpr[1], original_sp);
        assert_eq!(native.cpu.gpr[3], if cancel { 0 } else { (128 << 16) | 1 });
    }
}

#[test]
fn classic_menu_hook_uses_owned_stack_frame_and_restores_registers() {
    for auto_pop in [false, true] {
        for native_hook in [false, true] {
            run_menu_patch_during_tracking(auto_pop, false, true, native_hook);
        }
    }
}

fn run_menu_patch_during_tracking(auto_pop: bool, custom: bool, hook: bool, native_hook: bool) {
    use crate::memory::globals::addr;
    let ClassicPowerPcMdefFixture {
        mut runner,
        menu,
        record,
        marker,
        entry,
        stack,
    } = classic_powerpc_mdef_fixture();
    runner.dispatcher.menu_bar_hidden = false;
    runner.bus.write_word(addr::MBAR_HEIGHT, 20);
    runner.bus.write_word(addr::MENU_FLASH, 0);
    if !custom {
        let code = runner
            .bus
            .alloc(crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM.len() as u32);
        runner
            .bus
            .write_bytes(code, &crate::menu_manager::STANDARD_MENU_DEFINITION_SHIM);
        let handle = runner.bus.alloc(4);
        runner.bus.write_long(handle, code);
        runner.bus.write_long(record + 6, handle);
    }
    runner.bus.write_word(record + 2, 80);
    runner.bus.write_word(record + 4, 32);
    runner.bus.write_bytes(
        record + 14,
        b"\x06Custom\x01A\x00\x00\x00\x00\x01B\x00\x00\x00\x00\x00",
    );
    runner.bus.write_word(stack, 0);
    runner.bus.write_long(stack + 2, menu);
    runner
        .dispatcher
        .dispatch_menu(true, 0x135, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap()
        .unwrap();
    runner.dispatcher.draw_menu_bar_to_fb(&mut runner.bus);
    let original_port = *runner.dispatcher.current_port;
    let hook_port = runner.bus.alloc(170);
    let original_port_image = runner.bus.read_bytes(original_port, 170).to_vec();
    runner.bus.write_bytes(hook_port, &original_port_image);
    let parameters = stack + if auto_pop { 4 } else { 0 };
    let return_pc = entry + if auto_pop { 0x100 } else { 2 };
    runner.bus.write_word(return_pc, 0x60fe);
    runner
        .bus
        .write_word(entry, if auto_pop { 0xAD3D } else { 0xA93D });
    if auto_pop {
        runner.bus.write_long(stack, return_pc);
    }
    runner.bus.write_word(parameters, 10);
    runner.bus.write_word(parameters + 2, 16);
    runner.bus.write_long(parameters + 4, 0);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    let hook_marker = runner.bus.alloc(4);
    let hook_after_yield_marker = runner.bus.alloc(4);
    let cooperative_switch = hook && !native_hook && !auto_pop;
    let mut hook_yield_resume_pc = None;
    let mut worker_yield_result = None;
    if hook {
        let mut hook_words = vec![
            0x7e63, // MOVEQ #99,D7
            0x2c7c, // MOVEA.L #value,A6
            0x1234, 0x5678,
        ];
        if !native_hook {
            hook_words.extend([
                0x2f3c,
                (hook_port >> 16) as u16,
                hook_port as u16,
                0xa873, // SetPort(hook_port)
            ]);
        }
        hook_words.extend([
            0x52b9, // ADDQ.L #1,marker
            (hook_marker >> 16) as u16,
            hook_marker as u16,
        ]);
        if cooperative_switch {
            hook_words.extend([
                0x558f, // SUBQ.L #2,SP: Pascal result word
                0x42a7, // CLR.L -(SP): synthetic suggested ThreadID
                0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                0xabf2, // ThreadDispatch
                0x548f, // ADDQ.L #2,SP: pop Pascal result
            ]);
            hook_words.extend([
                0x52b9, // ADDQ.L #1,after-yield marker
                (hook_after_yield_marker >> 16) as u16,
                hook_after_yield_marker as u16,
            ]);
        }
        hook_words.extend([
            0x5279, // ADDQ.W #1,menuWidth
            ((record + 2) >> 16) as u16,
            (record + 2) as u16,
            0x4e75,
        ]);
        let code = runner.bus.alloc((hook_words.len() * 2) as u32);
        if cooperative_switch {
            let trap_index = hook_words.iter().position(|word| *word == 0xabf2).unwrap();
            hook_yield_resume_pc = Some(code + (trap_index as u32 + 1) * 2);
        }
        for (index, word) in hook_words.into_iter().enumerate() {
            runner.bus.write_word(code + index as u32 * 2, word);
        }
        runner.bus.write_long(0x0a30, code);
        if native_hook {
            use crate::guest_procedure::{
                ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
                ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA,
                ROUTINE_RECORD_FLAGS_OFFSET, ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
                ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            };
            let native_code = runner.bus.alloc(48);
            for (index, word) in [
                0x3ce0_0000 | (record >> 16),
                0x60e7_0000 | (record & 0xffff),
                0xa147_0002,
                0x394a_0001,
                0xb147_0002,
                0x3d00_0000 | (hook_marker >> 16),
                0x6108_0000 | (hook_marker & 0xffff),
                0x8128_0000,
                0x3929_0001,
                0x9128_0000,
                0x4e80_0020,
            ]
            .into_iter()
            .enumerate()
            {
                runner.bus.write_long(native_code + index as u32 * 4, word);
            }
            let descriptor = runner.bus.alloc(64);
            let tvector = descriptor + 48;
            let record = descriptor + ROUTINE_DESCRIPTOR_HEADER_SIZE;
            runner
                .bus
                .write_word(descriptor, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
            runner
                .bus
                .write_byte(descriptor + 2, ROUTINE_DESCRIPTOR_VERSION);
            runner.bus.write_byte(
                record + ROUTINE_RECORD_ISA_OFFSET,
                ROUTINE_RECORD_POWERPC_ISA,
            );
            runner.bus.write_word(
                record + ROUTINE_RECORD_FLAGS_OFFSET,
                ROUTINE_FLAG_USE_NATIVE_ISA,
            );
            runner
                .bus
                .write_long(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, tvector);
            runner.bus.write_long(tvector, native_code);
            runner.bus.write_long(tvector + 4, 0);
            runner.bus.write_long(0x0a30, descriptor);
        }

        runner.m68k.cpu.write_reg(Register::D7, 0x77777777);
        runner.m68k.cpu.write_reg(Register::A6, 0x66666666);
    }
    if hook && !native_hook {
        let hook_pointer = runner.bus.read_long(0x0a30);
        runner.bus.write_long(0x0a30, 0);
        runner.push_canonical_mouse_down(10, 16);
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
            if runner
                .dispatcher
                .menu_tracking
                .request_menu_hook(true)
                .is_some()
            {
                break;
            }
        }
        let key = runner
            .dispatcher
            .menu_tracking
            .request_menu_hook(true)
            .expect("held menu requests its classic hook");
        let call_depth = runner.dispatcher.guest_calls.depth();
        runner.bus.write_long(0x0a30, hook_pointer);
        let procedure = crate::guest_procedure::resolve_guest_procedure(
            &mut runner.bus,
            hook_pointer,
            0,
            None,
            GuestIsa::M68k,
            GuestIsa::M68k,
        )
        .expect("classic hook remains resolvable");
        assert_eq!(procedure.isa, GuestIsa::M68k);
        assert_eq!(procedure.entry, hook_pointer);
        let valid_sp = runner.m68k.cpu.read_reg(Register::A7);
        let frame_start = runner.bus.alloc(114);
        let frame_len = 114;
        runner
            .bus
            .protect_readonly_code(frame_start, frame_len as u32);
        assert!(runner.bus.is_guest_address_mapped(frame_start, frame_len));
        assert!(!runner.bus.is_guest_address_writable(frame_start, frame_len));
        let frame_snapshot = runner.bus.read_bytes(frame_start, frame_len).to_vec();
        runner
            .m68k
            .cpu
            .write_reg(Register::A7, frame_start + frame_len as u32);
        assert!(!runner.fire_menu_hook_proc(0xa93d));
        assert_eq!(
            runner.bus.read_bytes(frame_start, frame_len),
            frame_snapshot
        );
        assert_eq!(
            runner.dispatcher.menu_tracking.request_menu_hook(true),
            Some(key)
        );
        assert_eq!(runner.dispatcher.menu_tracking.context().classic_port, None);
        assert_eq!(runner.dispatcher.guest_calls.depth(), call_depth);
        runner.m68k.cpu.write_reg(Register::A7, valid_sp);
        if cooperative_switch {
            let worker = ExecutionTaskId::from_thread_id(3);
            let worker_entry = runner.bus.alloc(8);
            let worker_stack = runner.bus.alloc(64);
            let worker_sp = worker_stack + 58;
            for (index, word) in [
                0x303c, 0x0205, // MOVE.W #YieldToAnyThread,D0
                0xabf2, // ThreadDispatch back to the application
                0x60fe, // BRA.S -2 if no successor is runnable
            ]
            .into_iter()
            .enumerate()
            {
                runner.bus.write_word(worker_entry + index as u32 * 2, word);
            }
            runner.bus.write_long(worker_sp, 0);
            runner.bus.write_word(worker_sp + 4, 0xbeef);
            worker_yield_result = Some(worker_sp + 4);
            assert!(runner.dispatcher.guest_calls.register_task(worker));
            assert!(runner.dispatcher.guest_calls.set_thread_storage(
                worker,
                crate::guest_call::ThreadStorage {
                    stack_base: worker_stack,
                    stack_limit: worker_stack + 64,
                    ..Default::default()
                }
            ));
            assert!(runner.dispatcher.guest_calls.save_cooperative_context(
                worker,
                CooperativeThread {
                    a_regs: [0, 0, 0, 0, 0, 0, 0, worker_sp],
                    pc: worker_entry,
                    ..Default::default()
                }
            ));
            assert!(runner
                .dispatcher
                .guest_calls
                .set_scheduling_state(worker, crate::execution_kernel::ExecutionTaskState::Ready));
        }
        assert!(runner.fire_menu_hook_proc(0xa93d));
        assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), Some(key));
        assert!(runner
            .dispatcher
            .menu_tracking
            .context()
            .classic_port
            .is_some());
        if cooperative_switch {
            runner.bus.write_long(0x0a30, 0);
            let worker = ExecutionTaskId::from_thread_id(3);
            for _ in 0..32 {
                runner.run_steps(1, None);
                if runner.dispatcher.guest_calls.current_task() == worker {
                    break;
                }
            }
            assert_eq!(runner.dispatcher.guest_calls.current_task(), worker);
            assert_eq!(runner.bus.read_long(hook_marker), 1);
            assert_eq!(runner.bus.read_long(hook_after_yield_marker), 0);
            assert_eq!(runner.bus.read_byte(0x0172), 0);
            assert_eq!(*runner.dispatcher.current_port, hook_port);
            assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), None);
            assert!(runner.dispatcher.menu_tracking.menu_hook_is_pending(key));
            assert!(runner
                .dispatcher
                .menu_tracking
                .ready_call(GuestIsa::M68k)
                .is_none());
            let parked = runner
                .dispatcher
                .guest_calls
                .cooperative_context(ExecutionTaskId::APPLICATION)
                .expect("suspended hook context");
            assert_eq!(parked.pc, hook_yield_resume_pc.unwrap());
            for _ in 0..32 {
                runner.run_steps(1, None);
                if runner.dispatcher.guest_calls.current_task() == ExecutionTaskId::APPLICATION {
                    break;
                }
            }
            assert_eq!(
                runner.dispatcher.guest_calls.current_task(),
                ExecutionTaskId::APPLICATION
            );
            assert_eq!(runner.bus.read_word(worker_yield_result.unwrap()), 0);
            assert_eq!(runner.m68k.cpu.read_reg(Register::PC), parked.pc);
            assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parked.a_regs[7]);
            assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), Some(key));
            for _ in 0..32 {
                runner.run_steps(1, None);
                assert!(
                        runner.process_context.menu_tracking().is_some(),
                        "held root vanished before hook receipt consumption: task={:?} pc={:08x} button={:02x}",
                        runner.dispatcher.guest_calls.current_task(),
                        runner.m68k.cpu.read_reg(Register::PC),
                        runner.bus.read_byte(0x0172),
                    );
                if runner.bus.read_long(hook_after_yield_marker) == 1
                    && runner.dispatcher.menu_tracking.menu_hook_key().is_none()
                {
                    break;
                }
            }
            assert_eq!(runner.bus.read_long(hook_after_yield_marker), 1);
            assert_eq!(runner.dispatcher.menu_tracking.menu_hook_key(), None);
            assert!(runner.process_context.menu_tracking().is_some());
        }
    } else {
        runner.push_canonical_mouse_down(10, 16);
    }

    if !cooperative_switch {
        for _ in 0..8 {
            assert!(runner.run_steps(128, None).1);
        }
    }
    if hook && !native_hook {
        assert_eq!(*runner.dispatcher.current_port, hook_port);
        if cooperative_switch {
            assert_eq!(runner.bus.read_long(hook_after_yield_marker), 1);
        }
    }
    let rect = runner
        .process_context
        .menu_tracking()
        .expect("classic tracking remains active")
        .dropdown_rect();
    if custom {
        assert!(
            runner.bus.read_long(marker) > 0,
            "PowerPC draw callback ran"
        );
    }
    let (v, h) = (rect.0 + 24, rect.1 + 16);
    runner.dispatcher.set_mouse_position(v, h);
    for _ in 0..8 {
        assert!(runner.run_steps(128, None).1);
    }
    runner.push_canonical_mouse_up(v, h);
    let patch_marker = runner.bus.alloc(4);
    let patch = runner.bus.alloc(12);
    for (index, word) in [
        0x23fc,
        0,
        1,
        (patch_marker >> 16) as u16,
        patch_marker as u16,
        0x4e75,
    ]
    .into_iter()
    .enumerate()
    {
        runner.bus.write_word(patch + index as u32 * 2, word);
    }
    runner
        .dispatcher
        .install_trap_address(&mut runner.bus, 0xa93d, patch)
        .unwrap();
    for _ in 0..16 {
        assert!(runner.run_steps(128, None).1);
        assert_eq!(
            runner.bus.read_long(patch_marker),
            0,
            "a new MenuSelect patch intercepted the already-active interaction"
        );
        if runner.process_context.menu_tracking().is_none()
            && runner.dispatcher.guest_calls.is_empty()
        {
            break;
        }
    }
    assert!(runner.process_context.menu_tracking().is_none(),
            "tracking stayed live: pc={:08x}, sp={:08x}, patch={:08x}, marker={}, depth={}, pending={:?}",
            runner.m68k.cpu.read_reg(Register::PC), runner.m68k.cpu.read_reg(Register::A7), patch,
            runner.bus.read_long(patch_marker), runner.dispatcher.guest_calls.depth(),
            runner.dispatcher.menu_tracking.as_ref().and_then(|tracking| tracking.definition.as_ref()).and_then(|definition| definition.pending_invocation()));
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(runner.bus.read_long(parameters + 4), (140 << 16) | 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parameters + 4);
    assert_eq!(*runner.dispatcher.current_port, original_port);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    if hook {
        assert!(runner.bus.read_long(hook_marker) > 0, "the guest hook ran");
        assert!(runner.bus.read_word(record + 2) > 80);
        assert_eq!(runner.m68k.cpu.read_reg(Register::D7), 0x77777777);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A6), 0x66666666);
        assert!(runner.dispatcher.guest_calls.is_empty());
        assert!(runner.active_interrupt_callback.is_none());
    }

    if auto_pop {
        runner.bus.write_long(stack, return_pc);
    }
    runner.m68k.cpu.write_reg(Register::PC, entry);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    assert!(runner.run_steps(128, None).1);
    assert_eq!(
        runner.bus.read_long(patch_marker),
        1,
        "fresh entries must still honor the new patch"
    );
}

#[test]
fn standalone_classic_process_enters_powerpc_routine_descriptor_and_resumes_once() {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    };
    use crate::mixed_mode::proc_info;

    const DESCRIPTOR: u32 = 0x0030_0000;
    const TVECTOR: u32 = 0x0030_0100;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const CALLBACK_RTOC: u32 = 0x0030_0200;
    const M68K_RETURN: u32 = 0x0030_0300;
    const M68K_STACK: u32 = 0x0070_0000;
    const ARGUMENT: u32 = 0x1234_0000;

    let mut native = halted_ppc_app_with_sound(PpcSoundState::default())
        .ppc
        .take()
        .expect("native execution adapter");
    native
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    native.memory.add_region(
        CALLBACK,
        [
            0x3863_0007u32, // addi r3,r3,7
            0x4e80_0020,    // blr
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );

    let classic = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007f_ffc0,
        size_resource: None,
    };
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.stage_ppc_companion(native);
    runner.init_app(&classic);
    assert!(!runner.is_powerpc_app());

    let proc_info = proc_info::PASCAL_STACK_BASED
        | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
        | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
    runner
        .bus
        .write_word(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP);
    runner
        .bus
        .write_byte(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION);
    runner.bus.write_word(DESCRIPTOR + 10, 0);
    let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    runner.bus.write_long(record, proc_info);
    runner.bus.write_byte(
        record + ROUTINE_RECORD_ISA_OFFSET,
        ROUTINE_RECORD_POWERPC_ISA,
    );
    runner.bus.write_word(
        record + ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_FLAG_USE_NATIVE_ISA,
    );
    runner
        .bus
        .write_long(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR);
    runner.bus.write_long(TVECTOR, CALLBACK);
    runner.bus.write_long(TVECTOR + 4, CALLBACK_RTOC);
    runner.bus.write_long(M68K_STACK, M68K_RETURN);
    runner.bus.write_long(M68K_STACK + 4, ARGUMENT);
    runner.bus.write_word(M68K_RETURN, 0x201f); // MOVE.L (SP)+,D0
    runner.bus.write_word(M68K_RETURN + 2, 0x4e71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, DESCRIPTOR);
    runner.m68k.cpu.write_reg(Register::A7, M68K_STACK);

    let (classic_steps, classic_running) = runner.run_steps(2, None);
    assert!(classic_steps > 0);
    assert!(classic_running);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), DESCRIPTOR + 2);
    assert!(runner.dispatcher.guest_calls.has_powerpc_from_m68k());

    let (native_steps, native_running) = runner.run_steps(64, None);
    assert!(native_steps > 0);
    assert!(native_running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), M68K_RETURN);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), M68K_STACK + 8);
    assert_eq!(runner.bus.read_long(M68K_STACK + 8), ARGUMENT + 7);

    let (resumed_steps, resumed_running) = runner.run_steps(1, None);
    assert_eq!(resumed_steps, 1);
    assert!(resumed_running);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), ARGUMENT + 7);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), M68K_STACK + 12);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), M68K_RETURN + 2);
    assert!(runner.dispatcher.guest_calls.is_empty());
}

#[test]
fn raw_os_trap_patch_can_execute_a_native_routine_descriptor() {
    use crate::guest_call::{GuestCallTarget, M68kRegisterState, M68kResultSource};
    use crate::guest_procedure::{
        GuestIsa, ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, ROUTINE_RECORD_SIZE,
    };
    use crate::mixed_mode::proc_info;

    const TRAP: u16 = 0xA11E; // NewPtr
    const BYTE_COUNT: u32 = 0x1234;
    const DESCRIPTOR: u32 = 0x0301_0000;
    const TVECTOR: u32 = 0x0301_0100;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const M68K_FALLBACK: u32 = 0x0301_0200;
    const M68K_ENTRY: u32 = 0x0301_0300;
    const M68K_STACK: u32 = 0x0302_0000;
    const INITIAL_SP: u32 = M68K_STACK + 0x80;
    const RETURN_PC: u32 = 0x0302_0100;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);
    ppc_app
        .memory
        .add_region(CALLBACK, 0x4e80_0020u32.to_be_bytes().to_vec()); // blr
    ppc_app.memory.add_region(DESCRIPTOR, vec![0; 0x400]);
    ppc_app.memory.add_region(M68K_STACK, vec![0; 0x200]);
    ppc_app
        .memory
        .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    ppc_app
        .memory
        .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    ppc_app.memory.write_u16_be(DESCRIPTOR + 10, 1).unwrap();

    // NewPtr's documented register-based ProcInfo passes the actual trap
    // word from D1 first, then the allocation size from D0, and returns
    // the pointer in A0. Inside Macintosh: PowerPC System Software (1994),
    // pp. 1-67--1-68, Listing 1-14.
    let proc_info = proc_info::REGISTER_BASED
        | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
        | (4 << proc_info::REGISTER_RESULT_LOCATION_PHASE)
        | (6 << proc_info::REGISTER_PARAMETER_PHASE)
        | (3 << (proc_info::REGISTER_PARAMETER_PHASE + proc_info::REGISTER_PARAMETER_WIDTH));
    let m68k_record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    ppc_app.memory.write_u32_be(m68k_record, proc_info).unwrap();
    ppc_app
        .memory
        .write_u8(
            m68k_record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_M68K_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(
            m68k_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            M68K_FALLBACK,
        )
        .unwrap();
    let native_record = m68k_record + ROUTINE_RECORD_SIZE;
    ppc_app
        .memory
        .write_u32_be(native_record, proc_info)
        .unwrap();
    ppc_app
        .memory
        .write_u8(
            native_record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_POWERPC_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u16_be(
            native_record + ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_FLAG_USE_NATIVE_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(
            native_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            TVECTOR,
        )
        .unwrap();
    ppc_app.memory.write_u32_be(TVECTOR, CALLBACK).unwrap();
    ppc_app.memory.write_u32_be(TVECTOR + 4, 0).unwrap();
    ppc_app.memory.write_u16_be(M68K_FALLBACK, 0x4e75).unwrap();
    ppc_app.memory.write_u16_be(M68K_ENTRY, TRAP).unwrap();
    ppc_app.memory.write_u16_be(M68K_ENTRY + 2, 0x4e75).unwrap();
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    let mut registers = M68kRegisterState::default();
    registers.data[0] = BYTE_COUNT;
    registers.data[1] = 0xdead_beef;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            GuestCallTarget {
                isa: GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 4,
            registers,
            Some(M68kResultSource::Address(0)),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner
        .dispatcher
        .install_trap_address(&mut runner.bus, TRAP, DESCRIPTOR)
        .expect("native descriptor patch must install");

    let (m68k_steps, m68k_running) = runner.run_steps(2, None);
    assert!(m68k_steps > 0);
    assert!(m68k_running);
    let pending = runner
        .dispatcher
        .guest_calls
        .pending_powerpc_from_m68k()
        .expect("native record should be pending");
    assert_eq!(pending.arguments.as_slice(), &[u32::from(TRAP), BYTE_COUNT]);

    let (steps, running) = runner.run_steps(64, None);

    assert!(steps > 0);
    assert!(running);
    assert!(!runner.is_halted());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert!(runner.dispatcher.pending_native_trap_calls.is_empty());
    let ppc_app = runner.native.application().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.gpr[3], u32::from(TRAP));
}

#[test]
fn nested_cross_isa_calls_restore_each_68k_cpu_context() {
    use crate::guest_procedure::{
        ROUTINE_DESCRIPTOR_HEADER_SIZE, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP,
        ROUTINE_DESCRIPTOR_VERSION, ROUTINE_FLAG_USE_NATIVE_ISA, ROUTINE_RECORD_FLAGS_OFFSET,
        ROUTINE_RECORD_ISA_OFFSET, ROUTINE_RECORD_M68K_ISA, ROUTINE_RECORD_POWERPC_ISA,
        ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    };
    use crate::mixed_mode::proc_info;

    const DESCRIPTOR: u32 = 0x0301_0100;
    const TVECTOR: u32 = 0x0301_0200;
    const PPC_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const CALLBACK_RTOC: u32 = 0x0301_0300;
    const M68K_INNER: u32 = 0x0301_0400;
    const INNER_DESCRIPTOR: u32 = 0x0301_0600;
    const M68K_STACK: u32 = 0x0302_0000;
    const INITIAL_SP: u32 = M68K_STACK + 0x80;
    const RETURN_PC: u32 = 0x0302_0100;
    const ARGUMENT: u32 = 0x10;
    const OUTER_D6: u32 = 0x1357_2468;
    const INNER_D6: u32 = 0xdead_beef;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app
        .memory
        .add_region(PPC_STACK_BASE, vec![0; PPC_STACK_SIZE as usize]);

    let inner_words = [
        0x2c3c,
        (INNER_D6 >> 16) as u16,
        INNER_D6 as u16, // MOVE.L #INNER_D6,D6
        0x202f,
        0x0004, // MOVE.L 4(SP),D0
        0x5e80, // ADDQ.L #7,D0
        0x2f40,
        0x0008, // MOVE.L D0,8(SP)
        0x4e74,
        0x0004, // RTD #4
    ];
    ppc_app.memory.add_region(
        M68K_INNER,
        inner_words.into_iter().flat_map(u16::to_be_bytes).collect(),
    );

    let proc_info = proc_info::PASCAL_STACK_BASED
        | (proc_info::SIZE_FOUR << proc_info::RESULT_SIZE_PHASE)
        | (proc_info::SIZE_FOUR << proc_info::STACK_PARAMETER_PHASE);
    let first_branch_pc = PPC_CALLBACK + 6 * 4;
    let second_branch_pc = PPC_CALLBACK + 12 * 4;
    let callback_words = [
        0x7fe8_02a6, // MFLR R31
        0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
        0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
        0x3c80_0000 | (proc_info >> 16),
        0x6084_0000 | (proc_info & 0xffff),
        0x38a0_0000 | ARGUMENT, // LI R5,ARGUMENT
        ppc_test_relative_branch(first_branch_pc, PPC_IMPORT_TRAP_BASE) | 1, // BL CallUniversalProc
        0x3c60_0000 | (INNER_DESCRIPTOR >> 16),
        0x6063_0000 | (INNER_DESCRIPTOR & 0xffff),
        0x3c80_0000 | (proc_info >> 16),
        0x6084_0000 | (proc_info & 0xffff),
        0x38a0_0000 | ARGUMENT, // LI R5,ARGUMENT
        ppc_test_relative_branch(second_branch_pc, PPC_IMPORT_TRAP_BASE) | 1, // BL CallUniversalProc
        0x7fe8_03a6,                                                          // MTLR R31
        0x3863_0007,                                                          // ADDI R3,R3,7
        0x4e80_0020,                                                          // BLR
    ];
    ppc_app.memory.add_region(
        PPC_CALLBACK,
        callback_words
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    ppc_app.memory.add_region(DESCRIPTOR, vec![0; 0x200]);
    ppc_app
        .memory
        .write_u16_be(DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    ppc_app
        .memory
        .write_u8(DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    ppc_app.memory.write_u16_be(DESCRIPTOR + 10, 0).unwrap();
    let record = DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    ppc_app.memory.write_u32_be(record, proc_info).unwrap();
    ppc_app
        .memory
        .write_u8(
            record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_POWERPC_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u16_be(
            record + ROUTINE_RECORD_FLAGS_OFFSET,
            ROUTINE_FLAG_USE_NATIVE_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET, TVECTOR)
        .unwrap();
    ppc_app.memory.write_u32_be(TVECTOR, PPC_CALLBACK).unwrap();
    ppc_app
        .memory
        .write_u32_be(TVECTOR + 4, CALLBACK_RTOC)
        .unwrap();
    ppc_app.memory.add_region(INNER_DESCRIPTOR, vec![0; 0x100]);
    ppc_app
        .memory
        .write_u16_be(INNER_DESCRIPTOR, ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP)
        .unwrap();
    ppc_app
        .memory
        .write_u8(INNER_DESCRIPTOR + 2, ROUTINE_DESCRIPTOR_VERSION)
        .unwrap();
    ppc_app
        .memory
        .write_u16_be(INNER_DESCRIPTOR + 10, 0)
        .unwrap();
    let inner_record = INNER_DESCRIPTOR + ROUTINE_DESCRIPTOR_HEADER_SIZE;
    ppc_app
        .memory
        .write_u32_be(inner_record, proc_info)
        .unwrap();
    ppc_app
        .memory
        .write_u8(
            inner_record + ROUTINE_RECORD_ISA_OFFSET,
            ROUTINE_RECORD_M68K_ISA,
        )
        .unwrap();
    ppc_app
        .memory
        .write_u32_be(
            inner_record + ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
            M68K_INNER,
        )
        .unwrap();
    ppc_app.memory.add_region(M68K_STACK, vec![0; 0x200]);
    ppc_app.memory.write_u32_be(INITIAL_SP, RETURN_PC).unwrap();
    ppc_app
        .memory
        .write_u32_be(INITIAL_SP + 4, ARGUMENT)
        .unwrap();

    let mut call_universal_proc = test_ppc_import_binding(0, "InterfaceLib", "CallUniversalProc");
    call_universal_proc.trap_pc = PPC_IMPORT_TRAP_BASE;
    call_universal_proc.dispatcher_target = PpcImportDispatcherTarget::CallUniversalProc;
    ppc_app.import_count = 1;
    ppc_app.imports = vec![call_universal_proc];
    let mut outer_registers = crate::guest_call::M68kRegisterState::default();
    outer_registers.data[6] = OUTER_D6;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: DESCRIPTOR,
                rtoc: 0,
            },
            DESCRIPTOR,
            INITIAL_SP,
            RETURN_PC,
            INITIAL_SP + 8,
            outer_registers,
            Some(crate::guest_call::M68kResultSource::Memory {
                address: INITIAL_SP + 8,
                size: 4,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut maximum_parked = 0;
    let mut inner_entries = 0;
    let mut checked_wrong_boundary = false;
    for iteration in 0..128 {
        let (steps, running) = runner.run_steps(1, None);
        assert!(
                running,
                "cross-ISA execution stopped at iteration {iteration} after {steps} steps: pc=${:08x} sp=${:08x} frames={} parked={} ppc_pc=${:08x}",
                runner.m68k.cpu.read_reg(Register::PC),
                runner.m68k.cpu.read_reg(Register::A7),
                runner.dispatcher.guest_calls.len(),
                runner.dispatcher.guest_calls.m68k_context_bank().borrow().len(),
                runner.native.application()
                    .map_or(0, |ppc_app| ppc_app.cpu.pc),
            );
        maximum_parked = maximum_parked.max(
            runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .len(),
        );
        if !runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .is_empty()
            && runner.m68k.cpu.read_reg(Register::PC) == M68K_INNER + 6
        {
            inner_entries += 1;
        }
        if !checked_wrong_boundary
            && !runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow()
                .is_empty()
        {
            let frame_count = runner.dispatcher.guest_calls.len();
            let mut native_context = runner
                .native
                .take(NativeEngineRole::Application)
                .expect("PPC app");
            let mut ppc_app = native_context.adapter_mut();
            assert!(!runner.resume_m68k_after_powerpc(&mut ppc_app));
            assert_eq!(
                runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow()
                    .len(),
                1
            );
            assert_eq!(ppc_app.toolbox_startup.execution.calls().len(), frame_count);
            runner
                .native
                .restore(native_context)
                .unwrap_or_else(|_| panic!("native context lost its owner"));
            checked_wrong_boundary = true;
        }
        if runner.dispatcher.guest_calls.is_empty() {
            break;
        }
    }

    assert!(checked_wrong_boundary);
    assert_eq!(inner_entries, 2);
    assert_eq!(maximum_parked, 1);
    assert!(runner
        .dispatcher
        .guest_calls
        .m68k_context_bank()
        .borrow()
        .is_empty());
    assert!(runner.dispatcher.guest_calls.is_empty());
    assert_eq!(runner.m68k.cpu.core.d(6), OUTER_D6);
    let ppc_app = runner.native.application_mut().expect("PPC app retained");
    assert_eq!(ppc_app.cpu.pc, PPC_CODE_BASE);
    assert_eq!(ppc_app.cpu.gpr[3], ARGUMENT + 14);
    assert_eq!(
        ppc_app.memory.read_u32_be(INITIAL_SP + 8),
        Some(ARGUMENT + 14)
    );
}

#[test]
fn reverse_powerpc_return_sets_only_the_selected_68k_ccr_bit() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.m68k.cpu.core.set_ccr(0x10);
    assert!(runner.dispatcher.guest_calls.begin_m68k_to_powerpc(
        crate::guest_call::GuestCallTarget {
            isa: crate::guest_procedure::GuestIsa::PowerPc,
            entry: PPC_CODE_BASE,
            rtoc: 0,
        },
        crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
        0x0010_0000,
        0x0010_1000,
        Some(crate::guest_call::M68kResultTarget::Ccr { mask: 0x04 }),
    ));
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let mut ppc_app = native_context.adapter_mut();
    let return_pc = 0x01f0_4000;
    ppc_app
        .toolbox_startup
        .execution
        .calls()
        .activate_powerpc_from_m68k(&mut ppc_app.cpu, return_pc)
        .unwrap();
    ppc_app.cpu.pc = return_pc;
    ppc_app.cpu.gpr[3] = 1;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .complete_powerpc_for_m68k(&mut ppc_app.cpu));

    assert!(runner.resume_m68k_after_powerpc(&mut ppc_app));

    assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x14);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x0010_0000);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x0010_1000);
    assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
    runner
        .native
        .restore(native_context)
        .unwrap_or_else(|_| panic!("native context lost its owner"));
}

#[test]
fn reverse_special_case_results_restore_every_classic_output_layout() {
    use crate::mixed_mode::special_case;

    const SCRATCH: u32 = 0x0305_0000;
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let ppc_app = native_context.adapter_mut();
    ppc_app.memory.add_region(SCRATCH, vec![0; 8]);

    runner.m68k.cpu.core.set_ccr(0x13);
    for selector in [
        special_case::EOL_HOOK,
        special_case::PROTOCOL_HANDLER,
        special_case::SOCKET_LISTENER,
    ] {
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(selector).unwrap(),
            SCRATCH,
            1,
        ));
        assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x17);
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(selector).unwrap(),
            SCRATCH,
            0,
        ));
        assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x13);
    }

    runner.m68k.cpu.core.set_d(1, 0xaaaa_0000);
    for selector in [special_case::WIDTH_HOOK, special_case::NWIDTH_HOOK] {
        assert!(M68kExecution::apply_m68k_special_case_result(
            &mut runner.m68k.cpu,
            &mut ppc_app.memory,
            u8::try_from(selector).unwrap(),
            SCRATCH,
            0x1234_5678,
        ));
        assert_eq!(runner.m68k.cpu.core.d(1), 0xaaaa_5678);
    }

    runner.m68k.cpu.core.set_d(1, 0xbbbb_0000);
    runner.m68k.cpu.core.set_d(2, 0xcccc_0000);
    ppc_app.memory.write_u16_be(SCRATCH, 0x1111).unwrap();
    ppc_app.memory.write_u16_be(SCRATCH + 2, 0x2222).unwrap();
    ppc_app.memory.write_u8(SCRATCH + 4, 1).unwrap();
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
        SCRATCH,
        1,
    ));
    assert_eq!(runner.m68k.cpu.core.d(0), 0x0001_1111);
    assert_eq!(runner.m68k.cpu.core.d(1), 0xbbbb_2222);
    assert_eq!(runner.m68k.cpu.core.d(2), 0xcccc_0001);

    runner.m68k.cpu.core.set_d(0, 0xaaaa_0000);
    runner.m68k.cpu.core.set_d(1, 0xbbbb_0000);
    ppc_app.memory.write_u16_be(SCRATCH, 0x3333).unwrap();
    ppc_app.memory.write_u16_be(SCRATCH + 2, 0x4444).unwrap();
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::TE_FIND_WORD).unwrap(),
        SCRATCH,
        0,
    ));
    assert_eq!(runner.m68k.cpu.core.d(0), 0xaaaa_3333);
    assert_eq!(runner.m68k.cpu.core.d(1), 0xbbbb_4444);

    for (offset, value) in [(0, 0x5555), (2, 0x6666), (4, 0x7777)] {
        ppc_app
            .memory
            .write_u16_be(SCRATCH + offset, value)
            .unwrap();
    }
    runner.m68k.cpu.core.set_d(2, 0xaaaa_0000);
    runner.m68k.cpu.core.set_d(3, 0xbbbb_0000);
    runner.m68k.cpu.core.set_d(4, 0xcccc_0000);
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::TE_RECALC).unwrap(),
        SCRATCH,
        0,
    ));
    assert_eq!(runner.m68k.cpu.core.d(2), 0xaaaa_5555);
    assert_eq!(runner.m68k.cpu.core.d(3), 0xbbbb_6666);
    assert_eq!(runner.m68k.cpu.core.d(4), 0xcccc_7777);

    ppc_app.memory.write_u32_be(SCRATCH, 0xcafe_babe).unwrap();
    ppc_app.memory.write_u16_be(SCRATCH + 4, 0x8888).unwrap();
    runner.m68k.cpu.core.set_d(0, 0xdddd_0000);
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::TE_DO_TEXT).unwrap(),
        SCRATCH,
        0,
    ));
    assert_eq!(runner.m68k.cpu.core.a(0), 0xcafe_babe);
    assert_eq!(runner.m68k.cpu.core.d(0), 0xdddd_8888);

    runner.m68k.cpu.core.set_d(0, 0xeeee_0000);
    assert!(M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        u8::try_from(special_case::MBAR_HOOK).unwrap(),
        SCRATCH,
        0x1234_9999,
    ));
    assert_eq!(runner.m68k.cpu.core.d(0), 0xeeee_9999);
    assert!(!M68kExecution::apply_m68k_special_case_result(
        &mut runner.m68k.cpu,
        &mut ppc_app.memory,
        13,
        SCRATCH,
        0
    ));
    runner
        .native
        .restore(native_context)
        .unwrap_or_else(|_| panic!("native context lost its owner"));
}

#[test]
fn forward_special_case_results_restore_every_native_output_layout() {
    use crate::guest_call::PowerPcArguments;
    use crate::mixed_mode::special_case;

    const SCRATCH: u32 = 0x0306_0000;
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let ppc_app = native_context.adapter_mut();
    ppc_app.memory.add_region(SCRATCH, vec![0; 0x100]);
    let arguments = |values: &[u32]| PowerPcArguments::from_slice(values).unwrap();

    for selector in [special_case::HIGH_HOOK, special_case::DRAW_HOOK] {
        let values = vec![
            0;
            if selector == special_case::HIGH_HOOK {
                2
            } else {
                5
            }
        ];
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(None),
        );
    }

    for selector in [
        special_case::EOL_HOOK,
        special_case::PROTOCOL_HANDLER,
        special_case::SOCKET_LISTENER,
    ] {
        let values = vec![
            0;
            match selector {
                special_case::EOL_HOOK => 3,
                special_case::PROTOCOL_HANDLER => 6,
                special_case::SOCKET_LISTENER => 7,
                _ => unreachable!(),
            }
        ];
        runner.m68k.cpu.core.set_ccr(0x04);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(Some(1)),
        );
        runner.m68k.cpu.core.set_ccr(0);
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(Some(0)),
        );
    }

    runner.m68k.cpu.core.set_d(1, 0xaaaa_5678);
    for selector in [special_case::WIDTH_HOOK, special_case::NWIDTH_HOOK] {
        let values = vec![
            0;
            if selector == special_case::WIDTH_HOOK {
                5
            } else {
                8
            }
        ];
        assert_eq!(
            runner.m68k.complete_m68k_special_case_result(
                &mut ppc_app.memory,
                u8::try_from(selector).unwrap(),
                arguments(&values),
                None,
            ),
            Ok(Some(0x5678)),
        );
    }

    runner.m68k.cpu.core.set_d(0, 0x0001_1111);
    runner.m68k.cpu.core.set_d(1, 0xaaaa_2222);
    runner.m68k.cpu.core.set_d(2, 0xbbbb_0033);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::HIT_TEST_HOOK).unwrap(),
            arguments(&[0, 0, 0, 0, 0, 0, SCRATCH, SCRATCH + 2, SCRATCH + 4]),
            None,
        ),
        Ok(Some(1)),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH), Some(0x1111));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 2), Some(0x2222));
    assert_eq!(ppc_app.memory.read_u8(SCRATCH + 4), Some(0x33));

    runner.m68k.cpu.core.set_d(0, 0xaaaa_4444);
    runner.m68k.cpu.core.set_d(1, 0xbbbb_5555);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_FIND_WORD).unwrap(),
            arguments(&[0, 0, 0, 0, SCRATCH + 8, SCRATCH + 10]),
            None,
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 8), Some(0x4444));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 10), Some(0x5555));

    runner.m68k.cpu.core.set_d(2, 0xaaaa_6666);
    runner.m68k.cpu.core.set_d(3, 0xbbbb_7777);
    runner.m68k.cpu.core.set_d(4, 0xcccc_8888);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_RECALC).unwrap(),
            arguments(&[0, 0, SCRATCH + 12, SCRATCH + 14, SCRATCH + 16]),
            None,
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 12), Some(0x6666));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 14), Some(0x7777));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 16), Some(0x8888));

    runner.m68k.cpu.core.set_a(0, 0xcafe_babe);
    runner.m68k.cpu.core.set_d(0, 0xaaaa_9999);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_DO_TEXT).unwrap(),
            arguments(&[0, 0, 0, 0, SCRATCH + 20, SCRATCH + 24]),
            None,
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u32_be(SCRATCH + 20), Some(0xcafe_babe));
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 24), Some(0x9999));

    ppc_app.memory.write_u16_be(SCRATCH + 28, 1).unwrap();
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::GNE_FILTER_PROC).unwrap(),
            arguments(&[0, SCRATCH + 26]),
            Some(SCRATCH + 28),
        ),
        Ok(None),
    );
    assert_eq!(ppc_app.memory.read_u8(SCRATCH + 26), Some(1));

    // The callback writes multiple output locations as one ABI result.
    // A bad later destination must not leave an earlier output changed.
    ppc_app.memory.write_u16_be(SCRATCH + 30, 0xaaaa).unwrap();
    runner.m68k.cpu.core.set_d(0, 0x1111);
    runner.m68k.cpu.core.set_d(1, 0x2222);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::TE_FIND_WORD).unwrap(),
            arguments(&[0, 0, 0, 0, SCRATCH + 30, 0x0500_0000]),
            None,
        ),
        Err(()),
    );
    assert_eq!(ppc_app.memory.read_u16_be(SCRATCH + 30), Some(0xaaaa));

    runner.m68k.cpu.core.set_d(0, 0xaaaa_abcd);
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::MBAR_HOOK).unwrap(),
            arguments(&[0]),
            None,
        ),
        Ok(Some(0xabcd)),
    );
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            u8::try_from(special_case::HIGH_HOOK).unwrap(),
            arguments(&[]),
            None,
        ),
        Err(()),
    );
    assert_eq!(
        runner.m68k.complete_m68k_special_case_result(
            &mut ppc_app.memory,
            13,
            arguments(&[]),
            None,
        ),
        Err(()),
    );
    runner
        .native
        .restore(native_context)
        .unwrap_or_else(|_| panic!("native context lost its owner"));
}

#[test]
fn void_special_case_completion_preserves_nonzero_native_r3() {
    use crate::guest_call::{M68kResultSource, PowerPcArguments};
    use crate::mixed_mode::special_case;

    const M68K_ENTRY: u32 = 0x0306_1000;
    const INITIAL_SP: u32 = 0x0306_2000;
    const RETURN_PC: u32 = 0x0306_3000;
    const FINAL_SP: u32 = INITIAL_SP + 4;
    const NATIVE_R3: u32 = 0xCAFE_BABE;

    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let ppc_app = native_context.adapter_mut();
    ppc_app.cpu.gpr[3] = NATIVE_R3;
    let arguments = PowerPcArguments::from_slice(&[0, 0]).unwrap();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_powerpc_to_m68k(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::M68k,
                entry: M68K_ENTRY,
                rtoc: 0,
            },
            M68K_ENTRY,
            INITIAL_SP,
            RETURN_PC,
            FINAL_SP,
            crate::guest_call::M68kRegisterState::default(),
            Some(M68kResultSource::SpecialCase {
                selector: u8::try_from(special_case::HIGH_HOOK).unwrap(),
                arguments,
                stack_result: None,
            }),
            PPC_CODE_BASE,
            0,
            PpcNativeReturnGpr3::Preserve,
        ));
    let pending = ppc_app
        .toolbox_startup
        .execution
        .calls()
        .activate_m68k()
        .unwrap();
    runner.m68k.cpu.write_reg(Register::PC, pending.return_pc);
    runner.m68k.cpu.write_reg(Register::A7, pending.final_sp);

    assert!(runner
        .process_context
        .with_memory_and_cfm(|manager, _| runner.m68k.complete_pending(
            &mut ppc_app.memory,
            &mut ppc_app.cpu,
            pending,
            manager
        )));
    assert_eq!(ppc_app.cpu.gpr[3], NATIVE_R3);
    assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
}

#[test]
fn relaunch_with_pending_execution_preserves_the_existing_engine() {
    use crate::execution_kernel::ExecutionTaskState;
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let calls = runner.dispatcher.guest_calls.shared_handle();
    runner.m68k.cpu.write_reg(Register::PC, 0x1234);
    assert!(calls.begin_m68k(
        crate::guest_call::GuestCallTarget {
            isa: crate::guest_procedure::GuestIsa::M68k,
            entry: 0x1000,
            rtoc: 0,
        },
        0x2000,
        0x3000
    ));
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.init_app(&app)));
    assert!(rejected.is_err());
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), 0x1234);
    assert!(!calls.is_empty());
    assert!(calls.complete_m68k(0x2002, 0x3000));
    runner.init_app(&app);
    assert!(runner.m68k.can_relaunch());
    let worker = calls
        .create_native_thread(
            crate::guest_call::NativeThreadContext {
                context: PpcCpu::new().capture_execution_context(),
            },
            crate::guest_call::ThreadStorage {
                result_destination: 0,
                stack_base: 0,
                stack_limit: 0,
                managed_pointer: true,
            },
            true,
            |_| true,
        )
        .unwrap();
    let native_pc = runner.native.application().unwrap().cpu.pc;
    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| runner.init_app(&app)));
    assert!(rejected.is_err());
    assert_eq!(runner.native.application().unwrap().cpu.pc, native_pc);
    assert!(calls.scheduling_state(worker).is_some());
    assert!(calls.set_scheduling_state(worker, ExecutionTaskState::Ready));
    {
        let cpu = &mut runner.native.application_mut().unwrap().cpu;
        assert!(calls.yield_native_thread(cpu, worker.thread_id()).unwrap());
        assert!(calls
            .yield_native_thread(cpu, ExecutionTaskId::APPLICATION.thread_id())
            .unwrap());
        assert!(calls
            .retire_native_thread(worker, cpu, false, |_| true)
            .is_ok());
    }
    assert_eq!(calls.scheduling_state(worker), None);
    assert!(!calls.switch_to_task(worker));
    runner.set_launch_state(41, 1, u64::MAX);
    runner.init_app(&app);
    assert_eq!(
        runner.native.application().unwrap().cpu.time_base(),
        u64::MAX
    );
    {
        let cpu = &mut runner.native.application_mut().unwrap().cpu;
        assert_eq!(
            cpu.step_instruction((31 << 26) | (11 << 21) | (12 << 16) | (8 << 11) | (371 << 1)),
            ppc::PpcStepResult::Stepped
        );
        assert_eq!(cpu.gpr[11], u32::MAX);
        assert_eq!(cpu.time_base(), 0);
    }
    let mut replacement = ppc::PpcExecutionContext::fresh();
    replacement.architectural_mut().gpr[20] = 0xaabb_ccdd;
    let replacement = calls
        .create_native_thread(
            crate::guest_call::NativeThreadContext {
                context: replacement,
            },
            crate::guest_call::ThreadStorage::default(),
            false,
            |_| true,
        )
        .unwrap();
    let cpu = &mut runner.native.application_mut().unwrap().cpu;
    assert!(calls
        .yield_native_thread(cpu, replacement.thread_id())
        .unwrap());
    assert_eq!(cpu.gpr[20], 0xaabb_ccdd);
    assert_eq!(cpu.time_base(), 0);
}

#[test]
fn malformed_m68k_return_shapes_leave_registers_unchanged() {
    use crate::guest_call::{M68kResultTarget, M68kResume, PowerPcReturnState};
    let mut cpu = M68kCpu::new();
    let mut memory = PpcSectionMem::new();
    cpu.core.set_d(0, 0xabcd_1234);
    cpu.core.set_a(0, 0x1234_abcd);
    cpu.core.set_ccr(0x15);
    for target in [
        M68kResultTarget::Data { index: 8, size: 4 },
        M68kResultTarget::Address { index: 8, size: 4 },
        M68kResultTarget::Data { index: 0, size: 3 },
        M68kResultTarget::SpecialCase {
            selector: crate::mixed_mode::special_case::TE_FIND_WORD as u8,
            scratch: u32::MAX,
        },
    ] {
        assert!(!M68kExecution::apply_m68k_resume_result(
            &mut cpu,
            &mut memory,
            M68kResume {
                return_pc: 0x1000,
                final_sp: 0x2000,
                result: Some(target),
                powerpc: PowerPcReturnState { gpr3: 42 },
            }
        ));
        assert_eq!(cpu.core.d(0), 0xabcd_1234);
        assert_eq!(cpu.core.a(0), 0x1234_abcd);
        assert_eq!(cpu.core.get_ccr(), 0x15);
    }
}

#[test]
fn parked_m68k_caller_receives_native_register_and_ccr_results() {
    use crate::guest_call::M68kResultTarget;
    use crate::mixed_mode::special_case;

    const RETURN_PC: u32 = 0x0306_5000;
    const FINAL_SP: u32 = 0x0306_6000;
    for target in [
        M68kResultTarget::Data { index: 2, size: 4 },
        M68kResultTarget::Address { index: 3, size: 4 },
        M68kResultTarget::Ccr { mask: 4 },
        M68kResultTarget::SpecialCase {
            selector: special_case::WIDTH_HOOK as u8,
            scratch: 0,
        },
    ] {
        let app = halted_ppc_app_with_sound(PpcSoundState::default());
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        runner.init_app(&app);
        let mut native_context = runner.native.take(NativeEngineRole::Application).unwrap();
        let mut ppc_app = native_context.adapter_mut();
        assert!(ppc_app
            .toolbox_startup
            .execution
            .calls()
            .begin_m68k_to_powerpc(
                crate::guest_call::GuestCallTarget {
                    isa: crate::guest_procedure::GuestIsa::PowerPc,
                    entry: PPC_CODE_BASE,
                    rtoc: 0,
                },
                crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
                RETURN_PC,
                FINAL_SP,
                Some(target),
            ));
        ppc_app
            .toolbox_startup
            .execution
            .calls()
            .activate_powerpc_from_m68k(&mut ppc_app.cpu, RETURN_PC)
            .unwrap();
        ppc_app.cpu.pc = RETURN_PC;
        ppc_app.cpu.gpr[3] = 0x1234_5678;
        assert!(ppc_app
            .toolbox_startup
            .execution
            .calls()
            .complete_powerpc_for_m68k(&mut ppc_app.cpu));
        let (task, call_id) = ppc_app
            .toolbox_startup
            .execution
            .calls()
            .pending_m68k_resume_owner()
            .unwrap();
        runner.m68k.cpu.core.set_d(1, 0xabcd_0000);
        runner.m68k.cpu.core.set_d(7, 0xcafe_babe);
        runner.m68k.cpu.core.set_ccr(0x11);
        assert!(ppc_app
            .guest_calls()
            .park_context(
                &mut runner
                    .dispatcher
                    .guest_calls
                    .m68k_context_bank()
                    .borrow_mut(),
                task,
                call_id,
                std::mem::take(&mut runner.m68k.cpu),
            )
            .is_ok());
        assert!(runner.resume_m68k_after_powerpc(&mut ppc_app), "{target:?}");
        match target {
            M68kResultTarget::Data { .. } => assert_eq!(runner.m68k.cpu.core.d(2), 0x1234_5678),
            M68kResultTarget::Address { .. } => {
                assert_eq!(runner.m68k.cpu.core.a(3), 0x1234_5678)
            }
            M68kResultTarget::Ccr { .. } => assert_eq!(runner.m68k.cpu.core.get_ccr(), 0x15),
            M68kResultTarget::SpecialCase { .. } => {
                assert_eq!(runner.m68k.cpu.core.d(1), 0xabcd_5678)
            }
            _ => unreachable!(),
        }
        assert_eq!(runner.m68k.cpu.core.d(7), 0xcafe_babe);
        assert_eq!(runner.m68k.cpu.read_reg(Register::PC), RETURN_PC);
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), FINAL_SP);
        assert!(ppc_app.toolbox_startup.execution.calls().is_empty());
        assert!(runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .is_empty());
    }
}

#[test]
fn failed_m68k_result_application_keeps_resume_and_parked_context_retryable() {
    use crate::guest_call::M68kResultTarget;

    const RESULT: u32 = 0x0306_4000;
    const RETURN_PC: u32 = 0x0306_5000;
    const FINAL_SP: u32 = 0x0306_6000;
    const RESULT_VALUE: u32 = 0x1234_5678;
    const PARKED_D0: u32 = 0xA11C_E001;

    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let mut native_context = runner
        .native
        .take(NativeEngineRole::Application)
        .expect("PPC app");
    let mut ppc_app = native_context.adapter_mut();
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .begin_m68k_to_powerpc(
            crate::guest_call::GuestCallTarget {
                isa: crate::guest_procedure::GuestIsa::PowerPc,
                entry: PPC_CODE_BASE,
                rtoc: 0,
            },
            crate::guest_call::PowerPcArguments::from_slice(&[]).unwrap(),
            RETURN_PC,
            FINAL_SP,
            Some(M68kResultTarget::Memory {
                address: RESULT,
                size: 4,
            }),
        ));
    ppc_app
        .toolbox_startup
        .execution
        .calls()
        .activate_powerpc_from_m68k(&mut ppc_app.cpu, RETURN_PC)
        .unwrap();
    ppc_app.cpu.pc = RETURN_PC;
    ppc_app.cpu.gpr[3] = RESULT_VALUE;
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .complete_powerpc_for_m68k(&mut ppc_app.cpu));

    // Model the caller parked by a nested native-to-68K transition. The
    // failed write must not consume either this context or its completion.
    let (task, call_id) = ppc_app
        .guest_calls()
        .pending_m68k_resume_owner()
        .expect("completed continuation owner");
    runner.m68k.cpu.core.set_d(0, PARKED_D0);
    assert!(ppc_app
        .guest_calls()
        .park_context(
            &mut runner
                .dispatcher
                .guest_calls
                .m68k_context_bank()
                .borrow_mut(),
            task,
            call_id,
            std::mem::take(&mut runner.m68k.cpu),
        )
        .is_ok());
    assert_eq!(
        runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .task_len(task),
        1
    );
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_some());

    assert!(!runner.resume_m68k_after_powerpc(&mut ppc_app));
    assert_eq!(
        runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .task_len(task),
        1
    );
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_some());

    ppc_app.memory.add_readonly_region(RESULT, vec![0xaa; 4]);
    assert!(!runner.resume_m68k_after_powerpc(&mut ppc_app));
    assert_eq!(ppc_app.memory.read_u32_be(RESULT), Some(0xaaaa_aaaa));
    assert_eq!(
        runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .task_len(task),
        1
    );
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_some());

    ppc_app.memory.add_region(RESULT, vec![0; 4]);
    assert!(runner.resume_m68k_after_powerpc(&mut ppc_app));
    assert_eq!(
        runner
            .dispatcher
            .guest_calls
            .m68k_context_bank()
            .borrow()
            .task_len(task),
        0
    );
    assert!(ppc_app
        .toolbox_startup
        .execution
        .calls()
        .peek_m68k_resume()
        .is_none());
    assert_eq!(ppc_app.memory.read_u32_be(RESULT), Some(RESULT_VALUE));
    assert_eq!(runner.m68k.cpu.core.d(0), PARKED_D0);
}

#[test]
fn ppc_exit_to_shell_stops_before_tick_and_callback_phase() {
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    let mut sound = PpcSoundState::default();
    queue_ppc_sound_completion(&mut sound, 0x0300_1000, CALLBACK);
    let mut app = halted_ppc_app_with_sound(sound);
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app
        .memory
        .add_region(CALLBACK, 0x4e80_0020u32.to_be_bytes().to_vec());
    ppc_app
        .memory
        .write_u32_be(
            PPC_CODE_BASE,
            ppc_test_relative_branch(PPC_CODE_BASE, PPC_IMPORT_TRAP_BASE),
        )
        .unwrap();
    let mut exit = test_ppc_import_binding(0, "InterfaceLib", "ExitToShell");
    exit.trap_pc = PPC_IMPORT_TRAP_BASE;
    exit.dispatcher_target = PpcImportDispatcherTarget::ExitToShell;
    ppc_app.import_count = 1;
    ppc_app.imports = vec![exit];
    ppc_app.vbl_tasks.push(PpcVblTaskRecord {
        task_ptr: PPC_DATA_BASE + 0x3000,
        architecture: CallbackTaskArchitecture::PowerPc,
        slot: None,
        pending: false,
    });
    ppc_app.timer_tasks.push(PpcTimerTaskRecord {
        task_ptr: PPC_DATA_BASE + 0x3100,
        architecture: CallbackTaskArchitecture::PowerPc,
        extended: false,
        callback: CALLBACK,
        active: true,
        fire_at_tick: 0,
        fire_at_subtick: 0,
        last_fired_tick: None,
    });

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.set_instructions_per_tick(1_000_000);
    runner.tick_budget = 1_000_000;
    let initial_tick = runner.guest_tick();
    let initial_screen_events = runner.dispatcher.screen_event_count;

    let (_steps, running) = runner.run_steps(64, None);

    assert!(!running);
    assert!(runner.halted_by_exit_to_shell());
    assert_eq!(runner.guest_tick(), initial_tick);
    assert_eq!(runner.guest_tick(), initial_tick);
    assert_eq!(runner.dispatcher.screen_event_count, initial_screen_events);
    let ppc_app = runner.native.application().expect("PPC app retained");
    assert_eq!(ppc_app.vbl_tasks.len(), 1);
    assert_eq!(ppc_app.timer_tasks[0].last_fired_tick, None);
    assert_eq!(ppc_app.sound.manager.pending_sound_callbacks.len(), 1);
    assert!(ppc_app.sound.completion_invocations.is_empty());
}

const PPC_SOUND_EVENT_RECORD: u32 = PPC_DATA_BASE + 0x2000;
const PPC_SOUND_GET_EVENT_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
const PPC_SOUND_POST_EVENT_CALLBACK: u32 = PPC_CODE_BASE + 0x1100;
const PPC_SOUND_SET_EVENT_MASK_CALLBACK: u32 = PPC_CODE_BASE + 0x1200;

pub(super) fn ppc_test_relative_branch(from: u32, to: u32) -> u32 {
    0x4800_0000 | (to.wrapping_sub(from) & 0x03ff_fffc)
}

fn install_ppc_sound_event_boundary_fixture(app: &mut LoadedApp) {
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    let mut add_callback = |address: u32, mut words: Vec<u32>, import_index: u32| {
        let branch_address = address + u32::try_from(words.len()).unwrap() * 4;
        words.push(ppc_test_relative_branch(
            branch_address,
            PPC_IMPORT_TRAP_BASE + import_index * 4,
        ));
        ppc_app.memory.add_region(
            address,
            words.into_iter().flat_map(u32::to_be_bytes).collect(),
        );
    };
    add_callback(
        PPC_SOUND_GET_EVENT_CALLBACK,
        vec![
            0x3860_ffff, // li r3,-1
            0x3c80_0200, // lis r4,$0200
            0x6084_2000, // ori r4,r4,$2000
        ],
        0,
    );
    add_callback(
        PPC_SOUND_POST_EVENT_CALLBACK,
        vec![
            0x3860_0005, // li r3,5
            0x3c80_5566, // lis r4,$5566
            0x6084_7788, // ori r4,r4,$7788
        ],
        1,
    );
    add_callback(
        PPC_SOUND_SET_EVENT_MASK_CALLBACK,
        vec![0x3860_1234], // li r3,$1234
        2,
    );
    ppc_app
        .memory
        .add_region(PPC_SOUND_EVENT_RECORD, vec![0; 16]);
    ppc_app.import_count = 3;
    ppc_app.imports = [
        (
            "GetNextEvent",
            PpcImportDispatcherTarget::GetNextEvent(PpcEventPollOperation::GetNextEvent),
        ),
        ("PostEvent", PpcImportDispatcherTarget::PostEvent),
        ("SetEventMask", PpcImportDispatcherTarget::SetEventMask),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (symbol, dispatcher_target))| {
        let index = u32::try_from(index).unwrap();
        PpcImportBinding {
            library_index: 0,
            symbol_index: index,
            library_name: "InterfaceLib".into(),
            symbol_name: symbol.into(),
            class: 0,
            weak: false,
            address: PPC_IMPORT_TRAP_BASE + index * 4,
            tvector_address: None,
            trap_pc: PPC_IMPORT_TRAP_BASE + index * 4,
            dispatcher_target,
        }
    })
    .collect();
    ppc_app.gworlds.push(PpcGWorldRecord {
        ui_theme: crate::ui_theme::UiThemeId::ClassicSystem7,
        port: PPC_MAIN_GWORLD,
        pixmap_handle: 0,
        pixmap: 0,
        base_addr: 0,
        gdevice: PPC_MAIN_GDEVICE,
        width: 512,
        height: 342,
        depth: 8,
        row_bytes: 512,
        pixels_locked: false,
        pixels_no_purge: false,
    });
    ppc_app.set_input_snapshot(PpcInputSnapshot {
        mouse_v: 17,
        mouse_h: 19,
        ..PpcInputSnapshot::default()
    });
}

fn assert_ppc_sound_event_boundary(
    mut app: LoadedApp,
    fire_callbacks: impl FnOnce(&mut FixtureRunner),
) {
    install_ppc_sound_event_boundary_fixture(&mut app);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let (offset_v, offset_h) = runner.ppc_viewport_offset();
    assert!(
        offset_v > 0 && offset_h > 0,
        "fixture must use a centered viewport"
    );
    runner
        .process_context
        .shared_event_queue()
        .push_back(QueuedEvent {
            what: 3,
            message: 0x1122_3344,
            when: 0,
            where_v: 120,
            where_h: 180,
            modifiers: 0x0080,
        });

    fire_callbacks(&mut runner);

    let ppc_app = runner.native.application_mut().expect("PPC app");
    assert_eq!(
        ppc_app.memory.read_u16_be(PPC_SOUND_EVENT_RECORD + 10),
        Some(120)
    );
    assert_eq!(
        ppc_app.memory.read_u16_be(PPC_SOUND_EVENT_RECORD + 12),
        Some(180)
    );
    assert_eq!(runner.process_context.event_queue().len(), 1);
    let posted = runner.process_context.event_queue().get(0).unwrap();
    assert_eq!((posted.what, posted.message), (5, 0x5566_7788));
    assert_eq!((posted.where_v, posted.where_h), (17, 19));
    assert_eq!(
        runner
            .bus
            .read_word(crate::memory::globals::addr::SYS_EVT_MASK),
        0x1234
    );
    assert_eq!(
        ppc_app
            .memory
            .read_u16_be(crate::memory::globals::addr::SYS_EVT_MASK),
        Some(0x1234)
    );
}

#[test]
fn ppc_sound_doublebacks_preserve_centered_viewport_event_coordinates() {
    let callback = |callback| PpcSoundDoubleBackRecord {
        architecture: CallbackTaskArchitecture::PowerPc,
        channel: 0x0300_1000,
        header: 0x0300_2000,
        exhausted_buffer: 0x0300_3000,
        exhausted_buffer_index: 0,
        callback,
        tick: 0,
        instruction_count: 0,
    };
    let sound = PpcSoundState::default();
    sound.manager.replace_pending_process_doublebacks(vec![
        callback(PPC_SOUND_GET_EVENT_CALLBACK),
        callback(PPC_SOUND_POST_EVENT_CALLBACK),
        callback(PPC_SOUND_SET_EVENT_MASK_CALLBACK),
    ]);
    let app = halted_ppc_app_with_sound(sound);

    assert_ppc_sound_event_boundary(app, FixtureRunner::fire_pending_ppc_sound_doublebacks);
}

#[test]
fn ppc_sound_completions_preserve_centered_viewport_event_coordinates() {
    let mut sound = PpcSoundState::default();
    queue_ppc_sound_completion(&mut sound, 0x0300_1000, PPC_SOUND_GET_EVENT_CALLBACK);
    queue_ppc_sound_completion(&mut sound, 0x0300_1000, PPC_SOUND_POST_EVENT_CALLBACK);
    queue_ppc_sound_completion(&mut sound, 0x0300_1000, PPC_SOUND_SET_EVENT_MASK_CALLBACK);
    let app = halted_ppc_app_with_sound(sound);

    assert_ppc_sound_event_boundary(app, FixtureRunner::fire_pending_ppc_sound_completions);
}

#[test]
fn ppc_host_mouse_input_enters_the_shared_queue_in_global_coordinates() {
    use crate::memory::globals::addr;

    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    install_ppc_sound_event_boundary_fixture(&mut app);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    let (offset_v, offset_h) = runner.ppc_viewport_offset();
    assert!(offset_v > 0 && offset_h > 0);

    runner.push_mouse_down(offset_v.saturating_add(12), offset_h.saturating_add(34));

    let event = runner
        .process_context
        .event_queue()
        .back()
        .expect("mouseDown");
    assert_eq!((event.where_v, event.where_h), (12, 34));
    let ppc_app = runner.native.application().expect("PPC app");
    let native_event = ppc_app.event_queue.back().expect("shared mouseDown");
    assert_eq!((native_event.where_v, native_event.where_h), (12, 34));
    assert_eq!(runner.bus.read_word(addr::M_TEMP), 12);
    assert_eq!(runner.bus.read_word(addr::M_TEMP + 2), 34);
}

#[test]
fn ppc_event_queue_has_immediate_bidirectional_visibility() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    // Host input pushes directly into process context canonical queue
    runner.push_mouse_down(10, 20);
    assert_eq!(runner.process_context.event_queue().len(), 1);
    assert_eq!(
        (
            runner
                .process_context
                .event_queue()
                .front()
                .unwrap()
                .where_v,
            runner
                .process_context
                .event_queue()
                .front()
                .unwrap()
                .where_h
        ),
        (10, 20)
    );

    // The attached PPC adapter observes and mutates the canonical queue directly.
    {
        let app = runner.native.application_mut().unwrap();
        assert_eq!(app.event_queue.len(), 1);
        let event = app.event_queue.pop_front().unwrap();
        assert_eq!((event.where_v, event.where_h), (10, 20));
        app.event_queue.push_back(QueuedEvent {
            what: 2, // mouseUp
            message: 0,
            when: 0,
            where_v: 30,
            where_h: 40,
            modifiers: 0,
        });
    }

    // The mutation is immediately visible in ProcessContext without a sync copy.
    assert_eq!(runner.process_context.event_queue().len(), 1);
    assert_eq!(
        runner.process_context.event_queue().front().unwrap().what,
        2
    );

    // The attached 68K dispatcher observes the same queue continuously.
    assert_eq!(runner.dispatcher.event_queue.len(), 1);
    let event = runner.dispatcher.event_queue.pop_front().unwrap();
    assert_eq!((event.where_v, event.where_h), (30, 40));

    assert!(runner.process_context.event_queue().is_empty());
}

#[test]
fn ppc_sound_doubleback_can_complete_a_large_refill() {
    const CALLBACK_CYCLES: usize = 600_000;
    const CHANNEL: u32 = 0x0300_1000;
    const HEADER: u32 = 0x0300_2000;
    const BUFFER: u32 = 0x0300_3000;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;

    let sound = PpcSoundState::default();
    sound
        .manager
        .replace_pending_process_doublebacks(vec![PpcSoundDoubleBackRecord {
            architecture: CallbackTaskArchitecture::PowerPc,
            channel: CHANNEL,
            header: HEADER,
            exhausted_buffer: BUFFER,
            exhausted_buffer_index: 0,
            callback: CALLBACK,
            tick: 1,
            instruction_count: 1,
        }]);
    let mut app = halted_ppc_app_with_sound(sound);
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    let mut callback = Vec::with_capacity((CALLBACK_CYCLES + 1) * 4);
    for _ in 0..CALLBACK_CYCLES {
        callback.extend_from_slice(&0x6000_0000u32.to_be_bytes()); // nop
    }
    callback.extend_from_slice(&0x4e80_0020u32.to_be_bytes()); // blr
    ppc_app.memory.add_region(CALLBACK, callback);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.fire_pending_ppc_sound_doublebacks();

    assert!(!runner.is_halted());
    let sound = &runner.native.application().expect("PPC app").sound;
    assert!(sound.manager.pending_process_doublebacks.is_empty());
    let invocation = sound
        .completion_invocations
        .last()
        .expect("doubleback invocation");
    assert!(invocation.cycles > 250_000);
    assert_eq!(
        invocation.result,
        PpcRunResult::Halted {
            pc: PPC_HALT_PC,
            cycles: (CALLBACK_CYCLES + 1) as u64,
        }
    );
}

#[test]
fn ppc_sound_doubleback_runaway_still_stops_at_the_watchdog() {
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    let sound = PpcSoundState::default();
    sound
        .manager
        .replace_pending_process_doublebacks(vec![PpcSoundDoubleBackRecord {
            architecture: CallbackTaskArchitecture::PowerPc,
            channel: 0x0300_1000,
            header: 0x0300_2000,
            exhausted_buffer: 0x0300_3000,
            exhausted_buffer_index: 0,
            callback: CALLBACK,
            tick: 1,
            instruction_count: 1,
        }]);
    let mut app = halted_ppc_app_with_sound(sound);
    app.ppc.as_mut().unwrap().memory.add_region(
        CALLBACK,
        0x4800_0000u32.to_be_bytes().to_vec(), // b .
    );
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.fire_pending_ppc_sound_doublebacks();

    assert!(runner.is_halted());
    assert_eq!(runner.halted_pc(), Some(CALLBACK));
    let invocation = runner
        .native
        .application()
        .unwrap()
        .sound
        .completion_invocations
        .last()
        .unwrap();
    assert!(matches!(invocation.result, PpcRunResult::CycleLimit { cycles } if cycles > 0));
}

#[test]
fn ppc_sound_doubleback_callback_refills_and_plays_exhausted_buffer() {
    const CHANNEL: u32 = 0x0300_1000;
    const HEADER: u32 = 0x0300_2000;
    const BUFFER: u32 = 0x0300_3000;
    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const SAMPLES: [u8; 4] = [0x80, 0x90, 0x70, 0xa0];

    let sound = PpcSoundState::default();
    sound
        .manager
        .replace_double_buffer_playbacks(vec![PpcSoundDoubleBufferPlaybackRecord {
            channel: CHANNEL,
            header: HEADER,
            buffers: [BUFFER, 0],
            callback: CALLBACK,
            callback_architecture: CallbackTaskArchitecture::PowerPc,
            sample_rate_fixed: crate::sound::OUTPUT_RATE << 16,
            num_channels: 1,
            sample_size: 8,
            compression_id: 0,
            packet_size: 0,
            current_buffer_index: 0,
            callback_pending_mask: 0,
            active: true,
            host_initialized: false,
            host_buffer_loaded: false,
        }]);
    let mut app = halted_ppc_app_with_sound(sound);
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app.memory.add_region(BUFFER, vec![0; 32]);

    // Inside Macintosh: Sound (1994), pp. 2-147 and 2-178: a doubleback
    // receives the exhausted SndDoubleBufferPtr and refills dbNumFrames,
    // dbFlags, and dbSoundData before setting dbBufferReady.
    let callback = [
        0x38a0_0004u32, // li r5,4
        0x90a4_0000,    // stw r5,0(r4): dbNumFrames
        0x3ca0_8090,    // lis r5,$8090
        0x60a5_70a0,    // ori r5,r5,$70A0
        0x90a4_0010,    // stw r5,16(r4): dbSoundData
        0x38a0_0005,    // li r5,5: dbBufferReady | dbLastBuffer
        0x90a4_0004,    // stw r5,4(r4): dbFlags
        0x4e80_0020,    // blr
    ]
    .into_iter()
    .flat_map(u32::to_be_bytes)
    .collect::<Vec<_>>();
    ppc_app.memory.add_region(CALLBACK, callback);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);
    runner.mix_audio(SAMPLES.len());

    assert_eq!(runner.drain_audio(), SAMPLES);
    let ppc_app = runner
        .native
        .application()
        .expect("PPC app should stay loaded");
    let mut memory = ppc_app.memory.clone();
    assert_eq!(memory.read_u32_be(BUFFER), Some(SAMPLES.len() as u32));
    assert_eq!(memory.read_u32_be(BUFFER + 4), Some(0x04));
    assert_eq!(
        memory.read_u32_be(BUFFER + 16),
        Some(u32::from_be_bytes(SAMPLES))
    );
    assert_eq!(ppc_app.sound.completion_invocations.len(), 1);
    assert!(!ppc_app.sound.manager.double_buffer_playbacks[0].active);
    assert_eq!(
        runner.dispatcher().sound_manager.debug_samples_mixed,
        SAMPLES.len() as u64
    );
}

#[test]
fn ppc_sound_completion_preserves_callback_rnd_seed_update() {
    use crate::memory::globals::addr;

    const CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    let mut sound = PpcSoundState::default();
    queue_ppc_sound_completion(&mut sound, 0x0300_1000, CALLBACK);
    let mut app = halted_ppc_app_with_sound(sound);
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    ppc_app.memory.add_region(
        CALLBACK,
        [
            0x3c60_89abu32, // lis r3,$89ab
            0x6063_cdef,    // ori r3,r3,$cdef
            0x3880_0156,    // li r4,$0156
            0x9064_0000,    // stw r3,0(r4)
            0x4e80_0020,    // blr
        ]
        .into_iter()
        .flat_map(u32::to_be_bytes)
        .collect(),
    );

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_optional_launch_state(None, Some(1), None);
    runner.init_app(&app);
    runner.fire_pending_ppc_sound_completions();

    assert_eq!(runner.bus.read_long(addr::RND_SEED), 0x89ab_cdef);
    assert_eq!(
        runner
            .native
            .application_mut()
            .expect("PPC app")
            .memory
            .read_u32_be(addr::RND_SEED),
        Some(0x89ab_cdef)
    );
}

#[test]
fn ppc_sound_completions_see_ticks_advanced_by_prior_callback() {
    use crate::memory::globals::addr;

    const FIRST_CALLBACK: u32 = PPC_CODE_BASE + 0x1000;
    const SECOND_CALLBACK: u32 = PPC_CODE_BASE + 0x1100;
    let mut sound = PpcSoundState::default();
    queue_ppc_sound_completion(&mut sound, 0x0300_1000, FIRST_CALLBACK);
    queue_ppc_sound_completion(&mut sound, 0x0300_1000, SECOND_CALLBACK);
    let mut app = halted_ppc_app_with_sound(sound);
    let ppc_app = app.ppc.as_mut().expect("PPC app");
    ppc_app.cpu.alignment_policy = ppc::PpcAlignmentPolicy::EmulateData;
    ppc_app.memory.add_region(PPC_HALT_PC, vec![0; 64 * 1024]);
    ppc_app.memory.add_region(
        FIRST_CALLBACK,
        [0x6000_0000u32, 0x4e80_0020]
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );
    ppc_app.memory.add_region(
        SECOND_CALLBACK,
        [0x8060_016au32, 0x4e80_0020] // lwz r3,$016a(0); blr
            .into_iter()
            .flat_map(u32::to_be_bytes)
            .collect(),
    );

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_optional_launch_state(Some(41), None, None);
    runner.init_app(&app);
    runner.set_instructions_per_tick(2);
    runner.fire_pending_ppc_sound_completions();

    assert_eq!(runner.bus.read_long(addr::TICKS), 43);
    assert_eq!(runner.guest_tick(), 43);
    let ppc_app = runner.native.application_mut().expect("PPC app");
    assert_eq!(
        ppc_app
            .memory
            .read_u32_be(crate::memory::globals::addr::TICKS),
        Some(43)
    );
    assert_eq!(ppc_app.memory.read_u32_be(addr::TICKS), Some(43));
    assert_eq!(ppc_app.sound.completion_invocations.len(), 2);
    assert_eq!(ppc_app.sound.completion_invocations[1].end_r3, 42);
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
        process_file_system: SharedProcessFileSystem::from_state(
            ProcessFileSystemState {
                files: Default::default(),
                stdio_streams: ppc_initial_stdio_streams(),
                vfs_volumes: crate::process_context::SharedProcessValue::default(),
                vfs_directories: crate::process_context::SharedProcessValue::from_value(vec![
                    PpcVfsDirectory {
                        dir_id: 18,
                        parent_dir_id: 17,
                        path: "System Folder/Preferences/Test App Saves".to_string(),
                        creator: u32::from_be_bytes(*b"Nano"),
                        file_type: u32::from_be_bytes(*b"dir "),
                        finder_flags: 0x0080,
                        dirty: true,
                    },
                ]),
                next_vfs_dir_id: crate::process_context::SharedProcessValue::from_value(18),
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
        .write_u16_be(crate::memory::globals::addr::SYS_EVT_MASK, 0xffdf)
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
fn cursor_state_is_immediately_shared_between_cpu_adapters() {
    let app = halted_ppc_app_with_sound(PpcSoundState::default());
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_app(&app);

    let mut data = [0; 32];
    data[0] = 0x80;
    let mut mask = [0; 32];
    mask[0] = 0xc0;
    let ppc_app = runner.native.application_mut().expect("PPC app installed");
    ppc_app
        .cursor_state
        .install(crate::display::CursorImage::mono(data, mask, 3, 4));
    ppc_app.cursor_state.hide();

    assert_eq!(runner.dispatcher.cursor_level(), -1);
    assert!(!runner.dispatcher.cursor_visible());
    assert_eq!(runner.dispatcher.cursor_data(), Some((data, mask, 3, 4)));

    runner.dispatcher.cursor_state.show();
    assert_eq!(
        runner
            .native
            .application()
            .expect("PPC app installed")
            .cursor_level(),
        0
    );
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
    use crate::memory::globals::addr;

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
    use crate::memory::globals::addr;

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

#[test]
fn init_app_zeroes_fresh_top_of_stack() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    let stack_seed_start = app.initial_sp.saturating_sub(0x8000);
    assert_eq!(
        runner.bus.read_long(stack_seed_start),
        0,
        "fresh process stack should match a newly initialized application partition"
    );
}

#[test]
fn init_app_seeds_appparmhandle_with_empty_finder_information() {
    use crate::memory::globals::addr;

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher_mut()
        .set_launched_app_path("Games/Armor Alley");
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    let handle = runner.bus.read_long(addr::APP_PARM_HANDLE);
    assert_ne!(
        handle, 0,
        "AppParmHandle should point at Finder launch information"
    );
    let data_ptr = runner.bus.read_long(handle);
    assert_ne!(
        data_ptr, 0,
        "Finder launch information handle should be loaded"
    );
    assert_eq!(
        runner.bus.get_alloc_size(data_ptr),
        Some(4),
        "empty Finder launch information is message/count only"
    );
    assert_eq!(
        runner.bus.read_word(data_ptr),
        0,
        "message should be appOpen for a normal application launch"
    );
    assert_eq!(
        runner.bus.read_word(data_ptr + 2),
        0,
        "normal application launch has no selected documents"
    );
}

#[test]
fn init_app_seeds_current_application_fcb_low_memory_state() {
    use crate::memory::globals::addr;

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .dispatcher_mut()
        .vfs_rsrc
        .insert("Games/Armor Alley".to_string(), vec![0xA5; 1234]);
    runner
        .dispatcher_mut()
        .set_vfs_entry_metadata("Games/Armor Alley", *b"APPL", *b"TEST", 0);
    runner
        .dispatcher_mut()
        .set_launched_app_path("Games/Armor Alley");
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_word(addr::CUR_APREF_NUM),
        APPLICATION_RESOURCE_REFNUM,
        "CurApRefNum should be the app resource fork access path"
    );
    assert_eq!(
        runner.bus.read_word(addr::FS_FCB_LEN),
        HFS_FCB_SIZE,
        "System 7 FCB size should be exposed for direct low-memory readers"
    );
    let fcb_buffer = runner.bus.read_long(addr::FCB_S_PTR);
    assert_ne!(fcb_buffer, 0, "FCBSPtr should point to an FCB buffer");
    assert_eq!(
        runner.bus.read_word(fcb_buffer),
        HFS_FCB_BUFFER_SIZE,
        "FCB buffer length should include the leading length word"
    );
    let fcb = fcb_buffer + APPLICATION_RESOURCE_REFNUM as u32;
    assert_eq!(
        runner.bus.read_word(fcb + 4),
        0x0200,
        "the application access path should describe a resource fork"
    );
    assert_eq!(runner.bus.read_long(fcb + 8), 1234);
    assert_eq!(runner.bus.read_long(fcb + 12), 1234);
    assert_eq!(runner.bus.read_long(fcb + 50), u32::from_be_bytes(*b"APPL"));
    assert_eq!(
        runner.bus.read_long(fcb + 58),
        *runner.dispatcher.default_dir_id
    );

    let vcb = runner.bus.read_long(fcb + 20);
    assert_ne!(vcb, 0, "fcbVPtr should point to the boot volume VCB");
    assert_eq!(runner.bus.read_long(addr::DEF_VCB_PTR), vcb);
    assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 2), vcb);
    assert_eq!(runner.bus.read_long(addr::VCB_Q_HDR + 6), vcb);
    assert_eq!(runner.bus.read_word(vcb + 8), 0x4244);
    assert_eq!(
        runner.bus.read_word(vcb + 78) as i16,
        crate::trap::dispatch::BOOT_VOLUME_REF_NUM
    );
    assert_eq!(
        runner
            .dispatcher
            .open_files
            .get(&APPLICATION_RESOURCE_REFNUM),
        Some(&"__rsrc__Games/Armor Alley".to_string())
    );
}

#[test]
fn init_app_sets_legacy_sound_driver_low_memory_defaults() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::SD_VOLUME),
        1,
        "SdVolume ($0260) should boot to the nonzero legacy compatibility value"
    );
    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::SOUND_LEVEL),
        0,
        "SoundLevel ($027F) is a distinct Sound Driver amplitude byte"
    );
    let sound_base = runner
        .bus
        .read_long(crate::memory::globals::addr::SOUND_BASE);
    assert_eq!(
            sound_base, 0x007F_7880,
            "SoundBase ($0266) should point at the 370-word legacy sound buffer in reserved display memory"
        );
    assert_eq!(
        runner.bus.read_byte(sound_base),
        0x80,
        "legacy SoundBase buffer starts at neutral amplitude"
    );
}

#[test]
fn init_app_materializes_the_device_manager_unit_table() {
    use crate::memory::globals::{addr, DEFAULT_UNIT_TABLE_ENTRY_COUNT};

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    let table = runner.bus.read_long(addr::U_TABLE_BASE);
    assert_ne!(table, 0, "UTableBase should address the unit table");
    assert_eq!(
        runner.bus.read_word(addr::UNIT_NTRY_CNT),
        DEFAULT_UNIT_TABLE_ENTRY_COUNT
    );
    assert_eq!(
        runner.bus.get_alloc_size(table),
        Some(u32::from(DEFAULT_UNIT_TABLE_ENTRY_COUNT) * 4)
    );
    assert!(
        runner
            .bus
            .read_bytes(table, usize::from(DEFAULT_UNIT_TABLE_ENTRY_COUNT) * 4)
            .iter()
            .all(|&byte| byte == 0),
        "the unit table should start with nil DCE handles"
    );
}

#[test]
fn init_app_seeds_mmu32bit_low_memory_flag() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::MMU32_BIT),
        1,
        "MMU32Bit ($0CB2) should mirror Systemless's default 32-bit addressing mode"
    );
}

#[test]
fn init_app_can_start_in_twenty_four_bit_addressing_mode() {
    let mut runner = FixtureRunner::new(
        8 * 1024 * 1024,
        FixtureRunnerConfig {
            addressing_32_bit: false,
            ..FixtureRunnerConfig::default()
        },
    );
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    assert!(!runner.bus.addressing_32_bit());
    assert_eq!(runner.bus.ram_size(), 8 * 1024 * 1024);
    assert_eq!(runner.dispatcher.mmu_mode, 0);
    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::MMU32_BIT),
        0
    );
}

#[test]
fn twenty_four_bit_runner_caps_guest_ram_at_sixteen_megabytes() {
    let runner = FixtureRunner::new(
        64 * 1024 * 1024,
        FixtureRunnerConfig {
            addressing_32_bit: false,
            ..FixtureRunnerConfig::default()
        },
    );

    assert_eq!(runner.bus.ram_size(), 0x0100_0000);
}

#[test]
fn init_app_seeds_callable_swap_mmu_mode_trap_table_entry() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let entry = runner
        .bus
        .read_long(crate::memory::globals::addr::SWAP_MMU_MODE_TRAP);
    assert_ne!(entry, 0);
    assert_eq!(
        [runner.bus.read_word(entry), runner.bus.read_word(entry + 2),],
        [0xA05D, 0x4E75],
        "the $0574 OS trap-table entry should target SwapMMUMode followed by RTS"
    );

    let call_site = 0x0002_0000u32;
    let initial_sp = 0x007F_FE00u32;
    runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0574).W,A0
    runner.bus.write_word(call_site + 2, 0x0574);
    runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
    runner.bus.write_word(call_site + 6, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.m68k.cpu.write_reg(Register::D0, 0);

    let (steps, running) = runner.run_steps(4, None);

    assert!(running);
    assert_eq!(steps, 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::D0),
        1,
        "SwapMMUMode should return the previous 32-bit mode in D0"
    );
    assert_eq!(
        runner
            .bus
            .read_byte(crate::memory::globals::addr::MMU32_BIT),
        0,
        "the indirect call should update the requested addressing mode"
    );
    runner.bus.write_long(0x0002_1000, 0x1234_5678);
    assert_eq!(
        runner.bus.read_long(0xAB02_1000),
        0x1234_5678,
        "SwapMMUMode must change actual guest address translation"
    );
}

#[test]
fn swap_mmu_mode_returns_on_classic_stack_with_more_than_sixteen_megabytes() {
    let code0 = minimal_code0(0, 0x2000, 0, 0);
    let fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &code0)]);
    let fork = ResourceFork::parse(&fork_bytes).expect("parse synthetic app fork");
    let mut runner = FixtureRunner::new(64 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = runner.load_app(&fork).expect("load app");
    runner.init_app(&app);

    let call_site = 0x0002_0000;
    let return_pc = call_site + 8;
    let stack = app.initial_sp - 0x200;
    runner.bus.write_word(call_site, 0xA05D); // SwapMMUMode
    runner.bus.write_word(call_site + 2, 0x4E75); // RTS
    runner.bus.write_long(stack, return_pc);
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, stack);
    runner.m68k.cpu.write_reg(Register::D0, 0);

    let (steps, running) = runner.run_steps(2, None);

    assert!(running);
    assert_eq!(steps, 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 4);
    assert!(!runner.bus.addressing_32_bit());

    // PenMode has a permanent system-owned come-from head. Its raw
    // synthetic identity remains protected provenance while 24-bit guest
    // accesses are masked, so inline dispatch must not mistake the head
    // for an application-installed native patch.
    let pen_mode_site = call_site + 0x10;
    runner.bus.write_word(pen_mode_site, 0xA89C);
    runner.bus.write_word(stack + 4, 8);
    runner.m68k.cpu.write_reg(Register::PC, pen_mode_site);

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), pen_mode_site + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), stack + 6);
}

#[test]
fn init_app_seeds_callable_profile_come_from_head() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    let entry = crate::trap::dispatch::OS_TRAP_TABLE_BASE + 0x78 * 4;
    let previous_head = runner.bus.read_long(entry);
    runner
        .dispatcher
        .install_trap_address(&mut runner.bus, 0xA078, 0x0021_1000)
        .unwrap();
    runner.bus.write_long(0x28, 0x0021_2000);
    runner.init_app(&app);
    assert_eq!(
        runner.dispatcher.trap_table_profile,
        Some(TrapTableProfile::M68k68040)
    );
    assert_ne!(runner.bus.read_long(entry), previous_head);
    assert_ne!(
        runner.dispatcher.trap_table_address(&runner.bus, 0xA078),
        Some(0x0021_1000)
    );
    assert!(runner.dispatcher.aline_vector_is_default(&runner.bus));

    let head = runner
        .bus
        .read_long(crate::trap::dispatch::OS_TRAP_TABLE_BASE + 0x78 * 4);
    let successor = runner.bus.read_long(head + 4);
    assert_eq!(runner.bus.read_long(head), 0x6006_4EF9);
    runner.m68k.cpu.write_reg(Register::PC, head);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), successor);
}

#[test]
fn profile_unimplemented_gateway_executes_system_error_12() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let gateway = runner
        .dispatcher
        .trap_table_address(&runner.bus, 0xAA6E)
        .unwrap();
    assert_eq!(runner.bus.read_word(gateway), 0xAE6E);
    let call_site = 0x0002_0000;
    let initial_sp = 0x007F_FE00;
    runner.bus.write_word(call_site, 0x4E90); // JSR (A0)
    runner.m68k.cpu.write_reg(Register::A0, gateway);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.m68k.cpu.write_reg(Register::PC, call_site);

    let (steps, running) = runner.run_steps(2, None);

    assert_eq!(steps, 2);
    assert!(!running);
    assert_eq!(runner.dispatcher.current_trap_caller, Some(call_site + 2));
    assert_eq!(
        runner
            .bus
            .read_word(crate::memory::globals::addr::DS_ERR_CODE),
        12
    );
}

fn cursor_warp_runner() -> FixtureRunner {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    runner.set_mouse_position(352, 380);
    let return_pc = 0x0002_0000;
    runner.bus.write_word(return_pc, 0x60FE); // BRA.S *
    runner.m68k.cpu.write_reg(Register::PC, return_pc);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FE00);
    runner
}

fn request_cursor_warp(runner: &mut FixtureRunner) {
    use crate::memory::globals::addr;
    runner.bus.write_long(addr::M_TEMP, (140 << 16) | 300);
    runner.bus.write_long(addr::MOUSE_LOC, (140 << 16) | 300);
    runner.bus.write_byte(0x08CE, 1); // CrsrNew
}

#[test]
fn cursor_task_direct_call_adopts_guest_warp() {
    use crate::memory::globals::addr;
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    let task = runner.bus.read_long(addr::J_CRSR_TASK);
    let sp = runner.m68k.cpu.read_reg(Register::A7);
    let pc = runner.m68k.cpu.read_reg(Register::PC);
    runner.bus.write_long(sp - 4, pc);
    runner.m68k.cpu.write_reg(Register::A7, sp - 4);
    runner.m68k.cpu.write_reg(Register::PC, task);
    runner.m68k.cpu.write_reg(Register::D0, 0x12345678);
    runner.m68k.cpu.write_reg(Register::A0, 0x87654321);

    assert!(runner.run_steps(30, None).1);

    assert_eq!(runner.bus.read_long(addr::MOUSE_LOC2), (140 << 16) | 300);
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.take_guest_cursor_warp(), Some((140, 300)));
    assert_eq!(runner.take_guest_cursor_warp(), None);
    assert_eq!(runner.bus.read_byte(0x08CE), 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x12345678);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x87654321);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    runner.set_mouse_position(150, 310);
    assert_eq!(runner.take_guest_cursor_warp(), None);
    assert_eq!(runner.dispatcher.mouse_position(), (150, 310));
    assert_eq!(runner.bus.read_long(addr::MOUSE_LOC2), (150 << 16) | 310);
}

#[test]
fn cursor_task_vbl_adopts_pending_guest_warp() {
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    runner.advance_guest_tick();
    runner.run_steps(30, None);
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.bus.read_byte(0x08CE), 0);
}

#[test]
fn cursor_task_warp_reaches_event_trap_in_same_batch() {
    use crate::memory::globals::addr;
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    let task = runner.bus.read_long(addr::J_CRSR_TASK);
    let sp = runner.m68k.cpu.read_reg(Register::A7);
    let pc = runner.m68k.cpu.read_reg(Register::PC);
    let event = 0x0003_0000;
    runner.bus.write_word(pc, 0xA970); // GetNextEvent
    runner.bus.write_word(pc + 2, 0x60FE);
    runner.bus.write_long(sp - 4, pc);
    runner.bus.write_long(sp, event);
    runner.bus.write_word(sp + 4, 0); // null event only
    runner.m68k.cpu.write_reg(Register::A7, sp - 4);
    runner.m68k.cpu.write_reg(Register::PC, task);
    runner.run_steps(30, None);
    assert_eq!(runner.bus.read_word(event), 0);
    assert_eq!(runner.bus.read_word(event + 10), 140);
    assert_eq!(runner.bus.read_word(event + 12), 300);
}

#[test]
fn cursor_task_guest_wrapper_can_chain_to_default_task() {
    use crate::memory::globals::addr;
    let mut runner = cursor_warp_runner();
    let task = runner.bus.read_long(addr::J_CRSR_TASK);
    let wrapper = runner.bus.alloc(8);
    runner.bus.write_word(wrapper, 0x4EB9); // JSR default cursor task
    runner.bus.write_long(wrapper + 2, task);
    runner.bus.write_word(wrapper + 6, 0x4E75);
    runner.bus.write_long(addr::J_CRSR_TASK, wrapper);
    request_cursor_warp(&mut runner);
    runner.advance_guest_tick();
    assert!(runner.active_interrupt_callback.is_some());
    runner.run_steps(40, None);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.bus.read_byte(addr::CRSR_NEW), 0);
}

#[test]
fn cursor_task_waits_for_request_and_respects_interrupt_mask() {
    use crate::memory::globals::addr;
    let mut runner = cursor_warp_runner();
    request_cursor_warp(&mut runner);
    runner.bus.write_byte(addr::CRSR_NEW, 0);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (352, 380));
    runner.bus.write_byte(addr::CRSR_NEW, 1);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2100);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (352, 380));
    assert_eq!(runner.bus.read_byte(addr::CRSR_NEW), 1);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    runner.advance_guest_tick();
    assert_eq!(runner.dispatcher.mouse_position(), (140, 300));
    assert_eq!(runner.bus.read_byte(addr::CRSR_NEW), 0);
}

#[test]
fn init_app_seeds_cursor_task_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };

    runner.init_app(&app);

    assert_eq!(
        runner.bus.read_word(runner.default_cursor_task),
        0x4A38,
        "default cursor task should test the pending update flag"
    );
    assert_eq!(
        runner
            .bus
            .read_long(crate::memory::globals::addr::J_CRSR_TASK),
        runner.default_cursor_task,
        "JCrsrTask ($08EE) should boot to the callable cursor updater"
    );
}

#[test]
fn init_app_seeds_callable_show_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let entry = runner
        .bus
        .read_long(crate::memory::globals::addr::J_SHOW_CURSOR);
    assert_ne!(entry, 0);
    assert_eq!(
        [runner.bus.read_word(entry), runner.bus.read_word(entry + 2),],
        [0xA853, 0x4E75],
        "JShowCursor should target ShowCursor followed by RTS"
    );

    let call_site = 0x0002_0000u32;
    let initial_sp = 0x007F_FE00u32;
    runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0804).W,A0
    runner.bus.write_word(call_site + 2, 0x0804);
    runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
    runner.bus.write_word(call_site + 6, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.dispatcher.cursor_state.set_level_for_test(-1);

    let (steps, running) = runner.run_steps(4, None);

    assert!(running);
    assert_eq!(steps, 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
    assert_eq!(runner.dispatcher.cursor_level(), 0);
    assert!(runner.dispatcher.cursor_visible());
}

#[test]
fn init_app_seeds_callable_init_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let entry = runner
        .bus
        .read_long(crate::memory::globals::addr::J_INIT_CRSR);
    assert_ne!(entry, 0);
    assert_eq!(
        [runner.bus.read_word(entry), runner.bus.read_word(entry + 2)],
        [0xA850, 0x4E75],
        "JInitCrsr should target InitCursor followed by RTS"
    );

    let call_site = 0x0002_0000u32;
    let initial_sp = 0x007F_FE00u32;
    runner.bus.write_word(call_site, 0x2078); // MOVEA.L ($0814).W,A0
    runner.bus.write_word(call_site + 2, 0x0814);
    runner.bus.write_word(call_site + 4, 0x4E90); // JSR (A0)
    runner.bus.write_word(call_site + 6, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, call_site);
    runner.m68k.cpu.write_reg(Register::A7, initial_sp);
    runner.dispatcher.cursor_state.set_level_for_test(-1);

    let (steps, running) = runner.run_steps(4, None);

    assert!(running);
    assert_eq!(steps, 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), call_site + 6);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), initial_sp);
    assert_eq!(runner.dispatcher.cursor_level(), 0);
    assert!(runner.dispatcher.cursor_visible());
}

#[test]
fn init_app_seeds_callable_swap_font_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let swap_font_trampoline = runner
        .bus
        .read_long(crate::memory::globals::addr::J_SWAP_FONT);
    assert_ne!(swap_font_trampoline, 0);
    assert_eq!(
        [
            runner.bus.read_word(swap_font_trampoline),
            runner.bus.read_word(swap_font_trampoline + 2),
            runner.bus.read_word(swap_font_trampoline + 4),
        ],
        [0x205F, 0xA901, 0x4ED0]
    );

    let fm_input_sp = 0x007F_FE00u32;
    let fm_input = 0x0002_1000u32;
    let return_pc = 0x0002_0000u32;
    runner.bus.write_word(fm_input, 3); // family
    runner.bus.write_word(fm_input + 2, 12); // size
    runner.bus.write_byte(fm_input + 4, 0); // face
    runner.bus.write_byte(fm_input + 5, 1); // needBits
    runner.bus.write_word(fm_input + 6, 0); // device
    runner.bus.write_word(fm_input + 8, 1); // numer.v
    runner.bus.write_word(fm_input + 10, 1); // numer.h
    runner.bus.write_word(fm_input + 12, 1); // denom.v
    runner.bus.write_word(fm_input + 14, 1); // denom.h
    runner.bus.write_long(fm_input_sp, fm_input); // CONST VAR inRec
    runner.bus.write_long(fm_input_sp + 4, 0); // result slot
    runner.bus.write_long(fm_input_sp - 4, return_pc);
    runner.bus.write_word(return_pc, 0x4E71); // NOP
    runner
        .m68k
        .cpu
        .write_reg(Register::PC, swap_font_trampoline);
    runner.m68k.cpu.write_reg(Register::A7, fm_input_sp - 4);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), fm_input_sp + 4);
    assert_ne!(
        runner.bus.read_long(fm_input_sp + 4),
        0,
        "JSwapFont should return a non-NIL FMOutPtr through the Pascal result slot"
    );
}

#[test]
fn init_app_seeds_callable_shield_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let shield_cursor_trampoline = runner
        .bus
        .read_long(crate::memory::globals::addr::J_SHIELD_CURSOR);
    assert_ne!(shield_cursor_trampoline, 0);
    assert_eq!(
        [
            runner.bus.read_word(shield_cursor_trampoline),
            runner.bus.read_word(shield_cursor_trampoline + 2),
            runner.bus.read_word(shield_cursor_trampoline + 4),
        ],
        [0x205F, 0xA855, 0x4ED0]
    );

    let args_sp = 0x007F_FE00u32;
    let return_pc = 0x0002_0000u32;
    runner.bus.write_word(args_sp, 100); // left
    runner.bus.write_word(args_sp + 2, 120); // top
    runner.bus.write_word(args_sp + 4, 500); // right
    runner.bus.write_word(args_sp + 6, 420); // bottom
    runner.bus.write_long(args_sp - 4, return_pc);
    runner.bus.write_word(return_pc, 0x4E71); // NOP
    runner
        .m68k
        .cpu
        .write_reg(Register::PC, shield_cursor_trampoline);
    runner.m68k.cpu.write_reg(Register::A7, args_sp - 4);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::A7),
        args_sp + 8,
        "JShieldCursor should consume its four Pascal INTEGER arguments"
    );
}

#[test]
fn init_app_seeds_callable_hide_cursor_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let app = LoadedApp {
        ppc: None,
        code0_header: Code0Header {
            above_a5: 0,
            below_a5: 0x2000,
            jump_table_size: 0,
            jump_table_offset: 0,
        },
        a5_base: 0x0040_0000,
        jump_table: Vec::new(),
        segment_bases: HashMap::new(),
        loaded_image_end: 0,
        initial_sp: 0x007F_FFC0,
        size_resource: None,
    };
    runner.init_app(&app);

    let hide_cursor_trampoline = runner
        .bus
        .read_long(crate::memory::globals::addr::J_HIDE_CURSOR);
    assert_ne!(hide_cursor_trampoline, 0);
    assert_eq!(
        [
            runner.bus.read_word(hide_cursor_trampoline),
            runner.bus.read_word(hide_cursor_trampoline + 2),
            runner.bus.read_word(hide_cursor_trampoline + 4),
        ],
        [0x205F, 0xA852, 0x4ED0],
        "JHideCursor should pop the JSR return address, trap, and jump back"
    );

    let call_sp = 0x007F_FE00u32;
    let return_pc = 0x0002_0000u32;
    runner.bus.write_long(call_sp - 4, return_pc);
    runner.bus.write_word(return_pc, 0x4E71);
    runner
        .m68k
        .cpu
        .write_reg(Register::PC, hide_cursor_trampoline);
    runner.m68k.cpu.write_reg(Register::A7, call_sp - 4);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), return_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), call_sp);
    assert_eq!(runner.dispatcher().cursor_level(), -1);
}

#[test]
fn cursor_task_default_vector_does_not_inject_interrupt_on_guest_tick() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.install_cursor_task();
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner.bus.write_long(
        crate::memory::globals::addr::J_CRSR_TASK,
        runner.default_cursor_task,
    );
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.advance_guest_tick();

    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.cursor_task_trampoline, 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert_eq!(runner.bus.read_long(crate::memory::globals::addr::TICKS), 1);
}

#[test]
fn deferred_task_runs_at_next_interrupt_with_task_in_a0_and_parameter_in_a1() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let task = runner.bus.alloc(24);
    let callback = 0x0004_1234;
    let parameter = 0x8765_4321;
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let marker = 0x0005_0000;
    runner.bus.write_word(callback, 0x23C8); // MOVE.L A0,marker
    runner.bus.write_long(callback + 2, marker);
    runner.bus.write_word(callback + 6, 0x23C9); // MOVE.L A1,marker+4
    runner.bus.write_long(callback + 8, marker + 4);
    runner.bus.write_word(callback + 12, 0x4E75); // RTS
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.bus.write_word(task + 4, 7);
    runner.bus.write_long(task + 8, callback);
    runner.bus.write_long(task + 12, parameter);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::A0, 0x1234_5678);
    runner
        .dispatcher
        .enqueue_deferred_task(&mut runner.bus, task);

    assert!(!runner.fire_deferred_task());
    runner.advance_guest_tick();

    let active = runner
        .active_interrupt_callback
        .expect("deferred callback should run");
    assert_eq!(active.source, ActiveInterruptCallbackSource::DeferredTask);
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);
    let trampoline = runner.deferred_task_trampoline;
    assert_eq!(runner.bus.read_word(trampoline), 0x207C);
    assert_eq!(runner.bus.read_long(trampoline + 2), task);
    assert_eq!(runner.bus.read_word(trampoline + 6), 0x227C);
    assert_eq!(runner.bus.read_long(trampoline + 8), parameter);
    assert_eq!(runner.bus.read_word(trampoline + 12), 0x4EB9);
    assert_eq!(runner.bus.read_long(trampoline + 14), callback);
    assert_eq!(runner.bus.read_word(trampoline + 18), 0x4E75);
    assert!(runner.dispatcher.deferred_tasks.is_empty());

    runner.run_steps(8, None);
    assert_eq!(runner.bus.read_long(marker), task);
    assert_eq!(runner.bus.read_long(marker + 4), parameter);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x1234_5678);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
}

#[test]
fn cursor_task_callback_arms_interrupt_from_low_memory_vector() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = 0x0004_1234;

    runner
        .bus
        .write_long(crate::memory::globals::addr::J_CRSR_TASK, callback_addr);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::D0, 0x1111_1111);
    runner.m68k.cpu.write_reg(Register::D7, 0x7777_7777);
    runner.m68k.cpu.write_reg(Register::A0, 0xAAAA_0000);
    runner.m68k.cpu.write_reg(Register::A6, 0xCCCC_0000);
    runner.m68k.cpu.core.set_ccr(0x04);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2004);

    runner.advance_guest_tick();

    let active = runner
        .active_interrupt_callback
        .expect("cursor task callback should have been armed");
    assert!(matches!(
        active.source,
        ActiveInterruptCallbackSource::CursorTask
    ));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);
    assert_eq!(active.a_regs[7], interrupted_sp);
    assert_eq!(active.a_regs[6], 0xCCCC_0000);
    assert_eq!(active.d_regs[0], 0x1111_1111);
    assert_eq!(active.d_regs[7], 0x7777_7777);
    assert_eq!(active.sr, 0x2004);
    assert_eq!(active.ccr, 0x04);
    assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2104);

    assert_ne!(runner.cursor_task_trampoline, 0);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        runner.cursor_task_trampoline
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp - 4);
    assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
    assert_eq!(runner.bus.read_word(runner.cursor_task_trampoline), 0x48E7);
    assert_eq!(
        runner.bus.read_word(runner.cursor_task_trampoline + 4),
        0x4EB9
    );
    assert_eq!(
        runner.bus.read_long(runner.cursor_task_trampoline + 6),
        callback_addr
    );
    assert_eq!(
        runner.bus.read_word(runner.cursor_task_trampoline + 10),
        0x4CDF
    );
    assert_eq!(
        runner.bus.read_word(runner.cursor_task_trampoline + 14),
        0x4E75
    );
}

#[test]
fn cursor_task_defers_while_processor_priority_masks_level_one() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner
        .bus
        .write_long(crate::memory::globals::addr::J_CRSR_TASK, 0x0004_1234);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2100);

    runner.advance_guest_tick();

    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.cursor_task_trampoline, 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
}

#[test]
fn timer_callback_snapshot_preserves_interrupted_sp() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_8BAC;
    let interrupted_sp = 0x007F_FFC0;

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::D0, 0x1111_1111);
    runner.m68k.cpu.write_reg(Register::D7, 0x7777_7777);
    runner.m68k.cpu.write_reg(Register::A0, 0xAAAA_0000);
    runner.m68k.cpu.write_reg(Register::A6, 0xCCCC_0000);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_ccr(0x1F);
    runner.bus.write_word(0x0039_38C8 + 4, 0x8001);

    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: 0x0039_38C8,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: 0x0004_1234,
        active: true,
        fire_at_tick: 10,
        fire_at_subtick: 10_000_000,
        last_fired_tick: None,
    });

    runner.fire_timer_tasks(10);

    let active = runner
        .active_interrupt_callback
        .expect("timer callback should have been armed");

    assert!(matches!(
        active.source,
        ActiveInterruptCallbackSource::Timer
    ));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);
    assert_eq!(active.a_regs[7], interrupted_sp);
    assert_eq!(active.a_regs[6], 0xCCCC_0000);
    assert_eq!(active.d_regs[0], 0x1111_1111);
    assert_eq!(active.d_regs[7], 0x7777_7777);
    assert_eq!(active.sr & 0x001F, 0x001F);
    assert_eq!(active.ccr, 0x1F);
    assert_eq!(
        runner.bus.read_word(0x0039_38C8 + 4),
        1,
        "an expired Time Manager task must be inactive before tmAddr runs"
    );

    assert_ne!(runner.timer_trampoline, 0);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        runner.timer_trampoline
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp - 4);
    assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
}

#[test]
fn timer_callback_fired_at_tick_cap_runs_before_yielding() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = 0x0002_0000;

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.bus.write_long(0x016A, 100);
    runner.set_guest_tick_for_test(100);
    runner.tick_budget = 0;
    runner.bus.write_word(callback_addr, 0x4E75); // RTS
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: 0x0039_38C8,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: callback_addr,
        active: true,
        fire_at_tick: 101,
        fire_at_subtick: 101_000_000,
        last_fired_tick: None,
    });

    let (steps, running) = runner.run_steps(1, Some(101));

    assert!(running);
    assert_eq!(
        steps, 1,
        "a timer fired while reaching the tick cap must get a CPU slice"
    );
    assert_eq!(runner.bus.read_long(0x016A), 101);
    assert!(runner.active_interrupt_callback.is_some());
    assert_ne!(runner.timer_trampoline, 0);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        runner.timer_trampoline + 4
    );
}

#[test]
fn sub_vbl_timer_callback_fires_before_next_guest_tick() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = 0x0002_0000;

    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S to self
    runner.bus.write_word(callback_addr, 0x4E75); // RTS
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.bus.write_long(0x016A, 100);
    runner.set_guest_tick_for_test(100);
    runner.tick_budget = runner.instructions_per_tick as i32;
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: 0x0039_38C8,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: callback_addr,
        active: true,
        fire_at_tick: 101,
        fire_at_subtick: 100_200_000,
        last_fired_tick: None,
    });

    let steps = runner.instructions_per_tick as usize / 4;
    let (executed, running) = runner.run_steps(steps, None);
    let (_, still_running) = runner.run_steps(1, None);

    assert!(running);
    assert!(still_running);
    assert_eq!(executed, steps);
    assert_eq!(runner.guest_tick(), 100);
    assert!(!runner.dispatcher.timer_tasks[0].active);
    assert_ne!(runner.timer_trampoline, 0);
}

#[test]
fn timer_callback_return_runs_foreground_before_next_due_timer() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = 0x0002_0000;

    runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.bus.write_long(0x016A, 101);
    runner.set_guest_tick_for_test(101);
    runner.set_instructions_per_tick(1);
    runner.tick_budget = 0;
    runner.active_interrupt_callback = Some(ActiveInterruptCallback {
        source: ActiveInterruptCallbackSource::Timer,
        resume_pc: interrupted_pc,
        resume_sp: interrupted_sp,
        d_regs: [0; 8],
        a_regs: [0, 0, 0, 0, 0, 0, 0, interrupted_sp],
        sr: 0x2000,
        ccr: 0,
        restore_port: None,
    });
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr: 0x0039_38C8,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: callback_addr,
        active: true,
        fire_at_tick: 102,
        fire_at_subtick: 102_000_000,
        last_fired_tick: None,
    });

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        interrupted_pc + 2,
        "resumed foreground instruction should run before the next timer interrupt"
    );
    assert_eq!(
            runner.guest_tick(),
            101,
            "returning from an interrupt must not immediately spend an exhausted budget on another tick"
        );
    assert!(runner.active_interrupt_callback.is_none());
    assert!(
        runner.dispatcher.timer_tasks[0].active,
        "the next due timer should remain queued until foreground code gets a slice"
    );
}

#[test]
fn simultaneous_timer_callbacks_keep_undelivered_tasks_active() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.dispatcher.timer_tasks.extend([
        TimerTask {
            task_ptr: 0x0039_38C8,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: 0x0002_0000,
            active: true,
            fire_at_tick: 10,
            fire_at_subtick: 10_000_000,
            last_fired_tick: None,
        },
        TimerTask {
            task_ptr: 0x0039_3900,
            architecture: CallbackTaskArchitecture::M68k,
            extended: false,
            callback: 0x0002_1000,
            active: true,
            fire_at_tick: 10,
            fire_at_subtick: 10_000_000,
            last_fired_tick: None,
        },
    ]);

    runner.fire_timer_tasks(10);

    assert!(!runner.dispatcher.timer_tasks[0].active);
    assert!(
        runner.dispatcher.timer_tasks[1].active,
        "a second task due on the same tick must remain queued"
    );

    // The delivered task may re-prime itself from its callback. It must not
    // jump ahead of an older task that is still waiting for delivery.
    runner.dispatcher.timer_tasks.with_mut(|timer_tasks| {
        timer_tasks[0].active = true;
        timer_tasks[0].fire_at_tick = 11;
        timer_tasks[0].fire_at_subtick = 11_000_000;
    });
    runner.active_interrupt_callback = None;
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.fire_timer_tasks(11);

    assert!(
        runner.dispatcher.timer_tasks[0].active,
        "the newly re-primed task must wait behind the older due task"
    );
    assert!(!runner.dispatcher.timer_tasks[1].active);
    assert_eq!(
        runner.bus.read_long(runner.timer_trampoline + 6),
        0x0039_3900
    );
}

#[test]
fn self_reprimed_timer_can_fire_again_within_the_same_vbl() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let task_ptr = 0x0039_38C8;

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.dispatcher.timer_tasks.push(TimerTask {
        task_ptr,
        architecture: CallbackTaskArchitecture::M68k,
        extended: false,
        callback: 0x0002_0000,
        active: true,
        fire_at_tick: 10,
        fire_at_subtick: 10_100_000,
        last_fired_tick: None,
    });

    runner.fire_timer_tasks_at(10_100_000);
    assert_eq!(runner.dispatcher.timer_tasks[0].last_fired_tick, Some(10));

    // Model the callback returning and re-priming itself for another
    // revised Time Manager deadline inside the same VBL.
    runner.active_interrupt_callback = None;
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.dispatcher.timer_tasks.with_mut(|timer_tasks| {
        timer_tasks[0].active = true;
        timer_tasks[0].fire_at_tick = 11;
        timer_tasks[0].fire_at_subtick = 10_300_000;
    });

    runner.fire_timer_tasks_at(10_300_000);
    assert!(
        runner.active_interrupt_callback.is_some(),
        "a revised Time Manager task must honor a new sub-VBL deadline"
    );
    assert!(!runner.dispatcher.timer_tasks[0].active);
    assert_eq!(runner.dispatcher.timer_tasks[0].last_fired_tick, Some(10));
}

#[test]
fn sound_doubleback_callback_resume_restores_ccr_before_branch() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let header_ptr = 0x0020_0000;
    let exhausted_buf_ptr = 0x0020_1000;

    // BEQ.s -> MOVEQ #2,D0 path should be taken when Z is preserved.
    runner.bus.write_word(interrupted_pc, 0x6704);
    runner.bus.write_word(interrupted_pc + 2, 0x7001);
    runner.bus.write_word(interrupted_pc + 4, 0x6002);
    runner.bus.write_word(interrupted_pc + 6, 0x7002);
    runner.bus.write_word(interrupted_pc + 8, 0x4E71);

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::D0, 0);
    runner.m68k.cpu.core.set_ccr(0x04);

    runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
    runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr: 0x0004_1234,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.fire_sound_doubleback_callbacks();

    let active = runner
        .active_interrupt_callback
        .expect("sound callback should have been armed");
    assert!(matches!(
        active.source,
        ActiveInterruptCallbackSource::SoundDoubleBack
    ));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);

    // Simulate the trampoline returning to interrupted code with CCR clobbered.
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_ccr(0);

    let (steps, running) = runner.run_steps(3, None);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 2);
    assert!(runner.active_interrupt_callback.is_none());
}

#[test]
fn sound_doubleback_callback_trampoline_stacks_classic_pascal_order() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let header_ptr = 0x0020_0000;
    let chan_ptr = 0x0039_38C8;
    let exhausted_buf_ptr = 0x0020_1000;

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
    runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.fire_sound_doubleback_callbacks();
    let (_steps, running) = runner.run_steps(24, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    let saved_regs_sp = interrupted_sp - 4 - 32;
    assert_eq!(
        runner.bus.read_long(saved_regs_sp - 4),
        chan_ptr,
        "the first declared Pascal argument is pushed first"
    );
    assert_eq!(
        runner.bus.read_long(saved_regs_sp - 8),
        exhausted_buf_ptr,
        "the last declared Pascal argument is nearest the return address"
    );
}

fn write_double_buffer(bus: &mut MacMemoryBus, ptr: u32, samples: &[u8]) {
    bus.write_long(ptr, samples.len() as u32);
    bus.write_long(ptr + 4, 0x0000_0001);
    for (offset, sample) in samples.iter().copied().enumerate() {
        bus.write_byte(ptr + 16 + offset as u32, sample);
    }
}

#[test]
fn mix_audio_loads_ready_double_buffer_without_boundary_silence() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(18);
    let buf1_ptr = runner.bus.alloc(18);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0x90, 0x91]);
    write_double_buffer(&mut runner.bus, buf1_ptr, &[0xA0, 0xA1]);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    crate::trap::TrapDispatcher::load_double_buffer_samples(
        &mut runner.bus,
        &mut chan,
        buf0_ptr,
        OUTPUT_RATE << 16,
        1,
        8,
    );
    runner.dispatcher.sound_manager.add_channel(chan);

    runner.mix_audio(3);

    assert_eq!(
        runner.audio_buffer,
        vec![0x90, 0x91, 0xA0],
        "host mixing must continue into the ready paired buffer, not emit boundary silence"
    );
    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "dbBufferReady stays set until the doubleback callback starts"
    );
    assert_eq!(
        runner.bus.read_long(buf1_ptr + 4) & 0x01,
        0x01,
        "the paired buffer is still marked ready while it is playing"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks.len(),
        1,
        "exhausting buffer 0 still queues its doubleback refill"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
        0
    );

    let chan = &runner.dispatcher.sound_manager.channels[0];
    assert!(chan.is_playing(), "buffer 1 should still be playing");
    let db = chan.double_buffer.as_ref().expect("double-buffer active");
    assert_eq!(db.current_buffer, 1);
    assert!(db.waiting_for_callback);
}

#[test]
fn mix_audio_can_queue_other_doubleback_while_callback_is_active() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(17);
    let buf1_ptr = runner.bus.alloc(17);
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner.bus.write_word(interrupted_pc, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
    runner.bus.write_long(buf1_ptr, 1);
    runner.bus.write_long(buf1_ptr + 4, 0);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    crate::trap::TrapDispatcher::load_double_buffer_samples(
        &mut runner.bus,
        &mut chan,
        buf0_ptr,
        OUTPUT_RATE << 16,
        1,
        8,
    );
    runner.dispatcher.sound_manager.add_channel(chan);

    runner.mix_audio(1);
    assert_eq!(runner.dispatcher.sound_manager.pending_callbacks.len(), 1);
    assert!(
        runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active")
            .waiting_for_callback
    );

    runner.fire_sound_doubleback_callbacks();
    assert!(matches!(
        runner
            .active_interrupt_callback
            .expect("doubleback callback should be active")
            .source,
        ActiveInterruptCallbackSource::SoundDoubleBack
    ));
    assert!(
        runner.dispatcher.sound_manager.channels[0]
            .double_buffer
            .as_ref()
            .expect("double-buffer active")
            .waiting_for_callback,
        "callback remains outstanding until guest refills a buffer"
    );

    runner.mix_audio(16);

    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks.len(),
        1,
        "the paired unready buffer may queue its own callback while buffer 0 is active"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index, 1,
        "buffer 0 must not be duplicated; buffer 1 gets the new callback"
    );
    let db = runner.dispatcher.sound_manager.channels[0]
        .double_buffer
        .as_ref()
        .expect("double-buffer active");
    assert!(db.waiting_for_callback);
    assert_eq!(db.pending_callback_buffers, [true, true]);
}

#[test]
fn mix_audio_does_not_load_ready_double_buffer_while_callback_is_active() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(17);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: true,
        pending_callback_buffers: [true, false],
    });
    runner.dispatcher.sound_manager.add_channel(chan);
    runner.active_interrupt_callback = Some(ActiveInterruptCallback {
        source: ActiveInterruptCallbackSource::SoundDoubleBack,
        resume_pc: 0x0001_0000,
        resume_sp: 0x007F_FFC0,
        d_regs: [0; 8],
        a_regs: [0; 8],
        sr: 0x2000,
        ccr: 0,
        restore_port: None,
    });

    runner.mix_audio(1);

    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "ready buffer must not be consumed before the callback returns"
    );
    assert!(
        !runner.dispatcher.sound_manager.channels[0].is_playing(),
        "callback-active buffer load should be deferred"
    );
    assert_eq!(
        runner.audio_buffer,
        vec![0x80],
        "the host stream stays alive with silence while waiting"
    );

    runner.active_interrupt_callback = None;
    runner.try_load_pending_double_buffers();

    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "returned callback buffer stays marked ready while playback owns it"
    );
    assert!(
        runner.dispatcher.sound_manager.channels[0].is_playing(),
        "returned callback makes the refilled buffer available to the mixer"
    );
    let db = runner.dispatcher.sound_manager.channels[0]
        .double_buffer
        .as_ref()
        .expect("double-buffer active");
    assert_eq!(db.pending_callback_buffers, [false, false]);

    runner.mix_audio(1);
    assert_eq!(runner.audio_buffer, vec![0x80, 0xA0]);
}

#[test]
fn try_load_pending_double_buffers_recovers_ready_alternate_after_underrun() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(18);
    let buf1_ptr = runner.bus.alloc(18);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0, 0xA1]);
    runner.bus.write_long(buf1_ptr, 2);
    runner.bus.write_long(buf1_ptr + 4, 0);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 1,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    runner.dispatcher.sound_manager.add_channel(chan);

    runner.try_load_pending_double_buffers();

    let chan = &runner.dispatcher.sound_manager.channels[0];
    assert!(chan.is_playing(), "ready alternate buffer should load");
    let db = chan.double_buffer.as_ref().expect("double-buffer active");
    assert_eq!(db.current_buffer, 0);
    assert!(
        !db.waiting_for_callback,
        "loading a ready buffer completes the outstanding refill wait"
    );
    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "loading a ready alternate must not clear dbBufferReady before playback exhausts"
    );
}

#[test]
fn try_load_pending_double_buffers_does_not_replay_callback_pending_slot() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let chan_ptr = 0x0039_38C8;
    let callback_addr = 0x0004_1234;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(17);
    let buf1_ptr = runner.bus.alloc(17);

    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(header_ptr + 20, callback_addr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0]);
    runner.bus.write_long(buf1_ptr, 1);
    runner.bus.write_long(buf1_ptr + 4, 0);

    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 0,
        callback_addr,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: true,
        pending_callback_buffers: [true, false],
    });
    runner.dispatcher.sound_manager.add_channel(chan);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.try_load_pending_double_buffers();

    assert!(
        !runner.dispatcher.sound_manager.channels[0].is_playing(),
        "an exhausted slot must not replay just because dbBufferReady remains set"
    );
    assert_eq!(
        runner.bus.read_long(buf0_ptr + 4) & 0x01,
        0x01,
        "the flag remains ready until fire_sound_doubleback_callbacks clears it"
    );
}

#[test]
fn sound_command_callback_trampoline_passes_sndcommand_pointer() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr: 0x0004_5678,
            chan_ptr: 0x0039_38C8,
            cmd: crate::sound::SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0x1234,
                param2: 0x0001_43FC,
            },
        },
    );

    runner.fire_sound_callbacks();

    let active = runner
        .active_interrupt_callback
        .expect("sound callback should have been armed");
    assert!(matches!(
        active.source,
        ActiveInterruptCallbackSource::SoundCallback
    ));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);

    let tramp = runner.sound_callback_trampoline;
    let cmd_ptr = tramp + 34;
    let saved_regs_sp = interrupted_sp - 4 - 32;
    assert_eq!(runner.bus.read_long(tramp + 6), 0x0039_38C8);
    assert_eq!(runner.bus.read_long(tramp + 12), cmd_ptr);
    assert_eq!(runner.bus.read_long(tramp + 18), 0x0004_5678);
    assert_eq!(runner.bus.read_long(tramp + 24), saved_regs_sp);
    assert_eq!(runner.bus.read_word(cmd_ptr), crate::sound::cmd::CALLBACK);
    assert_eq!(runner.bus.read_word(cmd_ptr + 2), 0x1234);
    assert_eq!(runner.bus.read_long(cmd_ptr + 4), 0x0001_43FC);
    assert_eq!(
        runner.bus.get_alloc_size(tramp),
        None,
        "Systemless-owned command callback trampoline must stay outside the guest heap"
    );
}

#[test]
fn sound_command_callback_trampoline_does_not_perturb_guest_allocations() {
    let mut baseline = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let _baseline_callback = baseline.bus.alloc(2);
    let expected_next_guest_ptr = baseline.bus.alloc(64);

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let callback_addr = runner.bus.alloc(2);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: crate::sound::SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0,
                param2: 0,
            },
        },
    );

    runner.fire_sound_callbacks();
    let actual_next_guest_ptr = runner.bus.alloc(64);

    assert_eq!(
        actual_next_guest_ptr, expected_next_guest_ptr,
        "lazy callback setup must not consume application-visible heap space"
    );
}

#[test]
fn file_completion_callback_uses_documented_registers_and_restores_foreground() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let parameter_block = runner.bus.alloc(64);
    let callback_addr = runner.bus.alloc(2);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS
    for offset in (0..20).step_by(2) {
        runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
    }
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::A0, 0x1111_1111);
    runner.m68k.cpu.write_reg(Register::D0, 0x2222_2222);
    runner
        .dispatcher
        .pending_file_completions
        .push_back(PendingFileCompletion {
            parameter_block,
            completion_addr: callback_addr,
            result: -39,
        });

    assert!(runner.fire_file_completion_callback());
    assert_eq!(runner.bus.read_word(parameter_block + 16) as i16, -39);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), parameter_block);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0) as i32, -39);
    assert!(matches!(
        runner.active_interrupt_callback.map(|active| active.source),
        Some(ActiveInterruptCallbackSource::FileCompletion)
    ));
    assert!(
        runner
            .bus
            .get_alloc_size(runner.file_completion_trampoline)
            .is_none(),
        "Systemless-owned completion trampoline must stay outside the guest heap"
    );

    let (_, running) = runner.run_steps(6, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x1111_1111);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x2222_2222);
    assert!(!runner.is_halted());
}

#[test]
fn guest_cursor_recenter_does_not_generate_physical_adb_motion() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    assert!(runner
        .dispatcher
        .adb
        .set_device_handler(3, 0x0012_3456, 0, false));

    runner.set_mouse_position(100, 100);
    runner.dispatcher.adb.flush(3);

    let previous_mouse = runner
        .bus
        .read_long(crate::memory::globals::addr::MOUSE_LOC2);
    runner
        .bus
        .write_long(crate::memory::globals::addr::MOUSE_LOC2, (300 << 16) | 296);
    runner.sync_guest_mouse_position(previous_mouse);

    assert_eq!(runner.dispatcher.mouse_position(), (300, 296));
    assert_eq!(runner.dispatcher.adb.pending_packet_count(), 0);

    runner.set_mouse_position(100, 120);
    assert_eq!(runner.dispatcher.adb.pending_packet_count(), 1);
    assert_eq!(
        runner.dispatcher.adb.pop_pending_packet().unwrap().packet,
        [2, 0x80, 0x94]
    );
}

#[test]
fn adb_mouse_callback_uses_documented_registers_and_restores_foreground() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let data_area = runner.bus.alloc(16);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS
    for offset in (0..20).step_by(2) {
        runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
    }
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::A0, 0x1111_1111);
    runner.m68k.cpu.write_reg(Register::A1, 0x2222_2222);
    runner.m68k.cpu.write_reg(Register::A2, 0x3333_3333);
    runner.m68k.cpu.write_reg(Register::D0, 0x4444_4444);
    assert!(runner
        .dispatcher
        .adb
        .set_device_handler(3, callback_addr, data_area, false,));
    runner.dispatcher.adb.note_mouse_state((5, -10), true);

    assert!(runner.fire_adb_callback());
    let packet_ptr = runner.m68k.cpu.read_reg(Register::A0);
    assert_eq!(runner.bus.read_bytes(packet_ptr, 3), &[2, 5, 0xF6]);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A1), callback_addr);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A2), data_area);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x3C);
    assert!(matches!(
        runner.active_interrupt_callback.map(|active| active.source),
        Some(ActiveInterruptCallbackSource::Adb)
    ));
    assert!(
        runner
            .bus
            .get_alloc_size(runner.adb_callback_trampoline)
            .is_none(),
        "Systemless-owned ADB trampoline must stay outside the guest heap"
    );

    let (_, running) = runner.run_steps(6, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), 0x1111_1111);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A1), 0x2222_2222);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A2), 0x3333_3333);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 0x4444_4444);
    assert!(!runner.is_halted());
}

#[test]
fn sound_command_callback_trampoline_tolerates_one_long_pascal_cleanup() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(4);

    // Some Pascal callback epilogues pop one long argument by copying the
    // return address over it, then RTS.
    runner.bus.write_word(callback_addr, 0x2E9F); // MOVE.L (SP)+,(SP)
    runner.bus.write_word(callback_addr + 2, 0x4E75); // RTS
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: crate::sound::SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0x1234,
                param2: 0x0001_43FC,
            },
        },
    );

    runner.fire_sound_callbacks();
    let (steps, running) = runner.run_steps(10, None);

    assert!(running, "callback trampoline should resume foreground code");
    assert_eq!(steps, 10);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(!runner.is_halted());
    assert_eq!(
        runner
            .bus
            .get_alloc_size(runner.sound_file_completion_trampoline),
        None,
        "Systemless-owned file completion trampoline must stay outside the guest heap"
    );
}

#[test]
fn sound_file_completion_callback_trampoline_tolerates_c_style_cleanup() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping chan.
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::FileCompletion {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
        },
    );

    runner.fire_sound_callbacks();
    let (steps, running) = runner.run_steps(10, None);

    assert!(
        running,
        "file completion trampoline should resume foreground code"
    );
    assert_eq!(steps, 10);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(!runner.is_halted());
    assert_eq!(
        runner
            .bus
            .get_alloc_size(runner.sound_doubleback_trampoline),
        None,
        "Systemless-owned double-back trampoline must stay outside the guest heap"
    );
}

#[test]
fn sound_doubleback_callback_trampoline_tolerates_c_style_cleanup() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let header_ptr = 0x0020_0000;
    let exhausted_buf_ptr = 0x0020_1000;

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    runner.bus.write_word(interrupted_pc, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_long(header_ptr + 12, exhausted_buf_ptr);
    runner.bus.write_long(exhausted_buf_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 0,
        });

    runner.fire_sound_doubleback_callbacks();
    let (steps, running) = runner.run_steps(12, None);

    assert!(
        running,
        "doubleback trampoline should resume foreground code"
    );
    assert_eq!(steps, 12);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(!runner.is_halted());
}

#[test]
fn run_pending_sound_work_does_not_advance_ticks_or_foreground_code() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.bus.write_long(0x016A, 41);
    runner.set_guest_tick_for_test(41);
    runner.set_instructions_per_tick(1);
    runner.tick_budget = 0;

    runner
        .dispatcher
        .sound_manager
        .queue_sound_callback(PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0,
                param2: 0,
            },
        });

    let (steps, running) = runner.run_pending_sound_work(32);

    assert!(running);
    assert!(steps > 0, "sound callback trampoline should execute");
    assert_eq!(
        runner.guest_tick(),
        41,
        "callback-only slices must not advance application-visible ticks"
    );
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        interrupted_pc,
        "sound callback service must stop before resumed foreground code runs"
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(!runner.has_pending_sound_work());
}

#[test]
fn gui_cpu_slice_does_not_finalize_host_frame() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let callback_addr = runner.bus.alloc(2);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS
    runner
        .dispatcher
        .sound_manager
        .queue_sound_callback(PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr,
            chan_ptr: 0x0039_38C8,
            cmd: SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0,
                param2: 0,
            },
        });

    let (steps, running) = runner.run_gui_cpu_slice(0, 0);

    assert!(running);
    assert_eq!(steps, 0);
    assert!(
        runner.active_interrupt_callback.is_none(),
        "CPU-only GUI slices must not fire host-frame sound callbacks"
    );
    assert!(runner.has_pending_sound_work());
}

fn sound_chrome_runner() -> FixtureRunner {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let screen_base = 0x0040_0000;
    runner.dispatcher.screen_mode = (screen_base, 256, 256, 64, 8);
    runner.bus.write_long(0x0824, screen_base);
    runner.bus.write_word(0x0BAA, 20);
    // Menu titles come from the guest MenuList, not just the host cache.
    // Install a real menu so this oracle actually paints outline glyphs.
    let title = runner.bus.alloc(5);
    runner.bus.write_bytes(title, b"\x04File");
    let sp = 0x007F_FF80;
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_long(sp, title);
    runner.bus.write_word(sp + 4, 128);
    runner
        .dispatcher
        .dispatch(0xA931, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap(); // NewMenu
    let menu = runner.bus.read_long(sp + 6);
    assert_ne!(menu, 0);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_word(sp, 0);
    runner.bus.write_long(sp + 2, menu);
    runner
        .dispatcher
        .dispatch(0xA935, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap(); // InsertMenu
    runner.dispatcher.menu_bar_hidden = false;
    runner.prepare_text_presentation();
    runner.composite_frame();
    assert!(runner.bus.has_visible_outline_detail());
    let pixel = screen_base + 5 * 256 + 100;
    assert_ne!(runner.bus.read_byte(pixel), 0xAA);
    runner.bus.write_byte(pixel, 0xAA);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_word(0x0001_0000, 0x4E71); // foreground NOP
    runner.set_guest_tick_for_test(41);
    runner.set_instructions_per_tick(1);
    runner.tick_budget = 0;
    runner
}

#[test]
fn gui_sound_work_defers_chrome_but_matches_complete_slices() {
    for budget in [0, 1, 32] {
        let mut complete = sound_chrome_runner();
        let mut deferred = sound_chrome_runner();
        for runner in [&mut complete, &mut deferred] {
            let callback_addr = runner.bus.alloc(2);
            runner.bus.write_word(callback_addr, 0x4E75); // RTS
            for _ in 0..2 {
                runner.dispatcher.sound_manager.queue_sound_callback(
                    PendingSoundCallback::Command {
                        architecture: CallbackTaskArchitecture::M68k,
                        callback_addr,
                        chan_ptr: 0x0039_38C8,
                        cmd: SndCommand {
                            cmd: crate::sound::cmd::CALLBACK,
                            param1: 0,
                            param2: 0,
                        },
                    },
                );
            }
        }
        for _ in 0..64 {
            assert_eq!(
                complete.run_pending_sound_work(budget),
                deferred.run_gui_pending_sound_work(budget),
            );
            for reg in [Register::PC, Register::A7, Register::D0] {
                assert_eq!(
                    complete.m68k.cpu.read_reg(reg),
                    deferred.m68k.cpu.read_reg(reg)
                );
            }
            assert_eq!(deferred.guest_tick(), 41);
            assert_eq!(complete.guest_tick(), deferred.guest_tick());
            assert_eq!(
                complete.has_pending_sound_work(),
                deferred.has_pending_sound_work()
            );
            assert_eq!(
                complete
                    .dispatcher
                    .sound_manager
                    .pending_sound_callbacks
                    .len(),
                deferred
                    .dispatcher
                    .sound_manager
                    .pending_sound_callbacks
                    .len(),
            );
            let pixel = 0x0040_0000 + 5 * 256 + 100;
            assert_ne!(complete.bus.read_byte(pixel), 0xAA);
            assert_eq!(
                deferred.bus.read_byte(pixel),
                0xAA,
                "sound slices must not repaint chrome"
            );
            if budget == 0 || !deferred.has_pending_sound_work() {
                break;
            }
        }
        if budget > 0 {
            assert!(!deferred.has_pending_sound_work());
            assert_eq!(deferred.m68k.cpu.read_reg(Register::PC), 0x0001_0000);
            assert_eq!(deferred.m68k.cpu.read_reg(Register::A7), 0x007F_FFC0);
        }
        complete.composite_frame();
        deferred.composite_frame();
        assert!(
            deferred.bus.has_visible_outline_detail(),
            "fixture must exercise retained glyphs"
        );
        assert_eq!(
            complete.bus.save_pixel_bytes(0x0040_0000, 256 * 64),
            deferred.bus.save_pixel_bytes(0x0040_0000, 256 * 64),
            "logical pixels AND retained subpixel metadata must match",
        );
        let (cw, ch, complete_rgb, complete_draws) =
            complete.bus.outline_presentation_rgb().unwrap();
        let (dw, dh, deferred_rgb, deferred_draws) =
            deferred.bus.outline_presentation_rgb().unwrap();
        assert_eq!((cw, ch), (dw, dh));
        assert!(
            complete_rgb == deferred_rgb,
            "retained visible RGB must match"
        );
        // This cumulative counter is not visible state. Menu-bar caching
        // can eliminate glyph repainting in both paths; the overwritten
        // pixel assertions above still prove that only complete slices
        // restore chrome before the outer presentation pass.
        assert!(
            complete_draws >= deferred_draws,
            "deferred sound slices must not add glyph draws"
        );
        assert!(
            complete.bus.read_bytes(0, 8 * 1024 * 1024)
                == deferred.bus.read_bytes(0, 8 * 1024 * 1024)
        );
    }
}

#[test]
fn gui_sound_work_services_ready_double_buffers_even_with_zero_budget() {
    let mut runner = sound_chrome_runner();
    let chan_ptr = 0x0039_38C8;
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(18);
    let buf1_ptr = runner.bus.alloc(18);
    runner.bus.write_word(header_ptr, 1);
    runner.bus.write_word(header_ptr + 2, 8);
    runner.bus.write_long(header_ptr + 8, OUTPUT_RATE << 16);
    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    write_double_buffer(&mut runner.bus, buf0_ptr, &[0xA0, 0xA1]);
    runner.bus.write_long(buf1_ptr, 2);
    runner.bus.write_long(buf1_ptr + 4, 0);
    let mut chan = SndChannel::new(chan_ptr, false);
    chan.double_buffer = Some(DoubleBufferState {
        header_ptr,
        current_buffer: 1,
        callback_addr: 0,
        chan_ptr,
        sample_rate: OUTPUT_RATE << 16,
        num_channels: 1,
        sample_size: 8,
        last_buffer_seen: false,
        waiting_for_callback: false,
        pending_callback_buffers: [false; 2],
    });
    runner.dispatcher.sound_manager.add_channel(chan);
    assert_eq!(runner.run_gui_pending_sound_work(0), (0, true));
    let chan = &runner.dispatcher.sound_manager.channels[0];
    assert!(
        chan.is_playing(),
        "audio-only finalization must load a ready refill"
    );
    assert_eq!(chan.double_buffer.as_ref().unwrap().current_buffer, 0);
    assert_eq!(
        runner.audio_buffer_len(),
        0,
        "servicing is not an extra mix"
    );
    assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
    runner.mix_audio(2);
    assert_eq!(runner.audio_buffer, vec![0xA0, 0xA1]);
}

#[test]
fn gui_sound_work_leaves_parked_chrome_validation_to_composition() {
    let mut runner = sound_chrome_runner();
    runner.park_proven_idle_cycle(0x0002_0000, 205);
    assert!(runner.idle_cycle_sleep.is_some());
    // Test the finalization policy separately from guest execution: a
    // zero-budget CPU slice can independently cancel an idle observation.
    runner.finish_host_frame(FrameFinalization::AudioOnly, 0, true);
    assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
    runner.composite_frame();
    assert_ne!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
    assert!(
        runner.idle_cycle_sleep.is_none(),
        "changed repaint must still revoke the park"
    );
    assert!(runner.bus.suspend_write_probe().is_none());
}

#[test]
fn gui_sound_work_services_guest_written_queue_without_painting() {
    let mut runner = sound_chrome_runner();
    let chan_ptr = runner.bus.alloc(1088);
    runner
        .dispatcher
        .sound_manager
        .add_channel(SndChannel::new(chan_ptr, false));
    // Guest SndChannel: flags, qLength, qHead, qTail, then 8-byte commands.
    runner.bus.write_word(chan_ptr + 28, 0xFFFF);
    runner.bus.write_word(chan_ptr + 30, 128);
    runner.bus.write_word(chan_ptr + 32, 0);
    runner.bus.write_word(chan_ptr + 34, 1);
    runner
        .bus
        .write_word(chan_ptr + 36, crate::sound::cmd::VOLUME);
    runner.bus.write_word(chan_ptr + 38, 0);
    runner.bus.write_long(chan_ptr + 40, 0x0080_0040);
    assert_eq!(runner.run_gui_pending_sound_work(0), (0, true));
    assert_eq!(
        runner.bus.read_word(chan_ptr + 32),
        1,
        "guest queue must drain"
    );
    assert_eq!(
        runner.bus.read_word(chan_ptr + 28),
        0,
        "idle channel state must synchronize"
    );
    assert_eq!(
        runner.bus.read_word(chan_ptr + 20),
        0,
        "completed command must clear"
    );
    assert_eq!(runner.bus.read_byte(0x0040_0000 + 5 * 256 + 100), 0xAA);
}

#[test]
fn run_steps_paces_pending_sound_doublebacks_to_one_per_slice() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let callback_addr = runner.bus.alloc(2);
    let header_ptr = runner.bus.alloc(24);
    let buf0_ptr = runner.bus.alloc(16);
    let buf1_ptr = runner.bus.alloc(16);

    runner.bus.write_word(callback_addr, 0x4E75); // RTS without popping args.
    for offset in (0..512).step_by(2) {
        runner.bus.write_word(interrupted_pc + offset, 0x4E71); // NOP
    }
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.bus.write_long(header_ptr + 12, buf0_ptr);
    runner.bus.write_long(header_ptr + 16, buf1_ptr);
    runner.bus.write_long(buf0_ptr + 4, 0x0000_0001);
    runner.bus.write_long(buf1_ptr + 4, 0x0000_0001);
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 0,
        });
    runner
        .dispatcher
        .sound_manager
        .queue_doubleback_callback(PendingDoubleBackCallback {
            callback_addr,
            chan_ptr: 0x0039_38C8,
            header_ptr,
            exhausted_buffer_index: 1,
        });

    let (_steps, running) = runner.run_steps(96, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks.len(),
        1,
        "one CPU slice must not drain back-to-back doubleback interrupts"
    );
    assert_eq!(
        runner.dispatcher.sound_manager.pending_callbacks[0].exhausted_buffer_index,
        1
    );

    let (_steps, running) = runner.run_steps(96, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(
        runner.dispatcher.sound_manager.pending_callbacks.is_empty(),
        "the next CPU slice may dispatch the next pending doubleback"
    );
}

#[test]
fn vbl_callback_arms_interrupt_with_task_ptr_in_a0() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let task_ptr = 0x0020_2000;

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::A0, 0xAAAA_0000);
    runner.m68k.cpu.core.set_ccr(0x04);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2004);

    runner.bus.write_word(task_ptr + 4, 1); // qType = vType
    runner.bus.write_long(task_ptr + 6, 0x0004_1234); // vblAddr
    runner.bus.write_word(task_ptr + 10, 1); // vblCount
    runner.bus.write_word(task_ptr + 12, 0); // vblPhase
    runner.dispatcher.vbl_tasks.push(VblTask {
        task_ptr,
        architecture: CallbackTaskArchitecture::M68k,
        slot: Some(9),
        pending: false,
    });

    runner.fire_vbl_tasks();

    let active = runner
        .active_interrupt_callback
        .expect("vbl callback should have been armed");
    assert!(matches!(active.source, ActiveInterruptCallbackSource::Vbl));
    assert_eq!(active.resume_pc, interrupted_pc);
    assert_eq!(active.resume_sp, interrupted_sp);
    assert_eq!(active.sr, 0x2004);
    assert_eq!(active.ccr, 0x04);
    assert_eq!(runner.bus.read_word(task_ptr + 10), 0);

    assert_ne!(runner.vbl_trampoline, 0);
    assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2104);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        runner.vbl_trampoline
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp - 4);
    assert_eq!(runner.bus.read_long(interrupted_sp - 4), interrupted_pc);
    assert_eq!(runner.bus.read_word(runner.vbl_trampoline + 4), 0x207C);
    assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), task_ptr);
    assert_eq!(
        runner.bus.read_long(runner.vbl_trampoline + 12),
        0x0004_1234
    );
}

#[test]
fn simultaneous_vbl_callbacks_do_not_starve_later_queue_elements() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let first_ptr = 0x0020_2000;
    let second_ptr = 0x0020_2020;

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    for (task_ptr, callback) in [(first_ptr, 0x0004_1234), (second_ptr, 0x0004_5678)] {
        runner.bus.write_word(task_ptr + 4, 1);
        runner.bus.write_long(task_ptr + 6, callback);
        runner.bus.write_word(task_ptr + 10, 1);
        runner.dispatcher.vbl_tasks.push(VblTask {
            task_ptr,
            architecture: CallbackTaskArchitecture::M68k,
            slot: None,
            pending: false,
        });
    }

    runner.fire_vbl_tasks();
    assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), first_ptr);
    assert!(runner.dispatcher.vbl_tasks[1].pending);

    // Model the first callback rescheduling itself every retrace. The
    // already-due second element must run before the first one can run
    // again.
    runner.bus.write_word(first_ptr + 10, 1);
    runner.active_interrupt_callback = None;
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    runner.fire_vbl_tasks();

    assert_eq!(runner.bus.read_long(runner.vbl_trampoline + 6), second_ptr);
    assert!(runner.dispatcher.vbl_tasks[0].pending);
    assert!(!runner.dispatcher.vbl_tasks[1].pending);
}

#[test]
fn vbl_callback_defers_while_processor_priority_masks_level_one() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let task_ptr = 0x0020_2000;

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2100);

    runner.bus.write_word(task_ptr + 4, 1);
    runner.bus.write_long(task_ptr + 6, 0x0004_1234);
    runner.bus.write_word(task_ptr + 10, 1);
    runner.bus.write_word(task_ptr + 12, 0);
    runner.dispatcher.vbl_tasks.push(VblTask {
        task_ptr,
        architecture: CallbackTaskArchitecture::M68k,
        slot: None,
        pending: false,
    });

    runner.fire_vbl_tasks();

    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.bus.read_word(task_ptr + 10), 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
}

#[test]
fn classic_sound_callback_router_leaves_powerpc_completion_pending() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.dispatcher.sound_manager.queue_sound_callback(
        crate::sound::PendingSoundCallback::FileCompletion {
            architecture: CallbackTaskArchitecture::PowerPc,
            callback_addr: 0x0050_1000,
            chan_ptr: 0x0050_2000,
        },
    );

    assert!(!runner.fire_sound_callbacks());
    assert!(!runner.has_pending_sound_work());
    let (steps, running) = runner.run_pending_sound_work(8);
    assert_eq!(steps, 0);
    assert!(running);
    assert!(matches!(
        runner
            .dispatcher
            .sound_manager
            .pending_sound_callbacks
            .as_slice(),
        [crate::sound::PendingSoundCallback::FileCompletion {
            architecture: CallbackTaskArchitecture::PowerPc,
            callback_addr: 0x0050_1000,
            chan_ptr: 0x0050_2000,
        }]
    ));
}

#[test]
fn vbl_callback_restores_foreground_sr_after_return() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0002_0000;
    let interrupted_sp = 0x007F_FFC0;
    let task_ptr = 0x0020_2000;
    let callback_addr = 0x0004_1234;

    runner.bus.write_word(interrupted_pc, 0x4E71); // foreground NOP
    runner.bus.write_word(callback_addr, 0x4E75); // VBL callback RTS
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2004);

    runner.bus.write_word(task_ptr + 4, 1);
    runner.bus.write_long(task_ptr + 6, callback_addr);
    runner.bus.write_word(task_ptr + 10, 1);
    runner.bus.write_word(task_ptr + 12, 0);
    runner.dispatcher.vbl_tasks.push(VblTask {
        task_ptr,
        architecture: CallbackTaskArchitecture::M68k,
        slot: None,
        pending: false,
    });

    runner.fire_vbl_tasks();
    assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2104);

    let (_steps, running) = runner.run_steps(8, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.core.get_sr(), 0x2004);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
}

#[test]
fn shipped_host_execution_policy_preserves_public_defaults() {
    let runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    assert_eq!(runner.instructions_per_tick(), 12_000);
    assert_eq!(DEFAULT_VBL_HZ, 60.15);
    assert_eq!(DEFAULT_REALTIME_CPU_MHZ, 25.0);
    assert_eq!(DEFAULT_REALTIME_PPC_CPU_MHZ, 120.0);
    assert_eq!(DEFAULT_REALTIME_INSTRUCTIONS_PER_SECOND, 25_000_000.0);
    assert_eq!(default_realtime_instructions_per_tick(false), 415_628);
    assert_eq!(default_realtime_instructions_per_tick(true), 1_995_012);
}

#[test]
fn host_pacing_override_preserves_m68k_guest_profile_and_canonical_ticks() {
    const SYS_ENV: u32 = 0x0030_0000;
    const PROGRAM: u32 = 0x0001_0000;

    fn guest_profile(runner: &mut FixtureRunner) -> ([u32; 5], [u16; 3], u8, u32) {
        let mut gestalt = [0; 5];
        for (index, selector) in [*b"sysa", *b"cput", *b"proc", *b"fpu ", *b"mmu "]
            .into_iter()
            .enumerate()
        {
            runner
                .m68k
                .cpu
                .write_reg(Register::D0, u32::from_be_bytes(selector));
            runner
                .dispatcher
                .dispatch(0xA1AD, &mut runner.m68k.cpu, &mut runner.bus)
                .unwrap();
            gestalt[index] = runner.m68k.cpu.read_reg(Register::A0);
        }

        runner.m68k.cpu.write_reg(Register::A0, SYS_ENV);
        runner.m68k.cpu.write_reg(Register::D0, 2);
        runner
            .dispatcher
            .dispatch(0xA090, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        let sys_environs = [
            runner.bus.read_word(SYS_ENV + 2),
            runner.bus.read_word(SYS_ENV + 4),
            runner.bus.read_word(SYS_ENV + 6),
        ];
        let has_fpu = runner.bus.read_byte(SYS_ENV + 8);

        runner.m68k.cpu.write_reg(Register::D0, u32::MAX);
        runner
            .dispatcher
            .dispatch(0xA485, &mut runner.m68k.cpu, &mut runner.bus)
            .unwrap();
        let cpu_speed = runner.m68k.cpu.read_reg(Register::D0);
        (gestalt, sys_environs, has_fpu, cpu_speed)
    }

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let default_guest_profile = guest_profile(&mut runner);
    assert_eq!(
        default_guest_profile,
        ([1, 4, 5, 3, 4], [20, 0x0810, 5], 1, 25)
    );

    runner.set_instructions_per_tick(3);
    assert_eq!(guest_profile(&mut runner), default_guest_profile);

    for offset in (0..14).step_by(2) {
        runner.bus.write_word(PROGRAM + offset, 0x4E71);
    }
    runner.m68k.cpu.write_reg(Register::PC, PROGRAM);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.set_guest_tick_for_test(0);
    runner
        .bus
        .write_long(crate::memory::globals::addr::TICKS, 500);
    let tick_result = runner.m68k.cpu.read_reg(Register::A7);
    runner.bus.write_long(tick_result, 0);
    runner
        .dispatcher
        .dispatch(0xA975, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    assert_eq!(runner.bus.read_long(tick_result), 500);

    let (steps, running) = runner.run_steps(7, None);

    assert!(running);
    assert_eq!(steps, 7);
    assert_eq!(
        runner.bus.read_long(crate::memory::globals::addr::TICKS),
        502
    );
    runner.bus.write_long(tick_result, 0);
    runner
        .dispatcher
        .dispatch(0xA975, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    assert_eq!(runner.bus.read_long(tick_result), 502);
}

#[test]
fn host_pacing_override_preserves_powerpc_guest_profile_and_tick_visibility() {
    use crate::loader::ppc::tests::synthetic_pef_with_import;

    const RESPONSE: u32 = PPC_HEAP_BASE + 0x1000;
    const SYS_ENV: u32 = RESPONSE + 0x100;

    fn guest_state(runner: &mut FixtureRunner) -> ([(u32, u32); 5], [u16; 4], [u8; 2], u32) {
        let mut context = runner
            .native
            .take(NativeEngineRole::Companion)
            .expect("PPC companion installed");
        let native = context.adapter_mut();
        let mut capabilities = [(0, 0); 5];

        native.cpu.pc = native.imports[0].trap_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::TickCount;
        let probe = runner
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| {
                native.run_with_process_services(64, false, false, memory_manager, cfm)
            });
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        let tick_count = native.cpu.gpr[3];

        for (index, selector) in [*b"cput", *b"proc", *b"fpu ", *b"mmu ", *b"sysa"]
            .into_iter()
            .enumerate()
        {
            native.cpu.pc = native.imports[0].trap_pc;
            native.cpu.lr = PPC_HALT_PC;
            native.imports[0].dispatcher_target = PpcImportDispatcherTarget::Gestalt;
            native.cpu.gpr[3] = u32::from_be_bytes(selector);
            native.cpu.gpr[4] = RESPONSE;
            let probe = runner
                .process_context
                .with_memory_and_cfm(|memory_manager, cfm| {
                    native.run_with_process_services(64, false, false, memory_manager, cfm)
                });
            assert_eq!(probe.handled_import_count, 1);
            assert_eq!(probe.unsupported_import_index, None);
            capabilities[index] = (
                native.cpu.gpr[3],
                native.memory.read_u32_be(RESPONSE).unwrap(),
            );
        }

        native.cpu.pc = native.imports[0].trap_pc;
        native.cpu.lr = PPC_HALT_PC;
        native.imports[0].dispatcher_target = PpcImportDispatcherTarget::SysEnvirons;
        native.cpu.gpr[3] = 2;
        native.cpu.gpr[4] = SYS_ENV;
        let probe = runner
            .process_context
            .with_memory_and_cfm(|memory_manager, cfm| {
                native.run_with_process_services(64, false, false, memory_manager, cfm)
            });
        assert_eq!(probe.handled_import_count, 1);
        assert_eq!(probe.unsupported_import_index, None);
        assert_eq!(native.cpu.gpr[3], 0);
        let sys_environs = [
            native.memory.read_u16_be(SYS_ENV).unwrap(),
            native.memory.read_u16_be(SYS_ENV + 2).unwrap(),
            native.memory.read_u16_be(SYS_ENV + 4).unwrap(),
            native.memory.read_u16_be(SYS_ENV + 6).unwrap(),
        ];
        let sys_environs_flags = [
            native.memory.read_u8(SYS_ENV + 8).unwrap(),
            native.memory.read_u8(SYS_ENV + 9).unwrap(),
        ];

        assert!(runner.native.restore(context).is_ok());
        (capabilities, sys_environs, sys_environs_flags, tick_count)
    }

    let mut native = load_pef_application(&synthetic_pef_with_import(b"Gestalt")).unwrap();
    native.memory.add_region(RESPONSE, vec![0; 0x110]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.init_ppc_companion(native);
    runner
        .bus
        .write_long(crate::memory::globals::addr::TICKS, 700);

    let default_guest_state = guest_state(&mut runner);
    assert_eq!(
        default_guest_state,
        (
            [(0, 0x0104), (0, 2), (0, 3), (0, 4), ((-5551i32) as u32, 0)],
            [2, 20, 0x0810, 5],
            [1, 1],
            700,
        )
    );

    runner.set_instructions_per_tick(7);
    runner
        .bus
        .write_long(crate::memory::globals::addr::TICKS, 900);
    let paced_guest_state = guest_state(&mut runner);
    assert_eq!(paced_guest_state.0, default_guest_state.0);
    assert_eq!(paced_guest_state.1, default_guest_state.1);
    assert_eq!(paced_guest_state.2, default_guest_state.2);
    assert_eq!(paced_guest_state.3, 900);
}

#[test]
fn custom_instructions_per_tick_controls_tick_cadence() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let program_words = 14;

    for offset in (0..program_words).step_by(2) {
        runner.bus.write_word(program_start + offset, 0x4E71);
    }

    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.set_instructions_per_tick(3);

    let (steps, running) = runner.run_steps(7, None);

    assert!(running);
    assert_eq!(steps, 7);
    assert_eq!(runner.bus.read_long(0x016A), 2);
}

#[test]
fn non_idle_hle_trap_cost_advances_tick_budget() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    let sp = 0x0010_0000u32;
    let rect = 0x0020_0000u32;

    runner.bus.write_word(base, 0xA8A8); // _OffsetRect
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_word(sp, 1); // dv
    runner.bus.write_word(sp + 2, 2); // dh
    runner.bus.write_long(sp + 4, rect);
    runner.bus.write_word(rect, 10);
    runner.bus.write_word(rect + 2, 20);
    runner.bus.write_word(rect + 4, 30);
    runner.bus.write_word(rect + 6, 40);
    runner.bus.write_long(0x016A, 0);
    runner.set_guest_tick_for_test(0);
    runner.set_instructions_per_tick(5);

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(
        runner.guest_tick(),
        1,
        "non-idle HLE traps should consume tick budget beyond the base instruction"
    );
    assert_eq!(runner.bus.read_word(rect), 11);
    assert_eq!(runner.bus.read_word(rect + 2), 22);
}

#[test]
fn idle_hle_traps_do_not_apply_extra_tick_cost() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;

    runner.bus.write_word(base, 0xA975); // _TickCount
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
    runner.bus.write_long(0x016A, 42);
    runner.set_guest_tick_for_test(42);
    runner.set_instructions_per_tick(5);

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(
        runner.guest_tick(),
        42,
        "polling traps should not add synthetic HLE manager cost"
    );
    assert_eq!(runner.tick_budget, 4);
}

#[test]
fn hle_trap_cost_stops_gui_slice_at_tick_cap() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    let sp = 0x0010_0000u32;
    let rect = 0x0020_0000u32;

    runner.bus.write_word(base, 0xA8A8); // _OffsetRect
    runner.bus.write_word(base + 2, 0x4E71); // NOP that must wait for the next GUI slice
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_word(sp, 1);
    runner.bus.write_word(sp + 2, 2);
    runner.bus.write_long(sp + 4, rect);
    runner.bus.write_word(rect, 10);
    runner.bus.write_word(rect + 2, 20);
    runner.bus.write_word(rect + 4, 30);
    runner.bus.write_word(rect + 6, 40);
    runner.bus.write_long(0x016A, 0);
    runner.set_guest_tick_for_test(0);
    runner.set_instructions_per_tick(5);

    let (steps, running) = runner.run_gui_slice_with_audio(8, 1, 0);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(runner.guest_tick(), 1);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        base + 2,
        "the next guest instruction should be deferred once HLE cost reaches the GUI tick cap"
    );
}

/// Regression gate for the guest-owned TickCount invariant.
/// `advance_guest_tick` and the unfreeze path update low-memory `$016A`;
/// all semantic readers import those bytes before using host pacing state.
/// Any future path that bypasses that import can desynchronize double-
/// click detection, the TickCount handler, and diagnostic tick printouts.
#[test]
fn dispatcher_tick_count_stays_in_sync_with_bus() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let program_words = 20;

    // NOPs keep the CPU stepping without producing traps that
    // could interfere with tick accounting.
    for offset in (0..program_words).step_by(2) {
        runner.bus.write_word(program_start + offset, 0x4E71);
    }

    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    // Set both sides of the invariant to the same initial value.
    runner.set_guest_tick_for_test(0);
    runner.set_instructions_per_tick(3);

    // Step a few times; ticks should advance roughly every 3
    // instructions. After each run_steps, bus and dispatcher
    // must agree.
    for _ in 0..3 {
        let (_, running) = runner.run_steps(3, None);
        assert!(running);
        assert_eq!(
            runner.bus.read_long(0x016A),
            runner.guest_tick(),
            "guest low-memory Ticks ({}) diverged from semantic reader ({})",
            runner.bus.read_long(0x016A),
            runner.guest_tick(),
        );
    }
}

mod idle;

#[test]
fn tickcount_runner_uses_canonical_dispatch_and_accounting() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    // Plain TickCount call: SUBQ.W #4, A7 ; _TickCount ; NOP
    runner.bus.write_word(base, 0x594F); // SUBQ.W #4, A7 (reserve LONGINT slot)
    runner.bus.write_word(base + 2, 0xA975); // _TickCount
    runner.bus.write_word(base + 4, 0x4E71); // NOP (sentinel)
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);
    runner.set_guest_tick_for_test(0x1234_5678);
    runner.bus.write_long(0x016A, 0x1234_5678);
    runner.set_instructions_per_tick(1_000_000);

    let before_traps = runner.dispatcher.trap_count;
    // Two steps: the SUBQ first, then canonical trap dispatch.
    let (steps, running) = runner.run_steps(2, None);
    assert!(
        running,
        "runner should not halt on a canonical TickCount trap"
    );
    assert_eq!(steps, 2);
    assert_eq!(runner.bus.read_long(0x000F_FFFC), 0x1234_5678);
    assert_eq!(runner.dispatcher.trap_count - before_traps, 1);
}

#[test]
fn halted_by_exit_to_shell_classifies_clean_application_quit() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    runner.bus.write_word(base, 0xA9F4); // _ExitToShell
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

    let (_steps, running) = runner.run_steps(1, None);

    assert!(!running, "ExitToShell should stop the runner");
    assert!(runner.is_halted());
    assert_eq!(runner.halted_trap(), Some(0xA9F4));
    assert!(
        runner.halted_by_exit_to_shell(),
        "ExitToShell halt must be classified as a clean application exit"
    );
}

#[test]
fn unimplemented_trap_halts_at_the_faulting_instruction() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    let sp = 0x0010_0000u32;
    runner.bus.write_word(base, 0xAFFE);
    runner.bus.write_word(base + 2, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, sp);

    let (steps, running) = runner.run_steps(1, None);

    assert_eq!(steps, 1);
    assert!(
        !running,
        "an unclassified HLE row must fail closed (pc=${:08X})",
        runner.m68k.cpu.read_reg(Register::PC)
    );
    assert!(runner.is_halted());
    assert_eq!(runner.halted_pc(), Some(base));
    assert_eq!(runner.halted_trap(), Some(0xAFFE));
    assert_eq!(runner.halted_sp(), Some(sp));
}

#[test]
fn exit_to_shell_activates_launch_target_queued_until_event_yield() {
    let helper_code0 = minimal_code0(0, 0x2000, 0, 0);
    let helper_fork_bytes = make_resource_fork_bytes(&[(*b"CODE", 0, &helper_code0)]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;

    runner
        .dispatcher
        .vfs
        .insert("Apps/Register Helper".to_string(), Vec::new());
    runner
        .dispatcher
        .vfs_rsrc
        .insert("Apps/Register Helper".to_string(), helper_fork_bytes);
    runner.dispatcher.ensure_vfs_catalog();
    runner
        .dispatcher
        .queue_pending_launch_application("Apps/Register Helper", true);
    runner.bus.write_word(base, 0xA9F4); // _ExitToShell
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

    let (_steps, running) = runner.run_steps(1, None);

    assert!(
        running,
        "ExitToShell should activate a valid queued launch target"
    );
    assert!(!runner.is_halted());
    assert_eq!(
        runner.dispatcher.launched_app_path(),
        Some("Apps/Register Helper")
    );
}

#[test]
fn exit_to_shell_launches_best_application_created_by_installer() {
    let app_code0 = minimal_code0(0, 0x2000, 0, 0);
    let app_fork = make_resource_fork_bytes(&[(*b"CODE", 0, &app_code0)]);
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;

    runner
        .dispatcher
        .vfs
        .insert("Existing/Previous Game".to_string(), Vec::new());
    runner
        .dispatcher
        .vfs_rsrc
        .insert("Existing/Previous Game".to_string(), app_fork.clone());
    runner.dispatcher.set_vfs_entry_finfo(
        "Existing/Previous Game",
        u32::from_be_bytes(*b"APPL"),
        u32::from_be_bytes(*b"GAME"),
        0,
    );
    runner.arm_installer_handoff();
    for path in ["Installed/Register", "Installed/Main Game"] {
        runner.dispatcher.vfs.insert(path.to_string(), Vec::new());
        runner
            .dispatcher
            .vfs_rsrc
            .insert(path.to_string(), app_fork.clone());
        runner.dispatcher.set_vfs_entry_finfo(
            path,
            u32::from_be_bytes(*b"APPL"),
            u32::from_be_bytes(*b"GAME"),
            0,
        );
    }
    runner.bus.write_word(base, 0xA9F4); // _ExitToShell
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

    let (_steps, running) = runner.run_steps(1, None);

    assert!(running, "installer exit should activate the installed game");
    assert!(!runner.is_halted());
    assert_eq!(
        runner.dispatcher.launched_app_path(),
        Some("Installed/Main Game")
    );
}

#[test]
fn halted_by_exit_to_shell_rejects_invalid_pc_halts() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner
        .m68k
        .cpu
        .write_reg(Register::PC, runner.bus.ram_size());
    runner.m68k.cpu.write_reg(Register::A7, 0x0010_0000);

    let (_steps, running) = runner.run_steps(1, None);

    assert!(!running, "invalid PC should stop the runner");
    assert!(runner.is_halted());
    assert_eq!(runner.halted_trap(), None);
    assert!(
        !runner.halted_by_exit_to_shell(),
        "invalid-PC halts must not be reported as clean application exits"
    );
}

#[test]
fn ptinrect_runner_dispatch_matches_pascal_stack_contract() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    let sp = 0x0010_0000u32;
    let rect = 0x0020_0000u32;

    runner.bus.write_word(base, 0xA8AD); // _PtInRect
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.set_instructions_per_tick(1_000_000);

    runner.bus.write_long(sp, rect);
    runner.bus.write_word(sp + 4, 20); // pt.v
    runner.bus.write_word(sp + 6, 30); // pt.h
    runner.bus.write_word(rect, 10); // top
    runner.bus.write_word(rect + 2, 25); // left
    runner.bus.write_word(rect + 4, 40); // bottom
    runner.bus.write_word(rect + 6, 50); // right

    let before_traps = runner.dispatcher.trap_count;
    let before_game = runner.dispatcher.game_trap_count;

    let (steps, running) = runner.run_steps(1, None);

    assert!(
        running,
        "runner should not halt on canonical PtInRect dispatch"
    );
    assert_eq!(steps, 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + 8);
    assert_eq!(runner.bus.read_word(sp + 8), 0x0100);
    assert_eq!(runner.dispatcher.trap_count - before_traps, 1);
    assert_eq!(runner.dispatcher.game_trap_count - before_game, 1);
}

#[test]
fn eventavail_runner_dispatch_peeks_without_dequeueing() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    let sp = 0x0010_0000u32;
    let event = 0x0020_0000u32;

    runner.bus.write_word(base, 0xA971); // _EventAvail
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.set_instructions_per_tick(1_000_000);
    runner.bus.write_long(sp, event);
    runner.bus.write_word(sp + 4, 0x0008); // keyDownMask
    runner.push_key_down(0x31, b' ');

    let before_traps = runner.dispatcher.trap_count;
    let before_game = runner.dispatcher.game_trap_count;

    let (steps, running) = runner.run_steps(1, None);

    assert!(
        running,
        "runner should not halt on canonical EventAvail dispatch"
    );
    assert_eq!(steps, 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), base + 2);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp + 6);
    assert_eq!(runner.bus.read_word(sp + 6), 0x0100);
    assert_eq!(runner.bus.read_word(event), 3);
    assert_eq!(
        runner.bus.read_long(event + 2),
        (0x31u32 << 8) | u32::from(b' ')
    );
    assert_eq!(
        runner.process_context.event_queue().len(),
        1,
        "EventAvail must not dequeue the matching event"
    );
    assert_eq!(runner.dispatcher.trap_count - before_traps, 1);
    assert_eq!(
        runner.dispatcher.game_trap_count, before_game,
        "EventAvail remains excluded from game_trap_count as an idle trap"
    );
}

#[test]
fn tick_progress_persists_across_multiple_run_slices() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let program_words = 12;

    for offset in (0..program_words).step_by(2) {
        runner.bus.write_word(program_start + offset, 0x4E71);
    }

    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.set_instructions_per_tick(5);

    let (steps1, running1) = runner.run_steps(3, None);
    let (steps2, running2) = runner.run_steps(3, None);

    assert!(running1);
    assert!(running2);
    assert_eq!(steps1, 3);
    assert_eq!(steps2, 3);
    assert_eq!(runner.bus.read_long(0x016A), 1);
}

#[test]
fn tick_override_breaks_once_target_tick_is_reached() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let program_words = 16;

    for offset in (0..program_words).step_by(2) {
        runner.bus.write_word(program_start + offset, 0x4E71);
    }

    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.set_instructions_per_tick(4);

    let (steps, running) = runner.run_steps_with_audio(16, Some(0), 0);

    assert!(running);
    assert_eq!(steps, 3);
    assert_eq!(runner.bus.read_long(0x016A), 0);
}

#[test]
fn pending_wait_sleep_ticks_advance_in_headless_mode() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 3;

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(runner.bus.read_long(0x016A), 3);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
}

#[test]
fn pending_wait_sleep_ticks_capped_to_zero_in_headless() {
    // `cap=Some(0)` is the scripted default — `WaitNextEvent`
    // sleep is treated as a zero-cost return (matching real Mac OS
    // where WNE doesn't directly tick; only the VBL hardware
    // interrupt does).
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.set_wait_sleep_cap_in_headless(Some(0));
    runner.dispatcher.pending_wait_sleep_ticks = 60;

    let (_steps, _running) = runner.run_steps(1, None);

    // Zero ticks advanced (cap=0).
    assert_eq!(runner.bus.read_long(0x016A), 0);
    // But pending sleep is cleared so the game resumes immediately.
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
}

#[test]
fn pending_wait_sleep_ticks_capped_in_headless_when_opt_in() {
    // Headless callers (e.g. scripted harnesses) can opt in to a
    // per-WNE-call sleep tick cap matching GUI mode, preventing
    // tick counts from racing ahead of real-Mac VBL pacing during
    // event-loop-heavy gameplay.
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.set_wait_sleep_cap_in_headless(Some(1));
    runner.dispatcher.pending_wait_sleep_ticks = 60;

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    // Only 1 tick advanced (cap), not the full 60.
    assert_eq!(runner.bus.read_long(0x016A), 1);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    assert_eq!(runner.wait_sleep_cap_in_headless(), Some(1));
}

#[test]
fn pending_wait_sleep_ticks_suspends_foreground_until_gui_tick_cap() {
    // In GUI mode (tick_override=Some), WNE sleep advances VBL/timer time
    // up to the current frame cap but keeps the foreground app suspended
    // until the requested sleep expires. This prevents sleep=60 loops from
    // receiving 60 null events per second. Inside Macintosh: Processes
    // 1994, p. 2-8.
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 60;

    let (steps, running) = runner.run_steps(1, Some(10));

    assert!(running);
    assert_eq!(
        steps, 0,
        "foreground code should not resume while WNE sleep remains pending"
    );
    assert_eq!(runner.bus.read_long(0x016A), 10);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
}

#[test]
fn pending_wait_sleep_ticks_wakes_wait_next_event_with_queued_input() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let event_ptr = 0x0020_0000;
    let result_ptr = 0x0020_0020;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.bus.write_word(result_ptr, 0);
    runner.dispatcher.set_sent_open_app_event_for_test(true);
    runner
        .dispatcher
        .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 60;
    runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
        event_ptr,
        result_ptr,
        event_mask: 0xFFFF,
        mouse_rgn: 0,
        resume_pc: None,
        resume_sp: None,
    });
    runner.push_mouse_down(123, 456);

    let (steps, running) = runner.run_steps(1, Some(10));

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(
        runner.bus.read_word(event_ptr),
        1,
        "queued mouseDown should replace the pending null EventRecord"
    );
    assert_eq!(runner.bus.read_word(event_ptr + 10), 123u16);
    assert_eq!(runner.bus.read_word(event_ptr + 12), 456u16);
    assert_eq!(
        runner.bus.read_word(result_ptr),
        0xFFFF,
        "WaitNextEvent result slot should be rewritten to TRUE"
    );
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
}

#[test]
fn push_mouse_down_wakes_pending_wait_next_event_immediately() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let event_ptr = 0x0020_0000;
    let result_ptr = 0x0020_0020;

    runner.bus.write_word(result_ptr, 0);
    runner.dispatcher.set_sent_open_app_event_for_test(true);
    runner
        .dispatcher
        .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 60;
    runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
        event_ptr,
        result_ptr,
        event_mask: 0xFFFF,
        mouse_rgn: 0,
        resume_pc: None,
        resume_sp: None,
    });

    runner.push_mouse_down(123, 456);

    assert_eq!(
        runner.bus.read_word(event_ptr),
        1,
        "input injection should wake a sleeping WaitNextEvent before the next CPU slice"
    );
    assert_eq!(runner.bus.read_word(event_ptr + 10), 123u16);
    assert_eq!(runner.bus.read_word(event_ptr + 12), 456u16);
    assert_eq!(runner.bus.read_word(result_ptr), 0xFFFF);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
}

#[test]
fn set_mouse_position_wakes_pending_wait_next_event_with_mouse_moved_region() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let event_ptr = 0x0020_0000;
    let result_ptr = 0x0020_0020;
    let mouse_rgn = test_region_handle(&mut runner.bus, 10, 20, 30, 40);

    runner.set_mouse_position(20, 25);
    runner.bus.write_word(result_ptr, 0);
    runner.dispatcher.set_sent_open_app_event_for_test(true);
    runner
        .dispatcher
        .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 60;
    runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
        event_ptr,
        result_ptr,
        event_mask: 0x8000,
        mouse_rgn,
        resume_pc: None,
        resume_sp: None,
    });

    runner.set_mouse_position(50, 25);

    assert_eq!(
        runner.bus.read_word(event_ptr),
        15,
        "mouse movement outside the pending mouseRgn should wake WaitNextEvent with osEvt"
    );
    assert_eq!(runner.bus.read_long(event_ptr + 2), 0xFA00_0000);
    assert_eq!(runner.bus.read_word(event_ptr + 10), 50u16);
    assert_eq!(runner.bus.read_word(event_ptr + 12), 25u16);
    assert_eq!(runner.bus.read_word(result_ptr), 0xFFFF);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
    assert_eq!(
        runner.dispatcher.debug_mouse_moved_event_count, 1,
        "async wake path should share the normal mouse-moved event accounting"
    );
}

#[test]
fn set_mouse_position_leaves_pending_wait_next_event_asleep_without_event() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let event_ptr = 0x0020_0000;
    let result_ptr = 0x0020_0020;

    runner.bus.write_word(program_start, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 100);
    runner.set_guest_tick_for_test(100);
    runner.tick_budget = 0;
    runner.bus.write_word(result_ptr, 0xFFFF);
    runner.dispatcher.set_sent_open_app_event_for_test(true);
    runner.dispatcher.write_event_record(
        &mut runner.bus,
        event_ptr,
        0xFFFF,
        0xABCD_EF01,
        0,
        1,
        2,
        3,
    );
    runner.dispatcher.pending_wait_sleep_ticks = 60;
    runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
        event_ptr,
        result_ptr,
        event_mask: 0xFFFF,
        mouse_rgn: 0,
        resume_pc: None,
        resume_sp: None,
    });

    runner.set_mouse_position(123, 456);

    assert_eq!(
        runner.bus.read_word(event_ptr),
        0xFFFF,
        "mouse movement with no mouseRgn event should not rewrite the parked event record"
    );
    assert_eq!(runner.bus.read_long(event_ptr + 2), 0xABCD_EF01);
    assert_eq!(runner.bus.read_word(event_ptr + 10), 1);
    assert_eq!(runner.bus.read_word(event_ptr + 12), 2);
    assert_eq!(
        runner.bus.read_word(result_ptr),
        0xFFFF,
        "the pending WaitNextEvent result must remain untouched"
    );
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 60);
    assert!(runner.dispatcher.pending_wait_next_event_return.is_some());
    assert_eq!(runner.bus.read_word(0x0828), 123u16);
    assert_eq!(runner.bus.read_word(0x082A), 456u16);

    let (steps, running) = runner.run_steps(1, Some(110));
    assert!(running);
    assert_eq!(
        steps, 0,
        "foreground code must remain suspended while WaitNextEvent is asleep"
    );
    assert_eq!(
        runner.bus.read_long(0x016A),
        110,
        "the original WaitNextEvent sleep should continue toward expiry"
    );
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
}

#[test]
fn push_mouse_down_leaves_pending_wait_next_event_parked_during_interrupt_callback() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let event_ptr = 0x0020_0000;
    let result_ptr = 0x0020_0020;
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;

    runner.bus.write_word(result_ptr, 0);
    runner.dispatcher.set_sent_open_app_event_for_test(true);
    runner
        .dispatcher
        .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 60;
    runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
        event_ptr,
        result_ptr,
        event_mask: 0xFFFF,
        mouse_rgn: 0,
        resume_pc: None,
        resume_sp: None,
    });
    runner.active_interrupt_callback = Some(ActiveInterruptCallback {
        source: ActiveInterruptCallbackSource::Timer,
        resume_pc: interrupted_pc,
        resume_sp: interrupted_sp,
        d_regs: [0; 8],
        a_regs: [0, 0, 0, 0, 0, 0, 0, interrupted_sp],
        sr: 0x2000,
        ccr: 0,
        restore_port: None,
    });

    runner.push_mouse_down(123, 456);

    assert_eq!(
            runner.bus.read_word(event_ptr),
            0,
            "input must not rewrite a foreground WaitNextEvent record while an interrupt callback is active"
        );
    assert_eq!(runner.bus.read_word(result_ptr), 0);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 60);
    assert!(runner.dispatcher.pending_wait_next_event_return.is_some());
    assert!(
        runner
            .process_context
            .event_queue()
            .iter()
            .any(|event| event.what == 1 && event.where_v == 123 && event.where_h == 456),
        "the mouseDown should remain queued for the foreground event loop"
    );
}

#[test]
fn pending_wait_next_event_drops_stale_return_after_foreground_moves_on() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let parked_pc = 0x0001_0000;
    let stale_pc = 0x0001_0010;
    let parked_sp = 0x007F_FFC0;
    let event_ptr = 0x0020_0000;
    let result_ptr = 0x0020_0020;

    runner.bus.write_word(stale_pc, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, stale_pc);
    runner.m68k.cpu.write_reg(Register::A7, parked_sp);
    runner.bus.write_word(result_ptr, 0xA582);
    runner.dispatcher.set_sent_open_app_event_for_test(true);
    runner
        .dispatcher
        .write_event_record(&mut runner.bus, event_ptr, 0, 0, 0, 0, 0, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 60;
    runner.dispatcher.pending_wait_next_event_return = Some(PendingWaitNextEventReturn {
        event_ptr,
        result_ptr,
        event_mask: 0xFFFF,
        mouse_rgn: 0,
        resume_pc: Some(parked_pc),
        resume_sp: Some(parked_sp),
    });
    runner.push_mouse_down(123, 456);

    let (steps, running) = runner.run_steps(1, Some(10));

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(
        runner.bus.read_word(result_ptr),
        0xA582,
        "a stale WaitNextEvent return slot may now belong to a caller frame"
    );
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
    assert!(runner.dispatcher.pending_wait_next_event_return.is_none());
    assert!(
        runner
            .process_context
            .event_queue()
            .iter()
            .any(|event| event.what == 1 && event.where_v == 123 && event.where_h == 456),
        "stale WNE cleanup should not silently consume a queued event"
    );
}

#[test]
fn push_mouse_down_restores_foreground_budget_before_next_tick_cap_run() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 100);
    runner.set_guest_tick_for_test(100);
    runner.tick_budget = 0;

    runner.push_mouse_down(123, 456);

    let (steps, running) = runner.run_steps(1, Some(110));

    assert!(running);
    assert_eq!(
        steps, 1,
        "input injected at an exhausted tick boundary should let foreground code run"
    );
    assert_eq!(
        runner.bus.read_long(0x016A),
        100,
        "foreground input wake must not spend the next slice only advancing ticks"
    );
}

#[test]
fn set_mouse_position_restores_foreground_budget_before_next_tick_cap_run() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 100);
    runner.set_guest_tick_for_test(100);
    runner.tick_budget = 0;

    runner.set_mouse_position(123, 456);

    let (steps, running) = runner.run_steps(1, Some(110));

    assert!(running);
    assert_eq!(
        steps, 1,
        "mouse movement at an exhausted tick boundary should let polling foreground code run"
    );
    assert_eq!(
        runner.bus.read_long(0x016A),
        100,
        "foreground mouse-move wake must not spend the next slice only advancing ticks"
    );
}

#[test]
fn pending_wait_sleep_ticks_honors_app_owned_visible_dialog_snapshot_in_gui_mode() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let dialog_ptr = 0x0020_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.dispatcher.dialog_visible_snapshots.insert(
        dialog_ptr,
        crate::trap::dispatch::PersistentDialogSnapshot {
            bounds: (10, 10, 40, 40),
            pixels: Vec::new().into(),
        },
    );
    runner.dispatcher.pending_wait_sleep_ticks = 60;

    let (steps, running) = runner.run_steps(1, Some(10));

    assert!(running);
    assert_eq!(
        steps, 0,
        "app-owned visible dialogs must not collapse WaitNextEvent sleep before ModalDialog"
    );
    assert_eq!(runner.bus.read_long(0x016A), 10);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 50);
}

#[test]
fn pending_wait_sleep_ticks_honors_app_owned_visible_dialog_snapshot_in_headless_cap_zero() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let dialog_ptr = 0x0020_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.set_wait_sleep_cap_in_headless(Some(0));
    runner.dispatcher.dialog_visible_snapshots.insert(
        dialog_ptr,
        crate::trap::dispatch::PersistentDialogSnapshot {
            bounds: (10, 10, 40, 40),
            pixels: Vec::new().into(),
        },
    );
    runner.dispatcher.pending_wait_sleep_ticks = 60;

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(
        steps, 1,
        "headless cap zero must not collapse app-owned dialog sleep"
    );
    assert_eq!(runner.bus.read_long(0x016A), 60);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
}

#[test]
fn pending_wait_sleep_ticks_collapses_retained_modaldialog_snapshot_in_gui_mode() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;
    let dialog_ptr = 0x0020_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.dispatcher.dialog_visible_snapshots.insert(
        dialog_ptr,
        crate::trap::dispatch::PersistentDialogSnapshot {
            bounds: (10, 10, 40, 40),
            pixels: Vec::new().into(),
        },
    );
    runner.dispatcher.dialog_modal_entered.insert(dialog_ptr);
    runner.dispatcher.pending_wait_sleep_ticks = 60;

    let (steps, running) = runner.run_steps(1, Some(10));

    assert!(running);
    assert_eq!(
        steps, 1,
        "retained ModalDialog snapshots keep the existing app-yield path"
    );
    assert_eq!(runner.bus.read_long(0x016A), 0);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
}

#[test]
fn pending_wait_sleep_ticks_resumes_when_gui_sleep_expires_before_cap() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.dispatcher.pending_wait_sleep_ticks = 3;

    let (steps, running) = runner.run_steps(1, Some(10));

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(runner.bus.read_long(0x016A), 3);
    assert_eq!(runner.dispatcher.pending_wait_sleep_ticks, 0);
}

#[test]
fn pending_delay_ticks_advance_in_gui_mode() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let program_start = 0x0001_0000;

    runner.bus.write_word(program_start, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, program_start);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 0);
    runner.dispatcher.pending_delay_ticks = 3;

    let (steps, _running) = runner.run_steps(1, Some(10));

    assert_eq!(steps, 1);
    assert_eq!(runner.bus.read_long(0x016A), 3);
    assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 3);
}

#[test]
fn dialog_callback_scratch_preserves_materialized_toolbox_trap_table() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let table_start = crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE;
    let table_end = table_start + u32::from(crate::trap::dispatch::TOOLBOX_TRAP_TABLE_SLOTS) * 4;
    let scratch_start = runner.dialog_callback_scratch_base();
    let scratch_end = scratch_start + DIALOG_CALLBACK_SCRATCH_SIZE;
    assert!(scratch_end <= table_start || scratch_start >= table_end);

    const SHOW_WINDOW: u16 = 0xA915;
    let show_window_entry = table_start + u32::from(SHOW_WINDOW & 0x03FF) * 4;
    let original_show_window = runner.bus.read_long(show_window_entry);
    assert_eq!(
        runner
            .dispatcher
            .native_trap_handler(&runner.bus, SHOW_WINDOW),
        None
    );
    let filter_proc = 0x0004_2000u32;
    runner.bus.write_word(filter_proc, 0x4E56);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));

    assert!(runner.fire_dialog_filter_proc());
    assert_eq!(
        runner.bus.read_long(show_window_entry),
        original_show_window
    );
    assert_eq!(
        runner
            .dispatcher
            .native_trap_handler(&runner.bus, SHOW_WINDOW),
        None
    );
}

#[test]
fn dialog_filter_synthesized_null_event_uses_live_modifiers() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let filter_proc = 0x0004_2000u32;

    runner.bus.write_word(filter_proc, 0x4E56);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.dispatcher.set_mouse_position(222, 333);
    runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
        dialog_ptr: 0x0020_0000,
        bounds: (100, 200, 200, 360),
        title: String::new(),
        proc_id: 2,
        items: Vec::new(),
        default_item: 1,
        cancel_item: 2,
        edit_text: String::new(),
        edit_item: 0,
        saved_pixels: Vec::new().into(),
        stack_ptr: 0x007F_FFC0,
        item_hit_ptr: 0x0030_0000,
        rendered_pixels: Vec::new().into(),
        flash_remaining: 0,
        flash_delay: 0,
        flash_item: 0,
        edit_text_modified: false,
        draw_proc_queue: std::collections::VecDeque::new(),
        draw_procs_done: true,
        rendered_pixels_final: true,
        filter_presentation_epoch: None,
        filter_proc,
        game_managed: true,
        last_filter_event: None,
        popup_draws: Vec::new(),
        active_popup: None,
        active_button: None,
        active_user_item: None,
    });

    assert!(runner.fire_dialog_filter_proc());
    let event_ptr = runner.dialog_filter_event;
    assert_eq!(runner.bus.read_word(event_ptr), 0);
    assert_eq!(runner.bus.read_word(event_ptr + 10), 222);
    assert_eq!(runner.bus.read_word(event_ptr + 12), 333);
    assert_eq!(
        runner.bus.read_word(event_ptr + 14),
        runner.dispatcher.current_event_modifiers()
    );
    assert_eq!(
        runner.dialog_filter_last_null_event_tick,
        Some((0x0020_0000, 0))
    );
}

#[test]
fn dialog_filter_uses_active_dialog_pending_update_before_null_event() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let filter_proc = 0x0004_2000u32;
    let dialog_ptr = runner.bus.alloc(170);

    runner.bus.write_word(filter_proc, 0x4E56);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        dialog_ptr,
        0,
        100,
        120,
        220,
        360,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    runner.process_context.shared_event_queue().clear();
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
    runner
        .dispatcher
        .dialog_tracking
        .as_mut()
        .unwrap()
        .dialog_ptr = dialog_ptr;

    assert!(runner.fire_dialog_filter_proc());
    let event_ptr = runner.dialog_filter_event;
    assert_eq!(runner.bus.read_word(event_ptr), 6);
    assert_eq!(runner.bus.read_long(event_ptr + 2), dialog_ptr);
    assert_eq!(runner.dialog_filter_last_null_event_tick, None);
}

#[test]
fn dialog_filter_paces_synthetic_update_without_starving_queued_input() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let filter_proc = 0x0004_2000u32;
    let dialog_ptr = runner.bus.alloc(170);

    runner.bus.write_word(filter_proc, 0x4E56);
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.bus.write_long(0x016A, 17);
    runner.set_guest_tick_for_test(17);
    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        dialog_ptr,
        0,
        100,
        120,
        220,
        360,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    runner.process_context.shared_event_queue().clear();
    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
    runner
        .dispatcher
        .dialog_tracking
        .as_mut()
        .unwrap()
        .dialog_ptr = dialog_ptr;

    assert!(runner.fire_dialog_filter_proc());
    let event_ptr = runner.dialog_filter_event;
    assert_eq!(runner.bus.read_word(event_ptr), 6);
    assert_eq!(runner.bus.read_long(event_ptr + 2), dialog_ptr);

    runner.active_interrupt_callback = None;
    runner.m68k.cpu.write_reg(Register::PC, 0x0001_0000);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner
        .dispatcher
        .dialog_tracking
        .as_mut()
        .unwrap()
        .last_filter_event = None;
    assert!(
        !runner.dialog_filter_has_real_event_pending(dialog_ptr),
        "the same invalid-region update should not refire indefinitely in one guest tick"
    );

    runner
        .process_context
        .shared_event_queue()
        .push_back(QueuedEvent {
            what: 1,
            message: 0,
            when: 0,
            where_v: 123,
            where_h: 234,
            modifiers: 0,
        });
    assert!(
        runner.dialog_filter_has_real_event_pending(dialog_ptr),
        "queued user input must bypass synthetic update pacing"
    );
    assert!(runner.fire_dialog_filter_proc());
    assert_eq!(runner.bus.read_word(event_ptr), 1);
    assert_eq!(runner.bus.read_word(event_ptr + 10), 123);
    assert_eq!(runner.bus.read_word(event_ptr + 12), 234);
    assert!(
        runner.process_context.event_queue().is_empty(),
        "the queued mouse event should be consumed by the filter call"
    );

    runner.active_interrupt_callback = None;
    runner
        .dispatcher
        .dialog_tracking
        .as_mut()
        .unwrap()
        .last_filter_event = None;
    runner.bus.write_long(0x016A, 18);
    runner.set_guest_tick_for_test(18);
    assert!(
        runner.dialog_filter_has_real_event_pending(dialog_ptr),
        "a still-invalid dialog can surface another update event on the next guest tick"
    );
}

#[test]
fn dialog_filter_proc_leaves_dialog_port_current() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let main_port = runner.bus.alloc(170);
    let dialog_ptr = runner.bus.alloc(170);

    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        main_port,
        0,
        0,
        0,
        600,
        800,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        dialog_ptr,
        0,
        120,
        180,
        240,
        420,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    runner.dispatcher.set_current_port_state(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        main_port,
        None,
    );
    let filter_proc = runner.bus.alloc(8);
    runner.bus.write_word(filter_proc, 0x4E56); // LINK A6, valid filter entry

    runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(filter_proc, 0x0030_0000));
    runner
        .dispatcher
        .dialog_tracking
        .as_mut()
        .unwrap()
        .dialog_ptr = dialog_ptr;

    assert!(runner.fire_dialog_filter_proc());
    assert_eq!(*runner.dispatcher.current_port, dialog_ptr);
    assert_eq!(
        runner
            .active_interrupt_callback
            .as_ref()
            .and_then(|callback| callback.restore_port),
        None
    );

    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    let (_steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(*runner.dispatcher.current_port, dialog_ptr);
    assert!(runner.active_interrupt_callback.is_none());
}

#[test]
fn nested_dialog_callbacks_restore_parent_trampoline_and_child_filter_result() {
    for child_filter in [false, true] {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let foreground = 0x0001_0000;
        let parent = 0x0004_2000;
        let child = 0x0004_3000;
        let busy = 0x0005_0000;
        let sp = 0x007F_FFC0;
        runner.bus.write_word(foreground, 0x60FE);
        runner.m68k.cpu.write_reg(Register::PC, foreground);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.bus.write_byte(busy, 1);
        // Parent draws until the test releases it, then returns normally.
        for (i, word) in [0x4E56, 0, 0x4A39, 5, 0, 0x66F8, 0x4E5E, 0x4E75]
            .into_iter()
            .enumerate()
        {
            runner.bus.write_word(parent + i as u32 * 2, word);
        }
        assert!(runner.inject_dialog_draw_proc(parent, 1, 0x0020_0000, false));
        runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
        let parent_sp = runner.m68k.cpu.read_reg(Register::A7);
        let parent_trampoline = runner.dialog_draw_trampoline;
        let parent_saved_sp = runner.bus.read_long(parent_trampoline + 22);
        let code = if child_filter {
            // Pascal Boolean TRUE at 20(A6); callee pops three pointers.
            vec![0x4E56, 0, 0x1D7C, 1, 20, 0x4E5E, 0x4E74, 12]
        } else {
            vec![0x4E56, 0, 0x4E5E, 0x4E75]
        };
        for (i, word) in code.into_iter().enumerate() {
            runner.bus.write_word(child + i as u32 * 2, word);
        }
        if child_filter {
            runner.dispatcher.dialog_tracking = Some(dialog_tracking_for_test(child, 0x0030_0000));
            assert!(runner.fire_dialog_filter_proc());
        } else {
            assert!(runner.inject_dialog_draw_proc(child, 2, 0x0020_1000, false));
        }
        assert_eq!(runner.nested_dialog_calls.len(), 1);
        runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
        assert!(runner.nested_dialog_calls.is_empty());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), parent_sp);
        assert_eq!(
            runner.bus.read_long(parent_trampoline + 22),
            parent_saved_sp
        );
        if child_filter {
            assert_eq!(
                runner
                    .bus
                    .read_word(runner.dispatcher.dialog_filter_result_addr)
                    & 0x0100,
                0x0100
            );
        }
        runner.bus.write_byte(busy, 0);
        runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
        assert!(runner.active_interrupt_callback.is_none());
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    }
}

#[test]
fn sound_completion_interrupts_and_resumes_a_waiting_dialog_filter() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let foreground = 0x0001_0000;
    let filter = 0x0004_2000;
    let callback = 0x0004_3000;
    let busy = 0x0005_0000;
    let finished = busy + 1;
    let sp = 0x007F_FFC0;
    runner.bus.write_word(foreground, 0x60FE);
    runner.m68k.cpu.write_reg(Register::PC, foreground);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.bus.write_byte(busy, 1);
    // LINK; wait: TST.B busy; BNE wait; ST finished; UNLK; RTD #12.
    let code = [
        0x4E56, 0, 0x4A39, 5, 0, 0x66F8, 0x50F9, 5, 1, 0x4E5E, 0x4E74, 12,
    ];
    for (i, word) in code.into_iter().enumerate() {
        runner.bus.write_word(filter + i as u32 * 2, word);
    }
    // Sound completion: CLR.B busy; RTS.
    for (i, word) in [0x4239, 5, 0, 0x4E75].into_iter().enumerate() {
        runner.bus.write_word(callback + i as u32 * 2, word);
    }
    let mut tracking = dialog_tracking_for_test(filter, 0x0030_0000);
    tracking.dialog_ptr = 0x0020_0000;
    runner.dispatcher.dialog_tracking = Some(tracking);
    assert!(runner.fire_dialog_filter_proc());
    runner.run_gui_cpu_slice(100, runner.guest_tick() + 1);
    let paused_pc = runner.m68k.cpu.read_reg(Register::PC);
    let paused_sp = runner.m68k.cpu.read_reg(Register::A7);
    let tick = runner.guest_tick();
    runner
        .dispatcher
        .sound_manager
        .queue_sound_callback(PendingSoundCallback::Command {
            architecture: CallbackTaskArchitecture::M68k,
            callback_addr: callback,
            chan_ptr: 0x0039_38C8,
            cmd: SndCommand {
                cmd: crate::sound::cmd::CALLBACK,
                param1: 0,
                param2: 0,
            },
        });
    let (_, running) = runner.run_pending_sound_work(1000);
    assert!(running);
    assert_eq!(runner.bus.read_byte(busy), 0);
    assert_eq!(
        runner.bus.read_byte(finished),
        0,
        "audio service ran foreground code"
    );
    assert_eq!(runner.guest_tick(), tick);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), paused_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), paused_sp);
    assert!(matches!(
        runner.active_interrupt_callback.map(|c| c.source),
        Some(ActiveInterruptCallbackSource::DialogFilterProc)
    ));
    runner.run_gui_cpu_slice(100, tick + 1);
    assert_eq!(runner.bus.read_byte(finished), 0xFF);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(runner.suspended_dialog_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
}

#[test]
fn dialog_draw_callback_delay_respects_gui_deadlines_and_returns_final_ticks() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let foreground = 0x0001_0000;
    let callback = 0x0004_2000;
    let sp = 0x007F_FFC0;
    runner.bus.write_word(foreground, 0x60FE);
    runner.m68k.cpu.write_reg(Register::PC, foreground);
    runner.m68k.cpu.write_reg(Register::A7, sp);
    runner.set_instructions_per_tick(1_000_000);
    // LINK; MOVEA.L #2,A0; _Delay; MOVE.L D0,$50000; UNLK; RTS.
    for (i, word) in [
        0x4E56, 0, 0x207C, 0, 2, 0xA03B, 0x23C0, 5, 0, 0x4E5E, 0x4E75,
    ]
    .into_iter()
    .enumerate()
    {
        runner.bus.write_word(callback + i as u32 * 2, word);
    }
    assert!(runner.inject_dialog_draw_proc(callback, 1, 0x0020_0000, false));
    let tick = runner.guest_tick();
    runner.run_gui_cpu_slice(100, tick + 1);
    assert_eq!(runner.guest_tick(), tick + 1);
    assert_eq!(runner.dispatcher.pending_delay_ticks, 1);
    assert_eq!(runner.bus.read_long(0x0005_0000), 0);
    runner.run_gui_cpu_slice(100, tick + 2);
    runner.run_gui_cpu_slice(100, tick + 3);
    assert_eq!(runner.bus.read_long(0x0005_0000), tick + 2);
    assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
}

#[test]
fn dialog_callbacks_can_wait_for_ticks_across_gui_slices() {
    for filter in [false, true] {
        let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
        let foreground = 0x0001_0000;
        let proc_addr = 0x0004_2000;
        let sp = 0x007F_FFC0;
        runner.bus.write_word(foreground, 0x60FE); // BRA.S *
        runner.m68k.cpu.write_reg(Register::PC, foreground);
        runner.m68k.cpu.write_reg(Register::A7, sp);
        runner.instructions_per_tick = 32;
        runner.tick_budget = 32;
        // LINK A6,#0; MOVE.L Ticks,D0; wait: CMP.L Ticks,D0;
        // BEQ.S wait; UNLK A6; RTS (draw) / RTD #12 (filter).
        let code = [
            0x4E56,
            0,
            0x2038,
            0x016A,
            0xB0B8,
            0x016A,
            0x67FA,
            0x4E5E,
            if filter { 0x4E74 } else { 0x4E75 },
            12,
        ];
        for (i, word) in code.into_iter().enumerate() {
            runner.bus.write_word(proc_addr + i as u32 * 2, word);
        }
        if filter {
            let mut tracking = dialog_tracking_for_test(0, 0);
            tracking.dialog_ptr = 0x0020_0000;
            tracking.filter_proc = proc_addr;
            runner.dispatcher.dialog_tracking = Some(tracking);
            assert!(runner.fire_dialog_filter_proc());
        } else {
            assert!(runner.inject_dialog_draw_proc(proc_addr, 1, 0x0020_0000, false));
        }
        let tick = runner.guest_tick();
        let (_, running) = runner.run_gui_cpu_slice(500, tick + 1);
        assert!(running);
        assert_eq!(runner.guest_tick(), tick + 1, "filter={filter}");
        assert!(runner.active_interrupt_callback.is_some());
        let (_, running) = runner.run_gui_cpu_slice(500, tick + 2);
        assert!(running);
        assert!(
            runner.active_interrupt_callback.is_none(),
            "filter={filter}"
        );
        assert_eq!(runner.m68k.cpu.read_reg(Register::A7), sp);
    }
}

#[test]
fn dialog_draw_proc_trampoline_passes_item_first_and_tolerates_plain_rts() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let dialog_ptr = 0x0020_0000u32;
    let proc_addr = 0x0004_2000u32;
    let item_no = 5i16;

    // Keep foreground execution stable after the callback returns.
    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    // MPW-style proc prologue shape. It returns with plain RTS, leaving
    // callback parameters on the stack; the trampoline must restore A7.
    runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
    runner.bus.write_word(proc_addr + 2, 0x0000);
    runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
    runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS

    runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
        dialog_ptr,
        bounds: (0, 0, 64, 64),
        title: String::new(),
        proc_id: 1,
        items: Vec::new(),
        default_item: 0,
        cancel_item: 0,
        edit_text: String::new(),
        edit_item: 0,
        saved_pixels: Vec::new().into(),
        stack_ptr: interrupted_sp,
        item_hit_ptr: 0,
        rendered_pixels: Vec::new().into(),
        flash_remaining: 0,
        flash_delay: 0,
        flash_item: 0,
        edit_text_modified: false,
        draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
        draw_procs_done: false,
        rendered_pixels_final: false,
        filter_presentation_epoch: None,
        filter_proc: 0,
        game_managed: false,
        last_filter_event: None,
        popup_draws: Vec::new(),
        active_popup: None,
        active_button: None,
        active_user_item: None,
    });

    assert!(runner.fire_dialog_draw_procs());
    let tramp = runner.dialog_draw_trampoline;
    assert_eq!(runner.bus.read_word(tramp), 0x48E7);
    assert_eq!(runner.bus.read_word(tramp + 4), 0x2F3C);
    assert_eq!(runner.bus.read_long(tramp + 6), dialog_ptr);
    assert_eq!(runner.bus.read_word(tramp + 10), 0x3F3C);
    assert_eq!(runner.bus.read_word(tramp + 12), item_no as u16);
    assert_eq!(runner.bus.read_word(tramp + 14), 0x4EB9);
    assert_eq!(runner.bus.read_long(tramp + 16), proc_addr);
    assert_eq!(runner.bus.read_word(tramp + 20), 0x4FF9);
    assert_eq!(runner.bus.read_long(tramp + 22), interrupted_sp - 36);

    let (_steps, running) = runner.run_steps(16, None);

    assert!(running);
    assert!(
        runner.active_interrupt_callback.is_none(),
        "dialog callback should have resumed foreground code"
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
}

#[test]
fn modal_dialog_draw_procs_drain_before_foreground_code_resumes() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let proc_1 = 0x0004_2000u32;
    let proc_2 = 0x0004_2100u32;

    // Model an application loop that does not immediately call
    // ModalDialog again after the first injected callback returns.
    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    for proc_addr in [proc_1, proc_2] {
        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
    }

    let mut tracking = dialog_tracking_for_test(0, 0);
    tracking.draw_proc_queue = VecDeque::from([(proc_1, 3), (proc_2, 4)]);
    tracking.draw_procs_done = false;
    tracking.rendered_pixels_final = false;
    runner.dispatcher.dialog_tracking = Some(tracking);

    assert!(runner.fire_dialog_draw_procs());
    runner.deferred_tracking_refire_pc = Some(interrupted_pc + 2);
    let (_steps, running) = runner.run_steps(128, None);

    assert!(running);
    let tracking = runner.dispatcher.dialog_tracking.as_ref().unwrap();
    assert!(tracking.draw_proc_queue.is_empty());
    assert!(tracking.draw_procs_done);
    assert!(runner.active_interrupt_callback.is_none());
    assert!(runner.deferred_tracking_refire_pc.is_none());
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), interrupted_pc);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), interrupted_sp);
}

#[test]
fn modeless_dialog_draw_proc_accepts_a5_relative_proc_ptr() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let a5 = 0x0020_0000u32;
    let proc_offset = 0x0000_4200u32;
    let proc_addr = a5 + proc_offset;
    let dialog_ptr = runner.bus.alloc(170);

    runner.bus.write_word(interrupted_pc, 0x60FE);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.write_reg(Register::A5, a5);
    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        dialog_ptr,
        0,
        120,
        180,
        240,
        420,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
    runner.bus.write_word(proc_addr + 2, 0x0000);
    runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
    runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
    runner
        .dispatcher
        .modeless_dialog_draw_proc_queue
        .push_back((dialog_ptr, proc_offset, 5));

    assert!(runner.fire_modeless_dialog_draw_proc());

    let tramp = runner.dialog_draw_trampoline;
    assert_eq!(runner.bus.read_long(tramp + 16), proc_addr);
    assert_eq!(
        runner.dispatcher.active_modeless_dialog_draw_proc,
        Some(dialog_ptr)
    );
}

#[test]
fn modeless_dialog_draw_procs_drain_after_plain_trap() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let base = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let dialog_ptr = runner.bus.alloc(170);
    let proc_1 = 0x0004_2000u32;
    let proc_2 = 0x0004_2100u32;

    runner.bus.write_word(base, 0xA861); // _Random
    runner.bus.write_word(base + 2, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, base);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        dialog_ptr,
        0,
        120,
        180,
        240,
        420,
        "",
        2,
        true,
        false,
        false,
        0,
    );

    for proc_addr in [proc_1, proc_2] {
        runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
        runner.bus.write_word(proc_addr + 2, 0x0000);
        runner.bus.write_word(proc_addr + 4, 0x4E5E); // UNLK A6
        runner.bus.write_word(proc_addr + 6, 0x4E75); // RTS
    }
    runner.dispatcher.modeless_dialog_draw_proc_queue =
        VecDeque::from([(dialog_ptr, proc_1, 3), (dialog_ptr, proc_2, 5)]);

    let (_steps, running) = runner.run_steps(128, None);

    assert!(running);
    assert!(runner.dispatcher.modeless_dialog_draw_proc_queue.is_empty());
    assert_eq!(runner.dispatcher.active_modeless_dialog_draw_proc, None);
    assert!(
        runner.active_interrupt_callback.is_none(),
        "modeless draw callbacks should have returned to foreground code"
    );
}

#[test]
fn dialog_draw_proc_does_not_restore_over_guest_selected_dialog_port() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let main_port = runner.bus.alloc(170);
    let dialog_ptr = runner.bus.alloc(170);
    let proc_addr = 0x0004_2000u32;
    let item_no = 5i16;

    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        main_port,
        0,
        0,
        0,
        600,
        800,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    runner.dispatcher.init_cgraf_window(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        dialog_ptr,
        0,
        120,
        180,
        240,
        420,
        "",
        2,
        true,
        false,
        false,
        0,
    );
    runner.dispatcher.set_current_port_state(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        main_port,
        None,
    );

    runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
    runner.bus.write_word(proc_addr + 2, 0x0000);
    runner.bus.write_word(proc_addr + 4, 0x2F3C); // MOVE.L #dialog,-(SP)
    runner.bus.write_long(proc_addr + 6, dialog_ptr);
    runner.bus.write_word(proc_addr + 10, 0xA873); // _SetPort
    runner.bus.write_word(proc_addr + 12, 0x4E5E); // UNLK A6
    runner.bus.write_word(proc_addr + 14, 0x4E75); // RTS

    runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
        dialog_ptr,
        bounds: (120, 180, 240, 420),
        title: String::new(),
        proc_id: 1,
        items: Vec::new(),
        default_item: 0,
        cancel_item: 0,
        edit_text: String::new(),
        edit_item: 0,
        saved_pixels: Vec::new().into(),
        stack_ptr: interrupted_sp,
        item_hit_ptr: 0,
        rendered_pixels: Vec::new().into(),
        flash_remaining: 0,
        flash_delay: 0,
        flash_item: 0,
        edit_text_modified: false,
        draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
        draw_procs_done: false,
        rendered_pixels_final: false,
        filter_presentation_epoch: None,
        filter_proc: 0,
        game_managed: false,
        last_filter_event: None,
        popup_draws: Vec::new(),
        active_popup: None,
        active_button: None,
        active_user_item: None,
    });

    assert!(runner.fire_dialog_draw_procs());
    assert_eq!(
        runner
            .active_interrupt_callback
            .as_ref()
            .and_then(|callback| callback.restore_port),
        None
    );

    let (_steps, running) = runner.run_steps(32, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(
        *runner.dispatcher.current_port, dialog_ptr,
        "Dialog Manager must leave the dialog port current after the draw proc"
    );
}

#[test]
fn dialog_draw_proc_restores_parent_grafport_state_and_clip() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let dialog_ptr = runner.bus.alloc(170);
    let other_port = runner.bus.alloc(170);
    let proc_addr = 0x0004_2000u32;
    let clip_rect = 0x0004_2100u32;

    runner.bus.write_word(interrupted_pc, 0x60FE);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    for port in [dialog_ptr, other_port] {
        runner.dispatcher.init_cgraf_window(
            &mut runner.bus,
            &mut runner.m68k.cpu,
            port,
            0,
            120,
            180,
            240,
            420,
            "",
            2,
            true,
            false,
            false,
            0,
        );
    }
    runner.dispatcher.set_current_port_state(
        &mut runner.bus,
        &mut runner.m68k.cpu,
        dialog_ptr,
        None,
    );
    let clip_handle = runner.bus.read_long(dialog_ptr + 28);
    let clip_ptr = runner.bus.read_long(clip_handle);
    let expected_clip: Vec<u8> = (0..10)
        .map(|i| runner.bus.read_byte(clip_ptr + i))
        .collect();

    runner.bus.write_word(clip_rect, 1);
    runner.bus.write_word(clip_rect + 2, 2);
    runner.bus.write_word(clip_rect + 4, 3);
    runner.bus.write_word(clip_rect + 6, 4);
    let words = [
        0x4E56,
        0x0000, // LINK A6,#0
        0x3F3C,
        0x0007, // MOVE.W #7,-(SP), height
        0x3F3C,
        0x0006, // MOVE.W #6,-(SP), width
        0xA89B, // _PenSize
        0x3F3C,
        0x000C, // MOVE.W #12,-(SP)
        0xA89C, // _PenMode
        0x2F3C,
        (clip_rect >> 16) as u16,
        clip_rect as u16, // rect pointer
        0xA87B,           // _ClipRect
        0x2F3C,
        (other_port >> 16) as u16,
        other_port as u16, // port pointer
        0xA873,            // _SetPort
        0x4E5E,
        0x4E75, // UNLK; RTS
    ];
    for (i, word) in words.into_iter().enumerate() {
        runner.bus.write_word(proc_addr + i as u32 * 2, word);
    }
    runner.dispatcher.modeless_dialog_draw_proc_queue =
        VecDeque::from([(dialog_ptr, proc_addr, 5)]);

    assert!(runner.fire_modeless_dialog_draw_proc());
    let (_steps, running) = runner.run_steps(64, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(*runner.dispatcher.current_port, dialog_ptr);
    assert_eq!(runner.bus.read_word(dialog_ptr + 52), 1);
    assert_eq!(runner.bus.read_word(dialog_ptr + 54), 1);
    assert_eq!(runner.bus.read_word(dialog_ptr + 56), 8);
    assert_eq!(runner.bus.read_long(dialog_ptr + 28), clip_handle);
    let restored_clip_ptr = runner.bus.read_long(clip_handle);
    assert_eq!(
        (0..10)
            .map(|i| runner.bus.read_byte(restored_clip_ptr + i))
            .collect::<Vec<_>>(),
        expected_clip
    );
}

#[test]
fn dialog_draw_proc_pascal_stack_places_item_number_before_window_pointer() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000u32;
    let interrupted_sp = 0x007F_FFC0u32;
    let dialog_ptr = 0x0029_4240u32;
    let proc_addr = 0x0004_2000u32;
    let item_no = 2i16;
    let seen_item_addr = 0x0004_3000u32;
    let seen_dialog_addr = 0x0004_3004u32;

    runner.bus.write_word(interrupted_pc, 0x60FE); // BRA.S *-0
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);

    // PROCEDURE MyItem(theWindow: WindowPtr; itemNo: INTEGER);
    // Inside Macintosh Volume I, I-405. MPW Pascal prologues observe
    // itemNo at 8(A6) and theWindow at 10(A6).
    runner.bus.write_word(proc_addr, 0x4E56); // LINK A6,#0
    runner.bus.write_word(proc_addr + 2, 0x0000);
    runner.bus.write_word(proc_addr + 4, 0x302E); // MOVE.W 8(A6),D0
    runner.bus.write_word(proc_addr + 6, 0x0008);
    runner.bus.write_word(proc_addr + 8, 0x33C0); // MOVE.W D0,(abs).L
    runner.bus.write_long(proc_addr + 10, seen_item_addr);
    runner.bus.write_word(proc_addr + 14, 0x222E); // MOVE.L 10(A6),D1
    runner.bus.write_word(proc_addr + 16, 0x000A);
    runner.bus.write_word(proc_addr + 18, 0x23C1); // MOVE.L D1,(abs).L
    runner.bus.write_long(proc_addr + 20, seen_dialog_addr);
    runner.bus.write_word(proc_addr + 24, 0x4E5E); // UNLK A6
    runner.bus.write_word(proc_addr + 26, 0x4E75); // RTS

    runner.dispatcher.dialog_tracking = Some(crate::trap::dispatch::DialogTrackingState {
        dialog_ptr,
        bounds: (120, 180, 240, 420),
        title: String::new(),
        proc_id: 1,
        items: Vec::new(),
        default_item: 0,
        cancel_item: 0,
        edit_text: String::new(),
        edit_item: 0,
        saved_pixels: Vec::new().into(),
        stack_ptr: interrupted_sp,
        item_hit_ptr: 0,
        rendered_pixels: Vec::new().into(),
        flash_remaining: 0,
        flash_delay: 0,
        flash_item: 0,
        edit_text_modified: false,
        draw_proc_queue: VecDeque::from([(proc_addr, item_no)]),
        draw_procs_done: false,
        rendered_pixels_final: false,
        filter_presentation_epoch: None,
        filter_proc: 0,
        game_managed: false,
        last_filter_event: None,
        popup_draws: Vec::new(),
        active_popup: None,
        active_button: None,
        active_user_item: None,
    });

    assert!(runner.fire_dialog_draw_procs());
    let (_steps, running) = runner.run_steps(48, None);

    assert!(running);
    assert!(runner.active_interrupt_callback.is_none());
    assert_eq!(runner.bus.read_word(seen_item_addr) as i16, item_no);
    assert_eq!(runner.bus.read_long(seen_dialog_addr), dialog_ptr);
}

#[test]
fn pending_delay_ticks_fire_vbl_tasks_in_headless_mode() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let interrupted_pc = 0x0001_0000;
    let interrupted_sp = 0x007F_FFC0;
    let task_ptr = 0x0020_2000;

    runner.bus.write_word(interrupted_pc, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, interrupted_pc);
    runner.m68k.cpu.write_reg(Register::A7, interrupted_sp);
    runner.m68k.cpu.core.set_sr_noint_nosp(0x2000);
    runner.bus.write_long(0x016A, 0);
    runner.dispatcher.pending_delay_ticks = 1;

    runner.bus.write_word(task_ptr + 4, 1);
    runner.bus.write_long(task_ptr + 6, 0x0004_1234);
    runner.bus.write_word(task_ptr + 10, 1);
    runner.bus.write_word(task_ptr + 12, 0);
    runner.dispatcher.vbl_tasks.push(VblTask {
        task_ptr,
        architecture: CallbackTaskArchitecture::M68k,
        slot: None,
        pending: false,
    });

    let (steps, running) = runner.run_steps(1, None);

    assert!(running);
    assert_eq!(steps, 1);
    assert_eq!(runner.bus.read_long(0x016A), 1);
    assert_eq!(runner.dispatcher.pending_delay_ticks, 0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 1);
    assert!(matches!(
        runner.active_interrupt_callback,
        Some(ActiveInterruptCallback {
            source: ActiveInterruptCallbackSource::Vbl,
            ..
        })
    ));
}

/// `set_mouse_position` updates both the dispatcher's tracked
/// position and the six low-memory mouse globals (MTemp $0828,
/// RawMouse $082C, Mouse $0830) so guest code that polls them
/// directly sees the new coordinates without waiting for a click.
/// Inside Macintosh Volume II, II-371.
#[test]
fn set_mouse_position_updates_dispatcher_and_low_mem_globals() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.set_mouse_position(123, 456);

    assert_eq!(runner.dispatcher.input_state.mouse_position(), (123, 456));
    for off in [0x0828u32, 0x082C, 0x0830] {
        assert_eq!(runner.bus.read_word(off), 123u16, "v at ${:04X}", off);
        assert_eq!(runner.bus.read_word(off + 2), 456u16, "h at ${:04X}", off);
    }
}

#[test]
fn constructed_trap_tables_survive_companion_installation_and_observe_native_stores() {
    use crate::trap::dispatch::{OS_TRAP_TABLE_BASE, TOOLBOX_TRAP_TABLE_BASE};

    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let mut cells = Vec::new();
    for (base, count) in [(OS_TRAP_TABLE_BASE, 256), (TOOLBOX_TRAP_TABLE_BASE, 1024)] {
        for slot in 0..count {
            let cell = base + slot * 4;
            let handler = runner.bus.read_long(cell);
            assert_ne!(handler, 0);
            let instruction = runner.bus.read_word(handler);
            assert!(!runner.bus.try_write_word(handler, instruction ^ 0xFFFF));
            assert_eq!(runner.bus.read_word(handler), instruction);
            cells.push((cell, handler));
        }
    }
    let vectors = runner.dispatcher.trap_exception_vector_defaults.unwrap();
    let entry = TOOLBOX_TRAP_TABLE_BASE + 0x175 * 4; // TickCount
    let default = runner.bus.read_long(entry);
    let patch = 0x0010_1000;
    let program = 0x0010_0000;
    runner.bus.write_word(patch, 0x4E75); // RTS
    runner.bus.write_word(program, 0xA975); // TickCount
    runner.bus.write_word(program + 2, 0x4E71); // NOP

    // The companion joins an existing process; it must not replace its
    // guest-written table cells or exception vectors with launch defaults.
    runner.bus.write_long(entry, patch);
    runner.bus.write_long(0x2C, patch);
    let mut app = halted_ppc_app_with_sound(PpcSoundState::default());
    runner.init_ppc_companion(app.ppc.take().unwrap());
    assert_eq!(
        runner.dispatcher.trap_table_profile,
        Some(TrapTableProfile::M68k68040)
    );
    {
        let companion = runner
            .native
            .adapter_mut(NativeEngineRole::Companion)
            .unwrap();
        for (cell, handler) in cells {
            assert_eq!(
                companion.memory.read_u32_be(cell),
                Some(if cell == entry { patch } else { handler })
            );
        }
        assert_eq!(companion.memory.read_u32_be(0x28), Some(vectors[0]));
        assert_eq!(companion.memory.read_u32_be(0x2C), Some(patch));
        companion.memory.write_u32_be(entry, default).unwrap();
    }
    assert_eq!(
        runner.dispatcher.trap_table_address(&runner.bus, 0xA975),
        Some(default)
    );
    runner
        .native
        .adapter_mut(NativeEngineRole::Companion)
        .unwrap()
        .memory
        .write_u32_be(entry, patch)
        .unwrap();
    runner.m68k.cpu.write_reg(Register::D0, 0xA975);
    runner
        .dispatcher
        .dispatch(0xA746, &mut runner.m68k.cpu, &mut runner.bus)
        .unwrap();
    assert_eq!(runner.m68k.cpu.read_reg(Register::A0), patch);
    runner.m68k.cpu.write_reg(Register::PC, program);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    assert_eq!(runner.run_steps(1, None), (1, true));
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), patch);
    assert_eq!(runner.run_steps(1, None), (1, true));
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), program + 2);
}

/// A-line execution reaches the Trap Dispatcher through vector 10 at
/// `$28`; line-F reaches the Line 1111 emulator through vector 11 at
/// `$2C`. Both cells are writable system globals, so replacing either one
/// must expose the processor's format-0 frame to guest code rather than
/// silently entering HLE. Inside Macintosh Volume I (1985), p. I-89;
/// Inside Macintosh Volume III (1985), p. III-17.
#[test]
fn guest_line_vectors_receive_architectural_frames_and_restore_defaults() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let defaults = runner.dispatcher.trap_exception_vector_defaults.unwrap();
    let original_sp = 0x007F_FFC0;

    // MOVE.L #marker,D6; ADDQ.L #2,2(SP); RTE. The handler advances the
    // faulting PC in the format-0 frame before returning.
    let aline_handler = 0x0010_1000;
    runner.bus.write_word(aline_handler, 0x2C3C);
    runner.bus.write_long(aline_handler + 2, 0xA10E_0010);
    runner.bus.write_word(aline_handler + 6, 0x54AF);
    runner.bus.write_word(aline_handler + 8, 0x0002);
    runner.bus.write_word(aline_handler + 10, 0x4E73);
    runner.bus.write_long(0x28, aline_handler);

    let aline_program = 0x0010_0000;
    runner.bus.write_word(aline_program, 0xA975); // TickCount
    runner.bus.write_word(aline_program + 2, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, aline_program);
    runner.m68k.cpu.write_reg(Register::A7, original_sp);

    assert_eq!(runner.run_steps(1, None), (1, true));
    let aline_frame = original_sp - 8;
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), aline_handler);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), aline_frame);
    assert_eq!(runner.bus.read_long(aline_frame + 2), aline_program);
    assert_eq!(runner.bus.read_word(aline_frame + 6), 0x0028);
    assert_eq!(runner.run_steps(3, None), (3, true));
    assert_eq!(runner.m68k.cpu.read_reg(Register::D6), 0xA10E_0010);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), original_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), aline_program + 2);

    // Restoring the generated vector re-enables the ordinary HLE path.
    runner.bus.write_long(0x28, defaults[0]);
    runner.m68k.cpu.write_reg(Register::PC, aline_program);
    let trap_count = runner.dispatcher.trap_count;
    assert_eq!(runner.run_steps(1, None), (1, true));
    assert_eq!(runner.dispatcher.trap_count, trap_count + 1);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), aline_program + 2);

    // Repeat the same architectural proof for an unsupported F-line word.
    let fline_handler = 0x0010_1100;
    runner.bus.write_word(fline_handler, 0x2A3C); // MOVE.L #marker,D5
    runner.bus.write_long(fline_handler + 2, 0xF11E_0011);
    runner.bus.write_word(fline_handler + 6, 0x54AF);
    runner.bus.write_word(fline_handler + 8, 0x0002);
    runner.bus.write_word(fline_handler + 10, 0x4E73);
    runner.bus.write_long(0x2C, fline_handler);

    let fline_program = 0x0010_0200;
    runner.bus.write_word(fline_program, 0xF000);
    runner.bus.write_word(fline_program + 2, 0x4E71);
    runner.m68k.cpu.write_reg(Register::PC, fline_program);
    runner.m68k.cpu.write_reg(Register::A7, original_sp);

    assert_eq!(runner.run_steps(1, None), (1, true));
    let fline_frame = original_sp - 8;
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), fline_handler);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), fline_frame);
    assert_eq!(runner.bus.read_long(fline_frame + 2), fline_program);
    assert_eq!(runner.bus.read_word(fline_frame + 6), 0x002C);
    assert_eq!(runner.run_steps(3, None), (3, true));
    assert_eq!(runner.m68k.cpu.read_reg(Register::D5), 0xF11E_0011);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), original_sp);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), fline_program + 2);
    runner.bus.write_long(0x2C, defaults[1]);
}

/// FNOP is a valid 68040 coprocessor instruction, not a Line 1111
/// exception. Keeping a replacement vector 11 installed while it executes
/// proves opcode classification happens before exception delegation.
#[test]
fn valid_68040_fpu_opcode_does_not_enter_guest_vector_11() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let fline_handler = 0x0010_1100;
    runner.bus.write_word(fline_handler, 0x2E3C); // MOVE.L #sentinel,D7
    runner.bus.write_long(fline_handler + 2, 0xBADF_11E0);
    runner.bus.write_word(fline_handler + 6, 0x4E73);
    runner.bus.write_long(0x2C, fline_handler);

    let program = 0x0010_0000;
    runner.bus.write_word(program, 0xF280); // FNOP
    runner.bus.write_word(program + 2, 0x0000);
    runner.bus.write_word(program + 4, 0x4E71); // NOP
    runner.m68k.cpu.write_reg(Register::PC, program);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.m68k.cpu.write_reg(Register::D7, 0x1357_2468);

    assert_eq!(runner.run_steps(1, None), (1, true));
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), program + 4);
    assert_eq!(runner.m68k.cpu.read_reg(Register::A7), 0x007F_FFC0);
    assert_eq!(runner.m68k.cpu.read_reg(Register::D7), 0x1357_2468);
}

/// Running a `DIVU.W D0,D1` with `D0 = 0` must not halt the
/// runner. The `load_app_generic` loader installs an RTE stub at
/// `$00FE` and points vector 5 (`$14`) at it; the m68k crate's
/// zero-divide trap stacks the *next* PC and jumps to that vector,
/// so RTE-ing returns past the DIVU and execution continues.
/// Inside Macintosh Volume I, I-103 (Exception Vector Table);
/// M68000PRM ("If the source operand is zero, the result of the
/// operation is unpredictable").
#[test]
fn zero_divide_rte_handler_resumes_after_divu_by_zero() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    // Mirror what load_app_generic installs: RTE stub + vector.
    runner.bus.write_word(0x00FE, 0x4E73); // RTE
    runner.bus.write_long(0x0014, 0x0000_00FE);

    let prog = 0x0010_0000u32;
    runner.bus.write_word(prog, 0x82C0); // DIVU.W D0, D1
    runner.bus.write_word(prog + 2, 0x4E71); // NOP
    runner.bus.write_word(prog + 4, 0x4E71); // NOP

    runner.m68k.cpu.write_reg(Register::PC, prog);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.m68k.cpu.write_reg(Register::D0, 0);
    runner.m68k.cpu.write_reg(Register::D1, 100);

    // 1 step: DIVU.W traps, vectors to $00FE.
    // 2nd step: RTE at $00FE pops SR/PC, returns past DIVU.
    // 3rd step: NOP at prog+2.
    let (steps, running) = runner.run_steps(3, None);

    assert!(running, "runner must not halt on zero-divide");
    assert_eq!(steps, 3);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        prog + 4,
        "PC must advance past the DIVU+NOP without re-entering the trap"
    );
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::D1),
        100,
        "DIVU by zero must leave the destination register unchanged"
    );
}

/// CHK exception (vector 6) shares the same `$00FE` RTE stub as
/// the zero-divide handler. A `CHK.W #5, D0` with `D0 = 100`
/// exceeds the bound and triggers the trap; on a real Mac the
/// handler calls SysError, on Systemless we silently RTE so D0 is
/// preserved and the next instruction runs.
/// Inside Macintosh Volume I, I-103.
#[test]
fn chk_rte_handler_resumes_after_bounds_violation() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.bus.write_word(0x00FE, 0x4E73); // RTE
    runner.bus.write_long(0x0018, 0x0000_00FE); // CHK vector

    let prog = 0x0010_0000u32;
    runner.bus.write_word(prog, 0x41BC); // CHK.W #imm, D0
    runner.bus.write_word(prog + 2, 0x0005); // imm = 5
    runner.bus.write_word(prog + 4, 0x4E71); // NOP

    runner.m68k.cpu.write_reg(Register::PC, prog);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.m68k.cpu.write_reg(Register::D0, 100);

    // 1 step: CHK fires (100 > 5), vectors to $00FE.
    // 2nd step: RTE pops SR/PC, returns past CHK.
    // 3rd step: NOP executes.
    let (steps, running) = runner.run_steps(3, None);

    assert!(running, "runner must not halt on CHK bounds violation");
    assert_eq!(steps, 3);
    assert_eq!(
        runner.m68k.cpu.read_reg(Register::PC),
        prog + 6,
        "PC must advance past CHK (4 bytes) + NOP (2 bytes)"
    );
    assert_eq!(runner.m68k.cpu.read_reg(Register::D0), 100);
}

/// TRAPV (vector 7) shares the `$00FE` RTE stub. Pre-set the V
/// flag in CCR via the m68k API and execute TRAPV; the trap fires
/// because V is set, vectors to the RTE stub, and resumes at the
/// next instruction. Inside Macintosh Volume I, I-103.
#[test]
fn trapv_rte_handler_resumes_when_v_flag_is_set() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.bus.write_word(0x00FE, 0x4E73); // RTE
    runner.bus.write_long(0x001C, 0x0000_00FE); // TRAPV vector

    let prog = 0x0010_0000u32;
    runner.bus.write_word(prog, 0x4E76); // TRAPV
    runner.bus.write_word(prog + 2, 0x4E71); // NOP

    runner.m68k.cpu.write_reg(Register::PC, prog);
    runner.m68k.cpu.write_reg(Register::A7, 0x007F_FFC0);
    runner.m68k.cpu.core.set_ccr(0x02); // V flag set

    // 1: TRAPV traps; 2: RTE; 3: NOP.
    let (steps, running) = runner.run_steps(3, None);

    assert!(running, "runner must not halt on TRAPV");
    assert_eq!(steps, 3);
    assert_eq!(runner.m68k.cpu.read_reg(Register::PC), prog + 4);
}

/// `set_mouse_position` does NOT modify MBState ($0172) — it's a
/// move-without-button-change, so the button-state byte should
/// retain its prior value. The default at runner construction is
/// 0x80 (button up).
#[test]
fn set_mouse_position_leaves_mb_state_untouched() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.bus.write_byte(0x0172, 0x80);
    runner.set_mouse_position(50, 60);
    assert_eq!(runner.bus.read_byte(0x0172), 0x80);

    runner.bus.write_byte(0x0172, 0x00);
    runner.set_mouse_position(70, 80);
    assert_eq!(runner.bus.read_byte(0x0172), 0x00);
}

/// `push_mouse_down` must update MBState ($0172) to 0x00 (button
/// pressed) immediately AND sync the position globals so guest
/// code that polls these bytes directly sees the click without
/// waiting for the next tick advance.
/// Inside Macintosh Volume I, I-258 (MTemp/RawMouse/Mouse);
/// Inside Macintosh Volume II, II-371 (MBState polling).
#[test]
fn push_mouse_down_writes_mb_state_pressed_and_position() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.bus.write_byte(0x0172, 0x80); // start "button up"

    runner.push_mouse_down(123, 456);

    assert_eq!(
        runner.bus.read_byte(0x0172),
        0x00,
        "MBState must be 0x00 (pressed) immediately after push_mouse_down"
    );
    // All three position globals must mirror the click site so
    // games that poll them directly (Mouse $0830 etc.) see the
    // correct location, not the prior cursor-park position.
    assert_eq!(runner.bus.read_word(0x0828), 123u16);
    assert_eq!(runner.bus.read_word(0x082A), 456u16);
    assert_eq!(runner.bus.read_word(0x082C), 123u16);
    assert_eq!(runner.bus.read_word(0x082E), 456u16);
    assert_eq!(runner.bus.read_word(0x0830), 123u16);
    assert_eq!(runner.bus.read_word(0x0832), 456u16);
}

#[test]
fn pending_mouse_down_count_tracks_queued_clicks() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    assert_eq!(runner.pending_mouse_down_count(), 0);

    runner.push_mouse_down(10, 20);
    assert_eq!(runner.pending_mouse_down_count(), 1);

    runner.process_context.shared_event_queue().pop_front();
    assert_eq!(runner.pending_mouse_down_count(), 0);
}

/// `push_mouse_up` must update MBState ($0172) to 0x80 (button
/// released) immediately. On real hardware the ADB polls at ~200 Hz
/// so the latency between physical release and MBState=0x80 is a
/// few ms; deferring to advance_guest_tick (~16 ms) makes
/// frame-rate-dependent games read the wrong button state for too
/// many loop iterations after click-up. This test pins the
/// immediate-sync contract documented at runner.rs `push_mouse_up`.
#[test]
fn push_mouse_up_writes_mb_state_released_immediately() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.push_mouse_down(10, 20);
    assert_eq!(runner.bus.read_byte(0x0172), 0x00);

    runner.push_mouse_up(10, 20);
    assert_eq!(
        runner.bus.read_byte(0x0172),
        0x80,
        "MBState must flip back to 0x80 (released) immediately on push_mouse_up — \
             not deferred to the next tick"
    );
}

/// Regression: advance_guest_tick must NOT keep MBState at 0x00
/// when both mouseDown and a paired mouseUp are queued and
/// unconsumed. Polling-only games (Bonkheads-Deluxe class) never
/// call GetNextEvent — the queue accumulates indefinitely.
/// Pre-fix, the "any pending mouseDown → pressed" override left
/// $0172 stuck at 0x00 forever, so Button() always returned TRUE
/// and click detection broke silently. The fix counts unmatched
/// mouseDowns (mouseDown count − mouseUp count) and only treats
/// those as "still pressed".
#[test]
fn mb_state_releases_when_paired_mouseup_queued_but_unconsumed() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.push_mouse_down(10, 20);
    runner.push_mouse_up(10, 20);
    // Both events still queued (no GetNextEvent has run). Drive the
    // tick boundary that owns the MBState resync.
    runner.advance_guest_tick();

    assert_eq!(
        runner.bus.read_byte(0x0172),
        0x80,
        "advance_guest_tick must release MBState to 0x80 once a \
             paired mouseUp is queued behind the mouseDown — even when \
             nothing has drained the event queue"
    );
    // Sanity-check the events ARE still in the queue (this test is
    // about MBState despite the unconsumed events, not about queue
    // state). Read it from the canonical process_context queue.
    assert!(
        runner
            .process_context
            .event_queue()
            .iter()
            .any(|e| e.what == 1),
        "mouseDown event must remain in the queue (would be drained by GetNextEvent)"
    );
    assert!(
        runner
            .process_context
            .event_queue()
            .iter()
            .any(|e| e.what == 2),
        "mouseUp event must remain in the queue"
    );
}

/// Mirror of `mb_state_releases_when_paired_mouseup_queued_but_unconsumed`:
/// a SOLO mouseDown queued without a paired mouseUp must still pin
/// MBState to 0x00 across tick boundaries. This preserves the
/// original contract — code that hasn't yet started polling when
/// the click was injected gets at least one TRUE pulse — without
/// regressing into the stuck-pressed bug.
#[test]
fn mb_state_stays_pressed_with_solo_pending_mousedown() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());

    runner.push_mouse_down(10, 20);
    runner.advance_guest_tick();
    assert_eq!(
        runner.bus.read_byte(0x0172),
        0x00,
        "MBState must stay pressed across a tick advance while only \
             a mouseDown is queued (no paired mouseUp yet)"
    );
}

#[test]
fn menu_bar_policy_defaults_to_guest_control_and_supports_explicit_kiosk_modes() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
    assert!(
        runner.menu_bar_visible(),
        "library runners should permit guest menu chrome by default"
    );

    runner.set_menu_bar_policy(MenuBarPolicy::InitialKiosk);
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::InitialKiosk);
    assert!(!runner.menu_bar_visible());

    runner.set_menu_bar_visible(false);
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::ForceHidden);
    assert!(!runner.menu_bar_visible());

    runner.set_menu_bar_visible(true);
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
    assert!(runner.menu_bar_visible());
}

#[test]
fn initial_kiosk_releases_after_guest_hides_and_reveals_menu_bar() {
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    runner.set_menu_bar_policy(MenuBarPolicy::InitialKiosk);
    runner.dispatcher.front_window = 1;

    runner
        .bus
        .write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
    runner.force_advance_guest_tick();
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::InitialKiosk);
    assert!(!runner.menu_bar_visible());

    runner
        .bus
        .write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
    runner.force_advance_guest_tick();
    assert_eq!(runner.menu_bar_policy(), MenuBarPolicy::GuestControlled);
    assert!(runner.menu_bar_visible());
}

#[test]
fn disassemble_at_decodes_known_opcodes_with_correct_advance() {
    // Pins the FixtureRunner::disassemble_at public-API helper.
    // This is the library-level entry point for pixel-divergence
    // and trap-misroute investigations: pair with
    // SYSTEMLESS_TRACE_FB_WRITE_RANGE to see what code lives at a
    // suspect PC.
    //
    // Seed three known instructions in guest RAM, disassemble,
    // and verify:
    //   1. each entry's PC advances by the previous size
    //   2. the mnemonic for $4E71 is "NOP" (well-known fixed
    //      instruction; no operand words to consume)
    //   3. an A-line trap word ($A8EC = CopyBits) comes back as
    //      "DC.W $A8EC" — the m68k crate's convention for opcodes
    //      it doesn't have a regular decoder for
    //   4. the size returned is at least 2 and at most 10 (the
    //      clamp guard that prevents a malformed opcode from
    //      consuming wrap-around amounts)
    let mut runner = FixtureRunner::new(8 * 1024 * 1024, FixtureRunnerConfig::default());
    let pc = 0x10000u32;
    // $4E71 NOP
    runner.bus.write_word(pc, 0x4E71);
    // $A8EC (CopyBits trap-line word)
    runner.bus.write_word(pc + 2, 0xA8EC);
    // $4E71 NOP again
    runner.bus.write_word(pc + 4, 0x4E71);
    let out = runner.disassemble_at(pc, 3);
    assert_eq!(
        out.len(),
        3,
        "disassemble_at must return exactly count entries"
    );
    assert_eq!(
        out[0].0, pc,
        "first entry's PC must equal the requested start"
    );
    assert!(
        out[0].1.contains("NOP"),
        "$4E71 must disassemble to NOP, got: {}",
        out[0].1
    );
    assert!(
        out[0].2 >= 2 && out[0].2 <= 10,
        "instruction size must be in clamp range [2, 10], got {}",
        out[0].2
    );
    assert_eq!(
        out[1].0,
        pc + out[0].2,
        "second entry's PC must equal first PC + first size"
    );
    assert!(
        out[1].1.contains("$A8EC"),
        "A-line trap $A8EC must surface in mnemonic (DC.W form), got: {}",
        out[1].1
    );
    assert!(
        out[2].1.contains("NOP"),
        "third entry must be the second NOP we seeded"
    );
}

#[test]
fn disassemble_at_uses_the_configured_ram_boundary() {
    let mut runner = FixtureRunner::new(16 * 1024 * 1024, FixtureRunnerConfig::default());
    let pc = 12 * 1024 * 1024;
    runner.bus.write_word(pc, 0x4E71);

    let out = runner.disassemble_at(pc, 1);
    assert_eq!(out.len(), 1);
    assert!(
        out[0].1.contains("NOP"),
        "mapped RAM above 8 MiB must not be reported as unmapped"
    );
}
