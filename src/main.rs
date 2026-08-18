use gpui::{
    App, Application, Bounds, Context, ExternalPaths, PathPromptOptions, TitlebarOptions, Window,
    WindowBackgroundAppearance, WindowBounds::Windowed, WindowKind, WindowOptions, div, img, point,
    prelude::*, px, rgb, size,
};

use std::path::{Path, PathBuf};

const MODELS: [(&str, &str); 3] = [
    ("Real-ESRGAN", "Лучший выбор для фото"),
    ("AnimeSharp", "Аниме и иллюстрации"),
    ("UltraMix", "Детали и текстуры"),
];

pub struct ImageScaler {
    image_path: Option<PathBuf>,
    output_dir: Option<PathBuf>,
    selected_model: usize,
    model_menu_open: bool,
    scale: u8,
    sidebar_right: bool,
}

impl ImageScaler {
    pub fn new() -> Self {
        Self {
            image_path: None,
            output_dir: None,
            selected_model: 0,
            model_menu_open: false,
            scale: 4,
            sidebar_right: true,
        }
    }
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "bmp"
            )
        })
        .unwrap_or(false)
}

fn label(text: &'static str) -> impl IntoElement {
    div()
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(rgb(0x92929d))
        .child(text)
}

impl Render for ImageScaler {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let image_content = match &self.image_path {
            Some(path) => img(path.clone()).size_full().into_any_element(),
            None => div()
                .size_full()
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
                        .hover(|style| style.bg(rgb(0x2b3028)).border_color(rgb(0xa6ff6f)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(25.))
                        .text_color(rgb(0xa6ff6f))
                        .child("＋")
                        .on_click(cx.listener(|_, _, _, cx| {
                            let receiver = cx.prompt_for_paths(PathPromptOptions {
                                files: true,
                                directories: false,
                                multiple: false,
                                prompt: Some("Выбрать изображение".into()),
                            });
                            cx.spawn(async move |this, cx| {
                                if let Ok(Ok(Some(paths))) = receiver.await
                                    && let Some(path) =
                                        paths.into_iter().find(|path| is_image(path))
                                {
                                    let _ = this.update(cx, |this, cx| {
                                        this.image_path = Some(path);
                                        cx.notify();
                                    });
                                }
                            })
                            .detach();
                        })),
                )
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(0xe9e9ed))
                        .child("Перетащите изображение"),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(rgb(0x777781))
                        .child("PNG, JPG или WEBP · до 50 МБ"),
                )
                .into_any_element(),
        };

        let output_name = self
            .output_dir
            .as_deref()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .unwrap_or("Рядом с оригиналом")
            .to_string();

        let model_menu = self.model_menu_open.then(|| {
            div()
                .mt_1()
                .w_full()
                .rounded_lg()
                .border_1()
                .border_color(rgb(0x393941))
                .bg(rgb(0x202025))
                .overflow_hidden()
                .children(MODELS.iter().enumerate().map(|(index, (name, hint))| {
                    let selected = self.selected_model == index;
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
                                        .text_size(px(12.))
                                        .text_color(rgb(0xf1f1f3))
                                        .child(*name),
                                )
                                .child(
                                    div()
                                        .text_size(px(10.))
                                        .text_color(rgb(0x7e7e88))
                                        .child(*hint),
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

        let has_image = self.image_path.is_some();

        div()
            .size_full()
            .bg(rgb(0x111113))
            .text_color(rgb(0xe9e9ed))
            .flex()
            .when(!self.sidebar_right, |element| element.flex_row_reverse())
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .p_5()
                    .pt_8()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .id("image-drop-area")
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .rounded_xl()
                            .border_1()
                            .border_color(rgb(0x34343b))
                            .bg(rgb(0x18181b))
                            .drag_over::<ExternalPaths>(|style, paths, _, _| {
                                if paths.paths().iter().any(|path| is_image(path)) {
                                    style.border_color(rgb(0xa6ff6f)).bg(rgb(0x1d211b))
                                } else {
                                    style
                                }
                            })
                            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                                if let Some(path) = paths.paths().iter().find(|path| is_image(path))
                                {
                                    this.image_path = Some(path.clone());
                                    cx.notify();
                                }
                            }))
                            .child(image_content),
                    ),
            )
            .child(
                div()
                    .w(px(286.))
                    .h_full()
                    .flex_none()
                    .when(self.sidebar_right, |element| element.border_l_1())
                    .when(!self.sidebar_right, |element| element.border_r_1())
                    .border_color(rgb(0x29292f))
                    .bg(rgb(0x171719))
                    .px_4()
                    .pt_8()
                    .pb_4()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .mb_5()
                            .child(
                                div()
                                    .text_size(px(17.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .child("Настройки"),
                            )
                            .child(
                                div()
                                    .mt_1()
                                    .text_size(px(11.))
                                    .text_color(rgb(0x777781))
                                    .child("Подготовьте изображение к апскейлу"),
                            ),
                    )
                    .child(label("МОДЕЛЬ АПСКЕЙЛА"))
                    .child(
                        div()
                            .id("model-select")
                            .mt_2()
                            .w_full()
                            .px_3()
                            .py_2()
                            .rounded_lg()
                            .border_1()
                            .border_color(if self.model_menu_open {
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
                                            .text_size(px(12.))
                                            .child(MODELS[self.selected_model].0),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(0x7e7e88))
                                            .child(MODELS[self.selected_model].1),
                                    ),
                            )
                            .child(
                                div()
                                    .text_color(rgb(0x888892))
                                    .child(if self.model_menu_open { "⌃" } else { "⌄" }),
                            )
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.model_menu_open = !this.model_menu_open;
                                cx.notify();
                            })),
                    )
                    .children(model_menu)
                    .child(div().h(px(20.)))
                    .child(label("МАСШТАБ"))
                    .child(
                        div()
                            .mt_2()
                            .p_1()
                            .rounded_lg()
                            .bg(rgb(0x202025))
                            .flex()
                            .gap_1()
                            .children([2u8, 4u8].into_iter().map(|scale| {
                                let selected = self.scale == scale;
                                div()
                                    .id(("scale", scale as usize))
                                    .flex_1()
                                    .py_2()
                                    .rounded_md()
                                    .cursor_pointer()
                                    .text_center()
                                    .text_size(px(12.))
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
                    .child(div().h(px(20.)))
                    .child(label("СОХРАНИТЬ В"))
                    .child(
                        div()
                            .id("output-folder")
                            .mt_2()
                            .w_full()
                            .px_3()
                            .py_2()
                            .rounded_lg()
                            .border_1()
                            .border_color(rgb(0x393941))
                            .bg(rgb(0x202025))
                            .cursor_pointer()
                            .hover(|style| style.border_color(rgb(0x55555f)))
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(div().text_size(px(15.)).child("⌁"))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(11.))
                                    .text_color(rgb(0xb5b5bd))
                                    .child(output_name),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(0x777781))
                                    .child("Изменить"),
                            )
                            .on_click(cx.listener(|_, _, _, cx| {
                                let receiver = cx.prompt_for_paths(PathPromptOptions {
                                    files: false,
                                    directories: true,
                                    multiple: false,
                                    prompt: Some("Выбрать папку".into()),
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
                    .child(div().h(px(20.)))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_0p5()
                                    .child(div().text_size(px(12.)).child("Сайдбар справа"))
                                    .child(
                                        div()
                                            .text_size(px(10.))
                                            .text_color(rgb(0x777781))
                                            .child("Перемещает сайдбар вправо"),
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
                                    .bg(if self.sidebar_right {
                                        rgb(0x8bd45e)
                                    } else {
                                        rgb(0x3a3a42)
                                    })
                                    .flex()
                                    .justify_end()
                                    .when(!self.sidebar_right, |el| el.justify_start())
                                    .child(div().size(px(16.)).rounded_full().bg(rgb(0xf2f2f2)))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.sidebar_right = !this.sidebar_right;
                                        cx.notify();
                                    })),
                            ),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .mb_3()
                            .px_3()
                            .py_2()
                            .rounded_lg()
                            .bg(rgb(0x1e1e22))
                            .flex()
                            .justify_between()
                            .text_size(px(10.))
                            .text_color(rgb(0x7f7f89))
                            .child("Ожидаемый размер")
                            .child(format!("{}× больше", self.scale * self.scale)),
                    )
                    .child(
                        div()
                            .id("upscale-button")
                            .w_full()
                            .py_3()
                            .rounded_lg()
                            .text_center()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_size(px(12.))
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
                            .when(has_image, |el| {
                                el.cursor_pointer().hover(|style| style.bg(rgb(0xb7ff8a)))
                            })
                            .child(if has_image {
                                "Начать апскейл  →"
                            } else {
                                "Сначала добавьте изображение"
                            }),
                    ),
            )
    }
}

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
            |_, cx| cx.new(|_| ImageScaler::new()),
        )
        .unwrap();
    });
}
