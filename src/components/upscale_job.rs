use std::time::{Duration, Instant};

use gpui::{AnyElement, Context, div, prelude::*, px, rgb};

use crate::{
    inferencing::{Cancellation, OutputSettings, UpscaleRequest, upscale_image},
    state::{ImageScaler, UpscaleState},
};

pub(crate) fn render(state: &ImageScaler, cx: &mut Context<ImageScaler>) -> AnyElement {
    let has_image = state.image_path.is_some();
    let has_model = state
        .selected_model
        .and_then(|index| state.models.get(index))
        .is_some();
    let is_upscaling = matches!(state.upscale_state, UpscaleState::Running { .. });
    let progress = match &state.upscale_state {
        UpscaleState::Running { progress, .. } => *progress,
        _ => 0,
    };
    let elapsed_millis = match &state.upscale_state {
        UpscaleState::Running { elapsed_millis, .. } => *elapsed_millis,
        _ => 0,
    };
    let can_upscale = has_image && has_model && !is_upscaling;
    let button_label = if !has_image {
        state.language.text("add_image_first").to_string()
    } else if !has_model {
        state.language.text("add_model_first").to_string()
    } else if is_upscaling {
        format!(
            "{} {progress}% · {} · {}",
            state.language.text("upscaling"),
            format_elapsed(elapsed_millis),
            state.language.text("cancel")
        )
    } else {
        state.language.text("start_upscale").to_string()
    };

    let button = div()
        .id("upscale-button")
        .flex_none()
        .w_full()
        .py_3()
        .rounded_lg()
        .text_center()
        .font_weight(gpui::FontWeight::BOLD)
        .text_size(px(13.))
        .bg(if can_upscale {
            rgb(0xa6ff6f)
        } else {
            rgb(0x303036)
        })
        .text_color(if can_upscale {
            rgb(0x13200c)
        } else {
            rgb(0x696971)
        })
        .when(can_upscale, |element| {
            element
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0xb7ff8a)))
                .on_click(cx.listener(start))
        })
        .when(is_upscaling, |element| {
            element
                .cursor_pointer()
                .hover(|style| style.bg(rgb(0x3a3a42)))
                .on_click(cx.listener(|this, _, _, cx| {
                    if let UpscaleState::Running { cancellation, .. } = &this.upscale_state {
                        cancellation.cancel();
                    }
                    cx.notify();
                }))
        })
        .child(button_label);

    let error = match &state.upscale_state {
        UpscaleState::Error(error) => Some(error.clone()),
        _ => None,
    };
    div()
        .flex_none()
        .px_4()
        .pt_2()
        .pb_4()
        .bg(rgb(0x171719))
        .child(button)
        .children(error.map(|error| {
            div()
                .mt_2()
                .text_size(px(11.))
                .text_color(rgb(0xff7b72))
                .child(error)
        }))
        .into_any_element()
}

fn start(
    this: &mut ImageScaler,
    _: &gpui::ClickEvent,
    _: &mut gpui::Window,
    cx: &mut Context<ImageScaler>,
) {
    let Some(image_path) = this.image_path.clone() else {
        return;
    };
    let Some(model) = this
        .selected_model
        .and_then(|index| this.models.get(index))
        .cloned()
    else {
        return;
    };
    let request = UpscaleRequest {
        image_path,
        model_path: model.file_path,
        output: OutputSettings {
            directory: this.output_dir.clone(),
            format: this.output_format,
            suffix: this.filename_suffix.clone(),
            preserve_metadata: this.preserve_metadata,
        },
        engine: this.selected_engine,
        ncnn_model: model.ncnn,
    };

    let Ok(cancellation) = Cancellation::new().map(std::sync::Arc::new) else {
        this.upscale_state =
            UpscaleState::Error("Failed to initialize cancellation support".to_string());
        cx.notify();
        return;
    };
    let job_id = this.next_job_id;
    this.next_job_id = this.next_job_id.wrapping_add(1);
    this.upscale_state = UpscaleState::Running {
        job_id,
        progress: 0,
        elapsed_millis: 0,
        cancellation: cancellation.clone(),
    };
    this.upscaled_image_path = None;
    this.comparison_ready = false;
    this.detected_scale = None;
    this.execution_engine = None;

    let started_at = Instant::now();
    let executor = cx.background_executor().clone();
    cx.spawn(async move |this, cx| {
        loop {
            executor.timer(Duration::from_millis(100)).await;
            let active = this
                .update(cx, |this, cx| {
                    if let UpscaleState::Running {
                        job_id: active_job,
                        elapsed_millis,
                        ..
                    } = &mut this.upscale_state
                        && *active_job == job_id
                    {
                        *elapsed_millis = started_at.elapsed().as_millis() as u64;
                        cx.notify();
                        true
                    } else {
                        false
                    }
                })
                .unwrap_or(false);
            if !active {
                break;
            }
        }
    })
    .detach();

    let (progress_sender, progress_receiver) = async_channel::unbounded();
    let task = cx.background_spawn(async move {
        let result = upscale_image(request, cancellation, |progress| {
            let _ = progress_sender.try_send(progress.percent());
        });
        if let Err(error) = &result {
            eprintln!("[upscale] Failed: {error:#}");
        }
        result
    });
    cx.spawn(async move |this, cx| {
        while let Ok(progress) = progress_receiver.recv().await {
            let _ = this.update(cx, |this, cx| {
                if let UpscaleState::Running {
                    job_id: active_job,
                    progress: active_progress,
                    ..
                } = &mut this.upscale_state
                    && *active_job == job_id
                {
                    *active_progress = progress;
                    cx.notify();
                }
            });
        }
    })
    .detach();
    cx.spawn(async move |this, cx| {
        let result = task.await;
        let _ = this.update(cx, |this, cx| {
            let is_current = matches!(
                this.upscale_state,
                UpscaleState::Running { job_id: active, .. } if active == job_id
            );
            if !is_current {
                return;
            }
            match result {
                Ok(outcome) => {
                    this.upscaled_image_path = Some(outcome.output_path);
                    this.comparison_ready = true;
                    this.comparison_position = 0.5;
                    this.detected_scale = Some(outcome.scale);
                    this.execution_engine = Some(outcome.engine);
                    this.upscale_state = UpscaleState::Idle;
                }
                Err(error) if error.to_string().contains("cancelled") => {
                    this.upscale_state = UpscaleState::Idle;
                }
                Err(error) => {
                    this.upscale_state = UpscaleState::Error(error.to_string());
                }
            }
            cx.notify();
        });
    })
    .detach();
    cx.notify();
}

fn format_elapsed(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    let centiseconds = milliseconds % 1_000 / 10;
    format!("{seconds:02}:{centiseconds:02}")
}

#[cfg(test)]
mod tests {
    use super::format_elapsed;

    #[test]
    fn elapsed_time_uses_two_digit_seconds_and_fraction() {
        assert_eq!(format_elapsed(0), "00:00");
        assert_eq!(format_elapsed(65), "00:06");
        assert_eq!(format_elapsed(5_432), "05:43");
        assert_eq!(format_elapsed(65_432), "65:43");
    }
}
