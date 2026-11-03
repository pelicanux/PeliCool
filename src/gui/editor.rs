//! Editor de enquadramento. Confirmar prepara e salva; cancelar não toca no USB.
use super::{App, Event, Task, orientation_label, theme::Palette};
use eframe::egui::{
    self, Align2, Color32, FontId, Rect, RichText, Sense, Stroke, StrokeKind, Vec2,
};
use pelicool::media::{Fit, Framing, Orientation};
use std::path::PathBuf;

pub(super) struct Editor {
    path: PathBuf,
    replace: Option<usize>,
    texture: egui::TextureHandle,
    orientation: Orientation,
    fit: Fit,
    framing: Framing,
    display_rotation: Option<u16>,
}
impl Editor {
    pub(super) fn new(
        ctx: &egui::Context,
        path: PathBuf,
        image: image::RgbaImage,
        replace: Option<usize>,
        options: pelicool::media::PrepareOptions,
    ) -> Self {
        let texture = ctx.load_texture(
            "crop-source",
            egui::ColorImage::from_rgba_unmultiplied(
                [image.width() as usize, image.height() as usize],
                image.as_raw(),
            ),
            egui::TextureOptions::LINEAR,
        );
        Self {
            path,
            replace,
            texture,
            orientation: options.orientation,
            fit: options.fit,
            framing: options.framing,
            display_rotation: options.display_rotation,
        }
    }
}
impl App {
    pub(super) fn choose_media(&mut self, path: PathBuf, replace: Option<usize>) {
        self.startup.cancel();
        let animated = replace
            .and_then(|index| self.library.get(index))
            .map_or_else(|| super::is_gif(&path), |item| item.entry.animated);
        if replace.is_none()
            && self
                .library
                .iter()
                .filter(|item| item.entry.animated == animated)
                .count()
                >= self.media_limit(animated)
        {
            self.error = true;
            self.status="Os cinco espaços estão ocupados. Remova uma mídia desse tipo para adicionar outra.".into();
            return;
        }
        self.task = Some(Task::Prepare);
        self.error = false;
        self.status = "Abrindo mídia para ajustar o enquadramento…".into();
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = pelicool::media::source_preview(&path);
            let _ = tx.send(match result {
                Ok(image) => Event::EditorLoaded(path, image, replace),
                Err(error) => Event::Error(format!("Não foi possível abrir a mídia: {error:#}")),
            });
        });
    }

    pub(super) fn editor_dialog(&mut self, ctx: &egui::Context, p: Palette) {
        let native = self.module.info.screen_size.unwrap_or((640, 480));
        let Some(editor) = self.editor.as_mut() else {
            return;
        };
        let mut save = false;
        let mut cancel = false;
        let modal=egui::Modal::new(egui::Id::new("media-editor")).frame(p.card().inner_margin(18)).show(ctx,|ui|{
            ui.set_width(650.);
            ui.label(RichText::new("Ajustar mídia").size(22.));
            ui.label(RichText::new("Arraste para enquadrar. Use o zoom para escolher o trecho que aparecerá no display.").size(12.).color(p.muted));
            ui.horizontal(|ui|{
                if let Some(angle)=editor.display_rotation {ui.label(format!("Orientação: {angle}° · recorte antes do giro"));} else {egui::ComboBox::from_id_salt("editor-orientation").selected_text(orientation_label(editor.orientation)).show_ui(ui,|ui|{
                    for value in [Orientation::Auto,Orientation::Horizontal,Orientation::Vertical] {ui.selectable_value(&mut editor.orientation,value,orientation_label(value));}
                });}
                ui.selectable_value(&mut editor.fit,Fit::Cover,"Recortar");
                ui.selectable_value(&mut editor.fit,Fit::Contain,"Imagem inteira");
            });
            let (area,response)=ui.allocate_exact_size(egui::vec2(ui.available_width(),300.),Sense::drag());
            let source=editor.texture.size_vec2();
            let horizontal=editor.display_rotation.map_or_else(||match editor.orientation {Orientation::Auto=>source.x>source.y,Orientation::Horizontal=>true,Orientation::Vertical=>false},|angle|angle.is_multiple_of(180));
            let (w,h)=if horizontal {(native.0.max(native.1),native.0.min(native.1))}else{(native.0.min(native.1),native.0.max(native.1))};
            let ratio=f32::from(w)/f32::from(h);
            let screen=Rect::from_center_size(area.center(),egui::vec2(240.*ratio,240.));
            let scale=if editor.fit==Fit::Cover {(screen.width()/source.x).max(screen.height()/source.y)*editor.framing.zoom}
                else {(screen.width()/source.x).min(screen.height()/source.y)};
            let size=source*scale;
            if editor.fit==Fit::Cover {
                if response.dragged() {
                    let delta=ui.input(|input|input.pointer.delta());
                    if size.x>screen.width() {editor.framing.x=(editor.framing.x-delta.x/(size.x-screen.width())).clamp(0.,1.);}
                    if size.y>screen.height() {editor.framing.y=(editor.framing.y-delta.y/(size.y-screen.height())).clamp(0.,1.);}
                }
                if response.hovered() {
                    let wheel=ui.input(|input|input.smooth_scroll_delta.y);
                    editor.framing.zoom=(editor.framing.zoom*(wheel*0.002).exp()).clamp(1.,8.);
                }
            }
            let origin=if editor.fit==Fit::Cover {screen.min-egui::vec2((size.x-screen.width())*editor.framing.x,(size.y-screen.height())*editor.framing.y)} else {screen.center()-size/2.};
            let image=Rect::from_min_size(origin,size);
            let painter=ui.painter().with_clip_rect(area);
            painter.rect_filled(area,12,Color32::from_rgb(15,22,27));
            painter.image(editor.texture.id(),image,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.,1.)),Color32::WHITE.gamma_multiply(0.25));
            painter.with_clip_rect(screen).image(editor.texture.id(),image,Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1.,1.)),Color32::WHITE);
            painter.rect_stroke(screen,0,Stroke::new(2.0_f32,p.accent),StrokeKind::Outside);
            painter.text(screen.left_top()+egui::vec2(8.,8.),Align2::LEFT_TOP,format!("{w} × {h}"),FontId::proportional(11.),Color32::WHITE);
            ui.horizontal(|ui|{
                ui.add_enabled(editor.fit==Fit::Cover,egui::Slider::new(&mut editor.framing.zoom,1.0..=8.0).text("Zoom").suffix("×"));
                if ui.button("Centralizar").clicked(){editor.framing=Framing::default();}
            });
            ui.label(RichText::new("GIFs grandes são adaptados automaticamente. As outras mídias salvas serão mantidas.").size(11.).color(p.muted));
            ui.horizontal(|ui|{
                cancel=ui.button("Cancelar").clicked();
                save=ui.add(egui::Button::new(RichText::new("Salvar no display").color(if p.dark {Color32::from_rgb(9,40,32)}else{Color32::WHITE})).fill(p.accent).min_size(Vec2::new(180.,32.))).clicked();
            });
        });
        if modal.should_close() {
            cancel = true;
        }
        if save {
            let editor = self.editor.take().expect("editor");
            self.orientation = editor.orientation;
            self.fit = editor.fit;
            self.framing = editor.framing;
            self.stage(editor.path, editor.replace);
        } else if cancel {
            self.editor = None;
            self.status = "Edição cancelada. Nenhuma mídia foi alterada.".into();
        }
    }
}
