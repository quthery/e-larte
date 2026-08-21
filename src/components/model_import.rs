use std::{
    collections::HashSet,
    fs::{self, File},
    io,
    path::{Path, PathBuf},
    time::Duration,
};

use gpui::{
    AnyElement, Context, ExternalPaths, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    PathPromptOptions, Window, div, prelude::*, px, relative, rgb,
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Deserialize;

use crate::{
    models::{ImportedModel, MODELS, RemoteModelFile},
    state::{ImageScaler, ModelBrowserState},
};

const DIALOG_WIDTH: f32 = 580.;
const DIALOG_HEIGHT: f32 = 500.;
const PAGE_SIZE: usize = 24;

#[derive(Deserialize)]
struct HubModel {
    id: String,
    #[serde(default)]
    downloads: u64,
    #[serde(default)]
    siblings: Vec<HubFile>,
}

#[derive(Deserialize)]
struct HubFile {
    rfilename: String,
}

struct HubPage {
    models: Vec<RemoteModelFile>,
    next: Option<String>,
}

pub(crate) fn open(state: &mut ImageScaler, window: &mut Window, cx: &mut Context<ImageScaler>) {
    state.model_import_open = true;
    state.model_menu_open = false;
    state.model_import_notice = None;
    window.focus(&state.model_search_focus);

    if state.remote_models.is_empty()
        && !matches!(state.remote_models_state, ModelBrowserState::Loading)
    {
        load_models(state, true, cx);
    }
    cx.notify();
}

pub(crate) fn render(
    state: &ImageScaler,
    window: &mut Window,
    cx: &mut Context<ImageScaler>,
) -> AnyElement {
    let offset = state.model_import_offset;
    let dialog = div()
        .id("model-import-dialog")
        .absolute()
        .left(relative(0.5))
        .top(relative(0.5))
        .ml(px(-DIALOG_WIDTH / 2.) + offset.x)
        .mt(px(-DIALOG_HEIGHT / 2.) + offset.y)
        .w(px(DIALOG_WIDTH))
        .h(px(DIALOG_HEIGHT))
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(rgb(0x3b3b43))
        .bg(rgb(0x19191c))
        .shadow_2xl()
        .flex()
        .flex_col()
        .on_click(|_, _, cx| cx.stop_propagation())
        .child(header(state, cx))
        .child(content(state, window, cx));

    div()
        .id("model-import-overlay")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(gpui::rgba(0x08080ab8))
        .on_click(cx.listener(|this, _, _, cx| close(this, cx)))
        .on_mouse_move(cx.listener(on_dialog_mouse_move))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, _| this.model_import_drag_origin = None),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, _, _| this.model_import_drag_origin = None),
        )
        .child(dialog)
        .into_any_element()
}

fn header(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    div()
        .id("model-import-header")
        .flex_none()
        .h(px(50.))
        .px_5()
        .border_b_1()
        .border_color(rgb(0x2c2c32))
        .flex()
        .items_center()
        .justify_between()
        .child(
            div()
                .pt_1()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_size(px(15.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(state.language.text("import_models")),
                )
                .child(
                    div()
                        .text_size(px(11.))
                        .text_color(rgb(0xa0a0aa))
                        .child(state.language.text("import_models_hint")),
                ),
        )
        .child(
            div()
                .id("close-model-import")
                .size(px(26.))
                .rounded_md()
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(18.))
                .text_color(rgb(0x8f8f98))
                .hover(|style| style.bg(rgb(0x29292f)).text_color(rgb(0xf1f1f3)))
                .child("×")
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(|this, _, _, cx| close(this, cx))),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.model_import_drag_origin = Some(event.position);
                this.model_import_drag_start = this.model_import_offset;
                cx.stop_propagation();
            }),
        )
        .into_any_element()
}

fn content(state: &ImageScaler, window: &mut Window, cx: &mut Context<ImageScaler>) -> AnyElement {
    div()
        .flex_1()
        .min_h_0()
        .p_5()
        .flex()
        .flex_col()
        .gap_3()
        .child(local_drop_area(state, cx))
        .children(
            state
                .model_import_notice
                .as_ref()
                .map(|(success, message)| {
                    div()
                        .flex_none()
                        .px_3()
                        .py_1p5()
                        .rounded_md()
                        .bg(if *success {
                            rgb(0x20291d)
                        } else {
                            rgb(0x302021)
                        })
                        .text_size(px(11.))
                        .text_color(if *success {
                            rgb(0xa6ff6f)
                        } else {
                            rgb(0xff9a9a)
                        })
                        .child(message.clone())
                }),
        )
        .child(search_box(state, window, cx))
        .child(
            div()
                .flex_none()
                .mb(px(-5.))
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .text_size(px(10.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(0xaaaab4))
                        .child(state.language.text("popular_models")),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(0x888892))
                        .child("Hugging Face"),
                ),
        )
        .child(remote_list(state, cx))
        .into_any_element()
}

