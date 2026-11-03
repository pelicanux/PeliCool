use pelicool::drivers::mystique::protocol;
use pelicool::sensors::{self, Reader, Selection, Snapshot};
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "deepcool-sensors-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn put(&self, path: &str, content: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn cpu_load_excludes_guest_counters_and_uses_deltas() {
    assert_eq!(
        sensors::parse_cpu("cpu 100 20 30 400 50 10 20 5 100 0\n"),
        Some((450, 635))
    );
    assert_eq!(sensors::cpu_delta((100, 500), (160, 700)), Some(70.));
    assert_eq!(sensors::cpu_delta((100, 500), (90, 700)), None);
    assert_eq!(sensors::cpu_delta((100, 500), (100, 500)), None);
    assert_eq!(sensors::parse_cpu("cpu0 1 2 3 4"), None);
}

#[test]
fn ram_uses_available_memory_and_rejects_invalid_totals() {
    assert_eq!(
        sensors::parse_ram("MemTotal: 1000 kB\nMemFree: 100 kB\nMemAvailable: 250 kB"),
        Some(75.)
    );
    assert_eq!(
        sensors::parse_ram("MemTotal: 0 kB\nMemAvailable: 0 kB"),
        None
    );
    assert_eq!(
        sensors::parse_ram("MemTotal: 1000 kB\nMemFree: 100 kB"),
        None
    );
}

#[test]
fn reader_picks_cpu_sensor_and_never_guesses_gpu_fan_as_cpu_or_pump() {
    let fixture = Fixture::new();
    for (file, value) in [
        ("hwmon/hwmon0/name", "nvme"),
        ("hwmon/hwmon0/temp1_input", "80000"),
        ("hwmon/hwmon1/name", "amdgpu"),
        ("hwmon/hwmon1/temp1_input", "90000"),
        ("hwmon/hwmon1/fan1_input", "1234"),
        ("hwmon/hwmon2/name", "k10temp"),
        ("hwmon/hwmon2/temp1_label", "Tctl"),
        ("hwmon/hwmon2/temp1_input", "54321"),
        ("proc/stat", "cpu 100 0 0 100 0 0 0 0"),
        ("proc/meminfo", "MemTotal: 1000 kB\nMemAvailable: 400 kB"),
        ("cpu/cpu0/cpufreq/scaling_cur_freq", "4321000"),
    ] {
        fixture.put(file, value);
    }
    let mut reader = Reader::new(
        &fixture.0.join("hwmon"),
        &fixture.0.join("proc"),
        &fixture.0.join("cpu"),
    );
    fixture.put("proc/stat", "cpu 150 0 0 150 0 0 0 0");
    let snapshot = reader.sample();
    assert_eq!(snapshot.cpu_temp, Some(54.3));
    assert_eq!(snapshot.cpu_usage, Some(50.));
    assert_eq!(snapshot.ram_usage, Some(60.));
    assert_eq!(snapshot.cpu_mhz, Some(4321.));
    assert_eq!(snapshot.fan_rpm, None);
    assert_eq!(snapshot.pump_rpm, None);
    assert_eq!(snapshot.v12, None);
    reader.select(Selection {
        cpu_temp: Some("../../not-a-sensor".into()),
        ..Default::default()
    });
    assert_eq!(reader.sample().cpu_temp, None);
}

#[test]
fn wire_sensor_packet_matches_reference_decimal_encoding() {
    let snapshot = Snapshot {
        cpu_temp: Some(57.5),
        cpu_usage: Some(12.2),
        ram_usage: Some(63.8),
        v33: Some(3.30),
        v5: Some(5.05),
        v12: Some(12.10),
        cpu_mhz: Some(5270.),
        fan_rpm: Some(1234.),
        pump_rpm: Some(3400.),
        ..Default::default()
    };
    assert_eq!(
        protocol::sensor_payload(&snapshot),
        [
            57, 0, 5, 12, 0, 0, 64, 0, 0, 3, 0, 30, 5, 0, 5, 12, 0, 10, 5, 0, 27, 0xd2, 4, 0, 0x48,
            0x0d, 0,
        ]
    );
    assert_eq!(protocol::decimal3(Some(5.10), 2, false), [5, 0, 10]);
    assert_eq!(protocol::decimal3(Some(f64::NAN), 2, false), [0, 0, 0]);
}

#[test]
fn io_counters_skip_loopback_and_disk_partitions() {
    let network =
        "eth0: 100 1 0 0 0 0 0 0 200 1 0 0 0 0 0 0\nlo: 900 1 0 0 0 0 0 0 900 1 0 0 0 0 0 0";
    let disks = "259 0 nvme0n1 1 0 10 0 1 0 20 0\n259 1 nvme0n1p1 1 0 100 0 1 0 100 0\n8 0 sda 1 0 2 0 1 0 3 0";
    assert_eq!(
        sensors::io_counters(network, disks),
        Some([100, 200, 12 * 512, 23 * 512])
    );
    assert_eq!(sensors::io_counters("", ""), None);
}
