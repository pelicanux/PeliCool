mod common;
use anyhow::{Result, bail};
use common::TempDir;
use pelicool::{
    collection::{self, Cache, Catalog, Entry},
    device::Settings,
    drivers::{self, DisplayDriver, DriverModule},
    media::{Fit, Orientation, PreparedMedia},
    transport::Transport,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
#[derive(Default)]
struct Recorded {
    writes: Mutex<Vec<Vec<u8>>>,
    fail: AtomicBool,
}
struct MockTransport(Arc<Recorded>);
impl Transport for MockTransport {
    fn write(&mut self, _endpoint: u8, bytes: &[u8]) -> Result<()> {
        if self.0.fail.load(Ordering::SeqCst) {
            bail!("simulated unplug");
        }
        self.0.writes.lock().unwrap().push(bytes.to_vec());
        Ok(())
    }
    fn read(&mut self, _endpoint: u8) -> Result<Vec<u8>> {
        Ok(vec![])
    }
}
struct MockDriver(Box<dyn Transport + Send>);
impl DisplayDriver for MockDriver {
    fn probe(&mut self) -> Result<String> {
        Ok("mock".into())
    }
    fn clear_media(&mut self, _settings: Settings) -> Result<()> {
        self.0.write(1, &[0x33])
    }
    fn select_media(&mut self, slot: collection::Slot) -> Result<()> {
        self.0
            .write(1, &[0x22, u8::from(slot.animated), slot.index])
    }
    fn upload_collection(
        &mut self,
        media: &[&PreparedMedia],
        _settings: Settings,
        progress: &mut dyn FnMut(f32),
    ) -> Result<()> {
        self.0.write(1, &[0x11, media.len() as u8])?;
        progress(1.);
        Ok(())
    }
}
fn module() -> &'static DriverModule {
    Box::leak(Box::new(DriverModule {
        info: &drivers::mystique::INFO,
        create: |transport| Box::new(MockDriver(transport)),
        prepare_media: drivers::mystique::MODULE.prepare_media,
    }))
}
fn entry(temp: &TempDir, cache: &Cache, index: u8) -> Entry {
    let path = temp.0.join(format!("source-{index}.png"));
    image::RgbaImage::from_pixel(8, 6, image::Rgba([index, 80, 10, 255]))
        .save(&path)
        .unwrap();
    let prepared = pelicool::media::prepare(&path, Orientation::Horizontal, Fit::Cover).unwrap();
    cache
        .stage(&path, &prepared, Orientation::Horizontal, Fit::Cover)
        .unwrap()
}
#[test]
fn adding_keeps_previous_files_and_selects_the_new_slot_after_commit() {
    let temp = TempDir::new("media-save");
    let cache = Cache::at(temp.0.join("cache"));
    let _lock = cache.lock().unwrap();
    let old = entry(&temp, &cache, 1);
    let new = entry(&temp, &cache, 2);
    let mut catalog = Catalog {
        entries: vec![old.clone()],
        ..Default::default()
    };
    catalog.commit();
    cache.save(&catalog).unwrap();
    catalog.entries.push(new.clone());
    let state = Arc::new(Recorded::default());
    let mut device = module().connect(Box::new(MockTransport(state.clone())));
    let mut published = vec![];
    let result = collection::sync::save(
        &cache,
        &mut device,
        &mut catalog,
        Settings::default(),
        Some(&new.id),
        |_| {},
        |catalog| published.push(catalog.valid),
    )
    .unwrap();
    assert_eq!(result.shown.unwrap().index, 1);
    assert_eq!(published, [false, true]);
    assert_eq!(
        *state.writes.lock().unwrap(),
        [vec![0x11, 2], vec![0x22, 0, 1]]
    );
    let saved = cache.load().unwrap();
    assert_eq!(saved.media_rotation, Some(0));
    assert!(!saved.rotation_pending(&drivers::mystique::INFO, 0));
    assert!(saved.rotation_pending(&drivers::mystique::INFO, 180));
    assert!(saved.slot(&old.id).is_some());
    assert_eq!(saved.slot(&new.id).unwrap().index, 1);
    assert!(cache.original_path(&old).unwrap().exists());
}
#[test]
fn failed_save_invalidates_slots_and_keeps_all_local_media_for_retry() {
    let temp = TempDir::new("media-failure");
    let cache = Cache::at(temp.0.join("cache"));
    let _lock = cache.lock().unwrap();
    let old = entry(&temp, &cache, 1);
    let new = entry(&temp, &cache, 2);
    let mut catalog = Catalog {
        entries: vec![old.clone(), new.clone()],
        ..Default::default()
    };
    catalog.commit();
    cache.save(&catalog).unwrap();
    let state = Arc::new(Recorded::default());
    state.fail.store(true, Ordering::SeqCst);
    let mut device = module().connect(Box::new(MockTransport(state.clone())));
    assert!(
        collection::sync::save(
            &cache,
            &mut device,
            &mut catalog,
            Settings::default(),
            Some(&new.id),
            |_| {},
            |_| {}
        )
        .is_err()
    );
    assert!(!cache.load().unwrap().valid);
    for item in [&old, &new] {
        assert!(cache.media(item, false).is_ok());
    }
    state.fail.store(false, Ordering::SeqCst);
    assert!(
        collection::sync::save(
            &cache,
            &mut device,
            &mut catalog,
            Settings::default(),
            Some(&new.id),
            |_| {},
            |_| {}
        )
        .is_ok()
    );
    assert!(cache.load().unwrap().valid);
}
#[test]
fn removing_the_last_item_clears_storage_without_selecting_a_missing_slot() {
    let temp = TempDir::new("media-clear");
    let cache = Cache::at(temp.0.join("cache"));
    let _lock = cache.lock().unwrap();
    let mut catalog = Catalog::default();
    let state = Arc::new(Recorded::default());
    let mut device = module().connect(Box::new(MockTransport(state.clone())));
    assert!(
        collection::sync::save(
            &cache,
            &mut device,
            &mut catalog,
            Settings::default(),
            None,
            |_| {},
            |_| {}
        )
        .unwrap()
        .shown
        .is_none()
    );
    assert_eq!(*state.writes.lock().unwrap(), [vec![0x33]]);
    assert!(cache.load().unwrap().saved.is_empty());
}
