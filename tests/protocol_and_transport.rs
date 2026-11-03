use anyhow::{Result, bail};
use pelicool::{
    device::{DisplayMode, Preset, Settings},
    drivers::mystique::Controller,
    drivers::mystique::protocol::{self as p, Command},
    media::{EncodedFrame, PreparedMedia},
    transport::Transport,
};

#[derive(Default)]
struct Mock {
    writes: Vec<(u8, Vec<u8>)>,
    last_command: u8,
    fail_on: Option<usize>,
}

#[test]
fn presets_select_firmware_modes_without_erasing_stored_media() {
    for mode in [DisplayMode::Monitoring, DisplayMode::Recorder] {
        let mut controller = Controller::new(Mock::default());
        let preset = Preset {
            mode,
            main: 0,
            style: 1,
            auxiliary: 1,
            graph_clock: true,
            graph_temperature: false,
        };
        controller
            .configure_preset(Settings::default(), preset, true)
            .unwrap();
        let commands: Vec<_> = controller
            .transport
            .writes
            .iter()
            .map(|(_, packet)| packet[2])
            .collect();
        assert_eq!(controller.transport.writes[0].1[2..4], [0x03, mode as u8]);
        assert_eq!(controller.transport.writes[1].1[2..7], [0x04, 0, 1, 0, 1]);
        assert_eq!(controller.transport.writes[2].1[2..5], [0x05, 1, 0]);
        assert!(!commands.iter().any(|cmd| [0x0f, 0x13, 0x14].contains(cmd)));
        assert_eq!(commands.contains(&0x06), mode == DisplayMode::Recorder);
        let time = controller
            .transport
            .writes
            .iter()
            .find(|(_, packet)| packet[2] == 0x0a)
            .unwrap();
        assert_eq!(time.0, 0x02);
    }
}

#[test]
fn sensor_stream_uses_data_endpoint_and_missing_source_blocks_preset() {
    let mut controller = Controller::new(Mock::default());
    let snapshot = pelicool::sensors::Snapshot::default();
    assert!(Preset::default().check_sources(&snapshot).is_err());
    controller.update_sensors(&snapshot).unwrap();
    assert_eq!(
        controller
            .transport
            .writes
            .iter()
            .map(|(ep, _)| *ep)
            .collect::<Vec<_>>(),
        [0x02, 0x02]
    );
    assert_eq!(controller.transport.writes[1].1[2], 0x01);
    assert!(
        controller.transport.writes[1].1[3..30]
            .iter()
            .all(|&v| v == 0)
    );
}
impl Transport for Mock {
    fn write(&mut self, endpoint: u8, bytes: &[u8]) -> Result<()> {
        if self.fail_on == Some(self.writes.len()) {
            bail!("simulated unplug");
        }
        if bytes.len() == 48 && bytes[..2] == [0xaa, 0x2e] {
            self.last_command = bytes[2];
        }
        self.writes.push((endpoint, bytes.to_vec()));
        Ok(())
    }
    fn read(&mut self, _endpoint: u8) -> Result<Vec<u8>> {
        let mut raw = vec![0; 48];
        raw[..3].copy_from_slice(&[0x55, 0x2e, self.last_command]);
        raw[42..46].copy_from_slice(b"HIDC");
        let sum = p::checksum(&raw[..46]);
        raw[46..48].copy_from_slice(&sum.to_le_bytes());
        Ok(raw)
    }
}

// Corpo JPEG opaco: estes testes verificam o transporte, não o decodificador.
fn media() -> PreparedMedia {
    let mut jpeg = vec![0xff, 0xd8, 0xff];
    jpeg.extend([42u8; 97]);
    jpeg.extend([0xff, 0xd9]);
    PreparedMedia {
        display_rotation: Some(90),
        adaptation: None,
        frames: vec![EncodedFrame {
            jpeg,
            preview: image::RgbaImage::new(1, 1),
            delay_ms: 100,
        }],
        animated: false,
        horizontal: false,
        duration_ms: 0,
    }
}

#[test]
fn request_matches_documented_status_capture() {
    let packet = p::packet(Command::ScreenState, &[]).unwrap();
    assert_eq!(&packet[..3], &[0xaa, 0x2e, 0x10]);
    assert_eq!(&packet[42..48], &[0x48, 0x49, 0x44, 0x43, 0, 2]);
    assert!(packet[3..42].iter().all(|&b| b == 0));
}

#[test]
fn corrupt_or_wrong_reply_is_rejected() {
    let mut mock = Mock {
        last_command: Command::ScreenState as u8,
        ..Default::default()
    };
    let raw = mock.read(0x82).unwrap();
    assert!(p::validate_reply(&raw, Command::ScreenState).is_ok());
    for index in [0, 10, 42, 46] {
        let mut corrupt = raw.clone();
        corrupt[index] ^= 1;
        assert!(p::validate_reply(&corrupt, Command::ScreenState).is_err());
    }
    assert!(p::validate_reply(&raw, Command::Mode).is_err());
    assert!(p::validate_reply(&raw[..47], Command::ScreenState).is_err());
}

