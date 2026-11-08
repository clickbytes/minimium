use messages::ClientText as Text;
use std::{
    io::{self, BufRead},
    sync::mpsc,
    thread,
};

const MAX_CONSOLE_LINE_BYTES: usize = 512;

fn queue_console_input(
    sender: &mpsc::SyncSender<String>,
    bytes: &[u8],
    logger: &minimium_diagnostics::DomainLogger,
) -> bool {
    let line = String::from_utf8_lossy(bytes).into_owned();
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
    sender: mpsc::SyncSender<String>,
    logger: minimium_diagnostics::DomainLogger,
) {
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut stdin = stdin.lock();
        let mut line = Vec::with_capacity(MAX_CONSOLE_LINE_BYTES);
        let mut overlong = false;

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
                    if !overlong {
                        if line.len().saturating_add(content_bytes) <= MAX_CONSOLE_LINE_BYTES {
                            line.extend_from_slice(&available[..content_bytes]);
                        } else {
                            overlong = true;
                            line.clear();
                        }
                    }
                    complete = newline.is_some();
                }
            }

            if eof {
                break;
            }
            stdin.consume(consumed);
            if complete {
                if overlong {
                    emit_console_diagnostic(
                        &logger,
                        "client_runtime.console_input_too_long",
                        minimium_diagnostics::Level::Warn,
                        None,
                    );
                } else {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    if !queue_console_input(&sender, &line, &logger) {
                        break;
                    }
                }
                line.clear();
                overlong = false;
            }
        }

        if overlong {
            emit_console_diagnostic(
                &logger,
                "client_runtime.console_input_too_long",
                minimium_diagnostics::Level::Warn,
                None,
            );
        } else if !line.is_empty() {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let _ = queue_console_input(&sender, &line, &logger);
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
mod console;
#[cfg(feature = "desktop")]
mod renderer;

mod application;
mod client_commands;
mod config;
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
