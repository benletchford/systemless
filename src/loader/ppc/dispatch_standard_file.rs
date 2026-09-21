//! Typed Standard File Package dispatch for PowerPC imports.

use super::theme::*;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PpcStandardFileMode {
    GetModern,
    GetLegacy,
    PutModern,
    PutLegacy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcStandardFileCall {
    pub(super) mode: PpcStandardFileMode,
    pub(super) reply: u32,
    pub(super) return_address: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFileEntry {
    pub(super) name: Vec<u8>,
    pub(super) path: String,
    pub(super) dir_id: u32,
    pub(super) file_type: u32,
    pub(super) finder_flags: u16,
    pub(super) is_directory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFileGetTrackingState {
    pub(super) call: PpcStandardFileCall,
    pub(super) entries: Vec<PpcStandardFileEntry>,
    pub(super) current_dir_id: u32,
    pub(super) file_types: Option<Vec<u32>>,
    pub(super) selected: usize,
    pub(super) bounds: (i16, i16, i16, i16),
    pub(super) front_buffer: PpcFrontBuffer,
    pub(super) saved_pixels: crate::memory::SavedPixels<(i32, i32, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFileFilteringState {
    pub(super) import_pc: u32,
    pub(super) restore_rtoc: u32,
    callback: PpcCallbackTarget,
    callback_with_data: bool,
    pub(super) tracking: PpcStandardFileGetTrackingState,
    pub(super) next_entry: usize,
    pub(super) filter_pb: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFilePutTrackingState {
    pub(super) call: PpcStandardFileCall,
    pub(super) vref: i16,
    pub(super) dir_id: u32,
    pub(super) prompt: Vec<u8>,
    pub(super) name: Vec<u8>,
    pub(super) sel_start: usize,
    pub(super) sel_end: usize,
    pub(super) bounds: (i16, i16, i16, i16),
    pub(super) front_buffer: PpcFrontBuffer,
    pub(super) saved_pixels: crate::memory::SavedPixels<(i32, i32, u16)>,
}

pub(super) struct PpcStandardFileDispatchContext<'a> {
    pub(super) binding: &'a PpcImportBinding,
    pub(super) cpu: &'a mut PpcCpu,
    pub(super) memory: &'a mut PpcSectionMem,
    pub(super) startup: &'a mut PpcToolboxStartupState,
    pub(super) process_memory_manager: &'a mut ProcessNativeMemoryManager,
    pub(super) heap_cursor: &'a mut u32,
    pub(super) last_mem_error: &'a mut i16,
    pub(super) gworlds: &'a [PpcGWorldRecord],
    pub(super) vfs_directories: &'a mut Vec<PpcVfsDirectory>,
    pub(super) vfs_files: &'a ProcessVfsFileRecords,
    pub(super) vfs_resource_files: &'a [PpcVfsResourceFileRecord],
    pub(super) vfs_volumes: &'a [PpcVfsVolumeRecord],
    pub(super) default_dir_id: u32,
    pub(super) working_directories: &'a mut HashMap<i16, ProcessWorkingDirectory>,
    pub(super) next_working_directory_ref_num: &'a mut i16,
    pub(super) event_queue: &'a mut EventQueue,
}

pub(super) fn dispatch_standard_file_import(
    context: PpcStandardFileDispatchContext<'_>,
) -> Option<PpcImportAction> {
    let PpcStandardFileDispatchContext {
        binding,
        cpu,
        memory,
        startup,
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
    } = context;

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::StandardGetFile => Some(ppc_dispatch_standard_file(
            PpcStandardFileOperation::StandardGetFile,
            cpu,
            memory,
            startup,
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
        )),
        PpcImportDispatcherTarget::StandardFileCompatibility(operation) => {
            Some(ppc_dispatch_standard_file(
                operation,
                cpu,
                memory,
                startup,
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
            ))
        }
        _ => None,
    }
}

pub(super) const PPC_STANDARD_FILE_GET_DIALOG_WIDTH: i16 = 356;
pub(super) const PPC_STANDARD_FILE_GET_DIALOG_HEIGHT: i16 = 178;
pub(super) const PPC_STANDARD_FILE_GET_VOLUME_RECT: (i16, i16, i16, i16) = (12, 90, 31, 164);
pub(super) const PPC_STANDARD_FILE_GET_VOLUME_LABEL_RECT: (i16, i16, i16, i16) = (12, 268, 31, 352);
pub(super) const PPC_STANDARD_FILE_GET_LIST_RECT: (i16, i16, i16, i16) = (35, 18, 163, 236);
pub(super) const PPC_STANDARD_FILE_GET_SCROLL_RECT: (i16, i16, i16, i16) = (35, 235, 163, 251);
pub(super) const PPC_STANDARD_FILE_GET_EJECT_RECT: (i16, i16, i16, i16) = (38, 258, 59, 338);
pub(super) const PPC_STANDARD_FILE_GET_DESKTOP_RECT: (i16, i16, i16, i16) = (66, 258, 87, 338);
pub(super) const PPC_STANDARD_FILE_GET_SEPARATOR_RECT: (i16, i16, i16, i16) = (98, 258, 99, 338);
pub(super) const PPC_STANDARD_FILE_GET_CANCEL_RECT: (i16, i16, i16, i16) = (110, 258, 131, 338);
pub(super) const PPC_STANDARD_FILE_GET_OPEN_RECT: (i16, i16, i16, i16) = (138, 258, 159, 338);
pub(super) const PPC_STANDARD_FILE_GET_ROW_HEIGHT: i16 = 14;
pub(super) const PPC_STANDARD_FILE_PUT_DIALOG_WIDTH: i16 = 360;
pub(super) const PPC_STANDARD_FILE_PUT_DIALOG_HEIGHT: i16 = 148;
pub(super) const PPC_STANDARD_FILE_PUT_CANCEL_RECT: (i16, i16, i16, i16) = (103, 166, 125, 246);
pub(super) const PPC_STANDARD_FILE_PUT_SAVE_RECT: (i16, i16, i16, i16) = (103, 258, 125, 338);
pub(super) const PPC_STANDARD_FILE_PUT_NAME_RECT: (i16, i16, i16, i16) = (52, 24, 72, 330);

fn ppc_standard_file_reply_ptr(mode: PpcStandardFileMode, cpu: &PpcCpu) -> u32 {
    match mode {
        PpcStandardFileMode::GetModern => cpu.gpr[6],
        PpcStandardFileMode::GetLegacy => cpu.gpr[9],
        PpcStandardFileMode::PutModern => cpu.gpr[5],
        PpcStandardFileMode::PutLegacy => cpu.gpr[7],
    }
}

fn ppc_standard_file_call(mode: PpcStandardFileMode, cpu: &PpcCpu) -> PpcStandardFileCall {
    PpcStandardFileCall {
        mode,
        reply: ppc_standard_file_reply_ptr(mode, cpu),
        return_address: cpu.lr,
    }
}

fn ppc_standard_file_get_type_list(
    memory: &mut PpcSectionMem,
    num_types: i16,
    type_list_ptr: u32,
) -> Option<Option<Vec<u32>>> {
    // Inside Macintosh: Files (1992), pp. 3-50--3-51: -1 means all types,
    // zero means no selectable file types, and positive values name OSTypes.
    if num_types < -1 {
        return None;
    }
    if num_types == -1 {
        return Some(None);
    }
    if num_types == 0 {
        return Some(Some(Vec::new()));
    }
    if type_list_ptr == 0 {
        return None;
    }
    let count = usize::try_from(num_types).ok()?.min(64);
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        let offset = u32::try_from(index).ok()?.checked_mul(4)?;
        result.push(memory.read_u32_be(type_list_ptr.checked_add(offset)?)?);
    }
    Some(Some(result))
}

fn ppc_standard_file_get_entries(
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    dir_id: u32,
    file_types: Option<&[u32]>,
) -> Vec<PpcStandardFileEntry> {
    let Some(parent_path) = ppc_directory_path_for_id(vfs_directories, dir_id) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for directory in vfs_directories
        .iter()
        .filter(|directory| directory.parent_dir_id == dir_id)
    {
        entries.push(PpcStandardFileEntry {
            name: encode_mac_roman_lossy(ppc_vfs_basename(&directory.path))
                .into_iter()
                .take(63)
                .collect(),
            path: directory.path.clone(),
            dir_id: directory.dir_id,
            file_type: 0,
            finder_flags: directory.finder_flags,
            is_directory: true,
        });
    }
    for file in vfs_files {
        let Some(name) = ppc_child_name_for_parent(parent_path, &file.path) else {
            continue;
        };
        if file_types.is_some_and(|types| !types.contains(&file.file_type)) {
            continue;
        }
        entries.push(PpcStandardFileEntry {
            name: encode_mac_roman_lossy(name).into_iter().take(63).collect(),
            path: file.path.clone(),
            dir_id,
            file_type: file.file_type,
            finder_flags: file.finder_flags,
            is_directory: false,
        });
    }
    for fork in vfs_resource_files {
        // A resource-only record is visible, but a second resource record for
        // a data-backed file must not duplicate the catalog row.
        if vfs_files
            .iter()
            .any(|file| file.path.eq_ignore_ascii_case(&fork.path))
        {
            continue;
        }
        let Some(name) = ppc_child_name_for_parent(parent_path, &fork.path) else {
            continue;
        };
        if file_types.is_some_and(|types| !types.contains(&fork.file_type)) {
            continue;
        }
        entries.push(PpcStandardFileEntry {
            name: encode_mac_roman_lossy(name).into_iter().take(63).collect(),
            path: fork.path.clone(),
            dir_id,
            file_type: fork.file_type,
            finder_flags: fork.finder_flags,
            is_directory: false,
        });
    }
    entries.sort_by(|left, right| {
        (
            String::from_utf8_lossy(&left.name).to_ascii_lowercase(),
            u8::from(!left.is_directory),
            left.path.to_ascii_lowercase(),
        )
            .cmp(&(
                String::from_utf8_lossy(&right.name).to_ascii_lowercase(),
                u8::from(!right.is_directory),
                right.path.to_ascii_lowercase(),
            ))
    });
    entries
}

pub(super) fn ppc_capture_saved_detail<T>(
    memory: &PpcSectionMem,
    front: PpcFrontBuffer,
    point: (i32, i32),
    pixels: &mut crate::memory::SavedPixels<T>,
    index: usize,
) {
    if matches!(front.depth, 8 | 16) {
        let lanes = front.depth / 8;
        let address = front.base_addr + point.1 as u32 * front.row_bytes + point.0 as u32 * lanes;
        memory.presentation().capture_detail(
            pixels,
            index * lanes as usize,
            address,
            lanes as usize,
        );
    }
}

pub(super) fn ppc_restore_saved_detail<T>(
    memory: &PpcSectionMem,
    front: PpcFrontBuffer,
    point: (i32, i32),
    pixels: &crate::memory::SavedPixels<T>,
    index: usize,
) {
    if matches!(front.depth, 8 | 16) {
        let lanes = front.depth / 8;
        let address = front.base_addr + point.1 as u32 * front.row_bytes + point.0 as u32 * lanes;
        memory.presentation().restore_detail(
            pixels,
            index * lanes as usize,
            address,
            lanes as usize,
        );
    }
}

fn ppc_standard_file_save_pixels(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    bounds: (i16, i16, i16, i16),
) -> crate::memory::SavedPixels<(i32, i32, u16)> {
    let top = i32::from(bounds.0).max(0).min(front.height as i32);
    let left = i32::from(bounds.1).max(0).min(front.width as i32);
    let bottom = i32::from(bounds.2).max(0).min(front.height as i32);
    let right = i32::from(bounds.3).max(0).min(front.width as i32);
    let mut pixels = Vec::new();
    for y in top..bottom {
        for x in left..right {
            if let Some(pixel) = ppc_quickdraw_read_pixel(memory, front, (x, y)) {
                pixels.push((x, y, pixel));
            }
        }
    }
    let mut saved = crate::memory::SavedPixels::from(pixels);
    for index in 0..saved.len() {
        let (x, y, _) = saved[index];
        ppc_capture_saved_detail(memory, front, (x, y), &mut saved, index);
    }
    saved
}

fn ppc_standard_file_restore_pixels(
    memory: &mut PpcSectionMem,
    pixels: &crate::memory::SavedPixels<(i32, i32, u16)>,
    front: PpcFrontBuffer,
) {
    for (index, (x, y, value)) in pixels.iter().copied().enumerate() {
        let _ = ppc_quickdraw_write_raw_pixel(memory, front, (x, y), value);
        ppc_restore_saved_detail(memory, front, (x, y), pixels, index);
    }
}

fn ppc_standard_file_point_in_rect(point: (i16, i16), rect: (i16, i16, i16, i16)) -> bool {
    point.0 >= rect.0 && point.0 < rect.2 && point.1 >= rect.1 && point.1 < rect.3
}

fn ppc_standard_file_centered_bounds(
    front: PpcFrontBuffer,
    width: i16,
    height: i16,
    requested_origin: Option<(i16, i16)>,
) -> (i16, i16, i16, i16) {
    let (centered_top, centered_left) = (
        (front.height as i16).saturating_sub(height) / 2,
        (front.width as i16).saturating_sub(width) / 2,
    );
    let (requested_top, requested_left) = requested_origin.unwrap_or((centered_top, centered_left));
    let top = requested_top
        .max(0)
        .min((front.height as i16).saturating_sub(height).max(0));
    let left = requested_left
        .max(0)
        .min((front.width as i16).saturating_sub(width).max(0));
    (
        top,
        left,
        top.saturating_add(height),
        left.saturating_add(width),
    )
}

fn ppc_standard_file_point_from_gpr(point: u32) -> (i16, i16) {
    ((point >> 16) as u16 as i16, point as u16 as i16)
}

impl PpcStandardFileOperation {
    fn mode(self) -> PpcStandardFileMode {
        match self {
            Self::StandardGetFile | Self::CustomGetFile => PpcStandardFileMode::GetModern,
            Self::SfGetFile | Self::SfpGetFile => PpcStandardFileMode::GetLegacy,
            Self::CustomPutFile | Self::StandardPutFile => PpcStandardFileMode::PutModern,
            Self::SfpPutFile | Self::SfPutFile => PpcStandardFileMode::PutLegacy,
        }
    }

    fn requested_origin(self, cpu: &PpcCpu) -> Option<(i16, i16)> {
        let point = match self {
            Self::SfGetFile | Self::SfpGetFile | Self::SfpPutFile | Self::SfPutFile => cpu.gpr[3],
            Self::CustomGetFile => cpu.gpr[8],
            Self::CustomPutFile => cpu.gpr[7],
            Self::StandardGetFile | Self::StandardPutFile => return None,
        };
        let origin = ppc_standard_file_point_from_gpr(point);
        if origin == (-1, -1) {
            None
        } else {
            Some(origin)
        }
    }

    fn filter_pointer(self, cpu: &PpcCpu) -> (u32, bool) {
        match self {
            Self::StandardGetFile => (cpu.gpr[3], false),
            Self::CustomGetFile => (cpu.gpr[3], true),
            Self::SfGetFile | Self::SfpGetFile => (cpu.gpr[5], false),
            Self::CustomPutFile | Self::SfpPutFile | Self::SfPutFile | Self::StandardPutFile => {
                (0, false)
            }
        }
    }

    fn prompt_pointer(self, cpu: &PpcCpu) -> u32 {
        match self {
            Self::CustomPutFile | Self::StandardPutFile => cpu.gpr[3],
            Self::SfpPutFile | Self::SfPutFile => cpu.gpr[4],
            Self::StandardGetFile | Self::CustomGetFile | Self::SfGetFile | Self::SfpGetFile => 0,
        }
    }
}

fn ppc_standard_file_prompt(memory: &mut PpcSectionMem, prompt_ptr: u32) -> Vec<u8> {
    if prompt_ptr == 0 {
        return b"Save as:".to_vec();
    }
    let prompt = ppc_read_pstring_bytes(memory, prompt_ptr).unwrap_or_default();
    if prompt.is_empty() {
        b"Save as:".to_vec()
    } else {
        prompt
    }
}

const PPC_STANDARD_FILE_FILTER_PB_SIZE: u32 = 256;
const PPC_STANDARD_FILE_FILTER_NAME_OFFSET: u32 = 128;

fn ppc_standard_file_write_filter_pb(
    memory: &mut PpcSectionMem,
    filter_pb: u32,
    entry: &PpcStandardFileEntry,
) -> bool {
    if !ppc_memory_can_write_bytes(memory, filter_pb, PPC_STANDARD_FILE_FILTER_PB_SIZE) {
        return false;
    }
    let name_ptr = filter_pb.saturating_add(PPC_STANDARD_FILE_FILTER_NAME_OFFSET);
    ppc_write_pstring_bytes(memory, name_ptr, &entry.name)
        && memory.write_u32_be(filter_pb + 18, name_ptr).is_some()
        && memory
            .write_u16_be(filter_pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .is_some()
        && memory.write_u16_be(filter_pb + 28, 0).is_some()
        && memory.write_u32_be(filter_pb + 48, entry.dir_id).is_some()
        && ppc_write_finfo(
            memory,
            filter_pb + 32,
            entry.file_type,
            0,
            entry.finder_flags,
        )
        .is_some()
}

fn ppc_standard_file_filter_next_action(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    state: &mut PpcStandardFileFilteringState,
) -> Option<PpcImportAction> {
    while state
        .tracking
        .entries
        .get(state.next_entry)
        .is_some_and(|entry| entry.is_directory)
    {
        state.next_entry = state.next_entry.saturating_add(1);
    }
    let entry = state.tracking.entries.get(state.next_entry)?;
    if entry.is_directory {
        return None;
    }
    if !ppc_standard_file_write_filter_pb(memory, state.filter_pb, entry) {
        return None;
    }
    let arguments = if state.callback_with_data {
        vec![state.filter_pb, 0]
    } else {
        vec![state.filter_pb]
    };
    install_powerpc_call_arguments(cpu, memory, &arguments)?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: state.callback.entry,
            rtoc: state.callback.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            state.import_pc,
            state.restore_rtoc,
            PpcNativeReturnGpr3::Mask(0xff),
        ),
    )
    .into_ppc_import_action()
}

fn ppc_standard_file_dispose_filter_pb(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    filter_pb: u32,
) {
    if filter_pb != 0 {
        let _ = process_memory_manager.dispose_native_ptr(filter_pb);
        ppc_apply_process_native_allocator(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
        );
    }
}

pub(super) fn ppc_standard_file_draw_button(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    gworlds: &[PpcGWorldRecord],
    bounds: (i16, i16, i16, i16),
    rect: (i16, i16, i16, i16),
    label: &[u8],
    enabled: bool,
    is_default: bool,
) {
    let global = (
        bounds.0.saturating_add(rect.0),
        bounds.1.saturating_add(rect.1),
        bounds.0.saturating_add(rect.2),
        bounds.1.saturating_add(rect.3),
    );
    if ppc_ui_theme(gworlds) == UiThemeId::ClassicSystem7 {
        let _ = ppc_fill_front_rect(memory, front, global, PPC_RGB_WHITE);
        let color = if enabled {
            PPC_RGB_BLACK
        } else {
            PpcRgbColor {
                red: 0xaaaa,
                green: 0xaaaa,
                blue: 0xaaaa,
            }
        };
        if is_default {
            let outer = (
                global.0.saturating_sub(4),
                global.1.saturating_sub(4),
                global.2.saturating_add(4),
                global.3.saturating_add(4),
            );
            let _ = ppc_frame_front_round_rect(memory, front, outer, 8, 3, PPC_RGB_BLACK);
        }
        let _ = ppc_frame_front_round_rect(memory, front, global, 7, 1, color);
    } else {
        ppc_draw_retained_control_rect(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            global,
            crate::ui_theme::ControlKind::PushButton,
            enabled,
            is_default,
        );
    }
    let advance =
        ppc_text_bytes_advance_for_font(label, PPC_QD_TEXT_FONT_DEFAULT, PPC_QD_TEXT_SIZE_SYSTEM);
    let label_left = global
        .1
        .saturating_add((global.3.saturating_sub(global.1).saturating_sub(advance)) / 2);
    ppc_draw_dialog_text(
        memory,
        gworlds,
        (global.0, label_left, global.2, global.3),
        label,
        if enabled {
            PPC_RGB_BLACK
        } else {
            PpcRgbColor {
                red: 0xaaaa,
                green: 0xaaaa,
                blue: 0xaaaa,
            }
        },
    );
}

fn ppc_standard_file_draw_scrollbar(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    gworlds: &[PpcGWorldRecord],
    tracking: &PpcStandardFileGetTrackingState,
) {
    let bounds = tracking.bounds;
    let rect = (
        bounds.0.saturating_add(PPC_STANDARD_FILE_GET_SCROLL_RECT.0),
        bounds.1.saturating_add(PPC_STANDARD_FILE_GET_SCROLL_RECT.1),
        bounds.0.saturating_add(PPC_STANDARD_FILE_GET_SCROLL_RECT.2),
        bounds.1.saturating_add(PPC_STANDARD_FILE_GET_SCROLL_RECT.3),
    );
    let visible_rows = 8usize;
    let max = tracking.entries.len().saturating_sub(visible_rows);
    let first_visible = tracking
        .selected
        .saturating_sub(visible_rows.saturating_sub(1));
    let _ = front;
    ppc_draw_retained_scrollbar_rect(
        memory,
        gworlds,
        PPC_MAIN_GWORLD,
        rect,
        first_visible.min(i16::MAX as usize) as i16,
        0,
        max.min(i16::MAX as usize) as i16,
    );
}

fn ppc_standard_file_global_rect(
    bounds: (i16, i16, i16, i16),
    rect: (i16, i16, i16, i16),
) -> (i16, i16, i16, i16) {
    (
        bounds.0.saturating_add(rect.0),
        bounds.1.saturating_add(rect.1),
        bounds.0.saturating_add(rect.2),
        bounds.1.saturating_add(rect.3),
    )
}

fn ppc_standard_file_draw_get_dialog(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    tracking: &PpcStandardFileGetTrackingState,
) {
    let front = tracking.front_buffer;
    let bounds = tracking.bounds;
    ppc_draw_retained_dialog_frame(memory, gworlds, bounds, bounds, 2);
    let list = (
        bounds.0.saturating_add(PPC_STANDARD_FILE_GET_LIST_RECT.0),
        bounds.1.saturating_add(PPC_STANDARD_FILE_GET_LIST_RECT.1),
        bounds.0.saturating_add(PPC_STANDARD_FILE_GET_LIST_RECT.2),
        bounds.1.saturating_add(PPC_STANDARD_FILE_GET_LIST_RECT.3),
    );
    let _ = ppc_fill_front_rect(memory, front, list, PPC_RGB_WHITE);
    let _ = ppc_frame_front_rect(memory, front, list, PPC_RGB_BLACK, 1);
    let visible_rows = 8usize;
    let first_visible = tracking.selected.saturating_sub(visible_rows - 1);
    for row in 0..visible_rows {
        let index = first_visible + row;
        let Some(entry) = tracking.entries.get(index) else {
            break;
        };
        let row_top = list.0.saturating_add(2).saturating_add(
            i16::try_from(row)
                .unwrap_or(i16::MAX)
                .saturating_mul(PPC_STANDARD_FILE_GET_ROW_HEIGHT),
        );
        let row_bottom = row_top
            .saturating_add(PPC_STANDARD_FILE_GET_ROW_HEIGHT)
            .min(list.2.saturating_sub(1));
        let selected = index == tracking.selected;
        let row_rect = (
            row_top,
            list.1.saturating_add(2),
            row_bottom,
            list.3.saturating_sub(2),
        );
        if selected {
            let _ = ppc_fill_front_rect(memory, front, row_rect, PPC_RGB_BLACK);
        }
        let mut text = entry.name.clone();
        if entry.is_directory {
            text.extend_from_slice(b" >");
        }
        ppc_draw_dialog_text(
            memory,
            gworlds,
            (
                row_top.saturating_add(1),
                list.1.saturating_add(5),
                row_bottom,
                list.3.saturating_sub(3),
            ),
            &text,
            if selected {
                PPC_RGB_WHITE
            } else {
                PPC_RGB_BLACK
            },
        );
    }
    let volume_rect = ppc_standard_file_global_rect(bounds, PPC_STANDARD_FILE_GET_VOLUME_RECT);
    ppc_draw_retained_control_rect(
        memory,
        gworlds,
        PPC_MAIN_GWORLD,
        volume_rect,
        crate::ui_theme::ControlKind::PopupButton,
        true,
        false,
    );
    ppc_draw_dialog_text(memory, gworlds, volume_rect, b"Maci...", PPC_RGB_BLACK);
    ppc_draw_dialog_text(
        memory,
        gworlds,
        ppc_standard_file_global_rect(bounds, PPC_STANDARD_FILE_GET_VOLUME_LABEL_RECT),
        crate::trap::dispatch::BOOT_VOLUME_NAME.as_bytes(),
        PPC_RGB_BLACK,
    );
    ppc_standard_file_draw_scrollbar(memory, front, gworlds, tracking);
    ppc_standard_file_draw_button(
        memory,
        front,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_GET_EJECT_RECT,
        b"Eject",
        false,
        false,
    );
    ppc_standard_file_draw_button(
        memory,
        front,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_GET_DESKTOP_RECT,
        b"Desktop",
        true,
        false,
    );
    let separator = ppc_standard_file_global_rect(bounds, PPC_STANDARD_FILE_GET_SEPARATOR_RECT);
    let _ = ppc_fill_front_rect(memory, front, separator, PPC_RGB_BLACK);
    ppc_standard_file_draw_button(
        memory,
        front,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_GET_CANCEL_RECT,
        b"Cancel",
        true,
        false,
    );
    let open_enabled = tracking
        .entries
        .get(tracking.selected)
        .is_some_and(|entry| entry.is_directory || entry.file_type != 0);
    ppc_standard_file_draw_button(
        memory,
        front,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_GET_OPEN_RECT,
        b"Open",
        open_enabled,
        true,
    );
}

fn ppc_standard_file_draw_put_dialog(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    tracking: &PpcStandardFilePutTrackingState,
) {
    let front = tracking.front_buffer;
    let bounds = tracking.bounds;
    if !ppc_draw_themed_dialog_frame(memory, gworlds, bounds, bounds, 2) {
        let _ = ppc_fill_front_rect(memory, front, bounds, PPC_RGB_WHITE);
        let _ = ppc_frame_front_rect(memory, front, bounds, PPC_RGB_BLACK, 2);
    }
    ppc_draw_dialog_text(
        memory,
        gworlds,
        (
            bounds.0.saturating_add(14),
            bounds.1.saturating_add(18),
            bounds.0.saturating_add(32),
            bounds.1.saturating_add(330),
        ),
        b"Save File",
        PPC_RGB_BLACK,
    );
    ppc_draw_dialog_text(
        memory,
        gworlds,
        (
            bounds.0.saturating_add(32),
            bounds.1.saturating_add(18),
            bounds.0.saturating_add(50),
            bounds.1.saturating_add(330),
        ),
        &tracking.prompt,
        PPC_RGB_BLACK,
    );
    let name = (
        bounds.0.saturating_add(PPC_STANDARD_FILE_PUT_NAME_RECT.0),
        bounds.1.saturating_add(PPC_STANDARD_FILE_PUT_NAME_RECT.1),
        bounds.0.saturating_add(PPC_STANDARD_FILE_PUT_NAME_RECT.2),
        bounds.1.saturating_add(PPC_STANDARD_FILE_PUT_NAME_RECT.3),
    );
    let _ = ppc_fill_front_rect(memory, front, name, PPC_RGB_WHITE);
    let _ = ppc_frame_front_rect(memory, front, name, PPC_RGB_BLACK, 1);
    let selected = tracking.sel_start < tracking.sel_end;
    let themed = ppc_ui_theme(gworlds) != UiThemeId::ClassicSystem7;
    if selected && !themed {
        let _ = ppc_fill_front_rect(
            memory,
            front,
            (
                name.0,
                name.1.saturating_add(2),
                name.2,
                name.3.saturating_sub(2),
            ),
            PPC_RGB_BLACK,
        );
    }
    ppc_draw_dialog_text(
        memory,
        gworlds,
        (name.0.saturating_add(2), name.1, name.2, name.3),
        &tracking.name,
        if selected && !themed {
            PPC_RGB_WHITE
        } else {
            PPC_RGB_BLACK
        },
    );
    if selected && themed {
        ppc_draw_themed_selection(memory, gworlds, PPC_MAIN_GWORLD, name);
    }
    ppc_standard_file_draw_button(
        memory,
        front,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_PUT_CANCEL_RECT,
        b"Cancel",
        true,
        false,
    );
    ppc_standard_file_draw_button(
        memory,
        front,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_PUT_SAVE_RECT,
        b"Save",
        true,
        true,
    );
}

fn ppc_standard_file_write_cancel_reply(
    memory: &mut PpcSectionMem,
    mode: PpcStandardFileMode,
    reply: u32,
) {
    if reply == 0 {
        return;
    }
    // sfGood/good is the only defined result after cancellation; preserve
    // the caller-owned tail just as Standard File does on classic systems.
    let _ = memory.write_u8(reply, 0);
    let _ = mode;
}

fn ppc_standard_file_working_directory_ref(
    vref: i16,
    dir_id: u32,
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
) -> i16 {
    let root_dir_id = if vref == PPC_BOOT_VOLUME_REF_NUM {
        PPC_ROOT_DIR_ID
    } else {
        vfs_volumes
            .iter()
            .find(|volume| volume.ref_num == vref)
            .map(|volume| volume.root_dir_id)
            .unwrap_or(PPC_ROOT_DIR_ID)
    };
    if dir_id == root_dir_id {
        return vref;
    }
    if let Some(existing) = working_directories.values().find(|record| {
        record.volume_ref_num == vref && record.dir_id == dir_id && record.proc_id == 0
    }) {
        return existing.ref_num;
    }
    let mut ref_num = (*next_working_directory_ref_num).max(1);
    while ref_num == PPC_BOOT_VOLUME_REF_NUM || working_directories.contains_key(&ref_num) {
        ref_num = ref_num.saturating_add(1);
    }
    *next_working_directory_ref_num = ref_num.saturating_add(1);
    working_directories.insert(
        ref_num,
        ProcessWorkingDirectory {
            ref_num,
            volume_ref_num: vref,
            dir_id,
            proc_id: 0,
        },
    );
    ref_num
}

fn ppc_standard_file_write_get_reply(
    memory: &mut PpcSectionMem,
    mode: PpcStandardFileMode,
    reply: u32,
    entry: &PpcStandardFileEntry,
    legacy_wd_ref: i16,
) {
    if reply == 0 {
        return;
    }
    match mode {
        PpcStandardFileMode::GetModern => {
            if memory.write_u8(reply, 1).is_none() {
                return;
            }
            let _ = memory.write_u8(reply + 1, 0);
            let _ = memory.write_u32_be(reply + 2, entry.file_type);
            let _ = ppc_write_fsspec(
                memory,
                reply + 6,
                PPC_BOOT_VOLUME_REF_NUM,
                entry.dir_id,
                &entry.name,
            );
            let _ = memory.write_u16_be(reply + 76, 0);
            let _ = memory.write_u16_be(reply + 78, entry.finder_flags);
            let _ = memory.write_u8(reply + 80, 0);
            let _ = memory.write_u8(reply + 81, 0);
            let _ = memory.write_u32_be(reply + 82, 0);
            let _ = memory.write_u16_be(reply + 86, 0);
        }
        PpcStandardFileMode::GetLegacy => {
            if memory.write_u8(reply, 1).is_none() {
                return;
            }
            let _ = memory.write_u8(reply + 1, 0);
            let _ = memory.write_u32_be(reply + 2, entry.file_type);
            let _ = memory.write_u16_be(reply + 6, legacy_wd_ref as u16);
            let _ = memory.write_u16_be(reply + 8, 0);
            let _ = ppc_write_pstring_bytes(memory, reply + 10, &entry.name);
        }
        PpcStandardFileMode::PutModern | PpcStandardFileMode::PutLegacy => {}
    }
}

fn ppc_standard_file_write_put_reply(
    memory: &mut PpcSectionMem,
    mode: PpcStandardFileMode,
    reply: u32,
    vref: i16,
    dir_id: u32,
    name: &[u8],
    replacing: bool,
) {
    if reply == 0 {
        return;
    }
    match mode {
        PpcStandardFileMode::PutModern => {
            if memory.write_u8(reply, 1).is_none() {
                return;
            }
            let _ = memory.write_u8(reply + 1, u8::from(replacing));
            let _ = memory.write_u32_be(reply + 2, 0);
            let _ = ppc_write_fsspec(memory, reply + 6, vref, dir_id, name);
            let _ = memory.write_u16_be(reply + 76, 0);
            let _ = memory.write_u16_be(reply + 78, 0);
            let _ = memory.write_u8(reply + 80, 0);
            let _ = memory.write_u8(reply + 81, 0);
            let _ = memory.write_u32_be(reply + 82, 0);
            let _ = memory.write_u16_be(reply + 86, 0);
        }
        PpcStandardFileMode::PutLegacy => {
            if memory.write_u8(reply, 1).is_none() {
                return;
            }
            let _ = memory.write_u8(reply + 1, 0);
            let _ = memory.write_u32_be(reply + 2, 0);
            let _ = memory.write_u16_be(reply + 6, vref as u16);
            let _ = memory.write_u16_be(reply + 8, 0);
            let _ = ppc_write_pstring_bytes(memory, reply + 10, name);
        }
        PpcStandardFileMode::GetModern | PpcStandardFileMode::GetLegacy => {}
    }
}

fn ppc_standard_file_finish_get(
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    tracking: PpcStandardFileGetTrackingState,
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    accepted: bool,
) -> PpcImportAction {
    ppc_standard_file_restore_pixels(memory, &tracking.saved_pixels, tracking.front_buffer);
    if accepted
        && tracking
            .entries
            .get(tracking.selected)
            .is_some_and(|entry| !entry.is_directory)
    {
        let legacy_wd_ref = if tracking.call.mode == PpcStandardFileMode::GetLegacy {
            ppc_standard_file_working_directory_ref(
                PPC_BOOT_VOLUME_REF_NUM,
                tracking.entries.get(tracking.selected).unwrap().dir_id,
                vfs_volumes,
                working_directories,
                next_working_directory_ref_num,
            )
        } else {
            PPC_BOOT_VOLUME_REF_NUM
        };
        ppc_standard_file_write_get_reply(
            memory,
            tracking.call.mode,
            tracking.call.reply,
            tracking.entries.get(tracking.selected).unwrap(),
            legacy_wd_ref,
        );
    } else {
        ppc_standard_file_write_cancel_reply(memory, tracking.call.mode, tracking.call.reply);
    }
    startup.standard_file_get_tracking = None;
    PpcImportAction::ReturnPreserve
}

#[allow(clippy::too_many_arguments)]
fn ppc_standard_file_finish_put(
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    tracking: PpcStandardFilePutTrackingState,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    accepted: bool,
) -> PpcImportAction {
    ppc_standard_file_restore_pixels(memory, &tracking.saved_pixels, tracking.front_buffer);
    if accepted && !tracking.name.is_empty() {
        let replacing = ppc_fsspec_target_exists(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            tracking.dir_id,
            &tracking.name,
        );
        let vref = if tracking.call.mode == PpcStandardFileMode::PutLegacy {
            ppc_standard_file_working_directory_ref(
                tracking.vref,
                tracking.dir_id,
                vfs_volumes,
                working_directories,
                next_working_directory_ref_num,
            )
        } else {
            tracking.vref
        };
        ppc_standard_file_write_put_reply(
            memory,
            tracking.call.mode,
            tracking.call.reply,
            vref,
            tracking.dir_id,
            &tracking.name,
            replacing,
        );
    } else {
        ppc_standard_file_write_cancel_reply(memory, tracking.call.mode, tracking.call.reply);
    }
    startup.standard_file_put_tracking = None;
    PpcImportAction::ReturnPreserve
}

pub(super) fn ppc_standard_file_backspace_name(tracking: &mut PpcStandardFilePutTrackingState) {
    let start = tracking.sel_start.min(tracking.name.len());
    let end = tracking.sel_end.min(tracking.name.len()).max(start);
    if start < end {
        tracking.name.drain(start..end);
        tracking.sel_start = start;
        tracking.sel_end = start;
    } else if start > 0 {
        tracking.name.remove(start - 1);
        tracking.sel_start = start - 1;
        tracking.sel_end = start - 1;
    } else {
        tracking.sel_start = 0;
        tracking.sel_end = 0;
    }
}

pub(super) fn ppc_standard_file_insert_name_character(
    tracking: &mut PpcStandardFilePutTrackingState,
    character: u8,
) {
    let start = tracking.sel_start.min(tracking.name.len());
    let end = tracking.sel_end.min(tracking.name.len()).max(start);
    let retained_len = tracking.name.len().saturating_sub(end - start);
    if retained_len >= 63 {
        tracking.sel_start = start.min(63);
        tracking.sel_end = tracking.sel_start;
        return;
    }
    tracking.name.splice(start..end, [character]);
    tracking.name.truncate(63);
    tracking.sel_start = start.saturating_add(1).min(tracking.name.len());
    tracking.sel_end = tracking.sel_start;
}

#[allow(clippy::too_many_arguments)]
fn ppc_standard_file_get_service(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &ProcessVfsFileRecords,
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    event_queue: &mut EventQueue,
    gworlds: &[PpcGWorldRecord],
) -> PpcImportAction {
    let Some(mut tracking) = startup.standard_file_get_tracking.take() else {
        return PpcImportAction::ReturnPreserve;
    };
    if tracking.call.return_address != cpu.lr {
        return ppc_standard_file_finish_get(
            memory,
            startup,
            tracking,
            vfs_volumes,
            working_directories,
            next_working_directory_ref_num,
            false,
        );
    }
    let event = event_queue
        .iter()
        .position(|event| matches!(event.what, 1 | 3 | 5))
        .and_then(|index| event_queue.remove(index));
    let previous_dir_id = tracking.current_dir_id;
    let previous_selection = tracking.selected;
    let mut open = false;
    if let Some(event) = event {
        if event.what == 1 {
            let local = (
                event.where_v.saturating_sub(tracking.bounds.0),
                event.where_h.saturating_sub(tracking.bounds.1),
            );
            if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_GET_CANCEL_RECT) {
                return ppc_standard_file_finish_get(
                    memory,
                    startup,
                    tracking,
                    vfs_volumes,
                    working_directories,
                    next_working_directory_ref_num,
                    false,
                );
            }
            if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_GET_DESKTOP_RECT) {
                tracking.current_dir_id = PPC_ROOT_DIR_ID;
                tracking.entries = ppc_standard_file_get_entries(
                    vfs_directories,
                    vfs_files,
                    vfs_resource_files,
                    PPC_ROOT_DIR_ID,
                    tracking.file_types.as_deref(),
                );
                tracking.selected = 0;
            } else if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_GET_OPEN_RECT) {
                open = true;
            } else if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_GET_LIST_RECT)
                && !tracking.entries.is_empty()
            {
                let row = ((local.0 - PPC_STANDARD_FILE_GET_LIST_RECT.0 - 2)
                    / PPC_STANDARD_FILE_GET_ROW_HEIGHT)
                    .max(0) as usize;
                let first_visible = tracking.selected.saturating_sub(7);
                let index = first_visible.saturating_add(row);
                if index < tracking.entries.len() {
                    tracking.selected = index;
                }
            } else if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_GET_SCROLL_RECT)
                && !tracking.entries.is_empty()
            {
                let relative_v = local.0 - PPC_STANDARD_FILE_GET_SCROLL_RECT.0;
                let height =
                    PPC_STANDARD_FILE_GET_SCROLL_RECT.2 - PPC_STANDARD_FILE_GET_SCROLL_RECT.0;
                tracking.selected = if relative_v < 16 {
                    tracking.selected.saturating_sub(1)
                } else if relative_v >= height - 16 {
                    (tracking.selected + 1).min(tracking.entries.len() - 1)
                } else if relative_v < height / 2 {
                    tracking.selected.saturating_sub(8)
                } else {
                    (tracking.selected + 8).min(tracking.entries.len() - 1)
                };
            }
        } else {
            let character = event.message as u8;
            let key_code = (event.message >> 8) as u8;
            if character == b'\r'
                || character == 3
                || key_code == PPC_KEY_RETURN
                || key_code == PPC_KEY_NUMPAD_ENTER
            {
                open = true;
            } else if character == 0x1b || key_code == PPC_KEY_ESCAPE {
                return ppc_standard_file_finish_get(
                    memory,
                    startup,
                    tracking,
                    vfs_volumes,
                    working_directories,
                    next_working_directory_ref_num,
                    false,
                );
            } else if key_code == 0x7e || character == 0x1e {
                tracking.selected = tracking.selected.saturating_sub(1);
            } else if (key_code == 0x7d || character == 0x1f) && !tracking.entries.is_empty() {
                tracking.selected = (tracking.selected + 1).min(tracking.entries.len() - 1);
            }
        }
    }
    if open {
        if let Some(entry) = tracking.entries.get(tracking.selected) {
            if entry.is_directory {
                tracking.current_dir_id = entry.dir_id;
                tracking.entries = ppc_standard_file_get_entries(
                    vfs_directories,
                    vfs_files,
                    vfs_resource_files,
                    entry.dir_id,
                    tracking.file_types.as_deref(),
                );
                tracking.selected = 0;
            } else {
                return ppc_standard_file_finish_get(
                    memory,
                    startup,
                    tracking,
                    vfs_volumes,
                    working_directories,
                    next_working_directory_ref_num,
                    true,
                );
            }
        }
    }
    if tracking.current_dir_id != previous_dir_id || tracking.selected != previous_selection {
        ppc_standard_file_draw_get_dialog(memory, gworlds, &tracking);
    }
    startup.standard_file_get_tracking = Some(tracking);
    PpcImportAction::Yield(u64::MAX)
}

