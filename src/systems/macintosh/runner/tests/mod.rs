pub(super) use super::app_heap_start_for_loaded_app;
pub(super) use super::apply_retro68_rela_relocations;
pub(super) use super::DEFAULT_LAUNCH_TICKS;
use super::*;
use crate::audio::AudioBackend;
use crate::loader::ppc::*;
use crate::loader::{Code0Header, LoadedApp};
use crate::process_context::{
    PendingFileCompletion, ProcessFileSystemState, SharedProcessDisplayClut,
    SharedProcessDisplayGamma, SharedProcessFileSystem, SharedProcessGraphicsDevice,
    SharedProcessGraphicsPort, SharedProcessTickState,
};
use crate::sound::{PendingSoundCallback, PlaybackKind, SndChannel, SndCommand, OUTPUT_RATE};
use crate::trap::dispatch::{
    DialogItem, LoadedResources, QueuedEvent, ResourceFileMap, TimerTask, VblTask,
};
use ppc::{PpcCpu, PpcNativeReturnGpr3};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;

mod audio;
mod cfm;
mod cursor;
mod debug;
mod dialog;
mod display;
mod event;
mod idle;
mod keyboard;
mod launch;
mod low_memory;
mod menu;
mod mixed_mode;
mod partition;
mod process_lifecycle;
mod relocation;
mod sound;
mod thread;
mod time;
mod trap_patch;
mod vfs;
mod window;

pub(super) use dialog::dialog_tracking_for_test;
pub(super) use menu::run_classic_menu_select_with_powerpc_mdef_identity;

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
