//! PowerPC loader, memory, and CFM layout constants.

use crate::machine_profile::REFERENCE_MACHINE_PROFILE;

pub const PPC_CODE_BASE: u32 = 0x0100_0000;
pub(crate) const PPC_CUR_RES_FILE_ADDR: u32 = 0x0000_0A5A;
pub const PPC_IMPORT_TVECTOR_BASE: u32 = 0x01e0_0000;
pub const PPC_IMPORT_TRAP_BASE: u32 = 0x01f0_0000;
pub(crate) const PPC_IMPORT_DATA_BASE: u32 = 0x01d0_0000;
pub(crate) const PPC_IMPORT_DATA_SIZE: usize = 0x1000;
pub(crate) const PPC_IMPORT_CTYPE_POINTER: u32 = PPC_IMPORT_DATA_BASE + 0x40c;
pub(crate) const PPC_IMPORT_MATH_PI: u32 = PPC_IMPORT_DATA_BASE + 0x410;
pub(crate) const PPC_IMPORT_MATH_FE_DFL_ENV: u32 = PPC_IMPORT_DATA_BASE + 0x418;
pub(crate) const PPC_IMPORT_STD_DBL_EPSILON: u32 = PPC_IMPORT_DATA_BASE + 0x420;
pub(crate) const PPC_IMPORT_STD_DBL_MAX: u32 = PPC_IMPORT_DATA_BASE + 0x428;
pub(crate) const PPC_IMPORT_STD_DBL_MIN: u32 = PPC_IMPORT_DATA_BASE + 0x430;
pub(crate) const PPC_IMPORT_STD_FLT_EPSILON: u32 = PPC_IMPORT_DATA_BASE + 0x438;
pub(crate) const PPC_IMPORT_STD_FLT_MAX: u32 = PPC_IMPORT_DATA_BASE + 0x43c;
pub(crate) const PPC_IMPORT_STD_FLT_MIN: u32 = PPC_IMPORT_DATA_BASE + 0x440;
pub(crate) const PPC_IMPORT_STD_ERRNO: u32 = PPC_IMPORT_DATA_BASE + 0x444;
pub(crate) const PPC_IMPORT_STD_MAC_OS_ERR: u32 = PPC_IMPORT_DATA_BASE + 0x448;
pub(crate) const PPC_IMPORT_CTYPE_TABLE: u32 = PPC_IMPORT_DATA_BASE + 0x500;
pub(crate) const PPC_IMPORT_CUR_AP_NAME: u32 = PPC_IMPORT_DATA_BASE + 0x900;
// Metrowerks StdCLib exposes `_iob` as the three 24-byte FILE records used
// for stdin, stdout, and stderr. Keep this zero-initialized storage in the
// stable import-data page. The actual FILE fields are private to StdCLib, so
// stream state lives in host-side metadata keyed by the guest FILE pointer.
pub const PPC_CFM_MAIN_STUB_BASE: u32 = 0x01d8_0000;
pub const PPC_MAIN_GWORLD: u32 = 0x02f0_0000;
pub const PPC_MAIN_GDEVICE: u32 = 0x02f0_0100;
pub const PPC_DSP_BACK_GWORLD: u32 = 0x4001_0000;
pub const PPC_DATA_BASE: u32 = 0x0200_0000;
pub const PPC_HEAP_BASE: u32 = 0x0300_0000;
pub(crate) const PPC_HEAP_ALIGNMENT: u32 = 16;
pub const PPC_STACK_TOP: u32 = 0x0500_0000;
pub const PPC_DEFAULT_STACK_SIZE: u32 = 64 * 1024;
pub const PPC_STACK_SIZE: u32 = PPC_DEFAULT_STACK_SIZE;
pub const PPC_STACK_BASE: u32 = PPC_STACK_TOP - PPC_DEFAULT_STACK_SIZE;
pub const PPC_HALT_PC: u32 = 0;
pub(crate) const PPC_LOW_MEMORY_SIZE: usize = 64 * 1024;
pub(crate) const PPC_CLASSIC_APP_MEMORY_BASE: u32 = PPC_LOW_MEMORY_SIZE as u32;
pub(crate) const PPC_THE_ZONE_ADDR: u32 = 0x0000_0118;
pub(crate) const PPC_SYS_ZONE_ADDR: u32 = 0x0000_02a6;
pub(crate) const PPC_APPL_ZONE_ADDR: u32 = 0x0000_02aa;
pub(crate) const PPC_MMU_32BIT_ADDR: u32 = 0x0000_0cb2;
pub(crate) const PPC_CLASSIC_APP_MEMORY_SIZE: usize = 0x000f_0000;
pub const PPC_MEM_FULL_ERR: i16 = -108;
pub(crate) const PPC_NIL_HANDLE_ERR: i16 = -109;
pub(crate) const PPC_MEM_WZ_ERR: i16 = -111;
#[cfg(test)]
pub(crate) const PPC_MEM_PUR_ERR: i16 = -112;
pub(crate) const PPC_C_DEPTH_ERR: i16 = -157;
pub const PPC_NO_ERR: i16 = 0;
pub(crate) const PPC_EVT_NOT_ENB: i16 = 1;
pub const PPC_EOF_ERR: i16 = -39;
pub const PPC_FN_OPN_ERR: i16 = -38;
pub const PPC_POS_ERR: i16 = -40;
pub const PPC_NSV_ERR: i16 = -35;
pub const PPC_FNF_ERR: i16 = -43;
pub const PPC_F_BSY_ERR: i16 = -47;
pub const PPC_DUP_FN_ERR: i16 = -48;
pub const PPC_RF_NUM_ERR: i16 = -51;
pub const PPC_WR_PERM_ERR: i16 = -61;
pub const PPC_PARAM_ERR: i16 = -50;
pub const PPC_C_RES_ERR: i16 = -156;
pub const PPC_DIR_NF_ERR: i16 = -120;
pub(crate) const PPC_PIXMAP_TOO_DEEP_ERR: i16 = -148;
pub(crate) const PPC_RGN_TOO_BIG_ERR: i16 = -500;
pub const PPC_OPEN_ERR: i16 = -23;
pub const PPC_BD_NAM_ERR: i16 = -37;
pub const PPC_PROC_NOT_FOUND_ERR: i16 = -600;
pub const PPC_NOT_ENOUGH_HARDWARE_ERR: i16 = -201;
pub(crate) const PPC_SM_NO_MORE_SRSRCS_ERR: i16 = -344;
pub(crate) const PPC_NO_MPP_ERR: i16 = -3102;
pub(crate) const PPC_ERR_AE_DESC_NOT_FOUND: i16 = -1701;
pub(crate) const PPC_ERR_AE_COERCION_FAIL: i16 = -1700;
pub(crate) const PPC_ERR_AE_EVENT_NOT_HANDLED: i16 = -1708;
pub(crate) const PPC_AE_BUFFER_IS_SMALL: i16 = -607;
pub(crate) const PPC_HM_HELP_MANAGER_NOT_INITED: i16 = -855;
pub(crate) const PPC_HIGH_LEVEL_EVENT_MASK: u16 = 0x0400;
pub(crate) const PPC_HIGH_LEVEL_EVENT: u16 = 23;
pub(crate) const PPC_CORE_EVENT_CLASS: u32 = u32::from_be_bytes(*b"aevt");
pub(crate) const PPC_OPEN_APPLICATION_EVENT: u32 = u32::from_be_bytes(*b"oapp");
pub(crate) const PPC_TYPE_WILDCARD: u32 = u32::from_be_bytes(*b"****");
pub(crate) const PPC_KEY_EVENT_CLASS_ATTR: u32 = u32::from_be_bytes(*b"evcl");
pub(crate) const PPC_KEY_EVENT_ID_ATTR: u32 = u32::from_be_bytes(*b"evid");
pub(crate) const PPC_TYPE_TYPE: u32 = u32::from_be_bytes(*b"type");
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

