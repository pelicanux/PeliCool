//! Copie para src/drivers/seu_modelo/mod.rs e implemente com hardware real.
//! Este template compila, mas NÃO é um módulo registrado nem suporte funcional.
use anyhow::{Result, bail};
use pelicool::{
    device::DisplayMode,
    drivers::{Capabilities, DisplayDriver, DriverInfo, DriverModule},
    transport::{Transfer, Transport, UsbSpec},
};

pub static INFO: DriverInfo = DriverInfo {
    id: "novo-modelo",
    name: "Novo modelo com display",
    tested_models: &[],
    usb: UsbSpec {
        ids: &[], // Preencha com IDs confirmados; vazio impede registrar o template.
        interface: 0,
        transfer: Transfer::Bulk, // Ou Interrupt, conforme os descritores reais.
        endpoints: &[],
        read_size: 64,
    },
    capabilities: Capabilities {
        modes: &[DisplayMode::Monitoring],
        screen_power: false,
        brightness: false,
        rotation: false,
        gyro: false,
        rgb_modes: &[],
        live_sensors: true,
        clock_sync: false,
        native_presets: false,
        media: None,
    },
    default_mode: DisplayMode::Monitoring,
    landscape_upright: 0,
    screen_size: None,
};

struct Driver {
    _transport: Box<dyn Transport + Send>,
}

impl DisplayDriver for Driver {
    fn probe(&mut self) -> Result<String> {
        // Implemente uma consulta realmente documentada para este aparelho.
        // Não reutilize comandos MYSTIQUE apenas porque o fabricante coincide.
        bail!("Template: protocolo ainda não implementado.")
    }
    // Implemente configure_preset e update_sensors para o modo Monitoring.
    // Métodos opcionais herdados falham sem escrever no transporte.
}

pub const MODULE: DriverModule = DriverModule {
    info: &INFO,
    create: |transport| {
        Box::new(Driver {
            _transport: transport,
        })
    },
    prepare_media: pelicool::drivers::unavailable_media,
};
