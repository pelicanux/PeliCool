//! Restaura somente um slot já confirmado; nunca prepara, formata ou envia mídia.
use super::{App, Task};
use anyhow::{Result, ensure};
use eframe::egui;
use pelicool::{
    collection::{Cache, Catalog, Slot},
    device::DisplayMode,
    drivers::DriverInfo,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    time::{Duration, Instant},
};

pub(super) fn enabled(autostart: bool, capture: bool, importing: bool, mode: DisplayMode) -> bool {
    autostart && !capture && !importing && mode == DisplayMode::Media
}

#[derive(Default, Serialize, Deserialize)]
struct Remembered {
    media_id: Option<String>,
}

pub(super) struct Startup {
    remembered: Remembered,
    pending: Option<Retry>,
}

struct Retry {
    expires: Instant,
    due: Instant,
    in_flight: bool,
}

impl Startup {
    pub fn load(cache: &Cache) -> Result<Self> {
        let path = cache.root.join("settings/startup.json");
        let remembered = if path.exists() {
            ensure!(
                path.metadata()?.len() < 4096,
                "Preferência de mídia inicial inválida"
            );
            serde_json::from_slice(&fs::read(path)?)?
        } else {
            Remembered::default()
        };
        Ok(Self {
            remembered,
            pending: None,
        })
    }

    pub fn initial_index(&self, catalog: &Catalog) -> Option<usize> {
        self.remembered
            .media_id
            .as_ref()
            .and_then(|id| {
                catalog
                    .entries
                    .iter()
                    .position(|entry| &entry.id == id && catalog.slot(id).is_some())
            })
            .or_else(|| {
                catalog
                    .entries
                    .iter()
                    .position(|entry| catalog.slot(&entry.id).is_some())
            })
    }

    pub fn arm(&mut self, enabled: bool, now: Instant) {
        self.pending = enabled.then_some(Retry {
            expires: now + Duration::from_secs(30),
            due: now,
            in_flight: false,
        });
    }

    pub fn cancel(&mut self) {
        self.pending = None;
    }

    pub fn completed(&mut self, success: bool, now: Instant) {
        if let Some(pending) = self.pending.as_mut().filter(|pending| pending.in_flight) {
            if success || now >= pending.expires {
                self.cancel();
            } else {
                pending.in_flight = false;
                pending.due = now + Duration::from_secs(2);
            }
        }
    }

    pub fn remember(&mut self, catalog: &Catalog, slot: Slot, cache: &Cache) -> Result<()> {
        let Some(id) = catalog
            .entries
            .iter()
            .find(|entry| catalog.slot(&entry.id) == Some(slot))
            .map(|entry| &entry.id)
        else {
            return Ok(());
        };
        if self.remembered.media_id.as_ref() != Some(id) {
            let remembered = Remembered {
                media_id: Some(id.clone()),
            };
            pelicool::storage::write_json(&cache.root.join("settings/startup.json"), &remembered)?;
            self.remembered = remembered;
        }
        Ok(())
    }

    fn slot(&self, catalog: &Catalog, info: &DriverInfo) -> Option<Slot> {
        if catalog.rotation_pending(info, catalog.settings.rotation) {
            return None;
        }
        self.initial_index(catalog)
            .and_then(|index| catalog.slot(&catalog.entries[index].id))
    }
}

