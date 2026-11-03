use super::rotation::PreviewRotation;
use super::theme::Palette;
use eframe::egui::{
    self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2,
};

#[derive(Clone, Copy)]
pub enum Icon {
    Gear,
    Screen,
    Device,
    Plus,
    Sun,
    Moon,
    Play,
    Pause,
    Link,
    Settings,
}

pub fn icon(painter: &egui::Painter, rect: Rect, kind: Icon, color: Color32) {
    let center = rect.center();
    let scale = rect.width() / 24.0;
    let at = |x: f32, y: f32| center + egui::vec2((x - 12.0) * scale, (y - 12.0) * scale);
    let stroke = Stroke::new(1.65 * scale, color);
    let line = |points: &[(f32, f32)]| {
        painter.add(egui::Shape::line(
            points.iter().map(|&(x, y)| at(x, y)).collect(),
            stroke,
        ));
    };
    let outline = |x1, y1, x2, y2, r| {
        painter.rect_stroke(
            Rect::from_min_max(at(x1, y1), at(x2, y2)),
            r,
            stroke,
            StrokeKind::Inside,
        );
    };
    match kind {
        Icon::Screen => {
            outline(3., 4., 21., 17., 2);
            line(&[(8., 21.), (16., 21.)]);
            line(&[(12., 17.), (12., 21.)]);
        }
        Icon::Device => {
            outline(5., 3., 19., 21., 2);
            line(&[(9., 7.), (15., 7.)]);
            painter.circle_filled(at(12., 17.), scale, color);
        }
        Icon::Plus => {
            line(&[(5., 12.), (19., 12.)]);
            line(&[(12., 5.), (12., 19.)]);
        }
        Icon::Play => {
            painter.add(egui::Shape::convex_polygon(
                vec![at(8., 4.), at(20., 12.), at(8., 20.)],
                color,
                Stroke::NONE,
            ));
        }
        Icon::Pause => {
            painter.rect_filled(Rect::from_min_max(at(7., 5.), at(10., 19.)), 1, color);
            painter.rect_filled(Rect::from_min_max(at(14., 5.), at(17., 19.)), 1, color);
        }
        Icon::Sun => {
            painter.circle_stroke(center, 4. * scale, stroke);
            for index in 0..8 {
                let angle = index as f32 * std::f32::consts::TAU / 8.;
                let d = egui::vec2(angle.cos(), angle.sin());
                painter.line_segment([center + d * 7. * scale, center + d * 10. * scale], stroke);
            }
        }
        Icon::Moon => {
            line(&[
                (17., 3.),
                (11., 4.),
                (6., 8.),
                (5., 14.),
                (8., 19.),
                (14., 21.),
                (20., 18.),
                (15., 17.),
                (11., 13.),
                (11., 8.),
                (14., 4.),
                (17., 3.),
            ]);
        }
        Icon::Link => {
            line(&[
                (10., 7.),
                (12., 5.),
                (17., 5.),
                (20., 8.),
                (20., 12.),
                (16., 16.),
            ]);
            line(&[
                (14., 17.),
                (12., 19.),
                (7., 19.),
                (4., 16.),
                (4., 12.),
                (8., 8.),
            ]);
            line(&[(9., 15.), (15., 9.)]);
        }
        Icon::Gear => {
            let center = at(12., 12.);
            painter.circle_stroke(center, 6.8 * scale, stroke);
            painter.circle_stroke(center, 2.5 * scale, stroke);
            for tooth in 0..8 {
                let angle = tooth as f32 * std::f32::consts::TAU / 8.;
                let direction = egui::vec2(angle.cos(), angle.sin());
                painter.line_segment(
                    [
                        center + direction * 7. * scale,
                        center + direction * 10. * scale,
                    ],
                    Stroke::new(3.0_f32 * scale, color),
                );
            }
        }
        Icon::Settings => {
            for (y, x) in [(6., 9.), (12., 16.), (18., 8.)] {
                line(&[(3., y), (21., y)]);
                painter.circle_filled(at(x, y), 2.6 * scale, color);
            }
        }
    }
}