fn local_drop_area(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    let choose_prompt = state.language.text("choose_model").to_string();
    div()
        .id("model-file-drop-area")
        .group("model-file-drop")
        .flex_none()
        .h(px(104.))
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x42424b))
        .bg(rgb(0x1e1e22))
        .cursor_pointer()
        .hover(|style| style.border_color(rgb(0x6f9652)).bg(rgb(0x20241e)))
        .drag_over::<ExternalPaths>(|style, paths, _, _| {
            if paths.paths().iter().any(|path| is_model_path(path)) {
                style.border_color(rgb(0xa6ff6f)).bg(rgb(0x22291e))
            } else {
                style
            }
        })
        .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
            import_paths(this, paths.paths().to_vec(), cx);
        }))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_1()
        .child(
            div()
                .text_size(px(19.))
                .text_color(rgb(0xa6ff6f))
                .child("+"),
        )
        .child(
            div()
                .text_size(px(13.))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(state.language.text("drop_model")),
        )
        .child(
            div()
                .text_size(px(10.))
                .text_color(rgb(0x9696a0))
                .child(state.language.text("model_formats")),
        )
        .on_click(cx.listener(move |_, _, _, cx| {
            let receiver = cx.prompt_for_paths(PathPromptOptions {
                files: true,
                directories: false,
                multiple: true,
                prompt: Some(choose_prompt.clone().into()),
            });
            cx.spawn(async move |this, cx| {
                if let Ok(Ok(Some(paths))) = receiver.await {
                    let _ = this.update(cx, |this, cx| import_paths(this, paths, cx));
                }
            })
            .detach();
        }))
        .into_any_element()
}

fn search_box(
    state: &ImageScaler,
    window: &mut Window,
    cx: &mut Context<ImageScaler>,
) -> AnyElement {
    let empty = state.model_search.is_empty();
    div()
        .id("model-search")
        .flex_none()
        .h(px(38.))
        .px_3()
        .rounded_lg()
        .border_1()
        .border_color(if state.model_search_focus.is_focused(window) {
            rgb(0x6d904f)
        } else {
            rgb(0x35353c)
        })
        .bg(rgb(0x202025))
        .cursor_text()
        .track_focus(&state.model_search_focus)
        .flex()
        .items_center()
        .gap_2()
        .child(
            div()
                .text_size(px(14.))
                .text_color(rgb(0x9696a0))
                .child("⌕"),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_ellipsis()
                .text_size(px(12.))
                .text_color(if empty { rgb(0x92929c) } else { rgb(0xededf0) })
                .child(if empty {
                    state.language.text("search_models").to_string()
                } else {
                    state.model_search.clone()
                }),
        )
        .on_click(cx.listener(|this, _, window, _| window.focus(&this.model_search_focus)))
        .on_key_down(cx.listener(on_search_key_down))
        .into_any_element()
}

fn remote_list(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    let mut list = div()
        .id("remote-model-list")
        .flex_1()
        .min_h_0()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x2c2c32))
        .bg(rgb(0x171719))
        .overflow_y_scroll()
        .scrollbar_width(px(4.))
        .track_scroll(&state.model_list_scroll)
        .on_scroll_wheel(cx.listener(|this, _, _, cx| maybe_load_more(this, cx)));

    if state.remote_models.is_empty() {
        list = list.child(empty_remote_state(state, cx));
    } else {
        list = list.children(
            state
                .remote_models
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, model)| remote_model_row(state, index, model, cx)),
        );

        if matches!(state.remote_models_state, ModelBrowserState::Loading) {
            list = list.child(
                div()
                    .w_full()
                    .py_3()
                    .text_center()
                    .text_size(px(11.))
                    .text_color(rgb(0x9696a0))
                    .child(state.language.text("loading_models")),
            );
        } else if matches!(state.remote_models_state, ModelBrowserState::Error(_)) {
            list = list.child(retry_row(state, cx));
        }
    }

    list.into_any_element()
}

