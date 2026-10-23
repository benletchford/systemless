use systemless_ui::{dialog, egui, Appearance, Theme};

#[test]
fn accessible_form_exposes_controls_and_accepts_click_actions() {
    use egui::accesskit::{Action, ActionRequest, Role};
    let context = egui::Context::default();
    let theme = Theme::new(Appearance::Dark);
    theme.apply(&context);
    context.enable_accesskit();
    let fields = [dialog::Field {
        id: 1,
        text: "Enable sound",
        enabled: true,
        kind: dialog::FieldKind::Checkbox(false),
    }];
    let buttons = [dialog::Button {
        id: 2,
        text: "Save",
        enabled: true,
    }];
    let mut result = None;
    let mut draw = |events| {
        context.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                result = Some(dialog::show(
                    ui,
                    &theme,
                    dialog::Form {
                        title: "Preferences",
                        fields: &fields,
                        buttons: &buttons,
                        default: 2,
                        cancel: 0,
                    },
                    &mut [],
                ));
            },
        )
    };
    let mut output = draw(vec![]);
    output.textures_delta.clear();
    let update = output.platform_output.accesskit_update.take().unwrap();
    assert!(update
        .nodes
        .iter()
        .any(|(_, node)| node.role() == Role::CheckBox && node.label() == Some("Enable sound")));
    let save = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Button && node.label() == Some("Save"))
        .unwrap();
    assert!(save.1.supports_action(Action::Click));
    let mut output = draw(vec![egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::Click,
        target_node: save.0,
        target_tree: egui::accesskit::TreeId::ROOT,
        data: None,
    })]);
    output.textures_delta.clear();
    assert!(output.platform_output.accesskit_update.is_some());
    assert_eq!(result.unwrap().action, Some(2));
    context.disable_accesskit();
    let mut output = context.run_ui(Default::default(), |ui| {
        ui.label("Inactive accessibility");
    });
    output.textures_delta.clear();
    assert!(output.platform_output.accesskit_update.is_none());
}

#[test]
fn accessible_range_commits_exact_values_and_respects_disabled_state() {
    use egui::accesskit::{Action, ActionData, ActionRequest, Role, TreeId};
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut chosen = None;
    let mut draw = |enabled, events| {
        let mut output = context.run_ui(
            egui::RawInput {
                events,
                ..Default::default()
            },
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    chosen = systemless_ui::range::show(ui, "Volume", 0, -100, 100, false);
                });
            },
        );
        output.textures_delta.clear();
        output
    };
    let output = draw(true, vec![]);
    let update = output.platform_output.accesskit_update.unwrap();
    let slider = update
        .nodes
        .iter()
        .find(|(_, node)| node.role() == Role::Slider)
        .unwrap();
    assert!(slider.1.supports_action(Action::SetValue));
    let action = || {
        egui::Event::AccessKitActionRequest(ActionRequest {
            action: Action::SetValue,
            target_tree: TreeId::ROOT,
            target_node: slider.0,
            data: Some(ActionData::NumericValue(75.)),
        })
    };
    draw(true, vec![action()]);
    assert_eq!(chosen, Some(75));
    let mut output = context.run_ui(
        egui::RawInput {
            events: vec![action()],
            ..Default::default()
        },
        |ui| {
            ui.add_enabled_ui(false, |ui| {
                assert_eq!(
                    systemless_ui::range::show(ui, "Volume", 0, -100, 100, false),
                    None
                );
            });
        },
    );
    output.textures_delta.clear();
}

#[test]
fn accessible_popup_menu_and_window_expose_rows_and_accept_clicks() {
    use egui::accesskit::{Action, ActionRequest, Role, TreeId};
    use systemless_ui::{menu, window};
    let context = egui::Context::default();
    context.enable_accesskit();
    let mut locked = menu::Item::new(3u8, "Locked");
    locked.enabled = false;
    let mut checked = menu::Item::new(2u8, "Veteran");
    checked.checked = true;
    checked.help = Some("Experienced crew".into());
    let rows = vec![menu::Item::new(1u8, "Scout"), checked, locked];
    let mut state = window::WindowState::default();
    let mut draw = |events| {
        let mut chosen = None;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800., 600.),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                window::show(
                    ui.ctx(),
                    egui::Id::new("palette"),
                    "Find",
                    &mut state,
                    window::WindowOptions {
                        initial_rect: egui::Rect::from_min_size(
                            egui::pos2(400., 300.),
                            egui::vec2(240., 160.),
                        ),
                        enabled: true,
                        resizable: true,
                        closable: true,
                    },
                    |ui| ui.label("Search the document"),
                );
                chosen = menu::popup(
                    ui.ctx(),
                    egui::Id::new("menu"),
                    egui::pos2(100., 100.),
                    0,
                    &rows,
                )
                .chosen;
            },
        );
        output.textures_delta.clear();
        (output, chosen)
    };
    draw(vec![]);
    let (output, _) = draw(vec![]);
    let update = output.platform_output.accesskit_update.unwrap();
    let find = |name: &str| {
        update
            .nodes
            .iter()
            .find(|(_, node)| {
                node.label()
                    .or(node.value())
                    .is_some_and(|text| text.ends_with(name))
            })
            .cloned()
    };
    let scout = find("Scout").expect("menu rows are accessible");
    assert_eq!(scout.1.role(), Role::Button);
    assert!(scout.1.supports_action(Action::Click));
    assert!(find("Veteran").is_some(), "checked rows keep their names");
    assert!(find("Locked").is_some_and(|(_, node)| node.is_disabled()));
    assert!(
        find("Search the document").is_some(),
        "window content is exposed"
    );
    assert!(find("Find").is_some(), "window title is exposed");
    let (_, chosen) = draw(vec![egui::Event::AccessKitActionRequest(ActionRequest {
        action: Action::Click,
        target_node: scout.0,
        target_tree: TreeId::ROOT,
        data: None,
    })]);
    assert_eq!(chosen, Some(1));
}
