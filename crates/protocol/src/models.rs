use super::world_context::WorldContext;
use super::{limits::MAX_VISIBLE_PLAYERS, ServerErrorCode};
use minimium_core::ids::PlayerId;
use minimium_environment::{time::ClockSample, EnvironmentDescriptor};
use minimium_world::entity::{ComponentTypeId, EntityId, SchemaVersion};
use minimium_world::world::{Position3Mm, TransitionId, WorldManifest};
use std::{error::Error, fmt, sync::Arc};

#[derive(Debug, Clone, PartialEq)]
pub enum ClientMessage {
    /// None means no presentation; Some selects compressed or fallback textures.
    Presentation(Option<bool>),
    ContentReady([u8; 32]),
    Authenticate {
        provider_id: String,
        credentials: Vec<u8>,
    },
    Ping,
    SkyView {
        context: WorldContext,
        direction: [f32; 3],
        night: bool,
    },
    WorldInput {
        context: WorldContext,
        input: ClientWorldInput,
    },
    TransitionReady {
        transition_id: TransitionId,
        target_context: WorldContext,
    },
    CancelTransition(TransitionId),
    TimeProbe {
        seq: u32,
        client_send_monotonic_ms: u64,
    },
    ReplicationAck {
        context: WorldContext,
        sync_id: u64,
        revision: u64,
        accepted: bool,
    },
    Quit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ClientWorldInput {
    SetName(String),
    Motion(MovementSample),
    Fire { direction: [f32; 3] },
    MapTeleport { direction: [f32; 3] },
    SetAltitude { km: f32 },
    SetMovementPolicy { speed: f32, accel: f32 },
    SetFlightEnabled(bool),
    Chat(String),
    InteractEntity { entity: EntityId },
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntityReplicationOperation {
    Spawn {
        entity: EntityId,
    },
    ComponentUpsert {
        entity: EntityId,
        kind: ComponentTypeId,
        schema: SchemaVersion,
        bytes: Vec<u8>,
    },
    ComponentRemove {
        entity: EntityId,
        kind: ComponentTypeId,
    },
    Despawn {
        entity: EntityId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MovementSample {
    pub input_seq: u64,
    pub target_tick: u64,
    pub intent: minimium_simulation::MovementIntent,
}

impl MovementSample {
    pub const fn new(intent: minimium_simulation::MovementIntent) -> Self {
        Self {
            input_seq: 0,
            target_tick: 0,
            intent,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemotePlayer {
    pub id: PlayerId,
    pub name: Arc<str>,
    pub position: [f32; 3],
    pub position_mm: Position3Mm,
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
    Bolid {
        context: WorldContext,
        event: BolidEvent,
    },
    MovementPolicy(minimium_simulation::MovementPolicy),
    EntitySyncBegin {
        context: WorldContext,
        sync_id: u64,
        base_revision: u64,
        target_revision: u64,
        full: bool,
    },
    EntitySyncRecord {
        context: WorldContext,
        sync_id: u64,
        revision: u64,
        operation: EntityReplicationOperation,
    },
    EntitySyncComplete {
        context: WorldContext,
        sync_id: u64,
        target_revision: u64,
    },
    ContentOffer {
        material_hash: [u8; 32],
        http_port: u16,
        climate: minimium_environment::climate::ClimateScenario,
    },
    Welcome {
        server_tick: u64,
        id: PlayerId,
        map_fingerprint: u64,
        health: u16,
        position: [f32; 3],
        position_mm: Position3Mm,
        context: WorldContext,
        manifest: WorldManifest,
        transition_id: Option<TransitionId>,
        clock_sample: ClockSample,
        environment: EnvironmentDescriptor,
    },
    Pong,
    MapTeleportResult {
        position_mm: Option<Position3Mm>,
    },
    AltitudeResult {
        context: WorldContext,
        position_mm: Option<Position3Mm>,
    },
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
        server_tick: u64,
        resolved_input_seq: u64,
        context: WorldContext,
        full_snapshot: bool,
        self_movement: minimium_simulation::MovementIntent,
        self_position: [f32; 3],
        self_position_mm: Position3Mm,
        self_velocity: [f32; 3],
        self_grounded: bool,
        players: Box<RemotePlayers>,
        removed_players: RemotePlayerIds,
    },
    TransitionPrepare {
        transition_id: TransitionId,
        source_context: WorldContext,
        target_context: WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        destination_mm: Position3Mm,
        manifest: WorldManifest,
        clock_sample: ClockSample,
        environment: EnvironmentDescriptor,
    },
    TransitionCommitted {
        transition_id: TransitionId,
        server_tick: u64,
        context: WorldContext,
        map_fingerprint: u64,
        destination: [f32; 3],
        destination_mm: Position3Mm,
        health: u16,
        manifest: WorldManifest,
        clock_sample: ClockSample,
        environment: EnvironmentDescriptor,
    },
    TransitionCancelled {
        transition_id: TransitionId,
        context: WorldContext,
    },
    EnvironmentSync {
        context: WorldContext,
        clock_sample: ClockSample,
        environment: EnvironmentDescriptor,
    },
    TimeProbeReply {
        seq: u32,
        echoed_client_send_monotonic_ms: u64,
        server_receive_monotonic_ms: u64,
        server_send_monotonic_ms: u64,
        session_nonce: minimium_environment::time::SessionNonce,
    },
}

/// A transient, server-timed atmospheric streak in world coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BolidEvent {
    pub id: u64,
    pub start_monotonic_ms: u64,
    pub seed: u64,
    pub start_world_m: [f32; 3],
    pub end_world_m: [f32; 3],
    pub duration_ms: u16,
    pub kind: u8,
}

impl BolidEvent {
    pub fn valid(self) -> bool {
        self.id != 0
            && (300..=6000).contains(&self.duration_ms)
            && self.kind <= 2
            && self
                .start_world_m
                .iter()
                .chain(self.end_world_m.iter())
                .all(|x| x.is_finite() && x.abs() <= 10_000_000.0)
            && self
                .start_world_m
                .iter()
                .zip(self.end_world_m)
                .map(|(a, b)| (*a - b).powi(2))
                .sum::<f32>()
                > 1.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProtocolError {
    UnsupportedVersion(u8),
    UnknownMessage(u8),
    PayloadTooLarge,
    InvalidPayload,
    InvalidName,
    UnsupportedEnvironment,
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
            Self::UnsupportedEnvironment => {
                formatter.write_str("peer environment model or catalogue is unsupported")
            }
        }
    }
}

impl Error for ProtocolError {}