#[test]
fn file_header_preserves_16_bit_frame_metadata_and_checksums() {
    let header = p::file_header(
        2,
        &[1, 2, 3, 250],
        b"0123456789abcdef0123456789abcdef",
        300,
        1234,
        299,
    )
    .unwrap();
    assert_eq!(&header[..9], &[b'D', b'C', b'L', b'd', 2, 4, 0, 0, 0]);
    assert_eq!(&header[9..13], &[0, 1, 0, 0]);
    assert_eq!(&header[13..19], &[44, 1, 210, 4, 43, 1]);
    assert_eq!(
        u16::from_le_bytes([header[62], header[63]]),
        p::checksum(&header[..62])
    );
}

#[test]
fn probe_only_sends_state_query_on_data_endpoint() {
    let mut controller = Controller::new(Mock::default());
    controller.probe().unwrap();
    assert_eq!(controller.transport.writes.len(), 1);
    assert_eq!(controller.transport.writes[0].0, 0x02);
    assert_eq!(controller.transport.writes[0].1[2], 0x10);
}

#[test]
fn invalid_media_or_missing_replace_flag_never_writes_to_usb() {
    let mut controller = Controller::new(Mock::default());
    assert!(
        controller
            .upload(&media(), Settings::default(), false, |_| {})
            .is_err()
    );
    let mut invalid = media();
    invalid.frames.clear();
    assert!(
        controller
            .upload(&invalid, Settings::default(), true, |_| {})
            .is_err()
    );
    let invalid_settings = Settings {
        brightness: 101,
        ..Default::default()
    };
    assert!(
        controller
            .upload(&media(), invalid_settings, true, |_| {})
            .is_err()
    );
    assert!(controller.transport.writes.is_empty());
}

#[test]
fn invalid_rotation_pixels_never_erase_existing_media_and_preset_gyro_remains_available() {
    let mut controller = Controller::new(Mock::default());
    let mut invalid_pixels = media();
    invalid_pixels.display_rotation = None;
    invalid_pixels.horizontal = true;
    assert!(
        controller
            .upload(
                &invalid_pixels,
                Settings {
                    rotation: 180,
                    ..Default::default()
                },
                true,
                |_| {}
            )
            .is_err()
    );
    assert!(controller.transport.writes.is_empty());
    controller
        .configure_preset(
            Settings {
                gyro: true,
                ..Default::default()
            },
            Preset::default(),
            false,
        )
        .unwrap();
    let options = controller
        .transport
        .writes
        .iter()
        .find(|(_, packet)| packet[2] == Command::Options as u8)
        .unwrap();
    assert_eq!(options.1[4], 1);
}

#[test]
fn upload_keeps_header_and_jpeg_contiguous_and_commits_only_after_data() {
    let media = media();
    let mut controller = Controller::new(Mock::default());
    controller
        .upload(&media, Settings::default(), true, |_| {})
        .unwrap();
    let writes = &controller.transport.writes;
    // Mode, motion, options, format, begin, then contiguous file data.
    assert_eq!(writes[3].1[2], 0x14);
    assert_eq!(writes[4].1[2], 0x0f);
    assert_eq!(writes[5].1.len(), 64);
    assert_eq!(&writes[5].1[..4], b"DCLd");
    assert_eq!(&writes[6].1[..3], &[0xff, 0xd8, 0xff]);
    let file: Vec<u8> = writes[5..8]
        .iter()
        .flat_map(|(_, bytes)| bytes.clone())
        .collect();
    assert_eq!(&file[64..], media.frames[0].jpeg.as_slice());
    assert_eq!(writes[8].1, p::FINISH);
    assert_eq!(writes[writes.len() - 2].1[2..5], [0x08, 0, 0]);
    assert_eq!(writes.last().unwrap().1[2], Command::Options as u8);
}

#[test]
fn unplug_during_data_stops_without_retry_or_finish_marker() {
    let mut controller = Controller::new(Mock {
        fail_on: Some(6),
        ..Default::default()
    });
    assert!(
        controller
            .upload(&media(), Settings::default(), true, |_| {})
            .is_err()
    );
    assert_eq!(controller.transport.writes.len(), 6);
    assert!(
        !controller
            .transport
            .writes
            .iter()
            .any(|(_, bytes)| bytes == p::FINISH)
    );
}

