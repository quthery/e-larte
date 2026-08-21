use std::path::PathBuf;

#[derive(Clone, Debug)]
pub(crate) struct Model {
    pub(crate) name: String,
    pub(crate) file_path: PathBuf,
}

pub(crate) fn available() -> Vec<Model> {
    crate::model_storage::imported_models()
        .into_iter()
        .map(|path| Model {
            name: path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("Imported model")
                .to_string(),
            file_path: path,
        })
        .collect()
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
