//! Criação do estado e processamento de eventos da interface.
use super::{
    App, Event, Options, Task, WindowState, application, detect_device, editor, permissions,
    startup, theme::Palette, tray, worker,
};
use anyhow::Result;
use eframe::egui;
use pelicool::{
    collection::Cache,
    device::{DisplayMode, Preset, RgbMode},
    drivers::DriverModule,
    media::{Fit, Orientation},
    sensors::{self, Selection, Snapshot},
};
use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

impl App {
    pub(super) fn new(
        cc: &eframe::CreationContext<'_>,
        options: Options,
        module: &'static DriverModule,
        cache: Cache,
        collection_lock: pelicool::collection::CacheLock,
        maintenance: Arc<std::sync::Mutex<Option<application::Maintenance>>>,
    ) -> Result<Self> {
        let Options {
            driver: _,
            file,
            screenshot,
            dark,
            page,
            mode,
            no_tray,
            preview_close,
            preview_udev,
            start_minimized,
            preview_progress,
        } = options;
        let paths = pelicool::storage::paths::AppPaths::discover()?;
        let restore_on_start = startup::enabled(
            pelicool::installation::autostart_enabled(&paths),
            screenshot.is_some(),
            file.is_some(),
            mode,
        );
        let startup = startup::Startup::load(&cache)?;
        let mut permissions = permissions::Permissions::new(module.info, screenshot.is_some());
        if preview_udev {
            permissions.dialog = Some(pelicool::permissions::Action::Install);
        }
        let start_minimized = start_minimized && permissions.dialog.is_none();
        let preferences = pelicool::storage::preferences::Interface::load(&paths.config)?;
        let dark = dark.unwrap_or(preferences.dark);
        egui_extras::install_image_loaders(&cc.egui_ctx);
        Palette::new(dark).install(&cc.egui_ctx);
        use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
        let can_hide = cc.window_handle().is_ok_and(|handle| {
            matches!(
                handle.as_raw(),
                RawWindowHandle::Xlib(_) | RawWindowHandle::Xcb(_)
            )
        });
        let (tx, rx) = mpsc::channel();
        let worker = worker::Worker::new(tx.clone(), module, cache.clone());
        let loaded = cache.load();
        let load_error = loaded
            .as_ref()
            .err()
            .map(|error| format!("Não foi possível ler a coleção: {error:#}"));
        let catalog = loaded.unwrap_or_default();
        let mut settings = catalog.settings;
        if mode != DisplayMode::Media
            && settings.rgb == RgbMode::Media
            && module
                .info
                .capabilities
                .rgb_modes
                .contains(&RgbMode::Internal)
        {
            settings.rgb = RgbMode::Internal;
        }
        let rgb_profile = catalog
            .rgb_profile
            .clone()
            .or_else(pelicool::openrgb::startup_profile);
        let openrgb_live = catalog.openrgb_live;
        let mut app = Self {
            module,
            worker,
            tray: tray::Service::new(cc.egui_ctx.clone(), !no_tray && can_hide && (screenshot.is_none() || preview_close)),
            window_state: if preview_close { WindowState::Asking } else { WindowState::Open },
            preferences: preferences.clone(),
            startup,
            permissions,
            paths,
            remember_close: false,
            start_minimized,
            editor: None,
            framing: pelicool::media::Framing::default(),
            auto_upload: false,
            maintenance,
            settings_dialog: None,
            last_diagnostic: None,
            purge: false,
            _collection_lock: collection_lock,
            mode,
            preset: Preset {
                mode: if mode == DisplayMode::Media {
                    DisplayMode::Monitoring
                } else {
                    mode
                },
                ..Default::default()
            },
            monitoring: None,
            readings: Snapshot::default(),
            history: VecDeque::new(),
            sources: sensors::catalog(Path::new("/sys/class/hwmon")),
            sensor_selection: Selection::default(),
            screenshot,
            screenshot_requested: false,
            preview_progress,
            capture_after: Instant::now() + Duration::from_millis(700),
            dark,
            page,
            cache,
            catalog,
            selected: None,
            shown: None,
            auto_select: None,
            openrgb: "Aguardando aplicação dos ajustes de LEDs.".into(),
            openrgb_sync: preferences.openrgb.as_ref().is_none_or(|p|p.enabled),
            openrgb_use_sdk: preferences.openrgb.as_ref().map_or(openrgb_live, |p|p.sdk),
            openrgb_profiles: pelicool::openrgb::profiles(),
            openrgb_profile: preferences.openrgb.as_ref().map_or(rgb_profile, |p|p.profile.clone()),
            path: file.clone().map(|path| path.canonicalize().unwrap_or(path)),
            media: None,
            texture: None,
            library: vec![],
            thumbnails_pending: 0,
            orientation: Orientation::Auto,
            fit: Fit::Cover,
            settings,
            frame: 0,
            next_frame: Instant::now(),
            playing: true,
            task: None,
            progress: 0.,
            status: if mode == DisplayMode::Media {
                "Tudo pronto. Escolha uma mídia para personalizar seu display."
            } else {
                "Escolha o conteúdo e ative o preset. Pode minimizar o PeliCool; os sensores continuam ativos."
            }.into(),
            error: false,
            detected: detect_device(module),
            connected: false,
            last_sent: None,
            tx,
            rx,
        };
        for entry in app.catalog.entries.clone() {
            app.add_cached(entry);
        }
        if let Some(error) = load_error {
            app.error = true;
            app.status = error;
        }
        if let Some(path) = file {
            app.choose_media(path, None);
        } else if mode == DisplayMode::Media && !app.library.is_empty() {
            let initial_page = app.page;
            let index = app.startup.initial_index(&app.catalog).unwrap_or(0);
            app.select_cached(index);
            app.auto_select = None;
            app.auto_upload = false;
            app.page = initial_page;
        }
        app.startup.arm(restore_on_start, Instant::now());
        Ok(app)
    }

