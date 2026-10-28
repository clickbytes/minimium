use super::{scenarios::Scenario, stats::WorkerStats, worker::*};
use minimium_core::identity::{PlayerProfileId, PLAYER_PROFILE_ID_BYTES};
use minimium_protocol::{decode_server, encode_client, ClientMessage, ServerMessage};
use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy)]
pub(super) enum SetupExpectation {
    Welcome,
    Accepted,
}

#[derive(Clone, Copy)]
pub(super) struct SetupConfig<'a> {
    pub(super) socket: SocketAddr,
    pub(super) process_salt: [u8; 16],
    pub(super) scenario: Scenario,
    pub(super) total_clients: usize,
    pub(super) movement_frames: &'a [Vec<u8>],
    pub(super) ping_frame: &'a [u8],
}

pub(super) fn prepare_client(
    client_id: usize,
    config: &SetupConfig<'_>,
    existing_clients: &mut [LoadClient],
    stats: &mut WorkerStats,
) -> Option<LoadClient> {
    let SetupConfig {
        socket,
        process_salt,
        scenario,
        total_clients,
        movement_frames,
        ping_frame,
    } = *config;
    let connect_deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    let mut stream = loop {
        let now = Instant::now();
        if now >= connect_deadline {
            stats.errors = stats.errors.saturating_add(1);
            stats.timeouts = stats.timeouts.saturating_add(1);
            stats.handshake_failures = stats.handshake_failures.saturating_add(1);
            return None;
        }
        service_setup_clients(existing_clients, ping_frame, None, stats);
        match TcpStream::connect_timeout(
            &socket,
            connect_deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(100)),
        ) {
            Ok(stream) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
            Err(_) => {
                stats.errors = stats.errors.saturating_add(1);
                stats.handshake_failures = stats.handshake_failures.saturating_add(1);
                return None;
            }
        }
    };
    if stream.set_nonblocking(true).is_err() || stream.set_nodelay(true).is_err() {
        stats.errors = stats.errors.saturating_add(1);
        stats.handshake_failures = stats.handshake_failures.saturating_add(1);
        return None;
    }

    let mut profile_id = [0u8; PLAYER_PROFILE_ID_BYTES];
    profile_id[..8].copy_from_slice(&(client_id as u64).to_le_bytes());
    profile_id[8..].copy_from_slice(&process_salt[..8]);
    let profile_id = PlayerProfileId::from_bytes(profile_id);
    let identity_frame = match encode_client(&ClientMessage::Identify(profile_id)) {
        Ok(frame) => frame,
        Err(_) => {
            stats.errors = stats.errors.saturating_add(1);
            stats.handshake_failures = stats.handshake_failures.saturating_add(1);
            return None;
        }
    };
    let mut receive_buffer = Vec::with_capacity(2048);
    let deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    if write_setup_frame(&mut stream, &identity_frame, deadline, stats).is_err()
        || wait_for_setup_message(
            &mut stream,
            &mut receive_buffer,
            deadline,
            SetupExpectation::Welcome,
            existing_clients,
            ping_frame,
            stats,
        )
        .is_err()
    {
        stats.errors = stats.errors.saturating_add(1);
        stats.handshake_failures = stats.handshake_failures.saturating_add(1);
        if Instant::now() >= deadline {
            stats.timeouts = stats.timeouts.saturating_add(1);
        }
        return None;
    }
    stats.connected = stats.connected.saturating_add(1);

    let name_frame = match encode_client(&ClientMessage::SetName(format!("Bot{client_id}"))) {
        Ok(frame) => frame,
        Err(_) => {
            stats.errors = stats.errors.saturating_add(1);
            stats.handshake_failures = stats.handshake_failures.saturating_add(1);
            return None;
        }
    };
    let name_deadline = Instant::now() + HANDSHAKE_TIMEOUT;
    if write_setup_frame(&mut stream, &name_frame, name_deadline, stats).is_err()
        || wait_for_setup_message(
            &mut stream,
            &mut receive_buffer,
            name_deadline,
            SetupExpectation::Accepted,
            existing_clients,
            ping_frame,
            stats,
        )
        .is_err()
    {
        stats.errors = stats.errors.saturating_add(1);
        stats.handshake_failures = stats.handshake_failures.saturating_add(1);
        if Instant::now() >= name_deadline {
            stats.timeouts = stats.timeouts.saturating_add(1);
        }
        return None;
    }

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
                return None;
            }
        }
    } else {
        None
    };

    Some(LoadClient {
        client_id,
        stream,
        receive_buffer,
        next_ping_at: Instant::now(),
        ping_due: false,
        next_movement_at: Instant::now(),
        pending_movement_index: None,
        pending_dispersed_movement: false,
        dispersed_movement_frame,
        next_fire_at: Instant::now(),
        pending_fire_index: None,
        next_chat_at: Instant::now(),
        chat_due: false,
        pending_output: None,
        ping_sent_at: None,
        measurement_bytes_sent: 0,
        measurement_bytes_received: 0,
        movement_seed: client_id % movement_frames.len(),
        movement_sequence: 0,
        disconnected: false,
        disconnected_before_start: false,
        next_setup_ping_at: Instant::now() + PING_INTERVAL,
        setup_ping_offset: None,
    })
}

