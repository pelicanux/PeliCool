//! Importação única do armazenamento anterior. A origem permanece como backup.
use crate::collection::{Cache, Catalog};
use anyhow::{Context, Result, ensure};
use std::{fs, os::unix::fs::OpenOptionsExt, path::Path};

pub(crate) fn import(cache: &Cache, source: &Path) -> Result<()> {
    let old_catalog = source.join("collection.json");
    if cache.catalog_path().exists() || !old_catalog.exists() {
        return Ok(());
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(source.join("collection.lock"))?;
    lock.try_lock()
        .context("Feche a versão anterior do aplicativo para migrar os dados.")?;
    ensure!(
        fs::metadata(&old_catalog)?.len() <= 1024 * 1024,
        "Catálogo antigo grande demais."
    );
    let catalog: Catalog = serde_json::from_slice(&fs::read(&old_catalog)?)?;
    catalog.validate()?;
    let old = Cache::at(source.to_path_buf());
    // Valida tudo antes de publicar o novo catálogo, que mantém IDs e slots.
    for entry in &catalog.entries {
        old.media(entry, false)?;
    }
    for entry in &catalog.entries {
        for frame in 0..entry.delays.len() {
            let destination = cache.frame_path(entry, frame)?;
            super::private_dir(destination.parent().expect("frame directory"))?;
            let original = source
                .join("media")
                .join(&entry.id)
                .join(format!("{frame}.jpg"));
            fs::copy(original, destination)?;
        }
    }
    for entry in &catalog.entries {
        let original = source.join("media").join(&entry.id).join("original");
        if original.is_file() && original.metadata()?.len() <= 50 * 1024 * 1024 {
            fs::copy(
                original,
                cache.root.join("media").join(&entry.id).join("original"),
            )?;
        }
    }
    cache.save(&catalog)?;
    super::write_json(
        &cache.root.join("migration/import.json"),
        &serde_json::json!({
            "version": 1, "source": source, "original_preserved": true
        }),
    )?;
    Ok(())
}
