use pelicool::{
    collection::{Cache, Catalog, Slot},
    media::{self, Fit, Orientation},
};
use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
struct Fixture(Cache);
impl Fixture {
    fn new() -> Self {
        Self(Cache::at(std::env::temp_dir().join(format!(
                "mystique-cache-test-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0.root);
    }
}
#[test]
fn cached_frames_survive_missing_original_and_catalog_restart() {
    let f = Fixture::new();
    fs::create_dir_all(&f.0.root).unwrap();
    let source = f.0.root.join("photo.png");
    image::RgbaImage::from_pixel(12, 8, image::Rgba([40, 180, 110, 255]))
        .save(&source)
        .unwrap();
    let prepared = media::prepare(&source, Orientation::Auto, Fit::Cover).unwrap();
    let entry =
        f.0.stage(&source, &prepared, Orientation::Auto, Fit::Cover)
            .unwrap();
    fs::remove_file(source).unwrap();
    let mut catalog = Catalog {
        entries: vec![entry.clone()],
        ..Default::default()
    };
    catalog.commit();
    f.0.save(&catalog).unwrap();
    assert_eq!(
        f.0.load().unwrap().slot(&entry.id),
        Some(Slot {
            animated: false,
            index: 0
        })
    );
    let cached = f.0.media(&entry, true).unwrap();
    assert_eq!(cached.frames[0].jpeg, prepared.frames[0].jpeg);
    assert_eq!(
        cached.frames[0].preview.dimensions(),
        prepared.frames[0].preview.dimensions()
    );
    catalog.valid = false;
    f.0.save(&catalog).unwrap();
    assert_eq!(f.0.load().unwrap().slot(&entry.id), None);
}
#[test]
fn cache_rejects_traversal_and_duplicate_ids() {
    let f = Fixture::new();
    fs::create_dir_all(&f.0.root).unwrap();
    let source = f.0.root.join("photo.png");
    image::RgbaImage::new(8, 8).save(&source).unwrap();
    let media = media::prepare(&source, Orientation::Auto, Fit::Cover).unwrap();
    let mut entry =
        f.0.stage(&source, &media, Orientation::Auto, Fit::Cover)
            .unwrap();
    let catalog = Catalog {
        entries: vec![entry.clone(), entry.clone()],
        ..Default::default()
    };
    assert!(f.0.save(&catalog).is_err());
    entry.id = "../../anything".into();
    assert!(f.0.frame_path(&entry, 0).is_err());
}
#[test]
fn exclusive_lock_prevents_two_writers() {
    let f = Fixture::new();
    let lock = f.0.lock().unwrap();
    assert!(f.0.lock().is_err());
    drop(lock);
    assert!(f.0.lock().is_ok());
}

#[test]
fn extensionless_cached_gif_can_be_reopened_reframed_and_rendered_after_source_is_deleted() {
    use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder};
    use pelicool::{device::Settings, drivers::mystique::MODULE};
    let f = Fixture::new();
    std::fs::create_dir_all(&f.0.root).unwrap();
    let source = f.0.root.join("source.gif");
    {
        let mut encoder = GifEncoder::new(std::fs::File::create(&source).unwrap());
        for (color, delay) in [([240, 20, 20, 255], 100), ([20, 240, 20, 255], 200)] {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(80, 60, Rgba(color)),
                    0,
                    0,
                    Delay::from_numer_denom_ms(delay, 1),
                ))
                .unwrap();
        }
    }
    let prepared = MODULE
        .prepare(&source, Orientation::Auto, Fit::Cover)
        .unwrap();
    let mut entry =
        f.0.stage(&source, &prepared, Orientation::Auto, Fit::Cover)
            .unwrap();
    let before = std::fs::read(f.0.frame_path(&entry, 0).unwrap()).unwrap();
    std::fs::remove_file(&source).unwrap();
    let original = f.0.original_path(&entry).unwrap();
    assert_eq!(original.extension(), None);
    assert_eq!(
        media::source_preview(&original).unwrap().dimensions(),
        (80, 60)
    );
    entry.framing.zoom = 1.5;
    let rendered =
        f.0.display_media(
            &entry,
            &MODULE,
            Settings {
                rotation: MODULE.info.raw_rotation(90),
                ..Default::default()
            },
            media::MAX_OUTPUT,
        )
        .unwrap();
    assert!(rendered.animated);
    assert_eq!(rendered.display_rotation, Some(90));
    assert_eq!(rendered.frames.len(), 2);
    assert_eq!(rendered.duration_ms, 300);
    assert_eq!(
        std::fs::read(f.0.frame_path(&entry, 0).unwrap()).unwrap(),
        before
    );
    let canonical = MODULE
        .prepare(&original, entry.orientation, entry.fit)
        .unwrap();
    let replacement =
        f.0.stage(&original, &canonical, entry.orientation, entry.fit)
            .unwrap();
    let mut catalog = Catalog {
        entries: vec![replacement.clone()],
        ..Default::default()
    };
    catalog.commit();
    f.0.prune(&catalog).unwrap();
    assert!(media::source_preview(&f.0.original_path(&replacement).unwrap()).is_ok());
}

#[test]
fn legacy_catalog_preserves_slots_but_requires_explicit_rotation_update() {
    let f = Fixture::new();
    fs::create_dir_all(&f.0.root).unwrap();
    let source = f.0.root.join("photo.png");
    image::RgbaImage::new(8, 8).save(&source).unwrap();
    let prepared = media::prepare(&source, Orientation::Auto, Fit::Cover).unwrap();
    let entry =
        f.0.stage(&source, &prepared, Orientation::Auto, Fit::Cover)
            .unwrap();
    let mut catalog = Catalog {
        entries: vec![entry.clone()],
        ..Default::default()
    };
    catalog.commit();
    let mut json = serde_json::to_value(&catalog).unwrap();
    json.as_object_mut().unwrap().remove("media_rotation");
    let mut loaded: Catalog = serde_json::from_value(json).unwrap();
    assert!(loaded.slot(&entry.id).is_some());
    assert!(loaded.rotation_pending(&pelicool::drivers::mystique::INFO, 0));
    loaded.media_rotation = Some(180);
    loaded.media_transform_revision = pelicool::drivers::mystique::INFO
        .capabilities
        .media
        .unwrap()
        .transform_revision;
    f.0.save(&loaded).unwrap();
    let mut loaded = f.0.load().unwrap();
    assert!(!loaded.rotation_pending(&pelicool::drivers::mystique::INFO, 180));
    assert!(loaded.rotation_pending(&pelicool::drivers::mystique::INFO, 90));
    loaded.valid = false;
    assert!(loaded.rotation_pending(&pelicool::drivers::mystique::INFO, 180));
}

#[test]
fn pruning_removes_only_unreferenced_cache_ids_and_preserves_unknown_files() {
    let f = Fixture::new();
    let source = f.0.root.join("source.png");
    fs::create_dir_all(&f.0.root).unwrap();
    image::RgbaImage::new(8, 8).save(&source).unwrap();
    let media = media::prepare(&source, Orientation::Auto, Fit::Cover).unwrap();
    let keep =
        f.0.stage(&source, &media, Orientation::Auto, Fit::Cover)
            .unwrap();
    let remove =
        f.0.stage(&source, &media, Orientation::Auto, Fit::Cover)
            .unwrap();
    fs::write(f.0.root.join("media/personal.txt"), "keep").unwrap();
    let catalog = Catalog {
        entries: vec![keep.clone()],
        ..Default::default()
    };
    assert_eq!(f.0.prune(&catalog).unwrap(), 1);
    assert!(f.0.frame_path(&keep, 0).unwrap().exists());
    assert!(!f.0.frame_path(&remove, 0).unwrap().exists());
    assert!(f.0.root.join("media/personal.txt").exists());
    assert!(source.exists());
}
