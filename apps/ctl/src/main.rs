use minimium_commands::{
    format_cli_help, format_command_arguments, parse_cli, CliOptionSpec, CliParseOutcome,
    CommandError,
};
use minimium_platform::{admin_client, endpoint::ServerEndpoint};
use std::{error::Error, path::PathBuf};

const OPTIONS: &[CliOptionSpec] = &[
    CliOptionSpec {
        name: "--format",
        alias: None,
        value_name: Some("text|json"),
        description: "Response format. JSON selects the versioned administrative protocol.",
        repeatable: false,
        help: false,
    },
    CliOptionSpec {
        name: "--data-dir",
        alias: None,
        value_name: Some("PATH"),
        description: "Server data and endpoint directory.",
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
        description: "Show help without reading endpoint state.",
        repeatable: false,
        help: true,
    },
    CliOptionSpec {
        name: "--version",
        alias: None,
        value_name: None,
        description: "Show version without contacting a server.",
        repeatable: false,
        help: false,
    },
];

struct CtlError {
    exit_code: i32,
    message: String,
}

impl CtlError {
    fn arguments(error: CommandError) -> Self {
        Self {
            exit_code: 2,
            message: error.to_string(),
        }
    }

    fn runtime(error: impl Error) -> Self {
        Self {
            exit_code: 1,
            message: error.to_string(),
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("minimiumctl: {}", error.message);
        std::process::exit(error.exit_code);
    }
}

fn run() -> Result<(), CtlError> {
    let parsed = match parse_cli(
        std::env::args().skip(1),
        "minimiumctl [OPTIONS] [--] <server command>",
        OPTIONS,
    )
    .map_err(CtlError::arguments)?
    {
        CliParseOutcome::Help => {
            println!(
                "{}",
                format_cli_help("minimiumctl [OPTIONS] [--] <server command>", OPTIONS)
            );
            return Ok(());
        }
        CliParseOutcome::Parsed(values) => values,
    };
    if parsed.contains("--version") {
        println!("minimiumctl {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let json = match parsed.get("--format").unwrap_or("text") {
        "json" => true,
        "text" => false,
        _ => {
            return Err(CtlError {
                exit_code: 2,
                message: "--format accepts text or json".into(),
            })
        }
    };
    let command = format_command_arguments(parsed.positionals()).map_err(CtlError::arguments)?;
    let typed = if json {
        let request = admin_client::text_request(command.clone()).map_err(CtlError::runtime)?;
        request.encode().map_err(CtlError::arguments)?;
        Some(request)
    } else {
        None
    };
    let data_dir = parsed
        .get("--data-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(minimium_platform::files::DEFAULT_SERVER_DATA_DIR));
    let paths = minimium_platform::app_paths::AppPaths::resolve(
        &data_dir,
        &data_dir,
        &data_dir,
        &data_dir,
        parsed.get("--log-dir").map(std::path::Path::new),
    )
    .map_err(CtlError::runtime)?;
    let mut diagnostics = minimium_telemetry::logging::DiagnosticsRuntime::start(&paths.log_dir)
        .map_err(CtlError::runtime)?;
    let logger = diagnostics.logger(minimium_diagnostics::DomainId::Platform);
    logger.emit(logger.event(
        minimium_diagnostics::Level::Info,
        minimium_diagnostics::EventCode::new("ctl.starting").map_err(CtlError::runtime)?,
    ));
    let data_dir = paths.data_dir;

    // Endpoint state is read only after every local option and command boundary is parsed.
    let endpoint = ServerEndpoint::read(&data_dir).map_err(CtlError::runtime)?;
    let stop = command.trim() == "stop"
        || minimium_commands::syntax::canonical::parse_statement(&command).is_ok_and(|statement| {
            statement.name == "server.stop"
                && statement.access == minimium_commands::syntax::canonical::AccessKind::Invoke
                && statement.arguments.is_empty()
        });
    if let Some(request) = typed {
        let reply = admin_client::request_v1(&endpoint, &request);
        let line = reply
            .encode(request.correlation)
            .map_err(CtlError::runtime)?;
        println!(
            "{}",
            line.trim_start_matches(minimium_commands::admin::PREFIX)
        );
        if reply.result.outcome.is_err()
            || reply.result.receipt.stage == minimium_commands::results::Stage::Partial
        {
            return Err(CtlError {
                exit_code: 1,
                message: format!("operation {}", reply.result.receipt.stage.label()),
            });
        }
        if stop && reply.result.receipt.should_stop {
            admin_client::wait_for_shutdown(&data_dir, &endpoint).map_err(CtlError::runtime)?;
        }
    } else if stop {
        admin_client::stop_and_wait(&data_dir, &endpoint).map_err(CtlError::runtime)?;
        println!("server stopped cleanly");
    } else {
        let response = admin_client::request(&endpoint, &command).map_err(CtlError::runtime)?;
        println!("{}", if response.is_empty() { "ok" } else { &response });
    }
    drop(logger);
    diagnostics
        .shutdown_until(std::time::Instant::now() + std::time::Duration::from_secs(2))
        .map_err(CtlError::runtime)?;
    Ok(())
}
