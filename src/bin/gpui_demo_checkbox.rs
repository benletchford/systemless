//! Guest-controlled checkbox presentation with independent value and tracking state.

use gpui_kit::{
    base::{Checkbox, CheckboxIndicator},
    component::{ActiveTheme, IconName, IconNamed},
    prelude::*,
    *,
};

pub fn guest_checkbox(
    id: String,
    label: String,
    checked: bool,
    enabled: bool,
    pressed: bool,
    scale: f32,
    cx: &App,
) -> Checkbox {
    let theme = cx.theme();
    let pressed = enabled && pressed;
    let border = if pressed {
        theme.foreground
    } else if checked {
        theme.primary
    } else {
        theme.input
    };
    let background = if checked && pressed {
        theme.primary_active
    } else if checked {
        theme.primary
    } else if pressed {
        theme.secondary
    } else {
        theme.input_background()
    };
    // Value and tracking are independent: contrlHilite=11 does not toggle contrlValue.
    // Macintosh Toolbox Essentials (1992), pp. 5-89, 5-95.
    Checkbox::new(id)
        .checked(checked)
        .disabled(!enabled)
        .accessibility_label(label.clone())
        .tab_stop(false)
        .flex()
        .items_start()
        .gap_x(px(4. * scale))
        .text_size(px(12. * scale))
        .text_color(if enabled {
            theme.foreground
        } else {
            theme.muted_foreground
        })
        .w_full()
        .h_full()
        .child(
            CheckboxIndicator::new()
                .checked(checked)
                .disabled(!enabled)
                .relative()
                .size(px(14. * scale))
                .mt(px(1.75 * scale))
                .flex_shrink_0()
                .border_1()
                .when(pressed, |indicator| indicator.border_2())
                .border_color(if enabled { border } else { border.opacity(0.5) })
                .bg(if checked && !enabled {
                    background.opacity(0.5)
                } else {
                    background
                })
                .rounded(theme.radius.min(px(4. * scale)))
                .child(
                    svg()
                        .absolute()
                        .left(px(scale))
                        .top(px(scale))
                        .size(px(10. * scale))
                        .text_color(if enabled {
                            theme.primary_foreground
                        } else {
                            theme.primary_foreground.opacity(0.5)
                        })
                        .when(checked, |mark| mark.path(IconName::Check.path())),
                ),
        )
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .line_height(relative(1.25))
                .child(label),
        )
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}
