//! HLE-side PowerPC loader handoff.
//!
//! The PEF parser lives in [`super::pef`]. This module turns parsed and
//! instantiated PEF data into deterministic CPU + guest-address-space state:
//! section bases, relocations, synthetic import TVectors, and an initial
//! stack frame. Parsed loader facts are mapped here into the native runtime;
//! optional PEF dump formatting lives in the private `pef_dump` child.

#[cfg(test)]
use super::pef::SECTION_KIND_UNPACKED_DATA;
use super::pef::{
    apply_pef_relocations_detailed, instantiate_pef_sections, parse_pef_header,
    parse_pef_imported_symbols, parse_pef_loader_header, parse_pef_reloc_headers,
    parse_pef_sections, pef_reloc_chunk_stream, resolve_pef_imports, PefRelocApplyError,
    PefRelocContext, SECTION_KIND_CODE, SECTION_KIND_CONSTANT,
};
use super::ApplicationSizeResource;
use crate::callback_manager::CallbackTaskArchitecture;
use crate::cfm::fragment::{
    first_base_for_kind, first_data_base, resolve_fragment_exports, section_bases, CfmFragmentPlan,
    CfmSection as MappedSection,
};
use crate::cfm::{CfmLoadId, CfmOperation, CfmResourceCall, CfmResourcePreparation};
use crate::event_queue::{EventQueue, EventQueueProbeSnapshot, EventRecordSnapshot, QueuedEvent};
use crate::guest_call::{
    format_ppc_import_action, install_powerpc_call_arguments, ExecutionMenuViews,
    GuestCallContinuation, GuestCallEffect, GuestCallRequest, GuestCallTarget, MenuTrackingCall,
    MenuTrackingOrigin, NativeRetirement, SharedGuestCallStack,
};
use crate::guest_call::{MenuBarBuildResume, MenuBarCallOrigin};
use crate::guest_procedure::{
    resolve_guest_procedure, GuestIsa, GuestProcedure,
    ROUTINE_DESCRIPTOR_HEADER_SIZE as PPC_ROUTINE_DESCRIPTOR_HEADER_SIZE,
    ROUTINE_DESCRIPTOR_MIXED_MODE_TRAP as PPC_MIXED_MODE_TRAP,
    ROUTINE_DESCRIPTOR_VERSION as PPC_ROUTINE_DESCRIPTOR_VERSION,
    ROUTINE_FLAG_DONT_PASS_SELECTOR as PPC_ROUTINE_FLAG_DONT_PASS_SELECTOR,
    ROUTINE_FLAG_USE_NATIVE_ISA as PPC_ROUTINE_FLAG_USE_NATIVE_ISA,
    ROUTINE_RECORD_FLAGS_OFFSET as PPC_ROUTINE_RECORD_FLAGS_OFFSET,
    ROUTINE_RECORD_ISA_OFFSET as PPC_ROUTINE_RECORD_ISA_OFFSET,
    ROUTINE_RECORD_M68K_ISA as PPC_ROUTINE_RECORD_M68K_ISA,
    ROUTINE_RECORD_POWERPC_ISA as PPC_ROUTINE_RECORD_POWERPC_ISA,
    ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET as PPC_ROUTINE_RECORD_PROC_DESCRIPTOR_OFFSET,
    ROUTINE_RECORD_SIZE as PPC_ROUTINE_RECORD_SIZE,
};
#[cfg(test)]
use crate::guest_procedure::{
    ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE as PPC_ROUTINE_FLAG_PROC_DESCRIPTOR_RELATIVE,
    ROUTINE_RECORD_SELECTOR_OFFSET as PPC_ROUTINE_RECORD_SELECTOR_OFFSET,
};
use crate::list_manager::ProcessListManagerState;
use crate::machine_profile::{
    POWERPC_CARBON_VERSION_BCD, POWERPC_SYSTEM_VERSION_BCD, REFERENCE_MACHINE_PROFILE,
    REFERENCE_POWERPC_EXECUTION_CAPABILITIES,
};
use crate::managers::resource::{
    serialize_resource_fork_with_attrs, ResourceFork, ResourceForkEntry,
};
pub(crate) use crate::memory::{GuestAddressSpace as PpcSectionMem, MacMemoryBus, MemoryBus};
use crate::menu_manager::{MenuDefinitionTracking, MenuTrackingKind, SharedNativeMenuSelection};
use crate::menu_model::GuestMenuSnapshot;
use crate::process_context::{
    ProcessAeDescriptor, ProcessAppleEventHandler, ProcessContext, ProcessFileSystemState,
    ProcessHandleHeap, ProcessHandleRecord, ProcessHandleStateRecord, ProcessMemoryManager,
    ProcessNativeHeapState, ProcessNativeMemoryManager, ProcessNewHandleBackend,
    ProcessNewHandleRequest, ProcessPtrRecord, ProcessResourceManagerState,
    ProcessSyntheticAppleEvent, ProcessVfsFileRecords, ProcessVfsResourceFileRecords,
    ProcessWorkingDirectory, SharedProcessAppleEventDescriptors, SharedProcessAppleEventHandlers,
    SharedProcessAppleEventLaunchState, SharedProcessCallbackScheduling,
    SharedProcessCollectionManager, SharedProcessControlManager, SharedProcessCursorState,
    SharedProcessDialogText, SharedProcessDisplayClut, SharedProcessDisplayGamma,
    SharedProcessEventQueue, SharedProcessFileSystem, SharedProcessGraphicsDevice,
    SharedProcessGraphicsPort, SharedProcessInputState, SharedProcessMemoryManager,
    SharedProcessMixedModeM68kState, SharedProcessQuickDrawError,
    SharedProcessQuickDrawHiliteColors, SharedProcessQuickDrawOpColors,
    SharedProcessQuickDrawPixelStates, SharedProcessResourcePolicy, SharedProcessTickState,
    SharedProcessTimerTasks, SharedProcessVblTasks, SharedProcessWindowList,
    DEFAULT_QUICKDRAW_HILITE_COLOR,
};
use crate::process_manager::{
    resolve_process_application_metadata, ProcessSerialNumber, SingleProcessEnumeration,
};
use crate::quickdraw::fonts::style::{
    get_italic_end_extend, get_italic_slant, get_italic_underline_extend_left,
};
use crate::quickdraw::fonts::{
    font_id_for_name, font_name_for_id, get_font_face, get_font_face_scale_ratio,
    get_font_face_scaled, FONT_APPLICATION,
};
use crate::quickdraw::text::{
    get_font_metrics, get_glyph, get_glyph_italic, get_underline_thickness, QuickDrawTextStyle,
};
use crate::thread_manager::{RetiredThreadStorageEdge, ThreadManager};
use crate::trap::extended80::Extended80;
use crate::trap::manager::{
    TrapManager, TrapManagerMemoryOp, TrapManagerMemoryResult, TrapManagerSetError, TrapTableKind,
};
use crate::trap::types::{decode_mac_roman, encode_mac_roman_lossy, Rect};
use crate::trap::{pict, TrapDispatcher};
use crate::ui_theme::{render_scrollbar_bitmap, Rgb8, ThemeBitmap, UiThemeId};
use ppc::{
    PpcAlignmentPolicy, PpcCpu, PpcException, PpcExecutionContext, PpcFetchHistogram,
    PpcFetchObserver, PpcImportAction, PpcMemory, PpcMemoryWriteObserver, PpcNativeReturnGpr3,
    PpcRunResult,
};
use std::cell::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

mod dispatch_cfm;
use dispatch_cfm::*;
mod dispatch_apple_events;
use dispatch_apple_events::*;
mod dispatch_appearance;
mod dispatch_appletalk;
mod dispatch_bit_transfers;
mod dispatch_collection;
mod dispatch_color_tables;
mod dispatch_control;
mod dispatch_core_foundation;
mod dispatch_cursor;
mod dispatch_desk;
mod dispatch_devices;
mod dispatch_dialog;
mod dispatch_drawsprocket;
mod dispatch_event;
mod dispatch_files;
mod dispatch_fonts;
mod dispatch_gestalt;
mod dispatch_graphics_devices;
mod dispatch_gworlds;
mod dispatch_icon_services;
mod dispatch_inputsprocket;
mod dispatch_list;
mod dispatch_low_memory;
mod dispatch_math;
mod dispatch_memory;
mod dispatch_menu;
mod dispatch_mixed_mode;
mod dispatch_native_exceptions;
use dispatch_native_exceptions::*;
mod dispatch_display;
mod dispatch_palettes;
mod dispatch_picture;
mod dispatch_polygons;
mod dispatch_printing;
mod dispatch_process;
mod dispatch_qd3d;
mod dispatch_quickdraw;
mod dispatch_quicktime;
mod dispatch_regions;
mod dispatch_resources;
mod dispatch_scrap;
mod dispatch_sound;
mod dispatch_standard_file;
mod dispatch_stdc;
mod dispatch_stdio;
mod dispatch_system;
mod dispatch_textedit;
mod dispatch_threads;
mod dispatch_time;
mod dispatch_toolbox;
mod dispatch_window;
pub(crate) use dispatch_collection::PpcCollectionCallbackState;
use dispatch_control::*;
pub(crate) use dispatch_dialog::PpcDialogCallbackState;
use dispatch_dialog::*;
#[cfg(test)]
use dispatch_display::*;
#[cfg(test)]
use dispatch_list::*;
pub(in crate::systems::macintosh::loader::ppc) use dispatch_mixed_mode::*;
use dispatch_standard_file::*;
pub(super) use dispatch_stdc::*;
pub(crate) use dispatch_stdc::{PpcQsortState, PpcStdSignalState};
pub use dispatch_stdio::PpcStdIoOperation;
pub(crate) use dispatch_stdio::*;
#[cfg(test)]
pub(super) use dispatch_system::ppc_munger_compatibility;
pub use dispatch_system::PpcSystemCompatibilityOperation;
pub(super) use dispatch_window::*;
mod pef_dump;
mod theme;
use dispatch_time::ppc_sync_vbl_task_links;
#[cfg(test)]
pub(crate) use dispatch_time::{
    ppc_install_time_task, ppc_install_vbl_task, ppc_remove_time_task, ppc_remove_vbl_task,
};
#[cfg(test)]
use pef_dump::format_pef_dump_json;
use pef_dump::{maybe_write, PefDumpContext};
use theme::*;

use dispatch_event::{
    dispatch_button_import, dispatch_getkeys_import, dispatch_microseconds_import,
    dispatch_still_down_import, dispatch_tick_count_import, ppc_still_down_result,
    ppc_wait_mouse_up_result, PpcTickCountIdlePollState,
};

mod diagnostics;
mod dispatch_imports;
mod pict_rendering;
pub(crate) use diagnostics::*;
pub(crate) use pict_rendering::*;
pub mod events;
pub mod files;
pub mod fixmath;
pub mod graphics;
pub mod gworlds;
pub mod import_targets;
pub mod imports;
mod loaded_app_callbacks;
mod loaded_app_display;
mod loaded_app_execution;
pub mod pef_loader;
pub(crate) use dispatch_imports::{dispatch_supported_import, PpcDispatchContext};
pub(crate) use import_targets::dispatcher_target_for_import;
pub use import_targets::PpcImportDispatcherTarget;
pub(crate) use pef_loader::{
    align_up, load_pef_application_with_config_and_system_reservation_and_libraries,
    load_pef_application_with_named_fragment_and_libraries,
};
pub use pef_loader::{
    load_pef_application, load_pef_application_with_config, PpcLoadConfig, PpcLoadError,
    PpcRelocationImportSymbol,
};
#[cfg(test)]
pub(crate) use pef_loader::{
    load_pef_application_with_config_and_optional_system_reservation,
    load_pef_application_with_config_and_system_reservation, map_instantiated_sections,
};
mod loaded_app_gateways;
mod loaded_app_input;
mod loaded_app_memory;
mod loaded_app_menu;
mod loaded_app_mixed_mode;
mod loaded_app_probes;
mod loaded_app_process;
mod loaded_app_qd3d;
mod loaded_app_resources;
mod loaded_app_time;
mod loaded_app_vfs;
pub mod memory;
pub mod menu;
pub mod palettes;
pub mod qd3d;
pub(crate) mod qd3d_text;
pub mod quickdraw;
pub mod quicktime;
pub mod regions;
pub mod resources;
pub mod sound;
pub mod sprockets;
pub mod textedit;
pub mod vfs;

pub(crate) use events::*;
pub(crate) use files::*;
pub(crate) use fixmath::*;
pub use graphics::*;
pub(crate) use gworlds::*;
pub use imports::*;
pub(crate) use memory::*;
pub use menu::*;
pub(crate) use palettes::*;
pub use qd3d::*;
pub(crate) use quickdraw::*;
pub use quicktime::*;
use regions::*;
pub(crate) use resources::*;
pub use sound::*;
pub use sprockets::*;
pub(crate) use textedit::*;
pub use vfs::*;

#[derive(Debug, Clone, Copy)]
struct SystemlessPpcImportBindingPolicy;

impl PpcImportBindingPolicy for SystemlessPpcImportBindingPolicy {
    fn dispatcher_target(&self, library: &str, symbol: &str) -> PpcImportDispatcherTarget {
        dispatcher_target_for_import(library, symbol)
    }

    fn fixed_data_address(&self, library: &str, symbol: &str) -> Option<u32> {
        import_data_address_for(library, symbol)
    }

    fn is_explicit_hle_library(&self, library: &str) -> bool {
        ppc_is_explicit_hle_cfm_library(library)
    }
}