pub fn toggle(ui: &mut Ui, value: &mut bool, label: &str, p: Palette) -> Response {
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.), Sense::click());
    if response.clicked() {
        *value = !*value;
        response.mark_changed();
    }
    let track = Rect::from_center_size(
        Pos2::new(rect.right() - 20., rect.center().y),
        egui::vec2(36., 20.),
    );
    ui.painter()
        .rect_filled(track, 10, if *value { p.accent } else { p.border });
    ui.painter().circle_filled(
        Pos2::new(
            if *value {
                track.right() - 10.
            } else {
                track.left() + 10.
            },
            track.center().y,
        ),
        7.,
        Color32::WHITE,
    );
    ui.painter().text(
        rect.left_center(),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.),
        if ui.is_enabled() { p.text } else { p.muted },
    );
    response.widget_info(|| {
        egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *value, label)
    });
    response
}

pub fn rotation(ui: &mut Ui, angle: u16, selected: bool, p: Palette) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(55., 58.), Sense::click());
    ui.painter()
        .rect_filled(rect, 7, if selected { p.soft } else { p.field });
    let center = rect.center() - egui::vec2(0., 8.);
    let rotation = PreviewRotation::relative(angle);
    let size = rotation.size(egui::vec2(28., 20.));
    let screen = Rect::from_center_size(center, size);
    let color = if selected { p.accent } else { p.muted };
    ui.painter()
        .rect_stroke(screen, 3, Stroke::new(1.2_f32, color), StrokeKind::Inside);
    let tip = center + rotation.vector(egui::vec2(0., -6.));
    let bottom = center + rotation.vector(egui::vec2(0., 6.));
    ui.painter()
        .arrow(bottom, tip - bottom, Stroke::new(1.4_f32, color));
    ui.painter().text(
        Pos2::new(rect.center().x, rect.bottom() - 9.),
        Align2::CENTER_CENTER,
        format!("{angle}°"),
        FontId::proportional(10.),
        color,
    );
    response.on_hover_text(format!("{angle}° · {}", rotation.description(true)))
}

pub fn icon_button(
    ui: &mut Ui,
    kind: Icon,
    label: &str,
    fill: Color32,
    color: Color32,
    width: f32,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 32.), Sense::click());
    let fill = if response.hovered() {
        fill.gamma_multiply(0.93)
    } else {
        fill
    };
    ui.painter().rect_filled(
        rect,
        8,
        if ui.is_enabled() {
            fill
        } else {
            fill.gamma_multiply(0.5)
        },
    );
    let color = if ui.is_enabled() {
        color
    } else {
        color.gamma_multiply(0.45)
    };
    icon(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(rect.left() + 21., rect.center().y),
            Vec2::splat(18.),
        ),
        kind,
        color,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 40., rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.),
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn section(ui: &mut Ui, title: &str, subtitle: &str, palette: Palette) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        ui.label(
            egui::RichText::new(title)
                .size(16.)
                .strong()
                .color(palette.text),
        );
        if !subtitle.is_empty() {
            ui.label(egui::RichText::new(subtitle).size(12.).color(palette.muted));
        }
    });
}

pub fn pill(ui: &mut Ui, text: &str, color: Color32, background: Color32) {
    egui::Frame::new()
        .fill(background)
        .corner_radius(20)
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 14.0;
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(7.), Sense::hover());
                ui.painter().circle_filled(r.center(), 3., color);
                ui.label(egui::RichText::new(text).size(11.).color(color));
            });
        });
}

pub fn nav(ui: &mut Ui, kind: Icon, text: &str, selected: bool) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 38.), Sense::click());
    let color = if selected {
        Color32::from_rgb(97, 230, 202)
    } else {
        Color32::from_rgb(152, 172, 183)
    };
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            9,
            if selected {
                Color32::from_rgb(24, 56, 57)
            } else {
                Color32::from_rgb(24, 37, 46)
            },
        );
    }
    icon(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(rect.left() + 22., rect.center().y),
            Vec2::splat(20.),
        ),
        kind,
        color,
    );
    ui.painter().text(
        Pos2::new(rect.left() + 43., rect.center().y),
        Align2::LEFT_CENTER,
        text,
        FontId::proportional(13.),
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
