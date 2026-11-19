use std::{
    fmt,
    io::{self, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    time::{Duration, Instant},
};

use crate::QueueError;
mod typed;
use minimium_commands::{
    admin::{AdminRequestV1, Correlation},
    operation::OperationId,
    results::{OperationResult, Stage},
};
pub use typed::AdminInput;

const MAX_ACTIVE_CLIENTS: usize = 8;
const MAX_ACCEPTS_PER_POLL: usize = 4;
const MAX_LINE_BYTES: usize = 512;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REPLY_BYTES: usize = 64 * 1024;
const MAX_READ_BYTES_PER_CLIENT_PER_POLL: usize = 1024;
const MAX_WRITE_BYTES_PER_CLIENT_PER_POLL: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AdminRequestId(u64);

impl AdminRequestId {
    /// Creates an ID for an externally hosted transport; zero is reserved.
    pub const fn new(value: u64) -> Option<Self> {
        if value == 0 {
            None
        } else {
            Some(Self(value))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for AdminRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Neutral response data supplied after runtime-side execution and formatting.
pub struct AdminResponse {
    pub typed: Option<OperationResult>,
    pub success: bool,
    pub error_code: Option<String>,
    pub key: Option<String>,
    pub text: String,
    pub usage: Option<String>,
}

pub enum AdminTransportEvent {
    Request {
        id: AdminRequestId,
        peer: SocketAddr,
        input: AdminInput,
    },
}

pub(crate) struct AdminState {
    listener: Option<TcpListener>,
    token: String,
    clients: Vec<Client>,
    next_request_id: Option<u64>,
    correlations: typed::Correlations,
}

enum Phase {
    Authentication,
    Command,
    AwaitingResponse,
    WritingResponse,
}

struct Client {
    stream: TcpStream,
    peer: SocketAddr,
    phase: Phase,
    line: Vec<u8>,
    deadline: Instant,
    response_deadline: Option<Instant>,
    id: Option<AdminRequestId>,
    correlation: Option<(Correlation, OperationId)>,
    output: Vec<u8>,
    output_offset: usize,
}

impl AdminState {
    pub(crate) fn new(listener: TcpListener, token: String) -> io::Result<Self> {
        listener.set_nonblocking(true)?;
        Ok(Self {
            listener: Some(listener),
            token,
            clients: Vec::with_capacity(MAX_ACTIVE_CLIENTS),
            next_request_id: Some(1),
            correlations: Default::default(),
        })
    }

    pub(crate) fn poll(&mut self, now: Instant) -> io::Result<Vec<AdminTransportEvent>> {
        let mut accepted = 0;
        while self.listener.is_some() && accepted < MAX_ACCEPTS_PER_POLL {
            match self.listener.as_ref().expect("listener checked").accept() {
                Ok((stream, peer)) => {
                    accepted += 1;
                    stream.set_nonblocking(true)?;
                    if self.clients.len() >= MAX_ACTIVE_CLIENTS {
                        continue;
                    }
                    self.clients.push(Client {
                        stream,
                        peer,
                        phase: Phase::Authentication,
                        line: Vec::with_capacity(128),
                        deadline: now + REQUEST_TIMEOUT,
                        response_deadline: None,
                        id: None,
                        correlation: None,
                        output: Vec::new(),
                        output_offset: 0,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        let mut events = Vec::new();
        let mut index = 0;
        while index < self.clients.len() {
            let client = &mut self.clients[index];
            let mut keep = true;
            if matches!(client.phase, Phase::WritingResponse)
                && client
                    .response_deadline
                    .is_some_and(|deadline| now >= deadline)
            {
                keep = false;
            } else if now >= client.deadline && !matches!(client.phase, Phase::WritingResponse) {
                if matches!(client.phase, Phase::AwaitingResponse) {
                    if client.correlation.is_some() {
                        typed::reject(
                            client,
                            "unknown_after_disconnect",
                            "request expired before owner response",
                            Stage::UnknownAfterDisconnect,
                            now,
                        );
                    } else {
                        queue_reply(client, b"ERR request timed out\n", now);
                    }
                } else {
                    keep = false;
                }
            }
            if keep && matches!(client.phase, Phase::Authentication | Phase::Command) {
                let mut bytes = [0u8; MAX_READ_BYTES_PER_CLIENT_PER_POLL];
                match client.stream.read(&mut bytes) {
                    Ok(0) => keep = false,
                    Ok(count) => {
                        for byte in bytes[..count].iter().copied() {
                            if byte == b'\n' {
                                if client.line.last() == Some(&b'\r') {
                                    client.line.pop();
                                }
                                let line = match std::str::from_utf8(&client.line) {
                                    Ok(line) => line.to_owned(),
                                    Err(_) => {
                                        queue_reply(client, b"ERR invalid UTF-8\n", now);
                                        break;
                                    }
                                };
                                client.line.clear();
                                match client.phase {
                                    Phase::Authentication
                                        if line == format!("AUTH {}", self.token) =>
                                    {
                                        client.phase = Phase::Command
                                    }
                                    Phase::Authentication => {
                                        queue_reply(client, b"ERR unauthorized\n", now);
                                        break;
                                    }
                                    Phase::Command => {
                                        let input = if line.trim_start().starts_with("@console/") {
                                            match AdminRequestV1::decode(&line) {
                                                Ok(r) => {
                                                    client.correlation =
                                                        Some((r.correlation, r.operation()));
                                                    if let Err(error) =
                                                        self.correlations.admit(r.correlation)
                                                    {
                                                        typed::reject(
                                                            client,
                                                            error.code.as_str(),
                                                            &error.reason,
                                                            Stage::Rejected,
                                                            now,
                                                        );
                                                        break;
                                                    }
                                                    AdminInput::Typed(r)
                                                }
                                                Err(error) => {
                                                    typed::reject(
                                                        client,
                                                        error.code.as_str(),
                                                        &error.reason,
                                                        Stage::Rejected,
                                                        now,
                                                    );
                                                    break;
                                                }
                                            }
                                        } else {
                                            AdminInput::Legacy(line.trim().to_owned())
                                        };
                                        let Some(value) = self.next_request_id else {
                                            typed::reject(
                                                client,
                                                "result_limit",
                                                "transport request IDs exhausted",
                                                Stage::Rejected,
                                                now,
                                            );
                                            break;
                                        };
                                        self.next_request_id = value.checked_add(1);
                                        let id = AdminRequestId::new(value)
                                            .expect("host request IDs start at one");
                                        client.id = Some(id);
                                        client.phase = Phase::AwaitingResponse;
                                        events.push(AdminTransportEvent::Request {
                                            id,
                                            peer: client.peer,
                                            input,
                                        });
                                        break;
                                    }
                                    Phase::AwaitingResponse | Phase::WritingResponse => break,
                                }
                            } else if client.line.len() == MAX_LINE_BYTES {
                                queue_reply(
                                    client,
                                    b"ERR admin request line exceeds size limit\n",
                                    now,
                                );
                                break;
                            } else {
                                client.line.push(byte);
                            }
                        }
                    }
                    Err(error)
                        if error.kind() == io::ErrorKind::WouldBlock
                            || error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => keep = false,
                }
            }
            if keep && matches!(client.phase, Phase::WritingResponse) {
                let end = client.output.len().min(
                    client
                        .output_offset
                        .saturating_add(MAX_WRITE_BYTES_PER_CLIENT_PER_POLL),
                );
                while client.output_offset < end {
                    match client
                        .stream
                        .write(&client.output[client.output_offset..end])
                    {
                        Ok(0) => {
                            keep = false;
                            break;
                        }
                        Ok(count) => client.output_offset += count,
                        Err(error)
                            if error.kind() == io::ErrorKind::WouldBlock
                                || error.kind() == io::ErrorKind::Interrupted =>
                        {
                            break
                        }
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
        Ok(events)
    }

    pub(crate) fn respond(
        &mut self,
        id: AdminRequestId,
        response: AdminResponse,
    ) -> Result<(), QueueError> {
        let now = Instant::now();
        let Some(client) = self.clients.iter_mut().find(|client| {
            client.id == Some(id)
                && matches!(client.phase, Phase::AwaitingResponse)
                && now < client.deadline
        }) else {
            return Err(QueueError::UnknownAdminRequest);
        };
        if client.correlation.is_some() {
            if let Some(result) = response.typed {
                return typed::send(client, result, now);
            }
            typed::reject(
                client,
                response.error_code.as_deref().unwrap_or("invalid_response"),
                &response.text,
                if response.error_code.as_deref() == Some("server.stopping") {
                    Stage::Cancelled
                } else {
                    Stage::Rejected
                },
                now,
            );
            return Ok(());
        }
        let detail_bytes = response
            .text
            .len()
            .saturating_add(response.key.as_ref().map_or(0, String::len))
            .saturating_add(response.usage.as_ref().map_or(0, String::len))
            .saturating_add(response.error_code.as_ref().map_or(0, String::len));
        if detail_bytes > MAX_REPLY_BYTES {
            queue_reply(client, b"ERR reply exceeds size limit\n", now);
            return Err(QueueError::AdminReplyTooLarge);
        }
        let text = if response.success {
            format!("OK {}", response.text)
        } else {
            let code = response
                .error_code
                .as_deref()
                .unwrap_or("server.command.failed");
            encode_error_envelope(
                code,
                response.key.as_deref(),
                &response.text,
                response.usage.as_deref(),
            )
        };
        if text.len().saturating_add(1) > MAX_REPLY_BYTES {
            queue_reply(client, b"ERR reply exceeds size limit\n", now);
            return Err(QueueError::AdminReplyTooLarge);
        }
        let mut bytes = text.into_bytes();
        bytes.push(b'\n');
        queue_reply(client, &bytes, now);
        Ok(())
    }

    pub(crate) fn stop_admission(&mut self) {
        self.listener.take();
    }
    pub(crate) fn close_all(&mut self) {
        self.listener.take();
        self.clients.clear();
    }
    pub(crate) fn has_pending_output(&self) -> bool {
        self.clients.iter().any(|client| {
            matches!(client.phase, Phase::WritingResponse)
                && client.output_offset < client.output.len()
        })
    }
    pub(crate) fn is_pending(&self, id: AdminRequestId, now: Instant) -> bool {
        self.clients.iter().any(|client| {
            client.id == Some(id)
                && matches!(client.phase, Phase::AwaitingResponse)
                && now < client.deadline
        })
    }
    pub(crate) fn reject_pending(&mut self) {
        let ids = self
            .clients
            .iter()
            .filter(|client| matches!(client.phase, Phase::AwaitingResponse))
            .filter_map(|client| client.id)
            .collect::<Vec<_>>();
        for id in ids {
            let _ = self.respond(
                id,
                AdminResponse {
                    typed: None,
                    success: false,
                    error_code: Some("server.stopping".to_owned()),
                    key: None,
                    text: "server stopping".to_owned(),
                    usage: None,
                },
            );
        }
    }
}

fn queue_reply(client: &mut Client, bytes: &[u8], now: Instant) {
    client.phase = Phase::WritingResponse;
    client.response_deadline = Some(now + RESPONSE_TIMEOUT);
    client.output.clear();
    client.output.extend_from_slice(bytes);
    client.output_offset = 0;
}

#[derive(serde::Serialize)]
struct ErrorEnvelope<'a> {
    schema_version: u8,
    code: &'a str,
    key: Option<&'a str>,
    reason: &'a str,
    usage: Option<&'a str>,
}

fn encode_error_envelope(
    code: &str,
    key: Option<&str>,
    reason: &str,
    usage: Option<&str>,
) -> String {
    serde_json::to_string(&ErrorEnvelope {
        schema_version: 1,
        code,
        key,
        reason,
        usage,
    })
    .map_or_else(
        |_| "ERR admin error serialization failed".to_owned(),
        |json| format!("ERRJSON {json}"),
    )
}
