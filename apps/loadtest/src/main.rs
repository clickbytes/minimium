mod report;
use minimium_protocol::{
    decode_server, encode_client, ClientMessage, ServerMessage, ERROR_RATE_LIMIT,
};
use std::{
    env,
    io::{Read, Write},
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    sync::{Arc, Barrier},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const PING_INTERVAL: Duration = Duration::from_secs(1);
const PING_TIMEOUT: Duration = Duration::from_secs(10);
const MOVEMENT_INTERVAL: Duration = Duration::from_millis(50);
const FIRE_INTERVAL: Duration = Duration::from_millis(300);
const CHAT_INTERVAL: Duration = Duration::from_secs(25);
const POLL_INTERVAL: Duration = Duration::from_millis(5);
const MAX_RECEIVE_BUFFER_BYTES: usize = 64 * 1024;
const MOVEMENT_DIRECTIONS: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [0.707_106_77, 0.707_106_77],
    [0.0, 1.0],
    [-0.707_106_77, 0.707_106_77],
    [-1.0, 0.0],
    [-0.707_106_77, -0.707_106_77],
    [0.0, -1.0],
    [0.707_106_77, -0.707_106_77],
];
const RTT_BUCKETS_MICROS: [u64; 16] = [
    50, 100, 250, 500, 1_000, 2_000, 5_000, 10_000, 20_000, 50_000, 100_000, 250_000, 500_000,
    1_000_000, 2_500_000, 5_000_000,
];

struct LoadClient {
    stream: TcpStream,
    receive_buffer: Vec<u8>,
    next_ping_at: Instant,
    ping_due: bool,
    next_movement_at: Instant,
    pending_movement_index: Option<usize>,
    pending_dispersed_movement: bool,
    dispersed_movement_frame: Option<Vec<u8>>,
    next_fire_at: Instant,
    pending_fire_index: Option<usize>,
    next_chat_at: Instant,
    chat_due: bool,
    pending_output: Option<PendingOutput>,
    ping_sent_at: Option<Instant>,
    movement_seed: usize,
    movement_sequence: u64,
    disconnected: bool,
}

#[derive(Clone, Copy)]
struct PendingOutput {
    kind: OutboundKind,
    offset: usize,
}

struct WorkerConfig {
    socket: SocketAddr,
    process_salt: [u8; 16],
    seconds: u64,
    scenario: Scenario,
    total_clients: usize,
    ping_frame: Vec<u8>,
    movement_frames: Vec<Vec<u8>>,
    fire_frames: Vec<Vec<u8>>,
    chat_frame: Vec<u8>,
    barrier: Arc<Barrier>,
}

#[derive(Clone, Copy)]
enum OutboundKind {
    Ping,
    Movement(usize),
    DispersedMovement,
    Fire(usize),
    Chat,
}

#[derive(Clone, Copy)]
enum Scenario {
    Heartbeat,
    Movement,
    Combat,
    Chat,
    Clustered,
    Dispersed,
}

impl Scenario {
    fn name(self) -> &'static str {
        match self {
            Self::Heartbeat => "heartbeat",
            Self::Movement => "movement",
            Self::Combat => "combat",
            Self::Chat => "chat",
            Self::Clustered => "clustered",
            Self::Dispersed => "dispersed",
        }
    }

    fn includes_movement(self) -> bool {
        matches!(
            self,
            Self::Movement | Self::Combat | Self::Clustered | Self::Dispersed
        )
    }

    fn includes_combat(self) -> bool {
        matches!(self, Self::Combat)
    }

    fn includes_chat(self) -> bool {
        matches!(self, Self::Chat)
    }
}

#[derive(Default)]
struct WorkerStats {
    connected: u64,
    pongs: u64,
    errors: u64,
    timeouts: u64,
    moves_sent: u64,
    shots_sent: u64,
    shots_hit: u64,
    kills: u64,
    chat_sent: u64,
    chat_received: u64,
    rate_limited: u64,
    snapshots_received: u64,
    bytes_sent: u64,
    bytes_received: u64,
    rtt: RttStats,
}

#[derive(Default)]
struct RttStats {
    samples: u64,
    total_microseconds: u128,
    minimum_microseconds: Option<u64>,
    maximum_microseconds: u64,
    buckets: [u64; RTT_BUCKETS_MICROS.len() + 1],
}

