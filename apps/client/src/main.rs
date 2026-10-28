use minimium_core::{identity::PlayerProfileId, world::WorldMap};
use minimium_protocol::{
    decode_server, encode_client, ClientMessage, ServerMessage, ERROR_BUSY, ERROR_PROTOCOL,
    ERROR_RATE_LIMIT, ERROR_SERVER_FULL,
};
use std::{
    io::{self, BufRead, Read, Write},
    net::TcpStream,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const SAMPLE_MAP: &str = include_str!("../../../games/sample/maps/demo.map");
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
pub(crate) const WELCOME_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_CONSOLE_LINE_BYTES: usize = 512;
const CONSOLE_INPUT_QUEUE_CAPACITY: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionState {
    Disconnected,
    AwaitingWelcome { since: Instant },
    Connected,
}

pub(crate) fn server_error_text(code: u8) -> &'static str {
    match code {
        ERROR_PROTOCOL => "server rejected the protocol handshake",
        ERROR_BUSY => "server is busy; this can include a duplicate active profile",
        ERROR_SERVER_FULL => "server is full",
        ERROR_RATE_LIMIT => "request was rate-limited",
        _ => "server rejected the request",
    }
}

fn queue_console_input(sender: &mpsc::SyncSender<String>, bytes: &[u8]) -> bool {
    let line = String::from_utf8_lossy(bytes).into_owned();
    match sender.try_send(line) {
        Ok(()) => true,
        Err(mpsc::TrySendError::Full(_)) => {
            eprintln!("console input queue full; dropping line");
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
                        eprintln!("console input read failed: {error}");
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
                        "console input line exceeds {MAX_CONSOLE_LINE_BYTES} bytes; discarded"
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
            eprintln!("console input line exceeds {MAX_CONSOLE_LINE_BYTES} bytes; discarded");
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
            println!(
                "minimium-client [--connect HOST:PORT] [--map PATH] [--profile-dir PATH] [--server-data-dir PATH] [--threads 1|2|auto] [--demo] [--console]\nDefault: desktop client with a local server. F1 opens the console. --console selects the text client; host local starts/attaches its server."
            );
            return Ok(());
        }
        if options.console {
            run(options)
        } else {
            renderer::run(options)
        }
    })();
    if let Err(error) = result {
        eprintln!("client error: {error}");
        std::process::exit(1);
    }
}

