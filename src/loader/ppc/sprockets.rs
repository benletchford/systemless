//! DrawSprocket and InputSprocket emulation and trace records.

use std::collections::HashMap;
use std::sync::OnceLock;

use ppc::PpcCpu;

use super::graphics::{
    ppc_read_rgb_color, PpcGWorldAllocationRecord, PpcGWorldRecord, PpcRgbColor, PPC_RGB_BLACK,
};
use super::imports::{PpcHleImportTraceEntry, PpcInputSnapshot};
use super::*;
use crate::mac_roman::decode_mac_roman;
use crate::process_context::ProcessNativeMemoryManager;
use ppc::PpcMemory;


pub const PPC_DSP_FREQUENCY_60HZ: u32 = 60 << 16;
pub const PPC_DSP_SCREEN_WIDTH: u32 = 640;
pub const PPC_DSP_SCREEN_HEIGHT: u32 = 480;
pub const PPC_DSP_LARGE_SCREEN_WIDTH: u32 = 800;
pub const PPC_DSP_LARGE_SCREEN_HEIGHT: u32 = 600;
pub const PPC_DSP_CONTEXT_OPTION_QD3D_ACCEL: u32 = 1 << 0;
pub const PPC_DSP_DEPTH_MASK_16: u32 = 1 << 4;
pub const PPC_MAIN_SCREEN_STORAGE_DEPTH: u32 = 16;
pub const PPC_DSP_ADVERTISED_PAGE_COUNT: u32 = 2;
pub const PPC_MAIN_GWORLD: u32 = 0x02f0_0000;
pub const PPC_DSP_BACK_GWORLD: u32 = 0x0501_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcDspContextAttributes {
    pub frequency: u32,
    pub width: u32,
    pub height: u32,
    pub context_options: u32,
    pub display_best_depth_mask: u32,
    pub back_buffer_best_depth_mask: u32,
    pub display_depth: u32,
    pub back_buffer_depth: u32,
    pub page_count: u32,
}

