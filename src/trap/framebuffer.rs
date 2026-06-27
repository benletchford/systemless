//! Framebuffer drawing methods for menu bar and window chrome rendering.

use std::{collections::HashSet, sync::OnceLock};

use crate::memory::{MacMemoryBus, MemoryBus};
use crate::quickdraw::fonts::{heuristics::get_italic_slant, Glyph};
use crate::quickdraw::text::{
    get_font_metrics, get_glyph, get_glyph_italic, get_underline_thickness,
};
use crate::ui_theme::{
    CaretState, ControlKind, ControlState, DialogFrameKind, DialogFrameState, MenuBarState,
    MenuDropdownState, MenuItemState, MenuTitleState, Rgb8, ScrollbarOrientation, ScrollbarPart,
    ScrollbarState, TextFieldState, TextSelectionState, ThemeBitmap, ThemeDrawCtx, ThemeRect,
    UiThemeId,
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

// MTE 1992 p. 3-60 defines the low-order `Style` byte bit assignments:
// bold, italic, underline, outline, shadow, condensed, and extended.
const TEXT_STYLE_BOLD: u8 = 0x01;
const TEXT_STYLE_ITALIC: u8 = 0x02;
const TEXT_STYLE_UNDERLINE: u8 = 0x04;
const TEXT_STYLE_OUTLINE: u8 = 0x08;
const TEXT_STYLE_SHADOW: u8 = 0x10;
const TEXT_STYLE_CONDENSE: u8 = 0x20;
const TEXT_STYLE_EXTEND: u8 = 0x40;

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
        theme.draw_scrollbar(
            &mut ctx,
            ScrollbarState {
                rect: ThemeRect {
                    top: 0,
                    left: 0,
                    bottom: height,
                    right: width,
                },
                orientation: if height > width {
                    ScrollbarOrientation::Vertical
                } else {
                    ScrollbarOrientation::Horizontal
                },
                enabled: hilite != 255 && min < max,
                value,
                min,
                max,
                highlighted_part: ScrollbarPart::from_control_part_code(hilite),
            },
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
        for y in 0..bitmap.height() {
            for x in 0..bitmap.width() {
                let offset = ((y * bitmap.width() + x) * 4) as usize;
                let color = Rgb8 {
                    r: rgba[offset],
                    g: rgba[offset + 1],
                    b: rgba[offset + 2],
                };
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left.saturating_add(x as i16),
                    top.saturating_add(y as i16),
                    Self::theme_color_is_mono_black(color),
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
        for y in 0..bitmap.height() {
            for x in 0..bitmap.width() {
                let offset = ((y * bitmap.width() + x) * 4) as usize;
                let color = Rgb8 {
                    r: rgba[offset],
                    g: rgba[offset + 1],
                    b: rgba[offset + 2],
                };
                if !Self::theme_color_is_mono_black(color) {
                    continue;
                }
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left.saturating_add(x as i16),
                    top.saturating_add(y as i16),
                    true,
                );
            }
        }
    }

    fn theme_color_is_mono_black(color: Rgb8) -> bool {
        u16::from(color.r) + u16::from(color.g) + u16::from(color.b) < 128 * 3
    }

    fn active_gdevice_ctab(bus: &MacMemoryBus) -> Option<u32> {
        let gdevice_handle = {
            let current = bus.read_long(0x0CC8); // TheGDevice
            if current != 0 {
                current
            } else {
                bus.read_long(0x08A4) // MainDevice
            }
        };
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

    fn ctab_value_luma(bus: &MacMemoryBus, ctab: u32, wanted_value: u8) -> Option<u32> {
        let count = u32::from(bus.read_word(ctab + 6)).min(255) + 1;

        let ordinal = u32::from(wanted_value);
        if ordinal < count {
            let entry = ctab + 8 + ordinal * 8;
            if bus.read_word(entry) == u16::from(wanted_value) {
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
        let mut best_index = 0u8;
        let mut best_luma = 0u32;
        let mut found = false;
        for ordinal in 0..count {
            let entry = ctab + 8 + ordinal * 8;
            let value = bus.read_word(entry);
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

    pub(crate) fn fb_pixel_index_for_rgb(bus: &MacMemoryBus, rgb: [u16; 3]) -> Option<u8> {
        let ctab = Self::active_gdevice_ctab(bus)?;
        let count = u32::from(bus.read_word(ctab + 6)).min(255) + 1;

        // Imaging With QuickDraw 1994 p. 4-82 describes inverse-table
        // lookup as the Color Manager path from RGB colors to device pixel
        // values. Keep canonical endpoints pinned when the active table has
        // the standard white/black entries, then prefer exact matches before
        // falling back to nearest Euclidean distance in 16-bit RGB space.
        if rgb == [0, 0, 0] && Self::ctab_value_luma(bus, ctab, 255) == Some(0) {
            return Some(255);
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
            let value = bus.read_word(entry);
            if value > 255 {
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

    fn logical_mono_pixel_indexes(bus: &MacMemoryBus) -> (u8, u8) {
        (
            Self::logical_white_pixel_index(bus),
            Self::logical_black_pixel_index(bus),
        )
    }

    /// Set a single pixel in the framebuffer (screen coordinates).
    /// Works for both 1bpp and 8bpp screen modes.
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
        if x < 0 || y < 0 || x >= screen_width || y >= screen_height {
            return;
        }
        if pixel_size == 8 {
            let addr = screen_base + (y as u32) * row_bytes + (x as u32);
            bus.write_byte(
                addr,
                if black {
                    Self::logical_black_pixel_index(bus)
                } else {
                    Self::logical_white_pixel_index(bus)
                },
            );
        } else {
            let byte_offset = (y as u32) * row_bytes + (x as u32 / 8);
            let bit = 7 - (x as u32 % 8);
            let addr = screen_base + byte_offset;
            let b = bus.read_byte(addr);
            if black {
                bus.write_byte(addr, b | (1 << bit));
            } else {
                bus.write_byte(addr, b & !(1 << bit));
            }
        }
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
        if pixel_size != 8 {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                x,
                y,
                pixel_index != 0,
            );
            return;
        }
        if x < 0 || y < 0 || x >= screen_width || y >= screen_height {
            return;
        }
        let addr = screen_base + (y as u32) * row_bytes + (x as u32);
        bus.write_byte(addr, pixel_index);
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
        for y in top..bottom {
            let row_addr = screen_base + y * row_bytes;
            for x in left..right {
                bus.write_byte(row_addr + x, pixel_index);
            }
        }
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
            for y in top..bottom {
                let row_addr = screen_base + y * row_bytes;
                for x in left..right {
                    bus.write_byte(row_addr + x, fill);
                }
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
            for x in left..right {
                bus.write_byte(row_addr + x, fill);
            }
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
            0,
            None,
            true,
        );
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
        style: u8,
        pixel_index_override: Option<u8>,
        black: bool,
    ) {
        let gx = x + glyph.origin_x as i16;
        let gy = y + glyph.origin_y as i16;
        let gw = glyph.width as usize;
        let gh = glyph.height as usize;
        let metrics = synthetic_italic
            .map(|(font_id, font_size)| (font_id, font_size, get_font_metrics(font_id, font_size)));
        let text_index = if pixel_size == 8 {
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
                    let (dst_start, dst_end) = if (style & TEXT_STYLE_EXTEND) != 0 {
                        let start = (col as i16 * 4) / 3;
                        let end = (((col as i16 + 1) * 4) / 3).max(start + 1);
                        (start, end)
                    } else if (style & TEXT_STYLE_CONDENSE) != 0 {
                        let start = (col as i16 * 3) / 4;
                        (start, start + 1)
                    } else {
                        let start = col as i16;
                        (start, start + 1)
                    };
                    for dst_col in dst_start..dst_end {
                        let px = gx + dst_col + slant;
                        if let Some(text_index) = text_index {
                            if px >= 0 && py >= 0 && px < screen_width && py < screen_height {
                                let addr = screen_base + (py as u32) * row_bytes + (px as u32);
                                bus.write_byte(addr, text_index);
                            }
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
        style: u8,
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
                    if (style & TEXT_STYLE_BOLD) != 0 {
                        pixels.insert((px + 1, py));
                    }
                }
            }
        }

        pixels
    }

    fn fb_styled_glyph_advance(glyph: &Glyph, style: u8) -> i16 {
        let mut advance = glyph.advance as i16;
        if (style & TEXT_STYLE_BOLD) != 0 {
            // Menu item styles follow the classic Style bitset; the
            // System 7 MDEF renders bold item names with the synthetic
            // one-pixel strike and matching one-pixel pen advance while
            // CalcMenuSize keeps plain guest metrics. MTE 1992 pp. 3-133
            // to 3-134.
            advance += 1;
        }
        if (style & TEXT_STYLE_OUTLINE) != 0 {
            advance += 1;
        }
        if (style & TEXT_STYLE_SHADOW) != 0 {
            advance += 2;
        }
        if (style & TEXT_STYLE_CONDENSE) != 0 && advance >= 6 {
            advance -= 1;
        }
        if (style & TEXT_STYLE_EXTEND) != 0 {
            advance += 1;
        }
        advance.max(1)
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
        style: u8,
        pixel_index_override: Option<u8>,
        black: bool,
    ) -> i16 {
        let italic = (style & TEXT_STYLE_ITALIC) != 0;
        let (glyph_hit, synthetic_italic) = if italic {
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

        let glyph_y = if (style & TEXT_STYLE_SHADOW) != 0 {
            y - 1
        } else {
            y
        };
        let base_pixels =
            Self::fb_styled_glyph_base_pixels(x, glyph_y, glyph, data, synthetic_italic, style);

        if (style & (TEXT_STYLE_OUTLINE | TEXT_STYLE_SHADOW)) == 0 {
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
            return Self::fb_styled_glyph_advance(glyph, style);
        }

        // QuickDraw outlines/shadows text by smearing a 1-bit glyph mask,
        // then XORing the original glyph out of the result. That produces
        // hollow outline and shadow faces instead of drawing offset filled
        // glyph copies.
        let smear_max = if (style & TEXT_STYLE_SHADOW) != 0 && (style & TEXT_STYLE_OUTLINE) != 0 {
            3
        } else if (style & TEXT_STYLE_SHADOW) != 0 {
            2
        } else {
            1
        };
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

        Self::fb_styled_glyph_advance(glyph, style)
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
    ) -> i16 {
        let Some((glyph, data)) = get_glyph(font_id, font_size, ch) else {
            return 6;
        };
        let gx = x + glyph.origin_x as i16;
        let gy = y + glyph.origin_y as i16;
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

    fn fb_draw_string_clipped(
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

        if (style & TEXT_STYLE_UNDERLINE) != 0 && cx > x {
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
        // Vertically center text: baseline = top_margin + ascent
        let text_height = metrics.ascent + metrics.descent;
        let text_y = (menu_bar_height - text_height) / 2 + metrics.ascent;

        // Draw visible menu titles from the current menu list. InsertMenu
        // with beforeID=-1 installs a submenu/popup in the current menu
        // list without adding a menu-bar title. MTE 1992, p. 3-121.
        let mut x: i16 = 18;
        for menu in &self.menus {
            if !menu.visible_in_menu_bar {
                continue;
            }
            let title = &menu.title;
            let title_width = Self::fb_measure_string(title, font_id, font_size);
            // HIG 1992 p. 54 says unavailable menu titles remain visible
            // but dimmed; p. 55 says pressing a menu title highlights it.
            // Route title-state chrome through the provider while keeping
            // classic text metrics and title hit regions unchanged.
            self.draw_theme_menu_title_chrome(
                bus,
                1,
                x - 7,
                menu_bar_height - 1,
                x + title_width + 6,
                menu.enabled,
                false,
            );
            let title_index = Self::menu_title_pixel_index(bus, menu.id, pixel_size);
            let classic_plain_dimmed_title = self.ui_theme_id() == UiThemeId::ClassicSystem7
                && title_index.is_none()
                && !menu.enabled;
            let width = if classic_plain_dimmed_title {
                // MTE 1992 p. 3-131: DisableItem(menu, 0) disables the
                // whole menu title. On a plain classic screen dump, the
                // standard MDEF's dimmed title treatment resolves to the
                // menu-bar background, matching the System 7.5.3 oracle while
                // preserving title spacing and hit regions.
                title_width
            } else if Self::is_apple_mark_title(title) {
                Self::fb_draw_classic_apple_mark_title(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    x,
                    text_y,
                    title_index,
                );
                title_width
            } else if let Some(pixel_index) = title_index {
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
            x += width + 13;
        }
    }

    fn is_apple_mark_title(title: &str) -> bool {
        let mut chars = title.chars();
        matches!(chars.next(), Some('\u{14}' | '\u{F8FF}')) && chars.next().is_none()
    }

    fn fb_draw_classic_apple_mark_title(
        bus: &mut MacMemoryBus,
        screen_base: u32,
        row_bytes: u32,
        pixel_size: u16,
        screen_width: i16,
        screen_height: i16,
        x: i16,
        baseline_y: i16,
        pixel_index_override: Option<u8>,
    ) {
        // System 7's standard menu-bar appleMark title is MDEF-owned chrome,
        // not the raw Chicago $14 glyph. The mask below is the 11x14 title
        // bitmap captured from the BasiliskII/System 7.5.3 oracle; it is drawn
        // one pixel left of the title origin and with the same baseline as
        // Chicago 12 menu titles.
        const MASK: [&str; 14] = [
            ".......##..",
            "......##...",
            "......#....",
            "..###..###.",
            ".##########",
            "...........",
            "...........",
            "...........",
            "...........",
            "###########",
            ".##########",
            ".##########",
            "..########.",
            "...##..##..",
        ];

        let left = x - 1;
        let top = baseline_y - 12;
        for (dy, row) in MASK.iter().enumerate() {
            for (dx, byte) in row.as_bytes().iter().enumerate() {
                if *byte != b'#' {
                    continue;
                }
                let dst_x = left + dx as i16;
                let dst_y = top + dy as i16;
                if let Some(pixel_index) = pixel_index_override {
                    Self::fb_set_pixel_index(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        dst_x,
                        dst_y,
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
                        dst_x,
                        dst_y,
                        true,
                    );
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
        // The standard menu bar stamps the classic rounded screen-corner
        // mask when drawn at the top edge. IM:I I-354 defines DrawMenuBar
        // as the routine that redraws the current menu bar.
        const LEFT: &[(i16, i16)] = &[
            (0, 0),
            (1, 0),
            (2, 0),
            (3, 0),
            (4, 0),
            (0, 1),
            (1, 1),
            (2, 1),
            (0, 2),
            (1, 2),
            (0, 3),
            (0, 4),
        ];
        for &(x, y) in LEFT {
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
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                screen_width - 1 - x,
                y,
                true,
            );
        }
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
        if pixel_size != 8 || screen_w == 0 || screen_h == 0 {
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
            (bus.read_long(pm_ptr) & 0x3FFFFFFF, pm_ptr, pm_handle) // mask off flags
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
        let wr_top = bus.read_word(port + 16) as i16;
        let wr_left = bus.read_word(port + 18) as i16;
        let wr_bottom = bus.read_word(port + 20) as i16;
        let wr_right = bus.read_word(port + 22) as i16;

        let w = (wr_right - wr_left) as u32;
        let h = (wr_bottom - wr_top) as u32;
        if w == 0 || h == 0 {
            return;
        }

        let src_y_offset = wr_top.max(0) as u32;
        let src_x_offset = wr_left.max(0) as u32;
        let dst_y = src_y_offset;
        let dst_x = src_x_offset;

        // 1bpp source → 8bpp screen via per-pixel bit extraction.
        // For each source bit, resolve logical white/black through the
        // active GDevice ColorTable. Applications can repurpose 0xFF away
        // from black while still expecting mono source bits to scan out black.
        if port_pixel_size == 1 && pixel_size == 8 {
            let (white_index, black_index) = Self::logical_mono_pixel_indexes(bus);
            // Games may set portRect MUCH larger than the actual BitMap
            // bounds (e.g. StuntCopter: portRect=(0,0..567,791) but
            // BitMap=(0,0..261,426)). Clamp source reads to the BitMap
            // bounds (portBits.bounds at port + 8..15) so we don't walk
            // past valid source data into adjacent rows. Without this,
            // the per-row stride bug produces horizontally-doubled or
            // tiled content.
            let pb_top = bus.read_word(port + 8) as i16;
            let pb_left = bus.read_word(port + 10) as i16;
            let pb_bottom = bus.read_word(port + 12) as i16;
            let pb_right = bus.read_word(port + 14) as i16;
            let bitmap_w = (pb_right - pb_left).max(0) as u32;
            let bitmap_h = (pb_bottom - pb_top).max(0) as u32;
            let row_count = h.min((screen_h as u32).saturating_sub(dst_y)).min(bitmap_h);
            let col_count = w.min((screen_w as u32).saturating_sub(dst_x)).min(bitmap_w);
            for row in 0..row_count {
                let src_row_addr = port_base + (src_y_offset + row) * port_rb;
                let dst_row_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
                for col in 0..col_count {
                    let src_bit_x = src_x_offset + col;
                    let src_byte = bus.read_byte(src_row_addr + src_bit_x / 8);
                    let bit = (src_byte >> (7 - (src_bit_x & 7))) & 1;
                    let dst_idx = if bit == 0 { white_index } else { black_index };
                    bus.write_byte(dst_row_addr + col, dst_idx);
                }
            }
            return;
        }

        // Same-depth fast path. Anything that's neither matched-depth nor
        // 1bpp→8bpp falls through to a no-op.
        if port_pixel_size != pixel_size as u32 {
            return;
        }

        let row_count = h.min((screen_h as u32).saturating_sub(dst_y));
        let col_count = w.min((screen_w as u32).saturating_sub(dst_x));

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
                let translation =
                    self.build_palette_translation(bus, &src_clut, &dst_clut, screen_ctab_handle);
                for row in 0..row_count {
                    let src_addr = port_base + (src_y_offset + row) * port_rb + src_x_offset;
                    let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
                    for col in 0..col_count {
                        let src_idx = bus.read_byte(src_addr + col);
                        bus.write_byte(dst_addr + col, translation[src_idx as usize]);
                    }
                }
                return;
            }
        }

        // block_move per row.
        for row in 0..row_count {
            let src_addr = port_base + (src_y_offset + row) * port_rb + src_x_offset;
            let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
            bus.block_move(src_addr, dst_addr, col_count);
        }
    }

    /// Present a large app-managed CGrafPort when CopyBits did not present it.
    ///
    /// This is an HLE compatibility bridge, not a QuickDraw rule: real apps
    /// are responsible for copying offscreen ports to the screen. Some games
    /// keep a full-scene PixMap behind an OpenCPort/InitCPort and update it
    /// directly while their front window remains screen-backed. Without a
    /// Window Manager or video driver layer to observe that buffer, screenshots
    /// stay black even though the scene exists in guest memory.
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
            bus.read_long(pm_ptr) & 0x3FFF_FFFF
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
        if front_top > 0
            || front_left > 0
            || front_bottom < screen_h as i16
            || front_right < screen_w as i16
        {
            if trace {
                eprintln!(
                    "[BLIT-CPORT] skip: front bounds ({},{},{},{}) do not cover {}x{}",
                    front_top, front_left, front_bottom, front_right, screen_w, screen_h
                );
            }
            return;
        }

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
            let pm_ptr = bus.read_long(pm_handle);
            if pm_ptr == 0 {
                rejected_shape += 1;
                continue;
            }

            // CGrafPort/PixMap layout follows Imaging With QuickDraw
            // 1994, pp. 4-64..4-65: portPixMap is a PixMapHandle, with
            // baseAddr, rowBytes, bounds, pixelSize, and pmTable here.
            let base = bus.read_long(pm_ptr) & 0x3FFF_FFFF;
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
                    "[BLIT-CPORT] skip: no candidate (tracked={}, considered={}, shape_rejects={}, area_rejects={}, min_area={})",
                    self.cport_ports.len(),
                    considered_ports,
                    rejected_shape,
                    rejected_area,
                    min_area
                );
            }
            return;
        };
        let dst_x = (screen_w_u32 - candidate.width) / 2;
        let dst_y = (screen_h_u32 - candidate.height) / 2;
        let row_count = candidate.height.min(screen_h_u32.saturating_sub(dst_y));
        let col_count = candidate.width.min(screen_w_u32.saturating_sub(dst_x));
        if row_count == 0 || col_count == 0 {
            return;
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
                self.build_palette_translation(bus, &src_clut, &dst_clut, screen_ctab_handle);
            for row in 0..row_count {
                let src_addr = candidate.base + row * candidate.row_bytes;
                let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
                for col in 0..col_count {
                    let src_idx = bus.read_byte(src_addr + col);
                    bus.write_byte(dst_addr + col, translation[src_idx as usize]);
                }
            }
            return;
        }

        for row in 0..row_count {
            let src_addr = candidate.base + row * candidate.row_bytes;
            let dst_addr = screen_base + (dst_y + row) * screen_rb + dst_x;
            bus.block_move(src_addr, dst_addr, col_count);
        }
    }

    /// Draw window chrome (title bar, close box, border) into the framebuffer
    /// WIND bounds are the CONTENT RECT; title bar is drawn ABOVE it.
    pub(crate) fn draw_window_chrome(&self, bus: &mut MacMemoryBus, active: bool) {
        let (screen_base, row_bytes, screen_width, screen_height, pixel_size) =
            self.get_screen_params();
        let (wind_top, wind_left, wind_bottom, wind_right) = self.window_bounds;

        // Title bar area: drawn ABOVE the content rect
        // Clamp to menu bar height — the Window Manager never draws
        // chrome into the menu bar area.
        let menu_bar_height = bus.read_word(crate::memory::globals::addr::MBAR_HEIGHT) as i16;
        let tb_top = (wind_top - 19).max(menu_bar_height);
        let tb_bottom = wind_top - 1;
        let tb_left = wind_left - 1;
        let tb_right = wind_right + 1;

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
        let has_go_away = active && matches!(self.window_proc_id, 0 | 4) && self.go_away_flag;

        // Draw title bar border. Classic document WDEFs leave the top edge
        // open; System 7.5.3 paints only side edges, bottom separator, and
        // active pinstripes. movableDBoxProc keeps the full title-frame top.
        if is_movable_modal {
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
        }
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
        for y in tb_top..=tb_bottom {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                tb_left,
                y,
                true,
            );
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                tb_right - 1,
                y,
                true,
            );
        }

        // Calculate title text area if we have a title
        let font_id: i16 = 0; // Chicago
        let font_size: i16 = 12;
        let metrics = get_font_metrics(font_id, font_size);
        let text_height = metrics.ascent + metrics.descent;
        let tb_interior_height = tb_bottom - tb_top - 1;
        let text_y = tb_top + 1 + (tb_interior_height - text_height) / 2 + metrics.ascent;

        let (title_clear_left, title_clear_right) = if !self.window_title.is_empty() {
            let mut title_width: i16 = 0;
            for ch in self.window_title.chars() {
                if let Some((glyph, _)) = get_glyph(font_id, font_size, ch) {
                    title_width += glyph.advance as i16;
                } else {
                    title_width += 6;
                }
            }
            let text_x = tb_left + (tb_right - tb_left - title_width) / 2;
            (text_x - 8, text_x + title_width + 8)
        } else {
            (tb_right, tb_right) // No clear area
        };

        let _close_box_width = if has_go_away { 15i16 } else { 0 };

        if is_movable_modal {
            // movableDBoxProc: plain title bar, no stripes
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
                    text_y,
                    &self.window_title,
                    font_id,
                    font_size,
                );
            }
        } else {
            // documentProc/noGrowDocProc: stripes + optional close box

            // Draw close box if goAwayFlag is set.
            //
            // Classic Mac System 7.5.3 close-box graphic per BasiliskII golden
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
                for y in cb_top..(cb_top + cb_size) {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        cb_left,
                        y,
                        true,
                    );
                }

                // Bottom-right Γ: 8-tall right edge + 8-wide bottom edge,
                // inset 2 from the top-left and 1 from the bottom-right.
                let inner_right = cb_left + cb_size - 2; // x=cb_left+9
                let inner_bottom = cb_top + cb_size - 2; // y=cb_top+9
                for y in (cb_top + 2)..(cb_top + cb_size - 1) {
                    Self::fb_set_pixel(
                        bus,
                        screen_base,
                        row_bytes,
                        pixel_size,
                        screen_width,
                        screen_height,
                        inner_right,
                        y,
                        true,
                    );
                }
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

            // Draw horizontal stripe pattern in title bar (classic Mac pinstripes)
            // Only active windows get stripes; inactive windows have plain white title bars
            //
            // System 7.5.3 reserves only 6 px of clear-area on each side of
            // the title text for stripes (the 16-px `title_clear_left/right`
            // margin is for text-glyph hit-testing, not for stripes). The
            // active document WDEF paints pinstripe rows at title-bar offsets
            // 1, 3, and 5; this row placement is calibrated against the
            // BasiliskII System 7.5.3 oracle.
            // Inside Macintosh Volume V, V-188 figure 5-3.
            if active {
                let stripe_left_edge = tb_left + 2;
                let stripe_right_end = tb_right - 2;
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
                    text_y - 5,
                    &self.window_title,
                    font_id,
                    font_size,
                    (tb_top, tb_left, tb_bottom - 2, tb_right),
                );
            }
        }

        // Draw window content area border
        for y in wind_top..wind_bottom {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                wind_left - 1,
                y,
                true,
            );
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                wind_right,
                y,
                true,
            );
        }
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
        for y in tb_top..=(wind_bottom + 1) {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                wind_right + 1,
                y,
                true,
            );
        }
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

        // DrawGrowIcon draws the scroll bar separator lines:
        // - Vertical line at content_right - 15 from content_top to content_bottom
        // - Horizontal line at content_bottom - 15 from border_left to border_right
        let sep_x = content_right - 15;
        let sep_y = content_bottom - 15;
        let border_left = content_left - 1;
        let border_right = content_right + 1;

        // Vertical scroll separator (full content height)
        for y in content_top..content_bottom {
            Self::fb_set_pixel(
                bus,
                screen_base,
                row_bytes,
                pixel_size,
                screen_width,
                screen_height,
                sep_x,
                y,
                true,
            );
        }
        // Horizontal scroll separator (border to border)
        Self::fb_hline(
            bus,
            screen_base,
            row_bytes,
            pixel_size,
            screen_width,
            screen_height,
            sep_y,
            border_left,
            border_right + 1,
            true,
        );
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
            for y in top..bottom {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    left + dx,
                    y,
                    true,
                );
            }
        }
        // Right edge: 2 columns (right-2 and right-1, both inside the rect)
        for dx in 0..2i16 {
            for y in top..bottom {
                Self::fb_set_pixel(
                    bus,
                    screen_base,
                    row_bytes,
                    pixel_size,
                    screen_width,
                    screen_height,
                    right - 2 + dx,
                    y,
                    true,
                );
            }
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
    ///   - documentProc (0), noGrowDocProc (4): Title bar chrome
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
        if self.window_uses_custom_def_proc(bus, window_ptr) {
            return;
        }
        // plainDBox (2), dBoxProc (1), altDBoxProc (3), and rDocProc (16)
        // windows have NO title bar — only a border — per Inside Macintosh
        // Volume I, I-275. Dispatch to draw_window_frame for those procIDs
        // rather than draw_window_chrome (which paints title-bar chrome).
        let proc_id = self.window_proc_ids.get(&window_ptr).copied().unwrap_or(0);
        let port_version = bus.read_word(window_ptr + 6);
        let (pmap_top, pmap_left) = if (port_version & 0xC000) == 0xC000 {
            let pm_handle = bus.read_long(window_ptr + 2);
            let pm_ptr = bus.read_long(pm_handle);
            (
                bus.read_word(pm_ptr + 6) as i16,
                bus.read_word(pm_ptr + 8) as i16,
            )
        } else {
            (
                bus.read_word(window_ptr + 8) as i16,
                bus.read_word(window_ptr + 10) as i16,
            )
        };
        // wrapping_neg / wrapping_add match 68k Mac OS i16 wrap-
        // around — guards against debug-build panics on windows
        // whose pixmap.bounds.topLeft is i16::MIN or whose total
        // width/height exceeds i16 range.
        let wind_top = pmap_top.wrapping_neg();
        let wind_left = pmap_left.wrapping_neg();
        let port_bottom = bus.read_word(window_ptr + 20) as i16;
        let port_right = bus.read_word(window_ptr + 22) as i16;
        let wind_bottom = wind_top.wrapping_add(port_bottom);
        let wind_right = wind_left.wrapping_add(port_right);
        if wind_bottom <= wind_top || wind_right <= wind_left {
            return;
        }
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
    }

    pub(crate) fn draw_window_frame(&self, bus: &mut MacMemoryBus) {
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
            0 | 4 => {
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
                if front_is_dialog && dialog_ptr != front_window {
                    None
                } else {
                    Some(snapshot.clone())
                }
            })
            .collect();
        for snapshot in snapshots {
            let bounds = snapshot.bounds;
            self.restore_dialog_pixels(bus, bounds, &snapshot.pixels);
        }
    }

    /// Redraw the menu bar and window chrome into the framebuffer.
    ///
    /// On a real Mac, the Window Manager maintains these UI elements and redraws
    /// them after any update. Our emulator draws them as raw framebuffer pixels,
    /// so game drawing (explosions, etc.) can overwrite them. This method restores
    /// the chrome and should be called after each frame of emulation.
    pub fn redraw_chrome(&mut self, bus: &mut MacMemoryBus) {
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
        // `TrapDispatcher::menu_bar_hidden`) suppresses the chrome strip
        // even when MBarHeight is non-zero. Treat it like fullscreen for
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

        let skip_chrome = matches!(self.window_proc_id, 1 | 2 | 3 | 5) || effective_mbar <= 0;

        // Draw chrome for each visible non-front window FIRST
        // (back-to-front order per window_list which is front-to-back),
        // then the front window on top. Each back-window's chrome uses its
        // WindowRecord-stored state (bounds derived from portPixMap bounds,
        // title from titleHandle, goAway byte, hilited byte).
        if !skip_chrome && effective_mbar > 0 {
            let list_snapshot = self.window_list.clone();
            let saved_bounds = self.window_bounds;
            let saved_title = self.window_title.clone();
            let saved_proc = self.window_proc_id;
            let saved_go_away = self.go_away_flag;
            // Iterate back-to-front so earlier windows get overdrawn
            // by later ones.
            for &w in list_snapshot.iter().rev() {
                if w == self.front_window {
                    continue;
                }
                if bus.read_byte(w + 110u32) == 0 {
                    // Not visible.
                    continue;
                }
                let preserved_front_pixels: Vec<_> = self
                    .window_structure_rect(bus, w)
                    .map(|back_structure| {
                        list_snapshot
                            .iter()
                            .take_while(|&&front_window| front_window != w)
                            .filter(|&&front_window| bus.read_byte(front_window + 110u32) != 0)
                            .filter_map(|&front_window| {
                                self.window_structure_rect(bus, front_window)
                                    .and_then(|front_structure| {
                                        Self::rect_intersection(back_structure, front_structure)
                                    })
                                    .and_then(|rect| self.save_screen_rect_pixels(bus, rect))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                // Derive per-window screen bounds from port geometry:
                // init_cgraf_window writes pixmap bounds as
                // (-wind_top, -wind_left, screen_h - wind_top,
                //  screen_w - wind_left) — so wind_top = -bounds_top,
                // wind_left = -bounds_left, and window size comes
                // from portRect at window_ptr+16.
                let port_version = bus.read_word(w + 6);
                let (pmap_top, pmap_left) = if (port_version & 0xC000) == 0xC000 {
                    let pm_handle = bus.read_long(w + 2);
                    let pm_ptr = bus.read_long(pm_handle);
                    (
                        bus.read_word(pm_ptr + 6) as i16,
                        bus.read_word(pm_ptr + 8) as i16,
                    )
                } else {
                    (bus.read_word(w + 8) as i16, bus.read_word(w + 10) as i16)
                };
                let wind_top = -pmap_top;
                let wind_left = -pmap_left;
                let port_bottom = bus.read_word(w + 20) as i16;
                let port_right = bus.read_word(w + 22) as i16;
                let wind_bottom = wind_top + port_bottom;
                let wind_right = wind_left + port_right;
                // Degenerate / invalid — skip.
                if wind_bottom <= wind_top || wind_right <= wind_left {
                    continue;
                }
                self.window_bounds = (wind_top, wind_left, wind_bottom, wind_right);
                // Read title from titleHandle at +134.
                let title_h = bus.read_long(w + 134u32);
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
                self.go_away_flag = bus.read_byte(w + 112u32) != 0;
                // Use the per-window procID. Windows with no title bar
                // (plainDBox/dBoxProc/altDBoxProc) draw only a border.
                let w_proc = self.window_proc_ids.get(&w).copied().unwrap_or(0);
                if self.window_uses_custom_def_proc(bus, w) {
                    continue;
                }
                self.window_proc_id = w_proc;
                let hilited = bus.read_byte(w + 111u32) != 0;
                if matches!(w_proc, 1..=3) {
                    self.draw_window_frame(bus);
                } else {
                    self.draw_window_chrome(bus, hilited);
                }
                for (top, left, width, height, pixels) in preserved_front_pixels {
                    self.restore_screen_rect_pixels(bus, top, left, width, height, &pixels);
                }
            }
            // Restore front-window state before drawing front chrome.
            self.window_bounds = saved_bounds;
            self.window_title = saved_title;
            self.window_proc_id = saved_proc;
            self.go_away_flag = saved_go_away;
        }

        if self.front_window != 0 && !skip_chrome {
            // Use the front window's hilited byte rather than hard-coding
            // active=true so HiliteWindow(front, false) renders no stripes.
            let front_hilited = bus.read_byte(self.front_window + 111u32) != 0;
            self.draw_single_window_chrome_inline(bus, self.front_window, front_hilited);
        }
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
                    if popup.highlighted_item > 0 {
                        self.invert_dropdown_item_rect(
                            bus,
                            popup.active_menu,
                            popup.dropdown_rect,
                            popup.highlighted_item,
                        );
                    }
                }
            }
        } else {
            self.restore_visible_dialog_snapshots(bus);
            self.redraw_retained_modal_dialog_click(bus);
        }

        // If a menu dropdown is open, redraw it on top of the menu bar
        // so that the menu bar redraw doesn't erase it.
        if let Some(ref tracking) = self.menu_tracking {
            self.highlight_menu_title(bus, tracking.active_menu);
            self.draw_menu_dropdown(bus, tracking.active_menu, tracking.dropdown_rect);
            // During flash, alternate highlight: even remaining = highlighted,
            // odd remaining = not highlighted. Outside flash, always highlight.
            let show_highlight = if tracking.flash_remaining > 0 {
                tracking.flash_remaining % 2 == 0
            } else {
                true
            };
            if tracking.highlighted_item > 0 && show_highlight {
                self.invert_menu_item(bus, tracking.highlighted_item);
            }
        }
        self.capture_gui_frame(bus, "redraw_chrome");
    }
}

