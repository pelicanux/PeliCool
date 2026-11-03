//! Navegação, organização dos painéis e mensagens de estado.
use super::{App, Icon, Page, brand, progress, theme::Palette, widgets};
use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use pelicool::device::DisplayMode;

impl App {
    pub(super) fn sidebar(&mut self, ctx: &egui::Context) {
        let bg = Color32::from_rgb(15, 27, 35);
        egui::SidePanel::left("navigation")
            .exact_width(176.)
            .resizable(false)
            .frame(egui::Frame::new().fill(bg).inner_margin(12))
            .show(ctx, |ui| {
                ui.add_space(8.);
                ui.vertical_centered(|ui| {
                    ui.add(
                        egui::Image::new(brand::logo())
                            .max_width(132.)
                            .max_height(120.)
                            .maintain_aspect_ratio(true),
                    );
                    ui.add_space(5.);
                    ui.label(
                        RichText::new("Seu display, do seu jeito")
                            .size(10.)
                            .color(Color32::from_rgb(114, 148, 164)),
                    );
                });
                ui.add_space(18.);
                ui.label(
                    RichText::new("SEU ESPAÇO")
                        .size(10.)
                        .color(Color32::from_rgb(105, 131, 147)),
                );
                ui.add_space(5.);
                for (page, kind, label) in [
                    (Page::Personalize, Icon::Screen, "Personalização"),
                    (Page::Device, Icon::Device, "Dispositivo"),
                    (Page::Settings, Icon::Gear, "Configurações"),
                ] {
                    if widgets::nav(ui, kind, label, self.page == page).clicked() {
                        self.page = page;
                    }
                }
                let bottom = ui.max_rect().bottom() - 150.;
                ui.add_space((bottom - ui.cursor().top()).max(20.));
                egui::Frame::new()
                    .fill(Color32::from_rgb(22, 39, 48))
                    .corner_radius(10)
                    .inner_margin(12)
                    .show(ui, |ui| {
                        ui.set_width(126.);
                        ui.label(
                            RichText::new(self.model_name())
                                .size(12.)
                                .strong()
                                .color(Color32::from_rgb(221, 238, 239)),
                        );
                        ui.label(
                            RichText::new(if self.detected {
                                "USB detectado"
                            } else {
                                "Aguardando dispositivo"
                            })
                            .size(11.)
                            .color(Color32::from_rgb(125, 162, 173)),
                        );
                        ui.add_space(5.);
                        ui.label(
                            RichText::new("Linux · aplicativo nativo")
                                .size(10.)
                                .color(Color32::from_rgb(89, 206, 181)),
                        );
                    });
                ui.add_space(15.);
                ui.label(
                    RichText::new("Projeto independente")
                        .size(9.)
                        .color(Color32::from_rgb(102, 126, 141)),
                );
            });
    }

