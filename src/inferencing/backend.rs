use std::fmt;

use anyhow::Context;
use ort::{
    ep::ExecutionProvider,
    session::{Session, builder::SessionBuilder},
};
use serde::{Deserialize, Serialize};

use super::profile;

#[cfg(all(feature = "linux-cuda", feature = "linux-migraphx"))]
compile_error!("linux-cuda and linux-migraphx are mutually exclusive");

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[allow(dead_code)]
pub(crate) enum ExecutionBackend {
    CoreMl,
    DirectMl,
    Cuda,
    MiGraphX,
    Cpu,
}

impl fmt::Display for ExecutionBackend {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::CoreMl => "Core ML",
            Self::DirectMl => "DirectML",
            Self::Cuda => "CUDA",
            Self::MiGraphX => "MIGraphX",
            Self::Cpu => "CPU",
        })
    }
}

impl ExecutionBackend {
    pub(crate) fn available() -> Vec<Self> {
        let mut backends = Vec::new();

        #[cfg(target_os = "macos")]
        if ort::ep::CoreML::default().is_available().unwrap_or(false) {
            backends.push(Self::CoreMl);
        }
        #[cfg(target_os = "windows")]
        if ort::ep::DirectML::default().is_available().unwrap_or(false) {
            backends.push(Self::DirectMl);
        }
        #[cfg(all(target_os = "linux", feature = "linux-cuda"))]
        if ort::ep::CUDA::default().is_available().unwrap_or(false) {
            backends.push(Self::Cuda);
        }
        #[cfg(all(target_os = "linux", feature = "linux-migraphx"))]
        if ort::ep::MIGraphX::default().is_available().unwrap_or(false) {
            backends.push(Self::MiGraphX);
        }

        backends.push(Self::Cpu);
        backends
    }

    pub(crate) fn best_available() -> Self {
        Self::available().into_iter().next().unwrap_or(Self::Cpu)
    }

    fn provider_name(self) -> &'static str {
        match self {
            Self::CoreMl => "CoreMLExecutionProvider",
            Self::DirectMl => "DmlExecutionProvider",
            Self::Cuda => "CUDAExecutionProvider",
            Self::MiGraphX => "MIGraphXExecutionProvider",
            Self::Cpu => "CPUExecutionProvider",
        }
    }

    pub(crate) fn device_description(self) -> String {
        if let Ok(environment) = ort::environment::Environment::current()
            && let Some(device) = environment
                .devices()
                .find(|device| device.ep().ok() == Some(self.provider_name()))
        {
            let hardware = device.hardware_device();
            let vendor = hardware.vendor().unwrap_or("unknown vendor");
            return format!(
                "{vendor} {:?} (device id {}, {})",
                hardware.ty(),
                hardware.id(),
                self.provider_name(),
            );
        }

        match self {
            Self::CoreMl => "Apple CPU/GPU/Neural Engine (Core ML automatic selection)".into(),
            Self::DirectMl => "high-performance GPU (DirectML automatic adapter)".into(),
            Self::Cuda => "NVIDIA CUDA device 0".into(),
            Self::MiGraphX => "AMD ROCm device 0".into(),
            Self::Cpu => "system CPU".into(),
        }
    }
}

