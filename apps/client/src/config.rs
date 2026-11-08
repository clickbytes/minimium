use crate::client_commands::{parse_setting_value, ClientPropertyValue};
use minimium_commands::{
    parse_cli, CliOptionSpec, CliParseOutcome, CommandError, CommandErrorCode, ConfigDocument,
    ConfigErrorCode, ConfigResolution, PropertySource, MAX_CONFIG_BYTES,
};
use minimium_renderer::quality::SkyQuality;
use std::{collections::BTreeMap, path::Path};

const FOV_KEY: &str = "client.camera.fov";
const SENSITIVITY_KEY: &str = "client.input.mouse_sensitivity";
const INPUT_BINDINGS_KEY: &str = "client.input.bindings";
const SKY_QUALITY_KEY: &str = "client.graphics.sky_quality";
const SKY_NAMES_KEY: &str = "client.sky.names";
const CLOUD_DETAIL_KEY: &str = "client.weather.cloud_detail";
const ATMOSPHERE_DEBUG_KEY: &str = "client.debug.atmosphere";
const TERRAIN_DEBUG_KEY: &str = "client.debug.terrain";
const CLOUD_DEBUG_KEY: &str = "client.debug.clouds";
const SKY_DEMO_KEY: &str = "client.sky.demo";
const LOCALE_KEY: &str = "ui.locale";
const DEFAULTS: &[(&str, &str)] = &[
    (FOV_KEY, "70"),
    (SENSITIVITY_KEY, "0.0025"),
    (INPUT_BINDINGS_KEY, ""),
    (SKY_QUALITY_KEY, "standard"),
    (SKY_NAMES_KEY, "false"),
    (CLOUD_DETAIL_KEY, "6"),
    (ATMOSPHERE_DEBUG_KEY, "off"),
    (TERRAIN_DEBUG_KEY, "off"),
    (CLOUD_DEBUG_KEY, "off"),
    (SKY_DEMO_KEY, "false"),
    (LOCALE_KEY, "en"),
];
const PERSISTENT_KEYS: &[&str] = &[
    FOV_KEY,
    SENSITIVITY_KEY,
    INPUT_BINDINGS_KEY,
    SKY_QUALITY_KEY,
    SKY_NAMES_KEY,
    CLOUD_DETAIL_KEY,
    LOCALE_KEY,
];
const KNOWN_KEYS: &[&str] = &[
    FOV_KEY,
    SENSITIVITY_KEY,
    INPUT_BINDINGS_KEY,
    SKY_QUALITY_KEY,
    SKY_NAMES_KEY,
    CLOUD_DETAIL_KEY,
    ATMOSPHERE_DEBUG_KEY,
    TERRAIN_DEBUG_KEY,
    CLOUD_DEBUG_KEY,
    SKY_DEMO_KEY,
    LOCALE_KEY,
];
const OFFLINE_OPTIONS: &[CliOptionSpec] = &[
    CliOptionSpec {
        name: "--config",
        alias: None,
        value_name: Some("PATH"),
        description: "Configuration file to inspect.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--sources",
        alias: None,
        value_name: None,
        description: "Include requested/effective values, source and reason in show output.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--help",
        alias: Some("-h"),
        value_name: None,
        description: "Show config command help without reading the file.",
        repeatable: false,
        help: true,
    },
];

pub(crate) struct ClientPreferenceConfig {
    pub(crate) locale: minimium_localization::Locale,
    pub(crate) fov_degrees: f32,
    pub(crate) mouse_sensitivity: f32,
    pub(crate) input_bindings: String,
    pub(crate) sky_quality: SkyQuality,
    pub(crate) sky_names: bool,
    pub(crate) cloud_detail: u8,
    pub(crate) atmosphere_debug: minimium_renderer::atmosphere_debug::AtmosphereDebug,
    pub(crate) terrain_debug: minimium_renderer::terrain_debug::TerrainDebug,
    pub(crate) cloud_debug: minimium_environment::weather::CloudDebug,
    pub(crate) sky_demo: bool,
    pub(crate) sources: BTreeMap<String, PropertySource>,
    pub(crate) resolved_values: BTreeMap<String, String>,
    pub(crate) file_settings: BTreeMap<String, String>,
}

