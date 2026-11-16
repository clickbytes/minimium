use super::{ClientApplication, PendingSectorTransition};
use minimium_client_runtime::{ClientView, TransferTicket};
use minimium_protocol::WorldContext;
use minimium_world::world::{WorldInstanceId, WorldMap};
use std::collections::HashMap;

/// Read-only CPU inputs prepared by the application for desktop presentation.
pub(crate) struct DesktopRenderView<'a> {
    pub(crate) client: ClientView<'a>,
    pub(crate) context: Option<WorldContext>,
    pub(crate) world_catalog: &'a HashMap<WorldInstanceId, WorldMap>,
    pub(crate) equipped_tool: Option<minimium_world::items::ToolKind>,
    pub(crate) equipped_tool_model_index: Option<usize>,
    pub(crate) equipped_tool_profile: Option<minimium_simulation::tools::ToolDefinition>,
    pub(crate) max_tool_reach_m: f64,
    pub(crate) max_tool_radius_m: f64,
    pub(crate) has_tools: bool,
    pub(crate) pending: Option<PendingRenderView<'a>>,
}

/// Bounded context for preparing the one pending destination's presentation.
pub(crate) struct PendingRenderView<'a> {
    pub(crate) ticket: TransferTicket,
    pub(crate) transition: &'a PendingSectorTransition,
    pub(crate) terrain_inputs: minimium_procgen::TerrainInputs,
}

/// Presentation acknowledgement is tied to the exact CPU snapshot and transfer.
#[derive(Clone)]
pub(crate) struct TransferPresentationReady {
    ticket: TransferTicket,
    snapshot: minimium_client_runtime::world_readiness::CollisionSnapshot,
}

impl TransferPresentationReady {
    pub(crate) fn new(
        ticket: TransferTicket,
        snapshot: minimium_client_runtime::world_readiness::CollisionSnapshot,
    ) -> Self {
        Self { ticket, snapshot }
    }
}

impl ClientApplication {
    /// Active and pending content comes from its ticket-bound session bundle.
    /// Bootstrap Earth remains pending until its geography source is prepared.
    pub(crate) fn scene_terrain_inputs(
        &self,
    ) -> Result<Option<minimium_procgen::TerrainInputs>, minimium_procgen::GenerationError> {
        if let Some(session) = self.connection.session() {
            return Ok(session.terrain_inputs());
        }
        let instance = self.game.current_instance;
        let manifest = self
            .game
            .world_manifests
            .get(&instance)
            .copied()
            .ok_or_else(|| {
                minimium_procgen::GenerationError::invalid_manifest(
                    "bootstrap world has no manifest",
                )
            })?;
        let map = self.world_catalog.get(&instance).ok_or_else(|| {
            minimium_procgen::GenerationError::invalid_manifest(
                "bootstrap world has no resolved map",
            )
        })?;
        match crate::world_content::bootstrap_terrain_inputs(instance, manifest, map) {
            Ok(inputs) => Ok(Some(inputs)),
            Err(error)
                if error.kind()
                    == minimium_procgen::GenerationErrorKind::Sampling(
                        minimium_procgen::EarthSampleError::PackageUnavailable,
                    ) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }

    pub(crate) fn desktop_render_view(&self) -> DesktopRenderView<'_> {
        let context = self.game.session_context();
        let inventory = self.connection.session().map(|session| session.inventory());
        let equipped_tool = inventory.as_ref().and_then(|inventory| inventory.tool());
        let pending = self
            .transitions
            .pending_preparation()
            .and_then(|(id, transition)| {
                let ticket = TransferTicket::new(id, transition.context);
                if self.connection.host_readiness_requested(ticket) {
                    return None;
                }
                let terrain_inputs = self
                    .connection
                    .session()?
                    .terrain_inputs_for_context(transition.context)?;
                Some(PendingRenderView {
                    ticket,
                    transition,
                    terrain_inputs,
                })
            });
        DesktopRenderView {
            client: self.game.view(),
            context,
            world_catalog: &self.world_catalog,
            equipped_tool,
            equipped_tool_model_index: equipped_tool
                .and_then(minimium_game_sample::tool_model_index),
            equipped_tool_profile: equipped_tool
                .and_then(|tool| self.tool_profile.definition(tool)),
            max_tool_reach_m: self.tool_profile.max_reach_m(),
            max_tool_radius_m: self.tool_profile.max_radius_m(),
            has_tools: inventory
                .as_ref()
                .is_some_and(|inventory| inventory.slots.iter().any(Option::is_some)),
            pending,
        }
    }

    /// Accept presentation readiness only while the exact pending content is current.
    pub(crate) fn report_transfer_presentation_ready(
        &mut self,
        report: TransferPresentationReady,
        now: minimium_client_runtime::ClientTime,
    ) -> bool {
        if self.validate_time(now).is_err() {
            return false;
        }
        let ticket = report.ticket;
        if !self
            .transitions
            .contains(ticket.transition_id(), ticket.context())
            || self.connection.host_readiness_requested(ticket)
            || !self.transfer_cpu_content_ready(ticket)
        {
            return false;
        }
        self.report_transfer_host_ready(ticket, report.snapshot, now)
            .is_ok()
    }
}
