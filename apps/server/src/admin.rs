use minimium_platform::commands::ServerCommand;
use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    time::{Duration, Instant},
};

const MAX_ACTIVE_CLIENTS: usize = 8;
const MAX_ACCEPTS_PER_POLL: usize = 4;
const MAX_LINE_BYTES: usize = 512;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REPLY_BYTES: usize = 64 * 1024;
const MAX_READ_BYTES_PER_CLIENT_PER_POLL: usize = 1024;
const MAX_WRITE_BYTES_PER_CLIENT_PER_POLL: usize = 4096;

pub type RequestId = u64;

pub struct AdminRequest {
    pub id: RequestId,
    pub command: ServerCommand,
}

enum ClientPhase {
    Authentication,
    Command,
    AwaitingResponse,
    WritingResponse,
}

struct Client {
    stream: TcpStream,
    phase: ClientPhase,
    line: Vec<u8>,
    deadline: Instant,
    response_deadline: Option<Instant>,
    id: Option<RequestId>,
    output: Vec<u8>,
    output_offset: usize,
}

pub struct AdminServer {
    listener: TcpListener,
    token: String,
    clients: Vec<Client>,
    next_request_id: Option<RequestId>,
}

impl AdminServer {
    pub fn new(listener: TcpListener, token: String) -> io::Result<Self> {
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener,
            token,
            clients: Vec::with_capacity(MAX_ACTIVE_CLIENTS),
            next_request_id: Some(1),
        })
    }

    pub fn poll(&mut self, now: Instant) -> io::Result<Vec<AdminRequest>> {
        let mut accepted = 0;
        while accepted < MAX_ACCEPTS_PER_POLL {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    accepted += 1;
                    stream.set_nonblocking(true)?;
                    if self.clients.len() >= MAX_ACTIVE_CLIENTS {
                        drop(stream);
                        continue;
                    }
                    self.clients.push(Client {
                        stream,
                        phase: ClientPhase::Authentication,
                        line: Vec::with_capacity(128),
                        deadline: now + REQUEST_TIMEOUT,
                        response_deadline: None,
                        id: None,
                        output: Vec::new(),
                        output_offset: 0,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }

        let mut requests = Vec::new();
        let mut index = 0;
        while index < self.clients.len() {
            let mut keep = true;
            let client = &mut self.clients[index];
            if matches!(client.phase, ClientPhase::WritingResponse)
                && client
                    .response_deadline
                    .is_some_and(|deadline| now >= deadline)
            {
                keep = false;
            } else if now >= client.deadline
                && !matches!(client.phase, ClientPhase::WritingResponse)
            {
                if matches!(client.phase, ClientPhase::AwaitingResponse) {
                    queue_reply(client, b"ERR request timed out\n", now);
                } else {
                    keep = false;
                }
            }

            if keep
                && matches!(
                    client.phase,
                    ClientPhase::Authentication | ClientPhase::Command
                )
            {
                let mut buffer = [0u8; MAX_READ_BYTES_PER_CLIENT_PER_POLL];
                let mut consumed = 0;
                let mut reached_eof = false;
                loop {
                    match client.stream.read(&mut buffer[consumed..]) {
                        Ok(0) => {
                            reached_eof = true;
                            break;
                        }
                        Ok(count) => {
                            consumed += count;
                            if consumed == buffer.len() {
                                break;
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(_) => {
                            keep = false;
                            break;
                        }
                    }
                }
                for byte in buffer[..consumed].iter().copied() {
                    if !keep {
                        break;
                    }
                    if byte == b'\n' {
                        if client.line.last() == Some(&b'\r') {
                            client.line.pop();
                        }
                        let line = match std::str::from_utf8(&client.line) {
                            Ok(line) => line.to_owned(),
                            Err(_) => {
                                queue_reply(client, b"ERR invalid UTF-8\n", now);
                                client.line.clear();
                                break;
                            }
                        };
                        client.line.clear();
                        match client.phase {
                            ClientPhase::Authentication => {
                                if line == format!("AUTH {}", self.token) {
                                    client.phase = ClientPhase::Command;
                                } else {
                                    queue_reply(client, b"ERR unauthorized\n", now);
                                    break;
                                }
                            }
                            ClientPhase::Command => match ServerCommand::parse(line.trim()) {
                                Ok(command) => {
                                    let Some(id) = self.next_request_id else {
                                        queue_reply(client, b"ERR request IDs exhausted\n", now);
                                        break;
                                    };
                                    self.next_request_id = id.checked_add(1);
                                    client.id = Some(id);
                                    client.phase = ClientPhase::AwaitingResponse;
                                    requests.push(AdminRequest { id, command });
                                    break;
                                }
                                Err(error) => {
                                    queue_text_reply(client, format!("ERR {error}"), now);
                                    break;
                                }
                            },
                            ClientPhase::AwaitingResponse | ClientPhase::WritingResponse => break,
                        }
                    } else {
                        if client.line.len() == MAX_LINE_BYTES {
                            queue_reply(
                                client,
                                b"ERR admin request line exceeds size limit\n",
                                now,
                            );
                            break;
                        }
                        client.line.push(byte);
                    }
                }
                if reached_eof
                    && matches!(
                        client.phase,
                        ClientPhase::Authentication | ClientPhase::Command
                    )
                {
                    keep = false;
                }
            }

            if keep && matches!(client.phase, ClientPhase::WritingResponse) {
                let mut written = 0;
                while client.output_offset < client.output.len()
                    && written < MAX_WRITE_BYTES_PER_CLIENT_PER_POLL
                {
                    let end = client
                        .output_offset
                        .saturating_add(MAX_WRITE_BYTES_PER_CLIENT_PER_POLL - written)
                        .min(client.output.len());
                    match client
                        .stream
                        .write(&client.output[client.output_offset..end])
                    {
                        Ok(0) => {
                            keep = false;
                            break;
                        }
                        Ok(count) => {
                            client.output_offset += count;
                            written += count;
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(_) => {
                            keep = false;
                            break;
                        }
                    }
                }
                if client.output_offset == client.output.len() {
                    keep = false;
                }
            }

            if keep {
                index += 1;
            } else {
                self.clients.swap_remove(index);
            }
        }
        Ok(requests)
    }

    pub fn respond(&mut self, id: RequestId, success: bool, text: &str) {
        let Some(client) = self.clients.iter_mut().find(|client| {
            client.id == Some(id) && matches!(client.phase, ClientPhase::AwaitingResponse)
        }) else {
            return;
        };
        let prefix = if success { "OK " } else { "ERR " };
        let reply_len = prefix.len().saturating_add(text.len()).saturating_add(1);
        if reply_len > MAX_REPLY_BYTES {
            queue_reply(client, b"ERR reply exceeds size limit\n", Instant::now());
            return;
        }
        let mut bytes = Vec::with_capacity(reply_len);
        bytes.extend_from_slice(prefix.as_bytes());
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(b'\n');
        queue_reply(client, &bytes, Instant::now());
    }

    /// Returns whether a request is still waiting for its dispatcher response.
    pub fn is_pending(&self, id: RequestId, now: Instant) -> bool {
        self.clients.iter().any(|client| {
            client.id == Some(id)
                && matches!(client.phase, ClientPhase::AwaitingResponse)
                && now < client.deadline
        })
    }

    /// Rejects all requests that have not yet received a dispatcher response.
    pub fn reject_pending(&mut self, text: &str) {
        let ids: Vec<_> = self
            .clients
            .iter()
            .filter(|client| matches!(client.phase, ClientPhase::AwaitingResponse))
            .filter_map(|client| client.id)
            .collect();
        for id in ids {
            self.respond(id, false, text);
        }
    }

    pub fn has_pending_output(&self) -> bool {
        self.clients.iter().any(|client| {
            matches!(client.phase, ClientPhase::WritingResponse)
                && client.output_offset < client.output.len()
        })
    }
}

fn queue_text_reply(client: &mut Client, text: String, now: Instant) {
    let mut bytes = text.into_bytes();
    bytes.push(b'\n');
    queue_reply(client, &bytes, now);
}

fn queue_reply(client: &mut Client, bytes: &[u8], now: Instant) {
    client.output.clear();
    client.output.extend_from_slice(bytes);
    client.output_offset = 0;
    client.response_deadline = Some(now.checked_add(RESPONSE_TIMEOUT).unwrap_or(now));
    client.phase = ClientPhase::WritingResponse;
}
