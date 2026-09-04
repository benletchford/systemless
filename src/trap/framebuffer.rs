//! Framebuffer drawing methods for menu bar and window chrome rendering.

use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use crate::memory::{MacMemoryBus, MemoryBus};
use crate::menu_manager::{
    for_each_standard_menu_bar_corner_pixel, is_standard_system_menu_title,
    standard_menu_bar_system_mark_top, standard_menu_bar_title_baseline,
    standard_menu_title_advance, TrackedMenuPaneView,
};
use crate::quickdraw::fonts::{heuristics::get_italic_slant, Glyph};
use crate::quickdraw::text::{
    get_font_metrics, get_glyph, get_glyph_italic, get_underline_thickness, QuickDrawTextStyle,
};
use crate::ui_theme::{
    render_scrollbar_bitmap, CaretState, ControlKind, ControlState, DialogFrameKind,
    DialogFrameState, MenuBarState, MenuDropdownState, MenuItemState, MenuTitleState, Rgb8,
    TextFieldState, TextSelectionState, ThemeBitmap, ThemeDrawCtx, ThemeRect, UiThemeId,
};

/// Opt-in gate for visRgn auto-expansion. Setting
/// `SYSTEMLESS_NO_VISRGN_AUTO_EXPAND=1` skips expanding the front window's
/// visRgn.top when MBarHeight=0. Cached via OnceLock to keep the per-frame
/// redraw path syscall-free.
static NO_VISRGN_AUTO_EXPAND: OnceLock<bool> = OnceLock::new();
fn no_visrgn_auto_expand_enabled() -> bool {
    *NO_VISRGN_AUTO_EXPAND
        .get_or_init(|| std::env::var_os("SYSTEMLESS_NO_VISRGN_AUTO_EXPAND").is_some())
}

const STANDARD_GRAY_PATTERN: [u8; 8] = [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55];
impl super::TrapDispatcher {
    /// Read screen parameters from the dispatcher's screen_mode.
    /// Returns (screen_base, row_bytes, width, height, pixel_size).
    pub(super) fn get_screen_params(&self) -> (u32, u32, i16, i16, u16) {
        let (base, rb, w, h, ps) = self.screen_mode;
        (base, rb, w as i16, h as i16, ps)
    }

    pub(crate) fn draw_theme_push_button_chrome(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        enabled: bool,
        pressed: bool,
        is_default: bool,
    ) -> bool {
        self.draw_theme_control_chrome(
            bus,
            ControlKind::PushButton,
            top,
            left,
            bottom,
            right,
            enabled,
            pressed,
            false,
            is_default,
        )
    }

    pub(crate) fn draw_theme_control_chrome(
        &self,
        bus: &mut MacMemoryBus,
        kind: ControlKind,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        enabled: bool,
        pressed: bool,
        selected: bool,
        is_default: bool,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let pad = if kind == ControlKind::PushButton && is_default {
            theme.dialog_metrics().default_button_outline.max(0)
        } else {
            0
        };
        let bitmap_width = width.saturating_add(pad.saturating_mul(2)) as u32;
        let bitmap_height = height.saturating_add(pad.saturating_mul(2)) as u32;
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(bitmap_width, bitmap_height, palette.window_background);
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_control(
            &mut ctx,
            ControlState {
                kind,
                rect: ThemeRect {
                    top: pad,
                    left: pad,
                    bottom: pad + height,
                    right: pad + width,
                },
                enabled,
                pressed,
                selected,
                is_default,
            },
        );

        self.blit_theme_bitmap_mono(
            bus,
            top.saturating_sub(pad),
            left.saturating_sub(pad),
            &bitmap,
        );
        true
    }

