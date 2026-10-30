use crate::{metrics::RuntimeMetrics, network};
use minimium_core::identity::PlayerProfileId;
use minimium_core::ids::PlayerId;
use minimium_core::time::ClockRevision;
use minimium_protocol::WorldContext;
use minimium_protocol::{encode_server_into, RemotePlayers, ServerMessage, MAX_SERVER_FRAME_BYTES};
use std::{
    collections::VecDeque,
    net::TcpStream,
    time::{Duration, Instant},
};

pub(crate) const MAX_QUEUED_OUTPUT_BYTES: usize = 64 * 1024;
pub(crate) const MAX_QUEUED_OUTPUT_FRAMES: usize = 256;
pub(crate) const CLIENT_CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const TIME_PROBE_WINDOW: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug)]
pub(crate) enum CloseReason {
    Quit,
    Rejected,
    PeerEof,
    HandshakeTimeout,
    IdleTimeout,
    InvalidFrame,
    ReadError,
    WriteError,
    Backpressure,
    InvalidSequence,
}

#[derive(Clone, Copy)]
pub(crate) struct Identity {
    pub(crate) player: PlayerId,
    pub(crate) profile: PlayerProfileId,
}

#[derive(Clone, Copy)]
pub(crate) enum ConnectionPhase {
    AwaitIdentify,
    Active(Identity),
    Draining {
        identity: Option<Identity>,
        deadline: Instant,
        reason: CloseReason,
    },
}

pub(crate) struct Client {
    phase: ConnectionPhase,
    pub(crate) disconnect_reason: Option<CloseReason>,
    pub(crate) stream: TcpStream,
    pub(crate) input: Vec<u8>,
    output: Vec<u8>,
    output_offset: usize,
    output_frames: VecDeque<(usize, Instant)>,
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
    pub(crate) last_fire_at: Option<Instant>,
    pub(crate) last_chat_at: Option<Instant>,
    pub(crate) pending_health_update: Option<u16>,
    time_probe_received_at: [Option<Instant>; 2],
    pub(crate) last_environment_sync_at: Option<Instant>,
    pub(crate) last_environment_context: Option<WorldContext>,
    pub(crate) last_environment_revision: Option<ClockRevision>,
    pub(crate) last_time_probe_rejection_at: Option<Instant>,
}

impl Client {
    pub(crate) fn new(stream: TcpStream, connected_at: Instant) -> Self {
        Self {
            phase: ConnectionPhase::AwaitIdentify,
            disconnect_reason: None,
            stream,
            input: Vec::with_capacity(512),
            output: Vec::new(),
            output_offset: 0,
            output_frames: VecDeque::new(),
            bytes_in: 0,
            bytes_out: 0,
            frames_in: 0,
            frames_out: 0,
            encoding_us: 0,
            replicated_players: RemotePlayers::default(),
            replication_initialized: false,
            connected_at,
            last_message_at: connected_at,
            peer_eof: false,
            last_fire_at: None,
            last_chat_at: None,
            pending_health_update: None,
            time_probe_received_at: [None; 2],
            last_environment_sync_at: None,
            last_environment_context: None,
            last_environment_revision: None,
            last_time_probe_rejection_at: None,
        }
    }

    pub(crate) fn pending_output_bytes(&self) -> usize {
        self.output.len().saturating_sub(self.output_offset)
    }

    pub(crate) fn oldest_output_age(&self) -> Option<Duration> {
        self.output_frames.front().map(|(_, at)| at.elapsed())
    }
    pub(crate) fn identity(&self) -> Option<Identity> {
        match self.phase {
            ConnectionPhase::AwaitIdentify => None,
            ConnectionPhase::Active(identity) => Some(identity),
            ConnectionPhase::Draining { identity, .. } => identity,
        }
    }

    pub(crate) fn player_id(&self) -> Option<PlayerId> {
        self.identity().map(|identity| identity.player)
    }

    pub(crate) fn activate(&mut self, player: PlayerId, profile: PlayerProfileId) {
        debug_assert!(matches!(self.phase, ConnectionPhase::AwaitIdentify));
        self.phase = ConnectionPhase::Active(Identity { player, profile });
    }

