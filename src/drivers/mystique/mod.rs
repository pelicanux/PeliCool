//! DeepCool MYSTIQUE: driver validado no MYSTIQUE 360 (3633:0009).
//! Protocolo e transferências de mídia pertencem a este módulo.
pub mod protocol;
mod rotation;

use crate::{
    device::{DisplayMode, Preset, RgbMode, Settings},
    media::PreparedMedia,
    sensors::Snapshot,
    transport::Transport,
};
use anyhow::{Context as _, Result, ensure};
use chrono::{Datelike, Timelike};
use protocol::{self as p, Command};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ID: &str = "mystique";
pub static INFO: crate::drivers::DriverInfo = crate::drivers::DriverInfo {
    id: ID,
    name: "MYSTIQUE",
    tested_models: &["MYSTIQUE 360"],
    usb: crate::transport::UsbSpec {
        ids: &[crate::transport::UsbId {
            vendor: 0x3633,
            product: 0x0009,
        }],
        interface: 0,
        transfer: crate::transport::Transfer::Bulk,
        endpoints: &[0x01, 0x81, 0x02, 0x82],
        read_size: 64,
    },
    capabilities: crate::drivers::Capabilities {
        modes: &[
            DisplayMode::Monitoring,
            DisplayMode::Media,
            DisplayMode::Recorder,
        ],
        screen_power: true,
        brightness: true,
        rotation: true,
        gyro: true,
        rgb_modes: &[RgbMode::Motherboard, RgbMode::Internal, RgbMode::Media],
        live_sensors: true,
        clock_sync: true,
        native_presets: true,
        media: Some(crate::drivers::MediaSupport {
            format: crate::drivers::MediaFormat::MystiqueJpeg480x640,
            image_slots: 5,
            gif_slots: 5,
            rotation_requires_upload: true,
            transform_revision: 2,
        }),
    },
    default_mode: DisplayMode::Media,
    landscape_upright: 270,
    screen_size: Some((640, 480)),
};

pub const MODULE: crate::drivers::DriverModule = crate::drivers::DriverModule {
    info: &INFO,
    create: |transport| Box::new(Controller::new(transport)),
    prepare_media: crate::media::prepare_with,
};

pub struct Controller<T: Transport> {
    pub transport: T,
    media_settings: Option<Settings>,
}

