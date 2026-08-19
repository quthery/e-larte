use gpui::{
    AnyElement, Context, KeyDownEvent, PathPromptOptions, Window, div, prelude::*, px, rgb,
};
use std::path::Path;

use crate::{
    models::MODELS,
    state::{DenoiseStrength, ImageScaler, OutputFormat},
    theme::section_label,
};

pub(crate) fn render(
    state: &ImageScaler,
    window: &mut Window,
    cx: &mut Context<ImageScaler>,
) -> AnyElement {
    let output_name = state
        .output_dir
        .as_deref()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or(state.language.text("next_to_original"))
        .to_string();

    let model_menu = state.model_menu_open.then(|| {
        div()
            .id("model-menu")
            .flex_none()
            .mt_1()
            .w_full()
            .rounded_lg()
            .border_1()
            .border_color(rgb(0x393941))
            .bg(rgb(0x202025))
            .max_h(px(180.))
            .overflow_y_scroll()
            .scrollbar_width(px(4.))
            .children(MODELS.iter().enumerate().map(|(index, (name, hint))| {
                let selected = state.selected_model == index;
                div()
                    .id(("model-option", index))
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
                                    .child(*name),
                            )
                            .child(
                                div()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x92929c))
                                    .child(state.language.text(hint)),
                            ),
                    )
                    .when(selected, |el| {
                        el.child(
                            div()
                                .text_color(rgb(0xa6ff6f))
                                .text_size(px(13.))
                                .child("✓"),
                        )
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_model = index;
                        this.model_menu_open = false;
                        cx.notify();
                    }))
            }))
    });

    let has_image = state.image_path.is_some();
    let choose_folder_prompt = state.language.text("choose_folder").to_string();

    let settings_content = div()
        .id("settings-scroll")
        .flex_1()
        .min_h_0()
        .px_4()
        .pt_8()
        .pb_2()
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .scrollbar_width(px(4.))
        .child(
            div()
                .flex_none()
                .mb_5()
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(state.language.text("settings")),
                )
                .child(
                    div()
                        .mt_1()
                        .text_size(px(12.))
                        .text_color(rgb(0x92929c))
                        .child(state.language.text("settings_hint")),
                ),
        )
        .child(section_label(state.language.text("upscale_model")))
        .child(
            div()
                .id("model-select")
                .flex_none()
                .mt_2()
                .w_full()
                .px_3()
                .py_2()
                .rounded_lg()
                .border_1()
                .border_color(if state.model_menu_open {
                    rgb(0x6d904f)
                } else {
                    rgb(0x393941)
                })
                .bg(rgb(0x202025))
                .cursor_pointer()
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
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(MODELS[state.selected_model].0),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x92929c))
                                .child(state.language.text(MODELS[state.selected_model].1)),
                        ),
                )
                .child(
                    div()
                        .text_color(rgb(0x888892))
                        .child(if state.model_menu_open { "⌃" } else { "⌄" }),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.model_menu_open = !this.model_menu_open;
                    cx.notify();
                })),
        )
        .children(model_menu)
        .child(div().h(px(20.)).flex_none())
        .child(section_label(state.language.text("scale")))
        .child(
            div()
                .flex_none()
                .mt_2()
                .p_1()
                .rounded_lg()
                .bg(rgb(0x202025))
                .flex()
                .gap_1()
                .children([2u8, 4u8].into_iter().map(|scale| {
                    let selected = state.scale == scale;
                    div()
                        .id(("scale", scale as usize))
                        .flex_1()
                        .py_1()
                        .rounded_md()
                        .cursor_pointer()
                        .text_center()
                        .text_size(px(13.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .bg(if selected {
                            rgb(0x34343b)
                        } else {
                            rgb(0x202025)
                        })
                        .text_color(if selected {
                            rgb(0xa6ff6f)
                        } else {
                            rgb(0x85858f)
                        })
                        .child(format!("{}×", scale))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.scale = scale;
                            cx.notify();
                        }))
                })),
        )
        .child(div().h(px(20.)).flex_none())
        .child(section_label(state.language.text("save_to")))
        .child(
            div()
                .id("output-folder")
                .flex_none()
                .mt_2()
                .w_full()
                .px_3()
                .py_2p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(0x2d2d33))
                .bg(rgb(0x1c1c1f))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0x222226)).border_color(rgb(0x46464f)))
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(12.))
                        .text_color(rgb(0xd0d0d6))
                        .child(output_name),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_size(px(11.))
                        .text_color(rgb(0x92929c))
                        .child(state.language.text("change"))
                        .child(div().text_size(px(14.)).child("›")),
                )
                .on_click(cx.listener(move |_, _, _, cx| {
                    let receiver = cx.prompt_for_paths(PathPromptOptions {
                        files: false,
                        directories: true,
                        multiple: false,
                        prompt: Some(choose_folder_prompt.clone().into()),
                    });
                    cx.spawn(async move |this, cx| {
                        if let Ok(Ok(Some(paths))) = receiver.await
                            && let Some(path) = paths.into_iter().next()
                        {
                            let _ = this.update(cx, |this, cx| {
                                this.output_dir = Some(path);
                                cx.notify();
                            });
                        }
                    })
                    .detach();
                })),
        )
        .child(div().h(px(20.)).flex_none())
        .child(section_label(state.language.text("output_format")))
        .child(
            div()
                .flex_none()
                .mt_2()
                .p_1()
                .rounded_lg()
                .bg(rgb(0x202025))
                .flex()
                .gap_1()
                .children(
                    [
                        (
                            OutputFormat::Original,
                            state.language.text("format_original"),
                        ),
                        (OutputFormat::Png, "PNG"),
                        (OutputFormat::Jpeg, "JPG"),
                        (OutputFormat::Webp, "WebP"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (format, label))| {
                        let selected = state.output_format == format;
                        div()
                            .id(("output-format", index))
                            .flex_1()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .text_center()
                            .text_size(px(10.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .bg(if selected {
                                rgb(0x34343b)
                            } else {
                                rgb(0x202025)
                            })
                            .text_color(if selected {
                                rgb(0xa6ff6f)
                            } else {
                                rgb(0x85858f)
                            })
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.output_format = format;
                                cx.notify();
                            }))
                    }),
                ),
        )
        .child(div().h(px(20.)).flex_none())
        .child(section_label(state.language.text("denoise_strength")))
        .child(
            div()
                .flex_none()
                .mt_2()
                .p_1()
                .rounded_lg()
                .bg(rgb(0x202025))
                .flex()
                .gap_1()
                .children(
                    [
                        (DenoiseStrength::Low, state.language.text("denoise_low")),
                        (
                            DenoiseStrength::Medium,
                            state.language.text("denoise_medium"),
                        ),
                        (DenoiseStrength::High, state.language.text("denoise_high")),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(index, (strength, label))| {
                        let selected = state.denoise_strength == strength;
                        div()
                            .id(("denoise", index))
                            .flex_1()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .text_center()
                            .text_size(px(11.))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .bg(if selected {
                                rgb(0x34343b)
                            } else {
                                rgb(0x202025)
                            })
                            .text_color(if selected {
                                rgb(0xa6ff6f)
                            } else {
                                rgb(0x85858f)
                            })
                            .child(label)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.denoise_strength = strength;
                                cx.notify();
                            }))
                    }),
                ),
        )
        .child(div().h(px(20.)).flex_none())
        .child(section_label(state.language.text("filename_suffix")))
        .child(
            div()
                .id("filename-suffix")
                .flex_none()
                .mt_2()
                .w_full()
                .px_3()
                .py_2()
                .rounded_md()
                .border_1()
                .border_color(if state.suffix_focus.is_focused(window) {
                    rgb(0x6d904f)
                } else {
                    rgb(0x393941)
                })
                .bg(rgb(0x202025))
                .cursor_text()
                .track_focus(&state.suffix_focus)
                .text_size(px(12.))
                .text_color(if state.filename_suffix.is_empty() {
                    rgb(0x707078)
                } else {
                    rgb(0xd0d0d6)
                })
                .child(if state.filename_suffix.is_empty() {
                    state.language.text("suffix_placeholder").to_string()
                } else {
                    state.filename_suffix.clone()
                })
                .on_click(cx.listener(|this, _, window, _| {
                    window.focus(&this.suffix_focus);
                }))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if event.keystroke.key == "backspace" {
                        this.filename_suffix.pop();
                    } else if this.filename_suffix.chars().count() < 32
                        && let Some(text) = &event.keystroke.key_char
                    {
                        this.filename_suffix
                            .extend(text.chars().filter(|character| {
                                !matches!(
                                    character,
                                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                                )
                            }));
                    }
                    cx.notify();
                })),
        )
        .child(div().h(px(20.)).flex_none())
        .child(
            div()
                .flex_none()
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
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(state.language.text("preserve_metadata")),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x92929c))
                                .child(state.language.text("preserve_metadata_hint")),
                        ),
                )
                .child(
                    div()
                        .id("metadata-toggle")
                        .w(px(36.))
                        .h(px(20.))
                        .p(px(2.))
                        .rounded_full()
                        .cursor_pointer()
                        .bg(if state.preserve_metadata {
                            rgb(0x8bd45e)
                        } else {
                            rgb(0x3a3a42)
                        })
                        .flex()
                        .justify_end()
                        .when(!state.preserve_metadata, |element| element.justify_start())
                        .child(div().size(px(16.)).rounded_full().bg(rgb(0xf2f2f2)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.preserve_metadata = !this.preserve_metadata;
                            cx.notify();
                        })),
                ),
        )
        .child(div().h(px(20.)).flex_none())
        .child(
            div()
                .flex_none()
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
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .child(state.language.text("sidebar_right")),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(0x92929c))
                                .child(state.language.text("sidebar_right_hint")),
                        ),
                )
                .child(
                    div()
                        .id("face-toggle")
                        .w(px(36.))
                        .h(px(20.))
                        .p(px(2.))
                        .rounded_full()
                        .cursor_pointer()
                        .bg(if state.sidebar_right {
                            rgb(0x8bd45e)
                        } else {
                            rgb(0x3a3a42)
                        })
                        .flex()
                        .justify_end()
                        .when(!state.sidebar_right, |el| el.justify_start())
                        .child(div().size(px(16.)).rounded_full().bg(rgb(0xf2f2f2)))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.sidebar_right = !this.sidebar_right;
                            cx.notify();
                        })),
                ),
        )
        .child(div().h(px(4.)).flex_none());

    let upscale_button = div()
        .id("upscale-button")
        .flex_none()
        .w_full()
        .py_3()
        .rounded_lg()
        .text_center()
        .font_weight(gpui::FontWeight::BOLD)
        .text_size(px(13.))
        .bg(if has_image {
            rgb(0xa6ff6f)
        } else {
            rgb(0x303036)
        })
        .text_color(if has_image {
            rgb(0x13200c)
        } else {
            rgb(0x696971)
        })
        .when(has_image, |element| {
            element
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0xb7ff8a)))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.comparison_ready = true;
                    this.comparison_position = 0.5;
                    cx.notify();
                }))
        })
        .child(if has_image {
            state.language.text("start_upscale")
        } else {
            state.language.text("add_image_first")
        });

    div()
        .id("settings-sidebar")
        .w(px(286.))
        .h_full()
        .flex_none()
        .when(state.sidebar_right, |element| element.border_l_1())
        .when(!state.sidebar_right, |element| element.border_r_1())
        .border_color(rgb(0x29292f))
        .bg(rgb(0x171719))
        .flex()
        .flex_col()
        .child(settings_content)
        .child(
            div()
                .flex_none()
                .px_4()
                .pt_2()
                .pb_4()
                .bg(rgb(0x171719))
                .child(upscale_button),
        )
        .into_any_element()
}
