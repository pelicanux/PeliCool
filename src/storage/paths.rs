//! Raízes XDG com capitalização estável. Não escreve no diretório do AppImage.
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AppPaths {
    pub config: PathBuf,
    pub installation: PathBuf,
    pub applications: PathBuf,
    pub(crate) legacy: PathBuf,
}
impl AppPaths {
    pub fn discover() -> Result<Self> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .context("HOME não definido")?;
        Self::from_roots(
            &home,
            std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
        )
    }
    pub fn from_roots(home: &Path, config: Option<PathBuf>, data: Option<PathBuf>) -> Result<Self> {
        ensure!(home.is_absolute(), "HOME deve ser absoluto.");
        let config = config
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let data = data
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"));
        let result = Self {
            config: config.join("PeliCool"),
            installation: data.join("PeliCool"),
            applications: data.join("applications"),
            legacy: data.join("deepcool-linux"),
        };
        ensure!(
            !result.config.starts_with(&result.installation)
                && !result.installation.starts_with(&result.config),
            "As raízes de configuração e instalação precisam ser separadas."
        );
        Ok(result)
    }
    pub fn device(&self, id: &str) -> Result<PathBuf> {
        ensure!(
            !id.is_empty()
                && id
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-'),
            "ID de módulo inválido para armazenamento."
        );
        Ok(self.config.join("devices").join(id))
    }
    pub fn appimage(&self) -> PathBuf {
        self.installation.join("bin/PeliCool.AppImage")
    }
    pub fn desktop(&self) -> PathBuf {
        self.applications.join("pelicool.desktop")
    }
}
