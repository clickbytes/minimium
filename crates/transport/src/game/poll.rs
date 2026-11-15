use super::*;

pub(crate) fn new_game_state(
    listener: TcpListener,
    max_connections: usize,
) -> Arc<Mutex<GameState>> {
    Arc::new(Mutex::new(GameState {
        listener: Some(listener),
        connections: BTreeMap::new(),
        next_connection_id: Some(1),
        max_connections,
        totals: GameConnectionStats::default(),
        max_input_bytes: 0,
        max_output_bytes: 0,
        first_write_connection: 0,
    }))
}

pub(crate) fn poll_game(
    state: &Arc<Mutex<GameState>>,
    now: Instant,
) -> Result<Vec<GameTransportEvent>, GamePollError> {
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut events = Vec::with_capacity(MAX_MESSAGES_PER_POLL);
    accept_clients(&mut state, now, &mut events)?;
    let ids = state.connections.keys().copied().collect::<Vec<_>>();
    let mut read_budget = MAX_MESSAGES_PER_POLL;
    for id in ids {
        if events.len() >= MAX_MESSAGES_PER_POLL || read_budget == 0 {
            break;
        }
        let mut buffer = [0u8; 1024];
        let (bytes_read, input_len, read_error) = {
            let Some(socket) = state.connections.get_mut(&id) else {
                continue;
            };
            if socket.peer_eof || socket.close_after_flush.is_some() {
                continue;
            }
            let bytes_read = match socket.stream.read(&mut buffer) {
                Ok(0) => {
                    socket.peer_eof = true;
                    0
                }
                Ok(count) => {
                    socket.stats.bytes_in = socket.stats.bytes_in.saturating_add(count as u64);
                    socket.input.extend_from_slice(&buffer[..count]);
                    count
                }
                Err(error)
                    if error.kind() == io::ErrorKind::Interrupted
                        || error.kind() == io::ErrorKind::WouldBlock =>
                {
                    0
                }
                Err(_) => usize::MAX,
            };
            (bytes_read, socket.input.len(), bytes_read == usize::MAX)
        };
        if read_error {
            let socket = state.connections.remove(&id).expect("connection exists");
            events.push(disconnected(id, socket, GameDisconnectReason::ReadError));
            continue;
        }
        if bytes_read > 0 {
            state.totals.bytes_in = state.totals.bytes_in.saturating_add(bytes_read as u64);
        }
        state.max_input_bytes = state.max_input_bytes.max(input_len);
        if state
            .connections
            .get(&id)
            .is_some_and(|socket| socket.input.len() > minimium_protocol::MAX_FRAME_BYTES)
        {
            let socket = state.connections.remove(&id).expect("connection exists");
            events.push(disconnected(id, socket, GameDisconnectReason::InvalidFrame));
            continue;
        }
        while events.len() < MAX_MESSAGES_PER_POLL {
            let decoded = {
                let socket = state.connections.get(&id).expect("connection exists");
                minimium_protocol::decode_client(&socket.input)
            };
            match decoded {
                Ok(Some((message, bytes))) => {
                    {
                        let socket = state.connections.get_mut(&id).expect("connection exists");
                        socket.input.drain(..bytes);
                        socket.last_message_at = now;
                        socket.stats.frames_in = socket.stats.frames_in.saturating_add(1);
                    }
                    state.totals.frames_in = state.totals.frames_in.saturating_add(1);
                    events.push(GameTransportEvent::Message {
                        connection_id: id,
                        message,
                        at: now,
                    });
                    read_budget -= 1;
                }
                Ok(None) => break,
                Err(minimium_protocol::ProtocolError::UnsupportedVersion(_)) => {
                    let queued = if let Ok(frame) =
                        minimium_protocol::encode_server(&minimium_protocol::ServerMessage::Error(
                            minimium_protocol::ServerErrorCode::Protocol,
                        )) {
                        let socket = state.connections.get_mut(&id).expect("connection exists");
                        if socket.output_frames.len() < MAX_QUEUED_OUTPUT_FRAMES
                            && socket
                                .output
                                .len()
                                .saturating_sub(socket.output_offset)
                                .saturating_add(minimium_protocol::MAX_SERVER_FRAME_BYTES)
                                <= MAX_QUEUED_OUTPUT_BYTES
                        {
                            compact_output(socket);
                            socket.output.extend_from_slice(&frame);
                            socket.output_frames.push_back((frame.len(), now));
                            socket.close_after_flush = Some(now + Duration::from_secs(2));
                            socket.close_reason = Some(GameDisconnectReason::Rejected);
                            socket.stats.frames_out = socket.stats.frames_out.saturating_add(1);
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if queued {
                        state.totals.frames_out = state.totals.frames_out.saturating_add(1);
                        events.push(GameTransportEvent::Closing {
                            connection_id: id,
                            reason: GameDisconnectReason::Rejected,
                            at: now,
                        });
                    } else {
                        let socket = state.connections.remove(&id).expect("connection exists");
                        events.push(disconnected(id, socket, GameDisconnectReason::Backpressure));
                    }
                    break;
                }
                Err(_) => {
                    let socket = state.connections.remove(&id).expect("connection exists");
                    events.push(disconnected(id, socket, GameDisconnectReason::InvalidFrame));
                    break;
                }
            }
        }
    }
    Ok(events)
}

pub(crate) fn flush_game(state: &Arc<Mutex<GameState>>, now: Instant) -> Vec<GameTransportEvent> {
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut events = Vec::new();
    let mut write_budget = MAX_WRITE_BYTES_PER_POLL;
    let mut ids = state.connections.keys().copied().collect::<Vec<_>>();
    if !ids.is_empty() {
        let first = state.first_write_connection % ids.len();
        ids.rotate_left(first);
        state.first_write_connection = (first + 1) % ids.len();
    } else {
        state.first_write_connection = 0;
    }
    for id in ids {
        if write_budget == 0 {
            break;
        }
        let budget = write_budget.min(MAX_WRITE_BYTES_PER_CONNECTION);
        let (written, max_pending_output) = {
            let Some(socket) = state.connections.get_mut(&id) else {
                continue;
            };
            let pending_before = socket.output.len().saturating_sub(socket.output_offset);
            let start = socket.output_offset;
            let end = socket.output.len().min(start.saturating_add(budget));
            let mut written = 0;
            while socket.output_offset < end {
                match socket
                    .stream
                    .write(&socket.output[socket.output_offset..end])
                {
                    Ok(0) => {
                        socket.write_failed = true;
                        break;
                    }
                    Ok(count) => {
                        socket.output_offset += count;
                        written += count;
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                    Err(_) => {
                        socket.write_failed = true;
                        break;
                    }
                }
            }
            socket.stats.bytes_out = socket.stats.bytes_out.saturating_add(written as u64);
            account_frames(socket, written);
            (
                written,
                pending_before.max(socket.output.len().saturating_sub(socket.output_offset)),
            )
        };
        state.totals.bytes_out = state.totals.bytes_out.saturating_add(written as u64);
        state.max_output_bytes = state.max_output_bytes.max(max_pending_output);
        write_budget -= written;
    }
    for socket in state.connections.values_mut() {
        if socket.peer_eof && socket.close_after_flush.is_none() {
            socket.close_after_flush = Some(now + Duration::from_secs(2));
        }
    }
    let close_ids = state
        .connections
        .iter()
        .filter_map(|(id, socket)| {
            (socket.write_failed
                || socket.close_after_flush.is_some_and(|deadline| {
                    socket.output_offset == socket.output.len() || now >= deadline
                }))
            .then_some(*id)
        })
        .collect::<Vec<_>>();
    for id in close_ids
        .into_iter()
        .take(MAX_MESSAGES_PER_POLL.saturating_sub(events.len()))
    {
        if let Some(socket) = state.connections.remove(&id) {
            let reason = socket.close_reason.unwrap_or_else(|| {
                if socket.write_failed {
                    GameDisconnectReason::WriteError
                } else if socket.peer_eof {
                    GameDisconnectReason::PeerEof
                } else {
                    GameDisconnectReason::Closed
                }
            });
            events.push(disconnected(id, socket, reason));
        }
    }
    events
}

fn accept_clients(
    state: &mut GameState,
    now: Instant,
    events: &mut Vec<GameTransportEvent>,
) -> Result<(), GamePollError> {
    for _ in 0..MAX_ACCEPTS_PER_POLL {
        if events.len() >= MAX_MESSAGES_PER_POLL || state.connections.len() >= state.max_connections
        {
            break;
        }
        let Some(listener) = state.listener.as_ref() else {
            break;
        };
        match listener.accept() {
            Ok((stream, peer)) => {
                if let Err(error) = stream.set_nonblocking(true) {
                    events.push(GameTransportEvent::SocketSetupFailed {
                        operation: SocketSetupOperation::Nonblocking,
                        error_kind: error.kind(),
                    });
                    continue;
                }
                if let Err(error) = stream.set_nodelay(true) {
                    events.push(GameTransportEvent::SocketSetupFailed {
                        operation: SocketSetupOperation::Nodelay,
                        error_kind: error.kind(),
                    });
                    continue;
                }
                let value = state
                    .next_connection_id
                    .ok_or(GamePollError::IdentifierExhausted)?;
                state.next_connection_id = value.checked_add(1);
                let id = ConnectionId::new(value);
                state.connections.insert(
                    id,
                    GameSocket {
                        stream,
                        input: Vec::with_capacity(512),
                        output: Vec::new(),
                        output_offset: 0,
                        output_frames: VecDeque::new(),
                        close_after_flush: None,
                        close_reason: None,
                        last_message_at: now,
                        peer_eof: false,
                        write_failed: false,
                        stats: GameConnectionStats::default(),
                        encoding_time: Duration::ZERO,
                    },
                );
                events.push(GameTransportEvent::Connected {
                    connection_id: id,
                    peer,
                    at: now,
                });
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(GamePollError::Io(error)),
        }
    }
    Ok(())
}

pub(super) fn compact_output(socket: &mut GameSocket) {
    if socket.output_offset > 0 {
        let pending = socket.output.len() - socket.output_offset;
        socket.output.copy_within(socket.output_offset.., 0);
        socket.output.truncate(pending);
        socket.output_offset = 0;
    }
}

fn account_frames(socket: &mut GameSocket, mut written: usize) {
    while written > 0 {
        let Some((front, _)) = socket.output_frames.front_mut() else {
            break;
        };
        let consumed = written.min(*front);
        written -= consumed;
        *front -= consumed;
        if *front == 0 {
            socket.output_frames.pop_front();
        }
    }
}

fn disconnected(
    id: ConnectionId,
    socket: GameSocket,
    reason: GameDisconnectReason,
) -> GameTransportEvent {
    GameTransportEvent::Disconnected {
        connection_id: id,
        reason,
        stats: socket.stats,
    }
}
