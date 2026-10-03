use std::fmt;

use serde::{Deserialize, Serialize};

use super::ncnn;
use crate::models::Model;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum InferenceEngine {
    Ncnn,
    Onnx,
}

impl fmt::Display for InferenceEngine {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Ncnn => "Vulkan (NCNN)",
            Self::Onnx => "ONNX Runtime",
        })
    }
}

impl InferenceEngine {
    pub(crate) fn available_for(model: Option<&Model>) -> Vec<Self> {
        let mut engines = Vec::with_capacity(2);
        let ncnn_model = model.and_then(|model| model.ncnn);
        let has_ncnn = ncnn_model.is_some_and(ncnn::has_model);
        if has_ncnn && ncnn_model.is_some_and(|model| model.preferred) {
            engines.push(Self::Ncnn);
        }
        if model.is_none_or(|model| {
            model
                .file_path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("onnx"))
        }) {
            engines.push(Self::Onnx);
        }
        if has_ncnn && ncnn_model.is_some_and(|model| !model.preferred) {
            engines.push(Self::Ncnn);
        }
        engines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onnx_is_always_available() {
        assert!(InferenceEngine::available_for(None).contains(&InferenceEngine::Onnx));
    }
}
