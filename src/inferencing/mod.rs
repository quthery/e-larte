mod backend;
mod engine;
mod ncnn;
pub mod onnx;
mod output;
mod profile;
mod tiling;

use std::time::Instant;
use std::{path::PathBuf, sync::Arc};

use crate::models::NcnnModel;
use anyhow::Context;
use image::DynamicImage;

pub(crate) use engine::InferenceEngine;
pub(crate) use onnx::OnnxRuntime;
pub(crate) use output::{OutputFormat, OutputSettings};
pub(crate) use tiling::Cancellation;

pub(crate) fn ncnn_models() -> Vec<crate::models::Model> {
    ncnn::installed_models()
}

pub(crate) fn configure_diagnostics() {
    profile::configure(std::env::args_os().skip(1));
}

pub trait ModelRuntime {
    type Input;
    type Output;
    type Session;

    #[allow(dead_code)]
    fn tensorize(&self, image: &DynamicImage) -> anyhow::Result<Self::Input>;
    fn ensure_session(&mut self) -> anyhow::Result<&mut Self::Session>;
    #[allow(dead_code)]
    fn upscale(&mut self, input: Self::Input) -> anyhow::Result<Self::Output>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UpscaleStage {
    LoadingImage,
    LoadingModel,
    Inferencing,
    Saving,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct UpscaleProgress {
    pub(crate) stage: UpscaleStage,
    pub(crate) completed: u32,
    pub(crate) total: u32,
}

impl UpscaleProgress {
    pub(crate) fn percent(self) -> u8 {
        match self.stage {
            UpscaleStage::LoadingImage => 2,
            UpscaleStage::LoadingModel => 5,
            UpscaleStage::Inferencing => {
                let fraction = self.completed as f32 / self.total.max(1) as f32;
                (5.0 + fraction * 90.0).round().clamp(5.0, 95.0) as u8
            }
            UpscaleStage::Saving => 97,
        }
    }
}

pub(crate) struct UpscaleRequest {
    pub(crate) image_path: PathBuf,
    pub(crate) model_path: PathBuf,
    pub(crate) output: OutputSettings,
    pub(crate) engine: InferenceEngine,
    pub(crate) ncnn_model: Option<NcnnModel>,
}

pub(crate) struct UpscaleOutcome {
    pub(crate) output_path: PathBuf,
    pub(crate) scale: u32,
    pub(crate) engine: InferenceEngine,
}

pub(crate) fn upscale_image(
    request: UpscaleRequest,
    cancellation: Arc<Cancellation>,
    mut on_progress: impl FnMut(UpscaleProgress),
) -> anyhow::Result<UpscaleOutcome> {
    let total_started = Instant::now();
    profile::event(format_args!(
        "job start: image={}, model={}, requested_engine={}",
        request.image_path.display(),
        request.model_path.display(),
        request.engine,
    ));
    let stage_started = Instant::now();
    on_progress(UpscaleProgress {
        stage: UpscaleStage::LoadingImage,
        completed: 0,
        total: 1,
    });
    let source = output::load(&request.image_path)?;
    profile::event(format_args!(
        "image loaded: width={}, height={}, format={:?}, elapsed_ms={:.3}",
        source.pixels.width(),
        source.pixels.height(),
        source.source_format,
        profile::milliseconds(stage_started.elapsed()),
    ));
    if cancellation.is_cancelled() {
        anyhow::bail!("Upscaling cancelled");
    }

    on_progress(UpscaleProgress {
        stage: UpscaleStage::LoadingModel,
        completed: 0,
        total: 1,
    });
    let stage_started = Instant::now();
    let (pixels, scale) = match request.engine {
        InferenceEngine::Onnx => {
            let preferred = backend::ExecutionBackend::best_available();
            let mut runtime = OnnxRuntime::with_backend(&request.model_path, preferred);
            runtime.ensure_session().with_context(|| {
                format!("Failed to load model: {}", request.model_path.display())
            })?;
            let accelerator = runtime.backend();
            profile::event(format_args!(
                "model session ready: engine=ONNX Runtime, accelerator={}, elapsed_ms={:.3}",
                accelerator,
                profile::milliseconds(stage_started.elapsed()),
            ));
            eprintln!(
                "[upscale] Active backend: ONNX Runtime, accelerator={}, device={}",
                accelerator,
                accelerator.device_description(),
            );

            let inference_started = Instant::now();
            let result = tiling::upscale(
                &source.pixels,
                &mut runtime,
                &cancellation,
                |completed, total| {
                    on_progress(UpscaleProgress {
                        stage: UpscaleStage::Inferencing,
                        completed,
                        total,
                    });
                },
            );
            runtime.finish_profiling();
            let result = result?;
            profile::event(format_args!(
                "ONNX tiled inference complete: scale={}x, output={}x{}, elapsed_ms={:.3}",
                result.scale,
                result.pixels.width(),
                result.pixels.height(),
                profile::milliseconds(inference_started.elapsed()),
            ));
            (result.pixels, result.scale)
        }
        InferenceEngine::Ncnn => {
            let model = request
                .ncnn_model
                .context("The selected model is not compatible with NCNN")?;
            on_progress(UpscaleProgress {
                stage: UpscaleStage::Inferencing,
                completed: 0,
                total: 1,
            });
            let pixels = ncnn::upscale(&source.pixels, model, &cancellation)?;
            on_progress(UpscaleProgress {
                stage: UpscaleStage::Inferencing,
                completed: 1,
                total: 1,
            });
            (pixels, model.scale)
        }
    };
    profile::event(format_args!(
        "inference complete: engine={}, scale={}x, output={}x{}, elapsed_ms={:.3}",
        request.engine,
        scale,
        pixels.width(),
        pixels.height(),
        profile::milliseconds(stage_started.elapsed()),
    ));
    if cancellation.is_cancelled() {
        anyhow::bail!("Upscaling cancelled");
    }

    on_progress(UpscaleProgress {
        stage: UpscaleStage::Saving,
        completed: 0,
        total: 1,
    });
    let stage_started = Instant::now();
    let output_path =
        output::output_path(&request.image_path, source.source_format, &request.output)?;
    output::save(&output_path, pixels, &source, &request.output)?;
    profile::event(format_args!(
        "output saved: path={}, elapsed_ms={:.3}, total_ms={:.3}",
        output_path.display(),
        profile::milliseconds(stage_started.elapsed()),
        profile::milliseconds(total_started.elapsed()),
    ));

    Ok(UpscaleOutcome {
        output_path,
        scale,
        engine: request.engine,
    })
}

#[cfg(all(test, feature = "upscale-tests"))]
mod integration_tests {
    use super::*;

    #[test]
    fn tiled_inference_derives_scale_from_fixed_size_hat_model() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/testdata");
        let image = image::open(fixtures.join("images/testupscale.jpg"))
            .unwrap()
            .crop_imm(0, 0, 33, 33);
        let mut runtime = OnnxRuntime::new(fixtures.join("models/onnx/Real_HAT_GAN_SRx4.onnx"));
        runtime.ensure_session().unwrap();
        let output = tiling::upscale(
            &image,
            &mut runtime,
            &Cancellation::new().unwrap(),
            |_, _| {},
        )
        .unwrap();

        assert_eq!(output.scale, 4);
        assert_eq!(output.pixels.dimensions(), (132, 132));
    }
}
