use gpui::{AnyElement, Context, div, prelude::*, px, rgb};

use crate::{
    inferencing::InferenceEngine,
    state::{ImageScaler, UpscaleState},
    theme::section_label,
};

pub(crate) fn render(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    let is_running = matches!(state.upscale_state, UpscaleState::Running { .. });
    let selected = state.selected_engine;
    let options = state
        .available_engines()
        .into_iter()
        .map(|engine| {
            let description = match engine {
                InferenceEngine::Ncnn => state.language.text("ncnn_backend_hint"),
                InferenceEngine::Onnx => state.language.text("onnx_backend_hint"),
            };
            (engine, description.to_string())
        })
        .collect::<Vec<_>>();
    let menu = state.backend_menu_open.then(|| {
        div()
            .id("backend-menu")
            .absolute()
            .bottom(px(36.))
            .left_0()
            .right_0()
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x393941))
            .bg(rgb(0x202025))
            .children(
                options
                    .into_iter()
                    .enumerate()
                    .map(|(index, (engine, description))| {
                        let is_selected = selected == engine;
                        div()
                            .id(("backend-option", index))
                            .w_full()
                            .px_3()
                            .py_2()
                            .cursor_pointer()
                            .hover(|style| style.bg(rgb(0x2a2a31)))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .text_color(rgb(0xf1f1f3))
                                            .child(engine.to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(11.))
                                            .text_color(rgb(0x92929c))
                                            .child(description),
                                    ),
                            )
                            .when(is_selected, |element| {
                                element.child(
                                    div()
                                        .text_color(rgb(0xa6ff6f))
                                        .text_size(px(13.))
                                        .child("✓"),
                                )
                            })
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_engine = engine;
                                this.backend_menu_open = false;
                                this.save_preferences();
                                cx.notify();
                            }))
                    }),
            )
    });

    div()
        .flex_none()
        .child(section_label(state.language.text("inference_backend")))
        .child(
            div()
                .id("backend-select-wrapper")
                .relative()
                .mt_2()
                .w_full()
                .child(
                    div()
                        .id("backend-select")
                        .flex_none()
                        .w_full()
                        .px_3()
                        .py_2()
                        .rounded_lg()
                        .border_1()
                        .border_color(if state.backend_menu_open {
                            rgb(0x6d904f)
                        } else {
                            rgb(0x393941)
                        })
                        .bg(rgb(0x202025))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(13.))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(if is_running {
                                    rgb(0x696971)
                                } else {
                                    rgb(0xf1f1f3)
                                })
                                .child(selected.to_string()),
                        )
                        .child(
                            div()
                                .text_color(rgb(0x888892))
                                .child(if state.backend_menu_open {
                                    "⌃"
                                } else {
                                    "⌄"
                                }),
                        )
                        .when(!is_running, |element| {
                            element
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.model_menu_open = false;
                                    this.backend_menu_open = !this.backend_menu_open;
                                    cx.notify();
                                }))
                        }),
                )
                .children(menu),
        )
        .into_any_element()
}
