use std::{error::Error, fmt, sync::Arc};

pub const PROTOCOL_VERSION: u8 = 8;
pub const HEADER_BYTES: usize = 4;
pub const MAX_PAYLOAD_BYTES: usize = 4096;
pub const MAX_FRAME_BYTES: usize = HEADER_BYTES + MAX_PAYLOAD_BYTES;
pub const MAX_VISIBLE_PLAYERS: usize = 16;
pub const PLAYER_PROFILE_ID_BYTES: usize = 16;
pub const MAX_CHAT_BYTES: usize = 128;

const CLIENT_PING: u8 = 1;
const CLIENT_SET_NAME: u8 = 2;
const CLIENT_MOVE: u8 = 3;
const CLIENT_QUIT: u8 = 4;
const CLIENT_IDENTIFY: u8 = 5;
const CLIENT_FIRE: u8 = 6;
const CLIENT_CHAT: u8 = 7;
const SERVER_WELCOME: u8 = 0x81;
const SERVER_PONG: u8 = 0x82;
const SERVER_ACCEPTED: u8 = 0x83;
const SERVER_BYE: u8 = 0x84;
const SERVER_ERROR: u8 = 0x85;
const SERVER_SNAPSHOT: u8 = 0x86;
const SERVER_SHOT_RESULT: u8 = 0x87;
const SERVER_HEALTH_UPDATE: u8 = 0x88;
const SERVER_CHAT: u8 = 0x89;

pub const ERROR_PROTOCOL: u8 = 1;
pub const ERROR_BUSY: u8 = 2;
pub const ERROR_SERVER_FULL: u8 = 3;
pub const ERROR_RATE_LIMIT: u8 = 4;

