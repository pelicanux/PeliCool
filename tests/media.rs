use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder};
use pelicool::media::{Fit, Orientation, prepare};
use std::{
    fs::{self, File},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

struct TempFile(PathBuf);
impl TempFile {
    fn new(ext: &str) -> Self {
        Self(std::env::temp_dir().join(format!(
                "pelicool-test-{}-{}.{ext}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )))
    }
}
impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn static_frame_has_correct_orientation_jfif_and_420_sampling() {
    let path = TempFile::new("png");
    RgbaImage::from_pixel(80, 40, Rgba([250, 30, 10, 255]))
        .save(&path.0)
        .unwrap();
    let media = prepare(&path.0, Orientation::Auto, Fit::Cover).unwrap();
    assert!(!media.animated && media.horizontal);
    let frame = &media.frames[0];
    let decoded = image::load_from_memory(&frame.jpeg).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (480, 640));
    assert!(frame.preview.width() > frame.preview.height());
    assert!(frame.jpeg.windows(5).any(|w| w == b"JFIF\0"));
    let sof = frame
        .jpeg
        .windows(2)
        .position(|w| w == [0xff, 0xc0])
        .unwrap();
    assert_eq!(frame.jpeg[sof + 11], 0x22); // Luma 2×2, chroma 1×1.
    assert!(!frame.jpeg.windows(2).any(|w| w == [0xff, 0xc2])); // Baseline, não progressivo.
}

#[test]
fn landscape_270_and_portrait_zero_keep_the_source_top_and_left() {
    let path = TempFile::new("png");
    let colors = [
        [240, 20, 20, 255],
        [20, 240, 20, 255],
        [20, 20, 240, 255],
        [240, 240, 20, 255],
    ];
    RgbaImage::from_fn(80, 60, |x, y| {
        Rgba(colors[usize::from(x >= 40) + 2 * usize::from(y >= 30)])
    })
    .save(&path.0)
    .unwrap();
    for orientation in [Orientation::Horizontal, Orientation::Vertical] {
        let media = prepare(&path.0, orientation, Fit::Cover).unwrap();
        let jpeg = image::load_from_memory(&media.frames[0].jpeg)
            .unwrap()
            .to_rgba8();
        // Firmware 270° undoes landscape packing. Portrait data is already
        // upright at firmware 0°. Keep these byte transforms independent of
        // how angles are illustrated in the GUI.
        let displayed = if orientation == Orientation::Horizontal {
            image::imageops::rotate270(&jpeg)
        } else {
            jpeg
        };
        assert_eq!(displayed.width() > displayed.height(), media.horizontal);
        for (index, expected) in colors.iter().enumerate() {
            let x = displayed.width() * if index % 2 == 0 { 1 } else { 3 } / 4;
            let y = displayed.height() * if index < 2 { 1 } else { 3 } / 4;
            for (actual, expected) in displayed.get_pixel(x, y).0[..3].iter().zip(expected) {
                assert!(actual.abs_diff(*expected) < 12);
            }
        }
    }
}

#[test]
fn display_rotation_uses_original_crop_and_packs_all_angles_without_black_borders() {
    use pelicool::{device::Settings, drivers::mystique::MODULE, media::PrepareOptions};
    let path = TempFile::new("png");
    RgbaImage::from_fn(80, 60, |x, y| {
        Rgba(match (x < 40, y < 30) {
            (true, true) => [240, 20, 20, 255],
            (false, true) => [20, 240, 20, 255],
            (true, false) => [20, 20, 240, 255],
            _ => [240, 240, 20, 255],
        })
    })
    .save(&path.0)
    .unwrap();
    for angle in [0, 90, 180, 270] {
        let prepared = MODULE
            .prepare_for_display(
                &path.0,
                PrepareOptions::new(Orientation::Auto, Fit::Cover),
                Settings {
                    rotation: MODULE.info.raw_rotation(angle),
                    ..Default::default()
                },
            )
            .unwrap();
        assert_eq!(prepared.display_rotation, Some(angle));
        let wire = image::load_from_memory(&prepared.frames[0].jpeg)
            .unwrap()
            .to_rgb8();
        assert_eq!(wire.dimensions(), (480, 640));
        let physical = image::imageops::rotate270(&wire);
        for (x, y) in [(5, 5), (635, 5), (5, 475), (635, 475)] {
            assert!(physical.get_pixel(x, y).0.iter().any(|v| *v > 180));
        }
        let p = physical.get_pixel(160, 120);
        match angle {
            0 => assert!(p[0] > 200 && p[1] < 50),
            90 => assert!(p[2] > 200 && p[0] < 50),
            180 => assert!(p[0] > 200 && p[1] > 200),
            270 => assert!(p[1] > 200 && p[0] < 50),
            _ => unreachable!(),
        }
    }
    let mut left = PrepareOptions::new(Orientation::Auto, Fit::Cover);
    left.framing.x = 0.;
    let mut right = left;
    right.framing.x = 1.;
    let settings = Settings {
        rotation: MODULE.info.raw_rotation(90),
        ..Default::default()
    };
    assert_ne!(
        MODULE
            .prepare_for_display(&path.0, left, settings)
            .unwrap()
            .frames[0]
            .jpeg,
        MODULE
            .prepare_for_display(&path.0, right, settings)
            .unwrap()
            .frames[0]
            .jpeg
    );
}

#[test]
fn gif_keeps_frames_and_total_timing() {
    let path = TempFile::new("gif");
    {
        let mut encoder = GifEncoder::new(File::create(&path.0).unwrap());
        for (color, delay) in [
            ([255, 0, 0, 255], 100),
            ([0, 255, 0, 255], 200),
            ([0, 0, 255, 255], 300),
        ] {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(20, 30, Rgba(color)),
                    0,
                    0,
                    Delay::from_numer_denom_ms(delay, 1),
                ))
                .unwrap();
        }
    }
    let media = prepare(&path.0, Orientation::Auto, Fit::Contain).unwrap();
    assert!(media.animated && !media.horizontal);
    assert_eq!(media.frames.len(), 3);
    assert_eq!(media.duration_ms, 600);
    assert_eq!(
        media
            .frames
            .iter()
            .map(|frame| frame.delay_ms)
            .collect::<Vec<_>>(),
        [100, 200, 300]
    );
    assert_ne!(media.frames[0].jpeg, media.frames[1].jpeg);
}

#[test]
fn transparency_is_composited_on_black() {
    let path = TempFile::new("png");
    RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 0]))
        .save(&path.0)
        .unwrap();
    let media = prepare(&path.0, Orientation::Vertical, Fit::Cover).unwrap();
    let pixel = media.frames[0].preview.get_pixel(20, 20);
    assert!(pixel[0] < 3 && pixel[1] < 3 && pixel[2] < 3);
}

