use minimium_commands::{
    format_cli_help, format_command_arguments, parse_cli, CliOptionSpec, CliParseOutcome,
    CommandError,
};
use minimium_platform::{admin_client, endpoint::ServerEndpoint};
use std::{error::Error, path::PathBuf};

const OPTIONS: &[CliOptionSpec] = &[
    CliOptionSpec {
        name: "--data-dir",
        alias: None,
        value_name: Some("PATH"),
        description: "Server data and endpoint directory.",
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
    let command = format_command_arguments(parsed.positionals()).map_err(CtlError::arguments)?;
    let data_dir = parsed
        .get("--data-dir")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(minimium_platform::files::DEFAULT_SERVER_DATA_DIR));

    // Endpoint state is read only after every local option and command boundary is parsed.
    let endpoint = ServerEndpoint::read(&data_dir).map_err(CtlError::runtime)?;
    if command.trim() == "stop" {
        admin_client::stop_and_wait(&data_dir, &endpoint).map_err(CtlError::runtime)?;
        println!("server stopped cleanly");
    } else {
        let response = admin_client::request(&endpoint, &command).map_err(CtlError::runtime)?;
        println!("{}", if response.is_empty() { "ok" } else { &response });
    }
    Ok(())
}
