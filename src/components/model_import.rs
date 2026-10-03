use std::path::{Path, PathBuf};

use crate::{
    components::model_browser,
    model_hub, model_storage,
    models::{self, RemoteModelFile},
    state::{ImageScaler, ModelBrowserState},
};
use gpui::{
    AnyElement, Context, ExternalPaths, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    PathPromptOptions, Window, div, prelude::*, px, relative, rgb,
};

const DIALOG_WIDTH: f32 = 580.;
const DIALOG_HEIGHT: f32 = 500.;

pub(crate) fn open(state: &mut ImageScaler, window: &mut Window, cx: &mut Context<ImageScaler>) {
    state.model_import_open = true;
    state.model_menu_open = false;
    state.backend_menu_open = false;
    state.model_import_notice = None;
    window.focus(&state.model_search_focus);

    if state.remote_models.is_empty()
        && !matches!(state.remote_models_state, ModelBrowserState::Loading)
    {
        model_browser::load(state, true, cx);
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
            if paths
                .paths()
                .iter()
                .any(|path| model_storage::is_supported(path))
            {
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
        .on_scroll_wheel(cx.listener(|this, _, _, cx| model_browser::maybe_load_more(this, cx)));

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
            .child(state.language.text("no_remote_models"))
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
                            model_hub::format_downloads(model.downloads),
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
                                model_browser::download(this, action_model.clone(), cx);
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
            model_browser::load(this, reset, cx);
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
        "enter" => model_browser::load(state, true, cx),
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
        .filter(|path| model_storage::is_supported(path))
        .collect();
    if valid.is_empty() {
        state.model_import_notice = Some((false, state.language.text("invalid_model").to_string()));
        cx.notify();
        return;
    }

    state.model_import_notice = Some((true, state.language.text("importing_model").to_string()));
    cx.notify();
    let import = cx
        .background_executor()
        .spawn(async move { model_storage::import(&valid) });
    cx.spawn(async move |this, cx| {
        let result = import.await;
        let _ = this.update(cx, |this, cx| {
            match result {
                Ok(count) if count > 0 => {
                    refresh_imported_models(this);
                    this.model_import_notice = Some((
                        true,
                        this.language
                            .text(if count == 1 {
                                "model_imported"
                            } else {
                                "models_imported"
                            })
                            .to_string(),
                    ));
                }
                Ok(_) => {
                    this.model_import_notice =
                        Some((false, this.language.text("invalid_model").to_string()));
                }
                Err(error) => this.model_import_notice = Some((false, error)),
            }
            cx.notify();
        });
    })
    .detach();
}

fn refresh_imported_models(state: &mut ImageScaler) {
    state.cancel_active_job();
    state.models = models::available();
    if let Some(index) = (!state.models.is_empty()).then(|| state.models.len() - 1) {
        state.select_model(index);
    }
    state.model_menu_open = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_model_files_case_insensitively() {
        assert!(model_storage::is_supported(Path::new(
            "weights/RealESRGAN.ONNX"
        )));
        assert!(model_storage::is_supported(Path::new("model.onnx")));
        assert!(!model_storage::is_supported(Path::new("weights.pth")));
        assert!(!model_storage::is_supported(Path::new(
            "weights.safetensors"
        )));
        assert!(!model_storage::is_supported(Path::new("README.md")));
    }
}