fn run(options: options::ClientOptions) -> Result<(), Box<dyn std::error::Error>> {
    let map_path = options.map_path.clone();
    let map = if let Some(path) = map_path.as_deref() {
        WorldMap::load_file(path)
            .map_err(|error| format!("could not load map {}: {error}", path.display()))?
    } else {
        WorldMap::parse(SAMPLE_MAP).map_err(|error| format!("invalid bundled map: {error}"))?
    };
    let map_fingerprint = map.fingerprint();
    let (profile_dir, data_dir) = options.paths(map_fingerprint);
    let profile_id = PlayerProfileId::load_or_create(&profile_dir)?;
    println!("Minimium client console. Type `help` for commands.");
    let mut connection: Option<TcpStream> = None;
    let mut connection_state = ConnectionState::Disconnected;
    let mut receive_buffer = Vec::with_capacity(8192);
    let mut local_server: Option<bootstrap::LocalServer> = None;
    let (input_sender, input_receiver) = mpsc::sync_channel(CONSOLE_INPUT_QUEUE_CAPACITY);
    spawn_console_input_reader(input_sender);
    if let Some(address) = &options.connect {
        match setup_connection(bootstrap::connect(address), profile_id) {
            Ok((stream, since)) => {
                connection = Some(stream);
                connection_state = ConnectionState::AwaitingWelcome { since };
            }
            Err(error) => println!("connect failed: {error}; you can retry"),
        }
    }
    let mut last_heartbeat_sent_at = Instant::now();
    let mut prompt_needed = true;
    loop {
        if let ConnectionState::AwaitingWelcome { since } = connection_state {
            if Instant::now().saturating_duration_since(since) >= WELCOME_TIMEOUT {
                connection = None;
                connection_state = ConnectionState::Disconnected;
                receive_buffer.clear();
                println!("server welcome timed out; you can reconnect");
            }
        }
        if prompt_needed {
            print!("> ");
            io::stdout().flush()?;
            prompt_needed = false;
        }
        if let Some(stream) = connection.as_mut() {
            if connection_state == ConnectionState::Connected
                && Instant::now().duration_since(last_heartbeat_sent_at) >= HEARTBEAT_INTERVAL
            {
                let heartbeat = encode_client(&ClientMessage::Ping)?;
                match stream.write_all(&heartbeat) {
                    Ok(()) => last_heartbeat_sent_at = Instant::now(),
                    Err(error) => {
                        connection = None;
                        connection_state = ConnectionState::Disconnected;
                        receive_buffer.clear();
                        println!("connection lost ({error}); you can reconnect");
                    }
                }
            }
            receive(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                map_fingerprint,
                true,
            )?;
        }
        let line = match input_receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(input) => input,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        prompt_needed = true;
        let words: Vec<_> = line.split_whitespace().collect();
        match words.as_slice() {
            ["help"] => println!(
                "help | host local [--threads 1|2|auto] | connect HOST:PORT | ping | move X Z | jump | fire | say TEXT | name NAME | state | disconnect | quit"
            ),
            ["host", "local"] | ["host", "local", "--threads", _] => {
                if connection_state != ConnectionState::Disconnected {
                    println!("disconnect or wait for the current connection attempt first");
                    continue;
                }
                let lanes = if words.len() == 4 {
                    words[3]
                } else {
                    &options.threads
                };
                let (stream, server) = match bootstrap::connect_or_start(
                    &data_dir,
                    map_path.as_deref(),
                    lanes,
                ) {
                    Ok(result) => result,
                    Err(error) => {
                        println!("could not start or attach to local server: {error}; you can retry");
                        continue;
                    }
                };
                retain_local_server(&mut local_server, server);
                let (stream, since) = match setup_connection(Ok(stream), profile_id) {
                    Ok(connection) => connection,
                    Err(error) => {
                        println!("local connection setup failed: {error}; you can retry");
                        continue;
                    }
                };
                connection = Some(stream);
                connection_state = ConnectionState::AwaitingWelcome { since };
                println!("awaiting local server welcome");
                last_heartbeat_sent_at = Instant::now();
                receive(
                    &mut connection,
                    &mut connection_state,
                    &mut receive_buffer,
                    &mut last_heartbeat_sent_at,
                    map_fingerprint,
                    false,
                )?;
            }
            ["connect", address] => {
                if connection_state != ConnectionState::Disconnected {
                    println!("disconnect or wait for the current connection attempt first");
                    continue;
                }
                match setup_connection(bootstrap::connect(address), profile_id) {
                    Ok((stream, since)) => {
                        connection = Some(stream);
                        connection_state = ConnectionState::AwaitingWelcome { since };
                        println!("awaiting welcome from {address}");
                        last_heartbeat_sent_at = Instant::now();
                        receive(
                            &mut connection,
                            &mut connection_state,
                            &mut receive_buffer,
                            &mut last_heartbeat_sent_at,
                            map_fingerprint,
                            false,
                        )?;
                    }
                    Err(error) => println!("connect failed: {error}; you can retry"),
                }
            }
            ["ping"] => send_command(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                "PING",
                map_fingerprint,
            )?,
            ["move", x, z] => send_command(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                &format!("MOVE {x} {z}"),
                map_fingerprint,
            )?,
            ["jump"] => send_command(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                "JUMP",
                map_fingerprint,
            )?,
            ["fire"] => send_command(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                "FIRE",
                map_fingerprint,
            )?,
            ["say", text @ ..] if !text.is_empty() => send_command(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                &format!("CHAT {}", text.join(" ")),
                map_fingerprint,
            )?,
            ["name", name] => send_command(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                &format!("NAME {name}"),
                map_fingerprint,
            )?,
            ["state"] => receive(
                &mut connection,
                &mut connection_state,
                &mut receive_buffer,
                &mut last_heartbeat_sent_at,
                map_fingerprint,
                false,
            )?,
            ["disconnect"] => {
                connection = None;
                connection_state = ConnectionState::Disconnected;
                receive_buffer.clear();
                println!("disconnected");
            }
            ["fps", "show"] => println!("client console prototype; renderer not initialized"),
            ["quit"] | ["exit"] => break,
            [] => {}
            _ => println!("unknown command; type `help`"),
        }
    }
    drop(connection);
    if let Some(mut server) = local_server.take() {
        server.shutdown()?;
    }
    Ok(())
}

