use std::{
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::Duration,
};

use anyhow::{Context, bail};
use image::{DynamicImage, ImageFormat, RgbaImage};

use super::{Cancellation, profile};
use crate::models::NcnnModel;

struct Installation {
    binary: PathBuf,
    models: PathBuf,
}

pub(crate) fn has_model(model: NcnnModel) -> bool {
    installation().is_some_and(|installation| model_files_exist(&installation.models, model))
}

fn model_files_exist(directory: &Path, model: NcnnModel) -> bool {
    let name = model_file_stem(model);
    directory.join(format!("{name}.param")).is_file()
        && directory.join(format!("{name}.bin")).is_file()
}

fn model_file_stem(model: NcnnModel) -> String {
    if model.argument == "realesr-animevideov3" {
        format!("{}-x{}", model.argument, model.scale)
    } else {
        model.argument.to_string()
    }
}

pub(crate) fn installed_models() -> Vec<crate::models::Model> {
    let Some(installation) = installation() else {
        return Vec::new();
    };
    models_in(&installation.models)
}

fn models_in(directory: &Path) -> Vec<crate::models::Model> {
    [
        "realesrgan-x4plus",
        "realesrgan-x4plus-anime",
        "realesrnet-x4plus",
    ]
    .into_iter()
    .map(|argument| NcnnModel {
        argument,
        scale: 4,
        tile_size: 0,
        preferred: true,
    })
    .chain((2..=4).map(|scale| NcnnModel {
        argument: "realesr-animevideov3",
        scale,
        tile_size: 0,
        preferred: true,
    }))
    .filter_map(|model| {
        let name = model_file_stem(model);
        model_files_exist(directory, model).then(|| crate::models::Model {
            name: format!("{name} · Vulkan"),
            file_path: directory.join(format!("{name}.param")),
            ncnn: Some(model),
        })
    })
    .collect()
}

pub(crate) fn upscale(
    source: &DynamicImage,
    model: NcnnModel,
    cancellation: &Arc<Cancellation>,
) -> anyhow::Result<RgbaImage> {
    if cancellation.is_cancelled() {
        bail!("Upscaling cancelled");
    }
    let installation = installation().context("NCNN runtime is not installed")?;
    if !model_files_exist(&installation.models, model) {
        bail!("NCNN model files are missing for {}", model.argument);
    }

    let temporary = tempfile::Builder::new()
        .prefix("super-puper-ncnn-")
        .tempdir()
        .context("Failed to create NCNN working directory")?;
    let input = temporary.path().join("input.png");
    let output = temporary.path().join("output.png");
    source
        .save_with_format(&input, ImageFormat::Png)
        .context("Failed to prepare the image for NCNN")?;

    eprintln!(
        "[upscale] Active backend: NCNN, accelerator=Vulkan GPU (automatic), tile={}x{}, model={}",
        model.tile_size, model.tile_size, model.argument
    );
    profile::event(format_args!(
        "NCNN process start: binary={}, model={}, scale={}x, tile={}x{}",
        installation.binary.display(),
        model.argument,
        model.scale,
        model.tile_size,
        model.tile_size,
    ));

    let mut child = Command::new(&installation.binary)
        .arg("-i")
        .arg(&input)
        .arg("-o")
        .arg(&output)
        .arg("-m")
        .arg(&installation.models)
        .arg("-n")
        .arg(model.argument)
        .arg("-s")
        .arg(model.scale.to_string())
        .arg("-t")
        .arg(model.tile_size.to_string())
        .arg("-v")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start NCNN: {}", installation.binary.display()))?;
    let stdout = read_in_background("stdout", child.stdout.take());
    let stderr = read_in_background("stderr", child.stderr.take());

    let status = loop {
        if cancellation.is_cancelled() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            bail!("Upscaling cancelled");
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout.join();
                let _ = stderr.join();
                return Err(error).context("Failed to monitor NCNN");
            }
        }
        thread::sleep(Duration::from_millis(20));
    };
    stdout.join().unwrap_or_default();
    let diagnostics = stderr.join().unwrap_or_default();
    if !status.success() {
        bail!("Vulkan (NCNN) exited with {status}: {}", diagnostics.trim());
    }

    let result = image::open(&output)
        .context("NCNN did not produce a readable image")?
        .to_rgba8();
    let expected = (
        source.width().saturating_mul(model.scale),
        source.height().saturating_mul(model.scale),
    );
    if result.dimensions() != expected {
        bail!(
            "NCNN produced {}x{}, expected {}x{}",
            result.width(),
            result.height(),
            expected.0,
            expected.1
        );
    }
    Ok(result)
}

fn read_in_background(
    stream: &'static str,
    pipe: Option<impl Read + Send + 'static>,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut diagnostics = String::new();
        if let Some(pipe) = pipe {
            let mut reader = BufReader::new(pipe);
            let mut line = Vec::new();
            loop {
                line.clear();
                let Ok(read) = reader.read_until(b'\n', &mut line) else {
                    break;
                };
                if read == 0 {
                    break;
                }
                eprint!("[ncnn {stream}] {}", String::from_utf8_lossy(&line));
                diagnostics.push_str(&String::from_utf8_lossy(&line));
                if diagnostics.len() > 8_192 {
                    let mut boundary = diagnostics.len() - 8_192;
                    while !diagnostics.is_char_boundary(boundary) {
                        boundary += 1;
                    }
                    diagnostics.drain(..boundary);
                }
                if !line.ends_with(b"\n") {
                    eprintln!();
                }
            }
        }
        diagnostics
    })
}

