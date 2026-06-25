//! Internal UI theme contract for dialog/menu/control rendering.
//!
//! The first boundary is intentionally semantic and non-invasive: the default
//! `classic-system7` provider represents the current renderer, while
//! `systemless-default` is an explicit non-classic provider for future
//! Systemless-owned snapshots. Guest-visible Toolbox behavior remains outside
//! this boundary.

pub const CLASSIC_SYSTEM7_THEME: &str = "classic-system7";
pub const SYSTEMLESS_DEFAULT_THEME: &str = "systemless-default";
pub const CLASSIC_GUEST_METRICS: &str = "classic-guest-metrics";
pub const THEMED_GUEST_METRICS: &str = "themed-guest-metrics";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiThemeId {
    ClassicSystem7,
    SystemlessDefault,
}

impl UiThemeId {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClassicSystem7 => CLASSIC_SYSTEM7_THEME,
            Self::SystemlessDefault => SYSTEMLESS_DEFAULT_THEME,
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            CLASSIC_SYSTEM7_THEME => Ok(Self::ClassicSystem7),
            SYSTEMLESS_DEFAULT_THEME => Ok(Self::SystemlessDefault),
            _ => Err(format!("unknown ui theme {value:?}")),
        }
    }

    pub fn provider(self) -> &'static dyn UiTheme {
        match self {
            Self::ClassicSystem7 => &CLASSIC_SYSTEM7,
            Self::SystemlessDefault => &SYSTEMLESS_DEFAULT,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeMetricsMode {
    ClassicGuestMetrics,
    ThemedGuestMetrics,
}

impl ThemeMetricsMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClassicGuestMetrics => CLASSIC_GUEST_METRICS,
            Self::ThemedGuestMetrics => THEMED_GUEST_METRICS,
        }
    }

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            CLASSIC_GUEST_METRICS => Ok(Self::ClassicGuestMetrics),
            THEMED_GUEST_METRICS => Ok(Self::ThemedGuestMetrics),
            _ => Err(format!("unknown theme metrics mode {value:?}")),
        }
    }

    pub fn preserves_classic_guest_metrics(self) -> bool {
        self == Self::ClassicGuestMetrics
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogThemeMetrics {
    pub default_button_outline: i16,
    pub edit_text_frame_inset: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuThemeMetrics {
    pub menu_bar_height: i16,
    pub menu_item_height: i16,
    pub title_horizontal_padding: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlThemeMetrics {
    pub button_frame_width: i16,
    pub popup_indicator_width: i16,
    pub popup_item_height: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextTheme {
    pub caret_width: i16,
    pub selection_uses_xor: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiThemePalette {
    pub window_background: Rgb8,
    pub frame_dark: Rgb8,
    pub frame_light: Rgb8,
    pub selection: Rgb8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb8 {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub trait UiTheme: Sync {
    fn id(&self) -> UiThemeId;
    fn metrics_mode(&self) -> ThemeMetricsMode;
    fn dialog_metrics(&self) -> DialogThemeMetrics;
    fn menu_metrics(&self) -> MenuThemeMetrics;
    fn control_metrics(&self) -> ControlThemeMetrics;
    fn text_theme(&self) -> TextTheme;
    fn palette(&self) -> UiThemePalette;
    fn draw_dialog_frame(&self, ctx: &mut ThemeDrawCtx<'_>, state: DialogFrameState);
    fn draw_menu_bar(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuBarState);
    fn draw_menu_title(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuTitleState);
    fn draw_menu_dropdown(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuDropdownState);
    fn draw_menu_item(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuItemState);
    fn draw_text_field(&self, ctx: &mut ThemeDrawCtx<'_>, state: TextFieldState);
    fn draw_text_selection(&self, ctx: &mut ThemeDrawCtx<'_>, state: TextSelectionState);
    fn draw_caret(&self, ctx: &mut ThemeDrawCtx<'_>, state: CaretState);
    fn draw_control(&self, ctx: &mut ThemeDrawCtx<'_>, state: ControlState);
    fn draw_scrollbar(&self, ctx: &mut ThemeDrawCtx<'_>, state: ScrollbarState);
}

#[derive(Debug)]
pub struct ClassicSystem7Theme;

#[derive(Debug)]
pub struct SystemlessTheme;

pub static CLASSIC_SYSTEM7: ClassicSystem7Theme = ClassicSystem7Theme;
pub static SYSTEMLESS_DEFAULT: SystemlessTheme = SystemlessTheme;

const CLASSIC_DIALOG_METRICS: DialogThemeMetrics = DialogThemeMetrics {
    default_button_outline: 4,
    edit_text_frame_inset: 3,
};

const CLASSIC_MENU_METRICS: MenuThemeMetrics = MenuThemeMetrics {
    menu_bar_height: 20,
    menu_item_height: 16,
    title_horizontal_padding: 7,
};

const CLASSIC_CONTROL_METRICS: ControlThemeMetrics = ControlThemeMetrics {
    button_frame_width: 1,
    popup_indicator_width: 14,
    popup_item_height: 16,
};

const CLASSIC_TEXT_THEME: TextTheme = TextTheme {
    caret_width: 1,
    selection_uses_xor: true,
};

impl UiTheme for ClassicSystem7Theme {
    fn id(&self) -> UiThemeId {
        UiThemeId::ClassicSystem7
    }

    fn metrics_mode(&self) -> ThemeMetricsMode {
        ThemeMetricsMode::ClassicGuestMetrics
    }

    fn dialog_metrics(&self) -> DialogThemeMetrics {
        CLASSIC_DIALOG_METRICS
    }

    fn menu_metrics(&self) -> MenuThemeMetrics {
        CLASSIC_MENU_METRICS
    }

    fn control_metrics(&self) -> ControlThemeMetrics {
        CLASSIC_CONTROL_METRICS
    }

    fn text_theme(&self) -> TextTheme {
        CLASSIC_TEXT_THEME
    }

    fn palette(&self) -> UiThemePalette {
        UiThemePalette {
            window_background: Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            frame_dark: Rgb8 { r: 0, g: 0, b: 0 },
            frame_light: Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            selection: Rgb8 { r: 0, g: 0, b: 0 },
        }
    }

    fn draw_dialog_frame(&self, ctx: &mut ThemeDrawCtx<'_>, state: DialogFrameState) {
        draw_dialog_frame_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_bar(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuBarState) {
        draw_menu_bar_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_title(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuTitleState) {
        draw_menu_title_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_dropdown(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuDropdownState) {
        draw_menu_dropdown_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_item(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuItemState) {
        draw_menu_item_chrome(ctx, self.palette(), state);
    }

    fn draw_text_field(&self, ctx: &mut ThemeDrawCtx<'_>, state: TextFieldState) {
        draw_text_field_chrome(ctx, self.palette(), state);
    }

    fn draw_text_selection(&self, ctx: &mut ThemeDrawCtx<'_>, state: TextSelectionState) {
        draw_text_selection_chrome(ctx, self.palette(), state);
    }

    fn draw_caret(&self, ctx: &mut ThemeDrawCtx<'_>, state: CaretState) {
        draw_caret_chrome(ctx, self.palette(), state);
    }

    fn draw_control(&self, ctx: &mut ThemeDrawCtx<'_>, state: ControlState) {
        draw_control_chrome(ctx, self.palette(), state);
    }

    fn draw_scrollbar(&self, ctx: &mut ThemeDrawCtx<'_>, state: ScrollbarState) {
        draw_scrollbar_chrome(ctx, self.palette(), state);
    }
}

impl UiTheme for SystemlessTheme {
    fn id(&self) -> UiThemeId {
        UiThemeId::SystemlessDefault
    }

    fn metrics_mode(&self) -> ThemeMetricsMode {
        ThemeMetricsMode::ClassicGuestMetrics
    }

    fn dialog_metrics(&self) -> DialogThemeMetrics {
        CLASSIC_DIALOG_METRICS
    }

    fn menu_metrics(&self) -> MenuThemeMetrics {
        CLASSIC_MENU_METRICS
    }

    fn control_metrics(&self) -> ControlThemeMetrics {
        CLASSIC_CONTROL_METRICS
    }

    fn text_theme(&self) -> TextTheme {
        CLASSIC_TEXT_THEME
    }

    fn palette(&self) -> UiThemePalette {
        UiThemePalette {
            window_background: Rgb8 {
                r: 247,
                g: 248,
                b: 245,
            },
            frame_dark: Rgb8 {
                r: 33,
                g: 38,
                b: 44,
            },
            frame_light: Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            selection: Rgb8 {
                r: 51,
                g: 102,
                b: 204,
            },
        }
    }

    fn draw_dialog_frame(&self, ctx: &mut ThemeDrawCtx<'_>, state: DialogFrameState) {
        draw_systemless_dialog_frame_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_bar(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuBarState) {
        draw_systemless_menu_bar_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_title(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuTitleState) {
        draw_systemless_menu_title_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_dropdown(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuDropdownState) {
        draw_systemless_menu_dropdown_chrome(ctx, self.palette(), state);
    }

    fn draw_menu_item(&self, ctx: &mut ThemeDrawCtx<'_>, state: MenuItemState) {
        draw_systemless_menu_item_chrome(ctx, self.palette(), state);
    }

    fn draw_text_field(&self, ctx: &mut ThemeDrawCtx<'_>, state: TextFieldState) {
        draw_systemless_text_field_chrome(ctx, self.palette(), state);
    }

    fn draw_text_selection(&self, ctx: &mut ThemeDrawCtx<'_>, state: TextSelectionState) {
        draw_systemless_text_selection_chrome(ctx, self.palette(), state);
    }

    fn draw_caret(&self, ctx: &mut ThemeDrawCtx<'_>, state: CaretState) {
        draw_systemless_caret_chrome(ctx, self.palette(), state);
    }

    fn draw_control(&self, ctx: &mut ThemeDrawCtx<'_>, state: ControlState) {
        draw_systemless_control_chrome(ctx, self.palette(), state);
    }

    fn draw_scrollbar(&self, ctx: &mut ThemeDrawCtx<'_>, state: ScrollbarState) {
        draw_systemless_scrollbar_chrome(ctx, self.palette(), state);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlKind {
    PushButton,
    Checkbox,
    RadioButton,
    PopupButton,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeRect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogFrameKind {
    Document,
    DialogBox,
    PlainDialog,
    AlertDialog,
    NoGrowDocument,
    MovableDialog,
    Unknown,
}

impl DialogFrameKind {
    pub fn from_window_proc_id(proc_id: i16) -> Self {
        match proc_id {
            0 => Self::Document,
            1 => Self::DialogBox,
            2 => Self::PlainDialog,
            3 => Self::AlertDialog,
            4 => Self::NoGrowDocument,
            5 => Self::MovableDialog,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DialogFrameState {
    pub frame_rect: ThemeRect,
    pub content_rect: ThemeRect,
    pub kind: DialogFrameKind,
    pub active: bool,
    pub fill_content: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlState {
    pub kind: ControlKind,
    pub rect: ThemeRect,
    pub enabled: bool,
    pub pressed: bool,
    pub selected: bool,
    pub is_default: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarOrientation {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarPart {
    None,
    DecrementArrow,
    IncrementArrow,
    PageBefore,
    PageAfter,
    Thumb,
}

impl ScrollbarPart {
    pub fn from_control_part_code(part_code: u8) -> Self {
        match part_code {
            20 => Self::DecrementArrow,
            21 => Self::IncrementArrow,
            22 => Self::PageBefore,
            23 => Self::PageAfter,
            129 => Self::Thumb,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScrollbarState {
    pub rect: ThemeRect,
    pub orientation: ScrollbarOrientation,
    pub enabled: bool,
    pub value: i16,
    pub min: i16,
    pub max: i16,
    pub highlighted_part: ScrollbarPart,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuBarState {
    pub rect: ThemeRect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuTitleState {
    pub rect: ThemeRect,
    pub enabled: bool,
    pub highlighted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuDropdownState {
    pub rect: ThemeRect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MenuItemState {
    pub rect: ThemeRect,
    pub enabled: bool,
    pub highlighted: bool,
    pub separator: bool,
    pub has_icon: bool,
    pub checked: bool,
    pub has_command_key: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextFieldState {
    pub rect: ThemeRect,
    pub enabled: bool,
    pub focused: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextSelectionState {
    pub rect: ThemeRect,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CaretState {
    pub rect: ThemeRect,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeBitmap {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl ThemeBitmap {
    pub fn new(width: u32, height: u32, fill: Rgb8) -> Self {
        let mut rgba = vec![0; width as usize * height as usize * 4];
        for px in rgba.chunks_exact_mut(4) {
            px.copy_from_slice(&[fill.r, fill.g, fill.b, 0xFF]);
        }
        Self {
            width,
            height,
            rgba,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

pub struct ThemeDrawCtx<'a> {
    bitmap: &'a mut ThemeBitmap,
}

impl<'a> ThemeDrawCtx<'a> {
    pub fn new(bitmap: &'a mut ThemeBitmap) -> Self {
        Self { bitmap }
    }

    pub fn fill_rect(&mut self, rect: ThemeRect, color: Rgb8) {
        let top = rect.top.max(0).min(self.bitmap.height as i16) as u32;
        let left = rect.left.max(0).min(self.bitmap.width as i16) as u32;
        let bottom = rect.bottom.max(0).min(self.bitmap.height as i16) as u32;
        let right = rect.right.max(0).min(self.bitmap.width as i16) as u32;
        for y in top..bottom {
            for x in left..right {
                self.set_pixel(x, y, color);
            }
        }
    }

    pub fn frame_rect(&mut self, rect: ThemeRect, color: Rgb8) {
        if rect.top >= rect.bottom || rect.left >= rect.right {
            return;
        }
        self.fill_rect(
            ThemeRect {
                top: rect.top,
                left: rect.left,
                bottom: rect.top + 1,
                right: rect.right,
            },
            color,
        );
        self.fill_rect(
            ThemeRect {
                top: rect.bottom - 1,
                left: rect.left,
                bottom: rect.bottom,
                right: rect.right,
            },
            color,
        );
        self.fill_rect(
            ThemeRect {
                top: rect.top,
                left: rect.left,
                bottom: rect.bottom,
                right: rect.left + 1,
            },
            color,
        );
        self.fill_rect(
            ThemeRect {
                top: rect.top,
                left: rect.right - 1,
                bottom: rect.bottom,
                right: rect.right,
            },
            color,
        );
    }

    fn set_pixel(&mut self, x: u32, y: u32, color: Rgb8) {
        if x >= self.bitmap.width || y >= self.bitmap.height {
            return;
        }
        let idx = ((y * self.bitmap.width + x) * 4) as usize;
        self.bitmap.rgba[idx..idx + 4].copy_from_slice(&[color.r, color.g, color.b, 0xFF]);
    }
}

pub fn render_basic_button_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(96, 48, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    theme.draw_control(
        &mut ctx,
        ControlState {
            kind: ControlKind::PushButton,
            rect: ThemeRect {
                top: 14,
                left: 18,
                bottom: 34,
                right: 78,
            },
            enabled: true,
            pressed: false,
            selected: false,
            is_default: true,
        },
    );
    bitmap
}

pub fn render_basic_controls_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(120, 72, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    for state in [
        ControlState {
            kind: ControlKind::PushButton,
            rect: ThemeRect {
                top: 12,
                left: 18,
                bottom: 32,
                right: 84,
            },
            enabled: true,
            pressed: false,
            selected: false,
            is_default: true,
        },
        ControlState {
            kind: ControlKind::Checkbox,
            rect: ThemeRect {
                top: 44,
                left: 22,
                bottom: 56,
                right: 34,
            },
            enabled: true,
            pressed: false,
            selected: true,
            is_default: false,
        },
        ControlState {
            kind: ControlKind::RadioButton,
            rect: ThemeRect {
                top: 44,
                left: 70,
                bottom: 56,
                right: 82,
            },
            enabled: true,
            pressed: false,
            selected: true,
            is_default: false,
        },
    ] {
        theme.draw_control(&mut ctx, state);
    }
    bitmap
}

pub fn render_basic_dialog_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(224, 148, palette.frame_light);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);

    theme.draw_dialog_frame(
        &mut ctx,
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 10,
                left: 10,
                bottom: 138,
                right: 214,
            },
            content_rect: ThemeRect {
                top: 18,
                left: 18,
                bottom: 130,
                right: 206,
            },
            kind: DialogFrameKind::DialogBox,
            active: true,
            fill_content: true,
        },
    );

    ctx.fill_rect(
        ThemeRect {
            top: 30,
            left: 32,
            bottom: 32,
            right: 104,
        },
        palette.frame_dark,
    );
    theme.draw_text_field(
        &mut ctx,
        TextFieldState {
            rect: ThemeRect {
                top: 38,
                left: 32,
                bottom: 60,
                right: 192,
            },
            enabled: true,
            focused: true,
        },
    );
    theme.draw_text_selection(
        &mut ctx,
        TextSelectionState {
            rect: ThemeRect {
                top: 44,
                left: 42,
                bottom: 56,
                right: 92,
            },
            active: true,
        },
    );
    theme.draw_caret(
        &mut ctx,
        CaretState {
            rect: ThemeRect {
                top: 42,
                left: 120,
                bottom: 57,
                right: 121,
            },
            active: true,
        },
    );

    for (state, marker) in [
        (
            ControlState {
                kind: ControlKind::Checkbox,
                rect: ThemeRect {
                    top: 72,
                    left: 34,
                    bottom: 84,
                    right: 46,
                },
                enabled: true,
                pressed: false,
                selected: true,
                is_default: false,
            },
            ThemeRect {
                top: 77,
                left: 52,
                bottom: 79,
                right: 96,
            },
        ),
        (
            ControlState {
                kind: ControlKind::RadioButton,
                rect: ThemeRect {
                    top: 72,
                    left: 118,
                    bottom: 84,
                    right: 130,
                },
                enabled: true,
                pressed: false,
                selected: false,
                is_default: false,
            },
            ThemeRect {
                top: 77,
                left: 136,
                bottom: 79,
                right: 180,
            },
        ),
    ] {
        theme.draw_control(&mut ctx, state);
        ctx.fill_rect(marker, palette.frame_dark);
    }

    theme.draw_control(
        &mut ctx,
        ControlState {
            kind: ControlKind::PopupButton,
            rect: ThemeRect {
                top: 92,
                left: 32,
                bottom: 112,
                right: 128,
            },
            enabled: true,
            pressed: false,
            selected: false,
            is_default: false,
        },
    );
    ctx.fill_rect(
        ThemeRect {
            top: 100,
            left: 42,
            bottom: 102,
            right: 82,
        },
        palette.frame_dark,
    );

    for state in [
        ControlState {
            kind: ControlKind::PushButton,
            rect: ThemeRect {
                top: 102,
                left: 150,
                bottom: 122,
                right: 192,
            },
            enabled: true,
            pressed: false,
            selected: false,
            is_default: true,
        },
        ControlState {
            kind: ControlKind::PushButton,
            rect: ThemeRect {
                top: 102,
                left: 96,
                bottom: 122,
                right: 138,
            },
            enabled: true,
            pressed: false,
            selected: false,
            is_default: false,
        },
    ] {
        theme.draw_control(&mut ctx, state);
    }

    bitmap
}

pub fn render_control_states_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(176, 96, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    for state in [
        ControlState {
            kind: ControlKind::PushButton,
            rect: ThemeRect {
                top: 12,
                left: 14,
                bottom: 32,
                right: 72,
            },
            enabled: true,
            pressed: true,
            selected: false,
            is_default: false,
        },
        ControlState {
            kind: ControlKind::PushButton,
            rect: ThemeRect {
                top: 12,
                left: 92,
                bottom: 32,
                right: 158,
            },
            enabled: false,
            pressed: false,
            selected: false,
            is_default: true,
        },
        ControlState {
            kind: ControlKind::Checkbox,
            rect: ThemeRect {
                top: 46,
                left: 18,
                bottom: 58,
                right: 30,
            },
            enabled: true,
            pressed: true,
            selected: false,
            is_default: false,
        },
        ControlState {
            kind: ControlKind::Checkbox,
            rect: ThemeRect {
                top: 46,
                left: 58,
                bottom: 58,
                right: 70,
            },
            enabled: false,
            pressed: false,
            selected: true,
            is_default: false,
        },
        ControlState {
            kind: ControlKind::RadioButton,
            rect: ThemeRect {
                top: 46,
                left: 100,
                bottom: 58,
                right: 112,
            },
            enabled: true,
            pressed: true,
            selected: false,
            is_default: false,
        },
        ControlState {
            kind: ControlKind::RadioButton,
            rect: ThemeRect {
                top: 46,
                left: 140,
                bottom: 58,
                right: 152,
            },
            enabled: false,
            pressed: false,
            selected: true,
            is_default: false,
        },
        ControlState {
            kind: ControlKind::PopupButton,
            rect: ThemeRect {
                top: 70,
                left: 16,
                bottom: 88,
                right: 80,
            },
            enabled: true,
            pressed: true,
            selected: false,
            is_default: false,
        },
        ControlState {
            kind: ControlKind::PopupButton,
            rect: ThemeRect {
                top: 70,
                left: 96,
                bottom: 88,
                right: 160,
            },
            enabled: false,
            pressed: false,
            selected: false,
            is_default: false,
        },
    ] {
        theme.draw_control(&mut ctx, state);
    }
    bitmap
}

pub fn render_popup_control_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(128, 48, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    theme.draw_control(
        &mut ctx,
        ControlState {
            kind: ControlKind::PopupButton,
            rect: ThemeRect {
                top: 14,
                left: 14,
                bottom: 34,
                right: 114,
            },
            enabled: true,
            pressed: false,
            selected: false,
            is_default: false,
        },
    );
    bitmap
}

pub fn render_scrollbars_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(144, 104, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    for state in [
        ScrollbarState {
            rect: ThemeRect {
                top: 12,
                left: 18,
                bottom: 88,
                right: 34,
            },
            orientation: ScrollbarOrientation::Vertical,
            enabled: true,
            value: 40,
            min: 0,
            max: 100,
            highlighted_part: ScrollbarPart::Thumb,
        },
        ScrollbarState {
            rect: ThemeRect {
                top: 12,
                left: 46,
                bottom: 88,
                right: 62,
            },
            orientation: ScrollbarOrientation::Vertical,
            enabled: false,
            value: 0,
            min: 0,
            max: 100,
            highlighted_part: ScrollbarPart::None,
        },
        ScrollbarState {
            rect: ThemeRect {
                top: 22,
                left: 76,
                bottom: 38,
                right: 132,
            },
            orientation: ScrollbarOrientation::Horizontal,
            enabled: true,
            value: 70,
            min: 0,
            max: 100,
            highlighted_part: ScrollbarPart::IncrementArrow,
        },
        ScrollbarState {
            rect: ThemeRect {
                top: 58,
                left: 76,
                bottom: 74,
                right: 132,
            },
            orientation: ScrollbarOrientation::Horizontal,
            enabled: true,
            value: 15,
            min: 0,
            max: 100,
            highlighted_part: ScrollbarPart::PageBefore,
        },
    ] {
        theme.draw_scrollbar(&mut ctx, state);
    }
    bitmap
}

pub fn render_menu_chrome_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(160, 96, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    theme.draw_menu_bar(
        &mut ctx,
        MenuBarState {
            rect: ThemeRect {
                top: 0,
                left: 0,
                bottom: 20,
                right: 160,
            },
        },
    );
    for state in [
        MenuTitleState {
            rect: ThemeRect {
                top: 1,
                left: 11,
                bottom: 19,
                right: 45,
            },
            enabled: true,
            highlighted: false,
        },
        MenuTitleState {
            rect: ThemeRect {
                top: 1,
                left: 45,
                bottom: 19,
                right: 84,
            },
            enabled: true,
            highlighted: true,
        },
    ] {
        theme.draw_menu_title(&mut ctx, state);
    }
    theme.draw_menu_dropdown(
        &mut ctx,
        MenuDropdownState {
            rect: ThemeRect {
                top: 20,
                left: 18,
                bottom: 78,
                right: 124,
            },
        },
    );
    bitmap
}

pub fn render_menu_items_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(176, 104, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    let dropdown = ThemeRect {
        top: 12,
        left: 18,
        bottom: 92,
        right: 156,
    };
    theme.draw_menu_dropdown(&mut ctx, MenuDropdownState { rect: dropdown });

    for (index, state) in [
        MenuItemState {
            rect: ThemeRect {
                top: 17,
                left: 20,
                bottom: 33,
                right: 154,
            },
            enabled: true,
            highlighted: false,
            separator: false,
            has_icon: true,
            checked: true,
            has_command_key: true,
        },
        MenuItemState {
            rect: ThemeRect {
                top: 33,
                left: 20,
                bottom: 49,
                right: 154,
            },
            enabled: true,
            highlighted: true,
            separator: false,
            has_icon: false,
            checked: false,
            has_command_key: false,
        },
        MenuItemState {
            rect: ThemeRect {
                top: 49,
                left: 20,
                bottom: 65,
                right: 154,
            },
            enabled: false,
            highlighted: false,
            separator: true,
            has_icon: false,
            checked: false,
            has_command_key: false,
        },
        MenuItemState {
            rect: ThemeRect {
                top: 65,
                left: 20,
                bottom: 81,
                right: 154,
            },
            enabled: false,
            highlighted: false,
            separator: false,
            has_icon: true,
            checked: false,
            has_command_key: true,
        },
    ]
    .into_iter()
    .enumerate()
    {
        theme.draw_menu_item(&mut ctx, state);
        if !state.separator {
            let text_top = state.rect.top + 6;
            let text_left = state.rect.left + 20;
            let text_width = if index == 1 { 58 } else { 72 };
            ctx.fill_rect(
                ThemeRect {
                    top: text_top,
                    left: text_left,
                    bottom: text_top + 2,
                    right: text_left + text_width,
                },
                palette.frame_dark,
            );
        }
    }
    bitmap
}

pub fn render_edit_text_selection_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(176, 104, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    let active_field = ThemeRect {
        top: 10,
        left: 18,
        bottom: 40,
        right: 158,
    };
    theme.draw_text_field(
        &mut ctx,
        TextFieldState {
            rect: active_field,
            enabled: true,
            focused: true,
        },
    );
    theme.draw_text_selection(
        &mut ctx,
        TextSelectionState {
            rect: ThemeRect {
                top: 17,
                left: 26,
                bottom: 33,
                right: 84,
            },
            active: true,
        },
    );
    theme.draw_caret(
        &mut ctx,
        CaretState {
            rect: ThemeRect {
                top: 16,
                left: 112,
                bottom: 35,
                right: 113,
            },
            active: true,
        },
    );
    let inactive_field = ThemeRect {
        top: 58,
        left: 18,
        bottom: 90,
        right: 158,
    };
    theme.draw_text_field(
        &mut ctx,
        TextFieldState {
            rect: inactive_field,
            enabled: true,
            focused: false,
        },
    );
    theme.draw_text_selection(
        &mut ctx,
        TextSelectionState {
            rect: ThemeRect {
                top: 66,
                left: 28,
                bottom: 82,
                right: 100,
            },
            active: false,
        },
    );
    // Text 1993 appendix C: `TERec.active` marks when the selection
    // range is highlighted or the caret is displayed. The inactive
    // caret call keeps the preview covering that semantic no-op.
    theme.draw_caret(
        &mut ctx,
        CaretState {
            rect: ThemeRect {
                top: 64,
                left: 126,
                bottom: 84,
                right: 127,
            },
            active: false,
        },
    );
    bitmap
}

pub fn render_text_field_states_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(176, 92, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    for state in [
        TextFieldState {
            rect: ThemeRect {
                top: 10,
                left: 18,
                bottom: 32,
                right: 158,
            },
            enabled: true,
            focused: true,
        },
        TextFieldState {
            rect: ThemeRect {
                top: 38,
                left: 18,
                bottom: 60,
                right: 158,
            },
            enabled: true,
            focused: false,
        },
        TextFieldState {
            rect: ThemeRect {
                top: 66,
                left: 18,
                bottom: 86,
                right: 158,
            },
            enabled: false,
            focused: false,
        },
    ] {
        theme.draw_text_field(&mut ctx, state);
    }
    bitmap
}

pub fn render_dialog_frame_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(184, 112, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    theme.draw_dialog_frame(
        &mut ctx,
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 12,
                left: 12,
                bottom: 92,
                right: 118,
            },
            content_rect: ThemeRect {
                top: 20,
                left: 20,
                bottom: 84,
                right: 110,
            },
            kind: DialogFrameKind::DialogBox,
            active: true,
            fill_content: true,
        },
    );
    theme.draw_dialog_frame(
        &mut ctx,
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 34,
                left: 128,
                bottom: 88,
                right: 172,
            },
            content_rect: ThemeRect {
                top: 34,
                left: 128,
                bottom: 86,
                right: 170,
            },
            kind: DialogFrameKind::AlertDialog,
            active: true,
            fill_content: true,
        },
    );
    bitmap
}

pub fn render_dialog_frame_states_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(216, 128, palette.frame_light);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);

    for state in [
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 10,
                left: 10,
                bottom: 72,
                right: 104,
            },
            content_rect: ThemeRect {
                top: 18,
                left: 18,
                bottom: 64,
                right: 96,
            },
            kind: DialogFrameKind::DialogBox,
            active: true,
            fill_content: true,
        },
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 12,
                left: 122,
                bottom: 72,
                right: 204,
            },
            content_rect: ThemeRect {
                top: 18,
                left: 128,
                bottom: 62,
                right: 196,
            },
            kind: DialogFrameKind::AlertDialog,
            active: true,
            fill_content: true,
        },
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 84,
                left: 16,
                bottom: 118,
                right: 92,
            },
            content_rect: ThemeRect {
                top: 88,
                left: 20,
                bottom: 112,
                right: 86,
            },
            kind: DialogFrameKind::PlainDialog,
            active: true,
            fill_content: true,
        },
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 80,
                left: 118,
                bottom: 118,
                right: 204,
            },
            content_rect: ThemeRect {
                top: 88,
                left: 126,
                bottom: 110,
                right: 196,
            },
            kind: DialogFrameKind::DialogBox,
            active: false,
            fill_content: false,
        },
    ] {
        theme.draw_dialog_frame(&mut ctx, state);
    }

    bitmap
}

pub fn render_window_frame_states_preview(theme_id: UiThemeId) -> ThemeBitmap {
    let theme = theme_id.provider();
    let palette = theme.palette();
    let mut bitmap = ThemeBitmap::new(216, 128, palette.window_background);
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);

    for state in [
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 10,
                left: 10,
                bottom: 70,
                right: 104,
            },
            content_rect: ThemeRect {
                top: 24,
                left: 16,
                bottom: 62,
                right: 96,
            },
            kind: DialogFrameKind::Document,
            active: true,
            fill_content: true,
        },
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 12,
                left: 122,
                bottom: 70,
                right: 206,
            },
            content_rect: ThemeRect {
                top: 26,
                left: 130,
                bottom: 62,
                right: 198,
            },
            kind: DialogFrameKind::MovableDialog,
            active: true,
            fill_content: true,
        },
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 82,
                left: 18,
                bottom: 116,
                right: 84,
            },
            content_rect: ThemeRect {
                top: 88,
                left: 24,
                bottom: 110,
                right: 78,
            },
            kind: DialogFrameKind::PlainDialog,
            active: true,
            fill_content: true,
        },
        DialogFrameState {
            frame_rect: ThemeRect {
                top: 80,
                left: 112,
                bottom: 116,
                right: 194,
            },
            content_rect: ThemeRect {
                top: 94,
                left: 120,
                bottom: 108,
                right: 186,
            },
            kind: DialogFrameKind::NoGrowDocument,
            active: false,
            fill_content: false,
        },
    ] {
        theme.draw_dialog_frame(&mut ctx, state);
    }

    bitmap
}

fn draw_dialog_frame_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: DialogFrameState,
) {
    fill_dialog_frame_background(ctx, state, palette.frame_light);
    match state.kind {
        DialogFrameKind::DialogBox => {
            ctx.frame_rect(state.content_rect, palette.frame_dark);
            ctx.frame_rect(outset_rect(state.content_rect, 4), palette.frame_dark);
        }
        DialogFrameKind::PlainDialog => {
            ctx.frame_rect(state.content_rect, palette.frame_dark);
        }
        DialogFrameKind::MovableDialog => {
            ctx.frame_rect(state.frame_rect, palette.frame_dark);
            draw_two_pixel_frame(ctx, outset_rect(state.content_rect, 5), palette.frame_dark);
            draw_dialog_title_band(ctx, palette, state);
        }
        DialogFrameKind::Document | DialogFrameKind::NoGrowDocument => {
            ctx.frame_rect(state.frame_rect, palette.frame_dark);
            ctx.frame_rect(state.content_rect, palette.frame_dark);
            draw_dialog_title_band(ctx, palette, state);
            if state.active {
                draw_dialog_shadow(ctx, palette, state.content_rect);
            }
        }
        DialogFrameKind::AlertDialog | DialogFrameKind::Unknown => {
            ctx.frame_rect(state.content_rect, palette.frame_dark);
            draw_dialog_shadow(ctx, palette, state.content_rect);
        }
    }
}

fn draw_systemless_dialog_frame_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: DialogFrameState,
) {
    fill_dialog_frame_background(ctx, state, palette.window_background);
    ctx.frame_rect(state.frame_rect, palette.frame_dark);
    ctx.frame_rect(state.content_rect, palette.frame_dark);

    let inner_frame = inset_rect(state.frame_rect, 2);
    if inner_frame.top < inner_frame.bottom && inner_frame.left < inner_frame.right {
        ctx.frame_rect(inner_frame, palette.selection);
    }

    match state.kind {
        DialogFrameKind::DialogBox => {
            draw_dialog_frame_accent(ctx, palette, state.content_rect);
            ctx.frame_rect(outset_rect(state.content_rect, 4), palette.frame_dark);
        }
        DialogFrameKind::PlainDialog => {
            draw_dialog_frame_accent(ctx, palette, state.content_rect);
        }
        DialogFrameKind::MovableDialog => {
            draw_systemless_title_band(ctx, palette, state);
            draw_two_pixel_frame(ctx, outset_rect(state.content_rect, 5), palette.frame_dark);
        }
        DialogFrameKind::Document | DialogFrameKind::NoGrowDocument => {
            draw_systemless_title_band(ctx, palette, state);
            if state.active {
                draw_dialog_shadow(ctx, palette, state.content_rect);
            }
        }
        DialogFrameKind::AlertDialog | DialogFrameKind::Unknown => {
            draw_dialog_frame_accent(ctx, palette, state.content_rect);
            draw_dialog_shadow(ctx, palette, state.content_rect);
        }
    }
}

fn fill_dialog_frame_background(ctx: &mut ThemeDrawCtx<'_>, state: DialogFrameState, color: Rgb8) {
    fill_rect_excluding_inner(ctx, state.frame_rect, state.content_rect, color);
    if state.fill_content {
        ctx.fill_rect(state.content_rect, color);
    }
}

fn fill_rect_excluding_inner(
    ctx: &mut ThemeDrawCtx<'_>,
    outer: ThemeRect,
    inner: ThemeRect,
    color: Rgb8,
) {
    let inner_top = inner.top.max(outer.top).min(outer.bottom);
    let inner_left = inner.left.max(outer.left).min(outer.right);
    let inner_bottom = inner.bottom.max(outer.top).min(outer.bottom);
    let inner_right = inner.right.max(outer.left).min(outer.right);
    ctx.fill_rect(
        ThemeRect {
            top: outer.top,
            left: outer.left,
            bottom: inner_top,
            right: outer.right,
        },
        color,
    );
    ctx.fill_rect(
        ThemeRect {
            top: inner_bottom,
            left: outer.left,
            bottom: outer.bottom,
            right: outer.right,
        },
        color,
    );
    ctx.fill_rect(
        ThemeRect {
            top: inner_top,
            left: outer.left,
            bottom: inner_bottom,
            right: inner_left,
        },
        color,
    );
    ctx.fill_rect(
        ThemeRect {
            top: inner_top,
            left: inner_right,
            bottom: inner_bottom,
            right: outer.right,
        },
        color,
    );
}

fn draw_dialog_frame_accent(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    content: ThemeRect,
) {
    ctx.fill_rect(
        ThemeRect {
            top: content.top - 2,
            left: content.left + 3,
            bottom: content.top - 1,
            right: content.right - 3,
        },
        palette.selection,
    );
}

fn draw_dialog_title_band(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: DialogFrameState,
) {
    if state.frame_rect.top >= state.content_rect.top - 1 {
        return;
    }
    let band = ThemeRect {
        top: state.frame_rect.top + 1,
        left: state.frame_rect.left + 1,
        bottom: state.content_rect.top - 1,
        right: state.frame_rect.right - 1,
    };
    ctx.fill_rect(band, palette.frame_light);
    ctx.frame_rect(band, palette.frame_dark);
}

fn draw_systemless_title_band(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: DialogFrameState,
) {
    if state.frame_rect.top >= state.content_rect.top - 1 {
        return;
    }
    let band = ThemeRect {
        top: state.frame_rect.top + 2,
        left: state.frame_rect.left + 2,
        bottom: state.content_rect.top - 2,
        right: state.frame_rect.right - 2,
    };
    ctx.fill_rect(band, palette.frame_light);
    ctx.frame_rect(band, palette.selection);
}

fn draw_two_pixel_frame(ctx: &mut ThemeDrawCtx<'_>, rect: ThemeRect, color: Rgb8) {
    ctx.frame_rect(rect, color);
    ctx.frame_rect(inset_rect(rect, 1), color);
}

fn draw_dialog_shadow(ctx: &mut ThemeDrawCtx<'_>, palette: UiThemePalette, rect: ThemeRect) {
    ctx.fill_rect(
        ThemeRect {
            top: rect.top + 2,
            left: rect.right,
            bottom: rect.bottom + 2,
            right: rect.right + 2,
        },
        palette.frame_dark,
    );
    ctx.fill_rect(
        ThemeRect {
            top: rect.bottom,
            left: rect.left + 2,
            bottom: rect.bottom + 2,
            right: rect.right + 2,
        },
        palette.frame_dark,
    );
}

fn draw_menu_bar_chrome(ctx: &mut ThemeDrawCtx<'_>, palette: UiThemePalette, state: MenuBarState) {
    ctx.fill_rect(state.rect, palette.frame_light);
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.bottom - 1,
            left: state.rect.left,
            bottom: state.rect.bottom,
            right: state.rect.right,
        },
        palette.frame_dark,
    );
}

fn draw_systemless_menu_bar_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: MenuBarState,
) {
    ctx.fill_rect(state.rect, palette.window_background);
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.top,
            left: state.rect.left,
            bottom: state.rect.top + 1,
            right: state.rect.right,
        },
        palette.frame_dark,
    );
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.bottom - 1,
            left: state.rect.left,
            bottom: state.rect.bottom,
            right: state.rect.right,
        },
        palette.frame_dark,
    );
}

fn draw_menu_title_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: MenuTitleState,
) {
    if state.highlighted {
        ctx.fill_rect(state.rect, palette.selection);
    } else if !state.enabled {
        ctx.fill_rect(inset_rect(state.rect, 2), palette.window_background);
    }
}

fn draw_systemless_menu_title_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: MenuTitleState,
) {
    if state.highlighted {
        ctx.fill_rect(state.rect, palette.selection);
    } else if state.enabled {
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.bottom - 2,
                left: state.rect.left + 3,
                bottom: state.rect.bottom - 1,
                right: state.rect.right - 3,
            },
            palette.selection,
        );
    } else {
        ctx.frame_rect(inset_rect(state.rect, 2), palette.frame_dark);
    }
}

fn draw_menu_dropdown_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: MenuDropdownState,
) {
    ctx.fill_rect(state.rect, palette.frame_light);
    ctx.frame_rect(state.rect, palette.frame_dark);
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.top + 2,
            left: state.rect.right,
            bottom: state.rect.bottom + 1,
            right: state.rect.right + 1,
        },
        palette.frame_dark,
    );
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.bottom,
            left: state.rect.left + 2,
            bottom: state.rect.bottom + 1,
            right: state.rect.right + 1,
        },
        palette.frame_dark,
    );
}