struct PpcConnectedCfmBindingPolicy<'a> {
    connections: &'a [PpcCfmConnection],
}

impl PpcImportBindingPolicy for PpcConnectedCfmBindingPolicy<'_> {
    fn dispatcher_target(&self, library: &str, symbol: &str) -> PpcImportDispatcherTarget {
        dispatcher_target_for_import(library, symbol)
    }

    fn fixed_data_address(&self, library: &str, symbol: &str) -> Option<u32> {
        import_data_address_for(library, symbol)
    }

    fn resolved_import_address(&self, library: &str, symbol: &str, _class: u8) -> Option<u32> {
        self.connections
            .iter()
            .find(|connection| connection.library_name.eq_ignore_ascii_case(library))
            // PEF symbol classes annotate imports and exports; CFM binding
            // resolves the symbol by name within the selected library.
            .and_then(|connection| {
                connection
                    .exports
                    .iter()
                    .find(|export| export.name == symbol)
            })
            .map(|export| export.address)
            .or_else(|| self.fixed_data_address(library, symbol))
    }

    fn is_explicit_hle_library(&self, library: &str) -> bool {
        ppc_is_explicit_hle_cfm_library(library)
    }
}

fn ppc_import_layout() -> PpcImportLayout {
    PpcImportLayout {
        capacity: PPC_IMPORT_CAPACITY,
        tvector_base: PPC_IMPORT_TVECTOR_BASE,
        trap_base: PPC_IMPORT_TRAP_BASE,
    }
}

fn ppc_initial_import_error(error: PpcImportBindingError) -> PpcLoadError {
    match error {
        PpcImportBindingError::SymbolIndexOutOfRange {
            symbol_index,
            import_count,
        } => PpcLoadError::ImportBindingOutOfRange {
            symbol_index,
            import_count,
        },
        PpcImportBindingError::CapacityExceeded {
            import_count,
            capacity,
        } => PpcLoadError::ImportCapacityExceeded {
            import_count,
            capacity,
        },
        PpcImportBindingError::CountOverflow
        | PpcImportBindingError::BindingAddressOverflow
        | PpcImportBindingError::AddressTableOutOfRange => PpcLoadError::AddressOverflow,
        PpcImportBindingError::RegistryChanged => unreachable!("fresh import plan has no registry"),
    }
}

pub(in crate::systems::macintosh::loader::ppc) fn ppc_dynamic_import_error(
    error: PpcImportBindingError,
) -> i16 {
    match error {
        PpcImportBindingError::CountOverflow
        | PpcImportBindingError::CapacityExceeded { .. }
        | PpcImportBindingError::AddressTableOutOfRange => PPC_FRAG_NO_MEM,
        PpcImportBindingError::SymbolIndexOutOfRange { .. }
        | PpcImportBindingError::BindingAddressOverflow
        | PpcImportBindingError::RegistryChanged => PPC_FRAG_CORRUPT_ERR,
    }
}

pub(crate) fn ppc_initial_process_file_system() -> SharedProcessFileSystem {
    let mut state = ProcessFileSystemState::default();
    state.stdio_streams = ppc_initial_stdio_streams();
    state.next_file_ref_num = PPC_FIRST_FILE_REF_NUM;
    state.vfs_directories.replace(initial_ppc_vfs_directories());
    state
        .next_vfs_dir_id
        .with_mut(|next_dir_id| *next_dir_id = PPC_FIRST_DYNAMIC_DIR_ID);
    state
        .default_dir_id
        .with_mut(|default_dir_id| *default_dir_id = PPC_ROOT_DIR_ID);
    SharedProcessFileSystem::from_state(state)
}

pub const PPC_CODE_BASE: u32 = 0x0100_0000;
pub(crate) const PPC_CUR_RES_FILE_ADDR: u32 = 0x0000_0A5A;
pub const PPC_IMPORT_TVECTOR_BASE: u32 = 0x01e0_0000;
pub const PPC_IMPORT_TRAP_BASE: u32 = 0x01f0_0000;
const PPC_IMPORT_DATA_BASE: u32 = 0x01d0_0000;
const PPC_IMPORT_DATA_SIZE: usize = 0x1000;
const PPC_IMPORT_CTYPE_POINTER: u32 = PPC_IMPORT_DATA_BASE + 0x40c;
const PPC_IMPORT_MATH_PI: u32 = PPC_IMPORT_DATA_BASE + 0x410;
const PPC_IMPORT_MATH_FE_DFL_ENV: u32 = PPC_IMPORT_DATA_BASE + 0x418;
const PPC_IMPORT_STD_DBL_EPSILON: u32 = PPC_IMPORT_DATA_BASE + 0x420;
const PPC_IMPORT_STD_DBL_MAX: u32 = PPC_IMPORT_DATA_BASE + 0x428;
const PPC_IMPORT_STD_DBL_MIN: u32 = PPC_IMPORT_DATA_BASE + 0x430;
const PPC_IMPORT_STD_FLT_EPSILON: u32 = PPC_IMPORT_DATA_BASE + 0x438;
const PPC_IMPORT_STD_FLT_MAX: u32 = PPC_IMPORT_DATA_BASE + 0x43c;
const PPC_IMPORT_STD_FLT_MIN: u32 = PPC_IMPORT_DATA_BASE + 0x440;
const PPC_IMPORT_STD_ERRNO: u32 = PPC_IMPORT_DATA_BASE + 0x444;
const PPC_IMPORT_STD_MAC_OS_ERR: u32 = PPC_IMPORT_DATA_BASE + 0x448;
const PPC_IMPORT_CTYPE_TABLE: u32 = PPC_IMPORT_DATA_BASE + 0x500;
const PPC_IMPORT_CUR_AP_NAME: u32 = PPC_IMPORT_DATA_BASE + 0x900;
// Metrowerks StdCLib exposes `_iob` as the three 24-byte FILE records used
// for stdin, stdout, and stderr. Keep this zero-initialized storage in the
// stable import-data page. The actual FILE fields are private to StdCLib, so
// stream state lives in host-side metadata keyed by the guest FILE pointer.
pub const PPC_CFM_MAIN_STUB_BASE: u32 = 0x01d8_0000;
pub const PPC_MAIN_GWORLD: u32 = 0x02f0_0000;
pub const PPC_MAIN_GDEVICE: u32 = 0x02f0_0100;
pub const PPC_DSP_BACK_GWORLD: u32 = 0x0501_0000;
pub const PPC_DATA_BASE: u32 = 0x0200_0000;
pub const PPC_HEAP_BASE: u32 = 0x0300_0000;
pub(crate) const PPC_HEAP_ALIGNMENT: u32 = 16;
pub const PPC_STACK_TOP: u32 = 0x0500_0000;
pub const PPC_DEFAULT_STACK_SIZE: u32 = 64 * 1024;
pub const PPC_STACK_SIZE: u32 = PPC_DEFAULT_STACK_SIZE;
pub const PPC_STACK_BASE: u32 = PPC_STACK_TOP - PPC_DEFAULT_STACK_SIZE;
pub const PPC_HALT_PC: u32 = 0;
const PPC_LOW_MEMORY_SIZE: usize = 64 * 1024;
const PPC_CLASSIC_APP_MEMORY_BASE: u32 = PPC_LOW_MEMORY_SIZE as u32;
pub(crate) const PPC_THE_ZONE_ADDR: u32 = 0x0000_0118;
const PPC_SYS_ZONE_ADDR: u32 = 0x0000_02a6;
const PPC_APPL_ZONE_ADDR: u32 = 0x0000_02aa;
const PPC_MMU_32BIT_ADDR: u32 = 0x0000_0cb2;
const PPC_CLASSIC_APP_MEMORY_SIZE: usize = 0x000f_0000;
pub const PPC_MEM_FULL_ERR: i16 = -108;
pub(crate) const PPC_NIL_HANDLE_ERR: i16 = -109;
pub(crate) const PPC_MEM_WZ_ERR: i16 = -111;
#[cfg(test)]
const PPC_MEM_PUR_ERR: i16 = -112;
const PPC_C_DEPTH_ERR: i16 = -157;
pub const PPC_NO_ERR: i16 = 0;
const PPC_EVT_NOT_ENB: i16 = 1;
pub const PPC_EOF_ERR: i16 = -39;
pub const PPC_FN_OPN_ERR: i16 = -38;
pub const PPC_POS_ERR: i16 = -40;
pub const PPC_NSV_ERR: i16 = -35;
pub const PPC_FNF_ERR: i16 = -43;
pub const PPC_DUP_FN_ERR: i16 = -48;
pub const PPC_RF_NUM_ERR: i16 = -51;
pub const PPC_WR_PERM_ERR: i16 = -61;
pub const PPC_PARAM_ERR: i16 = -50;
pub const PPC_C_RES_ERR: i16 = -156;
pub const PPC_DIR_NF_ERR: i16 = -120;
const PPC_PIXMAP_TOO_DEEP_ERR: i16 = -148;
const PPC_RGN_TOO_BIG_ERR: i16 = -500;
const PPC_OPEN_ERR: i16 = -23;
const PPC_BD_NAM_ERR: i16 = -37;
const PPC_PROC_NOT_FOUND_ERR: i16 = -600;
pub(super) const PPC_NOT_ENOUGH_HARDWARE_ERR: i16 = -201;
const PPC_SM_NO_MORE_SRSRCS_ERR: i16 = -344;
const PPC_NO_MPP_ERR: i16 = -3102;
const PPC_ERR_AE_DESC_NOT_FOUND: i16 = -1701;
const PPC_ERR_AE_COERCION_FAIL: i16 = -1700;
const PPC_ERR_AE_EVENT_NOT_HANDLED: i16 = -1708;
const PPC_AE_BUFFER_IS_SMALL: i16 = -607;
const PPC_HM_HELP_MANAGER_NOT_INITED: i16 = -855;
const PPC_HIGH_LEVEL_EVENT_MASK: u16 = 0x0400;
const PPC_HIGH_LEVEL_EVENT: u16 = 23;
const PPC_CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
const PPC_OPEN_APPLICATION_EVENT: u32 = u32::from_be_bytes(*b"oapp");
const PPC_TYPE_WILDCARD: u32 = u32::from_be_bytes(*b"****");
const PPC_KEY_EVENT_CLASS_ATTR: u32 = u32::from_be_bytes(*b"evcl");
const PPC_KEY_EVENT_ID_ATTR: u32 = u32::from_be_bytes(*b"evid");
const PPC_TYPE_TYPE: u32 = u32::from_be_bytes(*b"type");
pub const PPC_BAD_FORMAT: i16 = -206;
pub const PPC_CHANNEL_NOT_BUSY: i16 = -211;
pub const PPC_GESTALT_UNDEF_SELECTOR_ERR: i16 = -5551;
pub const PPC_GESTALT_DUP_SELECTOR_ERR: i16 = -5552;
pub const PPC_FRAG_LIB_NOT_FOUND: i16 = -2804;
pub const PPC_FRAG_FORMAT_UNKNOWN: i16 = -2806;
pub const PPC_FRAG_HAD_UNRESOLVEDS: i16 = -2807;
pub const PPC_FRAG_NO_MEM: i16 = -2809;
pub const PPC_FRAG_INIT_LOOP: i16 = -2815;
pub const PPC_FRAG_NO_ADDR_SPACE: i16 = -2810;
pub const PPC_FRAG_LIB_CONN_ERR: i16 = -2817;
pub const PPC_FRAG_CONNECTION_ID_NOT_FOUND: i16 = -2801;
pub const PPC_FRAG_SYMBOL_NOT_FOUND: i16 = -2802;
pub const PPC_FRAG_CORRUPT_ERR: i16 = -2820;
pub const PPC_FRAG_USER_INIT_PROC_ERR: i16 = -2821;
pub const PPC_FRAG_ARCH_ERR: i16 = -2823;
pub const PPC_INVALID_COMPONENT_ID: i16 = -3000;
pub const PPC_RES_NOT_FOUND_ERR: i16 = -192;
pub const PPC_RES_F_NOT_FOUND_ERR: i16 = -193;
pub const PPC_RESOURCE_IN_MEMORY_ERR: i16 = -188;
pub const PPC_INPUT_OUT_OF_BOUNDS_ERR: i16 = -190;
pub const PPC_ADD_RES_FAILED: i16 = -194;
pub const PPC_RMV_RES_FAILED: i16 = -196;
pub const PPC_RES_ATTR_ERR: i16 = -198;
pub const PPC_MAP_READ_ERR: i16 = -199;

pub(super) const BLR: u32 = 0x4e80_0020;
pub(super) const PPC_FIRST_FILE_REF_NUM: i16 = 128;
pub(crate) const PPC_CLOSED_RESOURCE_REF_NUM: i16 = i16::MIN;
const PPC_PICT_INFO_SIZE: u32 = 104;
const PPC_CFM_MAIN_STUB_COUNT: u32 = 256;
const PPC_IMPORT_CAPACITY: u32 = 4096;
// The final mapped traps are reserved for guest-call and thread returns and
// the Dialog Manager's guest-callable standard filter procedure. They do not
// reduce the 4,096 application/CFM binding capacity.
const PPC_IMPORT_SLOT_COUNT: u32 = PPC_IMPORT_CAPACITY + 3;
const PPC_THREAD_RETURN_IMPORT_INDEX: u32 = PPC_IMPORT_CAPACITY + 1;
pub(super) const PPC_THREAD_RETURN_PC: u32 =
    PPC_IMPORT_TRAP_BASE + PPC_THREAD_RETURN_IMPORT_INDEX * 4;
