//! DrawSprocket and InputSprocket emulation and trace records.

use super::graphics::{PpcRgbColor, PPC_RGB_BLACK};
use super::imports::PpcInputSnapshot;

pub const PPC_DSP_FREQUENCY_60HZ: u32 = 60 << 16;
pub const PPC_DSP_SCREEN_WIDTH: u32 = 640;
pub const PPC_DSP_SCREEN_HEIGHT: u32 = 480;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
