use crate::{
    connection::{mark_disconnected, CloseReason},
    runtime::ServerRuntime,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::ClientMessage;

pub(super) enum RoutingOutcome {
    Handled,
    ContinueWith(ClientMessage),
}

impl ServerRuntime {
    pub(super) fn route_message(
        &mut self,
        index: usize,
        message: ClientMessage,
        player_id: Option<PlayerId>,
    ) -> RoutingOutcome {
        if let ClientMessage::WorldInput { context, .. } = &message {
            let Some(player) = player_id else {
                mark_disconnected(
                    index,
                    &mut self.connections.clients[index],
                    &mut self.connections.disconnected,
                    Some(CloseReason::InvalidSequence),
                );
                return RoutingOutcome::Handled;
            };
            if !self
                .game
                .worlds
                .world_input_context_is_current(player, *context)
            {
                self.metrics.rejected_inputs = self.metrics.rejected_inputs.saturating_add(1);
                return RoutingOutcome::Handled;
            }
        }

        RoutingOutcome::ContinueWith(message)
    }
}
