//! Tipos compartilhados e sessão de dispositivo independente do modelo.
use crate::{media::PreparedMedia, sensors::Snapshot};
use anyhow::{Context, Result, ensure};

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    clap::ValueEnum,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum RgbMode {
    Motherboard,
    Internal,
    #[default]
    Media,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, clap::ValueEnum)]
#[repr(u8)]
pub enum DisplayMode {
    Monitoring = 1,
    #[default]
    Media = 2,
    #[value(name = "recording", alias = "recorder")]
    Recorder = 3,
}

#[derive(Clone, Copy, Debug)]
pub struct Preset {
    pub mode: DisplayMode,
    pub main: u8,
    pub style: u8,
    pub auxiliary: u8,
    pub graph_clock: bool,
    pub graph_temperature: bool,
}
impl Default for Preset {
    fn default() -> Self {
        Self {
            mode: DisplayMode::Monitoring,
            main: 5,
            style: 0,
            auxiliary: 1,
            graph_clock: true,
            graph_temperature: true,
        }
    }
}
impl Preset {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.mode != DisplayMode::Media,
            "Escolha monitoramento ou gráficos para usar um preset."
        );
        ensure!(
            self.main <= 5 && self.style <= 2 && self.auxiliary <= 2,
            "Configuração de preset inválida."
        );
        ensure!(
            self.mode != DisplayMode::Recorder || self.graph_clock || self.graph_temperature,
            "Selecione ao menos uma curva do gráfico."
        );
        Ok(())
    }
    pub fn check_sources(&self, snapshot: &Snapshot) -> Result<()> {
        self.validate()?;
        if self.mode == DisplayMode::Monitoring && self.auxiliary == 0 {
            ensure!(
                snapshot.v33.is_some() && snapshot.v5.is_some() && snapshot.v12.is_some(),
                "Sensores das tensões 3.3V, 5V e 12V indisponíveis."
            );
        }
        if self.mode == DisplayMode::Recorder {
            ensure!(
                !self.graph_temperature || snapshot.cpu_temp.is_some(),
                "Selecione um sensor de temperatura da CPU disponível."
            );
            ensure!(
                !self.graph_clock || snapshot.cpu_mhz.is_some(),
                "Frequência da CPU indisponível."
            );
        } else {
            match self.main {
                0 => ensure!(
                    snapshot.cpu_mhz.is_some(),
                    "Frequência da CPU indisponível."
                ),
                2 => ensure!(
                    snapshot.pump_rpm.is_some(),
                    "Selecione o sensor de RPM da bomba na página Dispositivo."
                ),
                3 => ensure!(
                    snapshot.fan_rpm.is_some(),
                    "Selecione o sensor de RPM da ventoinha da CPU na página Dispositivo."
                ),
                4 => ensure!(
                    snapshot.fan_rpm.is_some() && snapshot.pump_rpm.is_some(),
                    "Selecione os sensores de RPM da ventoinha e da bomba."
                ),
                5 => ensure!(
                    snapshot.cpu_temp.is_some(),
                    "Selecione um sensor de temperatura da CPU disponível."
                ),
                _ => (),
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub screen_on: bool,
    pub gyro: bool,
    pub slideshow: bool,
    pub slide_seconds: u8,
    pub rgb: RgbMode,
    pub brightness: u8,
    /// Raw device angle. DriverInfo supplies the preview's upright reference;
    /// never replace saved values with preview-relative angles.
    pub rotation: u16,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            screen_on: true,
            gyro: false,
            slideshow: false,
            slide_seconds: 5,
            rgb: RgbMode::Media,
            brightness: 80,
            rotation: 0,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            [3, 5, 7].contains(&self.slide_seconds),
            "Configuração de slideshow inválida."
        );
        ensure!(self.brightness <= 100, "Brilho deve estar entre 0 e 100.");
        ensure!(
            [0, 90, 180, 270].contains(&self.rotation),
            "Rotação inválida."
        );
        Ok(())
    }
}

/// Sessão usada por GUI/CLI. Só delega recursos anunciados pelo módulo.
pub struct Device {
    module: &'static crate::drivers::DriverModule,
    driver: Box<dyn crate::drivers::DisplayDriver>,
}

