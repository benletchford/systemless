//! Styled compositions which retain egui's native interaction and accessibility.
use crate::Theme;
use egui::{Color32, Frame, InnerResponse, Margin, Response, RichText, Stroke, Ui};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ButtonStyle {
    #[default]
    Secondary,
    Primary,
    Ghost,
    Danger,
}
pub fn button(ui: &mut Ui, theme: &Theme, label: &str, variant: ButtonStyle) -> Response {
    let c = &theme.colors;
    let (fill, text, stroke) = match variant {
        ButtonStyle::Primary => (c.accent, c.on_accent, Stroke::NONE),
        ButtonStyle::Danger => (c.danger, c.background, Stroke::NONE),
        ButtonStyle::Ghost => (Color32::TRANSPARENT, c.text, Stroke::NONE),
        ButtonStyle::Secondary => (c.surface, c.text, Stroke::new(1., c.border)),
    };
    ui.add(
        egui::Button::new(RichText::new(label).color(text))
            .fill(fill)
            .stroke(stroke)
            .min_size(egui::vec2(0., theme.control_height())),
    )
}
pub fn badge(ui: &mut Ui, text: &str, color: Color32) -> Response {
    Frame::new()
        .fill(color.gamma_multiply(0.12))
        .corner_radius(5)
        .inner_margin(Margin::symmetric(8, 4))
        .show(ui, |ui| {
            ui.label(RichText::new(text).size(11.).strong().color(color))
        })
        .inner
}
pub fn card<R>(ui: &mut Ui, theme: &Theme, add: impl FnOnce(&mut Ui) -> R) -> InnerResponse<R> {
    Frame::new()
        .fill(theme.colors.surface)
        .stroke(Stroke::new(1., theme.colors.border))
        .corner_radius(10)
        .inner_margin(20)
        .show(ui, add)
}
pub fn section(ui: &mut Ui, theme: &Theme, title: &str, description: &str) {
    ui.label(RichText::new(title).size(20.).strong());
    ui.label(RichText::new(description).color(theme.colors.muted));
    ui.add_space(8.);
}

/// A compact square check mark, independent of the button corner radius.
pub fn checkbox(ui: &mut Ui, theme: &Theme, label: &str, value: &mut bool) -> Response {
    let selected = *value;
    ui.scope(|ui| {
        let widgets = &mut ui.style_mut().visuals.widgets;
        for widget in [
            &mut widgets.noninteractive,
            &mut widgets.inactive,
            &mut widgets.hovered,
            &mut widgets.active,
            &mut widgets.open,
        ] {
            widget.corner_radius = egui::CornerRadius::same(3);
            if selected {
                widget.bg_fill = theme.colors.accent;
                widget.bg_stroke = Stroke::new(1., theme.colors.accent);
                widget.fg_stroke = Stroke::new(1.5, theme.colors.on_accent);
            }
        }
        ui.checkbox(value, label)
    })
    .inner
}

pub fn radio(ui: &mut Ui, theme: &Theme, label: &str, selected: bool) -> Response {
    ui.scope(|ui| {
        if selected {
            let widgets = &mut ui.style_mut().visuals.widgets;
            for widget in [
                &mut widgets.noninteractive,
                &mut widgets.inactive,
                &mut widgets.hovered,
                &mut widgets.active,
                &mut widgets.open,
            ] {
                widget.fg_stroke = Stroke::new(1.5, theme.colors.accent);
                widget.bg_stroke = Stroke::new(1., theme.colors.accent);
            }
        }
        ui.radio(selected, label)
    })
    .inner
}
