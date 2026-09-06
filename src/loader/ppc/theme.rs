//! Native Toolbox presentation using the shared Systemless theme provider.

use super::*;
use crate::ui_theme::{ControlKind, ControlState, ThemeDrawCtx, ThemeRect};

pub(super) fn ppc_ui_theme(gworlds: &[PpcGWorldRecord]) -> UiThemeId {
    gworlds
        .iter()
        .find(|world| world.port == PPC_MAIN_GWORLD)
        .filter(|world| world.depth != 1)
        .map_or(UiThemeId::ClassicSystem7, |world| world.ui_theme)
}

pub(super) fn ppc_theme_rgb(rgb: Rgb8) -> PpcRgbColor {
    PpcRgbColor {
        red: u16::from(rgb.r) * 0x0101,
        green: u16::from(rgb.g) * 0x0101,
        blue: u16::from(rgb.b) * 0x0101,
    }
}

impl PpcLoadedApp {
    /// Select native Toolbox presentation without changing guest records or metrics.
    pub(crate) fn set_ui_theme(&mut self, theme: UiThemeId) {
        let Some(main) = self
            .gworlds
            .iter_mut()
            .find(|world| world.port == PPC_MAIN_GWORLD)
        else {
            return;
        };
        if main.ui_theme == theme {
            return;
        }
        main.ui_theme = theme;
        self.repaint_theme_desktop(false);
    }

    pub(crate) fn repaint_theme_desktop(&mut self, include_menu_bar: bool) {
        // The loader seeds the desktop before the runner selects presentation.
        // Repaint only the exposed desktop; existing window content remains owned
        // by the guest. Later WDEF/CDEF draws read the same screen provider.
        let Some(front) =
            ppc_live_front_buffer_for_gworld(&mut self.memory, &self.gworlds, PPC_MAIN_GWORLD)
        else {
            return;
        };
        let regions: Vec<_> = self
            .window_list
            .iter()
            .copied()
            .filter(|&window| ppc_window_is_visible(&mut self.memory, window))
            .collect();
        let regions: Vec<_> = regions
            .into_iter()
            .filter_map(|window| {
                ppc_window_global_structure_bounds(&mut self.memory, &self.gworlds, window)
            })
            .collect();
        let top = if include_menu_bar {
            0
        } else {
            self.memory.read_u16_be(PPC_MBAR_HEIGHT_ADDR).unwrap_or(20) as i32
        };
        let colors = [
            ppc_standard_desktop_color(&self.gworlds, 1, 0),
            ppc_standard_desktop_color(&self.gworlds, 0, 0),
        ]
        .map(|color| ppc_physical_screen_color_pixel(front, color, &self.screen_clut));
        for y in top..front.height as i32 {
            for x in 0..front.width as i32 {
                if regions.iter().any(|r| {
                    y >= i32::from(r.0)
                        && y < i32::from(r.2)
                        && x >= i32::from(r.1)
                        && x < i32::from(r.3)
                }) {
                    continue;
                }
                let ink = crate::window_manager::standard_desktop_pattern_is_ink(x, y);
                if let Some(pixel) = colors[usize::from(ink)] {
                    let _ = ppc_quickdraw_write_raw_pixel(&mut self.memory, front, (x, y), pixel);
                }
            }
        }
    }
}

pub(super) fn ppc_draw_themed_control(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    owner: u32,
    control: u32,
    proc_id: i16,
    is_default: bool,
    bounds: (i16, i16, i16, i16),
) -> Option<bool> {
    let theme_id = ppc_ui_theme(gworlds);
    if theme_id == UiThemeId::ClassicSystem7 {
        return None;
    }
    let kind = match proc_id {
        0 => ControlKind::PushButton,
        1 => ControlKind::Checkbox,
        2 => ControlKind::RadioButton,
        _ => return None,
    };
    let bounds = match proc_id {
        1 => crate::control_manager::standard_checkbox_layout(bounds).indicator,
        2 => crate::control_manager::standard_radio_button_layout(bounds).indicator,
        _ => bounds,
    };
    let (top, left, bottom, right) = bounds;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    if width <= 0 || height <= 0 {
        return Some(true);
    }
    let theme = theme_id.provider();
    let pad = if is_default {
        theme.dialog_metrics().default_button_outline.max(0)
    } else {
        0
    };
    let hilite = memory
        .read_u8(control + PPC_CONTROL_HILITE_OFFSET)
        .unwrap_or(0);
    let selected = memory
        .read_u16_be(control + PPC_CONTROL_VALUE_OFFSET)
        .unwrap_or(0)
        != 0;
    let mut bitmap = ThemeBitmap::new(
        width.saturating_add(pad.saturating_mul(2)) as u32,
        height.saturating_add(pad.saturating_mul(2)) as u32,
        theme.palette().window_background,
    );
    theme.draw_control(
        &mut ThemeDrawCtx::new(&mut bitmap),
        ControlState {
            kind,
            rect: ThemeRect {
                top: pad,
                left: pad,
                bottom: pad + height,
                right: pad + width,
            },
            enabled: hilite != 255,
            pressed: hilite != 0 && hilite != 255,
            selected,
            is_default,
        },
    );
    Some(ppc_blit_theme_bitmap(
        memory,
        gworlds,
        owner,
        top - pad,
        left - pad,
        &bitmap,
    ))
}