pub(crate) fn session_builder(
    preferred: ExecutionBackend,
) -> anyhow::Result<(SessionBuilder, ExecutionBackend)> {
    let mut builder = cpu_session_builder()?;

    #[cfg(target_os = "macos")]
    if preferred == ExecutionBackend::CoreMl {
        use ort::ep::{
            CoreML,
            coreml::{ComputeUnits, ModelFormat, SpecializationStrategy},
        };

        let cache = directories::ProjectDirs::from("com", "SuperPuper", env!("CARGO_PKG_NAME"))
            .map(|project| project.cache_dir().join("coreml"));
        if let Some(cache) = &cache {
            std::fs::create_dir_all(cache)
                .with_context(|| format!("Failed to create Core ML cache: {}", cache.display()))?;
        }

        let mut provider = CoreML::default()
            .with_model_format(ModelFormat::MLProgram)
            .with_compute_units(ComputeUnits::All)
            .with_specialization_strategy(SpecializationStrategy::FastPrediction);
        if let Some(cache) = cache {
            provider = provider.with_model_cache_dir(cache.to_string_lossy());
        }
        match provider.register(&mut builder) {
            Ok(()) => return Ok((builder, ExecutionBackend::CoreMl)),
            Err(error) => eprintln!("[upscale] Core ML unavailable: {error}; trying CPU"),
        }
    }

    #[cfg(target_os = "windows")]
    if preferred == ExecutionBackend::DirectMl {
        use ort::ep::{
            DirectML,
            directml::{DeviceFilter, PerformancePreference},
        };

        builder = builder
            .with_memory_pattern(false)
            .context("Failed to disable memory patterns for DirectML")?
            .with_parallel_execution(false)
            .context("Failed to select sequential execution for DirectML")?;
        let provider = DirectML::default()
            .with_device_filter(DeviceFilter::Gpu)
            .with_performance_preference(PerformancePreference::HighPerformance);
        match provider.register(&mut builder) {
            Ok(()) => return Ok((builder, ExecutionBackend::DirectMl)),
            Err(error) => eprintln!("[upscale] DirectML unavailable: {error}; trying CPU"),
        }
    }

    #[cfg(all(target_os = "linux", feature = "linux-cuda"))]
    if preferred == ExecutionBackend::Cuda {
        use ort::ep::{CUDA, cuda::ConvAlgorithmSearch};

        let provider = CUDA::default()
            .with_device_id(0)
            .with_conv_algorithm_search(ConvAlgorithmSearch::Heuristic)
            .with_conv_max_workspace(false)
            .with_tf32(false);
        match provider.register(&mut builder) {
            Ok(()) => return Ok((builder, ExecutionBackend::Cuda)),
            Err(error) => eprintln!("[upscale] CUDA unavailable: {error}; trying CPU"),
        }
    }

    #[cfg(all(target_os = "linux", feature = "linux-migraphx"))]
    if preferred == ExecutionBackend::MiGraphX {
        let provider = ort::ep::MIGraphX::default().with_device_id(0);
        match provider.register(&mut builder) {
            Ok(()) => return Ok((builder, ExecutionBackend::MiGraphX)),
            Err(error) => eprintln!("[upscale] MIGraphX unavailable: {error}; trying CPU"),
        }
    }

    Ok((builder, ExecutionBackend::Cpu))
}

pub(crate) fn cpu_session_builder() -> anyhow::Result<SessionBuilder> {
    let mut builder = Session::builder().context("Failed to create ONNX Runtime session")?;
    if let Some(prefix) = profile::ort_profile_prefix() {
        profile::event(format_args!(
            "ONNX Runtime trace enabled: prefix={}",
            prefix.display()
        ));
        builder = builder
            .with_profiling(prefix)
            .map_err(|error| anyhow::anyhow!("Failed to enable ONNX Runtime profiling: {error}"))?;
    }
    Ok(builder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_names_are_user_facing() {
        assert_eq!(ExecutionBackend::CoreMl.to_string(), "Core ML");
        assert_eq!(ExecutionBackend::DirectMl.to_string(), "DirectML");
        assert_eq!(ExecutionBackend::Cuda.to_string(), "CUDA");
        assert_eq!(ExecutionBackend::MiGraphX.to_string(), "MIGraphX");
        assert_eq!(ExecutionBackend::Cpu.to_string(), "CPU");
    }

    #[test]
    fn fallback_device_descriptions_are_not_empty() {
        for backend in [
            ExecutionBackend::CoreMl,
            ExecutionBackend::DirectMl,
            ExecutionBackend::Cuda,
            ExecutionBackend::MiGraphX,
            ExecutionBackend::Cpu,
        ] {
            assert!(!backend.device_description().is_empty());
        }
    }

    #[test]
    fn cpu_is_always_an_available_backend() {
        assert!(ExecutionBackend::available().contains(&ExecutionBackend::Cpu));
    }
}
