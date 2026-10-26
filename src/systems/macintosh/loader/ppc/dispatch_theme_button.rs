//! Appearance Manager button drawing and resumable guest callbacks.
//!
//! ABI: Universal Interfaces 3.4.1 Appearance.h. Callback ordering and clipping:
//! Apple Appearance Manager Reference, DrawThemeButton/ThemeButtonDrawProcPtr.

use super::*;
use crate::ui_theme::{ControlKind, ControlState, ThemeDrawCtx, ThemeRect};

type Bounds = (i16, i16, i16, i16);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct PpcThemeButtonState {
    calls: Vec<ButtonCall>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ButtonCall {
    import_pc: u32,
    return_pc: u32,
    rtoc: u32,
    port: u32,
    device: u32,
    original_clip: u32,
    drawing_state: Vec<u8>,
    scratch: u32,
    temporary_clip: u32,
    bounds: Bounds,
    background: Bounds,
    content: Bounds,
    kind: u16,
    state: u32,
    value: u16,
    adornment: u16,
    erase: u32,
    label: u32,
    user_data: u32,
    depth: u32,
    stage: u8,
}

pub(super) struct ThemeButtonContext<'a> {
    pub binding: &'a PpcImportBinding,
    pub cpu: &'a mut PpcCpu,
    pub memory: &'a mut PpcSectionMem,
    pub manager: &'a mut ProcessNativeMemoryManager,
    pub heap_cursor: &'a mut u32,
    pub last_mem_error: &'a mut i16,
    pub state: &'a mut PpcThemeButtonState,
    pub gworlds: &'a [PpcGWorldRecord],
    pub port: &'a mut u32,
    pub device: &'a mut u32,
    pub fore_color: &'a mut PpcRgbColor,
    pub back_color: &'a mut PpcRgbColor,
    pub pen_h: &'a mut i16,
    pub pen_v: &'a mut i16,
    pub text_mode: &'a mut i16,
    pub text_size: &'a mut i16,
}

fn read_bounds(memory: &mut PpcSectionMem, address: u32) -> Option<Bounds> {
    let bytes = ppc_memory_read_bytes(memory, address, 8)?;
    let word = |i| i16::from_be_bytes([bytes[i], bytes[i + 1]]);
    Some((word(0), word(2), word(4), word(6)))
}

fn write_bounds(memory: &mut PpcSectionMem, address: u32, bounds: Bounds) {
    for (i, word) in [bounds.0, bounds.1, bounds.2, bounds.3]
        .into_iter()
        .enumerate()
    {
        let _ = memory.write_u16_be(address + i as u32 * 2, word as u16);
    }
}

fn content_bounds(bounds: Bounds, kind: u16, adornment: u16) -> Bounds {
    let (top, left, bottom, right) = bounds;
    if matches!(kind, 1 | 2 | 6 | 13 | 14) {
        let width = if kind >= 13 { 12 } else { 16 };
        return if adornment & 16 != 0 {
            (top, left, bottom, right.saturating_sub(width).max(left))
        } else {
            (top, left.saturating_add(width).min(right), bottom, right)
        };
    }
    let inset = if matches!(kind, 8 | 10) { 2 } else { 4 };
    (
        top.saturating_add(inset).min(bottom),
        left.saturating_add(inset).min(right),
        bottom.saturating_sub(inset).max(top),
        right
            .saturating_sub(if kind == 5 { 18 } else { inset })
            .max(left),
    )
}

pub(super) fn dispatch_theme_button(
    mut context: ThemeButtonContext<'_>,
) -> Option<PpcImportAction> {
    if context.binding.dispatcher_target != PpcImportDispatcherTarget::DrawThemeButton {
        return None;
    }
    let resumed = context.cpu.lr == context.cpu.pc
        && context
            .state
            .calls
            .last()
            .is_some_and(|call| call.import_pc == context.cpu.pc);
    if !resumed {
        let result = begin_button(&mut context);
        if let Err(error) = result {
            ppc_apply_process_native_allocator(
                context.manager,
                context.memory,
                context.heap_cursor,
                context.last_mem_error,
            );
            return Some(PpcImportAction::Return(ppc_i16_result(error)));
        }
    } else {
        restore_drawing_state(&mut context);
    }
    let action = advance_button(&mut context);
    ppc_apply_process_native_allocator(
        context.manager,
        context.memory,
        context.heap_cursor,
        context.last_mem_error,
    );
    Some(action)
}