impl<T: Transport> Controller<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            media_settings: None,
        }
    }

    pub fn command(&mut self, command: Command, payload: &[u8]) -> Result<Vec<u8>> {
        let (ep_out, ep_in) = if matches!(
            command,
            Command::ScreenState | Command::Sensors | Command::Time
        ) {
            p::EP_DATA
        } else {
            p::EP_CMD
        };
        self.transport
            .write(ep_out, &p::packet(command, payload)?)?;
        let raw = self.transport.read(ep_in)?;
        p::validate_reply(&raw, command)?;
        Ok(raw)
    }

    pub fn probe(&mut self) -> Result<String> {
        let raw = self.command(Command::ScreenState, &[])?;
        Ok(format!(
            "MYSTIQUE 3633:0009 respondeu. Estado bruto da tela: {:02x?}",
            &raw[4..8]
        ))
    }

    pub fn apply(&mut self, settings: Settings) -> Result<()> {
        settings.validate()?;
        self.command(Command::Mode, &[2])?;
        let sync = u8::from(settings.rgb == RgbMode::Media);
        self.command(Command::Motion, &[0, sync, sync, 0])?;
        // Modo/seleção podem recarregar a orientação guardada pelo firmware.
        // Envie as opções por último e reaplique-as depois de selecionar um slot.
        self.options(settings, DisplayMode::Media)?;
        self.media_settings = Some(settings);
        // O firmware fica sem slideshow; a sequência mista pertence ao worker.
        Ok(())
    }

    fn options(&mut self, settings: Settings, mode: DisplayMode) -> Result<()> {
        settings.validate()?;
        let rgb = match settings.rgb {
            RgbMode::Motherboard => 0,
            RgbMode::Internal => 1,
            RgbMode::Media => {
                if mode == DisplayMode::Media {
                    2
                } else {
                    1
                }
            }
        };
        let brightness = if settings.screen_on {
            ((f32::from(settings.brightness) * 0.49).round() as u8) + 21
        } else {
            0
        };
        self.command(
            Command::Options,
            &[
                1,
                u8::from(settings.gyro && mode != DisplayMode::Media),
                (settings.rotation / 90) as u8,
                rgb,
                brightness,
            ],
        )?;
        Ok(())
    }

    pub fn configure_preset(
        &mut self,
        settings: Settings,
        preset: Preset,
        reset_graph: bool,
    ) -> Result<()> {
        preset.validate()?;
        settings.validate()?;
        self.media_settings = None;
        self.command(Command::Mode, &[preset.mode as u8])?;
        self.command(
            Command::Situational,
            &[preset.main, preset.style, 0, preset.auxiliary],
        )?;
        self.command(
            Command::Recorder,
            &[
                u8::from(preset.graph_clock),
                u8::from(preset.graph_temperature),
            ],
        )?;
        self.command(Command::TemperatureUnit, &[0])?;
        if reset_graph && preset.mode == DisplayMode::Recorder {
            self.command(Command::RecorderRefresh, &[1])?;
        }
        self.sync_clock()?;
        self.options(settings, preset.mode)?;
        Ok(())
    }

    pub fn sync_clock(&mut self) -> Result<()> {
        let now = chrono::Local::now();
        let year = now.year() as u16;
        self.command(
            Command::Time,
            &[
                year as u8,
                (year >> 8) as u8,
                now.month() as u8,
                now.day() as u8,
                now.hour() as u8,
                now.minute() as u8,
                now.second() as u8,
            ],
        )?;
        Ok(())
    }

    pub fn update_sensors(&mut self, snapshot: &Snapshot) -> Result<()> {
        self.command(Command::ScreenState, &[])?;
        self.command(Command::Sensors, &p::sensor_payload(snapshot))?;
        Ok(())
    }

    pub fn upload(
        &mut self,
        media: &PreparedMedia,
        settings: Settings,
        replace_media: bool,
        progress: impl FnMut(f32),
    ) -> Result<()> {
        ensure!(
            replace_media,
            "O envio exige substituir a mídia armazenada no cooler."
        );
        self.upload_collection(&[media], settings, progress)
    }

    pub fn select_media(&mut self, slot: crate::collection::Slot) -> Result<()> {
        slot.validate()?;
        self.command(
            Command::MotionScreen,
            &[u8::from(slot.animated), slot.index],
        )?;
        if let Some(settings) = self.media_settings {
            self.options(settings, DisplayMode::Media)?;
        }
        Ok(())
    }

    pub fn upload_collection(
        &mut self,
        media: &[&PreparedMedia],
        settings: Settings,
        mut progress: impl FnMut(f32),
    ) -> Result<()> {
        settings.validate()?;
        crate::collection::validate_batch(media)?;
        // O firmware gira os presets e o Loading, mas não os JPEGs armazenados.
        // Converta tudo antes de qualquer comando que possa apagar a coleção.
        let rotated = rotation::collection(media, settings.rotation)?;
        let media: Vec<_> = rotated.iter().collect();
        let generation = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let mut files = Vec::new();
        for (item, media) in media.iter().enumerate() {
            let id = format!("{:032x}", generation.wrapping_add(item as u128));
            let id: &[u8; 32] = id.as_bytes().try_into()?;
            for (index, frame) in media.frames.iter().enumerate() {
                let header = p::file_header(
                    if media.animated { 2 } else { 1 },
                    &frame.jpeg,
                    id,
                    if media.animated {
                        media.frames.len() as u16
                    } else {
                        0
                    },
                    media.duration_ms,
                    index as u16,
                )?;
                files.push([header.as_slice(), &frame.jpeg].concat());
            }
        }
        self.apply(settings)?;
        self.command(Command::FormatResources, &[])?;
        self.command(Command::StartDownload, &[])?;
        let total: usize = files.iter().map(Vec::len).sum();
        let mut sent = 0;
        for file in files {
            for chunk in file.chunks(64) {
                self.transport
                    .write(p::EP_CMD.0, chunk)
                    .context("Falha durante envio: a coleção no cooler pode estar incompleta")?;
                sent += chunk.len();
                progress(sent as f32 / total as f32);
            }
        }
        self.transport.write(p::EP_CMD.0, p::FINISH)?;
        std::thread::sleep(Duration::from_millis(150));
        self.apply(settings)?;
        self.select_media(crate::collection::Slot {
            animated: media[0].animated,
            index: 0,
        })?;
        Ok(())
    }
}

impl<T: Transport + Send + 'static> crate::drivers::DisplayDriver for Controller<T> {
    fn probe(&mut self) -> Result<String> {
        self.probe()
    }
    fn clear_media(&mut self, settings: Settings) -> Result<()> {
        self.apply(settings)?;
        self.command(Command::FormatResources, &[])?;
        Ok(())
    }
    fn apply(&mut self, settings: Settings) -> Result<()> {
        self.apply(settings)
    }
    fn configure_preset(&mut self, settings: Settings, preset: Preset, reset: bool) -> Result<()> {
        self.configure_preset(settings, preset, reset)
    }
    fn update_sensors(&mut self, snapshot: &Snapshot) -> Result<()> {
        self.update_sensors(snapshot)
    }
    fn sync_clock(&mut self) -> Result<()> {
        self.sync_clock()
    }
    fn reset_graph(&mut self) -> Result<()> {
        self.command(Command::RecorderRefresh, &[1])?;
        Ok(())
    }
    fn select_media(&mut self, slot: crate::collection::Slot) -> Result<()> {
        self.select_media(slot)
    }
    fn upload_collection(
        &mut self,
        media: &[&PreparedMedia],
        settings: Settings,
        progress: &mut dyn FnMut(f32),
    ) -> Result<()> {
        self.upload_collection(media, settings, progress)
    }
}