fn draw_systemless_menu_dropdown_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: MenuDropdownState,
) {
    ctx.fill_rect(state.rect, palette.frame_light);
    ctx.frame_rect(state.rect, palette.frame_dark);
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.top + 2,
            left: state.rect.left + 2,
            bottom: state.rect.top + 3,
            right: state.rect.right - 2,
        },
        palette.selection,
    );
}

fn draw_menu_item_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: MenuItemState,
) {
    if state.highlighted {
        ctx.fill_rect(state.rect, palette.selection);
    }
    if state.separator {
        let y = state.rect.top + (state.rect.bottom - state.rect.top) / 2;
        let mut x = state.rect.left + 6;
        while x < state.rect.right - 6 {
            ctx.fill_rect(
                ThemeRect {
                    top: y,
                    left: x,
                    bottom: y + 1,
                    right: x + 1,
                },
                palette.frame_dark,
            );
            x += 2;
        }
    }
    if state.checked {
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.top + 4,
                left: state.rect.left + 5,
                bottom: state.rect.bottom - 4,
                right: state.rect.left + 7,
            },
            palette.frame_dark,
        );
    }
    if state.has_icon {
        ctx.frame_rect(
            ThemeRect {
                top: state.rect.top + 4,
                left: state.rect.left + 12,
                bottom: state.rect.bottom - 4,
                right: state.rect.left + 17,
            },
            palette.frame_dark,
        );
    }
    if state.has_command_key {
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.top + 6,
                left: state.rect.right - 18,
                bottom: state.rect.top + 8,
                right: state.rect.right - 8,
            },
            palette.frame_dark,
        );
    }
    if !state.enabled && !state.separator {
        let mut y = state.rect.top + 2;
        while y < state.rect.bottom {
            ctx.fill_rect(
                ThemeRect {
                    top: y,
                    left: state.rect.left + 2,
                    bottom: y + 1,
                    right: state.rect.right - 2,
                },
                palette.frame_light,
            );
            y += 2;
        }
    }
}