fn begin_button(context: &mut ThemeButtonContext<'_>) -> Result<(), i16> {
    if context.cpu.gpr[3] == 0 || context.cpu.gpr[5] == 0 {
        return Err(PPC_PARAM_ERR);
    }
    let bounds = read_bounds(context.memory, context.cpu.gpr[3]).ok_or(PPC_PARAM_ERR)?;
    let kind = context.cpu.gpr[4] as u16;
    let info = ppc_memory_read_bytes(context.memory, context.cpu.gpr[5], 8).ok_or(PPC_PARAM_ERR)?;
    if context.cpu.gpr[6] != 0
        && ppc_memory_read_bytes(context.memory, context.cpu.gpr[6], 8).is_none()
    {
        return Err(PPC_PARAM_ERR);
    }
    let state = u32::from_be_bytes(info[0..4].try_into().unwrap());
    let value = u16::from_be_bytes([info[4], info[5]]);
    let adornment = u16::from_be_bytes([info[6], info[7]]);
    if kind > 15
        || !matches!(state, 0 | 1 | 2 | 3 | 6 | 7 | 8)
        || bounds.2 <= bounds.0
        || bounds.3 <= bounds.1
    {
        return Err(PPC_PARAM_ERR);
    }
    let surface = ppc_live_quickdraw_surface(context.memory, context.gworlds, *context.port)
        .ok_or(PPC_PARAM_ERR)?;
    let drawing_state =
        ppc_memory_read_bytes(context.memory, *context.port + 36, 40).ok_or(PPC_PARAM_ERR)?;
    let original_clip = context
        .memory
        .read_u32_be(*context.port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET)
        .ok_or(PPC_PARAM_ERR)?;
    let padding = if adornment & 5 != 0 {
        ppc_ui_theme(context.gworlds)
            .provider()
            .dialog_metrics()
            .default_button_outline
            .max(2)
    } else {
        0
    };
    let mut background = (
        bounds.0.saturating_sub(padding),
        bounds.1.saturating_sub(padding),
        bounds.2.saturating_add(padding),
        bounds.3.saturating_add(padding),
    );
    if matches!(kind, 1 | 2 | 6 | 13 | 14) && adornment & 32 != 0 {
        let span = if kind >= 13 { 10 } else { 13 };
        if adornment & 16 != 0 {
            background.1 = background.3.saturating_sub(span).max(background.1);
        } else {
            background.3 = background.1.saturating_add(span).min(background.3);
        }
    }
    let scratch = context.manager.new_native_ptr(context.memory, 24, true);
    if scratch == 0 {
        return Err(PPC_MEM_FULL_ERR);
    }
    write_bounds(context.memory, scratch, background);
    let content = content_bounds(bounds, kind, adornment);
    write_bounds(context.memory, scratch + 8, content);
    for (i, byte) in info.into_iter().enumerate() {
        let _ = context.memory.write_u8(scratch + 16 + i as u32, byte);
    }
    context.state.calls.push(ButtonCall {
        import_pc: context.cpu.pc,
        return_pc: context.cpu.lr,
        rtoc: context.cpu.gpr[2],
        port: *context.port,
        device: *context.device,
        original_clip,
        drawing_state,
        scratch,
        temporary_clip: 0,
        bounds,
        background,
        content,
        kind,
        state,
        value,
        adornment,
        erase: context.cpu.gpr[7],
        label: context.cpu.gpr[8],
        user_data: context.cpu.gpr[9],
        depth: u32::from(surface.front_buffer.depth),
        stage: 0,
    });
    Ok(())
}

fn restore_drawing_state(context: &mut ThemeButtonContext<'_>) {
    let call = context.state.calls.last_mut().unwrap();
    *context.port = call.port;
    *context.device = call.device;
    let _ = context.memory.write_u32_be(
        call.port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET,
        call.original_clip,
    );
    for (i, byte) in call.drawing_state.iter().copied().enumerate() {
        let _ = context.memory.write_u8(call.port + 36 + i as u32, byte);
    }
    if call.temporary_clip != 0 {
        context
            .manager
            .dispose_native_handle(context.memory, call.temporary_clip);
        call.temporary_clip = 0;
    }
    ppc_restore_process_port_draw_state(
        context.memory,
        call.port,
        context.fore_color,
        context.back_color,
        context.pen_h,
        context.pen_v,
        context.text_mode,
        context.text_size,
    );
}

