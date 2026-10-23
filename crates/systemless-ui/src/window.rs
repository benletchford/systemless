//! Application-owned nonmodal surfaces. Window IDs and state outlive snapshots.
use crate::egui;

/// Retain this alongside a logical window, including while it is hidden or the
/// renderer is recreated. Retire it together with the logical window identity.
#[derive(Clone, Debug, Default)]
pub struct WindowState {
    rect: Option<egui::Rect>,
    raise: bool,
}
impl WindowState {
    pub fn rect(&self) -> Option<egui::Rect> {
        self.rect
    }

    /// Raise on the next presentation, for example after application activation.
    pub fn request_front(&mut self) {
        self.raise = true;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct WindowOptions {
    /// Initial outer geometry. Later snapshots do not overwrite host movement.
    pub initial_rect: egui::Rect,
    pub enabled: bool,
    pub resizable: bool,
    /// Closing reports an intent; the application owns the window's lifetime.
    pub closable: bool,
}

pub struct WindowResponse<R> {
    pub inner: Option<R>,
    pub response: egui::Response,
    /// A pointer press reached this surface, including its title bar. The host
    /// should activate its corresponding application window before item actions.
    pub activated: bool,
    pub close_requested: bool,
}

/// Show an independent window without a modal backdrop. egui owns dragging,
/// edge resizing, hit testing and stacking. Keep the same ID for its lifetime.
/// The host chooses which window accepts form shortcuts; use
/// [`crate::dialog::show_with_shortcuts`] when rendering multiple forms.
pub fn show<R>(
    context: &egui::Context,
    id: egui::Id,
    title: &str,
    state: &mut WindowState,
    options: WindowOptions,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> WindowResponse<R> {
    let layer = egui::LayerId::new(egui::Order::Middle, id);
    if state.raise && options.enabled {
        context.move_to_top(layer);
        state.raise = false;
    }
    let mut open = true;
    let mut window = egui::Window::new(title)
        .id(id)
        .default_rect(state.rect.unwrap_or(options.initial_rect))
        .enabled(options.enabled)
        .resizable(options.resizable)
        .vscroll(true)
        .collapsible(false)
        .constrain(true);
    if options.closable {
        window = window.open(&mut open);
    }
    let response = window.show(context, content).expect("window starts open");
    state.rect = Some(response.response.rect);
    let activated = options.enabled
        && context.input(|input| input.pointer.any_pressed())
        && context
            .pointer_interact_pos()
            .is_some_and(|pos| context.layer_id_at(pos) == Some(response.response.layer_id));
    WindowResponse {
        inner: response.inner,
        response: response.response,
        activated,
        close_requested: !open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(
        ctx: &egui::Context,
        states: &mut [WindowState; 2],
        events: Vec<egui::Event>,
        enabled: bool,
        modal: bool,
    ) -> [WindowResponse<()>; 2] {
        let mut responses = Vec::new();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900., 700.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                responses.clear();
                for (index, state) in states.iter_mut().enumerate() {
                    responses.push(show(
                        ui.ctx(),
                        egui::Id::new(("nonmodal", index)),
                        "Inspector",
                        state,
                        WindowOptions {
                            initial_rect: egui::Rect::from_min_size(
                                egui::pos2(60. + index as f32 * 80., 60.),
                                egui::vec2(320., 240.),
                            ),
                            enabled,
                            resizable: true,
                            closable: true,
                        },
                        |ui| {
                            ui.label("Independent application window");
                        },
                    ));
                }
                if modal {
                    egui::Modal::new(egui::Id::new("blocking")).show(ui.ctx(), |ui| {
                        ui.label("Modal child");
                    });
                }
            },
        );
        output.textures_delta.clear();
        responses.try_into().ok().unwrap()
    }

    fn press(pos: egui::Pos2) -> Vec<egui::Event> {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]
    }

    #[test]
    fn overlapping_windows_activate_only_the_topmost_surface() {
        let ctx = egui::Context::default();
        let mut states = Default::default();
        frame(&ctx, &mut states, vec![], true, false);
        let windows = frame(&ctx, &mut states, vec![], true, false);
        let overlap = windows[0]
            .response
            .rect
            .intersect(windows[1].response.rect)
            .center();
        let windows = frame(&ctx, &mut states, press(overlap), true, false);
        assert_eq!(windows.iter().filter(|window| window.activated).count(), 1);
        assert!(windows[1].activated);
    }

    #[test]
    fn explicit_raise_changes_the_hit_target_without_activating_by_itself() {
        let ctx = egui::Context::default();
        let mut states = Default::default();
        frame(&ctx, &mut states, vec![], true, false);
        let windows = frame(&ctx, &mut states, vec![], true, false);
        let overlap = windows[0]
            .response
            .rect
            .intersect(windows[1].response.rect)
            .center();
        states[0].request_front();
        let windows = frame(&ctx, &mut states, vec![], true, false);
        assert!(windows.iter().all(|window| !window.activated));
        let windows = frame(&ctx, &mut states, press(overlap), true, false);
        assert!(windows[0].activated);
        assert!(!windows[1].activated);
    }

    #[test]
    fn dragging_keeps_other_window_geometry_and_restores_the_moved_position() {
        let ctx = egui::Context::default();
        let mut states = Default::default();
        frame(&ctx, &mut states, vec![], true, false);
        let before = frame(&ctx, &mut states, vec![], true, false);
        let start = before[0].response.rect.left_top() + egui::vec2(30., 12.);
        frame(&ctx, &mut states, press(start), true, false);
        let end = start + egui::vec2(45., 50.);
        frame(
            &ctx,
            &mut states,
            vec![egui::Event::PointerMoved(end)],
            true,
            false,
        );
        frame(
            &ctx,
            &mut states,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
            true,
            false,
        );
        let moved = frame(&ctx, &mut states, vec![], true, false);
        assert!(moved[0].response.rect.left() > before[0].response.rect.left() + 20.);
        assert_eq!(moved[1].response.rect, before[1].response.rect);
        let fresh = egui::Context::default();
        frame(&fresh, &mut states, vec![], true, false);
        let restored = frame(&fresh, &mut states, vec![], true, false);
        assert_eq!(restored[0].response.rect, moved[0].response.rect);
    }

    #[test]
    fn edge_resize_preserves_independent_window_geometry() {
        let ctx = egui::Context::default();
        let mut states = Default::default();
        frame(&ctx, &mut states, vec![], true, false);
        let before = frame(&ctx, &mut states, vec![], true, false);
        let start = before[1].response.rect.right_bottom() - egui::vec2(2., 2.);
        frame(&ctx, &mut states, press(start), true, false);
        let end = start + egui::vec2(80., 60.);
        frame(
            &ctx,
            &mut states,
            vec![egui::Event::PointerMoved(end)],
            true,
            false,
        );
        frame(
            &ctx,
            &mut states,
            vec![egui::Event::PointerButton {
                pos: end,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default(),
            }],
            true,
            false,
        );
        let resized = frame(&ctx, &mut states, vec![], true, false);
        assert!(resized[1].response.rect.width() > before[1].response.rect.width() + 40.);
        assert!(
            resized[1].response.rect.height() > before[1].response.rect.height() + 20.,
            "before {:?}, after {:?}",
            before[1].response.rect,
            resized[1].response.rect
        );
        assert_eq!(resized[0].response.rect, before[0].response.rect);
        let fresh = egui::Context::default();
        frame(&fresh, &mut states, vec![], true, false);
        let restored = frame(&fresh, &mut states, vec![], true, false);
        assert_eq!(restored[1].response.rect, resized[1].response.rect);
    }

    #[test]
    fn disabled_or_modal_blocked_windows_do_not_activate() {
        for (enabled, modal) in [(false, false), (true, true)] {
            let ctx = egui::Context::default();
            let mut states = Default::default();
            frame(&ctx, &mut states, vec![], enabled, modal);
            let windows = frame(&ctx, &mut states, vec![], enabled, modal);
            let point = windows[1].response.rect.left_top() + egui::vec2(30., 12.);
            let windows = frame(&ctx, &mut states, press(point), enabled, modal);
            assert!(windows.iter().all(|window| !window.activated));
        }
    }

    #[test]
    fn geometry_survives_context_recreation_and_independent_windows_keep_their_ids() {
        let mut states = Default::default();
        let ctx = egui::Context::default();
        frame(&ctx, &mut states, vec![], true, false);
        let before = frame(&ctx, &mut states, vec![], true, false);
        let fresh = egui::Context::default();
        frame(&fresh, &mut states, vec![], true, false);
        let after = frame(&fresh, &mut states, vec![], true, false);
        for index in 0..2 {
            assert_eq!(
                before[index].response.layer_id,
                after[index].response.layer_id
            );
            assert_eq!(before[index].response.rect, after[index].response.rect);
        }
        assert_ne!(after[0].response.layer_id, after[1].response.layer_id);
    }
}