#[cfg(test)]
mod redraw_chrome_tests {
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

    /// Counterpart to the kiosk-mode test above: when `menu_bar_hidden
    /// = false` (app-style hosting), `redraw_chrome` MUST paint the
    /// menu bar so menus are reachable. This pins the env-var-driven
    /// opt-out (SYSTEMLESS_SHOW_MENU_BAR=1 → menu_bar_hidden=false).
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
        disp.mouse_pos = (2, 400);
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
    fn redraw_chrome_blit_8bpp_to_8bpp_translates_port_ctab_to_screen() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(64 * 64);
        disp.screen_mode = (screen_base, 64, 64, 64, 8);
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

        bus.write_byte(offscreen_base + 5 * 64 + 10, 42);
        bus.write_byte(offscreen_base + 5 * 64 + 11, 8);
        bus.write_byte(screen_base + 5 * 64 + 10, 0xAA);
        bus.write_byte(screen_base + 5 * 64 + 11, 0xAA);

        disp.front_window = PORT_PTR;
        disp.window_bounds = (0, 0, 64, 64);
        disp.window_proc_id = 1;

        disp.blit_window_to_screen(&mut bus);

        assert_eq!(
            bus.read_byte(screen_base + 5 * 64 + 10),
            7,
            "8bpp window blit must translate source CTab index 42 to the screen index for its RGB"
        );
        assert_eq!(
            bus.read_byte(screen_base + 5 * 64 + 11),
            8,
            "same-RGB entries should remain stable through the translation table"
        );
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
    fn redraw_chrome_blits_large_manual_cport_centered_when_front_window_is_screen_backed() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        disp.window_list = vec![PORT_PTR];

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        disp.cport_ports.insert(manual_port);

