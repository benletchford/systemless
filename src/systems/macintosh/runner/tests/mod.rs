pub(super) use super::app_heap_start_for_loaded_app;
pub(super) use super::apply_retro68_rela_relocations;
pub(super) use super::APPLICATION_RESOURCE_REFNUM;
pub(super) use super::DEFAULT_LAUNCH_TICKS;
pub(super) use super::HFS_FCB_BUFFER_SIZE;
pub(super) use super::HFS_FCB_SIZE;
use super::*;
use crate::audio::AudioBackend;
use crate::loader::ppc::*;
use crate::loader::{Code0Header, LoadedApp};
use crate::process_context::{
    ProcessFileSystemState, SharedProcessDisplayClut, SharedProcessDisplayGamma,
    SharedProcessFileSystem, SharedProcessGraphicsDevice, SharedProcessGraphicsPort,
    SharedProcessTickState,
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
mod boot;
mod cfm;
mod cursor;
mod debug;
mod dialog;
mod display;
mod display_sync;
mod event;
mod execution;
mod idle;
mod keyboard;
mod launch;
mod low_memory;
mod menu;
mod mixed_mode;
mod oracle;
mod partition;
mod ppc_app;
mod process_lifecycle;
mod relocation;
mod sound;
mod thread;
mod time;
mod trap_dispatch;
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
        notification_response_context: None,
        parked_interrupt_callback: None,
        interrupt_callback_parks: 0,
        application_size: Default::default(),
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
        glm_mode: None,
        glm_callbacks: [None; 8],
        glm_callback_stack: Vec::new(),
        glm_allocations: HashMap::new(),
        glm_page_free_all_queue: VecDeque::new(),
        glm_error: 0,
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

pub(super) fn ppc_test_relative_branch(from: u32, to: u32) -> u32 {
    0x4800_0000 | (to.wrapping_sub(from) & 0x03ff_fffc)
}
