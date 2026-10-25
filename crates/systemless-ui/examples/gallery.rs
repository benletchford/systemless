use systemless_ui::{
    components, eframe, egui,
    egui_dock::{DockState, NodeIndex, TabViewer},
    window, workspace, Theme,
};
struct Gallery {
    dock: DockState<String>,
    name: systemless_ui::editor::Editor,
    enabled: bool,
    windows: [window::WindowState; 2],
    open: [bool; 2],
    active_window: usize,
    last_action: String,
    /// Anchor of an open popup menu and its current row.
    popup: Option<(egui::Pos2, u8)>,
    loadout: u8,
}
impl Default for Gallery {
    fn default() -> Self {
        let mut dock = DockState::new(vec!["Components".into()]);
        dock.main_surface_mut()
            .split_right(NodeIndex::root(), 0.7, vec!["Inspector".into()]);
        Self {
            dock,
            name: systemless_ui::editor::Editor {
                text: "My workspace".into(),
                selection: (12, 12),
                request_focus: false,
                multiline: false,
            },
            enabled: true,
            windows: Default::default(),
            open: [true; 2],
            active_window: 1,
            last_action: String::new(),
            popup: None,
            loadout: 1,
        }
    }
}
struct Viewer<'a> {
    name: &'a mut systemless_ui::editor::Editor,
    enabled: &'a mut bool,
}
impl TabViewer for Viewer<'_> {
    type Tab = String;
    fn id(&mut self, tab: &mut String) -> egui::Id {
        egui::Id::new(tab)
    }
    fn title(&mut self, tab: &mut String) -> egui::WidgetText {
        tab.as_str().into()
    }
    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut String) {
        let theme = Theme::default();
        if tab == "Inspector" {
            components::section(
                ui,
                &theme,
                "Workspace",
                "Drag tabs to split, dock or float panes.",
            );
            ui.checkbox(self.enabled, "Enable notifications");
        } else {
            components::section(
                ui,
                &theme,
                "Systemless UI",
                "An independent egui design system.",
            );
            let _ = systemless_ui::editor::show(
                ui,
                ui.id().with("workspace-name"),
                "Workspace name",
                self.name,
                63,
            );
            ui.horizontal_wrapped(|ui| {
                for variant in [
                    components::ButtonStyle::Primary,
                    components::ButtonStyle::Secondary,
                    components::ButtonStyle::Ghost,
                    components::ButtonStyle::Danger,
                ] {
                    let _ = components::button(ui, &theme, &format!("{variant:?}"), variant);
                }
            });
        }
    }
}
impl eframe::App for Gallery {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Show windows").clicked() {
                    self.open = [true; 2];
                }
                let names = ["Scout", "Veteran", "Expedition"];
                let popup = ui.button(format!("Loadout: {} ⏷", names[self.loadout as usize - 1]));
                if popup.clicked() {
                    self.popup = Some((popup.rect.left_top(), self.loadout));
                }
                ui.label(&self.last_action);
            });
            workspace::show(
                ui,
                &Theme::default(),
                &mut self.dock,
                &mut Viewer {
                    name: &mut self.name,
                    enabled: &mut self.enabled,
                },
            )
        });
        if let Some((anchor, current)) = self.popup {
            use systemless_ui::menu::Item;
            let names = ["Scout", "Veteran", "Expedition"];
            let mut rows: Vec<Item<u8>> = (1..=3)
                .map(|id| {
                    let mut row = Item::new(id, names[id as usize - 1]);
                    row.checked = id == current;
                    row
                })
                .collect();
            rows.insert(2, Item::separator(0));
            let response = systemless_ui::menu::popup(
                ui.ctx(),
                egui::Id::new("gallery-popup"),
                anchor,
                current as usize - 1,
                &rows,
            );
            if let Some(choice) = response.chosen {
                self.loadout = choice;
                self.popup = None;
            } else if response.dismissed {
                self.popup = None;
            }
        }
        for index in 0..self.windows.len() {
            if !self.open[index] {
                continue;
            }
            let title = ["Session", "Appearance"][index];
            let response = window::show(
                ui.ctx(),
                egui::Id::new(("gallery-window", index)),
                title,
                &mut self.windows[index],
                window::WindowOptions {
                    initial_rect: egui::Rect::from_min_size(
                        egui::pos2(80. + index as f32 * 380., 300. + index as f32 * 40.),
                        egui::vec2(420., 280.),
                    ),
                    enabled: true,
                    resizable: true,
                    closable: true,
                },
                |ui| {
                    systemless_ui::dialog::show_with_shortcuts(
                        ui,
                        &Theme::default(),
                        systemless_ui::dialog::Form {
                            title: "",
                            fields: &[systemless_ui::dialog::Field {
                                id: 3,
                                text: "Drag and resize these independent windows. Only the active window receives default and cancel shortcuts.",
                                enabled: true,
                                kind: systemless_ui::dialog::FieldKind::Text,
                            }],
                            buttons: &[
                                systemless_ui::dialog::Button { id: 1, text: "Apply", enabled: true },
                                systemless_ui::dialog::Button { id: 2, text: "Close", enabled: true },
                            ],
                            default: 1,
                            cancel: 2,
                            help: &[],
                        },
                        &mut [],
                        self.active_window == index,
                    ).action
                },
            );
            if response.activated {
                self.active_window = index;
            }
            match response.inner.flatten() {
                Some(1) => self.last_action = format!("Applied {title}"),
                Some(2) => self.open[index] = false,
                _ => {}
            }
            if response.close_requested {
                self.open[index] = false;
            }
        }
    }
}
fn main() -> eframe::Result {
    eframe::run_native(
        "Systemless UI",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            Theme::default().apply(&cc.egui_ctx);
            Ok(Box::new(Gallery::default()))
        }),
    )
}
