use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

#[derive(Clone, Copy)]
pub struct Palette {
    pub background: Color32,
    pub card: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub soft: Color32,
    pub field: Color32,
    pub dark: bool,
}

impl Palette {
    pub fn on_accent(self) -> Color32 {
        if self.dark {
            Color32::from_rgb(9, 40, 32)
        } else {
            Color32::WHITE
        }
    }

    pub fn new(dark: bool) -> Self {
        if dark {
            Self {
                background: Color32::from_rgb(15, 22, 27),
                card: Color32::from_rgb(23, 33, 39),
                text: Color32::from_rgb(231, 241, 243),
                muted: Color32::from_rgb(146, 165, 174),
                border: Color32::from_rgb(43, 58, 65),
                accent: Color32::from_rgb(73, 220, 185),
                soft: Color32::from_rgb(25, 61, 55),
                field: Color32::from_rgb(30, 43, 50),
                dark,
            }
        } else {
            Self {
                background: Color32::from_rgb(244, 247, 249),
                card: Color32::WHITE,
                text: Color32::from_rgb(28, 44, 55),
                muted: Color32::from_rgb(113, 132, 143),
                border: Color32::from_rgb(227, 234, 239),
                accent: Color32::from_rgb(0, 130, 114),
                soft: Color32::from_rgb(229, 246, 241),
                field: Color32::from_rgb(245, 248, 250),
                dark,
            }
        }
    }

    pub fn install(self, ctx: &egui::Context) {
        let mut style = egui::Style {
            visuals: if self.dark {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            },
            ..Default::default()
        };
        style.text_styles = [
            (
                TextStyle::Heading,
                FontId::new(22.0, FontFamily::Proportional),
            ),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (
                TextStyle::Button,
                FontId::new(13.0, FontFamily::Proportional),
            ),
            (
                TextStyle::Small,
                FontId::new(12.0, FontFamily::Proportional),
            ),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(10.0, 6.0);
        style.spacing.interact_size.y = 28.0;
        style.spacing.slider_width = 190.0;
        style.spacing.combo_width = 180.0;
        style.visuals.override_text_color = Some(self.text);
        style.visuals.panel_fill = self.background;
        style.visuals.window_fill = self.card;
        style.visuals.extreme_bg_color = self.field;
        style.visuals.faint_bg_color = self.field;
        style.visuals.selection.bg_fill = self.soft;
        style.visuals.selection.stroke = Stroke::new(1.0_f32, self.accent);
        style.visuals.hyperlink_color = self.accent;
        for widget in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.noninteractive,
        ] {
            widget.corner_radius = CornerRadius::same(8);
            widget.bg_stroke = Stroke::new(1.0_f32, self.border);
            widget.fg_stroke = Stroke::new(1.0_f32, self.text);
        }
        style.visuals.widgets.inactive.bg_fill = self.field;
        style.visuals.widgets.inactive.weak_bg_fill = self.field;
        style.visuals.widgets.hovered.bg_fill = self.soft;
        style.visuals.widgets.hovered.weak_bg_fill = self.soft;
        style.visuals.widgets.hovered.bg_stroke =
            Stroke::new(1.0_f32, self.accent.gamma_multiply(0.5));
        style.visuals.widgets.active.bg_fill = self.soft;
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.5_f32, self.accent);
        ctx.set_style(style);
    }

    pub fn card(self) -> egui::Frame {
        egui::Frame::new()
            .fill(self.card)
            .stroke(Stroke::new(1.0_f32, self.border))
            .corner_radius(14)
            .inner_margin(14)
    }
}