impl RttStats {
    fn record(&mut self, duration: Duration) {
        let microseconds = duration.as_micros().min(u128::from(u64::MAX)) as u64;
        let bucket = RTT_BUCKETS_MICROS
            .iter()
            .position(|limit| microseconds <= *limit)
            .unwrap_or(RTT_BUCKETS_MICROS.len());
        self.samples = self.samples.saturating_add(1);
        self.total_microseconds = self
            .total_microseconds
            .saturating_add(u128::from(microseconds));
        self.minimum_microseconds = Some(
            self.minimum_microseconds
                .unwrap_or(microseconds)
                .min(microseconds),
        );
        self.maximum_microseconds = self.maximum_microseconds.max(microseconds);
        self.buckets[bucket] = self.buckets[bucket].saturating_add(1);
    }

    fn merge(&mut self, other: Self) {
        self.samples = self.samples.saturating_add(other.samples);
        self.total_microseconds = self
            .total_microseconds
            .saturating_add(other.total_microseconds);
        if let Some(minimum) = other.minimum_microseconds {
            self.minimum_microseconds =
                Some(self.minimum_microseconds.unwrap_or(minimum).min(minimum));
        }
        self.maximum_microseconds = self.maximum_microseconds.max(other.maximum_microseconds);
        for (total, count) in self.buckets.iter_mut().zip(other.buckets) {
            *total = total.saturating_add(count);
        }
    }

    fn average_milliseconds(&self) -> Option<f64> {
        (self.samples > 0).then(|| self.total_microseconds as f64 / self.samples as f64 / 1_000.0)
    }

    fn percentile_milliseconds(&self, percentile: u64) -> Option<f64> {
        if self.samples == 0 {
            return None;
        }
        let rank = self.samples.saturating_mul(percentile).saturating_add(99) / 100;
        let mut cumulative = 0u64;
        for (index, count) in self.buckets.iter().enumerate() {
            cumulative = cumulative.saturating_add(*count);
            if cumulative >= rank {
                let upper_bound = RTT_BUCKETS_MICROS
                    .get(index)
                    .copied()
                    .unwrap_or(self.maximum_microseconds);
                return Some(upper_bound as f64 / 1_000.0);
            }
        }
        Some(self.maximum_microseconds as f64 / 1_000.0)
    }

    fn minimum_milliseconds(&self) -> Option<f64> {
        self.minimum_microseconds
            .map(|microseconds| microseconds as f64 / 1_000.0)
    }

    fn maximum_milliseconds(&self) -> Option<f64> {
        (self.samples > 0).then(|| self.maximum_microseconds as f64 / 1_000.0)
    }
}

