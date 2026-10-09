//! Translation between painted host glyph boundaries and guest insertion points.

#[derive(Clone, Debug)]
pub(crate) struct TextPointerMap {
    pub identity: (u32, u64),
    pub text: String,
    pub scroll_x: f32,
    pub guest_bounds: (i16, i16, i16, i16),
    pub positions: Vec<(f32, i16)>,
}

impl TextPointerMap {
    pub fn horizontal(&self, host_x: f32) -> Option<i16> {
        for pair in self.positions.windows(2) {
            if host_x < (pair[0].0 + pair[1].0) / 2. {
                return Some(pair[0].1);
            }
        }
        self.positions.last().map(|position| position.1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_glyph_midpoints_map_to_guest_positions_at_every_scale() {
        for scale in [0.75, 1., 1.5, 2.] {
            let map = TextPointerMap {
                identity: (1, 2), text: "iéW".into(), scroll_x: 0., guest_bounds: (10, 20, 30, 100),
                positions: [0., 3., 12., 25.].into_iter().zip([21, 26, 35, 47])
                    .map(|(x, guest)| (100. + x * scale, guest)).collect(),
            };
            for (x, expected) in [(-20., 21), (1., 21), (2., 26), (7., 26), (8., 35), (18., 35), (19., 47), (50., 47)] {
                assert_eq!(map.horizontal(100. + x * scale), Some(expected));
            }
        }
    }
}

/// Shape, scroll and paint a Roman single-line guest field from one layout.
/// Scrolling changes presentation only; pointer positions still come from the guest.
pub(crate) fn single_line(
    folder: &systemless::runner::StandardFileNewFolderSnapshot,
    identity: (u32, u64),
    output: std::rc::Rc<std::cell::RefCell<Option<TextPointerMap>>>,
    scale: f32,
    foreground: gpui_kit::Hsla,
    selection_color: gpui_kit::Hsla,
) -> impl gpui_kit::IntoElement {
    use gpui_kit::{prelude::*, *};
    let name = folder.name.clone();
    let bytes: Vec<usize> = name.char_indices().map(|(index, _)| index)
        .chain(std::iter::once(name.len())).collect();
    let start = bytes[folder.selection.0.min(bytes.len() - 1)];
    let end = bytes[folder.selection.1.min(bytes.len() - 1)].max(start);
    let visible = bytes[folder.visible_offset.min(bytes.len() - 1)];
    let guest_positions = folder.insertion_positions.clone();
    let caret_visible = folder.caret_visible;
    let guest_bounds = folder.layout.name;
    canvas(move |bounds, window, _| {
        let style = window.text_style();
        let display: SharedString = if name.is_empty() { " ".into() } else { name.clone().into() };
        let run = TextRun { len: display.len(), font: style.font(), color: foreground,
            background_color: None, underline: None, strikethrough: None };
        let line = window.text_system().shape_line(display, style.font_size.to_pixels(window.rem_size()), &[run], None);
        let caret_width = px(scale);
        let available = (bounds.size.width - caret_width).max(px(0.));
        let target = line.x_for_index(visible);
        let max_scroll = (line.x_for_index(*bytes.last().unwrap()) - available).max(px(0.));
        let old = output.borrow().as_ref().filter(|map| map.identity == identity).map_or(px(0.), |map| px(map.scroll_x));
        // Text 1993, TESelView: retain the existing origin while the target is
        // visible, scroll only far enough to reveal it, and clamp after deletion.
        let scroll = old.min(target).max(target - available).clamp(px(0.), max_scroll);
        let height = px(16. * scale);
        let origin = point(bounds.left() - scroll, bounds.top() + (bounds.size.height - height) / 2.);
        let positions = bytes.iter().zip(&guest_positions)
            .map(|(index, guest)| (f32::from(origin.x + line.x_for_index(*index)), *guest)).collect();
        *output.borrow_mut() = Some(TextPointerMap { identity, text: name, guest_bounds,
            scroll_x: f32::from(scroll), positions });
        (line, origin, height, caret_width)
    }, move |_, (line, origin, height, caret_width), window, cx| {
        let left = origin.x + line.x_for_index(start);
        let right = origin.x + line.x_for_index(end);
        if start != end {
            window.paint_quad(fill(Bounds::new(point(left, origin.y), size(right - left, height)), selection_color));
        }
        let _ = line.paint(origin, height, TextAlign::Left, None, window, cx);
        if start == end && caret_visible {
            window.paint_quad(fill(Bounds::new(point(left, origin.y), size(caret_width, height)), foreground));
        }
    }).size_full()
}
