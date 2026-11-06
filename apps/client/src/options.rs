pub(crate) use minimium_client_runtime::ThreadLane;
mod catalog;
use catalog::OPTIONS;
use minimium_commands::{
    format_cli_help, parse_cli, CliParseOutcome, CommandError, CommandErrorCode, PropertySource,
};
use minimium_renderer::quality::SkyQuality;
use std::{collections::BTreeMap, path::PathBuf};

const DEFAULT_LOCAL_DATA_DIR: &str = ".minimium-local";

/// Command-line options shared by the desktop and text clients.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientOptions {
    pub connect: Option<String>,
    pub map_path: Option<PathBuf>,
    pub world_definition_path: Option<PathBuf>,
    pub demo: bool,
    pub require_earth: bool,
    pub console: bool,
    pub windowed: bool,
    pub help: bool,
    pub version: bool,
    pub threads: ThreadLane,
    pub sky_quality: SkyQuality,
    pub profile_dir: Option<PathBuf>,
    pub server_data_dir: Option<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub server_config_path: Option<PathBuf>,
    pub server_settings: BTreeMap<String, String>,
    pub server_environment: BTreeMap<String, String>,
    pub threads_explicit: bool,
    pub startup_fov: f32,
    pub startup_mouse_sensitivity: f32,
    pub startup_sky_names: bool,
    pub startup_cloud_detail: u8,
    pub startup_atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug,
    pub startup_terrain_debug: minimium_renderer::terrain_debug::TerrainDebug,
    pub startup_cloud_debug: minimium_environment::weather::CloudDebug,
    pub startup_sky_demo: bool,
    pub preference_sources: BTreeMap<String, PropertySource>,
    pub config_file_settings: BTreeMap<String, String>,
}