fn empty_remote_state(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    match &state.remote_models_state {
        ModelBrowserState::Loading => div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.))
            .text_color(rgb(0xa0a0aa))
            .child(state.language.text("loading_models"))
            .into_any_element(),
        ModelBrowserState::Error(error) => div()
            .size_full()
            .px_5()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .child(
                div()
                    .text_size(px(12.))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(0xe0a0a0))
                    .child(state.language.text("models_offline")),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(rgb(0x9696a0))
                    .child(state.language.text("models_offline_hint")),
            )
            .child(
                div()
                    .max_w(px(430.))
                    .text_ellipsis()
                    .text_size(px(9.))
                    .text_color(rgb(0x5f5f68))
                    .child(error.clone()),
            )
            .child(retry_button(state, cx))
            .into_any_element(),
        ModelBrowserState::Idle => div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .text_size(px(11.))
            .text_color(rgb(0x9696a0))
            .child(state.language.text("no_models"))
            .child(
                div()
                    .text_size(px(9.))
                    .text_color(rgb(0x5f5f68))
                    .child(state.language.text("search_enter")),
            )
            .into_any_element(),
    }
}

fn remote_model_row(
    state: &ImageScaler,
    index: usize,
    model: RemoteModelFile,
    cx: &mut Context<ImageScaler>,
) -> AnyElement {
    let key = model.key();
    let downloading = state.downloading_model.as_deref() == Some(key.as_str());
    let extension = Path::new(model.display_name())
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("model")
        .to_uppercase();
    let action_model = model.clone();

    div()
        .id(("remote-model", index))
        .w_full()
        .min_h(px(56.))
        .px_3()
        .py_2()
        .border_b_1()
        .border_color(rgb(0x29292f))
        .flex()
        .items_center()
        .gap_3()
        .hover(|style| style.bg(rgb(0x1d1d21)))
        .child(
            div()
                .flex_none()
                .px_2()
                .py_1()
                .rounded_sm()
                .bg(rgb(0x29292f))
                .text_size(px(9.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0xa6ff6f))
                .child(extension),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(
                    div()
                        .text_ellipsis()
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(0xe7e7ea))
                        .child(model.display_name().to_string()),
                )
                .child(
                    div()
                        .text_ellipsis()
                        .text_size(px(10.))
                        .text_color(rgb(0x92929c))
                        .child(format!(
                            "{}  |  {} {}",
                            model.repo_id,
                            format_downloads(model.downloads),
                            state.language.text("downloads")
                        )),
                ),
        )
        .child(
            div()
                .id(("download-model", index))
                .flex_none()
                .px_3()
                .py_1p5()
                .rounded_md()
                .bg(if downloading {
                    rgb(0x2b3426)
                } else {
                    rgb(0x29292f)
                })
                .text_size(px(10.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(if downloading {
                    rgb(0x8eaa7c)
                } else {
                    rgb(0xc9c9cf)
                })
                .when(
                    !downloading && state.downloading_model.is_none(),
                    |button| {
                        button
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0xa6ff6f)).text_color(rgb(0x15200f)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                download_model(this, action_model.clone(), cx);
                            }))
                    },
                )
                .child(if downloading {
                    state.language.text("downloading")
                } else {
                    state.language.text("download")
                }),
        )
        .into_any_element()
}

fn retry_row(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    div()
        .w_full()
        .py_3()
        .flex()
        .items_center()
        .justify_center()
        .child(retry_button(state, cx))
        .into_any_element()
}

fn retry_button(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    div()
        .id("retry-model-search")
        .px_3()
        .py_1p5()
        .rounded_md()
        .bg(rgb(0x29292f))
        .cursor_pointer()
        .text_size(px(10.))
        .text_color(rgb(0xc9c9cf))
        .hover(|style| style.bg(rgb(0x34343b)))
        .child(state.language.text("retry"))
        .on_click(cx.listener(|this, _, _, cx| {
            let reset = this.remote_models.is_empty();
            load_models(this, reset, cx);
        }))
        .into_any_element()
}

fn close(state: &mut ImageScaler, cx: &mut Context<ImageScaler>) {
    state.model_import_open = false;
    state.model_import_drag_origin = None;
    cx.notify();
}

fn on_dialog_mouse_move(
    state: &mut ImageScaler,
    event: &MouseMoveEvent,
    window: &mut Window,
    cx: &mut Context<ImageScaler>,
) {
    let Some(origin) = state.model_import_drag_origin else {
        return;
    };
    if !event.dragging() {
        state.model_import_drag_origin = None;
        return;
    }

    let viewport = window.viewport_size();
    let max_x = ((viewport.width - px(DIALOG_WIDTH)) * 0.5 - px(8.)).max(px(0.));
    let max_y = ((viewport.height - px(DIALOG_HEIGHT)) * 0.5 - px(8.)).max(px(0.));
    state.model_import_offset.x =
        (state.model_import_drag_start.x + event.position.x - origin.x).clamp(-max_x, max_x);
    state.model_import_offset.y =
        (state.model_import_drag_start.y + event.position.y - origin.y).clamp(-max_y, max_y);
    cx.notify();
    window.refresh();
    window.request_animation_frame();
}

