//! A virtual file picker. The host supplies entries and performs all I/O.
use crate::{
    components::{button, ButtonStyle},
    egui, Theme,
};

#[derive(Clone, Copy)]
pub struct Entry<'a> {
    pub name: &'a str,
    pub directory: bool,
}
pub struct Listing<'a> {
    pub title: &'a str,
    pub directory: &'a str,
    pub entries: &'a [Entry<'a>],
    pub save: bool,
    pub writable: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Open(usize),
    Save(String),
    Parent,
    Root,
    Cancel,
}
/// Keep one state per dialog. Call `reset_selection` when the listing changes.
#[derive(Default)]
pub struct FilePicker {
    pub name: String,
    selected: Option<usize>,
}
impl FilePicker {
    pub fn reset_selection(&mut self) {
        self.selected = None;
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        theme: &Theme,
        listing: Listing<'_>,
    ) -> Option<Action> {
        let mut action = None;
        ui.set_min_width(420.);
        ui.heading(listing.title);
        ui.add_space(12.);
        ui.horizontal(|ui| {
            if button(ui, theme, "Up", ButtonStyle::Secondary).clicked() {
                action = Some(Action::Parent);
            }
            if button(ui, theme, "Desktop", ButtonStyle::Ghost).clicked() {
                action = Some(Action::Root);
            }
            ui.label(egui::RichText::new(listing.directory).color(theme.colors.muted));
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(300.)
            .min_scrolled_height(160.)
            .show(ui, |ui| {
                if listing.entries.is_empty() {
                    ui.weak("This folder is empty");
                }
                for (index, entry) in listing.entries.iter().enumerate() {
                    let label = if entry.directory {
                        format!("▸  {}", entry.name)
                    } else {
                        entry.name.to_owned()
                    };
                    let response = ui.selectable_label(self.selected == Some(index), label);
                    if response.clicked() {
                        self.selected = Some(index);
                        if listing.save && !entry.directory {
                            self.name = entry.name.to_owned();
                        }
                    }
                    if response.double_clicked() && (entry.directory || !listing.save) {
                        action = Some(Action::Open(index));
                    }
                }
            });
        ui.separator();
        if listing.save {
            ui.horizontal(|ui| {
                let label = ui.label("Name");
                ui.text_edit_singleline(&mut self.name)
                    .labelled_by(label.id);
            });
            if !listing.writable {
                ui.weak("This folder is read-only");
            }
        }
        ui.add_space(8.);
        ui.horizontal(|ui| {
            if button(ui, theme, "Cancel", ButtonStyle::Secondary).clicked() {
                action = Some(Action::Cancel);
            }
            if listing.save {
                let valid = listing.writable
                    && !self.name.trim().is_empty()
                    && !self.name.contains([':', '/'])
                    && !self.name.chars().any(char::is_control);
                if ui
                    .add_enabled_ui(valid, |ui| button(ui, theme, "Save", ButtonStyle::Primary))
                    .inner
                    .clicked()
                {
                    action = Some(Action::Save(self.name.clone()));
                }
                if let Some(index) = self
                    .selected
                    .filter(|&i| listing.entries.get(i).is_some_and(|e| e.directory))
                {
                    if button(ui, theme, "Open folder", ButtonStyle::Ghost).clicked() {
                        action = Some(Action::Open(index));
                    }
                }
            } else {
                let selected = self.selected.filter(|&i| i < listing.entries.len());
                if ui
                    .add_enabled_ui(selected.is_some(), |ui| {
                        button(ui, theme, "Open", ButtonStyle::Primary)
                    })
                    .inner
                    .clicked()
                {
                    action = selected.map(Action::Open);
                }
            }
        });
        if action.is_none()
            && ui.is_enabled()
            && ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Period))
        {
            action = Some(Action::Cancel);
        }
        action
    }
}