fn finish_button(context: &mut ThemeButtonContext<'_>, result: i16) -> PpcImportAction {
    restore_drawing_state(context);
    let call = context.state.calls.pop().unwrap();
    context.manager.dispose_native_ptr(call.scratch);
    context.cpu.lr = call.return_pc;
    context.cpu.gpr[2] = call.rtoc;
    PpcImportAction::Return(ppc_i16_result(result))
}

fn callback(context: &mut ThemeButtonContext<'_>, label: bool) -> Option<PpcImportAction> {
    let call = context.state.calls.last_mut()?;
    let procedure = if label { call.label } else { call.erase };
    let target = ppc_resolve_callback_target(context.memory, procedure, call.rtoc, None)?;
    if target.entry == 0
        || target.entry & 3 != 0
        || !ppc_memory_can_read_bytes(context.memory, target.entry, 4)
    {
        return None;
    }
    let bounds = if label { call.content } else { call.background };
    let clip = ppc_port_clip_storage(context.memory, call.port, Some(bounds))?;
    call.temporary_clip = context
        .manager
        .copy_bytes_to_new_native_handle(context.memory, &clip);
    if call.temporary_clip == 0 {
        return None;
    }
    context.memory.write_u32_be(
        call.port + PPC_CGRAF_PORT_CLIP_RGN_OFFSET,
        call.temporary_clip,
    )?;
    let palette = ppc_ui_theme(context.gworlds).provider().palette();
    let color = if matches!(call.state, 0 | 7 | 8) {
        Rgb8 {
            r: 128,
            g: 128,
            b: 128,
        }
    } else if call.state == 2 && matches!(call.kind, 0 | 5 | 11 | 12 | 15) {
        let brightness = u32::from(palette.selection.r) * 299
            + u32::from(palette.selection.g) * 587
            + u32::from(palette.selection.b) * 114;
        if brightness < 128_000 {
            Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            }
        } else {
            Rgb8 { r: 0, g: 0, b: 0 }
        }
    } else {
        palette.frame_dark
    };
    *context.fore_color = ppc_theme_rgb(color);
    let _ = ppc_write_rgb_color(
        context.memory,
        call.port + PPC_CGRAF_PORT_RGB_FG_COLOR_OFFSET,
        *context.fore_color,
    );
    *context.text_mode = 1; // srcOr: preserve the themed background behind glyphs.
    context
        .memory
        .write_u16_be(call.port + PPC_CGRAF_PORT_TX_MODE_OFFSET, 1)?;
    let color_device = u32::from(call.depth > 1);
    let args = if label {
        vec![
            call.scratch + 8,
            u32::from(call.kind),
            call.scratch + 16,
            call.user_data,
            call.depth,
            color_device,
        ]
    } else {
        vec![call.scratch, call.user_data, call.depth, color_device]
    };
    install_powerpc_call_arguments(context.cpu, context.memory, &args)?;
    GuestCallEffect::call_guest(
        GuestCallRequest::new(GuestCallTarget {
            isa: GuestIsa::PowerPc,
            entry: target.entry,
            rtoc: target.rtoc,
        }),
        GuestCallContinuation::to_powerpc(
            PPC_GUEST_CALL_RETURN_PC,
            call.import_pc,
            call.rtoc,
            PpcNativeReturnGpr3::Preserve,
        ),
    )
    .into_ppc_import_action()
}

