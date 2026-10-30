use super::world_context::WorldContext;
use super::{limits::MAX_VISIBLE_PLAYERS, ServerErrorCode};
use minimium_core::world::{TransitionId, WorldManifest};
use minimium_core::{identity::PlayerProfileId, ids::PlayerId};
use std::{error::Error, fmt, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    Identify(PlayerProfileId),
    Ping,
    SetName(String),
    Move {
        direction: [f32; 2],
        jump: bool,
    },
    Fire {
        direction: [f32; 3],
    },
    Chat(String),
    WorldInput {
        context: WorldContext,
        input: ClientWorldInput,
    },
    TransitionReady {
        transition_id: TransitionId,
        target_context: WorldContext,
    },
    CancelTransition(TransitionId),
    UsePortal,
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientWorldInput {
    SetName(String),
    Move { direction: [f32; 2], jump: bool },
    Fire { direction: [f32; 3] },
    Chat(String),
    UsePortal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemotePlayer {
    pub id: PlayerId,
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

    pub fn get(&self, id: PlayerId) -> Option<&RemotePlayer> {
        self.iter().find(|player| player.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &RemotePlayer> {
        self.entries[..self.len].iter().filter_map(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemotePlayerIds {
    entries: [Option<PlayerId>; MAX_VISIBLE_PLAYERS],
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
    pub fn push(&mut self, id: PlayerId) -> bool {
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

    pub fn contains(&self, id: PlayerId) -> bool {
        self.iter().any(|candidate| *candidate == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &PlayerId> {
        self.entries[..self.len].iter().filter_map(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    Welcome {
        id: PlayerId,
        map_fingerprint: u64,
        health: u16,
        position: [f32; 3],
        context: WorldContext,
        manifest: WorldManifest,
        transition_id: Option<TransitionId>,
    },
    Pong,
    Accepted,
    Bye,
    Error(ServerErrorCode),
    ShotResult {
        target_id: Option<PlayerId>,
        remaining_health: Option<u16>,
        killed: bool,
    },
    HealthUpdate(u16),
    Chat {
        sender_id: PlayerId,
        sender_name: String,
        text: String,
    },
    Snapshot {
        server_time_ms: u64,
        context: WorldContext,
        full_snapshot: bool,
        self_position: [f32; 3],
        self_velocity: [f32; 3],
        self_grounded: bool,
        players: RemotePlayers,
        removed_players: RemotePlayerIds,
    },
    TransitionPrepare {
        transition_id: TransitionId,
        source_context: WorldContext,
        target_context: WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        manifest: WorldManifest,
    },
    TransitionCommitted {
        transition_id: TransitionId,
        context: WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        health: u16,
        manifest: WorldManifest,
    },
    TransitionCancelled {
        transition_id: TransitionId,
        context: WorldContext,
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
