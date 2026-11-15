use super::*;
use minimium_protocol::{ClientWorldInput, ExcavationInput};
use minimium_world::items::{ActionResult, Inventory};

#[derive(Default)]
pub(super) struct ToolClientState {
    next_id: u64,
    pending: Option<(WorldContext, u64)>,
    last_use: Option<Instant>,
}

impl ClientSession {
    pub fn terrain_inputs(&self) -> minimium_procgen::TerrainInputs {
        let mut inputs = minimium_procgen::TerrainInputs::from(self.prepared_geography());
        if self.impacts.context == self.world_context && self.impacts.ready {
            inputs.remnants = self
                .impacts
                .remnants_for_craters(self.excavation.craters().as_ref());
        }
        if self.excavation_ready() {
            inputs.volumes = self.excavation.terrain();
            inputs.edits = self.excavation.craters();
        }
        inputs
    }

    pub fn terrain_inputs_for_context(
        &self,
        context: WorldContext,
    ) -> Option<minimium_procgen::TerrainInputs> {
        if self.shares_active_world_state(context) {
            return Some(self.terrain_inputs());
        }
        let pending = self
            .pending_world_content
            .as_ref()
            .filter(|pending| pending.context == context)?;
        Some(minimium_procgen::TerrainInputs::from(
            pending.content.geography.clone(),
        ))
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

    pub fn equip_tool(&mut self, slot: Option<u8>) -> Result<(), QueueError> {
        self.queue_world_input(ClientWorldInput::Excavation(ExcavationInput::Equip {
            slot,
        }))?;
        Ok(())
    }

    pub fn queue_tool_use(&mut self, direction: [f32; 3]) -> Result<bool, QueueError> {
        let context = self.world_context.ok_or(QueueError::AwaitingWelcome)?;
        if self.pending_transition.is_some() || !self.excavation_ready() {
            return Ok(false);
        }
        if self.tools.pending.is_some_and(|(c, _)| c != context) {
            self.tools.pending = None;
        }
        if self.tools.pending.is_some()
            || self
                .tools
                .last_use
                .is_some_and(|t| t.elapsed() < Duration::from_millis(500))
        {
            return Ok(false);
        }
        let action_id = self
            .tools
            .next_id
            .checked_add(1)
            .ok_or(QueueError::InputSequenceExhausted)?;
        self.queue_world_input(ClientWorldInput::Excavation(ExcavationInput::Use {
            action_id,
            direction,
        }))?;
        self.tools.next_id = action_id;
        self.tools.pending = Some((context, action_id));
        self.tools.last_use = Some(Instant::now());
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
