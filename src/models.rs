use std::path::PathBuf;

pub(crate) const MODELS: [(&str, &str); 3] = [
    ("Real-ESRGAN", "model_photo"),
    ("AnimeSharp", "model_anime"),
    ("UltraMix", "model_details"),
];

#[derive(Clone, Debug)]
pub(crate) struct ImportedModel {
    pub(crate) name: String,
    pub(crate) path: PathBuf,
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
