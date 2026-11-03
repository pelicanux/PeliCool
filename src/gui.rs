mod application;
mod brand;
mod capture;
mod closing;
mod dashboard;
mod editor;
mod layout;
mod library;
mod media_grid;
mod operations;
mod permissions;
mod presets;
mod preview;
mod progress;
mod rotation;
mod settings;
mod startup;
mod state;
mod theme;
mod tray;
mod widgets;
mod window;
mod worker;

use anyhow::Result;
use closing::WindowState;
use eframe::egui::{self};
use pelicool::{
    collection::{Cache, Catalog, Entry, Slot},
    device::{DisplayMode, Preset, RgbMode, Settings},
    drivers::{self, DriverModule},
    media::{Fit, Orientation, PreparedMedia},
    sensors::{Selection, Snapshot, Source},
};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{Receiver, Sender},
    },
    time::Instant,
};
use theme::Palette;
use widgets::Icon;

#[derive(Clone, Copy, PartialEq, Default, clap::ValueEnum)]
pub enum Page {
    #[default]
    Personalize,
    Device,
    Settings,
}

/// Estados visuais para capturas de documentação; nunca iniciam operações USB.
#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ProgressPreview {
    Preparing,
    Uploading,
}

#[derive(Default)]
pub struct Options {
    pub driver: Option<String>,
    pub file: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub dark: Option<bool>,
    pub page: Page,
    pub mode: DisplayMode,
    pub no_tray: bool,
    pub preview_close: bool,
    pub preview_udev: bool,
    pub start_minimized: bool,
    pub preview_progress: Option<ProgressPreview>,
}
#[derive(Clone, Copy, PartialEq)]
enum Task {
    Prepare,
    LoadPreview,
    Diagnose,
    Probe,
    Apply,
    Upload,
    Monitor,
    Stop,
    ResetGraph,
    Select,
}
enum Event {
    EditorLoaded(PathBuf, image::RgbaImage, Option<usize>),
    DiagnosticReady(PathBuf),
    OpenFolderError(String),
    PermissionsFinished(
        pelicool::permissions::Action,
        std::result::Result<String, String>,
    ),
    Sensors(Box<Snapshot>),
    Monitoring(Option<DisplayMode>),
    StreamError(String),
    Prepared(Arc<PreparedMedia>),
    Staged(Entry, Arc<PreparedMedia>, Option<usize>),
    Catalog(Catalog),
    OpenRgb(String),
    Shown(Slot),
    Thumbnail(PathBuf, image::RgbaImage),
    ThumbnailFailed,
    Progress(f32),
    Done(String),
    Error(String),
}
struct LibraryItem {
    entry: Entry,
    thumbnail: Option<egui::TextureHandle>,
}

struct App {
    module: &'static DriverModule,
    worker: worker::Worker,
    tray: tray::Service,
    window_state: WindowState,
    paths: pelicool::storage::paths::AppPaths,
    preferences: pelicool::storage::preferences::Interface,
    startup: startup::Startup,
    permissions: permissions::Permissions,
    remember_close: bool,
    start_minimized: bool,
    editor: Option<editor::Editor>,
    framing: pelicool::media::Framing,
    auto_upload: bool,
    maintenance: Arc<std::sync::Mutex<Option<application::Maintenance>>>,
    settings_dialog: Option<application::Dialog>,
    last_diagnostic: Option<PathBuf>,
    purge: bool,
    _collection_lock: pelicool::collection::CacheLock,
    mode: DisplayMode,
    preset: Preset,
    monitoring: Option<DisplayMode>,
    readings: Snapshot,
    history: VecDeque<Snapshot>,
    sources: Vec<Source>,
    sensor_selection: Selection,
    screenshot: Option<PathBuf>,
    screenshot_requested: bool,
    preview_progress: Option<ProgressPreview>,
    capture_after: Instant,
    dark: bool,
    page: Page,
    cache: Cache,
    catalog: Catalog,
    selected: Option<usize>,
    shown: Option<Slot>,
    auto_select: Option<Slot>,
    openrgb: String,
    openrgb_sync: bool,
    openrgb_use_sdk: bool,
    openrgb_profiles: Vec<PathBuf>,
    openrgb_profile: Option<PathBuf>,
    path: Option<PathBuf>,
    media: Option<Arc<PreparedMedia>>,
    texture: Option<egui::TextureHandle>,
    library: Vec<LibraryItem>,
    thumbnails_pending: usize,
    orientation: Orientation,
    fit: Fit,
    settings: Settings,
    frame: usize,
    next_frame: Instant,
    playing: bool,
    task: Option<Task>,
    progress: f32,
    status: String,
    error: bool,
    detected: bool,
    connected: bool,
    last_sent: Option<String>,
    tx: Sender<Event>,
    rx: Receiver<Event>,
}

impl App {
    fn save_interface(&mut self) {
        self.preferences.dark = self.dark;
        self.preferences.openrgb = Some(pelicool::storage::preferences::OpenRgb {
            enabled: self.openrgb_sync,
            sdk: self.openrgb_use_sdk,
            profile: self.openrgb_profile.clone(),
        });
        if let Err(error) = self.preferences.save(&self.paths.config) {
            self.error = true;
            self.status = format!("Não foi possível salvar preferências: {error:#}");
        }
    }
    fn set_dark(&mut self, ctx: &egui::Context, dark: bool) {
        self.dark = dark;
        Palette::new(dark).install(ctx);
        self.save_interface();
    }

    fn media_limit(&self, animated: bool) -> usize {
        self.module.info.capabilities.media.map_or(0, |media| {
            usize::from(if animated {
                media.gif_slots
            } else {
                media.image_slots
            })
        })
    }

    fn model_name(&self) -> &'static str {
        self.module.info.name
    }
}