    pub(crate) fn draw_theme_scrollbar_chrome(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        value: i16,
        min: i16,
        max: i16,
        hilite: u8,
    ) -> bool {
        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }
        let bitmap = render_scrollbar_bitmap(
            self.ui_theme_id(),
            width,
            height,
            value,
            min,
            max,
            hilite,
        );
        self.blit_theme_bitmap_mono(bus, top, left, &bitmap);
        true
    }

    pub(crate) fn draw_theme_menu_bar_chrome(&self, bus: &mut MacMemoryBus, height: i16) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let (_, _, screen_width, _, _) = self.get_screen_params();
        if screen_width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(
            screen_width as u32,
            height as u32,
            palette.window_background,
        );
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_menu_bar(
            &mut ctx,
            MenuBarState {
                rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: screen_width,
                },
            },
        );
        self.blit_theme_bitmap_mono(bus, 0, 0, &bitmap);
        true
    }

    pub(crate) fn draw_theme_menu_title_chrome(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        enabled: bool,
        highlighted: bool,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(width as u32, height as u32, palette.window_background);
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_menu_title(
            &mut ctx,
            MenuTitleState {
                rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: width,
                },
                enabled,
                highlighted,
            },
        );
        self.blit_theme_bitmap_mono(bus, top, left, &bitmap);
        true
    }

    pub(crate) fn draw_theme_menu_dropdown_chrome(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(width as u32, height as u32, palette.window_background);
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_menu_dropdown(
            &mut ctx,
            MenuDropdownState {
                rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: width,
                },
            },
        );
        self.blit_theme_bitmap_mono(bus, top, left, &bitmap);
        true
    }

    pub(crate) fn draw_theme_menu_item_chrome(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        enabled: bool,
        highlighted: bool,
        separator: bool,
        has_icon: bool,
        checked: bool,
        has_command_key: bool,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(width as u32, height as u32, palette.frame_light);
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_menu_item(
            &mut ctx,
            MenuItemState {
                rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: width,
                },
                enabled,
                highlighted,
                separator,
                has_icon,
                checked,
                has_command_key,
            },
        );
        self.blit_theme_bitmap_mono(bus, top, left, &bitmap);
        true
    }

    pub(crate) fn draw_theme_text_selection(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        active: bool,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(width as u32, height as u32, palette.window_background);
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_text_selection(
            &mut ctx,
            TextSelectionState {
                rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: width,
                },
                active,
            },
        );
        self.blit_theme_bitmap_mono_masked(bus, top, left, &bitmap);
        true
    }

    pub(crate) fn draw_theme_text_field(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        enabled: bool,
        focused: bool,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(width as u32, height as u32, palette.window_background);
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_text_field(
            &mut ctx,
            TextFieldState {
                rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: width,
                },
                enabled,
                focused,
            },
        );
        self.blit_theme_bitmap_mono(bus, top, left, &bitmap);
        true
    }

    pub(crate) fn draw_theme_caret(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        if width <= 0 || height <= 0 {
            return true;
        }

        let theme = self.ui_theme();
        let pad = 1i16;
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(
            width.saturating_add(pad.saturating_mul(2)) as u32,
            height as u32,
            palette.window_background,
        );
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_caret(
            &mut ctx,
            CaretState {
                rect: ThemeRect {
                    top: 0,
                    left: pad,
                    bottom: height,
                    right: pad + width,
                },
                active: true,
            },
        );
        self.blit_theme_bitmap_mono_masked(bus, top, left.saturating_sub(pad), &bitmap);
        true
    }

    pub(crate) fn draw_theme_dialog_frame(
        &self,
        bus: &mut MacMemoryBus,
        content: (i16, i16, i16, i16),
        frame: (i16, i16, i16, i16),
        proc_id: i16,
        active: bool,
        fill_content: bool,
    ) -> bool {
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
            return false;
        }

        let (frame_top, frame_left, frame_bottom, frame_right) = frame;
        let width = frame_right.saturating_sub(frame_left);
        let height = frame_bottom.saturating_sub(frame_top);
        if width <= 0 || height <= 0 {
            return true;
        }

        if !fill_content {
            self.erase_structure_frame_around_content(bus, frame, content);
        }

        let (content_top, content_left, content_bottom, content_right) = content;
        let theme = self.ui_theme();
        let palette = theme.palette();
        let mut bitmap = ThemeBitmap::new(width as u32, height as u32, palette.window_background);
        let mut ctx = ThemeDrawCtx::new(&mut bitmap);
        theme.draw_dialog_frame(
            &mut ctx,
            DialogFrameState {
                frame_rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: width,
                },
                content_rect: ThemeRect {
                    top: content_top.saturating_sub(frame_top),
                    left: content_left.saturating_sub(frame_left),
                    bottom: content_bottom.saturating_sub(frame_top),
                    right: content_right.saturating_sub(frame_left),
                },
                kind: DialogFrameKind::from_window_proc_id(proc_id),
                active,
                fill_content,
            },
        );
        if fill_content {
            self.blit_theme_bitmap_mono(bus, frame_top, frame_left, &bitmap);
        } else {
            self.blit_theme_bitmap_mono_masked(bus, frame_top, frame_left, &bitmap);
        }
        true
    }

    fn blit_theme_bitmap_mono(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bitmap: &ThemeBitmap,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let rgba = bitmap.rgba();
        let mut pixels = HashMap::new();
        for y in 0..bitmap.height() {
            for x in 0..bitmap.width() {
                let offset = ((y * bitmap.width() + x) * 4) as usize;
                let color = Rgb8 {
                    r: rgba[offset],
                    g: rgba[offset + 1],
                    b: rgba[offset + 2],
                };
                let pixel = *pixels
                    .entry((color.r, color.g, color.b))
                    .or_insert_with(|| self.theme_pixel_index(bus, color));
                Self::fb_set_pixel_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left.saturating_add(x as i16),
                    top.saturating_add(y as i16),
                    pixel,
                );
            }
        }
    }

    fn blit_theme_bitmap_mono_masked(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bitmap: &ThemeBitmap,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let rgba = bitmap.rgba();
        let transparent = self.ui_theme().palette().window_background;
        let mut pixels = HashMap::new();
        for y in 0..bitmap.height() {
            for x in 0..bitmap.width() {
                let offset = ((y * bitmap.width() + x) * 4) as usize;
                let color = Rgb8 {
                    r: rgba[offset],
                    g: rgba[offset + 1],
                    b: rgba[offset + 2],
                };
                if color == transparent {
                    continue;
                }
                let pixel = *pixels
                    .entry((color.r, color.g, color.b))
                    .or_insert_with(|| self.theme_pixel_index(bus, color));
                Self::fb_set_pixel_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left.saturating_add(x as i16),
                    top.saturating_add(y as i16),
                    pixel,
                );
            }
        }
    }

    fn theme_color_is_mono_black(color: Rgb8) -> bool {
        u16::from(color.r) + u16::from(color.g) + u16::from(color.b) < 128 * 3
    }

    fn theme_pixel_index(&self, bus: &MacMemoryBus, color: Rgb8) -> u8 {
        if self.screen_mode.4 == 1 {
            return if Self::theme_color_is_mono_black(color) {
                Self::logical_black_pixel_index(bus)
            } else {
                Self::logical_white_pixel_index(bus)
            };
        }
        let rgb = [
            u16::from(color.r) * 0x0101,
            u16::from(color.g) * 0x0101,
            u16::from(color.b) * 0x0101,
        ];
        Self::fb_main_screen_pixel_index_for_rgb(bus, rgb).unwrap_or_else(|| {
            super::pict::closest_clut_index(rgb[0], rgb[1], rgb[2], &self.device_clut)
        })
    }

    fn fill_theme_rect(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        color: Rgb8,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let pixel = self.theme_pixel_index(bus, color);
        let (top, left, bottom, right) = rect;
        for y in top.max(0)..bottom.min(screen_height) {
            for x in left.max(0)..right.min(screen_width) {
                Self::fb_set_pixel_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    pixel,
                );
            }
        }
    }

    pub(crate) fn fill_theme_desktop_rect(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        if self.ui_theme_id() == UiThemeId::ClassicSystem7 || pixel_size == 1 {
            Self::fb_fill_pattern_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top,
                left,
                bottom,
                right,
                crate::window_manager::STANDARD_DESKTOP_PATTERN,
            );
            return;
        }

        let palette = self.ui_theme().palette();
        let light = self.theme_pixel_index(bus, palette.desktop_light);
        let dark = self.theme_pixel_index(bus, palette.desktop_dark);
        let top = top.max(0).min(screen_height);
        let left = left.max(0).min(screen_width);
        let bottom = bottom.max(0).min(screen_height);
        let right = right.max(0).min(screen_width);
        for y in top..bottom {
            let pattern = crate::window_manager::STANDARD_DESKTOP_PATTERN
                [y.rem_euclid(8) as usize];
            for x in left..right {
                let bit = (pattern >> (7 - x.rem_euclid(8))) & 1;
                Self::fb_set_pixel_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    if bit == 0 { light } else { dark },
                );
            }
        }
    }

    fn gdevice_ctab(bus: &MacMemoryBus, gdevice_handle: u32) -> Option<u32> {
        if gdevice_handle == 0 {
            return None;
        }
        let gdevice = bus.read_long(gdevice_handle);
        if gdevice == 0 {
            return None;
        }
        let pixmap_handle = bus.read_long(gdevice + 22);
        if pixmap_handle == 0 {
            return None;
        }
        let pixmap = bus.read_long(pixmap_handle);
        if pixmap == 0 {
            return None;
        }
        let ctab_handle = bus.read_long(pixmap + 42);
        if ctab_handle == 0 {
            return None;
        }
        let ctab = bus.read_long(ctab_handle);
        if ctab == 0 {
            return None;
        }
        Some(ctab)
    }

    fn active_gdevice_ctab(bus: &MacMemoryBus) -> Option<u32> {
        let current = bus.read_long(0x0CC8); // TheGDevice
        let gdevice_handle = if current != 0 {
            current
        } else {
            bus.read_long(0x08A4) // MainDevice
        };
        Self::gdevice_ctab(bus, gdevice_handle)
    }

    fn main_gdevice_ctab(bus: &MacMemoryBus) -> Option<u32> {
        let main = bus.read_long(0x08A4); // MainDevice
                                          // Menu chrome is composited into the physical screen. Never fall
                                          // back to TheGDevice here: it may name an offscreen GWorld whose
                                          // palette values are unrelated to the packed framebuffer indexes.
        Self::gdevice_ctab(bus, main)
    }

    fn ctab_value_luma(bus: &MacMemoryBus, ctab: u32, wanted_value: u8) -> Option<u32> {
        let count = u32::from(bus.read_word(ctab + 6)).min(255) + 1;
        let device_table = (bus.read_word(ctab + 4) & 0x8000) != 0;

        let ordinal = u32::from(wanted_value);
        if ordinal < count {
            let entry = ctab + 8 + ordinal * 8;
            if device_table || bus.read_word(entry) == u16::from(wanted_value) {
                return Some(
                    u32::from(bus.read_word(entry + 2))
                        + u32::from(bus.read_word(entry + 4))
                        + u32::from(bus.read_word(entry + 6)),
                );
            }
        }

        for ordinal in 0..count {
            let entry = ctab + 8 + ordinal * 8;
            if bus.read_word(entry) != u16::from(wanted_value) {
                continue;
            }
            return Some(
                u32::from(bus.read_word(entry + 2))
                    + u32::from(bus.read_word(entry + 4))
                    + u32::from(bus.read_word(entry + 6)),
            );
        }
        None
    }

    fn best_luma_pixel_index(bus: &MacMemoryBus, ctab: u32, brightest: bool) -> Option<u8> {
        let count = u32::from(bus.read_word(ctab + 6)).min(255) + 1;
        let device_table = (bus.read_word(ctab + 4) & 0x8000) != 0;
        let mut best_index = 0u8;
        let mut best_luma = 0u32;
        let mut found = false;
        for ordinal in 0..count {
            let entry = ctab + 8 + ordinal * 8;
            let value = if device_table {
                ordinal as u16
            } else {
                bus.read_word(entry)
            };
            if value > 255 {
                continue;
            }
            let luma = u32::from(bus.read_word(entry + 2))
                + u32::from(bus.read_word(entry + 4))
                + u32::from(bus.read_word(entry + 6));
            let index = value as u8;
            let better = if brightest {
                luma > best_luma || (luma == best_luma && index < best_index)
            } else {
                luma < best_luma || (luma == best_luma && index < best_index)
            };
            if !found || better {
                best_index = index;
                best_luma = luma;
                found = true;
            }
        }

        found.then_some(best_index)
    }

    fn fb_pixel_index_for_rgb_in_ctab_with_entry_count(
        bus: &MacMemoryBus,
        ctab: u32,
        rgb: [u16; 3],
        entry_count: usize,
    ) -> Option<u8> {
        let entry_count = entry_count.clamp(1, 256);
        let count = (u32::from(bus.read_word(ctab + 6)).min(255) + 1).min(entry_count as u32);
        let device_table = (bus.read_word(ctab + 4) & 0x8000) != 0;

        // Imaging With QuickDraw 1994 p. 4-82 describes inverse-table
        // lookup as the Color Manager path from RGB colors to device pixel
        // values. Keep canonical endpoints pinned when the active table has
        // the standard white/black entries, then prefer exact matches before
        // falling back to nearest Euclidean distance in 16-bit RGB space.
        let terminal_index = (entry_count - 1) as u8;
        if rgb == [0, 0, 0] && Self::ctab_value_luma(bus, ctab, terminal_index) == Some(0) {
            return Some(terminal_index);
        }
        if rgb == [0xFFFF, 0xFFFF, 0xFFFF]
            && Self::ctab_value_luma(bus, ctab, 0) == Some(0xFFFF * 3)
        {
            return Some(0);
        }

        let mut best_index = None;
        let mut best_distance = u64::MAX;
        for ordinal in 0..count {
            let entry = ctab + 8 + ordinal * 8;
            let value = if device_table {
                ordinal as u16
            } else {
                bus.read_word(entry)
            };
            if usize::from(value) >= entry_count {
                continue;
            }
            let entry_rgb = [
                bus.read_word(entry + 2),
                bus.read_word(entry + 4),
                bus.read_word(entry + 6),
            ];
            let index = value as u8;
            if entry_rgb == rgb {
                return Some(index);
            }

            let dr = i64::from(entry_rgb[0]) - i64::from(rgb[0]);
            let dg = i64::from(entry_rgb[1]) - i64::from(rgb[1]);
            let db = i64::from(entry_rgb[2]) - i64::from(rgb[2]);
            let distance = (dr * dr + dg * dg + db * db) as u64;
            if distance < best_distance
                || (distance == best_distance && best_index.map_or(true, |best| index < best))
            {
                best_distance = distance;
                best_index = Some(index);
            }
        }
        best_index
    }

    fn fb_pixel_index_for_rgb_in_ctab(bus: &MacMemoryBus, ctab: u32, rgb: [u16; 3]) -> Option<u8> {
        Self::fb_pixel_index_for_rgb_in_ctab_with_entry_count(bus, ctab, rgb, 256)
    }

    pub(crate) fn fb_pixel_index_for_rgb(bus: &MacMemoryBus, rgb: [u16; 3]) -> Option<u8> {
        let ctab = Self::active_gdevice_ctab(bus)?;
        Self::fb_pixel_index_for_rgb_in_ctab(bus, ctab, rgb)
    }

    pub(crate) fn fb_main_screen_pixel_index_for_rgb(
        bus: &MacMemoryBus,
        rgb: [u16; 3],
    ) -> Option<u8> {
        let ctab = Self::main_gdevice_ctab(bus)?;
        Self::fb_pixel_index_for_rgb_in_ctab(bus, ctab, rgb)
    }

    fn fb_main_screen_pixel_index_for_rgb_with_entry_count(
        bus: &MacMemoryBus,
        rgb: [u16; 3],
        entry_count: usize,
    ) -> Option<u8> {
        let ctab = Self::main_gdevice_ctab(bus)?;
        Self::fb_pixel_index_for_rgb_in_ctab_with_entry_count(bus, ctab, rgb, entry_count)
    }

    /// Read back the RGB a pixel value resolves to through a device colour
    /// table.
    ///
    /// Imaging With QuickDraw 1994 p. 4-82 describes the colour table as
    /// the mapping from pixel values to RGB; this is the inverse of
    /// `fb_pixel_index_for_rgb`.
    fn fb_rgb_for_pixel_index_in_ctab(
        bus: &MacMemoryBus,
        ctab: u32,
        index: u8,
    ) -> Option<[u16; 3]> {
        let count = u32::from(bus.read_word(ctab + 6)).min(255) + 1;
        let device_table = (bus.read_word(ctab + 4) & 0x8000) != 0;

        let ordinal = u32::from(index);
        let read_entry = |entry: u32| {
            [
                bus.read_word(entry + 2),
                bus.read_word(entry + 4),
                bus.read_word(entry + 6),
            ]
        };
        if ordinal < count && device_table {
            return Some(read_entry(ctab + 8 + ordinal * 8));
        }
        if ordinal < count {
            let entry = ctab + 8 + ordinal * 8;
            if bus.read_word(entry) == u16::from(index) {
                return Some(read_entry(entry));
            }
        }
        for ordinal in 0..count {
            let entry = ctab + 8 + ordinal * 8;
            if bus.read_word(entry) == u16::from(index) {
                return Some(read_entry(entry));
            }
        }
        None
    }

    pub(crate) fn fb_main_screen_rgb_for_pixel_index(
        bus: &MacMemoryBus,
        index: u8,
    ) -> Option<[u16; 3]> {
        let ctab = Self::main_gdevice_ctab(bus)?;
        Self::fb_rgb_for_pixel_index_in_ctab(bus, ctab, index)
    }

    /// Resolve the pixel value halfway between two colours, the way
    /// `GetGray` does when the Menu Manager dims unavailable content.
    ///
    /// IM:V 1986 p. V-142 (`GetGray`) averages the supplied background
    /// and foreground colours and resolves the result against the
    /// device's colour table, returning FALSE when the device cannot
    /// express an intermediate shade. Callers treat `None` the same way
    /// the standard definition procedures treat that FALSE: fall back to
    /// the 50% grey pattern.
    #[cfg(test)]
    fn fb_gray_pixel_index_between_in_ctab(
        bus: &MacMemoryBus,
        ctab: u32,
        background: [u16; 3],
        foreground: [u16; 3],
    ) -> Option<u8> {
        let midpoint = [
            ((u32::from(background[0]) + u32::from(foreground[0])) / 2) as u16,
            ((u32::from(background[1]) + u32::from(foreground[1])) / 2) as u16,
            ((u32::from(background[2]) + u32::from(foreground[2])) / 2) as u16,
        ];
        let gray = Self::fb_pixel_index_for_rgb_in_ctab(bus, ctab, midpoint)?;
        let background_index = Self::fb_pixel_index_for_rgb_in_ctab(bus, ctab, background);
        let foreground_index = Self::fb_pixel_index_for_rgb_in_ctab(bus, ctab, foreground);
        if Some(gray) == background_index || Some(gray) == foreground_index {
            return None;
        }
        Some(gray)
    }

    #[cfg(test)]
    pub(crate) fn fb_gray_pixel_index_between(
        bus: &MacMemoryBus,
        background: [u16; 3],
        foreground: [u16; 3],
    ) -> Option<u8> {
        let ctab = Self::active_gdevice_ctab(bus)?;
        Self::fb_gray_pixel_index_between_in_ctab(bus, ctab, background, foreground)
    }

    fn logical_white_pixel_index(bus: &MacMemoryBus) -> u8 {
        let Some(ctab) = Self::active_gdevice_ctab(bus) else {
            return 0;
        };
        if Self::ctab_value_luma(bus, ctab, 0) == Some(0xFFFF * 3) {
            return 0;
        }
        Self::best_luma_pixel_index(bus, ctab, true).unwrap_or(0)
    }

    fn logical_black_pixel_index(bus: &MacMemoryBus) -> u8 {
        let Some(ctab) = Self::active_gdevice_ctab(bus) else {
            return 255;
        };
        if Self::ctab_value_luma(bus, ctab, 1) == Some(0) {
            return 1;
        }
        if Self::ctab_value_luma(bus, ctab, 255) == Some(0) {
            return 255;
        }
        Self::best_luma_pixel_index(bus, ctab, false).unwrap_or(255)
    }

    pub(crate) fn fb_get_pixel_index(
        bus: &MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
    ) -> Option<u8> {
        if x < 0 || y < 0 || x >= screen_width || y >= screen_height {
            return None;
        }
        match pixel_size {
            8 => Some(bus.read_byte(screen_base + y as u32 * row_bytes + x as u32)),
            bits @ (1 | 2 | 4) => {
                // QuickDraw lays the leftmost image bits into the high-order
                // positions of each word; packed indexed pixels follow that
                // same left-to-right ordering. Imaging With QuickDraw (1994),
                // pp. 2-8 and 4-10--4-11.
                let bits = u32::from(bits);
                let pixels_per_byte = 8 / bits;
                let byte_offset = y as u32 * row_bytes + x as u32 / pixels_per_byte;
                let shift = 8 - bits - (x as u32 % pixels_per_byte) * bits;
                Some((bus.read_byte(screen_base + byte_offset) >> shift) & ((1u8 << bits) - 1))
            }
            _ => None,
        }
    }

    /// Set a single pixel in the framebuffer (screen coordinates).
    /// Works for 1bpp, 2bpp, 4bpp, and 8bpp screen modes.
    pub(crate) fn fb_set_pixel(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        black: bool,
    ) {
        let pixel_index = if black {
            Self::logical_black_pixel_index(bus)
        } else {
            Self::logical_white_pixel_index(bus)
        };
        Self::fb_set_pixel_index(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
            pixel_index,
        );
    }

    pub(crate) fn fb_set_pixel_index(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        pixel_index: u8,
    ) {
        if x < 0 || y < 0 || x >= screen_width || y >= screen_height {
            return;
        }
        match pixel_size {
            8 => {
                let addr = screen_base + y as u32 * row_bytes + x as u32;
                bus.write_byte(addr, pixel_index);
            }
            bits @ (1 | 2 | 4) => {
                let bits = u32::from(bits);
                let pixels_per_byte = 8 / bits;
                let addr = screen_base + y as u32 * row_bytes + x as u32 / pixels_per_byte;
                let shift = 8 - bits - (x as u32 % pixels_per_byte) * bits;
                let field_mask = ((1u16 << bits) - 1) as u8;
                let mask = field_mask << shift;
                let byte = bus.read_byte(addr);
                bus.write_byte(addr, (byte & !mask) | ((pixel_index & field_mask) << shift));
            }
            _ => {}
        }
    }

    pub(crate) fn fb_fill_rect_index(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        pixel_index: u8,
    ) {
        if matches!(pixel_size, 2 | 4) {
            for y in top..bottom {
                for x in left..right {
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        pixel_index,
                    );
                }
            }
            return;
        }
        if pixel_size != 8 {
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top,
                left,
                bottom,
                right,
                pixel_index != 0,
            );
            return;
        }
        let top = top.max(0).min(screen_height) as u32;
        let left = left.max(0).min(screen_width) as u32;
        let bottom = bottom.max(0).min(screen_height) as u32;
        let right = right.max(0).min(screen_width) as u32;
        if top >= bottom || left >= right {
            return;
        }
        let width = right - left;
        for y in top..bottom {
            let row_addr = screen_base + y * row_bytes;
            bus.fill_bytes(row_addr + left, width, pixel_index);
        }
    }

    /// Fill the exposed framebuffer around a large centered presentation
    /// rectangle with the darkest active indexed color. Systemless suppresses
    /// the classic desktop in kiosk mode, so centered game surfaces sit on a
    /// black stage while their pixels remain untouched.
    pub(super) fn fill_kiosk_stage_around_rect(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
    ) -> bool {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, bottom, right) = rect;
        let width = right.saturating_sub(left);
        let height = bottom.saturating_sub(top);
        let large = width >= (screen_width / 2).max(1) && height >= (screen_height / 2).max(1);
        let centered = left >= 0
            && top >= 0
            && (screen_width - right - left).abs() <= 1
            && (screen_height - bottom - top).abs() <= 1;
        if !self.menu_bar_hidden
            || !large
            || !centered
            || (width >= screen_width && height >= screen_height)
        {
            return false;
        }

        let black_index = self
            .device_clut
            .iter()
            .enumerate()
            .min_by_key(|(_, rgb)| u32::from(rgb[0]) + u32::from(rgb[1]) + u32::from(rgb[2]))
            .map(|(index, _)| index as u8)
            .unwrap_or(255);
        for (margin_top, margin_left, margin_bottom, margin_right) in [
            (0, 0, top, screen_width),
            (bottom, 0, screen_height, screen_width),
            (top, 0, bottom, left),
            (top, right, bottom, screen_width),
        ] {
            if pixel_size == 8 {
                Self::fb_fill_rect_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    margin_top,
                    margin_left,
                    margin_bottom,
                    margin_right,
                    black_index,
                );
            } else {
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    margin_top,
                    margin_left,
                    margin_bottom,
                    margin_right,
                    true,
                );
            }
        }
        true
    }

    pub(super) fn kiosk_stage_margins_are_uniform(
        &self,
        bus: &MacMemoryBus,
        rect: (i16, i16, i16, i16),
    ) -> bool {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        if pixel_size != 8 {
            return false;
        }
        let (top, left, bottom, right) = rect;
        if top < 0
            || left < 0
            || bottom > screen_height
            || right > screen_width
            || top >= bottom
            || left >= right
        {
            return false;
        }

        // Reject common composed-screen cases before walking every exposed
        // pixel. A disagreement between any two samples proves the margins
        // are nonuniform; agreement is not treated as proof, so the complete
        // scan below remains the authority before any pixels are replaced.
        let middle_y = top + (bottom - top) / 2;
        let middle_x = left + (right - left) / 2;
        let samples = [
            (0, 0),
            (0, screen_width - 1),
            (screen_height - 1, 0),
            (screen_height - 1, screen_width - 1),
            (top - 1, middle_x),
            (bottom, middle_x),
            (middle_y, left - 1),
            (middle_y, right),
        ];
        let first_sample = bus.read_byte(screen_base);
        if samples.iter().any(|&(y, x)| {
            y >= 0
                && x >= 0
                && y < screen_height
                && x < screen_width
                && bus.read_byte(screen_base + y as u32 * row_bytes + x as u32) != first_sample
        }) {
            return false;
        }

        let mut margin_index = None;
        let mut pixels = vec![0; screen_width as usize];
        for y in 0..screen_height {
            let ranges = if y < top || y >= bottom {
                [(0, screen_width), (0, 0)]
            } else {
                [(0, left), (right, screen_width)]
            };
            for (start, end) in ranges {
                let length = end.saturating_sub(start) as usize;
                if length == 0 {
                    continue;
                }
                bus.read_bytes_into(
                    screen_base + y as u32 * row_bytes + start as u32,
                    &mut pixels[..length],
                );
                for &pixel in &pixels[..length] {
                    match margin_index {
                        Some(expected) if pixel != expected => return false,
                        None => margin_index = Some(pixel),
                        _ => {}
                    }
                }
            }
        }
        margin_index.is_some()
    }

    /// Fill a rectangle in the framebuffer
    pub(crate) fn fb_fill_rect(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        black: bool,
    ) {
        if pixel_size == 8 {
            let top = top.max(0).min(screen_height) as u32;
            let left = left.max(0).min(screen_width) as u32;
            let bottom = bottom.max(0).min(screen_height) as u32;
            let right = right.max(0).min(screen_width) as u32;
            if top >= bottom || left >= right {
                return;
            }
            let fill = if black {
                Self::logical_black_pixel_index(bus)
            } else {
                Self::logical_white_pixel_index(bus)
            };
            let width = right - left;
            for y in top..bottom {
                let row_addr = screen_base + y * row_bytes;
                bus.fill_bytes(row_addr + left, width, fill);
            }
            return;
        }
        for y in top..bottom {
            for x in left..right {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    black,
                );
            }
        }
    }

    pub(crate) fn fb_fill_pattern_rect(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
        pattern: [u8; 8],
    ) {
        let top = top.max(0).min(screen_height);
        let left = left.max(0).min(screen_width);
        let bottom = bottom.max(0).min(screen_height);
        let right = right.max(0).min(screen_width);
        if top >= bottom || left >= right {
            return;
        }

        if pixel_size == 8 {
            // Row-tiled: the pattern repeats every 8 pixels in x and 8 rows
            // in y, so build the eight possible rows once (with the logical
            // black/white indices resolved once, not per pixel) and write
            // each scanline as one slice -- what fb_set_pixel writes per
            // pixel, without the per-pixel bus round trips.
            let black = Self::logical_black_pixel_index(bus);
            let white = Self::logical_white_pixel_index(bus);
            let width = (right - left) as usize;
            let rows: Vec<Vec<u8>> = (0..8)
                .map(|pattern_row| {
                    let bits = pattern[pattern_row];
                    (left..right)
                        .map(|x| {
                            if (bits >> (7 - x.rem_euclid(8))) & 1 != 0 {
                                black
                            } else {
                                white
                            }
                        })
                        .collect()
                })
                .collect();
            for y in top..bottom {
                let addr = screen_base + (y as u32) * row_bytes + (left as u32);
                bus.write_bytes(addr, &rows[y.rem_euclid(8) as usize][..width]);
            }
            return;
        }

        if pixel_size == 16 {
            let black = [0x00u8, 0x00u8];
            let white = [0x7Fu8, 0xFFu8];
            let width = (right - left) as usize;
            let rows: Vec<Vec<u8>> = (0..8)
                .map(|pattern_row| {
                    let bits = pattern[pattern_row];
                    (left..right)
                        .flat_map(|x| {
                            if (bits >> (7 - x.rem_euclid(8))) & 1 != 0 {
                                black
                            } else {
                                white
                            }
                        })
                        .collect()
                })
                .collect();
            for y in top..bottom {
                let addr = screen_base + (y as u32) * row_bytes + (left as u32) * 2;
                bus.write_bytes(addr, &rows[y.rem_euclid(8) as usize][..width * 2]);
            }
            return;
        }

        for y in top..bottom {
            let row = pattern[y.rem_euclid(8) as usize];
            for x in left..right {
                let bit = (row >> (7 - x.rem_euclid(8))) & 1;
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    bit != 0,
                );
            }
        }
    }

    fn fb_pixel_is_logical_black(
        bus: &MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
    ) -> bool {
        if x < 0 || y < 0 || x >= screen_width || y >= screen_height {
            return false;
        }
        let field_mask = match pixel_size {
            1 | 2 | 4 => ((1u16 << pixel_size) - 1) as u8,
            8 => u8::MAX,
            _ => return false,
        };
        Self::fb_get_pixel_index(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
        ) == Some(Self::logical_black_pixel_index(bus) & field_mask)
    }

    fn visible_window_count(&self, bus: &MacMemoryBus) -> usize {
        let mut count = 0usize;
        let mut saw = HashSet::new();
        for &window in self.window_list.iter() {
            if window != 0 && saw.insert(window) && bus.read_byte(window + 110u32) != 0 {
                count += 1;
            }
        }
        if self.front_window != 0
            && saw.insert(self.front_window)
            && bus.read_byte(self.front_window + 110u32) != 0
        {
            count += 1;
        }
        count
    }

    fn front_window_is_dialog_like(&self) -> bool {
        self.front_window != 0
            && (self.dialog_items.contains_key(&self.front_window)
                || matches!(
                    self.window_proc_ids
                        .get(&self.front_window)
                        .copied()
                        .unwrap_or(self.window_proc_id),
                    1 | 2 | 3 | 5
                ))
    }

    fn exposed_background_samples_are_black(
        &self,
        bus: &MacMemoryBus,
        excluded_rect: (i16, i16, i16, i16),
    ) -> bool {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let top = excluded_rect.0.max(0).min(screen_height);
        let left = excluded_rect.1.max(0).min(screen_width);
        let bottom = excluded_rect.2.max(0).min(screen_height);
        let right = excluded_rect.3.max(0).min(screen_width);
        let rects = [
            (0, 0, top, screen_width),
            (bottom, 0, screen_height, screen_width),
            (top, 0, bottom, left),
            (top, right, bottom, screen_width),
        ];

        let mut sampled = false;
        for (rt, rl, rb, rr) in rects {
            if rt >= rb || rl >= rr {
                continue;
            }
            let mut y = rt;
            while y < rb {
                let mut x = rl;
                while x < rr {
                    sampled = true;
                    let sample_points = [
                        (x, y),
                        (x.saturating_add(1).min(rr - 1), y),
                        (x, y.saturating_add(1).min(rb - 1)),
                    ];
                    for (sample_x, sample_y) in sample_points {
                        if !Self::fb_pixel_is_logical_black(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            sample_x,
                            sample_y,
                        ) {
                            return false;
                        }
                    }
                    x = x.saturating_add(16);
                }
                y = y.saturating_add(16);
            }
        }
        sampled
    }

    fn fill_desktop_pattern_outside_rect(
        &self,
        bus: &mut MacMemoryBus,
        excluded_rect: (i16, i16, i16, i16),
    ) {
        let (_, _, screen_width, screen_height, _) = self.get_screen_params();
        let top = excluded_rect.0.max(0).min(screen_height);
        let left = excluded_rect.1.max(0).min(screen_width);
        let bottom = excluded_rect.2.max(0).min(screen_height);
        let right = excluded_rect.3.max(0).min(screen_width);
        let rects = [
            (0, 0, top, screen_width),
            (bottom, 0, screen_height, screen_width),
            (top, 0, bottom, left),
            (top, right, bottom, screen_width),
        ];

        for (rt, rl, rb, rr) in rects {
            self.fill_theme_desktop_rect(bus, rt, rl, rb, rr);
        }
    }

    fn refresh_saved_under_with_desktop_pattern(&mut self, bus: &MacMemoryBus, window: u32) {
        let (_, _, _, _, pixel_size) = self.get_screen_params();
        let palette = self.ui_theme().palette();
        let (black, white) = if self.ui_theme_id() == UiThemeId::ClassicSystem7 || pixel_size == 1 {
            (
                Self::logical_black_pixel_index(bus),
                Self::logical_white_pixel_index(bus),
            )
        } else {
            (
                self.theme_pixel_index(bus, palette.desktop_dark),
                self.theme_pixel_index(bus, palette.desktop_light),
            )
        };
        let Some((top, left, width, height, pixels)) =
            self.window_saved_under_pixels.get_mut(&window)
        else {
            return;
        };

        pixels.clear();
        pixels.reserve(*width as usize * *height as usize);
        for y in *top..top.saturating_add(*height) {
            let row = STANDARD_GRAY_PATTERN[y.rem_euclid(8) as usize];
            for x in *left..left.saturating_add(*width) {
                let bit = (row >> (7 - x.rem_euclid(8))) & 1;
                pixels.push(if pixel_size == 8 {
                    if bit != 0 {
                        black
                    } else {
                        white
                    }
                } else {
                    bit
                });
            }
        }
    }

    fn restore_kiosk_dialog_desktop_background(&mut self, bus: &mut MacMemoryBus) {
        if !self.menu_bar_hidden
            || self.fullscreen_locked
            || !self.front_window_is_dialog_like()
            || self.visible_window_count(bus) != 1
        {
            return;
        }

        let bounds = self
            .window_structure_rect(bus, self.front_window)
            .unwrap_or(self.window_bounds);
        if self.exposed_background_samples_are_black(bus, bounds) {
            // SetDeskCPat defines the desktop as the Window Manager's
            // patterned background. In kiosk mode, preserve black full-screen
            // game surfaces, but restore the standard desktop pattern behind
            // single floating dialogs whose exposed background is still the
            // startup black stage.
            // Inside Macintosh Volume V, V-210
            self.fill_desktop_pattern_outside_rect(bus, bounds);
            // A floating window can have captured its save-under while the
            // startup stage was still black. Keep that snapshot coherent with
            // the desktop pattern synthesized here so CloseWindow does not
            // restore stale window-shaped pixels.
            // Inside Macintosh Volume I, I-283 to I-284
            self.refresh_saved_under_with_desktop_pattern(bus, self.front_window);
        }
    }

    /// Draw a horizontal line in the framebuffer
    pub(crate) fn fb_hline(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        y: i16,
        x1: i16,
        x2: i16,
        black: bool,
    ) {
        if pixel_size == 8 {
            if y < 0 || y >= screen_height {
                return;
            }
            let left = x1.max(0).min(screen_width) as u32;
            let right = x2.max(0).min(screen_width) as u32;
            if left >= right {
                return;
            }
            let fill = if black {
                Self::logical_black_pixel_index(bus)
            } else {
                Self::logical_white_pixel_index(bus)
            };
            let row_addr = screen_base + (y as u32) * row_bytes;
            bus.fill_bytes(row_addr + left, right - left, fill);
            return;
        }
        for x in x1..x2 {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                black,
            );
        }
    }

    /// Vertical line of `black` pixels at column `x` from `top` (inclusive)
    /// to `bottom` (exclusive), clipped to the screen. `screen` is
    /// (base, row bytes, pixel size, width, height). On an 8-bit screen the
    /// colour is resolved once and the column is written with one strided
    /// bus fill; other depths set pixels one at a time.
    pub(crate) fn fb_vline(
        bus: &mut MacMemoryBus,
        screen: (u32, u32, u16, i16, i16),
        x: i16,
        top: i16,
        bottom: i16,
        black: bool,
    ) {
        let (screen_base, row_bytes, pixel_size, screen_width, screen_height) = screen;
        if pixel_size == 8 {
            if x < 0 || x >= screen_width {
                return;
            }
            let top = top.max(0);
            let bottom = bottom.min(screen_height);
            if top >= bottom {
                return;
            }
            let index = if black {
                Self::logical_black_pixel_index(bus)
            } else {
                Self::logical_white_pixel_index(bus)
            };
            let start = screen_base + top as u32 * row_bytes + x as u32;
            bus.fill_bytes_strided(start, row_bytes, (bottom - top) as u32, index);
            return;
        }
        for y in top..bottom {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                black,
            );
        }
    }

    pub(crate) fn fb_hline_index(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        y: i16,
        x1: i16,
        x2: i16,
        pixel_index: u8,
    ) {
        for x in x1..x2 {
            Self::fb_set_pixel_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                pixel_index,
            );
        }
    }

    /// Draw a single character glyph to the framebuffer, return advance width
    pub(crate) fn fb_draw_char(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        ch: char,
        font_id: i16,
        font_size: i16,
    ) -> i16 {
        if let Some((glyph, data)) = get_glyph(font_id, font_size, ch) {
            Self::fb_draw_glyph_bitmap(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                glyph,
                data,
            );
            glyph.advance as i16
        } else {
            6 // default advance for missing glyph
        }
    }

    fn fb_draw_glyph_bitmap(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        glyph: &Glyph,
        data: &[u8],
    ) {
        Self::fb_draw_glyph_bitmap_with_slant(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
            glyph,
            data,
            None,
            QuickDrawTextStyle::plain(),
            None,
            true,
        );
    }

    /// 8-bit glyph painter shared by the plain, clipped and plain-styled
    /// text paths: per glyph row, one bulk read of the covered destination
    /// span, the glyph's set pixels (coverage >= 128) applied against the
    /// host buffer, one bulk write; rows with no set pixel in view touch the
    /// bus not at all. `origin` is the glyph box's top-left on screen;
    /// `clip` is (top, left, bottom, right) in screen coordinates and is
    /// intersected with the screen.
    fn fb_blit_glyph_rows_8bpp(
        bus: &mut MacMemoryBus,
        screen: (u32, u32, i16, i16),
        origin: (i16, i16),
        glyph: &Glyph,
        data: &[u8],
        clip: (i16, i16, i16, i16),
        index: u8,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height) = screen;
        let (gx, gy) = origin;
        let gw = glyph.width as usize;
        let gh = glyph.height as usize;
        if gw == 0 || gh == 0 {
            return;
        }
        let (clip_top, clip_left, clip_bottom, clip_right) = clip;
        let clip_top = i32::from(clip_top.max(0));
        let clip_left = i32::from(clip_left.max(0));
        let clip_bottom = i32::from(clip_bottom.min(screen_height));
        let clip_right = i32::from(clip_right.min(screen_width));
        let col_start = (clip_left - i32::from(gx)).max(0);
        let col_end = (clip_right - i32::from(gx)).min(gw as i32);
        if col_start >= col_end {
            return;
        }
        let width = (col_end - col_start) as usize;
        let mut span = vec![0u8; width];
        for row in 0..gh {
            let py = i32::from(gy) + row as i32;
            if py < clip_top || py >= clip_bottom {
                continue;
            }
            let row_off = glyph.data_offset + row * gw + col_start as usize;
            let set = |col: usize| data.get(row_off + col).is_some_and(|&c| c >= 128);
            if !(0..width).any(set) {
                continue;
            }
            let start = screen_base + py as u32 * row_bytes + (i32::from(gx) + col_start) as u32;
            bus.read_bytes_into(start, &mut span);
            for (col, dst) in span.iter_mut().enumerate() {
                if set(col) {
                    *dst = index;
                }
            }
            bus.write_bytes(start, &span);
        }
    }

    fn fb_draw_glyph_bitmap_with_slant(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        glyph: &Glyph,
        data: &[u8],
        synthetic_italic: Option<(i16, i16)>,
        style: QuickDrawTextStyle,
        pixel_index_override: Option<u8>,
        black: bool,
    ) {
        let gx = x + glyph.origin_x as i16;
        let gy = y + glyph.origin_y as i16;
        let gw = glyph.width as usize;
        let gh = glyph.height as usize;
        // Unslanted, unstretched glyphs on an 8-bit screen go row by row.
        if pixel_size == 8 && synthetic_italic.is_none() && !style.extended() && !style.condensed()
        {
            let index = pixel_index_override.unwrap_or_else(|| {
                if black {
                    Self::logical_black_pixel_index(bus)
                } else {
                    Self::logical_white_pixel_index(bus)
                }
            });
            Self::fb_blit_glyph_rows_8bpp(
                bus,
                (screen_base, row_bytes, screen_width, screen_height),
                (gx, gy),
                glyph,
                data,
                (0, 0, screen_height, screen_width),
                index,
            );
            return;
        }
        let metrics = synthetic_italic
            .map(|(font_id, font_size)| (font_id, font_size, get_font_metrics(font_id, font_size)));
        let text_index = if matches!(pixel_size, 2 | 4 | 8) {
            Some(pixel_index_override.unwrap_or_else(|| {
                if black {
                    Self::logical_black_pixel_index(bus)
                } else {
                    Self::logical_white_pixel_index(bus)
                }
            }))
        } else {
            None
        };

        // Glyph data is 8-bit coverage per pixel (row-major, one byte
        // per pixel). Threshold at >=128 (bitmap glyphs are exclusively
        // 0 or 255).
        for row in 0..gh {
            for col in 0..gw {
                let byte_idx = glyph.data_offset + row * gw + col;
                if byte_idx < data.len() && data[byte_idx] >= 128 {
                    let py = gy + row as i16;
                    let slant = metrics
                        .as_ref()
                        .map(|(font_id, font_size, metrics)| {
                            get_italic_slant(*font_id, *font_size, metrics, y, py)
                        })
                        .unwrap_or(0);
                    let (dst_start, dst_end) = if style.extended() {
                        let start = (col as i16 * 4) / 3;
                        let end = (((col as i16 + 1) * 4) / 3).max(start + 1);
                        (start, end)
                    } else if style.condensed() {
                        let start = (col as i16 * 3) / 4;
                        (start, start + 1)
                    } else {
                        let start = col as i16;
                        (start, start + 1)
                    };
                    for dst_col in dst_start..dst_end {
                        let px = gx + dst_col + slant;
                        if let Some(text_index) = text_index {
                            Self::fb_set_pixel_index(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                px,
                                py,
                                text_index,
                            );
                        } else {
                            Self::fb_set_pixel(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                px,
                                py,
                                black,
                            );
                        }
                    }
                }
            }
        }
    }

    fn fb_set_styled_text_pixel(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        pixel_index_override: Option<u8>,
        black: bool,
    ) {
        if let Some(pixel_index) = pixel_index_override {
            Self::fb_set_pixel_index(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                pixel_index,
            );
        } else {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                black,
            );
        }
    }

    fn fb_styled_glyph_base_pixels(
        x: i16,
        y: i16,
        glyph: &Glyph,
        data: &[u8],
        synthetic_italic: Option<(i16, i16)>,
        style: QuickDrawTextStyle,
    ) -> HashSet<(i16, i16)> {
        let gx = x + glyph.origin_x as i16;
        let gy = y + glyph.origin_y as i16;
        let gw = glyph.width as usize;
        let gh = glyph.height as usize;
        let metrics = synthetic_italic
            .map(|(font_id, font_size)| (font_id, font_size, get_font_metrics(font_id, font_size)));
        let mut pixels = HashSet::new();

        for row in 0..gh {
            for col in 0..gw {
                let byte_idx = glyph.data_offset + row * gw + col;
                if byte_idx >= data.len() || data[byte_idx] < 128 {
                    continue;
                }

                let py = gy + row as i16;
                let slant = metrics
                    .as_ref()
                    .map(|(font_id, font_size, metrics)| {
                        get_italic_slant(*font_id, *font_size, metrics, y, py)
                    })
                    .unwrap_or(0);
                let start = col as i16;
                let (dst_start, dst_end) = (start, start + 1);

                for dst_col in dst_start..dst_end {
                    let px = gx + dst_col + slant;
                    pixels.insert((px, py));
                    if style.bold() {
                        pixels.insert((px + 1, py));
                    }
                }
            }
        }

        pixels
    }

    fn fb_draw_char_styled(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        ch: char,
        font_id: i16,
        font_size: i16,
        style: QuickDrawTextStyle,
        pixel_index_override: Option<u8>,
        black: bool,
    ) -> i16 {
        let (glyph_hit, synthetic_italic) = if style.italic() {
            if let Some(hit) = get_glyph_italic(font_id, font_size, ch) {
                (Some(hit), None)
            } else {
                (
                    get_glyph(font_id, font_size, ch),
                    Some((font_id, font_size)),
                )
            }
        } else {
            (get_glyph(font_id, font_size, ch), None)
        };

        let Some((glyph, data)) = glyph_hit else {
            return 6;
        };

        // Without a per-glyph style bit the styled painter's pixel set is
        // exactly the glyph bitmap; on an 8-bit screen let the row painter
        // write it instead of building the set.
        if pixel_size == 8 && !style.has_per_glyph_effect() {
            Self::fb_draw_glyph_bitmap_with_slant(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                glyph,
                data,
                None,
                QuickDrawTextStyle::plain(),
                pixel_index_override,
                black,
            );
            return i16::try_from(style.glyph_advance(i32::from(glyph.advance)))
                .unwrap_or(i16::MAX);
        }

        let glyph_y = y.saturating_add(style.glyph_y_offset() as i16);
        let base_pixels =
            Self::fb_styled_glyph_base_pixels(x, glyph_y, glyph, data, synthetic_italic, style);

        let Some(smear_max) = style.smear_max() else {
            for (px, py) in base_pixels.iter().copied() {
                Self::fb_set_styled_text_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    px,
                    py,
                    pixel_index_override,
                    black,
                );
            }
            return i16::try_from(style.glyph_advance(i32::from(glyph.advance)))
                .unwrap_or(i16::MAX);
        };

        // QuickDraw outlines/shadows text by smearing a 1-bit glyph mask,
        // then XORing the original glyph out of the result. That produces
        // hollow outline and shadow faces instead of drawing offset filled
        // glyph copies.
        let smear_max = i16::try_from(smear_max).unwrap_or(1);
        let min_x = base_pixels.iter().map(|(px, _)| *px).min().unwrap_or(x) - 1;
        let max_x = base_pixels.iter().map(|(px, _)| *px).max().unwrap_or(x) + smear_max;
        let min_y = base_pixels.iter().map(|(_, py)| *py).min().unwrap_or(y) - 1;
        let max_y = base_pixels.iter().map(|(_, py)| *py).max().unwrap_or(y) + smear_max;

        for py in min_y..=max_y {
            for px in min_x..=max_x {
                if base_pixels.contains(&(px, py)) {
                    continue;
                }
                let mut smeared = false;
                'smear: for dy in -1..=smear_max {
                    for dx in -1..=smear_max {
                        if base_pixels.contains(&(px - dx, py - dy)) {
                            smeared = true;
                            break 'smear;
                        }
                    }
                }
                if smeared {
                    Self::fb_set_styled_text_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        px,
                        py,
                        pixel_index_override,
                        black,
                    );
                }
            }
        }

        i16::try_from(style.glyph_advance(i32::from(glyph.advance))).unwrap_or(i16::MAX)
    }

    /// Draw a string to the framebuffer, return total width
    pub(crate) fn fb_draw_string(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        s: &str,
        font_id: i16,
        font_size: i16,
    ) -> i16 {
        let mut cx = x;
        for ch in s.chars() {
            cx += Self::fb_draw_char(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                cx,
                y,
                ch,
                font_id,
                font_size,
            );
        }
        cx - x
    }

    fn fb_draw_char_clipped(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        ch: char,
        font_id: i16,
        font_size: i16,
        clip: (i16, i16, i16, i16),
        text_index: Option<u8>,
    ) -> i16 {
        let Some((glyph, data)) = get_glyph(font_id, font_size, ch) else {
            return 6;
        };
        let gx = x + glyph.origin_x as i16;
        let gy = y + glyph.origin_y as i16;
        if let Some(index) = text_index.filter(|_| pixel_size == 8) {
            Self::fb_blit_glyph_rows_8bpp(
                bus,
                (screen_base, row_bytes, screen_width, screen_height),
                (gx, gy),
                glyph,
                data,
                clip,
                index,
            );
            return glyph.advance as i16;
        }
        let gw = glyph.width as usize;
        let gh = glyph.height as usize;
        let (clip_top, clip_left, clip_bottom, clip_right) = clip;
        for row in 0..gh {
            let py = gy + row as i16;
            if py < clip_top || py >= clip_bottom {
                continue;
            }
            for col in 0..gw {
                let px = gx + col as i16;
                if px < clip_left || px >= clip_right {
                    continue;
                }
                let byte_idx = glyph.data_offset + row * gw + col;
                if byte_idx < data.len() && data[byte_idx] >= 128 {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        px,
                        py,
                        true,
                    );
                }
            }
        }
        glyph.advance as i16
    }

    pub(crate) fn fb_draw_string_clipped(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        s: &str,
        font_id: i16,
        font_size: i16,
        clip: (i16, i16, i16, i16),
    ) -> i16 {
        // Resolve the text colour once per string, not once per pixel.
        let text_index = (pixel_size == 8).then(|| Self::logical_black_pixel_index(bus));
        let mut cx = x;
        for ch in s.chars() {
            cx += Self::fb_draw_char_clipped(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                cx,
                y,
                ch,
                font_id,
                font_size,
                clip,
                text_index,
            );
        }
        cx - x
    }

    /// Draw a string to the framebuffer with a classic Style bitset.
    ///
    /// MTE 1992 pp. 3-60 and 3-133 to 3-134 define menu item text
    /// styles as the `Style` bitset used by `SetItemStyle`; HIG 1992
    /// pp. 72 to 74 show Style menu item names displayed in their
    /// corresponding text styles. This helper changes drawn pixels only;
    /// callers keep measurements on the plain glyph advances so themed
    /// and classic-compatible guest metrics remain stable.
    pub(crate) fn fb_draw_string_styled(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        s: &str,
        font_id: i16,
        font_size: i16,
        style: u8,
    ) -> i16 {
        Self::fb_draw_string_styled_with_index(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
            s,
            font_id,
            font_size,
            style,
            None,
            true,
        )
    }

    pub(crate) fn fb_draw_string_styled_ink(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        s: &str,
        font_id: i16,
        font_size: i16,
        style: u8,
        black: bool,
    ) -> i16 {
        Self::fb_draw_string_styled_with_index(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
            s,
            font_id,
            font_size,
            style,
            None,
            black,
        )
    }

    pub(crate) fn fb_draw_string_styled_index(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        s: &str,
        font_id: i16,
        font_size: i16,
        style: u8,
        pixel_index: u8,
    ) -> i16 {
        Self::fb_draw_string_styled_with_index(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            x,
            y,
            s,
            font_id,
            font_size,
            style,
            Some(pixel_index),
            true,
        )
    }

    fn fb_draw_string_styled_with_index(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        y: i16,
        s: &str,
        font_id: i16,
        font_size: i16,
        style: u8,
        pixel_index_override: Option<u8>,
        black: bool,
    ) -> i16 {
        let style = QuickDrawTextStyle::from_bits(style);
        let mut cx = x;
        for ch in s.chars() {
            cx += Self::fb_draw_char_styled(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                cx,
                y,
                ch,
                font_id,
                font_size,
                style,
                pixel_index_override,
                black,
            );
        }

        if style.underline() && cx > x {
            let thickness = get_underline_thickness(font_id, font_size).max(1);
            for dy in 1..=thickness {
                if let Some(pixel_index) = pixel_index_override {
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y + dy,
                        pixel_index,
                    );
                    for underline_x in (x + 1)..cx {
                        Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            underline_x,
                            y + dy,
                            pixel_index,
                        );
                    }
                } else {
                    Self::fb_hline(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        y + dy,
                        x,
                        cx,
                        black,
                    );
                }
            }
        }

        cx - x
    }

    /// Draw the menu bar at the top of the screen.
    /// Height is read from the MBarHeight low-memory global ($0BAA).
    /// If MBarHeight is 0, the menu bar is hidden (full-screen mode).
    pub(crate) fn draw_menu_bar_to_fb(&self, bus: &mut MacMemoryBus) {
        if self.fullscreen_locked || self.menu_bar_hidden {
            return;
        }
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        if menu_bar_height <= 0 {
            return;
        }
        let menu_bar_bg_index = self.menu_bar_background_pixel_index(bus, pixel_size);

        if !self.draw_theme_menu_bar_chrome(bus, menu_bar_height) {
            // The System 7 menu bar is a white strip with a one-pixel
            // lower border. Macintosh Toolbox Essentials 1992, glossary
            // "menu bar".
            if let Some(bg_index) = menu_bar_bg_index {
                Self::fb_fill_rect_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    0,
                    0,
                    menu_bar_height,
                    screen_width,
                    bg_index,
                );
            } else {
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    0,
                    0,
                    menu_bar_height,
                    screen_width,
                    false,
                );
            }

            if let Some(black_index) = Self::menu_standard_pixel_index(bus, pixel_size, true) {
                Self::fb_hline_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    menu_bar_height - 1,
                    0,
                    screen_width,
                    black_index,
                );
            } else {
                Self::fb_hline(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    menu_bar_height - 1,
                    0,
                    screen_width,
                    true,
                );
            }
            Self::fb_draw_menu_bar_rounded_corners(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
            );
        }

        // Chicago 12 is the system font for menus (font_id=0, size=12)
        let font_id: i16 = 0;
        let font_size: i16 = 12;
        let metrics = get_font_metrics(font_id, font_size);
        let text_y = Self::menu_bar_title_baseline(menu_bar_height);

        // Draw visible menu titles from the current menu list. InsertMenu
        // with beforeID=-1 installs a submenu/popup in the current menu
        // list without adding a menu-bar title. MTE 1992, p. 3-121.
        for (menu_idx, region) in self.current_menu_title_regions_with_indices(bus) {
            let Some(menu) = self.menus.get(menu_idx) else {
                continue;
            };
            let title = &menu.title;
            let title_width = Self::menu_title_advance(title);
            let x = region.title_origin();
            let title_bg_index = self.menu_title_background_pixel_index(bus, menu.id, pixel_size);
            if self.ui_theme_id() == UiThemeId::ClassicSystem7 {
                // StandardMBDF establishes each title's RGB1/RGB2 pair and
                // erases the complete title cell to RGB2 before drawing its
                // text. This cell is also the rectangle later reversed by
                // HiliteMenu. IM:V 1986 pp. V-232 and V-252 to V-253.
                let (cell_top, cell_left, cell_bottom, cell_right) =
                    region.highlighted_rect(menu_bar_height);
                if let Some(bg_index) = title_bg_index {
                    Self::fb_fill_rect_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        cell_top,
                        cell_left,
                        cell_bottom,
                        cell_right,
                        bg_index,
                    );
                } else {
                    Self::fb_fill_rect(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        cell_top,
                        cell_left,
                        cell_bottom,
                        cell_right,
                        false,
                    );
                }
            }
            // HIG 1992 p. 54 says unavailable menu titles remain visible
            // but dimmed; p. 55 says pressing a menu title highlights it.
            // Route title-state chrome through the provider while keeping
            // classic text metrics and title hit regions unchanged.
            self.draw_theme_menu_title_chrome(
                bus,
                1,
                region.left,
                menu_bar_height - 1,
                region.right,
                menu.enabled,
                false,
            );
            let title_index = Self::menu_title_pixel_index(bus, menu.id, pixel_size);
            // MTE 1992 p. 3-131: DisableItem(menu, 0) disables the whole menu
            // title, and HIG 1992 p. 54 says an unavailable title stays
            // visible but dimmed. On a colour screen the standard definition
            // procedure resolves the dim shade through GetGray (IM:V 1986
            // p. V-142) and draws solid grey glyphs; a device with no
            // intermediate shade gets the title drawn and then knocked back
            // with the 50% grey pattern.
            let dim_title = self.ui_theme_id() == UiThemeId::ClassicSystem7 && !menu.enabled;
            let dim_index = if dim_title {
                Self::menu_dim_pixel_index(bus, pixel_size, title_index, title_bg_index)
            } else {
                None
            };
            let system_mark = Self::is_system_menu_mark_title(title);
            // The retro mark is multi-coloured original artwork, so it dims
            // through the pattern path on every screen depth rather than
            // collapsing to a single grey silhouette.
            let dim_with_pattern = dim_title && (dim_index.is_none() || system_mark);
            let width = if system_mark {
                self.fb_draw_retro_computer_menu_mark(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                );
                title_width
            } else if let Some(pixel_index) = dim_index.or(title_index) {
                Self::fb_draw_string_styled_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    text_y,
                    title,
                    font_id,
                    font_size,
                    0,
                    pixel_index,
                )
            } else {
                Self::fb_draw_string(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    text_y,
                    title,
                    font_id,
                    font_size,
                )
            };
            if dim_with_pattern {
                let (dim_top, dim_bottom) = if system_mark {
                    (1, menu_bar_height - 1)
                } else {
                    (
                        (text_y - metrics.ascent).max(0),
                        (text_y + metrics.descent).min(menu_bar_height - 1),
                    )
                };
                self.fb_apply_menu_title_dim_pattern(
                    bus,
                    (dim_top, x, dim_bottom, x.saturating_add(width)),
                    title_bg_index,
                    false,
                );
            }
        }

        if let Some(menu_idx) = self.current_menu_bar_highlight_index(bus) {
            self.highlight_menu_title(bus, menu_idx);
        }
    }

    pub(super) fn is_system_menu_mark_title(title: &str) -> bool {
        is_standard_system_menu_title(&super::menu::internal_menu_string_bytes(title))
    }

    /// Width the menu bar reserves for one menu title.
    ///
    /// The system menu's title is the mark character ($14, IM:I I-354),
    /// and the menu bar sizes its cell from the system font's mark glyph —
    /// 11 pixels in the Chicago 12 strike System 7.5.3 lays the bar out
    /// with. Systemless substitutes its own artwork for that glyph, so the
    /// cell is pinned to the reference advance rather than measured from
    /// whichever mark glyph the loaded font happens to carry: measuring it
    /// would shift the first title, and every title after it, whenever the
    /// font catalogue changed, and would crowd the artwork against the next
    /// title's highlight rectangle.
    pub(crate) fn menu_title_advance(title: &str) -> i16 {
        standard_menu_title_advance(&super::menu::internal_menu_string_bytes(title))
    }

    fn menu_bar_title_baseline(menu_bar_height: i16) -> i16 {
        let metrics = get_font_metrics(0, 12);
        standard_menu_bar_title_baseline(menu_bar_height, metrics.ascent, metrics.descent)
    }

    pub(super) fn fb_draw_retro_computer_menu_mark(
        &self,
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
    ) {
        // appleMark ($14) identifies the system menu title. Preserve its
        // measured advance while substituting original Systemless artwork.
        // Inside Macintosh Volume I, I-354
        let palette_indices = crate::ui_art::RETRO_COMPUTER_MENU_MARK_PALETTE.map(|rgb| {
            Self::fb_main_screen_pixel_index_for_rgb(bus, rgb).unwrap_or_else(|| {
                super::pict::closest_clut_index(rgb[0], rgb[1], rgb[2], &self.device_clut)
            })
        });

        let left = x;
        let metrics = get_font_metrics(0, 12);
        // Align the artwork to the same centered system-font baseline as an
        // ordinary title. This preserves the reference y=3 placement for a
        // 20-pixel bar while following a live MBarHeight. Clip to the menu
        // bar interior so a short bar cannot paint the window/desktop below.
        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let top =
            standard_menu_bar_system_mark_top(menu_bar_height, metrics.ascent, metrics.descent);
        let menu_bar_bottom = menu_bar_height.saturating_sub(1).max(0);
        for (dy, row) in crate::ui_art::RETRO_COMPUTER_MENU_MARK_PIXELS
            .into_iter()
            .enumerate()
        {
            for (dx, palette_index) in row.into_iter().enumerate() {
                if palette_index == 0 {
                    continue;
                }
                let dst_x = left + dx as i16;
                let dst_y = top + dy as i16;
                if dst_y < 0 || dst_y >= menu_bar_bottom {
                    continue;
                }
                if matches!(pixel_size, 2 | 4 | 8) {
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        dst_x,
                        dst_y,
                        palette_indices[usize::from(palette_index - 1)],
                    );
                } else if palette_index == 1 {
                    // In monochrome, retain the dark outline and face while
                    // the light case and screen use the menu-bar background.
                    if let Some(black_index) =
                        Self::menu_standard_pixel_index(bus, pixel_size, true)
                    {
                        Self::fb_set_pixel_index(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            dst_x,
                            dst_y,
                            black_index,
                        );
                    } else {
                        Self::fb_set_pixel(
                            bus,
                            screen_base,
                            row_bytes,
                            pixel_size,
                            screen_width,
                            screen_height,
                            dst_x,
                            dst_y,
                            true,
                        );
                    }
                }
            }
        }
    }

    pub(super) fn fb_apply_menu_title_dim_pattern(
        &self,
        bus: &mut MacMemoryBus,
        rect: (i16, i16, i16, i16),
        background_index: Option<u8>,
        background_black: bool,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (top, left, bottom, right) = rect;
        for y in top..bottom {
            for x in left..right {
                // Keep the pixels the 50% grey pattern covers — its
                // `$AA $55 …` bits are on where x + y is even.
                // Imaging With QuickDraw 1994 p. 3-9.
                if (x as i32 + y as i32) % 2 == 0 {
                    continue;
                }
                match background_index {
                    Some(index) => Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        index,
                    ),
                    None => Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        x,
                        y,
                        background_black,
                    ),
                }
            }
        }
    }

    pub(crate) fn fb_draw_menu_bar_rounded_corners(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
    ) {
        let black_index = Self::menu_standard_pixel_index(bus, pixel_size, true);
        for_each_standard_menu_bar_corner_pixel(screen_width, |x, y| {
            if let Some(pixel_index) = black_index {
                Self::fb_set_pixel_index(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    pixel_index,
                );
            } else {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    y,
                    true,
                );
            }
        });
    }

    /// Blit the front window's port pixels to the screen framebuffer.
    ///
    /// On a real Mac, the Window Manager composites window content to the
    /// screen. In Systemless HLE, games draw to the window's GrafPort which
    /// may use a different baseAddr than the screen framebuffer. This copies
    /// the window content so that screen captures reflect the actual game state.
    pub(crate) fn blit_window_to_screen(&self, bus: &mut MacMemoryBus) {
        let (screen_base, screen_rb, screen_w, screen_h, pixel_size) = self.screen_mode;
        let trace = std::env::var_os("SYSTEMLESS_TRACE_BLIT_WINDOW").is_some();
        if self.front_window == 0 {
            if trace {
                eprintln!("[BLIT] skip: front_window=0");
            }
            return;
        }
        if !matches!(pixel_size, 1 | 2 | 4 | 8) || screen_w == 0 || screen_h == 0 {
            if trace {
                eprintln!(
                    "[BLIT] skip: screen mode mismatch (pixel_size={}, w={}, h={})",
                    pixel_size, screen_w, screen_h
                );
            }
            return;
        }

        // Read the window's port baseAddr.
        // CGrafPort version flag is at offset +6 (not +0 which is `device`).
        // Inside Macintosh Volume V, V-47
        let port = self.front_window;
        let port_version = bus.read_word(port + 6);
        let is_cgraf_port = (port_version & 0xC000) == 0xC000;
        let (port_base, port_pixmap_ptr, port_pixmap_handle) = if is_cgraf_port {
            // CGrafPort: portPixMap handle at offset 2
            let pm_handle = bus.read_long(port + 2);
            if pm_handle == 0 {
                if trace {
                    eprintln!("[BLIT] skip: CGrafPort pm_handle=0");
                }
                return;
            }
            let pm_ptr = bus.read_long(pm_handle);
            if pm_ptr == 0 {
                if trace {
                    eprintln!(
                        "[BLIT] skip: CGrafPort pm_ptr=0 (pm_handle=${:08X})",
                        pm_handle
                    );
                }
                return;
            }
            (
                Self::offscreen_pixmap_base_ptr(bus, pm_ptr) & 0x3FFFFFFF,
                pm_ptr,
                pm_handle,
            ) // mask off flags
        } else {
            // GrafPort: portBits.baseAddr at offset 2
            (bus.read_long(port + 2), 0, 0)
        };

        // SetPortPix deliberately replaces a CGrafPort's portPixMap so
        // applications can draw or calculate in a buffer other than the
        // window/screen, then CopyBits it explicitly. If a tracked Window
        // Manager color window has been swapped to such a scratch PixMap,
        // do not synthesize an automatic whole-port presentation blit.
        // IM:V V-76; Imaging With QuickDraw 1994, 4-86..4-87.
        if is_cgraf_port
            && self
                .window_original_pixmaps
                .get(&port)
                .is_some_and(|&original| original != port_pixmap_handle)
        {
            if trace {
                eprintln!(
                    "[BLIT] skip: CGrafPort portPixMap swapped original=${:08X} current=${:08X}",
                    self.window_original_pixmaps[&port], port_pixmap_handle
                );
            }
            return;
        }

        // If the window already draws directly to the screen, no blit needed
        if port_base == screen_base || port_base == 0 {
            if trace {
                eprintln!(
                    "[BLIT] skip: port_base=${:08X} screen_base=${:08X} \
                     (port draws directly to screen or port_base is NIL)",
                    port_base, screen_base
                );
            }
            return;
        }

        // Read the port's rowBytes (from pixMap for CGrafPort, portBits for GrafPort)
        let port_rb = if is_cgraf_port {
            (bus.read_word(port_pixmap_ptr + 4) & 0x3FFF) as u32
        } else {
            (bus.read_word(port + 6) & 0x3FFF) as u32
        };

        // Read source port pixel size. For CGrafPort, PixMap.pixelSize
        // lives at offset +32 of the PixMap struct. Basic GrafPort is
        // implicitly 1bpp.
        let port_pixel_size: u32 = if is_cgraf_port {
            bus.read_word(port_pixmap_ptr + 32) as u32
        } else {
            1
        };
        let port_ctab_handle = if is_cgraf_port {
            bus.read_long(port_pixmap_ptr + 42)
        } else {
            0
        };

        // Read window content bounds (portRect in GrafPort at offset +16)
        let wr_top = i32::from(bus.read_word(port + 16) as i16);
        let wr_left = i32::from(bus.read_word(port + 18) as i16);
        let wr_bottom = i32::from(bus.read_word(port + 20) as i16);
        let wr_right = i32::from(bus.read_word(port + 22) as i16);

        let w = wr_right - wr_left;
        let h = wr_bottom - wr_top;
        if w <= 0 || h <= 0 {
            return;
        }
        let w = w as u32;
        let h = h as u32;

        let (global_top, global_left, _, _) = self.window_global_port_rect(bus, port);
        let (pb_top, pb_left, pb_bottom, pb_right) = self.port_bounds_rect(bus, port);
        let pb_top = i32::from(pb_top);
        let pb_left = i32::from(pb_left);
        let pb_bottom = i32::from(pb_bottom);
        let pb_right = i32::from(pb_right);

        let src_y_offset = (wr_top - pb_top).max(0) as u32;
        let src_x_offset = (wr_left - pb_left).max(0) as u32;
        let dst_y = i32::from(global_top).max(0) as u32;
        let dst_x = i32::from(global_left).max(0) as u32;

        let row_count = h.min((screen_h as u32).saturating_sub(dst_y));
        let col_count = w.min((screen_w as u32).saturating_sub(dst_x));
        let bitmap_w = (pb_right - pb_left).max(0) as u32;
        let bitmap_h = (pb_bottom - pb_top).max(0) as u32;
        let (bounded_row_count, mut bounded_col_count) = if bitmap_w != 0 && bitmap_h != 0 {
            (
                row_count.min(bitmap_h.saturating_sub(src_y_offset)),
                col_count.min(bitmap_w.saturating_sub(src_x_offset)),
            )
        } else {
            // A few legacy HLE fixtures and malformed ports omit PixMap
            // bounds while still providing a valid base, stride, and
            // portRect. Retain their portRect fallback without weakening the
            // bounds guard for well-formed guest PixMaps.
            (row_count, col_count)
        };
        if matches!(port_pixel_size, 1 | 2 | 4 | 8) {
            let source_row_pixels = port_rb.saturating_mul(8) / port_pixel_size;
            bounded_col_count =
                bounded_col_count.min(source_row_pixels.saturating_sub(src_x_offset));
        }
        let destination_row_pixels = screen_rb.saturating_mul(8) / u32::from(pixel_size);
        bounded_col_count = bounded_col_count.min(destination_row_pixels.saturating_sub(dst_x));

        // Color QuickDraw treats PixMap pixels through their pmTable. When
        // HLE composites a front-window offscreen CGrafPort to the screen,
        // mirror the CopyBits indexed-color translation rule instead of raw
        // copying mismatched 8bpp indices. IM:V V-91, V-95, V-136.
        if is_cgraf_port && port_pixel_size == 8 && pixel_size == 8 {
            let screen_ctab_handle = Self::gdevice_ctab_handle(bus, self.main_gdevice_handle);
            let src_ctab_seed = Self::ctab_seed(bus, port_ctab_handle);
            let dst_ctab_seed = Self::ctab_seed(bus, screen_ctab_handle);
            let src_clut = self.read_port_clut(bus, port_ctab_handle);
            let dst_clut = self.read_port_clut(bus, screen_ctab_handle);
            let hardware_palette_active = self.device_clut != self.color_manager_clut;
            let skip_canonical_to_screen = Self::uses_canonical_system_8bpp_clut(&src_clut);
            if port_ctab_handle != screen_ctab_handle
                && matches!(src_ctab_seed, Some(src_seed) if src_seed != 0)
                && src_ctab_seed != dst_ctab_seed
                && !skip_canonical_to_screen
                && !hardware_palette_active
            {
                let translation = self.build_palette_translation(
                    bus,
                    &src_clut,
                    &dst_clut,
                    screen_ctab_handle,
                    8,
                    8,
                );
                // Row-buffered: one bulk read, a host-side table pass and
                // one bulk write per row instead of two bus calls per pixel.
                let mut src_row = vec![0u8; bounded_col_count as usize];
                let mut dst_row = vec![0u8; bounded_col_count as usize];
                for row in 0..bounded_row_count {
                    let src_addr = port_base + (src_y_offset + row) * port_rb + src_x_offset;
                    let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
                    bus.read_bytes_into(src_addr, &mut src_row);
                    for (dst, src) in dst_row.iter_mut().zip(src_row.iter()) {
                        *dst = translation[*src as usize];
                    }
                    bus.write_bytes(dst_addr, &dst_row);
                }
                return;
            }
        }

        // Indexed windows can remain at their original PixMap depth while
        // SetDepth changes the screen. Read and write each side using its own
        // packing geometry, and translate colors through the active screen
        // table whenever their depths differ. Imaging With QuickDraw (1994),
        // pp. 4-27--4-28 and 4-81--4-82.
        if matches!(port_pixel_size, 1 | 2 | 4 | 8) && matches!(pixel_size, 1 | 2 | 4 | 8) {
            let row_count = bounded_row_count;
            let col_count = bounded_col_count;
            let screen_ctab_handle = Self::gdevice_ctab_handle(bus, self.main_gdevice_handle);
            let packed_translation = {
                let needs_mono_translation = port_pixel_size == 1 || pixel_size == 1;
                let needs_depth_translation = port_pixel_size != u32::from(pixel_size);
                let different_color_spaces = if is_cgraf_port {
                    let src_ctab_seed = Self::ctab_seed(bus, port_ctab_handle);
                    let dst_ctab_seed = Self::ctab_seed(bus, screen_ctab_handle);
                    port_ctab_handle != 0
                        && screen_ctab_handle != 0
                        && port_ctab_handle != screen_ctab_handle
                        && matches!((src_ctab_seed, dst_ctab_seed), (Some(src), Some(dst)) if src != 0 && src != dst)
                        && self.device_clut == self.color_manager_clut
                } else {
                    false
                };
                if needs_mono_translation || needs_depth_translation || different_color_spaces {
                    let src_clut = if port_pixel_size == 1 && !is_cgraf_port {
                        let mut clut = [[0u16; 3]; 256];
                        clut[0] = [0xffff, 0xffff, 0xffff];
                        clut
                    } else {
                        self.read_port_clut(bus, port_ctab_handle)
                    };
                    let dst_clut = self.read_port_clut(bus, screen_ctab_handle);
                    let mut translation = self.build_palette_translation(
                        bus,
                        &src_clut,
                        &dst_clut,
                        screen_ctab_handle,
                        port_pixel_size,
                        u32::from(pixel_size),
                    );
                    if port_pixel_size == 1 || pixel_size == 1 {
                        let src_entry_count = 1usize << port_pixel_size;
                        let dst_entry_count = 1usize << pixel_size;
                        for (index, rgb) in src_clut.iter().take(src_entry_count).enumerate() {
                            if let Some(pixel) =
                                Self::fb_main_screen_pixel_index_for_rgb_with_entry_count(
                                    bus,
                                    *rgb,
                                    dst_entry_count,
                                )
                            {
                                translation[index] = pixel;
                            }
                        }
                    }
                    Some(translation)
                } else {
                    None
                }
            };
            let src_pixels_per_byte = 8 / port_pixel_size;
            let src_field_mask = ((1u16 << port_pixel_size) - 1) as u8;
            let dst_pixel_size = u32::from(pixel_size);
            let dst_pixels_per_byte = 8 / dst_pixel_size;
            let dst_field_mask = ((1u16 << dst_pixel_size) - 1) as u8;
            if row_count == 0 || col_count == 0 {
                return;
            }
            // Row-buffered: bulk-read the source span (and, for a packed
            // destination, the destination span the pixels merge into),
            // repack against host buffers, one bulk write per row — instead
            // of one or two bus calls per pixel.
            let src_first = (src_x_offset / src_pixels_per_byte) as usize;
            let src_last = ((src_x_offset + col_count - 1) / src_pixels_per_byte) as usize;
            let dst_first = (dst_x / dst_pixels_per_byte) as usize;
            let dst_last = ((dst_x + col_count - 1) / dst_pixels_per_byte) as usize;
            let mut src_buf = vec![0u8; src_last - src_first + 1];
            let mut dst_buf = vec![0u8; dst_last - dst_first + 1];
            for row in 0..row_count {
                let src_row = port_base + (src_y_offset + row) * port_rb;
                let dst_row = screen_base + (dst_y + row) * screen_rb;
                bus.read_bytes_into(src_row + src_first as u32, &mut src_buf);
                if dst_pixel_size != 8 {
                    bus.read_bytes_into(dst_row + dst_first as u32, &mut dst_buf);
                }
                for col in 0..col_count {
                    let src_x = src_x_offset + col;
                    let src_index = if port_pixel_size == 8 {
                        src_buf[src_x as usize - src_first]
                    } else {
                        let src_byte = src_buf[(src_x / src_pixels_per_byte) as usize - src_first];
                        let src_shift =
                            8 - port_pixel_size - (src_x % src_pixels_per_byte) * port_pixel_size;
                        (src_byte >> src_shift) & src_field_mask
                    };
                    // Window Manager compositing observes each PixMap's
                    // pmTable just like indexed CopyBits. Restrict inverse
                    // matching to the entries representable at the packed
                    // destination depth; raw index copying is valid only when
                    // the two tables identify the same color space. Imaging
                    // With QuickDraw (1994), pp. 4-27--4-28 and 4-81--4-82.
                    let dst_index = packed_translation
                        .as_ref()
                        .map_or(src_index, |translation| translation[src_index as usize]);
                    let dst_x = dst_x + col;
                    if dst_pixel_size == 8 {
                        dst_buf[dst_x as usize - dst_first] = dst_index;
                    } else {
                        debug_assert!(dst_index <= dst_field_mask);
                        let index = (dst_x / dst_pixels_per_byte) as usize - dst_first;
                        let dst_shift =
                            8 - dst_pixel_size - (dst_x % dst_pixels_per_byte) * dst_pixel_size;
                        let dst_mask = dst_field_mask << dst_shift;
                        dst_buf[index] = (dst_buf[index] & !dst_mask) | (dst_index << dst_shift);
                    }
                }
                bus.write_bytes(dst_row + dst_first as u32, &dst_buf);
            }
            return;
        }

        if port_pixel_size != pixel_size as u32 {
            return;
        }

        // block_move per row.
        for row in 0..row_count {
            let src_addr = port_base + (src_y_offset + row) * port_rb + src_x_offset;
            let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
            bus.block_move(src_addr, dst_addr, col_count);
        }
    }

    /// Explicit screen frame that narrowly encloses a retained app-managed
    /// CPort PixMap. This correlates guest QuickDraw geometry without assuming
    /// that the offscreen buffer is centered.
    pub fn framed_manual_cport_presentation_rect(
        &self,
        bus: &MacMemoryBus,
    ) -> Option<super::dispatch::ScreenCopyBitsRect> {
        let frame = self.last_screen_frame_rect?;
        let (_, _, screen_width, screen_height, screen_depth) = self.screen_mode;
        let frame_width = frame.dst_right.saturating_sub(frame.dst_left);
        let frame_height = frame.dst_bottom.saturating_sub(frame.dst_top);
        if frame.dst_top < 0
            || frame.dst_left < 0
            || frame.dst_bottom > screen_height as i16
            || frame.dst_right > screen_width as i16
            || frame_width <= 0
            || frame_height <= 0
        {
            return None;
        }

        for &port in &self.cport_ports {
            if self.window_list.contains(&port)
                || self.gworld_devices.contains_key(&port)
                || (bus.read_word(port + 6) & 0xC000) != 0xC000
            {
                continue;
            }
            let pixmap_handle = bus.read_long(port + 2);
            let pixmap = (pixmap_handle != 0)
                .then(|| bus.read_long(pixmap_handle))
                .unwrap_or(0);
            if pixmap == 0
                || (Self::offscreen_pixmap_base_ptr(bus, pixmap) & 0x3FFF_FFFF)
                    == self.screen_mode.0
                || bus.read_word(pixmap + 32) != screen_depth
            {
                continue;
            }
            let pixmap_height = (bus.read_word(pixmap + 10) as i16)
                .saturating_sub(bus.read_word(pixmap + 6) as i16);
            let pixmap_width = (bus.read_word(pixmap + 12) as i16)
                .saturating_sub(bus.read_word(pixmap + 8) as i16);
            let horizontal_frame = frame_width.saturating_sub(pixmap_width);
            let vertical_frame = frame_height.saturating_sub(pixmap_height);

            // SetPortPix alone does not make this image visible: Imaging With
            // QuickDraw (1994), pp. 4-86..4-87 explicitly describes it as a
            // way to draw into a buffer other than the screen. Here the image
            // dimensions are used only to interpret a narrow FrameRect that
            // the guest separately drew into the screen framebuffer. The HLE
            // presentation path still refuses to copy an attached scratch
            // PixMap merely because its dimensions look plausible.
            if pixmap_width > 0
                && pixmap_height > 0
                && horizontal_frame == vertical_frame
                && (2..=16).contains(&horizontal_frame)
                && horizontal_frame % 2 == 0
            {
                return Some(frame);
            }
        }
        None
    }

    /// Geometry declared by a large app-managed color port behind a visible,
    /// screen-backed fullscreen window. Games commonly create this port before
    /// drawing their first useful frame, so the frontend can size itself
    /// without waiting for pixel-based border detection.
    pub fn declared_centered_presentation_rect(
        &self,
        bus: &MacMemoryBus,
    ) -> Option<super::dispatch::ScreenCopyBitsRect> {
        let (screen_base, _, screen_width, screen_height, screen_depth) = self.screen_mode;
        let screen_width = u32::from(screen_width);
        let screen_height = u32::from(screen_height);
        if self.front_window == 0
            || screen_width == 0
            || screen_height == 0
            || !self.window_visible(bus, self.front_window)
        {
            return None;
        }

        // Only trust offscreen geometry when the visible front window itself
        // is the fullscreen, screen-backed presentation surface.
        let front = self.front_window;
        if (bus.read_word(front + 6) & 0xC000) != 0xC000 {
            return None;
        }
        let front_pixmap_handle = bus.read_long(front + 2);
        let front_pixmap = (front_pixmap_handle != 0)
            .then(|| bus.read_long(front_pixmap_handle))
            .unwrap_or(0);
        if front_pixmap == 0 || (bus.read_long(front_pixmap) & 0x3FFF_FFFF) != screen_base {
            return None;
        }
        let port_covers_screen = (bus.read_word(front + 16) as i16) <= 0
            && (bus.read_word(front + 18) as i16) <= 0
            && (bus.read_word(front + 20) as i16) >= screen_height as i16
            && (bus.read_word(front + 22) as i16) >= screen_width as i16;
        let (top, left, bottom, right) = self.window_bounds;
        let window_covers_screen =
            top <= 0 && left <= 0 && bottom >= screen_height as i16 && right >= screen_width as i16;
        if !port_covers_screen && !window_covers_screen {
            return None;
        }

        // Prefer the largest substantial non-screen color port. Dialog and
        // sprite scratch ports are excluded by the half-screen area threshold.
        let min_area = u64::from(screen_width) * u64::from(screen_height) / 2;
        let mut best: Option<(u64, i16, i16, i16, i16)> = None;
        for &port in &self.cport_ports {
            if port == front
                || self.window_list.contains(&port)
                || self.gworld_devices.contains_key(&port)
                || (bus.read_word(port + 6) & 0xC000) != 0xC000
            {
                continue;
            }
            let pixmap_handle = bus.read_long(port + 2);
            // Imaging With QuickDraw 1994, pp. 4-86..4-87: SetPortPix
            // replaces the current CGrafPort's portPixMap. It is commonly
            // used to draw into an offscreen image that the application later
            // transfers explicitly. Do not infer visibility from a PixMap
            // attached this way merely because its dimensions look like a
            // scene buffer.
            if self
                .cport_original_pixmaps
                .get(&port)
                .is_none_or(|&original| original != pixmap_handle)
            {
                continue;
            }
            let pixmap = (pixmap_handle != 0)
                .then(|| bus.read_long(pixmap_handle))
                .unwrap_or(0);
            if pixmap == 0 || bus.read_word(pixmap + 32) != screen_depth {
                continue;
            }
            let src_top = bus.read_word(pixmap + 6) as i16;
            let src_left = bus.read_word(pixmap + 8) as i16;
            let src_bottom = bus.read_word(pixmap + 10) as i16;
            let src_right = bus.read_word(pixmap + 12) as i16;
            if src_bottom <= src_top || src_right <= src_left {
                continue;
            }
            let width = (src_right - src_left) as u32;
            let height = (src_bottom - src_top) as u32;
            let area = u64::from(width) * u64::from(height);
            if width > screen_width
                || height > screen_height
                || (width == screen_width && height == screen_height)
                || area < min_area
            {
                continue;
            }
            if best.map(|current| area > current.0).unwrap_or(true) {
                best = Some((area, src_top, src_left, src_bottom, src_right));
            }
        }

        let (_, src_top, src_left, src_bottom, src_right) = best?;
        let width = (src_right - src_left) as u32;
        let height = (src_bottom - src_top) as u32;
        let dst_left = ((screen_width - width) / 2) as i16;
        let dst_top = ((screen_height - height) / 2) as i16;
        Some(super::dispatch::ScreenCopyBitsRect {
            src_top,
            src_left,
            src_bottom,
            src_right,
            dst_top,
            dst_left,
            dst_bottom: dst_top.saturating_add(height as i16),
            dst_right: dst_left.saturating_add(width as i16),
        })
    }

    pub(crate) fn blit_large_manual_cport_to_screen(&mut self, bus: &mut MacMemoryBus) {
        let (screen_base, screen_rb, screen_w, screen_h, pixel_size) = self.screen_mode;
        let screen_w_u32 = u32::from(screen_w);
        let screen_h_u32 = u32::from(screen_h);
        let trace = std::env::var_os("SYSTEMLESS_TRACE_BLIT_WINDOW").is_some();
        let latched_port = self.manual_cport_presented_port;

        if self.front_window == 0
            || pixel_size != 8
            || screen_w == 0
            || screen_h == 0
            || (self.copybits_screen_count != 0 && latched_port == 0)
        {
            if trace {
                eprintln!(
                    "[BLIT-CPORT] skip: front=${:08X} pixel_size={} screen={}x{} copybits={} latched=${:08X}",
                    self.front_window,
                    pixel_size,
                    screen_w,
                    screen_h,
                    self.copybits_screen_count,
                    latched_port
                );
            }
            return;
        }
        if !self.window_visible(bus, self.front_window) {
            if trace {
                eprintln!(
                    "[BLIT-CPORT] skip: front=${:08X} is not visible",
                    self.front_window
                );
            }
            return;
        }

        let front_port = self.front_window;
        let front_is_cgraf = (bus.read_word(front_port + 6) & 0xC000) == 0xC000;
        let front_base = if front_is_cgraf {
            let pm_handle = bus.read_long(front_port + 2);
            let pm_ptr = (pm_handle != 0)
                .then(|| bus.read_long(pm_handle))
                .unwrap_or(0);
            if pm_ptr == 0 {
                return;
            }
            Self::offscreen_pixmap_base_ptr(bus, pm_ptr) & 0x3FFF_FFFF
        } else {
            bus.read_long(front_port + 2)
        };
        if front_base != screen_base {
            if trace {
                eprintln!(
                    "[BLIT-CPORT] skip: front base ${:08X} != screen ${:08X}",
                    front_base, screen_base
                );
            }
            return;
        }

        let front_top = bus.read_word(front_port + 16) as i16;
        let front_left = bus.read_word(front_port + 18) as i16;
        let front_bottom = bus.read_word(front_port + 20) as i16;
        let front_right = bus.read_word(front_port + 22) as i16;
        let port_rect_covers_screen = front_top <= 0
            && front_left <= 0
            && front_bottom >= screen_h as i16
            && front_right >= screen_w as i16;
        let (wt, wl, wb, wr) = self.window_bounds;
        let window_bounds_cover_screen =
            wt <= 0 && wl <= 0 && wb >= screen_h as i16 && wr >= screen_w as i16;
        let front_covers_presentation = port_rect_covers_screen || window_bounds_cover_screen;
        let screen_is_dark =
            self.screen_is_dark_for_manual_cport(bus, screen_base, screen_rb, screen_w, screen_h);
        let dark_screen_allows_presentation = !front_covers_presentation && screen_is_dark;

        #[derive(Clone, Copy)]
        struct Candidate {
            port: u32,
            base: u32,
            row_bytes: u32,
            width: u32,
            height: u32,
            ctab_handle: u32,
        }

        let screen_area = u64::from(screen_w_u32) * u64::from(screen_h_u32);
        let min_area = (screen_area / 8).max(1);
        let mut best: Option<Candidate> = None;
        let mut considered_ports = 0u32;
        let mut rejected_shape = 0u32;
        let mut rejected_replaced_pixmap = 0u32;
        let mut rejected_area = 0u32;
        for &port in &self.cport_ports {
            if port == front_port
                || self.window_list.contains(&port)
                || self.gworld_devices.contains_key(&port)
                || (self.copybits_screen_count != 0 && latched_port != 0 && port != latched_port)
            {
                continue;
            }
            considered_ports += 1;
            if (bus.read_word(port + 6) & 0xC000) != 0xC000 {
                rejected_shape += 1;
                continue;
            }
            let pm_handle = bus.read_long(port + 2);
            if pm_handle == 0 {
                rejected_shape += 1;
                continue;
            }
            // SetPortPix changes the drawing target; it does not publish that
            // target to the screen. The manual compatibility bridge is only
            // allowed to consider the PixMap originally installed by
            // OpenCPort/InitCPort. IWQD 1994, pp. 4-86..4-87.
            if self
                .cport_original_pixmaps
                .get(&port)
                .is_none_or(|&original| original != pm_handle)
            {
                rejected_replaced_pixmap += 1;
                continue;
            }
            let pm_ptr = bus.read_long(pm_handle);
            if pm_ptr == 0 {
                rejected_shape += 1;
                continue;
            }

            // CGrafPort/PixMap layout follows Imaging With QuickDraw
            // 1994, pp. 4-64..4-65: portPixMap is a PixMapHandle, with
            // baseAddr, rowBytes, bounds, pixelSize, and pmTable here.
            let base = Self::offscreen_pixmap_base_ptr(bus, pm_ptr) & 0x3FFF_FFFF;
            let row_bytes = (bus.read_word(pm_ptr + 4) & 0x3FFF) as u32;
            let top = bus.read_word(pm_ptr + 6) as i16;
            let left = bus.read_word(pm_ptr + 8) as i16;
            let bottom = bus.read_word(pm_ptr + 10) as i16;
            let right = bus.read_word(pm_ptr + 12) as i16;
            let width = (right - left).max(0) as u32;
            let height = (bottom - top).max(0) as u32;
            let port_pixel_size = bus.read_word(pm_ptr + 32) as u32;
            if base == 0
                || base == screen_base
                || port_pixel_size != 8
                || width == 0
                || height == 0
                || width > screen_w_u32
                || height > screen_h_u32
                || row_bytes < width
            {
                rejected_shape += 1;
                continue;
            }
            let area = u64::from(width) * u64::from(height);
            if area < min_area {
                rejected_area += 1;
                continue;
            }

            let candidate = Candidate {
                port,
                base,
                row_bytes,
                width,
                height,
                ctab_handle: bus.read_long(pm_ptr + 42),
            };
            if best
                .map(|current| area > u64::from(current.width) * u64::from(current.height))
                .unwrap_or(true)
            {
                best = Some(candidate);
            }
        }

        let Some(candidate) = best else {
            if trace {
                eprintln!(
                    "[BLIT-CPORT] skip: no candidate (tracked={}, considered={}, shape_rejects={}, replaced_pixmap_rejects={}, area_rejects={}, min_area={})",
                    self.cport_ports.len(),
                    considered_ports,
                    rejected_shape,
                    rejected_replaced_pixmap,
                    rejected_area,
                    min_area
                );
            }
            return;
        };
        if latched_port != candidate.port {
            let (visible_samples, distinct_indices) = self.manual_cport_sample_content(
                bus,
                candidate.base,
                candidate.row_bytes,
                candidate.width,
                candidate.height,
            );
            if visible_samples < 8 || distinct_indices < 2 {
                if trace {
                    eprintln!(
                        "[BLIT-CPORT] skip: candidate port=${:08X} base=${:08X} {}x{} has too little sampled content (visible={}, distinct={})",
                        candidate.port,
                        candidate.base,
                        candidate.width,
                        candidate.height,
                        visible_samples,
                        distinct_indices
                    );
                }
                return;
            }
            if !screen_is_dark {
                if trace {
                    eprintln!(
                        "[BLIT-CPORT] skip: candidate port=${:08X} base=${:08X} {}x{} cannot latch over visible screen content (samples={})",
                        candidate.port, candidate.base, candidate.width, candidate.height, visible_samples
                    );
                }
                return;
            }
        }
        if !front_covers_presentation && !dark_screen_allows_presentation {
            if trace {
                eprintln!(
                    "[BLIT-CPORT] skip: front portRect ({},{},{},{}) and window bounds ({},{},{},{}) do not cover {}x{}, screen is not dark enough for fallback presentation",
                    front_top,
                    front_left,
                    front_bottom,
                    front_right,
                    wt,
                    wl,
                    wb,
                    wr,
                    screen_w,
                    screen_h
                );
            }
            return;
        }
        let dst_x = (screen_w_u32 - candidate.width) / 2;
        let dst_y = (screen_h_u32 - candidate.height) / 2;
        let row_count = candidate.height.min(screen_h_u32.saturating_sub(dst_y));
        let col_count = candidate.width.min(screen_w_u32.saturating_sub(dst_x));
        if row_count == 0 || col_count == 0 {
            return;
        }

        let current_screen_witness = Self::manual_cport_screen_witness(
            bus,
            screen_base,
            screen_rb,
            dst_x,
            dst_y,
            col_count,
            row_count,
        );
        if latched_port == candidate.port && !self.manual_cport_screen_witness.is_empty() {
            let changed_samples = current_screen_witness
                .iter()
                .zip(&self.manual_cport_screen_witness)
                .filter(|(current, previous)| current != previous)
                .count();
            // A few changed samples can be cursor or Window Manager activity.
            // Broad changes mean the application is now drawing to the
            // physical framebuffer itself, so replaying an earlier offscreen
            // compatibility surface would overwrite authoritative pixels.
            if changed_samples >= 8 {
                if trace {
                    eprintln!(
                        "[BLIT-CPORT] releasing port=${:08X}: screen changed at {} witness samples",
                        candidate.port, changed_samples
                    );
                }
                self.manual_cport_presented_port = 0;
                self.manual_cport_screen_witness.clear();
                return;
            }
        }
        self.manual_cport_presented_port = candidate.port;

        if trace {
            eprintln!(
                "[BLIT-CPORT] presenting port=${:08X} base=${:08X} {}x{} at {},{}",
                candidate.port, candidate.base, col_count, row_count, dst_x, dst_y
            );
        }

        let screen_ctab_handle = Self::gdevice_ctab_handle(bus, self.main_gdevice_handle);
        let src_ctab_seed = Self::ctab_seed(bus, candidate.ctab_handle);
        let dst_ctab_seed = Self::ctab_seed(bus, screen_ctab_handle);
        let src_clut = self.read_port_clut(bus, candidate.ctab_handle);
        let dst_clut = self.read_port_clut(bus, screen_ctab_handle);
        let hardware_palette_active = self.device_clut != self.color_manager_clut;
        let skip_canonical_to_screen = Self::uses_canonical_system_8bpp_clut(&src_clut);
        if candidate.ctab_handle != screen_ctab_handle
            && matches!(src_ctab_seed, Some(src_seed) if src_seed != 0)
            && src_ctab_seed != dst_ctab_seed
            && !skip_canonical_to_screen
            && !hardware_palette_active
        {
            let translation =
                self.build_palette_translation(bus, &src_clut, &dst_clut, screen_ctab_handle, 8, 8);
            for row in 0..row_count {
                let src_addr = candidate.base + row * candidate.row_bytes;
                let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
                for col in 0..col_count {
                    let src_idx = bus.read_byte(src_addr + col);
                    bus.write_byte(dst_addr + col, translation[src_idx as usize]);
                }
            }
            self.manual_cport_screen_witness = Self::manual_cport_screen_witness(
                bus,
                screen_base,
                screen_rb,
                dst_x,
                dst_y,
                col_count,
                row_count,
            );
            return;
        }

        for row in 0..row_count {
            let src_addr = candidate.base + row * candidate.row_bytes;
            let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
            bus.block_move(src_addr, dst_addr, col_count);
        }
        self.manual_cport_screen_witness = Self::manual_cport_screen_witness(
            bus,
            screen_base,
            screen_rb,
            dst_x,
            dst_y,
            col_count,
            row_count,
        );
    }

    fn manual_cport_screen_witness(
        bus: &MacMemoryBus,
        screen_base: u32,
        screen_row_bytes: u32,
        left: u32,
        top: u32,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let step_x = (width / 16).max(1);
        let step_y = (height / 12).max(1);
        let mut witness = Vec::with_capacity(16 * 12);
        let mut y = step_y / 2;
        while y < height {
            let mut x = step_x / 2;
            while x < width {
                witness.push(bus.read_byte(screen_base + (top + y) * screen_row_bytes + left + x));
                x += step_x;
            }
            y += step_y;
        }
        witness
    }

    /// Geometry of the app-managed CGrafPort that the HLE has actually
    /// selected and centered on the guest screen. Exposing only the latched
    /// presentation port keeps frontends from mistaking large scratch ports
    /// for the visible game viewport.
    pub fn manual_cport_presentation_rect(
        &self,
        bus: &MacMemoryBus,
    ) -> Option<super::dispatch::ScreenCopyBitsRect> {
        let port = self.manual_cport_presented_port;
        if port == 0 || (bus.read_word(port + 6) & 0xC000) != 0xC000 {
            return None;
        }
        let pixmap_handle = bus.read_long(port + 2);
        let pixmap = (pixmap_handle != 0)
            .then(|| bus.read_long(pixmap_handle))
            .unwrap_or(0);
        if pixmap == 0 {
            return None;
        }

        let (_, _, screen_width, screen_height, screen_depth) = self.screen_mode;
        if bus.read_word(pixmap + 32) != screen_depth {
            return None;
        }
        let src_top = bus.read_word(pixmap + 6) as i16;
        let src_left = bus.read_word(pixmap + 8) as i16;
        let src_bottom = bus.read_word(pixmap + 10) as i16;
        let src_right = bus.read_word(pixmap + 12) as i16;
        let width = u32::from(src_right.saturating_sub(src_left) as u16);
        let height = u32::from(src_bottom.saturating_sub(src_top) as u16);
        let screen_width = u32::from(screen_width);
        let screen_height = u32::from(screen_height);
        if width == 0 || height == 0 || width > screen_width || height > screen_height {
            return None;
        }

        let dst_left = ((screen_width - width) / 2) as i16;
        let dst_top = ((screen_height - height) / 2) as i16;
        Some(super::dispatch::ScreenCopyBitsRect {
            src_top,
            src_left,
            src_bottom,
            src_right,
            dst_top,
            dst_left,
            dst_bottom: dst_top.saturating_add(height as i16),
            dst_right: dst_left.saturating_add(width as i16),
        })
    }

    fn manual_cport_sample_content(
        &self,
        bus: &MacMemoryBus,
        base: u32,
        row_bytes: u32,
        width: u32,
        height: u32,
    ) -> (u32, u32) {
        if base == 0 || width == 0 || height == 0 || row_bytes < width {
            return (0, 0);
        }
        let visible = |idx: u8| {
            let rgb = self.device_clut[idx as usize];
            rgb[0] > 0x1111 || rgb[1] > 0x1111 || rgb[2] > 0x1111
        };
        let sample = |x: u32, y: u32| -> Option<u8> {
            if x >= width || y >= height {
                return None;
            }
            Some(bus.read_byte(base + y * row_bytes + x))
        };

        let mut visible_samples = 0u32;
        let mut sampled_indices = [false; 256];
        for (x, y) in [
            (0, 0),
            (width - 1, 0),
            (0, height - 1),
            (width - 1, height - 1),
            (width / 2, height / 2),
        ] {
            if let Some(index) = sample(x, y) {
                sampled_indices[index as usize] = true;
                if visible(index) {
                    visible_samples += 1;
                }
            }
        }

        let step_x = (width / 32).max(1);
        let step_y = (height / 24).max(1);
        let mut y = step_y / 2;
        while y < height {
            let mut x = step_x / 2;
            while x < width {
                if let Some(index) = sample(x, y) {
                    sampled_indices[index as usize] = true;
                    if visible(index) {
                        visible_samples += 1;
                    }
                }
                x += step_x;
            }
            y += step_y;
        }
        (
            visible_samples,
            sampled_indices
                .into_iter()
                .filter(|present| *present)
                .count() as u32,
        )
    }

    fn screen_is_dark_for_manual_cport(
        &self,
        bus: &MacMemoryBus,
        screen_base: u32,
        screen_rb: u32,
        screen_w: u16,
        screen_h: u16,
    ) -> bool {
        if screen_w == 0 || screen_h == 0 || self.copybits_screen_count != 0 {
            return false;
        }
        let max_component = 0x1111;
        let sample_step_x = (u32::from(screen_w) / 16).max(1);
        let sample_step_y = (u32::from(screen_h) / 12).max(1);
        let mut y = sample_step_y / 2;
        while y < u32::from(screen_h) {
            let row = screen_base + y * screen_rb;
            let mut x = sample_step_x / 2;
            while x < u32::from(screen_w) {
                let idx = bus.read_byte(row + x) as usize;
                let rgb = self.device_clut[idx];
                if rgb[0] > max_component || rgb[1] > max_component || rgb[2] > max_component {
                    return false;
                }
                x += sample_step_x;
            }
            y += sample_step_y;
        }
        true
    }

    /// Draw window chrome (title bar, close box, border) into the framebuffer
    /// WIND bounds are the CONTENT RECT; title bar is drawn ABOVE it.
    pub(crate) fn draw_window_chrome(&self, bus: &mut MacMemoryBus, active: bool) {
        if self.windows_placed_offscreen.contains(&self.front_window) {
            return;
        }
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let screen = (
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
        );
        let (wind_top, wind_left, wind_bottom, wind_right) = self.window_bounds;
        let font_id: i16 = 0;
        let font_size: i16 = 12;
        let metrics = get_font_metrics(font_id, font_size);
        let title_width = self.window_title.chars().fold(0i16, |width, ch| {
            width.saturating_add(
                get_glyph(font_id, font_size, ch)
                    .map(|(glyph, _)| glyph.advance as i16)
                    .unwrap_or(6),
            )
        });

        // Title bar area: drawn ABOVE the content rect
        // Clamp to menu bar height — the Window Manager never draws
        // chrome into the menu bar area.
        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let chrome = crate::window_manager::standard_window_chrome(
            self.window_bounds,
            menu_bar_height,
            title_width,
            metrics.ascent,
            metrics.descent,
            !self.window_title.is_empty(),
            active,
            Self::window_is_document_proc(self.window_proc_id),
            self.go_away_flag,
            matches!(self.window_proc_id, 8 | 12),
        );
        let (tb_top, tb_left, tb_bottom_exclusive, tb_right) = chrome.background;
        let tb_bottom = tb_bottom_exclusive.saturating_sub(1);

        if self.ui_theme_id() != UiThemeId::ClassicSystem7 {
            // Keep the canonical WDEF geometry and title metrics while
            // applying Systemless-owned colors. System 7 explicitly allowed
            // color to enhance standard window frames; the controls and
            // measurements remain the familiar Macintosh ones.
            // Macintosh Human Interface Guidelines (1992), pp. 156--168.
            let palette = self.ui_theme().palette();
            self.fill_theme_rect(bus, chrome.background, palette.frame_light);
            for rect in chrome.ink.iter().copied() {
                self.fill_theme_rect(bus, rect, palette.frame_dark);
            }
            if active {
                // Repaint the long pinstripe runs in the logo blue. Short
                // close/zoom glyph strokes remain dark and retain their
                // classic legibility.
                for rect in chrome.ink.iter().copied() {
                    let width = rect.3.saturating_sub(rect.1);
                    if rect.2.saturating_sub(rect.0) == 1
                        && width >= 14
                        && rect.0 > tb_top
                        && rect.0 < tb_bottom
                    {
                        self.fill_theme_rect(bus, rect, palette.selection);
                    }
                }
            }
            if !self.window_title.is_empty() {
                Self::fb_draw_string_clipped(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    chrome.title_h,
                    chrome.title_baseline,
                    &self.window_title,
                    font_id,
                    font_size,
                    chrome.title_clip,
                );
            }
            self.capture_gui_frame(bus, "draw_window_chrome");
            return;
        }

        // Fill title bar with white (exclusive bottom)
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            tb_top,
            tb_left,
            tb_bottom + 1,
            tb_right,
            false,
        );

        let is_movable_modal = self.window_proc_id == 5;
        let has_go_away =
            active && Self::window_is_document_proc(self.window_proc_id) && self.go_away_flag;
        let has_zoom_box = active && matches!(self.window_proc_id, 8 | 12);

        // The title bar is part of the standard Window Manager frame and is
        // enclosed by the window outline. Macintosh Toolbox Essentials
        // (1992), Figure 4-2, pp. 4-5--4-6.
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            tb_top,
            tb_left,
            tb_right,
            true,
        );
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            tb_bottom,
            tb_left,
            tb_right,
            true,
        );
        // Left and right border of title bar
        Self::fb_vline(bus, screen, tb_left, tb_top, tb_bottom + 1, true);
        Self::fb_vline(bus, screen, tb_right - 1, tb_top, tb_bottom + 1, true);

        let (title_clear_left, title_clear_right) = if !self.window_title.is_empty() {
            let text_x = chrome.title_h;
            (text_x - 8, text_x + title_width + 8)
        } else {
            (tb_right, tb_right) // No clear area
        };

        let _close_box_width = if has_go_away { 15i16 } else { 0 };

        if is_movable_modal && !active {
            // Inactive movableDBoxProc: plain title bar, no stripes
            // Just draw the title text centered
            if !self.window_title.is_empty() {
                let text_x = title_clear_left + 8;
                Self::fb_draw_string(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    text_x,
                    chrome.title_baseline,
                    &self.window_title,
                    font_id,
                    font_size,
                );
            }
        } else {
            // documentProc/noGrowDocProc: stripes + optional close box

            // Draw close box if goAwayFlag is set.
            //
            // Classic Mac System 7.5.3 close-box graphic per BasiliskII reference
            // (window_goaway): NOT a clean FrameRect. The WDEF draws an 11×11
            // bounding region split into two shapes:
            //   * top-left  L-shape — top horizontal (11 wide) + left vertical
            //                         (11 tall), painting the 3D-highlight edge
            //   * bottom-right Γ-shape — right vertical (8 tall, inset 2 from
            //                            top + 1 from bottom) + bottom
            //                            horizontal (8 wide, inset 2 from left
            //                            + 1 from right), painting the inner
            //                            close-box outline
            // The 1-pixel gap between the two shapes gives the close box its
            // characteristic 3D-button appearance.
            // Inside Macintosh Volume V, V-188 figure 5-3.
            if has_go_away {
                let cb_size: i16 = 11;
                let interior_top = tb_top + 1;
                let interior_height = tb_bottom - interior_top;
                let cb_top = interior_top + (interior_height - cb_size) / 2;
                let cb_left = tb_left + 9; // 1px border + 8px padding

                // Top-left L: full 11-wide top edge + full 11-tall left edge
                Self::fb_hline(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    cb_top,
                    cb_left,
                    cb_left + cb_size,
                    true,
                );
                Self::fb_vline(bus, screen, cb_left, cb_top, cb_top + cb_size, true);

                // Bottom-right Γ: 8-tall right edge + 8-wide bottom edge,
                // inset 2 from the top-left and 1 from the bottom-right.
                let inner_right = cb_left + cb_size - 2; // x=cb_left+9
                let inner_bottom = cb_top + cb_size - 2; // y=cb_top+9
                Self::fb_vline(
                    bus,
                    screen,
                    inner_right,
                    cb_top + 2,
                    cb_top + cb_size - 1,
                    true,
                );
                Self::fb_hline(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    inner_bottom,
                    cb_left + 2,
                    cb_left + cb_size - 1,
                    true,
                );
            }

            for (top, left, bottom, right) in chrome.zoom_ink.iter().copied() {
                Self::fb_fill_rect(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    top,
                    left,
                    bottom,
                    right,
                    true,
                );
            }

            // Draw horizontal stripe pattern in title bar (classic Mac pinstripes)
            // Only active windows get stripes; inactive windows have plain white title bars
            //
            // System 7.5.3 reserves only 6 px of clear-area on each side of
            // the title text for stripes (the 16-px `title_clear_left/right`
            // margin is for text-glyph hit-testing, not for stripes). The
            // active document WDEF paints pinstripe rows at title-bar offsets
            // 1, 3, and 5; this row placement is calibrated against the
            // BasiliskII System 7.5.3 reference.
            // Inside Macintosh Volume V, V-188 figure 5-3.
            if active {
                let stripe_left_edge = tb_left + 2;
                let stripe_right_end = if has_zoom_box {
                    // Keep the pinstripes flush with the rightmost 15-pixel
                    // zoom/scrollbar control column used by the shared WDEF
                    // geometry. Macintosh Toolbox Essentials (1992), Figure
                    // 4-2 and Listing 5-17.
                    wind_right - 15
                } else {
                    tb_right - 2
                };
                let stripe_text_left = title_clear_left + 2;
                let stripe_text_right = title_clear_right - 2;

                // Close box region to skip (if present)
                let (cb_gap_left, cb_gap_right) = if has_go_away {
                    let cb_left = tb_left + 9;
                    let cb_right = cb_left + 10; // QD exclusive right
                    (cb_left - 1, cb_right + 2) // 1px gap left, 2px gap right
                } else {
                    (stripe_right_end, stripe_right_end) // no gap
                };

                for y in (tb_top + 1)..=(tb_bottom - 4) {
                    if (y - tb_top) % 2 == 1 {
                        // Draw stripe segments, skipping close box and title text gaps
                        // Segment 1: left edge to close box (or title text if no close box)
                        let seg1_end = if has_go_away {
                            cb_gap_left
                        } else {
                            stripe_text_left
                        };
                        if stripe_left_edge < seg1_end {
                            Self::fb_hline(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                y,
                                stripe_left_edge,
                                seg1_end,
                                true,
                            );
                        }
                        // Segment 2: after close box to title text (only if close box exists)
                        if has_go_away && cb_gap_right < stripe_text_left {
                            Self::fb_hline(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                y,
                                cb_gap_right,
                                stripe_text_left,
                                true,
                            );
                        }
                        // Segment 3: after title text to right edge
                        if stripe_text_right < stripe_right_end {
                            Self::fb_hline(
                                bus,
                                screen_base,
                                row_bytes,
                                pixel_size,
                                screen_width,
                                screen_height,
                                y,
                                stripe_text_right,
                                stripe_right_end,
                                true,
                            );
                        }
                    }
                }
            }

            // Draw title text centered in title bar. Active windows get
            // stripes and a close box; inactive windows keep the title text
            // over a plain title bar.
            if !self.window_title.is_empty() {
                let text_x = title_clear_left + 8;
                Self::fb_draw_string_clipped(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    text_x,
                    chrome.title_baseline,
                    &self.window_title,
                    font_id,
                    font_size,
                    (tb_top, tb_left, tb_bottom - 2, tb_right),
                );
            }
        }

        // Draw window content area border
        Self::fb_vline(bus, screen, wind_left - 1, wind_top, wind_bottom, true);
        Self::fb_vline(bus, screen, wind_right, wind_top, wind_bottom, true);
        // Bottom border line
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            wind_bottom,
            wind_left - 1,
            wind_right + 1,
            true,
        );

        // Shadow effect
        Self::fb_vline(
            bus,
            screen,
            wind_right + 1,
            tb_top,
            (wind_bottom + 1) + 1,
            true,
        );
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            wind_bottom + 1,
            tb_left + 1,
            wind_right + 2,
            true,
        );
        self.capture_gui_frame(bus, "draw_window_chrome");
    }

    /// Draw the grow icon (size box) in the bottom-right corner of a window.
    /// The grow icon is a 15x15 area at the intersection of scroll bars.
    /// Inside Macintosh Volume I, I-296
    pub(crate) fn draw_grow_icon(&self, bus: &mut MacMemoryBus, window_ptr: u32) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        // Read portRect (top, left, bottom, right) from the window record
        let port_top = bus.read_word(window_ptr + 16) as i16;
        let port_left = bus.read_word(window_ptr + 18) as i16;
        let port_bottom = bus.read_word(window_ptr + 20) as i16;
        let port_right = bus.read_word(window_ptr + 22) as i16;
        // Read PixMap bounds to get the origin offset
        let port_version = bus.read_word(window_ptr + 6);
        let (scr_top, scr_left) = if (port_version & 0xC000) == 0xC000 {
            let pm_handle = bus.read_long(window_ptr + 2);
            if pm_handle != 0 {
                let pm_ptr = bus.read_long(pm_handle);
                let bt = bus.read_word(pm_ptr + 6) as i16;
                let bl = bus.read_word(pm_ptr + 8) as i16;
                (-bt, -bl)
            } else {
                (0, 0)
            }
        } else {
            let bt = bus.read_word(window_ptr + 8) as i16;
            let bl = bus.read_word(window_ptr + 10) as i16;
            (-bt, -bl)
        };

        // Content area in screen coordinates
        let content_top = scr_top + port_top;
        let content_left = scr_left + port_left;
        let content_bottom = scr_top + port_bottom;
        let content_right = scr_left + port_right;

        let proc_id = self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0);
        if !matches!(proc_id, 0 | 8) {
            return;
        }
        let active = bus.read_byte(window_ptr + Self::WINDOW_HILITED_OFFSET) != 0;
        // DrawGrowIcon uses the Window Manager port and is clipped by the
        // structure regions of windows above the target. The HLE renderer
        // writes screen pixels directly, so preserve those occluded pixels.
        // Macintosh Toolbox Essentials (1992), pp. 4-106 and 4-111--4-112.
        let content = (content_top, content_left, content_bottom, content_right);
        let preserved_front_pixels: Vec<_> = self
            .window_list
            .iter()
            .take_while(|&&front_window| front_window != window_ptr)
            .filter(|&&front_window| self.window_visible(bus, front_window))
            .filter_map(|&front_window| {
                self.window_structure_rect(bus, front_window)
                    .and_then(|front_structure| Self::rect_intersection(content, front_structure))
                    .and_then(|overlap| self.save_screen_rect_pixels(bus, overlap))
            })
            .collect();
        let icon = crate::window_manager::standard_grow_icon(content, active);
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            icon.background.0,
            icon.background.1,
            icon.background.2,
            icon.background.3,
            false,
        );
        for (top, left, bottom, right) in icon.ink {
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top,
                left,
                bottom,
                right,
                true,
            );
        }
        for (top, left, width, height, pixels) in preserved_front_pixels {
            self.restore_screen_rect_pixels(bus, top, left, width, height, &pixels);
        }
    }

    /// Draw a 2-pixel thick rectangle border (FrameRect with PenSize 2,2).
    /// Coordinates are in QuickDraw convention: (top, left) inclusive, (bottom, right) exclusive.
    /// On the real Mac, the pen extends DOWN and RIGHT from each point,
    /// giving 2 rows at top but only 1 row at bottom (clipped to rect).
    pub(crate) fn draw_thick_rect_border(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let screen = (
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
        );
        // Top edge: 2 rows (pen at top extends to top+1)
        for dy in 0..2i16 {
            Self::fb_hline(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top + dy,
                left,
                right,
                true,
            );
        }
        // Bottom edge: 1 row at bottom-1 (pen at bottom-1 would extend to bottom, clipped)
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            bottom - 1,
            left,
            right,
            true,
        );
        // Left edge: 2 columns (pen at left extends to left+1)
        for dx in 0..2i16 {
            Self::fb_vline(bus, screen, left + dx, top, bottom, true);
        }
        // Right edge: 2 columns (right-2 and right-1, both inside the rect)
        for dx in 0..2i16 {
            Self::fb_vline(bus, screen, right - 2 + dx, top, bottom, true);
        }
    }

    /// Erase (fill white) the structure region for a window, then draw the frame.
    /// On a real Mac the WDEF erases the structure region before drawing borders.
    fn erase_structure_region(
        &self,
        bus: &mut MacMemoryBus,
        top: i16,
        left: i16,
        bottom: i16,
        right: i16,
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        Self::fb_fill_rect(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            top,
            left,
            bottom,
            right,
            false,
        );
    }

    /// Erase only the frame/shadow area around content, leaving the content
    /// pixels untouched. WDEFs erase their structure region before drawing
    /// borders, but in the HLE framebuffer the content area may already hold
    /// app-rendered pixels that should not be replaced with white.
    fn erase_structure_frame_around_content(
        &self,
        bus: &mut MacMemoryBus,
        structure: (i16, i16, i16, i16),
        content: (i16, i16, i16, i16),
    ) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (st, sl, sb, sr) = structure;
        let (ct, cl, cb, cr) = content;
        let mut erase = |top: i16, left: i16, bottom: i16, right: i16| {
            if bottom <= top || right <= left {
                return;
            }
            Self::fb_fill_rect(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                top,
                left,
                bottom,
                right,
                false,
            );
        };

        erase(st, sl, ct.min(sb), sr);
        erase(cb.max(st), sl, sb, sr);
        erase(ct.max(st), sl, cb.min(sb), cl.min(sr));
        erase(ct.max(st), cr.max(sl), cb.min(sb), sr);
    }

    /// Draw the window frame/border for a given procID.
    /// Called when a visible window is created to render its frame to the screen.
    /// This implements the standard WDEF rendering for each window type:
    ///   - plainDBox (2): Single 1-pixel border
    ///   - dBoxProc (1): Double border (outer 1px at ±8, inner 2px at ±5)
    ///   - altDBoxProc (3): Single border + 2-pixel drop shadow
    ///   - document/zoom document procs (0, 4, 8, 12, 16): Title bar chrome
    ///   - movableDBoxProc (5): Double border + title bar chrome
    ///
    /// Draws a single window's chrome inline by deriving its screen-coord
    /// bounds, title, and goAway flag from the WindowRecord. The dispatcher's
    /// per-window state is swapped in temporarily so draw_window_chrome reads
    /// the right context, then restored.
    pub(crate) fn draw_single_window_chrome_inline(
        &mut self,
        bus: &mut MacMemoryBus,
        window_ptr: u32,
        hilited: bool,
    ) {
        if window_ptr == 0 {
            return;
        }
        if bus.read_byte(window_ptr + 110u32) == 0 {
            return; // not visible
        }
        if self.windows_placed_offscreen.contains(&window_ptr) {
            return;
        }
        if self.window_uses_custom_def_proc(bus, window_ptr) {
            return;
        }
        // plainDBox (2), dBoxProc (1), and altDBoxProc (3) windows have
        // no title bar; dispatch to draw_window_frame rather than
        // draw_window_chrome (which paints title-bar chrome).
        let proc_id = self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0);
        let (wind_top, wind_left, wind_bottom, wind_right) = self
            .window_content_global_rect(bus, window_ptr)
            .unwrap_or_else(|| self.window_global_port_rect(bus, window_ptr));
        if wind_bottom <= wind_top || wind_right <= wind_left {
            return;
        }
        // ClipAbove excludes the complete structure region of every visible
        // window above the WDEF being drawn. The HLE WDEF paints directly into
        // screen RAM, so preserve those pixels around the raw frame draw.
        // Macintosh Toolbox Essentials (1992), pp. 4-106 and 4-118--4-119.
        let target_structure = self.window_structure_global_rect_for_proc(
            bus,
            (wind_top, wind_left, wind_bottom, wind_right),
            proc_id,
        );
        let preserved_front_pixels: Vec<_> = self
            .window_list
            .iter()
            .take_while(|&&front_window| front_window != window_ptr)
            .filter(|&&front_window| self.window_visible(bus, front_window))
            .filter_map(|&front_window| {
                self.window_structure_rect(bus, front_window)
                    .and_then(|front_structure| {
                        Self::rect_intersection(target_structure, front_structure)
                    })
                    .and_then(|overlap| self.save_screen_rect_pixels(bus, overlap))
            })
            .collect();
        let saved_bounds = self.window_bounds;
        let saved_title = self.window_title.clone();
        let saved_go_away = self.go_away_flag;
        let saved_proc = self.window_proc_id;
        self.window_bounds = (wind_top, wind_left, wind_bottom, wind_right);
        let title_h = bus.read_long(window_ptr + 134u32);
        self.window_title = if title_h != 0 {
            let title_p = bus.read_long(title_h);
            if title_p != 0 {
                String::from_utf8_lossy(&bus.read_pstring(title_p)).into_owned()
            } else {
                String::new()
            }
        } else {
            String::new()
        };
        self.go_away_flag = bus.read_byte(window_ptr + 112u32) != 0;
        self.window_proc_id = proc_id;
        if matches!(proc_id, 1..=3) {
            self.draw_window_frame(bus);
        } else {
            self.draw_window_chrome(bus, hilited);
        }
        self.window_bounds = saved_bounds;
        self.window_title = saved_title;
        self.go_away_flag = saved_go_away;
        self.window_proc_id = saved_proc;
        for (top, left, width, height, pixels) in preserved_front_pixels {
            self.restore_screen_rect_pixels(bus, top, left, width, height, &pixels);
        }
    }

    pub(crate) fn draw_window_frame(&self, bus: &mut MacMemoryBus) {
        if self.windows_placed_offscreen.contains(&self.front_window) {
            return;
        }
        let (wind_top, wind_left, wind_bottom, wind_right) = self.window_bounds;
        if matches!(self.window_proc_id, 1 | 2 | 3 | 5) {
            let frame = match self.window_proc_id {
                1 => (wind_top - 8, wind_left - 8, wind_bottom + 3, wind_right + 8),
                2 => (wind_top - 1, wind_left - 1, wind_bottom + 1, wind_right + 1),
                3 => (wind_top - 1, wind_left - 1, wind_bottom + 3, wind_right + 3),
                5 => (
                    wind_top - 23,
                    wind_left - 8,
                    wind_bottom + 8,
                    wind_right + 8,
                ),
                _ => (wind_top, wind_left, wind_bottom, wind_right),
            };
            if self.draw_theme_dialog_frame(
                bus,
                (wind_top, wind_left, wind_bottom, wind_right),
                frame,
                self.window_proc_id,
                true,
                false,
            ) {
                self.capture_gui_frame(bus, "draw_window_frame");
                return;
            }
        }

        match self.window_proc_id {
            2 => {
                // plainDBox: single 1-pixel black border, no chrome.
                // Inside Macintosh Volume I, I-275: plainDBox windows
                // get their canonical 1px border from the system WDEF.
                // The border sits OUTSIDE the content rect so it doesn't
                // paint over content the application drew inside.
                self.draw_rect_border(
                    bus,
                    wind_top - 1,
                    wind_left - 1,
                    wind_bottom + 1,
                    wind_right + 1,
                );
            }
            1 => {
                // dBoxProc: double border
                // Structure region = content expanded by 8
                // WDEF erases structure, then draws:
                //   1. Outer 1px border at (content-8, content-8, content+3, content+8)
                //   2. Inner 2px border at (content-5, content-5, content+3, content+5)
                // Note: bottom offset is +3 (not +8), making the border asymmetric.
                let struc_top = wind_top - 8;
                let struc_left = wind_left - 8;
                let struc_bottom = wind_bottom + 3;
                let struc_right = wind_right + 8;
                self.erase_structure_frame_around_content(
                    bus,
                    (struc_top, struc_left, struc_bottom, struc_right),
                    (wind_top, wind_left, wind_bottom, wind_right),
                );
                // Outer 1px border
                self.draw_rect_border(bus, struc_top, struc_left, struc_bottom, struc_right);
                // Inner 2px border (content-5 top/left/right, content+3 bottom)
                self.draw_thick_rect_border(
                    bus,
                    wind_top - 5,
                    wind_left - 5,
                    struc_bottom,
                    wind_right + 5,
                );
            }
            3 => {
                // altDBoxProc: single border + 2px drop shadow
                self.erase_structure_frame_around_content(
                    bus,
                    (wind_top - 1, wind_left - 1, wind_bottom + 3, wind_right + 3),
                    (wind_top, wind_left, wind_bottom, wind_right),
                );
                self.draw_rect_border(
                    bus,
                    wind_top - 1,
                    wind_left - 1,
                    wind_bottom + 1,
                    wind_right + 1,
                );
                // Shadow starts just below/right of the border (border bottom is at wind_bottom)
                self.draw_shadow(
                    bus,
                    wind_top - 1,
                    wind_left - 1,
                    wind_bottom + 1,
                    wind_right + 1,
                );
            }
            5 => {
                // movableDBoxProc: double border wrapping title bar space + content
                // The outer border extends above the content to leave room for a
                // title bar area (18px), but no title bar chrome is drawn inside.
                // Outer border: symmetric ±8 except top adds 15 for title bar space
                let struc_top = wind_top - 23;
                let struc_left = wind_left - 8;
                let struc_bottom = wind_bottom + 8;
                let struc_right = wind_right + 8;
                self.erase_structure_frame_around_content(
                    bus,
                    (struc_top, struc_left, struc_bottom, struc_right),
                    (wind_top, wind_left, wind_bottom, wind_right),
                );
                // Outer 1px border
                self.draw_rect_border(bus, struc_top, struc_left, struc_bottom, struc_right);
                // Inner 2px border around content area (±5)
                let thick_top = wind_top - 5;
                let thick_left = wind_left - 5;
                let thick_bottom = wind_bottom + 5;
                let thick_right = wind_right + 5;
                self.draw_thick_rect_border(bus, thick_top, thick_left, thick_bottom, thick_right);
                // draw_thick_rect_border draws 1 row at bottom; movDBox needs 2 rows
                let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
                    self.get_screen_params();
                Self::fb_hline(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    thick_bottom - 2,
                    thick_left,
                    thick_right,
                    true,
                );
            }
            proc_id if Self::window_is_document_proc(proc_id) => {
                // Document-style windows with title bars
                // Erase structure region (content + title bar + 1px border + shadow)
                let tb_top = wind_top - 19;
                self.erase_structure_region(
                    bus,
                    tb_top,
                    wind_left - 1,
                    wind_bottom + 2,
                    wind_right + 2,
                );
                self.draw_window_chrome(bus, true);
            }
            _ => {
                // Unknown procID: at least draw a single border
                self.draw_rect_border(
                    bus,
                    wind_top - 1,
                    wind_left - 1,
                    wind_bottom + 1,
                    wind_right + 1,
                );
            }
        }
        self.capture_gui_frame(bus, "draw_window_frame");
    }

    pub(crate) fn restore_visible_dialog_snapshots(&mut self, bus: &mut MacMemoryBus) {
        if self.dialog_visible_snapshots.is_empty() {
            return;
        }
        let front_window = self.front_window;
        let front_is_dialog = front_window != 0 && self.dialog_items.contains_key(&front_window);
        let snapshots: Vec<_> = self
            .dialog_visible_snapshots
            .iter()
            .filter_map(|(&dialog_ptr, snapshot)| {
                let vis_handle = bus.read_long(dialog_ptr + 24);
                if vis_handle != 0 && Self::region_handle_rect(bus, vis_handle).is_none() {
                    return None;
                }
                // A dialog the application painted itself owns its pixels;
                // replaying the snapshot the HLE captured before the app drew
                // would repaint the stale contents.
                // Inside Macintosh Volume I, I-415.
                if self.dialogs_drawn_by_app.contains(&dialog_ptr) {
                    return None;
                }
                if front_is_dialog && dialog_ptr != front_window {
                    None
                } else {
                    Some((dialog_ptr, snapshot.clone()))
                }
            })
            .collect();
        for (dialog_ptr, snapshot) in snapshots {
            let bounds = snapshot.bounds;
            let snapshot_rect = Self::dialog_saved_pixel_rect(bounds);
            let preserved_front_pixels: Vec<_> = self
                .window_list
                .iter()
                .position(|&window| window == dialog_ptr)
                .map(|dialog_index| {
                    self.window_list[..dialog_index]
                        .iter()
                        .filter(|&&window| self.window_visible(bus, window))
                        .filter_map(|&window| {
                            self.window_structure_rect(bus, window)
                                .and_then(|structure| {
                                    Self::rect_intersection(snapshot_rect, structure)
                                })
                                .and_then(|overlap| self.save_screen_rect_pixels(bus, overlap))
                        })
                        .collect()
                })
                .unwrap_or_default();
            self.restore_dialog_pixels(bus, bounds, &snapshot.pixels);
            for (top, left, width, height, pixels) in preserved_front_pixels {
                self.restore_screen_rect_pixels(bus, top, left, width, height, &pixels);
            }
        }
    }

    pub(crate) fn refresh_menu_bar_policy_from_guest(&mut self, bus: &MacMemoryBus) {
        if self.menu_bar_policy != crate::runner::MenuBarPolicy::InitialKiosk {
            return;
        }

        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        if menu_bar_height <= 0 && self.front_window != 0 {
            self.initial_kiosk_guest_hide_observed = true;
        } else if menu_bar_height > 0 && self.initial_kiosk_guest_hide_observed {
            self.menu_bar_policy = crate::runner::MenuBarPolicy::GuestControlled;
            self.menu_bar_hidden = false;
        }
    }

    /// Redraw the menu bar and window chrome into the framebuffer.
    ///
    /// On a real Mac, the Window Manager maintains these UI elements and redraws
    /// them after any update. Our emulator draws them as raw framebuffer pixels,
    /// so game drawing (explosions, etc.) can overwrite them. This method restores
    /// the chrome and should be called after each frame of emulation.
    pub fn redraw_chrome(&mut self, bus: &mut MacMemoryBus) {
        self.refresh_menu_bar_policy_from_guest(bus);

        // Blit front window's port pixels to screen framebuffer if they differ.
        // On real Mac OS the Window Manager composites windows to the screen.
        // In HLE, games draw to the window's GrafPort which may have a different
        // baseAddr than the screen. Copy the window content so screenshots work.
        //
        // ModalDialog's HLE first paints standard dialog content directly into
        // the screen framebuffer, then injects any userItem draw procs and
        // re-snapshots the completed result. While that snapshot is pending,
        // the dialog's offscreen port may still be blank; blitting it here
        // erases the partially rendered dialog before the draw procs can finish.
        let pending_dialog_snapshot = self
            .dialog_tracking
            .as_ref()
            .map(|tracking| !tracking.game_managed && !tracking.rendered_pixels_final)
            .unwrap_or(false);
        if !pending_dialog_snapshot {
            self.blit_window_to_screen(bus);
            self.blit_large_manual_cport_to_screen(bus);
        }

        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let (_, _, screen_w, screen_h, _) = self.screen_mode;

        // Detect fullscreen: the front window covers the entire screen
        // (top <= 0, left <= 0, bottom >= screen_h, right >= screen_w)
        // and MBarHeight is 0.  Once detected, lock fullscreen mode so
        // that the game temporarily restoring MBarHeight (e.g. on
        // cursor-at-top) cannot flash the menu bar.
        let (wt, wl, wb, wr) = self.window_bounds;
        if self.fullscreen_locked && menu_bar_height > 0 {
            self.fullscreen_locked = false;
        }

        if self.front_window != 0
            && wt <= 0
            && wl <= 0
            && wb >= screen_h as i16
            && wr >= screen_w as i16
            && menu_bar_height <= 0
        {
            self.fullscreen_locked = true;
        }

        self.restore_kiosk_dialog_desktop_background(bus);

        if !self.menus.is_empty() && !self.fullscreen_locked && !self.menu_bar_hidden {
            self.draw_menu_bar_to_fb(bus);
        }
        // Skip chrome for borderless/dialog window types that have no title bar.
        // procID 1 = dBoxProc, 2 = plainDBox, 3 = altDBoxProc
        // All other standard types (documentProc, noGrowDocProc, etc.) get
        // chrome. Application custom WDEFs draw their own frames and are
        // skipped per-window below.
        // Also skip chrome when MBarHeight is 0 — the game has hidden the menu bar
        // for full-screen mode, so window chrome should not be drawn either.
        // Inside Macintosh Volume I, I-299; Inside Macintosh Volume V, V-245

        // Games set MBarHeight to 0 by writing directly to the low-memory
        // global ($0BAA) for full-screen mode.  Since we can't intercept
        // memory writes, check here whether the front window's visRgn.top
        // is stale and needs expanding to cover the now-hidden menu bar area.
        // Inside Macintosh Volume V, V-245; Tricks of the Mac Game
        // Programming Gurus 1995, p. 30-265
        // `menu_bar_hidden` (default-on for game runtimes — see
        // `TrapDispatcher::menu_bar_hidden`) suppresses the menu bar even
        // when MBarHeight is non-zero. Treat it like fullscreen for
        // visRgn-expansion purposes so the band the menu bar would occupy
        // is owned by the front window's visRgn, not left unpainted.
        let effective_mbar = if self.fullscreen_locked || self.menu_bar_hidden {
            0
        } else {
            menu_bar_height
        };
        // Only expand visRgn when the menu bar is HIDDEN (effective_mbar == 0).
        // Games set MBarHeight=0 for fullscreen mode and expect their window's
        // visRgn to extend over the now-hidden menu bar area. Doing this
        // unconditionally would clobber wind_top for documentProc windows where
        // the menu bar is visible (degenerate title bar on every redraw).
        if self.front_window != 0 && !no_visrgn_auto_expand_enabled() && effective_mbar == 0 {
            let vis_top_expected = 0i16;
            let vis_rgn_handle = bus.read_long(self.front_window + 24);
            if vis_rgn_handle != 0 {
                let vis_rgn = bus.read_long(vis_rgn_handle);
                if vis_rgn != 0 {
                    let current_vis_top = bus.read_word(vis_rgn + 2) as i16;
                    if current_vis_top != vis_top_expected {
                        bus.write_word(vis_rgn + 2, vis_top_expected as u16);
                        // Also update clipRgn and portRect to match.
                        let clip_rgn_handle = bus.read_long(self.front_window + 28);
                        if clip_rgn_handle != 0 {
                            let clip_rgn = bus.read_long(clip_rgn_handle);
                            if clip_rgn != 0 {
                                let clip_top = bus.read_word(clip_rgn + 2) as i16;
                                if clip_top > vis_top_expected {
                                    bus.write_word(clip_rgn + 2, vis_top_expected as u16);
                                }
                            }
                        }
                        let port_top = bus.read_word(self.front_window + 16) as i16;
                        if port_top > vis_top_expected {
                            bus.write_word(self.front_window + 16, vis_top_expected as u16);
                        }
                        self.window_bounds.0 = vis_top_expected;
                    }
                }
            }
        }

        let hidden_menu_fullscreen_top = if menu_bar_height > 0 {
            menu_bar_height.saturating_add(2)
        } else {
            22
        };
        let hidden_menu_fullscreen_window = self.menu_bar_hidden
            && wt <= hidden_menu_fullscreen_top
            && wl <= 2
            && wb >= screen_h as i16 - 2
            && wr >= screen_w as i16 - 2;
        let skip_chrome =
            self.fullscreen_locked || menu_bar_height <= 0 || hidden_menu_fullscreen_window;

        // WindowList is the visual front-to-back order. BringToFront changes
        // that order without changing the active-window cache, so the cache
        // cannot be used as the compositing top layer.
        // Draw every visible frame in reverse WindowList order. The shared
        // single-window path applies ClipAbove semantics to each frame.
        // Macintosh Toolbox Essentials (1992), pp. 4-65 and 4-118--4-119.
        if !skip_chrome && menu_bar_height > 0 {
            let list_snapshot = self.window_list.to_vec();
            for &w in list_snapshot.iter().rev() {
                // Native PowerPC windows participate in the process-wide
                // WindowList, but their adapter already rendered their WDEF
                // chrome into the native front buffer. Only repaint windows
                // whose classic adapter owns the per-window WDEF metadata.
                if self.process_window_list_attached && !self.window_proc_ids.contains_key(&w) {
                    continue;
                }
                if bus.read_byte(w + 110u32) == 0 {
                    continue;
                }
                let hilited = bus.read_byte(w + 111u32) != 0;
                self.draw_single_window_chrome_inline(bus, w, hilited);
            }
        }
        // The kiosk stage is background chrome. Paint it after permanent
        // window frames but before any transient dialog/menu overlays so a
        // dropdown extending into the letterbox margins remains frontmost.
        self.fill_kiosk_stage_for_centered_game_surface(bus, self.front_window);
        // If a modal dialog is active, restore the rendered snapshot and
        // redraw only dynamic elements (edit text, button flash) on top.
        // Game-managed dialogs (all userItems) handle their own rendering
        // via the filter proc — skip restoration to avoid overwriting their content.
        // While userItem draw procs or filter procs are pending
        // (rendered_pixels_final=false), skip restoration so their output
        // accumulates in the framebuffer. ModalDialog re-snapshots the final
        // state and sets rendered_pixels_final=true before we begin restoring.
        if let Some(ref tracking) = self.dialog_tracking {
            if !tracking.game_managed && tracking.rendered_pixels_final {
                // Blit the pre-rendered dialog snapshot (includes pictures)
                self.restore_dialog_pixels(bus, tracking.bounds, &tracking.rendered_pixels);

                // Re-draw the edit text field on top (may have changed since snapshot)
                if tracking.edit_item > 0 {
                    let idx = (tracking.edit_item - 1) as usize;
                    if idx < tracking.items.len() {
                        let item = &tracking.items[idx];
                        let abs_top = tracking.bounds.0 + item.rect.0;
                        let abs_left = tracking.bounds.1 + item.rect.1;
                        let abs_bottom = tracking.bounds.0 + item.rect.2;
                        let abs_right = tracking.bounds.1 + item.rect.3;
                        self.draw_edit_text(
                            bus,
                            abs_top,
                            abs_left,
                            abs_bottom,
                            abs_right,
                            &tracking.edit_text,
                            !tracking.edit_text_modified,
                        );
                    }
                }

                // During flash, alternate highlight on the flashing button
                if tracking.flash_remaining > 0 && tracking.flash_item > 0 {
                    let fi = tracking.flash_item;
                    if (fi as usize) <= tracking.items.len() {
                        let item = &tracking.items[(fi - 1) as usize];
                        let (it, il, ib, ir) = item.rect;
                        let abs_top = tracking.bounds.0 + it;
                        let abs_left = tracking.bounds.1 + il;
                        let abs_bottom = tracking.bounds.0 + ib;
                        let abs_right = tracking.bounds.1 + ir;
                        if tracking.flash_remaining % 2 == 0 {
                            self.invert_button_rect(bus, abs_top, abs_left, abs_bottom, abs_right);
                        }
                    }
                }

                if let Some(ref popup) = tracking.active_popup {
                    self.draw_menu_dropdown(bus, popup.active_menu, popup.dropdown_rect);
                }
            }
        } else {
            self.restore_visible_dialog_snapshots(bus);
            self.redraw_retained_modal_dialog_click(bus);
        }

        // TrackControl popup menus are live overlays, just like ModalDialog's
        // popup and MenuSelect's pull-downs. Window/chrome compositing above
        // may cover them, so stamp the current tracking state back on top.
        if let Some((active_menu, dropdown_rect)) = self
            .control_tracking
            .as_ref()
            .filter(|tracking| tracking.popup_tracking)
            .map(|tracking| (tracking.active_menu, tracking.dropdown_rect))
        {
            self.draw_menu_dropdown(bus, active_menu, dropdown_rect);
        }

        // If MenuSelect has a menu hierarchy open, redraw the root and every
        // visible child in front-to-back order. During the odd phase of the
        // selection flash, suppress only the deepest selected row while the
        // pixels are drawn; the logical tracking state must remain intact.
        if let Some((_active_menu, dropdowns, hidden_depth)) =
            self.menu_tracking.as_ref().map(|tracking| {
                let dropdowns = std::iter::once((tracking.menu_handle, tracking.dropdown_rect()))
                    .chain(
                        tracking
                            .submenus
                            .iter()
                            .map(|submenu| (submenu.menu_handle, submenu.dropdown_rect())),
                    )
                    .collect::<Vec<_>>();
                let hide_classic_highlight = self.ui_theme_id() == UiThemeId::ClassicSystem7
                    && tracking.flash_remaining > 0
                    && tracking.flash_remaining % 2 != 0;
                let hidden_depth = hide_classic_highlight.then(|| {
                    tracking
                        .submenus
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, submenu)| submenu.highlighted_item > 0)
                        .map(|(depth, _)| depth + 1)
                        .unwrap_or(0)
                });
                (tracking.menu_handle, dropdowns, hidden_depth)
            })
        {
            // draw_menu_bar_to_fb already rendered the title selected by
            // TheMenu. Reapplying the classic MBDF highlight here would
            // invert that title a second time and make an open menu look
            // inactive. The retained overlay below owns only menu panes.
            let hidden_item = hidden_depth.and_then(|depth| {
                self.menu_tracking.as_mut().and_then(|tracking| {
                    if depth == 0 {
                        let item = tracking.highlighted_item;
                        tracking.highlighted_item = 0;
                        (item > 0).then_some((depth, item))
                    } else {
                        tracking.submenus.get_mut(depth - 1).and_then(|submenu| {
                            let item = submenu.highlighted_item;
                            submenu.highlighted_item = 0;
                            (item > 0).then_some((depth, item))
                        })
                    }
                })
            });
            for (menu_handle, rect) in dropdowns {
                if let Some(menu) = self.menu_index_for_handle(menu_handle) {
                    self.draw_menu_dropdown(bus, menu, rect);
                }
            }
            if let Some((depth, item)) = hidden_item {
                if let Some(tracking) = self.menu_tracking.as_mut() {
                    if depth == 0 {
                        tracking.highlighted_item = item;
                    } else if let Some(submenu) = tracking.submenus.get_mut(depth - 1) {
                        submenu.highlighted_item = item;
                    }
                }
            }
        }
        if let Some(tracking) = self.window_tracking.as_ref() {
            self.draw_window_drag_outline(bus, tracking.outline_rect);
        }
        if let Some(tracking) = self
            .go_away_tracking
            .as_ref()
            .filter(|tracking| tracking.highlighted)
        {
            self.toggle_standard_go_away_highlight(bus, tracking.highlight_rect);
        }
        if let Some((rect, pattern)) = self.region_tracking.as_ref().and_then(|tracking| {
            tracking
                .outline_rect
                .map(|rect| (rect, tracking.outline_pattern))
        }) {
            self.draw_drag_outline_pattern(bus, rect, pattern);
        }
        self.capture_gui_frame(bus, "redraw_chrome");
    }
}

