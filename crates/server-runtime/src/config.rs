mod parse;

use minimium_commands::{CommandError, PropertySource, ResolvedConfigValue};
use std::{collections::BTreeMap, path::PathBuf, thread};

pub const MAX_PLAYERS: usize = 512;
pub const DEFAULT_AUTOSAVE_SECONDS: u64 = 300;

pub fn default_sector_workers() -> usize {
    thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .saturating_sub(1)
        .max(1)
        .min(4)
}

#[derive(Clone, Copy)]
pub enum LaneSelection {
    Fixed(usize),
    Auto,
}

impl LaneSelection {
    pub fn count(self) -> usize {
        match self {
            Self::Fixed(count) => count,
            Self::Auto => thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(64),
        }
    }
}

pub struct ServerConfig {
    pub material_package_path: Option<PathBuf>,
    pub material_http_port: u16,
    pub climate_scenario_path: Option<PathBuf>,
    pub climate_relief_influence: Option<f64>,
    pub seed: Option<u128>,
    pub lanes: LaneSelection,
    pub sector_workers: usize,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub admin_token: Option<String>,
    pub startup_attempt_id: Option<String>,
    pub diagnostics_run_id: Option<String>,
    pub map_path: Option<PathBuf>,
    pub world_definition_path: Option<PathBuf>,
    pub planet_config_path: Option<PathBuf>,
    pub environment_config_path: Option<PathBuf>,
    pub bind_address: String,
    pub game_port: u16,
    pub max_players: usize,
    pub autosave_seconds: u64,
    pub bolid_interval: Option<(u64, u64)>,
    pub console_enabled: bool,
    pub dev_mode: bool,
    pub resolved_settings: BTreeMap<String, ResolvedConfigValue>,
}

pub(crate) fn parse_bolid_interval(value: &str) -> Option<Option<(u64, u64)>> {
    if value == "off" {
        return Some(None);
    }
    let (minimum, maximum) = value.split_once("..")?;
    let minimum = minimum.parse::<u64>().ok()?;
    let maximum = maximum.parse::<u64>().ok()?;
    (10..=3600).contains(&minimum).then_some(())?;
    (minimum..=3600)
        .contains(&maximum)
        .then_some(Some((minimum, maximum)))
}

pub use self::ServerConfig as RuntimeServerConfig;

impl ServerConfig {
    pub fn parse(args: &[String]) -> Result<Self, CommandError> {
        parse::parse(args, &BTreeMap::new())
    }

    pub fn parse_with_environment(
        args: &[String],
        process_environment: &BTreeMap<String, String>,
    ) -> Result<Self, CommandError> {
        let mut config = parse::parse(args, process_environment)?;
        config.admin_token = process_environment.get("MINIMIUM_ADMIN_TOKEN").cloned();
        Ok(config)
    }
}

pub fn offline_command_with_environment(
    args: &[String],
    process_environment: &BTreeMap<String, String>,
) -> Result<Option<String>, CommandError> {
    parse::offline_command(args, process_environment)
}

pub fn help_text() -> String {
    parse::help_text()
}

pub(crate) fn property_value<'a>(
    settings: &'a BTreeMap<String, ResolvedConfigValue>,
    key: &str,
) -> Option<(&'a str, Option<&'a str>, PropertySource)> {
    settings.get(key).map(|value| {
        (
            value.requested.as_str(),
            value.default.as_deref(),
            value.source,
        )
    })
}
