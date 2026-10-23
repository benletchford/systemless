//! Semantic tokens: components do not embed their own palettes or metrics.
use egui::{
    Color32, CornerRadius, FontFamily, FontId, Margin, Shadow, Stroke, TextStyle, Vec2, Visuals,
};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Appearance {
    Light,
    #[default]
    Dark,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    Compact,
    #[default]
    Comfortable,
}
#[derive(Clone, Debug)]
pub struct Palette {
    pub background: Color32,
    pub surface: Color32,
    pub elevated: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub selection: Color32,
}
#[derive(Clone, Debug)]
pub struct Theme {
    pub appearance: Appearance,
    pub density: Density,
    pub colors: Palette,
    pub font_size: f32,
    pub line_height: f32,
    pub radius: f32,
    pub spacing: f32,
    pub reduced_motion: bool,
}
impl Default for Theme {
    fn default() -> Self {
        Self::new(Appearance::Dark)
    }
}
impl Theme {
    pub fn new(appearance: Appearance) -> Self {
        let c = |hex: u32| Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
        let colors = match appearance {
            Appearance::Dark => Palette {
                background: c(0x17181d),
                surface: c(0x202229),
                elevated: c(0x292c35),
                text: c(0xebedf5),
                muted: c(0xa3a9bb),
                border: c(0x3b4050),
                accent: c(0xa499ff),
                on_accent: c(0x191330),
                danger: c(0xff929d),
                success: c(0x79dab4),
                warning: c(0xf0ca7c),
                selection: c(0x403961),
            },
            Appearance::Light => Palette {
                background: c(0xf8f9fc),
                surface: c(0xffffff),
                elevated: c(0xeff1f7),
                text: c(0x242736),
                muted: c(0x60697f),
                border: c(0xd2d7e4),
                accent: c(0x6350c6),
                on_accent: c(0xffffff),
                danger: c(0xb52743),
                success: c(0x197951),
                warning: c(0x856000),
                selection: c(0xe5e0ff),
            },
        };
        Self {
            appearance,
            density: Density::Comfortable,
            colors,
            font_size: 14.,
            line_height: 20.,
            radius: 7.,
            spacing: 8.,
            reduced_motion: false,
        }
    }
    pub fn density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }
    pub fn control_height(&self) -> f32 {
        match self.density {
            Density::Compact => 28.,
            Density::Comfortable => 36.,
        }
    }
}

impl Theme {
    /// Install semantic colors and metrics. Apply again when preferences change.
    pub fn apply(&self, ctx: &egui::Context) {
        let appearance = match self.appearance {
            Appearance::Dark => egui::Theme::Dark,
            Appearance::Light => egui::Theme::Light,
        };
        ctx.set_theme(appearance);
        let mut style = (*ctx.style_of(appearance)).clone();
        let c = &self.colors;
        let mut v = match self.appearance {
            Appearance::Dark => Visuals::dark(),
            Appearance::Light => Visuals::light(),
        };
        v.override_text_color = Some(c.text);
        v.weak_text_color = Some(c.muted);
        v.panel_fill = c.background;
        v.window_fill = c.surface;
        v.extreme_bg_color = c.background;
        v.faint_bg_color = c.elevated;
        v.window_stroke = Stroke::new(1., c.border);
        v.window_corner_radius = CornerRadius::same(12);
        v.menu_corner_radius = CornerRadius::same(8);
        v.window_shadow = Shadow {
            offset: [0, 10],
            blur: 28,
            spread: 2,
            color: Color32::from_black_alpha(if self.appearance == Appearance::Dark {
                100
            } else {
                35
            }),
        };
        v.selection.bg_fill = c.selection;
        v.selection.stroke = Stroke::new(1., c.accent);
        v.hyperlink_color = c.accent;
        v.warn_fg_color = c.warning;
        v.error_fg_color = c.danger;
        for widget in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            widget.corner_radius = CornerRadius::same(self.radius as u8);
            widget.fg_stroke = Stroke::new(1., c.text);
            widget.bg_stroke = Stroke::new(1., c.border);
            widget.bg_fill = c.surface;
            widget.weak_bg_fill = c.surface;
        }
        v.widgets.hovered.bg_fill = c.elevated;
        v.widgets.hovered.weak_bg_fill = c.elevated;
        v.widgets.hovered.bg_stroke = Stroke::new(1., c.accent);
        v.widgets.active.bg_fill = c.selection;
        v.widgets.active.weak_bg_fill = c.selection;
        v.widgets.active.bg_stroke = Stroke::new(1., c.accent);
        v.widgets.open.bg_fill = c.elevated;
        style.visuals = v;
        style.spacing.item_spacing = Vec2::new(self.spacing, self.spacing);
        style.spacing.button_padding = Vec2::new(12., 6.);
        style.spacing.interact_size = Vec2::new(36., self.control_height());
        style.spacing.window_margin = Margin::same(16);
        style.spacing.menu_margin = Margin::same(8);
        style.animation_time = if self.reduced_motion { 0. } else { 0.12 };
        style.text_styles.insert(
            TextStyle::Body,
            FontId::new(self.font_size, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Button,
            FontId::new(self.font_size, FontFamily::Proportional),
        );
        style.text_styles.insert(
            TextStyle::Heading,
            FontId::new(24., FontFamily::Proportional),
        );
        style
            .text_styles
            .insert(TextStyle::Small, FontId::new(11., FontFamily::Proportional));
        style.text_styles.insert(
            TextStyle::Monospace,
            FontId::new(13., FontFamily::Monospace),
        );
        ctx.set_style_of(appearance, style);
    }
    pub fn dock_style(&self, style: &egui::Style) -> egui_dock::Style {
        let mut dock = egui_dock::Style::from_egui(style);
        let c = &self.colors;
        dock.tab_bar.height = self.control_height() + 6.;
        dock.tab_bar.bg_fill = c.background;
        dock.tab_bar.hline_color = c.border;
        dock.tab_bar.corner_radius = CornerRadius::same(8);
        dock.tab.active.bg_fill = c.surface;
        dock.tab.active.text_color = c.text;
        dock.tab.focused.bg_fill = c.surface;
        dock.tab.focused.text_color = c.accent;
        dock.tab.inactive.bg_fill = c.background;
        dock.tab.inactive.text_color = c.muted;
        dock.tab.hovered.bg_fill = c.elevated;
        dock.tab.tab_body.bg_fill = c.surface;
        dock.tab.tab_body.inner_margin = Margin::same(20);
        dock.tab.tab_body.corner_radius = CornerRadius::same(8);
        dock.tab.tab_body.stroke = Stroke::new(1., c.border);
        dock.tab.minimum_width = Some(100.);
        dock.main_surface_border_stroke = Stroke::NONE;
        dock.overlay.selection_color = c.accent.gamma_multiply(0.25);
        dock
    }
}
