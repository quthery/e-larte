use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Instant,
};

use crate::inferencing::{
    ModelRuntime,
    backend::{self, ExecutionBackend},
    profile,
};
use anyhow::{Context, bail};
use image::DynamicImage;
use ndarray::{Array4, s};
use ort::{
    inputs,
    session::{RunOptions, Session},
    value::{TensorElementType, TensorRef, ValueType},
};

pub struct OnnxRuntime {
    session: Option<Session>,
    model_path: PathBuf,
    backend: ExecutionBackend,
    preferred_backend: ExecutionBackend,
}

pub struct OnnxInput {
    rgba: Array4<f32>,
}

impl OnnxRuntime {
    #[cfg(test)]
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self::with_backend(model_path, ExecutionBackend::best_available())
    }

    pub(crate) fn with_backend(
        model_path: impl Into<PathBuf>,
        preferred_backend: ExecutionBackend,
    ) -> Self {
        Self {
            session: None,
            model_path: model_path.into(),
            backend: ExecutionBackend::Cpu,
            preferred_backend,
        }
    }

    fn validate_session(session: &Session, model_path: &Path) -> anyhow::Result<()> {
        let [input] = session.inputs() else {
            bail!(
                "Model {} must have exactly one input, but has {}",
                model_path.display(),
                session.inputs().len(),
            );
        };

        Self::validate_tensor_type(input.dtype(), "input", model_path, true)?;

        let [output] = session.outputs() else {
            bail!(
                "Model {} must have exactly one output, but has {}",
                model_path.display(),
                session.outputs().len(),
            );
        };
        Self::validate_tensor_type(output.dtype(), "output", model_path, false)
    }

    pub(crate) fn input_size(&mut self) -> anyhow::Result<(Option<u32>, Option<u32>)> {
        let session = self.ensure_session()?;
        let ValueType::Tensor { shape, .. } = session.inputs()[0].dtype() else {
            unreachable!("session metadata was validated when it was loaded");
        };
        Ok((
            (shape[3] > 0).then_some(shape[3] as u32),
            (shape[2] > 0).then_some(shape[2] as u32),
        ))
    }

    pub(crate) fn backend(&self) -> ExecutionBackend {
        self.backend
    }

    pub(crate) fn upscale_with_options(
        &mut self,
        input: OnnxInput,
        run_options: &Arc<RunOptions>,
    ) -> anyhow::Result<Array4<f32>> {
        self.run(input, Some(run_options))
    }

    fn validate_tensor_type(
        value_type: &ValueType,
        kind: &str,
        model_path: &Path,
        validate_channels: bool,
    ) -> anyhow::Result<()> {
        let ValueType::Tensor { ty, shape, .. } = value_type else {
            bail!("Model {} {kind} must be a tensor", model_path.display());
        };
        if *ty != TensorElementType::Float32 {
            bail!(
                "Model {} {kind} must contain f32 values, but contains {ty:?}",
                model_path.display(),
            );
        }
        if shape.len() != 4 {
            bail!(
                "Model {} {kind} must be a 4D NCHW tensor, but has shape {shape:?}",
                model_path.display(),
            );
        }
        if ![-1, 1].contains(&shape[0]) {
            bail!(
                "Model {} {kind} batch dimension must be 1 or dynamic, but is {}",
                model_path.display(),
                shape[0],
            );
        }
        if shape[2] == 0 || shape[3] == 0 {
            bail!(
                "Model {} {kind} spatial dimensions cannot be zero",
                model_path.display(),
            );
        }
        if validate_channels {
            let declared_channels = shape[1];
            if ![-1, 3, 4].contains(&declared_channels) {
                bail!(
                    "Model {} expects {declared_channels} input channels; only RGB and RGBA models are supported",
                    model_path.display(),
                );
            }
        }
        Ok(())
    }
}

impl ModelRuntime for OnnxRuntime {
    type Input = OnnxInput;
    type Output = Array4<f32>;
    type Session = Session;

