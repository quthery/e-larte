use std::path::PathBuf;

use gpui::{Context, FocusHandle};

use crate::{
    localization::Language,
    models::{self, Model},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    Original,
    Png,
    Jpeg,
    Webp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DenoiseStrength {
    Low,
    Medium,
    High,
}

pub(crate) struct ImageScaler {
    pub(crate) image_path: Option<PathBuf>,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) selected_model: Option<usize>,
    pub(crate) model_menu_open: bool,
    pub(crate) scale: u8,
    pub(crate) sidebar_right: bool,
    pub(crate) comparison_position: f32,
    pub(crate) comparison_dragging: bool,
    pub(crate) comparison_ready: bool,
    pub(crate) language: Language,
    pub(crate) output_format: OutputFormat,
    pub(crate) preserve_metadata: bool,
    pub(crate) denoise_strength: DenoiseStrength,
    pub(crate) filename_suffix: String,
    pub(crate) suffix_focus: FocusHandle,
    pub(crate) models: Vec<Model>,
}

impl ImageScaler {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        let models = models::available();
        Self {
            image_path: None,
            output_dir: None,
            selected_model: (!models.is_empty()).then_some(0),
            model_menu_open: false,
            scale: 4,
            sidebar_right: true,
            comparison_position: 0.5,
            comparison_dragging: false,
            comparison_ready: false,
            language: Language::system(),
            output_format: OutputFormat::Original,
            preserve_metadata: true,
            denoise_strength: DenoiseStrength::Medium,
            filename_suffix: "_upscaled".to_string(),
            suffix_focus: cx.focus_handle(),
            models,
        }
    }
}