pub(super) fn write_setup_frame(
    stream: &mut TcpStream,
    frame: &[u8],
    deadline: Instant,
    stats: &mut WorkerStats,
) -> std::io::Result<()> {
    let mut offset = 0;
    while offset < frame.len() {
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "setup write timed out",
            ));
        }
        match stream.write(&frame[offset..]) {
            Ok(0) => return Err(std::io::ErrorKind::WriteZero.into()),
            Ok(count) => {
                offset += count;
                stats.bytes_sent = stats.bytes_sent.saturating_add(count as u64);
                stats.setup_bytes_sent = stats.setup_bytes_sent.saturating_add(count as u64);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(
                    POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

pub(super) fn wait_for_setup_message(
    stream: &mut TcpStream,
    receive_buffer: &mut Vec<u8>,
    deadline: Instant,
    expected: SetupExpectation,
    existing_clients: &mut [LoadClient],
    ping_frame: &[u8],
    stats: &mut WorkerStats,
) -> std::io::Result<()> {
    let mut chunk = [0u8; 4096];
    loop {
        service_setup_clients(existing_clients, ping_frame, None, stats);
        loop {
            match decode_server(receive_buffer) {
                Ok(Some((message, frame_bytes))) => {
                    receive_buffer.drain(..frame_bytes);
                    match (expected, message) {
                        (SetupExpectation::Welcome, ServerMessage::Welcome { .. })
                        | (SetupExpectation::Accepted, ServerMessage::Accepted) => return Ok(()),
                        (_, ServerMessage::Error(_)) => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::ConnectionRefused,
                                "server rejected load client setup",
                            ));
                        }
                        (SetupExpectation::Welcome, _) => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "server sent data before Welcome",
                            ));
                        }
                        (SetupExpectation::Accepted, ServerMessage::Welcome { .. }) => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "server sent duplicate Welcome",
                            ));
                        }
                        (SetupExpectation::Accepted, _) => {}
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, error));
                }
            }
        }
        if receive_buffer.len() > MAX_RECEIVE_BUFFER_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "setup response exceeds receive buffer limit",
            ));
        }
        if Instant::now() >= deadline {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "setup response timed out",
            ));
        }
        match stream.read(&mut chunk) {
            Ok(0) => return Err(std::io::ErrorKind::UnexpectedEof.into()),
            Ok(count) => {
                stats.bytes_received = stats.bytes_received.saturating_add(count as u64);
                stats.setup_bytes_received =
                    stats.setup_bytes_received.saturating_add(count as u64);
                receive_buffer.extend_from_slice(&chunk[..count]);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(
                    POLL_INTERVAL.min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Err(error) => return Err(error),
        }
    }
}