#[allow(clippy::too_many_arguments)]
fn ppc_standard_file_get_start(
    operation: PpcStandardFileOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &ProcessVfsFileRecords,
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    default_dir_id: u32,
    event_queue: &mut EventQueue,
    gworlds: &[PpcGWorldRecord],
    mode: PpcStandardFileMode,
    requested_origin: Option<(i16, i16)>,
) -> PpcImportAction {
    if let Some(mut filtering) = startup.standard_file_get_filtering.take() {
        if filtering.import_pc == cpu.pc && cpu.lr == cpu.pc {
            let candidate_index = filtering.next_entry;
            let accepted = cpu.gpr[3] & 0xff != 0;
            if accepted {
                filtering.next_entry = filtering.next_entry.saturating_add(1);
            } else if candidate_index < filtering.tracking.entries.len()
                && !filtering.tracking.entries[candidate_index].is_directory
            {
                filtering.tracking.entries.remove(candidate_index);
            }
            if let Some(action) = ppc_standard_file_filter_next_action(cpu, memory, &mut filtering)
            {
                startup.standard_file_get_filtering = Some(filtering);
                return action;
            }
            ppc_standard_file_dispose_filter_pb(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
                filtering.filter_pb,
            );
            let tracking = filtering.tracking;
            ppc_standard_file_draw_get_dialog(memory, gworlds, &tracking);
            startup.standard_file_get_tracking = Some(tracking);
            return PpcImportAction::Yield(u64::MAX);
        }
        ppc_standard_file_dispose_filter_pb(
            process_memory_manager,
            memory,
            heap_cursor,
            last_mem_error,
            filtering.filter_pb,
        );
        ppc_standard_file_restore_pixels(
            memory,
            &filtering.tracking.saved_pixels,
            filtering.tracking.front_buffer,
        );
        ppc_standard_file_write_cancel_reply(
            memory,
            filtering.tracking.call.mode,
            filtering.tracking.call.reply,
        );
        return PpcImportAction::ReturnPreserve;
    }
    if startup.standard_file_get_tracking.is_some() {
        return ppc_standard_file_get_service(
            cpu,
            memory,
            startup,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            vfs_volumes,
            working_directories,
            next_working_directory_ref_num,
            event_queue,
            gworlds,
        );
    }
    let (num_types, type_list) = match mode {
        PpcStandardFileMode::GetModern => (cpu.gpr[4] as u16 as i16, cpu.gpr[5]),
        PpcStandardFileMode::GetLegacy => (cpu.gpr[6] as u16 as i16, cpu.gpr[7]),
        PpcStandardFileMode::PutModern | PpcStandardFileMode::PutLegacy => {
            return PpcImportAction::ReturnPreserve
        }
    };
    let reply = ppc_standard_file_reply_ptr(mode, cpu);
    let Some(file_types) = ppc_standard_file_get_type_list(memory, num_types, type_list) else {
        ppc_standard_file_write_cancel_reply(memory, mode, reply);
        return PpcImportAction::ReturnPreserve;
    };
    let current_dir_id = ppc_directory_path_for_id(vfs_directories, default_dir_id)
        .map(|_| default_dir_id)
        .unwrap_or(PPC_ROOT_DIR_ID);
    let entries = ppc_standard_file_get_entries(
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        current_dir_id,
        file_types.as_deref(),
    );
    let Some(front_buffer) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) else {
        ppc_standard_file_write_cancel_reply(memory, mode, reply);
        return PpcImportAction::ReturnPreserve;
    };
    let bounds = ppc_standard_file_centered_bounds(
        front_buffer,
        PPC_STANDARD_FILE_GET_DIALOG_WIDTH,
        PPC_STANDARD_FILE_GET_DIALOG_HEIGHT,
        requested_origin,
    );
    let mut tracking = PpcStandardFileGetTrackingState {
        call: ppc_standard_file_call(mode, cpu),
        entries,
        current_dir_id,
        file_types,
        selected: 0,
        bounds,
        front_buffer,
        saved_pixels: ppc_standard_file_save_pixels(memory, front_buffer, bounds),
    };
    let (filter_ptr, callback_with_data) = operation.filter_pointer(cpu);
    if filter_ptr != 0 {
        if let Some(callback) = ppc_resolve_callback_target(memory, filter_ptr, cpu.gpr[2], None)
            .filter(|callback| memory.read_u32_be(callback.entry).is_some())
        {
            let filter_pb = process_memory_manager.new_native_ptr(
                memory,
                PPC_STANDARD_FILE_FILTER_PB_SIZE,
                true,
            );
            ppc_apply_process_native_allocator(
                process_memory_manager,
                memory,
                heap_cursor,
                last_mem_error,
            );
            if filter_pb != 0 {
                let mut filtering = PpcStandardFileFilteringState {
                    import_pc: cpu.pc,
                    restore_rtoc: cpu.gpr[2],
                    callback,
                    callback_with_data,
                    tracking,
                    next_entry: 0,
                    filter_pb,
                };
                if let Some(action) =
                    ppc_standard_file_filter_next_action(cpu, memory, &mut filtering)
                {
                    startup.standard_file_get_filtering = Some(filtering);
                    return action;
                }
                ppc_standard_file_dispose_filter_pb(
                    process_memory_manager,
                    memory,
                    heap_cursor,
                    last_mem_error,
                    filter_pb,
                );
                tracking = filtering.tracking;
            }
        }
    }
    ppc_standard_file_draw_get_dialog(memory, gworlds, &tracking);
    startup.standard_file_get_tracking = Some(tracking);
    PpcImportAction::Yield(u64::MAX)
}

