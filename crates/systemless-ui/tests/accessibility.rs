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
