mod parse;

use minimium_commands::{CommandError, PropertySource, ResolvedConfigValue};
use std::{collections::BTreeMap, path::PathBuf, thread};

pub const MAX_PLAYERS: usize = 512;
pub const DEFAULT_AUTOSAVE_SECONDS: u64 = 300;

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
    pub data_dir: PathBuf,
    pub map_path: Option<PathBuf>,
    pub world_definition_path: Option<PathBuf>,
    pub planet_config_path: Option<PathBuf>,
    pub environment_config_path: Option<PathBuf>,
    pub bind_address: String,
    pub game_port: u16,
    pub max_players: usize,
    pub autosave_seconds: u64,
    pub console_enabled: bool,
    pub resolved_settings: BTreeMap<String, ResolvedConfigValue>,
}

impl ServerConfig {
    pub fn parse(args: &[String]) -> Result<Self, CommandError> {
        parse::parse(args)
    }
}

pub fn offline_command(args: &[String]) -> Result<Option<String>, CommandError> {
    parse::offline_command(args)
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
