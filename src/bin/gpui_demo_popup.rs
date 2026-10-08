//! Presentation of guest-tracked standard popup rows. Input bubbles to the scene.
use gpui_kit::{component::ActiveTheme, prelude::*, *};
use systemless::menu_model::GuestPopupSnapshot;

pub fn popup(popup: &GuestPopupSnapshot, scale: f32, cx: &App) -> Div {
    let (top, left, bottom, right) = popup.bounds;
    let unit = |value: i32| px(value as f32 * scale);
    let mut pane = div()
        .absolute()
        .top(unit(i32::from(top)))
        .left(unit(i32::from(left)))
        .w(unit(i32::from(right) - i32::from(left)))
        .h(unit(i32::from(bottom) - i32::from(top)))
        .overflow_hidden()
        .bg(cx.theme().background)
        .border_1()
        .border_color(cx.theme().border);
    let mut offset = i32::from(popup.content_top) - i32::from(top);
    for (item, height) in popup.menu.items.iter().zip(&popup.row_heights) {
        let selected = item.number == popup.highlighted_item;
        let enabled = popup.menu.enabled && item.enabled;
        let mut row = div()
            .absolute()
            .left_0()
            .top(unit(offset))
            .w_full()
            .h(unit(i32::from(*height)))
            .flex()
            .items_center()
            .text_size(unit(12))
            .text_color(if enabled {
                cx.theme().foreground
            } else {
                cx.theme().muted_foreground
            });
        if selected {
            row = row.bg(cx.theme().selection);
        }
        if item.separator {
            row = row.child(div().w_full().h(unit(1)).bg(cx.theme().border));
        } else {
            row = row
                .child(div().w(unit(16)).flex_shrink_0().child(if item.checked {
                    "✓"
                } else {
                    ""
                }))
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(item.text.clone()),
                );
            if let Some(key) = item.key_equivalent {
                row = row.child(div().px_1().child(format!("⌘{key}")));
            }
            if item.submenu_id.is_some() {
                row = row.child(div().px_1().child("▸"));
            }
        }
        pane = pane.child(row);
        offset += i32::from(*height);
    }
    pane
}
