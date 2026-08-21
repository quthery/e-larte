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
