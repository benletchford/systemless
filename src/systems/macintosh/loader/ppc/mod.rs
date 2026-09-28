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
const PPC_PIXMAP_SIZE: u32 = 50;
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
const PPC_MATH_HOT_IMPORT_EXTRA_CYCLES: u64 = 128;
// TickCount is maintained by the vertical retrace interrupt, so it advances
// while the Toolbox draws on the application's behalf. Inside Macintosh:
// Processes (1993), p. 3-46. HLE QuickDraw and Resource Manager imports do
// that work on the host and otherwise charge no guest cycles, so a loop that
// redraws until TickCount changes sees drawing as free and repeats a full
// redraw many times within one tick at real host cost. Inside Macintosh
// gives no per-call timings; these fixed charges are a deliberate
// approximation of a 120 MHz 604 (DrawText ~6 us, CopyBits ~9 us,
// DrawPicture ~17 us). They are lower bounds -- large pictures and transfers
// take far longer on hardware -- so an application redrawing a few hundred
// items still has most of its tick left for its own code.
// A redraw of ~370 text calls, 165 pictures and 135 transfers costs under
// half of one tick's cycles.
const PPC_DRAW_TEXT_IMPORT_EXTRA_CYCLES: u64 = 768;
const PPC_MEASURE_TEXT_IMPORT_EXTRA_CYCLES: u64 = 256;
const PPC_DRAW_PICTURE_IMPORT_EXTRA_CYCLES: u64 = 2_048;
const PPC_BIT_TRANSFER_IMPORT_EXTRA_CYCLES: u64 = 1_024;
const PPC_DRAW_PRIMITIVE_IMPORT_EXTRA_CYCLES: u64 = 256;
const PPC_RESOURCE_IMPORT_EXTRA_CYCLES: u64 = 128;
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

struct PpcHleFetchObserver<'a> {
    histogram: Option<&'a mut PpcFetchHistogram>,
    trace_fetches: bool,
    trace_pc_range: Option<(u32, u32)>,
}

impl PpcFetchObserver for PpcHleFetchObserver<'_> {
    fn on_fetch(&mut self, pc: u32, word: u32) {
        if self.trace_fetches
            || self
                .trace_pc_range
                .map(|(start, end)| pc >= start && pc <= end)
                .unwrap_or(false)
        {
            eprintln!("{}", format_ppc_trace_fetch(pc, word));
        }
        if let Some(histogram) = self.histogram.as_deref_mut() {
            histogram.on_fetch(pc, word);
        }
    }

    fn on_fetch_cpu(&mut self, cpu: &PpcCpu, word: u32) {
        let pc = cpu.pc;
        let in_range = self
            .trace_pc_range
            .map(|(start, end)| pc >= start && pc <= end)
            .unwrap_or(false);
        let r27_matches = ppc_trace_regs_r27_filter()
            .map(|expected| cpu.gpr[27] == expected)
            .unwrap_or(true);
        let r3_matches = ppc_trace_regs_r3_filter()
            .map(|expected| cpu.gpr[3] == expected)
            .unwrap_or(true);
        if ppc_trace_regs_enabled() && in_range && r27_matches && r3_matches {
            eprintln!(
                "[PPC-TRACE] fetch pc=${:08X} word=${:08X} lr=${:08X} sp=${:08X} rtoc=${:08X} r3=${:08X} r4=${:08X} r5=${:08X} r12=${:08X} r27=${:08X} r28=${:08X} r29=${:08X} r30=${:08X} r31=${:08X}",
                pc,
                word,
                cpu.lr,
                cpu.gpr[1],
                cpu.gpr[2],
                cpu.gpr[3],
                cpu.gpr[4],
                cpu.gpr[5],
                cpu.gpr[12],
                cpu.gpr[27],
                cpu.gpr[28],
                cpu.gpr[29],
                cpu.gpr[30],
                cpu.gpr[31],
            );
            if let Some(histogram) = self.histogram.as_deref_mut() {
                histogram.on_fetch(pc, word);
            }
            return;
        }
        if ppc_trace_regs_enabled() && in_range {
            if let Some(histogram) = self.histogram.as_deref_mut() {
                histogram.on_fetch(pc, word);
            }
            return;
        }
        self.on_fetch(pc, word);
    }
}

static PPC_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TRACE_PC_RANGE: OnceLock<Option<(u32, u32)>> = OnceLock::new();
static PPC_HLE_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_GWORLD_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_TRIMESH_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_COLLISION_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static QD3D_DUMP_FRAME_ENABLED: OnceLock<bool> = OnceLock::new();
static QT_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_SOUND_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TIMER_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_PT_IN_RECT_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_RECENT_IMPORTS_ON_HALT_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TRACE_REGS_ENABLED: OnceLock<bool> = OnceLock::new();
static PPC_TRACE_REGS_R27_FILTER: OnceLock<Option<u32>> = OnceLock::new();
static PPC_TRACE_REGS_R3_FILTER: OnceLock<Option<u32>> = OnceLock::new();

fn ppc_trace_enabled() -> bool {
    *PPC_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_TRACE").is_some())
}

fn ppc_trace_pc_range() -> Option<(u32, u32)> {
    *PPC_TRACE_PC_RANGE.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_PPC_TRACE_PC_RANGE").ok()?;
        let mut parts = value.split(':');
        let start = parts.next()?.trim();
        let end = parts.next()?.trim();
        let parse = |s: &str| u32::from_str_radix(s.trim_start_matches("0x"), 16).ok();
        Some((parse(start)?, parse(end)?))
    })
}

fn ppc_trace_regs_enabled() -> bool {
    *PPC_TRACE_REGS_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_TRACE_REGS").is_some())
}

fn ppc_trace_regs_r27_filter() -> Option<u32> {
    *PPC_TRACE_REGS_R27_FILTER.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_PPC_TRACE_REGS_R27").ok()?;
        let trimmed = value.trim();
        let hex = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .or_else(|| trimmed.strip_prefix('$'))
            .unwrap_or(trimmed);
        u32::from_str_radix(hex, 16).ok()
    })
}

fn ppc_trace_regs_r3_filter() -> Option<u32> {
    *PPC_TRACE_REGS_R3_FILTER.get_or_init(|| {
        let value = std::env::var("SYSTEMLESS_PPC_TRACE_REGS_R3").ok()?;
        let trimmed = value.trim();
        let hex = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .or_else(|| trimmed.strip_prefix('$'))
            .unwrap_or(trimmed);
        u32::from_str_radix(hex, 16).ok()
    })
}

pub(super) fn ppc_hle_trace_enabled() -> bool {
    *PPC_HLE_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_HLE_TRACE").is_some())
}

pub(super) fn ppc_gworld_trace_enabled() -> bool {
    *PPC_GWORLD_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_GWORLD_TRACE").is_some())
}

fn ppc_recent_imports_on_halt_enabled() -> bool {
    *PPC_RECENT_IMPORTS_ON_HALT_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_RECENT_IMPORTS_ON_HALT").is_some())
}

pub(super) fn ppc_res_type_text(res_type: u32) -> String {
    let bytes = res_type.to_be_bytes();
    if bytes
        .iter()
        .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
    {
        String::from_utf8_lossy(&bytes).into_owned()
    } else {
        format!("${res_type:08X}")
    }
}

fn qd3d_trace_enabled() -> bool {
    *QD3D_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_TRACE").is_some())
}

pub(crate) fn qd3d_trimesh_trace_enabled() -> bool {
    *QD3D_TRIMESH_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_TRIMESH_TRACE").is_some())
}

pub(crate) fn qd3d_collision_trace_enabled() -> bool {
    *QD3D_COLLISION_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_COLLISION_TRACE").is_some())
}

pub(crate) fn qd3d_dump_frame_enabled() -> bool {
    *QD3D_DUMP_FRAME_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_QD3D_DUMP_FRAME").is_some())
}

pub(super) fn qt_trace_enabled() -> bool {
    *QT_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_QT_TRACE").is_some())
}

pub(super) fn ppc_sound_trace_enabled() -> bool {
    *PPC_SOUND_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_SOUND").is_some())
}

fn ppc_timer_trace_enabled() -> bool {
    *PPC_TIMER_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_TRACE_TIMER").is_some())
}

fn ppc_pt_in_rect_trace_enabled() -> bool {
    *PPC_PT_IN_RECT_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("SYSTEMLESS_PPC_TRACE_PT_IN_RECT").is_some())
}

pub(super) fn format_ppc_fourcc(value: u32) -> String {
    value
        .to_be_bytes()
        .into_iter()
        .map(|byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                char::from(byte)
            } else {
                '.'
            }
        })
        .collect()
}

fn format_ppc_trace_fetch(pc: u32, word: u32) -> String {
    format!("[PPC-TRACE] fetch pc=${:08X} word=${:08X}", pc, word)
}

fn format_ppc_trace_import(entry: &PpcHleImportTraceEntry) -> String {
    format!(
        "[PPC-TRACE] import #{} {}:{} pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X} target={:?}",
        entry.import_index,
        entry.library_name,
        entry.symbol_name,
        entry.pc,
        entry.lr,
        entry.rtoc,
        entry.sp,
        entry.dispatcher_target
    )
}

fn format_ppc_trace_unknown_import(index: u32, pc: u32, lr: u32, rtoc: u32, sp: u32) -> String {
    format!(
        "[PPC-TRACE] import #{} <unknown> pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X}",
        index, pc, lr, rtoc, sp
    )
}

fn is_sprocket_import(binding: &PpcImportBinding) -> bool {
    matches!(
        binding.library_name.as_str(),
        "DrawSprocketLib" | "InputSprocketLib"
    )
}

fn format_hex_opt(value: Option<u32>) -> String {
    value
        .map(|value| format!("${:08X}", value))
        .unwrap_or_else(|| "none".to_string())
}

pub(crate) fn format_hle_import_action(action: &PpcImportAction) -> String {
    format_ppc_import_action(action)
}

fn ppc_import_action_with_extra_cycles(
    action: PpcImportAction,
    extra_cycles: u64,
) -> PpcImportAction {
    if extra_cycles == 0 {
        return action;
    }
    match action {
        PpcImportAction::Return(value) => {
            PpcImportAction::ReturnWithExtraCycles(value, extra_cycles)
        }
        PpcImportAction::ReturnPreserve => {
            PpcImportAction::ReturnPreserveWithExtraCycles(extra_cycles)
        }
        PpcImportAction::ReturnPreserveWithExtraCycles(existing) => {
            PpcImportAction::ReturnPreserveWithExtraCycles(existing.saturating_add(extra_cycles))
        }
        PpcImportAction::ReturnWithExtraCycles(value, existing) => {
            PpcImportAction::ReturnWithExtraCycles(value, existing.saturating_add(extra_cycles))
        }
        _ => action,
    }
}

fn ppc_import_extra_cycles_for_target(target: &PpcImportDispatcherTarget) -> u64 {
    match target {
        PpcImportDispatcherTarget::Q3ViewEndRendering => 0,
        PpcImportDispatcherTarget::MathCeil
        | PpcImportDispatcherTarget::MathSqrt
        | PpcImportDispatcherTarget::MathExp
        | PpcImportDispatcherTarget::MathSin
        | PpcImportDispatcherTarget::MathCos
        | PpcImportDispatcherTarget::MathAsin
        | PpcImportDispatcherTarget::MathTan
        | PpcImportDispatcherTarget::MathAtan
        | PpcImportDispatcherTarget::MathAtan2
        | PpcImportDispatcherTarget::MathPow
        | PpcImportDispatcherTarget::MathFmod
        | PpcImportDispatcherTarget::MathLog
        | PpcImportDispatcherTarget::MathLog10 => PPC_MATH_HOT_IMPORT_EXTRA_CYCLES,
        // Rasterizing text: each call renders glyphs into the framebuffer.
        PpcImportDispatcherTarget::DrawChar
        | PpcImportDispatcherTarget::DrawText
        | PpcImportDispatcherTarget::DrawString => PPC_DRAW_TEXT_IMPORT_EXTRA_CYCLES,
        // Text measurement walks every glyph of the supplied bytes.
        PpcImportDispatcherTarget::MeasureText
        | PpcImportDispatcherTarget::TextWidth
        | PpcImportDispatcherTarget::TruncString
        | PpcImportDispatcherTarget::StringWidth
        | PpcImportDispatcherTarget::CharWidth
        | PpcImportDispatcherTarget::GetFontInfo
        | PpcImportDispatcherTarget::FontMetrics => PPC_MEASURE_TEXT_IMPORT_EXTRA_CYCLES,
        // PICT opcode interpretation plus rasterization.
        PpcImportDispatcherTarget::DrawPicture => PPC_DRAW_PICTURE_IMPORT_EXTRA_CYCLES,
        // Rectangular bit transfers between ports and GWorlds.
        PpcImportDispatcherTarget::CopyBits => PPC_BIT_TRANSFER_IMPORT_EXTRA_CYCLES,
        // Rect, region, oval and rounded-rect painting operations.
        PpcImportDispatcherTarget::PaintArc
        | PpcImportDispatcherTarget::PaintRect
        | PpcImportDispatcherTarget::PaintRoundRect
        | PpcImportDispatcherTarget::FillCRect
        | PpcImportDispatcherTarget::FillRgn
        | PpcImportDispatcherTarget::EraseOval
        | PpcImportDispatcherTarget::EraseRect
        | PpcImportDispatcherTarget::InvertRect
        | PpcImportDispatcherTarget::InvertRgn
        | PpcImportDispatcherTarget::FrameRect
        | PpcImportDispatcherTarget::FrameRgn => PPC_DRAW_PRIMITIVE_IMPORT_EXTRA_CYCLES,
        // Resource Manager fetches parse and copy resource data.
        PpcImportDispatcherTarget::GetIndString
        | PpcImportDispatcherTarget::GetString
        | PpcImportDispatcherTarget::GetResource
        | PpcImportDispatcherTarget::Get1Resource
        | PpcImportDispatcherTarget::Get1NamedResource
        | PpcImportDispatcherTarget::Get1IndResource
        | PpcImportDispatcherTarget::LoadResource
        | PpcImportDispatcherTarget::ReadPartialResource => PPC_RESOURCE_IMPORT_EXTRA_CYCLES,
        _ => 0,
    }
}

fn ppc_import_extra_cycles_for_binding(binding: &PpcImportBinding) -> u64 {
    if is_quickdraw_3d_library(&binding.library_name)
        || is_quickdraw_3d_accelerator_library(&binding.library_name)
    {
        return PPC_Q3_HOT_IMPORT_EXTRA_CYCLES;
    }
    ppc_import_extra_cycles_for_target(&binding.dispatcher_target)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Qd3dTraceSnapshot {
    objects: usize,
    views: usize,
    submissions: usize,
    completed_frames: usize,
    memory_storages: usize,
    files: usize,
    group_memberships: usize,
    trimeshes: usize,
    draw_contexts: usize,
    textures: usize,
    shaders: usize,
    styles: usize,
    cameras: usize,
    lights: usize,
    latest_object: Option<PpcQ3ObjectRecord>,
    latest_submission: Option<PpcQ3SubmissionRecord>,
    latest_frame_view: Option<u32>,
    latest_frame_submissions: usize,
}

fn qd3d_trace_snapshot(
    q3_objects: &[PpcQ3ObjectRecord],
    q3_views: &[PpcQ3ViewStateRecord],
    q3_submissions: &[PpcQ3SubmissionRecord],
    q3_completed_frames: &[PpcQ3CompletedFrameRecord],
    q3_state_only_completed_frame_batches: &[PpcQ3StateOnlyCompletedFrameBatch],
    q3_memory_storages: &[PpcQ3MemoryStorageRecord],
    q3_files: &[PpcQ3FileRecord],
    q3_group_memberships: &[PpcQ3GroupMembershipRecord],
    q3_trimeshes: &[PpcQ3TriMeshRecord],
    q3_draw_contexts: &[PpcQ3DrawContextRecord],
    q3_mipmap_textures: &[PpcQ3MipmapTextureRecord],
    q3_texture_shaders: &[PpcQ3TextureShaderRecord],
    q3_styles: &[PpcQ3StyleRecord],
    q3_cameras: &[PpcQ3CameraRecord],
    q3_lights: &[PpcQ3LightRecord],
) -> Qd3dTraceSnapshot {
    let (latest_frame_view, latest_frame_submissions) = q3_completed_frames
        .last()
        .map(|frame| (Some(frame.view), frame.submissions.len()))
        .unwrap_or((None, 0));
    Qd3dTraceSnapshot {
        objects: q3_objects.len(),
        views: q3_views.len(),
        submissions: q3_submissions.len(),
        completed_frames: q3_completed_frames.len().saturating_add(
            q3_state_only_completed_frame_total(q3_state_only_completed_frame_batches),
        ),
        memory_storages: q3_memory_storages.len(),
        files: q3_files.len(),
        group_memberships: q3_group_memberships.len(),
        trimeshes: q3_trimeshes.len(),
        draw_contexts: q3_draw_contexts.len(),
        textures: q3_mipmap_textures.len(),
        shaders: q3_texture_shaders.len(),
        styles: q3_styles.len(),
        cameras: q3_cameras.len(),
        lights: q3_lights.len(),
        latest_object: q3_objects.last().copied(),
        latest_submission: q3_submissions.last().copied(),
        latest_frame_view,
        latest_frame_submissions,
    }
}

fn qd3d_trace_count(label: &str, before: usize, after: usize) -> String {
    let delta = after as isize - before as isize;
    format!("{}={}({:+})", label, after, delta)
}

fn q3_object_kind_name(kind: PpcQ3ObjectKind) -> &'static str {
    match kind {
        PpcQ3ObjectKind::Generic => "generic",
        PpcQ3ObjectKind::MemoryStorage => "memory-storage",
    }
}

fn q3_submission_kind_name(kind: PpcQ3SubmissionKind) -> &'static str {
    match kind {
        PpcQ3SubmissionKind::Shader => "shader",
        PpcQ3SubmissionKind::Style => "style",
        PpcQ3SubmissionKind::FogStyle => "fog-style",
        PpcQ3SubmissionKind::TriMesh => "trimesh",
        PpcQ3SubmissionKind::MatrixTransform => "matrix-transform",
        PpcQ3SubmissionKind::ResetTransform => "reset-transform",
        PpcQ3SubmissionKind::Push => "push",
        PpcQ3SubmissionKind::Pop => "pop",
        PpcQ3SubmissionKind::Object => "object",
    }
}

fn format_qd3d_latest_object(snapshot: &Qd3dTraceSnapshot) -> String {
    snapshot
        .latest_object
        .map(|object| {
            format!(
                "object={} kind={} type=${:08X} data=${:08X}/{}",
                format_hex_opt(Some(object.object)),
                q3_object_kind_name(object.kind),
                object.object_type,
                object.data_ptr,
                object.data_size
            )
        })
        .unwrap_or_else(|| "object=none".to_string())
}