#[test]
fn oversized_dimensions_are_rejected_before_conversion() {
    let path = TempFile::new("png");
    RgbaImage::new(4097, 1).save(&path.0).unwrap();
    assert!(prepare(&path.0, Orientation::Auto, Fit::Cover).is_err());
}

#[test]
fn long_gif_is_adapted_automatically_without_trimming_its_duration() {
    let path = TempFile::new("gif");
    {
        let mut encoder = GifEncoder::new(File::create(&path.0).unwrap());
        for index in 0..181 {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(8, 6, Rgba([index as u8, 30, 100, 255])),
                    0,
                    0,
                    Delay::from_numer_denom_ms(20, 1),
                ))
                .unwrap();
        }
    }
    let media = prepare(&path.0, Orientation::Horizontal, Fit::Cover).unwrap();
    assert!(media.frames.len() <= 120 && media.frames.len() > 1);
    assert_eq!(media.duration_ms, 3620);
    assert_eq!(
        media
            .frames
            .iter()
            .map(|frame| u32::from(frame.delay_ms))
            .sum::<u32>(),
        3620
    );
    let report = media.adaptation.unwrap();
    assert_eq!(report.source_frames, 181);
    assert_eq!(report.output_frames, 120);
}
#[test]
fn crop_keeps_the_requested_side_and_rejects_invalid_coordinates() {
    use pelicool::media::{Framing, PrepareOptions, prepare_with};
    let path = TempFile::new("png");
    RgbaImage::from_fn(200, 60, |x, _| {
        if x < 100 {
            Rgba([240, 10, 10, 255])
        } else {
            Rgba([10, 10, 240, 255])
        }
    })
    .save(&path.0)
    .unwrap();
    let mut options = PrepareOptions::new(Orientation::Horizontal, Fit::Cover);
    options.framing = Framing {
        zoom: 2.,
        x: 1.,
        y: 0.5,
    };
    let media = prepare_with(&path.0, options).unwrap();
    let pixel = media.frames[0].preview.get_pixel(100, 100);
    assert!(pixel[2] > 200 && pixel[0] < 30);
    options.framing.x = f32::NAN;
    assert!(prepare_with(&path.0, options).is_err());
}
