//! Cache local e catálogo dos cinco slots por tipo. Nenhuma escrita USB aqui.
use crate::media::{EncodedFrame, Fit, Orientation, PreparedMedia};
mod source;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub const SLOTS_PER_KIND: usize = 5;
pub const MAX_COLLECTION_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Slot {
    pub animated: bool,
    pub index: u8,
}
impl Slot {
    pub fn validate(self) -> Result<()> {
        ensure!(
            usize::from(self.index) < SLOTS_PER_KIND,
            "Slot fora do limite de cinco por tipo."
        );
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub source: PathBuf,
    pub animated: bool,
    pub horizontal: bool,
    pub orientation: Orientation,
    pub fit: Fit,
    pub duration_ms: u16,
    pub delays: Vec<u16>,
    #[serde(default)]
    pub framing: crate::media::Framing,
    #[serde(default)]
    pub adaptation: Option<crate::media::Adaptation>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Saved {
    pub id: String,
    pub slot: Slot,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub entries: Vec<Entry>,
    pub saved: Vec<Saved>,
    pub valid: bool,
    /// Ângulo aplicado aos arquivos enviados; ausente nos catálogos anteriores.
    pub media_rotation: Option<u16>,
    pub media_transform_revision: u8,
    #[serde(skip_serializing)]
    pub settings: crate::device::Settings,
    #[serde(skip_serializing)]
    pub rgb_profile: Option<PathBuf>,
    #[serde(skip_serializing)]
    pub openrgb_live: bool,
}
#[derive(Serialize, Deserialize)]
struct DevicePreferences {
    settings: crate::device::Settings,
    rgb_profile: Option<PathBuf>,
    openrgb_live: bool,
}
/// Mantém a trava do módulo e uma trava compartilhada da configuração global.
pub struct CacheLock {
    _collection: fs::File,
    _configuration: Option<fs::File>,
}
impl Catalog {
    pub fn rotation_pending(&self, info: &crate::drivers::DriverInfo, rotation: u16) -> bool {
        !self.entries.is_empty()
            && info.capabilities.media.is_some_and(|m| {
                m.rotation_requires_upload
                    && (!self.valid
                        || self.media_rotation != Some(rotation)
                        || self.media_transform_revision != m.transform_revision)
            })
    }
    pub fn slot(&self, id: &str) -> Option<Slot> {
        self.valid
            .then(|| self.saved.iter().find(|s| s.id == id).map(|s| s.slot))
            .flatten()
    }
    /// Slot previsto após publicar a seleção local inteira, mesmo antes de commit.
    pub fn planned_slot(&self, id: &str) -> Option<Slot> {
        let mut counts = [0u8; 2];
        for entry in &self.entries {
            let kind = usize::from(entry.animated);
            let slot = Slot {
                animated: entry.animated,
                index: counts[kind],
            };
            counts[kind] += 1;
            if entry.id == id {
                return Some(slot);
            }
        }
        None
    }
    pub fn commit(&mut self) {
        let mut counts = [0u8; 2];
        self.saved = self
            .entries
            .iter()
            .map(|entry| {
                let kind = usize::from(entry.animated);
                let slot = Slot {
                    animated: entry.animated,
                    index: counts[kind],
                };
                counts[kind] += 1;
                Saved {
                    id: entry.id.clone(),
                    slot,
                }
            })
            .collect();
        self.valid = true;
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.media_rotation
                .is_none_or(|angle| [0, 90, 180, 270].contains(&angle)),
            "Ângulo da coleção inválido."
        );
        ensure!(
            self.entries.len() <= SLOTS_PER_KIND * 2 && self.saved.len() <= SLOTS_PER_KIND * 2,
            "Catálogo grande demais."
        );
        for animated in [false, true] {
            ensure!(
                self.entries
                    .iter()
                    .filter(|e| e.animated == animated)
                    .count()
                    <= SLOTS_PER_KIND,
                "Máximo de cinco imagens e cinco GIFs."
            );
        }
        let mut ids = std::collections::HashSet::new();
        for entry in &self.entries {
            valid_id(&entry.id)?;
            entry.framing.validate()?;
            ensure!(ids.insert(&entry.id), "ID duplicado no catálogo.");
            ensure!(
                !entry.delays.is_empty() && entry.delays.len() <= crate::media::MAX_FRAMES,
                "Quadros do cache inválidos."
            );
            ensure!(
                entry.animated || entry.delays.len() == 1,
                "Imagem do cache inválida."
            );
        }
        for saved in &self.saved {
            valid_id(&saved.id)?;
            saved.slot.validate()?;
        }
        Ok(())
    }
}
fn valid_id(id: &str) -> Result<()> {
    ensure!(
        id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()),
        "ID do cache inválido."
    );
    Ok(())
}
#[derive(Clone)]
pub struct Cache {
    pub root: PathBuf,
    legacy: Option<PathBuf>,
    config_root: Option<PathBuf>,
}
impl Cache {
    pub fn at(root: PathBuf) -> Self {
        Self {
            root,
            legacy: None,
            config_root: None,
        }
    }

