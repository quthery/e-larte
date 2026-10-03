use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct Model {
    pub(crate) name: String,
    pub(crate) file_path: PathBuf,
    pub(crate) ncnn: Option<NcnnModel>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NcnnModel {
    pub(crate) argument: &'static str,
    pub(crate) scale: u32,
    pub(crate) tile_size: u32,
    pub(crate) preferred: bool,
}

pub(crate) fn available() -> Vec<Model> {
    crate::inferencing::ncnn_models()
        .into_iter()
        .chain(
            crate::model_storage::imported_models()
                .into_iter()
                .map(|path| Model {
                    name: path
                        .file_stem()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Imported model")
                        .to_string(),
                    ncnn: ncnn_model(&path),
                    file_path: path,
                }),
        )
        .collect()
}

fn ncnn_model(path: &std::path::Path) -> Option<NcnnModel> {
    let stem = path.file_stem()?.to_str()?;
    let stem = stem
        .rsplit_once('-')
        .filter(|(_, suffix)| suffix.chars().all(|character| character.is_ascii_digit()))
        .map_or(stem, |(stem, _)| stem);
    let normalized = stem
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect::<String>();

    match normalized.as_str() {
        "realesrganx4plus" => Some(NcnnModel {
            argument: "realesrgan-x4plus",
            scale: 4,
            tile_size: 0,
            preferred: true,
        }),
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteModelFile {
    pub(crate) repo_id: String,
    pub(crate) file_name: String,
    pub(crate) downloads: u64,
}

impl RemoteModelFile {
    pub(crate) fn key(&self) -> String {
        format!("{}/{}", self.repo_id, self.file_name)
    }

    pub(crate) fn display_name(&self) -> &str {
        self.file_name.rsplit('/').next().unwrap_or(&self.file_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn maps_only_the_compatible_realesrgan_model_to_ncnn() {
        assert_eq!(
            ncnn_model(Path::new("Real-ESRGAN-x4plus.onnx")),
            Some(NcnnModel {
                argument: "realesrgan-x4plus",
                scale: 4,
                tile_size: 0,
                preferred: true,
            })
        );
        assert!(ncnn_model(Path::new("Real-ESRGAN-x4plus-2.onnx")).is_some());
        assert!(ncnn_model(Path::new("Real_HAT_GAN_SRx4.onnx")).is_none());
        assert!(ncnn_model(Path::new("Real-ESRGAN_x2plus.onnx")).is_none());
    }
}
