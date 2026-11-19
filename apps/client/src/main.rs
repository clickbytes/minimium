use messages::ClientText as Text;
mod host_time;
use std::{
    io::{self, BufRead},
    sync::mpsc,
    thread,
};

use minimium_commands::MAX_INPUT_BYTES;
type ConsoleInput = Result<String, minimium_commands::CommandError>;

fn queue_console_input(
    sender: &mpsc::SyncSender<ConsoleInput>,
    line: ConsoleInput,
    logger: &minimium_diagnostics::DomainLogger,
) -> bool {
    match sender.try_send(line) {
        Ok(()) => true,
        Err(mpsc::TrySendError::Full(_)) => {
            emit_console_diagnostic(
                logger,
                "client_runtime.console_input_queue_full",
                minimium_diagnostics::Level::Warn,
                None,
            );
            true
        }
        Err(mpsc::TrySendError::Disconnected(_)) => false,
    }
}

pub(crate) fn spawn_console_input_reader(
    sender: mpsc::SyncSender<ConsoleInput>,
    logger: minimium_diagnostics::DomainLogger,
) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        let mut line = Vec::with_capacity(MAX_INPUT_BYTES + 1);
        let mut length = 0usize;
        let mut last = None;

        loop {
            let mut consumed = 0;
            let mut complete = false;
            let mut eof = false;
            {
                let available = match stdin.fill_buf() {
                    Ok(available) => available,
                    Err(error) => {
                        emit_console_diagnostic(
                            &logger,
                            "client_runtime.console_input_read_failed",
                            minimium_diagnostics::Level::Error,
                            Some(error.to_string()),
                        );
                        break;
                    }
                };
                if available.is_empty() {
                    eof = true;
                } else {
                    let newline = available.iter().position(|byte| *byte == b'\n');
                    consumed = newline.map_or(available.len(), |index| index + 1);
                    let content_bytes = newline.unwrap_or(consumed);
                    let chunk = &available[..content_bytes];
                    if let Some(byte) = chunk.last() {
                        last = Some(*byte);
                    }
                    length = length.saturating_add(content_bytes);
                    if length <= MAX_INPUT_BYTES + 1 {
                        line.extend_from_slice(chunk);
                    }
                    complete = newline.is_some();
                }
            }

            stdin.consume(consumed);
            if complete || (eof && length > 0) {
                let bytes = length - usize::from(complete && last == Some(b'\r'));
                let input = if bytes > MAX_INPUT_BYTES {
                    Err(minimium_commands::input_too_long(bytes))
                } else {
                    String::from_utf8(line[..bytes].to_vec()).map_err(|_| {
                        minimium_commands::CommandError::new(
                            minimium_commands::CommandErrorCode::InvalidValue,
                            Some("input"),
                            "invalid UTF-8",
                            None::<String>,
                        )
                    })
                };
                if !queue_console_input(&sender, input, &logger) {
                    break;
                }
                line.clear();
                length = 0;
                last = None;
            }
            if eof {
                break;
            }
        }
    });
}

fn emit_console_diagnostic(
    logger: &minimium_diagnostics::DomainLogger,
    code: &'static str,
    level: minimium_diagnostics::Level,
    reason: Option<String>,
) {
    if let Ok(code) = minimium_diagnostics::EventCode::new(code) {
        let mut event = logger.event(level, code);
        if let Some(reason) = reason {
            event = event.field("reason", &reason);
        }
        logger.emit(event);
    }
}

fn main() {
    let mut locale = minimium_localization::Locale::English;
    let result = (|| {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        if let Some(output) = config::handle_offline_command(&args)? {
            println!("{output}");
            return Ok(());
        }
        let options = options::ClientOptions::parse(args)?;
        locale = options.locale;
        if options.help {
            println!("{}", options::help_text());
            return Ok(());
        }
        if options.version {
            println!("{}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        client_commands::initialize_registry();
        if options.console {
            terminal_client::run(options)
        } else {
            #[cfg(feature = "desktop")]
            {
                renderer::run(options)
            }
            #[cfg(not(feature = "desktop"))]
            {
                Err("desktop support was not compiled; use --console".into())
            }
        }
    })();
    if let Err(error) = result {
        let display = match minimium_localization::Localizer::new(None) {
            Ok(localizer) => Text::ClientError(error.to_string()).localized(&localizer, locale),
            Err(localization_error) => {
                format!("Client error: {error} (localization unavailable: {localization_error})")
            }
        };
        eprintln!("{display}");
        let exit_code = if error
            .downcast_ref::<minimium_commands::CommandError>()
            .is_some()
        {
            2
        } else {
            1
        };
        std::process::exit(exit_code);
    }
}

#[cfg(feature = "desktop")]
mod audio;
#[cfg(feature = "desktop")]
mod console;
#[cfg(feature = "desktop")]
mod renderer;

mod application;
mod bootstrap;
mod client_commands;
mod config;
mod content_jobs;
mod diagnostics;
mod environment;
mod input;
mod messages;
mod options;
pub(crate) use minimium_client_runtime as session;
#[cfg(feature = "desktop")]
mod stats;
mod terminal_client;
mod world_content;
