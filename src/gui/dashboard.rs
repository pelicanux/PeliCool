use super::{App, Task, theme::Palette, widgets};
use eframe::egui::{self, Align2, FontId, Pos2, RichText, Sense, Stroke};
use pelicool::sensors::Kind;
fn fmt(value: Option<f64>, digits: usize, unit: &str) -> String {
    value
        .map(|v| format!("{v:.digits$}{unit}"))
        .unwrap_or_else(|| "--".into())
}
fn gib(value: Option<f64>) -> String {
    fmt(value.map(|v| v / 1073741824.), 1, " GiB")
}
fn gauge(ui: &mut egui::Ui, usage: Option<f64>, p: Palette) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(150., 125.), Sense::hover());
    let center = Pos2::new(rect.center().x, rect.top() + 77.);
    let radius = 58.;
    let arc = |fraction: f32| {
        (0..=80)
            .map(|i| {
                let angle = std::f32::consts::PI * 0.75
                    + std::f32::consts::PI * 1.5 * fraction * i as f32 / 80.;
                center + egui::vec2(angle.cos(), angle.sin()) * radius
            })
            .collect()
    };
    ui.painter()
        .add(egui::Shape::line(arc(1.), Stroke::new(5.0_f32, p.field)));
    if let Some(value) = usage {
        ui.painter().add(egui::Shape::line(
            arc((value / 100.).clamp(0., 1.) as f32),
            Stroke::new(4.0_f32, p.accent),
        ));
    }
    ui.painter().text(
        center - egui::vec2(0., 13.),
        Align2::CENTER_CENTER,
        "USO",
        FontId::proportional(10.),
        p.muted,
    );
    ui.painter().text(
        center + egui::vec2(0., 10.),
        Align2::CENTER_CENTER,
        fmt(usage, 0, "%"),
        FontId::proportional(31.),
        p.text,
    );
}
impl App {
    pub(super) fn device_page(&mut self, ui: &mut egui::Ui, p: Palette) {
        self.page_title(
            ui,
            p,
            "Seu sistema, ao vivo.",
            "Sensores disponíveis no Linux · atualização a cada segundo.",
        );
        ui.columns(2, |columns| {
            for (i, gpu) in [false, true].into_iter().enumerate() {
                p.card().inner_margin(16).show(&mut columns[i], |ui| {
                    ui.set_width((ui.available_width() - 2.).max(200.));
                    let s = &self.readings;
                    widgets::section(
                        ui,
                        if gpu { "GPU" } else { "CPU" },
                        if gpu {
                            s.gpu_name.as_deref().unwrap_or("Sem telemetria da GPU")
                        } else {
                            s.cpu_name.as_deref().unwrap_or("Processador")
                        },
                        p,
                    );
                    ui.horizontal(|ui| {
                        gauge(ui, if gpu { s.gpu_usage } else { s.cpu_usage }, p);
                        ui.vertical(|ui| {
                            for (name, value) in [
                                (
                                    "Temperatura",
                                    fmt(if gpu { s.gpu_temp } else { s.cpu_temp }, 1, " °C"),
                                ),
                                (
                                    "Frequência",
                                    fmt(if gpu { s.gpu_mhz } else { s.cpu_mhz }, 0, " MHz"),
                                ),
                                (
                                    "Memória",
                                    if gpu {
                                        gib(s.gpu_vram_used)
                                    } else {
                                        fmt(s.ram_usage, 0, "% RAM")
                                    },
                                ),
                            ] {
                                ui.label(RichText::new(value).size(16.).color(p.accent));
                                ui.label(RichText::new(name).size(10.).color(p.muted));
                            }
                        });
                    });
                });
            }
        });
        ui.add_space(10.);
        ui.columns(3, |columns| {
            p.card().inner_margin(16).show(&mut columns[0], |ui| {
                widgets::section(ui, "RAM", "Memória do sistema", p);
                ui.label(RichText::new(fmt(self.readings.ram_usage, 0, "%")))
                    .on_hover_text("Uso calculado com MemAvailable");
                ui.add(
                    egui::ProgressBar::new((self.readings.ram_usage.unwrap_or(0.) / 100.) as f32)
                        .fill(p.accent),
                );
                ui.label(
                    RichText::new(format!(
                        "{} / {}",
                        gib(self.readings.ram_used),
                        gib(self.readings.ram_total)
                    ))
                    .size(11.)
                    .color(p.muted),
                );
            });
            p.card().inner_margin(16).show(&mut columns[1], |ui| {
                widgets::section(ui, "Armazenamento", "Discos físicos · total", p);
                ui.label(fmt(
                    self.readings.disk_read.map(|v| v / 1048576.),
                    1,
                    " MiB/s leitura",
                ));
                ui.label(fmt(
                    self.readings.disk_write.map(|v| v / 1048576.),
                    1,
                    " MiB/s escrita",
                ));
            });
            p.card().inner_margin(16).show(&mut columns[2], |ui| {
                widgets::section(ui, "Rede", "Interfaces · total", p);
                ui.label(fmt(
                    self.readings.network_rx.map(|v| v / 1024.),
                    1,
                    " KiB/s recebidos",
                ));
                ui.label(fmt(
                    self.readings.network_tx.map(|v| v / 1024.),
                    1,
                    " KiB/s enviados",
                ));
            });
        });
        ui.add_space(12.);
        p.card().inner_margin(16).show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.).max(200.));
            widgets::section(
                ui,
                "Sensores detectados",
                "Temperaturas, rotações e tensões expostas pelo sistema.",
                p,
            );
            egui::Grid::new("live-sensors")
                .num_columns(4)
                .min_col_width(160.)
                .spacing(egui::vec2(20., 8.))
                .show(ui, |ui| {
                    for (i, metric) in self.readings.available.iter().enumerate() {
                        let unit = match metric.kind {
                            Kind::Temperature => " °C",
                            Kind::Fan => " RPM",
                            Kind::Voltage => " V",
                        };
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&metric.label).size(10.).color(p.muted));
                            ui.label(
                                RichText::new(format!("{:.1}{unit}", metric.value))
                                    .size(15.)
                                    .color(p.accent),
                            );
                        });
                        if i % 4 == 3 {
                            ui.end_row();
                        }
                    }
                });
            if self.readings.available.is_empty() {
                ui.label("Nenhum sensor hwmon disponível.");
            }
        });
        ui.add_space(10.);
        egui::CollapsingHeader::new("Escolher fontes para o LCD").show(ui, |ui| {
            self.source_controls(ui, p, ui.available_width());
        });
        egui::CollapsingHeader::new("Conexão com o MYSTIQUE").show(ui, |ui| {
            self.device_card(ui, p, ui.available_width());
        });
        if self.task == Some(Task::Probe) {
            ui.spinner();
        }
    }
}
