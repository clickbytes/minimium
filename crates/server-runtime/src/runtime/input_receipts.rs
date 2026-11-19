use super::*;
use crate::world_inputs::WorldInput;
use minimium_protocol::{PlayerActionDecision, PlayerActionKind};

impl ServerRuntime {
    pub(super) fn resolve_grant_receipts(&mut self) {
        let pending = std::mem::take(&mut self.game.pending_grant_receipts);
        for (player, (context, sequence, target)) in pending {
            let decision = if !self
                .game
                .worlds
                .world_input_context_is_current(player, context)
            {
                Some(PlayerActionDecision::Rejected(6))
            } else if self.excavation.storage_failed() {
                Some(PlayerActionDecision::Rejected(5))
            } else if let Some(profile) = self.player_profile(player) {
                let confirmed = self.equipment.confirmed(profile);
                (confirmed.slots == target.slots && confirmed.equipped == target.equipped)
                    .then_some(PlayerActionDecision::Committed)
            } else {
                Some(PlayerActionDecision::Rejected(5))
            };
            if let Some(decision) = decision {
                self.send_player_action_receipt(
                    player,
                    sequence,
                    context,
                    PlayerActionKind::GrantTools,
                    decision,
                );
            } else {
                self.game
                    .pending_grant_receipts
                    .insert(player, (context, sequence, target));
            }
        }
    }

    pub(super) fn validate_player_input(&mut self, input: &WorldInput) -> bool {
        use minimium_core::identity::ServerPermission as Permission;
        let (player, context, additional) = match input {
            WorldInput::Move(id, ctx, _, s) => (
                *id,
                *ctx,
                (s.intent.vertical != 0.0).then_some(Permission::DeveloperFly),
            ),
            WorldInput::Fire(id, ctx, _, _)
            | WorldInput::Chat(id, ctx, _, _)
            | WorldInput::Rename(id, ctx, _, _) => (*id, *ctx, None),
            WorldInput::Excavation(id, ctx, minimium_protocol::ExcavationInput::GrantTools) => {
                (*id, *ctx, Some(Permission::DeveloperTeleport))
            }
            WorldInput::GrantTools(id, ctx, _) => (*id, *ctx, Some(Permission::DeveloperTeleport)),
            _ => return true,
        };
        if !self
            .game
            .worlds
            .world_input_context_is_current(player, context)
        {
            self.metrics.rejected_inputs = self.metrics.rejected_inputs.saturating_add(1);
            return false;
        }
        let Some(index) = self
            .connections
            .client_indices_by_player
            .get(&player)
            .copied()
        else {
            return false;
        };
        let allowed = self.connections.clients[index].identity().is_some_and(|i| {
            let permissions = i.principal.permissions();
            permissions.contains(Permission::Gameplay)
                && additional.is_none_or(|p| permissions.contains(p))
        });
        if !allowed {
            self.metrics.rejected_inputs = self.metrics.rejected_inputs.saturating_add(1);
        }
        allowed
    }
}