#[allow(clippy::too_many_arguments)]
fn ppc_standard_file_put_start(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    default_dir_id: u32,
    gworlds: &[PpcGWorldRecord],
    mode: PpcStandardFileMode,
    prompt_ptr: u32,
    requested_origin: Option<(i16, i16)>,
) -> PpcImportAction {
    let name_ptr = match mode {
        PpcStandardFileMode::PutModern => cpu.gpr[4],
        PpcStandardFileMode::PutLegacy => cpu.gpr[5],
        PpcStandardFileMode::GetModern | PpcStandardFileMode::GetLegacy => {
            return PpcImportAction::ReturnPreserve
        }
    };
    let mut name = if name_ptr != 0 {
        ppc_read_pstring_bytes(memory, name_ptr).unwrap_or_default()
    } else {
        Vec::new()
    };
    if name.is_empty() {
        name.extend_from_slice(b"Untitled");
    }
    name.truncate(63);
    let reply = ppc_standard_file_reply_ptr(mode, cpu);
    let Some(front_buffer) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) else {
        ppc_standard_file_write_cancel_reply(memory, mode, reply);
        return PpcImportAction::ReturnPreserve;
    };
    let bounds = ppc_standard_file_centered_bounds(
        front_buffer,
        PPC_STANDARD_FILE_PUT_DIALOG_WIDTH,
        PPC_STANDARD_FILE_PUT_DIALOG_HEIGHT,
        requested_origin,
    );
    let tracking = PpcStandardFilePutTrackingState {
        call: ppc_standard_file_call(mode, cpu),
        vref: PPC_BOOT_VOLUME_REF_NUM,
        dir_id: default_dir_id,
        prompt: ppc_standard_file_prompt(memory, prompt_ptr),
        name: name.clone(),
        sel_start: 0,
        sel_end: name.len(),
        bounds,
        front_buffer,
        saved_pixels: ppc_standard_file_save_pixels(memory, front_buffer, bounds),
    };
    ppc_standard_file_draw_put_dialog(memory, gworlds, &tracking);
    startup.standard_file_put_tracking = Some(tracking);
    PpcImportAction::Yield(u64::MAX)
}

