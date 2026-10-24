use systemless_ui::{dialog, egui};

#[test]
fn movable_modal_preserves_position_controls_and_backdrop_ownership() {
    let ctx = egui::Context::default();
    let id = egui::Id::new("movable-preferences");
    let mut state = dialog::ModalState::default();
    let clamped = {
        let mut time = 0.;
        let mut frame = |events| {
            time += 0.2;
            let mut result = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let background = ui.button("Background action");
                    let response = dialog::show_modal(&ctx, id, true, &mut state, |ui| {
                        ui.set_min_width(260.);
                        ui.heading("Preferences");
                        ui.label("A movable modal dialog");
                        let button = ui.button("Save");
                        (button.rect, button.clicked())
                    });
                    result = Some((
                        response.response.rect,
                        response.inner,
                        background.rect,
                        background.clicked(),
                    ));
                },
            );
            output.textures_delta.clear();
            result.unwrap()
        };
        let button = |pos, pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        for _ in 0..3 {
            frame(Vec::new());
        }
        let initial = frame(Vec::new()).0;
        assert!(
            initial.width() < 400.,
            "drag grip must not expand the form width"
        );
        let start = initial.center_top() + egui::vec2(0., 14.);
        frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        let end = start + egui::vec2(110., 65.);
        frame(vec![egui::Event::PointerMoved(end)]);
        frame(vec![button(end, false)]);
        let moved = frame(Vec::new());
        assert!(
            (moved.0.center().x - initial.center().x - 110.).abs() < 2.,
            "initial={initial:?}, moved={:?}",
            moved.0
        );
        assert!((moved.0.center().y - initial.center().y - 65.).abs() < 2.);
        assert_eq!(
            frame(Vec::new()).0,
            moved.0,
            "position must survive idle frames"
        );
        let save = moved.1 .0.center();
        frame(vec![egui::Event::PointerMoved(save), button(save, true)]);
        assert!(
            frame(vec![button(save, false)]).1 .1,
            "movement must not consume button clicks"
        );
        let background = moved.2.center();
        frame(vec![
            egui::Event::PointerMoved(background),
            button(background, true),
        ]);
        assert!(
            !frame(vec![button(background, false)]).3,
            "modal backdrop must block other UI"
        );
        let start = moved.0.center_top() + egui::vec2(0., 14.);
        frame(vec![egui::Event::PointerMoved(start), button(start, true)]);
        let far = start + egui::vec2(2000., 2000.);
        frame(vec![egui::Event::PointerMoved(far)]);
        frame(vec![button(far, false)]);
        let clamped = frame(Vec::new()).0;
        assert!(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800., 600.))
                .contains_rect(clamped)
        );
        clamped
    };
    let recreated = egui::Context::default();
    let mut restored = None;
    for _ in 0..3 {
        let mut output = recreated.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                ..Default::default()
            },
            |_ui| {
                restored = Some(
                    dialog::show_modal(&recreated, id, true, &mut state, |ui| {
                        ui.set_min_width(260.);
                        ui.heading("Preferences");
                        ui.label("A movable modal dialog");
                        let _ = ui.button("Save");
                    })
                    .response
                    .rect,
                );
            },
        );
        output.textures_delta.clear();
    }
    assert!(
        (restored.unwrap().center() - clamped.center()).length() < 2.,
        "renderer recreation must preserve dialog position"
    );
}