fn on_search_key_down(
    state: &mut ImageScaler,
    event: &KeyDownEvent,
    _: &mut Window,
    cx: &mut Context<ImageScaler>,
) {
    match event.keystroke.key.as_str() {
        "enter" => load_models(state, true, cx),
        "escape" => close(state, cx),
        "backspace" => {
            state.model_search.pop();
            state.model_import_notice = None;
            cx.notify();
        }
        _ if state.model_search.chars().count() < 80 => {
            if let Some(text) = &event.keystroke.key_char {
                state
                    .model_search
                    .extend(text.chars().filter(|character| !character.is_control()));
                state.model_import_notice = None;
                cx.notify();
            }
        }
        _ => {}
    }
}

fn import_paths(state: &mut ImageScaler, paths: Vec<PathBuf>, cx: &mut Context<ImageScaler>) {
    let valid: Vec<_> = paths
        .into_iter()
        .filter(|path| is_model_path(path))
        .collect();
    if valid.is_empty() {
        state.model_import_notice = Some((false, state.language.text("invalid_model").to_string()));
        cx.notify();
        return;
    }

    let count = valid.len();
    for path in valid {
        add_imported_model(state, path);
    }
    state.model_import_notice = Some((
        true,
        state
            .language
            .text(if count == 1 {
                "model_imported"
            } else {
                "models_imported"
            })
            .to_string(),
    ));
    cx.notify();
}

fn add_imported_model(state: &mut ImageScaler, path: PathBuf) {
    if let Some(index) = state
        .imported_models
        .iter()
        .position(|model| model.path == path)
    {
        state.selected_model = MODELS.len() + index;
        return;
    }

    let name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Imported model")
        .to_string();
    state.imported_models.push(ImportedModel { name, path });
    state.selected_model = MODELS.len() + state.imported_models.len() - 1;
}

fn is_model_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("onnx"))
        .unwrap_or(false)
}

fn load_models(state: &mut ImageScaler, reset: bool, cx: &mut Context<ImageScaler>) {
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
        hub_search_url(&state.model_search)
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
        .spawn(async move { fetch_hub_page(&url) });
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
                Err(error) => {
                    this.remote_models_state = ModelBrowserState::Error(error);
                }
            }
            cx.notify();
        });
    })
    .detach();
}

fn maybe_load_more(state: &mut ImageScaler, cx: &mut Context<ImageScaler>) {
    if state.remote_models_next.is_none()
        || matches!(state.remote_models_state, ModelBrowserState::Loading)
    {
        return;
    }
    let offset = -state.model_list_scroll.offset().y;
    let maximum = state.model_list_scroll.max_offset().height;
    if maximum <= px(0.) || maximum - offset <= px(140.) {
        load_models(state, false, cx);
    }
}

