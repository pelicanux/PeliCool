use super::{App, Task, theme::Palette, widgets, worker};
use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, RichText, Stroke};
use pelicool::{
    device::{DisplayMode, RgbMode},
    sensors::Kind,
};

pub(super) fn mode_name(mode: DisplayMode) -> &'static str {
    match mode {
        DisplayMode::Monitoring => "Monitoring",
        DisplayMode::Media => "Media",
        DisplayMode::Recorder => "Recording",
    }
}
fn main_name(index: u8) -> &'static str {
    match index {
        0 => "Frequência da CPU",
        1 => "Hora",
        2 => "RPM da bomba",
        3 => "RPM da ventoinha da CPU",
        4 => "Ventoinha + bomba",
        _ => "Temperatura da CPU",
    }
}
fn style_name(index: u8) -> &'static str {
    match index {
        1 => "Anel",
        2 => "Cronômetro",
        _ => "Normal",
    }
}
fn aux_name(index: u8) -> &'static str {
    match index {
        0 => "Tensões",
        2 => "Dados dos núcleos",
        _ => "Monitoramento do sistema",
    }
}
fn value(value: Option<f64>, digits: usize) -> String {
    value
        .filter(|v| v.is_finite())
        .map(|v| format!("{v:.digits$}"))
        .unwrap_or_else(|| "--".into())
}

