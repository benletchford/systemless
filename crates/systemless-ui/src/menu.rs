//! Menu rows for menu bars and anchored popup menus. Items carry an
//! application-defined identity; the application validates any choice.
use crate::egui;

/// One menu row. Separators and submenu parents are never chosen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item<Id> {
    pub id: Id,
    pub text: String,
    pub enabled: bool,
    pub checked: bool,
    pub shortcut: Option<String>,
    /// Hover help, such as a classic help balloon for the row.
    pub help: Option<String>,
    pub separator: bool,
    /// A non-empty list makes this row open a submenu.
    pub children: Vec<Item<Id>>,
}

impl<Id> Item<Id> {
    pub fn new(id: Id, text: impl Into<String>) -> Self {
        Self {
            id,
            text: text.into(),
            enabled: true,
            checked: false,
            shortcut: None,
            help: None,
            separator: false,
            children: Vec::new(),
        }
    }
    pub fn separator(id: Id) -> Self {
        Self {
            separator: true,
            enabled: false,
            ..Self::new(id, "")
        }
    }
}

/// Render rows into the current menu UI and return the chosen identity.
/// Choosing closes the enclosing egui menu.
pub fn items<Id: Copy>(ui: &mut egui::Ui, items: &[Item<Id>]) -> Option<Id> {
    let mut chosen = None;
    for item in items {
        if item.separator {
            ui.separator();
            continue;
        }
        ui.add_enabled_ui(item.enabled, |ui| {
            if !item.children.is_empty() {
                let response = ui.menu_button(&item.text, |ui| {
                    if let Some(id) = self::items(ui, &item.children) {
                        chosen = Some(id);
                    }
                });
                mark_menu_title(&response.response);
                return;
            }
            // Mark checked rows explicitly: selection fill is invisible on
            // frameless menu rows.
            let text = if item.checked {
                format!("✔ {}", item.text)
            } else {
                item.text.clone()
            };
            // Only checked rows opt into toggle semantics; others are plain
            // menu items to assistive technology.
            let mut button = egui::Button::new(text);
            if item.checked {
                button = button.selected(true);
            }
            if let Some(shortcut) = &item.shortcut {
                button = button.shortcut_text(shortcut);
            }
            let mut response = ui.add(button);
            ui.ctx().accesskit_node_builder(response.id, |node| {
                use egui::accesskit::{Role, Toggled};
                if item.checked {
                    node.set_role(Role::MenuItemCheckBox);
                    node.set_toggled(Toggled::True);
                } else {
                    node.set_role(Role::MenuItem);
                }
                node.set_label(match &item.shortcut {
                    Some(shortcut) => format!("{} {shortcut}", item.text),
                    None => item.text.clone(),
                });
            });
            if let Some(help) = &item.help {
                response = response.on_hover_text(help);
            }
            if response.clicked() {
                chosen = Some(item.id);
                ui.close();
            }
        });
    }
    chosen
}

/// Expose a response that opens a menu (a menu bar title or submenu parent)
/// as a menu item with a menu popup.
pub fn mark_menu_title(response: &egui::Response) {
    response.ctx.accesskit_node_builder(response.id, |node| {
        node.set_role(egui::accesskit::Role::MenuItem);
        node.set_has_popup(egui::accesskit::HasPopup::Menu);
    });
}

pub struct PopupResponse<Id> {
    pub chosen: Option<Id>,
    /// Escape or a press outside the menu and its submenus.
    pub dismissed: bool,
    pub rect: egui::Rect,
}

