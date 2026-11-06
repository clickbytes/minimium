use super::ServerConfig;
mod values;
use minimium_commands::{
    parse_cli, CliOptionSpec, CliParseOutcome, CommandError, CommandErrorCode, MAX_CONFIG_BYTES,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use values::{
    config_from, effective_reason, effective_value, error, resolve, JsonSetting, JsonView,
    LEGACY_OPTIONS,
};

const KEYS: &[&str] = &[
    "server.threads",
    "server.data_dir",
    "server.bind",
    "server.game_port",
    "server.max_players",
    "server.autosave_interval",
    "server.headless",
    "server.material_package",
    "server.material_http_port",
    "world.seed",
    "world.map",
    "world.definition",
    "world.planet_config",
    "world.environment_config",
    "world.climate.initial_scenario",
    "world.climate.relief_influence",
];

const OPTIONS: &[CliOptionSpec] = &[
    option(
        "--config",
        Some("-c"),
        Some("PATH"),
        "RON settings file.",
        false,
        false,
    ),
    option(
        "--set",
        None,
        Some("KEY=VALUE"),
        "Override one setting; repeatable.",
        true,
        false,
    ),
    option(
        "--seed",
        None,
        Some("DECIMAL|0xHEX"),
        "Generated world seed.",
        false,
        false,
    ),
    option(
        "--threads",
        None,
        Some("1..64|auto"),
        "Server worker lanes.",
        false,
        false,
    ),
    option(
        "--data-dir",
        None,
        Some("PATH"),
        "Server data directory.",
        false,
        false,
    ),
    option(
        "--map",
        None,
        Some("PATH"),
        "Legacy map input.",
        false,
        false,
    ),
    option(
        "--world-definition",
        None,
        Some("PATH"),
        "Authored world definition.",
        false,
        false,
    ),
    option(
        "--planet-config",
        None,
        Some("PATH"),
        "Generated planet preset.",
        false,
        false,
    ),
    option(
        "--environment-config",
        None,
        Some("PATH"),
        "Initial world clock config.",
        false,
        false,
    ),
    option(
        "--climate-scenario",
        None,
        Some("PATH"),
        "Initial climate scenario.",
        false,
        false,
    ),
    option(
        "--climate-relief-influence",
        None,
        Some("0..1"),
        "Climate relief influence.",
        false,
        false,
    ),
    option(
        "--material-package",
        None,
        Some("PATH"),
        "Material package.",
        false,
        false,
    ),
    option(
        "--material-http-port",
        None,
        Some("PORT"),
        "Material HTTP port.",
        false,
        false,
    ),
    option(
        "--bind",
        None,
        Some("IP"),
        "Game listener IP.",
        false,
        false,
    ),
    option(
        "--game-port",
        None,
        Some("PORT"),
        "Game listener port.",
        false,
        false,
    ),
    option(
        "--max-players",
        None,
        Some("N"),
        "Maximum players (1..512).",
        false,
        false,
    ),
    option(
        "--autosave-seconds",
        None,
        Some("0..604800"),
        "Periodic world save interval.",
        false,
        false,
    ),
    option(
        "--headless",
        None,
        None,
        "Disable stdin commands.",
        false,
        false,
    ),
    option("--help", Some("-h"), None, "Show help.", false, true),
];

const OFFLINE_OPTIONS: &[CliOptionSpec] = &[
    option(
        "--config",
        Some("-c"),
        Some("PATH"),
        "RON settings file.",
        false,
        false,
    ),
    option(
        "--set",
        None,
        Some("KEY=VALUE"),
        "Apply a command-line override; repeatable.",
        true,
        false,
    ),
    option(
        "--sources",
        None,
        None,
        "Include setting source metadata.",
        false,
        false,
    ),
    option(
        "--format",
        None,
        Some("FORMAT"),
        "Output format: text or json.",
        false,
        false,
    ),
    option(
        "--help",
        Some("-h"),
        None,
        "Show config command help.",
        false,
        true,
    ),
];

const fn option(
    name: &'static str,
    alias: Option<&'static str>,
    value_name: Option<&'static str>,
    description: &'static str,
    repeatable: bool,
    help: bool,
) -> CliOptionSpec {
    CliOptionSpec {
        name,
        alias,
        value_name,
        description,
        repeatable,
        help,
    }
}

pub(super) fn parse(args: &[String]) -> Result<ServerConfig, CommandError> {
    let start = if args.first().map(String::as_str) == Some("run") {
        1
    } else {
        0
    };
    if start == 0 && args.first().is_some_and(|arg| !arg.starts_with('-')) {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            "run",
            "expected subcommand `run`",
            "minimium-server run [OPTIONS]",
        ));
    }
    let program = "minimium-server run [OPTIONS]";
    let parsed = match parse_cli(args.iter().skip(start).cloned(), program, OPTIONS)? {
        CliParseOutcome::Help => {
            return Err(error(
                CommandErrorCode::InvalidArguments,
                "--help",
                "help is handled before configuration parsing",
                program,
            ))
        }
        CliParseOutcome::Parsed(parsed) => parsed,
    };
    if !parsed.positionals().is_empty() {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            parsed.positionals()[0].as_str(),
            "unexpected positional argument",
            program,
        ));
    }
    let file_path = parsed.get("--config").map(Path::new);
    let mut overrides = BTreeMap::new();
    let mut explicit = BTreeSet::new();
    for assignment in parsed.get_all("--set").flatten() {
        let (key, value) = assignment.split_once('=').ok_or_else(|| {
            error(
                CommandErrorCode::InvalidArguments,
                "--set",
                "expected KEY=VALUE",
                program,
            )
        })?;
        if !KEYS.contains(&key) {
            return Err(error(
                CommandErrorCode::UnknownProperty,
                key,
                "unknown server setting",
                "help [KEY]",
            ));
        }
        if !explicit.insert(key.to_owned()) {
            return Err(error(
                CommandErrorCode::DuplicateOption,
                key,
                "setting was supplied more than once",
                program,
            ));
        }
        overrides.insert(key.to_owned(), value.to_owned());
    }
    for (option_name, key) in LEGACY_OPTIONS {
        if let Some(value) = parsed.get(option_name) {
            if !explicit.insert((*key).to_owned()) {
                return Err(error(
                    CommandErrorCode::ConflictingOptions,
                    key,
                    format!("{option_name} conflicts with --set {key}=..."),
                    program,
                ));
            }
            overrides.insert((*key).to_owned(), value.to_owned());
        }
    }
    if parsed.contains("--headless") {
        if !explicit.insert("server.headless".to_owned()) {
            return Err(error(
                CommandErrorCode::ConflictingOptions,
                "server.headless",
                "--headless conflicts with --set server.headless=...",
                program,
            ));
        }
        overrides.insert("server.headless".to_owned(), "true".to_owned());
    }
    let resolved = resolve(file_path, overrides)?;
    config_from(resolved)
}

