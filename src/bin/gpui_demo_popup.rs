//! Presentation of guest-tracked standard popup rows. Input bubbles to the scene.
use super::frames::Rect;
use gpui_kit::{component::ActiveTheme, prelude::*, *};
use systemless::menu_model::GuestPopupSnapshot;

/// Standard MDEF owns its pane and two one-pixel shadow strips.
/// Macintosh Toolbox Essentials (1992), pp. 3-120, 3-122--3-123.
/// Matches the runtime's StandardMenuChrome PopUp plan.
pub fn owned_rects(bounds: (i16, i16, i16, i16)) -> Vec<Rect> {
    let pane = Rect::from(bounds);
    if pane.width() <= 0 || pane.height() <= 0 {
        return Vec::new();
    }
    let mut rects = vec![pane];
    if pane.top + 3 <= pane.bottom {
        rects.push(Rect {
            top: pane.top + 3,
            left: pane.right,
            bottom: pane.bottom + 1,
            right: pane.right + 1,
        });
    }
    if pane.left + 3 <= pane.right {
        rects.push(Rect {
            top: pane.bottom,
            left: pane.left + 3,
            bottom: pane.bottom + 1,
            right: pane.right + 1,
        });
    }
    rects
}

pub fn popup(popup: &GuestPopupSnapshot, scale: f32, cx: &App) -> Div {
    let (top, left, bottom, right) = popup.bounds;
    let unit = |value: i32| px(value as f32 * scale);
    let mut pane = div()
        .absolute()
        .top_0()
        .left_0()
        .w(unit(i32::from(right) - i32::from(left)))
        .h(unit(i32::from(bottom) - i32::from(top)))
        .overflow_hidden()
        .bg(cx.theme().background);
    let mut offset = i32::from(popup.content_top) - i32::from(top);
    for (item, height) in popup.menu.items.iter().zip(&popup.row_heights) {
        let selected = item.number == popup.highlighted_item;
        let enabled = popup.menu.enabled && item.enabled;
        let mut row = div()
            .absolute()
            .left(unit(1))
            .top(unit(offset))
            .w(unit((i32::from(right) - i32::from(left) - 2).max(0)))
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
    let width = i32::from(right) - i32::from(left);
    let height = i32::from(bottom) - i32::from(top);
    // Paint the border last so selected rows cannot erase it.
    for (y, x, h, w) in [
        (0, 0, 1, width),
        (height - 1, 0, 1, width),
        (0, 0, height, 1),
        (0, width - 1, height, 1),
    ] {
        pane = pane.child(
            div()
                .absolute()
                .top(unit(y))
                .left(unit(x))
                .h(unit(h))
                .w(unit(w))
                .bg(cx.theme().border),
        );
    }
    let mut overlay = div()
        .absolute()
        .top(unit(i32::from(top)))
        .left(unit(i32::from(left)))
        .w(unit(width + 1))
        .h(unit(height + 1));
    for shadow in owned_rects(popup.bounds).into_iter().skip(1) {
        overlay = overlay.child(
            div()
                .absolute()
                .top(unit(shadow.top - i32::from(top)))
                .left(unit(shadow.left - i32::from(left)))
                .w(unit(shadow.width()))
                .h(unit(shadow.height()))
                .bg(cx.theme().border),
        );
    }
    overlay.child(pane)
}

#[cfg(test)]
mod tests {
    use super::owned_rects;
    #[test]
    fn popup_owned_pixels_preserve_unpainted_shadow_corners() {
        let rects = owned_rects((10, 20, 50, 100));
        let owns = |y, x| {
            rects
                .iter()
                .any(|r| y >= r.top && y < r.bottom && x >= r.left && x < r.right)
        };
        assert!(owns(10, 20));
        assert!(owns(49, 99));
        assert!(!owns(12, 100));
        assert!(owns(13, 100));
        assert!(!owns(50, 22));
        assert!(owns(50, 23));
        assert!(owns(50, 100));
        assert!(!owns(51, 100));
        assert!(owned_rects((10, 20, 10, 100)).is_empty());
    }
}