impl App {
    pub(super) fn mode_bar(&mut self, ui: &mut egui::Ui, p: Palette) {
        let previous = self.mode;
        p.card().inner_margin(10).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled_ui(self.task.is_none(), |ui| {
                    for mode in [
                        DisplayMode::Monitoring,
                        DisplayMode::Media,
                        DisplayMode::Recorder,
                    ] {
                        if !self.module.info.capabilities.modes.contains(&mode) {
                            continue;
                        }
                        let text = match mode {
                            DisplayMode::Monitoring => "Monitoring · presets",
                            DisplayMode::Media => "Media · imagens e GIFs",
                            DisplayMode::Recorder => "Recording · gráficos",
                        };
                        ui.selectable_value(&mut self.mode, mode, text);
                    }
                });
                if let Some(active) = self.monitoring {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        widgets::pill(
                            ui,
                            &format!("{} ativo", mode_name(active)),
                            p.accent,
                            p.soft,
                        );
                    });
                }
            });
        });
        if previous != self.mode
            && self.mode != DisplayMode::Media
            && self.settings.rgb == RgbMode::Media
            && self
                .module
                .info
                .capabilities
                .rgb_modes
                .contains(&RgbMode::Internal)
        {
            self.settings.rgb = RgbMode::Internal;
        }
        ui.add_space(8.);
    }

    pub(super) fn preset_controls(&mut self, ui: &mut egui::Ui, p: Palette, width: f32) {
        if !self.module.info.capabilities.native_presets {
            p.card().show(ui, |ui| {
                widgets::section(
                    ui,
                    "Monitoramento",
                    "As leituras são enviadas ao layout oferecido pelo módulo conectado.",
                    p,
                );
            });
            return;
        }
        p.card().inner_margin(16).show(ui,|ui|{
            ui.spacing_mut().item_spacing.y = 5.0;
            ui.set_width((width-34.).max(200.));
            ui.add_enabled_ui(self.task.is_none(),|ui|{
                if self.mode==DisplayMode::Monitoring {
                    widgets::section(ui,"Personalizar o preset","Escolha o conteúdo do layout nativo do display.",p);
                    ui.label(RichText::new("Área principal").size(12.));
                    egui::ComboBox::from_id_salt("preset-main").width(ui.available_width()-4.).selected_text(main_name(self.preset.main)).show_ui(ui,|ui|{
                        for index in 0..=5 {
                            let available=match index{0=>self.readings.cpu_mhz.is_some(),2=>self.readings.pump_rpm.is_some(),3=>self.readings.fan_rpm.is_some(),4=>self.readings.fan_rpm.is_some()&&self.readings.pump_rpm.is_some(),5=>self.readings.cpu_temp.is_some(),_=>true};
                            ui.add_enabled_ui(available,|ui|{ui.selectable_value(&mut self.preset.main,index,main_name(index));});
                        }
                    });
                    ui.columns(2,|columns|{
                        columns[0].label(RichText::new("Estilo (frequência)").size(12.));
                        columns[0].add_enabled_ui(self.preset.main == 0, |ui| {
                            egui::ComboBox::from_id_salt("preset-style").width(ui.available_width()-4.).selected_text(style_name(self.preset.style)).show_ui(ui,|ui|{
                                for index in 0..=2 {ui.selectable_value(&mut self.preset.style,index,style_name(index));}
                            });
                        });
                        columns[1].label(RichText::new("Área auxiliar").size(12.));
                        egui::ComboBox::from_id_salt("preset-aux").width(columns[1].available_width()-4.).selected_text(aux_name(self.preset.auxiliary)).show_ui(&mut columns[1],|ui|{
                            for index in 0..=2 {
                                let available=index!=0||(self.readings.v33.is_some()&&self.readings.v5.is_some()&&self.readings.v12.is_some());
                                ui.add_enabled_ui(available,|ui|{ui.selectable_value(&mut self.preset.auxiliary,index,aux_name(index));});
                            }
                        });
                    });
                }else{
                    widgets::section(ui,"Conteúdo do gráfico","Curvas desenhadas pelo próprio MYSTIQUE.",p);
                    widgets::toggle(ui,&mut self.preset.graph_clock,"Frequência da CPU",p);
                    widgets::toggle(ui,&mut self.preset.graph_temperature,"Temperatura da CPU",p);
                    ui.add_enabled_ui(self.monitoring==Some(DisplayMode::Recorder),|ui|{
                        if ui.button("Reiniciar gráfico").clicked(){self.operation(Task::ResetGraph);}
                    });
                    ui.label(RichText::new("O gráfico do display usa seu próprio histórico. A prévia é ilustrativa.").size(11.).color(p.muted));
                }
            });
        });
    }

    pub(super) fn source_controls(&mut self, ui: &mut egui::Ui, p: Palette, width: f32) {
        let before = self.sensor_selection.clone();
        p.card().show(ui,|ui|{
            ui.set_width((width-42.).max(200.));
            widgets::section(ui,"Fontes dos sensores","Leituras locais do Linux para o display.",p);
            for (key,title,kind) in [(0,"Temperatura da CPU",Kind::Temperature),(1,"Ventoinha da CPU",Kind::Fan),(2,"Bomba",Kind::Fan)] {
                let selected=match key{0=>&mut self.sensor_selection.cpu_temp,1=>&mut self.sensor_selection.fan,_=>&mut self.sensor_selection.pump};
                let empty=if key==0{"Automático (sensor da CPU)"}else{"Não configurado"};
                let name=self.sources.iter().find(|source|Some(&source.id)==selected.as_ref()).map(|source|source.label.as_str()).unwrap_or(empty);
                ui.label(RichText::new(title).size(12.));
                egui::ComboBox::from_id_salt(("sensor",key)).width(ui.available_width()-4.).selected_text(name).show_ui(ui,|ui|{
                    ui.selectable_value(selected,None,empty);
                    for source in self.sources.iter().filter(|source|source.kind==kind){ui.selectable_value(selected,Some(source.id.clone()),&source.label);}
                });
            }
            ui.label(RichText::new("A ventoinha da GPU não é selecionada automaticamente como ventoinha da CPU ou bomba.").size(10.).color(p.muted));
        });
        if before != self.sensor_selection {
            let _ = self
                .worker
                .tx
                .send(worker::Request::Sources(self.sensor_selection.clone()));
        }
    }

    pub(super) fn live_summary(&self, ui: &mut egui::Ui, p: Palette) {
        ui.horizontal_wrapped(|ui| {
            for (label, reading) in [
                ("CPU", format!("{} °C", value(self.readings.cpu_temp, 1))),
                ("Uso", format!("{}%", value(self.readings.cpu_usage, 0))),
                ("RAM", format!("{}%", value(self.readings.ram_usage, 0))),
                (
                    "Clock",
                    format!("{} GHz", value(self.readings.cpu_mhz.map(|v| v / 1000.), 2)),
                ),
            ] {
                ui.label(RichText::new(label).size(11.).color(p.muted));
                ui.label(RichText::new(reading).size(12.).color(p.accent));
            }
        });
        ui.label(
            RichText::new(format!(
                "Fonte da CPU: {}",
                self.readings
                    .temp_source
                    .as_deref()
                    .unwrap_or("indisponível")
            ))
            .size(10.)
            .color(p.muted),
        );
    }

    pub(super) fn draw_preset(&self, painter: &egui::Painter, screen: Rect) {
        let mint = Color32::from_rgb(87, 226, 205);
        let orange = Color32::from_rgb(243, 169, 99);
        let inner = screen.shrink(12.);
        let factor = (screen.width() / 300.).clamp(0.55, 1.3);
        if self.mode == DisplayMode::Recorder {
            let mut curves = vec![];
            if self.preset.graph_clock {
                curves.push(("CPU FREQUENCY", false, mint));
            }
            if self.preset.graph_temperature {
                curves.push(("CPU TEMPERATURE", true, orange));
            }
            let rows = curves.len().max(1);
            for (index, (label, temp, color)) in curves.into_iter().enumerate() {
                let rect = Rect::from_min_max(
                    Pos2::new(
                        inner.left(),
                        inner.top() + inner.height() / rows as f32 * index as f32,
                    ),
                    Pos2::new(
                        inner.right(),
                        inner.top() + inner.height() / rows as f32 * (index + 1) as f32 - 6.,
                    ),
                );
                painter.text(
                    rect.left_top(),
                    Align2::LEFT_TOP,
                    label,
                    FontId::proportional(9. * factor),
                    color,
                );
                let chart = Rect::from_min_max(
                    rect.min + egui::vec2(0., 18. * factor),
                    rect.max - egui::vec2(55. * factor, 0.),
                );
                for i in 0..=4 {
                    let y = chart.top() + chart.height() * i as f32 / 4.;
                    painter.line_segment(
                        [Pos2::new(chart.left(), y), Pos2::new(chart.right(), y)],
                        Stroke::new(0.5_f32, Color32::from_gray(40)),
                    );
                }
                for i in 0..=8 {
                    let x = chart.left() + chart.width() * i as f32 / 8.;
                    painter.line_segment(
                        [Pos2::new(x, chart.top()), Pos2::new(x, chart.bottom())],
                        Stroke::new(0.5_f32, Color32::from_gray(40)),
                    );
                }
                let samples: Vec<_> = self
                    .history
                    .iter()
                    .map(|snapshot| {
                        if temp {
                            snapshot.cpu_temp
                        } else {
                            snapshot.cpu_mhz.map(|v| v / 1000.)
                        }
                    })
                    .collect();
                let maximum = if temp { 120. } else { 8. };
                let points: Vec<_> = samples
                    .iter()
                    .enumerate()
                    .filter_map(|(index, value)| {
                        value.map(|value| {
                            Pos2::new(
                                chart.left()
                                    + chart.width() * index as f32
                                        / (samples.len().max(2) - 1) as f32,
                                chart.bottom()
                                    - chart.height() * (value / maximum).clamp(0., 1.) as f32,
                            )
                        })
                    })
                    .collect();
                if points.len() > 1 {
                    painter.add(egui::Shape::line(points, Stroke::new(1.3_f32, color)));
                }
                let reading = if temp {
                    value(self.readings.cpu_temp, 0)
                } else {
                    value(self.readings.cpu_mhz.map(|v| v / 1000.), 2)
                };
                painter.text(
                    Pos2::new(rect.right(), rect.center().y),
                    Align2::RIGHT_CENTER,
                    reading,
                    FontId::proportional(21. * factor),
                    color,
                );
            }
        } else {
            let (title, reading, unit) = match self.preset.main {
                0 => (
                    "CPU FREQUENCY",
                    value(self.readings.cpu_mhz.map(|v| v / 1000.), 2),
                    "GHz",
                ),
                1 => ("TIME", chrono::Local::now().format("%H:%M").to_string(), ""),
                2 => ("PUMP SPEED", value(self.readings.pump_rpm, 0), "RPM"),
                3 => ("CPU FAN", value(self.readings.fan_rpm, 0), "RPM"),
                4 => (
                    "FAN / PUMP",
                    format!(
                        "{} / {}",
                        value(self.readings.fan_rpm, 0),
                        value(self.readings.pump_rpm, 0)
                    ),
                    "RPM",
                ),
                _ => ("CPU TEMPERATURE", value(self.readings.cpu_temp, 1), "°C"),
            };
            let center = Pos2::new(inner.left() + inner.width() * 0.38, inner.center().y);
            if self.preset.main == 0 && self.preset.style == 1 {
                painter.circle_stroke(center, inner.height() * 0.37, Stroke::new(3.0_f32, mint));
            }
            if self.preset.main == 0 && self.preset.style == 2 {
                painter.rect_stroke(
                    Rect::from_center_size(
                        center,
                        egui::vec2(inner.width() * 0.66, inner.height() * 0.75),
                    ),
                    8,
                    Stroke::new(1.5_f32, mint),
                    egui::StrokeKind::Inside,
                );
            }
            painter.text(
                center - egui::vec2(0., 44. * factor),
                Align2::CENTER_CENTER,
                title,
                FontId::proportional(10. * factor),
                mint,
            );
            painter.text(
                center,
                Align2::CENTER_CENTER,
                reading,
                FontId::proportional(if self.preset.main == 4 {
                    21. * factor
                } else {
                    37. * factor
                }),
                Color32::WHITE,
            );
            painter.text(
                center + egui::vec2(0., 35. * factor),
                Align2::CENTER_CENTER,
                unit,
                FontId::proportional(13. * factor),
                mint,
            );
            let auxiliary: Vec<_> = if self.preset.auxiliary == 0 {
                vec![
                    ("3.3V", value(self.readings.v33, 2)),
                    ("5V", value(self.readings.v5, 2)),
                    ("12V", value(self.readings.v12, 2)),
                ]
            } else {
                vec![
                    ("CPU °C", value(self.readings.cpu_temp, 0)),
                    ("CPU %", value(self.readings.cpu_usage, 0)),
                    ("RAM %", value(self.readings.ram_usage, 0)),
                ]
            };
            for (index, (label, reading)) in auxiliary.iter().enumerate() {
                let at = Pos2::new(
                    inner.right() - 2.,
                    inner.top() + 25. * factor + index as f32 * 43. * factor,
                );
                painter.text(
                    at,
                    Align2::RIGHT_CENTER,
                    reading,
                    FontId::proportional(18. * factor),
                    mint,
                );
                painter.text(
                    at + egui::vec2(0., 15. * factor),
                    Align2::RIGHT_CENTER,
                    label,
                    FontId::proportional(8. * factor),
                    Color32::from_gray(140),
                );
            }
        }
    }

    pub(super) fn preset_footer(&mut self, ctx: &egui::Context, p: Palette) {
        egui::TopBottomPanel::bottom("actions")
            .show_separator_line(false)
            .frame(
                egui::Frame::new()
                    .fill(p.card)
                    .inner_margin(egui::Margin::symmetric(20, 8)),
            )
            .show(ctx, |ui| {
                let left = (ui.available_width() - 350.).max(200.);
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        egui::vec2(left, 40.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_min_width(left);
                            ui.spacing_mut().item_spacing.y = 3.;
                            ui.label(
                                RichText::new(if self.monitoring.is_some() {
                                    "Sensores atualizados a cada segundo"
                                } else {
                                    "Preset pronto para ativar"
                                })
                                .size(11.)
                                .strong()
                                .color(p.accent),
                            );
                            ui.add(
                                egui::Label::new(RichText::new(&self.status).size(10.).color(
                                    if self.error {
                                        Color32::from_rgb(198, 92, 71)
                                    } else {
                                        p.muted
                                    },
                                ))
                                .wrap(),
                            );
                        },
                    );
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(self.task.is_none(), |ui| {
                            let label = if self.monitoring == Some(self.mode) {
                                "Aplicar preset"
                            } else if self.mode == DisplayMode::Recorder {
                                "Ativar Recording"
                            } else {
                                "Ativar Monitoring"
                            };
                            if ui
                                .add_sized([196., 32.], egui::Button::new(label).fill(p.soft))
                                .clicked()
                            {
                                self.operation(Task::Monitor);
                            }
                            ui.add_enabled_ui(self.monitoring.is_some(), |ui| {
                                if ui
                                    .add_sized([124., 32.], egui::Button::new("Parar atualização"))
                                    .clicked()
                                {
                                    self.operation(Task::Stop);
                                }
                            });
                        });
                    });
                });
            });
    }
}
