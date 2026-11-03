//! Encaminhamento das ações explícitas para o worker serial.
use super::{App, Task, worker};

impl App {
    pub(super) fn operation(&mut self, mut task: Task) {
        if self.task.is_some() || self.permissions.busy {
            return;
        }
        self.startup.cancel();
        let preserve_shown = task == Task::Apply;
        if matches!(task, Task::Apply | Task::Select)
            && self
                .catalog
                .rotation_pending(self.module.info, self.settings.rotation)
        {
            task = Task::Upload;
            self.auto_select = None;
        }
        self.task = Some(task);
        self.error = false;
        self.progress = 0.0;
        self.status = match task {
            Task::Upload => "Preparando orientação e salvando mídias no display…",
            Task::Apply => "Aplicando tela e iluminação…",
            Task::Monitor => "Ativando o preset do display…",
            Task::Stop => "Interrompendo a atualização…",
            Task::ResetGraph => "Reiniciando o gráfico…",
            Task::Select => "Selecionando a mídia salva no display…",
            _ => "Consultando o dispositivo…",
        }
        .into();
        if matches!(
            task,
            Task::Apply | Task::Monitor | Task::Select | Task::Upload
        ) {
            self.catalog.settings = self.settings;
            self.catalog.rgb_profile = self.openrgb_profile.clone();
            self.catalog.openrgb_live = self.openrgb_use_sdk;
            if let Err(error) = self.cache.save(&self.catalog) {
                self.error = true;
                self.task = None;
                self.status = format!("Erro ao salvar ajustes: {error:#}");
                return;
            }
        }
        let bridge = pelicool::openrgb::Sync {
            enabled: self.openrgb_sync,
            live_sdk: self.openrgb_use_sdk,
            profile: self.openrgb_profile.clone(),
        };
        let request = match task {
            Task::Upload => worker::Request::Upload {
                catalog: self.catalog.clone(),
                settings: self.settings,
                selected: preserve_shown
                    .then(|| {
                        self.shown.and_then(|slot| {
                            self.catalog
                                .saved
                                .iter()
                                .find(|saved| saved.slot == slot)
                                .map(|saved| saved.id.clone())
                        })
                    })
                    .flatten()
                    .or_else(|| {
                        self.selected
                            .and_then(|index| self.library.get(index))
                            .map(|item| item.entry.id.clone())
                    }),
                bridge,
            },
            Task::Apply => worker::Request::Apply(
                self.settings,
                self.shown.or_else(|| {
                    self.selected
                        .and_then(|index| self.library.get(index))
                        .and_then(|item| self.catalog.slot(&item.entry.id))
                }),
                bridge,
            ),
            Task::Select => {
                let Some(slot) = self.auto_select.take() else {
                    self.task = None;
                    return;
                };
                worker::Request::Select(slot, self.settings, bridge)
            }
            Task::Monitor => {
                self.preset.mode = self.mode;
                worker::Request::Monitor(self.settings, self.preset, bridge)
            }
            Task::Stop => worker::Request::Stop,
            Task::ResetGraph => worker::Request::ResetGraph,
            _ => worker::Request::Probe,
        };
        if self.worker.tx.send(request).is_err() {
            self.task = None;
            self.error = true;
            self.status = "O serviço USB foi encerrado. Reabra o aplicativo.".into();
        }
    }
}
