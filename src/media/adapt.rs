//! Adaptação temporal e JPEG. Nunca remove o final da animação nem altera sua duração.
use super::{Adaptation, EncodedFrame, MAX_FRAMES, PreparedMedia, encode_jpeg, preview};
use anyhow::{Result, ensure};
use image::RgbImage;

pub(super) struct RasterFrame {
    pub pixels: RgbImage,
    pub delay: u16,
}

/// Amostra pela linha do tempo, incluindo início e fim; acumula tempos descartados.
pub(super) fn plan(delays: &[u16], limit: usize) -> Vec<(usize, u16)> {
    if delays.len() <= limit {
        return delays.iter().enumerate().map(|(i, &d)| (i, d)).collect();
    }
    let mut total = 0u32;
    let starts: Vec<_> = delays
        .iter()
        .map(|&delay| {
            let start = total;
            total += u32::from(delay);
            start
        })
        .collect();
    let mut indices: Vec<_> = (0..limit)
        .map(|slot| {
            let time = (total - 1) * slot as u32 / (limit - 1) as u32;
            starts
                .partition_point(|&start| start <= time)
                .saturating_sub(1)
        })
        .collect();
    indices.dedup();
    indices
        .iter()
        .enumerate()
        .map(|(i, &index)| {
            let end = indices.get(i + 1).map_or(total, |&next| starts[next]);
            (index, (end - starts[index]) as u16)
        })
        .collect()
}

pub(super) fn optimize(
    rasters: Vec<RasterFrame>,
    horizontal: bool,
    duration: u16,
    source_frames: usize,
    budget: usize,
) -> Result<PreparedMedia> {
    let delays: Vec<_> = rasters.iter().map(|frame| frame.delay).collect();
    let mut limit = rasters.len().min(MAX_FRAMES);
    loop {
        let selection = plan(&delays, limit);
        for quality in [90, 85, 80, 75] {
            let mut bytes = 0;
            let mut jpegs = Vec::new();
            for &(index, _) in &selection {
                let jpeg = encode_jpeg(&rasters[index].pixels, quality)?;
                bytes += jpeg.len();
                if bytes > budget {
                    break;
                }
                jpegs.push(jpeg);
            }
            if jpegs.len() == selection.len() {
                let mut frames = Vec::new();
                for (jpeg, (_, delay_ms)) in jpegs.into_iter().zip(selection) {
                    frames.push(EncodedFrame {
                        preview: preview(&jpeg, horizontal)?,
                        jpeg,
                        delay_ms,
                    });
                }
                let animated = frames.len() > 1;
                return Ok(PreparedMedia {
                    display_rotation: None,
                    adaptation: Some(Adaptation {
                        source_frames,
                        output_frames: frames.len(),
                        jpeg_quality: quality,
                    }),
                    frames,
                    horizontal,
                    animated,
                    duration_ms: if animated { duration } else { 0 },
                });
            }
        }
        ensure!(
            limit > 2,
            "Não foi possível adaptar a mídia ao espaço disponível. Remova uma mídia antes de adicionar esta."
        );
        limit = (limit * 3 / 4).max(2);
    }
}

#[cfg(test)]
mod tests {
    use super::plan;
    #[test]
    fn temporal_sampling_keeps_first_last_and_variable_total_duration() {
        let mut delays = vec![20; 250];
        delays[50] = 2000;
        let plan = plan(&delays, 120);
        assert_eq!(plan.first().unwrap().0, 0);
        assert_eq!(plan.last().unwrap().0, 249);
        assert!(plan.len() <= 120);
        assert_eq!(plan.iter().map(|(_, d)| u32::from(*d)).sum::<u32>(), 6980);
        assert!(plan.iter().any(|(_, delay)| *delay >= 2000));
    }
}

#[cfg(test)]
mod quality_tests {
    use super::*;
    #[test]
    fn jpeg_quality_is_reduced_before_discarding_frames_to_meet_a_byte_budget() {
        let mut seed = 1u32;
        let raster = RgbImage::from_fn(480, 640, |_, _| {
            let mut pixel = [0; 3];
            for value in &mut pixel {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *value = (seed >> 24) as u8;
            }
            image::Rgb(pixel)
        });
        let full = encode_jpeg(&raster, 90).unwrap().len() * 4;
        let frames = (0..4)
            .map(|_| RasterFrame {
                pixels: raster.clone(),
                delay: 100,
            })
            .collect();
        let converted = optimize(frames, false, 400, 4, full * 9 / 10).unwrap();
        assert_eq!(converted.frames.len(), 4);
        assert_eq!(converted.duration_ms, 400);
        assert!(converted.byte_len() <= full * 9 / 10);
        assert!(converted.adaptation.unwrap().jpeg_quality < 90);
    }
}