fn format_qd3d_latest_submission(snapshot: &Qd3dTraceSnapshot) -> String {
    snapshot
        .latest_submission
        .map(|submission| {
            format!(
                "submission={} view={} primary={} secondary={}",
                q3_submission_kind_name(submission.kind),
                format_hex_opt(Some(submission.view)),
                format_hex_opt(Some(submission.primary)),
                format_hex_opt(Some(submission.secondary))
            )
        })
        .unwrap_or_else(|| "submission=none".to_string())
}

fn format_qd3d_latest_frame(snapshot: &Qd3dTraceSnapshot) -> String {
    snapshot
        .latest_frame_view
        .map(|view| {
            format!(
                "frame_view={} frame_submissions={}",
                format_hex_opt(Some(view)),
                snapshot.latest_frame_submissions
            )
        })
        .unwrap_or_else(|| "frame=none".to_string())
}

fn format_qd3d_trace(
    entry: &PpcHleImportTraceEntry,
    args: [u32; 6],
    action: &str,
    before: &Qd3dTraceSnapshot,
    after: &Qd3dTraceSnapshot,
) -> String {
    format!(
        "[QD3D-TRACE] {}:{} pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X} r3=${:08X} r4=${:08X} r5=${:08X} r6=${:08X} r7=${:08X} r8=${:08X} action={} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}",
        entry.library_name,
        entry.symbol_name,
        entry.pc,
        entry.lr,
        entry.rtoc,
        entry.sp,
        args[0],
        args[1],
        args[2],
        args[3],
        args[4],
        args[5],
        action,
        qd3d_trace_count("objects", before.objects, after.objects),
        qd3d_trace_count("views", before.views, after.views),
        qd3d_trace_count("submissions", before.submissions, after.submissions),
        qd3d_trace_count("frames", before.completed_frames, after.completed_frames),
        qd3d_trace_count("storages", before.memory_storages, after.memory_storages),
        qd3d_trace_count("files", before.files, after.files),
        qd3d_trace_count("groups", before.group_memberships, after.group_memberships),
        qd3d_trace_count("trimeshes", before.trimeshes, after.trimeshes),
        qd3d_trace_count("draw_contexts", before.draw_contexts, after.draw_contexts),
        qd3d_trace_count("textures", before.textures, after.textures),
        qd3d_trace_count("shaders", before.shaders, after.shaders),
        qd3d_trace_count("styles", before.styles, after.styles),
        qd3d_trace_count("cameras", before.cameras, after.cameras),
        qd3d_trace_count("lights", before.lights, after.lights),
        format_qd3d_latest_object(after),
        format_qd3d_latest_submission(after),
        format_qd3d_latest_frame(after)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PpcWatchRange {
    start: u32,
    len: u32,
}

impl PpcWatchRange {
    fn contains(self, addr: u32) -> bool {
        let start = u64::from(self.start);
        let end = start + u64::from(self.len);
        let addr = u64::from(addr);
        addr >= start && addr < end
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PpcWatchWriteRecord {
    pc: u32,
    lr: u32,
    rtoc: u32,
    sp: u32,
    addr: u32,
    value: u8,
}

struct PpcWatchObserver {
    range: PpcWatchRange,
}

impl PpcMemoryWriteObserver for PpcWatchObserver {
    fn on_write(&mut self, pc: u32, lr: u32, rtoc: u32, sp: u32, addr: u32, value: u8) {
        if self.range.contains(addr) {
            eprintln!(
                "{}",
                format_ppc_watch_write(PpcWatchWriteRecord {
                    pc,
                    lr,
                    rtoc,
                    sp,
                    addr,
                    value,
                })
            );
        }
    }
}

static PPC_WATCH_RANGE: OnceLock<Option<PpcWatchRange>> = OnceLock::new();

fn ppc_watch_range() -> Option<PpcWatchRange> {
    *PPC_WATCH_RANGE.get_or_init(|| {
        let value = std::env::var_os("SYSTEMLESS_PPC_WATCH")?;
        parse_ppc_watch_range_value(&value)
    })
}

fn parse_ppc_watch_range_value(value: &std::ffi::OsStr) -> Option<PpcWatchRange> {
    let value = value.to_str()?.trim();
    if value.is_empty() {
        return None;
    }
    let mut parts = value.split(':');
    let start = parse_ppc_watch_addr(parts.next()?)?;
    let len = match parts.next() {
        Some(raw_len) => parse_ppc_watch_len(raw_len)?,
        None => 1,
    };
    if parts.next().is_some() {
        return None;
    }
    if len == 0 || u64::from(start) + u64::from(len) > (u64::from(u32::MAX) + 1) {
        return None;
    }
    Some(PpcWatchRange { start, len })
}

fn parse_ppc_watch_addr(value: &str) -> Option<u32> {
    let value = value
        .trim()
        .trim_start_matches('$')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if value.is_empty() {
        return None;
    }
    u32::from_str_radix(value, 16).ok()
}

fn parse_ppc_watch_len(value: &str) -> Option<u32> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let len = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map_or_else(
            || value.parse().ok(),
            |hex| u32::from_str_radix(hex, 16).ok(),
        )?;
    (len != 0).then_some(len)
}

fn format_ppc_watch_write(record: PpcWatchWriteRecord) -> String {
    format!(
        "[PPC-WATCH] pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X} addr=${:08X} value=${:02X}",
        record.pc, record.lr, record.rtoc, record.sp, record.addr, record.value
    )
}

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

struct PpcDispatchContext<'a> {
    binding: &'a PpcImportBinding,
    cpu: &'a mut PpcCpu,
    memory: &'a mut PpcSectionMem,
    process_memory_manager: &'a mut ProcessNativeMemoryManager,
    heap_cursor: &'a mut u32,
    heap_limit: u32,
    native_heap_ceiling: u32,
    last_mem_error: &'a mut i16,
    tick_count: &'a mut u32,
    cycles_per_tick: u32,
    current_resource_refnum: &'a mut i16,
    last_resource_error: &'a mut i16,
    resource_policy: &'a SharedProcessResourcePolicy,
    native_exception_handler: &'a Cell<u32>,
    stdc_qsort_stack: &'a mut Vec<PpcQsortState>,
    dialog_callback_stack: &'a mut Vec<PpcDialogCallbackState>,
    collection_callback_stack: &'a mut Vec<PpcCollectionCallbackState>,
    apple_events: &'a mut PpcAppleEventState,
    cfm_connections: &'a mut Vec<PpcCfmConnection>,
    cfm_library_fragments: &'a mut Vec<PpcCfmLibraryFragment>,
    next_cfm_connection_id: &'a mut u32,
    import_run_state: &'a mut PpcImportRunState,
    controls: &'a mut Vec<PpcControlRecord>,
    aliases: &'a mut Vec<PpcAliasRecord>,
    gworlds: &'a mut Vec<PpcGWorldRecord>,
    gworld_pixel_states: &'a SharedProcessQuickDrawPixelStates,
    window_list: &'a SharedProcessWindowList,
    q3_objects: &'a mut Vec<PpcQ3ObjectRecord>,
    q3_object_refs: &'a mut Vec<PpcQ3ObjectReferenceRecord>,
    next_q3_object: &'a mut u32,
    q3_error_state: &'a mut PpcQ3ErrorState,
    q3_lifecycle: &'a mut PpcQ3LifecycleState,
    q3_memory_storages: &'a mut Vec<PpcQ3MemoryStorageRecord>,
    q3_files: &'a mut Vec<PpcQ3FileRecord>,
    q3_group_memberships: &'a mut Vec<PpcQ3GroupMembershipRecord>,
    q3_file_groups: &'a mut Vec<PpcQ3FileGroupRecord>,
    q3_views: &'a mut Vec<PpcQ3ViewStateRecord>,
    q3_submissions: &'a mut Vec<PpcQ3SubmissionRecord>,
    q3_view_transforms: &'a mut Vec<PpcQ3ViewTransformRecord>,
    q3_submission_transforms: &'a mut Vec<PpcQ3SubmissionTransformRecord>,
    q3_view_materials: &'a mut Vec<PpcQ3ViewMaterialRecord>,
    q3_submission_materials: &'a mut Vec<PpcQ3SubmissionMaterialRecord>,
    q3_submission_lights: &'a mut Vec<PpcQ3SubmissionLightRecord>,
    q3_view_state_stack: &'a mut Vec<PpcQ3ViewStateSnapshotRecord>,
    q3_completed_frames: &'a mut Vec<PpcQ3CompletedFrameRecord>,
    q3_retained_frames: &'a mut Vec<PpcQ3RetainedFrameRecord>,
    q3_state_only_completed_frame_batches: &'a mut Vec<PpcQ3StateOnlyCompletedFrameBatch>,
    q3_fog_styles: &'a mut Vec<PpcQ3FogStyleRecord>,
    q3_attributes: &'a mut Vec<PpcQ3AttributeRecord>,
    q3_shader_uv_transforms: &'a mut Vec<PpcQ3ShaderUvTransformRecord>,
    q3_shader_boundaries: &'a mut Vec<PpcQ3ShaderBoundaryRecord>,
    q3_mipmap_textures: &'a mut Vec<PpcQ3MipmapTextureRecord>,
    q3_texture_shaders: &'a mut Vec<PpcQ3TextureShaderRecord>,
    q3_renderer_preferences: &'a mut Vec<PpcQ3RendererPreferenceRecord>,
    q3_draw_contexts: &'a mut Vec<PpcQ3DrawContextRecord>,
    q3_trimeshes: &'a mut Vec<PpcQ3TriMeshRecord>,
    q3_styles: &'a mut Vec<PpcQ3StyleRecord>,
    q3_cameras: &'a mut Vec<PpcQ3CameraRecord>,
    q3_lights: &'a mut Vec<PpcQ3LightRecord>,
    input_sprocket: &'a mut PpcInputSprocketState,
    input_sprocket_virtual_elements: &'a mut Vec<PpcInputSprocketVirtualElementRecord>,
    toolbox_startup: &'a mut PpcToolboxStartupState,
    quicktime: &'a mut PpcQuickTimeState,
    sound: &'a mut PpcSoundState,
    timer_tasks: &'a SharedProcessTimerTasks,
    vbl_tasks: &'a SharedProcessVblTasks,
    callback_scheduling: &'a SharedProcessCallbackScheduling,
    files: &'a mut Vec<PpcFileRecord>,
    writable_refnums: &'a mut HashSet<u16>,
    vfs_files: &'a mut ProcessVfsFileRecords,
    stdio_streams: &'a mut HashMap<u32, PpcStdioStreamRecord>,
    deleted_vfs_file_paths: &'a mut Vec<String>,
    resource_files: &'a mut Vec<PpcResourceFileRecord>,
    vfs_resource_files: &'a mut ProcessVfsResourceFileRecords,
    vfs_resources: &'a mut Vec<PpcVfsResourceRecord>,
    next_file_ref_num: &'a mut i16,
    current_gworld: &'a mut u32,
    current_gdevice: &'a mut u32,
    quickdraw_op_colors: &'a SharedProcessQuickDrawOpColors,
    quickdraw_hilite_colors: &'a SharedProcessQuickDrawHiliteColors,
    screen_clut: &'a mut [[u16; 3]; 256],
    color_manager_clut: &'a mut [[u16; 3]; 256],
    display_gamma: &'a SharedProcessDisplayGamma,
    quickdraw_fore_color: &'a mut PpcRgbColor,
    quickdraw_fore_indices: &'a mut HashMap<u32, u8>,
    quickdraw_back_color: &'a mut PpcRgbColor,
    quickdraw_pen_h: &'a mut i16,
    quickdraw_pen_v: &'a mut i16,
    quickdraw_text_mode: &'a mut i16,
    quickdraw_text_size: &'a mut i16,
    cursor_state: &'a SharedProcessCursorState,
    vfs_volumes: &'a [PpcVfsVolumeRecord],
    vfs_directories: &'a mut Vec<PpcVfsDirectory>,
    next_vfs_dir_id: &'a mut u32,
    default_dir_id: u32,
    working_directories: &'a mut HashMap<i16, ProcessWorkingDirectory>,
    next_working_directory_ref_num: &'a mut i16,
    application_working_directory_ref_num: &'a mut i16,
    launched_app_path: Option<&'a str>,
    param_text: &'a SharedProcessDialogText,
    scrap: &'a mut PpcScrapState,
    list_manager: &'a mut ProcessListManagerState,
    collections: &'a SharedProcessCollectionManager,
    input: PpcInputSnapshot,
    event_queue: &'a mut EventQueue,
    draw_sprocket: &'a mut PpcDrawSprocketState,
}

fn dispatch_supported_import(context: PpcDispatchContext<'_>) -> Option<PpcImportAction> {
    let PpcDispatchContext {
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        heap_limit,
        native_heap_ceiling,
        last_mem_error,
        tick_count,
        cycles_per_tick,
        current_resource_refnum,
        last_resource_error,
        resource_policy,
        native_exception_handler,
        stdc_qsort_stack,
        dialog_callback_stack,
        collection_callback_stack,
        apple_events,
        cfm_connections,
        cfm_library_fragments,
        next_cfm_connection_id,
        import_run_state,
        controls,
        aliases,
        gworlds,
        gworld_pixel_states,
        window_list,
        q3_objects,
        q3_object_refs,
        next_q3_object,
        q3_error_state,
        q3_lifecycle,
        q3_memory_storages,
        q3_files,
        q3_group_memberships,
        q3_file_groups,
        q3_views,
        q3_submissions,
        q3_view_transforms,
        q3_submission_transforms,
        q3_view_materials,
        q3_submission_materials,
        q3_submission_lights,
        q3_view_state_stack,
        q3_completed_frames,
        q3_retained_frames,
        q3_state_only_completed_frame_batches,
        q3_fog_styles,
        q3_attributes,
        q3_shader_uv_transforms,
        q3_shader_boundaries,
        q3_mipmap_textures,
        q3_texture_shaders,
        q3_renderer_preferences,
        q3_draw_contexts,
        q3_trimeshes,
        q3_styles,
        q3_cameras,
        q3_lights,
        input_sprocket,
        input_sprocket_virtual_elements,
        toolbox_startup,
        quicktime,
        sound,
        timer_tasks,
        vbl_tasks,
        callback_scheduling,
        files,
        writable_refnums,
        vfs_files,
        stdio_streams,
        deleted_vfs_file_paths,
        resource_files,
        vfs_resource_files,
        vfs_resources,
        next_file_ref_num,
        current_gworld,
        current_gdevice,
        quickdraw_op_colors,
        quickdraw_hilite_colors,
        screen_clut,
        color_manager_clut,
        display_gamma,
        quickdraw_fore_color,
        quickdraw_fore_indices,
        quickdraw_back_color,
        quickdraw_pen_h,
        quickdraw_pen_v,
        quickdraw_text_mode,
        quickdraw_text_size,
        cursor_state,
        vfs_volumes,
        vfs_directories,
        next_vfs_dir_id,
        default_dir_id,
        working_directories,
        next_working_directory_ref_num,
        application_working_directory_ref_num,
        launched_app_path,
        param_text,
        scrap,
        list_manager,
        collections,
        input,
        event_queue,
        draw_sprocket,
    } = context;
    let _menu_root = (matches!(
        binding.dispatcher_target,
        PpcImportDispatcherTarget::MenuSelect | PpcImportDispatcherTarget::PopUpMenuSelect
    ))
    .then(|| {
        let call = match binding.dispatcher_target {
            PpcImportDispatcherTarget::PopUpMenuSelect => ppc_popup_menu_call(cpu),
            _ => ppc_menu_select_call(cpu, cpu.gpr[3]),
        };
        toolbox_startup.execution.enter_menu_call(call)
    });
    // The process registry is authoritative. Refresh the legacy vector
    // booleans at each native boundary so rendering/debugging code that still
    // reads them sees any classic-side transition before this import runs.
    ppc_sync_gworld_pixel_state_mirrors(gworlds, gworld_pixel_states);

    // Toolbox helpers use an import-scoped read view for guest ABI decoding.
    // Allocation ownership remains in the process Memory Manager, and the
    // next import observes its canonical records immediately.
    let handles = &mut process_memory_manager.native_handle_records().to_vec();
    if is_quickdraw_3d_library(&binding.library_name)
        && !matches!(
            binding.dispatcher_target,
            PpcImportDispatcherTarget::Q3ErrorGet
        )
        && q3_error_state.clear_on_next_q3_call
    {
        q3_error_state.clear();
    }

    if let Some(action) =
        dispatch_qd3d::dispatch_q3_core_import(dispatch_qd3d::PpcQ3CoreDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            q3_objects,
            next_q3_object,
            q3_error_state,
            q3_lifecycle,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_qd3d::dispatch_q3_storage_file_import(
        dispatch_qd3d::PpcQ3StorageFileDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            process_memory_manager,
            memory,
            stores: PpcQ3ObjectStores {
                q3_objects,
                q3_object_refs,
                q3_renderer_preferences,
                q3_files,
                q3_group_memberships,
                q3_file_groups,
                q3_views,
                q3_submissions,
                q3_view_transforms,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_submission_lights,
                q3_view_state_stack,
                q3_completed_frames,
                q3_retained_frames,
                q3_fog_styles,
                q3_memory_storages,
                q3_attributes,
                q3_shader_uv_transforms,
                q3_shader_boundaries,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_draw_contexts,
                q3_trimeshes,
                q3_styles,
                q3_cameras,
                q3_lights,
            },
            next_q3_object,
            q3_error_state,
            heap_cursor,
            heap_limit,
            last_mem_error,
            vfs_directories,
            vfs_files,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_qd3d::dispatch_q3_geometry_import(dispatch_qd3d::PpcQ3GeometryDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            process_memory_manager,
            memory,
            stores: PpcQ3ObjectStores {
                q3_objects,
                q3_object_refs,
                q3_renderer_preferences,
                q3_files,
                q3_group_memberships,
                q3_file_groups,
                q3_views,
                q3_submissions,
                q3_view_transforms,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_submission_lights,
                q3_view_state_stack,
                q3_completed_frames,
                q3_retained_frames,
                q3_fog_styles,
                q3_memory_storages,
                q3_attributes,
                q3_shader_uv_transforms,
                q3_shader_boundaries,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_draw_contexts,
                q3_trimeshes,
                q3_styles,
                q3_cameras,
                q3_lights,
            },
            next_q3_object,
            q3_error_state,
            heap_cursor,
            last_mem_error,
            gworlds,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_qd3d::dispatch_q3_group_view_import(dispatch_qd3d::PpcQ3GroupViewDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            stores: PpcQ3ObjectStores {
                q3_objects,
                q3_object_refs,
                q3_renderer_preferences,
                q3_files,
                q3_group_memberships,
                q3_file_groups,
                q3_views,
                q3_submissions,
                q3_view_transforms,
                q3_submission_transforms,
                q3_view_materials,
                q3_submission_materials,
                q3_submission_lights,
                q3_view_state_stack,
                q3_completed_frames,
                q3_retained_frames,
                q3_fog_styles,
                q3_memory_storages,
                q3_attributes,
                q3_shader_uv_transforms,
                q3_shader_boundaries,
                q3_mipmap_textures,
                q3_texture_shaders,
                q3_draw_contexts,
                q3_trimeshes,
                q3_styles,
                q3_cameras,
                q3_lights,
            },
            next_q3_object,
            q3_state_only_completed_frame_batches,
            gworlds,
            current_gworld: *current_gworld,
            q3_error_state,
            input_idle: input.is_idle(),
        })
    {
        return Some(action);
    }

    let mut current_menu_list = ppc_current_menu_list(memory);
    if let Some(action) = dispatch_cfm::dispatch_cfm_import(dispatch_cfm::PpcCfmDispatchContext {
        binding,
        cpu,
        guest_calls: &toolbox_startup.execution.calls(),
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        cfm_connections,
        cfm_library_fragments,
        vfs_files,
        vfs_resource_files,
        vfs_directories,
        next_cfm_connection_id,
        import_run_state,
    }) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_files::dispatch_file_import(dispatch_files::PpcFileDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            handles,
            aliases,
            files,
            writable_refnums,
            vfs_files,
            vfs_directories,
            next_vfs_dir_id,
            deleted_vfs_file_paths,
            vfs_resource_files,
            resource_files,
            vfs_resources,
            next_file_ref_num,
            current_resource_refnum,
            last_resource_error,
            default_dir_id,
            launched_app_path,
            vfs_volumes,
            working_directories,
            next_working_directory_ref_num,
            application_working_directory_ref_num,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_resources::dispatch_resource_import(
        dispatch_resources::PpcResourceDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            resource_files,
            vfs_resource_files,
            vfs_resources,
            current_resource_refnum,
            resource_policy,
            last_resource_error,
        },
    ) {
        return Some(action);
    }

    if let Some(action) = dispatch_icon_services::dispatch_icon_services_import(
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        &mut toolbox_startup.icon_refs,
        vfs_directories,
        vfs_files,
        vfs_resource_files,
        vfs_resources,
        gworlds,
        *current_gworld,
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_stdc::dispatch_stdc_import(dispatch_stdc::PpcStdCDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            stdc_qsort_stack,
            stdc_signal_state: &mut toolbox_startup.stdc_signal_state,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_core_foundation::dispatch_core_foundation_import(
        binding,
        cpu,
        memory,
        process_memory_manager,
        heap_cursor,
        last_mem_error,
        toolbox_startup,
    ) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_regions::dispatch_region_import(dispatch_regions::PpcRegionDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            current_gworld: *current_gworld,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_polygons::dispatch_polygon_import(dispatch_polygons::PpcPolygonDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            current_gworld: *current_gworld,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_bit_transfers::dispatch_bit_transfer_import(
        dispatch_bit_transfers::PpcBitTransferDispatchContext {
            binding,
            cpu,
            memory,
            gworlds,
            window_list,
            current_gworld: *current_gworld,
            current_gdevice: *current_gdevice,
            quickdraw_op_colors,
            color_manager_clut,
            quickdraw_fore_color: *quickdraw_fore_color,
            quickdraw_fore_index: quickdraw_fore_indices.get(current_gworld).copied(),
            quickdraw_back_color: *quickdraw_back_color,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }
    if let Some(action) = dispatch_graphics_devices::dispatch_graphics_device_import(
        dispatch_graphics_devices::PpcGraphicsDeviceDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            current_gdevice,
            toolbox_startup,
            screen_clut,
            color_manager_clut,
        },
    ) {
        return Some(action);
    }
    if let Some(action) = dispatch_color_tables::dispatch_color_table_import(
        dispatch_color_tables::PpcColorTableDispatchContext {
            binding,
            cpu,
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum,
            current_gworld,
            quickdraw_hilite_colors,
            tick_count,
            current_gdevice,
            screen_clut,
            color_manager_clut,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_palettes::dispatch_palette_import(dispatch_palettes::PpcPaletteDispatchContext {
            binding,
            cpu,
            memory,
            gworlds,
            window_list,
            current_gdevice: *current_gdevice,
            screen_clut,
            color_manager_clut,
            toolbox_startup,
            event_queue,
            tick_count: *tick_count,
            input,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_gworlds::dispatch_gworld_import(dispatch_gworlds::PpcGWorldDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            gworld_pixel_states,
            current_gworld,
            current_gdevice,
            quickdraw_op_colors,
            quickdraw_hilite_colors,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_back_color,
            quickdraw_pen_h,
            quickdraw_pen_v,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_quickdraw::dispatch_quickdraw_import(
        dispatch_quickdraw::PpcQuickDrawDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            handles,
            gworlds,
            tick_count: *tick_count,
            current_gworld: *current_gworld,
            current_gdevice: *current_gdevice,
            quickdraw_op_colors,
            quickdraw_hilite_colors,
            screen_clut,
            color_manager_clut,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_back_color,
            quickdraw_pen_h,
            quickdraw_pen_v,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_picture::dispatch_picture_import(dispatch_picture::PpcPictureDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            gworlds,
            current_gworld: *current_gworld,
            screen_clut,
            color_manager_clut,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_fonts::dispatch_font_import(dispatch_fonts::PpcFontDispatchContext {
            binding,
            cpu,
            memory,
            toolbox_startup,
            gworlds,
            current_gworld: *current_gworld,
            quickdraw_text_mode,
            quickdraw_text_size,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_pen_h,
            quickdraw_pen_v,
            vfs_resources,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_sound::dispatch_sound_import(dispatch_sound::PpcSoundDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            current_resource_refnum: *current_resource_refnum,
            handles,
            files,
            vfs_files,
            vfs_resources,
            sound,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_event::dispatch_time_import(dispatch_event::PpcTimeDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            tick_count: *tick_count,
            cycles_per_tick,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_event::dispatch_event_import(dispatch_event::PpcEventDispatchContext {
            binding,
            cpu,
            memory,
            handles,
            gworlds,
            current_gworld: *current_gworld,
            current_menu_list,
            screen_clut,
            toolbox_startup,
            apple_events,
            event_queue,
            input,
            tick_count: *tick_count,
        })
    {
        return Some(action);
    }
    if let Some(action) = dispatch_low_memory::dispatch_low_memory_import(
        dispatch_low_memory::PpcLowMemoryDispatchContext {
            target: &binding.dispatcher_target,
            cpu,
            memory,
            current_menu_list,
            default_dir_id,
        },
    ) {
        return Some(action);
    }
    if let Some(action) =
        dispatch_math::dispatch_math_import(&binding.dispatcher_target, cpu, memory)
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_toolbox::dispatch_toolbox_import(dispatch_toolbox::PpcToolboxDispatchContext {
            binding,
            cpu,
            memory,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_memory::dispatch_memory_import(dispatch_memory::PpcMemoryDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            native_heap_ceiling,
            last_mem_error,
            handles,
            aliases,
            vfs_resources,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_menu::dispatch_menu_import(dispatch_menu::PpcMenuDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            last_resource_error,
            handles,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            resource_policy,
            toolbox_startup,
            current_menu_list: &mut current_menu_list,
            gworlds,
            screen_clut,
            current_gworld,
            current_gdevice,
            event_queue,
            input,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_threads::dispatch_thread_import(dispatch_threads::PpcThreadDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            toolbox_startup,
        })
    {
        return Some(action);
    }
    if let Some(action) =
        dispatch_textedit::dispatch_textedit_import(dispatch_textedit::PpcTextEditDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            toolbox_startup,
            current_gworld: *current_gworld,
            tick_count: *tick_count,
            quickdraw_text_mode: *quickdraw_text_mode,
            quickdraw_text_size: *quickdraw_text_size,
            quickdraw_fore_color,
            quickdraw_back_color,
            quickdraw_fore_indices,
            scrap,
            gworlds,
            input,
            event_queue,
        })
    {
        return Some(action);
    }
    let screen_bits = ppc_screen_bits_addr(toolbox_startup.init_graf_global_ptr)
        .filter(|ptr| *ptr != 0 && ppc_memory_can_write_bytes(memory, *ptr, 14));
    if let Some(action) = dispatch_drawsprocket::dispatch_drawsprocket_import(
        dispatch_drawsprocket::PpcDrawSprocketDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            gworld_allocations: &mut toolbox_startup.gworld_allocations,
            current_gdevice: *current_gdevice,
            draw_sprocket,
            input,
            screen_clut,
            screen_bits,
        },
    ) {
        return Some(action);
    }
    if let Some(action) = dispatch_inputsprocket::dispatch_inputsprocket_import(
        dispatch_inputsprocket::PpcInputSprocketDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            input_sprocket,
            input_sprocket_virtual_elements,
            input,
            tick_count: *tick_count,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
        },
    ) {
        return Some(action);
    }
    if let Some(action) = dispatch_quicktime::dispatch_quicktime_import(
        dispatch_quicktime::PpcQuickTimeDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            last_mem_error,
            handles,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            vfs_resources,
            gworlds,
            current_gworld: *current_gworld,
            quicktime,
            sound,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_time::dispatch_time_import(dispatch_time::PpcTimeDispatchContext {
            binding,
            cpu,
            memory,
            timer_tasks,
            vbl_tasks,
            callback_scheduling,
            tick_count: *tick_count,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_standard_file::dispatch_standard_file_import(
        dispatch_standard_file::PpcStandardFileDispatchContext {
            binding,
            cpu,
            memory,
            startup: toolbox_startup,
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
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_control::dispatch_control_import(dispatch_control::PpcControlDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            gworlds,
            screen_clut,
            current_gworld: *current_gworld,
            toolbox_startup,
            input,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_list::dispatch_list_import(dispatch_list::PpcListDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            list_manager,
            gworlds,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            tick_count: *tick_count,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_collection::dispatch_collection_import(
        dispatch_collection::PpcCollectionDispatchContext {
            binding,
            cpu,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            collections,
            callback_stack: collection_callback_stack,
        },
    ) {
        return Some(action);
    }

    if let Some(action) = dispatch_appearance::dispatch_appearance_import(
        binding,
        cpu,
        memory,
        handles,
        controls,
        gworlds,
        vfs_resources,
        *current_resource_refnum,
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_dialog::dispatch_dialog_import(dispatch_dialog::PpcDialogDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            gworlds,
            screen_clut,
            color_manager_clut,
            current_gworld,
            current_gdevice,
            window_list,
            toolbox_startup,
            event_queue,
            dialog_callback_stack,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
            param_text,
            tick_count: *tick_count,
            input,
            quickdraw_text_mode: *quickdraw_text_mode,
            quickdraw_text_size: *quickdraw_text_size,
            quickdraw_fore_color,
            quickdraw_back_color,
            quickdraw_fore_indices,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_window::dispatch_window_import(dispatch_window::PpcWindowDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            controls,
            gworlds,
            window_list,
            draw_sprocket,
            current_gworld,
            current_gdevice,
            quickdraw_fore_color,
            quickdraw_fore_indices,
            quickdraw_back_color,
            screen_clut,
            color_manager_clut,
            toolbox_startup,
            input,
            tick_count: *tick_count,
            event_queue,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_display::dispatch_display_import(dispatch_display::PpcDisplayDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            gworlds,
            toolbox_startup,
            screen_clut,
            color_manager_clut,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_devices::dispatch_device_import(dispatch_devices::PpcDeviceDispatchContext {
            binding,
            cpu,
            memory,
            current_gdevice: *current_gdevice,
            screen_clut,
            display_gamma,
            toolbox_startup,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_desk::dispatch_desk_import(dispatch_desk::PpcDeskDispatchContext { binding })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_gestalt::dispatch_gestalt_import(dispatch_gestalt::PpcGestaltDispatchContext {
            binding,
            cpu,
            memory,
            toolbox_startup,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_cursor::dispatch_cursor_import(dispatch_cursor::PpcCursorDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            vfs_resources,
            current_resource_refnum: *current_resource_refnum,
            last_resource_error,
            cursor_state,
            gworlds,
            current_gworld: *current_gworld,
            screen_clut,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_apple_events::dispatch_apple_event_import(
        dispatch_apple_events::PpcAppleEventDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            apple_events,
            toolbox_startup,
        },
    ) {
        return Some(action);
    }

    if let Some(action) =
        dispatch_process::dispatch_process_import(dispatch_process::PpcProcessDispatchContext {
            binding,
            cpu,
            memory,
            vfs_directories,
            vfs_files,
            vfs_resource_files,
            launched_app_path,
        })
    {
        return Some(action);
    }

    if let Some(action) =
        dispatch_scrap::dispatch_scrap_import(dispatch_scrap::PpcScrapDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            scrap,
        })
    {
        return Some(action);
    }

    if let Some(action) = dispatch_mixed_mode::dispatch_mixed_mode_import(
        dispatch_mixed_mode::PpcMixedModeDispatchContext {
            binding,
            cpu,
            memory,
            process_memory_manager,
            heap_cursor,
            heap_limit,
            last_mem_error,
            toolbox_startup,
            cfm_connections,
            next_cfm_connection_id,
            import_run_state,
        },
    ) {
        return action;
    }

    if let Some(action) = dispatch_native_exceptions::dispatch_native_exception_import(
        dispatch_native_exceptions::PpcNativeExceptionDispatchContext {
            binding,
            cpu,
            native_exception_handler,
        },
    ) {
        return Some(action);
    }

    match binding.dispatcher_target {
        PpcImportDispatcherTarget::Collection(_) => {
            unreachable!("collection imports return through dispatch_collection_import")
        }
        PpcImportDispatcherTarget::InstallExceptionHandler => {
            unreachable!("native exception imports return through dispatch_native_exception_import")
        }
        PpcImportDispatcherTarget::RegisterAppearanceClient
        | PpcImportDispatcherTarget::ActivateControl
        | PpcImportDispatcherTarget::DeactivateControl
        | PpcImportDispatcherTarget::IsControlActive
        | PpcImportDispatcherTarget::CollapseWindow
        | PpcImportDispatcherTarget::IsWindowCollapsed
        | PpcImportDispatcherTarget::UnregisterAppearanceClient
        | PpcImportDispatcherTarget::SetControlFontStyle => {
            unreachable!("appearance imports return through dispatch_appearance_import")
        }
        PpcImportDispatcherTarget::NewPtr { .. }
        | PpcImportDispatcherTarget::DisposePtr
        | PpcImportDispatcherTarget::GetPtrSize
        | PpcImportDispatcherTarget::SetPtrSize
        | PpcImportDispatcherTarget::RecoverHandle
        | PpcImportDispatcherTarget::BlockMove
        | PpcImportDispatcherTarget::BlockZero
        | PpcImportDispatcherTarget::PtrToHand
        | PpcImportDispatcherTarget::PtrToXHand
        | PpcImportDispatcherTarget::HandToHand
        | PpcImportDispatcherTarget::HandAndHand
        | PpcImportDispatcherTarget::NewHandle { .. }
        | PpcImportDispatcherTarget::TempNewHandle
        | PpcImportDispatcherTarget::TempDisposeHandle
        | PpcImportDispatcherTarget::HoldMemory
        | PpcImportDispatcherTarget::UnholdMemory
        | PpcImportDispatcherTarget::DisposeHandle
        | PpcImportDispatcherTarget::EmptyHandle
        | PpcImportDispatcherTarget::GetHandleSize
        | PpcImportDispatcherTarget::SetHandleSize
        | PpcImportDispatcherTarget::HLock
        | PpcImportDispatcherTarget::HLockHi
        | PpcImportDispatcherTarget::HGetState
        | PpcImportDispatcherTarget::HSetState
        | PpcImportDispatcherTarget::HUnlock
        | PpcImportDispatcherTarget::MoveHHi
        | PpcImportDispatcherTarget::HNoPurge
        | PpcImportDispatcherTarget::HPurge
        | PpcImportDispatcherTarget::GetZone
        | PpcImportDispatcherTarget::SetZone
        | PpcImportDispatcherTarget::InitZone
        | PpcImportDispatcherTarget::SystemZone
        | PpcImportDispatcherTarget::ApplicationZone
        | PpcImportDispatcherTarget::MaxApplZone
        | PpcImportDispatcherTarget::MoreMasters
        | PpcImportDispatcherTarget::FlushCodeCache
        | PpcImportDispatcherTarget::GetApplLimit
        | PpcImportDispatcherTarget::SetApplLimit
        | PpcImportDispatcherTarget::HeapFreeBytes
        | PpcImportDispatcherTarget::MaxMem
        | PpcImportDispatcherTarget::PurgeMem
        | PpcImportDispatcherTarget::PurgeMemSys
        | PpcImportDispatcherTarget::MemError => {
            unreachable!("memory imports return through dispatch_memory_import")
        }
        PpcImportDispatcherTarget::InitMenus
        | PpcImportDispatcherTarget::NewMenu
        | PpcImportDispatcherTarget::DisposeMenu
        | PpcImportDispatcherTarget::GetMenu
        | PpcImportDispatcherTarget::GetItemCmd
        | PpcImportDispatcherTarget::SetItemCmd
        | PpcImportDispatcherTarget::GetItemMark
        | PpcImportDispatcherTarget::CountMItems
        | PpcImportDispatcherTarget::GetMenuItemText
        | PpcImportDispatcherTarget::SetMenuItemText
        | PpcImportDispatcherTarget::DeleteMenuItem
        | PpcImportDispatcherTarget::CalcMenuSize
        | PpcImportDispatcherTarget::PopUpMenuSelect
        | PpcImportDispatcherTarget::InsertMenu
        | PpcImportDispatcherTarget::DeleteMenu
        | PpcImportDispatcherTarget::AppendMenu
        | PpcImportDispatcherTarget::InsertMenuItem
        | PpcImportDispatcherTarget::AppendResMenu
        | PpcImportDispatcherTarget::InsertResMenu
        | PpcImportDispatcherTarget::EnableMenuItem
        | PpcImportDispatcherTarget::DisableMenuItem
        | PpcImportDispatcherTarget::SetItemMark
        | PpcImportDispatcherTarget::CheckItem
        | PpcImportDispatcherTarget::GetMenuBar
        | PpcImportDispatcherTarget::GetNewMBar
        | PpcImportDispatcherTarget::ClearMenuBar
        | PpcImportDispatcherTarget::SetMenuBar
        | PpcImportDispatcherTarget::GetMenuHandle
        | PpcImportDispatcherTarget::DrawMenuBar
        | PpcImportDispatcherTarget::InvalMenuBar
        | PpcImportDispatcherTarget::FlashMenuBar
        | PpcImportDispatcherTarget::HMGetHelpMenuHandle
        | PpcImportDispatcherTarget::HMGetBalloons
        | PpcImportDispatcherTarget::HiliteMenu
        | PpcImportDispatcherTarget::MenuNoop
        | PpcImportDispatcherTarget::MenuKey
        | PpcImportDispatcherTarget::MenuEvent
        | PpcImportDispatcherTarget::MenuChoice
        | PpcImportDispatcherTarget::GetMBarHeight
        | PpcImportDispatcherTarget::SetMBarHeight
        | PpcImportDispatcherTarget::MenuSelect => {
            unreachable!("menu imports return through dispatch_menu_import")
        }
        PpcImportDispatcherTarget::GetCurrentThread
        | PpcImportDispatcherTarget::NewThreadEntryUPP
        | PpcImportDispatcherTarget::DisposeThreadEntryUPP
        | PpcImportDispatcherTarget::NewThreadTerminationUPP
        | PpcImportDispatcherTarget::DisposeThreadTerminationUPP
        | PpcImportDispatcherTarget::NewThreadSwitchUPP
        | PpcImportDispatcherTarget::DisposeThreadSwitchUPP
        | PpcImportDispatcherTarget::SetThreadTerminator
        | PpcImportDispatcherTarget::SetThreadSwitcher
        | PpcImportDispatcherTarget::GetThreadState
        | PpcImportDispatcherTarget::GetThreadCurrentTaskRef
        | PpcImportDispatcherTarget::GetThreadStateGivenTaskRef
        | PpcImportDispatcherTarget::SetThreadReadyGivenTaskRef
        | PpcImportDispatcherTarget::SetThreadState
        | PpcImportDispatcherTarget::SetThreadStateEndCritical
        | PpcImportDispatcherTarget::CreateThreadPool
        | PpcImportDispatcherTarget::GetFreeThreadCount
        | PpcImportDispatcherTarget::GetSpecificFreeThreadCount
        | PpcImportDispatcherTarget::GetDefaultThreadStackSize
        | PpcImportDispatcherTarget::ThreadCurrentStackSpace
        | PpcImportDispatcherTarget::NewThread
        | PpcImportDispatcherTarget::YieldToThread
        | PpcImportDispatcherTarget::YieldToAnyThread
        | PpcImportDispatcherTarget::DisposeThread
        | PpcImportDispatcherTarget::ThreadBeginCritical
        | PpcImportDispatcherTarget::ThreadEndCritical => {
            unreachable!("thread imports return through dispatch_thread_import")
        }
        PpcImportDispatcherTarget::TEInit
        | PpcImportDispatcherTarget::TENew
        | PpcImportDispatcherTarget::TEStyleNew
        | PpcImportDispatcherTarget::TESetStyle
        | PpcImportDispatcherTarget::TEUseStyleScrap
        | PpcImportDispatcherTarget::TEContinuousStyle
        | PpcImportDispatcherTarget::TEGetText
        | PpcImportDispatcherTarget::TEDispose
        | PpcImportDispatcherTarget::TEActivate { .. }
        | PpcImportDispatcherTarget::TESetSelect
        | PpcImportDispatcherTarget::TESetText
        | PpcImportDispatcherTarget::TECalText
        | PpcImportDispatcherTarget::TEInsert { .. }
        | PpcImportDispatcherTarget::TEDelete
        | PpcImportDispatcherTarget::TEKey
        | PpcImportDispatcherTarget::TEClick
        | PpcImportDispatcherTarget::TEIdle
        | PpcImportDispatcherTarget::TEUpdate
        | PpcImportDispatcherTarget::TETextBox
        | PpcImportDispatcherTarget::TESetAlignment
        | PpcImportDispatcherTarget::TEGetHeight
        | PpcImportDispatcherTarget::TEGetPoint
        | PpcImportDispatcherTarget::TEScroll { .. }
        | PpcImportDispatcherTarget::TEAutoView
        | PpcImportDispatcherTarget::TECopy { .. }
        | PpcImportDispatcherTarget::TEPaste { .. }
        | PpcImportDispatcherTarget::TETransferScrap { .. }
        | PpcImportDispatcherTarget::TEScrapHandle
        | PpcImportDispatcherTarget::TEScrapLength { .. } => {
            unreachable!("textedit imports return through dispatch_textedit_import")
        }
        PpcImportDispatcherTarget::DSpStartup
        | PpcImportDispatcherTarget::DSpGetVersion
        | PpcImportDispatcherTarget::DSpShutdown
        | PpcImportDispatcherTarget::DSpGetFirstContext
        | PpcImportDispatcherTarget::DSpGetNextContext
        | PpcImportDispatcherTarget::DSpProcessEvent
        | PpcImportDispatcherTarget::DSpBlitFastest
        | PpcImportDispatcherTarget::DSpCanUserSelectContext
        | PpcImportDispatcherTarget::DSpGetMouse
        | PpcImportDispatcherTarget::DSpFindContextFromPoint
        | PpcImportDispatcherTarget::DSpContextGlobalToLocal
        | PpcImportDispatcherTarget::DSpContextLocalToGlobal
        | PpcImportDispatcherTarget::DSpFindBestContext
        | PpcImportDispatcherTarget::DSpFindBestContextOnDisplayID
        | PpcImportDispatcherTarget::DSpUserSelectContext
        | PpcImportDispatcherTarget::DSpSetBlankingColor
        | PpcImportDispatcherTarget::DSpAltBufferNew
        | PpcImportDispatcherTarget::DSpAltBufferGetCGrafPtr
        | PpcImportDispatcherTarget::DSpContextReserve
        | PpcImportDispatcherTarget::DSpContextRelease
        | PpcImportDispatcherTarget::DSpContextSetState
        | PpcImportDispatcherTarget::DSpContextGetState
        | PpcImportDispatcherTarget::DSpContextFadeGamma
        | PpcImportDispatcherTarget::DSpContextFadeGammaIn
        | PpcImportDispatcherTarget::DSpContextFadeGammaOut
        | PpcImportDispatcherTarget::DSpContextGetFrontBuffer
        | PpcImportDispatcherTarget::DSpContextGetBackBuffer
        | PpcImportDispatcherTarget::DSpContextSwapBuffers
        | PpcImportDispatcherTarget::DSpContextSetClutEntries
        | PpcImportDispatcherTarget::DSpContextGetClutEntries
        | PpcImportDispatcherTarget::DSpContextGetDisplayID
        | PpcImportDispatcherTarget::DSpContextGetAttributes
        | PpcImportDispatcherTarget::DSpContextGetFlattenedSize
        | PpcImportDispatcherTarget::DSpContextFlatten
        | PpcImportDispatcherTarget::DSpContextRestore
        | PpcImportDispatcherTarget::DSpContextSetVblProc
        | PpcImportDispatcherTarget::DSpContextIsBusy
        | PpcImportDispatcherTarget::DSpAltBufferDispose
        | PpcImportDispatcherTarget::DSpContextInvalBackBufferRect
        | PpcImportDispatcherTarget::DSpContextSetUnderlayAltBuffer => {
            unreachable!("drawsprocket imports return through dispatch_drawsprocket_import")
        }
        PpcImportDispatcherTarget::ISpElementNewVirtualFromNeeds
        | PpcImportDispatcherTarget::ISpElementListNew
        | PpcImportDispatcherTarget::ISpElementListAddElements
        | PpcImportDispatcherTarget::ISpElementListGetNextEvent
        | PpcImportDispatcherTarget::ISpElementListFlush
        | PpcImportDispatcherTarget::ISpDevicesExtract
        | PpcImportDispatcherTarget::ISpDevicesExtractByClass
        | PpcImportDispatcherTarget::ISpDeviceGetDefinition
        | PpcImportDispatcherTarget::ISpDeviceGetElementList
        | PpcImportDispatcherTarget::ISpElementListExtract
        | PpcImportDispatcherTarget::ISpElementGetInfo
        | PpcImportDispatcherTarget::ISpElementGetSimpleState
        | PpcImportDispatcherTarget::ISpGetVersion
        | PpcImportDispatcherTarget::ISpStartup
        | PpcImportDispatcherTarget::ISpShutdown
        | PpcImportDispatcherTarget::ISpInit
        | PpcImportDispatcherTarget::ISpStop
        | PpcImportDispatcherTarget::ISpSuspend
        | PpcImportDispatcherTarget::ISpResume
        | PpcImportDispatcherTarget::ISpDevicesActivate
        | PpcImportDispatcherTarget::ISpDevicesDeactivate
        | PpcImportDispatcherTarget::ISpConfigure => {
            unreachable!("inputsprocket imports return through dispatch_inputsprocket_import")
        }
        PpcImportDispatcherTarget::QtEnterMovies
        | PpcImportDispatcherTarget::QtExitMovies
        | PpcImportDispatcherTarget::QtGetMoviesError
        | PpcImportDispatcherTarget::QtGetMoviesStickyError
        | PpcImportDispatcherTarget::QtClearMoviesStickyError
        | PpcImportDispatcherTarget::QtGetGraphicsImporterForFile
        | PpcImportDispatcherTarget::QtOpenADefaultComponent
        | PpcImportDispatcherTarget::QtGraphicsImportSetDataHandle
        | PpcImportDispatcherTarget::QtGraphicsImportGetImageDescription
        | PpcImportDispatcherTarget::QtGraphicsImportGetBoundsRect
        | PpcImportDispatcherTarget::QtGraphicsImportSetGWorld
        | PpcImportDispatcherTarget::QtGraphicsImportDraw
        | PpcImportDispatcherTarget::QtOpenMovieFile
        | PpcImportDispatcherTarget::QtNewMovieFromFile
        | PpcImportDispatcherTarget::QtGetMovieBox
        | PpcImportDispatcherTarget::QtSetMovieBox
        | PpcImportDispatcherTarget::QtSetMovieGWorld
        | PpcImportDispatcherTarget::QtStartMovie
        | PpcImportDispatcherTarget::QtStopMovie
        | PpcImportDispatcherTarget::QtMoviesTask
        | PpcImportDispatcherTarget::QtDisposeMovie
        | PpcImportDispatcherTarget::QtIsMovieDone
        | PpcImportDispatcherTarget::QtGoToBeginningOfMovie
        | PpcImportDispatcherTarget::QtGoToEndOfMovie
        | PpcImportDispatcherTarget::QtGetMovieDuration
        | PpcImportDispatcherTarget::QtLoadMovieIntoRam
        | PpcImportDispatcherTarget::QtCloseMovieFile => {
            unreachable!("quicktime imports return through dispatch_quicktime_import")
        }
        PpcImportDispatcherTarget::InsTime
        | PpcImportDispatcherTarget::InsXTime
        | PpcImportDispatcherTarget::PrimeTime
        | PpcImportDispatcherTarget::RmvTime
        | PpcImportDispatcherTarget::VInstall
        | PpcImportDispatcherTarget::VRemove
        | PpcImportDispatcherTarget::SlotVInstall
        | PpcImportDispatcherTarget::SlotVRemove => {
            unreachable!("time and vbl imports return through dispatch_time_import")
        }
        PpcImportDispatcherTarget::DrawGrowIcon => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::GetPictInfo
        | PpcImportDispatcherTarget::DrawPicture
        | PpcImportDispatcherTarget::KillPicture => {
            unreachable!("picture imports return through dispatch_picture_import")
        }
        PpcImportDispatcherTarget::InitCursor
        | PpcImportDispatcherTarget::GetQDGlobalsArrow
        | PpcImportDispatcherTarget::HideCursor
        | PpcImportDispatcherTarget::ShowCursor
        | PpcImportDispatcherTarget::ShieldCursor
        | PpcImportDispatcherTarget::CrsrDevNextDevice
        | PpcImportDispatcherTarget::CrsrDevMoveTo
        | PpcImportDispatcherTarget::GetCursor
        | PpcImportDispatcherTarget::SetCursor
        | PpcImportDispatcherTarget::GetCCursor
        | PpcImportDispatcherTarget::SetCCursor
        | PpcImportDispatcherTarget::DisposeCCursor
        | PpcImportDispatcherTarget::GetCIcon
        | PpcImportDispatcherTarget::PlotCIcon
        | PpcImportDispatcherTarget::DisposeCIcon => {
            unreachable!("cursor and cicon imports return through dispatch_cursor_import")
        }
        PpcImportDispatcherTarget::FSClose
        | PpcImportDispatcherTarget::PBClose
        | PpcImportDispatcherTarget::PBFlushFile
        | PpcImportDispatcherTarget::FSRead
        | PpcImportDispatcherTarget::PBRead
        | PpcImportDispatcherTarget::FSWrite
        | PpcImportDispatcherTarget::PBWrite
        | PpcImportDispatcherTarget::GetEOF
        | PpcImportDispatcherTarget::PBGetEOF
        | PpcImportDispatcherTarget::SetEOF
        | PpcImportDispatcherTarget::AllocContig
        | PpcImportDispatcherTarget::PBSetEOF
        | PpcImportDispatcherTarget::GetFPos
        | PpcImportDispatcherTarget::SetFPos
        | PpcImportDispatcherTarget::PBSetFPos
        | PpcImportDispatcherTarget::PBCreate(_)
        | PpcImportDispatcherTarget::FSpCreate
        | PpcImportDispatcherTarget::HCreate
        | PpcImportDispatcherTarget::HRename
        | PpcImportDispatcherTarget::Create
        | PpcImportDispatcherTarget::FSpDelete
        | PpcImportDispatcherTarget::DeleteByName(_)
        | PpcImportDispatcherTarget::FSOpen
        | PpcImportDispatcherTarget::FSpCreateResFile
        | PpcImportDispatcherTarget::HCreateResFile
        | PpcImportDispatcherTarget::FSpOpenResFile
        | PpcImportDispatcherTarget::FSpOpenDF
        | PpcImportDispatcherTarget::FSpOpenRF
        | PpcImportDispatcherTarget::HOpen
        | PpcImportDispatcherTarget::PBOpen
        | PpcImportDispatcherTarget::PBHOpenDF
        | PpcImportDispatcherTarget::CurResFile
        | PpcImportDispatcherTarget::UseResFile
        | PpcImportDispatcherTarget::OpenResFile
        | PpcImportDispatcherTarget::HOpenResFile
        | PpcImportDispatcherTarget::ResError
        | PpcImportDispatcherTarget::GetVol
        | PpcImportDispatcherTarget::GetWDInfo
        | PpcImportDispatcherTarget::HGetVol
        | PpcImportDispatcherTarget::HSetVol
        | PpcImportDispatcherTarget::FlushVol
        | PpcImportDispatcherTarget::PBFlushVol
        | PpcImportDispatcherTarget::PBHGetVInfo
        | PpcImportDispatcherTarget::GetVInfo
        | PpcImportDispatcherTarget::PBDTGetPath
        | PpcImportDispatcherTarget::PBDTGetCommentSync
        | PpcImportDispatcherTarget::PBGetFInfo
        | PpcImportDispatcherTarget::PBHGetFInfo
        | PpcImportDispatcherTarget::PBSetFInfo
        | PpcImportDispatcherTarget::PBHSetFInfo
        | PpcImportDispatcherTarget::FSpGetFInfo
        | PpcImportDispatcherTarget::GetFInfo
        | PpcImportDispatcherTarget::HGetFInfo
        | PpcImportDispatcherTarget::FSpSetFInfo
        | PpcImportDispatcherTarget::HSetFInfo
        | PpcImportDispatcherTarget::PBGetCatInfo
        | PpcImportDispatcherTarget::PBSetCatInfo
        | PpcImportDispatcherTarget::DirCreate
        | PpcImportDispatcherTarget::FSpDirCreate
        | PpcImportDispatcherTarget::FSMakeFSSpec
        | PpcImportDispatcherTarget::PBGetFCBInfo
        | PpcImportDispatcherTarget::FindFolder
        | PpcImportDispatcherTarget::ResolveAliasFile
        | PpcImportDispatcherTarget::ResolveAliasFileWithMountFlags
        | PpcImportDispatcherTarget::GetIconRefFromFile
        | PpcImportDispatcherTarget::GetIconRef
        | PpcImportDispatcherTarget::PlotIconRef
        | PpcImportDispatcherTarget::ReleaseIconRef
        | PpcImportDispatcherTarget::ResolveAlias
        | PpcImportDispatcherTarget::UpdateAlias
        | PpcImportDispatcherTarget::NewAlias
        | PpcImportDispatcherTarget::NewAliasMinimalFromFullPath
        | PpcImportDispatcherTarget::FileCompatibility(_) => {
            unreachable!("file imports return through dispatch_file_import")
        }
        PpcImportDispatcherTarget::SetResLoad
        | PpcImportDispatcherTarget::LMGetResLoad
        | PpcImportDispatcherTarget::LoadResource
        | PpcImportDispatcherTarget::GetResource
        | PpcImportDispatcherTarget::Get1Resource
        | PpcImportDispatcherTarget::GetNamedResource
        | PpcImportDispatcherTarget::Get1NamedResource
        | PpcImportDispatcherTarget::GetIndResource
        | PpcImportDispatcherTarget::Get1IndResource
        | PpcImportDispatcherTarget::CountResources
        | PpcImportDispatcherTarget::Count1Resources
        | PpcImportDispatcherTarget::CountTypes
        | PpcImportDispatcherTarget::Count1Types
        | PpcImportDispatcherTarget::GetIndType
        | PpcImportDispatcherTarget::Get1IndType
        | PpcImportDispatcherTarget::UniqueID
        | PpcImportDispatcherTarget::Unique1ID
        | PpcImportDispatcherTarget::ReleaseResource
        | PpcImportDispatcherTarget::DetachResource
        | PpcImportDispatcherTarget::GetIndString
        | PpcImportDispatcherTarget::GetString
        | PpcImportDispatcherTarget::GetResAttrs
        | PpcImportDispatcherTarget::SetResAttrs
        | PpcImportDispatcherTarget::GetResInfo
        | PpcImportDispatcherTarget::GetResourceSizeOnDisk
        | PpcImportDispatcherTarget::SetResInfo
        | PpcImportDispatcherTarget::HomeResFile
        | PpcImportDispatcherTarget::UpdateResFile
        | PpcImportDispatcherTarget::AddResource
        | PpcImportDispatcherTarget::ChangedResource
        | PpcImportDispatcherTarget::WriteResource
        | PpcImportDispatcherTarget::RemoveResource
        | PpcImportDispatcherTarget::ReadPartialResource
        | PpcImportDispatcherTarget::CloseResFile
        | PpcImportDispatcherTarget::GetPicture
        | PpcImportDispatcherTarget::GetIconSuite
        | PpcImportDispatcherTarget::GetIcon
        | PpcImportDispatcherTarget::GetPattern
        | PpcImportDispatcherTarget::GetIndPattern
        | PpcImportDispatcherTarget::GetPixPat
        | PpcImportDispatcherTarget::GetIntlResource => {
            unreachable!("resource imports return through dispatch_resource_import")
        }
        PpcImportDispatcherTarget::InitGraf
        | PpcImportDispatcherTarget::GetForeColor
        | PpcImportDispatcherTarget::GetBackColor
        | PpcImportDispatcherTarget::ForeColor
        | PpcImportDispatcherTarget::BackColor
        | PpcImportDispatcherTarget::RGBForeColor
        | PpcImportDispatcherTarget::RGBBackColor
        | PpcImportDispatcherTarget::OpColor
        | PpcImportDispatcherTarget::HiliteColor
        | PpcImportDispatcherTarget::PmForeColor
        | PpcImportDispatcherTarget::PmBackColor
        | PpcImportDispatcherTarget::Color2Index
        | PpcImportDispatcherTarget::Index2Color
        | PpcImportDispatcherTarget::RGB2HSL
        | PpcImportDispatcherTarget::RGB2HSV
        | PpcImportDispatcherTarget::HSV2RGB
        | PpcImportDispatcherTarget::SetRect
        | PpcImportDispatcherTarget::SectRect
        | PpcImportDispatcherTarget::UnionRect
        | PpcImportDispatcherTarget::EqualRect
        | PpcImportDispatcherTarget::EmptyRect
        | PpcImportDispatcherTarget::SetPt
        | PpcImportDispatcherTarget::EqualPt
        | PpcImportDispatcherTarget::AddPt
        | PpcImportDispatcherTarget::SubPt
        | PpcImportDispatcherTarget::LocalToGlobal
        | PpcImportDispatcherTarget::GlobalToLocal
        | PpcImportDispatcherTarget::PtInRect
        | PpcImportDispatcherTarget::OffsetRect
        | PpcImportDispatcherTarget::MapRect
        | PpcImportDispatcherTarget::InsetRect => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::MathCeil
        | PpcImportDispatcherTarget::MathSqrt
        | PpcImportDispatcherTarget::MathExp
        | PpcImportDispatcherTarget::MathSin
        | PpcImportDispatcherTarget::MathCos
        | PpcImportDispatcherTarget::MathAsin
        | PpcImportDispatcherTarget::MathTan
        | PpcImportDispatcherTarget::MathAtan
        | PpcImportDispatcherTarget::MathAtan2
        | PpcImportDispatcherTarget::MathPow
        | PpcImportDispatcherTarget::MathFmod
        | PpcImportDispatcherTarget::MathLog
        | PpcImportDispatcherTarget::MathLog10
        | PpcImportDispatcherTarget::MathDtox80
        | PpcImportDispatcherTarget::X2Fix
        | PpcImportDispatcherTarget::FixRatio
        | PpcImportDispatcherTarget::FixMul
        | PpcImportDispatcherTarget::FixDiv
        | PpcImportDispatcherTarget::Long2Fix
        | PpcImportDispatcherTarget::Fix2Long
        | PpcImportDispatcherTarget::FixRound
        | PpcImportDispatcherTarget::Fix2Frac
        | PpcImportDispatcherTarget::Frac2Fix
        | PpcImportDispatcherTarget::Frac2X
        | PpcImportDispatcherTarget::X2Frac
        | PpcImportDispatcherTarget::FracSin
        | PpcImportDispatcherTarget::FracCos
        | PpcImportDispatcherTarget::FracSqrt
        | PpcImportDispatcherTarget::FracMul
        | PpcImportDispatcherTarget::FracDiv
        | PpcImportDispatcherTarget::FixATan2
        | PpcImportDispatcherTarget::WideAdd
        | PpcImportDispatcherTarget::WideSubtract
        | PpcImportDispatcherTarget::WideNegate
        | PpcImportDispatcherTarget::WideShift
        | PpcImportDispatcherTarget::WideBitShift
        | PpcImportDispatcherTarget::WideMultiply
        | PpcImportDispatcherTarget::WideDivide
        | PpcImportDispatcherTarget::WideWideDivide
        | PpcImportDispatcherTarget::WideCompare
        | PpcImportDispatcherTarget::WideSquareRoot => {
            unreachable!("math imports return through dispatch_math_import")
        }
        PpcImportDispatcherTarget::MoveTo
        | PpcImportDispatcherTarget::Move
        | PpcImportDispatcherTarget::LineTo
        | PpcImportDispatcherTarget::Line => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::DrawChar
        | PpcImportDispatcherTarget::DrawText
        | PpcImportDispatcherTarget::DrawString
        | PpcImportDispatcherTarget::TextFont
        | PpcImportDispatcherTarget::TextFace
        | PpcImportDispatcherTarget::TextMode
        | PpcImportDispatcherTarget::TextSize => {
            unreachable!("font and text imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::PaintRect
        | PpcImportDispatcherTarget::EraseRect
        | PpcImportDispatcherTarget::InvertRect
        | PpcImportDispatcherTarget::FrameRect
        | PpcImportDispatcherTarget::FillRect
        | PpcImportDispatcherTarget::FillCRect => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameOval
        | PpcImportDispatcherTarget::PaintOval
        | PpcImportDispatcherTarget::EraseOval
        | PpcImportDispatcherTarget::PaintArc => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameRgn
        | PpcImportDispatcherTarget::PaintRgn
        | PpcImportDispatcherTarget::FillRgn
        | PpcImportDispatcherTarget::InvertRgn => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::FrameRoundRect | PpcImportDispatcherTarget::PaintRoundRect => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::InvalRect
        | PpcImportDispatcherTarget::ValidRect
        | PpcImportDispatcherTarget::BeginUpdate
        | PpcImportDispatcherTarget::EndUpdate => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::ClipRect
        | PpcImportDispatcherTarget::GetClip
        | PpcImportDispatcherTarget::SetClip
        | PpcImportDispatcherTarget::BitMapToRegion
        | PpcImportDispatcherTarget::NewRgn
        | PpcImportDispatcherTarget::DisposeRgn
        | PpcImportDispatcherTarget::CopyRgn
        | PpcImportDispatcherTarget::OpenRgn
        | PpcImportDispatcherTarget::CloseRgn
        | PpcImportDispatcherTarget::SectRgn
        | PpcImportDispatcherTarget::UnionRgn
        | PpcImportDispatcherTarget::DiffRgn
        | PpcImportDispatcherTarget::XorRgn
        | PpcImportDispatcherTarget::SetEmptyRgn
        | PpcImportDispatcherTarget::SetRectRgn
        | PpcImportDispatcherTarget::RectRgn
        | PpcImportDispatcherTarget::OffsetRgn
        | PpcImportDispatcherTarget::EmptyRgn
        | PpcImportDispatcherTarget::PtInRgn
        | PpcImportDispatcherTarget::RectInRgn => {
            unreachable!("Region Manager imports return through dispatch_region_import")
        }
        PpcImportDispatcherTarget::GetPen
        | PpcImportDispatcherTarget::HidePen
        | PpcImportDispatcherTarget::ShowPen
        | PpcImportDispatcherTarget::PenSize
        | PpcImportDispatcherTarget::PenMode
        | PpcImportDispatcherTarget::PenNormal
        | PpcImportDispatcherTarget::PenPixPat
        | PpcImportDispatcherTarget::GetPenState
        | PpcImportDispatcherTarget::SetPenState => {
            unreachable!("QuickDraw imports return through dispatch_quickdraw_import")
        }
        PpcImportDispatcherTarget::CopyBits => {
            unreachable!("bit-transfer imports return through dispatch_bit_transfer_import")
        }
        PpcImportDispatcherTarget::OpenPoly
        | PpcImportDispatcherTarget::ClosePoly
        | PpcImportDispatcherTarget::KillPoly
        | PpcImportDispatcherTarget::FramePoly
        | PpcImportDispatcherTarget::PaintPoly
        | PpcImportDispatcherTarget::FillPoly => {
            unreachable!("Polygon Manager imports return through dispatch_polygon_import")
        }
        PpcImportDispatcherTarget::NewCWindow
        | PpcImportDispatcherTarget::GetNewCWindow
        | PpcImportDispatcherTarget::GetWRefCon
        | PpcImportDispatcherTarget::SetWRefCon
        | PpcImportDispatcherTarget::SizeWindow
        | PpcImportDispatcherTarget::MoveWindow => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::ShowWindow
        | PpcImportDispatcherTarget::HideWindow
        | PpcImportDispatcherTarget::ShowHide
        | PpcImportDispatcherTarget::CloseWindow => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::CloseDialog | PpcImportDispatcherTarget::DisposeDialog => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::FrontWindow
        | PpcImportDispatcherTarget::SetWinColor
        | PpcImportDispatcherTarget::PaintOne
        | PpcImportDispatcherTarget::PaintBehind
        | PpcImportDispatcherTarget::CalcVisBehind => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::SelectWindow => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::ActivatePalette
        | PpcImportDispatcherTarget::NSetPalette
        | PpcImportDispatcherTarget::GetPalette => {
            unreachable!("palette imports return through dispatch_palette_import")
        }
        PpcImportDispatcherTarget::GetPort
        | PpcImportDispatcherTarget::SetPort
        | PpcImportDispatcherTarget::GetWindowPort
        | PpcImportDispatcherTarget::SetPortWindowPort
        | PpcImportDispatcherTarget::NewGWorld
        | PpcImportDispatcherTarget::UpdateGWorld
        | PpcImportDispatcherTarget::DisposeGWorld
        | PpcImportDispatcherTarget::QDError
        | PpcImportDispatcherTarget::GetGWorld
        | PpcImportDispatcherTarget::SetGWorld
        | PpcImportDispatcherTarget::GetGWorldDevice
        | PpcImportDispatcherTarget::GetGWorldPixMap
        | PpcImportDispatcherTarget::OpenPort
        | PpcImportDispatcherTarget::OpenCPort
        | PpcImportDispatcherTarget::CloseCPort
        | PpcImportDispatcherTarget::SetPortBits { .. }
        | PpcImportDispatcherTarget::GetPixBaseAddr
        | PpcImportDispatcherTarget::GetPixRowBytes
        | PpcImportDispatcherTarget::LockPixels
        | PpcImportDispatcherTarget::UnlockPixels
        | PpcImportDispatcherTarget::GetPixelsState
        | PpcImportDispatcherTarget::SetPixelsState
        | PpcImportDispatcherTarget::AllowPurgePixels
        | PpcImportDispatcherTarget::NoPurgePixels
        | PpcImportDispatcherTarget::SetOrigin => {
            unreachable!("GWorld imports return through dispatch_gworld_import")
        }
        PpcImportDispatcherTarget::GetWMgrPort => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::GetGDevice
        | PpcImportDispatcherTarget::SetGDevice
        | PpcImportDispatcherTarget::GetDeviceList
        | PpcImportDispatcherTarget::GetMainDevice
        | PpcImportDispatcherTarget::GetMaxDevice
        | PpcImportDispatcherTarget::GetNextDevice
        | PpcImportDispatcherTarget::TestDeviceAttribute
        | PpcImportDispatcherTarget::SetDeviceAttribute
        | PpcImportDispatcherTarget::HasDepth
        | PpcImportDispatcherTarget::SetDepth => {
            unreachable!("graphics-device imports return through dispatch_graphics_device_import")
        }
        PpcImportDispatcherTarget::GetSysFont
        | PpcImportDispatcherTarget::GetAppFont
        | PpcImportDispatcherTarget::GetDefFontSize
        | PpcImportDispatcherTarget::GetFontName => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::DMGetDisplayIDByGDevice
        | PpcImportDispatcherTarget::DMGetNameByAVID
        | PpcImportDispatcherTarget::DMGetGDeviceByDisplayID => {
            unreachable!("display manager imports return through dispatch_display_import")
        }
        PpcImportDispatcherTarget::GetCTable
        | PpcImportDispatcherTarget::GetCTSeed
        | PpcImportDispatcherTarget::MakeITable
        | PpcImportDispatcherTarget::CTabChanged
        | PpcImportDispatcherTarget::ProtectEntry
        | PpcImportDispatcherTarget::ReserveEntry
        | PpcImportDispatcherTarget::RestoreEntries
        | PpcImportDispatcherTarget::SetEntries
        | PpcImportDispatcherTarget::RestoreDeviceClut
        | PpcImportDispatcherTarget::DisposeCTable
        | PpcImportDispatcherTarget::NewPixMap
        | PpcImportDispatcherTarget::DisposePixMap => {
            unreachable!("color-table imports return through dispatch_color_table_import")
        }
        PpcImportDispatcherTarget::FindWindow => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::GetDCtlEntry
        | PpcImportDispatcherTarget::GetADBInfo
        | PpcImportDispatcherTarget::OpenDriver
        | PpcImportDispatcherTarget::Control
        | PpcImportDispatcherTarget::PBControl
        | PpcImportDispatcherTarget::PBStatus => {
            unreachable!("Device Manager imports return through dispatch_device_import")
        }
        PpcImportDispatcherTarget::Gestalt | PpcImportDispatcherTarget::NewGestaltValue => {
            unreachable!("Gestalt imports return through dispatch_gestalt_import")
        }
        PpcImportDispatcherTarget::GetSharedLibrary
        | PpcImportDispatcherTarget::FindSymbol
        | PpcImportDispatcherTarget::CountSymbols
        | PpcImportDispatcherTarget::GetIndSymbol
        | PpcImportDispatcherTarget::CloseConnection
        | PpcImportDispatcherTarget::GetMemFragment
        | PpcImportDispatcherTarget::GetDiskFragment => {
            unreachable!("cfm imports return through dispatch_cfm_import")
        }
        PpcImportDispatcherTarget::StandardGetFile => {
            unreachable!("standard file imports return through dispatch_standard_file_import")
        }
        PpcImportDispatcherTarget::GetScrap
        | PpcImportDispatcherTarget::PutScrap
        | PpcImportDispatcherTarget::ZeroScrap
        | PpcImportDispatcherTarget::LoadScrap
        | PpcImportDispatcherTarget::UnloadScrap => {
            unreachable!("Scrap Manager imports return through dispatch_scrap_import")
        }
        PpcImportDispatcherTarget::DMGetFirstScreenDevice
        | PpcImportDispatcherTarget::DMGetNextScreenDevice
        | PpcImportDispatcherTarget::DMGetDisplayMode
        | PpcImportDispatcherTarget::DMCheckDisplayMode
        | PpcImportDispatcherTarget::DMSetDisplayMode
        | PpcImportDispatcherTarget::DMNewDisplayModeList
        | PpcImportDispatcherTarget::DMGetIndexedDisplayModeFromList
        | PpcImportDispatcherTarget::DMDisposeList
        | PpcImportDispatcherTarget::DMBeginConfigureDisplays
        | PpcImportDispatcherTarget::DMEndConfigureDisplays => {
            unreachable!("display manager imports return through dispatch_display_import")
        }
        PpcImportDispatcherTarget::GetNewDialog
        | PpcImportDispatcherTarget::NewDialog
        | PpcImportDispatcherTarget::NewFeaturesDialog
        | PpcImportDispatcherTarget::GetDialogItem
        | PpcImportDispatcherTarget::GetDialogItemAsControl
        | PpcImportDispatcherTarget::SetDialogItem
        | PpcImportDispatcherTarget::GetDialogItemText
        | PpcImportDispatcherTarget::SetDialogItemText
        | PpcImportDispatcherTarget::SetDialogDefaultItem
        | PpcImportDispatcherTarget::SetDialogCancelItem
        | PpcImportDispatcherTarget::SetDialogTracksCursor
        | PpcImportDispatcherTarget::StdFilterProc
        | PpcImportDispatcherTarget::GetStdFilterProc
        | PpcImportDispatcherTarget::DrawDialog
        | PpcImportDispatcherTarget::ModalDialog => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::DrawControls => {
            unreachable!("control imports return through dispatch_control_import")
        }
        PpcImportDispatcherTarget::SetControlTitle
        | PpcImportDispatcherTarget::SetControlValue
        | PpcImportDispatcherTarget::HiliteControl => {
            unreachable!("control imports return through dispatch_control_import")
        }
        PpcImportDispatcherTarget::InitFonts => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::InitWindows => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::MeasureText | PpcImportDispatcherTarget::RealFont => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::SelectDialogItemText
        | PpcImportDispatcherTarget::InitDialogs => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::SystemTask
        | PpcImportDispatcherTarget::SystemClick
        | PpcImportDispatcherTarget::OpenDeskAcc => {
            unreachable!("Desk Manager imports return through dispatch_desk_import")
        }
        PpcImportDispatcherTarget::AEInstallEventHandler
        | PpcImportDispatcherTarget::AEProcessAppleEvent => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::LNew
        | PpcImportDispatcherTarget::LDispose
        | PpcImportDispatcherTarget::LAddRow
        | PpcImportDispatcherTarget::LDelRow
        | PpcImportDispatcherTarget::LGetSelect
        | PpcImportDispatcherTarget::LSetSelect
        | PpcImportDispatcherTarget::LSetCell
        | PpcImportDispatcherTarget::LGetCell
        | PpcImportDispatcherTarget::LClick
        | PpcImportDispatcherTarget::LActivate
        | PpcImportDispatcherTarget::LSetDrawingMode
        | PpcImportDispatcherTarget::LScroll
        | PpcImportDispatcherTarget::LSize
        | PpcImportDispatcherTarget::LUpdate
        | PpcImportDispatcherTarget::LAutoScroll
        | PpcImportDispatcherTarget::LSearch => {
            unreachable!("list imports return through dispatch_list_import")
        }
        PpcImportDispatcherTarget::SysEnvirons
        | PpcImportDispatcherTarget::SVersion
        | PpcImportDispatcherTarget::EqualString
        | PpcImportDispatcherTarget::NumToString
        | PpcImportDispatcherTarget::StringToNum
        | PpcImportDispatcherTarget::Random
        | PpcImportDispatcherTarget::BitAnd
        | PpcImportDispatcherTarget::BitOr
        | PpcImportDispatcherTarget::BitTst => {
            unreachable!("Toolbox Utilities imports return through dispatch_toolbox_import")
        }
        PpcImportDispatcherTarget::IUEqualPString => None,
        PpcImportDispatcherTarget::TextWidth
        | PpcImportDispatcherTarget::TruncString
        | PpcImportDispatcherTarget::StringWidth
        | PpcImportDispatcherTarget::CharWidth
        | PpcImportDispatcherTarget::GetFontInfo
        | PpcImportDispatcherTarget::FontMetrics
        | PpcImportDispatcherTarget::GetFNum => {
            unreachable!("Font Manager imports return through dispatch_font_import")
        }
        PpcImportDispatcherTarget::AESetInteractionAllowed
        | PpcImportDispatcherTarget::AEGetInteractionAllowed => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::StdMemset
        | PpcImportDispatcherTarget::StdMemcmp
        | PpcImportDispatcherTarget::StdMemcpy
        | PpcImportDispatcherTarget::StdMemmove => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::StdMalloc
        | PpcImportDispatcherTarget::StdFree
        | PpcImportDispatcherTarget::StdCalloc
        | PpcImportDispatcherTarget::StdRealloc
        | PpcImportDispatcherTarget::StdStrcpy
        | PpcImportDispatcherTarget::StdStrncpy
        | PpcImportDispatcherTarget::StdStrcat
        | PpcImportDispatcherTarget::StdStrncat
        | PpcImportDispatcherTarget::StdStrcmp
        | PpcImportDispatcherTarget::StdStrncmp
        | PpcImportDispatcherTarget::StdStrlen
        | PpcImportDispatcherTarget::StdMemchr
        | PpcImportDispatcherTarget::StdStrchr
        | PpcImportDispatcherTarget::StdStrrchr
        | PpcImportDispatcherTarget::StdStrspn
        | PpcImportDispatcherTarget::StdStrcspn
        | PpcImportDispatcherTarget::StdStrpbrk
        | PpcImportDispatcherTarget::StdStrstr
        | PpcImportDispatcherTarget::StdAtoi
        | PpcImportDispatcherTarget::StdGetenv
        | PpcImportDispatcherTarget::StdSprintf => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::StdIoCompatibility(operation) => {
            Some(ppc_dispatch_process_stdio_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                files,
                writable_refnums,
                vfs_files,
                next_file_ref_num,
                stdio_streams,
            ))
        }
        PpcImportDispatcherTarget::StdAbs
        | PpcImportDispatcherTarget::StdToupper
        | PpcImportDispatcherTarget::StdTolower
        | PpcImportDispatcherTarget::StdIsalnum
        | PpcImportDispatcherTarget::StdIsalpha
        | PpcImportDispatcherTarget::StdIsascii
        | PpcImportDispatcherTarget::StdIscntrl
        | PpcImportDispatcherTarget::StdIsdigit
        | PpcImportDispatcherTarget::StdIsgraph
        | PpcImportDispatcherTarget::StdIslower
        | PpcImportDispatcherTarget::StdIsprint
        | PpcImportDispatcherTarget::StdIspunct
        | PpcImportDispatcherTarget::StdIsspace
        | PpcImportDispatcherTarget::StdIsupper
        | PpcImportDispatcherTarget::StdIsxdigit
        | PpcImportDispatcherTarget::StdToascii
        | PpcImportDispatcherTarget::StdSrand
        | PpcImportDispatcherTarget::StdRand
        | PpcImportDispatcherTarget::StdTime
        | PpcImportDispatcherTarget::P2CStr
        | PpcImportDispatcherTarget::C2PStr
        | PpcImportDispatcherTarget::CopyCStringToPascal
        | PpcImportDispatcherTarget::CopyPascalStringToC
        | PpcImportDispatcherTarget::UpperText => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::CfStringMakeConstantString
        | PpcImportDispatcherTarget::CfStringCreateWithCString
        | PpcImportDispatcherTarget::CfStringCreateWithPascalString
        | PpcImportDispatcherTarget::CfStringCreateWithBytes
        | PpcImportDispatcherTarget::CfStringGetCString
        | PpcImportDispatcherTarget::CfStringGetBytes
        | PpcImportDispatcherTarget::CfStringGetLength
        | PpcImportDispatcherTarget::CfStringGetSystemEncoding
        | PpcImportDispatcherTarget::CfRetain
        | PpcImportDispatcherTarget::CfRelease
        | PpcImportDispatcherTarget::CfGetRetainCount
        | PpcImportDispatcherTarget::CfBundleGetBundleWithIdentifier => {
            unreachable!("Core Foundation imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::GetCurrentProcess
        | PpcImportDispatcherTarget::WakeUpProcess
        | PpcImportDispatcherTarget::SameProcess
        | PpcImportDispatcherTarget::GetProcessInformation => {
            unreachable!("Process Manager imports return through dispatch_process_import")
        }
        PpcImportDispatcherTarget::ParamText
        | PpcImportDispatcherTarget::AlertReturnDefault
        | PpcImportDispatcherTarget::StandardAlert => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::Q3Initialize
        | PpcImportDispatcherTarget::Q3Exit
        | PpcImportDispatcherTarget::Q3GetVersion => {
            unreachable!("QuickDraw 3D core imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3MemoryStorageNew
        | PpcImportDispatcherTarget::Q3MemoryStorageNewBuffer
        | PpcImportDispatcherTarget::Q3FSSpecStorageNew => {
            unreachable!("QuickDraw 3D storage imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3NewObject => {
            unreachable!("QuickDraw 3D core imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3FileNew => {
            unreachable!("QuickDraw 3D file imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ViewNew
        | PpcImportDispatcherTarget::Q3DisplayGroupNew
        | PpcImportDispatcherTarget::Q3OrderedDisplayGroupNew => {
            unreachable!("QuickDraw 3D group/view imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ErrorGet => {
            unreachable!("QuickDraw 3D core imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ObjectDispose
        | PpcImportDispatcherTarget::Q3ObjectDuplicate
        | PpcImportDispatcherTarget::Q3SharedGetReference
        | PpcImportDispatcherTarget::Q3SharedIsReferenced
        | PpcImportDispatcherTarget::Q3SharedGetType
        | PpcImportDispatcherTarget::Q3ShapeGetType
        | PpcImportDispatcherTarget::Q3ShapeGetLeafType
        | PpcImportDispatcherTarget::Q3ObjectIsDrawable
        | PpcImportDispatcherTarget::Q3ObjectIsType
        | PpcImportDispatcherTarget::Q3ObjectGetType
        | PpcImportDispatcherTarget::Q3ObjectGetLeafType
        | PpcImportDispatcherTarget::Q3GeometryGetType => {
            unreachable!("QuickDraw 3D object imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ShaderGetType => {
            unreachable!("QuickDraw 3D shader imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3GroupGetType
        | PpcImportDispatcherTarget::Q3RendererNewFromType
        | PpcImportDispatcherTarget::Q3RendererGetType
        | PpcImportDispatcherTarget::Q3RendererSync
        | PpcImportDispatcherTarget::Q3RendererFlush => {
            unreachable!("QuickDraw 3D object/renderer imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3InteractiveRendererSetDoubleBufferBypass
        | PpcImportDispatcherTarget::Q3InteractiveRendererSetPreferences
        | PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveContextHints
        | PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveContextHints
        | PpcImportDispatcherTarget::Q3InteractiveRendererGetRaveDrawContexts
        | PpcImportDispatcherTarget::Q3InteractiveRendererSetRaveTextureFilter => {
            unreachable!("QuickDraw 3D object/renderer imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3TextureShaderNew
        | PpcImportDispatcherTarget::Q3LambertIlluminationNew
        | PpcImportDispatcherTarget::Q3NullIlluminationNew
        | PpcImportDispatcherTarget::Q3PhongIlluminationNew
        | PpcImportDispatcherTarget::Q3TextureShaderGetTexture
        | PpcImportDispatcherTarget::Q3MipmapTextureNew
        | PpcImportDispatcherTarget::Q3MipmapTextureGetMipmap => {
            unreachable!(
                "QuickDraw 3D shader imports return through dispatch_q3_shader_style_import_fast"
            )
        }
        PpcImportDispatcherTarget::Q3StorageGetType
        | PpcImportDispatcherTarget::Q3MemoryStorageSet
        | PpcImportDispatcherTarget::Q3MemoryStorageGetBuffer
        | PpcImportDispatcherTarget::Q3MemoryStorageSetBuffer
        | PpcImportDispatcherTarget::Q3MemoryStorageGetType => {
            unreachable!("QuickDraw 3D storage imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ViewAngleAspectCameraNew
        | PpcImportDispatcherTarget::Q3OrthographicCameraNew
        | PpcImportDispatcherTarget::Q3ViewPlaneCameraNew
        | PpcImportDispatcherTarget::Q3CameraGetPlacement
        | PpcImportDispatcherTarget::Q3CameraSetPlacement
        | PpcImportDispatcherTarget::Q3CameraGetRange
        | PpcImportDispatcherTarget::Q3CameraSetRange
        | PpcImportDispatcherTarget::Q3CameraGetViewPort
        | PpcImportDispatcherTarget::Q3CameraSetViewPort
        | PpcImportDispatcherTarget::Q3CameraGetWorldToView
        | PpcImportDispatcherTarget::Q3CameraGetViewToFrustum => {
            unreachable!("QuickDraw 3D scene imports return through dispatch_q3_scene_import_fast")
        }
        PpcImportDispatcherTarget::Q3LightGetType
        | PpcImportDispatcherTarget::Q3LightGetState
        | PpcImportDispatcherTarget::Q3LightSetState
        | PpcImportDispatcherTarget::Q3LightGetBrightness
        | PpcImportDispatcherTarget::Q3LightSetBrightness
        | PpcImportDispatcherTarget::Q3LightGetColor
        | PpcImportDispatcherTarget::Q3LightSetColor
        | PpcImportDispatcherTarget::Q3LightGetData
        | PpcImportDispatcherTarget::Q3LightSetData
        | PpcImportDispatcherTarget::Q3AmbientLightNew
        | PpcImportDispatcherTarget::Q3AmbientLightGetData
        | PpcImportDispatcherTarget::Q3AmbientLightSetData
        | PpcImportDispatcherTarget::Q3DirectionalLightNew
        | PpcImportDispatcherTarget::Q3DirectionalLightGetCastShadowsState
        | PpcImportDispatcherTarget::Q3DirectionalLightSetCastShadowsState
        | PpcImportDispatcherTarget::Q3DirectionalLightGetDirection
        | PpcImportDispatcherTarget::Q3DirectionalLightSetDirection
        | PpcImportDispatcherTarget::Q3DirectionalLightGetData
        | PpcImportDispatcherTarget::Q3DirectionalLightSetData => {
            unreachable!("QuickDraw 3D scene imports return through dispatch_q3_scene_import_fast")
        }
        PpcImportDispatcherTarget::Q3PointLightNew
        | PpcImportDispatcherTarget::Q3PointLightGetCastShadowsState
        | PpcImportDispatcherTarget::Q3PointLightSetCastShadowsState
        | PpcImportDispatcherTarget::Q3PointLightGetAttenuation
        | PpcImportDispatcherTarget::Q3PointLightSetAttenuation
        | PpcImportDispatcherTarget::Q3PointLightGetLocation
        | PpcImportDispatcherTarget::Q3PointLightSetLocation
        | PpcImportDispatcherTarget::Q3PointLightGetData
        | PpcImportDispatcherTarget::Q3PointLightSetData
        | PpcImportDispatcherTarget::Q3SpotLightNew
        | PpcImportDispatcherTarget::Q3SpotLightGetCastShadowsState
        | PpcImportDispatcherTarget::Q3SpotLightSetCastShadowsState
        | PpcImportDispatcherTarget::Q3SpotLightGetAttenuation
        | PpcImportDispatcherTarget::Q3SpotLightSetAttenuation
        | PpcImportDispatcherTarget::Q3SpotLightGetLocation
        | PpcImportDispatcherTarget::Q3SpotLightSetLocation
        | PpcImportDispatcherTarget::Q3SpotLightGetDirection
        | PpcImportDispatcherTarget::Q3SpotLightSetDirection
        | PpcImportDispatcherTarget::Q3SpotLightGetHotAngle
        | PpcImportDispatcherTarget::Q3SpotLightSetHotAngle
        | PpcImportDispatcherTarget::Q3SpotLightGetOuterAngle
        | PpcImportDispatcherTarget::Q3SpotLightSetOuterAngle
        | PpcImportDispatcherTarget::Q3SpotLightGetFallOff
        | PpcImportDispatcherTarget::Q3SpotLightSetFallOff
        | PpcImportDispatcherTarget::Q3SpotLightGetData
        | PpcImportDispatcherTarget::Q3SpotLightSetData => {
            unreachable!("QuickDraw 3D scene imports return through dispatch_q3_scene_import_fast")
        }
        PpcImportDispatcherTarget::Q3ShaderGetUVTransform
        | PpcImportDispatcherTarget::Q3ShaderSetUVTransform
        | PpcImportDispatcherTarget::Q3ShaderGetUBoundary
        | PpcImportDispatcherTarget::Q3ShaderSetUBoundary
        | PpcImportDispatcherTarget::Q3ShaderGetVBoundary
        | PpcImportDispatcherTarget::Q3ShaderSetVBoundary => {
            unreachable!(
                "QuickDraw 3D shader imports return through dispatch_q3_shader_style_import_fast"
            )
        }
        PpcImportDispatcherTarget::Q3BackfacingStyleNew
        | PpcImportDispatcherTarget::Q3BackfacingStyleGet
        | PpcImportDispatcherTarget::Q3BackfacingStyleSet
        | PpcImportDispatcherTarget::Q3InterpolationStyleNew
        | PpcImportDispatcherTarget::Q3InterpolationStyleGet
        | PpcImportDispatcherTarget::Q3InterpolationStyleSet
        | PpcImportDispatcherTarget::Q3FillStyleNew
        | PpcImportDispatcherTarget::Q3FillStyleGet
        | PpcImportDispatcherTarget::Q3FillStyleSet
        | PpcImportDispatcherTarget::Q3OrientationStyleNew
        | PpcImportDispatcherTarget::Q3OrientationStyleGet
        | PpcImportDispatcherTarget::Q3OrientationStyleSet => {
            unreachable!(
                "QuickDraw 3D style imports return through dispatch_q3_shader_style_import_fast"
            )
        }
        PpcImportDispatcherTarget::Q3TriMeshNew
        | PpcImportDispatcherTarget::Q3TriMeshGetData
        | PpcImportDispatcherTarget::Q3TriMeshSetData
        | PpcImportDispatcherTarget::Q3TriMeshEmptyData
        | PpcImportDispatcherTarget::Q3AttributeSetNew
        | PpcImportDispatcherTarget::Q3AttributeSetAdd
        | PpcImportDispatcherTarget::Q3AttributeSetGet
        | PpcImportDispatcherTarget::Q3AttributeSetClear
        | PpcImportDispatcherTarget::Q3AttributeSetContains
        | PpcImportDispatcherTarget::Q3AttributeSetGetNextAttributeType => {
            unreachable!("QuickDraw 3D geometry imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3StorageGetSize
        | PpcImportDispatcherTarget::Q3StorageGetData
        | PpcImportDispatcherTarget::Q3StorageSetData => {
            unreachable!("QuickDraw 3D storage imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3PixmapDrawContextNew
        | PpcImportDispatcherTarget::Q3MacDrawContextNew
        | PpcImportDispatcherTarget::Q3DrawContextGetPane => {
            unreachable!("QuickDraw 3D geometry imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3Vector3DNormalize
        | PpcImportDispatcherTarget::Q3Vector3DLength
        | PpcImportDispatcherTarget::Q3Vector2DNormalize
        | PpcImportDispatcherTarget::Q3Vector3DCross
        | PpcImportDispatcherTarget::Q3Point2DDistance
        | PpcImportDispatcherTarget::Q3Point3DDistance
        | PpcImportDispatcherTarget::Q3Point3DCrossProductTri
        | PpcImportDispatcherTarget::Q3BoundingBoxSetFromPoints3D
        | PpcImportDispatcherTarget::Q3Matrix3x3SetTranslate
        | PpcImportDispatcherTarget::Q3Matrix4x4SetIdentity
        | PpcImportDispatcherTarget::Q3Matrix4x4SetTranslate
        | PpcImportDispatcherTarget::Q3Matrix4x4SetScale
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateX
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateY
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateZ
        | PpcImportDispatcherTarget::Q3Matrix4x4SetRotateXyz
        | PpcImportDispatcherTarget::Q3Matrix4x4Multiply
        | PpcImportDispatcherTarget::Q3Matrix4x4Transpose
        | PpcImportDispatcherTarget::Q3Matrix4x4Invert
        | PpcImportDispatcherTarget::Q3Point3DTransform
        | PpcImportDispatcherTarget::Q3Point3DTo3DTransformArray
        | PpcImportDispatcherTarget::Q3Point3DTo4DTransformArray
        | PpcImportDispatcherTarget::Q3Vector3DTransform
        | PpcImportDispatcherTarget::Q3MatrixTransformNew
        | PpcImportDispatcherTarget::Q3MatrixTransformSet
        | PpcImportDispatcherTarget::Q3TransformGetMatrix => {
            unreachable!("QuickDraw 3D math imports return through dispatch_q3_math_import_fast")
        }
        PpcImportDispatcherTarget::Q3FileSetStorage
        | PpcImportDispatcherTarget::Q3FileOpenRead
        | PpcImportDispatcherTarget::Q3FileReadObject
        | PpcImportDispatcherTarget::Q3FileIsEndOfFile
        | PpcImportDispatcherTarget::Q3FileClose => {
            unreachable!("QuickDraw 3D file imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3GroupAddObject
        | PpcImportDispatcherTarget::Q3GroupAddObjectBefore
        | PpcImportDispatcherTarget::Q3GroupCountObjects
        | PpcImportDispatcherTarget::Q3GroupGetFirstPosition
        | PpcImportDispatcherTarget::Q3GroupGetNextPosition
        | PpcImportDispatcherTarget::Q3GroupGetFirstPositionOfType
        | PpcImportDispatcherTarget::Q3GroupGetPositionObject
        | PpcImportDispatcherTarget::Q3GroupRemovePosition
        | PpcImportDispatcherTarget::Q3LightGroupNew => {
            unreachable!("QuickDraw 3D group imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ViewSetRenderer
        | PpcImportDispatcherTarget::Q3ViewGetRenderer
        | PpcImportDispatcherTarget::Q3ViewSetLightGroup
        | PpcImportDispatcherTarget::Q3ViewGetLightGroup
        | PpcImportDispatcherTarget::Q3ViewSetDrawContext
        | PpcImportDispatcherTarget::Q3ViewGetDrawContext
        | PpcImportDispatcherTarget::Q3ViewSetCamera
        | PpcImportDispatcherTarget::Q3ViewGetCamera
        | PpcImportDispatcherTarget::Q3ViewGetWorldToFrustumMatrixState
        | PpcImportDispatcherTarget::Q3ViewGetFrustumToWindowMatrixState
        | PpcImportDispatcherTarget::Q3ViewStartRendering
        | PpcImportDispatcherTarget::Q3ViewEndRendering
        | PpcImportDispatcherTarget::Q3ViewStartBoundingBox
        | PpcImportDispatcherTarget::Q3ViewEndBoundingBox
        | PpcImportDispatcherTarget::Q3ViewStartBoundingSphere
        | PpcImportDispatcherTarget::Q3ViewEndBoundingSphere
        | PpcImportDispatcherTarget::Q3ViewCancel => {
            unreachable!("QuickDraw 3D view imports return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3ShaderSubmit | PpcImportDispatcherTarget::Q3StyleSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3BackfacingStyleSubmit
        | PpcImportDispatcherTarget::Q3InterpolationStyleSubmit
        | PpcImportDispatcherTarget::Q3FillStyleSubmit
        | PpcImportDispatcherTarget::Q3OrientationStyleSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3FogStyleSubmit
        | PpcImportDispatcherTarget::Q3TriMeshSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3MatrixTransformSubmit
        | PpcImportDispatcherTarget::Q3ResetTransformSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::Q3PushSubmit
        | PpcImportDispatcherTarget::Q3PopSubmit
        | PpcImportDispatcherTarget::Q3ObjectSubmit => {
            unreachable!("QuickDraw 3D submissions return through typed dispatch")
        }
        PpcImportDispatcherTarget::QADeviceGetFirstEngine => {
            Some(PpcImportAction::Return(PPC_QA_ENGINE))
        }
        PpcImportDispatcherTarget::QADeviceGetNextEngine => Some(PpcImportAction::Return(0)),
        PpcImportDispatcherTarget::QAEngineGestalt => Some(PpcImportAction::Return(
            qd3d::ppc_qa_engine_gestalt(cpu, memory),
        )),
        PpcImportDispatcherTarget::CloseComponent => Some(PpcImportAction::Return(ppc_i16_result(
            ppc_close_component(cpu, quicktime),
        ))),
        PpcImportDispatcherTarget::NewRoutineDescriptor
        | PpcImportDispatcherTarget::NewIOCompletionUPP
        | PpcImportDispatcherTarget::DisposeIOCompletionUPP
        | PpcImportDispatcherTarget::NewControlUserPaneDrawUPP
        | PpcImportDispatcherTarget::DisposeControlUserPaneDrawUPP
        | PpcImportDispatcherTarget::NewAEEventHandlerUPP
        | PpcImportDispatcherTarget::DisposeAEEventHandlerUPP
        | PpcImportDispatcherTarget::NewFatRoutineDescriptor
        | PpcImportDispatcherTarget::DisposeRoutineDescriptor
        | PpcImportDispatcherTarget::CallUniversalProc
        | PpcImportDispatcherTarget::CallOSTrapUniversalProc => {
            unreachable!("mixed mode imports return through dispatch_mixed_mode_import")
        }
        PpcImportDispatcherTarget::NGetTrapAddress
        | PpcImportDispatcherTarget::GetToolTrapAddress
        | PpcImportDispatcherTarget::GetOSTrapAddress => {
            let trap_word = cpu.gpr[3] as u16;
            let toolbox = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::GetToolTrapAddress
            ) || (matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::NGetTrapAddress
            ) && cpu.gpr[4] != 0);
            Some(PpcImportAction::Return(
                ppc_logical_trap_address(memory, trap_word, toolbox).unwrap_or(0),
            ))
        }
        PpcImportDispatcherTarget::SetToolTrapAddress
        | PpcImportDispatcherTarget::SetOSTrapAddress
        | PpcImportDispatcherTarget::NSetTrapAddress => {
            let toolbox = matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::SetToolTrapAddress
            ) || (matches!(
                binding.dispatcher_target,
                PpcImportDispatcherTarget::NSetTrapAddress
            ) && cpu.gpr[5] != 0);
            Some(
                if ppc_set_logical_trap_address(memory, cpu.gpr[4] as u16, toolbox, cpu.gpr[3]) {
                    PpcImportAction::ReturnPreserve
                } else {
                    PpcImportAction::Halt
                },
            )
        }
        PpcImportDispatcherTarget::LegacyMemoryUtility(operation) => {
            ppc_dispatch_legacy_memory_utility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            )
        }
        PpcImportDispatcherTarget::LegacyControl(_) => {
            unreachable!("control imports return through dispatch_control_import")
        }
        PpcImportDispatcherTarget::LegacyWindow(_) => {
            unreachable!("window imports return through dispatch_window_import")
        }
        PpcImportDispatcherTarget::AppleEventCompatibility(_) => {
            unreachable!("Apple Event imports return through dispatch_apple_event_import")
        }
        PpcImportDispatcherTarget::DialogCompatibility(_) => {
            unreachable!("dialog imports return through dispatch_dialog_import")
        }
        PpcImportDispatcherTarget::QuickDrawCompatibility(operation) => {
            Some(dispatch_quickdraw::ppc_dispatch_quickdraw_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                vfs_resources,
                *current_resource_refnum,
                gworlds,
                *current_gworld,
                *current_gdevice,
                screen_clut,
                color_manager_clut,
                *quickdraw_fore_color,
                quickdraw_fore_indices.get(current_gworld).copied(),
                *quickdraw_back_color,
                toolbox_startup,
            ))
        }
        PpcImportDispatcherTarget::SystemCompatibility(operation) => {
            Some(dispatch_system::ppc_dispatch_system_compatibility(
                operation,
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
                launched_app_path,
            ))
        }
        PpcImportDispatcherTarget::AppleTalkCompatibility(operation) => Some(
            dispatch_appletalk::ppc_dispatch_appletalk_compatibility(operation, cpu, memory),
        ),
        PpcImportDispatcherTarget::PrintingCompatibility(operation) => Some(
            dispatch_printing::ppc_dispatch_printing_compatibility(operation),
        ),
        PpcImportDispatcherTarget::SlotCompatibility => {
            Some(ppc_dispatch_slot_compatibility(binding, cpu, memory))
        }
        PpcImportDispatcherTarget::StandardFileCompatibility(_) => {
            unreachable!("standard file imports return through dispatch_standard_file_import")
        }
        PpcImportDispatcherTarget::SysBeep
        | PpcImportDispatcherTarget::SndSoundManagerVersion
        | PpcImportDispatcherTarget::UnsignedFixedMulDiv
        | PpcImportDispatcherTarget::GetSoundOutputInfo
        | PpcImportDispatcherTarget::GetCompressionInfo
        | PpcImportDispatcherTarget::GetSoundVol
        | PpcImportDispatcherTarget::SetSoundVol
        | PpcImportDispatcherTarget::GetDefaultOutputVolume
        | PpcImportDispatcherTarget::SetDefaultOutputVolume
        | PpcImportDispatcherTarget::SndNewChannel
        | PpcImportDispatcherTarget::SndDisposeChannel
        | PpcImportDispatcherTarget::SndPlay
        | PpcImportDispatcherTarget::SndChannelStatus
        | PpcImportDispatcherTarget::SndGetInfo
        | PpcImportDispatcherTarget::SndSetInfo
        | PpcImportDispatcherTarget::ParseSndHeader
        | PpcImportDispatcherTarget::SndDoCommand
        | PpcImportDispatcherTarget::SndDoImmediate
        | PpcImportDispatcherTarget::SndPlayDoubleBuffer
        | PpcImportDispatcherTarget::SndStartFilePlay
        | PpcImportDispatcherTarget::SndPauseFilePlay
        | PpcImportDispatcherTarget::SndStopFilePlay
        | PpcImportDispatcherTarget::GetSoundHeaderOffset
        | PpcImportDispatcherTarget::SoundInputCompatibility(_) => {
            unreachable!("sound imports return through dispatch_sound_import")
        }
        PpcImportDispatcherTarget::SpeechCompatibility(operation) => {
            Some(ppc_dispatch_speech_compatibility(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::QuickTimeCompatibility(operation) => Some(
            dispatch_quicktime_compatibility(operation, cpu, memory, quicktime),
        ),
        PpcImportDispatcherTarget::InputSprocketCompatibility(_) => {
            unreachable!(
                "input sprocket compatibility imports return through dispatch_inputsprocket_import"
            )
        }
        PpcImportDispatcherTarget::MathCompatibility(operation) => {
            Some(ppc_dispatch_math_compatibility(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::Math64(operation) => {
            Some(ppc_dispatch_math64(operation, cpu, memory))
        }
        PpcImportDispatcherTarget::StdCCompatibility(_) => {
            unreachable!("stdc imports return through dispatch_stdc_import")
        }
        PpcImportDispatcherTarget::ObjectSupportCompatibility => {
            Some(ppc_dispatch_object_support_compatibility(
                cpu,
                process_memory_manager,
                memory,
                heap_cursor,
                heap_limit,
                last_mem_error,
                handles,
            ))
        }
        PpcImportDispatcherTarget::GlideSstQueryBoards => {
            // 3Dfx Glide 2.4 Reference Manual, grSstQueryBoards: the routine
            // returns FXFALSE when it detects no Voodoo Graphics subsystem.
            // Systemless exposes the generic QuickDraw 3D Accelerator path,
            // not a fabricated 3Dfx board, so leave hwConfig untouched.
            Some(PpcImportAction::Return(0))
        }
        PpcImportDispatcherTarget::FlushEvents
        | PpcImportDispatcherTarget::SetEventMask
        | PpcImportDispatcherTarget::GetNextEvent(_)
        | PpcImportDispatcherTarget::GetOSEvent
        | PpcImportDispatcherTarget::EventAvail
        | PpcImportDispatcherTarget::OSEventAvail
        | PpcImportDispatcherTarget::PostEvent
        | PpcImportDispatcherTarget::Button
        | PpcImportDispatcherTarget::StillDown
        | PpcImportDispatcherTarget::WaitMouseUp
        | PpcImportDispatcherTarget::GetKeys
        | PpcImportDispatcherTarget::GetMouse => {
            unreachable!("event imports return through dispatch_event_import")
        }
        PpcImportDispatcherTarget::LMGetMenuList
        | PpcImportDispatcherTarget::LMSetMenuHook
        | PpcImportDispatcherTarget::LMGetMenuFlash
        | PpcImportDispatcherTarget::LMGetPaintWhite
        | PpcImportDispatcherTarget::LMGetSysMap
        | PpcImportDispatcherTarget::LMGetCurApRefNum
        | PpcImportDispatcherTarget::GetVCBQHdr
        | PpcImportDispatcherTarget::LMGetSysEvtMask
        | PpcImportDispatcherTarget::LMSetSysEvtMask
        | PpcImportDispatcherTarget::LMGetDefltStack
        | PpcImportDispatcherTarget::LMGetCurStackBase
        | PpcImportDispatcherTarget::LMSetPaintWhite
        | PpcImportDispatcherTarget::LMSetResumeProc
        | PpcImportDispatcherTarget::LMSetACount
        | PpcImportDispatcherTarget::LMSetANumber
        | PpcImportDispatcherTarget::LMSetDlgFont
        | PpcImportDispatcherTarget::SetMenuFlash
        | PpcImportDispatcherTarget::GetGrayRgn
        | PpcImportDispatcherTarget::LMSetGrayRgn
        | PpcImportDispatcherTarget::LMGetUTableBase
        | PpcImportDispatcherTarget::LMGetCurDirStore
        | PpcImportDispatcherTarget::LMSetCurDirStore
        | PpcImportDispatcherTarget::LMGetSFSaveDisk
        | PpcImportDispatcherTarget::LMSetSFSaveDisk
        | PpcImportDispatcherTarget::LMGetRndSeed
        | PpcImportDispatcherTarget::LMSetRndSeed
        | PpcImportDispatcherTarget::SetCurrentA5
        | PpcImportDispatcherTarget::SetA5
        | PpcImportDispatcherTarget::LMGetCurrentA5 => {
            unreachable!("low-memory imports return through dispatch_low_memory_import")
        }
        PpcImportDispatcherTarget::TickCount
        | PpcImportDispatcherTarget::GetDateTime
        | PpcImportDispatcherTarget::ReadDateTime
        | PpcImportDispatcherTarget::ReadLocation
        | PpcImportDispatcherTarget::GetTime
        | PpcImportDispatcherTarget::Delay
        | PpcImportDispatcherTarget::GetDblTime
        | PpcImportDispatcherTarget::LMGetTime
        | PpcImportDispatcherTarget::SecondsToDate
        | PpcImportDispatcherTarget::Microseconds
        | PpcImportDispatcherTarget::AbsoluteToNanoseconds => {
            unreachable!("time imports return through dispatch_time_import")
        }
        PpcImportDispatcherTarget::ReturnError(error) => {
            Some(PpcImportAction::Return(ppc_i16_result(error)))
        }
        PpcImportDispatcherTarget::ReturnNoErr => Some(PpcImportAction::Return(0)),
        PpcImportDispatcherTarget::ReturnOne => Some(PpcImportAction::Return(1)),
        PpcImportDispatcherTarget::NoOpPreserve => Some(PpcImportAction::ReturnPreserve),
        PpcImportDispatcherTarget::ExitToShell => Some(PpcImportAction::Halt),
        PpcImportDispatcherTarget::UnresolvedWeak | PpcImportDispatcherTarget::Unsupported => None,
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

fn ppc_dispatch_slot_compatibility(
    _binding: &PpcImportBinding,
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
) -> PpcImportAction {
    if cpu.gpr[3] != 0 {
        let _ = memory.write_u32_be(cpu.gpr[3], 0);
    }
    PpcImportAction::Return(ppc_i16_result(PPC_SM_NO_MORE_SRSRCS_ERR))
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

#[allow(clippy::too_many_arguments)]
fn ppc_dispatch_object_support_compatibility(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
) -> PpcImportAction {
    let result_ptr = cpu.gpr[8];
    if result_ptr == 0 || !ppc_memory_can_write_bytes(memory, result_ptr, 8) {
        return PpcImportAction::Return(ppc_i16_result(PPC_PARAM_ERR));
    }
    let mut data = Vec::with_capacity(12);
    data.extend_from_slice(&cpu.gpr[3].to_be_bytes());
    data.extend_from_slice(&cpu.gpr[5].to_be_bytes());
    data.extend_from_slice(&cpu.gpr[6].to_be_bytes());
    let result = ppc_create_process_owned_ae_desc(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        None,
        result_ptr,
        u32::from_be_bytes(*b"obj "),
        &data,
    );
    PpcImportAction::Return(ppc_i16_result(result))
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

pub(crate) fn ppc_rgb555_to_rgb16(pixel: u16) -> [u16; 3] {
    fn component(value: u16) -> u16 {
        (((u32::from(value) * 65_535) + 15) / 31) as u16
    }
    [
        component((pixel >> 10) & 0x1f),
        component((pixel >> 5) & 0x1f),
        component(pixel & 0x1f),
    ]
}

pub(crate) fn ppc_rgb555_to_clut_index(pixel: u16, clut: &[[u16; 3]; 256]) -> u8 {
    let [red, green, blue] = ppc_rgb555_to_rgb16(pixel & 0x7fff);
    pict::closest_clut_index(red, green, blue, clut)
}

fn ppc_draw_raw_quilt_frame(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    data: &[u8],
    pict_bounds: (i16, i16, i16, i16),
    dst_rect: (i16, i16, i16, i16),
    clut: &[[u16; 3]; 256],
    zero_is_opaque: bool,
) -> bool {
    if data.len() < 24 || data[10..24].iter().any(|byte| *byte != 0) {
        return false;
    }
    let (src_top, src_left, src_bottom, src_right) = pict_bounds;
    let src_width = i32::from(src_right) - i32::from(src_left);
    let src_height = i32::from(src_bottom) - i32::from(src_top);
    if src_width <= 0 || src_height <= 0 {
        return false;
    }
    let raw = &data[24..];
    let Ok(src_height_usize) = usize::try_from(src_height) else {
        return false;
    };
    if src_height_usize == 0 || raw.len() % src_height_usize != 0 {
        return false;
    }
    let src_row_bytes = raw.len() / src_height_usize;
    let Ok(src_width_usize) = usize::try_from(src_width) else {
        return false;
    };
    if src_row_bytes < src_width_usize {
        return false;
    }
    let (dst_top, dst_left, dst_bottom, dst_right) = dst_rect;
    let dst_width = i32::from(dst_right) - i32::from(dst_left);
    let dst_height = i32::from(dst_bottom) - i32::from(dst_top);
    if dst_width <= 0 || dst_height <= 0 {
        return false;
    }
    let copy_top = i32::from(dst_top).max(0).min(front_buffer.height as i32);
    let copy_bottom = i32::from(dst_bottom).max(0).min(front_buffer.height as i32);
    let copy_left = i32::from(dst_left).max(0).min(front_buffer.width as i32);
    let copy_right = i32::from(dst_right).max(0).min(front_buffer.width as i32);
    if copy_bottom <= copy_top || copy_right <= copy_left {
        return false;
    }

    for y in copy_top..copy_bottom {
        let src_y =
            ((y - i32::from(dst_top)) * src_height / dst_height).clamp(0, src_height - 1) as usize;
        for x in copy_left..copy_right {
            let src_x = ((x - i32::from(dst_left)) * src_width / dst_width).clamp(0, src_width - 1)
                as usize;
            let Some(mut color_index) = raw
                .get(src_y.saturating_mul(src_row_bytes).saturating_add(src_x))
                .copied()
            else {
                continue;
            };
            if zero_is_opaque && color_index == 0 {
                color_index = pict::closest_clut_index(0, 0, 0, clut);
            }
            if front_buffer.depth == 8 {
                let Some(dst_addr) = front_buffer
                    .base_addr
                    .checked_add((y as u32).saturating_mul(front_buffer.row_bytes))
                    .and_then(|row| row.checked_add(x as u32))
                else {
                    continue;
                };
                let _ = memory.write_u8(dst_addr, color_index);
            } else if front_buffer.depth == 16 {
                let [red, green, blue] = clut[color_index as usize];
                let pixel = ppc_q3_rgb555((
                    f32::from(red) / 65_535.0,
                    f32::from(green) / 65_535.0,
                    f32::from(blue) / 65_535.0,
                ));
                let _ = ppc_q3_write_software_pixel(memory, front_buffer, (x, y), pixel);
            } else {
                return false;
            }
        }
    }
    true
}

pub(super) fn ppc_draw_pict_bytes_to_16bpp(
    memory: &mut PpcSectionMem,
    front_buffer: PpcFrontBuffer,
    data: &[u8],
    dst_rect: (i16, i16, i16, i16),
    clut: &[[u16; 3]; 256],
    device_ct_seed: u32,
    quilt_zero_is_opaque: bool,
) -> bool {
    if front_buffer.base_addr == 0 || front_buffer.width == 0 || front_buffer.height == 0 {
        return false;
    }
    let Some((pict_offset, bounds)) = ppc_qt_pict_record_offset_and_bounds(data) else {
        return false;
    };
    let (dst_top, dst_left, dst_bottom, dst_right) = dst_rect;
    if dst_bottom <= dst_top || dst_right <= dst_left {
        return false;
    }

    let Ok(width) = u16::try_from(front_buffer.width) else {
        return false;
    };
    let Ok(height) = u16::try_from(front_buffer.height) else {
        return false;
    };
    if pict_offset == 0
        && ppc_draw_raw_quilt_frame(
            memory,
            front_buffer,
            data,
            bounds,
            dst_rect,
            clut,
            quilt_zero_is_opaque,
        )
    {
        return true;
    }
    if matches!(front_buffer.depth, 1 | 2 | 4 | 8) {
        let Some(minimum_row_bytes) = front_buffer
            .width
            .checked_mul(front_buffer.depth)
            .and_then(|bits| bits.checked_add(7))
            .map(|bits| bits / 8)
        else {
            return false;
        };
        if front_buffer.row_bytes < minimum_row_bytes {
            return false;
        }
        let row_bytes = front_buffer.row_bytes;
        let Some(buffer_len) = row_bytes.checked_mul(front_buffer.height) else {
            return false;
        };
        let Some(buffer_len_usize) = usize::try_from(buffer_len).ok() else {
            return false;
        };
        let pict_base = 0x0001_0000u32;
        let Some(pict_end) = pict_base.checked_add(u32::try_from(data.len()).unwrap_or(u32::MAX))
        else {
            return false;
        };
        let screen_base = (pict_end.saturating_add(0x0fff)) & !0x0fffu32;
        let Some(screen_end) = screen_base.checked_add(buffer_len) else {
            return false;
        };
        let Some(ram_size) = usize::try_from(screen_end.saturating_add(0x1000)).ok() else {
            return false;
        };
        if ram_size > 128 * 1024 * 1024 {
            return false;
        }

        let mut indexed = vec![0u8; buffer_len_usize];
        for y in 0..front_buffer.height {
            let Some(src_addr) = front_buffer
                .base_addr
                .checked_add(y.saturating_mul(front_buffer.row_bytes))
            else {
                return false;
            };
            let Some(dst_offset) = usize::try_from(y.saturating_mul(row_bytes)).ok() else {
                return false;
            };
            let Some(row_len) = usize::try_from(row_bytes).ok() else {
                return false;
            };
            if memory
                .read_bytes_into(src_addr, &mut indexed[dst_offset..dst_offset + row_len])
                .is_none()
            {
                return false;
            }
        }

        // Color QuickDraw matches colors only against pixel values the
        // destination PixMap can represent. A short 1/2/4-bit CTable is
        // overlaid on the logical 8-bit table, so hide that inherited tail
        // from the PICT renderer; otherwise a color can match (for example)
        // index 42 and then be truncated to the low one or two packed bits.
        // This is the same depth-limiting rule used by the 68K DrawPicture
        // path. Imaging With QuickDraw (1994), pp. 4-81--4-83 and 7-44--7-45.
        let packed_clut = matches!(front_buffer.depth, 1 | 2 | 4).then(|| {
            let mut packed_clut = *clut;
            let entry_count = 1usize << front_buffer.depth;
            let terminal = packed_clut[entry_count - 1];
            packed_clut[entry_count..].fill(terminal);
            packed_clut
        });
        let draw_clut = packed_clut.as_ref().unwrap_or(clut);

        let mut bus = MacMemoryBus::new(ram_size);
        bus.write_bytes(pict_base, data);
        bus.write_bytes(screen_base, &indexed);
        bus.begin_uncapped_write_probe();
        let (rendered, _) = pict::draw_picture(
            &mut bus,
            pict_base + u32::try_from(pict_offset).unwrap_or(0),
            dst_top,
            dst_left,
            dst_bottom,
            dst_right,
            (
                screen_base,
                row_bytes,
                width,
                height,
                front_buffer.depth as u16,
            ),
            draw_clut,
            device_ct_seed,
            None,
        );
        if !rendered {
            return false;
        }

        return ppc_commit_picture_writes(memory, &mut bus, screen_base, front_buffer, buffer_len);
    }

    if front_buffer.depth != 16 || front_buffer.row_bytes < front_buffer.width.saturating_mul(2) {
        return false;
    }
    let row_bytes = front_buffer.row_bytes;
    let Some(buffer_len) = row_bytes.checked_mul(front_buffer.height) else {
        return false;
    };
    let Some(buffer_len_usize) = usize::try_from(buffer_len).ok() else {
        return false;
    };
    let pict_base = 0x0001_0000u32;
    let Some(pict_end) = pict_base.checked_add(u32::try_from(data.len()).unwrap_or(u32::MAX))
    else {
        return false;
    };
    let screen_base = (pict_end.saturating_add(0x0fff)) & !0x0fffu32;
    let Some(screen_end) = screen_base.checked_add(buffer_len) else {
        return false;
    };
    let Some(ram_size) = usize::try_from(screen_end.saturating_add(0x1000)).ok() else {
        return false;
    };
    if ram_size > 128 * 1024 * 1024 {
        return false;
    }

    let mut direct = vec![0u8; buffer_len_usize];
    for y in 0..front_buffer.height {
        let Some(src_addr) = front_buffer
            .base_addr
            .checked_add(y.saturating_mul(front_buffer.row_bytes))
        else {
            return false;
        };
        let Some(dst_offset) = usize::try_from(y.saturating_mul(row_bytes)).ok() else {
            return false;
        };
        let Some(row_len) = usize::try_from(row_bytes).ok() else {
            return false;
        };
        if memory
            .read_bytes_into(src_addr, &mut direct[dst_offset..dst_offset + row_len])
            .is_none()
        {
            return false;
        }
    }

    let mut bus = MacMemoryBus::new(ram_size);
    bus.write_bytes(pict_base, data);
    bus.write_bytes(screen_base, &direct);
    bus.begin_uncapped_write_probe();
    let (rendered, _) = pict::draw_picture(
        &mut bus,
        pict_base + u32::try_from(pict_offset).unwrap_or(0),
        dst_top,
        dst_left,
        dst_bottom,
        dst_right,
        (screen_base, row_bytes, width, height, 16),
        clut,
        device_ct_seed,
        None,
    );
    if !rendered {
        return false;
    }

    ppc_commit_picture_writes(memory, &mut bus, screen_base, front_buffer, buffer_len)
}

fn ppc_commit_picture_writes(
    memory: &mut PpcSectionMem,
    bus: &mut MacMemoryBus,
    screen_base: u32,
    front_buffer: PpcFrontBuffer,
    buffer_len: u32,
) -> bool {
    // DrawPicture scales the picture into dstRect; its drawing operations can
    // extend beyond the picture frame. Copy the actual writes, not a rectangle
    // or a logical-pixel diff. Imaging With QuickDraw (1994), pp. 7-44--7-45.
    for range in bus.finish_write_probe_ranges() {
        let start = range.start.max(screen_base);
        let end = range.end.min(screen_base + buffer_len);
        if start < end {
            let bytes = bus.read_bytes(start, (end - start) as usize);
            if memory
                .write_bytes(front_buffer.base_addr + start - screen_base, &bytes)
                .is_none()
            {
                return false;
            }
        }
    }
    true
}

fn ppc_gestalt(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    toolbox_startup: &PpcToolboxStartupState,
) -> i16 {
    let selector = cpu.gpr[3];
    let response_ptr = cpu.gpr[4];
    if response_ptr == 0 {
        return PPC_PARAM_ERR;
    }

    let Some((response, err)) = ppc_gestalt_response(selector).or_else(|| {
        toolbox_startup
            .gestalt_values
            .get(&selector)
            .copied()
            .map(|value| (value, PPC_NO_ERR))
    }) else {
        if ppc_hle_trace_enabled() {
            eprintln!(
                "[PPC-TRACE] Gestalt({:?}) -> gestaltUndefSelectorErr",
                ppc_res_type_text(selector)
            );
        }
        if memory.write_u32_be(response_ptr, 0).is_none() {
            return PPC_PARAM_ERR;
        }
        return PPC_GESTALT_UNDEF_SELECTOR_ERR;
    };
    if memory.write_u32_be(response_ptr, response).is_none() {
        return PPC_PARAM_ERR;
    }
    if ppc_hle_trace_enabled() {
        eprintln!(
            "[PPC-TRACE] Gestalt({:?}) -> ${response:08X} err={err} lr=${:08X}",
            ppc_res_type_text(selector),
            cpu.lr
        );
    }
    err
}

fn ppc_gestalt_response(selector: u32) -> Option<(u32, i16)> {
    match &selector.to_be_bytes() {
        b"vers" => Some((0x0001, PPC_NO_ERR)),
        b"sysv" => Some((u32::from(POWERPC_SYSTEM_VERSION_BCD), PPC_NO_ERR)),
        b"cbon" => Some((u32::from(POWERPC_CARBON_VERSION_BCD), PPC_NO_ERR)),
        b"ostt" => Some((crate::trap::dispatch::OS_TRAP_TABLE_BASE, PPC_NO_ERR)),
        b"tbtt" => Some((crate::trap::dispatch::TOOLBOX_TRAP_TABLE_BASE, PPC_NO_ERR)),
        b"evnt" => Some((0x0001, PPC_NO_ERR)),
        b"cput" => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.native_cpu_type,
            PPC_NO_ERR,
        )),
        b"sysa" => REFERENCE_POWERPC_EXECUTION_CAPABILITIES
            .system_architecture
            .map(|architecture| (architecture, PPC_NO_ERR)),
        b"proc" => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.processor_type,
            PPC_NO_ERR,
        )),
        b"mach" => Some((
            u32::from(REFERENCE_MACHINE_PROFILE.gestalt_machine_type),
            PPC_NO_ERR,
        )),
        // Match the 68K toolbox profile: System 7 Color QuickDraw 1.3.
        // Reporting only the original 32-Bit QuickDraw release makes native
        // PPC applications select obsolete monochrome-GWorld fallbacks.
        b"qd  " => Some((0x0230, PPC_NO_ERR)),
        b"qdrw" => Some((0x000F, PPC_NO_ERR)),
        b"ram " => Some((REFERENCE_MACHINE_PROFILE.ram_size_bytes, PPC_NO_ERR)),
        // With virtual memory disabled, logical and physical RAM are equal.
        b"lram" => Some((REFERENCE_MACHINE_PROFILE.ram_size_bytes, PPC_NO_ERR)),
        b"fpu " => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.fpu_type,
            PPC_NO_ERR,
        )),
        b"mmu " => Some((
            REFERENCE_POWERPC_EXECUTION_CAPABILITIES.mmu_type,
            PPC_NO_ERR,
        )),
        b"snd " => Some((0x1CFB, PPC_NO_ERR)),
        b"tmgr" => Some((2, PPC_NO_ERR)),
        // Thread Manager (1999), p. 19: bits 0 and 2 advertise the manager
        // and its PowerPC ThreadsLib. Exact stack-size matching (bit 1)
        // remains unavailable.
        b"thds" => Some((0b101, PPC_NO_ERR)),
        b"dplv" => Some((0x0002_0006, PPC_NO_ERR)),
        b"dply" => Some((0x0000_0007, PPC_NO_ERR)),
        b"alis" => Some((1, PPC_NO_ERR)),
        b"fs  " => Some(((1 << 0) | (1 << 1), PPC_NO_ERR)),
        b"fold" => Some((1, PPC_NO_ERR)),
        b"qtim" => Some((PPC_QUICKTIME_VERSION, PPC_NO_ERR)),
        b"drag" => Some((0, PPC_NO_ERR)),
        b"os  " => Some((0x00FF, PPC_NO_ERR)),
        b"powr" => Some((0, PPC_NO_ERR)),
        b"appr" => Some((1, PPC_NO_ERR)),
        b"addr" => Some((0b111, PPC_NO_ERR)),
        b"sdev" => Some((0, PPC_NO_ERR)),
        b"stdf" => Some((1, PPC_NO_ERR)),
        b"help" => Some((1, PPC_NO_ERR)),
        b"vm  " => Some((0, PPC_NO_ERR)),
        b"cfrg" => Some((1, PPC_NO_ERR)),
        b"mixd" => Some((1, PPC_NO_ERR)),
        b"qd3d" => Some((1, PPC_NO_ERR)),
        b"q3v " => Some((PPC_QD3D_VERSION, PPC_NO_ERR)),
        b"a/ux" => Some((0, PPC_GESTALT_UNDEF_SELECTOR_ERR)),
        _ => None,
    }
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

pub(super) fn minimal_pict_bytes() -> [u8; 12] {
    [
        0x00, 0x0c, // picSize
        0x00, 0x00, 0x00, 0x00, // top, left
        0x00, 0x01, 0x00, 0x01, // bottom, right
        0x00, 0xff, // opEndPic
    ]
}

fn ppc_get_pict_info(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let _pic_handle = cpu.gpr[3];
    let pict_info_ptr = cpu.gpr[4];
    if pict_info_ptr == 0 || !ppc_memory_can_write_bytes(memory, pict_info_ptr, PPC_PICT_INFO_SIZE)
    {
        return PPC_PARAM_ERR;
    }
    for offset in 0..PPC_PICT_INFO_SIZE {
        if memory.write_u8(pict_info_ptr + offset, 0).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    PPC_NO_ERR
}

fn ppc_new_pixmap(
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    current_gdevice: u32,
) -> u32 {
    // Inside Macintosh: Imaging With QuickDraw (1994), pp. 4-85--4-86:
    // NewPixMap clones the current device PixMap except for its ColorTable.
    // It allocates that handle but deliberately leaves the table uninitialized;
    // the application must install a table that describes its pixels.
    let source_pixmap = memory
        .read_u32_be(current_gdevice)
        .filter(|device| *device != 0)
        .and_then(|device| memory.read_u32_be(device.checked_add(22)?))
        .filter(|handle| *handle != 0)
        .and_then(|handle| memory.read_u32_be(handle))
        .filter(|pixmap| *pixmap != 0)
        .unwrap_or(PPC_MAIN_PIXMAP);
    let Some(mut pixmap_bytes) = ppc_memory_read_bytes(memory, source_pixmap, PPC_PIXMAP_SIZE)
    else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    let ctable_bytes = [0; 8];
    let ctable_size = u32::try_from(ctable_bytes.len()).unwrap_or(u32::MAX);
    if !ppc_heap_can_alloc_sequence(
        memory,
        *heap_cursor,
        heap_limit,
        &[4, ctable_size, 4, PPC_PIXMAP_SIZE],
    ) {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    let ctable_handle = ppc_process_alloc_handle_with_bytes(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        &ctable_bytes,
    );
    let pixmap_handle = ppc_process_alloc_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        last_mem_error,
        handles,
        PPC_PIXMAP_SIZE,
        true,
    );
    if ctable_handle == 0 || pixmap_handle == 0 {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return 0;
    }
    pixmap_bytes[0..4].copy_from_slice(&0u32.to_be_bytes());
    pixmap_bytes[22..26].copy_from_slice(&0x0048_0000u32.to_be_bytes());
    pixmap_bytes[26..30].copy_from_slice(&0x0048_0000u32.to_be_bytes());
    pixmap_bytes[42..46].copy_from_slice(&ctable_handle.to_be_bytes());
    let Some(pixmap) = memory.read_u32_be(pixmap_handle) else {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    };
    if memory.write_bytes(pixmap, &pixmap_bytes).is_none() {
        *last_mem_error = PPC_PARAM_ERR;
        return 0;
    }
    *last_mem_error = PPC_NO_ERR;
    pixmap_handle
}

fn ppc_dispose_pixmap(
    pixmap_handle: u32,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    indexed_screen_ctables: &mut HashMap<u32, u32>,
) {
    if pixmap_handle == 0 {
        return;
    }
    // The same reference specifies that DisposePixMap owns both the PixMap
    // record and its pmTable handle. Read pmTable before invalidating pm.
    let ctable_handle = memory
        .read_u32_be(pixmap_handle)
        .filter(|pixmap| *pixmap != 0)
        .and_then(|pixmap| memory.read_u32_be(pixmap + 42))
        .filter(|handle| *handle != 0)
        .or_else(|| indexed_screen_ctables.remove(&pixmap_handle))
        .unwrap_or(0);
    if ctable_handle != 0 {
        let _ = ppc_dispose_process_native_handle(
            process_memory_manager,
            memory,
            heap_cursor,
            heap_limit,
            last_mem_error,
            handles,
            ctable_handle,
        );
        indexed_screen_ctables.retain(|_, handle| *handle != ctable_handle);
    }
    indexed_screen_ctables.remove(&pixmap_handle);
    let _ = ppc_dispose_process_native_handle(
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        pixmap_handle,
    );
}

fn ppc_text_width(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    text_font: i16,
    text_size: i16,
    text_face: u8,
) -> u32 {
    let text_ptr = cpu.gpr[3];
    let first_byte = cpu.gpr[4];
    let byte_count = cpu.gpr[5];
    let mut bytes = Vec::with_capacity(byte_count.min(i16::MAX as u32) as usize);
    for offset in 0..byte_count {
        let Some(addr) = text_ptr
            .checked_add(first_byte)
            .and_then(|base| base.checked_add(offset))
        else {
            break;
        };
        bytes.push(memory.read_u8(addr).unwrap_or(0));
    }
    ppc_text_width_bytes(text_font, text_size, text_face, &bytes).max(0) as u32
}

fn ppc_get_font_info(memory: &mut PpcSectionMem, info_ptr: u32, text_font: i16, text_size: i16) {
    if info_ptr == 0 || !ppc_memory_can_write_bytes(memory, info_ptr, 8) {
        return;
    }
    // Inside Macintosh Volume I (1985), p. I-173: FontInfo contains the
    // current port font's ascent, descent, maximum advance, and leading.
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let metrics = face.metrics;
    let _ = memory.write_u16_be(
        info_ptr,
        ppc_scale_font_value(i32::from(metrics.ascent), numerator, denominator) as u16,
    );
    let _ = memory.write_u16_be(
        info_ptr + 2,
        ppc_scale_font_value(i32::from(metrics.descent), numerator, denominator) as u16,
    );
    let _ = memory.write_u16_be(
        info_ptr + 4,
        ppc_scale_font_value(i32::from(metrics.wid_max), numerator, denominator) as u16,
    );
    let _ = memory.write_u16_be(
        info_ptr + 6,
        ppc_scale_font_value(i32::from(metrics.leading), numerator, denominator) as u16,
    );
}

fn ppc_font_metrics(memory: &mut PpcSectionMem, metrics_ptr: u32, text_font: i16, text_size: i16) {
    if metrics_ptr == 0 || !ppc_memory_can_write_bytes(memory, metrics_ptr, 20) {
        return;
    }
    // Inside Macintosh: Text (1993), pp. 4-54--4-55: FMetricRec stores
    // ascent, descent, leading, and maximum width as Fixed values, followed
    // by a handle to the global width table. Systemless does not model that
    // table, matching the 68k HLE by returning NIL for its handle.
    let (face, numerator, denominator) = get_font_face_scale_ratio(text_font, text_size);
    let metrics = face.metrics;
    let to_fixed = |value: i16| -> u32 { (i32::from(value) as u32) << 16 };
    let _ = memory.write_u32_be(
        metrics_ptr,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.ascent),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(
        metrics_ptr + 4,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.descent),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(
        metrics_ptr + 8,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.leading),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(
        metrics_ptr + 12,
        to_fixed(ppc_scale_font_value(
            i32::from(metrics.wid_max),
            numerator,
            denominator,
        )),
    );
    let _ = memory.write_u32_be(metrics_ptr + 16, 0);
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

fn ppc_logical_trap_address(
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

fn ppc_set_logical_trap_address(
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
