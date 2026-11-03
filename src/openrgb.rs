//! Cliente mínimo do SDK público OpenRGB, protocolo 0 negociado.
//! Reaplica somente o estado corrente da placa-mãe; não carrega perfis globais.
use anyhow::{Context, Result, ensure};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    time::Duration,
};

#[derive(Clone, Default)]
pub struct Sync {
    pub enabled: bool,
    pub live_sdk: bool,
    pub profile: Option<std::path::PathBuf>,
}
pub fn config_dir() -> std::path::PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|v| std::path::PathBuf::from(v).join(".config")))
        .unwrap_or_else(|| std::path::PathBuf::from(".config"))
        .join("OpenRGB")
}
pub fn profiles() -> Vec<std::path::PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(config_dir().join("profiles"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    paths.sort();
    paths
}
pub fn startup_profile() -> Option<std::path::PathBuf> {
    let raw = std::fs::read(config_dir().join("OpenRGB.json")).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let name = value
        .pointer("/ProfileManager/open_profile/name")?
        .as_str()?;
    if !name
        .chars()
        .all(|c| c.is_alphanumeric() || " _-".contains(c))
    {
        return None;
    }
    let path = config_dir().join("profiles").join(format!("{name}.json"));
    path.is_file().then_some(path)
}
#[derive(Debug)]
pub struct ProfileBoard {
    pub name: String,
    pub mode: String,
    pub colors: Vec<u32>,
    pub detector: String,
}
pub fn profile_board(raw: &[u8]) -> Result<ProfileBoard> {
    ensure!(
        raw.len() <= 8 * 1024 * 1024,
        "Perfil OpenRGB grande demais."
    );
    let profile: serde_json::Value = serde_json::from_slice(raw)?;
    let controllers = profile["controllers"]
        .as_array()
        .context("Perfil JSON OpenRGB inválido.")?;
    let boards: Vec<_> = controllers
        .iter()
        .filter(|v| v["type"].as_u64() == Some(0))
        .collect();
    ensure!(
        boards.len() == 1,
        "Escolha um perfil com exatamente uma placa-mãe."
    );
    let board = boards[0];
    let name = board["name"]
        .as_str()
        .context("Placa-mãe sem nome no perfil.")?
        .to_owned();
    ensure!(
        name.len() >= 3 && !name.starts_with('-'),
        "Nome de placa-mãe inválido."
    );
    let mode = board["active_mode"]
        .as_u64()
        .context("Modo ausente no perfil.")? as usize;
    let mode = board["modes"]
        .get(mode)
        .and_then(|m| m["name"].as_str())
        .context("Modo inválido no perfil.")?
        .to_owned();
    ensure!(
        matches!(mode.as_str(), "Direct" | "Static"),
        "A leitura sem servidor aceita perfis de cor fixa (Direct/Static). Use o SDK para efeitos."
    );
    let colors = board["colors"]
        .as_array()
        .context("Perfil sem cores da placa-mãe.")?;
    ensure!(
        !colors.is_empty() && colors.len() <= 4096,
        "Quantidade de LEDs inválida."
    );
    let colors = colors
        .iter()
        .map(|v| {
            let c = v.as_u64().context("Cor inválida no perfil.")?;
            ensure!(c <= 0xffffff, "Cor fora do intervalo RGB.");
            Ok(c as u32)
        })
        .collect::<Result<Vec<_>>>()?;
    let detector = board["description"]
        .as_str()
        .unwrap_or_default()
        .trim_end_matches(" Device")
        .to_owned();
    Ok(ProfileBoard {
        name,
        mode,
        colors,
        detector,
    })
}
/// Converte RGBColor 0x00BBGGRR do OpenRGB para RRGGBB da CLI.
pub fn hex_color(color: u32) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        color & 255,
        (color >> 8) & 255,
        (color >> 16) & 255
    )
}
pub fn apply_profile(path: Option<&std::path::Path>) -> Result<String> {
    let fallback = startup_profile();
    let path = path
        .or(fallback.as_deref())
        .context("selecione um perfil JSON salvo no OpenRGB")?;
    ensure!(
        std::fs::metadata(path)?.len() <= 8 * 1024 * 1024,
        "Perfil grande demais."
    );
    let board = profile_board(&std::fs::read(path)?)?;
    let colors = board
        .colors
        .iter()
        .map(|&c| hex_color(c))
        .collect::<Vec<_>>()
        .join(",");
    // Configuração temporária impede os gatilhos de perfil global ao iniciar/sair.
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let temporary =
        std::env::temp_dir().join(format!("pelicool-openrgb-{}-{stamp}", std::process::id()));
    std::fs::create_dir(&temporary)?;
    let result = (|| -> Result<String> {
        let mut config: serde_json::Value = std::fs::read(config_dir().join("OpenRGB.json"))
            .ok()
            .and_then(|s| serde_json::from_slice(&s).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        config["ProfileManager"] = serde_json::json!({});
        config["AutoStart"] = serde_json::json!({"enabled":false});
        let detectors = config
            .pointer_mut("/Detectors/detectors")
            .and_then(|v| v.as_object_mut())
            .context("Configuração de detectores OpenRGB indisponível.")?;
        ensure!(
            !board.detector.is_empty() && detectors.contains_key(&board.detector),
            "Não foi possível isolar o detector da placa-mãe deste perfil. Use o SDK ou a interface do OpenRGB."
        );
        for (name, enabled) in detectors {
            *enabled = serde_json::Value::Bool(name == &board.detector);
        }
        std::fs::write(temporary.join("OpenRGB.json"), serde_json::to_vec(&config)?)?;
        let configuration = config_dir().join("Configuration.json");
        if configuration.exists() {
            std::fs::copy(configuration, temporary.join("Configuration.json"))?;
        }
        let log = std::fs::File::create(temporary.join("run.log"))?;
        let mut child = host_command("openrgb")
            .arg("--noautoconnect")
            .arg("--config")
            .arg(&temporary)
            .args([
                "--loglevel",
                "0",
                "--list-devices",
                "--device",
                &board.name,
                "--mode",
                &board.mode,
                "--color",
                &colors,
            ])
            .stdout(log.try_clone()?)
            .stderr(log)
            .spawn()
            .context("não foi possível iniciar a CLI do OpenRGB")?;
        let start = std::time::Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                ensure!(
                    status.success(),
                    "OpenRGB não conseguiu reaplicar o perfil (código {:?}).",
                    status.code()
                );
                break;
            }
            if start.elapsed() > Duration::from_secs(15) {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!("OpenRGB não respondeu em 15 segundos.");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let mut output = String::new();
        std::fs::File::open(temporary.join("run.log"))?
            .take(1024 * 1024)
            .read_to_string(&mut output)?;
        ensure!(
            output.contains(&board.name),
            "A placa-mãe do perfil não foi detectada pela CLI do OpenRGB."
        );
        Ok(format!(
            "{} · {}",
            path.file_stem().unwrap_or_default().to_string_lossy(),
            board.name
        ))
    })();
    let _ = std::fs::remove_dir_all(temporary);
    result
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(count)
            .ok_or_else(|| anyhow::anyhow!("Pacote OpenRGB inválido."))?;
        ensure!(end <= self.bytes.len(), "Pacote OpenRGB truncado.");
        let result = &self.bytes[self.at..end];
        self.at = end;
        Ok(result)
    }
    fn u16(&mut self) -> Result<usize> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into()?) as usize)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into()?))
    }
    fn string(&mut self) -> Result<String> {
        let len = self.u16()?;
        ensure!(len > 0, "String OpenRGB inválida.");
        let text = self.take(len)?;
        ensure!(text.last() == Some(&0), "String OpenRGB sem terminador.");
        Ok(String::from_utf8_lossy(&text[..len - 1]).into_owned())
    }
}
pub struct Motherboard {
    pub name: String,
    pub mode: Vec<u8>,
    pub colors: Vec<u8>,
}
/// Analisa dados do protocolo 0; valida todos os comprimentos antes de reenviar.
pub fn parse_motherboard(raw: &[u8]) -> Result<Option<Motherboard>> {
    let mut c = Cursor { bytes: raw, at: 0 };
    ensure!(c.u32()? as usize == raw.len(), "Tamanho OpenRGB inválido.");
    let kind = c.u32()?;
    let name = c.string()?;
    if kind != 0 {
        return Ok(None);
    }
    for _ in 0..4 {
        c.string()?;
    }
    let count = c.u16()?;
    let active = c.u32()? as usize;
    ensure!(count <= 1024 && active < count, "Modo OpenRGB inválido.");
    let mut active_data = None;
    for i in 0..count {
        let start = c.at;
        c.string()?;
        c.take(9 * 4)?;
        let colors = c.u16()?;
        c.take(colors * 4)?;
        if i == active {
            active_data = Some(raw[start..c.at].to_vec());
        }
    }
    let zones = c.u16()?;
    ensure!(zones <= 4096, "Zonas OpenRGB inválidas.");
    for _ in 0..zones {
        c.string()?;
        c.take(4 * 4)?;
        let matrix = c.u16()?;
        c.take(matrix)?;
    }
    let leds = c.u16()?;
    for _ in 0..leds {
        c.string()?;
        c.take(4)?;
    }
    let color_start = c.at;
    let colors = c.u16()?;
    c.take(colors * 4)?;
    ensure!(c.at == raw.len(), "Formato OpenRGB inesperado.");
    let colors = [
        ((raw.len() - color_start + 4) as u32)
            .to_le_bytes()
            .as_slice(),
        &raw[color_start..],
    ]
    .concat();
    let mode_data = active_data.ok_or_else(|| anyhow::anyhow!("Modo OpenRGB ausente."))?;
    let mode = [
        ((mode_data.len() + 8) as u32).to_le_bytes().as_slice(),
        &(active as u32).to_le_bytes(),
        &mode_data,
    ]
    .concat();
    Ok(Some(Motherboard { name, mode, colors }))
}
fn send(stream: &mut TcpStream, device: u32, command: u32, data: &[u8]) -> Result<()> {
    stream.write_all(b"ORGB")?;
    for field in [device, command, data.len() as u32] {
        stream.write_all(&field.to_le_bytes())?;
    }
    stream.write_all(data)?;
    Ok(())
}
fn receive(stream: &mut TcpStream, device: u32, command: u32) -> Result<Vec<u8>> {
    let mut header = [0; 16];
    stream.read_exact(&mut header)?;
    ensure!(&header[..4] == b"ORGB", "Resposta OpenRGB inválida.");
    ensure!(
        u32::from_le_bytes(header[4..8].try_into()?) == device
            && u32::from_le_bytes(header[8..12].try_into()?) == command,
        "Lista OpenRGB mudou. Tente aplicar novamente."
    );
    let len = u32::from_le_bytes(header[12..].try_into()?) as usize;
    ensure!(len <= 8 * 1024 * 1024, "Resposta OpenRGB grande demais.");
    let mut data = vec![0; len];
    stream.read_exact(&mut data)?;
    Ok(data)
}
pub fn refresh_motherboard() -> Result<String> {
    let address: SocketAddr = "127.0.0.1:6742".parse()?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_millis(350))
        .context("ative o SDK Server do OpenRGB em 127.0.0.1:6742")?;
    stream.set_read_timeout(Some(Duration::from_millis(700)))?;
    stream.set_write_timeout(Some(Duration::from_millis(700)))?;
    send(&mut stream, 0, 40, &0u32.to_le_bytes())?;
    receive(&mut stream, 0, 40)?;
    send(&mut stream, 0, 50, b"PeliCool\0")?;
    send(&mut stream, 0, 0, &[])?;
    let raw = receive(&mut stream, 0, 0)?;
    ensure!(raw.len() == 4, "Servidor não negociou protocolo 0.");
    let count = u32::from_le_bytes(
        raw.try_into()
            .map_err(|_| anyhow::anyhow!("Contagem OpenRGB inválida."))?,
    );
    ensure!(count <= 128, "Muitos dispositivos OpenRGB.");
    let mut boards = Vec::new();
    for device in 0..count {
        send(&mut stream, device, 1, &[])?;
        let raw = receive(&mut stream, device, 1)?;
        if let Some(board) = parse_motherboard(&raw)? {
            boards.push((device, board));
        }
    }
    // Evita escolher silenciosamente entre vários controladores de placa-mãe.
    ensure!(
        boards.len() == 1,
        "esperado um controlador de placa-mãe no OpenRGB; encontrados {}",
        boards.len()
    );
    let (device, board) = boards.pop().expect("one board");
    send(&mut stream, device, 1101, &board.mode)?;
    send(&mut stream, device, 1050, &board.colors)?;
    stream.flush()?;
    // Barreira de leitura: o servidor processou as escritas, não confirma o LED físico.
    send(&mut stream, 0, 0, &[])?;
    receive(&mut stream, 0, 0)?;
    Ok(board.name)
}

// Bibliotecas do AppImage não devem contaminar programas instalados no sistema.
fn host_command(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    if let Some(original) = std::env::var_os("PELICOOL_HOST_LIBRARY_PATH") {
        if original.is_empty() {
            command.env_remove("LD_LIBRARY_PATH");
        } else {
            command.env("LD_LIBRARY_PATH", original);
        }
    }
    command
}