pub(crate) const BLR: u32 = 0x4e80_0020;
pub const PPC_FIRST_FILE_REF_NUM: i16 = 128;
pub(crate) const PPC_CLOSED_RESOURCE_REF_NUM: i16 = i16::MIN;
pub(crate) const PPC_PICT_INFO_SIZE: u32 = 104;
pub(crate) const PPC_CFM_MAIN_STUB_COUNT: u32 = 256;
pub const PPC_IMPORT_CAPACITY: u32 = 4096;
// The final mapped traps are reserved for guest-call and thread returns and
// the Dialog Manager's guest-callable standard filter procedure. They do not
// reduce the 4,096 application/CFM binding capacity.
pub(crate) const PPC_IMPORT_SLOT_COUNT: u32 = PPC_IMPORT_CAPACITY + 3;
pub(crate) const PPC_THREAD_RETURN_IMPORT_INDEX: u32 = PPC_IMPORT_CAPACITY + 1;
pub const PPC_THREAD_RETURN_PC: u32 = PPC_IMPORT_TRAP_BASE + PPC_THREAD_RETURN_IMPORT_INDEX * 4;
pub(crate) const PPC_GUEST_CALL_RETURN_IMPORT_INDEX: u32 = PPC_IMPORT_CAPACITY;
pub(crate) const PPC_STD_FILTER_IMPORT_INDEX: u32 = PPC_IMPORT_CAPACITY + 2;
pub const PPC_STD_FILTER_TVECTOR: u32 = PPC_IMPORT_TVECTOR_BASE + PPC_STD_FILTER_IMPORT_INDEX * 8;
pub const PPC_FIRST_CFM_CONNECTION_ID: u32 = 1;
pub const PPC_CFM_FIND_LIB: u32 = 2;
pub const PPC_CFM_LOAD_LIB: u32 = 1;
pub const PPC_CFM_LOAD_NEW_COPY: u32 = 5;
pub const PPC_CFM_POWERPC_ARCH: u32 = u32::from_be_bytes(*b"pwpc");
pub const PPC_CFM_ANY_ARCH: u32 = 0x3F3F_3F3F;
#[cfg(test)]
pub(crate) use crate::cfm::CFM_INIT_BLOCK_SIZE as PPC_CFM_INIT_BLOCK_SIZE;
pub const PPC_INITIAL_STACK_FRAME_SIZE: u32 = 64;
pub(crate) const PPC_INTERRUPT_RED_ZONE_SIZE: u32 = 224;
pub(crate) const PPC_PARAMETER_AREA_OFFSET: u32 = 24;
pub(crate) const PPC_LINKAGE_BACK_CHAIN_OFFSET: u32 = 0;
pub(crate) const PPC_LINKAGE_SAVED_CR_OFFSET: u32 = 4;
pub(crate) const PPC_LINKAGE_SAVED_LR_OFFSET: u32 = 8;
pub(crate) const PPC_LINKAGE_SAVED_RTOC_OFFSET: u32 = 20;
pub const PPC_GUEST_CALL_RETURN_PC: u32 =
    PPC_IMPORT_TRAP_BASE + PPC_GUEST_CALL_RETURN_IMPORT_INDEX * 4;
