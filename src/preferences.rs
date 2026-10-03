use std::{fs, path::PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::inferencing::{InferenceEngine, OutputFormat};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub(crate) struct Preferences {
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) output_format: OutputFormat,
    pub(crate) preserve_metadata: bool,
    pub(crate) filename_suffix: String,
    pub(crate) sidebar_right: bool,
    pub(crate) inference_engine: Option<InferenceEngine>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            output_dir: None,
            output_format: OutputFormat::Original,
            preserve_metadata: true,
            filename_suffix: "_upscaled".to_string(),
            sidebar_right: true,
            inference_engine: None,
        }
    }
}

impl Preferences {
    pub(crate) fn load() -> Self {
        let Some(path) = path() else {
            return Self::default();
        };
        fs::read(path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self) -> anyhow::Result<()> {
        let path = path().ok_or_else(|| anyhow::anyhow!("Preferences directory is unavailable"))?;
        let parent = path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Preferences path has no parent"))?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension("json.tmp");
        fs::write(&temporary, serde_json::to_vec_pretty(self)?)?;
        #[cfg(target_os = "windows")]
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(temporary, path)?;
        Ok(())
    }
}

fn path() -> Option<PathBuf> {
    ProjectDirs::from("com", "SuperPuper", env!("CARGO_PKG_NAME"))
        .map(|directories| directories.config_dir().join("preferences.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_defaults() {
        let preferences: Preferences = serde_json::from_str(r#"{"sidebar_right":false}"#).unwrap();
        assert!(!preferences.sidebar_right);
        assert_eq!(preferences.output_format, OutputFormat::Original);
        assert!(preferences.preserve_metadata);
    }
}