fn setup_connection(
    connection: io::Result<TcpStream>,
    profile_id: PlayerProfileId,
) -> Result<(TcpStream, Instant), Box<dyn std::error::Error>> {
    let mut stream = connection?;
    stream.set_read_timeout(Some(Duration::from_millis(25)))?;
    stream.set_write_timeout(Some(Duration::from_secs(3)))?;
    let since = send_identity(&mut stream, profile_id)?;
    Ok((stream, since))
}

fn retain_local_server(
    current: &mut Option<bootstrap::LocalServer>,
    new_server: bootstrap::LocalServer,
) {
    match new_server {
        owned @ bootstrap::LocalServer::Owned(_) => {
            if let Some(mut old) = current.replace(owned) {
                if let Err(error) = old.shutdown() {
                    eprintln!("could not clean up previous local server guard: {error}");
                }
            }
        }
        attached @ bootstrap::LocalServer::Attached => {
            let replace = match current.as_mut() {
                Some(server @ bootstrap::LocalServer::Owned(_)) => {
                    match server.is_owned_process_running() {
                        Ok(true) => false,
                        Ok(false) => true,
                        Err(error) => {
                            eprintln!("could not check previous local server process: {error}");
                            false
                        }
                    }
                }
                _ => true,
            };
            if replace {
                if let Some(mut old) = current.replace(attached) {
                    if let Err(error) = old.shutdown() {
                        eprintln!("could not clean up previous local server guard: {error}");
                    }
                }
            }
        }
    }
}

fn send_command(
    connection: &mut Option<TcpStream>,
    connection_state: &mut ConnectionState,
    receive_buffer: &mut Vec<u8>,
    last_heartbeat_sent_at: &mut Instant,
    command: &str,
    map_fingerprint: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    if *connection_state != ConnectionState::Connected {
        if matches!(*connection_state, ConnectionState::AwaitingWelcome { .. }) {
            println!("waiting for server welcome; command was not sent");
        } else {
            println!("not connected");
        }
        return Ok(());
    }
    let Some(stream) = connection.as_mut() else {
        println!("not connected");
        return Ok(());
    };
    let message = match command.split_whitespace().collect::<Vec<_>>().as_slice() {
        ["PING"] => ClientMessage::Ping,
        ["MOVE", x, z] => ClientMessage::Move {
            direction: [x.parse::<f32>()?, z.parse::<f32>()?],
            jump: false,
        },
        ["JUMP"] => ClientMessage::Move {
            direction: [0.0; 2],
            jump: true,
        },
        ["FIRE"] => ClientMessage::Fire {
            direction: [1.0, 0.0, 0.0],
        },
        ["CHAT", text @ ..] if !text.is_empty() => ClientMessage::Chat(text.join(" ")),
        ["NAME", name] => ClientMessage::SetName((*name).to_owned()),
        ["QUIT"] => ClientMessage::Quit,
        _ => return Err("invalid client command".into()),
    };
    if let Err(error) = stream.write_all(&encode_client(&message)?) {
        *connection = None;
        *connection_state = ConnectionState::Disconnected;
        receive_buffer.clear();
        println!("connection lost ({error}); you can reconnect");
        return Ok(());
    }
    receive(
        connection,
        connection_state,
        receive_buffer,
        last_heartbeat_sent_at,
        map_fingerprint,
        false,
    )
}

fn send_identity(
    stream: &mut TcpStream,
    profile_id: PlayerProfileId,
) -> Result<Instant, Box<dyn std::error::Error>> {
    stream.write_all(&encode_client(&ClientMessage::Identify(
        *profile_id.as_bytes(),
    ))?)?;
    Ok(Instant::now())
}

