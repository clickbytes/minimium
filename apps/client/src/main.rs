use minimium_core::{endpoint::ServerEndpoint, identity::PlayerProfileId, world::WorldMap};
use minimium_protocol::{decode_server, encode_client, ClientMessage, ServerMessage};
use std::{
    env,
    io::{self, BufRead, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const SAMPLE_MAP: &str = include_str!("../../../games/sample/maps/demo.map");

fn main() {
    if std::env::args().any(|arg| arg == "--help" || arg == "-h") {
        println!("minimium-client [--connect HOST:PORT] [--map PATH] [--demo] [--console]\nDefault: open the desktop client and start a local server if none is running.\nF1 opens the in-game command console; --demo opens without a server; --console opens the text prototype.");
        return;
    }
    if !std::env::args().any(|arg| arg == "--console") {
        if let Err(error) = renderer::run() {
            eprintln!("renderer error: {error}");
            std::process::exit(1);
        }
        return;
    }
    if let Err(error) = run() {
        eprintln!("client error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let startup_args: Vec<String> = env::args().skip(1).collect();
    if startup_args
        .iter()
        .any(|arg| arg == "--help" || arg == "-h")
    {
        println!("Minimium client console\nCommands: host local [--threads 1|2|auto], connect HOST:PORT, ping, move X Z, jump, say NAME, state, disconnect, quit");
        return Ok(());
    }
    let mut map_path = None;
    let mut index = 0;
    while index < startup_args.len() {
        if startup_args[index] == "--map" {
            if map_path.is_some() {
                return Err("--map may only be specified once".into());
            }
            index += 1;
            map_path = Some(PathBuf::from(
                startup_args.get(index).ok_or("--map requires a path")?,
            ));
        }
        index += 1;
    }
    let map = if let Some(path) = map_path.as_deref() {
        WorldMap::load_file(path)
            .map_err(|error| format!("could not load map {}: {error}", path.display()))?
    } else {
        WorldMap::parse(SAMPLE_MAP).map_err(|error| format!("invalid bundled map: {error}"))?
    };
    let map_fingerprint = map.fingerprint();
    let data_dir = if map_path.is_some() {
        PathBuf::from(".minimium-local").join(format!("map-{map_fingerprint:016x}"))
    } else {
        PathBuf::from(".minimium-local")
    };
    let profile_id = PlayerProfileId::load_or_create(&data_dir)?;
    println!("Minimium client console. Type `help` for commands.");
    let stdin = io::stdin();
    let mut connection: Option<TcpStream> = None;
    let mut receive_buffer = Vec::with_capacity(8192);
    let mut local_server: Option<Child> = None;
    let mut line = String::new();
    loop {
        print!("> ");
        io::stdout().flush()?;
        line.clear();
        if stdin.lock().read_line(&mut line)? == 0 {
            break;
        }
        let words: Vec<_> = line.split_whitespace().collect();
        match words.as_slice() {
            ["help"] => println!("help | host local [--threads 1|2|auto] | connect HOST:PORT | ping | move X Z | jump | say NAME | state | disconnect | quit"),
            ["host", "local"] | ["host", "local", "--threads", _] => {
                if connection.is_some() { println!("disconnect before starting a local server"); continue; }
                let lanes = if words.len() == 4 { words[3] } else { "1" };
                let server_bin = sibling_binary("minimium-server")?;
                let mut server = Command::new(server_bin);
                server
                    .arg("run")
                    .arg("--headless")
                    .arg("--threads")
                    .arg(lanes)
                    .arg("--data-dir")
                    .arg(&data_dir);
                if let Some(map_path) = &map_path {
                    server.arg("--map").arg(map_path);
                }
                let child = server
                    .stdin(Stdio::null())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()?;
                local_server = Some(child);
                let endpoint = wait_for_endpoint(&data_dir)?;
                let mut stream = TcpStream::connect(("127.0.0.1", endpoint.game_port))?;
                stream.set_read_timeout(Some(Duration::from_millis(200)))?;
                send_identity(&mut stream, profile_id)?;
                connection = Some(stream);
                println!("connected to local server on port {}", endpoint.game_port);
                receive(&mut connection, &mut receive_buffer, map_fingerprint)?;
            }
            ["connect", address] => {
                let mut stream = TcpStream::connect(address.to_socket_addrs()?.next().ok_or("invalid address")?)?;
                stream.set_read_timeout(Some(Duration::from_millis(200)))?;
                send_identity(&mut stream, profile_id)?;
                connection = Some(stream);
                println!("connected to {address}");
                receive(&mut connection, &mut receive_buffer, map_fingerprint)?;
            }
            ["ping"] => send_command(
                &mut connection,
                &mut receive_buffer,
                "PING",
                map_fingerprint,
            )?,
            ["move", x, z] => {
                send_command(
                    &mut connection,
                    &mut receive_buffer,
                    &format!("MOVE {x} {z}"),
                    map_fingerprint,
                )?
            }
            ["jump"] => send_command(
                &mut connection,
                &mut receive_buffer,
                "JUMP",
                map_fingerprint,
            )?,
            ["say", name] => {
                send_command(
                    &mut connection,
                    &mut receive_buffer,
                    &format!("NAME {name}"),
                    map_fingerprint,
                )?
            }
            ["state"] => receive(&mut connection, &mut receive_buffer, map_fingerprint)?,
            ["disconnect"] => { connection = None; println!("disconnected"); }
            ["fps", "show"] => println!("client console prototype; renderer not initialized"),
            ["quit"] | ["exit"] => break,
            [] => {},
            _ => println!("unknown command; type `help`"),
        }
    }
    drop(connection);
    if let Some(mut child) = local_server.take() {
        let endpoint = ServerEndpoint::read(&data_dir).ok();
        if let Some(endpoint) = endpoint {
            let _ = send_admin(&endpoint, "stop");
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            if child.try_wait()?.is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = child.kill();
    }
    Ok(())
}

fn sibling_binary(name: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let current = env::current_exe()?;
    let executable = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_owned()
    };
    let candidate = current.with_file_name(executable);
    if candidate.exists() {
        Ok(candidate)
    } else {
        Err(format!(
            "server executable not found at {}; build `minimium-server` first",
            candidate.display()
        )
        .into())
    }
}

fn wait_for_endpoint(
    data_dir: &std::path::Path,
) -> Result<ServerEndpoint, Box<dyn std::error::Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(endpoint) = ServerEndpoint::read(data_dir) {
            return Ok(endpoint);
        }
        if Instant::now() >= deadline {
            return Err("timed out waiting for local server".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn send_command(
    connection: &mut Option<TcpStream>,
    receive_buffer: &mut Vec<u8>,
    command: &str,
    map_fingerprint: u64,
) -> Result<(), Box<dyn std::error::Error>> {
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
        ["NAME", name] => ClientMessage::SetName((*name).to_owned()),
        ["QUIT"] => ClientMessage::Quit,
        _ => return Err("invalid client command".into()),
    };
    stream.write_all(&encode_client(&message)?)?;
    receive(connection, receive_buffer, map_fingerprint)
}

fn send_identity(
    stream: &mut TcpStream,
    profile_id: PlayerProfileId,
) -> Result<(), Box<dyn std::error::Error>> {
    stream.write_all(&encode_client(&ClientMessage::Identify(
        *profile_id.as_bytes(),
    ))?)?;
    Ok(())
}

fn receive(
    connection: &mut Option<TcpStream>,
    receive_buffer: &mut Vec<u8>,
    expected_map_fingerprint: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let Some(stream) = connection.as_mut() else {
        println!("not connected");
        return Ok(());
    };
    let mut buffer = [0u8; 8192];
    match stream.read(&mut buffer) {
        Ok(0) => {
            *connection = None;
            println!("server disconnected");
            return Ok(());
        }
        Ok(count) => receive_buffer.extend_from_slice(&buffer[..count]),
        Err(error)
            if error.kind() == io::ErrorKind::WouldBlock
                || error.kind() == io::ErrorKind::TimedOut => {}
        Err(error) => return Err(error.into()),
    }
    if receive_buffer.len() > 64 * 1024 {
        receive_buffer.clear();
        return Err("server frame exceeds protocol limits".into());
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
                    } => {
                        if map_fingerprint != expected_map_fingerprint {
                            println!(
                                "map mismatch: client={expected_map_fingerprint:016x} server={map_fingerprint:016x}"
                            );
                            *connection = None;
                            receive_buffer.clear();
                            return Ok(());
                        }
                        println!("server assigned player id {id}");
                    }
                    ServerMessage::Pong => println!("PONG"),
                    ServerMessage::Accepted => println!("ok"),
                    ServerMessage::Bye => println!("server closed the session"),
                    ServerMessage::Error(code) => println!("server error code {code}"),
                    ServerMessage::Snapshot {
                        self_position,
                        self_velocity,
                        self_grounded,
                        players,
                    } => println!(
                        "self=({:.2},{:.2},{:.2}) velocity=({:.2},{:.2},{:.2}) grounded={} nearby_players={}",
                        self_position[0],
                        self_position[1],
                        self_position[2],
                        self_velocity[0],
                        self_velocity[1],
                        self_velocity[2],
                        self_grounded,
                        players.len()
                    ),
                }
            }
            Ok(None) => break,
            Err(error) => {
                receive_buffer.clear();
                return Err(error.into());
            }
        }
    }
    if !received_message {
        println!("no server response yet");
    }
    Ok(())
}

fn send_admin(endpoint: &ServerEndpoint, command: &str) -> Result<(), Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(("127.0.0.1", endpoint.admin_port))?;
    writeln!(stream, "AUTH {}", endpoint.token)?;
    writeln!(stream, "{command}")?;
    Ok(())
}
mod console;
mod renderer;
