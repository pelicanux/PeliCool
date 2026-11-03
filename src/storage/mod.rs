//! Persistência local. Caminhos, preferências e migração não conhecem USB.
pub(crate) mod migration;
pub mod paths;
pub mod preferences;

use anyhow::{Context, Result};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::Path,
};

pub fn private_dir(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    Ok(())
}

/// Publica JSON por rename; um arquivo incompleto nunca substitui o anterior.
pub fn write_json(path: &Path, value: &impl serde::Serialize) -> Result<()> {
    let parent = path.parent().context("Arquivo sem diretório")?;
    private_dir(parent)?;
    let temporary = parent.join(format!(
        ".write-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    let result = (|| -> Result<()> {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec_pretty(value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

/// Compartilhada durante uso; exclusiva durante remoção das configurações.
pub fn configuration_lock(root: &Path, exclusive: bool) -> Result<fs::File> {
    let runtime = root.join("runtime");
    private_dir(&runtime)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(runtime.join("application.lock"))?;
    if exclusive {
        file.try_lock()
            .context("Feche o PeliCool e seus comandos antes de desinstalar.")?;
    } else {
        file.try_lock_shared()
            .context("A configuração está em manutenção. Tente novamente.")?;
    }
    Ok(file)
}