pub(crate) const PPC_INITIALIZERS_TRAMPOLINE_BASE: u32 = PPC_IMPORT_TRAP_BASE - 0x1_0000;
pub(crate) const PPC_APPLICATION_INIT_RETURN_PC: u32 = PPC_IMPORT_TRAP_BASE - 0x100;
pub(crate) const PPC_EXCEPTION_INFORMATION_SIZE: u32 = 24;
pub(crate) const PPC_EXCEPTION_MACHINE_INFORMATION_SIZE: u32 = 64;
pub(crate) const PPC_EXCEPTION_REGISTER_INFORMATION_SIZE: u32 = 256;
pub(crate) const PPC_EXCEPTION_FPU_INFORMATION_SIZE: u32 = 264;
pub(crate) const PPC_EXCEPTION_VECTOR_INFORMATION_SIZE: u32 = 532;
pub(crate) const PPC_EXCEPTION_MEMORY_INFORMATION_SIZE: u32 = 16;
// Universal Interfaces 3.4, MachineExceptions.h defines these exception and
// reference-kind values. Mac OS 8.1 reports status 5 for an unmapped access.
pub(crate) const PPC_ILLEGAL_INSTRUCTION_EXCEPTION: u32 = 1;
pub(crate) const PPC_TRAP_EXCEPTION: u32 = 2;
pub(crate) const PPC_UNMAPPED_MEMORY_EXCEPTION: u32 = 4;
pub(crate) const PPC_UNMAPPED_MEMORY_ERROR: u32 = 5;
pub(crate) const PPC_WRITE_REFERENCE: u32 = 0;
pub(crate) const PPC_READ_REFERENCE: u32 = 1;
pub(crate) const PPC_PROCINFO_CALLING_CONVENTION_MASK: u32 =
    crate::mixed_mode::proc_info::CALLING_CONVENTION_MASK;
