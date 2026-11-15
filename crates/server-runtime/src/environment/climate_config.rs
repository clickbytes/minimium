//! Persistent climate scenario setup, separate from material transport.
use crate::config::ServerConfig;
use minimium_environment::climate::ClimateScenario;
use std::{io, path::PathBuf};

pub(crate) struct ClimateConfiguration {
    pub(crate) scenario: ClimateScenario,
    destination: PathBuf,
    staged: Option<Vec<u8>>,
}

impl ClimateConfiguration {
    pub(crate) fn load(config: &ServerConfig) -> io::Result<Self> {
        let destination = config.data_dir.join("world.climate.ron");
        let saved = destination.exists();
        let source = if saved {
            if config.climate_scenario_path.is_some() {
                return Err(io::Error::other(
                    "--climate-scenario initializes a new world only",
                ));
            }
            minimium_asset_io::materials::read_bounded(&destination, 16 * 1024)
                .map_err(io::Error::other)?
        } else if let Some(path) = &config.climate_scenario_path {
            minimium_asset_io::materials::read_bounded(path, 16 * 1024).map_err(io::Error::other)?
        } else {
            ron::to_string(&ClimateScenario::default())
                .map_err(io::Error::other)?
                .into_bytes()
        };
        let mut scenario: ClimateScenario =
            ron::from_str(std::str::from_utf8(&source).map_err(io::Error::other)?)
                .map_err(io::Error::other)?;
        let changed = config
            .climate_relief_influence
            .is_some_and(|value| value != scenario.relief_influence);
        if let Some(value) = config.climate_relief_influence {
            scenario.relief_influence = value;
        }
        scenario.validate().map_err(io::Error::other)?;
        let staged = if !saved || changed {
            Some(
                ron::to_string(&scenario)
                    .map_err(io::Error::other)?
                    .into_bytes(),
            )
        } else {
            None
        };
        Ok(Self {
            scenario,
            destination,
            staged,
        })
    }

    pub(crate) fn commit(&self) -> io::Result<()> {
        let Some(bytes) = &self.staged else {
            return Ok(());
        };
        minimium_platform::files::atomic_write_file(
            &self.destination.with_extension("tmp"),
            &self.destination,
            bytes,
        )
    }
}