pub(crate) fn resolve_preferences(
    path: Option<&Path>,
    command_line: &BTreeMap<String, String>,
) -> Result<ClientPreferenceConfig, CommandError> {
    let document = path.map(read_document).transpose()?;
    if let Some(document) = &document {
        document
            .validate_keys(PERSISTENT_KEYS.iter().copied())
            .map_err(map_config_error)?;
    }
    if let Some(key) = command_line
        .keys()
        .find(|key| !KNOWN_KEYS.contains(&key.as_str()))
    {
        return Err(CommandError::new(
            CommandErrorCode::UnknownProperty,
            Some(key.as_str()),
            "unknown client property",
            Some("help [KEY]"),
        ));
    }
    let defaults = DEFAULTS
        .iter()
        .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
        .collect::<BTreeMap<_, _>>();
    let environment = mapped_environment_values();
    let resolved =
        ConfigResolution::resolve(&defaults, document.as_ref(), &environment, command_line);
    let mut sources = BTreeMap::new();
    let mut resolved_values = BTreeMap::new();
    for (key, value) in resolved.iter() {
        sources.insert(key.to_owned(), value.source);
        resolved_values.insert(key.to_owned(), value.requested.clone());
        if key == LOCALE_KEY {
            if minimium_localization::Locale::from_setting(&value.requested).is_none() {
                return Err(CommandError::new(
                    CommandErrorCode::InvalidValue,
                    Some(LOCALE_KEY),
                    "expected en or pl",
                    Some("config show ui.locale"),
                ));
            }
        } else if key == INPUT_BINDINGS_KEY {
            crate::input::InputMapper::from_setting(&value.requested).map_err(|error| {
                CommandError::new(
                    CommandErrorCode::InvalidValue,
                    Some(INPUT_BINDINGS_KEY),
                    error,
                    Some("config show client.input.bindings"),
                )
            })?;
        } else {
            parse_setting_value(key, &value.requested)?;
        }
    }
    let value = |key: &str| resolved_values.get(key).expect("setting has a default");
    let typed = |key: &str| parse_setting_value(key, value(key)).expect("validated setting");
    let fov_degrees = match typed(FOV_KEY) {
        ClientPropertyValue::CameraFov(value) => value,
        _ => unreachable!("FOV key has a decimal descriptor"),
    };
    let locale = minimium_localization::Locale::from_setting(value(LOCALE_KEY))
        .expect("locale was validated above");
    let mouse_sensitivity = match typed(SENSITIVITY_KEY) {
        ClientPropertyValue::MouseSensitivity(value) => value,
        _ => unreachable!("sensitivity key has a decimal descriptor"),
    };
    let input_bindings = value(INPUT_BINDINGS_KEY).clone();
    let sky_quality = match typed(SKY_QUALITY_KEY) {
        ClientPropertyValue::SkyQuality(value) => value,
        _ => unreachable!("quality key has an enum descriptor"),
    };
    let sky_names = match typed(SKY_NAMES_KEY) {
        ClientPropertyValue::SkyNames(value) => value,
        _ => unreachable!("names key has a boolean descriptor"),
    };
    let cloud_detail = match typed(CLOUD_DETAIL_KEY) {
        ClientPropertyValue::CloudDetail(value) => value,
        _ => unreachable!("detail key has an integer descriptor"),
    };
    let atmosphere_debug = match typed(ATMOSPHERE_DEBUG_KEY) {
        ClientPropertyValue::AtmosphereDebug(value) => value,
        _ => unreachable!("atmosphere debug key has an enum descriptor"),
    };
    let terrain_debug = match typed(TERRAIN_DEBUG_KEY) {
        ClientPropertyValue::TerrainDebug(value) => value,
        _ => unreachable!("terrain debug key has an enum descriptor"),
    };
    let cloud_debug = match typed(CLOUD_DEBUG_KEY) {
        ClientPropertyValue::CloudDebug(value) => value,
        _ => unreachable!("cloud debug key has an enum descriptor"),
    };
    let sky_demo = match typed(SKY_DEMO_KEY) {
        ClientPropertyValue::SkyDemo(value) => value,
        _ => unreachable!("demo key has a boolean descriptor"),
    };
    if atmosphere_debug != minimium_renderer::atmosphere_debug::AtmosphereDebug::Off
        && terrain_debug != minimium_renderer::terrain_debug::TerrainDebug::Off
    {
        return Err(CommandError::new(
            CommandErrorCode::InvalidValue,
            Some(ATMOSPHERE_DEBUG_KEY),
            "atmosphere and terrain diagnostics are mutually exclusive",
            Some("set client.debug.atmosphere VALUE or client.debug.terrain VALUE"),
        ));
    }
    Ok(ClientPreferenceConfig {
        locale,
        fov_degrees,
        mouse_sensitivity,
        input_bindings,
        sky_quality,
        sky_names,
        cloud_detail,
        atmosphere_debug,
        terrain_debug,
        cloud_debug,
        sky_demo,
        sources,
        resolved_values,
        file_settings: document.map_or_else(BTreeMap::new, |document| document.settings().clone()),
    })
}

fn mapped_environment_values() -> BTreeMap<String, String> {
    [
        ("MINIMIUM_CLIENT_CAMERA_FOV", FOV_KEY),
        ("MINIMIUM_CLIENT_MOUSE_SENSITIVITY", SENSITIVITY_KEY),
        ("MINIMIUM_CLIENT_SKY_QUALITY", SKY_QUALITY_KEY),
        ("MINIMIUM_CLIENT_SKY_NAMES", SKY_NAMES_KEY),
        ("MINIMIUM_CLIENT_CLOUD_DETAIL", CLOUD_DETAIL_KEY),
    ]
    .into_iter()
    .filter_map(|(environment, key)| {
        std::env::var(environment)
            .ok()
            .map(|value| (key.to_owned(), value))
    })
    .collect()
}

