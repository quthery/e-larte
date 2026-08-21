use gpui::{Context, FontWeight, Render, Window, div, prelude::*, rgb};

use crate::{components, state::ImageScaler};

impl Render for ImageScaler {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let drop_area = components::drop_area::render(self, window, cx);
        let sidebar = components::sidebar::render(self, window, cx);
        let model_import = self
            .model_import_open
            .then(|| components::model_import::render(self, window, cx));

        div()
            .relative()
            .size_full()
            .font_family(".SystemUIFont")
            .font_weight(FontWeight::NORMAL)
            .bg(rgb(0x111113))
            .text_color(rgb(0xf0f0f3))
            .flex()
            .when(!self.sidebar_right, |element| element.flex_row_reverse())
            .child(drop_area)
            .child(sidebar)
            .children(model_import)
    }
}