#[cfg(test)]
mod redraw_chrome_tests {
    use super::super::dispatch::{ControlTrackingState, DialogItem, ScreenCopyBitsRect};
    use super::super::menu::{test_tracked_menu_state, tracked_submenu_state, Menu, MenuItem};
    use super::super::test_helpers::setup_with_port;
    use super::super::TrapDispatcher;
    use crate::memory::MemoryBus;

    // Window/port layout from `setup_with_port`:
    //   port_ptr      = 0x181000
    //   port_top      = port_ptr + 16 (word)
    //   visRgn handle = port_ptr + 24 (long) → 0x182100
    //   visRgn        = 0x182000 (rgnSize @ +0, top @ +2, ...)
    //   clipRgn handle= port_ptr + 28 (long) → 0x182300
    //   clipRgn       = 0x182200
    const PORT_PTR: u32 = 0x181000;
    const VIS_RGN: u32 = 0x182000;
    const CLIP_RGN: u32 = 0x182200;
    const WINDOW_VISIBLE_OFFSET: u32 = 110;

    #[test]
    fn menu_bar_title_baseline_tracks_live_height() {
        assert_eq!(TrapDispatcher::menu_bar_title_baseline(12), 11);
        assert_eq!(TrapDispatcher::menu_bar_title_baseline(20), 14);
        assert_eq!(TrapDispatcher::menu_bar_title_baseline(30), 19);
    }