    fn tensorize(&self, image: &DynamicImage) -> anyhow::Result<Self::Input> {
        let rgba = image.to_rgba8();
        self.tensorize_rgba(&rgba)
    }

    fn ensure_session(&mut self) -> anyhow::Result<&mut Session> {
        if self.session.is_none() {
            let session_started = Instant::now();
            let (mut builder, mut selected_backend) =
                backend::session_builder(self.preferred_backend)?;
            profile::event(format_args!(
                "session builder configured: requested_backend={}, elapsed_ms={:.3}",
                self.preferred_backend,
                profile::milliseconds(session_started.elapsed()),
            ));
            let commit_started = Instant::now();
            let session = match builder.commit_from_file(&self.model_path) {
                Ok(session) => session,
                Err(accelerator_error) if selected_backend != ExecutionBackend::Cpu => {
                    eprintln!(
                        "[upscale] {} session failed on {}: {accelerator_error}; retrying on CPU",
                        selected_backend,
                        selected_backend.device_description(),
                    );
                    selected_backend = ExecutionBackend::Cpu;
                    backend::cpu_session_builder()
                        .context("Failed to create CPU fallback session")?
                        .commit_from_file(&self.model_path)
                        .with_context(|| {
                            format!(
                                "Hardware acceleration failed ({accelerator_error}); CPU fallback could not load model: {}",
                                self.model_path.display()
                            )
                        })?
                }
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!("Failed to load model: {}", self.model_path.display())
                    });
                }
            };
            profile::event(format_args!(
                "session committed: backend={}, elapsed_ms={:.3}",
                selected_backend,
                profile::milliseconds(commit_started.elapsed()),
            ));

            let validation_started = Instant::now();
            Self::validate_session(&session, &self.model_path)?;
            profile::event(format_args!(
                "session validated: input={:?}, output={:?}, elapsed_ms={:.3}",
                session.inputs()[0].dtype(),
                session.outputs()[0].dtype(),
                profile::milliseconds(validation_started.elapsed()),
            ));
            eprintln!(
                "[upscale] Active accelerator: backend={}, device={}, model={}",
                selected_backend,
                selected_backend.device_description(),
                self.model_path.display(),
            );
            self.backend = selected_backend;
            self.session = Some(session);
        }

        self.session.as_mut().context("Session is not initialized")
    }

    fn upscale(&mut self, input: Self::Input) -> anyhow::Result<Self::Output> {
        self.run(input, None)
    }
}

impl OnnxRuntime {
    pub(crate) fn finish_profiling(&mut self) {
        if !profile::enabled() {
            return;
        }
        if let Some(session) = &mut self.session {
            match session.end_profiling() {
                Ok(path) => profile::event(format_args!("ONNX Runtime trace saved: {path}")),
                Err(error) => profile::event(format_args!(
                    "ONNX Runtime trace could not be finalized: {error}"
                )),
            }
        }
    }

    pub(crate) fn tensorize_rgba(&self, rgba: &image::RgbaImage) -> anyhow::Result<OnnxInput> {
        let (width, height) = rgba.dimensions();
        let mut tensor = Array4::<f32>::zeros((1, 4, height as usize, width as usize));
        let plane = width as usize * height as usize;
        let destination = tensor
            .as_slice_mut()
            .context("Input tensor is not contiguous")?;
        for (index, pixel) in rgba.as_raw().chunks_exact(4).enumerate() {
            destination[index] = f32::from(pixel[0]) / 255.0;
            destination[plane + index] = f32::from(pixel[1]) / 255.0;
            destination[plane * 2 + index] = f32::from(pixel[2]) / 255.0;
            destination[plane * 3 + index] = f32::from(pixel[3]) / 255.0;
        }
        Ok(OnnxInput { rgba: tensor })
    }

