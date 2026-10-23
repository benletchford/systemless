//! Transactional range editing: commit pointer drags on release, keyboard edits immediately.
use crate::egui;

#[derive(Clone, Copy)]
struct Draft {
    source: i16,
    value: i16,
    min: i16,
    max: i16,
    dragging: bool,
    cancelled: bool,
}

pub fn show(
    ui: &mut egui::Ui,
    label: &str,
    source: i16,
    min: i16,
    max: i16,
    request_focus: bool,
) -> Option<i16> {
    if min > max || source < min || source > max {
        ui.add_enabled(false, egui::Label::new(label));
        return None;
    }
    let id = ui.id().with("range-draft");
    let mut draft = ui.data(|data| data.get_temp::<Draft>(id)).unwrap_or(Draft {
        source,
        value: source,
        min,
        max,
        dragging: false,
        cancelled: false,
    });
    if (draft.source, draft.min, draft.max) != (source, min, max) {
        draft.cancelled |= draft.dragging;
        draft.source = source;
        draft.min = min;
        draft.max = max;
        if !draft.dragging {
            draft.value = source;
        }
    }
    if draft.dragging
        && (!ui.is_enabled()
            || ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)))
    {
        draft.cancelled = true;
    }
    let was_dragging = draft.dragging;
    let response = ui
        .scope(|ui| {
            let accent = ui.visuals().selection.stroke.color;
            let rail = ui.visuals().widgets.inactive.bg_stroke.color;
            let width = (ui.available_width() - 48.).clamp(40., 300.);
            let style = ui.style_mut();
            style.spacing.slider_width = width;
            style.spacing.slider_rail_height = 6.;
            style.spacing.interact_size.y = 22.;
            style.visuals.selection.bg_fill = accent;
            style.visuals.widgets.inactive.bg_fill = rail;
            style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(2., accent);
            style.visuals.widgets.hovered.bg_fill = accent;
            style.visuals.widgets.hovered.fg_stroke = egui::Stroke::new(2., accent);
            style.visuals.widgets.active.bg_fill = accent;
            style.visuals.widgets.active.fg_stroke = egui::Stroke::new(2., accent);
            let label = (!label.is_empty()).then(|| ui.label(label));
            let response = ui
                .horizontal(|ui| {
                    let response = ui.add_enabled(
                        !draft.cancelled && min < max,
                        egui::Slider::new(&mut draft.value, min..=max)
                            .show_value(false)
                            .trailing_fill(true)
                            .handle_shape(egui::style::HandleShape::Circle),
                    );
                    ui.weak(draft.value.to_string());
                    response
                })
                .inner;
            if let Some(label) = label {
                response.labelled_by(label.id)
            } else {
                response
            }
        })
        .inner;
    if ui.is_enabled() && (request_focus || response.is_pointer_button_down_on()) {
        response.request_focus();
    }
    let down = ui.input(|input| input.pointer.primary_down());
    draft.dragging = response.is_pointer_button_down_on() || was_dragging && down;
    let commit = !draft.cancelled
        && ui.is_enabled()
        && (response.drag_stopped()
            || response.clicked()
            || response.changed() && !draft.dragging && !was_dragging)
        && (draft.value != source || response.drag_stopped() || response.clicked());
    let result = commit.then_some(draft.value);
    if !draft.dragging {
        // The caller owns the canonical value; do not retain an unacknowledged
        // draft after this transaction ends.
        draft.value = source;
        draft.cancelled = false;
    }
    ui.data_mut(|data| data.insert_temp(id, draft));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rail(shape: &egui::Shape) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Rect(rect) if rect.rect.width() > 50. && rect.rect.height() <= 8. => {
                Some(rect.rect)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(rail),
            _ => None,
        }
    }
    fn frame(
        ctx: &egui::Context,
        source: i16,
        enabled: bool,
        visible: bool,
        events: Vec<egui::Event>,
    ) -> (Option<i16>, Option<egui::Rect>) {
        let mut result = None;
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 400.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                if visible {
                    result = ui
                        .add_enabled_ui(enabled, |ui| show(ui, "Level", source, -100, 100, false))
                        .inner;
                }
            },
        );
        let rect = output.shapes.iter().find_map(|shape| rail(&shape.shape));
        output.textures_delta.clear();
        (result, rect)
    }
    fn pointer(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            },
        ]
    }
    #[test]
    fn drags_commit_only_on_release_and_keyboard_changes_commit_immediately() {
        let ctx = egui::Context::default();
        crate::Theme::default().apply(&ctx);
        let (value, rect) = frame(&ctx, 0, true, true, Vec::new());
        assert_eq!(value, None);
        let rect = rect.unwrap();
        let start = rect.center();
        let end = egui::pos2(rect.right() - 10., start.y);
        assert_eq!(frame(&ctx, 0, true, true, pointer(start, true)).0, None);
        assert_eq!(
            frame(&ctx, 0, true, true, vec![egui::Event::PointerMoved(end)]).0,
            None
        );
        let value = frame(&ctx, 0, true, true, pointer(end, false)).0.unwrap();
        assert!((50..=100).contains(&value));
        assert_eq!(frame(&ctx, value, true, true, Vec::new()).0, None);
        let next = frame(
            &ctx,
            value,
            true,
            true,
            vec![egui::Event::Key {
                key: egui::Key::ArrowLeft,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        )
        .0
        .unwrap();
        assert!(next < value && next >= -100);
    }
    #[test]
    fn source_conflicts_disable_escape_and_hidden_parents_cancel_drags() {
        for mode in 0..4 {
            let ctx = egui::Context::default();
            crate::Theme::default().apply(&ctx);
            let rect = frame(&ctx, 0, true, true, Vec::new()).1.unwrap();
            let end = egui::pos2(rect.right() - 10., rect.center().y);
            frame(&ctx, 0, true, true, pointer(rect.center(), true));
            assert_eq!(
                frame(&ctx, 0, true, true, vec![egui::Event::PointerMoved(end)]).0,
                None
            );
            let source = if mode == 0 { 50 } else { 0 };
            let events = if mode == 2 {
                vec![egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]
            } else {
                Vec::new()
            };
            assert_eq!(frame(&ctx, source, mode != 1, mode != 3, events).0, None);
            assert_eq!(
                frame(&ctx, source, true, mode != 3, pointer(end, false)).0,
                None
            );
            assert_eq!(frame(&ctx, source, true, true, Vec::new()).0, None);
        }
    }
}
