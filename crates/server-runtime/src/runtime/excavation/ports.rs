//! Host adapters for the transaction owner; no port exposes ServerRuntime.
mod publication;
use super::{
    coordinator::{Origin, PreparationRequest},
    *,
};
use crate::runtime::impacts::{self, Impacts};
use minimium_world::impact::BodyState;

pub(super) fn terrain_inputs(
    worlds: &WorldRegistry,
    geography: &Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    world: WorldInstanceId,
) -> Option<minimium_procgen::TerrainInputs> {
    worlds
        .prepared_terrain_inputs(world, geography.as_ref())
        .ok()
}

pub(super) struct WorkSubmission<'a> {
    pool: &'a SectorGenerationPool,
    impacts: &'a mut Impacts,
}

impl<'a> WorkSubmission<'a> {
    pub(super) fn new(pool: &'a SectorGenerationPool, impacts: &'a mut Impacts) -> Self {
        Self { pool, impacts }
    }

    pub(super) fn submit(
        &mut self,
        bodies: &[BodyState],
        job: Box<dyn FnOnce() -> Result<Prepared, PreparationError> + Send>,
    ) -> io::Result<Receiver<Result<Prepared, PreparationError>>> {
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        self.impacts.begin_checkpoints(bodies)?;
        if let Err(error) = self.pool.prepare(Box::new(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(job))
                .unwrap_or(Err(ActionRejection::Capacity.into()));
            let _ = sender.send(result);
        })) {
            self.cancel(bodies);
            return Err(error);
        }
        Ok(receiver)
    }

    pub(super) fn cancel(&mut self, bodies: &[BodyState]) {
        for body in bodies {
            self.impacts.discard_pending_checkpoint(body);
        }
    }
}

pub(super) struct Publication<'a> {
    state: &'a mut ExcavationState,
    equipment: &'a mut crate::runtime::equipment::Equipment,
    worlds: &'a mut WorldRegistry,
    sectors: &'a mut minimium_sectors::SectorManager<SectorPayload>,
    impacts: &'a mut Impacts,
    components: &'a minimium_world::entity::ComponentRegistry,
    owner_authority: &'a crate::runtime::ComponentOwnerAuthority,
    geography: &'a Option<Arc<dyn minimium_procgen::PreparedGeography>>,
    logger: &'a minimium_diagnostics::DomainLogger,
    shutdown: &'a crate::composition::ShutdownRequest,
}

impl<'a> Publication<'a> {
    pub(super) fn new(
        state: &'a mut ExcavationState,
        equipment: &'a mut crate::runtime::equipment::Equipment,
        worlds: &'a mut WorldRegistry,
        sectors: &'a mut minimium_sectors::SectorManager<SectorPayload>,
        impacts: &'a mut Impacts,
        components: &'a minimium_world::entity::ComponentRegistry,
        owner_authority: &'a crate::runtime::ComponentOwnerAuthority,
        geography: &'a Option<Arc<dyn minimium_procgen::PreparedGeography>>,
        logger: &'a minimium_diagnostics::DomainLogger,
        shutdown: &'a crate::composition::ShutdownRequest,
    ) -> Self {
        Self {
            state,
            equipment,
            worlds,
            sectors,
            impacts,
            components,
            owner_authority,
            geography,
            logger,
            shutdown,
        }
    }

    fn inputs(&self, world: WorldInstanceId) -> Option<minimium_procgen::TerrainInputs> {
        terrain_inputs(self.worlds, self.geography, world)
    }

    pub(super) fn is_current(&self, request: &PreparationRequest) -> bool {
        !request
            .origin
            .player
            .zip(request.origin.context)
            .is_some_and(|(player, context)| {
                !self.worlds.world_input_context_is_current(player, context)
            })
            && self
                .inputs(request.world)
                .is_some_and(|inputs| inputs.revision() == request.input_revision)
    }

    pub(super) fn checkpoints_match(&self, bodies: &[BodyState]) -> bool {
        self.impacts.checkpoints_match(bodies)
    }

    pub(super) fn clear_pending_impacts(&mut self, bodies: &[BodyState]) {
        for body in bodies {
            self.impacts.discard_pending_checkpoint(body);
        }
    }

    pub(super) fn impact_failure(&mut self, reason: &str) {
        self.impacts
            .fail(self.logger, io::Error::other(reason.to_owned()));
    }

    pub(super) fn request_shutdown(&self) {
        self.shutdown.request();
    }

    pub(super) fn record_prepare(&mut self, elapsed: Duration) {
        self.state.metrics.prepare.record(elapsed);
    }

    pub(super) fn record_persistence(&mut self, elapsed: Duration) {
        self.state.metrics.persistence.record(elapsed);
    }

    pub(super) fn reserve_inventory(
        &mut self,
        plan: &WorldChangePlan,
    ) -> Result<(), ActionRejection> {
        self.equipment.reserve_plan(plan)
    }

    pub(super) fn cancel_inventory(&mut self, mutation_id: u64) {
        self.equipment.cancel_plan(mutation_id);
    }

    fn action_result(&mut self, origin: Origin, result: ActionResult) {
        if let (Some(player), Some(context)) = (origin.player, origin.context) {
            self.equipment.finish_action(player, context, result);
        }
        self.state.record_result(self.worlds, origin, result);
    }

    pub(super) fn reject_action(&mut self, origin: Origin, reason: ActionRejection) {
        if let Some(action_id) = origin.action_id {
            self.action_result(origin, ActionResult::Rejected { action_id, reason });
        }
    }

    pub(super) fn reject(&mut self, request: PreparationRequest, reason: ActionRejection) {
        self.reject_action(request.origin, reason);
        for body in request.impact_bodies {
            self.impacts.discard_pending_checkpoint(&body);
            if !matches!(reason, ActionRejection::Busy | ActionRejection::Stale) {
                self.impact_failure(&format!("impact geometry refused: {reason:?}"));
            }
        }
    }

    pub(super) fn report_geometry(&self, request: &PreparationRequest, failure: GeometryFailure) {
        crate::diagnostics::emit(
            self.logger,
            minimium_diagnostics::Level::Warn,
            "world.excavation_geometry_rejected",
            &[
                ("reason", format!("{:?}", failure.reason)),
                ("mutation_id", failure.mutation_id.to_string()),
                (
                    "action_id",
                    request
                        .origin
                        .action_id
                        .map_or_else(|| "none".to_owned(), |id| id.to_string()),
                ),
                (
                    "center_m",
                    format!(
                        "{:.1},{:.1},{:.1}",
                        failure.center_m[0], failure.center_m[1], failure.center_m[2]
                    ),
                ),
                ("chunks", format!("{:?}", failure.chunk_coordinates)),
                ("world", request.world.to_string()),
            ],
        );
    }
}
