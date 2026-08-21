use std::path::{Path, PathBuf};

use crate::inferencing::ModelRuntime;
use anyhow::{Context, bail};
use image::DynamicImage;
use ndarray::{Array4, s};
use ort::{
    inputs,
    session::Session,
    value::{TensorElementType, TensorRef, ValueType},
};

pub struct OnnxRuntime {
    session: Option<Session>,
    model_path: PathBuf,
}

pub struct OnnxInput {
    rgba: Array4<f32>,
}

impl OnnxRuntime {
    pub fn new(model_path: impl Into<PathBuf>) -> Self {
        Self {
            session: None,
            model_path: model_path.into(),
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

        let (width, height) = rgba.dimensions();

        let mut tensor = Array4::<f32>::zeros((1, 4, height as usize, width as usize));
        for (x, y, pixel) in rgba.enumerate_pixels() {
            for (channel, value) in pixel.0.into_iter().enumerate() {
                tensor[[0, channel, y as usize, x as usize]] = value as f32 / 255.0;
            }
        }

        Ok(OnnxInput { rgba: tensor })
    }

    fn ensure_session(&mut self) -> anyhow::Result<&mut Session> {
        if self.session.is_none() {
            let session = Session::builder()
                .context("Failed to create session builder")?
                .commit_from_file(&self.model_path)
                .with_context(|| format!("Failed to load model: {}", self.model_path.display()))?;

            Self::validate_session(&session, &self.model_path)?;
            self.session = Some(session);
        }

        self.session.as_mut().context("Session is not initialized")
    }

    fn upscale(&mut self, input: Self::Input) -> anyhow::Result<Self::Output> {
        let session = self.ensure_session()?;
        let channels = match session.inputs()[0].dtype() {
            ValueType::Tensor { shape, .. } => shape[1],
            _ => unreachable!("session metadata was validated when it was loaded"),
        };

        let outputs = if channels == 3 {
            let rgb = input.rgba.slice(s![.., 0..3, .., ..]);
            session.run(inputs![TensorRef::from_array_view(rgb)?])?
        } else {
            session.run(inputs![TensorRef::from_array_view(&input.rgba)?])?
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
}

#[cfg(all(test, feature = "upscale-tests"))]
mod upscale_tests {
    use std::path::PathBuf;

    use image::{DynamicImage, Rgb, RgbImage, RgbaImage};

    use super::*;

    fn upscale_fixture(
        model_name: &str,
        scale: u32,
        tile_size: u32,
        padding: u32,
        output_name: &str,
    ) {
        let core_size = tile_size - padding * 2;

        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testsdata");
        let image = image::open(fixtures.join("testupscale.jpg")).unwrap();
        let (width, height) = (image.width(), image.height());
        let source = image.to_rgba8();
        let mut runtime = OnnxRuntime::new(fixtures.join(model_name));
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
        upscaled.save(fixtures.join(output_name)).unwrap();
    }

    #[test]
    fn realesrgan_upscales_fixture_with_2x_and_4x_models() {
        upscale_fixture(
            "Real-ESRGAN_x2plus.onnx",
            2,
            64,
            4,
            "testupscale_upscaled2x.jpg",
        );
        upscale_fixture(
            "Real-ESRGAN-x4plus.onnx",
            4,
            128,
            8,
            "testupscale_upscaled4x.jpg",
        );
    }

    #[test]
    fn converted_hat_upscales_fixture_tile_four_times() {
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testsdata");
        let image = image::open(fixtures.join("testupscale.jpg"))
            .unwrap()
            .crop_imm(0, 0, 64, 64);
        let mut runtime = OnnxRuntime::new(fixtures.join("Real_HAT_GAN_SRx4.onnx"));
        let input = runtime.tensorize(&image).unwrap();
        let output = runtime.upscale(input).unwrap();

        assert_eq!(output.shape(), &[1, 4, 256, 256]);
        assert!(output.iter().all(|value| value.is_finite()));
    }

    #[test]
    fn hat_upscales_complete_fixture_four_times() {
        upscale_fixture("Real_HAT_GAN_SRx4.onnx", 4, 64, 16, "testupscale_hat4x.jpg");
    }
}
