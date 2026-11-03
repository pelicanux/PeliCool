//! Um único worker possui o USB e serializa mídia, presets e sensores.
use super::Event;
use pelicool::{
    collection::{Cache, Catalog, Slot},
    device::{self, Device, DisplayMode, Preset, RgbMode, Settings},
    drivers::DriverModule,
    sensors::{Reader, Selection},
};
use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub enum Request {
    Probe,
    Apply(Settings, Option<Slot>, pelicool::openrgb::Sync),
    Upload {
        catalog: Catalog,
        settings: Settings,
        selected: Option<String>,
        bridge: pelicool::openrgb::Sync,
    },
    Select(Slot, Settings, pelicool::openrgb::Sync),
    Monitor(Settings, Preset, pelicool::openrgb::Sync),
    Stop,
    ResetGraph,
    Sources(Selection),
    Shutdown,
}
pub struct Worker {
    pub tx: Sender<Request>,
    join: Option<JoinHandle<()>>,
}
struct Playback {
    settings: Settings,
    slots: Vec<Slot>,
    current: usize,
    due: Instant,
    durations: Vec<u16>,
}
impl Playback {
    fn is_due(&self, now: Instant) -> bool {
        self.settings.slideshow && self.slots.len() > 1 && now >= self.due
    }

    fn unchanged(&self, settings: Settings, selected: Option<Slot>) -> bool {
        self.settings == settings
            && selected.is_none_or(|slot| self.slots.get(self.current) == Some(&slot))
    }

