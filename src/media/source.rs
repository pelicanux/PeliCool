//! Primeiro quadro para o editor, identificado pelo conteúdo, mesmo sem extensão.
use anyhow::Result;
use image::{DynamicImage, ImageDecoder, ImageReader, RgbaImage};
use std::path::Path;

pub fn source_preview(path: &Path) -> Result<RgbaImage> {
    anyhow::ensure!(
        path.metadata()?.len() <= 50 * 1024 * 1024,
        "Arquivo excede 50 MiB."
    );
    let mut reader = ImageReader::open(path)?.with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder()?;
    let orientation = decoder.orientation()?;
    let mut image = DynamicImage::from_decoder(decoder)?;
    image.apply_orientation(orientation);
    Ok(if image.width() > 1600 || image.height() > 1600 {
        image.thumbnail(1600, 1600)
    } else {
        image
    }
    .to_rgba8())
}
