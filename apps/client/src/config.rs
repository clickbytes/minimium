use crate::{
    application::StartupSettings,
    client_commands::{local_setting_specs, parse_setting_value, ClientPropertyKey as Key},
};
use minimium_commands::{
    parse_cli, CliOptionSpec, CliParseOutcome, CommandError, CommandErrorCode, ConfigDocument,
    ConfigErrorCode, ConfigResolution, MAX_CONFIG_BYTES,
};
use std::{collections::BTreeMap, path::Path};

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
    pub(crate) startup: StartupSettings,
    pub(crate) file_settings: BTreeMap<String, String>,
}

fn persistent_keys() -> impl Iterator<Item = &'static str> {
    local_setting_specs()
        .filter(|spec| spec.persistent)
        .map(|spec| spec.key)
}

pub(crate) fn resolve_preferences(
    path: Option<&Path>,
    command_line: &BTreeMap<String, String>,
) -> Result<ClientPreferenceConfig, CommandError> {
    let document = path.map(read_document).transpose()?;
    if let Some(document) = &document {
        document
            .validate_keys(persistent_keys())
            .map_err(map_config_error)?;
    }
    if let Some(key) = command_line
        .keys()
        .find(|key| !local_setting_specs().any(|spec| spec.key == key.as_str()))
    {
        return Err(CommandError::new(
            CommandErrorCode::UnknownProperty,
            Some(key.as_str()),
            "unknown client property",
            Some("help [KEY]"),
        ));
    }
    let defaults = local_setting_specs()
        .map(|spec| {
            (
                spec.key.to_owned(),
                spec.default.expect("local startup default").to_owned(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let environment = mapped_environment_values();
    let resolved =
        ConfigResolution::resolve(&defaults, document.as_ref(), &environment, command_line);
    let startup = StartupSettings::from_resolution(&resolved)?;
    let atmosphere_debug = startup.atmosphere_debug();
    let terrain_debug = startup.terrain_debug();
    if atmosphere_debug != minimium_renderer::atmosphere_debug::AtmosphereDebug::Off
        && terrain_debug != minimium_renderer::terrain_debug::TerrainDebug::Off
    {
        return Err(CommandError::new(
            CommandErrorCode::InvalidValue,
            Some(Key::AtmosphereDebug.key()),
            "atmosphere and terrain diagnostics are mutually exclusive",
            Some("set client.debug.atmosphere VALUE or client.debug.terrain VALUE"),
        ));
    }
    Ok(ClientPreferenceConfig {
        startup,
        file_settings: document.map_or_else(BTreeMap::new, |document| document.settings().clone()),
    })
}

fn mapped_environment_values() -> BTreeMap<String, String> {
    [
        ("MINIMIUM_CLIENT_CAMERA_FOV", Key::CameraFov.key()),
        (
            "MINIMIUM_CLIENT_MOUSE_SENSITIVITY",
            Key::MouseSensitivity.key(),
        ),
        ("MINIMIUM_CLIENT_SKY_QUALITY", Key::SkyQuality.key()),
        ("MINIMIUM_CLIENT_SKY_NAMES", Key::SkyNames.key()),
        ("MINIMIUM_CLIENT_CLOUD_DETAIL", Key::CloudDetail.key()),
        ("MINIMIUM_METEOR_SEED", Key::MeteorSeed.key()),
        ("MINIMIUM_METEORS", Key::MeteorEnabled.key()),
    ]
    .into_iter()
    .filter_map(|(environment, key)| {
        std::env::var(environment).ok().map(|value| {
            (
                key.to_owned(),
                if key == Key::MeteorEnabled.key() && value == "0" {
                    "false".into()
                } else if key == Key::MeteorEnabled.key() && value == "1" {
                    "true".into()
                } else {
                    value
                },
            )
        })
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
    let mut records = crate::application::SettingsStore::new(&resolved.startup);
    for value in resolved.startup.iter() {
        records.set_unavailable(
            value.key().key(),
            "configuration preview; no client consumer applied the value".to_owned(),
        );
    }
    let output = resolved
        .startup
        .iter()
        .map(|setting| {
            let key = setting.key().key();
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

pub(crate) struct ConfigSaveReceipt {
    pub path: std::path::PathBuf,
    pub bytes: usize,
}

pub(crate) fn save_preferences(
    path: &Path,
    file_settings: &BTreeMap<String, String>,
    session_settings: &BTreeMap<String, String>,
) -> Result<ConfigSaveReceipt, CommandError> {
    let mut settings = file_settings
        .iter()
        .filter(|(key, _)| persistent_keys().any(|allowed| allowed == key.as_str()))
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    for (key, value) in session_settings {
        if persistent_keys().any(|allowed| allowed == key.as_str()) {
            let value = parse_setting_value(key, value)?;
            settings.insert(value.key().key().to_owned(), value.encode());
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
    use std::io::Write;
    minimium_platform::files::atomic_write_with_stage(&temporary, path, |file| {
        file.write_all(&bytes)
    })
    .map_err(|(stage, error)| {
        CommandError::new(
            CommandErrorCode::StorageFailure,
            Some(path.display().to_string()),
            format!("{stage}: {error}"),
            None::<String>,
        )
    })?;
    Ok(ConfigSaveReceipt {
        path: path.to_owned(),
        bytes: bytes.len(),
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
