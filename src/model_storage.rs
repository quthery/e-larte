use std::{
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;

const SUPPORTED_EXTENSION: &str = "onnx";

pub(crate) fn models_dir() -> Result<PathBuf, String> {
    let project =
        ProjectDirs::from("com", "SuperPuper", env!("CARGO_PKG_NAME")).ok_or_else(|| {
            "the operating system did not provide an application data directory".to_string()
        })?;
    let directory = project.data_local_dir().join("models");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("could not create model directory: {error}"))?;
    Ok(directory)
}

pub(crate) fn imported_models() -> Vec<PathBuf> {
    let Ok(directory) = models_dir() else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut paths: Vec<_> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_supported(path))
        .collect();
    paths.sort();
    paths
}

pub(crate) fn import(paths: &[PathBuf]) -> Result<usize, String> {
    let directory = models_dir()?;
    let mut imported = 0;

    for source in paths
        .iter()
        .filter(|path| path.is_file() && is_supported(path))
    {
        let Some(file_name) = source.file_name() else {
            continue;
        };
        let destination = unique_destination(&directory, file_name);
        fs::copy(source, &destination)
            .map_err(|error| format!("could not import {}: {error}", source.display()))?;
        imported += 1;
    }

    Ok(imported)
}

fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(SUPPORTED_EXTENSION))
}

pub(crate) fn has_supported_model(paths: &[PathBuf]) -> bool {
    paths
        .iter()
        .any(|path| path.is_file() && is_supported(path))
}

fn unique_destination(directory: &Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let initial = directory.join(file_name);
    if !initial.exists() {
        return initial;
    }

    let source = Path::new(file_name);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("model");
    let extension = source.extension().and_then(|value| value.to_str());
    for index in 2.. {
        let candidate = match extension {
            Some(extension) => directory.join(format!("{stem}-{index}.{extension}")),
            None => directory.join(format!("{stem}-{index}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::is_supported;
    use std::path::Path;

    #[test]
    fn supports_only_onnx_models_case_insensitively() {
        assert!(is_supported(Path::new("model.onnx")));
        assert!(is_supported(Path::new("model.ONNX")));
        assert!(!is_supported(Path::new("model.pt")));
        assert!(!is_supported(Path::new("model.pth")));
        assert!(!is_supported(Path::new("model.safetensors")));
        assert!(!is_supported(Path::new("model.bin")));
        assert!(!is_supported(Path::new("model.param")));
    }
}