    pub(super) fn receive(&mut self, ctx: &egui::Context) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                Event::EditorLoaded(path, image, replace) => {
                    self.task = None;
                    let framing = replace
                        .and_then(|index| self.library.get(index))
                        .map_or(pelicool::media::Framing::default(), |item| {
                            item.entry.framing
                        });
                    let mut options =
                        pelicool::media::PrepareOptions::new(self.orientation, self.fit);
                    options.framing = framing;
                    options.display_rotation = self
                        .module
                        .info
                        .capabilities
                        .media
                        .filter(|m| m.rotation_requires_upload)
                        .map(|_| self.module.info.display_rotation(self.settings.rotation));
                    self.editor = Some(editor::Editor::new(ctx, path, image, replace, options));
                }
                Event::DiagnosticReady(path) => {
                    self.last_diagnostic = Some(path.clone());
                    self.task = None;
                    self.status = format!(
                        "Diagnóstico salvo em {}. Revise antes de compartilhar; nada foi enviado.",
                        path.display()
                    );
                }
                Event::OpenFolderError(message) => {
                    self.error = true;
                    self.status = message;
                }
                Event::PermissionsFinished(action, result) => {
                    self.permissions_finished(action, result)
                }
                Event::Shown(slot) => {
                    self.shown = Some(slot);
                    if let Err(error) = self.startup.remember(&self.catalog, slot, &self.cache) {
                        self.error = true;
                        self.status = format!("Não foi possível lembrar a última mídia: {error:#}");
                    }
                }
                Event::Catalog(catalog) => {
                    self.catalog = catalog;
                    if self.catalog.entries.is_empty() {
                        self.shown = None;
                    }
                }
                Event::OpenRgb(message) => self.openrgb = message,
                Event::Staged(entry, media, replace) => {
                    let count = self
                        .library
                        .iter()
                        .enumerate()
                        .filter(|(i, item)| {
                            Some(*i) != replace && item.entry.animated == entry.animated
                        })
                        .count();
                    if count >= self.media_limit(entry.animated) {
                        self.task = None;
                        self.error = true;
                        self.status =
                            "Limite de arquivos deste tipo atingido para o módulo.".into();
                        continue;
                    }

                    if let Some(index) = replace {
                        self.library.remove(index);
                        self.catalog.entries.remove(index);
                        self.selected = None;
                    }
                    self.path = Some(entry.source.clone());
                    self.add_cached(entry.clone());
                    self.catalog.entries.push(entry);
                    self.selected = Some(self.library.len() - 1);
                    self.auto_select = None;
                    if let Err(error) = self.cache.save(&self.catalog) {
                        self.error = true;
                        self.status = format!("Não foi possível salvar a coleção: {error:#}");
                        self.task = None;
                        continue;
                    }
                    let _ = self.tx.send(Event::Prepared(media));
                }
                Event::Sensors(snapshot) => {
                    let snapshot = *snapshot;
                    self.history.push_back(snapshot.clone());
                    if self.history.len() > 420 {
                        self.history.pop_front();
                    }
                    self.readings = snapshot;
                }
                Event::Monitoring(mode) => {
                    if mode == Some(DisplayMode::Recorder) && self.monitoring != mode {
                        self.history.clear();
                    }
                    self.monitoring = mode;
                }
                Event::StreamError(message) => {
                    self.monitoring = None;
                    self.connected = false;
                    self.error = true;
                    self.status = message;
                }
                Event::Prepared(media) => {
                    let saved = self
                        .selected
                        .and_then(|index| self.library.get(index))
                        .is_some_and(|item| self.catalog.slot(&item.entry.id).is_some());
                    self.status = if saved {
                        "Mídia já salva no display. Clique numa miniatura para exibir.".to_owned()
                    } else {
                        format!(
                            "{} preparada · {} quadro(s) · {:.1} MiB",
                            if media.animated {
                                "Animação"
                            } else {
                                "Imagem"
                            },
                            media.frames.len(),
                            media.byte_len() as f32 / 1048576.
                        )
                    };
                    self.next_frame =
                        Instant::now() + Duration::from_millis(u64::from(media.frames[0].delay_ms));
                    self.media = Some(media);
                    self.frame = 0;
                    self.task = None;
                    self.texture = None;
                    if std::mem::take(&mut self.auto_upload) {
                        self.operation(Task::Upload);
                    } else if self.auto_select.is_some() {
                        self.operation(Task::Select);
                    }
                }
                Event::Thumbnail(path, image) => {
                    self.thumbnails_pending = self.thumbnails_pending.saturating_sub(1);
                    if let Some(item) = self.library.iter_mut().find(|item| {
                        self.cache.frame_path(&item.entry, 0).ok().as_ref() == Some(&path)
                    }) {
                        item.thumbnail = Some(ctx.load_texture(
                            path.to_string_lossy(),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [image.width() as usize, image.height() as usize],
                                image.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                Event::ThumbnailFailed => {
                    self.thumbnails_pending = self.thumbnails_pending.saturating_sub(1)
                }
                Event::Progress(value) => self.progress = value,
                Event::Done(message) => {
                    self.startup.completed(true, Instant::now());
                    if self.task == Some(Task::ResetGraph) {
                        self.history.clear();
                    }
                    if self.task == Some(Task::Upload) {
                        self.last_sent = Some(format!("{} mídia(s) salvas", self.library.len()));
                    }
                    let stopped = self.task == Some(Task::Stop);
                    self.task = None;
                    self.status = message;
                    if !stopped {
                        self.connected = true;
                        self.detected = true;
                    }
                    if self.media.is_none()
                        && let Some(index) = self.selected
                    {
                        self.select_cached(index);
                    }
                }
                Event::Error(message) => {
                    self.startup.completed(false, Instant::now());
                    if matches!(
                        self.task,
                        Some(Task::Probe | Task::Upload | Task::Apply | Task::Monitor)
                    ) {
                        self.connected = false;
                    }
                    self.task = None;
                    self.auto_upload = false;
                    self.error = true;
                    self.status = message;
                }
            }
        }
        if let Some(media) = &self.media
            && self.window_state != WindowState::Hidden
        {
            if self.playing && media.animated && Instant::now() >= self.next_frame {
                self.frame = (self.frame + 1) % media.frames.len();
                self.next_frame = Instant::now()
                    + Duration::from_millis(u64::from(media.frames[self.frame].delay_ms));
                self.texture = None;
            }
            if self.texture.is_none() {
                let image = &media.frames[self.frame].preview;
                self.texture = Some(ctx.load_texture(
                    "media-preview",
                    egui::ColorImage::from_rgba_unmultiplied(
                        [image.width() as usize, image.height() as usize],
                        image.as_raw(),
                    ),
                    egui::TextureOptions::LINEAR,
                ));
            }
        }
        ctx.request_repaint_after(if self.window_state == WindowState::Hidden {
            Duration::from_secs(1)
        } else {
            Duration::from_millis(250)
        });
        if self.task.is_some()
            || self.thumbnails_pending > 0
            || (self.window_state != WindowState::Hidden
                && self.playing
                && self.media.as_ref().is_some_and(|media| media.animated))
        {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}