    fn run(
        &mut self,
        input: OnnxInput,
        run_options: Option<&Arc<RunOptions>>,
    ) -> anyhow::Result<Array4<f32>> {
        let session = self.ensure_session()?;
        let channels = match session.inputs()[0].dtype() {
            ValueType::Tensor { shape, .. } => shape[1],
            _ => unreachable!("session metadata was validated when it was loaded"),
        };

        let outputs = if channels == 3 {
            let rgb = input.rgba.slice(s![.., 0..3, .., ..]);
            let input = TensorRef::from_array_view(rgb)?;
            match run_options {
                Some(options) => session.run_with_options(inputs![input], options)?,
                None => session.run(inputs![input])?,
            }
        } else {
            let input = TensorRef::from_array_view(&input.rgba)?;
            match run_options {
                Some(options) => session.run_with_options(inputs![input], options)?,
                None => session.run(inputs![input])?,
            }
        };

        let model_output = outputs
            .values()
            .next()
            .context("Model returned no outputs")?
            .try_extract_array::<f32>()?
            .to_owned()
            .into_dimensionality::<ndarray::Ix4>()?;

        if model_output.shape()[1] == 4 {
            return Ok(model_output);
        }
        if model_output.shape()[1] != 3 {
            bail!(
                "Model returned {} channels; expected RGB or RGBA",
                model_output.shape()[1]
            );
        }

        let (_, _, output_height, output_width) = model_output.dim();
        let (_, _, input_height, input_width) = input.rgba.dim();
        let mut rgba = Array4::<f32>::zeros((1, 4, output_height, output_width));
        rgba.slice_mut(s![.., 0..3, .., ..]).assign(&model_output);
        for y in 0..output_height {
            for x in 0..output_width {
                let source_y = y * input_height / output_height;
                let source_x = x * input_width / output_width;
                rgba[[0, 3, y, x]] = input.rgba[[0, 3, source_y, source_x]];
            }
        }

        Ok(rgba)
    }
}

#[cfg(test)]
mod tests {
    use image::{DynamicImage, Rgb, RgbImage, Rgba, RgbaImage};
    use ort::value::{Shape, SymbolicDimensions};

    use super::*;

    #[test]
    fn tensorize_adds_opaque_alpha_to_rgb_images() {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(1, 1, Rgb([0, 127, 255])));
        let runtime = OnnxRuntime::new("model.onnx");

        let tensor = runtime.tensorize(&image).unwrap();

        assert_eq!(tensor.rgba.shape(), &[1, 4, 1, 1]);
        assert_eq!(tensor.rgba[[0, 0, 0, 0]], 0.0); // это типо tensor[0][0][0][0] просто в одном []
        assert_eq!(tensor.rgba[[0, 1, 0, 0]], 127.0 / 255.0);
        assert_eq!(tensor.rgba[[0, 2, 0, 0]], 1.0);
        assert_eq!(tensor.rgba[[0, 3, 0, 0]], 1.0);
    }

    #[test]
    fn tensorize_preserves_rgba_channels_in_nchw_order() {
        let image = DynamicImage::ImageRgba8(RgbaImage::from_pixel(1, 1, Rgba([10, 20, 30, 40])));
        let runtime = OnnxRuntime::new("model.onnx");

        let tensor = runtime.tensorize(&image).unwrap();

        assert_eq!(tensor.rgba.shape(), &[1, 4, 1, 1]);
        assert_eq!(tensor.rgba[[0, 0, 0, 0]], 10.0 / 255.0);
        assert_eq!(tensor.rgba[[0, 1, 0, 0]], 20.0 / 255.0);
        assert_eq!(tensor.rgba[[0, 2, 0, 0]], 30.0 / 255.0);
        assert_eq!(tensor.rgba[[0, 3, 0, 0]], 40.0 / 255.0);
    }

    #[test]
    fn validation_rejects_model_with_unsupported_input_channels() {
        let value_type = ValueType::Tensor {
            ty: TensorElementType::Float32,
            shape: Shape::new([1, 2, -1, -1]),
            dimension_symbols: SymbolicDimensions::new([
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]),
        };

        let error =
            OnnxRuntime::validate_tensor_type(&value_type, "input", Path::new("model.onnx"), true)
                .unwrap_err();

        assert!(error.to_string().contains("expects 2 input channels"));
    }

    #[test]
    fn validation_rejects_batched_image_models() {
        let value_type = ValueType::Tensor {
            ty: TensorElementType::Float32,
            shape: Shape::new([2, 3, -1, -1]),
            dimension_symbols: SymbolicDimensions::new([
                String::new(),
                String::new(),
                String::new(),
                String::new(),
            ]),
        };

        let error =
            OnnxRuntime::validate_tensor_type(&value_type, "input", Path::new("model.onnx"), true)
                .unwrap_err();
        assert!(error.to_string().contains("batch dimension"));
    }
}

