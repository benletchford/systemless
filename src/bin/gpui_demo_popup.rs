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
    // Standard MDEF reserves scrolling-arrow slots. Draw intersecting rows
    // beneath the arrow overlays so their exposed portions remain visible,
    // matching the guest hit regions. Inside Macintosh V, V-248--V-249.
    let (scroll_up, scroll_down) = popup.scroll_indicators();
    let height = i32::from(bottom) - i32::from(top);
    let visible_top = if scroll_up { 16 } else { 0 };
    let visible_bottom = height - if scroll_down { 16 } else { 0 };
    let mut offset = i32::from(popup.content_top) - i32::from(top);
    for (item, height) in popup.menu.items.iter().zip(&popup.row_heights) {
        if offset + i32::from(*height) <= visible_top || offset >= visible_bottom {
            offset += i32::from(*height);
            continue;
        }
        let selected = item.number == popup.highlighted_item;
        let enabled = popup.menu.enabled && item.enabled;
        let mut row = div()
            .absolute()
            .left_0()
            .top(unit(offset))
            .w(unit((i32::from(right) - i32::from(left)).max(0)))
            .h(unit(i32::from(*height)))
            .flex()
            .items_center();
        if selected {
            row = row.bg(cx.theme().selection);
        }
        if item.separator {
            row = row.child(div().w_full().h(unit(1)).bg(cx.theme().border));
        } else {
            row = row.overflow_hidden().child(super::text::classic_popup_row(
                popup, item, *height, scale,
                if enabled { cx.theme().foreground } else { cx.theme().muted_foreground },
            ));
        }
        pane = pane.child(row);
        offset += i32::from(*height);
    }
    for (visible, y, up) in [(scroll_up, 0, true), (scroll_down, height - 16, false)] {
        if visible {
            pane = pane.child(
                div()
                    .absolute()
                    .left_0()
                    .top(unit(y))
                    .w_full()
                    .h(unit(16))
                    .overflow_hidden()
                    .bg(cx.theme().background)
                    .child(scroll_indicator(up, scale, cx.theme().foreground)),
            );
        }
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

fn scroll_indicator(up: bool, scale: f32, foreground: Hsla) -> impl IntoElement {
    let pixels = systemless::menu_model::standard_scroll_indicator_pixels(up);
    canvas(move |bounds, _, _| bounds, move |_, bounds, window, _| {
        let center = (f32::from(bounds.size.width) / scale).round() as i32 / 2;
        for &(x, y) in &pixels {
            window.paint_quad(fill(Bounds::new(
                point(bounds.left() + px((center + i32::from(x)) as f32 * scale),
                    bounds.top() + px(f32::from(y) * scale)),
                size(px(scale), px(scale))), foreground));
        }
    }).size_full()
}

#[cfg(test)]
mod tests {
    use super::owned_rects;
    use systemless::menu_model::{GuestMenu, GuestPopupSnapshot};

    #[test]
    fn popup_scroll_indicators_follow_guest_content_origin() {
        let mut popup = GuestPopupSnapshot {
            font: Default::default(),
            menu: GuestMenu {
                guest_id: 1,
                generation: 1,
                id: 143,
                title: String::new(),
                enabled: true,
                standard_definition: true,
                hierarchical: true,
                visible_in_menu_bar: false,
                items: Vec::new(),
            },
            bounds: (20, 30, 100, 200),
            content_top: 20,
            row_heights: vec![16; 10],
            highlighted_item: 0,
        };
        assert_eq!(popup.scroll_indicators(), (false, true));
        popup.content_top = -12;
        assert_eq!(popup.scroll_indicators(), (true, true));
        popup.content_top = -60;
        assert_eq!(popup.scroll_indicators(), (true, false));
        popup.row_heights = vec![16, 6, 16];
        popup.content_top = 20;
        assert_eq!(popup.scroll_indicators(), (false, false));
    }

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
