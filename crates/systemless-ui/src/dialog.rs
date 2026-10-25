//! Standard message dialogs with semantic actions supplied by the host.
use crate::{
    components::{button, ButtonStyle},
    egui, Theme,
};

/// Presentation state kept by the application for the lifetime of a dialog.
/// Retain it across renderer/context recreation and temporarily hidden children.
#[derive(Clone, Debug, Default)]
pub struct ModalState {
    position: Option<egui::Pos2>,
}

/// Present a modal with an optional draggable surface. Movement changes only
/// presentation; application-owned form coordinates and semantic actions stay intact.
/// The stable ID retains its position while the dialog remains alive.
pub fn show_modal<R>(
    context: &egui::Context,
    id: egui::Id,
    movable: bool,
    state: &mut ModalState,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::ModalResponse<R> {
    if !movable {
        return egui::Modal::new(id).show(context, content);
    }
    let position = state
        .position
        .unwrap_or_else(|| context.content_rect().center());
    let mut delta = egui::Vec2::ZERO;
    let response = egui::Modal::new(id)
        .area(
            egui::Area::new(id)
                .kind(egui::UiKind::Modal)
                .order(egui::Order::Foreground)
                .pivot(egui::Align2::CENTER_CENTER)
                .fixed_pos(position)
                .constrain(true),
        )
        .show(context, |ui| {
            // Modal's backdrop owns background drags. Use an explicit grip
            // so editors, buttons and range controls retain their gestures.
            let (_, strip) = ui.allocate_space(egui::vec2(0., 18.));
            let inner = content(ui);
            let grip_rect = egui::Rect::from_min_max(
                egui::pos2(ui.min_rect().left(), strip.top()),
                egui::pos2(ui.min_rect().right(), strip.bottom()),
            );
            let grip = ui
                .interact(grip_rect, id.with("move-grip"), egui::Sense::drag())
                .on_hover_cursor(egui::CursorIcon::Grab)
                .on_hover_text("Move dialog");
            ui.painter().rect_filled(
                egui::Rect::from_center_size(grip.rect.center(), egui::vec2(32., 3.)),
                1.5,
                ui.visuals().weak_text_color(),
            );
            if grip.dragged() {
                delta = grip.drag_delta();
            }
            inner
        });
    if delta != egui::Vec2::ZERO {
        let position = response.response.rect.center() + delta;
        state.position = Some(position);
        context.request_repaint();
    }
    response
}

pub struct Button<'a> {
    pub id: i16,
    pub text: &'a str,
    pub enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind<'a> {
    Range {
        value: i16,
        min: i16,
        max: i16,
        request_focus: bool,
    },
    Popup {
        value: i16,
        choices: &'a [Choice<'a>],
    },
    Text,
    /// An editable value supplied in `show`'s editor list.
    Edit,
    Image {
        texture: egui::TextureId,
        size: [u16; 2],
        monochrome: bool,
        interactive: bool,
    },
    Checkbox(bool),
    Radio(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choice<'a> {
    pub value: i16,
    pub text: &'a str,
    pub enabled: bool,
}

pub struct Field<'a> {
    pub id: i16,
    pub text: &'a str,
    pub enabled: bool,
    pub kind: FieldKind<'a>,
}

/// A reusable form specification; editor drafts remain owned by the caller.
pub struct Form<'a> {
    pub title: &'a str,
    pub fields: &'a [Field<'a>],
    pub buttons: &'a [Button<'a>],
    pub default: i16,
    pub cancel: i16,
    /// Hover help by field or button ID, such as classic help balloons.
    pub help: &'a [(i16, &'a str)],
}

pub struct EditorField {
    pub id: i16,
    pub editor: crate::editor::Editor,
    pub max_chars: usize,
}

#[derive(Default)]
pub struct FormResponse {
    pub action: Option<i16>,
    /// Control field ID and committed value. Synchronize editor changes before
    /// applying these commands to an external runtime.
    pub values: Vec<(i16, i16)>,
    /// Changed editors are updated in place. Flush these changes before applying
    /// an action when the host must synchronize them with another runtime.
    pub edits: Vec<(i16, crate::editor::Change)>,
}

