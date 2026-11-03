//! Conversão de entrada: enquadramento, adaptação temporal e codificação para o display.
mod adapt;
mod framing;
mod source;
pub use framing::Framing;
pub use source::source_preview;

use anyhow::{Context, Result, ensure};
use image::{
    AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, ImageReader, RgbaImage,
    codecs::gif::GifDecoder,
    imageops::{self, FilterType},
};
use jpeg_encoder::{ColorType, Encoder, SamplingFactor};
use std::{
    fs::File,
    io::{BufReader, Cursor},
    path::Path,
};

pub const MAX_FRAMES: usize = 120;
const MAX_INPUT: u64 = 50 * 1024 * 1024;
pub const MAX_OUTPUT: usize = 24 * 1024 * 1024;
const PIXEL_BUDGET: u64 = 1_000_000_000;

#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    clap::ValueEnum,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Orientation {
    #[default]
    Auto,
    Horizontal,
    Vertical,
}
#[derive(
    Clone,
    Copy,
    PartialEq,
    Eq,
    Debug,
    clap::ValueEnum,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum Fit {
    #[default]
    Cover,
    Contain,
}

#[derive(Clone)]
pub struct EncodedFrame {
    pub jpeg: Vec<u8>,
    pub preview: RgbaImage,
    pub delay_ms: u16,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Adaptation {
    pub source_frames: usize,
    pub output_frames: usize,
    pub jpeg_quality: u8,
}
impl Adaptation {
    pub fn changed(&self) -> bool {
        self.source_frames != self.output_frames || self.jpeg_quality != 90
    }
    pub fn description(&self) -> String {
        format!(
            "Adaptação automática: {} → {} quadros · JPEG {} · duração preservada",
            self.source_frames, self.output_frames, self.jpeg_quality
        )
    }
}
#[derive(Clone, Copy)]
pub struct PrepareOptions {
    pub orientation: Orientation,
    pub fit: Fit,
    pub framing: Framing,
    pub max_bytes: usize,
    /// Giro horário a partir da posição horizontal de referência. None mantém
    /// o contrato canônico antigo, usado no cache e por módulos nativos.
    pub display_rotation: Option<u16>,
}
impl PrepareOptions {
    pub fn new(orientation: Orientation, fit: Fit) -> Self {
        Self {
            orientation,
            fit,
            framing: Framing::default(),
            max_bytes: MAX_OUTPUT,
            display_rotation: None,
        }
    }
}

#[derive(Clone)]
pub struct PreparedMedia {
    /// Pixels já preparados para este giro relativo; evita uma segunda rotação.
    pub display_rotation: Option<u16>,
    pub adaptation: Option<Adaptation>,
    pub frames: Vec<EncodedFrame>,
    pub animated: bool,
    pub horizontal: bool,
    pub duration_ms: u16,
}

impl PreparedMedia {
    pub fn byte_len(&self) -> usize {
        self.frames.iter().map(|frame| frame.jpeg.len()).sum()
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.display_rotation
                .is_none_or(|angle| [0, 90, 180, 270].contains(&angle)),
            "Orientação da mídia inválida."
        );
        ensure!(
            !self.frames.is_empty() && self.frames.len() <= MAX_FRAMES,
            "Quantidade de quadros inválida."
        );
        ensure!(
            self.animated || self.frames.len() == 1,
            "Imagem estática deve ter apenas um quadro."
        );
        ensure!(
            !self.animated || self.duration_ms > 0,
            "Duração de GIF inválida."
        );
        ensure!(
            self.byte_len() <= MAX_OUTPUT,
            "Mídia convertida excede 24 MiB."
        );
        for frame in &self.frames {
            ensure!(
                frame.jpeg.starts_with(&[0xff, 0xd8, 0xff]) && frame.jpeg.ends_with(&[0xff, 0xd9]),
                "Quadro JPEG inválido."
            );
        }
        Ok(())
    }
}

fn limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(128 * 1024 * 1024);
    limits
}

pub fn prepare(path: &Path, orientation: Orientation, fit: Fit) -> Result<PreparedMedia> {
    prepare_with(path, PrepareOptions::new(orientation, fit))
}