pub(crate) const PPC_PROCINFO_PASCAL_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::PASCAL_STACK_BASED;
pub(crate) const PPC_PROCINFO_C_STACK_BASED: u32 = crate::mixed_mode::proc_info::C_STACK_BASED;
pub(crate) const PPC_PROCINFO_REGISTER_BASED: u32 = crate::mixed_mode::proc_info::REGISTER_BASED;
pub(crate) const PPC_PROCINFO_THINK_C_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::THINK_C_STACK_BASED;
pub(crate) const PPC_PROCINFO_D0_DISPATCHED_PASCAL_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::D0_DISPATCHED_PASCAL_STACK_BASED;
pub(crate) const PPC_PROCINFO_D0_DISPATCHED_C_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::D0_DISPATCHED_C_STACK_BASED;
pub(crate) const PPC_PROCINFO_D1_DISPATCHED_PASCAL_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::D1_DISPATCHED_PASCAL_STACK_BASED;
pub(crate) const PPC_PROCINFO_STACK_DISPATCHED_PASCAL_STACK_BASED: u32 =
    crate::mixed_mode::proc_info::STACK_DISPATCHED_PASCAL_STACK_BASED;
pub(crate) const PPC_PROCINFO_SPECIAL_CASE: u32 = crate::mixed_mode::proc_info::SPECIAL_CASE;
pub(crate) const PPC_PROCINFO_RESULT_SIZE_PHASE: u32 =
    crate::mixed_mode::proc_info::RESULT_SIZE_PHASE;
pub(crate) const PPC_PROCINFO_STACK_PARAMETER_PHASE: u32 =
    crate::mixed_mode::proc_info::STACK_PARAMETER_PHASE;
pub(crate) const PPC_PROCINFO_STACK_PARAMETER_WIDTH: u32 =
    crate::mixed_mode::proc_info::STACK_PARAMETER_WIDTH;
pub(crate) const PPC_PROCINFO_DISPATCHED_SELECTOR_SIZE_PHASE: u32 =
    crate::mixed_mode::proc_info::DISPATCHED_SELECTOR_SIZE_PHASE;
pub(crate) const PPC_PROCINFO_DISPATCHED_PARAMETER_PHASE: u32 =
    crate::mixed_mode::proc_info::DISPATCHED_PARAMETER_PHASE;
pub(crate) const PPC_PROCINFO_REGISTER_RESULT_LOCATION_PHASE: u32 =
    crate::mixed_mode::proc_info::REGISTER_RESULT_LOCATION_PHASE;
pub(crate) const PPC_PROCINFO_REGISTER_PARAMETER_PHASE: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_PHASE;
pub(crate) const PPC_PROCINFO_REGISTER_PARAMETER_WIDTH: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_WIDTH;
pub(crate) const PPC_PROCINFO_REGISTER_PARAMETER_SIZE_MASK: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_SIZE_MASK;
pub(crate) const PPC_PROCINFO_REGISTER_PARAMETER_WHICH_SHIFT: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_WHICH_SHIFT;
pub(crate) const PPC_PROCINFO_REGISTER_PARAMETER_WHICH_MASK: u32 =
    crate::mixed_mode::proc_info::REGISTER_PARAMETER_WHICH_MASK;
pub(crate) const PPC_PROCINFO_REGISTER_CCR_C: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_C;
pub(crate) const PPC_PROCINFO_REGISTER_CCR_V: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_V;
pub(crate) const PPC_PROCINFO_REGISTER_CCR_Z: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_Z;
pub(crate) const PPC_PROCINFO_REGISTER_CCR_N: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_N;
pub(crate) const PPC_PROCINFO_REGISTER_CCR_X: u32 = crate::mixed_mode::proc_info::REGISTER_CCR_X;
pub(crate) const PPC_CR0_LT_BIT: u8 = 0;
pub(crate) const PPC_CR0_EQ_BIT: u8 = 2;
pub(crate) const PPC_PROCINFO_SIZE_NONE: u32 = crate::mixed_mode::proc_info::SIZE_NONE;
pub(crate) const PPC_PROCINFO_SIZE_ONE: u32 = crate::mixed_mode::proc_info::SIZE_ONE;
pub(crate) const PPC_PROCINFO_SIZE_TWO: u32 = crate::mixed_mode::proc_info::SIZE_TWO;
pub(crate) const PPC_PROCINFO_SIZE_FOUR: u32 = crate::mixed_mode::proc_info::SIZE_FOUR;

