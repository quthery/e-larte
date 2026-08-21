pub mod onnx;

use image::DynamicImage;

pub trait ModelRuntime {
    type Input;
    type Output;
    type Session;

    fn tensorize(&self, image: &DynamicImage) -> anyhow::Result<Self::Input>;

    fn ensure_session(&mut self) -> anyhow::Result<&mut Self::Session>;

    fn upscale(&mut self, input: Self::Input) -> anyhow::Result<Self::Output>;
}
