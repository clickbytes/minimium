use super::*;
use minimium_protocol::{ClientWorldInput, ExcavationInput};
use minimium_world::items::{ActionResult, Inventory};

#[derive(Clone, Copy, Debug)]
pub struct WaterReplicaSummary {
    pub context: WorldContext,
    pub sync_id: Option<u64>,
    pub fluid_revision: u64,
    pub terrain_version: Option<minimium_procgen::SectorContentVersion>,
    pub catalog_digest: Option<[u8; 32]>,
    pub region: Option<([i32; 3], [i32; 3])>,
}

#[derive(Default)]
pub(super) struct ToolClientState {
    next_id: u64,
    pending: Option<(WorldContext, u64)>,
    last_use: Option<u64>,
}

impl ClientSession {
    pub fn terrain_inputs(&self) -> Option<minimium_procgen::TerrainInputs> {
        let active = self
            .active_world_content
            .as_ref()
            .filter(|content| Some(content.context) == self.world_context)?;
        let instance = active.context.instance();
        let mut edits = None;
        let mut remnants = None;
        let mut volumes = None;
        if self.impacts.context == self.world_context && self.impacts.ready {
            remnants = self
                .impacts
                .remnants_for_craters(self.excavation.craters().as_ref());
        }
        if self.excavation_ready() {
            volumes = self.excavation.terrain();
            edits = self.excavation.craters();
        }
        let overlays =
            minimium_procgen::TerrainOverlaySnapshot::new(instance, edits, remnants, volumes);
        minimium_procgen::TerrainInputs::new(active.content.prepared_terrain().clone(), overlays)
            .ok()
    }

    pub fn terrain_inputs_for_context(
        &self,
        context: WorldContext,
    ) -> Option<minimium_procgen::TerrainInputs> {
        if self.shares_active_world_state(context) {
            return self.terrain_inputs();
        }
        let pending = self
            .pending_world_content
            .as_ref()
            .filter(|pending| pending.context == context)?;
        let overlays = minimium_procgen::TerrainOverlaySnapshot::empty_for(context.instance());
        minimium_procgen::TerrainInputs::new(pending.content.prepared_terrain().clone(), overlays)
            .ok()
    }

    pub fn excavation_ready(&self) -> bool {
        self.world_context.is_some_and(|c| self.excavation.ready(c))
    }

    pub fn water_read(&self) -> Option<std::sync::Arc<dyn minimium_world::fluid::WaterRead>> {
        let snapshot = self.excavation.fluids()?;
        let mut replica = crate::WaterReplica::new(snapshot);
        replica.install_region(self.excavation.fluid_region());
        if let Some(catalog) = self.water_catalog() {
            replica.install_catalog(catalog);
        }
        replica.as_read()
    }

    pub fn fluid_snapshot(&self) -> Option<std::sync::Arc<minimium_fluids::FluidSnapshot>> {
        self.excavation.fluids()
    }

    pub fn water_replica_summary(&self) -> Option<WaterReplicaSummary> {
        let context = self.active_world_context()?;
        let snapshot = self.excavation.fluids()?;
        if snapshot.world() != context.instance()
            || self.excavation.fluid_context() != Some(context)
        {
            return None;
        }
        Some(WaterReplicaSummary {
            context,
            sync_id: self.excavation.published_fluid_sync_id(),
            fluid_revision: snapshot.revision(),
            terrain_version: self
                .terrain_inputs_for_context(context)
                .map(|inputs| inputs.revision()),
            catalog_digest: self
                .water_catalog_for_context(context)
                .map(|catalog| catalog.digest),
            region: self.excavation.fluid_region(),
        })
    }

    pub fn water_region_covers_capsule(
        &self,
        feet_m: [f64; 3],
        up: [f64; 3],
        height_m: f64,
    ) -> bool {
        self.excavation
            .water_region_covers_capsule(feet_m, up, height_m)
    }

    pub fn water_catalog(&self) -> Option<std::sync::Arc<minimium_procgen::WaterCatalog>> {
        self.water_catalog_for_context(self.world_context?)
    }

    pub fn water_catalog_for_context(
        &self,
        context: WorldContext,
    ) -> Option<std::sync::Arc<minimium_procgen::WaterCatalog>> {
        self.active_world_content
            .as_ref()
            .filter(|bundle| bundle.context == context)
            .and_then(|bundle| bundle.water_catalog.clone())
            .or_else(|| {
                self.pending_world_content
                    .as_ref()
                    .filter(|bundle| bundle.context == context)
                    .and_then(|bundle| bundle.water_catalog.clone())
            })
    }

    pub fn inventory(&self) -> Inventory {
        self.excavation.inventory()
    }

    pub fn tool_action_pending(&self) -> bool {
        self.tools
            .pending
            .is_some_and(|(context, _)| Some(context) == self.world_context)
    }

    pub fn queue_tool_use(
        &mut self,
        direction: [f32; 3],
        now: ClientTime,
    ) -> Result<bool, QueueError> {
        self.accept_time(now).map_err(QueueError::Time)?;
        let context = self.world_context.ok_or(QueueError::AwaitingWelcome)?;
        if self.pending_transition.is_some() || !self.excavation_ready() {
            return Ok(false);
        }
        if self.tools.pending.is_some_and(|(c, _)| c != context) {
            self.tools.pending = None;
        }
        let game_ms = self
            .clock_sample
            .map_or(0, |s| s.boundary().simulation_ms());
        if self.clock_sample.is_some_and(|s| s.rate_ppm().get() == 0)
            || self.tools.pending.is_some()
            || self
                .tools
                .last_use
                .is_some_and(|t| game_ms.saturating_sub(t) < 500)
        {
            return Ok(false);
        }
        let action_id = self
            .tools
            .next_id
            .checked_add(1)
            .ok_or(QueueError::InputSequenceExhausted)?;
        self.queue_world_input(
            ClientWorldInput::Excavation(ExcavationInput::Use {
                action_id,
                direction,
            }),
            now,
        )?;
        self.tools.next_id = action_id;
        self.tools.pending = Some((context, action_id));
        self.tools.last_use = Some(game_ms);
        Ok(true)
    }

    pub(crate) fn receive_tool_result(&mut self, context: WorldContext, result: ActionResult) {
        let id = match result {
            ActionResult::Committed { action_id, .. }
            | ActionResult::Rejected { action_id, .. } => action_id,
        };
        if self.tools.pending == Some((context, id)) {
            self.tools.pending = None;
        }
    }
}
