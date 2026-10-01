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
    REFERENCE_POWERPC_CPU_CLOCK_HZ, REFERENCE_POWERPC_EXECUTION_CAPABILITIES,
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
mod math_compatibility;
use math_compatibility::*;
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

mod constants;
pub use constants::*;
mod diagnostics;
mod dispatch_imports;
mod import_policies;
mod loaded_app;
pub use loaded_app::PpcLoadedApp;
use loaded_app::PpcLaunchPartitionStorage;
mod pict_rendering;
mod process_memory;
mod surface;
mod toolbox_startup;
mod traps;
pub(crate) use diagnostics::*;
#[cfg(test)]
pub(crate) use dispatch_math::{ppc_math_ceil, ppc_math_fmod};
pub(crate) use dispatch_toolbox::*;
pub(crate) use import_policies::*;
pub(in crate::systems::macintosh::loader::ppc) use import_policies::{
    ppc_dynamic_import_error, ppc_initial_import_error,
};
pub(crate) use loaded_app_callbacks::*;
pub(crate) use loaded_app_execution::*;
#[cfg(test)]
pub(crate) use loaded_app_execution::{ppc_virtual_microseconds, ppc_virtual_tick_count};
pub(crate) use pict_rendering::*;
pub(crate) use process_memory::*;
pub(crate) use surface::*;
pub use toolbox_startup::PpcToolboxStartupState;
pub(crate) use traps::*;
pub mod events;
pub mod files;
pub mod fixmath;
pub mod graphics;
mod classic_gl_agl;
mod classic_gl_framebuffer;
pub mod gworlds;
pub mod import_targets;
pub mod imports;
mod loaded_app_callbacks;
mod loaded_app_display;
mod loaded_app_execution;
pub(crate) use loaded_app_execution::ppc_random;
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

pub use math_compatibility::PpcMath64Operation;
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

pub type PpcHandleRecord = ProcessHandleRecord;
pub type PpcPtrRecord = ProcessPtrRecord;
pub type PpcHandleStateRecord = ProcessHandleStateRecord;

#[cfg(test)]
pub(crate) mod tests;
