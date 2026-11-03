//! Barra flutuante de tamanho fixo; apenas transferências têm porcentagem.
use super::{Task, theme::Palette};
use eframe::egui::{self, Align2, FontId, Rect, Sense, Vec2};

pub(super) const ROW_HEIGHT: f32 = 28.;

pub(super) fn show(ui: &mut egui::Ui, p: Palette, task: Option<Task>, progress: f32) {
    let (rect, response) =
        ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW_HEIGHT), Sense::hover());
    let text = match task {
        Some(Task::Prepare) => "Preparando mídia…".into(),
        Some(Task::Upload) if progress <= 0. => "Preparando orientação e arquivos…".into(),
        Some(Task::Upload) => format!(
            "Salvando no display · {}%",
            (progress.clamp(0., 1.) * 100.) as u32
        ),
        Some(Task::Apply) => "Aplicando tela e iluminação…".into(),
        Some(Task::LoadPreview | Task::Select) => "Selecionando mídia…".into(),
        Some(_) => "Trabalhando…".into(),
        None => String::new(),
    };
    let painter = ui.painter();
    let transferring = matches!(task, Some(Task::Prepare | Task::Upload));
    if transferring {
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::ProgressIndicator, true, &text)
        });
        painter.rect_filled(rect, 14, p.field);
    }
    let font = FontId::proportional(13.);
    let anchor = if transferring {
        Align2::CENTER_CENTER
    } else {
        Align2::LEFT_CENTER
    };
    let position = if transferring {
        rect.center()
    } else {
        rect.left_center()
    };
    if task == Some(Task::Prepare) || (task == Some(Task::Upload) && progress <= 0.) {
        let width = rect.width() * 0.28;
        let phase = ui.input(|input| (input.time * 0.65).fract()) as f32;
        let left = rect.left() - width + phase * (rect.width() + width);
        painter.with_clip_rect(rect.shrink(1.)).rect_filled(
            Rect::from_min_size(
                egui::pos2(left, rect.top()),
                Vec2::new(width, rect.height()),
            ),
            14,
            p.accent.gamma_multiply(0.22),
        );
        ui.ctx().request_repaint();
    }
    let filled = if task == Some(Task::Upload) {
        let filled = Rect::from_min_size(
            rect.min,
            Vec2::new(rect.width() * progress.clamp(0., 1.), rect.height()),
        );
        painter.rect_filled(filled, 14, p.accent);
        Some(filled)
    } else {
        None
    };
    painter.text(
        position,
        anchor,
        &text,
        font.clone(),
        if transferring { p.text } else { p.accent },
    );
    if let Some(filled) = filled {
        painter
            .with_clip_rect(filled)
            .text(position, anchor, text, font, p.on_accent());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_does_not_move_content_between_idle_loading_and_transfer_states() {
        let ctx = egui::Context::default();
        let mut positions = vec![];
        for task in [
            None,
            Some(Task::LoadPreview),
            Some(Task::Prepare),
            Some(Task::Apply),
            Some(Task::Upload),
        ] {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    show(ui, Palette::new(true), task, 0.36);
                    positions.push(ui.label("mesma posição").rect.top());
                });
            });
        }
        assert!(positions.iter().all(|position| *position == positions[0]));
    }
}