#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    Identify([u8; PLAYER_PROFILE_ID_BYTES]),
    Ping,
    SetName(String),
    Move { direction: [f32; 2], jump: bool },
    Fire { direction: [f32; 3] },
    Chat(String),
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemotePlayer {
    pub id: u64,
    pub name: Arc<str>,
    pub position: [f32; 3],
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemotePlayers {
    entries: [Option<RemotePlayer>; MAX_VISIBLE_PLAYERS],
    len: usize,
}

impl Default for RemotePlayers {
    fn default() -> Self {
        Self {
            entries: std::array::from_fn(|_| None),
            len: 0,
        }
    }
}

impl RemotePlayers {
    pub fn push(&mut self, player: RemotePlayer) -> bool {
        if self.len == MAX_VISIBLE_PLAYERS || self.get(player.id).is_some() {
            return false;
        }
        self.entries[self.len] = Some(player);
        self.len += 1;
        true
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn get(&self, id: u64) -> Option<&RemotePlayer> {
        self.iter().find(|player| player.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &RemotePlayer> {
        self.entries[..self.len].iter().filter_map(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePlayerIds {
    entries: [Option<u64>; MAX_VISIBLE_PLAYERS],
    len: usize,
}

impl Default for RemotePlayerIds {
    fn default() -> Self {
        Self {
            entries: [None; MAX_VISIBLE_PLAYERS],
            len: 0,
        }
    }
}

impl RemotePlayerIds {
    pub fn push(&mut self, id: u64) -> bool {
        if self.len == MAX_VISIBLE_PLAYERS || self.contains(id) {
            return false;
        }
        self.entries[self.len] = Some(id);
        self.len += 1;
        true
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn contains(&self, id: u64) -> bool {
        self.iter().any(|candidate| *candidate == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &u64> {
        self.entries[..self.len].iter().filter_map(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    Welcome {
        id: u64,
        map_fingerprint: u64,
        health: u16,
    },
    Pong,
    Accepted,
    Bye,
    Error(u8),
    ShotResult {
        target_id: Option<u64>,
        remaining_health: Option<u16>,
        killed: bool,
    },
    HealthUpdate(u16),
    Chat {
        sender_id: u64,
        sender_name: String,
        text: String,
    },
    Snapshot {
        server_time_ms: u64,
        full_snapshot: bool,
        self_position: [f32; 3],
        self_velocity: [f32; 3],
        self_grounded: bool,
        players: RemotePlayers,
        removed_players: RemotePlayerIds,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    UnsupportedVersion(u8),
    UnknownMessage(u8),
    PayloadTooLarge,
    InvalidPayload,
    InvalidName,
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion(version) => {
                write!(formatter, "unsupported protocol version {version}")
            }
            Self::UnknownMessage(kind) => write!(formatter, "unknown message type {kind}"),
            Self::PayloadTooLarge => formatter.write_str("protocol payload exceeds limit"),
            Self::InvalidPayload => formatter.write_str("invalid protocol payload"),
            Self::InvalidName => formatter.write_str("invalid player name"),
        }
    }
}

impl Error for ProtocolError {}

pub fn encode_client(message: &ClientMessage) -> Result<Vec<u8>, ProtocolError> {
    let mut payload = Vec::with_capacity(16);
    let kind = match message {
        ClientMessage::Identify(profile_id) => {
            payload.extend_from_slice(profile_id);
            CLIENT_IDENTIFY
        }
        ClientMessage::Ping => CLIENT_PING,
        ClientMessage::SetName(name) => {
            write_name(name, &mut payload)?;
            CLIENT_SET_NAME
        }
        ClientMessage::Move { direction, jump } => {
            write_direction(*direction, &mut payload)?;
            payload.push(u8::from(*jump));
            CLIENT_MOVE
        }
        ClientMessage::Fire { direction } => {
            write_unit_direction(*direction, &mut payload)?;
            CLIENT_FIRE
        }
        ClientMessage::Chat(text) => {
            write_chat_text(text, &mut payload)?;
            CLIENT_CHAT
        }
        ClientMessage::Quit => CLIENT_QUIT,
    };
    encode_frame(kind, &payload)
}

pub fn encode_server(message: &ServerMessage) -> Result<Vec<u8>, ProtocolError> {
    let mut frame = Vec::with_capacity(128);
    encode_server_into(&mut frame, message)?;
    Ok(frame)
}

pub fn encode_server_into(
    frame: &mut Vec<u8>,
    message: &ServerMessage,
) -> Result<(), ProtocolError> {
    let frame_start = frame.len();
    frame.extend_from_slice(&[0; HEADER_BYTES]);
    let payload_start = frame.len();
    let encoded = (|| {
        let kind = match message {
            ServerMessage::Welcome {
                id,
                map_fingerprint,
                health,
            } => {
                if *health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&id.to_le_bytes());
                frame.extend_from_slice(&map_fingerprint.to_le_bytes());
                frame.extend_from_slice(&health.to_le_bytes());
                SERVER_WELCOME
            }
            ServerMessage::Pong => SERVER_PONG,
            ServerMessage::Accepted => SERVER_ACCEPTED,
            ServerMessage::Bye => SERVER_BYE,
            ServerMessage::Error(code) => {
                frame.push(*code);
                SERVER_ERROR
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            } => {
                if target_id.is_some() != remaining_health.is_some()
                    || (*killed && target_id.is_none())
                {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.push(u8::from(target_id.is_some()));
                if let (Some(id), Some(health)) = (target_id, remaining_health) {
                    frame.extend_from_slice(&id.to_le_bytes());
                    frame.extend_from_slice(&health.to_le_bytes());
                }
                frame.push(u8::from(*killed));
                SERVER_SHOT_RESULT
            }
            ServerMessage::HealthUpdate(health) => {
                if *health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&health.to_le_bytes());
                SERVER_HEALTH_UPDATE
            }
            ServerMessage::Chat {
                sender_id,
                sender_name,
                text,
            } => {
                frame.extend_from_slice(&sender_id.to_le_bytes());
                write_name(sender_name, frame)?;
                write_chat_text(text, frame)?;
                SERVER_CHAT
            }
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                self_grounded,
                players,
                removed_players,
            } => {
                if (*full_snapshot && !removed_players.is_empty())
                    || players
                        .iter()
                        .any(|player| removed_players.contains(player.id))
                {
                    return Err(ProtocolError::InvalidPayload);
                }
                frame.extend_from_slice(&server_time_ms.to_le_bytes());
                write_vector(*self_position, frame)?;
                write_vector(*self_velocity, frame)?;
                frame.push(u8::from(*self_grounded));
                frame.push(u8::from(*full_snapshot));
                frame.push(players.len() as u8);
                frame.push(removed_players.len() as u8);
                for player in players.iter() {
                    frame.extend_from_slice(&player.id.to_le_bytes());
                    write_name(&player.name, frame)?;
                    write_vector(player.position, frame)?;
                }
                for id in removed_players.iter() {
                    frame.extend_from_slice(&id.to_le_bytes());
                }
                SERVER_SNAPSHOT
            }
        };
        Ok(kind)
    })();

    let kind = match encoded {
        Ok(kind) => kind,
        Err(error) => {
            frame.truncate(frame_start);
            return Err(error);
        }
    };
    let payload_bytes = frame.len() - payload_start;
    if payload_bytes > MAX_PAYLOAD_BYTES || payload_bytes > u16::MAX as usize {
        frame.truncate(frame_start);
        return Err(ProtocolError::PayloadTooLarge);
    }
    frame[frame_start] = PROTOCOL_VERSION;
    frame[frame_start + 1] = kind;
    frame[frame_start + 2..frame_start + HEADER_BYTES]
        .copy_from_slice(&(payload_bytes as u16).to_le_bytes());
    Ok(())
}

pub fn decode_client(input: &[u8]) -> Result<Option<(ClientMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match kind {
        CLIENT_IDENTIFY => ClientMessage::Identify(reader.profile_id()?),
        CLIENT_PING => ClientMessage::Ping,
        CLIENT_SET_NAME => ClientMessage::SetName(reader.name()?),
        CLIENT_MOVE => ClientMessage::Move {
            direction: reader.direction()?,
            jump: reader.boolean()?,
        },
        CLIENT_FIRE => {
            let direction = reader.vector()?;
            if !is_unit_direction(direction) {
                return Err(ProtocolError::InvalidPayload);
            }
            ClientMessage::Fire { direction }
        }
        CLIENT_CHAT => ClientMessage::Chat(reader.chat_text()?.to_owned()),
        CLIENT_QUIT => ClientMessage::Quit,
        other => return Err(ProtocolError::UnknownMessage(other)),
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}

pub fn decode_server(input: &[u8]) -> Result<Option<(ServerMessage, usize)>, ProtocolError> {
    let Some((kind, payload, frame_bytes)) = decode_frame(input)? else {
        return Ok(None);
    };
    let mut reader = Reader::new(payload);
    let message = match kind {
        SERVER_WELCOME => {
            let id = reader.u64()?;
            let map_fingerprint = reader.u64()?;
            let health = reader.u16()?;
            if health > 100 {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::Welcome {
                id,
                map_fingerprint,
                health,
            }
        }
        SERVER_PONG => ServerMessage::Pong,
        SERVER_ACCEPTED => ServerMessage::Accepted,
        SERVER_BYE => ServerMessage::Bye,
        SERVER_ERROR => ServerMessage::Error(reader.byte()?),
        SERVER_SHOT_RESULT => {
            let hit = reader.boolean()?;
            let (target_id, remaining_health) = if hit {
                let id = reader.u64()?;
                let health = reader.u16()?;
                if health > 100 {
                    return Err(ProtocolError::InvalidPayload);
                }
                (Some(id), Some(health))
            } else {
                (None, None)
            };
            let killed = reader.boolean()?;
            if killed && !hit {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::ShotResult {
                target_id,
                remaining_health,
                killed,
            }
        }
        SERVER_HEALTH_UPDATE => {
            let health = reader.u16()?;
            if health > 100 {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::HealthUpdate(health)
        }
        SERVER_CHAT => ServerMessage::Chat {
            sender_id: reader.u64()?,
            sender_name: reader.name()?,
            text: reader.chat_text()?.to_owned(),
        },
        SERVER_SNAPSHOT => {
            let server_time_ms = reader.u64()?;
            let self_position = reader.vector()?;
            let self_velocity = reader.vector()?;
            let self_grounded = reader.boolean()?;
            let full_snapshot = reader.boolean()?;
            let count = reader.byte()? as usize;
            let removed_count = reader.byte()? as usize;
            if count > MAX_VISIBLE_PLAYERS
                || removed_count > MAX_VISIBLE_PLAYERS
                || (full_snapshot && removed_count > 0)
            {
                return Err(ProtocolError::InvalidPayload);
            }
            let mut players = RemotePlayers::default();
            for _ in 0..count {
                if !players.push(RemotePlayer {
                    id: reader.u64()?,
                    name: reader.shared_name()?,
                    position: reader.vector()?,
                }) {
                    return Err(ProtocolError::InvalidPayload);
                }
            }
            let mut removed_players = RemotePlayerIds::default();
            for _ in 0..removed_count {
                if !removed_players.push(reader.u64()?) {
                    return Err(ProtocolError::InvalidPayload);
                }
            }
            if players
                .iter()
                .any(|player| removed_players.contains(player.id))
            {
                return Err(ProtocolError::InvalidPayload);
            }
            ServerMessage::Snapshot {
                server_time_ms,
                full_snapshot,
                self_position,
                self_velocity,
                self_grounded,
                players,
                removed_players,
            }
        }
        other => return Err(ProtocolError::UnknownMessage(other)),
    };
    reader.finish()?;
    Ok(Some((message, frame_bytes)))
}

fn encode_frame(kind: u8, payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if payload.len() > MAX_PAYLOAD_BYTES || payload.len() > u16::MAX as usize {
        return Err(ProtocolError::PayloadTooLarge);
    }
    let mut frame = Vec::with_capacity(HEADER_BYTES + payload.len());
    frame.push(PROTOCOL_VERSION);
    frame.push(kind);
    frame.extend_from_slice(&(payload.len() as u16).to_le_bytes());
    frame.extend_from_slice(payload);
    Ok(frame)
}

fn decode_frame(input: &[u8]) -> Result<Option<(u8, &[u8], usize)>, ProtocolError> {
    if input.len() < HEADER_BYTES {
        return Ok(None);
    }
    if input[0] != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion(input[0]));
    }
    let payload_bytes = u16::from_le_bytes([input[2], input[3]]) as usize;
    if payload_bytes > MAX_PAYLOAD_BYTES {
        return Err(ProtocolError::PayloadTooLarge);
    }
    let frame_bytes = HEADER_BYTES + payload_bytes;
    if input.len() < frame_bytes {
        return Ok(None);
    }
    Ok(Some((
        input[1],
        &input[HEADER_BYTES..frame_bytes],
        frame_bytes,
    )))
}

fn write_name(name: &str, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if name.is_empty()
        || name.len() > 24
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(ProtocolError::InvalidName);
    }
    output.push(name.len() as u8);
    output.extend_from_slice(name.as_bytes());
    Ok(())
}

fn write_chat_text(text: &str, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if text.is_empty() || text.len() > MAX_CHAT_BYTES || text.chars().any(char::is_control) {
        return Err(ProtocolError::InvalidPayload);
    }
    output.push(text.len() as u8);
    output.extend_from_slice(text.as_bytes());
    Ok(())
}

fn write_vector(vector: [f32; 3], output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !vector.iter().all(|value| value.is_finite()) {
        return Err(ProtocolError::InvalidPayload);
    }
    for value in vector {
        output.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn write_direction(direction: [f32; 2], output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !direction
        .iter()
        .all(|value| value.is_finite() && (-1.0..=1.0).contains(value))
    {
        return Err(ProtocolError::InvalidPayload);
    }
    for value in direction {
        output.extend_from_slice(&value.to_le_bytes());
    }
    Ok(())
}

fn write_unit_direction(direction: [f32; 3], output: &mut Vec<u8>) -> Result<(), ProtocolError> {
    if !is_unit_direction(direction) {
        return Err(ProtocolError::InvalidPayload);
    }
    write_vector(direction, output)
}

fn is_unit_direction(direction: [f32; 3]) -> bool {
    if direction.iter().any(|value| !value.is_finite()) {
        return false;
    }
    let length_squared = direction.iter().map(|value| value * value).sum::<f32>();
    (0.99..=1.01).contains(&length_squared)
}

struct Reader<'a> {
    payload: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    fn new(payload: &'a [u8]) -> Self {
        Self { payload, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ProtocolError> {
        let end = self
            .offset
            .checked_add(count)
            .filter(|end| *end <= self.payload.len())
            .ok_or(ProtocolError::InvalidPayload)?;
        let bytes = &self.payload[self.offset..end];
        self.offset = end;
        Ok(bytes)
    }

    fn byte(&mut self) -> Result<u8, ProtocolError> {
        Ok(self.take(1)?[0])
    }

    fn boolean(&mut self) -> Result<bool, ProtocolError> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(ProtocolError::InvalidPayload),
        }
    }

    fn u64(&mut self) -> Result<u64, ProtocolError> {
        let bytes: [u8; 8] = self
            .take(8)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u64::from_le_bytes(bytes))
    }

    fn u16(&mut self) -> Result<u16, ProtocolError> {
        let bytes: [u8; 2] = self
            .take(2)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)?;
        Ok(u16::from_le_bytes(bytes))
    }

    fn vector(&mut self) -> Result<[f32; 3], ProtocolError> {
        let mut vector = [0.0; 3];
        for value in &mut vector {
            let bytes: [u8; 4] = self
                .take(4)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidPayload)?;
            *value = f32::from_le_bytes(bytes);
        }
        if !vector.iter().all(|value| value.is_finite()) {
            return Err(ProtocolError::InvalidPayload);
        }
        Ok(vector)
    }

    fn direction(&mut self) -> Result<[f32; 2], ProtocolError> {
        let mut direction = [0.0; 2];
        for value in &mut direction {
            let bytes: [u8; 4] = self
                .take(4)?
                .try_into()
                .map_err(|_| ProtocolError::InvalidPayload)?;
            *value = f32::from_le_bytes(bytes);
            if !value.is_finite() || !(-1.0..=1.0).contains(value) {
                return Err(ProtocolError::InvalidPayload);
            }
        }
        Ok(direction)
    }

    fn profile_id(&mut self) -> Result<[u8; PLAYER_PROFILE_ID_BYTES], ProtocolError> {
        self.take(PLAYER_PROFILE_ID_BYTES)?
            .try_into()
            .map_err(|_| ProtocolError::InvalidPayload)
    }

    fn name(&mut self) -> Result<String, ProtocolError> {
        Ok(self.name_text()?.to_owned())
    }

    fn shared_name(&mut self) -> Result<Arc<str>, ProtocolError> {
        Ok(Arc::from(self.name_text()?))
    }

    fn name_text(&mut self) -> Result<&'a str, ProtocolError> {
        let byte_count = self.byte()? as usize;
        let bytes = self.take(byte_count)?;
        let name = std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidName)?;
        if name.is_empty()
            || name.len() > 24
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        {
            return Err(ProtocolError::InvalidName);
        }
        Ok(name)
    }

    fn chat_text(&mut self) -> Result<&'a str, ProtocolError> {
        let byte_count = self.byte()? as usize;
        if byte_count == 0 || byte_count > MAX_CHAT_BYTES {
            return Err(ProtocolError::InvalidPayload);
        }
        let bytes = self.take(byte_count)?;
        let text = std::str::from_utf8(bytes).map_err(|_| ProtocolError::InvalidPayload)?;
        if text.chars().any(char::is_control) {
            return Err(ProtocolError::InvalidPayload);
        }
        Ok(text)
    }

    fn finish(&self) -> Result<(), ProtocolError> {
        if self.offset == self.payload.len() {
            Ok(())
        } else {
            Err(ProtocolError::InvalidPayload)
        }
    }
}
