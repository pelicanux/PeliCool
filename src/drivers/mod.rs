//! Módulos compilados do PeliCool. Registre novos modelos em REGISTERED;
//! protocolo, IDs USB e conversão de mídia ficam no módulo correspondente.
pub mod mystique;

use crate::{
    collection::Slot,
    device::{Device, DisplayMode, Preset, RgbMode, Settings},
    media::{Fit, Orientation, PrepareOptions, PreparedMedia},
    sensors::Snapshot,
    transport::{Transport, UsbId, UsbSpec, UsbTransport},
};
use anyhow::{Context, Result, bail, ensure};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaFormat {
    /// Contrato atual do cache: JPEG portrait 480×640; landscape packed 90°.
    /// Outro formato exige seu próprio conversor/cache antes de habilitar mídia.
    MystiqueJpeg480x640,
}
impl MediaFormat {
    /// Dimensões dos pixels codificados, independentemente da posição do conteúdo.
    pub fn encoded_size(self) -> (u16, u16) {
        match self {
            Self::MystiqueJpeg480x640 => (480, 640),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MediaSupport {
    pub format: MediaFormat,
    pub image_slots: u8,
    pub gif_slots: u8,
    /// O modelo exige preparar/enviar novamente os pixels ao mudar o ângulo.
    pub rotation_requires_upload: bool,
    /// Revisão do preparo de pixels; mudanças invalidam só a confirmação do envio.
    pub transform_revision: u8,
}

#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub modes: &'static [DisplayMode],
    pub screen_power: bool,
    pub brightness: bool,
    pub rotation: bool,
    pub gyro: bool,
    pub rgb_modes: &'static [RgbMode],
    pub live_sensors: bool,
    pub clock_sync: bool,
    pub native_presets: bool,
    pub media: Option<MediaSupport>,
}

#[derive(Clone, Copy, Debug)]
pub struct DriverInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub tested_models: &'static [&'static str],
    pub usb: UsbSpec,
    pub capabilities: Capabilities,
    pub default_mode: DisplayMode,
    pub landscape_upright: u16,
    pub screen_size: Option<(u16, u16)>,
}
impl DriverInfo {
    pub fn display_rotation(&self, raw: u16) -> u16 {
        (raw + 360 - self.landscape_upright) % 360
    }
    pub fn raw_rotation(&self, display: u16) -> u16 {
        (display + self.landscape_upright) % 360
    }
}

/// Interface comum. Métodos opcionais falham sem I/O por padrão.
/// Nunca é necessário expor comandos USB brutos à interface gráfica.
pub trait DisplayDriver: Send {
    fn probe(&mut self) -> Result<String>;
    fn clear_media(&mut self, _settings: Settings) -> Result<()> {
        bail!("Remoção de mídia não implementada neste módulo.");
    }
    fn apply(&mut self, _settings: Settings) -> Result<()> {
        bail!("Ajustes de mídia não implementados neste módulo.")
    }
    fn configure_preset(
        &mut self,
        _settings: Settings,
        _preset: Preset,
        _reset: bool,
    ) -> Result<()> {
        bail!("Monitoramento não implementado neste módulo.")
    }
    fn update_sensors(&mut self, _snapshot: &Snapshot) -> Result<()> {
        bail!("Envio de sensores não implementado neste módulo.")
    }
    fn sync_clock(&mut self) -> Result<()> {
        bail!("Sincronização do relógio não implementada neste módulo.")
    }
    fn reset_graph(&mut self) -> Result<()> {
        bail!("Reinício de gráficos não implementado neste módulo.")
    }
    fn select_media(&mut self, _slot: Slot) -> Result<()> {
        bail!("Seleção de mídia não implementada neste módulo.")
    }
    fn upload_collection(
        &mut self,
        _media: &[&PreparedMedia],
        _settings: Settings,
        _progress: &mut dyn FnMut(f32),
    ) -> Result<()> {
        bail!("Envio de mídia não implementado neste módulo.")
    }
}

#[derive(Clone, Copy)]
pub struct DriverModule {
    pub info: &'static DriverInfo,
    pub create: fn(Box<dyn Transport + Send>) -> Box<dyn DisplayDriver>,
    pub prepare_media: fn(&Path, PrepareOptions) -> Result<PreparedMedia>,
}

impl DriverModule {
    pub fn prepare_for_display(
        &self,
        path: &Path,
        mut options: PrepareOptions,
        settings: Settings,
    ) -> Result<PreparedMedia> {
        settings.validate()?;
        if self
            .info
            .capabilities
            .media
            .is_some_and(|m| m.rotation_requires_upload)
        {
            options.display_rotation = Some(self.info.display_rotation(settings.rotation));
        }
        self.prepare_with(path, options)
    }
    pub fn connect(&'static self, transport: Box<dyn Transport + Send>) -> Device {
        Device::new(self, (self.create)(transport))
    }

    pub fn open(&'static self) -> Result<Device> {
        Ok(self.connect(Box::new(UsbTransport::open(self.info)?)))
    }

