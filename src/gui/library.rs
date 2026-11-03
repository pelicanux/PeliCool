//! Cópias locais e preparação assíncrona. O worker é o único dono do USB.
use super::{App, Event, LibraryItem, Page, Task, is_gif, thumbnail};
use anyhow::Result;
use pelicool::{collection::Entry, device::DisplayMode};
use std::{path::PathBuf, sync::Arc};

impl App {
    pub(super) fn add_cached(&mut self, entry: Entry) {
        let path = match self.cache.frame_path(&entry, 0) {
            Ok(path) => path,
            Err(_) => return,
        };
        let horizontal = entry.horizontal;
        self.library.push(LibraryItem {
            entry,
            thumbnail: None,
        });
        self.thumbnails_pending += 1;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let event = match thumbnail(&path) {
                Ok(image) => Event::Thumbnail(
                    path,
                    if horizontal {
                        image::imageops::rotate270(&image)
                    } else {
                        image
                    },
                ),
                Err(_) => Event::ThumbnailFailed,
            };
            let _ = tx.send(event);
        });
    }

    pub(super) fn stage(&mut self, path: PathBuf, replace: Option<usize>) {
        let animated = replace
            .and_then(|index| self.library.get(index))
            .map_or_else(|| is_gif(&path), |item| item.entry.animated);
        if replace.is_none()
            && self
                .library
                .iter()
                .filter(|item| item.entry.animated == animated)
                .count()
                >= self.media_limit(animated)
        {
            self.error = true;
            self.status = format!(
                "O módulo aceita até {} arquivos deste tipo. Remova um item antes de adicionar outro.",
                self.media_limit(animated)
            );
            return;
        }
        self.mode = DisplayMode::Media;
        self.page = Page::Personalize;
        self.task = Some(Task::Prepare);
        self.progress = 0.;
        self.error = false;
        self.status = "Preparando mídia e salvando uma cópia local…".into();
        let (tx, cache, orientation, fit) = (
            self.tx.clone(),
            self.cache.clone(),
            self.orientation,
            self.fit,
        );
        let module = self.module;
        let entries = self.catalog.entries.clone();
        let framing = self.framing;
        self.auto_upload = true;
        std::thread::spawn(move || {
            let result = (|| -> Result<_> {
                let mut occupied = 0;
                for (index, entry) in entries.iter().enumerate() {
                    if Some(index) != replace {
                        occupied += cache.media(entry, false)?.byte_len();
                    }
                }
                let mut options = pelicool::media::PrepareOptions::new(orientation, fit);
                options.framing = framing;
                options.max_bytes = pelicool::collection::MAX_COLLECTION_BYTES
                    .saturating_sub(occupied)
                    .min(pelicool::media::MAX_OUTPUT);
                let media = module.prepare_with(&path, options)?;
                let mut entry = cache.stage(&path, &media, orientation, fit)?;
                entry.framing = framing;
                if let Some(index) = replace {
                    entry.name = entries[index].name.clone();
                }
                Ok((entry, Arc::new(media)))
            })();
            let _ = tx.send(match result {
                Ok((entry, media)) => Event::Staged(entry, media, replace),
                Err(error) => Event::Error(format!("{error:#}")),
            });
        });
    }

    pub(super) fn select_cached(&mut self, index: usize) {
        self.startup.cancel();
        let entry = self.library[index].entry.clone();
        self.mode = DisplayMode::Media;
        self.page = Page::Personalize;
        self.selected = Some(index);
        self.path = Some(entry.source.clone());
        self.orientation = entry.orientation;
        self.fit = entry.fit;
        self.auto_select = self.catalog.slot(&entry.id);
        self.auto_upload = self.auto_select.is_none();
        self.task = Some(Task::LoadPreview);
        self.error = false;
        self.status = "Carregando a prévia local…".into();
        let (tx, cache) = (self.tx.clone(), self.cache.clone());
        std::thread::spawn(move || {
            let _ = tx.send(match cache.media(&entry, true) {
                Ok(media) => Event::Prepared(Arc::new(media)),
                Err(error) => Event::Error(format!("{error:#}")),
            });
        });
    }

    pub(super) fn import_kind(&mut self, animated: bool) {
        let mut dialog = rfd::FileDialog::new().set_title(if animated {
            "Adicionar GIF"
        } else {
            "Adicionar imagem"
        });
        dialog = if animated {
            dialog.add_filter("GIF", &["gif"])
        } else {
            dialog.add_filter("Imagens", &["jpg", "jpeg", "png", "webp"])
        };
        if let Some(path) = dialog.pick_file() {
            self.choose_media(path, None);
        }
    }

    pub(super) fn prepare(&mut self) {
        let Some(path) = self.path.clone() else {
            return;
        };
        let resolved = self
            .selected
            .and_then(|index| self.library.get(index))
            .map(|item| self.cache.original_path(&item.entry));
        let path = match resolved {
            Some(Ok(path)) => path,
            Some(Err(error)) => {
                self.status = error.to_string();
                let Some(path) = rfd::FileDialog::new()
                    .set_title("Localizar o arquivo original para reeditar")
                    .add_filter("Mídia", &["png", "jpg", "jpeg", "gif", "webp"])
                    .pick_file()
                else {
                    return;
                };
                path
            }
            None => path,
        };
        self.choose_media(path, self.selected);
    }
}