pub(super) fn service_setup_clients(
    clients: &mut [LoadClient],
    ping_frame: &[u8],
    deadline: Option<Instant>,
    stats: &mut WorkerStats,
) {
    let now = Instant::now();
    for client in clients.iter_mut().filter(|client| !client.disconnected) {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            break;
        }
        if client.setup_ping_offset.is_none() && now >= client.next_setup_ping_at {
            client.setup_ping_offset = Some(0);
            client.next_setup_ping_at = now + PING_INTERVAL;
        }
        if let Some(offset) = client.setup_ping_offset {
            match client.stream.write(&ping_frame[offset..]) {
                Ok(0) => {
                    client.disconnected = true;
                    client.disconnected_before_start = true;
                    stats.errors = stats.errors.saturating_add(1);
                    stats.disconnects_before_start =
                        stats.disconnects_before_start.saturating_add(1);
                    stats.ready = stats.ready.saturating_sub(1);
                    continue;
                }
                Ok(count) => {
                    stats.bytes_sent = stats.bytes_sent.saturating_add(count as u64);
                    stats.setup_bytes_sent = stats.setup_bytes_sent.saturating_add(count as u64);
                    let next_offset = offset + count;
                    client.setup_ping_offset =
                        (next_offset < ping_frame.len()).then_some(next_offset);
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => {
                    client.disconnected = true;
                    client.disconnected_before_start = true;
                    stats.errors = stats.errors.saturating_add(1);
                    stats.disconnects_before_start =
                        stats.disconnects_before_start.saturating_add(1);
                    stats.ready = stats.ready.saturating_sub(1);
                    continue;
                }
            }
        }
        drain_setup_traffic_until(client, stats, deadline);
    }
}

pub(super) fn drain_setup_traffic_until(
    client: &mut LoadClient,
    stats: &mut WorkerStats,
    deadline: Option<Instant>,
) {
    let mut chunk = [0u8; 4096];
    loop {
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return;
        }
        match client.stream.read(&mut chunk) {
            Ok(0) => {
                client.disconnected = true;
                client.disconnected_before_start = true;
                stats.errors = stats.errors.saturating_add(1);
                stats.disconnects_before_start = stats.disconnects_before_start.saturating_add(1);
                stats.ready = stats.ready.saturating_sub(1);
                return;
            }
            Ok(count) => {
                stats.bytes_received = stats.bytes_received.saturating_add(count as u64);
                stats.setup_bytes_received =
                    stats.setup_bytes_received.saturating_add(count as u64);
                client.receive_buffer.extend_from_slice(&chunk[..count]);
                if client.receive_buffer.len() > MAX_RECEIVE_BUFFER_BYTES {
                    client.disconnected = true;
                    client.disconnected_before_start = true;
                    stats.errors = stats.errors.saturating_add(1);
                    stats.disconnects_before_start =
                        stats.disconnects_before_start.saturating_add(1);
                    stats.ready = stats.ready.saturating_sub(1);
                    return;
                }
                loop {
                    match decode_server(&client.receive_buffer) {
                        Ok(Some((message, frame_bytes))) => {
                            client.receive_buffer.drain(..frame_bytes);
                            if matches!(message, ServerMessage::Error(_)) {
                                client.disconnected = true;
                                client.disconnected_before_start = true;
                                stats.errors = stats.errors.saturating_add(1);
                                stats.disconnects_before_start =
                                    stats.disconnects_before_start.saturating_add(1);
                                stats.ready = stats.ready.saturating_sub(1);
                                return;
                            }
                        }
                        Ok(None) => break,
                        Err(_) => {
                            client.disconnected = true;
                            client.disconnected_before_start = true;
                            stats.errors = stats.errors.saturating_add(1);
                            stats.disconnects_before_start =
                                stats.disconnects_before_start.saturating_add(1);
                            stats.ready = stats.ready.saturating_sub(1);
                            return;
                        }
                    }
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return,
            Err(_) => {
                client.disconnected = true;
                client.disconnected_before_start = true;
                stats.errors = stats.errors.saturating_add(1);
                stats.disconnects_before_start = stats.disconnects_before_start.saturating_add(1);
                stats.ready = stats.ready.saturating_sub(1);
                return;
            }
        }
    }
}