    pub(super) fn topbar(&mut self, ctx: &egui::Context, p: Palette) {
        egui::TopBottomPanel::top("topbar")
            .exact_height(52.)
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.card)
                    .inner_margin(egui::Margin::symmetric(20, 10)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Dispositivos").size(13.).color(p.muted));
                    ui.label(RichText::new("/").color(p.border));
                    ui.label(RichText::new(self.model_name()).size(13.).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let (rect, response) =
                            ui.allocate_exact_size(Vec2::splat(32.), Sense::click());
                        if response.hovered() {
                            ui.painter().rect_filled(rect, 8, p.field);
                        }
                        widgets::icon(
                            ui.painter(),
                            rect.shrink(6.),
                            if self.dark { Icon::Sun } else { Icon::Moon },
                            p.muted,
                        );
                        if response
                            .on_hover_text("Alternar tema claro/escuro")
                            .clicked()
                        {
                            self.set_dark(ctx, !self.dark);
                        }
                        let (label, color) = if self.connected {
                            ("USB verificado", p.accent)
                        } else if self.detected {
                            ("Dispositivo detectado", p.accent)
                        } else {
                            ("USB não detectado", p.muted)
                        };
                        widgets::pill(ui, label, color, p.soft);
                    });
                });
            });
    }

    pub(super) fn footer(&mut self, ctx: &egui::Context, p: Palette) {
        if self.page == Page::Settings {
            egui::TopBottomPanel::bottom("preferences-status").show(ctx, |ui| {
                ui.label(
                    RichText::new(if self.error || self.task.is_some() {
                        &self.status
                    } else {
                        "As preferências são salvas automaticamente."
                    })
                    .color(if self.error {
                        Color32::from_rgb(198, 92, 71)
                    } else {
                        p.muted
                    }),
                );
            });
            return;
        }
        if self.page == Page::Device {
            egui::TopBottomPanel::bottom("sensor-status")
                .frame(
                    egui::Frame::new()
                        .fill(p.card)
                        .inner_margin(egui::Margin::symmetric(20, 10)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("Leituras locais do Linux · 1 segundo")
                                .size(12.)
                                .color(p.accent),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.link("Personalizar o LCD").clicked() {
                                self.page = Page::Personalize;
                            }
                        });
                    });
                });
            return;
        }
        if self.mode != DisplayMode::Media {
            self.preset_footer(ctx, p);
            return;
        }
        // Não reserve um rodapé vazio. O progresso fica sobreposto para não
        // deslocar os controles quando começa ou termina uma transferência.
        if !self.error && !matches!(self.task, Some(super::Task::Prepare | super::Task::Upload)) {
            return;
        }
        let width = (ctx.available_rect().width() - 32.).max(200.);
        egui::Area::new(egui::Id::new("media-status"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-16., -12.))
            .show(ctx, |ui| {
                p.card().inner_margin(8).show(ui, |ui| {
                    ui.set_width((width - 18.).max(180.));
                    if self.error {
                        ui.horizontal(|ui| {
                            ui.add_sized(
                                egui::vec2(ui.available_width() - 32., progress::ROW_HEIGHT),
                                egui::Label::new(
                                    RichText::new(&self.status)
                                        .size(12.)
                                        .color(Color32::from_rgb(198, 92, 71)),
                                )
                                .truncate(),
                            )
                            .on_hover_text(&self.status);
                            if ui.small_button("×").on_hover_text("Fechar aviso").clicked() {
                                self.error = false;
                            }
                        });
                    } else {
                        progress::show(ui, p, self.task, self.progress);
                    }
                });
            });
    }

    pub(super) fn page_title(
        &mut self,
        ui: &mut egui::Ui,
        p: Palette,
        title: &str,
        subtitle: &str,
    ) {
        ui.label(RichText::new(title).size(23.).strong().color(p.text));
        ui.label(RichText::new(subtitle).size(12.).color(p.muted));
        ui.add_space(4.);
    }

    pub(super) fn personalize(&mut self, ui: &mut egui::Ui, p: Palette) {
        self.mode_bar(ui, p);
        let width = ui.available_width();
        if width >= 820. {
            let right = 294.;
            let left = width - right - 14.;
            ui.horizontal_top(|ui| {
                ui.allocate_ui_with_layout(
                    egui::vec2(left, 420.),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.preview_card(ui, p, left);
                        ui.add_space(4.0);
                        if self.mode == DisplayMode::Media {
                            self.media_grid(ui, p);
                        } else {
                            self.preset_controls(ui, p, left);
                        }
                    },
                );
                ui.add_space(8.);
                ui.allocate_ui_with_layout(
                    egui::vec2(right, 420.),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| self.settings_card(ui, p, right),
                );
            });
        } else {
            self.preview_card(ui, p, width);
            ui.add_space(8.0);
            if self.mode == DisplayMode::Media {
                self.media_grid(ui, p);
            } else {
                self.preset_controls(ui, p, width);
            }
            ui.add_space(10.);
            self.settings_card(ui, p, width);
        }
    }
}