const PPC_GUEST_CALL_RETURN_IMPORT_INDEX: u32 = PPC_IMPORT_CAPACITY;
const PPC_STD_FILTER_IMPORT_INDEX: u32 = PPC_IMPORT_CAPACITY + 2;
const PPC_STD_FILTER_TVECTOR: u32 = PPC_IMPORT_TVECTOR_BASE + PPC_STD_FILTER_IMPORT_INDEX * 8;
const PPC_FIRST_CFM_CONNECTION_ID: u32 = 1;
const PPC_CFM_FIND_LIB: u32 = 2;
const PPC_CFM_LOAD_LIB: u32 = 1;
const PPC_CFM_LOAD_NEW_COPY: u32 = 5;
const PPC_CFM_POWERPC_ARCH: u32 = u32::from_be_bytes(*b"pwpc");
const PPC_CFM_ANY_ARCH: u32 = 0x3F3F_3F3F;
#[cfg(test)]
use crate::cfm::CFM_INIT_BLOCK_SIZE as PPC_CFM_INIT_BLOCK_SIZE;
pub(super) const PPC_INITIAL_STACK_FRAME_SIZE: u32 = 64;
const PPC_INTERRUPT_RED_ZONE_SIZE: u32 = 224;
const PPC_PARAMETER_AREA_OFFSET: u32 = 24;
const PPC_LINKAGE_BACK_CHAIN_OFFSET: u32 = 0;
const PPC_LINKAGE_SAVED_CR_OFFSET: u32 = 4;
const PPC_LINKAGE_SAVED_LR_OFFSET: u32 = 8;
const PPC_LINKAGE_SAVED_RTOC_OFFSET: u32 = 20;
pub(super) const PPC_GUEST_CALL_RETURN_PC: u32 =
    PPC_IMPORT_TRAP_BASE + PPC_GUEST_CALL_RETURN_IMPORT_INDEX * 4;
const PPC_INITIALIZERS_TRAMPOLINE_BASE: u32 = PPC_IMPORT_TRAP_BASE - 0x1_0000;
const PPC_APPLICATION_INIT_RETURN_PC: u32 = PPC_IMPORT_TRAP_BASE - 0x100;
const PPC_EXCEPTION_INFORMATION_SIZE: u32 = 24;
const PPC_EXCEPTION_MACHINE_INFORMATION_SIZE: u32 = 64;
const PPC_EXCEPTION_REGISTER_INFORMATION_SIZE: u32 = 256;
const PPC_EXCEPTION_FPU_INFORMATION_SIZE: u32 = 264;
const PPC_EXCEPTION_VECTOR_INFORMATION_SIZE: u32 = 532;
const PPC_EXCEPTION_MEMORY_INFORMATION_SIZE: u32 = 16;
// Universal Interfaces 3.4, MachineExceptions.h defines these exception and
// reference-kind values. Mac OS 8.1 reports status 5 for an unmapped access.
const PPC_ILLEGAL_INSTRUCTION_EXCEPTION: u32 = 1;
const PPC_TRAP_EXCEPTION: u32 = 2;
const PPC_UNMAPPED_MEMORY_EXCEPTION: u32 = 4;
const PPC_UNMAPPED_MEMORY_ERROR: u32 = 5;
const PPC_WRITE_REFERENCE: u32 = 0;
const PPC_READ_REFERENCE: u32 = 1;
const PPC_PROCINFO_CALLING_CONVENTION_MASK: u32 =
    crate::mixed_mode::proc_info::CALLING_CONVENTION_MASK;
const PPC_PROCINFO_PASCAL_STACK_BASED: u32 = crate::mixed_mode::proc_info::PASCAL_STACK_BASED;
const PPC_PROCINFO_C_STACK_BASED: u32 = crate::mixed_mode::proc_info::C_STACK_BASED;
const PPC_PROCINFO_REGISTER_BASED: u32 = crate::mixed_mode::proc_info::REGISTER_BASED;
const PPC_PROCINFO_THINK_C_STACK_BASED: u32 = crate::mixed_mode::proc_info::THINK_C_STACK_BASED;
const PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::D0_DISPATCHED_PASCAL_STACK_BASED;
const PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::D0_DISPATCHED_C_STACK_BASED;
const PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::D1_DISPATCHED_PASCAL_STACK_BASED;
const PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED;
const PPC_PROCINFO_SPECIAL_CASE: u32 = crate::mixed_mode::proc_info::SPECIAL_CASE;
const PPC_PROCINFO_RESULT_SIZE_PHASE: u32 = crate::mixed_mode::proc_info::RESULT_SIZE_PHASE;
const PPC_PROCINFO_STACK_PARAMETER_PHASE: u32 = crate::mixed_mode::proc_info::STACK_PARAMETER_PHASE;
const PPC_PROCINFO_STACK_PARAMETER_WIDTH: u32 = crate::mixed_mode::proc_info::STACK_PARAMETER_WIDTH;
const PPC_PROCINFO_DISPATCHED_SELECTOR_SIZE_PHASE: u32 =
    crate::mixed_mode::proc_info::DISPATCHED_SELECTOR_SIZE_PHASE;
const PPC_PROCINFO_DISPATCHED_PARAMETER_PHASE: u32 =
    crate::mixed_mode::proc_info::DISPATCHED_PARAMETER_PHASE;
const PPC_PROCINFO_REGISTER_RESULT_LOCATION_PHASE: u32 =
    crate::mixed_mode::proc_info::REGISTER_RESULT_LOCATION_PHASE;
const PPC_PROCINFO_REGISTER_PARAMETER_PHASE: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_PHASE;
const PPC_PROCINFO_REGISTER_PARAMETER_WIDTH: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_WIDTH;
const PPC_PROCINFO_REGISTER_PARAMETER_SIZE_MASK: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_SIZE_MASK;
const PPC_PROCINFO_REGISTER_PARAMETER_WHICH_SHIFT: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_WHICH_SHIFT;
const PPC_PROCINFO_REGISTER_PARAMETER_WHICH_MASK: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_WHICH_MASK;
const PPC_PROCINFO_REGISTER_CCR_C: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_C;
const PPC_PROCINFO_REGISTER_CCR_V: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_V;
const PPC_PROCINFO_REGISTER_CCR_Z: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_Z;
const PPC_PROCINFO_REGISTER_CCR_N: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_N;
const PPC_PROCINFO_REGISTER_CCR_X: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_X;
const PPC_CR0_LT_BIT: u8 = 0;
const PPC_CR0_EQ_BIT: u8 = 2;
const PPC_PROCINFO_SIZE_NONE: u32 = crate::mixed_mode::proc_info::SIZE_NONE;
const PPC_PROCINFO_SIZE_ONE: u32 = crate::mixed_mode::proc_info::SIZE_ONE;
const PPC_PROCINFO_SIZE_TWO: u32 = crate::mixed_mode::proc_info::SIZE_TWO;
const PPC_PROCINFO_SIZE_FOUR: u32 = crate::mixed_mode::proc_info::SIZE_FOUR;

pub(crate) const PPC_LIVE_TRAP_IMPORT_WORDS: &[u16] = &[0xA973, 0xA974, 0xA975, 0xA976, 0xA977];
const PPC_PROCINFO_MAX_STACK_PARAMETERS: usize = crate::mixed_mode::proc_info::MAX_STACK_PARAMETERS;
const PPC_PROCINFO_MAX_DISPATCHED_STACK_PARAMETERS: usize =
    crate::mixed_mode::proc_info::MAX_DISPATCHED_STACK_PARAMETERS;
const PPC_PROCINFO_MAX_REGISTER_PARAMETERS: usize =
    crate::mixed_mode::proc_info::MAX_REGISTER_PARAMETERS;
const PPC_CALL_UNIVERSAL_PROC_FIXED_WORD_PARAMETERS: usize = 2;
const PPC_CALL_UNIVERSAL_PROC_REGISTER_VARARGS: usize = 6;
const PPC_NATIVE_PARAMETER_GPR_COUNT: usize = 8;
const PPC_MAX_STACK_SIZE: u32 = PPC_STACK_TOP - PPC_HEAP_BASE;
pub(super) const PPC_RAND_SEED_ADDR: u32 = 0x0000_0156;
const PPC_GRAY_RGN_ADDR: u32 = 0x0000_09ee;
const PPC_DEFAULT_DOUBLE_TIME_TICKS: u32 = 20;
pub(crate) const PPC_RES_CHANGED_ATTR: u16 = 0x0002;
pub(crate) const PPC_RES_PROTECTED_ATTR: u16 = 0x0008;
pub(super) const PPC_RES_PROBLEM: i16 = -204;
const PPC_NO_SCRAP_ERR: i16 = -100;
const PPC_NO_TYPE_ERR: i16 = -102;
const PPC_QUICKTIME_VERSION: u32 = 0x0300_0000;
// The 'q3v ' Gestalt selector uses the 'vers' encoding for QuickDraw 3D 1.6.
// Apple, develop Issue 24 (Dec. 1995), p. 106; Macintosh Toolbox Essentials, p. 1-42.
const PPC_QD3D_VERSION: u32 = 0x0160_8000;
const PPC_MAIN_GDEVICE_RECORD: u32 = 0x02f0_0200;
const PPC_MAIN_GDEVICE_FLAGS: u16 =
    (1 << 0) | (1 << 10) | (1 << 11) | (1 << 12) | (1 << 13) | (1 << 15);
const PPC_MAIN_PIXMAP_HANDLE: u32 = 0x02f0_0300;
const PPC_MAIN_PIXMAP: u32 = 0x02f0_0400;
const PPC_GRAY_RGN_HANDLE: u32 = 0x02f0_0500;
const PPC_GRAY_RGN: u32 = 0x02f0_0600;
const PPC_MAIN_DCE_HANDLE: u32 = 0x02f0_0700;
const PPC_MAIN_DCE: u32 = 0x02f0_0800;
pub(crate) const PPC_MAIN_CTABLE_HANDLE: u32 = 0x02f0_0900;
const PPC_MAIN_CTABLE: u32 = 0x02f0_3000;
pub(crate) const PPC_MAIN_CTABLE_SIZE: u32 = 8 + 256 * 8;
const PPC_MAIN_VIS_RGN_HANDLE: u32 = 0x02f0_0b00;
const PPC_MAIN_VIS_RGN: u32 = 0x02f0_0c00;
const PPC_MAIN_CLIP_RGN_HANDLE: u32 = 0x02f0_0d00;
const PPC_MAIN_CLIP_RGN: u32 = 0x02f0_0e00;
const PPC_MAIN_GAMMA_TABLE: u32 = 0x02f0_1000;
const PPC_MAIN_GAMMA_TABLE_SIZE: u32 = 12 + 256;
const PPC_PORT_LIST_HANDLE: u32 = 0x02f0_1300;
const PPC_PORT_LIST: u32 = 0x02f0_1400;
const PPC_UNIT_TABLE: u32 = 0x02f0_1500;
const PPC_SOUND_DCE_HANDLE: u32 = 0x02f0_1600;
const PPC_SOUND_DCE: u32 = 0x02f0_1700;
const PPC_PORT_LIST_ADDR: u32 = 0x0d66;
pub(crate) const PPC_APPLICATION_ZONE: u32 = 0x02f0_2000;
pub(crate) const PPC_SYSTEM_ZONE: u32 = 0x02f0_2100;
const PPC_ZONE_STORAGE_SIZE: usize = 64;
pub(crate) const PPC_ZONE_HEAP_TYPE_OFFSET: u32 = 30;
pub(crate) const PPC_ZONE_32_BIT_HEAP: u8 = 1;
pub(crate) const PPC_ZONE_NEW_STYLE_HEAP: u8 = 2;
/// Main framebuffer. Placed in the free span below the toolbox structures at
/// [`PPC_MAIN_GWORLD`] rather than in the 960 KB hole under [`PPC_HEAP_BASE`]:
/// that hole held only an 800x600 16-bit buffer (969,600 bytes), so a larger
/// screen ran into the guest heap. Overlapping regions are not an error in
/// `PpcSectionMem` — they set a sticky flag that drops every read and write in
/// the process onto a per-byte path — so the collision cost far more than it
/// announced. `ppc_main_screen_fits` keeps this span honest.
pub(super) const PPC_MAIN_SCREEN_BASE: u32 = 0x02a0_0000;
const PPC_MAIN_PIXEL_DEPTH: u32 = REFERENCE_MACHINE_PROFILE.screen_depth as u32;
pub const PPC_QD_TEXT_FONT_DEFAULT: i16 = 0;
pub const PPC_QD_TEXT_MODE_SRC_OR: i16 = 1;
pub const PPC_QD_TEXT_SIZE_SYSTEM: i16 = 0;
const PPC_QD_PEN_MODE_PAT_COPY: i16 = 8;
const PPC_CGRAF_PORT_PN_LOC_OFFSET: u32 = 48;
const PPC_CGRAF_PORT_PN_SIZE_OFFSET: u32 = 52;
const PPC_CGRAF_PORT_PN_MODE_OFFSET: u32 = 56;
const PPC_CGRAF_PORT_PN_VIS_OFFSET: u32 = 66;
const PPC_CGRAF_PORT_VIS_RGN_OFFSET: u32 = 24;
const PPC_CGRAF_PORT_CLIP_RGN_OFFSET: u32 = 28;
const PPC_CGRAF_PORT_GRAF_VARS_OFFSET: u32 = 8;
const PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET: u32 = 36;
const PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET: u32 = 42;
const PPC_CGRAF_PORT_BK_PIXPAT_OFFSET: u32 = 32;
const PPC_CGRAF_PORT_TX_FONT_OFFSET: u32 = 68;
const PPC_CGRAF_PORT_TX_FACE_OFFSET: u32 = 70;
const PPC_CGRAF_PORT_TX_MODE_OFFSET: u32 = 72;
const PPC_CGRAF_PORT_TX_SIZE_OFFSET: u32 = 74;
const PPC_CGRAF_PORT_RGN_SAVE_OFFSET: u32 = 96;
const PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET: u32 = 156;
const PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET: u32 = 160;
const PPC_GRAF_PORT_SIZE: u32 = 108;
const PPC_GDEVICE_SIZE: u32 = 62;
pub(super) const PPC_PIXMAP_SIZE: u32 = 50;
const PPC_CGRAF_PORT_SIZE: u32 = 170;
const PPC_KEY_RETURN: u8 = 0x24;
const PPC_KEY_A: u8 = 0x00;
const PPC_KEY_B: u8 = 0x0b;
const PPC_KEY_G: u8 = 0x05;
const PPC_KEY_M: u8 = 0x2e;
const PPC_KEY_Q: u8 = 0x0c;
const PPC_KEY_Z: u8 = 0x06;
const PPC_KEY_1: u8 = 0x12;
const PPC_KEY_2: u8 = 0x13;
const PPC_KEY_EQUAL: u8 = 0x18;
const PPC_KEY_MINUS: u8 = 0x1b;
const PPC_KEY_COMMA: u8 = 0x2b;
const PPC_KEY_PERIOD: u8 = 0x2f;
const PPC_KEY_TAB: u8 = 0x30;
const PPC_KEY_SPACE: u8 = 0x31;
const PPC_KEY_ESCAPE: u8 = 0x35;
const PPC_KEY_COMMAND: u8 = 0x37;
const PPC_KEY_SHIFT: u8 = 0x38;
const PPC_KEY_OPTION: u8 = 0x3a;
const PPC_KEY_CONTROL: u8 = 0x3b;
const PPC_KEY_CONTROL_RIGHT: u8 = 0x3e;
const PPC_KEY_LEFT: u8 = 0x7b;
const PPC_KEY_RIGHT: u8 = 0x7c;
const PPC_KEY_DOWN: u8 = 0x7d;
const PPC_KEY_UP: u8 = 0x7e;
const PPC_KEY_NUMPAD_ADD: u8 = 0x45;
const PPC_KEY_NUMPAD_ENTER: u8 = 0x4c;
const PPC_KEY_NUMPAD_SUBTRACT: u8 = 0x4e;
const PPC_KEY_NUMPAD_LEFT: u8 = 0x56;
const PPC_KEY_NUMPAD_DOWN: u8 = 0x57;
const PPC_KEY_NUMPAD_RIGHT: u8 = 0x58;
const PPC_KEY_NUMPAD_UP: u8 = 0x5b;
const PPC_CONTROL_RECORD_SIZE: u32 = 296;
const PPC_CONTROL_NEXT_OFFSET: u32 = 0;
const PPC_CONTROL_OWNER_OFFSET: u32 = 4;
const PPC_CONTROL_RECT_OFFSET: u32 = 8;
const PPC_CONTROL_VISIBLE_OFFSET: u32 = 16;
const PPC_CONTROL_HILITE_OFFSET: u32 = 17;
const PPC_CONTROL_VALUE_OFFSET: u32 = 18;
const PPC_CONTROL_MIN_OFFSET: u32 = 20;
const PPC_CONTROL_MAX_OFFSET: u32 = 22;
const PPC_CONTROL_ACTION_OFFSET: u32 = 32;
const PPC_CONTROL_REF_CON_OFFSET: u32 = 36;
const PPC_CONTROL_TITLE_OFFSET: u32 = 40;
const PPC_KEY_MAP_SIZE: u32 = 16;
const PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 = 4;
const PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES: u64 = 7_296;
const PPC_MICROSECONDS_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 =
    PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD;