pub(super) fn help_text() -> String {
    minimium_commands::format_cli_help("minimium-server run", OPTIONS)
}

pub(super) fn offline_command(args: &[String]) -> Result<Option<String>, CommandError> {
    if args.first().map(String::as_str) != Some("config") {
        return Ok(None);
    }
    let action = args.get(1).map(String::as_str).ok_or_else(|| {
        error(
            CommandErrorCode::InvalidArguments,
            "config",
            "expected validate or show",
            "config validate|show --config PATH",
        )
    })?;
    if !matches!(action, "validate" | "show") {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            "config",
            "expected validate or show",
            "config validate|show --config PATH",
        ));
    }
    let program = format!("minimium-server config {action} --config PATH");
    let parsed = match parse_cli(args.iter().skip(2).cloned(), &program, OFFLINE_OPTIONS)? {
        CliParseOutcome::Help => {
            return Ok(Some(format!(
                "{program}\n--sources and --format json are available with show"
            )))
        }
        CliParseOutcome::Parsed(parsed) => parsed,
    };
    if !parsed.positionals().is_empty() {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            parsed.positionals()[0].as_str(),
            "unexpected positional argument",
            &program,
        ));
    }
    if action == "validate" && (parsed.contains("--sources") || parsed.get("--format").is_some()) {
        return Err(error(
            CommandErrorCode::InvalidArguments,
            "config validate",
            "format and sources are only available with show",
            &program,
        ));
    }
    let mut overrides = BTreeMap::new();
    for assignment in parsed.get_all("--set").flatten() {
        let (key, value) = assignment.split_once('=').ok_or_else(|| {
            error(
                CommandErrorCode::InvalidArguments,
                "--set",
                "expected KEY=VALUE",
                program.as_str(),
            )
        })?;
        if overrides.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(error(
                CommandErrorCode::DuplicateOption,
                key,
                "setting was supplied more than once",
                program.as_str(),
            ));
        }
    }
    let path = parsed.get("--config").map(Path::new);
    let resolved = resolve(path, overrides)?;
    let _validated_runtime_config = config_from(resolved.clone())?;
    if action == "validate" {
        return Ok(Some(format!(
            "valid server configuration schema=1 settings={}",
            resolved.len()
        )));
    }
    let output_format = parsed.get("--format").unwrap_or("text");
    if !matches!(output_format, "text" | "json") {
        return Err(error(
            CommandErrorCode::InvalidValue,
            "--format",
            "format must be text or json",
            program.as_str(),
        ));
    }
    let include_sources = parsed.contains("--sources");
    if output_format == "json" {
        let settings = resolved
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    JsonSetting {
                        requested: value.requested.clone(),
                        effective: effective_value(key, &value.requested),
                        default: value.default.clone(),
                        source: value.source.as_str(),
                        reason: effective_reason(key, &value.requested),
                    },
                )
            })
            .collect();
        let output = serde_json::to_string(&JsonView {
            schema_version: 1,
            settings,
        })
        .map_err(|e| {
            error(
                CommandErrorCode::InvalidArguments,
                "config show",
                format!("JSON serialization failed: {e}"),
                "config show --format json",
            )
        })?;
        if output.len() > MAX_CONFIG_BYTES {
            return Err(error(
                CommandErrorCode::InvalidArguments,
                "config show",
                "resolved JSON output exceeds 64 KiB",
                "config show --format json",
            ));
        }
        Ok(Some(output))
    } else {
        Ok(Some(
            resolved
                .iter()
                .map(|(key, value)| {
                    let line = format!(
                        "{key}={} effective={}",
                        value.requested,
                        effective_value(key, &value.requested)
                    );
                    if include_sources {
                        format!(
                            "{line} default={} source={}",
                            value.default.as_deref().unwrap_or("n/a"),
                            value.source.as_str()
                        )
                    } else {
                        line
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        ))
    }
}