fn receive(
    connection: &mut Option<TcpStream>,
    connection_state: &mut ConnectionState,
    receive_buffer: &mut Vec<u8>,
    last_heartbeat_sent_at: &mut Instant,
    expected_map_fingerprint: u64,
    quiet: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(stream) = connection.as_mut() else {
        *connection_state = ConnectionState::Disconnected;
        receive_buffer.clear();
        if !quiet {
            println!("not connected");
        }
        return Ok(());
    };
    let mut buffer = [0u8; 8192];
    match stream.read(&mut buffer) {
        Ok(0) => {
            *connection = None;
            *connection_state = ConnectionState::Disconnected;
            receive_buffer.clear();
            println!("server disconnected; you can reconnect");
            return Ok(());
        }
        Ok(count) => receive_buffer.extend_from_slice(&buffer[..count]),
        Err(error)
            if error.kind() == io::ErrorKind::WouldBlock
                || error.kind() == io::ErrorKind::TimedOut => {}
        Err(error) => {
            *connection = None;
            *connection_state = ConnectionState::Disconnected;
            receive_buffer.clear();
            println!("connection lost ({error}); you can reconnect");
            return Ok(());
        }
    }
    if receive_buffer.len() > 64 * 1024 {
        *connection = None;
        *connection_state = ConnectionState::Disconnected;
        receive_buffer.clear();
        println!("server frame exceeds protocol limits; you can reconnect");
        return Ok(());
    }
    let mut received_message = false;
    loop {
        match decode_server(receive_buffer) {
            Ok(Some((message, frame_bytes))) => {
                received_message = true;
                receive_buffer.drain(..frame_bytes);
                match message {
                    ServerMessage::Welcome {
                        id,
                        map_fingerprint,
                        health,
                    } => {
                        if !matches!(*connection_state, ConnectionState::AwaitingWelcome { .. }) {
                            *connection = None;
                            *connection_state = ConnectionState::Disconnected;
                            receive_buffer.clear();
                            println!("unexpected server welcome; connection closed");
                            return Ok(());
                        }
                        if map_fingerprint != expected_map_fingerprint {
                            println!(
                                "map mismatch: client={expected_map_fingerprint:016x} server={map_fingerprint:016x}"
                            );
                            *connection = None;
                            *connection_state = ConnectionState::Disconnected;
                            receive_buffer.clear();
                            return Ok(());
                        }
                        *connection_state = ConnectionState::Connected;
                        *last_heartbeat_sent_at = Instant::now();
                        println!("connected as player {id} ({health} HP)");
                    }
                    ServerMessage::Error(code) => {
                        let message = server_error_text(code);
                        if *connection_state != ConnectionState::Connected {
                            *connection = None;
                            *connection_state = ConnectionState::Disconnected;
                            receive_buffer.clear();
                            println!("server rejected connection: {message}; you can reconnect");
                            return Ok(());
                        }
                        println!("server error: {message}");
                    }
                    ServerMessage::Bye => {
                        *connection = None;
                        *connection_state = ConnectionState::Disconnected;
                        receive_buffer.clear();
                        println!("server closed the session; you can reconnect");
                        return Ok(());
                    }
                    _ if *connection_state != ConnectionState::Connected => {
                        *connection = None;
                        *connection_state = ConnectionState::Disconnected;
                        receive_buffer.clear();
                        println!("server sent data before welcome; connection closed");
                        return Ok(());
                    }
                    ServerMessage::Pong if !quiet => println!("PONG"),
                    ServerMessage::Accepted if !quiet => println!("ok"),
                    ServerMessage::ShotResult {
                        target_id,
                        remaining_health,
                        killed,
                    } if !quiet => match (target_id, remaining_health, killed) {
                        (Some(target_id), Some(health), true) => {
                            println!("hit player {target_id}; target respawned ({health} HP)")
                        }
                        (Some(target_id), Some(health), false) => {
                            println!("hit player {target_id} ({health} HP)")
                        }
                        _ => println!("shot missed"),
                    },
                    ServerMessage::HealthUpdate(health) if !quiet => {
                        println!("health: {health} HP")
                    }
                    ServerMessage::Chat {
                        sender_id,
                        sender_name,
                        text,
                    } => println!("[{sender_name}#{sender_id}] {text}"),
                    ServerMessage::Snapshot {
                        server_time_ms,
                        full_snapshot,
                        self_position,
                        self_velocity,
                        self_grounded,
                        players,
                        removed_players,
                    } if !quiet => println!(
                        "server_time={}ms full={} self=({:.2},{:.2},{:.2}) velocity=({:.2},{:.2},{:.2}) grounded={} player_updates={} player_removals={}",
                        server_time_ms,
                        full_snapshot,
                        self_position[0],
                        self_position[1],
                        self_position[2],
                        self_velocity[0],
                        self_velocity[1],
                        self_velocity[2],
                        self_grounded,
                        players.len(),
                        removed_players.len()
                    ),
                    _ => {}
                }
            }
            Ok(None) => break,
            Err(error) => {
                *connection = None;
                *connection_state = ConnectionState::Disconnected;
                receive_buffer.clear();
                println!("invalid server protocol data ({error}); you can reconnect");
                return Ok(());
            }
        }
    }
    if !received_message && !quiet {
        println!("no server response yet");
    }
    Ok(())
}

mod console;
mod renderer;

mod bootstrap;
mod options;
mod stats;
