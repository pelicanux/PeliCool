//! Transporte USB compartilhado; não conhece comandos de nenhum cooler.
use crate::drivers::DriverInfo;
use anyhow::{Context as _, Result, ensure};
use rusb::{Context, DeviceHandle, UsbContext};
use std::time::Duration;

pub trait Transport {
    fn write(&mut self, endpoint: u8, bytes: &[u8]) -> Result<()>;
    fn read(&mut self, endpoint: u8) -> Result<Vec<u8>>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UsbId {
    pub vendor: u16,
    pub product: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transfer {
    Bulk,
    Interrupt,
}

#[derive(Clone, Copy, Debug)]
pub struct UsbSpec {
    pub ids: &'static [UsbId],
    pub interface: u8,
    pub transfer: Transfer,
    pub endpoints: &'static [u8],
    pub read_size: usize,
}

pub struct UsbTransport {
    handle: DeviceHandle<Context>,
    spec: UsbSpec,
}

impl UsbTransport {
    pub fn open(info: &DriverInfo) -> Result<Self> {
        let context = Context::new().context("Não foi possível inicializar libusb")?;
        let devices = context.devices()?;
        let mut found = vec![];
        for device in devices.iter() {
            let descriptor = device.device_descriptor()?;
            if info.usb.ids.contains(&UsbId {
                vendor: descriptor.vendor_id(),
                product: descriptor.product_id(),
            }) {
                found.push(device);
            }
        }
        ensure!(
            found.len() == 1,
            "Esperado um dispositivo do módulo {}; encontrados: {}. Conecte somente uma unidade desse módulo.",
            info.id,
            found.len()
        );
        let device = found.remove(0);
        let config = device.active_config_descriptor()?;
        let compatible = config
            .interfaces()
            .flat_map(|i| i.descriptors())
            .any(|interface| {
                interface.interface_number() == info.usb.interface
                    && interface.setting_number() == 0
                    && info.usb.endpoints.iter().all(|expected| {
                        interface.endpoint_descriptors().any(|endpoint| {
                            endpoint.address() == *expected
                                && endpoint.transfer_type()
                                    == match info.usb.transfer {
                                        Transfer::Bulk => rusb::TransferType::Bulk,
                                        Transfer::Interrupt => rusb::TransferType::Interrupt,
                                    }
                        })
                    })
            });
        ensure!(
            compatible,
            "A interface USB não corresponde ao módulo {}. Nenhum comando foi enviado.",
            info.id
        );
        let handle = device
            .open()
            .context("Não foi possível abrir o cooler. Verifique a permissão udev")?;
        match handle.set_auto_detach_kernel_driver(true) {
            Ok(()) | Err(rusb::Error::NotSupported) => (),
            Err(error) => return Err(error.into()),
        }
        handle
            .claim_interface(info.usb.interface)
            .context("USB ocupado ou sem permissão. Feche outros controladores do dispositivo")?;
        Ok(Self {
            handle,
            spec: info.usb,
        })
    }
}

impl Transport for UsbTransport {
    fn write(&mut self, endpoint: u8, bytes: &[u8]) -> Result<()> {
        ensure!(
            self.spec.endpoints.contains(&endpoint) && endpoint & 0x80 == 0,
            "Endpoint de escrita não declarado pelo módulo."
        );
        let timeout = Duration::from_secs(5);
        let count = match self.spec.transfer {
            Transfer::Bulk => self.handle.write_bulk(endpoint, bytes, timeout)?,
            Transfer::Interrupt => self.handle.write_interrupt(endpoint, bytes, timeout)?,
        };
        ensure!(
            count == bytes.len(),
            "Escrita USB incompleta: {count}/{} bytes.",
            bytes.len()
        );
        Ok(())
    }

    fn read(&mut self, endpoint: u8) -> Result<Vec<u8>> {
        ensure!(
            self.spec.endpoints.contains(&endpoint) && endpoint & 0x80 != 0,
            "Endpoint de leitura não declarado pelo módulo."
        );
        let mut buffer = vec![0; self.spec.read_size];
        let timeout = Duration::from_millis(1500);
        let count = match self.spec.transfer {
            Transfer::Bulk => self.handle.read_bulk(endpoint, &mut buffer, timeout)?,
            Transfer::Interrupt => self.handle.read_interrupt(endpoint, &mut buffer, timeout)?,
        };
        buffer.truncate(count);
        Ok(buffer)
    }
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn write(&mut self, endpoint: u8, bytes: &[u8]) -> Result<()> {
        (**self).write(endpoint, bytes)
    }
    fn read(&mut self, endpoint: u8) -> Result<Vec<u8>> {
        (**self).read(endpoint)
    }
}
