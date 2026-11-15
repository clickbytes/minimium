pub(crate) use minimium_client_runtime::ThreadLane;
mod catalog;
mod diagnostic_input;
mod paths;
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
    pub locale: minimium_localization::Locale,
    pub startup_locale: minimium_localization::Locale,
    pub connect: Option<String>,
    pub map_path: Option<PathBuf>,
    pub world_definition_path: Option<PathBuf>,
    pub demo: bool,
    pub dev_mode: bool,
    pub dev_stdin: bool,
    pub require_earth: bool,
    pub console: bool,
    pub windowed: bool,
    pub help: bool,
    pub version: bool,
    pub threads: ThreadLane,
    pub sky_quality: SkyQuality,
    pub profile_dir: Option<PathBuf>,
    pub log_dir: Option<PathBuf>,
    pub cache_dir: PathBuf,
    pub cache_limit_bytes: u64,
    pub server_data_dir: Option<PathBuf>,
    pub earth_package_dir: PathBuf,
    pub config_path: Option<PathBuf>,
    pub server_config_path: Option<PathBuf>,
    pub server_settings: BTreeMap<String, String>,
    pub server_environment: BTreeMap<String, String>,
    pub threads_explicit: bool,
    pub startup_fov: f32,
    pub startup_mouse_sensitivity: f32,
    pub input_bindings: String,
    pub startup_input_bindings: String,
    pub startup_sky_names: bool,
    pub startup_cloud_detail: u8,
    pub startup_atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug,
    pub startup_terrain_debug: minimium_renderer::terrain_debug::TerrainDebug,
    pub startup_cloud_debug: minimium_environment::weather::CloudDebug,
    pub startup_sky_demo: bool,
    pub meteor_preferences: BTreeMap<String, String>,
    pub preference_sources: BTreeMap<String, PropertySource>,
    pub config_file_settings: BTreeMap<String, String>,
    working_dir: Option<PathBuf>,
}