fn draw_systemless_menu_item_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: MenuItemState,
) {
    if state.separator {
        let y = state.rect.top + (state.rect.bottom - state.rect.top) / 2;
        ctx.fill_rect(
            ThemeRect {
                top: y,
                left: state.rect.left + 8,
                bottom: y + 1,
                right: state.rect.right - 8,
            },
            palette.selection,
        );
        return;
    }

    if state.highlighted {
        ctx.frame_rect(inset_rect(state.rect, 1), palette.selection);
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.top + 2,
                left: state.rect.left + 2,
                bottom: state.rect.bottom - 2,
                right: state.rect.left + 5,
            },
            palette.selection,
        );
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.bottom - 3,
                left: state.rect.left + 6,
                bottom: state.rect.bottom - 2,
                right: state.rect.right - 6,
            },
            palette.selection,
        );
    }

    if state.checked {
        ctx.frame_rect(
            ThemeRect {
                top: state.rect.top + 3,
                left: state.rect.left + 5,
                bottom: state.rect.bottom - 3,
                right: state.rect.left + 13,
            },
            palette.selection,
        );
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.bottom - 7,
                left: state.rect.left + 7,
                bottom: state.rect.bottom - 5,
                right: state.rect.left + 11,
            },
            palette.selection,
        );
    }

    if state.has_icon {
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.top + 5,
                left: state.rect.left + 14,
                bottom: state.rect.bottom - 5,
                right: state.rect.left + 17,
            },
            palette.selection,
        );
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.top + 7,
                left: state.rect.left + 12,
                bottom: state.rect.bottom - 7,
                right: state.rect.left + 19,
            },
            palette.selection,
        );
    }

    if state.has_command_key {
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.top + 4,
                left: state.rect.right - 15,
                bottom: state.rect.bottom - 4,
                right: state.rect.right - 13,
            },
            palette.selection,
        );
    }

    if !state.enabled {
        ctx.frame_rect(inset_rect(state.rect, 3), palette.frame_dark);
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.bottom - 4,
                left: state.rect.left + 6,
                bottom: state.rect.bottom - 3,
                right: state.rect.right - 6,
            },
            palette.frame_dark,
        );
    }
}

