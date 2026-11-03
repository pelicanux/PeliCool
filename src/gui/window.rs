//! Fechamento, bandeja e restauração da janela.
use super::{
    App, Task, WindowState, brand,
    closing::{Choice, Effect},
    theme::Palette,
    tray,
};
use eframe::egui::{self, RichText};
use std::time::Instant;

impl App {
    pub(super) fn window_effect(&self, ctx: &egui::Context, effect: Effect) {
        use egui::ViewportCommand as Command;
        match effect {
            Effect::None => (),
            Effect::Hide => ctx.send_viewport_cmd(Command::Visible(false)),
            Effect::Minimize => ctx.send_viewport_cmd(Command::Minimized(true)),
            Effect::Show => {
                ctx.send_viewport_cmd(Command::Visible(true));
                ctx.send_viewport_cmd(Command::Minimized(false));
                ctx.send_viewport_cmd(Command::Focus);
            }
            Effect::Exit => ctx.send_viewport_cmd(Command::Close),
        }
    }

    pub(super) fn desktop_events(&mut self, ctx: &egui::Context) {
        while let Ok(action) = self.tray.actions.try_recv() {
            let effect = match action {
                tray::Action::Show => self.window_state.restore(),
                tray::Action::Minimize => self.window_state.choose(
                    Choice::Minimize,
                    self.tray.available(),
                    self.task == Some(Task::Upload),
                ),
                tray::Action::Quit => self.window_state.choose(
                    Choice::Exit,
                    self.tray.available(),
                    self.task == Some(Task::Upload),
                ),
            };
            self.window_effect(ctx, effect);
        }
        if self.start_minimized && Instant::now() >= self.capture_after {
            self.start_minimized = false;
            let effect = self
                .window_state
                .choose(Choice::Minimize, self.tray.available(), false);
            self.window_effect(ctx, effect);
        }
        let effect = self.window_state.reconcile_tray(self.tray.available());
        self.window_effect(ctx, effect);
        if ctx.input(|input| input.viewport().close_requested())
            && self.window_state.request_close()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.preferences.minimize_on_close {
                let effect = self.window_state.choose(
                    Choice::Minimize,
                    self.tray.available(),
                    self.task == Some(Task::Upload),
                );
                self.window_effect(ctx, effect);
            } else {
                self.window_effect(ctx, Effect::Show);
            }
        }
    }

    pub(super) fn close_dialog(&mut self, ctx: &egui::Context, p: Palette) {
        if self.window_state != WindowState::Asking {
            return;
        }
        let tray = self.tray.available();
        let uploading = self.task == Some(Task::Upload);
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("close-pelicool"))
            .frame(p.card().inner_margin(22))
            .show(ctx, |ui| {
                ui.set_width(420.);
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new(brand::logo()).max_width(56.).max_height(52.).maintain_aspect_ratio(true));
                    ui.vertical(|ui| {
                        ui.label(RichText::new("O que deseja fazer?").size(21.).strong());
                        ui.label(RichText::new(brand::NAME).size(12.).color(p.muted));
                    });
                });
                ui.add_space(12.);
                ui.label("Minimizar mantém as funções ativas. Sair encerra a atualização dos sensores e o slideshow.");
                ui.add_space(9.);
                ui.label(RichText::new(if tray {
                    "A janela poderá ser reaberta pelo ícone do PeliCool na bandeja."
                } else {
                    "Sem integração com a bandeja neste modo. A janela ficará acessível pela visão de atividades ou pelo painel do sistema."
                }).size(12.).color(p.muted));
                if uploading {
                    ui.add_space(9.);
                    ui.label(RichText::new("Envio em andamento. Você pode minimizar; sair ficará disponível quando o envio terminar.").size(12.).color(p.accent));
                }
                ui.add_space(10.);
                ui.checkbox(&mut self.remember_close,"Lembrar esta opção: minimizar ao fechar");
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    if ui.add_sized([92., 34.], egui::Button::new("Cancelar")).clicked() {
                        choice = Some(Choice::Cancel);
                    }
                    if ui.add_enabled(!uploading, egui::Button::new("Sair").min_size(egui::vec2(80., 34.))).clicked() {
                        choice = Some(Choice::Exit);
                    }
                    let label = if tray { "Minimizar na bandeja" } else { "Minimizar janela" };
                    if ui.add_sized([228., 34.], egui::Button::new(RichText::new(label).color(p.text).strong()).fill(p.soft)).clicked() {
                        choice = Some(Choice::Minimize);
                    }
                });
            });
        if modal.should_close() && choice.is_none() {
            choice = Some(Choice::Cancel);
        }
        if let Some(choice) = choice {
            if matches!(choice, Choice::Minimize) && self.remember_close {
                self.preferences.minimize_on_close = true;
                self.save_interface();
            }
            let effect = self
                .window_state
                .choose(choice, self.tray.available(), uploading);
            self.window_effect(ctx, effect);
        }
    }
}