impl ClientOptions {
    pub fn resolved_log_dir(&self) -> Result<PathBuf, std::io::Error> {
        let working_dir = self.working_dir.as_deref().ok_or_else(|| {
            std::io::Error::other("client paths requested before process startup")
        })?;
        Ok(
            minimium_platform::app_paths::AppPaths::resolve_selected_from(
                working_dir,
                &[self
                    .log_dir
                    .as_deref()
                    .unwrap_or_else(|| std::path::Path::new("logs"))],
            )
            .remove(0),
        )
    }

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
        options.working_dir = Some(std::env::current_dir().map_err(|error| {
            cli_error(
                CommandErrorCode::InvalidValue,
                "paths",
                format!("could not capture process working directory: {error}"),
            )
        })?);
        options.connect = values.get("--connect").map(str::to_owned);
        options.map_path = values.get("--map").map(PathBuf::from);
        options.world_definition_path = values.get("--world-definition").map(PathBuf::from);
        options.profile_dir = values.get("--profile-dir").map(PathBuf::from);
        options.log_dir = values.get("--log-dir").map(PathBuf::from);
        options.cache_limit_bytes = values
            .get("--cache-limit-bytes")
            .map(str::parse)
            .transpose()
            .map_err(|_| {
                cli_error(
                    CommandErrorCode::InvalidValue,
                    "--cache-limit-bytes",
                    "expected a positive byte count",
                )
            })?
            .unwrap_or(minimium_asset_io::DEFAULT_CACHE_LIMIT_BYTES);
        if options.cache_limit_bytes == 0 {
            return Err(cli_error(
                CommandErrorCode::InvalidValue,
                "--cache-limit-bytes",
                "expected a positive byte count",
            ));
        }
        options.cache_dir = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_LOCAL_DATA_DIR))
            .join("minimium/materials");
        options.server_data_dir = values.get("--server-data-dir").map(PathBuf::from);
        options.earth_package_dir = std::env::var_os("MINIMIUM_EARTH_PACKAGE")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("third_party/gebco-2026"));
        options.demo = values.contains("--demo");
        options.dev_mode = values.contains("--dev");
        options.dev_stdin = values.contains("--dev-stdin");
        options.require_earth = values.contains("--require-earth");
        options.console |= values.contains("--console");
        diagnostic_input::validate(&mut options)?;
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
                CommandErrorCode::ConflictingOptions,
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
                        CommandErrorCode::ConflictingOptions,
                        key,
                        "client launch option conflicts with --server-set",
                    ));
                }
            }
        }
        options.server_environment = paths::mapped_local_server_environment();
        paths::resolve_client_paths(&mut options)?;

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
                CommandErrorCode::ConflictingOptions,
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
        options.locale = preferences.locale;
        options.startup_locale = preferences.locale;
        options.startup_mouse_sensitivity = preferences.mouse_sensitivity;
        options.input_bindings = preferences.input_bindings.clone();
        options.startup_input_bindings = preferences.input_bindings;
        options.sky_quality = preferences.sky_quality;
        options.startup_sky_names = preferences.sky_names;
        options.startup_cloud_detail = preferences.cloud_detail;
        options.startup_atmosphere_debug = preferences.atmosphere_debug;
        options.startup_terrain_debug = preferences.terrain_debug;
        options.startup_cloud_debug = preferences.cloud_debug;
        options.startup_sky_demo = preferences.sky_demo;
        options.meteor_preferences = preferences
            .resolved_values
            .iter()
            .filter(|(key, _)| key.starts_with("client.sky.meteors."))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
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
            && (!options.server_settings.is_empty()
                || options.server_config_path.is_some()
                || options.dev_mode)
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
            locale: minimium_localization::Locale::English,
            startup_locale: minimium_localization::Locale::English,
            connect: None,
            map_path: None,
            world_definition_path: None,
            demo: false,
            dev_mode: false,
            dev_stdin: false,
            require_earth: false,
            console: !cfg!(feature = "desktop"),
            windowed: false,
            help,
            version: false,
            threads: ThreadLane::One,
            sky_quality: SkyQuality::Standard,
            profile_dir: None,
            log_dir: None,
            cache_dir: PathBuf::from(DEFAULT_LOCAL_DATA_DIR).join("minimium/materials"),
            cache_limit_bytes: minimium_asset_io::DEFAULT_CACHE_LIMIT_BYTES,
            server_data_dir: None,
            earth_package_dir: PathBuf::from("third_party/gebco-2026"),
            config_path: None,
            server_config_path: None,
            server_settings: BTreeMap::new(),
            server_environment: BTreeMap::new(),
            threads_explicit: false,
            startup_fov: 70.0,
            startup_mouse_sensitivity: 0.0025,
            input_bindings: String::new(),
            startup_input_bindings: String::new(),
            startup_sky_names: false,
            startup_cloud_detail: 6,
            startup_atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug::Off,
            startup_terrain_debug: minimium_renderer::terrain_debug::TerrainDebug::Off,
            startup_cloud_debug: minimium_environment::weather::CloudDebug::Off,
            startup_sky_demo: false,
            meteor_preferences: BTreeMap::new(),
            preference_sources: BTreeMap::new(),
            config_file_settings: BTreeMap::new(),
            working_dir: None,
        }
    }

    /// Resolve independent profile and server storage directories.
    pub fn paths(
        &self,
        map_fingerprint: u64,
    ) -> Result<minimium_platform::app_paths::AppPaths, std::io::Error> {
        let default_dir = if self.map_path.is_some() || self.world_definition_path.is_some() {
            PathBuf::from(DEFAULT_LOCAL_DATA_DIR).join(format!("map-{map_fingerprint:016x}"))
        } else {
            PathBuf::from(DEFAULT_LOCAL_DATA_DIR)
        };
        let profile_dir = self.profile_dir.as_deref().unwrap_or(&default_dir);
        let data_dir = self.server_data_dir.as_deref().unwrap_or(&default_dir);
        let cache_dir = &self.cache_dir;
        let config_dir = self
            .config_path
            .as_deref()
            .and_then(|path| path.parent())
            .unwrap_or_else(|| std::path::Path::new("."));
        let working_dir = self.working_dir.as_deref().ok_or_else(|| {
            std::io::Error::other("client paths requested before process startup")
        })?;
        Ok(minimium_platform::app_paths::AppPaths::resolve_from(
            working_dir,
            profile_dir,
            data_dir,
            cache_dir,
            config_dir,
            self.log_dir.as_deref(),
        ))
    }

    pub(crate) fn local_server_launch_config(
        &self,
        data_dir: PathBuf,
        log_dir: PathBuf,
        parent_run_id: String,
        command_threads: Option<ThreadLane>,
    ) -> Result<minimium_client_runtime::LocalServerLaunchConfig, CommandError> {
        let mut settings = self.server_settings.clone();
        if self.dev_mode
            && settings
                .insert("server.dev-mode".to_owned(), "true".to_owned())
                .is_some()
        {
            return Err(cli_error(
                CommandErrorCode::ConflictingOptions,
                "server.dev-mode",
                "--dev conflicts with --server-set server.dev-mode=...",
            ));
        }
        if let Some(threads) = command_threads {
            if settings
                .insert("server.threads".to_owned(), threads.as_str().to_owned())
                .is_some()
            {
                return Err(cli_error(
                    CommandErrorCode::ConflictingOptions,
                    "server.threads",
                    "host local thread option conflicts with --server-set",
                ));
            }
        }
        Ok(minimium_client_runtime::LocalServerLaunchConfig {
            data_dir,
            log_dir,
            parent_run_id,
            diagnostic_logger: None,
            data_dir_explicit: self.server_data_dir.is_some(),
            config_path: self.server_config_path.clone(),
            settings,
            environment: self.server_environment.clone(),
            client_map_path: self.map_path.clone(),
            client_world_definition_path: self.world_definition_path.clone(),
        })
    }
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