fn draw_text_field_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: TextFieldState,
) {
    let fill = if state.enabled {
        palette.frame_light
    } else {
        palette.window_background
    };
    ctx.fill_rect(state.rect, fill);
    ctx.frame_rect(state.rect, palette.frame_dark);
}

fn draw_systemless_text_field_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: TextFieldState,
) {
    let fill = if state.enabled {
        palette.frame_light
    } else {
        palette.window_background
    };
    ctx.fill_rect(state.rect, fill);
    ctx.frame_rect(state.rect, palette.frame_dark);
    if state.focused {
        ctx.frame_rect(inset_rect(state.rect, 2), palette.selection);
    } else if !state.enabled {
        ctx.frame_rect(inset_rect(state.rect, 2), palette.frame_dark);
    }
}

fn draw_text_selection_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: TextSelectionState,
) {
    if state.active {
        ctx.fill_rect(state.rect, palette.selection);
    }
}

fn draw_systemless_text_selection_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: TextSelectionState,
) {
    if !state.active {
        ctx.frame_rect(state.rect, palette.selection);
        return;
    }
    ctx.frame_rect(state.rect, palette.selection);
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.bottom - 2,
            left: state.rect.left + 1,
            bottom: state.rect.bottom,
            right: state.rect.right - 1,
        },
        palette.selection,
    );
}

