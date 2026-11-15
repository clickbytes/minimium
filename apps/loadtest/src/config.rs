use minimium_commands::{
    format_cli_help, parse_cli, CliOptionSpec, CliParseOutcome, CliValues, CommandError,
    CommandErrorCode,
};
use std::{path::PathBuf, thread};

use super::scenarios::Scenario;

const OPTIONS: &[CliOptionSpec] = &[
    CliOptionSpec {
        name: "--address",
        alias: None,
        value_name: Some("HOST:PORT"),
        description: "Server address; required for a run.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--scenario",
        alias: None,
        value_name: Some("NAME"),
        description: "heartbeat, movement, combat, chat, clustered, or dispersed.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--clients",
        alias: None,
        value_name: Some("N"),
        description: "Client count from 1 through 10000 (default 100).",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--seconds",
        alias: None,
        value_name: Some("N"),
        description: "Run duration from 1 through 86400 seconds (default 10).",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--threads",
        alias: None,
        value_name: Some("1|2|auto"),
        description: "Worker selection; auto uses at most two workers.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--format",
        alias: None,
        value_name: Some("text|json"),
        description: "Report format (default text).",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--scenario-id",
        alias: None,
        value_name: Some("TEXT"),
        description: "Optional report scenario identifier (maximum 512 bytes).",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--cpu-label",
        alias: None,
        value_name: Some("TEXT"),
        description: "Optional user supplied CPU label (maximum 512 bytes).",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--cpu-mhz",
        alias: None,
        value_name: Some("N"),
        description: "Optional positive user supplied CPU clock in MHz.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--server-data-dir",
        alias: None,
        value_name: Some("PATH"),
        description: "Sample authenticated local server status around the run.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--log-dir",
        alias: None,
        value_name: Some("PATH"),
        description: "Select the process log directory (default: ./logs).",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--help",
        alias: Some("-h"),
        value_name: None,
        description: "Show this help and exit without network or workers.",
        repeatable: false,
        help: true,
    },
];

#[derive(Clone, Copy)]
pub(super) enum WorkerSelection {
    One,
    Two,
    Auto,
}

impl WorkerSelection {
    fn worker_limit(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Auto => thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(2),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::One => "1",
            Self::Two => "2",
            Self::Auto => "auto",
        }
    }
}

pub(super) struct StartupConfig {
    pub(super) address: String,
    pub(super) clients: usize,
    pub(super) seconds: u64,
    pub(super) worker_selection: WorkerSelection,
    pub(super) output_format: OutputFormat,
    pub(super) scenario: Scenario,
    pub(super) cpu_label: Option<String>,
    pub(super) cpu_mhz: Option<u32>,
    pub(super) scenario_id: Option<String>,
    pub(super) server_data_dir: Option<PathBuf>,
    pub(super) log_dir: Option<PathBuf>,
}

#[derive(Clone, Copy)]
pub(super) enum OutputFormat {
    Text,
    Json,
}

pub(super) enum ParseOutcome {
    Help,
    Config(StartupConfig),
}

impl StartupConfig {
    pub(super) fn worker_count(&self) -> usize {
        self.worker_selection.worker_limit().min(self.clients)
    }

    pub(super) fn thread_mode(&self) -> &'static str {
        self.worker_selection.as_str()
    }
}

pub(super) fn help() -> String {
    format_cli_help("minimium-loadtest --address HOST:PORT [options]", OPTIONS)
}