    fn set_window_structure_rect(
        bus: &mut crate::memory::MacMemoryBus,
        window_ptr: u32,
        rect: (i16, i16, i16, i16),
    ) {
        let region = bus.alloc(10);
        bus.write_word(region, 10);
        bus.write_word(region + 2, rect.0 as u16);
        bus.write_word(region + 4, rect.1 as u16);
        bus.write_word(region + 6, rect.2 as u16);
        bus.write_word(region + 8, rect.3 as u16);
        let handle = bus.alloc(4);
        bus.write_long(handle, region);
        bus.write_long(window_ptr + 114, handle);
    }

    fn screen_pixel_is_black(
        disp: &TrapDispatcher,
        bus: &crate::memory::MacMemoryBus,
        x: i16,
        y: i16,
    ) -> bool {
        let (screen_base, row_bytes, width, height, depth) = disp.screen_mode;
        TrapDispatcher::fb_pixel_is_logical_black(
            bus,
            screen_base,
            row_bytes,
            depth,
            width as i16,
            height as i16,
            x,
            y,
        )
    }

    fn overlay_test_menu(
        id: i16,
        title: &str,
        item: &str,
        visible_in_menu_bar: bool,
        hierarchical_item: bool,
    ) -> Menu {
        Menu {
            id,
            title: title.to_owned(),
            items: vec![MenuItem {
                text: item.to_owned(),
                icon: 0,
                key_equiv: if hierarchical_item { 0x1B } else { 0 },
                mark: 0,
                style: 0,
                enabled: true,
            }],
            enabled: true,
            handle: id as u32,
            in_menu_bar: true,
            hierarchical: !visible_in_menu_bar,
            visible_in_menu_bar,
        }
    }