impl ClientOptions {
    /// Parse client arguments, excluding the executable name.
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, CommandError> {
        let values = match parse_cli(args, "minimium-client [options]", OPTIONS)? {
            CliParseOutcome::Help => return Ok(Self::defaults(true)),
            CliParseOutcome::Parsed(values) => values,
        };
        if !values.positionals().is_empty() {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--",
                "unexpected positional arguments",
            ));
        }
        let mut options = Self::defaults(false);
        options.connect = values.get("--connect").map(str::to_owned);
        options.map_path = values.get("--map").map(PathBuf::from);
        options.world_definition_path = values.get("--world-definition").map(PathBuf::from);
        options.profile_dir = values.get("--profile-dir").map(PathBuf::from);
        options.server_data_dir = values.get("--server-data-dir").map(PathBuf::from);
        options.demo = values.contains("--demo");
        options.require_earth = values.contains("--require-earth");
        options.console |= values.contains("--console");
        options.windowed = values.contains("--windowed");
        options.version = values.contains("--version");
        options.threads = values
            .get("--threads")
            .map(ThreadLane::parse)
            .transpose()
            .map_err(|reason| {
                CommandError::new(
                    CommandErrorCode::InvalidValue,
                    Some("--threads"),
                    reason,
                    Some(help_text()),
                )
            })?
            .unwrap_or(ThreadLane::One);
        options.config_path = values.get("--config").map(PathBuf::from);
        options.server_config_path = values.get("--server-config").map(PathBuf::from);
        options.threads_explicit = values.contains("--threads");

        for assignment in values.get_all("--server-set").flatten() {
            let (key, value) = assignment.split_once('=').ok_or_else(|| {
                cli_error(
                    CommandErrorCode::InvalidValue,
                    "--server-set",
                    "expected KEY=VALUE",
                )
            })?;
            if key.is_empty() {
                return Err(cli_error(
                    CommandErrorCode::InvalidValue,
                    "--server-set",
                    "setting key cannot be empty",
                ));
            }
            if options
                .server_settings
                .insert(key.to_owned(), value.to_owned())
                .is_some()
            {
                return Err(cli_error(
                    CommandErrorCode::DuplicateOption,
                    key,
                    "the same server setting was specified more than once",
                ));
            }
        }
        if options.server_data_dir.is_some()
            && options.server_settings.contains_key("server.data_dir")
        {
            return Err(cli_error(
                CommandErrorCode::DuplicateOption,
                "server.data_dir",
                "--server-data-dir conflicts with --server-set server.data_dir=...",
            ));
        }
        for (key, value) in [
            (
                "world.map",
                options
                    .map_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
            ),
            (
                "world.definition",
                options
                    .world_definition_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned()),
            ),
            (
                "server.threads",
                options
                    .threads_explicit
                    .then(|| options.threads.as_str().to_owned()),
            ),
        ] {
            if let Some(value) = value {
                if options
                    .server_settings
                    .insert(key.to_owned(), value)
                    .is_some()
                {
                    return Err(cli_error(
                        CommandErrorCode::DuplicateOption,
                        key,
                        "client launch option conflicts with --server-set",
                    ));
                }
            }
        }
        options.server_environment = mapped_local_server_environment();

        let mut settings = BTreeMap::new();
        for value in values.get_all("--set") {
            let value = value.ok_or_else(|| {
                cli_error(
                    CommandErrorCode::MissingValue,
                    "--set",
                    "--set requires KEY=VALUE",
                )
            })?;
            let (key, value) = value.split_once('=').ok_or_else(|| {
                cli_error(
                    CommandErrorCode::InvalidValue,
                    "--set",
                    "--set requires KEY=VALUE",
                )
            })?;
            if key.is_empty() {
                return Err(cli_error(
                    CommandErrorCode::InvalidValue,
                    "--set",
                    "setting key cannot be empty",
                ));
            }
            if settings.insert(key.to_owned(), value.to_owned()).is_some() {
                return Err(cli_error(
                    CommandErrorCode::DuplicateOption,
                    key,
                    "the same setting key was specified more than once",
                ));
            }
        }
        if values.contains("--sky-quality") && settings.contains_key("client.graphics.sky_quality")
        {
            return Err(cli_error(
                CommandErrorCode::DuplicateOption,
                "client.graphics.sky_quality",
                "--sky-quality conflicts with --set for the same setting",
            ));
        }
        if options.version {
            return Ok(options);
        }
        if let Some(quality) = values.get("--sky-quality") {
            settings.insert("client.graphics.sky_quality".to_owned(), quality.to_owned());
        }
        let preferences =
            crate::config::resolve_preferences(options.config_path.as_deref(), &settings)?;
        options.startup_fov = preferences.fov_degrees;
        options.startup_mouse_sensitivity = preferences.mouse_sensitivity;
        options.sky_quality = preferences.sky_quality;
        options.startup_sky_names = preferences.sky_names;
        options.startup_cloud_detail = preferences.cloud_detail;
        options.startup_atmosphere_debug = preferences.atmosphere_debug;
        options.startup_terrain_debug = preferences.terrain_debug;
        options.startup_cloud_debug = preferences.cloud_debug;
        options.startup_sky_demo = preferences.sky_demo;
        options.preference_sources = preferences.sources;
        options.config_file_settings = preferences.file_settings;

        if options.demo && options.connect.is_some() {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--demo",
                "--demo cannot be used with --connect",
            ));
        }
        if options.connect.is_some()
            && (!options.server_settings.is_empty() || options.server_config_path.is_some())
        {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--server-config",
                "local server settings cannot be used with --connect",
            ));
        }
        if options.demo
            && (!options.server_settings.is_empty() || options.server_config_path.is_some())
        {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--server-config",
                "local server settings cannot be used with --demo",
            ));
        }
        if options.demo && options.console {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--demo",
                "--demo cannot be used with --console",
            ));
        }
        if options.require_earth && options.demo {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--require-earth",
                "--require-earth cannot be used with --demo",
            ));
        }
        if options.require_earth && options.console {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--require-earth",
                "--require-earth requires the desktop client",
            ));
        }
        if options.map_path.is_some() && options.world_definition_path.is_some() {
            return Err(cli_error(
                CommandErrorCode::InvalidArguments,
                "--map",
                "--map and --world-definition cannot be used together",
            ));
        }
        Ok(options)
    }

    fn defaults(help: bool) -> Self {
        Self {
            connect: None,
            map_path: None,
            world_definition_path: None,
            demo: false,
            require_earth: false,
            console: !cfg!(feature = "desktop"),
            windowed: false,
            help,
            version: false,
            threads: ThreadLane::One,
            sky_quality: SkyQuality::Standard,
            profile_dir: None,
            server_data_dir: None,
            config_path: None,
            server_config_path: None,
            server_settings: BTreeMap::new(),
            server_environment: BTreeMap::new(),
            threads_explicit: false,
            startup_fov: 70.0,
            startup_mouse_sensitivity: 0.0025,
            startup_sky_names: false,
            startup_cloud_detail: 6,
            startup_atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug::Off,
            startup_terrain_debug: minimium_renderer::terrain_debug::TerrainDebug::Off,
            startup_cloud_debug: minimium_environment::weather::CloudDebug::Off,
            startup_sky_demo: false,
            preference_sources: BTreeMap::new(),
            config_file_settings: BTreeMap::new(),
        }
    }

    /// Resolve independent profile and server storage directories.
    pub fn paths(&self, map_fingerprint: u64) -> (PathBuf, PathBuf) {
        let default_dir = if self.map_path.is_some() || self.world_definition_path.is_some() {
            PathBuf::from(DEFAULT_LOCAL_DATA_DIR).join(format!("map-{map_fingerprint:016x}"))
        } else {
            PathBuf::from(DEFAULT_LOCAL_DATA_DIR)
        };
        (
            self.profile_dir
                .clone()
                .unwrap_or_else(|| default_dir.clone()),
            self.server_data_dir.clone().unwrap_or(default_dir),
        )
    }

    pub(crate) fn local_server_launch_config(
        &self,
        data_dir: PathBuf,
        command_threads: Option<ThreadLane>,
    ) -> Result<minimium_client_runtime::LocalServerLaunchConfig, CommandError> {
        let mut settings = self.server_settings.clone();
        if let Some(threads) = command_threads {
            if settings
                .insert("server.threads".to_owned(), threads.as_str().to_owned())
                .is_some()
            {
                return Err(cli_error(
                    CommandErrorCode::DuplicateOption,
                    "server.threads",
                    "host local thread option conflicts with --server-set",
                ));
            }
        }
        Ok(minimium_client_runtime::LocalServerLaunchConfig {
            data_dir,
            data_dir_explicit: self.server_data_dir.is_some(),
            config_path: self.server_config_path.clone(),
            settings,
            environment: self.server_environment.clone(),
            client_map_path: self.map_path.clone(),
            client_world_definition_path: self.world_definition_path.clone(),
        })
    }
}