pub(crate) const PPC_LIVE_TRAP_IMPORT_WORDS: &[u16] = &[0xA973, 0xA974, 0xA975, 0xA976, 0xA977];
pub(crate) const PPC_PROCINFO_MAX_STACK_PARAMETERS: usize =
    crate::mixed_mode::proc_info::MAX_STACK_PARAMETERS;
pub(crate) const PPC_PROCINFO_MAX_DISPATCHED_STACK_PARAMETERS: usize =
    crate::mixed_mode::proc_info::MAX_DISPATCHED_STACK_PARAMETERS;
pub(crate) const PPC_PROCINFO_MAX_REGISTER_PARAMETERS: usize =
    crate::mixed_mode::proc_info::MAX_REGISTER_PARAMETERS;
pub(crate) const PPC_CALL_UNIVERSAL_PROC_FIXED_WORD_PARAMETERS: usize = 2;
pub(crate) const PPC_CALL_UNIVERSAL_PROC_REGISTER_VARARGS: usize = 6;
pub(crate) const PPC_NATIVE_PARAMETER_GPR_COUNT: usize = 8;
pub(crate) const PPC_MAX_STACK_SIZE: u32 = PPC_STACK_TOP - PPC_HEAP_BASE;
pub const PPC_RAND_SEED_ADDR: u32 = 0x0000_0156;
pub(crate) const PPC_GRAY_RGN_ADDR: u32 = 0x0000_09ee;
pub(crate) const PPC_DEFAULT_DOUBLE_TIME_TICKS: u32 = 20;
pub(crate) const PPC_RES_CHANGED_ATTR: u16 = 0x0002;
pub(crate) const PPC_RES_PROTECTED_ATTR: u16 = 0x0008;
pub const PPC_RES_PROBLEM: i16 = -204;
pub(crate) const PPC_NO_SCRAP_ERR: i16 = -100;
pub(crate) const PPC_NO_TYPE_ERR: i16 = -102;
// The 'q3v ' Gestalt selector uses the 'vers' encoding for QuickDraw 3D 1.6.
// Apple, develop Issue 24 (Dec. 1995), p. 106; Macintosh Toolbox Essentials, p. 1-42.
pub(crate) const PPC_QD3D_VERSION: u32 = 0x0160_8000;
pub(crate) const PPC_MAIN_GDEVICE_RECORD: u32 = 0x02f0_0200;
pub(crate) const PPC_MAIN_GDEVICE_FLAGS: u16 =
    (1 << 0) | (1 << 10) | (1 << 11) | (1 << 12) | (1 << 13) | (1 << 15);
