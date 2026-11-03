//! Protocolo MYSTIQUE adaptado do Mystique Control (MIT).
//! Ver THIRD-PARTY-NOTICES.md e LICENSES/gpteixeira-MIT.txt.

use anyhow::{Result, ensure};

pub const EP_CMD: (u8, u8) = (0x01, 0x81);
pub const EP_DATA: (u8, u8) = (0x02, 0x82);
pub const FINISH: &[u8; 55] = &{
    let mut bytes = [0u8; 55];
    let magic = b"dcldfinish";
    let mut index = 0;
    while index < magic.len() {
        bytes[index] = magic[index];
        index += 1;
    }
    bytes
};

#[derive(Clone, Copy, Debug)]
#[repr(u8)]
pub enum Command {
    Sensors = 0x01,
    Options = 0x02,
    Mode = 0x03,
    Situational = 0x04,
    Recorder = 0x05,
    RecorderRefresh = 0x06,
    Motion = 0x07,
    MotionScreen = 0x08,
    Time = 0x0a,
    TemperatureUnit = 0x0b,
    StartDownload = 0x0f,
    ScreenState = 0x10,
    FormatResources = 0x14,
}

/// Formato decimal observado na implementação de referência: inteiro LE
/// seguido dos dígitos fracionários, sem escala implícita no campo inteiro.
pub fn decimal3(value: Option<f64>, digits: usize, trim_zeroes: bool) -> [u8; 3] {
    let value = value
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
        .clamp(0.0, 65535.0);
    let mut text = format!("{value:.digits$}");
    if trim_zeroes && text.contains('.') {
        text = text.trim_end_matches('0').trim_end_matches('.').to_owned();
    }
    let (whole, frac) = text.split_once('.').unwrap_or((&text, "0"));
    let whole = whole.parse::<u16>().unwrap_or(0);
    let fraction = frac.parse::<u8>().unwrap_or(0);
    [whole as u8, (whole >> 8) as u8, fraction]
}

pub fn sensor_payload(values: &crate::sensors::Snapshot) -> [u8; 27] {
    let fields = [
        decimal3(values.cpu_temp, 1, true),
        decimal3(values.cpu_usage.map(f64::round), 0, true),
        decimal3(values.ram_usage.map(f64::round), 0, true),
        decimal3(values.v33, 2, false),
        decimal3(values.v5, 2, false),
        decimal3(values.v12, 2, false),
        decimal3(values.cpu_mhz.map(|v| v / 1000.0), 2, false),
        decimal3(values.fan_rpm, 0, true),
        decimal3(values.pump_rpm, 0, true),
    ];
    let mut payload = [0; 27];
    for (chunk, field) in payload.as_chunks_mut::<3>().0.iter_mut().zip(fields) {
        *chunk = field;
    }
    payload
}

pub fn checksum(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .fold(0u16, |sum, &byte| sum.wrapping_add(u16::from(byte)))
}

pub fn packet(command: Command, payload: &[u8]) -> Result<[u8; 48]> {
    ensure!(payload.len() <= 39, "Payload maior que 39 bytes.");
    let mut out = [0u8; 48];
    out[..3].copy_from_slice(&[0xaa, 0x2e, command as u8]);
    out[3..3 + payload.len()].copy_from_slice(payload);
    out[42..46].copy_from_slice(b"HIDC");
    let sum = checksum(&out[..46]);
    out[46..48].copy_from_slice(&sum.to_le_bytes());
    Ok(out)
}

pub fn validate_reply(raw: &[u8], command: Command) -> Result<()> {
    ensure!(
        raw.len() == 48,
        "Resposta USB com tamanho inesperado: {}.",
        raw.len()
    );
    ensure!(
        raw[..2] == [0x55, 0x2e] && &raw[42..46] == b"HIDC",
        "Cabeçalho/rodapé USB inválido."
    );
    ensure!(
        checksum(&raw[..46]) == u16::from_le_bytes([raw[46], raw[47]]),
        "Checksum USB inválido."
    );
    ensure!(
        raw[2] == command as u8,
        "Comando da resposta não corresponde ao pedido: {:02x}.",
        raw[2]
    );
    ensure!(
        raw[3] == 0,
        "Cooler recusou o comando {:02x}: status {}.",
        command as u8,
        raw[3]
    );
    Ok(())
}

pub fn file_header(
    kind: u8,
    jpeg: &[u8],
    id: &[u8; 32],
    count: u16,
    duration: u16,
    index: u16,
) -> Result<[u8; 64]> {
    ensure!(kind == 1 || kind == 2, "Tipo de arquivo inválido.");
    ensure!(
        id.iter().all(u8::is_ascii_hexdigit),
        "ID de mídia inválido."
    );
    ensure!(
        kind != 1 || (count == 0 && duration == 0 && index == 0),
        "Metadados de imagem inválidos."
    );
    ensure!(
        kind != 2 || (count > 0 && index < count && duration > 0),
        "Metadados de GIF inválidos."
    );
    ensure!(jpeg.len() <= u32::MAX as usize, "JPEG grande demais.");
    let mut header = [0u8; 64];
    header[..4].copy_from_slice(b"DCLd");
    header[4] = kind;
    header[5..9].copy_from_slice(&(jpeg.len() as u32).to_le_bytes());
    header[9..11].copy_from_slice(&checksum(jpeg).to_le_bytes());
    header[13..15].copy_from_slice(&count.to_le_bytes());
    header[15..17].copy_from_slice(&duration.to_le_bytes());
    header[17..19].copy_from_slice(&index.to_le_bytes());
    header[20..52].copy_from_slice(id);
    let sum = checksum(&header[..62]);
    header[62..64].copy_from_slice(&sum.to_le_bytes());
    Ok(header)
}
