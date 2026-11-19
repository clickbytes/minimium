use super::ServerRuntime;
use crate::connection::{mark_disconnected, queue_message, CloseReason};
use minimium_core::ids::PlayerId;
use minimium_protocol::{
    ClientMessage, ClientWorldInput, JumpOutcome, MotionRejection, MovementOutcome,
    MovementReceipt, ServerMessage, WorldContext,
};
use minimium_simulation::JumpResolution;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) struct Ticket {
    pub context: WorldContext,
    pub request_id: u64,
    pub input_seq: u64,
    pub scheduled: bool,
}

impl ServerRuntime {
    pub(super) fn reject_jump_message(
        &mut self,
        index: usize,
        message: &ClientMessage,
        reason: MotionRejection,
    ) {
        if let ClientMessage::WorldInput {
            sequence,
            context,
            input: ClientWorldInput::Jump { input_seq },
            ..
        } = message
        {
            self.send_jump(
                index,
                Ticket {
                    context: *context,
                    request_id: *sequence,
                    input_seq: *input_seq,
                    scheduled: false,
                },
                JumpOutcome::Rejected(reason),
            );
        }
    }
    pub(super) fn send_jump(&mut self, index: usize, ticket: Ticket, outcome: JumpOutcome) {
        let receipt = MovementReceipt {
            request_id: ticket.request_id,
            input_seq: ticket.input_seq,
            context: ticket.context,
            tick: if outcome == JumpOutcome::Accepted {
                self.environment_clock.boundary().tick().saturating_add(1)
            } else {
                self.step_boundary().tick()
            },
            outcome: MovementOutcome::Jump(outcome),
        };
        let client = &mut self.connections.clients[index];
        if !queue_message(
            client,
            &mut self.metrics,
            ServerMessage::MovementReceipt(receipt),
        ) {
            mark_disconnected(
                index,
                client,
                &mut self.connections.disconnected,
                Some(CloseReason::Backpressure),
            );
        }
    }
    pub(super) fn abort_jumps(&mut self, player: PlayerId, reason: MotionRejection) {
        if let Some(tickets) = self.game.jumps.remove(&player) {
            if let Some(index) = self
                .connections
                .client_indices_by_player
                .get(&player)
                .copied()
            {
                for ticket in tickets {
                    self.send_jump(index, ticket, JumpOutcome::Rejected(reason));
                }
            }
        }
    }
    pub(super) fn complete_jumps(&mut self, decisions: &HashMap<PlayerId, JumpResolution>) {
        let players: Vec<_> = self.game.jumps.keys().copied().collect();
        for player in players {
            let Some(index) = self
                .connections
                .client_indices_by_player
                .get(&player)
                .copied()
            else {
                self.game.jumps.remove(&player);
                continue;
            };
            let Some(tickets) = self.game.jumps.remove(&player) else {
                continue;
            };
            let context = self.game.worlds.context_for_player(player);
            let mut first = None;
            for ticket in tickets {
                if Some(ticket.context) != context {
                    self.send_jump(
                        index,
                        ticket,
                        JumpOutcome::Rejected(MotionRejection::StaleContext),
                    );
                } else if !ticket.scheduled {
                    self.game.jumps.entry(player).or_default().push(ticket);
                } else {
                    let outcome = match decisions.get(&player).copied() {
                        None => JumpOutcome::Rejected(MotionRejection::GeometryUnavailable),
                        Some(resolution) => match first {
                            None => {
                                first = Some(ticket.request_id);
                                JumpOutcome::Resolved(resolution)
                            }
                            Some(applied_request_id) => JumpOutcome::Coalesced {
                                applied_request_id,
                                resolution,
                            },
                        },
                    };
                    self.send_jump(index, ticket, outcome);
                }
            }
        }
    }
}
