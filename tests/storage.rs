mod common;
use common::TempDir;
use pelicool::{
    collection::{Cache, Catalog},
    media::{self, Fit, Orientation},
    storage::{paths::AppPaths, preferences::Interface},
};
use std::fs;
#[test]
fn xdg_paths_keep_case_ignore_relative_roots_and_never_overlap() {
    let temp = TempDir::new("paths");
    let paths = AppPaths::from_roots(&temp.0, Some("relative".into()), Some("".into())).unwrap();
    assert_eq!(paths.config, temp.0.join(".config/PeliCool"));
    assert_eq!(paths.installation, temp.0.join(".local/share/PeliCool"));
    assert!(AppPaths::from_roots(&temp.0, Some(temp.0.clone()), Some(temp.0.clone())).is_err());
}
#[test]
fn theme_and_close_choice_survive_restart_independently_of_devices() {
    let temp = TempDir::new("preferences");
    let paths = temp.paths();
    assert!(!Interface::load(&paths.config).unwrap().dark);
    Interface {
        dark: true,
        minimize_on_close: true,
        ..Default::default()
    }
    .save(&paths.config)
    .unwrap();
    let restarted = Interface::load(&paths.config).unwrap();
    assert!(restarted.dark && restarted.minimize_on_close);
    assert!(!paths.device("mystique").unwrap().exists());
    fs::write(paths.config.join("settings/interface.json"), b"broken").unwrap();
    assert!(Interface::load(&paths.config).is_err());
}
#[test]
fn migration_keeps_media_slots_preferences_originals_and_the_old_backup() {
    let temp = TempDir::new("migration");
    let paths = temp.paths();
    let legacy = temp.0.join("data/deepcool-linux");
    let old = Cache::at(legacy.clone());
    let source = temp.0.join("source.png");
    image::RgbaImage::new(8, 6).save(&source).unwrap();
    let media = media::prepare(&source, Orientation::Horizontal, Fit::Cover).unwrap();
    let entry = old
        .stage(&source, &media, Orientation::Horizontal, Fit::Cover)
        .unwrap();
    let mut catalog = Catalog {
        entries: vec![entry.clone()],
        ..Default::default()
    };
    catalog.settings.brightness = 17;
    catalog.commit();
    let old_json = serde_json::json!({"entries":catalog.entries,"saved":catalog.saved,"valid":true,"settings":catalog.settings,"rgb_profile":null,"openrgb_live":true});
    fs::write(
        legacy.join("collection.json"),
        serde_json::to_vec(&old_json).unwrap(),
    )
    .unwrap();
    let cache = Cache::for_paths(&paths, "mystique").unwrap();
    let lock = cache.lock().unwrap();
    let migrated = cache.load().unwrap();
    assert_eq!(migrated.settings.brightness, 17);
    assert!(migrated.openrgb_live);
    assert_eq!(migrated.slot(&entry.id), catalog.slot(&entry.id));
    assert_eq!(
        cache.media(&entry, false).unwrap().frames[0].jpeg,
        media.frames[0].jpeg
    );
    assert_eq!(
        fs::read(cache.original_path(&entry).unwrap()).unwrap(),
        fs::read(&source).unwrap()
    );
    assert!(legacy.join("collection.json").exists());
    assert!(cache.root.join("settings/preferences.json").exists());
    assert!(
        !fs::read_to_string(cache.catalog_path())
            .unwrap()
            .contains("brightness")
    );
    let mut changed = migrated;
    changed.settings.brightness = 29;
    cache.save(&changed).unwrap();
    drop(lock);
    let _lock = cache.lock().unwrap();
    assert_eq!(cache.load().unwrap().settings.brightness, 29);
}
#[test]
fn active_old_version_prevents_migration_instead_of_copying_an_inconsistent_catalog() {
    let temp = TempDir::new("migration-lock");
    let paths = temp.paths();
    let old = temp.0.join("data/deepcool-linux");
    fs::create_dir_all(&old).unwrap();
    fs::write(old.join("collection.json"), b"{}").unwrap();
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(old.join("collection.lock"))
        .unwrap();
    file.try_lock().unwrap();
    let cache = Cache::for_paths(&paths, "mystique").unwrap();
    assert!(cache.lock().is_err());
    assert!(!cache.catalog_path().exists());
    drop(file);
    assert!(cache.lock().is_ok());
}