impl Default for PpcDspContextAttributes {
    fn default() -> Self {
        Self {
            frequency: PPC_DSP_FREQUENCY_60HZ,
            width: PPC_DSP_SCREEN_WIDTH,
            height: PPC_DSP_SCREEN_HEIGHT,
            context_options: PPC_DSP_CONTEXT_OPTION_QD3D_ACCEL,
            display_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
            back_buffer_best_depth_mask: PPC_DSP_DEPTH_MASK_16,
            display_depth: PPC_MAIN_SCREEN_STORAGE_DEPTH,
            back_buffer_depth: PPC_MAIN_SCREEN_STORAGE_DEPTH,
            page_count: PPC_DSP_ADVERTISED_PAGE_COUNT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcDspGammaFadeKind {
    Manual,
    In,
    Out,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcDspContextPlayState {
    Active,
    Paused,
    Inactive,
}

impl Default for PpcDspContextPlayState {
    fn default() -> Self {
        Self::Inactive
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcDrawSprocketState {
    pub started: bool,
    pub blanking_color: PpcRgbColor,
    pub reserved_context: Option<u32>,
    pub active_context: Option<u32>,
    pub context_state: PpcDspContextPlayState,
    pub context_attributes: PpcDspContextAttributes,
    pub front_buffer_gworld: u32,
    pub back_buffer_gworld: u32,
    pub last_fade_context: Option<u32>,
    pub last_fade_kind: Option<PpcDspGammaFadeKind>,
    pub last_fade_percent: Option<i32>,
    pub last_fade_zero_color: Option<PpcRgbColor>,
    pub fade_count: u32,
    pub last_user_select_display_id: Option<u32>,
    pub last_user_select_event_proc: Option<u32>,
    pub user_select_count: u32,
    pub last_swap_context: Option<u32>,
    pub swap_count: u32,
    pub vbl_proc: Option<u32>,
    pub vbl_refcon: Option<u32>,
    pub(crate) desktop_snapshot: Option<PpcDspDesktopSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcDspDesktopSnapshot {
    screen_bytes: Vec<u8>,
    gdevice_bytes: Vec<u8>,
    screen_clut: [[u16; 3]; 256],
    records: Vec<PpcGWorldRecord>,
    pixmaps: Vec<(u32, Vec<u8>)>,
}

impl Default for PpcDrawSprocketState {
    fn default() -> Self {
        Self {
            started: false,
            blanking_color: PPC_RGB_BLACK,
            reserved_context: None,
            active_context: None,
            context_state: PpcDspContextPlayState::Inactive,
            context_attributes: PpcDspContextAttributes::default(),
            front_buffer_gworld: PPC_MAIN_GWORLD,
            back_buffer_gworld: PPC_DSP_BACK_GWORLD,
            last_fade_context: None,
            last_fade_kind: None,
            last_fade_percent: None,
            last_fade_zero_color: None,
            fade_count: 0,
            last_user_select_display_id: None,
            last_user_select_event_proc: None,
            user_select_count: 0,
            last_swap_context: None,
            swap_count: 0,
            vbl_proc: None,
            vbl_refcon: None,
            desktop_snapshot: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcDrawSprocketTraceEntry {
    pub import_index: u32,
    pub pc: u32,
    pub action: String,
    pub result: i16,
    pub context: Option<u32>,
    pub requested_state: Option<String>,
    pub requested_frequency: Option<u32>,
    pub requested_width: Option<u32>,
    pub requested_height: Option<u32>,
    pub requested_context_options: Option<u32>,
    pub requested_display_depth_mask: Option<u32>,
    pub requested_back_buffer_depth_mask: Option<u32>,
    pub requested_display_depth: Option<u32>,
    pub requested_back_buffer_depth: Option<u32>,
    pub requested_page_count: Option<u32>,
    pub can_user_select: Option<bool>,
    pub fade_kind: Option<String>,
    pub fade_percent: Option<i32>,
    pub fade_zero_red: Option<u16>,
    pub fade_zero_green: Option<u16>,
    pub fade_zero_blue: Option<u16>,
    pub reserved_context: Option<u32>,
    pub active_context: Option<u32>,
    pub context_state: String,
    pub front_buffer_gworld: u32,
    pub back_buffer_gworld: u32,
    pub last_swap_context: Option<u32>,
    pub swap_count: u32,
    pub fade_count: u32,
    pub frequency: u32,
    pub width: u32,
    pub height: u32,
    pub context_options: u32,
    pub display_depth_mask: u32,
    pub back_buffer_depth_mask: u32,
    pub display_depth: u32,
    pub back_buffer_depth: u32,
    pub page_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpcInputSprocketActionBinding {
    AxisYaw,
    AxisPitch,
    AxisHorizontal,
    AxisVertical,
    AxisDirectional,
    ButtonLeft,
    ButtonRight,
    ButtonForward,
    ButtonBackward,
    ButtonCameraLeft,
    ButtonCameraRight,
    ButtonJump,
    ButtonFire,
    ButtonWeapon,
    ButtonPickup,
    ButtonJetUp,
    ButtonJetDown,
    ButtonPause,
    ButtonZoomIn,
    ButtonZoomOut,
    ButtonCameraMode,
    ButtonToggleMusic,
    ButtonToggleAmbientSound,
    ButtonVolumeUp,
    ButtonVolumeDown,
    ButtonToggleGps,
    ButtonQuit,
    ButtonConfirm,
    ButtonPrimary,
    DpadDirectional,
    DeltaYaw,
    DeltaPitch,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PpcInputSprocketState {
    pub initialized: bool,
    pub suspended: bool,
    pub keyboard_active: bool,
    pub mouse_active: bool,
    pub configure_count: u32,
    pub virtual_element_count: u32,
    pub last_virtual_need_count: u32,
    pub last_virtual_needs_ptr: u32,
    pub last_virtual_elements_out_ptr: u32,
}

impl Default for PpcInputSprocketState {
    fn default() -> Self {
        Self {
            initialized: false,
            suspended: false,
            keyboard_active: true,
            mouse_active: true,
            configure_count: 0,
            virtual_element_count: 0,
            last_virtual_need_count: 0,
            last_virtual_needs_ptr: 0,
            last_virtual_elements_out_ptr: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcInputSprocketVirtualElementRecord {
    pub element: u32,
    pub need_index: u32,
    pub need_source: u32,
    pub kind: u32,
    pub default_state: u32,
    pub action_binding: PpcInputSprocketActionBinding,
    pub need_name: String,
    pub need_record: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PpcInputSprocketVirtualElementDraft {
    pub(crate) need_index: u32,
    pub(crate) need_source: u32,
    pub(crate) kind: u32,
    pub(crate) default_state: u32,
    pub(crate) action_binding: PpcInputSprocketActionBinding,
    pub(crate) need_name: String,
    pub(crate) need_record: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PpcInputSprocketSimpleStateTraceEntry {
    pub import_index: u32,
    pub pc: u32,
    pub element: u32,
    pub state_ptr: u32,
    pub state: u32,
    pub kind: u32,
    pub kind_name: String,
    pub fallback_state: u32,
    pub need_name: String,
    pub action_binding: String,
    pub input: PpcInputSnapshot,
    pub input_sprocket: PpcInputSprocketState,
}

pub const PPC_DSP_CONTEXT_ALREADY_RESERVED_ERR: i16 = -30444;
pub const PPC_DSP_CONTEXT_NOT_RESERVED_ERR: i16 = -30445;
pub const PPC_DSP_CONTEXT_NOT_FOUND_ERR: i16 = -30446;
pub const PPC_DSP_BACK_PIXMAP_HANDLE: u32 = 0x0501_0300;
pub const PPC_DSP_BACK_PIXMAP: u32 = 0x0501_0400;
pub const PPC_DSP_BACK_VIS_RGN_HANDLE: u32 = 0x0501_0500;
pub const PPC_DSP_BACK_VIS_RGN: u32 = 0x0501_0600;
pub const PPC_DSP_BACK_CLIP_RGN_HANDLE: u32 = 0x0501_0700;
pub const PPC_DSP_BACK_CLIP_RGN: u32 = 0x0501_0800;
pub const PPC_DSP_BACK_SCREEN_BASE: u32 = 0x0502_0000;
/// Span available to the DrawSprocket back buffer above [`PPC_STACK_TOP`].
pub const PPC_DSP_BACK_SCREEN_SPAN: u32 = 0x0080_0000;
pub const PPC_DSP_CONTEXT_STATE_ACTIVE: u32 = 0;
pub const PPC_DSP_CONTEXT_STATE_PAUSED: u32 = 1;
pub const PPC_DSP_CONTEXT_STATE_INACTIVE: u32 = 2;
pub const PPC_DSP_CONTEXT: u32 = 0x0500_0000;
pub const PPC_DSP_CONTEXT_ATTRIBUTES_SIZE: u32 = 72;
pub const PPC_DSP_DEPTH_MASK_8: u32 = 1 << 3;
pub const PPC_DSP_DISPLAY_ID: u32 = 1;
pub const PPC_DSP_BUFFER_KIND_NORMAL: u32 = 0;
pub const PPC_DSP_MAX_REQUESTED_PAGE_COUNT: u32 = 2;
pub const PPC_ISP_DEVICE_COUNT: u32 = 2;
pub const PPC_ISP_KEYBOARD_DEVICE: u32 = 0x0500_2000;
pub const PPC_ISP_MOUSE_DEVICE: u32 = 0x0500_2010;
pub const PPC_ISP_KEYBOARD_ELEMENT: u32 = 0x0500_2020;
pub const PPC_ISP_MOUSE_X_ELEMENT: u32 = 0x0500_2030;
pub const PPC_ISP_MOUSE_Y_ELEMENT: u32 = 0x0500_2040;
pub const PPC_ISP_MOUSE_BUTTON_ELEMENT: u32 = 0x0500_2050;
pub const PPC_ISP_DEVICE_DEFINITION_SIZE: u32 = 92;
pub const PPC_ISP_ELEMENT_INFO_SIZE: u32 = 80;
pub const PPC_ISP_DEVICE_CLASS_KEYBOARD: u32 = u32::from_be_bytes(*b"keyd");
pub const PPC_ISP_DEVICE_CLASS_MOUSE: u32 = u32::from_be_bytes(*b"mous");
pub const PPC_ISP_ELEMENT_LABEL_NONE: u32 = u32::from_be_bytes(*b"none");
pub const PPC_ISP_ELEMENT_LABEL_CURSOR_X: u32 = u32::from_be_bytes(*b"curx");
pub const PPC_ISP_ELEMENT_LABEL_CURSOR_Y: u32 = u32::from_be_bytes(*b"cury");
pub const PPC_ISP_ELEMENT_LABEL_MOUSE_ONE: u32 = u32::from_be_bytes(*b"mou1");
pub const PPC_ISP_NEED_SIZE: u32 = 92;
pub const PPC_ISP_NEED_KIND_OFFSET: u32 = 68;
pub const PPC_ISP_VIRTUAL_ELEMENT_RECORD_SIZE: u32 = 16 + PPC_ISP_NEED_SIZE;
pub const PPC_ISP_ELEMENT_NEED_INDEX_OFFSET: u32 = 8;
pub const PPC_ISP_ELEMENT_NEED_SOURCE_OFFSET: u32 = 12;
pub const PPC_ISP_ELEMENT_NEED_RECORD_OFFSET: u32 = 16;
pub const PPC_ISP_ELEMENT_LIST_HEADER_SIZE: u32 = 8;
pub const PPC_ISP_ELEMENT_LIST_ENTRY_SIZE: u32 = 12;
pub const PPC_ISP_ELEMENT_LIST_CAPACITY: u32 = 64;
pub const PPC_ISP_ELEMENT_LIST_RECORD_SIZE: u32 = PPC_ISP_ELEMENT_LIST_HEADER_SIZE
    + PPC_ISP_ELEMENT_LIST_ENTRY_SIZE * PPC_ISP_ELEMENT_LIST_CAPACITY;
pub const PPC_ISP_ELEMENT_EVENT_SIZE: u32 = 20;
pub const PPC_ISP_ELEMENT_KIND_BUTTON: u32 = 0x6275_746e;
pub const PPC_ISP_ELEMENT_KIND_DPAD: u32 = 0x6470_6164;
pub const PPC_ISP_ELEMENT_KIND_AXIS: u32 = 0x6178_6973;
pub const PPC_ISP_ELEMENT_KIND_DELTA: u32 = 0x6465_6c74;
pub const PPC_ISP_AXIS_MIDDLE: u32 = 0x7fff_ffff;
pub const PPC_ISP_AXIS_LOW: u32 = 0;
pub const PPC_ISP_AXIS_HIGH: u32 = 0xffff_ffff;
pub const PPC_ISP_BUTTON_PRESSED: u32 = 1;
pub const PPC_ISP_DPAD_UP: u32 = 1 << 0;
pub const PPC_ISP_DPAD_RIGHT: u32 = 1 << 1;
pub const PPC_ISP_DPAD_DOWN: u32 = 1 << 2;
pub const PPC_ISP_DPAD_LEFT: u32 = 1 << 3;

static SPROCKET_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();
pub(crate) fn sprocket_trace_enabled() -> bool {
    *SPROCKET_TRACE_ENABLED.get_or_init(|| std::env::var_os("SYSTEMLESS_SPROCKET_TRACE").is_some())
}

pub(crate) fn format_rgb_opt(value: Option<PpcRgbColor>) -> String {
    value
        .map(|color| {
            format!(
                "${:04X}/${:04X}/${:04X}",
                color.red, color.green, color.blue
            )
        })
        .unwrap_or_else(|| "none".to_string())
}

pub(crate) fn dsp_gamma_kind_name(kind: PpcDspGammaFadeKind) -> &'static str {
    match kind {
        PpcDspGammaFadeKind::Manual => "manual",
        PpcDspGammaFadeKind::In => "in",
        PpcDspGammaFadeKind::Out => "out",
    }
}

pub(crate) fn dsp_context_play_state_name(state: PpcDspContextPlayState) -> &'static str {
    match state {
        PpcDspContextPlayState::Active => "active",
        PpcDspContextPlayState::Paused => "paused",
        PpcDspContextPlayState::Inactive => "inactive",
    }
}

pub(crate) fn format_dsp_last_fade(state: &PpcDrawSprocketState) -> String {
    match (state.last_fade_kind, state.last_fade_percent) {
        (Some(kind), Some(percent)) => format!(
            "{} context={} percent={} zero={}",
            dsp_gamma_kind_name(kind),
            format_hex_opt(state.last_fade_context),
            percent,
            format_rgb_opt(state.last_fade_zero_color)
        ),
        _ => "none".to_string(),
    }
}

pub(crate) fn isp_element_kind_name(kind: u32) -> &'static str {
    match kind {
        PPC_ISP_ELEMENT_KIND_AXIS => "axis",
        PPC_ISP_ELEMENT_KIND_BUTTON => "button",
        PPC_ISP_ELEMENT_KIND_DPAD => "dpad",
        PPC_ISP_ELEMENT_KIND_DELTA => "delta",
        _ => "unknown",
    }
}

pub(crate) fn isp_action_binding_name(binding: PpcInputSprocketActionBinding) -> &'static str {
    match binding {
        PpcInputSprocketActionBinding::AxisYaw => "axis/yaw",
        PpcInputSprocketActionBinding::AxisPitch => "axis/pitch",
        PpcInputSprocketActionBinding::AxisHorizontal => "axis/horizontal",
        PpcInputSprocketActionBinding::AxisVertical => "axis/vertical",
        PpcInputSprocketActionBinding::AxisDirectional => "axis/directional",
        PpcInputSprocketActionBinding::ButtonLeft => "button/left",
        PpcInputSprocketActionBinding::ButtonRight => "button/right",
        PpcInputSprocketActionBinding::ButtonForward => "button/forward",
        PpcInputSprocketActionBinding::ButtonBackward => "button/backward",
        PpcInputSprocketActionBinding::ButtonCameraLeft => "button/camera-left",
        PpcInputSprocketActionBinding::ButtonCameraRight => "button/camera-right",
        PpcInputSprocketActionBinding::ButtonJump => "button/jump",
        PpcInputSprocketActionBinding::ButtonFire => "button/fire",
        PpcInputSprocketActionBinding::ButtonWeapon => "button/weapon",
        PpcInputSprocketActionBinding::ButtonPickup => "button/pickup",
        PpcInputSprocketActionBinding::ButtonJetUp => "button/jet-up",
        PpcInputSprocketActionBinding::ButtonJetDown => "button/jet-down",
        PpcInputSprocketActionBinding::ButtonPause => "button/pause",
        PpcInputSprocketActionBinding::ButtonZoomIn => "button/zoom-in",
        PpcInputSprocketActionBinding::ButtonZoomOut => "button/zoom-out",
        PpcInputSprocketActionBinding::ButtonCameraMode => "button/camera-mode",
        PpcInputSprocketActionBinding::ButtonToggleMusic => "button/music-toggle",
        PpcInputSprocketActionBinding::ButtonToggleAmbientSound => "button/ambient-toggle",
        PpcInputSprocketActionBinding::ButtonVolumeUp => "button/volume-up",
        PpcInputSprocketActionBinding::ButtonVolumeDown => "button/volume-down",
        PpcInputSprocketActionBinding::ButtonToggleGps => "button/gps-toggle",
        PpcInputSprocketActionBinding::ButtonQuit => "button/quit",
        PpcInputSprocketActionBinding::ButtonConfirm => "button/confirm",
        PpcInputSprocketActionBinding::ButtonPrimary => "button/primary",
        PpcInputSprocketActionBinding::DpadDirectional => "dpad/directional",
        PpcInputSprocketActionBinding::DeltaYaw => "delta/yaw",
        PpcInputSprocketActionBinding::DeltaPitch => "delta/pitch",
        PpcInputSprocketActionBinding::Unknown => "unknown",
    }
}

pub(crate) fn format_isp_trace_name(name: &str) -> String {
    name.chars().flat_map(char::escape_default).collect()
}

pub(crate) fn format_isp_last_virtual_bindings(
    input_sprocket: &PpcInputSprocketState,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> String {
    let count = input_sprocket.last_virtual_need_count as usize;
    if count == 0 || virtual_elements.is_empty() {
        return "[]".to_string();
    }
    let start = virtual_elements.len().saturating_sub(count);
    let entries: Vec<String> = virtual_elements[start..]
        .iter()
        .map(|record| {
            format!(
                "#{} {} '{}'={}",
                record.need_index,
                isp_element_kind_name(record.kind),
                format_isp_trace_name(&record.need_name),
                isp_action_binding_name(record.action_binding)
            )
        })
        .collect();
    format!("[{}]", entries.join(","))
}

pub(crate) fn format_sprocket_action(action: &PpcImportAction) -> String {
    format_hle_import_action(action)
}

pub(crate) fn format_sprocket_trace(
    entry: &PpcHleImportTraceEntry,
    args: [u32; 6],
    action: &str,
    draw_sprocket: &PpcDrawSprocketState,
    input_sprocket: &PpcInputSprocketState,
    input_sprocket_virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> String {
    let state = if entry.library_name == "DrawSprocketLib" {
        format!(
            "dsp started={} reserved={} active={} state={} attrs={}x{} display_depth={} back_depth={} pages={} front={} back={} swaps={} last_swap={} fades={} last_fade={}",
            draw_sprocket.started,
            format_hex_opt(draw_sprocket.reserved_context),
            format_hex_opt(draw_sprocket.active_context),
            dsp_context_play_state_name(draw_sprocket.context_state),
            draw_sprocket.context_attributes.width,
            draw_sprocket.context_attributes.height,
            draw_sprocket.context_attributes.display_depth,
            draw_sprocket.context_attributes.back_buffer_depth,
            draw_sprocket.context_attributes.page_count,
            format_hex_opt(Some(draw_sprocket.front_buffer_gworld)),
            format_hex_opt(Some(draw_sprocket.back_buffer_gworld)),
            draw_sprocket.swap_count,
            format_hex_opt(draw_sprocket.last_swap_context),
            draw_sprocket.fade_count,
            format_dsp_last_fade(draw_sprocket)
        )
    } else if entry.symbol_name == "ISpElement_NewVirtualFromNeeds" {
        format!(
            "isp initialized={} suspended={} keyboard={} mouse={} virtuals={} last_need_count={} last_needs={} last_elements={} configure_count={} last_bindings={}",
            input_sprocket.initialized,
            input_sprocket.suspended,
            input_sprocket.keyboard_active,
            input_sprocket.mouse_active,
            input_sprocket.virtual_element_count,
            input_sprocket.last_virtual_need_count,
            format_hex_opt(Some(input_sprocket.last_virtual_needs_ptr)),
            format_hex_opt(Some(input_sprocket.last_virtual_elements_out_ptr)),
            input_sprocket.configure_count,
            format_isp_last_virtual_bindings(input_sprocket, input_sprocket_virtual_elements)
        )
    } else {
        format!(
            "isp initialized={} suspended={} keyboard={} mouse={} virtuals={} last_need_count={} last_needs={} last_elements={} configure_count={}",
            input_sprocket.initialized,
            input_sprocket.suspended,
            input_sprocket.keyboard_active,
            input_sprocket.mouse_active,
            input_sprocket.virtual_element_count,
            input_sprocket.last_virtual_need_count,
            format_hex_opt(Some(input_sprocket.last_virtual_needs_ptr)),
            format_hex_opt(Some(input_sprocket.last_virtual_elements_out_ptr)),
            input_sprocket.configure_count
        )
    };
    format!(
        "[SPROCKET-TRACE] {}:{} pc=${:08X} lr=${:08X} rtoc=${:08X} sp=${:08X} r3=${:08X} r4=${:08X} r5=${:08X} r6=${:08X} r7=${:08X} r8=${:08X} action={} {}",
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
        state
    )
}


pub(crate) fn ppc_read_dsp_context_attributes(
    memory: &mut PpcSectionMem,
    attributes: u32,
) -> Option<PpcDspContextAttributes> {
    Some(PpcDspContextAttributes {
        frequency: memory.read_u32_be(attributes)?,
        width: memory.read_u32_be(attributes + 4)?,
        height: memory.read_u32_be(attributes + 8)?,
        context_options: memory.read_u32_be(attributes + 28)?,
        back_buffer_best_depth_mask: memory.read_u32_be(attributes + 32)?,
        display_best_depth_mask: memory.read_u32_be(attributes + 36)?,
        back_buffer_depth: memory.read_u32_be(attributes + 40)?,
        display_depth: memory.read_u32_be(attributes + 44)?,
        page_count: memory.read_u32_be(attributes + 48)?,
    })
}

pub(crate) fn ppc_selected_dsp_context_attributes(
    requested: PpcDspContextAttributes,
) -> PpcDspContextAttributes {
    let mut selected = PpcDspContextAttributes::default();
    if !ppc_dsp_context_request_is_supported(requested) {
        return selected;
    }
    if requested.width != 0 {
        selected.width = requested.width;
    }
    if requested.height != 0 {
        selected.height = requested.height;
    }
    let display_depth = ppc_selected_dsp_depth(
        requested.display_best_depth_mask,
        requested.display_depth,
        selected.display_depth,
    );
    let back_buffer_depth = ppc_selected_dsp_depth(
        requested.back_buffer_best_depth_mask,
        requested.back_buffer_depth,
        display_depth,
    );
    selected.display_best_depth_mask = 1 << display_depth.trailing_zeros();
    selected.back_buffer_best_depth_mask = 1 << back_buffer_depth.trailing_zeros();
    selected.display_depth = display_depth;
    selected.back_buffer_depth = back_buffer_depth;
    if requested.page_count != 0 {
        selected.page_count = requested.page_count;
    }
    selected
}

pub(crate) fn ppc_selected_dsp_depth(depth_mask: u32, best_depth: u32, default_depth: u32) -> u32 {
    if matches!(best_depth, 8 | 16)
        && (depth_mask == 0 || depth_mask & (1 << best_depth.trailing_zeros()) != 0)
    {
        return best_depth;
    }
    if depth_mask & PPC_DSP_DEPTH_MASK_16 != 0 {
        16
    } else if depth_mask & PPC_DSP_DEPTH_MASK_8 != 0 {
        8
    } else {
        default_depth
    }
}

pub(crate) fn ppc_dsp_context_request_is_supported(requested: PpcDspContextAttributes) -> bool {
    let width = if requested.width == 0 {
        PPC_DSP_SCREEN_WIDTH
    } else {
        requested.width
    };
    let height = if requested.height == 0 {
        PPC_DSP_SCREEN_HEIGHT
    } else {
        requested.height
    };
    matches!(
        (width, height),
        (PPC_DSP_SCREEN_WIDTH, PPC_DSP_SCREEN_HEIGHT)
            | (PPC_DSP_LARGE_SCREEN_WIDTH, PPC_DSP_LARGE_SCREEN_HEIGHT)
    ) && width <= ppc_main_screen_width()
        && height <= ppc_main_screen_height()
        && ppc_dsp_depth_request_is_supported(
            requested.display_best_depth_mask,
            requested.display_depth,
        )
        && ppc_dsp_back_buffer_depth_request_is_supported(
            requested.back_buffer_best_depth_mask,
            requested.back_buffer_depth,
        )
        && (requested.page_count == 0 || requested.page_count <= PPC_DSP_MAX_REQUESTED_PAGE_COUNT)
}

pub(crate) fn ppc_dsp_depth_request_is_supported(depth_mask: u32, best_depth: u32) -> bool {
    const SUPPORTED_DEPTH_MASK: u32 = PPC_DSP_DEPTH_MASK_8 | PPC_DSP_DEPTH_MASK_16;
    (depth_mask == 0 || depth_mask & SUPPORTED_DEPTH_MASK != 0)
        && (best_depth == 0 || matches!(best_depth, 8 | 16))
}

pub(crate) fn ppc_dsp_back_buffer_depth_request_is_supported(depth_mask: u32, best_depth: u32) -> bool {
    if depth_mask == 1 && best_depth == 1 {
        return true;
    }
    ppc_dsp_depth_request_is_supported(depth_mask, best_depth)
}

pub(crate) fn ppc_read_optional_dsp_context_attributes(
    memory: &mut PpcSectionMem,
    attributes: u32,
) -> Option<PpcDspContextAttributes> {
    if attributes == 0 {
        Some(PpcDspContextAttributes::default())
    } else {
        ppc_read_dsp_context_attributes(memory, attributes)
    }
}

pub(crate) fn ppc_record_requested_dsp_context_attributes(
    memory: &mut PpcSectionMem,
    attributes: u32,
    draw_sprocket: &mut PpcDrawSprocketState,
) -> Option<()> {
    let requested = ppc_read_optional_dsp_context_attributes(memory, attributes)?;
    draw_sprocket.context_attributes = ppc_selected_dsp_context_attributes(requested);
    Some(())
}

pub(crate) fn ppc_write_dsp_context_attributes(
    memory: &mut PpcSectionMem,
    attributes: u32,
    context_attributes: PpcDspContextAttributes,
) -> Option<()> {
    for offset in 0..PPC_DSP_CONTEXT_ATTRIBUTES_SIZE {
        memory.write_u8(attributes + offset, 0)?;
    }
    memory.write_u32_be(attributes, context_attributes.frequency)?;
    memory.write_u32_be(attributes + 4, context_attributes.width)?;
    memory.write_u32_be(attributes + 8, context_attributes.height)?;
    memory.write_u32_be(attributes + 28, context_attributes.context_options)?;
    memory.write_u32_be(
        attributes + 32,
        context_attributes.back_buffer_best_depth_mask,
    )?;
    memory.write_u32_be(attributes + 36, context_attributes.display_best_depth_mask)?;
    memory.write_u32_be(attributes + 40, context_attributes.back_buffer_depth)?;
    memory.write_u32_be(attributes + 44, context_attributes.display_depth)?;
    memory.write_u32_be(attributes + 48, context_attributes.page_count)?;
    Some(())
}

pub(crate) fn ppc_dsp_context_get_buffer(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    buffer_gworld: u32,
    buffer_out_ptr: u32,
) -> i16 {
    if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
        return error;
    }
    if buffer_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, buffer_out_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    if memory.write_u32_be(buffer_out_ptr, buffer_gworld).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_context_error(context: u32) -> Option<i16> {
    if context == PPC_DSP_CONTEXT {
        None
    } else {
        Some(PPC_DSP_CONTEXT_NOT_FOUND_ERR)
    }
}

pub(crate) fn ppc_dsp_context_state_from_raw(state: u32) -> Option<PpcDspContextPlayState> {
    match state {
        PPC_DSP_CONTEXT_STATE_ACTIVE => Some(PpcDspContextPlayState::Active),
        PPC_DSP_CONTEXT_STATE_PAUSED => Some(PpcDspContextPlayState::Paused),
        PPC_DSP_CONTEXT_STATE_INACTIVE => Some(PpcDspContextPlayState::Inactive),
        _ => None,
    }
}

pub(crate) fn ppc_dsp_context_get_back_buffer(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    draw_sprocket: &PpcDrawSprocketState,
) -> i16 {
    if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
        return error;
    }
    let buffer_kind = cpu.gpr[4];
    let buffer_out_ptr = cpu.gpr[5];
    if buffer_kind != PPC_DSP_BUFFER_KIND_NORMAL {
        return PPC_PARAM_ERR;
    }
    ppc_dsp_context_get_buffer(
        cpu,
        memory,
        draw_sprocket.back_buffer_gworld,
        buffer_out_ptr,
    )
}

pub(crate) fn ppc_dsp_can_user_select_context(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let desired_attributes_ptr = cpu.gpr[3];
    let can_select_out_ptr = cpu.gpr[4];
    if can_select_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, can_select_out_ptr, 1) {
        return PPC_PARAM_ERR;
    }
    if desired_attributes_ptr != 0
        && ppc_read_dsp_context_attributes(memory, desired_attributes_ptr).is_none()
    {
        return PPC_PARAM_ERR;
    }
    if memory.write_u8(can_select_out_ptr, 0).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_get_first_context(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let display_id = cpu.gpr[3];
    let context_out_ptr = cpu.gpr[4];
    if context_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, context_out_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    // Inside Macintosh: Apple Game Sprockets Guide, "Summary of
    // DrawSprocket" (1996), lists DSpGetFirstContext(DisplayIDType,
    // DSpContextReference *) as the start of per-display context enumeration.
    if display_id != 0 && display_id != PPC_DSP_DISPLAY_ID {
        return PPC_DSP_CONTEXT_NOT_FOUND_ERR;
    }
    if memory
        .write_u32_be(context_out_ptr, PPC_DSP_CONTEXT)
        .is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_get_next_context(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let current_context = cpu.gpr[3];
    let context_out_ptr = cpu.gpr[4];
    if context_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, context_out_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    if let Some(error) = ppc_dsp_context_error(current_context) {
        return error;
    }
    // Systemless exposes one faithful software display context, so the first
    // valid context is also the end of the enumeration described above.
    PPC_DSP_CONTEXT_NOT_FOUND_ERR
}

pub(crate) fn ppc_dsp_process_event(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let event_was_processed_out_ptr = cpu.gpr[4];
    if event_was_processed_out_ptr == 0
        || !ppc_memory_can_write_bytes(memory, event_was_processed_out_ptr, 1)
    {
        return PPC_PARAM_ERR;
    }
    // Inside Macintosh: Apple Game Sprockets Guide, "Summary of
    // DrawSprocket" (1996), defines DSpProcessEvent(EventRecord *, Boolean *).
    // Systemless does not consume any host-only DrawSprocket window events.
    if memory.write_u8(event_was_processed_out_ptr, 0).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_find_best_context(
    memory: &mut PpcSectionMem,
    desired_attributes_ptr: u32,
    context_out_ptr: u32,
    draw_sprocket: &mut PpcDrawSprocketState,
) -> i16 {
    if context_out_ptr == 0 || !ppc_memory_can_write_bytes(memory, context_out_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    let Some(requested) = ppc_read_optional_dsp_context_attributes(memory, desired_attributes_ptr)
    else {
        return PPC_PARAM_ERR;
    };
    if sprocket_trace_enabled() {
        eprintln!(
            "[SPROCKET-TRACE] DSpFindBestContext requested attrs={}x{} display_mask=${:08X} back_mask=${:08X} display_best={} back_best={} pages={}",
            requested.width,
            requested.height,
            requested.display_best_depth_mask,
            requested.back_buffer_best_depth_mask,
            requested.display_depth,
            requested.back_buffer_depth,
            requested.page_count
        );
    }
    if !ppc_dsp_context_request_is_supported(requested) {
        return PPC_DSP_CONTEXT_NOT_FOUND_ERR;
    }
    if memory
        .write_u32_be(context_out_ptr, PPC_DSP_CONTEXT)
        .is_none()
    {
        return PPC_PARAM_ERR;
    }
    draw_sprocket.context_attributes = ppc_selected_dsp_context_attributes(requested);
    draw_sprocket.started = true;
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_user_select_context(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    draw_sprocket: &mut PpcDrawSprocketState,
) -> i16 {
    let result = ppc_dsp_find_best_context(memory, cpu.gpr[3], cpu.gpr[6], draw_sprocket);
    if result == PPC_NO_ERR {
        draw_sprocket.last_user_select_display_id = Some(cpu.gpr[4]);
        draw_sprocket.last_user_select_event_proc = Some(cpu.gpr[5]);
        draw_sprocket.user_select_count = draw_sprocket.user_select_count.saturating_add(1);
    }
    result
}

pub(crate) fn ppc_dsp_set_blanking_color(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    draw_sprocket: &mut PpcDrawSprocketState,
) -> i16 {
    // Inside Macintosh: Apple Game Sprockets Guide (1996),
    // "DSpSetBlankingColor": the RGBColor applies to the blanking window for
    // every display while any context is active.
    if cpu.gpr[3] == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(color) = ppc_read_rgb_color(memory, cpu.gpr[3]) else {
        return PPC_PARAM_ERR;
    };
    draw_sprocket.blanking_color = color;
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_get_mouse(cpu: &PpcCpu, memory: &mut PpcSectionMem, input: &PpcInputSnapshot) -> i16 {
    // DrawSprocket.h: DSpGetMouse reports a global QuickDraw Point.
    let out_global_point = cpu.gpr[3];
    if out_global_point == 0 || !ppc_memory_can_write_bytes(memory, out_global_point, 4) {
        return PPC_PARAM_ERR;
    }
    if memory
        .write_u16_be(out_global_point, input.mouse_v as u16)
        .is_none()
        || memory
            .write_u16_be(out_global_point + 2, input.mouse_h as u16)
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_find_context_from_point(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    // Inside Macintosh: Apple Game Sprockets Guide (1996),
    // "DSpFindContextFromPoint". PowerPC passes the four-byte Point value in r3.
    let global_v = (cpu.gpr[3] >> 16) as u16 as i16;
    let global_h = cpu.gpr[3] as u16 as i16;
    let out_context = cpu.gpr[4];
    if out_context == 0 || !ppc_memory_can_write_bytes(memory, out_context, 4) {
        return PPC_PARAM_ERR;
    }
    if global_v < 0
        || global_h < 0
        || global_v >= ppc_main_screen_height() as i16
        || global_h >= ppc_main_screen_width() as i16
    {
        return PPC_DSP_CONTEXT_NOT_FOUND_ERR;
    }
    if memory.write_u32_be(out_context, PPC_DSP_CONTEXT).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_context_global_to_local(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    // Systemless currently exposes one DrawSprocket context whose origin is
    // the main display origin, so global and context-local coordinates match.
    if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
        return error;
    }
    let point = cpu.gpr[4];
    if point == 0 || !ppc_memory_can_write_bytes(memory, point, 4) {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn ppc_dsp_alt_buffer_new(
    cpu: &PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    last_mem_error: &mut i16,
    handles: &mut Vec<PpcHandleRecord>,
    gworlds: &mut Vec<PpcGWorldRecord>,
    gworld_allocations: &mut HashMap<u32, PpcGWorldAllocationRecord>,
    current_gdevice: u32,
    draw_sprocket: &PpcDrawSprocketState,
) -> i16 {
    if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
        return error;
    }
    let out_alt_buffer = cpu.gpr[6];
    if out_alt_buffer == 0 || !ppc_memory_can_write_bytes(memory, out_alt_buffer, 4) {
        return PPC_PARAM_ERR;
    }
    let (width, height) = if cpu.gpr[5] == 0 {
        (
            draw_sprocket.context_attributes.width,
            draw_sprocket.context_attributes.height,
        )
    } else {
        let Some(width) = memory.read_u32_be(cpu.gpr[5]) else {
            return PPC_PARAM_ERR;
        };
        let Some(height) = memory.read_u32_be(cpu.gpr[5] + 4) else {
            return PPC_PARAM_ERR;
        };
        let Some(_options) = memory.read_u32_be(cpu.gpr[5] + 8) else {
            return PPC_PARAM_ERR;
        };
        (width, height)
    };
    if width == 0 || height == 0 || width > i16::MAX as u32 || height > i16::MAX as u32 {
        return PPC_PARAM_ERR;
    }

    let rect_ptr = ppc_process_heap_alloc(process_memory_manager, memory, heap_cursor, 8, true);
    if rect_ptr == 0
        || ppc_write_rect(memory, rect_ptr, 0, 0, height as i16, width as i16).is_none()
    {
        *last_mem_error = PPC_MEM_FULL_ERR;
        return PPC_MEM_FULL_ERR;
    }
    let mut world_cpu = cpu.clone();
    world_cpu.gpr[3] = out_alt_buffer;
    world_cpu.gpr[4] = draw_sprocket.context_attributes.display_depth;
    world_cpu.gpr[5] = rect_ptr;
    world_cpu.gpr[6] = 0;
    world_cpu.gpr[7] = current_gdevice;
    world_cpu.gpr[8] = 0;
    ppc_new_gworld(
        &mut world_cpu,
        process_memory_manager,
        memory,
        heap_cursor,
        heap_limit,
        last_mem_error,
        handles,
        gworlds,
        gworld_allocations,
        current_gdevice,
    )
}

pub(crate) fn ppc_dsp_alt_buffer_get_cgraf_ptr(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
) -> i16 {
    if cpu.gpr[4] != PPC_DSP_BUFFER_KIND_NORMAL
        || cpu.gpr[5] == 0
        || cpu.gpr[6] == 0
        || !ppc_memory_can_write_bytes(memory, cpu.gpr[5], 4)
        || !ppc_memory_can_write_bytes(memory, cpu.gpr[6], 4)
    {
        return PPC_PARAM_ERR;
    }
    let Some(world) = gworlds.iter().find(|world| world.port == cpu.gpr[3]) else {
        return PPC_PARAM_ERR;
    };
    if memory.write_u32_be(cpu.gpr[5], world.port).is_none()
        || memory.write_u32_be(cpu.gpr[6], world.gdevice).is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_context_reserve(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    draw_sprocket: &mut PpcDrawSprocketState,
    _gworlds: &mut [PpcGWorldRecord],
) -> i16 {
    let context = cpu.gpr[3];
    if let Some(error) = ppc_dsp_context_error(context) {
        return error;
    }
    if draw_sprocket.reserved_context.is_some() {
        return PPC_DSP_CONTEXT_ALREADY_RESERVED_ERR;
    }
    if cpu.gpr[4] != 0
        && ppc_record_requested_dsp_context_attributes(memory, cpu.gpr[4], draw_sprocket).is_none()
    {
        return PPC_PARAM_ERR;
    }
    // Reserving an inactive context must leave the desktop display mode and
    // its pixels alone. The switch occurs when SetState activates the context.
    draw_sprocket.back_buffer_gworld = if draw_sprocket.context_attributes.page_count == 1 {
        draw_sprocket.front_buffer_gworld
    } else {
        PPC_DSP_BACK_GWORLD
    };
    draw_sprocket.started = true;
    draw_sprocket.reserved_context = Some(context);
    draw_sprocket.active_context = None;
    draw_sprocket.context_state = PpcDspContextPlayState::Inactive;
    PPC_NO_ERR
}

pub(crate) fn ppc_configure_dsp_framebuffers(
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
    attributes: PpcDspContextAttributes,
) -> Option<()> {
    if !matches!(
        (attributes.width, attributes.height),
        (PPC_DSP_SCREEN_WIDTH, PPC_DSP_SCREEN_HEIGHT)
            | (PPC_DSP_LARGE_SCREEN_WIDTH, PPC_DSP_LARGE_SCREEN_HEIGHT)
    ) || attributes.width > ppc_main_screen_width()
        || attributes.height > ppc_main_screen_height()
        || attributes.display_depth != attributes.back_buffer_depth
        || !matches!(attributes.display_depth, 8 | 16)
    {
        return None;
    }
    let row_bytes = ppc_row_bytes(attributes.width, attributes.display_depth)?;

    // Activating a context switches the display and drawing buffers to the
    // selected depth. An inactive reservation leaves the desktop untouched.
    for record in gworlds.iter_mut().filter(|record| {
        record.base_addr == PPC_MAIN_SCREEN_BASE || record.base_addr == PPC_DSP_BACK_SCREEN_BASE
    }) {
        record.depth = attributes.display_depth;
        record.row_bytes = row_bytes;
        if matches!(record.port, PPC_MAIN_GWORLD | PPC_DSP_BACK_GWORLD) {
            record.width = attributes.width;
            record.height = attributes.height;
            ppc_write_pixmap(
                memory,
                record.pixmap,
                record.base_addr,
                row_bytes,
                0,
                0,
                attributes.height as i16,
                attributes.width as i16,
                attributes.display_depth,
            )?;
            if attributes.display_depth <= 8 {
                memory.write_u32_be(record.pixmap + 42, PPC_MAIN_CTABLE_HANDLE)?;
            }
        } else {
            // A screen-backed window keeps its local bounds through a display
            // switch. Only its shared pixel format changes while the context
            // is active.
            let ctable = if attributes.display_depth <= 8 {
                memory
                    .read_u32_be(record.pixmap + 42)
                    .filter(|ptr| *ptr != 0)
                    .unwrap_or(PPC_MAIN_CTABLE_HANDLE)
            } else {
                0
            };
            ppc_update_pixmap_depth(
                memory,
                record.pixmap,
                row_bytes,
                attributes.display_depth,
                ctable,
            )?;
        }
    }
    ppc_write_gdevice(
        memory,
        PPC_MAIN_GDEVICE_RECORD,
        PPC_MAIN_PIXMAP_HANDLE,
        0,
        0,
        attributes.height as i16,
        attributes.width as i16,
    )?;
    Some(())
}

fn ppc_dsp_blank_display(
    memory: &mut PpcSectionMem,
    attributes: PpcDspContextAttributes,
    blanking_color: PpcRgbColor,
    screen_clut: &[[u16; 3]; 256],
) -> Option<()> {
    let row_bytes = ppc_row_bytes(attributes.width, attributes.display_depth)?;
    let size = usize::try_from(row_bytes.checked_mul(attributes.height)?).ok()?;
    let mut pixels = vec![0; size];
    match attributes.display_depth {
        8 => pixels.fill(ppc_rgb_color_to_index_in_clut(blanking_color, screen_clut, 256)),
        16 => {
            let pixel = ppc_rgb_color_to_rgb555(blanking_color).to_be_bytes();
            for pair in pixels.chunks_exact_mut(2) {
                pair.copy_from_slice(&pixel);
            }
        }
        _ => return None,
    }
    memory.write_bytes(PPC_MAIN_SCREEN_BASE, &pixels)?;
    Some(())
}

fn ppc_dsp_capture_desktop(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    screen_clut: &[[u16; 3]; 256],
) -> Option<PpcDspDesktopSnapshot> {
    let screen_bytes =
        ppc_memory_read_bytes(memory, PPC_MAIN_SCREEN_BASE, ppc_main_screen_buffer_size())?;
    let gdevice_bytes = ppc_memory_read_bytes(memory, PPC_MAIN_GDEVICE_RECORD, PPC_GDEVICE_SIZE)?;
    let records: Vec<_> = gworlds
        .iter()
        .filter(|record| {
            record.base_addr == PPC_MAIN_SCREEN_BASE || record.base_addr == PPC_DSP_BACK_SCREEN_BASE
        })
        .copied()
        .collect();
    let mut pixmaps = Vec::new();
    for record in &records {
        if !pixmaps.iter().any(|(address, _)| *address == record.pixmap) {
            pixmaps.push((
                record.pixmap,
                ppc_memory_read_bytes(memory, record.pixmap, PPC_PIXMAP_SIZE)?,
            ));
        }
    }
    Some(PpcDspDesktopSnapshot {
        screen_bytes,
        gdevice_bytes,
        screen_clut: *screen_clut,
        records,
        pixmaps,
    })
}

pub(crate) fn ppc_dsp_restore_desktop(
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
    screen_clut: &mut [[u16; 3]; 256],
    draw_sprocket: &mut PpcDrawSprocketState,
) -> bool {
    let Some(snapshot) = draw_sprocket.desktop_snapshot.as_ref() else {
        return true;
    };
    if memory
        .write_bytes(PPC_MAIN_SCREEN_BASE, &snapshot.screen_bytes)
        .is_none()
        || memory
            .write_bytes(PPC_MAIN_GDEVICE_RECORD, &snapshot.gdevice_bytes)
            .is_none()
    {
        return false;
    }
    for (address, pixmap) in &snapshot.pixmaps {
        let still_live = snapshot.records.iter().any(|old| {
            old.pixmap == *address
                && gworlds
                    .iter()
                    .any(|record| record.port == old.port && record.pixmap == *address)
        });
        if still_live && memory.write_bytes(*address, pixmap).is_none() {
            return false;
        }
    }
    for old in &snapshot.records {
        if let Some(record) = gworlds.iter_mut().find(|record| record.port == old.port) {
            if record.pixmap == old.pixmap {
                *record = *old;
            }
        }
    }
    *screen_clut = snapshot.screen_clut;
    draw_sprocket.desktop_snapshot = None;
    true
}

pub(crate) fn ppc_dsp_context_release(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
    screen_clut: &mut [[u16; 3]; 256],
    draw_sprocket: &mut PpcDrawSprocketState,
) -> i16 {
    let context = cpu.gpr[3];
    if let Some(error) = ppc_dsp_context_error(context) {
        return error;
    }
    if draw_sprocket.reserved_context != Some(context) {
        return PPC_DSP_CONTEXT_NOT_RESERVED_ERR;
    }
    if !ppc_dsp_restore_desktop(memory, gworlds, screen_clut, draw_sprocket) {
        return PPC_PARAM_ERR;
    }
    draw_sprocket.reserved_context = None;
    draw_sprocket.active_context = None;
    draw_sprocket.context_state = PpcDspContextPlayState::Inactive;
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_context_set_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    gworlds: &mut [PpcGWorldRecord],
    screen_clut: &mut [[u16; 3]; 256],
    draw_sprocket: &mut PpcDrawSprocketState,
) -> i16 {
    let context = cpu.gpr[3];
    if let Some(error) = ppc_dsp_context_error(context) {
        return error;
    }
    let Some(state) = ppc_dsp_context_state_from_raw(cpu.gpr[4]) else {
        return PPC_PARAM_ERR;
    };
    if draw_sprocket.reserved_context != Some(context) {
        return PPC_DSP_CONTEXT_NOT_RESERVED_ERR;
    }
    match state {
        PpcDspContextPlayState::Active => {
            if draw_sprocket.desktop_snapshot.is_none() {
                let Some(snapshot) = ppc_dsp_capture_desktop(memory, gworlds, screen_clut) else {
                    return PPC_PARAM_ERR;
                };
                draw_sprocket.desktop_snapshot = Some(snapshot);
                if ppc_configure_dsp_framebuffers(
                    memory,
                    gworlds,
                    draw_sprocket.context_attributes,
                )
                .is_none()
                    || ppc_dsp_blank_display(
                        memory,
                        draw_sprocket.context_attributes,
                        draw_sprocket.blanking_color,
                        screen_clut,
                    )
                    .is_none()
                {
                    let _ = ppc_dsp_restore_desktop(memory, gworlds, screen_clut, draw_sprocket);
                    return PPC_PARAM_ERR;
                }
            }
            draw_sprocket.started = true;
            draw_sprocket.active_context = Some(context);
            draw_sprocket.context_state = state;
        }
        PpcDspContextPlayState::Paused | PpcDspContextPlayState::Inactive => {
            if !ppc_dsp_restore_desktop(memory, gworlds, screen_clut, draw_sprocket) {
                return PPC_PARAM_ERR;
            }
            draw_sprocket.started = true;
            draw_sprocket.active_context = None;
            draw_sprocket.context_state = state;
        }
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_context_get_state(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    draw_sprocket: &PpcDrawSprocketState,
) -> i16 {
    let context = cpu.gpr[3];
    if let Some(error) = ppc_dsp_context_error(context) {
        return error;
    }
    let state_out = cpu.gpr[4];
    if state_out == 0 || !ppc_memory_can_write_bytes(memory, state_out, 4) {
        return PPC_PARAM_ERR;
    }
    if draw_sprocket.reserved_context != Some(context) {
        return PPC_DSP_CONTEXT_NOT_RESERVED_ERR;
    }
    let state = match draw_sprocket.context_state {
        PpcDspContextPlayState::Active => PPC_DSP_CONTEXT_STATE_ACTIVE,
        PpcDspContextPlayState::Paused => PPC_DSP_CONTEXT_STATE_PAUSED,
        PpcDspContextPlayState::Inactive => PPC_DSP_CONTEXT_STATE_INACTIVE,
    };
    let _ = memory.write_u32_be(state_out, state);
    PPC_NO_ERR
}

pub(crate) fn ppc_optional_rgb_color(memory: &mut PpcSectionMem, color: u32) -> Option<Option<PpcRgbColor>> {
    if color == 0 {
        Some(None)
    } else {
        Some(Some(ppc_read_rgb_color(memory, color)?))
    }
}

pub(crate) fn ppc_dsp_context_fade_gamma(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    draw_sprocket: &mut PpcDrawSprocketState,
    kind: PpcDspGammaFadeKind,
) -> i16 {
    let context = cpu.gpr[3];
    if context != 0 {
        if let Some(error) = ppc_dsp_context_error(context) {
            return error;
        }
        if kind != PpcDspGammaFadeKind::Manual && draw_sprocket.reserved_context != Some(context) {
            return PPC_DSP_CONTEXT_NOT_RESERVED_ERR;
        }
    } else if kind != PpcDspGammaFadeKind::Manual && draw_sprocket.reserved_context.is_none() {
        return PPC_DSP_CONTEXT_NOT_RESERVED_ERR;
    }
    let (percent, zero_color_ptr) = match kind {
        PpcDspGammaFadeKind::Manual => (cpu.gpr[4] as i32, cpu.gpr[5]),
        PpcDspGammaFadeKind::In => (100, cpu.gpr[4]),
        PpcDspGammaFadeKind::Out => (0, cpu.gpr[4]),
    };
    let Some(zero_color) = ppc_optional_rgb_color(memory, zero_color_ptr) else {
        return PPC_PARAM_ERR;
    };
    draw_sprocket.last_fade_context = Some(context);
    draw_sprocket.last_fade_kind = Some(kind);
    draw_sprocket.last_fade_percent = Some(percent);
    draw_sprocket.last_fade_zero_color = zero_color;
    draw_sprocket.fade_count = draw_sprocket.fade_count.saturating_add(1);
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_context_swap_buffers(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    draw_sprocket: &mut PpcDrawSprocketState,
    gworlds: &[PpcGWorldRecord],
) -> i16 {
    let context = cpu.gpr[3];
    if let Some(error) = ppc_dsp_context_error(context) {
        return error;
    }
    if draw_sprocket.active_context != Some(context) {
        return PPC_PARAM_ERR;
    }
    let Some(front) = gworlds
        .iter()
        .find(|record| record.port == draw_sprocket.front_buffer_gworld)
        .copied()
    else {
        return PPC_PARAM_ERR;
    };
    let Some(back) = gworlds
        .iter()
        .find(|record| record.port == draw_sprocket.back_buffer_gworld)
        .copied()
    else {
        return PPC_PARAM_ERR;
    };
    if !ppc_dsp_present_back_buffer(memory, back, front) {
        return PPC_PARAM_ERR;
    }
    draw_sprocket.last_swap_context = Some(context);
    draw_sprocket.swap_count = draw_sprocket.swap_count.saturating_add(1);
    PPC_NO_ERR
}

pub(crate) fn ppc_dsp_present_back_buffer(
    memory: &mut PpcSectionMem,
    back: PpcGWorldRecord,
    front: PpcGWorldRecord,
) -> bool {
    if back.width != front.width
        || back.height != front.height
        || back.depth != front.depth
        || !matches!(back.depth, 8 | 16)
    {
        return false;
    }
    let row_len =
        match ppc_row_bytes(back.width, back.depth).and_then(|len| usize::try_from(len).ok()) {
            Some(row_len) => row_len,
            None => return false,
        };
    let mut frame = vec![0; row_len.saturating_mul(back.height as usize)];
    for row in 0..back.height {
        let Some(src) = back
            .base_addr
            .checked_add(row.saturating_mul(back.row_bytes))
        else {
            return false;
        };
        let start = row as usize * row_len;
        if memory
            .read_bytes_into(src, &mut frame[start..start + row_len])
            .is_none()
        {
            return false;
        }
    }
    for row in 0..front.height {
        let Some(dst) = front
            .base_addr
            .checked_add(row.saturating_mul(front.row_bytes))
        else {
            return false;
        };
        let start = row as usize * row_len;
        if memory
            .write_bytes(dst, &frame[start..start + row_len])
            .is_none()
        {
            return false;
        }
    }

    // Apple Game Sprockets Guide (1996), DSpContext_SwapBuffers, p. 2-55:
    // the invalid parts of the context's back buffer (or the entire buffer
    // when no dirty rectangles are supplied) are drawn to the screen.
    true
}

pub(crate) fn ppc_dsp_context_set_clut_entries(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    screen_clut: &mut [[u16; 3]; 256],
) -> i16 {
    if let Some(error) = ppc_dsp_context_error(cpu.gpr[3]) {
        return error;
    }
    let entries_ptr = cpu.gpr[4];
    let starting_entry = usize::from(cpu.gpr[5] as u16);
    let entry_count = usize::from(cpu.gpr[6] as u16);
    let Some(end_entry) = starting_entry.checked_add(entry_count) else {
        return PPC_PARAM_ERR;
    };
    if end_entry > screen_clut.len() {
        return PPC_PARAM_ERR;
    }
    let Some(byte_count) = entry_count.checked_mul(8) else {
        return PPC_PARAM_ERR;
    };
    if byte_count != 0
        && (entries_ptr == 0 || !ppc_memory_can_read_bytes(memory, entries_ptr, byte_count as u32))
    {
        return PPC_PARAM_ERR;
    }

    // Inside Macintosh: Apple Game Sprockets Guide (1996),
    // DSpContext_SetCLUTEntries, p. 2-70: inEntries is an array of ColorSpec
    // records replacing a zero-based contiguous range of the context's CLUT.
    for offset in 0..entry_count {
        let entry_ptr = entries_ptr + (offset as u32) * 8;
        let Some(red) = memory.read_u16_be(entry_ptr + 2) else {
            return PPC_PARAM_ERR;
        };
        let Some(green) = memory.read_u16_be(entry_ptr + 4) else {
            return PPC_PARAM_ERR;
        };
        let Some(blue) = memory.read_u16_be(entry_ptr + 6) else {
            return PPC_PARAM_ERR;
        };
        screen_clut[starting_entry + offset] = [red, green, blue];
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_i16_return_value(action: &PpcImportAction) -> Option<i16> {
    match *action {
        PpcImportAction::Return(value) => Some(value as i16),
        _ => None,
    }
}

pub(crate) fn ppc_draw_sprocket_action_name(target: &PpcImportDispatcherTarget) -> Option<&'static str> {
    match target {
        PpcImportDispatcherTarget::DSpStartup => Some("startup"),
        PpcImportDispatcherTarget::DSpShutdown => Some("shutdown"),
        PpcImportDispatcherTarget::DSpGetFirstContext => Some("get_first_context"),
        PpcImportDispatcherTarget::DSpGetNextContext => Some("get_next_context"),
        PpcImportDispatcherTarget::DSpProcessEvent => Some("process_event"),
        PpcImportDispatcherTarget::DSpCanUserSelectContext => Some("can_user_select_context"),
        PpcImportDispatcherTarget::DSpGetMouse => Some("get_mouse"),
        PpcImportDispatcherTarget::DSpFindContextFromPoint => Some("find_context_from_point"),
        PpcImportDispatcherTarget::DSpContextGlobalToLocal => Some("context_global_to_local"),
        PpcImportDispatcherTarget::DSpContextLocalToGlobal => Some("context_local_to_global"),
        PpcImportDispatcherTarget::DSpContextGetState => Some("context_get_state"),
        PpcImportDispatcherTarget::DSpFindBestContext => Some("find_best_context"),
        PpcImportDispatcherTarget::DSpUserSelectContext => Some("user_select_context"),
        PpcImportDispatcherTarget::DSpSetBlankingColor => Some("set_blanking_color"),
        PpcImportDispatcherTarget::DSpAltBufferNew => Some("alt_buffer_new"),
        PpcImportDispatcherTarget::DSpAltBufferGetCGrafPtr => Some("alt_buffer_get_cgraf_ptr"),
        PpcImportDispatcherTarget::DSpContextReserve => Some("context_reserve"),
        PpcImportDispatcherTarget::DSpContextRelease => Some("context_release"),
        PpcImportDispatcherTarget::DSpContextSetState => Some("context_set_state"),
        PpcImportDispatcherTarget::DSpContextFadeGamma => Some("fade_gamma"),
        PpcImportDispatcherTarget::DSpContextFadeGammaIn => Some("fade_gamma_in"),
        PpcImportDispatcherTarget::DSpContextFadeGammaOut => Some("fade_gamma_out"),
        PpcImportDispatcherTarget::DSpContextGetFrontBuffer => Some("get_front_buffer"),
        PpcImportDispatcherTarget::DSpContextGetBackBuffer => Some("get_back_buffer"),
        PpcImportDispatcherTarget::DSpContextSwapBuffers => Some("swap_buffers"),
        PpcImportDispatcherTarget::DSpContextSetClutEntries => Some("set_clut_entries"),
        PpcImportDispatcherTarget::DSpContextGetDisplayID => Some("get_display_id"),
        PpcImportDispatcherTarget::DSpContextGetAttributes => Some("get_attributes"),
        PpcImportDispatcherTarget::DSpContextSetVblProc => Some("set_vbl_proc"),
        PpcImportDispatcherTarget::DSpContextIsBusy => Some("is_busy"),
        PpcImportDispatcherTarget::DSpAltBufferDispose => Some("alt_buffer_dispose"),
        PpcImportDispatcherTarget::DSpContextInvalBackBufferRect => Some("inval_back_buffer_rect"),
        PpcImportDispatcherTarget::DSpContextSetUnderlayAltBuffer => Some("set_underlay_alt_buffer"),
        _ => None,
    }
}

pub(crate) fn ppc_draw_sprocket_trace_entry(
    import_index: u32,
    pc: u32,
    memory: &mut PpcSectionMem,
    cpu: &PpcCpu,
    action: &str,
    result: i16,
    draw_sprocket: &PpcDrawSprocketState,
) -> PpcDrawSprocketTraceEntry {
    let attributes = draw_sprocket.context_attributes;
    let requested_attributes = ppc_draw_sprocket_trace_requested_attributes(action, cpu, memory);
    let fade_zero_color = draw_sprocket.last_fade_zero_color;
    PpcDrawSprocketTraceEntry {
        import_index,
        pc,
        action: action.to_string(),
        result,
        context: ppc_draw_sprocket_trace_context(action, cpu),
        requested_state: ppc_draw_sprocket_trace_requested_state(action, cpu),
        requested_frequency: requested_attributes.map(|attributes| attributes.frequency),
        requested_width: requested_attributes.map(|attributes| attributes.width),
        requested_height: requested_attributes.map(|attributes| attributes.height),
        requested_context_options: requested_attributes
            .map(|attributes| attributes.context_options),
        requested_display_depth_mask: requested_attributes
            .map(|attributes| attributes.display_best_depth_mask),
        requested_back_buffer_depth_mask: requested_attributes
            .map(|attributes| attributes.back_buffer_best_depth_mask),
        requested_display_depth: requested_attributes.map(|attributes| attributes.display_depth),
        requested_back_buffer_depth: requested_attributes
            .map(|attributes| attributes.back_buffer_depth),
        requested_page_count: requested_attributes.map(|attributes| attributes.page_count),
        can_user_select: ppc_draw_sprocket_trace_can_user_select(action, result, cpu, memory),
        fade_kind: draw_sprocket
            .last_fade_kind
            .map(|kind| dsp_gamma_kind_name(kind).to_string()),
        fade_percent: draw_sprocket.last_fade_percent,
        fade_zero_red: fade_zero_color.map(|color| color.red),
        fade_zero_green: fade_zero_color.map(|color| color.green),
        fade_zero_blue: fade_zero_color.map(|color| color.blue),
        reserved_context: draw_sprocket.reserved_context,
        active_context: draw_sprocket.active_context,
        context_state: dsp_context_play_state_name(draw_sprocket.context_state).to_string(),
        front_buffer_gworld: draw_sprocket.front_buffer_gworld,
        back_buffer_gworld: draw_sprocket.back_buffer_gworld,
        last_swap_context: draw_sprocket.last_swap_context,
        swap_count: draw_sprocket.swap_count,
        fade_count: draw_sprocket.fade_count,
        frequency: attributes.frequency,
        width: attributes.width,
        height: attributes.height,
        context_options: attributes.context_options,
        display_depth_mask: attributes.display_best_depth_mask,
        back_buffer_depth_mask: attributes.back_buffer_best_depth_mask,
        display_depth: attributes.display_depth,
        back_buffer_depth: attributes.back_buffer_depth,
        page_count: attributes.page_count,
    }
}

pub(crate) fn ppc_draw_sprocket_trace_requested_attributes(
    action: &str,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> Option<PpcDspContextAttributes> {
    let attributes_ptr = match action {
        "can_user_select_context" | "find_best_context" | "user_select_context" => cpu.gpr[3],
        "context_reserve" => cpu.gpr[4],
        _ => return None,
    };
    ppc_read_optional_dsp_context_attributes(memory, attributes_ptr)
}

pub(crate) fn ppc_draw_sprocket_trace_can_user_select(
    action: &str,
    result: i16,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
) -> Option<bool> {
    if action != "can_user_select_context" || result != PPC_NO_ERR {
        return None;
    }
    memory.read_u8(cpu.gpr[4]).map(|value| value != 0)
}

pub(crate) fn ppc_draw_sprocket_trace_context(action: &str, cpu: &PpcCpu) -> Option<u32> {
    match action {
        "context_reserve" | "context_release" | "context_set_state" | "fade_gamma"
        | "fade_gamma_in" | "fade_gamma_out" | "get_front_buffer" | "get_back_buffer"
        | "swap_buffers" | "get_display_id" | "get_attributes" => Some(cpu.gpr[3]),
        _ => None,
    }
}

pub(crate) fn ppc_draw_sprocket_trace_requested_state(action: &str, cpu: &PpcCpu) -> Option<String> {
    if action != "context_set_state" {
        return None;
    }
    Some(
        ppc_dsp_context_state_from_raw(cpu.gpr[4])
            .map(dsp_context_play_state_name)
            .unwrap_or("invalid")
            .to_string(),
    )
}

pub(crate) fn ppc_isp_element_new_virtual_from_needs(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    heap_limit: u32,
    input_sprocket: &mut PpcInputSprocketState,
    virtual_elements: &mut Vec<PpcInputSprocketVirtualElementRecord>,
) -> i16 {
    let count = cpu.gpr[3];
    let needs_ptr = cpu.gpr[4];
    let out_elements_ptr = cpu.gpr[5];
    if (count > 0 && needs_ptr == 0) || out_elements_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    let Some(output_size) = count.checked_mul(4) else {
        return PPC_PARAM_ERR;
    };
    if !ppc_memory_can_write_bytes(memory, out_elements_ptr, output_size) {
        return PPC_PARAM_ERR;
    }

    let mut drafts = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
    for index in 0..count {
        let Some(need_ptr) = index
            .checked_mul(PPC_ISP_NEED_SIZE)
            .and_then(|offset| needs_ptr.checked_add(offset))
        else {
            return PPC_PARAM_ERR;
        };
        let Some(need_record) = ppc_isp_read_need_record(memory, need_ptr) else {
            return PPC_PARAM_ERR;
        };
        let Some(kind) = ppc_isp_need_record_kind(&need_record) else {
            return PPC_PARAM_ERR;
        };
        let state = ppc_isp_default_simple_state(kind);
        let need_name = ppc_isp_need_record_name(&need_record);
        let action_binding = ppc_isp_action_binding(kind, Some(&need_name));
        drafts.push(PpcInputSprocketVirtualElementDraft {
            need_index: index,
            need_source: need_ptr,
            kind,
            default_state: state,
            action_binding,
            need_name,
            need_record,
        });
    }
    if !ppc_heap_can_alloc_repeated(
        memory,
        *heap_cursor,
        heap_limit,
        PPC_ISP_VIRTUAL_ELEMENT_RECORD_SIZE,
        count,
    ) {
        return PPC_MEM_FULL_ERR;
    }

    let mut created_records = Vec::with_capacity(drafts.len());
    for draft in drafts {
        let element = ppc_process_heap_alloc(
            process_memory_manager,
            memory,
            heap_cursor,
            PPC_ISP_VIRTUAL_ELEMENT_RECORD_SIZE,
            true,
        );
        if element == 0
            || memory.write_u32_be(element, draft.kind).is_none()
            || memory
                .write_u32_be(element + 4, draft.default_state)
                .is_none()
            || memory
                .write_u32_be(
                    element + PPC_ISP_ELEMENT_NEED_INDEX_OFFSET,
                    draft.need_index,
                )
                .is_none()
            || memory
                .write_u32_be(
                    element + PPC_ISP_ELEMENT_NEED_SOURCE_OFFSET,
                    draft.need_source,
                )
                .is_none()
            || !ppc_isp_write_need_record(
                memory,
                element + PPC_ISP_ELEMENT_NEED_RECORD_OFFSET,
                &draft.need_record,
            )
            || draft
                .need_index
                .checked_mul(4)
                .and_then(|offset| out_elements_ptr.checked_add(offset))
                .and_then(|slot| memory.write_u32_be(slot, element))
                .is_none()
        {
            return PPC_MEM_FULL_ERR;
        }
        created_records.push(PpcInputSprocketVirtualElementRecord {
            element,
            need_index: draft.need_index,
            need_source: draft.need_source,
            kind: draft.kind,
            default_state: draft.default_state,
            action_binding: draft.action_binding,
            need_name: draft.need_name,
            need_record: draft.need_record,
        });
    }

    input_sprocket.initialized = true;
    input_sprocket.virtual_element_count =
        input_sprocket.virtual_element_count.saturating_add(count);
    input_sprocket.last_virtual_need_count = count;
    input_sprocket.last_virtual_needs_ptr = needs_ptr;
    input_sprocket.last_virtual_elements_out_ptr = out_elements_ptr;
    virtual_elements.extend(created_records);
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_list_new(
    cpu: &mut PpcCpu,
    process_memory_manager: &mut ProcessNativeMemoryManager,
    memory: &mut PpcSectionMem,
    heap_cursor: &mut u32,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> i16 {
    let count = cpu.gpr[3];
    let elements_ptr = cpu.gpr[4];
    let out_list_ptr = cpu.gpr[5];
    let flags = cpu.gpr[6];
    let Some(elements_size) = count.checked_mul(4) else {
        return PPC_PARAM_ERR;
    };
    if count > PPC_ISP_ELEMENT_LIST_CAPACITY
        || out_list_ptr == 0
        || !ppc_memory_can_write_bytes(memory, out_list_ptr, 4)
        || (count > 0
            && (elements_ptr == 0
                || !ppc_memory_can_read_bytes(memory, elements_ptr, elements_size)))
    {
        return PPC_PARAM_ERR;
    }

    // Apple Game Sprockets Legacy Reference (2003), ISpElementList_New:
    // the returned value is an opaque list reference initialized with the
    // supplied elements. The host-private tail tracks each member's refCon
    // and last state so event transitions can be delivered deterministically.
    let _ = memory.write_u32_be(out_list_ptr, 0);
    let list = ppc_process_heap_alloc(
        process_memory_manager,
        memory,
        heap_cursor,
        PPC_ISP_ELEMENT_LIST_RECORD_SIZE,
        true,
    );
    if list == 0 {
        return PPC_MEM_FULL_ERR;
    }
    if memory.write_u32_be(list, count).is_none()
        || memory.write_u32_be(list + 4, flags).is_none()
        || memory.write_u32_be(out_list_ptr, list).is_none()
    {
        return PPC_MEM_FULL_ERR;
    }
    for index in 0..count {
        let Some(element) = memory.read_u32_be(elements_ptr + index * 4) else {
            return PPC_PARAM_ERR;
        };
        if !ppc_isp_element_list_write_entry(
            memory,
            list,
            index,
            element,
            0,
            ppc_isp_virtual_element_default_state(element, virtual_elements),
        ) {
            return PPC_MEM_FULL_ERR;
        }
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_list_add_elements(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> i16 {
    let list = cpu.gpr[3];
    let ref_con = cpu.gpr[4];
    let count = cpu.gpr[5];
    let elements_ptr = cpu.gpr[6];
    let Some(elements_size) = count.checked_mul(4) else {
        return PPC_PARAM_ERR;
    };
    if list == 0
        || !ppc_memory_can_write_bytes(memory, list, PPC_ISP_ELEMENT_LIST_RECORD_SIZE)
        || (count > 0
            && (elements_ptr == 0
                || !ppc_memory_can_read_bytes(memory, elements_ptr, elements_size)))
    {
        return PPC_PARAM_ERR;
    }
    let Some(existing_count) = memory.read_u32_be(list) else {
        return PPC_PARAM_ERR;
    };
    let Some(new_count) = existing_count.checked_add(count) else {
        return PPC_PARAM_ERR;
    };
    if new_count > PPC_ISP_ELEMENT_LIST_CAPACITY {
        return PPC_PARAM_ERR;
    }
    for offset in 0..count {
        let Some(element) = memory.read_u32_be(elements_ptr + offset * 4) else {
            return PPC_PARAM_ERR;
        };
        if !ppc_isp_element_list_write_entry(
            memory,
            list,
            existing_count + offset,
            element,
            ref_con,
            ppc_isp_virtual_element_default_state(element, virtual_elements),
        ) {
            return PPC_PARAM_ERR;
        }
    }
    if memory.write_u32_be(list, new_count).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_list_get_next_event(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    input: PpcInputSnapshot,
    input_sprocket: PpcInputSprocketState,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
    tick_count: u32,
) -> i16 {
    let list = cpu.gpr[3];
    let buffer_size = cpu.gpr[4];
    let event_ptr = cpu.gpr[5];
    let was_event_ptr = cpu.gpr[6];
    if list == 0
        || !ppc_memory_can_read_bytes(memory, list, PPC_ISP_ELEMENT_LIST_RECORD_SIZE)
        || was_event_ptr == 0
        || !ppc_memory_can_write_bytes(memory, was_event_ptr, 1)
        || (buffer_size > 0
            && (event_ptr == 0 || !ppc_memory_can_write_bytes(memory, event_ptr, buffer_size)))
    {
        return PPC_PARAM_ERR;
    }

    // Apple Game Sprockets Legacy Reference (2003),
    // ISpElementList_GetNextEvent: events contain AbsoluteTime, element,
    // refCon, and data. Button data is 1 on press and 0 on release.
    let count = memory
        .read_u32_be(list)
        .unwrap_or(0)
        .min(PPC_ISP_ELEMENT_LIST_CAPACITY);
    for index in 0..count {
        let Some((element, ref_con, previous_state)) =
            ppc_isp_element_list_read_entry(memory, list, index)
        else {
            return PPC_PARAM_ERR;
        };
        let Some(current_state) = ppc_isp_virtual_element_simple_state(
            element,
            input,
            input_sprocket,
            virtual_elements,
        ) else {
            continue;
        };
        if current_state == previous_state {
            continue;
        }

        let event = [
            0u32,
            tick_count,
            element,
            ref_con,
            current_state,
        ];
        let copy_size = buffer_size.min(PPC_ISP_ELEMENT_EVENT_SIZE);
        for offset in 0..copy_size {
            let word = event[(offset / 4) as usize];
            let shift = 24 - (offset % 4) * 8;
            if memory
                .write_u8(event_ptr + offset, ((word >> shift) & 0xff) as u8)
                .is_none()
            {
                return PPC_PARAM_ERR;
            }
        }
        if memory
            .write_u32_be(
                ppc_isp_element_list_entry_address(list, index) + 8,
                current_state,
            )
            .is_none()
            || memory.write_u8(was_event_ptr, 1).is_none()
        {
            return PPC_PARAM_ERR;
        }
        return if buffer_size < PPC_ISP_ELEMENT_EVENT_SIZE {
            PPC_PARAM_ERR
        } else {
            PPC_NO_ERR
        };
    }

    if memory.write_u8(was_event_ptr, 0).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_list_flush(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    input: PpcInputSnapshot,
    input_sprocket: PpcInputSprocketState,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> i16 {
    let list = cpu.gpr[3];
    if list == 0 || !ppc_memory_can_write_bytes(memory, list, PPC_ISP_ELEMENT_LIST_RECORD_SIZE) {
        return PPC_PARAM_ERR;
    }
    let count = memory
        .read_u32_be(list)
        .unwrap_or(0)
        .min(PPC_ISP_ELEMENT_LIST_CAPACITY);
    for index in 0..count {
        let Some((element, _, _)) = ppc_isp_element_list_read_entry(memory, list, index) else {
            return PPC_PARAM_ERR;
        };
        let Some(state) = ppc_isp_virtual_element_simple_state(
            element,
            input,
            input_sprocket,
            virtual_elements,
        ) else {
            continue;
        };
        if memory
            .write_u32_be(ppc_isp_element_list_entry_address(list, index) + 8, state)
            .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_list_entry_address(list: u32, index: u32) -> u32 {
    list + PPC_ISP_ELEMENT_LIST_HEADER_SIZE + index * PPC_ISP_ELEMENT_LIST_ENTRY_SIZE
}

pub(crate) fn ppc_isp_element_list_write_entry(
    memory: &mut PpcSectionMem,
    list: u32,
    index: u32,
    element: u32,
    ref_con: u32,
    state: u32,
) -> bool {
    let entry = ppc_isp_element_list_entry_address(list, index);
    memory.write_u32_be(entry, element).is_some()
        && memory.write_u32_be(entry + 4, ref_con).is_some()
        && memory.write_u32_be(entry + 8, state).is_some()
}

pub(crate) fn ppc_isp_element_list_read_entry(
    memory: &mut PpcSectionMem,
    list: u32,
    index: u32,
) -> Option<(u32, u32, u32)> {
    let entry = ppc_isp_element_list_entry_address(list, index);
    Some((
        memory.read_u32_be(entry)?,
        memory.read_u32_be(entry + 4)?,
        memory.read_u32_be(entry + 8)?,
    ))
}

pub(crate) fn ppc_isp_virtual_element_default_state(
    element: u32,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> u32 {
    virtual_elements
        .iter()
        .find(|record| record.element == element)
        .map_or(0, |record| record.default_state)
}

pub(crate) fn ppc_isp_virtual_element_simple_state(
    element: u32,
    input: PpcInputSnapshot,
    input_sprocket: PpcInputSprocketState,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> Option<u32> {
    let record = virtual_elements
        .iter()
        .find(|record| record.element == element)?;
    Some(ppc_isp_input_simple_state(
        record.kind,
        record.default_state,
        input,
        input_sprocket,
        record.action_binding,
    ))
}

pub(crate) fn ppc_isp_read_need_record(memory: &mut PpcSectionMem, need_ptr: u32) -> Option<Vec<u8>> {
    let mut record = Vec::with_capacity(usize::try_from(PPC_ISP_NEED_SIZE).ok()?);
    for offset in 0..PPC_ISP_NEED_SIZE {
        record.push(memory.read_u8(need_ptr.checked_add(offset)?)?);
    }
    Some(record)
}

pub(crate) fn ppc_isp_write_need_record(
    memory: &mut PpcSectionMem,
    element_need_ptr: u32,
    need_record: &[u8],
) -> bool {
    for (offset, byte) in need_record.iter().copied().enumerate() {
        let Ok(offset) = u32::try_from(offset) else {
            return false;
        };
        let Some(addr) = element_need_ptr.checked_add(offset) else {
            return false;
        };
        if memory.write_u8(addr, byte).is_none() {
            return false;
        };
    }
    true
}

pub(crate) fn ppc_isp_need_record_kind(need_record: &[u8]) -> Option<u32> {
    let offset = usize::try_from(PPC_ISP_NEED_KIND_OFFSET).ok()?;
    let bytes: [u8; 4] = need_record.get(offset..offset + 4)?.try_into().ok()?;
    Some(u32::from_be_bytes(bytes))
}

pub(crate) fn ppc_isp_need_record_name(need_record: &[u8]) -> String {
    let len = usize::from(need_record.first().copied().unwrap_or(0))
        .min(63)
        .min(need_record.len().saturating_sub(1));
    decode_mac_roman(&need_record[1..1 + len])
}

pub(crate) fn ppc_isp_default_simple_state(kind: u32) -> u32 {
    match kind {
        PPC_ISP_ELEMENT_KIND_AXIS => PPC_ISP_AXIS_MIDDLE,
        PPC_ISP_ELEMENT_KIND_BUTTON | PPC_ISP_ELEMENT_KIND_DPAD | PPC_ISP_ELEMENT_KIND_DELTA => 0,
        _ => 0,
    }
}

pub(crate) fn ppc_isp_action_binding(kind: u32, need_name: Option<&str>) -> PpcInputSprocketActionBinding {
    match kind {
        PPC_ISP_ELEMENT_KIND_AXIS => {
            if ppc_isp_need_name_matches(need_name, &["pitch", "look"]) {
                PpcInputSprocketActionBinding::AxisPitch
            } else if ppc_isp_need_name_matches(need_name, &["yaw", "turn"]) {
                PpcInputSprocketActionBinding::AxisYaw
            } else if ppc_isp_need_name_matches(need_name, &["walk", "horizontal"]) {
                PpcInputSprocketActionBinding::AxisHorizontal
            } else if ppc_isp_need_name_matches(need_name, &["climb", "vertical"]) {
                PpcInputSprocketActionBinding::AxisVertical
            } else {
                PpcInputSprocketActionBinding::AxisDirectional
            }
        }
        PPC_ISP_ELEMENT_KIND_BUTTON => {
            if ppc_isp_need_name_matches_all(need_name, &["swivel", "left"])
                || ppc_isp_need_name_matches_all(need_name, &["camera", "left"])
            {
                PpcInputSprocketActionBinding::ButtonCameraLeft
            } else if ppc_isp_need_name_matches_all(need_name, &["swivel", "right"])
                || ppc_isp_need_name_matches_all(need_name, &["camera", "right"])
            {
                PpcInputSprocketActionBinding::ButtonCameraRight
            } else if ppc_isp_need_name_matches(need_name, &["jump"]) {
                PpcInputSprocketActionBinding::ButtonJump
            } else if ppc_isp_need_name_matches(need_name, &["fire", "attack"]) {
                PpcInputSprocketActionBinding::ButtonFire
            } else if ppc_isp_need_name_matches_all(need_name, &["select", "weapon"])
                || ppc_isp_need_name_matches_all(need_name, &["next", "weapon"])
            {
                PpcInputSprocketActionBinding::ButtonWeapon
            } else if ppc_isp_need_name_matches(need_name, &["pickup", "pick up", "throw"]) {
                PpcInputSprocketActionBinding::ButtonPickup
            } else if ppc_isp_need_name_matches_all(need_name, &["jet", "up"]) {
                PpcInputSprocketActionBinding::ButtonJetUp
            } else if ppc_isp_need_name_matches_all(need_name, &["jet", "down"]) {
                PpcInputSprocketActionBinding::ButtonJetDown
            } else if ppc_isp_need_name_matches(need_name, &["pause", "escape"]) {
                PpcInputSprocketActionBinding::ButtonPause
            } else if ppc_isp_need_name_matches(need_name, &["return", "enter", "confirm"]) {
                PpcInputSprocketActionBinding::ButtonConfirm
            } else if ppc_isp_need_name_matches_all(need_name, &["zoom", "in"]) {
                PpcInputSprocketActionBinding::ButtonZoomIn
            } else if ppc_isp_need_name_matches_all(need_name, &["zoom", "out"]) {
                PpcInputSprocketActionBinding::ButtonZoomOut
            } else if ppc_isp_need_name_matches_all(need_name, &["camera", "mode"]) {
                PpcInputSprocketActionBinding::ButtonCameraMode
            } else if ppc_isp_need_name_matches_all(need_name, &["toggle", "music"])
                || ppc_isp_need_name_matches_all(need_name, &["music", "on"])
            {
                PpcInputSprocketActionBinding::ButtonToggleMusic
            } else if ppc_isp_need_name_matches_all(need_name, &["ambient", "sound"]) {
                PpcInputSprocketActionBinding::ButtonToggleAmbientSound
            } else if ppc_isp_need_name_matches_all(need_name, &["raise", "volume"])
                || ppc_isp_need_name_matches_all(need_name, &["volume", "up"])
            {
                PpcInputSprocketActionBinding::ButtonVolumeUp
            } else if ppc_isp_need_name_matches_all(need_name, &["lower", "volume"])
                || ppc_isp_need_name_matches_all(need_name, &["volume", "down"])
            {
                PpcInputSprocketActionBinding::ButtonVolumeDown
            } else if ppc_isp_need_name_matches(need_name, &["gps"]) {
                PpcInputSprocketActionBinding::ButtonToggleGps
            } else if ppc_isp_need_name_matches_all(need_name, &["quit", "application"]) {
                PpcInputSprocketActionBinding::ButtonQuit
            } else if ppc_isp_need_name_matches(need_name, &["left"]) {
                PpcInputSprocketActionBinding::ButtonLeft
            } else if ppc_isp_need_name_matches(need_name, &["right"]) {
                PpcInputSprocketActionBinding::ButtonRight
            } else if ppc_isp_need_name_matches(need_name, &["forward", "up"]) {
                PpcInputSprocketActionBinding::ButtonForward
            } else if ppc_isp_need_name_matches(need_name, &["backward", "down"]) {
                PpcInputSprocketActionBinding::ButtonBackward
            } else {
                PpcInputSprocketActionBinding::ButtonPrimary
            }
        }
        PPC_ISP_ELEMENT_KIND_DPAD => PpcInputSprocketActionBinding::DpadDirectional,
        PPC_ISP_ELEMENT_KIND_DELTA => {
            if ppc_isp_need_name_matches(need_name, &["pitch", "look"]) {
                PpcInputSprocketActionBinding::DeltaPitch
            } else {
                PpcInputSprocketActionBinding::DeltaYaw
            }
        }
        _ => PpcInputSprocketActionBinding::Unknown,
    }
}

pub(crate) fn ppc_isp_input_simple_state(
    kind: u32,
    fallback_state: u32,
    input: PpcInputSnapshot,
    input_sprocket: PpcInputSprocketState,
    action_binding: PpcInputSprocketActionBinding,
) -> u32 {
    if input_sprocket.suspended {
        return fallback_state;
    }
    match kind {
        PPC_ISP_ELEMENT_KIND_AXIS => {
            if !input_sprocket.keyboard_active {
                return fallback_state;
            }
            let (low_keys, high_keys): (&[u8], &[u8]) = if matches!(
                action_binding,
                PpcInputSprocketActionBinding::AxisYaw
                    | PpcInputSprocketActionBinding::AxisHorizontal
            ) {
                (
                    &[PPC_KEY_LEFT, PPC_KEY_NUMPAD_LEFT],
                    &[PPC_KEY_RIGHT, PPC_KEY_NUMPAD_RIGHT],
                )
            } else if matches!(
                action_binding,
                PpcInputSprocketActionBinding::AxisPitch
                    | PpcInputSprocketActionBinding::AxisVertical
            ) {
                (
                    &[PPC_KEY_UP, PPC_KEY_NUMPAD_UP],
                    &[PPC_KEY_DOWN, PPC_KEY_NUMPAD_DOWN],
                )
            } else {
                (
                    &[
                        PPC_KEY_LEFT,
                        PPC_KEY_NUMPAD_LEFT,
                        PPC_KEY_UP,
                        PPC_KEY_NUMPAD_UP,
                    ],
                    &[
                        PPC_KEY_RIGHT,
                        PPC_KEY_NUMPAD_RIGHT,
                        PPC_KEY_DOWN,
                        PPC_KEY_NUMPAD_DOWN,
                    ],
                )
            };
            let low = input.any_key_down(low_keys);
            let high = input.any_key_down(high_keys);
            match (low, high) {
                (true, false) => PPC_ISP_AXIS_LOW,
                (false, true) => PPC_ISP_AXIS_HIGH,
                (false, false) => fallback_state,
                (true, true) => PPC_ISP_AXIS_MIDDLE,
            }
        }
        PPC_ISP_ELEMENT_KIND_BUTTON => {
            let pressed = ppc_isp_button_pressed(input, input_sprocket, action_binding);
            if pressed {
                PPC_ISP_BUTTON_PRESSED
            } else {
                fallback_state
            }
        }
        PPC_ISP_ELEMENT_KIND_DPAD => {
            if !input_sprocket.keyboard_active {
                return fallback_state;
            }
            let mut state = 0;
            if input.any_key_down(&[PPC_KEY_UP, PPC_KEY_NUMPAD_UP]) {
                state |= PPC_ISP_DPAD_UP;
            }
            if input.any_key_down(&[PPC_KEY_RIGHT, PPC_KEY_NUMPAD_RIGHT]) {
                state |= PPC_ISP_DPAD_RIGHT;
            }
            if input.any_key_down(&[PPC_KEY_DOWN, PPC_KEY_NUMPAD_DOWN]) {
                state |= PPC_ISP_DPAD_DOWN;
            }
            if input.any_key_down(&[PPC_KEY_LEFT, PPC_KEY_NUMPAD_LEFT]) {
                state |= PPC_ISP_DPAD_LEFT;
            }
            if state == 0 {
                fallback_state
            } else {
                state
            }
        }
        PPC_ISP_ELEMENT_KIND_DELTA => {
            if !input_sprocket.mouse_active {
                return fallback_state;
            }
            ppc_isp_mouse_delta_fixed(input, action_binding) as u32
        }
        _ => fallback_state,
    }
}

pub(crate) fn ppc_isp_mouse_delta_fixed(
    input: PpcInputSnapshot,
    action_binding: PpcInputSprocketActionBinding,
) -> i32 {
    let center_h = (ppc_main_screen_width() / 2) as i32;
    let center_v = (ppc_main_screen_height() / 2) as i32;
    let pixel_delta = if action_binding == PpcInputSprocketActionBinding::DeltaPitch {
        center_v.saturating_sub(i32::from(input.mouse_v))
    } else {
        i32::from(input.mouse_h).saturating_sub(center_h)
    };
    pixel_delta.saturating_mul(0x0001_0000)
}

pub(crate) fn ppc_isp_button_pressed(
    input: PpcInputSnapshot,
    input_sprocket: PpcInputSprocketState,
    action_binding: PpcInputSprocketActionBinding,
) -> bool {
    match action_binding {
        PpcInputSprocketActionBinding::ButtonLeft => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_LEFT, PPC_KEY_NUMPAD_LEFT])
        }
        PpcInputSprocketActionBinding::ButtonRight => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_RIGHT, PPC_KEY_NUMPAD_RIGHT])
        }
        PpcInputSprocketActionBinding::ButtonForward => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_UP, PPC_KEY_NUMPAD_UP])
        }
        PpcInputSprocketActionBinding::ButtonBackward => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_DOWN, PPC_KEY_NUMPAD_DOWN])
        }
        PpcInputSprocketActionBinding::ButtonCameraLeft => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_COMMA])
        }
        PpcInputSprocketActionBinding::ButtonCameraRight => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_PERIOD])
        }
        PpcInputSprocketActionBinding::ButtonJump => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_COMMAND])
        }
        PpcInputSprocketActionBinding::ButtonFire => {
            (input_sprocket.mouse_active && input.mouse_button)
                || (input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_SPACE]))
        }
        PpcInputSprocketActionBinding::ButtonWeapon => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_SHIFT])
        }
        PpcInputSprocketActionBinding::ButtonPickup => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_OPTION])
        }
        PpcInputSprocketActionBinding::ButtonJetUp => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_A])
        }
        PpcInputSprocketActionBinding::ButtonJetDown => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_Z])
        }
        PpcInputSprocketActionBinding::ButtonPause => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_ESCAPE])
        }
        PpcInputSprocketActionBinding::ButtonZoomIn => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_1])
        }
        PpcInputSprocketActionBinding::ButtonZoomOut => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_2])
        }
        PpcInputSprocketActionBinding::ButtonCameraMode => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_TAB])
        }
        PpcInputSprocketActionBinding::ButtonToggleMusic => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_CONTROL, PPC_KEY_CONTROL_RIGHT])
                && input.key_down(PPC_KEY_M)
        }
        PpcInputSprocketActionBinding::ButtonToggleAmbientSound => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_CONTROL, PPC_KEY_CONTROL_RIGHT])
                && input.key_down(PPC_KEY_B)
        }
        PpcInputSprocketActionBinding::ButtonVolumeUp => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_EQUAL, PPC_KEY_NUMPAD_ADD])
        }
        PpcInputSprocketActionBinding::ButtonVolumeDown => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_MINUS, PPC_KEY_NUMPAD_SUBTRACT])
        }
        PpcInputSprocketActionBinding::ButtonToggleGps => {
            input_sprocket.keyboard_active && input.any_key_down(&[PPC_KEY_G])
        }
        PpcInputSprocketActionBinding::ButtonQuit => {
            input_sprocket.keyboard_active
                && input.key_down(PPC_KEY_COMMAND)
                && input.key_down(PPC_KEY_Q)
        }
        PpcInputSprocketActionBinding::ButtonConfirm => {
            input_sprocket.keyboard_active
                && input.any_key_down(&[PPC_KEY_RETURN, PPC_KEY_NUMPAD_ENTER])
        }
        _ => {
            (input_sprocket.mouse_active && input.mouse_button)
                || (input_sprocket.keyboard_active
                    && input.any_key_down(&[PPC_KEY_SPACE, PPC_KEY_RETURN, PPC_KEY_NUMPAD_ENTER]))
        }
    }
}