fn download_model(state: &mut ImageScaler, model: RemoteModelFile, cx: &mut Context<ImageScaler>) {
    if state.downloading_model.is_some() {
        return;
    }
    let key = model.key();
    state.downloading_model = Some(key.clone());
    state.model_import_notice = None;
    cx.notify();

    let download = cx
        .background_executor()
        .spawn(async move { download_hub_model(&model) });
    cx.spawn(async move |this, cx| {
        let result = download.await;
        let _ = this.update(cx, |this, cx| {
            if this.downloading_model.as_deref() != Some(key.as_str()) {
                return;
            }
            this.downloading_model = None;
            match result {
                Ok(path) => {
                    add_imported_model(this, path);
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

fn hub_search_url(query: &str) -> String {
    let query = if query.trim().is_empty() {
        "upscal"
    } else {
        query.trim()
    };
    format!(
        "https://huggingface.co/api/models?search={}&sort=downloads&direction=-1&limit={PAGE_SIZE}&full=true",
        utf8_percent_encode(query, NON_ALPHANUMERIC)
    )
}

fn fetch_hub_page(url: &str) -> Result<HubPage, String> {
    let mut current_url = url.to_string();
    let mut models = Vec::new();
    let mut next = None;

    // ONNX files are relatively sparse in Hub search results. Walk through a
    // few cursor pages so an empty API page does not look like an empty search.
    for _ in 0..5 {
        let page = fetch_hub_page_once(&current_url)?;
        models.extend(page.models);
        next = page.next;
        if !models.is_empty() || next.is_none() {
            break;
        }
        current_url = next.clone().expect("next URL was checked above");
    }

    Ok(HubPage { models, next })
}

fn fetch_hub_page_once(url: &str) -> Result<HubPage, String> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(18)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(url)
        .header("User-Agent", "super-puper-image-scaler/0.1")
        .call()
        .map_err(|error| error.to_string())?;
    let next = response
        .headers()
        .get("link")
        .and_then(|value| value.to_str().ok())
        .and_then(parse_next_link);
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| error.to_string())?;
    let repositories: Vec<HubModel> =
        serde_json::from_str(&body).map_err(|error| error.to_string())?;
    let models = repositories
        .into_iter()
        .filter_map(|repository| {
            choose_model_file(&repository.siblings).map(|file_name| RemoteModelFile {
                repo_id: repository.id,
                file_name,
                downloads: repository.downloads,
            })
        })
        .collect();
    Ok(HubPage { models, next })
}

fn choose_model_file(files: &[HubFile]) -> Option<String> {
    files
        .iter()
        .filter(|file| is_model_path(Path::new(&file.rfilename)))
        .min_by_key(|file| file.rfilename.matches('/').count())
        .map(|file| file.rfilename.clone())
}

fn parse_next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|link| {
        let (url, relation) = link.trim().split_once(';')?;
        if relation.trim() != "rel=\"next\"" {
            return None;
        }
        url.trim()
            .strip_prefix('<')
            .and_then(|url| url.strip_suffix('>'))
            .map(str::to_string)
    })
}

fn download_hub_model(model: &RemoteModelFile) -> Result<PathBuf, String> {
    let download_root = dirs::download_dir()
        .or_else(|| std::env::current_dir().ok())
        .ok_or_else(|| "Downloads directory is unavailable".to_string())?
        .join("Super Puper Image Scaler")
        .join("Models");
    fs::create_dir_all(&download_root).map_err(|error| error.to_string())?;

    let file_name = Path::new(&model.file_name)
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Invalid model file name".to_string())?;
    let destination = unique_download_path(&download_root, file_name);
    let partial = destination.with_extension(format!(
        "{}.part",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("model")
    ));
    let url = format!(
        "https://huggingface.co/{}/resolve/main/{}?download=true",
        encode_path(&model.repo_id),
        encode_path(&model.file_name)
    );
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60 * 30)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .get(&url)
        .header("User-Agent", "super-puper-image-scaler/0.1")
        .call()
        .map_err(|error| error.to_string())?;
    let mut output = File::create(&partial).map_err(|error| error.to_string())?;
    if let Err(error) = io::copy(&mut response.body_mut().as_reader(), &mut output) {
        let _ = fs::remove_file(&partial);
        return Err(error.to_string());
    }
    fs::rename(&partial, &destination).map_err(|error| {
        let _ = fs::remove_file(&partial);
        error.to_string()
    })?;
    Ok(destination)
}

fn unique_download_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("model");
    let extension = path.extension().and_then(|extension| extension.to_str());
    for suffix in 2..10_000 {
        let name = match extension {
            Some(extension) => format!("{stem} ({suffix}).{extension}"),
            None => format!("{stem} ({suffix})"),
        };
        let candidate = directory.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("downloaded-{file_name}"))
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn format_downloads(downloads: u64) -> String {
    if downloads >= 1_000_000 {
        format!("{:.1}M", downloads as f64 / 1_000_000.)
    } else if downloads >= 1_000 {
        format!("{:.1}K", downloads as f64 / 1_000.)
    } else {
        downloads.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_model_files_case_insensitively() {
        assert!(is_model_path(Path::new("weights/RealESRGAN.ONNX")));
        assert!(is_model_path(Path::new("model.onnx")));
        assert!(!is_model_path(Path::new("weights.pth")));
        assert!(!is_model_path(Path::new("weights.safetensors")));
        assert!(!is_model_path(Path::new("README.md")));
    }

    #[test]
    fn parses_hugging_face_next_link() {
        let header = "<https://huggingface.co/api/models?cursor=abc>; rel=\"next\"";
        assert_eq!(
            parse_next_link(header).as_deref(),
            Some("https://huggingface.co/api/models?cursor=abc")
        );
    }

    #[test]
    fn selects_only_onnx_files() {
        let files = vec![
            HubFile {
                rfilename: "weights.safetensors".into(),
            },
            HubFile {
                rfilename: "weights.onnx".into(),
            },
        ];
        assert_eq!(choose_model_file(&files).as_deref(), Some("weights.onnx"));
    }

    #[test]
    #[ignore]
    fn live_hugging_face_response_is_compatible() {
        let page = fetch_hub_page(&hub_search_url("upscal")).unwrap();
        assert!(!page.models.is_empty());
        assert!(page.next.is_some());
    }
}
