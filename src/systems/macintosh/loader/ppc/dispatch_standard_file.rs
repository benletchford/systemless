//! Typed Standard File Package dispatch for PowerPC imports.

use super::theme::*;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PpcStandardFileMode {
    GetModern,
    GetLegacy,
    PutModern,
    PutLegacy,
    /// Navigation Services NavPutFile, which shares the put dialog but
    /// returns an OSErr and a NavReplyRecord.
    PutNavigation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcStandardFileCall {
    pub(super) mode: PpcStandardFileMode,
    pub(super) reply: u32,
    pub(super) return_address: u32,
    /// The lowercase C glue (sfputfile) passes C strings and receives the
    /// reply's fName as a C string.
    pub(super) c_strings: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFileEntry {
    pub(super) name: Vec<u8>,
    pub(super) path: String,
    pub(super) dir_id: u32,
    pub(super) file_type: u32,
    pub(super) creator: u32,
    pub(super) finder_flags: u16,
    pub(super) is_directory: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFileGetTrackingState {
    pub(super) call: PpcStandardFileCall,
    pub(super) entries: Vec<PpcStandardFileEntry>,
    pub(super) current_dir_id: u32,
    pub(super) file_types: Option<Vec<u32>>,
    filter: Option<PpcStandardFileFilter>,
    pub(super) selected: usize,
    /// Current folder first, then each enclosing folder up to the volume.
    pub(super) folder_chain: Vec<(u32, Vec<u8>)>,
    pub(super) folder_popup_open: bool,
    last_list_click: Option<(u32, usize)>,
    pub(super) bounds: (i16, i16, i16, i16),
    pub(super) front_buffer: PpcFrontBuffer,
    pub(super) saved_pixels: crate::memory::SavedPixels<(i32, i32, u16)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PpcStandardFileFilter {
    callback: PpcCallbackTarget,
    callback_with_data: bool,
    your_data_ptr: u32,
    include_directories: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFileFilteringState {
    pub(super) import_pc: u32,
    pub(super) restore_rtoc: u32,
    pub(super) tracking: PpcStandardFileGetTrackingState,
    pub(super) next_entry: usize,
    pub(super) filter_pb: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PpcStandardFilePutTrackingState {
    pub(super) call: PpcStandardFileCall,
    pub(super) vref: i16,
    /// The folder the list shows, which receives the saved file.
    pub(super) dir_id: u32,
    /// Every folder and file in `dir_id`. The put list shows existing files
    /// so the user can see what a name would replace.
    pub(super) entries: Vec<PpcStandardFileEntry>,
    pub(super) selected: Option<usize>,
    /// Current folder first, then each enclosing folder up to the volume.
    pub(super) folder_chain: Vec<(u32, Vec<u8>)>,
    pub(super) folder_popup_open: bool,
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
    pub(super) heap_limit: u32,
    pub(super) handles: &'a mut Vec<PpcHandleRecord>,
    pub(super) apple_event_descriptors: &'a SharedProcessAppleEventDescriptors,
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
        heap_limit,
        handles,
        apple_event_descriptors,
    } = context;
    let mut navigation = PpcNavigationReplyContext {
        heap_limit,
        handles,
        descriptors: apple_event_descriptors,
    };

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
            &mut navigation,
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
                &mut navigation,
            ))
        }
        _ => None,
    }
}

/// The allocation state NavPutFile needs to build its reply's AEDescList.
pub(super) struct PpcNavigationReplyContext<'a> {
    heap_limit: u32,
    handles: &'a mut Vec<PpcHandleRecord>,
    descriptors: &'a SharedProcessAppleEventDescriptors,
}

// Navigation.h: NavDialogOptions fields (mac68k alignment).
const PPC_NAV_DIALOG_OPTIONS_SAVED_FILE_NAME_OFFSET: u32 = 1034;
const PPC_NAV_DIALOG_OPTIONS_MESSAGE_OFFSET: u32 = 1290;
// Navigation.h: NavReplyRecord is 256 bytes; selection is at +6.
const PPC_NAV_REPLY_RECORD_SIZE: usize = 256;
pub(super) const PPC_NAV_REPLY_SELECTION_OFFSET: u32 = 6;
const PPC_TYPE_NULL: u32 = u32::from_be_bytes(*b"null");
const PPC_TYPE_FSS: u32 = u32::from_be_bytes(*b"fss ");
const PPC_TYPE_AE_LIST: u32 = u32::from_be_bytes(*b"list");
const PPC_USER_CANCELED_ERR: i16 = -128;

// Navigation.h: kNavDefaultNavDlogOptions.
const PPC_NAV_DEFAULT_DIALOG_OPTION_FLAGS: u32 = 0x0000_00E4;
const PPC_NAV_DIALOG_OPTIONS_SIZE: usize = 2048;
const PPC_NAV_DIALOG_OPTIONS_CLIENT_NAME_OFFSET: u32 = 10;

