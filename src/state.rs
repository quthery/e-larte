use std::path::PathBuf;

use gpui::{Context, FocusHandle, Pixels, Point, ScrollHandle, point, px};

use crate::{
    localization::Language,
    models::{ImportedModel, MODELS, RemoteModelFile},
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

#[derive(Clone, Debug)]
pub(crate) enum ModelBrowserState {
    Idle,
    Loading,
    Error(String),
}

pub(crate) struct ImageScaler {
    pub(crate) image_path: Option<PathBuf>,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) selected_model: usize,
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
    pub(crate) imported_models: Vec<ImportedModel>,
    pub(crate) model_import_open: bool,
    pub(crate) model_import_offset: Point<Pixels>,
    pub(crate) model_import_drag_origin: Option<Point<Pixels>>,
    pub(crate) model_import_drag_start: Point<Pixels>,
    pub(crate) model_search: String,
    pub(crate) model_search_focus: FocusHandle,
    pub(crate) remote_models: Vec<RemoteModelFile>,
    pub(crate) remote_models_next: Option<String>,
    pub(crate) remote_models_state: ModelBrowserState,
    pub(crate) model_search_generation: u64,
    pub(crate) model_list_scroll: ScrollHandle,
    pub(crate) downloading_model: Option<String>,
    pub(crate) model_import_notice: Option<(bool, String)>,
}

impl ImageScaler {
    pub(crate) fn new(cx: &mut Context<Self>) -> Self {
        Self {
            image_path: None,
            output_dir: None,
            selected_model: 0,
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
            imported_models: Vec::new(),
            model_import_open: false,
            model_import_offset: point(px(0.), px(0.)),
            model_import_drag_origin: None,
            model_import_drag_start: point(px(0.), px(0.)),
            model_search: String::new(),
            model_search_focus: cx.focus_handle(),
            remote_models: Vec::new(),
            remote_models_next: None,
            remote_models_state: ModelBrowserState::Idle,
            model_search_generation: 0,
            model_list_scroll: ScrollHandle::new(),
            downloading_model: None,
            model_import_notice: None,
        }
    }

    pub(crate) fn selected_model_name(&self) -> String {
        if self.selected_model < MODELS.len() {
            MODELS[self.selected_model].0.to_string()
        } else {
            self.imported_models
                .get(self.selected_model - MODELS.len())
                .map(|model| model.name.clone())
                .unwrap_or_else(|| MODELS[0].0.to_string())
        }
    }

    pub(crate) fn selected_model_hint(&self) -> String {
        if self.selected_model < MODELS.len() {
            self.language
                .text(MODELS[self.selected_model].1)
                .to_string()
        } else {
            self.imported_models
                .get(self.selected_model - MODELS.len())
                .map(|model| model.path.display().to_string())
                .unwrap_or_default()
        }
    }
}