        bus.write_byte(manual_base, 0x44);
        bus.write_byte(manual_base + 419 * 640 + 639, 0x55);
        bus.write_byte(screen_base + 90 * 800 + 79, 0xAA);
        bus.write_byte(screen_base + 90 * 800 + 80, 0xAA);
        bus.write_byte(screen_base + 509 * 800 + 719, 0xAA);

        disp.blit_large_manual_cport_to_screen(&mut bus);

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
    fn redraw_chrome_does_not_blit_manual_cport_after_screen_copybits() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        disp.window_list = vec![PORT_PTR];
        disp.copybits_screen_count = 1;

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        disp.cport_ports.insert(manual_port);

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
    fn redraw_chrome_continues_latched_manual_cport_after_later_screen_copybits() {
        let (mut disp, _cpu, mut bus) = setup_with_port();

        let screen_base = bus.alloc(800 * 600);
        disp.screen_mode = (screen_base, 800, 800, 600, 8);
        bus.write_long(0x0824, screen_base);
        install_8bpp_cgrafport(&mut bus, screen_base, 800, 800, 600, 0);
        bus.write_byte(PORT_PTR + WINDOW_VISIBLE_OFFSET, 0xFF);
        disp.front_window = PORT_PTR;
        disp.window_list = vec![PORT_PTR];

        let manual_port = bus.alloc(200);
        let manual_base = bus.alloc(640 * 420);
        install_8bpp_cgrafport_at(&mut bus, manual_port, manual_base, 640, 640, 420, 0);
        disp.cport_ports.insert(manual_port);

        let dst = screen_base + 90 * 800 + 80;
        bus.write_byte(manual_base, 0x44);
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
}