fn draw_caret_chrome(ctx: &mut ThemeDrawCtx<'_>, palette: UiThemePalette, state: CaretState) {
    if state.active {
        ctx.fill_rect(state.rect, palette.selection);
    }
}

fn draw_systemless_caret_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: CaretState,
) {
    if !state.active {
        return;
    }
    ctx.fill_rect(state.rect, palette.selection);
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.top,
            left: state.rect.left - 1,
            bottom: state.rect.top + 1,
            right: state.rect.right + 1,
        },
        palette.selection,
    );
    ctx.fill_rect(
        ThemeRect {
            top: state.rect.bottom - 1,
            left: state.rect.left - 1,
            bottom: state.rect.bottom,
            right: state.rect.right + 1,
        },
        palette.selection,
    );
}

fn draw_control_chrome(ctx: &mut ThemeDrawCtx<'_>, palette: UiThemePalette, state: ControlState) {
    match state.kind {
        ControlKind::PushButton => draw_button_chrome(ctx, palette, state),
        ControlKind::Checkbox => draw_checkbox_chrome(ctx, palette, state),
        ControlKind::RadioButton => draw_radio_button_chrome(ctx, palette, state),
        ControlKind::PopupButton => draw_popup_button_chrome(ctx, palette, state),
    }
}

fn draw_systemless_control_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: ControlState,
) {
    draw_control_chrome(ctx, palette, state);
    match state.kind {
        ControlKind::Checkbox if state.selected => {
            ctx.fill_rect(inset_rect(state.rect, 3), palette.selection);
        }
        ControlKind::RadioButton if state.selected => {
            ctx.fill_rect(inset_rect(state.rect, 2), palette.selection);
        }
        ControlKind::PopupButton => {
            let indicator = popup_indicator_rect(state.rect);
            ctx.fill_rect(indicator, palette.selection);
            ctx.frame_rect(indicator, palette.frame_light);
        }
        _ => {}
    }
    if !state.enabled {
        ctx.frame_rect(inset_rect(state.rect, 2), palette.frame_dark);
    }
}