    #[test]
    fn redraw_chrome_repaints_hierarchical_menus_and_flashes_only_the_leaf() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, _, screen_height, _) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * u32::from(screen_height), 0);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menu_bar_hidden = false;
        disp.menus = vec![
            overlay_test_menu(700, "Root", "More", true, true),
            overlay_test_menu(701, "Child", "Leaf", false, false),
        ];
        let root_rect = (20, 10, 38, 100);
        let child_rect = (24, 140, 42, 230);
        let mut child = tracked_submenu_state(701, 1, child_rect, Vec::new());
        child.highlighted_item = 1;
        let mut tracking = test_tracked_menu_state(700, root_rect, 1);
        tracking.submenus.push(child);
        tracking.flash_remaining = 5;
        tracking.flash_result = (701u32 << 16) | 1;
        *disp.menu_tracking = Some(tracking);

        disp.redraw_chrome(&mut bus);

        assert!(
            screen_pixel_is_black(&disp, &bus, child_rect.1, child_rect.0),
            "frame compositing should repaint every visible submenu"
        );
        assert!(
            screen_pixel_is_black(&disp, &bus, 70, root_rect.0 + 8),
            "an odd leaf-flash phase must keep its highlighted parent row visible"
        );
        assert!(
            !screen_pixel_is_black(&disp, &bus, 210, child_rect.0 + 8),
            "the odd flash phase should draw the selected leaf without highlight"
        );
        let tracking = disp.menu_tracking.as_ref().unwrap();
        assert_eq!(tracking.highlighted_item, 1);
        assert_eq!(tracking.submenus[0].highlighted_item, 1);

        disp.menu_tracking.as_mut().unwrap().flash_remaining = 6;
        disp.redraw_chrome(&mut bus);
        assert!(
            screen_pixel_is_black(&disp, &bus, 210, child_rect.0 + 8),
            "the even flash phase should redraw the deepest selected row"
        );
    }

    #[test]
    fn redraw_chrome_repaints_live_trackcontrol_popup_menu() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, _, screen_height, _) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * u32::from(screen_height), 0);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menu_bar_hidden = true;
        disp.menus = vec![overlay_test_menu(702, "Popup", "Choice", false, false)];
        let dropdown_rect = (80, 250, 98, 340);
        disp.control_tracking = Some(ControlTrackingState {
            ctrl_handle: 0x1234,
            ctrl_ptr: 0x5678,
            popup_tracking: true,
            active_menu: 0,
            highlighted_item: 1,
            saved_pixels: Vec::new(),
            dropdown_rect,
            popup_content_top: dropdown_rect.0,
            popup_scroll_direction: None,
            simple_part: 0,
            simple_screen_rect: (0, 0, 0, 0),
            simple_highlighted: false,
            saved_hilite: 0,
            stack_ptr: 0,
            scrollbar_action_proc: 0,
            scrollbar_part: 0,
            scrollbar_last_action_tick: 0,
            scrollbar_idle_refires: 0,
            scrollbar_callback_pending: false,
        });

        disp.redraw_chrome(&mut bus);

        assert!(
            screen_pixel_is_black(&disp, &bus, dropdown_rect.1, dropdown_rect.0),
            "frame compositing should repaint a live popup's border"
        );
        assert!(
            screen_pixel_is_black(&disp, &bus, 320, dropdown_rect.0 + 8),
            "frame compositing should retain the popup's selected row"
        );
        assert_eq!(
            disp.control_tracking.as_ref().unwrap().highlighted_item,
            1,
            "redrawing a popup must not mutate its logical selection"
        );
    }

    #[test]
    fn redraw_chrome_places_live_popup_over_the_kiosk_stage() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, screen_width, screen_height, _) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * u32::from(screen_height), 0);

        // A 640x480 plainDBox front window centered on the 800x600 screen
        // activates the black kiosk-stage margins.
        bus.write_word(PORT_PTR + 8, (-60i16) as u16);
        bus.write_word(PORT_PTR + 10, (-80i16) as u16);
        bus.write_word(PORT_PTR + 12, 540);
        bus.write_word(PORT_PTR + 14, 720);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 480);
        bus.write_word(PORT_PTR + 22, 640);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (60, 80, 540, 720);
        disp.window_proc_id = 2;
        disp.window_proc_ids.insert(PORT_PTR, 2);
        disp.menu_bar_hidden = true;
        *disp.device_clut = [[0xFFFF, 0xFFFF, 0xFFFF]; 256];
        disp.device_clut[37] = [0, 0, 0];

        disp.menus = vec![overlay_test_menu(703, "Popup", "Choice", false, false)];
        let dropdown_rect = (100, 10, 118, 70);
        disp.control_tracking = Some(ControlTrackingState {
            ctrl_handle: 0x1234,
            ctrl_ptr: 0x5678,
            popup_tracking: true,
            active_menu: 0,
            highlighted_item: 0,
            saved_pixels: Vec::new(),
            dropdown_rect,
            popup_content_top: dropdown_rect.0,
            popup_scroll_direction: None,
            simple_part: 0,
            simple_screen_rect: (0, 0, 0, 0),
            simple_highlighted: false,
            saved_hilite: 0,
            stack_ptr: 0,
            scrollbar_action_proc: 0,
            scrollbar_part: 0,
            scrollbar_last_action_tick: 0,
            scrollbar_idle_refires: 0,
            scrollbar_callback_pending: false,
        });

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 108 * row_bytes + 5),
            37,
            "precondition: the kiosk pass should fill the exposed left margin"
        );
        assert_eq!(
            bus.read_byte(screen_base + 108 * row_bytes + 60),
            0,
            "the live popup's white interior should remain above the black stage"
        );
        assert_eq!(screen_width, 800, "test fixture assumes an 800px screen");
    }

    #[test]
    fn active_movable_dialog_draws_stripes_without_window_controls() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let white = TrapDispatcher::logical_white_pixel_index(&bus);
        bus.fill_bytes(screen_base, 800 * 600, white);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (103, 152, 497, 647);
        disp.window_proc_id = 5;
        disp.window_title.clear();
        disp.go_away_flag = true;

        disp.draw_window_chrome(&mut bus, true);

        assert!(screen_pixel_is_black(&disp, &bus, 200, 85));
        assert!(!screen_pixel_is_black(&disp, &bus, 200, 86));
        assert!(screen_pixel_is_black(&disp, &bus, 200, 87));
        assert!(
            !screen_pixel_is_black(&disp, &bus, 160, 88),
            "movable dialog must omit the close box even when goAwayFlag is set"
        );
        assert!(
            !screen_pixel_is_black(&disp, &bus, 628, 88),
            "movable dialog must omit the zoom box"
        );
    }

    #[test]
    fn inactive_movable_dialog_keeps_a_plain_framed_title_bar() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        let white = TrapDispatcher::logical_white_pixel_index(&bus);
        bus.fill_bytes(screen_base, 800 * 600, white);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (103, 152, 497, 647);
        disp.window_proc_id = 5;
        disp.window_title.clear();
        disp.go_away_flag = true;

        disp.draw_window_chrome(&mut bus, false);

        assert!(screen_pixel_is_black(&disp, &bus, 200, 84));
        assert!(!screen_pixel_is_black(&disp, &bus, 200, 85));
        assert!(!screen_pixel_is_black(&disp, &bus, 160, 88));
        assert!(!screen_pixel_is_black(&disp, &bus, 628, 88));
    }

    #[test]
    fn systemless_desktop_keeps_the_classic_checker_geometry_with_logo_colors() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        disp.set_ui_theme_id(crate::ui_theme::UiThemeId::SystemlessDefault);

        disp.fill_theme_desktop_rect(&mut bus, 24, 24, 26, 26);

        let (base, row_bytes, width, height, depth) = disp.screen_mode;
        let dark = TrapDispatcher::fb_get_pixel_index(
            &bus, base, row_bytes, depth, width as i16, height as i16, 24, 24,
        )
        .unwrap();
        let light = TrapDispatcher::fb_get_pixel_index(
            &bus, base, row_bytes, depth, width as i16, height as i16, 25, 24,
        )
        .unwrap();

        assert_ne!(dark, light, "the desktop must retain the alternating Mac pattern");
        assert_ne!(dark, TrapDispatcher::logical_black_pixel_index(&bus));
        assert_ne!(light, TrapDispatcher::logical_white_pixel_index(&bus));
    }

    #[test]
    fn indexed_framebuffer_helpers_preserve_adjacent_four_bit_pixels() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(2);
        bus.write_byte(screen_base, 0xAB);
        bus.write_byte(screen_base + 1, 0xCD);

        TrapDispatcher::fb_set_pixel_index(&mut bus, screen_base, 2, 4, 4, 1, 0, 0, 3);
        TrapDispatcher::fb_set_pixel_index(&mut bus, screen_base, 2, 4, 4, 1, 1, 0, 4);
        assert_eq!(bus.read_byte(screen_base), 0x34);
        assert_eq!(bus.read_byte(screen_base + 1), 0xCD);

        TrapDispatcher::fb_fill_rect_index(&mut bus, screen_base, 2, 4, 4, 1, 0, 1, 1, 3, 5);
        assert_eq!(bus.read_byte(screen_base), 0x35);
        assert_eq!(bus.read_byte(screen_base + 1), 0x5D);

        disp.screen_mode = (screen_base, 2, 4, 1, 4);
        let saved = disp
            .save_screen_rect_pixels(&bus, (0, 1, 1, 3))
            .expect("packed screen rectangle");
        assert_eq!(saved.4, vec![5, 5]);
        bus.write_byte(screen_base, 0xAA);
        bus.write_byte(screen_base + 1, 0xAA);
        disp.restore_screen_rect_pixels(&mut bus, saved.0, saved.1, saved.2, saved.3, &saved.4);
        assert_eq!(bus.read_byte(screen_base), 0xA5);
        assert_eq!(bus.read_byte(screen_base + 1), 0x5A);
    }

    #[test]
    fn indexed_framebuffer_helpers_preserve_adjacent_two_bit_pixels() {
        let (_disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(1);
        bus.write_byte(screen_base, 0b11_10_01_00);

        TrapDispatcher::fb_set_pixel_index(&mut bus, screen_base, 1, 2, 4, 1, 1, 0, 1);
        assert_eq!(bus.read_byte(screen_base), 0b11_01_01_00);

        TrapDispatcher::fb_fill_rect_index(&mut bus, screen_base, 1, 2, 4, 1, 0, 2, 1, 4, 2);
        assert_eq!(bus.read_byte(screen_base), 0b11_01_10_10);
        assert_eq!(
            (0..4)
                .map(|x| TrapDispatcher::fb_get_pixel_index(&bus, screen_base, 1, 2, 4, 1, x, 0,))
                .collect::<Vec<_>>(),
            vec![Some(3), Some(1), Some(2), Some(2)]
        );
    }

    #[test]
    fn two_bit_indexed_glyph_draw_writes_fields_not_whole_bytes() {
        let (_disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(2);
        bus.write_byte(screen_base, 0b01_01_01_01);
        let glyph = super::Glyph {
            width: 2,
            height: 1,
            advance: 2,
            origin_x: 0,
            origin_y: 0,
            data_offset: 0,
        };

        TrapDispatcher::fb_draw_glyph_bitmap_with_slant(
            &mut bus,
            screen_base,
            2,
            2,
            8,
            1,
            1,
            0,
            &glyph,
            &[0xff, 0xff],
            None,
            super::QuickDrawTextStyle::plain(),
            Some(3),
            true,
        );

        assert_eq!(bus.read_byte(screen_base), 0b01_11_11_01);
        assert_eq!(bus.read_byte(screen_base + 1), 0x00);
    }

    #[test]
    fn main_screen_color_lookup_never_falls_back_to_thegdevice() {
        let (_disp, _cpu, mut bus) = setup_with_port();
        let mut foreign = TrapDispatcher::new();
        let foreign_base = bus.alloc(16 * 16);
        foreign.set_screen_mode_for_test(foreign_base, 16, 16, 16, 8);
        let foreign_device = foreign.ensure_main_gdevice(&mut bus);
        bus.write_long(0x0CC8, foreign_device); // TheGDevice
        bus.write_long(0x08A4, 0); // MainDevice

        assert!(
            TrapDispatcher::fb_pixel_index_for_rgb(&bus, [0, 0xFFFF, 0]).is_some(),
            "precondition: the foreign active device should have a valid table"
        );
        assert_eq!(
            TrapDispatcher::fb_main_screen_pixel_index_for_rgb(&bus, [0, 0xFFFF, 0]),
            None,
            "screen chrome must not resolve through an offscreen TheGDevice when MainDevice is NIL"
        );
    }

    #[test]
    fn redraw_chrome_finishes_centered_plain_game_window_on_black_stage() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, screen_w, screen_h, pixel_size) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0);
        TrapDispatcher::fb_fill_rect_index(
            &mut bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_w as i16,
            screen_h as i16,
            60,
            80,
            540,
            720,
            42,
        );

        // A 640x480 plainDBox front window centered on the 800x600 screen.
        // Its BitMap shares the screen, so redraw_chrome's compositor leaves
        // these already-composed pixels in place before applying the stage.
        bus.write_word(PORT_PTR + 8, (-60i16) as u16);
        bus.write_word(PORT_PTR + 10, (-80i16) as u16);
        bus.write_word(PORT_PTR + 12, 540);
        bus.write_word(PORT_PTR + 14, 720);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 480);
        bus.write_word(PORT_PTR + 22, 640);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (60, 80, 540, 720);
        disp.window_proc_id = 2;
        disp.window_proc_ids.insert(PORT_PTR, 2);
        disp.menu_bar_hidden = true;
        *disp.device_clut = [[0xFFFF, 0xFFFF, 0xFFFF]; 256];
        disp.device_clut[37] = [0, 0, 0];

        disp.redraw_chrome(&mut bus);

        assert_eq!(bus.read_byte(screen_base), 37);
        assert_eq!(bus.read_byte(screen_base + 300 * row_bytes + 40), 37);
        assert_eq!(bus.read_byte(screen_base + 300 * row_bytes + 400), 42);
        assert_eq!(bus.read_byte(screen_base + 599 * row_bytes + 799), 37);
    }

    #[test]
    fn kiosk_stage_uses_last_centered_copybits_beneath_small_game_window() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, screen_w, screen_h, pixel_size) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0);
        TrapDispatcher::fb_fill_rect_index(
            &mut bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_w as i16,
            screen_h as i16,
            60,
            80,
            540,
            720,
            255,
        );
        TrapDispatcher::fb_fill_rect_index(
            &mut bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_w as i16,
            screen_h as i16,
            193,
            240,
            406,
            560,
            42,
        );

        // A small transient game window is frontmost over the last centered
        // screen CopyBits surface that establishes the kiosk aperture.
        bus.write_word(PORT_PTR + 8, (-193i16) as u16);
        bus.write_word(PORT_PTR + 10, (-240i16) as u16);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 213);
        bus.write_word(PORT_PTR + 22, 320);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        set_window_structure_rect(&mut bus, PORT_PTR, (185, 232, 414, 568));
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_proc_ids.insert(PORT_PTR, 1);
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 480,
            src_right: 640,
            dst_top: 60,
            dst_left: 80,
            dst_bottom: 540,
            dst_right: 720,
        });
        disp.menu_bar_hidden = true;
        *disp.device_clut = [[0xFFFF, 0xFFFF, 0xFFFF]; 256];
        disp.device_clut[255] = [0, 0, 0];

        disp.fill_kiosk_stage_for_centered_game_surface(&mut bus, PORT_PTR);

        assert_eq!(bus.read_byte(screen_base), 255);
        assert_eq!(bus.read_byte(screen_base + 300 * row_bytes + 100), 255);
        assert_eq!(bus.read_byte(screen_base + 300 * row_bytes + 400), 42);
        assert_eq!(bus.read_byte(screen_base + 599 * row_bytes + 799), 255);

        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0);
        TrapDispatcher::fb_fill_rect_index(
            &mut bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_w as i16,
            screen_h as i16,
            60,
            80,
            540,
            720,
            255,
        );
        bus.write_byte(screen_base + 1, 7);
        let before = bus.read_bytes(screen_base, (row_bytes * screen_h as u32) as usize);

        disp.fill_kiosk_stage_for_centered_game_surface(&mut bus, PORT_PTR);

        assert_eq!(
            bus.read_bytes(screen_base, before.len()),
            before,
            "nonuniform application-painted margins must remain unchanged"
        );
    }

    #[test]
    fn kiosk_stage_preserves_transient_frames_crossing_the_saved_aperture() {
        for (proc_id, content, structure) in [
            (1i16, (60i16, 80i16, 213i16, 320i16), (52, 72, 221, 328)),
            (3i16, (400i16, 600i16, 540i16, 720i16), (399, 599, 543, 723)),
        ] {
            let (mut disp, _cpu, mut bus) = setup_with_port();
            let (screen_base, row_bytes, screen_w, screen_h, pixel_size) = disp.screen_mode;
            bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0);
            TrapDispatcher::fb_fill_rect_index(
                &mut bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_w as i16,
                screen_h as i16,
                60,
                80,
                540,
                720,
                255,
            );
            bus.write_word(PORT_PTR + 8, content.0.wrapping_neg() as u16);
            bus.write_word(PORT_PTR + 10, content.1.wrapping_neg() as u16);
            bus.write_word(PORT_PTR + 16, 0);
            bus.write_word(PORT_PTR + 18, 0);
            bus.write_word(PORT_PTR + 20, content.2.wrapping_sub(content.0) as u16);
            bus.write_word(PORT_PTR + 22, content.3.wrapping_sub(content.1) as u16);
            bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
            set_window_structure_rect(&mut bus, PORT_PTR, structure);
            disp.front_window = PORT_PTR;
            *disp.window_list = vec![PORT_PTR];
            disp.window_proc_ids.insert(PORT_PTR, proc_id);
            disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
                src_top: 0,
                src_left: 0,
                src_bottom: 480,
                src_right: 640,
                dst_top: 60,
                dst_left: 80,
                dst_bottom: 540,
                dst_right: 720,
            });
            disp.menu_bar_hidden = true;
            *disp.device_clut = [[0xFFFF, 0xFFFF, 0xFFFF]; 256];
            disp.device_clut[255] = [0, 0, 0];

            disp.fill_kiosk_stage_for_centered_game_surface(&mut bus, PORT_PTR);

            assert_eq!(
                bus.read_byte(screen_base),
                0,
                "proc {proc_id} structure crossing the aperture must suppress the stage fill"
            );
        }
    }

    #[test]
    fn kiosk_stage_skips_transient_windows_without_structure_geometry() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, screen_w, screen_h, pixel_size) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0);
        TrapDispatcher::fb_fill_rect_index(
            &mut bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_w as i16,
            screen_h as i16,
            60,
            80,
            540,
            720,
            255,
        );
        bus.write_word(PORT_PTR + 8, (-193i16) as u16);
        bus.write_word(PORT_PTR + 10, (-240i16) as u16);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 213);
        bus.write_word(PORT_PTR + 22, 320);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_proc_ids.insert(PORT_PTR, 1);
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 480,
            src_right: 640,
            dst_top: 60,
            dst_left: 80,
            dst_bottom: 540,
            dst_right: 720,
        });
        disp.menu_bar_hidden = true;
        *disp.device_clut = [[0xFFFF, 0xFFFF, 0xFFFF]; 256];
        disp.device_clut[255] = [0, 0, 0];

        disp.fill_kiosk_stage_for_centered_game_surface(&mut bus, PORT_PTR);

        assert_eq!(bus.read_byte(screen_base), 0);
    }

    #[test]
    fn redraw_chrome_does_not_stage_centered_document_window() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, _screen_w, screen_h, _) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0);
        bus.write_word(PORT_PTR + 8, (-60i16) as u16);
        bus.write_word(PORT_PTR + 10, (-80i16) as u16);
        bus.write_word(PORT_PTR + 12, 540);
        bus.write_word(PORT_PTR + 14, 720);
        bus.write_word(PORT_PTR + 20, 480);
        bus.write_word(PORT_PTR + 22, 640);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (60, 80, 540, 720);
        disp.window_proc_id = 0;
        disp.window_proc_ids.insert(PORT_PTR, 0);
        disp.menu_bar_hidden = true;
        *disp.device_clut = [[0xFFFF, 0xFFFF, 0xFFFF]; 256];
        disp.device_clut[37] = [0, 0, 0];
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 480,
            src_right: 640,
            dst_top: 60,
            dst_left: 80,
            dst_bottom: 540,
            dst_right: 720,
        });

        disp.redraw_chrome(&mut bus);

        assert_eq!(bus.read_byte(screen_base), 0);
    }

    #[test]
    fn redraw_chrome_stages_last_centered_copybits_surface_with_stale_window_record() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, screen_w, screen_h, pixel_size) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0);
        TrapDispatcher::fb_fill_rect_index(
            &mut bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_w as i16,
            screen_h as i16,
            100,
            80,
            500,
            720,
            42,
        );

        bus.write_word(PORT_PTR + 8, 0);
        bus.write_word(PORT_PTR + 10, 0);
        bus.write_word(PORT_PTR + 12, screen_h);
        bus.write_word(PORT_PTR + 14, screen_w);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, screen_h);
        bus.write_word(PORT_PTR + 22, screen_w);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (0, 0, screen_h as i16, screen_w as i16);
        disp.window_proc_id = 2;
        disp.window_proc_ids.remove(&PORT_PTR);
        disp.menu_bar_hidden = true;
        *disp.device_clut = [[0xFFFF, 0xFFFF, 0xFFFF]; 256];
        disp.device_clut[37] = [0, 0, 0];
        disp.last_screen_copybits_rect = Some(ScreenCopyBitsRect {
            src_top: 0,
            src_left: 0,
            src_bottom: 200,
            src_right: 320,
            dst_top: 100,
            dst_left: 80,
            dst_bottom: 500,
            dst_right: 720,
        });

        assert!(disp.kiosk_stage_margins_are_uniform(&bus, (100, 80, 500, 720)));

        disp.redraw_chrome(&mut bus);

        assert_eq!(bus.read_byte(screen_base), 37);
        assert_eq!(bus.read_byte(screen_base + 300 * row_bytes + 40), 37);
        assert_eq!(bus.read_byte(screen_base + 300 * row_bytes + 400), 42);
        assert_eq!(bus.read_byte(screen_base + 599 * row_bytes + 799), 37);
        bus.write_byte(screen_base + 1, 38);
        assert!(!disp.kiosk_stage_margins_are_uniform(&bus, (100, 80, 500, 720)));
    }

    fn track_manual_cport(disp: &mut TrapDispatcher, bus: &crate::memory::MacMemoryBus, port: u32) {
        disp.cport_ports.insert(port);
        disp.cport_original_pixmaps
            .insert(port, bus.read_long(port + 2));
    }

    #[test]
    fn fb_fill_pattern_rect_tiles_standard_gray_pattern() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(8 * 8);
        disp.screen_mode = (screen_base, 8, 8, 8, 8);

        TrapDispatcher::fb_fill_pattern_rect(
            &mut bus,
            screen_base,
            8,
            8,
            8,
            8,
            0,
            0,
            8,
            8,
            [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
        );

        assert_eq!(bus.read_byte(screen_base), 255);
        assert_eq!(bus.read_byte(screen_base + 1), 0);
        assert_eq!(bus.read_byte(screen_base + 8), 0);
        assert_eq!(bus.read_byte(screen_base + 9), 255);
    }

    #[test]
    fn fb_fill_pattern_rect_row_tiling_matches_per_pixel_semantics_at_odd_offsets() {
        // A 24x12 8bpp screen, a rect (3,5)-(11,21) whose left edge is not
        // 8-aligned and whose top is not a multiple of 8, and a pattern
        // whose eight rows all differ: every pixel inside the rect must be
        // black (255) where the pattern bit for (y mod 8, x mod 8) is set
        // and white (0) otherwise, and nothing outside may change.
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(24 * 12);
        disp.screen_mode = (screen_base, 24, 24, 12, 8);
        for offset in 0..(24 * 12) as u32 {
            bus.write_byte(screen_base + offset, 7);
        }
        let pattern = [0x81u8, 0x42, 0x24, 0x18, 0xF0, 0x0F, 0xAA, 0x01];
        TrapDispatcher::fb_fill_pattern_rect(
            &mut bus,
            screen_base,
            24,
            8,
            24,
            12,
            3,
            5,
            11,
            21,
            pattern,
        );
        for y in 0..12i16 {
            for x in 0..24i16 {
                let value = bus.read_byte(screen_base + (y as u32) * 24 + x as u32);
                let inside = (3..11).contains(&y) && (5..21).contains(&x);
                let expected = if !inside {
                    7
                } else if (pattern[y.rem_euclid(8) as usize] >> (7 - x.rem_euclid(8))) & 1 != 0 {
                    255
                } else {
                    0
                };
                assert_eq!(value, expected, "pixel ({y}, {x})");
            }
        }
    }

    fn install_twilight_style_black_index(
        disp: &mut TrapDispatcher,
        bus: &mut crate::memory::MacMemoryBus,
    ) {
        let gdevice_handle = disp.ensure_main_gdevice(bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let gdevice = bus.read_long(gdevice_handle);
        let pixmap_handle = bus.read_long(gdevice + 22);
        let pixmap = bus.read_long(pixmap_handle);
        let ctab_handle = bus.read_long(pixmap + 42);
        let ctab = bus.read_long(ctab_handle);

        let black_entry = ctab + 8 + 8;
        bus.write_word(black_entry, 1);
        bus.write_word(black_entry + 2, 0);
        bus.write_word(black_entry + 4, 0);
        bus.write_word(black_entry + 6, 0);

        let repurposed_entry = ctab + 8 + 255 * 8;
        bus.write_word(repurposed_entry, 255);
        bus.write_word(repurposed_entry + 2, 0xFFFF);
        bus.write_word(repurposed_entry + 4, 0xFFFF);
        bus.write_word(repurposed_entry + 6, 0xCCCC);
    }

    fn make_ctab_handle(
        bus: &mut crate::memory::MacMemoryBus,
        clut: &[[u16; 3]; 256],
        seed: u32,
    ) -> u32 {
        let ctab = bus.alloc(8 + 256 * 8);
        bus.write_long(ctab, seed);
        bus.write_word(ctab + 4, 0x8000);
        bus.write_word(ctab + 6, 255);
        for index in 0u32..256 {
            let entry = ctab + 8 + index * 8;
            let [r, g, b] = clut[index as usize];
            bus.write_word(entry, index as u16);
            bus.write_word(entry + 2, r);
            bus.write_word(entry + 4, g);
            bus.write_word(entry + 6, b);
        }
        let handle = bus.alloc(4);
        bus.write_long(handle, ctab);
        handle
    }

    fn install_8bpp_cgrafport(
        bus: &mut crate::memory::MacMemoryBus,
        base: u32,
        row_bytes: u16,
        width: u16,
        height: u16,
        ctab_handle: u32,
    ) -> u32 {
        let pixmap = bus.alloc(50);
        bus.write_long(pixmap, base);
        bus.write_word(pixmap + 4, row_bytes | 0x8000);
        bus.write_word(pixmap + 6, 0);
        bus.write_word(pixmap + 8, 0);
        bus.write_word(pixmap + 10, height);
        bus.write_word(pixmap + 12, width);
        bus.write_word(pixmap + 14, 0);
        bus.write_word(pixmap + 16, 0);
        bus.write_long(pixmap + 18, 0);
        bus.write_long(pixmap + 22, 0x0048_0000);
        bus.write_long(pixmap + 26, 0x0048_0000);
        bus.write_word(pixmap + 30, 0);
        bus.write_word(pixmap + 32, 8);
        bus.write_word(pixmap + 34, 1);
        bus.write_word(pixmap + 36, 8);
        bus.write_long(pixmap + 38, 0);
        bus.write_long(pixmap + 42, ctab_handle);
        bus.write_long(pixmap + 46, 0);

        let pixmap_handle = bus.alloc(4);
        bus.write_long(pixmap_handle, pixmap);
        bus.write_long(PORT_PTR + 2, pixmap_handle);
        bus.write_word(PORT_PTR + 6, 0xC000);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, height);
        bus.write_word(PORT_PTR + 22, width);
        pixmap_handle
    }

    fn install_8bpp_cgrafport_at(
        bus: &mut crate::memory::MacMemoryBus,
        port: u32,
        base: u32,
        row_bytes: u16,
        width: u16,
        height: u16,
        ctab_handle: u32,
    ) -> u32 {
        let pixmap = bus.alloc(50);
        bus.write_long(pixmap, base);
        bus.write_word(pixmap + 4, row_bytes | 0x8000);
        bus.write_word(pixmap + 6, 0);
        bus.write_word(pixmap + 8, 0);
        bus.write_word(pixmap + 10, height);
        bus.write_word(pixmap + 12, width);
        bus.write_long(pixmap + 22, 0x0048_0000);
        bus.write_long(pixmap + 26, 0x0048_0000);
        bus.write_word(pixmap + 32, 8);
        bus.write_word(pixmap + 34, 1);
        bus.write_word(pixmap + 36, 8);
        bus.write_long(pixmap + 42, ctab_handle);

        let pixmap_handle = bus.alloc(4);
        bus.write_long(pixmap_handle, pixmap);
        bus.write_long(port + 2, pixmap_handle);
        bus.write_word(port + 6, 0xC000);
        bus.write_word(port + 16, 0);
        bus.write_word(port + 18, 0);
        bus.write_word(port + 20, height);
        bus.write_word(port + 22, width);
        pixmap_handle
    }

    /// When the menu bar is visible (MBarHeight > 0), the per-frame visRgn
    /// auto-expansion in `redraw_chrome` MUST NOT fire for a documentProc
    /// window whose visRgn.top is below the menu bar.
    #[test]
    fn redraw_chrome_preserves_window_bounds_when_menu_bar_visible() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        // MissileCommand-shaped layout: WIND bounds (40, 2, 339, 508),
        // documentProc, menu bar visible at y=0..19.
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        bus.write_word(VIS_RGN + 2, 40); // visRgn.top
        bus.write_word(CLIP_RGN + 2, 40); // clipRgn.top
        bus.write_word(PORT_PTR + 16, 40); // port_top
        disp.front_window = PORT_PTR;
        disp.window_bounds = (40, 2, 339, 508);
        disp.window_proc_id = 0; // documentProc
        disp.fullscreen_locked = false;
        // Specifically pin "host menu bar visible" — the constructor
        // hides it by default for game runtimes.
        disp.menu_bar_hidden = false;

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            disp.window_bounds.0, 40,
            "window_bounds.0 must not be clobbered when MBarHeight>0"
        );
        assert_eq!(
            bus.read_word(VIS_RGN + 2) as i16,
            40,
            "visRgn.top must not be rewritten when MBarHeight>0"
        );
        assert_eq!(
            bus.read_word(PORT_PTR + 16) as i16,
            40,
            "port_top must not be rewritten when MBarHeight>0"
        );
    }

    /// When the menu bar is hidden (MBarHeight == 0), the per-frame visRgn
    /// auto-expansion in `redraw_chrome` MUST fire and sweep the front
    /// window's visRgn.top down to 0 — fullscreen games rely on this so
    /// they can paint over the y=0..19 region.
    #[test]
    fn redraw_chrome_expands_visrgn_when_menu_bar_hidden() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        // EV-shaped layout: front window's visRgn.top is still 20
        // (left over from before the game wrote MBarHeight=0).
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);
        bus.write_word(VIS_RGN + 2, 20); // stale visRgn.top
        bus.write_word(CLIP_RGN + 2, 20); // stale clipRgn.top
        bus.write_word(PORT_PTR + 16, 20); // stale port_top
        disp.front_window = PORT_PTR;
        disp.window_bounds = (20, 0, 342, 512);
        disp.window_proc_id = 0; // documentProc
        disp.fullscreen_locked = false;

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_word(VIS_RGN + 2) as i16,
            0,
            "visRgn.top must be expanded to 0 when MBarHeight=0"
        );
        assert_eq!(
            disp.window_bounds.0, 0,
            "window_bounds.0 must be updated to match expanded visRgn"
        );
        assert_eq!(
            bus.read_word(PORT_PTR + 16) as i16,
            0,
            "port_top must be updated to match expanded visRgn"
        );
    }

    /// Host-side `menu_bar_hidden = true` with MBarHeight > 0 must still
    /// expand the front window's visRgn down to y=0. Otherwise the band
    /// the menu bar would have occupied is owned by nobody — the host
    /// suppresses its chrome paint, but the window also won't paint
    /// there, leaving stale or uninitialized pixels at the top of every
    /// fullscreen game capture.
    #[test]
    fn redraw_chrome_expands_visrgn_when_host_hides_menu_bar() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        // Game has NOT zeroed MBarHeight (a polite app that just
        // installed menus and never went fullscreen) — but the host
        // suppresses chrome because we're running it as a game.
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        bus.write_word(VIS_RGN + 2, 20); // visRgn.top below where chrome would be
        bus.write_word(CLIP_RGN + 2, 20);
        bus.write_word(PORT_PTR + 16, 20);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (20, 0, 342, 512);
        disp.window_proc_id = 0; // documentProc
        disp.fullscreen_locked = false;
        disp.menu_bar_hidden = true;

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_word(VIS_RGN + 2) as i16,
            0,
            "visRgn.top must be expanded to 0 when host hides the menu bar"
        );
        assert_eq!(
            disp.window_bounds.0, 0,
            "window_bounds.0 must follow visRgn.top up to 0"
        );
        assert_eq!(
            bus.read_word(PORT_PTR + 16) as i16,
            0,
            "port_top must follow visRgn.top up to 0"
        );
    }

    /// `redraw_chrome` MUST NOT paint the menu bar (the white strip at
    /// y=0..MBarHeight) when `menu_bar_hidden = true`, even if the
    /// guest has installed menus and left MBarHeight > 0. This pins
    /// the kiosk-mode contract: the host suppresses chrome regardless
    /// of game state, so fullscreen captures match the BasiliskII
    /// reference that has no menu bar to draw.
    #[test]
    fn redraw_chrome_does_not_paint_menu_bar_when_menu_bar_hidden() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        // Allocate a real screen buffer so blit_window_to_screen has
        // somewhere to land (without a real screen, the test would
        // pass trivially).
        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        // Pre-fill the would-be menu bar band with a sentinel byte so
        // any chrome paint trampling it is detectable.
        for i in 0u32..(800 * 20) {
            bus.write_byte(screen_base + i, 0xAA);
        }

        // Game has installed menus and left MBarHeight = 20, but the
        // host runtime is configured for kiosk mode.
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menus.push(super::super::menu::Menu {
            id: 1,
            title: String::from("Apple"),
            items: Vec::new(),
            enabled: true,
            handle: 0,
            in_menu_bar: true,
            hierarchical: false,
            visible_in_menu_bar: true,
        });
        // Skip blit_window_to_screen by leaving front_window=0 — the
        // test specifically targets draw_menu_bar_to_fb, not the window
        // composite path.
        disp.front_window = 0;
        disp.fullscreen_locked = false;
        disp.menu_bar_hidden = true;

        disp.redraw_chrome(&mut bus);

        // Sample a few bytes inside the menu-bar band. None should
        // have been overwritten with white (0xFF) — they should still
        // be the 0xAA sentinel we pre-filled.
        for &x in &[0u32, 100, 400, 799] {
            for &y in &[0u32, 5, 10, 19] {
                let off = y * 800 + x;
                assert_eq!(
                    bus.read_byte(screen_base + off),
                    0xAA,
                    "menu bar pixel at (x={}, y={}) should not be repainted \
                     when menu_bar_hidden=true",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn redraw_chrome_repaints_document_window_when_host_hides_menu_bar() {
        let (mut disp, mut cpu, mut bus) = super::super::test_helpers::setup();
        let screen_base = bus.alloc(800 * 600);
        for offset in 0..800 * 600 {
            bus.write_byte(screen_base + offset, 0xAA);
        }
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(crate::memory::globals::addr::SCREEN_BITS, screen_base);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menu_bar_hidden = true;

        let window_addr = bus.alloc(256);
        disp.init_cgraf_window(
            &mut bus,
            &mut cpu,
            window_addr,
            screen_base,
            60,
            30,
            220,
            370,
            "Zoom",
            8,
            true,
            true,
            true,
            0,
        );

        for y in 0..20u32 {
            for x in 0..800u32 {
                bus.write_byte(screen_base + y * 800 + x, 0xAA);
            }
        }
        for y in 41..59u32 {
            for x in 29..371u32 {
                bus.write_byte(screen_base + y * 800 + x, 0xAA);
            }
        }

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 10 * 800 + 100),
            0xAA,
            "hidden host menu bar should not be repainted"
        );
        assert_ne!(
            bus.read_byte(screen_base + 45 * 800 + 50),
            0xAA,
            "non-fullscreen zoom document windows should redraw titlebar chrome"
        );
    }

    /// Counterpart to the kiosk-mode test above: when `menu_bar_hidden
    /// = false` (guest-controlled hosting), `redraw_chrome` MUST paint
    /// the menu bar so menus are reachable.
    #[test]
    fn redraw_chrome_paints_menu_bar_when_not_hidden() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        // Pre-fill with sentinel so we can detect any paint.
        for i in 0u32..(800 * 20) {
            bus.write_byte(screen_base + i, 0xAA);
        }

        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menus.push(super::super::menu::Menu {
            id: 1,
            title: String::from("Apple"),
            items: Vec::new(),
            enabled: true,
            handle: 0,
            in_menu_bar: true,
            hierarchical: false,
            visible_in_menu_bar: true,
        });
        disp.front_window = 0;
        disp.fullscreen_locked = false;
        disp.menu_bar_hidden = false;

        disp.redraw_chrome(&mut bus);

        // The menu bar fills with white. In Mac 8bpp CLUT convention
        // index 0 = white, index 255 = black (Imaging With QuickDraw
        // 1994, 4-7), so painted pixels read back as 0x00, not 0xFF.
        // Either way, they cannot still be the 0xAA sentinel we
        // pre-filled — that's the load-bearing assertion.
        for &x in &[100u32, 400, 700] {
            for &y in &[5u32, 10] {
                let off = y * 800 + x;
                let pix = bus.read_byte(screen_base + off);
                assert_ne!(
                    pix, 0xAA,
                    "menu bar pixel at (x={}, y={}) should be painted \
                     when menu_bar_hidden=false (read 0x{:02X}, sentinel 0xAA)",
                    x, y, pix
                );
            }
        }
    }

    /// 800x600 8bpp screen with the standard colour table, every byte set
    /// to a sentinel index that is neither white nor black.
    fn text_fixture() -> (TrapDispatcher, crate::memory::MacMemoryBus, u32) {
        let (mut disp, _cpu, mut bus) = super::super::test_helpers::setup();
        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(crate::memory::globals::addr::SCRN_BASE, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);
        bus.fill_bytes(screen_base, 800 * 600, 0x11);
        (disp, bus, screen_base)
    }

    fn screen_bytes(bus: &crate::memory::MacMemoryBus, screen_base: u32) -> Vec<u8> {
        bus.read_bytes(screen_base, 800 * 600)
    }

    /// The per-pixel string painter this change replaced on 8-bit screens:
    /// every glyph pixel with coverage >= 128 set through
    /// `fb_set_pixel_index`, optionally restricted to `clip`.
    fn reference_draw_string(
        bus: &mut crate::memory::MacMemoryBus,
        screen_base: u32,
        x: i16,
        y: i16,
        s: &str,
        clip: Option<(i16, i16, i16, i16)>,
        index: u8,
    ) {
        let mut cx = x;
        for ch in s.chars() {
            let Some((glyph, data)) = crate::quickdraw::text::get_glyph(0, 12, ch) else {
                cx += 6;
                continue;
            };
            let gx = cx + glyph.origin_x as i16;
            let gy = y + glyph.origin_y as i16;
            let gw = glyph.width as usize;
            for row in 0..glyph.height as usize {
                for col in 0..gw {
                    let byte_idx = glyph.data_offset + row * gw + col;
                    if byte_idx >= data.len() || data[byte_idx] < 128 {
                        continue;
                    }
                    let (px, py) = (gx + col as i16, gy + row as i16);
                    if let Some((t, l, b, r)) = clip {
                        if py < t || py >= b || px < l || px >= r {
                            continue;
                        }
                    }
                    TrapDispatcher::fb_set_pixel_index(
                        bus,
                        screen_base,
                        800,
                        8,
                        800,
                        600,
                        px,
                        py,
                        index,
                    );
                }
            }
            cx += glyph.advance as i16;
        }
    }

    #[test]
    fn glyph_row_blit_matches_the_per_pixel_painter() {
        let (_disp, mut bus, screen_base) = text_fixture();
        let black = TrapDispatcher::logical_black_pixel_index(&bus);
        // Fully inside, off the left and top edges, and off the right and
        // bottom edges.
        let cases = [(100i16, 100i16), (-4, 3), (790, 596)];
        for (x, y) in cases {
            TrapDispatcher::fb_draw_string(
                &mut bus,
                screen_base,
                800,
                8,
                800,
                600,
                x,
                y,
                "Gate!",
                0,
                12,
            );
        }
        let fast = screen_bytes(&bus, screen_base);
        bus.fill_bytes(screen_base, 800 * 600, 0x11);
        for (x, y) in cases {
            reference_draw_string(&mut bus, screen_base, x, y, "Gate!", None, black);
        }
        let reference = screen_bytes(&bus, screen_base);
        assert!(
            fast == reference,
            "row-blitted text must equal the per-pixel painter"
        );
        let painted = fast.iter().filter(|&&b| b == black).count();
        assert!(
            painted > 50,
            "the strings drew something ({painted} pixels)"
        );
    }

    #[test]
    fn clipped_glyph_row_blit_matches_the_per_pixel_painter() {
        let (_disp, mut bus, screen_base) = text_fixture();
        let black = TrapDispatcher::logical_black_pixel_index(&bus);
        // A clip that cuts through the glyph boxes on every side: glyph rows
        // sit above the baseline `y`, so derive the band from the 'G' glyph.
        let (g, _) = crate::quickdraw::text::get_glyph(0, 12, 'G').expect("glyph");
        let g_top = 100 + g.origin_y as i16;
        let clip = (g_top + 2, 104i16, g_top + g.height as i16 - 2, 128i16);
        TrapDispatcher::fb_draw_string_clipped(
            &mut bus,
            screen_base,
            800,
            8,
            800,
            600,
            100,
            100,
            "Gate!",
            0,
            12,
            clip,
        );
        let fast = screen_bytes(&bus, screen_base);
        bus.fill_bytes(screen_base, 800 * 600, 0x11);
        reference_draw_string(&mut bus, screen_base, 100, 100, "Gate!", Some(clip), black);
        let reference = screen_bytes(&bus, screen_base);
        assert!(
            fast == reference,
            "clipped row-blitted text must equal the per-pixel painter"
        );
        let painted = fast.iter().filter(|&&b| b == black).count();
        assert!(
            painted > 10,
            "the clip left something visible ({painted} pixels)"
        );
        let unclipped = {
            bus.fill_bytes(screen_base, 800 * 600, 0x11);
            reference_draw_string(&mut bus, screen_base, 100, 100, "Gate!", None, black);
            screen_bytes(&bus, screen_base)
                .iter()
                .filter(|&&b| b == black)
                .count()
        };
        assert!(painted < unclipped, "the clip removed something");
    }

    #[test]
    fn plain_styled_text_matches_the_plain_painter() {
        let (_disp, mut bus, screen_base) = text_fixture();
        let black = TrapDispatcher::logical_black_pixel_index(&bus);
        TrapDispatcher::fb_draw_string_styled_index(
            &mut bus,
            screen_base,
            800,
            8,
            800,
            600,
            40,
            40,
            "File Edit",
            0,
            12,
            0,
            black,
        );
        let styled = screen_bytes(&bus, screen_base);
        bus.fill_bytes(screen_base, 800 * 600, 0x11);
        TrapDispatcher::fb_draw_string(
            &mut bus,
            screen_base,
            800,
            8,
            800,
            600,
            40,
            40,
            "File Edit",
            0,
            12,
        );
        let plain = screen_bytes(&bus, screen_base);
        assert!(
            styled == plain,
            "style 0 through the styled painter equals the plain painter"
        );
        assert!(plain.contains(&black));
    }

    #[test]
    fn fb_vline_matches_the_per_pixel_loop_and_clips() {
        let (_disp, mut bus, screen_base) = text_fixture();
        let screen = (screen_base, 800u32, 8u16, 800i16, 600i16);
        let cases = [
            (10i16, -5i16, 20i16, true),
            (799, 590, 700, true),
            (800, 0, 10, true),
            (-1, 0, 10, true),
            (50, 30, 30, true),
            (60, 40, 30, true),
            (70, 10, 40, false),
        ];
        for &(x, top, bottom, black) in &cases {
            TrapDispatcher::fb_vline(&mut bus, screen, x, top, bottom, black);
        }
        let fast = screen_bytes(&bus, screen_base);
        bus.fill_bytes(screen_base, 800 * 600, 0x11);
        for &(x, top, bottom, black) in &cases {
            for y in top..bottom {
                TrapDispatcher::fb_set_pixel(&mut bus, screen_base, 800, 8, 800, 600, x, y, black);
            }
        }
        let reference = screen_bytes(&bus, screen_base);
        assert!(fast == reference, "fb_vline must equal the per-pixel loop");
        let black = TrapDispatcher::logical_black_pixel_index(&bus);
        let white = TrapDispatcher::logical_white_pixel_index(&bus);
        assert_eq!(fast.iter().filter(|&&b| b == black).count(), 20 + 10);
        assert_eq!(fast.iter().filter(|&&b| b == white).count(), 30);
    }

    #[test]
    fn redraw_chrome_restores_desktop_pattern_behind_single_kiosk_dialog() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, screen_w, screen_h, _) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0xFF);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.menu_bar_hidden = true;
        disp.fullscreen_locked = false;
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (100, 180, 208, 620);
        disp.window_proc_id = 5;
        disp.window_proc_ids.insert(PORT_PTR, 5);
        disp.dialog_items
            .insert(PORT_PTR, vec![DialogItem::default()]);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 1);
        disp.window_saved_under_pixels
            .insert(PORT_PTR, (100, 180, 440, 108, vec![0xFF; 440 * 108]));

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base),
            0xFF,
            "standard gray pattern starts with a black pixel"
        );
        assert_eq!(
            bus.read_byte(screen_base + 1),
            0x00,
            "standard gray pattern should add white desktop pixels"
        );
        assert_eq!(
            bus.read_byte(screen_base + 120 * row_bytes + 200),
            0xFF,
            "dialog bounds must not be overwritten by the desktop fill"
        );
        let saved = &disp.window_saved_under_pixels[&PORT_PTR].4;
        assert_eq!(
            &saved[..2],
            &[0xFF, 0x00],
            "the save-under snapshot must track the synthesized desktop pattern"
        );
        assert_eq!(screen_w, 800, "test assumes the default 800-wide screen");
    }

    #[test]
    fn redraw_chrome_keeps_existing_content_behind_kiosk_dialog() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let (screen_base, row_bytes, _screen_w, screen_h, _) = disp.screen_mode;
        bus.fill_bytes(screen_base, row_bytes * screen_h as u32, 0xFF);
        bus.write_byte(screen_base, 0x00);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);

        disp.menu_bar_hidden = true;
        disp.fullscreen_locked = false;
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (100, 180, 208, 620);
        disp.window_proc_id = 5;
        disp.window_proc_ids.insert(PORT_PTR, 5);
        disp.dialog_items
            .insert(PORT_PTR, vec![DialogItem::default()]);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 1);

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 16),
            0xFF,
            "nonblack exposed content should suppress the desktop-pattern fill"
        );
    }

    /// Pin directive #3 from the task brief: "stays hidden even when
    /// the mouse hits the top of the screen". Hovering the mouse at
    /// y<MBarHeight while menu_bar_hidden=true must NOT trigger any
    /// chrome paint or menu-bar rendering. The previous tests cover
    /// the per-frame redraw_chrome guard; this test specifically
    /// re-runs redraw_chrome after a mouse_pos update to the top of
    /// the screen (the cursor-on-mbar scenario the user keeps
    /// flagging) and verifies the y=0..19 band still matches the
    /// pre-update sentinel.
    #[test]
    fn redraw_chrome_does_not_paint_on_mouse_at_top_when_hidden() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        // Pre-fill the menu bar band with sentinel.
        for i in 0u32..(800 * 20) {
            bus.write_byte(screen_base + i, 0xAA);
        }

        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.menus.push(super::super::menu::Menu {
            id: 1,
            title: String::from("Apple"),
            items: Vec::new(),
            enabled: true,
            handle: 0,
            in_menu_bar: true,
            hierarchical: false,
            visible_in_menu_bar: true,
        });
        disp.front_window = 0;
        disp.fullscreen_locked = false;
        disp.menu_bar_hidden = true;

        // Initial redraw — sentinel preserved.
        disp.redraw_chrome(&mut bus);
        for x in [0u32, 100, 400, 799] {
            for y in [0u32, 5, 10, 19] {
                assert_eq!(
                    bus.read_byte(screen_base + y * 800 + x),
                    0xAA,
                    "initial frame: sentinel must hold at (x={}, y={})",
                    x,
                    y
                );
            }
        }

        // Move mouse to the top of the screen — the very scenario
        // the task brief calls out. mouse_pos updates to (v=2, h=400)
        // which is well inside the would-be menu-bar band.
        disp.input_state.mouse_pos = (2, 400);
        bus.write_word(0x0828, 2u16); // MTemp.v
        bus.write_word(0x082A, 400u16); // MTemp.h
        bus.write_word(0x082C, 2u16); // RawMouse.v
        bus.write_word(0x082E, 400u16); // RawMouse.h
        bus.write_word(0x0830, 2u16); // Mouse.v
        bus.write_word(0x0832, 400u16); // Mouse.h

        // Re-run redraw_chrome. With menu_bar_hidden=true the band
        // must STILL be untouched even though the cursor is now
        // inside it.
        disp.redraw_chrome(&mut bus);
        for x in [0u32, 100, 400, 799] {
            for y in [0u32, 5, 10, 19] {
                assert_eq!(
                    bus.read_byte(screen_base + y * 800 + x),
                    0xAA,
                    "after mouse-at-top: sentinel must still hold at (x={}, y={})",
                    x,
                    y
                );
            }
        }
    }

    #[test]
    fn redraw_chrome_skips_window_blit_while_dialog_snapshot_pending() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        let offscreen_base = bus.alloc(64 * 200);
        bus.write_long(PORT_PTR + 2, offscreen_base);
        bus.write_word(PORT_PTR + 6, 64);
        bus.write_word(PORT_PTR + 8, 0);
        bus.write_word(PORT_PTR + 10, 0);
        bus.write_word(PORT_PTR + 12, 200);
        bus.write_word(PORT_PTR + 14, 512);

        let probe = screen_base + 10 * 800 + 10;
        bus.write_byte(probe, 0x42);
        bus.write_byte(offscreen_base + 10 * 64 + 1, 0x00);

        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 200, 512);
        disp.window_proc_id = 1;
        disp.dialog_tracking = Some(super::super::dispatch::DialogTrackingState {
            game_managed: false,
            rendered_pixels_final: false,
            ..Default::default()
        });

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x42,
            "pending ModalDialog snapshots must not be erased by blank offscreen port blits"
        );

        if let Some(tracking) = disp.dialog_tracking.as_mut() {
            tracking.rendered_pixels_final = true;
        }
        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x00,
            "once the dialog snapshot is final, normal port blitting should resume"
        );
    }

    #[test]
    fn redraw_chrome_does_not_restore_stale_dialog_pixels_while_filter_snapshot_pending() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(64 * 64);
        for i in 0..64u32 * 64 {
            bus.write_byte(screen_base + i, 0x00);
        }
        disp.screen_mode = (screen_base, 64, 64, 64, 8);
        bus.write_long(0x0824, screen_base);
        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 0);

        let bounds = (10, 10, 30, 30);
        let stale_pixels = disp.save_dialog_pixels(&bus, bounds);
        let probe = screen_base + 18 * 64 + 18;
        bus.write_byte(probe, 0x44);

        disp.dialog_tracking = Some(super::super::dispatch::DialogTrackingState {
            bounds,
            game_managed: false,
            draw_procs_done: true,
            rendered_pixels: stale_pixels,
            rendered_pixels_final: false,
            ..Default::default()
        });

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x44,
            "non-final ModalDialog snapshots must not overwrite live filter/userItem drawing"
        );

        if let Some(tracking) = disp.dialog_tracking.as_mut() {
            tracking.rendered_pixels_final = true;
        }
        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x00,
            "final ModalDialog snapshots should still be restored normally"
        );
    }

    #[test]
    fn redraw_chrome_restores_visible_dialog_snapshot_after_modaldialog_returns() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        let offscreen_base = bus.alloc(64 * 200);
        bus.write_long(PORT_PTR + 2, offscreen_base);
        bus.write_word(PORT_PTR + 6, 64);
        bus.write_word(PORT_PTR + 8, 0);
        bus.write_word(PORT_PTR + 10, 0);
        bus.write_word(PORT_PTR + 12, 200);
        bus.write_word(PORT_PTR + 14, 512);

        let probe = screen_base + 10 * 800 + 10;
        bus.write_byte(probe, 0x11);
        bus.write_byte(offscreen_base + 10 * 64 + 1, 0x00);

        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 200, 512);
        disp.window_proc_id = 1;
        for y in 0..20u32 {
            for x in 0..20u32 {
                bus.write_byte(screen_base + y * 800 + x, 0x77);
            }
        }
        let pixels = disp.save_dialog_pixels(&bus, (5, 5, 15, 15));
        for y in 0..20u32 {
            for x in 0..20u32 {
                bus.write_byte(screen_base + y * 800 + x, 0x11);
            }
        }
        let dialog_ptr = 0x00D1_A106;
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            super::super::dispatch::PersistentDialogSnapshot {
                bounds: (5, 5, 15, 15),
                pixels,
            },
        );

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x77,
            "visible dialog snapshot should remain composited even when the front port changes"
        );
    }

    #[test]
    fn restore_visible_dialog_snapshots_preserves_application_drawn_pixels() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(64 * 64);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);
        bus.write_long(0x0824, screen_base);

        let bounds = (10, 10, 30, 30);
        for y in 10..30u32 {
            for x in 10..30u32 {
                bus.write_byte(screen_base + y * 64 + x, 0x22);
            }
        }
        let stale_pixels = disp.save_dialog_pixels(&bus, bounds);

        let dialog_ptr = 0x00D1_A106;
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            super::super::dispatch::PersistentDialogSnapshot {
                bounds,
                pixels: stale_pixels,
            },
        );
        disp.dialogs_drawn_by_app.insert(dialog_ptr);

        let probe = screen_base + 18 * 64 + 18;
        bus.write_byte(probe, 0x77);
        disp.restore_visible_dialog_snapshots(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x77,
            "a retained snapshot must not overwrite pixels painted after DrawDialog"
        );
    }

    #[test]
    fn restore_visible_dialog_snapshots_preserves_windows_in_front() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(64 * 64);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);
        bus.write_long(0x0824, screen_base);

        let dialog = bus.alloc(256);
        let front = bus.alloc(256);
        let dialog_bounds = (10, 10, 40, 40);
        set_window_structure_rect(&mut bus, dialog, (2, 2, 48, 48));
        set_window_structure_rect(&mut bus, front, (18, 18, 32, 32));
        bus.write_byte(dialog + 110, 0xFF);
        bus.write_byte(front + 110, 0xFF);
        *disp.window_list = vec![front, dialog];
        disp.front_window = front;

        for y in 2..48u32 {
            for x in 2..48u32 {
                bus.write_byte(screen_base + y * 64 + x, 0x77);
            }
        }
        let dialog_pixels = disp.save_dialog_pixels(&bus, dialog_bounds);
        for y in 2..48u32 {
            for x in 2..48u32 {
                bus.write_byte(screen_base + y * 64 + x, 0x11);
            }
        }
        disp.dialog_visible_snapshots.insert(
            dialog,
            super::super::dispatch::PersistentDialogSnapshot {
                bounds: dialog_bounds,
                pixels: dialog_pixels,
            },
        );

        disp.restore_visible_dialog_snapshots(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 24 * 64 + 24),
            0x11,
            "a retained background dialog must not paint over a window in front"
        );
        assert_eq!(
            bus.read_byte(screen_base + 12 * 64 + 12),
            0x77,
            "the exposed part of a retained background dialog should still be restored"
        );
    }

    #[test]
    fn restore_visible_dialog_snapshots_skips_fully_occluded_dialogs() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(64 * 64);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);
        bus.write_long(0x0824, screen_base);

        let bounds = (10, 10, 30, 30);
        for y in 10..30u32 {
            for x in 10..30u32 {
                bus.write_byte(screen_base + y * 64 + x, 0x22);
            }
        }
        let stale_pixels = disp.save_dialog_pixels(&bus, bounds);

        let dialog_ptr = bus.alloc(170);
        let vis_data = bus.alloc(10);
        bus.write_word(vis_data, 10);
        bus.write_long(vis_data + 2, 0);
        bus.write_long(vis_data + 6, 0);
        let vis_handle = bus.alloc(4);
        bus.write_long(vis_handle, vis_data);
        bus.write_long(dialog_ptr + 24, vis_handle);
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            super::super::dispatch::PersistentDialogSnapshot {
                bounds,
                pixels: stale_pixels,
            },
        );

        let probe = screen_base + 18 * 64 + 18;
        bus.write_byte(probe, 0x77);
        disp.restore_visible_dialog_snapshots(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x77,
            "a fully occluded dialog snapshot must not overwrite its front window"
        );
    }

    #[test]
    fn restore_visible_dialog_snapshots_skips_parent_when_child_dialog_is_front() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        for y in 0..40u32 {
            for x in 0..40u32 {
                bus.write_byte(screen_base + y * 800 + x, 0x77);
            }
        }
        let parent_pixels = disp.save_dialog_pixels(&bus, (5, 5, 15, 15));
        let child_pixels = disp.save_dialog_pixels(&bus, (20, 20, 30, 30));
        for y in 0..40u32 {
            for x in 0..40u32 {
                bus.write_byte(screen_base + y * 800 + x, 0x11);
            }
        }

        let parent_dialog = 0x00D1_A106;
        let child_dialog = 0x00D1_A107;
        disp.dialog_visible_snapshots.insert(
            parent_dialog,
            super::super::dispatch::PersistentDialogSnapshot {
                bounds: (5, 5, 15, 15),
                pixels: parent_pixels,
            },
        );
        disp.dialog_visible_snapshots.insert(
            child_dialog,
            super::super::dispatch::PersistentDialogSnapshot {
                bounds: (20, 20, 30, 30),
                pixels: child_pixels,
            },
        );
        disp.front_window = child_dialog;
        disp.dialog_items.insert(child_dialog, Vec::new());

        disp.restore_visible_dialog_snapshots(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 10 * 800 + 10),
            0x11,
            "retained parent dialog snapshot must not overpaint the front child dialog"
        );
        assert_eq!(
            bus.read_byte(screen_base + 25 * 800 + 25),
            0x77,
            "front child dialog snapshot should still be restored"
        );
    }

    #[test]
    fn refresh_visible_dialog_snapshot_for_port_captures_later_screen_updates() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        for y in 0..40u32 {
            for x in 0..40u32 {
                bus.write_byte(screen_base + y * 800 + x, 0x11);
            }
        }

        let dialog_ptr = 0x00D1_A106;
        let bounds = (10, 10, 20, 20);
        let pixels = disp.save_dialog_pixels(&bus, bounds);
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            super::super::dispatch::PersistentDialogSnapshot { bounds, pixels },
        );

        let probe = screen_base + 12 * 800 + 12;
        bus.write_byte(probe, 0x77);
        disp.refresh_visible_dialog_snapshot_for_port(&bus, dialog_ptr);

        bus.write_byte(probe, 0x00);
        disp.restore_visible_dialog_snapshots(&mut bus);

        assert_eq!(
            bus.read_byte(probe),
            0x77,
            "refresh should retain QuickDraw updates made after ModalDialog returned"
        );
    }

    #[test]
    fn refresh_visible_dialog_snapshot_region_updates_only_touched_pixels() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(64 * 64);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);
        bus.write_long(0x0824, screen_base);

        for y in 0..40u32 {
            for x in 0..40u32 {
                bus.write_byte(screen_base + y * 64 + x, 0x11);
            }
        }

        let dialog_ptr = 0x00D1_A106;
        let bounds = (10, 10, 30, 30);
        let pixels = disp.save_dialog_pixels(&bus, bounds);
        disp.dialog_visible_snapshots.insert(
            dialog_ptr,
            super::super::dispatch::PersistentDialogSnapshot { bounds, pixels },
        );

        let touched_probe = screen_base + 12 * 64 + 12;
        let untouched_probe = screen_base + 20 * 64 + 20;
        bus.write_byte(touched_probe, 0x77);
        bus.write_byte(untouched_probe, 0x88);
        disp.refresh_visible_dialog_snapshot_region_for_port(&bus, dialog_ptr, (12, 12, 13, 13));

        bus.write_byte(touched_probe, 0x00);
        bus.write_byte(untouched_probe, 0x00);
        disp.restore_visible_dialog_snapshots(&mut bus);

        assert_eq!(bus.read_byte(touched_probe), 0x77);
        assert_eq!(
            bus.read_byte(untouched_probe),
            0x11,
            "pixels outside the reported QuickDraw damage must retain their prior snapshot value"
        );
    }

    #[test]
    fn refresh_visible_dialog_snapshot_region_preserves_untouched_packed_pixels() {
        for pixel_size in [1u16, 2, 4] {
            let (mut disp, _cpu, mut bus) = setup_with_port();
            let row_bytes = 64 * u32::from(pixel_size) / 8;
            let screen_base = bus.alloc(row_bytes * 64);
            disp.screen_mode = (screen_base, row_bytes, 64, 64, pixel_size);
            bus.write_long(0x0824, screen_base);

            let dialog_ptr = 0x00D1_A106;
            let bounds = (8, 8, 24, 24);
            let pixels = disp.save_dialog_pixels(&bus, bounds);
            disp.dialog_visible_snapshots.insert(
                dialog_ptr,
                super::super::dispatch::PersistentDialogSnapshot { bounds, pixels },
            );

            let touched = (12u32, 12u32);
            let untouched = (20u32, 20u32);
            let pixels_per_byte = 8 / u32::from(pixel_size);
            let pixel_mask = |x: u32| {
                let shift =
                    8 - u32::from(pixel_size) - (x % pixels_per_byte) * u32::from(pixel_size);
                (((1u16 << pixel_size) - 1) as u8) << shift
            };
            let pixel_addr = |x: u32, y: u32| screen_base + y * row_bytes + x / pixels_per_byte;

            bus.write_byte(pixel_addr(touched.0, touched.1), pixel_mask(touched.0));
            bus.write_byte(
                pixel_addr(untouched.0, untouched.1),
                pixel_mask(untouched.0),
            );
            disp.refresh_visible_dialog_snapshot_region_for_port(
                &bus,
                dialog_ptr,
                (
                    touched.1 as i16,
                    touched.0 as i16,
                    touched.1 as i16 + 1,
                    touched.0 as i16 + 1,
                ),
            );

            bus.write_byte(pixel_addr(touched.0, touched.1), 0);
            bus.write_byte(pixel_addr(untouched.0, untouched.1), 0);
            disp.restore_visible_dialog_snapshots(&mut bus);

            assert_ne!(
                bus.read_byte(pixel_addr(touched.0, touched.1)) & pixel_mask(touched.0),
                0,
                "{pixel_size}-bit touched pixel should be retained"
            );
            assert_eq!(
                bus.read_byte(pixel_addr(untouched.0, untouched.1)) & pixel_mask(untouched.0),
                0,
                "{pixel_size}-bit pixel outside the damage rect should keep its prior value"
            );
        }
    }

    #[test]
    fn fb_fill_rect_uses_active_ctab_brightest_entry_for_white() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(100 * 100);
        disp.screen_mode = (screen_base, 100, 100, 100, 8);
        bus.write_long(0x0824, screen_base);

        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);
        let gdevice = bus.read_long(gdevice_handle);
        let pixmap_handle = bus.read_long(gdevice + 22);
        let pixmap = bus.read_long(pixmap_handle);
        let ctab_handle = bus.read_long(pixmap + 42);
        let ctab = bus.read_long(ctab_handle);
        for index in 0u32..256 {
            let entry = ctab + 8 + index * 8;
            bus.write_word(entry, index as u16);
            bus.write_word(entry + 2, 0);
            bus.write_word(entry + 4, 0);
            bus.write_word(entry + 6, 0);
        }
        let white_entry = ctab + 8 + 8;
        bus.write_word(white_entry, 1);
        bus.write_word(white_entry + 2, 0xFFFF);
        bus.write_word(white_entry + 4, 0xFFFF);
        bus.write_word(white_entry + 6, 0xFFFF);

        TrapDispatcher::fb_fill_rect(
            &mut bus,
            screen_base,
            100,
            8,
            100,
            100,
            10,
            10,
            20,
            20,
            false,
        );

        assert_eq!(
            bus.read_byte(screen_base + 10 * 100 + 10),
            1,
            "logical white must follow the active ColorTable, not hard-code CLUT index 0"
        );
    }

    #[test]
    fn fb_fill_rect_uses_active_ctab_darkest_entry_for_black() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(100 * 100);
        disp.screen_mode = (screen_base, 100, 100, 100, 8);
        bus.write_long(0x0824, screen_base);
        install_twilight_style_black_index(&mut disp, &mut bus);

        TrapDispatcher::fb_fill_rect(
            &mut bus,
            screen_base,
            100,
            8,
            100,
            100,
            10,
            10,
            20,
            20,
            true,
        );

        assert_eq!(
            bus.read_byte(screen_base + 10 * 100 + 10),
            1,
            "logical black must follow the active ColorTable when index 255 is not black"
        );
    }

    /// When a front window's port is 1bpp and the screen is 8bpp,
    /// `blit_window_to_screen` MUST do per-pixel bit extraction (each src
    /// bit → 0 or 0xFF). Falling through to the same-depth `block_move`
    /// path treats pixel-width as byte-width — a stride bug that produces
    /// a tiled-garbage band on screen.
    #[test]
    fn redraw_chrome_blit_1bpp_to_8bpp_is_bit_extracted() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        // setup_with_port doesn't initialize disp.screen_mode. Allocate
        // a real 800×600 8bpp screen buffer in bus memory (the default
        // 4MB RAM doesn't reach the play-runner's $01F80000 base).
        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);

        // Allocate an offscreen 1bpp port buffer separate from the
        // screen. Basic GrafPort (port_version & 0xC000 != 0xC000) is
        // implicitly 1bpp.
        let offscreen_base = bus.alloc(64 * 200);
        bus.write_long(PORT_PTR + 2, offscreen_base); // portBits.baseAddr
        bus.write_word(PORT_PTR + 6, 64); // rowBytes (no flag bits = basic GrafPort)

        // setup_with_port wrote portBits.bounds (0,0,342,512) and
        // portRect (0,0,342,512). Override portBits.bounds to match
        // our 64-byte rowBytes × 200-row allocation: (0, 0, 200, 512).
        bus.write_word(PORT_PTR + 8, 0); // bounds.top
        bus.write_word(PORT_PTR + 10, 0); // bounds.left
        bus.write_word(PORT_PTR + 12, 200); // bounds.bottom
        bus.write_word(PORT_PTR + 14, 512); // bounds.right

        // Source pattern: row 30 byte 12 = 0b10000000 (bit 7 set) means
        // source pixel (row=30, col=96) = 1. Adjacent pixels (col=97..103)
        // are 0. After 1bpp→8bpp conversion, screen pixel (30, 96) should
        // become 0xFF and (30, 97) should become 0x00.
        bus.write_byte(offscreen_base + 30 * 64 + 12, 0b10000000);

        // Pre-fill the test pixels with a sentinel so we can detect
        // both bit values being correctly written.
        let row30_x96 = screen_base + 30 * 800 + 96;
        let row30_x97 = screen_base + 30 * 800 + 97;
        bus.write_byte(row30_x96, 0xAB);
        bus.write_byte(row30_x97, 0xAB);

        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 200, 400);
        disp.window_proc_id = 1; // dBoxProc → skip chrome draw
        disp.fullscreen_locked = false;

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(row30_x96),
            0xFF,
            "1bpp src bit=1 must map to 8bpp screen idx 0xFF"
        );
        assert_eq!(
            bus.read_byte(row30_x97),
            0x00,
            "1bpp src bit=0 must map to 8bpp screen idx 0x00 (blit MUST fire)"
        );
    }

    #[test]
    fn redraw_chrome_blit_1bpp_to_8bpp_uses_active_ctab_black_entry() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);
        install_twilight_style_black_index(&mut disp, &mut bus);

        let offscreen_base = bus.alloc(64 * 200);
        bus.write_long(PORT_PTR + 2, offscreen_base);
        bus.write_word(PORT_PTR + 6, 64);
        bus.write_word(PORT_PTR + 8, 0);
        bus.write_word(PORT_PTR + 10, 0);
        bus.write_word(PORT_PTR + 12, 200);
        bus.write_word(PORT_PTR + 14, 512);

        bus.write_byte(offscreen_base + 30 * 64 + 12, 0b10000000);

        let row30_x96 = screen_base + 30 * 800 + 96;
        let row30_x97 = screen_base + 30 * 800 + 97;
        bus.write_byte(row30_x96, 0xAB);
        bus.write_byte(row30_x97, 0xAB);

        bus.write_word(crate::memory::globals::addr::MBAR_HEIGHT, 20);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 200, 400);
        disp.window_proc_id = 1;
        disp.fullscreen_locked = false;

        disp.redraw_chrome(&mut bus);

        assert_eq!(
            bus.read_byte(row30_x96),
            1,
            "1bpp src bit=1 must use the active ColorTable's black index"
        );
        assert_eq!(
            bus.read_byte(row30_x97),
            0,
            "1bpp src bit=0 must still use the active ColorTable's white index"
        );
    }

    #[test]
    fn redraw_chrome_blit_1bpp_to_1bpp_preserves_odd_edges_and_padding() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(8);
        disp.screen_mode = (screen_base, 4, 16, 1, 1);
        let (screen_clut, _) = TrapDispatcher::standard_mac_indexed_clut(1).unwrap();
        *disp.color_manager_clut = screen_clut;
        *disp.device_clut = screen_clut;
        bus.write_long(0x0824, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let offscreen_base = bus.alloc(8);
        bus.write_long(PORT_PTR + 2, offscreen_base);
        bus.write_word(PORT_PTR + 6, 3);
        bus.write_word(PORT_PTR + 8, 0);
        bus.write_word(PORT_PTR + 10, (-1i16) as u16);
        bus.write_word(PORT_PTR + 12, 1);
        bus.write_word(PORT_PTR + 14, 9);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 1);
        bus.write_word(PORT_PTR + 22, 8);
        let source = [0xad, 0x7f, 0xcd];
        bus.write_bytes(offscreen_base, &source);
        bus.fill_bytes(screen_base, 8, 0xaa);
        disp.front_window = PORT_PTR;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(bus.read_bytes(screen_base, 4), vec![0xad, 0x2a, 0xaa, 0xaa]);
        assert_eq!(bus.read_bytes(screen_base + 4, 4), vec![0xaa; 4]);
        assert_eq!(bus.read_bytes(offscreen_base, 3), source);
    }

    #[test]
    fn redraw_chrome_blit_1bpp_to_2bpp_uses_destination_packing_and_palette() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(8);
        disp.screen_mode = (screen_base, 4, 12, 1, 2);
        let (screen_clut, _) = TrapDispatcher::standard_mac_indexed_clut(2).unwrap();
        *disp.color_manager_clut = screen_clut;
        *disp.device_clut = screen_clut;
        bus.write_long(0x0824, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let offscreen_base = bus.alloc(8);
        bus.write_long(PORT_PTR + 2, offscreen_base);
        bus.write_word(PORT_PTR + 6, 3);
        bus.write_word(PORT_PTR + 8, 0);
        bus.write_word(PORT_PTR + 10, (-1i16) as u16);
        bus.write_word(PORT_PTR + 12, 1);
        bus.write_word(PORT_PTR + 14, 9);
        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 1);
        bus.write_word(PORT_PTR + 22, 8);
        let source = [0xad, 0x7f, 0xcd];
        bus.write_bytes(offscreen_base, &source);
        bus.fill_bytes(screen_base, 8, 0xaa);
        disp.front_window = PORT_PTR;

        disp.blit_window_to_screen(&mut bus);

        let expected = [0u8, 3, 0, 3, 3, 0, 3, 0];
        assert_eq!(
            TrapDispatcher::fb_get_pixel_index(&bus, screen_base, 4, 2, 12, 1, 0, 0),
            Some(2)
        );
        for (offset, expected) in expected.into_iter().enumerate() {
            assert_eq!(
                TrapDispatcher::fb_get_pixel_index(
                    &bus,
                    screen_base,
                    4,
                    2,
                    12,
                    1,
                    offset as i16 + 1,
                    0,
                ),
                Some(expected)
            );
        }
        for x in 9..12 {
            assert_eq!(
                TrapDispatcher::fb_get_pixel_index(&bus, screen_base, 4, 2, 12, 1, x, 0),
                Some(2)
            );
        }
        assert_eq!(bus.read_byte(screen_base + 3), 0xaa);
        assert_eq!(bus.read_bytes(screen_base + 4, 4), vec![0xaa; 4]);
        assert_eq!(bus.read_bytes(offscreen_base, 3), source);
    }

    #[test]
    fn redraw_chrome_blit_2bpp_to_1bpp_translates_colors_and_preserves_canaries() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(8);
        disp.screen_mode = (screen_base, 4, 16, 1, 1);
        let (screen_clut, _) = TrapDispatcher::standard_mac_indexed_clut(1).unwrap();
        *disp.color_manager_clut = screen_clut;
        *disp.device_clut = screen_clut;
        bus.write_long(0x0824, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let (src_clut, _) = TrapDispatcher::standard_mac_indexed_clut(2).unwrap();
        let src_ctab_handle = make_ctab_handle(&mut bus, &src_clut, 0x2000_0002);
        let offscreen_base = bus.alloc(8);
        let pixmap_handle =
            install_8bpp_cgrafport(&mut bus, offscreen_base, 4, 10, 1, src_ctab_handle);
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_word(pixmap + 8, (-1i16) as u16);
        bus.write_word(pixmap + 12, 9);
        bus.write_word(pixmap + 32, 2);
        bus.write_word(pixmap + 36, 2);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 22, 8);
        let source = [0x8c, 0xf3, 0x2a, 0xcd];
        bus.write_bytes(offscreen_base, &source);
        bus.fill_bytes(screen_base, 8, 0xaa);
        disp.front_window = PORT_PTR;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(bus.read_bytes(screen_base, 4), vec![0xad, 0x2a, 0xaa, 0xaa]);
        assert_eq!(bus.read_bytes(screen_base + 4, 4), vec![0xaa; 4]);
        assert_eq!(bus.read_bytes(offscreen_base, 4), source);
    }

    #[test]
    fn redraw_chrome_blit_8bpp_to_8bpp_translates_port_ctab_to_screen() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(70 * 64);
        disp.screen_mode = (screen_base, 70, 70, 64, 8);
        bus.write_long(0x0824, screen_base);

        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let mut src_clut = TrapDispatcher::standard_mac_8bpp_clut();
        let dst_clut = TrapDispatcher::standard_mac_8bpp_clut();
        src_clut[42] = dst_clut[7];
        let src_ctab_handle = make_ctab_handle(&mut bus, &src_clut, 0x1234_5678);

        let offscreen_base = bus.alloc(64 * 64);
        install_8bpp_cgrafport(&mut bus, offscreen_base, 64, 64, 64, src_ctab_handle);
        bus.write_word(PORT_PTR + 22, 70);

        bus.write_byte(offscreen_base + 5 * 64 + 10, 42);
        bus.write_byte(offscreen_base + 5 * 64 + 11, 8);
        bus.write_byte(screen_base + 5 * 70 + 10, 0xAA);
        bus.write_byte(screen_base + 5 * 70 + 11, 0xAA);
        bus.fill_bytes(screen_base + 5 * 70 + 64, 6, 0xA5);

        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 64, 64);
        disp.window_proc_id = 1;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 5 * 70 + 10),
            7,
            "8bpp window blit must translate source CTab index 42 to the screen index for its RGB"
        );
        assert_eq!(
            bus.read_byte(screen_base + 5 * 70 + 11),
            8,
            "same-RGB entries should remain stable through the translation table"
        );
        assert_eq!(
            bus.read_bytes(screen_base + 5 * 70 + 64, 6),
            vec![0xA5; 6],
            "an oversized portRect must not read past the source PixMap bounds"
        );
    }

    #[test]
    fn redraw_chrome_blit_4bpp_to_4bpp_copies_packed_pixels() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(4 * 2);
        disp.screen_mode = (screen_base, 4, 8, 2, 4);
        bus.write_long(0x0824, screen_base);

        let (clut, _) = TrapDispatcher::standard_mac_indexed_clut(4).unwrap();
        let ctab_handle = make_ctab_handle(&mut bus, &clut, 4);
        let offscreen_base = bus.alloc(4 * 2);
        let pixmap_handle = install_8bpp_cgrafport(&mut bus, offscreen_base, 4, 8, 2, ctab_handle);
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_word(pixmap + 32, 4);
        bus.write_word(pixmap + 36, 4);

        bus.write_bytes(offscreen_base, &[0x12, 0x34, 0x56, 0x78]);
        bus.fill_bytes(screen_base, 4 * 2, 0xAA);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 2, 8);
        disp.window_proc_id = 1;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(bus.read_bytes(screen_base, 4), vec![0x12, 0x34, 0x56, 0x78]);
        assert_eq!(
            bus.read_bytes(screen_base + 4, 4),
            vec![0x00, 0x00, 0x00, 0x00]
        );
    }

    #[test]
    fn redraw_chrome_blit_2bpp_to_2bpp_copies_packed_pixels() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(2 * 2);
        disp.screen_mode = (screen_base, 2, 8, 2, 2);
        bus.write_long(0x0824, screen_base);

        let (clut, _) = TrapDispatcher::standard_mac_indexed_clut(2).unwrap();
        let ctab_handle = make_ctab_handle(&mut bus, &clut, 2);
        // Reserve more than the four visible bytes: a four-byte allocation is
        // the in-memory shape of a classic Handle and the PixMap base resolver
        // intentionally treats it as one.  The extra guard bytes keep this
        // fixture focused on packed-pixel copying.
        let offscreen_base = bus.alloc(2 * 2 + 4);
        let pixmap_handle = install_8bpp_cgrafport(&mut bus, offscreen_base, 2, 8, 2, ctab_handle);
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_word(pixmap + 32, 2);
        bus.write_word(pixmap + 36, 2);

        bus.write_bytes(offscreen_base, &[0b00_01_10_11, 0b11_10_01_00]);
        bus.fill_bytes(screen_base, 2 * 2, 0xaa);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 2, 8);
        disp.window_proc_id = 1;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(
            bus.read_bytes(screen_base, 2),
            vec![0b00_01_10_11, 0b11_10_01_00]
        );
        assert_eq!(bus.read_bytes(screen_base + 2, 2), vec![0x00, 0x00]);
    }

    #[test]
    fn redraw_chrome_blit_2bpp_clamps_oversized_port_rect_to_pixmap_bounds() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(8);
        disp.screen_mode = (screen_base, 4, 12, 1, 2);
        bus.write_long(0x0824, screen_base);
        let (clut, _) = TrapDispatcher::standard_mac_indexed_clut(2).unwrap();
        *disp.color_manager_clut = clut;
        *disp.device_clut = clut;
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let ctab_handle = make_ctab_handle(&mut bus, &clut, 2);
        let offscreen_base = bus.alloc(8);
        let pixmap_handle = install_8bpp_cgrafport(&mut bus, offscreen_base, 3, 8, 1, ctab_handle);
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_word(pixmap + 32, 2);
        bus.write_word(pixmap + 36, 2);
        bus.write_word(PORT_PTR + 22, 12);
        let source = [0b00_01_10_11, 0b11_10_01_00, 0xcd];
        bus.write_bytes(offscreen_base, &source);
        bus.fill_bytes(screen_base, 8, 0xaa);
        disp.front_window = PORT_PTR;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(
            bus.read_bytes(screen_base, 4),
            vec![source[0], source[1], 0xaa, 0xaa]
        );
        assert_eq!(bus.read_bytes(screen_base + 4, 4), vec![0xaa; 4]);
        assert_eq!(bus.read_bytes(offscreen_base, 3), source);
    }

    #[test]
    fn window_blit_clamps_extreme_bounds_to_indexed_source_and_destination_rows() {
        for depth in [1u16, 2, 4, 8] {
            let (mut disp, _cpu, mut bus) = setup_with_port();
            let screen_base = bus.alloc(8);
            disp.screen_mode = (screen_base, 1, 16, 1, depth);
            bus.write_long(0x0824, screen_base);
            let (screen_clut, _) = TrapDispatcher::standard_mac_indexed_clut(depth).unwrap();
            *disp.color_manager_clut = screen_clut;
            *disp.device_clut = screen_clut;
            let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
            bus.write_long(0x08A4, gdevice_handle);
            bus.write_long(0x0CC8, gdevice_handle);
            let screen_ctab_handle = TrapDispatcher::gdevice_ctab_handle(&bus, gdevice_handle);

            let (source_ctab_handle, source_pixel, expected_first) = if depth == 8 {
                let mut source_clut = screen_clut;
                source_clut[42] = screen_clut[7];
                (make_ctab_handle(&mut bus, &source_clut, 0x1234_5678), 42, 7)
            } else {
                (screen_ctab_handle, 0, 0)
            };
            let offscreen_base = bus.alloc(8);
            let pixmap_handle =
                install_8bpp_cgrafport(&mut bus, offscreen_base, 1, 16, 1, source_ctab_handle);
            let pixmap = bus.read_long(pixmap_handle);
            bus.write_word(pixmap + 32, depth);
            bus.write_word(pixmap + 36, depth);
            if depth == 1 {
                // Exercise the full signed QuickDraw coordinate span. The
                // declared geometry is deliberately much wider than either
                // one-byte row and must not overflow i16 arithmetic.
                bus.write_word(pixmap + 8, i16::MIN as u16);
                bus.write_word(pixmap + 12, i16::MAX as u16);
                bus.write_word(PORT_PTR + 18, i16::MIN as u16);
                bus.write_word(PORT_PTR + 22, i16::MAX as u16);
            }
            let source = [source_pixel, 0xCD, 0xCD, 0xCD];
            bus.write_bytes(offscreen_base, &source);
            bus.fill_bytes(screen_base, 8, 0xAA);
            disp.front_window = PORT_PTR;

            disp.blit_window_to_screen(&mut bus);

            assert_eq!(
                bus.read_byte(screen_base),
                expected_first,
                "{depth}bpp blit should still copy the representable first row byte"
            );
            assert_eq!(
                bus.read_bytes(screen_base + 1, 7),
                vec![0xAA; 7],
                "{depth}bpp destination padding must remain untouched"
            );
            assert_eq!(bus.read_bytes(offscreen_base, 4), source);
        }
    }

    #[test]
    fn redraw_chrome_blit_2bpp_translates_different_pixmap_tables() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(2);
        disp.screen_mode = (screen_base, 2, 8, 1, 2);
        bus.write_long(0x0824, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);
        let screen_ctab_handle = TrapDispatcher::gdevice_ctab_handle(&bus, gdevice_handle);
        let screen_ctab = bus.read_long(screen_ctab_handle);
        bus.write_long(screen_ctab, 0x2222_0002);

        let mut dst_clut = TrapDispatcher::standard_mac_8bpp_clut();
        dst_clut[1] = [0x1234, 0x5678, 0x9abc];
        *disp.color_manager_clut = dst_clut;
        *disp.device_clut = dst_clut;
        let mut src_clut = dst_clut;
        src_clut[3] = dst_clut[1];
        src_clut[1] = [0xffff, 0, 0];
        let src_ctab_handle = make_ctab_handle(&mut bus, &src_clut, 0x1111_0002);

        let offscreen_base = bus.alloc(2);
        let pixmap_handle =
            install_8bpp_cgrafport(&mut bus, offscreen_base, 2, 8, 1, src_ctab_handle);
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_word(pixmap + 32, 2);
        bus.write_word(pixmap + 36, 2);
        bus.write_bytes(offscreen_base, &[0xff, 0xff]);
        bus.fill_bytes(screen_base, 2, 0xaa);
        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 1, 8);
        disp.window_proc_id = 1;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(
            bus.read_bytes(screen_base, 2),
            vec![0x55, 0x55],
            "source index 3 must map to the active 2bpp destination's exact color at index 1"
        );
    }

    #[test]
    fn redraw_chrome_blit_2bpp_to_8bpp_translates_pixels_without_touching_padding() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(16);
        disp.screen_mode = (screen_base, 16, 12, 1, 8);
        bus.write_long(0x0824, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let mut dst_clut = TrapDispatcher::standard_mac_8bpp_clut();
        let destination_indices = [10u8, 20, 30, 40];
        let colors = [
            [0x1111, 0x2222, 0x3333],
            [0x4444, 0x5555, 0x6666],
            [0x7777, 0x8888, 0x9999],
            [0xaaaa, 0xbbbb, 0xcccc],
        ];
        for (index, color) in destination_indices.into_iter().zip(colors) {
            dst_clut[index as usize] = color;
        }
        *disp.color_manager_clut = dst_clut;
        *disp.device_clut = dst_clut;

        let mut src_clut = [[0u16; 3]; 256];
        src_clut[..4].copy_from_slice(&colors);
        let src_ctab_handle = make_ctab_handle(&mut bus, &src_clut, 0x2000_0002);
        let offscreen_base = bus.alloc(8);
        let pixmap_handle =
            install_8bpp_cgrafport(&mut bus, offscreen_base, 3, 6, 1, src_ctab_handle);
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_word(pixmap + 8, (-1i16) as u16);
        bus.write_word(pixmap + 12, 5);
        bus.write_word(pixmap + 32, 2);
        bus.write_word(pixmap + 36, 2);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 22, 5);

        let source = [0b11_00_01_10, 0b11_01_00_00, 0xcc];
        bus.write_bytes(offscreen_base, &source);
        bus.fill_bytes(screen_base, 16, 0xa5);
        disp.front_window = PORT_PTR;
        disp.window_proc_id = 1;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(bus.read_byte(screen_base), 0xa5);
        assert_eq!(bus.read_bytes(screen_base + 1, 5), vec![10, 20, 30, 40, 20]);
        assert_eq!(bus.read_bytes(screen_base + 6, 10), vec![0xa5; 10]);
        assert_eq!(bus.read_bytes(offscreen_base, 3), source);
    }

    #[test]
    fn redraw_chrome_blit_4bpp_to_2bpp_preserves_odd_edges_and_row_padding() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(8);
        disp.screen_mode = (screen_base, 4, 12, 1, 2);
        bus.write_long(0x0824, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let colors = [
            [0xffff, 0xffff, 0xffff],
            [0xffff, 0, 0],
            [0, 0xffff, 0],
            [0, 0, 0],
        ];
        let mut dst_clut = [[0u16; 3]; 256];
        dst_clut[..4].copy_from_slice(&colors);
        *disp.color_manager_clut = dst_clut;
        *disp.device_clut = dst_clut;

        let mut src_clut = [[0u16; 3]; 256];
        for (index, color) in src_clut[..16].iter_mut().enumerate() {
            *color = colors[index % colors.len()];
        }
        let src_ctab_handle = make_ctab_handle(&mut bus, &src_clut, 0x4000_0004);
        let offscreen_base = bus.alloc(10);
        let pixmap_handle =
            install_8bpp_cgrafport(&mut bus, offscreen_base, 6, 10, 1, src_ctab_handle);
        let pixmap = bus.read_long(pixmap_handle);
        bus.write_word(pixmap + 8, (-1i16) as u16);
        bus.write_word(pixmap + 12, 9);
        bus.write_word(pixmap + 32, 4);
        bus.write_word(pixmap + 36, 4);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 22, 8);

        let source = [0xf0, 0x12, 0x31, 0x23, 0x0f, 0xcd];
        bus.write_bytes(offscreen_base, &source);
        bus.fill_bytes(screen_base, 8, 0xaa);
        disp.front_window = PORT_PTR;
        disp.window_proc_id = 1;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(bus.read_bytes(screen_base, 4), vec![0x86, 0xdb, 0x2a, 0xaa]);
        assert_eq!(bus.read_bytes(screen_base + 4, 4), vec![0xaa; 4]);
        assert_eq!(bus.read_bytes(offscreen_base, 6), source);
    }

    #[test]
    fn redraw_chrome_blit_skips_tracked_window_swapped_to_scratch_pixmap() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(64 * 64);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);
        bus.write_long(0x0824, screen_base);
        let gdevice_handle = disp.ensure_main_gdevice(&mut bus);
        bus.write_long(0x08A4, gdevice_handle);
        bus.write_long(0x0CC8, gdevice_handle);

        let clut = TrapDispatcher::standard_mac_8bpp_clut();
        let ctab_handle = make_ctab_handle(&mut bus, &clut, 8);
        let original_base = bus.alloc(64 * 64);
        let original_pixmap =
            install_8bpp_cgrafport(&mut bus, original_base, 64, 64, 64, ctab_handle);
        disp.window_original_pixmaps
            .insert(PORT_PTR, original_pixmap);

        let scratch_base = bus.alloc(64 * 64);
        install_8bpp_cgrafport(&mut bus, scratch_base, 64, 64, 64, ctab_handle);

        bus.write_byte(screen_base + 8 * 64 + 8, 0xAA);
        bus.write_byte(scratch_base + 8 * 64 + 8, 0x22);
        disp.front_window = PORT_PTR;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 8 * 64 + 8),
            0xAA,
            "SetPortPix scratch buffers must not be auto-presented as window contents"
        );

        bus.write_long(PORT_PTR + 2, original_pixmap);
        bus.write_byte(original_base + 8 * 64 + 8, 0x11);

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 8 * 64 + 8),
            0x11,
            "the original tracked window backing PixMap should still be presented"
        );
    }

    #[test]
    fn framed_manual_cport_uses_explicit_off_center_guest_geometry() {
        let (mut disp, _cpu, mut bus) = setup_with_port();
        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(512 * 322);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 512, 512, 322, 0);
        track_manual_cport(&mut disp, &bus, manual_port);
        disp.last_screen_frame_rect = Some(crate::trap::dispatch::ScreenCopyBitsRect {
            src_top: 85,
            src_left: 141,
            src_bottom: 413,
            src_right: 659,
            dst_top: 85,
            dst_left: 141,
            dst_bottom: 413,
            dst_right: 659,
        });

        assert_eq!(
            disp.framed_manual_cport_presentation_rect(&bus),
            disp.last_screen_frame_rect,
            "a guest-drawn 518x328 frame should locate its 512x322 retained image without centering it"
        );
    }

    #[test]
    fn redraw_chrome_blits_large_manual_cport_centered_when_front_window_is_screen_backed() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[0] = [0, 0, 0];
        disp.device_clut[0xAA] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        bus.fill_bytes(manual_base, 640 * 420, 0x44);
        bus.write_byte(manual_base + 419 * 640 + 639, 0x55);
        bus.write_byte(screen_base + 90 * 800 + 79, 0xAA);
        bus.write_byte(screen_base + 90 * 800 + 80, 0xAA);
        bus.write_byte(screen_base + 509 * 800 + 719, 0xAA);

        assert_eq!(
            disp.declared_centered_presentation_rect(&bus),
            Some(crate::trap::dispatch::ScreenCopyBitsRect {
                src_top: 0,
                src_left: 0,
                src_bottom: 420,
                src_right: 640,
                dst_top: 90,
                dst_left: 80,
                dst_bottom: 510,
                dst_right: 720,
            }),
            "the frontend should learn the declared viewport before the first scene is presented"
        );

        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            disp.manual_cport_presentation_rect(&bus),
            Some(crate::trap::dispatch::ScreenCopyBitsRect {
                src_top: 0,
                src_left: 0,
                src_bottom: 420,
                src_right: 640,
                dst_top: 90,
                dst_left: 80,
                dst_bottom: 510,
                dst_right: 720,
            }),
            "only the manual CPort actually selected for presentation should become a frontend viewport"
        );

        assert_eq!(
            bus.read_byte(screen_base + 90 * 800 + 80),
            0x44,
            "640x420 manual scene should be centered at x=80,y=90 on an 800x600 screen"
        );
        assert_eq!(
            bus.read_byte(screen_base + 509 * 800 + 719),
            0x55,
            "bottom-right source pixel should land at the centered destination edge"
        );
        assert_eq!(
            bus.read_byte(screen_base + 90 * 800 + 79),
            0xAA,
            "manual CPort presentation must not top-left blit or overrun the centered scene"
        );
    }

    #[test]
    fn redraw_chrome_blits_large_manual_cport_when_fullscreen_port_rect_origin_is_shifted() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[0] = [0, 0, 0];
        disp.device_clut[0xAA] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (0, 0, 600, 800);

        // SetOrigin and related port operations can shift the local
        // portRect while the Window Manager bounds still cover the screen.
        bus.write_word(PORT_PTR + 16, (-85i16) as u16);
        bus.write_word(PORT_PTR + 18, (-144i16) as u16);
        bus.write_word(PORT_PTR + 20, 515);
        bus.write_word(PORT_PTR + 22, 656);

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        bus.fill_bytes(manual_base, 640 * 420, 0x44);
        bus.write_byte(manual_base + 640 * 420 - 1, 0x55);
        bus.write_byte(screen_base + 90 * 800 + 80, 0xAA);

        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 90 * 800 + 80),
            0x44,
            "shifted portRect must not hide a full-screen tracked window from the manual CPort presentation bridge"
        );
    }

    #[test]
    fn redraw_chrome_blits_large_manual_cport_when_small_front_window_leaves_dark_screen() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[255] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0xFF);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (185, 226, 415, 574);

        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 230);
        bus.write_word(PORT_PTR + 22, 348);

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        let dst = screen_base + 90 * 800 + 80;
        bus.fill_bytes(manual_base, 640 * 420, 0x44);
        bus.write_byte(manual_base + 640 * 420 - 1, 0x55);
        bus.write_byte(dst, 0xFF);

        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(dst),
            0x44,
            "a blank screen-backed front window can still reveal a large app-managed scene buffer"
        );
    }

    #[test]
    fn redraw_chrome_does_not_blit_large_manual_cport_over_nonblank_small_front_window() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[1] = [0xFFFF, 0xFFFF, 0xFFFF];
        disp.device_clut[255] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0xFF);
        bus.write_byte(screen_base + 25 * 800 + 25, 1);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (185, 226, 415, 574);

        bus.write_word(PORT_PTR + 16, 0);
        bus.write_word(PORT_PTR + 18, 0);
        bus.write_word(PORT_PTR + 20, 230);
        bus.write_word(PORT_PTR + 22, 348);

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        let dst = screen_base + 90 * 800 + 80;
        bus.write_byte(manual_base, 0x44);
        bus.write_byte(dst, 0xAA);

        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(dst),
            0xAA,
            "manual scene fallback must not overpaint an already visible small window"
        );
    }

    #[test]
    fn redraw_chrome_does_not_latch_blank_manual_cport_over_nonblank_fullscreen() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[1] = [0xFFFF, 0xFFFF, 0xFFFF];
        disp.device_clut[255] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0x01);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (0, 0, 600, 800);

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(656 * 600);
        bus.fill_bytes(manual_base, 656 * 600, 0xFF);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 656, 656, 600, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        let covered_probe = screen_base + 300 * 800 + 400;
        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(covered_probe),
            0x01,
            "a blank manual CPort must not overpaint already-rendered fullscreen content"
        );
        assert_eq!(
            disp.manual_cport_presented_port, 0,
            "blank manual CPorts must not latch presentation privilege before real screen blits"
        );

        disp.copybits_screen_count = 1;
        bus.write_byte(covered_probe, 0x02);
        bus.write_byte(manual_base, 0x44);
        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(covered_probe),
            0x02,
            "once screen CopyBits is active, an unlatched manual CPort must stay suppressed"
        );
    }

    #[test]
    fn redraw_chrome_does_not_latch_manual_cport_over_visible_fullscreen_content() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[1] = [0xFFFF, 0xFFFF, 0xFFFF];
        bus.fill_bytes(screen_base, 800 * 600, 0x01);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.window_bounds = (0, 0, 600, 800);

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(656 * 600);
        bus.fill_bytes(manual_base, 656 * 600, 0x44);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 656, 656, 600, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        let covered_probe = screen_base + 300 * 800 + 400;
        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(covered_probe),
            0x01,
            "an unlatched manual CPort must not overpaint visible fullscreen content"
        );
        assert_eq!(
            disp.manual_cport_presented_port, 0,
            "visible fullscreen content should prevent acquiring the manual CPort fallback latch"
        );
    }

    #[test]
    fn redraw_chrome_does_not_blit_manual_cport_after_screen_copybits() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[0] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];
        disp.copybits_screen_count = 1;

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        bus.write_byte(manual_base, 0x44);
        bus.write_byte(screen_base + 90 * 800 + 80, 0xAA);

        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 90 * 800 + 80),
            0xAA,
            "apps already presenting through CopyBits should not be overwritten by the fallback"
        );
    }

    #[test]
    fn redraw_chrome_does_not_present_a_setportpix_scratch_buffer() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[0] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];

        let manual_port = bus.alloc(200);
        let original_base = bus.alloc(800 * 600);
        install_8bpp_cgrafport_at(&mut bus, manual_port, original_base, 800, 800, 600, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        // SetPortPix replaces the original handle with a scratch PixMap. The
        // game may draw a complete-looking image there, but Apple documents
        // it as a different drawing target, not an implicit screen update.
        let manual_base = bus.alloc(512 * 322);
        bus.fill_bytes(manual_base, 512 * 322, 0x44);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 512, 512, 322, 0);
        disp.last_screen_frame_rect = Some(crate::trap::dispatch::ScreenCopyBitsRect {
            src_top: 85,
            src_left: 141,
            src_bottom: 413,
            src_right: 659,
            dst_top: 85,
            dst_left: 141,
            dst_bottom: 413,
            dst_right: 659,
        });

        let centered_probe = screen_base + 139 * 800 + 144;
        disp.blit_large_manual_cport_to_screen(&mut bus);
        assert_eq!(bus.read_byte(centered_probe), 0);
        assert_eq!(disp.manual_cport_presented_port, 0);
        assert_eq!(disp.declared_centered_presentation_rect(&bus), None);
        assert_eq!(
            disp.framed_manual_cport_presentation_rect(&bus),
            disp.last_screen_frame_rect,
            "the explicit screen frame may locate the viewport without presenting the attached scratch PixMap"
        );
    }

    #[test]
    fn redraw_chrome_continues_latched_manual_cport_after_later_screen_copybits() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[0] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        let dst = screen_base + 90 * 800 + 80;
        bus.fill_bytes(manual_base, 640 * 420, 0x44);
        bus.write_byte(manual_base + 640 * 420 - 1, 0x55);
        disp.blit_large_manual_cport_to_screen(&mut bus);
        assert_eq!(bus.read_byte(dst), 0x44);

        disp.copybits_screen_count = 1;
        bus.write_byte(dst, 0xAA);
        bus.write_byte(manual_base, 0x66);
        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(dst),
            0x66,
            "a manual CPort selected before a later CopyBits should keep presenting"
        );
    }

    #[test]
    fn redraw_chrome_releases_latched_manual_cport_after_direct_framebuffer_draw() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        disp.device_clut[0] = [0, 0, 0];
        bus.fill_bytes(screen_base, 800 * 600, 0);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        *disp.window_list = vec![PORT_PTR];

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        track_manual_cport(&mut disp, &bus, manual_port);

        let dst = screen_base + 90 * 800 + 80;
        bus.fill_bytes(manual_base, 640 * 420, 0x44);
        bus.write_byte(manual_base + 640 * 420 - 1, 0x55);
        disp.blit_large_manual_cport_to_screen(&mut bus);
        assert_eq!(bus.read_byte(dst), 0x44);

        for row in 0..420 {
            bus.fill_bytes(dst + row * 800, 640, 0xAA);
        }
        bus.fill_bytes(manual_base, 640 * 420, 0x66);
        disp.blit_large_manual_cport_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(dst),
            0xAA,
            "direct framebuffer rendering must remain authoritative over the fallback CPort"
        );
        assert_eq!(
            disp.manual_cport_presented_port, 0,
            "substantial direct framebuffer rendering should release the compatibility latch"
        );
    }
}
