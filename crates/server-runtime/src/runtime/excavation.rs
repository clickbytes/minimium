//! Bounded owner-lane coordinator: prepare, persist, then publish at a tick boundary.
mod admission;
mod hydrology;
mod impacts;
mod inventory;
mod metrics;
pub(crate) use metrics::ExcavationStatus;
mod preparation;
mod publication;
mod replication;
mod scene;
mod streaming;
mod water;
use super::*;
use minimium_protocol::{ExcavationInput, ExcavationMessage, ServerMessage, WorldContext};
use minimium_storage::excavation::{
    ExcavationJournal, ExcavationRestore, WorldMutation, JOURNAL_FILE_NAME,
};
use minimium_terrain::{TerrainEditRequest, TerrainStore};
use minimium_world::{
    items::{ActionRejection, ActionResult, Inventory, ToolUseIntent},
    world::WorldInstanceId,
};

pub(super) struct Excavation {
    config: crate::composition::ExcavationConfig,
    journal: ExcavationJournal,
    restored: ExcavationRestore,
    inventories: HashMap<PlayerProfileId, Inventory>,
    inventory_requests: HashMap<PlayerId, (WorldContext, Inventory)>,
    action_results: HashMap<PlayerId, (WorldContext, ActionResult)>,
    gates: HashMap<PlayerId, (WorldContext, minimium_simulation::tools::ToolUseGate)>,
    preparing: Option<Preparation>,
    pending: Option<Pending>,
    next_id: u64,
    failure: Option<String>,
    clients: HashMap<PlayerId, streaming::Cursor>,
    metrics: metrics::ExcavationMetrics,
    waters: HashMap<WorldInstanceId, water::WaterWorld>,
    water_catalogs: HashMap<WorldInstanceId, [u8; 32]>,
    water_jobs: HashMap<WorldInstanceId, hydrology::WaterJob>,
    water_failures: HashMap<WorldInstanceId, hydrology::WaterFailure>,
    next_water_step: Instant,
    water_checkpoint_due: HashMap<WorldInstanceId, Instant>,
    shutting_down: bool,
}

struct Preparation {
    player: Option<PlayerId>,
    context: Option<WorldContext>,
    action_id: Option<u64>,
    world: WorldInstanceId,
    impact_bodies: Vec<minimium_world::impact::BodyState>,
    input_revision: minimium_procgen::SectorContentVersion,
    receiver: Receiver<Result<Prepared, PreparationError>>,
    started: Instant,
}

#[derive(Debug)]
pub(super) enum PreparationError {
    Rejected(ActionRejection),
    Geometry(GeometryFailure),
}

#[derive(Debug)]
pub(super) struct GeometryFailure {
    pub reason: minimium_procgen::ExcavationGeometryError,
    pub mutation_id: u64,
    pub center_m: [f64; 3],
    pub chunk_coordinates: Vec<[i32; 3]>,
}

impl From<ActionRejection> for PreparationError {
    fn from(reason: ActionRejection) -> Self {
        Self::Rejected(reason)
    }
}

struct Prepared {
    mutation: WorldMutation,
    terrain: Option<Arc<TerrainStore>>,
    sectors: std::collections::BTreeMap<minimium_world::world::SectorKey, Arc<SectorPayload>>,
    collision_elapsed: Duration,
    water: Option<water::WaterWorld>,
    craters: Option<Arc<minimium_terrain::TerrainSnapshot>>,
}

struct Pending {
    player: Option<PlayerId>,
    context: Option<WorldContext>,
    action_id: Option<u64>,
    prepared: Prepared,
    started: Instant,
}

impl Excavation {
    pub(super) fn soil_depth_m(&self) -> f64 {
        self.config.soil_depth_m
    }

    pub(super) fn open(
        path: &std::path::Path,
        config: crate::composition::ExcavationConfig,
    ) -> io::Result<Self> {
        if !config.valid() {
            return Err(io::Error::other("invalid excavation configuration"));
        }
        let (journal, restored) = ExcavationJournal::open(&path.join(JOURNAL_FILE_NAME))?;
        Ok(Self {
            config,
            journal,
            inventories: restored.inventories.iter().map(|(p, i)| (*p, *i)).collect(),
            next_id: restored.last_mutation_id,
            restored,
            gates: HashMap::new(),
            inventory_requests: HashMap::new(),
            action_results: HashMap::new(),
            preparing: None,
            pending: None,
            failure: None,
            clients: HashMap::new(),
            metrics: Default::default(),
            waters: HashMap::new(),
            water_catalogs: HashMap::new(),
            water_jobs: HashMap::new(),
            water_failures: HashMap::new(),
            next_water_step: Instant::now(),
            water_checkpoint_due: HashMap::new(),
            shutting_down: false,
        })
    }

    fn busy(&self) -> bool {
        self.preparing.is_some() || self.pending.is_some()
    }

    fn allocate_id(&mut self) -> Option<u64> {
        self.next_id = self.next_id.checked_add(1)?;
        Some(self.next_id)
    }
}

impl ServerRuntime {
    fn excavation_storage_rejection(&mut self, error: io::Error) -> ActionRejection {
        match error.kind() {
            io::ErrorKind::InvalidInput => ActionRejection::Capacity,
            io::ErrorKind::WouldBlock => ActionRejection::Busy,
            _ => {
                self.excavation.failure = Some(error.to_string());
                ActionRejection::Storage
            }
        }
    }

    pub(super) fn excavation_reserves(&self, world: WorldInstanceId) -> bool {
        self.excavation
            .preparing
            .as_ref()
            .is_some_and(|p| p.world == world)
            || self
                .excavation
                .pending
                .as_ref()
                .is_some_and(|p| p.prepared.mutation.world == world)
    }

    pub(super) fn player_inventory(&self, player: PlayerId) -> Inventory {
        self.excavation_profile(player)
            .and_then(|p| self.excavation.inventories.get(&p).copied())
            .unwrap_or_default()
    }

    fn excavation_profile(&self, player: PlayerId) -> Option<PlayerProfileId> {
        self.connections.clients.iter().find_map(|client| {
            client
                .identity()
                .filter(|id| id.player == player)
                .map(|id| id.principal.profile())
        })
    }

    fn excavation_inputs(&self, world: WorldInstanceId) -> Option<minimium_procgen::TerrainInputs> {
        let owner = self.game.worlds.get(world)?;
        Some(minimium_procgen::TerrainInputs {
            geography: self.prepared_geography.clone(),
            edits: Some(owner.terrain.clone()),
            remnants: Some(owner.remnants.clone()),
            volumes: owner.volumes.clone(),
        })
    }

    pub(super) fn excavation_result(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        result: ActionResult,
    ) {
        if let Some((c, gate)) = self.excavation.gates.get_mut(&player) {
            if *c == context {
                gate.finish(result);
            }
        }
        if self.game.worlds.context_for_player(player) == Some(context) {
            // One accepted action per player means one bounded response slot.
            // Streaming retries it until the control frame enters the socket queue.
            self.excavation
                .action_results
                .insert(player, (context, result));
        }
    }

    fn send_excavation(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        message: ExcavationMessage,
    ) {
        if self.game.worlds.context_for_player(player) != Some(context) {
            return;
        }
        if let Some(client) = self
            .connections
            .clients
            .iter_mut()
            .find(|c| c.player_id() == Some(player))
        {
            crate::connection::queue_message(
                client,
                &mut self.metrics,
                ServerMessage::Excavation { context, message },
            );
        }
    }
}
