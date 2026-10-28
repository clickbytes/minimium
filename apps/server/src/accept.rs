use crate::{connection::Client, MAX_PENDING_HANDSHAKES, MAX_PENDING_WORLD_INPUTS};
use minimium_protocol::{encode_server, ServerErrorCode, ServerMessage};
use std::{
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
                    .filter(|client| client.player_id().is_none())
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
                clients.push(Client::new(stream, connected_at));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) => return Err(error),
        }
    }
    Ok(())
}
