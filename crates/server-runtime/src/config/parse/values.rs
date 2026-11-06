use super::KEYS;
use crate::config::{LaneSelection, ServerConfig, DEFAULT_AUTOSAVE_SECONDS, MAX_PLAYERS};
use minimium_commands::{
    CommandError, CommandErrorCode, ConfigDocument, ConfigError, ConfigResolution,
    ResolvedConfigValue, MAX_CONFIG_BYTES,
};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    net::IpAddr,
    path::{Path, PathBuf},
    thread,
};

pub(super) fn resolve(
    path: Option<&Path>,
    overrides: BTreeMap<String, String>,
) -> Result<BTreeMap<String, ResolvedConfigValue>, CommandError> {
    let document = path.map(read_document).transpose()?;
    if let Some(document) = &document {
        document
            .validate_keys(KEYS.iter().copied())
            .map_err(map_config_error)?;
    }
    let defaults = defaults();
    let environment = environment_values();
    let resolution =
        ConfigResolution::resolve(&defaults, document.as_ref(), &environment, &overrides);
    let resolved = resolution
        .iter()
        .map(|(key, value)| (key.to_owned(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    for (key, value) in &resolved {
        validate_value(key, &value.requested)?;
    }
    Ok(resolved)
}

pub(super) fn config_from(
    resolved_settings: BTreeMap<String, ResolvedConfigValue>,
) -> Result<ServerConfig, CommandError> {
    let get = |key: &str| resolved_settings[key].requested.as_str();
    let seed = if get("world.seed") == "auto" {
        None
    } else {
        Some(parse_seed(get("world.seed"))?)
    };
    let lanes = if get("server.threads") == "auto" {
        LaneSelection::Auto
    } else {
        LaneSelection::Fixed(parse_value::<usize>(
            "server.threads",
            get("server.threads"),
        )?)
    };
    let path = |key: &str| (!get(key).is_empty()).then(|| PathBuf::from(get(key)));
    let config = ServerConfig {
        material_package_path: path("server.material_package"),
        material_http_port: parse_value(
            "server.material_http_port",
            get("server.material_http_port"),
        )?,
        climate_scenario_path: path("world.climate.initial_scenario"),
        climate_relief_influence: optional_float(get("world.climate.relief_influence")),
        seed,
        lanes,
        data_dir: PathBuf::from(get("server.data_dir")),
        map_path: path("world.map"),
        world_definition_path: path("world.definition"),
        planet_config_path: path("world.planet_config"),
        environment_config_path: path("world.time.initial_config"),
        bind_address: get("server.bind").to_owned(),
        game_port: parse_value("server.game_port", get("server.game_port"))?,
        max_players: parse_value("server.max_players", get("server.max_players"))?,
        autosave_seconds: parse_value("server.autosave_interval", get("server.autosave_interval"))?,
        console_enabled: get("server.headless") == "false",
        resolved_settings,
    };
    if config.map_path.is_some() && config.world_definition_path.is_some() {
        return Err(error(
            CommandErrorCode::InvalidValue,
            "world.map",
            "world.map and world.definition cannot both be set",
            "--map PATH | --world-definition PATH",
        ));
    }
    if config.seed.is_some()
        && (config.map_path.is_some() || config.world_definition_path.is_some())
    {
        return Err(error(
            CommandErrorCode::InvalidValue,
            "world.seed",
            "seed is only available for generated worlds",
            "--seed DECIMAL|0xHEX",
        ));
    }
    if config.planet_config_path.is_some()
        && (config.map_path.is_some() || config.world_definition_path.is_some())
    {
        return Err(error(
            CommandErrorCode::InvalidValue,
            "world.planet_config",
            "planet preset is only available for generated worlds",
            "--planet-config PATH",
        ));
    }
    Ok(config)
}

pub(super) const LEGACY_OPTIONS: &[(&str, &str)] = &[
    ("--seed", "world.seed"),
    ("--threads", "server.threads"),
    ("--data-dir", "server.data_dir"),
    ("--map", "world.map"),
    ("--world-definition", "world.definition"),
    ("--planet-config", "world.planet_config"),
    ("--environment-config", "world.time.initial_config"),
    ("--climate-scenario", "world.climate.initial_scenario"),
    (
        "--climate-relief-influence",
        "world.climate.relief_influence",
    ),
    ("--material-package", "server.material_package"),
    ("--material-http-port", "server.material_http_port"),
    ("--bind", "server.bind"),
    ("--game-port", "server.game_port"),
    ("--max-players", "server.max_players"),
    ("--autosave-seconds", "server.autosave_interval"),
];

fn defaults() -> BTreeMap<String, String> {
    let mut defaults = [
        ("server.threads", "1"),
        (
            "server.data_dir",
            minimium_platform::files::DEFAULT_SERVER_DATA_DIR,
        ),
        ("server.bind", "127.0.0.1"),
        ("server.game_port", "0"),
        ("server.max_players", "512"),
        ("server.headless", "false"),
        ("server.material_package", ""),
        ("server.material_http_port", "0"),
        ("world.seed", "auto"),
        ("world.map", ""),
        ("world.definition", ""),
        ("world.planet_config", ""),
        ("world.time.initial_config", ""),
        ("world.climate.initial_scenario", ""),
        ("world.climate.relief_influence", "unset"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_owned(), value.to_owned()))
    .collect::<BTreeMap<_, _>>();
    defaults.insert(
        "server.autosave_interval".to_owned(),
        DEFAULT_AUTOSAVE_SECONDS.to_string(),
    );
    defaults
}

fn environment_values() -> BTreeMap<String, String> {
    [
        ("MINIMIUM_SERVER_THREADS", "server.threads"),
        ("MINIMIUM_SERVER_DATA_DIR", "server.data_dir"),
        ("MINIMIUM_SERVER_BIND", "server.bind"),
        ("MINIMIUM_SERVER_GAME_PORT", "server.game_port"),
        ("MINIMIUM_SERVER_MAX_PLAYERS", "server.max_players"),
        (
            "MINIMIUM_SERVER_AUTOSAVE_INTERVAL",
            "server.autosave_interval",
        ),
        ("MINIMIUM_SERVER_HEADLESS", "server.headless"),
        ("MINIMIUM_SERVER_SEED", "world.seed"),
        ("MINIMIUM_SERVER_PLANET_CONFIG", "world.planet_config"),
        (
            "MINIMIUM_SERVER_CLIMATE_RELIEF_INFLUENCE",
            "world.climate.relief_influence",
        ),
    ]
    .into_iter()
    .filter_map(|(name, key)| {
        std::env::var(name)
            .ok()
            .map(|value| (key.to_owned(), value))
    })
    .collect()
}

fn validate_value(key: &str, value: &str) -> Result<(), CommandError> {
    let invalid = |reason: String| {
        error(
            CommandErrorCode::InvalidValue,
            key,
            reason,
            "config validate | --set KEY=VALUE",
        )
    };
    match key {
        "server.threads" if value == "auto" => Ok(()),
        "server.threads" => parse_value::<usize>(key, value).and_then(|n| {
            (1..=64)
                .contains(&n)
                .then_some(())
                .ok_or_else(|| invalid("threads must be 1..64 or auto".into()))
        }),
        "server.data_dir" => nonempty(key, value, &invalid),
        "server.bind" => value
            .parse::<IpAddr>()
            .map(|_| ())
            .map_err(|_| invalid("bind must be an IP address".into())),
        "server.game_port" | "server.material_http_port" => {
            parse_value::<u16>(key, value).map(|_| ())
        }
        "server.max_players" => parse_value::<usize>(key, value).and_then(|n| {
            (1..=MAX_PLAYERS)
                .contains(&n)
                .then_some(())
                .ok_or_else(|| invalid(format!("max players must be 1..={MAX_PLAYERS}")))
        }),
        "server.autosave_interval" => parse_value::<u64>(key, value).and_then(|n| {
            (n <= 604_800)
                .then_some(())
                .ok_or_else(|| invalid("autosave interval must be 0..604800 seconds".into()))
        }),
        "server.headless" => bool_value(key, value),
        "server.material_package"
        | "world.map"
        | "world.definition"
        | "world.planet_config"
        | "world.time.initial_config"
        | "world.climate.initial_scenario" => Ok(()),
        "world.seed" if value == "auto" => Ok(()),
        "world.seed" => parse_seed(value).map(|_| ()),
        "world.climate.relief_influence" if value == "unset" => Ok(()),
        "world.climate.relief_influence" => {
            let number = value.parse::<f64>().map_err(|_| {
                invalid("climate relief influence must be finite 0..1 or unset".into())
            })?;
            if number.is_finite() && (0.0..=1.0).contains(&number) {
                Ok(())
            } else {
                Err(invalid(
                    "climate relief influence must be finite 0..1 or unset".into(),
                ))
            }
        }
        _ => Err(error(
            CommandErrorCode::UnknownProperty,
            key,
            "unknown server setting",
            "help [KEY]",
        )),
    }
}

fn nonempty(
    key: &str,
    value: &str,
    invalid: &impl Fn(String) -> CommandError,
) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        Err(invalid(format!("{key} must not be empty")))
    } else {
        Ok(())
    }
}

fn bool_value(key: &str, value: &str) -> Result<(), CommandError> {
    match value {
        "true" | "false" => Ok(()),
        _ => Err(error(
            CommandErrorCode::InvalidValue,
            key,
            "expected true or false",
            "set KEY true|false",
        )),
    }
}

fn parse_value<T: std::str::FromStr>(key: &str, value: &str) -> Result<T, CommandError> {
    value.parse().map_err(|_| {
        error(
            CommandErrorCode::InvalidValue,
            key,
            format!("invalid value `{value}`"),
            "config validate | --set KEY=VALUE",
        )
    })
}

fn parse_seed(value: &str) -> Result<u128, CommandError> {
    if let Some(hex) = value.strip_prefix("0x") {
        u128::from_str_radix(hex, 16)
    } else {
        value.parse()
    }
    .map_err(|_| {
        error(
            CommandErrorCode::InvalidValue,
            "world.seed",
            "seed must be a decimal or 0x hexadecimal u128",
            "--seed DECIMAL|0xHEX",
        )
    })
}

fn optional_float(value: &str) -> Option<f64> {
    (value != "unset").then(|| value.parse().expect("validated finite float"))
}

fn read_document(path: &Path) -> Result<ConfigDocument, CommandError> {
    let mut file = File::open(path).map_err(|e| {
        error(
            CommandErrorCode::InvalidValue,
            "--config",
            format!("could not read {}: {e}", path.display()),
            "--config PATH",
        )
    })?;
    let mut bytes = Vec::with_capacity(4096);
    file.by_ref()
        .take((MAX_CONFIG_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| {
            error(
                CommandErrorCode::InvalidValue,
                "--config",
                format!("could not read {}: {e}", path.display()),
                "--config PATH",
            )
        })?;
    ConfigDocument::parse(&bytes).map_err(map_config_error)
}

fn map_config_error(error_value: ConfigError) -> CommandError {
    let code = match error_value.code {
        minimium_commands::ConfigErrorCode::UnknownKey => CommandErrorCode::UnknownProperty,
        _ => CommandErrorCode::InvalidValue,
    };
    CommandError::new(
        code,
        error_value.key.as_deref(),
        error_value.reason,
        Some("config validate --config PATH"),
    )
}

pub(super) fn error(
    code: CommandErrorCode,
    subject: &str,
    reason: impl Into<String>,
    usage: &str,
) -> CommandError {
    CommandError::new(code, Some(subject), reason, Some(usage))
}

#[derive(Serialize)]
pub(super) struct JsonView {
    pub(super) schema_version: u32,
    pub(super) settings: BTreeMap<String, JsonSetting>,
}

#[derive(Serialize)]
pub(super) struct JsonSetting {
    pub(super) requested: String,
    pub(super) effective: String,
    pub(super) default: Option<String>,
    pub(super) source: &'static str,
    pub(super) reason: Option<&'static str>,
}

pub(super) fn effective_value(key: &str, requested: &str) -> String {
    if key == "server.threads" && requested == "auto" {
        thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .min(64)
            .to_string()
    } else {
        requested.to_owned()
    }
}

pub(super) fn effective_reason(key: &str, requested: &str) -> Option<&'static str> {
    (key == "server.threads" && requested == "auto")
        .then_some("auto resolved to the available worker lane count")
}