    pub(crate) fn accept_time_probe(&mut self, now: Instant) -> bool {
        for received_at in &mut self.time_probe_received_at {
            if received_at.is_some_and(|at| now.saturating_duration_since(at) >= TIME_PROBE_WINDOW)
            {
                *received_at = None;
            }
        }
        if let Some(slot) = self
            .time_probe_received_at
            .iter_mut()
            .find(|received_at| received_at.is_none())
        {
            *slot = Some(now);
            true
        } else {
            false
        }
    }

    pub(crate) fn mark_environment_sent(
        &mut self,
        context: WorldContext,
        revision: ClockRevision,
        now: Instant,
    ) {
        self.last_environment_sync_at = Some(now);
        self.last_environment_context = Some(context);
        self.last_environment_revision = Some(revision);
    }

    pub(crate) fn close_deadline(&self) -> Option<Instant> {
        match self.phase {
            ConnectionPhase::Draining { deadline, .. } => Some(deadline),
            _ => None,
        }
    }

    pub(crate) fn begin_close(&mut self, now: Instant, reason: CloseReason) {
        if self.close_deadline().is_none() {
            self.phase = ConnectionPhase::Draining {
                identity: self.identity(),
                deadline: now + CLIENT_CLOSE_TIMEOUT,
                reason,
            };
        }
    }

    pub(crate) fn reason(&self) -> Option<CloseReason> {
        self.disconnect_reason.or(match self.phase {
            ConnectionPhase::Draining { reason, .. } => Some(reason),
            _ => None,
        })
    }

    pub(crate) fn flush(&mut self, budget: usize) -> network::FlushResult {
        let result = network::flush(
            &mut self.stream,
            &self.output,
            &mut self.output_offset,
            budget,
        );
        account_output(self, result.written);
        result
    }
}

pub(crate) fn queue_close_response(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
    now: Instant,
) -> bool {
    let reason = if matches!(message, ServerMessage::Bye) {
        CloseReason::Quit
    } else {
        CloseReason::Rejected
    };
    if !queue_message(client, metrics, message) {
        metrics.dropped_messages = metrics.dropped_messages.saturating_add(1);
        client.disconnect_reason = Some(CloseReason::Backpressure);
        return false;
    }
    client.begin_close(now, reason);
    true
}

pub(crate) fn queue_message(
    client: &mut Client,
    metrics: &mut RuntimeMetrics,
    message: ServerMessage,
) -> bool {
    if client.close_deadline().is_some() {
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
    if client.close_deadline().is_some() {
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

fn prepare_output(client: &mut Client) {
    if client.output_offset > 0 {
        let pending = client.output.len() - client.output_offset;
        client.output.copy_within(client.output_offset.., 0);
        client.output.truncate(pending);
        client.output_offset = 0;
    }
}

fn account_output(client: &mut Client, mut written: usize) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{TcpListener, TcpStream};

    fn client_without_io() -> Client {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let peer = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        drop(peer);
        Client::new(server, Instant::now())
    }

    #[test]
    fn byte_budget_rejects_before_frame_count_limit() {
        let mut client = client_without_io();
        let mut metrics = RuntimeMetrics::default();
        let frame = [0; 1024];

        for _ in 0..MAX_QUEUED_OUTPUT_BYTES / frame.len() {
            assert!(queue_encoded_frame(&mut client, &frame, &mut metrics));
        }

        assert_eq!(client.output_frames.len(), 64);
        assert_eq!(client.pending_output_bytes(), MAX_QUEUED_OUTPUT_BYTES);
        assert!(!queue_encoded_frame(&mut client, &[0], &mut metrics));
        assert!(!queue_message(
            &mut client,
            &mut metrics,
            ServerMessage::Bye
        ));
        assert_eq!(client.output_frames.len(), 64);
    }

    #[test]
    fn frame_count_limit_rejects_before_byte_budget() {
        let mut client = client_without_io();
        let mut metrics = RuntimeMetrics::default();

        for _ in 0..MAX_QUEUED_OUTPUT_FRAMES {
            assert!(queue_encoded_frame(&mut client, &[0], &mut metrics));
        }

        assert_eq!(client.pending_output_bytes(), MAX_QUEUED_OUTPUT_FRAMES);
        assert!(!queue_encoded_frame(&mut client, &[0], &mut metrics));
        assert!(!queue_message(
            &mut client,
            &mut metrics,
            ServerMessage::Bye
        ));
        assert_eq!(client.output_frames.len(), MAX_QUEUED_OUTPUT_FRAMES);
    }
}
