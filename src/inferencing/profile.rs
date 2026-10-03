use std::{
    ffi::OsString,
    fmt,
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Default)]
struct Configuration {
    enabled: bool,
    tile_size: Option<u32>,
}

static CONFIGURATION: OnceLock<Configuration> = OnceLock::new();

pub(crate) fn configure(arguments: impl IntoIterator<Item = OsString>) {
    let mut configuration = Configuration::default();
    for argument in arguments {
        let argument = argument.to_string_lossy();
        if argument == "--profile-inference" {
            configuration.enabled = true;
        } else if let Some(value) = argument.strip_prefix("--tile-size=") {
            match value.parse::<u32>() {
                Ok(size) if (64..=1024).contains(&size) => configuration.tile_size = Some(size),
                _ => eprintln!(
                    "[upscale] Ignoring invalid {argument}; tile size must be between 64 and 1024"
                ),
            }
        }
    }
    let _ = CONFIGURATION.set(configuration);
    if configuration.enabled {
        eprintln!(
            "[profile] inference profiling enabled; dynamic_tile_size={}",
            configuration
                .tile_size
                .map_or_else(|| "automatic".to_string(), |size| format!("{size}x{size}")),
        );
    }
}

fn configuration() -> Configuration {
    CONFIGURATION.get().copied().unwrap_or_default()
}

pub(crate) fn enabled() -> bool {
    configuration().enabled
}

pub(crate) fn tile_size_override() -> Option<u32> {
    configuration().tile_size
}

pub(crate) fn event(arguments: fmt::Arguments<'_>) {
    if enabled() {
        eprintln!("[profile] {arguments}");
    }
}

pub(crate) fn milliseconds(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

pub(crate) fn ort_profile_prefix() -> Option<PathBuf> {
    if !enabled() {
        return None;
    }
    let directory = directories::ProjectDirs::from("com", "SuperPuper", env!("CARGO_PKG_NAME"))?
        .data_local_dir()
        .join("profiles");
    if let Err(error) = std::fs::create_dir_all(&directory) {
        eprintln!("[profile] Could not create profile directory: {error}");
        return None;
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    Some(directory.join(format!("onnx-{timestamp}")))
}

#[cfg(test)]
mod tests {
    use super::milliseconds;
    use std::time::Duration;

    #[test]
    fn duration_is_reported_in_fractional_milliseconds() {
        assert_eq!(milliseconds(Duration::from_micros(1_500)), 1.5);
    }
}
