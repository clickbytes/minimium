use crate::metrics::RuntimeMetrics;
use minimium_core::identity::PlayerProfileId;
use minimium_core::ids::PlayerId;
use minimium_protocol::{encode_server_into, RemotePlayers, ServerMessage, MAX_SERVER_FRAME_BYTES};
use std::{
    collections::VecDeque,
    net::TcpStream,
    time::{Duration, Instant},
};

pub(crate) const MAX_QUEUED_OUTPUT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_QUEUED_OUTPUT_FRAMES: usize = 256;
pub(crate) const CLIENT_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) struct Client {
    pub(crate) player_id: Option<PlayerId>,
    pub(crate) profile_id: Option<PlayerProfileId>,
    pub(crate) stream: TcpStream,
    pub(crate) input: Vec<u8>,
    pub(crate) output: Vec<u8>,
    pub(crate) output_offset: usize,
    pub(crate) output_frames: VecDeque<(usize, Instant)>,
    pub(crate) bytes_in: u64,
    pub(crate) bytes_out: u64,
    pub(crate) frames_in: u64,
    pub(crate) frames_out: u64,
    pub(crate) encoding_us: u64,
    pub(crate) replicated_players: RemotePlayers,
    pub(crate) replication_initialized: bool,
    pub(crate) connected_at: Instant,
    pub(crate) last_message_at: Instant,
    pub(crate) peer_eof: bool,
    pub(crate) close_deadline: Option<Instant>,
    pub(crate) last_fire_at: Option<Instant>,
    pub(crate) last_chat_at: Option<Instant>,
    pub(crate) pending_health_update: Option<u16>,
}

pub(crate) fn queue_close_response(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
    now: Instant,
) -> bool {
    if !queue_message(client, metrics, message) {
        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
        return false;
    }
    client.close_deadline = Some(now + CLIENT_CLOSE_TIMEOUT);
    true
}

pub(crate) fn queue_message(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
) -> bool {
    if client.close_deadline.is_some() {
        return false;
    }
    if client.output_frames.len() >= MAX_QUEUED_OUTPUT_FRAMES {
        return false;
    }
    prepare_output(client);
    if client.output.len() + MAX_SERVER_FRAME_BYTES > MAX_QUEUED_OUTPUT_BYTES {
        return false;
    }
    client.output.reserve_exact(MAX_SERVER_FRAME_BYTES);
    let frame_start = client.output.len();
    let encoding_started = Instant::now();
    let result = encode_server_into(&mut client.output, &message);
    let encoding_time = encoding_started.elapsed();
    metrics.encoding.record(encoding_time);
    client.encoding_us = client
        .encoding_us
        .saturating_add(encoding_time.as_micros().min(u64::MAX as u128) as u64);
    match result {
        Ok(()) if client.output.len() <= MAX_QUEUED_OUTPUT_BYTES => {
            client
                .output_frames
                .push_back((client.output.len() - frame_start, Instant::now()));
            client.frames_out += 1;
            metrics.frames_out += 1;
            true
        }
        Ok(()) => {
            client.output.truncate(frame_start);
            false
        }
        Err(error) => {
            client.output.truncate(frame_start);
            eprintln!("could not encode game message: {error}");
            false
        }
    }
}

pub(crate) fn queue_encoded_frame(
    client: &mut Client,
    frame: &[u8],
    metrics: &mut RuntimeMetrics,
) -> bool {
    if client.close_deadline.is_some() {
        return false;
    }
    if client.output_frames.len() >= MAX_QUEUED_OUTPUT_FRAMES {
        return false;
    }
    prepare_output(client);
    if client.output.len().saturating_add(frame.len()) > MAX_QUEUED_OUTPUT_BYTES {
        return false;
    }
    client.output.reserve(frame.len());
    client.output.extend_from_slice(frame);
    client
        .output_frames
        .push_back((frame.len(), Instant::now()));
    client.frames_out += 1;
    metrics.frames_out += 1;
    true
}

pub(crate) fn prepare_output(client: &mut Client) {
    if client.output_offset > 0 {
        let pending = client.output.len() - client.output_offset;
        client.output.copy_within(client.output_offset.., 0);
        client.output.truncate(pending);
        client.output_offset = 0;
    }
}

pub(crate) fn account_output(client: &mut Client, mut written: usize) {
    while written > 0 {
        let Some((remaining, _)) = client.output_frames.front_mut() else {
            break;
        };
        let consumed = written.min(*remaining);
        *remaining -= consumed;
        written -= consumed;
        if *remaining == 0 {
            client.output_frames.pop_front();
        }
    }
}
