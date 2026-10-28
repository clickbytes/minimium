use crate::{connection::Client, MAX_PENDING_HANDSHAKES, MAX_PENDING_WORLD_INPUTS};
use minimium_protocol::{encode_server, RemotePlayers, ServerErrorCode, ServerMessage};
use std::{
    collections::VecDeque,
    io::{self, Write},
    net::TcpListener,
    time::Instant,
};

const MAX_ACCEPTS_PER_POLL: usize = MAX_PENDING_HANDSHAKES;

pub(crate) fn accept_clients(
    listener: &TcpListener,
    clients: &mut Vec<Client>,
    max_client_connections: usize,
    pending_world_input_count: usize,
) -> io::Result<()> {
    let mut accepted_this_poll = 0usize;
    loop {
        if accepted_this_poll >= MAX_ACCEPTS_PER_POLL {
            break;
        }
        match listener.accept() {
            Ok((stream, _)) => {
                accepted_this_poll += 1;
                if let Err(error) = stream.set_nonblocking(true) {
                    eprintln!("client I/O setup failed: {error}");
                    continue;
                }
                let rejection = if clients.len() >= max_client_connections
                    || pending_world_input_count + clients.len() >= MAX_PENDING_WORLD_INPUTS
                {
                    Some(ServerErrorCode::ServerFull)
                } else if clients
                    .iter()
                    .filter(|client| client.player_id.is_none())
                    .count()
                    >= MAX_PENDING_HANDSHAKES
                {
                    Some(ServerErrorCode::Busy)
                } else {
                    None
                };
                if let Some(error_code) = rejection {
                    let mut stream = stream;
                    if let Ok(frame) = encode_server(&ServerMessage::Error(error_code)) {
                        let _ = stream.write(&frame);
                    }
                    continue;
                }
                if let Err(error) = stream.set_nodelay(true) {
                    eprintln!("client I/O setup failed: {error}");
                    continue;
                }
                if let Err(error) = stream.set_nonblocking(true) {
                    eprintln!("client I/O setup failed: {error}");
                    continue;
                }
                let connected_at = Instant::now();
                let client = Client {
                    player_id: None,
                    profile_id: None,
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
                    close_deadline: None,
                    last_fire_at: None,
                    last_chat_at: None,
                    pending_health_update: None,
                };
                clients.push(client);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