fn mapped_local_server_environment() -> BTreeMap<String, String> {
    let mut mapped = [
        "MINIMIUM_SERVER_THREADS",
        "MINIMIUM_SERVER_DATA_DIR",
        "MINIMIUM_SERVER_BIND",
        "MINIMIUM_SERVER_GAME_PORT",
        "MINIMIUM_SERVER_MAX_PLAYERS",
        "MINIMIUM_SERVER_AUTOSAVE_INTERVAL",
        "MINIMIUM_SERVER_HEADLESS",
        "MINIMIUM_SERVER_SEED",
        "MINIMIUM_SERVER_PLANET_CONFIG",
        "MINIMIUM_SERVER_CLIMATE_RELIEF_INFLUENCE",
    ]
    .into_iter()
    .filter_map(|name| {
        std::env::var(name)
            .ok()
            .filter(|value| !value.is_empty())
            .map(|value| (name.to_owned(), value))
    })
    .collect::<BTreeMap<_, _>>();
    for (source, target) in [
        ("MINIMIUM_LOCAL_SEED", "MINIMIUM_SERVER_SEED"),
        (
            "MINIMIUM_LOCAL_PLANET_CONFIG",
            "MINIMIUM_SERVER_PLANET_CONFIG",
        ),
        (
            "MINIMIUM_LOCAL_CLIMATE_RELIEF_INFLUENCE",
            "MINIMIUM_SERVER_CLIMATE_RELIEF_INFLUENCE",
        ),
    ] {
        if !mapped.contains_key(target) {
            if let Some(value) = std::env::var(source).ok().filter(|value| !value.is_empty()) {
                mapped.insert(target.to_owned(), value);
            }
        }
    }
    mapped
}

pub(crate) fn help_text() -> String {
    format_cli_help("minimium-client [options]", OPTIONS)
}

fn cli_error(
    code: CommandErrorCode,
    subject: impl Into<String>,
    reason: impl Into<String>,
) -> CommandError {
    CommandError::new(code, Some(subject.into()), reason, Some(help_text()))
}
