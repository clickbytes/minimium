use super::*;
use crate::world_inputs::{WorldEvent, WorldInput};

const INPUT_IDLE_TIMEOUT: Duration = Duration::from_millis(250);

impl ServerRuntime {
    pub(super) fn apply_inputs(&mut self) {
        // Reuse the allocation and deliver each event before the next input.
        let mut pending = std::mem::take(&mut self.game.pending_world_inputs);
        for (input, decoded_at) in pending.drain(..) {
            if let Some(decoded_at) = decoded_at {
                self.metrics
                    .input_wait
                    .record(Instant::now().saturating_duration_since(decoded_at));
            }
            if let Some(event) = self.apply_input(input) {
                self.emit_world_event(event);
            }
        }
        self.game.pending_world_inputs = pending;
        let now = Instant::now();
        for (id, last_received) in &self.game.input_last_received {
            if now.duration_since(*last_received) > INPUT_IDLE_TIMEOUT {
                if let Some(input) = self.game.active_inputs.get_mut(id) {
                    input.direction = [0.0; 2];
                }
            }
        }
    }

    fn apply_input(&mut self, input: WorldInput) -> Option<WorldEvent> {
        match input {
            WorldInput::Join(id, name, position, health) => {
                self.game
                    .world
                    .join_at_with_health(id, name, position, health);
                self.game
                    .active_inputs
                    .insert(id, CharacterInput::default());
                self.game.input_last_received.insert(id, Instant::now());
            }
            WorldInput::Leave(id) => {
                if let Some(player) = self.game.world.player(id) {
                    if let Err(error) = self.operations.player_store.update_from_player(player) {
                        eprintln!("could not capture player {} before leave: {error}", id);
                    }
                }
                self.game.world.leave(id);
                self.game.active_inputs.remove(&id);
                self.game.input_last_received.remove(&id);
                if let Some(profile) = self.operations.player_store.profile_by_player.get(&id) {
                    self.game.pending_departures.remove(profile);
                }
            }
            WorldInput::Move(id, input) => {
                let current = self.game.active_inputs.entry(id).or_default();
                current.direction = input.direction;
                current.jump |= input.jump;
                self.game.input_last_received.insert(id, Instant::now());
            }
            WorldInput::Fire(id, direction) => {
                self.metrics.shots_fired = self.metrics.shots_fired.saturating_add(1);
                let outcome = self.game.world.fire_hitscan(id, direction)?;
                let target_health = outcome
                    .target_id
                    .and_then(|target| self.game.world.player_health(target));
                if outcome.target_id.is_some() {
                    self.metrics.shots_hit = self.metrics.shots_hit.saturating_add(1);
                    if outcome.killed {
                        self.metrics.player_kills = self.metrics.player_kills.saturating_add(1);
                    }
                }
                return Some(WorldEvent::Shot {
                    shooter: id,
                    outcome,
                    target_health,
                });
            }
            WorldInput::Chat(id, text) => {
                if !self.operations.player_store.active_players.contains(&id) {
                    return None;
                }
                let name = self.operations.player_store.name_for_player(id)?.to_owned();
                return Some(WorldEvent::Chat {
                    sender: id,
                    name,
                    text,
                });
            }
            WorldInput::Rename(id, name) => {
                self.game.world.rename_player(id, name.clone());
                self.operations.player_store.rename(id, name);
            }
        }
        None
    }
}