impl App {
    pub(super) fn restore_startup(&mut self, ctx: &egui::Context) {
        if self.permissions.dialog.is_some() || self.permissions.busy {
            return;
        }
        let Some(mut pending) = self.startup.pending.take() else {
            return;
        };
        let now = Instant::now();
        if self.mode != DisplayMode::Media || self.editor.is_some() {
            return;
        }
        if pending.in_flight || self.task.is_some() || now < pending.due {
            self.startup.pending = Some(pending);
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        if now >= pending.expires {
            self.error = true;
            self.status = "Não foi possível restaurar a mídia ao iniciar. Confira a conexão USB e as permissões.".into();
            return;
        }
        let Some(slot) = self
            .startup
            .slot(&self.catalog, self.module.info)
            .filter(|_| {
                !self
                    .catalog
                    .rotation_pending(self.module.info, self.settings.rotation)
            })
        else {
            // Não redirecionar ao upload automático de operations::operation.
            self.status = "A mídia inicial ainda não está confirmada no display. Aplique-a uma vez pela interface.".into();
            return;
        };
        self.detected = super::detect_device(self.module);
        if !self.detected {
            pending.due = now + Duration::from_secs(1);
            self.startup.pending = Some(pending);
            ctx.request_repaint_after(Duration::from_secs(1));
            return;
        }
        self.auto_select = Some(slot);
        self.operation(Task::Select);
        pending.in_flight = true;
        self.startup.pending = Some(pending);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelicool::{
        collection::Entry,
        drivers::mystique::MODULE,
        media::{Fit, Orientation},
    };

    #[test]
    fn only_authorized_media_launches_restore_and_captures_never_do() {
        assert!(enabled(true, false, false, DisplayMode::Media));
        assert!(!enabled(false, false, false, DisplayMode::Media));
        assert!(!enabled(true, true, false, DisplayMode::Media));
        assert!(!enabled(true, false, true, DisplayMode::Media));
        assert!(!enabled(true, false, false, DisplayMode::Monitoring));
    }

    fn catalog() -> Catalog {
        let mut catalog = Catalog {
            entries: [false, true]
                .into_iter()
                .enumerate()
                .map(|(index, animated)| Entry {
                    id: format!("item-{index}"),
                    name: "Test".into(),
                    source: "unused".into(),
                    animated,
                    horizontal: true,
                    orientation: Orientation::Auto,
                    fit: Fit::Cover,
                    duration_ms: 100,
                    delays: vec![100],
                    framing: Default::default(),
                    adaptation: None,
                })
                .collect(),
            ..Default::default()
        };
        catalog.commit();
        catalog.media_rotation = Some(catalog.settings.rotation);
        catalog.media_transform_revision =
            MODULE.info.capabilities.media.unwrap().transform_revision;
        catalog
    }

    #[test]
    fn remembered_gif_survives_restart_and_unconfirmed_media_never_restores() {
        let root = std::env::temp_dir().join(format!(
            "pelicool-startup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let cache = Cache::at(root.clone());
        let mut startup = Startup::load(&cache).unwrap();
        let mut catalog = catalog();
        assert_eq!(
            startup.slot(&catalog, MODULE.info),
            Some(Slot {
                animated: false,
                index: 0
            })
        );
        startup
            .remember(
                &catalog,
                Slot {
                    animated: true,
                    index: 0,
                },
                &cache,
            )
            .unwrap();
        startup = Startup::load(&cache).unwrap();
        assert_eq!(startup.initial_index(&catalog), Some(1));
        assert_eq!(
            startup.slot(&catalog, MODULE.info),
            Some(Slot {
                animated: true,
                index: 0
            })
        );
        catalog.media_transform_revision = 0;
        assert!(startup.slot(&catalog, MODULE.info).is_none());
        catalog.valid = false;
        assert!(startup.slot(&catalog, MODULE.info).is_none());
        catalog.entries.remove(1);
        catalog.commit();
        assert_eq!(startup.initial_index(&catalog), Some(0));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_startup_retries_with_delay_and_manual_actions_cancel_it() {
        let now = Instant::now();
        let mut startup = Startup {
            remembered: Remembered::default(),
            pending: None,
        };
        startup.arm(false, now);
        assert!(startup.pending.is_none());
        startup.arm(true, now);
        startup.pending.as_mut().unwrap().in_flight = true;
        startup.completed(false, now);
        let pending = startup.pending.as_ref().unwrap();
        assert!(!pending.in_flight);
        assert_eq!(pending.due, now + Duration::from_secs(2));
        startup.cancel();
        assert!(startup.pending.is_none());
        startup.arm(true, now);
        startup.pending.as_mut().unwrap().in_flight = true;
        startup.completed(true, now);
        assert!(startup.pending.is_none());
        startup.arm(true, now);
        startup.pending.as_mut().unwrap().in_flight = true;
        startup.completed(false, now + Duration::from_secs(30));
        assert!(startup.pending.is_none());
    }
}
