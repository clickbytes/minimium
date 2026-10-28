use super::{
    client_setup::{prepare_client, service_setup_clients, SetupConfig},
    scenarios::Scenario,
    stats::{ClientMeasurement, WorkerStats},
};
use minimium_protocol::{decode_server, ServerMessage, ERROR_RATE_LIMIT};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

pub(super) const PING_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const PING_TIMEOUT: Duration = Duration::from_secs(10);
pub(super) const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const MOVEMENT_INTERVAL: Duration = Duration::from_millis(50);
pub(super) const FIRE_INTERVAL: Duration = Duration::from_millis(300);
pub(super) const CHAT_INTERVAL: Duration = Duration::from_secs(25);
pub(super) const POLL_INTERVAL: Duration = Duration::from_millis(5);
pub(super) const MAX_RECEIVE_BUFFER_BYTES: usize = 64 * 1024;
pub(super) const MOVEMENT_DIRECTIONS: [[f32; 2]; 8] = [
    [1.0, 0.0],
    [0.707_106_77, 0.707_106_77],
    [0.0, 1.0],
    [-0.707_106_77, 0.707_106_77],
    [-1.0, 0.0],
    [-0.707_106_77, -0.707_106_77],
    [0.0, -1.0],
    [0.707_106_77, -0.707_106_77],
];
pub(super) struct LoadClient {
    pub(super) client_id: usize,
    pub(super) stream: TcpStream,
    pub(super) receive_buffer: Vec<u8>,
    pub(super) next_ping_at: Instant,
    pub(super) ping_due: bool,
    pub(super) next_movement_at: Instant,
    pub(super) pending_movement_index: Option<usize>,
    pub(super) pending_dispersed_movement: bool,
    pub(super) dispersed_movement_frame: Option<Vec<u8>>,
    pub(super) next_fire_at: Instant,
    pub(super) pending_fire_index: Option<usize>,
    pub(super) next_chat_at: Instant,
    pub(super) chat_due: bool,
    pub(super) pending_output: Option<PendingOutput>,
    pub(super) ping_sent_at: Option<Instant>,
    pub(super) measurement_bytes_sent: u64,
    pub(super) measurement_bytes_received: u64,
    pub(super) movement_seed: usize,
    pub(super) movement_sequence: u64,
    pub(super) disconnected: bool,
    pub(super) disconnected_before_start: bool,
    pub(super) next_setup_ping_at: Instant,
    pub(super) setup_ping_offset: Option<usize>,
}

#[derive(Clone, Copy)]
pub(super) struct PendingOutput {
    pub(super) kind: OutboundKind,
    pub(super) offset: usize,
}

pub(super) struct WorkerConfig {
    pub(super) socket: SocketAddr,
    pub(super) process_salt: [u8; 16],
    pub(super) seconds: u64,
    pub(super) scenario: Scenario,
    pub(super) total_clients: usize,
    pub(super) ping_frame: Vec<u8>,
    pub(super) movement_frames: Vec<Vec<u8>>,
    pub(super) fire_frames: Vec<Vec<u8>>,
    pub(super) chat_frame: Vec<u8>,
    pub(super) ready_sender: mpsc::Sender<()>,
    pub(super) start_receiver: mpsc::Receiver<Instant>,
    pub(super) abort_setup: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
pub(super) enum OutboundKind {
    Ping,
    Movement(usize),
    DispersedMovement,
    Fire(usize),
    Chat,
}

pub(super) fn run_worker(client_ids: Vec<usize>, config: WorkerConfig) -> WorkerStats {
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
        ready_sender,
        start_receiver,
        abort_setup,
    } = config;
    let mut stats = WorkerStats::default();
    let mut clients = Vec::with_capacity(client_ids.len());
    let setup_config = SetupConfig {
        socket,
        process_salt,
        scenario,
        total_clients,
        movement_frames: &movement_frames,
        ping_frame: &ping_frame,
    };
    for client_id in client_ids {
        if abort_setup.load(Ordering::Acquire) {
            return stats;
        }
        if let Some(client) = prepare_client(client_id, &setup_config, &mut clients, &mut stats) {
            clients.push(client);
        } else {
            stats.per_client.push(ClientMeasurement {
                generator_id: client_id,
                bytes_sent: 0,
                bytes_received: 0,
                disconnected: true,
            });
        }
    }

    stats.ready = clients.iter().filter(|client| !client.disconnected).count() as u64;
    if ready_sender.send(()).is_err() {
        return stats;
    }
    let start = loop {
        match start_receiver.recv_timeout(POLL_INTERVAL) {
            Ok(start) => break start,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                service_setup_clients(&mut clients, &ping_frame, None, &mut stats);
                if abort_setup.load(Ordering::Acquire) {
                    return stats;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return stats,
        }
    };
    let until = start + Duration::from_secs(seconds);
    while Instant::now() < start {
        service_setup_clients(&mut clients, &ping_frame, Some(start), &mut stats);
        thread::sleep(
            start
                .saturating_duration_since(Instant::now())
                .min(POLL_INTERVAL),
        );
    }
    for client in &mut clients {
        let client_id = client.client_id;
        client.next_ping_at = start;
        client.next_movement_at = start;
        client.next_fire_at = start;
        client.next_chat_at = start + Duration::from_millis(((client_id % 500) * 50) as u64);
    }
    let mut input = [0u8; 2048];
    while Instant::now() < until && !abort_setup.load(Ordering::Acquire) {
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
                if Instant::now() >= until {
                    continue;
                }
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
                        client.measurement_bytes_sent =
                            client.measurement_bytes_sent.saturating_add(written as u64);
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
                if Instant::now() >= until {
                    break;
                }
                match client.stream.read(&mut input) {
                    Ok(0) => {
                        client.disconnected = true;
                        stats.errors = stats.errors.saturating_add(1);
                        break;
                    }
                    Ok(count) => {
                        stats.bytes_received = stats.bytes_received.saturating_add(count as u64);
                        client.measurement_bytes_received = client
                            .measurement_bytes_received
                            .saturating_add(count as u64);
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
    stats.disconnects_during_measurement = clients
        .iter()
        .filter(|client| client.disconnected && !client.disconnected_before_start)
        .count() as u64;
    stats.connected_at_end = stats
        .ready
        .saturating_sub(stats.disconnects_during_measurement);
    stats.per_client.reserve(clients.len().min(10_000));
    stats
        .per_client
        .extend(clients.iter().map(|client| ClientMeasurement {
            generator_id: client.client_id,
            bytes_sent: client.measurement_bytes_sent,
            bytes_received: client.measurement_bytes_received,
            disconnected: client.disconnected,
        }));
    stats
}