#[cfg(all(test, feature = "upscale-tests"))]
mod upscale_tests {
    use std::path::PathBuf;

    use image::{DynamicImage, Rgb, RgbImage, RgbaImage};

    use super::*;

    fn upscale_fixture(model_name: &str, scale: u32, tile_size: u32, padding: u32) {
        let core_size = tile_size - padding * 2;

        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/testdata");
        let image = image::open(fixtures.join("images/testupscale.jpg"))
            .unwrap()
            .crop_imm(0, 0, 33, 33);
        let (width, height) = (image.width(), image.height());
        let source = image.to_rgba8();
        let mut runtime = OnnxRuntime::new(fixtures.join("models/onnx").join(model_name));
        let mut upscaled = RgbImage::new(width * scale, height * scale);

        for tile_y in (0..height).step_by(core_size as usize) {
            for tile_x in (0..width).step_by(core_size as usize) {
                let tile = RgbaImage::from_fn(tile_size, tile_size, |x, y| {
                    let source_x = (tile_x as i64 + x as i64 - padding as i64)
                        .clamp(0, width as i64 - 1) as u32;
                    let source_y = (tile_y as i64 + y as i64 - padding as i64)
                        .clamp(0, height as i64 - 1) as u32;
                    *source.get_pixel(source_x, source_y)
                });
                let input = runtime.tensorize(&DynamicImage::ImageRgba8(tile)).unwrap();
                let output = runtime.upscale(input).unwrap();

                assert_eq!(
                    output.shape(),
                    &[
                        1,
                        4,
                        (tile_size * scale) as usize,
                        (tile_size * scale) as usize
                    ]
                );
                assert!(output.iter().all(|value| value.is_finite()));

                let output_tile_width = (width - tile_x).min(core_size) * scale;
                let output_tile_height = (height - tile_y).min(core_size) * scale;
                for y in 0..output_tile_height {
                    for x in 0..output_tile_width {
                        let channel = |index| {
                            let model_x = x + padding * scale;
                            let model_y = y + padding * scale;
                            (output[[0, index, model_y as usize, model_x as usize]].clamp(0.0, 1.0)
                                * 255.0)
                                .round() as u8
                        };
                        upscaled.put_pixel(
                            tile_x * scale + x,
                            tile_y * scale + y,
                            Rgb([channel(0), channel(1), channel(2)]),
                        );
                    }
                }
            }
        }

        assert_eq!(upscaled.dimensions(), (width * scale, height * scale));
    }

    #[test]
    fn realesrgan_upscales_fixture_with_2x_and_4x_models() {
        upscale_fixture("Real-ESRGAN_x2plus.onnx", 2, 64, 4);
        upscale_fixture("Real-ESRGAN-x4plus.onnx", 4, 128, 8);
    }

    #[test]
    fn converted_hat_upscales_fixture_tile_four_times() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tools/testdata");
        let image = image::open(fixtures.join("images/testupscale.jpg"))
            .unwrap()
            .crop_imm(0, 0, 64, 64);
        let mut runtime = OnnxRuntime::new(fixtures.join("models/onnx/Real_HAT_GAN_SRx4.onnx"));
        let input = runtime.tensorize(&image).unwrap();
        let output = runtime.upscale(input).unwrap();

        assert_eq!(output.shape(), &[1, 4, 256, 256]);
        assert!(output.iter().all(|value| value.is_finite()));
    }
}
