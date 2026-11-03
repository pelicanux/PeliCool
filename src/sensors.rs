//! Leitura de hwmon e /proc. Nunca escreve sensores nem controla PWM.
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Temperature,
    Fan,
    Voltage,
}
#[derive(Clone)]
pub struct Source {
    pub id: String,
    pub label: String,
    pub kind: Kind,
    pub path: PathBuf,
    pub chip: String,
    pub channel_label: String,
}
#[derive(Clone, Default, PartialEq)]
pub struct Selection {
    pub cpu_temp: Option<String>,
    pub fan: Option<String>,
    pub pump: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub cpu_name: Option<String>,
    pub gpu_name: Option<String>,
    pub gpu_temp: Option<f64>,
    pub gpu_usage: Option<f64>,
    pub gpu_mhz: Option<f64>,
    pub gpu_vram_used: Option<f64>,
    pub gpu_vram_total: Option<f64>,
    pub ram_total: Option<f64>,
    pub ram_used: Option<f64>,
    pub network_rx: Option<f64>,
    pub network_tx: Option<f64>,
    pub disk_read: Option<f64>,
    pub disk_write: Option<f64>,
    pub available: Vec<Metric>,
    pub cpu_temp: Option<f64>,
    pub cpu_usage: Option<f64>,
    pub ram_usage: Option<f64>,
    pub cpu_mhz: Option<f64>,
    pub fan_rpm: Option<f64>,
    pub pump_rpm: Option<f64>,
    pub v33: Option<f64>,
    pub v5: Option<f64>,
    pub v12: Option<f64>,
    pub temp_source: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Metric {
    pub label: String,
    pub kind: Kind,
    pub value: f64,
}

pub fn io_counters(network: &str, disks: &str) -> Option<[u64; 4]> {
    let mut values = [0u64; 4];
    let mut found = false;
    for line in network.lines() {
        if let Some((name, raw)) = line.split_once(':')
            && name.trim() != "lo"
        {
            let fields: Vec<_> = raw.split_whitespace().collect();
            if let (Some(rx), Some(tx)) = (
                fields.first().and_then(|s| s.parse::<u64>().ok()),
                fields.get(8).and_then(|s| s.parse::<u64>().ok()),
            ) {
                values[0] = values[0].checked_add(rx)?;
                values[1] = values[1].checked_add(tx)?;
                found = true;
            }
        }
    }
    for line in disks.lines() {
        let f: Vec<_> = line.split_whitespace().collect();
        let Some(name) = f.get(2) else { continue };
        let physical = (name.starts_with("nvme") && !name.contains('p'))
            || ["sd", "vd", "xvd"].iter().any(|prefix| {
                name.starts_with(prefix) && name.chars().all(|c| c.is_ascii_alphabetic())
            });
        if !physical {
            continue;
        }
        if let (Some(read), Some(write)) = (
            f.get(5).and_then(|s| s.parse::<u64>().ok()),
            f.get(9).and_then(|s| s.parse::<u64>().ok()),
        ) {
            values[2] = values[2].checked_add(read.checked_mul(512)?)?;
            values[3] = values[3].checked_add(write.checked_mul(512)?)?;
            found = true;
        }
    }
    found.then_some(values)
}

pub fn catalog(root: &Path) -> Vec<Source> {
    let mut chips: Vec<_> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .collect();
    chips.sort();
    let mut sources = vec![];
    for directory in chips {
        let Some(chip) = read(directory.join("name")) else {
            continue;
        };
        let device = directory
            .join("device")
            .canonicalize()
            .ok()
            .and_then(|path| path.file_name().map(|v| v.to_string_lossy().into_owned()))
            .unwrap_or_else(|| {
                directory
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            });
        let mut inputs: Vec<_> = fs::read_dir(&directory)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        inputs.sort();
        for path in inputs {
            let filename = path.file_name().unwrap_or_default().to_string_lossy();
            let Some(channel) = filename.strip_suffix("_input") else {
                continue;
            };
            let kind = if numbered(channel, "temp") {
                Kind::Temperature
            } else if numbered(channel, "fan") {
                Kind::Fan
            } else if numbered(channel, "in") {
                Kind::Voltage
            } else {
                continue;
            };
            let label = read(directory.join(format!("{channel}_label")))
                .unwrap_or_else(|| channel.to_owned());
            sources.push(Source {
                id: format!("{chip}@{device}/{channel}"),
                label: format!("{chip} · {label}"),
                kind,
                path: path.clone(),
                chip: chip.clone(),
                channel_label: label,
            });
        }
    }
    sources
}
fn numbered(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
}
fn read(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_owned())
}
fn number(path: impl AsRef<Path>) -> Option<f64> {
    read(path)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

pub fn default_temperature(sources: &[Source]) -> Option<&Source> {
    for (chip, labels) in [
        ("k10temp", &["Tdie", "Tctl"][..]),
        ("zenpower", &["Tdie", "Tctl"][..]),
        ("coretemp", &["Package id 0"][..]),
        ("cpu_thermal", &[][..]),
    ] {
        for label in labels {
            if let Some(source) = sources.iter().find(|source| {
                source.chip == chip
                    && source.kind == Kind::Temperature
                    && source.channel_label == *label
            }) {
                return Some(source);
            }
        }
        if let Some(source) = sources
            .iter()
            .find(|source| source.chip == chip && source.kind == Kind::Temperature)
        {
            return Some(source);
        }
    }
    None
}

pub fn parse_cpu(raw: &str) -> Option<(u64, u64)> {
    let mut fields = raw.lines().next()?.split_whitespace();
    if fields.next() != Some("cpu") {
        return None;
    }
    let fields: Vec<u64> = fields
        .take(8)
        .map(str::parse)
        .collect::<std::result::Result<_, _>>()
        .ok()?;
    if fields.len() < 4 {
        return None;
    }
    Some((
        fields[3].checked_add(*fields.get(4).unwrap_or(&0))?,
        fields
            .iter()
            .try_fold(0u64, |sum, value| sum.checked_add(*value))?,
    ))
}
pub fn cpu_delta(previous: (u64, u64), current: (u64, u64)) -> Option<f64> {
    let total = current.1.checked_sub(previous.1)?;
    let idle = current.0.checked_sub(previous.0)?;
    if total == 0 || idle > total {
        return None;
    }
    Some(100. * (1. - idle as f64 / total as f64))
}
pub fn parse_ram(raw: &str) -> Option<f64> {
    let find = |key: &str| {
        raw.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            (name == key)
                .then(|| value.split_whitespace().next()?.parse::<f64>().ok())
                .flatten()
        })
    };
    let total = find("MemTotal")?;
    let available = find("MemAvailable")?;
    if total <= 0. || available < 0. || available > total {
        return None;
    }
    Some((total - available) / total * 100.)
}

