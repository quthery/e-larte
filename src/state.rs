use std::{path::PathBuf, sync::Arc};

use gpui::{Bounds, Context, FocusHandle, Pixels, Point, ScrollHandle, point, px};

use crate::{
    inferencing::{Cancellation, InferenceEngine, OutputFormat},
    localization::Language,
    models::RemoteModelFile,
    models::{self, Model},
    preferences::Preferences,
};

#[derive(Clone, Debug)]
pub(crate) enum ModelBrowserState {
    Idle,
    Loading,
    Error(String),
}

pub(crate) enum UpscaleState {
    Idle,
    Running {
        job_id: u64,
        progress: u8,
        elapsed_millis: u64,
        cancellation: Arc<Cancellation>,
    },
    Error(String),
}

pub(crate) struct ImageScaler {
    pub(crate) image_path: Option<PathBuf>,
    pub(crate) output_dir: Option<PathBuf>,
    pub(crate) selected_model: Option<usize>,
    pub(crate) model_menu_open: bool,
    pub(crate) sidebar_right: bool,
    pub(crate) comparison_position: f32,
    pub(crate) comparison_bounds: Option<Bounds<Pixels>>,
    pub(crate) comparison_dragging: bool,
    pub(crate) comparison_ready: bool,
    pub(crate) upscaled_image_path: Option<PathBuf>,
    pub(crate) upscale_state: UpscaleState,
    pub(crate) language: Language,
    pub(crate) output_format: OutputFormat,
    pub(crate) preserve_metadata: bool,
    pub(crate) filename_suffix: String,
    pub(crate) detected_scale: Option<u32>,
    pub(crate) execution_engine: Option<InferenceEngine>,
    pub(crate) selected_engine: InferenceEngine,
    pub(crate) backend_menu_open: bool,
    pub(crate) next_job_id: u64,
    pub(crate) suffix_focus: FocusHandle,
    pub(crate) models: Vec<Model>,
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
        let models = models::available();
        let preferences = Preferences::load();
        let selected_model = (!models.is_empty()).then_some(0);
        let available_engines =
            InferenceEngine::available_for(selected_model.and_then(|index| models.get(index)));
        let selected_engine = preferences
            .inference_engine
            .filter(|engine| available_engines.contains(engine))
            .or_else(|| available_engines.first().copied())
            .unwrap_or(InferenceEngine::Onnx);
        Self {
            image_path: None,
            output_dir: preferences.output_dir,
            selected_model,
            model_menu_open: false,
            sidebar_right: preferences.sidebar_right,
            comparison_position: 0.5,
            comparison_bounds: None,
            comparison_dragging: false,
            comparison_ready: false,
            upscaled_image_path: None,
            upscale_state: UpscaleState::Idle,
            language: Language::system(),
            output_format: preferences.output_format,
            preserve_metadata: preferences.preserve_metadata,
            filename_suffix: preferences.filename_suffix,
            detected_scale: None,
            execution_engine: None,
            selected_engine,
            backend_menu_open: false,
            next_job_id: 1,
            suffix_focus: cx.focus_handle(),
            models,
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

    pub(crate) fn save_preferences(&self) {
        let preferences = Preferences {
            output_dir: self.output_dir.clone(),
            output_format: self.output_format,
            preserve_metadata: self.preserve_metadata,
            filename_suffix: self.filename_suffix.clone(),
            sidebar_right: self.sidebar_right,
            inference_engine: Some(self.selected_engine),
        };
        if let Err(error) = preferences.save() {
            eprintln!("Failed to save preferences: {error:#}");
        }
    }

    pub(crate) fn cancel_active_job(&mut self) {
        if let UpscaleState::Running { cancellation, .. } = &self.upscale_state {
            cancellation.cancel();
        }
        self.upscale_state = UpscaleState::Idle;
    }

    pub(crate) fn available_engines(&self) -> Vec<InferenceEngine> {
        InferenceEngine::available_for(self.selected_model.and_then(|index| self.models.get(index)))
    }

    pub(crate) fn select_model(&mut self, index: usize) {
        self.cancel_active_job();
        self.selected_model = Some(index);
        let available = self.available_engines();
        if !available.contains(&self.selected_engine)
            && let Some(engine) = available.first()
        {
            self.selected_engine = *engine;
            self.save_preferences();
        }
        self.upscaled_image_path = None;
        self.comparison_ready = false;
        self.detected_scale = None;
        self.execution_engine = None;
    }

    pub(crate) fn set_image(&mut self, path: PathBuf) {
        self.cancel_active_job();
        self.image_path = Some(path);
        self.upscaled_image_path = None;
        self.comparison_ready = false;
        self.detected_scale = None;
        self.execution_engine = None;
    }
}
