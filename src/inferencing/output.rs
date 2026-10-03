use std::{
    fs::{self, File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
};

use anyhow::{Context, bail};
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader,
    RgbaImage,
    codecs::{bmp::BmpEncoder, jpeg::JpegEncoder, png::PngEncoder, webp::WebPEncoder},
    metadata::Orientation,
};
use serde::{Deserialize, Serialize};

use super::profile;

const MAX_INPUT_BYTES: u64 = 50 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum OutputFormat {
    Original,
    Png,
    Jpeg,
    Webp,
}

#[derive(Clone, Debug)]
pub(crate) struct OutputSettings {
    pub(crate) directory: Option<PathBuf>,
    pub(crate) format: OutputFormat,
    pub(crate) suffix: String,
    pub(crate) preserve_metadata: bool,
}

#[derive(Debug)]
pub(crate) struct LoadedImage {
    pub(crate) pixels: DynamicImage,
    pub(crate) source_format: ImageFormat,
    exif: Option<Vec<u8>>,
    icc: Option<Vec<u8>>,
}

pub(crate) fn load(path: &Path) -> anyhow::Result<LoadedImage> {
    let started = std::time::Instant::now();
    let metadata = fs::metadata(path)
        .with_context(|| format!("Failed to read image information: {}", path.display()))?;
    if metadata.len() > MAX_INPUT_BYTES {
        bail!("Input image is larger than the 50 MiB limit");
    }

    let reader = ImageReader::open(path)
        .with_context(|| format!("Failed to open image: {}", path.display()))?
        .with_guessed_format()
        .context("Failed to identify the input image format")?;
    let source_format = reader.format().context("Unknown input image format")?;
    let mut decoder = reader
        .into_decoder()
        .context("Failed to create image decoder")?;
    let orientation = decoder
        .orientation()
        .context("Failed to read image orientation")?;
    let mut exif = decoder
        .exif_metadata()
        .context("Failed to read EXIF metadata")?;
    let icc = decoder
        .icc_profile()
        .context("Failed to read ICC profile")?;
    if let Some(exif) = &mut exif {
        let _ = Orientation::remove_from_exif_chunk(exif);
    }
    let mut pixels = DynamicImage::from_decoder(decoder).context("Failed to decode image")?;
    pixels.apply_orientation(orientation);
    profile::event(format_args!(
        "decode details: file_bytes={}, exif_bytes={}, icc_bytes={}, elapsed_ms={:.3}",
        metadata.len(),
        exif.as_ref().map_or(0, Vec::len),
        icc.as_ref().map_or(0, Vec::len),
        profile::milliseconds(started.elapsed()),
    ));

    Ok(LoadedImage {
        pixels,
        source_format,
        exif,
        icc,
    })
}

pub(crate) fn output_path(
    source: &Path,
    source_format: ImageFormat,
    settings: &OutputSettings,
) -> anyhow::Result<PathBuf> {
    let directory = settings
        .directory
        .clone()
        .or_else(|| source.parent().map(Path::to_path_buf))
        .context("Input image has no parent directory")?;
    if !directory.is_dir() {
        bail!("Output directory is unavailable: {}", directory.display());
    }
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("Input image has no valid file name")?;
    let extension = extension(settings.format, source_format)?;
    unique_path(&directory, stem, &safe_suffix(&settings.suffix), extension)
}

