//! Representação visual do display e da posição física.
use super::{App, Icon, Task, rotation::PreviewRotation, theme::Palette, widgets};
use eframe::egui::{
    self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use pelicool::device::DisplayMode;

impl App {
    pub(super) fn preview_card(&mut self, ui: &mut egui::Ui, p: Palette, width: f32) {
        let margin = 14;
        p.card().inner_margin(margin).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 5.0;
            ui.set_width((width - 2.0 * f32::from(margin) - 2.0).max(200.));
            ui.horizontal(|ui| {
                widgets::section(
                    ui,
                    if self.mode == DisplayMode::Media {
                        "Prévia da tela"
                    } else {
                        "Prévia do preset"
                    },
                    if self.mode == DisplayMode::Media {
                        if self.settings.gyro {
                            "Rotação automática ativa; prévia da posição manual."
                        } else {
                            "Mídia convertida com a posição manual selecionada."
                        }
                    } else {
                        "Layout ilustrativo com leituras reais do Linux."
                    },
                    p,
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.mode == DisplayMode::Media
                        && self.media.as_ref().is_some_and(|media| media.animated)
                    {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(30.), Sense::click());
                        ui.painter().rect_filled(rect, 8, p.field);
                        widgets::icon(
                            ui.painter(),
                            rect.shrink(8.),
                            if self.playing {
                                Icon::Pause
                            } else {
                                Icon::Play
                            },
                            p.accent,
                        );
                        if response.on_hover_text("Pausar/retomar prévia").clicked() {
                            self.playing = !self.playing;
                        }
                    }
                });
            });
            ui.add_space(8.);
            let (stage, _) = ui.allocate_exact_size(
                egui::vec2(
                    ui.available_width(),
                    if self.mode == DisplayMode::Media {
                        160.0
                    } else {
                        180.0
                    },
                ),
                Sense::hover(),
            );
            let painter = ui.painter();
            painter.rect_filled(
                stage,
                12,
                if p.dark {
                    Color32::from_rgb(18, 28, 34)
                } else {
                    Color32::from_rgb(241, 246, 247)
                },
            );
            let ring = if p.dark {
                Color32::from_rgb(32, 49, 55)
            } else {
                Color32::from_rgb(224, 235, 235)
            };
            for radius in [95., 130., 165., 200.] {
                painter.with_clip_rect(stage).circle_stroke(
                    stage.center(),
                    radius,
                    Stroke::new(1.0_f32, ring),
                );
            }
            let screen_ratio = self
                .module
                .info
                .screen_size
                .map(|(w, h)| egui::vec2(f32::from(w), f32::from(h)))
                .unwrap_or(egui::vec2(4., 3.));
            let ratio = screen_ratio;
            let rotation = PreviewRotation::relative(
                self.module.info.display_rotation(self.settings.rotation),
            );
            let rotated_ratio = rotation.size(ratio);
            let scale = ((stage.width() - 80.) / rotated_ratio.x)
                .min((stage.height() - 56.) / rotated_ratio.y);
            let screen = Rect::from_center_size(stage.center(), rotated_ratio * scale);
            let content = Rect::from_center_size(stage.center(), ratio * scale);
            let body = screen.expand(13.);
            painter.add(
                egui::epaint::Shadow {
                    offset: [0, 9],
                    blur: 24,
                    spread: 0,
                    color: Color32::from_black_alpha(48),
                }
                .as_shape(body, 10),
            );
            painter.rect_filled(body, 10, Color32::from_rgb(24, 32, 36));
            painter.rect_stroke(
                body.shrink(2.),
                8,
                Stroke::new(1.0_f32, Color32::from_rgb(63, 78, 81)),
                StrokeKind::Inside,
            );
            painter.rect_filled(screen.expand(2.), 2, Color32::BLACK);
            rotation.paint(painter, content.center(), || {
                painter.text(
                    content.center_top() - egui::vec2(0., 7.),
                    Align2::CENTER_CENTER,
                    "MARCA",
                    FontId::proportional(6.),
                    Color32::from_rgb(132, 168, 172),
                );
            });
            if self.mode != DisplayMode::Media {
                rotation.paint(painter, screen.center(), || {
                    self.draw_preset(painter, content)
                });
            } else if let Some(texture) = &self.texture {
                let source = texture.size_vec2();
                let image_scale = if self.fit == pelicool::media::Fit::Cover {
                    (content.width() / source.x).max(content.height() / source.y)
                } else {
                    (content.width() / source.x).min(content.height() / source.y)
                };
                let image_rect = Rect::from_center_size(content.center(), source * image_scale);
                rotation.image(&painter.with_clip_rect(screen), texture.id(), image_rect);
            } else {
                widgets::icon(
                    painter,
                    Rect::from_center_size(screen.center() - egui::vec2(0., 18.), Vec2::splat(36.)),
                    Icon::Screen,
                    Color32::from_rgb(98, 135, 143),
                );
                painter.text(
                    screen.center() + egui::vec2(0., 24.),
                    Align2::CENTER_CENTER,
                    "Escolha sua primeira mídia",
                    FontId::proportional(13.),
                    Color32::from_rgb(158, 185, 191),
                );
            }
            if self.task == Some(Task::Prepare) {
                painter.rect_filled(stage, 12, p.field);
                egui::Spinner::new().color(p.accent).paint_at(
                    ui,
                    Rect::from_center_size(stage.center() - egui::vec2(0., 18.), Vec2::splat(28.)),
                );
                ui.painter().text(
                    stage.center() + egui::vec2(0., 22.),
                    Align2::CENTER_CENTER,
                    "Preparando mídia…",
                    FontId::proportional(14.),
                    p.text,
                );
            }
            ui.add_space(10.);
            if self.mode != DisplayMode::Media {
                self.live_summary(ui, p);
                return;
            }
            ui.horizontal(|ui| {
                widgets::pill(
                    ui,
                    if self.media.as_ref().is_some_and(|media| media.animated) {
                        "GIF animado"
                    } else {
                        "Wallpaper"
                    },
                    p.accent,
                    p.soft,
                );
                ui.label(RichText::new("Display LCD").size(12.).color(p.muted));
                if let Some(media) = &self.media {
                    ui.label(
                        RichText::new(if media.animated {
                            format!(
                                "{} quadros · {:.1} s",
                                media.frames.len(),
                                f32::from(media.duration_ms) / 1000.
                            )
                        } else {
                            "Imagem estática".into()
                        })
                        .size(12.)
                        .color(p.muted),
                    );
                }
            });
            ui.add_space(7.);
            if ui
                .add_enabled(
                    self.task.is_none() && self.selected.is_some(),
                    egui::Button::new("Editar enquadramento"),
                )
                .clicked()
            {
                self.prepare();
            }
            if let Some(report) = self
                .media
                .as_ref()
                .and_then(|media| media.adaptation.as_ref())
                .filter(|report| report.changed())
            {
                ui.label(RichText::new(report.description()).size(11.).color(p.muted));
            }
        });
    }
}