const PPC_MICROSECONDS_IDLE_POLL_EXTRA_CYCLES: u64 = PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES;
const PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 =
    PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD;
const PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES: u64 = PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES;
const PPC_TICK_COUNT_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 =
    PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD;
const PPC_FIXED_MAC_TIME: u32 = 3_786_912_000;
pub(super) const PPC_MICROSECONDS_PER_TICK: u64 = 16_625;
pub(super) const PPC_QT_GRAPHICS_IMPORTER: u32 = 0x0500_3000;
const PPC_QT_MOVIE: u32 = 0x0500_3010;
pub(super) const PPC_QT_MOVIE_TASKS_PER_SECOND: u64 = 60;

pub(crate) fn ppc_interrupt_callback_stack_pointer(interrupted_sp: u32) -> u32 {
    // Inside Macintosh: PowerPC System Software (1994), pp. 1-46–1-47:
    // an interrupt handler must skip the 224-byte Red Zone before using the
    // stack because an optimized leaf routine may keep live nonvolatile
    // registers there without allocating a frame.
    interrupted_sp
        .saturating_sub(PPC_INTERRUPT_RED_ZONE_SIZE)
        .saturating_sub(PPC_INITIAL_STACK_FRAME_SIZE)
        & !0xFu32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcMath64Operation {
    LongDoubleToSInt64,
    LongDoubleToUInt64,
    S32Set,
    S64Absolute,
    S64Add,
    S64And,
    S64BitwiseAnd,
    S64BitwiseEor,
    S64BitwiseNot,
    S64BitwiseOr,
    S64Compare,
    S64Divide,
    S64Eor,
    S64Max,
    S64Min,
    S64Multiply,
    S64Negate,
    S64Not,
    S64Or,
    S64Set,
    S64SetU,
    S64ShiftLeft,
    S64ShiftRight,
    S64Subtract,
    SInt64ToLongDouble,
    SInt64ToUInt64,
    U32SetU,
    U64Add,
    U64And,
    U64BitwiseAnd,
    U64BitwiseEor,
    U64BitwiseNot,
    U64BitwiseOr,
    U64Compare,
    U64Divide,
    U64Eor,
    U64Max,
    U64Multiply,
    U64Not,
    U64Or,
    U64Set,
    U64SetU,
    U64ShiftLeft,
    U64ShiftRight,
    U64Subtract,
    UInt64ToLongDouble,
    UInt64ToSInt64,
}

pub use math_compatibility::PpcMathCompatibilityOperation;

pub use dispatch_stdc::PpcStdCCompatibilityOperation;

pub use sound::{PpcSoundInputCompatibilityOperation, PpcSpeechCompatibilityOperation};

pub use dispatch_standard_file::PpcStandardFileOperation;

pub use dispatch_dialog::PpcDialogCompatibilityOperation;

#[cfg(test)]
pub(super) use dispatch_appletalk::ppc_dispatch_appletalk_compatibility;
pub use dispatch_appletalk::PpcAppleTalkCompatibilityOperation;
#[cfg(test)]
pub(super) use dispatch_printing::ppc_dispatch_printing_compatibility;
pub use dispatch_printing::PpcPrintingCompatibilityOperation;
pub use dispatch_quickdraw::PpcQuickDrawCompatibilityOperation;

pub use dispatch_files::PpcFileCompatibilityOperation;

pub use files::{PpcDeleteByNameOperation, PpcParameterBlockCreateOperation};

pub use memory::PpcLegacyMemoryUtilityOperation;

pub use dispatch_window::PpcLegacyWindowOperation;

pub use dispatch_control::PpcLegacyControlOperation;

pub use dispatch_inputsprocket::PpcInputSprocketCompatibilityOperation;

pub use dispatch_apple_events::PpcAppleEventCompatibilityOperation;

pub use dispatch_event::PpcEventPollOperation;

pub use dispatch_collection::PpcCollectionOperation;

/// Backward-compatible native-loader name for the shared event record.
pub type PpcQueuedEvent = QueuedEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PpcCallbackTarget {
    pub(super) entry: u32,
    pub(super) rtoc: u32,
    pub(super) proc_info: u32,
    pub(super) routine_flags: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PpcAppleEventDispatchAllocation {
    resume_guest_call_depth: usize,
    descriptors: u32,
    event_handle: u32,
    reply_handle: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct PpcAppleEventState {
    pub(crate) apple_event_launch_state: SharedProcessAppleEventLaunchState,
    handlers: SharedProcessAppleEventHandlers,
    descriptors: SharedProcessAppleEventDescriptors,
    pending_dispatches: Vec<PpcAppleEventDispatchAllocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcToolboxStartupState {
    gestalt_values: HashMap<u32, u32>,
    pub init_graf_count: u32,
    pub init_graf_global_ptr: u32,
    pub fonts_initialized: bool,
    pub windows_initialized: bool,
    pub menus_initialized: bool,
    pub menu_bar_draw_count: u32,
    pub host_menu_bar_hidden: bool,
    pub(crate) pending_native_menu_selection: SharedNativeMenuSelection,
    pub(crate) execution: ExecutionMenuViews,
    /// Process-owned 68k switch marker/gateway and compatibility stack used
    /// whenever native PowerPC enters classic code through Mixed Mode.
    mixed_mode_m68k: SharedProcessMixedModeM68kState,
    system_allocations: PpcSystemAllocationPool,
    cf_strings: dispatch_core_foundation::PpcCfStringState,
    icon_refs: dispatch_icon_services::PpcIconRefState,
    go_away_tracking: Option<PpcGoAwayTrackingState>,
    drag_window_tracking: Option<PpcDragWindowTrackingState>,
    grow_window_tracking: Option<PpcGrowWindowTrackingState>,
    /// Retained native Standard File calls are resumed at the same import
    /// frame after the host supplies a mouse or keyboard event.
    standard_file_get_filtering: Option<PpcStandardFileFilteringState>,
    standard_file_get_tracking: Option<PpcStandardFileGetTrackingState>,
    standard_file_put_tracking: Option<PpcStandardFilePutTrackingState>,
    pub text_edit_initialized: bool,
    pub dialogs_initialized: bool,
    pub dialog_resume_proc: u32,
    pub flush_events_count: u32,
    pub last_flush_event_mask: u16,
    pub last_flush_stop_mask: u16,
    pub dispose_dialog_count: u32,
    pub last_disposed_dialog: u32,
    /// Most recent EventRecord exposed through the native event imports.
    pub(crate) last_event_record: Option<EventRecordSnapshot>,
    pub(crate) event_queue_probe: EventQueueProbeSnapshot,
    pub(crate) last_button_result: Option<bool>,
    pub(crate) last_still_down_result: Option<bool>,
    pub(crate) last_wait_mouse_up_result: Option<bool>,
    pub(crate) activation_event_seen: bool,
    pub(crate) update_event_seen: bool,
    pub delay_deadline: Option<u32>,
    pub next_ct_seed: u32,
    pub(crate) last_quickdraw_error: SharedProcessQuickDrawError,
    pub open_region_port: u32,
    pub open_region_save_handle: u32,
    pub open_region_bounds: Option<(i16, i16, i16, i16)>,
    /// Exact collected scanlines for framed shapes in an open region.
    pub open_region_rows: Option<(i16, Vec<Vec<i16>>)>,
    /// PicHandle, recording port, frame, and PICT v2 command stream.
    open_picture: Option<(u32, u32, (i16, i16, i16, i16), Vec<u8>)>,
    application_palette: u32,
    application_palette_updates: u16,
    palette_allocations: Vec<PpcPaletteAllocation>,
    active_device_palettes: HashMap<u32, u32>,
    known_gdevices: Vec<u32>,
    indexed_screen_ctables: HashMap<u32, u32>,
    indexed_screen_mode: Option<(u32, bool)>,
    gworld_allocations: HashMap<u32, PpcGWorldAllocationRecord>,
    quickdraw_back_indices: HashMap<u32, u8>,
    pub clut_protected: [bool; 256],
    pub clut_reserved: [bool; 256],
    clut_protected_by_device: HashMap<u32, [bool; 256]>,
    clut_reserved_by_device: HashMap<u32, [bool; 256]>,
    pub quickdraw_pen_pattern: [u8; 8],
    /// Current Color QuickDraw background pattern. A set bit selects the
    /// foreground color and a clear bit selects the background color, as in
    /// the classic `Pattern` record used by BackPat.
    pub quickdraw_back_pattern: [u8; 8],
    pub ae_interaction_allowed: u8,
    pub(crate) stdc_signal_state: PpcStdSignalState,
}

impl Default for PpcToolboxStartupState {
    fn default() -> Self {
        Self {
            gestalt_values: HashMap::new(),
            init_graf_count: 0,
            init_graf_global_ptr: 0,
            fonts_initialized: false,
            windows_initialized: false,
            menus_initialized: false,
            menu_bar_draw_count: 0,
            host_menu_bar_hidden: false,
            pending_native_menu_selection: SharedNativeMenuSelection::default(),
            execution: ExecutionMenuViews::detached(),
            mixed_mode_m68k: SharedProcessMixedModeM68kState::default(),
            system_allocations: PpcSystemAllocationPool::default(),
            cf_strings: dispatch_core_foundation::PpcCfStringState::default(),
            icon_refs: dispatch_icon_services::PpcIconRefState::default(),
            go_away_tracking: None,
            drag_window_tracking: None,
            grow_window_tracking: None,
            standard_file_get_filtering: None,
            standard_file_get_tracking: None,
            standard_file_put_tracking: None,
            text_edit_initialized: false,
            dialogs_initialized: false,
            dialog_resume_proc: 0,
            flush_events_count: 0,
            last_flush_event_mask: 0,
            last_flush_stop_mask: 0,
            dispose_dialog_count: 0,
            last_disposed_dialog: 0,
            last_event_record: None,
            event_queue_probe: EventQueueProbeSnapshot::default(),
            last_button_result: None,
            last_still_down_result: None,
            last_wait_mouse_up_result: None,
            activation_event_seen: false,
            update_event_seen: false,
            delay_deadline: None,
            next_ct_seed: 0,
            last_quickdraw_error: SharedProcessQuickDrawError::default(),
            open_region_port: 0,
            open_region_save_handle: 0,
            open_region_bounds: None,
            open_region_rows: None,
            open_picture: None,
            application_palette: 0,
            application_palette_updates: 0,
            palette_allocations: Vec::new(),
            active_device_palettes: HashMap::new(),
            known_gdevices: vec![PPC_MAIN_GDEVICE],
            indexed_screen_ctables: HashMap::new(),
            indexed_screen_mode: Some((PPC_MAIN_PIXEL_DEPTH, true)),
            gworld_allocations: HashMap::new(),
            quickdraw_back_indices: HashMap::new(),
            clut_protected: [false; 256],
            clut_reserved: [false; 256],
            clut_protected_by_device: HashMap::new(),
            clut_reserved_by_device: HashMap::new(),
            quickdraw_pen_pattern: [0xff; 8],
            quickdraw_back_pattern: [0x00; 8],
            ae_interaction_allowed: 1,
            stdc_signal_state: PpcStdSignalState::default(),
        }
    }
}

impl PpcToolboxStartupState {
    pub(crate) fn retained_host_overlay_rects(&self) -> Vec<(i16, i16, i16, i16)> {
        self.standard_file_get_tracking
            .iter()
            .map(|tracking| tracking.bounds)
            .chain(
                self.standard_file_put_tracking
                    .iter()
                    .map(|tracking| tracking.bounds),
            )
            .collect()
    }

    fn active_menu_definition(&self) -> Option<&MenuDefinitionTracking> {
        self.execution
            .menu()
            .as_ref()
            .and_then(PpcMenuTracking::active_definition)
            .or(self.execution.menu().context().definition.as_ref())
    }

    fn with_active_menu_definition_mut<R>(
        &self,
        update: impl FnOnce(&mut MenuDefinitionTracking) -> R,
    ) -> Option<R> {
        if self
            .execution
            .menu()
            .as_ref()
            .and_then(PpcMenuTracking::active_definition)
            .is_some()
        {
            return self
                .execution
                .with_menu_state_mut(|tracking| tracking.active_definition_mut().map(update))
                .flatten();
        }
        self.execution
            .with_existing_menu_context_mut(|context| context.definition.as_mut().map(update))
            .flatten()
    }

    fn clear_active_menu_definition(&mut self) {
        if self
            .execution
            .with_menu_state_mut(|tracking| tracking.take_active_definition().is_some())
            .unwrap_or(false)
        {
            return;
        }
        self.execution
            .with_existing_menu_context_mut(|context| context.definition = None);
    }
}

pub type PpcHandleRecord = ProcessHandleRecord;
pub type PpcPtrRecord = ProcessPtrRecord;
pub type PpcHandleStateRecord = ProcessHandleStateRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcPixMapBits {
    pub(crate) base_addr: u32,
    pub(crate) row_bytes: u32,
    pub(crate) top: i16,
    pub(crate) left: i16,
    pub(crate) bottom: i16,
    pub(crate) right: i16,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) depth: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PpcResolvedPixMapBits {
    pub(crate) bits: PpcPixMapBits,
    pub(crate) guest_bounds: bool,
    pub(crate) authoritative_bounds: bool,
}

#[derive(Debug, Default)]
pub(crate) struct PpcProcessMemoryManager(SharedProcessMemoryManager);

#[cfg(test)]
struct PpcTestHeapCursor {
    memory_manager: SharedProcessMemoryManager,
    value: u32,
}

#[cfg(test)]
struct PpcTestHandles {
    memory_manager: SharedProcessMemoryManager,
    value: Vec<PpcHandleRecord>,
}

#[cfg(test)]
impl std::ops::Deref for PpcTestHandles {
    type Target = Vec<PpcHandleRecord>;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[cfg(test)]
impl std::ops::DerefMut for PpcTestHandles {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[cfg(test)]
impl Drop for PpcTestHandles {
    fn drop(&mut self) {
        let mut memory_manager = self.memory_manager.borrow_mut();
        let records: Vec<_> = self
            .value
            .iter()
            .map(|record| {
                (
                    *record,
                    memory_manager
                        .state_for_handle(record.handle)
                        .unwrap_or(0x40),
                )
            })
            .collect();
        memory_manager.register_native_handle_records(records);
    }
}

#[cfg(test)]
impl std::ops::Deref for PpcTestHeapCursor {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

#[cfg(test)]
impl std::ops::DerefMut for PpcTestHeapCursor {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

#[cfg(test)]
impl Drop for PpcTestHeapCursor {
    fn drop(&mut self) {
        self.memory_manager
            .borrow_mut()
            .mutate_native_allocator(|allocator| {
                allocator.heap.heap_cursor = self.value;
            });
    }
}

impl Clone for PpcProcessMemoryManager {
    fn clone(&self) -> Self {
        Self(self.0.detached_clone())
    }
}

impl PpcProcessMemoryManager {
    pub(crate) fn with_heap(heap_cursor: u32, heap_limit: u32) -> Self {
        let memory_manager = Self::default();
        {
            let mut process_memory_manager = memory_manager.0.borrow_mut();
            let process_memory_manager = process_memory_manager.native_mut();
            process_memory_manager.publish_native_allocator(
                ProcessNativeHeapState {
                    heap_base: PPC_HEAP_BASE,
                    heap_cursor,
                    heap_limit,
                    last_mem_error: PPC_NO_ERR,
                    heap_maximized: false,
                    master_pointer_blocks_requested: 0,
                },
                &[],
                &[],
                &[],
            );
            // Inside Macintosh: Memory (1992), pp. 2-83--2-85: the
            // application limit is an API-visible heap/stack boundary, not
            // the allocator's physical mapping ceiling.
            process_memory_manager.set_application_heap_limit(heap_limit);
        }
        memory_manager
    }

    fn attach_to(&mut self, memory_manager: SharedProcessMemoryManager) {
        self.0 = memory_manager;
    }

    fn ptr_eq(&self, memory_manager: &SharedProcessMemoryManager) -> bool {
        self.0.ptr_eq(memory_manager)
    }

    fn heap_limit(&self, fallback: u32) -> u32 {
        self.0
            .borrow()
            .native_heap_state()
            .map_or(fallback, |heap| heap.heap_limit)
    }

    fn heap_cursor(&self, fallback: u32) -> u32 {
        self.0
            .borrow()
            .native_heap_state()
            .map_or(fallback, |heap| heap.heap_cursor)
    }

    fn last_mem_error(&self) -> i16 {
        self.0
            .borrow()
            .native_heap_state()
            .map_or(0, |heap| heap.last_mem_error)
    }

    fn application_heap_limit(&self, fallback: u32) -> u32 {
        self.0.borrow().application_heap_limit(fallback)
    }
    #[cfg(test)]
    fn heap_cursor_mut(&self) -> PpcTestHeapCursor {
        PpcTestHeapCursor {
            memory_manager: self.0.clone(),
            value: self.heap_cursor(PPC_HEAP_BASE),
        }
    }

    #[cfg(test)]
    fn handles_mut(&self) -> PpcTestHandles {
        PpcTestHandles {
            memory_manager: self.0.clone(),
            value: self.0.borrow().native_handle_records().to_vec(),
        }
    }

    fn handles(&self) -> Vec<PpcHandleRecord> {
        self.0.borrow().native_handle_records().to_vec()
    }
}

#[derive(Debug, Clone)]
pub struct PpcLoadedApp {
    pub cpu: PpcCpu,
    /// Mapped guest bytes shared by native and emulated 68k execution.
    pub memory: crate::memory::GuestAddressSpace,
    pub entry_pc: u32,
    pub rtoc: u32,
    pub stack_base: u32,
    pub stack_size: u32,
    pub stack_pointer: u32,
    /// Process-scoped host pacing snapshot for the wrapping Macintosh clock.
    /// Guest-visible time is always read from low-memory `Ticks`; this handle
    /// only lets callback scheduling share the last observed value while a
    /// native slice is active.
    pub(crate) tick_state: SharedProcessTickState,
    pub clock_cycles_per_tick: u32,
    pub clock_cycle_phase: u32,
    /// Canonical system-owned trap gateways captured when this native adapter
    /// joins a materialized process. A live table entry equal to one of these
    /// identities still selects the HLE default; every other callable entry is
    /// an application patch and must run through Mixed Mode.
    pub(crate) trap_default_gateways: HashMap<u16, u32>,
    pub native_exception_handler: u32,
    pub(crate) native_exception_stack: Vec<PpcNativeExceptionContext>,
    pub(crate) stdc_qsort_stack: Vec<PpcQsortState>,
    pub(crate) dialog_callback_stack: Vec<PpcDialogCallbackState>,
    pub(crate) collection_callback_stack: Vec<PpcCollectionCallbackState>,
    pub(crate) pending_file_completions: VecDeque<(u32, u32)>,
    pub(crate) apple_events: PpcAppleEventState,
    /// Standalone CFM seed; None after a runner moves it into its process.
    /// Installed execution must receive the process service explicitly.
    pub cfm: Option<PpcCfmState>,
    pub(crate) controls: SharedProcessControlManager,
    pub aliases: Vec<PpcAliasRecord>,
    pub gworlds: Vec<PpcGWorldRecord>,
    /// Process-owned state bits keyed by PixMapHandle. GWorld geometry,
    /// allocation, and rendering records remain in `gworlds`.
    pub(crate) gworld_pixel_states: SharedProcessQuickDrawPixelStates,
    pub q3_objects: Vec<PpcQ3ObjectRecord>,
    pub q3_object_refs: Vec<PpcQ3ObjectReferenceRecord>,
    pub next_q3_object: u32,
    pub q3_error_state: PpcQ3ErrorState,
    pub q3_lifecycle: PpcQ3LifecycleState,
    pub q3_memory_storages: Vec<PpcQ3MemoryStorageRecord>,
    pub q3_files: Vec<PpcQ3FileRecord>,
    pub q3_group_memberships: Vec<PpcQ3GroupMembershipRecord>,
    pub q3_file_groups: Vec<PpcQ3FileGroupRecord>,
    pub q3_views: Vec<PpcQ3ViewStateRecord>,
    pub q3_submissions: Vec<PpcQ3SubmissionRecord>,
    pub q3_view_transforms: Vec<PpcQ3ViewTransformRecord>,
    pub q3_submission_transforms: Vec<PpcQ3SubmissionTransformRecord>,
    pub q3_view_materials: Vec<PpcQ3ViewMaterialRecord>,
    pub q3_submission_materials: Vec<PpcQ3SubmissionMaterialRecord>,
    pub q3_submission_lights: Vec<PpcQ3SubmissionLightRecord>,
    pub q3_view_state_stack: Vec<PpcQ3ViewStateSnapshotRecord>,
    pub q3_completed_frames: Vec<PpcQ3CompletedFrameRecord>,
    pub q3_retained_frames: Vec<PpcQ3RetainedFrameRecord>,
    pub q3_state_only_completed_frame_batches: Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    pub q3_fog_styles: Vec<PpcQ3FogStyleRecord>,
    pub q3_attributes: Vec<PpcQ3AttributeRecord>,
    pub q3_shader_uv_transforms: Vec<PpcQ3ShaderUvTransformRecord>,
    pub q3_shader_boundaries: Vec<PpcQ3ShaderBoundaryRecord>,
    pub q3_mipmap_textures: Vec<PpcQ3MipmapTextureRecord>,
    pub q3_texture_shaders: Vec<PpcQ3TextureShaderRecord>,
    pub q3_renderer_preferences: Vec<PpcQ3RendererPreferenceRecord>,
    pub q3_draw_contexts: Vec<PpcQ3DrawContextRecord>,
    pub q3_trimeshes: Vec<PpcQ3TriMeshRecord>,
    pub q3_styles: Vec<PpcQ3StyleRecord>,
    pub q3_cameras: Vec<PpcQ3CameraRecord>,
    pub q3_lights: Vec<PpcQ3LightRecord>,
    pub input_sprocket: PpcInputSprocketState,
    pub input_sprocket_virtual_elements: Vec<PpcInputSprocketVirtualElementRecord>,
    pub toolbox_startup: PpcToolboxStartupState,
    pub quicktime: PpcQuickTimeState,
    pub sound: PpcSoundState,
    pub(crate) timer_tasks: SharedProcessTimerTasks,
    pub(crate) vbl_tasks: SharedProcessVblTasks,
    pub(crate) callback_scheduling: SharedProcessCallbackScheduling,
    pub(crate) process_file_system: SharedProcessFileSystem,
    pub(crate) current_gworld: SharedProcessGraphicsPort,
    pub(crate) current_gdevice: SharedProcessGraphicsDevice,
    pub(crate) quickdraw_op_colors: SharedProcessQuickDrawOpColors,
    pub(crate) quickdraw_hilite_colors: SharedProcessQuickDrawHiliteColors,
    pub screen_clut: SharedProcessDisplayClut,
    pub color_manager_clut: SharedProcessDisplayClut,
    pub(crate) display_gamma: SharedProcessDisplayGamma,
    /// Whether QuickDraw draw state is canonical in the attached process's
    /// current CGrafPort record and must be reloaded at each import boundary.
    pub(crate) process_quickdraw_port_state_attached: bool,
    pub quickdraw_fore_color: PpcRgbColor,
    pub(crate) quickdraw_fore_indices: HashMap<u32, u8>,
    pub quickdraw_back_color: PpcRgbColor,
    pub quickdraw_pen_h: i16,
    pub quickdraw_pen_v: i16,
    pub quickdraw_text_mode: i16,
    pub quickdraw_text_size: i16,
    pub(crate) cursor_state: SharedProcessCursorState,
    pub(crate) param_text: SharedProcessDialogText,
    pub scrap: PpcScrapState,
    pub(crate) list_manager: PpcListManagerState,
    pub(crate) collections: SharedProcessCollectionManager,
    pub halt_pc: u32,
    pub import_trap_base: u32,
    pub import_count: u32,
    pub imports: Vec<PpcImportBinding>,
    pub section_bases: Vec<Option<u32>>,
    pub input: PpcInputSnapshot,
    pub(crate) process_input: SharedProcessInputState,
    pub(crate) event_queue: SharedProcessEventQueue,
    pub(crate) window_list: crate::process_context::SharedProcessWindowList,
    pub(crate) process_memory_manager: PpcProcessMemoryManager,
    pub draw_sprocket: PpcDrawSprocketState,
}

impl std::ops::Deref for PpcLoadedApp {
    type Target = ProcessFileSystemState;

    fn deref(&self) -> &Self::Target {
        &self.process_file_system
    }
}

pub(super) fn ppc_front_buffer_for_gworld(
    gworlds: &[PpcGWorldRecord],
    gworld: u32,
) -> Option<PpcFrontBuffer> {
    gworlds
        .iter()
        .find(|record| record.port == gworld)
        .map(|record| PpcFrontBuffer {
            base_addr: record.base_addr,
            row_bytes: record.row_bytes,
            width: record.width,
            height: record.height,
            depth: record.depth,
        })
}

pub(super) fn ppc_live_front_buffer_for_gworld(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    gworld: u32,
) -> Option<PpcFrontBuffer> {
    ppc_live_quickdraw_surface(memory, gworlds, gworld).map(|surface| surface.front_buffer)
}

pub(super) fn ppc_live_quickdraw_surface(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    gworld: u32,
) -> Option<PpcQuickDrawSurface> {
    let record = gworlds.iter().find(|record| record.port == gworld);
    // Color QuickDraw permits callers to replace a CGrafPort's portPixMap
    // after OpenCPort. Resolve that live Handle on every drawing operation;
    // the host-side GWorld record describes the port at creation time only.
    // A monochrome GrafPort embeds a BitMap at portBits instead.
    let live_bits = gworld.checked_add(6).and_then(|row_bytes| {
        let is_color = memory.read_u16_be(row_bytes)? & 0x8000 != 0;
        let port_bits = gworld.checked_add(2)?;
        if is_color {
            let pixmap_handle = memory.read_u32_be(port_bits)?;
            let pixmap = memory.read_u32_be(pixmap_handle)?;
            let bits = ppc_read_pixmap_bits(memory, pixmap)?;
            let ctable_handle = pixmap
                .checked_add(42)
                .and_then(|pm_table| memory.read_u32_be(pm_table))
                .filter(|handle| *handle != 0);
            Some((bits, ctable_handle))
        } else {
            let mut bits = ppc_read_pixmap_bits(memory, port_bits)?;
            let row_capacity = bits.row_bytes.checked_mul(8)?.checked_div(bits.depth)?;
            if bits.width > row_capacity {
                // Basic GrafPorts commonly start as a copy of screenBits and
                // are then converted to an offscreen port by replacing only
                // baseAddr/rowBytes and portRect. In that idiom the copied
                // BitMap.bounds can remain screen-sized even though portRect
                // and rowBytes describe the actual backing store. QuickDraw
                // clips drawing to portRect; recover that live extent when
                // the stale bounds cannot possibly fit in one bitmap row.
                // Imaging With QuickDraw (1994), pp. 2-38--2-40, 2-46.
                let (top, left, bottom, right) = ppc_read_rect(memory, gworld + 16)?;
                let (width, height) = ppc_rect_dimensions(top, left, bottom, right);
                if width == 0 || height == 0 || width > row_capacity {
                    return None;
                }
                bits.top = top;
                bits.left = left;
                bits.bottom = bottom;
                bits.right = right;
                bits.width = width;
                bits.height = height;
            }
            Some((bits, None))
        }
    });
    live_bits
        .map(|(bits, ctable_handle)| PpcQuickDrawSurface {
            front_buffer: PpcFrontBuffer {
                base_addr: bits.base_addr,
                row_bytes: bits.row_bytes,
                width: bits.width,
                height: bits.height,
                depth: bits.depth,
            },
            top: bits.top,
            left: bits.left,
            ctable_handle,
        })
        .or_else(|| {
            let record = record?;
            let ctable_handle = (record.pixmap != 0)
                .then_some(record.pixmap)
                .and_then(|pixmap| pixmap.checked_add(42))
                .and_then(|pm_table| memory.read_u32_be(pm_table))
                .filter(|handle| *handle != 0);
            Some(PpcQuickDrawSurface {
                front_buffer: PpcFrontBuffer {
                    base_addr: record.base_addr,
                    row_bytes: record.row_bytes,
                    width: record.width,
                    height: record.height,
                    depth: record.depth,
                },
                top: 0,
                left: 0,
                ctable_handle,
            })
        })
}

fn ppc_live_gworld_ctable_handle(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    gworld: u32,
) -> Option<u32> {
    let record = gworlds.iter().find(|record| record.port == gworld)?;
    let live_pixmap = gworld
        .checked_add(6)
        .and_then(|row_bytes| memory.read_u16_be(row_bytes))
        .filter(|row_bytes| row_bytes & 0x8000 != 0)
        .and_then(|_| gworld.checked_add(2))
        .and_then(|port_bits| memory.read_u32_be(port_bits))
        .and_then(|pixmap_handle| memory.read_u32_be(pixmap_handle));
    live_pixmap
        .or_else(|| (record.pixmap != 0).then_some(record.pixmap))
        .and_then(|pixmap| pixmap.checked_add(42))
        .and_then(|pm_table| memory.read_u32_be(pm_table))
        .filter(|ctable_handle| *ctable_handle != 0)
}

fn ppc_live_gworld_clut(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    gworld: u32,
    _screen_clut: &[[u16; 3]; 256],
    color_manager_clut: &[[u16; 3]; 256],
) -> [[u16; 3]; 256] {
    let Some(ctable_handle) = ppc_live_gworld_ctable_handle(memory, gworlds, gworld) else {
        return *color_manager_clut;
    };
    // Imaging With QuickDraw (1994), "Color Tables": RGB-to-pixel mapping is
    // performed through the destination PixMap's logical ColorTable. A display
    // driver cscSetEntries request may temporarily fade the physical CLUT
    // without replacing that table, so never substitute the host's hardware
    // palette here—even for a PixMap whose baseAddr is the main screen.
    ppc_read_ctable_clut(memory, ctable_handle, color_manager_clut).unwrap_or(*color_manager_clut)
}

fn ppc_front_buffer_8bpp_pixel_addr(
    front_buffer: PpcFrontBuffer,
    (x, y): (i32, i32),
) -> Option<u32> {
    if x < 0 || y < 0 || front_buffer.depth != 8 {
        return None;
    }
    let (x, y) = (x as u32, y as u32);
    if x >= front_buffer.width || y >= front_buffer.height || x >= front_buffer.row_bytes {
        return None;
    }
    front_buffer
        .base_addr
        .checked_add(y.checked_mul(front_buffer.row_bytes)?)?
        .checked_add(x)
}

#[cfg(test)]
fn ppc_rgb_color_to_8bpp_index(color: PpcRgbColor) -> u8 {
    ppc_rgb_color_to_8bpp_index_in_clut(color, &TrapDispatcher::standard_mac_8bpp_clut())
}

#[cfg(test)]
fn ppc_rgb_color_to_8bpp_index_in_clut(color: PpcRgbColor, clut: &[[u16; 3]; 256]) -> u8 {
    ppc_rgb555_to_clut_index(ppc_rgb_color_to_rgb555(color), clut)
}

pub(super) fn ppc_indexed_depth_entry_count(depth: u32) -> Option<usize> {
    matches!(depth, 1 | 2 | 4 | 8).then(|| 1usize << depth)
}

pub(super) fn ppc_rgb_color_to_index_in_clut(
    color: PpcRgbColor,
    clut: &[[u16; 3]; 256],
    entry_count: usize,
) -> u8 {
    ppc_rgb_color_to_valid_index_in_clut(color, clut, &[true; 256], entry_count).unwrap_or(0)
}

fn ppc_rgb_color_to_valid_index_in_clut(
    color: PpcRgbColor,
    clut: &[[u16; 3]; 256],
    valid: &[bool; 256],
    entry_count: usize,
) -> Option<u8> {
    let target = ppc_rgb_color_to_rgb555(color);
    let target_r = i64::from((target >> 10) & 0x1f);
    let target_g = i64::from((target >> 5) & 0x1f);
    let target_b = i64::from(target & 0x1f);
    clut.iter()
        .take(entry_count.min(clut.len()))
        .enumerate()
        .filter(|(index, _)| valid[*index])
        .min_by_key(|(_, [red, green, blue])| {
            let red = i64::from(*red >> 11);
            let green = i64::from(*green >> 11);
            let blue = i64::from(*blue >> 11);
            (red - target_r).pow(2) + (green - target_g).pow(2) + (blue - target_b).pow(2)
        })
        .map(|(index, _)| index as u8)
}

pub(super) fn ppc_quickdraw_surface_color_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    color: PpcRgbColor,
) -> Option<u16> {
    match surface.front_buffer.depth {
        depth @ (1 | 2 | 4 | 8) => {
            // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-81--4-82:
            // Color2Index maps an RGBColor through the destination GDevice's
            // inverse table. For an offscreen PixMap, its own ColorTable is the
            // destination palette, so RGB QuickDraw primitives must not assume
            // the canonical system palette.
            // The same volume, pp. 4-14--4-16 and 4-46--4-47, defines indexed
            // pixel values at 1, 2, 4, and 8 bits. Match only the entries that
            // the destination pixel can represent; matching all 256 and then
            // truncating would select a different color and corrupt its index.
            let fallback = TrapDispatcher::standard_mac_indexed_clut(depth as u16)
                .map(|(clut, _)| clut)
                .unwrap_or_else(TrapDispatcher::standard_mac_8bpp_clut);
            let clut = surface
                .ctable_handle
                .and_then(|handle| ppc_read_ctable_clut(memory, handle, &fallback))
                .unwrap_or(fallback);
            Some(u16::from(ppc_rgb_color_to_index_in_clut(
                color,
                &clut,
                ppc_indexed_depth_entry_count(depth)?,
            )))
        }
        16 => Some(ppc_rgb_color_to_rgb555(color)),
        _ => None,
    }
}

pub(super) fn ppc_quickdraw_surface_fore_pixel(
    memory: &mut PpcSectionMem,
    surface: PpcQuickDrawSurface,
    color: PpcRgbColor,
    explicit_index: Option<u8>,
) -> Option<u16> {
    if let Some(entry_count) = ppc_indexed_depth_entry_count(surface.front_buffer.depth) {
        if let Some(index) = explicit_index {
            return Some(u16::from(index) & (entry_count as u16 - 1));
        }
    }
    ppc_quickdraw_surface_color_pixel(memory, surface, color)
}

pub(super) fn ppc_quickdraw_write_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
    color: PpcRgbColor,
) -> bool {
    match front_buffer.depth {
        depth @ (1 | 2 | 4 | 8) => {
            let fallback = TrapDispatcher::standard_mac_indexed_clut(depth as u16)
                .map(|(clut, _)| clut)
                .unwrap_or_else(TrapDispatcher::standard_mac_8bpp_clut);
            let clut = if front_buffer.base_addr == PPC_MAIN_SCREEN_BASE {
                ppc_read_ctable_clut(memory, PPC_MAIN_CTABLE_HANDLE, &fallback).unwrap_or(fallback)
            } else {
                fallback
            };
            let pixel = ppc_rgb_color_to_index_in_clut(
                color,
                &clut,
                ppc_indexed_depth_entry_count(depth).unwrap_or(1),
            );
            ppc_quickdraw_write_raw_pixel(memory, front_buffer, point, u16::from(pixel))
        }
        16 => {
            ppc_q3_write_software_pixel(memory, front_buffer, point, ppc_rgb_color_to_rgb555(color))
        }
        _ => false,
    }
}

pub(super) fn ppc_quickdraw_read_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
) -> Option<u16> {
    match front_buffer.depth {
        depth @ (1 | 2 | 4) => {
            let (x, y) = point;
            let x = u32::try_from(x).ok()?;
            let y = u32::try_from(y).ok()?;
            if x >= front_buffer.width || y >= front_buffer.height {
                return None;
            }
            let pixels_per_byte = 8 / depth;
            let byte_offset = x / pixels_per_byte;
            if byte_offset >= front_buffer.row_bytes {
                return None;
            }
            let byte = memory.read_u8(
                front_buffer
                    .base_addr
                    .checked_add(y.checked_mul(front_buffer.row_bytes)?)?
                    .checked_add(byte_offset)?,
            )?;
            let shift = 8 - depth - (x % pixels_per_byte) * depth;
            let mask = (1u8 << depth) - 1;
            Some(u16::from((byte >> shift) & mask))
        }
        8 => memory
            .read_u8(ppc_front_buffer_8bpp_pixel_addr(front_buffer, point)?)
            .map(u16::from),
        16 => ppc_q3_read_software_pixel(memory, front_buffer, point),
        _ => None,
    }
}

pub(super) fn ppc_quickdraw_write_raw_pixel(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    point: (i32, i32),
    value: u16,
) -> bool {
    match front_buffer.depth {
        depth @ (1 | 2 | 4) => {
            let (x, y) = point;
            let (Ok(x), Ok(y)) = (u32::try_from(x), u32::try_from(y)) else {
                return false;
            };
            if x >= front_buffer.width || y >= front_buffer.height {
                return false;
            }
            let Some(row_offset) = y.checked_mul(front_buffer.row_bytes) else {
                return false;
            };
            let pixels_per_byte = 8 / depth;
            let byte_offset = x / pixels_per_byte;
            if byte_offset >= front_buffer.row_bytes {
                return false;
            }
            let Some(addr) = front_buffer
                .base_addr
                .checked_add(row_offset)
                .and_then(|row| row.checked_add(byte_offset))
            else {
                return false;
            };
            let Some(byte) = memory.read_u8(addr) else {
                return false;
            };
            let shift = 8 - depth - (x % pixels_per_byte) * depth;
            let value_mask = (1u8 << depth) - 1;
            let pixel_mask = value_mask << shift;
            let packed = ((value as u8) & value_mask) << shift;
            memory
                .write_u8(addr, (byte & !pixel_mask) | packed)
                .is_some()
        }
        8 => {
            let Some(addr) = ppc_front_buffer_8bpp_pixel_addr(front_buffer, point) else {
                return false;
            };
            memory.write_u8(addr, value as u8).is_some()
        }
        16 => ppc_q3_write_software_pixel(memory, front_buffer, point, value),
        _ => false,
    }
}

pub(crate) fn ppc_zero_guest_bytes(memory: &mut PpcSectionMem, addr: u32, len: u32) -> bool {
    if addr == 0 || !ppc_memory_can_write_bytes(memory, addr, len) {
        return false;
    }
    for offset in 0..len {
        if memory.write_u8(addr + offset, 0).is_none() {
            return false;
        }
    }
    true
}

mod math_compatibility;
use math_compatibility::*;

fn ppc_math_ceil(cpu: &mut PpcCpu) {
    // Inside Macintosh: PowerPC Numerics (1994), pp. 9-7--9-8:
    // ceil rounds upward without an inexact exception and preserves signed
    // zero, infinities, and quiet NaNs. A signaling NaN raises invalid and
    // returns the corresponding quiet NaN.
    const EXPONENT_MASK: u64 = 0x7ff0_0000_0000_0000;
    const FRACTION_MASK: u64 = 0x000f_ffff_ffff_ffff;
    const QUIET_NAN_BIT: u64 = 0x0008_0000_0000_0000;
    let bits = cpu.fpr[1];
    let signaling_nan = bits & EXPONENT_MASK == EXPONENT_MASK
        && bits & FRACTION_MASK != 0
        && bits & QUIET_NAN_BIT == 0;
    if signaling_nan {
        cpu.fpr[1] = bits | QUIET_NAN_BIT;
        cpu.set_fpscr_bit(0, true);
        cpu.set_fpscr_bit(2, true);
        cpu.set_fpscr_bit(7, true);
        if cpu.fpscr_bit(24) {
            cpu.set_fpscr_bit(1, true);
        }
    } else {
        cpu.fpr[1] = f64::from_bits(bits).ceil().to_bits();
    }
}

fn ppc_math_fmod(cpu: &mut PpcCpu) {
    // The PowerPC C ABI passes the two double arguments in f1/f2 and returns
    // the remainder in f1. Rust's floating remainder has C fmod semantics:
    // its magnitude is less than the divisor and its sign follows the dividend.
    let dividend = f64::from_bits(cpu.fpr[1]);
    let divisor = f64::from_bits(cpu.fpr[2]);
    cpu.fpr[1] = (dividend % divisor).to_bits();
}

fn ppc_math_dtox80(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> bool {
    let Some(hi) = memory.read_u32_be(cpu.gpr[3]) else {
        return false;
    };
    let Some(lo) = memory.read_u32_be(cpu.gpr[3].wrapping_add(4)) else {
        return false;
    };
    let extended = Extended80::from(f64::from_bits((u64::from(hi) << 32) | u64::from(lo)));
    let sign_bit = if extended.sign { 0x8000 } else { 0 };
    let out = cpu.gpr[4];

    // Universal Interfaces fp.h (MathLib 2), `dtox80(const double *, extended80 *)`:
    // convert a PowerPC double into the 10-byte big-endian 68K extended format.
    memory
        .write_u16_be(out, sign_bit | extended.exponent)
        .is_some()
        && memory
            .write_u16_be(out.wrapping_add(2), (extended.significand >> 48) as u16)
            .is_some()
        && memory
            .write_u16_be(out.wrapping_add(4), (extended.significand >> 32) as u16)
            .is_some()
        && memory
            .write_u16_be(out.wrapping_add(6), (extended.significand >> 16) as u16)
            .is_some()
        && memory
            .write_u16_be(out.wrapping_add(8), extended.significand as u16)
            .is_some()
}

fn ppc_virtual_microseconds(
    tick_count: u32,
    cycles_per_tick: u32,
    cycle_phase: u32,
    elapsed_cycles: u64,
) -> u64 {
    let cycles_per_tick = u64::from(cycles_per_tick.max(1));
    let elapsed_cycles = u64::from(cycle_phase).saturating_add(elapsed_cycles);
    u64::from(tick_count)
        .saturating_mul(PPC_MICROSECONDS_PER_TICK)
        .saturating_add(elapsed_cycles.saturating_mul(PPC_MICROSECONDS_PER_TICK) / cycles_per_tick)
}

fn ppc_virtual_tick_count(
    tick_count: u32,
    cycles_per_tick: u32,
    cycle_phase: u32,
    elapsed_cycles: u64,
) -> u32 {
    // Inside Macintosh: Processes (1993), p. 3-46: TickCount is the low-memory
    // time counter maintained by the vertical retrace interrupt. The native
    // runner keeps the persisted counter at the start of an execution slice,
    // so imports within that slice must include elapsed guest cycles. This is
    // especially important when an idle poll is accelerated with extra cycles:
    // inventing a caller-local future tick can make two Toolbox clock reads
    // disagree and send applications down their fatal startup path.
    let cycles_per_tick = u64::from(cycles_per_tick.max(1));
    let elapsed_cycles = u64::from(cycle_phase).saturating_add(elapsed_cycles);
    tick_count.wrapping_add((elapsed_cycles / cycles_per_tick) as u32)
}

fn ppc_cycles_until_next_tick(cycles_per_tick: u32, cycle_phase: u32, elapsed_cycles: u64) -> u64 {
    let cycles_per_tick = u64::from(cycles_per_tick.max(1));
    let cycle_phase = u64::from(cycle_phase).saturating_add(elapsed_cycles) % cycles_per_tick;
    cycles_per_tick - cycle_phase
}

fn dispatch_simple_hot_import_fast(
    target: &PpcImportDispatcherTarget,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    microseconds: u64,
) -> Option<PpcImportAction> {
    match target {
        PpcImportDispatcherTarget::Microseconds => Some(dispatch_microseconds_import(
            cpu,
            memory,
            microseconds,
            None,
        )),
        PpcImportDispatcherTarget::AbsoluteToNanoseconds => {
            // PowerPC's struct-return ABI places the output pointer in r3 and
            // the 64-bit AbsoluteTime input in r4:r5. Our virtual absolute
            // clock counts microseconds, so conversion to nanoseconds is exact.
            let output = cpu.gpr[3];
            let absolute = (u64::from(cpu.gpr[4]) << 32) | u64::from(cpu.gpr[5]);
            if output != 0 && ppc_memory_can_write_bytes(memory, output, 8) {
                let _ = memory.write_u64_be(output, absolute.saturating_mul(1_000));
            }
            Some(PpcImportAction::ReturnPreserve)
        }
        _ => dispatch_math::dispatch_math_import(target, cpu, memory),
    }
}

pub(super) fn ppc_random(memory: &mut PpcSectionMem) -> u16 {
    let old_seed = memory.read_u32_be(PPC_RAND_SEED_ADDR).unwrap_or(1);
    let seed = if old_seed == 0 { 1 } else { old_seed };
    let new_seed = ((u64::from(seed) * 16_807) % 2_147_483_647) as u32;
    let _ = memory.write_u32_be(PPC_RAND_SEED_ADDR, new_seed);
    let result = new_seed as u16;
    if result == 0x8000 {
        0
    } else {
        result
    }
}

fn ppc_string_to_num(cpu: &PpcCpu, memory: &mut PpcSectionMem) {
    let string_ptr = cpu.gpr[3];
    let number_ptr = cpu.gpr[4];
    if string_ptr == 0 || number_ptr == 0 || !ppc_memory_can_write_bytes(memory, number_ptr, 4) {
        return;
    }
    let Some(bytes) = ppc_read_pstring_bytes(memory, string_ptr) else {
        return;
    };
    let mut index = 0usize;
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        index += 1;
    }
    let mut sign = 1i64;
    if let Some(byte) = bytes.get(index) {
        if *byte == b'-' {
            sign = -1;
            index += 1;
        } else if *byte == b'+' {
            index += 1;
        }
    }
    let mut value = 0i64;
    while let Some(byte) = bytes.get(index) {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(i64::from(byte - b'0'));
        index += 1;
    }
    let signed = value
        .saturating_mul(sign)
        .clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let _ = memory.write_u32_be(number_ptr, signed as u32);
}

pub(super) fn ppc_write_pstring_bytes(memory: &mut PpcSectionMem, addr: u32, bytes: &[u8]) -> bool {
    let len = bytes.len().min(255);
    if memory.write_u8(addr, len as u8).is_none() {
        return false;
    }
    for (offset, byte) in bytes.iter().copied().take(len).enumerate() {
        let Some(byte_addr) = addr.checked_add(1 + offset as u32) else {
            return false;
        };
        if memory.write_u8(byte_addr, byte).is_none() {
            return false;
        }
    }
    true
}

fn ppc_f64_to_fixed(value: f64) -> u32 {
    (value * 65536.0)
        .round()
        .clamp(i32::MIN as f64, i32::MAX as f64) as i32 as u32
}

fn ppc_sys_environs(memory: &mut PpcSectionMem, rec_ptr: u32) -> i16 {
    if rec_ptr < 0x100 || !ppc_memory_can_write_bytes(memory, rec_ptr, 16) {
        return PPC_PARAM_ERR;
    }
    let _ = memory.write_u16_be(rec_ptr, 2);
    let _ = memory.write_u16_be(rec_ptr + 2, REFERENCE_MACHINE_PROFILE.gestalt_machine_type);
    let _ = memory.write_u16_be(rec_ptr + 4, POWERPC_SYSTEM_VERSION_BCD);
    let _ = memory.write_u16_be(
        rec_ptr + 6,
        REFERENCE_MACHINE_PROFILE.gestalt_processor_type as u16,
    );
    let _ = memory.write_u8(rec_ptr + 8, u8::from(REFERENCE_MACHINE_PROFILE.has_fpu()));
    let _ = memory.write_u8(rec_ptr + 9, 1);
    let _ = memory.write_u16_be(rec_ptr + 10, 0);
    let _ = memory.write_u16_be(rec_ptr + 12, 0);
    let _ = memory.write_u16_be(rec_ptr + 14, 0);
    PPC_NO_ERR
}

#[cfg(test)]
fn ppc_raw_trap_table_entry(trap_word: u16, toolbox: bool) -> u32 {
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    TrapManager::table_address(trap_word, kind)
}

pub(crate) fn ppc_logical_trap_address(
    memory: &mut PpcSectionMem,
    trap_word: u16,
    toolbox: bool,
) -> Option<u32> {
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    let protected_memory = memory.shared_view();
    TrapManager::get_address_with_provenance(
        trap_word,
        kind,
        |operation| match operation {
            TrapManagerMemoryOp::ReadLong(address) => {
                Some(TrapManagerMemoryResult::Long(memory.read_u32_be(address)?))
            }
            TrapManagerMemoryOp::WriteLong { .. }
            | TrapManagerMemoryOp::WriteProtectedLong { .. } => None,
        },
        move |address| protected_memory.is_shared_readonly_range(address, 4),
    )
}

/// Resolve a native import through the process's live Trap Manager entry
/// before any HLE shortcut runs. The default identity comes from the system
/// gateway registry, never from opcode inspection, so saved defaults remain
/// callable while direct table writes and SetTrapAddress patches take effect
/// immediately.
fn ppc_live_trap_import_action(
    target: &PpcImportDispatcherTarget,
    default_gateways: &HashMap<u16, u32>,
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    toolbox_startup: &mut PpcToolboxStartupState,
) -> Result<Option<PpcImportAction>, ()> {
    let (trap_word, toolbox, proc_info, argument_count) = match target {
        // pascal Boolean StillDown(void)
        PpcImportDispatcherTarget::StillDown => (0xA973, true, 0x10, 0),
        // pascal Boolean Button(void)
        PpcImportDispatcherTarget::Button => (0xA974, true, 0x10, 0),
        // pascal LONGINT TickCount(void)
        PpcImportDispatcherTarget::TickCount => (0xA975, true, 0x30, 0),
        // pascal void GetKeys(KeyMap *)
        PpcImportDispatcherTarget::GetKeys => (0xA976, true, 0xC0, 1),
        // pascal Boolean WaitMouseUp(void)
        PpcImportDispatcherTarget::WaitMouseUp => (0xA977, true, 0x10, 0),
        _ => return Ok(None),
    };
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    let table_entry = TrapManager::table_address(trap_word, kind);
    if memory.read_u32_be(table_entry).is_none() {
        // A detached PEF adapter has no process trap topology until the runner
        // attaches it. That standalone case retains the HLE import behavior.
        return Ok(None);
    }
    let handler = ppc_logical_trap_address(memory, trap_word, toolbox).ok_or(())?;
    let canonical_word = crate::trap::manager::raw_trap_route(trap_word).canonical_word;
    if default_gateways.get(&canonical_word) == Some(&handler) {
        return Ok(None);
    }
    if handler == 0 {
        return if default_gateways.is_empty() {
            // Standalone PEF construction currently exposes zero-filled low
            // memory without materializing system gateways (#1491).
            Ok(None)
        } else {
            Err(())
        };
    }

    let heap = process_memory_manager.native_heap_state().ok_or(())?;
    if argument_count > 6 {
        return Err(());
    }
    let mut saved_arguments = [0; 8];
    saved_arguments.copy_from_slice(&cpu.gpr[3..11]);
    for index in (0..argument_count).rev() {
        cpu.gpr[5 + index] = saved_arguments[index];
    }
    cpu.gpr[3] = handler;
    cpu.gpr[4] = proc_info;
    let mut heap_cursor = heap.heap_cursor;
    let heap_limit = process_memory_manager.native_allocation_limit(heap.heap_limit);
    let action = ppc_call_universal_proc(
        cpu,
        process_memory_manager,
        memory,
        &mut heap_cursor,
        heap_limit,
        toolbox_startup,
        GuestIsa::M68k,
    );
    if action.is_none() {
        cpu.gpr[3..11].copy_from_slice(&saved_arguments);
        return Err(());
    }
    Ok(action)
}

pub(crate) fn ppc_set_logical_trap_address(
    memory: &mut PpcSectionMem,
    trap_word: u16,
    toolbox: bool,
    handler: u32,
) -> bool {
    let kind = if toolbox {
        TrapTableKind::Toolbox
    } else {
        TrapTableKind::OperatingSystem
    };
    let protected_memory = memory.shared_view();
    let result = TrapManager::set_address_with_provenance(
        trap_word,
        kind,
        handler,
        |operation| match operation {
            TrapManagerMemoryOp::ReadLong(address) => {
                Some(TrapManagerMemoryResult::Long(memory.read_u32_be(address)?))
            }
            TrapManagerMemoryOp::WriteLong { address, value } => memory
                .write_u32_be(address, value)
                .map(|()| TrapManagerMemoryResult::Written),
            TrapManagerMemoryOp::WriteProtectedLong { address, value } => memory
                .write_shared_system_u32_be(address, value)
                .map(|()| TrapManagerMemoryResult::Written),
        },
        move |address| protected_memory.is_shared_readonly_range(address, 4),
    );
    if matches!(result, Err(TrapManagerSetError::InvalidComeFromHead)) {
        // NSetTrapAddress raises system error 12 for this malformed splice.
        // Inside Macintosh: Operating System Utilities (1994), p. 8-30.
        let _ = memory.write_u16_be(crate::memory::globals::addr::DS_ERR_CODE, 12);
    }
    result.is_ok()
}

pub(super) fn ppc_write_rect(
    memory: &mut PpcSectionMem,
    rect_ptr: u32,
    top: i16,
    left: i16,
    bottom: i16,
    right: i16,
) -> Option<()> {
    memory.write_u16_be(rect_ptr, top as u16)?;
    memory.write_u16_be(rect_ptr + 2, left as u16)?;
    memory.write_u16_be(rect_ptr + 4, bottom as u16)?;
    memory.write_u16_be(rect_ptr + 6, right as u16)?;
    Some(())
}

pub(super) fn ppc_read_rect(
    memory: &mut PpcSectionMem,
    rect_ptr: u32,
) -> Option<(i16, i16, i16, i16)> {
    let top = memory.read_u16_be(rect_ptr)? as i16;
    let left = memory.read_u16_be(rect_ptr + 2)? as i16;
    let bottom = memory.read_u16_be(rect_ptr + 4)? as i16;
    let right = memory.read_u16_be(rect_ptr + 6)? as i16;
    Some((top, left, bottom, right))
}

pub(super) fn ppc_read_pstring(memory: &mut PpcSectionMem, addr: u32) -> Option<String> {
    Some(decode_mac_roman(&ppc_read_pstring_bytes(memory, addr)?))
}

fn ppc_equal_string(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> u32 {
    let a_ptr = cpu.gpr[3];
    let b_ptr = cpu.gpr[4];
    let case_sensitive = (cpu.gpr[5] & 0xff) != 0;
    let _diac_sensitive = (cpu.gpr[6] & 0xff) != 0;
    let Some(a_bytes) = ppc_read_pstring_bytes(memory, a_ptr) else {
        return 0;
    };
    let Some(b_bytes) = ppc_read_pstring_bytes(memory, b_ptr) else {
        return 0;
    };
    let equal = if case_sensitive {
        a_bytes == b_bytes
    } else {
        a_bytes.eq_ignore_ascii_case(&b_bytes)
    };

    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] EqualString a={:?} b={:?} case_sensitive={} -> {}",
            decode_mac_roman(&a_bytes),
            decode_mac_roman(&b_bytes),
            case_sensitive,
            equal
        );
    }

    u32::from(equal)
}

pub(super) fn ppc_read_pstring_bytes(memory: &mut PpcSectionMem, addr: u32) -> Option<Vec<u8>> {
    let len = memory.read_u8(addr)? as usize;
    let mut bytes = Vec::with_capacity(len);
    for offset in 0..len {
        bytes.push(memory.read_u8(addr.checked_add(1 + offset as u32)?)?);
    }
    Some(bytes)
}

pub(super) fn ppc_i16_result(value: i16) -> u32 {
    i32::from(value) as u32
}

pub(crate) fn ppc_run_result_cycles(result: PpcRunResult) -> u64 {
    match result {
        PpcRunResult::CycleLimit { cycles }
        | PpcRunResult::Halted { cycles, .. }
        | PpcRunResult::Unimplemented { cycles, .. }
        | PpcRunResult::MemoryFault { cycles, .. }
        | PpcRunResult::Exception { cycles, .. }
        | PpcRunResult::FetchFault { cycles, .. } => cycles,
    }
}

fn ppc_run_result_with_cycles(result: PpcRunResult, cycles: u64) -> PpcRunResult {
    match result {
        PpcRunResult::CycleLimit { .. } => PpcRunResult::CycleLimit { cycles },
        PpcRunResult::Halted { pc, .. } => PpcRunResult::Halted { pc, cycles },
        PpcRunResult::Unimplemented { pc, error, .. } => {
            PpcRunResult::Unimplemented { pc, error, cycles }
        }
        PpcRunResult::MemoryFault {
            pc,
            addr,
            was_write,
            ..
        } => PpcRunResult::MemoryFault {
            pc,
            addr,
            was_write,
            cycles,
        },
        PpcRunResult::Exception { pc, exception, .. } => PpcRunResult::Exception {
            pc,
            exception,
            cycles,
        },
        PpcRunResult::FetchFault { pc, .. } => PpcRunResult::FetchFault { pc, cycles },
    }
}

struct PpcRetiredThreadStorageEdge<'a> {
    manager: &'a mut ProcessNativeMemoryManager,
}

impl RetiredThreadStorageEdge for PpcRetiredThreadStorageEdge<'_> {
    fn release_classic(&mut self, stack_base: u32) {
        self.manager
            .dispose_classic_ptr_from_native_import(stack_base);
    }

    fn release_native(&mut self, stack_base: u32) {
        self.manager.dispose_native_ptr(stack_base);
    }
}

pub(super) fn ppc_release_retired_thread_storage(
    manager: &mut ProcessNativeMemoryManager,
    retirement: NativeRetirement,
    recycle: bool,
) {
    let storage = match retirement {
        NativeRetirement::Removed(storage) | NativeRetirement::Switched(storage) => storage,
    };
    ThreadManager::release_retired_storage(
        storage,
        recycle,
        &mut PpcRetiredThreadStorageEdge { manager },
    );
}

pub(super) fn ppc_resolve_callback_target(
    memory: &mut PpcSectionMem,
    proc_ptr: u32,
    default_rtoc: u32,
    selector: Option<u32>,
) -> Option<PpcCallbackTarget> {
    let procedure = resolve_guest_procedure(
        memory,
        proc_ptr,
        default_rtoc,
        selector,
        GuestIsa::PowerPc,
        GuestIsa::PowerPc,
    )?;
    (procedure.isa == GuestIsa::PowerPc).then_some(PpcCallbackTarget {
        entry: procedure.entry,
        rtoc: procedure.rtoc,
        proc_info: procedure.proc_info,
        routine_flags: procedure.routine_flags,
    })
}

fn import_data_address_for(library_name: &str, symbol_name: &str) -> Option<u32> {
    match (library_name, symbol_name) {
        ("StdCLib", "_IntEnv") => Some(PPC_IMPORT_DATA_BASE),
        ("StdCLib", "__C_phase") => Some(PPC_IMPORT_DATA_BASE + 0x400),
        ("StdCLib", "__target_for_exit") => Some(PPC_IMPORT_DATA_BASE + 0x404),
        ("StdCLib", "_exit_status") => Some(PPC_IMPORT_DATA_BASE + 0x408),
        ("StdCLib", "_iob") => Some(PPC_STDIO_IOB_ADDR),
        ("StdCLib", "__p_CType") => Some(PPC_IMPORT_CTYPE_POINTER),
        ("StdCLib", "_DBL_EPSILON") => Some(PPC_IMPORT_STD_DBL_EPSILON),
        ("StdCLib", "_DBL_MAX") => Some(PPC_IMPORT_STD_DBL_MAX),
        ("StdCLib", "_DBL_MIN") => Some(PPC_IMPORT_STD_DBL_MIN),
        ("StdCLib", "_FLT_EPSILON") => Some(PPC_IMPORT_STD_FLT_EPSILON),
        ("StdCLib", "_FLT_MAX") => Some(PPC_IMPORT_STD_FLT_MAX),
        ("StdCLib", "_FLT_MIN") => Some(PPC_IMPORT_STD_FLT_MIN),
        ("StdCLib", "errno") => Some(PPC_IMPORT_STD_ERRNO),
        ("StdCLib", "MacOSErr") => Some(PPC_IMPORT_STD_MAC_OS_ERR),
        // PowerPC Numerics exposes `pi` as an addressable MathLib export.
        // Some CFM clients label the import as a transition-vector symbol but
        // dereference it directly as a double, so bind the known export to
        // stable data storage regardless of the PEF class tag.
        ("MathLib", "pi") => Some(PPC_IMPORT_MATH_PI),
        ("MathLib", "_FE_DFL_ENV") => Some(PPC_IMPORT_MATH_FE_DFL_ENV),
        _ => None,
    }
}

fn ppc_seed_import_data(memory: &mut PpcSectionMem) {
    // The Metrowerks runtime imports these as true CFM data symbols, not
    // callable entry points. Give them stable writable storage and point the
    // ctype indirection at a complete 256-entry classification table.
    let _ = memory.write_u32_be(PPC_IMPORT_CTYPE_POINTER, PPC_IMPORT_CTYPE_TABLE);
    let _ = memory.write_u64_be(PPC_IMPORT_MATH_PI, std::f64::consts::PI.to_bits());
    // Inside Macintosh: PowerPC Numerics (1994), Chapter 8 and Appendix C:
    // PowerPC fenv_t is a 32-bit word, with zero selecting round-to-nearest
    // and leaving every floating-point exception flag clear.
    let _ = memory.write_u32_be(PPC_IMPORT_MATH_FE_DFL_ENV, 0);
    // Universal Interfaces 3.4 float.h exposes these values by dereferencing
    // imported StdCLib objects. Seed their exact IEEE single- and
    // double-precision representations in big-endian guest memory.
    let _ = memory.write_u64_be(PPC_IMPORT_STD_DBL_EPSILON, f64::EPSILON.to_bits());
    let _ = memory.write_u64_be(PPC_IMPORT_STD_DBL_MAX, f64::MAX.to_bits());
    let _ = memory.write_u64_be(PPC_IMPORT_STD_DBL_MIN, f64::MIN_POSITIVE.to_bits());
    let _ = memory.write_u32_be(PPC_IMPORT_STD_FLT_EPSILON, f32::EPSILON.to_bits());
    let _ = memory.write_u32_be(PPC_IMPORT_STD_FLT_MAX, f32::MAX.to_bits());
    let _ = memory.write_u32_be(PPC_IMPORT_STD_FLT_MIN, f32::MIN_POSITIVE.to_bits());
    // Universal Interfaces 3.4 errno.h declares StdCLib's writable `errno`
    // as an int and `MacOSErr` as a short. Both begin clear at process launch.
    let _ = memory.write_u32_be(PPC_IMPORT_STD_ERRNO, 0);
    let _ = memory.write_u16_be(PPC_IMPORT_STD_MAC_OS_ERR, 0);
    let _ = memory.write_bytes(
        PPC_STDIO_IOB_ADDR,
        &vec![0; (3 * PPC_STDIO_FILE_SIZE) as usize],
    );
    for byte in 0u16..=255 {
        let _ = memory.write_u8(
            PPC_IMPORT_CTYPE_TABLE + u32::from(byte),
            ppc_ctype_entry(byte as u8),
        );
    }
}

#[cfg(test)]
pub(crate) mod tests;
