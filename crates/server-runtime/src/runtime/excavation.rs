//! Bounded owner-lane coordinator: prepare, persist, then publish at a tick boundary.
mod admission;
mod coordinator;
mod ports;
use coordinator::{ExcavationCoordinator, Origin, PreparationRequest};
mod hydrology;
mod impacts;
mod inventory;
mod metrics;
pub use metrics::ExcavationStatus;
mod preparation;
mod publication;
mod replication;
mod scene;
mod streaming;
mod water;
use super::*;
use minimium_protocol::{ExcavationInput, ExcavationMessage, ServerMessage, WorldContext};
use minimium_storage::excavation::{ExcavationJournalPort, ExcavationRestore, WorldMutation};
use minimium_terrain::{TerrainEditRequest, TerrainStore};
use minimium_world::{
    items::{ActionRejection, ActionResult, Inventory, ToolUseIntent},
    world::WorldInstanceId,
};

pub(super) struct Excavation {
    coordinator: ExcavationCoordinator,
    state: ExcavationState,
}

struct ExcavationState {
    config: crate::composition::ExcavationConfig,
    async_work_policy: crate::AsyncWorkPolicy,
    restored: ExcavationRestore,
    inventories: HashMap<PlayerProfileId, Inventory>,
    inventory_requests: HashMap<PlayerId, (WorldContext, Inventory)>,
    action_results: HashMap<PlayerId, (WorldContext, ActionResult)>,
    gates: HashMap<PlayerId, (WorldContext, minimium_simulation::tools::ToolUseGate)>,
    clients: HashMap<PlayerId, streaming::Cursor>,
    metrics: metrics::ExcavationMetrics,
    waters: HashMap<WorldInstanceId, water::WaterWorld>,
    water_catalogs: HashMap<WorldInstanceId, [u8; 32]>,
    water_jobs: HashMap<WorldInstanceId, hydrology::WaterJob>,
    water_failures: HashMap<WorldInstanceId, hydrology::WaterFailure>,
    next_water_step: Instant,
    water_checkpoint_due: HashMap<WorldInstanceId, Instant>,
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

impl Excavation {
    pub(super) fn from_storage(
        journal: Box<dyn ExcavationJournalPort>,
        restored: ExcavationRestore,
        config: crate::composition::ExcavationConfig,
        async_work_policy: crate::AsyncWorkPolicy,
        now: Instant,
    ) -> io::Result<Self> {
        if !config.valid() {
            return Err(io::Error::other("invalid excavation configuration"));
        }
        Ok(Self {
            coordinator: ExcavationCoordinator::new(journal, restored.last_mutation_id),
            state: ExcavationState {
                config,
                async_work_policy,
                inventories: restored.inventories.iter().map(|(p, i)| (*p, *i)).collect(),
                restored,
                gates: HashMap::new(),
                inventory_requests: HashMap::new(),
                action_results: HashMap::new(),
                clients: HashMap::new(),
                metrics: Default::default(),
                waters: HashMap::new(),
                water_catalogs: HashMap::new(),
                water_jobs: HashMap::new(),
                water_failures: HashMap::new(),
                next_water_step: now,
                water_checkpoint_due: HashMap::new(),
            },
        })
    }

    fn busy(&self) -> bool {
        self.coordinator.busy()
    }

    fn allocate_id(&mut self) -> Option<u64> {
        self.coordinator.allocate_id()
    }
}

impl ServerRuntime {
    fn excavation_storage_rejection(&mut self, error: io::Error) -> ActionRejection {
        self.excavation.coordinator.storage_rejection(error)
    }

    pub(super) fn excavation_reserves(&self, world: WorldInstanceId) -> bool {
        self.excavation.coordinator.reserves(world)
    }

    pub(super) fn player_inventory(&self, player: PlayerId) -> Inventory {
        self.excavation_profile(player)
            .and_then(|p| self.excavation.state.inventories.get(&p).copied())
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
        ports::terrain_inputs(&self.game.worlds, &self.prepared_geography, world)
    }

    pub(super) fn excavation_result(
        &mut self,
        player: PlayerId,
        context: WorldContext,
        result: ActionResult,
    ) {
        self.excavation.state.record_result(
            &self.game.worlds,
            Origin {
                player: Some(player),
                context: Some(context),
                action_id: None,
            },
            result,
        );
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

impl ExcavationState {
    fn record_result(&mut self, worlds: &WorldRegistry, origin: Origin, result: ActionResult) {
        let (Some(player), Some(context)) = (origin.player, origin.context) else {
            return;
        };
        if let Some((current, gate)) = self.gates.get_mut(&player) {
            if *current == context {
                gate.finish(result);
            }
        }
        if worlds.context_for_player(player) == Some(context) {
            self.action_results.insert(player, (context, result));
        }
    }
}
