use anyhow::Result;
use pelicool::{
    collection::{Cache, Slot},
    device::{DisplayMode, Preset, Settings},
    drivers::{self, Capabilities, DisplayDriver, DriverInfo, DriverModule},
    sensors::Snapshot,
    transport::{Transfer, Transport, UsbId, UsbSpec},
};
use std::sync::{Arc, Mutex};

#[allow(dead_code)]
#[path = "common/driver_template.rs"]
mod driver_template;

type Writes = Arc<Mutex<Vec<(u8, Vec<u8>)>>>;

struct RecordingTransport(Writes);
impl Transport for RecordingTransport {
    fn write(&mut self, endpoint: u8, bytes: &[u8]) -> Result<()> {
        self.0.lock().unwrap().push((endpoint, bytes.to_vec()));
        Ok(())
    }
    fn read(&mut self, _endpoint: u8) -> Result<Vec<u8>> {
        Ok(vec![0x99])
    }
}

// Driver de teste deliberadamente diferente do protocolo MYSTIQUE.
struct TestDriver(Box<dyn Transport + Send>);
impl DisplayDriver for TestDriver {
    fn probe(&mut self) -> Result<String> {
        self.0.write(0x04, &[0x42])?;
        assert_eq!(self.0.read(0x84)?, [0x99]);
        Ok("test-driver".into())
    }
    fn configure_preset(
        &mut self,
        _settings: Settings,
        _preset: Preset,
        _reset: bool,
    ) -> Result<()> {
        self.0.write(0x04, &[0x11])
    }
    fn update_sensors(&mut self, snapshot: &Snapshot) -> Result<()> {
        self.0
            .write(0x04, &[0x12, snapshot.cpu_temp.unwrap_or(0.) as u8])
    }
}

