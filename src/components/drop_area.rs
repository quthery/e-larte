use gpui::{
    AnyElement, Context, ExternalPaths, MouseButton, MouseMoveEvent, PathPromptOptions, Window,
    div, img, prelude::*, px, relative, rgb,
};

use crate::{image_utils::is_image, state::ImageScaler};

pub(crate) fn render(
    state: &ImageScaler,
    _window: &mut Window,
    cx: &mut Context<ImageScaler>,
) -> AnyElement {
    let image_content = match &state.image_path {
        Some(path) => img(path.clone()).size_full().into_any_element(),
        None => empty_state(state),
    };
    let has_image = state.image_path.is_some();
    let choose_image_prompt = state.language.text("choose_image").to_string();

    div()
        .flex_1()
        .min_w_0()
        .p_5()
        .flex()
        .flex_col()
        .gap_4()
        .child(
            div()
                .id("image-drop-area")
                .group("empty-drop-area")
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .rounded_xl()
                .border_1()
                .border_color(rgb(0x34343b))
                .bg(rgb(0x18181b))
                .when(!has_image, |area| {
                    area.cursor_pointer()
                        .hover(|style| style.border_color(rgb(0x6d904f)).bg(rgb(0x1b1d1a)))
                        .drag_over::<ExternalPaths>(|style, paths, _, _| {
                            if paths.paths().iter().any(|path| is_image(path)) {
                                style.border_color(rgb(0xa6ff6f)).bg(rgb(0x1d211b))
                            } else {
                                style
                            }
                        })
                        .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                            if let Some(path) = paths.paths().iter().find(|path| is_image(path)) {
                                this.image_path = Some(path.clone());
                                this.comparison_ready = false;
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |_, _, _, cx| {
                            let receiver = cx.prompt_for_paths(PathPromptOptions {
                                files: true,
                                directories: false,
                                multiple: false,
                                prompt: Some(choose_image_prompt.clone().into()),
                            });
                            cx.spawn(async move |this, cx| {
                                if let Ok(Ok(Some(paths))) = receiver.await
                                    && let Some(path) =
                                        paths.into_iter().find(|path| is_image(path))
                                {
                                    let _ = this.update(cx, |this, cx| {
                                        this.image_path = Some(path);
                                        this.comparison_ready = false;
                                        cx.notify();
                                    });
                                }
                            })
                            .detach();
                        }))
                })
                .child(image_content)
                .when(has_image && state.comparison_ready, |area| {
                    area.child(comparison_layer(state.comparison_position, cx))
                }),
        )
        .into_any_element()
}

fn empty_state(state: &ImageScaler) -> AnyElement {
    div()
        .size_full()
        .group_hover("empty-drop-area", |style| style.opacity(0.55))
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap_3()
        .child(
            div()
                .id("select-image-button")
                .size(px(58.))
                .rounded_full()
                .bg(rgb(0x222228))
                .border_1()
                .border_color(rgb(0x34343b))
                .cursor_pointer()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(25.))
                .text_color(rgb(0xa6ff6f))
                .child("＋"),
        )
        .child(
            div()
                .text_size(px(18.))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(0xf2f2f4))
                .child(state.language.text("drop_image")),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(rgb(0xa6a6b0))
                .child(state.language.text("supported_images")),
        )
        .into_any_element()
}

fn comparison_layer(position: f32, cx: &mut Context<ImageScaler>) -> AnyElement {
    div()
        .id("comparison-layer")
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
            if !this.comparison_dragging {
                return;
            }
            if event.pressed_button != Some(MouseButton::Left) {
                this.comparison_dragging = false;
                cx.notify();
                return;
            }

            let area_width = window.viewport_size().width - px(326.);
            let area_left = if this.sidebar_right {
                px(20.)
            } else {
                px(306.)
            };
            this.comparison_position = ((event.position.x - area_left) / area_width).clamp(0., 1.);
            cx.notify();
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, _| {
                this.comparison_dragging = false;
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, _, _| {
                this.comparison_dragging = false;
            }),
        )
        .child(comparison_slider(position, cx))
        .into_any_element()
}

fn comparison_slider(position: f32, cx: &mut Context<ImageScaler>) -> AnyElement {
    div()
        .id("comparison-slider")
        .absolute()
        .top_0()
        .left(relative(position))
        .ml(px(-20.))
        .h_full()
        .w(px(40.))
        .cursor_col_resize()
        .child(
            div()
                .absolute()
                .top_0()
                .left(px(19.))
                .h_full()
                .w(px(2.))
                .bg(rgb(0xffffff))
                .hover(|style| style.bg(rgb(0xa6ff6f))),
        )
        .child(
            div()
                .absolute()
                .top(relative(0.5))
                .left(px(2.))
                .mt(px(-18.))
                .size(px(36.))
                .rounded_full()
                .border_1()
                .border_color(rgb(0xffffff))
                .bg(rgb(0x202025))
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(15.))
                .text_color(rgb(0xffffff))
                .hover(|style| style.border_color(rgb(0xa6ff6f)).bg(rgb(0x293323)))
                .child("‹  ›"),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.comparison_dragging = true;
                cx.stop_propagation();
            }),
        )
        .on_click(cx.listener(|_, _, _, cx| {
            cx.stop_propagation();
        }))
        .into_any_element()
}