#[allow(clippy::too_many_arguments, clippy::ptr_arg)]
fn ppc_dispatch_standard_file(
    operation: PpcStandardFileOperation,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    gworlds: &[PpcGWorldRecord],
    vfs_directories: &mut Vec<PpcVfsDirectory>,
    vfs_files: &ProcessVfsFileRecords,
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_volumes: &[PpcVfsVolumeRecord],
    default_dir_id: u32,
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    event_queue: &mut EventQueue,
) -> PpcImportAction {
    let mode = operation.mode();
    let requested_origin = operation.requested_origin(cpu);
    match mode {
        PpcStandardFileMode::GetModern | PpcStandardFileMode::GetLegacy => {
            ppc_standard_file_get_start(
                operation,
                cpu,
                memory,
                startup,
                process_memory_manager,
                heap_cursor,
                last_mem_error,
                vfs_directories,
                vfs_files,
                vfs_resource_files,
                vfs_volumes,
                working_directories,
                next_working_directory_ref_num,
                default_dir_id,
                event_queue,
                gworlds,
                mode,
                requested_origin,
            )
        }
        PpcStandardFileMode::PutModern | PpcStandardFileMode::PutLegacy => {
            if let Some(mut tracking) = startup.standard_file_put_tracking.take() {
                if tracking.call.return_address != cpu.lr {
                    return ppc_standard_file_finish_put(
                        memory,
                        startup,
                        tracking,
                        vfs_directories,
                        vfs_files,
                        vfs_resource_files,
                        vfs_volumes,
                        working_directories,
                        next_working_directory_ref_num,
                        false,
                    );
                }
                let event = event_queue
                    .iter()
                    .position(|event| matches!(event.what, 1 | 3 | 5))
                    .and_then(|index| event_queue.remove(index));
                let mut accept = false;
                if let Some(event) = event {
                    if event.what == 1 {
                        let local = (
                            event.where_v.saturating_sub(tracking.bounds.0),
                            event.where_h.saturating_sub(tracking.bounds.1),
                        );
                        if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_PUT_CANCEL_RECT)
                        {
                            return ppc_standard_file_finish_put(
                                memory,
                                startup,
                                tracking,
                                vfs_directories,
                                vfs_files,
                                vfs_resource_files,
                                vfs_volumes,
                                working_directories,
                                next_working_directory_ref_num,
                                false,
                            );
                        }
                        accept =
                            ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_PUT_SAVE_RECT);
                    } else {
                        let character = event.message as u8;
                        let key_code = (event.message >> 8) as u8;
                        if character == b'\r'
                            || character == 3
                            || key_code == PPC_KEY_RETURN
                            || key_code == PPC_KEY_NUMPAD_ENTER
                        {
                            accept = true;
                        } else if character == 0x1b || key_code == PPC_KEY_ESCAPE {
                            return ppc_standard_file_finish_put(
                                memory,
                                startup,
                                tracking,
                                vfs_directories,
                                vfs_files,
                                vfs_resource_files,
                                vfs_volumes,
                                working_directories,
                                next_working_directory_ref_num,
                                false,
                            );
                        } else if character.eq_ignore_ascii_case(&b'a')
                            && event.modifiers & 0x0100 != 0
                        {
                            tracking.sel_start = 0;
                            tracking.sel_end = tracking.name.len();
                        } else if character == 0x08 || key_code == 0x33 {
                            ppc_standard_file_backspace_name(&mut tracking);
                        } else if event.modifiers & 0x0100 == 0
                            && (0x20..=0x7e).contains(&character)
                            && !matches!(character, b'/' | b':')
                            && tracking.sel_start <= tracking.sel_end
                            && tracking.sel_end <= tracking.name.len()
                        {
                            ppc_standard_file_insert_name_character(&mut tracking, character);
                        }
                    }
                }
                if accept {
                    return ppc_standard_file_finish_put(
                        memory,
                        startup,
                        tracking,
                        vfs_directories,
                        vfs_files,
                        vfs_resource_files,
                        vfs_volumes,
                        working_directories,
                        next_working_directory_ref_num,
                        true,
                    );
                }
                ppc_standard_file_draw_put_dialog(memory, gworlds, &tracking);
                startup.standard_file_put_tracking = Some(tracking);
                PpcImportAction::Yield(u64::MAX)
            } else {
                let prompt_ptr = operation.prompt_pointer(cpu);
                ppc_standard_file_put_start(
                    cpu,
                    memory,
                    startup,
                    default_dir_id,
                    gworlds,
                    mode,
                    prompt_ptr,
                    requested_origin,
                )
            }
        }
    }
}
