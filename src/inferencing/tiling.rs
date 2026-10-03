use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, bail};
use image::{DynamicImage, RgbaImage};
use ndarray::Array4;
use ort::session::RunOptions;

use super::{OnnxRuntime, profile};

const MAX_OUTPUT_BYTES: usize = 1024 * 1024 * 1024;
const DEFAULT_DYNAMIC_TILE: u32 = 256;

pub(crate) struct Cancellation {
    cancelled: AtomicBool,
    run_options: Arc<RunOptions>,
}

impl Cancellation {
    pub(crate) fn new() -> anyhow::Result<Self> {
        Ok(Self {
            cancelled: AtomicBool::new(false),
            run_options: Arc::new(RunOptions::new().context("Failed to create inference options")?),
        })
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        let _ = self.run_options.terminate();
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

pub(crate) struct TiledOutput {
    pub(crate) pixels: RgbaImage,
    pub(crate) scale: u32,
}

struct TileRun {
    output: Array4<f32>,
    tensorize: Duration,
    inference: Duration,
}

#[derive(Default)]
struct Totals {
    preparation: Duration,
    tensorize: Duration,
    inference: Duration,
    copy: Duration,
}

pub(crate) fn upscale(
    image: &DynamicImage,
    runtime: &mut OnnxRuntime,
    cancellation: &Cancellation,
    mut on_progress: impl FnMut(u32, u32),
) -> anyhow::Result<TiledOutput> {
    let total_started = Instant::now();
    let (fixed_width, fixed_height) = runtime.input_size()?;
    let dynamic_tile = profile::tile_size_override().unwrap_or(DEFAULT_DYNAMIC_TILE);
    let tile_width = fixed_width.unwrap_or(dynamic_tile);
    let tile_height = fixed_height.unwrap_or(dynamic_tile);
    let padding = (tile_width.min(tile_height) / 4).clamp(1, 16);
    if tile_width <= padding * 2 || tile_height <= padding * 2 {
        bail!("Model input is too small for safe tiled inference");
    }

    let conversion_started = Instant::now();
    let source = image.to_rgba8();
    let source_conversion = conversion_started.elapsed();
    let (width, height) = source.dimensions();
    let core_width = tile_width - padding * 2;
    let core_height = tile_height - padding * 2;
    let columns = width.div_ceil(core_width);
    let rows = height.div_ceil(core_height);
    let total = columns
        .checked_mul(rows)
        .context("Image needs too many tiles")?;
    profile::event(format_args!(
        "tiling configured: backend={}, source={}x{}, tile={}x{}, padding={}, core={}x{}, grid={}x{}, tiles={}, rgba_conversion_ms={:.3}, override={:?}",
        runtime.backend(),
        width,
        height,
        tile_width,
        tile_height,
        padding,
        core_width,
        core_height,
        columns,
        rows,
        total,
        profile::milliseconds(source_conversion),
        profile::tile_size_override(),
    ));

    let mut totals = Totals::default();
    let preparation_started = Instant::now();
    let first_tile = make_tile(&source, 0, 0, tile_width, tile_height, padding);
    let preparation = preparation_started.elapsed();
    let first_run = run_tile(runtime, &first_tile, cancellation)?;
    let scale = detect_scale(&first_run.output, tile_width, tile_height)?;
    let output_width = width
        .checked_mul(scale)
        .context("Output width is too large")?;
    let output_height = height
        .checked_mul(scale)
        .context("Output height is too large")?;
    let byte_len = (output_width as usize)
        .checked_mul(output_height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .context("Output image is too large")?;
    if byte_len > MAX_OUTPUT_BYTES {
        bail!("Output image would require more than 1 GiB of pixel memory");
    }
    let allocation_started = Instant::now();
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(byte_len)
        .context("Not enough memory for the output image")?;
    bytes.resize(byte_len, 0);
    let mut output = RgbaImage::from_raw(output_width, output_height, bytes)
        .context("Failed to allocate output image")?;
    profile::event(format_args!(
        "output allocated: dimensions={}x{}, bytes={}, elapsed_ms={:.3}, detected_scale={}x",
        output_width,
        output_height,
        byte_len,
        profile::milliseconds(allocation_started.elapsed()),
        scale,
    ));

    let copy_started = Instant::now();
    copy_core(
        &first_run.output,
        &mut output,
        0,
        0,
        width.min(core_width),
        height.min(core_height),
        padding,
        scale,
    )?;
    let copy = copy_started.elapsed();
    totals.preparation += preparation;
    totals.tensorize += first_run.tensorize;
    totals.inference += first_run.inference;
    totals.copy += copy;
    log_tile(1, total, 0, 0, preparation, &first_run, copy);
    let mut completed = 1;
    on_progress(completed, total);

    for tile_y in (0..height).step_by(core_height as usize) {
        for tile_x in (0..width).step_by(core_width as usize) {
            if tile_x == 0 && tile_y == 0 {
                continue;
            }
            if cancellation.is_cancelled() {
                bail!("Upscaling cancelled");
            }
            let preparation_started = Instant::now();
            let tile = make_tile(&source, tile_x, tile_y, tile_width, tile_height, padding);
            let preparation = preparation_started.elapsed();
            let tile_run = run_tile(runtime, &tile, cancellation)?;
            let tile_scale = detect_scale(&tile_run.output, tile_width, tile_height)?;
            if tile_scale != scale {
                bail!("Model output scale changed between tiles");
            }
            let copy_started = Instant::now();
            copy_core(
                &tile_run.output,
                &mut output,
                tile_x,
                tile_y,
                (width - tile_x).min(core_width),
                (height - tile_y).min(core_height),
                padding,
                scale,
            )?;
            let copy = copy_started.elapsed();
            completed += 1;
            totals.preparation += preparation;
            totals.tensorize += tile_run.tensorize;
            totals.inference += tile_run.inference;
            totals.copy += copy;
            log_tile(
                completed,
                total,
                tile_x,
                tile_y,
                preparation,
                &tile_run,
                copy,
            );
            on_progress(completed, total);
        }
    }

    profile::event(format_args!(
        "tiling totals: tiles={}, preparation_ms={:.3}, tensorize_ms={:.3}, inference_ms={:.3}, copy_ms={:.3}, total_ms={:.3}",
        total,
        profile::milliseconds(totals.preparation),
        profile::milliseconds(totals.tensorize),
        profile::milliseconds(totals.inference),
        profile::milliseconds(totals.copy),
        profile::milliseconds(total_started.elapsed()),
    ));
    Ok(TiledOutput {
        pixels: output,
        scale,
    })
}

fn run_tile(
    runtime: &mut OnnxRuntime,
    tile: &RgbaImage,
    cancellation: &Cancellation,
) -> anyhow::Result<TileRun> {
    if cancellation.is_cancelled() {
        bail!("Upscaling cancelled");
    }
    let tensorize_started = Instant::now();
    let input = runtime.tensorize_rgba(tile)?;
    let tensorize = tensorize_started.elapsed();
    let inference_started = Instant::now();
    let output = runtime
        .upscale_with_options(input, &cancellation.run_options)
        .map_err(|error| {
            if cancellation.is_cancelled() {
                anyhow::anyhow!("Upscaling cancelled")
            } else {
                error
            }
        })?;
    Ok(TileRun {
        output,
        tensorize,
        inference: inference_started.elapsed(),
    })
}

#[allow(clippy::too_many_arguments)]
fn log_tile(
    index: u32,
    total: u32,
    tile_x: u32,
    tile_y: u32,
    preparation: Duration,
    run: &TileRun,
    copy: Duration,
) {
    profile::event(format_args!(
        "tile {index}/{total}: origin=({tile_x},{tile_y}), prepare_ms={:.3}, tensorize_ms={:.3}, inference_ms={:.3}, copy_ms={:.3}, tile_total_ms={:.3}",
        profile::milliseconds(preparation),
        profile::milliseconds(run.tensorize),
        profile::milliseconds(run.inference),
        profile::milliseconds(copy),
        profile::milliseconds(preparation + run.tensorize + run.inference + copy),
    ));
}

fn make_tile(
    source: &RgbaImage,
    tile_x: u32,
    tile_y: u32,
    width: u32,
    height: u32,
    padding: u32,
) -> RgbaImage {
    let (source_width, source_height) = source.dimensions();
    RgbaImage::from_fn(width, height, |x, y| {
        let source_x =
            (tile_x as i64 + x as i64 - padding as i64).clamp(0, source_width as i64 - 1) as u32;
        let source_y =
            (tile_y as i64 + y as i64 - padding as i64).clamp(0, source_height as i64 - 1) as u32;
        *source.get_pixel(source_x, source_y)
    })
}

fn detect_scale(output: &Array4<f32>, input_width: u32, input_height: u32) -> anyhow::Result<u32> {
    let shape = output.shape();
    if shape.len() != 4 || shape[0] != 1 || shape[1] != 4 {
        bail!("Model returned shape {shape:?}; expected one RGBA image");
    }
    let output_width = u32::try_from(shape[3]).context("Model output is too wide")?;
    let output_height = u32::try_from(shape[2]).context("Model output is too tall")?;
    if output_width % input_width != 0 || output_height % input_height != 0 {
        bail!("Model output does not use an integer scale");
    }
    let horizontal = output_width / input_width;
    let vertical = output_height / input_height;
    if horizontal != vertical {
        bail!("Model uses different horizontal and vertical scales");
    }
    if !(2..=16).contains(&horizontal) {
        bail!("Model scale {horizontal}x is outside the supported 2x-16x range");
    }
    Ok(horizontal)
}

#[allow(clippy::too_many_arguments)]
fn copy_core(
    model_output: &Array4<f32>,
    output: &mut RgbaImage,
    tile_x: u32,
    tile_y: u32,
    core_width: u32,
    core_height: u32,
    padding: u32,
    scale: u32,
) -> anyhow::Result<()> {
    let shape = model_output.shape();
    let model_width = shape[3];
    let model_height = shape[2];
    let plane = model_width
        .checked_mul(model_height)
        .context("Model output plane is too large")?;
    let source = model_output
        .as_slice()
        .context("Model output tensor is not contiguous")?;
    let destination_width = output.width() as usize;
    let destination: &mut [u8] = output.as_mut();
    let scaled_padding = padding as usize * scale as usize;
    let scaled_core_width = core_width as usize * scale as usize;
    let scaled_core_height = core_height as usize * scale as usize;
    let destination_x = tile_x as usize * scale as usize;
    let destination_y = tile_y as usize * scale as usize;

    for y in 0..scaled_core_height {
        let model_row = (y + scaled_padding) * model_width + scaled_padding;
        let destination_row = (destination_y + y) * destination_width + destination_x;
        for x in 0..scaled_core_width {
            let source_index = model_row + x;
            let destination_index = (destination_row + x) * 4;
            destination[destination_index] = quantize(source[source_index]);
            destination[destination_index + 1] = quantize(source[plane + source_index]);
            destination[destination_index + 2] = quantize(source[plane * 2 + source_index]);
            destination[destination_index + 3] = quantize(source[plane * 3 + source_index]);
        }
    }
    Ok(())
}

#[inline]
fn quantize(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};
    use ndarray::Array4;

    use super::{copy_core, detect_scale};

    #[test]
    fn scale_is_derived_from_model_output() {
        assert_eq!(
            detect_scale(&Array4::zeros((1, 4, 128, 128)), 64, 64).unwrap(),
            2
        );
        assert_eq!(
            detect_scale(&Array4::zeros((1, 4, 256, 256)), 64, 64).unwrap(),
            4
        );
    }

    #[test]
    fn scale_rejects_asymmetric_and_fractional_outputs() {
        assert!(detect_scale(&Array4::zeros((1, 4, 128, 256)), 64, 64).is_err());
        assert!(detect_scale(&Array4::zeros((1, 4, 100, 100)), 64, 64).is_err());
    }

    #[test]
    fn contiguous_copy_converts_nchw_to_rgba() {
        let mut tensor = Array4::zeros((1, 4, 2, 2));
        tensor.slice_mut(ndarray::s![0, 0, .., ..]).fill(1.0);
        tensor.slice_mut(ndarray::s![0, 1, .., ..]).fill(0.5);
        tensor.slice_mut(ndarray::s![0, 2, .., ..]).fill(0.0);
        tensor.slice_mut(ndarray::s![0, 3, .., ..]).fill(1.0);
        let mut output = RgbaImage::new(2, 2);

        copy_core(&tensor, &mut output, 0, 0, 2, 2, 0, 1).unwrap();

        assert_eq!(*output.get_pixel(1, 1), Rgba([255, 128, 0, 255]));
    }
}