pub(super) fn ppc_theme_components(rgb: Rgb8) -> [u16; 3] {
    let rgb = ppc_theme_rgb(rgb);
    [rgb.red, rgb.green, rgb.blue]
}

pub(super) fn ppc_draw_themed_selection(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    port: u32,
    bounds: (i16, i16, i16, i16),
) -> bool {
    let theme = ppc_ui_theme(gworlds);
    if theme == UiThemeId::ClassicSystem7 {
        return false;
    }
    let (top, left, bottom, right) = bounds;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    if width <= 0 || height <= 0 {
        return true;
    }
    let provider = theme.provider();
    let transparent = provider.palette().window_background;
    let mut bitmap = ThemeBitmap::new(width as u32, height as u32, transparent);
    provider.draw_text_selection(
        &mut ThemeDrawCtx::new(&mut bitmap),
        crate::ui_theme::TextSelectionState {
            rect: ThemeRect {
                top: 0,
                left: 0,
                bottom: height,
                right: width,
            },
            active: true,
        },
    );
    ppc_blit_theme_bitmap_masked(memory, gworlds, port, top, left, &bitmap, Some(transparent));
    true
}

pub(super) fn ppc_draw_themed_dialog_frame(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    content: (i16, i16, i16, i16),
    frame: (i16, i16, i16, i16),
    proc_id: i16,
) -> bool {
    let theme = ppc_ui_theme(gworlds);
    if theme == UiThemeId::ClassicSystem7 {
        return false;
    }
    let width = frame.3.saturating_sub(frame.1);
    let height = frame.2.saturating_sub(frame.0);
    if width <= 0 || height <= 0 {
        return true;
    }
    let provider = theme.provider();
    let mut bitmap = ThemeBitmap::new(
        width as u32,
        height as u32,
        provider.palette().window_background,
    );
    provider.draw_dialog_frame(
        &mut ThemeDrawCtx::new(&mut bitmap),
        crate::ui_theme::DialogFrameState {
            frame_rect: ThemeRect {
                top: 0,
                left: 0,
                bottom: height,
                right: width,
            },
            content_rect: ThemeRect {
                top: content.0 - frame.0,
                left: content.1 - frame.1,
                bottom: content.2 - frame.0,
                right: content.3 - frame.1,
            },
            kind: crate::ui_theme::DialogFrameKind::from_window_proc_id(proc_id),
            active: true,
            fill_content: true,
        },
    );
    ppc_blit_theme_bitmap(memory, gworlds, PPC_MAIN_GWORLD, frame.0, frame.1, &bitmap);
    true
}

pub(super) fn ppc_draw_themed_control_rect(
    memory: &mut PpcSectionMem,
    gworlds: &[PpcGWorldRecord],
    owner: u32,
    bounds: (i16, i16, i16, i16),
    kind: ControlKind,
    enabled: bool,
    is_default: bool,
) -> bool {
    let theme = ppc_ui_theme(gworlds);
    if theme == UiThemeId::ClassicSystem7 {
        return false;
    }
    let provider = theme.provider();
    let pad = if is_default {
        provider.dialog_metrics().default_button_outline.max(0)
    } else {
        0
    };
    let (top, left, bottom, right) = bounds;
    let width = right.saturating_sub(left);
    let height = bottom.saturating_sub(top);
    if width <= 0 || height <= 0 {
        return true;
    }
    let mut bitmap = ThemeBitmap::new(
        (width + 2 * pad) as u32,
        (height + 2 * pad) as u32,
        provider.palette().window_background,
    );
    provider.draw_control(
        &mut ThemeDrawCtx::new(&mut bitmap),
        ControlState {
            kind,
            rect: ThemeRect {
                top: pad,
                left: pad,
                bottom: pad + height,
                right: pad + width,
            },
            enabled,
            pressed: false,
            selected: false,
            is_default,
        },
    );
    ppc_blit_theme_bitmap(memory, gworlds, owner, top - pad, left - pad, &bitmap);
    true
}