fn draw_button_chrome(ctx: &mut ThemeDrawCtx<'_>, palette: UiThemePalette, state: ControlState) {
    let fill = if state.enabled {
        palette.frame_light
    } else {
        palette.window_background
    };
    ctx.fill_rect(state.rect, fill);
    ctx.frame_rect(state.rect, palette.frame_dark);
    if state.pressed {
        ctx.fill_rect(
            ThemeRect {
                top: state.rect.top + 2,
                left: state.rect.left + 2,
                bottom: state.rect.bottom - 2,
                right: state.rect.right - 2,
            },
            palette.selection,
        );
    }
    if state.is_default {
        ctx.frame_rect(
            ThemeRect {
                top: state.rect.top - 4,
                left: state.rect.left - 4,
                bottom: state.rect.bottom + 4,
                right: state.rect.right + 4,
            },
            palette.selection,
        );
    }
}

fn draw_checkbox_chrome(ctx: &mut ThemeDrawCtx<'_>, palette: UiThemePalette, state: ControlState) {
    let fill = if state.enabled {
        palette.frame_light
    } else {
        palette.window_background
    };
    ctx.fill_rect(state.rect, fill);
    ctx.frame_rect(state.rect, palette.frame_dark);
    if state.pressed {
        ctx.fill_rect(inset_rect(state.rect, 2), palette.selection);
    }
    if state.selected {
        let size = (state.rect.bottom - state.rect.top).min(state.rect.right - state.rect.left);
        for i in 2..size - 2 {
            ctx.fill_rect(
                ThemeRect {
                    top: state.rect.top + i,
                    left: state.rect.left + i,
                    bottom: state.rect.top + i + 1,
                    right: state.rect.left + i + 1,
                },
                palette.selection,
            );
            ctx.fill_rect(
                ThemeRect {
                    top: state.rect.top + i,
                    left: state.rect.right - 1 - i,
                    bottom: state.rect.top + i + 1,
                    right: state.rect.right - i,
                },
                palette.selection,
            );
        }
    }
}

fn draw_radio_button_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: ControlState,
) {
    let fill = if state.enabled {
        palette.frame_light
    } else {
        palette.window_background
    };
    ctx.fill_rect(state.rect, fill);
    ctx.frame_rect(state.rect, palette.frame_dark);
    if state.pressed {
        ctx.fill_rect(inset_rect(state.rect, 2), palette.selection);
    }
    if state.selected {
        ctx.fill_rect(inset_rect(state.rect, 3), palette.selection);
    }
}

fn draw_popup_button_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: ControlState,
) {
    let fill = if state.enabled {
        palette.frame_light
    } else {
        palette.window_background
    };
    ctx.fill_rect(state.rect, fill);
    ctx.frame_rect(state.rect, palette.frame_dark);
    if state.pressed {
        ctx.fill_rect(inset_rect(state.rect, 2), palette.selection);
    }

    let indicator = popup_indicator_rect(state.rect);
    ctx.frame_rect(indicator, palette.frame_dark);
    let mid_y = (indicator.top + indicator.bottom) / 2;
    let mid_x = (indicator.left + indicator.right) / 2;
    for row in 0..4 {
        ctx.fill_rect(
            ThemeRect {
                top: mid_y - 2 + row,
                left: mid_x - 3 + row,
                bottom: mid_y - 1 + row,
                right: mid_x + 4 - row,
            },
            palette.selection,
        );
    }
}

fn draw_scrollbar_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: ScrollbarState,
) {
    if state.rect.top >= state.rect.bottom || state.rect.left >= state.rect.right {
        return;
    }

    ctx.fill_rect(state.rect, palette.frame_light);
    ctx.frame_rect(state.rect, palette.frame_dark);

    let (decrement, increment) = scrollbar_arrow_rects(state);
    let track = scrollbar_track_rect(state);
    ctx.fill_rect(track, palette.window_background);
    if state.enabled && state.min < state.max {
        draw_scrollbar_pattern(ctx, track, palette.selection);
    }

    if let Some(thumb) = scrollbar_thumb_rect(state) {
        let (page_before, page_after) = scrollbar_page_rects(state, thumb);
        if state.highlighted_part == ScrollbarPart::PageBefore {
            ctx.fill_rect(page_before, palette.selection);
        }
        if state.highlighted_part == ScrollbarPart::PageAfter {
            ctx.fill_rect(page_after, palette.selection);
        }
        let thumb_fill = if state.highlighted_part == ScrollbarPart::Thumb {
            palette.selection
        } else {
            palette.frame_light
        };
        ctx.fill_rect(thumb, thumb_fill);
        ctx.frame_rect(thumb, palette.frame_dark);
        draw_scrollbar_grip(ctx, palette, state.orientation, thumb);
    }

    draw_scrollbar_button(
        ctx,
        palette,
        state.orientation,
        decrement,
        true,
        state.highlighted_part == ScrollbarPart::DecrementArrow,
    );
    draw_scrollbar_button(
        ctx,
        palette,
        state.orientation,
        increment,
        false,
        state.highlighted_part == ScrollbarPart::IncrementArrow,
    );
}