    fn new(settings: Settings, catalog: &Catalog, selected: Option<Slot>) -> Self {
        // A miniatura escolhida define a categoria. O slideshow permanece nela
        // até uma seleção explícita, inclusive quando só existe uma mídia do tipo.
        let selected = selected
            .filter(|slot| {
                catalog
                    .entries
                    .iter()
                    .any(|e| catalog.slot(&e.id) == Some(*slot))
            })
            .or_else(|| catalog.entries.iter().find_map(|e| catalog.slot(&e.id)));
        let (slots, durations): (Vec<_>, Vec<_>) = catalog
            .entries
            .iter()
            .filter_map(|e| catalog.slot(&e.id).map(|slot| (slot, e.duration_ms)))
            .filter(|(slot, _)| selected.is_some_and(|selected| slot.animated == selected.animated))
            .unzip();
        let current = selected
            .and_then(|slot| slots.iter().position(|s| *s == slot))
            .unwrap_or(0);
        let mut result = Self {
            settings,
            slots,
            current,
            due: Instant::now(),
            durations,
        };
        result.schedule();
        result
    }
    fn schedule(&mut self) {
        let duration = u64::from(self.durations.get(self.current).copied().unwrap_or(0));
        let interval = u64::from(self.settings.slide_seconds) * 1000;
        let delay = if duration > 0 {
            interval.div_ceil(duration) * duration
        } else {
            interval
        };
        self.due = Instant::now() + Duration::from_millis(delay);
    }
}
impl Worker {
    pub fn new(events: Sender<Event>, module: &'static DriverModule, cache: Cache) -> Self {
        let (tx, rx) = mpsc::channel();
        let join = thread::spawn(move || {
            let mut reader = Reader::default();
            let mut controller: Option<Device> = None;
            let mut active: Option<Preset> = None;
            let mut thermal_active = false;
            let mut playback: Option<Playback> = None;
            let mut next = Instant::now();
            let mut sync = Instant::now();
            loop {
                if Instant::now() >= next {
                    let snapshot = reader.sample();
                    let _ = events.send(Event::Sensors(Box::new(snapshot.clone())));
                    if let Some(device) = controller.as_mut() {
                        let result = (|| -> anyhow::Result<()> {
                            let thermal = thermal_active
                                || playback
                                    .as_ref()
                                    .is_some_and(|p| p.settings.rgb == RgbMode::Internal);
                            if let Some(preset) = active {
                                preset.check_sources(&snapshot)?;
                            }
                            if thermal {
                                anyhow::ensure!(
                                    snapshot.cpu_temp.is_some(),
                                    "Temperatura da CPU indisponível para o alerta térmico."
                                );
                            }
                            if active.is_some() || thermal {
                                device.update_sensors(&snapshot)?;
                            }
                            if active.is_some() && sync.elapsed() >= Duration::from_secs(30) {
                                device.sync_clock()?;
                                sync = Instant::now();
                            }
                            Ok(())
                        })();
                        if let Err(error) = result {
                            controller = None;
                            active = None;
                            playback = None;
                            let _ = events.send(Event::Monitoring(None));
                            let _ = events.send(Event::StreamError(device::describe_error(&error)));
                        }
                    }
                    next = Instant::now() + Duration::from_secs(1);
                }
                if let (Some(device), Some(p)) = (controller.as_mut(), playback.as_mut())
                    && p.is_due(Instant::now())
                {
                    p.current = (p.current + 1) % p.slots.len();
                    match device.select_media(p.slots[p.current]) {
                        Ok(()) => {
                            p.schedule();
                            let _ = events.send(Event::Shown(p.slots[p.current]));
                        }
                        Err(error) => {
                            controller = None;
                            playback = None;
                            active = None;
                            let _ = events.send(Event::Monitoring(None));
                            let _ = events.send(Event::StreamError(device::describe_error(&error)));
                        }
                    }
                }
                let wake = playback
                    .as_ref()
                    .filter(|p| p.settings.slideshow && p.slots.len() > 1)
                    .map(|p| p.due.min(next))
                    .unwrap_or(next);
                let request = match rx.recv_timeout(wake.saturating_duration_since(Instant::now()))
                {
                    Ok(r) => r,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                match request {
                    Request::Shutdown => break,
                    Request::Sources(selection) => reader.select(selection),
                    Request::Stop => {
                        controller = None;
                        active = None;
                        playback = None;
                        let _ = events.send(Event::Monitoring(None));
                        report(
                            &events,
                            Ok(
                                "Atualização e slideshow interrompidos. As mídias foram preservadas."
                                    .into(),
                            ),
                        );
                    }
                    Request::Probe => {
                        let result = if let Some(d) = controller.as_mut() {
                            d.probe()
                        } else {
                            module.open().and_then(|mut d| d.probe())
                        };
                        report(&events, result);
                    }
                    Request::ResetGraph => {
                        let result = (|| {
                            anyhow::ensure!(
                                active.is_some_and(|p| p.mode == DisplayMode::Recorder),
                                "Ative Recording primeiro."
                            );
                            let d = controller
                                .as_mut()
                                .ok_or_else(|| anyhow::anyhow!("USB desconectado."))?;
                            d.reset_graph()?;
                            Ok("Histórico do gráfico reiniciado.".into())
                        })();
                        report(&events, result);
                    }
                    Request::Monitor(settings, preset, bridge) => {
                        let result = (|| -> anyhow::Result<String> {
                            validate_thermal(&mut reader, settings)?;
                            let snapshot = reader.sample();
                            preset.check_sources(&snapshot)?;
                            let reset = active.map(|p| p.mode) != Some(preset.mode);
                            if controller.is_none() {
                                controller = Some(module.open()?);
                            }
                            let d = controller.as_mut().expect("opened");
                            d.configure_preset(settings, preset, reset)?;
                            d.update_sensors(&snapshot)?;
                            active = Some(preset);
                            thermal_active = settings.rgb == RgbMode::Internal;
                            playback = None;
                            sync = Instant::now();
                            next = Instant::now() + Duration::from_secs(1);
                            let _ = events.send(Event::Sensors(Box::new(snapshot)));
                            let _ = events.send(Event::Monitoring(Some(preset.mode)));
                            resync_rgb(&events, settings, bridge);
                            Ok("Preset ativado. Sensores atualizados a cada segundo, mesmo com o PeliCool minimizado.".into())
                        })();
                        if result.is_err() {
                            controller = None;
                            active = None;
                            playback = None;
                            let _ = events.send(Event::Monitoring(None));
                        }
                        report(&events, result);
                    }
                    Request::Apply(settings, selected, bridge) => {
                        let result = (|| -> anyhow::Result<String> {
                            if controller.is_some()
                                && active.is_none()
                                && playback
                                    .as_ref()
                                    .is_some_and(|p| p.unchanged(settings, selected))
                            {
                                resync_rgb(&events, settings, bridge);
                                return Ok("Estes ajustes já estão aplicados no display.".into());
                            }
                            validate_thermal(&mut reader, settings)?;
                            let catalog = cache.load()?;
                            anyhow::ensure!(
                                !catalog.rotation_pending(module.info, settings.rotation),
                                "A orientação da mídia exige atualizar a coleção. Aplique pela interface antes de iniciar reprodução sem upload."
                            );
                            anyhow::ensure!(
                                !settings.slideshow
                                    || (catalog.valid
                                        && catalog
                                            .entries
                                            .iter()
                                            .any(|e| catalog.slot(&e.id).is_some())),
                                "Salve uma imagem ou GIF antes de ativar o slideshow."
                            );
                            if controller.is_none() {
                                controller = Some(module.open()?);
                            }
                            controller.as_mut().expect("opened").apply(settings)?;
                            thermal_active = settings.rgb == RgbMode::Internal;
                            let selected = selected.or_else(|| {
                                playback
                                    .as_ref()
                                    .and_then(|p| p.slots.get(p.current).copied())
                            });
                            let p = Playback::new(settings, &catalog, selected);
                            if let Some(slot) = p.slots.get(p.current).copied() {
                                controller.as_mut().expect("opened").select_media(slot)?;
                                let _ = events.send(Event::Shown(slot));
                            }
                            playback = Some(p);
                            active = None;
                            let _ = events.send(Event::Monitoring(None));
                            resync_rgb(&events, settings, bridge);
                            Ok(
                                "Tela, iluminação e slideshow aplicados. Nenhum arquivo reenviado."
                                    .into(),
                            )
                        })();
                        if result.is_err() {
                            controller = None;
                            playback = None;
                            active = None;
                            let _ = events.send(Event::Monitoring(None));
                        }
                        report(&events, result);
                    }
                    Request::Select(slot, settings, bridge) => {
                        let result = (|| -> anyhow::Result<String> {
                            let catalog = cache.load()?;
                            anyhow::ensure!(
                                !catalog.rotation_pending(module.info, settings.rotation),
                                "A orientação da mídia ainda não foi salva no display."
                            );
                            anyhow::ensure!(
                                catalog.valid && catalog.saved.iter().any(|s| s.slot == slot),
                                "Mídia ainda não salva. Clique na miniatura para tentar novamente."
                            );
                            validate_thermal(&mut reader, settings)?;
                            if controller.is_none() {
                                controller = Some(module.open()?);
                            }
                            let d = controller.as_mut().expect("opened");
                            let apply = active.is_some()
                                || playback.as_ref().is_none_or(|p| p.settings != settings);
                            if apply {
                                d.apply(settings)?;
                                resync_rgb(&events, settings, bridge);
                            }
                            d.select_media(slot)?;
                            thermal_active = settings.rgb == RgbMode::Internal;
                            active = None;
                            playback = Some(Playback::new(settings, &catalog, Some(slot)));
                            let _ = events.send(Event::Monitoring(None));
                            let _ = events.send(Event::Shown(slot));
                            Ok("Mídia selecionada na memória do display, sem upload.".into())
                        })();
                        if result.is_err() {
                            controller = None;
                            playback = None;
                            active = None;
                            let _ = events.send(Event::Monitoring(None));
                        }
                        report(&events, result);
                    }
                    Request::Upload {
                        mut catalog,
                        settings,
                        selected,
                        bridge,
                    } => {
                        let result = (|| -> anyhow::Result<String> {
                            validate_thermal(&mut reader, settings)?;
                            if controller.is_none() {
                                controller = Some(module.open()?);
                            }
                            active = None;
                            playback = None;
                            let _ = events.send(Event::Monitoring(None));
                            let mut last_progress = Instant::now() - Duration::from_secs(1);
                            let saved = pelicool::collection::sync::save(
                                &cache,
                                controller.as_mut().expect("opened"),
                                &mut catalog,
                                settings,
                                selected.as_deref(),
                                |value| {
                                    if value >= 1.
                                        || last_progress.elapsed() >= Duration::from_millis(50)
                                    {
                                        let _ = events.send(Event::Progress(value));
                                        last_progress = Instant::now();
                                    }
                                },
                                |catalog| {
                                    let _ = events.send(Event::Catalog(catalog.clone()));
                                },
                            )?;
                            thermal_active = settings.rgb == RgbMode::Internal;
                            if let Some(slot) = saved.shown {
                                playback = Some(Playback::new(settings, &catalog, Some(slot)));
                                let _ = events.send(Event::Shown(slot));
                            }
                            resync_rgb(&events, settings, bridge);
                            Ok(saved.cleanup_warning.unwrap_or_else(||"Mídias salvas no display. Clique nas miniaturas para trocar instantaneamente.".into()))
                        })();
                        if result.is_err() {
                            controller = None;
                            playback = None;
                            active = None;
                            let _ = events.send(Event::Monitoring(None));
                        }
                        report(&events, result);
                    }
                }
            }
        });
        Self {
            tx,
            join: Some(join),
        }
    }
}
fn validate_thermal(reader: &mut Reader, settings: Settings) -> anyhow::Result<()> {
    if settings.rgb == RgbMode::Internal {
        anyhow::ensure!(
            reader.sample().cpu_temp.is_some(),
            "Selecione uma fonte de temperatura da CPU disponível."
        );
    }
    Ok(())
}
fn resync_rgb(events: &Sender<Event>, settings: Settings, bridge: pelicool::openrgb::Sync) {
    if settings.rgb == RgbMode::Motherboard && bridge.enabled {
        std::thread::sleep(Duration::from_millis(200));
        let message = match if bridge.live_sdk {
            pelicool::openrgb::refresh_motherboard()
        } else {
            pelicool::openrgb::apply_profile(bridge.profile.as_deref())
        } {
            Ok(name) => format!("OpenRGB reaplicado: {name}."),
            Err(error) => format!("Placa-mãe ativada. OpenRGB: {error:#}"),
        };
        let _ = events.send(Event::OpenRgb(message));
    }
}
fn report(events: &Sender<Event>, result: anyhow::Result<String>) {
    let _ = events.send(match result {
        Ok(message) => Event::Done(message),
        Err(error) => Event::Error(device::describe_error(&error)),
    });
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.tx.send(Request::Shutdown);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub(super) fn play_collection(
    seconds: u64,
    interval: u8,
    module: &'static DriverModule,
) -> anyhow::Result<()> {
    let cache = Cache::for_driver(module.info.id)?;
    let _lock = cache.lock()?;
    let settings = cache.load()?.settings;
    let (tx, rx) = mpsc::channel();
    let worker = Worker::new(tx, module, cache);
    worker.tx.send(Request::Apply(
        Settings {
            slideshow: true,
            slide_seconds: interval,
            ..settings
        },
        None,
        pelicool::openrgb::Sync::default(),
    ))?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(seconds) {
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Event::Shown(slot)) => println!(
                "{} · slot {}",
                if slot.animated { "GIF" } else { "Imagem" },
                slot.index
            ),
            Ok(Event::Done(message)) => println!("{message}"),
            Ok(Event::Error(message) | Event::StreamError(message)) => anyhow::bail!(message),
            Err(mpsc::RecvTimeoutError::Disconnected) => anyhow::bail!("Worker encerrado."),
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mixed_catalog() -> Catalog {
        use pelicool::{
            collection::Entry,
            media::{Fit, Orientation},
        };
        let mut catalog = Catalog {
            entries: [(true, 800), (false, 0), (true, 1200), (false, 0)]
                .into_iter()
                .enumerate()
                .map(|(index, (animated, duration_ms))| Entry {
                    id: format!("media-{index}"),
                    name: format!("Mídia {index}"),
                    source: format!("original-{index}").into(),
                    animated,
                    horizontal: true,
                    orientation: Orientation::Auto,
                    fit: Fit::Cover,
                    duration_ms,
                    delays: vec![duration_ms],
                    framing: Default::default(),
                    adaptation: None,
                })
                .collect(),
            ..Default::default()
        };
        catalog.commit();
        catalog
    }

    #[test]
    fn slideshow_keeps_only_the_selected_type_with_its_matching_durations() {
        let catalog = mixed_catalog();
        let settings = Settings {
            slideshow: true,
            ..Default::default()
        };
        for (animated, durations) in [(false, vec![0, 0]), (true, vec![800, 1200])] {
            let selected = Slot { animated, index: 1 };
            let p = Playback::new(settings, &catalog, Some(selected));
            assert_eq!(p.slots, vec![Slot { animated, index: 0 }, selected]);
            assert_eq!(p.durations, durations);
            assert_eq!(p.slots[p.current], selected);
            // Uma nova miniatura de outra categoria reconstrói a sequência;
            // nenhuma lista mistura os tipos, mesmo com entradas intercaladas.
        }
    }

    #[test]
    fn one_image_does_not_fall_back_to_gifs_even_when_the_interval_is_overdue() {
        let mut catalog = mixed_catalog();
        catalog.entries.pop();
        catalog.commit();
        let settings = Settings {
            slideshow: true,
            ..Default::default()
        };
        let slot = Slot {
            animated: false,
            index: 0,
        };
        let mut p = Playback::new(settings, &catalog, Some(slot));
        p.due = Instant::now() - Duration::from_secs(10);
        assert_eq!(p.slots, vec![slot]);
        assert!(!p.is_due(Instant::now()));
        let mut gifs = Playback::new(
            settings,
            &catalog,
            Some(Slot {
                animated: true,
                index: 0,
            }),
        );
        gifs.due = p.due;
        assert_eq!(gifs.slots.len(), 2);
        assert!(gifs.slots.iter().all(|slot| slot.animated));
        assert!(gifs.is_due(Instant::now()));
    }

    #[test]
    fn slideshow_defaults_to_the_first_confirmed_type_and_skips_unconfirmed_media() {
        let mut catalog = mixed_catalog();
        for selected in [
            None,
            Some(Slot {
                animated: false,
                index: 4,
            }),
        ] {
            let p = Playback::new(Settings::default(), &catalog, selected);
            assert_eq!(p.slots.len(), 2);
            assert!(p.slots.iter().all(|slot| slot.animated));
            assert_eq!(p.current, 0);
        }
        catalog.valid = false;
        let p = Playback::new(Settings::default(), &catalog, None);
        assert!(p.slots.is_empty());
        assert!(p.durations.is_empty());
    }

    #[test]
    fn disabling_slideshow_blocks_an_already_overdue_transition_and_changes_require_apply() {
        let slot = Slot {
            animated: false,
            index: 0,
        };
        let mut p = Playback {
            settings: Settings {
                slideshow: true,
                ..Default::default()
            },
            slots: vec![
                slot,
                Slot {
                    animated: false,
                    index: 1,
                },
            ],
            current: 0,
            due: Instant::now() - Duration::from_secs(10),
            durations: vec![0, 0],
        };
        assert!(p.is_due(Instant::now()));
        let disabled = Settings {
            slideshow: false,
            ..p.settings
        };
        assert!(!p.unchanged(disabled, Some(slot)));
        p.settings = disabled;
        assert!(!p.is_due(Instant::now()));
        assert!(p.unchanged(disabled, Some(slot)));
        assert!(!p.unchanged(
            Settings {
                rotation: 90,
                ..disabled
            },
            Some(slot)
        ));
        assert!(!p.unchanged(disabled, Some(p.slots[1])));
    }

    #[test]
    fn slideshow_waits_a_whole_number_of_gif_loops() {
        let mut p = Playback {
            settings: Settings {
                slide_seconds: 5,
                ..Default::default()
            },
            slots: vec![Slot {
                animated: true,
                index: 0,
            }],
            current: 0,
            due: Instant::now(),
            durations: vec![800],
        };
        let before = Instant::now();
        p.schedule();
        let wait = p.due.duration_since(before);
        assert!(wait >= Duration::from_millis(5600) && wait < Duration::from_millis(5650));
        p.durations[0] = 12000;
        let before = Instant::now();
        p.schedule();
        assert!(p.due.duration_since(before) >= Duration::from_secs(12));
    }
}
