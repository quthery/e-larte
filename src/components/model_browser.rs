use std::collections::HashSet;

use gpui::{Context, px};

use crate::{
    model_hub, model_storage,
    models::{self, RemoteModelFile},
    state::{ImageScaler, ModelBrowserState},
};

pub(crate) fn load(state: &mut ImageScaler, reset: bool, cx: &mut Context<ImageScaler>) {
    if matches!(state.remote_models_state, ModelBrowserState::Loading) {
        return;
    }

    if reset {
        state.model_search_generation = state.model_search_generation.wrapping_add(1);
        state.remote_models.clear();
        state.remote_models_next = None;
        state
            .model_list_scroll
            .set_offset(gpui::point(px(0.), px(0.)));
    }

    let url = if reset {
        model_hub::search_url(&state.model_search)
    } else if let Some(next) = state.remote_models_next.clone() {
        next
    } else {
        return;
    };
    let generation = state.model_search_generation;
    state.remote_models_state = ModelBrowserState::Loading;
    cx.notify();

    let fetch = cx
        .background_executor()
        .spawn(async move { model_hub::fetch_page(&url) });
    cx.spawn(async move |this, cx| {
        let result = fetch.await;
        let _ = this.update(cx, |this, cx| {
            if generation != this.model_search_generation {
                return;
            }
            match result {
                Ok(page) => {
                    let mut known: HashSet<String> = this
                        .remote_models
                        .iter()
                        .map(RemoteModelFile::key)
                        .collect();
                    this.remote_models.extend(
                        page.models
                            .into_iter()
                            .filter(|model| known.insert(model.key())),
                    );
                    this.remote_models_next = page.next;
                    this.remote_models_state = ModelBrowserState::Idle;
                }
                Err(error) => this.remote_models_state = ModelBrowserState::Error(error),
            }
            cx.notify();
        });
    })
    .detach();
}

pub(crate) fn maybe_load_more(state: &mut ImageScaler, cx: &mut Context<ImageScaler>) {
    if state.remote_models_next.is_none()
        || matches!(state.remote_models_state, ModelBrowserState::Loading)
    {
        return;
    }
    let offset = -state.model_list_scroll.offset().y;
    let maximum = state.model_list_scroll.max_offset().height;
    if maximum <= px(0.) || maximum - offset <= px(140.) {
        load(state, false, cx);
    }
}

pub(crate) fn download(
    state: &mut ImageScaler,
    model: RemoteModelFile,
    cx: &mut Context<ImageScaler>,
) {
    if state.downloading_model.is_some() {
        return;
    }
    let key = model.key();
    state.downloading_model = Some(key.clone());
    state.model_import_notice = None;
    cx.notify();

    let download = cx.background_executor().spawn(async move {
        let path = model_hub::download(&model)?;
        let count = model_storage::import(&[path])?;
        if count == 0 {
            return Err("Downloaded file is not a supported ONNX model".to_string());
        }
        Ok(())
    });
    cx.spawn(async move |this, cx| {
        let result = download.await;
        let _ = this.update(cx, |this, cx| {
            if this.downloading_model.as_deref() != Some(key.as_str()) {
                return;
            }
            this.downloading_model = None;
            match result {
                Ok(()) => {
                    this.cancel_active_job();
                    this.models = models::available();
                    if let Some(index) = (!this.models.is_empty()).then(|| this.models.len() - 1) {
                        this.select_model(index);
                    }
                    this.model_menu_open = false;
                    this.model_import_notice = Some((
                        true,
                        format!(
                            "{} - {}",
                            this.language.text("model_imported"),
                            this.language.text("saved_to_downloads")
                        ),
                    ));
                }
                Err(error) => {
                    this.model_import_notice = Some((
                        false,
                        format!("{}: {error}", this.language.text("download_failed")),
                    ));
                }
            }
            cx.notify();
        });
    })
    .detach();
}
