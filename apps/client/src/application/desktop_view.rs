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

pub(crate) struct PendingRenderView<'a> {
    pub(crate) ticket: TransferTicket,
    pub(crate) transition: &'a PendingSectorTransition,
    pub(crate) terrain_inputs: minimium_procgen::TerrainInputs,
}

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
    pub(crate) fn desktop_render_view(&self) -> DesktopRenderView<'_> {
        let client = self.core_view();
        let context = client.context;
        let inventory = self.session().map(|session| session.inventory());
        let equipped_tool = inventory.as_ref().and_then(|inventory| inventory.tool());
        let pending = self
            .transitions
            .pending_preparation()
            .and_then(|(id, transition)| {
                let ticket = TransferTicket::new(id, transition.context);
                if self.connection_view().host_readiness_requested(ticket) {
                    return None;
                }
                let terrain_inputs = self
                    .session()?
                    .terrain_inputs_for_context(transition.context)?;
                Some(PendingRenderView {
                    ticket,
                    transition,
                    terrain_inputs,
                })
            });
        let tool_profile = self.core.tool_profile();
        DesktopRenderView {
            client,
            context,
            world_catalog: self.world_catalog_view(),
            equipped_tool,
            equipped_tool_model_index: equipped_tool
                .and_then(minimium_game_sample::tool_model_index),
            equipped_tool_profile: equipped_tool.and_then(|tool| tool_profile.definition(tool)),
            max_tool_reach_m: tool_profile.max_reach_m(),
            max_tool_radius_m: tool_profile.max_radius_m(),
            has_tools: inventory
                .as_ref()
                .is_some_and(|inventory| inventory.slots.iter().any(Option::is_some)),
            pending,
        }
    }

    pub(crate) fn report_transfer_presentation_ready(
        &mut self,
        report: TransferPresentationReady,
        now: minimium_client_runtime::ClientTime,
    ) -> bool {
        if self.core.validate_time(now).is_err() {
            return false;
        }
        let ticket = report.ticket;
        if !self
            .transitions
            .contains(ticket.transition_id(), ticket.context())
            || self.connection_view().host_readiness_requested(ticket)
            || !self.core.transfer_cpu_content_ready(ticket)
        {
            return false;
        }
        self.core
            .report_transfer_host_ready(ticket, report.snapshot, now)
            .is_ok()
    }
}