impl WorkerStats {
    fn merge(&mut self, other: Self) {
        self.connected = self.connected.saturating_add(other.connected);
        self.pongs = self.pongs.saturating_add(other.pongs);
        self.errors = self.errors.saturating_add(other.errors);
        self.timeouts = self.timeouts.saturating_add(other.timeouts);
        self.moves_sent = self.moves_sent.saturating_add(other.moves_sent);
        self.shots_sent = self.shots_sent.saturating_add(other.shots_sent);
        self.shots_hit = self.shots_hit.saturating_add(other.shots_hit);
        self.kills = self.kills.saturating_add(other.kills);
        self.chat_sent = self.chat_sent.saturating_add(other.chat_sent);
        self.chat_received = self.chat_received.saturating_add(other.chat_received);
        self.rate_limited = self.rate_limited.saturating_add(other.rate_limited);
        self.snapshots_received = self
            .snapshots_received
            .saturating_add(other.snapshots_received);
        self.bytes_sent = self.bytes_sent.saturating_add(other.bytes_sent);
        self.bytes_received = self.bytes_received.saturating_add(other.bytes_received);
        self.rtt.merge(other.rtt);
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("loadtest: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut address = None;
    let mut clients = 100usize;
    let mut seconds = 10u64;
    let mut thread_mode = "auto".to_owned();
    let mut output_format = "text".to_owned();
    let mut scenario_name = "heartbeat".to_owned();
    let mut cpu_label = None;
    let mut cpu_mhz = None;
    let mut scenario_id = None;
    let mut server_data_dir = None;
    let mut seen = std::collections::HashSet::new();
    let args: Vec<String> = env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        if !seen.insert(args[index].clone()) {
            return Err(format!("duplicate option: {}", args[index]).into());
        }
        match args[index].as_str() {
            "--cpu-label" | "--scenario-id" | "--server-data-dir" | "--cpu-mhz" => {
                let option = &args[index];
                index += 1;
                let value = args
                    .get(index)
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| format!("{option} requires a value"))?;
                if value.len() > 512 {
                    return Err(format!("{option} exceeds 512 bytes").into());
                }
                match option.as_str() {
                    "--cpu-label" => cpu_label = Some(value.clone()),
                    "--scenario-id" => scenario_id = Some(value.clone()),
                    "--server-data-dir" => server_data_dir = Some(std::path::PathBuf::from(value)),
                    _ => {
                        let mhz: u32 = value.parse()?;
                        if mhz == 0 {
                            return Err("--cpu-mhz must be positive".into());
                        }
                        cpu_mhz = Some(mhz);
                    }
                }
            }
            "--address" => {
                index += 1;
                address = Some(
                    args.get(index)
                        .ok_or("--address requires HOST:PORT")?
                        .clone(),
                );
            }
            "--clients" => {
                index += 1;
                clients = args
                    .get(index)
                    .ok_or("--clients requires a number")?
                    .parse()?;
            }
            "--seconds" => {
                index += 1;
                seconds = args
                    .get(index)
                    .ok_or("--seconds requires a number")?
                    .parse()?;
            }
            "--threads" => {
                index += 1;
                thread_mode = args
                    .get(index)
                    .ok_or("--threads requires 1, 2, or auto")?
                    .clone();
            }
            "--format" => {
                index += 1;
                output_format = args
                    .get(index)
                    .ok_or("--format requires text or json")?
                    .clone();
            }
            "--scenario" => {
                index += 1;
                scenario_name = args
                    .get(index)
                    .ok_or("--scenario requires heartbeat, movement, combat, chat, clustered, or dispersed")?
                    .clone();
            }
            "--help" | "-h" => {
                println!("minimium-loadtest --address HOST:PORT [--scenario heartbeat|movement|combat|chat|clustered|dispersed] [--clients N] [--seconds N] [--threads 1|2|auto] [--format text|json] [--scenario-id TEXT] [--cpu-label TEXT] [--cpu-mhz N] [--server-data-dir PATH]");
                return Ok(());
            }
            other => return Err(format!("unknown option: {other}").into()),
        }
        index += 1;
    }
    let address = address.ok_or("--address is required")?;
    if clients == 0 || clients > 10_000 {
        return Err("clients must be between 1 and 10000".into());
    }
    if !matches!(output_format.as_str(), "text" | "json") {
        return Err("format must be text or json".into());
    }
    let scenario = match scenario_name.as_str() {
        "heartbeat" => Scenario::Heartbeat,
        "movement" => Scenario::Movement,
        "combat" => Scenario::Combat,
        "chat" => Scenario::Chat,
        "clustered" => Scenario::Clustered,
        "dispersed" => Scenario::Dispersed,
        _ => {
            return Err(
                "scenario must be heartbeat, movement, combat, chat, clustered, or dispersed"
                    .into(),
            )
        }
    };
    let worker_limit = match thread_mode.as_str() {
        "1" => 1,
        "2" => 2,
        "auto" => thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1)
            .min(2),
        _ => return Err("threads must be 1, 2, or auto".into()),
    };
    if !(1..=86400).contains(&seconds) {
        return Err("seconds must be between 1 and 86400".into());
    }
    let worker_count = worker_limit.min(clients);
    let mut metadata =
        report::ReportMetadata::new(scenario_id.unwrap_or_else(|| scenario_name.clone()));
    metadata.cpu_label = cpu_label;
    metadata.cpu_mhz = cpu_mhz;
    metadata.requested_seconds = seconds;
    metadata.address = address.clone();
    metadata.thread_mode = thread_mode.clone();
    if let Some(path) = &server_data_dir {
        match report::capture_server(path) {
            Ok(sample) => metadata.server_before = Some(sample),
            Err(error) => eprintln!("server sample before unavailable: {error}"),
        }
    }
    let socket = address.to_socket_addrs()?.next().ok_or("invalid address")?;
    let process_salt = SystemTime::now()
        .duration_since(UNIX_EPOCH)?
        .as_nanos()
        .to_le_bytes();
    let ping_frame = encode_client(&ClientMessage::Ping)?;
    let movement_frames = MOVEMENT_DIRECTIONS
        .map(|direction| {
            encode_client(&ClientMessage::Move {
                direction,
                jump: false,
            })
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let fire_frames = MOVEMENT_DIRECTIONS
        .map(|direction| {
            encode_client(&ClientMessage::Fire {
                direction: [direction[0], 0.0, direction[1]],
            })
        })
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    let chat_frame = encode_client(&ClientMessage::Chat(String::from("load test message")))?;
    let barrier = Arc::new(Barrier::new(worker_count + 1));
    let mut workers = Vec::with_capacity(worker_count);

    for worker_id in 0..worker_count {
        let client_ids = (worker_id..clients).step_by(worker_count).collect();
        let barrier = Arc::clone(&barrier);
        let ping_frame = ping_frame.clone();
        let movement_frames = movement_frames.clone();
        let fire_frames = fire_frames.clone();
        let chat_frame = chat_frame.clone();
        workers.push(thread::spawn(move || {
            run_worker(
                client_ids,
                WorkerConfig {
                    socket,
                    process_salt,
                    seconds,
                    scenario,
                    total_clients: clients,
                    ping_frame,
                    movement_frames,
                    fire_frames,
                    chat_frame,
                    barrier,
                },
            )
        }));
    }

    barrier.wait();
    let start = Instant::now();
    let mut stats = WorkerStats::default();
    for worker in workers {
        match worker.join() {
            Ok(worker_stats) => stats.merge(worker_stats),
            Err(_) => stats.errors = stats.errors.saturating_add(1),
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    if let Some(path) = &server_data_dir {
        match report::capture_server(path) {
            Ok(sample) => metadata.server_after = Some(sample),
            Err(error) => eprintln!("server sample after unavailable: {error}"),
        }
    }
    report::print_report(
        &output_format,
        scenario.name(),
        clients,
        worker_count,
        elapsed,
        &stats,
        &metadata,
    );
    Ok(())
}

fn run_worker(client_ids: Vec<usize>, config: WorkerConfig) -> WorkerStats {
    let WorkerConfig {
        socket,
        process_salt,
        seconds,
        scenario,
        total_clients,
        ping_frame,
        movement_frames,
        fire_frames,
        chat_frame,
        barrier,
    } = config;
    let mut stats = WorkerStats::default();
    let mut clients = Vec::with_capacity(client_ids.len());
    for client_id in client_ids {
        let Ok(mut stream) = TcpStream::connect_timeout(&socket, Duration::from_secs(5)) else {
            stats.errors = stats.errors.saturating_add(1);
            continue;
        };
        let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
        let _ = stream.set_nodelay(true);

        let mut profile_id = [0u8; 16];
        profile_id[..8].copy_from_slice(&(client_id as u64).to_le_bytes());
        profile_id[8..].copy_from_slice(&process_salt[..8]);
        let identity_frame = encode_client(&ClientMessage::Identify(profile_id));
        let name_frame = encode_client(&ClientMessage::SetName(format!("Bot{client_id}")));
        let (Ok(identity_frame), Ok(name_frame)) = (identity_frame, name_frame) else {
            stats.errors = stats.errors.saturating_add(1);
            continue;
        };
        let dispersed_movement_frame = if matches!(scenario, Scenario::Dispersed) {
            let angle = std::f64::consts::TAU * client_id as f64 / total_clients as f64;
            let direction = [angle.cos() as f32, angle.sin() as f32];
            match encode_client(&ClientMessage::Move {
                direction,
                jump: false,
            }) {
                Ok(frame) => Some(frame),
                Err(_) => {
                    stats.errors = stats.errors.saturating_add(1);
                    continue;
                }
            }
        } else {
            None
        };
        if stream.write_all(&identity_frame).is_err() {
            stats.errors = stats.errors.saturating_add(1);
            continue;
        }
        stats.bytes_sent = stats.bytes_sent.saturating_add(identity_frame.len() as u64);
        if stream.write_all(&name_frame).is_err() {
            stats.errors = stats.errors.saturating_add(1);
            continue;
        }
        stats.bytes_sent = stats.bytes_sent.saturating_add(name_frame.len() as u64);
        if stream.set_nonblocking(true).is_err() {
            stats.errors = stats.errors.saturating_add(1);
            continue;
        }
        stats.connected = stats.connected.saturating_add(1);
        clients.push(LoadClient {
            stream,
            receive_buffer: Vec::with_capacity(2048),
            next_ping_at: Instant::now(),
            ping_due: false,
            next_movement_at: Instant::now(),
            pending_movement_index: None,
            pending_dispersed_movement: false,
            dispersed_movement_frame,
            next_fire_at: Instant::now(),
            pending_fire_index: None,
            next_chat_at: Instant::now() + Duration::from_millis(((client_id % 500) * 50) as u64),
            chat_due: false,
            pending_output: None,
            ping_sent_at: None,
            movement_seed: client_id % movement_frames.len(),
            movement_sequence: 0,
            disconnected: false,
        });
    }

    barrier.wait();
    let until = Instant::now() + Duration::from_secs(seconds);
    let mut input = [0u8; 2048];
    while Instant::now() < until {
        let now = Instant::now();
        for client in &mut clients {
            if client.disconnected {
                continue;
            }
            if now >= client.next_ping_at {
                client.ping_due = true;
                client.next_ping_at = now + PING_INTERVAL;
            }
            if scenario.includes_movement() && now >= client.next_movement_at {
                let direction_index = match scenario {
                    Scenario::Clustered => {
                        let outbound = (client.movement_sequence / 10) % 2 == 0;
                        Some(
                            (client.movement_seed + if outbound { 0 } else { 4 })
                                % movement_frames.len(),
                        )
                    }
                    Scenario::Dispersed => {
                        client.pending_dispersed_movement = true;
                        None
                    }
                    _ => Some(
                        (client.movement_seed + (client.movement_sequence / 60) as usize)
                            % movement_frames.len(),
                    ),
                };
                if let Some(direction_index) = direction_index {
                    client.pending_movement_index = Some(direction_index);
                }
                client.movement_sequence = client.movement_sequence.saturating_add(1);
                client.next_movement_at = now + MOVEMENT_INTERVAL;
            }
            if scenario.includes_combat() && now >= client.next_fire_at {
                let direction_index = (client.movement_seed
                    + (client.movement_sequence / 60) as usize)
                    % fire_frames.len();
                client.pending_fire_index = Some(direction_index);
                client.next_fire_at = now + FIRE_INTERVAL;
            }
            if scenario.includes_chat() && now >= client.next_chat_at {
                client.chat_due = true;
                client.next_chat_at = now + CHAT_INTERVAL;
            }
            if client.pending_output.is_none() {
                client.pending_output = if client.ping_due && client.ping_sent_at.is_none() {
                    Some(PendingOutput {
                        kind: OutboundKind::Ping,
                        offset: 0,
                    })
                } else if client.chat_due {
                    Some(PendingOutput {
                        kind: OutboundKind::Chat,
                        offset: 0,
                    })
                } else if let Some(index) = client.pending_fire_index.take() {
                    Some(PendingOutput {
                        kind: OutboundKind::Fire(index),
                        offset: 0,
                    })
                } else if client.pending_dispersed_movement {
                    client.pending_dispersed_movement = false;
                    Some(PendingOutput {
                        kind: OutboundKind::DispersedMovement,
                        offset: 0,
                    })
                } else {
                    client
                        .pending_movement_index
                        .take()
                        .map(|index| PendingOutput {
                            kind: OutboundKind::Movement(index),
                            offset: 0,
                        })
                };
            }
            if let Some(pending) = client.pending_output {
                let frame = match pending.kind {
                    OutboundKind::Ping => &ping_frame,
                    OutboundKind::Movement(index) => &movement_frames[index],
                    OutboundKind::DispersedMovement => client
                        .dispersed_movement_frame
                        .as_ref()
                        .expect("dispersed scenario prepared each client movement frame"),
                    OutboundKind::Fire(index) => &fire_frames[index],
                    OutboundKind::Chat => &chat_frame,
                };
                match client.stream.write(&frame[pending.offset..]) {
                    Ok(0) => {
                        client.disconnected = true;
                        stats.errors = stats.errors.saturating_add(1);
                        continue;
                    }
                    Ok(written) => {
                        stats.bytes_sent = stats.bytes_sent.saturating_add(written as u64);
                        let next_offset = pending.offset + written;
                        if next_offset < frame.len() {
                            client.pending_output = Some(PendingOutput {
                                offset: next_offset,
                                ..pending
                            });
                        } else {
                            client.pending_output = None;
                            match pending.kind {
                                OutboundKind::Ping => {
                                    client.ping_due = false;
                                    client.ping_sent_at = Some(Instant::now());
                                }
                                OutboundKind::Movement(_) => {
                                    stats.moves_sent = stats.moves_sent.saturating_add(1);
                                }
                                OutboundKind::DispersedMovement => {
                                    stats.moves_sent = stats.moves_sent.saturating_add(1);
                                }
                                OutboundKind::Fire(_) => {
                                    stats.shots_sent = stats.shots_sent.saturating_add(1);
                                }
                                OutboundKind::Chat => {
                                    client.chat_due = false;
                                    stats.chat_sent = stats.chat_sent.saturating_add(1);
                                }
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        client.disconnected = true;
                        stats.errors = stats.errors.saturating_add(1);
                        continue;
                    }
                }
            }

            loop {
                match client.stream.read(&mut input) {
                    Ok(0) => {
                        client.disconnected = true;
                        stats.errors = stats.errors.saturating_add(1);
                        break;
                    }
                    Ok(count) => {
                        stats.bytes_received = stats.bytes_received.saturating_add(count as u64);
                        client.receive_buffer.extend_from_slice(&input[..count]);
                        if client.receive_buffer.len() > MAX_RECEIVE_BUFFER_BYTES {
                            client.disconnected = true;
                            stats.errors = stats.errors.saturating_add(1);
                            break;
                        }
                        loop {
                            match decode_server(&client.receive_buffer) {
                                Ok(Some((message, frame_bytes))) => {
                                    client.receive_buffer.drain(..frame_bytes);
                                    match message {
                                        ServerMessage::Pong => {
                                            stats.pongs = stats.pongs.saturating_add(1);
                                            if let Some(sent_at) = client.ping_sent_at.take() {
                                                stats.rtt.record(
                                                    Instant::now()
                                                        .saturating_duration_since(sent_at),
                                                );
                                            }
                                        }
                                        ServerMessage::Error(ERROR_RATE_LIMIT) => {
                                            stats.rate_limited =
                                                stats.rate_limited.saturating_add(1);
                                        }
                                        ServerMessage::Error(_) | ServerMessage::Bye => {
                                            client.disconnected = true;
                                            stats.errors = stats.errors.saturating_add(1);
                                        }
                                        ServerMessage::Snapshot { .. } => {
                                            stats.snapshots_received =
                                                stats.snapshots_received.saturating_add(1);
                                        }
                                        ServerMessage::ShotResult {
                                            target_id, killed, ..
                                        } => {
                                            if target_id.is_some() {
                                                stats.shots_hit = stats.shots_hit.saturating_add(1);
                                            }
                                            if killed {
                                                stats.kills = stats.kills.saturating_add(1);
                                            }
                                        }
                                        ServerMessage::Chat { .. } => {
                                            stats.chat_received =
                                                stats.chat_received.saturating_add(1);
                                        }
                                        _ => {}
                                    }
                                }
                                Ok(None) => break,
                                Err(_) => {
                                    client.disconnected = true;
                                    stats.errors = stats.errors.saturating_add(1);
                                    break;
                                }
                            }
                        }
                        if client.disconnected {
                            break;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        client.disconnected = true;
                        stats.errors = stats.errors.saturating_add(1);
                        break;
                    }
                }
            }
            if !client.disconnected
                && client.ping_sent_at.is_some_and(|sent_at| {
                    Instant::now().saturating_duration_since(sent_at) >= PING_TIMEOUT
                })
            {
                client.disconnected = true;
                stats.timeouts = stats.timeouts.saturating_add(1);
            }
        }
        thread::sleep(POLL_INTERVAL);
    }
    let finished_at = Instant::now();
    for client in &mut clients {
        if !client.disconnected
            && client.ping_sent_at.is_some_and(|sent_at| {
                finished_at.saturating_duration_since(sent_at) >= PING_TIMEOUT
            })
        {
            stats.timeouts = stats.timeouts.saturating_add(1);
        }
    }
    stats
}
