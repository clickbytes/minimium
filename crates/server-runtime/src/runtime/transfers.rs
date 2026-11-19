//! Pending-transfer cleanup never changes active world membership.
use super::ServerRuntime;
use crate::world_registry::CancelledTransfer;
use std::time::Instant;

impl ServerRuntime {
    pub(super) fn expire_pending_transfers(&mut self, now: Instant) {
        for cancelled in self.game.worlds.expire_transfers(now) {
            crate::diagnostics::emit(
                &self.world_logger,
                minimium_diagnostics::Level::Info,
                "world.transfer_expired",
                &[
                    ("player_id", cancelled.player.to_string()),
                    ("transition_id", format!("{:?}", cancelled.transition_id)),
                ],
            );
            self.finish_transfer_cancellation(cancelled);
        }
    }

    pub(super) fn finish_transfer_cancellation(&mut self, cancelled: CancelledTransfer) {
        if self
            .game
            .pending_portal_transitions
            .get(&cancelled.player)
            .copied()
            == Some(cancelled.transition_id)
        {
            self.game
                .pending_portal_transitions
                .remove(&cancelled.player);
            if let Some((source_context, sequence)) =
                self.game.pending_portal_receipts.remove(&cancelled.player)
            {
                self.send_player_action_receipt(
                    cancelled.player,
                    sequence,
                    source_context,
                    minimium_protocol::PlayerActionKind::Portal,
                    minimium_protocol::PlayerActionDecision::Rejected(8),
                );
            }
        }
        if let Some(tickets) = self.game.transfer_sector_tickets.remove(&cancelled.player) {
            for ticket in tickets {
                self.game.sectors.unpin(ticket);
                if !self.game.sectors.is_pinned(ticket) {
                    // Shared interests retain their pins. Installed data remains cacheable.
                    let _ = self.game.sectors.cancel(ticket);
                }
            }
        }
        self.sector_workers.discard_cancelled(&self.game.sectors);
        let Some(index) = self
            .connections
            .client_indices_by_player
            .get(&cancelled.player)
            .copied()
        else {
            return;
        };
        if !crate::connection::queue_message(
            &mut self.connections.clients[index],
            &mut self.metrics,
            minimium_protocol::ServerMessage::TransitionCancelled {
                transition_id: cancelled.transition_id,
                context: cancelled.source_context,
            },
        ) {
            self.metrics.dropped_messages = self.metrics.dropped_messages.saturating_add(1);
            crate::connection::mark_disconnected(
                index,
                &mut self.connections.clients[index],
                &mut self.connections.disconnected,
                Some(crate::connection::CloseReason::Backpressure),
            );
        }
    }
}