impl Device {
    pub fn module(&self) -> &'static crate::drivers::DriverModule {
        self.module
    }
    pub(crate) fn new(
        module: &'static crate::drivers::DriverModule,
        driver: Box<dyn crate::drivers::DisplayDriver>,
    ) -> Self {
        Self { module, driver }
    }

    pub fn info(&self) -> &'static crate::drivers::DriverInfo {
        self.module.info
    }

    pub fn probe(&mut self) -> Result<String> {
        self.driver.probe()
    }

    fn require_mode(&self, mode: DisplayMode) -> Result<()> {
        ensure!(
            self.info().capabilities.modes.contains(&mode),
            "O módulo {} não oferece o modo {mode:?}.",
            self.info().id
        );
        Ok(())
    }

    fn check_settings(&self, settings: Settings) -> Result<()> {
        settings.validate()?;
        let caps = self.info().capabilities;
        let defaults = Settings::default();
        ensure!(
            caps.screen_power || settings.screen_on,
            "Este módulo não controla energia da tela."
        );
        ensure!(
            caps.brightness || settings.brightness == defaults.brightness,
            "Este módulo não ajusta brilho."
        );
        ensure!(
            caps.rotation || settings.rotation == 0,
            "Este módulo não ajusta rotação."
        );
        ensure!(
            caps.gyro || !settings.gyro,
            "Este módulo não oferece giroscópio."
        );
        ensure!(
            !settings.slideshow || caps.media.is_some(),
            "Este módulo não oferece slideshow."
        );
        ensure!(
            caps.rgb_modes.contains(&settings.rgb)
                || (caps.rgb_modes.is_empty() && settings.rgb == defaults.rgb),
            "Modo de iluminação não suportado pelo módulo {}.",
            self.info().id
        );
        Ok(())
    }

    pub fn apply(&mut self, settings: Settings) -> Result<()> {
        self.require_mode(DisplayMode::Media)?;
        self.check_settings(settings)?;
        self.driver.apply(settings)
    }

    pub fn configure_preset(
        &mut self,
        settings: Settings,
        preset: Preset,
        reset_graph: bool,
    ) -> Result<()> {
        self.require_mode(preset.mode)?;
        preset.validate()?;
        self.check_settings(settings)?;
        self.driver.configure_preset(settings, preset, reset_graph)
    }

    pub fn update_sensors(&mut self, snapshot: &Snapshot) -> Result<()> {
        ensure!(
            self.info().capabilities.live_sensors,
            "Este módulo não recebe sensores."
        );
        self.driver.update_sensors(snapshot)
    }

    pub fn sync_clock(&mut self) -> Result<()> {
        if self.info().capabilities.clock_sync {
            self.driver.sync_clock()?;
        }
        Ok(())
    }

    pub fn reset_graph(&mut self) -> Result<()> {
        self.require_mode(DisplayMode::Recorder)?;
        self.driver.reset_graph()
    }

    pub fn select_media(&mut self, slot: crate::collection::Slot) -> Result<()> {
        self.require_mode(DisplayMode::Media)?;
        slot.validate()?;
        let media = self
            .info()
            .capabilities
            .media
            .context("Perfil de mídia ausente")?;
        let limit = if slot.animated {
            media.gif_slots
        } else {
            media.image_slots
        };
        ensure!(
            slot.index < limit,
            "Slot não suportado pelo módulo {}.",
            self.info().id
        );
        self.driver.select_media(slot)
    }

    pub fn validate_collection(&self, media: &[&PreparedMedia], settings: Settings) -> Result<()> {
        self.require_mode(DisplayMode::Media)?;
        self.check_settings(settings)?;
        crate::collection::validate_batch(media)?;
        let limits = self
            .info()
            .capabilities
            .media
            .context("Perfil de mídia ausente")?;
        for (animated, limit) in [(false, limits.image_slots), (true, limits.gif_slots)] {
            ensure!(
                media.iter().filter(|m| m.animated == animated).count() <= usize::from(limit),
                "Quantidade de mídias excede os slots do módulo {}.",
                self.info().id
            );
        }
        Ok(())
    }

    pub fn validate_clear_media(&self, settings: Settings) -> Result<()> {
        self.require_mode(DisplayMode::Media)?;
        ensure!(
            self.info().capabilities.media.is_some(),
            "O módulo não oferece mídia."
        );
        self.check_settings(settings)
    }
    pub fn clear_media(&mut self, settings: Settings) -> Result<()> {
        self.validate_clear_media(settings)?;
        self.driver.clear_media(settings)
    }

    pub fn upload_collection(
        &mut self,
        media: &[&PreparedMedia],
        settings: Settings,
        mut progress: impl FnMut(f32),
    ) -> Result<()> {
        self.validate_collection(media, settings)?;
        self.driver
            .upload_collection(media, settings, &mut progress)
    }
}

pub fn open() -> Result<Device> {
    crate::drivers::resolve(None)?.open()
}

pub fn describe_error(error: &anyhow::Error) -> String {
    if let Some(rusb::Error::Access) = error.downcast_ref::<rusb::Error>() {
        return "Sem permissão USB. Instale a regra udev descrita no README e reconecte o dispositivo.".into();
    }
    if let Some(rusb::Error::Busy) = error.downcast_ref::<rusb::Error>() {
        return "USB ocupado. Feche outros aplicativos ou VMs que usam o display.".into();
    }
    format!("{error:#}")
}
