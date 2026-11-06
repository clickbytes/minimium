use crate::{
    connection::{mark_disconnected, CloseReason},
    runtime::ServerRuntime,
};
use minimium_core::ids::PlayerId;
use minimium_protocol::{ClientMessage, ClientWorldInput};
use minimium_simulation::CharacterInput;

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
        let message = match message {
            ClientMessage::WorldInput { context, input } => {
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
                    .world_input_context_is_current(player, context)
                {
                    self.metrics.rejected_inputs = self.metrics.rejected_inputs.saturating_add(1);
                    return RoutingOutcome::Handled;
                }
                match input {
                    ClientWorldInput::SetName(name) => ClientMessage::SetName(name),
                    ClientWorldInput::Motion(input) => ClientMessage::Motion(input),
                    ClientWorldInput::Move { direction, jump } => {
                        ClientMessage::Move { direction, jump }
                    }
                    ClientWorldInput::Fire { direction } => ClientMessage::Fire { direction },
                    ClientWorldInput::Chat(text) => ClientMessage::Chat(text),
                    ClientWorldInput::UsePortal => unreachable!("portal branch handled earlier"),
                    ClientWorldInput::SetAltitude { .. } | ClientWorldInput::MapTeleport { .. } => {
                        unreachable!("world-control branch handled earlier")
                    }
                }
            }
            other => other,
        };
        let message = match message {
            ClientMessage::Move { direction, jump } => ClientMessage::Motion(CharacterInput {
                direction,
                jump,
                ..Default::default()
            }),
            other => other,
        };

        RoutingOutcome::ContinueWith(message)
    }
}