pub struct Reader {
    pub sources: Vec<Source>,
    selection: Selection,
    proc_root: PathBuf,
    cpu_root: PathBuf,
    previous: Option<(u64, u64)>,
    io_previous: Option<(std::time::Instant, [u64; 4])>,
}
impl Default for Reader {
    fn default() -> Self {
        Self::new(
            Path::new("/sys/class/hwmon"),
            Path::new("/proc"),
            Path::new("/sys/devices/system/cpu"),
        )
    }
}
impl Reader {
    pub fn new(hwmon: &Path, proc_root: &Path, cpu_root: &Path) -> Self {
        Self {
            sources: catalog(hwmon),
            selection: Selection::default(),
            io_previous: None,
            proc_root: proc_root.into(),
            cpu_root: cpu_root.into(),
            previous: read(proc_root.join("stat")).and_then(|raw| parse_cpu(&raw)),
        }
    }
    pub fn select(&mut self, selection: Selection) {
        self.selection = selection;
    }
    fn selected(&self, id: &Option<String>, kind: Kind, scale: f64) -> Option<f64> {
        let source = self
            .sources
            .iter()
            .find(|source| Some(&source.id) == id.as_ref() && source.kind == kind)?;
        number(&source.path).map(|value| value / scale)
    }
    fn rail(&self, labels: &[&str]) -> Option<f64> {
        let source = self.sources.iter().find(|source| {
            source.kind == Kind::Voltage
                && labels
                    .iter()
                    .any(|label| source.channel_label.eq_ignore_ascii_case(label))
        })?;
        number(&source.path).map(|v| v / 1000.)
    }
    fn clock(&self) -> Option<f64> {
        let mut frequencies = vec![];
        if let Ok(entries) = fs::read_dir(&self.cpu_root) {
            for entry in entries.flatten() {
                if numbered(&entry.file_name().to_string_lossy(), "cpu")
                    && let Some(value) = number(entry.path().join("cpufreq/scaling_cur_freq"))
                {
                    frequencies.push(value / 1000.);
                }
            }
        }
        if frequencies.is_empty()
            && let Some(raw) = read(self.proc_root.join("cpuinfo"))
        {
            for line in raw.lines() {
                if let Some((key, value)) = line.split_once(':')
                    && key.trim() == "cpu MHz"
                    && let Ok(value) = value.trim().parse::<f64>()
                {
                    frequencies.push(value);
                }
            }
        }
        frequencies
            .into_iter()
            .filter(|value| value.is_finite() && *value > 0.)
            .max_by(f64::total_cmp)
    }
    pub fn sample(&mut self) -> Snapshot {
        let current = read(self.proc_root.join("stat")).and_then(|raw| parse_cpu(&raw));
        let cpu_usage = self
            .previous
            .zip(current)
            .and_then(|(previous, current)| cpu_delta(previous, current));
        self.previous = current;
        let temp = self
            .selection
            .cpu_temp
            .as_ref()
            .and_then(|id| {
                self.sources
                    .iter()
                    .find(|source| &source.id == id && source.kind == Kind::Temperature)
            })
            .or_else(|| {
                if self.selection.cpu_temp.is_none() {
                    default_temperature(&self.sources)
                } else {
                    None
                }
            });
        let available = self
            .sources
            .iter()
            .filter_map(|source| {
                number(&source.path).map(|v| Metric {
                    label: source.label.clone(),
                    kind: source.kind,
                    value: if source.kind == Kind::Fan {
                        v
                    } else {
                        v / 1000.
                    },
                })
            })
            .collect();
        let gpu_source = self
            .sources
            .iter()
            .find(|s| s.chip == "amdgpu" && s.kind == Kind::Temperature);
        let gpu_device = gpu_source
            .and_then(|source| source.path.parent())
            .map(|p| p.join("device"));
        let gpu_value = |file: &str| gpu_device.as_ref().and_then(|p| number(p.join(file)));
        let gpu_mhz = gpu_device
            .as_ref()
            .and_then(|p| read(p.join("pp_dpm_sclk")))
            .and_then(|raw| {
                raw.lines().find(|s| s.contains('*')).and_then(|line| {
                    line.split_whitespace().find_map(|v| {
                        v.strip_suffix("Mhz")
                            .or_else(|| v.strip_suffix("MHz"))
                            .and_then(|v| v.parse::<f64>().ok())
                    })
                })
            });
        let mem = read(self.proc_root.join("meminfo")).unwrap_or_default();
        let ram_total = mem
            .lines()
            .find_map(|line| {
                line.strip_prefix("MemTotal:")
                    .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
            })
            .map(|v| v * 1024.);
        let ram_usage = parse_ram(&mem);
        let mut rates = [None; 4];
        let now = std::time::Instant::now();
        let io = io_counters(
            &read(self.proc_root.join("net/dev")).unwrap_or_default(),
            &read(self.proc_root.join("diskstats")).unwrap_or_default(),
        );
        if let Some(current) = io {
            if let Some((time, previous)) = self.io_previous {
                let seconds = now.duration_since(time).as_secs_f64();
                if seconds > 0. {
                    for i in 0..4 {
                        rates[i] = current[i]
                            .checked_sub(previous[i])
                            .map(|delta| delta as f64 / seconds);
                    }
                }
            }
            self.io_previous = Some((now, current));
        } else {
            self.io_previous = None;
        }
        Snapshot {
            cpu_name: read(self.proc_root.join("cpuinfo")).and_then(|raw| {
                raw.lines().find_map(|line| {
                    line.split_once(':')
                        .filter(|(key, _)| key.trim() == "model name")
                        .map(|(_, value)| value.trim().to_owned())
                })
            }),
            gpu_name: gpu_source.map(|_| "AMD GPU · amdgpu".into()),
            gpu_temp: gpu_source.and_then(|s| number(&s.path)).map(|v| v / 1000.),
            gpu_usage: gpu_value("gpu_busy_percent"),
            gpu_mhz,
            gpu_vram_used: gpu_value("mem_info_vram_used"),
            gpu_vram_total: gpu_value("mem_info_vram_total"),
            ram_total,
            ram_used: ram_total
                .zip(ram_usage)
                .map(|(total, usage)| total * usage / 100.),
            network_rx: rates[0],
            network_tx: rates[1],
            disk_read: rates[2],
            disk_write: rates[3],
            available,
            cpu_temp: temp
                .and_then(|source| number(&source.path))
                .map(|v| (v / 100.).round() / 10.)
                .filter(|v| *v >= -50. && *v <= 200.),
            temp_source: temp.map(|source| source.label.clone()),
            cpu_usage,
            ram_usage,
            cpu_mhz: self.clock(),
            fan_rpm: self.selected(&self.selection.fan, Kind::Fan, 1.),
            pump_rpm: self.selected(&self.selection.pump, Kind::Fan, 1.),
            v33: self.rail(&["+3.3V", "3.3V", "3VCC", "VCC3"]),
            v5: self.rail(&["+5V", "5V", "5VCC", "VCC5"]),
            v12: self.rail(&["+12V", "12V", "12VCC", "VCC12"]),
        }
    }
}
