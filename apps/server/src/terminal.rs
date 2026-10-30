use minimium_platform::commands::ServerCommand;
use std::{
    io::{self, Read},
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
    thread,
};

const TERMINAL_QUEUE_CAPACITY: usize = 64;
const MAX_TERMINAL_LINE_BYTES: usize = 512;
const READ_BUFFER_BYTES: usize = 256;

pub fn spawn() -> io::Result<Receiver<ServerCommand>> {
    let (sender, receiver) = mpsc::sync_channel(TERMINAL_QUEUE_CAPACITY);
    thread::Builder::new()
        .name("minimium-console-input".to_owned())
        .spawn(move || read_commands(sender))?;
    Ok(receiver)
}

fn read_commands(sender: SyncSender<ServerCommand>) {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut buffer = [0u8; READ_BUFFER_BYTES];
    let mut line = Vec::with_capacity(MAX_TERMINAL_LINE_BYTES);
    let mut overlong = false;

    loop {
        let count = match input.read(&mut buffer) {
            Ok(0) => {
                if !line.is_empty() || overlong {
                    submit_line(&sender, &mut line, &mut overlong);
                }
                return;
            }
            Ok(count) => count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => {
                eprintln!("ERR terminal input: {error}");
                return;
            }
        };

        for byte in buffer[..count].iter().copied() {
            if byte == b'\n' {
                submit_line(&sender, &mut line, &mut overlong);
            } else if !overlong {
                if line.len() == MAX_TERMINAL_LINE_BYTES {
                    line.clear();
                    overlong = true;
                } else {
                    line.push(byte);
                }
            }
        }
    }
}

fn submit_line(sender: &SyncSender<ServerCommand>, line: &mut Vec<u8>, overlong: &mut bool) {
    if *overlong {
        eprintln!("ERR terminal command exceeds 512 bytes");
        line.clear();
        *overlong = false;
        return;
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    let command_line = match std::str::from_utf8(line) {
        Ok(command_line) => command_line,
        Err(_) => {
            eprintln!("ERR terminal command is not UTF-8");
            line.clear();
            return;
        }
    };
    if command_line.trim().is_empty() {
        line.clear();
        return;
    }
    match ServerCommand::parse(command_line) {
        Ok(command) => match sender.try_send(command) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => eprintln!("ERR command queue full; command dropped"),
            Err(TrySendError::Disconnected(_)) => eprintln!("ERR command queue closed"),
        },
        Err(error) => eprintln!("ERR command: {error}"),
    }
    line.clear();
}