pub(crate) const PPC_MAIN_PIXMAP_HANDLE: u32 = 0x02f0_0300;
pub(crate) const PPC_MAIN_PIXMAP: u32 = 0x02f0_0400;
pub(crate) const PPC_GRAY_RGN_HANDLE: u32 = 0x02f0_0500;
pub(crate) const PPC_GRAY_RGN: u32 = 0x02f0_0600;
pub(crate) const PPC_MAIN_DCE_HANDLE: u32 = 0x02f0_0700;
pub(crate) const PPC_MAIN_DCE: u32 = 0x02f0_0800;
pub(crate) const PPC_MAIN_CTABLE_HANDLE: u32 = 0x02f0_0900;
pub(crate) const PPC_MAIN_CTABLE: u32 = 0x02f0_3000;
pub(crate) const PPC_MAIN_CTABLE_SIZE: u32 = 8 + 256 * 8;
pub(crate) const PPC_MAIN_VIS_RGN_HANDLE: u32 = 0x02f0_0b00;
pub(crate) const PPC_MAIN_VIS_RGN: u32 = 0x02f0_0c00;
pub(crate) const PPC_MAIN_CLIP_RGN_HANDLE: u32 = 0x02f0_0d00;
pub(crate) const PPC_MAIN_CLIP_RGN: u32 = 0x02f0_0e00;
pub(crate) const PPC_MAIN_GAMMA_TABLE: u32 = 0x02f0_1000;
pub(crate) const PPC_MAIN_GAMMA_TABLE_SIZE: u32 = 12 + 256;
pub(crate) const PPC_PORT_LIST_HANDLE: u32 = 0x02f0_1300;
pub(crate) const PPC_PORT_LIST: u32 = 0x02f0_1400;
pub(crate) const PPC_UNIT_TABLE: u32 = 0x02f0_1500;
pub(crate) const PPC_SOUND_DCE_HANDLE: u32 = 0x02f0_1600;
pub(crate) const PPC_SOUND_DCE: u32 = 0x02f0_1700;
pub(crate) const PPC_PORT_LIST_ADDR: u32 = 0x0d66;
pub(crate) const PPC_APPLICATION_ZONE: u32 = 0x02f0_2000;
pub(crate) const PPC_SYSTEM_ZONE: u32 = 0x02f0_2100;
pub(crate) const PPC_ZONE_STORAGE_SIZE: usize = 64;
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
pub const PPC_MAIN_SCREEN_BASE: u32 = 0x02a0_0000;
pub(crate) const PPC_MAIN_PIXEL_DEPTH: u32 = REFERENCE_MACHINE_PROFILE.screen_depth as u32;
pub const PPC_QD_TEXT_FONT_DEFAULT: i16 = 0;
pub const PPC_QD_TEXT_MODE_SRC_OR: i16 = 1;
pub const PPC_QD_TEXT_SIZE_SYSTEM: i16 = 0;
pub(crate) const PPC_QD_PEN_MODE_PAT_COPY: i16 = 8;
pub(crate) const PPC_CGRAF_PORT_PN_LOC_OFFSET: u32 = 48;
pub(crate) const PPC_CGRAF_PORT_PN_SIZE_OFFSET: u32 = 52;
pub(crate) const PPC_CGRAF_PORT_PN_MODE_OFFSET: u32 = 56;
pub(crate) const PPC_CGRAF_PORT_PN_VIS_OFFSET: u32 = 66;
pub(crate) const PPC_CGRAF_PORT_VIS_RGN_OFFSET: u32 = 24;
pub(crate) const PPC_CGRAF_PORT_CLIP_RGN_OFFSET: u32 = 28;
pub(crate) const PPC_CGRAF_PORT_GRAF_VARS_OFFSET: u32 = 8;
pub(crate) const PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET: u32 = 36;
pub(crate) const PPC_CGRAF_PORT_RGB_BK_COLOR_OFFSET: u32 = 42;
pub(crate) const PPC_CGRAF_PORT_BK_PIXPAT_OFFSET: u32 = 32;
pub(crate) const PPC_CGRAF_PORT_TX_FONT_OFFSET: u32 = 68;
pub(crate) const PPC_CGRAF_PORT_TX_FACE_OFFSET: u32 = 70;
pub(crate) const PPC_CGRAF_PORT_TX_MODE_OFFSET: u32 = 72;
pub(crate) const PPC_CGRAF_PORT_TX_SIZE_OFFSET: u32 = 74;
pub(crate) const PPC_CGRAF_PORT_RGN_SAVE_OFFSET: u32 = 96;
pub(crate) const PPC_CGRAF_PORT_PALETTE_HANDLE_OFFSET: u32 = 156;
pub(crate) const PPC_CGRAF_PORT_PALETTE_UPDATES_OFFSET: u32 = 160;
pub(crate) const PPC_GRAF_PORT_SIZE: u32 = 108;
pub(crate) const PPC_GDEVICE_SIZE: u32 = 62;
pub const PPC_PIXMAP_SIZE: u32 = 50;
pub(crate) const PPC_CGRAF_PORT_SIZE: u32 = 170;
pub(crate) const PPC_KEY_RETURN: u8 = 0x24;
pub(crate) const PPC_KEY_A: u8 = 0x00;
pub(crate) const PPC_KEY_B: u8 = 0x0b;
pub(crate) const PPC_KEY_G: u8 = 0x05;
pub(crate) const PPC_KEY_M: u8 = 0x2e;
pub(crate) const PPC_KEY_Q: u8 = 0x0c;
pub(crate) const PPC_KEY_Z: u8 = 0x06;
pub(crate) const PPC_KEY_1: u8 = 0x12;
pub(crate) const PPC_KEY_2: u8 = 0x13;
pub(crate) const PPC_KEY_EQUAL: u8 = 0x18;
pub(crate) const PPC_KEY_MINUS: u8 = 0x1b;
pub(crate) const PPC_KEY_COMMA: u8 = 0x2b;
pub(crate) const PPC_KEY_PERIOD: u8 = 0x2f;
pub(crate) const PPC_KEY_TAB: u8 = 0x30;
pub(crate) const PPC_KEY_SPACE: u8 = 0x31;
pub(crate) const PPC_KEY_ESCAPE: u8 = 0x35;
pub(crate) const PPC_KEY_COMMAND: u8 = 0x37;
pub(crate) const PPC_KEY_SHIFT: u8 = 0x38;
pub(crate) const PPC_KEY_OPTION: u8 = 0x3a;
pub(crate) const PPC_KEY_CONTROL: u8 = 0x3b;
pub(crate) const PPC_KEY_CONTROL_RIGHT: u8 = 0x3e;
pub(crate) const PPC_KEY_LEFT: u8 = 0x7b;
pub(crate) const PPC_KEY_RIGHT: u8 = 0x7c;
pub(crate) const PPC_KEY_DOWN: u8 = 0x7d;
pub(crate) const PPC_KEY_UP: u8 = 0x7e;
pub(crate) const PPC_KEY_NUMPAD_ADD: u8 = 0x45;
pub(crate) const PPC_KEY_NUMPAD_ENTER: u8 = 0x4c;
pub(crate) const PPC_KEY_NUMPAD_SUBTRACT: u8 = 0x4e;
pub(crate) const PPC_KEY_NUMPAD_LEFT: u8 = 0x56;
pub(crate) const PPC_KEY_NUMPAD_DOWN: u8 = 0x57;
pub(crate) const PPC_KEY_NUMPAD_RIGHT: u8 = 0x58;
pub(crate) const PPC_KEY_NUMPAD_UP: u8 = 0x5b;
pub(crate) const PPC_CONTROL_RECORD_SIZE: u32 = 296;
pub(crate) const PPC_CONTROL_NEXT_OFFSET: u32 = 0;
pub(crate) const PPC_CONTROL_OWNER_OFFSET: u32 = 4;
pub(crate) const PPC_CONTROL_RECT_OFFSET: u32 = 8;
pub(crate) const PPC_CONTROL_VISIBLE_OFFSET: u32 = 16;
pub(crate) const PPC_CONTROL_HILITE_OFFSET: u32 = 17;
pub(crate) const PPC_CONTROL_VALUE_OFFSET: u32 = 18;
pub(crate) const PPC_CONTROL_MIN_OFFSET: u32 = 20;
pub(crate) const PPC_CONTROL_MAX_OFFSET: u32 = 22;
pub(crate) const PPC_CONTROL_ACTION_OFFSET: u32 = 32;
pub(crate) const PPC_CONTROL_REF_CON_OFFSET: u32 = 36;
pub(crate) const PPC_CONTROL_TITLE_OFFSET: u32 = 40;
pub(crate) const PPC_KEY_MAP_SIZE: u32 = 16;
pub(crate) const PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 = 4;
pub(crate) const PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES: u64 = 7_296;
pub(crate) const PPC_MICROSECONDS_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 =
    PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD;
pub(crate) const PPC_MICROSECONDS_IDLE_POLL_EXTRA_CYCLES: u64 = PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES;
pub(crate) const PPC_BUTTON_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 =
    PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD;
pub(crate) const PPC_BUTTON_IDLE_POLL_EXTRA_CYCLES: u64 = PPC_GETKEYS_IDLE_POLL_EXTRA_CYCLES;
pub(crate) const PPC_TICK_COUNT_IDLE_POLL_FAST_FORWARD_THRESHOLD: u32 =
    PPC_GETKEYS_IDLE_POLL_FAST_FORWARD_THRESHOLD;
pub(crate) const PPC_FIXED_MAC_TIME: u32 = 3_786_912_000;
pub const PPC_MICROSECONDS_PER_TICK: u64 = 16_625;
pub const PPC_QT_GRAPHICS_IMPORTER: u32 = 0x4000_3000;
pub(crate) const PPC_QT_MOVIE: u32 = 0x4000_3010;
pub const PPC_QT_MOVIE_TASKS_PER_SECOND: u64 = 60;

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