fn draw_systemless_scrollbar_chrome(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    state: ScrollbarState,
) {
    if state.rect.top >= state.rect.bottom || state.rect.left >= state.rect.right {
        return;
    }

    ctx.fill_rect(state.rect, palette.window_background);
    ctx.frame_rect(state.rect, palette.frame_dark);

    let (decrement, increment) = scrollbar_arrow_rects(state);
    let track = scrollbar_track_rect(state);
    ctx.fill_rect(track, palette.frame_light);
    ctx.frame_rect(track, palette.frame_dark);

    if let Some(thumb) = scrollbar_thumb_rect(state) {
        let (page_before, page_after) = scrollbar_page_rects(state, thumb);
        if state.highlighted_part == ScrollbarPart::PageBefore {
            ctx.fill_rect(inset_rect(page_before, 1), palette.selection);
        }
        if state.highlighted_part == ScrollbarPart::PageAfter {
            ctx.fill_rect(inset_rect(page_after, 1), palette.selection);
        }
        let thumb_fill = if state.highlighted_part == ScrollbarPart::Thumb {
            palette.frame_dark
        } else {
            palette.selection
        };
        ctx.fill_rect(inset_rect(thumb, 1), thumb_fill);
        ctx.frame_rect(thumb, palette.frame_dark);
    }

    draw_systemless_scrollbar_button(
        ctx,
        palette,
        state.orientation,
        decrement,
        true,
        state.highlighted_part == ScrollbarPart::DecrementArrow,
    );
    draw_systemless_scrollbar_button(
        ctx,
        palette,
        state.orientation,
        increment,
        false,
        state.highlighted_part == ScrollbarPart::IncrementArrow,
    );

    if !state.enabled || state.min >= state.max {
        ctx.frame_rect(inset_rect(state.rect, 3), palette.frame_dark);
    }
}

fn scrollbar_arrow_extent(state: ScrollbarState) -> i16 {
    let axis_len = match state.orientation {
        ScrollbarOrientation::Vertical => state.rect.bottom - state.rect.top,
        ScrollbarOrientation::Horizontal => state.rect.right - state.rect.left,
    };
    axis_len.max(0).min(16)
}

fn scrollbar_arrow_rects(state: ScrollbarState) -> (ThemeRect, ThemeRect) {
    let extent = scrollbar_arrow_extent(state);
    match state.orientation {
        ScrollbarOrientation::Vertical => (
            ThemeRect {
                top: state.rect.top,
                left: state.rect.left,
                bottom: state.rect.top + extent,
                right: state.rect.right,
            },
            ThemeRect {
                top: state.rect.bottom - extent,
                left: state.rect.left,
                bottom: state.rect.bottom,
                right: state.rect.right,
            },
        ),
        ScrollbarOrientation::Horizontal => (
            ThemeRect {
                top: state.rect.top,
                left: state.rect.left,
                bottom: state.rect.bottom,
                right: state.rect.left + extent,
            },
            ThemeRect {
                top: state.rect.top,
                left: state.rect.right - extent,
                bottom: state.rect.bottom,
                right: state.rect.right,
            },
        ),
    }
}

fn scrollbar_track_rect(state: ScrollbarState) -> ThemeRect {
    let extent = scrollbar_arrow_extent(state);
    match state.orientation {
        ScrollbarOrientation::Vertical => ThemeRect {
            top: state.rect.top + extent,
            left: state.rect.left + 1,
            bottom: state.rect.bottom - extent,
            right: state.rect.right - 1,
        },
        ScrollbarOrientation::Horizontal => ThemeRect {
            top: state.rect.top + 1,
            left: state.rect.left + extent,
            bottom: state.rect.bottom - 1,
            right: state.rect.right - extent,
        },
    }
}

fn scrollbar_thumb_rect(state: ScrollbarState) -> Option<ThemeRect> {
    if !state.enabled || state.min >= state.max {
        return None;
    }

    let track = scrollbar_track_rect(state);
    let track_len = match state.orientation {
        ScrollbarOrientation::Vertical => track.bottom - track.top,
        ScrollbarOrientation::Horizontal => track.right - track.left,
    };
    if track_len <= 0 {
        return None;
    }

    let thumb_len = track_len.min(16);
    let range = i32::from(state.max) - i32::from(state.min);
    if range <= 0 {
        return None;
    }
    let value = (i32::from(state.value) - i32::from(state.min)).clamp(0, range);
    let travel = i32::from(track_len - thumb_len);
    let offset = ((value * travel) / range) as i16;

    Some(match state.orientation {
        ScrollbarOrientation::Vertical => ThemeRect {
            top: track.top + offset,
            left: state.rect.left,
            bottom: track.top + offset + thumb_len,
            right: state.rect.right,
        },
        ScrollbarOrientation::Horizontal => ThemeRect {
            top: state.rect.top,
            left: track.left + offset,
            bottom: state.rect.bottom,
            right: track.left + offset + thumb_len,
        },
    })
}

fn scrollbar_page_rects(state: ScrollbarState, thumb: ThemeRect) -> (ThemeRect, ThemeRect) {
    let track = scrollbar_track_rect(state);
    match state.orientation {
        ScrollbarOrientation::Vertical => (
            ThemeRect {
                top: track.top,
                left: track.left,
                bottom: thumb.top,
                right: track.right,
            },
            ThemeRect {
                top: thumb.bottom,
                left: track.left,
                bottom: track.bottom,
                right: track.right,
            },
        ),
        ScrollbarOrientation::Horizontal => (
            ThemeRect {
                top: track.top,
                left: track.left,
                bottom: track.bottom,
                right: thumb.left,
            },
            ThemeRect {
                top: track.top,
                left: thumb.right,
                bottom: track.bottom,
                right: track.right,
            },
        ),
    }
}

fn draw_scrollbar_pattern(ctx: &mut ThemeDrawCtx<'_>, rect: ThemeRect, color: Rgb8) {
    for y in rect.top..rect.bottom {
        for x in rect.left..rect.right {
            if (x + y) & 3 == 0 {
                ctx.fill_rect(
                    ThemeRect {
                        top: y,
                        left: x,
                        bottom: y + 1,
                        right: x + 1,
                    },
                    color,
                );
            }
        }
    }
}

fn draw_scrollbar_button(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    orientation: ScrollbarOrientation,
    rect: ThemeRect,
    decrement: bool,
    highlighted: bool,
) {
    ctx.fill_rect(
        inset_rect(rect, 1),
        if highlighted {
            palette.selection
        } else {
            palette.frame_light
        },
    );
    ctx.frame_rect(rect, palette.frame_dark);
    draw_scrollbar_arrow(ctx, palette, orientation, rect, decrement);
}

fn draw_systemless_scrollbar_button(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    orientation: ScrollbarOrientation,
    rect: ThemeRect,
    decrement: bool,
    highlighted: bool,
) {
    ctx.fill_rect(
        inset_rect(rect, 1),
        if highlighted {
            palette.selection
        } else {
            palette.window_background
        },
    );
    ctx.frame_rect(rect, palette.frame_dark);
    draw_scrollbar_arrow(ctx, palette, orientation, rect, decrement);
}

fn draw_scrollbar_arrow(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    orientation: ScrollbarOrientation,
    rect: ThemeRect,
    decrement: bool,
) {
    let center_y = (rect.top + rect.bottom) / 2;
    let center_x = (rect.left + rect.right) / 2;
    for row in 0..5 {
        let half = row.min(4 - row) as i16;
        let (top, left, bottom, right) = match (orientation, decrement) {
            (ScrollbarOrientation::Vertical, true) => (
                center_y - 3 + row as i16,
                center_x - half,
                center_y - 2 + row as i16,
                center_x + half + 1,
            ),
            (ScrollbarOrientation::Vertical, false) => (
                center_y + 1 - row as i16,
                center_x - half,
                center_y + 2 - row as i16,
                center_x + half + 1,
            ),
            (ScrollbarOrientation::Horizontal, true) => (
                center_y - half,
                center_x - 3 + row as i16,
                center_y + half + 1,
                center_x - 2 + row as i16,
            ),
            (ScrollbarOrientation::Horizontal, false) => (
                center_y - half,
                center_x + 1 - row as i16,
                center_y + half + 1,
                center_x + 2 - row as i16,
            ),
        };
        ctx.fill_rect(
            ThemeRect {
                top,
                left,
                bottom,
                right,
            },
            palette.frame_dark,
        );
    }
}

fn draw_scrollbar_grip(
    ctx: &mut ThemeDrawCtx<'_>,
    palette: UiThemePalette,
    orientation: ScrollbarOrientation,
    rect: ThemeRect,
) {
    match orientation {
        ScrollbarOrientation::Vertical => {
            let left = (rect.left + rect.right) / 2 - 3;
            for y in [rect.top + 5, rect.top + 7, rect.top + 9, rect.top + 11] {
                ctx.fill_rect(
                    ThemeRect {
                        top: y,
                        left,
                        bottom: y + 1,
                        right: left + 6,
                    },
                    palette.frame_dark,
                );
            }
        }
        ScrollbarOrientation::Horizontal => {
            let top = (rect.top + rect.bottom) / 2 - 3;
            for x in [rect.left + 5, rect.left + 7, rect.left + 9, rect.left + 11] {
                ctx.fill_rect(
                    ThemeRect {
                        top,
                        left: x,
                        bottom: top + 6,
                        right: x + 1,
                    },
                    palette.frame_dark,
                );
            }
        }
    }
}

fn popup_indicator_rect(rect: ThemeRect) -> ThemeRect {
    ThemeRect {
        top: rect.top + 2,
        left: rect.right - 18,
        bottom: rect.bottom - 2,
        right: rect.right - 2,
    }
}