pub(super) fn parse_env() -> Result<ParseOutcome, CommandError> {
    parse_args(std::env::args().skip(1))
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<ParseOutcome, CommandError> {
    let values = match parse_cli(args, "minimium-loadtest", OPTIONS)? {
        CliParseOutcome::Help => return Ok(ParseOutcome::Help),
        CliParseOutcome::Parsed(values) => values,
    };
    reject_positionals(&values)?;
    let address = values.get("--address").ok_or_else(|| {
        cli_error(
            CommandErrorCode::MissingValue,
            "--address",
            "--address is required",
        )
    })?;
    let clients = parse_number::<usize>(&values, "--clients", 100)?;
    if !(1..=10_000).contains(&clients) {
        return Err(cli_error(
            CommandErrorCode::InvalidValue,
            "--clients",
            "clients must be between 1 and 10000",
        ));
    }
    let seconds = parse_number::<u64>(&values, "--seconds", 10)?;
    if !(1..=86_400).contains(&seconds) {
        return Err(cli_error(
            CommandErrorCode::InvalidValue,
            "--seconds",
            "seconds must be between 1 and 86400",
        ));
    }
    let worker_selection = match values.get("--threads").unwrap_or("auto") {
        "1" => WorkerSelection::One,
        "2" => WorkerSelection::Two,
        "auto" => WorkerSelection::Auto,
        _ => {
            return Err(cli_error(
                CommandErrorCode::InvalidValue,
                "--threads",
                "threads must be 1, 2, or auto",
            ));
        }
    };
    let output_format = match values.get("--format").unwrap_or("text") {
        "text" => OutputFormat::Text,
        "json" => OutputFormat::Json,
        _ => {
            return Err(cli_error(
                CommandErrorCode::InvalidValue,
                "--format",
                "format must be text or json",
            ));
        }
    };
    let scenario = match values.get("--scenario").unwrap_or("heartbeat") {
        "heartbeat" => Scenario::Heartbeat,
        "movement" => Scenario::Movement,
        "combat" => Scenario::Combat,
        "chat" => Scenario::Chat,
        "clustered" => Scenario::Clustered,
        "dispersed" => Scenario::Dispersed,
        _ => {
            return Err(cli_error(
                CommandErrorCode::InvalidValue,
                "--scenario",
                "scenario must be heartbeat, movement, combat, chat, clustered, or dispersed",
            ));
        }
    };
    let scenario_id = text_value(&values, "--scenario-id")?;
    let cpu_label = text_value(&values, "--cpu-label")?;
    let cpu_mhz = values
        .get("--cpu-mhz")
        .map(|value| {
            value.parse::<u32>().map_err(|_| {
                cli_error(
                    CommandErrorCode::InvalidValue,
                    "--cpu-mhz",
                    "cpu-mhz must be a positive integer",
                )
            })
        })
        .transpose()?;
    if cpu_mhz == Some(0) {
        return Err(cli_error(
            CommandErrorCode::InvalidValue,
            "--cpu-mhz",
            "cpu-mhz must be positive",
        ));
    }
    let server_data_dir = values.get("--server-data-dir").map(PathBuf::from);
    let log_dir = values.get("--log-dir").map(PathBuf::from);
    Ok(ParseOutcome::Config(StartupConfig {
        address: address.to_owned(),
        clients,
        seconds,
        worker_selection,
        output_format,
        scenario,
        cpu_label,
        cpu_mhz,
        scenario_id,
        server_data_dir,
        log_dir,
    }))
}

fn parse_number<T>(values: &CliValues, name: &str, default: T) -> Result<T, CommandError>
where
    T: std::str::FromStr,
{
    values
        .get(name)
        .map(|value| {
            value.parse().map_err(|_| {
                cli_error(
                    CommandErrorCode::InvalidValue,
                    name,
                    format!("{name} must be a number"),
                )
            })
        })
        .unwrap_or(Ok(default))
}

fn text_value(values: &CliValues, name: &str) -> Result<Option<String>, CommandError> {
    let value = values.get(name).map(str::to_owned);
    if value.as_ref().is_some_and(|value| value.len() > 512) {
        return Err(cli_error(
            CommandErrorCode::InvalidValue,
            name,
            format!("{name} exceeds 512 bytes"),
        ));
    }
    Ok(value)
}

fn reject_positionals(values: &CliValues) -> Result<(), CommandError> {
    if values.positionals().is_empty() {
        Ok(())
    } else {
        Err(cli_error(
            CommandErrorCode::InvalidArguments,
            "--",
            "unexpected positional arguments",
        ))
    }
}

fn cli_error(
    code: CommandErrorCode,
    subject: impl Into<String>,
    reason: impl Into<String>,
) -> CommandError {
    CommandError::new(code, Some(subject.into()), reason, Some(help()))
}