/// NavGetDefaultDialogOptions(NavDialogOptions *) fills in version 0, the
/// default option flags, location (-1, -1) for the default position, and
/// the application's name as clientName; the other strings stay empty.
pub(super) fn ppc_nav_get_default_dialog_options(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    launched_app_path: Option<&str>,
) -> PpcImportAction {
    let options = cpu.gpr[3];
    if options == 0
        || memory
            .write_bytes(options, &[0; PPC_NAV_DIALOG_OPTIONS_SIZE])
            .is_none()
    {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    let _ = memory.write_u32_be(options + 2, PPC_NAV_DEFAULT_DIALOG_OPTION_FLAGS);
    let _ = memory.write_u32_be(options + 6, u32::MAX);
    let mut name = launched_app_path
        .map(ppc_vfs_basename_bytes)
        .unwrap_or_default();
    name.truncate(255);
    let _ = ppc_write_pstring_bytes(
        memory,
        options + PPC_NAV_DIALOG_OPTIONS_CLIENT_NAME_OFFSET,
        &name,
    );
    PpcImportAction::Return(ppc_i16_result(PPC_NO_ERR))
}

/// The result a put dialog returns when it is cancelled.
fn ppc_standard_file_put_cancelled(mode: PpcStandardFileMode) -> PpcImportAction {
    if mode == PpcStandardFileMode::PutNavigation {
        PpcImportAction::Return(ppc_i16_result(PPC_USER_CANCELED_ERR))
    } else {
        PpcImportAction::ReturnPreserve
    }
}

/// Writes NavPutFile's NavReplyRecord. An accepted dialog's selection is an
/// AEDescList holding the chosen file's FSSpec; a cancelled dialog leaves
/// validRecord false and a null selection.
#[allow(clippy::too_many_arguments)]
fn ppc_navigation_write_put_reply(
    memory: &mut PpcSectionMem,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    navigation: &mut PpcNavigationReplyContext<'_>,
    reply: u32,
    saved: Option<(i16, u32, &[u8], bool)>,
) -> i16 {
    if reply == 0
        || memory
            .write_bytes(reply, &[0; PPC_NAV_REPLY_RECORD_SIZE])
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    let selection = reply + PPC_NAV_REPLY_SELECTION_OFFSET;
    let _ = memory.write_u32_be(selection, PPC_TYPE_NULL);
    let Some((vref, dir_id, name, replacing)) = saved else {
        return PPC_USER_CANCELED_ERR;
    };
    let mut fsspec = vec![0; 70];
    fsspec[..2].copy_from_slice(&vref.to_be_bytes());
    fsspec[2..6].copy_from_slice(&dir_id.to_be_bytes());
    let length = name.len().min(63);
    fsspec[6] = length as u8;
    fsspec[7..7 + length].copy_from_slice(&name[..length]);
    let result = super::dispatch_apple_events::ppc_create_process_owned_ae_desc(
        process_memory_manager,
        memory,
        heap_cursor,
        navigation.heap_limit,
        last_mem_error,
        navigation.handles,
        Some(navigation.descriptors),
        selection,
        PPC_TYPE_AE_LIST,
        &[],
    );
    if result != PPC_NO_ERR {
        let _ = memory.write_u32_be(selection, PPC_TYPE_NULL);
        return result;
    }
    let list = ProcessAeDescriptor {
        desc_type: PPC_TYPE_AE_LIST,
        items: vec![(
            PPC_TYPE_WILDCARD,
            ProcessAeDescriptor {
                desc_type: PPC_TYPE_FSS,
                data: fsspec,
                ..Default::default()
            },
        )],
        ..Default::default()
    };
    super::dispatch_apple_events::ppc_store_ae_descriptor_semantics(
        memory,
        navigation.descriptors,
        selection,
        list,
    );
    // version 0 has no saveFileName; validRecord and replacing follow.
    let _ = memory.write_u8(reply + 2, 1);
    let _ = memory.write_u8(reply + 3, u8::from(replacing));
    PPC_NO_ERR
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
pub(super) const PPC_STANDARD_FILE_GET_FOLDER_MENU_RECT: (i16, i16, i16, i16) = (31, 90, 175, 250);
pub(super) const PPC_STANDARD_FILE_GET_FOLDER_MENU_ROW_HEIGHT: i16 = 16;
// The System 7 put dialog (DLOG -3999), laid out as the 68K Standard File
// package draws it. Inside Macintosh: Files (1992), pp. 3-8--3-9.
pub(super) const PPC_STANDARD_FILE_PUT_DIALOG_WIDTH: i16 = 360;
pub(super) const PPC_STANDARD_FILE_PUT_DIALOG_HEIGHT: i16 = 260;
pub(super) const PPC_STANDARD_FILE_PUT_VOLUME_RECT: (i16, i16, i16, i16) = (12, 90, 31, 164);
pub(super) const PPC_STANDARD_FILE_PUT_VOLUME_LABEL_RECT: (i16, i16, i16, i16) =
    (12, 258, 31, 354);
pub(super) const PPC_STANDARD_FILE_PUT_LIST_RECT: (i16, i16, i16, i16) = (38, 18, 156, 236);
pub(super) const PPC_STANDARD_FILE_PUT_SCROLL_RECT: (i16, i16, i16, i16) = (38, 235, 156, 251);
pub(super) const PPC_STANDARD_FILE_PUT_EJECT_RECT: (i16, i16, i16, i16) = (38, 258, 59, 338);
pub(super) const PPC_STANDARD_FILE_PUT_DESKTOP_RECT: (i16, i16, i16, i16) = (66, 258, 87, 338);
pub(super) const PPC_STANDARD_FILE_PUT_PROMPT_RECT: (i16, i16, i16, i16) = (166, 24, 184, 330);
pub(super) const PPC_STANDARD_FILE_PUT_NAME_RECT: (i16, i16, i16, i16) = (188, 24, 208, 330);
pub(super) const PPC_STANDARD_FILE_PUT_CANCEL_RECT: (i16, i16, i16, i16) = (220, 166, 242, 246);
pub(super) const PPC_STANDARD_FILE_PUT_SAVE_RECT: (i16, i16, i16, i16) = (220, 258, 242, 338);
/// Both Standard File lists show eight rows.
const PPC_STANDARD_FILE_VISIBLE_ROWS: usize = 8;

fn ppc_standard_file_reply_ptr(mode: PpcStandardFileMode, cpu: &PpcCpu) -> u32 {
    match mode {
        PpcStandardFileMode::GetModern => cpu.gpr[6],
        PpcStandardFileMode::GetLegacy => cpu.gpr[9],
        PpcStandardFileMode::PutModern => cpu.gpr[5],
        PpcStandardFileMode::PutLegacy => cpu.gpr[7],
        PpcStandardFileMode::PutNavigation => cpu.gpr[4],
    }
}

fn ppc_standard_file_call(
    mode: PpcStandardFileMode,
    cpu: &PpcCpu,
    c_strings: bool,
) -> PpcStandardFileCall {
    PpcStandardFileCall {
        mode,
        reply: ppc_standard_file_reply_ptr(mode, cpu),
        return_address: cpu.lr,
        c_strings,
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
            creator: directory.creator,
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
            creator: file.creator,
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
            creator: fork.creator,
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

fn ppc_standard_file_folder_chain(
    vfs_directories: &[PpcVfsDirectory],
    dir_id: u32,
) -> Vec<(u32, Vec<u8>)> {
    let mut chain = Vec::new();
    let mut current = dir_id;
    while current != PPC_ROOT_DIR_ID && chain.len() < 32 {
        let Some(directory) = vfs_directories
            .iter()
            .find(|directory| directory.dir_id == current)
        else {
            break;
        };
        chain.push((
            current,
            encode_mac_roman_lossy(ppc_vfs_basename(&directory.path))
                .into_iter()
                .take(63)
                .collect(),
        ));
        current = directory.parent_dir_id;
    }
    chain.push((
        PPC_ROOT_DIR_ID,
        crate::trap::dispatch::BOOT_VOLUME_NAME.as_bytes().to_vec(),
    ));
    chain
}

/// Index into `folder_chain` of the popup row under a dialog-local point.
/// Horizontal chrome of the folder popup around its label: the label starts
/// 15 pixels inside the box, the arrow takes the last 19, and 8 more keep the
/// arrow clear of the label (as the 68K Standard File package draws it).
const PPC_STANDARD_FILE_FOLDER_POPUP_CHROME: i16 = 42;

fn ppc_standard_file_folder_label(folder_chain: &[(u32, Vec<u8>)]) -> &[u8] {
    folder_chain
        .first()
        .map_or(&[][..], |(_, name)| name.as_slice())
}

/// The folder popup, sized to the folder's name and centred over the list:
/// never narrower than the dialog's popup item, never wider than the list.
pub(super) fn ppc_standard_file_folder_popup_rect(
    base: (i16, i16, i16, i16),
    list: (i16, i16, i16, i16),
    folder_chain: &[(u32, Vec<u8>)],
) -> (i16, i16, i16, i16) {
    let list_width = list.3 - list.1;
    let width = ppc_text_bytes_advance_for_font(
        ppc_standard_file_folder_label(folder_chain),
        PPC_QD_TEXT_FONT_DEFAULT,
        PPC_QD_TEXT_SIZE_SYSTEM,
    )
    .saturating_add(PPC_STANDARD_FILE_FOLDER_POPUP_CHROME)
    .clamp(base.3 - base.1, list_width);
    let left = list.1 + (list_width - width) / 2;
    (base.0, left, base.2, left + width)
}

/// The popup's menu drops from the popup's bottom-left corner.
fn ppc_standard_file_folder_menu_rect(popup: (i16, i16, i16, i16)) -> (i16, i16, i16, i16) {
    let menu = PPC_STANDARD_FILE_GET_FOLDER_MENU_RECT;
    let width = (menu.3 - menu.1).max(popup.3 - popup.1);
    (popup.2, popup.1, popup.2 + (menu.2 - menu.0), popup.1 + width)
}

fn ppc_standard_file_folder_menu_row(
    folder_chain: &[(u32, Vec<u8>)],
    popup: (i16, i16, i16, i16),
    local: (i16, i16),
) -> Option<usize> {
    let menu = ppc_standard_file_folder_menu_rect(popup);
    if !ppc_standard_file_point_in_rect(local, menu) {
        return None;
    }
    let row = ((local.0 - menu.0) / PPC_STANDARD_FILE_GET_FOLDER_MENU_ROW_HEIGHT) as usize;
    (row < folder_chain.len()).then_some(row)
}

/// The folder popup's menu, dropped below the popup button. The get and put
/// dialogs place that button at the same spot.
fn ppc_standard_file_draw_folder_menu(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    front: PpcFrontBuffer,
    bounds: (i16, i16, i16, i16),
    popup: (i16, i16, i16, i16),
    folder_chain: &[(u32, Vec<u8>)],
) {
    let menu_rect = ppc_standard_file_folder_menu_rect(popup);
    // Rows past the dialog's bottom edge are not shown; the menu does not scroll.
    let rows = (menu_rect.2 - menu_rect.0) / PPC_STANDARD_FILE_GET_FOLDER_MENU_ROW_HEIGHT;
    let rows = folder_chain.len().min(rows as usize) as i16;
    let mut menu = ppc_standard_file_global_rect(bounds, menu_rect);
    menu.2 = menu
        .0
        .saturating_add(rows * PPC_STANDARD_FILE_GET_FOLDER_MENU_ROW_HEIGHT);
    let _ = ppc_fill_front_rect(memory, front, menu, PPC_RGB_WHITE);
    let _ = ppc_frame_front_rect(memory, front, menu, PPC_RGB_BLACK, 1);
    for row in 0..rows {
        let top = menu.0 + row * PPC_STANDARD_FILE_GET_FOLDER_MENU_ROW_HEIGHT;
        ppc_draw_dialog_text(
            memory,
            gworlds,
            (
                top.saturating_add(1),
                menu.1.saturating_add(5),
                top + PPC_STANDARD_FILE_GET_FOLDER_MENU_ROW_HEIGHT,
                menu.3.saturating_sub(3),
            ),
            &folder_chain[row as usize].1,
            PPC_RGB_BLACK,
        );
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcStandardFileOperation {
    StandardGetFile,
    CustomGetFile,
    CustomPutFile,
    SfGetFile,
    SfpGetFile,
    SfpPutFile,
    SfPutFile,
    /// sfputfile(Point *where, const char *prompt, const char *origName,
    /// DlgHookUPP dlgHook, SFReply *reply): the C glue for SFPutFile.
    SfPutFileCString,
    StandardPutFile,
    /// NavPutFile(defaultLocation, reply, dialogOptions, eventProc,
    /// fileType, fileCreator, callBackUD).
    NavPutFile,
}

impl PpcStandardFileOperation {
    fn mode(self) -> PpcStandardFileMode {
        match self {
            Self::StandardGetFile | Self::CustomGetFile => PpcStandardFileMode::GetModern,
            Self::SfGetFile | Self::SfpGetFile => PpcStandardFileMode::GetLegacy,
            Self::CustomPutFile | Self::StandardPutFile => PpcStandardFileMode::PutModern,
            Self::SfpPutFile | Self::SfPutFile | Self::SfPutFileCString => {
                PpcStandardFileMode::PutLegacy
            }
            Self::NavPutFile => PpcStandardFileMode::PutNavigation,
        }
    }

    fn requested_origin(self, cpu: &PpcCpu, memory: &mut PpcSectionMem) -> Option<(i16, i16)> {
        let point = match self {
            Self::SfPutFileCString => memory.read_u32_be(cpu.gpr[3]).unwrap_or(u32::MAX),
            Self::SfGetFile | Self::SfpGetFile | Self::SfpPutFile | Self::SfPutFile => cpu.gpr[3],
            Self::CustomGetFile => cpu.gpr[8],
            Self::CustomPutFile => cpu.gpr[7],
            Self::StandardGetFile | Self::StandardPutFile | Self::NavPutFile => return None,
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
            Self::CustomPutFile
            | Self::SfpPutFile
            | Self::SfPutFile
            | Self::SfPutFileCString
            | Self::StandardPutFile
            | Self::NavPutFile => (0, false),
        }
    }

    fn prompt_pointer(self, cpu: &PpcCpu) -> u32 {
        match self {
            Self::CustomPutFile | Self::StandardPutFile => cpu.gpr[3],
            Self::SfpPutFile | Self::SfPutFile | Self::SfPutFileCString => cpu.gpr[4],
            // NavDialogOptions.message is the prompt above the name field.
            Self::NavPutFile if cpu.gpr[5] != 0 => {
                cpu.gpr[5] + PPC_NAV_DIALOG_OPTIONS_MESSAGE_OFFSET
            }
            Self::NavPutFile => 0,
            Self::StandardGetFile | Self::CustomGetFile | Self::SfGetFile | Self::SfpGetFile => 0,
        }
    }
}

fn ppc_standard_file_string(memory: &mut PpcSectionMem, ptr: u32, c_string: bool) -> Vec<u8> {
    if c_string {
        ppc_std_c_string(memory, ptr, 255)
    } else {
        ppc_read_pstring_bytes(memory, ptr).unwrap_or_default()
    }
}

fn ppc_standard_file_prompt(
    memory: &mut PpcSectionMem,
    prompt_ptr: u32,
    c_string: bool,
) -> Vec<u8> {
    if prompt_ptr == 0 {
        return b"Save as:".to_vec();
    }
    let prompt = ppc_standard_file_string(memory, prompt_ptr, c_string);
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
    // Inside Macintosh: Files (1992), Catalog Information Parameter Block:
    // ioFlAttrib is byte 30, and bit 4 identifies a directory.
    let name_ptr = filter_pb.saturating_add(PPC_STANDARD_FILE_FILTER_NAME_OFFSET);
    ppc_write_pstring_bytes(memory, name_ptr, &entry.name)
        && memory.write_u32_be(filter_pb + 18, name_ptr).is_some()
        && memory
            .write_u16_be(filter_pb + 22, PPC_BOOT_VOLUME_REF_NUM as u16)
            .is_some()
        && memory.write_u16_be(filter_pb + 28, 0).is_some()
        && memory
            .write_u8(filter_pb + 30, if entry.is_directory { 0x10 } else { 0 })
            .is_some()
        && memory.write_u32_be(filter_pb + 48, entry.dir_id).is_some()
        && ppc_write_finfo(
            memory,
            filter_pb + 32,
            entry.file_type,
            entry.creator,
            entry.finder_flags,
        )
        .is_some()
}

fn ppc_standard_file_filter_next_action(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    state: &mut PpcStandardFileFilteringState,
) -> Option<PpcImportAction> {
    let filter = state.tracking.filter?;
    while !filter.include_directories
        && state
            .tracking
            .entries
            .get(state.next_entry)
            .is_some_and(|entry| entry.is_directory)
    {
        state.next_entry = state.next_entry.saturating_add(1);
    }
    let entry = state.tracking.entries.get(state.next_entry)?;
    if !ppc_standard_file_write_filter_pb(memory, state.filter_pb, entry) {
        return None;
    }
    let arguments = if filter.callback_with_data {
        vec![state.filter_pb, filter.your_data_ptr]
    } else {
        vec![state.filter_pb]
    };
    install_powerpc_call_arguments(cpu, memory, &arguments)?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: filter.callback.entry,
            rtoc: filter.callback.rtoc,
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

fn ppc_standard_file_begin_filtering(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    tracking: PpcStandardFileGetTrackingState,
) -> Result<PpcImportAction, PpcStandardFileGetTrackingState> {
    if tracking.filter.is_none() {
        return Err(tracking);
    }
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
    if filter_pb == 0 {
        return Err(tracking);
    }
    let mut filtering = PpcStandardFileFilteringState {
        import_pc: cpu.pc,
        restore_rtoc: cpu.gpr[2],
        tracking,
        next_entry: 0,
        filter_pb,
    };
    if let Some(action) = ppc_standard_file_filter_next_action(cpu, memory, &mut filtering) {
        startup.standard_file_get_filtering = Some(filtering);
        return Ok(action);
    }
    ppc_standard_file_dispose_filter_pb(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        filter_pb,
    );
    Err(filtering.tracking)
}

// Pixel-exact ports of the 68K Standard File package's classic (System 7
// theme) drawing in trap/dialog.rs and trap/toolbox.rs, so a PowerPC
// application's Standard File dialogs match its 68K twin. Other themes draw
// controls through the shared theme provider, as the 68K side does.

/// The font the Standard File package draws with: the system font, 12 point.
const PPC_SF_FONT: i16 = 0;
const PPC_SF_SIZE: i16 = 12;

fn ppc_sf_classic(gworlds: &[PpcGWorldRecord]) -> bool {
    ppc_ui_theme(gworlds) == UiThemeId::ClassicSystem7
}

fn ppc_sf_metrics() -> crate::quickdraw::fonts::FontMetrics {
    crate::quickdraw::text::get_font_metrics(PPC_SF_FONT, PPC_SF_SIZE)
}

fn ppc_sf_measure(bytes: &[u8]) -> i16 {
    ppc_text_bytes_advance_for_font(bytes, PPC_SF_FONT, PPC_SF_SIZE)
}

/// The screen's colour table on an indexed screen.
fn ppc_sf_screen_clut(memory: &mut PpcSectionMem, front: PpcFrontBuffer) -> Option<[[u16; 3]; 256]> {
    if front.depth != 8 || front.base_addr != PPC_MAIN_SCREEN_BASE {
        return None;
    }
    let fallback = TrapDispatcher::standard_mac_8bpp_clut();
    Some(ppc_read_ctable_clut(memory, PPC_MAIN_CTABLE_HANDLE, &fallback).unwrap_or(fallback))
}

/// The pixel the 68K package draws black or white with. On an indexed
/// screen that is not simply the nearest colour: black is entry 1 when it
/// is black (else 255 when black, else the darkest entry) and white is
/// entry 0 when it is white (else the brightest), lowest index on ties.
/// The difference shows wherever pixels are inverted.
fn ppc_sf_ink(memory: &mut PpcSectionMem, front: PpcFrontBuffer, black: bool) -> Option<u16> {
    let Some(clut) = ppc_sf_screen_clut(memory, front) else {
        return ppc_front_rgb_pixel(
            memory,
            front,
            if black { PPC_RGB_BLACK } else { PPC_RGB_WHITE },
        );
    };
    let luma = |index: usize| clut[index].iter().map(|&value| u32::from(value)).sum::<u32>();
    if black && luma(1) == 0 {
        return Some(1);
    }
    if black && luma(255) == 0 {
        return Some(255);
    }
    if !black && luma(0) == 0xffff * 3 {
        return Some(0);
    }
    let best = (0..256).fold(0usize, |best, index| {
        let better = if black {
            luma(index) < luma(best)
        } else {
            luma(index) > luma(best)
        };
        if better { index } else { best }
    });
    Some(best as u16)
}

/// The entry nearest 0xAAAA gray, lowest index on ties, which the 68K
/// package draws disabled button titles with on an indexed screen.
fn ppc_sf_gray_index(memory: &mut PpcSectionMem, front: PpcFrontBuffer) -> Option<u8> {
    let clut = ppc_sf_screen_clut(memory, front)?;
    (0..256usize)
        .min_by_key(|&index| {
            clut[index]
                .iter()
                .map(|&value| (i32::from(value) - 0xaaaa).unsigned_abs())
                .sum::<u32>()
        })
        .map(|index| index as u8)
}

fn ppc_sf_fill(memory: &mut PpcSectionMem, front: PpcFrontBuffer, rect: (i16, i16, i16, i16), black: bool) {
    let top = i32::from(rect.0).max(0);
    let left = i32::from(rect.1).max(0);
    let bottom = i32::from(rect.2).min(front.height as i32);
    let right = i32::from(rect.3).min(front.width as i32);
    if top >= bottom || left >= right {
        return;
    }
    let Some(pixel) = ppc_sf_ink(memory, front, black) else {
        return;
    };
    for y in top..bottom {
        for x in left..right {
            let _ = ppc_quickdraw_write_raw_pixel(memory, front, (x, y), pixel);
        }
    }
}

/// A one-pixel black frame just inside `rect` (draw_rect_border).
fn ppc_sf_border(memory: &mut PpcSectionMem, front: PpcFrontBuffer, rect: (i16, i16, i16, i16)) {
    let (top, left, bottom, right) = rect;
    ppc_sf_fill(memory, front, (top, left, top + 1, right - 1), true);
    ppc_sf_fill(memory, front, (bottom - 1, left, bottom, right - 1), true);
    ppc_sf_fill(memory, front, (top, left, bottom, left + 1), true);
    ppc_sf_fill(memory, front, (top, right - 1, bottom, right), true);
}

/// Text in the Standard File font with its baseline at `pen`, clipped to
/// `clip` when given.
fn ppc_sf_text(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    pen: (i16, i16),
    bytes: &[u8],
    color: PpcRgbColor,
    clip: Option<(i16, i16, i16, i16)>,
) {
    let explicit_index = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD).and_then(|front| {
        ppc_sf_screen_clut(memory, front)?;
        if color == PPC_SF_DISABLED_GRAY {
            ppc_sf_gray_index(memory, front)
        } else {
            ppc_sf_ink(memory, front, color == PPC_RGB_BLACK).map(|pixel| pixel as u8)
        }
    });
    let _ = ppc_with_unclipped_screen_port(memory, |memory| {
        ppc_draw_text_bytes_clipped(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            pen,
            PPC_SF_FONT,
            PPC_SF_SIZE,
            PPC_QD_TEXT_MODE_SRC_OR,
            color,
            explicit_index,
            clip,
            bytes,
        )
    });
}

const PPC_SF_DISABLED_GRAY: PpcRgbColor = PpcRgbColor {
    red: 0xaaaa,
    green: 0xaaaa,
    blue: 0xaaaa,
};

/// Inverts every pixel of `rect` (InvertRect on the screen's pixel values).
fn ppc_sf_invert(memory: &mut PpcSectionMem, front: PpcFrontBuffer, rect: (i16, i16, i16, i16)) {
    let mask: u16 = match front.depth {
        8 => 0x00ff,
        16 => 0x7fff,
        _ => return,
    };
    for y in i32::from(rect.0).max(0)..i32::from(rect.2).min(front.height as i32) {
        for x in i32::from(rect.1).max(0)..i32::from(rect.3).min(front.width as i32) {
            if let Some(pixel) = ppc_quickdraw_read_pixel(memory, front, (x, y)) {
                let _ = ppc_quickdraw_write_raw_pixel(memory, front, (x, y), !pixel & mask);
            }
        }
    }
}

/// A static text item (TextBox): wrapped to the item, drawn one pixel in,
/// first baseline at the font's ascent, clipped to the item's bottom right.
fn ppc_sf_static_text(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    rect: (i16, i16, i16, i16),
    bytes: &[u8],
) {
    let metrics = ppc_sf_metrics();
    let line_height = metrics.ascent + metrics.descent + metrics.leading;
    let max_width = (rect.3 - rect.1).saturating_sub(crate::dialog_manager::DIALOG_TEXT_LEFT_INSET);
    let mut y = rect.0 + metrics.ascent;
    if y >= rect.2 && rect.2 > rect.0 {
        y = rect.2 - 1;
    }
    for line in crate::quickdraw::text::wrap_classic_text(bytes, max_width.max(1), |_, byte| {
        ppc_text_byte_advance_for_font(byte, PPC_SF_FONT, PPC_SF_SIZE)
    }) {
        let mut end = line.visible_end;
        while end > line.start && matches!(bytes[end - 1], b' ' | b'\r' | b'\n') {
            end -= 1;
        }
        if end > line.start && y <= rect.2 {
            ppc_sf_text(
                memory,
                gworlds,
                (rect.1 + crate::dialog_manager::DIALOG_TEXT_LEFT_INSET, y),
                &bytes[line.start..end],
                PPC_RGB_BLACK,
                Some((i16::MIN, i16::MIN, rect.2, rect.3)),
            );
        }
        y += line_height;
        if y > rect.2 {
            break;
        }
    }
}

/// A push button (draw_button_state): the classic rounded outline, the
/// default item's three-pixel ring, and the centred title (gray when
/// disabled).
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
    let (top, left, bottom, right) = ppc_standard_file_global_rect(bounds, rect);
    if !ppc_sf_classic(gworlds) {
        ppc_draw_retained_control_rect(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            (top, left, bottom, right),
            crate::ui_theme::ControlKind::PushButton,
            enabled,
            is_default,
        );
    } else if bottom - top < 8 || right - left < 8 {
        ppc_sf_fill(memory, front, (top, left, bottom, right), false);
        ppc_sf_border(memory, front, (top, left, bottom, right));
    } else {
        let rows: &[(i16, i16, i16, i16, bool)] = if bottom - top >= 24 {
            &[
                (top, left + 4, top + 1, right - 4, false),
                (top + 1, left + 2, top + 2, right - 2, false),
                (top + 2, left + 1, top + 4, right - 1, false),
                (top + 4, left, bottom - 4, right, false),
                (bottom - 4, left + 1, bottom - 2, right - 1, false),
                (bottom - 2, left + 2, bottom - 1, right - 2, false),
                (bottom - 1, left + 4, bottom, right - 4, false),
                (top, left + 4, top + 1, right - 4, true),
                (top + 1, left + 2, top + 2, left + 4, true),
                (top + 1, right - 4, top + 2, right - 2, true),
                (top + 2, left + 1, top + 3, left + 2, true),
                (top + 2, right - 2, top + 3, right - 1, true),
                (top + 3, left + 1, top + 4, left + 2, true),
                (top + 3, right - 2, top + 4, right - 1, true),
                (top + 4, left, bottom - 4, left + 1, true),
                (top + 4, right - 1, bottom - 4, right, true),
                (bottom - 4, left + 1, bottom - 3, left + 2, true),
                (bottom - 4, right - 2, bottom - 3, right - 1, true),
                (bottom - 3, left + 1, bottom - 2, left + 2, true),
                (bottom - 3, right - 2, bottom - 2, right - 1, true),
                (bottom - 2, left + 2, bottom - 1, left + 4, true),
                (bottom - 2, right - 4, bottom - 1, right - 2, true),
                (bottom - 1, left + 4, bottom, right - 4, true),
            ]
        } else {
            &[
                (top, left + 3, top + 1, right - 3, false),
                (top + 1, left + 1, top + 3, right - 1, false),
                (top + 3, left, bottom - 3, right, false),
                (bottom - 3, left + 1, bottom - 1, right - 1, false),
                (bottom - 1, left + 3, bottom, right - 3, false),
                (top, left + 3, top + 1, right - 3, true),
                (top + 1, left + 1, top + 2, left + 3, true),
                (top + 1, right - 3, top + 2, right - 1, true),
                (top + 2, left + 1, top + 3, left + 2, true),
                (top + 2, right - 2, top + 3, right - 1, true),
                (top + 3, left, bottom - 3, left + 1, true),
                (top + 3, right - 1, bottom - 3, right, true),
                (bottom - 3, left + 1, bottom - 2, left + 2, true),
                (bottom - 3, right - 2, bottom - 2, right - 1, true),
                (bottom - 2, left + 1, bottom - 1, left + 3, true),
                (bottom - 2, right - 3, bottom - 1, right - 1, true),
                (bottom - 1, left + 3, bottom, right - 3, true),
            ]
        };
        for &(row_top, row_left, row_bottom, row_right, black) in rows {
            ppc_sf_fill(memory, front, (row_top, row_left, row_bottom, row_right), black);
        }
        if is_default {
            let outline =
                crate::dialog_manager::evaluate_dialog_default_button_outline((top, left, bottom, right));
            let _ = ppc_frame_front_round_rect(
                memory,
                front,
                outline.outer_rect(),
                outline.oval_width(),
                outline.thickness(),
                PPC_RGB_BLACK,
            );
        }
    }
    let metrics = ppc_sf_metrics();
    let text_x = left + (right - left - ppc_sf_measure(label)) / 2;
    let text_y = top + (bottom - top - (metrics.ascent + metrics.descent)) / 2 + metrics.ascent;
    ppc_sf_text(
        memory,
        gworlds,
        (text_x, text_y),
        label,
        if enabled { PPC_RGB_BLACK } else { PPC_SF_DISABLED_GRAY },
        None,
    );
}

/// The Standard File dialog window: plainDBox, its content erased to white
/// and a one-pixel frame just outside it.
fn ppc_sf_dialog_frame(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    front: PpcFrontBuffer,
    bounds: (i16, i16, i16, i16),
) {
    let frame = (bounds.0 - 1, bounds.1 - 1, bounds.2 + 1, bounds.3 + 1);
    if ppc_sf_classic(gworlds) || !ppc_draw_themed_dialog_frame(memory, gworlds, bounds, frame, 2) {
        ppc_sf_fill(memory, front, bounds, false);
        ppc_sf_border(memory, front, frame);
    }
}

/// The first list row shown: the list scrolls just far enough to show the
/// selection on its last row.
fn ppc_standard_file_first_visible(selected: Option<usize>) -> usize {
    selected
        .unwrap_or(0)
        .saturating_sub(PPC_STANDARD_FILE_VISIBLE_ROWS - 1)
}

fn ppc_standard_file_draw_scrollbar(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    bounds: (i16, i16, i16, i16),
    scroll_rect: (i16, i16, i16, i16),
    entry_count: usize,
    selected: Option<usize>,
) {
    let max = entry_count.saturating_sub(PPC_STANDARD_FILE_VISIBLE_ROWS);
    let first_visible = ppc_standard_file_first_visible(selected);
    ppc_draw_retained_scrollbar_rect(
        memory,
        gworlds,
        PPC_MAIN_GWORLD,
        ppc_standard_file_global_rect(bounds, scroll_rect),
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

/// Left inset of an entry name inside the Standard File list, leaving room
/// for the small folder or document glyph drawn before it.
const PPC_STANDARD_FILE_GET_NAME_INSET: i16 = 20;

/// Draw the small monochrome folder or document glyph that precedes each
/// Standard File list entry (the same glyphs the 68k list uses). Selected
/// rows are inverted, so the glyph is drawn white on the black highlight.
fn ppc_standard_file_draw_entry_icon(
    memory: &mut PpcSectionMem,
    front: PpcFrontBuffer,
    origin: (i16, i16),
    directory: bool,
    selected: bool,
) {
    use crate::systems::macintosh::ui_art::{
        STANDARD_FILE_DOCUMENT_ICON_ROWS, STANDARD_FILE_FOLDER_ICON_ROWS,
        STANDARD_FILE_ICON_WIDTH,
    };
    let rows = if directory {
        STANDARD_FILE_FOLDER_ICON_ROWS
    } else {
        STANDARD_FILE_DOCUMENT_ICON_ROWS
    };
    let ink = if selected { PPC_RGB_WHITE } else { PPC_RGB_BLACK };
    let Some(pixel) = ppc_front_rgb_pixel(memory, front, ink) else {
        return;
    };
    let (top, left) = (i32::from(origin.0), i32::from(origin.1));
    for (y, row) in rows.into_iter().enumerate() {
        for x in 0..STANDARD_FILE_ICON_WIDTH {
            if row & (1 << (STANDARD_FILE_ICON_WIDTH - 1 - x)) != 0 {
                let _ = ppc_quickdraw_write_raw_pixel(
                    memory,
                    front,
                    (left + i32::from(x), top + y as i32),
                    pixel,
                );
            }
        }
    }
}

/// The Standard File list: a folder or document glyph and the name of each
/// visible entry, with the selection inverted (draw_standard_file_get_list
/// and draw_standard_file_put_list).
fn ppc_standard_file_draw_list(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    front: PpcFrontBuffer,
    bounds: (i16, i16, i16, i16),
    list_rect: (i16, i16, i16, i16),
    entries: &[PpcStandardFileEntry],
    selected: Option<usize>,
) {
    let (top, left, bottom, right) = ppc_standard_file_global_rect(bounds, list_rect);
    ppc_sf_fill(memory, front, (top, left, bottom, right), false);
    ppc_sf_fill(memory, front, (top, left, top + 1, right), true);
    ppc_sf_fill(memory, front, (bottom - 1, left, bottom, right), true);
    ppc_sf_fill(memory, front, (top, left, bottom, left + 1), true);
    ppc_sf_fill(memory, front, (top, right - 1, bottom, right), true);
    let selected = selected.filter(|_| !entries.is_empty()).map(|index| index.min(entries.len() - 1));
    let first_visible = ppc_standard_file_first_visible(selected);
    for row in 0..PPC_STANDARD_FILE_VISIBLE_ROWS {
        let index = first_visible + row;
        let Some(entry) = entries.get(index) else {
            break;
        };
        let row_top = top + 2 + row as i16 * PPC_STANDARD_FILE_GET_ROW_HEIGHT;
        let row_bottom = (row_top + PPC_STANDARD_FILE_GET_ROW_HEIGHT).min(bottom - 1);
        let is_selected = selected == Some(index);
        if is_selected {
            ppc_sf_fill(memory, front, (row_top, left + 2, row_bottom, right - 2), true);
        }
        ppc_standard_file_draw_entry_icon(
            memory,
            front,
            (row_top + 1, left + 4),
            entry.is_directory,
            is_selected,
        );
        // Names past 36 characters end in an ellipsis.
        let name = if entry.name.len() > 36 {
            [&entry.name[..36], b"..."].concat()
        } else {
            entry.name.clone()
        };
        ppc_sf_text(
            memory,
            gworlds,
            (left + PPC_STANDARD_FILE_GET_NAME_INSET, row_top + 11),
            &name,
            if is_selected { PPC_RGB_WHITE } else { PPC_RGB_BLACK },
            None,
        );
    }
}

/// `title` cut with an ellipsis to fit `available_width`
/// (popup_control_display_title).
fn ppc_sf_popup_title(title: &[u8], available_width: i16) -> Vec<u8> {
    if available_width <= 0 {
        return Vec::new();
    }
    if ppc_sf_measure(title) <= available_width {
        return title.to_vec();
    }
    let ellipsis_width = ppc_sf_measure(b"...");
    if ellipsis_width > available_width {
        return Vec::new();
    }
    let mut prefix = Vec::new();
    let mut prefix_width = 0;
    for &byte in title {
        let width = ppc_sf_measure(&[byte]);
        if prefix_width + width + ellipsis_width > available_width {
            break;
        }
        prefix.push(byte);
        prefix_width += width;
    }
    prefix.extend_from_slice(b"...");
    prefix
}

/// The folder popup (draw_popup_control): a white box with a one-pixel
/// frame, a drop shadow below and to the right, the downward triangle, and
/// the folder name. Macintosh Toolbox Essentials (1992), pp. 5-26--5-27.
/// The volume's name stands beside it as a static text item.
fn ppc_standard_file_draw_folder_popup(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    bounds: (i16, i16, i16, i16),
    popup: (i16, i16, i16, i16),
    label_rect: (i16, i16, i16, i16),
    folder_chain: &[(u32, Vec<u8>)],
) {
    let Some(front) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) else {
        return;
    };
    let (top, left, bottom, right) = ppc_standard_file_global_rect(bounds, popup);
    if !ppc_sf_classic(gworlds) {
        ppc_draw_retained_control_rect(
            memory,
            gworlds,
            PPC_MAIN_GWORLD,
            (top, left, bottom, right),
            crate::ui_theme::ControlKind::PopupButton,
            true,
            false,
        );
    } else {
        ppc_sf_fill(memory, front, (top, left, bottom, right + 1), false);
        ppc_sf_fill(memory, front, (top, left, top + 1, right), true);
        ppc_sf_fill(memory, front, (bottom - 2, left, bottom - 1, right), true);
        ppc_sf_fill(memory, front, (top, left, bottom - 1, left + 1), true);
        ppc_sf_fill(memory, front, (top, right - 1, bottom - 1, right), true);
        ppc_sf_fill(memory, front, (top + 3, right, bottom - 1, right + 1), true);
        ppc_sf_fill(memory, front, (bottom - 1, left + 3, bottom, right + 1), true);
        let tri_x = right - 12;
        for row in 0..6 {
            ppc_sf_fill(
                memory,
                front,
                (top + 6 + row, tri_x - 5 + row, top + 7 + row, tri_x + 6 - row),
                true,
            );
        }
    }
    let text_x = left + 15;
    let text_right = (right - 19).max(text_x);
    let title = ppc_sf_popup_title(ppc_standard_file_folder_label(folder_chain), text_right - text_x);
    if !title.is_empty() {
        let metrics = ppc_sf_metrics();
        let text_y =
            top + (bottom - top - (metrics.ascent + metrics.descent)) / 2 + metrics.ascent - 1;
        ppc_sf_text(
            memory,
            gworlds,
            (text_x, text_y),
            &title,
            PPC_RGB_BLACK,
            Some((top, text_x, bottom, text_right)),
        );
    }
    ppc_sf_static_text(
        memory,
        gworlds,
        ppc_standard_file_global_rect(bounds, label_rect),
        crate::trap::dispatch::BOOT_VOLUME_NAME.as_bytes(),
    );
}

fn ppc_standard_file_draw_get_dialog(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    tracking: &PpcStandardFileGetTrackingState,
) {
    // In the 68K package's order: the window, its static text, its buttons in
    // DrawControls order, then the default button, popup, separator and list.
    let front = tracking.front_buffer;
    let bounds = tracking.bounds;
    ppc_sf_dialog_frame(memory, gworlds, front, bounds);
    let open_enabled = tracking
        .entries
        .get(tracking.selected)
        .is_some_and(|entry| entry.is_directory || entry.file_type != 0);
    for (rect, label, enabled) in [
        (PPC_STANDARD_FILE_GET_DESKTOP_RECT, &b"Desktop"[..], true),
        (PPC_STANDARD_FILE_GET_EJECT_RECT, b"Eject", false),
        (PPC_STANDARD_FILE_GET_CANCEL_RECT, b"Cancel", true),
    ] {
        ppc_standard_file_draw_button(memory, front, gworlds, bounds, rect, label, enabled, false);
    }
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
    ppc_standard_file_draw_folder_popup(
        memory,
        gworlds,
        bounds,
        ppc_standard_file_get_popup_rect(tracking),
        PPC_STANDARD_FILE_GET_VOLUME_LABEL_RECT,
        &tracking.folder_chain,
    );
    let separator = ppc_standard_file_global_rect(bounds, PPC_STANDARD_FILE_GET_SEPARATOR_RECT);
    ppc_sf_border(memory, front, (separator.0, separator.1, separator.2 + 1, separator.3));
    ppc_standard_file_draw_list(
        memory,
        gworlds,
        front,
        bounds,
        PPC_STANDARD_FILE_GET_LIST_RECT,
        &tracking.entries,
        Some(tracking.selected),
    );
    ppc_standard_file_draw_scrollbar(
        memory,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_GET_SCROLL_RECT,
        tracking.entries.len(),
        Some(tracking.selected),
    );
    if tracking.folder_popup_open {
        ppc_standard_file_draw_folder_menu(
            memory,
            gworlds,
            front,
            bounds,
            ppc_standard_file_get_popup_rect(tracking),
            &tracking.folder_chain,
        );
    }
}

fn ppc_standard_file_get_popup_rect(
    tracking: &PpcStandardFileGetTrackingState,
) -> (i16, i16, i16, i16) {
    ppc_standard_file_folder_popup_rect(
        PPC_STANDARD_FILE_GET_VOLUME_RECT,
        PPC_STANDARD_FILE_GET_LIST_RECT,
        &tracking.folder_chain,
    )
}

fn ppc_standard_file_put_popup_rect(
    tracking: &PpcStandardFilePutTrackingState,
) -> (i16, i16, i16, i16) {
    ppc_standard_file_folder_popup_rect(
        PPC_STANDARD_FILE_PUT_VOLUME_RECT,
        PPC_STANDARD_FILE_PUT_LIST_RECT,
        &tracking.folder_chain,
    )
}

fn ppc_standard_file_draw_put_dialog(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    tracking: &PpcStandardFilePutTrackingState,
) {
    // In the 68K package's order: the window, its static and edit text, its
    // buttons in DrawControls order, then the default button, popup, list
    // and insertion point.
    let front = tracking.front_buffer;
    let bounds = tracking.bounds;
    ppc_sf_dialog_frame(memory, gworlds, front, bounds);
    ppc_sf_static_text(
        memory,
        gworlds,
        ppc_standard_file_global_rect(bounds, PPC_STANDARD_FILE_PUT_PROMPT_RECT),
        &tracking.prompt,
    );
    ppc_standard_file_draw_name_field(memory, gworlds, tracking);
    let save_enabled = !tracking.name.is_empty();
    for (rect, label, enabled) in [
        (PPC_STANDARD_FILE_PUT_EJECT_RECT, &b"Eject"[..], false),
        (PPC_STANDARD_FILE_PUT_DESKTOP_RECT, b"Desktop", true),
        (PPC_STANDARD_FILE_PUT_CANCEL_RECT, b"Cancel", true),
    ] {
        ppc_standard_file_draw_button(memory, front, gworlds, bounds, rect, label, enabled, false);
    }
    ppc_standard_file_draw_button(
        memory,
        front,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_PUT_SAVE_RECT,
        b"Save",
        save_enabled,
        true,
    );
    ppc_standard_file_draw_folder_popup(
        memory,
        gworlds,
        bounds,
        ppc_standard_file_put_popup_rect(tracking),
        PPC_STANDARD_FILE_PUT_VOLUME_LABEL_RECT,
        &tracking.folder_chain,
    );
    ppc_standard_file_draw_list(
        memory,
        gworlds,
        front,
        bounds,
        PPC_STANDARD_FILE_PUT_LIST_RECT,
        &tracking.entries,
        tracking.selected,
    );
    ppc_standard_file_draw_scrollbar(
        memory,
        gworlds,
        bounds,
        PPC_STANDARD_FILE_PUT_SCROLL_RECT,
        tracking.entries.len(),
        tracking.selected,
    );
    // With nothing selected, the insertion point stands before the
    // character at the selection.
    if tracking.sel_start == tracking.sel_end {
        let (top, left, bottom, right) =
            ppc_standard_file_global_rect(bounds, PPC_STANDARD_FILE_PUT_NAME_RECT);
        let at = tracking.sel_start.min(tracking.name.len());
        let x = left + ppc_sf_measure(&tracking.name[..at]);
        if x < right {
            ppc_sf_fill(memory, front, (top, x, bottom, x + 1), true);
        }
    }
    if tracking.folder_popup_open {
        ppc_standard_file_draw_folder_menu(
            memory,
            gworlds,
            front,
            bounds,
            ppc_standard_file_put_popup_rect(tracking),
            &tracking.folder_chain,
        );
    }
}

/// The name field, an editText item: a white field framed three pixels
/// outside the item, its text one pixel in, and the selection inverted
/// (Inside Macintosh Volume I, pp. I-405, I-414, I-422).
fn ppc_standard_file_draw_name_field(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    tracking: &PpcStandardFilePutTrackingState,
) {
    let front = tracking.front_buffer;
    let item = ppc_standard_file_global_rect(tracking.bounds, PPC_STANDARD_FILE_PUT_NAME_RECT);
    let (top, left, bottom, right) = item;
    let frame = crate::dialog_manager::edit_text_frame_rect(item);
    ppc_sf_fill(memory, front, frame, false);
    ppc_sf_border(memory, front, frame);
    let metrics = ppc_sf_metrics();
    let line_height = metrics.ascent + metrics.descent + metrics.leading;
    let name = &tracking.name;
    let mut baseline = top + metrics.ascent;
    for line in crate::quickdraw::text::wrap_classic_text(name, (right - left).max(1), |_, byte| {
        ppc_text_byte_advance_for_font(byte, PPC_SF_FONT, PPC_SF_SIZE)
    }) {
        if baseline - metrics.ascent >= bottom {
            break;
        }
        let mut end = line.next.min(name.len());
        while end > line.start && matches!(name[end - 1], b'\r' | b'\n') {
            end -= 1;
        }
        ppc_sf_text(
            memory,
            gworlds,
            (left + 1, baseline),
            &name[line.start..end],
            PPC_RGB_BLACK,
            Some(item),
        );
        baseline += line_height;
    }
    let start = tracking.sel_start.min(name.len());
    let end = tracking.sel_end.min(name.len());
    if name.is_empty() || start >= end {
        return;
    }
    // TextEdit's selection: glyphs start one pixel in, but a selection from
    // the first character includes that pixel; one reaching the end runs to
    // the field's right edge.
    let inset = crate::dialog_manager::DIALOG_TEXT_LEFT_INSET;
    let selection_left = if start == 0 {
        left
    } else {
        (left + inset + ppc_sf_measure(&name[..start])).min(right)
    };
    let selection_right = if end >= name.len() {
        right
    } else {
        (left + inset + ppc_sf_measure(&name[..end])).min(right)
    };
    let selection = (top, selection_left, (top + line_height).min(bottom), selection_right);
    if selection.1 >= selection.3 || selection.0 >= selection.2 {
        return;
    }
    if ppc_sf_classic(gworlds) {
        ppc_sf_invert(memory, front, selection);
    } else {
        ppc_draw_themed_selection(memory, gworlds, PPC_MAIN_GWORLD, selection);
    }
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
        PpcStandardFileMode::PutModern
        | PpcStandardFileMode::PutLegacy
        | PpcStandardFileMode::PutNavigation => {}
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
    c_string: bool,
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
            if c_string {
                // fName is a Str63; the C glue converts it in place.
                let name = &name[..name.len().min(63)];
                for (offset, byte) in name.iter().chain(std::iter::once(&0)).enumerate() {
                    let _ = memory.write_u8(reply + 10 + offset as u32, *byte);
                }
            } else {
                let _ = ppc_write_pstring_bytes(memory, reply + 10, name);
            }
        }
        PpcStandardFileMode::GetModern
        | PpcStandardFileMode::GetLegacy
        | PpcStandardFileMode::PutNavigation => {}
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
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    navigation: &mut PpcNavigationReplyContext<'_>,
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
    startup.standard_file_put_tracking = None;
    if tracking.call.mode == PpcStandardFileMode::PutNavigation {
        let replacing = ppc_fsspec_target_exists(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            tracking.dir_id,
            &tracking.name,
        );
        let saved = (accepted && !tracking.name.is_empty()).then_some((
            tracking.vref,
            tracking.dir_id,
            tracking.name.as_slice(),
            replacing,
        ));
        let result = ppc_navigation_write_put_reply(
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            navigation,
            tracking.call.reply,
            saved,
        );
        return PpcImportAction::Return(ppc_i16_result(result));
    }
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
            tracking.call.c_strings,
        );
    } else {
        ppc_standard_file_write_cancel_reply(memory, tracking.call.mode, tracking.call.reply);
    }
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
        .position(|event| matches!(event.what, 1 | 2 | 3 | 5))
        .and_then(|index| event_queue.remove(index));
    let previous_dir_id = tracking.current_dir_id;
    let previous_selection = tracking.selected;
    let previous_popup_open = tracking.folder_popup_open;
    let mut open = false;
    if let Some(event) = event {
        if tracking.folder_popup_open {
            let local = (
                event.where_v.saturating_sub(tracking.bounds.0),
                event.where_h.saturating_sub(tracking.bounds.1),
            );
            let row = matches!(event.what, 1 | 2)
                .then(|| {
                    ppc_standard_file_folder_menu_row(
                        &tracking.folder_chain,
                        ppc_standard_file_get_popup_rect(&tracking),
                        local,
                    )
                })
                .flatten();
            if let Some(row) = row {
                tracking.folder_popup_open = false;
                let dir_id = tracking.folder_chain[row].0;
                if dir_id != tracking.current_dir_id {
                    tracking.current_dir_id = dir_id;
                    tracking.entries = ppc_standard_file_get_entries(
                        vfs_directories,
                        vfs_files,
                        vfs_resource_files,
                        dir_id,
                        tracking.file_types.as_deref(),
                    );
                    tracking.selected = 0;
                }
            } else if !(event.what == 2
                && ppc_standard_file_point_in_rect(local, ppc_standard_file_get_popup_rect(&tracking)))
            {
                // Releasing on the popup button leaves the menu open for a second click.
                tracking.folder_popup_open = false;
            }
        } else if event.what == 1 {
            let local = (
                event.where_v.saturating_sub(tracking.bounds.0),
                event.where_h.saturating_sub(tracking.bounds.1),
            );
            let last_list_click = tracking.last_list_click.take();
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
            if ppc_standard_file_point_in_rect(local, ppc_standard_file_get_popup_rect(&tracking)) {
                tracking.folder_popup_open = true;
            } else if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_GET_DESKTOP_RECT) {
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
                let double_time = memory
                    .read_u32_be(crate::memory::globals::addr::DOUBLE_TIME)
                    .unwrap_or(PPC_DEFAULT_DOUBLE_TIME_TICKS);
                // The first click can scroll the list, so the second opens its selection.
                open = last_list_click.is_some_and(|(when, last_row)| {
                    last_row == row && event.when.wrapping_sub(when) <= double_time
                });
                if !open && index < tracking.entries.len() {
                    tracking.selected = index;
                    tracking.last_list_click = Some((event.when, row));
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
        } else if event.what != 2 {
            tracking.last_list_click = None;
            let character = event.message as u8;
            let key_code = (event.message >> 8) as u8;
            if crate::dialog_manager::is_dialog_default_key(character, key_code) {
                open = true;
            } else if crate::dialog_manager::is_dialog_cancel_key(
                character,
                key_code,
                event.modifiers,
            ) {
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
    if tracking.current_dir_id != previous_dir_id {
        tracking.folder_chain =
            ppc_standard_file_folder_chain(vfs_directories, tracking.current_dir_id);
        // CustomGetFile filters each displayed folder, not just the initial
        // listing. Inside Macintosh: Files (1992), pp. 3-20--3-21.
        tracking = match ppc_standard_file_begin_filtering(
            cpu,
            memory,
            startup,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            tracking,
        ) {
            Ok(action) => return action,
            Err(tracking) => tracking,
        };
    }
    if tracking.current_dir_id != previous_dir_id
        || tracking.selected != previous_selection
        || tracking.folder_popup_open != previous_popup_open
    {
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
            // A CustomGetFile file filter returns TRUE to exclude an item.
            // Inside Macintosh: Files (1992), p. 3-20.
            let excluded = cpu.gpr[3] & 0xff != 0;
            if excluded && candidate_index < filtering.tracking.entries.len() {
                filtering.tracking.entries.remove(candidate_index);
            } else {
                filtering.next_entry = filtering.next_entry.saturating_add(1);
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
            // The callback returned through the import stub; resume the
            // modal call with its original caller as the link register.
            cpu.lr = tracking.call.return_address;
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
        cpu.lr = filtering.tracking.call.return_address;
        return PpcImportAction::ReturnPreserve;
    }
    if startup.standard_file_get_tracking.is_some() {
        return ppc_standard_file_get_service(
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
            event_queue,
            gworlds,
        );
    }
    let (num_types, type_list) = match mode {
        PpcStandardFileMode::GetModern => (cpu.gpr[4] as u16 as i16, cpu.gpr[5]),
        PpcStandardFileMode::GetLegacy => (cpu.gpr[6] as u16 as i16, cpu.gpr[7]),
        PpcStandardFileMode::PutModern
        | PpcStandardFileMode::PutLegacy
        | PpcStandardFileMode::PutNavigation => return PpcImportAction::ReturnPreserve,
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
    let (filter_ptr, callback_with_data) = operation.filter_pointer(cpu);
    let filter = (filter_ptr != 0)
        .then(|| ppc_resolve_callback_target(memory, filter_ptr, cpu.gpr[2], None))
        .flatten()
        .filter(|callback| memory.read_u32_be(callback.entry).is_some())
        .map(|callback| PpcStandardFileFilter {
            callback,
            callback_with_data,
            // CustomGetFile's eleventh argument is the private pointer passed
            // to its callbacks. Inside Macintosh: Files (1992), p. 3-52.
            your_data_ptr: if callback_with_data {
                ppc_parameter_area_slot_addr(cpu.gpr[1], PPC_NATIVE_PARAMETER_GPR_COUNT + 2)
                    .and_then(|address| memory.read_u32_be(address))
                    .unwrap_or(0)
            } else {
                0
            },
            include_directories: operation == PpcStandardFileOperation::CustomGetFile
                && num_types == -1,
        });
    let tracking = PpcStandardFileGetTrackingState {
        call: ppc_standard_file_call(mode, cpu, false),
        entries,
        current_dir_id,
        file_types,
        filter,
        selected: 0,
        folder_chain: ppc_standard_file_folder_chain(vfs_directories, current_dir_id),
        folder_popup_open: false,
        last_list_click: None,
        bounds,
        front_buffer,
        saved_pixels: ppc_standard_file_save_pixels(memory, front_buffer, bounds),
    };
    let tracking = match ppc_standard_file_begin_filtering(
        cpu,
        memory,
        startup,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        tracking,
    ) {
        Ok(action) => return action,
        Err(tracking) => tracking,
    };
    ppc_standard_file_draw_get_dialog(memory, gworlds, &tracking);
    startup.standard_file_get_tracking = Some(tracking);
    PpcImportAction::Yield(u64::MAX)
}

#[allow(clippy::too_many_arguments)]
fn ppc_standard_file_put_start(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    default_dir_id: u32,
    gworlds: &[PpcGWorldRecord],
    mode: PpcStandardFileMode,
    prompt_ptr: u32,
    requested_origin: Option<(i16, i16)>,
    c_strings: bool,
) -> PpcImportAction {
    let name_ptr = match mode {
        PpcStandardFileMode::PutModern => cpu.gpr[4],
        PpcStandardFileMode::PutLegacy => cpu.gpr[5],
        // NavDialogOptions.savedFileName is the default name.
        PpcStandardFileMode::PutNavigation if cpu.gpr[5] != 0 => {
            cpu.gpr[5] + PPC_NAV_DIALOG_OPTIONS_SAVED_FILE_NAME_OFFSET
        }
        PpcStandardFileMode::PutNavigation => 0,
        PpcStandardFileMode::GetModern | PpcStandardFileMode::GetLegacy => {
            return PpcImportAction::ReturnPreserve
        }
    };
    let mut name = if name_ptr != 0 {
        ppc_standard_file_string(memory, name_ptr, c_strings)
    } else {
        Vec::new()
    };
    if name.is_empty() {
        name.extend_from_slice(b"Untitled");
    }
    name.truncate(63);
    let reply = ppc_standard_file_reply_ptr(mode, cpu);
    let Some(front_buffer) = ppc_front_buffer_for_gworld(gworlds, PPC_MAIN_GWORLD) else {
        if mode == PpcStandardFileMode::PutNavigation {
            // Without a screen the dialog cannot be shown; the reply is
            // written as though the user cancelled it.
            if reply != 0
                && memory
                    .write_bytes(reply, &[0; PPC_NAV_REPLY_RECORD_SIZE])
                    .is_some()
            {
                let _ = memory.write_u32_be(reply + PPC_NAV_REPLY_SELECTION_OFFSET, PPC_TYPE_NULL);
            }
        } else {
            ppc_standard_file_write_cancel_reply(memory, mode, reply);
        }
        return ppc_standard_file_put_cancelled(mode);
    };
    let bounds = ppc_standard_file_centered_bounds(
        front_buffer,
        PPC_STANDARD_FILE_PUT_DIALOG_WIDTH,
        PPC_STANDARD_FILE_PUT_DIALOG_HEIGHT,
        requested_origin,
    );
    let dir_id = ppc_directory_path_for_id(vfs_directories, default_dir_id)
        .map(|_| default_dir_id)
        .unwrap_or(PPC_ROOT_DIR_ID);
    let tracking = PpcStandardFilePutTrackingState {
        call: ppc_standard_file_call(mode, cpu, c_strings),
        vref: PPC_BOOT_VOLUME_REF_NUM,
        dir_id,
        entries: ppc_standard_file_get_entries(
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            dir_id,
            None,
        ),
        selected: None,
        folder_chain: ppc_standard_file_folder_chain(vfs_directories, dir_id),
        folder_popup_open: false,
        prompt: ppc_standard_file_prompt(memory, prompt_ptr, c_strings),
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

/// Shows `dir_id` in the put dialog's list, with nothing selected.
fn ppc_standard_file_put_show_folder(
    tracking: &mut PpcStandardFilePutTrackingState,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &[PpcVfsFileRecord],
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    dir_id: u32,
) {
    tracking.dir_id = dir_id;
    tracking.entries =
        ppc_standard_file_get_entries(vfs_directories, vfs_files, vfs_resource_files, dir_id, None);
    tracking.selected = None;
    tracking.folder_chain = ppc_standard_file_folder_chain(vfs_directories, dir_id);
}

/// The selected entry's folder, if a folder is selected.
fn ppc_standard_file_put_selected_folder(tracking: &PpcStandardFilePutTrackingState) -> Option<u32> {
    tracking
        .selected
        .and_then(|index| tracking.entries.get(index))
        .filter(|entry| entry.is_directory)
        .map(|entry| entry.dir_id)
}

/// One pass of the modal put dialog: handles at most one event, then either
/// returns the reply or yields with the dialog still up. The list, folder
/// popup and Desktop button choose the folder that receives the file, as in
/// the 68K Standard File package (Inside Macintosh: Files (1992),
/// pp. 3-8--3-12).
#[allow(clippy::too_many_arguments)]
fn ppc_standard_file_put_service(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    startup: &mut PpcToolboxStartupState,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    heap_cursor: &mut u32,
    last_mem_error: &mut i16,
    navigation: &mut PpcNavigationReplyContext<'_>,
    mut tracking: PpcStandardFilePutTrackingState,
    vfs_directories: &[PpcVfsDirectory],
    vfs_files: &ProcessVfsFileRecords,
    vfs_resource_files: &[PpcVfsResourceFileRecord],
    vfs_volumes: &[PpcVfsVolumeRecord],
    working_directories: &mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &mut i16,
    event_queue: &mut EventQueue,
    gworlds: &[PpcGWorldRecord],
) -> PpcImportAction {
    let mut finish = |memory: &mut PpcSectionMem,
                      startup: &mut PpcToolboxStartupState,
                      tracking: PpcStandardFilePutTrackingState,
                      accepted: bool| {
        ppc_standard_file_finish_put(
            memory,
            startup,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            navigation,
            tracking,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            vfs_volumes,
            working_directories,
            next_working_directory_ref_num,
            accepted,
        )
    };
    if tracking.call.return_address != cpu.lr {
        return finish(memory, startup, tracking, false);
    }
    let event = event_queue
        .iter()
        .position(|event| matches!(event.what, 1 | 2 | 3 | 5))
        .and_then(|index| event_queue.remove(index));
    let Some(event) = event else {
        // Keep the dialog on top of anything the application drew meanwhile.
        ppc_standard_file_draw_put_dialog(memory, gworlds, &tracking);
        startup.standard_file_put_tracking = Some(tracking);
        return PpcImportAction::Yield(u64::MAX);
    };
    let mut accept = false;
    let mut show_folder = None;
    let local = (
        event.where_v.saturating_sub(tracking.bounds.0),
        event.where_h.saturating_sub(tracking.bounds.1),
    );
    if tracking.folder_popup_open {
        let row = matches!(event.what, 1 | 2)
            .then(|| {
                ppc_standard_file_folder_menu_row(
                    &tracking.folder_chain,
                    ppc_standard_file_put_popup_rect(&tracking),
                    local,
                )
            })
            .flatten();
        if let Some(row) = row {
            tracking.folder_popup_open = false;
            show_folder = Some(tracking.folder_chain[row].0);
        } else if !(event.what == 2
            && ppc_standard_file_point_in_rect(local, ppc_standard_file_put_popup_rect(&tracking)))
        {
            // Releasing on the popup button leaves the menu open for a second click.
            tracking.folder_popup_open = false;
        }
    } else if event.what == 1 {
        if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_PUT_CANCEL_RECT) {
            return finish(memory, startup, tracking, false);
        }
        if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_PUT_SAVE_RECT) {
            accept = true;
        } else if ppc_standard_file_point_in_rect(local, ppc_standard_file_put_popup_rect(&tracking)) {
            tracking.folder_popup_open = true;
        } else if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_PUT_DESKTOP_RECT) {
            show_folder = Some(PPC_ROOT_DIR_ID);
        } else if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_PUT_LIST_RECT)
            && !tracking.entries.is_empty()
        {
            let row = ((local.0 - PPC_STANDARD_FILE_PUT_LIST_RECT.0 - 2)
                / PPC_STANDARD_FILE_GET_ROW_HEIGHT)
                .max(0) as usize;
            let index = ppc_standard_file_first_visible(tracking.selected).saturating_add(row);
            if index < tracking.entries.len() {
                // A click on the folder already selected opens it, as a
                // double click does.
                if tracking.selected == Some(index) && tracking.entries[index].is_directory {
                    show_folder = Some(tracking.entries[index].dir_id);
                } else {
                    tracking.selected = Some(index);
                }
            }
        } else if ppc_standard_file_point_in_rect(local, PPC_STANDARD_FILE_PUT_SCROLL_RECT)
            && !tracking.entries.is_empty()
        {
            let relative_v = local.0 - PPC_STANDARD_FILE_PUT_SCROLL_RECT.0;
            let height = PPC_STANDARD_FILE_PUT_SCROLL_RECT.2 - PPC_STANDARD_FILE_PUT_SCROLL_RECT.0;
            let selected = tracking.selected.unwrap_or(0);
            let last = tracking.entries.len() - 1;
            tracking.selected = Some(if relative_v < 16 {
                selected.saturating_sub(1)
            } else if relative_v >= height - 16 {
                (selected + 1).min(last)
            } else if relative_v < height / 2 {
                selected.saturating_sub(PPC_STANDARD_FILE_VISIBLE_ROWS)
            } else {
                (selected + PPC_STANDARD_FILE_VISIBLE_ROWS).min(last)
            });
        }
    } else if event.what != 2 {
        let character = event.message as u8;
        let key_code = (event.message >> 8) as u8;
        let command = event.modifiers & 0x0100 != 0;
        if crate::dialog_manager::is_dialog_default_key(character, key_code) {
            accept = true;
        } else if crate::dialog_manager::is_dialog_cancel_key(character, key_code, event.modifiers) {
            return finish(memory, startup, tracking, false);
        } else if command && (key_code == 0x7e || character == 0x1e) {
            // Command-Up Arrow shows the enclosing folder.
            show_folder = vfs_directories
                .iter()
                .find(|directory| directory.dir_id == tracking.dir_id)
                .map(|directory| directory.parent_dir_id)
                .filter(|parent| *parent >= PPC_ROOT_DIR_ID);
        } else if command && (key_code == 0x7d || character == 0x1f) {
            // Command-Down Arrow opens the selected folder.
            show_folder = ppc_standard_file_put_selected_folder(&tracking);
        } else if command && character.eq_ignore_ascii_case(&b'd') {
            show_folder = Some(PPC_ROOT_DIR_ID);
        } else if command && character.eq_ignore_ascii_case(&b'a') {
            tracking.sel_start = 0;
            tracking.sel_end = tracking.name.len();
        } else if (key_code == 0x7e || character == 0x1e) && !tracking.entries.is_empty() {
            tracking.selected = Some(tracking.selected.unwrap_or(0).saturating_sub(1));
        } else if (key_code == 0x7d || character == 0x1f) && !tracking.entries.is_empty() {
            tracking.selected = Some(
                tracking
                    .selected
                    .map_or(0, |selected| selected + 1)
                    .min(tracking.entries.len() - 1),
            );
        } else if character == 0x08 || key_code == 0x33 {
            ppc_standard_file_backspace_name(&mut tracking);
        } else if !command
            && (0x20..=0x7e).contains(&character)
            && !matches!(character, b'/' | b':')
            && tracking.sel_start <= tracking.sel_end
            && tracking.sel_end <= tracking.name.len()
        {
            ppc_standard_file_insert_name_character(&mut tracking, character);
        }
    }
    // Save stays disabled while the name is empty.
    if accept && !tracking.name.is_empty() {
        return finish(memory, startup, tracking, true);
    }
    if let Some(dir_id) = show_folder {
        ppc_standard_file_put_show_folder(
            &mut tracking,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            dir_id,
        );
    }
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
    navigation: &mut PpcNavigationReplyContext<'_>,
) -> PpcImportAction {
    let mode = operation.mode();
    let requested_origin = operation.requested_origin(cpu, memory);
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
        PpcStandardFileMode::PutModern
        | PpcStandardFileMode::PutLegacy
        | PpcStandardFileMode::PutNavigation => {
            if let Some(tracking) = startup.standard_file_put_tracking.take() {
                ppc_standard_file_put_service(
                    cpu,
                    memory,
                    startup,
                    process_memory_manager,
                    heap_cursor,
                    last_mem_error,
                    navigation,
                    tracking,
                    vfs_directories,
                    vfs_files,
                    vfs_resource_files,
                    vfs_volumes,
                    working_directories,
                    next_working_directory_ref_num,
                    event_queue,
                    gworlds,
                )
            } else {
                let prompt_ptr = operation.prompt_pointer(cpu);
                ppc_standard_file_put_start(
                    cpu,
                    memory,
                    startup,
                    vfs_directories,
                    vfs_files,
                    vfs_resource_files,
                    default_dir_id,
                    gworlds,
                    mode,
                    prompt_ptr,
                    requested_origin,
                    operation == PpcStandardFileOperation::SfPutFileCString,
                )
            }
        }
    }
}
