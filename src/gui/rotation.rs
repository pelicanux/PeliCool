use eframe::egui::{self, Pos2, Rect, Vec2};

/// Rotação visual relativa à marca do bloco. Settings preserva o ângulo bruto;
/// o módulo converte esse valor antes de desenhar moldura, conteúdo e ícones.
#[derive(Clone, Copy)]
pub(super) struct PreviewRotation {
    degrees: u16,
}

impl PreviewRotation {
    pub(super) fn relative(degrees: u16) -> Self {
        Self {
            degrees: degrees % 360,
        }
    }
    pub(super) fn vector(self, v: Vec2) -> Vec2 {
        match self.degrees {
            90 => egui::vec2(-v.y, v.x),
            180 => -v,
            270 => egui::vec2(v.y, -v.x),
            _ => v,
        }
    }

    pub(super) fn size(self, size: Vec2) -> Vec2 {
        if self.degrees.is_multiple_of(180) {
            size
        } else {
            egui::vec2(size.y, size.x)
        }
    }

    pub(super) fn description(self, horizontal: bool) -> String {
        let size = self.size(if horizontal {
            egui::vec2(4., 3.)
        } else {
            egui::vec2(3., 4.)
        });
        let format = if size.x > size.y {
            "Horizontal"
        } else {
            "Vertical"
        };
        let top = match self.degrees {
            90 => "topo para a direita",
            180 => "topo para baixo",
            270 => "topo para a esquerda",
            _ => "topo para cima",
        };
        format!("{format} · {top}")
    }

    pub(super) fn image(self, painter: &egui::Painter, texture: egui::TextureId, content: Rect) {
        let mut mesh = egui::Mesh::with_texture(texture);
        mesh.add_rect_with_uv(
            content,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1., 1.)),
            egui::Color32::WHITE,
        );
        for vertex in &mut mesh.vertices {
            vertex.pos = content.center() + self.vector(vertex.pos - content.center());
        }
        painter.add(mesh);
    }

    /// Rotate the preset's tessellated geometry so text, charts and media use
    /// exactly the same transform. The LCD frame is painted separately.
    pub(super) fn paint(self, painter: &egui::Painter, center: Pos2, draw: impl FnOnce()) {
        if self.degrees == 0 {
            draw();
            return;
        }
        let ctx = painter.ctx();
        let layer = painter.layer_id();
        let start = ctx.graphics_mut(|graphics| graphics.entry(layer).next_idx());
        draw();
        let shapes = ctx.graphics_mut(|graphics| {
            let list = graphics.entry(layer);
            let shapes: Vec<_> = list.all_entries().skip(start.0).cloned().collect();
            for index in start.0..list.next_idx().0 {
                list.reset_shape(egui::layers::ShapeIdx(index));
            }
            shapes
        });
        for primitive in ctx.tessellate(shapes, ctx.pixels_per_point()) {
            if let egui::epaint::Primitive::Mesh(mut mesh) = primitive.primitive {
                for vertex in &mut mesh.vertices {
                    vertex.pos = center + self.vector(vertex.pos - center);
                }
                painter.add(mesh);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_and_content_share_the_angle_relative_to_the_block_brand() {
        let info = &pelicool::drivers::mystique::INFO;
        for (display, raw, size, top) in [
            (0, 270, egui::vec2(4., 3.), egui::vec2(0., -1.)),
            (90, 0, egui::vec2(3., 4.), egui::vec2(1., 0.)),
            (180, 90, egui::vec2(4., 3.), egui::vec2(0., 1.)),
            (270, 180, egui::vec2(3., 4.), egui::vec2(-1., 0.)),
        ] {
            assert_eq!(info.raw_rotation(display), raw);
            assert_eq!(info.display_rotation(raw), display);
            let rotation = PreviewRotation::relative(display);
            assert_eq!(rotation.size(egui::vec2(4., 3.)), size);
            assert_eq!(rotation.vector(egui::vec2(0., -1.)), top);
        }
    }
}
