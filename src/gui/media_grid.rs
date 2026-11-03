use super::{App, Icon, theme::Palette, widgets};
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};

enum Action {
    Select(usize),
    Remove(usize),
    Add(bool),
}

impl App {
    pub(super) fn media_grid(&mut self, ui: &mut egui::Ui, p: Palette) {
        p.card().inner_margin(10).show(ui, |ui| {
            ui.set_width((ui.available_width() - 2.).max(200.));
            ui.spacing_mut().item_spacing = egui::vec2(10., 5.);
            let mut action = None;
            ui.add_enabled_ui(self.task.is_none(), |ui| {
                ui.columns(2, |columns| {
                    for (column, animated) in columns.iter_mut().zip([false, true]) {
                        let limit = self.media_limit(animated);
                        let indices: Vec<_> = self
                            .library
                            .iter()
                            .enumerate()
                            .filter(|(_, item)| item.entry.animated == animated)
                            .map(|(index, _)| index)
                            .collect();
                        column.horizontal(|ui| {
                            ui.label(
                                RichText::new(if animated { "GIFs" } else { "Imagens" })
                                    .strong()
                                    .size(12.),
                            );
                            ui.label(
                                RichText::new(format!("{} / {limit}", indices.len()))
                                    .size(10.)
                                    .color(p.muted),
                            );
                        });
                        let side = ((column.available_width() - 24.) / 5.).min(56.);
                        column.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.;
                            for position in 0..limit {
                                let (rect, response) =
                                    ui.allocate_exact_size(Vec2::splat(side), Sense::click());
                                let painter = ui.painter();
                                let item = indices.get(position).copied();
                                let selected = item.is_some() && item == self.selected;
                                painter.rect_filled(rect, 7, p.field);
                                painter.rect_stroke(
                                    rect,
                                    7,
                                    Stroke::new(
                                        if selected { 1.7_f32 } else { 1.0_f32 },
                                        if selected { p.accent } else { p.border },
                                    ),
                                    StrokeKind::Inside,
                                );
                                if let Some(index) = item {
                                    let item = &self.library[index];
                                    let saved = self.catalog.slot(&item.entry.id);
                                    let playing = saved.is_some() && saved == self.shown;
                                    if let Some(texture) = &item.thumbnail {
                                        let area = rect.shrink(4.);
                                        let size = texture.size_vec2();
                                        let size = size
                                            * (area.width() / size.x).min(area.height() / size.y);
                                        painter.image(
                                            texture.id(),
                                            Rect::from_center_size(area.center(), size),
                                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
                                            Color32::WHITE,
                                        );
                                    }
                                    if playing {
                                        let badge = Rect::from_min_size(
                                            rect.min + egui::vec2(3., 3.),
                                            egui::vec2(side - 6., 14.),
                                        );
                                        painter.rect_filled(
                                            badge,
                                            3,
                                            Color32::from_black_alpha(180),
                                        );
                                        painter.text(
                                            badge.center(),
                                            Align2::CENTER_CENTER,
                                            "Exibindo",
                                            FontId::proportional(8.),
                                            Color32::WHITE,
                                        );
                                    }
                                    if response.clicked() {
                                        action = Some(Action::Select(index));
                                    }
                                    response.clone().context_menu(|ui| {
                                        if ui.button("Remover do display").clicked() {
                                            action = Some(Action::Remove(index));
                                            ui.close_menu();
                                        }
                                    });
                                    response.on_hover_text(format!(
                                        "{}\n{}",
                                        item.entry.name,
                                        if saved.is_some() {
                                            "Clique para exibir sem reenviar."
                                        } else {
                                            "Clique para tentar salvar esta mídia novamente."
                                        }
                                    ));
                                } else {
                                    widgets::icon(
                                        painter,
                                        Rect::from_center_size(rect.center(), Vec2::splat(20.)),
                                        Icon::Plus,
                                        p.muted,
                                    );
                                    if response
                                        .on_hover_text(if animated {
                                            "Adicionar GIF"
                                        } else {
                                            "Adicionar imagem"
                                        })
                                        .clicked()
                                    {
                                        action = Some(Action::Add(animated));
                                    }
                                }
                            }
                        });
                    }
                });
            });
            ui.label(
                RichText::new("+ adiciona · miniatura troca a tela · botão direito remove")
                    .size(10.)
                    .color(p.muted),
            );
            match action {
                Some(Action::Select(index)) => self.select_cached(index),
                Some(Action::Add(animated)) => self.import_kind(animated),
                Some(Action::Remove(index)) => {
                    let previous = self
                        .selected
                        .and_then(|selected| self.library.get(selected))
                        .map(|item| item.entry.id.clone());
                    let removed = self.library.remove(index);
                    self.catalog.entries.remove(index);
                    self.selected = previous
                        .as_ref()
                        .and_then(|id| self.library.iter().position(|item| item.entry.id == *id))
                        .or_else(|| {
                            (!self.library.is_empty())
                                .then_some(index.min(self.library.len().saturating_sub(1)))
                        });
                    if previous.as_ref() == Some(&removed.entry.id) {
                        self.media = None;
                        self.texture = None;
                        self.path = None;
                    }
                    self.status = "Removendo mídia do display…".into();
                    if let Err(error) = self.cache.save(&self.catalog) {
                        self.error = true;
                        self.status = format!("Erro ao salvar: {error:#}");
                    } else {
                        self.operation(super::Task::Upload);
                    }
                }
                None => {}
            }
        });
    }
}
