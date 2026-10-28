use messages::ClientText as Text;
pub(crate) const SAMPLE_MAP: &str = include_str!("../../../games/sample/maps/demo.map");
use std::{
    io::{self, BufRead},
    sync::mpsc,
    thread,
};

const MAX_CONSOLE_LINE_BYTES: usize = 512;

fn queue_console_input(sender: &mpsc::SyncSender<String>, bytes: &[u8]) -> bool {
    let line = String::from_utf8_lossy(bytes).into_owned();
    match sender.try_send(line) {
        Ok(()) => true,
        Err(mpsc::TrySendError::Full(_)) => {
            eprintln!("{}", Text::ConsoleInputQueueFull);
            true
        }
        Err(mpsc::TrySendError::Disconnected(_)) => false,
    }
}

fn spawn_console_input_reader(sender: mpsc::SyncSender<String>) {
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
                        eprintln!("{}", Text::ConsoleInputReadFailed(error.to_string()));
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
                    eprintln!(
                        "{}",
                        Text::ConsoleInputTooLong {
                            max_bytes: MAX_CONSOLE_LINE_BYTES,
                        }
                    );
                } else {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    if !queue_console_input(&sender, &line) {
                        break;
                    }
                }
                line.clear();
                overlong = false;
            }
        }

        if overlong {
            eprintln!(
                "{}",
                Text::ConsoleInputTooLong {
                    max_bytes: MAX_CONSOLE_LINE_BYTES,
                }
            );
        } else if !line.is_empty() {
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            let _ = queue_console_input(&sender, &line);
        }
    });
}

fn main() {
    let result = (|| {
        let options = options::ClientOptions::parse(std::env::args().skip(1))?;
        if options.help {
            println!("{}", Text::CommandLineHelp);
            return Ok(());
        }
        if options.console {
            terminal_client::run(options)
        } else {
            renderer::run(options)
        }
    })();
    if let Err(error) = result {
        eprintln!("{}", Text::ClientError(error.to_string()));
        std::process::exit(1);
    }
}

mod console;
mod renderer;

mod bootstrap;
mod client_commands;
mod connection_controller;
mod connector;
mod messages;
mod options;
mod session;
mod stats;
mod terminal_client;