#[test]
fn sixth_file_or_invalid_slot_never_writes_to_usb() {
    use pelicool::collection::Slot;
    let picture = media();
    let mut controller = Controller::new(Mock::default());
    assert!(
        controller
            .upload_collection(&[&picture; 6], Settings::default(), |_| {})
            .is_err()
    );
    assert!(
        controller
            .select_media(Slot {
                animated: true,
                index: 5
            })
            .is_err()
    );
    assert!(controller.transport.writes.is_empty());
    controller
        .select_media(Slot {
            animated: true,
            index: 4,
        })
        .unwrap();
    assert_eq!(controller.transport.writes.len(), 1);
    assert_eq!(controller.transport.writes[0].1[2..5], [0x08, 1, 4]);
}
#[test]
fn mixed_collection_has_unique_file_ids_and_only_one_erase() {
    let picture = media();
    let mut gif = media();
    gif.animated = true;
    gif.duration_ms = 100;
    let mut controller = Controller::new(Mock::default());
    controller
        .upload_collection(&[&picture, &picture, &gif], Settings::default(), |_| {})
        .unwrap();
    let headers: Vec<_> = controller
        .transport
        .writes
        .iter()
        .filter(|(_, b)| b.starts_with(b"DCLd"))
        .map(|(_, b)| b)
        .collect();
    assert_eq!(headers.len(), 3);
    assert_eq!([headers[0][4], headers[1][4], headers[2][4]], [1, 1, 2]);
    assert_ne!(&headers[0][20..52], &headers[1][20..52]);
    assert_ne!(&headers[1][20..52], &headers[2][20..52]);
    assert_eq!(
        controller
            .transport
            .writes
            .iter()
            .filter(|(_, b)| b.len() == 48 && b[2] == 0x14)
            .count(),
        1
    );
}
#[test]
fn power_gyro_and_native_temperature_alert_match_vendor_options() {
    use pelicool::device::RgbMode;
    let mut controller = Controller::new(Mock::default());
    controller
        .apply(Settings {
            screen_on: false,
            gyro: true,
            rotation: 270,
            rgb: RgbMode::Internal,
            ..Default::default()
        })
        .unwrap();
    let options = controller
        .transport
        .writes
        .iter()
        .find(|(_, bytes)| bytes[2] == Command::Options as u8)
        .unwrap();
    assert_eq!(options.1[2..8], [2, 1, 0, 3, 1, 0]);
}

/// Simula firmware que recarrega a orientação ao trocar modo ou slot.
#[derive(Default)]
struct ReloadingFirmware {
    transport: Mock,
    rotation: u8,
}
impl Transport for ReloadingFirmware {
    fn write(&mut self, endpoint: u8, bytes: &[u8]) -> Result<()> {
        self.transport.write(endpoint, bytes)?;
        match bytes[2] {
            command if command == Command::Mode as u8 || command == Command::MotionScreen as u8 => {
                self.rotation = 0
            }
            command if command == Command::Options as u8 => self.rotation = bytes[5],
            _ => (),
        }
        Ok(())
    }
    fn read(&mut self, endpoint: u8) -> Result<Vec<u8>> {
        self.transport.read(endpoint)
    }
}

#[test]
fn manual_rotation_survives_mode_changes_and_image_or_gif_selection() {
    use pelicool::collection::Slot;
    for rotation in [0, 90, 180, 270] {
        let mut controller = Controller::new(ReloadingFirmware::default());
        controller
            .apply(Settings {
                rotation,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(controller.transport.rotation, (rotation / 90) as u8);
        for animated in [false, true] {
            controller
                .select_media(Slot { animated, index: 1 })
                .unwrap();
            assert_eq!(controller.transport.rotation, (rotation / 90) as u8);
        }
        assert!(
            !controller
                .transport
                .transport
                .writes
                .iter()
                .any(|(_, bytes)| bytes[2] == Command::FormatResources as u8)
        );
    }
}

#[test]
fn hardware_slideshow_stays_disabled_even_when_the_worker_sequences_media() {
    for slideshow in [false, true] {
        let mut controller = Controller::new(Mock::default());
        controller
            .apply(Settings {
                slideshow,
                ..Default::default()
            })
            .unwrap();
        let motion = controller
            .transport
            .writes
            .iter()
            .find(|(_, bytes)| bytes[2] == Command::Motion as u8)
            .unwrap();
        assert_eq!(
            motion.1[3], 0,
            "Animação automática do firmware deve permanecer desativada."
        );
        assert_eq!(
            motion.1[6], 0,
            "Duração automática do firmware deve permanecer desativada."
        );
    }
}

#[test]
fn manual_rotation_is_also_applied_after_monitoring_and_recording_mode_changes() {
    for mode in [DisplayMode::Monitoring, DisplayMode::Recorder] {
        let mut controller = Controller::new(ReloadingFirmware::default());
        controller
            .configure_preset(
                Settings {
                    rotation: 270,
                    ..Default::default()
                },
                Preset {
                    mode,
                    ..Default::default()
                },
                true,
            )
            .unwrap();
        assert_eq!(controller.transport.rotation, 3);
    }
}
