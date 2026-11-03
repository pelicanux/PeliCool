//! Aviso inicial e manutenção das permissões; nenhum pedido de senha automático.
use super::{App, Event, theme::Palette, worker};
use eframe::egui::{self, RichText};
use pelicool::permissions::{self, Action, RuleState};
use std::{path::Path, time::Instant};

pub(super) struct Permissions {
    rule: RuleState,
    access: Option<bool>,
    pub dialog: Option<Action>,
    pub busy: bool,
    message: String,
}
impl Permissions {
    pub fn new(info: &pelicool::drivers::DriverInfo, capture: bool) -> Self {
        let rule = permissions::rule_state(Path::new("/etc/udev/rules.d"));
        let access = if capture {
            None
        } else {
            permissions::usb_access(info)
        };
        let mut state = Self {
            rule,
            access,
            dialog: None,
            busy: false,
            message: String::new(),
        };
        if !capture && (rule == RuleState::Missing || access == Some(false)) {
            state.dialog = Some(Action::Install);
        }
        state
    }
    pub fn refresh(&mut self, info: &pelicool::drivers::DriverInfo) {
        self.rule = permissions::rule_state(Path::new("/etc/udev/rules.d"));
        self.access = permissions::usb_access(info);
    }
    fn editable(&self) -> bool {
        matches!(
            self.rule,
            RuleState::Missing | RuleState::Installed | RuleState::Legacy
        )
    }
    fn dismiss(&mut self) {
        self.dialog = None;
    }
}

impl App {
    pub(super) fn permission_card(&mut self, ui: &mut egui::Ui, p: Palette) {
        p.card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.;
            ui.label(RichText::new("Acesso USB sem root").size(17.));
            ui.label(match self.permissions.rule {
                RuleState::Installed => "Regra udev PeliCool instalada.",
                RuleState::Legacy => "Regra de uma versão anterior encontrada. Você pode atualizá-la.",
                RuleState::Missing => "A regra udev PeliCool ainda não está instalada.",
                RuleState::Modified => "Uma regra foi modificada ou redirecionada. Revise-a manualmente; ela será preservada.",
                RuleState::Unreadable => "Não foi possível ler as regras. Confira as permissões da pasta do sistema.",
            });
            ui.label(RichText::new(match self.permissions.access {
                Some(true) => "O usuário atual tem acesso ao dispositivo.",
                Some(false) => "O dispositivo foi detectado, mas o usuário atual não tem acesso USB.",
                None => "Conecte o dispositivo para verificar o acesso USB.",
            }).size(11.).color(p.muted));
            ui.add_enabled_ui(!self.permissions.busy && self.task.is_none(), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if ui.add_enabled(self.permissions.editable(), egui::Button::new(
                        if self.permissions.rule == RuleState::Installed { "Reinstalar regra udev" } else { "Instalar regra udev" }
                    )).clicked() { self.permissions.dialog = Some(Action::Install); }
                    if ui.add_enabled(matches!(self.permissions.rule, RuleState::Installed | RuleState::Legacy),
                        egui::Button::new("Remover regra udev")).clicked() { self.permissions.dialog = Some(Action::Remove); }
                    if ui.button("Verificar acesso").clicked() { self.permissions.refresh(self.module.info); }
                });
            });
            ui.label(RichText::new("Só a instalação ou remoção pede autenticação administrativa. O PeliCool continua sem root.").size(11.).color(p.muted));
            if self.permissions.busy { ui.horizontal(|ui| { ui.spinner(); ui.label("Aguardando autenticação do sistema…"); }); }
            if !self.permissions.message.is_empty() { ui.label(RichText::new(&self.permissions.message).size(12.).color(p.muted)); }
            ui.collapsing("Alternativa pelo terminal", |ui| {
                ui.label("Se o sistema não tiver uma janela de autenticação, execute com o caminho do seu AppImage:");
                ui.monospace("sudo ./PeliCool.AppImage permissions install");
                ui.monospace("sudo ./PeliCool.AppImage permissions remove");
            });
        });
    }

    pub(super) fn permission_dialog(&mut self, ctx: &egui::Context, p: Palette) {
        let Some(action) = self.permissions.dialog else {
            return;
        };
        let mut confirm = false;
        let mut cancel = false;
        let modal = egui::Modal::new(egui::Id::new("udev-permissions"))
            .frame(p.card().inner_margin(22)).show(ctx, |ui| {
                ui.set_width(440.);
                ui.heading(if action == Action::Install { "Usar o PeliCool sem root" } else { "Remover a regra udev?" });
                ui.add_space(8.);
                ui.label(if action == Action::Install {
                    "Instale a regra udev para permitir acesso USB aos modelos suportados durante sua sessão. A instalação pede a senha de administrador do sistema."
                } else {
                    "Isso remove somente a regra gerenciada pelo PeliCool. O acesso USB sem root pode deixar de funcionar. Reconecte o dispositivo para renovar as permissões."
                });
                ui.label(RichText::new("Você pode fechar esta janela e instalar ou remover a regra depois em Configurações, na seção Acesso USB sem root.").size(12.).color(p.muted));
                if !self.permissions.editable() {
                    ui.label("Foi encontrada uma regra modificada ou não legível. Ela não será sobrescrita ou removida.");
                }
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    cancel = ui.button("Agora não").clicked();
                    confirm = ui.add_enabled(self.permissions.editable() && !self.permissions.busy && self.task.is_none(),
                        egui::Button::new(if action == Action::Install { "Instalar regra udev" } else { "Remover regra udev" })).clicked();
                });
            });
        if cancel || modal.should_close() {
            self.permissions.dismiss();
        }
        if confirm {
            self.permissions.dismiss();
            self.startup.cancel();
            // Serializa a parada antes de renovar permissões. Upload ativo bloqueia a ação.
            let _ = self.worker.tx.send(worker::Request::Stop);
            self.permissions.busy = true;
            self.permissions.message.clear();
            let (tx, ctx) = (self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let result =
                    permissions::authenticated(action).map_err(|error| format!("{error:#}"));
                let _ = tx.send(Event::PermissionsFinished(action, result));
                ctx.request_repaint();
            });
        }
    }

    pub(super) fn permissions_finished(&mut self, action: Action, result: Result<String, String>) {
        self.permissions.busy = false;
        self.permissions.refresh(self.module.info);
        match result {
            Ok(message) => {
                self.permissions.message = message;
                if action == Action::Install
                    && pelicool::installation::autostart_enabled(&self.paths)
                {
                    self.startup.arm(true, Instant::now());
                }
            }
            Err(message) => {
                self.error = true;
                self.status = message.clone();
                self.permissions.message = message;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn documentation_captures_never_prompt_for_authentication() {
        let state = Permissions::new(pelicool::drivers::mystique::MODULE.info, true);
        assert!(state.dialog.is_none());
        assert!(state.access.is_none());
        assert!(!state.busy);
    }
    #[test]
    fn dismissing_setup_keeps_install_and_remove_available_in_settings() {
        let mut state = Permissions {
            rule: RuleState::Missing,
            access: Some(false),
            dialog: Some(Action::Install),
            busy: false,
            message: String::new(),
        };
        state.dismiss();
        assert!(state.dialog.is_none() && state.editable() && !state.busy);
        state.dialog = Some(Action::Install);
        state.dismiss();
        state.rule = RuleState::Installed;
        assert!(state.editable());
        state.dialog = Some(Action::Remove);
        state.dismiss();
        assert_eq!(state.rule, RuleState::Installed);
        state.rule = RuleState::Modified;
        assert!(!state.editable());
    }
}
