//! Ajustes do dispositivo; integrações globais ficam em application.rs.
use super::{
    App, Icon, Page, Task, brand, rgb_label, rotation::PreviewRotation, theme::Palette, widgets,
};
use eframe::egui::{self, RichText};
use pelicool::device::{DisplayMode, RgbMode};

impl App {
    pub(super) fn settings_card(&mut self, ui: &mut egui::Ui, p: Palette, width: f32) {
        p.card().inner_margin(12).show(ui, |ui| {
            ui.set_width((width - 26.).max(200.));
            ui.spacing_mut().item_spacing.y = 3.;
            let caps = self.module.info.capabilities;
            widgets::section(ui, "Tela e iluminação", "Aplique os ajustes quando terminar.", p);
            ui.add_enabled_ui(self.task.is_none(), |ui| {
                if caps.screen_power {
                    widgets::toggle(ui, &mut self.settings.screen_on, "Tela ligada", p);
                }
                if !caps.rgb_modes.is_empty() {
                    ui.label(RichText::new("Iluminação dos LEDs").size(12.).strong());
                    egui::ComboBox::from_id_salt("rgb")
                        .width(ui.available_width() - 4.)
                        .selected_text(rgb_label(self.settings.rgb))
                        .show_ui(ui, |ui| {
                            for &mode in caps.rgb_modes {
                                if mode != RgbMode::Media || self.mode == DisplayMode::Media {
                                    ui.selectable_value(&mut self.settings.rgb, mode, rgb_label(mode));
                                }
                            }
                        });
                    let description = match self.settings.rgb {
                        RgbMode::Internal => "Alerta térmico nativo. A temperatura é enviada a cada segundo.",
                        RgbMode::Motherboard => "Recebe o sinal do cabo ARGB da placa-mãe.",
                        RgbMode::Media => "Os LEDs acompanham as cores da imagem ou GIF.",
                    };
                    ui.label(RichText::new(description).size(10.).color(p.muted));
                    if self.settings.rgb == RgbMode::Motherboard
                        && ui.small_button("Configurar OpenRGB…").clicked()
                    {
                        self.page = Page::Settings;
                    }
                }
                if caps.brightness {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Brilho").size(12.).strong());
                        ui.label(RichText::new(format!("{}%", self.settings.brightness)).size(12.).color(p.accent));
                    });
                    ui.add_enabled_ui(self.settings.screen_on, |ui| {
                        ui.spacing_mut().slider_width = ui.available_width();
                        ui.add(egui::Slider::new(&mut self.settings.brightness, 0..=100).show_value(false));
                    });
                }
                let encoded_rotation = self.mode == DisplayMode::Media && caps.media.is_some_and(|m| m.rotation_requires_upload);
                if caps.gyro && !encoded_rotation {
                    widgets::toggle(ui, &mut self.settings.gyro, "Rotação automática", p);
                }
                if encoded_rotation {
                    self.settings.gyro = false;
                }
                if caps.rotation {
                    ui.add_enabled_ui(!self.settings.gyro, |ui| {
                        ui.label(RichText::new(if encoded_rotation {"Orientação da mídia"} else {"Posição da tela"}).size(12.).strong());
                        let display_rotation=self.module.info.display_rotation(self.settings.rotation);
                        ui.horizontal(|ui| {
                            for angle in [0, 90, 180, 270] {
                                if widgets::rotation(ui, angle, display_rotation == angle, p).clicked() {
                                    self.settings.rotation = self.module.info.raw_rotation(angle);
                                }
                            }
                        });
                        ui.label(RichText::new(PreviewRotation::relative(display_rotation).description(true)).size(10.).color(p.muted));
                        ui.label(RichText::new("0°: marca do bloco em cima e legível.").size(10.).color(p.muted));
                        if encoded_rotation {
                            ui.label(RichText::new("Aplicar outra posição prepara e atualiza a coleção. Giroscópio disponível nos presets.").size(10.).color(p.muted));
                            ui.label(RichText::new("Editar enquadramento ajusta o recorte à orientação escolhida.").size(10.).color(p.muted));
                        }
                    });
                }
                if self.mode == DisplayMode::Media {
                    ui.separator();
                    let previous = (self.settings.slideshow, self.settings.slide_seconds);
                    widgets::toggle(ui, &mut self.settings.slideshow, "Slideshow", p);
                    ui.add_enabled_ui(self.settings.slideshow, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Intervalo").size(11.));
                            for seconds in [3, 5, 7] {
                                ui.selectable_value(&mut self.settings.slide_seconds, seconds, format!("{seconds}s"));
                            }
                        });
                    });
                    ui.label(RichText::new("Alterna apenas o tipo selecionado: imagens ou GIFs. GIFs completam a reprodução antes da troca.").size(10.).color(p.muted));
                    if previous != (self.settings.slideshow, self.settings.slide_seconds) {
                        self.operation(Task::Apply);
                    }
                }
                if widgets::icon_button(ui, Icon::Settings, "Aplicar tela e LEDs", p.soft, p.accent, ui.available_width()).clicked() {
                    self.operation(if self.mode == DisplayMode::Media { Task::Apply } else { Task::Monitor });
                }
            });
        });
    }

    pub(super) fn device_card(&mut self, ui: &mut egui::Ui, p: Palette, width: f32) {
        p.card().show(ui, |ui| {
            ui.set_width((width - 42.0).max(200.0));
            widgets::section(
                ui,
                self.module.info.name,
                &self
                    .module
                    .info
                    .usb
                    .ids
                    .iter()
                    .map(|id| format!("USB {:04x}:{:04x}", id.vendor, id.product))
                    .collect::<Vec<_>>()
                    .join(" · "),
                p,
            );
            ui.add_space(16.0);
            for (title, value) in [
                ("Módulo ativo", self.module.info.id.to_owned()),
                (
                    "Modelos validados",
                    self.module.info.tested_models.join(", "),
                ),
                (
                    "Conexão",
                    if self.connected {
                        "Comunicação USB confirmada"
                    } else if self.detected {
                        "Detectado pelo Linux"
                    } else {
                        "Dispositivo não detectado"
                    }
                    .to_owned(),
                ),
                (
                    "Última mídia enviada nesta sessão",
                    self.last_sent
                        .clone()
                        .unwrap_or_else(|| "Nenhum envio nesta sessão".into()),
                ),
                ("Aplicativo", format!("{} · Rust", brand::NAME)),
            ] {
                ui.label(RichText::new(title).size(12.0).color(p.muted));
                ui.label(RichText::new(value).size(15.0));
                ui.add_space(8.0);
            }
            ui.add_space(8.0);
            ui.add_enabled_ui(self.task.is_none(), |ui| {
                if widgets::icon_button(
                    ui,
                    Icon::Link,
                    "Testar conexão USB",
                    p.soft,
                    p.accent,
                    212.0,
                )
                .clicked()
                {
                    self.operation(Task::Probe);
                }
            });
        });
    }
}
