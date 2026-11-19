#[derive(Clone, Debug)]
pub struct CommandHelp {
    pub(crate) usage: &'static str,
    pub(crate) description: String,
    pub(crate) arguments: &'static [minimium_commands::CommandArgumentSpec],
}
impl CommandHelp {
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn usage(&self) -> &'static str {
        self.usage
    }
    pub fn arguments(&self) -> &'static [minimium_commands::CommandArgumentSpec] {
        self.arguments
    }
}
#[derive(Clone, Debug)]
pub struct PlayerRow {
    pub(crate) id: minimium_core::ids::PlayerId,
    pub(crate) name: String,
    pub(crate) world: minimium_world::world::WorldInstanceId,
    pub(crate) position: [f32; 3],
}
impl PlayerRow {
    pub fn id(&self) -> minimium_core::ids::PlayerId {
        self.id
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn world(&self) -> minimium_world::world::WorldInstanceId {
        self.world
    }
    pub fn position(&self) -> [f32; 3] {
        self.position
    }
}
#[derive(Clone, Debug)]
pub struct WorldRow {
    pub(crate) id: minimium_world::world::WorldInstanceId,
    pub(crate) players: usize,
    pub(crate) topology: minimium_world::world::WorldTopology,
    pub(crate) generation: minimium_world::world::GenerationRevision,
}
impl WorldRow {
    pub fn id(&self) -> minimium_world::world::WorldInstanceId {
        self.id
    }
    pub fn players(&self) -> usize {
        self.players
    }
    pub fn topology(&self) -> minimium_world::world::WorldTopology {
        self.topology
    }
    pub fn generation(&self) -> minimium_world::world::GenerationRevision {
        self.generation
    }
}
#[derive(Clone, Debug)]
pub struct WorldInspect {
    pub(crate) id: minimium_world::world::WorldInstanceId,
    pub(crate) players: usize,
    pub(crate) manifest: minimium_world::world::WorldManifest,
}
impl WorldInspect {
    pub fn id(&self) -> minimium_world::world::WorldInstanceId {
        self.id
    }
    pub fn players(&self) -> usize {
        self.players
    }
    pub fn manifest(&self) -> &minimium_world::world::WorldManifest {
        &self.manifest
    }
}
#[derive(Clone, Debug)]
pub struct EntityRow {
    pub(crate) world: minimium_world::world::WorldInstanceId,
    pub(crate) kind: EntityKind,
}
impl EntityRow {
    pub fn world(&self) -> minimium_world::world::WorldInstanceId {
        self.world
    }
    pub fn kind(&self) -> EntityKindView<'_> {
        match &self.kind {
            EntityKind::Entity(id) => EntityKindView::Entity(*id),
            EntityKind::Player(id, name) => EntityKindView::Player(*id, name),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EntityKindView<'a> {
    Entity(u64),
    Player(minimium_core::ids::PlayerId, &'a str),
}
#[derive(Clone, Debug)]
pub struct EntityComponentRow {
    pub(crate) kind: minimium_world::entity::ComponentTypeId,
    pub(crate) schema: minimium_world::entity::SchemaVersion,
    pub(crate) bytes: usize,
}
impl EntityComponentRow {
    pub fn kind(&self) -> minimium_world::entity::ComponentTypeId {
        self.kind
    }
    pub fn schema(&self) -> minimium_world::entity::SchemaVersion {
        self.schema
    }
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}
#[derive(Clone, Debug)]
pub(crate) enum EntityKind {
    Entity(u64),
    Player(minimium_core::ids::PlayerId, String),
}
#[derive(Clone, Debug)]
pub struct PropertyReport {
    pub(crate) key: String,
    pub(crate) value: minimium_commands::PropertyValue<String>,
}
impl PropertyReport {
    pub fn key(&self) -> &str {
        &self.key
    }
    pub fn value(&self) -> &minimium_commands::PropertyValue<String> {
        &self.value
    }
}
pub(crate) enum PropertyReportOutcome {
    Value(PropertyReport),
    Unknown(String),
    Unavailable { key: String, reason: String },
}
#[derive(Clone, Debug)]
pub struct ClientTrafficRow {
    pub(crate) id: minimium_core::ids::PlayerId,
    pub(crate) bytes_in: u64,
    pub(crate) bytes_out: u64,
    pub(crate) decoded_frames: u64,
    pub(crate) queued_frames: u64,
    pub(crate) encoding_us: u64,
    pub(crate) connected_ms: u128,
}
impl ClientTrafficRow {
    pub fn id(&self) -> minimium_core::ids::PlayerId {
        self.id
    }
    pub fn bytes_in(&self) -> u64 {
        self.bytes_in
    }
    pub fn bytes_out(&self) -> u64 {
        self.bytes_out
    }
    pub fn decoded_frames(&self) -> u64 {
        self.decoded_frames
    }
    pub fn queued_frames(&self) -> u64 {
        self.queued_frames
    }
    pub fn encoding_us(&self) -> u64 {
        self.encoding_us
    }
    pub fn connected_ms(&self) -> u128 {
        self.connected_ms
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct ClientTrafficSnapshot {
    pub rows: Vec<ClientTrafficRow>,
    pub truncated: bool,
}
#[derive(Clone, Debug)]
pub struct StatusReport {
    pub(crate) players: usize,
    pub(crate) lanes: usize,
    pub(crate) ticks: u64,
    pub(crate) clock_probe_requests: u64,
    pub(crate) clock_probe_accepted: u64,
    pub(crate) clock_probe_rejected: u64,
    pub(crate) environment_sync_messages: u64,
    pub(crate) scheduled_world_instances: usize,
    pub(crate) startup_system_runs: usize,
    pub(crate) persistence: minimium_storage::PersistenceStatus,
    pub(crate) runtime: Option<RuntimeStatusSnapshot>,
}
impl StatusReport {
    pub fn players(&self) -> usize {
        self.players
    }
    pub fn lanes(&self) -> usize {
        self.lanes
    }
    pub fn ticks(&self) -> u64 {
        self.ticks
    }
    pub fn clock_probe_requests(&self) -> u64 {
        self.clock_probe_requests
    }
    pub fn clock_probe_accepted(&self) -> u64 {
        self.clock_probe_accepted
    }
    pub fn clock_probe_rejected(&self) -> u64 {
        self.clock_probe_rejected
    }
    pub fn environment_sync_messages(&self) -> u64 {
        self.environment_sync_messages
    }
    pub fn scheduled_world_instances(&self) -> usize {
        self.scheduled_world_instances
    }
    pub fn startup_system_runs(&self) -> usize {
        self.startup_system_runs
    }
    pub fn persistence(&self) -> &minimium_storage::PersistenceStatus {
        &self.persistence
    }
    pub fn runtime(&self) -> Option<&RuntimeStatusSnapshot> {
        self.runtime.as_ref()
    }
}
#[derive(Clone, Debug)]
pub struct RuntimeStatusSnapshot {
    pub(crate) game_address: String,
    pub(crate) max_players: usize,
    pub(crate) tick_ms: u128,
    pub(crate) snapshot_ms: u128,
    pub(crate) autosave_seconds: u64,
    pub(crate) protocol_version: u32,
    pub(crate) save_version: u32,
    pub(crate) map_fingerprint: u64,
    pub(crate) environment_profiles: Vec<(u128, u64, minimium_environment::EnvironmentProfile)>,
    pub(crate) environment_profiles_truncated: bool,
    pub(crate) impact: crate::runtime::ImpactStatus,
    pub(crate) excavation: crate::runtime::ExcavationStatus,
}
impl RuntimeStatusSnapshot {
    pub fn game_address(&self) -> &str {
        &self.game_address
    }
    pub fn max_players(&self) -> usize {
        self.max_players
    }
    pub fn tick_ms(&self) -> u128 {
        self.tick_ms
    }
    pub fn snapshot_ms(&self) -> u128 {
        self.snapshot_ms
    }
    pub fn autosave_seconds(&self) -> u64 {
        self.autosave_seconds
    }
    pub fn protocol_version(&self) -> u32 {
        self.protocol_version
    }
    pub fn save_version(&self) -> u32 {
        self.save_version
    }
    pub fn map_fingerprint(&self) -> u64 {
        self.map_fingerprint
    }
    pub fn environment_profiles(&self) -> &[(u128, u64, minimium_environment::EnvironmentProfile)] {
        &self.environment_profiles
    }
    pub fn environment_profiles_truncated(&self) -> bool {
        self.environment_profiles_truncated
    }
    pub fn impact(&self) -> &crate::runtime::ImpactStatus {
        &self.impact
    }
    pub fn excavation(&self) -> &crate::runtime::ExcavationStatus {
        &self.excavation
    }
}