pub(crate) fn save(
    path: &Path,
    pixels: RgbaImage,
    source: &LoadedImage,
    settings: &OutputSettings,
) -> anyhow::Result<()> {
    let total_started = std::time::Instant::now();
    let format = resolved_format(settings.format, source.source_format)?;
    let (part_path, file) = create_partial(path)?;
    profile::event(format_args!(
        "encode start: format={format:?}, dimensions={}x{}, preserve_metadata={}, temporary={}",
        pixels.width(),
        pixels.height(),
        settings.preserve_metadata,
        part_path.display(),
    ));
    let encode_started = std::time::Instant::now();
    let result = encode(
        BufWriter::new(file),
        pixels,
        format,
        settings.preserve_metadata.then_some(source),
    )
    .with_context(|| format!("Failed to encode image as {format:?}"));
    if let Err(error) = result {
        let _ = fs::remove_file(&part_path);
        return Err(error);
    }
    profile::event(format_args!(
        "encode complete: elapsed_ms={:.3}",
        profile::milliseconds(encode_started.elapsed()),
    ));
    let finalize_started = std::time::Instant::now();
    fs::rename(&part_path, path).with_context(|| {
        let _ = fs::remove_file(&part_path);
        format!("Failed to finalize output: {}", path.display())
    })?;
    profile::event(format_args!(
        "output finalized: rename_ms={:.3}, save_total_ms={:.3}",
        profile::milliseconds(finalize_started.elapsed()),
        profile::milliseconds(total_started.elapsed()),
    ));
    Ok(())
}

fn encode(
    mut writer: BufWriter<File>,
    pixels: RgbaImage,
    format: ImageFormat,
    metadata: Option<&LoadedImage>,
) -> anyhow::Result<()> {
    let (width, height) = pixels.dimensions();
    match format {
        ImageFormat::Png => {
            let mut encoder = PngEncoder::new(writer);
            apply_metadata(&mut encoder, metadata);
            encoder.write_image(&pixels, width, height, ExtendedColorType::Rgba8)?;
        }
        ImageFormat::Jpeg => {
            let rgb = flatten_onto_white(&pixels);
            let mut encoder = JpegEncoder::new_with_quality(writer, 92);
            apply_metadata(&mut encoder, metadata);
            encoder.write_image(&rgb, width, height, ExtendedColorType::Rgb8)?;
        }
        ImageFormat::WebP => {
            let mut encoder = WebPEncoder::new_lossless(writer);
            apply_metadata(&mut encoder, metadata);
            encoder.write_image(&pixels, width, height, ExtendedColorType::Rgba8)?;
        }
        ImageFormat::Bmp => {
            BmpEncoder::new(&mut writer).write_image(
                &pixels,
                width,
                height,
                ExtendedColorType::Rgba8,
            )?;
        }
        _ => bail!("Original image format {format:?} cannot be encoded"),
    }
    Ok(())
}

fn flatten_onto_white(pixels: &RgbaImage) -> image::RgbImage {
    image::RgbImage::from_fn(pixels.width(), pixels.height(), |x, y| {
        let pixel = pixels.get_pixel(x, y);
        let alpha = u16::from(pixel[3]);
        let blend =
            |channel: u8| ((u16::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        image::Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])])
    })
}

fn apply_metadata(encoder: &mut impl ImageEncoder, metadata: Option<&LoadedImage>) {
    let Some(metadata) = metadata else {
        return;
    };
    if let Some(icc) = &metadata.icc {
        let _ = encoder.set_icc_profile(icc.clone());
    }
    if let Some(exif) = &metadata.exif {
        let _ = encoder.set_exif_metadata(exif.clone());
    }
}

fn safe_suffix(suffix: &str) -> String {
    suffix
        .chars()
        .filter(|character| !character.is_control())
        .map(|character| match character {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            character => character,
        })
        .take(32)
        .collect()
}

fn resolved_format(selected: OutputFormat, original: ImageFormat) -> anyhow::Result<ImageFormat> {
    match selected {
        OutputFormat::Original => match original {
            ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP | ImageFormat::Bmp => {
                Ok(original)
            }
            _ => bail!("Original image format {original:?} cannot be encoded"),
        },
        OutputFormat::Png => Ok(ImageFormat::Png),
        OutputFormat::Jpeg => Ok(ImageFormat::Jpeg),
        OutputFormat::Webp => Ok(ImageFormat::WebP),
    }
}