pub fn prepare_with(path: &Path, options: PrepareOptions) -> Result<PreparedMedia> {
    options.framing.validate()?;
    ensure!(
        options
            .display_rotation
            .is_none_or(|angle| [0, 90, 180, 270].contains(&angle)),
        "Rotação de preparo inválida."
    );
    ensure!(
        options.max_bytes > 0 && options.max_bytes <= MAX_OUTPUT,
        "Limite de conversão inválido."
    );
    ensure!(
        path.metadata()
            .context("Não foi possível ler o arquivo")?
            .len()
            <= MAX_INPUT,
        "Arquivo excede 50 MiB."
    );
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format().context("Formato de imagem desconhecido")?;
    ensure!(
        matches!(
            format,
            ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::Gif | ImageFormat::WebP
        ),
        "Use PNG, JPEG, WebP ou GIF."
    );
    let mut rasters = Vec::new();
    let horizontal;
    let duration;
    let source_frames;
    if format == ImageFormat::Gif {
        let mut metadata = gif::DecodeOptions::new();
        metadata.skip_frame_decoding(true);
        let mut metadata = metadata.read_info(BufReader::new(File::open(path)?))?;
        ensure!(
            metadata.width() <= 4096 && metadata.height() <= 4096,
            "GIF excede 4096 pixels por lado."
        );
        let mut delays = Vec::new();
        let mut total = 0u32;
        while let Some(frame) = metadata.read_next_frame()? {
            let delay = (u32::from(frame.delay) * 10).clamp(20, 10000) as u16;
            total += u32::from(delay);
            ensure!(
                total <= 65535,
                "O GIF excede a duração de 65,535 segundos aceita pelo display. Use uma animação mais curta."
            );
            delays.push(delay);
        }
        ensure!(!delays.is_empty(), "O GIF não contém quadros.");
        source_frames = delays.len();
        duration = total as u16;
        ensure!(
            u64::from(metadata.width()) * u64::from(metadata.height()) * source_frames as u64
                <= PIXEL_BUDGET,
            "GIF muito complexo para conversão local. Reduza a resolução original."
        );
        let selection = adapt::plan(&delays, MAX_FRAMES);
        let mut wanted = selection.iter().peekable();
        let mut decoder = GifDecoder::new(BufReader::new(File::open(path)?))?;
        decoder.set_limits(limits())?;
        let dimensions = decoder.dimensions();
        horizontal = if let Some(rotation) = options.display_rotation {
            rotation.is_multiple_of(180)
        } else {
            match options.orientation {
                Orientation::Auto => dimensions.0 > dimensions.1,
                Orientation::Horizontal => true,
                Orientation::Vertical => false,
            }
        };
        let mut seen = 0;
        for (index, frame) in decoder.into_frames().enumerate() {
            let frame = frame?;
            seen += 1;
            if let Some(&&(target, delay)) = wanted.peek()
                && index == target
            {
                rasters.push(adapt::RasterFrame {
                    pixels: raster(
                        DynamicImage::ImageRgba8(frame.into_buffer()),
                        horizontal,
                        options.fit,
                        options.framing,
                        options.display_rotation,
                    )?,
                    delay,
                });
                wanted.next();
            }
        }
        ensure!(
            seen == source_frames && wanted.next().is_none(),
            "O GIF mudou durante a conversão. Adicione-o novamente."
        );
    } else {
        let mut reader = reader;
        reader.limits(limits());
        let mut decoder = reader.into_decoder()?;
        let orient = decoder.orientation()?;
        let mut image = DynamicImage::from_decoder(decoder)?;
        image.apply_orientation(orient);
        horizontal = options.display_rotation.map_or_else(
            || horizontal_image(&image, options.orientation),
            |rotation| rotation.is_multiple_of(180),
        );
        source_frames = 1;
        duration = 0;
        rasters.push(adapt::RasterFrame {
            pixels: raster(
                image,
                horizontal,
                options.fit,
                options.framing,
                options.display_rotation,
            )?,
            delay: 100,
        });
    }
    let mut output = adapt::optimize(
        rasters,
        if options.display_rotation.is_some() {
            true
        } else {
            horizontal
        },
        duration,
        source_frames,
        options.max_bytes,
    )?;
    output.display_rotation = options.display_rotation;
    output.validate()?;
    Ok(output)
}

fn horizontal_image(image: &DynamicImage, orientation: Orientation) -> bool {
    match orientation {
        Orientation::Auto => image.width() > image.height(),
        Orientation::Horizontal => true,
        Orientation::Vertical => false,
    }
}

fn raster(
    mut image: DynamicImage,
    horizontal: bool,
    fit: Fit,
    framing: Framing,
    display_rotation: Option<u16>,
) -> Result<image::RgbImage> {
    let (width, height) = if horizontal { (640, 480) } else { (480, 640) };
    if fit == Fit::Cover {
        let (x, y, w, h) =
            framing.rect(image.width(), image.height(), width as f32 / height as f32);
        image = image.crop_imm(x, y, w, h);
    }
    // Transparência é composta sobre preto antes do redimensionamento.
    let mut rgb = image::RgbImage::new(image.width(), image.height());
    for (x, y, rgba) in image.to_rgba8().enumerate_pixels() {
        let alpha = u16::from(rgba[3]);
        rgb.put_pixel(
            x,
            y,
            image::Rgb([
                (u16::from(rgba[0]) * alpha / 255) as u8,
                (u16::from(rgba[1]) * alpha / 255) as u8,
                (u16::from(rgba[2]) * alpha / 255) as u8,
            ]),
        );
    }
    let image = DynamicImage::ImageRgb8(rgb);
    let view = match fit {
        Fit::Cover => image
            .resize_to_fill(width, height, FilterType::Lanczos3)
            .to_rgb8(),
        Fit::Contain => {
            let small = image.resize(width, height, FilterType::Lanczos3).to_rgb8();
            let mut canvas = image::RgbImage::new(width, height);
            imageops::overlay(
                &mut canvas,
                &small,
                i64::from((width - small.width()) / 2),
                i64::from((height - small.height()) / 2),
            );
            canvas
        }
    };
    let device = if let Some(rotation) = display_rotation {
        let physical = match rotation {
            90 => imageops::rotate90(&view),
            180 => imageops::rotate180(&view),
            270 => imageops::rotate270(&view),
            _ => view,
        };
        imageops::rotate90(&physical)
    } else if horizontal {
        imageops::rotate90(&view)
    } else {
        view
    };
    Ok(device)
}

pub(crate) fn encode_jpeg(device: &image::RgbImage, quality: u8) -> Result<Vec<u8>> {
    let mut jpeg = vec![];
    let mut encoder = Encoder::new(&mut jpeg, quality);
    encoder.set_sampling_factor(SamplingFactor::F_2_2);
    encoder.encode(device.as_raw(), 480, 640, ColorType::Rgb)?;
    Ok(jpeg)
}

fn preview(jpeg: &[u8], horizontal: bool) -> Result<RgbaImage> {
    let decoded = ImageReader::new(Cursor::new(jpeg))
        .with_guessed_format()?
        .decode()?
        .to_rgba8();
    let upright = if horizontal {
        imageops::rotate270(&decoded)
    } else {
        decoded
    };
    Ok(DynamicImage::ImageRgba8(upright)
        .thumbnail(480, 480)
        .to_rgba8())
}