fn is_gif(path: &Path) -> bool {
    path.extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"))
}
fn is_media(path: &Path) -> bool {
    path.extension().is_some_and(|ext| {
        ["jpg", "jpeg", "png", "webp", "gif"]
            .iter()
            .any(|value| ext.eq_ignore_ascii_case(value))
    })
}
fn thumbnail(path: &Path) -> Result<image::RgbaImage> {
    anyhow::ensure!(
        path.metadata()?.len() <= 50 * 1024 * 1024,
        "Arquivo grande demais."
    );
    let mut reader = image::ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    Ok(reader.decode()?.thumbnail(220, 140).to_rgba8())
}
fn detect_device(module: &'static DriverModule) -> bool {
    drivers::detected_at(Path::new("/sys/bus/usb/devices"))
        .is_ok_and(|found| found.iter().any(|found| found.info.id == module.info.id))
}
fn rgb_label(mode: RgbMode) -> &'static str {
    match mode {
        RgbMode::Media => "Acompanhar imagem/GIF",
        RgbMode::Motherboard => "Sincronizar com a placa-mãe",
        RgbMode::Internal => "Alerta térmico da CPU",
    }
}
fn orientation_label(value: Orientation) -> &'static str {
    match value {
        Orientation::Auto => "Automático",
        Orientation::Horizontal => "Horizontal",
        Orientation::Vertical => "Vertical",
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive(ctx);
        self.restore_startup(ctx);
        if let Some(preview) = self.preview_progress {
            self.task = Some(match preview {
                ProgressPreview::Preparing => Task::Prepare,
                ProgressPreview::Uploading => Task::Upload,
            });
            self.progress = 0.36;
            self.status = match preview {
                ProgressPreview::Preparing => "Preparando mídia e salvando uma cópia local…",
                ProgressPreview::Uploading => "Salvando mídias no display…",
            }
            .into();
        }
        self.screenshots(ctx);
        self.desktop_events(ctx);
        if self.window_state == WindowState::Hidden {
            return;
        }
        if self.task.is_none() && self.editor.is_none() && self.settings_dialog.is_none() {
            let path = ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.as_ref().filter(|path| is_media(path)).cloned())
            });
            if let Some(path) = path {
                self.choose_media(path, None);
            }
        }
        let p = Palette::new(self.dark);
        self.sidebar(ctx);
        self.topbar(ctx, p);
        self.footer(ctx, p);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(p.background)
                    .inner_margin(egui::Margin::symmetric(16, 12)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match self.page {
                        Page::Personalize => self.personalize(ui, p),
                        Page::Device => self.device_page(ui, p),
                        Page::Settings => self.application_page(ui, p),
                    });
            });
        self.close_dialog(ctx, p);
        self.editor_dialog(ctx, p);
        self.settings_dialog(ctx, p);
        self.permission_dialog(ctx, p);
        if self.screenshot.is_some()
            && !self.screenshot_requested
            && Instant::now() >= self.capture_after
            && (self.task.is_none() || self.preview_progress.is_some())
            && (self.preview_progress.is_none() || self.media.is_some() || self.library.is_empty())
            && self.thumbnails_pending == 0
            && ((self.mode == DisplayMode::Media && self.page == Page::Personalize)
                || self.history.len() >= 2
                || self.page == Page::Settings)
        {
            self.screenshot_requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
            ctx.request_repaint();
        }
    }
}

pub fn run(mut options: Options) -> Result<()> {
    anyhow::ensure!(
        options.preview_progress.is_none() || options.screenshot.is_some(),
        "A prévia de progresso exige uma captura de documentação."
    );
    drivers::validate_registry(drivers::REGISTERED)?;
    let module = drivers::resolve(options.driver.as_deref())?;
    if !module.info.capabilities.modes.contains(&options.mode) {
        options.mode = module.info.default_mode;
    }
    let cache = Cache::for_driver(module.info.id)?;
    let collection_lock = cache.lock()?;
    // winit 0.30 cannot hide, unminimize or focus native Wayland windows.
    // Use X11/XWayland for recoverable tray windows when it is available;
    // --no-tray and Wayland-only sessions retain normal window minimization.
    let use_x11 =
        !options.no_tray && std::env::var_os("DISPLAY").is_some_and(|display| !display.is_empty());
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(brand::NAME)
            .with_app_id("pelicool")
            .with_icon(brand::window_icon())
            .with_inner_size([1180., 680.])
            .with_min_inner_size([1000., 640.]),
        event_loop_builder: use_x11.then(|| {
            Box::new(
                |builder: &mut eframe::EventLoopBuilder<eframe::UserEvent>| {
                    use winit::platform::x11::EventLoopBuilderExtX11;
                    builder.with_x11();
                },
            ) as eframe::EventLoopBuilderHook
        }),
        ..Default::default()
    };
    let maintenance = Arc::new(std::sync::Mutex::new(None));
    let requested = maintenance.clone();
    eframe::run_native(
        "pelicool",
        native_options,
        Box::new(move |cc| {
            Ok(Box::new(App::new(
                cc,
                options,
                module,
                cache,
                collection_lock,
                requested,
            )?))
        }),
    )
    .map_err(|error| anyhow::anyhow!("Não foi possível abrir a interface: {error}"))?;
    if let Some(action) = maintenance.lock().expect("maintenance lock").take() {
        let result = application::perform(action);
        let message = match &result {
            Ok(message) => message.clone(),
            Err(error) => format!("{error:#}"),
        };
        rfd::MessageDialog::new()
            .set_title("PeliCool")
            .set_description(message)
            .show();
        result?;
    }
    Ok(())
}

pub fn play_collection(seconds: u64, interval: u8, module: &'static DriverModule) -> Result<()> {
    worker::play_collection(seconds, interval, module)
}
