use gpui::{IntoElement, div, prelude::*, px, rgb};

pub(crate) fn section_label(text: &'static str) -> impl IntoElement {
    div()
        .flex_none()
        .text_size(px(12.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(0xaaaab4))
        .child(text)
}
