//! Transação de mídia: valida tudo, invalida slots, grava, seleciona e confirma.
//! A GUI não precisa expor formatação/reenvio da seleção local ao usuário.
use super::{Cache, Catalog, Slot};
use crate::device::{Device, Settings};
use anyhow::Result;

pub struct SavedMedia {
    pub shown: Option<Slot>,
    pub cleanup_warning: Option<String>,
}

pub fn save(
    cache: &Cache,
    device: &mut Device,
    catalog: &mut Catalog,
    settings: Settings,
    selected: Option<&str>,
    progress: impl FnMut(f32),
    mut published: impl FnMut(&Catalog),
) -> Result<SavedMedia> {
    catalog.validate()?;
    let canonical = catalog
        .entries
        .iter()
        .map(|entry| cache.media(entry, false))
        .collect::<Result<Vec<_>>>()?;
    let canonical_refs: Vec<_> = canonical.iter().collect();
    if !canonical_refs.is_empty() {
        crate::collection::validate_batch(&canonical_refs)?;
    }
    let mut spare =
        super::MAX_COLLECTION_BYTES - canonical.iter().map(|m| m.byte_len()).sum::<usize>();
    let mut media = Vec::with_capacity(canonical.len());
    for (entry, old) in catalog.entries.iter().zip(&canonical) {
        let budget = (old.byte_len() + spare).min(crate::media::MAX_OUTPUT);
        let rendered = cache.display_media(entry, device.module(), settings, budget)?;
        spare = spare + old.byte_len() - rendered.byte_len();
        media.push(rendered);
    }
    let refs: Vec<_> = media.iter().collect();
    if refs.is_empty() {
        device.validate_clear_media(settings)?;
    } else {
        device.validate_collection(&refs, settings)?;
    }
    let chosen = selected
        .and_then(|id| catalog.planned_slot(id))
        .or_else(|| {
            catalog
                .entries
                .first()
                .and_then(|entry| catalog.planned_slot(&entry.id))
        });
    catalog.valid = false;
    cache.save(catalog)?;
    published(catalog);
    if refs.is_empty() {
        device.clear_media(settings)?;
    } else {
        device.upload_collection(&refs, settings, progress)?;
    }
    if let Some(slot) = chosen {
        device.select_media(slot)?;
    }
    catalog.commit();
    catalog.media_rotation = device
        .info()
        .capabilities
        .media
        .filter(|media| media.rotation_requires_upload)
        .map(|_| settings.rotation);
    catalog.media_transform_revision = device
        .info()
        .capabilities
        .media
        .map_or(0, |m| m.transform_revision);
    cache.save(catalog)?;
    published(catalog);
    let cleanup_warning = cache
        .prune(catalog)
        .err()
        .map(|error| format!("Mídia salva; não foi possível limpar cópias antigas: {error:#}"));
    Ok(SavedMedia {
        shown: chosen,
        cleanup_warning,
    })
}
