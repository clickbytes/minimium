use crate::{connection::queue_message, runtime::ServerRuntime};
use minimium_protocol::{
    ClientMessage, ClientWorldInput, PlayerActionDecision, PlayerActionKind, PlayerActionReceipt,
    ServerMessage,
};

pub(super) fn send(
    runtime: &mut ServerRuntime,
    index: usize,
    message: &ClientMessage,
    decision: PlayerActionDecision,
) {
    let ClientMessage::WorldInput {
        sequence,
        context,
        input,
        ..
    } = message
    else {
        return;
    };
    let kind = match input {
        ClientWorldInput::Motion(_) => PlayerActionKind::Move,
        ClientWorldInput::Chat(_) => PlayerActionKind::Chat,
        ClientWorldInput::SetName(_) => PlayerActionKind::Name,
        ClientWorldInput::Fire { .. } => PlayerActionKind::Fire,
        ClientWorldInput::InteractEntity { .. } => PlayerActionKind::Portal,
        ClientWorldInput::MapTeleport { .. } => PlayerActionKind::Goto,
        ClientWorldInput::SetAltitude { .. } => PlayerActionKind::Altitude,
        ClientWorldInput::Excavation(minimium_protocol::ExcavationInput::GrantTools) => {
            PlayerActionKind::GrantTools
        }
        _ => return,
    };
    let receipt = PlayerActionReceipt {
        sequence: *sequence,
        context: *context,
        kind,
        decision,
    };
    if !queue_message(
        &mut runtime.connections.clients[index],
        &mut runtime.metrics,
        ServerMessage::PlayerActionReceipt(receipt),
    ) {
        runtime.metrics.dropped_messages = runtime.metrics.dropped_messages.saturating_add(1);
    }
}

impl ServerRuntime {
    pub(in crate::runtime) fn send_player_action_receipt(
        &mut self,
        player: minimium_core::ids::PlayerId,
        sequence: u64,
        context: minimium_protocol::WorldContext,
        kind: PlayerActionKind,
        decision: PlayerActionDecision,
    ) {
        let Some(index) = self
            .connections
            .client_indices_by_player
            .get(&player)
            .copied()
        else {
            return;
        };
        let receipt = PlayerActionReceipt {
            sequence,
            context,
            kind,
            decision,
        };
        if !queue_message(
            &mut self.connections.clients[index],
            &mut self.metrics,
            ServerMessage::PlayerActionReceipt(receipt),
        ) {
            self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
        }
    }
}

pub(super) fn decision(
    outcome: crate::runtime::action_state::ActorActionAdmission,
) -> PlayerActionDecision {
    use crate::runtime::action_state::ActorActionAdmission as A;
    match outcome {
        A::Queued => PlayerActionDecision::Accepted,
        A::RateLimited => PlayerActionDecision::Rejected(4),
        A::Busy => PlayerActionDecision::Rejected(2),
        A::ActorUnavailable => PlayerActionDecision::Rejected(5),
        A::InvalidIntent => PlayerActionDecision::Rejected(3),
    }
}