/// Show a popup menu with the row `aligned` (zero-based) placed at `anchor`,
/// as classic popup menus place their current item over the control.
pub fn popup<Id: Copy>(
    context: &egui::Context,
    id: egui::Id,
    anchor: egui::Pos2,
    aligned: usize,
    rows: &[Item<Id>],
) -> PopupResponse<Id> {
    let style = context.global_style();
    let row = style.spacing.interact_size.y + style.spacing.item_spacing.y;
    let position = anchor - egui::vec2(8., row * aligned as f32 + 4.);
    let area = egui::Area::new(id)
        .order(egui::Order::Foreground)
        .kind(egui::UiKind::Menu)
        .fixed_pos(position)
        .constrain(true)
        .show(context, |ui| {
            egui::Frame::menu(ui.style())
                .show(ui, |ui| {
                    // Menu rows: frameless, full-width and left-aligned.
                    ui.style_mut().visuals.button_frame = false;
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    ui.set_min_width(160.);
                    egui::ScrollArea::vertical()
                        .max_height(context.content_rect().height() * 0.8)
                        .show(ui, |ui| {
                            ui.with_layout(
                                egui::Layout::top_down_justified(egui::Align::Min),
                                |ui| items(ui, rows),
                            )
                            .inner
                        })
                        .inner
                })
                .inner
        });
    let dismissed = context.input(|input| {
        input.key_pressed(egui::Key::Escape)
            || (input.pointer.any_pressed()
                && input
                    .pointer
                    .interact_pos()
                    .is_some_and(|pos| !area.response.rect.contains(pos)))
    }) && !egui::Popup::is_any_open(context);
    PopupResponse {
        chosen: area.inner,
        dismissed,
        rect: area.response.rect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Item<u8>> {
        let mut checked = Item::new(2, "Second");
        checked.checked = true;
        let mut disabled = Item::new(4, "Locked");
        disabled.enabled = false;
        vec![Item::new(1, "First"), checked, Item::separator(3), disabled]
    }

    fn frame(
        context: &egui::Context,
        events: Vec<egui::Event>,
        rows: &[Item<u8>],
    ) -> PopupResponse<u8> {
        let mut result = None;
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
                result = Some(popup(
                    ui.ctx(),
                    egui::Id::new("popup"),
                    egui::pos2(300., 200.),
                    1,
                    rows,
                ));
            },
        );
        output.textures_delta.clear();
        result.unwrap()
    }

    fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        }
    }

    #[test]
    fn popup_aligns_rows_chooses_enabled_items_and_dismisses() {
        let context = egui::Context::default();
        let rows = rows();
        // egui sizes a new area on its first frame; later frames are stable.
        frame(&context, Vec::new(), &rows);
        let first = frame(&context, Vec::new(), &rows);
        let shown = frame(&context, Vec::new(), &rows);
        assert_eq!(first.rect, shown.rect, "position is stable");
        assert!((shown.rect.left() - 292.).abs() < 1.);
        // The aligned (second) row sits over the anchor.
        assert!(shown.rect.top() < 200. && shown.rect.bottom() > 220.);
        let height = shown.rect.height() / rows.len() as f32;
        let second = egui::pos2(shown.rect.center().x, 200. + height / 3.);
        frame(
            &context,
            vec![egui::Event::PointerMoved(second), click(second, true)],
            &rows,
        );
        let response = frame(&context, vec![click(second, false)], &rows);
        assert_eq!(response.chosen, Some(2));
        // Disabled rows cannot be chosen.
        let locked = egui::pos2(shown.rect.center().x, shown.rect.bottom() - 10.);
        frame(
            &context,
            vec![egui::Event::PointerMoved(locked), click(locked, true)],
            &rows,
        );
        let response = frame(&context, vec![click(locked, false)], &rows);
        assert_eq!(response.chosen, None);
        assert!(!response.dismissed);
        // Outside presses and Escape dismiss without a choice.
        let outside = egui::pos2(20., 580.);
        let response = frame(
            &context,
            vec![egui::Event::PointerMoved(outside), click(outside, true)],
            &rows,
        );
        assert!(response.dismissed && response.chosen.is_none());
        let response = frame(
            &context,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            &rows,
        );
        assert!(response.dismissed);
    }
}