fn advance_button(context: &mut ThemeButtonContext<'_>) -> PpcImportAction {
    loop {
        let call = context.state.calls.last_mut().unwrap();
        match call.stage {
            0 => {
                call.stage = 1;
                if call.erase != 0 {
                    return callback(context, false)
                        .unwrap_or_else(|| finish_button(context, PPC_PARAM_ERR));
                }
                let (top, left, bottom, right) = call.background;
                let width = i32::from(right) - i32::from(left);
                let height = i32::from(bottom) - i32::from(top);
                if i64::from(width) * i64::from(height) > 16_777_216 {
                    return finish_button(context, PPC_MEM_FULL_ERR);
                }
                let bitmap = ThemeBitmap::new(
                    width as u32,
                    height as u32,
                    ppc_ui_theme(context.gworlds)
                        .provider()
                        .palette()
                        .window_background,
                );
                dispatch_control::ppc_blit_theme_bitmap(
                    context.memory,
                    context.gworlds,
                    call.port,
                    top,
                    left,
                    &bitmap,
                );
            }
            1 => {
                call.stage = 2;
                let (top, left, bottom, right) = call.bounds;
                let Some((bitmap, pad, transparent)) = theme_button_bitmap(
                    ppc_ui_theme(context.gworlds),
                    call.kind,
                    call.state,
                    call.value,
                    call.adornment,
                    right.saturating_sub(left),
                    bottom.saturating_sub(top),
                ) else {
                    return finish_button(context, PPC_PARAM_ERR);
                };
                dispatch_control::ppc_blit_theme_bitmap_masked(
                    context.memory,
                    context.gworlds,
                    call.port,
                    top.saturating_sub(pad),
                    left.saturating_sub(pad),
                    &bitmap,
                    Some(transparent),
                );
                if call.label != 0
                    && !matches!(call.kind, 4 | 7)
                    && !(matches!(call.kind, 1 | 2 | 6 | 13 | 14) && call.adornment & 32 != 0)
                {
                    return callback(context, true)
                        .unwrap_or_else(|| finish_button(context, PPC_PARAM_ERR));
                }
            }
            _ => return finish_button(context, PPC_NO_ERR),
        }
    }
}
// Appearance.h defines kinds 0..15, values off/on/mixed, and adornment bitfields.
pub(super) fn theme_button_bitmap(
    theme_id: UiThemeId,
    kind: u16,
    state: u32,
    value: u16,
    adornment: u16,
    width: i16,
    height: i16,
) -> Option<(ThemeBitmap, i16, Rgb8)> {
    if !(0..=15).contains(&kind) || width <= 0 || height <= 0 {
        return None;
    }
    let theme = theme_id.provider();
    let palette = theme.palette();
    let default = adornment & 1 != 0;
    let focus = adornment & 4 != 0;
    let pad = if default || focus {
        theme.dialog_metrics().default_button_outline.max(2)
    } else {
        0
    };
    let total_width = i32::from(width) + 2 * i32::from(pad);
    let total_height = i32::from(height) + 2 * i32::from(pad);
    if total_width > i32::from(i16::MAX)
        || total_height > i32::from(i16::MAX)
        || i64::from(total_width) * i64::from(total_height) > 16_777_216
    {
        return None;
    }
    let transparent = Rgb8 { r: 1, g: 2, b: 3 };
    let mut bitmap = ThemeBitmap::new(total_width as u32, total_height as u32, transparent);
    let rect = ThemeRect {
        top: pad,
        left: pad,
        bottom: pad + height,
        right: pad + width,
    };
    let enabled = matches!(state, 1 | 2 | 3 | 6);
    let pressed = state == 2 || (kind == 7 && state == 3);
    let ink = if enabled {
        palette.frame_dark
    } else {
        Rgb8 {
            r: ((u16::from(palette.frame_dark.r) + u16::from(palette.window_background.r)) / 2)
                as u8,
            g: ((u16::from(palette.frame_dark.g) + u16::from(palette.window_background.g)) / 2)
                as u8,
            b: ((u16::from(palette.frame_dark.b) + u16::from(palette.window_background.b)) / 2)
                as u8,
        }
    };
    let mut ctx = ThemeDrawCtx::new(&mut bitmap);
    match kind {
        0 | 1 | 5 | 13 => {
            let control = match kind {
                0 => ControlKind::PushButton,
                1 | 13 => ControlKind::Checkbox,
                2 | 14 => ControlKind::RadioButton,
                _ => ControlKind::PopupButton,
            };
            let rect = if matches!(kind, 1 | 2 | 13 | 14) {
                let size = (if kind >= 13 { 10 } else { 13 }).min(width).min(height);
                let top = rect.top + (height - size) / 2;
                let left = if adornment & 16 != 0 {
                    rect.right - size
                } else {
                    rect.left
                };
                ThemeRect {
                    top,
                    left,
                    bottom: top + size,
                    right: left + size,
                }
            } else {
                rect
            };
            theme.draw_control(
                &mut ctx,
                ControlState {
                    kind: control,
                    rect,
                    enabled,
                    pressed,
                    selected: value == 1,
                    is_default: default,
                },
            );
            if value == 2 && matches!(kind, 1 | 13) {
                let center = rect.top + (rect.bottom - rect.top) / 2;
                ctx.fill_rect(
                    ThemeRect {
                        top: center,
                        bottom: center + 2,
                        left: rect.left + 3,
                        right: rect.right - 3,
                    },
                    ink,
                );
            }
        }
        2 | 14 => {
            let size = (if kind == 14 { 10 } else { 13 }).min(width).min(height);
            let left = if adornment & 16 != 0 {
                rect.right - size
            } else {
                rect.left
            };
            let top = rect.top + (height - size) / 2;
            for y in 0..size {
                for x in 0..size {
                    if !theme_button_contains(x, y, size, size, false) {
                        continue;
                    }
                    let inner = theme_button_contains(x - 1, y - 1, size - 2, size - 2, false);
                    let dot = value != 0
                        && theme_button_contains(x - 4, y - 4, size - 8, size - 8, false);
                    let color = if !inner || dot {
                        ink
                    } else if pressed {
                        palette.selection
                    } else {
                        palette.frame_light
                    };
                    ctx.fill_rect(
                        ThemeRect {
                            top: top + y,
                            left: left + x,
                            bottom: top + y + 1,
                            right: left + x + 1,
                        },
                        color,
                    );
                }
            }
        }
        6 => {
            let size = 13.min(width).min(height);
            let left = if adornment & 16 != 0 {
                rect.right - size
            } else {
                rect.left
            };
            let top = rect.top + (height - size) / 2;
            theme_button_triangle(
                &mut ctx,
                ThemeRect {
                    top,
                    left,
                    bottom: top + size,
                    right: left + size,
                },
                match value {
                    1 => 1,
                    2 => 2,
                    _ => 0,
                },
                ink,
            );
        }
        7 => {
            let middle = rect.top + (rect.bottom - rect.top) / 2;
            let up = ThemeRect {
                bottom: middle,
                ..rect
            };
            let down = ThemeRect {
                top: middle,
                ..rect
            };
            for (half, down_state, direction) in [(up, 2, 3), (down, 3, 1)] {
                theme_button_bevel(
                    &mut ctx,
                    half,
                    1,
                    state == down_state,
                    palette.window_background,
                    palette.frame_light,
                    ink,
                );
                theme_button_triangle(&mut ctx, half, direction, ink);
            }
        }
        11 | 12 | 15 => {
            // Use an inner contour to keep the border one pixel wide, including
            // for non-square round buttons and small rounded bevels.
            for y in 0..height {
                for x in 0..width {
                    if !theme_button_contains(x, y, width, height, kind == 15) {
                        continue;
                    }
                    let interior =
                        theme_button_contains(x - 1, y - 1, width - 2, height - 2, kind == 15);
                    let color = if !interior {
                        ink
                    } else if pressed {
                        palette.selection
                    } else {
                        palette.window_background
                    };
                    ctx.fill_rect(
                        ThemeRect {
                            top: pad + y,
                            left: pad + x,
                            bottom: pad + y + 1,
                            right: pad + x + 1,
                        },
                        color,
                    );
                }
            }
        }
        _ => {
            let bevel = match kind {
                8 => 1,
                9 => 3,
                _ => 2,
            };
            theme_button_bevel(
                &mut ctx,
                rect,
                bevel,
                pressed,
                palette.window_background,
                palette.frame_light,
                ink,
            );
            if kind == 4 {
                let direction = if adornment & (1 << 6) != 0 {
                    2
                } else if adornment & (1 << 9) != 0 {
                    3
                } else {
                    1
                };
                if adornment & (1 << 8) != 0 {
                    let middle = rect.top + (rect.bottom - rect.top) / 2;
                    theme_button_triangle(
                        &mut ctx,
                        ThemeRect {
                            bottom: middle,
                            ..rect
                        },
                        3,
                        ink,
                    );
                    theme_button_triangle(
                        &mut ctx,
                        ThemeRect {
                            top: middle,
                            ..rect
                        },
                        1,
                        ink,
                    );
                } else {
                    theme_button_triangle(&mut ctx, rect, direction, ink);
                }
            } else if kind == 10 {
                let r = ThemeRect {
                    left: (rect.right - 12).max(rect.left),
                    ..rect
                };
                theme_button_triangle(
                    &mut ctx,
                    r,
                    if adornment & (1 << 8) != 0 { 3 } else { 1 },
                    ink,
                );
            }
        }
    }
    if focus {
        ctx.frame_rect(
            ThemeRect {
                top: 0,
                left: 0,
                bottom: total_height as i16,
                right: total_width as i16,
            },
            palette.accent,
        );
    }
    Some((bitmap, pad, transparent))
}

