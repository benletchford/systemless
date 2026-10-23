//! Host-owned text editing state, independent of any application/runtime bridge.
use crate::egui;

/// Selection endpoints count Unicode characters, not UTF-8 bytes. The primary
/// endpoint moves with Shift+Arrow; retaining its direction matters for editing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Editor {
    pub text: String,
    pub selection: (usize, usize),
    /// A one-shot focus request. Disabled editors retain it until enabled.
    pub request_focus: bool,
    pub multiline: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Change {
    pub text: bool,
    pub selection: bool,
    pub gained_focus: bool,
}

impl Change {
    pub fn changed(self) -> bool {
        self.text || self.selection || self.gained_focus
    }
}

/// Show an editor with a stable host-supplied ID. The host owns the
/// draft and may replace its text/selection between frames after acknowledgement
/// or an external update. Encoding conversion and acceptance belong to the host.
pub fn show(
    ui: &mut egui::Ui,
    id: egui::Id,
    label: &str,
    editor: &mut Editor,
    max_chars: usize,
) -> Change {
    let label = (!label.is_empty()).then(|| ui.label(label));
    let count = editor.text.chars().count();
    let selection = (editor.selection.0.min(count), editor.selection.1.min(count));
    let before = editor.selection;
    let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
    let range = egui::text::CCursorRange {
        primary: egui::text::CCursor::new(selection.0),
        secondary: egui::text::CCursor::new(selection.1),
        h_pos: None,
    };
    let current = state.cursor.char_range();
    if current.is_none_or(|current| {
        current.primary.index != range.primary.index
            || current.secondary.index != range.secondary.index
    }) {
        state.cursor.set_char_range(Some(range));
        egui::TextEdit::store_state(ui.ctx(), id, state);
    }
    if editor.request_focus && ui.is_enabled() {
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    let widget = if editor.multiline {
        egui::TextEdit::multiline(&mut editor.text).desired_rows(3)
    } else {
        egui::TextEdit::singleline(&mut editor.text)
    };
    let output = widget
        .id(id)
        .interactive(ui.is_enabled())
        .char_limit(max_chars)
        .min_size(egui::vec2(0., ui.spacing().interact_size.y))
        .vertical_align(if editor.multiline {
            egui::Align::TOP
        } else {
            egui::Align::Center
        })
        .margin(egui::Margin::symmetric(10, 4))
        .desired_width(f32::INFINITY)
        .show(ui);
    if editor.request_focus && output.response.has_focus() {
        editor.request_focus = false;
    }
    editor.selection = output
        .cursor_range
        .map(|range| (range.primary.index.0, range.secondary.index.0))
        .unwrap_or(selection);
    let change = Change {
        text: output.response.changed(),
        selection: before != editor.selection,
        gained_focus: output.response.gained_focus(),
    };
    if let Some(label) = label {
        output.response.response.labelled_by(label.id);
    }
    change
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_selection_external_updates_and_disabled_input() {
        let ctx = egui::Context::default();
        let mut editor = Editor {
            text: "café".into(),
            selection: (4, 3),
            request_focus: true,
            multiline: false,
        };
        let frame = |editor: &mut Editor, events, enabled| {
            let mut change = Change::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    events,
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 200.),
                    )),
                    ..Default::default()
                },
                |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        change = show(ui, egui::Id::new("name"), "Name", editor, 8);
                    });
                },
            );
            output.textures_delta.clear();
            change
        };
        frame(&mut editor, Vec::new(), true);
        let change = frame(&mut editor, vec![egui::Event::Text("ö".into())], true);
        assert!(change.text);
        assert_eq!(editor.text, "cafö");
        assert_eq!(editor.selection, (4, 4));

        // A runtime update must replace egui's retained cursor as well as text.
        editor.text = "guest".into();
        editor.selection = (5, 0);
        frame(&mut editor, Vec::new(), true);
        frame(
            &mut editor,
            vec![egui::Event::Text("123456789".into())],
            true,
        );
        assert_eq!(editor.text, "12345678");
        assert_eq!(editor.selection, (8, 8));

        editor.text = "locked".into();
        editor.selection = (6, 0);
        editor.request_focus = true;
        let change = frame(&mut editor, vec![egui::Event::Text("x".into())], false);
        assert!(!change.text);
        assert_eq!(editor.text, "locked");
        assert!(editor.request_focus);

        editor.selection = (usize::MAX, usize::MAX);
        frame(&mut editor, Vec::new(), true);
        assert_eq!(editor.selection, (6, 6));
    }
}