pub(crate) fn ppc_isp_need_name_matches(need_name: Option<&str>, patterns: &[&str]) -> bool {
    let Some(need_name) = need_name else {
        return false;
    };
    let name = need_name.to_ascii_lowercase();
    patterns.iter().any(|pattern| name.contains(pattern))
}

pub(crate) fn ppc_isp_need_name_matches_all(need_name: Option<&str>, patterns: &[&str]) -> bool {
    let Some(need_name) = need_name else {
        return false;
    };
    let name = need_name.to_ascii_lowercase();
    patterns.iter().all(|pattern| name.contains(pattern))
}

pub(crate) fn ppc_isp_init(input_sprocket: &mut PpcInputSprocketState) -> i16 {
    input_sprocket.initialized = true;
    input_sprocket.suspended = false;
    input_sprocket.keyboard_active = true;
    input_sprocket.mouse_active = true;
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_stop(input_sprocket: &mut PpcInputSprocketState) -> i16 {
    input_sprocket.initialized = false;
    input_sprocket.suspended = false;
    input_sprocket.keyboard_active = false;
    input_sprocket.mouse_active = false;
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_suspend(input_sprocket: &mut PpcInputSprocketState) -> i16 {
    input_sprocket.suspended = true;
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_resume(input_sprocket: &mut PpcInputSprocketState) -> i16 {
    input_sprocket.initialized = true;
    input_sprocket.suspended = false;
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_configure(input_sprocket: &mut PpcInputSprocketState) -> i16 {
    input_sprocket.initialized = true;
    input_sprocket.configure_count = input_sprocket.configure_count.saturating_add(1);
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_devices_activate(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    input_sprocket: &mut PpcInputSprocketState,
) -> i16 {
    ppc_isp_devices_set_active(cpu, memory, input_sprocket, true)
}

pub(crate) fn ppc_isp_devices_deactivate(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    input_sprocket: &mut PpcInputSprocketState,
) -> i16 {
    ppc_isp_devices_set_active(cpu, memory, input_sprocket, false)
}

pub(crate) fn ppc_isp_devices_set_active(
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    input_sprocket: &mut PpcInputSprocketState,
    active: bool,
) -> i16 {
    let Some(devices) = ppc_isp_device_arguments(cpu, memory) else {
        return PPC_PARAM_ERR;
    };
    if devices.iter().any(|device| !ppc_isp_known_device(*device)) {
        return PPC_PARAM_ERR;
    }
    input_sprocket.initialized = true;
    for device in devices {
        ppc_isp_set_device_active(input_sprocket, device, active);
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_device_arguments(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> Option<Vec<u32>> {
    let first = cpu.gpr[3];
    let second = cpu.gpr[4];
    if ppc_isp_known_device(first) {
        return Some(vec![first]);
    }
    if first == 0 {
        return Some(Vec::new());
    }
    if first <= PPC_ISP_DEVICE_COUNT {
        return ppc_isp_read_device_list(memory, second, first);
    }
    if second <= PPC_ISP_DEVICE_COUNT {
        return ppc_isp_read_device_list(memory, first, second);
    }
    None
}

pub(crate) fn ppc_isp_read_device_list(
    memory: &mut PpcSectionMem,
    devices_ptr: u32,
    count: u32,
) -> Option<Vec<u32>> {
    if count == 0 {
        return Some(Vec::new());
    }
    if devices_ptr == 0 {
        return None;
    }
    let mut devices = Vec::with_capacity(usize::try_from(count).ok()?);
    for index in 0..count {
        devices.push(memory.read_u32_be(devices_ptr.checked_add(index.checked_mul(4)?)?)?);
    }
    Some(devices)
}

pub(crate) fn ppc_isp_known_device(device: u32) -> bool {
    matches!(device, PPC_ISP_KEYBOARD_DEVICE | PPC_ISP_MOUSE_DEVICE)
}

pub(crate) fn ppc_isp_set_device_active(
    input_sprocket: &mut PpcInputSprocketState,
    device: u32,
    active: bool,
) -> bool {
    match device {
        PPC_ISP_KEYBOARD_DEVICE => {
            input_sprocket.keyboard_active = active;
            true
        }
        PPC_ISP_MOUSE_DEVICE => {
            input_sprocket.mouse_active = active;
            true
        }
        _ => false,
    }
}

pub(crate) fn ppc_isp_device_get_definition(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let device = cpu.gpr[3];
    let buffer_len = cpu.gpr[4];
    let definition_ptr = cpu.gpr[5];
    let (name, device_class, permanent_id) = match device {
        PPC_ISP_KEYBOARD_DEVICE => (b"Keyboard".as_slice(), PPC_ISP_DEVICE_CLASS_KEYBOARD, 1),
        PPC_ISP_MOUSE_DEVICE => (b"Mouse".as_slice(), PPC_ISP_DEVICE_CLASS_MOUSE, 2),
        _ => return PPC_PARAM_ERR,
    };
    if buffer_len < PPC_ISP_DEVICE_DEFINITION_SIZE
        || definition_ptr == 0
        || !ppc_memory_can_write_bytes(memory, definition_ptr, PPC_ISP_DEVICE_DEFINITION_SIZE)
    {
        return PPC_PARAM_ERR;
    }

    // Apple InputSprocket.h 1.7 (QuickTime 6.0.2 SDK),
    // ISpDevice_GetDefinition: the caller supplies sizeof(ISpDeviceDefinition)
    // (92 bytes), beginning with Str63 and followed by seven UInt32 fields.
    for offset in 0..PPC_ISP_DEVICE_DEFINITION_SIZE {
        if memory.write_u8(definition_ptr + offset, 0).is_none() {
            return PPC_PARAM_ERR;
        }
    }
    if memory.write_u8(definition_ptr, name.len() as u8).is_none() {
        return PPC_PARAM_ERR;
    }
    for (offset, byte) in name.iter().copied().enumerate() {
        if memory
            .write_u8(definition_ptr + 1 + offset as u32, byte)
            .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }
    if memory
        .write_u32_be(definition_ptr + 64, device_class)
        .is_none()
        || memory
            .write_u32_be(definition_ptr + 68, device_class)
            .is_none()
        || memory
            .write_u32_be(definition_ptr + 72, permanent_id)
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_device_get_element_list(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let device = cpu.gpr[3];
    let out_element_list_ptr = cpu.gpr[4];
    if !ppc_isp_known_device(device)
        || out_element_list_ptr == 0
        || !ppc_memory_can_write_bytes(memory, out_element_list_ptr, 4)
    {
        return PPC_PARAM_ERR;
    }

    // InputSprocket element-list references are opaque. Keep the synthetic
    // device reference as the corresponding list identity so later list APIs
    // can distinguish the keyboard and mouse without exposing guest memory.
    memory
        .write_u32_be(out_element_list_ptr, device)
        .expect("element-list output pointer was prevalidated");
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_list_extract(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let element_list = cpu.gpr[3];
    let buffer_count = cpu.gpr[4];
    let out_count_ptr = cpu.gpr[5];
    let buffer_ptr = cpu.gpr[6];
    let elements: &[u32] = match element_list {
        PPC_ISP_KEYBOARD_DEVICE => &[PPC_ISP_KEYBOARD_ELEMENT],
        PPC_ISP_MOUSE_DEVICE => &[
            PPC_ISP_MOUSE_X_ELEMENT,
            PPC_ISP_MOUSE_Y_ELEMENT,
            PPC_ISP_MOUSE_BUTTON_ELEMENT,
        ],
        _ => return PPC_PARAM_ERR,
    };
    if out_count_ptr == 0
        || !ppc_memory_can_write_bytes(memory, out_count_ptr, 4)
        || (buffer_count > 0 && buffer_ptr == 0)
    {
        return PPC_PARAM_ERR;
    }
    let copy_count = buffer_count.min(elements.len() as u32);
    let Some(output_size) = copy_count.checked_mul(4) else {
        return PPC_PARAM_ERR;
    };
    if copy_count > 0 && !ppc_memory_can_write_bytes(memory, buffer_ptr, output_size) {
        return PPC_PARAM_ERR;
    }

    memory
        .write_u32_be(out_count_ptr, elements.len() as u32)
        .expect("element-list count pointer was prevalidated");
    for (index, element) in elements
        .iter()
        .copied()
        .take(copy_count as usize)
        .enumerate()
    {
        memory
            .write_u32_be(buffer_ptr + index as u32 * 4, element)
            .expect("element-list buffer was prevalidated");
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_get_info(cpu: &PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let element = cpu.gpr[3];
    let info_ptr = cpu.gpr[4];
    let (label, kind, name) = match element {
        PPC_ISP_KEYBOARD_ELEMENT => (
            PPC_ISP_ELEMENT_LABEL_NONE,
            PPC_ISP_ELEMENT_KIND_BUTTON,
            b"Keyboard Key".as_slice(),
        ),
        PPC_ISP_MOUSE_X_ELEMENT => (
            PPC_ISP_ELEMENT_LABEL_CURSOR_X,
            PPC_ISP_ELEMENT_KIND_DELTA,
            b"Mouse X Delta".as_slice(),
        ),
        PPC_ISP_MOUSE_Y_ELEMENT => (
            PPC_ISP_ELEMENT_LABEL_CURSOR_Y,
            PPC_ISP_ELEMENT_KIND_DELTA,
            b"Mouse Y Delta".as_slice(),
        ),
        PPC_ISP_MOUSE_BUTTON_ELEMENT => (
            PPC_ISP_ELEMENT_LABEL_MOUSE_ONE,
            PPC_ISP_ELEMENT_KIND_BUTTON,
            b"Mouse Button".as_slice(),
        ),
        _ => return PPC_PARAM_ERR,
    };
    if info_ptr == 0 || !ppc_memory_can_write_bytes(memory, info_ptr, PPC_ISP_ELEMENT_INFO_SIZE) {
        return PPC_PARAM_ERR;
    }

    // ISpElementInfo is two OSTypes, a Str63, and two reserved UInt32s.
    for offset in 0..PPC_ISP_ELEMENT_INFO_SIZE {
        memory
            .write_u8(info_ptr + offset, 0)
            .expect("element info pointer was prevalidated");
    }
    memory.write_u32_be(info_ptr, label).unwrap();
    memory.write_u32_be(info_ptr + 4, kind).unwrap();
    memory.write_u8(info_ptr + 8, name.len() as u8).unwrap();
    for (offset, byte) in name.iter().copied().enumerate() {
        memory.write_u8(info_ptr + 9 + offset as u32, byte).unwrap();
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_devices_extract(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let buffer_count = cpu.gpr[3];
    let out_count_ptr = cpu.gpr[4];
    let buffer_ptr = cpu.gpr[5];
    if out_count_ptr == 0 || (buffer_count > 0 && buffer_ptr == 0) {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, out_count_ptr, 4) {
        return PPC_PARAM_ERR;
    }

    let devices = [PPC_ISP_KEYBOARD_DEVICE, PPC_ISP_MOUSE_DEVICE];
    let copy_count = buffer_count.min(PPC_ISP_DEVICE_COUNT);
    let Some(output_size) = copy_count.checked_mul(4) else {
        return PPC_PARAM_ERR;
    };
    if copy_count > 0 && !ppc_memory_can_write_bytes(memory, buffer_ptr, output_size) {
        return PPC_PARAM_ERR;
    }

    memory
        .write_u32_be(out_count_ptr, PPC_ISP_DEVICE_COUNT)
        .unwrap();
    let copy_count = usize::try_from(copy_count).unwrap();
    for (index, device) in devices.iter().copied().take(copy_count).enumerate() {
        if memory
            .write_u32_be(buffer_ptr + (index as u32) * 4, device)
            .is_none()
        {
            return PPC_PARAM_ERR;
        }
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_devices_extract_by_class(cpu: &mut PpcCpu, memory: &mut PpcSectionMem) -> i16 {
    let device_class = cpu.gpr[3];
    let buffer_count = cpu.gpr[4];
    let out_count_ptr = cpu.gpr[5];
    let buffer_ptr = cpu.gpr[6];
    if out_count_ptr == 0 || (buffer_count > 0 && buffer_ptr == 0) {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, out_count_ptr, 4) {
        return PPC_PARAM_ERR;
    }

    let device = match device_class {
        PPC_ISP_DEVICE_CLASS_KEYBOARD => Some(PPC_ISP_KEYBOARD_DEVICE),
        PPC_ISP_DEVICE_CLASS_MOUSE => Some(PPC_ISP_MOUSE_DEVICE),
        _ => None,
    };
    let device_count = u32::from(device.is_some());
    let copy_count = buffer_count.min(device_count);
    let Some(output_size) = copy_count.checked_mul(4) else {
        return PPC_PARAM_ERR;
    };
    if copy_count > 0 && !ppc_memory_can_write_bytes(memory, buffer_ptr, output_size) {
        return PPC_PARAM_ERR;
    }

    memory.write_u32_be(out_count_ptr, device_count).unwrap();
    if copy_count > 0
        && memory
            .write_u32_be(
                buffer_ptr,
                device.expect("device exists when copy count is nonzero"),
            )
            .is_none()
    {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_element_get_simple_state(
    cpu: &mut PpcCpu,
    memory: &mut PpcSectionMem,
    input: PpcInputSnapshot,
    input_sprocket: PpcInputSprocketState,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> i16 {
    let element = cpu.gpr[3];
    let state_ptr = cpu.gpr[4];
    if element == 0 || state_ptr == 0 {
        return PPC_PARAM_ERR;
    }
    if !ppc_memory_can_write_bytes(memory, state_ptr, 4) {
        return PPC_PARAM_ERR;
    }
    let (Some(kind), Some(fallback_state)) =
        (memory.read_u32_be(element), memory.read_u32_be(element + 4))
    else {
        return PPC_PARAM_ERR;
    };
    let need_name = ppc_isp_element_need_name(memory, element);
    let action_binding = ppc_isp_element_action_binding(virtual_elements, element, kind)
        .unwrap_or_else(|| ppc_isp_action_binding(kind, need_name.as_deref()));
    let state =
        ppc_isp_input_simple_state(kind, fallback_state, input, input_sprocket, action_binding);
    if memory.write_u32_be(state_ptr, state).is_none() {
        return PPC_PARAM_ERR;
    }
    PPC_NO_ERR
}

pub(crate) fn ppc_isp_simple_state_trace_entry(
    import_index: u32,
    pc: u32,
    cpu: &PpcCpu,
    memory: &mut PpcSectionMem,
    input: PpcInputSnapshot,
    input_sprocket: PpcInputSprocketState,
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
) -> Option<PpcInputSprocketSimpleStateTraceEntry> {
    let element = cpu.gpr[3];
    let state_ptr = cpu.gpr[4];
    let kind = memory.read_u32_be(element)?;
    let fallback_state = memory.read_u32_be(element.checked_add(4)?)?;
    let state = memory.read_u32_be(state_ptr)?;
    let need_name = ppc_isp_element_need_name(memory, element).unwrap_or_default();
    let action_binding = ppc_isp_element_action_binding(virtual_elements, element, kind)
        .unwrap_or_else(|| ppc_isp_action_binding(kind, Some(&need_name)));

    Some(PpcInputSprocketSimpleStateTraceEntry {
        import_index,
        pc,
        element,
        state_ptr,
        state,
        kind,
        kind_name: isp_element_kind_name(kind).to_string(),
        fallback_state,
        need_name,
        action_binding: isp_action_binding_name(action_binding).to_string(),
        input,
        input_sprocket,
    })
}

pub(crate) fn ppc_isp_element_action_binding(
    virtual_elements: &[PpcInputSprocketVirtualElementRecord],
    element: u32,
    kind: u32,
) -> Option<PpcInputSprocketActionBinding> {
    virtual_elements
        .iter()
        .rev()
        .find(|record| record.element == element && record.kind == kind)
        .map(|record| record.action_binding)
}

pub(crate) fn ppc_isp_element_need_name(memory: &mut PpcSectionMem, element: u32) -> Option<String> {
    let name_ptr = element.checked_add(PPC_ISP_ELEMENT_NEED_RECORD_OFFSET)?;
    let len = usize::from(memory.read_u8(name_ptr)?).min(63);
    let mut bytes = Vec::with_capacity(len);
    for offset in 0..len {
        bytes.push(memory.read_u8(name_ptr.checked_add(1 + offset as u32)?)?);
    }
    Some(decode_mac_roman(&bytes))
}
