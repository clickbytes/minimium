use super::*;
use crate::world_inputs::WorldInput;
use minimium_protocol::{PlayerActionDecision, PlayerActionKind};
const INPUT_IDLE_TIMEOUT: Duration = Duration::from_millis(250);

pub(super) type ScheduledMotion = std::collections::BTreeMap<
    u64,
    (
        minimium_protocol::WorldContext,
        u64,
        minimium_protocol::MovementSample,
    ),
>;

impl ServerRuntime {
    pub(super) fn resolve_movement_input(
        &mut self,
        player: minimium_core::ids::PlayerId,
        input_seq: u64,
    ) {
        if let Some(pending) = self.game.pending_input_seqs.get_mut(&player) {
            pending.remove(&input_seq);
            if pending.is_empty() {
                self.game.pending_input_seqs.remove(&player);
            }
        }
        let first_pending = self
            .game
            .pending_input_seqs
            .get(&player)
            .and_then(|set| set.first().copied());
        let resolved = first_pending.map_or_else(
            || self.game.last_input_seq.get(&player).copied().unwrap_or(0),
            |first| first.saturating_sub(1),
        );
        self.game
            .resolved_input_seq
            .entry(player)
            .and_modify(|seq| *seq = (*seq).max(resolved))
            .or_insert(resolved);
    }

    pub(super) fn apply_inputs(&mut self) {
        // Reuse the allocation and deliver each event before the next input.
        let mut pending = std::mem::take(&mut self.game.pending_world_inputs);
        let mut teleported = std::collections::HashSet::new();
        for (input, decoded_at) in pending.drain(..) {
            if let Some(decoded_at) = decoded_at {
                self.metrics
                    .input_wait
                    .record(self.now.saturating_duration_since(decoded_at));
            }
            if let WorldInput::Move(player, context, sequence, sample) = &input {
                if teleported.contains(player) {
                    self.resolve_movement_input(*player, sample.input_seq);
                    self.send_player_action_receipt(
                        *player,
                        *sequence,
                        *context,
                        PlayerActionKind::Move,
                        PlayerActionDecision::Rejected(8),
                    );
                    continue;
                }
            }
            if let Some(event) = self.apply_input(input, &mut teleported) {
                self.emit_world_event(event);
            }
        }
        self.game.pending_world_inputs = pending;
        self.apply_scheduled_movement_inputs();
        let now = self.now;
        for (id, last_received) in &self.game.input_last_received {
            if now.duration_since(*last_received) > INPUT_IDLE_TIMEOUT {
                if let Some(input) = self.game.active_inputs.get_mut(id) {
                    input.direction = [0.0; 2];
                    input.vertical = 0.0;
                }
            }
        }
    }

    fn apply_scheduled_movement_inputs(&mut self) {
        let tick = self.step_boundary().tick();
        let players: Vec<_> = self.game.pending_movement_inputs.keys().copied().collect();
        for player in players {
            let (current, due_receipts, empty) = {
                let Some(samples) = self.game.pending_movement_inputs.get_mut(&player) else {
                    continue;
                };
                let due: Vec<_> = samples
                    .range(..=tick)
                    .map(|(target, sample)| (*target, *sample))
                    .collect();
                let current = due
                    .iter()
                    .filter(|(target, _)| *target == tick)
                    .map(|(_, sample)| *sample)
                    .max_by_key(|(_, _, sample)| sample.input_seq);
                let receipts = due
                    .iter()
                    .map(|(_, (context, sequence, sample))| (*context, *sequence, sample.input_seq))
                    .collect::<Vec<_>>();
                for (target, _) in due {
                    samples.remove(&target);
                }
                (current, receipts, samples.is_empty())
            };
            if empty {
                self.game.pending_movement_inputs.remove(&player);
            }
            let committed = current.filter(|(context, _, _)| {
                self.game
                    .worlds
                    .world_input_context_is_current(player, *context)
            });
            if let Some((context, sequence, sample)) = committed {
                self.game
                    .applied_input
                    .insert(player, (sample.input_seq, tick));
                self.game.active_inputs.insert(player, sample.intent);
                self.send_player_action_receipt(
                    player,
                    sequence,
                    context,
                    PlayerActionKind::Move,
                    PlayerActionDecision::Committed,
                );
            }
            for (context, sequence, input_seq) in due_receipts {
                self.resolve_movement_input(player, input_seq);
                if committed.is_none_or(|(_, committed_sequence, _)| committed_sequence != sequence)
                {
                    self.send_player_action_receipt(
                        player,
                        sequence,
                        context,
                        PlayerActionKind::Move,
                        PlayerActionDecision::Rejected(
                            if self
                                .game
                                .worlds
                                .world_input_context_is_current(player, context)
                            {
                                8
                            } else {
                                6
                            },
                        ),
                    );
                }
            }
        }
    }
}