fn installation() -> Option<Installation> {
    let binary_override = std::env::var_os("SUPER_PUPER_NCNN_BINARY");
    let models_override = std::env::var_os("SUPER_PUPER_NCNN_MODELS");
    if binary_override.is_some() || models_override.is_some() {
        return valid_installation(binary_override?.into(), models_override?.into());
    }

    let mut roots = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        roots.push(directory.join("ncnn"));
        if let Some(contents) = directory.parent() {
            roots.push(contents.join("Resources/ncnn"));
        }
    }
    if let Some(project) =
        directories::ProjectDirs::from("com", "SuperPuper", env!("CARGO_PKG_NAME"))
    {
        roots.push(project.data_local_dir().join("runtimes/ncnn"));
    }
    roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/runtimes/ncnn"));
    if let Some(paths) = std::env::var_os("PATH") {
        roots.extend(std::env::split_paths(&paths));
    }
    find_installation(&roots)
}

fn find_installation(roots: &[PathBuf]) -> Option<Installation> {
    for root in roots {
        if let Some(installation) =
            valid_installation(root.join(binary_name()), root.join("models"))
        {
            return Some(installation);
        }
    }
    None
}

fn binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "realesrgan-ncnn-vulkan.exe"
    } else {
        "realesrgan-ncnn-vulkan"
    }
}

fn valid_installation(binary: PathBuf, models: PathBuf) -> Option<Installation> {
    if !binary.is_file() || !models.is_dir() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&binary).ok()?.permissions().mode() & 0o111 == 0 {
            return None;
        }
    }
    Some(Installation { binary, models })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_installation_is_rejected() {
        assert!(valid_installation(PathBuf::from("missing"), PathBuf::from("missing")).is_none());
    }

    #[test]
    fn output_dimensions_are_scale_based() {
        let image = DynamicImage::new_rgba8(12, 7);
        let scale = 4;
        assert_eq!((image.width() * scale, image.height() * scale), (48, 28));
    }

    #[test]
    #[ignore = "requires an installed NCNN runtime and a working Vulkan GPU"]
    fn bundled_runtime_upscales_a_small_image() {
        use crate::inferencing::{
            InferenceEngine, OutputFormat, OutputSettings, UpscaleRequest, upscale_image,
        };

        let temporary = tempfile::tempdir().unwrap();
        let image_path = temporary.path().join("input.png");
        let source = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(16, 16, |x, y| {
            image::Rgba([(x * 16) as u8, (y * 16) as u8, 128, 255])
        }));
        source.save(&image_path).unwrap();
        let catalog = installed_models();
        assert!(
            !catalog.is_empty(),
            "Install the Vulkan runtime and model weights first"
        );
        for model in catalog {
            assert_eq!(
                InferenceEngine::available_for(Some(&model)),
                vec![InferenceEngine::Ncnn]
            );
            let ncnn = model.ncnn.unwrap();
            let mut progress = Vec::new();
            let outcome = upscale_image(
                UpscaleRequest {
                    image_path: image_path.clone(),
                    model_path: model.file_path,
                    ncnn_model: Some(ncnn),
                    engine: InferenceEngine::Ncnn,
                    output: OutputSettings {
                        directory: Some(temporary.path().to_path_buf()),
                        format: OutputFormat::Png,
                        suffix: format!("_{}", model_file_stem(ncnn)),
                        preserve_metadata: true,
                    },
                },
                Arc::new(Cancellation::new().unwrap()),
                |event| progress.push(event.percent()),
            )
            .unwrap();
            assert_eq!(outcome.engine, InferenceEngine::Ncnn);
            assert_eq!(outcome.scale, ncnn.scale);
            assert_eq!(progress.last(), Some(&97));
            assert_ne!(outcome.output_path, image_path);
            let output = image::open(outcome.output_path).unwrap().to_rgba8();
            assert_eq!(output.dimensions(), (16 * ncnn.scale, 16 * ncnn.scale));
            assert!(output.pixels().any(|pixel| pixel[0] != 0));
            assert!(output.pixels().all(|pixel| pixel[3] == 255));
        }
    }

    #[test]
    fn discovers_runtime_and_requires_both_model_files() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let models = root.join("models");
        std::fs::create_dir(&models).unwrap();
        let binary = root.join(binary_name());
        std::fs::write(&binary, b"runtime").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o644)).unwrap();
            assert!(find_installation(&[root.to_path_buf()]).is_none());
            std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert!(find_installation(&[root.join("missing"), root.to_path_buf()]).is_some());
        assert!(models_in(&models).is_empty());
        std::fs::write(models.join("realesrgan-x4plus.param"), b"param").unwrap();
        assert!(models_in(&models).is_empty());
        std::fs::write(models.join("realesrgan-x4plus.bin"), b"weights").unwrap();
        std::fs::write(models.join("realesr-animevideov3-x2.param"), b"param").unwrap();
        std::fs::write(models.join("realesr-animevideov3-x2.bin"), b"weights").unwrap();
        let catalog = models_in(&models);
        assert_eq!(catalog.len(), 2);
        assert_eq!(catalog[0].ncnn.unwrap().argument, "realesrgan-x4plus");
        assert_eq!(catalog[1].ncnn.unwrap().scale, 2);
        assert_eq!(
            catalog[1].file_path,
            models.join("realesr-animevideov3-x2.param")
        );
    }
}
