//! Preferências globais da interface e integrações; independentes do modelo.
use super::write_json;
use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenRgb {
    pub enabled: bool,
    pub sdk: bool,
    pub profile: Option<PathBuf>,
}
impl Default for OpenRgb {
    fn default() -> Self {
        Self {
            enabled: true,
            sdk: false,
            profile: None,
        }
    }
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Interface {
    pub dark: bool,
    pub minimize_on_close: bool,
    pub openrgb: Option<OpenRgb>,
}
impl Interface {
    pub fn load(root: &Path) -> Result<Self> {
        let path = root.join("settings/interface.json");
        if !path.exists() {
            return Ok(Self::default());
        }
        ensure!(
            fs::metadata(&path)?.len() < 65536,
            "Preferências da interface grandes demais."
        );
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    }
    pub fn save(&self, root: &Path) -> Result<()> {
        write_json(&root.join("settings/interface.json"), self)
    }
}
