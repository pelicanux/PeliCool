//! Preferências gerais e manutenção. A remoção só ocorre depois de fechar o worker.
use super::{App, Event, Task, WindowState, theme::Palette, widgets};
use eframe::egui::{self, RichText};
use pelicool::{installation, storage::paths::AppPaths};
use std::path::PathBuf;

pub(super) enum Maintenance {
    Install(PathBuf),
    Uninstall { purge: bool },
}
pub(super) enum Dialog {
    Install(PathBuf),
    Uninstall,
    Diagnose,
}
pub(super) fn perform(action: Maintenance) -> anyhow::Result<String> {
    let paths = AppPaths::discover()?;
    match action {
        Maintenance::Install(source) => {
            installation::install(&paths, &source)?;
            Ok(format!(
                "PeliCool instalado em {}. Abra-o pelo menu de aplicativos.",
                paths.installation.display()
            ))
        }
        Maintenance::Uninstall { purge } => {
            installation::uninstall(&paths, purge)?;
            Ok(if purge {
                "PeliCool desinstalado. Dados e configurações removidos."
            } else {
                "PeliCool desinstalado. Suas configurações foram mantidas."
            }
            .into())
        }
    }
}
impl App {
    pub(super) fn application_page(&mut self, ui: &mut egui::Ui, p: Palette) {
        self.page_title(
            ui,
            p,
            "Configurações",
            "Preferências do PeliCool e integrações do sistema.",
        );
        p.card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.label(RichText::new("Sobre").size(17.));
            ui.label(RichText::new("PeliCool").size(19.).strong());
            ui.label(format!("Versão {}", env!("CARGO_PKG_VERSION")));
            ui.label(
                RichText::new("Personalização de displays LCD e iluminação no Linux.")
                    .size(12.)
                    .color(p.muted),
            );
            ui.label(
                RichText::new("Projeto independente e de código aberto.")
                    .size(11.)
                    .color(p.muted),
            );
        });
        ui.add_space(10.);
        p.card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.;
            ui.label(RichText::new("Aparência e comportamento").size(17.));
            let mut dark = self.dark;
            if widgets::toggle(ui, &mut dark, "Tema escuro", p).changed() {
                self.set_dark(ui.ctx(), dark);
            }
            if widgets::toggle(
                ui,
                &mut self.preferences.minimize_on_close,
                "Minimizar ao fechar, sem perguntar",
                p,
            )
            .changed()
            {
                self.save_interface();
            }
            ui.label(
                RichText::new(
                    "Sem bandeja, a janela minimiza para o painel/atividades e continua acessível.",
                )
                .size(11.)
                .color(p.muted),
            );
            let installed = installation::is_installed(&self.paths);
            let mut autostart = installation::autostart_enabled(&self.paths);
            ui.add_enabled_ui(installed, |ui| {
                if widgets::toggle(ui, &mut autostart, "Iniciar com o sistema, minimizado", p)
                    .changed()
                    && let Err(error) = installation::set_autostart(&self.paths, autostart)
                {
                    self.error = true;
                    self.status = format!("{error:#}");
                }
            });
            if autostart {
                ui.label(
                    RichText::new("A última imagem ou GIF é restaurada no display ao iniciar.")
                        .size(11.)
                        .color(p.muted),
                );
            }
            if !installed {
                ui.label(
                    RichText::new("Instale o AppImage para ativar a inicialização automática.")
                        .size(11.)
                        .color(p.muted),
                );
            }
        });
        ui.add_space(10.);
        self.permission_card(ui, p);
        ui.add_space(10.);
        p.card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.;
            ui.label(RichText::new("Comunicação com OpenRGB").size(17.));
            let before = (
                self.openrgb_sync,
                self.openrgb_use_sdk,
                self.openrgb_profile.clone(),
            );
            widgets::toggle(
                ui,
                &mut self.openrgb_sync,
                "Reaplicar o perfil ao sincronizar LEDs",
                p,
            );
            ui.add_enabled_ui(self.openrgb_sync, |ui| {
                widgets::toggle(
                    ui,
                    &mut self.openrgb_use_sdk,
                    "Usar SDK Server local (opcional)",
                    p,
                );
                ui.label(
                    RichText::new(if self.openrgb_use_sdk {
                        "Conecta somente em 127.0.0.1:6742. Ative o SDK Server no OpenRGB."
                    } else {
                        "Usa o perfil salvo pela CLI do OpenRGB. Não exige servidor."
                    })
                    .size(11.)
                    .color(p.muted),
                );
                egui::ComboBox::from_id_salt("settings-openrgb-profile")
                    .selected_text(
                        self.openrgb_profile
                            .as_ref()
                            .and_then(|p| p.file_stem())
                            .map(|name| name.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Perfil de inicialização do OpenRGB".into()),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.openrgb_profile,
                            None,
                            "Perfil de inicialização do OpenRGB",
                        );
                        for path in &self.openrgb_profiles {
                            ui.selectable_value(
                                &mut self.openrgb_profile,
                                Some(path.clone()),
                                path.file_stem().unwrap_or_default().to_string_lossy(),
                            );
                        }
                    });
                ui.label(RichText::new(&self.openrgb).size(11.).color(p.muted));
            });
            if before
                != (
                    self.openrgb_sync,
                    self.openrgb_use_sdk,
                    self.openrgb_profile.clone(),
                )
            {
                self.save_interface();
            }
        });
        ui.add_space(10.);
        p.card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.;
            ui.label(RichText::new("Instalação e dados").size(17.));
            ui.label(format!("Configurações: {}", self.paths.config.display()));
            ui.label(format!(
                "AppImage instalado: {}",
                self.paths.installation.display()
            ));
            ui.add_enabled_ui(self.task.is_none(), |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .button(if installation::is_installed(&self.paths) {
                            "Atualizar AppImage instalado"
                        } else {
                            "Instalar AppImage"
                        })
                        .clicked()
                    {
                        let source =
                            std::env::var_os("APPIMAGE").map(PathBuf::from).or_else(|| {
                                rfd::FileDialog::new()
                                    .set_title("Selecionar PeliCool AppImage")
                                    .add_filter("AppImage", &["AppImage", "appimage"])
                                    .pick_file()
                            });
                        if let Some(source) = source {
                            self.settings_dialog = Some(Dialog::Install(source));
                        }
                    }
                    if ui.button("Desinstalar / limpar dados").clicked() {
                        self.settings_dialog = Some(Dialog::Uninstall);
                    }
                });
            });
        });
        ui.add_space(10.);
        p.card().inner_margin(12).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.;
            ui.label(RichText::new("Ajudar com novos dispositivos").size(17.));
            ui.label("O diagnóstico ajuda a identificar displays e planejar novos módulos. Você revisa o relatório e decide se quer compartilhá-lo.");
            let folder = self.last_diagnostic.clone()
                .filter(|path| path.is_dir())
                .or_else(|| {
                    let folder = self.paths.config.join("diagnostics");
                    folder.is_dir().then_some(folder)
                });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(self.task.is_none(), egui::Button::new("Coletar diagnóstico local…"))
                    .clicked()
                {
                    self.settings_dialog = Some(Dialog::Diagnose);
                }
                if ui.add_enabled(folder.is_some(), egui::Button::new("Abrir pasta do diagnóstico"))
                    .on_disabled_hover_text("Faça uma coleta para abrir a pasta com os dados.")
                    .clicked()
                    && let Some(path) = folder
                {
                    open_diagnostic_folder(path, self.tx.clone(), ui.ctx().clone());
                }
            });
            if let Some(path) = &self.last_diagnostic {
                ui.label(
                    RichText::new(format!("Relatório salvo: {}", path.display()))
                        .size(11.)
                        .color(p.muted),
                );
            }
        });
    }
    pub(super) fn settings_dialog(&mut self, ctx: &egui::Context, p: Palette) {
        let Some(dialog) = &self.settings_dialog else {
            return;
        };
        let mut confirm = false;
        let mut cancel = false;
        let modal=egui::Modal::new(egui::Id::new("settings-confirmation")).frame(p.card().inner_margin(20)).show(ctx,|ui|{
            ui.set_width(450.);
            match dialog {
                Dialog::Install(source)=>{
                    ui.heading("Instalar PeliCool");
                    ui.label(format!("Copiar {} para {} e criar um atalho no menu?",source.display(),self.paths.installation.display()));
                    ui.label("A janela fechará para concluir a instalação. Não exige sudo.");
                }
                Dialog::Uninstall=>{
                    ui.heading("Desinstalar PeliCool");
                    ui.label("Remove o AppImage instalado, o atalho e a inicialização automática. A janela será fechada.");
                    ui.checkbox(&mut self.purge,"Remover também configurações, mídias locais e diagnósticos");
                    ui.label("Os arquivos originais fora das pastas do PeliCool e os perfis do OpenRGB são preservados. A regra udev de sistema exige remoção administrativa separada.");
                }
                Dialog::Diagnose=>{
                    ui.heading("Autorizar diagnóstico local");
                    ui.label("Coleta distribuição/kernel, arquitetura, sessão gráfica, metadados USB de aparelhos candidatos e nomes de fontes de sensores.");
                    ui.label("Não coleta usuário, hostname, IP, MAC, números de série, mídias ou arquivos pessoais. Não envia comandos USB e não usa rede.");
                    ui.label("O relatório fica em Configurações/PeliCool/diagnostics. Revise os arquivos antes de compartilhar manualmente. Não exige root.");
                }
            }
            ui.add_space(12.);
            ui.horizontal(|ui|{cancel=ui.button("Cancelar").clicked();confirm=ui.button(if matches!(dialog,Dialog::Diagnose){"Autorizar coleta"}else{"Confirmar"}).clicked();});
        });
        if cancel || modal.should_close() {
            self.settings_dialog = None;
            return;
        }
        if confirm {
            match self.settings_dialog.take().expect("dialog") {
                Dialog::Diagnose => {
                    let output = self.paths.config.join("diagnostics").join(format!(
                        "report-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    ));
                    let tx = self.tx.clone();
                    self.task = Some(Task::Diagnose);
                    self.status = "Coletando diagnóstico local…".into();
                    std::thread::spawn(move || {
                        let _ = tx.send(match pelicool::diagnostics::collect(&output) {
                            Ok(path) => Event::DiagnosticReady(path),
                            Err(error) => Event::Error(format!("{error:#}")),
                        });
                    });
                }
                dialog => {
                    let action = match dialog {
                        Dialog::Install(source) => Maintenance::Install(source),
                        Dialog::Uninstall => Maintenance::Uninstall { purge: self.purge },
                        _ => unreachable!(),
                    };
                    *self.maintenance.lock().expect("maintenance lock") = Some(action);
                    self.window_state = WindowState::Exiting;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }
}

/// Abre somente a pasta local, sem shell e sem propagar bibliotecas do AppImage
/// para o gerenciador de arquivos instalado no sistema.
fn open_diagnostic_folder(
    path: PathBuf,
    events: std::sync::mpsc::Sender<Event>,
    ctx: egui::Context,
) {
    std::thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            anyhow::ensure!(
                path.is_dir(),
                "A pasta do diagnóstico não está mais disponível."
            );
            let mut command = std::process::Command::new("xdg-open");
            command.arg(path);
            if let Some(original) = std::env::var_os("PELICOOL_HOST_LIBRARY_PATH") {
                if original.is_empty() {
                    command.env_remove("LD_LIBRARY_PATH");
                } else {
                    command.env("LD_LIBRARY_PATH", original);
                }
            }
            let result = command.output().map_err(|error| {
                anyhow::anyhow!("Não foi possível abrir o gerenciador de arquivos: {error}")
            })?;
            anyhow::ensure!(
                result.status.success(),
                "Não foi possível abrir a pasta do diagnóstico: {}",
                String::from_utf8_lossy(&result.stderr).trim()
            );
            Ok(())
        })();
        if let Err(error) = result {
            let _ = events.send(Event::OpenFolderError(format!("{error:#}")));
            ctx.request_repaint();
        }
    });
}
