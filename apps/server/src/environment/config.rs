use minimium_core::time::{ClockMode, OfflinePolicy, TimeScalePpm, WorldTimeMs};
use serde::Deserialize;
use std::{io, path::Path};

pub(crate) const MAX_CONFIG_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EnvironmentConfig {
    pub(crate) mode: ClockMode,
    pub(crate) initial_utc: Option<WorldTimeMs>,
    pub(crate) scale_ppm: TimeScalePpm,
    pub(crate) offline_policy: OfflinePolicy,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            mode: ClockMode::RealTime,
            initial_utc: None,
            scale_ppm: TimeScalePpm::ONE_X,
            offline_policy: OfflinePolicy::Pause,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EnvironmentConfigFile {
    schema_version: u8,
    #[serde(default)]
    mode: ConfigMode,
    #[serde(default)]
    initial_utc: Option<String>,
    #[serde(default = "default_scale_ppm")]
    scale_ppm: u32,
    #[serde(default)]
    offline_policy: ConfigOfflinePolicy,
}

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum ConfigMode {
    #[default]
    RealTime,
    TimeScale,
}

#[derive(Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
enum ConfigOfflinePolicy {
    #[default]
    Pause,
    Advance,
}

pub(crate) fn load(path: &Path) -> io::Result<EnvironmentConfig> {
    let bytes = minimium_platform::files::read_bounded_file(path, MAX_CONFIG_BYTES)?;
    let source = std::str::from_utf8(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "environment config is not UTF-8",
        )
    })?;
    let raw: EnvironmentConfigFile = ron::from_str(source).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("environment config {}: {error}", path.display()),
        )
    })?;
    if raw.schema_version != 1 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "environment config schema_version must be 1",
        ));
    }
    let scale_ppm = TimeScalePpm::new(raw.scale_ppm).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("environment config: {error}"),
        )
    })?;
    let initial_utc = raw
        .initial_utc
        .as_deref()
        .map(minimium_platform::commands::parse_world_time_utc)
        .transpose()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mode = match raw.mode {
        ConfigMode::RealTime => ClockMode::RealTime,
        ConfigMode::TimeScale => ClockMode::TimeScale,
    };
    let offline_policy = match raw.offline_policy {
        ConfigOfflinePolicy::Pause => OfflinePolicy::Pause,
        ConfigOfflinePolicy::Advance => OfflinePolicy::Advance,
    };
    if mode == ClockMode::RealTime && initial_utc.is_some() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "real_time environment config must omit initial_utc",
        ));
    }
    if mode == ClockMode::RealTime && scale_ppm != TimeScalePpm::ONE_X {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "real_time environment config requires scale_ppm=1000000",
        ));
    }
    Ok(EnvironmentConfig {
        mode,
        initial_utc,
        scale_ppm,
        offline_policy,
    })
}

const fn default_scale_ppm() -> u32 {
    1_000_000
}