    pub fn prepare(
        &self,
        path: &Path,
        orientation: Orientation,
        fit: Fit,
    ) -> Result<PreparedMedia> {
        ensure!(
            self.info.capabilities.media.is_some(),
            "O módulo {} não aceita imagens/GIFs.",
            self.info.id
        );
        self.prepare_with(path, PrepareOptions::new(orientation, fit))
    }
    pub fn prepare_with(&self, path: &Path, options: PrepareOptions) -> Result<PreparedMedia> {
        ensure!(
            self.info.capabilities.media.is_some(),
            "O módulo não aceita mídia."
        );
        (self.prepare_media)(path, options)
    }
}

/// Único ponto de registro. O template em tests/common/driver_template.rs não é registrado.
pub static REGISTERED: &[DriverModule] = &[mystique::MODULE];

pub fn by_id(id: &str) -> Result<&'static DriverModule> {
    REGISTERED
        .iter()
        .find(|module| module.info.id == id)
        .with_context(|| format!("Módulo desconhecido: {id}. Consulte pelicool drivers."))
}

pub fn matching(modules: &[DriverModule], id: UsbId) -> Result<Option<&DriverModule>> {
    let mut matches = modules
        .iter()
        .filter(|module| module.info.usb.ids.contains(&id));
    let first = matches.next();
    ensure!(
        matches.next().is_none(),
        "Identificador USB {:04x}:{:04x} reivindicado por mais de um módulo.",
        id.vendor,
        id.product
    );
    Ok(first)
}

/// Somente sysfs: não abre o dispositivo e não envia comandos USB.
pub fn detected_at(root: &Path) -> Result<Vec<&'static DriverModule>> {
    let mut found = vec![];
    for entry in std::fs::read_dir(root)
        .context("Não foi possível listar dispositivos USB em sysfs")?
        .flatten()
    {
        let id = |name| {
            std::fs::read_to_string(entry.path().join(name))
                .ok()
                .and_then(|s| u16::from_str_radix(s.trim(), 16).ok())
        };
        if let (Some(vendor), Some(product)) = (id("idVendor"), id("idProduct"))
            && let Some(module) = matching(REGISTERED, UsbId { vendor, product })?
        {
            found.push(module);
        }
    }
    Ok(found)
}

/// Sem aparelho, a interface/conversão offline usa o módulo inicial existente.
/// Mais de um alvo exige escolher o módulo; open() ainda rejeita duas unidades.
pub fn resolve(id: Option<&str>) -> Result<&'static DriverModule> {
    if let Some(id) = id {
        return by_id(id);
    }
    let found = detected_at(Path::new("/sys/bus/usb/devices"))?;
    ensure!(
        found.len() <= 1,
        "Mais de um cooler compatível conectado. Escolha um módulo com --driver ID e mantenha uma unidade por módulo."
    );
    match found.first() {
        Some(module) => Ok(*module),
        None => by_id(mystique::ID),
    }
}

pub fn unavailable_media(_path: &Path, _options: PrepareOptions) -> Result<PreparedMedia> {
    bail!("Este módulo não oferece conversão de mídia.")
}

pub fn validate_registry(modules: &[DriverModule]) -> Result<()> {
    ensure!(!modules.is_empty(), "Nenhum módulo registrado.");
    let mut names = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    for module in modules {
        let info = module.info;
        ensure!(
            !info.id.is_empty()
                && info
                    .id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
            "ID de módulo inválido."
        );
        ensure!(
            names.insert(info.id),
            "ID de módulo duplicado: {}.",
            info.id
        );
        ensure!(
            !info.usb.ids.is_empty()
                && info.usb.read_size > 0
                && info.usb.read_size <= 65536
                && !info.usb.endpoints.is_empty(),
            "Especificação USB incompleta em {}.",
            info.id
        );
        for id in info.usb.ids {
            ensure!(
                ids.insert(*id),
                "USB {:04x}:{:04x} duplicado no registro.",
                id.vendor,
                id.product
            );
        }
        let caps = info.capabilities;
        ensure!(
            !caps.rgb_modes.contains(&RgbMode::Internal) || caps.live_sensors,
            "Alerta térmico exige envio de sensores em {}.",
            info.id
        );
        ensure!(
            !caps.rgb_modes.contains(&RgbMode::Media) || caps.media.is_some(),
            "Iluminação por mídia exige suporte de mídia em {}.",
            info.id
        );
        if let Some((width, height)) = info.screen_size {
            ensure!(
                width > 0 && height > 0,
                "Dimensões de tela inválidas em {}.",
                info.id
            );
        }
        ensure!(
            caps.modes.contains(&info.default_mode),
            "Modo padrão não suportado em {}.",
            info.id
        );
        ensure!(
            caps.modes.contains(&DisplayMode::Media) == caps.media.is_some(),
            "Modo de mídia sem perfil de conversão, ou perfil sem modo, em {}.",
            info.id
        );
        ensure!(
            !caps.modes.iter().any(|mode| *mode != DisplayMode::Media) || caps.live_sensors,
            "Monitoramento exige envio de sensores em {}.",
            info.id
        );
        ensure!(
            [0, 90, 180, 270].contains(&info.landscape_upright),
            "Referência de rotação inválida."
        );
        if let Some(media) = caps.media {
            ensure!(
                !media.rotation_requires_upload || caps.rotation,
                "Rotação por pixels exige suporte de rotação em {}.",
                info.id
            );
            ensure!(
                media.image_slots <= 5
                    && media.gif_slots <= 5
                    && media.image_slots + media.gif_slots > 0,
                "O cache atual admite até cinco slots por tipo."
            );
        }
    }
    Ok(())
}