/// A message is a form containing only explanatory text and actions.
pub fn message(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    paragraphs: &[&str],
    buttons: &[Button<'_>],
    default: i16,
    cancel: i16,
) -> Option<i16> {
    let fields: Vec<_> = paragraphs
        .iter()
        .map(|text| Field {
            id: 0,
            text,
            enabled: true,
            kind: FieldKind::Text,
        })
        .collect();
    form(ui, theme, title, &fields, buttons, default, cancel)
}

/// Render an immutable form snapshot and return an action identifier. The host
/// applies changes; checkbox/radio values always come from its next snapshot.
pub fn form(
    ui: &mut egui::Ui,
    theme: &Theme,
    title: &str,
    fields: &[Field<'_>],
    buttons: &[Button<'_>],
    default: i16,
    cancel: i16,
) -> Option<i16> {
    show(
        ui,
        theme,
        Form {
            title,
            fields,
            buttons,
            default,
            cancel,
            help: &[],
        },
        &mut [],
    )
    .action
}

/// Render a form with host-owned editor drafts and semantic actions. All IDs
/// must be unique within the form; each Edit field requires an EditorField.
pub fn show(
    ui: &mut egui::Ui,
    theme: &Theme,
    form: Form<'_>,
    editors: &mut [EditorField],
) -> FormResponse {
    show_with_shortcuts(ui, theme, form, editors, true)
}

/// Render a form whose default/cancel shortcuts are explicitly owned by the
/// application. In a nonmodal workspace only its active form should receive
/// these keys; inactive forms still render and retain their editor drafts.
pub fn show_with_shortcuts(
    ui: &mut egui::Ui,
    theme: &Theme,
    form: Form<'_>,
    editors: &mut [EditorField],
    shortcuts: bool,
) -> FormResponse {
    let Form {
        title,
        fields,
        buttons,
        default,
        cancel,
        help,
    } = form;
    let help_for = |id: i16| {
        help.iter()
            .find(|(item, _)| *item == id)
            .map(|(_, text)| *text)
    };
    let mut edits = Vec::new();
    let mut values = Vec::new();
    let mut cancelling = false;
    ui.set_min_width(360.);
    ui.set_max_width(520.);
    if !title.is_empty() {
        ui.heading(title);
        ui.add_space(12.);
    }
    let mut result = if shortcuts
        && ui.is_enabled()
        && !egui::Popup::is_any_open(ui.ctx())
        && !ui.input(|input| input.pointer.any_down())
    {
        let key = if ui.input_mut(|i| {
            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::Period)
        }) {
            cancelling = true;
            Some(cancel)
        } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            Some(default)
        } else {
            None
        };
        key.filter(|id| buttons.iter().any(|b| b.id == *id && b.enabled))
    } else {
        None
    };
    egui::ScrollArea::vertical()
        .id_salt("dialog-fields")
        .max_height((ui.ctx().content_rect().height() * 0.65).max(40.))
        .show(ui, |ui| {
            for field in fields {
                ui.push_id(field.id, |ui| {
                    let response = ui.add_enabled_ui(field.enabled, |ui| match field.kind {
                        FieldKind::Range {
                            value,
                            min,
                            max,
                            request_focus,
                        } => {
                            if let Some(value) =
                                crate::range::show(ui, field.text, value, min, max, request_focus)
                            {
                                values.push((field.id, value));
                            }
                        }
                        FieldKind::Popup { value, choices } => {
                            let label = ui.label(field.text);
                            let selected = choices
                                .iter()
                                .find(|choice| choice.value == value)
                                .map_or("Select…", |choice| choice.text);
                            let response = egui::ComboBox::from_id_salt("popup")
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                                .selected_text(selected)
                                .show_ui(ui, |ui| {
                                    for choice in choices {
                                        if choice.text == "-" {
                                            ui.separator();
                                            continue;
                                        }
                                        if ui
                                            .add_enabled(
                                                choice.enabled,
                                                egui::Button::selectable(
                                                    choice.value == value,
                                                    choice.text,
                                                ),
                                            )
                                            .clicked()
                                        {
                                            ui.close();
                                            values.push((field.id, choice.value));
                                        }
                                    }
                                });
                            response.response.labelled_by(label.id);
                        }
                        FieldKind::Text => {
                            ui.label(field.text);
                        }
                        FieldKind::Edit => {
                            if let Some(editor) = editors.iter_mut().find(|e| e.id == field.id) {
                                let change = crate::editor::show(
                                    ui,
                                    ui.id().with("editor"),
                                    field.text,
                                    &mut editor.editor,
                                    editor.max_chars,
                                );
                                if change.changed() {
                                    edits.push((field.id, change));
                                }
                            }
                        }
                        FieldKind::Image {
                            texture,
                            size,
                            monochrome,
                            interactive,
                        } => {
                            let desired = egui::vec2(f32::from(size[0]), f32::from(size[1]));
                            let scale = (ui.available_width() / desired.x.max(1.))
                                .min(240. / desired.y.max(1.))
                                .min(1.);
                            let mut image = egui::Image::new((texture, desired * scale))
                                .alt_text(field.text)
                                .sense(if interactive {
                                    egui::Sense::click()
                                } else {
                                    egui::Sense::hover()
                                });
                            if monochrome {
                                image = image.tint(theme.colors.text);
                            }
                            if ui.add(image).clicked() && interactive && result.is_none() {
                                result = Some(field.id);
                            }
                        }
                        FieldKind::Checkbox(mut value) => {
                            if crate::components::checkbox(ui, theme, field.text, &mut value)
                                .clicked()
                                && result.is_none()
                            {
                                result = Some(field.id);
                            }
                        }
                        FieldKind::Radio(value) => {
                            if crate::components::radio(ui, theme, field.text, value).clicked()
                                && result.is_none()
                            {
                                result = Some(field.id);
                            }
                        }
                    });
                    if let Some(text) = help_for(field.id) {
                        response.response.on_hover_text(text);
                    }
                });
                ui.add_space(8.);
            }
        });
    ui.add_space(12.);
    ui.horizontal_wrapped(|ui| {
        for item in buttons {
            let style = if item.id == default {
                ButtonStyle::Primary
            } else {
                ButtonStyle::Secondary
            };
            let mut response = ui
                .add_enabled_ui(item.enabled, |ui| button(ui, theme, item.text, style))
                .inner;
            if let Some(text) = help_for(item.id) {
                response = response.on_hover_text(text);
            }
            if response.clicked() && result.is_none() {
                result = Some(item.id);
                cancelling = item.id == cancel && item.id != default;
            }
        }
    });
    if cancelling {
        values.clear();
    } else if !values.is_empty() {
        // A control selection is its own interaction. Finish it before a
        // simultaneous default activation, preserving the selected value.
        result = None;
    }

    FormResponse {
        action: result,
        values,
        edits,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editable_form_reports_text_before_default_action() {
        let ctx = egui::Context::default();
        let theme = Theme::default();
        let mut editors = [EditorField {
            id: 3,
            editor: crate::editor::Editor {
                text: "old".into(),
                selection: (3, 0),
                request_focus: true,
                multiline: true,
            },
            max_chars: 63,
        }];
        let mut frame = |events| {
            let mut response = FormResponse::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    response = show(
                        ui,
                        &theme,
                        Form {
                            title: "Preferences",
                            fields: &[Field {
                                id: 3,
                                text: "Name",
                                enabled: true,
                                kind: FieldKind::Edit,
                            }],
                            buttons: &[Button {
                                id: 1,
                                text: "Save",
                                enabled: true,
                            }],
                            default: 1,
                            cancel: 2,
                            help: &[],
                        },
                        &mut editors,
                    );
                },
            );
            output.textures_delta.clear();
            (response, editors[0].editor.text.clone())
        };
        frame(Vec::new());
        let (response, text) = frame(vec![
            egui::Event::Text("new\nname".into()),
            egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            },
        ]);
        assert_eq!(text, "new\nname");
        assert_eq!(response.action, Some(1));
        assert_eq!(response.edits.len(), 1);
        assert_eq!(response.edits[0].0, 3);
        assert!(response.edits[0].1.text);
    }

    #[test]
    fn image_fields_preserve_clickable_and_informational_semantics() {
        let ctx = egui::Context::default();
        let theme = Theme::default();
        theme.apply(&ctx);
        let texture = ctx.load_texture(
            "icon",
            egui::ColorImage::filled([2, 2], egui::Color32::WHITE),
            egui::TextureOptions::NEAREST,
        );
        let frame = |interactive, events| {
            let mut action = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 300.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    action = form(
                        ui,
                        &theme,
                        "Image",
                        &[Field {
                            id: 3,
                            text: "Application icon",
                            enabled: true,
                            kind: FieldKind::Image {
                                texture: texture.id(),
                                size: [32, 32],
                                monochrome: true,
                                interactive,
                            },
                        }],
                        &[],
                        0,
                        0,
                    );
                },
            );
            output.textures_delta.clear();
            let bounds = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == texture.id() => {
                        Some(mesh.calc_bounds())
                    }
                    egui::Shape::Rect(rect)
                        if rect
                            .brush
                            .as_ref()
                            .is_some_and(|brush| brush.fill_texture_id == texture.id()) =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .unwrap();
            (action, bounds)
        };
        for interactive in [false, true] {
            let (_, rect) = frame(interactive, Vec::new());
            let pos = rect.center();
            frame(
                interactive,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    },
                ],
            );
            let (action, _) = frame(
                interactive,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            assert_eq!(action, interactive.then_some(3));
        }
    }

    fn text_center(shape: &egui::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::Shape::Text(text) if text.galley.job.text == label => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| text_center(shape, label)),
            _ => None,
        }
    }

    #[test]
    fn form_choices_emit_actions_and_return_still_activates_default() {
        let ctx = egui::Context::default();
        let theme = Theme::default();
        theme.apply(&ctx);
        let mut time = 0.;
        let mut frame = |events| {
            time += 0.2;
            let mut action = None;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    action = form(
                        ui,
                        &theme,
                        "Preferences",
                        &[
                            Field {
                                id: 4,
                                text: "Sound Effects",
                                enabled: true,
                                kind: FieldKind::Checkbox(false),
                            },
                            Field {
                                id: 5,
                                text: "High Quality",
                                enabled: true,
                                kind: FieldKind::Radio(false),
                            },
                            Field {
                                id: 6,
                                text: "Locked Option",
                                enabled: false,
                                kind: FieldKind::Checkbox(true),
                            },
                        ],
                        &[
                            Button {
                                id: 1,
                                text: "Save",
                                enabled: true,
                            },
                            Button {
                                id: 2,
                                text: "Cancel",
                                enabled: true,
                            },
                        ],
                        1,
                        2,
                    );
                },
            );
            output.textures_delta.clear();
            (action, output.shapes)
        };
        frame(Vec::new());
        for (label, expected) in [
            ("Sound Effects", Some(4)),
            ("High Quality", Some(5)),
            ("Locked Option", None),
        ] {
            let (_, shapes) = frame(Vec::new());
            let pos = shapes
                .iter()
                .find_map(|s| text_center(&s.shape, label))
                .unwrap();
            frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                },
            ]);
            assert_eq!(
                frame(vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default()
                }])
                .0,
                expected
            );
            assert_eq!(
                frame(vec![egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default()
                }])
                .0,
                Some(1)
            );
            frame(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: Default::default(),
            }]);
        }
        let (_, shapes) = frame(Vec::new());
        let pos = shapes
            .iter()
            .find_map(|s| text_center(&s.shape, "Cancel"))
            .unwrap();
        frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ]);
        assert_eq!(
            frame(vec![egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: Default::default()
            }])
            .0,
            Some(2)
        );
        assert_eq!(
            frame(vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default()
            }])
            .0,
            Some(1)
        );
        assert_eq!(
            frame(vec![egui::Event::Key {
                key: egui::Key::Period,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::COMMAND
            }])
            .0,
            Some(2)
        );
    }
    #[test]
    fn popup_choices_preserve_disabled_rows_and_escape_does_not_cancel_form() {
        let ctx = egui::Context::default();
        let theme = Theme::default();
        theme.apply(&ctx);
        let choices = [
            Choice {
                value: 1,
                text: "Low",
                enabled: true,
            },
            Choice {
                value: 2,
                text: "Unavailable",
                enabled: false,
            },
            Choice {
                value: 3,
                text: "-",
                enabled: false,
            },
            Choice {
                value: 4,
                text: "High",
                enabled: true,
            },
        ];
        let mut time = 0.;
        let mut frame = |events| {
            time += 0.2;
            let mut response = FormResponse::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    response = show(
                        ui,
                        &theme,
                        Form {
                            title: "Preferences",
                            fields: &[Field {
                                id: 7,
                                text: "Quality",
                                enabled: true,
                                kind: FieldKind::Popup {
                                    value: 1,
                                    choices: &choices,
                                },
                            }],
                            buttons: &[
                                Button {
                                    id: 1,
                                    text: "Save",
                                    enabled: true,
                                },
                                Button {
                                    id: 2,
                                    text: "Cancel",
                                    enabled: true,
                                },
                            ],
                            default: 1,
                            cancel: 2,
                            help: &[],
                        },
                        &mut [],
                    );
                },
            );
            let positions: std::collections::HashMap<_, _> = ["Low", "Unavailable", "High"]
                .into_iter()
                .filter_map(|label| {
                    output
                        .shapes
                        .iter()
                        .filter_map(|shape| text_center(&shape.shape, label))
                        .next_back()
                        .map(|pos| (label, pos))
                })
                .collect();
            output.textures_delta.clear();
            (response, positions)
        };
        let click = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                },
            ]
        };
        let (_, positions) = frame(Vec::new());
        let low = positions["Low"];
        frame(click(low, true));
        frame(click(low, false));
        let (_, positions) = frame(Vec::new());
        let disabled = positions["Unavailable"];
        frame(click(disabled, true));
        let (response, _) = frame(click(disabled, false));
        assert!(response.values.is_empty());
        assert!(egui::Popup::is_any_open(&ctx));
        let (response, _) = frame(vec![egui::Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        }]);
        assert_eq!(response.action, None);
        assert!(!egui::Popup::is_any_open(&ctx));
        frame(click(low, true));
        frame(click(low, false));
        let (_, positions) = frame(Vec::new());
        let selected = positions["Low"];
        frame(click(selected, true));
        let (response, _) = frame(click(selected, false));
        assert_eq!(response.values, vec![(7, 1)]);
        assert_eq!(response.action, None);
        frame(click(low, true));
        frame(click(low, false));
        let (_, positions) = frame(Vec::new());
        let high = positions["High"];
        frame(click(high, true));
        let (response, _) = frame(click(high, false));
        assert_eq!(response.values, vec![(7, 4)]);
        assert_eq!(response.action, None);
        assert!(!egui::Popup::is_any_open(&ctx));
    }
    #[test]
    fn range_changes_finish_before_default_activation_and_cancel_discards_them() {
        let ctx = egui::Context::default();
        let theme = Theme::default();
        theme.apply(&ctx);
        let mut request_focus = true;
        let mut frame = |source, events| {
            let mut result = FormResponse::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600., 400.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    result = show(
                        ui,
                        &theme,
                        Form {
                            title: "Preferences",
                            fields: &[Field {
                                id: 3,
                                text: "Level",
                                enabled: true,
                                kind: FieldKind::Range {
                                    value: source,
                                    min: -100,
                                    max: 100,
                                    request_focus,
                                },
                            }],
                            buttons: &[
                                Button {
                                    id: 1,
                                    text: "Save",
                                    enabled: true,
                                },
                                Button {
                                    id: 2,
                                    text: "Cancel",
                                    enabled: true,
                                },
                            ],
                            default: 1,
                            cancel: 2,
                            help: &[],
                        },
                        &mut [],
                    );
                },
            );
            output.textures_delta.clear();
            request_focus = false;
            result
        };
        let key = |key| egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Default::default(),
        };
        frame(0, Vec::new());
        let result = frame(0, vec![key(egui::Key::ArrowRight), key(egui::Key::Enter)]);
        assert_eq!(result.values, vec![(3, 1)]);
        assert_eq!(result.action, None);
        let result = frame(1, vec![key(egui::Key::Enter)]);
        assert!(result.values.is_empty());
        assert_eq!(result.action, Some(1));
        let result = frame(1, vec![key(egui::Key::ArrowLeft), key(egui::Key::Escape)]);
        assert!(result.values.is_empty());
        assert_eq!(result.action, Some(2));
    }
}

#[cfg(test)]
mod nonmodal_shortcut_tests {
    use super::*;

    #[test]
    fn only_the_active_form_consumes_default_and_cancel_shortcuts() {
        for (key, expected) in [(egui::Key::Enter, 1), (egui::Key::Escape, 2)] {
            let ctx = egui::Context::default();
            let mut actions = Vec::new();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events: vec![egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Default::default(),
                    }],
                    ..Default::default()
                },
                |ui| {
                    for active in [false, true, false] {
                        ui.push_id(actions.len(), |ui| {
                            actions.push(
                                show_with_shortcuts(
                                    ui,
                                    &Theme::default(),
                                    Form {
                                        title: "Preferences",
                                        fields: &[],
                                        buttons: &[
                                            Button {
                                                id: 1,
                                                text: "Apply",
                                                enabled: true,
                                            },
                                            Button {
                                                id: 2,
                                                text: "Cancel",
                                                enabled: true,
                                            },
                                        ],
                                        default: 1,
                                        cancel: 2,
                                        help: &[],
                                    },
                                    &mut [],
                                    active,
                                )
                                .action,
                            );
                        });
                    }
                },
            );
            output.textures_delta.clear();
            assert_eq!(actions, [None, Some(expected), None]);
        }
    }
}
