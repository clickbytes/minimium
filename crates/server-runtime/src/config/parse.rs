use super::ServerConfig;
mod keys;
mod offline;
mod values;
use keys::KEYS;
use minimium_commands::{
    parse_cli, CliOptionSpec, CliParseOutcome, CommandError, CommandErrorCode, MAX_CONFIG_BYTES,
};
pub(crate) use offline::offline_command;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use values::{
    config_from, effective_reason, effective_value, error, resolve, JsonSetting, JsonView,
    LEGACY_OPTIONS,
};

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
        "--dev",
        None,
        None,
        "Enable explicitly authorized developer actions.",
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
        "--sector-workers",
        None,
        Some("1..16|auto"),
        "Sector generation workers.",
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
        "--log-dir",
        None,
        Some("PATH"),
        "Process log directory (default: ./logs).",
        false,
        false,
    ),
    option(
        "--parent-run-id",
        None,
        Some("RUN_ID"),
        "Correlate child and parent process log runs.",
        false,
        false,
    ),
    option(
        "--startup-attempt-id",
        None,
        Some("ID"),
        "Identify this local server startup attempt.",
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
        "--log-dir",
        None,
        Some("PATH"),
        "Process log directory (default: ./logs).",
        false,
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

pub(super) fn parse(
    args: &[String],
    process_environment: &BTreeMap<String, String>,
) -> Result<ServerConfig, CommandError> {
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
    if parsed.contains("--dev") {
        if !explicit.insert("server.dev-mode".to_owned()) {
            return Err(error(
                CommandErrorCode::ConflictingOptions,
                "server.dev-mode",
                "--dev conflicts with --set server.dev-mode=...",
                program,
            ));
        }
        overrides.insert("server.dev-mode".to_owned(), "true".to_owned());
    }
    let resolved = resolve(file_path, overrides, process_environment)?;
    let mut config = config_from(resolved)?;
    if let Some(path) = parsed.get("--log-dir") {
        config.log_dir = Path::new(path).to_path_buf();
    }
    config.startup_attempt_id = parsed.get("--startup-attempt-id").map(str::to_owned);
    Ok(config)
}

pub(super) fn help_text() -> String {
    minimium_commands::format_cli_help("minimium-server run", OPTIONS)
}
