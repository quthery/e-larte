mod app;
mod components;
mod image_utils;
mod localization;
mod models;
mod state;
mod theme;

use gpui::{
    App, AppContext, Application, Bounds, TitlebarOptions, WindowBackgroundAppearance,
    WindowBounds::Windowed, WindowKind, WindowOptions, point, px, size,
};
use state::ImageScaler;

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::new(point(px(100.0), px(100.0)), size(px(920.0), px(640.0)));
        cx.activate(true);

        cx.open_window(
            WindowOptions {
                window_bounds: Some(Windowed(bounds)),
                titlebar: Some(TitlebarOptions::default()),
                focus: true,
                show: true,
                kind: WindowKind::Normal,
                is_movable: true,
                is_resizable: true,
                is_minimizable: true,
                display_id: None,
                window_background: WindowBackgroundAppearance::default(),
                app_id: None,
                window_min_size: Some(size(px(760.), px(520.))),
                window_decorations: None,
                tabbing_identifier: None,
            },
            |_, cx| cx.new(ImageScaler::new),
        )
        .unwrap();
    });
}