fn inset_rect(rect: ThemeRect, inset: i16) -> ThemeRect {
    ThemeRect {
        top: rect.top + inset,
        left: rect.left + inset,
        bottom: rect.bottom - inset,
        right: rect.right - inset,
    }
}

fn outset_rect(rect: ThemeRect, outset: i16) -> ThemeRect {
    ThemeRect {
        top: rect.top - outset,
        left: rect.left - outset,
        bottom: rect.bottom + outset,
        right: rect.right + outset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb_at(bitmap: &ThemeBitmap, x: u32, y: u32) -> [u8; 3] {
        let idx = ((y * bitmap.width() + x) * 4) as usize;
        [
            bitmap.rgba()[idx],
            bitmap.rgba()[idx + 1],
            bitmap.rgba()[idx + 2],
        ]
    }

    #[test]
    fn parses_known_theme_ids_and_rejects_unknown_ids() {
        assert_eq!(
            UiThemeId::parse(CLASSIC_SYSTEM7_THEME),
            Ok(UiThemeId::ClassicSystem7)
        );
        assert_eq!(
            UiThemeId::parse(SYSTEMLESS_DEFAULT_THEME),
            Ok(UiThemeId::SystemlessDefault)
        );
        assert!(UiThemeId::parse("../bad").is_err());
    }

    #[test]
    fn parses_metrics_modes_and_marks_classic_guest_metrics() {
        assert_eq!(
            ThemeMetricsMode::parse(CLASSIC_GUEST_METRICS),
            Ok(ThemeMetricsMode::ClassicGuestMetrics)
        );
        assert_eq!(
            ThemeMetricsMode::parse(THEMED_GUEST_METRICS),
            Ok(ThemeMetricsMode::ThemedGuestMetrics)
        );
        assert!(ThemeMetricsMode::ClassicGuestMetrics.preserves_classic_guest_metrics());
        assert!(!ThemeMetricsMode::ThemedGuestMetrics.preserves_classic_guest_metrics());
    }

    #[test]
    fn systemless_theme_keeps_classic_guest_metrics_but_has_distinct_palette() {
        let classic = UiThemeId::ClassicSystem7.provider();
        let themed = UiThemeId::SystemlessDefault.provider();

        assert_eq!(
            classic.metrics_mode(),
            ThemeMetricsMode::ClassicGuestMetrics
        );
        assert_eq!(themed.metrics_mode(), ThemeMetricsMode::ClassicGuestMetrics);
        assert_eq!(classic.dialog_metrics(), themed.dialog_metrics());
        assert_eq!(classic.menu_metrics(), themed.menu_metrics());
        assert_eq!(classic.control_metrics(), themed.control_metrics());
        assert_eq!(classic.text_theme(), themed.text_theme());
        assert_ne!(classic.palette(), themed.palette());
    }

    #[test]
    fn basic_button_preview_routes_through_theme_provider() {
        let classic = render_basic_button_preview(UiThemeId::ClassicSystem7);
        let themed = render_basic_button_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 96);
        assert_eq!(classic.height(), 48);
        assert_eq!(classic.rgba().len(), 96 * 48 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn basic_controls_preview_routes_checkbox_and_radio_through_theme_provider() {
        let classic = render_basic_controls_preview(UiThemeId::ClassicSystem7);
        let themed = render_basic_controls_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 120);
        assert_eq!(classic.height(), 72);
        assert_eq!(classic.rgba().len(), 120 * 72 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn basic_dialog_preview_routes_composed_dialog_surface_through_theme_provider() {
        let classic = render_basic_dialog_preview(UiThemeId::ClassicSystem7);
        let themed = render_basic_dialog_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 224);
        assert_eq!(classic.height(), 148);
        assert_eq!(classic.rgba().len(), 224 * 148 * 4);
        assert_ne!(classic.rgba(), themed.rgba());

        let palette = UiThemeId::SystemlessDefault.provider().palette();
        let selection = [
            palette.selection.r,
            palette.selection.g,
            palette.selection.b,
        ];

        assert_eq!(
            rgb_at(&themed, 24, 16),
            selection,
            "dialog-box accent should use provider selection chrome"
        );
        assert_eq!(
            rgb_at(&themed, 36, 40),
            selection,
            "focused editText field should use provider focus chrome"
        );
        assert_eq!(
            rgb_at(&themed, 116, 96),
            selection,
            "popup control indicator should use provider chrome"
        );
        assert_eq!(
            rgb_at(&themed, 148, 98),
            selection,
            "default push button outline should use provider chrome"
        );
    }

    #[test]
    fn control_states_preview_routes_pressed_and_disabled_states_through_theme_provider() {
        let classic = render_control_states_preview(UiThemeId::ClassicSystem7);
        let themed = render_control_states_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 176);
        assert_eq!(classic.height(), 96);
        assert_eq!(classic.rgba().len(), 176 * 96 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn popup_control_preview_routes_through_theme_provider() {
        let classic = render_popup_control_preview(UiThemeId::ClassicSystem7);
        let themed = render_popup_control_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 128);
        assert_eq!(classic.height(), 48);
        assert_eq!(classic.rgba().len(), 128 * 48 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn scrollbars_preview_routes_through_theme_provider() {
        let classic = render_scrollbars_preview(UiThemeId::ClassicSystem7);
        let themed = render_scrollbars_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 144);
        assert_eq!(classic.height(), 104);
        assert_eq!(classic.rgba().len(), 144 * 104 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn menu_chrome_preview_routes_through_theme_provider() {
        let classic = render_menu_chrome_preview(UiThemeId::ClassicSystem7);
        let themed = render_menu_chrome_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 160);
        assert_eq!(classic.height(), 96);
        assert_eq!(classic.rgba().len(), 160 * 96 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn menu_items_preview_routes_semantic_states_through_theme_provider() {
        let classic = render_menu_items_preview(UiThemeId::ClassicSystem7);
        let themed = render_menu_items_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 176);
        assert_eq!(classic.height(), 104);
        assert_eq!(classic.rgba().len(), 176 * 104 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn edit_text_selection_preview_routes_through_theme_provider() {
        let classic = render_edit_text_selection_preview(UiThemeId::ClassicSystem7);
        let themed = render_edit_text_selection_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 176);
        assert_eq!(classic.height(), 104);
        assert_eq!(classic.rgba().len(), 176 * 104 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn text_field_states_preview_routes_enabled_focus_and_disabled_through_theme_provider() {
        let classic = render_text_field_states_preview(UiThemeId::ClassicSystem7);
        let themed = render_text_field_states_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 176);
        assert_eq!(classic.height(), 92);
        assert_eq!(classic.rgba().len(), 176 * 92 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn inactive_text_caret_state_is_provider_noop() {
        let theme = UiThemeId::SystemlessDefault.provider();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(24, 24, palette.window_background);
        let before = bitmap.rgba().to_vec();

        {
            let mut ctx = ThemeDrawCtx::new(&mut bitmap);
            // Text 1993 appendix C: inactive TextEdit records do not
            // display the caret, even though their selection offsets remain.
            theme.draw_caret(
                &mut ctx,
                CaretState {
                    rect: ThemeRect {
                        top: 4,
                        left: 12,
                        bottom: 20,
                        right: 13,
                    },
                    active: false,
                },
            );
        }

        assert_eq!(bitmap.rgba(), before.as_slice());
    }

    #[test]
    fn dialog_frame_preview_routes_through_theme_provider() {
        let classic = render_dialog_frame_preview(UiThemeId::ClassicSystem7);
        let themed = render_dialog_frame_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 184);
        assert_eq!(classic.height(), 112);
        assert_eq!(classic.rgba().len(), 184 * 112 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }

    #[test]
    fn dialog_frame_states_preview_routes_kind_active_and_fill_states_through_provider() {
        let classic = render_dialog_frame_states_preview(UiThemeId::ClassicSystem7);
        let themed = render_dialog_frame_states_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 216);
        assert_eq!(classic.height(), 128);
        assert_eq!(classic.rgba().len(), 216 * 128 * 4);
        assert_ne!(classic.rgba(), themed.rgba());

        let palette = UiThemeId::SystemlessDefault.provider().palette();
        assert_eq!(
            rgb_at(&themed, 32, 32),
            [
                palette.window_background.r,
                palette.window_background.g,
                palette.window_background.b,
            ],
            "filled dialog content should use the provider window background"
        );
        assert_eq!(
            rgb_at(&themed, 160, 100),
            [
                palette.frame_light.r,
                palette.frame_light.g,
                palette.frame_light.b
            ],
            "inactive no-fill dialog content should preserve the preview canvas"
        );
    }

    #[test]
    fn window_frame_states_preview_routes_through_theme_provider() {
        let classic = render_window_frame_states_preview(UiThemeId::ClassicSystem7);
        let themed = render_window_frame_states_preview(UiThemeId::SystemlessDefault);

        assert_eq!(classic.width(), 216);
        assert_eq!(classic.height(), 128);
        assert_eq!(classic.rgba().len(), 216 * 128 * 4);
        assert_ne!(classic.rgba(), themed.rgba());
    }
}