pub(crate) fn handle_offline_command(args: &[String]) -> Result<Option<String>, CommandError> {
    if args.first().map(String::as_str) != Some("config") {
        return Ok(None);
    }
    let action = args.get(1).map(String::as_str).ok_or_else(|| {
        CommandError::new(
            CommandErrorCode::InvalidArguments,
            Some("config"),
            "expected validate or show",
            Some("config validate --config PATH | config show --config PATH [--sources]"),
        )
    })?;
    if !matches!(action, "validate" | "show") {
        return Err(CommandError::new(
            CommandErrorCode::InvalidArguments,
            Some("config"),
            "expected validate or show",
            Some("config validate --config PATH | config show --config PATH [--sources]"),
        ));
    }
    let program = format!("minimium-client config {action} --config PATH");
    let values = match parse_cli(args.iter().skip(2).cloned(), &program, OFFLINE_OPTIONS)? {
        CliParseOutcome::Help => return Ok(Some(program)),
        CliParseOutcome::Parsed(values) => values,
    };
    if !values.positionals().is_empty() {
        return Err(CommandError::new(
            CommandErrorCode::InvalidArguments,
            Some("config"),
            "unexpected positional argument",
            Some(program),
        ));
    }
    let path = values.get("--config").ok_or_else(|| {
        CommandError::new(
            CommandErrorCode::MissingValue,
            Some("--config"),
            "configuration path is required",
            Some(program.as_str()),
        )
    })?;
    if action == "validate" && values.contains("--sources") {
        return Err(CommandError::new(
            CommandErrorCode::InvalidArguments,
            Some("--sources"),
            "sources are available with config show",
            Some(program.as_str()),
        ));
    }
    let resolved = resolve_preferences(Some(Path::new(path)), &BTreeMap::new())?;
    if action == "validate" {
        return Ok(Some(format!(
            "valid client configuration schema=1 settings={}",
            resolved.file_settings.len()
        )));
    }
    let include_sources = values.contains("--sources");
    let mut records = crate::application::SettingsStore::new(
        resolved.resolved_values.clone(),
        resolved.sources.clone(),
    );
    for key in resolved.resolved_values.keys() {
        records.set_unavailable(
            key,
            "configuration preview; no client consumer applied the value".to_owned(),
        );
    }
    let output = resolved
        .resolved_values
        .keys()
        .map(|key| {
            let value = records
                .requested(key)
                .expect("resolved setting record exists");
            if include_sources {
                format!(
                    "{key} requested={value} effective={} source={} reason={}",
                    records.effective(key).unwrap_or("unavailable"),
                    records.source(key).as_str(),
                    records.difference_reason(key).unwrap_or("none"),
                )
            } else {
                format!("{key}={value}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Some(output))
}

pub(crate) fn save_preferences(
    path: &Path,
    file_settings: &BTreeMap<String, String>,
    session_settings: &BTreeMap<String, String>,
) -> Result<(), CommandError> {
    let mut settings = file_settings
        .iter()
        .filter(|(key, _)| PERSISTENT_KEYS.contains(&key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    for (key, value) in session_settings {
        if PERSISTENT_KEYS.contains(&key.as_str()) {
            parse_setting_value(key, value)?;
            settings.insert(key.clone(), value.clone());
        }
    }
    let bytes = ConfigDocument::new(settings)
        .to_ron()
        .map_err(map_config_error)?
        .into_bytes();
    if bytes.len() > MAX_CONFIG_BYTES {
        return Err(CommandError::new(
            CommandErrorCode::InvalidValue,
            Some(path.display().to_string()),
            "configuration exceeds 65536 bytes",
            None::<String>,
        ));
    }
    let temporary = temporary_path(path);
    minimium_platform::files::atomic_write_file(&temporary, path, &bytes).map_err(|error| {
        CommandError::new(
            CommandErrorCode::Io,
            Some(path.display().to_string()),
            format!("could not save configuration: {error}"),
            None::<String>,
        )
    })
}

fn read_document(path: &Path) -> Result<ConfigDocument, CommandError> {
    let bytes =
        minimium_platform::files::read_bounded_file(path, MAX_CONFIG_BYTES).map_err(|error| {
            CommandError::new(
                CommandErrorCode::Io,
                Some(path.display().to_string()),
                format!("could not read configuration: {error}"),
                None::<String>,
            )
        })?;
    ConfigDocument::parse(&bytes).map_err(map_config_error)
}

fn map_config_error(error: minimium_commands::ConfigError) -> CommandError {
    let code = match error.code {
        ConfigErrorCode::UnknownKey => CommandErrorCode::UnknownProperty,
        ConfigErrorCode::TooLarge
        | ConfigErrorCode::InvalidDocument
        | ConfigErrorCode::UnsupportedSchema => CommandErrorCode::InvalidValue,
        ConfigErrorCode::Serialization => CommandErrorCode::Io,
    };
    CommandError::new(code, error.key, error.reason, None::<String>)
}

fn temporary_path(destination: &Path) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let name = destination.file_name().map_or_else(
        || "client-config".to_owned(),
        |name| name.to_string_lossy().into_owned(),
    );
    destination.with_file_name(format!("{name}.tmp-{}-{nonce}", std::process::id()))
}