fn extension(selected: OutputFormat, original: ImageFormat) -> anyhow::Result<&'static str> {
    Ok(match resolved_format(selected, original)? {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::WebP => "webp",
        ImageFormat::Bmp => "bmp",
        format => bail!("No output extension for {format:?}"),
    })
}

fn unique_path(
    directory: &Path,
    stem: &str,
    suffix: &str,
    extension: &str,
) -> anyhow::Result<PathBuf> {
    for index in 1..10_000 {
        let discriminator = if index == 1 {
            String::new()
        } else {
            format!("-{index}")
        };
        let path = directory.join(format!("{stem}{suffix}{discriminator}.{extension}"));
        if !path.exists() {
            return Ok(path);
        }
    }
    bail!("Could not choose a unique output file name")
}

fn create_partial(path: &Path) -> anyhow::Result<(PathBuf, File)> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("Output path has no valid file name")?;
    for index in 1..1000 {
        let candidate = path.with_file_name(format!(".{file_name}.{index}.part"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error).context("Failed to create temporary output file"),
        }
    }
    bail!("Could not create a temporary output file")
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::GenericImageView;

    #[test]
    fn output_path_never_overwrites_existing_files() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("photo.jpeg");
        fs::write(&source, b"source").unwrap();
        fs::write(directory.path().join("photo_upscaled.jpg"), b"existing").unwrap();
        let settings = OutputSettings {
            directory: None,
            format: OutputFormat::Jpeg,
            suffix: "_upscaled".into(),
            preserve_metadata: false,
        };

        assert_eq!(
            output_path(&source, ImageFormat::Jpeg, &settings).unwrap(),
            directory.path().join("photo_upscaled-2.jpg")
        );
    }

    #[test]
    fn selected_formats_have_canonical_extensions() {
        assert_eq!(
            extension(OutputFormat::Original, ImageFormat::Jpeg).unwrap(),
            "jpg"
        );
        assert_eq!(
            extension(OutputFormat::Png, ImageFormat::Jpeg).unwrap(),
            "png"
        );
        assert_eq!(
            extension(OutputFormat::Webp, ImageFormat::Jpeg).unwrap(),
            "webp"
        );
    }

    #[test]
    fn unsafe_suffix_characters_cannot_create_subdirectories() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("photo.png");
        fs::write(&source, b"source").unwrap();
        let settings = OutputSettings {
            directory: None,
            format: OutputFormat::Png,
            suffix: "../../bad:name".into(),
            preserve_metadata: false,
        };

        assert_eq!(
            output_path(&source, ImageFormat::Png, &settings).unwrap(),
            directory.path().join("photo.._.._bad_name.png")
        );
    }

    #[test]
    fn jpeg_flattens_transparency_onto_white() {
        let pixels = RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
        assert_eq!(
            flatten_onto_white(&pixels).get_pixel(0, 0).0,
            [255, 255, 255]
        );
    }

    #[test]
    fn selected_formats_are_encoded_and_decodable() {
        for (selected, expected) in [
            (OutputFormat::Png, ImageFormat::Png),
            (OutputFormat::Jpeg, ImageFormat::Jpeg),
            (OutputFormat::Webp, ImageFormat::WebP),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let source_path = directory.path().join("source.png");
            let source = LoadedImage {
                pixels: DynamicImage::new_rgba8(2, 3),
                source_format: ImageFormat::Png,
                exif: None,
                icc: None,
            };
            let settings = OutputSettings {
                directory: None,
                format: selected,
                suffix: "_result".into(),
                preserve_metadata: true,
            };
            let destination = output_path(&source_path, source.source_format, &settings).unwrap();
            save(
                &destination,
                RgbaImage::from_pixel(2, 3, image::Rgba([10, 20, 30, 128])),
                &source,
                &settings,
            )
            .unwrap();

            let reader = ImageReader::open(destination)
                .unwrap()
                .with_guessed_format()
                .unwrap();
            assert_eq!(reader.format(), Some(expected));
            assert_eq!(reader.decode().unwrap().dimensions(), (2, 3));
        }
    }
}
