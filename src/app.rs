use gpui::{Context, Render, Window, div, prelude::*, rgb};

use crate::{components, state::ImageScaler};

impl Render for ImageScaler {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let drop_area = components::drop_area::render(self, window, cx);
        let sidebar = components::sidebar::render(self, window, cx);

        div()
            .size_full()
            .bg(rgb(0x111113))
            .text_color(rgb(0xe9e9ed))
            .flex()
            .when(!self.sidebar_right, |element| element.flex_row_reverse())
            .child(drop_area)
            .child(sidebar)
    }
}
