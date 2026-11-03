//! Orientação dos JPEGs no MYSTIQUE. Os arquivos canônicos do cache não mudam.
use crate::{
    collection,
    media::{self, EncodedFrame, PreparedMedia},
};
use anyhow::{Result, ensure};
use image::{
    DynamicImage, ImageReader, RgbImage,
    imageops::{self, FilterType},
};
use std::io::Cursor;

pub(super) fn collection(input: &[&PreparedMedia], degrees: u16) -> Result<Vec<PreparedMedia>> {
    ensure!([0, 90, 180, 270].contains(&degrees), "Rotação inválida.");
    collection::validate_batch(input)?;
    let relative = super::INFO.display_rotation(degrees);
    let mut spare =
        collection::MAX_COLLECTION_BYTES - input.iter().map(|m| m.byte_len()).sum::<usize>();
    let mut output = Vec::with_capacity(input.len());
    for original in input {
        let budget = (original.byte_len() + spare).min(media::MAX_OUTPUT);
        ensure!(
            original
                .display_rotation
                .is_none_or(|angle| angle == relative),
            "Mídia preparada para outra orientação. Prepare novamente antes de enviar."
        );
        let rotated = if original.display_rotation == Some(relative) {
            (*original).clone()
        } else {
            rotate(
                original,
                (relative + if original.horizontal { 0 } else { 90 }) % 360,
                budget,
            )?
        };
        spare = spare + original.byte_len() - rotated.byte_len();
        output.push(rotated);
    }
    collection::validate_batch(&output.iter().collect::<Vec<_>>())?;
    Ok(output)
}

fn canvas(image: RgbImage, degrees: u16) -> RgbImage {
    let rotated = match degrees {
        90 => imageops::rotate90(&image),
        180 => imageops::rotate180(&image),
        270 => imageops::rotate270(&image),
        _ => image,
    };
    if rotated.dimensions() == (480, 640) {
        return rotated;
    }
    // Fallback para catálogos sem original. O fluxo normal refaz o recorte
    // antes de girar; aqui só existem os JPEGs antigos já enquadrados.
    let fitted = DynamicImage::ImageRgb8(rotated)
        .resize_to_fill(480, 640, FilterType::Lanczos3)
        .to_rgb8();
    let mut canvas = RgbImage::new(480, 640);
    imageops::overlay(
        &mut canvas,
        &fitted,
        i64::from((480 - fitted.width()) / 2),
        i64::from((640 - fitted.height()) / 2),
    );
    canvas
}

fn rotate(input: &PreparedMedia, degrees: u16, budget: usize) -> Result<PreparedMedia> {
    if degrees == 0 {
        return Ok(input.clone());
    }
    let mut pixels = Vec::with_capacity(input.frames.len());
    for frame in &input.frames {
        let mut reader = ImageReader::new(Cursor::new(&frame.jpeg)).with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(480);
        limits.max_image_height = Some(640);
        limits.max_alloc = Some(8 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode()?.to_rgb8();
        ensure!(
            image.dimensions() == (480, 640),
            "Quadro MYSTIQUE deve ter 480×640 pixels."
        );
        pixels.push(canvas(image, degrees));
    }
    // Nunca encadeie rotações de JPEGs já girados e nunca descarte quadros aqui.
    // O orçamento total é verificado antes de formatar/iniciar transferência.
    for quality in [95, 90, 85, 80, 75] {
        let mut bytes = 0;
        let mut frames = Vec::with_capacity(pixels.len());
        for (pixels, original) in pixels.iter().zip(&input.frames) {
            let jpeg = media::encode_jpeg(pixels, quality)?;
            bytes += jpeg.len();
            if bytes > budget {
                break;
            }
            frames.push(EncodedFrame {
                jpeg,
                preview: image::RgbaImage::new(1, 1),
                delay_ms: original.delay_ms,
            });
        }
        if frames.len() == input.frames.len() {
            let output = PreparedMedia {
                display_rotation: None,
                frames,
                adaptation: input.adaptation.clone(),
                animated: input.animated,
                horizontal: input.horizontal,
                duration_ms: input.duration_ms,
            };
            output.validate()?;
            return Ok(output);
        }
    }
    anyhow::bail!(
        "A mídia girada excede o espaço disponível. Reduza a coleção antes de aplicar a rotação."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(animated: bool) -> PreparedMedia {
        let pixels = RgbImage::from_fn(480, 640, |x, y| match (x < 240, y < 320) {
            (true, true) => image::Rgb([240, 20, 20]),
            (false, true) => image::Rgb([20, 240, 20]),
            (true, false) => image::Rgb([20, 20, 240]),
            _ => image::Rgb([240, 240, 20]),
        });
        let jpeg = media::encode_jpeg(&pixels, 95).unwrap();
        let frame = EncodedFrame {
            jpeg,
            preview: image::RgbaImage::new(1, 1),
            delay_ms: 100,
        };
        PreparedMedia {
            display_rotation: None,
            frames: if animated {
                vec![
                    frame.clone(),
                    EncodedFrame {
                        delay_ms: 200,
                        ..frame
                    },
                ]
            } else {
                vec![frame]
            },
            adaptation: None,
            animated,
            horizontal: true,
            duration_ms: if animated { 300 } else { 0 },
        }
    }
    #[test]
    fn all_angles_rotate_pixels_preserve_aspect_timing_and_canonical_jpeg() {
        for animated in [false, true] {
            let source = sample(animated);
            let canonical = source.frames[0].jpeg.clone();
            for angle in [0, 90, 180, 270] {
                let output = collection(&[&source], super::super::INFO.raw_rotation(angle))
                    .unwrap()
                    .remove(0);
                assert_eq!(output.frames.len(), source.frames.len());
                assert_eq!(output.duration_ms, source.duration_ms);
                assert_eq!(
                    output.frames.iter().map(|f| f.delay_ms).collect::<Vec<_>>(),
                    source.frames.iter().map(|f| f.delay_ms).collect::<Vec<_>>()
                );
                let pixels = image::load_from_memory(&output.frames[0].jpeg)
                    .unwrap()
                    .to_rgb8();
                assert_eq!(pixels.dimensions(), (480, 640));
                let top_left = *pixels.get_pixel(100, 220);
                match angle {
                    0 => assert!(top_left[0] > 200 && top_left[1] < 50),
                    90 => assert!(top_left[2] > 200 && top_left[0] < 50),
                    180 => assert!(top_left[0] > 200 && top_left[1] > 200),
                    270 => assert!(top_left[1] > 200 && top_left[0] < 50),
                    _ => unreachable!(),
                }
                if angle == 90 || angle == 270 {
                    assert!(pixels.get_pixel(240, 20).0.iter().any(|v| *v > 100));
                }
                assert_eq!(source.frames[0].jpeg, canonical);
            }
            assert_eq!(
                collection(&[&source], super::super::INFO.raw_rotation(0)).unwrap()[0].frames[0]
                    .jpeg,
                canonical
            );
        }
    }
    #[test]
    fn invalid_pixels_or_insufficient_budget_fail_before_upload() {
        let mut source = sample(false);
        assert!(rotate(&source, 180, 1).is_err());
        source.frames[0].jpeg = vec![0xff, 0xd8, 0xff, 0xd9];
        assert!(collection(&[&source], 180).is_err());
    }
}