fn theme_button_triangle(ctx: &mut ThemeDrawCtx<'_>, rect: ThemeRect, direction: u8, color: Rgb8) {
    let size = ((rect.right - rect.left).min(rect.bottom - rect.top) / 2).clamp(0, 7);
    let x = rect.left + (rect.right - rect.left) / 2;
    let y = rect.top + (rect.bottom - rect.top) / 2;
    for step in 0..size {
        let span = size - step;
        let (left, top, right, bottom) = match direction {
            0 => (
                x - size / 2 + step,
                y - span / 2,
                x - size / 2 + step + 1,
                y + (span + 1) / 2,
            ),
            2 => (
                x + size / 2 - step,
                y - span / 2,
                x + size / 2 - step + 1,
                y + (span + 1) / 2,
            ),
            3 => (
                x - span / 2,
                y + size / 2 - step,
                x + (span + 1) / 2,
                y + size / 2 - step + 1,
            ),
            _ => (
                x - span / 2,
                y - size / 2 + step,
                x + (span + 1) / 2,
                y - size / 2 + step + 1,
            ),
        };
        ctx.fill_rect(
            ThemeRect {
                top,
                left,
                bottom,
                right,
            },
            color,
        );
    }
}

fn theme_button_contains(x: i16, y: i16, width: i16, height: i16, rounded: bool) -> bool {
    if width <= 0 || height <= 0 || x < 0 || y < 0 || x >= width || y >= height {
        return false;
    }
    if rounded {
        let radius = 6.min(width / 2).min(height / 2);
        if radius == 0 {
            return true;
        }
        let corner_x = if x < radius {
            radius - 1 - x
        } else if x >= width - radius {
            x - (width - radius)
        } else {
            0
        };
        let corner_y = if y < radius {
            radius - 1 - y
        } else if y >= height - radius {
            y - (height - radius)
        } else {
            0
        };
        let dx = i32::from(2 * corner_x + 1);
        let dy = i32::from(2 * corner_y + 1);
        dx * dx + dy * dy <= i32::from(2 * radius).pow(2)
    } else {
        let rx = i64::from(width);
        let ry = i64::from(height);
        let dx = 2 * i64::from(x) + 1 - rx;
        let dy = 2 * i64::from(y) + 1 - ry;
        dx * dx * ry * ry + dy * dy * rx * rx <= rx * rx * ry * ry
    }
}

fn theme_button_bevel(
    ctx: &mut ThemeDrawCtx<'_>,
    rect: ThemeRect,
    bevel: i16,
    pressed: bool,
    background: Rgb8,
    light: Rgb8,
    ink: Rgb8,
) {
    ctx.fill_rect(rect, background);
    let (light, dark) = if pressed { (ink, light) } else { (light, ink) };
    for inset in 0..bevel {
        let r = ThemeRect {
            top: rect.top + inset,
            left: rect.left + inset,
            bottom: rect.bottom - inset,
            right: rect.right - inset,
        };
        if r.top >= r.bottom || r.left >= r.right {
            break;
        }
        ctx.fill_rect(
            ThemeRect {
                bottom: r.top + 1,
                ..r
            },
            light,
        );
        ctx.fill_rect(
            ThemeRect {
                right: r.left + 1,
                ..r
            },
            light,
        );
        ctx.fill_rect(
            ThemeRect {
                top: r.bottom - 1,
                ..r
            },
            dark,
        );
        ctx.fill_rect(
            ThemeRect {
                left: r.right - 1,
                ..r
            },
            dark,
        );
    }
}