    pub fn for_driver(id: &str) -> Result<Self> {
        Self::for_paths(&crate::storage::paths::AppPaths::discover()?, id)
    }
    pub fn for_paths(paths: &crate::storage::paths::AppPaths, id: &str) -> Result<Self> {
        let legacy = if id == crate::drivers::mystique::ID {
            paths.legacy.clone()
        } else {
            paths.legacy.join("devices").join(id)
        };
        Ok(Self {
            root: paths.device(id)?,
            legacy: Some(legacy),
            config_root: Some(paths.config.clone()),
        })
    }
    pub fn catalog_path(&self) -> PathBuf {
        self.root.join("collection/catalog.json")
    }

    pub fn lock(&self) -> Result<CacheLock> {
        let configuration = self
            .config_root
            .as_ref()
            .map(|root| crate::storage::configuration_lock(root, false))
            .transpose()?;
        crate::storage::private_dir(&self.root.join("runtime"))?;
        let file = fs::OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(self.root.join("runtime/collection.lock"))?;
        file.try_lock().context(
            "A coleção já está aberta em outra janela ou comando. Feche a outra instância.",
        )?;
        if let Some(legacy) = &self.legacy {
            crate::storage::migration::import(self, legacy)?;
        }
        Ok(CacheLock {
            _collection: file,
            _configuration: configuration,
        })
    }
    pub fn load(&self) -> Result<Catalog> {
        let path = self.catalog_path();
        if !path.exists() {
            return Ok(Catalog::default());
        }
        ensure!(
            fs::metadata(&path)?.len() <= 1024 * 1024,
            "Catálogo local grande demais."
        );
        let mut catalog: Catalog = serde_json::from_slice(&fs::read(path)?)?;
        let preferences = self.root.join("settings/preferences.json");
        if preferences.exists() {
            ensure!(
                fs::metadata(&preferences)?.len() <= 65536,
                "Preferências grandes demais."
            );
            let prefs: DevicePreferences = serde_json::from_slice(&fs::read(preferences)?)?;
            catalog.settings = prefs.settings;
            catalog.rgb_profile = prefs.rgb_profile;
            catalog.openrgb_live = prefs.openrgb_live;
        }
        catalog.validate()?;
        Ok(catalog)
    }
    pub fn save(&self, catalog: &Catalog) -> Result<()> {
        catalog.validate()?;
        let preferences = DevicePreferences {
            settings: catalog.settings,
            rgb_profile: catalog.rgb_profile.clone(),
            openrgb_live: catalog.openrgb_live,
        };
        crate::storage::write_json(&self.root.join("settings/preferences.json"), &preferences)?;
        crate::storage::write_json(&self.catalog_path(), &catalog)?;
        Ok(())
    }
    pub fn frame_path(&self, entry: &Entry, frame: usize) -> Result<PathBuf> {
        valid_id(&entry.id)?;
        ensure!(frame < entry.delays.len(), "Quadro do cache inexistente.");
        Ok(self
            .root
            .join("media")
            .join(&entry.id)
            .join(format!("{frame}.jpg")))
    }
    pub fn stage(
        &self,
        source: &Path,
        media: &PreparedMedia,
        orientation: Orientation,
        fit: Fit,
    ) -> Result<Entry> {
        media.validate()?;
        let id = format!(
            "{:032x}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        );
        let directory = self.root.join("media").join(&id);
        crate::storage::private_dir(&directory)?;
        for (i, frame) in media.frames.iter().enumerate() {
            fs::write(directory.join(format!("{i}.jpg")), &frame.jpeg)?;
        }
        if source.is_file() {
            ensure!(
                source.metadata()?.len() <= 50 * 1024 * 1024,
                "Original excede 50 MiB."
            );
            fs::copy(source, directory.join("original"))?;
        }
        Ok(Entry {
            framing: crate::media::Framing::default(),
            adaptation: media.adaptation.clone(),
            id,
            name: source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            source: source.canonicalize().unwrap_or_else(|_| source.to_owned()),
            animated: media.animated,
            horizontal: media.horizontal,
            orientation,
            fit,
            duration_ms: media.duration_ms,
            delays: media.frames.iter().map(|f| f.delay_ms).collect(),
        })
    }
    pub fn original_path(&self, entry: &Entry) -> Result<PathBuf> {
        valid_id(&entry.id)?;
        let original = self.root.join("media").join(&entry.id).join("original");
        if original.is_file() {
            Ok(original)
        } else {
            source::resolve(&entry.source)
        }
    }

    /// Recalcula o recorte no original antes de girar; o cache de JPEGs permanece
    /// canônico. Sem original, o driver ainda pode reutilizar os quadros salvos.
    pub fn display_media(
        &self,
        entry: &Entry,
        module: &crate::drivers::DriverModule,
        settings: crate::device::Settings,
        budget: usize,
    ) -> Result<PreparedMedia> {
        if !module
            .info
            .capabilities
            .media
            .is_some_and(|m| m.rotation_requires_upload)
        {
            return self.media(entry, false);
        }
        let Ok(path) = self.original_path(entry) else {
            return self.media(entry, false);
        };
        let mut options = crate::media::PrepareOptions::new(entry.orientation, entry.fit);
        options.framing = entry.framing;
        options.max_bytes = budget;
        let media = module.prepare_for_display(&path, options, settings)?;
        let copy = self.root.join("media").join(&entry.id).join("original");
        if !copy.is_file() {
            fs::copy(&path, copy).context("Não foi possível preservar a cópia original")?;
        }
        Ok(media)
    }

    /// Remove apenas pastas de IDs do cache que não são mais referenciadas.
    pub fn prune(&self, catalog: &Catalog) -> Result<usize> {
        catalog.validate()?;
        let keep: std::collections::HashSet<_> = catalog
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .chain(catalog.saved.iter().map(|saved| saved.id.as_str()))
            .collect();
        let media = self.root.join("media");
        if !media.exists() {
            return Ok(0);
        }
        let mut removed = 0;
        for entry in fs::read_dir(media)? {
            let entry = entry?;
            let id = entry.file_name();
            let Some(id) = id.to_str() else {
                continue;
            };
            if valid_id(id).is_ok() && !keep.contains(id) {
                let kind = entry.file_type()?;
                if kind.is_symlink() {
                    fs::remove_file(entry.path())?;
                } else if kind.is_dir() {
                    fs::remove_dir_all(entry.path())?;
                } else {
                    continue;
                }
                removed += 1;
            }
        }
        Ok(removed)
    }

    pub fn media(&self, entry: &Entry, previews: bool) -> Result<PreparedMedia> {
        ensure!(
            !entry.delays.is_empty() && entry.delays.len() <= crate::media::MAX_FRAMES,
            "Cache de mídia inválido."
        );
        let mut frames = Vec::new();
        let mut size = 0;
        for (i, &delay_ms) in entry.delays.iter().enumerate() {
            let path = self.frame_path(entry, i)?;
            ensure!(
                fs::metadata(&path)?.len() <= 2 * 1024 * 1024,
                "Quadro do cache grande demais."
            );
            let jpeg =
                fs::read(path).context("Mídia local ausente. Adicione o arquivo novamente.")?;
            size += jpeg.len();
            ensure!(size <= 24 * 1024 * 1024, "Cache excede 24 MiB.");
            let preview = if previews {
                let mut reader =
                    image::ImageReader::new(std::io::Cursor::new(&jpeg)).with_guessed_format()?;
                let mut limits = image::Limits::default();
                limits.max_image_width = Some(640);
                limits.max_image_height = Some(640);
                limits.max_alloc = Some(4 * 1024 * 1024);
                reader.limits(limits);
                let decoded = reader.decode()?.to_rgba8();
                let upright = if entry.horizontal {
                    image::imageops::rotate270(&decoded)
                } else {
                    decoded
                };
                image::DynamicImage::ImageRgba8(upright)
                    .thumbnail(480, 480)
                    .to_rgba8()
            } else {
                image::RgbaImage::new(1, 1)
            };
            frames.push(EncodedFrame {
                jpeg,
                preview,
                delay_ms,
            });
        }
        let media = PreparedMedia {
            display_rotation: None,
            adaptation: entry.adaptation.clone(),
            frames,
            animated: entry.animated,
            horizontal: entry.horizontal,
            duration_ms: entry.duration_ms,
        };
        media.validate()?;
        Ok(media)
    }
}
pub fn validate_batch(media: &[&PreparedMedia]) -> Result<()> {
    ensure!(!media.is_empty(), "Adicione pelo menos uma imagem ou GIF.");
    ensure!(
        media.len() <= SLOTS_PER_KIND * 2,
        "Máximo de cinco imagens e cinco GIFs."
    );
    let mut counts = [0; 2];
    let mut bytes = 0;
    for item in media {
        item.validate()?;
        counts[usize::from(item.animated)] += 1;
        bytes += item.byte_len();
    }
    ensure!(
        counts.iter().all(|&v| v <= SLOTS_PER_KIND),
        "Máximo de cinco imagens e cinco GIFs."
    );
    ensure!(
        bytes <= MAX_COLLECTION_BYTES,
        "Coleção convertida excede 64 MiB."
    );
    Ok(())
}

pub mod sync;