static TEST_INFO: DriverInfo = DriverInfo {
    id: "test-cooler",
    name: "Test cooler",
    tested_models: &[],
    usb: UsbSpec {
        ids: &[UsbId {
            vendor: 0xff00,
            product: 0xff01,
        }],
        interface: 1,
        transfer: Transfer::Interrupt,
        endpoints: &[0x04, 0x84],
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
static TEST_MODULE: DriverModule = DriverModule {
    info: &TEST_INFO,
    create: |transport| Box::new(TestDriver(transport)),
    prepare_media: drivers::unavailable_media,
};

#[test]
fn registry_contains_mystique_and_its_matcher_requires_exact_usb_ids() {
    drivers::validate_registry(drivers::REGISTERED).unwrap();
    let module = drivers::by_id("mystique").unwrap();
    assert!(module.info.tested_models.contains(&"MYSTIQUE 360"));
    assert!(
        drivers::matching(
            &[drivers::mystique::MODULE],
            UsbId {
                vendor: 0x3633,
                product: 0x9999
            }
        )
        .unwrap()
        .is_none()
    );
    assert!(
        drivers::matching(
            &[drivers::mystique::MODULE],
            UsbId {
                vendor: 0x1234,
                product: 0x0009
            }
        )
        .unwrap()
        .is_none()
    );
    assert!(drivers::by_id("not-a-driver").is_err());
}

#[test]
fn template_compiles_but_is_not_a_registered_functional_driver() {
    assert!(drivers::by_id(driver_template::INFO.id).is_err());
    assert!(drivers::validate_registry(&[driver_template::MODULE]).is_err());
    let writes = Writes::default();
    let mut device = driver_template::MODULE.connect(Box::new(RecordingTransport(writes.clone())));
    assert!(device.probe().is_err());
    assert!(writes.lock().unwrap().is_empty());
}

#[test]
fn every_registered_usb_id_has_an_exact_local_session_udev_rule() {
    let rules = include_str!("../udev/70-pelicool.rules");
    for module in drivers::REGISTERED {
        for id in module.info.usb.ids {
            assert!(
                rules.lines().any(|line| {
                    !line.trim_start().starts_with('#')
                        && line.contains(&format!("ATTR{{idVendor}}==\"{:04x}\"", id.vendor))
                        && line.contains(&format!("ATTR{{idProduct}}==\"{:04x}\"", id.product))
                        && line.contains("TAG+=\"uaccess\"")
                }),
                "Regra udev específica ausente para {}",
                module.info.id
            );
        }
    }
}

#[test]
fn another_protocol_uses_the_same_device_api_without_mystique_packets() {
    drivers::validate_registry(&[TEST_MODULE]).unwrap();
    let writes = Writes::default();
    let mut device = TEST_MODULE.connect(Box::new(RecordingTransport(writes.clone())));
    assert_eq!(device.info().id, "test-cooler");
    assert_eq!(device.probe().unwrap(), "test-driver");
    device
        .configure_preset(Settings::default(), Preset::default(), false)
        .unwrap();
    device
        .update_sensors(&Snapshot {
            cpu_temp: Some(42.),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        *writes.lock().unwrap(),
        [
            (0x04, vec![0x42]),
            (0x04, vec![0x11]),
            (0x04, vec![0x12, 42])
        ]
    );
}

#[test]
fn unsupported_features_fail_before_any_transport_write() {
    let writes = Writes::default();
    let mut device = TEST_MODULE.connect(Box::new(RecordingTransport(writes.clone())));
    assert!(device.apply(Settings::default()).is_err());
    assert!(
        device
            .select_media(Slot {
                animated: false,
                index: 0
            })
            .is_err()
    );
    assert!(device.reset_graph().is_err());
    assert!(
        device
            .configure_preset(
                Settings {
                    brightness: 10,
                    ..Default::default()
                },
                Preset::default(),
                false
            )
            .is_err()
    );
    assert!(
        device
            .configure_preset(
                Settings {
                    gyro: true,
                    ..Default::default()
                },
                Preset::default(),
                false
            )
            .is_err()
    );
    assert!(
        device
            .configure_preset(
                Settings {
                    rotation: 90,
                    ..Default::default()
                },
                Preset::default(),
                false
            )
            .is_err()
    );
    assert!(
        device
            .upload_collection(&[], Settings::default(), |_| {})
            .is_err()
    );
    assert!(
        TEST_MODULE
            .prepare(
                std::path::Path::new("missing.png"),
                Default::default(),
                Default::default()
            )
            .is_err()
    );
    device.sync_clock().unwrap(); // O agendador ignora relógio não anunciado.
    assert!(writes.lock().unwrap().is_empty());
}

#[test]
fn module_specific_slot_limits_are_checked_before_mystique_commands() {
    let base = *drivers::by_id("mystique").unwrap();
    let mut info = *base.info;
    info.id = "test-limited-mystique";
    info.capabilities.media.as_mut().unwrap().image_slots = 1;
    info.capabilities.media.as_mut().unwrap().gif_slots = 0;
    let module = Box::leak(Box::new(DriverModule {
        info: Box::leak(Box::new(info)),
        ..base
    }));
    let writes = Writes::default();
    let mut device = module.connect(Box::new(RecordingTransport(writes.clone())));
    let media = pelicool::media::PreparedMedia {
        display_rotation: None,
        adaptation: None,
        frames: vec![pelicool::media::EncodedFrame {
            jpeg: vec![0xff, 0xd8, 0xff, 0xd9],
            preview: image::RgbaImage::new(1, 1),
            delay_ms: 100,
        }],
        animated: false,
        horizontal: true,
        duration_ms: 0,
    };
    device
        .validate_collection(&[&media], Settings::default())
        .unwrap();
    assert!(
        device
            .validate_collection(&[&media, &media], Settings::default())
            .is_err()
    );
    assert!(
        device
            .upload_collection(&[&media, &media], Settings::default(), |_| {})
            .is_err()
    );
    assert!(
        device
            .select_media(Slot {
                animated: false,
                index: 1
            })
            .is_err()
    );
    assert!(
        device
            .select_media(Slot {
                animated: true,
                index: 0
            })
            .is_err()
    );
    assert!(writes.lock().unwrap().is_empty());
}

#[test]
fn mystique_factory_preserves_options_and_raw_saved_rotation() {
    struct EchoTransport(Writes);
    impl Transport for EchoTransport {
        fn write(&mut self, endpoint: u8, bytes: &[u8]) -> Result<()> {
            self.0.lock().unwrap().push((endpoint, bytes.to_vec()));
            Ok(())
        }
        fn read(&mut self, _endpoint: u8) -> Result<Vec<u8>> {
            let mut reply = self.0.lock().unwrap().last().unwrap().1.clone();
            reply[0] = 0x55;
            reply[3..42].fill(0);
            let sum = pelicool::drivers::mystique::protocol::checksum(&reply[..46]);
            reply[46..48].copy_from_slice(&sum.to_le_bytes());
            Ok(reply)
        }
    }
    let writes = Writes::default();
    let mut device = drivers::by_id("mystique")
        .unwrap()
        .connect(Box::new(EchoTransport(writes.clone())));
    assert!(device.probe().unwrap().contains("MYSTIQUE"));
    device
        .apply(Settings {
            rotation: 270,
            ..Default::default()
        })
        .unwrap();
    let writes = writes.lock().unwrap();
    assert_eq!((writes[0].0, writes[0].1[2]), (0x02, 0x10));
    assert_eq!(
        (writes[3].0, writes[3].1[2], writes[3].1[5]),
        (0x01, 0x02, 3)
    );
    assert_eq!(
        writes.iter().map(|(_, p)| p[2]).collect::<Vec<_>>(),
        [0x10, 0x03, 0x07, 0x02]
    );
}

#[test]
fn ambiguous_module_registration_is_rejected() {
    let original = *drivers::by_id("mystique").unwrap();
    assert!(drivers::validate_registry(&[original, original]).is_err());
    let mut info = *original.info;
    info.id = "different-id-same-usb";
    let other = DriverModule {
        info: Box::leak(Box::new(info)),
        ..original
    };
    assert!(drivers::validate_registry(&[original, other]).is_err());
    assert!(
        drivers::matching(
            &[original, other],
            UsbId {
                vendor: 0x3633,
                product: 0x0009
            }
        )
        .is_err()
    );
}

#[test]
fn caches_are_isolated_under_the_pelicool_configuration_root() {
    let paths = pelicool::storage::paths::AppPaths::discover().unwrap();
    assert_eq!(
        Cache::for_driver("mystique").unwrap().root,
        paths.config.join("devices/mystique")
    );
    assert_eq!(
        Cache::for_driver("test-cooler").unwrap().root,
        paths.config.join("devices/test-cooler")
    );
    for invalid in ["../mystique", "/tmp/escape", "x/y", ""] {
        assert!(Cache::for_driver(invalid).is_err());
    }
}

#[test]
fn usb_detection_is_read_only_and_does_not_match_other_deepcool_products() {
    let unknown = (0u16..=u16::MAX)
        .find(|&product| {
            drivers::matching(
                drivers::REGISTERED,
                UsbId {
                    vendor: 0x3633,
                    product,
                },
            )
            .unwrap()
            .is_none()
        })
        .unwrap();
    let unknown = format!("{unknown:04x}");
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "pelicool-usb-detection-{}-{stamp}",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("1-1")).unwrap();
    std::fs::create_dir_all(root.join("1-2")).unwrap();
    for (directory, product) in [("1-1", "0009"), ("1-2", unknown.as_str())] {
        std::fs::write(root.join(directory).join("idVendor"), "3633\n").unwrap();
        std::fs::write(root.join(directory).join("idProduct"), product).unwrap();
    }
    let detected = drivers::detected_at(&root).unwrap();
    assert_eq!(detected.len(), 1);
    assert_eq!(detected[0].info.id, "mystique");
    assert_eq!(
        std::fs::read_to_string(root.join("1-2/idProduct")).unwrap(),
        unknown
    );
    std::fs::remove_dir_all(root).unwrap();
}
